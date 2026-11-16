"""Pure logic for the BL-0167 realism loop (unit-tested in test_core.py).

Nothing here touches git, Godot or the network except GitGuard/apply helpers,
which take explicit paths so tests can use a temp repo.
"""
from __future__ import annotations

import dataclasses
import hashlib
import json
import pathlib
import re
import subprocess

PASS, FAIL, XFAIL, ERROR, MISSING, UNAVAILABLE = "pass", "fail", "xfail", "error", "missing", "unavailable"
OK_STATES = {PASS, XFAIL}
PROTECTED = {"main", "master", "step3-attire", "step4-office-bed", "step2b-pathfollow", "client-v1"}


# ---------------------------------------------------------------- results
def passing(results: dict) -> set:
    return {k for k, v in results.items() if v.get("status") in OK_STATES}


def failing(results: dict) -> set:
    return {k for k, v in results.items() if v.get("status") in (FAIL, ERROR)}


def regressions(before: dict, after: dict, ignore=()) -> list:
    """Checks that passed (or xfailed) before and do not now. Missing = regression;
    a strict xfail that starts passing (XPASS) is a regression too."""
    out = []
    for k in sorted(passing(before) - set(ignore)):
        a = after.get(k)
        if a is None or a.get("status") not in OK_STATES:
            out.append(k)
        elif before[k].get("status") == XFAIL and a.get("status") != XFAIL:
            out.append(k)  # strict xfail: an unexpected pass is a regression
    return out


def fail_before_gate(before: dict, after: dict, target_checks: list) -> tuple[bool, str]:
    """A fix counts only if at least one target check FAILED before and PASSES after,
    and no target check that failed before is now erroring."""
    fixed = [c for c in target_checks if before.get(c, {}).get("status") == FAIL and after.get(c, {}).get("status") == PASS]
    if not fixed:
        notrun = [c for c in target_checks if c not in after]
        if notrun and len(notrun) == len(target_checks):
            # Run 20261116-1525 iter-01: a parse error hung the suite; 'missing' is "did not run", not "still failing".
            return False, f"target checks did not run (missing after: {notrun}); evaluation aborted or timed out"
        tb = {c: before.get(c, {}).get("status", MISSING) for c in target_checks}
        ta = {c: after.get(c, {}).get("status", MISSING) for c in target_checks}
        return False, f"no target check went fail->pass (before {tb}, after {ta})"
    return True, "fail->pass: " + ", ".join(fixed)


def new_check_gate(on_mutant: dict, on_current: dict, check_id: str) -> tuple[bool, str]:
    """A generated check is accepted only if it FAILS on the known-bad mutant
    (fail-before) and runs (pass or fail, not error) on the current code."""
    m = on_mutant.get(check_id, {}).get("status")
    c = on_current.get(check_id, {}).get("status")
    if m != FAIL:
        return False, f"new check {check_id} did not fail on the known-bad mutant (got {m})"
    if c not in (PASS, FAIL):
        return False, f"new check {check_id} does not run cleanly on current code (got {c})"
    return True, f"{check_id}: fails on mutant, {c} on current"


def score(results: dict) -> int:
    return len(failing(results))


# ---------------------------------------------------------------- coverage
def coverage(spec: list, results: dict, known_checks: set) -> dict:
    """Per spec attribute: has_check (check id named and known), and status."""
    rows = []
    for item in spec:
        cid = str(item.get("check") or MISSING)
        has = cid != MISSING and cid.upper() != "MISSING" and cid in known_checks
        st = results.get(cid, {}).get("status", UNAVAILABLE) if has else MISSING
        rows.append({"id": item.get("id"), "check": cid if has else MISSING, "status": st})
    n = len(rows)
    with_check = sum(1 for r in rows if r["check"] != MISSING)
    p = sum(1 for r in rows if r["status"] in OK_STATES)
    f = sum(1 for r in rows if r["status"] in (FAIL, ERROR))
    return {
        "attributes": n, "with_check": with_check, "without_check": n - with_check,
        "passing": p, "failing": f, "unverified": with_check - p - f,
        "coverage_pct": round(100.0 * with_check / n, 1) if n else 0.0,
        "rows": rows,
    }


# ---------------------------------------------------------------- stop rules
@dataclasses.dataclass
class StopCfg:
    n_seeds: int = 3
    k_no_progress: int = 3
    max_iterations: int = 12
    require_coverage: float = 100.0
    clock_times: tuple = ("0530", "0800", "1230", "1830", "2200", "0230")


