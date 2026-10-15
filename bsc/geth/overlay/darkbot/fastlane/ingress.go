package fastlane

// C1.4 — geth ingress producer on Wire V2.
//
// Per Transactions or PooledTransactions packet of one peer:
//
//	fully decoded packet
//	  -> T_packet_decoded           (once per packet, before validation)
//	  -> geth basic validation       (handleTransactions; result returned unchanged)
//	  -> T_ingress                   (once per packet, after validation succeeded)
//	  -> destination prefilter -> selector prefilter
//	  -> canonical signed bytes
//	  -> publish into the peer's own SPSC lane (L1 Release commit)
//
// One active ETH peer owns exactly one bound peer-ingress lane through a
// PeerIngressV2 held on its eth.Peer for the connection's lifetime, so the
// publish path needs neither a lock nor a lookup. Bind, close and release run
// outside the transaction hot path, under a mutex. A Fast-Lane refusal never
// changes geth's handling of a packet: it is counted, never returned.

import (
	"bytes"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"fmt"
	"hash/crc32"
	"os"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
)

const (
	envIngressV2Dir          = "DARKBOT_FASTLANE_V2_DIR"
	envIngressV2Generation   = "DARKBOT_FASTLANE_V2_GENERATION"
	envIngressV2MaxPeers     = "DARKBOT_FASTLANE_V2_MAX_PEERS"
	envIngressV2ChurnReserve = "DARKBOT_FASTLANE_V2_CHURN_RESERVE"
	envIngressV2Scope        = "DARKBOT_FASTLANE_V2_SCOPE"
	envC19bAcceptTxsUnsynced = "DARKBOT_C19B_ACCEPT_TXS_UNSYNCED"

	ingressScopeMainnet     = "mainnet"
	ingressScopeC19bTestnet = "c19b-testnet"

	// c14PeerLanePoolBound is the qualified upper bound of
	// max_peers + churn_reserve for C1.4. Startup fails above it.
	c14PeerLanePoolBound = 256

	c14TxpoolLaneID    = uint16(1)
	c14FirstPeerLaneID = uint16(16)

	// Production lane plan. A Flap launch transaction is far below the
	// 3840-byte payload of a 4096-byte slot.
	c14PeerLaneCapacity     = uint32(64)
	c14PeerLaneSlotStride   = uint32(4096)
	c14TxpoolLaneCapacity   = uint32(4096)
	c14TxpoolLaneSlotStride = uint32(4096)

	c14GapEventLimit = 256

	prefilterScopeMagic = "DBC14PF1"
	prefilterRoleLaunch = uint8(1)
)

// IngressSourceV2 is the 16-byte source field of an ingress record.
type IngressSourceV2 [16]byte

var (
	// IngressSourceEthBroadcast marks a record from an ETH Transactions packet.
	IngressSourceEthBroadcast = newIngressSourceV2("eth-broadcast")
	// IngressSourceEthPooled marks a record from an ETH PooledTransactions packet.
	IngressSourceEthPooled = newIngressSourceV2("eth-pooled")
	ingressSourceTxpool    = newIngressSourceV2("txpool")
)

func newIngressSourceV2(name string) IngressSourceV2 {
	var source IngressSourceV2
	copy(source[:], name)
	return source
}

// serializeTxV2 produces the canonical signed transaction bytes. It is a
// variable only so that tests can count calls; production never replaces it.
var serializeTxV2 = func(tx *types.Transaction) ([]byte, error) { return tx.MarshalBinary() }

// ---------------------------------------------------------------------------
// Prefilter: immutable startup snapshot of the address+selector superset.
// ---------------------------------------------------------------------------

type prefilterSelectorV2 struct {
	Selector [4]byte
	Role     uint8
}

// prefilterScopeV2 is a conservative superset filter, never venue authority.
// Its canonical bytes are
//
//	"DBC14PF1" | chain_id u64 LE | address count u16 LE | addresses (20 B, ascending)
//	| selector count u16 LE | (selector 4 B, role u8), ascending by selector
//
// and its fingerprint is CRC32C over them. The only role is 1, launch.
type prefilterScopeV2 struct {
	chainID     uint64
	addresses   []common.Address
	selectors   []prefilterSelectorV2
	canonical   []byte
	fingerprint uint32
}

