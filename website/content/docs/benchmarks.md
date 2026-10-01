# Benchmarks

The repository includes a deterministic TPC-H-inspired benchmark suite in
`bench/` and the CLI throughput smoke command `lessdb bench`. These are local
baseline tools; they do not establish a fair cross-engine performance ranking.

## Run the retained suite

```sh
cargo build --release --locked -j 1
cargo build --release --locked --manifest-path bench/Cargo.toml -j 1
LESS_BIN="$PWD/target/release/lessdb" ./bench/target/release/less-bench --sf 0.01
```

The suite uses seeded generated data and throwaway directories. It measures
analytical queries, storage, memory, restart/shared-storage checks, vector and
graph workloads, interactive queries and MCP calls. Reports are written to
`bench/results/latest.md` and `bench/results/results.json`.

The [benchmark playbook](/playbooks/benchmark) explains reproducibility.
Historical imported-workload reports and their pipeline have been removed from
the current tree. Preserve exact revisions, settings, raw samples and errors in
new reports; separate smoke correctness from sustained throughput evidence.
