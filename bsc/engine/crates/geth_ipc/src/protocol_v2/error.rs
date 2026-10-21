// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// Errors of the Wire V2 arena, lanes and records.

#[derive(Debug)]
pub enum V2Error {
    ZeroGeneration,
    InvalidLaneCount(usize),
    DuplicateLaneId(u16),
    InvalidCapacity(usize),
    InvalidSlotStride(usize),
    SizeOverflow,
    Io(io::Error),
    MapFailed(io::Error),
    BadMagic,
    BadVersion(u16),
    BadEndian(u16),
    BadLayout(&'static str),
    GenerationMismatch {
        expected: u64,
        observed: u64,
    },
    LaneNotFound(u16),
    LaneMismatch(&'static str),
    PayloadTooLarge {
        len: usize,
        max: usize,
    },
    QueueFull {
        sequence: u64,
    },
    Shutdown,
    CorruptSequence {
        expected: u64,
        observed: u64,
    },
    LaneEpochMismatch {
        expected: u64,
        observed: u64,
    },
    ChecksumMismatch {
        expected: u32,
        actual: u32,
    },
    UnknownRecordKind(u16),
    LifecycleState {
        expected: u32,
        observed: u32,
    },
    LaneNotDrained {
        read: u64,
        final_write: u64,
    },
    EpochExhausted,
    /// A lifecycle call named another binding of the lane than the current
    /// one: `expected` is the lane's epoch, `observed` the caller's.
    LeaseEpochMismatch {
        expected: u64,
        observed: u64,
    },
    /// A record was written after the close froze `final_write_sequence`.
    /// The lane has been quarantined.
    LateWriteAfterClose {
        final_write: u64,
        write: u64,
    },
    /// No FREE peer lane is available. Nothing was waited for.
    PoolExhausted {
        peer_lanes: usize,
    },
    /// `max_peers` lanes are already bound.
    PeerLimitReached {
        max_peers: usize,
    },
    /// The pool bounds do not describe the arena's peer lanes.
    InvalidPoolBounds {
        peer_lanes: usize,
        max_peers: usize,
        churn_reserve: usize,
    },
    /// The lane is QUARANTINED; it is not consumed, bound or reused in this
    /// generation.
    LaneQuarantined {
        reason: u64,
    },
    /// The lifecycle state word holds no defined state. The lane has been
    /// quarantined.
    InvalidLifecycleState {
        observed: u32,
    },
    /// Another live process holds this runtime role.
    RoleLeaseHeld {
        role: &'static str,
    },
    /// The arena file already exists; it was neither attached nor
    /// overwritten.
    ArenaExists {
        probe: ArenaProbeV2,
    },
    /// The signed-transaction egress lane has no generic producer; it is
    /// written only against a consumed intent permit (E6-2).
    EgressRequiresPermit,
    /// An open handle of this process already holds this role of the lane;
    /// a lane has exactly one producer and one consumer (E6-3/E7-7).
    LaneRoleHeld {
        lane_id: u16,
        role: &'static str,
    },
}

impl fmt::Display for V2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroGeneration => f.write_str("fastlane v2: generation must be nonzero"),
            Self::InvalidLaneCount(v) => write!(f, "fastlane v2: invalid lane count {v}"),
            Self::DuplicateLaneId(v) => write!(f, "fastlane v2: duplicate lane id {v}"),
            Self::InvalidCapacity(v) => write!(f, "fastlane v2: invalid capacity {v}"),
            Self::InvalidSlotStride(v) => write!(f, "fastlane v2: invalid slot stride {v}"),
            Self::SizeOverflow => f.write_str("fastlane v2: mapping size overflow"),
            Self::Io(e) => write!(f, "fastlane v2: I/O {e}"),
            Self::MapFailed(e) => write!(f, "fastlane v2: mmap {e}"),
            Self::BadMagic => f.write_str("fastlane v2: bad magic"),
            Self::BadVersion(v) => write!(f, "fastlane v2: unsupported version {v}"),
            Self::BadEndian(v) => write!(f, "fastlane v2: bad endian marker {v:#x}"),
            Self::BadLayout(v) => write!(f, "fastlane v2: bad layout {v}"),
            Self::GenerationMismatch { expected, observed } => {
                write!(
                    f,
                    "fastlane v2: generation mismatch {observed} != {expected}"
                )
            }
            Self::LaneNotFound(v) => write!(f, "fastlane v2: lane {v} not found"),
            Self::LaneMismatch(v) => write!(f, "fastlane v2: lane mismatch {v}"),
            Self::PayloadTooLarge { len, max } => {
                write!(f, "fastlane v2: payload {len} exceeds {max}")
            }
            Self::QueueFull { sequence } => write!(f, "fastlane v2: queue full at {sequence}"),
            Self::Shutdown => f.write_str("fastlane v2: lane shutdown"),
            Self::CorruptSequence { expected, observed } => {
                write!(f, "fastlane v2: corrupt sequence {observed} != {expected}")
            }
            Self::LaneEpochMismatch { expected, observed } => {
                write!(f, "fastlane v2: lane epoch {observed} != {expected}")
            }
            Self::ChecksumMismatch { expected, actual } => {
                write!(f, "fastlane v2: checksum {actual:#x} != {expected:#x}")
            }
            Self::UnknownRecordKind(v) => write!(f, "fastlane v2: unknown record kind {v}"),
            Self::LifecycleState { expected, observed } => {
                write!(f, "fastlane v2: lifecycle state {observed} != {expected}")
            }
            Self::LaneNotDrained { read, final_write } => {
                write!(
                    f,
                    "fastlane v2: lane not drained read={read} final_write={final_write}"
                )
            }
            Self::EpochExhausted => f.write_str("fastlane v2: lane epoch exhausted"),
            Self::LeaseEpochMismatch { expected, observed } => {
                write!(
                    f,
                    "fastlane v2: lease epoch {observed} != lane epoch {expected}"
                )
            }
            Self::LateWriteAfterClose { final_write, write } => {
                write!(
                    f,
                    "fastlane v2: write {write} after close at {final_write}; lane quarantined"
                )
            }
            Self::PoolExhausted { peer_lanes } => {
                write!(f, "fastlane v2: no free peer lane among {peer_lanes}")
            }
            Self::PeerLimitReached { max_peers } => {
                write!(f, "fastlane v2: {max_peers} peer lanes already bound")
            }
            Self::InvalidPoolBounds {
                peer_lanes,
                max_peers,
                churn_reserve,
            } => write!(
                f,
                "fastlane v2: pool bounds max_peers={max_peers} + churn_reserve={churn_reserve} \
                 do not match {peer_lanes} peer lanes"
            ),
            Self::LaneQuarantined { reason } => {
                write!(f, "fastlane v2: lane quarantined (reason {reason})")
            }
            Self::InvalidLifecycleState { observed } => {
                write!(f, "fastlane v2: invalid lifecycle state {observed}")
            }
            Self::RoleLeaseHeld { role } => {
                write!(f, "fastlane v2: role {role} is held by another process")
            }
            Self::ArenaExists { probe } => {
                write!(f, "fastlane v2: arena already exists ({probe:?})")
            }
            Self::EgressRequiresPermit => f.write_str(
                "fastlane v2: the egress lane is written only through EgressProducerV2",
            ),
            Self::LaneRoleHeld { lane_id, role } => {
                write!(f, "fastlane v2: lane {lane_id} {role} is already open in this process")
            }
        }
    }
}

impl std::error::Error for V2Error {}
