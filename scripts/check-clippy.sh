#!/bin/bash

set -euo pipefail

# Install clippy if not available
cargo clippy --version || rustup component add clippy

echo "Checking clippy"
cargo clippy --all-targets --all-features --no-deps -- -D warnings -A clippy::uninlined_format_args
echo
