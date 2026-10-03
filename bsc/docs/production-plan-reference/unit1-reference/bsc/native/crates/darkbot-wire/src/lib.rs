//! Primitive wire encodings for the DarkBot BSC runtime.
//!
//! This crate has no runtime dependencies and performs no I/O. It contains exactly one
//! implementation of each encoding; every other crate uses it.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod rlp;
