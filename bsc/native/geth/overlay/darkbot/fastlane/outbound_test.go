package fastlane

import (
	"encoding/binary"
	"errors"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

// c17Peer is a selected peer whose send can be held or made to fail.
type c17Peer struct {
	id   [32]byte
	hold chan struct{}
	fail bool
	know bool

	mu   sync.Mutex
	raws [][]byte
	sent atomic.Uint64
}

func (p *c17Peer) send(hash common.Hash, raw []byte) error {
	if p.hold != nil {
		<-p.hold
	}
	if p.fail {
		return errors.New("peer write failed")
	}
	p.mu.Lock()
	p.raws = append(p.raws, raw)
	p.mu.Unlock()
	p.sent.Add(1)
	return nil
}

func (p *c17Peer) target() OutboundTargetV2 {
	return OutboundTargetV2{
		NodeID: p.id,
		Send:   p.send,
		Known:  func(common.Hash) bool { return p.know },
	}
}

func c17Start(t *testing.T, workers int) (*ingressRuntimeV2, ingressConfigV2) {
	t.Helper()
	cfg := c14TestConfig(t, 2, 0, 8, 512)
	cfg.OutboundWorkers = workers
	r := c14Start(t, cfg)
	t.Cleanup(r.shutdown)
	return r, cfg
}

// c17Egress publishes one signed transaction on the egress lane, as HOT0
// does, and returns the transaction.
func c17Egress(t *testing.T, r *ingressRuntimeV2, nonce uint64) *types.Transaction {
	t.Helper()
	tx := c14Tx(t, nonce, &c14Portal, c14BuyCalldata(), true)
	raw, err := tx.MarshalBinary()
	if err != nil {
		t.Fatal(err)
	}
	now, err := MonotonicNS()
	if err != nil {
		t.Fatal(err)
	}
	err = r.outbound.egress.publish(recordV2{
		Kind:        OutboundSignedTx,
		TimestampNS: now,
		ClockID:     v2ClockMonotonic,
		Hash:        tx.Hash(),
		Payload:     raw,
	})
	if err != nil {
		t.Fatal(err)
	}
	return tx
}

func c17Wait(t *testing.T, what string, done func() bool) {
	t.Helper()
	deadline := time.Now().Add(10 * time.Second)
	for !done() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(time.Millisecond)
	}
}

// c17Evidence drains one evidence lane: index 0 is the coordinator's lane,
// index i+1 worker i's.
func c17Evidence(t *testing.T, cfg ingressConfigV2, index int) []*recordV2 {
	t.Helper()
	lane := outboundLanePlanV2(cfg.OutboundWorkers)[1+index]
	return c17Drain(t, c14Consumer(t, cfg, lane))
}

func c17Drain(t *testing.T, lane *laneV2) []*recordV2 {
	t.Helper()
	var out []*recordV2
	for {
		record, ok, err := lane.tryConsume()
		if err != nil {
			t.Fatal(err)
		}
		if !ok {
			return out
		}
		out = append(out, record)
	}
}

type c17Aux struct {
	status   OutboundStatusV2
	worker   int
	selected int
	queued   int
	egress   uint64
	observed uint64
	third    uint64
}

func c17DecodeAux(record *recordV2) c17Aux {
	return c17Aux{
		status:   OutboundStatusV2(record.Aux[0]),
		worker:   int(record.Aux[1]),
		selected: int(record.Aux[2]),
		queued:   int(record.Aux[3]),
		egress:   binary.LittleEndian.Uint64(record.Aux[8:16]),
		observed: binary.LittleEndian.Uint64(record.Aux[16:24]),
		third:    binary.LittleEndian.Uint64(record.Aux[24:32]),
	}
}

func c17Snapshot(r *ingressRuntimeV2) OutboundEvidenceV2 {
	return r.outbound.evidence()
}

