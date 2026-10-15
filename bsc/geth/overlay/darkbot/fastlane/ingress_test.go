package fastlane

import (
	"bufio"
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
)

const (
	c14ScopeGoldenHex     = "444243313450463138000000000000000100e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de001008cb5772c01"
	c14ScopeGoldenCRC32C  = uint32(0x1692c814)
	c14AuxGoldenHex       = "0807060504030201000000000000000000000000000000000000000000000000"
	c14TestKeyHex         = "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318"
	c14InteropEnv         = "DARKBOT_C14_INTEROP_DIR"
	c14InteropGeneration  = uint64(1404)
	c14InteropManifestRel = "c14-interop.manifest"
)

var (
	c14Portal       = common.HexToAddress("0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0")
	c14OtherAddress = common.HexToAddress("0x00000000000000000000000000000000000000aa")
)

func c14Accept() error { return nil }

func c14Node(base byte) [32]byte {
	var node [32]byte
	for i := range node {
		node[i] = base + byte(i)
	}
	return node
}

func c14Fixture(t testing.TB, index int) []byte {
	t.Helper()
	data, err := hex.DecodeString(c14LaunchFixtures[index].Hex)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

// c14BuyCalldata: swapExactInput selector followed by argument words. The
// prefilter looks at the selector only.
func c14BuyCalldata() []byte {
	data := []byte{0xef, 0x7e, 0xc2, 0xe7}
	return append(data, make([]byte, 6*32)...)
}

// c14SmallLaunch: the launch selector and one word, small enough for 512-byte
// slots.
func c14SmallLaunch(tag byte) []byte {
	data := []byte{0x8c, 0xb5, 0x77, 0x2c}
	word := make([]byte, 32)
	word[31] = tag
	return append(data, word...)
}

func c14Tx(t testing.TB, nonce uint64, to *common.Address, data []byte, dynamic bool) *types.Transaction {
	t.Helper()
	key, err := crypto.HexToECDSA(c14TestKeyHex)
	if err != nil {
		t.Fatal(err)
	}
	var inner types.TxData
	if dynamic {
		inner = &types.DynamicFeeTx{
			ChainID:   big.NewInt(56),
			Nonce:     nonce,
			GasTipCap: big.NewInt(1_000_000_000),
			GasFeeCap: big.NewInt(3_000_000_000),
			Gas:       6_000_000,
			To:        to,
			Value:     new(big.Int),
			Data:      data,
		}
	} else {
		inner = &types.LegacyTx{
			Nonce:    nonce,
			GasPrice: big.NewInt(1_000_000_000),
			Gas:      6_000_000,
			To:       to,
			Value:    new(big.Int),
			Data:     data,
		}
	}
	tx, err := types.SignNewTx(key, types.LatestSignerForChainID(big.NewInt(56)), inner)
	if err != nil {
		t.Fatal(err)
	}
	return tx
}

func c14TestConfig(t *testing.T, maxPeers, churn int, peerCapacity, peerStride uint32) ingressConfigV2 {
	t.Helper()
	cfg := defaultIngressConfigV2(t.TempDir(), 14, maxPeers, churn)
	cfg.PeerLaneCapacity = peerCapacity
	cfg.PeerLaneSlotStride = peerStride
	cfg.TxpoolLaneCapacity = 64
	cfg.TxpoolLaneSlotStride = 4096
	return cfg
}

func c14Start(t *testing.T, cfg ingressConfigV2) *ingressRuntimeV2 {
	t.Helper()
	r, err := startIngressRuntimeV2(cfg)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

func c14PeerLane(cfg ingressConfigV2, id uint16) laneConfigV2 {
	return laneConfigV2{ID: id, Direction: v2GethToRust, Class: v2PeerIngress, Capacity: cfg.PeerLaneCapacity, SlotStride: cfg.PeerLaneSlotStride}
}

func c14TxpoolLane(cfg ingressConfigV2) laneConfigV2 {
	return laneConfigV2{ID: c14TxpoolLaneID, Direction: v2GethToRust, Class: v2TxpoolEvidence, Capacity: cfg.TxpoolLaneCapacity, SlotStride: cfg.TxpoolLaneSlotStride}
}

// c14Consumer maps the arena separately, as the Rust process does.
func c14Consumer(t *testing.T, cfg ingressConfigV2, lane laneConfigV2) *laneV2 {
	t.Helper()
	return testOpenLaneV2(t, arenaPathV2(cfg.Dir, cfg.Generation), cfg.Generation, lane)
}

func c14Bind(t *testing.T, r *ingressRuntimeV2, node [32]byte) *PeerIngressV2 {
	t.Helper()
	p := r.bindPeer(node)
	if !p.Covered() {
		t.Fatalf("peer %x not covered: %s", node[:4], p.GapReason())
	}
	return p
}

func c14Decoded(rec *recordV2) uint64 {
	return binary.LittleEndian.Uint64(rec.Aux[0:8])
}

// c14CheckEarlyRecord checks the C1.4 EARLY_TX_SEEN wire semantics.
func c14CheckEarlyRecord(t *testing.T, rec *recordV2, source IngressSourceV2, peer [32]byte, tx *types.Transaction) {
	t.Helper()
	raw, err := tx.MarshalBinary()
	if err != nil {
		t.Fatal(err)
	}
	decoded := c14Decoded(rec)
	switch {
	case rec.Kind != EarlyTxSeen:
		t.Fatalf("kind %d", rec.Kind)
	case rec.ClockID != v2ClockMonotonic:
		t.Fatalf("clock_id %d", rec.ClockID)
	case decoded == 0 || decoded > rec.TimestampNS:
		t.Fatalf("T_packet_decoded %d, T_ingress %d", decoded, rec.TimestampNS)
	case !bytes.Equal(rec.Aux[8:], make([]byte, 24)):
		t.Fatalf("aux[8..32] not zero: %x", rec.Aux[8:])
	case rec.Source != [16]byte(source):
		t.Fatalf("source %q", rec.Source)
	case rec.Peer != peer:
		t.Fatalf("peer %x", rec.Peer[:4])
	case rec.Hash != tx.Hash():
		t.Fatalf("hash %x", rec.Hash)
	case !bytes.Equal(rec.Payload, raw):
		t.Fatal("payload is not the canonical signed transaction")
	}
}

// c14GethDuplicateRule mirrors the duplicate-hash rejection of geth's
// handleTransactions, the validation the hook passes to IngressV2.
func c14GethDuplicateRule(txs []*types.Transaction) func() error {
	return func() error {
		seen := make(map[common.Hash]struct{})
		for _, tx := range txs {
			if _, dup := seen[tx.Hash()]; dup {
				return fmt.Errorf("multiple copies of the same hash %v", tx.Hash())
			}
			seen[tx.Hash()] = struct{}{}
		}
		return nil
	}
}

// ---------------------------------------------------------------------------
// Prefilter scope
// ---------------------------------------------------------------------------

func TestC14ScopeIsTheCommittedFlapScope(t *testing.T) {
	if got := hex.EncodeToString(c14FlapScopeV2.canonical); got != c14ScopeGoldenHex {
		t.Fatalf("canonical scope %s", got)
	}
	if c14FlapScopeV2.fingerprint != c14ScopeGoldenCRC32C {
		t.Fatalf("fingerprint %#x", c14FlapScopeV2.fingerprint)
	}
	ev := c14FlapScopeV2.evidence()
	if ev.ChainID != 56 || len(ev.Addresses) != 1 || ev.Addresses[0] != "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0" ||
		strings.Join(ev.Selectors, ",") != "0x8cb5772c:launch" || ev.Fingerprint != "0x1692c814" ||
		ev.Version != "DBC14PF1" {
		t.Fatalf("scope evidence %+v", ev)
	}
	portal := []common.Address{c14Portal}
	launch := []prefilterSelectorV2{{Selector: [4]byte{0x8c, 0xb5, 0x77, 0x2c}, Role: prefilterRoleLaunch}}
	twice := append(append([]prefilterSelectorV2(nil), launch...), launch...)
	buyRole := []prefilterSelectorV2{{Selector: [4]byte{0xef, 0x7e, 0xc2, 0xe7}, Role: 2}}
	invalid := []struct {
		name      string
		chainID   uint64
		addresses []common.Address
		selectors []prefilterSelectorV2
	}{
		{"no chain", 0, portal, launch},
		{"no address", 56, nil, launch},
		{"no selector", 56, portal, nil},
		{"dup address", 56, []common.Address{c14Portal, c14Portal}, launch},
		{"dup selector", 56, portal, twice},
		{"non-launch role", 56, portal, buyRole},
	}
	for _, tc := range invalid {
		if _, err := newPrefilterScopeV2(tc.chainID, tc.addresses, tc.selectors); err == nil {
			t.Fatalf("%s accepted", tc.name)
		}
	}
}

func TestC14FixturesAreTheCommittedCalldata(t *testing.T) {
	if len(c14LaunchFixtures) != 3 {
		t.Fatalf("%d fixtures", len(c14LaunchFixtures))
	}
	for i, fixture := range c14LaunchFixtures {
		data := c14Fixture(t, i)
		sum := sha256.Sum256(data)
		if hex.EncodeToString(sum[:]) != fixture.SHA256 {
			t.Fatalf("%s differs from the committed fixture", fixture.Name)
		}
		if !bytes.Equal(data[:4], []byte{0x8c, 0xb5, 0x77, 0x2c}) {
			t.Fatalf("%s is not newTokenV6", fixture.Name)
		}
	}
}

func TestC14PrefilterPassesEveryAuthorizedLaunch(t *testing.T) {
	passed := 0
	for i, fixture := range c14LaunchFixtures {
		for _, dynamic := range []bool{false, true} {
			if !c14FlapScopeV2.match(c14Tx(t, uint64(i), &c14Portal, c14Fixture(t, i), dynamic)) {
				t.Fatalf("false negative: %s dynamic=%v", fixture.Name, dynamic)
			}
			passed++
		}
	}
	if passed != 6 {
		t.Fatalf("passed %d", passed)
	}
}

func TestC14PrefilterRejectsNonMatches(t *testing.T) {
	launch := c14Fixture(t, 0)
	for name, tx := range map[string]*types.Transaction{
		"other address":         c14Tx(t, 1, &c14OtherAddress, launch, false),
		"portal, other sel":     c14Tx(t, 2, &c14Portal, []byte{0xde, 0xad, 0xbe, 0xef, 0x00}, true),
		"portal, 3 bytes":       c14Tx(t, 3, &c14Portal, []byte{0x8c, 0xb5, 0x77}, false),
		"portal, no data":       c14Tx(t, 4, &c14Portal, nil, false),
		"contract creation":     c14Tx(t, 5, nil, launch, false),
		"ordinary BUY, legacy":  c14Tx(t, 6, &c14Portal, c14BuyCalldata(), false),
		"ordinary BUY, dynamic": c14Tx(t, 7, &c14Portal, c14BuyCalldata(), true),
	} {
		if c14FlapScopeV2.match(tx) {
			t.Fatalf("%s passed the prefilter", name)
		}
	}
}

func TestC14LaunchPassesAndOrdinaryBuyIsFiltered(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	node := c14Node(0x18)
	p := c14Bind(t, r, node)
	launch := c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)
	buy := c14Tx(t, 1, &c14Portal, c14BuyCalldata(), true)
	if err := IngressV2(p, IngressSourceEthBroadcast, []*types.Transaction{buy, launch}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if c := p.counters(); c.Matched != 1 || c.Filtered != 1 || c.Published != 1 {
		t.Fatalf("counters %+v", c)
	}
	recs := testDrainV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, p.lease.LaneID)))
	if len(recs) != 1 {
		t.Fatalf("%d records", len(recs))
	}
	c14CheckEarlyRecord(t, recs[0], IngressSourceEthBroadcast, node, launch)
}

