#!/usr/bin/env bash
# C1.3 Go<->Rust lane-lifecycle interop on one Wire-V2 arena.
#
# Four alternating processes share one mmap arena, as geth and the Rust
# runtime do in production:
#   1. geth: create, bind lane 1, publish 3, close; bind lane 2, break a CRC
#   2. rust: consume lane 1 to final_write, ack drain; quarantine lane 2
#   3. geth: release lane 1, bind again (epoch + 1), publish; see quarantine
#   4. rust: consume the new binding's record; see quarantine
#
# Each phase must print its marker. A skipped Go test or an unmatched Rust test
# filter exits 0 without running anything, so a missing marker is a failure.
#
# Prerequisite: the overlay is applied to the pinned checkout
#   python3 bsc/tools/c1_apply_geth_overlay.py bsc/vendor/bsc-geth
# This script changes nothing outside its temporary directory.
set -euo pipefail

PIN="cdb7548b5baacfdae92f9f63437d6456411665f3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE="$ROOT/bsc/engine"
OVERLAY="$ROOT/bsc/geth/overlay/darkbot/fastlane"
GETH="${DARKBOT_BSC_GETH_DIR:-$ROOT/bsc/vendor/bsc-geth}"

head="$(git -C "$GETH" rev-parse HEAD)"
if [ "$head" != "$PIN" ]; then
  echo "C13_INTEROP_FAIL reason=geth-head $head != $PIN" >&2
  exit 1
fi
for file in v2.go v2_test.go; do
  if ! cmp -s "$OVERLAY/$file" "$GETH/darkbot/fastlane/$file"; then
    echo "C13_INTEROP_FAIL reason=overlay-not-applied file=$file" >&2
    echo "run: python3 bsc/tools/c1_apply_geth_overlay.py $GETH" >&2
    exit 1
  fi
done

DIR="$(mktemp -d "${TMPDIR:-/tmp}/darkbot-c13-interop.XXXXXX")"
LOGS="$(mktemp -d "${TMPDIR:-/tmp}/darkbot-c13-interop-logs.XXXXXX")"
trap 'rm -rf "$DIR" "$LOGS"' EXIT
export DARKBOT_C13_INTEROP_DIR="$DIR"

go_phase() {
  (cd "$GETH" && go test ./darkbot/fastlane -count=1 -v -run "^$1\$")
}

rust_phase() {
  (cd "$ENGINE" && cargo test -p darkbot-bsc-geth-ipc --locked --offline -- \
    --exact "v2::tests::$1" --ignored --nocapture --test-threads=1)
}

run_phase() {
  local name="$1" marker="$2"
  shift 2
  local log="$LOGS/$name.log"
  if ! "$@" >"$log" 2>&1; then
    cat "$log" >&2
    echo "C13_INTEROP_FAIL phase=$name reason=exit-status" >&2
    exit 1
  fi
  if ! grep -q "$marker" "$log"; then
    cat "$log" >&2
    echo "C13_INTEROP_FAIL phase=$name reason=marker-missing" >&2
    exit 1
  fi
  echo "phase $name: OK"
}

run_phase geth-1 C13_INTEROP_GETH_PHASE1_OK go_phase TestV2InteropGethPhase1
run_phase rust-1 C13_INTEROP_RUST_PHASE1_OK rust_phase interop_rust_phase_1
run_phase geth-2 C13_INTEROP_GETH_PHASE2_OK go_phase TestV2InteropGethPhase2
run_phase rust-2 C13_INTEROP_RUST_PHASE2_OK rust_phase interop_rust_phase_2
echo "C1_3_GO_RUST_LIFECYCLE_INTEROP_PASS"
