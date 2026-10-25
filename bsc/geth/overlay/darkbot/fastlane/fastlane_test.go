package fastlane

import (
	"testing"

	"golang.org/x/sys/unix"
)

// The record kinds are wire vocabulary shared with Rust (geth_ipc wire.rs
// RecordKind); their numbers never change.
func TestRecordKindWireValues(t *testing.T) {
	for want, kind := range map[uint16]RecordKind{
		1: EarlyTxSeen,
		2: TxpoolAccepted,
		3: CanonicalHead,
		4: Reorg,
		5: OutboundSignedTx,
		6: OutboundPeerHandoff,
		7: Gap,
		8: Shutdown,
	} {
		if uint16(kind) != want {
			t.Fatalf("record kind %d has value %d", want, kind)
		}
	}
}

func TestMonotonicNSReadsATime(t *testing.T) {
	first, err := MonotonicNS()
	if !clockReadingOKV2(first, err) {
		t.Fatalf("first reading %d, %v", first, err)
	}
	second, err := MonotonicNS()
	if !clockReadingOKV2(second, err) || second < first {
		t.Fatalf("second reading %d after %d, %v", second, first, err)
	}
}

// A failed or zero reading is never a time. unix.EINVAL is what
// clock_gettime reports for an unusable clock.
func TestClockReadingRefusesFailedAndZeroReadings(t *testing.T) {
	for _, tc := range []struct {
		now  uint64
		err  error
		want bool
	}{
		{now: 1, want: true},
		{now: 0},
		{now: 1, err: unix.EINVAL},
		{now: 0, err: unix.EINVAL},
	} {
		if got := clockReadingOKV2(tc.now, tc.err); got != tc.want {
			t.Fatalf("reading %d, %v: ok=%v", tc.now, tc.err, got)
		}
	}
}
