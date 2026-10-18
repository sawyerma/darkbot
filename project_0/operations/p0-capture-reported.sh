#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
test -f project_0/.env || { echo "Missing project_0/.env"; exit 2; }
mkdir -p /tmp/darkbot "$HOME/.darkbot"
chmod 700 /tmp/darkbot "$HOME/.darkbot"
export P0_ENV_FILE="project_0/config/market.public.env:project_0/.env"
export P0_REPORT_SOCKET="${P0_REPORT_SOCKET:-/tmp/darkbot/project0.sock}"
export P0_REPORT_GENERATION_FILE="${P0_REPORT_GENERATION_FILE:-$HOME/.darkbot/p0.generation}"
export P0_MARKET_DURATION_SECS="${P0_MARKET_DURATION_SECS:-300}"
exec project_0/native/target/release/p0-market-capture-reported