func TestC14NonMatchesAreDroppedBeforeSerialization(t *testing.T) {
	calls := 0
	saved := serializeTxV2
	serializeTxV2 = func(tx *types.Transaction) ([]byte, error) {
		calls++
		return tx.MarshalBinary()
	}
	defer func() { serializeTxV2 = saved }()

	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	node := c14Node(0x10)
	p := c14Bind(t, r, node)
	m0 := c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)
	m1 := c14Tx(t, 3, &c14Portal, c14SmallLaunch(3), true)
	txs := []*types.Transaction{
		m0,
		c14Tx(t, 1, &c14OtherAddress, c14SmallLaunch(1), false),
		c14Tx(t, 2, &c14Portal, []byte{1, 2, 3, 4, 5}, false),
		m1,
	}
	if err := IngressV2(p, IngressSourceEthBroadcast, txs, c14Accept); err != nil {
		t.Fatal(err)
	}
	if calls != 2 {
		t.Fatalf("serialized %d transactions, want only the 2 matches", calls)
	}
	c := p.counters()
	if c.Packets != 1 || c.Matched != 2 || c.Filtered != 2 || c.Published != 2 {
		t.Fatalf("counters %+v", c)
	}
	recs := testDrainV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, p.lease.LaneID)))
	if len(recs) != 2 || recs[0].Hash != m0.Hash() || recs[1].Hash != m1.Hash() {
		t.Fatalf("records %d", len(recs))
	}
}

