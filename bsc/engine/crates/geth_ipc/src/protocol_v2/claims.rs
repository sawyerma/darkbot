// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// In-process exclusive lane roles (E6-3/E7-7).
//
// Memory-ordering contract. Slot bytes are plain, non-atomic mmap bytes. They
// are race-free only because exactly one producer and one consumer own a lane:
// the producer reuses a slot only after it observed (Acquire) the consumer's
// Release of `read_sequence`, and the consumer reads a slot only after it
// observed (Acquire) the producer's Release of `committed_sequence` and
// `write_sequence`. No fence or commit re-check makes a second, concurrent
// writer safe or reliably detectable, so a second writer is excluded instead:
// - other Unix users by the directory trust contract (`crate::trust`, E7-5);
// - a second geth or Rust process by the `geth-owner` / `rust-owner` role
//   leases, which a production runtime holds for its whole life;
// - a second producer or consumer of the same lane in this process by the
//   claims below: opening a role of a lane that an open handle already holds
//   fails with [`V2Error::LaneRoleHeld`]. Claims are keyed by the arena file's
//   device and inode, so separate mappings of one arena cannot bypass them.
// The commit re-check of a guard detects a visible change of the commit word
// only; it is a diagnostic, not a rewrite or memory-safety guarantee.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LaneRoleV2 {
    Producer,
    Consumer,
}

impl LaneRoleV2 {
    fn name(self) -> &'static str {
        match self {
            Self::Producer => "producer",
            Self::Consumer => "consumer",
        }
    }
}

type LaneClaimKeyV2 = (u64, u64, u16, LaneRoleV2);

static LANE_CLAIMS_V2: std::sync::Mutex<BTreeSet<LaneClaimKeyV2>> =
    std::sync::Mutex::new(BTreeSet::new());

/// One held lane role; released when dropped.
struct LaneClaimV2 {
    keys: Vec<LaneClaimKeyV2>,
}

impl LaneClaimV2 {
    /// Claims `role` of every lane in `lane_ids` of the arena with the
    /// `(device, inode)` identity, all or none. Cold path: only at open.
    fn acquire(identity: (u64, u64), lane_ids: &[u16], role: LaneRoleV2) -> Result<Self, V2Error> {
        let keys: Vec<LaneClaimKeyV2> = lane_ids
            .iter()
            .map(|&lane| (identity.0, identity.1, lane, role))
            .collect();
        let mut claims = LANE_CLAIMS_V2.lock().unwrap_or_else(|e| e.into_inner());
        for (index, key) in keys.iter().enumerate() {
            if claims.contains(key) || keys[..index].contains(key) {
                return Err(V2Error::LaneRoleHeld {
                    lane_id: key.2,
                    role: role.name(),
                });
            }
        }
        claims.extend(keys.iter().copied());
        Ok(Self { keys })
    }
}

/// The `(device, inode)` identity of an opened arena file.
fn arena_identity(file: &File) -> Result<(u64, u64), V2Error> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(V2Error::Io)?;
    Ok((metadata.dev(), metadata.ino()))
}

impl Drop for LaneClaimV2 {
    fn drop(&mut self) {
        let mut claims = LANE_CLAIMS_V2.lock().unwrap_or_else(|e| e.into_inner());
        for key in &self.keys {
            claims.remove(key);
        }
    }
}
