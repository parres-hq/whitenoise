#!/bin/bash

set -euo pipefail

# Install clippy if not available
cargo clippy --version || rustup component add clippy

echo "🔍 Running clippy checks..."
cargo clippy --all-targets --all-features --no-deps -- -D warnings -A clippy::uninlined_format_args

echo "✅ Clippy checks passed"
echo
