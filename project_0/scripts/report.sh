#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# JFI-01/03: the pinned readers serve historical runs only; run_store.py refuses a run with
# native records (read natively: p0ctl report/verify/backtest). No JSON views are exported.
# --run ID|PATH is located across all run roots and its layout checked before the pinned readers run.
exec python3 "$DIR/../operations/capture/run_store.py" exec "$DIR/../operations/reporting/market_report.py" "$@"
