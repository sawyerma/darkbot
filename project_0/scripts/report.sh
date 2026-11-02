#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# R-PY (Nutzerentscheidung 01.11.2026): the pinned historical readers run offline only — no
# credential and no endpoint reaches them (none from a .env, none inherited), so no reader can
# make an RPC call; online reads are the native tools' (p0ctl verify, the provider registry).
for v in ${!HELIUS_@}; do unset "$v"; done
unset P0_RPC_URL P0_AUDIT_RPC_URL P0_OPERATIONAL_CONFIG_FILE P0_PROFILE
# JFI-01/03: the pinned readers serve historical runs only; run_store.py refuses a run with
# native records (read natively: p0ctl report/verify/backtest). No JSON views are exported.
# --run ID|PATH is located across all run roots and its layout checked before the pinned readers run.
exec python3 "$DIR/../operations/capture/run_store.py" exec "$DIR/../operations/reporting/market_report.py" "$@"
