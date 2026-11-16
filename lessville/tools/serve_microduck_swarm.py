#!/usr/bin/env python3
"""Serve a browser-visible four-MicroDuck MuJoCo swarm with context sidecars.

The body meshes and scene structure come from the official microduck_rl
repository. This visual twin deliberately keeps real-time control outside
LessDB; an optional local LessVille URL receives sampled envelopes only.
"""

from __future__ import annotations

import argparse
import copy
import io
import json
import math
import threading
import time
import xml.etree.ElementTree as ET
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import urlparse
from urllib.request import Request, urlopen

import mujoco
from PIL import Image

HTML = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>LessVille Physical District — MicroDuck Swarm</title>
<style>
:root{--bg:#08100d;--panel:#122019;--panel2:#1b2d22;--line:#35543f;--ink:#edf6ee;--muted:#9db1a2;--green:#a7ed75;--cyan:#75ddd4;--amber:#f4c86d;--orange:#ff9b55}*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 20% 0,#193525 0,#08100d 46%);color:var(--ink);font:14px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace}.wrap{max-width:1280px;margin:auto;padding:20px 16px 48px}header{display:flex;justify-content:space-between;gap:20px;align-items:flex-start;border-bottom:1px solid var(--line);padding-bottom:18px}.kicker{text-transform:uppercase;letter-spacing:.16em;font-size:10px;color:var(--green)}h1{font-size:clamp(21px,4vw,36px);letter-spacing:-.05em;margin:5px 0}.muted{color:var(--muted)}.status{border:1px solid #617c48;color:var(--green);border-radius:999px;padding:7px 11px;font-size:11px;white-space:nowrap}.layout{display:grid;grid-template-columns:minmax(0,1.45fr) minmax(320px,.75fr);gap:16px;margin-top:16px}.panel{background:var(--panel);border:1px solid var(--line);border-radius:12px;padding:14px;box-shadow:0 10px 32px #0005}.visual{padding:0;overflow:hidden}.visual-head{display:flex;justify-content:space-between;padding:12px 14px;border-bottom:1px solid var(--line);background:var(--panel2)}.visual img{display:block;width:100%;aspect-ratio:4/3;object-fit:contain;background:#20364d}.caption{padding:10px 14px;color:var(--muted);font-size:11px;border-top:1px solid var(--line)}h2{font-size:12px;text-transform:uppercase;letter-spacing:.12em;color:var(--cyan);margin:0 0 12px}.grid{display:grid;grid-template-columns:repeat(2,1fr);gap:8px}.metric{background:var(--panel2);border:1px solid var(--line);border-radius:8px;padding:9px}.metric b{display:block;font-size:16px;color:var(--green)}.metric span{font-size:10px;color:var(--muted)}.duck{border-top:1px solid var(--line);padding:10px 0}.duck:first-child{border-top:0;padding-top:0}.duck-head{display:flex;justify-content:space-between}.duck-id{color:var(--orange);font-weight:700}.duck-detail{color:var(--muted);font-size:11px;margin-top:4px}.chip{display:inline-block;border-radius:999px;padding:3px 7px;font-size:10px;text-transform:uppercase;letter-spacing:.08em}.live{background:#21482a;color:var(--green)}.shared{background:#21404a;color:var(--cyan)}.draft{background:#4c3c1b;color:var(--amber)}.boundary{border-left:3px solid var(--amber);background:#2c2518;color:#eadbb0;padding:11px;font-size:11px;margin-top:12px}.flow{display:grid;gap:7px}.step{display:grid;grid-template-columns:28px 1fr;gap:8px;padding:9px;border:1px solid var(--line);border-radius:8px;background:var(--panel2)}.step strong{display:block}.step small{color:var(--muted)}.num{color:var(--cyan)}code{color:var(--cyan)}@media(max-width:800px){header{display:block}.status{display:inline-block;margin-top:10px}.layout{display:block}.side{margin-top:16px}}
</style></head><body><main class="wrap"><header><div><div class="kicker">LessVille physical district / swarm visual twin</div><h1>MicroDuck <span class="muted">· four-body MuJoCo scene</span></h1><p class="muted">Four official MicroDuck models move together. The right side explains what a swarm agent can share, remember and propose.</p></div><div class="status"><span id="run-status">CONNECTING</span></div></header><div class="layout"><section><div class="panel visual"><div class="visual-head"><strong>Official MicroDuck swarm</strong><span id="phase" class="muted">loading phase</span></div><img id="frame" src="/frame.png" alt="Live four-MicroDuck MuJoCo render"><div class="caption">The visible bodies are MuJoCo models from microduck_rl. LessDB is sampled around the swarm, not inserted into the 50 Hz control path.</div></div></section><aside class="side"><section class="panel"><h2>Shared situation</h2><div class="grid"><div class="metric"><b id="episode">—</b><span>episode</span></div><div class="metric"><b id="count">4</b><span>agents</span></div><div class="metric"><b id="sequence">—</b><span>sequence</span></div><div class="metric"><b id="persist">—</b><span>persisted observations</span></div></div></section><section class="panel" style="margin-top:16px"><h2>Swarm context</h2><div id="ducks"><p class="muted">Waiting for observations…</p></div><div class="boundary"><strong>Why this matters:</strong> each duck has local state, while proximity observations become shared context with source IDs and a bounded proposal. This is the data problem a swarm needs LessDB to solve.</div></section><section class="panel" style="margin-top:16px"><h2>Interaction chain</h2><div class="flow"><div class="step"><div class="num">01</div><div><strong>Observe</strong><small>pose and neighbour relationship</small></div></div><div class="step"><div class="num">02</div><div><strong>Share selectively</strong><small>swarm scope, not every raw sensor tick</small></div></div><div class="step"><div class="num">03</div><div><strong>Propose</strong><small>yield, continue or regroup</small></div></div><div class="step"><div class="num">04</div><div><strong>Reconcile</strong><small>result and sequence are durable evidence</small></div></div></div></section></aside></div></main><script>const frame=document.querySelector('#frame');setInterval(()=>frame.src='/frame.png?t='+Date.now(),120);async function refresh(){try{const s=await fetch('/state.json?t='+Date.now()).then(r=>r.json());document.querySelector('#run-status').textContent=s.lessdb;document.querySelector('#phase').textContent=s.phase;document.querySelector('#episode').textContent=s.episode_id;document.querySelector('#sequence').textContent=s.sequence;document.querySelector('#persist').textContent=s.persisted_observations ?? '—';document.querySelector('#ducks').innerHTML=s.ducks.map(d=>`<div class="duck"><div class="duck-head"><span class="duck-id">${d.id}</span><span class="chip ${d.nearby.length?'shared':'live'}">${d.nearby.length?'shared context':'local state'}</span></div><div class="duck-detail">position ${d.x_m}, ${d.y_m} m · nearby ${d.nearby.join(', ')||'none'}</div><div class="duck-detail">memory <code>${d.memory}</code> · proposal <code>${d.proposal}</code></div></div>`).join('')}catch(e){document.querySelector('#run-status').textContent='RENDER ERROR'}}setInterval(refresh,450);refresh();</script></body></html>"""


def build_scene(robot_root: Path, count: int) -> Path:
    source = ET.parse(robot_root / "robot_groundcontact.xml").getroot()
    base_scene = ET.parse(robot_root / "scene.xml").getroot()
    root = ET.Element("mujoco", {"model": "microduck_swarm"})
    for child in source:
        if child.tag in {"compiler", "default", "asset"}:
            root.append(copy.deepcopy(child))
    root.append(copy.deepcopy(base_scene.find("asset")))
    visual = ET.SubElement(root, "visual")
    ET.SubElement(visual, "headlight", {"diffuse": "0.6 0.6 0.6", "ambient": "0.3 0.3 0.3", "specular": "0 0 0"})
    ET.SubElement(visual, "rgba", {"haze": "0.15 0.25 0.35 1"})
    ET.SubElement(visual, "global", {"azimuth": "160", "elevation": "-20", "offwidth": "640", "offheight": "480"})
    world = ET.SubElement(root, "worldbody")
    ET.SubElement(world, "light", {"pos": "0 0 3.5", "dir": "0 0 -1", "directional": "true"})
    ET.SubElement(world, "geom", {"name": "floor", "size": "0 0 0.05", "pos": "0 0 0", "type": "plane", "material": "groundplane"})
    base = source.find("worldbody/body")
    radius = 0.32
    for index in range(count):
        angle = 2 * math.pi * index / count
        x, y = radius * math.cos(angle), radius * math.sin(angle)
        body = copy.deepcopy(base)
        prefix = f"duck_{index + 1}_"
        body.set("name", prefix + body.get("name", "trunk_base"))
        body.set("pos", f"{x:.5f} {y:.5f} 0.12")
        for element in body.iter():
            if "name" in element.attrib:
                element.set("name", prefix + element.attrib["name"])
        world.append(body)
    path = robot_root / f"lessville_swarm_{count}.xml"
    ET.ElementTree(root).write(path, encoding="unicode")
    return path


class Swarm:
    def __init__(self, model_path: Path, count: int, episode_id: str, lessville_url: str | None):
        self.model = mujoco.MjModel.from_xml_path(str(model_path))
        self.data = mujoco.MjData(self.model)
        self.count = count
        self.episode_id = episode_id
        self.lessville_url = lessville_url.rstrip("/") if lessville_url else None
        self.lock = threading.Lock()
        self.frame = b""
        self.state: dict[str, Any] = {"episode_id": episode_id, "lessdb": "PREVIEW", "phase": "booting"}
        self.started = time.monotonic()
        self.render_thread = threading.Thread(target=self.run, daemon=True)
        self.render_thread.start()

    def root_addr(self, index: int) -> int:
        joint = mujoco.mj_name2id(self.model, mujoco.mjtObj.mjOBJ_JOINT, f"duck_{index + 1}_trunk_base_freejoint")
        return int(self.model.jnt_qposadr[joint])

    def post_envelope(self, duck: dict[str, Any], sequence: int) -> bool:
        if not self.lessville_url:
            return False
        event = {
            "episode_id": self.episode_id,
            "timestamp": f"sequence-{sequence}",
            "sequence": sequence,
            "sensor_id": f"microduck:{duck['id']}:pose",
            "kind": "swarm_pose_observed",
            "frame_id": "sim:microduck-swarm-floor",
            "pose_mm": {"x": round(duck["x_m"] * 1000), "y": round(duck["y_m"] * 1000), "z": 120},
            "uncertainty": {"radius_mm": 5, "confidence_ppm": 970000},
            "evidence_hash": f"sha256:microduck-swarm-{sequence}-{duck['id']}",
            "payload_ref": f"evidence://{self.episode_id}/{duck['id']}/{sequence}",
            "location": "simulated:microduck-swarm",
            "activity": duck["proposal"],
            "recipient_agent": f"agent:{duck['id']}",
            "shared_context": {
                "scope_id": "swarm:microduck",
                "grant_id": "grant:microduck-neighbor-v1",
                "owner_agent": "agent:microduck-coordinator",
                "recipient_agents": [f"agent:{duck['id']}", "agent:microduck-coordinator"],
                "purpose": "bounded neighbor coordination",
                "allowed_kinds": ["swarm_pose_observed", "neighbor_pose"],
                "policy_version": "microduck-swarm-v1",
            },
            "action_kind": "move_to_safe_pose" if duck["proposal"] == "yield-right" else "walk",
            "policy_version": "microduck-swarm-visual-v1",
            "max_observation_age_ms": 2000,
        }
        try:
            from physical_ai_adapter import adapt_event
            envelope = adapt_event("microduck", event)
            request = Request(
                self.lessville_url + "/api/physical/replay",
                data=json.dumps(envelope).encode(),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urlopen(request, timeout=1):
                pass
            proposal = envelope.get("proposal")
            if proposal:
                reconciliation = {
                    "attempt_id": f"attempt:{self.episode_id}:{duck['id']}:{sequence}",
                    "proposal_id": proposal["proposal_id"],
                    "episode_id": self.episode_id,
                    "state": "succeeded",
                    "observed_at": f"sequence-{sequence}",
                    "error_code": None,
                }
                reconcile_request = Request(
                    self.lessville_url + "/api/physical/reconcile",
                    data=json.dumps(reconciliation).encode(),
                    headers={"Content-Type": "application/json"},
                    method="POST",
                )
                with urlopen(reconcile_request, timeout=1):
                    pass
            return True
        except Exception:
            return False

    def persistence_counts(self) -> dict[str, int]:
        if not self.lessville_url:
            return {}
        try:
            with urlopen(self.lessville_url + "/api/physical/summary", timeout=1) as response:
                payload = json.loads(response.read())
            return {key: int(value) for key, value in payload.get("persisted", {}).items()}
        except Exception:
            return {}

    def run(self) -> None:
        renderer = mujoco.Renderer(self.model, height=480, width=640)
        camera = mujoco.MjvCamera(); camera.type = mujoco.mjtCamera.mjCAMERA_FREE; camera.lookat[:] = [0, 0, 0.10]; camera.distance = 1.05; camera.azimuth = 0; camera.elevation = -15
        addresses = [self.root_addr(index) for index in range(self.count)]
        sequence = 0
        last_bridge = 0.0
        persisted: dict[str, int] = {}
        while True:
            now = time.monotonic() - self.started
            phase = ["explore", "neighbour-check", "yield-right", "regroup"][int(now // 3) % 4]
            positions: list[tuple[float, float]] = []
            for index in range(self.count):
                orbit = 2 * math.pi * index / self.count
                x = 0.32 * math.cos(now * 0.22 + orbit) + 0.08 * math.sin(now * 0.73 + index)
                y = 0.32 * math.sin(now * 0.22 + orbit) + 0.06 * math.cos(now * 0.61 + index)
                positions.append((x, y))
                address = addresses[index]
                self.data.qpos[address:address + 3] = [x, y, 0.12]
                self.data.qpos[address + 3:address + 7] = [1, 0, 0, 0]
                # Small leg articulation makes the scene visibly alive without
                # pretending to run MicroDuck's trained policy here.
                joint_name = f"duck_{index + 1}_left_hip_pitch"
                jid = mujoco.mj_name2id(self.model, mujoco.mjtObj.mjOBJ_JOINT, joint_name)
                if jid >= 0:
                    self.data.qpos[self.model.jnt_qposadr[jid]] = 0.18 * math.sin(now * 3 + index)
            mujoco.mj_forward(self.model, self.data)
            ducks = []
            for index, (x, y) in enumerate(positions):
                nearby = [f"duck-{other + 1}" for other, (ox, oy) in enumerate(positions) if other != index and math.hypot(x - ox, y - oy) < 0.5]
                proposal = "yield-right" if phase == "yield-right" and nearby else ("share-neighbour-observation" if nearby else "continue-explore")
                ducks.append({"id": f"duck-{index + 1}", "x_m": round(x, 3), "y_m": round(y, 3), "nearby": nearby, "memory": "swarm/beacon-nearby" if nearby else "local/pose", "proposal": proposal})
            renderer.update_scene(self.data, camera=camera)
            image = Image.fromarray(renderer.render()).convert("RGB")
            buffer = io.BytesIO(); image.save(buffer, format="JPEG", quality=82)
            sequence += 1
            bridged = False
            if time.monotonic() - last_bridge > 1.0:
                bridged = False
                for duck in ducks:
                    # Do not use any(generator): it would stop after the first
                    # successful duck and silently drop the rest of the swarm.
                    bridged = self.post_envelope(duck, sequence) or bridged
                persisted = self.persistence_counts()
                last_bridge = time.monotonic()
            with self.lock:
                self.frame = buffer.getvalue()
                self.state = {"episode_id": self.episode_id, "phase": phase, "sequence": sequence, "lessdb": "LESSDB BRIDGED" if bridged else ("PREVIEW CONTEXT" if not self.lessville_url else "BRIDGE WAITING"), "persisted_observations": persisted.get("observations", 0), "persisted_proposals": persisted.get("proposals", 0), "ducks": ducks}
            time.sleep(0.075)

    def snapshot(self) -> tuple[bytes, dict[str, Any]]:
        with self.lock:
            return self.frame, dict(self.state)


class Handler(BaseHTTPRequestHandler):
    swarm: Swarm

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        if path == "/":
            body, content_type = HTML.encode(), "text/html; charset=utf-8"
        elif path == "/frame.png":
            body, _ = self.swarm.snapshot(); content_type = "image/jpeg"
        elif path == "/state.json":
            _, state = self.swarm.snapshot(); body, content_type = json.dumps(state).encode(), "application/json"
        elif path == "/health":
            body, content_type = b'{"ok":true,"renderer":"mujoco","profile":"microduck-swarm"}', "application/json"
        else:
            self.send_error(404); return
        self.send_response(200); self.send_header("Content-Type", content_type); self.send_header("Cache-Control", "no-store"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)

    def log_message(self, format: str, *args: Any) -> None:
        return


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--robot-root", type=Path, required=True, help="official microduck robot/microduck directory")
    parser.add_argument("--ducks", type=int, default=4)
    parser.add_argument("--episode-id", default="episode:microduck-swarm-visual")
    parser.add_argument("--lessville-url", help="optional private LessVille base URL for sampled envelope persistence")
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8796)
    args = parser.parse_args()
    if not 1 <= args.ducks <= 4:
        raise SystemExit("--ducks must be between 1 and 4")
    scene = build_scene(args.robot_root, args.ducks)
    swarm = Swarm(scene, args.ducks, args.episode_id, args.lessville_url)
    Handler.swarm = swarm
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(json.dumps({"ok": True, "renderer": "mujoco", "profile": "microduck-swarm", "ducks": args.ducks, "port": args.port, "episode_id": args.episode_id}), flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
