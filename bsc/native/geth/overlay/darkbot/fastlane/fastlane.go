package fastlane

import (
	"encoding/binary"
	"errors"
	"fmt"
	"hash/crc32"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"unsafe"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"golang.org/x/sys/unix"
)

const (
	magic            = "DBFLN001"
	version          = uint16(1)
	directionIngress = uint16(1)
	directionEgress  = uint16(2)

	ringHeaderSize = 128
	slotHeaderSize = 192

	offMagic       = 0
	offVersion     = 8
	offDirection   = 10
	offHeaderSize  = 12
	offGeneration  = 16
	offCapacity    = 24
	offSlotSize    = 28
	offWriteSeq    = 56
	offReadSeq     = 64
	offDroppedFull = 72
	offCorrupt     = 80
	offShutdown    = 88

	slotOffCommitted = 0
	slotOffKind      = 8
	slotOffFlags     = 10
	slotOffPayload   = 12
	slotOffTimestamp = 16
	slotOffSource    = 24
	slotOffPeer      = 40
	slotOffHash      = 72
	slotOffAux       = 104
	slotOffCRC       = 136

	minSlotSize  = 512
	maxSlotSize  = 256 * 1024
	maxCapacity  = 4096
	emptySequence = ^uint64(0)
)

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

type Record struct {
	Sequence    uint64
	Kind        RecordKind
	Flags       uint16
	TimestampNS uint64
	Source      [16]byte
	Peer        [32]byte
	Hash        common.Hash
	Aux         [32]byte
	Payload     []byte
}

type ring struct {
	file       *os.File
	data       []byte
	generation uint64
	capacity   uint32
	slotSize   uint32
	direction  uint16
}

type Manager struct {
	ingress *ring
	egress  *ring
	fanout  int
}

var global atomic.Pointer[Manager]

func InitFromEnv() error {
	dir := strings.TrimSpace(os.Getenv("DARKBOT_FASTLANE_DIR"))
	if dir == "" {
		return nil
	}
	if global.Load() != nil {
		return errors.New("darkbot fastlane already initialized")
	}
	generation, err := requiredUint64("DARKBOT_FASTLANE_GENERATION")
	if err != nil {
		return err
	}
	capacity64, err := requiredUint64("DARKBOT_FASTLANE_CAPACITY")
	if err != nil {
		return err
	}
	slotSize64, err := requiredUint64("DARKBOT_FASTLANE_SLOT_SIZE")
	if err != nil {
		return err
	}
	fanout64, err := requiredUint64("DARKBOT_FASTLANE_OUTBOUND_FANOUT")
	if err != nil {
		return err
	}
	if fanout64 == 0 || fanout64 > 16 {
		return fmt.Errorf("DARKBOT_FASTLANE_OUTBOUND_FANOUT must be 1..16")
	}
	capacity := uint32(capacity64)
	slotSize := uint32(slotSize64)
	if uint64(capacity) != capacity64 || uint64(slotSize) != slotSize64 {
		return errors.New("darkbot fastlane capacity/slot size overflow")
	}
	if err := validateConfig(generation, capacity, slotSize); err != nil {
		return err
	}
	info, err := os.Stat(dir)
	if err != nil {
		return fmt.Errorf("darkbot fastlane dir: %w", err)
	}
	if !info.IsDir() {
		return errors.New("darkbot fastlane path is not a directory")
	}

	ingress, err := createRing(filepath.Join(dir, "geth-to-rust.mmap"), generation, capacity, slotSize, directionIngress)
	if err != nil {
		return fmt.Errorf("create ingress ring: %w", err)
	}
	egress, err := createRing(filepath.Join(dir, "rust-to-geth.mmap"), generation, capacity, slotSize, directionEgress)
	if err != nil {
		_ = ingress.close()
		return fmt.Errorf("create egress ring: %w", err)
	}
	manager := &Manager{ingress: ingress, egress: egress, fanout: int(fanout64)}
	if !global.CompareAndSwap(nil, manager) {
		_ = ingress.close()
		_ = egress.close()
		return errors.New("darkbot fastlane initialized concurrently")
	}
	return nil
}

func Enabled() bool {
	return global.Load() != nil
}

func OutboundFanout() int {
	if manager := global.Load(); manager != nil {
		return manager.fanout
	}
	return 0
}

func Close() {
	manager := global.Swap(nil)
	if manager == nil {
		return
	}
	manager.ingress.setShutdown()
	manager.egress.setShutdown()
	_ = manager.ingress.close()
	_ = manager.egress.close()
}

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

