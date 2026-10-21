#!/usr/bin/env bash
set -euo pipefail
umask 077
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"
(cd "$ROOT/bsc/engine" && cargo build --locked --offline \
  -p darkbot-bsc-execution --features g07-mainnet --bin g07_pre_live_readiness)
exec "$ROOT/bsc/engine/target/debug/g07_pre_live_readiness" "$@"