func newPrefilterScopeV2(chainID uint64, addresses []common.Address, selectors []prefilterSelectorV2) (*prefilterScopeV2, error) {
	if chainID == 0 || len(addresses) == 0 || len(selectors) == 0 || len(addresses) > 0xffff || len(selectors) > 0xffff {
		return nil, errors.New("fastlane c1.4 prefilter scope must name a chain, addresses and selectors")
	}
	addrs := append([]common.Address(nil), addresses...)
	sort.Slice(addrs, func(i, j int) bool { return bytes.Compare(addrs[i][:], addrs[j][:]) < 0 })
	sels := append([]prefilterSelectorV2(nil), selectors...)
	sort.Slice(sels, func(i, j int) bool { return bytes.Compare(sels[i].Selector[:], sels[j].Selector[:]) < 0 })
	for i := range addrs {
		if i > 0 && addrs[i] == addrs[i-1] {
			return nil, fmt.Errorf("fastlane c1.4 prefilter duplicate address %x", addrs[i])
		}
	}
	for i := range sels {
		if sels[i].Role != prefilterRoleLaunch {
			return nil, fmt.Errorf("fastlane c1.4 prefilter selector %x has unknown role %d", sels[i].Selector, sels[i].Role)
		}
		if i > 0 && sels[i].Selector == sels[i-1].Selector {
			return nil, fmt.Errorf("fastlane c1.4 prefilter duplicate selector %x", sels[i].Selector)
		}
	}
	canonical := make([]byte, 0, len(prefilterScopeMagic)+8+2+20*len(addrs)+2+5*len(sels))
	canonical = append(canonical, prefilterScopeMagic...)
	canonical = binary.LittleEndian.AppendUint64(canonical, chainID)
	canonical = binary.LittleEndian.AppendUint16(canonical, uint16(len(addrs)))
	for _, addr := range addrs {
		canonical = append(canonical, addr[:]...)
	}
	canonical = binary.LittleEndian.AppendUint16(canonical, uint16(len(sels)))
	for _, sel := range sels {
		canonical = append(canonical, sel.Selector[:]...)
		canonical = append(canonical, sel.Role)
	}
	return &prefilterScopeV2{
		chainID:     chainID,
		addresses:   addrs,
		selectors:   sels,
		canonical:   canonical,
		fingerprint: crc32.Checksum(canonical, v2CRC32CTable),
	}, nil
}

func mustPrefilterScopeV2(chainID uint64, addresses []common.Address, selectors []prefilterSelectorV2) *prefilterScopeV2 {
	scope, err := newPrefilterScopeV2(chainID, addresses, selectors)
	if err != nil {
		panic(err)
	}
	return scope
}

// c14FlapScopeV2 is the C1.4 peer-ingress prefilter scope: the supported
// launch-selector superset of the committed Rust venue scope
// (darkbot-core venues::flap), deployment bsc_mainnet_v5_24_0 (chain 56,
// Portal v5.24.0) with NEW_TOKEN_V6_SELECTOR, the launch call of every
// supported newTokenV6 mode. BUY traffic (swapExactInput) is not launch
// detection and stays off the peer lanes. The canonical bytes are a golden
// value checked by Go, by the Rust mirror and by darkbot-core.
var c14FlapScopeV2 = mustPrefilterScopeV2(56,
	[]common.Address{{
		0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa,
		0xe6, 0x5d, 0x27, 0x7d, 0xd6, 0xb8, 0xe6, 0x5c, 0x9d, 0xe0,
	}},
	[]prefilterSelectorV2{
		{Selector: [4]byte{0x8c, 0xb5, 0x77, 0x2c}, Role: prefilterRoleLaunch},
	},
)

// c19bTestnetFlapScopeV2 is qualification-only.
// Production/default remains the frozen Chain-56 C1.4 scope.
var c19bTestnetFlapScopeV2 = mustPrefilterScopeV2(97,
	[]common.Address{{
		0x5b, 0xea, 0xca, 0xf7, 0xab, 0xcb, 0xb3, 0xab, 0x28, 0x0e,
		0x80, 0xd0, 0x07, 0xfd, 0x31, 0xfc, 0xe2, 0x65, 0x10, 0xe9,
	}},
	[]prefilterSelectorV2{
		{Selector: [4]byte{0x8c, 0xb5, 0x77, 0x2c}, Role: prefilterRoleLaunch},
	},
)

func ingressScopeFromEnvV2() (*prefilterScopeV2, error) {
	switch value := strings.TrimSpace(os.Getenv(envIngressV2Scope)); value {
	case "", ingressScopeMainnet:
		return c14FlapScopeV2, nil
	case ingressScopeC19bTestnet:
		return c19bTestnetFlapScopeV2, nil
	default:
		return nil, fmt.Errorf(
			"fastlane c1.4 invalid %s=%q",
			envIngressV2Scope,
			value,
		)
	}
}

// C19bUnsyncedAcceptTxsAllowed gates the qualification-only pre-sync
// transaction acceptance override. Production/default behavior is unchanged.
func C19bUnsyncedAcceptTxsAllowed(networkID uint64) (bool, error) {
	arm := strings.TrimSpace(os.Getenv(envC19bAcceptTxsUnsynced))
	if arm == "" {
		return false, nil
	}
	if arm != "YES" {
		return false, fmt.Errorf(
			"fastlane c1.9b invalid %s=%q; expected unset or YES",
			envC19bAcceptTxsUnsynced,
			arm,
		)
	}

	scope := strings.TrimSpace(os.Getenv(envIngressV2Scope))
	if scope != ingressScopeC19bTestnet {
		return false, fmt.Errorf(
			"fastlane c1.9b %s=YES requires %s=%s",
			envC19bAcceptTxsUnsynced,
			envIngressV2Scope,
			ingressScopeC19bTestnet,
		)
	}
	if networkID != 97 {
		return false, fmt.Errorf(
			"fastlane c1.9b unsynced transaction acceptance requires network id 97, got %d",
			networkID,
		)
	}
	return true, nil
}

