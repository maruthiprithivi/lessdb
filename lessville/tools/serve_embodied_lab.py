#!/usr/bin/env python3
"""Serve a unified observer-only hub for official robot simulator visual twins.

The hub composes existing local visual services. It does not control physics,
robot daemons or actuators, and it does not write to LessDB itself.
"""
from __future__ import annotations

import argparse
import json
import mimetypes
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import urlparse

HTML = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>LessDB Embodied Lab</title>
<style>
:root{color-scheme:dark;--bg:#08110f;--panel:#111f1c;--panel2:#182d27;--line:#315247;--ink:#eff8f1;--muted:#a5bbb0;--mint:#8de7bd;--cyan:#83d9e2;--amber:#f2c878;--red:#ff9e8d}
*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 15% 0,#1c4035 0,#08110f 46%);color:var(--ink);font:15px/1.45 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}.wrap{max-width:1380px;margin:auto;padding:24px 18px 50px}header{display:flex;justify-content:space-between;gap:24px;align-items:flex-start;border-bottom:1px solid var(--line);padding-bottom:20px}.eyebrow{text-transform:uppercase;letter-spacing:.16em;font-size:10px;color:var(--mint)}h1{font-size:clamp(27px,5vw,50px);letter-spacing:-.055em;line-height:1.02;margin:6px 0 10px}h1 span{color:var(--muted);font-weight:400}p{color:var(--muted);line-height:1.55}.status{border:1px solid #477761;color:var(--mint);border-radius:999px;padding:8px 12px;font-size:12px;white-space:nowrap}.tabs{display:flex;gap:8px;flex-wrap:wrap;margin:20px 0}.tab{appearance:none;background:var(--panel);border:1px solid var(--line);color:var(--muted);padding:10px 13px;border-radius:9px;cursor:pointer;font-weight:650}.tab:hover,.tab.active{background:var(--panel2);border-color:var(--mint);color:var(--ink)}.tab small{display:block;font-weight:400;color:var(--muted);font-size:10px;margin-top:2px}.layout{display:grid;grid-template-columns:minmax(0,1.6fr) minmax(320px,.8fr);gap:17px}.panel{background:#111f1cee;border:1px solid var(--line);border-radius:14px;padding:15px;box-shadow:0 14px 44px #0005}.stage{padding:0;overflow:hidden}.stage-head{display:flex;justify-content:space-between;gap:10px;align-items:center;padding:13px 15px;border-bottom:1px solid var(--line);background:var(--panel2)}.stage-head strong{font-size:14px}.pill{border-radius:999px;padding:4px 8px;font-size:10px;text-transform:uppercase;letter-spacing:.08em}.live{background:#214a35;color:var(--mint)}.twin{background:#24434b;color:var(--cyan)}.review{background:#4d3e20;color:var(--amber)}#scene{display:block;width:100%;aspect-ratio:4/3;object-fit:contain;background:#152e3f}.scene-caption{padding:12px 15px;border-top:1px solid var(--line);color:var(--muted);font-size:12px}.side{display:grid;gap:17px;align-content:start}h2{font-size:12px;text-transform:uppercase;letter-spacing:.13em;color:var(--cyan);margin:0 0 12px}.metrics{display:grid;grid-template-columns:1fr 1fr;gap:9px}.metric{background:var(--panel2);border:1px solid var(--line);border-radius:9px;padding:10px;min-width:0}.metric b{display:block;color:var(--mint);font-size:17px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.metric span{display:block;color:var(--muted);font-size:10px;margin-top:3px}.chain{display:grid;gap:7px}.step{display:grid;grid-template-columns:24px 1fr;gap:8px;padding:9px;background:var(--panel2);border:1px solid var(--line);border-radius:8px}.step i{font-style:normal;color:var(--cyan)}.step strong{display:block;font-size:13px}.step small{color:var(--muted);font-size:11px}.boundary{border-left:3px solid var(--amber);background:#2f2819;color:#eadcb9;padding:11px;font-size:12px}.source{font-size:11px;color:var(--muted);overflow-wrap:anywhere}.source code{color:var(--cyan)}.notice{margin:17px 0 0;padding:13px 15px;border:1px solid #4a5c36;background:#263021;border-radius:11px;color:#dfe9cb;font-size:12px}.detail{white-space:pre-wrap;word-break:break-word;background:#091513;border-radius:8px;padding:10px;color:#b9d6cb;font:11px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace;max-height:250px;overflow:auto}.lessdb-ok{color:var(--mint)}.warn{color:var(--amber)}@media(max-width:820px){header{display:block}.status{display:inline-block;margin-top:13px}.layout{display:block}.side{margin-top:17px}.wrap{padding:17px 11px 36px}.stage-head{align-items:flex-start;flex-direction:column}}
</style>
</head>
<body><main class="wrap">
<header><div><div class="eyebrow">LessDB / embodied agent research</div><h1>Embodied Lab <span>— simulator district</span></h1><p>One observer surface for official robot simulation paths. Choose a scene, watch the physical model, and inspect the bounded situation, proposal and reconciliation lineage that LessDB stores beside it.</p></div><div class="status" id="overall">CONNECTING</div></header>
<nav class="tabs" aria-label="Simulator scenes">
<button class="tab active" data-scene="reachy"><span>Reachy Mini</span><small>official MuJoCo model</small></button>
<button class="tab" data-scene="swarm"><span>MicroDuck swarm</span><small>four official bodies</small></button>
<button class="tab" data-scene="camera"><span>MicroDuck robotd</span><small>official camera stream</small></button>
<button class="tab" data-scene="operator"><span>MicroDuck operator</span><small>daemon joint state</small></button>
</nav>
<section class="layout"><div><div class="panel stage"><div class="stage-head"><strong id="scene-title">Reachy Mini · visual twin</strong><span class="pill twin" id="scene-badge">MODEL TWIN</span></div><img id="scene" src="/scene/reachy/frame.png" alt="Selected official robot simulator scene"><div class="scene-caption" id="scene-caption">Official Reachy Mini MuJoCo model with an observer-side motion cycle. This view is not connected to a physical robot or actuator controller.</div></div><div class="notice"><strong>How this relates to LessVille:</strong> LessVille makes an agent town visible. This lab applies the same idea to embodied agents: a simulator owns the world; LessDB owns bounded memory, context scope, task lineage, proposals and outcomes.</div></div>
<aside class="side"><section class="panel"><h2>Live situation</h2><div class="metrics"><div class="metric"><b id="sim-status">—</b><span>simulator</span></div><div class="metric"><b id="agent-count">—</b><span>agents/bodies</span></div><div class="metric"><b id="sim-time">—</b><span>sim time / phase</span></div><div class="metric"><b id="observations">—</b><span>LessDB observations</span></div><div class="metric"><b id="proposals">—</b><span>proposals</span></div><div class="metric"><b id="reconciliations">—</b><span>reconciliations</span></div></div></section><section class="panel"><h2>Agent-to-world loop</h2><div class="chain"><div class="step"><i>01</i><div><strong>Observe</strong><small id="observe-copy">pose, policy state and sensor evidence</small></div></div><div class="step"><i>02</i><div><strong>Recall selectively</strong><small id="recall-copy">frame, time and explicit swarm scope</small></div></div><div class="step"><i>03</i><div><strong>Propose</strong><small id="proposal-copy">draft action with freshness and lineage</small></div></div><div class="step"><i>04</i><div><strong>Reconcile</strong><small id="reconcile-copy">simulator result, duplicate and error state</small></div></div></div></section><section class="panel"><h2>Truth boundary</h2><div class="boundary" id="boundary">The simulator owns physics, policy timing, collision handling and safety. LessDB is not a motor controller, emergency stop or sim-to-real guarantee.</div><p class="source" id="source">source: waiting</p><div class="detail" id="detail">Waiting for the selected simulator…</div></section></aside></section>
</main><script>
const scenes={
 reachy:{title:'Reachy Mini · visual twin',badge:'MODEL TWIN',badgeClass:'twin',image:'/scene/reachy/frame.png',caption:'Official Reachy Mini MuJoCo model with an observer-side motion cycle. This view is not connected to a physical robot or actuator controller.',source:'official Reachy Mini MuJoCo model; prior SDK daemon smoke test is separate',observe:'head pose and simulator phase',recall:'time-valid preference and scene evidence',proposal:'look_at or move_to_safe_pose',reconcile:'visual-twin sampled result'},
 swarm:{title:'MicroDuck · four-body swarm',badge:'SWARM TWIN',badgeClass:'twin',image:'/scene/swarm/frame.png',caption:'Four official MicroDuck models with proximity-derived shared context. The local replay bridge persists bounded observations, proposals and reconciliation evidence.',source:'official microduck_rl MuJoCo models; private LessDB replay bridge',observe:'pose and neighbour relationship',recall:'explicit neighbour scope, not global sensor access',proposal:'yield-right, regroup or continue-explore',reconcile:'per-duck proposal outcome'},
 camera:{title:'MicroDuck · official robotd camera',badge:'LIVE DAEMON',badgeClass:'live',image:'/scene/camera/frame.jpg',caption:'The official body-server camera stream from the live MicroDuck robotd --sim path. The daemon and MuJoCo own the 50 Hz control loop.',source:'official duck-sim body server + robotd --sim',observe:'camera frame, trunk state and IMU',recall:'sampled evidence and policy episode context',proposal:'bounded drive intent outside LessDB',reconcile:'daemon/controller outcome'},
 operator:{title:'MicroDuck · daemon-driven operator view',badge:'LIVE DAEMON',badgeClass:'live',image:'/scene/operator/external.jpg',caption:'Official MicroDuck model with joint positions mirrored from the live body-server state. It is an operator view of daemon state, not a second controller.',source:'official robotd body state mirrored onto official model',observe:'15-joint body state and sim time',recall:'episode, policy and device provenance',proposal:'executor-owned motion intent',reconcile:'sampled daemon result'}
};
let selected='reachy';
const $=id=>document.getElementById(id);
function choose(name){selected=name;const s=scenes[name];document.querySelectorAll('.tab').forEach(b=>b.classList.toggle('active',b.dataset.scene===name));$('scene-title').textContent=s.title;$('scene-badge').textContent=s.badge;$('scene-badge').className='pill '+s.badgeClass;$('scene-caption').textContent=s.caption;$('scene').alt=s.title;$('source').textContent='source: '+s.source;refreshImage();refresh();}
function refreshImage(){const s=scenes[selected];$('scene').src=s.image+'?t='+Date.now()}
async function refresh(){try{const v=await fetch('/api/status?scene='+selected,{cache:'no-store'}).then(r=>r.json());const s=v.scene||{};const p=v.lessdb||{};$('overall').textContent=v.all_live?'ALL SELECTED SERVICES LIVE':'PARTIAL / CHECKING';$('sim-status').textContent=s.status||'—';$('agent-count').textContent=s.agents??s.bodies??'—';$('sim-time').textContent=s.phase||s.sim_time||'—';$('observations').textContent=p.observations??'—';$('proposals').textContent=p.proposals??'—';$('reconciliations').textContent=p.reconciliations??'—';$('detail').textContent=JSON.stringify({scene:s,lessdb:p},null,2);$('boundary').textContent=v.boundary||'The simulator owns physics and safety; LessDB stores bounded provenance.'}catch(e){$('overall').textContent='RETRYING';$('detail').textContent='Waiting for local simulator services…'}}
document.querySelectorAll('.tab').forEach(b=>b.addEventListener('click',()=>choose(b.dataset.scene)));setInterval(refreshImage,900);setInterval(refresh,1000);choose('reachy');
</script></body></html>"""

class Hub:
    def __init__(self, services: dict[str, str], lessdb: str):
        self.services = {k: v.rstrip('/') for k, v in services.items()}
        self.lessdb = lessdb.rstrip('/')

    def get_json(self, url: str) -> dict[str, Any]:
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "lessdb-embodied-lab/1"})
            with urllib.request.urlopen(request, timeout=1.5) as response:
                value = json.loads(response.read())
            return value if isinstance(value, dict) else {}
        except (OSError, ValueError, urllib.error.URLError):
            return {}

    def get_bytes(self, url: str) -> tuple[bytes, str] | None:
        # The operator view renders the official MuJoCo model on demand and
        # can briefly return 503 while its render context is busy. Retry a
        # bounded number of times; never turn this into a control loop.
        for _ in range(3):
            try:
                request = urllib.request.Request(url, headers={"User-Agent": "lessdb-embodied-lab/1"})
                with urllib.request.urlopen(request, timeout=5) as response:
                    content_type = response.headers.get_content_type()
                    return response.read(), content_type
            except (OSError, urllib.error.URLError):
                continue
        return None

    def status(self, selected: str) -> dict[str, Any]:
        state_urls = {
            "reachy": self.services["reachy"] + "/state.json",
            "swarm": self.services["swarm"] + "/state.json",
            "camera": self.services["camera"] + "/state.json",
            "operator": self.services["operator"] + "/state.json",
        }
        scene = self.get_json(state_urls[selected])
        lessdb = self.get_json(self.lessdb + "/api/physical/summary")
        persisted = lessdb.get("persisted", {}) if isinstance(lessdb, dict) else {}
        if selected == "reachy":
            normalized = {"status": "RUNNING" if scene.get("phase") else "WAITING", "agents": 1, "phase": scene.get("phase"), "sim_time": scene.get("sequence")}
        elif selected == "swarm":
            normalized = {"status": scene.get("lessdb", "WAITING"), "agents": len(scene.get("ducks", [])) or 4, "phase": scene.get("phase"), "sim_time": scene.get("sequence")}
        elif selected == "camera":
            camera = scene.get("camera", {})
            body = scene.get("body", {})
            normalized = {"status": "LIVE CAMERA" if camera.get("connected") else "WAITING", "agents": 1, "phase": f"{body.get('sim_time', '—')} s", "sim_time": body.get("trunk_z")}
        else:
            normalized = {"status": "LIVE DAEMON" if scene.get("external_model_ready") else "WAITING", "agents": 1, "phase": scene.get("phase"), "sim_time": scene.get("sequence")}
        all_live = bool(scene) and bool(lessdb)
        return {"all_live": all_live, "scene": normalized, "lessdb": persisted, "boundary": "Physics, policy timing, simulator safety and daemon control remain outside LessDB. LessDB stores bounded observations, context scope, proposals and reconciliation evidence."}

    def proxy(self, kind: str, path: str) -> tuple[bytes, str] | None:
        mapping = {
            "reachy": (self.services["reachy"], "/frame.png"),
            "swarm": (self.services["swarm"], "/frame.png"),
            "camera": (self.services["camera"], "/frame.jpg"),
            "operator": (self.services["operator"], "/external.jpg"),
        }
        base, upstream_path = mapping[kind]
        return self.get_bytes(base + upstream_path)

class Handler(BaseHTTPRequestHandler):
    hub: Hub

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        path = parsed.path
        if path == "/":
            body, content_type = HTML.encode(), "text/html; charset=utf-8"
        elif path == "/api/status":
            query = parsed.query
            selected = "reachy"
            for item in query.split("&"):
                if item.startswith("scene="):
                    selected = item.split("=", 1)[1]
            if selected not in {"reachy", "swarm", "camera", "operator"}:
                selected = "reachy"
            body, content_type = json.dumps(self.hub.status(selected), separators=(",", ":")).encode(), "application/json"
        elif path.startswith("/scene/"):
            pieces = path.split("/")
            if len(pieces) < 4 or pieces[2] not in {"reachy", "swarm", "camera", "operator"}:
                self.send_error(404); return
            value = self.hub.proxy(pieces[2], "/" + "/".join(pieces[3:]))
            if value is None:
                self.send_response(503); self.end_headers(); return
            body, content_type = value
        elif path == "/health":
            body, content_type = b'{"ok":true,"service":"lessdb-embodied-lab","boundary":"observer-only"}', "application/json"
        else:
            self.send_error(404); return
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args: Any) -> None:
        return

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8799)
    parser.add_argument("--reachy-url", default="http://127.0.0.1:8795")
    parser.add_argument("--swarm-url", default="http://127.0.0.1:8796")
    parser.add_argument("--camera-url", default="http://127.0.0.1:8797")
    parser.add_argument("--operator-url", default="http://127.0.0.1:8798")
    parser.add_argument("--lessdb-url", default="http://127.0.0.1:8792")
    args = parser.parse_args()
    hub = Hub({"reachy": args.reachy_url, "swarm": args.swarm_url, "camera": args.camera_url, "operator": args.operator_url}, args.lessdb_url)
    Handler.hub = hub
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(json.dumps({"ok": True, "service": "lessdb-embodied-lab", "port": args.port, "observer_only": True}), flush=True)
    server.serve_forever()
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
