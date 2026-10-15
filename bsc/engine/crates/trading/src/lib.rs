//! DarkBot BSC runtime core.
//!
//! Every semantic in this crate exists exactly once and is shared by replay,
//! PAPER, shadow and LIVE (Runtime Contract §6). The crate performs no I/O.
//!
//! Modules are added as their units are implemented. Ownership, sequencing
//! and acceptance are governed by
//! `bsc/docs/BSC_PRODUCTION_BUILD_MASTER_PLAN.md`.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod chain_state;
pub mod economics;
pub mod intent;
pub mod signer;
pub mod submission_policy;
pub mod transactions;
pub mod venues;
