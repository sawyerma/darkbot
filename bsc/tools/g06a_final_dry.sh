#!/usr/bin/env bash
# G0.6a requalification DRY (R0, DRY-0 .. DRY-4) of the frozen G0.7 attempt-1
# candidate, binary records only (DBREC001). No JSON is written or parsed:
# every step writes a record and the checks read typed fields with
# `dbrec_show --get`; the verdict is the typed `g06a_requal_check`. DRY-1
# signs locally; nothing is sent (read-only RPC and public documentation
# reads only, no builder), and the guard must be UNARMED. The previous JSON
# chain is kept unchanged as g06a_final_dry_legacy_v2.sh (historical only).
#
# R0 needs no eth_getLogs: it verifies every header from the frozen DRY's
# block on (hash chain, logs bloom, receipts against receiptsRoot) on the
# configured BSC_MAINNET_RPC_HTTPS and anchors both ends on
# DARKBOT_G06A_ARCHIVE_RPC_HTTPS (default: the free public Blast endpoint),
# which also serves the historical state the simulations read. The scan of
# an earlier run of this script can be reused with
# DARKBOT_G06A_UPGRADE_SCAN=<its portal_upgrade_scan.dbrec>; DRY-0 extends
# it to the preflight's block in any case.
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
build -p darkbot-bsc-qualification --bin g06a_native_bnb_quote --bin g06a_self_launch_bundle \
  --bin g06a_builder_policy_record --bin g06a_requal_check

set -a
# shellcheck disable=SC1090
source "$SECRETS"
set +a

: "${BSC_LAUNCHER_PRIVATE_KEY:?BSC_LAUNCHER_PRIVATE_KEY missing in bsc/secrets.env}"
: "${BSC_BUYER_PRIVATE_KEY:?BSC_BUYER_PRIVATE_KEY missing in bsc/secrets.env}"
: "${BSC_MAINNET_RPC_HTTPS:?BSC_MAINNET_RPC_HTTPS missing in bsc/secrets.env}"
export BSC_MAINNET_RPC_HTTPS
export DARKBOT_G06A_ARCHIVE_RPC_HTTPS="${DARKBOT_G06A_ARCHIVE_RPC_HTTPS:-https://bsc-mainnet.public.blastapi.io}"
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
SCAN="$OUT_DIR/portal_upgrade_scan.dbrec"
UPGRADE="$OUT_DIR/portal_upgrade.dbrec"
QUOTE="$OUT_DIR/native_quote.dbrec"
ECON="$OUT_DIR/economics_crosscheck.dbrec"
BUILDER_POLICY="$OUT_DIR/builder_policy.dbrec"
LOCAL_SIM="$OUT_DIR/local_simulation.dbrec"
DRY="$OUT_DIR/dry_result.dbrec"

printf '\n=== R0: PORTAL UPGRADE COVERAGE SINCE THE FROZEN DRY (READ-ONLY) ===\n'
if [[ -n "${DARKBOT_G06A_UPGRADE_SCAN:-}" ]]; then
  cp "$DARKBOT_G06A_UPGRADE_SCAN" "$SCAN" || die "cannot reuse $DARKBOT_G06A_UPGRADE_SCAN"
  "$ENGINE/target/debug/dbrec_show" "$SCAN" > "$OUT_DIR/portal_upgrade_scan.view.txt" || die "reused scan is not a record"
  printf 'R0_SCAN=REUSED  from=%s\n' "$DARKBOT_G06A_UPGRADE_SCAN"
else
  "$ENGINE/target/debug/g06a_preflight_record" --upgrade-scan-only --from "$((FROZEN_DRY_HEAD + 1))" \
    --baseline "$FROZEN/preflight.dbrec" --out "$SCAN" > "$OUT_DIR/portal_upgrade_scan.view.txt" \
    2> "$OUT_DIR/portal_upgrade_scan.progress.txt" || die "R0 coverage failed (see portal_upgrade_scan.progress.txt)"
fi
[[ "$(get scanned_from_block "$SCAN")" == "$((FROZEN_DRY_HEAD + 1))" ]] || die "R0 scan does not start right after the frozen head"
printf 'R0_SCAN=COMPLETE  to=%s  headers=%s  bloom_candidates=%s\n' \
  "$(get scanned_to_block "$SCAN")" "$(get headers_verified "$SCAN")" "$(get bloom_candidate_blocks "$SCAN")"

printf '\n=== 0/6 DRY-0: READ-ONLY PREFLIGHT, R0 CLOSED AT ITS BLOCK ===\n'
"$ENGINE/target/debug/g06a_preflight_record" --out "$PREFLIGHT" --upgrade-scan "$SCAN" --upgrade-out "$UPGRADE" \
  > "$OUT_DIR/preflight.view.txt" 2> "$OUT_DIR/preflight.progress.txt" \
  || die "read-only preflight or the closing R0 coverage failed (see preflight.progress.txt)"