// match checks the destination first and the 4-byte selector second. It
// serializes nothing.
func (s *prefilterScopeV2) match(tx *types.Transaction) bool {
	to := tx.To()
	if to == nil {
		return false
	}
	addressMatch := false
	for i := range s.addresses {
		if *to == s.addresses[i] {
			addressMatch = true
			break
		}
	}
	if !addressMatch {
		return false
	}
	data := tx.Data()
	if len(data) < 4 {
		return false
	}
	for i := range s.selectors {
		sel := &s.selectors[i].Selector
		if data[0] == sel[0] && data[1] == sel[1] && data[2] == sel[2] && data[3] == sel[3] {
			return true
		}
	}
	return false
}

// ---------------------------------------------------------------------------
// Runtime facade
// ---------------------------------------------------------------------------

type ingressConfigV2 struct {
	Dir                  string
	Generation           uint64
	MaxPeers             int
	ChurnReserve         int
	PeerLaneCapacity     uint32
	PeerLaneSlotStride   uint32
	TxpoolLaneCapacity   uint32
	TxpoolLaneSlotStride uint32
	// OutboundWorkers is the number of C1.7 outbound peer workers; 0 leaves
	// local-geth outbound off and its lanes out of the arena.
	OutboundWorkers int
}

func defaultIngressConfigV2(dir string, generation uint64, maxPeers, churnReserve int) ingressConfigV2 {
	return ingressConfigV2{
		Dir:                  dir,
		Generation:           generation,
		MaxPeers:             maxPeers,
		ChurnReserve:         churnReserve,
		PeerLaneCapacity:     c14PeerLaneCapacity,
		PeerLaneSlotStride:   c14PeerLaneSlotStride,
		TxpoolLaneCapacity:   c14TxpoolLaneCapacity,
		TxpoolLaneSlotStride: c14TxpoolLaneSlotStride,
	}
}

func (c ingressConfigV2) validate() error {
	if c.Generation == 0 {
		return errors.New("fastlane c1.4 generation must be nonzero")
	}
	if c.MaxPeers < 1 || c.ChurnReserve < 0 {
		return fmt.Errorf("fastlane c1.4 invalid peer bounds max_peers=%d churn_reserve=%d", c.MaxPeers, c.ChurnReserve)
	}
	if c.MaxPeers+c.ChurnReserve > c14PeerLanePoolBound {
		return fmt.Errorf("fastlane c1.4 max_peers %d + churn_reserve %d exceeds the qualified lane-pool bound %d",
			c.MaxPeers, c.ChurnReserve, c14PeerLanePoolBound)
	}
	if c.OutboundWorkers < 0 || c.OutboundWorkers > c17MaxOutboundWorkers {
		return fmt.Errorf("fastlane c1.7 outbound workers %d outside 0..%d", c.OutboundWorkers, c17MaxOutboundWorkers)
	}
	return nil
}

// lanePlan: the dedicated Txpool lane, one FREE lane per peer slot, the
// dedicated Chain lane (C1.5-C1.8) and, with outbound workers, the C1.7
// egress and evidence lanes. Lanes are found by id; their order is free.
func (c ingressConfigV2) lanePlan() []laneConfigV2 {
	peerLanes := c.MaxPeers + c.ChurnReserve
	lanes := make([]laneConfigV2, 0, peerLanes+4+c.OutboundWorkers)
	lanes = append(lanes, laneConfigV2{
		ID:         c14TxpoolLaneID,
		Direction:  v2GethToRust,
		Class:      v2TxpoolEvidence,
		Capacity:   c.TxpoolLaneCapacity,
		SlotStride: c.TxpoolLaneSlotStride,
	})
	for i := 0; i < peerLanes; i++ {
		lanes = append(lanes, laneConfigV2{
			ID:         c14FirstPeerLaneID + uint16(i),
			Direction:  v2GethToRust,
			Class:      v2PeerIngress,
			Capacity:   c.PeerLaneCapacity,
			SlotStride: c.PeerLaneSlotStride,
		})
	}
	lanes = append(lanes, laneConfigV2{
		ID:         c15ChainLaneID,
		Direction:  v2GethToRust,
		Class:      v2ChainEvidence,
		Capacity:   c15ChainLaneCapacity,
		SlotStride: c15ChainLaneSlotStride,
	})
	if c.OutboundWorkers > 0 {
		lanes = append(lanes, outboundLanePlanV2(c.OutboundWorkers)...)
	}
	return lanes
}

