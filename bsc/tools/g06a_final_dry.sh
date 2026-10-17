#!/usr/bin/env bash
# G0.6a requalification DRY (R0, DRY-0 .. DRY-4) of the frozen G0.7 attempt-1
# candidate, binary records only (DBREC001). No JSON is written or parsed:
# every step writes a record and the checks read typed fields with
# `dbrec_show --get`. DRY-1 signs locally; nothing is sent (read-only RPC
# only, no builder), and the guard must be UNARMED. The previous JSON chain
# is kept unchanged as g06a_final_dry_legacy_v2.sh (historical only).
set -euo pipefail
IFS=$'\n\t'
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

SECRETS="$ROOT/bsc/secrets.env"
ENGINE="$ROOT/bsc/engine"
FROZEN="$ROOT/bsc/native/evidence/g06a/g07_binary_import_20261016"
FROZEN_DRY_HEAD=127865760

die() {
  printf 'STOP: %s\n' "$*" >&2
  exit 1
}

build() {
  (cd "$ENGINE" && cargo build --quiet --locked "$@") || die "build failed: $*"
}

get() {
  "$ENGINE/target/debug/dbrec_show" --get "$1" "$2"
}

lower() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

[[ -f "$SECRETS" ]] || die "missing $SECRETS"
git check-ignore -q bsc/secrets.env || die "bsc/secrets.env is not ignored by Git"
grep -Fq 'pub const FROZEN_APPROVED_V4_SHA256: &str = "UNARMED";' \
  bsc/engine/crates/execution/src/acceptance_guard.rs \
  || die "Mainnet live guard is not UNARMED"

build -p darkbot-bsc-record --bin dbrec_show --bin dbrec_compare
build -p darkbot-bsc-execution --features g07-mainnet --bin g06a_preflight_record
build -p darkbot-bsc-qualification --bin g06a_native_bnb_quote --bin g06a_self_launch_bundle

set -a
# shellcheck disable=SC1090
source "$SECRETS"
set +a

: "${BSC_LAUNCHER_PRIVATE_KEY:?BSC_LAUNCHER_PRIVATE_KEY missing in bsc/secrets.env}"
: "${BSC_BUYER_PRIVATE_KEY:?BSC_BUYER_PRIVATE_KEY missing in bsc/secrets.env}"
# R0 needs eth_getLogs over the whole range: the operator RPC, not a public one.
: "${BSC_MAINNET_RPC_HTTPS:?BSC_MAINNET_RPC_HTTPS (operator RPC with eth_getLogs) missing in bsc/secrets.env}"
export BSC_MAINNET_RPC_HTTPS
export BSC_LAUNCHER_ADDRESS="${BSC_LAUNCHER_ADDRESS:-0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd}"
export BSC_BUYER_ADDRESS="${BSC_BUYER_ADDRESS:-0x9c8a0Db41914beEE52Dd49790c4F087E252743DE}"

export DARKBOT_G06A_LAUNCHER_ADDRESS="$BSC_LAUNCHER_ADDRESS"
export DARKBOT_G06A_BUYER_ADDRESS="$BSC_BUYER_ADDRESS"
export DARKBOT_G06A_LAUNCHER_PRIVATE_KEY="$BSC_LAUNCHER_PRIVATE_KEY"
export DARKBOT_G06A_BUYER_PRIVATE_KEY="$BSC_BUYER_PRIVATE_KEY"

# The frozen attempt-1 candidate, unchanged (0.05 gwei, 0.000001 BNB BUY).
export DARKBOT_G06A_TAX_V3_IMPL="0x024f18294970b5c76c0691b87f138a0317156422"
export DARKBOT_G06A_GAS_PRICE_WEI="50000000"
export DARKBOT_G06A_LAUNCH_GAS_LIMIT="6603270"
export DARKBOT_G06A_BUY_GAS_LIMIT="3000000"
export DARKBOT_G06A_LAUNCH_VALUE_WEI="0"
export DARKBOT_G06A_BUY_VALUE_WEI="1000000000000"
export DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI="50000000"

