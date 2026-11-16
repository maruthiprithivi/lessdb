#!/usr/bin/env python3
"""Serve the LessVille security, verification and trust simulation.

The browser surface is read-only. In staging mode this process advances the
private LessVille security backend on a timer; the backend persists each event
and verdict in embedded LessDB.
"""
from __future__ import annotations

import argparse
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.request import Request, urlopen
from urllib.parse import urlparse

HTML = r'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>LessVille Security Lab</title><style>
:root{color-scheme:dark;--bg:#090c14;--panel:#141a27;--panel2:#1d2536;--line:#394762;--ink:#f3f6ff;--muted:#a9b5ce;--cyan:#83e5ee;--green:#a8ec82;--amber:#f4c96e;--red:#ff9d8d;--purple:#c4a7ff}*{box-sizing:border-box}body{margin:0;background:radial-gradient(circle at 20% 0,#283553,#090c14 52%);color:var(--ink);font:14px/1.5 system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}.wrap{max-width:1400px;margin:auto;padding:22px 16px 52px}header{display:flex;justify-content:space-between;gap:18px;align-items:flex-start;border-bottom:1px solid var(--line);padding-bottom:18px}.eyebrow{text-transform:uppercase;letter-spacing:.16em;color:var(--cyan);font-size:10px}h1{font-size:clamp(26px,5vw,50px);letter-spacing:-.06em;line-height:1;margin:8px 0 12px}p,.muted{color:var(--muted)}.status{color:var(--green);border:1px solid #557b53;border-radius:999px;padding:8px 12px;font-size:11px;white-space:nowrap}.layout{display:grid;grid-template-columns:minmax(0,1.55fr) minmax(330px,.8fr);gap:17px;margin-top:18px}.panel{background:#141a27ef;border:1px solid var(--line);border-radius:14px;padding:15px;box-shadow:0 14px 40px #0005}.hero{display:grid;grid-template-columns:1fr 1fr;gap:9px}.quad{padding:13px;border:1px solid var(--line);background:var(--panel2);border-radius:10px;min-height:90px}.quad.active{border-color:var(--cyan);box-shadow:0 0 0 1px #83e5ee55 inset}.quad b{display:block;color:var(--cyan);margin-bottom:5px}.quad span{font-size:12px;color:var(--muted)}h2{font-size:12px;text-transform:uppercase;letter-spacing:.13em;color:var(--cyan);margin:0 0 12px}.metrics{display:grid;grid-template-columns:repeat(3,1fr);gap:8px}.metric{background:var(--panel2);border:1px solid var(--line);border-radius:9px;padding:9px;min-width:0}.metric b{display:block;color:var(--green);font-size:18px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.metric span{font-size:10px;color:var(--muted)}.actors{display:grid;grid-template-columns:repeat(3,1fr);gap:8px;margin-top:16px}.actor{background:var(--panel2);border:1px solid var(--line);border-radius:9px;padding:10px}.actor strong{display:flex;justify-content:space-between;gap:5px;color:var(--purple)}.actor small{display:block;color:var(--muted);font-size:11px;margin-top:5px}.cap{color:var(--cyan);font-size:10px;margin-top:7px;line-height:1.5}.event{border-top:1px solid var(--line);padding:10px 0}.event:first-child{border-top:0;padding-top:0}.event-head{display:flex;justify-content:space-between;gap:8px}.event-head b{color:var(--amber)}.event small{display:block;color:var(--muted);margin-top:4px}.pill{border-radius:999px;padding:3px 7px;font-size:10px;white-space:nowrap}.blocked{color:var(--green);background:#24472c}.danger{color:var(--red);background:#4f272b}.held{color:var(--amber);background:#4a3c20}.guard{display:grid;gap:8px}.guard-row{display:flex;justify-content:space-between;align-items:center;gap:10px;background:var(--panel2);padding:9px;border-radius:8px}.on{color:var(--green)}.boundary{border-left:3px solid var(--amber);background:#302a1c;color:#efdfb4;padding:11px;font-size:11px;margin-top:14px}.foot{margin-top:16px;color:var(--muted);font-size:11px}@media(max-width:900px){header{display:block}.status{display:inline-block;margin-top:12px}.layout{display:block}.side{margin-top:17px}.actors{grid-template-columns:1fr 1fr}}@media(max-width:560px){.hero,.actors,.metrics{grid-template-columns:1fr}.wrap{padding-inline:11px}}
</style></head><body><main class="wrap"><header><div><div class="eyebrow">LessVille / security, verification and trust</div><h1>Security Lab <span class="muted">— four kinds of not-knowing</span></h1><p>This is a deterministic research simulation, not a production security product. Named actors discover how private data, untrusted content, tool combinations and outbound paths interact. Every step is recorded as LessDB event and decision provenance.</p></div><div class="status" id="status">CONNECTING</div></header><section class="layout"><section><div class="panel"><h2>Knowledge quadrant</h2><div class="hero"><div class="quad" data-q="known-known"><b>Known known</b><span>Evidence and capability are explicit.</span></div><div class="quad" data-q="known-unknown"><b>Known unknown</b><span>The missing check is visible and can be requested.</span></div><div class="quad" data-q="unknown-known"><b>Unknown known</b><span>A fact exists in another scope but is not yet in context.</span></div><div class="quad" data-q="unknown-unknown"><b>Unknown unknown</b><span>The next tool or evidence gap has not been modelled yet.</span></div></div></div><div class="panel" style="margin-top:16px"><h2>Actor network</h2><div class="actors" id="actors"></div></div><div class="panel" style="margin-top:16px"><h2>Recent evidence and decisions</h2><div id="events"><p class="muted">Waiting for the first scenario step…</p></div></div></section><aside class="side"><section class="panel"><h2>Current threat path</h2><div class="metrics"><div class="metric"><b id="tick">—</b><span>tick</span></div><div class="metric"><b id="quadrant">—</b><span>quadrant</span></div><div class="metric"><b id="verdict">—</b><span>verdict</span></div></div><div class="boundary"><strong>Combined-path check:</strong> the lab treats private data, untrusted content and an outbound path as a compound risk. A safe tool in isolation is not automatically safe in combination.</div></section><section class="panel"><h2>Guardrails</h2><div class="guard" id="guards"></div></section><section class="panel"><h2>LessDB record</h2><div id="record" class="muted">Loading persisted counts…</div><div class="boundary">The simulation uses application-owned event and decision rows in embedded LessDB. This does not claim native row ACL, a complete policy engine, formal verification or a security certification.</div></section></aside></section><div class="foot">Observer-only staging surface. The scenario driver is private; this page exposes state and provenance, not mutation controls.</div></main><script>
function esc(v){return String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}function verdictClass(v){return v&&v.startsWith('danger')?'danger':v&&v.startsWith('blocked')?'blocked':'held'}async function refresh(){try{const s=await fetch('/state.json?t='+Date.now()).then(r=>r.json());const w=s.world;document.querySelector('#status').textContent='SCENARIO RUNNING';document.querySelector('#tick').textContent=w.tick;document.querySelector('#quadrant').textContent=w.quadrant;const last=w.events[w.events.length-1];document.querySelector('#verdict').innerHTML=last?`<span class="pill ${verdictClass(last.verdict)}">${esc(last.verdict)}</span>`:'—';document.querySelectorAll('.quad').forEach(q=>q.classList.toggle('active',q.dataset.q===w.quadrant));document.querySelector('#actors').innerHTML=w.actors.map(a=>`<div class="actor"><strong><span>${esc(a.name)}</span><span>${esc(a.trust)}</span></strong><small>${esc(a.persona)}</small><div class="cap">${a.capabilities.map(esc).join(' · ')}</div><div class="muted" style="font-size:10px;margin-top:5px">visibility: ${esc(a.visibility)}</div></div>`).join('');document.querySelector('#guards').innerHTML=Object.entries(w.guardrails).map(([k,v])=>`<div class="guard-row"><span>${esc(k.replaceAll('_',' '))}</span><b class="${v?'on':''}">${v?'ON':'OFF'}</b></div>`).join('');document.querySelector('#events').innerHTML=w.events.slice().reverse().slice(0,9).map(e=>{const a=e.assessment||{};return `<div class="event"><div class="event-head"><b>${esc(e.kind)} · ${esc(e.actor)}</b><span class="pill ${verdictClass(e.verdict)}">${esc(e.verdict)}</span></div><small>${esc(e.detail)}</small><small>quadrant=${esc(e.quadrant)} · evidence=${e.evidence.map(esc).join(', ')}</small><small>typed=${esc(a.schema||'legacy')} · policy=${esc(a.policy_version||'—')} · factors=${(a.risk_factors||[]).map(esc).join(', ')}</small></div>`}).join('');document.querySelector('#record').innerHTML=`<b>LessDB events:</b> ${esc(s.persisted?.events??'—')}<br><b>LessDB decisions:</b> ${esc(s.persisted?.decisions??'—')}<br><b>state rows:</b> ${esc(s.persisted?.state_rows??'—')}<br><span class="muted">${esc(s.boundary??'')}</span>`}catch(e){document.querySelector('#status').textContent='RETRYING'}}setInterval(refresh,700);refresh();</script></body></html>'''


def read_json(base: str, path: str) -> dict:
    with urlopen(base.rstrip('/') + path, timeout=4) as response:
        return json.loads(response.read())


def step(base: str) -> dict:
    request = Request(base.rstrip('/') + '/api/security/step', data=b'{}', headers={'Content-Type': 'application/json'}, method='POST')
    with urlopen(request, timeout=4) as response:
        return json.loads(response.read())


class Lab:
    def __init__(self, backend: str, auto: bool):
        self.backend = backend
        self.auto = auto
        self.lock = threading.Lock()
        self.last = {'ok': False, 'world': {'tick': 0, 'actors': [], 'events': []}, 'persisted': {}}
        threading.Thread(target=self.loop, daemon=True).start()

    def loop(self) -> None:
        while True:
            try:
                if self.auto:
                    step(self.backend)
                state = read_json(self.backend, '/api/security/state')
                tables = read_json(self.backend, '/api/tables').get('tables', {})
                with self.lock:
                    self.last = {
                        'ok': state.get('ok', False),
                        'world': state.get('world', {}),
                        'persisted': {'events': tables.get('lv_events', 0), 'decisions': tables.get('lv_decisions', 0), 'state_rows': tables.get('lv_kv', 0)},
                        'boundary': state.get('boundary', ''),
                    }
            except Exception as exc:
                with self.lock:
                    self.last = {'ok': False, 'error': type(exc).__name__, 'world': {'tick': 0, 'actors': [], 'events': []}, 'persisted': {}}
            time.sleep(2.5 if self.auto else 1.0)

    def snapshot(self) -> dict:
        with self.lock:
            return json.loads(json.dumps(self.last))


class Handler(BaseHTTPRequestHandler):
    lab: Lab
    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path == '/':
            body, content_type = HTML.encode(), 'text/html; charset=utf-8'
        elif path == '/state.json':
            body, content_type = json.dumps(self.lab.snapshot()).encode(), 'application/json'
        elif path == '/health':
            body, content_type = json.dumps({'ok': True, 'service': 'lessville-security-lab', 'observer_only': True}).encode(), 'application/json'
        else:
            self.send_error(404); return
        self.send_response(200); self.send_header('Content-Type', content_type); self.send_header('Cache-Control', 'no-store'); self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)
    def log_message(self, *_args: object) -> None:
        return


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--backend', default='http://127.0.0.1:8812')
    parser.add_argument('--port', type=int, default=8810)
    parser.add_argument('--no-auto', action='store_true')
    parser.add_argument('--host', default='0.0.0.0')
    args = parser.parse_args()
    lab = Lab(args.backend, not args.no_auto)
    Handler.lab = lab
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(json.dumps({'ok': True, 'service': 'lessville-security-lab', 'port': args.port, 'observer_only': True}), flush=True)
    server.serve_forever()


if __name__ == '__main__':
    main()
