package fastlane

import (
	"encoding/binary"
	"errors"
	"fmt"
	"hash/crc32"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"unsafe"

	"github.com/ethereum/go-ethereum/common"
	"golang.org/x/sys/unix"
)

const (
	v2Magic           = "DBFLN002"
	v2Version         = uint16(2)
	v2EndianMarker    = uint16(0x0102)
	v2Alignment       = 128
	v2SuperHeaderSize = 256
	v2DirEntrySize    = 128
	v2LaneHeaderSize  = 384
	v2SlotHeaderSize  = 256
	v2MinSlotStride   = 512
	v2MaxSlotStride   = 256 * 1024
	v2MaxCapacity     = 4096
	v2MaxLanes        = 1024
	v2CRCKindCRC32C   = uint32(1)
	v2ClockMonotonic  = uint32(1)

	v2LaneStateFree        = uint32(0)
	v2LaneStateBound       = uint32(1)
	v2LaneStateClosing     = uint32(2)
	v2LaneStateQuarantined = uint32(3)

	v2SHMagic          = 0
	v2SHVersion        = 8
	v2SHEndian         = 10
	v2SHHeaderSize     = 12
	v2SHGeneration     = 16
	v2SHLaneCount      = 24
	v2SHDirSize        = 28
	v2SHLaneHeaderSize = 32
	v2SHSlotHeaderSize = 36
	v2SHAlignment      = 40
	v2SHCRCKind        = 44
	v2SHTotalBytes     = 48

	v2DELaneID     = 0
	v2DEDirection  = 2
	v2DEClass      = 4
	v2DEFlags      = 6
	v2DECapacity   = 8
	v2DESlotStride = 12
	v2DELaneOffset = 16
	v2DELaneBytes  = 24

	v2LHWriteSeq    = 0
	v2LHDroppedFull = 8
	v2LHOversize    = 16
	v2LHProducer    = 24
	v2LHReadSeq     = 128
	v2LHCorrupt     = 136
	v2LHDrained     = 144
	v2LHGeneration  = 256
	v2LHEpoch       = 264
	v2LHState       = 272
	v2LHFinalWrite  = 280
	v2LHShutdown    = 288
	v2LHOwner       = 296
	// Reserved in C1.2; named by C1.3. Written once by the first quarantining
	// party; zero means none.
	v2LHQuarantineReason = 328

	v2SlotCommitted = 0
	v2SlotKind      = 8
	v2SlotFlags     = 10
	v2SlotPayload   = 12
	v2SlotTimestamp = 16
	v2SlotClockID   = 24
	v2SlotSource    = 32
	v2SlotPeer      = 48
	v2SlotHash      = 80
	v2SlotAux       = 112
	v2SlotEpoch     = 144
	v2SlotCRC32C    = 152

	v2EmptySequence = ^uint64(0)
)

// Quarantine reasons, identical to the Rust QUARANTINE_* constants.
const (
	v2QuarantineNone           = uint64(0)
	v2QuarantineCorruptSeq     = uint64(1)
	v2QuarantinePayloadLength  = uint64(2)
	v2QuarantineEpochMismatch  = uint64(3)
	v2QuarantineChecksum       = uint64(4)
	v2QuarantineRecordKind     = uint64(5)
	v2QuarantineLateWrite      = uint64(6)
	v2QuarantineInvalidState   = uint64(7)
	v2QuarantineRestart        = uint64(8)
	v2QuarantineOperator       = uint64(9)
	v2RoleLeaseVersion         = 1
	v2RoleGethOwner            = "geth-owner"
	v2RoleRustOwner            = "rust-owner"
	v2ArenaFilePrefix          = "fastlane-v2-g"
	v2ArenaFileSuffix          = ".mmap"
	v2ArenaFileGenerationWidth = 20
)

type laneDirectionV2 uint16

const (
	v2GethToRust laneDirectionV2 = 1
	v2RustToGeth laneDirectionV2 = 2
)

type laneClassV2 uint16

const (
	v2PeerIngress             laneClassV2 = 1
	v2TxpoolEvidence          laneClassV2 = 2
	v2ChainEvidence           laneClassV2 = 3
	v2OutboundHandoffEvidence laneClassV2 = 4
	v2SignedTransactionEgress laneClassV2 = 5
)

type laneConfigV2 struct {
	ID         uint16
	Direction  laneDirectionV2
	Class      laneClassV2
	Flags      uint16
	Capacity   uint32
	SlotStride uint32
}

type recordV2 struct {
	Sequence    uint64
	LaneID      uint16
	LaneEpoch   uint64
	Kind        RecordKind
	Flags       uint16
	TimestampNS uint64
	ClockID     uint32
	Source      [16]byte
	Peer        [32]byte
	Hash        common.Hash
	Aux         [32]byte
	Payload     []byte
}

type arenaV2 struct {
	file *os.File
	data []byte
}

type laneV2 struct {
	arena  *arenaV2
	config laneConfigV2
	offset int
}

var v2CRC32CTable = crc32.MakeTable(crc32.Castagnoli)

// C1.3 lane lifecycle. Every lifecycle field (lane_epoch, state,
// final_write_sequence, shutdown, the four owner words, drained_epoch) is
// accessed atomically only; the byte layout is the frozen C1.2 one. State
// changes are compare-and-swap transitions; the field writes that precede a
// transition are published by it. A lease is (lane id, owner, epoch): close
// and release need all three, so a lease of an earlier binding never acts on
// a reused lane. The arena's lifecycle calls are not internally serialized:
// one allocator (peerLanePoolV2, behind its mutex) owns them.

var (
	errV2LifecycleState = errors.New("fastlane v2 lifecycle state")
	errV2OwnerMismatch  = errors.New("fastlane v2 owner mismatch")
	errV2LeaseEpoch     = errors.New("fastlane v2 lease epoch mismatch")
	errV2NotDrained     = errors.New("fastlane v2 lane not drained")
	errV2LateWrite      = errors.New("fastlane v2 write after close")
	errV2EpochExhausted = errors.New("fastlane v2 lane epoch exhausted")
	errV2NotPeerLane    = errors.New("fastlane v2 lifecycle requires peer-ingress lane")
	errV2PoolExhausted  = errors.New("fastlane v2 no free peer lane")
	errV2PeerLimit      = errors.New("fastlane v2 peer limit reached")
	errV2PoolBounds     = errors.New("fastlane v2 pool bounds do not match the arena")
	errV2BigEndianHost  = errors.New("fastlane v2 requires a little-endian host")
	errV2Shutdown       = errors.New("fastlane v2 shutdown")
	errV2Corrupt        = errors.New("fastlane v2 corrupt record")
	errV2Quarantined    = errors.New("fastlane v2 lane quarantined")
	errV2RoleLeaseHeld  = errors.New("fastlane v2 role lease held by another process")
	errV2ArenaExists    = errors.New("fastlane v2 arena already exists")
	errV2RuntimeActive  = errors.New("fastlane v2 runtime already installed")
	// Same texts as before C1.4; now sentinels so that callers can count
	// them without comparing strings.
	errV2QueueFull       = errors.New("fastlane v2 queue full")
	errV2PayloadTooLarge = errors.New("fastlane v2 payload too large")
)

// corruptRecordErrorV2: the first structural defect of a record. The lane is
// quarantined and the incident counted once.
type corruptRecordErrorV2 struct {
	Reason uint64
	Detail string
}

func (e *corruptRecordErrorV2) Error() string {
	return "fastlane v2 " + e.Detail + "; lane quarantined"
}

func (e *corruptRecordErrorV2) Is(target error) bool { return target == errV2Corrupt }

// laneQuarantinedErrorV2: the lane is QUARANTINED; it is not consumed, bound
// or reused in this generation.
type laneQuarantinedErrorV2 struct {
	Reason uint64
}

func (e *laneQuarantinedErrorV2) Error() string {
	return fmt.Sprintf("fastlane v2 lane quarantined (reason %d)", e.Reason)
}

