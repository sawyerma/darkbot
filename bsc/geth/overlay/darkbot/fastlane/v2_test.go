package fastlane

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"hash/crc32"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
)

func testArenaV2Path(t *testing.T) string {
	t.Helper()
	return filepath.Join(t.TempDir(), "fastlane-v2.mmap")
}

func testPeerLaneV2(id uint16) laneConfigV2 {
	return laneConfigV2{
		ID:         id,
		Direction:  v2GethToRust,
		Class:      v2PeerIngress,
		Capacity:   4,
		SlotStride: 512,
	}
}

func testRecordV2(payload []byte) recordV2 {
	var source [16]byte
	var peer [32]byte
	var aux [32]byte
	for i := range source {
		source[i] = 0x11
	}
	for i := range peer {
		peer[i] = 0x22
		aux[i] = 0x44
	}
	return recordV2{
		Kind:        EarlyTxSeen,
		Flags:       3,
		TimestampNS: 123456789,
		ClockID:     v2ClockMonotonic,
		Source:      source,
		Peer:        peer,
		Hash:        common.BytesToHash(bytesRepeatV2(0x33, 32)),
		Aux:         aux,
		Payload:     payload,
	}
}

func bytesRepeatV2(value byte, count int) []byte {
	out := make([]byte, count)
	for i := range out {
		out[i] = value
	}
	return out
}

func TestV2CRC32CGoldenVectors(t *testing.T) {
	if got := crc32.Checksum([]byte("123456789"), v2CRC32CTable); got != 0xe3069283 {
		t.Fatalf("standard CRC32C = %#x", got)
	}

	slot := make([]byte, 512)
	r := testRecordV2([]byte("signed-transaction"))
	binary.LittleEndian.PutUint16(slot[v2SlotKind:], uint16(r.Kind))
	binary.LittleEndian.PutUint16(slot[v2SlotFlags:], r.Flags)
	binary.LittleEndian.PutUint32(slot[v2SlotPayload:], uint32(len(r.Payload)))
	binary.LittleEndian.PutUint64(slot[v2SlotTimestamp:], r.TimestampNS)
	binary.LittleEndian.PutUint32(slot[v2SlotClockID:], r.ClockID)
	copy(slot[v2SlotSource:v2SlotSource+16], r.Source[:])
	copy(slot[v2SlotPeer:v2SlotPeer+32], r.Peer[:])
	copy(slot[v2SlotHash:v2SlotHash+32], r.Hash[:])
	copy(slot[v2SlotAux:v2SlotAux+32], r.Aux[:])
	binary.LittleEndian.PutUint64(slot[v2SlotEpoch:], 7)
	copy(slot[v2SlotHeaderSize:], r.Payload)

	if got := recordCRC32CV2(slot, len(r.Payload)); got != 0xde714cce {
		t.Fatalf("cross-language CRC32C = %#x", got)
	}
}

func TestV2RoundTripWrapAndDirectory(t *testing.T) {
	path := testArenaV2Path(t)
	lanes := []laneConfigV2{testPeerLaneV2(10), testPeerLaneV2(11)}
	created, err := createArenaV2(path, 7, lanes)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := created.bindPeerLane(10, [32]byte{0x10}); err != nil {
		t.Fatal(err)
	}
	if len(created.data)%v2Alignment != 0 {
		t.Fatalf("arena len %d not aligned", len(created.data))
	}
	if err := created.close(); err != nil {
		t.Fatal(err)
	}

	producerArena, err := openArenaV2(path, 7)
	if err != nil {
		t.Fatal(err)
	}
	defer producerArena.close()
	consumerArena, err := openArenaV2(path, 7)
	if err != nil {
		t.Fatal(err)
	}
	defer consumerArena.close()

	producer, err := producerArena.lane(lanes[0])
	if err != nil {
		t.Fatal(err)
	}
	consumer, err := consumerArena.lane(lanes[0])
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 32; i++ {
		if err := producer.publish(testRecordV2([]byte{byte(i)})); err != nil {
			t.Fatalf("publish %d: %v", i, err)
		}
		record, ok, err := consumer.tryConsume()
		if err != nil || !ok {
			t.Fatalf("consume %d: ok=%v err=%v", i, ok, err)
		}
		if record.Sequence != uint64(i) || record.LaneID != 10 || record.LaneEpoch != 2 {
			t.Fatalf("bad identity at %d: %+v", i, record)
		}
		if len(record.Payload) != 1 || record.Payload[0] != byte(i) {
			t.Fatalf("payload mismatch at %d", i)
		}
	}

	wrong := lanes[0]
	wrong.Capacity = 8
	if _, err := producerArena.lane(wrong); err == nil {
		t.Fatal("lane config mismatch accepted")
	}
}

func TestV2FullNeverOverwritesUnread(t *testing.T) {
	path := testArenaV2Path(t)
	laneCfg := testPeerLaneV2(1)
	created, err := createArenaV2(path, 9, []laneConfigV2{laneCfg})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := created.bindPeerLane(1, [32]byte{0x01}); err != nil {
		t.Fatal(err)
	}
	created.close()

	pa, _ := openArenaV2(path, 9)
	defer pa.close()
	ca, _ := openArenaV2(path, 9)
	defer ca.close()
	p, _ := pa.lane(laneCfg)
	c, _ := ca.lane(laneCfg)

	for i := 0; i < int(laneCfg.Capacity); i++ {
		if err := p.publish(testRecordV2([]byte{byte(i)})); err != nil {
			t.Fatal(err)
		}
	}
	if err := p.publish(testRecordV2([]byte{9})); err == nil {
		t.Fatal("expected full lane failure")
	}
	if got := atomic.LoadUint64(p.atomic(v2LHDroppedFull)); got != 1 {
		t.Fatalf("dropped_full=%d", got)
	}
	for i := 0; i < int(laneCfg.Capacity); i++ {
		r, ok, err := c.tryConsume()
		if err != nil || !ok {
			t.Fatalf("consume %d: ok=%v err=%v", i, ok, err)
		}
		if len(r.Payload) != 1 || r.Payload[0] != byte(i) {
			t.Fatalf("unread record overwritten at %d", i)
		}
	}
}

func TestV2HeaderAndPayloadCorruptionDetected(t *testing.T) {
	for _, target := range []string{"header", "payload"} {
		t.Run(target, func(t *testing.T) {
			path := testArenaV2Path(t)
			cfg := testPeerLaneV2(2)
			created, err := createArenaV2(path, 11, []laneConfigV2{cfg})
			if err != nil {
				t.Fatal(err)
			}
			if _, err := created.bindPeerLane(2, [32]byte{0x02}); err != nil {
				t.Fatal(err)
			}
			created.close()

			pa, _ := openArenaV2(path, 11)
			p, _ := pa.lane(cfg)
			if err := p.publish(testRecordV2([]byte("abc"))); err != nil {
				t.Fatal(err)
			}
			pa.close()

			mut, err := openArenaV2(path, 11)
			if err != nil {
				t.Fatal(err)
			}
			lane, _ := mut.lane(cfg)
			slot := lane.slot(0)
			if target == "header" {
				slot[v2SlotHash] ^= 0x01
			} else {
				slot[v2SlotHeaderSize] ^= 0x01
			}
			mut.close()

			ca, _ := openArenaV2(path, 11)
			defer ca.close()
			c, _ := ca.lane(cfg)
			if _, _, err := c.tryConsume(); !errors.Is(err, errV2Corrupt) {
				t.Fatalf("corruption accepted: %v", err)
			}
			if _, _, err := c.tryConsume(); !errors.Is(err, errV2Quarantined) {
				t.Fatalf("second poll: %v", err)
			}
			if s := c.stats(); s.CorruptRecords != 1 || s.ReadSequence != 0 || s.State != v2LaneStateQuarantined {
				t.Fatalf("after corruption: %+v", s)
			}
		})
	}
}

func TestV2IndependentLanesHaveIndependentProducers(t *testing.T) {
	path := testArenaV2Path(t)
	lanes := []laneConfigV2{testPeerLaneV2(20), testPeerLaneV2(21)}
	created, err := createArenaV2(path, 13, lanes)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := created.bindPeerLane(20, [32]byte{0x20}); err != nil {
		t.Fatal(err)
	}
	if _, err := created.bindPeerLane(21, [32]byte{0x21}); err != nil {
		t.Fatal(err)
	}
	created.close()

	var wg sync.WaitGroup
	wg.Add(2)
	for laneIndex := range lanes {
		laneIndex := laneIndex
		go func() {
			defer wg.Done()
			a, err := openArenaV2(path, 13)
			if err != nil {
				t.Error(err)
				return
			}
			defer a.close()
			p, err := a.lane(lanes[laneIndex])
			if err != nil {
				t.Error(err)
				return
			}
			for n := 0; n < 100; n++ {
				for {
					err := p.publish(testRecordV2([]byte{byte(n)}))
					if err == nil {
						break
					}
					if err.Error() != "fastlane v2 queue full" {
						t.Errorf("publish: %v", err)
						return
					}
					runtime.Gosched()
				}
			}
		}()
	}

	consumers := make([]*laneV2, 2)
	consumerArenas := make([]*arenaV2, 2)
	for i := range lanes {
		a, err := openArenaV2(path, 13)
		if err != nil {
			t.Fatal(err)
		}
		consumerArenas[i] = a
		c, err := a.lane(lanes[i])
		if err != nil {
			t.Fatal(err)
		}
		consumers[i] = c
		defer a.close()
	}

	got := [2]int{}
	for got[0] < 100 || got[1] < 100 {
		for i := range consumers {
			if got[i] >= 100 {
				continue
			}
			_, ok, err := consumers[i].tryConsume()
			if err != nil {
				t.Fatal(err)
			}
			if ok {
				got[i]++
			}
		}
		runtime.Gosched()
	}
	wg.Wait()
	if got != [2]int{100, 100} {
		t.Fatalf("got=%v", got)
	}
}

func TestV2CreateNewRefusesStaleArena(t *testing.T) {
	path := testArenaV2Path(t)
	cfg := testPeerLaneV2(1)
	first, err := createArenaV2(path, 17, []laneConfigV2{cfg})
	if err != nil {
		t.Fatal(err)
	}
	defer first.close()
	if _, err := createArenaV2(path, 18, []laneConfigV2{cfg}); err == nil {
		t.Fatal("stale arena silently reused")
	}
}

func TestV2FileIsOwnerOnly(t *testing.T) {
	path := testArenaV2Path(t)
	cfg := testPeerLaneV2(1)
	a, err := createArenaV2(path, 19, []laneConfigV2{cfg})
	if err != nil {
		t.Fatal(err)
	}
	defer a.close()
	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if info.Mode().Perm() != 0o600 {
		t.Fatalf("mode=%#o", info.Mode().Perm())
	}
}

