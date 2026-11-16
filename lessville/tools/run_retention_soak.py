#!/usr/bin/env python3
"""Multi-day retention soak: drive a fresh private LessVille backend through N
simulated nights with /api/control step, then report hot-table row counts,
retention events and step latency. Regression harness for the 2026-11-14
unbounded hot-table growth outage.

usage: run_retention_soak.py --binary PATH [--days 5] [--port 8792]
"""
import argparse, json, os, shutil, subprocess, sys, tempfile, time, urllib.request

def call(base, path, body=None):
    req = urllib.request.Request(base + path, method="POST" if body else "GET",
                                 data=json.dumps(body).encode() if body else None,
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--days", type=int, default=5)
    ap.add_argument("--port", type=int, default=8792)
    a = ap.parse_args()
    data = tempfile.mkdtemp(prefix="lv-soak-")
    env = dict(os.environ, LESSVILLE_DATA=data, LESSVILLE_PORT=str(a.port), LESSVILLE_PUBLIC="0",
               LESSVILLE_LIVE_WEATHER="0", LESSVILLE_CHECKPOINT_SECS="999999")
    env.pop("LESSVILLE_CKPT_URL", None)
    env.pop("LESSVILLE_LLM_URL", None)
    p = subprocess.Popen([a.binary], env=env, stdout=open(data + ".log", "w"), stderr=subprocess.STDOUT)
    base = f"http://127.0.0.1:{a.port}"
    ok = False
    try:
        for _ in range(60):
            try:
                call(base, "/api/health"); break
            except Exception:
                time.sleep(0.5)
        call(base, "/api/control", {"action": "pause"})
        start_day = call(base, "/api/state")["clock"]["day"]
        slow = []
        steps = 0
        while call(base, "/api/state")["clock"]["day"] < start_day + a.days:
            t = time.time()
            call(base, "/api/control", {"action": "step"})
            dt = time.time() - t
            slow.append(dt)
            steps += 1
            if steps % 186 == 0:
                st = call(base, "/api/state")["clock"]
                print(f"day {st['day']} {st['label']} steps={steps} p95_step={sorted(slow[-186:])[int(0.95*len(slow[-186:]))-1]:.3f}s", flush=True)
        tables = call(base, "/api/tables")
        print("tables:", json.dumps(tables)[:1500])
        log = open(data + ".log").read()
        ret = [l for l in log.splitlines() if "retention" in l and "failed" in l]
        print("retention failure lines:", len(ret))
        first = sorted(slow[:186]); last = sorted(slow[-186:])
        print(f"p95 step first day {first[int(.95*len(first))-1]:.3f}s last day {last[int(.95*len(last))-1]:.3f}s")
        ok = True
    finally:
        p.terminate(); p.wait(timeout=20)
        shutil.rmtree(data, ignore_errors=True)
    sys.exit(0 if ok else 1)

if __name__ == "__main__":
    main()
