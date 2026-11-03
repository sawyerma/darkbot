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
//! - [`loopback`]: the LIVE release path's builder stand-in on 127.0.0.1
//!   (plan 28.10.), qualified under its own implementation, never sending.
//!
//! A diagnostic dispatch runs over any [`Transport`], the TLS transport
//! included. A release dispatch takes only a qualified plan over qualified
//! transports: the LIVE end (`live_end`, `runtime` feature) qualifies each
//! prepared transaction's plan with the verified LIVE qualification (its
//! adapters' qualification records) and binds a 48Club or BlockRazor TLS
//! connection when its complete measured Q5A profile matches fail-closed, or
//! the loopback stand-in; the native owner admits every intent before any
//! byte can leave. Nothing else releases.
//!
//! [`Transport`]: transport::Transport

pub mod blockrazor;
pub mod club48;
// The local node's FastLane egress lane (geth_ipc): no runtime path uses it yet.
#[cfg(feature = "local-egress")]
pub mod egress;
pub mod frame;
pub mod local_geth;
pub mod loopback;
pub mod response;
pub mod send;
pub mod tls;
pub mod transport;

#[cfg(test)]
mod release_tests;