func (e *laneQuarantinedErrorV2) Is(target error) bool { return target == errV2Quarantined }

// arenaProbeV2 classifies what lies at an arena path.
type arenaProbeV2 int

const (
	arenaAbsentV2 arenaProbeV2 = iota
	arenaCurrentV2
	arenaStaleGenerationV2
	arenaNotAnArenaV2
)

// arenaExistsErrorV2: create found an existing file; it was neither attached
// nor overwritten.
type arenaExistsErrorV2 struct {
	Probe    arenaProbeV2
	Observed uint64 // generation, for arenaStaleGenerationV2
}

func (e *arenaExistsErrorV2) Error() string {
	return fmt.Sprintf("fastlane v2 arena already exists (probe %d, generation %d)", e.Probe, e.Observed)
}

func (e *arenaExistsErrorV2) Is(target error) bool { return target == errV2ArenaExists }

// laneStateErrorV2: a lifecycle call found another state than it requires.
type laneStateErrorV2 struct {
	Expected uint32
	Observed uint32
}

func (e *laneStateErrorV2) Error() string {
	return fmt.Sprintf("fastlane v2 lifecycle state %d != %d", e.Observed, e.Expected)
}

func (e *laneStateErrorV2) Is(target error) bool { return target == errV2LifecycleState }

// leaseEpochErrorV2: the caller's lease names another binding of the lane.
type leaseEpochErrorV2 struct {
	Expected uint64 // the lane's epoch
	Observed uint64 // the lease's epoch
}

func (e *leaseEpochErrorV2) Error() string {
	return fmt.Sprintf("fastlane v2 lease epoch %d != lane epoch %d", e.Observed, e.Expected)
}

func (e *leaseEpochErrorV2) Is(target error) bool { return target == errV2LeaseEpoch }

// notDrainedErrorV2: release before the drain of this epoch was acknowledged.
type notDrainedErrorV2 struct {
	Read       uint64
	FinalWrite uint64
}

func (e *notDrainedErrorV2) Error() string {
	return fmt.Sprintf("fastlane v2 lane not drained read=%d final_write=%d", e.Read, e.FinalWrite)
}

func (e *notDrainedErrorV2) Is(target error) bool { return target == errV2NotDrained }

// lateWriteErrorV2: a record past final_write_sequence; the lane is
// quarantined.
type lateWriteErrorV2 struct {
	FinalWrite uint64
	Write      uint64
}

func (e *lateWriteErrorV2) Error() string {
	return fmt.Sprintf("fastlane v2 write %d after close at %d; lane quarantined", e.Write, e.FinalWrite)
}

func (e *lateWriteErrorV2) Is(target error) bool { return target == errV2LateWrite }

// poolExhaustedErrorV2: no FREE peer lane. Nothing was waited for.
type poolExhaustedErrorV2 struct {
	PeerLanes int
}

func (e *poolExhaustedErrorV2) Error() string {
	return fmt.Sprintf("fastlane v2 no free peer lane among %d", e.PeerLanes)
}

func (e *poolExhaustedErrorV2) Is(target error) bool { return target == errV2PoolExhausted }

func v2StateError(expected, observed uint32) error {
	return &laneStateErrorV2{Expected: expected, Observed: observed}
}

// hostLittleEndianV2 reports whether atomic 64-bit stores produce the wire's
// little-endian byte order on this host.
func hostLittleEndianV2() bool {
	probe := uint16(1)
	return *(*byte)(unsafe.Pointer(&probe)) == 1
}

// laneStatsV2 is a diagnostic snapshot; each field is read atomically, the
// snapshot as a whole is not.
type laneStatsV2 struct {
	WriteSequence      uint64
	ReadSequence       uint64
	DroppedFull        uint64
	Oversize           uint64
	CorruptRecords     uint64
	LaneEpoch          uint64
	Generation         uint64
	Shutdown           bool
	State              uint32
	FinalWriteSequence uint64
	DrainedEpoch       uint64
	Owner              [32]byte
	QuarantineReason   uint64
}

func validateLaneConfigV2(c laneConfigV2) error {
	if c.Capacity == 0 || c.Capacity > v2MaxCapacity {
		return fmt.Errorf("fastlane v2 invalid capacity %d", c.Capacity)
	}
	if c.SlotStride < v2MinSlotStride || c.SlotStride > v2MaxSlotStride ||
		c.SlotStride%v2Alignment != 0 || c.SlotStride <= v2SlotHeaderSize {
		return fmt.Errorf("fastlane v2 invalid slot stride %d", c.SlotStride)
	}
	return nil
}

func laneBytesV2(c laneConfigV2) (uint64, error) {
	if err := validateLaneConfigV2(c); err != nil {
		return 0, err
	}
	return uint64(v2LaneHeaderSize) + uint64(c.Capacity)*uint64(c.SlotStride), nil
}

func alignUpV2(v uint64) (uint64, error) {
	if v > ^uint64(0)-uint64(v2Alignment-1) {
		return 0, errors.New("fastlane v2 size overflow")
	}
	return (v + uint64(v2Alignment-1)) &^ uint64(v2Alignment-1), nil
}

func createArenaV2(path string, generation uint64, lanes []laneConfigV2) (*arenaV2, error) {
	if !hostLittleEndianV2() {
		return nil, errV2BigEndianHost
	}
	if generation == 0 {
		return nil, errors.New("fastlane v2 generation must be nonzero")
	}
	if len(lanes) == 0 || len(lanes) > v2MaxLanes {
		return nil, fmt.Errorf("fastlane v2 invalid lane count %d", len(lanes))
	}
	seen := make(map[uint16]struct{}, len(lanes))
	cursor, err := alignUpV2(uint64(v2SuperHeaderSize + len(lanes)*v2DirEntrySize))
	if err != nil {
		return nil, err
	}
	offsets := make([]uint64, len(lanes))
	lengths := make([]uint64, len(lanes))
	for i, lane := range lanes {
		if _, ok := seen[lane.ID]; ok {
			return nil, fmt.Errorf("fastlane v2 duplicate lane id %d", lane.ID)
		}
		seen[lane.ID] = struct{}{}
		length, err := laneBytesV2(lane)
		if err != nil {
			return nil, err
		}
		offsets[i] = cursor
		lengths[i] = length
		cursor, err = alignUpV2(cursor + length)
		if err != nil {
			return nil, err
		}
	}
	if cursor > uint64(^uint(0)>>1) {
		return nil, errors.New("fastlane v2 arena too large")
	}

	file, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_RDWR, 0o600)
	if errors.Is(err, fs.ErrExist) {
		probe, observed, probeErr := probeArenaV2(path, generation)
		if probeErr != nil {
			return nil, probeErr
		}
		return nil, &arenaExistsErrorV2{Probe: probe, Observed: observed}
	}
	if err != nil {
		return nil, err
	}
	if err := file.Truncate(int64(cursor)); err != nil {
		_ = file.Close()
		return nil, err
	}
	data, err := unix.Mmap(int(file.Fd()), 0, int(cursor), unix.PROT_READ|unix.PROT_WRITE, unix.MAP_SHARED)
	if err != nil {
		_ = file.Close()
		return nil, err
	}
	clear(data)
	a := &arenaV2{file: file, data: data}
	copy(data[v2SHMagic:v2SHMagic+8], []byte(v2Magic))
	binary.LittleEndian.PutUint16(data[v2SHVersion:], v2Version)
	binary.LittleEndian.PutUint16(data[v2SHEndian:], v2EndianMarker)
	binary.LittleEndian.PutUint32(data[v2SHHeaderSize:], v2SuperHeaderSize)
	binary.LittleEndian.PutUint64(data[v2SHGeneration:], generation)
	binary.LittleEndian.PutUint32(data[v2SHLaneCount:], uint32(len(lanes)))
	binary.LittleEndian.PutUint32(data[v2SHDirSize:], v2DirEntrySize)
	binary.LittleEndian.PutUint32(data[v2SHLaneHeaderSize:], v2LaneHeaderSize)
	binary.LittleEndian.PutUint32(data[v2SHSlotHeaderSize:], v2SlotHeaderSize)
	binary.LittleEndian.PutUint32(data[v2SHAlignment:], v2Alignment)
	binary.LittleEndian.PutUint32(data[v2SHCRCKind:], v2CRCKindCRC32C)
	binary.LittleEndian.PutUint64(data[v2SHTotalBytes:], cursor)

	for i, lane := range lanes {
		d := v2SuperHeaderSize + i*v2DirEntrySize
		binary.LittleEndian.PutUint16(data[d+v2DELaneID:], lane.ID)
		binary.LittleEndian.PutUint16(data[d+v2DEDirection:], uint16(lane.Direction))
		binary.LittleEndian.PutUint16(data[d+v2DEClass:], uint16(lane.Class))
		binary.LittleEndian.PutUint16(data[d+v2DEFlags:], lane.Flags)
		binary.LittleEndian.PutUint32(data[d+v2DECapacity:], lane.Capacity)
		binary.LittleEndian.PutUint32(data[d+v2DESlotStride:], lane.SlotStride)
		binary.LittleEndian.PutUint64(data[d+v2DELaneOffset:], offsets[i])
		binary.LittleEndian.PutUint64(data[d+v2DELaneBytes:], lengths[i])

		o := int(offsets[i])
		binary.LittleEndian.PutUint64(data[o+v2LHGeneration:], generation)
		binary.LittleEndian.PutUint64(data[o+v2LHEpoch:], 1)
		initialState := v2LaneStateBound
		if lane.Class == v2PeerIngress {
			initialState = v2LaneStateFree
		}
		binary.LittleEndian.PutUint32(data[o+v2LHState:], initialState)
		for slot := uint32(0); slot < lane.Capacity; slot++ {
			s := o + v2LaneHeaderSize + int(slot)*int(lane.SlotStride)
			atomic.StoreUint64(v2AtomicU64(data, s+v2SlotCommitted), v2EmptySequence)
		}
	}
	if err := unix.Msync(data, unix.MS_SYNC); err != nil {
		_ = a.close()
		return nil, err
	}
	return a, nil
}