// PeerIngressCountersV2 counts one peer's ingress work. Every refusal is
// counted; nothing is dropped silently.
type PeerIngressCountersV2 struct {
	Packets             uint64 `json:"packets"`
	RejectedPackets     uint64 `json:"rejected_packets"`
	Matched             uint64 `json:"matched"`
	Filtered            uint64 `json:"filtered"`
	Published           uint64 `json:"published"`
	QueueFull           uint64 `json:"queue_full"`
	Oversize            uint64 `json:"oversize"`
	Refused             uint64 `json:"refused"`
	SerializeErrors     uint64 `json:"serialize_errors"`
	ClockErrors         uint64 `json:"clock_errors"`
	UncoveredCandidates uint64 `json:"uncovered_candidates"`
}

func (c *PeerIngressCountersV2) add(o PeerIngressCountersV2) {
	c.Packets += o.Packets
	c.RejectedPackets += o.RejectedPackets
	c.Matched += o.Matched
	c.Filtered += o.Filtered
	c.Published += o.Published
	c.QueueFull += o.QueueFull
	c.Oversize += o.Oversize
	c.Refused += o.Refused
	c.SerializeErrors += o.SerializeErrors
	c.ClockErrors += o.ClockErrors
	c.UncoveredCandidates += o.UncoveredCandidates
}

// IngressGapEventV2 is structured evidence of missing Fast-Lane coverage.
type IngressGapEventV2 struct {
	Peer        string `json:"peer"`
	Reason      string `json:"reason"`
	MonotonicNS uint64 `json:"monotonic_ns"`
}

type ingressRuntimeV2 struct {
	config ingressConfigV2
	rt     *gethRuntimeV2
	txpool *laneV2
	scope  *prefilterScopeV2

	// C1.5-C1.8: the Chain lane's producer, and the C1.7 outbound runtime
	// (nil without outbound workers).
	chain    *chainPublisherV2
	outbound *outboundRuntimeV2

	mu           sync.Mutex
	pending      []peerLaneLeaseV2
	live         map[*PeerIngressV2]struct{}
	gaps         []IngressGapEventV2
	gapTotal     uint64
	closedTotals PeerIngressCountersV2

	coveredBinds    atomic.Uint64
	uncoveredBinds  atomic.Uint64
	reclaimed       atomic.Uint64
	releaseFailures atomic.Uint64
	closeFailures   atomic.Uint64

	txpoolPublished       atomic.Uint64
	txpoolRefused         atomic.Uint64
	txpoolSerializeErrors atomic.Uint64
	txpoolClockErrors     atomic.Uint64
}

var (
	ingressActiveV2  atomic.Pointer[ingressRuntimeV2]
	ingressRetiredV2 struct {
		sync.Mutex
		list []*ingressRuntimeV2
	}
)

// startIngressRuntimeV2 validates the configuration and the peer bound before
// anything is created, then starts the C1.3 geth runtime (geth-owner role
// lease, O_EXCL arena of the generation, bounded pool).
func startIngressRuntimeV2(cfg ingressConfigV2) (*ingressRuntimeV2, error) {
	return startIngressRuntimeV2Scoped(cfg, c14FlapScopeV2)
}

func startIngressRuntimeV2Scoped(cfg ingressConfigV2, scope *prefilterScopeV2) (*ingressRuntimeV2, error) {
	if scope == nil {
		return nil, errors.New("fastlane c1.4 prefilter scope is nil")
	}
	if err := cfg.validate(); err != nil {
		return nil, err
	}
	info, err := os.Stat(cfg.Dir)
	if err != nil {
		return nil, fmt.Errorf("fastlane c1.4 dir: %w", err)
	}
	if !info.IsDir() {
		return nil, errors.New("fastlane c1.4 path is not a directory")
	}
	rt, err := startGethRuntimeV2(cfg.Dir, cfg.Generation, cfg.lanePlan(),
		peerLanePoolBoundsV2{MaxPeers: cfg.MaxPeers, ChurnReserve: cfg.ChurnReserve})
	if err != nil {
		return nil, err
	}
	txpool, err := rt.arena.laneByID(c14TxpoolLaneID)
	if err != nil {
		rt.shutdown()
		return nil, err
	}
	r := &ingressRuntimeV2{
		config: cfg,
		rt:     rt,
		txpool: txpool,
		scope:  scope,
		live:   make(map[*PeerIngressV2]struct{}),
	}
	chain, err := rt.arena.laneByID(c15ChainLaneID)
	if err != nil {
		rt.shutdown()
		return nil, err
	}
	r.chain = &chainPublisherV2{lane: chain}
	if cfg.OutboundWorkers > 0 {
		outbound, err := newOutboundRuntimeV2(rt.arena, cfg.OutboundWorkers)
		if err != nil {
			rt.shutdown()
			return nil, err
		}
		r.outbound = outbound
	}
	return r, nil
}