def decide_stop(history: list, cfg: StopCfg) -> tuple[bool, str]:
    """history: list of iteration records with keys
    failing (int), coverage_pct (float), seeds_all_pass (int), times_all_pass (list),
    progress (bool), error_sig (str or "").  Returns (stop, reason)."""
    if not history:
        return False, ""
    last = history[-1]
    if (last.get("failing", 1) == 0 and last.get("coverage_pct", 0.0) >= cfg.require_coverage
            and last.get("seeds_all_pass", 0) >= cfg.n_seeds
            and set(cfg.clock_times) <= set(last.get("times_all_pass", []))):
        return True, f"done: all checks pass on {cfg.n_seeds} seeds across {len(cfg.clock_times)} clock times, coverage {last['coverage_pct']}%"
    if len(history) >= 2 and last.get("error_sig") and last.get("error_sig") == history[-2].get("error_sig"):
        return True, f"G1/G10: same harness error twice: {last['error_sig']}"
    if len(history) >= cfg.k_no_progress and not any(h.get("progress") for h in history[-cfg.k_no_progress:]):
        return True, f"no progress for {cfg.k_no_progress} iterations"
    if len(history) >= cfg.max_iterations:
        return True, f"max iterations ({cfg.max_iterations})"
    return False, ""


def pick_target(results: dict, targets: list, cov: dict, all_targets: list | None = None) -> dict | None:
    """Configured targets in order: a fix target whose check fails, or a
    generate-check target whose attribute still has no check.  Then any other
    spec attribute without a check (missing-check target, fail-before gated).
    A target with after: [ids] waits until every check of those targets passes
    (sub-jobs 1a -> 1b -> 1c build on each other)."""
    missing = {str(r["id"]) for r in cov.get("rows", []) if r["check"] == MISSING}
    by_id = {x["id"]: x for x in (all_targets or targets)}
    for t in targets:
        if any(results.get(c, {}).get("status") not in OK_STATES
               for pre in t.get("after", []) for c in by_id.get(pre, {}).get("checks", [])):
            continue
        if t.get("kind", "fix") == "generate_check":
            if str(t.get("attribute")) in missing:
                return dict(t, kind="generate_check", checks=[])
            continue
        if any(results.get(c, {}).get("status") in (FAIL, ERROR) for c in t.get("checks", [])):
            return dict(t, kind="fix")
    for r in cov.get("rows", []):
        if r["check"] == MISSING:
            return {"id": "gen-check:" + str(r["id"]), "kind": "generate_check", "attribute": r["id"], "checks": []}
    return None


# ---------------------------------------------------------------- patches
BLOCK = re.compile(r"^FILE: (?P<file>\S+)\n<<<<<<< SEARCH\n(?P<search>.*?)\n=======\n(?P<replace>.*?)\n>>>>>>> REPLACE", re.S | re.M)
NEWFILE = re.compile(r"^NEWFILE: (?P<file>\S+)\n```[a-z]*\n(?P<body>.*?)\n```", re.S | re.M)


def parse_edits(text: str) -> list:
    edits = [{"file": m["file"], "search": m["search"], "replace": m["replace"]} for m in BLOCK.finditer(text)]
    edits += [{"file": m["file"], "new": m["body"] + "\n"} for m in NEWFILE.finditer(text)]
    return edits


# Tolerant text form: optional **/backticks around the path, optional ```lang fence lines
# between FILE: and SEARCH and after REPLACE.
BLOCK_LOOSE = re.compile(
    r"^[ \t]*(?:\*\*)?FILE:(?:\*\*)?[ \t]*`?(?P<file>[^\s`*]+)`?[ \t]*\n(?:[ \t]*```[\w+-]*[ \t]*\n)?"
    r"<<<<<<< SEARCH\n(?P<search>.*?)\n?=======\n(?P<replace>.*?)\n?>>>>>>> REPLACE", re.S | re.M)
THINK = re.compile(r"<think>.*?(?:</think>|\Z)", re.S)

# JSON schema for fix proposals (vLLM response_format json_schema, strict).
def edit_schema(files: list, plan_chars: int = 2000, max_edits: int = 12) -> dict:
    fprop = {"type": "string", "enum": list(files)} if files else {"type": "string"}
    return {"type": "object", "additionalProperties": False, "required": ["plan", "edits"],
            "properties": {"plan": {"type": "string", "maxLength": plan_chars},
                           "edits": {"type": "array", "minItems": 1, "maxItems": max_edits,
                                     "items": {"type": "object", "additionalProperties": False,
                                               "required": ["file", "func", "search", "replace"],
                                               "properties": {"file": fprop, "func": {"type": "string"},
                                                              "search": {"type": "string", "minLength": 1},
                                                              "replace": {"type": "string"}}}}}}