// ---------------------------------------------------------------------------
// Boundary
// ---------------------------------------------------------------------------

func TestC14DecodedIsCapturedBeforeAndIngressAfterValidation(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	node := c14Node(0x20)
	p := c14Bind(t, r, node)
	tx := c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)
	var validationStart, validationEnd uint64
	validate := func() error {
		validationStart, _ = MonotonicNS()
		time.Sleep(3 * time.Millisecond)
		validationEnd, _ = MonotonicNS()
		return nil
	}
	if err := IngressV2(p, IngressSourceEthBroadcast, []*types.Transaction{tx}, validate); err != nil {
		t.Fatal(err)
	}
	recs := testDrainV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, p.lease.LaneID)))
	if len(recs) != 1 {
		t.Fatalf("%d records", len(recs))
	}
	rec := recs[0]
	c14CheckEarlyRecord(t, rec, IngressSourceEthBroadcast, node, tx)
	decoded, ingress := c14Decoded(rec), rec.TimestampNS
	if !(decoded <= validationStart && validationEnd <= ingress) {
		t.Fatalf("order decoded=%d validation=[%d,%d] ingress=%d", decoded, validationStart, validationEnd, ingress)
	}
	if ingress-decoded < uint64(3*time.Millisecond) {
		t.Fatalf("T_ingress - T_packet_decoded = %d ns does not contain the validation", ingress-decoded)
	}
	if rec.LaneEpoch != p.lease.Epoch {
		t.Fatalf("record epoch %d, lease epoch %d", rec.LaneEpoch, p.lease.Epoch)
	}
}

func TestC14AuxGoldenEncoding(t *testing.T) {
	aux := ingressAuxV2(0x0102030405060708)
	if hex.EncodeToString(aux[:]) != c14AuxGoldenHex {
		t.Fatalf("aux %x", aux)
	}
}

func TestC14RejectedPacketsPublishNothing(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	p := c14Bind(t, r, c14Node(0x30))
	consumer := c14Consumer(t, cfg, c14PeerLane(cfg, p.lease.LaneID))
	match := c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)

	// The validation's error is returned unchanged; nothing is published.
	reject := errors.New("rejected by validation")
	if err := IngressV2(p, IngressSourceEthBroadcast, []*types.Transaction{match}, func() error { return reject }); err != reject {
		t.Fatalf("validation error changed: %v", err)
	}
	// A real invalid packet: two copies of one transaction, which geth's
	// handleTransactions refuses.
	dup := []*types.Transaction{match, match}
	if err := IngressV2(p, IngressSourceEthPooled, dup, c14GethDuplicateRule(dup)); err == nil ||
		!strings.Contains(err.Error(), "multiple copies of the same hash") {
		t.Fatalf("duplicate packet accepted: %v", err)
	}
	if s := consumer.stats(); s.WriteSequence != 0 {
		t.Fatalf("a rejected packet was published: write=%d", s.WriteSequence)
	}
	if c := p.counters(); c.Packets != 2 || c.RejectedPackets != 2 || c.Matched != 0 || c.Published != 0 {
		t.Fatalf("counters %+v", c)
	}
	// Without a Fast-Lane handle the boundary is the validation alone.
	if err := IngressV2(nil, IngressSourceEthBroadcast, dup, c14GethDuplicateRule(dup)); err == nil {
		t.Fatal("validation skipped without a handle")
	}
	if err := IngressV2(nil, IngressSourceEthBroadcast, []*types.Transaction{match}, c14Accept); err != nil {
		t.Fatal(err)
	}
}

func TestC14TransactionsAndPooledShareBoundarySemantics(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 4096)
	r := c14Start(t, cfg)
	node := c14Node(0x40)
	p := c14Bind(t, r, node)
	broadcast := c14Tx(t, 0, &c14Portal, c14Fixture(t, 0), false)
	pooled := c14Tx(t, 1, &c14Portal, c14Fixture(t, 1), true)
	if err := IngressV2(p, IngressSourceEthBroadcast, []*types.Transaction{broadcast}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if err := IngressV2(p, IngressSourceEthPooled, []*types.Transaction{pooled}, c14Accept); err != nil {
		t.Fatal(err)
	}
	recs := testDrainV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, p.lease.LaneID)))
	if len(recs) != 2 {
		t.Fatalf("%d records", len(recs))
	}
	c14CheckEarlyRecord(t, recs[0], IngressSourceEthBroadcast, node, broadcast)
	c14CheckEarlyRecord(t, recs[1], IngressSourceEthPooled, node, pooled)
	if recs[1].TimestampNS < recs[0].TimestampNS {
		t.Fatal("later packet has an earlier T_ingress")
	}
}

// ---------------------------------------------------------------------------
// Peer lanes
// ---------------------------------------------------------------------------

func TestC14ParallelPeersPublishIntoSeparateLanes(t *testing.T) {
	const peers, packets = 6, 200
	cfg := c14TestConfig(t, peers, 0, 512, 512)
	r := c14Start(t, cfg)
	handles := make([]*PeerIngressV2, peers)
	txs := make([][]*types.Transaction, peers)
	for i := range handles {
		handles[i] = c14Bind(t, r, c14Node(byte(0x50+i)))
		for n := 0; n < packets; n++ {
			txs[i] = append(txs[i], c14Tx(t, uint64(n), &c14Portal, c14SmallLaunch(byte(i)), n%2 == 0))
		}
	}
	lanes := make(map[uint16]bool)
	for _, p := range handles {
		if lanes[p.lease.LaneID] {
			t.Fatalf("lane %d bound twice", p.lease.LaneID)
		}
		lanes[p.lease.LaneID] = true
	}
	var wg sync.WaitGroup
	start := make(chan struct{})
	for i, p := range handles {
		wg.Add(1)
		go func(i int, p *PeerIngressV2) {
			defer wg.Done()
			<-start
			for n := 0; n < packets; n++ {
				if err := IngressV2(p, IngressSourceEthBroadcast, txs[i][n:n+1], c14Accept); err != nil {
					t.Error(err)
					return
				}
			}
		}(i, p)
	}
	close(start)
	wg.Wait()
	for i, p := range handles {
		recs := testDrainV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, p.lease.LaneID)))
		if len(recs) != packets {
			t.Fatalf("peer %d: %d records", i, len(recs))
		}
		for n, rec := range recs {
			var tx types.Transaction
			if err := tx.UnmarshalBinary(rec.Payload); err != nil {
				t.Fatal(err)
			}
			if rec.Peer != c14Node(byte(0x50+i)) || rec.Sequence != uint64(n) || tx.Nonce() != uint64(n) ||
				rec.Hash != txs[i][n].Hash() || rec.LaneEpoch != p.lease.Epoch {
				t.Fatalf("peer %d record %d belongs elsewhere", i, n)
			}
		}
		if c := p.counters(); c.Published != packets || c.QueueFull != 0 || c.Refused != 0 {
			t.Fatalf("peer %d counters %+v", i, c)
		}
	}
}

