#!/usr/bin/env python3
"""Show an external official MicroDuck model driven by robotd state plus its real camera view."""
from __future__ import annotations

import argparse
import json
import socket
import struct
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from io import BytesIO
from pathlib import Path

import mujoco
import numpy as np
from PIL import Image

WIDTH, HEIGHT = 640, 360


def uyvy_to_jpeg(packed: bytes) -> bytes:
    values = np.frombuffer(packed, dtype=np.uint8).reshape(HEIGHT, WIDTH // 2, 4)
    u = values[:, :, 0].astype(np.float32)
    y0 = values[:, :, 1].astype(np.float32)
    v = values[:, :, 2].astype(np.float32)
    y1 = values[:, :, 3].astype(np.float32)
    y = np.stack((y0, y1), axis=2)
    u = np.repeat(u[:, :, None], 2, axis=2)
    v = np.repeat(v[:, :, None], 2, axis=2)
    rgb = np.stack(
        (
            y + 1.402 * (v - 128),
            y - 0.344136 * (u - 128) - 0.714136 * (v - 128),
            y + 1.772 * (u - 128),
        ),
        axis=3,
    )
    image = Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8).reshape(HEIGHT, WIDTH, 3), "RGB")
    output = BytesIO()
    image.save(output, format="JPEG", quality=82, optimize=True)
    return output.getvalue()


