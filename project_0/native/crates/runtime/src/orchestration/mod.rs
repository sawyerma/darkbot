//! Single state-owner lifecycle and bounded worker coordination.

pub mod autostart;
pub mod boot_audit;
pub mod boot_control;
pub mod capability_control;
pub mod control;
#[cfg(feature = "crate_health")]
pub mod fe_freeze;
#[cfg(feature = "crate_health")]
pub mod fe_qualification;
pub mod provider_evidence;
pub mod remote_workers;
pub mod source_control;
#[cfg(feature = "crate_health")]
mod startup;
#[cfg(feature = "crate_health")]
mod state_owner;
pub mod until_stopped;
pub mod until_stopped_view;
pub mod winner_tracker;
#[cfg(feature = "crate_health")]
pub use startup::{failure_report, run, run_id_from_env, run_with, shadow_failure_code};

#[cfg(feature = "crate_health")]
pub mod fe_replay;

#[cfg(feature = "crate_health")]
pub mod boot_network;
