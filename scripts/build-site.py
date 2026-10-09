#!/usr/bin/env python3
"""Build the static LessDB website from repo docs + website/content.

Inputs:
  docs/*.md            -> website/docs/*.html
  README.md            -> website/docs/getting-started.html
  website/content/playbooks/*.md   -> website/playbooks/*.html
  website/content/use-cases/*.md   -> website/use-cases/*.html

Runs with the stdlib only; output is plain static HTML served by
Cloudflare Pages (no build step on the edge).
"""
import html
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent
WEBSITE = ROOT / "website"
GITHUB = "https://lessdb.dev//blob/main"

NAV = [
    ("Home", "/", False),
    ("Docs", "/docs/getting-started", True),
    ("Playbooks", "/playbooks/", True),
    ("Use cases", "/use-cases/", True),
    ("Downloads", "/downloads/", True),
]


def esc(s):
    return html.escape(s, quote=False)


def inline(text):
    """Inline markdown: `code`, **bold**, *em*, [text](url), <br>."""
    text = esc(text)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    text = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", text)
    text = re.sub(r"(?<!\*)\*([^*\n]+)\*(?!\*)", r"<em>\1</em>", text)
    text = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", link_repl, text)
    return text


def link_repl(m):
    label, url = m.group(1), m.group(2)
    if url.startswith("http") or url.startswith("#"):
        return f'<a href="{url}">{label}</a>'
    if url.endswith(".md"):
        url = url[:-3]
    elif url.endswith(".html"):
        url = url[:-5]
    elif url.startswith("../crates/") or url.startswith("crates/"):
        p = url.removeprefix("../")
        return f'<a href="{GITHUB}/{p}">{label}</a>'
    elif url.startswith("bench/results/"):
        return f'<a href="{GITHUB}/{url}">{label}</a>'
    return f'<a href="{url}">{label}</a>'