func TestC14DisconnectDrainReleaseReconnect(t *testing.T) {
	cfg := c14TestConfig(t, 1, 1, 8, 512)
	r := c14Start(t, cfg)
	arena := r.rt.arena
	nodeA := c14Node(0x60)
	a := c14Bind(t, r, nodeA)
	laneA, epochA := a.lease.LaneID, a.lease.Epoch
	if err := IngressV2(a, IngressSourceEthBroadcast, []*types.Transaction{c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)}, c14Accept); err != nil {
		t.Fatal(err)
	}
	a.Close()
	a.Close() // idempotent
	if s := testStatsV2(t, arena, laneA); s.State != v2LaneStateClosing || s.FinalWriteSequence != 1 {
		t.Fatalf("lane after disconnect %+v", s)
	}

	// The unread record keeps A's lane out of the pool; B gets the other lane.
	b := c14Bind(t, r, c14Node(0x61))
	if b.lease.LaneID == laneA {
		t.Fatal("an undrained lane was reused")
	}
	b.Close()
	testAckV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, b.lease.LaneID)))
	c := c14Bind(t, r, c14Node(0x62))
	if c.lease.LaneID != b.lease.LaneID || c.lease.Epoch != b.lease.Epoch+1 {
		t.Fatalf("drained lane not reused: lease %+v after %+v", c.lease, b.lease)
	}
	if s := r.snapshot(); s.Reclaimed != 1 || s.PendingRelease != 1 {
		t.Fatalf("reclaimed=%d pending=%d", s.Reclaimed, s.PendingRelease)
	}

	// Rust drains A's lane, then A reconnects with the same node ID.
	consumerA := c14Consumer(t, cfg, c14PeerLane(cfg, laneA))
	if recs := testDrainV2(t, consumerA); len(recs) != 1 || recs[0].LaneEpoch != epochA {
		t.Fatalf("drain of A: %d records", len(recs))
	}
	testAckV2(t, consumerA)
	c.Close()
	testAckV2(t, c14Consumer(t, cfg, c14PeerLane(cfg, c.lease.LaneID)))
	a2 := c14Bind(t, r, nodeA)
	if a2.lease.LaneID != laneA || a2.lease.Epoch != epochA+1 {
		t.Fatalf("reconnect lease %+v", a2.lease)
	}

	// Neither the closed handle nor a forged handle of the old binding
	// writes into the new binding.
	stale := c14Tx(t, 1, &c14Portal, c14SmallLaunch(1), false)
	if err := IngressV2(a, IngressSourceEthBroadcast, []*types.Transaction{stale}, c14Accept); err != nil {
		t.Fatal(err)
	}
	forged := &PeerIngressV2{runtime: r, owner: nodeA, lane: a.lane, lease: a.lease}
	if err := IngressV2(forged, IngressSourceEthBroadcast, []*types.Transaction{stale}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if a.counters().Refused != 1 || forged.counters().Refused != 1 || forged.counters().Published != 0 {
		t.Fatalf("stale handles: closed=%+v forged=%+v", a.counters(), forged.counters())
	}
	if s := testStatsV2(t, arena, laneA); s.WriteSequence != 1 {
		t.Fatalf("a stale handle wrote: write=%d", s.WriteSequence)
	}
	fresh := c14Tx(t, 2, &c14Portal, c14SmallLaunch(2), true)
	if err := IngressV2(a2, IngressSourceEthPooled, []*types.Transaction{fresh}, c14Accept); err != nil {
		t.Fatal(err)
	}
	recs := testDrainV2(t, consumerA)
	if len(recs) != 1 || recs[0].Hash != fresh.Hash() || recs[0].LaneEpoch != epochA+1 || recs[0].Sequence != 1 {
		t.Fatalf("new binding records %d", len(recs))
	}
}

func TestC14LaneExhaustionIsAnExplicitGap(t *testing.T) {
	cfg := c14TestConfig(t, 2, 0, 8, 512)
	r := c14Start(t, cfg)
	a := c14Bind(t, r, c14Node(0x70))
	c14Bind(t, r, c14Node(0x71))
	if err := IngressV2(a, IngressSourceEthBroadcast, []*types.Transaction{c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)}, c14Accept); err != nil {
		t.Fatal(err)
	}
	a.Close() // CLOSING with an unread record: not reusable

	nodeC := c14Node(0x72)
	c := r.bindPeer(nodeC)
	if c.Covered() || c.GapReason() != "pool_exhausted" {
		t.Fatalf("exhausted pool bound a peer: covered=%v reason=%q", c.Covered(), c.GapReason())
	}
	// The uncovered peer keeps normal geth processing; its candidates are
	// counted as missed coverage, never published.
	txs := []*types.Transaction{
		c14Tx(t, 1, &c14Portal, c14SmallLaunch(1), false),
		c14Tx(t, 2, &c14OtherAddress, c14SmallLaunch(2), false),
	}
	if err := IngressV2(c, IngressSourceEthBroadcast, txs, c14Accept); err != nil {
		t.Fatal(err)
	}
	if cc := c.counters(); cc.UncoveredCandidates != 1 || cc.Published != 0 || cc.Filtered != 1 {
		t.Fatalf("uncovered counters %+v", cc)
	}
	s := r.snapshot()
	if s.UncoveredBinds != 1 || s.GapTotal != 1 || len(s.Gaps) != 1 || s.Gaps[0].Reason != "pool_exhausted" ||
		s.Gaps[0].Peer != hex.EncodeToString(nodeC[:]) || s.LiveUncovered != 1 || s.Peers.UncoveredCandidates != 1 ||
		s.Pool.Exhausted != 1 {
		t.Fatalf("gap evidence %+v", s)
	}
	encoded, err := json.Marshal(s)
	if err != nil || !bytes.Contains(encoded, []byte(`"reason":"pool_exhausted"`)) {
		t.Fatalf("evidence json: %v", err)
	}

	// The peer limit is a gap too.
	cfgLimit := c14TestConfig(t, 1, 1, 8, 512)
	limited := c14Start(t, cfgLimit)
	c14Bind(t, limited, c14Node(0x73))
	if p := limited.bindPeer(c14Node(0x74)); p.Covered() || p.GapReason() != "peer_limit" {
		t.Fatalf("peer limit: covered=%v reason=%q", p.Covered(), p.GapReason())
	}
}

