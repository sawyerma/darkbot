#!/usr/bin/env bash
# Project_0 smoke entry point: controlled C0-E1 reconnect through a market-only loopback proxy. No firewall changes.
# The run goes through the one controller (`capture.sh econ4h` -> `p0ctl run start`): boot audit, the operator's
# `--network-audit FILE --mainnet-approved TOKEN` (passed through), owner lock. Its acceptance reads only native records
# (F-10, P0-PROD-CTRL-02): the C0-E1 master qualification the launcher prints after the capture; no JSON, no export.
#   smoke.sh --network-audit FILE --mainnet-approved TOKEN [more p0ctl run start options]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# Native acceptance of a finished smoke run: the master qualification over its .p0j records (written by the
# launcher's post-capture chain). With --chain-verify off only the canonical verification and the economics replay
# may fail; every capture, deployment, technical and trading gate must hold. `P0_SMOKE_CHECK_RUN=DIR` re-checks a run.
accept() {
  local logs="$1"
  [[ -d "$1/logs" ]] && logs="$1/logs"
  local report="$logs/post_capture_report.txt" failed code axis frames gaps
  fail() { echo "CONTROLLED_GAP_SMOKE_FAIL: $*" >&2; exit 1; }
  grep -q 'GAP_PROXY_CONTROLLED_DISCONNECT' "$logs/gap_proxy.log" 2>/dev/null || fail 'no controlled disconnect in the proxy log'
  [[ -f "$report" ]] || fail "no post-capture report: $report"
  for axis in capture_clock=CAPTURE_CLOCK_COMPLETE live_coverage=LIVE_COVERAGE_COMPLETE \
              capture_journal=CAPTURE_JOURNAL_COMPLETE deployment_continuity=DEPLOYMENT_CONTINUITY_PROVEN \
              trading=LIVE_DISABLED_NO_SUBMISSION; do
    grep -qx "$axis" "$report" || fail "qualification axis not met: $axis"
  done
  # The controlled gap itself (review 5), from the typed market summary as `p0-journal frames` text: at
  # least one gap, every gap recovered, no unresolved reconciliation slot.
  local summary="$1/capture/market/market_summary.p0j"
  [[ -f "$summary" ]] || fail "no typed market summary: $summary"
  frames="$("${P0_JOURNAL_TOOL:-${CARGO_TARGET_DIR:-$ROOT/project_0/native/target}/release/p0-journal}" frames "$summary")" \
    || fail 'the market summary journal does not verify'
  gaps="$(grep -o 'gap_count: [0-9]*' <<< "$frames" | head -1 | awk '{print $2}')"
  [[ "${gaps:-0}" -ge 1 ]] || fail 'no gap recorded: the controlled disconnect produced no gap'
  grep -q 'gap_unrecovered: 0,' <<< "$frames" || fail 'unrecovered gaps'
  grep -q 'slots_unresolved: 0,' <<< "$frames" || fail 'unresolved reconciliation slots'
  failed="$(sed -n 's/^failed=//p' "$report")"
  [[ -n "$failed" ]] || fail 'no failed= line in the qualification'
  if [[ "$failed" != - ]]; then
    IFS=, read -r -a codes <<< "$failed"
    for code in "${codes[@]}"; do
      case "$code" in
        VERIFICATION_NOT_CANONICAL|ECONOMICS_NOT_REPLAYED) ;;
        *) fail "master gate failed: $code" ;;
      esac
    done
  fi
  echo 'CONTROLLED_GAP_SMOKE_PASS'
}
if [[ -n "${P0_SMOKE_CHECK_RUN:-}" ]]; then
  accept "$P0_SMOKE_CHECK_RUN"
  exit 0
fi
ENV_FILE="${P0_ENV_FILE:-$ROOT/project_0/.env}"
if [[ -f "$ENV_FILE" ]]; then
  mode="$(stat -c '%a' "$ENV_FILE" 2>/dev/null || stat -f '%Lp' "$ENV_FILE")"
  [[ "$mode" == 600 ]] || { echo 'env must be owner-only 600' >&2; exit 2; }
  set -a
  source "$ENV_FILE"
  set +a
fi
export P0_GAP_UPSTREAM_WS_URL="${P0_MARKET_WS_URL:-${P0_WS_URL:-}}"
if [[ -z "$P0_GAP_UPSTREAM_WS_URL" && -n "${HELIUS_API_KEY:-}" ]]; then
  export P0_GAP_UPSTREAM_WS_URL="wss://mainnet.helius-rpc.com/?api-key=$HELIUS_API_KEY"
