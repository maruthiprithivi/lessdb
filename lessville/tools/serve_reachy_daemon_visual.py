#!/usr/bin/env python3
"""Serve an observer-only Reachy Mini view backed by the official simulated daemon.

The local demo driver sends bounded semantic head targets through the official SDK;
the renderer reads the daemon pose and mirrors it onto the official MuJoCo model.
LessDB remains a sampled evidence/context sidecar, never the real-time controller.
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

HTML = r'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>LessVille Embodied Lab · Reachy Mini daemon</title>
<style>
:root{color-scheme:dark;--bg:#07110e;--panel:#122019;--panel2:#1b3024;--line:#355943;--ink:#eef8ef;--muted:#a6bca9;--green:#a7ed75;--cyan:#75ddd4;--amber:#f4c86d;--red:#ff9a83}*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 15% 0,#214b36,#07110e 50%);color:var(--ink);font:14px/1.45 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}.wrap{max-width:1320px;margin:auto;padding:22px 16px 48px}header{display:flex;justify-content:space-between;gap:20px;align-items:flex-start;border-bottom:1px solid var(--line);padding-bottom:18px}.eyebrow{text-transform:uppercase;letter-spacing:.16em;font-size:10px;color:var(--green)}h1{font-size:clamp(25px,5vw,46px);line-height:1;letter-spacing:-.055em;margin:7px 0 12px}h1 span{color:var(--muted);font-weight:400}p{color:var(--muted);line-height:1.55}.status{border:1px solid #648c55;color:var(--green);border-radius:999px;padding:8px 12px;white-space:nowrap;font-size:11px}.layout{display:grid;grid-template-columns:minmax(0,1.5fr) minmax(310px,.8fr);gap:17px;margin-top:18px}.panel{background:#122019ee;border:1px solid var(--line);border-radius:14px;padding:15px;box-shadow:0 16px 46px #0005}.visual{padding:0;overflow:hidden}.visual-head{display:flex;justify-content:space-between;gap:10px;align-items:center;padding:13px 15px;background:var(--panel2);border-bottom:1px solid var(--line)}.visual img{display:block;width:100%;aspect-ratio:4/3;object-fit:contain;background:#20364d}.caption{padding:12px 15px;border-top:1px solid var(--line);color:var(--muted);font-size:12px}.side{display:grid;gap:17px;align-content:start}h2{font-size:12px;text-transform:uppercase;letter-spacing:.13em;color:var(--cyan);margin:0 0 12px}.metrics{display:grid;grid-template-columns:1fr 1fr;gap:8px}.metric{background:var(--panel2);border:1px solid var(--line);border-radius:9px;padding:9px;min-width:0}.metric b{display:block;font-size:16px;color:var(--green);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.metric span{display:block;color:var(--muted);font-size:10px;margin-top:3px}.flow{display:grid;gap:7px}.step{display:grid;grid-template-columns:28px 1fr;gap:8px;padding:9px;border:1px solid var(--line);border-radius:8px;background:var(--panel2)}.step strong{display:block}.step small{color:var(--muted)}.num{color:var(--cyan)}.boundary{border-left:3px solid var(--amber);background:#302919;color:#ebdbb5;padding:11px;font-size:11px}.detail{white-space:pre-wrap;word-break:break-word;background:#081411;border-radius:8px;padding:10px;color:#b8d4c8;font:10px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace;max-height:240px;overflow:auto}@media(max-width:800px){header{display:block}.status{display:inline-block;margin-top:12px}.layout{display:block}.side{margin-top:17px}}
</style></head><body><main class="wrap"><header><div><div class="eyebrow">LessVille / embodied agent research</div><h1>Reachy Mini <span>— official daemon-backed view</span></h1><p>This page is driven by the official Reachy Mini MuJoCo daemon through its local SDK. The visible model mirrors daemon readback; LessDB stores only bounded evidence and lineage around the run.</p></div><div class="status" id="status">CONNECTING</div></header><section class="layout"><section><div class="panel visual"><div class="visual-head"><strong>Official Reachy Mini model</strong><span id="phase">waiting for daemon</span></div><img id="frame" src="/frame.png" alt="Reachy Mini model driven by the official simulated daemon"><div class="caption">The local driver sends only bounded semantic demo targets. Physics, articulation, timing and safety remain owned by the official simulator daemon. This public-facing page exposes read-only output only.</div></div></section><aside class="side"><section class="panel"><h2>Daemon and evidence</h2><div class="metrics"><div class="metric"><b id="daemon">—</b><span>official SDK daemon</span></div><div class="metric"><b id="episode">—</b><span>episode</span></div><div class="metric"><b id="sequence">—</b><span>readback sequence</span></div><div class="metric"><b id="lessdb">—</b><span>LessDB sidecar</span></div></div></section><section class="panel"><h2>One bounded loop</h2><div class="flow"><div class="step"><div class="num">01</div><div><strong>Semantic target</strong><small id="target">waiting for SDK driver</small></div></div><div class="step"><div class="num">02</div><div><strong>Daemon readback</strong><small id="pose">waiting for pose</small></div></div><div class="step"><div class="num">03</div><div><strong>Evidence</strong><small id="evidence">waiting for evidence</small></div></div><div class="step"><div class="num">04</div><div><strong>Reconciliation</strong><small id="reconciliation">waiting for result</small></div></div></div></section><section class="panel"><h2>Truth boundary</h2><div class="boundary">Reachy’s daemon and simulator own movement and safety. LessDB is not an emergency stop, actuator controller, native physical-ACL engine or sim-to-real certificate. The staging hub combines this source with MicroDuck views without merging their physics.</div><div class="detail" id="detail">Loading daemon state…</div></section></aside></section></main><script>
const frame=document.querySelector('#frame');setInterval(()=>frame.src='/frame.png?t='+Date.now(),160);async function refresh(){try{const s=await fetch('/state.json?t='+Date.now()).then(r=>r.json());document.querySelector('#status').textContent=s.daemon_connected?'DAEMON CONNECTED':'DAEMON WAITING';document.querySelector('#phase').textContent=s.phase;document.querySelector('#daemon').textContent=s.daemon_connected?'official sim':'offline';document.querySelector('#episode').textContent=s.episode_id;document.querySelector('#sequence').textContent=s.sequence;document.querySelector('#lessdb').textContent=s.lessdb;document.querySelector('#target').textContent=s.target;document.querySelector('#pose').textContent=s.pose;document.querySelector('#evidence').textContent=s.evidence;document.querySelector('#reconciliation').textContent=s.reconciliation;document.querySelector('#detail').textContent=JSON.stringify(s,null,2)}catch(e){document.querySelector('#status').textContent='RETRYING'}}setInterval(refresh,450);refresh();</script></body></html>'''


def euler_from_matrix(matrix: list[list[float]]) -> tuple[float, float, float, tuple[float, float, float]]:
    roll = math.atan2(matrix[2][1], matrix[2][2])
    pitch = math.atan2(-matrix[2][0], math.sqrt(matrix[2][1] ** 2 + matrix[2][2] ** 2))
    yaw = math.atan2(matrix[1][0], matrix[0][0])
    return roll, pitch, yaw, (float(matrix[0][3]), float(matrix[1][3]), float(matrix[2][3]))


class Twin:
    def __init__(self, model_path: Path, episode_id: str, drive_demo: bool, lessdb_url: str | None):
        self.model = mujoco.MjModel.from_xml_path(str(model_path))
        self.data = mujoco.MjData(self.model)
        self.renderer: mujoco.Renderer | None = None
        self.camera = mujoco.MjvCamera()
        self.camera.type = mujoco.mjtCamera.mjCAMERA_FREE
        self.camera.lookat[:] = [0, 0, 0.14]
        self.camera.distance = 0.65
        self.camera.azimuth = 0
        self.camera.elevation = -12
        self.episode_id = episode_id
        self.drive_demo = drive_demo
        self.lessdb_url = lessdb_url.rstrip('/') if lessdb_url else None
        self.lock = threading.Lock()
        self.frame = b''
        self.pose_matrix: list[list[float]] | None = None
        self.state: dict[str, Any] = {'episode_id': episode_id, 'phase': 'booting', 'daemon_connected': False}
        self.started = time.monotonic()
        self.reader = threading.Thread(target=self.sdk_loop, daemon=True)
        self.renderer_thread = threading.Thread(target=self.render_loop, daemon=True)
        self.reader.start()
        self.renderer_thread.start()

    def sdk_loop(self) -> None:
        try:
            from reachy_mini import ReachyMini
            from reachy_mini.utils import create_head_pose
            with ReachyMini(connection_mode='localhost_only', media_backend='no_media') as mini:
                targets = [(10, 5, 'look-left'), (-10, -5, 'look-right'), (0, 0, 'safe-pose')]
                index = 0
                next_target = 0.0
                while True:
                    now = time.monotonic() - self.started
                    if self.drive_demo and now >= next_target:
                        z, roll, label = targets[index % len(targets)]
                        mini.goto_target(head=create_head_pose(z=z, roll=roll, mm=True, degrees=True), antennas=[0.35, -0.35] if index % 2 == 0 else [0, 0], duration=0.7)
                        index += 1
                        next_target = now + 3.2
                    matrix = mini.get_current_head_pose().tolist()
                    with self.lock:
                        self.pose_matrix = matrix
                        self.state.update({'daemon_connected': True, 'target': targets[(index - 1) % len(targets)][2] if index else 'observe-only', 'pose': json.dumps(matrix[0][3:4] + matrix[1][3:4] + matrix[2][3:4], separators=(',', ':'))})
                    time.sleep(0.12)
        except Exception as exc:
            with self.lock:
                self.state.update({'daemon_connected': False, 'daemon_error': type(exc).__name__})
            while True:
                time.sleep(1.0)

    def lessdb_status(self) -> str:
        if not self.lessdb_url:
            return 'not connected'
        try:
            from urllib.request import urlopen
            with urlopen(self.lessdb_url + '/api/physical/summary', timeout=0.8) as response:
                payload = json.loads(response.read())
            persisted = payload.get('persisted', {})
            return f"obs {int(persisted.get('observations', 0))} · prop {int(persisted.get('proposals', 0))}"
        except Exception:
            return 'sidecar waiting'

    def render_loop(self) -> None:
        self.renderer = mujoco.Renderer(self.model, height=480, width=640)
        while True:
            with self.lock:
                matrix = self.pose_matrix
                prior = dict(self.state)
            now = time.monotonic() - self.started
            roll = pitch = yaw = 0.0
            translation = (0.0, 0.0, 0.0)
            if matrix:
                roll, pitch, yaw, translation = euler_from_matrix(matrix)
            for index in range(1, 7):
                self.data.ctrl[index] = max(-0.25, min(0.25, pitch * (1 if index % 2 else -1) + roll * 0.35))
            self.data.ctrl[0] = max(-0.2, min(0.2, yaw * 0.25))
            if self.model.nq > 36:
                self.data.qpos[35] = max(-0.55, min(0.55, roll * 2.2))
                self.data.qpos[36] = max(-0.55, min(0.55, -roll * 2.2))
            mujoco.mj_step(self.model, self.data)
            self.renderer.update_scene(self.data, camera=self.camera)
            image = Image.fromarray(self.renderer.render()).convert('RGB')
            buffer = io.BytesIO(); image.save(buffer, format='JPEG', quality=84)
            sequence = int(now * 8)
            state = {
                **prior,
                'episode_id': self.episode_id,
                'phase': prior.get('target', 'observe'),
                'sequence': sequence,
                'evidence': f'sha256:daemon-pose-{sequence}',
                'reconciliation': 'sampled SDK readback; no public actuation endpoint',
                'lessdb': self.lessdb_status(),
                'pose': 'x=%.4fm y=%.4fm z=%.4fm' % translation,
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
    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path == '/':
            body, content_type = HTML.encode(), 'text/html; charset=utf-8'
        elif path == '/frame.png':
            body, content_type = self.twin.snapshot()[0], 'image/jpeg'
        elif path == '/state.json':
            body, content_type = json.dumps(self.twin.snapshot()[1]).encode(), 'application/json'
        elif path == '/health':
            body, content_type = json.dumps({'ok': True, 'service': 'reachy-mini-daemon-visual', 'observer_only': True}).encode(), 'application/json'
        else:
            self.send_error(404); return
        self.send_response(200); self.send_header('Content-Type', content_type); self.send_header('Cache-Control', 'no-store'); self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *_args: Any) -> None:
        return


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model-root', type=Path, required=True)
    parser.add_argument('--episode-id', default='episode:reachy-mini-daemon-staging')
    parser.add_argument('--lessdb-url')
    parser.add_argument('--no-drive-demo', action='store_true')
    parser.add_argument('--host', default='0.0.0.0')
    parser.add_argument('--port', type=int, default=8804)
    args = parser.parse_args()
    twin = Twin(args.model_root / 'scene.xml', args.episode_id, not args.no_drive_demo, args.lessdb_url)
    Handler.twin = twin
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(json.dumps({'ok': True, 'service': 'reachy-mini-daemon-visual', 'port': args.port, 'observer_only': True}), flush=True)
    server.serve_forever()


if __name__ == '__main__':
    main()
