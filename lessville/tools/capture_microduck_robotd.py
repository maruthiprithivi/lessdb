#!/usr/bin/env python3
"""Capture bounded events from the official MicroDuck robotd --sim path into LessDB.

This is a local experiment bridge. It samples the MuJoCo body socket while sending a
short semantic robot.move intent through the real robotd IPC socket. LessDB receives
provenance and bounded state; it is not in the control loop.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import socket
import time
import urllib.request
from datetime import datetime, timezone
from pathlib import Path


def post_json(url: str, payload: dict) -> dict:
    request = urllib.request.Request(
        url,
        data=json.dumps(payload, separators=(",", ":")).encode(),
        headers={"content-type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.load(response)


def rpc(robot_socket: str, method: str, params: dict) -> dict:
    with socket.socket(socket.AF_UNIX) as sock:
        sock.settimeout(3)
        sock.connect(robot_socket)
        stream = sock.makefile("rw")
        stream.write(
            json.dumps(
                {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
            )
            + "\n"
        )
        stream.flush()
        line = stream.readline()
        return json.loads(line) if line else {}


def body_read(host: str, port: int) -> dict:
    with socket.socket() as sock:
        sock.settimeout(3)
        sock.connect((host, port))
        stream = sock.makefile("rw")
        stream.write(json.dumps({"op": "hello", "protocol": 1, "joints": 15}) + "\n")
        stream.flush()
        hello = json.loads(stream.readline())
        stream.write('{"op":"read"}\n')
        stream.flush()
        state = json.loads(stream.readline())
    return {"hello": hello, "state": state}


def evidence_hash(value: dict) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--robot-socket", required=True)
    parser.add_argument("--body-host", default="127.0.0.1")
    parser.add_argument("--body-port", type=int, default=7810)
    parser.add_argument("--lessdb-url", default="http://127.0.0.1:8792")
    parser.add_argument("--duration", type=float, default=4.0)
    parser.add_argument("--sample-hz", type=float, default=5.0)
    parser.add_argument("--episode-id", default="")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()

    episode_id = args.episode_id or f"microduck-robotd:{int(time.time())}"
    interval = 1.0 / max(1.0, min(args.sample_hz, 10.0))
    samples: list[dict] = []
    started = datetime.now(timezone.utc).isoformat()

    # The action is sent to the real daemon IPC surface. LessDB only receives the
    # resulting observations and a proposal/reconciliation record after sampling.
    rpc(args.robot_socket, "robot.move", {"vx": 0.15, "vy": 0.0, "vyaw": 0.0})
    deadline = time.monotonic() + max(1.0, min(args.duration, 30.0))
    sequence = 0
    try:
        while time.monotonic() < deadline:
            captured_at = datetime.now(timezone.utc).isoformat()
            raw = body_read(args.body_host, args.body_port)
            state = raw.get("state", {})
            bounded = {
                "sim_time": state.get("sim_time"),
                "trunk_z": state.get("trunk_z"),
                "imu": state.get("imu"),
                "read_keys": sorted(state.keys()),
            }
            observation_id = f"{episode_id}:obs:{sequence}"
            proposal_id = f"{episode_id}:proposal:{sequence}"
            envelope = {
                "schema": "lessdb-physical-envelope/v1",
                "profile": "microduck-robotd-sim",
                "simulator": "pollen-microduck-rl-mujoco",
                "control_interface": "pollen-microduck-robotd-sim-ipc",
                "episode_id": episode_id,
                "situation": {
                    "context_id": f"{episode_id}:context",
                    "logical_time": sequence,
                    "location": "simulated-floor",
                },
                "observation": {
                    "observation_id": observation_id,
                    "sensor_id": "microduck:robotd-sim-body",
                    "kind": "robotd_policy_state",
                    "observed_at": captured_at,
                    "sequence": sequence,
                    "evidence_hash": evidence_hash(bounded),
                    "payload_ref": f"local://{episode_id}/state/{sequence}",
                    "bounded_state": bounded,
                },
                "provenance": {
                    "source": "official-robotd-sim",
                    "daemon_event": "robot.move -> body.read",
                    "policy_observation": "real daemon loop output; not model reasoning",
                },
                "proposal": {
                    "proposal_id": proposal_id,
                    "action_kind": "observe_policy_state",
                    "state": "draft",
                    "idempotency_key": f"{episode_id}:idem:{sequence}",
                    "source_observation_ids": [observation_id],
                },
            }
            replay = post_json(args.lessdb_url.rstrip("/") + "/api/physical/replay", envelope)
            reconciliation = post_json(
                args.lessdb_url.rstrip("/") + "/api/physical/reconcile",
                {
                    "attempt_id": f"{episode_id}:attempt:{sequence}",
                    "proposal_id": proposal_id,
                    "episode_id": episode_id,
                    "state": "succeeded",
                    "observed_at": captured_at,
                    "sim_minute": sequence,
                    "error_code": None,
                    "executor_id": "microduck:robotd-sim",
                    "actual_target": "simulated-policy-state",
                },
            )
            samples.append(
                {
                    "sequence": sequence,
                    "observation_id": observation_id,
                    "proposal_id": proposal_id,
                    "sim_time": state.get("sim_time"),
                    "trunk_z": state.get("trunk_z"),
                    "replay": replay,
                    "reconciliation": reconciliation,
                }
            )
            sequence += 1
            time.sleep(interval)
    finally:
        # Let the daemon's intent expiry stop motion rather than leaving a command active.
        try:
            rpc(args.robot_socket, "robot.move", {"vx": 0.0, "vy": 0.0, "vyaw": 0.0})
        except Exception:
            pass

    report = {
        "episode_id": episode_id,
        "started_at": started,
        "finished_at": datetime.now(timezone.utc).isoformat(),
        "control_boundary": "robotd and MuJoCo own control; LessDB receives sampled provenance only",
        "samples": len(samples),
        "first": samples[0] if samples else None,
        "last": samples[-1] if samples else None,
    }
    encoded = json.dumps(report, indent=2, sort_keys=True)
    if args.report:
        args.report.write_text(encoded + "\n")
    print(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
