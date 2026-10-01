# LessDB development tasks

.PHONY: build release test lint fmt clean install bench

build:
	cargo build

release:
	cargo build --release

# Full build including server, MCP and GPU crates
build-all:
	cargo build --workspace --features less-cli/gpu

test:
	cargo test

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt

clean:
	cargo clean

# Reclaim build artifacts/caches while keeping the fast debug cache
clean-artifacts:
	scripts/cleanup.sh

# Everything, including the debug build cache
clean-all:
	scripts/cleanup.sh --aggressive

install:
	cargo install --path crates/less-cli

bench:
	cargo run --release -p less-cli -- bench --rows 5000000

.PHONY: sdk-python sdk-node wheel

sdk-python:
	cd sdks/python && maturin build --release

sdk-node:
	cd sdks/node && npm install && npx napi build --platform --release

wheel: sdk-python
	@ls sdks/python/target/wheels/*.whl

.PHONY: bench-suite

bench-suite:
	cargo build --release --features less-cli/cloud,less-cli/gpu
	cargo build --release --manifest-path bench/Cargo.toml
	./bench/target/release/less-bench --sf 0.01