func TestC14StartupBoundsAreEnforced(t *testing.T) {
	dir := t.TempDir()
	for _, cfg := range []ingressConfigV2{
		defaultIngressConfigV2(dir, 1, 200, 57),
		defaultIngressConfigV2(dir, 1, 0, 2),
		defaultIngressConfigV2(dir, 0, 4, 1),
		defaultIngressConfigV2(dir, 1, 4, -1),
	} {
		if _, err := startIngressRuntimeV2(cfg); err == nil {
			t.Fatalf("config accepted: %+v", cfg)
		}
	}
	if entries, _ := os.ReadDir(dir); len(entries) != 0 {
		t.Fatalf("a rejected config created %d files", len(entries))
	}
	atBound := defaultIngressConfigV2(dir, 1, 255, 1)
	atBound.PeerLaneCapacity, atBound.PeerLaneSlotStride = 4, 512
	atBound.TxpoolLaneCapacity, atBound.TxpoolLaneSlotStride = 4, 512
	r, err := startIngressRuntimeV2(atBound)
	if err != nil {
		t.Fatalf("bound itself rejected: %v", err)
	}
	if s := r.snapshot(); s.Pool.PeerLanes != c14PeerLanePoolBound || s.PoolBound != c14PeerLanePoolBound {
		t.Fatalf("pool %+v", s.Pool)
	}
}

func TestC14InitFromEnvAndPeerCapacity(t *testing.T) {
	ShutdownIngressV2()
	t.Cleanup(ShutdownIngressV2)
	t.Setenv(envIngressV2Dir, "")
	if err := InitIngressV2FromEnv(); err != nil || IngressV2Enabled() {
		t.Fatalf("disabled init: err=%v enabled=%v", err, IngressV2Enabled())
	}
	if BindPeerIngressV2(c14Node(1)) != nil || CheckIngressV2PeerCapacity(1000) != nil {
		t.Fatal("disabled runtime acted")
	}
	t.Setenv(envIngressV2Dir, t.TempDir())
	t.Setenv(envIngressV2Generation, "")
	if err := InitIngressV2FromEnv(); err == nil {
		t.Fatal("missing generation accepted")
	}
	t.Setenv(envIngressV2Generation, "9")
	t.Setenv(envIngressV2MaxPeers, "300")
	t.Setenv(envIngressV2ChurnReserve, "0")
	if err := InitIngressV2FromEnv(); err == nil {
		t.Fatal("peer bound above the pool accepted")
	}
	t.Setenv(envIngressV2MaxPeers, "4")
	t.Setenv(envIngressV2ChurnReserve, "2")
	if err := InitIngressV2FromEnv(); err != nil || !IngressV2Enabled() {
		t.Fatalf("init: %v", err)
	}
	if err := InitIngressV2FromEnv(); err == nil {
		t.Fatal("second init accepted")
	}
	if CheckIngressV2PeerCapacity(5) == nil || CheckIngressV2PeerCapacity(4) != nil {
		t.Fatal("peer capacity check")
	}
	if p := BindPeerIngressV2(c14Node(2)); !p.Covered() {
		t.Fatalf("bind: %s", p.GapReason())
	}
	snapshot, ok := IngressV2Snapshot()
	if !ok || snapshot.MaxPeers != 4 || snapshot.ChurnReserve != 2 || snapshot.PeerLaneCapacity != c14PeerLaneCapacity ||
		snapshot.Scope.Fingerprint != "0x1692c814" {
		t.Fatalf("snapshot %+v", snapshot)
	}
	ShutdownIngressV2()
	if IngressV2Enabled() {
		t.Fatal("still enabled after shutdown")
	}
}

func TestC19bTestnetScopeIsExplicitAndFailClosed(t *testing.T) {
	ShutdownIngressV2()
	t.Cleanup(ShutdownIngressV2)

	// No opt-in: production Mainnet scope remains unchanged.
	t.Setenv(envIngressV2Scope, "")
	scope, err := ingressScopeFromEnvV2()
	if err != nil || scope != c14FlapScopeV2 {
		t.Fatalf("default scope: scope=%p err=%v", scope, err)
	}

	mainnet := scope.evidence()
	if mainnet.ChainID != 56 ||
		len(mainnet.Addresses) != 1 ||
		mainnet.Addresses[0] != "0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0" {
		t.Fatalf("default Mainnet scope changed: %+v", mainnet)
	}

	// Qualification requires explicit opt-in.
	t.Setenv(envIngressV2Scope, ingressScopeC19bTestnet)
	scope, err = ingressScopeFromEnvV2()
	if err != nil || scope != c19bTestnetFlapScopeV2 {
		t.Fatalf("C1.9b scope: scope=%p err=%v", scope, err)
	}

	testnet := scope.evidence()
	if testnet.ChainID != 97 ||
		len(testnet.Addresses) != 1 ||
		testnet.Addresses[0] != "0x5beacaf7abcbb3ab280e80d007fd31fce26510e9" ||
		strings.Join(testnet.Selectors, ",") != "0x8cb5772c:launch" {
		t.Fatalf("C1.9b Testnet scope evidence: %+v", testnet)
	}

	// Prove env selection reaches the actual runtime.
	t.Setenv(envIngressV2Dir, t.TempDir())
	t.Setenv(envIngressV2Generation, "1909")
	t.Setenv(envIngressV2MaxPeers, "2")
	t.Setenv(envIngressV2ChurnReserve, "1")
	t.Setenv(envIngressV2Scope, ingressScopeC19bTestnet)

	if err := InitIngressV2FromEnv(); err != nil {
		t.Fatalf("C1.9b runtime init: %v", err)
	}

	snapshot, ok := IngressV2Snapshot()
	if !ok ||
		snapshot.Scope.ChainID != 97 ||
		len(snapshot.Scope.Addresses) != 1 ||
		snapshot.Scope.Addresses[0] != "0x5beacaf7abcbb3ab280e80d007fd31fce26510e9" {
		t.Fatalf("C1.9b runtime snapshot: %+v", snapshot)
	}

	ShutdownIngressV2()

	// Unknown scope values fail closed.
	t.Setenv(envIngressV2Scope, "invalid")
	if _, err := ingressScopeFromEnvV2(); err == nil {
		t.Fatal("invalid scope accepted")
	}
}

