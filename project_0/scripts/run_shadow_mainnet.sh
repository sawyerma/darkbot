#!/usr/bin/env bash
set -euo pipefail

# Project_0 first Mainnet Shadow qualification.
# Secrets/endpoints stay in the caller environment; this script never writes them.

need() {
  local k="$1"
  if [[ -z "${!k:-}" ]]; then
    echo "missing required env: $k" >&2
    exit 2
  fi
}

need P0_WS_URL
need P0_RPC_URL
need P0_GROUP_ID
need P0_KEYPAIR_PATH
need P0_MAX_SLIPPAGE_BPS

export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
export P0_MODE=shadow
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

echo "Project_0 Mainnet Shadow"
echo "  program=$P0_PROGRAM_ID"
echo "  group=$P0_GROUP_ID"
echo "  mode=$P0_MODE"
echo "  zero_prefund_qualified=$P0_ZERO_PREFUND_QUALIFIED"
echo "  zero_prefund_shadow_probe=$P0_ZERO_PREFUND_SHADOW_PROBE"
echo "  record=$P0_RECORD_PATH"
echo "  summary=$P0_SUMMARY_PATH"

exec cargo run --release \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1" \
  --bin p0-longtail