func openArenaV2(path string, generation uint64) (*arenaV2, error) {
	if !hostLittleEndianV2() {
		return nil, errV2BigEndianHost
	}
	if generation == 0 {
		return nil, errors.New("fastlane v2 generation must be nonzero")
	}
	file, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		return nil, err
	}
	info, err := file.Stat()
	if err != nil {
		_ = file.Close()
		return nil, err
	}
	if info.Size() < v2SuperHeaderSize {
		_ = file.Close()
		return nil, errors.New("fastlane v2 short arena")
	}
	data, err := unix.Mmap(int(file.Fd()), 0, int(info.Size()), unix.PROT_READ|unix.PROT_WRITE, unix.MAP_SHARED)
	if err != nil {
		_ = file.Close()
		return nil, err
	}
	a := &arenaV2{file: file, data: data}
	if err := a.validate(generation); err != nil {
		_ = a.close()
		return nil, err
	}
	// Every directory entry must describe a valid lane of this generation.
	seen := make(map[uint16]struct{})
	for _, id := range a.laneIDs() {
		if _, ok := seen[id]; ok {
			_ = a.close()
			return nil, fmt.Errorf("fastlane v2 duplicate lane id %d", id)
		}
		seen[id] = struct{}{}
		if _, err := a.laneByID(id); err != nil {
			_ = a.close()
			return nil, err
		}
	}
	return a, nil
}

func (a *arenaV2) validate(generation uint64) error {
	if string(a.data[v2SHMagic:v2SHMagic+8]) != v2Magic {
		return errors.New("fastlane v2 bad magic")
	}
	if binary.LittleEndian.Uint16(a.data[v2SHVersion:]) != v2Version {
		return errors.New("fastlane v2 version mismatch")
	}
	if binary.LittleEndian.Uint16(a.data[v2SHEndian:]) != v2EndianMarker {
		return errors.New("fastlane v2 endian mismatch")
	}
	if binary.LittleEndian.Uint32(a.data[v2SHHeaderSize:]) != v2SuperHeaderSize ||
		binary.LittleEndian.Uint32(a.data[v2SHDirSize:]) != v2DirEntrySize ||
		binary.LittleEndian.Uint32(a.data[v2SHLaneHeaderSize:]) != v2LaneHeaderSize ||
		binary.LittleEndian.Uint32(a.data[v2SHSlotHeaderSize:]) != v2SlotHeaderSize ||
		binary.LittleEndian.Uint32(a.data[v2SHAlignment:]) != v2Alignment ||
		binary.LittleEndian.Uint32(a.data[v2SHCRCKind:]) != v2CRCKindCRC32C {
		return errors.New("fastlane v2 layout mismatch")
	}
	if binary.LittleEndian.Uint64(a.data[v2SHGeneration:]) != generation {
		return errors.New("fastlane v2 generation mismatch")
	}
	if binary.LittleEndian.Uint64(a.data[v2SHTotalBytes:]) != uint64(len(a.data)) {
		return errors.New("fastlane v2 length mismatch")
	}
	count := binary.LittleEndian.Uint32(a.data[v2SHLaneCount:])
	if count == 0 || count > v2MaxLanes {
		return errors.New("fastlane v2 lane count mismatch")
	}
	if v2SuperHeaderSize+int(count)*v2DirEntrySize > len(a.data) {
		return errors.New("fastlane v2 directory out of bounds")
	}
	return nil
}

func (a *arenaV2) lane(expected laneConfigV2) (*laneV2, error) {
	if err := validateLaneConfigV2(expected); err != nil {
		return nil, err
	}
	count := int(binary.LittleEndian.Uint32(a.data[v2SHLaneCount:]))
	for i := 0; i < count; i++ {
		d := v2SuperHeaderSize + i*v2DirEntrySize
		if binary.LittleEndian.Uint16(a.data[d+v2DELaneID:]) != expected.ID {
			continue
		}
		if binary.LittleEndian.Uint16(a.data[d+v2DEDirection:]) != uint16(expected.Direction) ||
			binary.LittleEndian.Uint16(a.data[d+v2DEClass:]) != uint16(expected.Class) ||
			binary.LittleEndian.Uint16(a.data[d+v2DEFlags:]) != expected.Flags ||
			binary.LittleEndian.Uint32(a.data[d+v2DECapacity:]) != expected.Capacity ||
			binary.LittleEndian.Uint32(a.data[d+v2DESlotStride:]) != expected.SlotStride {
			return nil, errors.New("fastlane v2 lane config mismatch")
		}
		offset := binary.LittleEndian.Uint64(a.data[d+v2DELaneOffset:])
		length := binary.LittleEndian.Uint64(a.data[d+v2DELaneBytes:])
		wantLength, _ := laneBytesV2(expected)
		if offset%v2Alignment != 0 || length != wantLength || offset+length > uint64(len(a.data)) {
			return nil, errors.New("fastlane v2 lane range mismatch")
		}
		o := int(offset)
		if binary.LittleEndian.Uint64(a.data[o+v2LHGeneration:]) != binary.LittleEndian.Uint64(a.data[v2SHGeneration:]) {
			return nil, errors.New("fastlane v2 lane generation mismatch")
		}
		return &laneV2{arena: a, config: expected, offset: o}, nil
	}
	return nil, fmt.Errorf("fastlane v2 lane %d not found", expected.ID)
}

