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

# Run all tests (unit tests, integration tests, and doc tests)
test:
    cargo test --all-features --all-targets
    cargo test --all-features --doc

# Check clippy
check-clippy:
    @bash scripts/check-clippy.sh

# Check fmt
check-fmt:
    @bash scripts/check-fmt.sh

# Check docs
check-docs:
    @bash scripts/check-docs.sh

# Check all
check:
    @bash scripts/check-all.sh

# Run pre-commit checks (linting, formatting, docs, tests, integration tests)
precommit: check test int-test


######################
# Build
######################



######################
# Utilities
######################

# Publish a NIP-89 handler
publish-nip89:
    ./scripts/publish_nip89_handler.sh
