#!/usr/bin/env bash
# Project_0 capture entry point (structure iteration 21). Read-only observation: LIVE and submission stay disabled.
#   capture.sh [econ4h] --duration 10m|1h|5h|24h [typed options]   typed C0-E1 measurement; verification and report follow
#   capture.sh econ4h-launcher [--duration D | --measurement-config FILE]   econ4h capture only (former run_econ4h.sh)
#   capture.sh market                                                 C0-MO unfiltered read-only market observer
#   capture.sh shadow                                                 Mainnet Shadow, observe-only submission adapter
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
  MODE="$(stat -f '%Lp' "$ENV_FILE" 2>/dev/null || stat -c '%a' "$ENV_FILE" 2>/dev/null || true)"
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
KEYPAIR_MODE="$(stat -f '%Lp' "$KEYPAIR_REAL" 2>/dev/null || stat -c '%a' "$KEYPAIR_REAL" 2>/dev/null || true)"
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
load_env
absolute_paths
case "${P0_ECON_RUN_MODE:-auto}" in
  auto|full) ;;
  *) echo "P0_ECON_RUN_MODE quick is incompatible with a complete capture; unset it. Duration now starts after StreamReady." >&2; exit 2 ;;
esac

MEASUREMENT_CONFIG_INPUT=""
if [[ "$#" != 0 ]]; then
  if [[ "$#" == 2 && "$1" == --measurement-config ]]; then
    MEASUREMENT_CONFIG_INPUT="$2"
    if [[ "$MEASUREMENT_CONFIG_INPUT" != /* ]]; then
      MEASUREMENT_CONFIG_INPUT="$PWD/$MEASUREMENT_CONFIG_INPUT"
    fi
    export P0_MEASUREMENT_DURATION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["config"]["run"]["duration"])' "$MEASUREMENT_CONFIG_INPUT")"
    export P0_MARKET_SOURCE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["config"]["run"]["capture_source"])' "$MEASUREMENT_CONFIG_INPUT")"
  elif [[ "$#" == 2 && "$1" == --duration ]]; then
    export P0_MEASUREMENT_DURATION="$2"
  else
    echo 'Usage: capture.sh econ4h-launcher [--duration 10m|1h|5h|24h | --measurement-config FILE]' >&2; exit 2
  fi
fi

require_signer
export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
case "${P0_LIVE_ENABLED:-false}:${P0_SUBMISSION_ENABLED:-false}" in
  false:false|0:0|false:0|0:false) ;;
  *) echo "econ4h requires LIVE and submission disabled" >&2; exit 2 ;;
esac
# Allow provider WSS backlog to reach the unchanged finalized-root barrier.
# The 2026-10-10 smoke exceeded the old 30s wait; this never skips reconciliation.
export P0_RECOVERY_ROOT_CATCHUP_SECS="${P0_RECOVERY_ROOT_CATCHUP_SECS:-120}"
# Soft deadline 120s, real finalized-root stall 60s, absolute phase cap 300s.
# Startup wait includes snapshot A+B and may be longer than the 60s measurement.
export P0_RECOVERY_ROOT_STALL_SECS="${P0_RECOVERY_ROOT_STALL_SECS:-60}"
export P0_RECOVERY_ROOT_HARD_MAX_SECS="${P0_RECOVERY_ROOT_HARD_MAX_SECS:-300}"
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
RUN_DIR="$(py operations.capture.run_store allocate "$P0_ECON_OUT_DIR" "$P0_ECON_RUN_ID" "$P0_RUN_LAYOUT")" || exit 2
if [[ "$P0_RUN_LAYOUT" == v2 ]]; then
  # Pre-capture documents are staged and assembled into config.json before the capture starts.
  CONFIG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/p0-run-config.XXXXXX")"
  trap 'rm -rf "$CONFIG_DIR"' EXIT
  LOG_DIR="$RUN_DIR/logs"; CAPTURE_DIR="$RUN_DIR/capture"
else
  CONFIG_DIR="$RUN_DIR"; LOG_DIR="$RUN_DIR"; CAPTURE_DIR="$RUN_DIR"
fi
if [[ -n "$MEASUREMENT_CONFIG_INPUT" ]]; then
  py operations.cli.measurement_cli prepare "$RUN_DIR" "$MEASUREMENT_CONFIG_INPUT" "$CONFIG_DIR"
fi
cd "$REPO_ROOT"
echo "C0-E1: build; capture starts after bootstrap and StreamReady; submission disabled"
cargo build --release --locked --manifest-path "$PROJECT_DIR/native/Cargo.toml" \
  --features "crate_health txv1" --bin p0-longtail --bin p0-measurement-plan --bin p0-strict-verify --bin p0-econ-replay \
  --bin p0-journal --bin p0-capacity 2>&1 | tee "$LOG_DIR/build.log" || {
  echo "Build failed: $LOG_DIR/build.log" >&2; exit 2;
}
BIN="${CARGO_TARGET_DIR:-$PROJECT_DIR/native/target}/release/p0-longtail"
PLAN_BIN="$(dirname "$BIN")/p0-measurement-plan"
# Plan is compiled exactly once. Every consumer loads this persisted typed plan.
if [[ -n "${P0_MEASUREMENT_PLAN:-}" ]]; then
  echo "launcher creates its own plan; unset P0_MEASUREMENT_PLAN" >&2; exit 2
fi
# JFI-04: the runtime reads the typed plan journal; the JSON on stdout is the run store's staged part.
mkdir -p "$CAPTURE_DIR"
"$PLAN_BIN" --out "$CAPTURE_DIR/measurement_plan.p0j" > "$CONFIG_DIR/measurement_plan.json"
export P0_MEASUREMENT_PLAN="$CAPTURE_DIR/measurement_plan.p0j"
export P0_ECON_DURATION_SECS="$(python3 - "$CONFIG_DIR/measurement_plan.json" <<'PLAN'
import json, sys
print(json.load(open(sys.argv[1]))['duration_seconds'])
PLAN
)"
echo "Capture: ${P0_ECON_DURATION_SECS}s; separate bootstrap, drain and verification budgets in the measurement plan"
# SK-06 capacity preflight (native): measured lane rates of the newest sealed earlier runs, a write
# and sync probe on this volume, OS memory/CPU, operational budgets and the verifier budget of the
# selected chain-verify mode. A capture longer than 600 s starts only when QUALIFIED; the native
# report capture/capacity_preflight.p0j is the record, storage_preflight.json its configuration render.
CAPACITY_ARGS=(--duration "$P0_ECON_DURATION_SECS" --out "$CAPTURE_DIR" --capture-source "$P0_MARKET_SOURCE"
  --basis-root "$P0_ECON_OUT_DIR" --exclude-run "$RUN_DIR" --storage-preflight-render "$CONFIG_DIR/storage_preflight.json")
if [[ -n "$MEASUREMENT_CONFIG_INPUT" ]]; then
  read -r -a VERIFY_ARGS <<< "$(python3 -c 'import json,sys
v=json.load(open(sys.argv[1]))["config"]["verification"]
print("--chain-verify",v["mode"],"--verify-rps",v["rps"],"--verify-calls",v["max_calls"],"--verify-seconds",v["max_seconds"],
      "--verify-max-pages",v["max_pages"],"--verify-bytes",v["max_bytes"],"--verify-disk-bytes",v["max_disk_bytes"])' "$MEASUREMENT_CONFIG_INPUT")"
  CAPACITY_ARGS+=("${VERIFY_ARGS[@]}")
fi
"$(dirname "$BIN")/p0-capacity" "${CAPACITY_ARGS[@]}" | tee "$LOG_DIR/capacity_preflight.txt" || {
  echo "Capacity preflight: start not allowed (report: $LOG_DIR/capacity_preflight.txt). Nothing was deleted." >&2; exit 2;
}
# Binary journals (PERSIST-01); the daily summary is a frame of the opportunity journal.
export P0_RECORD_PATH="$CAPTURE_DIR/opportunities.p0j"
export P0_MT0_PATH="$CAPTURE_DIR/mt0.p0j"
export P0_MIN_PROFIT="${P0_MIN_PROFIT:-0}"
export P0_ZERO_PREFUND_QUALIFIED=false
export P0_ZERO_PREFUND_SHADOW_PROBE=true
export P0_JUPITER_MAX_ACCOUNTS="${P0_JUPITER_MAX_ACCOUNTS:-64}"
export P0_INGRESS_BUFFER="${P0_INGRESS_BUFFER:-65536}"
export P0_MAX_CONCURRENT="${P0_MAX_CONCURRENT:-1}"
export P0_MAX_OUTSTANDING="${P0_MAX_OUTSTANDING:-1}"
export P0_MAX_AGE_NS="${P0_MAX_AGE_NS:-2000000000}"
export P0_DEADLINE_NS="${P0_DEADLINE_NS:-2000000000}"

mkdir -p "$(dirname "$P0_RECORD_PATH")"

# Bind local (possibly uncommitted) source and the actual executable to this run.
# Do not record environment values, endpoints or signer paths.
python3 - "$BIN" "$CONFIG_DIR" <<'MANIFEST'
import hashlib, json, pathlib, subprocess, sys
binary, run = map(pathlib.Path, sys.argv[1:])
paths = subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '--',
    'project_0/native', 'project_0/operations', 'project_0/scripts'], text=True).splitlines()
source = {}
for name in sorted(set(paths)):
    p = pathlib.Path(name)
    if p.is_file() and p.suffix in ('.rs', '.toml', '.lock', '.sh', '.py'):
        source[name] = hashlib.sha256(p.read_bytes()).hexdigest()
manifest = {'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'source_sha256': source,
    'live_enabled': False, 'submission_enabled': False}
with (run / 'candidate_manifest.json').open('x') as f:
    json.dump(manifest, f, indent=2)
    f.write('\n')
MANIFEST
if [[ "$P0_RUN_LAYOUT" == v2 ]]; then
  py operations.capture.run_store config "$RUN_DIR" "$CONFIG_DIR" >/dev/null || exit 2
fi
status=0
# All durations follow the same capture lifecycle. Never timebox the entire
# process by capture duration: bootstrap and verification have separate budgets.
"$BIN" > "$LOG_DIR/console_summary.txt" 2> >(tee "$LOG_DIR/runtime.log" >&2) || status=$?
wait # stderr tee must finish before immutable evidence hashing
cat "$LOG_DIR/console_summary.txt"
if [[ -n "$MEASUREMENT_CONFIG_INPUT" ]]; then
  # Actual process has drained/exited. Freeze raw evidence before historical RPC.
  py operations.cli.measurement_cli finish "$RUN_DIR" "$MEASUREMENT_CONFIG_INPUT" || status=$?
fi
echo "Reports and detailed logs: $RUN_DIR"
exit "$status"
}

market() {
load_env
absolute_paths
# transactions = Helius transactionSubscribe (Developer+ plan), needs only the WebSocket.
# logs         = standard logsSubscribe (all plans), additionally needs RPC for post-capture enrichment.
export P0_MARKET_SOURCE="${P0_MARKET_SOURCE:-transactions}"
case "$P0_MARKET_SOURCE" in
  transactions|logs) ;;
  *) echo "invalid P0_MARKET_SOURCE=$P0_MARKET_SOURCE (transactions|logs)" >&2; exit 2 ;;
esac

# Endpoint selection lives in Rust (MarketConfig): an explicit P0_WS_URL / P0_RPC_URL always wins;
# otherwise HELIUS_API_KEY derives them (transactions -> Helius Enhanced WSS, logs -> standard WSS + RPC).
#   Helius Free:                         P0_MARKET_SOURCE=logs
#   Helius Developer+ (transactionSubscribe): P0_MARKET_SOURCE=transactions (preferred)
# Provider support for transactionSubscribe is never assumed; the smoke run must show the ACK.
if [[ -z "${P0_WS_URL:-}" && -z "${HELIUS_API_KEY:-}" ]]; then
  echo "missing endpoint: set P0_WS_URL or HELIUS_API_KEY" >&2
  exit 2
fi
if [[ "$P0_MARKET_SOURCE" == "logs" && -z "${P0_RPC_URL:-}" && -z "${HELIUS_API_KEY:-}" ]]; then
  echo "note: no P0_RPC_URL/HELIUS_API_KEY - liquidation enrichment unavailable (capture, summary and report still run)" >&2
fi

# Market mode never needs a signer: make sure none is passed through by accident.
unset P0_KEYPAIR_PATH P0_EXPECTED_PUBKEY

export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
export P0_MODE=market
export P0_MARKET_DURATION_SECS="${P0_MARKET_DURATION_SECS:-14400}"
export P0_MARKET_COMMITMENT="${P0_MARKET_COMMITMENT:-finalized}"
export P0_MARKET_RUN_ID="${P0_MARKET_RUN_ID:-market_$(date -u +%Y%m%dT%H%M%SZ)}"
# Run storage contract: data/market (layout V1) by default, data/runs with P0_RUN_STORE=runs.
# Standalone C0-MO has no pre-capture manifest, so it always writes layout V1.
P0_MARKET_OUT_DIR="$(py operations.capture.run_store root market "${P0_MARKET_OUT_DIR:-}")" || exit 2
export P0_MARKET_OUT_DIR
export P0_RUNTIME_LOCK="${P0_RUNTIME_LOCK:-$(dirname "$P0_MARKET_OUT_DIR")/.p0-runtime.lock}"
export P0_INGRESS_BUFFER="${P0_INGRESS_BUFFER:-65536}"

py operations.capture.run_store allocate "$P0_MARKET_OUT_DIR" "$P0_MARKET_RUN_ID" >/dev/null || exit 2

echo "Project_0 Unfiltered Live Market Observer"
echo "  program=$P0_PROGRAM_ID"
echo "  source=$P0_MARKET_SOURCE"
echo "  commitment=$P0_MARKET_COMMITMENT"
echo "  duration_seconds=$P0_MARKET_DURATION_SECS"
echo "  endpoints: see Rust banner (hosts redacted)"
echo "  output=$P0_MARKET_OUT_DIR/$P0_MARKET_RUN_ID"

cd "$REPO_ROOT"
exec cargo run --release \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1" \
  --bin p0-longtail
}

shadow() {
load_env
absolute_paths
require_signer
case "${P0_LIVE_ENABLED:-false}:${P0_SUBMISSION_ENABLED:-false}" in
  false:false|0:0|false:0|0:false) ;;
  *) echo "shadow requires LIVE and submission disabled" >&2; exit 2 ;;
esac
export P0_LIVE_ENABLED=false P0_SUBMISSION_ENABLED=false
export P0_PROGRAM_ID="${P0_PROGRAM_ID:-MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA}"
export P0_MODE=shadow
export P0_MIN_PROFIT="${P0_MIN_PROFIT:-0}"
export P0_ZERO_PREFUND_QUALIFIED=false
export P0_ZERO_PREFUND_SHADOW_PROBE=true
# One directory per Shadow run: journals are created exclusively and never appended across runs.
export P0_RECORD_PATH="${P0_RECORD_PATH:-$PROJECT_DIR/data/shadow/shadow_$(date -u +%Y%m%dT%H%M%SZ)_$$/opportunities.p0j}"
export P0_JUPITER_MAX_ACCOUNTS="${P0_JUPITER_MAX_ACCOUNTS:-64}"
export P0_INGRESS_BUFFER="${P0_INGRESS_BUFFER:-65536}"
export P0_MAX_CONCURRENT="${P0_MAX_CONCURRENT:-1}"
export P0_MAX_OUTSTANDING="${P0_MAX_OUTSTANDING:-1}"
export P0_MAX_AGE_NS="${P0_MAX_AGE_NS:-2000000000}"
export P0_DEADLINE_NS="${P0_DEADLINE_NS:-2000000000}"

mkdir -p "$(dirname "$P0_RECORD_PATH")"
export P0_RUNTIME_LOCK="${P0_RUNTIME_LOCK:-$PROJECT_DIR/data/.p0-runtime.lock}"

echo "Project_0 Mainnet Shadow"
echo "  program=$P0_PROGRAM_ID"
echo "  group=$P0_GROUP_ID"
echo "  rpc_host=$(endpoint_host P0_RPC_URL)"
echo "  ws_host=$(endpoint_host P0_WS_URL)"
echo "  mode=$P0_MODE"
echo "  zero_prefund_qualified=$P0_ZERO_PREFUND_QUALIFIED"
echo "  zero_prefund_shadow_probe=$P0_ZERO_PREFUND_SHADOW_PROBE"
echo "  record=$P0_RECORD_PATH"
echo "  journals=binary .p0j: record (opportunities, reconcile, daily summary) and <record>.mt0.p0j"

cd "$REPO_ROOT"
exec cargo run --release \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1" \
  --bin p0-longtail
}

case "${1:-}" in
  econ4h-launcher) shift; econ4h_launcher "$@" ;;
  market) shift; market "$@" ;;
  shadow) shift; shadow "$@" ;;
  econ4h) shift; py operations.cli.measurement_cli "$@" ;;
  *) py operations.cli.measurement_cli "$@" ;;
esac