func TestC17LanePlanAndBounds(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	off := c14Start(t, cfg)
	t.Cleanup(off.shutdown)
	if off.outbound != nil || off.chain == nil {
		t.Fatalf("outbound=%v chain=%v", off.outbound, off.chain)
	}
	if _, err := off.rt.arena.laneByID(c17EgressLaneID); err == nil {
		t.Fatal("egress lane exists without outbound workers")
	}

	tooMany := c14TestConfig(t, 1, 0, 8, 512)
	tooMany.OutboundWorkers = c17MaxOutboundWorkers + 1
	if _, err := startIngressRuntimeV2(tooMany); err == nil {
		t.Fatal("too many outbound workers accepted")
	}

	r, _ := c17Start(t, 3)
	plan := outboundLanePlanV2(3)
	if len(plan) != 5 || plan[0].Direction != v2RustToGeth || plan[0].Class != v2SignedTransactionEgress {
		t.Fatalf("plan %+v", plan)
	}
	for _, lane := range plan[1:] {
		if lane.Direction != v2GethToRust || lane.Class != v2OutboundHandoffEvidence {
			t.Fatalf("evidence lane %+v", lane)
		}
		if _, err := r.rt.arena.laneByID(lane.ID); err != nil {
			t.Fatal(err)
		}
	}
	if len(r.outbound.workers) != 3 {
		t.Fatalf("%d workers", len(r.outbound.workers))
	}
}

func TestC17EverySelectedPeerGetsTheSameSignedBytes(t *testing.T) {
	r, cfg := c17Start(t, 3)
	peers := []*c17Peer{{id: c14Node(0xa1)}, {id: c14Node(0xa2)}, {id: c14Node(0xa3)}}
	targets := []OutboundTargetV2{peers[0].target(), peers[1].target(), peers[2].target()}
	if err := r.outbound.assign(targets); err != nil {
		t.Fatal(err)
	}
	r.outbound.start()
	before, err := MonotonicNS()
	if err != nil {
		t.Fatal(err)
	}
	tx := c17Egress(t, r, 1)
	raw, _ := tx.MarshalBinary()
	c17Wait(t, "three handoffs", func() bool {
		return peers[0].sent.Load()+peers[1].sent.Load()+peers[2].sent.Load() == 3
	})
	c17Wait(t, "dispatch counted", func() bool { return c17Snapshot(r).Dispatched == 1 })
	r.outbound.stopAndWait()

	for i, peer := range peers {
		if len(peer.raws) != 1 || string(peer.raws[0]) != string(raw) {
			t.Fatalf("peer %d did not get the exact signed bytes", i)
		}
		recs := c17Evidence(t, cfg, i+1)
		if len(recs) != 1 {
			t.Fatalf("worker %d: %d evidence records", i, len(recs))
		}
		rec, aux := recs[0], c17DecodeAux(recs[0])
		if rec.Kind != OutboundPeerHandoff || rec.Hash != tx.Hash() || rec.Peer != peer.id ||
			rec.Source != [16]byte(outboundSourceWorkerV2) || rec.ClockID != v2ClockMonotonic {
			t.Fatalf("worker %d record %+v", i, rec)
		}
		if aux.status != OutboundHandedOff || aux.worker != i || aux.selected != 3 || aux.egress != 0 {
			t.Fatalf("worker %d aux %+v", i, aux)
		}
		// The worker's own timestamps, in order: observed, send start, send return.
		if !(before <= aux.observed && aux.observed <= aux.third && aux.third <= rec.TimestampNS) {
			t.Fatalf("worker %d times before=%d observed=%d start=%d done=%d", i, before, aux.observed, aux.third, rec.TimestampNS)
		}
	}
	coordinator := c17Evidence(t, cfg, 0)
	if len(coordinator) != 1 {
		t.Fatalf("%d coordinator records", len(coordinator))
	}
	aux := c17DecodeAux(coordinator[0])
	if aux.status != OutboundDispatched || aux.worker != c17NoWorker || aux.selected != 3 || aux.queued != 3 {
		t.Fatalf("coordinator aux %+v", aux)
	}
	if coordinator[0].Source != [16]byte(outboundSourceCoordinatorV2) || coordinator[0].Hash != tx.Hash() {
		t.Fatalf("coordinator record %+v", coordinator[0])
	}
	// aux third on a coordinator record is Rust's egress commit timestamp.
	if aux.third == 0 || aux.third > aux.observed {
		t.Fatalf("egress commit %d observed %d", aux.third, aux.observed)
	}
}