func (r *ingressRuntimeV2) install() error {
	if err := installGethRuntimeV2(r.rt); err != nil {
		return err
	}
	if !ingressActiveV2.CompareAndSwap(nil, r) {
		gethRuntimeActiveV2.CompareAndSwap(r.rt, nil)
		return errV2RuntimeActive
	}
	return nil
}

// shutdown refuses every later bind and publish and never unmaps; the
// runtime stays referenced until process exit (K11).
func (r *ingressRuntimeV2) shutdown() {
	ingressRetiredV2.Lock()
	ingressRetiredV2.list = append(ingressRetiredV2.list, r)
	ingressRetiredV2.Unlock()
	// The outbound goroutines end before the lanes are sealed.
	if r.outbound != nil {
		r.outbound.stopAndWait()
	}
	r.rt.shutdown()
}

// InitIngressV2FromEnv starts the C1.4 ingress runtime when
// DARKBOT_FASTLANE_V2_DIR is set. Generation, max peers and churn reserve are
// then required. A configuration above the qualified lane-pool bound fails.
func InitIngressV2FromEnv() error {
	dir := strings.TrimSpace(os.Getenv(envIngressV2Dir))
	if dir == "" {
		return nil
	}
	if ingressActiveV2.Load() != nil {
		return errors.New("fastlane c1.4 ingress already initialized")
	}
	generation, err := ingressEnvUint(envIngressV2Generation, 64)
	if err != nil {
		return err
	}
	maxPeers, err := ingressEnvUint(envIngressV2MaxPeers, 16)
	if err != nil {
		return err
	}
	churn, err := ingressEnvUint(envIngressV2ChurnReserve, 16)
	if err != nil {
		return err
	}
	cfg := defaultIngressConfigV2(dir, generation, int(maxPeers), int(churn))
	if strings.TrimSpace(os.Getenv(envOutboundV2Workers)) != "" {
		workers, err := ingressEnvUint(envOutboundV2Workers, 8)
		if err != nil {
			return err
		}
		cfg.OutboundWorkers = int(workers)
	}
	scope, err := ingressScopeFromEnvV2()
	if err != nil {
		return err
	}
	r, err := startIngressRuntimeV2Scoped(cfg, scope)
	if err != nil {
		return err
	}
	if err := r.install(); err != nil {
		r.shutdown()
		return err
	}
	return nil
}

func ingressEnvUint(name string, bits int) (uint64, error) {
	value := strings.TrimSpace(os.Getenv(name))
	if value == "" {
		return 0, fmt.Errorf("%s is required when %s is set", name, envIngressV2Dir)
	}
	parsed, err := strconv.ParseUint(value, 10, bits)
	if err != nil {
		return 0, fmt.Errorf("%s: %w", name, err)
	}
	return parsed, nil
}

// IngressV2Enabled reports whether the C1.4 ingress runtime is active.
func IngressV2Enabled() bool {
	return ingressActiveV2.Load() != nil
}

// CheckIngressV2PeerCapacity fails when geth would accept more peers than the
// configured Fast-Lane peer count; those peers would run without coverage.
func CheckIngressV2PeerCapacity(maxPeers int) error {
	r := ingressActiveV2.Load()
	if r == nil {
		return nil
	}
	if maxPeers > r.config.MaxPeers {
		return fmt.Errorf("geth MaxPeers %d exceeds %s=%d; peers above it would run without Fast-Lane coverage",
			maxPeers, envIngressV2MaxPeers, r.config.MaxPeers)
	}
	return nil
}

// ShutdownIngressV2 deactivates the runtime: no later bind or publish
// succeeds, closes and drains of existing bindings stay possible, nothing is
// unmapped. Idempotent.
func ShutdownIngressV2() {
	r := ingressActiveV2.Swap(nil)
	if r == nil {
		return
	}
	r.shutdown()
	gethRuntimeActiveV2.CompareAndSwap(r.rt, nil)
}

// ---------------------------------------------------------------------------
// Peer binding
// ---------------------------------------------------------------------------

// PeerIngressV2 is one ETH connection's Fast-Lane producer. It is created
// after the peer registered, stored on the eth.Peer, used only by that peer's
// handler goroutine and closed after its message loop returned. A peer
// without a lane (pool exhausted, peer limit, shutdown) keeps a handle that
// counts its missed candidates: it is never reported as covered.
type PeerIngressV2 struct {
	runtime *ingressRuntimeV2
	owner   [32]byte
	lane    *laneV2
	lease   peerLaneLeaseV2
	gap     string
	closed  bool

	packets             atomic.Uint64
	rejectedPackets     atomic.Uint64
	matched             atomic.Uint64
	filtered            atomic.Uint64
	published           atomic.Uint64
	queueFull           atomic.Uint64
	oversize            atomic.Uint64
	refused             atomic.Uint64
	serializeErrors     atomic.Uint64
	clockErrors         atomic.Uint64
	uncoveredCandidates atomic.Uint64
}