def parse_proposal(text: str, finish: str | None = None, usage: dict | None = None) -> tuple[list, str]:
    """Return (edits, reason).  edits == [] means rejected and reason says why, in words a human can act on.
    Truncation / empty output is reported as such, never as a format problem (ROOT-CAUSE-unparseable-patch.md)."""
    text = text or ""
    u = usage or {}
    rt = (u.get("completion_tokens_details") or {}).get("reasoning_tokens")
    tok = f"completion_tokens={u.get('completion_tokens')}, reasoning_tokens={rt}"
    if finish == "length":
        return [], f"truncated: finish_reason=length ({tok}, content {len(text)} chars); raise max_tokens or cut thinking"
    body = THINK.sub("", text.replace("\r\n", "\n")).strip()
    if not body:
        return [], f"empty content (finish_reason={finish}, {tok}); model produced no answer text"
    if body.startswith("```"):
        m = re.match(r"```[\w-]*\n(.*?)\n?```\s*\Z", body, re.S)
        if m and m.group(1).lstrip().startswith("{"):
            body = m.group(1).strip()
    if body.startswith("{"):
        try:
            d = json.loads(body)
        except json.JSONDecodeError as ex:
            return [], f"invalid JSON at char {ex.pos}: {ex.msg}"
        es = d.get("edits") if isinstance(d, dict) else None
        if not isinstance(es, list) or not es:
            return [], "JSON has no non-empty 'edits' list"
        out = []
        for n, e in enumerate(es):
            if not isinstance(e, dict) or not isinstance(e.get("file"), str) or not isinstance(e.get("search"), str) \
                    or not isinstance(e.get("replace"), str) or not e["search"]:
                return [], f"edit {n} needs string file/search/replace with non-empty search"
            out.append({"file": e["file"], "search": e["search"], "replace": e["replace"]}
                       | ({"func": e["func"]} if isinstance(e.get("func"), str) and e["func"] else {}))
        return out, "ok (json)"
    edits = [{"file": m["file"], "search": m["search"], "replace": m["replace"]} for m in BLOCK_LOOSE.finditer(body)]
    edits += [{"file": m["file"], "new": m["body"] + "\n"} for m in NEWFILE.finditer(body)]
    if edits:
        return edits, "ok (search/replace text)"
    if "<<<<<<< SEARCH" in body:
        return [], "SEARCH marker present but block malformed (needs FILE: line, =======, >>>>>>> REPLACE)"
    return [], f"no edit blocks and not JSON ({len(body)} chars of prose)"


def edit_size_problem(edits: list, max_replace: int, max_total: int) -> str:
    """'' if every replace is small enough, else why.  Run 20261116-1525: iter-01 put a 6219-char
    whole-function rewrite in one replace (it broke the parse); iter-02 tried more and hit the token cap."""
    sizes = [len(e.get("replace", e.get("new", ""))) for e in edits]
    big = [(n, s) for n, s in enumerate(sizes) if s > max_replace]
    total = sum(sizes)
    if big or total > max_total:
        return (f"edits too large: replace chars per edit {sizes} (cap {max_replace}), total {total} (cap {max_total}); "
                "send minimal search/replace edits that change only the lines that must change")
    return ""


def import_errors(log: str) -> list:
    """Error lines from a Godot import/check-only log (SCRIPT ERROR / Parse Error / ERROR), each with its 'at:' line."""
    lines = re.sub(r"\x1b\[[0-9;]*m", "", log or "").splitlines()
    out = []
    for i, l in enumerate(lines):
        if re.match(r"^\s*(SCRIPT ERROR|ERROR|Parse Error)\b", l) or "Parse Error" in l:
            at = lines[i + 1].strip() if i + 1 < len(lines) and lines[i + 1].strip().startswith("at:") else ""
            out.append((l.strip() + (" " + at if at else ""))[:300])
    return out


_FUNC = re.compile(r"^(?:static\s+)?func\s+([A-Za-z_]\w*)\s*\(", re.M)