func TestC17SlowPeerDoesNotDelayTheOtherPeer(t *testing.T) {
	r, cfg := c17Start(t, 2)
	slow := &c17Peer{id: c14Node(0xb1), hold: make(chan struct{})}
	fast := &c17Peer{id: c14Node(0xb2)}
	if err := r.outbound.assign([]OutboundTargetV2{slow.target(), fast.target()}); err != nil {
		t.Fatal(err)
	}
	r.outbound.start()

	// The first BUY: the slow peer's worker is stuck inside its send.
	first := c17Egress(t, r, 1)
	c17Wait(t, "fast peer handoff", func() bool { return fast.sent.Load() == 1 })
	// The slow worker has taken the first BUY out of its queue and is held.
	c17Wait(t, "slow worker inside its send", func() bool { return len(r.outbound.workers[0].jobs) == 0 })
	if slow.sent.Load() != 0 {
		t.Fatal("slow peer finished early")
	}
	// More BUYs than the slow worker's queue holds: the fast peer still gets
	// every one, the slow worker's overflow is reported, nothing waits.
	const more = c17WorkerQueueDepth + 4
	for nonce := uint64(2); nonce < 2+more; nonce++ {
		c17Egress(t, r, nonce)
	}
	c17Wait(t, "fast peer got every BUY", func() bool { return fast.sent.Load() == 1+more })
	c17Wait(t, "all dispatched", func() bool { return c17Snapshot(r).Dispatched == 1+more })
	snapshot := c17Snapshot(r)
	if snapshot.QueueFull != 4 || slow.sent.Load() != 0 {
		t.Fatalf("queue_full=%d slow_sent=%d", snapshot.QueueFull, slow.sent.Load())
	}
	// The fast worker's first handoff was recorded while the slow peer was
	// still held.
	fastEvidence := c17Evidence(t, cfg, 2)
	if len(fastEvidence) != 1+more || fastEvidence[0].Hash != first.Hash() {
		t.Fatalf("%d fast evidence records", len(fastEvidence))
	}
	if got := c17Evidence(t, cfg, 1); len(got) != 0 {
		t.Fatalf("slow worker reported %d handoffs while held", len(got))
	}

	close(slow.hold)
	c17Wait(t, "slow peer drained", func() bool { return slow.sent.Load() == 1+c17WorkerQueueDepth })
	r.outbound.stopAndWait()
	full := 0
	for _, rec := range c17Evidence(t, cfg, 0) {
		aux := c17DecodeAux(rec)
		if aux.status == OutboundQueueFull {
			if aux.worker != 0 {
				t.Fatalf("queue full attributed to worker %d", aux.worker)
			}
			full++
		}
	}
	if full != 4 {
		t.Fatalf("%d QUEUE_FULL evidence records", full)
	}
	if got := len(c17Evidence(t, cfg, 1)); got != 1+c17WorkerQueueDepth {
		t.Fatalf("slow worker evidence %d", got)
	}
}

func TestC17FailingPeerIsUnknownAndThePartialHandoffIsVisible(t *testing.T) {
	r, cfg := c17Start(t, 2)
	failing := &c17Peer{id: c14Node(0xc1), fail: true}
	healthy := &c17Peer{id: c14Node(0xc2)}
	if err := r.outbound.assign([]OutboundTargetV2{failing.target(), healthy.target()}); err != nil {
		t.Fatal(err)
	}
	r.outbound.start()
	tx := c17Egress(t, r, 1)
	c17Wait(t, "both workers reported", func() bool {
		s := c17Snapshot(r)
		return s.PerWorker[0].SendErrors == 1 && s.PerWorker[1].HandedOff == 1
	})
	r.outbound.stopAndWait()

	failed := c17Evidence(t, cfg, 1)
	if len(failed) != 1 || c17DecodeAux(failed[0]).status != OutboundSendErrorUnknown ||
		failed[0].Peer != failing.id || failed[0].Hash != tx.Hash() {
		t.Fatalf("failing worker evidence %+v", failed)
	}
	handed := c17Evidence(t, cfg, 2)
	if len(handed) != 1 || c17DecodeAux(handed[0]).status != OutboundHandedOff || handed[0].Peer != healthy.id {
		t.Fatalf("healthy worker evidence %+v", handed)
	}
	// One of two selected peers handed off: a partial handoff, with both
	// outcomes on record and neither called a purchase.
	summary := c17DecodeAux(c17Evidence(t, cfg, 0)[0])
	if summary.status != OutboundDispatched || summary.selected != 2 || summary.queued != 2 {
		t.Fatalf("summary %+v", summary)
	}
}

