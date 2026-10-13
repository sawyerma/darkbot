#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${P0_ENV_FILE:-$DIR/../.env}"
if [[ -f "$ENV_FILE" ]]; then
  MODE="$(stat -f '%Lp' "$ENV_FILE" 2>/dev/null || stat -c '%a' "$ENV_FILE")"
  [[ "$MODE" == 600 ]] || { echo 'env permissions must be 600' >&2; exit 2; }
  set -a
  source "$ENV_FILE"
  set +a
fi
exec python3 "$DIR/fe_readonly_preflight.py" "$@"
