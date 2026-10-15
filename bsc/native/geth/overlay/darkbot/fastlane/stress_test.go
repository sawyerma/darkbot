package fastlane

import (
	"sync"
	"sync/atomic"
	"testing"

	"github.com/ethereum/go-ethereum/core/types"
)

// C1.8 concurrency and state-safety stress, geth side. These tests provoke
// queueing, saturation and churn on purpose and check ownership and
// accounting under that load: nothing is lost silently, nothing is
// overwritten, every refusal is counted. They prove invariants, not latency.

// c18Consume drains `lane` on its own goroutine until `stop` is closed and
// the lane is empty, and returns the records in lane order.
func c18Consume(t *testing.T, lane *laneV2, stop <-chan struct{}, out *[]*recordV2, wg *sync.WaitGroup) {
	t.Helper()
	wg.Add(1)
	go func() {
		defer wg.Done()
		for {
			record, ok, err := lane.tryConsume()
			if err != nil {
				t.Errorf("lane %d: %v", lane.config.ID, err)
				return
			}
			if ok {
				*out = append(*out, record)
				continue
			}
			select {
			case <-stop:
				// One more look: a record published just before stop.
				if record, ok, _ := lane.tryConsume(); ok {
					*out = append(*out, record)
					continue
				}
				return
			default:
			}
		}
	}()
}

func TestC18SimultaneousPeerHandlersLoseNothingSilently(t *testing.T) {
	const peers, packets = 12, 400
	cfg := c14TestConfig(t, peers, 0, 16, 512)
	r := c14Start(t, cfg)
	t.Cleanup(r.shutdown)
	handles := make([]*PeerIngressV2, peers)
	seen := make([][]*recordV2, peers)
	stop := make(chan struct{})
	var consumers, producers sync.WaitGroup
	for i := range handles {
		handles[i] = c14Bind(t, r, c14Node(byte(0x20+i)))
		lane := c14Consumer(t, cfg, c14PeerLane(cfg, handles[i].lease.LaneID))
		c18Consume(t, lane, stop, &seen[i], &consumers)
	}
	// Every peer handler is its lane's only producer and runs on its own
	// goroutine, as in geth. One launch per packet.
	for i, handle := range handles {
		producers.Add(1)
		go func(i int, handle *PeerIngressV2) {
			defer producers.Done()
			for n := 0; n < packets; n++ {
				tx := c14Tx(t, uint64(n), &c14Portal, c14SmallLaunch(byte(i)), n%2 == 0)
				if err := IngressV2(handle, IngressSourceEthBroadcast, []*types.Transaction{tx}, c14Accept); err != nil {
					t.Errorf("peer %d: %v", i, err)
					return
				}
			}
		}(i, handle)
	}
	producers.Wait()
	close(stop)
	consumers.Wait()
	for i, handle := range handles {
		c := handle.counters()
		// Every matched launch was either published or counted as refused.
		if c.Matched != packets || c.Published+c.QueueFull != packets || c.Refused != 0 || c.Oversize != 0 {
			t.Fatalf("peer %d counters %+v", i, c)
		}
		if uint64(len(seen[i])) != c.Published {
			t.Fatalf("peer %d: consumer saw %d of %d published", i, len(seen[i]), c.Published)
		}
		node := c14Node(byte(0x20 + i))
		for n, record := range seen[i] {
			if record.Sequence != uint64(n) || record.Peer != node || record.Kind != EarlyTxSeen {
				t.Fatalf("peer %d record %d: %+v", i, n, record)
			}
			if n > 0 && record.TimestampNS < seen[i][n-1].TimestampNS {
				t.Fatalf("peer %d: timestamps went backwards in its lane", i)
			}
		}
		stats := handle.lane.stats()
		if stats.DroppedFull != c.QueueFull || stats.CorruptRecords != 0 {
			t.Fatalf("peer %d lane stats %+v", i, stats)
		}
	}
}

func TestC18StalledConsumerIsCountedAndNeverOverwritten(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	t.Cleanup(r.shutdown)
	handle := c14Bind(t, r, c14Node(0x31))
	const packets = 100
	first := make([]*types.Transaction, 0, 8)
	for n := 0; n < packets; n++ {
		tx := c14Tx(t, uint64(n), &c14Portal, c14SmallLaunch(1), false)
		if n < 8 {
			first = append(first, tx)
		}
		if err := IngressV2(handle, IngressSourceEthPooled, []*types.Transaction{tx}, c14Accept); err != nil {
			t.Fatal(err)
		}
	}
	c := handle.counters()
	if c.Published != 8 || c.QueueFull != packets-8 {
		t.Fatalf("counters %+v", c)
	}
	recs := testDrainV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, handle.lease.LaneID)))
	if len(recs) != 8 {
		t.Fatalf("%d records", len(recs))
	}
	for n, record := range recs {
		// The first eight launches, in order: nothing later overwrote them.
		if record.Hash != first[n].Hash() || record.Sequence != uint64(n) {
			t.Fatalf("record %d is not launch %d", n, n)
		}
	}
}