func TestC17UnselectedWorkerSlotsAreReportedNotSent(t *testing.T) {
	r, cfg := c17Start(t, 3)
	only := &c17Peer{id: c14Node(0xd1)}
	if err := r.outbound.assign([]OutboundTargetV2{only.target()}); err != nil {
		t.Fatal(err)
	}
	r.outbound.start()
	c17Egress(t, r, 1)
	c17Wait(t, "handoff", func() bool { return only.sent.Load() == 1 })
	c17Wait(t, "dispatch counted", func() bool { return c17Snapshot(r).Dispatched == 1 })
	r.outbound.stopAndWait()
	noTarget, dispatched := 0, 0
	for _, rec := range c17Evidence(t, cfg, 0) {
		switch aux := c17DecodeAux(rec); aux.status {
		case OutboundNoTarget:
			noTarget++
		case OutboundDispatched:
			dispatched++
			if aux.selected != 1 || aux.queued != 1 {
				t.Fatalf("summary %+v", aux)
			}
		}
	}
	if noTarget != 2 || dispatched != 1 {
		t.Fatalf("no_target=%d dispatched=%d", noTarget, dispatched)
	}
}

func TestC17NoGoroutineIsCreatedPerBuy(t *testing.T) {
	r, _ := c17Start(t, 4)
	peers := []*c17Peer{{id: c14Node(0xe1)}, {id: c14Node(0xe2)}, {id: c14Node(0xe3)}, {id: c14Node(0xe4)}}
	targets := make([]OutboundTargetV2, 0, len(peers))
	for _, peer := range peers {
		targets = append(targets, peer.target())
	}
	if err := r.outbound.assign(targets); err != nil {
		t.Fatal(err)
	}
	base := runtime.NumGoroutine()
	if !r.outbound.start() || r.outbound.start() {
		t.Fatal("start is not once-only")
	}
	// One coordinator and one worker per slot, created before any BUY.
	c17Wait(t, "permanent goroutines", func() bool { return runtime.NumGoroutine() == base+5 })
	const buys = 200
	for nonce := uint64(0); nonce < buys; nonce++ {
		c17Egress(t, r, nonce)
		c17Wait(t, "handoffs", func() bool { return peers[3].sent.Load() == nonce+1 })
		if got := runtime.NumGoroutine(); got != base+5 {
			t.Fatalf("goroutines %d after BUY %d, want %d", got, nonce, base+5)
		}
	}
	for i, peer := range peers {
		c17Wait(t, "all handoffs", func() bool { return c17Snapshot(r).PerWorker[i].HandedOff == buys })
		if got := peer.sent.Load(); got != buys {
			t.Fatalf("peer %d received %d", i, got)
		}
	}
	r.outbound.stopAndWait()
	r.outbound.stopAndWait()
	c17Wait(t, "goroutines ended", func() bool { return runtime.NumGoroutine() == base })
	if c17Snapshot(r).Running {
		t.Fatal("still running after stop")
	}
}

func TestC17PeerSelectionIsStableAndOutsideDispatch(t *testing.T) {
	r, _ := c17Start(t, 2)
	a, b, c := &c17Peer{id: c14Node(0xf1)}, &c17Peer{id: c14Node(0xf2)}, &c17Peer{id: c14Node(0xf3)}
	workers := r.outbound.workers
	if err := r.outbound.assign([]OutboundTargetV2{a.target(), b.target()}); err != nil {
		t.Fatal(err)
	}
	if workers[0].target.Load().NodeID != a.id || workers[1].target.Load().NodeID != b.id {
		t.Fatal("initial assignment")
	}
	// A leaves, C arrives: B keeps its worker, C takes A's.
	if err := r.outbound.assign([]OutboundTargetV2{b.target(), c.target()}); err != nil {
		t.Fatal(err)
	}
	if workers[0].target.Load().NodeID != c.id || workers[1].target.Load().NodeID != b.id {
		t.Fatal("reassignment moved a peer that stayed selected")
	}
	// The same selection again changes nothing.
	before := c17Snapshot(r).Reassignments
	if err := r.outbound.assign([]OutboundTargetV2{c.target(), b.target()}); err != nil {
		t.Fatal(err)
	}
	if c17Snapshot(r).Reassignments != before {
		t.Fatal("unchanged selection was reassigned")
	}
	// No peers: every slot is cleared.
	if err := r.outbound.assign(nil); err != nil {
		t.Fatal(err)
	}
	if workers[0].target.Load() != nil || workers[1].target.Load() != nil {
		t.Fatal("slots kept a peer that is no longer selected")
	}
	if err := r.outbound.assign([]OutboundTargetV2{{NodeID: a.id}}); !errors.Is(err, errOutboundNoSend) {
		t.Fatalf("target without send: %v", err)
	}
}