func PublishTransaction(kind RecordKind, source string, peer []byte, tx *types.Transaction) {
	manager := global.Load()
	if manager == nil || tx == nil {
		return
	}
	raw, err := tx.MarshalBinary()
	if err != nil {
		return
	}
	timestamp, err := MonotonicNS()
	if err != nil {
		return
	}
	var peerID [32]byte
	copy(peerID[:], peer)
	var sourceID [16]byte
	copy(sourceID[:], source)
	hash := tx.Hash()
	_ = manager.ingress.publish(Record{
		Kind:        kind,
		TimestampNS: timestamp,
		Source:      sourceID,
		Peer:        peerID,
		Hash:        hash,
		Payload:     raw,
	})
}

func PublishCanonical(header *types.Header, txCount int) {
	manager := global.Load()
	if manager == nil || header == nil {
		return
	}
	timestamp, err := MonotonicNS()
	if err != nil {
		return
	}
	var source [16]byte
	copy(source[:], "canonical")
	var aux [32]byte
	copy(aux[:], header.ParentHash[:])
	payload := make([]byte, 12)
	binary.LittleEndian.PutUint64(payload[0:8], header.Number.Uint64())
	binary.LittleEndian.PutUint32(payload[8:12], uint32(txCount))
	_ = manager.ingress.publish(Record{
		Kind:        CanonicalHead,
		TimestampNS: timestamp,
		Source:      source,
		Hash:        header.Hash(),
		Aux:         aux,
		Payload:     payload,
	})
}

func PublishReorg(oldHead, newHead, commonHead *types.Header) {
	manager := global.Load()
	if manager == nil || oldHead == nil || newHead == nil || commonHead == nil {
		return
	}
	timestamp, err := MonotonicNS()
	if err != nil {
		return
	}
	var source [16]byte
	copy(source[:], "reorg")
	var aux [32]byte
	commonHash := commonHead.Hash()
	copy(aux[:], commonHash[:])
	payload := make([]byte, 88)
	oldHash := oldHead.Hash()
	newHash := newHead.Hash()
	binary.LittleEndian.PutUint64(payload[0:8], oldHead.Number.Uint64())
	copy(payload[8:40], oldHash[:])
	binary.LittleEndian.PutUint64(payload[40:48], newHead.Number.Uint64())
	copy(payload[48:80], newHash[:])
	binary.LittleEndian.PutUint64(payload[80:88], commonHead.Number.Uint64())
	_ = manager.ingress.publish(Record{
		Kind:        Reorg,
		TimestampNS: timestamp,
		Source:      source,
		Hash:        newHead.Hash(),
		Aux:         aux,
		Payload:     payload,
	})
}

func TryOutbound() (*Record, bool, error) {
	manager := global.Load()
	if manager == nil {
		return nil, false, nil
	}
	return manager.egress.tryConsume()
}

func PublishOutboundHandoff(hash common.Hash, peer []byte) {
	manager := global.Load()
	if manager == nil {
		return
	}
	timestamp, err := MonotonicNS()
	if err != nil {
		return
	}
	var peerID [32]byte
	copy(peerID[:], peer)
	var source [16]byte
	copy(source[:], "p2p-send")
	_ = manager.ingress.publish(Record{
		Kind:        OutboundPeerHandoff,
		TimestampNS: timestamp,
		Source:      source,
		Peer:        peerID,
		Hash:        hash,
	})
}

func requiredUint64(name string) (uint64, error) {
	value := strings.TrimSpace(os.Getenv(name))
	if value == "" {
		return 0, fmt.Errorf("%s is required when DARKBOT_FASTLANE_DIR is set", name)
	}
	parsed, err := strconv.ParseUint(value, 10, 64)
	if err != nil {
		return 0, fmt.Errorf("%s: %w", name, err)
	}
	return parsed, nil
}

func validateConfig(generation uint64, capacity, slotSize uint32) error {
	if generation == 0 {
		return errors.New("darkbot fastlane generation must be nonzero")
	}
	if capacity == 0 || capacity > maxCapacity {
		return fmt.Errorf("darkbot fastlane capacity %d outside 1..%d", capacity, maxCapacity)
	}
	if slotSize < minSlotSize || slotSize > maxSlotSize || slotSize%64 != 0 || slotSize <= slotHeaderSize {
		return fmt.Errorf("darkbot fastlane invalid slot size %d", slotSize)
	}
	return nil
}