func TestV2PeerLaneLifecycleRequiresDrainBeforeReuse(t *testing.T) {
	path := testArenaV2Path(t)
	cfg := testPeerLaneV2(30)
	arena, err := createArenaV2(path, 21, []laneConfigV2{cfg})
	if err != nil {
		t.Fatal(err)
	}
	defer arena.close()

	ownerA := [32]byte{0xa1}
	ownerB := [32]byte{0xb2}

	lane, err := arena.lane(cfg)
	if err != nil {
		t.Fatal(err)
	}
	if state := atomic.LoadUint32(v2AtomicU32(arena.data, lane.offset+v2LHState)); state != v2LaneStateFree {
		t.Fatalf("initial state=%d", state)
	}
	epochA, err := arena.bindPeerLane(30, ownerA)
	if err != nil {
		t.Fatal(err)
	}
	if epochA != 2 {
		t.Fatalf("epochA=%d", epochA)
	}

	producerArena, _ := openArenaV2(path, 21)
	defer producerArena.close()
	producer, _ := producerArena.lane(cfg)
	consumerArena, _ := openArenaV2(path, 21)
	defer consumerArena.close()
	consumer, _ := consumerArena.lane(cfg)

	if err := producer.publish(testRecordV2([]byte("one"))); err != nil {
		t.Fatal(err)
	}
	finalWrite, err := arena.beginClosePeerLane(30, ownerA, epochA)
	if err != nil {
		t.Fatal(err)
	}
	if finalWrite != 1 {
		t.Fatalf("finalWrite=%d", finalWrite)
	}
	if err := arena.releasePeerLane(30, ownerA, epochA); err == nil {
		t.Fatal("released before drain")
	}
	if ok, err := consumer.acknowledgeDrainedIfClosing(); ok || err != nil {
		t.Fatalf("acknowledged before consuming final record: ok=%v err=%v", ok, err)
	}
	if _, ok, err := consumer.tryConsume(); err != nil || !ok {
		t.Fatalf("consume ok=%v err=%v", ok, err)
	}
	if ok, err := consumer.acknowledgeDrainedIfClosing(); !ok || err != nil {
		t.Fatalf("drain was not acknowledged: ok=%v err=%v", ok, err)
	}
	if err := arena.releasePeerLane(30, ownerA, epochA); err != nil {
		t.Fatal(err)
	}

	if state := atomic.LoadUint32(v2AtomicU32(arena.data, lane.offset+v2LHState)); state != v2LaneStateFree {
		t.Fatalf("released state=%d", state)
	}
	if got := arena.data[lane.offset+v2LHOwner : lane.offset+v2LHOwner+32]; string(got) != string(make([]byte, 32)) {
		t.Fatal("owner not cleared")
	}
	epochB, err := arena.bindPeerLane(30, ownerB)
	if err != nil {
		t.Fatal(err)
	}
	if epochB != 3 {
		t.Fatalf("epochB=%d", epochB)
	}
}

func TestV2PeerLaneLifecycleRejectsWrongOwnerAndFreePublish(t *testing.T) {
	path := testArenaV2Path(t)
	cfg := testPeerLaneV2(31)
	arena, err := createArenaV2(path, 22, []laneConfigV2{cfg})
	if err != nil {
		t.Fatal(err)
	}
	defer arena.close()

	freeArena, _ := openArenaV2(path, 22)
	freeLane, _ := freeArena.lane(cfg)
	if err := freeLane.publish(testRecordV2([]byte("x"))); err == nil {
		t.Fatal("FREE peer lane accepted publish")
	}
	freeArena.close()

	owner := [32]byte{0x11}
	wrong := [32]byte{0x22}
	epoch, err := arena.bindPeerLane(31, owner)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := arena.bindPeerLane(31, owner); err == nil {
		t.Fatal("double bind accepted")
	}
	if _, err := arena.beginClosePeerLane(31, wrong, epoch); err == nil {
		t.Fatal("wrong owner closed lane")
	}
	if _, err := arena.beginClosePeerLane(31, owner, epoch); err != nil {
		t.Fatal(err)
	}

	consumerArena, _ := openArenaV2(path, 22)
	defer consumerArena.close()
	consumer, _ := consumerArena.lane(cfg)
	if ok, err := consumer.acknowledgeDrainedIfClosing(); !ok || err != nil {
		t.Fatalf("empty closing lane did not acknowledge drain: ok=%v err=%v", ok, err)
	}
	if err := arena.releasePeerLane(31, wrong, epoch); err == nil {
		t.Fatal("wrong owner released lane")
	}
	if err := arena.releasePeerLane(31, owner, epoch); err != nil {
		t.Fatal(err)
	}
}

// ---------------------------------------------------------------------------
// C1.3 lane lifecycle
// ---------------------------------------------------------------------------

func testOwnerV2(tag byte) [32]byte {
	var owner [32]byte
	for i := range owner {
		owner[i] = tag
	}
	return owner
}

func testTxpoolLaneV2(id uint16) laneConfigV2 {
	return laneConfigV2{
		ID:         id,
		Direction:  v2GethToRust,
		Class:      v2TxpoolEvidence,
		Capacity:   4,
		SlotStride: 512,
	}
}

func testCreateArenaV2(t *testing.T, generation uint64, lanes ...laneConfigV2) (*arenaV2, string) {
	t.Helper()
	path := testArenaV2Path(t)
	arena, err := createArenaV2(path, generation, lanes)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = arena.close() })
	return arena, path
}

// testOpenLaneV2 maps the arena separately, as another process would.
func testOpenLaneV2(t *testing.T, path string, generation uint64, cfg laneConfigV2) *laneV2 {
	t.Helper()
	arena, err := openArenaV2(path, generation)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = arena.close() })
	lane, err := arena.lane(cfg)
	if err != nil {
		t.Fatal(err)
	}
	return lane
}

func testStatsV2(t *testing.T, arena *arenaV2, id uint16) laneStatsV2 {
	t.Helper()
	stats, err := arena.laneStats(id)
	if err != nil {
		t.Fatal(err)
	}
	return stats
}

