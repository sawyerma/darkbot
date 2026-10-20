//! DarkBot BSC qualification harnesses.
//!
//! This crate measures frozen production code on the real network and
//! writes the evidence a qualification decision is made from. It decides
//! nothing itself: it never constructs a qualified transport, never marks a
//! provider feature or property as qualified, and never sends anything
//! that could execute.
//!
//! Q5A, the real TLS transport qualification:
//!
//! - [`probe`]: the safe application probe, an `eth_sendBundle` without
//!   transactions that no builder can include;
//! - [`measure`]: the measurement over the frozen `TlsConnectionOwner` and
//!   `TlsTransport`;
//! - [`evidence`]: what is recorded, and its JSON form;
//! - [`stats`] and [`verdict`]: aggregation and the acceptance rules;
//! - [`identity`]: what the build is bound to;
//! - [`json`]: a small JSON writer.

#![forbid(unsafe_code)]

pub mod evidence;
pub mod identity;
pub mod json;
pub mod measure;
pub mod probe;
pub mod stats;
pub mod verdict;

pub mod status;