func createRing(path string, generation uint64, capacity, slotSize uint32, direction uint16) (*ring, error) {
	if err := validateConfig(generation, capacity, slotSize); err != nil {
		return nil, err
	}
	total := uint64(ringHeaderSize) + uint64(capacity)*uint64(slotSize)
	if total > uint64(^uint(0)>>1) {
		return nil, errors.New("darkbot fastlane mapping too large")
	}
	file, err := os.OpenFile(path, os.O_CREATE|os.O_EXCL|os.O_RDWR, 0o600)
	if err != nil {
		return nil, err
	}
	if err := file.Truncate(int64(total)); err != nil {
		_ = file.Close()
		return nil, err
	}
	data, err := unix.Mmap(int(file.Fd()), 0, int(total), unix.PROT_READ|unix.PROT_WRITE, unix.MAP_SHARED)
	if err != nil {
		_ = file.Close()
		return nil, err
	}
	r := &ring{file: file, data: data, generation: generation, capacity: capacity, slotSize: slotSize, direction: direction}
	copy(data[offMagic:offMagic+8], []byte(magic))
	binary.LittleEndian.PutUint16(data[offVersion:offVersion+2], version)
	binary.LittleEndian.PutUint16(data[offDirection:offDirection+2], direction)
	binary.LittleEndian.PutUint32(data[offHeaderSize:offHeaderSize+4], ringHeaderSize)
	binary.LittleEndian.PutUint64(data[offGeneration:offGeneration+8], generation)
	binary.LittleEndian.PutUint32(data[offCapacity:offCapacity+4], capacity)
	binary.LittleEndian.PutUint32(data[offSlotSize:offSlotSize+4], slotSize)
	for i := uint32(0); i < capacity; i++ {
		atomic.StoreUint64(r.slotU64(i, slotOffCommitted), emptySequence)
	}
	if err := unix.Msync(data, unix.MS_SYNC); err != nil {
		_ = r.close()
		return nil, err
	}
	return r, nil
}

func openRing(path string, generation uint64, capacity, slotSize uint32, direction uint16) (*ring, error) {
	if err := validateConfig(generation, capacity, slotSize); err != nil {
		return nil, err
	}
	file, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		return nil, err
	}
	total := uint64(ringHeaderSize) + uint64(capacity)*uint64(slotSize)
	info, err := file.Stat()
	if err != nil {
		_ = file.Close()
		return nil, err
	}
	if uint64(info.Size()) != total {
		_ = file.Close()
		return nil, errors.New("darkbot fastlane mapping length mismatch")
	}
	data, err := unix.Mmap(int(file.Fd()), 0, int(total), unix.PROT_READ|unix.PROT_WRITE, unix.MAP_SHARED)
	if err != nil {
		_ = file.Close()
		return nil, err
	}
	r := &ring{file: file, data: data, generation: generation, capacity: capacity, slotSize: slotSize, direction: direction}
	if err := r.validateHeader(); err != nil {
		_ = r.close()
		return nil, err
	}
	return r, nil
}

func (r *ring) validateHeader() error {
	if string(r.data[offMagic:offMagic+8]) != magic {
		return errors.New("darkbot fastlane bad magic")
	}
	if binary.LittleEndian.Uint16(r.data[offVersion:offVersion+2]) != version {
		return errors.New("darkbot fastlane version mismatch")
	}
	if binary.LittleEndian.Uint16(r.data[offDirection:offDirection+2]) != r.direction {
		return errors.New("darkbot fastlane direction mismatch")
	}
	if binary.LittleEndian.Uint32(r.data[offHeaderSize:offHeaderSize+4]) != ringHeaderSize {
		return errors.New("darkbot fastlane header size mismatch")
	}
	if binary.LittleEndian.Uint64(r.data[offGeneration:offGeneration+8]) != r.generation {
		return errors.New("darkbot fastlane generation mismatch")
	}
	if binary.LittleEndian.Uint32(r.data[offCapacity:offCapacity+4]) != r.capacity {
		return errors.New("darkbot fastlane capacity mismatch")
	}
	if binary.LittleEndian.Uint32(r.data[offSlotSize:offSlotSize+4]) != r.slotSize {
		return errors.New("darkbot fastlane slot size mismatch")
	}
	return nil
}

