#!/bin/bash

set -euo pipefail

echo "📦 Installing recommended development tools for WhiteNoise..."
echo ""

TOOLS=(
    "cargo-nextest:Faster parallel test runner"
    "cargo-audit:Security vulnerability scanner"
    "cargo-outdated:Check for outdated dependencies"
    "cargo-llvm-cov:Code coverage reporting"
    "cargo-msrv:Verify minimum Rust version"
    "cargo-deny:License and dependency checker"
    "cargo-expand:Macro expansion tool"
)

FAILED=()
SUCCESS=()

for tool_info in "${TOOLS[@]}"; do
    IFS=':' read -r tool desc <<< "$tool_info"
    echo "Installing $tool ($desc)..."

    if cargo install "$tool" --quiet 2>/dev/null; then
        SUCCESS+=("$tool")
        echo "✅ $tool installed successfully"
    else
        # Check if already installed
        if command -v "$tool" &> /dev/null; then
            SUCCESS+=("$tool")
            echo "✅ $tool (already installed)"
        else
            FAILED+=("$tool")
            echo "❌ Failed to install $tool"
        fi
    fi
    echo ""
done

echo "========================================"
echo "Installation Summary"
echo "========================================"
echo "✅ Successfully installed: ${#SUCCESS[@]}/${#TOOLS[@]}"
echo ""

if [ ${#FAILED[@]} -gt 0 ]; then
    echo "❌ Failed to install:"
    for tool in "${FAILED[@]}"; do
        echo "  - $tool"
    done
    echo ""
    echo "You can try installing failed tools manually with:"
    for tool in "${FAILED[@]}"; do
        echo "  cargo install $tool"
    done
    exit 1
else
    echo "🎉 All tools installed successfully!"
    echo ""
    echo "Run 'just --list' to see available development commands"
fi

