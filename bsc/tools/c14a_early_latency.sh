#!/usr/bin/env bash
set -euo pipefail

PIN="cdb7548b5baacfdae92f9f63437d6456411665f3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NATIVE="$ROOT/bsc/native"
SOURCE_GETH="$ROOT/bsc/vendor/bsc-geth"
EVIDENCE="$NATIVE/evidence/c1"
DATE="$(date +%Y%m%d)"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/darkbot-c14a.XXXXXX")"
GETH="$RUN_DIR/bsc-geth"
DATA="$RUN_DIR/run"
GO_BIN="$RUN_DIR/c14a-go.test"
RUST_BIN="$NATIVE/target/release/examples/c14a_consumer"
GO_LOG="$EVIDENCE/c1-4a-early-latency-$DATE-go.log"
RUST_LOG="$EVIDENCE/c1-4a-early-latency-$DATE-rust.log"
RAW="$EVIDENCE/c1-4a-early-latency-$DATE.csv"
JSON="$EVIDENCE/c1-4a-early-latency-$DATE.json"
SNAPSHOT="$EVIDENCE/c1-4a-early-latency-$DATE-go-snapshot.json"
LANES="$EVIDENCE/c1-4a-early-latency-$DATE-lane-stats.csv"

cleanup() {
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

mkdir -p "$EVIDENCE" "$DATA"

if [ "$(git -C "$SOURCE_GETH" rev-parse "$PIN")" != "$PIN" ]; then
  echo "C14A_FAIL reason=geth-pin-unavailable" >&2
  exit 1
fi

echo "=== C1.4a CREATE DISPOSABLE PINNED GETH CLONE ==="
git clone --shared --no-checkout "$SOURCE_GETH" "$GETH" >/dev/null 2>&1
git -C "$GETH" checkout --detach "$PIN" >/dev/null 2>&1
python3 "$ROOT/bsc/tools/c1_apply_geth_overlay.py" "$GETH"

python3 - "$GETH/darkbot/fastlane/v2.go" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1])
s = p.read_text()
old = '''\tatomic.StoreUint64(v2AtomicU64(slot, v2SlotCommitted), write+1)\n\tatomic.StoreUint64(l.atomic(v2LHWriteSeq), write+1)\n\treturn nil\n'''
new = '''\tatomic.StoreUint64(v2AtomicU64(slot, v2SlotCommitted), write+1)\n\tc14aProbePreCommit(slot)\n\tatomic.StoreUint64(l.atomic(v2LHWriteSeq), write+1)\n\tc14aProbePostCommit(slot)\n\treturn nil\n'''
if s.count(old) != 1:
    raise SystemExit("C14A_FAIL reason=publish-anchor")
p.write_text(s.replace(old, new, 1))
PY

cat > "$GETH/darkbot/fastlane/c14a_probe.go" <<'GO'
package fastlane

import "sync/atomic"

const (
	c14aProbePreCommitOffset  = 160
	c14aProbePostCommitOffset = 168
	c14aProbeMagicOffset      = 176
	c14aProbeMagic            = uint64(0x4331344150524f42)
)

func c14aProbePreCommit(slot []byte) {
	ns, err := MonotonicNS()
	if err != nil {
		return
	}
	atomic.StoreUint64(v2AtomicU64(slot, c14aProbePreCommitOffset), ns)
}

func c14aProbePostCommit(slot []byte) {
	ns, err := MonotonicNS()
	if err != nil {
		return
	}
	atomic.StoreUint64(v2AtomicU64(slot, c14aProbePostCommitOffset), ns)
	atomic.StoreUint64(v2AtomicU64(slot, c14aProbeMagicOffset), c14aProbeMagic)
}
GO

cat > "$GETH/darkbot/fastlane/c14a_latency_test.go" <<'GO'
package fastlane

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/core/types"
)

const (
	c14aGeneration        = uint64(1410)
	c14aPeers             = 4
	c14aWarmupPerPeer     = 32
	c14aSamplesPerPeer    = 250
	c14aTotalPerPeer      = c14aWarmupPerPeer + c14aSamplesPerPeer
	c14aPeerCapacity      = uint32(1024)
	c14aPeerSlotStride = uint32(4096)
	c14aSyncTimeout    = 30 * time.Second
	c14aSyncPoll       = time.Millisecond
)

func c14aRunDir(t *testing.T) string {
	t.Helper()
	dir := os.Getenv("DARKBOT_C14A_DIR")
	if dir == "" {
		t.Skip("C1.4a latency harness; run by bsc/tools/c14a_early_latency.sh")
	}
	return dir
}

