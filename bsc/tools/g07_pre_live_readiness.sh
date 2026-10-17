#!/usr/bin/env bash
set -euo pipefail
umask 077
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"
exec cargo run --locked --offline --manifest-path bsc/engine/Cargo.toml \
  -p darkbot-bsc-execution --features g07-mainnet --bin g07_pre_live_readiness -- "$@"