func testDrainV2(t *testing.T, lane *laneV2) []*recordV2 {
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

func testStateErrorV2(err error) (*laneStateErrorV2, bool) {
	var stateErr *laneStateErrorV2
	ok := errors.As(err, &stateErr)
	return stateErr, ok
}

func testExpectStateV2(t *testing.T, err error, expected, observed uint32) {
	t.Helper()
	stateErr, ok := testStateErrorV2(err)
	if !ok || stateErr.Expected != expected || stateErr.Observed != observed {
		t.Fatalf("want lifecycle state %d != %d, got %v", observed, expected, err)
	}
}

func testAckV2(t *testing.T, lane *laneV2) {
	t.Helper()
	if ok, err := lane.acknowledgeDrainedIfClosing(); !ok || err != nil {
		t.Fatalf("drain not acknowledged: ok=%v err=%v", ok, err)
	}
}

func TestV2LifecycleFullCycleAndSecondBind(t *testing.T) {
	cfg := testPeerLaneV2(40)
	arena, path := testCreateArenaV2(t, 41, cfg)
	if s := testStatsV2(t, arena, 40); s.State != v2LaneStateFree || s.LaneEpoch != 1 || s.Owner != ([32]byte{}) {
		t.Fatalf("initial %+v", s)
	}
	ownerA, ownerB := testOwnerV2(0xa1), testOwnerV2(0xb2)

	// FREE -> BOUND
	epochA, err := arena.bindPeerLane(40, ownerA)
	if err != nil {
		t.Fatal(err)
	}
	if s := testStatsV2(t, arena, 40); epochA != 2 || s.State != v2LaneStateBound || s.LaneEpoch != 2 || s.Owner != ownerA {
		t.Fatalf("bound epoch=%d %+v", epochA, s)
	}
	producer := testOpenLaneV2(t, path, 41, cfg)
	consumer := testOpenLaneV2(t, path, 41, cfg)
	for _, payload := range []string{"a0", "a1"} {
		if err := producer.publish(testRecordV2([]byte(payload))); err != nil {
			t.Fatal(err)
		}
	}

	// BOUND -> CLOSING
	finalWrite, err := arena.beginClosePeerLane(40, ownerA, epochA)
	if err != nil {
		t.Fatal(err)
	}
	if s := testStatsV2(t, arena, 40); finalWrite != 2 || s.State != v2LaneStateClosing || s.FinalWriteSequence != 2 {
		t.Fatalf("closing final=%d %+v", finalWrite, s)
	}

	// CLOSING -> drained
	drained := testDrainV2(t, consumer)
	if len(drained) != 2 {
		t.Fatalf("drained %d records", len(drained))
	}
	for _, record := range drained {
		if record.LaneEpoch != epochA {
			t.Fatalf("record epoch %d != %d", record.LaneEpoch, epochA)
		}
	}
	testAckV2(t, consumer)
	if s := testStatsV2(t, arena, 40); s.DrainedEpoch != epochA {
		t.Fatalf("drained epoch %d", s.DrainedEpoch)
	}

	// drained -> FREE
	if err := arena.releasePeerLane(40, ownerA, epochA); err != nil {
		t.Fatal(err)
	}
	s := testStatsV2(t, arena, 40)
	if s.State != v2LaneStateFree || s.Owner != ([32]byte{}) || s.FinalWriteSequence != 0 ||
		s.WriteSequence != 2 || s.ReadSequence != 2 {
		t.Fatalf("released %+v", s)
	}

	// FREE -> second BOUND: the epoch advances, sequences continue.
	epochB, err := arena.bindPeerLane(40, ownerB)
	if err != nil || epochB != epochA+1 {
		t.Fatalf("second bind epoch=%d err=%v", epochB, err)
	}
	if err := producer.publish(testRecordV2([]byte("b0"))); err != nil {
		t.Fatal(err)
	}
	record, ok, err := consumer.tryConsume()
	if err != nil || !ok || record.Sequence != 2 || record.LaneEpoch != epochB || string(record.Payload) != "b0" {
		t.Fatalf("after reuse ok=%v err=%v record=%+v", ok, err, record)
	}
}

func TestV2PublishWhileFreeOrClosingIsRejected(t *testing.T) {
	cfg := testPeerLaneV2(41)
	arena, path := testCreateArenaV2(t, 42, cfg)
	producer := testOpenLaneV2(t, path, 42, cfg)

	testExpectStateV2(t, producer.publish(testRecordV2([]byte("x"))), v2LaneStateBound, v2LaneStateFree)
	if s := producer.stats(); s.WriteSequence != 0 || s.DroppedFull != 0 || s.Oversize != 0 {
		t.Fatalf("FREE publish changed the lane: %+v", s)
	}
	if committed := atomic.LoadUint64(v2AtomicU64(producer.slot(0), v2SlotCommitted)); committed != v2EmptySequence {
		t.Fatalf("FREE publish wrote slot: committed=%d", committed)
	}

	epoch, err := arena.bindPeerLane(41, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	if err := producer.publish(testRecordV2([]byte("last"))); err != nil {
		t.Fatal(err)
	}
	finalWrite, err := arena.beginClosePeerLane(41, testOwnerV2(1), epoch)
	if err != nil {
		t.Fatal(err)
	}
	testExpectStateV2(t, producer.publish(testRecordV2([]byte("late"))), v2LaneStateBound, v2LaneStateClosing)
	if got := producer.stats().WriteSequence; got != finalWrite {
		t.Fatalf("write_sequence %d != final_write %d", got, finalWrite)
	}
}

func TestV2DoubleBindRejectedWithoutSideEffects(t *testing.T) {
	arena, _ := testCreateArenaV2(t, 44, testPeerLaneV2(43))
	epoch, err := arena.bindPeerLane(43, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	_, err = arena.bindPeerLane(43, testOwnerV2(2))
	testExpectStateV2(t, err, v2LaneStateFree, v2LaneStateBound)
	if s := testStatsV2(t, arena, 43); s.State != v2LaneStateBound || s.LaneEpoch != epoch || s.Owner != testOwnerV2(1) {
		t.Fatalf("double bind changed the lane: %+v", s)
	}
}

func TestV2CloseAndReleaseRequireOwnerAndEpoch(t *testing.T) {
	cfg := testPeerLaneV2(44)
	arena, path := testCreateArenaV2(t, 45, cfg)
	owner := testOwnerV2(1)
	epoch, err := arena.bindPeerLane(44, owner)
	if err != nil {
		t.Fatal(err)
	}

	if _, err := arena.beginClosePeerLane(44, testOwnerV2(2), epoch); !errors.Is(err, errV2OwnerMismatch) {
		t.Fatalf("wrong owner close: %v", err)
	}
	var leaseErr *leaseEpochErrorV2
	if _, err := arena.beginClosePeerLane(44, owner, epoch+1); !errors.As(err, &leaseErr) ||
		leaseErr.Expected != epoch || leaseErr.Observed != epoch+1 {
		t.Fatalf("wrong epoch close: %v", err)
	}
	if s := testStatsV2(t, arena, 44); s.State != v2LaneStateBound || s.FinalWriteSequence != 0 {
		t.Fatalf("rejected close changed the lane: %+v", s)
	}

	if _, err := arena.beginClosePeerLane(44, owner, epoch); err != nil {
		t.Fatal(err)
	}
	testAckV2(t, testOpenLaneV2(t, path, 45, cfg))
	if err := arena.releasePeerLane(44, testOwnerV2(2), epoch); !errors.Is(err, errV2OwnerMismatch) {
		t.Fatalf("wrong owner release: %v", err)
	}
	if err := arena.releasePeerLane(44, owner, epoch-1); !errors.As(err, &leaseErr) ||
		leaseErr.Expected != epoch || leaseErr.Observed != epoch-1 {
		t.Fatalf("wrong epoch release: %v", err)
	}
	if s := testStatsV2(t, arena, 44); s.State != v2LaneStateClosing || s.Owner != owner {
		t.Fatalf("rejected release changed the lane: %+v", s)
	}
	if err := arena.releasePeerLane(44, owner, epoch); err != nil {
		t.Fatal(err)
	}
}

func TestV2ReleaseBeforeDrainAndUnreadRecordPreventReuse(t *testing.T) {
	cfg := testPeerLaneV2(47)
	arena, path := testCreateArenaV2(t, 48, cfg)
	owner := testOwnerV2(1)
	epoch, err := arena.bindPeerLane(47, owner)
	if err != nil {
		t.Fatal(err)
	}
	producer := testOpenLaneV2(t, path, 48, cfg)
	consumer := testOpenLaneV2(t, path, 48, cfg)
	for n := byte(0); n < 3; n++ {
		if err := producer.publish(testRecordV2([]byte{n})); err != nil {
			t.Fatal(err)
		}
	}
	if _, err := arena.beginClosePeerLane(47, owner, epoch); err != nil {
		t.Fatal(err)
	}
	var notDrained *notDrainedErrorV2
	if err := arena.releasePeerLane(47, owner, epoch); !errors.As(err, &notDrained) ||
		notDrained.Read != 0 || notDrained.FinalWrite != 3 {
		t.Fatalf("release before drain: %v", err)
	}
	for i := 0; i < 2; i++ {
		if _, ok, err := consumer.tryConsume(); err != nil || !ok {
			t.Fatalf("consume %d: ok=%v err=%v", i, ok, err)
		}
	}
	if ok, err := consumer.acknowledgeDrainedIfClosing(); ok || err != nil {
		t.Fatalf("acknowledged with an unread record: ok=%v err=%v", ok, err)
	}
	if err := arena.releasePeerLane(47, owner, epoch); !errors.As(err, &notDrained) ||
		notDrained.Read != 2 || notDrained.FinalWrite != 3 {
		t.Fatalf("release with an unread record: %v", err)
	}
	_, err = arena.bindPeerLane(47, testOwnerV2(2))
	testExpectStateV2(t, err, v2LaneStateFree, v2LaneStateClosing)

	record, ok, err := consumer.tryConsume()
	if err != nil || !ok || record.Payload[0] != 2 {
		t.Fatalf("last record: ok=%v err=%v", ok, err)
	}
	testAckV2(t, consumer)
	if err := arena.releasePeerLane(47, owner, epoch); err != nil {
		t.Fatal(err)
	}
	if next, err := arena.bindPeerLane(47, testOwnerV2(2)); err != nil || next != epoch+1 {
		t.Fatalf("reuse after drain: epoch=%d err=%v", next, err)
	}
}

func TestV2EmptyClosingLaneDrainsAtOnce(t *testing.T) {
	cfg := testPeerLaneV2(48)
	arena, path := testCreateArenaV2(t, 49, cfg)
	owner := testOwnerV2(1)
	epoch, err := arena.bindPeerLane(48, owner)
	if err != nil {
		t.Fatal(err)
	}
	if final, err := arena.beginClosePeerLane(48, owner, epoch); err != nil || final != 0 {
		t.Fatalf("close final=%d err=%v", final, err)
	}
	testAckV2(t, testOpenLaneV2(t, path, 49, cfg))
	if s := testStatsV2(t, arena, 48); s.DrainedEpoch != epoch {
		t.Fatalf("drained epoch %d", s.DrainedEpoch)
	}
	if err := arena.releasePeerLane(48, owner, epoch); err != nil {
		t.Fatal(err)
	}
}

func TestV2WrongArenaOrLaneGenerationRejected(t *testing.T) {
	cfg := testPeerLaneV2(50)
	path := testArenaV2Path(t)
	created, err := createArenaV2(path, 50, []laneConfigV2{cfg})
	if err != nil {
		t.Fatal(err)
	}
	_ = created.close()
	for _, generation := range []uint64{0, 51} {
		if a, err := openArenaV2(path, generation); err == nil {
			_ = a.close()
			t.Fatalf("generation %d accepted", generation)
		}
	}

	// A lane header that disagrees with the arena generation.
	mut, err := openArenaV2(path, 50)
	if err != nil {
		t.Fatal(err)
	}
	lane, err := mut.lane(cfg)
	if err != nil {
		t.Fatal(err)
	}
	atomic.StoreUint64(lane.atomic(v2LHGeneration), 49)
	_ = mut.close()
	if a, err := openArenaV2(path, 50); err == nil {
		_ = a.close()
		t.Fatal("lane of another generation accepted")
	}
}

func TestV2RecordOfAnotherEpochQuarantinesLaneOnce(t *testing.T) {
	cfg := testPeerLaneV2(51)
	arena, path := testCreateArenaV2(t, 52, cfg)
	epoch, err := arena.bindPeerLane(51, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	producer := testOpenLaneV2(t, path, 52, cfg)
	if err := producer.publish(testRecordV2([]byte("r"))); err != nil {
		t.Fatal(err)
	}
	// Rewrite the record's epoch and re-seal its CRC, so that only the epoch
	// check can refuse it.
	slot := producer.slot(0)
	binary.LittleEndian.PutUint64(slot[v2SlotEpoch:], epoch+7)
	payloadLen := int(binary.LittleEndian.Uint32(slot[v2SlotPayload:]))
	binary.LittleEndian.PutUint32(slot[v2SlotCRC32C:], recordCRC32CV2(slot, payloadLen))

	consumer := testOpenLaneV2(t, path, 52, cfg)
	var corrupt *corruptRecordErrorV2
	if _, _, err := consumer.tryConsume(); !errors.As(err, &corrupt) || corrupt.Reason != v2QuarantineEpochMismatch {
		t.Fatalf("record of another epoch: %v", err)
	}
	var quarantined *laneQuarantinedErrorV2
	for i := 0; i < 4; i++ {
		if _, _, err := consumer.tryConsume(); !errors.As(err, &quarantined) || quarantined.Reason != v2QuarantineEpochMismatch {
			t.Fatalf("poll %d after quarantine: %v", i, err)
		}
	}
	if s := consumer.stats(); s.State != v2LaneStateQuarantined || s.CorruptRecords != 1 ||
		s.ReadSequence != 0 || s.QuarantineReason != v2QuarantineEpochMismatch {
		t.Fatalf("after quarantine: %+v", s)
	}
}

func TestV2LaneEpochExhaustionFailsClosed(t *testing.T) {
	cfg := testPeerLaneV2(52)
	arena, path := testCreateArenaV2(t, 53, cfg)
	lane, err := arena.lane(cfg)
	if err != nil {
		t.Fatal(err)
	}
	atomic.StoreUint64(lane.atomic(v2LHEpoch), ^uint64(0))
	if _, err := arena.bindPeerLane(52, testOwnerV2(1)); !errors.Is(err, errV2EpochExhausted) {
		t.Fatalf("exhausted epoch: %v", err)
	}
	if s := testStatsV2(t, arena, 52); s.State != v2LaneStateFree || s.LaneEpoch != ^uint64(0) || s.Owner != ([32]byte{}) {
		t.Fatalf("exhausted bind changed the lane: %+v", s)
	}

	// The last epoch is usable once; the bind after it is refused.
	atomic.StoreUint64(lane.atomic(v2LHEpoch), ^uint64(0)-1)
	last, err := arena.bindPeerLane(52, testOwnerV2(1))
	if err != nil || last != ^uint64(0) {
		t.Fatalf("last epoch=%d err=%v", last, err)
	}
	if _, err := arena.beginClosePeerLane(52, testOwnerV2(1), last); err != nil {
		t.Fatal(err)
	}
	testAckV2(t, testOpenLaneV2(t, path, 53, cfg))
	if err := arena.releasePeerLane(52, testOwnerV2(1), last); err != nil {
		t.Fatal(err)
	}
	if _, err := arena.bindPeerLane(52, testOwnerV2(2)); !errors.Is(err, errV2EpochExhausted) {
		t.Fatalf("bind past the last epoch: %v", err)
	}
	if s := testStatsV2(t, arena, 52); s.State != v2LaneStateFree {
		t.Fatalf("state %d", s.State)
	}
}

func testPoolSnapshotV2(t *testing.T, arena *arenaV2, ids []uint16) []laneStatsV2 {
	t.Helper()
	out := make([]laneStatsV2, 0, len(ids))
	for _, id := range ids {
		out = append(out, testStatsV2(t, arena, id))
	}
	return out
}

func testSameSnapshotV2(t *testing.T, before, after []laneStatsV2) {
	t.Helper()
	for i := range before {
		if before[i] != after[i] {
			t.Fatalf("lane %d changed: %+v -> %+v", i, before[i], after[i])
		}
	}
}

func TestV2PoolBindsOnlyFreeLanesAndReportsExhaustion(t *testing.T) {
	ids := []uint16{60, 61, 62}
	arena, path := testCreateArenaV2(t, 60, testPeerLaneV2(60), testPeerLaneV2(61), testPeerLaneV2(62), testTxpoolLaneV2(100))
	pool, err := newPeerLanePoolV2(arena, peerLanePoolBoundsV2{MaxPeers: 2, ChurnReserve: 1})
	if err != nil {
		t.Fatal(err)
	}
	if got := pool.stats().PeerLanes; got != 3 {
		t.Fatalf("peer lanes %d", got)
	}
	a, err := pool.allocate(testOwnerV2(0xa))
	if err != nil {
		t.Fatal(err)
	}
	b, err := pool.allocate(testOwnerV2(0xb))
	if err != nil {
		t.Fatal(err)
	}
	if a.LaneID == b.LaneID {
		t.Fatal("one lane allocated twice")
	}

	started := time.Now()
	before := testPoolSnapshotV2(t, arena, ids)
	if _, err := pool.allocate(testOwnerV2(0xc)); !errors.Is(err, errV2PeerLimit) {
		t.Fatalf("peer limit: %v", err)
	}
	testSameSnapshotV2(t, before, testPoolSnapshotV2(t, arena, ids))

	// A closing (still draining) lane frees a peer slot; the reserve lane
	// takes the new peer.
	if _, err := pool.beginClose(a); err != nil {
		t.Fatal(err)
	}
	c, err := pool.allocate(testOwnerV2(0xc))
	if err != nil || c.LaneID == a.LaneID || c.LaneID == b.LaneID {
		t.Fatalf("reserve lane: lease=%+v err=%v", c, err)
	}

	// No FREE lane left: explicit, nothing stolen.
	if _, err := pool.beginClose(b); err != nil {
		t.Fatal(err)
	}
	before = testPoolSnapshotV2(t, arena, ids)
	var exhausted *poolExhaustedErrorV2
	if _, err := pool.allocate(testOwnerV2(0xd)); !errors.As(err, &exhausted) || exhausted.PeerLanes != 3 {
		t.Fatalf("exhaustion: %v", err)
	}
	testSameSnapshotV2(t, before, testPoolSnapshotV2(t, arena, ids))
	if time.Since(started) > time.Second {
		t.Fatal("allocation waited")
	}
	stats := pool.stats()
	if stats.Free != 0 || stats.Bound != 1 || stats.Closing != 2 || stats.Quarantined != 0 ||
		stats.Allocations != 3 || stats.Exhausted != 1 || stats.PeerLimitRefusals != 1 {
		t.Fatalf("stats %+v", stats)
	}

	// Once a's lane is drained and released, it is the one reused.
	testAckV2(t, testOpenLaneV2(t, path, 60, testPeerLaneV2(a.LaneID)))
	if err := pool.release(a); err != nil {
		t.Fatal(err)
	}
	d, err := pool.allocate(testOwnerV2(0xd))
	if err != nil || d.LaneID != a.LaneID || d.Epoch != a.Epoch+1 {
		t.Fatalf("reuse: lease=%+v err=%v", d, err)
	}
}

func TestV2PoolBoundsMustMatchArena(t *testing.T) {
	arena, _ := testCreateArenaV2(t, 61, testPeerLaneV2(63), testPeerLaneV2(64), testTxpoolLaneV2(101))
	for _, bounds := range []peerLanePoolBoundsV2{{0, 2}, {3, 0}, {2, 1}, {1, 0}, {3, -1}} {
		if _, err := newPeerLanePoolV2(arena, bounds); !errors.Is(err, errV2PoolBounds) {
			t.Fatalf("bounds %+v: %v", bounds, err)
		}
	}
	pool, err := newPeerLanePoolV2(arena, peerLanePoolBoundsV2{MaxPeers: 1, ChurnReserve: 1})
	if err != nil || pool.stats().PeerLanes != 2 {
		t.Fatalf("matching bounds: %v", err)
	}
}

func TestV2PoolConcurrentAllocateNeverDoubleAssigns(t *testing.T) {
	const lanes = 8
	configs := make([]laneConfigV2, 0, lanes)
	for i := 0; i < lanes; i++ {
		configs = append(configs, testPeerLaneV2(uint16(200+i)))
	}
	arena, _ := testCreateArenaV2(t, 62, configs...)
	pool, err := newPeerLanePoolV2(arena, peerLanePoolBoundsV2{MaxPeers: 6, ChurnReserve: 2})
	if err != nil {
		t.Fatal(err)
	}
	const callers = 32
	leases := make([]peerLaneLeaseV2, callers)
	errs := make([]error, callers)
	var start, done sync.WaitGroup
	start.Add(1)
	for i := 0; i < callers; i++ {
		done.Add(1)
		go func(i int) {
			defer done.Done()
			start.Wait()
			leases[i], errs[i] = pool.allocate(testOwnerV2(byte(i + 1)))
		}(i)
	}
	start.Done()
	done.Wait()

	granted := map[uint16]peerLaneLeaseV2{}
	for i := 0; i < callers; i++ {
		if errs[i] != nil {
			if !errors.Is(errs[i], errV2PeerLimit) && !errors.Is(errs[i], errV2PoolExhausted) {
				t.Fatalf("caller %d: unexpected error %v", i, errs[i])
			}
			continue
		}
		if _, dup := granted[leases[i].LaneID]; dup {
			t.Fatalf("lane %d granted twice", leases[i].LaneID)
		}
		granted[leases[i].LaneID] = leases[i]
	}
	if len(granted) != 6 {
		t.Fatalf("granted %d leases, want max_peers=6", len(granted))
	}
	for id, lease := range granted {
		if s := testStatsV2(t, arena, id); s.State != v2LaneStateBound || s.Owner != lease.Owner || s.LaneEpoch != lease.Epoch {
			t.Fatalf("lane %d does not hold its lease: %+v", id, s)
		}
	}
	if stats := pool.stats(); stats.Allocations != 6 || stats.PeerLimitRefusals+stats.Exhausted != callers-6 {
		t.Fatalf("stats %+v", stats)
	}
}

func TestV2ChurnKeepsEpochsMonotonicAndSequencesContinuous(t *testing.T) {
	cfg := testPeerLaneV2(70)
	arena, path := testCreateArenaV2(t, 70, cfg)
	pool, err := newPeerLanePoolV2(arena, peerLanePoolBoundsV2{MaxPeers: 1, ChurnReserve: 0})
	if err != nil {
		t.Fatal(err)
	}
	producer := testOpenLaneV2(t, path, 70, cfg)
	consumer := testOpenLaneV2(t, path, 70, cfg)
	previousEpoch := uint64(1)
	nextSequence := uint64(0)
	for cycle := uint32(0); cycle < 200; cycle++ {
		var owner [32]byte
		binary.LittleEndian.PutUint32(owner[:4], cycle)
		lease, err := pool.allocate(owner)
		if err != nil {
			t.Fatal(err)
		}
		if lease.Epoch != previousEpoch+1 {
			t.Fatalf("cycle %d: epoch %d after %d", cycle, lease.Epoch, previousEpoch)
		}
		previousEpoch = lease.Epoch

		records := int(cycle % 5)
		for index := 0; index < records; index++ {
			payload := make([]byte, 8)
			binary.LittleEndian.PutUint32(payload[:4], cycle)
			binary.LittleEndian.PutUint32(payload[4:], uint32(index))
			if err := producer.publish(testRecordV2(payload)); err != nil {
				t.Fatal(err)
			}
		}
		finalWrite, err := pool.beginClose(lease)
		if err != nil || finalWrite != nextSequence+uint64(records) {
			t.Fatalf("cycle %d: final=%d err=%v", cycle, finalWrite, err)
		}
		// Only this binding's records, in order, under this epoch.
		drained := testDrainV2(t, consumer)
		if len(drained) != records {
			t.Fatalf("cycle %d: drained %d of %d", cycle, len(drained), records)
		}
		for index, record := range drained {
			if record.Sequence != nextSequence+uint64(index) || record.LaneEpoch != lease.Epoch ||
				binary.LittleEndian.Uint32(record.Payload[:4]) != cycle ||
				binary.LittleEndian.Uint32(record.Payload[4:]) != uint32(index) {
				t.Fatalf("cycle %d record %d: %+v", cycle, index, record)
			}
		}
		nextSequence += uint64(records)
		testAckV2(t, consumer)
		if err := pool.release(lease); err != nil {
			t.Fatal(err)
		}
		if s := testStatsV2(t, arena, 70); s.State != v2LaneStateFree || s.Owner != ([32]byte{}) ||
			s.WriteSequence != nextSequence || s.ReadSequence != nextSequence {
			t.Fatalf("cycle %d after release: %+v", cycle, s)
		}
	}
	if stats := pool.stats(); stats.Allocations != 200 || stats.Releases != 200 {
		t.Fatalf("stats %+v", stats)
	}
}

func TestV2StaleLeaseNeverActsOnReusedLane(t *testing.T) {
	cfg := testPeerLaneV2(71)
	arena, path := testCreateArenaV2(t, 71, cfg)
	pool, err := newPeerLanePoolV2(arena, peerLanePoolBoundsV2{MaxPeers: 1})
	if err != nil {
		t.Fatal(err)
	}
	consumer := testOpenLaneV2(t, path, 71, cfg)
	first, err := pool.allocate(testOwnerV2(7))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := pool.beginClose(first); err != nil {
		t.Fatal(err)
	}
	testAckV2(t, consumer)
	if err := pool.release(first); err != nil {
		t.Fatal(err)
	}
	second, err := pool.allocate(testOwnerV2(7)) // same owner bytes
	if err != nil || second.Epoch != first.Epoch+1 {
		t.Fatalf("second lease %+v err=%v", second, err)
	}

	var leaseErr *leaseEpochErrorV2
	if _, err := pool.beginClose(first); !errors.As(err, &leaseErr) ||
		leaseErr.Expected != second.Epoch || leaseErr.Observed != first.Epoch {
		t.Fatalf("stale close: %v", err)
	}
	testExpectStateV2(t, pool.release(first), v2LaneStateClosing, v2LaneStateBound)
	if s := testStatsV2(t, arena, 71); s.State != v2LaneStateBound || s.LaneEpoch != second.Epoch || s.Owner != testOwnerV2(7) {
		t.Fatalf("stale lease changed the lane: %+v", s)
	}
	if _, err := pool.beginClose(second); err != nil {
		t.Fatal(err)
	}
}

// testReuseAfterFullRingV2 binds the lane once, fills all four slots
// (committed 1..4), drains, releases and binds again. It returns the first
// and second epoch.
func testReuseAfterFullRingV2(t *testing.T, arena *arenaV2, producer, consumer *laneV2, id uint16) (uint64, uint64) {
	t.Helper()
	first, err := arena.bindPeerLane(id, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	for n := byte(0); n < 4; n++ {
		if err := producer.publish(testRecordV2([]byte{n})); err != nil {
			t.Fatal(err)
		}
	}
	if got := len(testDrainV2(t, consumer)); got != 4 {
		t.Fatalf("drained %d", got)
	}
	if _, err := arena.beginClosePeerLane(id, testOwnerV2(1), first); err != nil {
		t.Fatal(err)
	}
	testAckV2(t, consumer)
	if err := arena.releasePeerLane(id, testOwnerV2(1), first); err != nil {
		t.Fatal(err)
	}
	second, err := arena.bindPeerLane(id, testOwnerV2(2))
	if err != nil {
		t.Fatal(err)
	}
	return first, second
}

func TestV2SlotsOfEarlierBindingNeverAccepted(t *testing.T) {
	cfgA, cfgB := testPeerLaneV2(72), testPeerLaneV2(75)
	arena, path := testCreateArenaV2(t, 72, cfgA, cfgB)

	// A write_sequence that runs ahead of its slot: slot 0 still holds
	// committed_sequence 1 from the first binding, never 5.
	producer := testOpenLaneV2(t, path, 72, cfgA)
	consumer := testOpenLaneV2(t, path, 72, cfgA)
	testReuseAfterFullRingV2(t, arena, producer, consumer, 72)
	atomic.StoreUint64(producer.atomic(v2LHWriteSeq), 5)
	var corrupt *corruptRecordErrorV2
	if _, _, err := consumer.tryConsume(); !errors.As(err, &corrupt) ||
		corrupt.Reason != v2QuarantineCorruptSeq || corrupt.Detail != "corrupt sequence 1 != 5" {
		t.Fatalf("stale committed sequence: %v", err)
	}
	if s := consumer.stats(); s.ReadSequence != 4 || s.State != v2LaneStateQuarantined {
		t.Fatalf("after stale sequence: %+v", s)
	}

	// Even with a matching committed_sequence, the old record's epoch is
	// refused.
	producerB := testOpenLaneV2(t, path, 72, cfgB)
	consumerB := testOpenLaneV2(t, path, 72, cfgB)
	first, second := testReuseAfterFullRingV2(t, arena, producerB, consumerB, 75)
	atomic.StoreUint64(producerB.atomic(v2LHWriteSeq), 5)
	atomic.StoreUint64(v2AtomicU64(producerB.slot(0), v2SlotCommitted), 5)
	if _, _, err := consumerB.tryConsume(); !errors.As(err, &corrupt) || corrupt.Reason != v2QuarantineEpochMismatch ||
		corrupt.Detail != fmt.Sprintf("lane epoch mismatch %d != %d", first, second) {
		t.Fatalf("old epoch record: %v", err)
	}
	if read := consumerB.stats().ReadSequence; read != 4 {
		t.Fatalf("read=%d", read)
	}
}

func TestV2WriteAfterCloseQuarantinesLane(t *testing.T) {
	cfgA, cfgB := testPeerLaneV2(73), testPeerLaneV2(74)
	arena, path := testCreateArenaV2(t, 73, cfgA, cfgB)

	// Detected by the consumer's drain acknowledgement.
	epoch, err := arena.bindPeerLane(73, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	producer := testOpenLaneV2(t, path, 73, cfgA)
	consumer := testOpenLaneV2(t, path, 73, cfgA)
	if err := producer.publish(testRecordV2([]byte("0"))); err != nil {
		t.Fatal(err)
	}
	if final, err := arena.beginClosePeerLane(73, testOwnerV2(1), epoch); err != nil || final != 1 {
		t.Fatalf("close final=%d err=%v", final, err)
	}
	// A producer that passed its state check before the close lands one
	// more record.
	atomic.StoreUint64(producer.atomic(v2LHWriteSeq), 2)
	if _, ok, err := consumer.tryConsume(); err != nil || !ok {
		t.Fatalf("consume: ok=%v err=%v", ok, err)
	}
	var late *lateWriteErrorV2
	if ok, err := consumer.acknowledgeDrainedIfClosing(); ok || !errors.As(err, &late) || late.FinalWrite != 1 || late.Write != 2 {
		t.Fatalf("late write at ack: ok=%v err=%v", ok, err)
	}
	if s := testStatsV2(t, arena, 73); s.State != v2LaneStateQuarantined || !s.Shutdown ||
		s.QuarantineReason != v2QuarantineLateWrite || s.CorruptRecords != 0 {
		t.Fatalf("not quarantined as a late write: %+v", s)
	}
	testExpectStateV2(t, arena.releasePeerLane(73, testOwnerV2(1), epoch), v2LaneStateClosing, v2LaneStateQuarantined)
	testExpectStateV2(t, producer.publish(testRecordV2([]byte("x"))), v2LaneStateBound, v2LaneStateQuarantined)

	// Detected by the release after an acknowledged drain.
	epochB, err := arena.bindPeerLane(74, testOwnerV2(2))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := arena.beginClosePeerLane(74, testOwnerV2(2), epochB); err != nil {
		t.Fatal(err)
	}
	laneB := testOpenLaneV2(t, path, 73, cfgB)
	testAckV2(t, laneB)
	atomic.StoreUint64(laneB.atomic(v2LHWriteSeq), 1)
	if err := arena.releasePeerLane(74, testOwnerV2(2), epochB); !errors.As(err, &late) || late.FinalWrite != 0 || late.Write != 1 {
		t.Fatalf("late write at release: %v", err)
	}
	_, err = arena.bindPeerLane(74, testOwnerV2(3))
	testExpectStateV2(t, err, v2LaneStateFree, v2LaneStateQuarantined)
}

func TestV2QuarantineTerminalAndRestartQuarantinesOldGeneration(t *testing.T) {
	cfgs := []laneConfigV2{testPeerLaneV2(80), testPeerLaneV2(81), testTxpoolLaneV2(102)}
	path := testArenaV2Path(t)
	arena, err := createArenaV2(path, 80, cfgs)
	if err != nil {
		t.Fatal(err)
	}
	epoch, err := arena.bindPeerLane(80, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	consumer := testOpenLaneV2(t, path, 80, cfgs[0])
	producer := testOpenLaneV2(t, path, 80, cfgs[0])
	if previous := consumer.quarantine(); previous != v2LaneStateBound {
		t.Fatalf("previous state %d", previous)
	}
	if previous := consumer.quarantine(); previous != v2LaneStateQuarantined {
		t.Fatalf("quarantine not idempotent: %d", previous)
	}
	testExpectStateV2(t, producer.publish(testRecordV2([]byte("x"))), v2LaneStateBound, v2LaneStateQuarantined)
	_, err = arena.beginClosePeerLane(80, testOwnerV2(1), epoch)
	testExpectStateV2(t, err, v2LaneStateBound, v2LaneStateQuarantined)
	_, err = arena.bindPeerLane(80, testOwnerV2(2))
	testExpectStateV2(t, err, v2LaneStateFree, v2LaneStateQuarantined)
	_ = arena.close()

	// Restart: the new runtime quarantines every lane of the old generation
	// before abandoning it. No record is destroyed.
	old, err := openArenaV2(path, 80)
	if err != nil {
		t.Fatal(err)
	}
	defer old.close()
	if old.generation() != 80 {
		t.Fatalf("generation %d", old.generation())
	}
	if previous, err := old.quarantineLane(81); err != nil || previous != v2LaneStateFree {
		t.Fatalf("quarantineLane previous=%d err=%v", previous, err)
	}
	if n, err := old.quarantineAll(); err != nil || n != 1 {
		t.Fatalf("quarantineAll newly=%d err=%v", n, err)
	}
	if n, err := old.quarantineAll(); err != nil || n != 0 {
		t.Fatalf("second quarantineAll newly=%d err=%v", n, err)
	}
	for id, reason := range map[uint16]uint64{80: v2QuarantineOperator, 81: v2QuarantineOperator, 102: v2QuarantineRestart} {
		if s := testStatsV2(t, old, id); s.State != v2LaneStateQuarantined || !s.Shutdown || s.QuarantineReason != reason {
			t.Fatalf("lane %d: %+v", id, s)
		}
	}
	txpool := testOpenLaneV2(t, path, 80, cfgs[2])
	testExpectStateV2(t, txpool.publish(testRecordV2([]byte("t"))), v2LaneStateBound, v2LaneStateQuarantined)
	pool, err := newPeerLanePoolV2(old, peerLanePoolBoundsV2{MaxPeers: 1, ChurnReserve: 1})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := pool.allocate(testOwnerV2(9)); !errors.Is(err, errV2PoolExhausted) {
		t.Fatalf("allocation from a quarantined generation: %v", err)
	}
	if q := pool.stats().Quarantined; q != 2 {
		t.Fatalf("quarantined %d", q)
	}
	if a, err := openArenaV2(path, 81); err == nil {
		_ = a.close()
		t.Fatal("old generation opened as the new one")
	}
}

func TestV2IndependentPeerLanesCycleConcurrently(t *testing.T) {
	const (
		lanes    = 4
		rounds   = 25
		perRound = 6
	)
	configs := make([]laneConfigV2, 0, lanes)
	for i := 0; i < lanes; i++ {
		configs = append(configs, testPeerLaneV2(uint16(90+i)))
	}
	arena, path := testCreateArenaV2(t, 90, configs...)
	boundRound := make([]atomic.Uint64, lanes)
	publishedRound := make([]atomic.Uint64, lanes)
	ownerFor := func(lane int, round uint32) [32]byte {
		var owner [32]byte
		owner[0] = byte(lane)
		binary.LittleEndian.PutUint32(owner[1:5], round)
		return owner
	}
	owners := make([][32]byte, lanes)
	epochs := make([]uint64, lanes)
	for lane, cfg := range configs {
		owners[lane] = ownerFor(lane, 0)
		epoch, err := arena.bindPeerLane(cfg.ID, owners[lane])
		if err != nil {
			t.Fatal(err)
		}
		epochs[lane] = epoch
		boundRound[lane].Store(1)
	}

	deadline := time.Now().Add(30 * time.Second)
	var producers sync.WaitGroup
	for lane, cfg := range configs {
		producers.Add(1)
		go func(lane int, cfg laneConfigV2) {
			defer producers.Done()
			a, err := openArenaV2(path, 90)
			if err != nil {
				t.Error(err)
				return
			}
			defer a.close()
			producer, err := a.lane(cfg)
			if err != nil {
				t.Error(err)
				return
			}
			for round := uint32(0); round < rounds; round++ {
				for boundRound[lane].Load() != uint64(round)+1 {
					if time.Now().After(deadline) {
						t.Errorf("producer %d waited too long", lane)
						return
					}
					runtime.Gosched()
				}
				for index := uint32(0); index < perRound; index++ {
					payload := make([]byte, 12)
					binary.LittleEndian.PutUint32(payload[0:], uint32(lane))
					binary.LittleEndian.PutUint32(payload[4:], round)
					binary.LittleEndian.PutUint32(payload[8:], index)
					for {
						err := producer.publish(testRecordV2(payload))
						if err == nil {
							break
						}
						if err.Error() != "fastlane v2 queue full" || time.Now().After(deadline) {
							t.Errorf("lane %d: %v", lane, err)
							return
						}
						runtime.Gosched()
					}
				}
				publishedRound[lane].Store(uint64(round) + 1)
			}
		}(lane, cfg)
	}

	consumers := make([]*laneV2, lanes)
	for lane, cfg := range configs {
		consumers[lane] = testOpenLaneV2(t, path, 90, cfg)
	}
	roundOf := make([]uint32, lanes)
	consumed := make([]uint32, lanes)
	finished := 0
	for finished < lanes {
		for lane, cfg := range configs {
			if roundOf[lane] == rounds {
				continue
			}
			for {
				record, ok, err := consumers[lane].tryConsume()
				if err != nil {
					t.Fatal(err)
				}
				if !ok {
					break
				}
				if record.LaneEpoch != epochs[lane] ||
					binary.LittleEndian.Uint32(record.Payload[0:]) != uint32(lane) ||
					binary.LittleEndian.Uint32(record.Payload[4:]) != roundOf[lane] ||
					binary.LittleEndian.Uint32(record.Payload[8:]) != consumed[lane] {
					t.Fatalf("lane %d round %d record %d: %+v", lane, roundOf[lane], consumed[lane], record)
				}
				consumed[lane]++
			}
			if consumed[lane] == perRound && publishedRound[lane].Load() == uint64(roundOf[lane])+1 {
				finalWrite, err := arena.beginClosePeerLane(cfg.ID, owners[lane], epochs[lane])
				if err != nil || finalWrite != uint64((roundOf[lane]+1)*perRound) {
					t.Fatalf("lane %d close final=%d err=%v", lane, finalWrite, err)
				}
				testAckV2(t, consumers[lane])
				if err := arena.releasePeerLane(cfg.ID, owners[lane], epochs[lane]); err != nil {
					t.Fatal(err)
				}
				roundOf[lane]++
				consumed[lane] = 0
				if roundOf[lane] == rounds {
					finished++
					continue
				}
				owners[lane] = ownerFor(lane, roundOf[lane])
				next, err := arena.bindPeerLane(cfg.ID, owners[lane])
				if err != nil || next != epochs[lane]+1 {
					t.Fatalf("lane %d rebind epoch=%d err=%v", lane, next, err)
				}
				epochs[lane] = next
				boundRound[lane].Store(uint64(roundOf[lane]) + 1)
			}
		}
		if time.Now().After(deadline) {
			t.Fatalf("lifecycle stalled: rounds=%v", roundOf)
		}
		runtime.Gosched()
	}
	producers.Wait()
	for lane, cfg := range configs {
		s := testStatsV2(t, arena, cfg.ID)
		if s.State != v2LaneStateFree || s.LaneEpoch != 1+rounds || s.ReadSequence != rounds*perRound || epochs[lane] != 1+rounds {
			t.Fatalf("lane %d final %+v", lane, s)
		}
	}
}

// ---------------------------------------------------------------------------
// K12: structural corruption quarantines once, never skips
// ---------------------------------------------------------------------------

func TestV2EveryStructuralDefectQuarantinesOnceAndIsNeverSkipped(t *testing.T) {
	cases := []struct {
		defect string
		reason uint64
	}{
		{"sequence", v2QuarantineCorruptSeq},
		{"payload-length", v2QuarantinePayloadLength},
		{"epoch", v2QuarantineEpochMismatch},
		{"crc", v2QuarantineChecksum},
		{"kind", v2QuarantineRecordKind},
	}
	for _, tc := range cases {
		t.Run(tc.defect, func(t *testing.T) {
			cfg := testPeerLaneV2(110)
			arena, path := testCreateArenaV2(t, 110, cfg)
			owner := testOwnerV2(1)
			epoch, err := arena.bindPeerLane(110, owner)
			if err != nil {
				t.Fatal(err)
			}
			producer := testOpenLaneV2(t, path, 110, cfg)
			for _, payload := range []string{"bad", "good"} {
				if err := producer.publish(testRecordV2([]byte(payload))); err != nil {
					t.Fatal(err)
				}
			}
			slot := producer.slot(0)
			switch tc.defect {
			case "sequence":
				atomic.StoreUint64(v2AtomicU64(slot, v2SlotCommitted), 7)
			case "payload-length":
				binary.LittleEndian.PutUint32(slot[v2SlotPayload:], cfg.SlotStride-v2SlotHeaderSize+1)
			case "epoch":
				binary.LittleEndian.PutUint64(slot[v2SlotEpoch:], epoch+1)
			case "crc":
				slot[v2SlotHeaderSize] ^= 0x01
			case "kind":
				binary.LittleEndian.PutUint16(slot[v2SlotKind:], 99)
			}
			if tc.defect == "epoch" || tc.defect == "kind" {
				// Re-seal so that only the targeted check can refuse it.
				payloadLen := int(binary.LittleEndian.Uint32(slot[v2SlotPayload:]))
				binary.LittleEndian.PutUint32(slot[v2SlotCRC32C:], recordCRC32CV2(slot, payloadLen))
			}

			consumer := testOpenLaneV2(t, path, 110, cfg)
			var corrupt *corruptRecordErrorV2
			if _, _, err := consumer.tryConsume(); !errors.As(err, &corrupt) || corrupt.Reason != tc.reason {
				t.Fatalf("first poll: %v", err)
			}
			var quarantined *laneQuarantinedErrorV2
			for i := 0; i < 5; i++ {
				if _, _, err := consumer.tryConsume(); !errors.As(err, &quarantined) || quarantined.Reason != tc.reason {
					t.Fatalf("poll %d: %v", i, err)
				}
			}
			if s := consumer.stats(); s.State != v2LaneStateQuarantined || s.CorruptRecords != 1 ||
				s.ReadSequence != 0 || s.QuarantineReason != tc.reason || !s.Shutdown {
				t.Fatalf("after quarantine: %+v", s)
			}
			// Not consumed, not reusable, no publish.
			testExpectStateV2(t, producer.publish(testRecordV2([]byte("x"))), v2LaneStateBound, v2LaneStateQuarantined)
			_, err = arena.beginClosePeerLane(110, owner, epoch)
			testExpectStateV2(t, err, v2LaneStateBound, v2LaneStateQuarantined)
			_, err = arena.bindPeerLane(110, testOwnerV2(2))
			testExpectStateV2(t, err, v2LaneStateFree, v2LaneStateQuarantined)
			if _, err := consumer.acknowledgeDrainedIfClosing(); !errors.Is(err, errV2Quarantined) {
				t.Fatalf("ack on a quarantined lane: %v", err)
			}
		})
	}
}

func TestV2RecordsOutsideBindingQuarantineAsLateWrites(t *testing.T) {
	cfgA, cfgB := testPeerLaneV2(111), testPeerLaneV2(112)
	arena, path := testCreateArenaV2(t, 111, cfgA, cfgB)

	// A record on a FREE lane.
	laneA := testOpenLaneV2(t, path, 111, cfgA)
	atomic.StoreUint64(laneA.atomic(v2LHWriteSeq), 1)
	var late *lateWriteErrorV2
	if _, _, err := laneA.tryConsume(); !errors.As(err, &late) || late.FinalWrite != 0 || late.Write != 1 {
		t.Fatalf("record on a FREE lane: %v", err)
	}
	if s := laneA.stats(); s.State != v2LaneStateQuarantined || s.QuarantineReason != v2QuarantineLateWrite ||
		s.CorruptRecords != 0 || s.ReadSequence != 0 {
		t.Fatalf("FREE lane after late write: %+v", s)
	}

	// A record at final_write_sequence on a CLOSING lane.
	epoch, err := arena.bindPeerLane(112, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	laneB := testOpenLaneV2(t, path, 111, cfgB)
	if err := laneB.publish(testRecordV2([]byte("in"))); err != nil {
		t.Fatal(err)
	}
	if _, err := arena.beginClosePeerLane(112, testOwnerV2(1), epoch); err != nil {
		t.Fatal(err)
	}
	atomic.StoreUint64(laneB.atomic(v2LHWriteSeq), 2)
	if record, ok, err := laneB.tryConsume(); err != nil || !ok || string(record.Payload) != "in" {
		t.Fatalf("record inside the binding: ok=%v err=%v", ok, err)
	}
	if _, _, err := laneB.tryConsume(); !errors.As(err, &late) || late.FinalWrite != 1 || late.Write != 2 {
		t.Fatalf("record past final_write: %v", err)
	}
	if s := laneB.stats(); s.State != v2LaneStateQuarantined || s.QuarantineReason != v2QuarantineLateWrite ||
		s.CorruptRecords != 0 || s.ReadSequence != 1 {
		t.Fatalf("CLOSING lane after late write: %+v", s)
	}
}

func TestV2PoolNeverTakesBoundClosingOrQuarantinedLanes(t *testing.T) {
	ids := []uint16{120, 121, 122, 123}
	arena, _ := testCreateArenaV2(t, 120, testPeerLaneV2(120), testPeerLaneV2(121), testPeerLaneV2(122), testPeerLaneV2(123))
	bound, err := arena.bindPeerLane(120, testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	closing, err := arena.bindPeerLane(121, testOwnerV2(2))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := arena.beginClosePeerLane(121, testOwnerV2(2), closing); err != nil {
		t.Fatal(err)
	}
	if _, err := arena.quarantineLane(122); err != nil {
		t.Fatal(err)
	}
	pool, err := newPeerLanePoolV2(arena, peerLanePoolBoundsV2{MaxPeers: 3, ChurnReserve: 1})
	if err != nil {
		t.Fatal(err)
	}
	lease, err := pool.allocate(testOwnerV2(4))
	if err != nil || lease.LaneID != 123 {
		t.Fatalf("allocation: lease=%+v err=%v", lease, err)
	}
	before := testPoolSnapshotV2(t, arena, ids)
	if _, err := pool.allocate(testOwnerV2(5)); !errors.Is(err, errV2PoolExhausted) {
		t.Fatalf("exhaustion: %v", err)
	}
	testSameSnapshotV2(t, before, testPoolSnapshotV2(t, arena, ids))
	if before[0].LaneEpoch != bound {
		t.Fatalf("BOUND lane epoch changed: %d != %d", before[0].LaneEpoch, bound)
	}
	if s := pool.stats(); s.Free != 0 || s.Bound != 2 || s.Closing != 1 || s.Quarantined != 1 {
		t.Fatalf("stats %+v", s)
	}
}

// ---------------------------------------------------------------------------
// K11: shutdown never unmaps a live user
// ---------------------------------------------------------------------------

func TestV2RuntimeShutdownRefusesBindsAndPublishesWithoutUnmapping(t *testing.T) {
	dir := t.TempDir()
	lanes := []laneConfigV2{testPeerLaneV2(130), testPeerLaneV2(131), testTxpoolLaneV2(132)}
	rt, err := startGethRuntimeV2(dir, 130, lanes, peerLanePoolBoundsV2{MaxPeers: 1, ChurnReserve: 1})
	if err != nil {
		t.Fatal(err)
	}
	if err := installGethRuntimeV2(rt); err != nil {
		t.Fatal(err)
	}
	if err := installGethRuntimeV2(rt); !errors.Is(err, errV2RuntimeActive) {
		t.Fatalf("second install: %v", err)
	}

	// A publisher loads the runtime once and keeps its lane across the
	// shutdown, as a peer handler would.
	held := currentGethRuntimeV2()
	lease, err := held.pool.allocate(testOwnerV2(1))
	if err != nil {
		t.Fatal(err)
	}
	lane, err := held.arena.laneByID(lease.LaneID)
	if err != nil {
		t.Fatal(err)
	}
	var published atomic.Uint64
	stopped := make(chan error, 1)
	go func() {
		deadline := time.Now().Add(30 * time.Second)
		for {
			err := lane.publish(testRecordV2([]byte("p")))
			switch {
			case err == nil:
				published.Add(1)
			case err.Error() == "fastlane v2 queue full":
				runtime.Gosched()
			case errors.Is(err, errV2Shutdown):
				stopped <- nil
				return
			default:
				stopped <- err
				return
			}
			if time.Now().After(deadline) {
				stopped <- errors.New("shutdown never observed")
				return
			}
		}
	}()

	consumer := testOpenLaneV2(t, arenaPathV2(dir, 130), 130, testPeerLaneV2(lease.LaneID))
	consumed := uint64(0)
	for consumed < 8 {
		if _, ok, err := consumer.tryConsume(); err != nil {
			t.Fatal(err)
		} else if ok {
			consumed++
		}
		runtime.Gosched()
	}
	shutdownGethRuntimeV2()
	shutdownGethRuntimeV2() // idempotent
	if err := <-stopped; err != nil {
		t.Fatal(err)
	}
	if currentGethRuntimeV2() != nil {
		t.Fatal("runtime still active after shutdown")
	}

	// Everything published is still readable; then the lane reports its end.
	for {
		_, ok, err := consumer.tryConsume()
		if errors.Is(err, errV2Shutdown) {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		if ok {
			consumed++
		}
	}
	if consumed != published.Load() {
		t.Fatalf("consumed %d of %d published", consumed, published.Load())
	}
	// The held mapping is still mapped: the publisher's lane reads its header.
	if string(held.arena.data[v2SHMagic:v2SHMagic+8]) != v2Magic {
		t.Fatal("mapping no longer readable")
	}
	if _, err := held.pool.allocate(testOwnerV2(2)); !errors.Is(err, errV2Shutdown) {
		t.Fatalf("allocation after shutdown: %v", err)
	}
	other := uint16(130)
	if lease.LaneID == 130 {
		other = 131
	}
	if _, err := held.arena.bindPeerLane(other, testOwnerV2(3)); !errors.Is(err, errV2Shutdown) {
		t.Fatalf("bind after shutdown: %v", err)
	}
	txpool, err := held.arena.lane(lanes[2])
	if err != nil {
		t.Fatal(err)
	}
	if err := txpool.publish(testRecordV2([]byte("t"))); !errors.Is(err, errV2Shutdown) {
		t.Fatalf("dedicated lane publish after shutdown: %v", err)
	}
	// Close, drain and release stay deterministic during shutdown.
	if _, err := held.pool.beginClose(lease); err != nil {
		t.Fatal(err)
	}
	testAckV2(t, consumer)
	if err := held.pool.release(lease); err != nil {
		t.Fatal(err)
	}
	// The role stays held until process exit.
	if _, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 130); !errors.Is(err, errV2RoleLeaseHeld) {
		t.Fatalf("geth-owner lease after shutdown: %v", err)
	}
}

func TestV2RuntimeStartFailsClosedOnBoundsAndExistingArena(t *testing.T) {
	dir := t.TempDir()
	lanes := []laneConfigV2{testPeerLaneV2(140), testPeerLaneV2(141)}
	if _, err := startGethRuntimeV2(dir, 140, lanes, peerLanePoolBoundsV2{MaxPeers: 3}); !errors.Is(err, errV2PoolBounds) {
		t.Fatalf("bad bounds: %v", err)
	}
	if _, err := os.Stat(arenaPathV2(dir, 140)); !errors.Is(err, os.ErrNotExist) {
		t.Fatalf("arena created despite bad bounds: %v", err)
	}
	created, err := createArenaV2(arenaPathV2(dir, 140), 140, lanes)
	if err != nil {
		t.Fatal(err)
	}
	_ = created.close()
	var exists *arenaExistsErrorV2
	if _, err := startGethRuntimeV2(dir, 140, lanes, peerLanePoolBoundsV2{MaxPeers: 1, ChurnReserve: 1}); !errors.As(err, &exists) ||
		exists.Probe != arenaCurrentV2 {
		t.Fatalf("existing arena: %v", err)
	}
	// The failed start released its role.
	lease, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 140)
	if err != nil {
		t.Fatal(err)
	}
	_ = lease.release()
}

// ---------------------------------------------------------------------------
// K14: exclusive runtime role leases
// ---------------------------------------------------------------------------

func TestV2RoleLeasesAreExclusivePerRoleAndCoexistAcrossRoles(t *testing.T) {
	dir := t.TempDir()
	geth, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 7)
	if err != nil {
		t.Fatal(err)
	}
	rust, err := acquireRoleLeaseV2(dir, v2RoleRustOwner, 7)
	if err != nil {
		t.Fatal(err)
	}
	defer rust.release()
	if _, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 7); !errors.Is(err, errV2RoleLeaseHeld) {
		t.Fatalf("second geth-owner: %v", err)
	}
	if _, err := acquireRoleLeaseV2(dir, v2RoleRustOwner, 8); !errors.Is(err, errV2RoleLeaseHeld) {
		t.Fatalf("second rust-owner: %v", err)
	}
	if _, err := acquireRoleLeaseV2(dir, "observer", 7); err == nil {
		t.Fatal("unknown role accepted")
	}
	if _, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 0); err == nil {
		t.Fatal("zero generation accepted")
	}
	content, err := os.ReadFile(filepath.Join(dir, "geth-owner.lock"))
	if err != nil {
		t.Fatal(err)
	}
	if want := fmt.Sprintf("version=1\nrole=geth-owner\ngeneration=7\npid=%d\n", os.Getpid()); string(content) != want {
		t.Fatalf("lease content %q", content)
	}

	// Closing the owner releases the lock; the file stays.
	if err := geth.release(); err != nil {
		t.Fatal(err)
	}
	if err := geth.release(); err != nil {
		t.Fatal(err)
	}
	again, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 9)
	if err != nil {
		t.Fatal(err)
	}
	_ = again.release()
}

const leaseChildEnvV2 = "DARKBOT_FASTLANE_LEASE_CHILD_DIR"

// TestV2RoleLeaseChildHolder is the helper process of
// TestV2RoleLeaseReleasedWhenHolderProcessDies.
func TestV2RoleLeaseChildHolder(t *testing.T) {
	dir := os.Getenv(leaseChildEnvV2)
	if dir == "" {
		t.Skip("helper process only")
	}
	lease, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 1)
	if err != nil {
		fmt.Println("DARKBOT_LEASE_ERROR", err)
		os.Exit(2)
	}
	fmt.Println("DARKBOT_LEASE_LOCKED")
	time.Sleep(time.Minute) // killed by the parent
	_ = lease.release()
	os.Exit(0)
}

func TestV2RoleLeaseReleasedWhenHolderProcessDies(t *testing.T) {
	dir := t.TempDir()
	cmd := exec.Command(os.Args[0], "-test.run=^TestV2RoleLeaseChildHolder$", "-test.v")
	cmd.Env = append(os.Environ(), leaseChildEnvV2+"="+dir)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
	})
	reader := bufio.NewReader(stdout)
	for {
		line, err := reader.ReadString('\n')
		if strings.Contains(line, "DARKBOT_LEASE_LOCKED") {
			break
		}
		if err != nil {
			t.Fatalf("child did not take the lease: last=%q err=%v", line, err)
		}
	}
	if _, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 1); !errors.Is(err, errV2RoleLeaseHeld) {
		t.Fatalf("lease held by the child: %v", err)
	}
	if err := cmd.Process.Kill(); err != nil {
		t.Fatal(err)
	}
	_ = cmd.Wait()
	lease, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, 1)
	if err != nil {
		t.Fatalf("lease after the holder died: %v", err)
	}
	_ = lease.release()
}