fi
[[ -n "$P0_GAP_UPSTREAM_WS_URL" ]] || { echo 'missing market upstream endpoint' >&2; exit 2; }
case "${P0_LIVE_ENABLED:-false}:${P0_SUBMISSION_ENABLED:-false}" in
  false:false|0:0|false:0|0:false) ;;
  *) echo 'LIVE and submission must be disabled' >&2; exit 2 ;;
esac
export P0_LIVE_ENABLED=false P0_SUBMISSION_ENABLED=false
SMOKE_DURATION="${P0_SMOKE_DURATION:-61s}"
export P0_ECON_RUN_ID="${P0_ECON_RUN_ID:-c0e1_gap_smoke_$(date -u +%Y%m%dT%H%M%SZ)}"
# The launcher's own run-store contract: an invalid store or an existing run id is refused before the proxy connects.
P0_ECON_OUT_DIR="$(PYTHONPATH="$ROOT/project_0" python3 -m operations.capture.run_store root econ4h "${P0_ECON_OUT_DIR:-}")" || exit 2
export P0_ECON_OUT_DIR
PYTHONPATH="$ROOT/project_0" python3 -m operations.capture.run_store check "$P0_ECON_OUT_DIR" "$P0_ECON_RUN_ID" >/dev/null || exit 2
# Proxy build log, readiness file and probe output are reconstructible working data (data/cache policy);
# the proxy log that belongs to a run is copied into the run below.
mkdir -p "$ROOT/project_0/data/cache/gap_proxy"
scratch="$(mktemp -d "$ROOT/project_0/data/cache/gap_proxy/$P0_ECON_RUN_ID.XXXXXX")"
export P0_GAP_PROXY_READY_FILE="$scratch/ready"
proxy_pid=''
cleanup() {
  if [[ -n "$proxy_pid" ]]; then kill "$proxy_pid" 2>/dev/null || true; wait "$proxy_pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
cargo build --locked --manifest-path "$ROOT/project_0/native/Cargo.toml" -p darkbot-solana --example p0-gap-ws-proxy > "$scratch/build.log" 2>&1
"${CARGO_TARGET_DIR:-$ROOT/project_0/native/target}/debug/examples/p0-gap-ws-proxy" > "$scratch/proxy.stdout" 2> "$scratch/proxy.log" &
proxy_pid=$!
for _ in {1..300}; do
  [[ -s "$P0_GAP_PROXY_READY_FILE" ]] && break
  kill -0 "$proxy_pid" 2>/dev/null || { echo "proxy failed; logs: $scratch"; exit 2; }
  sleep 0.1
done
[[ -s "$P0_GAP_PROXY_READY_FILE" ]] || { echo 'proxy readiness timeout'; exit 2; }
export P0_MARKET_WS_URL="$(< "$P0_GAP_PROXY_READY_FILE")"
if [[ "${P0_GAP_PROBE_ONLY:-false}" == true ]]; then
  P0_GAP_UPSTREAM_WS_URL="$P0_MARKET_WS_URL" \
    "${CARGO_TARGET_DIR:-$ROOT/project_0/native/target}/debug/examples/p0-gap-ws-proxy" --probe-proxy \
    > "$scratch/probe.stdout" 2> "$scratch/probe.log"
  cat "$scratch/probe.log"
  echo "Short proxy qualification evidence: $scratch"
  exit 0
fi
# Environment was already sourced; preserve the proxy override in the inner launcher. The controller validates the
# arguments, refuses a run without the operator's approval and allocates the run (an existing id is refused).
status=0
P0_ENV_FILE="$scratch/not-present.env" bash "$ROOT/project_0/scripts/capture.sh" econ4h \
  --duration "$SMOKE_DURATION" --capture-source logs --chain-verify off "$@" || status=$?
run="$P0_ECON_OUT_DIR/$P0_ECON_RUN_ID"
if [[ ! -d "$run" ]]; then
  echo "CONTROLLED_GAP_SMOKE_NOT_RUN: no run $P0_ECON_RUN_ID (launcher status $status); proxy logs: $scratch" >&2
  exit 2
fi
logs="$run"; [[ -d "$run/logs" ]] && logs="$run/logs"
cp "$scratch/proxy.log" "$logs/gap_proxy.log"
echo "Controlled interruption evidence: $logs/gap_proxy.log (launcher status $status; with --chain-verify off the post-capture chain ends NOT_QUALIFIED by design)"
accept "$run"