// BindPeerIngressV2 binds one FREE peer lane to the peer's node ID, outside
// the transaction hot path. It returns nil when the Fast Lane is disabled.
func BindPeerIngressV2(nodeID [32]byte) *PeerIngressV2 {
	r := ingressActiveV2.Load()
	if r == nil {
		return nil
	}
	return r.bindPeer(nodeID)
}

func (r *ingressRuntimeV2) bindPeer(owner [32]byte) *PeerIngressV2 {
	p := &PeerIngressV2{runtime: r, owner: owner}
	r.mu.Lock()
	defer r.mu.Unlock()
	r.reclaimLocked()
	lease, err := r.rt.pool.allocate(owner)
	if err == nil {
		lane, laneErr := r.rt.arena.laneByID(lease.LaneID)
		if laneErr == nil {
			p.lane, p.lease = lane, lease
			r.coveredBinds.Add(1)
			r.live[p] = struct{}{}
			return p
		}
		// A lane the pool just bound must resolve. If it does not, take it
		// out of service rather than leave it bound without a producer.
		_, _ = r.rt.arena.quarantineLane(lease.LaneID)
		err = laneErr
	}
	p.gap = ingressGapReasonV2(err)
	r.uncoveredBinds.Add(1)
	r.recordGapLocked(owner, p.gap)
	r.live[p] = struct{}{}
	return p
}

func ingressGapReasonV2(err error) string {
	switch {
	case errors.Is(err, errV2PoolExhausted):
		return "pool_exhausted"
	case errors.Is(err, errV2PeerLimit):
		return "peer_limit"
	case errors.Is(err, errV2Shutdown):
		return "shutdown"
	default:
		return "bind_failed: " + err.Error()
	}
}

func (r *ingressRuntimeV2) recordGapLocked(owner [32]byte, reason string) {
	now, _ := MonotonicNS()
	event := IngressGapEventV2{Peer: hex.EncodeToString(owner[:]), Reason: reason, MonotonicNS: now}
	r.gapTotal++
	if len(r.gaps) >= c14GapEventLimit {
		copy(r.gaps, r.gaps[1:])
		r.gaps[len(r.gaps)-1] = event
		return
	}
	r.gaps = append(r.gaps, event)
}

// reclaimLocked releases closed lanes whose drain Rust acknowledged. A lane
// that can no longer be released (quarantined) leaves the pool visibly.
func (r *ingressRuntimeV2) reclaimLocked() {
	kept := r.pending[:0]
	for _, lease := range r.pending {
		err := r.rt.pool.release(lease)
		switch {
		case err == nil:
			r.reclaimed.Add(1)
		case errors.Is(err, errV2NotDrained):
			kept = append(kept, lease)
		default:
			r.releaseFailures.Add(1)
			r.recordGapLocked(lease.Owner, "lane_lost: "+err.Error())
		}
	}
	r.pending = kept
}

// Covered reports whether the peer has a bound lane.
func (p *PeerIngressV2) Covered() bool {
	return p != nil && p.lane != nil
}

// GapReason names why the peer has no lane; empty when covered.
func (p *PeerIngressV2) GapReason() string {
	if p == nil {
		return ""
	}
	return p.gap
}

// Close ends the binding once the peer's message loop returned, i.e. after
// its last publish: BOUND -> CLOSING. The lane returns to the pool only after
// Rust acknowledged the drain; it is reclaimed on a later bind or snapshot.
// Idempotent; a nil receiver does nothing.
func (p *PeerIngressV2) Close() {
	if p == nil || p.closed {
		return
	}
	p.closed = true
	r := p.runtime
	r.mu.Lock()
	defer r.mu.Unlock()
	delete(r.live, p)
	r.closedTotals.add(p.counters())
	if p.lane == nil {
		return
	}
	if _, err := r.rt.pool.beginClose(p.lease); err != nil {
		r.closeFailures.Add(1)
		r.recordGapLocked(p.owner, "close_failed: "+err.Error())
		return
	}
	r.pending = append(r.pending, p.lease)
	r.reclaimLocked()
}

func (p *PeerIngressV2) counters() PeerIngressCountersV2 {
	return PeerIngressCountersV2{
		Packets:             p.packets.Load(),
		RejectedPackets:     p.rejectedPackets.Load(),
		Matched:             p.matched.Load(),
		Filtered:            p.filtered.Load(),
		Published:           p.published.Load(),
		QueueFull:           p.queueFull.Load(),
		Oversize:            p.oversize.Load(),
		Refused:             p.refused.Load(),
		SerializeErrors:     p.serializeErrors.Load(),
		ClockErrors:         p.clockErrors.Load(),
		UncoveredCandidates: p.uncoveredCandidates.Load(),
	}
}

// ---------------------------------------------------------------------------
// Ingress boundary (transaction hot path)
// ---------------------------------------------------------------------------

