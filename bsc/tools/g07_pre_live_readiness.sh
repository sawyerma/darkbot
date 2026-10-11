#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'
umask 077

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT"

SECRETS="$ROOT/bsc/secrets.env"
CONTRACT="$ROOT/bsc/gates/gate0/evidence/g01_mainnet_acceptance_proposal_v2.json"
BUILDER_POLICY="$ROOT/bsc/gates/gate0/evidence/g06a_builder_floor_addendum_2026-10-11.json"
GUARD="$ROOT/bsc/gates/gate0/src/acceptance_guard.rs"

die() {
  printf 'STOP: %s\n' "$*" >&2
  exit 1
}

[[ -f "$SECRETS" ]] || die "missing $SECRETS"
git check-ignore -q bsc/secrets.env || die "bsc/secrets.env is not ignored by Git"

grep -Fq 'pub const FROZEN_APPROVED_V2_SHA256: &str = "UNARMED";' "$GUARD" \
  || die "live guard is armed; read-only readiness refuses to continue"

python3 - "$CONTRACT" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
a = v["authorization"]
if a.get("approved") is not False:
    raise SystemExit("STOP: acceptance.approved is not false")
if a.get("execution_ready") is not False:
    raise SystemExit("STOP: acceptance.execution_ready is not false")
if a.get("approved_contract_sha256") is not None:
    raise SystemExit("STOP: approved_contract_sha256 is already set")
if v.get("status") != "PROPOSED_NOT_AUTHORIZED":
    raise SystemExit("STOP: unexpected acceptance status")
print("LIVE_GUARD=UNARMED")
print("ACCEPTANCE=PROPOSED_NOT_AUTHORIZED")
PY

[[ -f "$BUILDER_POLICY" ]] || die "missing 2026-10-11 builder policy evidence"
python3 - "$BUILDER_POLICY" <<'PY'
import json, sys
v = json.load(open(sys.argv[1]))
if v.get("schema") != "darkbot.g06a.builder-floor-evidence-addendum.v2":
    raise SystemExit("STOP: wrong builder policy schema")
if v.get("observed_date") != "2026-10-11":
    raise SystemExit("STOP: builder policy is not the 2026-10-11 check")
if v.get("status") != "DRY3_POLICY_READY" or v.get("dry3_ready") is not True:
    raise SystemExit("STOP: builder policy is not ready")
if v.get("strictest_explicit_verified_floor_wei") != "50000000":
    raise SystemExit("STOP: unexpected strict builder floor")
if v.get("stage1_signed_gas_price_policy_wei") != "50000000":
    raise SystemExit("STOP: unexpected Stage-1 signed gas policy")
print("BUILDER_POLICY=PASS")
PY

DRY_DIR="$(find "$ROOT/bsc/gates/gate0/evidence" -maxdepth 1 -type d -name 'g06a_final_dry_*' -print | sort | tail -n 1)"
[[ -n "$DRY_DIR" ]] || die "no local g06a_final_dry evidence directory found"

DRY="$DRY_DIR/dry_result.json"
BASELINE="$DRY_DIR/preflight.json"
LOCAL_SIM="$DRY_DIR/local_simulation.json"
ECON="$DRY_DIR/economics_crosscheck.json"
for f in "$DRY" "$BASELINE" "$LOCAL_SIM" "$ECON"; do
  [[ -f "$f" ]] || die "missing G0.6 evidence file: $f"
done

python3 - "$DRY" "$LOCAL_SIM" "$ECON" <<'PY'
import json, sys
dry = json.load(open(sys.argv[1]))
sim = json.load(open(sys.argv[2]))
econ = json.load(open(sys.argv[3]))
d = dry.get("dry_pass", {})
required = (
    "DRY_1_bytes_and_signers",
    "DRY_2_independent_local_simulation",
    "DRY_3_gas_floor_and_budget",
    "DRY_4_expiry_nonces_zero_egress",
)
if dry.get("status") != "G0.6A_SELF_LAUNCH_BUNDLE_DRY":
    raise SystemExit("STOP: wrong G0.6 DRY status")
if not d.get("all") or not all(d.get(k) for k in required):
    raise SystemExit("STOP: G0.6 DRY1..DRY4 are not all PASS")
if dry.get("network_write_capability") != "NONE_IN_THIS_BINARY":
    raise SystemExit("STOP: G0.6 DRY binary had network write capability")