def gd_excerpt(src: str, funcs: list, header: bool = True) -> str:
    """Exact text of the top-level header (before the first func) and of the named functions, in file
    order, with '# ... N lines omitted ...' markers.  Search blocks copied from it match the real file."""
    lines = src.splitlines(keepends=True)
    starts = [(src.count("\n", 0, m.start()), m.group(1)) for m in _FUNC.finditer(src)]
    if not starts:
        return src
    want = {f for f in funcs if not f.startswith("re:")}
    pats = [re.compile(f[3:]) for f in funcs if f.startswith("re:")]  # e.g. re:poly|arc: helpers a previous job added
    keep = [(0, starts[0][0])] if header else []
    for k, (ln, name) in enumerate(starts):
        if name in want or any(pt.search(name) for pt in pats):
            end = starts[k + 1][0] if k + 1 < len(starts) else len(lines)
            keep.append((ln, end))
    out, pos = [], 0
    for a, b in keep:
        if a > pos:
            out.append(f"# ... {a - pos} lines omitted ...\n")
        out.append("".join(lines[a:b]))
        pos = b
    if pos < len(lines):
        out.append(f"# ... {len(lines) - pos} lines omitted ...\n")
    return "".join(out)


def clean_code(s: str) -> str:
    """Undo the JSON-side quote mangling seen in the G5 probe after 6c03bcf (&quot; and curly quotes)."""
    import html
    if "&quot;" in s or "&amp;" in s or "&lt;" in s or "&gt;" in s:
        s = html.unescape(s)
    return s.replace("\u201c", '"').replace("\u201d", '"').replace("\u2018", "'").replace("\u2019", "'")


def func_span(src: str, name: str) -> tuple | None:
    """(start, end) char span of top-level GDScript function `name` (up to the next top-level func)."""
    ms = list(_FUNC.finditer(src))
    for k, m in enumerate(ms):
        if m.group(1) == name:
            return m.start(), (ms[k + 1].start() if k + 1 < len(ms) else len(src))
    return None


def locate(cur: str, search: str, func: str = "") -> tuple:
    if func:
        span, why = _locate(cur, search, func)
        if span is None and ("0 times" in why or "not found" in why):
            span2, why2 = _locate(cur, search, "")  # wrong func name: fall back to the whole file
            return (span2, why2 + " (func ignored)") if span2 else (None, why)
        return span, why
    return _locate(cur, search, "")


def _locate(cur: str, search: str, func: str = "") -> tuple:
    """Find the unique span for `search`, returning ((start, end), how) or (None, why).
    Order: exact; quote-normalised; then quote-insensitive (the probe's searches dropped every double quote).
    `func` (optional) scopes the search to that function so a line repeated in another function is unique."""
    lo, hi = 0, len(cur)
    if func:
        sp = func_span(cur, func)
        if sp is None:
            return None, f"func {func!r} not found"
        lo, hi = sp
    region = cur[lo:hi]
    for how, s in (("exact", search), ("quote-normalised", clean_code(search))):
        n = region.count(s)
        if n == 1:
            i = region.index(s)
            return (lo + i, lo + i + len(s)), how
        if n > 1:
            return None, f"SEARCH matched {n} times" + (f" in func {func}" if func else "") + " (give the function name in 'func' or a longer search)"
    # Quote-insensitive: compare with every double quote removed, then map back to the real span.
    s2 = clean_code(search).replace('"', "")
    if not s2.strip():
        return None, "SEARCH matched 0 times"
    idx = [i for i, ch in enumerate(region) if ch != '"']
    flat = "".join(region[i] for i in idx)
    n = flat.count(s2)
    if n == 1:
        j = flat.index(s2)
        a, b = idx[j], idx[j + len(s2) - 1] + 1
        # widen over a quote that closes the match (e.g. search ended at a string literal)
        while b < len(region) and region[b] == '"' and (b + 1 >= len(region) or region[b + 1] in "]),:; \n\t"):
            b += 1
        return (lo + a, lo + b), "quote-insensitive"
    return None, f"SEARCH matched {n} times" + (f" in func {func}" if func else "") + (" (even ignoring quotes)" if n == 0 else "")


# ---------------------------------------------------------------- whole-function edits (run 20261116-1653)
# ROOT-CAUSE-job1a-scope.md: search/replace splices into nested functions landed at the wrong indent level
# (continue outside its loop) or re-applied (4x var start_poly).  job1a now only replaces whole functions.
_TOP_DECL = re.compile(r"^(?:static\s+func|func|var|const|class|class_name|signal|enum|extends|@\w+)\b", re.M)


