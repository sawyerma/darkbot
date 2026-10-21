//! Single state-owner lifecycle and bounded worker coordination.

pub mod boot_audit;
pub mod control;
#[cfg(feature = "crate_health")]
pub mod fe_qualification;
pub mod provider_evidence;
pub mod remote_workers;
#[cfg(feature = "crate_health")]
mod startup;
#[cfg(feature = "crate_health")]
mod state_owner;
pub mod winner_tracker;
#[cfg(feature = "crate_health")]
pub use startup::{failure_report, run, shadow_failure_code};

#[cfg(feature = "crate_health")]
pub mod fe_replay;

#[cfg(feature = "crate_health")]
pub mod boot_network;