func TestC17InvalidEgressRecordsAreRefusedExplicitly(t *testing.T) {
	r, cfg := c17Start(t, 1)
	peer := &c17Peer{id: c14Node(0x71)}
	if err := r.outbound.assign([]OutboundTargetV2{peer.target()}); err != nil {
		t.Fatal(err)
	}
	r.outbound.start()
	tx := c14Tx(t, 1, &c14Portal, c14BuyCalldata(), true)
	raw, _ := tx.MarshalBinary()
	bad := []recordV2{
		{Kind: OutboundSignedTx, Hash: common.Hash{1}, Payload: raw},
		{Kind: OutboundSignedTx, Hash: tx.Hash(), Payload: []byte{0xde, 0xad}},
		{Kind: EarlyTxSeen, Hash: tx.Hash(), Payload: raw},
	}
	for _, record := range bad {
		record.ClockID = v2ClockMonotonic
		record.TimestampNS = 1
		if err := r.outbound.egress.publish(record); err != nil {
			t.Fatal(err)
		}
	}
	good := c17Egress(t, r, 2)
	c17Wait(t, "valid BUY handed off", func() bool { return peer.sent.Load() == 1 })
	r.outbound.stopAndWait()
	if s := c17Snapshot(r); s.InvalidEgress != 3 || s.Dispatched != 1 || s.EgressRecords != 4 {
		t.Fatalf("snapshot %+v", s)
	}
	recs := c17Evidence(t, cfg, 0)
	if len(recs) != 4 {
		t.Fatalf("%d coordinator records", len(recs))
	}
	for i, rec := range recs[:3] {
		if aux := c17DecodeAux(rec); aux.status != OutboundInvalidEgress || aux.egress != uint64(i) {
			t.Fatalf("record %d aux %+v", i, aux)
		}
	}
	if recs[3].Hash != good.Hash() || c17DecodeAux(recs[3]).status != OutboundDispatched {
		t.Fatalf("last record %+v", recs[3])
	}
}

func TestC17PeerThatKnowsTheTransactionIsNotSentTo(t *testing.T) {
	r, cfg := c17Start(t, 1)
	peer := &c17Peer{id: c14Node(0x72), know: true}
	if err := r.outbound.assign([]OutboundTargetV2{peer.target()}); err != nil {
		t.Fatal(err)
	}
	r.outbound.start()
	c17Egress(t, r, 1)
	c17Wait(t, "already known", func() bool { return c17Snapshot(r).PerWorker[0].AlreadyKnown == 1 })
	r.outbound.stopAndWait()
	recs := c17Evidence(t, cfg, 1)
	if len(recs) != 1 || c17DecodeAux(recs[0]).status != OutboundAlreadyKnown || peer.sent.Load() != 0 {
		t.Fatalf("evidence %+v sent=%d", recs, peer.sent.Load())
	}
}

func TestC17EvidenceKeepsPerWorkerOrderAndEgressSequence(t *testing.T) {
	r, cfg := c17Start(t, 2)
	peers := []*c17Peer{{id: c14Node(0x81)}, {id: c14Node(0x82)}}
	if err := r.outbound.assign([]OutboundTargetV2{peers[0].target(), peers[1].target()}); err != nil {
		t.Fatal(err)
	}
	r.outbound.start()
	const buys = 5
	hashes := make([]common.Hash, 0, buys)
	for nonce := uint64(0); nonce < buys; nonce++ {
		hashes = append(hashes, c17Egress(t, r, nonce).Hash())
	}
	c17Wait(t, "handoffs", func() bool { return peers[0].sent.Load() == buys && peers[1].sent.Load() == buys })
	r.outbound.stopAndWait()
	for worker := 0; worker < 2; worker++ {
		recs := c17Evidence(t, cfg, worker+1)
		if len(recs) != buys {
			t.Fatalf("worker %d: %d records", worker, len(recs))
		}
		for i, rec := range recs {
			aux := c17DecodeAux(rec)
			// Each worker reports in egress order, with its own lane
			// sequence and non-decreasing timestamps.
			if rec.Hash != hashes[i] || aux.egress != uint64(i) || rec.Sequence != uint64(i) || aux.worker != worker {
				t.Fatalf("worker %d record %d: %+v aux %+v", worker, i, rec, aux)
			}
			if i > 0 && rec.TimestampNS < recs[i-1].TimestampNS {
				t.Fatalf("worker %d timestamps went backwards", worker)
			}
		}
	}
}

