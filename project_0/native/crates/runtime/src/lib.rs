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

/// S4-04: the baseline build (`--features alloc-count`) counts every thread's allocations
/// (`darkbot_common::alloc`); every other build uses the system allocator and counts none.
#[cfg(feature = "alloc-count")]
#[global_allocator]
static ALLOCATOR: darkbot_common::alloc::Counting = darkbot_common::alloc::Counting;

pub mod analytics;
/// F-16: the process console lives in `darkbot-common` (the solana crate prints through it).
pub use darkbot_common::console;
/// Operator env files (`KEY=value` lines; `P0_ENV_FILE`), loaded by the raw capture and by
/// `p0ctl boot` for the same environment (DEC-07).
pub mod env_file;
pub mod execution;
pub mod market;
pub mod orchestration;

/// Shared persistent layout used by capture, verification and analytics.
pub mod run_layout;
pub mod status_tap;
pub mod storage;

// Preserve the published Rust API for binaries, pinned verifier sources and consumers.
// Re-exports refer to the single implementation in its owning domain.
pub use analytics::{econ, measurement, record, shadow_ledger};
#[cfg(feature = "crate_health")]
pub use execution::route;
pub use execution::{builder, lookup_tables, one_shot, pending, pipeline};
pub use market::{observer as market_observer, winner};
#[cfg(feature = "crate_health")]
pub use orchestration::fe_qualification;
pub use orchestration::{remote_workers, winner_tracker};
