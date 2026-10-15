// C1.5-C1.8: canonical/reorg provenance on the Wire-V2 Chain lane.
//
// The Chain lane is a dedicated SPSC lane (class ChainEvidence). Its only
// producer is the chain-write path of the pinned client: every patched hook
// in core/blockchain.go runs with BlockChain.chainmu held (see
// bsc/docs/BSC_C1_8_CHAIN_LANE_OWNERSHIP.md and the overlay tool's
// C1_8_CHAIN_OWNERSHIP_VERIFIED check).
//
// That proof is about the pinned source. At run time the publisher also
// checks the claim itself: a publish that finds another publish in progress
// writes nothing, is counted as an ownership violation, and the loss is
// reported in-band by a GAP record before the next record. Lost records
// (lane full, clock failure) are reported the same way, so Chain-lane
// evidence never has an unaccounted hole.
//
// Record fields keep the semantics the V1 ring carried:
//
//	CANONICAL_HEAD  hash = block hash, aux = parent hash,
//	                payload = number u64 LE | tx count u32 LE
//	REORG           hash = new head hash, aux = common ancestor hash,
//	                payload = old number u64 LE | old hash | new number u64 LE
//	                | new hash | common number u64 LE
//	GAP             aux[0..8] = records lost before this point, u64 LE
//
// Publishing changes no chain state and never blocks the chain-write path.

package fastlane

import (
	"encoding/binary"
	"sync/atomic"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

const (
	c15ChainLaneID         = uint16(2)
	c15ChainLaneCapacity   = uint32(1024)
	c15ChainLaneSlotStride = uint32(512)
)

var (
	chainSourceCanonicalV2 = newIngressSourceV2("canonical")
	chainSourceReorgV2     = newIngressSourceV2("reorg")
	chainSourceGapV2       = newIngressSourceV2("chain-gap")
)

// chainPublisherV2 is the Chain lane's producer.
type chainPublisherV2 struct {
	lane *laneV2

	// inside is 1 while a publish is in progress. It is a detector, not a
	// lock: a caller that finds it set does not wait.
	inside atomic.Uint32
	// pendingLost counts records lost since the last GAP record.
	pendingLost atomic.Uint64

	published           atomic.Uint64
	gaps                atomic.Uint64
	lost                atomic.Uint64
	clockErrors         atomic.Uint64
	ownershipViolations atomic.Uint64
}

// ChainEvidenceV2 is the Chain lane's producer evidence.
type ChainEvidenceV2 struct {
	LaneID              uint16 `json:"lane_id"`
	Published           uint64 `json:"published"`
	Gaps                uint64 `json:"gap_records"`
	Lost                uint64 `json:"lost_records"`
	PendingLost         uint64 `json:"lost_records_not_yet_reported"`
	ClockErrors         uint64 `json:"clock_errors"`
	OwnershipViolations uint64 `json:"ownership_violations"`
	DroppedFull         uint64 `json:"lane_dropped_full"`
	Oversize            uint64 `json:"lane_oversize"`
}

func (c *chainPublisherV2) publish(kind RecordKind, source IngressSourceV2, hash common.Hash, aux [32]byte, payload []byte) {
	if !c.inside.CompareAndSwap(0, 1) {
		// Another publish is in progress: this call is a second producer.
		c.ownershipViolations.Add(1)
		c.lost.Add(1)
		c.pendingLost.Add(1)
		return
	}
	defer c.inside.Store(0)

	now, err := MonotonicNS()
	if err != nil {
		c.clockErrors.Add(1)
		c.lost.Add(1)
		c.pendingLost.Add(1)
		return
	}
	if pending := c.pendingLost.Load(); pending != 0 {
		var gapAux [32]byte
		binary.LittleEndian.PutUint64(gapAux[0:8], pending)
		err := c.lane.publish(recordV2{
			Kind:        Gap,
			TimestampNS: now,
			ClockID:     v2ClockMonotonic,
			Source:      [16]byte(chainSourceGapV2),
			Aux:         gapAux,
		})
		if err != nil {
			// The gap itself could not be published; this record is lost
			// behind it and stays accounted in pendingLost.
			c.lost.Add(1)
			c.pendingLost.Add(1)
			return
		}
		c.gaps.Add(1)
		c.pendingLost.Add(^(pending - 1))
	}
	err = c.lane.publish(recordV2{
		Kind:        kind,
		TimestampNS: now,
		ClockID:     v2ClockMonotonic,
		Source:      [16]byte(source),
		Hash:        hash,
		Aux:         aux,
		Payload:     payload,
	})
	if err != nil {
		c.lost.Add(1)
		c.pendingLost.Add(1)
		return
	}
	c.published.Add(1)
}

func (c *chainPublisherV2) evidence() ChainEvidenceV2 {
	stats := c.lane.stats()
	return ChainEvidenceV2{
		LaneID:              c.lane.config.ID,
		Published:           c.published.Load(),
		Gaps:                c.gaps.Load(),
		Lost:                c.lost.Load(),
		PendingLost:         c.pendingLost.Load(),
		ClockErrors:         c.clockErrors.Load(),
		OwnershipViolations: c.ownershipViolations.Load(),
		DroppedFull:         stats.DroppedFull,
		Oversize:            stats.Oversize,
	}
}

// PublishCanonicalV2 publishes a CANONICAL_HEAD record. The caller holds
// BlockChain.chainmu.
func PublishCanonicalV2(header *types.Header, txCount int) {
	r := ingressActiveV2.Load()
	if r == nil || r.chain == nil || header == nil {
		return
	}
	var aux [32]byte
	copy(aux[:], header.ParentHash[:])
	var payload [12]byte
	binary.LittleEndian.PutUint64(payload[0:8], header.Number.Uint64())
	binary.LittleEndian.PutUint32(payload[8:12], uint32(txCount))
	r.chain.publish(CanonicalHead, chainSourceCanonicalV2, header.Hash(), aux, payload[:])
}

// PublishReorgV2 publishes a REORG record. The caller holds
// BlockChain.chainmu.
func PublishReorgV2(oldHead, newHead, commonHead *types.Header) {
	r := ingressActiveV2.Load()
	if r == nil || r.chain == nil || oldHead == nil || newHead == nil || commonHead == nil {
		return
	}
	oldHash := oldHead.Hash()
	newHash := newHead.Hash()
	var aux [32]byte
	copy(aux[:], commonHead.Hash().Bytes())
	var payload [88]byte
	binary.LittleEndian.PutUint64(payload[0:8], oldHead.Number.Uint64())
	copy(payload[8:40], oldHash[:])
	binary.LittleEndian.PutUint64(payload[40:48], newHead.Number.Uint64())
	copy(payload[48:80], newHash[:])
	binary.LittleEndian.PutUint64(payload[80:88], commonHead.Number.Uint64())
	r.chain.publish(Reorg, chainSourceReorgV2, newHash, aux, payload[:])
}

// ChainV2Snapshot returns the Chain lane's producer evidence, false when the
// Fast Lane is disabled.
func ChainV2Snapshot() (ChainEvidenceV2, bool) {
	r := ingressActiveV2.Load()
	if r == nil || r.chain == nil {
		return ChainEvidenceV2{}, false
	}
	return r.chain.evidence(), true
}
