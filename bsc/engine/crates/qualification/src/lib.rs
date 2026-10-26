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
//!
//! C1 qualification (typed binary records, no JSON): [`c1_latency`] (C1.9
//! `c19_report`), [`c1_hook_path`] (C1.9b `c19b_report`), [`c1_freeze`]
//! (C1.10 `c1_freeze`) and what they share, [`c1_common`].

#![forbid(unsafe_code)]

pub mod c1_common;
pub mod c1_freeze;
pub mod c1_hook_path;
pub mod c1_latency;
pub mod economics_snapshot;
pub mod evidence;
pub mod identity;
pub mod json;
pub mod measure;
pub mod probe;
pub mod stats;
pub mod verdict;

#[cfg(feature = "status-reporter")]
pub mod status;

/// Scope of the DRY binary's structural zero-egress assertion, not a transitive network proof.
pub const DRY_STRUCTURAL_SCOPE_NOTE: &str = "STRUCTURAL_CODE_SCOPE_ONLY: zero-egress flag covers this binary, not all dependencies or read-only RPC helpers; eligible for DRY-1/DRY-4 if all other predicates pass";
