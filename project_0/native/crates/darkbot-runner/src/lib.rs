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

pub mod builder;
pub mod fixture;
pub mod pending;
pub mod pipeline;
pub mod record;
pub mod remote_workers;
#[cfg(feature = "crate_health")]
pub mod route;
pub mod shadow_ledger;
pub mod winner;
pub mod winner_tracker;

pub mod lookup_tables;
pub mod market_observer;
