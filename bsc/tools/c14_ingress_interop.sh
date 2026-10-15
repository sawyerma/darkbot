#!/usr/bin/env bash
# C1.4 Go<->Rust ingress interop on one Wire-V2 arena.
#
# Four alternating processes share one mmap arena, as geth and the Rust
# runtime do in production:
#   1. geth: runtime start (geth-owner lease, O_EXCL arena), bind peers A and
#      B, publish through the C1.4 boundary (Transactions and
#      PooledTransactions), a validation-rejected packet, a non-matching
#      transaction, the Txpool lane with every accepted transaction, then a
#      peer-lane record later than its txpool acceptance; A disconnects with
#      an unread record
#   2. rust: check owner/state/epoch, both boundary timestamps, provenance and
#      canonical bytes; run the false-negative audit (two planted misses must
#      be found: one never on a peer lane, one on a peer lane only after the
#      txpool accepted it); drain and acknowledge A's lane
#   3. geth: release A's lane, bind it to C with epoch + 1, refuse a stale
#      handle of A, publish C's record
#   4. rust: only C's record follows, under the new epoch
#
# Each phase must print its marker. A skipped Go test or an unmatched Rust test
# filter exits 0 without running anything, so a missing marker is a failure.
#
# Prerequisite: the C1.4 overlay is applied to the pinned checkout
#   python3 bsc/tools/c1_apply_geth_overlay.py bsc/vendor/bsc-geth
# This script changes nothing outside its temporary directories.
set -euo pipefail

PIN="cdb7548b5baacfdae92f9f63437d6456411665f3"
MARKER="DARKBOT_C1_FASTLANE_C1_4"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NATIVE="$ROOT/bsc/native"
OVERLAY="$NATIVE/geth/overlay/darkbot/fastlane"
GETH="${DARKBOT_BSC_GETH_DIR:-$ROOT/bsc/vendor/bsc-geth}"

head="$(git -C "$GETH" rev-parse HEAD)"
if [ "$head" != "$PIN" ]; then
  echo "C14_INTEROP_FAIL reason=geth-head $head != $PIN" >&2
  exit 1
fi
if [ "$(cat "$GETH/.darkbot-c1-fastlane-c14" 2>/dev/null)" != "$MARKER" ]; then
  echo "C14_INTEROP_FAIL reason=c1.4-overlay-not-applied" >&2
  echo "run: python3 bsc/tools/c1_apply_geth_overlay.py $GETH" >&2
  exit 1
fi
for source in "$OVERLAY"/*.go; do
  file="$(basename "$source")"
  if ! cmp -s "$source" "$GETH/darkbot/fastlane/$file"; then
    echo "C14_INTEROP_FAIL reason=overlay-out-of-date file=$file" >&2
    echo "run: python3 bsc/tools/c1_apply_geth_overlay.py $GETH" >&2
    exit 1
  fi
done

DIR="$(mktemp -d "${TMPDIR:-/tmp}/darkbot-c14-interop.XXXXXX")"
LOGS="$(mktemp -d "${TMPDIR:-/tmp}/darkbot-c14-interop-logs.XXXXXX")"
trap 'rm -rf "$DIR" "$LOGS"' EXIT
export DARKBOT_C14_INTEROP_DIR="$DIR"

go_phase() {
  (cd "$GETH" && go test ./darkbot/fastlane -count=1 -v -run "^$1\$")
}

rust_phase() {
  (cd "$NATIVE" && cargo test -p darkbot-fastlane --locked --offline -- \
    --exact "ingress::tests::$1" --ignored --nocapture --test-threads=1)
}

run_phase() {
  local name="$1" marker="$2"
  shift 2
  local log="$LOGS/$name.log"
  if ! "$@" >"$log" 2>&1; then
    cat "$log" >&2
    echo "C14_INTEROP_FAIL phase=$name reason=exit-status" >&2
    exit 1
  fi
  if ! grep -q "$marker" "$log"; then
    cat "$log" >&2
    echo "C14_INTEROP_FAIL phase=$name reason=marker-missing" >&2
    exit 1
  fi
  echo "phase $name: OK"
}

run_phase geth-1 C14_INTEROP_GETH_PHASE1_OK go_phase TestC14InteropGethPhase1
run_phase rust-1 C14_INTEROP_RUST_PHASE1_OK rust_phase interop_c14_rust_phase_1
run_phase geth-2 C14_INTEROP_GETH_PHASE2_OK go_phase TestC14InteropGethPhase2
run_phase rust-2 C14_INTEROP_RUST_PHASE2_OK rust_phase interop_c14_rust_phase_2
echo "C1_4_GO_RUST_INGRESS_INTEROP_PASS"
