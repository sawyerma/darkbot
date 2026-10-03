//! DarkBot BSC runtime core.
//!
//! Every semantic in this crate exists exactly once and is shared by replay,
//! PAPER, shadow and LIVE (Runtime Contract §6). The crate performs no I/O.
//!
//! Modules are added as their Gate-0 implementations are consolidated; see
//! `bsc/native/C0_INVENTORY.md` for the mapping of every Gate-0 function to
//! its production module.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod signer;
pub mod tx;
