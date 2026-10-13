#!/usr/bin/env bash
# One shared typed terminal/frontend contract; Rust owns time boundaries.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/measurement_cli.py" "$@"
