//! C1.2 Fast-Lane Wire V2 with the C1.3 peer-lane lifecycle.
//!
//! Wire V2 is intentionally implemented alongside V1 until C1.4 switches the
//! geth ingress hooks. The arena contains independently owned SPSC lanes.
//! C1.5 replaces the owned receive copy with a borrowed slot guard.
//!
//! # Peer-lane lifecycle (C1.3)
//!
//! ```text
//! FREE --bind--> BOUND --close--> CLOSING --drain ack--> (drained) --release--> FREE
//!   any state --quarantine--> QUARANTINED (terminal for this generation)
//! ```
//!
//! - Every lifecycle field (`lane_epoch`, state, `final_write_sequence`,
//!   `shutdown`, the four owner words, `drained_epoch`) is accessed atomically
//!   only; the byte layout is the frozen C1.2 one.
//! - State changes are compare-and-swap transitions. Field writes precede the
//!   transition and are published by its Release; readers Acquire the state
//!   first.
//! - A lease is `(lane id, owner, lane_epoch)`. Close and release require the
//!   exact owner and epoch, so a lease of an earlier binding never acts on a
//!   reused lane, even with the same owner bytes.
//! - Every bind increments `lane_epoch`; exhaustion fails closed. Sequence
//!   numbers stay monotonic across reuse: a slot of an earlier binding carries
//!   a `committed_sequence` that can never equal a future expected one, and
//!   its record epoch is checked as well.
//! - A record written after `final_write_sequence` was frozen quarantines the
//!   lane instead of releasing it.
//! - Peer connect/disconnect and allocation are outside the transaction hot
//!   path; `publish` keeps exactly its C1.2 work, including the state check.
//!
//! # Borrowed receive (C1.5)
//!
//! [`ConsumerV2::try_acquire`] and [`LaneSetV2::try_acquire`] validate the
//! next committed record exactly as [`ConsumerV2::try_consume`] always did and
//! return a [`RecordGuardV2`]. The guard exposes the record as a
//! [`BorrowedRecordV2`] that points into the mmap slot; nothing is copied to
//! the heap. `read_sequence` advances only when the guard is released or
//! dropped, so the producer, which refuses to publish while
//! `write - read >= capacity`, cannot reuse the slot earlier and can reuse it
//! immediately afterwards. The guard borrows its consumer exclusively and the
//! borrowed record borrows the guard, so neither can outlive the slot.

#[cfg(not(target_endian = "little"))]
compile_error!("Fast-Lane Wire V2 requires a little-endian target");

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::marker::PhantomData;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::{AUX_LEN, HASH_LEN, PEER_LEN, RecordKind, SOURCE_LEN};

// Wire V2 stays ONE Rust module: its parts live in separate files by
// responsibility but are included into this module, so that privacy and,
// above all, the release code generation of the hot path (inlining across
// what would otherwise be separate codegen units) remain identical to the
// single-file implementation they were split from.
include!("arena.rs");
include!("checksum.rs");
include!("consumer.rs");
include!("error.rs");
include!("guards.rs");
include!("lanes.rs");
include!("layout.rs");
include!("lifecycle.rs");
include!("mapping.rs");
include!("peer_pool.rs");
include!("producer.rs");
include!("records.rs");
include!("role_lease.rs");

#[cfg(test)]
mod c15_tests;
#[cfg(test)]
mod tests;
