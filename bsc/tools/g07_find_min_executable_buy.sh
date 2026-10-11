#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

cd "$(git rev-parse --show-toplevel)"

source bsc/secrets.env
export BSC_MAINNET_RPC_HTTPS="${BSC_MAINNET_RPC_HTTPS:-https://bsc-dataseed.bnbchain.org}"
export DARKBOT_G06A_GAS_PRICE_WEI="${DARKBOT_G06A_GAS_PRICE_WEI:-50000000}"

grep -Fq 'pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";' \
  bsc/gates/gate0/src/acceptance_guard.rs || {
    echo "STOP: guard is not UNARMED" >&2
    exit 1
  }

# Automatic bounded search. No hand-maintained value list.
MAX_BUY_VALUE_WEI="${DARKBOT_G07_SEARCH_MAX_BUY_VALUE_WEI:-1000000000000000}"
RESOLUTION_WEI="${DARKBOT_G07_SEARCH_RESOLUTION_WEI:-1000000}"
PROBE_RUNS="${DARKBOT_G07_SEARCH_PROBE_RUNS:-3}"
REQUIRED_PASSES="${DARKBOT_G07_SEARCH_REQUIRED_PASSES:-2}"

[[ "$MAX_BUY_VALUE_WEI" =~ ^[0-9]+$ ]] || { echo "bad MAX_BUY_VALUE_WEI" >&2; exit 2; }
[[ "$RESOLUTION_WEI" =~ ^[1-9][0-9]*$ ]] || { echo "bad RESOLUTION_WEI" >&2; exit 2; }
[[ "$PROBE_RUNS" =~ ^[1-9][0-9]*$ ]] || { echo "bad PROBE_RUNS" >&2; exit 2; }
[[ "$REQUIRED_PASSES" =~ ^[1-9][0-9]*$ ]] || { echo "bad REQUIRED_PASSES" >&2; exit 2; }
(( REQUIRED_PASSES <= PROBE_RUNS )) || { echo "REQUIRED_PASSES > PROBE_RUNS" >&2; exit 2; }

tmp="$(mktemp -d -t darkbot-g07-buy-search.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT
preflight="$tmp/preflight.json"

node bsc/gates/gate0/tools/g06a_preflight_readonly.cjs "$preflight" >/dev/null

probe() {
  local v="$1"
  local passes=0
  local run

  export DARKBOT_G06A_BUY_VALUE_WEI="$v"

  for ((run=1; run<=PROBE_RUNS; run++)); do
    if node bsc/gates/gate0/tools/g06a_economics_evm_crosscheck.cjs "$preflight" \
        >"$tmp/${v}_${run}.json" 2>"$tmp/${v}_${run}.err"; then
      passes=$((passes + 1))
    fi

    # Decide early once either outcome can no longer change.
    if (( passes >= REQUIRED_PASSES )); then
      echo "  probe $v wei: PASS ($passes/$run)"
      return 0
    fi
    if (( passes + (PROBE_RUNS - run) < REQUIRED_PASSES )); then
      echo "  probe $v wei: FAIL ($passes/$run)"
      return 1
    fi
  done

  echo "  probe $v wei: FAIL ($passes/$PROBE_RUNS)"
  return 1
}

echo "MIN_EXECUTABLE_BUY_SEARCH=READ_ONLY"
echo "GAS_PRICE_WEI=$DARKBOT_G06A_GAS_PRICE_WEI"
echo "MAX_BUY_VALUE_WEI=$MAX_BUY_VALUE_WEI"
echo "SEARCH_RESOLUTION_WEI=$RESOLUTION_WEI"
echo "PROBE_POLICY=$REQUIRED_PASSES/$PROBE_RUNS passes required"
echo
echo "=== EXPONENTIAL BRACKETING ==="

lo=0
hi="$RESOLUTION_WEI"

while true; do
  if (( hi > MAX_BUY_VALUE_WEI )); then
    hi="$MAX_BUY_VALUE_WEI"
  fi

  if probe "$hi"; then
    break
  fi

  lo="$hi"
  if (( hi == MAX_BUY_VALUE_WEI )); then
    echo "STOP: no executable BUY found up to MAX_BUY_VALUE_WEI=$MAX_BUY_VALUE_WEI" >&2
    exit 3
  fi

  next=$((hi * 2))
  if (( next <= hi || next > MAX_BUY_VALUE_WEI )); then
    next="$MAX_BUY_VALUE_WEI"
  fi
  hi="$next"
done

echo
echo "BRACKET_FAIL_WEI=$lo"
echo "BRACKET_PASS_WEI=$hi"
echo
echo "=== BINARY REFINEMENT ==="

while (( hi - lo > RESOLUTION_WEI )); do
  mid=$(((lo + hi) / 2))
  # Quantize to the configured resolution and keep strict progress.
  mid=$(( (mid / RESOLUTION_WEI) * RESOLUTION_WEI ))
  if (( mid <= lo )); then
    mid=$((lo + RESOLUTION_WEI))
  fi
  if (( mid >= hi )); then
    break
  fi

  if probe "$mid"; then
    hi="$mid"
  else
    lo="$mid"
  fi
done

echo
echo "=== FINAL STABILITY CHECK ==="
# Final candidate must pass all final verification runs, not just majority.
export DARKBOT_G06A_BUY_VALUE_WEI="$hi"
for run in 1 2 3; do
  node bsc/gates/gate0/tools/g06a_economics_evm_crosscheck.cjs "$preflight" \
    >"$tmp/final_${run}.json" 2>"$tmp/final_${run}.err" || {
      echo "STOP: final candidate failed stability run $run" >&2
      cat "$tmp/final_${run}.err" >&2
      exit 4
    }
  echo "  final $hi wei: PASS ($run/3)"
done

echo
echo "LAST_CONFIRMED_FAIL_WEI=$lo"
echo "FIRST_STABLE_PASS_WEI=$hi"
echo "SEARCH_RESOLUTION_WEI=$RESOLUTION_WEI"
python3 - "$lo" "$hi" <<'PY'
from decimal import Decimal
import sys
lo, hi = map(Decimal, sys.argv[1:3])
scale = Decimal(10) ** 18
print("LAST_CONFIRMED_FAIL_BNB=" + str(lo / scale))
print("FIRST_STABLE_PASS_BNB=" + str(hi / scale))
PY
echo "LIVE_SEND=NONE"
echo "NEXT=Freeze FIRST_STABLE_PASS_WEI and run one full G0.6 final DRY before funding."
