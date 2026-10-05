//! DarkBot BSC runner.
//!
//! The runner owns what touches the outside world: transports, provider
//! framing, the clock reads and the placement of the last deadline check,
//! response parsing and transport measurements. It decides nothing about
//! the trade. Economic intent, nonce, signed bytes, plan, budget, envelope
//! options, validity bounds, capability records and release profiles are
//! `darkbot-core`'s and are consumed here as they are.
//!
//! The dependency points one way: this crate depends on the core, never
//! the reverse.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod inclusion;

// The unit tests of the release path share the fixtures of the
// integration tests, which name this crate by its name.
#[cfg(test)]
extern crate self as darkbot_runner;

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;