// IngressV2 is the C1.4 boundary of one fully decoded Transactions or
// PooledTransactions packet. validate is geth's own basic validation for the
// hook (handleTransactions). Its result is returned unchanged; a rejected
// packet publishes nothing.
func IngressV2(p *PeerIngressV2, source IngressSourceV2, txs []*types.Transaction, validate func() error) error {
	if p == nil {
		return validate()
	}
	p.packets.Add(1)
	decoded, clockErr := MonotonicNS() // T_packet_decoded
	if err := validate(); err != nil {
		p.rejectedPackets.Add(1)
		return err
	}
	if clockErr != nil {
		p.clockErrors.Add(1)
		return nil
	}
	p.publishValidated(source, decoded, txs)
	return nil
}

func (p *PeerIngressV2) publishValidated(source IngressSourceV2, decoded uint64, txs []*types.Transaction) {
	ingress, err := MonotonicNS() // T_ingress
	if err != nil || decoded == 0 {
		p.clockErrors.Add(1)
		return
	}
	scope := p.runtime.scope
	matched := uint64(0)
	for _, tx := range txs {
		if !scope.match(tx) {
			continue
		}
		matched++
		if p.lane == nil || p.closed {
			continue
		}
		raw, err := serializeTxV2(tx)
		if err != nil {
			p.serializeErrors.Add(1)
			continue
		}
		record := recordV2{
			Kind:        EarlyTxSeen,
			TimestampNS: ingress,
			ClockID:     v2ClockMonotonic,
			Source:      [16]byte(source),
			Peer:        p.owner,
			Hash:        tx.Hash(),
			Aux:         ingressAuxV2(decoded),
			Payload:     raw,
		}
		switch err := p.lane.publishLeased(record, p.lease.Epoch); {
		case err == nil:
			p.published.Add(1)
		case errors.Is(err, errV2QueueFull):
			p.queueFull.Add(1)
		case errors.Is(err, errV2PayloadTooLarge):
			p.oversize.Add(1)
		default:
			p.refused.Add(1)
		}
	}
	p.matched.Add(matched)
	p.filtered.Add(uint64(len(txs)) - matched)
	switch {
	case matched == 0:
	case p.closed:
		// A handle of a closed binding never writes.
		p.refused.Add(matched)
	case p.lane == nil:
		p.uncoveredCandidates.Add(matched)
	}
}

// ingressAuxV2 encodes the aux field of an EARLY_TX_SEEN record:
// aux[0..8] = T_packet_decoded (u64 LE, same clock as timestamp_ns),
// aux[8..32] = zero, reserved.
func ingressAuxV2(decoded uint64) [32]byte {
	var aux [32]byte
	binary.LittleEndian.PutUint64(aux[0:8], decoded)
	return aux
}

// PublishTxpoolAcceptedV2 records every transaction the local txpool
// accepted, independent of the peer prefilter, on the dedicated Txpool lane.
// Its only caller is geth's txBroadcastLoop, the lane's sole producer.
func PublishTxpoolAcceptedV2(txs []*types.Transaction) {
	r := ingressActiveV2.Load()
	if r == nil {
		return
	}
	r.publishTxpool(txs)
}

func (r *ingressRuntimeV2) publishTxpool(txs []*types.Transaction) {
	now, err := MonotonicNS()
	if err != nil {
		// Every transaction of the batch stays unpublished; count each one.
		r.txpoolClockErrors.Add(uint64(len(txs)))
		return
	}
	for _, tx := range txs {
		raw, err := serializeTxV2(tx)
		if err != nil {
			r.txpoolSerializeErrors.Add(1)
			continue
		}
		record := recordV2{
			Kind:        TxpoolAccepted,
			TimestampNS: now,
			ClockID:     v2ClockMonotonic,
			Source:      [16]byte(ingressSourceTxpool),
			Hash:        tx.Hash(),
			Payload:     raw,
		}
		// Queue full and oversize are also counted in the lane header.
		if err := r.txpool.publish(record); err != nil {
			r.txpoolRefused.Add(1)
			continue
		}
		r.txpoolPublished.Add(1)
	}
}

// ---------------------------------------------------------------------------
// Evidence
// ---------------------------------------------------------------------------

// IngressScopeEvidenceV2 describes the bound prefilter snapshot.
type IngressScopeEvidenceV2 struct {
	Version      string   `json:"version"`
	ChainID      uint64   `json:"chain_id"`
	Addresses    []string `json:"addresses"`
	Selectors    []string `json:"selectors"`
	CanonicalHex string   `json:"canonical_hex"`
	Fingerprint  string   `json:"fingerprint_crc32c"`
}

