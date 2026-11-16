#!/usr/bin/env python3
"""Build REALISM-SPEC.md + realism-spec.yaml (BL-0167) from local-Qwen drafts.

Drafts (one per category) were written by qwen3.8-27b-nvfp4 from an
auto-extracted inventory; this script only normalises, validates check ids,
attaches ThinkStrat's citations (REALISM-SPEC-CITATIONS.md) and labels design
choices.  The attribute-to-citation mapping is also drafted by local Qwen
(--map) and cached in citation-map.json.

  python3 build_spec.py --drafts DIR --citations FILE --out DIR [--map]
"""
from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import urllib.request

import yaml

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

CATS = ["characters_body", "characters_behaviour", "space_vehicles", "objects", "environment", "ui"]
DESIGN_CHOICE = {"A13", "A14", "B7", "D6", "E5", "E6", "E7", "E8", "F2", "F3"}

# Items ThinkStrat / CoS asked for explicitly (16 Nov 13:55 SGT).
REQUIRED = [
    {"id": "environment.wind.normal_day_leaf_rustle_not_tree_sway", "category": "environment", "entity": "trees and foliage",
     "attribute": "wind response on a normal day", "rule": "On a normal Singapore day (mean wind about 2 m/s, Beaufort 1-2) leaves rustle gently and trees do NOT sway; visible tree sway appears only during monsoon surges or squalls.",
     "reference_value": "about 2 m/s mean (monthly 1.4-2.8 m/s): leaves rustle; Beaufort 5 (17-21 kn) before small trees sway; surges up to 10 m/s, squall gusts 40-80 km/h",
     "citation_rows": ["C5", "C6", "C4"], "check": "MISSING",
     "vision": "On a calm-day frame pair, only leaf clusters shift by a pixel or two; trunks and whole canopies do not lean or shear."},
    {"id": "environment.sky.keyed_to_sg_sunrise_sunset_twilight", "category": "environment", "entity": "sky and lighting",
     "attribute": "sky keyframes keyed to real clock times", "rule": "Sky colour, ambient light and lamp state follow the sourced 2026 Singapore sunrise, sunset and civil/nautical twilight times: night before civil dawn, twilight colours only inside the civil twilight window, full day after sunrise, dusk colours only inside the civil dusk window.",
     "reference_value": "sunrise 06:46-07:16, sunset 18:50-19:20, civil twilight ~20-23 min each side (civil dawn 06:24-06:55, civil dusk ends 19:11-19:42)",
     "citation_rows": ["B1", "B2", "B4"], "check": "MISSING",
     "vision": "At 05:30 the sky is night-dark with lamps lit; at 07:00-07:30 it reads as early-morning day; at 19:00 dusk colours; at 22:00 and 02:30 night."},
    {"id": "characters_body.cafe_staff.apron", "category": "characters_body", "entity": "maya, bo (café staff)",
     "attribute": "apron at work", "rule": "Café staff wear an apron while working at the café. This is a DESIGN CONVENTION, not a legal rule: Singapore food hygiene law only requires food handlers to wear clean attire.",
     "reference_value": "legal minimum: clean attire (no apron requirement)", "citation_rows": ["F1"], "check": "render.attire",
     "vision": "Maya and Bo at the café show an apron; elsewhere they do not; clothes look clean."},
]


def load_draft(p: pathlib.Path) -> list:
    t = p.read_text().strip()
    t = re.sub(r"^```[a-z]*\n|\n```$", "", t)
    if p.suffix == ".json":
        m = re.search(r"\[.*\]", t, re.S)
        return json.loads(m.group(0)) if m else []
    d = yaml.safe_load(t)
    if isinstance(d, dict) and len(d) == 1:
        d = next(iter(d.values()))
    return d if isinstance(d, list) else []


def parse_citations(path: pathlib.Path) -> dict:
    rows = {}
    for l in path.read_text().splitlines():
        m = re.match(r"^\| ([A-F]\d+) \| (.*?) \| (.*?) \| (.*?) \| (.*?) \| (.*?) \|$", l)
        if m:
            rows[m.group(1)] = {"row": m.group(1), "attribute": m.group(2), "value": m.group(4), "source": m.group(5), "verdict": m.group(6)}
    return rows


def catalog(drafts: pathlib.Path, cfg_path: pathlib.Path) -> set:
    ids = set()
    for l in (drafts / "checks.yaml").read_text().splitlines():
        m = re.match(r"^([a-z0-9_.]+):", l)
        if m:
            ids.add(m.group(1))
    cfg = yaml.safe_load(cfg_path.read_text())
    ids |= {c["id"] for c in cfg["checks"]}
    ids |= {"suite.office_bed_a3", "suite.office_known_limitations", "suite.other_checks", "g15.import_md5"}
    return ids