RUN_ID="g06a_requal_dry_$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$ROOT/bsc/native/evidence/g06a/$RUN_ID"
mkdir -p "$OUT_DIR"
PREFLIGHT="$OUT_DIR/preflight.dbrec"
UPGRADE="$OUT_DIR/portal_upgrade.dbrec"
QUOTE="$OUT_DIR/native_quote.dbrec"
ECON="$OUT_DIR/economics_crosscheck.dbrec"
LOCAL_SIM="$OUT_DIR/local_simulation.dbrec"
DRY="$OUT_DIR/dry_result.dbrec"

printf '\n=== 0/6 R0 + DRY-0: READ-ONLY PREFLIGHT AND PORTAL UPGRADE SCAN ===\n'
SCAN="$("$ENGINE/target/debug/g06a_preflight_record" --out "$PREFLIGHT" \
  --upgrade-scan-from "$FROZEN_DRY_HEAD" --baseline "$FROZEN/preflight.dbrec" --upgrade-out "$UPGRADE" \
  | tee "$OUT_DIR/preflight.view.txt" | grep -E '^(UPGRADES|LAST_UPGRADE_IMPLEMENTATION)=')" \
  || die "read-only preflight or upgrade scan failed"
CURRENT_IMPL="$(get portal_implementation "$PREFLIGHT")"
[[ "$SCAN" == *"UPGRADES=1"* ]] || die "expected exactly one Portal upgrade since block $FROZEN_DRY_HEAD: $SCAN"
[[ "$SCAN" == *"LAST_UPGRADE_IMPLEMENTATION=$CURRENT_IMPL"* ]] || die "upgrade scan does not end at the current implementation"
[[ "$(get wallet_qualification.addresses_bound "$PREFLIGHT")" == "true" ]] || die "wallet addresses are not bound"
[[ "$(get wallet_qualification.distinct "$PREFLIGHT")" == "true" ]] || die "Launcher and Buyer are not distinct"
if [[ "$(get wallet_qualification.funding_pass "$PREFLIGHT")" == "true" ]]; then
  printf 'LIVE_FUNDING=PASS\n'
else
  printf 'LIVE_FUNDING=PENDING  DRY_CONTINUES=true\n'
fi
export DARKBOT_G06A_LAUNCHER_NONCE="$(get launcher_pending_nonce "$PREFLIGHT")"
export DARKBOT_G06A_BUYER_NONCE="$(get buyer_pending_nonce "$PREFLIGHT")"
export DARKBOT_G06A_SEND_HEAD="$(get latest_block_number "$PREFLIGHT")"
export DARKBOT_G06A_MAX_BLOCK="$((DARKBOT_G06A_SEND_HEAD + 2))"
printf 'PREFLIGHT=PASS  head=%s  implementation=%s  launcher_nonce=%s  buyer_nonce=%s\n' \
  "$DARKBOT_G06A_SEND_HEAD" "$CURRENT_IMPL" "$DARKBOT_G06A_LAUNCHER_NONCE" "$DARKBOT_G06A_BUYER_NONCE"

printf '\n=== 1/6 LOCAL CURVE-20 QUOTE ===\n'
"$ENGINE/target/debug/g06a_native_bnb_quote" "$PREFLIGHT" "$QUOTE" > "$OUT_DIR/native_quote.view.txt" \
  || die "native quote failed"
export DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT="$(get expected_output "$QUOTE")"
printf 'QUOTE=PASS  expected_output=%s\n' "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT"

printf '\n=== 2/6 INDEPENDENT READ-ONLY EVM CROSS-CHECK ===\n'
node bsc/tools/g06a_economics_evm_crosscheck.cjs "$PREFLIGHT" "$ECON" > "$OUT_DIR/economics_crosscheck.out.txt" \
  || die "independent EVM cross-check failed"
[[ "$(get status "$ECON")" == "EVM_SEQUENCE_PASS" ]] || die "independent EVM cross-check did not pass"
[[ "$(get simulated_received_amount "$ECON")" == "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT" ]] \
  || die "Curve-20 quote and independent EVM output differ"