func c14aWaitForFile(t *testing.T, path string) {
	t.Helper()
	deadline := time.Now().Add(c14aSyncTimeout)
	for time.Now().Before(deadline) {
		if _, err := os.Stat(path); err == nil {
			return
		}
		time.Sleep(c14aSyncPoll)
	}
	t.Fatalf("timed out waiting for %s", path)
}

func c14aWriteFile(t *testing.T, path string, data []byte) {
	t.Helper()
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
}

func TestC14AEarlyLatencyProducer(t *testing.T) {
	dir := c14aRunDir(t)
	cfg := defaultIngressConfigV2(dir, c14aGeneration, c14aPeers, 0)
	cfg.PeerLaneCapacity = c14aPeerCapacity
	cfg.PeerLaneSlotStride = c14aPeerSlotStride
	cfg.TxpoolLaneCapacity = 64
	cfg.TxpoolLaneSlotStride = 4096

	r := c14Start(t, cfg)
	handles := make([]*PeerIngressV2, c14aPeers)
	txs := make([][]*types.Transaction, c14aPeers)
	laneIDs := make([]string, c14aPeers)

	for peer := 0; peer < c14aPeers; peer++ {
		handles[peer] = c14Bind(t, r, c14Node(byte(0x20+peer)))
		laneIDs[peer] = strconv.FormatUint(uint64(handles[peer].lease.LaneID), 10)
		txs[peer] = make([]*types.Transaction, c14aTotalPerPeer)
		for sample := 0; sample < c14aTotalPerPeer; sample++ {
			nonce := uint64(peer*c14aTotalPerPeer + sample)
			txs[peer][sample] = c14Tx(
				t,
				nonce,
				&c14Portal,
				c14SmallLaunch(byte(sample)),
				(peer+sample)%2 == 0,
			)
		}
	}

	manifest := strings.Join([]string{
		"generation=" + strconv.FormatUint(cfg.Generation, 10),
		"peers=" + strconv.Itoa(c14aPeers),
		"warmup_per_peer=" + strconv.Itoa(c14aWarmupPerPeer),
		"samples_per_peer=" + strconv.Itoa(c14aSamplesPerPeer),
		"peer_lane_capacity=" + strconv.FormatUint(uint64(cfg.PeerLaneCapacity), 10),
		"peer_lane_slot_stride=" + strconv.FormatUint(uint64(cfg.PeerLaneSlotStride), 10),
		"lane_ids=" + strings.Join(laneIDs, ","),
		"clock_id=" + strconv.FormatUint(uint64(v2ClockMonotonic), 10),
		"load_model=parallel_peers_one_outstanding_per_lane_with_explicit_warmup",
	}, "\n") + "\n"
	c14aWriteFile(t, filepath.Join(dir, "manifest.txt"), []byte(manifest))
	c14aWaitForFile(t, filepath.Join(dir, "rust.ready"))

	start := make(chan struct{})
	errs := make(chan error, c14aPeers)
	var wg sync.WaitGroup
	for peer := 0; peer < c14aPeers; peer++ {
		wg.Add(1)
		go func(peer int) {
			defer wg.Done()
			<-start
			source := IngressSourceEthBroadcast
			if peer%2 != 0 {
				source = IngressSourceEthPooled
			}
			for sample := 0; sample < c14aTotalPerPeer; sample++ {
				if err := IngressV2(
					handles[peer],
					source,
					txs[peer][sample:sample+1],
					c14Accept,
				); err != nil {
					errs <- fmt.Errorf("peer %d sample %d: %w", peer, sample, err)
					return
				}

				// C1.4a measures latency feasibility, not the C1.8 burst-stress gate.
				// Keep at most one outstanding record on each independent peer lane.
				expectedRead := uint64(sample + 1)
				drainDeadline := time.Now().Add(c14aSyncTimeout)
				for handles[peer].lane.stats().ReadSequence < expectedRead {
					if time.Now().After(drainDeadline) {
						errs <- fmt.Errorf(
							"peer %d sample %d: consumer did not drain sequence %d",
							peer, sample, expectedRead,
						)
						return
					}
					time.Sleep(10 * time.Microsecond)
				}
			}
		}(peer)
	}

	close(start)
	wg.Wait()
	close(errs)
	for err := range errs {
		t.Fatal(err)
	}

	snapshot := r.snapshot()
	expected := uint64(c14aPeers * c14aTotalPerPeer)
	if snapshot.LiveCovered != c14aPeers || snapshot.LiveUncovered != 0 ||
		snapshot.GapTotal != 0 || snapshot.Peers.Published != expected ||
		snapshot.Peers.QueueFull != 0 || snapshot.Peers.Oversize != 0 ||
		snapshot.Peers.Refused != 0 || snapshot.Peers.SerializeErrors != 0 ||
		snapshot.Peers.ClockErrors != 0 || snapshot.Peers.UncoveredCandidates != 0 {
		t.Fatalf("C1.4a producer evidence %+v", snapshot)
	}

	encoded, err := json.MarshalIndent(snapshot, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	encoded = append(encoded, '\n')
	c14aWriteFile(t, filepath.Join(dir, "go_snapshot.json"), encoded)
	c14aWriteFile(t, filepath.Join(dir, "go.done"), []byte("ok\n"))
	c14aWaitForFile(t, filepath.Join(dir, "rust.done"))
	fmt.Println("C14A_GETH_PRODUCER_OK")
}
GO

gofmt -w \
  "$GETH/darkbot/fastlane/v2.go" \
  "$GETH/darkbot/fastlane/c14a_probe.go" \
  "$GETH/darkbot/fastlane/c14a_latency_test.go"

export DARKBOT_C14A_DIR="$DATA"

echo "=== C1.4a BUILD GO MEASUREMENT BINARY ==="
(cd "$GETH" && go test -c -o "$GO_BIN" ./darkbot/fastlane)

echo "=== C1.4a BUILD RUST RELEASE CONSUMER ==="
(cd "$NATIVE" && cargo build -p darkbot-fastlane --release --example c14a_consumer --locked --offline)

echo "=== C1.4a START GO PRODUCERS ==="
"$GO_BIN" -test.run '^TestC14AEarlyLatencyProducer$' -test.count=1 -test.v >"$GO_LOG" 2>&1 &
GO_PID=$!

READY=0
for _ in $(seq 1 3000); do
  if [ -f "$DATA/manifest.txt" ]; then
    READY=1
    break
  fi
  if ! kill -0 "$GO_PID" 2>/dev/null; then
    cat "$GO_LOG" >&2
    echo "C14A_FAIL reason=go-producer-exited-before-manifest" >&2
    exit 1
  fi
  sleep 0.01
done
if [ "$READY" -ne 1 ]; then
  kill "$GO_PID" 2>/dev/null || true
  cat "$GO_LOG" >&2
  echo "C14A_FAIL reason=manifest-timeout" >&2
  exit 1
fi

echo "=== C1.4a RUN RUST CONSUMER ==="
if ! "$RUST_BIN" >"$RUST_LOG" 2>&1; then
  kill "$GO_PID" 2>/dev/null || true
  cat "$RUST_LOG" >&2
  cat "$GO_LOG" >&2
  echo "C14A_FAIL reason=rust-consumer" >&2
  exit 1
fi

if ! wait "$GO_PID"; then
  cat "$GO_LOG" >&2
  echo "C14A_FAIL reason=go-producer" >&2
  exit 1
fi

if ! grep -q 'C14A_GETH_PRODUCER_OK' "$GO_LOG"; then
  cat "$GO_LOG" >&2
  echo "C14A_FAIL reason=go-marker-missing" >&2
  exit 1
fi
if ! grep -q 'C14A_RUST_CONSUMER_OK' "$RUST_LOG"; then
  cat "$RUST_LOG" >&2
  echo "C14A_FAIL reason=rust-marker-missing" >&2
  exit 1
fi

cp "$DATA/raw.csv" "$RAW"
cp "$DATA/go_snapshot.json" "$SNAPSHOT"
cp "$DATA/lane_stats.csv" "$LANES"

python3 "$ROOT/bsc/tools/c14a_analyze.py" \
  --run-dir "$DATA" \
  --repo "$ROOT" \
  --geth "$GETH" \
  --go-bin "$GO_BIN" \
  --rust-bin "$RUST_BIN" \
  --raw-evidence "$RAW" \
  --go-log "$GO_LOG" \
  --rust-log "$RUST_LOG" \
  --output "$JSON"

CLASSIFICATION="$(python3 - "$JSON" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1]))["classification"])
PY
)"
MAX_NS="$(python3 - "$JSON" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1]))["metrics"]["T_ingress_to_L2"]["max_ns"])
PY
)"

echo "=== C1.4a RESULT ==="
echo "classification=$CLASSIFICATION"
echo "max_T_ingress_to_L2_ns=$MAX_NS"
echo "evidence=$JSON"
echo "raw=$RAW"

case "$CLASSIFICATION" in
  GREEN_STRONG_HEADROOM|GREEN_FEASIBLE)
    echo "C1_4A_GATE_PASS"
    ;;
  AMBER_LOW_HEADROOM)
    echo "C1_4A_GATE_AMBER"
    exit 2
    ;;
  *)
    echo "C1_4A_GATE_FAIL"
    exit 1
    ;;
esac
