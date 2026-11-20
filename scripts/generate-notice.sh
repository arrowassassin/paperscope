#!/bin/bash
# Generate SPDX-compliant NOTICE file from cargo metadata
# This script wraps the Python generator for CI integration

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

# Run the Python generator
python3 "$SCRIPT_DIR/generate-notice.py"