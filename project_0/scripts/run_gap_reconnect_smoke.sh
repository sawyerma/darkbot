#!/usr/bin/env bash
# Controlled C0-E1 reconnect, using a market-only loopback proxy. No firewall changes.
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
export P0_ECON_DURATION_SECS="${P0_ECON_DURATION_SECS:-600}"
export P0_ECON_SNAPSHOT_SECS="${P0_ECON_SNAPSHOT_SECS:-150}"
export P0_ECON_RUN_ID="${P0_ECON_RUN_ID:-c0e1_gap_smoke_$(date -u +%Y%m%dT%H%M%SZ)}"
export P0_ECON_OUT_DIR="${P0_ECON_OUT_DIR:-$ROOT/project_0/data/econ4h}"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/p0-gap-proxy.XXXXXX")"
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
for _ in {1..100}; do
  [[ -s "$P0_GAP_PROXY_READY_FILE" ]] && break
  kill -0 "$proxy_pid" 2>/dev/null || { echo "proxy failed; logs: $scratch"; exit 2; }
  sleep 0.1
done
[[ -s "$P0_GAP_PROXY_READY_FILE" ]] || { echo 'proxy readiness timeout'; exit 2; }
export P0_MARKET_WS_URL="$(< "$P0_GAP_PROXY_READY_FILE")"
# Environment was already sourced; preserve the proxy override in the inner launcher.
status=0
P0_ENV_FILE="$scratch/not-present.env" bash project_0/scripts/run_econ4h.sh || status=$?
run="$P0_ECON_OUT_DIR/$P0_ECON_RUN_ID"
if [[ -d "$run" ]]; then cp "$scratch/proxy.log" "$run/gap_proxy.log"; fi
echo "Controlled interruption evidence: $scratch/proxy.log"
[[ "$status" == 0 ]] || exit "$status"
python3 - "$run" <<'PY'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1])
f = json.loads((p / 'economics_final.json').read_text())
m = json.loads((p / 'market/summary.json').read_text())
events = [json.loads(line) for line in (p / 'market/coverage.jsonl').read_text().splitlines()]
names = [e['event'] for e in events]
assert 'GAP_PROXY_CONTROLLED_DISCONNECT' in (p / 'gap_proxy.log').read_text()
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
assert all((p / ('hour_%02d.json' % i)).is_file() for i in range(1, 5))
print('CONTROLLED_GAP_SMOKE_PASS')
PY