// bindPeerLane: FREE -> BOUND for a new peer. Refused after shutdown.
// Increments lane_epoch (fails closed on exhaustion), writes the owner, clears
// final_write_sequence and publishes BOUND. Returns the new epoch, which with
// the owner forms the lease. The previous binding's drained_epoch needs no
// clearing: it names an older epoch, which never equals the new one. A FREE
// lane holding unread records breaks the release invariant: it is
// quarantined and the bind fails.
func (a *arenaV2) bindPeerLane(id uint16, owner [32]byte) (uint64, error) {
	lane, err := a.peerLane(id)
	if err != nil {
		return 0, err
	}
	if state := lane.state(); state != v2LaneStateFree {
		return 0, v2StateError(v2LaneStateFree, state)
	}
	// Shutdown is terminal: set by a runtime shutdown or a quarantine, never
	// cleared, so no owner is bound after it began.
	if atomic.LoadUint64(lane.atomic(v2LHShutdown)) != 0 {
		return 0, errV2Shutdown
	}
	epoch := atomic.LoadUint64(lane.atomic(v2LHEpoch))
	if epoch == ^uint64(0) {
		return 0, errV2EpochExhausted
	}
	write := atomic.LoadUint64(lane.atomic(v2LHWriteSeq))
	read := atomic.LoadUint64(lane.atomic(v2LHReadSeq))
	if write != read {
		lane.quarantineFor(v2QuarantineLateWrite)
		return 0, &lateWriteErrorV2{FinalWrite: read, Write: write}
	}
	next := epoch + 1
	lane.storeOwner(owner)
	atomic.StoreUint64(lane.atomic(v2LHFinalWrite), 0)
	atomic.StoreUint64(lane.atomic(v2LHEpoch), next)
	if err := lane.transition(v2LaneStateFree, v2LaneStateBound); err != nil {
		return 0, err
	}
	return next, nil
}

// beginClosePeerLane: BOUND -> CLOSING. The caller is the lane's producer and
// has published its last record: final_write_sequence is frozen at the
// current write_sequence before CLOSING is published. Requires the lease's
// owner and epoch. Returns final_write_sequence.
func (a *arenaV2) beginClosePeerLane(id uint16, owner [32]byte, epoch uint64) (uint64, error) {
	lane, err := a.peerLane(id)
	if err != nil {
		return 0, err
	}
	if state := lane.state(); state != v2LaneStateBound {
		return 0, v2StateError(v2LaneStateBound, state)
	}
	if err := lane.checkLease(owner, epoch); err != nil {
		return 0, err
	}
	finalWrite := atomic.LoadUint64(lane.atomic(v2LHWriteSeq))
	atomic.StoreUint64(lane.atomic(v2LHFinalWrite), finalWrite)
	if err := lane.transition(v2LaneStateBound, v2LaneStateClosing); err != nil {
		return 0, err
	}
	return finalWrite, nil
}

// releasePeerLane: CLOSING -> FREE, only after the consumer acknowledged the
// drain of this epoch: requires the lease's owner and epoch,
// drained_epoch == lane_epoch and
// read_sequence == final_write_sequence == write_sequence. A write past
// final_write_sequence quarantines the lane. Clears the owner and
// final_write_sequence, then publishes FREE.
func (a *arenaV2) releasePeerLane(id uint16, owner [32]byte, epoch uint64) error {
	lane, err := a.peerLane(id)
	if err != nil {
		return err
	}
	if state := lane.state(); state != v2LaneStateClosing {
		return v2StateError(v2LaneStateClosing, state)
	}
	if err := lane.checkLease(owner, epoch); err != nil {
		return err
	}
	finalWrite := atomic.LoadUint64(lane.atomic(v2LHFinalWrite))
	write := atomic.LoadUint64(lane.atomic(v2LHWriteSeq))
	if write != finalWrite {
		lane.quarantineFor(v2QuarantineLateWrite)
		return &lateWriteErrorV2{FinalWrite: finalWrite, Write: write}
	}
	read := atomic.LoadUint64(lane.atomic(v2LHReadSeq))
	drained := atomic.LoadUint64(lane.atomic(v2LHDrained))
	if drained != epoch || read != finalWrite {
		return &notDrainedErrorV2{Read: read, FinalWrite: finalWrite}
	}
	lane.storeOwner([32]byte{})
	atomic.StoreUint64(lane.atomic(v2LHFinalWrite), 0)
	return lane.transition(v2LaneStateClosing, v2LaneStateFree)
}

// quarantineLane quarantines one lane of any class (reason operator):
// shutdown = 1, state QUARANTINED. Terminal for this generation; idempotent.
// Returns the previous state.
func (a *arenaV2) quarantineLane(id uint16) (uint32, error) {
	lane, err := a.laneByID(id)
	if err != nil {
		return 0, err
	}
	return lane.quarantine(), nil
}

// quarantineAll quarantines every lane of this arena (reason restart), as a
// runtime restart does with the previous generation before abandoning it.
// Returns how many lanes were not quarantined before. Destroys no record.
func (a *arenaV2) quarantineAll() (int, error) {
	newly := 0
	for _, id := range a.laneIDs() {
		lane, err := a.laneByID(id)
		if err != nil {
			return newly, err
		}
		if _, made := lane.quarantineFor(v2QuarantineRestart); made {
			newly++
		}
	}
	return newly, nil
}

// beginShutdown sets shutdown = 1 on every lane. From then on every bind and
// every publish of this arena is refused; records already written stay
// readable until the consumer drained them. Idempotent. Nothing is unmapped.
func (a *arenaV2) beginShutdown() error {
	for _, id := range a.laneIDs() {
		lane, err := a.laneByID(id)
		if err != nil {
			return err
		}
		atomic.StoreUint64(lane.atomic(v2LHShutdown), 1)
	}
	return nil
}

func (a *arenaV2) generation() uint64 {
	return binary.LittleEndian.Uint64(a.data[v2SHGeneration:])
}

// laneIDs returns the lane ids in directory order.
func (a *arenaV2) laneIDs() []uint16 {
	count := int(binary.LittleEndian.Uint32(a.data[v2SHLaneCount:]))
	ids := make([]uint16, 0, count)
	for i := 0; i < count; i++ {
		ids = append(ids, binary.LittleEndian.Uint16(a.data[v2SuperHeaderSize+i*v2DirEntrySize+v2DELaneID:]))
	}
	return ids
}

// peerLaneIDs returns the ids of all peer-ingress lanes, in directory order.
func (a *arenaV2) peerLaneIDs() ([]uint16, error) {
	var ids []uint16
	for _, id := range a.laneIDs() {
		lane, err := a.laneByID(id)
		if err != nil {
			return nil, err
		}
		if lane.config.Class == v2PeerIngress {
			ids = append(ids, id)
		}
	}
	return ids, nil
}

func (a *arenaV2) peerLane(id uint16) (*laneV2, error) {
	lane, err := a.laneByID(id)
	if err != nil {
		return nil, err
	}
	if lane.config.Class != v2PeerIngress {
		return nil, errV2NotPeerLane
	}
	return lane, nil
}

func (a *arenaV2) laneStats(id uint16) (laneStatsV2, error) {
	lane, err := a.laneByID(id)
	if err != nil {
		return laneStatsV2{}, err
	}
	return lane.stats(), nil
}

func (a *arenaV2) laneByID(id uint16) (*laneV2, error) {
	count := int(binary.LittleEndian.Uint32(a.data[v2SHLaneCount:]))
	for i := 0; i < count; i++ {
		d := v2SuperHeaderSize + i*v2DirEntrySize
		if binary.LittleEndian.Uint16(a.data[d+v2DELaneID:]) != id {
			continue
		}
		cfg := laneConfigV2{
			ID:         id,
			Direction:  laneDirectionV2(binary.LittleEndian.Uint16(a.data[d+v2DEDirection:])),
			Class:      laneClassV2(binary.LittleEndian.Uint16(a.data[d+v2DEClass:])),
			Flags:      binary.LittleEndian.Uint16(a.data[d+v2DEFlags:]),
			Capacity:   binary.LittleEndian.Uint32(a.data[d+v2DECapacity:]),
			SlotStride: binary.LittleEndian.Uint32(a.data[d+v2DESlotStride:]),
		}
		return a.lane(cfg)
	}
	return nil, fmt.Errorf("fastlane v2 lane %d not found", id)
}