// IngressEvidenceV2 is the readable C1.4 runtime evidence.
type IngressEvidenceV2 struct {
	Dir                  string                 `json:"dir"`
	Generation           uint64                 `json:"generation"`
	MaxPeers             int                    `json:"max_peers"`
	ChurnReserve         int                    `json:"churn_reserve"`
	PoolBound            int                    `json:"qualified_pool_bound"`
	PeerLaneCapacity     uint32                 `json:"peer_lane_capacity"`
	PeerLaneSlotStride   uint32                 `json:"peer_lane_slot_stride"`
	TxpoolLaneCapacity   uint32                 `json:"txpool_lane_capacity"`
	TxpoolLaneSlotStride uint32                 `json:"txpool_lane_slot_stride"`
	Scope                IngressScopeEvidenceV2 `json:"prefilter_scope"`
	Pool                 peerLanePoolStatsV2    `json:"pool"`
	LivePeers            int                    `json:"live_peers"`
	LiveCovered          int                    `json:"live_covered"`
	LiveUncovered        int                    `json:"live_uncovered"`
	CoveredBinds         uint64                 `json:"covered_binds"`
	UncoveredBinds       uint64                 `json:"uncovered_binds"`
	PendingRelease       int                    `json:"pending_release"`
	Reclaimed            uint64                 `json:"reclaimed"`
	ReleaseFailures      uint64                 `json:"release_failures"`
	CloseFailures        uint64                 `json:"close_failures"`
	Peers                PeerIngressCountersV2  `json:"peers"`
	TxpoolPublished      uint64                 `json:"txpool_published"`
	TxpoolRefused        uint64                 `json:"txpool_refused"`
	TxpoolSerializeErrs  uint64                 `json:"txpool_serialize_errors"`
	TxpoolClockErrors    uint64                 `json:"txpool_clock_errors"`
	TxpoolLane           laneStatsV2            `json:"txpool_lane"`
	GapTotal             uint64                 `json:"gap_total"`
	Gaps                 []IngressGapEventV2    `json:"gaps"`
}

// IngressV2Snapshot returns the active runtime's evidence; false when the
// Fast Lane is disabled.
func IngressV2Snapshot() (IngressEvidenceV2, bool) {
	r := ingressActiveV2.Load()
	if r == nil {
		return IngressEvidenceV2{}, false
	}
	return r.snapshot(), true
}

func (s *prefilterScopeV2) evidence() IngressScopeEvidenceV2 {
	addresses := make([]string, 0, len(s.addresses))
	for _, addr := range s.addresses {
		addresses = append(addresses, "0x"+hex.EncodeToString(addr[:]))
	}
	selectors := make([]string, 0, len(s.selectors))
	for _, sel := range s.selectors {
		selectors = append(selectors, "0x"+hex.EncodeToString(sel.Selector[:])+":launch")
	}
	return IngressScopeEvidenceV2{
		Version:      prefilterScopeMagic,
		ChainID:      s.chainID,
		Addresses:    addresses,
		Selectors:    selectors,
		CanonicalHex: hex.EncodeToString(s.canonical),
		Fingerprint:  fmt.Sprintf("0x%08x", s.fingerprint),
	}
}

func (r *ingressRuntimeV2) snapshot() IngressEvidenceV2 {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.reclaimLocked()
	totals := r.closedTotals
	covered, uncovered := 0, 0
	for p := range r.live {
		totals.add(p.counters())
		if p.lane != nil {
			covered++
		} else {
			uncovered++
		}
	}
	return IngressEvidenceV2{
		Dir:                  r.config.Dir,
		Generation:           r.config.Generation,
		MaxPeers:             r.config.MaxPeers,
		ChurnReserve:         r.config.ChurnReserve,
		PoolBound:            c14PeerLanePoolBound,
		PeerLaneCapacity:     r.config.PeerLaneCapacity,
		PeerLaneSlotStride:   r.config.PeerLaneSlotStride,
		TxpoolLaneCapacity:   r.config.TxpoolLaneCapacity,
		TxpoolLaneSlotStride: r.config.TxpoolLaneSlotStride,
		Scope:                r.scope.evidence(),
		Pool:                 r.rt.pool.stats(),
		LivePeers:            len(r.live),
		LiveCovered:          covered,
		LiveUncovered:        uncovered,
		CoveredBinds:         r.coveredBinds.Load(),
		UncoveredBinds:       r.uncoveredBinds.Load(),
		PendingRelease:       len(r.pending),
		Reclaimed:            r.reclaimed.Load(),
		ReleaseFailures:      r.releaseFailures.Load(),
		CloseFailures:        r.closeFailures.Load(),
		Peers:                totals,
		TxpoolPublished:      r.txpoolPublished.Load(),
		TxpoolRefused:        r.txpoolRefused.Load(),
		TxpoolSerializeErrs:  r.txpoolSerializeErrors.Load(),
		TxpoolClockErrors:    r.txpoolClockErrors.Load(),
		TxpoolLane:           r.txpool.stats(),
		GapTotal:             r.gapTotal,
		Gaps:                 append([]IngressGapEventV2(nil), r.gaps...),
	}
}