func TestC14QueueFullAndOversizeAreVisible(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 4, 512)
	r := c14Start(t, cfg)
	p := c14Bind(t, r, c14Node(0x80))
	var txs []*types.Transaction
	for n := 0; n < 6; n++ {
		txs = append(txs, c14Tx(t, uint64(n), &c14Portal, c14SmallLaunch(byte(n)), false))
	}
	if err := IngressV2(p, IngressSourceEthBroadcast, txs, c14Accept); err != nil {
		t.Fatal(err)
	}
	oversized := c14Tx(t, 7, &c14Portal, c14Fixture(t, 0), false)
	if err := IngressV2(p, IngressSourceEthBroadcast, []*types.Transaction{oversized}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if c := p.counters(); c.Published != 4 || c.QueueFull != 2 || c.Oversize != 1 {
		t.Fatalf("counters %+v", c)
	}
	consumer := c14Consumer(t, cfg, c14PeerLane(cfg, p.lease.LaneID))
	if s := consumer.stats(); s.DroppedFull != 2 || s.Oversize != 1 {
		t.Fatalf("lane counters %+v", s)
	}
	recs := testDrainV2(t, consumer)
	for n, rec := range recs {
		if rec.Hash != txs[n].Hash() {
			t.Fatalf("record %d overwritten", n)
		}
	}
	if len(recs) != 4 {
		t.Fatalf("%d records", len(recs))
	}
}

func TestC14ShutdownWithActivePeers(t *testing.T) {
	ShutdownIngressV2()
	t.Cleanup(ShutdownIngressV2)
	cfg := c14TestConfig(t, 3, 0, 1024, 512)
	r := c14Start(t, cfg)
	if err := r.install(); err != nil {
		t.Fatal(err)
	}
	var handles []*PeerIngressV2
	for i := 0; i < 3; i++ {
		p := BindPeerIngressV2(c14Node(byte(0x90 + i)))
		if !p.Covered() {
			t.Fatal(p.GapReason())
		}
		handles = append(handles, p)
	}
	tx := c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)
	var wg sync.WaitGroup
	deadline := time.Now().Add(10 * time.Second)
	for _, p := range handles {
		wg.Add(1)
		go func(p *PeerIngressV2) {
			defer wg.Done()
			for p.counters().Refused == 0 {
				if err := IngressV2(p, IngressSourceEthBroadcast, []*types.Transaction{tx}, c14Accept); err != nil {
					t.Error(err)
					return
				}
				if time.Now().After(deadline) {
					t.Error("shutdown never observed")
					return
				}
			}
		}(p)
	}
	time.Sleep(5 * time.Millisecond)
	ShutdownIngressV2()
	wg.Wait()
	if IngressV2Enabled() || BindPeerIngressV2(c14Node(0x99)) != nil {
		t.Fatal("runtime active after shutdown")
	}
	if string(r.rt.arena.data[v2SHMagic:v2SHMagic+8]) != v2Magic {
		t.Fatal("mapping no longer readable")
	}
	if late := r.bindPeer(c14Node(0x9a)); late.Covered() || late.GapReason() != "shutdown" {
		t.Fatalf("bind after shutdown: %q", late.GapReason())
	}
	for _, p := range handles {
		p.Close()
		if s := testStatsV2(t, r.rt.arena, p.lease.LaneID); s.State != v2LaneStateClosing {
			t.Fatalf("lane %d after close: %+v", p.lease.LaneID, s)
		}
	}
}

func TestC14TxpoolEvidenceIsIndependentOfThePrefilter(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	p := c14Bind(t, r, c14Node(0xa0))
	match := c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)
	other := c14Tx(t, 1, &c14OtherAddress, c14SmallLaunch(1), true)
	r.publishTxpool([]*types.Transaction{match, other})
	recs := testDrainV2(t, c14Consumer(t, cfg, c14TxpoolLane(cfg)))
	if len(recs) != 2 {
		t.Fatalf("%d txpool records", len(recs))
	}
	for i, tx := range []*types.Transaction{match, other} {
		raw, _ := tx.MarshalBinary()
		rec := recs[i]
		if rec.Kind != TxpoolAccepted || rec.Source != [16]byte(ingressSourceTxpool) || rec.Peer != ([32]byte{}) ||
			rec.Hash != tx.Hash() || !bytes.Equal(rec.Payload, raw) || rec.ClockID != v2ClockMonotonic || rec.Aux != ([32]byte{}) {
			t.Fatalf("txpool record %d", i)
		}
	}
	if s := testStatsV2(t, r.rt.arena, p.lease.LaneID); s.WriteSequence != 0 {
		t.Fatal("txpool evidence reached a peer lane")
	}
	if s := r.snapshot(); s.TxpoolPublished != 2 || s.TxpoolRefused != 0 {
		t.Fatalf("txpool counters %+v", s)
	}
}

// A transaction the txpool producer cannot serialize or publish leaves no
// TXPOOL_ACCEPTED record; only the producer counters show it, per transaction.
func TestC14TxpoolProducerFailuresAreCounted(t *testing.T) {
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	good := c14Tx(t, 0, &c14Portal, c14SmallLaunch(0), false)
	bad := c14Tx(t, 1, &c14Portal, c14SmallLaunch(1), true)
	saved := serializeTxV2
	serializeTxV2 = func(tx *types.Transaction) ([]byte, error) {
		if tx.Hash() == bad.Hash() {
			return nil, errors.New("serialize failure")
		}
		return saved(tx)
	}
	defer func() { serializeTxV2 = saved }()
	r.publishTxpool([]*types.Transaction{good, bad})
	// After runtime shutdown every otherwise publishable txpool record is
	// refused. c14Start does not install the runtime globally, so this test
	// shuts down its own handle; ShutdownIngressV2 is covered by
	// TestC14ShutdownWithActivePeers.
	r.shutdown()
	r.publishTxpool([]*types.Transaction{bad, good})
	s := r.snapshot()
	if s.TxpoolPublished != 1 || s.TxpoolSerializeErrs != 2 || s.TxpoolRefused != 1 || s.TxpoolClockErrors != 0 {
		t.Fatalf("txpool counters %+v", s)
	}
	consumer := c14Consumer(t, cfg, c14TxpoolLane(cfg))
	record, ok, err := consumer.tryConsume()
	if err != nil || !ok || record == nil || record.Hash != good.Hash() {
		t.Fatalf("first txpool record: ok=%v err=%v record=%+v", ok, err, record)
	}
	record, ok, err = consumer.tryConsume()
	if !errors.Is(err, errV2Shutdown) || ok || record != nil {
		t.Fatalf("txpool lane end after shutdown: ok=%v err=%v record=%+v", ok, err, record)
	}
}

