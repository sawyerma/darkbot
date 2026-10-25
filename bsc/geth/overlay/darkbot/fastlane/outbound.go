// C1.7: local-geth outbound with persistent peer-send ownership.
//
//	Rust egress SPSC lane
//	  -> outbound coordinator (one permanent goroutine, the lane's consumer)
//	  -> permanent worker 0 -> its selected peer
//	  -> permanent worker 1 -> its selected peer
//	  -> ...
//
// Ownership:
//
//   - The coordinator and every worker are started once by StartOutboundV2
//     and live until StopOutboundV2. Nothing is started per BUY.
//   - Each worker owns one job queue (only the coordinator sends, only the
//     worker receives) and one evidence lane (only the worker publishes).
//     The coordinator owns its own evidence lane.
//   - The coordinator hands a BUY to every worker without waiting: a worker
//     that is still inside a slow peer's send does not delay the handoff to
//     the next worker. A full queue is reported, never waited for.
//   - Which peer a worker sends to is decided by AssignOutboundTargetsV2,
//     called by the handler's maintenance goroutine. The dispatch path reads
//     one atomic pointer per worker; it never walks or copies the peer set.
//
// Evidence: every dispatched BUY yields one coordinator record (DISPATCHED,
// or INVALID_EGRESS) and one record per worker slot from whoever decided that
// slot's outcome. A worker's record carries the worker's own clock reading
// taken when the peer send returned. A returned send is a local transport
// handoff. It is not remote receipt, inclusion, execution or a purchase.
// A failed or zero clock reading never stamps a record: the record is left
// out and counted as a clock error. A BUY whose observe time, or a send whose
// start time, could not be read is not dispatched or not made (fail closed).
//
// Evidence record (kind OUTBOUND_PEER_HANDOFF, class OutboundHandoffEvidence):
//
//	timestamp_ns  the event time on the publisher's clock (clock id 1)
//	hash          the signed transaction hash
//	peer          the target node id, zero when no peer is involved
//	aux[0]        status
//	aux[1]        worker index (0xff on a DISPATCHED/INVALID_EGRESS record)
//	aux[2]        workers that had a peer when the BUY was dispatched
//	aux[3]        workers the BUY was queued to (coordinator records)
//	aux[8..16]    egress lane sequence, u64 LE
//	aux[16..24]   coordinator observe time, u64 LE
//	aux[24..32]   worker: time just before the peer send, u64 LE;
//	              coordinator: Rust's egress record timestamp, u64 LE

package fastlane