func (l *laneV2) publish(record recordV2) error {
	return l.publishChecked(record, 0, false)
}

// publishLeased publishes only while the lane is still bound to leaseEpoch.
// A producer handle of an earlier binding is refused before it writes
// anything, so it can never write into a later binding of the lane (C1.4).
func (l *laneV2) publishLeased(record recordV2, leaseEpoch uint64) error {
	return l.publishChecked(record, leaseEpoch, true)
}

func (l *laneV2) publishChecked(record recordV2, leaseEpoch uint64, leased bool) error {
	state := atomic.LoadUint32(v2AtomicU32(l.arena.data, l.offset+v2LHState))
	if state != v2LaneStateBound {
		return v2StateError(v2LaneStateBound, state)
	}
	epoch := atomic.LoadUint64(l.atomic(v2LHEpoch))
	if leased && epoch != leaseEpoch {
		return &leaseEpochErrorV2{Expected: epoch, Observed: leaseEpoch}
	}
	maxPayload := int(l.config.SlotStride) - v2SlotHeaderSize
	if len(record.Payload) > maxPayload {
		atomic.AddUint64(l.atomic(v2LHOversize), 1)
		return errV2PayloadTooLarge
	}
	if atomic.LoadUint64(l.atomic(v2LHShutdown)) != 0 {
		return errV2Shutdown
	}
	write := atomic.LoadUint64(l.atomic(v2LHWriteSeq))
	read := atomic.LoadUint64(l.atomic(v2LHReadSeq))
	if write-read >= uint64(l.config.Capacity) {
		atomic.AddUint64(l.atomic(v2LHDroppedFull), 1)
		return errV2QueueFull
	}
	index := uint32(write % uint64(l.config.Capacity))
	slot := l.slot(index)
	atomic.StoreUint64(v2AtomicU64(slot, v2SlotCommitted), v2EmptySequence)
	clear(slot[8:v2SlotHeaderSize])
	binary.LittleEndian.PutUint16(slot[v2SlotKind:], uint16(record.Kind))
	binary.LittleEndian.PutUint16(slot[v2SlotFlags:], record.Flags)
	binary.LittleEndian.PutUint32(slot[v2SlotPayload:], uint32(len(record.Payload)))
	binary.LittleEndian.PutUint64(slot[v2SlotTimestamp:], record.TimestampNS)
	binary.LittleEndian.PutUint32(slot[v2SlotClockID:], record.ClockID)
	copy(slot[v2SlotSource:v2SlotSource+16], record.Source[:])
	copy(slot[v2SlotPeer:v2SlotPeer+32], record.Peer[:])
	copy(slot[v2SlotHash:v2SlotHash+32], record.Hash[:])
	copy(slot[v2SlotAux:v2SlotAux+32], record.Aux[:])
	binary.LittleEndian.PutUint64(slot[v2SlotEpoch:], epoch)
	copy(slot[v2SlotHeaderSize:v2SlotHeaderSize+len(record.Payload)], record.Payload)
	binary.LittleEndian.PutUint32(slot[v2SlotCRC32C:], recordCRC32CV2(slot, len(record.Payload)))
	atomic.StoreUint64(v2AtomicU64(slot, v2SlotCommitted), write+1)
	atomic.StoreUint64(l.atomic(v2LHWriteSeq), write+1)
	return nil
}

// tryConsume consumes the next record; ok is false if the lane is empty.
//
// A QUARANTINED lane is not consumed. The first structural defect of a record
// (sequence, payload length, lane epoch, CRC32C, record kind, or an unknown
// lifecycle state) quarantines the lane, counts exactly one corruption
// incident and returns that defect; every later call returns
// laneQuarantinedErrorV2 without counting again. A record on a FREE lane, or
// at or past final_write_sequence on a CLOSING lane, was written outside its
// binding: the lane is quarantined as a late write. read_sequence never
// advances past a refused record.
func (l *laneV2) tryConsume() (*recordV2, bool, error) {
	read := atomic.LoadUint64(l.atomic(v2LHReadSeq))
	write := atomic.LoadUint64(l.atomic(v2LHWriteSeq))
	// Loaded after write_sequence: a record observed above was published by a
	// producer that saw BOUND, so this load sees BOUND or later.
	state := l.state()
	// The state is validated before the empty check: an undefined state
	// quarantines the lane even when no record is waiting.
	if err := l.admitState(state); err != nil {
		return nil, false, err
	}
	if read == write {
		if atomic.LoadUint64(l.atomic(v2LHShutdown)) != 0 {
			return nil, false, errV2Shutdown
		}
		return nil, false, nil
	}
	if state == v2LaneStateFree {
		return nil, false, l.quarantineLateWrite(read, write)
	}
	if state == v2LaneStateClosing {
		if finalWrite := atomic.LoadUint64(l.atomic(v2LHFinalWrite)); read >= finalWrite {
			return nil, false, l.quarantineLateWrite(finalWrite, write)
		}
	}

	index := uint32(read % uint64(l.config.Capacity))
	slot := l.slot(index)
	observed := atomic.LoadUint64(v2AtomicU64(slot, v2SlotCommitted))
	if observed != read+1 {
		return nil, false, l.corrupt(v2QuarantineCorruptSeq, fmt.Sprintf("corrupt sequence %d != %d", observed, read+1))
	}
	payloadLen := int(binary.LittleEndian.Uint32(slot[v2SlotPayload:]))
	if payloadLen > int(l.config.SlotStride)-v2SlotHeaderSize {
		return nil, false, l.corrupt(v2QuarantinePayloadLength, fmt.Sprintf("corrupt payload length %d", payloadLen))
	}
	epoch := binary.LittleEndian.Uint64(slot[v2SlotEpoch:])
	if expectedEpoch := atomic.LoadUint64(l.atomic(v2LHEpoch)); epoch != expectedEpoch {
		return nil, false, l.corrupt(v2QuarantineEpochMismatch, fmt.Sprintf("lane epoch mismatch %d != %d", epoch, expectedEpoch))
	}
	expectedCRC := binary.LittleEndian.Uint32(slot[v2SlotCRC32C:])
	if actualCRC := recordCRC32CV2(slot, payloadLen); expectedCRC != actualCRC {
		return nil, false, l.corrupt(v2QuarantineChecksum, fmt.Sprintf("checksum mismatch %#x != %#x", actualCRC, expectedCRC))
	}
	kind := RecordKind(binary.LittleEndian.Uint16(slot[v2SlotKind:]))
	if kind < EarlyTxSeen || kind > Shutdown {
		return nil, false, l.corrupt(v2QuarantineRecordKind, fmt.Sprintf("unknown record kind %d", kind))
	}
	var source [16]byte
	var peer [32]byte
	var hash common.Hash
	var aux [32]byte
	copy(source[:], slot[v2SlotSource:v2SlotSource+16])
	copy(peer[:], slot[v2SlotPeer:v2SlotPeer+32])
	copy(hash[:], slot[v2SlotHash:v2SlotHash+32])
	copy(aux[:], slot[v2SlotAux:v2SlotAux+32])
	payload := append([]byte(nil), slot[v2SlotHeaderSize:v2SlotHeaderSize+payloadLen]...)
	record := &recordV2{
		Sequence:    read,
		LaneID:      l.config.ID,
		LaneEpoch:   epoch,
		Kind:        kind,
		Flags:       binary.LittleEndian.Uint16(slot[v2SlotFlags:]),
		TimestampNS: binary.LittleEndian.Uint64(slot[v2SlotTimestamp:]),
		ClockID:     binary.LittleEndian.Uint32(slot[v2SlotClockID:]),
		Source:      source,
		Peer:        peer,
		Hash:        hash,
		Aux:         aux,
		Payload:     payload,
	}
	atomic.StoreUint64(l.atomic(v2LHReadSeq), read+1)
	return record, true, nil
}

