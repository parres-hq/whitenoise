#!/usr/bin/env bash
set -e

# Extract coverage percentage from lcov.info file
# Usage: ./extract-coverage.sh [lcov-file]

LCOV_FILE="${1:-lcov.info}"

if [ ! -f "$LCOV_FILE" ]; then
    echo "Error: LCOV file not found: $LCOV_FILE" >&2
    exit 1
fi

# Extract coverage using LH (Lines Hit) and LF (Lines Found) for accuracy
LINES_HIT=$(grep -E "^LH:" "$LCOV_FILE" | awk -F: '{sum+=$2} END {print sum}')
TOTAL_LINES=$(grep -E "^LF:" "$LCOV_FILE" | awk -F: '{sum+=$2} END {print sum}')

if [ -z "$TOTAL_LINES" ] || [ "$TOTAL_LINES" -eq 0 ]; then
    echo "0.0"
    exit 0
fi

# Calculate percentage with 2 decimal places
COVERAGE=$(awk "BEGIN {printf \"%.2f\", ($LINES_HIT / $TOTAL_LINES) * 100}")

echo "$COVERAGE"
