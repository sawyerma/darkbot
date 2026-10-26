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
//! - [`tls`]: the production transport of the two builders, a warm
//!   HTTP/1.1-over-TLS connection per builder and the owner that builds and
//!   keeps it outside any dispatch.
//!
//! A diagnostic dispatch runs over any [`Transport`], the TLS transport
//! included. After the accepted Q5A run, the TLS module can bind a concrete
//! 48Club or BlockRazor connection as transport-qualified when its complete
//! measured profile matches fail-closed. This does not make either provider
//! release-qualified: the frozen provider semantic properties are still
//! unqualified, so a release plan remains blocked until those separate
//! gates pass.
//!
//! [`Transport`]: transport::Transport

pub mod blockrazor;
pub mod club48;
// The local node's FastLane egress lane (geth_ipc) is the runtime's.
#[cfg(feature = "runtime")]
pub mod egress;
pub mod frame;
pub mod local_geth;
pub mod response;
pub mod send;
pub mod tls;
pub mod transport;

#[cfg(test)]
mod release_tests;
