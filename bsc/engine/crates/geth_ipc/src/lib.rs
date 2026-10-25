//! C1 bidirectional shared-memory Fast Lane.
//!
//! This crate defines the production wire/ring contract used between pinned
//! bsc-geth and the Rust runtime. It is deliberately transport-only: it does
//! not perform venue semantics, chain reconciliation, signing, provider
//! submission or LIVE authorization.
//!
//! Wire V2 (`protocol_v2`): per-lane one-producer/one-consumer, fixed-capacity,
//! mmap-backed and sequence-numbered rings. Producer publication is Release;
//! consumer observation is Acquire. Queue overflow is explicit and never
//! overwrites unread data.

pub mod host;
pub mod hot0;
pub mod ingress;
pub mod outbound;
pub mod protocol_v2;
pub mod trust;
pub mod wire;

// The shared wire vocabulary keeps its paths at the crate root.
pub use wire::{AUX_LEN, HASH_LEN, PEER_LEN, RecordKind, SOURCE_LEN, UnknownRecordKind};

/// Wire V2 under its pre-R11 module path: the same items as [`protocol_v2`],
/// kept so that existing consumers of `darkbot_bsc_geth_ipc::v2` stay valid.
pub use protocol_v2 as v2;
