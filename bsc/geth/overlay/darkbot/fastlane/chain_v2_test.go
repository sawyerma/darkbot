package fastlane

import (
	"encoding/binary"
	"math/big"
	"sync"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

func c15ChainLane() laneConfigV2 {
	return laneConfigV2{
		ID:         c15ChainLaneID,
		Direction:  v2GethToRust,
		Class:      v2ChainEvidence,
		Capacity:   c15ChainLaneCapacity,
		SlotStride: c15ChainLaneSlotStride,
	}
}

func c15Header(number int64, parent byte) *types.Header {
	return &types.Header{
		Number:     big.NewInt(number),
		ParentHash: common.Hash{parent},
		Difficulty: big.NewInt(2),
	}
}

// c15Installed starts a runtime and installs it as the process's Fast Lane,
// as newHandler does, so the Publish*V2 hooks reach it.
func c15Installed(t *testing.T) (*ingressRuntimeV2, ingressConfigV2) {
	t.Helper()
	cfg := c14TestConfig(t, 1, 0, 8, 512)
	r := c14Start(t, cfg)
	if err := r.install(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(ShutdownIngressV2)
	return r, cfg
}

func TestC15ChainRecordsKeepTheirFieldSemantics(t *testing.T) {
	r, cfg := c15Installed(t)
	head := c15Header(77, 0x11)
	PublishCanonicalV2(head, 3)
	oldHead, newHead, ancestor := c15Header(10, 0x21), c15Header(11, 0x22), c15Header(8, 0x23)
	PublishReorgV2(oldHead, newHead, ancestor)
	PublishCanonicalV2(nil, 0)
	PublishReorgV2(oldHead, nil, ancestor)

	recs := testDrainV2(t, c14Consumer(t, cfg, c15ChainLane()))
	if len(recs) != 2 {
		t.Fatalf("%d chain records", len(recs))
	}
	canonical := recs[0]
	if canonical.Kind != CanonicalHead || canonical.Hash != head.Hash() || canonical.Aux != [32]byte(head.ParentHash) ||
		canonical.Source != [16]byte(chainSourceCanonicalV2) || canonical.ClockID != v2ClockMonotonic {
		t.Fatalf("canonical record %+v", canonical)
	}
	if len(canonical.Payload) != 12 || binary.LittleEndian.Uint64(canonical.Payload[0:8]) != 77 ||
		binary.LittleEndian.Uint32(canonical.Payload[8:12]) != 3 {
		t.Fatalf("canonical payload %x", canonical.Payload)
	}
	reorg := recs[1]
	oldHash, newHash, commonHash := oldHead.Hash(), newHead.Hash(), ancestor.Hash()
	if reorg.Kind != Reorg || reorg.Hash != newHash || reorg.Aux != [32]byte(commonHash) ||
		reorg.Source != [16]byte(chainSourceReorgV2) || len(reorg.Payload) != 88 {
		t.Fatalf("reorg record %+v", reorg)
	}
	payload := reorg.Payload
	if binary.LittleEndian.Uint64(payload[0:8]) != 10 || string(payload[8:40]) != string(oldHash[:]) ||
		binary.LittleEndian.Uint64(payload[40:48]) != 11 || string(payload[48:80]) != string(newHash[:]) ||
		binary.LittleEndian.Uint64(payload[80:88]) != 8 {
		t.Fatalf("reorg payload %x", payload)
	}
	if recs[1].TimestampNS < recs[0].TimestampNS {
		t.Fatal("chain lane timestamps went backwards")
	}
	evidence, ok := ChainV2Snapshot()
	if !ok || evidence.Published != 2 || evidence.Lost != 0 || evidence.OwnershipViolations != 0 || evidence.LaneID != c15ChainLaneID {
		t.Fatalf("chain evidence %+v ok=%v", evidence, ok)
	}
	if r.chain.evidence() != evidence {
		t.Fatal("snapshot differs from the runtime's chain evidence")
	}
}

func TestC18SecondChainPublisherIsDetectedAndReportedInBand(t *testing.T) {
	r, cfg := c15Installed(t)
	PublishCanonicalV2(c15Header(1, 0x01), 0)
	// Another publish is in progress: the next two calls are a second
	// producer. They write nothing.
	r.chain.inside.Store(1)
	PublishCanonicalV2(c15Header(2, 0x02), 0)
	PublishReorgV2(c15Header(2, 0x02), c15Header(3, 0x03), c15Header(1, 0x01))
	r.chain.inside.Store(0)
	evidence := r.chain.evidence()
	if evidence.OwnershipViolations != 2 || evidence.Published != 1 || evidence.Lost != 2 || evidence.PendingLost != 2 {
		t.Fatalf("after violation %+v", evidence)
	}
	// The next publish of the owner reports the loss before its record.
	PublishCanonicalV2(c15Header(4, 0x04), 0)
	recs := testDrainV2(t, c14Consumer(t, cfg, c15ChainLane()))
	if len(recs) != 3 {
		t.Fatalf("%d chain records", len(recs))
	}
	gap := recs[1]
	if recs[0].Kind != CanonicalHead || gap.Kind != Gap || recs[2].Kind != CanonicalHead ||
		gap.Source != [16]byte(chainSourceGapV2) || binary.LittleEndian.Uint64(gap.Aux[0:8]) != 2 {
		t.Fatalf("records %+v %+v %+v", recs[0], gap, recs[2])
	}
	evidence = r.chain.evidence()
	if evidence.Gaps != 1 || evidence.PendingLost != 0 || evidence.Published != 2 {
		t.Fatalf("after gap %+v", evidence)
	}
}

func TestC18FullChainLaneLosesNothingSilently(t *testing.T) {
	r, cfg := c15Installed(t)
	capacity := int64(c15ChainLaneCapacity)
	for number := int64(0); number < capacity+5; number++ {
		PublishCanonicalV2(c15Header(number, 0x01), 0)
	}
	evidence := r.chain.evidence()
	if evidence.Published != uint64(capacity) || evidence.Lost != 5 || evidence.PendingLost != 5 || evidence.DroppedFull < 5 {
		t.Fatalf("full lane %+v", evidence)
	}
	consumer := c14Consumer(t, cfg, c15ChainLane())
	first := testDrainV2(t, consumer)
	if len(first) != int(capacity) {
		t.Fatalf("%d records before the gap", len(first))
	}
	// With room again, the gap precedes the next head.
	PublishCanonicalV2(c15Header(capacity+5, 0x01), 0)
	rest := testDrainV2(t, consumer)
	if len(rest) != 2 || rest[0].Kind != Gap || binary.LittleEndian.Uint64(rest[0].Aux[0:8]) != 5 || rest[1].Kind != CanonicalHead ||
		binary.LittleEndian.Uint64(rest[0].Aux[8:16]) != 0 || len(rest[0].Payload) != 0 {
		t.Fatalf("after room: %+v", rest)
	}
	// No overwrite: the first record is still the first head.
	if binary.LittleEndian.Uint64(first[0].Payload[0:8]) != 0 || first[0].Sequence != 0 {
		t.Fatalf("first record %+v", first[0])
	}
}

// Under one lock, as under chainmu, concurrent callers are one producer.
func TestC18ChainPublishersUnderOneLockAreASingleProducer(t *testing.T) {
	r, cfg := c15Installed(t)
	var chainmu sync.Mutex
	const goroutines, each = 8, 100
	var wg sync.WaitGroup
	for g := 0; g < goroutines; g++ {
		wg.Add(1)
		go func(g int) {
			defer wg.Done()
			for i := 0; i < each; i++ {
				chainmu.Lock()
				PublishCanonicalV2(c15Header(int64(g*each+i), byte(g)), i)
				chainmu.Unlock()
			}
		}(g)
	}
	wg.Wait()
	evidence := r.chain.evidence()
	if evidence.OwnershipViolations != 0 || evidence.Lost != 0 || evidence.Published != goroutines*each {
		t.Fatalf("locked publishers %+v", evidence)
	}
	recs := testDrainV2(t, c14Consumer(t, cfg, c15ChainLane()))
	if len(recs) != goroutines*each {
		t.Fatalf("%d records", len(recs))
	}
	for i, rec := range recs {
		if rec.Sequence != uint64(i) || rec.Kind != CanonicalHead {
			t.Fatalf("record %d: %+v", i, rec)
		}
		if i > 0 && rec.TimestampNS < recs[i-1].TimestampNS {
			t.Fatal("timestamps went backwards under one lock")
		}
	}
}

// Without the lock the single-producer claim is false. The detector must
// keep the lane intact and account for every call: published + lost equals
// the calls made, every loss is reported by a GAP or still pending, and no
// record is torn.
func TestC18UnlockedChainPublishersNeverCorruptTheLane(t *testing.T) {
	r, cfg := c15Installed(t)
	const goroutines, each = 8, 100
	var wg sync.WaitGroup
	for g := 0; g < goroutines; g++ {
		wg.Add(1)
		go func(g int) {
			defer wg.Done()
			for i := 0; i < each; i++ {
				PublishCanonicalV2(c15Header(int64(g*each+i), byte(g)), i)
			}
		}(g)
	}
	wg.Wait()
	evidence := r.chain.evidence()
	if evidence.Published+evidence.Lost != goroutines*each {
		t.Fatalf("unaccounted calls %+v", evidence)
	}
	if evidence.Lost != evidence.OwnershipViolations {
		t.Fatalf("losses that are not ownership violations %+v", evidence)
	}
	// Every record validates (tryConsume checks sequence, epoch and CRC32C).
	recs := testDrainV2(t, c14Consumer(t, cfg, c15ChainLane()))
	heads, reported := uint64(0), uint64(0)
	for i, rec := range recs {
		if rec.Sequence != uint64(i) {
			t.Fatalf("record %d has sequence %d", i, rec.Sequence)
		}
		switch rec.Kind {
		case CanonicalHead:
			heads++
		case Gap:
			reported += binary.LittleEndian.Uint64(rec.Aux[0:8])
		default:
			t.Fatalf("unexpected kind %d", rec.Kind)
		}
	}
	if heads != evidence.Published || reported+evidence.PendingLost != evidence.Lost {
		t.Fatalf("heads=%d reported=%d evidence %+v", heads, reported, evidence)
	}
}

func TestE72HeadResetsArePublishedAsReasonedGaps(t *testing.T) {
	_, cfg := c15Installed(t)
	oldHead, newHead := c15Header(90, 0x31), c15Header(40, 0x32)
	PublishCanonicalV2(oldHead, 1)
	PublishHeadResetV2(ChainHeadResetSetHeadV2, oldHead, newHead)
	PublishHeadResetV2(ChainHeadResetSnapCommitV2, newHead, oldHead)
	PublishHeadResetV2(ChainHeadResetGenesisResetV2, oldHead, c15Header(0, 0))

	recs := testDrainV2(t, c14Consumer(t, cfg, c15ChainLane()))
	if len(recs) != 4 || recs[0].Kind != CanonicalHead {
		t.Fatalf("%d chain records", len(recs))
	}
	for i, reason := range []uint64{ChainHeadResetSetHeadV2, ChainHeadResetSnapCommitV2, ChainHeadResetGenesisResetV2} {
		gap := recs[i+1]
		if gap.Kind != Gap || gap.Source != [16]byte(chainSourceHeadResetV2) || gap.Hash != (common.Hash{}) ||
			binary.LittleEndian.Uint64(gap.Aux[0:8]) != 0 || binary.LittleEndian.Uint64(gap.Aux[8:16]) != reason ||
			len(gap.Payload) != 80 {
			t.Fatalf("head reset %d record %+v", reason, gap)
		}
	}
	first := recs[1].Payload
	oldHash, newHash := oldHead.Hash(), newHead.Hash()
	if binary.LittleEndian.Uint64(first[0:8]) != 90 || string(first[8:40]) != string(oldHash[:]) ||
		binary.LittleEndian.Uint64(first[40:48]) != 40 || string(first[48:80]) != string(newHash[:]) {
		t.Fatalf("set head payload %x", first)
	}
	if evidence, ok := ChainV2Snapshot(); !ok || evidence.Published != 4 || evidence.Lost != 0 {
		t.Fatalf("chain evidence %+v", evidence)
	}
}
