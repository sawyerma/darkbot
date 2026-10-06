package fastlane

import (
	"encoding/binary"
	"hash/crc32"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"sync/atomic"
	"testing"

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
		if record.Sequence != uint64(i) || record.LaneID != 10 || record.LaneEpoch != 1 {
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
			if _, _, err := c.tryConsume(); err == nil {
				t.Fatal("corruption accepted")
			}
			if got := atomic.LoadUint64(c.atomic(v2LHCorrupt)); got != 1 {
				t.Fatalf("corrupt_records=%d", got)
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
	finalWrite, err := arena.beginClosePeerLane(30, ownerA)
	if err != nil {
		t.Fatal(err)
	}
	if finalWrite != 1 {
		t.Fatalf("finalWrite=%d", finalWrite)
	}
	if err := arena.releasePeerLane(30, ownerA); err == nil {
		t.Fatal("released before drain")
	}
	if consumer.acknowledgeDrainedIfClosing() {
		t.Fatal("acknowledged before consuming final record")
	}
	if _, ok, err := consumer.tryConsume(); err != nil || !ok {
		t.Fatalf("consume ok=%v err=%v", ok, err)
	}
	if !consumer.acknowledgeDrainedIfClosing() {
		t.Fatal("drain was not acknowledged")
	}
	if err := arena.releasePeerLane(30, ownerA); err != nil {
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
	if _, err := arena.bindPeerLane(31, owner); err != nil {
		t.Fatal(err)
	}
	if _, err := arena.bindPeerLane(31, owner); err == nil {
		t.Fatal("double bind accepted")
	}
	if _, err := arena.beginClosePeerLane(31, wrong); err == nil {
		t.Fatal("wrong owner closed lane")
	}
	if _, err := arena.beginClosePeerLane(31, owner); err != nil {
		t.Fatal(err)
	}

	consumerArena, _ := openArenaV2(path, 22)
	defer consumerArena.close()
	consumer, _ := consumerArena.lane(cfg)
	if !consumer.acknowledgeDrainedIfClosing() {
		t.Fatal("empty closing lane did not acknowledge drain")
	}
	if err := arena.releasePeerLane(31, wrong); err == nil {
		t.Fatal("wrong owner released lane")
	}
	if err := arena.releasePeerLane(31, owner); err != nil {
		t.Fatal(err)
	}
}