def _chat(url: str, prompt: str, max_tokens: int) -> str:
    body = {"model": "qwen3.8-27b-nvfp4", "messages": [{"role": "user", "content": prompt}], "max_tokens": max_tokens,
            "temperature": 0, "chat_template_kwargs": {"enable_thinking": False}}
    req = urllib.request.Request(url + "/chat/completions", json.dumps(body).encode(), {"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req, timeout=1800))["choices"][0]["message"]["content"]


def llm_map(items: list, cits: dict, checks_txt: str, urls: list) -> dict:
    """Per attribute: citation rows (list) and whether the named check really asserts it.
    Chunks of 40, at most 2 concurrent per endpoint (Steward limit)."""
    import concurrent.futures as cf
    crows = "\n".join(f"{k} | {v['attribute']} | {v['value'][:110]}" for k, v in cits.items())
    chunks = [items[i:i + 40] for i in range(0, len(items), 40)]
    def one(ci):
        ch = chunks[ci]
        lines = "\n".join(f"{it['id']} | {it['attribute']} | {it['rule'][:150]} | check={it['check']}" for it in ch)
        prompt = ("For each Lessville (a Singapore-like town sim) realism attribute below decide two things.\n"
                  "1) rows: which reference rows give a real-world value or norm for it (walking speed->A1/A4, bus speed->A8, bus dwell or acceleration->A14, "
                  "bus headway->A10, sunrise/sunset/twilight/sky by time->B1/B2/B4, lighting lux->B7, rain->C3, wind/foliage->C5/C6, clouds->C7, temperature->C1, "
                  "footpath width->D1, doors->D2, chair/seat height->D3, desk/table height->D4, person height/scale->D5, queues/personal space->D6, "
                  "café/hawker hours->E5, grocery hours->E6, university/school hours->E7/E3, park/trail hours->E8, office hours/work hours->E1/E2, gym hours->E4, "
                  "staff attire/apron->F1, office/home/gym attire->F2, attire vs weather->F3, running->A13). Empty list if none.\n"
                  "2) check_ok: true only if the named check, as described in the catalog, really asserts this exact rule; false if it is only loosely related or MISSING.\n\n"
                  f"Reference rows:\n{crows}\n\nCheck catalog:\n{checks_txt}\n\nAttributes (id | attribute | rule | check):\n{lines}\n\n"
                  'Output ONLY JSON: {"<id>": {"rows": ["A1"], "check_ok": true}, ...} with every id above.')
        url = urls[ci % len(urls)]
        t = _chat(url, prompt, 4000)
        m = re.search(r"\{.*\}", t, re.S)
        return json.loads(m.group(0)) if m else {}
    out = {}
    with cf.ThreadPoolExecutor(2 * len(urls)) as ex:
        for d in ex.map(one, range(len(chunks))):
            out.update(d)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--drafts", type=pathlib.Path, required=True)
    ap.add_argument("--citations", type=pathlib.Path, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--config", type=pathlib.Path, default=pathlib.Path(__file__).with_name("config.yaml"))
    ap.add_argument("--map", action="store_true", help="draft the citation mapping with local Qwen (cached)")
    ap.add_argument("--llm-url", default="http://127.0.0.1:18000/v1,http://127.0.0.1:18001/v1")
    a = ap.parse_args()
    known = catalog(a.drafts, a.config)
    cits = parse_citations(a.citations)
    items, seen, problems = [], set(), []
    for cat in CATS:
        p = a.drafts / f"{cat}.json"
        p = p if p.exists() else a.drafts / f"{cat}.yaml"
        for it in load_draft(p):
            if not isinstance(it, dict) or not it.get("id") or not it.get("rule"):
                problems.append(f"{cat}: dropped malformed item {str(it)[:80]}")
                continue
            iid = re.sub(r"[^a-z0-9_.]+", "_", str(it["id"]).lower())
            if not iid.startswith(cat + "."):
                iid = f"{cat}.{iid}"
            while iid in seen:
                iid += "_2"
            seen.add(iid)
            chk = str(it.get("check") or "MISSING").strip()
            if chk != "MISSING" and chk not in known:
                problems.append(f"{iid}: unknown check {chk!r} -> MISSING")
                chk = "MISSING"
            items.append({"id": iid, "category": cat, "entity": str(it.get("entity", "")), "attribute": str(it.get("attribute", "")),
                          "rule": str(it["rule"]).strip(), "reference_value": it.get("reference_value"),
                          "check": chk, "vision": str(it.get("vision", "")).strip(), "drafted_by": "qwen3.8-27b-nvfp4 (local)"})
    for r in REQUIRED:
        items = [x for x in items if x["id"] != r["id"]]
        items.append(dict(r, drafted_by="Builder (from ThinkStrat/CoS input 16 Nov 13:55 SGT)"))
    mp = a.out / "citation-map.json"
    mapping = json.loads(mp.read_text()) if mp.exists() else {}
    if a.map and (not mapping or not isinstance(next(iter(mapping.values())), dict)):
        mapping = llm_map([x for x in items if "citation_rows" not in x], cits, (a.drafts / "checks.yaml").read_text(), a.llm_url.split(","))
        mp.write_text(json.dumps(mapping, indent=1))
    for x in items:
        m = mapping.get(x["id"]) if isinstance(mapping.get(x["id"]), dict) else {}
        if "citation_rows" not in x and x["check"] != "MISSING" and m and m.get("check_ok") is False:
            problems.append(f"{x['id']}: drafted check {x['check']} judged not to assert the rule -> MISSING")
            x["check_claimed"] = x["check"]
            x["check"] = "MISSING"
        rows = x.pop("citation_rows", None) or [r for r in (m.get("rows") or []) if r in cits]
        x["citation_rows"] = rows
        srcs = [cits[r] for r in rows if r in cits]
        if any(r in DESIGN_CHOICE for r in rows):
            x["basis"] = "DESIGN CHOICE (no source)"
            x["source"] = "DESIGN CHOICE (no source)"
        elif srcs:
            x["basis"] = "real-world (cited)"
            x["source"] = "; ".join(f"{s['row']}: {s['source']}" for s in srcs)
            if not x.get("reference_value") or x["id"] not in {r["id"] for r in REQUIRED}:
                x["reference_value_cited"] = "; ".join(f"{s['row']}: {s['value']}" for s in srcs)
        else:
            x["basis"] = "unreviewed"
            x["source"] = "TBD"
        x["verdict"] = ""
    n = len(items)
    with_check = sum(1 for x in items if x["check"] != "MISSING")
    doc = {"version": 1, "item": "BL-0167", "generated": "2026-11-16 SGT", "attributes": items,
           "counts": {"attributes": n, "with_check": with_check, "missing_check": n - with_check,
                      "cited": sum(1 for x in items if x["basis"] == "real-world (cited)"),
                      "design_choice": sum(1 for x in items if x["basis"].startswith("DESIGN")),
                      "unreviewed": sum(1 for x in items if x["basis"] == "unreviewed")},
           "validation_notes": problems}
    a.out.mkdir(parents=True, exist_ok=True)
    (a.out / "realism-spec.yaml").write_text(yaml.safe_dump(doc, sort_keys=False, allow_unicode=True, width=200))
    md = ["# Lessville REALISM-SPEC (BL-0167)", "",
          "**Internal only.** Machine-readable twin: `realism-spec.yaml` (the loop reads it). Generated by `tools/realism_loop/build_spec.py`.",
          "",
          "How it was made: rules, reference values, check mapping and vision cues were drafted by local qwen3.8-27b-nvfp4 (megatron/bumble) from an auto-extracted repo inventory "
          "(`spec-draft/inventory.md`), one call per category. Builder normalised ids and rejected any check id that does not exist (listed at the end). "
          "Citations come only from ThinkStrat's `REALISM-SPEC-CITATIONS.md`; the attribute-to-row match was drafted by local Qwen and should be checked. "
          "Model-drafted reference values without a citation are not sources (basis 'unreviewed', source TBD).",
          "",
          f"Counts: {n} attributes; {with_check} with an automated check, {n - with_check} MISSING (the loop turns MISSING into check targets with a fail-before gate); "
          f"{doc['counts']['cited']} cited, {doc['counts']['design_choice']} DESIGN CHOICE (no source), {doc['counts']['unreviewed']} unreviewed.",
          "",
          "Columns: **Verdict** is blank for ThinkStrat to fill (OK / WRONG).", ""]
    for cat in CATS:
        rows = [x for x in items if x["category"] == cat]
        md += [f"## {cat.replace('_', ' ').title()} ({len(rows)})", "",
               "| id | entity | attribute | real-world rule | reference value | citation | check | vision reviewer looks for | verdict |",
               "|---|---|---|---|---|---|---|---|---|"]
        for x in rows:
            ref = x.get("reference_value_cited") or x.get("reference_value") or "—"
            if x.get("reference_value_cited") and x.get("reference_value"):
                ref = f"{x['reference_value']} (cited: {x['reference_value_cited']})"
            cite = x["source"] if x["basis"] != "unreviewed" else "source: TBD"
            cell = lambda s: str(s).replace("|", "/").replace("\n", " ")
            md.append(f"| `{x['id']}` | {cell(x['entity'])} | {cell(x['attribute'])} | {cell(x['rule'])} | {cell(ref)} | {cell(cite)} | {x['check']} | {cell(x['vision'])} | {x['verdict']} |")
        md.append("")
    md += ["## Design choices (no source, per ThinkStrat)", "",
           "Running speed (A13); bus dwell time and acceleration (A14); lux by time of day (B7); queue spacing and personal distance (D6); "
           "café, kopitiam and hawker hours (E5); supermarket hours (E6); university class hours (E7); park hours (E8); office, home and gym attire (F2); attire by weather (F3). "
           "Café staff aprons are a design convention; SG food hygiene only requires clean attire (F1).", "",
           "## Validation notes", ""] + [f"- {p}" for p in problems] + [""]
    (a.out / "REALISM-SPEC.md").write_text("\n".join(md))
    print(json.dumps(doc["counts"]), f"problems={len(problems)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
