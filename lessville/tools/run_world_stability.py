#!/usr/bin/env python3
"""Run the real LessVille service at x4 and x8 and check world invariants."""
from __future__ import annotations

import argparse
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

PROJECT_ROOT = pathlib.Path(__file__).resolve().parents[1]


def request(base: str, method: str, path: str, payload: dict | None = None) -> dict:
    body = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(
        base + path,
        data=body,
        method=method,
        headers={"content-type": "application/json", "user-agent": "LessVille-local-stability"},
    )
    with urllib.request.urlopen(req, timeout=5) as response:
        return json.load(response)


def wait_for_health(base: str, process: subprocess.Popen[bytes]) -> None:
    for _ in range(50):
        if process.poll() is not None:
            raise RuntimeError(f"LessVille child exited before health check: {process.returncode}")
        try:
            if request(base, "GET", "/api/health").get("ok"):
                return
        except (OSError, urllib.error.URLError):
            time.sleep(0.2)
    raise RuntimeError("LessVille service did not become healthy")


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


def check_snapshot(snapshot: dict, previous_minute: int | None) -> tuple[int, set[str]]:
    minute = int(snapshot["clock"]["minute"])
    if previous_minute is not None and minute < previous_minute:
        raise AssertionError(f"clock moved backwards: {previous_minute} -> {minute}")
    traffic = snapshot.get("traffic", [])
    ids = [str(v["id"]) for v in traffic]
    if len(ids) != len(set(ids)):
        raise AssertionError("traffic IDs are not unique")
    for vehicle in traffic:
        position = float(vehicle["position"])
        if not 0.0 <= position <= 100.0:
            raise AssertionError(f"traffic position escaped route: {vehicle}")
        if vehicle["phase"] not in {"cruising", "yielding_safe_gap", "turning_at_junction"}:
            raise AssertionError(f"unknown traffic phase: {vehicle['phase']}")
    background = snapshot.get("background", [])
    background_ids = [str(v["id"]) for v in background]
    if len(background_ids) != len(set(background_ids)):
        raise AssertionError("background IDs are not unique")
    if len(background_ids) < 8:
        raise AssertionError("background population disappeared")
    return minute, {str(snapshot["clock"].get("weather_source", ""))}


def run_speed(binary: pathlib.Path, port: int, speed: int, seconds: float) -> dict:
    with tempfile.TemporaryDirectory(prefix="lessville-stability-") as data_dir:
        env = os.environ.copy()
        env.update({"LESSVILLE_DATA": data_dir, "LESSVILLE_PORT": str(port), "LESSVILLE_PUBLIC": "0", "LESSVILLE_LIVE_WEATHER": "0"})
        process = subprocess.Popen([str(binary)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        base = f"http://127.0.0.1:{port}"
        try:
            wait_for_health(base, process)
            request(base, "POST", "/api/control", {"action": "speed", "speed": speed})
            start = time.monotonic()
            previous = None
            weather_sources: set[str] = set()
            samples = 0
            while time.monotonic() - start < seconds:
                snapshot = request(base, "GET", "/api/state")
                previous, sources = check_snapshot(snapshot, previous)
                weather_sources |= sources
                samples += 1
                time.sleep(0.35)
            if weather_sources != {"seeded-fallback"}:
                raise AssertionError(f"unexpected weather source set: {sorted(weather_sources)}")
            return {"speed": speed, "samples": samples, "last_minute": previous, "weather_sources": sorted(weather_sources)}
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        default=pathlib.Path(os.environ.get("LESSVILLE_BINARY", PROJECT_ROOT / "backend/agentd/target/debug/lessville-agentd")),
        help="path to a locally built lessville-agentd executable (or set LESSVILLE_BINARY)",
    )
    parser.add_argument("--seconds", type=float, default=14.0)
    parser.add_argument("--port", type=int, default=8793)
    args = parser.parse_args()
    if not args.binary.exists():
        raise SystemExit(f"missing binary: {args.binary}")
    results = [run_speed(args.binary, free_port(), speed, args.seconds) for speed in (4, 8)]
    print(json.dumps({"ok": True, "runs": results}, indent=2))


if __name__ == "__main__":
    main()
