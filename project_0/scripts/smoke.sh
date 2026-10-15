#!/usr/bin/env bash
# Project_0 smoke entry point (structure iteration 22): controlled C0-E1 reconnect through a market-only loopback
# proxy. No firewall changes. The former run_gap_reconnect_smoke.sh forwards here unchanged.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENV_FILE="${P0_ENV_FILE:-$ROOT/project_0/.env}"
if [[ -f "$ENV_FILE" ]]; then
  mode="$(stat -f '%Lp' "$ENV_FILE" 2>/dev/null || stat -c '%a' "$ENV_FILE")"
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
export P0_MARKET_SOURCE=logs
export P0_ECON_DURATION_SECS="${P0_ECON_DURATION_SECS:-60}"
export P0_ECON_DRAIN_SECS="${P0_ECON_DRAIN_SECS:-15}"
export P0_ECON_RUN_ID="${P0_ECON_RUN_ID:-c0e1_gap_smoke_$(date -u +%Y%m%dT%H%M%SZ)}"
P0_ECON_OUT_DIR="$(PYTHONPATH="$ROOT/project_0" python3 -m operations.capture.run_store root econ4h "${P0_ECON_OUT_DIR:-}")" || exit 2
export P0_ECON_OUT_DIR
# Reject an invalid or existing run id before the proxy connects upstream; capture.sh econ4h-launcher allocates it.
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
cd "$ROOT"
cargo build --locked --manifest-path project_0/native/Cargo.toml -p darkbot-solana --example p0-gap-ws-proxy > "$scratch/build.log" 2>&1
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
# Environment was already sourced; preserve the proxy override in the inner launcher.
status=0
P0_ENV_FILE="$scratch/not-present.env" bash project_0/scripts/capture.sh econ4h-launcher || status=$?
run="$P0_ECON_OUT_DIR/$P0_ECON_RUN_ID"
if [[ -d "$run" ]]; then
  # Layout V1 keeps the log at the run root, layout V2 under logs/ (run_store artifact gap_proxy_log).
  layout="$(PYTHONPATH="$ROOT/project_0" python3 -m operations.capture.run_store layout econ4h "$P0_ECON_OUT_DIR")" || exit 2
  cp "$scratch/proxy.log" "$(PYTHONPATH="$ROOT/project_0" python3 -m operations.capture.run_store path "$run" gap_proxy_log "$layout")"
fi
echo "Controlled interruption evidence: $scratch/proxy.log"
[[ "$status" == 0 ]] || exit "$status"
python3 - "$run" "$ROOT/project_0" <<'PY'
import json, pathlib, sys
sys.path.insert(0, sys.argv[2])
from operations.capture import run_store
p = pathlib.Path(sys.argv[1])
f = json.loads(run_store.artifact(p, 'economics_final').read_text())
m = json.loads(run_store.market_summary(p).read_text())
events = [json.loads(line) for line in (run_store.market_dir(p) / 'coverage.jsonl').read_text().splitlines()]
names = [e['event'] for e in events]
assert 'GAP_PROXY_CONTROLLED_DISCONNECT' in run_store.artifact(p, 'gap_proxy_log').read_text()
assert names.index('DRAINED') < names.index('SHADOW_RPC_QUIESCENT') < names.index('FULL_SLOT_RECONCILIATION_FINAL')
assert names.index('DRAINED') < names.index('GAP_RECOVERY_STARTED') < names.index('GAP_RECOVERED')
assert m['gap_count'] >= 1 and m['gap_recovered'] == m['gap_count'] and m['gap_unrecovered'] == 0
assert m['full_slot_reconciliation_complete'] is True
assert m['liquidation_path_coverage_complete'] is True
assert m['market_capture_complete'] is True
assert m['full_slot_reconciliation']['slots_unresolved'] == 0
assert names.index('FULL_SLOT_RECONCILIATION_FINAL') < names.index('GAP_RECOVERED')
assert f['canonical']['cumulative']['finalized'] is True
t = f['technical_qualification']
assert t['verdict'] == 'PASS' and t['submissions'] == 0 and t['analytics_queue_drops'] == 0
assert all(run_store.artifact(p, 'hour_%02d' % i).is_file() for i in range(1, 5))
print('CONTROLLED_GAP_SMOKE_PASS')
PY
