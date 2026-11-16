#!/usr/bin/env python3
"""Build the pinned Linux/amd64 agent and prepare its Cloudflare image context."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
CONTAINER = ROOT / "deploy/cloudflare/container"
# Official rust:1.96-bookworm manifest, pinned instead of a mutable tag.
BUILD_IMAGE = "rust@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663"
RUST_TOOLCHAIN = "1.96.1"
PLATFORM = "linux/amd64"


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def run(args, **kwargs):
    print("+ " + " ".join(map(str, args)), flush=True)
    return subprocess.run(args, check=True, **kwargs)


def source_files():
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
        yield ROOT / "backend/agentd" / name
    yield from sorted((ROOT / "backend/agentd/src").rglob("*.rs"))
    for path in sorted((ROOT / "vendor/lessdb").rglob("*")):
        if path.is_file() and "target" not in path.relative_to(ROOT / "vendor/lessdb").parts:
            yield path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jobs", type=int, default=1, help="Cargo jobs (default: 1 for 2 GB Docker VMs)")
    parser.add_argument("--image-tag", help="Also build this local runtime image after preparing its context")
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")

    data = CONTAINER / "data"
    unexpected = [p for p in data.iterdir()
                  if p.name != ".keep" or not p.is_file() or p.is_symlink() or p.stat().st_size]
    if unexpected:
        raise SystemExit("Refusing to package runtime data: container/data must contain only an empty .keep")

    staging = ROOT / ".local/cloudflare-build"
    staging.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="source-", dir=staging) as temp:
        work = Path(temp)
        manifest = {}
        for source in source_files():
            relative = source.relative_to(ROOT)
            destination = work / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            content = source.read_bytes()
            destination.write_bytes(content)
            manifest[relative.as_posix()] = sha256(content)
        provenance = json.loads((work / "vendor/lessdb/PROVENANCE.json").read_text())
        if provenance["schema"] != "lessville-vendored-source/v1":
            raise SystemExit("Unsupported vendored engine provenance schema")
        for entry in provenance["files"]:
            if manifest.get("vendor/lessdb/" + entry["path"]) != entry["sha256"]:
                raise SystemExit("Vendored engine differs from its provenance: " + entry["path"])
        source_sha = sha256(json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode())
        output = work / "output"
        output.mkdir()
        print(f"Frozen {len(manifest)} source inputs: sha256={source_sha}", flush=True)
        run([
            "docker", "run", "--rm", "--platform", PLATFORM,
            "--mount", f"type=bind,source={work},target=/work,readonly",
            "--mount", f"type=bind,source={output},target=/output",
            "--mount", "type=volume,source=lessville-cargo-registry,target=/usr/local/cargo/registry",
            "--mount", "type=volume,source=lessville-linux-amd64-target,target=/target",
            "--workdir", "/work/backend/agentd",
            "--env", "CARGO_TARGET_DIR=/target", "--env", "CARGO_INCREMENTAL=0",
            "--env", f"RUSTUP_TOOLCHAIN={RUST_TOOLCHAIN}",
            "--env", "RUSTFLAGS=--remap-path-prefix=/work=.",
            BUILD_IMAGE, "sh", "-eu", "-c",
            'rustc --version; cargo build --release --locked --jobs "$1"; '
            'cp /target/release/lessville-agentd /output/lessville-agentd',
            "build", str(args.jobs),
        ])
        binary = (output / "lessville-agentd").read_bytes()
        # ELF64, little endian, EM_X86_64. Reject a macOS or ARM artifact before packaging.
        if binary[:6] != b"\x7fELF\x02\x01" or int.from_bytes(binary[18:20], "little") != 62:
            raise SystemExit("Build output is not a Linux x86-64 ELF executable")
        binary_sha = sha256(binary)
        chunk_size = (len(binary) + 23) // 24
        parts = work / "bin-parts"
        parts.mkdir()
        for index in range(24):
            (parts / f"part-{index:02}").write_bytes(binary[index * chunk_size:(index + 1) * chunk_size])
        if sha256(b"".join(path.read_bytes() for path in sorted(parts.iterdir()))) != binary_sha:
            raise SystemExit("Chunk reconstruction checksum mismatch")

        old_parts = CONTAINER / "bin-parts"
        if old_parts.exists():
            shutil.rmtree(old_parts)
        shutil.copytree(parts, old_parts)
        provenance_sha = manifest["vendor/lessdb/PROVENANCE.json"]
        marker = (
            "lessville runtime checkpoint=300s archive=168h instances=1\n"
            f"build platform={PLATFORM} rust={RUST_TOOLCHAIN} image={BUILD_IMAGE}\n"
            f"backend source sha256={source_sha} vendor-provenance sha256={provenance_sha} "
            "checkpoint-prefix=lessville/v3\n"
            f"binary sha256={binary_sha}\n"
        )
        (CONTAINER / "release-marker.txt").write_text(marker)
        (staging / "build-manifest.json").write_text(json.dumps({
            "schema": "lessville-linux-build/v1", "platform": PLATFORM,
            "build_image": BUILD_IMAGE, "rust_toolchain": RUST_TOOLCHAIN, "source_sha256": source_sha,
            "binary_sha256": binary_sha, "binary_bytes": len(binary),
            "sources": manifest,
        }, indent=2, sort_keys=True) + "\n")
        print(f"Prepared 24 chunks, {len(binary)} bytes, sha256={binary_sha}", flush=True)
        print(f"Build manifest: {staging / 'build-manifest.json'}", flush=True)

    if args.image_tag:
        run(["docker", "build", "--platform", PLATFORM, "--tag", args.image_tag, str(CONTAINER)])


if __name__ == "__main__":
    main()
