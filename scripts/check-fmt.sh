#!/bin/bash

set -euo pipefail

flags=""

# Check if "check" is passed as an argument
if [[ "$#" -gt 0 && "$1" == "check" ]]; then
    flags="--check"
    echo "🔍 Checking code formatting..."
else
    echo "🔧 Applying code formatting..."
fi

# Install rustfmt if not available
cargo fmt --version || rustup component add rustfmt

# Check workspace crates
cargo fmt --all -- --config format_code_in_doc_comments=true $flags

if [[ -n "$flags" ]]; then
    echo "✅ Format check passed"
else
    echo "✅ Code formatted successfully"
fi
echo