// ---------------------------------------------------------------------------
// K15: generation, restart, O_EXCL primitives
// ---------------------------------------------------------------------------

func TestV2ExistingArenasDetectedNeverAttachedNeverOverwritten(t *testing.T) {
	dir := t.TempDir()
	cfg := testPeerLaneV2(150)
	current := arenaPathV2(dir, 41)
	if filepath.Base(current) != "fastlane-v2-g00000000000000000041.mmap" {
		t.Fatalf("arena path %s", current)
	}
	if probe, _, err := probeArenaV2(current, 41); err != nil || probe != arenaAbsentV2 {
		t.Fatalf("absent: probe=%d err=%v", probe, err)
	}
	if _, _, err := probeArenaV2(current, 0); err == nil {
		t.Fatal("zero generation accepted")
	}
	created, err := createArenaV2(current, 41, []laneConfigV2{cfg})
	if err != nil {
		t.Fatal(err)
	}
	_ = created.close()
	if probe, _, err := probeArenaV2(current, 41); err != nil || probe != arenaCurrentV2 {
		t.Fatalf("current: probe=%d err=%v", probe, err)
	}
	if probe, observed, err := probeArenaV2(current, 42); err != nil || probe != arenaStaleGenerationV2 || observed != 41 {
		t.Fatalf("stale: probe=%d observed=%d err=%v", probe, observed, err)
	}

	// O_EXCL on an existing arena: explicit result, bytes untouched.
	before, err := os.ReadFile(current)
	if err != nil {
		t.Fatal(err)
	}
	var exists *arenaExistsErrorV2
	if _, err := createArenaV2(current, 41, []laneConfigV2{cfg}); !errors.As(err, &exists) || exists.Probe != arenaCurrentV2 {
		t.Fatalf("existing current arena: %v", err)
	}
	if _, err := createArenaV2(current, 42, []laneConfigV2{cfg}); !errors.As(err, &exists) ||
		exists.Probe != arenaStaleGenerationV2 || exists.Observed != 41 {
		t.Fatalf("existing stale arena: %v", err)
	}
	if after, _ := os.ReadFile(current); !bytes.Equal(before, after) {
		t.Fatal("existing arena overwritten")
	}
	if a, err := openArenaV2(current, 42); err == nil {
		_ = a.close()
		t.Fatal("stale generation attached")
	}

	// A file that is not an arena.
	junk := filepath.Join(dir, "fastlane-v2-g00000000000000000007.mmap")
	if err := os.WriteFile(junk, []byte("not an arena"), 0o600); err != nil {
		t.Fatal(err)
	}
	if probe, _, err := probeArenaV2(junk, 7); err != nil || probe != arenaNotAnArenaV2 {
		t.Fatalf("junk: probe=%d err=%v", probe, err)
	}
	if _, err := createArenaV2(junk, 7, []laneConfigV2{cfg}); !errors.As(err, &exists) || exists.Probe != arenaNotAnArenaV2 {
		t.Fatalf("existing junk: %v", err)
	}
	if content, _ := os.ReadFile(junk); string(content) != "not an arena" {
		t.Fatal("junk overwritten")
	}

	// A new generation gets its own file beside the old ones.
	next, err := createArenaV2(arenaPathV2(dir, 42), 42, []laneConfigV2{cfg})
	if err != nil {
		t.Fatal(err)
	}
	_ = next.close()
	if err := os.WriteFile(filepath.Join(dir, "unrelated.mmap"), []byte("x"), 0o600); err != nil {
		t.Fatal(err)
	}
	stale, err := staleArenasV2(dir, 42)
	if err != nil {
		t.Fatal(err)
	}
	want := []staleArenaV2{{Generation: 7, Path: junk}, {Generation: 41, Path: current}}
	if len(stale) != len(want) || stale[0] != want[0] || stale[1] != want[1] {
		t.Fatalf("stale arenas %+v", stale)
	}
}