def func_decl_span(src: str, name: str) -> tuple | None:
    """(start, end) of top-level function `name`: from its `func name(` line to the next top-level
    func/var/const/class/... line or EOF, minus trailing blank lines and column-0 comments (they belong to
    whatever follows).  None if there is no such function."""
    m = re.search(r"^(?:static\s+)?func\s+" + re.escape(name) + r"\s*\(", src, re.M)
    if not m:
        return None
    nxt = None
    for d in _TOP_DECL.finditer(src, m.end()):
        nxt = d.start()
        break
    end = len(src) if nxt is None else nxt
    lines = src[m.start():end].splitlines(keepends=True)
    while len(lines) > 1 and (not lines[-1].strip() or lines[-1].startswith("#")):
        lines.pop()
    return m.start(), m.start() + len("".join(lines))


def func_at_line(src: str, line: int) -> str | None:
    """Name of the top-level function whose span holds 1-based `line`, else None."""
    for m in _FUNC.finditer(src):
        sp = func_decl_span(src, m.group(1))
        if sp and src.count("\n", 0, sp[0]) + 1 <= line <= src.count("\n", 0, max(sp[0], sp[1] - 1)) + 1:
            return m.group(1)
    return None


def parse_error_locs(err: str) -> list:
    """[(rel_path, line)] from Godot error text such as '(res://scripts/x.gd:414)'."""
    return [(p, int(n)) for p, n in re.findall(r"res://([\w./-]+\.gd):(\d+)", err or "")]


def func_schema(files: list, plan_chars: int = 2000, max_funcs: int = 6) -> dict:
    fprop = {"type": "string", "enum": list(files)} if files else {"type": "string"}
    return {"type": "object", "additionalProperties": False, "required": ["plan", "functions"],
            "properties": {"plan": {"type": "string", "maxLength": plan_chars},
                           "functions": {"type": "array", "minItems": 1, "maxItems": max_funcs,
                                         "items": {"type": "object", "additionalProperties": False,
                                                   "required": ["file", "target", "code"],
                                                   "properties": {"file": fprop,
                                                                  "target": {"type": "string", "minLength": 1},
                                                                  "code": {"type": "string", "minLength": 10}}}}}}


def parse_func_proposal(text: str, finish: str | None = None, usage: dict | None = None) -> tuple[list, str]:
    """Whole-function proposals: ([{file, target, code}], reason).  [] means rejected."""
    text = text or ""
    u = usage or {}
    tok = f"completion_tokens={u.get('completion_tokens')}, reasoning_tokens={(u.get('completion_tokens_details') or {}).get('reasoning_tokens')}"
    if finish == "length":
        return [], f"truncated: finish_reason=length ({tok}, content {len(text)} chars); raise max_tokens or cut thinking"
    body = THINK.sub("", text.replace("\r\n", "\n")).strip()
    if not body:
        return [], f"empty content (finish_reason={finish}, {tok}); model produced no answer text"
    m = re.match(r"```[\w-]*\n(.*?)\n?```\s*\Z", body, re.S)
    if m:
        body = m.group(1).strip()
    try:
        d = json.loads(body)
    except json.JSONDecodeError as ex:
        return [], f"invalid JSON at char {ex.pos}: {ex.msg}"
    fs = d.get("functions") if isinstance(d, dict) else None
    if not isinstance(fs, list) or not fs:
        return [], "JSON has no non-empty 'functions' list (whole-function mode: {file, target, code})"
    out = []
    for n, f in enumerate(fs):
        if not isinstance(f, dict) or not all(isinstance(f.get(k), str) and f.get(k) for k in ("file", "target", "code")):
            return [], f"function {n} needs non-empty string file/target/code"
        out.append({"file": f["file"], "target": f["target"], "code": f["code"]})
    return out, "ok (json functions)"


# Run 20261116-2022 (ROOT-CAUSE-job1a-arc-follower.md): a 4 KB function inside a strict-JSON string looped
# ' -> void:' 2640x with no \n escape.  answer_format: code_blocks = raw fenced GDScript, one block per function.
_BLOCK_HDR = re.compile(r"^[ \t]*#{2,4}[ \t]*TARGET:[ \t]*`?([A-Za-z_]\w*)`?(?:[ \t]+FILE:[ \t]*`?([\w./-]+)`?)?[ \t]*$", re.M)