func TestC18ChurnUnderLoadNeverReusesALaneEarly(t *testing.T) {
	cfg := c14TestConfig(t, 2, 1, 16, 512)
	r := c14Start(t, cfg)
	t.Cleanup(r.shutdown)
	epochs := map[uint16]uint64{}
	for cycle := 0; cycle < 300; cycle++ {
		node := c14Node(byte(cycle%200 + 1))
		handle := r.bindPeer(node)
		if !handle.Covered() {
			t.Fatalf("cycle %d: uncovered: %s", cycle, handle.GapReason())
		}
		laneID, epoch := handle.lease.LaneID, handle.lease.Epoch
		if previous, ok := epochs[laneID]; ok && epoch <= previous {
			t.Fatalf("cycle %d: lane %d epoch %d after %d", cycle, laneID, epoch, previous)
		}
		epochs[laneID] = epoch
		published := cycle % 4
		for n := 0; n < published; n++ {
			tx := c14Tx(t, uint64(cycle*10+n), &c14Portal, c14SmallLaunch(byte(n)), false)
			if err := IngressV2(handle, IngressSourceEthBroadcast, []*types.Transaction{tx}, c14Accept); err != nil {
				t.Fatal(err)
			}
		}
		handle.Close()
		// A closed handle writes nothing, also not into a later binding.
		late := c14Tx(t, uint64(cycle*10+9), &c14Portal, c14SmallLaunch(9), false)
		if err := IngressV2(handle, IngressSourceEthBroadcast, []*types.Transaction{late}, c14Accept); err != nil {
			t.Fatal(err)
		}
		consumer := c14Consumer(t, cfg, c14PeerLane(cfg, laneID))
		recs := testDrainV2(t, consumer)
		if len(recs) != published {
			t.Fatalf("cycle %d: %d records, want %d", cycle, len(recs), published)
		}
		for _, record := range recs {
			if record.LaneEpoch != epoch || record.Peer != node {
				t.Fatalf("cycle %d: record of another binding %+v", cycle, record)
			}
		}
		drained, err := consumer.acknowledgeDrainedIfClosing()
		if err != nil || !drained {
			t.Fatalf("cycle %d: drain ack %v %v", cycle, drained, err)
		}
	}
	snapshot := r.snapshot()
	if snapshot.UncoveredBinds != 0 || snapshot.ReleaseFailures != 0 {
		t.Fatalf("snapshot %+v", snapshot)
	}
}

func TestC18LaneExhaustionUnderChurnIsAnExplicitGap(t *testing.T) {
	cfg := c14TestConfig(t, 2, 0, 16, 512)
	r := c14Start(t, cfg)
	t.Cleanup(r.shutdown)
	// Two peers hold the two lanes and disconnect; nobody drains.
	for i := 0; i < 2; i++ {
		handle := c14Bind(t, r, c14Node(byte(0x40+i)))
		tx := c14Tx(t, uint64(i), &c14Portal, c14SmallLaunch(byte(i)), false)
		if err := IngressV2(handle, IngressSourceEthBroadcast, []*types.Transaction{tx}, c14Accept); err != nil {
			t.Fatal(err)
		}
		handle.Close()
	}
	// Every further peer is uncovered, and its matching launches are counted.
	for i := 0; i < 20; i++ {
		handle := r.bindPeer(c14Node(byte(0x60 + i)))
		if handle.Covered() {
			t.Fatalf("peer %d took a lane that was not drained", i)
		}
		tx := c14Tx(t, uint64(100+i), &c14Portal, c14SmallLaunch(byte(i)), false)
		if err := IngressV2(handle, IngressSourceEthBroadcast, []*types.Transaction{tx}, c14Accept); err != nil {
			t.Fatal(err)
		}
		handle.Close()
	}
	snapshot := r.snapshot()
	if snapshot.UncoveredBinds != 20 || snapshot.CoveredBinds != 2 {
		t.Fatalf("binds covered=%d uncovered=%d", snapshot.CoveredBinds, snapshot.UncoveredBinds)
	}
	if snapshot.Peers.UncoveredCandidates != 20 || snapshot.GapTotal != 20 {
		t.Fatalf("uncovered candidates %d gap events %d", snapshot.Peers.UncoveredCandidates, snapshot.GapTotal)
	}
}

