#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_DIR/.." && pwd)"

# [SEC-02] Local secrets are loaded from project_0/.env when present.
# The file is git-ignored and must be owner-only. Callers may still export values
# explicitly; exported values remain authoritative if no local file exists.
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

# Public signer identity is versioned; secret key material is not.
# shellcheck source=/dev/null
source "$PROJECT_DIR/config/wallet.public.env"

# Project_0 first Mainnet Shadow qualification.
# Secrets/endpoints stay local; this script never writes or prints them.

need() {
  local k="$1"
  if [[ -z "${!k:-}" ]]; then
    echo "missing required env: $k" >&2
    exit 2
  fi
}

if [[ -z "${P0_RPC_URL:-}" || -z "${P0_WS_URL:-}" ]]; then
  if [[ -z "${HELIUS_API_KEY:-}" ]]; then
    echo "missing endpoints: set P0_RPC_URL+P0_WS_URL or HELIUS_API_KEY" >&2
    exit 2
  fi
  export P0_RPC_URL="${P0_RPC_URL:-https://mainnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}}"
  export P0_WS_URL="${P0_WS_URL:-wss://mainnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}}"
fi

export P0_GROUP_ID="${P0_GROUP_ID:-4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8}"

need P0_KEYPAIR_PATH
need P0_MAX_SLIPPAGE_BPS
need P0_EXPECTED_PUBKEY

# [SEC-02] Fail closed on wrong or weakly protected signer.
KEYPAIR_REAL="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$P0_KEYPAIR_PATH")"
case "$KEYPAIR_REAL" in
  "$REPO_ROOT"/*)
    echo "refusing Project_0 signer stored inside repository" >&2
    exit 2
    ;;
esac
if [[ ! -f "$KEYPAIR_REAL" ]]; then
  echo "Project_0 keypair file not found" >&2
  exit 2
fi
KEYPAIR_MODE="$(stat -f '%Lp' "$KEYPAIR_REAL" 2>/dev/null || stat -c '%a' "$KEYPAIR_REAL" 2>/dev/null || true)"
if [[ "$KEYPAIR_MODE" != "600" ]]; then
  echo "insecure Project_0 keypair permissions: mode=$KEYPAIR_MODE (require 600)" >&2
  exit 2
fi
# Verify a standard Solana 64-byte JSON keypair without depending on a globally
# installed Solana CLI. The public key is the final 32 bytes of the keypair file.
ACTUAL_PUBKEY="$(
  python3 - "$KEYPAIR_REAL" <<'PY'
import json
import sys

alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
path = sys.argv[1]

try:
    with open(path, "r", encoding="utf-8") as fh:
        raw = json.load(fh)
except Exception as exc:
    print(f"cannot read Project_0 keypair: {exc}", file=sys.stderr)
    raise SystemExit(2)

if (
    not isinstance(raw, list)
    or len(raw) != 64
    or any(not isinstance(v, int) or isinstance(v, bool) or v < 0 or v > 255 for v in raw)
):
    print("invalid Project_0 Solana keypair JSON: expected exactly 64 byte values", file=sys.stderr)
    raise SystemExit(2)

pub = bytes(raw[32:64])
n = int.from_bytes(pub, "big")
encoded = ""
while n:
    n, rem = divmod(n, 58)
    encoded = alphabet[rem] + encoded

leading_zeroes = 0
for b in pub:
    if b != 0:
        break
    leading_zeroes += 1

print("1" * leading_zeroes + encoded)
PY
)" || {
  echo "failed to derive Project_0 signer identity" >&2
  exit 2
}
if [[ "$ACTUAL_PUBKEY" != "$P0_EXPECTED_PUBKEY" ]]; then
  echo "Project_0 signer identity mismatch: derived=$ACTUAL_PUBKEY expected=$P0_EXPECTED_PUBKEY" >&2
  exit 2
fi

export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
case "${P0_LIVE_ENABLED:-false}:${P0_SUBMISSION_ENABLED:-false}" in
  false:false|0:0|false:0|0:false) ;;
  *) echo "econ4h requires LIVE and submission disabled" >&2; exit 2 ;;
esac
export P0_MODE=econ4h
export P0_LIVE_ENABLED=false
export P0_SUBMISSION_ENABLED=false
export P0_ECON_DURATION_SECS="${P0_ECON_DURATION_SECS:-14400}"
export P0_ECON_SNAPSHOT_SECS="${P0_ECON_SNAPSHOT_SECS:-3600}"
export P0_ECON_RUN_ID="${P0_ECON_RUN_ID:-econ4h_$(date -u +%Y%m%dT%H%M%SZ)}"
export P0_ECON_OUT_DIR="${P0_ECON_OUT_DIR:-$PROJECT_DIR/data/econ4h}"
export P0_MARKET_SOURCE="${P0_MARKET_SOURCE:-logs}"
RUN_DIR="$P0_ECON_OUT_DIR/$P0_ECON_RUN_ID"
if [[ -e "$RUN_DIR" ]]; then echo "econ run directory already exists" >&2; exit 2; fi
mkdir -p "$RUN_DIR"
export P0_RECORD_PATH="$RUN_DIR/opportunities.jsonl"
export P0_SUMMARY_PATH="$RUN_DIR/daily_summary.jsonl"
export P0_MT0_PATH="$RUN_DIR/mt0.jsonl"
export P0_MIN_PROFIT="${P0_MIN_PROFIT:-0}"
export P0_ZERO_PREFUND_QUALIFIED=false
export P0_ZERO_PREFUND_SHADOW_PROBE=true
export P0_RECORD_PATH="${P0_RECORD_PATH:-project_0/data/shadow/opportunities.jsonl}"
export P0_SUMMARY_PATH="${P0_SUMMARY_PATH:-project_0/data/shadow/daily_summary.jsonl}"
export P0_JUPITER_MAX_ACCOUNTS="${P0_JUPITER_MAX_ACCOUNTS:-64}"
export P0_INGRESS_BUFFER="${P0_INGRESS_BUFFER:-65536}"
export P0_MAX_CONCURRENT="${P0_MAX_CONCURRENT:-1}"
export P0_MAX_OUTSTANDING="${P0_MAX_OUTSTANDING:-1}"
export P0_MAX_AGE_NS="${P0_MAX_AGE_NS:-2000000000}"
export P0_DEADLINE_NS="${P0_DEADLINE_NS:-2000000000}"

mkdir -p "$(dirname "$P0_RECORD_PATH")" "$(dirname "$P0_SUMMARY_PATH")"

cd "$REPO_ROOT"
echo "C0-E1: build, then $P0_ECON_DURATION_SECS seconds; four automatic snapshots; submission disabled"
cargo build --release --locked --manifest-path "$PROJECT_DIR/native/Cargo.toml" \
  --features "crate_health txv1" --bin p0-longtail > "$RUN_DIR/build.log" 2>&1 || {
  echo "Build failed: $RUN_DIR/build.log" >&2; exit 2;
}
BIN="${CARGO_TARGET_DIR:-$PROJECT_DIR/native/target}/release/p0-longtail"
status=0
"$BIN" > "$RUN_DIR/console_summary.txt" 2> "$RUN_DIR/runtime.log" || status=$?
cat "$RUN_DIR/console_summary.txt"
echo "Reports and detailed logs: $RUN_DIR"
exit "$status"
