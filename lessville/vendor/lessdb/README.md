# Lessville's pinned embedded engine

This directory contains the nine local crates required by Lessville's native
backend, including their tests. They come from the qualified reconstructed
private archive at commit `0367d81d935a452f28cb2a15a62ecce2463698b1` (archive
baseline `84ae452`, with reconstructed `less-agent` contracts). They are an
isolated application dependency, not a replacement for the parent repository's
public LessDB engine or a claim that the two formats are interchangeable.

[PROVENANCE.json](PROVENANCE.json) records every copied file's SHA-256 and its
upstream committed SHA-256. The source checkout had one existing uncommitted
change, `crates/less-agent/src/lib.rs`; its exact working bytes are included,
and [UPSTREAM-WORKTREE.patch](UPSTREAM-WORKTREE.patch) preserves the full diff.
That diff expands formatting and adds optional trailing commas. The vendored
`Cargo.toml` restricts workspace membership to these nine crates while retaining
the source package, dependency and build profile metadata. All other copied
files remain unchanged. The original [MIT license](LICENSE) is included.

Lessville's `backend/agentd/Cargo.toml` points directly here. Its existing
`Cargo.lock` controls registry dependencies; vendoring does not upgrade them.
Build from `backend/agentd` with `cargo +1.96 build --locked`. The parent
`vendor/.gdignore` excludes this native source from Godot resource import and
Web exports.

The dependency preserves the locally tested engine bytes and storage format.
It does not independently prove compatibility with the current deployed R2
checkpoint: restore a copy using the release binary before rollout. Keep the
existing checkpoint namespace and preserve its history; changing dependency
paths must not seed or replace the live world. The current engine supports a
single local writer, not a distributed shared-disk database.
