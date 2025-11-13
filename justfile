######################
# Development
######################

# Default recipe - show available commands
default:
    @just --list

# Clear local data dirs (ensure you're not running docker compose before running)
clear-dev-data:
    rm -rf ./dev/data/*

# Run integration_test binary using the local relays and data dirs
int-test:
    rm -rf ./dev/data/integration_test/
    RUST_LOG=debug,\
    sqlx=info,\
    refinery_core=error,\
    keyring=info,\
    nostr_relay_pool=error,\
    mdk_sqlite_storage=error,\
    tungstenite=error,\
    integration_test=debug \
    cargo run --bin integration_test --features integration-tests -- --data-dir ./dev/data/integration_test/ --logs-dir ./dev/data/integration_test/

# Run integration_test binary with flamegraph to analyze performance
int-test-flamegraph:
    rm -rf ./dev/data/integration_test/ cargo-flamegraph.trace flamegraph.svg
    RUST_LOG=debug,\
    sqlx=info,\
    refinery_core=error,\
    keyring=info,\
    nostr_relay_pool=error,\
    mdk_sqlite_storage=error,\
    tungstenite=error,\
    integration_test=debug \
    CARGO_PROFILE_RELEASE_DEBUG=true \
    CARGO_PROFILE_RELEASE_LTO=false \
    CARGO_PROFILE_RELEASE_STRIP=false \
    cargo flamegraph --release --bin integration_test --features integration-tests --deterministic -- --data-dir ./dev/data/integration_test/ --logs-dir ./dev/data/integration_test/

# Run performance benchmarks (not included in CI)
benchmark:
    rm -rf ./dev/data/benchmark_test/
    RUST_LOG=warn,benchmark_test=info,whitenoise=info \
    cargo run --bin benchmark_test --features benchmark-tests --release -- --data-dir ./dev/data/benchmark_test/ --logs-dir ./dev/data/benchmark_test/
    rm -rf ./dev/data/benchmark_test/

# Run benchmarks and save results with timestamp
benchmark-save:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p ./benchmark_results
    rm -rf ./dev/data/benchmark_test/
    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    echo "Running benchmarks and saving to ./benchmark_results/benchmark_${TIMESTAMP}.log"
    RUST_LOG=warn,benchmark_test=info,whitenoise=info \
    cargo run --bin benchmark_test --features benchmark-tests --release -- \
    --data-dir ./dev/data/benchmark_test/ \
    --logs-dir ./dev/data/benchmark_test/ \
    | tee ./benchmark_results/benchmark_${TIMESTAMP}.log
    rm -rf ./dev/data/benchmark_test/
    echo "Results saved to ./benchmark_results/benchmark_${TIMESTAMP}.log"

# Run all tests (unit tests, integration tests, and doc tests)
# Uses nextest for faster parallel execution if available, falls back to cargo test
test:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v cargo-nextest &> /dev/null; then
        echo "Running tests with nextest (parallel)..."
        cargo nextest run --all-features --all-targets
        cargo test --all-features --doc
    else
        echo "Running tests with cargo test..."
        echo "Tip: Install cargo-nextest for faster parallel testing: just install-tools"
        cargo test --all-features --all-targets
        cargo test --all-features --doc
    fi

# Run tests with standard cargo test (slower, sequential)
test-cargo:
    cargo test --all-features --all-targets
    cargo test --all-features --doc

# Check clippy
check-clippy:
    @bash scripts/check-clippy.sh

# Fix clippy issues automatically where possible
fix-clippy:
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged

# Check fmt
check-fmt:
    @bash scripts/check-fmt.sh check

# Apply formatting
fmt:
    @bash scripts/check-fmt.sh

# Check docs
check-docs:
    @bash scripts/check-docs.sh

# Check all (fast checks before running tests)
check:
    @bash scripts/check-all.sh

# Run pre-commit checks (linting, formatting, docs, tests, integration tests)
precommit: check test int-test

# Quick pre-commit (skip integration tests)
precommit-quick: check test

# Check for outdated dependencies
outdated:
    cargo outdated --workspace --root-deps-only

# Update dependencies
update:
    cargo update

# Audit dependencies for security vulnerabilities
# Ignoring:
# - RUSTSEC-2023-0071: RSA Marvin Attack (transitive via sqlx-mysql, not used in our SQLite-only app)
# - RUSTSEC-2024-0384: instant unmaintained (transitive via rust-nostr, low risk)
audit:
    cargo audit --ignore RUSTSEC-2023-0071 --ignore RUSTSEC-2024-0384

# Generate and open documentation
doc:
    cargo doc --no-deps --all-features --open

# Clean build artifacts and data
clean:
    cargo clean
    rm -rf ./dev/data/*

# Force running tests with nextest (requires nextest to be installed)
test-nextest:
    cargo nextest run --all-features --all-targets
    cargo test --all-features --doc

# Generate code coverage report
coverage:
    cargo llvm-cov clean --workspace
    cargo llvm-cov --workspace --lcov --output-path lcov.info
    @echo "Coverage report: lcov.info"

# Generate HTML code coverage report
coverage-html:
    cargo llvm-cov clean --workspace
    cargo llvm-cov --workspace --html
    @echo "HTML report: target/llvm-cov/html/index.html"

# Check minimum supported Rust version
check-msrv:
    cargo msrv verify

# Run cargo-deny checks (licenses, security, sources)
deny-check:
    cargo deny check

# Install recommended development tools
install-tools:
    @bash scripts/install-dev-tools.sh

######################
# Build
######################

# Build release binary
build-release:
    cargo build --release

######################
# Docker
######################

# Start docker compose services
docker-up:
    docker compose up -d

# Stop docker compose services
docker-down:
    docker compose down -v

# Show docker compose logs
docker-logs:
    docker compose logs -f

######################
# Utilities
######################

# Publish a NIP-89 handler
publish-nip89:
    ./scripts/publish_nip89_handler.sh
