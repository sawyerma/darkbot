#!/usr/bin/env bash
# [C0-MO] Project 0 — Unfiltered Live Market Observer (read-only).
# Starts the existing p0-longtail binary with P0_MODE=market. No keypair, no signer,
# no simulation, no submission. All business logic lives in Rust; this script only
# prepares environment and output location.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_DIR/.." && pwd)"

# [SEC-02] Local secrets are loaded from project_0/.env when present (owner-only).
ENV_FILE="${P0_ENV_FILE:-$PROJECT_DIR/.env}"
if [[ -f "$ENV_FILE" ]]; then
  MODE="$(stat -f '%Lp' "$ENV_FILE" 2>/dev/null || stat -c '%a' "$ENV_FILE" 2>/dev/null || true)"
  if [[ "$MODE" != "600" ]]; then
    echo "insecure Project_0 env permissions: $ENV_FILE mode=$MODE (require 600)" >&2
    exit 2
  fi
  set -a
  # shellcheck source=/dev/null
  source "$ENV_FILE"
  set +a
fi

# transactions = Helius transactionSubscribe (Developer+ plan), needs only the WebSocket.
# logs         = standard logsSubscribe (all plans), additionally needs RPC for post-capture enrichment.
export P0_MARKET_SOURCE="${P0_MARKET_SOURCE:-transactions}"
case "$P0_MARKET_SOURCE" in
  transactions|logs) ;;
  *) echo "invalid P0_MARKET_SOURCE=$P0_MARKET_SOURCE (transactions|logs)" >&2; exit 2 ;;
esac

# Endpoint selection lives in Rust (MarketConfig): an explicit P0_WS_URL / P0_RPC_URL always wins;
# otherwise HELIUS_API_KEY derives them (transactions -> Helius Enhanced WSS, logs -> standard WSS + RPC).
#   Helius Free:                         P0_MARKET_SOURCE=logs
#   Helius Developer+ (transactionSubscribe): P0_MARKET_SOURCE=transactions (preferred)
# Provider support for transactionSubscribe is never assumed; the smoke run must show the ACK.
if [[ -z "${P0_WS_URL:-}" && -z "${HELIUS_API_KEY:-}" ]]; then
  echo "missing endpoint: set P0_WS_URL or HELIUS_API_KEY" >&2
  exit 2
fi
if [[ "$P0_MARKET_SOURCE" == "logs" && -z "${P0_RPC_URL:-}" && -z "${HELIUS_API_KEY:-}" ]]; then
  echo "note: no P0_RPC_URL/HELIUS_API_KEY - liquidation enrichment unavailable (capture, summary and report still run)" >&2
fi

# Market mode never needs a signer: make sure none is passed through by accident.
unset P0_KEYPAIR_PATH P0_EXPECTED_PUBKEY

export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
export P0_MODE=market
export P0_MARKET_DURATION_SECS="${P0_MARKET_DURATION_SECS:-14400}"
export P0_MARKET_COMMITMENT="${P0_MARKET_COMMITMENT:-finalized}"
export P0_MARKET_RUN_ID="${P0_MARKET_RUN_ID:-market_$(date -u +%Y%m%dT%H%M%SZ)}"
# Run storage contract: data/market by default, data/runs with P0_RUN_STORE=runs.
P0_MARKET_OUT_DIR="$(python3 "$SCRIPT_DIR/run_store.py" root market "${P0_MARKET_OUT_DIR:-}")" || exit 2
export P0_MARKET_OUT_DIR
export P0_INGRESS_BUFFER="${P0_INGRESS_BUFFER:-65536}"

python3 "$SCRIPT_DIR/run_store.py" allocate "$P0_MARKET_OUT_DIR" "$P0_MARKET_RUN_ID" >/dev/null || exit 2

echo "Project_0 Unfiltered Live Market Observer"
echo "  program=$P0_PROGRAM_ID"
echo "  source=$P0_MARKET_SOURCE"
echo "  commitment=$P0_MARKET_COMMITMENT"
echo "  duration_seconds=$P0_MARKET_DURATION_SECS"
echo "  endpoints: see Rust banner (hosts redacted)"
echo "  output=$P0_MARKET_OUT_DIR/$P0_MARKET_RUN_ID"

cd "$REPO_ROOT"
exec cargo run --release \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1" \
  --bin p0-longtail
