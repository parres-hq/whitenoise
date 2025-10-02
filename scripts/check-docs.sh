#!/bin/bash

set -euo pipefail

# Ensure rustdoc is available
rustdoc --version || rustup component add rust-docs

echo "📚 Checking documentation..."
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

echo "✅ Documentation check passed"
echo