export DARKBOT_G06A_SALT="$(get salt "$ECON")"
[[ "$DARKBOT_G06A_SALT" == "$(get salt "$FROZEN/economics_crosscheck.dbrec")" ]] \
  || die "the deterministic salt differs from the frozen candidate: new economic decision required"
printf 'EVM_CROSSCHECK=PASS  exact_output_match=true  frozen_salt=true\n'

printf '\n=== 3/6 BUILDER FLOOR EVIDENCE ===\n'
BUILDER_POLICY="$FROZEN/builder_policy.dbrec"
export DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256="$(shasum -a 256 "$BUILDER_POLICY" | awk '{print $1}')"
printf 'BUILDER_POLICY=%s  sha256=%s  (fresh refresh <=24h before arming is a separate step)\n' \
  "$BUILDER_POLICY" "$DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256"

printf '\n=== 4/6 EXACT WALLET-BOUND LOCAL SIMULATION ===\n'
node bsc/tools/g06a_local_simulate.cjs "$LOCAL_SIM" > "$OUT_DIR/local_simulation.out.txt" \
  || die "exact local simulation failed"
[[ "$(get launcher "$LOCAL_SIM")" == "$(lower "$BSC_LAUNCHER_ADDRESS")" ]] \
  || die "Launcher private key does not match frozen Launcher address"
[[ "$(get buyer "$LOCAL_SIM")" == "$(lower "$BSC_BUYER_ADDRESS")" ]] \
  || die "Buyer private key does not match frozen Buyer address"
[[ "$(get output_difference "$LOCAL_SIM")" == "0" ]] || die "exact local simulation differs from local quote"
[[ "$(get launch_success "$LOCAL_SIM")" == "true" && "$(get buy_success "$LOCAL_SIM")" == "true" ]] \
  || die "exact local launch/BUY sequence failed"
printf 'WALLET_BINDING=PASS\nEXACT_LOCAL_SIMULATION=PASS\n'

printf '\n=== 5/6 DRY1-DRY4 -- ZERO LIVE EGRESS ===\n'
"$ENGINE/target/debug/g06a_self_launch_bundle" --verify-evidence "$LOCAL_SIM" --out "$DRY" \
  > "$OUT_DIR/dry_result.view.txt" || die "final G0.6A DRY qualification failed"
for key in DRY_1_bytes_and_signers DRY_2_independent_local_simulation DRY_3_gas_floor_and_budget DRY_4_expiry_nonces_zero_egress all; do
  value="$(get "dry_pass.$key" "$DRY")"
  printf '%s=%s\n' "$key" "$([[ "$value" == "true" ]] && echo PASS || echo FAIL)"
  [[ "$value" == "true" ]] || die "dry_pass.$key is not true"
done
[[ "$(get network_write_capability "$DRY")" == "NONE_IN_THIS_BINARY" ]] || die "DRY binary reports network write capability"

printf '\n=== 6/6 COMPARISON WITH THE FROZEN DRY ===\n'
for name in preflight economics_crosscheck local_simulation dry_result; do
  "$ENGINE/target/debug/dbrec_compare" "$FROZEN/$name.dbrec" "$OUT_DIR/$name.dbrec" \
    > "$OUT_DIR/COMPARISON_$name.md" || true
done
for field in transactions.launch_tx_hash transactions.buy_tx_hash transactions.token transactions.min_output; do
  [[ "$(get "$field" "$DRY")" == "$(get "$field" "$FROZEN/dry_result.dbrec")" ]] \
    || die "$field differs from the frozen DRY: the signed bytes changed, a new economic approval is required"
done
printf 'SIGNED_BYTES=IDENTICAL_TO_FROZEN_DRY\n'

(cd "$OUT_DIR" && shasum -a 256 ./*.dbrec ./*.txt ./*.md > SHA256SUMS)
printf '\nFINAL_DRY=PASS\nEVIDENCE_DIR=%s\nLIVE_SUBMISSION=NOT_AUTHORIZED\n' "$OUT_DIR"
printf 'NEXT=Review the comparison reports; then contract v4 proposal, builder refresh and readiness. No live run without explicit authorization.\n'
