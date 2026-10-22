#!/usr/bin/env bash
# C0E1-P6-GATE: the binding offline gate of the whole C0-E1 chain (capture, verification off|strict,
# report, master qualification, persistence and resume invariants) in every supported feature set.
# Offline only: no Mainnet, no network audit, no capture, no LIVE. Every step runs; the exit code is
# non-zero if any step failed. Logs: $1 or native/target/p6-gate/<UTC time>.
set -uo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT="$(cd "$DIR/.." && pwd)"
NATIVE="$PROJECT/native"
LOGS="${1:-$NATIVE/target/p6-gate/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$LOGS"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
failed=0

step() { # name, command...
  local name="$1"; shift
  (cd "$NATIVE" && "$@") > "$LOGS/$name.log" 2>&1
  local code=$?
  local tests
  tests="$(grep '^test result' "$LOGS/$name.log" | awk '{p+=$4; f+=$6; i+=$8} END{if (NR) print p"/"f"/"i}')"
  echo "$name exit=$code${tests:+ tests=$tests}"
  [[ "$code" -eq 0 ]] || failed=1
}

step fmt cargo fmt --all --check
step clippy_all cargo clippy --locked --all-targets --all-features -- -D warnings
step clippy_default cargo clippy --locked --workspace --all-targets -- -D warnings
step clippy_runner_default cargo clippy --locked -p darkbot-runner --all-targets -- -D warnings
step clippy_runner_txv1 cargo clippy --locked -p darkbot-runner --all-targets --features txv1 -- -D warnings
step test_all cargo test --locked --all-features --no-fail-fast -- --test-threads=2
step test_default cargo test --locked --workspace --no-fail-fast -- --test-threads=2
step test_runner_default cargo test --locked -p darkbot-runner --no-fail-fast -- --test-threads=2
step test_runner_txv1 cargo test --locked -p darkbot-runner --features txv1 --no-fail-fast -- --test-threads=2
step release cargo build --locked --release --all-features
for s in "$DIR"/*.sh; do
  bash -n "$s" > "$LOGS/bash_n.log" 2>&1 || { echo "bash_n exit=1 $s"; failed=1; }
done
(cd "$PROJECT/.." && python3 -m unittest discover -s project_0/tests -t project_0) > "$LOGS/python.log" 2>&1
code=$?
echo "python exit=$code $(tail -1 "$LOGS/python.log")"
[[ "$code" -eq 0 ]] || failed=1
echo "P6_GATE_OFFLINE=$([[ "$failed" -eq 0 ]] && echo PASS || echo FAIL) logs=$LOGS"
exit "$failed"
