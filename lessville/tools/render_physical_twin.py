#!/usr/bin/env python3
"""Render LessDB physical envelopes as a self-contained LessVille-style twin."""

from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
from typing import Any


def esc(value: Any) -> str:
    return html.escape(str(value), quote=True)


def short(value: Any, size: int = 18) -> str:
    text = str(value or "—")
    return text if len(text) <= size else text[:size] + "…"


def render(envelopes: list[dict[str, Any]], reconciliation: dict[str, Any] | None = None) -> str:
    if not envelopes:
        raise ValueError("the envelope file contains no simulator observations")
    first = envelopes[0]
    profile = first.get("profile", "unknown")
    simulator = first.get("simulator", "unknown")
    episode = first.get("episode_id", "unknown")
    proposals = [e.get("proposal") for e in envelopes if e.get("proposal")]
    observations = [e.get("observation", {}) for e in envelopes]
    reconciliation = reconciliation or {}
    reconciliation_state = reconciliation.get("state", "pending")
    timeline = []
    for envelope in envelopes:
        observation = envelope.get("observation", {})
        pose = observation.get("pose") or {}
        uncertainty = observation.get("uncertainty") or {}
        proposal = envelope.get("proposal") or {}
        timeline.append(f"""
        <article class="event">
          <div class="event-seq">SEQ {esc(observation.get('sequence', '—'))}</div>
          <div class="event-main">
            <h3>{esc(observation.get('kind', 'observation'))}</h3>
            <p class="muted">{esc(observation.get('observed_at', ''))} · {esc(observation.get('sensor_id', ''))}</p>
            <div class="facts">
              <span>pose <b>{esc(pose.get('x_mm', '—'))},{esc(pose.get('y_mm', '—'))},{esc(pose.get('z_mm', '—'))} mm</b></span>
              <span>uncertainty <b>±{esc(uncertainty.get('radius_mm', '—'))} mm</b></span>
              <span>evidence <b>{esc(short(observation.get('evidence_hash'), 26))}</b></span>
            </div>
          </div>
          <div class="event-action"><span class="chip draft">{esc(proposal.get('state', 'observed'))}</span><br><small>{esc(proposal.get('action_kind', 'sampled state'))}</small></div>
        </article>
        """)

    proposal_rows = []
    for proposal in proposals:
        safety = proposal.get("safety") or {}
        proposal_rows.append(f"""
        <div class="proposal-row">
          <div><strong>{esc(proposal.get('action_kind', 'action'))}</strong><br><small>{esc(proposal.get('proposal_id', ''))}</small></div>
          <span class="chip draft">{esc(proposal.get('state', 'draft'))}</span>
          <div class="proposal-detail">freshness ≤ {esc(safety.get('max_observation_age_ms', '—'))} ms<br>idempotency {esc(short(proposal.get('idempotency_key'), 22))}</div>
        </div>
        """)

    reconciliation_panel = f"""
    <section class="panel" style="margin-top:16px"><h2>LessDB reconciliation</h2>
      <div class="proposal-row"><div><strong>{esc(reconciliation_state)}</strong><br><small>{esc(reconciliation.get('attempt_id', 'No executor attempt recorded'))}</small></div><span class="chip {'good' if reconciliation_state == 'succeeded' else 'draft'}">{esc(reconciliation_state)}</span><div class="proposal-detail">proposal {esc(reconciliation.get('proposal_id', '—'))}<br>observed {esc(reconciliation.get('observed_at', '—'))}{('<br>error ' + esc(reconciliation.get('error_code'))) if reconciliation.get('error_code') else ''}</div></div>
    </section>
    """ if reconciliation else """
    <section class="panel" style="margin-top:16px"><h2>LessDB reconciliation</h2><p class="muted">No executor result has been attached yet.</p></section>
    """

    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>LessVille Physical Twin — {esc(profile)}</title>
