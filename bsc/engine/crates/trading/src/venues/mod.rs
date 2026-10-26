//! Venue protocols. One package per protocol family; no shared assumptions
//! between families. The EVM address derivation (EIP-1014 CREATE2, EIP-1167
//! clones) of `flap::address` is shared: it holds no protocol assumption.

pub mod flap;
pub mod fourmeme;
