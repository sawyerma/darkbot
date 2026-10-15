#!/usr/bin/env bash
# C1.9 native latency qualification harness.
#
# Measures the integrated C1.5-C1.8 candidate on this host:
#
#   T_packet_decoded -> T_ingress -> L1 (SPSC Release commit) -> L2 (Rust
#   Acquire in the HOT0 poller), and Rust egress commit -> first and every
#   selected-peer handoff of the C1.7 outbound workers.
#
# Like C1.4a it builds a disposable clone of the pinned geth, applies the
# overlay, and adds the L1 probes to that clone only; the repository and the
# vendored checkout are not modified. The window is fixed before the run:
# DARKBOT_C19_WARMUP_PER_PEER records per peer are warmup, kept in the raw
# evidence and excluded from the statistics; the following
# DARKBOT_C19_SAMPLES_PER_PEER are the qualification window.
#
# Default: real selected devp2p peers over authenticated RLPx on localhost,
# through the production Peer.SendRawTransaction/p2p.Send/transport path.
# Optional loopback_microbenchmark measures only workers and a TCP write.
# C1.9b is a separate controlled-launch ingress/hook-path qualification.
#
# This script produces measurements. It does not decide the C1.9 gate: the
# evidence is reviewed against the plan. Exit 0 means the run and its
# integrity checks completed.
set -euo pipefail

PIN="cdb7548b5baacfdae92f9f63437d6456411665f3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE="$ROOT/bsc/engine"
SOURCE_GETH="$ROOT/bsc/vendor/bsc-geth"
EVIDENCE="$ROOT/bsc/native/evidence/c1"
STAMP="$(date +%Y%m%dT%H%M%S)"
RUN_DIR="$(mktemp -d "${TMPDIR:-/tmp}/darkbot-c19.XXXXXX")"
GETH="$RUN_DIR/bsc-geth"
DATA="$RUN_DIR/run"
GO_BIN="$RUN_DIR/c19-go.test"
RUST_BIN="$ENGINE/target/release/examples/c19_consumer"
HOST_BIN="$ENGINE/target/release/examples/c16_host_profile"
PREFIX="$EVIDENCE/c1-9-native-latency-$STAMP"

export DARKBOT_C19_PEERS="${DARKBOT_C19_PEERS:-4}"
export DARKBOT_C19_WARMUP_PER_PEER="${DARKBOT_C19_WARMUP_PER_PEER:-32}"
export DARKBOT_C19_SAMPLES_PER_PEER="${DARKBOT_C19_SAMPLES_PER_PEER:-2500}"
export DARKBOT_C19_OUTBOUND_WORKERS="${DARKBOT_C19_OUTBOUND_WORKERS:-2}"
PROFILE="${DARKBOT_C19_PROFILE:-S}"
export DARKBOT_C19_OUTBOUND_MODE="${DARKBOT_C19_OUTBOUND_MODE:-production_devp2p_selected_peer_handoff}"
case "$DARKBOT_C19_OUTBOUND_MODE" in
  production_devp2p_selected_peer_handoff|loopback_microbenchmark) ;;
  *) echo "C19_FAIL unknown outbound mode" >&2; exit 2 ;;
esac
export DARKBOT_C19_PURPOSE="${DARKBOT_C19_PURPOSE:-qualification}"
case "$DARKBOT_C19_PURPOSE" in
  qualification|regression_smoke) ;;
  *) echo "C19_FAIL unknown measurement purpose" >&2; exit 2 ;;
esac
GO_PACKAGE=./darkbot/fastlane

cleanup() {
  rm -rf "$RUN_DIR"
}
trap cleanup EXIT

mkdir -p "$EVIDENCE" "$DATA"

if [ "$(git -C "$SOURCE_GETH" rev-parse "$PIN")" != "$PIN" ]; then
  echo "C19_FAIL reason=geth-pin-unavailable" >&2
  exit 1
fi

echo "=== C1.9 CREATE DISPOSABLE PINNED GETH CLONE ==="
git clone --shared --no-checkout "$SOURCE_GETH" "$GETH" >/dev/null 2>&1
git -C "$GETH" checkout --detach "$PIN" >/dev/null 2>&1
python3 "$ROOT/bsc/tools/c1_apply_geth_overlay.py" "$GETH"