func (r *ring) publish(record Record) error {
	if len(record.Payload) > int(r.slotSize)-slotHeaderSize {
		return errors.New("darkbot fastlane payload too large")
	}
	if atomic.LoadUint64(r.headerU64(offShutdown)) != 0 {
		return errors.New("darkbot fastlane shutdown")
	}
	write := atomic.LoadUint64(r.headerU64(offWriteSeq))
	read := atomic.LoadUint64(r.headerU64(offReadSeq))
	if write-read >= uint64(r.capacity) {
		atomic.AddUint64(r.headerU64(offDroppedFull), 1)
		return errors.New("darkbot fastlane queue full")
	}
	index := uint32(write % uint64(r.capacity))
	atomic.StoreUint64(r.slotU64(index, slotOffCommitted), emptySequence)
	slot := r.slot(index)
	binary.LittleEndian.PutUint16(slot[slotOffKind:slotOffKind+2], uint16(record.Kind))
	binary.LittleEndian.PutUint16(slot[slotOffFlags:slotOffFlags+2], record.Flags)
	binary.LittleEndian.PutUint32(slot[slotOffPayload:slotOffPayload+4], uint32(len(record.Payload)))
	binary.LittleEndian.PutUint64(slot[slotOffTimestamp:slotOffTimestamp+8], record.TimestampNS)
	copy(slot[slotOffSource:slotOffSource+16], record.Source[:])
	copy(slot[slotOffPeer:slotOffPeer+32], record.Peer[:])
	copy(slot[slotOffHash:slotOffHash+32], record.Hash[:])
	copy(slot[slotOffAux:slotOffAux+32], record.Aux[:])
	binary.LittleEndian.PutUint32(slot[slotOffCRC:slotOffCRC+4], crc32.ChecksumIEEE(record.Payload))
	copy(slot[slotHeaderSize:slotHeaderSize+len(record.Payload)], record.Payload)
	atomic.StoreUint64(r.slotU64(index, slotOffCommitted), write+1)
	atomic.StoreUint64(r.headerU64(offWriteSeq), write+1)
	return nil
}

func (r *ring) tryConsume() (*Record, bool, error) {
	read := atomic.LoadUint64(r.headerU64(offReadSeq))
	write := atomic.LoadUint64(r.headerU64(offWriteSeq))
	if read == write {
		if atomic.LoadUint64(r.headerU64(offShutdown)) != 0 {
			return nil, false, errors.New("darkbot fastlane shutdown")
		}
		return nil, false, nil
	}
	index := uint32(read % uint64(r.capacity))
	observed := atomic.LoadUint64(r.slotU64(index, slotOffCommitted))
	if observed != read+1 {
		atomic.AddUint64(r.headerU64(offCorrupt), 1)
		return nil, false, fmt.Errorf("darkbot fastlane corrupt sequence %d != %d", observed, read+1)
	}
	slot := r.slot(index)
	payloadLen := int(binary.LittleEndian.Uint32(slot[slotOffPayload : slotOffPayload+4]))
	if payloadLen < 0 || payloadLen > int(r.slotSize)-slotHeaderSize {
		atomic.AddUint64(r.headerU64(offCorrupt), 1)
		return nil, false, errors.New("darkbot fastlane corrupt payload length")
	}
	payload := append([]byte(nil), slot[slotHeaderSize:slotHeaderSize+payloadLen]...)
	expectedCRC := binary.LittleEndian.Uint32(slot[slotOffCRC : slotOffCRC+4])
	if crc32.ChecksumIEEE(payload) != expectedCRC {
		atomic.AddUint64(r.headerU64(offCorrupt), 1)
		return nil, false, errors.New("darkbot fastlane checksum mismatch")
	}
	var source [16]byte
	var peer [32]byte
	var hash common.Hash
	var aux [32]byte
	copy(source[:], slot[slotOffSource:slotOffSource+16])
	copy(peer[:], slot[slotOffPeer:slotOffPeer+32])
	copy(hash[:], slot[slotOffHash:slotOffHash+32])
	copy(aux[:], slot[slotOffAux:slotOffAux+32])
	record := &Record{
		Sequence:    read,
		Kind:        RecordKind(binary.LittleEndian.Uint16(slot[slotOffKind : slotOffKind+2])),
		Flags:       binary.LittleEndian.Uint16(slot[slotOffFlags : slotOffFlags+2]),
		TimestampNS: binary.LittleEndian.Uint64(slot[slotOffTimestamp : slotOffTimestamp+8]),
		Source:      source,
		Peer:        peer,
		Hash:        hash,
		Aux:         aux,
		Payload:     payload,
	}
	atomic.StoreUint64(r.headerU64(offReadSeq), read+1)
	return record, true, nil
}

func (r *ring) setShutdown() {
	atomic.StoreUint64(r.headerU64(offShutdown), 1)
}

func (r *ring) close() error {
	var first error
	if r.data != nil {
		if err := unix.Munmap(r.data); err != nil {
			first = err
		}
		r.data = nil
	}
	if r.file != nil {
		if err := r.file.Close(); err != nil && first == nil {
			first = err
		}
		r.file = nil
	}
	return first
}

func (r *ring) headerU64(offset int) *uint64 {
	return (*uint64)(unsafe.Pointer(&r.data[offset]))
}

func (r *ring) slot(index uint32) []byte {
	start := ringHeaderSize + int(index)*int(r.slotSize)
	return r.data[start : start+int(r.slotSize)]
}

func (r *ring) slotU64(index uint32, offset int) *uint64 {
	slot := r.slot(index)
	return (*uint64)(unsafe.Pointer(&slot[offset]))
}
