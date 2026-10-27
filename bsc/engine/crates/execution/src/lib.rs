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

/// G0.7 acceptance guard: the compiled-in, fail-closed check of the binary
/// v3 acceptance contract, and that contract's reviewed values. Built only
/// with the `acceptance-guard` feature; the production runner build does not
/// contain them.
#[cfg(feature = "acceptance-guard")]
pub mod acceptance_contract;
#[cfg(feature = "acceptance-guard")]
pub mod acceptance_guard;
/// The 24-h freshness of the bound builder policy (BSC-004).
#[cfg(feature = "acceptance-guard")]
pub mod builder_policy;
/// The one definition of the G0.7 bundle semantics and wire bodies (BSC-003).
#[cfg(feature = "acceptance-guard")]
pub mod bundle_wire;
/// The launch-path code identity snapshot (v9.1 C1/R1): check and drift.
#[cfg(feature = "acceptance-guard")]
pub mod launch_path;
/// The intrinsic consistency of an R0 upgrade-coverage record (v9.1 D3).
#[cfg(feature = "acceptance-guard")]
pub mod r0;

/// JSON-RPC over curl with endpoint and request on stdin, never on argv.
/// Built only with the `rpc-wire` feature (G0.7, R0 and G0.8 tools).
#[cfg(feature = "rpc-wire")]
pub mod rpc_wire;

/// The one shared, atomically consumed authority of a G0.7 attempt (BSC-005).
#[cfg(feature = "g07-mainnet")]
pub mod attempt_authority;
/// G0.7 bounded qualification capture (adapter over `darkbot-bsc-journal`).
/// Built only with the `g07-mainnet` feature.
#[cfg(feature = "g07-mainnet")]
pub mod qualification_capture;
/// The native owner of the five signal sources' modes (C0/C1, order V2 §5).
#[cfg(feature = "runtime")]
pub mod sources;
/// The native WSS pending-transaction source adapter (verified full txs).
#[cfg(feature = "runtime")]
pub mod wss;
/// The one owner of first sightings across the sources (C1, plan §5a.3).
#[cfg(feature = "runtime")]
pub mod ingress;
/// Canonical launches joined with the pending sightings (G0.4 denominator).
#[cfg(feature = "runtime")]
pub mod canonical_join;
/// The G0.4 market census replayed from the runtime journals.
#[cfg(feature = "runtime")]
pub mod g04_census;
/// The G0.4 verdict against the versioned criteria file.
#[cfg(feature = "runtime")]
pub mod g04_criteria;
/// The runtime's bounded binary journal.
#[cfg(feature = "runtime")]
pub mod runtime_journal;

#[cfg(feature = "runtime")]
pub mod replay_proof;
/// The capacity preflight of a planned scoped capture.
#[cfg(feature = "runtime")]
pub mod capture_preflight;
/// The Phase F quality verdict of a finished run.
#[cfg(feature = "runtime")]
pub mod capture_quality;
/// The read-only runtime process: sources, ingress, journal, status.
#[cfg(feature = "runtime")]
pub mod runtime;
/// Phase H: Strategy C on the journaled canonical records (PAPER).
#[cfg(feature = "runtime")]
pub mod runtime_strategy;
/// The native canonical chain follower: G0.4 launches from canonical receipts.
#[cfg(feature = "runtime")]
pub mod chain_follower;
/// The PAPER identity and sink: the end of the PAPER path (no transport).
#[cfg(feature = "runtime")]
pub mod paper;
/// The C3 production path in PAPER mode, ending at the PAPER sink.
#[cfg(feature = "runtime")]
pub mod paper_exec;
/// The read-only startup probe of the live Flap deployment and venue terms.
#[cfg(feature = "runtime")]
pub mod venue_probe;

// The unit tests of the release path share the fixtures of the
// integration tests, which name this crate by its name.
#[cfg(test)]
extern crate self as darkbot_bsc_execution;

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod test_common;