def parse_func_blocks(text: str, finish: str | None = None, usage: dict | None = None, default_file: str | None = None) -> tuple[list, str]:
    """'### TARGET: name [FILE: path]' + a ``` fenced block per function -> ([{file, target, code}], reason)."""
    text = text or ""
    u = usage or {}
    tok = f"completion_tokens={u.get('completion_tokens')}, reasoning_tokens={(u.get('completion_tokens_details') or {}).get('reasoning_tokens')}"
    if finish == "degenerate_repetition":
        return [], f"degenerate repetition: stream aborted ({u.get('abort', 'repeat detector')}; content {len(text)} chars); raw output saved"
    if finish == "length":
        return [], f"truncated: finish_reason=length ({tok}, content {len(text)} chars); raise max_tokens or cut thinking"
    body = THINK.sub("", text.replace("\r\n", "\n"))
    if not body.strip():
        return [], f"empty content (finish_reason={finish}, {tok}); model produced no answer text"
    hdrs = list(_BLOCK_HDR.finditer(body))
    if not hdrs:
        return [], "no '### TARGET: <function>' header followed by a fenced code block"
    out = []
    for k, h in enumerate(hdrs):
        seg = body[h.end(): hdrs[k + 1].start() if k + 1 < len(hdrs) else len(body)]
        m = re.match(r"\s*```[\w+-]*[ \t]*\n(.*?)^[ \t]*```[ \t]*$", seg, re.S | re.M)
        if not m:
            if re.match(r"\s*```", seg):
                return [], f"unclosed code block for target {h.group(1)!r}"
            return [], f"target {h.group(1)!r} is not followed by a fenced code block"
        f = h.group(2) or default_file
        if not f:
            return [], f"target {h.group(1)!r} needs 'FILE: <path>' (more than one editable file)"
        out.append({"file": f, "target": h.group(1), "code": m.group(1)})
    return out, "ok (code blocks)"