// admitState refuses a QUARANTINED lane and quarantines a lane whose state
// word holds no defined state. FREE, BOUND and CLOSING pass.
func (l *laneV2) admitState(state uint32) error {
	switch state {
	case v2LaneStateFree, v2LaneStateBound, v2LaneStateClosing:
		return nil
	case v2LaneStateQuarantined:
		return &laneQuarantinedErrorV2{Reason: l.quarantineReason()}
	default:
		return l.corrupt(v2QuarantineInvalidState, fmt.Sprintf("invalid lifecycle state %d", state))
	}
}

// corrupt quarantines for a structural defect. Only the call that moves the
// lane into QUARANTINED counts the incident and reports the defect.
func (l *laneV2) corrupt(reason uint64, detail string) error {
	if _, made := l.quarantineFor(reason); made {
		atomic.AddUint64(l.atomic(v2LHCorrupt), 1)
		return &corruptRecordErrorV2{Reason: reason, Detail: detail}
	}
	return &laneQuarantinedErrorV2{Reason: l.quarantineReason()}
}

func (l *laneV2) quarantineLateWrite(finalWrite, write uint64) error {
	l.quarantineFor(v2QuarantineLateWrite)
	return &lateWriteErrorV2{FinalWrite: finalWrite, Write: write}
}

func (l *laneV2) quarantineReason() uint64 {
	return atomic.LoadUint64(l.atomic(v2LHQuarantineReason))
}

// acknowledgeDrainedIfClosing is the consumer's half of the release
// protocol. On a CLOSING lane whose records up to final_write_sequence have
// all been consumed, it stores drained_epoch = lane_epoch and returns true.
// It returns false while the lane is not CLOSING or records remain. A record
// past final_write_sequence quarantines the lane and fails; a QUARANTINED
// lane fails.
func (l *laneV2) acknowledgeDrainedIfClosing() (bool, error) {
	state := l.state()
	if err := l.admitState(state); err != nil {
		return false, err
	}
	if state != v2LaneStateClosing {
		return false, nil
	}
	finalWrite := atomic.LoadUint64(l.atomic(v2LHFinalWrite))
	write := atomic.LoadUint64(l.atomic(v2LHWriteSeq))
	if write != finalWrite {
		return false, l.quarantineLateWrite(finalWrite, write)
	}
	if atomic.LoadUint64(l.atomic(v2LHReadSeq)) != finalWrite {
		return false, nil
	}
	epoch := atomic.LoadUint64(l.atomic(v2LHEpoch))
	atomic.StoreUint64(l.atomic(v2LHDrained), epoch)
	return true, nil
}

func (l *laneV2) state() uint32 {
	return atomic.LoadUint32(v2AtomicU32(l.arena.data, l.offset+v2LHState))
}

// transition is the compare-and-swap of the lifecycle state.
func (l *laneV2) transition(from, to uint32) error {
	if atomic.CompareAndSwapUint32(v2AtomicU32(l.arena.data, l.offset+v2LHState), from, to) {
		return nil
	}
	return v2StateError(from, l.state())
}

// quarantine quarantines the lane (reason operator): terminal for this
// generation, idempotent. Returns the previous state.
func (l *laneV2) quarantine() uint32 {
	previous, _ := l.quarantineFor(v2QuarantineOperator)
	return previous
}

// quarantineFor sets shutdown = 1, records reason if none is recorded yet,
// then moves any state -> QUARANTINED by compare-and-swap. It returns the
// previous state (QUARANTINED if it already was) and whether this call made
// the transition, which happens exactly once per lane and generation.
func (l *laneV2) quarantineFor(reason uint64) (uint32, bool) {
	atomic.StoreUint64(l.atomic(v2LHShutdown), 1)
	atomic.CompareAndSwapUint64(l.atomic(v2LHQuarantineReason), v2QuarantineNone, reason)
	state := v2AtomicU32(l.arena.data, l.offset+v2LHState)
	for {
		current := atomic.LoadUint32(state)
		if current == v2LaneStateQuarantined {
			return current, false
		}
		if atomic.CompareAndSwapUint32(state, current, v2LaneStateQuarantined) {
			return current, true
		}
	}
}

// storeOwner writes the owner tag as four atomic little-endian 64-bit words
// (offset 296 is 8-byte aligned), ordered by the following transition.
func (l *laneV2) storeOwner(owner [32]byte) {
	for word := 0; word < 4; word++ {
		atomic.StoreUint64(l.atomic(v2LHOwner+8*word), binary.LittleEndian.Uint64(owner[8*word:]))
	}
}

func (l *laneV2) loadOwner() [32]byte {
	var owner [32]byte
	for word := 0; word < 4; word++ {
		binary.LittleEndian.PutUint64(owner[8*word:], atomic.LoadUint64(l.atomic(v2LHOwner+8*word)))
	}
	return owner
}

func (l *laneV2) checkLease(owner [32]byte, epoch uint64) error {
	if l.loadOwner() != owner {
		return errV2OwnerMismatch
	}
	if current := atomic.LoadUint64(l.atomic(v2LHEpoch)); current != epoch {
		return &leaseEpochErrorV2{Expected: current, Observed: epoch}
	}
	return nil
}

func (l *laneV2) stats() laneStatsV2 {
	return laneStatsV2{
		WriteSequence:      atomic.LoadUint64(l.atomic(v2LHWriteSeq)),
		ReadSequence:       atomic.LoadUint64(l.atomic(v2LHReadSeq)),
		DroppedFull:        atomic.LoadUint64(l.atomic(v2LHDroppedFull)),
		Oversize:           atomic.LoadUint64(l.atomic(v2LHOversize)),
		CorruptRecords:     atomic.LoadUint64(l.atomic(v2LHCorrupt)),
		LaneEpoch:          atomic.LoadUint64(l.atomic(v2LHEpoch)),
		Generation:         atomic.LoadUint64(l.atomic(v2LHGeneration)),
		Shutdown:           atomic.LoadUint64(l.atomic(v2LHShutdown)) != 0,
		State:              l.state(),
		FinalWriteSequence: atomic.LoadUint64(l.atomic(v2LHFinalWrite)),
		DrainedEpoch:       atomic.LoadUint64(l.atomic(v2LHDrained)),
		Owner:              l.loadOwner(),
		QuarantineReason:   atomic.LoadUint64(l.atomic(v2LHQuarantineReason)),
	}
}

func (l *laneV2) atomic(relative int) *uint64 {
	return v2AtomicU64(l.arena.data, l.offset+relative)
}

func (l *laneV2) slot(index uint32) []byte {
	start := l.offset + v2LaneHeaderSize + int(index)*int(l.config.SlotStride)
	return l.arena.data[start : start+int(l.config.SlotStride)]
}

func (a *arenaV2) close() error {
	var first error
	if a.data != nil {
		if err := unix.Munmap(a.data); err != nil {
			first = err
		}
		a.data = nil
	}
	if a.file != nil {
		if err := a.file.Close(); err != nil && first == nil {
			first = err
		}
		a.file = nil
	}
	return first
}

func recordCRC32CV2(slot []byte, payloadLen int) uint32 {
	h := crc32.New(v2CRC32CTable)
	_, _ = h.Write(slot[8:v2SlotCRC32C])
	_, _ = h.Write(slot[v2SlotHeaderSize : v2SlotHeaderSize+payloadLen])
	return h.Sum32()
}

func v2AtomicU64(data []byte, offset int) *uint64 {
	return (*uint64)(unsafe.Pointer(&data[offset]))
}

func v2AtomicU32(data []byte, offset int) *uint32 {
	return (*uint32)(unsafe.Pointer(&data[offset]))
}