func TestC14RoleLeaseAndGenerationRulesHold(t *testing.T) {
	cfg := c14TestConfig(t, 2, 1, 8, 512)
	c14Start(t, cfg)
	if _, err := startIngressRuntimeV2(cfg); !errors.Is(err, errV2RoleLeaseHeld) {
		t.Fatalf("second geth-owner in one dir: %v", err)
	}
	other := cfg
	other.Dir = t.TempDir()
	existing, err := createArenaV2(arenaPathV2(other.Dir, other.Generation), other.Generation, other.lanePlan())
	if err != nil {
		t.Fatal(err)
	}
	_ = existing.close()
	var exists *arenaExistsErrorV2
	if _, err := startIngressRuntimeV2(other); !errors.As(err, &exists) || exists.Probe != arenaCurrentV2 {
		t.Fatalf("existing arena: %v", err)
	}
	lease, err := acquireRoleLeaseV2(other.Dir, v2RoleGethOwner, other.Generation)
	if err != nil {
		t.Fatalf("failed start kept the role: %v", err)
	}
	_ = lease.release()
}

// ---------------------------------------------------------------------------
// C1.4 Go<->Rust ingress interop, run by bsc/native/tools/c14_ingress_interop.sh
// ---------------------------------------------------------------------------

func c14InteropConfig(dir string) ingressConfigV2 {
	cfg := defaultIngressConfigV2(dir, c14InteropGeneration, 2, 1)
	cfg.PeerLaneCapacity, cfg.PeerLaneSlotStride = 8, 4096
	cfg.TxpoolLaneCapacity, cfg.TxpoolLaneSlotStride = 16, 4096
	return cfg
}

func c14InteropDir(t *testing.T) string {
	t.Helper()
	dir := os.Getenv(c14InteropEnv)
	if dir == "" {
		t.Skip("C1.4 Go<->Rust interop phase; run by bsc/native/tools/c14_ingress_interop.sh")
	}
	return dir
}

func c14InteropTxs(t *testing.T) map[string]*types.Transaction {
	return map[string]*types.Transaction{
		"launch1":  c14Tx(t, 0, &c14Portal, c14Fixture(t, 0), false),
		"nonmatch": c14Tx(t, 1, &c14OtherAddress, c14Fixture(t, 0), false),
		"buy":      c14Tx(t, 2, &c14Portal, c14BuyCalldata(), true),
		"launch2":  c14Tx(t, 3, &c14Portal, c14Fixture(t, 1), true),
		"launch3":  c14Tx(t, 4, &c14Portal, c14Fixture(t, 2), false),
		"late":     c14Tx(t, 5, &c14Portal, c14Fixture(t, 0), true),
	}
}

func c14WriteManifest(t *testing.T, dir string, lines []string, appendMode bool) {
	t.Helper()
	flags := os.O_CREATE | os.O_WRONLY | os.O_TRUNC
	if appendMode {
		flags = os.O_WRONLY | os.O_APPEND
	}
	f, err := os.OpenFile(filepath.Join(dir, c14InteropManifestRel), flags, 0o600)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	for _, line := range lines {
		if _, err := fmt.Fprintln(f, line); err != nil {
			t.Fatal(err)
		}
	}
}

func c14ReadManifest(t *testing.T, dir string) map[string]string {
	t.Helper()
	f, err := os.Open(filepath.Join(dir, c14InteropManifestRel))
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	out := make(map[string]string)
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		key, value, ok := strings.Cut(scanner.Text(), "=")
		if ok {
			out[key] = value
		}
	}
	return out
}

