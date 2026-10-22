//! DarkBot Project 0.
//!
//! - `protocol`: Discriminators, Account-Typen, Manifest-Konstanten (P1A) — Werte aus P1A-01 pinnen.
//! - `index`: Reverse-Indices und Schwellen-Index (P2).
//! - `schedule`: Premium- und Tag-Scheduler (P2, zeitgetriebene Trigger b/c).
//! - `health`: Health-Engine-Contract; Implementierung = `marginfi`-Crate (P3, Feature `crate_health`).
//! - `census`: C0 Account-Klassen (healthy/race/long-tail/premium/bad debt/unsupported) und Summen.
//! - `economics`: deterministische Profit-Engine, Premium-Kurve, Failed-Cost-Matrix (P4A).
//! - `prepared`: Prepared-Candidate-Cache mit Invalidation (P2-06/07).
//! - `safety`: Controlled-LIVE Limits und Halts (P8-LT).
//! - `receivership`: Instruction-Builder über `marginfi::accounts/instruction` (P5-06, Feature crate_health).
//! - `execution`: Quarantäne, Backoff, Simulate-first, Confirmation-Policy (Eva-Verhalten adaptiert).
//! - `planner`: Receivership-Sequenzregeln, Allowlist, NO_SWAP (P5 — Regeln; Instruction-Bau via Crate).
//!
//! Status: UNTESTED / VERIFY.

pub mod census;
pub mod economics;
pub mod execution;
pub mod funding;
pub mod health;
pub mod index;
#[cfg(feature = "crate_health")]
pub mod integration;
pub mod planner;
#[cfg(feature = "crate_health")]
pub mod projection;
pub mod protocol;
pub mod receivership;
pub mod safety;
pub mod schedule;
#[cfg(feature = "crate_health")]
pub mod sizing; // [FIX-59] exact protocol sizing; market realizability remains in RouteProvider/P4A
