//! Production capture storage (master plan §11, binding Phase F extension):
//! bounded capture memory by bytes, segmented runs and the capacity
//! preflight, on top of the existing recorder and the one `DBJNL001` codec.

pub mod budget;
pub mod preflight;
pub mod segments;
