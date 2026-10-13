#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

cd "$(git rev-parse --show-toplevel)"

# A pruned RPC cannot serve one frozen historical parent indefinitely.
# Restart the ENTIRE independent search with a new preflight; never mix blocks.
if [[ "${DARKBOT_G07_SEARCH_INNER:-}" != YES ]]; then
  mkdir -p bsc/local_diagnostics
  for restart in 0 1 2; do
    logdir="$(mktemp -d bsc/local_diagnostics/g07-controller.XXXXXX)"
    log="$logdir/controller.log"
    if DARKBOT_G07_SEARCH_INNER=YES bash "$0" >"$log" 2>&1; then
      cat "$log"
      exit 0
    fi
    cat "$log"
    if grep -Eq "missing trie node|state .* unavailable|pruned state" "$log" && (( restart < 2 )); then
      echo "RPC_PRUNED_STATE_RESTART=$((restart + 1))/2; restarting entire search with a fresh block"
      continue
    fi
    if grep -Eq "local launch simulation reverted|EVM_DIVERGENCE_LAUNCH" "$log"; then
      echo "VERDICT=QUALIFICATION_INCOMPLETE reason=EVM_DIVERGENCE_LAUNCH" >&2
    elif grep -Eq "missing trie node|pruned state" "$log"; then
      echo "VERDICT=QUALIFICATION_INCOMPLETE reason=RPC_STATE_UNAVAILABLE" >&2
    fi
    echo "STOP: search failed; controller evidence: $log" >&2
    exit 5
  done
fi
source bsc/secrets.env
export BSC_MAINNET_RPC_HTTPS="${BSC_MAINNET_RPC_HTTPS:-https://bsc-dataseed.bnbchain.org}"
# Current read-only gas suggestion; builder floor from frozen evidence.
DARKBOT_G06A_GAS_PRICE_WEI="$(node -e '(async()=>{const fs=require("fs");const p=JSON.parse(fs.readFileSync("bsc/gates/gate0/evidence/g06a_builder_floor_addendum_2026-10-11.json"));if(!p.blockrazor.verified)throw Error("unverified builder floor");const floor=BigInt(p.blockrazor.explicit_minimum_bundle_gas_price_wei);const r=await fetch(process.env.BSC_MAINNET_RPC_HTTPS,{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify({jsonrpc:"2.0",id:1,method:"eth_gasPrice",params:[]}),signal:AbortSignal.timeout(10000)});if(!r.ok)throw Error("gas RPC HTTP "+r.status);const j=await r.json();if(j.error||!j.result)throw Error("invalid eth_gasPrice");const rpc=BigInt(j.result);const gas=rpc>floor?rpc:floor;if(gas<=0n||gas>1000000000n)throw Error("gas exceeds authorized ceiling");console.error("RPC_GAS_PRICE_WEI="+rpc+" BUILDER_FLOOR_WEI="+floor+" GAS_POLICY=DYNAMIC_CANDIDATE_ONLY");process.stdout.write(String(gas));})().catch(e=>{console.error("STOP gas: "+e.message);process.exitCode=1})' )"
export DARKBOT_G06A_GAS_PRICE_WEI

grep -Fq 'pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";' \
  bsc/gates/gate0/src/acceptance_guard.rs || {
    echo "STOP: guard is not UNARMED" >&2
    exit 1
  }

# Automatic bounded search. No hand-maintained value list.
MAX_BUY_VALUE_WEI="${DARKBOT_G07_SEARCH_MAX_BUY_VALUE_WEI:-1000000000000000}"
RESOLUTION_WEI="${DARKBOT_G07_SEARCH_RESOLUTION_WEI:-1}"
PROBE_RUNS="${DARKBOT_G07_SEARCH_PROBE_RUNS:-3}"
REQUIRED_PASSES="${DARKBOT_G07_SEARCH_REQUIRED_PASSES:-2}"

