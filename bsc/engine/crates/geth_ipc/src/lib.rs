//! C1 bidirectional shared-memory Fast Lane.
//!
//! This crate defines the production wire/ring contract used between pinned
//! bsc-geth and the Rust runtime. It is deliberately transport-only: it does
//! not perform venue semantics, chain reconciliation, signing, provider
//! submission or LIVE authorization.
//!
//! The ring is one-producer/one-consumer, fixed-capacity, mmap-backed and
//! sequence-numbered. Producer publication is Release; consumer observation
//! is Acquire. Queue overflow is explicit and never overwrites unread data.

pub mod ingress;
pub mod protocol_v1;
pub mod v2;

// Wire V1 keeps its public paths at the crate root (`darkbot_bsc_geth_ipc::Producer`, ...).
pub use protocol_v1::*;
