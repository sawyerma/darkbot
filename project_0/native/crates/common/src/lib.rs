//! DarkBot native common crate (S0).
//!
//! Enthält nur chain-generische Produktionsbausteine:
//! - `timing`: Timing Contract (T_rx … T_observed_landed) auf monotoner Uhr
//! - `alloc`: Allokationen je Thread, nur mit dem zählenden Allocator (S4-04 Baseline-Build)
//! - `histogram`: allokationsfreie Latenz-Histogramme (p50/p95/p99/max/count)
//! - `channel`: bounded Channels mit explizitem Overflow (kein stiller Overwrite)
//! - `lifecycle`: Shutdown-Signal und RunState (fail-closed HALT)
//! - `journal`: Journal-/Replay-Contract mit Sequenz-Lückenerkennung
//! - `config`: typisierte Env-Config ohne Dependencies
//! - `error`: HaltReason-Taxonomie
//!
//! Keine Chain-Abstraktion. Keine Businesslogik.

pub mod alloc;
pub mod channel;
pub mod config;
pub mod console;
pub mod error;
pub mod histogram;
pub mod journal;
pub mod lifecycle;
pub mod mt0;
pub mod timing;

/// Version des Timing-/Telemetry-Contracts. Änderungen sind Gate-relevant.
pub const CONTRACT_VERSION: &str = "S0-1";