[[ "$MAX_BUY_VALUE_WEI" =~ ^[0-9]+$ ]] || { echo "bad MAX_BUY_VALUE_WEI" >&2; exit 2; }
[[ "$RESOLUTION_WEI" =~ ^[1-9][0-9]*$ ]] || { echo "bad RESOLUTION_WEI" >&2; exit 2; }
[[ "$PROBE_RUNS" =~ ^[1-9][0-9]*$ ]] || { echo "bad PROBE_RUNS" >&2; exit 2; }
[[ "$REQUIRED_PASSES" =~ ^[1-9][0-9]*$ ]] || { echo "bad REQUIRED_PASSES" >&2; exit 2; }
(( REQUIRED_PASSES <= PROBE_RUNS )) || { echo "REQUIRED_PASSES > PROBE_RUNS" >&2; exit 2; }
(( MAX_BUY_VALUE_WEI >= RESOLUTION_WEI )) || { echo "MAX below resolution" >&2; exit 2; }
(( MAX_BUY_VALUE_WEI <= 1000000000000000 )) || { echo "MAX exceeds frozen buy ceiling" >&2; exit 2; }

mkdir -p bsc/local_diagnostics
tmp="$(mktemp -d bsc/local_diagnostics/g07-search.XXXXXX)"
echo "EVIDENCE_DIR=$tmp"
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
    elif grep -Eq "local BUY simulation reverted:|independent EVM BUY returned zero" "$tmp/${v}_${run}.err"; then
      echo "  BUY_REJECT=$v run=$run"
    else
      echo "STOP: NON_BUY_FAILURE value=$v run=$run" >&2
      cat "$tmp/${v}_${run}.err" >&2
      exit 5
    fi

    # Decide early once either outcome can no longer change.
    if (( passes >= REQUIRED_PASSES )); then
      echo "  probe $v wei: PROVISIONAL_PASS ($passes/$run; final stability pending)"
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
echo "BRACKET_PROVISIONAL_PASS_WEI=$hi"
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

# Monotonicity sanity witnesses for the frozen parent and gas candidate.
# This cannot establish universal monotonicity, but catches immediate counterexamples.
echo
echo "=== MONOTONICITY WITNESSES ==="
for factor in 2 10; do
  witness=$((hi * factor))
  if (( witness <= hi || witness > MAX_BUY_VALUE_WEI )); then
    continue
  fi
  if ! probe "$witness"; then
    echo "STOP: QUALIFICATION_INCOMPLETE nonmonotonic witness=$witness above passing candidate=$hi" >&2
    exit 6
  fi
done

# All final records must refer to the same parent, buyer output and buy value.
python3 - "$tmp" "$hi" <<'PY'
import json, pathlib, sys
d = pathlib.Path(sys.argv[1])
v = str(sys.argv[2])
rows = [json.loads((d / f"final_{i}.json").read_text()) for i in (1, 2, 3)]
keys = ("buy_value_wei", "simulated_received_amount", "simulated_buyer_balance_after", "launch_execution_gas", "buy_execution_gas")
base = rows[0]
if any(str(r.get("buy_value_wei")) != v or r.get("status") != "EVM_SEQUENCE_PASS" for r in rows):
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE invalid final record")
if any(r.get("state_source") != base.get("state_source") or any(r.get(k) != base.get(k) for k in keys) for r in rows[1:]):
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE nondeterministic final records")
print("FINAL_RECORD_CONSISTENCY=PASS")
print("TOKEN_RAW_UNITS=" + str(base["simulated_received_amount"]))
PY

echo
if (( lo == 0 )); then echo "LOWER_BOUND=UNTESTED_BELOW_RESOLUTION"; else echo "LAST_CONFIRMED_BUY_REJECT_WEI=$lo"; fi
echo "FIRST_STABLE_PASS_WEI=$hi"
echo "VERDICT=LOCAL_EVM_MINIMUM_CANDIDATE_STABLE_NOT_MAINNET_AUTHORIZED"
echo "SEARCH_RESOLUTION_WEI=$RESOLUTION_WEI"
python3 - "$lo" "$hi" <<'PY'
from decimal import Decimal
import sys
lo, hi = map(Decimal, sys.argv[1:3])
scale = Decimal(10) ** 18
if lo: print("LAST_CONFIRMED_BUY_REJECT_BNB=" + str(lo / scale))
print("FIRST_STABLE_PASS_BNB=" + str(hi / scale))
PY
echo "LIVE_SEND=NONE"
echo "EVIDENCE_DIR=$tmp"
echo "NEXT=Requalify exact gas and BUY in full G0.6 DRY before any authorization."
