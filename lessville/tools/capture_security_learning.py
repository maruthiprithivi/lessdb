#!/usr/bin/env python3
"""Capture observer-only security simulation evolution for later analysis."""
from __future__ import annotations

import argparse
import json
import time
import urllib.request
from pathlib import Path


def get_state(base: str) -> dict:
    with urllib.request.urlopen(base.rstrip("/") + "/state.json", timeout=10) as response:
        value = json.load(response)
    return value.get("world", value)


def projection(world: dict) -> dict:
    actors = {
        actor.get("id", ""): actor.get("trust")
        for actor in world.get("actors", [])
    }
    events = world.get("events", [])
    proposals = world.get("policy_proposals", [])
    last_proposal = proposals[-1] if proposals else None
    return {
        "tick": world.get("tick"),
        "quadrant": world.get("quadrant"),
        "trust": actors,
        "open_questions": len(world.get("open_questions", [])),
        "events": len(events),
        "last_verdict": events[-1].get("verdict") if events else None,
        "last_assessment_schema": (events[-1].get("assessment") or {}).get("schema") if events else None,
        "policy_proposals": len(proposals),
        "last_policy_proposal": {
            "proposal_id": last_proposal.get("proposal_id"),
            "policy_version": last_proposal.get("policy_version"),
            "state": last_proposal.get("state"),
            "requires_approval": last_proposal.get("requires_approval"),
            "auto_apply": last_proposal.get("auto_apply"),
        } if last_proposal else None,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:8810")
    parser.add_argument("--seconds", type=float, default=15.0)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--out", type=Path, default=Path("/tmp/security-learning-capture.json"))
    args = parser.parse_args()

    samples = []
    end = time.monotonic() + max(1.0, args.seconds)
    while time.monotonic() < end:
        samples.append(projection(get_state(args.url)))
        time.sleep(max(0.2, args.interval))

    ticks = [s["tick"] for s in samples if isinstance(s["tick"], int)]
    quadrants = sorted({s["quadrant"] for s in samples})
    report = {
        "ok": bool(samples),
        "source": "observer-only security lab state feed",
        "deterministic_policy_boundary": True,
        "sample_count": len(samples),
        "tick_start": ticks[0] if ticks else None,
        "tick_end": ticks[-1] if ticks else None,
        "quadrants_seen": quadrants,
        "trust_start": samples[0]["trust"] if samples else {},
        "trust_end": samples[-1]["trust"] if samples else {},
        "event_count_start": samples[0]["events"] if samples else 0,
        "event_count_end": samples[-1]["events"] if samples else 0,
        "last_verdict": samples[-1]["last_verdict"] if samples else None,
        "assessment_schema": samples[-1]["last_assessment_schema"] if samples else None,
        "policy_proposal_count_end": samples[-1]["policy_proposals"] if samples else 0,
        "policy_proposals": samples[-1]["last_policy_proposal"] if samples else None,
        "samples": samples,
        "boundary": "Observed score changes and policy proposals are deterministic application evidence; proposals require approval and never auto-apply, and this is not model self-awareness or a security certification.",
    }
    args.out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: report[k] for k in ("ok", "sample_count", "tick_start", "tick_end", "quadrants_seen", "event_count_end", "last_verdict", "assessment_schema", "policy_proposal_count_end", "policy_proposals")}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