def convert(md_text, rel_dir):
    """Minimal but faithful markdown -> HTML for this doc set."""
    out = []
    lines = md_text.split("\n")
    i = 0
    in_code = None
    list_stack = []  # ("ul"|"ol",)
    para = []

    def flush_para():
        nonlocal para
        if para:
            out.append("<p>" + inline(" ".join(para)) + "</p>")
            para = []

    def close_lists(depth):
        while len(list_stack) > depth:
            out.append("</" + list_stack.pop()[0] + ">")

    while i < len(lines):
        line = lines[i]
        # fenced code
        if line.strip().startswith("```"):
            lang = line.strip()[3:].strip()
            buf = []
            i += 1
            while i < len(lines) and not lines[i].strip().startswith("```"):
                buf.append(lines[i])
                i += 1
            i += 1
            flush_para()
            close_lists(0)
            cls = f' class="language-{lang}"' if lang else ""
            out.append(f"<pre{cls}><code>" + esc("\n".join(buf)) + "</code></pre>")
            continue
        if line.strip() == "":
            flush_para()
            close_lists(0)
            i += 1
            continue
        # headings
        m = re.match(r"^(#{1,4})\s+(.*)$", line)
        if m:
            flush_para()
            close_lists(0)
            lvl = len(m.group(1))
            out.append(f"<h{lvl}>{inline(m.group(2))}</h{lvl}>")
            i += 1
            continue
        # horizontal rule
        if re.match(r"^(\s*[-*_]\s*){3,}$", line):
            flush_para()
            close_lists(0)
            out.append("<hr/>")
            i += 1
            continue
        # table
        if line.strip().startswith("|") and i + 1 < len(lines) and re.match(
            r"^\s*\|?[\s:|-]+\|?\s*$", lines[i + 1]
        ):
            flush_para()
            close_lists(0)
            headers = [c.strip() for c in line.strip().strip("|").split("|")]
            i += 2
            rows = []
            while i < len(lines) and lines[i].strip().startswith("|"):
                rows.append([c.strip() for c in lines[i].strip().strip("|").split("|")])
                i += 1
            out.append("<table><thead><tr>" + "".join(f"<th>{inline(h)}</th>" for h in headers) + "</tr></thead><tbody>")
            for r in rows:
                out.append("<tr>" + "".join(f"<td>{inline(c)}</td>" for c in r) + "</tr>")
            out.append("</tbody></table>")
            continue
        # blockquote
        if line.startswith(">"):
            flush_para()
            close_lists(0)
            buf = []
            while i < len(lines) and lines[i].startswith(">"):
                buf.append(lines[i].lstrip("> ").strip())
                i += 1
            out.append("<blockquote>" + inline(" ".join(buf)) + "</blockquote>")
            continue
        # lists
        m = re.match(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$", line)
        if m:
            flush_para()
            indent = len(m.group(1))
            kind = "ol" if m.group(2)[0].isdigit() else "ul"
            depth = indent // 2
            while len(list_stack) > depth:
                out.append("</" + list_stack.pop()[0] + ">")
            if not list_stack or list_stack[-1][0] != kind:
                list_stack.append((kind, depth))
                out.append(f"<{kind}>")
            out.append("<li>" + inline(m.group(3)) + "</li>")
            i += 1
            continue
        # normal paragraph line
        para.append(line.strip())
        i += 1

    flush_para()
    close_lists(0)
    return "\n".join(out)


def rel(path_from_page, target):
    """Relative extensionless href from the output page to target."""
    a = pathlib.Path(path_from_page)
    b = pathlib.Path(target)
    r = pathlib.os.path.relpath(b, a.parent)
    if r.endswith(".html"):
        r = r[:-5]
    if r.endswith("/index"):
        r = r[:-5] or "."
    return r


def page(source_md, out_html, title, group, groups, body_html=None):
    if body_html is None:
        body_html = convert(source_md.read_text(), out_html.parent)

    # sidebar: all groups with their pages
    links = []
    for g_name, g_pages in groups:
        links.append(f'<div class="side-group">{esc(g_name)}</div>')
        for label, href_target, active in g_pages:
            cls = " class='active'" if active else ""
            links.append(f"<a href='{rel(out_html, href_target)}'{cls}>{esc(label)}</a>")
    sidebar = "\n".join(links)

    nav_html = ""
    for label, href, _ in NAV:
        if href == "/":
            nav_html += f"<a href='{rel(out_html, 'website/index.html')}'>{label}</a>"
        else:
            nav_html += f"<a href='{href}'>{label}</a>"

    return f"""<!DOCTYPE html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{esc(title)} · LessDB</title>
<meta name="description" content="{esc(title)} — LessDB, one database for agents and humans."/>
<link rel="icon" type="image/svg+xml" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='7' fill='%230a0a07'/%3E%3Crect x='6' y='4' width='20' height='14' rx='4' fill='%232e2e26'/%3E%3Crect x='9' y='7' width='3' height='4' fill='%23f6f09c'/%3E%3Crect x='20' y='7' width='3' height='4' fill='%23f6f09c'/%3E%3Crect x='10' y='20' width='12' height='7' rx='3' fill='%23ffd54a'/%3E%3Crect x='14' y='23' width='4' height='2' fill='%23fff8b0'/%3E%3C/svg%3E"/>
<link rel="preconnect" href="https://fonts.googleapis.com"/>
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin/>
<link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet"/>
<link rel="stylesheet" href="{rel(out_html, 'website/styles.css')}"/>
<link rel="stylesheet" href="{rel(out_html, 'website/docs.css')}"/>
</head>
<body>
<header class="site-header">
  <div class="header-frame"><div class="header-inner">
    <a href="{rel(out_html, 'website/index.html')}" class="brand"><svg class="brand-mark" width="28" height="28" viewBox="0 0 21 24" fill="none" role="img" aria-label="Lumo mascot mark" id="mascotBrand"></svg>&nbsp;LessDB</a>
    <nav class="main-nav" aria-label="Primary">{nav_html}<a href="https://lessdb.dev/" class="github-link">GitHub <span data-github-stars aria-label="Loading star count"></span></a></nav>
  </div></div>
</header>
<div class="docs-layout">
  <aside class="docs-sidebar">{sidebar}</aside>
  <main class="docs-content">
    <h1>{esc(title)}</h1>
    {body_html}
  </main>
</div>
<footer class="docs-footer">
  <div class="footer-inner">
    <span>LessDB — one database for agents and humans. <a href='{rel(out_html, 'website/docs/license.html')}' style='color:inherit'>MIT License</a>.<br/>⚗️ Experimental — built with coding agents: a DBMS for AI, with AI.</span>
    <a href="https://lessdb.dev/" class="gh-link">GitHub ↗</a>
  </div>
</footer>
<script src="{rel(out_html, 'website/app.js')}"></script>
<script src="{rel(out_html, 'website/github-stars.js')}"></script>
</body>
</html>
"""


def first_h1(md_text):
    for line in md_text.split("\n"):
        m = re.match(r"^#\s+(.*)$", line.strip())
        if m:
            return m.group(1)
    return "Documentation"


def main():
    WEBSITE.mkdir(exist_ok=True)
    (WEBSITE / "docs").mkdir(exist_ok=True)
    (WEBSITE / "playbooks").mkdir(exist_ok=True)
    (WEBSITE / "use-cases").mkdir(exist_ok=True)

    # ---- the LessDB coding-agent skill (served at /skills/lessdb/SKILL.md)
    import shutil as _shutil
    skill_src = ROOT / "skills" / "lessdb" / "SKILL.md"
    if skill_src.exists():
        (WEBSITE / "skills" / "lessdb").mkdir(parents=True, exist_ok=True)
        _shutil.copyfile(skill_src, WEBSITE / "skills" / "lessdb" / "SKILL.md")
        print("skills:", "website/skills/lessdb/SKILL.md")

    # ---- docs pages (site-authored; keeps the public docs free of
    # internal jargon and third-party comparisons)
    for stale in (WEBSITE / "docs").glob("*.html"):
        if stale.name != "architecture-interactive.html":
            stale.unlink()
    docs = [(md, WEBSITE / "docs" / f"{md.stem}.html")
            for md in sorted((WEBSITE / "content" / "docs").glob("*.md"))]

    # ---- playbooks + use cases (content lives in website/content/)
    for stale in list((WEBSITE / "playbooks").glob("*.html")) + list((WEBSITE / "use-cases").glob("*.html")):
        stale.unlink()
    playbooks = sorted((WEBSITE / "content" / "playbooks").glob("*.md"))
    use_cases = sorted((WEBSITE / "content" / "use-cases").glob("*.md"))

    groups = []
    docs_group = [("Getting started", WEBSITE / "docs/getting-started.html", False)]
    for md, out in docs:
        if md.stem != "getting-started":
            docs_group.append((md.stem.replace("-", " ").title(), out, False))
    groups.append(("Docs", docs_group))
    groups.append(("Interactive", [("Architecture diagram", WEBSITE / "docs/architecture-interactive.html", False)]))

    # index pages for playbooks / use cases
    def index_page(folder, group_name, files):
        cards = []
        for md in files:
            title = first_h1(md.read_text())
            out = WEBSITE / folder / (md.stem + ".html")
            desc = ""
            for line in md.read_text().split("\n"):
                if line.strip() and not line.startswith("#"):
                    desc = line.strip()
                    break
            cards.append(f"<a class='card' href='{rel(WEBSITE / folder / 'index.html', out)}'><h3>{esc(title)}</h3><p>{esc(desc[:180])}</p></a>")
        return "\n".join(cards)

    # render everything with a two-pass so sidebars list real pages
    for md, out in docs:
        title = first_h1(md.read_text())
        body = None
        if md.stem.lower() == "architecture" and md.parent.name == "docs":
            body = convert(md.read_text(), out.parent) + (
                '<div class="card-grid"><a class="card" href="architecture-interactive">'
                "<h3>Interactive diagram →</h3><p>Clickable map of every component, its data structures, "
                "and six animated flow walkthroughs (INSERT, QUERY, OPTIMIZE, crash recovery, shared storage).</p></a></div>"
            )
        html = page(md, out, title, "docs", groups, body_html=body)
        out.write_text(html)
        print("docs:", out.relative_to(WEBSITE))

    # playbooks / use-cases sidebars
    for folder, group_name, files in [("playbooks", "Playbooks", playbooks), ("use-cases", "Use cases", use_cases)]:
        side = [(group_name, [(first_h1(md.read_text()), WEBSITE / folder / (md.stem + ".html"), False) for md in files])]
        for md in files:
            out = WEBSITE / folder / (md.stem + ".html")
            title = first_h1(md.read_text())
            out.write_text(page(md, out, title, folder, side))
            print(f"{folder}:", out.relative_to(WEBSITE))
        idx = WEBSITE / folder / "index.html"
        cards = index_page(folder, group_name, files)
        idx.write_text(f"""<!DOCTYPE html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>{group_name} · LessDB</title>
<meta name="description" content="{group_name} — LessDB, one database for agents and humans."/>
<link rel="icon" type="image/svg+xml" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='7' fill='%230a0a07'/%3E%3Crect x='6' y='4' width='20' height='14' rx='4' fill='%232e2e26'/%3E%3Crect x='9' y='7' width='3' height='4' fill='%23f6f09c'/%3E%3Crect x='20' y='7' width='3' height='4' fill='%23f6f09c'/%3E%3Crect x='10' y='20' width='12' height='7' rx='3' fill='%23ffd54a'/%3E%3Crect x='14' y='23' width='4' height='2' fill='%23fff8b0'/%3E%3C/svg%3E"/>
<link href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet"/>
<link rel="stylesheet" href="{rel(idx, WEBSITE / 'styles.css')}"/>
<link rel="stylesheet" href="{rel(idx, WEBSITE / 'docs.css')}"/>
</head>
<body>
<header class="site-header"><div class="header-frame"><div class="header-inner">
  <a href="{rel(idx, WEBSITE / 'index.html')}" class="brand"><svg class="brand-mark" width="28" height="28" viewBox="0 0 21 24" fill="none" role="img" aria-label="Lumo mascot mark" id="mascotBrand"></svg>&nbsp;LessDB</a>
  <nav class="main-nav">
    <a href="{rel(idx, WEBSITE / 'index.html')}">Home</a>
    <a href="{rel(idx, WEBSITE / 'docs/getting-started.html')}">Docs</a>
    <a href="{rel(idx, WEBSITE / 'playbooks/index.html')}">Playbooks</a>
    <a href="{rel(idx, WEBSITE / 'use-cases/index.html')}">Use cases</a>
    <a href="{rel(idx, WEBSITE / 'downloads/index.html')}">Downloads</a>
    <a href="https://lessdb.dev/" class="github-link">GitHub <span data-github-stars aria-label="Loading star count"></span></a>
  </nav>
</div></div></header>
<main class="docs-content standalone">
  <h1>{group_name}</h1>
  <div class="card-grid">{cards}</div>
</main>
<script src="{rel(idx, WEBSITE / 'app.js')}"></script>
<script src="{rel(idx, WEBSITE / 'github-stars.js')}"></script>
</body>
</html>
""")
        print(folder, "index:", idx.relative_to(WEBSITE))


if __name__ == "__main__":
    main()
