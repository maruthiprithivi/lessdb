# Contributing to LessDB

LessDB is an experimental database exploring long-term agent knowledge, memory
persistence and cross-agent communication. Collaboration is welcome: bring use
cases, designs, bug reports, documentation and reproducible tests. Correctness,
durability and access boundaries take priority over performance. The source
snapshot has known gaps; the README status is authoritative about maturity.

## Setup and validation

Install Git, a native linker/build toolchain, and rustup. The repository pins
Rust 1.96 with rustfmt and Clippy. Python 3 builds the static website; Node 22
runs its dependency-free star-counter tests. Run builds/tests on your isolated
Linux development or CI host, with throwaway data directories. The project’s
maintainer uses Optimus rather than the local Mac for validation.

```sh
git clone https://lessdb.dev/.git
cd lessdb
cargo build --locked -j2
cargo fmt --all --check
cargo test --locked --workspace -j2
cargo clippy --locked --workspace --all-targets -j2 -- -D warnings
cargo run --release --locked -p less-sqllogictest -j2 -- tests/sqllogictest
python3 scripts/build-site.py
node --test website/tests/github-stars.test.cjs
```

These are contribution gates, not a claim that this baseline has passed every
gate. Report exact commands, commit, platform, failures and skipped tests.
Use `--offline` only after dependencies have been fetched. Keep CPU/memory
budgets explicit on shared hosts. Do not run benchmarks during other builds.

## Architecture and conventions

Read [architecture](docs/ARCHITECTURE.md), [design decisions](docs/DESIGN-DECISIONS.md)
and [roadmap](docs/ROADMAP.md). The Rust workspace separates catalog, storage,
engine, DataFusion query integration, CLI and server interfaces. `bench/` is
a standalone Cargo package; `scripts/build-site.py` generates documentation
pages from the source content under `website/content/`. Preserve generated
page/source agreement.

Follow existing Rust APIs and formatting. Prefer small changes, explicit error
propagation and bounded resource use. Avoid duplicate abstractions, heavyweight
services and new dependencies without a demonstrated need. Never bypass
checks, fsync barriers, isolation or result validation to improve timing.

## Tests and performance

Add regression tests for the actual failure, including negative access cases,
concurrency, malformed input and recovery when relevant. Process-exit tests
do not prove physical power-loss safety. Document acknowledgement, fsync,
transaction and isolation semantics rather than claiming unsupported ACID.

The independent small-scale analytical benchmark is built separately:

```sh
cargo build --release --locked --manifest-path bench/Cargo.toml -j2
bench/target/release/less-bench --sf 0.01
```

Inspect its help and source before increasing scale. Benchmark comparisons
must pin both engine versions/commits, logical types/results, data seed/hash,
hardware, threads, memory/spill budgets, persistence/durability, session and
output behavior. Separate warm from cold, retain every run and report variance,
unsupported queries, failures and resource usage. Do not clear shared caches
or compare unlike hardware. Include workloads unfavorable to LessDB.
Simultaneous ingestion/query tests need offered and achieved rates, per-client
p95/p99, bounded queues, fairness, correctness and recovery after overload.
A microbenchmark improvement is not proof of an end-to-end agent benefit.

## Issues and pull requests

Search existing issues first. Describe a reproducible problem, expected/actual
behavior, minimal input, exact commit/platform and acceptance tests. Never
include credentials or private data. Discuss changes to storage format,
durability, access scope, APIs or dependencies before a broad implementation.

Create a focused branch and PR linking the issue. Explain behavior, validation
receipts, footprint and compatibility risks. Clearly identify stacked PR
dependencies. Do not report unmerged work as a feature of main. Keep patches
reviewable and benchmark evidence reproducible; preserve third-party licenses.

## Vulnerability reporting

Do not post secrets or exploitable vulnerability details in a public issue.
A dedicated private reporting channel has not yet been verified for this fresh
repository. Contact the maintainer through their verified GitHub profile to
arrange a private channel before sending sensitive details. This guide does
not invent an email address or promise a response deadline.

## License

Contributions to LessDB follow the existing [MIT License](LICENSE). The canonical
copyright notice remains `Copyright (c) 2025-2026 LessDB Project and LessDB
contributors`.

Forks and redistributions that contain copies or substantial portions of LessDB
must retain its copyright and MIT permission notices. Keep the original `LICENSE`
file with the covered code; do not replace or remove its notices. MIT permits
commercial use, modification, distribution and sublicensing. It does not require
your separate additions or entire fork to be MIT-licensed, nor require publishing
your changes. This guide adds no license restrictions.

Dependencies keep their own licenses; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
