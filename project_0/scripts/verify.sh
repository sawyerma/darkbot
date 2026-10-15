#!/usr/bin/env bash
# Read-only historical verification. No signer, build, wallet or trading process.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ " ${*} " != *" --help "* && " ${*} " != *" --self-test "* && " ${*} " != *" --offline "* ]]; then
  ENV_FILE="${P0_ENV_FILE:-$DIR/../.env}"
  if [[ -f "$ENV_FILE" ]]; then
    MODE="$(stat -f '%Lp' "$ENV_FILE" 2>/dev/null || stat -c '%a' "$ENV_FILE")"
    [[ "$MODE" == 600 ]] || { echo 'env permissions must be 600' >&2; exit 2; }
    set -a
    source "$ENV_FILE"
    set +a
  fi
fi
if [[ " ${*} " == *" --self-test "* ]]; then
  exec python3 "$DIR/audit_market_quick.py" "$@"
fi
# --run ID|PATH is located across all run roots and checked for layout V1 before the pinned readers run.
exec python3 "$DIR/run_store.py" exec "$DIR/measurement_verify.py" "$@"
