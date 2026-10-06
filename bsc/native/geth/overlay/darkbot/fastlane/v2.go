package fastlane

import (
	"encoding/binary"
	"errors"
	"fmt"
	"hash/crc32"
	"os"
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
		binary.LittleEndian.PutUint32(data[o+v2LHState:], 1)
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

func (l *laneV2) publish(record recordV2) error {
	maxPayload := int(l.config.SlotStride) - v2SlotHeaderSize
	if len(record.Payload) > maxPayload {
		atomic.AddUint64(l.atomic(v2LHOversize), 1)
		return errors.New("fastlane v2 payload too large")
	}
	if atomic.LoadUint64(l.atomic(v2LHShutdown)) != 0 {
		return errors.New("fastlane v2 shutdown")
	}
	write := atomic.LoadUint64(l.atomic(v2LHWriteSeq))
	read := atomic.LoadUint64(l.atomic(v2LHReadSeq))
	if write-read >= uint64(l.config.Capacity) {
		atomic.AddUint64(l.atomic(v2LHDroppedFull), 1)
		return errors.New("fastlane v2 queue full")
	}
	epoch := atomic.LoadUint64(l.atomic(v2LHEpoch))
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

func (l *laneV2) tryConsume() (*recordV2, bool, error) {
	read := atomic.LoadUint64(l.atomic(v2LHReadSeq))
	write := atomic.LoadUint64(l.atomic(v2LHWriteSeq))
	if read == write {
		if atomic.LoadUint64(l.atomic(v2LHShutdown)) != 0 {
			return nil, false, errors.New("fastlane v2 shutdown")
		}
		return nil, false, nil
	}
	index := uint32(read % uint64(l.config.Capacity))
	slot := l.slot(index)
	observed := atomic.LoadUint64(v2AtomicU64(slot, v2SlotCommitted))
	if observed != read+1 {
		atomic.AddUint64(l.atomic(v2LHCorrupt), 1)
		return nil, false, fmt.Errorf("fastlane v2 corrupt sequence %d != %d", observed, read+1)
	}
	payloadLen := int(binary.LittleEndian.Uint32(slot[v2SlotPayload:]))
	if payloadLen > int(l.config.SlotStride)-v2SlotHeaderSize {
		atomic.AddUint64(l.atomic(v2LHCorrupt), 1)
		return nil, false, errors.New("fastlane v2 corrupt payload length")
	}
	epoch := binary.LittleEndian.Uint64(slot[v2SlotEpoch:])
	expectedEpoch := atomic.LoadUint64(l.atomic(v2LHEpoch))
	if epoch != expectedEpoch {
		atomic.AddUint64(l.atomic(v2LHCorrupt), 1)
		return nil, false, errors.New("fastlane v2 lane epoch mismatch")
	}
	expectedCRC := binary.LittleEndian.Uint32(slot[v2SlotCRC32C:])
	actualCRC := recordCRC32CV2(slot, payloadLen)
	if expectedCRC != actualCRC {
		atomic.AddUint64(l.atomic(v2LHCorrupt), 1)
		return nil, false, errors.New("fastlane v2 checksum mismatch")
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
		Kind:        RecordKind(binary.LittleEndian.Uint16(slot[v2SlotKind:])),
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