import (
	"encoding/binary"
	"errors"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

const envOutboundV2Workers = "DARKBOT_FASTLANE_V2_OUTBOUND_WORKERS"

const (
	c17EgressLaneID           = uint16(3)
	c17EgressLaneCapacity     = uint32(256)
	c17EgressLaneSlotStride   = uint32(4096)
	c17CoordinatorLaneID      = uint16(512)
	c17FirstWorkerLaneID      = uint16(513)
	c17EvidenceLaneCapacity   = uint32(1024)
	c17EvidenceLaneSlotStride = uint32(512)
	c17MaxOutboundWorkers     = 16
	c17WorkerQueueDepth       = 8
	c17NoWorker               = 0xff
)

// OutboundStatusV2 is the outcome of one worker slot, or of the dispatch.
type OutboundStatusV2 uint8

const (
	// OutboundHandedOff: the peer send returned without error.
	OutboundHandedOff OutboundStatusV2 = 1
	// OutboundSendErrorUnknown: the peer send returned an error. Bytes may
	// have left; the outcome is unknown, never "not sent".
	OutboundSendErrorUnknown OutboundStatusV2 = 2
	// OutboundNoTarget: the worker slot had no peer. Nothing was sent.
	OutboundNoTarget OutboundStatusV2 = 3
	// OutboundQueueFull: the worker was busy and its queue full. Nothing
	// was sent through this worker.
	OutboundQueueFull OutboundStatusV2 = 4
	// OutboundAlreadyKnown: the peer already knows the transaction. Nothing
	// was sent to it.
	OutboundAlreadyKnown OutboundStatusV2 = 5
	// OutboundInvalidEgress: the egress record was not a signed transaction
	// with the stated hash. Nothing was sent.
	OutboundInvalidEgress OutboundStatusV2 = 6
	// OutboundDispatched: the coordinator's summary of one BUY.
	OutboundDispatched OutboundStatusV2 = 7
)

var (
	outboundSourceWorkerV2      = newIngressSourceV2("p2p-worker")
	outboundSourceCoordinatorV2 = newIngressSourceV2("p2p-coord")

	errOutboundNoSend = errors.New("fastlane outbound target without send function")
)

// OutboundTargetV2 is one selected peer as the outbound workers see it.
type OutboundTargetV2 struct {
	NodeID [32]byte
	// Send hands the raw signed transaction to the peer and returns when
	// the local transport write returned.
	Send func(hash common.Hash, raw []byte) error
	// Known reports whether the peer already knows the transaction; nil
	// means unknown.
	Known func(hash common.Hash) bool
}

type outboundJobV2 struct {
	// Immutable assignment captured once by dispatch; never reload the slot.
	target     *OutboundTargetV2
	hash       common.Hash
	raw        []byte
	egressSeq  uint64
	observedNS uint64
	selected   int
}

type outboundWorkerV2 struct {
	index    int
	evidence *laneV2
	jobs     chan outboundJobV2
	target   atomic.Pointer[OutboundTargetV2]

	handedOff     atomic.Uint64
	sendErrors    atomic.Uint64
	noTarget      atomic.Uint64
	alreadyKnown  atomic.Uint64
	evidenceLost  atomic.Uint64
	abandonedStop atomic.Uint64
	// clockErrors counts failed clock readings: the record it would have
	// stamped is not published. clockRefused counts the sends among them
	// that were not made because the time before the send was not read.
	clockErrors  atomic.Uint64
	clockRefused atomic.Uint64
}

type outboundRuntimeV2 struct {
	egress      *laneV2
	coordinator *laneV2
	workers     []*outboundWorkerV2

	stop    chan struct{}
	done    sync.WaitGroup
	started atomic.Bool
	stopped atomic.Bool

	// assignMu serializes peer assignment. It is never taken on the
	// dispatch path.
	assignMu      sync.Mutex
	reassignments atomic.Uint64

	egressRecords atomic.Uint64
	invalidEgress atomic.Uint64
	dispatched    atomic.Uint64
	queued        atomic.Uint64
	queueFull     atomic.Uint64
	noTarget      atomic.Uint64
	evidenceLost  atomic.Uint64
	egressErrors  atomic.Uint64
	// clockErrors counts egress records not dispatched because their observe
	// time could not be read.
	clockErrors atomic.Uint64
}

// outboundLanePlanV2 lists the lanes of `workers` outbound workers: the
// Rust->geth egress lane, the coordinator's evidence lane and one evidence
// lane per worker.
func outboundLanePlanV2(workers int) []laneConfigV2 {
	lanes := make([]laneConfigV2, 0, workers+2)
	lanes = append(lanes, laneConfigV2{
		ID:         c17EgressLaneID,
		Direction:  v2RustToGeth,
		Class:      v2SignedTransactionEgress,
		Capacity:   c17EgressLaneCapacity,
		SlotStride: c17EgressLaneSlotStride,
	})
	for i := 0; i <= workers; i++ {
		lanes = append(lanes, laneConfigV2{
			ID:         c17CoordinatorLaneID + uint16(i),
			Direction:  v2GethToRust,
			Class:      v2OutboundHandoffEvidence,
			Capacity:   c17EvidenceLaneCapacity,
			SlotStride: c17EvidenceLaneSlotStride,
		})
	}
	return lanes
}

// newOutboundRuntimeV2 takes the outbound lanes of `arena`. Every lane was
// created by this process; nothing is started yet.
func newOutboundRuntimeV2(arena *arenaV2, workers int) (*outboundRuntimeV2, error) {
	if workers < 1 || workers > c17MaxOutboundWorkers {
		return nil, fmt.Errorf("fastlane c1.7 outbound workers %d outside 1..%d", workers, c17MaxOutboundWorkers)
	}
	egress, err := arena.laneByID(c17EgressLaneID)
	if err != nil {
		return nil, err
	}
	coordinator, err := arena.laneByID(c17CoordinatorLaneID)
	if err != nil {
		return nil, err
	}
	o := &outboundRuntimeV2{
		egress:      egress,
		coordinator: coordinator,
		stop:        make(chan struct{}),
	}
	for i := 0; i < workers; i++ {
		lane, err := arena.laneByID(c17FirstWorkerLaneID + uint16(i))
		if err != nil {
			return nil, err
		}
		o.workers = append(o.workers, &outboundWorkerV2{
			index:    i,
			evidence: lane,
			jobs:     make(chan outboundJobV2, c17WorkerQueueDepth),
		})
	}
	return o, nil
}

// start launches the coordinator and the workers, once.
func (o *outboundRuntimeV2) start() bool {
	if !o.started.CompareAndSwap(false, true) {
		return false
	}
	o.done.Add(len(o.workers) + 1)
	for _, w := range o.workers {
		go w.run(o)
	}
	go o.coordinate()
	return true
}

// stopAndWait ends the coordinator and the workers and waits for them.
func (o *outboundRuntimeV2) stopAndWait() {
	if !o.started.Load() || !o.stopped.CompareAndSwap(false, true) {
		return
	}
	close(o.stop)
	o.done.Wait()
}

// assign binds the current peer selection to the worker slots. A worker
// keeps its peer while that peer is still selected; free slots take peers
// that no worker has. It runs on the caller's goroutine, outside dispatch.
func (o *outboundRuntimeV2) assign(targets []OutboundTargetV2) error {
	o.assignMu.Lock()
	defer o.assignMu.Unlock()
	selected := make(map[[32]byte]int, len(targets))
	for i := range targets {
		if targets[i].Send == nil {
			return errOutboundNoSend
		}
		selected[targets[i].NodeID] = i
	}
	held := make(map[[32]byte]bool, len(o.workers))
	for _, w := range o.workers {
		current := w.target.Load()
		if current == nil {
			continue
		}
		if _, ok := selected[current.NodeID]; ok && !held[current.NodeID] {
			held[current.NodeID] = true
			continue
		}
		w.target.Store(nil)
		o.reassignments.Add(1)
	}
	next := 0
	for _, w := range o.workers {
		if w.target.Load() != nil {
			continue
		}
		for next < len(targets) && held[targets[next].NodeID] {
			next++
		}
		if next == len(targets) {
			break
		}
		target := targets[next]
		held[target.NodeID] = true
		next++
		w.target.Store(&target)
		o.reassignments.Add(1)
	}
	return nil
}

// coordinate is the egress lane's only consumer.
func (o *outboundRuntimeV2) coordinate() {
	defer o.done.Done()
	applyPermanentThreadQoS("outbound-coordinator")
	spins := uint32(0)
	for {
		select {
		case <-o.stop:
			return
		default:
		}
		record, ok, err := o.egress.tryConsume()
		if err != nil {
			if errors.Is(err, errV2Shutdown) {
				return
			}
			// The egress lane is quarantined or corrupt: nothing more can
			// be read from it in this generation.
			o.egressErrors.Add(1)
			return
		}
		if !ok {
			spins++
			if spins&1023 == 0 {
				runtime.Gosched()
			}
			continue
		}
		spins = 0
		o.dispatch(record)
	}
}

// dispatch hands one egress record to every worker slot without waiting.
func (o *outboundRuntimeV2) dispatch(record *recordV2) {
	observed, clockErr := MonotonicNS()
	o.dispatchObserved(record, observed, clockErr)
}

// dispatchObserved dispatches one egress record observed at `observed`.
// Without a valid observe time it fails closed: the BUY reaches no worker
// and no record is stamped, only clockErrors counts it. Every egress record
// is then DISPATCHED, INVALID_EGRESS or a clock error.
func (o *outboundRuntimeV2) dispatchObserved(record *recordV2, observed uint64, clockErr error) {
	o.egressRecords.Add(1)
	if !clockReadingOKV2(observed, clockErr) {
		o.clockErrors.Add(1)
		return
	}
	if !validOutboundRecordV2(record) {
		o.invalidEgress.Add(1)
		o.report(OutboundInvalidEgress, record, c17NoWorker, 0, 0, observed, [32]byte{})
		return
	}
	job := outboundJobV2{
		hash:       record.Hash,
		raw:        record.Payload,
		egressSeq:  record.Sequence,
		observedNS: observed,
	}
	var targets [c17MaxOutboundWorkers]*OutboundTargetV2
	for i, w := range o.workers {
		targets[i] = w.target.Load()
		if targets[i] != nil {
			job.selected++
		}
	}
	queued := 0
	for i, w := range o.workers {
		job.target = targets[i]
		if job.target == nil {
			o.noTarget.Add(1)
			o.report(OutboundNoTarget, record, w.index, job.selected, 0, observed, [32]byte{})
			continue
		}
		select {
		case w.jobs <- job:
			queued++
		default:
			o.queueFull.Add(1)
			o.report(OutboundQueueFull, record, w.index, job.selected, 0, observed, job.target.NodeID)
		}
	}
	o.dispatched.Add(1)
	o.queued.Add(uint64(queued))
	o.report(OutboundDispatched, record, c17NoWorker, job.selected, queued, observed, [32]byte{})
}

// validOutboundRecordV2 accepts exactly one signed transaction whose hash is
// the record's hash.
func validOutboundRecordV2(record *recordV2) bool {
	if record.Kind != OutboundSignedTx || len(record.Payload) == 0 {
		return false
	}
	var tx types.Transaction
	if err := tx.UnmarshalBinary(record.Payload); err != nil {
		return false
	}
	return tx.Hash() == record.Hash
}

func outboundAuxV2(status OutboundStatusV2, worker, selected, queued int, egressSeq, observedNS, thirdNS uint64) [32]byte {
	var aux [32]byte
	aux[0] = byte(status)
	aux[1] = byte(worker)
	aux[2] = byte(selected)
	aux[3] = byte(queued)
	binary.LittleEndian.PutUint64(aux[8:16], egressSeq)
	binary.LittleEndian.PutUint64(aux[16:24], observedNS)
	binary.LittleEndian.PutUint64(aux[24:32], thirdNS)
	return aux
}

// report publishes one coordinator evidence record.
func (o *outboundRuntimeV2) report(status OutboundStatusV2, record *recordV2, worker, selected, queued int, observedNS uint64, peer [32]byte) {
	err := o.coordinator.publish(recordV2{
		Kind:        OutboundPeerHandoff,
		TimestampNS: observedNS,
		ClockID:     v2ClockMonotonic,
		Source:      [16]byte(outboundSourceCoordinatorV2),
		Peer:        peer,
		Hash:        record.Hash,
		Aux:         outboundAuxV2(status, worker, selected, queued, record.Sequence, observedNS, record.TimestampNS),
	})
	if err != nil {
		o.evidenceLost.Add(1)
	}
}

// run is one worker: it owns its queue's receive side, its evidence lane and
// every send to its peer.
func (w *outboundWorkerV2) run(o *outboundRuntimeV2) {
	defer o.done.Done()
	applyPermanentThreadQoS("outbound-worker")
	for {
		select {
		case <-o.stop:
			w.abandonedStop.Add(uint64(len(w.jobs)))
			return
		case job := <-w.jobs:
			w.handle(job)
		}
	}
}

func (w *outboundWorkerV2) handle(job outboundJobV2) {
	target := job.target
	if target == nil {
		// No bound target: never silently retarget a queued BUY.
		w.noTarget.Add(1)
		now, clockErr := MonotonicNS()
		w.reportClocked(job, OutboundNoTarget, [32]byte{}, now, now, clockErr)
		return
	}
	if target.Known != nil && target.Known(job.hash) {
		w.alreadyKnown.Add(1)
		now, clockErr := MonotonicNS()
		w.reportClocked(job, OutboundAlreadyKnown, target.NodeID, now, now, clockErr)
		return
	}
	start, clockErr := MonotonicNS()
	w.sendAt(job, target, start, clockErr)
}

// sendAt makes the peer send whose start time is `start`, read just before
// it. Without a valid start time it fails closed: the send is not made,
// because its evidence could not be stamped.
func (w *outboundWorkerV2) sendAt(job outboundJobV2, target *OutboundTargetV2, start uint64, startErr error) {
	if !clockReadingOKV2(start, startErr) {
		w.clockErrors.Add(1)
		w.clockRefused.Add(1)
		return
	}
	err := target.Send(job.hash, job.raw)
	// The worker's own clock, read as soon as the send returned.
	done, clockErr := MonotonicNS()
	status := OutboundHandedOff
	if err != nil {
		w.sendErrors.Add(1)
		status = OutboundSendErrorUnknown
	} else {
		w.handedOff.Add(1)
	}
	w.reportClocked(job, status, target.NodeID, start, done, clockErr)
}

// reportClocked publishes the worker record only when its event time is a
// valid clock reading. Otherwise the outcome stays counted, the record is not
// published and clockErrors counts it: evidence is never stamped 0.
func (w *outboundWorkerV2) reportClocked(job outboundJobV2, status OutboundStatusV2, peer [32]byte, startNS, eventNS uint64, clockErr error) {
	if !clockReadingOKV2(eventNS, clockErr) {
		w.clockErrors.Add(1)
		return
	}
	w.report(job, status, peer, startNS, eventNS)
}

// report publishes one worker evidence record on the worker's own lane.
func (w *outboundWorkerV2) report(job outboundJobV2, status OutboundStatusV2, peer [32]byte, startNS, eventNS uint64) {
	err := w.evidence.publish(recordV2{
		Kind:        OutboundPeerHandoff,
		TimestampNS: eventNS,
		ClockID:     v2ClockMonotonic,
		Source:      [16]byte(outboundSourceWorkerV2),
		Peer:        peer,
		Hash:        job.hash,
		Aux:         outboundAuxV2(status, w.index, job.selected, 0, job.egressSeq, job.observedNS, startNS),
	})
	if err != nil {
		w.evidenceLost.Add(1)
	}
}

// OutboundWorkerEvidenceV2 is one worker's counters.
type OutboundWorkerEvidenceV2 struct {
	Index         int    `json:"index"`
	EvidenceLane  uint16 `json:"evidence_lane"`
	HasTarget     bool   `json:"has_target"`
	HandedOff     uint64 `json:"handed_off"`
	SendErrors    uint64 `json:"send_errors_unknown"`
	NoTarget      uint64 `json:"no_target"`
	AlreadyKnown  uint64 `json:"already_known"`
	EvidenceLost  uint64 `json:"evidence_lost"`
	AbandonedStop uint64 `json:"abandoned_at_stop"`
	QueueDepth    int    `json:"queue_depth"`
	ClockErrors   uint64 `json:"clock_errors"`
	ClockRefused  uint64 `json:"clock_refused_sends"`
}

// OutboundEvidenceV2 is the outbound runtime's producer evidence.
type OutboundEvidenceV2 struct {
	Workers       int                        `json:"workers"`
	Running       bool                       `json:"running"`
	EgressRecords uint64                     `json:"egress_records"`
	InvalidEgress uint64                     `json:"invalid_egress"`
	Dispatched    uint64                     `json:"dispatched"`
	Queued        uint64                     `json:"queued"`
	QueueFull     uint64                     `json:"queue_full"`
	NoTarget      uint64                     `json:"no_target"`
	EvidenceLost  uint64                     `json:"coordinator_evidence_lost"`
	EgressErrors  uint64                     `json:"egress_lane_errors"`
	Reassignments uint64                     `json:"target_reassignments"`
	ClockErrors   uint64                     `json:"clock_errors"`
	PerWorker     []OutboundWorkerEvidenceV2 `json:"per_worker"`
	QoS           map[string]string          `json:"qos"`
}

func (o *outboundRuntimeV2) evidence() OutboundEvidenceV2 {
	out := OutboundEvidenceV2{
		Workers:       len(o.workers),
		Running:       o.started.Load() && !o.stopped.Load(),
		EgressRecords: o.egressRecords.Load(),
		InvalidEgress: o.invalidEgress.Load(),
		Dispatched:    o.dispatched.Load(),
		Queued:        o.queued.Load(),
		QueueFull:     o.queueFull.Load(),
		NoTarget:      o.noTarget.Load(),
		EvidenceLost:  o.evidenceLost.Load(),
		EgressErrors:  o.egressErrors.Load(),
		Reassignments: o.reassignments.Load(),
		ClockErrors:   o.clockErrors.Load(),
		QoS:           permanentThreadQoSSnapshot(),
	}
	for _, w := range o.workers {
		out.PerWorker = append(out.PerWorker, OutboundWorkerEvidenceV2{
			Index:         w.index,
			EvidenceLane:  w.evidence.config.ID,
			HasTarget:     w.target.Load() != nil,
			HandedOff:     w.handedOff.Load(),
			SendErrors:    w.sendErrors.Load(),
			NoTarget:      w.noTarget.Load(),
			AlreadyKnown:  w.alreadyKnown.Load(),
			EvidenceLost:  w.evidenceLost.Load(),
			AbandonedStop: w.abandonedStop.Load(),
			QueueDepth:    len(w.jobs),
			ClockErrors:   w.clockErrors.Load(),
			ClockRefused:  w.clockRefused.Load(),
		})
	}
	return out
}

// OutboundV2Enabled reports whether outbound workers are configured.
func OutboundV2Enabled() bool {
	r := ingressActiveV2.Load()
	return r != nil && r.outbound != nil
}

// StartOutboundV2 starts the coordinator and the workers once. It returns
// false when outbound is not configured or already started.
func StartOutboundV2() bool {
	r := ingressActiveV2.Load()
	if r == nil || r.outbound == nil {
		return false
	}
	return r.outbound.start()
}

// StopOutboundV2 stops the coordinator and the workers and waits for them.
// It is called before the lanes are sealed.
func StopOutboundV2() {
	r := ingressActiveV2.Load()
	if r == nil || r.outbound == nil {
		return
	}
	r.outbound.stopAndWait()
}

// AssignOutboundTargetsV2 gives the outbound workers the current peer
// selection. The handler's maintenance goroutine calls it; it is never on
// the BUY dispatch path.
func AssignOutboundTargetsV2(targets []OutboundTargetV2) error {
	r := ingressActiveV2.Load()
	if r == nil || r.outbound == nil {
		return nil
	}
	return r.outbound.assign(targets)
}

// OutboundV2Snapshot returns the outbound evidence, false when outbound is
// not configured.
func OutboundV2Snapshot() (OutboundEvidenceV2, bool) {
	r := ingressActiveV2.Load()
	if r == nil || r.outbound == nil {
		return OutboundEvidenceV2{}, false
	}
	return r.outbound.evidence(), true
}
