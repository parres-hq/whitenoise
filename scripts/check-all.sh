#!/bin/bash

set -euo pipefail

./scripts/check-fmt.sh
./scripts/check-docs.sh
./scripts/check-clippy.sh
