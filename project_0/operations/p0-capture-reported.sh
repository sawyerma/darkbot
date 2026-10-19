#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
if [ -z "${P0_LAUNCH_ID:-}" ]; then
  exec "$ROOT/ops/darkbotctl" capture start "${P0_MARKET_DURATION_SECS:-until-stopped}"
fi
: "${P0_ENV_FILE:?launch configuration missing}"
: "${P0_REPORT_SOCKET:?reporter socket missing}"
: "${P0_REPORT_GENERATION_FILE:?generation file missing}"
: "${P0_MARKET_DURATION_SECS:?explicit duration or until-stopped required}"
: "${P0_CAPTURE_BIN:?deployed capture missing}"
exec "$P0_CAPTURE_BIN"
