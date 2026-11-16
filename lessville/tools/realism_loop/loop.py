#!/usr/bin/env python3
"""BL-0167 Lessville local realism loop: render -> check -> vision review -> local
LLM fix -> re-check, with auto-revert on regression.  Local models only.

  python3 tools/realism_loop/loop.py start  [--config C]   # detached run (setsid), returns at once
  python3 tools/realism_loop/loop.py run    [--config C]   # foreground
  python3 tools/realism_loop/loop.py stop                  # graceful: finishes the current step
  python3 tools/realism_loop/loop.py status                # state.json + last iteration lines
  python3 tools/realism_loop/loop.py summarize RUN_DIR     # re-derive SUMMARY.md / coverage from raw jsonl

See README.md.  Git-safe: writes only on loop/* branches in its own worktree.
"""
from __future__ import annotations

import argparse
import base64
import concurrent.futures as cf
import datetime as dt
import io
import json
import os
import pathlib
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import traceback
import urllib.request

import yaml

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import core  # noqa: E402

HERE = pathlib.Path(__file__).resolve().parent
DEFAULT_CFG = HERE / "config.yaml"
SGT = dt.timezone(dt.timedelta(hours=8))


def now() -> str:
    return dt.datetime.now(SGT).strftime("%Y-%m-%d %H:%M:%S SGT")


def load_cfg(path) -> dict:
    cfg = yaml.safe_load(pathlib.Path(path).read_text())
    cfg["_path"] = str(path)
    return cfg


def sh(cmd, cwd=None, env=None, timeout=1800, log=None) -> tuple[int, str]:
    e = dict(os.environ)
    e.update(env or {})
    try:
        r = subprocess.run(cmd, cwd=cwd, env=e, capture_output=True, text=True, timeout=timeout,
                           shell=isinstance(cmd, str))
        out = (r.stdout or "") + (r.stderr or "")
        code = r.returncode
    except subprocess.TimeoutExpired as ex:
        out = f"TIMEOUT after {timeout}s\n" + str(ex.stdout or "")[-2000:]
        code = 124
    if log:
        pathlib.Path(log).write_text(out)
    return code, out


# ======================================================================= limits
class Limits:
    """Machine Steward's BL-0167 slot terms, enforced in code (config: limits)."""

    def __init__(self, cfg: dict):
        self.cfg = cfg
        lim = cfg["limits"]
        self.sems = {e["name"]: threading.Semaphore(int(e.get("max_concurrent", 2))) for e in cfg["endpoints"]}
        self.mem_pause = float(lim.get("megatron_mem_pause_pct", 85))
        self.mem_stop = float(lim.get("megatron_mem_stop_pct", 95))
        self._mem = (0.0, 0.0)
        self.waits = 0

    def megatron_mem_pct(self) -> float:
        t, v = self._mem
        if time.time() - t < 30:
            return v
        code, out = sh(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", self.cfg["limits"].get("megatron_ssh", "megatron"),
                        "free -b | awk '/Mem:/{print $3/$2*100}'"], timeout=20)
        try:
            v = float(out.strip().splitlines()[-1])
        except Exception:
            v = -1.0
        self._mem = (time.time(), v)
        return v

    @staticmethod
    def waiting(url: str) -> float:
        base = url.split("/v1")[0]
        try:
            txt = urllib.request.urlopen(base + "/metrics", timeout=8).read().decode()
            vals = [float(m) for m in re.findall(r"^vllm:num_requests_waiting\{[^}]*\} ([0-9.]+)$", txt, re.M)]
            return max(vals) if vals else 0.0
        except Exception:
            return -1.0

    def gate(self, ep: dict, log) -> None:
        """Back off while the server has waiting requests or megatron memory > pause."""
        for attempt in range(int(self.cfg["limits"].get("backoff_tries", 40))):
            w = self.waiting(ep["url"]) if ep.get("box") == "megatron" or ep.get("check_waiting", True) else 0.0
            m = self.megatron_mem_pct() if ep.get("box") == "megatron" else 0.0
            if m >= self.mem_stop:
                raise RuntimeError(f"megatron memory {m:.0f}% >= {self.mem_stop}% stop line")
            if w <= 0 and m < self.mem_pause:
                return
            self.waits += 1
            log(f"limits: back off ({ep['name']} waiting={w} megatron_mem={m:.0f}%), try {attempt + 1}")
            time.sleep(int(self.cfg["limits"].get("backoff_s", 30)))
        raise RuntimeError(f"limits: {ep['name']} still busy after back-off")

    @staticmethod
    def bumble_blackout() -> bool:
        t = dt.datetime.now(SGT).time()
        return dt.time(1, 45) <= t < dt.time(3, 0)


def post_json(url: str, body: dict, timeout: int) -> dict:
    req = urllib.request.Request(url + "/chat/completions", json.dumps(body).encode(), {"Content-Type": "application/json"})
    return json.load(urllib.request.urlopen(req, timeout=timeout))


