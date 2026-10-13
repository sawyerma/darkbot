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

# 1 wei is the smallest positive native amount. Avoid redundant fresh historical
# launch replays if it is locally executable. Qualify 1/1/1 + 2/10 witnesses
# against ONE post-launch EVM state via existing in-process checkpoint/revert.
if (( RESOLUTION_WEI == 1 )); then
  echo "=== SINGLE-PARENT MINIMUM PATH ==="
  export DARKBOT_G06A_BUY_VALUE_WEI=1
  export DARKBOT_G06A_BUY_VALUES_WEI="1,1,1,2,10"
  if node bsc/gates/gate0/tools/g06a_economics_evm_crosscheck.cjs "$preflight" \
      >"$tmp/one_wei_batch.json" 2>"$tmp/one_wei_batch.err"; then
    python3 - "$tmp/one_wei_batch.json" "$DARKBOT_G06A_GAS_PRICE_WEI" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
gas_price = int(sys.argv[2])
values = [1, 1, 1, 2, 10]
rows = d.get("batch_buy_trials")
if d.get("status") != "EVM_SEQUENCE_PASS" or not isinstance(rows, list) or len(rows) != len(values):
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE invalid single-parent batch")
for i, (v, r) in enumerate(zip(values, rows)):
    if (int(r.get("buy_value_wei", -1)) != v or
        int(r.get("received_raw", 0)) <= 0 or
        r.get("received_raw") != r.get("buyer_balance_raw")):
        raise SystemExit("STOP: QUALIFICATION_INCOMPLETE invalid trial " + str(i))
if any(rows[i] != rows[0] for i in (1, 2)):
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE nondeterministic 1-wei trials")
if int(rows[3]["received_raw"]) < int(rows[0]["received_raw"]) or int(rows[4]["received_raw"]) < int(rows[3]["received_raw"]):
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE nonmonotonic local receipt")
if not d.get("post_launch_checkpoint_revert") or d.get("mainnet_eth_call_launch_success") is not True:
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE missing launch/source binding")
print("SINGLE_PARENT_BLOCK=" + str(d["state_source"]["block_number"]))
print("SINGLE_PARENT_HASH=" + str(d["state_source"]["block_hash"]))
print("ONE_WEI_BUY_REPLAY=3/3_PASS")
print("MONOTONICITY_WITNESSES=2_WEI_AND_10_WEI_PASS_LOCAL")
print("FIRST_LOCALLY_QUALIFIED_POSITIVE_VALUE_WEI=1")
print("RAW_TOKEN_RECEIVED_1_WEI=" + str(rows[0]["received_raw"]))
limit_gas = (6603270 + 3000000) * gas_price
print("LAUNCH_PLUS_BUY_GAS_LIMIT_RESERVE_WEI=" + str(limit_gas))
print("SIMULATED_LAUNCH_PLUS_BUY_GAS_FEE_WEI=" +
      str((int(d["launch_execution_gas"]) + int(rows[0]["execution_gas"])) * gas_price))
print("BUILDER_TIPS_AND_EXTRA_FEES=NOT_QUALIFIED")
print("VERDICT=LOCAL_EVM_ONE_WEI_SINGLE_PARENT_PASS_NOT_MAINNET_AUTHORIZED")
PY
    echo "LIVE_SEND=NONE"
    echo "EVIDENCE_DIR=$tmp"
    echo "NEXT=Qualify exact proposed Mainnet test value/gas/builder expenses in scoped G0.6 DRY."
    exit 0
  fi

  # Only the first BUY itself may reject the minimal amount; infrastructure
  # failure or later inconsistent BUY trials are not economic counterexamples.
  if grep -Eq '^Error: (local BUY simulation reverted:.*trial=0|independent EVM BUY returned zero; trial=0)' \
       "$tmp/one_wei_batch.err"; then
    echo "ONE_WEI=BUY_REJECT_AFTER_LAUNCH; continuing automatic bracketing"
    unset DARKBOT_G06A_BUY_VALUES_WEI
  else
    echo "STOP: QUALIFICATION_INCOMPLETE single-parent replay failed" >&2
    cat "$tmp/one_wei_batch.err" >&2
    exit 4
  fi
fi



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
# One EVM launch and three BUY executions, each reverted to the same post-launch state.
export DARKBOT_G06A_BUY_VALUE_WEI="$hi"
export DARKBOT_G06A_BUY_VALUES_WEI="$hi,$hi,$hi"
node bsc/gates/gate0/tools/g06a_economics_evm_crosscheck.cjs "$preflight" \
  >"$tmp/final_batch.json" 2>"$tmp/final_batch.err" || {
    echo "STOP: QUALIFICATION_INCOMPLETE final in-process BUY batch failed" >&2
    cat "$tmp/final_batch.err" >&2
    exit 4
  }
python3 - "$tmp/final_batch.json" "$hi" <<'PY'
import json, sys
d=json.load(open(sys.argv[1]))
v=str(sys.argv[2])
rows=d.get("batch_buy_trials")
if not isinstance(rows,list) or len(rows)!=3 or d.get("status")!="EVM_SEQUENCE_PASS":
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE missing 3-trial batch")
if any(str(r.get("buy_value_wei"))!=v or int(r.get("received_raw",0))<=0 or r.get("received_raw")!=r.get("buyer_balance_raw") for r in rows):
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE invalid BUY batch")
if any(r!=rows[0] for r in rows[1:]):
    raise SystemExit("STOP: QUALIFICATION_INCOMPLETE nondeterministic BUY batch")
print("FINAL_IN_PROCESS_BUY_TRIALS=3/3_PASS")
print("FINAL_RAW_TOKEN_RECEIPT="+str(rows[0]["received_raw"]))
PY
unset DARKBOT_G06A_BUY_VALUES_WEI

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
