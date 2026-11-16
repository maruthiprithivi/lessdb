#!/usr/bin/env python3
"""Translate simulator event JSONL into LessDB physical-agent envelopes.

This adapter is deliberately transport-neutral. It does not connect to a robot,
call a simulator, or execute an actuator command. A Reachy or Microduck adapter
can map its SDK/daemon event into the input shape below, then persist the output
through an application-owned LessDB broker.

Input fields:
  episode_id, timestamp, sequence, sensor_id, kind, frame_id, pose_mm,
  uncertainty, evidence_hash, payload_ref, calibration_ref, action_kind
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from typing import Any


PROFILES: dict[str, dict[str, Any]] = {
    "reachy-mini": {
        "simulator": "reachy-mini-mujoco",
        "control_interface": "reachy-mini-sdk-localhost",
        "allowed_actions": ["look_at", "express", "move_to_safe_pose"],
        "evidence_streams": ["camera", "audio_event", "head_pose", "antenna_pose"],
    },
    "reachy-2": {
        "simulator": "reachy-2-gazebo-or-mujoco",
        "control_interface": "reachy-2-sdk-server",
        "allowed_actions": ["inspect", "move_to_safe_pose", "grasp_proposal"],
        "evidence_streams": ["camera", "joint_state", "tracked_body_pose", "action_result"],
    },
    "microduck": {
        "simulator": "microduck-mujoco",
        "control_interface": "microduck-daemon-json-rpc",
        "allowed_actions": ["walk", "stand_up", "grab_proposal", "move_to_safe_pose"],
        "evidence_streams": ["camera", "lidar", "imu", "policy_action", "action_result"],
    },
}


def stable_id(prefix: str, *parts: object) -> str:
    value = ":".join(str(part) for part in parts)
    digest = hashlib.sha256(value.encode("utf-8")).hexdigest()[:16]
    return f"{prefix}:{digest}"


def _required(event: dict[str, Any], name: str) -> Any:
    value = event.get(name)
    if value is None or value == "":
        raise ValueError(f"missing required field: {name}")
    return value


def _pose(event: dict[str, Any]) -> dict[str, Any] | None:
    raw = event.get("pose_mm")
    if raw is None:
        return None
    if not isinstance(raw, dict) or not all(k in raw for k in ("x", "y", "z")):
        raise ValueError("pose_mm must contain x, y and z")
    return {
        "frame_id": _required(event, "frame_id"),
        "parent_frame": event.get("parent_frame"),
        "x_mm": int(raw["x"]),
        "y_mm": int(raw["y"]),
        "z_mm": int(raw["z"]),
        "yaw_mdeg": int(raw.get("yaw_mdeg", 0)),
        "pitch_mdeg": int(raw.get("pitch_mdeg", 0)),
        "roll_mdeg": int(raw.get("roll_mdeg", 0)),
    }


def _uncertainty(event: dict[str, Any]) -> dict[str, Any] | None:
    raw = event.get("uncertainty")
    if raw is None:
        return None
    if not isinstance(raw, dict):
        raise ValueError("uncertainty must be an object")
    radius = int(raw.get("radius_mm", 0))
    confidence = int(raw.get("confidence_ppm", 0))
    if radius < 0 or not 0 <= confidence <= 1_000_000:
        raise ValueError("uncertainty is outside bounded integer ranges")
    return {
        "radius_mm": radius,
        "confidence_ppm": confidence,
        "covariance_ref": raw.get("covariance_ref"),
    }


def adapt_event(profile: str, event: dict[str, Any]) -> dict[str, Any]:
    if profile not in PROFILES:
        raise ValueError(f"unsupported profile: {profile}")
    cfg = PROFILES[profile]
    episode_id = str(_required(event, "episode_id"))
    timestamp = str(_required(event, "timestamp"))
    sequence = int(_required(event, "sequence"))
    if sequence < 0:
        raise ValueError("sequence must be non-negative")
    sensor_id = str(_required(event, "sensor_id"))
    kind = str(_required(event, "kind"))
    observation_id = stable_id("obs", profile, episode_id, sequence, sensor_id, kind)
    context_id = stable_id("ctx", profile, episode_id, sequence)

    observation: dict[str, Any] = {
        "observation_id": observation_id,
        "sensor_id": sensor_id,
        "kind": kind,
        "observed_at": timestamp,
        "logical_time": event.get("logical_time"),
        "valid_until": event.get("valid_until"),
        "pose": _pose(event),
        "uncertainty": _uncertainty(event),
        "payload_ref": event.get("payload_ref"),
        "calibration_ref": event.get("calibration_ref"),
        "evidence_hash": event.get("evidence_hash"),
        "sequence": sequence,
        "source_refs": [
            {
                "kind": "simulator_event",
                "id": stable_id("event", profile, episode_id, sequence),
                "relation": "observed_from",
            }
        ],
    }

    shared_context = event.get("shared_context")
    if not isinstance(shared_context, dict):
        shared_context = {
            "scope_id": "swarm:microduck" if profile == "microduck" else f"local:{profile}",
            "grant_id": "grant:microduck-neighbor-v1" if profile == "microduck" else f"grant:{profile}-local-v1",
            "owner_agent": "agent:coordinator" if profile == "microduck" else f"agent:{profile}",
            "recipient_agents": [event.get("recipient_agent", "agent:coordinator")],
            "purpose": "bounded neighbor coordination" if profile == "microduck" else "local agent observation",
            "allowed_kinds": [kind],
            "policy_version": event.get("policy_version", "physical-p1"),
        }

    envelope: dict[str, Any] = {
        "schema": "lessdb-physical-envelope/v1",
        "profile": profile,
        "simulator": cfg["simulator"],
        "control_interface": cfg["control_interface"],
        "episode_id": episode_id,
        "situation": {
            "context_id": context_id,
            "observed_at": timestamp,
            "location": event.get("location", "simulated:unknown"),
            "activity": event.get("activity", "bounded_observation"),
            "world_conditions": list(event.get("world_conditions", ["simulator:running"])),
            "allowed_actions": cfg["allowed_actions"],
            "recent_observation_ids": [observation_id],
            "shared_context": shared_context,
        },
        "observation": observation,
        "provenance": {
            "source": "simulator_adapter",
            "evidence_streams": cfg["evidence_streams"],
            "raw_media_stored_by": "application_or_device_evidence_store",
        },
        "proposal": None,
    }

    action_kind = event.get("action_kind")
    if action_kind is not None:
        action_kind = str(action_kind)
        if action_kind not in cfg["allowed_actions"]:
            raise ValueError(f"action_kind is not allowed for {profile}: {action_kind}")
        # A swarm can emit several observations at the same sequence. Include
        # the source observation so proposals remain agent/observation-specific
        # instead of collapsing into one same-tick row.
        proposal_id = stable_id("proposal", profile, episode_id, observation_id, action_kind)
        envelope["proposal"] = {
            "proposal_id": proposal_id,
            "context_id": context_id,
            "target_id": event.get("target_id", "simulated:robot"),
            "action_kind": action_kind,
            "idempotency_key": stable_id("idem", profile, episode_id, observation_id, action_kind),
            "state": "draft",
            "source_observation_ids": [observation_id],
            "safety": {
                "policy_version": event.get("policy_version", "simulator-p1"),
                "allowed_action_kinds": cfg["allowed_actions"],
                "max_duration_ms": event.get("max_duration_ms", 5_000),
                "max_observation_age_ms": event.get("max_observation_age_ms", 2_000),
                "required_approvals": ["safety:simulator"],
                "stop_conditions": ["observation_stale", "human_stop", "interlock_unknown"],
            },
            "proposed_by": event.get("proposed_by", f"agent:{profile}"),
            "approved_by": None,
            "executor_id": None,
        }
    return envelope


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=sorted(PROFILES), required=True)
    parser.add_argument("--example", action="store_true", help="print one valid input event and exit")
    args = parser.parse_args()
    if args.example:
        print(json.dumps({
            "episode_id": "episode:demo",
            "timestamp": "2026-01-15T08:20:00Z",
            "sequence": 1,
            "sensor_id": f"sensor:{args.profile}:camera",
            "kind": "scene_observed",
            "frame_id": "sim:floor-1",
            "pose_mm": {"x": 12500, "y": 4200, "z": 0},
            "uncertainty": {"radius_mm": 80, "confidence_ppm": 970000},
            "evidence_hash": "sha256:demo-frame",
            "payload_ref": "evidence://demo/frame-1",
            "calibration_ref": "calibration://demo/v1",
            "action_kind": PROFILES[args.profile]["allowed_actions"][0],
        }, separators=(",", ":")))
        return
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            event = json.loads(line)
            print(json.dumps(adapt_event(args.profile, event), separators=(",", ":")))
        except (json.JSONDecodeError, TypeError, ValueError) as exc:
            print(json.dumps({"error": str(exc)}), file=sys.stderr)
            raise SystemExit(2) from exc


if __name__ == "__main__":
    main()
