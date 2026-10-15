//! Economics: exact integer arithmetic, slippage policy and the scoped
//! venue quotes (master plan Unit 3, adoption row SN05).
//!
//! Every function is pure and returns a typed error instead of panicking or
//! wrapping. Amounts are atomic integer units; there is no floating point.

pub mod arith;
pub mod fees;
pub mod flap;
pub mod policy;
pub mod slippage;
