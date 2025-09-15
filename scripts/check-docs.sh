#!/bin/bash

set -euo pipefail

# Ensure rustdoc is available
rustdoc --version || rustup component add rust-docs

echo "Checking docs"
cargo doc --no-deps --all-features
echo
