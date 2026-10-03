//! Flap protocol, supported scope: `newTokenV6` launches of token version 6
//! (`TOKEN_TAXED_V3`) and the direct native-input `swapExactInput` BUY.
//!
//! This is the single implementation of the Flap ABI, the token-address
//! derivation, the deployment binding, the launch classification and the
//! final BUY binding (master plan Unit 2, adoption row SN02). It performs
//! no I/O and decodes without copying.
//!
//! The layers are kept apart:
//!
//! - [`abi`] decodes facts. It accepts every canonical call, including
//!   launches of modes that are not supported, so capture keeps them.
//! - [`deployment`] holds the witnessed deployment constants and validates
//!   the live deployment against them.
//! - [`profile`] is the fail-closed trading admission: a launch is
//!   supported only if every behaviour-relevant field matches a witnessed
//!   execution on a validated deployment.
//! - [`binding`] compares signed BUY bytes with the admitted intent.
//!
//! Typed-transaction ingress validation is a separate boundary and not part
//! of this venue package.

pub mod abi;
pub mod address;
pub mod binding;
pub mod deployment;
pub mod profile;
