//! Phase H: Strategy C on the BSC production inputs (master plan §13 H).
//!
//! The original backend's policy (`LEGACY_BACKEND_FILTER_V1` of the Solana
//! engine) ported once into Rust on the same state, venue decode and
//! economics as the rest of the engine: the canonical follower's Flap
//! launches, terms and trades, the exact curve replay (`book`), the window
//! features (`features`, exact fixed point), the typed filters (`filters`)
//! and the deterministic lifecycle (`engine`) with complete entry and exit
//! costs (`economics::flap::position`). PAPER only: nothing here signs or
//! sends; Solana observation-time results are not carried over.

pub mod book;
pub mod engine;
pub mod features;
pub mod filters;
pub mod fixed;

#[cfg(test)]
mod engine_tests;