python3 - "$GETH/darkbot/fastlane/v2.go" <<'PY'
import sys
from pathlib import Path
p = Path(sys.argv[1])
s = p.read_text()
old = '''\tatomic.StoreUint64(v2AtomicU64(slot, v2SlotCommitted), write+1)\n\tatomic.StoreUint64(l.atomic(v2LHWriteSeq), write+1)\n\treturn nil\n'''
new = '''\tatomic.StoreUint64(v2AtomicU64(slot, v2SlotCommitted), write+1)\n\tc19ProbePreCommit(slot)\n\tatomic.StoreUint64(l.atomic(v2LHWriteSeq), write+1)\n\tc19ProbePostCommit(slot)\n\treturn nil\n'''
if s.count(old) != 1:
    raise SystemExit("C19_FAIL reason=publish-anchor")
p.write_text(s.replace(old, new, 1))
PY

cat > "$GETH/darkbot/fastlane/c19_probe.go" <<'GO'
package fastlane

import "sync/atomic"

// Measurement-only probes around the SPSC Release commit, in reserved bytes
// of the slot header outside the record's CRC domain. They exist only in the
// disposable measurement clone.
const (
	c19ProbePreCommitOffset  = 160
	c19ProbePostCommitOffset = 168
	c19ProbeMagicOffset      = 176
	c19ProbeMagic            = uint64(0x43313950524f4245)
)

func c19ProbePreCommit(slot []byte) {
	ns, err := MonotonicNS()
	if err != nil {
		return
	}
	atomic.StoreUint64(v2AtomicU64(slot, c19ProbePreCommitOffset), ns)
}

func c19ProbePostCommit(slot []byte) {
	ns, err := MonotonicNS()
	if err != nil {
		return
	}
	atomic.StoreUint64(v2AtomicU64(slot, c19ProbePostCommitOffset), ns)
	atomic.StoreUint64(v2AtomicU64(slot, c19ProbeMagicOffset), c19ProbeMagic)
}
GO

cat > "$GETH/darkbot/fastlane/c19_latency_test.go" <<'GO'
package fastlane

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

const (
	c19Generation  = uint64(1909)
	c19SyncTimeout = 120 * time.Second
)

func c19Env(t *testing.T, name string) int {
	t.Helper()
	value, err := strconv.Atoi(os.Getenv(name))
	if err != nil || value < 0 {
		t.Fatalf("%s: %q", name, os.Getenv(name))
	}
	return value
}