<style>
:root {{ --ink:#e8f0e8; --muted:#95a79a; --panel:#14221b; --panel2:#1b2d22; --line:#34533f; --green:#9be564; --amber:#f5c86b; --cyan:#75d7d2; --bg:#09100d; }}
* {{ box-sizing:border-box; }} body {{ margin:0; background:var(--bg); color:var(--ink); font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace; }}
.wrap {{ max-width:1180px; margin:0 auto; padding:28px 18px 54px; }}
header {{ display:flex; justify-content:space-between; gap:24px; align-items:flex-start; border-bottom:1px solid var(--line); padding-bottom:22px; }}
h1 {{ margin:0; font-size:clamp(22px,4vw,38px); letter-spacing:-.04em; }} .kicker {{ color:var(--green); text-transform:uppercase; letter-spacing:.14em; font-size:11px; }}
.muted, small {{ color:var(--muted); }} .status {{ border:1px solid #557344; color:var(--green); padding:6px 10px; border-radius:999px; white-space:nowrap; }}
.grid {{ display:grid; grid-template-columns:1.35fr .85fr; gap:16px; margin-top:18px; }} .panel {{ background:var(--panel); border:1px solid var(--line); border-radius:12px; padding:18px; box-shadow:0 8px 30px #0004; }}
h2 {{ font-size:14px; margin:0 0 14px; color:var(--cyan); text-transform:uppercase; letter-spacing:.1em; }}
.metrics {{ display:grid; grid-template-columns:repeat(4,1fr); gap:8px; margin-top:18px; }} .metric {{ background:var(--panel2); border:1px solid var(--line); border-radius:9px; padding:12px; }} .metric b {{ display:block; font-size:21px; color:var(--green); }} .metric span {{ color:var(--muted); font-size:11px; }}
.event {{ display:grid; grid-template-columns:58px 1fr 130px; gap:14px; border-top:1px solid var(--line); padding:14px 0; align-items:start; }} .event:first-child {{ border-top:0; padding-top:0; }}
.event-seq {{ color:var(--amber); font-size:11px; padding-top:3px; }} h3 {{ margin:0; font-size:15px; }} .event p {{ margin:3px 0 9px; font-size:11px; }} .facts {{ display:flex; flex-wrap:wrap; gap:7px; }} .facts span {{ border:1px solid var(--line); color:var(--muted); padding:5px 7px; border-radius:6px; font-size:11px; }} .facts b {{ color:var(--ink); font-weight:500; }} .event-action {{ text-align:right; color:var(--muted); font-size:11px; }}
.chip {{ display:inline-block; padding:3px 7px; border-radius:999px; font-size:10px; text-transform:uppercase; letter-spacing:.08em; }} .draft {{ background:#4c3c1b; color:var(--amber); }} .good {{ background:#23482c; color:var(--green); }}
.flow {{ display:grid; gap:8px; }} .flow-step {{ display:grid; grid-template-columns:34px 1fr; gap:10px; align-items:center; padding:10px; border:1px solid var(--line); border-radius:8px; background:var(--panel2); }} .flow-step .num {{ color:var(--cyan); font-size:16px; }} .flow-step strong {{ display:block; }} .flow-step span {{ color:var(--muted); font-size:11px; }}
.proposal-row {{ display:grid; grid-template-columns:1fr auto; gap:8px; align-items:center; border-top:1px solid var(--line); padding:11px 0; }} .proposal-row:first-child {{ border-top:0; padding-top:0; }} .proposal-detail {{ grid-column:1 / -1; color:var(--muted); font-size:11px; }}
.notice {{ border-left:3px solid var(--amber); background:#292217; padding:12px; color:#ead9aa; font-size:12px; margin-top:16px; }} code {{ color:var(--cyan); }}
@media (max-width:760px) {{ header,.grid {{ display:block; }} header .status {{ display:inline-block; margin-top:14px; }} .metrics {{ grid-template-columns:repeat(2,1fr); }} .panel + .panel {{ margin-top:16px; }} .event {{ grid-template-columns:48px 1fr; }} .event-action {{ grid-column:2; text-align:left; }} }}
</style>
</head>
<body><main class="wrap">
<header><div><div class="kicker">LessVille physical twin / LessDB observability</div><h1>{esc(profile)} <span class="muted">· {esc(episode)}</span></h1><p class="muted">Simulator: <code>{esc(simulator)}</code>. A sampled, replayable view of agent context and physical outcomes.</p></div><div class="status">SIMULATOR RUN CAPTURED</div></header>
<section class="metrics"><div class="metric"><b>{len(observations)}</b><span>observations</span></div><div class="metric"><b>{len(proposals)}</b><span>draft proposals</span></div><div class="metric"><b>{esc(profile)}</b><span>profile</span></div><div class="metric"><b>{esc(reconciliation_state)}</b><span>reconciliation</span></div></section>
<div class="grid"><section class="panel"><h2>Observed episode</h2>{''.join(timeline)}</section><aside><section class="panel"><h2>Agent-to-world loop</h2><div class="flow"><div class="flow-step"><div class="num">01</div><div><strong>Agent task</strong><span>episode and purpose are identified</span></div></div><div class="flow-step"><div class="num">02</div><div><strong>Situation + evidence</strong><span>bounded pose, time, sensor and provenance</span></div></div><div class="flow-step"><div class="num">03</div><div><strong>Proposal</strong><span>draft action with freshness and idempotency</span></div></div><div class="flow-step"><div class="num">04</div><div><strong>Reconciliation</strong><span>simulator result returns as durable evidence</span></div></div></div></section><section class="panel" style="margin-top:16px"><h2>LessDB proposals</h2>{''.join(proposal_rows) or '<p class="muted">No proposal was emitted.</p>'}</section>{reconciliation_panel}</aside></div>
<div class="notice"><strong>Boundary:</strong> LessDB stores episode identity, sampled observations, context and provenance. The simulator owns physics and real-time control. This view is an observability twin, not a safety certification or a direct motor-control surface.</div>
</main></body></html>"""


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--reconciliation", type=Path)
    args = parser.parse_args()
    envelopes = [json.loads(line) for line in args.input.read_text(encoding="utf-8").splitlines() if line.strip()]
    reconciliation = json.loads(args.reconciliation.read_text(encoding="utf-8")) if args.reconciliation else None
    args.output.write_text(render(envelopes, reconciliation), encoding="utf-8")
    print(json.dumps({"output": str(args.output), "observations": len(envelopes)}))


if __name__ == "__main__":
    main()