class Twin:
    def __init__(self, robot_root: Path, camera_port: int, body_port: int, lessdb_url: str):
        self.robot_root = robot_root
        self.camera_port = camera_port
        self.body_port = body_port
        self.lessdb_url = lessdb_url.rstrip("/")
        self.lock = threading.Lock()
        self.body: dict = {}
        self.first_person: bytes | None = None
        self.external: bytes | None = None
        self.frame_count = 0
        self.render_count = 0
        self.persisted: dict = {}
        self.external_error: str | None = None
        self.stop = threading.Event()

    @staticmethod
    def read_exact(sock: socket.socket, count: int) -> bytes:
        chunks: list[bytes] = []
        while count:
            part = sock.recv(count)
            if not part:
                raise ConnectionError("stream closed")
            chunks.append(part)
            count -= len(part)
        return b"".join(chunks)

    def camera_loop(self) -> None:
        while not self.stop.is_set():
            try:
                with socket.create_connection(("127.0.0.1", self.camera_port), timeout=5) as sock:
                    while not self.stop.is_set():
                        size = struct.unpack("<I", self.read_exact(sock, 4))[0]
                        if size != WIDTH * HEIGHT * 2:
                            raise ValueError(f"unexpected UYVY size {size}")
                        image = uyvy_to_jpeg(self.read_exact(sock, size))
                        with self.lock:
                            self.first_person = image
                            self.frame_count += 1
            except (OSError, ConnectionError, ValueError):
                time.sleep(0.5)

    def body_loop(self) -> None:
        while not self.stop.is_set():
            try:
                with socket.create_connection(("127.0.0.1", self.body_port), timeout=3) as sock:
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
                        time.sleep(0.05)
            except (OSError, ConnectionError, json.JSONDecodeError):
                time.sleep(0.5)

    def _external_loop(self) -> None:
        model = mujoco.MjModel.from_xml_path(str(self.robot_root / "scene.xml"))
        data = mujoco.MjData(model)
        renderer = mujoco.Renderer(model, height=480, width=640)
        camera = mujoco.MjvCamera()
        camera.type = mujoco.mjtCamera.mjCAMERA_FREE
        camera.distance = 0.62
        camera.azimuth = 135
        camera.elevation = -18
        root_joint = mujoco.mj_name2id(model, mujoco.mjtObj.mjOBJ_JOINT, "trunk_base_freejoint")
        root_addr = int(model.jnt_qposadr[root_joint]) if root_joint >= 0 else 0
        joint_addresses = sorted(
            int(model.jnt_qposadr[index])
            for index in range(model.njnt)
            if index != root_joint and int(model.jnt_type[index]) != int(mujoco.mjtJoint.mjJNT_FREE)
        )
        while not self.stop.is_set():
            with self.lock:
                state = dict(self.body.get("state", self.body))
            positions = state.get("positions") or []
            trunk = state.get("trunk") or [0.0, 0.0, state.get("trunk_z", 0.12)]
            imu = state.get("imu") or {}
            quat = imu.get("quat") or [1.0, 0.0, 0.0, 0.0]
            data.qpos[root_addr : root_addr + 3] = list(trunk[:3])
            data.qpos[root_addr + 3 : root_addr + 7] = list(quat[:4])
            for address, position in zip(joint_addresses, positions):
                if address < model.nq:
                    data.qpos[address] = position
            mujoco.mj_forward(model, data)
            camera.lookat[:] = data.qpos[root_addr : root_addr + 3]
            renderer.update_scene(data, camera=camera)
            image = Image.fromarray(renderer.render()).convert("RGB")
            output = BytesIO()
            image.save(output, format="JPEG", quality=84, optimize=True)
            with self.lock:
                self.external = output.getvalue()
                self.render_count += 1
            time.sleep(0.08)

    def external_loop(self) -> None:
        try:
            self._external_loop()
        except Exception as error:  # keep /state.json honest instead of failing silently
            with self.lock:
                self.external_error = f"{type(error).__name__}: {error}"
            print(f"external_model_error={self.external_error}", flush=True)

    def render_external(self) -> bytes:
        """Render one operator frame in the request thread from the latest daemon state."""
        with self.lock:
            state = dict(self.body.get("state", self.body))
        model = mujoco.MjModel.from_xml_path(str(self.robot_root / "scene.xml"))
        data = mujoco.MjData(model)
        renderer = mujoco.Renderer(model, height=480, width=640)
        camera = mujoco.MjvCamera()
        camera.type = mujoco.mjtCamera.mjCAMERA_FREE
        camera.distance = 0.62
        camera.azimuth = 135
        camera.elevation = -18
        root_joint = mujoco.mj_name2id(model, mujoco.mjtObj.mjOBJ_JOINT, "trunk_base_freejoint")
        root_addr = int(model.jnt_qposadr[root_joint])
        trunk = state.get("trunk") or [0.0, 0.0, state.get("trunk_z", 0.12)]
        quat = (state.get("imu") or {}).get("quat") or [1.0, 0.0, 0.0, 0.0]
        data.qpos[root_addr : root_addr + 3] = list(trunk[:3])
        data.qpos[root_addr + 3 : root_addr + 7] = list(quat[:4])
        addresses = sorted(
            int(model.jnt_qposadr[index])
            for index in range(model.njnt)
            if index != root_joint and int(model.jnt_type[index]) != int(mujoco.mjtJoint.mjJNT_FREE)
        )
        for address, position in zip(addresses, state.get("positions") or []):
            if address < model.nq:
                data.qpos[address] = position
        mujoco.mj_forward(model, data)
        camera.lookat[:] = data.qpos[root_addr : root_addr + 3]
        renderer.update_scene(data, camera=camera)
        image = Image.fromarray(renderer.render()).convert("RGB")
        output = BytesIO()
        image.save(output, format="JPEG", quality=84, optimize=True)
        payload = output.getvalue()
        with self.lock:
            self.external = payload
            self.render_count += 1
            self.external_error = None
        return payload

    def lessdb_loop(self) -> None:
        while not self.stop.is_set():
            try:
                with urllib.request.urlopen(self.lessdb_url + "/api/physical/summary", timeout=5) as response:
                    payload = json.load(response)
                with self.lock:
                    self.persisted = payload.get("persisted", {})
            except (OSError, ValueError, urllib.error.URLError):
                pass
            self.stop.wait(2)

    def state(self) -> dict:
        with self.lock:
            body = self.body.get("state", self.body)
            return {
                "source": "official MicroDuck robotd --sim + body_server + official MuJoCo model",
                "camera_connected": self.first_person is not None,
                "external_model_ready": self.external is not None,
                "external_error": self.external_error,
                "camera_frames": self.frame_count,
                "external_frames": self.render_count,
                "body": {
                    "sim_time": body.get("sim_time"),
                    "trunk": body.get("trunk"),
                    "trunk_z": body.get("trunk_z"),
                    "positions": len(body.get("positions") or []),
                    "imu": body.get("imu"),
                },
                "lessdb": {
                    "persisted": self.persisted,
                    "boundary": "LessDB stores sampled provenance; robotd/MuJoCo own control and safety.",
                },
            }