def post_stream(url: str, body: dict, timeout: int):
    """Yield the parsed SSE chunks of a streamed chat completion; closing the generator drops the connection
    (vLLM aborts the request on disconnect)."""
    req = urllib.request.Request(url + "/chat/completions", json.dumps(body).encode(), {"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        for raw in resp:
            line = raw.decode("utf-8", "replace").strip()
            if not line.startswith("data:"):
                continue
            data = line[5:].strip()
            if data == "[DONE]":
                return
            yield json.loads(data)


def _streamed(ep: dict, body: dict, timeout: int) -> dict:
    """Stream one completion with the repetition guard (run 20261116-2022: 782 s spent on ' -> void:' x2640)."""
    det, rdet = core.RepeatDetector(), core.RepeatDetector()
    text, reasoning, finish, usage, n, abort = [], [], None, {}, 0, ""
    gen = post_stream(ep["url"], dict(body, stream=True, stream_options={"include_usage": True}), timeout)
    try:
        for ch in gen:
            n += 1
            if ch.get("usage"):
                usage = ch["usage"]
            for c in ch.get("choices") or []:
                d = c.get("delta") or {}
                if d.get("content"):
                    text.append(d["content"])
                    abort = abort or det.feed(d["content"])
                rc = d.get("reasoning") or d.get("reasoning_content")
                if rc:
                    reasoning.append(rc)
                    abort = abort or rdet.feed(rc)
                finish = c.get("finish_reason") or finish
            if abort:
                break
    finally:
        gen.close()
    if abort:
        finish = "degenerate_repetition"
        usage = dict(usage, abort=abort, chunks=n)
    return {"text": "".join(text), "reasoning": "".join(reasoning), "finish": finish, "usage": usage,
            "raw": {"streamed": True, "chunks": n, "abort": abort, "finish_reason": finish}}


def llm(cfg, limits: Limits, messages, log, max_tokens=None, thinking=None, response_format=None, stream=False) -> dict:
    """Returns text (content only), the reasoning text, finish_reason, usage and the full raw response,
    so callers can save everything and tell truncation apart from bad output."""
    lc = cfg["llm"]
    eps = [e for e in cfg["endpoints"] if e["role"] == "text"]
    last = None
    for ep in eps:
        if ep.get("fallback_only") and Limits.bumble_blackout():
            continue
        body = {"model": ep["model"], "messages": messages, "temperature": lc.get("temperature", 0.2),
                "max_tokens": int(max_tokens or lc["max_tokens"]),
                "chat_template_kwargs": {"enable_thinking": bool(lc.get("thinking", True) if thinking is None else thinking)}}
        if response_format:
            body["response_format"] = response_format
        try:
            limits.gate(ep, log)
            if stream:
                with limits.sems[ep["name"]]:
                    t = time.time()
                    s = _streamed(ep, body, int(lc.get("timeout_s", 2400)))
                if s["finish"] == "degenerate_repetition":
                    log(f"llm: {ep['name']} stream aborted: degenerate repetition ({s['usage'].get('abort')})")
                return dict(s, endpoint=ep["name"], model=ep["model"], secs=round(time.time() - t),
                            request={k: v for k, v in body.items() if k != "messages"} | {"stream": True})
            with limits.sems[ep["name"]]:
                t = time.time()
                r = post_json(ep["url"], body, int(lc.get("timeout_s", 2400)))
            msg = r["choices"][0]["message"]
            return {"text": msg.get("content") or "", "reasoning": msg.get("reasoning") or msg.get("reasoning_content") or "",
                    "endpoint": ep["name"], "model": ep["model"], "secs": round(time.time() - t), "usage": r.get("usage") or {},
                    "finish": r["choices"][0].get("finish_reason"), "raw": r,
                    "request": {k: v for k, v in body.items() if k != "messages"}}
        except Exception as ex:  # try the fallback endpoint
            last = ex
            log(f"llm: {ep['name']} failed: {repr(ex)[:200]}")
    raise RuntimeError(f"llm unavailable: {last!r}"[:300])


def _crop_b64(path: str, max_side: int) -> str:
    from PIL import Image
    im = Image.open(path).convert("RGB")
    if max(im.size) > max_side:
        s = max_side / max(im.size)
        im = im.resize((max(1, int(im.width * s)), max(1, int(im.height * s))), Image.LANCZOS)
    assert max(im.size) <= max_side
    buf = io.BytesIO()
    im.save(buf, "PNG")
    return "data:image/png;base64," + base64.b64encode(buf.getvalue()).decode()


def vision(cfg, limits: Limits, items: list, log) -> dict:
    """items: [{path, ask, id}] zoomed crops only.  Enforces <=4 images/request,
    <=1024 px crops, max_tokens <=512.  Returns check result for vision.crop_review."""
    vc = cfg["vision"]
    eps = [e for e in cfg["endpoints"] if e["role"] == "vision"]
    if not vc.get("enabled", True) or not eps:
        return {"status": core.UNAVAILABLE, "detail": "vision: unavailable (no endpoint configured)", "reviews": []}
    ep = eps[0]
    try:
        urllib.request.urlopen(ep["url"] + "/models", timeout=8).read()
    except Exception as ex:
        return {"status": core.UNAVAILABLE, "detail": f"vision: unavailable ({ep['url']}: {repr(ex)[:120]})", "reviews": []}
    per = min(int(vc.get("images_per_request", 4)), 4)
    max_side = min(int(vc.get("max_side_px", 1024)), 1024)
    max_tok = min(int(vc.get("max_tokens", 400)), 512)
    items = items[: int(vc.get("max_crops_per_eval", 16))]
    reviews, fails = [], 0
    for i in range(0, len(items), per):
        batch = items[i:i + per]
        content = [{"type": "text", "text": vc["prompt"] + "\n" + "\n".join(
            f"Image {j + 1} ({b['id']}): {b['ask']}" for j, b in enumerate(batch)) +
            '\nAnswer ONLY JSON: [{"image":1,"verdict":"PASS"|"FAIL","issue":"..."}]'}]
        for b in batch:
            content.append({"type": "image_url", "image_url": {"url": _crop_b64(b["path"], max_side)}})
        body = {"model": ep["model"], "temperature": 0, "max_tokens": max_tok,
                "messages": [{"role": "user", "content": content}]}
        try:
            limits.gate(ep, log)
            with limits.sems[ep["name"]]:
                r = post_json(ep["url"], body, 300)
            txt = r["choices"][0]["message"].get("content") or ""
            m = re.search(r"\[.*\]", txt, re.S)
            verdicts = json.loads(m.group(0)) if m else []
        except Exception as ex:
            txt, verdicts = f"error: {repr(ex)[:160]}", []
        for j, b in enumerate(batch):
            v = next((x for x in verdicts if int(x.get("image", 0)) == j + 1), None)
            verdict = str(v.get("verdict", "")).upper() if v else "UNPARSED"
            if verdict == "FAIL":
                fails += 1
            reviews.append({"id": b["id"], "path": b["path"], "verdict": verdict, "issue": (v or {}).get("issue", txt[:200])})
    unparsed = sum(1 for r in reviews if r["verdict"] not in ("PASS", "FAIL"))
    st = core.FAIL if fails else (core.ERROR if unparsed == len(reviews) and reviews else core.PASS)
    return {"status": st, "detail": f"vision {ep['model']}: {len(reviews)} crops, {fails} FAIL, {unparsed} unparsed (advisory; G4 review is CoS)", "reviews": reviews}


# ======================================================================= evaluation
class Evaluator:
    def __init__(self, cfg, wt: pathlib.Path, limits: Limits, log):
        self.cfg, self.wt, self.limits, self.log = cfg, wt, limits, log
        self.godot = cfg["godot"]

    def _godot(self, args, env=None, render=False, timeout=1200, log=None):
        cmd = [self.godot] + (["--rendering-driver", "opengl3"] if render else ["--headless"]) + ["--path", str(self.wt)] + args
        if render:
            cmd = ["xvfb-run", "-a", "-s", "-screen 0 1920x1080x24"] + cmd
        cmd = ["nice", "-n", "10"] + cmd
        return sh(cmd, cwd=self.wt, env=env, timeout=timeout, log=log)

    def g15(self, out: pathlib.Path) -> dict:
        imp = self.wt / ".godot" / "imported"
        shutil.rmtree(imp, ignore_errors=True)
        for p in self.wt.rglob("*.ctex"):
            p.unlink()
        code, txt = self._godot(["--import", "--quit"], timeout=900, log=out / "import.log")
        n, bad = core.verify_import_md5(self.wt)
        # Run 20261116-1525 iter-01: "import rc=0 errors=1" was reported as PASS (only ^ERROR lines were
        # counted, SCRIPT ERROR / Parse Error lines were not, and the count never failed the check).
        errs = core.import_errors(txt)
        ok = code == 0 and not bad and n > 0 and not errs
        return {"status": core.PASS if ok else core.FAIL, "errors": errs[:5],
                "detail": (f"import errors: {' | '.join(errs[:3])}; " if errs else "") +
                          f"cache cleared; import rc={code} errors={len(errs)}; source_md5 verified {n - len(bad)}/{n}" + (f"; {bad[:3]}" if bad else "")}

    def check_only(self, files: list) -> str:
        """Cheap pre-gate (about 1 s per script): Godot --check-only parse of every edited .gd plus the
        scripts that preload it.  '' if clean, else '<file>: <first error lines>'."""
        gds = [f for f in files if f.endswith(".gd")]
        if not gds:
            return ""
        deps = [f for f in self.cfg.get("parse_also", []) if (self.wt / f).exists()]
        for f in list(dict.fromkeys(gds + deps)):
            code, txt = sh([self.godot, "--headless", "--path", str(self.wt), "--check-only", "-s", "res://" + f],
                           cwd=self.wt, timeout=120)
            errs = core.import_errors(txt)
            if code != 0 or errs:
                return f"{f}: " + (" | ".join(errs[:3]) if errs else f"check-only rc={code}: {txt.strip()[-300:]}")
        return ""

    def renders(self, out: pathlib.Path, seed: int) -> dict:
        res = {}
        times = ",".join(self.cfg["stop"]["clock_times"])
        jobs = []
        for r in self.cfg["renders"]:
            env = {k: str(v).format(out=out, times=times, seed=seed) for k, v in r.get("env", {}).items()}
            jobs.append((r["id"], r["script"], env))
        def one(j):
            rid, script, env = j
            code, txt = self._godot(["-s", script], env=env, render=True, timeout=1500, log=out / f"render-{rid}.log")
            d = pathlib.Path(env.get(next(iter(env)), str(out)))
            pngs = len(list(pathlib.Path(out / rid).rglob("*.png"))) if (out / rid).exists() else 0
            return rid, {"status": core.PASS if code == 0 and pngs > 0 else core.ERROR, "detail": f"rc={code} pngs={pngs}"}
        with cf.ThreadPoolExecutor(int(self.cfg["limits"].get("box_parallel_godot", 2))) as ex:
            for rid, r in ex.map(one, jobs):
                res[f"render.{rid}.ran"] = r
        return res

    def suite(self, out: pathlib.Path) -> dict:
        code, txt = self._godot(["res://test/test_suite.tscn"], timeout=int(self.cfg.get("suite_timeout_s", 600)), log=out / "suite.log")
        res = core.parse_suite(txt)
        named = {v["name"] for v in res.values()}
        other = [l for l in re.findall(r"^ERROR: \[lessville-tests\] (.*)$", txt, re.M)
                 if not any(l.startswith(n + ":") for n in named)]
        m = re.search(r"\[lessville-tests\] (PASS|FAIL): (\d+)[^\n]*", txt)
        res["suite.other_checks"] = {"status": core.FAIL if other else core.PASS,
                                     "detail": ("; ".join(other)[:400] if other else "assets/routing/realism/seating/main-scene checks ok")}
        res["suite.summary"] = {"status": core.PASS if code == 0 else core.FAIL, "detail": (m.group(0) if m else f"rc={code}, no summary line")[:300]}
        if not m:
            res["suite.summary"]["status"] = core.ERROR
        return res

    def checks(self, out: pathlib.Path) -> dict:
        res = {}
        for c in self.cfg["checks"] + self._generated():
            cmd = c["cmd"].format(out=out, wt=self.wt)
            code, txt = sh(cmd, cwd=self.wt / "tools", timeout=900, log=out / f"check-{c['id']}.log")
            lines = [l for l in txt.strip().splitlines() if l.strip()]
            verdict = [l for l in lines if re.match(r"^\s*(PASS|FAIL|SUMMARY|EXIT|RESULT)\b", l)]
            tail = (verdict or lines)[-1:] or [""]
            res[c["id"]] = {"status": core.PASS if code == 0 else core.FAIL, "detail": tail[0][:300]}
        return res

    def _generated(self) -> list:
        p = self.wt / "tools" / "realism_loop" / "generated_checks.yaml"
        return (yaml.safe_load(p.read_text()) or []) if p.exists() else []

    def sims(self, out: pathlib.Path, seeds: list, seconds: float) -> tuple[dict, int, list]:
        g = self.cfg["sim"]
        def one(seed):
            env = {"SIM_SEED": str(seed), "SIM_SECONDS": str(seconds), "SIM_SETTLE": str(g.get("settle", 240)),
                   "SIM_BG": str(g.get("background", 10)), "SIM_MINUTE": str(g.get("minute", 480))}
            code, txt = self._godot(["-s", "res://tools/sim_crossings.gd"], env=env, timeout=int(g.get("timeout_s", 2400)),
                                    log=out / f"sim-seed{seed}.log")
            m = re.search(r"^\[crossing-sim\] (\{.*\})$", txt, re.M)
            if not m:
                return seed, None, f"seed {seed}: no result (rc={code})"
            d = json.loads(m.group(1))
            pc = d["per_crossing"]
            zo = pc["crossing_west"]["overlap_ticks"] + pc["crossing_east"]["overlap_ticks"]
            fails = []
            if zo: fails.append(f"zebra overlap {zo}")
            if d["facing_bad_ticks"] or d["max_tangent_err_deg"] > 11.48: fails.append(f"facing {d['max_facing_err_deg']:.1f}/{d['max_tangent_err_deg']:.1f} deg")
            if d["off_nav"]: fails.append(f"off-nav {d['off_nav']}")
            if d["max_wait_s"] > 10.0: fails.append(f"max wait {d['max_wait_s']:.1f}s")
            if d["unreached"]: fails.append(f"unreached {len(d['unreached'])}")
            summary = {"seed": seed, "zebra_overlap": zo, "min_margin": min(pc["crossing_west"]["min_margin"], pc["crossing_east"]["min_margin"]),
                       "facing_max": d["max_facing_err_deg"], "tangent_max": d["max_tangent_err_deg"], "off_nav": d["off_nav"],
                       "max_wait": d["max_wait_s"], "unreached": len(d["unreached"]), "west_visits": pc["crossing_west"]["visits"],
                       "boarding_overlap_info": pc["bus_boarding"]["overlap_ticks"], "fails": fails}
            return seed, summary, ""
        rows, errs = [], []
        with cf.ThreadPoolExecutor(int(self.cfg["limits"].get("box_parallel_godot", 2))) as ex:
            for seed, s, e in ex.map(one, seeds):
                (rows.append(s) if s else errs.append(e))
        ok = [r for r in rows if not r["fails"]]
        st = core.ERROR if errs else (core.PASS if len(ok) == len(seeds) else core.FAIL)
        detail = "; ".join(f"seed {r['seed']}: " + (", ".join(r["fails"]) or "pass") + f" (min margin {r['min_margin']:.1f}px, max wait {r['max_wait']:.1f}s)" for r in rows) + ("; " + "; ".join(errs) if errs else "")
        return {"sim.crossings_multihour": {"status": st, "detail": detail[:600], "seeds": rows}}, len(ok), rows

    def times_all_pass(self, results: dict) -> list:
        gates = json.loads((self.wt / "config" / "atmosphere_gates.json").read_text())
        gated = set((gates.get("times") or {}).keys()) if isinstance(gates.get("times"), dict) else set()
        if results.get("render.atmosphere", {}).get("status") != core.PASS:
            return []
        timed_ok = all(results.get(c, {}).get("status") in core.OK_STATES for c in self.cfg.get("time_dependent_checks", []))
        return sorted(t for t in self.cfg["stop"]["clock_times"] if t in gated and timed_ok)

    def gallery(self, out: pathlib.Path, label: str) -> list:
        """4x crops at each zebra for every busy moment, plus a motion GIF; collect other @4x crops."""
        from PIL import Image, ImageDraw
        gdir = out / "gallery"
        gdir.mkdir(parents=True, exist_ok=True)
        items = []
        meta_p = out / "crossings" / "crossings-meta.json"
        boxes = self.cfg["gallery"]["crossing_boxes"]
        if meta_p.exists():
            meta = json.loads(meta_p.read_text())
            for mi, m in enumerate(meta["moments"]):
                strip = []
                for fi, fr in enumerate(m["frames"]):
                    for cname, box in boxes.items():
                        im = Image.open(fr["path"]).convert("RGB").crop(tuple(box))
                        im4 = im.resize((im.width * 4, im.height * 4), Image.NEAREST)
                        if fi == 0:
                            p = gdir / f"{label}-{cname}-m{mi}-t{m['t']:.2f}@4x.png"
                            im4.save(p)
                            items.append({"id": p.stem, "path": str(p), "ask": self.cfg["gallery"]["crossing_ask"]})
                        if cname == m["crossing"]:
                            strip.append(im.resize((im.width * 2, im.height * 2), Image.NEAREST))
                if strip:
                    gif = gdir / f"{label}-{m['crossing']}-m{mi}-motion.gif"
                    strip[0].save(gif, save_all=True, append_images=strip[1:], duration=250, loop=0)
                    w = sum(s.width for s in strip)
                    sheet = Image.new("RGB", (w, strip[0].height))
                    x = 0
                    for s in strip:
                        sheet.paste(s, (x, 0)); x += s.width
                    sheet.save(gdir / f"{label}-{m['crossing']}-m{mi}-strip.png")
        for pat, ask in self.cfg["gallery"].get("extra_crops", {}).items():
            for p in sorted(out.glob(pat))[:4]:
                items.append({"id": p.parent.name + "/" + p.stem, "path": str(p), "ask": ask})
        idx = ["# Gallery " + label, "", "Builder/loop does not self-certify (G4). CoS reviews.", ""]
        for it in items:
            idx.append(f"- `{it['id']}`: {it['path']}")
        for g in sorted(gdir.glob("*.gif")):
            idx.append(f"- motion: {g}")
        (gdir / "index.md").write_text("\n".join(idx) + "\n")
        return items

    def evaluate(self, out: pathlib.Path, seeds: list, seconds: float, quick=False, do_vision=True, label="eval") -> dict:
        out.mkdir(parents=True, exist_ok=True)
        t0 = time.time()
        res = {"g15.import_md5": self.g15(out)}
        info = {"quick": quick}
        if res["g15.import_md5"]["status"] != core.PASS and res["g15.import_md5"].get("errors"):
            # Broken scripts: the suite would hang to its timeout (run 20261116-1525 iter-01, 25 min). Stop here.
            info["aborted"] = "import failed: " + " | ".join(res["g15.import_md5"]["errors"][:3])
            info["secs"] = round(time.time() - t0)
            (out / "results.json").write_text(json.dumps({"results": res, "info": info}, indent=1, default=str))
            return {"results": res, "info": info}
        res.update(self.suite(out))
        if not quick:
            res.update(self.renders(out, seeds[0] if seeds else 1))
            res.update(self.checks(out))
            sim, n_ok, rows = self.sims(out, seeds, seconds)
            res.update(sim)
            info["seeds_all_pass"] = n_ok
            info["times_all_pass"] = self.times_all_pass(res)
            items = self.gallery(out, label)
            info["gallery"] = str(out / "gallery")
            if do_vision:
                res["vision.crop_review"] = vision(self.cfg, self.limits, items, self.log)
        info["secs"] = round(time.time() - t0)
        (out / "results.json").write_text(json.dumps({"results": res, "info": info}, indent=1, default=str))
        return {"results": res, "info": info}


# ======================================================================= proposal
FIX_PROMPT = """You are fixing a realism defect in Lessville, a Godot 4.7 GDScript 2D town sim.
Target: {tid} - {title}
Job spec (follow it; do NOT repeat the failed approaches it lists):
<spec>
{spec}
</spec>
Failing checks now:
{fails}
Checks that pass now and MUST keep passing (any regression is auto-reverted):
{passing}
Source excerpts (exact text; unrelated functions are omitted and marked "# ... N lines omitted ..."):
{files}
{feedback}
Answer with ONE JSON object only (it is schema-checked):
{{"plan": "<= {plan_chars} chars: what you change and why it fixes the failing checks without breaking the passing ones",
  "edits": [{{"file": "<one of: {editable}>", "func": "<name of the function that contains the search text, or \\"\\" for top-level lines>",
             "search": "<exact contiguous text copied from that function, unique in it>",
             "replace": "<replacement text>"}}]}}
Rules: each "search" must be copied character-for-character (tabs and double quotes included, JSON-escaped as \\")
from the file text above and must occur exactly once in the function named in "func"; never invent comments in a search; keep each search short (a few lines around the change) but unique; edits apply
in order. Do not edit tests under test/ except to ADD new checks. Do not loosen any gate. Use tabs for GDScript indentation.
SIZE LIMIT (hard, checked before anything is applied): make MINIMAL edits. Never rewrite a whole function or file.
Each "replace" must be at most {max_replace} characters and all "replace" texts together at most {max_total} characters;
larger proposals are rejected unread. Only the "search" text is removed: a search that is just a function signature leaves the old body in place
(run 20261116-1525 broke the parse that way). Change only the lines that must change; add small helper functions instead of
rewriting big ones. The code must parse: declare every variable once per scope and keep valid GDScript 2 (Godot 4.7)."""

# Run 20261116-1653 (ROOT-CAUSE-job1a-scope.md): search/replace splices landed in the wrong scope.  Jobs with
# edit_mode: whole_function answer with complete functions; the harness swaps each function's whole span.
FUNC_TAIL = """Answer with ONE JSON object only (it is schema-checked):
{{"plan": "<= {plan_chars} chars: what you change and why it fixes the failing checks without breaking the passing ones",
  "functions": [{{"file": "<one of: {editable}>", "target": "<function name>",
                 "code": "<the COMPLETE new function: its 'func' line and its whole body, nothing else>"}}]}}
Rules (WHOLE-FUNCTION MODE, enforced by the harness before anything is applied):
- Each item replaces the ENTIRE existing function named "target" (from its 'func' line to the next top-level declaration).
- "code" starts at column 0 with 'func <target>(' (or 'static func <target>(') using exactly the target name, and holds the full
  body indented with tabs. No other functions, no top-level var/const, no fragments or partial snippets: those are rejected.
- To add a helper, use a new target name with its complete code; it is appended to the end of the file.
- Functions you may replace: {funcs}. Each target at most once. Copy unchanged lines exactly (double quotes included, JSON-escaped as \\").
- SIZE LIMIT: each "code" at most {max_func} characters, all "code" together at most {max_total} characters.
- Do not edit tests under test/. Do not loosen any gate. The code must parse as GDScript 2 (Godot 4.7): declare every
  variable once per scope and keep every continue/break inside its loop."""

BLOCKS_TAIL = """Answer in this plain-text format (NOT JSON; code is raw GDScript, never escaped):
PLAN: <= {plan_chars} chars: what you change and why it fixes the failing checks without breaking the passing ones
### TARGET: <function name>
```gdscript
<the COMPLETE new function: its 'func' line and its whole body, real tabs, nothing else>
```
(one '### TARGET:' header + one fenced block per function; file: {editable})
Rules (WHOLE-FUNCTION MODE, enforced by the harness before anything is applied):
- Each block replaces the ENTIRE existing function named in its TARGET header (from its 'func' line to the next top-level declaration).
- The block starts at column 0 with 'func <target>(' (or 'static func <target>(') using exactly the target name, and holds the full
  body indented with tabs. No other functions, no top-level var/const, no fragments or partial snippets: those are rejected.
- To add a helper, use a new target name with its complete code; it is appended to the end of the file.
- Functions you may replace: {funcs}. Each target at most once. Copy unchanged lines exactly.
- SIZE LIMIT: each block at most {max_func} characters, all blocks together at most {max_total} characters.
- Do not edit tests under test/. Do not loosen any gate. The code must parse as GDScript 2 (Godot 4.7): declare every
  variable once per scope and keep every continue/break inside its loop."""

REPAIR_MSG = """Your proposal was applied and the Godot 4.7 parse check FAILED:
{perr}
Offending code as it is in the file now (line numbers on the left are file lines, not part of the code):
{offending}
Send ONE corrected proposal in the same JSON format. It is applied on top of the file AS IT IS NOW (your previous change
is already in it). {mode_rule} Fix the parse error and keep the intended behaviour. This is the only repair pass."""

GEN_PROMPT = """Lessville (Godot 4.7, GDScript; renders under {out} as PNG; python3 + PIL available).
Spec attribute with NO automated check:
{attr}
Existing check examples: tools/check_atmosphere.py style (exit 0 = pass, nonzero = fail, prints one summary line last).
Write ONE new python check at tools/realism_checks/{slug}.py that asserts this attribute on the renders dir passed as
--dir (or on repo files under --root). It must be deterministic and must FAIL on a known-bad state.
Also write a MUTATION: one SEARCH/REPLACE edit on the game code/config that makes the attribute wrong, so the check fails on it.
Output exactly:
NEWFILE: tools/realism_checks/{slug}.py
```
<python>
```
MUTATION:
FILE: <path>
<<<<<<< SEARCH
<exact text>
=======
<bad text>
>>>>>>> REPLACE
Render dir layout: {layout}"""


def save_raw(d: pathlib.Path, r: dict, prefix: str = "proposal") -> None:
    """Always keep the raw model output (content AND reasoning, finish_reason, usage, request params)."""
    (d / f"{prefix}.txt").write_text(r.get("text") or "")
    (d / f"{prefix}.raw.json").write_text(json.dumps({k: r.get(k) for k in ("model", "endpoint", "secs", "finish", "usage", "request", "text", "reasoning", "raw")}, indent=1, default=str))


def read_context(wt: pathlib.Path, files: list, budget: int, funcs: dict | None = None) -> str:
    """Exact file text, or for files listed in funcs only the header plus the named functions
    (run 20261116-1525: whole files made an 82 KB prompt and invited whole-file rewrites)."""
    out, used = [], 0
    for f in files:
        p = wt / f
        if not p.exists():
            continue
        t = p.read_text()
        if funcs and f in funcs:
            t = core.gd_excerpt(t, funcs[f])
        if used + len(t) > budget:
            t = t[: max(0, budget - used)]
        used += len(t)
        out.append(f"=== {f} ===\n{t}")
    return "\n".join(out)


# ======================================================================= the loop
class Loop:
    def __init__(self, cfg: dict, run_id: str | None = None):
        self.cfg = cfg
        self.run_id = run_id or dt.datetime.now(SGT).strftime("%Y%m%d-%H%M")
        self.run_dir = pathlib.Path(cfg["runs_dir"]) / self.run_id
        self.run_dir.mkdir(parents=True, exist_ok=True)
        self.wt = pathlib.Path(cfg["worktree"])
        self.limits = Limits(cfg)
        self.stopfile = pathlib.Path(cfg["runs_dir"]) / "STOP"
        self.history: list = []
        self.strikes: dict = {}
        self.blocked: set = set()
        self.feedback: dict = {}
        self.rejections: dict = {}
        if cfg.get("only_targets"):  # e.g. a single-job re-run (CoS third attempt at job1a)
            cfg["targets"] = [t for t in cfg["targets"] if t["id"] in cfg["only_targets"]]

    # ---------------------------------------------------------------- utils
    def log(self, msg: str) -> None:
        line = f"[{now()}] {msg}"
        print(line, flush=True)
        with open(self.run_dir / "loop.log", "a") as f:
            f.write(line + "\n")

    def state(self, **kw) -> None:
        st = {"run_id": self.run_id, "pid": os.getpid(), "updated": now(), "run_dir": str(self.run_dir),
              "worktree": str(self.wt), "branch": self.run_branch}
        st.update(kw)
        (self.run_dir / "state.json").write_text(json.dumps(st, indent=1))
        (pathlib.Path(self.cfg["runs_dir"]) / "current.json").write_text(json.dumps(st, indent=1))

    def stopped(self) -> bool:
        return self.stopfile.exists()

    def lesson(self, title: str, body: dict) -> None:
        d = self.run_dir / "lessons"
        d.mkdir(exist_ok=True)
        n = len(list(d.glob("*.md"))) + 1
        txt = [f"# {self.run_id}-{n:02d}: {title}", "",
               f"- Date (SGT): {now()}", "- Bot: Grok The Builder (BL-0167 realism loop, unattended)",
               "- System: box renders + megatron qwen3.8-27b-nvfp4 / qwen3-vl-8b", "- Item: BL-0167 (job " + body.get("target", "") + ")"]
        for k in ("what happened", "attempts and cost", "root cause", "fix", "guard proposal"):
            txt.append(f"- {k.capitalize()}: {body.get(k, 'TBD')}")
        (d / f"{self.run_id}-{n:02d}.md").write_text("\n".join(txt) + "\n")

    # ---------------------------------------------------------------- git
    def setup_worktree(self) -> None:
        repo = pathlib.Path(self.cfg["repo"])
        self.run_branch = f"loop/run-{self.run_id}"
        g = core.GitGuard(repo)
        if self.wt.exists():
            cur = core.GitGuard(self.wt).branch()
            if not cur.startswith("loop/"):
                raise RuntimeError(f"worktree {self.wt} is on {cur}; refusing to reuse")
            g.git("worktree", "remove", "--force", str(self.wt))
        g.git("worktree", "add", "-b", self.run_branch, str(self.wt), self.cfg["base_branch"])
        self.git = core.GitGuard(self.wt)
        self.git.assert_loop_branch()

    # ---------------------------------------------------------------- spec / coverage
    def spec(self) -> list:
        p = pathlib.Path(self.cfg["spec"])
        items = (yaml.safe_load(p.read_text()) or {}).get("attributes", []) if p.exists() else []
        gen = self.wt / "tools" / "realism_loop" / "generated_checks.yaml"
        bound = {c["attribute"]: c["id"] for c in ((yaml.safe_load(gen.read_text()) or []) if gen.exists() else [])}
        for it in items:
            if it.get("id") in bound:
                it["check"] = bound[it["id"]]
        return items

    def cov(self, results: dict) -> dict:
        r = dict(results)
        for alias, real in (self.cfg.get("check_aliases") or {}).items():
            if real in results:
                r[alias] = results[real]
        return core.coverage(self.spec(), r, self.known_checks(r))

    def known_checks(self, results: dict) -> set:
        ks = set(results)
        ks |= {c["id"] for c in self.cfg["checks"]}
        return ks

    # ---------------------------------------------------------------- records
    def record(self, rec: dict) -> None:
        self.state(phase=f"after iter {rec.get('iter')}")
        with open(self.run_dir / "iterations.jsonl", "a") as f:
            f.write(json.dumps(rec, default=str) + "\n")
        summarize(self.run_dir, self.cfg)

    def eval_full(self, tag: str) -> dict:
        s = self.cfg["sim"]
        return Evaluator(self.cfg, self.wt, self.limits, self.log).evaluate(
            self.run_dir / tag, s["seeds"], s["seconds"], quick=False, label=tag)

    def eval_quick(self, tag: str) -> dict:
        return Evaluator(self.cfg, self.wt, self.limits, self.log).evaluate(self.run_dir / tag, [], 0, quick=True)

    # ---------------------------------------------------------------- iteration
    def parse_gate(self, changed: list) -> str:
        """Seconds-long Godot --check-only of the edited scripts, before any suite run."""
        if not self.cfg.get("godot"):
            return ""
        return Evaluator(self.cfg, self.wt, self.limits, self.log).check_only(changed)

    def _job(self, target: dict) -> dict:
        return yaml.safe_load((HERE / "jobs" / target["job"]).read_text()) if target.get("job") else {}

    def build_fix_prompt(self, target: dict, results: dict) -> str:
        job = self._job(target)
        lc = self.cfg.get("llm", {})
        spec = job.get("spec") or ""
        if job.get("spec_md"):
            md = pathlib.Path(job["spec_md"]).read_text().splitlines()
            rng = job.get("spec_lines")  # [first, last], 1-based inclusive; default whole file
            spec = (spec + "\n" if spec else "") + "\n".join(md[rng[0] - 1:rng[1]] if rng else md)
        spec = spec or target.get("title", "")
        fails = "\n".join(f"- {k}: {v.get('detail', '')}" for k, v in sorted(results.items()) if v.get("status") in (core.FAIL, core.ERROR))
        passing = ", ".join(sorted(core.passing(results)))
        files = read_context(self.wt, job.get("context_files", []), int(lc.get("context_chars", 160000)), job.get("context_funcs") or {})
        editable = [f for f in job.get("context_files", []) if f.startswith(tuple(self.cfg["allowed_edit_prefixes"])) and f not in job.get("read_only", [])]
        fb = (getattr(self, "feedback", {}) or {}).get(target["id"])
        feedback = (f"\nPREVIOUS ATTEMPT WAS REJECTED (do not repeat it): {fb}\n" if fb else "")
        if job.get("edit_mode") == "whole_function":
            head = FIX_PROMPT[:FIX_PROMPT.index("Answer with ONE JSON object")]
            cf = job.get("context_funcs") or {}
            funcs = "; ".join(f"{f}: " + ", ".join(n for n in cf.get(f, []) if not n.startswith("re:")) for f in editable if cf.get(f)) or "any in the excerpts"
            tail = BLOCKS_TAIL if job.get("answer_format") == "code_blocks" else FUNC_TAIL
            return (head + tail).format(tid=target["id"], title=target.get("title", ""), spec=spec, fails=fails, passing=passing,
                                             files=files, feedback=feedback, editable=", ".join(editable), plan_chars=int(lc.get("fix_plan_chars", 2000)),
                                             funcs=funcs, max_func=int(lc.get("fix_max_func_chars", 6000)), max_total=int(lc.get("fix_max_total_func_chars", 12000)))
        return FIX_PROMPT.format(tid=target["id"], title=target.get("title", ""), spec=spec, fails=fails, passing=passing,
                                 files=files, feedback=feedback, editable=", ".join(editable), plan_chars=int(lc.get("fix_plan_chars", 2000)),
                                 max_replace=int(lc.get("fix_max_replace_chars", 2500)), max_total=int(lc.get("fix_max_total_replace_chars", 6000)))

    def propose_fix(self, target: dict, results: dict, out_dir: pathlib.Path | None = None) -> tuple[str, dict]:
        job = self._job(target)
        lc = self.cfg["llm"]
        editable = [f for f in job.get("context_files", []) if f.startswith(tuple(self.cfg["allowed_edit_prefixes"])) and f not in job.get("read_only", [])]
        plan_chars = int(lc.get("fix_plan_chars", 2000))
        prompt = self.build_fix_prompt(target, results)
        d = out_dir or self.cur_dir
        (d / "prompt.txt").write_text(prompt)
        # Root cause of run 20261116-1409: thinking on + 16k cap -> all tokens spent in <think>, empty content.
        # Fix proposals: thinking off, strict JSON schema (server-side guided decoding), file enum = editable files.
        rf = self._response_format(job, editable)
        r = llm(self.cfg, self.limits, [{"role": "user", "content": prompt}], self.log,
                max_tokens=int(lc.get("fix_max_tokens", 12000)), thinking=bool(lc.get("fix_thinking", False)), response_format=rf,
                stream=bool(lc.get("fix_stream", False)))
        save_raw(d, r)
        return r["text"], r

    def _editable(self, job: dict) -> list:
        return [f for f in job.get("context_files", []) if f.startswith(tuple(self.cfg["allowed_edit_prefixes"])) and f not in job.get("read_only", [])]

    def _response_format(self, job: dict, editable: list) -> dict:
        lc = self.cfg.get("llm", {})
        plan_chars = int(lc.get("fix_plan_chars", 2000))
        if job.get("answer_format") == "code_blocks":
            return None  # plain text: raw fenced code, no JSON escaping (run 20261116-2022)
        if job.get("edit_mode") == "whole_function":
            return {"type": "json_schema", "json_schema": {"name": "functions", "strict": True,
                                                           "schema": core.func_schema(editable, plan_chars, int(lc.get("fix_max_funcs", 6)))}}
        return {"type": "json_schema", "json_schema": {"name": "edits", "strict": True,
                                                       "schema": core.edit_schema(editable, plan_chars, int(lc.get("fix_max_edits", 12)))}}

    def repair_fix(self, target: dict, prev_text: str, perr: str, offending: str, out_dir: pathlib.Path | None = None) -> tuple[str, dict]:
        """The ONE repair pass (G20): same conversation, plus the exact parse error and the offending function."""
        job = self._job(target)
        lc = self.cfg["llm"]
        d = out_dir or self.cur_dir
        prompt = (d / "prompt.txt").read_text() if (d / "prompt.txt").exists() else self.build_fix_prompt(target, {})
        rule = ("Give every function you change again IN FULL as '### TARGET: <name>' + one fenced gdscript block (same rules, no JSON)."
                if job.get("answer_format") == "code_blocks" else
                "Give every function you change again IN FULL (whole-function items; same rules)." if job.get("edit_mode") == "whole_function"
                else "Edits must match the file as it is now.")
        msg = REPAIR_MSG.format(perr=perr, offending=offending, mode_rule=rule)
        (d / "repair.prompt.txt").write_text(msg)
        r = llm(self.cfg, self.limits, [{"role": "user", "content": prompt}, {"role": "assistant", "content": prev_text},
                                        {"role": "user", "content": msg}], self.log,
                max_tokens=int(lc.get("fix_max_tokens", 12000)), thinking=bool(lc.get("fix_thinking", False)),
                response_format=self._response_format(job, self._editable(job)), stream=bool(lc.get("fix_stream", False)))
        save_raw(d, r, "repair")
        return r["text"], r

    def offending_code(self, perr: str, changed: list) -> str:
        """Numbered text of the function(s) holding the parse error lines (or +-12 lines around them)."""
        out = []
        for rel, line in list(dict.fromkeys(core.parse_error_locs(perr)))[:2]:
            p = self.wt / rel
            if not p.exists():
                continue
            src = p.read_text()
            lines = src.splitlines()
            fn = core.func_at_line(src, line)
            if fn:
                a, b = core.func_decl_span(src, fn)
                lo, hi = src.count("\n", 0, a) + 1, src.count("\n", 0, b)
            else:
                lo, hi = max(1, line - 12), min(len(lines), line + 12)
            out.append(f"=== {rel} lines {lo}-{hi}" + (f" (func {fn})" if fn else "") + " ===\n" +
                       "\n".join(f"{n:5d}{'>' if n == line else ' '} {lines[n - 1]}" for n in range(lo, hi + 1)))
        return "\n".join(out)[:12000] or "(no file:line in the error; see the error text)"

    def _apply_proposal(self, job: dict, text: str, meta: dict, rec: dict) -> list:
        """Parse, size-check, validate and apply one proposal (search/replace or whole-function). ValueError on rejection."""
        lc = self.cfg.get("llm", {})
        prefixes = tuple(self.cfg["allowed_edit_prefixes"])
        if job.get("edit_mode") == "whole_function":
            if job.get("answer_format") == "code_blocks":
                ed = self._editable(job)
                fs, why_p = core.parse_func_blocks(text, meta.get("finish"), meta.get("usage"), ed[0] if len(ed) == 1 else None)
            else:
                fs, why_p = core.parse_func_proposal(text, meta.get("finish"), meta.get("usage"))
            rec["edits"], rec["parse"] = len(fs), why_p
            if not fs:
                self.log(f"proposal rejected ({why_p}); raw in {self.cur_dir}; content head: {text[:300]!r}")
                raise ValueError(f"proposal rejected: {why_p}")
            sizes = [len(f["code"]) for f in fs]
            rec["replace_chars"] = sum(sizes)
            mf, mt = int(lc.get("fix_max_func_chars", 6000)), int(lc.get("fix_max_total_func_chars", 12000))
            if max(sizes) > mf or sum(sizes) > mt:
                raise ValueError(f"functions too large: code chars {sizes} (cap {mf} each, {mt} total)")
            shown = job.get("context_funcs") or {}
            for f in fs:
                names = [n for n in shown.get(f["file"], []) if not n.startswith("re:")]
                p = self.wt / f["file"]
                exists = p.exists() and core.func_decl_span(p.read_text(), f["target"]) is not None
                if names and exists and f["target"] not in names:
                    raise ValueError(f"edits do not apply: function {f['target']!r} was not shown and may not be replaced (allowed: {names})")
            bad = core.check_functions(self.wt, fs, prefixes, allow_new=bool(job.get("allow_new_functions")))
            if bad:
                raise ValueError(f"edits do not apply (whole function mode): {bad}")
            return core.apply_functions(self.wt, fs, prefixes, allow_new=bool(job.get("allow_new_functions")))
        edits, why_p = core.parse_proposal(text, meta.get("finish"), meta.get("usage"))
        rec["edits"], rec["parse"] = len(edits), why_p
        if not edits:
            self.log(f"proposal rejected ({why_p}); raw: {self.cur_dir / 'proposal.raw.json'}; content head: {text[:300]!r}")
            raise ValueError(f"proposal rejected: {why_p}")
        big = core.edit_size_problem(edits, int(lc.get("fix_max_replace_chars", 2500)), int(lc.get("fix_max_total_replace_chars", 6000)))
        rec["replace_chars"] = sum(len(e.get("replace", "")) for e in edits)
        if big:
            raise ValueError(big)
        bad = core.check_edits(self.wt, edits, prefixes)
        if bad:
            self.log(f"edits do not apply ({bad}); raw: {self.cur_dir / 'proposal.raw.json'}")
            raise ValueError(f"edits do not apply: {bad}")
        return core.apply_edits(self.wt, edits, prefixes)

    def iteration(self, i: int, base: dict) -> dict:
        self.cur_dir = self.run_dir / f"iter-{i:02d}"
        self.cur_dir.mkdir(exist_ok=True)
        results = base["results"]
        cov = self.cov(results)
        targets = [t for t in self.cfg["targets"] if t["id"] not in self.blocked]
        target = core.pick_target(results, targets, {"rows": [r for r in cov["rows"] if "gen-check:" + str(r["id"]) not in self.blocked]},
                                  all_targets=self.cfg["targets"])
        rec = {"iter": i, "time": now(), "target": target["id"] if target else None, "kind": target.get("kind") if target else None,
               "failing_before": sorted(core.failing(results)), "coverage_pct": cov["coverage_pct"], "progress": False,
               "accepted": False, "error_sig": ""}
        if not target:
            rec["note"] = "no target left"
            return rec
        head0 = self.git.head()
        scratch = f"loop/scratch/{self.run_id}-i{i:02d}"
        self.git.git("checkout", "-q", "-B", scratch)
        self.git.assert_loop_branch()
        try:
            if target["kind"] == "fix":
                text, meta = self.propose_fix(target, results)
                rec["model"] = f"{meta['model']}@{meta['endpoint']} {meta['secs']}s {(meta.get('usage') or {}).get('completion_tokens')} tok finish={meta.get('finish')}"
                job = self._job(target)
                rec["edit_mode"] = job.get("edit_mode", "search_replace")
                changed = self._apply_proposal(job, text, meta, rec)
                rec["changed"] = changed
                # G20 parse pre-check of every patched file inside the iteration; ONE repair pass on failure.
                perr = self.parse_gate(changed)
                rec["parse_gate"] = perr or "ok"
                if perr:
                    self.log(f"iter {i}: patch broke parse; one repair pass: {perr[:300]}")
                    rep_ = {"first_error": perr[:600]}
                    rec["repair"] = rep_
                    try:
                        text2, meta2 = self.repair_fix(target, text, perr, self.offending_code(perr, changed))
                        rep_["model"] = f"{meta2.get('model')}@{meta2.get('endpoint')} {meta2.get('secs')}s {(meta2.get('usage') or {}).get('completion_tokens')} tok finish={meta2.get('finish')}"
                        changed = sorted(set(changed) | set(self._apply_proposal(job, text2, meta2, rep_)))
                    except Exception as ex2:
                        raise ValueError(f"patch broke parse: {perr}; repair pass failed: {str(ex2)[:300]}")
                    rec["changed"] = changed
                    perr2 = self.parse_gate(changed)
                    rep_["parse_gate"] = perr2 or "ok"
                    if perr2:
                        raise ValueError(f"patch broke parse after 1 repair pass: {perr2} (first error: {perr[:300]})")
                quick = self.eval_quick(f"iter-{i:02d}/quick")
                q = quick["results"]
                if quick.get("info", {}).get("aborted") or q.get("g15.import_md5", {}).get("status") == core.FAIL:
                    g = q.get("g15.import_md5", {})
                    raise ValueError("patch broke parse: " + (" | ".join(g.get("errors") or []) or g.get("detail") or quick["info"].get("aborted", ""))[:500])
                regq = [k for k in core.regressions(results, q, self.cfg.get("advisory_checks", [])) if k in q]
                okq, whyq = core.fail_before_gate(results, q, target["checks"])
                rec["quick"] = {"gate": whyq, "regressions": regq}
                if not okq or regq:
                    raise ValueError(f"quick gate: {whyq}; regressions {regq}")
                full = self.eval_full(f"iter-{i:02d}/after")
                after = full["results"]
                regs = core.regressions(results, after, self.cfg.get("advisory_checks", []))
                ok, why = core.fail_before_gate(results, after, target["checks"])
                rec.update({"gate": why, "regressions": regs, "failing_after": sorted(core.failing(after))})
                if not ok or regs:
                    raise ValueError(f"gate: {why}; regressions {regs}")
                msg = (f"loop {self.run_id} i{i:02d}: {target['id']}\n\nModel: {rec['model']}\nGate: {why}\n"
                       f"Failing before: {len(core.failing(results))}, after: {len(core.failing(after))}\nNo regressions.")
                sha = self.git.commit(changed, msg, self.cfg["author"])
                self.git.git("checkout", "-q", self.run_branch)
                self.git.git("merge", "-q", "--ff-only", scratch)
                rec.update({"accepted": True, "commit": sha, "progress": True})
                base.clear(); base.update(full)
            else:
                rec.update(self.generate_check(target, i))
        except Exception as ex:
            rec["rejected"] = str(ex)[:600]
            self.feedback[target["id"]] = str(ex)[:800]  # shown to the model on the next attempt at this target
            self.log(f"iter {i}: rejected: {str(ex)[:300]}")
            # Keep the scratch branch for audit, put the run branch back.
            try:
                dirty = self.git.git("status", "--porcelain")
                if dirty:
                    self.git.git("add", "-A", "--", "scripts", "test", "tools", "config")
                    name, email = re.match(r"(.*) <(.*)>", self.cfg["author"]).groups()
                    self.git.git("-c", f"user.name={name}", "-c", f"user.email={email}", "commit", "-q", "-m", f"loop {self.run_id} i{i:02d}: REJECTED candidate for {target['id']} (audit only)", check=False)
            except Exception:
                pass
            self.git.git("checkout", "-q", "-f", self.run_branch)
            if self.git.head() != head0:
                self.git.revert_to(head0)
            sig = re.sub(r"\d+", "N", str(ex))[:120]
            n = self.strikes.get(target["id"], 0) + 1
            self.strikes[target["id"]] = n
            self.rejections.setdefault(target["id"], []).append(f"iter {i}: {str(ex)[:300]}")
            if n >= int(self.cfg.get("max_attempts_per_target", 2)):
                self.blocked.add(target["id"])
                self.lesson(f"{target['id']} rejected {n}x; target parked (G1)", {
                    "target": target["id"], "what happened": " || ".join(self.rejections[target["id"]]),
                    "attempts and cost": f"{n} local-model attempts (+ at most 1 repair pass each), no spend",
                    "root cause": f"NOT YET WRITTEN: write {self.run_dir}/ROOT-CAUSE-{target['id']}.md from iter-NN/prompt.txt, proposal*.txt, repair*.txt and the replayed diffs before any further attempt",
                    "fix": "human or CoS decision; the loop will not try a third time", "guard proposal": "G1 applied by the harness"})
            if isinstance(ex, RuntimeError) and "unavailable" in str(ex):
                rec["error_sig"] = sig
        return rec

    def generate_check(self, target: dict, i: int) -> dict:
        attr = next((a for a in self.spec() if a.get("id") == target["attribute"]), {"id": target["attribute"]})
        slug = re.sub(r"[^a-z0-9]+", "_", str(attr["id"]).lower()).strip("_")[:60]
        cid = f"gen.{slug}"
        layout = ", ".join(sorted({p.parent.name for p in (self.run_dir / "baseline").rglob("*.png")}))[:800]
        prompt = GEN_PROMPT.format(out="<render dir>", attr=yaml.safe_dump(attr, sort_keys=False), slug=slug, layout=layout)
        (self.cur_dir / "prompt.txt").write_text(prompt)
        r = llm(self.cfg, self.limits, [{"role": "user", "content": prompt}], self.log, thinking=False, max_tokens=6000)
        save_raw(self.cur_dir, r)
        text = r["text"]
        if r.get("finish") == "length" or not text.strip():
            raise ValueError(core.parse_proposal(text, r.get("finish"), r.get("usage"))[1])
        text = core.THINK.sub("", text)
        head, _, mut = text.partition("MUTATION:")
        new = core.parse_edits(head)
        mutation = core.parse_edits(mut)
        if not new or not mutation or not any("new" in e for e in new):
            raise ValueError("generated check: missing NEWFILE or MUTATION")
        changed = core.apply_edits(self.wt, [e for e in new if "new" in e], ("tools/realism_checks/",))
        cmd = f"python3 {{wt}}/tools/realism_checks/{slug}.py --dir {{out}} --root {{wt}}"
        rd = self.run_dir / "baseline"
        c_cur = sh(cmd.format(wt=self.wt, out=rd), cwd=self.wt, timeout=300, log=self.cur_dir / "gen-current.log")
        cur = {cid: {"status": core.PASS if c_cur[0] == 0 else (core.FAIL if c_cur[0] == 1 else core.ERROR)}}
        # Fail-before: apply the mutation in a throwaway copy of the affected files, re-render quick-only if needed.
        backup = {e["file"]: (self.wt / e["file"]).read_text() for e in mutation if (self.wt / e["file"]).exists()}
        try:
            core.apply_edits(self.wt, mutation, tuple(self.cfg["allowed_edit_prefixes"]))
            mdir = self.cur_dir / "mutant"
            Evaluator(self.cfg, self.wt, self.limits, self.log).renders(mdir, 1)
            c_mut = sh(cmd.format(wt=self.wt, out=mdir), cwd=self.wt, timeout=300, log=self.cur_dir / "gen-mutant.log")
        finally:
            for f, t in backup.items():
                (self.wt / f).write_text(t)
        mutr = {cid: {"status": core.FAIL if c_mut[0] == 1 else (core.PASS if c_mut[0] == 0 else core.ERROR)}}
        ok, why = core.new_check_gate(mutr, cur, cid)
        if not ok:
            raise ValueError(why)
        reg = self.wt / "tools" / "realism_loop" / "generated_checks.yaml"
        lst = (yaml.safe_load(reg.read_text()) or []) if reg.exists() else []
        lst.append({"id": cid, "attribute": attr["id"], "cmd": cmd, "fail_before": why})
        reg.write_text(yaml.safe_dump(lst, sort_keys=False))
        sha = self.git.commit(changed + ["tools/realism_loop/generated_checks.yaml"],
                              f"loop {self.run_id} i{i:02d}: add check {cid}\n\nModel: {r['model']}@{r['endpoint']}\nFail-before: {why}", self.cfg["author"])
        scratch = self.git.branch()
        self.git.git("checkout", "-q", self.run_branch)
        self.git.git("merge", "-q", "--ff-only", scratch)
        return {"accepted": True, "commit": sha, "progress": True, "gate": why, "new_check": cid,
                "model": f"{r['model']}@{r['endpoint']} {r['secs']}s"}

    # ---------------------------------------------------------------- main
    def run(self) -> int:
        self.run_branch = f"loop/run-{self.run_id}"
        if self.stopfile.exists():
            self.stopfile.unlink()
        sc = self.cfg["stop"]
        stopcfg = core.StopCfg(n_seeds=sc["n_seeds"], k_no_progress=sc["k_no_progress"], max_iterations=sc["max_iterations"],
                               require_coverage=sc.get("require_coverage", 100.0), clock_times=tuple(sc["clock_times"]))
        try:
            self.setup_worktree()
            self.state(phase="baseline")
            self.log(f"run {self.run_id}: worktree {self.wt} on {self.run_branch} from {self.cfg['base_branch']}")
            base = self.eval_full("baseline")
            r0 = base["results"]
            cov = self.cov(r0)
            self.write_coverage(cov, 0)
            rec0 = {"iter": 0, "time": now(), "target": "baseline", "failing_after": sorted(core.failing(r0)),
                    "coverage_pct": cov["coverage_pct"], "progress": False, "accepted": False,
                    "seeds_all_pass": base["info"].get("seeds_all_pass", 0), "times_all_pass": base["info"].get("times_all_pass", []),
                    "failing": len(core.failing(r0)), "results": {k: {"status": v["status"], "detail": v.get("detail", "")[:300]} for k, v in r0.items()},
                    "gallery": base["info"].get("gallery"), "error_sig": ""}
            self.record(rec0)
            self.history = [rec0]
            i = 0
            while True:
                stop, why = core.decide_stop(self.history, stopcfg)
                if self.stopped():
                    stop, why = True, "STOP file"
                if stop:
                    break
                i += 1
                self.state(phase=f"iter {i}")
                rec = self.iteration(i, base)
                r = base["results"]
                cov = self.cov(r)
                self.write_coverage(cov, i)
                rec.update({"failing": len(core.failing(r)), "coverage_pct": cov["coverage_pct"],
                            "seeds_all_pass": base["info"].get("seeds_all_pass", 0), "times_all_pass": base["info"].get("times_all_pass", []),
                            "results": {k: {"status": v["status"], "detail": v.get("detail", "")[:300]} for k, v in r.items()}})
                self.history.append(rec)
                self.record(rec)
                if rec.get("note") == "no target left":
                    why = "no target left (all remaining targets parked by G1 or none)"
                    break
            self.log(f"stop: {why}")
            self.state(phase="stopped", reason=why)
            with open(self.run_dir / "iterations.jsonl", "a") as f:
                f.write(json.dumps({"stop": why, "time": now()}) + "\n")
            summarize(self.run_dir, self.cfg)
            return 0
        except Exception as ex:
            self.log("FATAL: " + traceback.format_exc()[-1500:])
            self.lesson("harness fatal error", {"what happened": repr(ex)[:400], "root cause": "see loop.log", "attempts and cost": "1, local only"})
            self.state(phase="failed", reason=repr(ex)[:300])
            summarize(self.run_dir, self.cfg)
            return 1

    def write_coverage(self, cov: dict, i: int) -> None:
        c = {k: v for k, v in cov.items() if k != "rows"}
        c.update({"iter": i, "time": now(), "missing": [r["id"] for r in cov["rows"] if r["check"] == core.MISSING],
                  "failing_attrs": [r["id"] for r in cov["rows"] if r["status"] in (core.FAIL, core.ERROR)], "rows": cov["rows"]})
        (self.run_dir / "coverage.json").write_text(json.dumps(c, indent=1))


# ======================================================================= summary
def summarize(run_dir: pathlib.Path, cfg: dict | None = None) -> None:
    run_dir = pathlib.Path(run_dir)
    recs, stop = [], None
    p = run_dir / "iterations.jsonl"
    if p.exists():
        for l in p.read_text().splitlines():
            if not l.strip():
                continue
            d = json.loads(l)
            if "stop" in d:
                stop = d
            else:
                recs.append(d)
    cov = json.loads((run_dir / "coverage.json").read_text()) if (run_dir / "coverage.json").exists() else {}
    st = json.loads((run_dir / "state.json").read_text()) if (run_dir / "state.json").exists() else {}
    L = [f"# BL-0167 realism loop — run {run_dir.name}", "",
         f"State: **{st.get('phase', '?')}**" + (f" — {st.get('reason')}" if st.get("reason") else "") + f" (updated {st.get('updated', '?')})",
         f"Branch: `{st.get('branch', '?')}` in `{st.get('worktree', '?')}` (loop branches only; master/step3/step4 untouched; client-v1 HELD)", ""]
    if cov:
        L += ["## Coverage", f"- Attributes in spec: {cov.get('attributes')}; with a check: {cov.get('with_check')}; without: {cov.get('without_check')} ({cov.get('coverage_pct')}% covered)",
              f"- Passing: {cov.get('passing')}; failing: {cov.get('failing')}; with check but not yet run: {cov.get('unverified')}", ""]
    if recs:
        last = recs[-1]
        L += ["## Latest check results", "| check | status | detail |", "|---|---|---|"]
        for k, v in sorted((last.get("results") or {}).items()):
            L.append(f"| {k} | {v['status']} | {str(v.get('detail', '')).replace('|', '/')[:160]} |")
        L += ["", f"Seeds all-pass: {last.get('seeds_all_pass')}; clock times all-pass: {', '.join(last.get('times_all_pass') or []) or 'none'}", ""]
        L += ["## Iterations", "| # | time | target | outcome | failing | coverage |", "|---|---|---|---|---|---|"]
        for r in recs:
            out = ("ACCEPTED " + str(r.get("commit", ""))[:8]) if r.get("accepted") else ("rejected: " + str(r.get("rejected", ""))[:120] if r.get("rejected") else (r.get("note") or "baseline"))
            L.append(f"| {r['iter']} | {r['time'][11:16]} | {r.get('target')} | {out.replace('|', '/')} | {r.get('failing')} | {r.get('coverage_pct')}% |")
    if stop:
        L += ["", f"**Stopped:** {stop['stop']} ({stop['time']})"]
    galleries = sorted(run_dir.glob("*/gallery/index.md")) + sorted(run_dir.glob("*/*/gallery/index.md"))
    if galleries:
        L += ["", "## Crop galleries (for CoS G4 review; the loop does not self-certify)"] + [f"- {g}" for g in galleries]
    lessons = sorted((run_dir / "lessons").glob("*.md")) if (run_dir / "lessons").exists() else []
    if lessons:
        L += ["", "## Lessons (Lessons Keeper format)"] + [f"- {x}" for x in lessons]
    (run_dir / "SUMMARY.md").write_text("\n".join(L) + "\n")


# ======================================================================= G2 gate
def dry_propose(cfg: dict, results_json: str, out: pathlib.Path | None) -> int:
    """One proposal call for the first fix target against a recorded results.json, WITHOUT touching the
    worktree: parse, then apply to a temp copy of the editable files.  Exit 0 only if edits parse and apply."""
    import tempfile
    res = json.loads(pathlib.Path(results_json).read_text())["results"]
    tid = os.environ.get("DRY_TARGET")
    target = next(t for t in cfg["targets"] if t.get("kind") == "fix" and (not tid or t["id"] == tid))
    lp = Loop.__new__(Loop)
    lp.cfg, lp.wt, lp.limits, lp.feedback = cfg, pathlib.Path(cfg.get("dry_worktree") or cfg["worktree"]), Limits(cfg), {}
    out = out or pathlib.Path(cfg["runs_dir"]) / ("dry-" + dt.datetime.now(SGT).strftime("%Y%m%d-%H%M"))
    out.mkdir(parents=True, exist_ok=True)
    lp.log = lambda m: print(f"[{now()}] {m}", flush=True)
    text, meta = lp.propose_fix(target, res, out_dir=out)
    edits, why = core.parse_proposal(text, meta.get("finish"), meta.get("usage"))
    rep_ = {"target": target["id"], "finish": meta.get("finish"), "usage": meta.get("usage"), "secs": meta.get("secs"),
            "parse": why, "edits": len(edits), "files": sorted({e["file"] for e in edits}),
            "replace_chars": [len(e.get("replace", "")) for e in edits], "prompt_chars": len((out / "prompt.txt").read_text())}
    lc = cfg.get("llm", {})
    rep_["size"] = core.edit_size_problem(edits, int(lc.get("fix_max_replace_chars", 2500)), int(lc.get("fix_max_total_replace_chars", 6000))) or "ok"
    if edits and rep_["size"] == "ok":
        with tempfile.TemporaryDirectory() as td:
            for f in {e["file"] for e in edits}:
                src = lp.wt / f
                if src.exists():
                    (pathlib.Path(td) / f).parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy(src, pathlib.Path(td) / f)
            bad = core.check_edits(pathlib.Path(td), edits, tuple(cfg["allowed_edit_prefixes"]))
            if not bad:
                changed = core.apply_edits(pathlib.Path(td), edits, tuple(cfg["allowed_edit_prefixes"]))
                rep_["applied_to_temp_copy"] = changed
                diff = subprocess.run(["bash", "-c", " ".join(f"diff -u {lp.wt / f} {pathlib.Path(td) / f};" for f in changed)],
                                      capture_output=True, text=True).stdout
                (out / "dry.diff").write_text(diff)
                rep_["diff_lines"] = len(diff.splitlines())
            rep_["apply"] = bad or "ok"
    ok = bool(edits) and rep_.get("apply") == "ok" and rep_["size"] == "ok"
    rep_["gate"] = "PASS" if ok else "FAIL"
    (out / "dry-result.json").write_text(json.dumps(rep_, indent=1, default=str))
    print(json.dumps(rep_, indent=1, default=str))
    return 0 if ok else 3


# ======================================================================= CLI
def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["start", "run", "stop", "status", "summarize", "dry-propose"])
    ap.add_argument("arg", nargs="?")
    ap.add_argument("--config", default=str(DEFAULT_CFG))
    ap.add_argument("--run-id", default=None)
    ap.add_argument("--out", default=None, help="dry-propose: directory for prompt/raw/result")
    a = ap.parse_args(argv)
    cfg = load_cfg(a.config)
    runs = pathlib.Path(cfg["runs_dir"])
    runs.mkdir(parents=True, exist_ok=True)
    if a.cmd == "run":
        return Loop(cfg, a.run_id).run()
    if a.cmd == "start":
        cur = runs / "current.json"
        if cur.exists():
            st = json.loads(cur.read_text())
            try:
                os.kill(int(st.get("pid", 0)), 0)
                if st.get("phase") not in ("stopped", "failed"):
                    print(f"already running: pid {st['pid']} run {st['run_id']}")
                    return 1
            except (OSError, ValueError):
                pass
        rid = a.run_id or dt.datetime.now(SGT).strftime("%Y%m%d-%H%M")
        # The runner executes from a frozen copy of the harness so the loop's own
        # worktree edits can never change the code that is judging them.
        frozen = runs / rid / "harness"
        shutil.copytree(HERE, frozen, dirs_exist_ok=True, ignore=shutil.ignore_patterns("__pycache__"))
        shutil.copy(a.config, frozen / "config.yaml")
        logf = open(runs / rid / "stdout.log", "a")
        p = subprocess.Popen([sys.executable, str(frozen / "loop.py"), "run", "--config", str(frozen / "config.yaml"), "--run-id", rid],
                             stdout=logf, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, start_new_session=True, cwd="/")
        (runs / "current.json").write_text(json.dumps({"run_id": rid, "pid": p.pid, "phase": "starting", "run_dir": str(runs / rid), "updated": now()}))
        print(f"started run {rid} pid {p.pid}; summary: {runs / rid / 'SUMMARY.md'}")
        return 0
    if a.cmd == "stop":
        (runs / "STOP").write_text(now())
        print("STOP file written; the loop exits after its current step (kill the pid in current.json to stop at once)")
        return 0
    if a.cmd == "status":
        cur = runs / "current.json"
        if not cur.exists():
            print("no run")
            return 0
        st = json.loads(cur.read_text())
        try:
            os.kill(int(st.get("pid", 0)), 0); alive = True
        except (OSError, ValueError):
            alive = False
        print(json.dumps(dict(st, alive=alive), indent=1))
        it = pathlib.Path(st["run_dir"]) / "iterations.jsonl"
        if it.exists():
            for l in it.read_text().splitlines()[-3:]:
                d = json.loads(l)
                print({k: d.get(k) for k in ("iter", "time", "target", "accepted", "rejected", "failing", "coverage_pct", "stop")})
        lg = pathlib.Path(st["run_dir"]) / "loop.log"
        if lg.exists():
            print("".join(lg.read_text().splitlines(True)[-5:]))
        return 0
    if a.cmd == "dry-propose":
        return dry_propose(cfg, a.arg, pathlib.Path(a.out) if a.out else None)
    if a.cmd == "summarize":
        summarize(pathlib.Path(a.arg), cfg)
        print(pathlib.Path(a.arg) / "SUMMARY.md")
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
