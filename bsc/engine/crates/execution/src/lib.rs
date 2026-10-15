//! DarkBot BSC runner.
//!
//! The runner owns what touches the outside world: transports, provider
//! framing, the clock reads and the placement of the last deadline check,
//! response parsing and transport measurements. It decides nothing about
//! the trade. Economic intent, nonce, signed bytes, plan, budget, envelope
//! options, validity bounds, capability records and release profiles are
//! `darkbot-bsc-trading`'s and are consumed here as they are.
//!
//! The dependency points one way: this crate depends on the core, never
//! the reverse.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod startup;
pub mod submission;

/// G0.7 acceptance guard: the compiled-in, fail-closed check of the frozen
/// v2 acceptance contract. Built only with the `acceptance-guard` feature;
/// the production runner build does not contain it.
#[cfg(feature = "acceptance-guard")]
pub mod acceptance_guard;

/// G0.7 bounded qualification capture (adapter over `darkbot-bsc-journal`).
/// Built only with the `g07-mainnet` feature.
#[cfg(feature = "g07-mainnet")]
pub mod qualification_capture;

// The unit tests of the release path share the fixtures of the
// integration tests, which name this crate by its name.
#[cfg(test)]
extern crate self as darkbot_bsc_execution;

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;