func TestV2UndefinedLifecycleStateQuarantinesEvenAnEmptyLane(t *testing.T) {
	// 160: empty, polled; 161: one record waiting, polled; 162: empty, acked.
	cfgs := []laneConfigV2{testPeerLaneV2(160), testPeerLaneV2(161), testPeerLaneV2(162)}
	arena, path := testCreateArenaV2(t, 160, cfgs...)
	if _, err := arena.bindPeerLane(161, testOwnerV2(1)); err != nil {
		t.Fatal(err)
	}
	producer := testOpenLaneV2(t, path, 160, cfgs[1])
	if err := producer.publish(testRecordV2([]byte("r"))); err != nil {
		t.Fatal(err)
	}
	for i, cfg := range cfgs {
		lane := testOpenLaneV2(t, path, 160, cfg)
		atomic.StoreUint32(v2AtomicU32(lane.arena.data, lane.offset+v2LHState), 99)
		var err error
		if i == 2 {
			_, err = lane.acknowledgeDrainedIfClosing()
		} else {
			_, _, err = lane.tryConsume()
		}
		var corrupt *corruptRecordErrorV2
		if !errors.As(err, &corrupt) || corrupt.Reason != v2QuarantineInvalidState {
			t.Fatalf("lane %d: undefined state returned %v", cfg.ID, err)
		}
		var quarantined *laneQuarantinedErrorV2
		for j := 0; j < 3; j++ {
			if _, _, err := lane.tryConsume(); !errors.As(err, &quarantined) || quarantined.Reason != v2QuarantineInvalidState {
				t.Fatalf("lane %d poll %d: %v", cfg.ID, j, err)
			}
		}
		if s := lane.stats(); s.State != v2LaneStateQuarantined || s.CorruptRecords != 1 ||
			s.QuarantineReason != v2QuarantineInvalidState || !s.Shutdown {
			t.Fatalf("lane %d: %+v", cfg.ID, s)
		}
	}
	// The waiting record was never consumed.
	if read := testStatsV2(t, arena, 161).ReadSequence; read != 0 {
		t.Fatalf("read=%d", read)
	}
}

