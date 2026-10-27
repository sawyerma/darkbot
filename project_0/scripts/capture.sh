#!/usr/bin/env bash
# Project_0 capture entry point (structure iteration 21). Read-only observation: LIVE and submission stay disabled.
#   capture.sh [econ4h] --duration 10m|1h|5h|24h [typed options]   typed C0-E1 measurement via `p0ctl run start`
#   capture.sh econ4h-launcher --duration D [native options]           econ4h capture, native verify + report (p0ctl only)
#   capture.sh market                                                 refused (DEC-07): the raw capture starts through its boot flow
#   capture.sh shadow --duration D [-- OPERATIONAL_ARGS]              bounded Mainnet Shadow via `p0ctl run start --mode shadow`
#   capture.sh shadow-launcher --duration D [-- OPERATIONAL_ARGS]     Shadow runtime of that run (p0ctl only)
# scripts/test.sh (PHASENPLAN operator command) runs `capture.sh econ4h`. Every mode works from any working directory.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PROJECT_DIR/.." && pwd)"
py() { PYTHONPATH="$PROJECT_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -m "$@"; }

# Path-valued settings keep the caller's working directory as their base, also after `cd "$REPO_ROOT"`, so the signer
# gate and the runtime always see the same files.
absolute_paths() {
  local v
  for v in P0_KEYPAIR_PATH P0_OPERATIONAL_CONFIG_FILE P0_RECORD_PATH P0_MT0_PATH \
           P0_ECON_OUT_DIR P0_MARKET_OUT_DIR CARGO_TARGET_DIR; do
    if [[ -n "${!v:-}" && "${!v}" != /* ]]; then export "$v=$PWD/${!v}"; fi
  done
}

# Emit only scheme/hostname; credentials may be in userinfo, path, query or fragment.
endpoint_host() {
  python3 -c 'import os,sys
from urllib.parse import urlsplit
try:
    u=urlsplit(os.environ[sys.argv[1]])
    print(u.scheme + "://" + u.hostname if u.scheme and u.hostname else "INVALID_ENDPOINT")
except ValueError:
    print("INVALID_ENDPOINT")' "$1"
}

load_env() {
# [SEC-02] Local secrets are loaded from project_0/.env when present.
# The file is git-ignored and must be owner-only. Callers may still export values
# explicitly; exported values remain authoritative if no local file exists.
ENV_FILE="${P0_ENV_FILE:-$PROJECT_DIR/.env}"
if [[ -f "$ENV_FILE" ]]; then
  MODE="$(stat -c '%a' "$ENV_FILE" 2>/dev/null || stat -f '%Lp' "$ENV_FILE" 2>/dev/null || true)"
  if [[ "$MODE" != "600" ]]; then
    echo "insecure Project_0 env permissions: $ENV_FILE mode=$MODE (require 600)" >&2
    exit 2
  fi
  set -a
  # shellcheck source=/dev/null
  source "$ENV_FILE"
  set +a
fi
}

# Shared signer gate of econ4h and shadow: public identity, endpoints, owner-only keypair outside the repository.
require_signer() {
# Public signer identity is versioned; secret key material is not.
# shellcheck source=/dev/null
source "$PROJECT_DIR/config/wallet.public.env"

# Project_0 first Mainnet Shadow qualification.
# Secrets/endpoints stay local; this script never writes or prints them.

need() {
  local k="$1"
  if [[ -z "${!k:-}" ]]; then
    echo "missing required env: $k" >&2
    exit 2
  fi
}

if [[ -z "${P0_RPC_URL:-}" || -z "${P0_WS_URL:-}" ]]; then
  if [[ -z "${HELIUS_API_KEY:-}" ]]; then
    echo "missing endpoints: set P0_RPC_URL+P0_WS_URL or HELIUS_API_KEY" >&2
    exit 2
  fi
  export P0_RPC_URL="${P0_RPC_URL:-https://mainnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}}"
  export P0_WS_URL="${P0_WS_URL:-wss://mainnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}}"
fi

export P0_GROUP_ID="${P0_GROUP_ID:-4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8}"

need P0_KEYPAIR_PATH
need P0_MAX_SLIPPAGE_BPS
need P0_EXPECTED_PUBKEY

# [SEC-02] Fail closed on wrong or weakly protected signer.
KEYPAIR_REAL="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$P0_KEYPAIR_PATH")"
case "$KEYPAIR_REAL" in
  "$REPO_ROOT"/*)
    echo "refusing Project_0 signer stored inside repository" >&2
    exit 2
    ;;
esac
if [[ ! -f "$KEYPAIR_REAL" ]]; then
  echo "Project_0 keypair file not found" >&2
  exit 2
fi
KEYPAIR_MODE="$(stat -c '%a' "$KEYPAIR_REAL" 2>/dev/null || stat -f '%Lp' "$KEYPAIR_REAL" 2>/dev/null || true)"
if [[ "$KEYPAIR_MODE" != "600" ]]; then
  echo "insecure Project_0 keypair permissions: mode=$KEYPAIR_MODE (require 600)" >&2
  exit 2
fi
# Verify a standard Solana 64-byte JSON keypair without depending on a globally
# installed Solana CLI. The public key is the final 32 bytes of the keypair file.
ACTUAL_PUBKEY="$(
  python3 - "$KEYPAIR_REAL" <<'PY'
import json
import sys

alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
path = sys.argv[1]

try:
    with open(path, "r", encoding="utf-8") as fh:
        raw = json.load(fh)
except Exception as exc:
    print(f"cannot read Project_0 keypair: {exc}", file=sys.stderr)
    raise SystemExit(2)

if (
    not isinstance(raw, list)
    or len(raw) != 64
    or any(not isinstance(v, int) or isinstance(v, bool) or v < 0 or v > 255 for v in raw)
):
    print("invalid Project_0 Solana keypair JSON: expected exactly 64 byte values", file=sys.stderr)
    raise SystemExit(2)

pub = bytes(raw[32:64])
n = int.from_bytes(pub, "big")
encoded = ""
while n:
    n, rem = divmod(n, 58)
    encoded = alphabet[rem] + encoded

leading_zeroes = 0
for b in pub:
    if b != 0:
        break
    leading_zeroes += 1

print("1" * leading_zeroes + encoded)
PY
)" || {
  echo "failed to derive Project_0 signer identity" >&2
  exit 2
}
if [[ "$ACTUAL_PUBKEY" != "$P0_EXPECTED_PUBKEY" ]]; then
  echo "Project_0 signer identity mismatch: derived=$ACTUAL_PUBKEY expected=$P0_EXPECTED_PUBKEY" >&2
  exit 2
fi
}

econ4h_launcher() {
# Started only by `p0ctl run start` (boot audit, owner lock, operator Mainnet approval).
if [[ -z "${P0_CTL_LAUNCH_RUN_ID:-}" ]]; then
  echo "LAUNCHER_REQUIRES_P0CTL: econ4h-launcher is started only by p0ctl run start; use scripts/test.sh --duration D" >&2; exit 2
fi
load_env
absolute_paths
case "${P0_ECON_RUN_MODE:-auto}" in
  auto|full) ;;
  *) echo "P0_ECON_RUN_MODE quick is incompatible with a complete capture; unset it. Duration now starts after StreamReady." >&2; exit 2 ;;
esac

# Native, typed launch arguments (JFI-04): no JSON configuration handoff.
usage() {
  echo 'Usage: capture.sh econ4h-launcher --duration 10m|1h|5h|24h [--capture-source logs|transactions]
         [--chain-verify off|strict] [--verify-rps N] [--verify-calls N] [--verify-seconds N] [--verify-bytes N]
         [--verify-disk-bytes N] [--verify-ram-bytes N] [--verify-min-free-bytes N] [--verify-max-pages N]
         [--provider-evidence FILE.p0j]
         [--report-* VALUE ...] [--format table|csv] [-- OPERATIONAL_ARGS...]' >&2
  exit 2
}
DURATION=""; CHAIN_VERIFY=off; CAPACITY_VERIFY=(); STRICT_VERIFY=(); REPORT_ARGS=(); RUNTIME_ARGS=()
while [[ "$#" -gt 0 ]]; do
  if [[ "$1" == -- ]]; then
    shift
    # A relative `--config FILE` keeps the caller's working directory as its base (as absolute_paths).
    while [[ "$#" -gt 0 ]]; do
      RUNTIME_ARGS+=("$1")
      if [[ "$1" == --config && "$#" -ge 2 ]]; then
        shift; if [[ "$1" == /* ]]; then RUNTIME_ARGS+=("$1"); else RUNTIME_ARGS+=("$PWD/$1"); fi
      fi
      shift
    done
    break
  fi
  [[ "$#" -ge 2 ]] || usage  # every launch option takes one value
  case "$1" in
    --duration) [[ -z "$DURATION" ]] || usage; DURATION="$2" ;;
    --capture-source) export P0_MARKET_SOURCE="$2" ;;
    --chain-verify|--verify) CHAIN_VERIFY="$2" ;;
    --verify-rps|--verify-calls|--verify-seconds|--verify-bytes|--verify-disk-bytes)
      CAPACITY_VERIFY+=("$1" "$2"); STRICT_VERIFY+=("$1" "$2") ;;
    --verify-max-pages) CAPACITY_VERIFY+=("$1" "$2") ;;
    --verify-ram-bytes|--verify-min-free-bytes|--provider-evidence) STRICT_VERIFY+=("$1" "$2") ;;
    --report-*|--format|--verification-result) REPORT_ARGS+=("$1" "$2") ;;
    *) usage ;;
  esac
  shift 2
done
[[ -n "$DURATION" ]] || usage
case "$CHAIN_VERIFY" in
  off|strict) ;;
  index|crosscheck)
    echo "CHAIN_VERIFY_${CHAIN_VERIFY}_BLOCKED: the index/crosscheck verifier exists only as the pinned Python reader, which writes JSON evidence (JFI-03); capture with --chain-verify strict or off. Historical index verification stays an explicit offline run of scripts/verify.sh." >&2
    exit 2 ;;
  *) usage ;;
esac
export P0_MEASUREMENT_DURATION="$DURATION"

require_signer
export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
case "${P0_LIVE_ENABLED:-false}:${P0_SUBMISSION_ENABLED:-false}" in
  false:false|0:0|false:0|0:false) ;;
  *) echo "econ4h requires LIVE and submission disabled" >&2; exit 2 ;;
esac
# D11: the recovery root waits (P0_RECOVERY_ROOT_*) have one default, the runtime's
# (runtime_config::defaults: catch-up 120s, stall 60s, cap 300s); an operator value passes through.
export P0_MODE=econ4h
export P0_LIVE_ENABLED=false
export P0_SUBMISSION_ENABLED=false
export P0_ECON_RUN_ID="${P0_ECON_RUN_ID:-econ4h_$(date -u +%Y%m%dT%H%M%SZ)}"
export P0_MARKET_SOURCE="${P0_MARKET_SOURCE:-logs}"
# Run storage contract: data/runs (layout V2) by default, data/econ4h (layout V1) with
# P0_RUN_STORE=legacy. The run id is validated and the run directory is created exactly once.
P0_RUN_LAYOUT="$(py operations.capture.run_store layout econ4h "${P0_ECON_OUT_DIR:-}")" || exit 2
P0_ECON_OUT_DIR="$(py operations.capture.run_store root econ4h "${P0_ECON_OUT_DIR:-}")" || exit 2
export P0_ECON_OUT_DIR P0_RUN_LAYOUT
# FEUI-01: one runtime owner per data store (p0-longtail refuses a second owner); p0ctl uses the same file.
export P0_RUNTIME_LOCK="${P0_RUNTIME_LOCK:-$(dirname "$P0_ECON_OUT_DIR")/.p0-runtime.lock}"
# The run's code identity (head, source hashes) is read from this checkout, not the build path.
export P0_PROJECT_DIR="$PROJECT_DIR"
RUN_DIR="$(py operations.capture.run_store allocate "$P0_ECON_OUT_DIR" "$P0_ECON_RUN_ID" "$P0_RUN_LAYOUT")" || exit 2
if [[ "$P0_RUN_LAYOUT" == v2 ]]; then
  LOG_DIR="$RUN_DIR/logs"; CAPTURE_DIR="$RUN_DIR/capture"
else
  LOG_DIR="$RUN_DIR"; CAPTURE_DIR="$RUN_DIR"
fi
cd "$REPO_ROOT"
echo "C0-E1: build; capture starts after bootstrap and StreamReady; submission disabled"
cargo build --release --locked --manifest-path "$PROJECT_DIR/native/Cargo.toml" \
  --features "crate_health txv1" --bin p0-longtail --bin p0-measurement-plan --bin p0-strict-verify \
  --bin p0-journal --bin p0-capacity --bin p0ctl 2>&1 | tee "$LOG_DIR/build.log" || {
  echo "Build failed: $LOG_DIR/build.log" >&2; exit 2;
}
BIN="${CARGO_TARGET_DIR:-$PROJECT_DIR/native/target}/release/p0-longtail"
BIN_DIR="$(dirname "$BIN")"
# Plan is compiled exactly once. Every consumer loads this persisted typed plan.
if [[ -n "${P0_MEASUREMENT_PLAN:-}" ]]; then
  echo "launcher creates its own plan; unset P0_MEASUREMENT_PLAN" >&2; exit 2
fi
mkdir -p "$CAPTURE_DIR"
# JFI-04: the runtime reads the typed plan journal; stdout is its duration in seconds (text).
P0_ECON_DURATION_SECS="$("$BIN_DIR/p0-measurement-plan" --out "$CAPTURE_DIR/measurement_plan.p0j")" || exit 2
export P0_ECON_DURATION_SECS P0_MEASUREMENT_PLAN="$CAPTURE_DIR/measurement_plan.p0j"
# DEC-16: an unbounded run (until-stopped:W:R) measures windows of W until its stop; its capacity
# preflight covers its minimum runway R (a fixed run: its duration).
CAPACITY_SECS="$("$BIN_DIR/p0-measurement-plan" --runway)" || exit 2
echo "Capture: ${P0_ECON_DURATION_SECS}s (capacity horizon ${CAPACITY_SECS}s); separate bootstrap, drain and verification budgets in the measurement plan"
# SK-06 capacity preflight (native): measured lane rates of the newest sealed earlier runs, a write
# and sync probe on this volume, OS memory/CPU, operational budgets and the verifier budget of the
# selected chain-verify mode. A capture longer than 600 s starts only when QUALIFIED; the native
# report capture/capacity_preflight.p0j is the record.
"$BIN_DIR/p0-capacity" --duration "$CAPACITY_SECS" --out "$CAPTURE_DIR" --capture-source "$P0_MARKET_SOURCE" \
  --basis-root "$P0_ECON_OUT_DIR" --exclude-run "$RUN_DIR" --chain-verify "$CHAIN_VERIFY" ${CAPACITY_VERIFY[@]+"${CAPACITY_VERIFY[@]}"} \
  | tee "$LOG_DIR/capacity_preflight.txt" || {
  echo "Capacity preflight: start not allowed (report: $LOG_DIR/capacity_preflight.txt). Nothing was deleted." >&2; exit 2;
}
# Binary journals (PERSIST-01); the daily summary is a frame of the opportunity journal.
export P0_RECORD_PATH="$CAPTURE_DIR/opportunities.p0j"
export P0_MT0_PATH="$CAPTURE_DIR/mt0.p0j"
export P0_ZERO_PREFUND_QUALIFIED=false
export P0_ZERO_PREFUND_SHADOW_PROBE=true
# DEC-05: the owner limits (P0_MIN_PROFIT, P0_MAX_*, P0_JUPITER_MAX_ACCOUNTS, …) pass through;
# their one default is the runtime's parameter registry.

mkdir -p "$(dirname "$P0_RECORD_PATH")"
# The runtime binds code identity (head, binary and source hashes) in its typed run manifest.
status=0
# All durations follow the same capture lifecycle. Never timebox the entire
# process by capture duration: bootstrap and verification have separate budgets.
"$BIN" ${RUNTIME_ARGS[@]+"${RUNTIME_ARGS[@]}"} > "$LOG_DIR/console_summary.txt" 2> >(tee "$LOG_DIR/runtime.log" >&2) || status=$?
wait # stderr tee must finish before immutable evidence hashing
cat "$LOG_DIR/console_summary.txt"
if [[ -f "$CAPTURE_DIR/run_manifest.p0j" ]]; then
  # Post-capture production chain (C0E1-P5-AUTO), one native entry over the run's .p0j records:
  # verification (off|strict), native report, master qualification. No JSON is written.
  "$BIN_DIR/p0ctl" --project "$PROJECT_DIR" post-capture --run "$RUN_DIR" --chain-verify "$CHAIN_VERIFY" \
    ${STRICT_VERIFY[@]+"${STRICT_VERIFY[@]}"} ${REPORT_ARGS[@]+"${REPORT_ARGS[@]}"} \
    > "$LOG_DIR/post_capture_report.txt" || status=2
fi
echo "Reports and detailed logs: $RUN_DIR"
exit "$status"
}

# Operator entry (`test.sh --duration D`): the native control plane validates the arguments, runs
# the offline boot audit, owns the run (lock, stop) and starts econ4h-launcher. No Python launcher
# and no JSON handoff (JFI-04).
econ4h() {
if [[ -n "${P0_CTL_LAUNCH_RUN_ID:-}" ]]; then
  echo "RECURSIVE_LAUNCH: capture.sh econ4h inside the launcher of run $P0_CTL_LAUNCH_RUN_ID" >&2; exit 2
fi
absolute_paths
# The build runs in the repository; p0ctl and its launcher keep the caller's working directory, the
# base of relative paths such as `-- --config FILE`.
(cd "$REPO_ROOT" && cargo build --release --locked --quiet --manifest-path "$PROJECT_DIR/native/Cargo.toml" \
  --features "crate_health txv1" --bin p0ctl >&2) || { echo "p0ctl build failed" >&2; exit 2; }
exec "${CARGO_TARGET_DIR:-$PROJECT_DIR/native/target}/release/p0ctl" --project "$PROJECT_DIR" run start "$@"
}

# DEC-07 (P0-013): a raw capture reads Mainnet only through its boot flow (mode market): the
# desktop capture p0-market-capture-reported, started by `darkbotctl p0 boot start` with the
# admission of its approved network record. This launcher's p0-longtail market mode is never
# admitted, so it starts nothing.
market() {
echo "BOOT_AUDIT_REQUIRED: capture.sh market starts no raw capture; it starts only through its boot flow (Capture tab, or darkbotctl p0 boot offline --mode market --duration D, network and start approval)" >&2
exit 2
}

# Operator entry of a bounded Mainnet Shadow run (P6-MT MT11/MT12): the native control plane validates
# the arguments, runs the boot audit, requires the Mainnet approval, owns the run (lock, stop) and starts
# shadow-launcher. F-01 (CTRL-01/03): there is no Shadow start outside this controller.
shadow() {
if [[ -n "${P0_CTL_LAUNCH_RUN_ID:-}" ]]; then
  echo "RECURSIVE_LAUNCH: capture.sh shadow inside the launcher of run $P0_CTL_LAUNCH_RUN_ID" >&2; exit 2
fi
absolute_paths
(cd "$REPO_ROOT" && cargo build --release --locked --quiet --manifest-path "$PROJECT_DIR/native/Cargo.toml" \
  --features "crate_health txv1" --bin p0ctl >&2) || { echo "p0ctl build failed" >&2; exit 2; }
exec "${CARGO_TARGET_DIR:-$PROJECT_DIR/native/target}/release/p0ctl" --project "$PROJECT_DIR" run start --mode shadow "$@"
}

shadow_launcher() {
# Started only by `p0ctl run start --mode shadow` (boot audit, owner lock, operator Mainnet approval).
if [[ -z "${P0_CTL_LAUNCH_RUN_ID:-}" ]]; then
  echo "LAUNCHER_REQUIRES_P0CTL: shadow-launcher is started only by p0ctl run start --mode shadow; use capture.sh shadow --duration D" >&2; exit 2
fi
load_env
absolute_paths
usage() { echo 'Usage: capture.sh shadow-launcher --duration D [-- OPERATIONAL_ARGS...]' >&2; exit 2; }
DURATION=""; RUNTIME_ARGS=()
while [[ "$#" -gt 0 ]]; do
  if [[ "$1" == -- ]]; then
    shift
    # A relative `--config FILE` keeps the caller's working directory as its base (as absolute_paths).
    while [[ "$#" -gt 0 ]]; do
      RUNTIME_ARGS+=("$1")
      if [[ "$1" == --config && "$#" -ge 2 ]]; then
        shift; if [[ "$1" == /* ]]; then RUNTIME_ARGS+=("$1"); else RUNTIME_ARGS+=("$PWD/$1"); fi
      fi
      shift
    done
    break
  fi
  [[ "$#" -ge 2 && "$1" == --duration && -z "$DURATION" ]] || usage
  DURATION="$2"; shift 2
done
[[ -n "$DURATION" ]] || usage
case "${P0_SHADOW_RUN_ID:-}" in
  ''|.*|*[!A-Za-z0-9_.-]*) echo "P0_SHADOW_RUN_ID invalid or missing (set by p0ctl run start)" >&2; exit 2 ;;
esac
require_signer
case "${P0_LIVE_ENABLED:-false}:${P0_SUBMISSION_ENABLED:-false}" in
  false:false|0:0|false:0|0:false) ;;
  *) echo "shadow requires LIVE and submission disabled" >&2; exit 2 ;;
esac
export P0_LIVE_ENABLED=false P0_SUBMISSION_ENABLED=false
# The bounded operational window: the runtime resolves its typed configuration with this duration
# (the same rule p0ctl audited) and ends after it, counted from Ready.
export P0_MEASUREMENT_DURATION="$DURATION"
export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
export P0_MODE=shadow
export P0_ZERO_PREFUND_QUALIFIED=false
export P0_ZERO_PREFUND_SHADOW_PROBE=true
# DEC-05: the owner limits (P0_MIN_PROFIT, P0_MAX_*, P0_JUPITER_MAX_ACCOUNTS, …) pass through;
# their one default is the runtime's parameter registry.
export P0_RUNTIME_LOCK="${P0_RUNTIME_LOCK:-$PROJECT_DIR/data/.p0-runtime.lock}"
# The run's code identity (head, source hashes) is read from this checkout, not the build path.
export P0_PROJECT_DIR="$PROJECT_DIR"
# One directory per Shadow run, created exactly once: journals are never appended across runs.
RUN_DIR="$PROJECT_DIR/data/shadow/$P0_SHADOW_RUN_ID"
mkdir -p "$PROJECT_DIR/data/shadow"
mkdir "$RUN_DIR" 2>/dev/null || { echo "RUN_EXISTS: $RUN_DIR" >&2; exit 2; }
export P0_RECORD_PATH="$RUN_DIR/opportunities.p0j"

cd "$REPO_ROOT"
cargo build --release --locked --manifest-path "$PROJECT_DIR/native/Cargo.toml" \
  --features "crate_health txv1" --bin p0-longtail --bin p0-measurement-plan --bin p0ctl >&2 || {
  echo "Build failed" >&2; exit 2;
}
BIN="${CARGO_TARGET_DIR:-$PROJECT_DIR/native/target}/release/p0-longtail"
BIN_DIR="$(dirname "$BIN")"
# JFI-04: the runtime reads the typed plan journal; stdout is its duration in seconds (text).
SHADOW_SECS="$("$BIN_DIR/p0-measurement-plan" --out "$RUN_DIR/measurement_plan.p0j")" || exit 2
export P0_MEASUREMENT_PLAN="$RUN_DIR/measurement_plan.p0j"

echo "Project_0 Mainnet Shadow (run $P0_SHADOW_RUN_ID, ${SHADOW_SECS}s from Ready)"
echo "  program=$P0_PROGRAM_ID"
echo "  group=$P0_GROUP_ID"
echo "  rpc_host=$(endpoint_host P0_RPC_URL)"
echo "  ws_host=$(endpoint_host P0_WS_URL)"
echo "  mode=$P0_MODE"
echo "  zero_prefund_qualified=$P0_ZERO_PREFUND_QUALIFIED"
echo "  zero_prefund_shadow_probe=$P0_ZERO_PREFUND_SHADOW_PROBE"
echo "  record=$P0_RECORD_PATH"
echo "  journals=binary .p0j: record (opportunities, reconcile, daily summary) and <record>.mt0.p0j"
exec "$BIN" ${RUNTIME_ARGS[@]+"${RUNTIME_ARGS[@]}"}
}

case "${1:-}" in
  econ4h-launcher) shift; econ4h_launcher "$@" ;;
  market) shift; market "$@" ;;
  shadow) shift; shadow "$@" ;;
  shadow-launcher) shift; shadow_launcher "$@" ;;
  econ4h) shift; econ4h "$@" ;;
  *) econ4h "$@" ;;
esac
