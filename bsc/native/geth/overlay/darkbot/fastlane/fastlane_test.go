package fastlane

import (
	"os"
	"path/filepath"
	"sync/atomic"
	"testing"

	"github.com/ethereum/go-ethereum/common"
)

func testRingPath(t *testing.T) string {
	t.Helper()
	return filepath.Join(t.TempDir(), "ring.mmap")
}

func TestRingRoundTripAndWrap(t *testing.T) {
	path := testRingPath(t)
	producer, err := createRing(path, 7, 4, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	defer producer.close()

	consumer, err := openRing(path, 7, 4, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	defer consumer.close()

	for i := 0; i < 32; i++ {
		payload := []byte{byte(i), 0xaa, 0x55}
		hash := common.BytesToHash([]byte{byte(i + 1)})
		if err := producer.publish(Record{
			Kind:        EarlyTxSeen,
			TimestampNS: uint64(1000 + i),
			Hash:        hash,
			Payload:     payload,
		}); err != nil {
			t.Fatalf("publish %d: %v", i, err)
		}
		record, ok, err := consumer.tryConsume()
		if err != nil {
			t.Fatalf("consume %d: %v", i, err)
		}
		if !ok {
			t.Fatalf("consume %d: no record", i)
		}
		if record.Sequence != uint64(i) {
			t.Fatalf("sequence %d != %d", record.Sequence, i)
		}
		if record.Hash != hash {
			t.Fatalf("hash mismatch at %d", i)
		}
		if string(record.Payload) != string(payload) {
			t.Fatalf("payload mismatch at %d", i)
		}
	}
}

func TestRingFullDoesNotOverwriteUnread(t *testing.T) {
	path := testRingPath(t)
	producer, err := createRing(path, 3, 2, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	defer producer.close()

	consumer, err := openRing(path, 3, 2, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	defer consumer.close()

	for i := 0; i < 2; i++ {
		if err := producer.publish(Record{Kind: EarlyTxSeen, Payload: []byte{byte(i)}}); err != nil {
			t.Fatal(err)
		}
	}
	if err := producer.publish(Record{Kind: EarlyTxSeen, Payload: []byte{9}}); err == nil {
		t.Fatal("expected full ring failure")
	}
	if got := atomic.LoadUint64(producer.headerU64(offDroppedFull)); got != 1 {
		t.Fatalf("dropped_full=%d", got)
	}

	for i := 0; i < 2; i++ {
		record, ok, err := consumer.tryConsume()
		if err != nil || !ok {
			t.Fatalf("consume %d: ok=%v err=%v", i, ok, err)
		}
		if len(record.Payload) != 1 || record.Payload[0] != byte(i) {
			t.Fatalf("unread slot overwritten at %d", i)
		}
	}
}

func TestIdentityMismatchFailsClosed(t *testing.T) {
	path := testRingPath(t)
	ring, err := createRing(path, 5, 4, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	if err := ring.close(); err != nil {
		t.Fatal(err)
	}

	if _, err := openRing(path, 6, 4, 1024, directionIngress); err == nil {
		t.Fatal("generation mismatch accepted")
	}
	if _, err := openRing(path, 5, 4, 1024, directionEgress); err == nil {
		t.Fatal("direction mismatch accepted")
	}
}

func TestCreateNewRefusesStaleRing(t *testing.T) {
	path := testRingPath(t)
	first, err := createRing(path, 1, 4, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	defer first.close()

	if _, err := createRing(path, 2, 4, 1024, directionIngress); err == nil {
		t.Fatal("stale ring was silently reused")
	}
}

func TestShutdownIsVisible(t *testing.T) {
	path := testRingPath(t)
	producer, err := createRing(path, 9, 4, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	defer producer.close()

	consumer, err := openRing(path, 9, 4, 1024, directionIngress)
	if err != nil {
		t.Fatal(err)
	}
	defer consumer.close()

	producer.setShutdown()
	if _, ok, err := consumer.tryConsume(); err == nil || ok {
		t.Fatalf("shutdown not observed: ok=%v err=%v", ok, err)
	}
}

func TestMain(m *testing.M) {
	global.Store(nil)
	code := m.Run()
	global.Store(nil)
	os.Exit(code)
}
