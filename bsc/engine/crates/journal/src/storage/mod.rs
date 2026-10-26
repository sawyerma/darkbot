//! Production capture storage (master plan §11, binding Phase F extension):
//! bounded capture memory by bytes, segmented runs, the capacity preflight,
//! the writer worker and its metrics, on top of the existing recorder and
//! the one `DBJNL001` codec.

pub mod budget;
pub mod metrics;
pub mod preflight;
pub mod segments;
pub mod writer;
