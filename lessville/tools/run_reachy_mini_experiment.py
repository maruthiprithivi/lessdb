#!/usr/bin/env python3
"""Run a small Reachy Mini MuJoCo episode and emit LessDB-ready envelopes.

The daemon must already be running, for example:
  reachy-mini-daemon --sim --headless

This script owns only the semantic SDK client call. It does not put LessDB in
Reachy Mini's physics/control loop. The resulting JSONL can be fed into the
physical replay harness or inspected by a LessVille-style visual twin.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

SDK_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(SDK_ROOT))
from physical_ai_adapter import adapt_event  # noqa: E402


def timestamp() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def pose_event(mini, episode_id: str, sequence: int, action_kind: str) -> dict[str, object]:
    matrix = mini.get_current_head_pose().tolist()
    state = json.dumps(matrix, separators=(",", ":"), sort_keys=True)
    evidence_hash = "sha256:" + hashlib.sha256(state.encode("utf-8")).hexdigest()
    return {
        "episode_id": episode_id,
        "timestamp": timestamp(),
        "sequence": sequence,
        "sensor_id": "reachy-mini:head-pose",
        "kind": "head_pose_observed",
        "frame_id": "reachy-mini:base",
        "pose_mm": {
            "x": round(float(matrix[0][3]) * 1000),
            "y": round(float(matrix[1][3]) * 1000),
            "z": round(float(matrix[2][3]) * 1000),
        },
        "uncertainty": {"radius_mm": 1, "confidence_ppm": 990000},
        "evidence_hash": evidence_hash,
        "payload_ref": f"evidence://{episode_id}/head-pose/{sequence}",
        "calibration_ref": "calibration://reachy-mini/simulated-default",
        "location": "simulated:reachy-mini",
        "activity": "semantic_head_motion",
        "world_conditions": ["mujoco:headless", "media:no_media"],
        "action_kind": action_kind,
        "policy_version": "reachy-mini-sdk-demo-v1",
        "max_observation_age_ms": 2000,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--episode-id", default="episode:reachy-mini-demo")
    args = parser.parse_args()

    try:
        from reachy_mini import ReachyMini
        from reachy_mini.utils import create_head_pose
    except ImportError as exc:
        raise SystemExit("Install reachy-mini[mujoco] in the experiment environment first") from exc

    args.out.parent.mkdir(parents=True, exist_ok=True)
    envelopes: list[dict[str, object]] = []
    with ReachyMini(connection_mode="localhost_only", media_backend="no_media") as mini:
        mini.goto_target(
            head=create_head_pose(z=10, roll=5, mm=True, degrees=True),
            antennas=[0.35, -0.35],
            duration=0.6,
        )
        envelopes.append(adapt_event("reachy-mini", pose_event(mini, args.episode_id, 1, "look_at")))
        mini.goto_target(
            head=create_head_pose(z=0, roll=0, mm=True, degrees=True),
            antennas=[0, 0],
            duration=0.6,
        )
        envelopes.append(adapt_event("reachy-mini", pose_event(mini, args.episode_id, 2, "move_to_safe_pose")))

    with args.out.open("w", encoding="utf-8") as handle:
        for envelope in envelopes:
            handle.write(json.dumps(envelope, separators=(",", ":")) + "\n")
    print(json.dumps({"episode_id": args.episode_id, "envelopes": len(envelopes), "out": str(args.out)}))


if __name__ == "__main__":
    main()
