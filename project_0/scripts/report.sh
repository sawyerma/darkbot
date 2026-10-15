#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# --run ID|PATH is located across all run roots and checked for layout V1 before the pinned readers run.
exec python3 "$DIR/run_store.py" exec "$DIR/market_report.py" "$@"