// ---------------------------------------------------------------------------
// C1.3 Go<->Rust lifecycle interop. Four alternating processes on one arena,
// run in order by bsc/tools/c13_lifecycle_interop.sh:
//   geth phase 1 -> rust phase 1 -> geth phase 2 -> rust phase 2.
// Without DARKBOT_C13_INTEROP_DIR these tests skip.
// ---------------------------------------------------------------------------

const (
	interopEnvV2        = "DARKBOT_C13_INTEROP_DIR"
	interopGenerationV2 = uint64(1303)
)

// testInteropOwnerV2 is a non-uniform owner, so a word-order mismatch between
// Go and Rust cannot hide.
func testInteropOwnerV2(base byte) [32]byte {
	var owner [32]byte
	for i := range owner {
		owner[i] = base + byte(i)
	}
	return owner
}

func testInteropDirV2(t *testing.T) string {
	t.Helper()
	dir := os.Getenv(interopEnvV2)
	if dir == "" {
		t.Skip("C1.3 Go<->Rust interop phase; run by bsc/tools/c13_lifecycle_interop.sh")
	}
	return dir
}

// Phase 1: create, bind, publish, close lane 1; bind lane 2 and leave one
// record with a broken CRC for Rust to find.
func TestV2InteropGethPhase1(t *testing.T) {
	dir := testInteropDirV2(t)
	arena, err := createArenaV2(arenaPathV2(dir, interopGenerationV2), interopGenerationV2,
		[]laneConfigV2{testPeerLaneV2(1), testPeerLaneV2(2)})
	if err != nil {
		t.Fatal(err)
	}
	defer arena.close()
	ownerA := testInteropOwnerV2(0x10)
	epochA, err := arena.bindPeerLane(1, ownerA)
	if err != nil || epochA != 2 {
		t.Fatalf("bind lane 1: epoch=%d err=%v", epochA, err)
	}
	laneA, err := arena.lane(testPeerLaneV2(1))
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 3; i++ {
		if err := laneA.publish(testRecordV2([]byte(fmt.Sprintf("interop-%d", i)))); err != nil {
			t.Fatal(err)
		}
	}
	if final, err := arena.beginClosePeerLane(1, ownerA, epochA); err != nil || final != 3 {
		t.Fatalf("close lane 1: final=%d err=%v", final, err)
	}

	epochB, err := arena.bindPeerLane(2, testInteropOwnerV2(0x40))
	if err != nil || epochB != 2 {
		t.Fatalf("bind lane 2: epoch=%d err=%v", epochB, err)
	}
	laneB, err := arena.lane(testPeerLaneV2(2))
	if err != nil {
		t.Fatal(err)
	}
	if err := laneB.publish(testRecordV2([]byte("corrupt-me"))); err != nil {
		t.Fatal(err)
	}
	laneB.slot(0)[v2SlotHeaderSize] ^= 0x01
	fmt.Println("C13_INTEROP_GETH_PHASE1_OK")
}

