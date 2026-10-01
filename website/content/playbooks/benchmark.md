# Benchmark playbook

Use the benchmark tools present in this revision:

```sh
lessdb bench --rows 5000000
cargo build --release --locked --manifest-path bench/Cargo.toml -j 1
LESS_BIN=/path/to/lessdb ./bench/target/release/less-bench --sf 0.01
```

The standalone suite requires the main CLI binary for its MCP phase; build it
separately or pass `--skip-mcp`. Reports land in `bench/results/`.

Run only on an authorized, idle native benchmark host. Record the exact source
and binary hashes, hardware, concurrency, durability, cache state and settings.
Retain every sample and error; label small runs as smoke checks. Do not present
historical measurements or unlike workloads as a current performance ranking.
See [benchmarks](/docs/benchmarks) for the retained suite's scope.
