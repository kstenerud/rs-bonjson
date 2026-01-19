.PHONY: bench quickbench test build clean check fmt clippy all profile profile-bench

# Run criterion benchmarks (rigorous, with statistical analysis)
bench:
	cargo bench

# Run quick benchmark comparison (faster, good for iteration)
quickbench:
	cargo run --release --example quick_bench

# Run all tests
test:
	cargo test

# Build in release mode
build:
	cargo build --release

# Check for compile errors without building
check:
	cargo check

# Format code
fmt:
	cargo fmt

# Run clippy lints
clippy:
	cargo clippy -- -D warnings

# Clean build artifacts
clean:
	cargo clean

# Run all checks (useful before committing)
all: fmt clippy test

# Profile quick benchmark and open in profiler UI
# Requires: cargo install samply
profile:
	cargo build --release --example quick_bench
	samply record ./target/release/examples/quick_bench

# Profile criterion benchmarks
profile-bench:
	cargo build --release --bench comparison
	samply record ./target/release/deps/comparison-* --bench