HTML = """<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>MicroDuck · robotd visual twin</title><style>:root{color-scheme:dark;--bg:#071015;--panel:#10202a;--line:#2e4c57;--ink:#effaf7;--muted:#a7bdc4;--accent:#70e1c0}*{box-sizing:border-box}body{margin:0;background:linear-gradient(140deg,#071015,#17313a);color:var(--ink);font:15px system-ui,sans-serif}main{max-width:1280px;margin:auto;padding:24px 16px 44px}h1{font-size:clamp(24px,4vw,40px);margin:0 0 6px}p{color:var(--muted);line-height:1.45}.status{display:inline-block;border:1px solid #397665;color:var(--accent);border-radius:999px;padding:7px 11px;font-size:12px}.views{display:grid;grid-template-columns:1fr 1fr;gap:16px}.panel{background:#10202ae8;border:1px solid var(--line);border-radius:14px;padding:12px;box-shadow:0 16px 50px #0005}.panel h2{font-size:14px;margin:0 0 10px}.panel img{display:block;width:100%;aspect-ratio:16/9;object-fit:cover;background:#03070a;border-radius:9px}.caption{font-size:12px;margin:9px 2px 0;color:var(--muted)}.below{display:grid;grid-template-columns:1fr 1fr;gap:16px;margin-top:16px}pre{white-space:pre-wrap;word-break:break-word;background:#08131a;padding:12px;border-radius:9px;color:#c8dbde;font-size:12px}.note{border-left:3px solid var(--accent);padding-left:10px}@media(max-width:780px){.views,.below{grid-template-columns:1fr}main{padding:16px 10px}}</style><main><p>OFFICIAL MICRODUCK / ROBOTD --SIM</p><h1>One duck, two truths</h1><p>The external view is the official MuJoCo model driven by actual <code>robotd --sim</code> joint state. The first-person view is the real body-server camera stream. LessDB receives bounded samples beside the control loop.</p><span class="status" id="status">connecting…</span><section class="views"><div class="panel"><h2>External model · daemon state</h2><img id="external" src="/external.jpg" alt="External view of official MicroDuck model driven by robotd"><p class="caption">This is the view an operator uses to understand motion.</p></div><div class="panel"><h2>Robot camera · actual simulator stream</h2><img id="first" src="/first-person.jpg" alt="First-person camera from official MicroDuck body server"><p class="caption">This is the same camera stream the simulated duck exposes to its media path.</p></div></section><section class="below"><div class="panel"><h2>Daemon and LessDB state</h2><pre id="state">loading…</pre></div><div class="panel"><h2>Boundary</h2><p class="note">The official daemon and MuJoCo own physics, policy timing, actuator state and safety behavior. LessDB stores sampled observations, proposals and reconciliation evidence. It is not a robot controller or emergency stop.</p></div></section></main><script>const ext=document.querySelector('#external'),first=document.querySelector('#first');async function refresh(){try{const s=await fetch('/state.json?t='+Date.now()).then(r=>r.json());ext.src='/external.jpg?t='+Date.now();first.src='/first-person.jpg?t='+Date.now();document.querySelector('#status').textContent=(s.external_model_ready&&s.camera_connected?'LIVE OFFICIAL DAEMON + CAMERA':'WAITING FOR OFFICIAL STREAM');document.querySelector('#state').textContent=JSON.stringify(s,null,2)}catch(e){document.querySelector('#status').textContent='RETRYING'}}setInterval(refresh,700);refresh();</script>"""


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--robot-root", type=Path, required=True)
    parser.add_argument("--camera-port", type=int, default=7901)
    parser.add_argument("--body-port", type=int, default=7810)
    parser.add_argument("--lessdb-url", default="http://127.0.0.1:8792")
    parser.add_argument("--port", type=int, default=8798)
    args = parser.parse_args()
    twin = Twin(args.robot_root, args.camera_port, args.body_port, args.lessdb_url)
    for target in (twin.camera_loop, twin.body_loop, twin.lessdb_loop):
        threading.Thread(target=target, daemon=True).start()

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):  # noqa: N802
            path = self.path.split("?", 1)[0]
            if path == "/external.jpg":
                try:
                    payload = twin.render_external()
                except Exception as error:
                    with twin.lock:
                        twin.external_error = f"{type(error).__name__}: {error}"
                    payload = None
            else:
                with twin.lock:
                    payload = {"/first-person.jpg": twin.first_person}.get(path)
            if path in {"/external.jpg", "/first-person.jpg"}:
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
            if path == "/state.json":
                payload = json.dumps(twin.state(), separators=(",", ":")).encode()
                content_type = "application/json"
            elif path == "/health":
                payload = b'{"ok":true,"renderer":"official-microduck-robotd"}'
                content_type = "application/json"
            else:
                payload = HTML.encode()
                content_type = "text/html; charset=utf-8"
            self.send_response(200)
            self.send_header("content-type", content_type)
            self.send_header("cache-control", "no-store")
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *_args):
            return

    server = ThreadingHTTPServer(("0.0.0.0", args.port), Handler)
    try:
        server.serve_forever()
    finally:
        twin.stop.set()
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
