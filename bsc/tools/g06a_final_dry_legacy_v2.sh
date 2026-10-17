#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

SECRETS="$ROOT/bsc/secrets.env"
CONTRACT="$ROOT/bsc/config/g01_mainnet_acceptance_proposal_v2.json"
BUILDER_POLICY="$ROOT/bsc/config/g06a_builder_floor_addendum_2026-10-11.json"
ENGINE="$ROOT/bsc/engine"

die() {
  printf 'STOP: %s\n' "$*" >&2
  exit 1
}

# Build in the single Cargo workspace (its rust-toolchain.toml applies there),
# then run the binary from the repository root as `cargo run` did before: the
# binaries resolve repository-relative contract paths.
engine_run() {
  local bin="$1"; shift
  (cd "$ENGINE" && cargo build --quiet --locked -p darkbot-bsc-qualification --bin "$bin") || return 1
  "$ENGINE/target/debug/$bin" "$@"
}

json_get() {
  python3 - "$1" "$2" <<'PY'
import json, sys
p, key = sys.argv[1], sys.argv[2]
v = json.load(open(p))
for part in key.split("."):
    v = v[part]
if isinstance(v, bool):
    print("true" if v else "false")
elif v is None:
    print("")
else:
    print(v)
PY
}

[[ -f "$SECRETS" ]] || die "missing $SECRETS"
git check-ignore -q bsc/secrets.env || die "bsc/secrets.env is not ignored by Git"
grep -Fq 'pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";' \
  bsc/engine/crates/execution/src/acceptance_guard.rs \
  || die "Mainnet live guard is not UNARMED"

python3 - "$CONTRACT" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
a = v["authorization"]
if a.get("approved") is not False or a.get("execution_ready") is not False:
    raise SystemExit("STOP: acceptance contract is not in DRY/unapproved state")
print("LIVE_GUARD=UNARMED")
print("ACCEPTANCE=PROPOSED_NOT_AUTHORIZED")
PY

set -a
# shellcheck disable=SC1090
source "$SECRETS"
set +a

: "${BSC_LAUNCHER_PRIVATE_KEY:?BSC_LAUNCHER_PRIVATE_KEY missing in bsc/secrets.env}"
: "${BSC_BUYER_PRIVATE_KEY:?BSC_BUYER_PRIVATE_KEY missing in bsc/secrets.env}"

export BSC_MAINNET_RPC_HTTPS="${BSC_MAINNET_RPC_HTTPS:-https://bsc-dataseed.bnbchain.org}"
export BSC_LAUNCHER_ADDRESS="${BSC_LAUNCHER_ADDRESS:-0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd}"
export BSC_BUYER_ADDRESS="${BSC_BUYER_ADDRESS:-0x9c8a0Db41914beEE52Dd49790c4F087E252743DE}"

export DARKBOT_G06A_LAUNCHER_ADDRESS="$BSC_LAUNCHER_ADDRESS"
export DARKBOT_G06A_BUYER_ADDRESS="$BSC_BUYER_ADDRESS"
export DARKBOT_G06A_LAUNCHER_PRIVATE_KEY="$BSC_LAUNCHER_PRIVATE_KEY"
export DARKBOT_G06A_BUYER_PRIVATE_KEY="$BSC_BUYER_PRIVATE_KEY"

export DARKBOT_G06A_TAX_V3_IMPL="0x024f18294970b5c76c0691b87f138a0317156422"
export DARKBOT_G06A_GAS_PRICE_WEI="50000000"
export DARKBOT_G06A_LAUNCH_GAS_LIMIT="6603270"
export DARKBOT_G06A_BUY_GAS_LIMIT="3000000"
export DARKBOT_G06A_LAUNCH_VALUE_WEI="0"
export DARKBOT_G06A_BUY_VALUE_WEI="1000000000000"
export DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI="50000000"
# 2026-10-11 final one-attempt minimal-spend candidate: 0.05 gwei + 0.000001 BNB BUY.

RUN_ID="g06a_final_dry_$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$ROOT/bsc/data/g06a_final_dry/$RUN_ID"
mkdir -p "$OUT_DIR"

PREFLIGHT="$OUT_DIR/preflight.json"
QUOTE="$OUT_DIR/native_quote.json"
ECON="$OUT_DIR/economics_crosscheck.json"
LOCAL_SIM="$OUT_DIR/local_simulation.json"
DRY="$OUT_DIR/dry_result.json"

printf '\n=== 1/5 READ-ONLY MAINNET PREFLIGHT ===\n'
node bsc/tools/g06a_preflight_readonly.cjs "$PREFLIGHT" > "$OUT_DIR/preflight_console.json"

