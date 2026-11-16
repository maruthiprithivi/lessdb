#!/usr/bin/env python3
"""Expose the official MicroDuck body-server camera plus bounded daemon/LessDB state."""
from __future__ import annotations

import argparse
import json
import socket
import struct
import threading
import urllib.error
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import numpy as np
from PIL import Image
from io import BytesIO

WIDTH, HEIGHT = 640, 360


class TwinState:
    def __init__(self, camera_host: str, camera_port: int, body_host: str, body_port: int, lessdb_url: str):
        self.camera_host = camera_host
        self.camera_port = camera_port
        self.body_host = body_host
        self.body_port = body_port
        self.lessdb_url = lessdb_url.rstrip("/")
        self.lock = threading.Lock()
        self.jpeg: bytes | None = None
        self.frame_count = 0
        self.last_frame_at: float | None = None
        self.body: dict = {}
        self.persisted: dict = {}
        self.stop = threading.Event()

    def read_exact(self, stream, count: int) -> bytes:
        chunks = []
        remaining = count
        while remaining:
            part = stream.recv(remaining)
            if not part:
                raise ConnectionError("camera stream closed")
            chunks.append(part)
            remaining -= len(part)
        return b"".join(chunks)

    def camera_loop(self) -> None:
        while not self.stop.is_set():
            try:
                with socket.create_connection((self.camera_host, self.camera_port), timeout=5) as sock:
                    while not self.stop.is_set():
                        size = struct.unpack("<I", self.read_exact(sock, 4))[0]
                        if size != WIDTH * HEIGHT * 2:
                            raise ValueError(f"unexpected UYVY frame size: {size}")
                        packed = np.frombuffer(self.read_exact(sock, size), dtype=np.uint8).reshape(HEIGHT, WIDTH // 2, 4)
                        u = packed[:, :, 0].astype(np.float32)
                        y0 = packed[:, :, 1].astype(np.float32)
                        v = packed[:, :, 2].astype(np.float32)
                        y1 = packed[:, :, 3].astype(np.float32)
                        y = np.stack((y0, y1), axis=2)
                        u = np.repeat(u[:, :, None], 2, axis=2)
                        v = np.repeat(v[:, :, None], 2, axis=2)
                        r = y + 1.402 * (v - 128)
                        g = y - 0.344136 * (u - 128) - 0.714136 * (v - 128)
                        b = y + 1.772 * (u - 128)
                        rgb = np.clip(np.stack((r, g, b), axis=3), 0, 255).astype(np.uint8).reshape(HEIGHT, WIDTH, 3)
                        image = Image.fromarray(rgb, "RGB")
                        output = BytesIO()
                        image.save(output, format="JPEG", quality=82, optimize=True)
                        with self.lock:
                            self.jpeg = output.getvalue()
                            self.frame_count += 1
                            self.last_frame_at = time.time()
            except (OSError, ConnectionError, ValueError):
                time.sleep(0.5)

    def body_loop(self) -> None:
        while not self.stop.is_set():
            try:
                with socket.create_connection((self.body_host, self.body_port), timeout=3) as sock:
                    stream = sock.makefile("rw")
                    stream.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n")
                    stream.flush()
                    stream.readline()
                    while not self.stop.is_set():
                        stream.write('{"op":"read"}\n')
                        stream.flush()
                        line = stream.readline()
                        if not line:
                            raise ConnectionError("body stream closed")
                        with self.lock:
                            self.body = json.loads(line)
                        time.sleep(0.2)
            except (OSError, ConnectionError, json.JSONDecodeError):
                time.sleep(0.5)

    def lessdb_loop(self) -> None:
        while not self.stop.is_set():
            try:
                with urllib.request.urlopen(self.lessdb_url + "/api/physical/summary", timeout=5) as response:
                    value = json.load(response)
                with self.lock:
                    self.persisted = value.get("persisted", {})
            except (OSError, ValueError, urllib.error.URLError):
                pass
            self.stop.wait(2.0)

    def snapshot(self) -> dict:
        with self.lock:
            body = self.body.get("state", self.body)
            return {
                "source": "official MicroDuck body_server camera + robotd --sim",
                "camera": {
                    "connected": self.jpeg is not None,
                    "frame_count": self.frame_count,
                    "last_frame_age_ms": None if self.last_frame_at is None else round((time.time() - self.last_frame_at) * 1000),
                },
                "body": {
                    "sim_time": body.get("sim_time"),
                    "trunk_z": body.get("trunk_z"),
                    "imu": body.get("imu"),
                },
                "lessdb": {
                    "persisted": self.persisted,
                    "boundary": "LessDB stores sampled provenance; robotd/MuJoCo own control and safety.",
                },
            }


HTML = """<!doctype html>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>MicroDuck · official robotd simulation + LessDB</title>
<style>
:root{color-scheme:dark;--bg:#081016;--panel:#101c24;--line:#28424f;--accent:#70e1c0;--muted:#9db4be}
*{box-sizing:border-box}body{margin:0;background:linear-gradient(145deg,#071017,#122632);font:15px system-ui,sans-serif;color:#eef8f5}
main{max-width:1180px;margin:auto;padding:28px 20px}header{display:flex;justify-content:space-between;gap:18px;align-items:end;margin-bottom:18px}h1{margin:0;font-size:clamp(22px,4vw,38px)}p{color:var(--muted);line-height:1.5}.badge{border:1px solid #356b61;color:var(--accent);padding:7px 10px;border-radius:999px;white-space:nowrap}.grid{display:grid;grid-template-columns:minmax(0,2fr) minmax(270px,1fr);gap:18px}.stage,.panel{background:rgba(16,28,36,.9);border:1px solid var(--line);border-radius:16px;box-shadow:0 18px 50px #0005}.stage{padding:14px}.stage img{display:block;width:100%;aspect-ratio:16/9;object-fit:cover;border-radius:10px;background:#05090c;image-rendering:auto}.panel{padding:18px}.panel h2{font-size:16px;margin:0 0 14px}.stats{display:grid;grid-template-columns:1fr 1fr;gap:10px}.stat{padding:11px;border:1px solid #223945;border-radius:10px}.label{font-size:11px;color:var(--muted);text-transform:uppercase;letter-spacing:.08em}.value{font-size:20px;margin-top:3px;color:var(--accent)}pre{font-size:11px;white-space:pre-wrap;word-break:break-word;color:#b8d0d5;background:#091219;padding:10px;border-radius:9px}.note{font-size:12px;border-left:3px solid var(--accent);padding-left:10px;color:#bfd2d5}@media(max-width:760px){header{display:block}.badge{display:inline-block;margin-top:12px}.grid{grid-template-columns:1fr}main{padding:16px 12px}}
</style>
<main><header><div><h1>MicroDuck · real simulator view</h1><p>The image is the official MuJoCo body-server camera driven by the official <code>robotd --sim</code> loop. The side panel reports sampled provenance persisted in embedded LessDB.</p></div><div class="badge" id="status">connecting…</div></header>
<section class="grid"><div class="stage"><img id="frame" src="/frame.jpg" alt="Live camera frame from the official MicroDuck MuJoCo simulator"><p class="note">This is a simulator, not hardware. LessDB is not in the 50 Hz control loop and does not provide robot safety interlocks.</p></div><aside class="panel"><h2>Daemon + LessDB state</h2><div class="stats"><div class="stat"><div class="label">Frames</div><div class="value" id="frames">—</div></div><div class="stat"><div class="label">Sim time</div><div class="value" id="sim">—</div></div><div class="stat"><div class="label">Observations</div><div class="value" id="obs">—</div></div><div class="stat"><div class="label">Reconciled</div><div class="value" id="rec">—</div></div></div><pre id="detail">loading…</pre></aside></section></main>
<script>
const frame=document.querySelector('#frame'); const status=document.querySelector('#status');
async function refresh(){try{const s=await (await fetch('/state.json',{cache:'no-store'})).json();const c=s.camera||{},b=s.body||{},p=s.lessdb?.persisted||{};frame.src='/frame.jpg?t='+Date.now();document.querySelector('#frames').textContent=c.frame_count??'—';document.querySelector('#sim').textContent=b.sim_time==null?'—':Number(b.sim_time).toFixed(1)+' s';document.querySelector('#obs').textContent=p.observations??'—';document.querySelector('#rec').textContent=p.reconciliations??'—';status.textContent=c.connected?'LIVE OFFICIAL CAMERA':'WAITING FOR CAMERA';document.querySelector('#detail').textContent=JSON.stringify({source:s.source,camera:c,body:b,lessdb:s.lessdb},null,2)}catch(e){status.textContent='RETRYING';}} setInterval(refresh,1000); refresh();
</script>
"""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bind", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8797)
    parser.add_argument("--camera-port", type=int, default=7901)
    parser.add_argument("--body-port", type=int, default=7810)
    parser.add_argument("--lessdb-url", default="http://127.0.0.1:8792")
    args = parser.parse_args()
    state = TwinState("127.0.0.1", args.camera_port, "127.0.0.1", args.body_port, args.lessdb_url)
    for target in (state.camera_loop, state.body_loop, state.lessdb_loop):
        threading.Thread(target=target, daemon=True).start()

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):  # noqa: N802
            if self.path.startswith("/frame.jpg"):
                with state.lock:
                    payload = state.jpeg
                if payload is None:
                    self.send_response(503)
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header("content-type", "image/jpeg")
                self.send_header("cache-control", "no-store")
                self.end_headers()
                self.wfile.write(payload)
                return
            if self.path == "/state.json":
                payload = json.dumps(state.snapshot(), separators=(",", ":")).encode()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.send_header("cache-control", "no-store")
                self.end_headers()
                self.wfile.write(payload)
                return
            if self.path == "/health":
                payload = json.dumps({"ok": True, "camera": state.jpeg is not None}).encode()
                self.send_response(200)
                self.send_header("content-type", "application/json")
                self.end_headers()
                self.wfile.write(payload)
                return
            payload = HTML.encode()
            self.send_response(200)
            self.send_header("content-type", "text/html; charset=utf-8")
            self.send_header("cache-control", "no-store")
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *_args):
            return

    server = ThreadingHTTPServer((args.bind, args.port), Handler)
    try:
        server.serve_forever()
    finally:
        state.stop.set()
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