func c19WaitForFile(t *testing.T, path string) {
	t.Helper()
	deadline := time.Now().Add(c19SyncTimeout)
	for time.Now().Before(deadline) {
		if _, err := os.Stat(path); err == nil {
			return
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatalf("timed out waiting for %s", path)
}

func c19WriteFile(t *testing.T, path string, data []byte) {
	t.Helper()
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
}

// c19LoopbackTarget is a selected peer whose send is one write to a
// connected loopback TCP socket with TCP_NODELAY: a local transport write.
func c19LoopbackTarget(t *testing.T, id byte) OutboundTargetV2 {
	t.Helper()
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	go func() {
		conn, err := listener.Accept()
		if err == nil {
			_, _ = io.Copy(io.Discard, conn)
		}
	}()
	conn, err := net.Dial("tcp", listener.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	if tcp, ok := conn.(*net.TCPConn); ok {
		_ = tcp.SetNoDelay(true)
	}
	t.Cleanup(func() {
		_ = conn.Close()
		_ = listener.Close()
	})
	return OutboundTargetV2{
		NodeID: c14Node(id),
		Send: func(_ common.Hash, raw []byte) error {
			_, err := conn.Write(raw)
			return err
		},
	}
}

var c19MakeTarget = func(t *testing.T, id byte, total int) OutboundTargetV2 {
 return c19LoopbackTarget(t, id)
}

func TestC19NativeLatencyProducer(t *testing.T) {
	dir := os.Getenv("DARKBOT_C19_DIR")
	if dir == "" {
		t.Skip("C1.9 latency harness; run by bsc/tools/c19_native_latency.sh")
	}
	peers := c19Env(t, "DARKBOT_C19_PEERS")
	warmup := c19Env(t, "DARKBOT_C19_WARMUP_PER_PEER")
	samples := c19Env(t, "DARKBOT_C19_SAMPLES_PER_PEER")
	workers := c19Env(t, "DARKBOT_C19_OUTBOUND_WORKERS")
	perPeer := warmup + samples
	if peers < 1 || samples < 1 || perPeer >= 4096 {
		t.Fatalf("peers=%d warmup=%d samples=%d", peers, warmup, samples)
	}

	cfg := defaultIngressConfigV2(dir, c19Generation, peers, 0)
	// No slot is reused during the run, so every L1 probe stays readable.
	cfg.PeerLaneCapacity = 4096
	cfg.PeerLaneSlotStride = 4096
	cfg.TxpoolLaneCapacity = 64
	cfg.TxpoolLaneSlotStride = 4096
	cfg.OutboundWorkers = workers
	r := c14Start(t, cfg)
	if err := r.install(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(ShutdownIngressV2)

	total := peers * perPeer
	if workers > 0 {
		targets := make([]OutboundTargetV2, 0, workers)
		for i := 0; i < workers; i++ {
			targets = append(targets, c19MakeTarget(t, byte(0x80+i), total))
		}
		if err := AssignOutboundTargetsV2(targets); err != nil {
			t.Fatal(err)
		}
		if !StartOutboundV2() {
			t.Fatal("outbound did not start")
		}
	}
	// One prepared signed BUY per launch, for the Rust side to publish.
	var buys strings.Builder
	for i := 0; i < total; i++ {
		tx := c14Tx(t, uint64(1_000_000+i), &c14Portal, c14BuyCalldata(), i%2 == 0)
		raw, err := tx.MarshalBinary()
		if err != nil {
			t.Fatal(err)
		}
		hash := tx.Hash()
		buys.WriteString(hex.EncodeToString(hash[:]) + " " + hex.EncodeToString(raw) + "\n")
	}
	c19WriteFile(t, filepath.Join(dir, "buys.txt"), []byte(buys.String()))

	handles := make([]*PeerIngressV2, peers)
	txs := make([][]*types.Transaction, peers)
	laneIDs := make([]string, peers)
	for peer := 0; peer < peers; peer++ {
		handles[peer] = c14Bind(t, r, c14Node(byte(0x20+peer)))
		laneIDs[peer] = strconv.FormatUint(uint64(handles[peer].lease.LaneID), 10)
		txs[peer] = make([]*types.Transaction, perPeer)
		for sample := 0; sample < perPeer; sample++ {
			nonce := uint64(peer*perPeer + sample)
			// Unique calldata per launch: no duplicate hashes across peers.
			data := append(c14SmallLaunch(byte(sample)), byte(peer), byte(sample>>8))
			txs[peer][sample] = c14Tx(t, nonce, &c14Portal, data, (peer+sample)%2 == 0)
		}
	}

	manifest := strings.Join([]string{
		"generation=" + strconv.FormatUint(cfg.Generation, 10),
		"peers=" + strconv.Itoa(peers),
		"warmup_per_peer=" + strconv.Itoa(warmup),
		"samples_per_peer=" + strconv.Itoa(samples),
		"outbound_workers=" + strconv.Itoa(workers),
		"peer_lane_capacity=" + strconv.FormatUint(uint64(cfg.PeerLaneCapacity), 10),
		"peer_lane_slot_stride=" + strconv.FormatUint(uint64(cfg.PeerLaneSlotStride), 10),
		"lane_ids=" + strings.Join(laneIDs, ","),
		"clock_id=" + strconv.FormatUint(uint64(v2ClockMonotonic), 10),
		"clock=CLOCK_MONOTONIC",
		"load_model=parallel_peers_one_outstanding_per_lane_with_explicit_warmup",
		"outbound_mode=" + os.Getenv("DARKBOT_C19_OUTBOUND_MODE"),
        "purpose=" + os.Getenv("DARKBOT_C19_PURPOSE"),
        "transport_topology=controlled_localhost_peers_no_external_network",
		"chain_load=canonical_head_every_500us_under_one_lock",
	}, "\n") + "\n"
	c19WriteFile(t, filepath.Join(dir, "manifest.txt"), []byte(manifest))
	c19WaitForFile(t, filepath.Join(dir, "rust.ready"))

	// Chain evidence is published throughout, as under chain import.
	stopChain := make(chan struct{})
	var chainDone sync.WaitGroup
	chainDone.Add(1)
	go func() {
		defer chainDone.Done()
		for number := int64(1); ; number++ {
			select {
			case <-stopChain:
				return
			default:
			}
			PublishCanonicalV2(c15Header(number, byte(number)), int(number%200))
			time.Sleep(500 * time.Microsecond)
		}
	}()

	start := make(chan struct{})
	errs := make(chan error, peers)
	var wg sync.WaitGroup
	for peer := 0; peer < peers; peer++ {
		wg.Add(1)
		go func(peer int) {
			defer wg.Done()
			<-start
			source := IngressSourceEthBroadcast
			if peer%2 != 0 {
				source = IngressSourceEthPooled
			}
			for sample := 0; sample < perPeer; sample++ {
				if err := IngressV2(handles[peer], source, txs[peer][sample:sample+1], c14Accept); err != nil {
					errs <- fmt.Errorf("peer %d sample %d: %w", peer, sample, err)
					return
				}
				// Latency, not the C1.8 burst gate: at most one outstanding
				// record on each independent peer lane.
				expectedRead := uint64(sample + 1)
				deadline := time.Now().Add(c19SyncTimeout)
				for handles[peer].lane.stats().ReadSequence < expectedRead {
					if time.Now().After(deadline) {
						errs <- fmt.Errorf("peer %d sample %d: not drained", peer, sample)
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
	close(stopChain)
	chainDone.Wait()

	if workers > 0 {
		deadline := time.Now().Add(c19SyncTimeout)
		for {
			outbound, _ := OutboundV2Snapshot()
			handed := uint64(0)
			for _, worker := range outbound.PerWorker {
				handed += worker.HandedOff
			}
			if handed == uint64(total*workers) {
				break
			}
			if time.Now().After(deadline) {
				t.Fatalf("outbound did not settle: %+v", outbound)
			}
			time.Sleep(time.Millisecond)
		}
		StopOutboundV2()
	}

	snapshot := r.snapshot()
	expected := uint64(total)
	if snapshot.LiveCovered != peers || snapshot.LiveUncovered != 0 ||
		snapshot.GapTotal != 0 || snapshot.Peers.Published != expected ||
		snapshot.Peers.QueueFull != 0 || snapshot.Peers.Oversize != 0 ||
		snapshot.Peers.Refused != 0 || snapshot.Peers.SerializeErrors != 0 ||
		snapshot.Peers.ClockErrors != 0 || snapshot.Peers.UncoveredCandidates != 0 {
		t.Fatalf("C1.9 producer evidence %+v", snapshot)
	}
	chain, _ := ChainV2Snapshot()
	if chain.OwnershipViolations != 0 || chain.Lost != 0 || chain.Published == 0 {
		t.Fatalf("C1.9 chain evidence %+v", chain)
	}
	evidence := map[string]any{"ingress": snapshot, "chain": chain}
	if outbound, ok := OutboundV2Snapshot(); ok {
		if outbound.Dispatched != expected || outbound.InvalidEgress != 0 || outbound.QueueFull != 0 ||
			outbound.EvidenceLost != 0 || outbound.EgressErrors != 0 {
			t.Fatalf("C1.9 outbound evidence %+v", outbound)
		}
		evidence["outbound"] = outbound
	}
	encoded, err := json.MarshalIndent(evidence, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	c19WriteFile(t, filepath.Join(dir, "go_snapshot.json"), append(encoded, '\n'))
	c19WriteFile(t, filepath.Join(dir, "go.done"), []byte("ok\n"))
	c19WaitForFile(t, filepath.Join(dir, "rust.done"))
	fmt.Println("C19_GETH_PRODUCER_OK")
}
GO

gofmt -w \
  "$GETH/darkbot/fastlane/v2.go" \
  "$GETH/darkbot/fastlane/c19_probe.go" \
  "$GETH/darkbot/fastlane/c19_latency_test.go"

export DARKBOT_C19_DIR="$DATA"

echo "=== C1.9 BUILD GO MEASUREMENT BINARY ==="
if [ "$DARKBOT_C19_OUTBOUND_MODE" = production_devp2p_selected_peer_handoff ]; then
  python3 "$ROOT/bsc/tools/c19_prepare_production.py" "$GETH"
  GO_PACKAGE=./eth/protocols/eth
fi
(cd "$GETH" && go test -c -o "$GO_BIN" "$GO_PACKAGE")

echo "=== C1.9 BUILD RUST RELEASE CONSUMER AND HOST PROFILE ==="
(cd "$ENGINE" && cargo build -p darkbot-bsc-geth-ipc --release --locked --offline \
  --example c19_consumer --example c16_host_profile)

echo "=== C1.9 HOST PROFILE ==="
"$HOST_BIN" --profile "$PROFILE" --out "$DATA/host_profile.json"

echo "=== C1.9 START GO PRODUCERS ==="
"$GO_BIN" -test.run '^TestC19NativeLatencyProducer$' -test.count=1 -test.v -test.timeout 30m \
  >"$DATA/go.log" 2>&1 &
GO_PID=$!

READY=0
for _ in $(seq 1 6000); do
  if [ -f "$DATA/manifest.txt" ]; then
    READY=1
    break
  fi
  if ! kill -0 "$GO_PID" 2>/dev/null; then
    cat "$DATA/go.log" >&2
    echo "C19_FAIL reason=go-producer-exited-before-manifest" >&2
    exit 1
  fi
  sleep 0.01
done
if [ "$READY" -ne 1 ]; then
  kill "$GO_PID" 2>/dev/null || true
  cat "$DATA/go.log" >&2
  echo "C19_FAIL reason=manifest-timeout" >&2
  exit 1
fi

echo "=== C1.9 RUN RUST CONSUMER ==="
if ! "$RUST_BIN" >"$DATA/rust.log" 2>&1; then
  kill "$GO_PID" 2>/dev/null || true
  cat "$DATA/rust.log" >&2
  cat "$DATA/go.log" >&2
  echo "C19_FAIL reason=rust-consumer" >&2
  exit 1
fi
if ! wait "$GO_PID"; then
  cat "$DATA/go.log" >&2
  echo "C19_FAIL reason=go-producer" >&2
  exit 1
fi
if ! grep -q 'C19_GETH_PRODUCER_OK' "$DATA/go.log"; then
  echo "C19_FAIL reason=go-marker-missing" >&2
  exit 1
fi
if ! grep -q 'C19_RUST_CONSUMER_OK' "$DATA/rust.log"; then
  echo "C19_FAIL reason=rust-marker-missing" >&2
  exit 1
fi

# The raw evidence is kept exactly as recorded, warmup included.
for name in raw_ingress.csv raw_outbound.csv go_snapshot.json rust_summary.txt manifest.txt \
  host_profile.json go.log rust.log; do
  cp "$DATA/$name" "$PREFIX-$name"
done

python3 "$ROOT/bsc/tools/c19_analyze.py" \
  --raw-ingress "$PREFIX-raw_ingress.csv" \
  --raw-outbound "$PREFIX-raw_outbound.csv" \
  --manifest "$PREFIX-manifest.txt" \
  --go-snapshot "$PREFIX-go_snapshot.json" \
  --rust-summary "$PREFIX-rust_summary.txt" \
  --host-profile "$PREFIX-host_profile.json" \
  --repo "$ROOT" \
  --geth "$GETH" \
  --go-bin "$GO_BIN" \
  --rust-bin "$RUST_BIN" \
  --output "$PREFIX.json"

echo "=== C1.9 RESULT ==="
echo "evidence=$PREFIX.json"
echo "C1_9_MEASUREMENT_COMPLETE (review the evidence; this script does not pass the gate)"