func TestC17ShutdownStopsTheWorkersBeforeSealingTheLanes(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	cfg.OutboundWorkers = 2
	r := c14Start(t, cfg)
	base := runtime.NumGoroutine()
	r.outbound.start()
	c17Wait(t, "started", func() bool { return runtime.NumGoroutine() == base+3 })
	r.shutdown()
	c17Wait(t, "stopped", func() bool { return runtime.NumGoroutine() == base })
	if c17Snapshot(r).Running {
		t.Fatal("running after shutdown")
	}
	// The sealed egress lane refuses a late BUY.
	err := r.outbound.egress.publish(recordV2{Kind: OutboundSignedTx, ClockID: v2ClockMonotonic, Payload: []byte{1}})
	if !errors.Is(err, errV2Shutdown) {
		t.Fatalf("publish after shutdown: %v", err)
	}
}

func TestC16PermanentThreadQoSIsRecordedPerRole(t *testing.T) {
	t.Setenv(envPermanentThreadQoS, "no-such-class")
	done := make(chan struct{})
	go func() {
		defer close(done)
		applyPermanentThreadQoS("test-unknown")
	}()
	<-done
	if got := permanentThreadQoSSnapshot()["test-unknown"]; got != "refused:unknown-class:no-such-class" {
		t.Fatalf("unknown class outcome %q", got)
	}
	t.Setenv(envPermanentThreadQoS, "")
	applyPermanentThreadQoS("test-off")
	if got := permanentThreadQoSSnapshot()["test-off"]; got != "not-requested" {
		t.Fatalf("off outcome %q", got)
	}
	t.Setenv(envPermanentThreadQoS, "user-initiated")
	done = make(chan struct{})
	go func() {
		defer close(done)
		applyPermanentThreadQoS("test-class")
	}()
	<-done
	got := permanentThreadQoSSnapshot()["test-class"]
	want := "applied:user-initiated"
	if !threadQoSSupported {
		want = "unsupported:" + threadQoSUnsupportedReason
	}
	if got != want {
		t.Fatalf("class outcome %q, want %q", got, want)
	}
}

// The worker is deliberately not started until after reassignment: no sleeps
// or scheduler timing are needed to reproduce the queued-BUY race.
func TestC17QueuedBuyKeepsDispatchTargetAfterReassignment(t *testing.T) {
	for _, failed := range []bool{false, true} {
		t.Run(fmt.Sprint("closed=", failed), func(t *testing.T) {
			r, cfg := c17Start(t, 1)
			a := &c17Peer{id: c14Node(0xa1), fail: failed}
			c := &c17Peer{id: c14Node(0xc1)}
			if err := r.outbound.assign([]OutboundTargetV2{a.target()}); err != nil {
				t.Fatal(err)
			}
			tx := c17Egress(t, r, 701)
			rec, ok, err := r.outbound.egress.tryConsume()
			if err != nil || !ok {
				t.Fatal("missing egress", err)
			}
			r.outbound.dispatch(rec)
			if err := r.outbound.assign([]OutboundTargetV2{c.target()}); err != nil {
				t.Fatal(err)
			}
			w := r.outbound.workers[0]
			w.handle(<-w.jobs)
			if c.sent.Load() != 0 {
				t.Fatal("queued BUY retargeted to C")
			}
			later := c17Egress(t, r, 702)
			rec, ok, err = r.outbound.egress.tryConsume()
			if err != nil || !ok {
				t.Fatal("missing later egress", err)
			}
			r.outbound.dispatch(rec)
			w.handle(<-w.jobs)
			evidence := c17Evidence(t, cfg, 1)
			status := OutboundHandedOff
			if failed {
				status = OutboundSendErrorUnknown
			}
			if len(evidence) != 2 || evidence[0].Peer != a.id || evidence[0].Hash != tx.Hash() ||
				c17DecodeAux(evidence[0]).status != status || evidence[1].Peer != c.id ||
				evidence[1].Hash != later.Hash() || c.sent.Load() != 1 {
				t.Fatalf("wrong bound-peer evidence: %+v", evidence)
			}
		})
	}
}