// Phase 2: after Rust drained and acknowledged lane 1 and quarantined lane 2,
// release lane 1, bind it again with epoch+1 and publish one record; verify
// Rust's quarantine of lane 2 from the Go side.
func TestV2InteropGethPhase2(t *testing.T) {
	dir := testInteropDirV2(t)
	arena, err := openArenaV2(arenaPathV2(dir, interopGenerationV2), interopGenerationV2)
	if err != nil {
		t.Fatal(err)
	}
	defer arena.close()
	ownerA := testInteropOwnerV2(0x10)
	if s := testStatsV2(t, arena, 1); s.State != v2LaneStateClosing || s.DrainedEpoch != 2 ||
		s.ReadSequence != 3 || s.FinalWriteSequence != 3 || s.Owner != ownerA {
		t.Fatalf("lane 1 before release: %+v", s)
	}
	if err := arena.releasePeerLane(1, ownerA, 2); err != nil {
		t.Fatal(err)
	}
	if s := testStatsV2(t, arena, 1); s.State != v2LaneStateFree || s.Owner != ([32]byte{}) || s.FinalWriteSequence != 0 {
		t.Fatalf("lane 1 after release: %+v", s)
	}
	epochC, err := arena.bindPeerLane(1, testInteropOwnerV2(0x70))
	if err != nil || epochC != 3 {
		t.Fatalf("rebind lane 1: epoch=%d err=%v", epochC, err)
	}
	laneA, err := arena.lane(testPeerLaneV2(1))
	if err != nil {
		t.Fatal(err)
	}
	if err := laneA.publish(testRecordV2([]byte("interop-3"))); err != nil {
		t.Fatal(err)
	}

	if s := testStatsV2(t, arena, 2); s.State != v2LaneStateQuarantined || s.QuarantineReason != v2QuarantineChecksum ||
		s.CorruptRecords != 1 || s.ReadSequence != 0 || !s.Shutdown {
		t.Fatalf("lane 2 as quarantined by Rust: %+v", s)
	}
	laneB, err := arena.lane(testPeerLaneV2(2))
	if err != nil {
		t.Fatal(err)
	}
	testExpectStateV2(t, laneB.publish(testRecordV2([]byte("x"))), v2LaneStateBound, v2LaneStateQuarantined)
	_, err = arena.bindPeerLane(2, testInteropOwnerV2(0x50))
	testExpectStateV2(t, err, v2LaneStateFree, v2LaneStateQuarantined)
	fmt.Println("C13_INTEROP_GETH_PHASE2_OK")
}
