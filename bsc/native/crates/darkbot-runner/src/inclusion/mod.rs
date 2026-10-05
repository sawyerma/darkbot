//! Inclusion: the runner part of Unit 5.
//!
//! - [`transport`]: the bounded transport interface, connection states,
//!   the binding of a qualified transport, and the monotonic clock.
//! - [`frame`]: bounded request frames built from a submission plan.
//! - [`club48`], [`blockrazor`], [`local_geth`]: the three adapters'
//!   framing.
//! - [`send`]: the diagnostic and the release dispatch, the placement of
//!   the last deadline check, and response polling.
//! - [`response`]: bounded parsing of provider responses.
//!
//! This is the transport-independent part. A diagnostic dispatch runs over
//! any [`Transport`]; a release dispatch only over a qualified transport,
//! which nothing outside the unit tests can make yet. A concrete TLS
//! connector is a later slice of the same unit. No real provider is
//! contacted, and none is qualified, by anything in here.
//!
//! [`Transport`]: transport::Transport

pub mod blockrazor;
pub mod club48;
pub mod frame;
pub mod local_geth;
pub mod response;
pub mod send;
pub mod transport;

#[cfg(test)]
mod release_tests;