CURRENT_IMPL="$(get portal_implementation "$PREFLIGHT")"
# The typed R0 verdict is g06a_requal_check's; this only reports the count.
printf 'R0_UPGRADES=%s  current_implementation=%s\n' \
  "$("$ENGINE/target/debug/dbrec_show" "$UPGRADE" | grep -c '^upgrades\[[0-9]*\]\.block = ' || true)" "$CURRENT_IMPL"
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
# The state of the preflight's block is read on the archive endpoint (the
# configured endpoint keeps only ~128 blocks of state).
BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
  node bsc/tools/g06a_economics_evm_crosscheck.cjs "$PREFLIGHT" "$ECON" > "$OUT_DIR/economics_crosscheck.out.txt" \
  || die "independent EVM cross-check failed"
[[ "$(get status "$ECON")" == "EVM_SEQUENCE_PASS" ]] || die "independent EVM cross-check did not pass"
[[ "$(get simulated_received_amount "$ECON")" == "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT" ]] \
  || die "Curve-20 quote and independent EVM output differ"
export DARKBOT_G06A_SALT="$(get salt "$ECON")"
[[ "$DARKBOT_G06A_SALT" == "$(get salt "$FROZEN/economics_crosscheck.dbrec")" ]] \
  || die "the deterministic salt differs from the frozen candidate: new economic decision required"
printf 'EVM_CROSSCHECK=PASS  exact_output_match=true  frozen_salt=true\n'

printf '\n=== 3/6 BUILDER FLOOR RE-DETERMINATION (OFFICIAL DOCUMENTATION, READ-ONLY) ===\n'
"$ENGINE/target/debug/g06a_builder_policy_record" "$FROZEN/builder_policy.dbrec" "$BUILDER_POLICY" \
  > "$OUT_DIR/builder_policy.view.txt" || die "builder requirements changed or could not be re-determined"
[[ "$(get strictest_explicit_verified_floor_wei "$BUILDER_POLICY")" == "$DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI" ]] \
  || die "re-determined builder floor differs from the frozen candidate: new economic decision required"
export DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256="$(shasum -a 256 "$BUILDER_POLICY" | awk '{print $1}')"
printf 'BUILDER_POLICY=RE-DETERMINED  floor_wei=%s  sha256=%s\n' \
  "$DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI" "$DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256"

printf '\n=== 4/6 EXACT WALLET-BOUND LOCAL SIMULATION ===\n'
BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
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

printf '\n=== 6/6 TYPED COMPARISON WITH THE FROZEN DRY ===\n'
# Field-by-field reports (exit 1 = differences, which the typed check below
# judges; exit 2 = unreadable input, a STOP).
for name in preflight economics_crosscheck local_simulation dry_result; do
  rc=0
  "$ENGINE/target/debug/dbrec_compare" "$FROZEN/$name.dbrec" "$OUT_DIR/$name.dbrec" \
    > "$OUT_DIR/COMPARISON_$name.md" || rc=$?
  [[ "$rc" -le 1 ]] || die "dbrec_compare could not read $name (exit $rc)"
done
rc=0
"$ENGINE/target/debug/g06a_requal_check" "$OUT_DIR" "$FROZEN" "$BUILDER_POLICY" \
  > "$OUT_DIR/requalification_status.view.txt" || rc=$?
grep -E '^(FLAP_REQUALIFICATION|STATUS_SHA256)=|^checks\[[0-9]+\]\.(name|status) = ' "$OUT_DIR/requalification_status.view.txt" || true
(cd "$OUT_DIR" && shasum -a 256 ./*.dbrec ./*.txt ./*.md > SHA256SUMS)
[[ "$rc" -eq 0 ]] || die "FLAP_REQUALIFICATION=STOP (exit $rc): see requalification_status.view.txt; evidence kept in $OUT_DIR"
for field in transactions.launch_tx_hash transactions.buy_tx_hash transactions.token transactions.min_output; do
  [[ "$(get "$field" "$DRY")" == "$(get "$field" "$FROZEN/dry_result.dbrec")" ]] \
    || die "$field differs from the frozen DRY: the signed bytes changed, a new economic approval is required"
done
printf 'SIGNED_BYTES=IDENTICAL_TO_FROZEN_DRY\n'
printf '\nFLAP_REQUALIFICATION=PASS\nEVIDENCE_DIR=%s\nLIVE_SUBMISSION=NOT_AUTHORIZED  GUARD=UNARMED\n' "$OUT_DIR"
printf 'NEXT=Review the evidence; then contract v4 proposal and readiness. No live run without explicit authorization.\n'
