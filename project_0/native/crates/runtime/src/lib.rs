#![recursion_limit = "512"]

//! Runner: Ingress → State → Detection (3 Trigger) → Health → Economics → Planner → TX → Submit → Reconcile.
//!
//! Zwei Modi, ein Code:
//! - `Shadow` (P6-LT): alles bis zur Send-Boundary; Opportunity + gebaute TX werden
//!   aufgezeichnet, nichts wird gesendet.
//! - `Live` (P8-LT): Safety-Check vor Handoff, Reconciliation nach Landing.
//!
//! Die Protokollteile sind Traits: `HealthEngine` (P3, marginfi-Crate) und
//! `ReceivershipBuilder` (P5-06, Instructions aus Crate/IDL). Ohne diese beiden
//! läuft der Runner nur bis Detection und zeichnet `UNAVAILABLE` auf — fail closed.
//!
//! Status: UNTESTED / VERIFY.

pub mod analytics;
pub mod execution;
pub mod market;
pub mod orchestration;

/// Shared persistent layout used by capture, verification and analytics.
pub mod run_layout;
pub mod storage;

// Preserve the published Rust API for binaries, pinned verifier sources and consumers.
// Re-exports refer to the single implementation in its owning domain.
pub use analytics::{econ, fixture, measurement, record, shadow_ledger};
#[cfg(feature = "crate_health")]
pub use execution::route;
pub use execution::{builder, lookup_tables, one_shot, pending, pipeline};
pub use market::{observer as market_observer, winner};
#[cfg(feature = "crate_health")]
pub use orchestration::fe_qualification;
pub use orchestration::{remote_workers, winner_tracker};
