#!/usr/bin/env python3
"""Stage a Godot Web export and content-addressed R2 binaries; no network writes."""
import hashlib
import json
from pathlib import Path
import shutil


def prepare(root: Path) -> dict:
    dist = root / "dist"
    public = root / "deploy/cloudflare/public"
    html = (dist / "index.html").read_text()
    if html.count("</head>") != 1 or "const config = {" not in html:
        raise ValueError("Expected the exported Godot loading shell")
    routes = {}
    binaries = []
    for suffix in ("wasm", "pck"):
        name = f"index.{suffix}"
        data = (dist / name).read_bytes()
        if not data or f'"{name}":{len(data)}' not in html:
            raise ValueError(f"Godot export has missing or mismatched {name}")
        digest = hashlib.sha256(data).hexdigest()
        key = f"lessville/game/sha256/{digest}.{suffix}"
        routes[name] = f"/manus-storage/game/sha256/{digest}.{suffix}"
        binaries.append({"file": f"dist/{name}", "key": key, "sha256": digest, "bytes": len(data)})
    # Keep the deployed loader convention: only the two engine requests are remapped.
    shim = '<script>(function(){var R=' + json.dumps(routes, separators=(",", ":")) + ';var f=window.fetch.bind(window);window.fetch=function(i,o){var u=typeof i==="string"?i:(i&&i.url)||"";var b=u.split("?")[0].split("/").pop();return R[b]?f(R[b],o):f(i,o)};})();</script>'
    public.mkdir(parents=True, exist_ok=True)
    for file in dist.iterdir():
        if file.is_file() and file.suffix not in (".wasm", ".pck", ".html"):
            if file.stat().st_size > 25 * 1024 * 1024:
                raise ValueError(f"Static asset exceeds Cloudflare limit: {file.name}")
            shutil.copy2(file, public / file.name)
    (public / "index.html").write_text(html.replace("</head>", shim + "\n</head>"))
    result = {"binaries": binaries, "html_sha256": hashlib.sha256((public / "index.html").read_bytes()).hexdigest()}
    (root / ".local").mkdir(exist_ok=True)
    (root / ".local/cloudflare-web.json").write_text(json.dumps(result, indent=2) + "\n")
    assert all(url in (public / "index.html").read_text() for url in routes.values())
    return result


if __name__ == "__main__":
    print(json.dumps(prepare(Path(__file__).resolve().parent.parent), indent=2))