[[ "$(json_get "$PREFLIGHT" wallet_qualification.addresses_bound)" == "true" ]] || die "wallet addresses are not bound"
[[ "$(json_get "$PREFLIGHT" wallet_qualification.distinct)" == "true" ]] || die "Launcher and Buyer are not distinct"
if [[ "$(json_get "$PREFLIGHT" wallet_qualification.funding_pass)" == "true" ]]; then
  printf 'LIVE_FUNDING=PASS\n'
else
  printf 'LIVE_FUNDING=PENDING  DRY_CONTINUES=true\n'
fi

export DARKBOT_G06A_LAUNCHER_NONCE="$(json_get "$PREFLIGHT" launcher_pending_nonce)"
export DARKBOT_G06A_BUYER_NONCE="$(json_get "$PREFLIGHT" buyer_pending_nonce)"
export DARKBOT_G06A_SEND_HEAD="$(json_get "$PREFLIGHT" latest_block_number)"
export DARKBOT_G06A_MAX_BLOCK="$((DARKBOT_G06A_SEND_HEAD + 2))"
printf 'PREFLIGHT=PASS  head=%s  launcher_nonce=%s  buyer_nonce=%s\n' "$DARKBOT_G06A_SEND_HEAD" "$DARKBOT_G06A_LAUNCHER_NONCE" "$DARKBOT_G06A_BUYER_NONCE"

printf '\n=== 2/5 LOCAL CURVE-20 QUOTE ===\n'
engine_run g06a_native_bnb_quote "$PREFLIGHT" > "$QUOTE"
export DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT="$(json_get "$QUOTE" expected_output)"
[[ -n "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT" ]] || die "native quote did not produce expected_output"
printf 'QUOTE=PASS  expected_output=%s\n' "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT"

printf '\n=== 3/5 INDEPENDENT READ-ONLY EVM CROSS-CHECK ===\n'
node bsc/tools/g06a_economics_evm_crosscheck.cjs "$PREFLIGHT" > "$ECON"
[[ "$(json_get "$ECON" status)" == "EVM_SEQUENCE_PASS" ]] || die "independent EVM cross-check did not pass"
EVM_OUTPUT="$(json_get "$ECON" simulated_received_amount)"
[[ "$EVM_OUTPUT" == "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT" ]] || die "Curve-20 quote and independent EVM output differ"
export DARKBOT_G06A_SALT="$(json_get "$ECON" salt)"
[[ -n "$DARKBOT_G06A_SALT" ]] || die "cross-check did not produce a salt"
printf 'EVM_CROSSCHECK=PASS  exact_output_match=true\n'

[[ -f "$BUILDER_POLICY" ]] || die "missing current builder policy evidence"
export DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256="$(shasum -a 256 "$BUILDER_POLICY" | awk '{print $1}')"

printf '\n=== 4/5 EXACT WALLET-BOUND LOCAL SIMULATION ===\n'
node bsc/tools/g06a_local_simulate.cjs "$LOCAL_SIM"
python3 - "$LOCAL_SIM" "$BSC_LAUNCHER_ADDRESS" "$BSC_BUYER_ADDRESS" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
expected_launcher = sys.argv[2].lower()
expected_buyer = sys.argv[3].lower()
if v["launcher"].lower() != expected_launcher:
    raise SystemExit("STOP: Launcher private key does not match frozen Launcher address")
if v["buyer"].lower() != expected_buyer:
    raise SystemExit("STOP: Buyer private key does not match frozen Buyer address")
if str(v["output_difference"]) != "0":
    raise SystemExit("STOP: exact wallet-bound local simulation differs from local quote")
if not v["launch_success"] or not v["buy_success"]:
    raise SystemExit("STOP: exact local launch/BUY sequence failed")
print("WALLET_BINDING=PASS")
print("EXACT_LOCAL_SIMULATION=PASS")
PY

printf '\n=== 5/5 DRY1-DRY4 -- ZERO LIVE EGRESS ===\n'
engine_run g06a_self_launch_bundle --verify-evidence "$LOCAL_SIM" | tee "$DRY"
python3 - "$DRY" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
d = v["dry_pass"]
for key in ("DRY_1_bytes_and_signers","DRY_2_independent_local_simulation","DRY_3_gas_floor_and_budget","DRY_4_expiry_nonces_zero_egress"):
    print(f"{key}={'PASS' if d.get(key) else 'FAIL'}")
if not d.get("all"):
    raise SystemExit("STOP: final G0.6A DRY qualification failed")
if v.get("network_write_capability") != "NONE_IN_THIS_BINARY":
    raise SystemExit("STOP: DRY binary unexpectedly reports network write capability")
print("FINAL_DRY=PASS")
print("LIVE_SUBMISSION=NOT_AUTHORIZED")
PY

printf '\nEVIDENCE_DIR=%s\n' "$OUT_DIR"
printf 'NEXT=Review/freeze DRY evidence; do not run G0.7 live without explicit authorization.\n'
