//! Inclusion: provider-neutral submission semantics (master plan Unit 5,
//! §8; adoption rows SN08, SN10–SN12).
//!
//! This module owns what a submission *means*: which adapter may carry
//! which kind of submission, in which order the transactions go, until
//! when, under which budget, and what can still happen after bytes were
//! disclosed. It contains no transport, no TLS, no HTTP and no provider
//! wire format; those belong to the runner.
//!
//! - [`capability`]: versioned, frozen capability records of the adapters
//!   and the binding of the opaque adapter indices of the intent lane to
//!   them.
//! - [`release`]: release profiles: the envelope options and the block
//!   window a release requires, and what must be established about an
//!   adapter before it counts for a release.
//! - [`plan`]: the immutable submission plan of one economic intent, its
//!   final check before dispatch, and the after-handoff validity model.
//!
//! One economic intent has one plan. Fan-out sends the same BUY, with the
//! same nonce and the same bytes, through several adapters; only the
//! provider envelope may differ. The plan never holds signed bytes: the
//! only source of the BUY bytes is the intent lane, at the moment an
//! adapter consumes its permit.

pub mod capability;
pub mod plan;
pub mod release;

#[cfg(test)]
mod tests;
