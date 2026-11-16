#!/usr/bin/env python3
"""Serve a browser-visible MuJoCo physical twin with a LessDB context sidecar.

This uses an official simulator model supplied by the installed Reachy Mini SDK.
It is a visual demonstration layer, not the robot's real-time controller.
"""

from __future__ import annotations

import argparse
import io
import json
import math
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

import mujoco
from PIL import Image


HTML = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>LessVille Physical District — Reachy Mini</title>
<style>
:root{--bg:#08100d;--panel:#122019;--panel2:#1b2d22;--line:#35543f;--ink:#edf6ee;--muted:#9db1a2;--green:#a7ed75;--cyan:#75ddd4;--amber:#f4c86d;--red:#ff9a83}
*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 20% 0,#193525 0,#08100d 46%);color:var(--ink);font:14px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace}.wrap{max-width:1280px;margin:auto;padding:20px 16px 48px}header{display:flex;justify-content:space-between;gap:20px;align-items:flex-start;border-bottom:1px solid var(--line);padding-bottom:18px}.kicker{text-transform:uppercase;letter-spacing:.16em;font-size:10px;color:var(--green)}h1{font-size:clamp(21px,4vw,36px);letter-spacing:-.05em;margin:5px 0}.muted{color:var(--muted)}.status{border:1px solid #617c48;color:var(--green);border-radius:999px;padding:7px 11px;font-size:11px;white-space:nowrap}.layout{display:grid;grid-template-columns:minmax(0,1.45fr) minmax(300px,.75fr);gap:16px;margin-top:16px}.panel{background:color-mix(in srgb,var(--panel) 95%,transparent);border:1px solid var(--line);border-radius:12px;padding:14px;box-shadow:0 10px 32px #0005}.visual{padding:0;overflow:hidden}.visual-head{display:flex;justify-content:space-between;padding:12px 14px;border-bottom:1px solid var(--line);background:var(--panel2)}.visual img{display:block;width:100%;aspect-ratio:4/3;object-fit:contain;background:#20364d}.caption{padding:10px 14px;color:var(--muted);font-size:11px;border-top:1px solid var(--line)}h2{font-size:12px;text-transform:uppercase;letter-spacing:.12em;color:var(--cyan);margin:0 0 12px}.grid{display:grid;grid-template-columns:repeat(2,1fr);gap:8px}.metric{background:var(--panel2);border:1px solid var(--line);border-radius:8px;padding:9px}.metric b{display:block;font-size:16px;color:var(--green)}.metric span{font-size:10px;color:var(--muted)}.row{border-top:1px solid var(--line);padding:10px 0}.row:first-child{border-top:0;padding-top:0}.label{font-size:10px;color:var(--muted);text-transform:uppercase;letter-spacing:.08em}.value{margin-top:3px;overflow-wrap:anywhere}.chip{display:inline-block;border-radius:999px;padding:3px 7px;font-size:10px;text-transform:uppercase;letter-spacing:.08em}.live{background:#21482a;color:var(--green)}.draft{background:#4c3c1b;color:var(--amber)}.safe{color:var(--green)}.boundary{border-left:3px solid var(--amber);background:#2c2518;color:#eadbb0;padding:11px;font-size:11px;margin-top:12px}.flow{display:grid;gap:7px}.step{display:grid;grid-template-columns:28px 1fr;gap:8px;padding:9px;border:1px solid var(--line);border-radius:8px;background:var(--panel2)}.step strong{display:block}.step small{color:var(--muted)}.num{color:var(--cyan)}code{color:var(--cyan)}@media(max-width:800px){header{display:block}.status{display:inline-block;margin-top:10px}.layout{display:block}.side{margin-top:16px}}
</style></head>
<body><main class="wrap"><header><div><div class="kicker">LessVille physical district / live visual twin</div><h1>Reachy Mini <span class="muted">· MuJoCo</span></h1><p class="muted">A real 3D robot model moves on the left. LessDB’s bounded situation, proposal and evidence lineage stays visible on the right.</p></div><div class="status"><span id="run-status">CONNECTING</span></div></header>
<div class="layout"><section><div class="panel visual"><div class="visual-head"><strong>Official Reachy Mini model</strong><span id="phase" class="muted">loading phase</span></div><img id="frame" src="/frame.png" alt="Live Reachy Mini MuJoCo render"><div class="caption">The simulator owns physics and articulation. LessDB is outside the control loop and stores the sampled context and outcome.</div></div></section><aside class="side"><section class="panel"><h2>Current situation</h2><div class="grid"><div class="metric"><b id="episode">—</b><span>episode</span></div><div class="metric"><b id="sequence">—</b><span>observation sequence</span></div><div class="metric"><b id="obs-kind">—</b><span>observation</span></div><div class="metric"><b id="recon">—</b><span>reconciliation</span></div></div></section><section class="panel" style="margin-top:16px"><h2>Agent-to-world loop</h2><div class="flow"><div class="step"><div class="num">01</div><div><strong>Situation</strong><small>current pose, phase, simulator time</small></div></div><div class="step"><div class="num">02</div><div><strong>Evidence</strong><small id="evidence">waiting for hash</small></div></div><div class="step"><div class="num">03</div><div><strong>Proposal</strong><small id="proposal">waiting for proposal</small></div></div><div class="step"><div class="num">04</div><div><strong>Reconciliation</strong><small id="reconciliation">waiting for executor result</small></div></div></div><div class="boundary"><strong>Boundary:</strong> this is an observability twin. It does not claim native safety governance, sim-to-real transfer or direct motor control.</div></section></aside></div></main>
<script>
const frame=document.querySelector('#frame');let n=0;setInterval(()=>{frame.src='/frame.png?t='+Date.now()},120);
async function refresh(){try{const s=await fetch('/state.json?t='+Date.now()).then(r=>r.json());document.querySelector('#run-status').textContent='SIMULATION RUNNING';document.querySelector('#phase').textContent=s.phase;document.querySelector('#episode').textContent=s.episode_id;document.querySelector('#sequence').textContent=s.sequence;document.querySelector('#obs-kind').textContent=s.observation_kind;document.querySelector('#recon').textContent=s.reconciliation;document.querySelector('#evidence').textContent=s.evidence;document.querySelector('#proposal').textContent=s.proposal;document.querySelector('#reconciliation').textContent=s.reconciliation_detail}catch(e){document.querySelector('#run-status').textContent='RENDER ERROR'}}setInterval(refresh,400);refresh();
</script></body></html>"""


class Twin:
    def __init__(self, model_path: Path, episode_id: str):
        self.model = mujoco.MjModel.from_xml_path(str(model_path))
        self.data = mujoco.MjData(self.model)
        # The packaged Reachy Mini scene declares a 640x480 offscreen buffer.
        # Keep the browser viewport responsive while rendering at that native size.
        # The renderer is created in the animation thread below because EGL
        # contexts are thread-affine.
        self.renderer: mujoco.Renderer | None = None
        self.camera = mujoco.MjvCamera()
        self.camera.type = mujoco.mjtCamera.mjCAMERA_FREE
        self.camera.lookat[:] = [0, 0, 0.14]
        self.camera.distance = 0.65
        self.camera.azimuth = 0
        self.camera.elevation = -12
        self.episode_id = episode_id
        self.lock = threading.Lock()
        self.frame = b""
        self.state: dict[str, Any] = {"episode_id": episode_id, "phase": "booting"}
        self.started = time.monotonic()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self) -> None:
        self.renderer = mujoco.Renderer(self.model, height=480, width=640)
        phases = ["observe", "look-left", "look-right", "settle"]
        while True:
            now = time.monotonic() - self.started
            phase_index = int(now // 3.0) % len(phases)
            phase = phases[phase_index]
            phase_t = now % 3.0
            wave = math.sin(phase_t * math.pi / 3.0)
            if phase == "look-left":
                head = 0.14 * wave
            elif phase == "look-right":
                head = -0.14 * wave
            else:
                head = 0.04 * math.sin(now * 1.2)
            # These six actuators are the official Reachy Mini Stewart platform.
            for index in range(1, 7):
                self.data.ctrl[index] = max(-0.25, min(0.25, head * (1 if index % 2 else -1)))
            self.data.ctrl[0] = 0.12 * math.sin(now * 0.45)
            mujoco.mj_step(self.model, self.data)
            # The antenna joints are direct pose targets in this demonstration.
            self.data.qpos[35] = 0.55 * math.sin(now * 1.4)
            self.data.qpos[36] = 0.55 * math.sin(now * 1.4 + math.pi)
            mujoco.mj_forward(self.model, self.data)
            self.renderer.update_scene(self.data, camera=self.camera)
            image = Image.fromarray(self.renderer.render()).convert("RGB")
            buffer = io.BytesIO()
            image.save(buffer, format="JPEG", quality=82)
            evidence = f"sha256:render-seq-{int(now * 10)}"
            state = {
                "episode_id": self.episode_id,
                "phase": phase,
                "sequence": int(now * 10),
                "observation_kind": "head_pose_observed",
                "evidence": evidence,
                "proposal": f"draft:{'look_at' if phase != 'settle' else 'move_to_safe_pose'}",
                "reconciliation": "succeeded",
                "reconciliation_detail": "sampled visual twin result; executor outside renderer",
            }
            with self.lock:
                self.frame = buffer.getvalue()
                self.state = state
            time.sleep(0.075)

    def snapshot(self) -> tuple[bytes, dict[str, Any]]:
        with self.lock:
            return self.frame, dict(self.state)


class Handler(BaseHTTPRequestHandler):
    twin: Twin

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        if path == "/":
            body = HTML.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
        elif path == "/frame.png":
            body, _ = self.twin.snapshot()
            self.send_response(200)
            self.send_header("Content-Type", "image/jpeg")
            self.send_header("Cache-Control", "no-store")
        elif path == "/state.json":
            _, state = self.twin.snapshot()
            body = json.dumps(state).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Cache-Control", "no-store")
        elif path == "/health":
            body = b'{"ok":true,"renderer":"mujoco","boundary":"visual-twin"}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
        else:
            self.send_error(404)
            return
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: Any) -> None:
        return


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-root", type=Path, required=True, help="Reachy Mini mjcf directory")
    parser.add_argument("--episode-id", default="episode:reachy-mini-visual")
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8795)
    args = parser.parse_args()
    twin = Twin(args.model_root / "scene.xml", args.episode_id)
    Handler.twin = twin
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(json.dumps({"ok": True, "renderer": "mujoco", "port": args.port, "episode_id": args.episode_id}), flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