class RepeatDetector:
    """Streaming guard: abort when the same non-blank line repeats more than `max_repeats` times in a row, or the
    output tail is more than `max_repeats` back-to-back copies of one unit of >= 4 chars (min period >= 4)."""

    def __init__(self, max_repeats: int = 20, min_unit: int = 4, max_unit: int = 400, check_every: int = 64):
        self.n, self.min_unit, self.max_unit, self.every = max_repeats, min_unit, max_unit, check_every
        self.buf, self.partial, self.last, self.count, self.since = "", "", None, 0, 0

    def feed(self, chunk: str) -> str:
        if not chunk:
            return ""
        self.buf = (self.buf + chunk)[-(self.max_unit * (self.n + 1) + 64):]
        self.partial += chunk
        *lines, self.partial = self.partial.split("\n")
        for l in lines:
            l = l.strip()
            if not l:
                continue
            self.count = self.count + 1 if l == self.last else 1
            self.last = l
            if self.count > self.n:
                return f"line repeated {self.count}x: {l[:60]!r}"
        self.since += len(chunk)
        if self.since >= self.every:
            self.since = 0
            return self._periodic()
        return ""

    def _periodic(self) -> str:
        t, k = self.buf, self.n + 1
        for p in range(self.min_unit, self.max_unit + 1):
            L = p * k
            if L > len(t):
                break
            seg = t[-L:]
            if seg[:-p] != seg[p:]:
                continue
            unit = seg[-p:]
            if not unit.strip() or any(p % q == 0 and unit == unit[:q] * (p // q) for q in range(1, self.min_unit)):
                continue
            return f"segment {unit[:40]!r} ({p} chars) repeated more than {self.n}x"
        return ""


def whole_function_problem(code: str, target: str) -> str:
    """'' if `code` is exactly one complete top-level function named `target`, else why (a splice)."""
    lines = code.strip("\n").split("\n")
    m = re.match(r"^(?:static\s+)?func\s+([A-Za-z_]\w*)\s*\(", lines[0]) if lines else None
    if not m:
        return f"code for {target!r} is not a whole function: it must start at column 0 with 'func {target}(' (no fragments or splices)"
    if m.group(1) != target:
        return f"signature name {m.group(1)!r} does not match target {target!r}"
    body = [l for l in lines[1:] if l.strip()]
    if not body:
        return f"function {target!r} has no body"
    for l in body:
        if not l[0].isspace() and not l.startswith("#"):
            return (f"code for {target!r} is not one whole function: column-0 line {l.strip()[:60]!r} "
                    "(a second declaration or a statement outside the function body)")
    return ""


def _stage_functions(root: pathlib.Path, funcs: list, allowed_prefixes: tuple, allow_new: bool) -> tuple[dict, str]:
    staged, seen = {}, set()
    for n, f in enumerate(funcs):
        rel, target, code = f["file"], f["target"], clean_code(f["code"])
        if rel.startswith("/") or ".." in rel or not rel.startswith(allowed_prefixes):
            return {}, f"function {n}: outside allowed paths: {rel}"
        if (rel, target) in seen:
            return {}, f"function {n}: target {target!r} given more than once in {rel}"
        seen.add((rel, target))
        why = whole_function_problem(code, target)
        if why:
            return {}, f"function {n}: {why}"
        p = pathlib.Path(root) / rel
        cur = staged.get(rel, p.read_text() if p.exists() else None)
        if cur is None:
            return {}, f"function {n}: no such file: {rel}"
        new = code.strip("\n") + "\n"
        sp = func_decl_span(cur, target)
        if sp is None:
            if not allow_new:
                return {}, f"function {n}: target {target!r} not found in {rel} (new functions are not allowed for this job)"
            staged[rel] = cur.rstrip("\n") + "\n\n\n" + new
            continue
        staged[rel] = cur[:sp[0]] + new + cur[sp[1]:]
    return staged, ""


def check_functions(root: pathlib.Path, funcs: list, allowed_prefixes: tuple, allow_new: bool = False) -> str:
    """Dry run of apply_functions: '' if every whole-function replacement is valid, else why."""
    return _stage_functions(root, funcs, allowed_prefixes, allow_new)[1]


def apply_functions(root: pathlib.Path, funcs: list, allowed_prefixes: tuple, allow_new: bool = False) -> list:
    """Replace each target function's whole span (see func_decl_span) with the new code.  Atomic:
    raises ValueError and writes nothing if any item is a splice, mismatched, duplicated or missing."""
    staged, why = _stage_functions(root, funcs, allowed_prefixes, allow_new)
    if why:
        raise ValueError(why)
    for rel, body in staged.items():
        (pathlib.Path(root) / rel).write_text(body)
    return sorted(staged)


def check_edits(root: pathlib.Path, edits: list, allowed_prefixes: tuple) -> str:
    """Dry-run apply_edits on in-memory copies; '' if every edit would apply, else the reason."""
    staged = {}
    for n, e in enumerate(edits):
        rel = e["file"]
        if rel.startswith("/") or ".." in rel or not rel.startswith(allowed_prefixes):
            return f"edit {n}: outside allowed paths: {rel}"
        p = pathlib.Path(root) / rel
        if "new" in e:
            if p.exists() and rel not in staged:
                return f"edit {n}: NEWFILE exists: {rel}"
            staged[rel] = e["new"]
            continue
        cur = staged.get(rel, p.read_text() if p.exists() else None)
        if cur is None:
            return f"edit {n}: no such file: {rel}"
        span, why = locate(cur, e["search"], e.get("func", ""))
        if span is None:
            return f"edit {n}: {why} in {rel}"
        staged[rel] = cur[:span[0]] + clean_code(e["replace"]) + cur[span[1]:]
    return ""


def apply_edits(root: pathlib.Path, edits: list, allowed_prefixes: tuple) -> list:
    """Apply SEARCH/REPLACE edits; every SEARCH must match exactly once.
    Raises ValueError (nothing written) on any mismatch or disallowed path."""
    staged = {}
    for e in edits:
        rel = e["file"]
        if rel.startswith("/") or ".." in rel or not rel.startswith(allowed_prefixes):
            raise ValueError(f"edit outside allowed paths: {rel}")
        p = root / rel
        if "new" in e:
            if p.exists() and rel not in staged:
                raise ValueError(f"NEWFILE exists: {rel}")
            staged[rel] = e["new"]
            continue
        cur = staged.get(rel, p.read_text() if p.exists() else None)
        if cur is None:
            raise ValueError(f"no such file: {rel}")
        span, why = locate(cur, e["search"], e.get("func", ""))
        if span is None:
            raise ValueError(f"{why} in {rel}")
        staged[rel] = cur[:span[0]] + clean_code(e["replace"]) + cur[span[1]:]
    for rel, body in staged.items():
        (root / rel).parent.mkdir(parents=True, exist_ok=True)
        (root / rel).write_text(body)
    return sorted(staged)


# ---------------------------------------------------------------- git safety
class GitGuard:
    def __init__(self, repo: pathlib.Path):
        self.repo = pathlib.Path(repo)

    def git(self, *args, check=True) -> str:
        r = subprocess.run(["git", "-C", str(self.repo), *args], capture_output=True, text=True)
        if check and r.returncode != 0:
            raise RuntimeError(f"git {' '.join(args)}: {r.stderr.strip()[:400]}")
        return r.stdout.strip()

    def branch(self) -> str:
        return self.git("rev-parse", "--abbrev-ref", "HEAD")

    def assert_loop_branch(self) -> str:
        b = self.branch()
        if b in PROTECTED or not b.startswith("loop/"):
            raise RuntimeError(f"refusing to write on branch {b!r}: loop writes only on loop/* branches")
        return b

    def head(self) -> str:
        return self.git("rev-parse", "HEAD")

    def revert_to(self, sha: str) -> None:
        self.assert_loop_branch()
        self.git("reset", "--hard", sha)
        self.git("clean", "-fdq", "--", "scripts", "test", "tools", "config", "assets")

    def commit(self, paths: list, message: str, author: str) -> str:
        self.assert_loop_branch()
        self.git("add", "--", *paths)
        name, email = re.match(r"(.*) <(.*)>", author).groups()
        self.git("-c", f"user.name={name}", "-c", f"user.email={email}", "commit", "-q", "-m", message)
        return self.head()


# ---------------------------------------------------------------- G15
def verify_import_md5(project: pathlib.Path) -> tuple[int, list]:
    """For every *.png.import, source md5 must equal the imported .md5's source_md5."""
    project = pathlib.Path(project)
    bad, n = [], 0
    for imp in project.rglob("*.png.import"):
        if ".godot" in imp.parts or "site" in imp.parts:
            continue
        txt = imp.read_text(errors="ignore")
        m = re.search(r'path="res://\.godot/imported/([^"]+)\.ctex"', txt)
        src = imp.with_suffix("")
        if not m or not src.exists():
            continue
        md5f = project / ".godot" / "imported" / (m.group(1) + ".md5")
        n += 1
        if not md5f.exists():
            bad.append(f"{src.relative_to(project)}: no import md5")
            continue
        want = re.search(r'source_md5="([0-9a-f]+)"', md5f.read_text())
        got = hashlib.md5(src.read_bytes()).hexdigest()
        if not want or want.group(1) != got:
            bad.append(f"{src.relative_to(project)}: source_md5 mismatch")
    return n, bad


def parse_suite(log: str) -> dict:
    """Map lessville-tests case lines to check ids suite.<slug>."""
    out = {}
    for m in re.finditer(r"\[lessville-tests\] (PASS|FAIL): ([^\n—]+?)(?: — (.*))?$", log, re.M):
        name = m.group(2).strip()
        if re.match(r"^\d+ (of|checks)", name):
            continue
        out[slug(name)] = {"status": PASS if m.group(1) == "PASS" else FAIL, "detail": (m.group(3) or "").strip()[:400], "name": name}
    return out


SUITE_IDS = {
    "facing matches heading": "suite.facing_matches_heading",
    "pedestrians stay on sidewalk/crossing": "suite.pedestrians_stay_on_nav",
    "bus follows route, stops and dwells": "suite.bus_route_and_dwell",
    "crossing overlap (dense seeded sim)": "suite.crossing_overlap",
    "walkers progress (no stall, max wait 10 s)": "suite.walkers_progress",
    "atmosphere follows the clock": "suite.atmosphere",
    "attire follows role and place": "suite.attire",
    "town seats are drawn at step-1 anchors": "suite.terrace_visible",
    "lin profile hair covers the skull like the front frames": "suite.lin_profile_hair",
    "terrace fg and window-glass do not cover a sitter's head": "suite.terrace_sitter_layers",
    "office has no baked bed": "suite.office_bed",
    "office bed patch matches neighbouring floor": "suite.office_bed_patch",
    "office bed a3 artefacts, stamps, and desk silhouette": "suite.office_bed_a3",
    "office floor known limitations (strict xfail)": "suite.office_known_limitations",
    "walkers follow arc-length path (job 1a)": "suite.arc_follow",
    "same-direction followers keep a gap (job 1b)": "suite.follow_gap",
}


def slug(name: str) -> str:
    k = name.strip().lower()
    return SUITE_IDS.get(k, "suite." + re.sub(r"[^a-z0-9]+", "_", k).strip("_"))
