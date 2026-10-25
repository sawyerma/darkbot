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

SCRIPT_DIR="$(cd "${BASH_SOURCE[0]%/*}" && pwd)"
source "$SCRIPT_DIR/g06a_shell_scope.sh"
clear_exports
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

# Modes (v9.1 D2-D4; BSC-001: no default): --full (A: R0 .. DRY-4 at one
# head, needs the wallet keys); --prepare (key-free preview at H1 for the selected
# candidate: bsc/secrets.env is never read, steps 4 and 5 never run, the
# output is PREPARE_ONLY / NOT_REQUALIFIED); SIGN <prepare_dir> (at a fresh
# head H2: only the verified R0 record and candidate reference of PREPARE
# cross, everything else is recomputed before the wallet-key steps; private
# output directory). Every key-reading mode (--full, --sign) needs the
# explicit signing flag before anything is read; the flag only carries the
# user's authorization, it is not one.
MODE=""
PROFILE=SELF_A
PREPARED=""
case "$#:${1:-}:${2:-}:${3:-}" in
  "1:--full::") MODE=FULL ;;
  "1:--prepare::" | "3:--prepare:--profile:A") MODE=PREPARE ;;
  "3:--prepare:--profile:B") MODE=PREPARE PROFILE=SELF_B ;;
  2:--sign:?*:) MODE=SIGN PREPARED="$2" ;;
  *) printf 'usage: g06a_final_dry.sh --prepare [--profile A|B] | --sign <prepare_dir> | --full\n' >&2; exit 2 ;;
esac
if [[ "$MODE" != PREPARE && "${DARKBOT_G06A_SIGN:-}" != YES ]]; then
  printf 'STOP: --%s signs locally with the wallet keys; set DARKBOT_G06A_SIGN=YES only with the explicit user authorization (AWAITING_USER_AUTHORIZATION)\n' "$(printf '%s' "$MODE" | tr '[:upper:]' '[:lower:]')" >&2
  exit 2
fi

SECRETS="$ROOT/bsc/secrets.env"
ENGINE="$ROOT/bsc/engine"
FROZEN="$ROOT/bsc/native/evidence/g06a/g07_binary_import_20261016"
FROZEN_DRY_HEAD=127865760

die() {
  printf 'STOP: %s\n' "$*" >&2
  exit 1
}

build() {
  (cd "$ENGINE" && run cargo build --quiet --locked "$@") || die "build failed: $*"
}

get() {
  run "$ENGINE/target/debug/dbrec_show" --get "$1" "$2"
}

lower() {
  printf '%s' "$1" | run tr '[:upper:]' '[:lower:]'
}

if [[ "$MODE" != PREPARE ]]; then
  [[ -f "$SECRETS" ]] || die "missing $SECRETS"
  run git check-ignore -q bsc/secrets.env || die "bsc/secrets.env is not ignored by Git"
fi
run grep -Fq 'pub const FROZEN_APPROVED_V5_SHA256: &str = "UNARMED";' \
  bsc/engine/crates/execution/src/acceptance_guard.rs \
  || die "Mainnet live guard is not UNARMED"

build -p darkbot-bsc-record --bin dbrec_show --bin dbrec_compare
build -p darkbot-bsc-execution --features g07-mainnet --bin g06a_preflight_record --bin g06a_dry_handoff
build -p darkbot-bsc-qualification --bin g06a_native_bnb_quote --bin g06a_self_launch_bundle \
  --bin g06a_builder_policy_record --bin g06a_requal_check

if [[ "$MODE" != PREPARE ]]; then
  set +a
  # shellcheck disable=SC1090
  source "$SECRETS"
  set +a
  clear_exports
  : "${BSC_LAUNCHER_PRIVATE_KEY:?BSC_LAUNCHER_PRIVATE_KEY missing in bsc/secrets.env}"
  : "${BSC_BUYER_PRIVATE_KEY:?BSC_BUYER_PRIVATE_KEY missing in bsc/secrets.env}"
  : "${BSC_MAINNET_RPC_HTTPS:?BSC_MAINNET_RPC_HTTPS missing in bsc/secrets.env}"
else
  # Only the read-only RPC endpoint, from the caller's environment.
  : "${BSC_MAINNET_RPC_HTTPS:?PREPARE reads no key file: provide BSC_MAINNET_RPC_HTTPS (read-only RPC) in the environment}"
fi
DARKBOT_G06A_ARCHIVE_RPC_HTTPS="${DARKBOT_G06A_ARCHIVE_RPC_HTTPS:-https://bsc-mainnet.public.blastapi.io}"
BSC_LAUNCHER_ADDRESS="${BSC_LAUNCHER_ADDRESS:-0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd}"
BSC_BUYER_ADDRESS="${BSC_BUYER_ADDRESS:-0x9c8a0Db41914beEE52Dd49790c4F087E252743DE}"

DARKBOT_G06A_LAUNCHER_ADDRESS="$BSC_LAUNCHER_ADDRESS"
DARKBOT_G06A_BUYER_ADDRESS="$BSC_BUYER_ADDRESS"

# The frozen attempt-1 candidate, unchanged (0.05 gwei, 0.000001 BNB BUY).
DARKBOT_G06A_TAX_V3_IMPL="0x024f18294970b5c76c0691b87f138a0317156422"
DARKBOT_G06A_GAS_PRICE_WEI="50000000"
DARKBOT_G06A_LAUNCH_GAS_LIMIT="6603270"
DARKBOT_G06A_BUY_GAS_LIMIT="3000000"
DARKBOT_G06A_LAUNCH_VALUE_WEI="0"
DARKBOT_G06A_BUY_VALUE_WEI="1000000000000"
DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI="50000000"

HANDOFF="$ENGINE/target/debug/g06a_dry_handoff"
ref() { printf '%s\n' "$CANDIDATE_REF" | run awk -F= -v k="$1" '$1 == k { print $2 }'; }
if [[ "$MODE" == PREPARE ]]; then
  # The selected candidate's public reference (store-verified); B's salt
  # stays in the store until exec-salt hands it to one consumer.
  CANDIDATE_REF="$(run "$HANDOFF" selection --profile "$PROFILE")" || die "no verified selected $PROFILE candidate"
  CANDIDATE_TOKEN="$(ref token)"
  CANDIDATE_COMMITMENT="$(ref salt_commitment)"
  RUN_ID="g06a_prepare_$(run date -u +%Y%m%dT%H%M%SZ)"
  OUT_DIR="$ROOT/bsc/native/evidence/g06a/$RUN_ID"
  run mkdir -p "$OUT_DIR"
elif [[ "$MODE" == SIGN ]]; then
  # D3: verify the PREPARE handoff; it creates the new private SIGN
  # directory with only the R0 record and the public candidate record.
  PREPARED="$(cd "$PREPARED" 2>/dev/null && pwd)" || die "no PREPARE directory"
  run git check-ignore -q bsc/.local/g06a_sign || die "bsc/.local is not ignored by Git"
  run mkdir -p "$ROOT/bsc/.local/g06a_sign"
  RUN_ID="g06a_sign_$(run date -u +%Y%m%dT%H%M%SZ)"
  OUT_DIR="$ROOT/bsc/.local/g06a_sign/$RUN_ID"
  CANDIDATE_REF="$(run "$HANDOFF" verify --prepare "$PREPARED" --frozen "$FROZEN" --out "$OUT_DIR")" \
    || die "the PREPARE handoff does not verify (R0 record or candidate)"
  PROFILE="$(ref profile)"
  R0_H1_TO="$(ref r0_to_block)"
  CANDIDATE_RECORD="$OUT_DIR/candidate.dbrec"
  CANDIDATE_TOKEN="$(get_required token "$CANDIDATE_RECORD")" || die "candidate record unreadable"
  CANDIDATE_COMMITMENT="$(get_required salt_commitment "$CANDIDATE_RECORD")" || die "candidate record unreadable"
  [[ "$PROFILE" == SELF_A || "$PROFILE" == SELF_B ]] && [[ "$R0_H1_TO" =~ ^[0-9]+$ ]] || die "malformed handoff reference"
else
  RUN_ID="g06a_requal_dry_$(run date -u +%Y%m%dT%H%M%SZ)"
  OUT_DIR="$ROOT/bsc/native/evidence/g06a/$RUN_ID"
  run mkdir -p "$OUT_DIR"
fi
if [[ "$MODE" != FULL ]]; then
  CANDIDATE_ID="$(ref candidate_id)"
  GENERATION="$(ref generation)"
  [[ "$CANDIDATE_ID" =~ ^0x[[:xdigit:]]{64}$ && "$GENERATION" =~ ^[0-9]+$ \
    && "$CANDIDATE_TOKEN" =~ ^0x[[:xdigit:]]{40}$ && "$CANDIDATE_COMMITMENT" =~ ^0x[[:xdigit:]]{64}$ ]] \
    || die "malformed candidate reference"
  printf 'CANDIDATE=%s  id=%s  generation=%s\n' "$PROFILE" "$CANDIDATE_ID" "$GENERATION"
fi
PREFLIGHT="$OUT_DIR/preflight.dbrec"
SCAN="$OUT_DIR/portal_upgrade_scan.dbrec"
UPGRADE="$OUT_DIR/portal_upgrade.dbrec"
QUOTE="$OUT_DIR/native_quote.dbrec"
ECON="$OUT_DIR/economics_crosscheck.dbrec"
BUILDER_POLICY="$OUT_DIR/builder_policy.dbrec"
LOCAL_SIM="$OUT_DIR/local_simulation.dbrec"
LAUNCH_PATH="$OUT_DIR/launch_path.dbrec"
DRY="$OUT_DIR/dry_result.dbrec"

printf '\n=== R0: PORTAL UPGRADE COVERAGE SINCE THE FROZEN DRY (READ-ONLY) ===\n'
if [[ "$MODE" == SIGN ]]; then
  # The verified H1 coverage; DRY-0 extends it to H2 (a reorged end STOPs).
  run "$ENGINE/target/debug/dbrec_show" "$SCAN" > "$OUT_DIR/portal_upgrade_scan.view.txt" || die "handed-off scan is not a record"
  printf 'R0_SCAN=HANDED_OFF  prepare=%s  to=%s\n' "$PREPARED" "$R0_H1_TO"
elif [[ -n "${DARKBOT_G06A_UPGRADE_SCAN:-}" ]]; then
  run cp "$DARKBOT_G06A_UPGRADE_SCAN" "$SCAN" || die "cannot reuse $DARKBOT_G06A_UPGRADE_SCAN"
  run "$ENGINE/target/debug/dbrec_show" "$SCAN" > "$OUT_DIR/portal_upgrade_scan.view.txt" || die "reused scan is not a record"
  printf 'R0_SCAN=REUSED  from=%s\n' "$DARKBOT_G06A_UPGRADE_SCAN"
else
  run "$ENGINE/target/debug/g06a_preflight_record" --upgrade-scan-only --from "$((FROZEN_DRY_HEAD + 1))" \
    --baseline "$FROZEN/preflight.dbrec" --out "$SCAN" > "$OUT_DIR/portal_upgrade_scan.view.txt" \
    2> "$OUT_DIR/portal_upgrade_scan.progress.txt" || die "R0 coverage failed (see portal_upgrade_scan.progress.txt)"
fi
_dbrec_1="$(get_required scanned_from_block "$SCAN")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_1}" == "$((FROZEN_DRY_HEAD + 1))" ]] || die "R0 scan does not start right after the frozen head"
_dbrec_4="$(get_required scanned_to_block "$SCAN")" || die "required typed DBREC field unavailable"
_dbrec_3="$(get_required headers_verified "$SCAN")" || die "required typed DBREC field unavailable"
_dbrec_2="$(get_required bloom_candidate_blocks "$SCAN")" || die "required typed DBREC field unavailable"
printf 'R0_SCAN=COMPLETE  to=%s  headers=%s  bloom_candidates=%s\n' \
  "${_dbrec_4}" "${_dbrec_3}" "${_dbrec_2}"

printf '\n=== 0/6 DRY-0: READ-ONLY PREFLIGHT, R0 CLOSED AT ITS BLOCK ===\n'
run "$ENGINE/target/debug/g06a_preflight_record" --out "$PREFLIGHT" --upgrade-scan "$SCAN" --upgrade-out "$UPGRADE" \
  > "$OUT_DIR/preflight.view.txt" 2> "$OUT_DIR/preflight.progress.txt" \
  || die "read-only preflight or the closing R0 coverage failed (see preflight.progress.txt)"
if [[ "$MODE" == SIGN ]]; then
  _dbrec_31="$(get_required latest_block_number "$PREFLIGHT")" || die "required typed DBREC field unavailable"
  (( _dbrec_31 > R0_H1_TO )) || die "SIGN needs a fresh head H2 beyond the PREPARE coverage"
fi
_dbrec_5="$(get_required portal_implementation "$PREFLIGHT")" || die "required typed DBREC field unavailable"
CURRENT_IMPL="${_dbrec_5}"
# The typed R0 verdict is g06a_requal_check's; this only reports the count.
printf 'R0_UPGRADES=%s  current_implementation=%s\n' \
  "$(run "$ENGINE/target/debug/dbrec_show" "$UPGRADE" | run grep -c '^upgrades\[[0-9]*\]\.block = ' || true)" "$CURRENT_IMPL"
_dbrec_6="$(get_required wallet_qualification.addresses_bound "$PREFLIGHT")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_6}" == "true" ]] || die "wallet addresses are not bound"
_dbrec_7="$(get_required wallet_qualification.distinct "$PREFLIGHT")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_7}" == "true" ]] || die "Launcher and Buyer are not distinct"
_dbrec_8="$(get_required wallet_qualification.funding_pass "$PREFLIGHT")" || die "required typed DBREC field unavailable"
if [[ "${_dbrec_8}" == "true" ]]; then
  printf 'LIVE_FUNDING=PASS\n'
else
  printf 'LIVE_FUNDING=PENDING  DRY_CONTINUES=true\n'
fi
_dbrec_9="$(get_required launcher_pending_nonce "$PREFLIGHT")" || die "required typed DBREC field unavailable"
DARKBOT_G06A_LAUNCHER_NONCE="${_dbrec_9}"
_dbrec_10="$(get_required buyer_pending_nonce "$PREFLIGHT")" || die "required typed DBREC field unavailable"
DARKBOT_G06A_BUYER_NONCE="${_dbrec_10}"
_dbrec_11="$(get_required latest_block_number "$PREFLIGHT")" || die "required typed DBREC field unavailable"
DARKBOT_G06A_SEND_HEAD="${_dbrec_11}"
# BSC-002: every local EVM run reads exactly this block, by hash.
_dbrec_33="$(get_required latest_block_hash "$PREFLIGHT")" || die "required typed DBREC field unavailable"
DARKBOT_G06A_SEND_HEAD_HASH="${_dbrec_33}"
[[ ${#DARKBOT_G06A_SEND_HEAD} -lt 19 || ( ${#DARKBOT_G06A_SEND_HEAD} -eq 19 && "$DARKBOT_G06A_SEND_HEAD" < "9223372036854775806" ) ]] || die "head exceeds safe shell range"
DARKBOT_G06A_MAX_BLOCK="$((DARKBOT_G06A_SEND_HEAD + 2))"
printf 'PREFLIGHT=PASS  head=%s  implementation=%s  launcher_nonce=%s  buyer_nonce=%s\n' \
  "$DARKBOT_G06A_SEND_HEAD" "$CURRENT_IMPL" "$DARKBOT_G06A_LAUNCHER_NONCE" "$DARKBOT_G06A_BUYER_NONCE"

printf '\n=== 1/6 LOCAL CURVE-20 QUOTE ===\n'
run "$ENGINE/target/debug/g06a_native_bnb_quote" "$PREFLIGHT" "$QUOTE" > "$OUT_DIR/native_quote.view.txt" \
  || die "native quote failed"
_dbrec_12="$(get_required expected_output "$QUOTE")" || die "required typed DBREC field unavailable"
DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT="${_dbrec_12}"
printf 'QUOTE=PASS  expected_output=%s\n' "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT"

printf '\n=== 2/6 INDEPENDENT READ-ONLY EVM CROSS-CHECK ===\n'
# The state of the preflight's block is read on the archive endpoint (the
# configured endpoint keeps only ~128 blocks of state).
if [[ "$PROFILE" == SELF_B ]]; then
  # B: local EVM only, the salt only through the handoff (D1).
  BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
    run "$HANDOFF" exec-salt --candidate "$CANDIDATE_ID" --generation "$GENERATION" --preflight "$PREFLIGHT" -- \
    node bsc/tools/g06a_economics_evm_crosscheck.cjs "$PREFLIGHT" "$ECON" --private > "$OUT_DIR/economics_crosscheck.out.txt" \
    || die "private EVM cross-check failed"
else
  BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
    run node bsc/tools/g06a_economics_evm_crosscheck.cjs "$PREFLIGHT" "$ECON" > "$OUT_DIR/economics_crosscheck.out.txt" \
    || die "independent EVM cross-check failed"
fi
_dbrec_13="$(get_required status "$ECON")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_13}" == "EVM_SEQUENCE_PASS" ]] || die "independent EVM cross-check did not pass"
_dbrec_14="$(get_required simulated_received_amount "$ECON")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_14}" == "$DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT" ]] \
  || die "Curve-20 quote and independent EVM output differ"
if [[ "$PROFILE" == SELF_B ]]; then
  _dbrec_29="$(get_required salt_commitment "$ECON")" || die "required typed DBREC field unavailable"
  [[ "$(lower "${_dbrec_29}")" == "$(lower "$CANDIDATE_COMMITMENT")" ]] \
    || die "the private cross-check is not of the selected candidate's salt"
  printf 'EVM_CROSSCHECK=PASS  exact_output_match=true  private_salt=commitment_only\n'
else
  _dbrec_15="$(get_required salt "$ECON")" || die "required typed DBREC field unavailable"
  # A's frozen salt is public; it reaches only its local consumers (D1).
  A_SALT="${_dbrec_15}"
  _dbrec_16="$(get_required salt "$FROZEN/economics_crosscheck.dbrec")" || die "required typed DBREC field unavailable"
  [[ "$A_SALT" == "${_dbrec_16}" ]] \
    || die "the deterministic salt differs from the frozen candidate: new economic decision required"
  printf 'EVM_CROSSCHECK=PASS  exact_output_match=true  frozen_salt=true\n'
fi
if [[ "$MODE" != FULL ]]; then
  _dbrec_30="$(get_required token "$ECON")" || die "required typed DBREC field unavailable"
  [[ "$(lower "${_dbrec_30}")" == "$(lower "$CANDIDATE_TOKEN")" ]] || die "the cross-check token is not the selected candidate's"
fi

printf '\n=== 3/6 BUILDER FLOOR RE-DETERMINATION (OFFICIAL DOCUMENTATION, READ-ONLY) ===\n'
run "$ENGINE/target/debug/g06a_builder_policy_record" "$FROZEN/builder_policy.dbrec" "$BUILDER_POLICY" \
  > "$OUT_DIR/builder_policy.view.txt" || die "builder requirements changed or could not be re-determined"
_dbrec_17="$(get_required strictest_explicit_verified_floor_wei "$BUILDER_POLICY")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_17}" == "$DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI" ]] \
  || die "re-determined builder floor differs from the frozen candidate: new economic decision required"
DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256="$(required_sha256 "$BUILDER_POLICY")" || die "builder policy SHA unavailable"
printf 'BUILDER_POLICY=RE-DETERMINED  floor_wei=%s  sha256=%s\n' \
  "$DARKBOT_G06A_STRICTEST_BUILDER_FLOOR_WEI" "$DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256"

printf '\n=== 4b/6 LAUNCH-PATH SNAPSHOT (C1/R1, LOCAL EVM, READ-ONLY, KEY-FREE) ===\n'
# Every executed contract's code hash and every storage word selecting one,
# at the preflight's block; the typed verdict is g06a_requal_check's DRY5.
# Independent of step 4 (no signed simulation), so it runs first.
if [[ "$PROFILE" == SELF_B ]]; then
  BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
    run "$HANDOFF" exec-salt --candidate "$CANDIDATE_ID" --generation "$GENERATION" --preflight "$PREFLIGHT" -- \
    node bsc/tools/g06a_launch_path_trace.cjs "$PREFLIGHT" "$LAUNCH_PATH" > "$OUT_DIR/launch_path.out.txt" \
    || die "launch-path trace failed"
else
  BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
    with_public_salt "$A_SALT" node bsc/tools/g06a_launch_path_trace.cjs "$PREFLIGHT" "$LAUNCH_PATH" > "$OUT_DIR/launch_path.out.txt" \
    || die "launch-path trace failed"
fi
run "$ENGINE/target/debug/dbrec_show" "$LAUNCH_PATH" > "$OUT_DIR/launch_path.view.txt" || die "launch path is not a record"
_dbrec_27="$(get_required launch_success "$LAUNCH_PATH")" || die "required typed DBREC field unavailable"
_dbrec_28="$(get_required buy_success "$LAUNCH_PATH")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_27}" == "true" && "${_dbrec_28}" == "true" ]] || die "traced launch/BUY failed"
printf 'LAUNCH_PATH=RECORDED\n'

if [[ "$MODE" == PREPARE ]]; then
  printf '\n=== PREPARE COMPLETE (KEY-FREE; STEPS 4, 5 AND THE REQUALIFICATION VERDICT NOT RUN) ===\n'
  # D3: only the coherent R0 record (by SHA-256) and the candidate reference
  # cross to SIGN; H1 economics never do.
  run "$HANDOFF" prepare --run "$OUT_DIR" --profile "$PROFILE" --frozen "$FROZEN" \
    || die "the H1 R0 record or the candidate reference cannot be handed off"
  printf 'PREPARE_ONLY / NOT_REQUALIFIED: key-free H1 preview of %s %s (generation %s); no signed bytes, no DRY PASS, no v5 readiness or approval\n' \
    "$PROFILE" "$CANDIDATE_ID" "$GENERATION" > "$OUT_DIR/PREPARE_ONLY.txt"
  (cd "$OUT_DIR" && run shasum -a 256 ./*.dbrec ./*.txt > SHA256SUMS)
  printf 'MODE=PREPARE_ONLY  REQUALIFICATION=NOT_RUN  EVIDENCE_DIR=%s\n' "$OUT_DIR"
  printf 'NEXT=SIGN at a fresh head H2 (wallet keys; AWAITING_USER_AUTHORIZATION). No live run without explicit authorization.\n'
  exit 0
fi

printf '\n=== 4/6 EXACT WALLET-BOUND LOCAL SIMULATION ===\n'
if [[ "$PROFILE" == SELF_B ]]; then
  BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
    with_keys "$HANDOFF" exec-salt --candidate "$CANDIDATE_ID" --generation "$GENERATION" --preflight "$PREFLIGHT" -- \
    node bsc/tools/g06a_local_simulate.cjs "$LOCAL_SIM" > "$OUT_DIR/local_simulation.out.txt" \
    || die "exact local simulation failed"
else
  BSC_MAINNET_RPC_HTTPS="$DARKBOT_G06A_ARCHIVE_RPC_HTTPS" \
    with_keys_public_salt "$A_SALT" node bsc/tools/g06a_local_simulate.cjs "$LOCAL_SIM" > "$OUT_DIR/local_simulation.out.txt" \
    || die "exact local simulation failed"
fi
_dbrec_18="$(get_required launcher "$LOCAL_SIM")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_18}" == "$(lower "$BSC_LAUNCHER_ADDRESS")" ]] \
  || die "Launcher private key does not match frozen Launcher address"
_dbrec_19="$(get_required buyer "$LOCAL_SIM")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_19}" == "$(lower "$BSC_BUYER_ADDRESS")" ]] \
  || die "Buyer private key does not match frozen Buyer address"
_dbrec_20="$(get_required output_difference "$LOCAL_SIM")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_20}" == "0" ]] || die "exact local simulation differs from local quote"
_dbrec_22="$(get_required launch_success "$LOCAL_SIM")" || die "required typed DBREC field unavailable"
_dbrec_21="$(get_required buy_success "$LOCAL_SIM")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_22}" == "true" && "${_dbrec_21}" == "true" ]] \
  || die "exact local launch/BUY sequence failed"
printf 'WALLET_BINDING=PASS\nEXACT_LOCAL_SIMULATION=PASS\n'

printf '\n=== 5/6 DRY1-DRY4 -- ZERO LIVE EGRESS ===\n'
if [[ "$PROFILE" == SELF_B ]]; then
  with_keys "$HANDOFF" exec-salt --candidate "$CANDIDATE_ID" --generation "$GENERATION" --preflight "$PREFLIGHT" -- \
    "$ENGINE/target/debug/g06a_self_launch_bundle" --verify-evidence "$LOCAL_SIM" --out "$DRY" \
    > "$OUT_DIR/dry_result.view.txt" || die "final G0.6A DRY qualification failed"
else
  with_keys_public_salt "$A_SALT" "$ENGINE/target/debug/g06a_self_launch_bundle" --verify-evidence "$LOCAL_SIM" --out "$DRY" \
    > "$OUT_DIR/dry_result.view.txt" || die "final G0.6A DRY qualification failed"
fi
drop_keys
for key in DRY_1_bytes_and_signers DRY_2_independent_local_simulation DRY_3_gas_floor_and_budget DRY_4_expiry_nonces_zero_egress all; do
_dbrec_23="$(get_required "dry_pass.$key" "$DRY")" || die "required typed DBREC field unavailable"
  value="${_dbrec_23}"
  printf '%s=%s\n' "$key" "$([[ "$value" == "true" ]] && echo PASS || echo FAIL)"
  [[ "$value" == "true" ]] || die "dry_pass.$key is not true"
done
_dbrec_24="$(get_required network_write_capability "$DRY")" || die "required typed DBREC field unavailable"
[[ "${_dbrec_24}" == "NONE_IN_THIS_BINARY" ]] || die "DRY binary reports network write capability"

printf '\n=== 6/6 TYPED COMPARISON WITH THE FROZEN DRY ===\n'
# Field-by-field reports (exit 1 = differences, which the typed check below
# judges; exit 2 = unreadable input, a STOP).
for name in preflight economics_crosscheck local_simulation dry_result; do
  rc=0
  run "$ENGINE/target/debug/dbrec_compare" "$FROZEN/$name.dbrec" "$OUT_DIR/$name.dbrec" \
    > "$OUT_DIR/COMPARISON_$name.md" || rc=$?
  [[ "$rc" -le 1 ]] || die "dbrec_compare could not read $name (exit $rc)"
done
rc=0
if [[ "$PROFILE" == SELF_B ]]; then
  # B's own run: DRY6 binds the candidate (ID, deployment, token, commitment).
  run "$ENGINE/target/debug/g06a_requal_check" "$OUT_DIR" "$FROZEN" "$BUILDER_POLICY" --candidate "$CANDIDATE_RECORD" \
    > "$OUT_DIR/requalification_status.view.txt" || rc=$?
else
  run "$ENGINE/target/debug/g06a_requal_check" "$OUT_DIR" "$FROZEN" "$BUILDER_POLICY" \
    > "$OUT_DIR/requalification_status.view.txt" || rc=$?
fi
run grep -E '^(FLAP_REQUALIFICATION|STATUS_SHA256)=|^checks\[[0-9]+\]\.(name|status) = ' "$OUT_DIR/requalification_status.view.txt" || true
(cd "$OUT_DIR" && run shasum -a 256 ./*.dbrec ./*.txt ./*.md > SHA256SUMS)
[[ "$rc" -eq 0 ]] || die "FLAP_REQUALIFICATION=STOP (exit $rc): see requalification_status.view.txt; evidence kept in $OUT_DIR"
if [[ "$PROFILE" == SELF_A ]]; then
  for field in transactions.launch_tx_hash transactions.buy_tx_hash transactions.token transactions.min_output; do
  _dbrec_26="$(get_required "$field" "$DRY")" || die "required typed DBREC field unavailable"
  _dbrec_25="$(get_required "$field" "$FROZEN/dry_result.dbrec")" || die "required typed DBREC field unavailable"
    [[ "${_dbrec_26}" == "${_dbrec_25}" ]] \
      || die "$field differs from the frozen DRY: the signed bytes changed, a new economic approval is required"
  done
  printf 'SIGNED_BYTES=IDENTICAL_TO_FROZEN_DRY\n'
else
  _dbrec_32="$(get_required transactions.token "$DRY")" || die "required typed DBREC field unavailable"
  [[ "$(lower "${_dbrec_32}")" == "$(lower "$CANDIDATE_TOKEN")" ]] || die "the DRY token is not the selected candidate's"
  printf 'SIGNED_BYTES=OWN_SELF_B_BYTES (separate economic approval required)\n'
fi
printf '\nFLAP_REQUALIFICATION=PASS\nEVIDENCE_DIR=%s\nLIVE_SUBMISSION=NOT_AUTHORIZED  GUARD=UNARMED\n' "$OUT_DIR"
printf 'NEXT=Review the evidence; then contract v5 proposal and readiness. No live run without explicit authorization.\n'
