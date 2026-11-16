#!/usr/bin/env python3
"""Serve a unified, observer-only LessVille physical world.

The official Reachy Mini and MicroDuck simulators remain separate physics
owners. This process gives them a shared district coordinate frame, clock,
context-scope view and LessDB telemetry projection without entering control
loops or exposing actuator endpoints.
"""
from __future__ import annotations

import argparse
import json
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import urlparse

HTML = r'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>LessVille Physical World · LessDB Embodied Lab</title>
<style>
:root{color-scheme:dark;--bg:#07100e;--panel:#10201b;--panel2:#183027;--line:#31564a;--ink:#eef8f1;--muted:#a4bcb0;--mint:#8ce6bc;--cyan:#82dbe5;--amber:#f2c877;--red:#ff9e8d}*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 15% 0,#1e493a,#07100e 48%);color:var(--ink);font:14px/1.45 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}.wrap{max-width:1450px;margin:auto;padding:24px 18px 54px}header{display:flex;justify-content:space-between;gap:22px;align-items:flex-start;border-bottom:1px solid var(--line);padding-bottom:18px}.eyebrow{text-transform:uppercase;letter-spacing:.16em;font-size:10px;color:var(--mint)}h1{font-size:clamp(28px,5vw,52px);line-height:1;letter-spacing:-.055em;margin:7px 0 11px}h1 span{color:var(--muted);font-weight:400}p{color:var(--muted);line-height:1.55}.status{border:1px solid #4d8269;color:var(--mint);border-radius:999px;padding:8px 12px;white-space:nowrap;font-size:11px}.layout{display:grid;grid-template-columns:minmax(0,1.65fr) minmax(320px,.75fr);gap:17px;margin-top:18px}.panel{background:#10201bee;border:1px solid var(--line);border-radius:14px;padding:15px;box-shadow:0 16px 46px #0005}.world{padding:0;overflow:hidden}.world-head{display:flex;justify-content:space-between;gap:12px;align-items:center;padding:13px 15px;background:var(--panel2);border-bottom:1px solid var(--line)}.world-head strong{font-size:14px}.pill{border-radius:999px;padding:4px 8px;font-size:10px;text-transform:uppercase;letter-spacing:.08em}.live{background:#214b35;color:var(--mint)}.observer{background:#29404a;color:var(--cyan)}canvas{display:block;width:100%;height:auto;aspect-ratio:16/9;background:#0b1a18}.caption{padding:12px 15px;border-top:1px solid var(--line);color:var(--muted);font-size:12px}.side{display:grid;gap:17px;align-content:start}h2{font-size:12px;text-transform:uppercase;letter-spacing:.13em;color:var(--cyan);margin:0 0 12px}.metrics{display:grid;grid-template-columns:1fr 1fr;gap:8px}.metric{background:var(--panel2);border:1px solid var(--line);border-radius:9px;padding:9px;min-width:0}.metric b{display:block;font-size:17px;color:var(--mint);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.metric span{display:block;color:var(--muted);font-size:10px;margin-top:3px}.scope{border-top:1px solid var(--line);padding:9px 0}.scope:first-child{border-top:0;padding-top:0}.scope-head{display:flex;justify-content:space-between;gap:8px}.scope strong{color:var(--ink);font-size:12px}.scope small{display:block;color:var(--muted);font-size:10px;margin-top:3px}.grant{color:var(--cyan);font:10px ui-monospace,SFMono-Regular,Menlo,monospace}.source-grid{display:grid;grid-template-columns:1fr 1fr;gap:8px}.source-card{background:var(--panel2);border:1px solid var(--line);border-radius:8px;overflow:hidden}.source-card img{display:block;width:100%;aspect-ratio:4/3;object-fit:cover;background:#102c36}.source-card span{display:block;padding:7px;color:var(--muted);font-size:10px}.boundary{border-left:3px solid var(--amber);background:#302919;color:#ebdbb5;padding:11px;font-size:11px}.detail{white-space:pre-wrap;word-break:break-word;background:#081411;border-radius:8px;padding:10px;color:#b8d4c8;font:10px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace;max-height:220px;overflow:auto}.bar{height:7px;background:#0a1714;border-radius:99px;overflow:hidden;margin-top:7px}.bar i{display:block;height:100%;background:linear-gradient(90deg,var(--cyan),var(--mint));width:0}.legend{display:flex;gap:14px;flex-wrap:wrap;padding:10px 15px;color:var(--muted);font-size:11px}.legend b{color:var(--ink)}@media(max-width:850px){header{display:block}.status{display:inline-block;margin-top:12px}.layout{display:block}.side{margin-top:17px}.wrap{padding:17px 11px 38px}.world-head{align-items:flex-start;flex-direction:column}.source-grid{grid-template-columns:1fr}}
</style></head><body><main class="wrap">
<header><div><div class="eyebrow">LessVille / embodied agent research</div><h1>Physical World <span>— one shared district</span></h1><p>Reachy Mini and the MicroDuck swarm now appear in one bounded LessVille world coordinate system. The source simulators still own their physics; LessDB records the spatial evidence, scoped context and proposal lineage around them.</p></div><div class="status" id="status">CONNECTING</div></header>
<section class="layout"><div><div class="panel world"><div class="world-head"><strong>LessVille embodied district · shared-coordinate view</strong><span class="pill observer">OBSERVER ONLY</span></div><canvas id="world" width="1200" height="675" aria-label="Unified coordinate map showing Reachy Mini and four MicroDucks"></canvas><div class="legend"><span><b style="color:#8ce6bc">● Reachy Mini</b> station</span><span><b style="color:#82dbe5">● MicroDuck</b> swarm agents</span><span><b style="color:#f2c877">—</b> explicit context grant</span><span>frame: <b id="frame-name">lessville:embodied-district/v1</b></span></div><div class="caption">This is a shared-world orchestration view, not a merged physics engine. Reachy and MicroDuck retain separate official simulator/control loops while the coordinator gives them one explainable frame, clock and context boundary.</div></div><div class="panel" style="margin-top:17px"><h2>Official source renders</h2><div class="source-grid"><div class="source-card"><img id="reachy" src="/source/reachy/frame.png" alt="Reachy Mini official MuJoCo render"><span>Reachy Mini · official model twin</span></div><div class="source-card"><img id="swarm" src="/source/swarm/frame.png" alt="MicroDuck swarm official MuJoCo render"><span>MicroDuck swarm · four official bodies</span></div><div class="source-card"><img id="camera" src="/source/camera/frame.jpg" alt="MicroDuck official daemon camera"><span>MicroDuck robotd · official camera</span></div><div class="source-card"><img id="operator" src="/source/operator/external.jpg" alt="MicroDuck daemon-driven operator view"><span>MicroDuck operator · daemon state mirror</span></div></div></div></div>
<aside class="side"><section class="panel"><h2>Real-time telemetry</h2><div class="metrics"><div class="metric"><b id="clock">—</b><span>shared world tick</span></div><div class="metric"><b id="entities">—</b><span>entities</span></div><div class="metric"><b id="frames">—</b><span>spatial frames</span></div><div class="metric"><b id="scopes">—</b><span>context scopes</span></div><div class="metric"><b id="obs">—</b><span>LessDB observations</span></div><div class="metric"><b id="props">—</b><span>proposals / reconciliations</span></div></div><div class="bar"><i id="coverage"></i></div><small id="coverage-label" style="color:var(--muted)">loading analytics window coverage…</small></section><section class="panel"><h2>Shared-context scopes</h2><div id="scope-list"><span style="color:var(--muted)">waiting for LessDB analytics…</span></div></section><section class="panel"><h2>Telemetry interpretation</h2><div class="boundary">Spatial frame IDs, sequence ranges and scope grants are application metadata carried in bounded envelopes. LessDB is providing persistence and analytics here; it is not silently becoming native spatial indexing, native row ACL, robot safety or a cross-system transaction.</div><p id="source-status" style="font-size:11px">source services: checking…</p><div class="detail" id="detail">Waiting for the unified world state…</div></section></aside></section>
</main><script>
const canvas=document.querySelector('#world'),ctx=canvas.getContext('2d');let state={};
function clamp(v,a,b){return Math.max(a,Math.min(b,v))}function text(t,x,y,size=14,color='#eef8f1',weight='500'){ctx.fillStyle=color;ctx.font=weight+' '+size+'px system-ui';ctx.fillText(t,x,y)}
function drawWorld(s){const w=canvas.width,h=canvas.height;ctx.clearRect(0,0,w,h);ctx.fillStyle='#0b1b18';ctx.fillRect(0,0,w,h);ctx.strokeStyle='#18372e';ctx.lineWidth=1;for(let x=70;x<w;x+=80){ctx.beginPath();ctx.moveTo(x,55);ctx.lineTo(x,h-65);ctx.stroke()}for(let y=75;y<h-65;y+=70){ctx.beginPath();ctx.moveTo(45,y);ctx.lineTo(w-45,y);ctx.stroke()}ctx.fillStyle='#122b25';ctx.fillRect(70,100,270,420);ctx.fillStyle='#173b32';ctx.fillRect(430,140,590,320);text('HOME / REACHY STATION',88,130,12,'#82dbe5','650');text('MICRODUCK COORDINATION FLOOR',450,170,12,'#82dbe5','650');text('shared frame: lessville:embodied-district/v1',70,h-30,11,'#a4bcb0');const entities=s.entities||[];const ducks=entities.filter(e=>e.kind==='MicroDuck');const reachy=entities.find(e=>e.kind==='Reachy Mini');if(reachy){ctx.fillStyle='#8ce6bc';ctx.beginPath();ctx.arc(reachy.x*w,reachy.y*h,25,0,Math.PI*2);ctx.fill();ctx.strokeStyle='#dfffee';ctx.stroke();text('R',reachy.x*w-6,reachy.y*h+7,18,'#07100e','700');text('Reachy Mini',reachy.x*w-48,reachy.y*h+43,12,'#8ce6bc','650')}
ctx.strokeStyle='#f2c877';ctx.lineWidth=2;for(const d of ducks){if(d.nearby)for(const id of d.nearby){const other=entities.find(e=>e.id===id);if(other&&d.id<id){ctx.beginPath();ctx.moveTo(d.x*w,d.y*h);ctx.lineTo(other.x*w,other.y*h);ctx.stroke()}}}for(const d of ducks){ctx.fillStyle='#82dbe5';ctx.beginPath();ctx.arc(d.x*w,d.y*h,18,0,Math.PI*2);ctx.fill();ctx.strokeStyle='#dffbff';ctx.stroke();text(d.label,d.x*w-27,d.y*h+34,11,'#82dbe5','650');text(d.id.replace('duck-','D'),d.x*w-5,d.y*h+5,12,'#07100e','700')}text('N',w-68,78,15,'#a4bcb0','700');ctx.strokeStyle='#a4bcb0';ctx.beginPath();ctx.moveTo(w-60,112);ctx.lineTo(w-60,85);ctx.stroke();ctx.beginPath();ctx.moveTo(w-60,85);ctx.lineTo(w-66,94);ctx.moveTo(w-60,85);ctx.lineTo(w-54,94);ctx.stroke()}
function renderScopes(scopes){document.querySelector('#scope-list').innerHTML=(scopes||[]).map(s=>`<div class="scope"><div class="scope-head"><strong>${s.scope_id}</strong><span style="color:var(--mint);font-size:10px">${s.samples??0} samples</span></div><small><span class="grant">${s.grant_id||'coordinator projection'}</span><br>${s.recipients||'bounded recipients'} · ${s.explicit_grant_metadata?'explicit envelope grant':'coordinator projection'}</small></div>`).join('')||'<span style="color:var(--muted)">no scope rows in current analytics window</span>'}
async function refresh(){try{const v=await fetch('/api/unified',{cache:'no-store'}).then(r=>r.json());state=v;drawWorld(v);const t=v.telemetry||{},p=t.persisted||{};document.querySelector('#status').textContent=v.all_live?'UNIFIED WORLD LIVE':'PARTIAL / CHECKING';document.querySelector('#clock').textContent=v.world_tick??'—';document.querySelector('#entities').textContent=(v.entities||[]).length;document.querySelector('#frames').textContent=(t.frames||[]).length;document.querySelector('#scopes').textContent=(t.scopes||[]).length;document.querySelector('#obs').textContent=p.observations??'—';document.querySelector('#props').textContent=(p.proposals??'—')+' / '+(p.reconciliations??'—');document.querySelector('#coverage').style.width=(t.window_coverage_percent??0)+'%';document.querySelector('#coverage-label').textContent='bounded analytics window '+(t.sample_window??0)+' rows · sequence span '+(t.sequence_span??0)+' · latest '+(t.latest_observed_at||'not supplied');renderScopes(v.scopes);document.querySelector('#source-status').textContent='sources: '+Object.entries(v.sources||{}).map(([k,x])=>k+'='+x).join(' · ');document.querySelector('#detail').textContent=JSON.stringify({world_frame:v.world_frame,telemetry:t,sources:v.sources},null,2)}catch(e){document.querySelector('#status').textContent='RETRYING';document.querySelector('#detail').textContent='Waiting for simulator and LessDB services…'}}setInterval(refresh,1000);setInterval(()=>{for(const id of ['reachy','swarm','camera','operator'])document.querySelector('#'+id).src='/source/'+id+(id==='camera'?'/frame.jpg':id==='operator'?'/external.jpg':'/frame.png')+'?t='+Date.now()},1000);refresh();
</script></body></html>'''

class World:
    def __init__(self, services: dict[str, str], lessdb: str):
        self.services = {k: v.rstrip('/') for k, v in services.items()}
        self.lessdb = lessdb.rstrip('/')
        self.started = time.monotonic()

    def get_json(self, url: str) -> dict[str, Any]:
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "lessdb-unified-physical-world/1"})
            with urllib.request.urlopen(req, timeout=2) as response:
                value = json.loads(response.read())
            return value if isinstance(value, dict) else {}
        except (OSError, ValueError, urllib.error.URLError):
            return {}

    def get_bytes(self, url: str) -> tuple[bytes, str] | None:
        for _ in range(2):
            try:
                req = urllib.request.Request(url, headers={"User-Agent": "lessdb-unified-physical-world/1"})
                with urllib.request.urlopen(req, timeout=5) as response:
                    return response.read(), response.headers.get_content_type()
            except (OSError, urllib.error.URLError):
                pass
        return None

    def source_states(self) -> dict[str, dict[str, Any]]:
        paths = {"reachy": "/state.json", "swarm": "/state.json", "camera": "/state.json", "operator": "/state.json"}
        return {name: self.get_json(self.services[name] + path) for name, path in paths.items()}

    def telemetry(self) -> dict[str, Any]:
        analytics = self.get_json(self.lessdb + "/api/physical/analytics")
        if not analytics:
            analytics = self.get_json(self.lessdb + "/api/physical/summary")
        if "frames" not in analytics:
            analytics.update({"world_frame": "lessville:embodied-district/v1", "frames": [], "scopes": [], "sample_window": 0, "sequence": {"span": 0}})
        seq = analytics.get("sequence", {})
        analytics["sequence_span"] = seq.get("span", 0)
        analytics["latest_observed_at"] = next((x.get("latest_observed_at") for x in analytics.get("frames", []) if x.get("latest_observed_at")), None)
        analytics["window_coverage_percent"] = 100 if analytics.get("sample_window", 0) else 0
        return analytics

    def state(self) -> dict[str, Any]:
        source = self.source_states()
        swarm = source.get("swarm", {})
        ducks = swarm.get("ducks", [])
        entities = [{"id": "reachy-mini", "label": "Reachy", "kind": "Reachy Mini", "x": 0.22, "y": 0.49, "frame_id": "reachy-mini/base", "scope_ids": ["local:reachy-mini", "district:embodied"]}]
        for duck in ducks or [{"id": f"duck-{i}", "x_m": 0.0, "y_m": 0.0, "nearby": []} for i in range(1, 5)]:
            x = 0.53 + float(duck.get("x_m", 0)) * 0.58
            y = 0.50 + float(duck.get("y_m", 0)) * 0.58
            entities.append({"id": duck.get("id", "duck-unknown"), "label": "Duck", "kind": "MicroDuck", "x": max(.38, min(.92, x)), "y": max(.23, min(.80, y)), "nearby": duck.get("nearby", []), "frame_id": f"microduck/{duck.get('id','unknown')}/base", "scope_ids": ["swarm:microduck", "district:embodied"]})
        telemetry = self.telemetry()
        discovered_scopes = telemetry.get("scopes", [])
        scopes = [{"scope_id": "district:embodied", "grant_id": "grant:embodied-district-observe-v1", "recipients": "Reachy + MicroDuck coordinator", "samples": 0, "explicit_grant_metadata": False}]
        for scope in discovered_scopes:
            scope = dict(scope)
            scope.setdefault("recipients", "bounded recipients")
            scopes.append(scope)
        if not any(s["scope_id"] == "local:reachy-mini" for s in scopes):
            scopes.append({"scope_id": "local:reachy-mini", "grant_id": "grant:reachy-local-v1", "recipients": "Reachy Mini", "samples": 0, "explicit_grant_metadata": False})
        if not any(s["scope_id"] == "swarm:microduck" for s in scopes):
            scopes.append({"scope_id": "swarm:microduck", "grant_id": "grant:microduck-neighbor-v1", "recipients": "MicroDuck coordinator + four agents", "samples": 0, "explicit_grant_metadata": False})
        live = {name: bool(value) for name, value in source.items()}
        return {"all_live": all(live.values()) and bool(telemetry), "world_frame": "lessville:embodied-district/v1", "world_tick": int(time.monotonic() - self.started), "entities": entities, "scopes": scopes, "sources": live, "telemetry": telemetry, "boundary": "This is an observer-only shared-coordinate orchestration view. Official simulators and robot daemons own physics, policy timing, control and safety; LessDB stores bounded evidence and application-owned context analytics."}

class Handler(BaseHTTPRequestHandler):
    world: World
    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        path = parsed.path
        if path == "/":
            body, content_type = HTML.encode(), "text/html; charset=utf-8"
        elif path in {"/api/unified", "/api/telemetry"}:
            value = self.world.state()
            body = json.dumps(value if path == "/api/unified" else value["telemetry"], separators=(",", ":")).encode()
            content_type = "application/json"
        elif path.startswith("/source/"):
            parts = path.split("/")
            if len(parts) < 4 or parts[2] not in self.world.services:
                self.send_error(404); return
            target = self.world.services[parts[2]] + "/" + "/".join(parts[3:])
            value = self.world.get_bytes(target)
            if value is None:
                self.send_response(503); self.end_headers(); return
            body, content_type = value
        elif path == "/health":
            body, content_type = b'{"ok":true,"service":"lessdb-unified-physical-world","observer_only":true}', "application/json"
        else:
            self.send_error(404); return
        self.send_response(200); self.send_header("Content-Type", content_type); self.send_header("Cache-Control", "no-store"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *_args: Any) -> None:
        return

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=8800)
    parser.add_argument("--reachy-url", default="http://127.0.0.1:8795")
    parser.add_argument("--swarm-url", default="http://127.0.0.1:8796")
    parser.add_argument("--camera-url", default="http://127.0.0.1:8797")
    parser.add_argument("--operator-url", default="http://127.0.0.1:8798")
    parser.add_argument("--lessdb-url", default="http://127.0.0.1:8792")
    args = parser.parse_args()
    Handler.world = World({"reachy": args.reachy_url, "swarm": args.swarm_url, "camera": args.camera_url, "operator": args.operator_url}, args.lessdb_url)
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(json.dumps({"ok": True, "service": "lessdb-unified-physical-world", "port": args.port, "observer_only": True}), flush=True)
    server.serve_forever()
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
