//! Single state-owner lifecycle and bounded worker coordination.

#[cfg(feature = "crate_health")]
pub mod fe_qualification;
pub mod remote_workers;
#[cfg(feature = "crate_health")]
mod startup;
#[cfg(feature = "crate_health")]
mod state_owner;
pub mod winner_tracker;
#[cfg(feature = "crate_health")]
pub use startup::run;

#[cfg(feature = "crate_health")]
pub mod fe_replay;
