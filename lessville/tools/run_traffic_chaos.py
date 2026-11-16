#!/usr/bin/env python3
"""Exercise LessVille traffic actors and signals at multiple simulation speeds.

This is a local, observer-safe chaos harness. It starts a fresh private backend,
changes only its local clock speed, and checks route bounds, identity stability,
safe gaps and red/amber junction boundaries. It is not a capacity benchmark.
"""
from __future__ import annotations

import argparse
import json
import os
import socket
import subprocess
import shutil
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def request(base: str, path: str, method: str = "GET", payload: dict | None = None) -> dict:
    data = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(
        base + path,
        data=data,
        headers={"content-type": "application/json"},
        method=method,
    )
    with urllib.request.urlopen(req, timeout=8) as response:
        return json.load(response)


def wait_ready(base: str) -> None:
    for _ in range(60):
        try:
            request(base, "/api/health")
            return
        except (OSError, urllib.error.URLError):
            time.sleep(0.15)
    raise RuntimeError("LessVille backend did not become ready")


def validate_snapshot(snapshot: dict, previous: dict | None) -> tuple[dict, dict | None]:
    traffic = snapshot.get("traffic", [])
    lights = snapshot.get("traffic_lights", [])
    background = snapshot.get("background", [])
    assert len(traffic) >= 4, "traffic population disappeared"
    assert len(lights) >= 2, "traffic-light population disappeared"
    assert len({actor["id"] for actor in traffic}) == len(traffic), "duplicate traffic identity"
    assert len({person["id"] for person in background}) == len(background), "duplicate background identity"
    assert all(light["phase"] in {"green", "amber", "red"} for light in lights)
    assert all(0.0 <= actor["position"] <= 100.0 for actor in traffic)
    assert all(actor["direction"] in {"east", "west"} for actor in traffic)
    assert all(
        actor["phase"]
        in {"cruising", "turning_at_junction", "yielding_safe_gap", "stopped_at_red_light"}
        for actor in traffic
    )

    main_phase = next(light["phase"] for light in lights if light["id"] == "signal-main-east-west")
    prior_by_id = {actor["id"]: actor for actor in (previous or {}).get("traffic", [])}
    for actor in traffic:
        prior = prior_by_id.get(actor["id"])
        if prior and main_phase in {"red", "amber"}:
            crossed = (prior["position"] < 50.0 <= actor["position"]) or (
                prior["position"] > 50.0 >= actor["position"]
            )
            assert not crossed, f"{actor['id']} crossed a {main_phase} junction"
        if actor["phase"] == "stopped_at_red_light":
            assert main_phase in {"red", "amber"}

    for left_index, left in enumerate(traffic):
        for right in traffic[left_index + 1 :]:
            if left["lane"] == right["lane"] and left["direction"] == right["direction"]:
                assert abs(left["position"] - right["position"]) >= 0.01

    identities = {"traffic": sorted(actor["id"] for actor in traffic), "background": sorted(person["id"] for person in background)}
    if previous is not None:
        assert identities == previous["identities"], "actor identity changed during chaos run"
    return identities, {"traffic": traffic, "identities": identities}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--seconds-per-speed", type=float, default=3.0)
    args = parser.parse_args()
    if not args.binary.is_file():
        raise SystemExit(f"backend binary not found: {args.binary}")

    port = free_port()
    data_dir = Path(tempfile.mkdtemp(prefix="lessville-traffic-chaos-"))
    env = os.environ.copy()
    env.update(
        {
            "LESSVILLE_DATA": str(data_dir),
            "LESSVILLE_PORT": str(port),
            "LESSVILLE_PUBLIC": "0",
            "LESSVILLE_LIVE_WEATHER": "0",
        }
    )
    process = subprocess.Popen(
        [str(args.binary)],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    base = f"http://127.0.0.1:{port}"
    speeds = [1, 4, 8]
    samples = 0
    max_minute = 0
    identities: dict | None = None
    previous_state: dict | None = None
    try:
        wait_ready(base)
        for speed in speeds:
            request(base, "/api/control", "POST", {"action": "speed", "speed": speed})
            end = time.monotonic() + max(1.0, args.seconds_per_speed)
            while time.monotonic() < end:
                state = request(base, "/api/state")
                identities, previous_state = validate_snapshot(state, previous_state)
                max_minute = max(max_minute, int(state["clock"]["minute"]))
                samples += 1
                time.sleep(0.2)
        print(
            json.dumps(
                {
                    "ok": True,
                    "speeds_tested": speeds,
                    "samples": samples,
                    "max_logical_minute": max_minute,
                    "traffic_actors": identities["traffic"] if identities else [],
                    "background_actors": identities["background"] if identities else [],
                    "checks": [
                        "bounded route positions",
                        "stable actor identities",
                        "valid signal phases",
                        "no red/amber junction crossing",
                        "same-lane separation",
                    ],
                },
                sort_keys=True,
            )
        )
        return 0
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
        shutil.rmtree(data_dir, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
