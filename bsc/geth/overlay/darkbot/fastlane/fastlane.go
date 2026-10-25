package fastlane

// Shared by every Wire-V2 lane: the record-kind vocabulary (the same values
// as the Rust wire vocabulary) and the monotonic clock. The V1 ring and its
// API were removed (BSC-045); producers and consumers run on Wire V2 only.

import (
	"errors"

	"golang.org/x/sys/unix"
)

// RecordKind is the kind field of a Wire-V2 record.
type RecordKind uint16

const (
	EarlyTxSeen         RecordKind = 1
	TxpoolAccepted      RecordKind = 2
	CanonicalHead       RecordKind = 3
	Reorg               RecordKind = 4
	OutboundSignedTx    RecordKind = 5
	OutboundPeerHandoff RecordKind = 6
	Gap                 RecordKind = 7
	Shutdown            RecordKind = 8
)

// MonotonicNS reads CLOCK_MONOTONIC (clock id 1 of a Wire-V2 record).
func MonotonicNS() (uint64, error) {
	var ts unix.Timespec
	if err := unix.ClockGettime(unix.CLOCK_MONOTONIC, &ts); err != nil {
		return 0, err
	}
	if ts.Sec < 0 || ts.Nsec < 0 {
		return 0, errors.New("negative CLOCK_MONOTONIC result")
	}
	return uint64(ts.Sec)*1_000_000_000 + uint64(ts.Nsec), nil
}

// clockReadingOKV2 reports whether one MonotonicNS reading is a time. A
// failed or zero reading never is, and never stamps evidence.
func clockReadingOKV2(now uint64, err error) bool {
	return err == nil && now != 0
}
