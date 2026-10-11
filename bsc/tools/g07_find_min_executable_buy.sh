#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

source bsc/secrets.env
export BSC_MAINNET_RPC_HTTPS="${BSC_MAINNET_RPC_HTTPS:-https://bsc-dataseed.bnbchain.org}"
export DARKBOT_G06A_GAS_PRICE_WEI="50000000"

grep -Fq 'pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";'   bsc/gates/gate0/src/acceptance_guard.rs || {
    echo "STOP: guard is not UNARMED" >&2
    exit 1
  }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
preflight="$tmp/preflight.json"

node bsc/gates/gate0/tools/g06a_preflight_readonly.cjs "$preflight" >/dev/null

values=(
  1000000000000
  2000000000000
  5000000000000
  10000000000000
  20000000000000
  50000000000000
  100000000000000
  200000000000000
  500000000000000
  1000000000000000
)

echo "MIN_EXECUTABLE_BUY_PROBE=READ_ONLY"
echo "BUY_VALUE_WEI | RESULT"

for v in "${values[@]}"; do
  export DARKBOT_G06A_BUY_VALUE_WEI="$v"
  if node bsc/gates/gate0/tools/g06a_economics_evm_crosscheck.cjs "$preflight"       >"$tmp/$v.json" 2>"$tmp/$v.err"; then
    echo "$v | PASS"
    echo "FIRST_EXECUTABLE_BUY_VALUE_WEI=$v"
    echo "LIVE_SEND=NONE"
    exit 0
  else
    echo "$v | FAIL"
  fi
done

echo "STOP: no PASS in bounded range" >&2
exit 2
