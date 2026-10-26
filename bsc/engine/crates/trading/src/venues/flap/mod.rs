//! Flap protocol, the full production decoder (Phase D) of the verified
//! Portal v5.27.0: pending recognition of every launch and trade call of
//! the verified ABI ([`recognize`]; `newTokenV6` and `swapExactInput`
//! decoded, the others explicitly unsupported) and canonical decode of the
//! Portal's events with a witnessed live log ([`events`]: launch, BUY,
//! SELL, mode and configuration; migration and the others of the ABI are
//! explicitly unsupported). Trading admission stays the `TOKEN_TAXED_V3`
//! profile, and only on a deployment whose economics are witnessed on its
//! implementation (v5.27.0: G0.6a reconfirmed, `deployment`).
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
//! - [`recognize`] and [`events`] are the one decoder of capture, replay,
//!   PAPER and LIVE (the runtime ingress, the canonical follower and the
//!   PAPER path; FastLane bytes decode identically, the production HOT0
//!   consumer arrives with the synced geth source).
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
pub mod events;
pub mod profile;
pub mod recognize;

#[cfg(test)]
mod decoder_tests;