// peerLanePoolBoundsV2: at most MaxPeers lanes are bound at once;
// ChurnReserve further lanes take peers that connect while earlier lanes
// still drain. Their sum must equal the arena's peer-lane count, so a
// configuration asking for more peers than the arena qualifies fails at
// startup.
type peerLanePoolBoundsV2 struct {
	MaxPeers     int
	ChurnReserve int
}

// peerLaneLeaseV2 is one binding of one peer lane. Close and release need all
// three fields.
type peerLaneLeaseV2 struct {
	LaneID uint16
	Epoch  uint64
	Owner  [32]byte
}

// peerLanePoolStatsV2: pool counters and the current state of every peer
// lane.
type peerLanePoolStatsV2 struct {
	PeerLanes         int
	Free              int
	Bound             int
	Closing           int
	Quarantined       int
	Allocations       uint64
	Exhausted         uint64
	PeerLimitRefusals uint64
	Releases          uint64
}

// peerLanePoolV2 is the bounded allocator of the arena's peer-ingress lanes,
// for peer connect/disconnect outside the transaction hot path. Its mutex
// serializes every lifecycle call it makes. It never waits and never spins
// for a lane: no FREE lane and the peer limit are explicit results, counted
// in peerLanePoolStatsV2. Only FREE lanes are bound; BOUND, CLOSING and
// QUARANTINED lanes are never taken.
type peerLanePoolV2 struct {
	mu                sync.Mutex
	arena             *arenaV2
	lanes             []*laneV2
	bounds            peerLanePoolBoundsV2
	shuttingDown      bool
	cursor            int
	allocations       uint64
	exhausted         uint64
	peerLimitRefusals uint64
	releases          uint64
}

func newPeerLanePoolV2(arena *arenaV2, bounds peerLanePoolBoundsV2) (*peerLanePoolV2, error) {
	ids, err := arena.peerLaneIDs()
	if err != nil {
		return nil, err
	}
	if err := checkPoolBoundsV2(bounds, len(ids)); err != nil {
		return nil, err
	}
	lanes := make([]*laneV2, 0, len(ids))
	for _, id := range ids {
		lane, err := arena.laneByID(id)
		if err != nil {
			return nil, err
		}
		lanes = append(lanes, lane)
	}
	return &peerLanePoolV2{arena: arena, lanes: lanes, bounds: bounds}, nil
}

// allocate binds the next FREE peer lane to owner, round-robin from the lane
// after the previous allocation. Each lane is examined at most once.
func (p *peerLanePoolV2) allocate(owner [32]byte) (peerLaneLeaseV2, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.shuttingDown {
		return peerLaneLeaseV2{}, errV2Shutdown
	}
	if p.countLocked(v2LaneStateBound) >= p.bounds.MaxPeers {
		p.peerLimitRefusals++
		return peerLaneLeaseV2{}, fmt.Errorf("%w: %d bound", errV2PeerLimit, p.bounds.MaxPeers)
	}
	count := len(p.lanes)
	for step := 0; step < count; step++ {
		index := (p.cursor + step) % count
		lane := p.lanes[index]
		if lane.state() != v2LaneStateFree {
			continue
		}
		epoch, err := p.arena.bindPeerLane(lane.config.ID, owner)
		if errors.Is(err, errV2LifecycleState) {
			// The state left FREE between the check and the
			// compare-and-swap (a concurrent quarantine): not this lane.
			continue
		}
		if err != nil {
			return peerLaneLeaseV2{}, err
		}
		p.cursor = (index + 1) % count
		p.allocations++
		return peerLaneLeaseV2{LaneID: lane.config.ID, Epoch: epoch, Owner: owner}, nil
	}
	p.exhausted++
	return peerLaneLeaseV2{}, &poolExhaustedErrorV2{PeerLanes: count}
}

// beginClose: BOUND -> CLOSING for the lease. Returns final_write_sequence.
func (p *peerLanePoolV2) beginClose(lease peerLaneLeaseV2) (uint64, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.arena.beginClosePeerLane(lease.LaneID, lease.Owner, lease.Epoch)
}

// release: CLOSING -> FREE for the lease, once its drain is acknowledged.
func (p *peerLanePoolV2) release(lease peerLaneLeaseV2) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	if err := p.arena.releasePeerLane(lease.LaneID, lease.Owner, lease.Epoch); err != nil {
		return err
	}
	p.releases++
	return nil
}

// beginShutdown: no further allocation, and shutdown = 1 on every lane of the
// arena, so no publish and no bind succeeds afterwards. Close and release of
// existing leases stay possible. Idempotent.
func (p *peerLanePoolV2) beginShutdown() error {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.shuttingDown = true
	return p.arena.beginShutdown()
}

func (p *peerLanePoolV2) stats() peerLanePoolStatsV2 {
	p.mu.Lock()
	defer p.mu.Unlock()
	return peerLanePoolStatsV2{
		PeerLanes:         len(p.lanes),
		Free:              p.countLocked(v2LaneStateFree),
		Bound:             p.countLocked(v2LaneStateBound),
		Closing:           p.countLocked(v2LaneStateClosing),
		Quarantined:       p.countLocked(v2LaneStateQuarantined),
		Allocations:       p.allocations,
		Exhausted:         p.exhausted,
		PeerLimitRefusals: p.peerLimitRefusals,
		Releases:          p.releases,
	}
}

func (p *peerLanePoolV2) countLocked(state uint32) int {
	n := 0
	for _, lane := range p.lanes {
		if lane.state() == state {
			n++
		}
	}
	return n
}

func checkPoolBoundsV2(bounds peerLanePoolBoundsV2, peerLanes int) error {
	if bounds.MaxPeers <= 0 || bounds.ChurnReserve < 0 || bounds.MaxPeers+bounds.ChurnReserve != peerLanes {
		return fmt.Errorf("%w: max_peers=%d churn_reserve=%d peer_lanes=%d",
			errV2PoolBounds, bounds.MaxPeers, bounds.ChurnReserve, peerLanes)
	}
	return nil
}

// ---------------------------------------------------------------------------
// K15: generation-named arena files, probing, stale arenas
// ---------------------------------------------------------------------------

// arenaPathV2 is the arena file of one generation:
// <dir>/fastlane-v2-g<20 digits>.mmap. A new generation therefore never
// collides with the file of an earlier one, and O_EXCL stays the guard
// against silent reuse.
func arenaPathV2(dir string, generation uint64) string {
	return filepath.Join(dir, fmt.Sprintf("%s%0*d%s", v2ArenaFilePrefix, v2ArenaFileGenerationWidth, generation, v2ArenaFileSuffix))
}

