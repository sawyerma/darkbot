#!/usr/bin/env bash
# Project_0 preflight entry point (structure iteration 25): read-only FE preflight and provider/config inspection.
# No signer, submission or value transfer. The former fe_readonly_preflight.sh forwards here unchanged.
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
exec env PYTHONPATH="$DIR/..${PYTHONPATH:+:$PYTHONPATH}" python3 -m operations.qualification.fe_readonly_preflight "$@"
