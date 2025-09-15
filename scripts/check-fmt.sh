#!/bin/bash

set -euo pipefail

flags=""

# Check if "check" is passed as an argument
if [[ "$#" -gt 0 && "$1" == "check" ]]; then
    flags="--check"
fi

# Install rustfmt if not available
cargo fmt --version || rustup component add rustfmt

echo "Checking fmt"

# Check workspace crates
cargo fmt --all -- --config format_code_in_doc_comments=true $flags

echo
