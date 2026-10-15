#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

SECRETS="$ROOT/bsc/secrets.env"
NATIVE="$ROOT/bsc/native"

die(){ printf 'STOP: %s\n' "$*" >&2; exit 1; }

# Build in the single Cargo workspace (its rust-toolchain.toml applies there),
# then run the binary from the repository root as `cargo run` did before: the
# binaries resolve repository-relative contract paths.
native_run() {
  local bin="$1"; shift
  (cd "$NATIVE" && cargo build --quiet --locked -p darkbot-qualify --bin "$bin") || return 1
  "$NATIVE/target/debug/$bin" "$@"
}

[[ -f "$SECRETS" ]] || die "missing bsc/secrets.env"
git check-ignore -q bsc/secrets.env || die "bsc/secrets.env is not ignored"

# This probe must remain non-live.
grep -Fq 'pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";'   bsc/native/crates/runtime/src/acceptance_guard.rs || die "live guard is armed"

set -a
# shellcheck disable=SC1090
source "$SECRETS"
set +a

export BSC_MAINNET_RPC_HTTPS="${BSC_MAINNET_RPC_HTTPS:-https://bsc-dataseed.bnbchain.org}"
export BSC_LAUNCHER_ADDRESS="${BSC_LAUNCHER_ADDRESS:-0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd}"
export BSC_BUYER_ADDRESS="${BSC_BUYER_ADDRESS:-0x9c8a0Db41914beEE52Dd49790c4F087E252743DE}"
export DARKBOT_G06A_LAUNCHER_ADDRESS="$BSC_LAUNCHER_ADDRESS"
export DARKBOT_G06A_BUYER_ADDRESS="$BSC_BUYER_ADDRESS"

TMP="$(mktemp -d -t darkbot-g07-min-spend.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

PREFLIGHT="$TMP/preflight.json"
node bsc/tools/g06a_preflight_readonly.cjs "$PREFLIGHT" >/dev/null

printf 'MIN_SPEND_PROBE=READ_ONLY\n'
printf 'CURRENT_HEAD=%s\n' "$(python3 - "$PREFLIGHT" <<'PY'
import json,sys
print(json.load(open(sys.argv[1]))["latest_block_number"])
PY
)"

# These are candidate BUY values only. No transaction is submitted.
# The quote implementation is pure/local and validates that output/minOutput stay positive.
BUY_VALUES=(
  1000000000000000
  500000000000000
  100000000000000
  50000000000000
  10000000000000
  1000000000000
)

printf '\nBUY_VALUE_WEI | EXPECTED_OUTPUT | MIN_OUTPUT | LOCAL_QUOTE\n'
for v in "${BUY_VALUES[@]}"; do
  export DARKBOT_G06A_BUY_VALUE_WEI="$v"
  q="$TMP/q_$v.json"
  if native_run g06a_native_bnb_quote "$PREFLIGHT" > "$q" 2>"$TMP/q_$v.err"; then
    python3 - "$q" <<'PY'
import json,sys
v=json.load(open(sys.argv[1]))
ok=int(v["expected_output"])>0 and int(v["min_output"])>0
print(f"{v['buy_value_wei']} | {v['expected_output']} | {v['min_output']} | {'PASS' if ok else 'FAIL'}")
PY
  else
    printf '%s | - | - | FAIL\n' "$v"
  fi
done

printf '\nGAS_POLICY_CANDIDATES_WEI\n'
printf '50000000   # 0.05 gwei, current explicit BlockRazor floor evidence\n'
printf '100000000  # 0.10 gwei\n'
printf '200000000  # 0.20 gwei\n'
printf '500000000  # 0.50 gwei\n'
printf '1000000000 # 1.00 gwei, current conservative Stage-1 policy\n'

python3 - "$PREFLIGHT" <<'PY'
import json,sys
v=json.load(open(sys.argv[1]))
print("CURRENT_RPC_GAS_PRICE_WEI="+v["gas_price_wei"])
print("NOTE=Candidate gas price must be >= current network gas price and >= every armed builder's verified floor.")
PY

printf '\nLIVE_SEND=IMPOSSIBLE_FROM_THIS_SCRIPT\n'
printf 'NEXT=Use results only to choose one final candidate; then rerun the full G0.6 DRY exactly once for that candidate before funding.\n'
