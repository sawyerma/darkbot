// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// The peer-lane pool: bounded lane leases for connected peers.

/// Size of the peer-lane pool: at most `max_peers` lanes are bound at once;
/// `churn_reserve` further lanes take peers that connect while earlier lanes
/// still drain. Their sum must equal the arena's peer-lane count, so a
/// configuration asking for more peers than the arena qualifies fails at
/// startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolBoundsV2 {
    pub max_peers: usize,
    pub churn_reserve: usize,
}

/// One binding of one peer lane: lane id, owner and the epoch the bind
/// produced. Close and release need all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerLaneLeaseV2 {
    lane_id: u16,
    epoch: u64,
    owner: [u8; OWNER_LEN],
}

impl PeerLaneLeaseV2 {
    pub fn lane_id(&self) -> u16 {
        self.lane_id
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn owner(&self) -> [u8; OWNER_LEN] {
        self.owner
    }
}

/// Pool counters and the current state of every peer lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoolStatsV2 {
    pub peer_lanes: usize,
    pub free: usize,
    pub bound: usize,
    pub closing: usize,
    pub quarantined: usize,
    pub allocations: u64,
    pub exhausted: u64,
    pub peer_limit_refusals: u64,
    pub releases: u64,
}

/// Bounded allocator of the arena's peer-ingress lanes. Never waits and
/// never spins for a lane: no FREE lane and the peer limit are explicit
/// results, counted in [`PoolStatsV2`]. Only FREE lanes are bound; BOUND,
/// CLOSING and QUARANTINED lanes are never taken.
///
/// Peer connect/disconnect is outside the transaction hot path; a pool shared
/// by several connection handlers belongs behind one mutex.
pub struct PeerLanePoolV2 {
    arena: ArenaV2,
    lanes: Vec<(u16, usize)>,
    bounds: PoolBoundsV2,
    shutting_down: bool,
    cursor: usize,
    allocations: u64,
    exhausted: u64,
    peer_limit_refusals: u64,
    releases: u64,
}

impl PeerLanePoolV2 {
    pub fn new(arena: ArenaV2, bounds: PoolBoundsV2) -> Result<Self, V2Error> {
        let mut lanes = Vec::new();
        for lane_id in arena.peer_lane_ids()? {
            let (_, lane_offset) = locate_lane_by_id(&arena.mapping, lane_id)?;
            lanes.push((lane_id, lane_offset));
        }
        let wanted = bounds.max_peers.checked_add(bounds.churn_reserve);
        if bounds.max_peers == 0 || wanted != Some(lanes.len()) {
            return Err(V2Error::InvalidPoolBounds {
                peer_lanes: lanes.len(),
                max_peers: bounds.max_peers,
                churn_reserve: bounds.churn_reserve,
            });
        }
        Ok(Self {
            arena,
            lanes,
            bounds,
            shutting_down: false,
            cursor: 0,
            allocations: 0,
            exhausted: 0,
            peer_limit_refusals: 0,
            releases: 0,
        })
    }

    /// Binds the next FREE peer lane to `owner`, round-robin from the lane
    /// after the previous allocation. Each lane is examined at most once.
    pub fn allocate(&mut self, owner: [u8; OWNER_LEN]) -> Result<PeerLaneLeaseV2, V2Error> {
        if self.shutting_down {
            return Err(V2Error::Shutdown);
        }
        if self.count(LANE_STATE_BOUND) >= self.bounds.max_peers {
            self.peer_limit_refusals = self.peer_limit_refusals.saturating_add(1);
            return Err(V2Error::PeerLimitReached {
                max_peers: self.bounds.max_peers,
            });
        }
        let count = self.lanes.len();
        for step in 0..count {
            let index = (self.cursor + step) % count;
            let (lane_id, lane_offset) = self.lanes[index];
            if lane_state(&self.arena.mapping, lane_offset) != LANE_STATE_FREE {
                continue;
            }
            match self.arena.bind_peer_lane(lane_id, owner) {
                Ok(epoch) => {
                    self.cursor = (index + 1) % count;
                    self.allocations = self.allocations.saturating_add(1);
                    return Ok(PeerLaneLeaseV2 {
                        lane_id,
                        epoch,
                        owner,
                    });
                }
                // The state left FREE between the check and the
                // compare-and-swap (a concurrent quarantine): not this lane.
                Err(V2Error::LifecycleState { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        self.exhausted = self.exhausted.saturating_add(1);
        Err(V2Error::PoolExhausted { peer_lanes: count })
    }

    /// BOUND -> CLOSING for the lease. Returns `final_write_sequence`.
    pub fn begin_close(&mut self, lease: &PeerLaneLeaseV2) -> Result<u64, V2Error> {
        self.arena
            .begin_close_peer_lane(lease.lane_id, lease.owner, lease.epoch)
    }

    /// CLOSING -> FREE for the lease, once its drain is acknowledged.
    pub fn release(&mut self, lease: &PeerLaneLeaseV2) -> Result<(), V2Error> {
        self.arena
            .release_peer_lane(lease.lane_id, lease.owner, lease.epoch)?;
        self.releases = self.releases.saturating_add(1);
        Ok(())
    }

    /// Begins shutdown: no further allocation, and `shutdown = 1` on every
    /// lane of the arena, so no publish and no bind succeeds afterwards.
    /// Close and release of existing leases stay possible. Idempotent.
    pub fn begin_shutdown(&mut self) -> Result<(), V2Error> {
        self.shutting_down = true;
        self.arena.begin_shutdown()
    }

    pub fn bounds(&self) -> PoolBoundsV2 {
        self.bounds
    }

    pub fn arena(&self) -> &ArenaV2 {
        &self.arena
    }

    pub fn arena_mut(&mut self) -> &mut ArenaV2 {
        &mut self.arena
    }

    pub fn stats(&self) -> PoolStatsV2 {
        PoolStatsV2 {
            peer_lanes: self.lanes.len(),
            free: self.count(LANE_STATE_FREE),
            bound: self.count(LANE_STATE_BOUND),
            closing: self.count(LANE_STATE_CLOSING),
            quarantined: self.count(LANE_STATE_QUARANTINED),
            allocations: self.allocations,
            exhausted: self.exhausted,
            peer_limit_refusals: self.peer_limit_refusals,
            releases: self.releases,
        }
    }

    fn count(&self, state: u32) -> usize {
        self.lanes
            .iter()
            .filter(|(_, lane_offset)| lane_state(&self.arena.mapping, *lane_offset) == state)
            .count()
    }
}
