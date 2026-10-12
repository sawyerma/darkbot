//! DarkBot native Solana core.
//!
//! - `ingress`: IngressSource-Contract, Event-Modell, `WsRpcControlSource` (S1-LT),
//!   Transaktionsversions-Erkennung (config → versioned → legacy).
//! - `state`: kanonischer RAM-State mit Bootstrap-Puffer, Snapshot/Reconcile,
//!   Slot/Root-Policy, Blockhash-Cache (S2).
//! - `tx`: TX-Builder v0 (+ v1-Stub hinter Feature `txv1`), Signer (S3-01..03).
//! - `submit`: SubmissionAdapter-Contract, RPC- und Jito-Adapter, Landing-Reconciliation (S3-04..06).
//!
//! Kein Project-0-Wissen in dieser Crate. Kein Provider-Name in Businesslogik:
//! Provider erscheinen nur als URL in der Konfiguration.
//!
//! Status: UNTESTED / VERIFY — geschrieben ohne Compiler und ohne Netzwerk.

pub mod ingress;
pub mod provider;
pub mod state;
pub mod submit;
pub mod tx;

pub use solana_sdk::pubkey::Pubkey;
