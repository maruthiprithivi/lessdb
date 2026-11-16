#!/usr/bin/env python3
"""Generate deterministic LessDB agent-adoption scenario fixtures.

This is a scenario-design tool, not a simulator and not a policy engine. It
creates bounded JSON fixtures that an application-specific runner can execute
against Lessville or another LessDB-backed agent application.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
from pathlib import Path

PROFILES = (
    "local-single-user",
    "family-assistant",
    "consumer-companion",
    "professional-copilot",
    "software-factory",
    "organization-community",
    "scientific-swarm",
    "embodied-household",
    "industrial-operations",
    "field-robotics",
    "reachy-mini",
    "reachy-2",
    "microduck",
)

PHYSICAL_PROFILES = frozenset(
    {
        "embodied-household",
        "industrial-operations",
        "field-robotics",
        "reachy-mini",
        "reachy-2",
        "microduck",
    }
)

SIMULATOR_PROFILES = {
    "embodied-household": {
        "simulator": "generic-mujoco",
        "control_interface": "adapter-only",
        "evidence_streams": ["camera", "imu", "action_result"],
    },
    "industrial-operations": {
        "simulator": "generic-mujoco",
        "control_interface": "adapter-only",
        "evidence_streams": ["camera", "pose", "calibration", "action_result"],
    },
    "field-robotics": {
        "simulator": "generic-mujoco",
        "control_interface": "adapter-only",
        "evidence_streams": ["camera", "pose", "gps_or_map", "action_result"],
    },
    "reachy-mini": {
        "simulator": "reachy-mini-mujoco",
        "control_interface": "reachy-mini-sdk-localhost",
        "evidence_streams": ["camera", "audio_event", "head_pose", "antenna_pose"],
    },
    "reachy-2": {
        "simulator": "reachy-2-gazebo-or-mujoco",
        "control_interface": "reachy-2-sdk-server",
        "evidence_streams": ["camera", "joint_state", "tracked_body_pose", "action_result"],
    },
    "microduck": {
        "simulator": "microduck-mujoco",
        "control_interface": "microduck-daemon-json-rpc",
        "evidence_streams": ["camera", "lidar", "imu", "policy_action", "action_result"],
    },
}

SCOPE_CASES = (
    ("shared", "private", "scope_denied"),
    ("project", "sensitive", "sensitive_scope_denied"),
    ("analyst_projection", "private", "projection_excluded"),
)
LIFECYCLE_CASES = (
    ("active", "expired", "expired"),
    ("active", "archived", "archived"),
    ("active", "revoked", "revoked"),
    ("active", "deleted", "deleted"),
)
ACTION_CASES = ("draft", "pending", "approved", "denied", "completed")
FAILURE_CASES = ("fresh", "replay", "retry", "policy_drift", "recipient_change")


def stable_id(prefix: str, seed: int, index: int) -> str:
    digest = hashlib.sha256(f"{prefix}:{seed}:{index}".encode()).hexdigest()[:12]
    return f"{prefix}-{digest}"


def choose(rng: random.Random, values: tuple[str, ...]) -> str:
    return values[rng.randrange(len(values))]


def build_case(seed: int, index: int, profile: str) -> dict[str, object]:
    rng = random.Random(seed * 1009 + index * 9176)
    include_scope, hidden_scope, omission = SCOPE_CASES[rng.randrange(len(SCOPE_CASES))]
    active_state, hidden_state, lifecycle_reason = LIFECYCLE_CASES[
        rng.randrange(len(LIFECYCLE_CASES))
    ]
    scenario_id = f"SC-{index + 1:02d}-{profile}-{seed:04d}"
    context_id = stable_id("ctx", seed, index)
    task_id = stable_id("task", seed, index)
    item_id = stable_id("item", seed, index)
    hidden_id = stable_id("hidden", seed, index)
    proposal_id = stable_id("proposal", seed, index)
    idempotency_key = stable_id("idem", seed, index)
    action_state = ACTION_CASES[rng.randrange(len(ACTION_CASES))]
    failure_variant = choose(rng, FAILURE_CASES)
    purpose = {
        "local-single-user": "recall_private_note",
        "family-assistant": "prepare_shared_response",
        "consumer-companion": "bounded_companion_answer",
        "professional-copilot": "draft_project_update",
        "software-factory": "review_task_artifact",
        "organization-community": "coordinate_shared_request",
        "scientific-swarm": "verify_time_spatial_observation",
        "embodied-household": "propose_safe_household_action",
        "industrial-operations": "verify_facility_observation",
        "field-robotics": "reconcile_field_intervention",
        "reachy-mini": "recall_companion_preference_and_propose_expression",
        "reachy-2": "verify_manipulation_scene_before_proposal",
        "microduck": "reconcile_policy_action_and_observed_outcome",
    }[profile]
    case = {
        "scenario_id": scenario_id,
        "profile": profile,
        "seed": seed,
        "policy_version": "p1",
        "tenant": f"fixture:{profile}",
        "actors": [
            {"id": "human:owner", "role": "owner"},
            {"id": f"agent:{profile}", "role": "agent"},
            {"id": "service:policy", "role": "policy"},
            {"id": "service:executor", "role": "executor"},
        ],
        "situation": {
            "context_id": context_id,
            "observed_at": "2026-01-15T08:20:00+08:00",
            "logical_minute": 500,
            "time_zone": "Asia/Singapore",
            "location": "cafe/table1",
            "activity": "bounded_task",
            "nearby_entities": ["agent:counter", "object:order"],
            "commitments": ["complete_one_task"],
            "world_conditions": ["cafe:open"],
            "allowed_actions": ["draft", "recall", "remember"],
        },
        "recall_intent": {
            "task_id": task_id,
            "purpose": purpose,
            "memory_kinds": ["event", "memory", "decision"],
            "scopes": [include_scope],
            "max_items": 6,
            "max_bytes": 4096,
            "freshness_minutes": 180,
        },
        "seed_items": [
            {
                "id": item_id,
                "scope": include_scope,
                "lifecycle": active_state,
                "content": "Approved source fact for the bounded task.",
                "source_ref": f"event:{stable_id('event', seed, index)}",
            },
            {
                "id": hidden_id,
                "scope": hidden_scope,
                "lifecycle": hidden_state,
                "content": "Restricted source fact that must not enter this packet.",
                "source_ref": f"memory:{stable_id('memory', seed, index)}",
            },
        ],
        "action": {
            "proposal_id": proposal_id,
            "state": action_state,
            "idempotency_key": idempotency_key,
            "failure_variant": failure_variant,
        },
        "expected": {
            "included_item_ids": [item_id],
            "omitted_item_ids": [hidden_id],
            "omission_reasons": [omission, lifecycle_reason],
            "max_items": 6,
            "max_bytes": 4096,
            "no_model_authority": True,
            "no_external_action_before_approval": action_state in {"draft", "pending"},
            "deduplicate_on_retry": failure_variant in {"retry", "replay"},
        },
        "oracle": {
            "application_state": "matches_action_state",
            "lessdb_rows": [
                "session",
                "observation",
                "recall",
                "packet_snapshot",
                "decision",
                "provenance",
            ],
            "packet_visibility": "included_and_omitted_are_policy_consistent",
            "rendered_or_analyst_view": "matches_packet_and_application_state",
        },
    }
    if profile in PHYSICAL_PROFILES:
        case["situation"].update(
            {
                "location": "site:fixture/floor-1",
                "activity": "bounded_inspection",
                "nearby_entities": ["sensor:fixture", "asset:fixture"],
                "world_conditions": ["device:online", "interlock:armed"],
                "allowed_actions": ["observe", "propose_actuation"],
            }
        )
        case["simulator"] = SIMULATOR_PROFILES[profile]
        case["physical"] = {
            "observation_ids": [stable_id("observation", seed, index)],
            "sensor_id": f"sensor:{profile}:fixture",
            "coordinate_frame": "site:fixture/floor-1",
            "pose_mm": {"x": 12500, "y": 4200, "z": 0},
            "uncertainty": {"radius_mm": 80, "confidence_ppm": 970000},
            "evidence_ref": stable_id("evidence", seed, index),
            "safety_envelope": {
                "allowed_action_kinds": ["inspect", "move_to_safe_pose"],
                "required_approvals": ["safety:fixture"],
                "stop_conditions": ["observation_stale", "human_stop"],
            },
            "proposal_id": proposal_id,
            "idempotency_key": idempotency_key,
        }
        case["expected"]["physical_rows"] = [
            "observation",
            "proposal",
            "approval_or_denial",
            "executor_reconciliation",
        ]
        case["expected"]["no_actuation_before_approval"] = True
        case["oracle"]["physical_state"] = (
            "controller_and_reconciliation_match_approved_proposal"
        )
    return case


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed", type=int, default=20261009)
    parser.add_argument("--count", type=int, default=7)
    parser.add_argument("--profile", choices=PROFILES, action="append")
    parser.add_argument("--out", type=Path, default=Path("generated-agent-scenarios.json"))
    args = parser.parse_args()
    if args.count < 1 or args.count > 1000:
        parser.error("--count must be between 1 and 1000")
    profiles = tuple(args.profile) if args.profile else PROFILES
    scenarios = [build_case(args.seed, i, profiles[i % len(profiles)]) for i in range(args.count)]
    payload = {
        "schema": "lessdb-agent-scenario/v1",
        "seed": args.seed,
        "count": len(scenarios),
        "scenarios": scenarios,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"out": str(args.out), "schema": payload["schema"], "count": len(scenarios)}))


if __name__ == "__main__":
    main()