// probeArenaV2 classifies the file at path against generation from its
// SuperHeader only. It never maps, attaches, modifies or deletes the file.
// For arenaStaleGenerationV2 the observed generation is returned.
func probeArenaV2(path string, generation uint64) (arenaProbeV2, uint64, error) {
	if generation == 0 {
		return 0, 0, errors.New("fastlane v2 generation must be nonzero")
	}
	file, err := os.Open(path)
	if errors.Is(err, fs.ErrNotExist) {
		return arenaAbsentV2, 0, nil
	}
	if err != nil {
		return 0, 0, err
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil {
		return 0, 0, err
	}
	header := make([]byte, v2SuperHeaderSize)
	if info.Size() < v2SuperHeaderSize {
		return arenaNotAnArenaV2, 0, nil
	}
	if _, err := io.ReadFull(file, header); err != nil {
		return arenaNotAnArenaV2, 0, nil
	}
	count := binary.LittleEndian.Uint32(header[v2SHLaneCount:])
	observed := binary.LittleEndian.Uint64(header[v2SHGeneration:])
	if string(header[v2SHMagic:v2SHMagic+8]) != v2Magic ||
		binary.LittleEndian.Uint16(header[v2SHVersion:]) != v2Version ||
		binary.LittleEndian.Uint16(header[v2SHEndian:]) != v2EndianMarker ||
		binary.LittleEndian.Uint32(header[v2SHHeaderSize:]) != v2SuperHeaderSize ||
		binary.LittleEndian.Uint32(header[v2SHDirSize:]) != v2DirEntrySize ||
		binary.LittleEndian.Uint32(header[v2SHLaneHeaderSize:]) != v2LaneHeaderSize ||
		binary.LittleEndian.Uint32(header[v2SHSlotHeaderSize:]) != v2SlotHeaderSize ||
		binary.LittleEndian.Uint32(header[v2SHAlignment:]) != v2Alignment ||
		binary.LittleEndian.Uint32(header[v2SHCRCKind:]) != v2CRCKindCRC32C ||
		binary.LittleEndian.Uint64(header[v2SHTotalBytes:]) != uint64(info.Size()) ||
		count == 0 || count > v2MaxLanes || observed == 0 {
		return arenaNotAnArenaV2, 0, nil
	}
	if observed == generation {
		return arenaCurrentV2, observed, nil
	}
	return arenaStaleGenerationV2, observed, nil
}

// staleArenaV2 names an arena file of another generation.
type staleArenaV2 struct {
	Generation uint64
	Path       string
}

// staleArenasV2 lists arena files in dir named for a generation other than
// generation, sorted by generation. Read-only: nothing is opened for writing,
// attached or deleted; disposal belongs to the startup orchestration.
func staleArenasV2(dir string, generation uint64) ([]staleArenaV2, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	var stale []staleArenaV2
	for _, entry := range entries {
		name := entry.Name()
		if !strings.HasPrefix(name, v2ArenaFilePrefix) || !strings.HasSuffix(name, v2ArenaFileSuffix) {
			continue
		}
		digits := strings.TrimSuffix(strings.TrimPrefix(name, v2ArenaFilePrefix), v2ArenaFileSuffix)
		if len(digits) != v2ArenaFileGenerationWidth || strings.Trim(digits, "0123456789") != "" {
			continue
		}
		observed, err := strconv.ParseUint(digits, 10, 64)
		if err != nil || observed == generation {
			continue
		}
		stale = append(stale, staleArenaV2{Generation: observed, Path: filepath.Join(dir, name)})
	}
	sort.Slice(stale, func(i, j int) bool { return stale[i].Generation < stale[j].Generation })
	return stale, nil
}

// ---------------------------------------------------------------------------
// K14: exclusive runtime role leases
// ---------------------------------------------------------------------------

// roleLeaseV2 is an exclusive role lease: flock(LOCK_EX|LOCK_NB) on
// <dir>/<role>.lock, held through the open descriptor for the lease's
// lifetime. The kernel lock is the only source of exclusivity; the file's
// content (version, role, generation, pid) is diagnostic. release, or the
// death of the process, closes the descriptor and releases the lock. The lock
// file is never deleted, so no second process can lock a fresh inode while
// the first still holds the old one.
type roleLeaseV2 struct {
	mu   sync.Mutex
	file *os.File
	path string
	role string
}

func acquireRoleLeaseV2(dir, role string, generation uint64) (*roleLeaseV2, error) {
	if role != v2RoleGethOwner && role != v2RoleRustOwner {
		return nil, fmt.Errorf("fastlane v2 unknown runtime role %q", role)
	}
	if generation == 0 {
		return nil, errors.New("fastlane v2 generation must be nonzero")
	}
	path := filepath.Join(dir, role+".lock")
	file, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0o600)
	if err != nil {
		return nil, err
	}
	if err := unix.Flock(int(file.Fd()), unix.LOCK_EX|unix.LOCK_NB); err != nil {
		_ = file.Close()
		if errors.Is(err, unix.EWOULDBLOCK) {
			return nil, fmt.Errorf("%w: %s", errV2RoleLeaseHeld, path)
		}
		return nil, err
	}
	content := fmt.Sprintf("version=%d\nrole=%s\ngeneration=%d\npid=%d\n", v2RoleLeaseVersion, role, generation, os.Getpid())
	if err := file.Truncate(0); err != nil {
		_ = file.Close()
		return nil, err
	}
	if _, err := file.WriteAt([]byte(content), 0); err != nil {
		_ = file.Close()
		return nil, err
	}
	return &roleLeaseV2{file: file, path: path, role: role}, nil
}

// release closes the descriptor, which releases the lock. Idempotent.
func (l *roleLeaseV2) release() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.file == nil {
		return nil
	}
	err := l.file.Close()
	l.file = nil
	return err
}

// ---------------------------------------------------------------------------
// K11: process-lifetime ownership of the geth side
// ---------------------------------------------------------------------------

// gethRuntimeV2 is geth's owner of one Fast-Lane generation: the geth-owner
// role lease, the arena and its peer-lane pool.
//
// Lifetime contract: the mapping is never unmapped and the lease is never
// released while the process lives. A publisher may therefore hold a lane
// across a shutdown without any reference count in the hot path. Shutdown
// refuses every later bind and publish; the operating system unmaps and
// unlocks at process exit.
type gethRuntimeV2 struct {
	lease    *roleLeaseV2
	arena    *arenaV2
	pool     *peerLanePoolV2
	stopping atomic.Bool
}

var (
	gethRuntimeActiveV2 atomic.Pointer[gethRuntimeV2]
	// Shut-down runtimes stay referenced until process exit, so their files
	// (lease descriptor, arena) are never finalized by the garbage collector.
	gethRuntimeRetiredV2 struct {
		sync.Mutex
		list []*gethRuntimeV2
	}
)

// startGethRuntimeV2 takes the geth-owner role in dir, creates the arena of
// generation (O_EXCL, never over an existing file) and its pool. Bounds are
// checked before anything is created.
func startGethRuntimeV2(dir string, generation uint64, lanes []laneConfigV2, bounds peerLanePoolBoundsV2) (*gethRuntimeV2, error) {
	peerLanes := 0
	for _, lane := range lanes {
		if lane.Class == v2PeerIngress {
			peerLanes++
		}
	}
	if err := checkPoolBoundsV2(bounds, peerLanes); err != nil {
		return nil, err
	}
	lease, err := acquireRoleLeaseV2(dir, v2RoleGethOwner, generation)
	if err != nil {
		return nil, err
	}
	arena, err := createArenaV2(arenaPathV2(dir, generation), generation, lanes)
	if err != nil {
		_ = lease.release()
		return nil, err
	}
	pool, err := newPeerLanePoolV2(arena, bounds)
	if err != nil {
		// Nobody has seen this mapping yet: unmapping it here is safe.
		_ = arena.close()
		_ = lease.release()
		return nil, err
	}
	return &gethRuntimeV2{lease: lease, arena: arena, pool: pool}, nil
}

// installGethRuntimeV2 makes rt the process's active runtime.
func installGethRuntimeV2(rt *gethRuntimeV2) error {
	if !gethRuntimeActiveV2.CompareAndSwap(nil, rt) {
		return errV2RuntimeActive
	}
	return nil
}

// currentGethRuntimeV2 returns the active runtime, nil once shutdown began.
func currentGethRuntimeV2() *gethRuntimeV2 {
	return gethRuntimeActiveV2.Load()
}

// shutdownGethRuntimeV2 deactivates the active runtime and shuts it down. A
// caller that loaded it earlier keeps a valid mapping; its binds and
// publishes are refused from now on.
func shutdownGethRuntimeV2() {
	rt := gethRuntimeActiveV2.Swap(nil)
	if rt == nil {
		return
	}
	rt.shutdown()
}

// shutdown: refuse every later bind and publish, keep close, drain and
// release of existing leases possible, never unmap. Idempotent.
func (rt *gethRuntimeV2) shutdown() {
	if !rt.stopping.CompareAndSwap(false, true) {
		return
	}
	gethRuntimeRetiredV2.Lock()
	gethRuntimeRetiredV2.list = append(gethRuntimeRetiredV2.list, rt)
	gethRuntimeRetiredV2.Unlock()
	// The arena was validated at creation; every lane lookup succeeds.
	_ = rt.pool.beginShutdown()
}