if d.get("zero_egress_structural") is not True:
    raise SystemExit("STOP: zero-egress structural proof missing")
if sim.get("launch_success") is not True or sim.get("buy_success") is not True:
    raise SystemExit("STOP: exact local simulation did not pass")
if str(sim.get("output_difference")) != "0":
    raise SystemExit("STOP: local simulation output mismatch")
if econ.get("status") != "EVM_SEQUENCE_PASS":
    raise SystemExit("STOP: independent EVM cross-check did not pass")
print("G06_FINAL_DRY=PASS")
print("ZERO_LIVE_EGRESS=PASS")
PY

set -a
# shellcheck disable=SC1090
source "$SECRETS"
set +a

export BSC_MAINNET_RPC_HTTPS="${BSC_MAINNET_RPC_HTTPS:-https://bsc-dataseed.bnbchain.org}"
export BSC_LAUNCHER_ADDRESS="${BSC_LAUNCHER_ADDRESS:-0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd}"
export BSC_BUYER_ADDRESS="${BSC_BUYER_ADDRESS:-0x9c8a0Db41914beEE52Dd49790c4F087E252743DE}"
export DARKBOT_G06A_LAUNCHER_ADDRESS="$BSC_LAUNCHER_ADDRESS"
export DARKBOT_G06A_BUYER_ADDRESS="$BSC_BUYER_ADDRESS"

TMP_PREFLIGHT="$(mktemp -t darkbot-g07-prelive.XXXXXX.json)"
trap 'rm -f "$TMP_PREFLIGHT"' EXIT
node bsc/gates/gate0/tools/g06a_preflight_readonly.cjs "$TMP_PREFLIGHT" >/dev/null

python3 - "$TMP_PREFLIGHT" "$BASELINE" <<'PY'
import json, sys
cur = json.load(open(sys.argv[1]))
base = json.load(open(sys.argv[2]))

if cur.get("chain_id") != "56":
    raise SystemExit("STOP: current preflight is not BSC Mainnet")
if cur.get("launcher_address", "").lower() != "0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd":
    raise SystemExit("STOP: Launcher address drift")
if cur.get("buyer_address", "").lower() != "0x9c8a0db41914beee52dd49790c4f087e252743de":
    raise SystemExit("STOP: Buyer address drift")

for key in (
    "chain_id",
    "portal",
    "portal_code_sha256",
    "portal_version",
    "portal_implementation",
    "portal_implementation_code_sha256",
    "tax_v3_impl",
    "tax_v3_impl_code_sha256",
    "native_quote_configuration",
    "protocol_fee_bps",
    "tax_on_bonding_curve_enabled",
):
    if cur.get(key) != base.get(key):
        raise SystemExit(f"STOP: Mainnet deployment/economics drift at {key}")

w = cur["wallet_qualification"]
print(f"CURRENT_HEAD={cur['latest_block_number']}")
print(f"LAUNCHER_NONCE={cur['launcher_pending_nonce']}")
print(f"BUYER_NONCE={cur['buyer_pending_nonce']}")
print(f"LAUNCHER_BALANCE_WEI={cur['launcher_balance_wei']}")
print(f"BUYER_BALANCE_WEI={cur['buyer_balance_wei']}")
print(f"LAUNCHER_REQUIRED_WEI={w['launcher_required_worst_case_wei']}")
print(f"BUYER_REQUIRED_WEI={w['buyer_required_worst_case_wei']}")
print("FUNDING=" + ("PASS" if w.get("funding_pass") is True else "PENDING"))
PY

printf 'G06_EVIDENCE_DIR=%s\n' "$DRY_DIR"
printf 'G06_DRY_SHA256=%s\n' "$(shasum -a 256 "$DRY" | awk '{print $1}')"
printf 'G06_PREFLIGHT_SHA256=%s\n' "$(shasum -a 256 "$BASELINE" | awk '{print $1}')"
printf 'BUILDER_POLICY_SHA256=%s\n' "$(shasum -a 256 "$BUILDER_POLICY" | awk '{print $1}')"
printf 'ACCEPTANCE_V2_SHA256=%s\n' "$(shasum -a 256 "$CONTRACT" | awk '{print $1}')"

printf 'PRE_LIVE_READINESS=PASS_READ_ONLY\n'
printf 'LIVE_SEND=IMPOSSIBLE_FROM_THIS_SCRIPT\n'
printf 'NEXT=Funding must PASS; then explicit user authorization is required before any acceptance arming or G0.7 send.\n'