// Canonical/reorg evidence, peer ingress, txpool evidence and local-geth
// outbound all run at once, each with its own producer and lane.
func TestC18CanonicalAndReorgUnderTransactionLoad(t *testing.T) {
	const peers, packets, heads, buys = 6, 300, 400, 60
	cfg := c14TestConfig(t, peers, 0, 16, 512)
	cfg.OutboundWorkers = 2
	r := c14Start(t, cfg)
	if err := r.install(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(ShutdownIngressV2)
	slow := &c17Peer{id: c14Node(0x91)}
	failing := &c17Peer{id: c14Node(0x92), fail: true}
	if err := AssignOutboundTargetsV2([]OutboundTargetV2{slow.target(), failing.target()}); err != nil {
		t.Fatal(err)
	}
	if !StartOutboundV2() {
		t.Fatal("outbound did not start")
	}

	stop := make(chan struct{})
	var consumers, producers sync.WaitGroup
	var chainRecords, txpoolRecords []*recordV2
	c18Consume(t, c14Consumer(t, cfg, c15ChainLane()), stop, &chainRecords, &consumers)
	c18Consume(t, c14Consumer(t, cfg, c14TxpoolLane(cfg)), stop, &txpoolRecords, &consumers)
	peerSeen := make([][]*recordV2, peers)
	handles := make([]*PeerIngressV2, peers)
	for i := range handles {
		handles[i] = c14Bind(t, r, c14Node(byte(0xa0+i)))
		lane := c14Consumer(t, cfg, c14PeerLane(cfg, handles[i].lease.LaneID))
		c18Consume(t, lane, stop, &peerSeen[i], &consumers)
	}

	var txpoolPublished atomic.Uint64
	for i, handle := range handles {
		producers.Add(1)
		go func(i int, handle *PeerIngressV2) {
			defer producers.Done()
			for n := 0; n < packets; n++ {
				tx := c14Tx(t, uint64(n), &c14Portal, c14SmallLaunch(byte(i)), false)
				_ = IngressV2(handle, IngressSourceEthBroadcast, []*types.Transaction{tx}, c14Accept)
			}
		}(i, handle)
	}
	// The txpool producer: one goroutine, as txBroadcastLoop.
	producers.Add(1)
	go func() {
		defer producers.Done()
		for n := 0; n < packets; n++ {
			tx := c14Tx(t, uint64(10_000+n), &c14OtherAddress, c14SmallLaunch(1), false)
			PublishTxpoolAcceptedV2([]*types.Transaction{tx})
			txpoolPublished.Add(1)
		}
	}()
	// The chain writer: several goroutines, serialized by one lock as by
	// chainmu, publishing heads and reorgs.
	var chainmu sync.Mutex
	for g := 0; g < 4; g++ {
		producers.Add(1)
		go func(g int) {
			defer producers.Done()
			for n := 0; n < heads/4; n++ {
				chainmu.Lock()
				number := int64(g*1000 + n)
				PublishCanonicalV2(c15Header(number, byte(g)), n)
				if n%10 == 9 {
					PublishReorgV2(c15Header(number, byte(g)), c15Header(number+1, byte(g)), c15Header(number-1, byte(g)))
				}
				chainmu.Unlock()
			}
		}(g)
	}
	// HOT0's side of outbound: one producer of the egress lane.
	producers.Add(1)
	go func() {
		defer producers.Done()
		for n := uint64(0); n < buys; n++ {
			c17Egress(t, r, 50_000+n)
			c17Wait(t, "egress drained", func() bool { return c17Snapshot(r).EgressRecords == n+1 })
		}
	}()
	producers.Wait()
	c17Wait(t, "outbound settled", func() bool {
		s := c17Snapshot(r)
		return s.PerWorker[0].HandedOff == buys && s.PerWorker[1].SendErrors == buys
	})
	StopOutboundV2()
	close(stop)
	consumers.Wait()

	chain := r.chain.evidence()
	wantChain := uint64(heads + heads/10)
	if chain.OwnershipViolations != 0 || chain.Lost != 0 || chain.Published != wantChain || uint64(len(chainRecords)) != wantChain {
		t.Fatalf("chain evidence %+v, consumer saw %d", chain, len(chainRecords))
	}
	for n, record := range chainRecords {
		if record.Sequence != uint64(n) || (record.Kind != CanonicalHead && record.Kind != Reorg) {
			t.Fatalf("chain record %d: %+v", n, record)
		}
		if n > 0 && record.TimestampNS < chainRecords[n-1].TimestampNS {
			t.Fatal("chain lane timestamps went backwards under load")
		}
	}
	snapshot := r.snapshot()
	if snapshot.TxpoolPublished+snapshot.TxpoolRefused != txpoolPublished.Load() || uint64(len(txpoolRecords)) != snapshot.TxpoolPublished {
		t.Fatalf("txpool published=%d refused=%d seen=%d", snapshot.TxpoolPublished, snapshot.TxpoolRefused, len(txpoolRecords))
	}
	for i, handle := range handles {
		c := handle.counters()
		if c.Published+c.QueueFull != packets || uint64(len(peerSeen[i])) != c.Published {
			t.Fatalf("peer %d counters %+v, consumer saw %d", i, c, len(peerSeen[i]))
		}
	}
	outbound := c17Snapshot(r)
	if outbound.Dispatched != buys || outbound.InvalidEgress != 0 || outbound.EvidenceLost != 0 {
		t.Fatalf("outbound %+v", outbound)
	}
	// The failing peer never delayed or removed the healthy peer's handoff.
	if got := uint64(len(c17Evidence(t, cfg, 1))); got != buys {
		t.Fatalf("healthy worker evidence %d", got)
	}
	if got := uint64(len(c17Evidence(t, cfg, 2))); got != buys {
		t.Fatalf("failing worker evidence %d", got)
	}
}
