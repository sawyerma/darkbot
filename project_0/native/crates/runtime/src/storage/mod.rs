//! Storage architecture SK-01…09 (PHASENPLAN): capture byte budget and the market journal
//! writer lane. The `.p0j` codec itself stays in `analytics::journal`.
pub mod budget;
pub mod preflight;
pub mod segments;
pub mod writer;
