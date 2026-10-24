#!/usr/bin/env bash
# Read-only historical verification. No signer, build, wallet or trading process.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ " ${*} " != *" --help "* && " ${*} " != *" --self-test "* && " ${*} " != *" --offline "* ]]; then
  ENV_FILE="${P0_ENV_FILE:-$DIR/../.env}"
  if [[ -f "$ENV_FILE" ]]; then
    MODE="$(stat -c '%a' "$ENV_FILE" 2>/dev/null || stat -f '%Lp' "$ENV_FILE")"
    [[ "$MODE" == 600 ]] || { echo 'env permissions must be 600' >&2; exit 2; }
    set -a
    source "$ENV_FILE"
    set +a
  fi
fi
if [[ " ${*} " == *" --self-test "* ]]; then
  exec python3 "$DIR/../operations/verification/audit_market_quick.py" "$@"
fi
# JFI-01/03: the pinned readers serve historical runs only; run_store.py refuses a run with
# native records (read natively: p0ctl report/verify/backtest). No JSON views are exported.
# --run ID|PATH is located across all run roots and its layout checked before the pinned readers run.
exec python3 "$DIR/../operations/capture/run_store.py" exec "$DIR/../operations/verification/measurement_verify.py" "$@"