// Phase 1: two peers publish through the boundary; a rejected packet, a
// transaction to another address and an ordinary BUY publish nothing; the
// txpool lane receives every transaction. Two planted false negatives for the
// Rust audit: launch3, which no peer lane carries, and late, which a peer
// lane carries only after the txpool accepted it. Peer A disconnects with an
// unread record.
func TestC14InteropGethPhase1(t *testing.T) {
	dir := c14InteropDir(t)
	cfg := c14InteropConfig(dir)
	r := c14Start(t, cfg)
	nodeA, nodeB := c14Node(0x10), c14Node(0x40)
	a, b := c14Bind(t, r, nodeA), c14Bind(t, r, nodeB)
	txs := c14InteropTxs(t)
	if err := IngressV2(a, IngressSourceEthBroadcast, []*types.Transaction{txs["launch1"], txs["nonmatch"], txs["buy"]}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if err := IngressV2(b, IngressSourceEthPooled, []*types.Transaction{txs["launch2"]}, c14Accept); err != nil {
		t.Fatal(err)
	}
	reject := errors.New("rejected by validation")
	if err := IngressV2(a, IngressSourceEthBroadcast, []*types.Transaction{txs["launch3"]}, func() error { return reject }); err != reject {
		t.Fatal(err)
	}
	r.publishTxpool([]*types.Transaction{txs["launch1"], txs["nonmatch"], txs["buy"], txs["launch2"], txs["launch3"], txs["late"]})
	// Observed on a peer lane only after the txpool accepted it.
	if err := IngressV2(b, IngressSourceEthPooled, []*types.Transaction{txs["late"]}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if a.counters().Published != 1 || b.counters().Published != 2 {
		t.Fatalf("published a=%d b=%d", a.counters().Published, b.counters().Published)
	}
	a.Close()
	// Txpool producer counters after the last txpool publish.
	snapshot := r.snapshot()
	lines := []string{
		"generation=" + strconv.FormatUint(cfg.Generation, 10),
		"peer_lanes=" + strconv.Itoa(cfg.MaxPeers+cfg.ChurnReserve),
		"peer_lane_capacity=" + strconv.FormatUint(uint64(cfg.PeerLaneCapacity), 10),
		"peer_lane_slot_stride=" + strconv.FormatUint(uint64(cfg.PeerLaneSlotStride), 10),
		"txpool_lane=" + strconv.FormatUint(uint64(c14TxpoolLaneID), 10),
		"txpool_lane_capacity=" + strconv.FormatUint(uint64(cfg.TxpoolLaneCapacity), 10),
		"txpool_lane_slot_stride=" + strconv.FormatUint(uint64(cfg.TxpoolLaneSlotStride), 10),
		"node_a=" + hex.EncodeToString(nodeA[:]),
		"node_b=" + hex.EncodeToString(nodeB[:]),
		"lane_a=" + strconv.FormatUint(uint64(a.lease.LaneID), 10),
		"lane_b=" + strconv.FormatUint(uint64(b.lease.LaneID), 10),
		"epoch_a=" + strconv.FormatUint(a.lease.Epoch, 10),
		"txpool_published=" + strconv.FormatUint(snapshot.TxpoolPublished, 10),
		"txpool_refused=" + strconv.FormatUint(snapshot.TxpoolRefused, 10),
		"txpool_serialize_errors=" + strconv.FormatUint(snapshot.TxpoolSerializeErrs, 10),
		"txpool_clock_errors=" + strconv.FormatUint(snapshot.TxpoolClockErrors, 10),
	}
	for _, name := range []string{"launch1", "nonmatch", "buy", "launch2", "launch3", "late"} {
		lines = append(lines, "hash_"+name+"="+hex.EncodeToString(txs[name].Hash().Bytes()))
	}
	c14WriteManifest(t, dir, lines, false)
	fmt.Println("C14_INTEROP_GETH_PHASE1_OK")
}

// Phase 2: after Rust drained and acknowledged A's lane, release it; a new
// peer C binds it with epoch+1; a stale handle of A is refused; C publishes.
func TestC14InteropGethPhase2(t *testing.T) {
	dir := c14InteropDir(t)
	cfg := c14InteropConfig(dir)
	m := c14ReadManifest(t, dir)
	arena, err := openArenaV2(arenaPathV2(dir, cfg.Generation), cfg.Generation)
	if err != nil {
		t.Fatal(err)
	}
	defer arena.close()
	laneA64, _ := strconv.ParseUint(m["lane_a"], 10, 16)
	epochA, _ := strconv.ParseUint(m["epoch_a"], 10, 64)
	laneA := uint16(laneA64)
	nodeA, nodeC := c14Node(0x10), c14Node(0x70)
	if err := arena.releasePeerLane(laneA, nodeA, epochA); err != nil {
		t.Fatalf("release after the Rust drain: %v", err)
	}
	pool, err := newPeerLanePoolV2(arena, peerLanePoolBoundsV2{MaxPeers: cfg.MaxPeers, ChurnReserve: cfg.ChurnReserve})
	if err != nil {
		t.Fatal(err)
	}
	leaseC, err := pool.allocate(nodeC)
	if err != nil || leaseC.LaneID != laneA || leaseC.Epoch != epochA+1 {
		t.Fatalf("rebind: %+v %v", leaseC, err)
	}
	lane, err := arena.laneByID(laneA)
	if err != nil {
		t.Fatal(err)
	}
	scopeOnly := &ingressRuntimeV2{scope: c14FlapScopeV2}
	launch3 := c14InteropTxs(t)["launch3"]
	if hex.EncodeToString(launch3.Hash().Bytes()) != m["hash_launch3"] {
		t.Fatal("launch3 is not reproducible")
	}
	stale := &PeerIngressV2{runtime: scopeOnly, owner: nodeA, lane: lane, lease: peerLaneLeaseV2{LaneID: laneA, Epoch: epochA, Owner: nodeA}}
	if err := IngressV2(stale, IngressSourceEthBroadcast, []*types.Transaction{launch3}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if stale.counters().Refused != 1 || lane.stats().WriteSequence != 1 {
		t.Fatalf("stale handle wrote: %+v", stale.counters())
	}
	c := &PeerIngressV2{runtime: scopeOnly, owner: nodeC, lane: lane, lease: leaseC}
	if err := IngressV2(c, IngressSourceEthPooled, []*types.Transaction{launch3}, c14Accept); err != nil {
		t.Fatal(err)
	}
	if c.counters().Published != 1 {
		t.Fatalf("C counters %+v", c.counters())
	}
	c14WriteManifest(t, dir, []string{
		"node_c=" + hex.EncodeToString(nodeC[:]),
		"epoch_c=" + strconv.FormatUint(leaseC.Epoch, 10),
	}, true)
	fmt.Println("C14_INTEROP_GETH_PHASE2_OK")
}

func TestC19bUnsyncedAcceptTxsQualificationGate(t *testing.T) {
	tests := []struct {
		name      string
		scope     string
		arm       string
		networkID uint64
		want      bool
		wantErr   bool
	}{
		{
			name:      "production_default_does_not_override",
			scope:     "",
			arm:       "",
			networkID: 56,
		},
		{
			name:      "testnet_scope_without_arm_does_not_override",
			scope:     ingressScopeC19bTestnet,
			arm:       "",
			networkID: 97,
		},
		{
			name:      "explicit_testnet_qualification_is_allowed",
			scope:     ingressScopeC19bTestnet,
			arm:       "YES",
			networkID: 97,
			want:      true,
		},
		{
			name:      "mainnet_scope_with_arm_fails_closed",
			scope:     ingressScopeMainnet,
			arm:       "YES",
			networkID: 56,
			wantErr:   true,
		},
		{
			name:      "testnet_scope_on_wrong_network_fails_closed",
			scope:     ingressScopeC19bTestnet,
			arm:       "YES",
			networkID: 56,
			wantErr:   true,
		},
		{
			name:      "invalid_arm_fails_closed",
			scope:     ingressScopeC19bTestnet,
			arm:       "yes",
			networkID: 97,
			wantErr:   true,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Setenv(envIngressV2Scope, tc.scope)
			t.Setenv(envC19bAcceptTxsUnsynced, tc.arm)

			got, err := C19bUnsyncedAcceptTxsAllowed(tc.networkID)

			if (err != nil) != tc.wantErr {
				t.Fatalf("error=%v wantErr=%v", err, tc.wantErr)
			}
			if got != tc.want {
				t.Fatalf("allowed=%v want=%v", got, tc.want)
			}
		})
	}
}
