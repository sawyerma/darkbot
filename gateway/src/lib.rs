//! DarkBot Control Gateway (F3-Umfang).
//! Vertrag: frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md. Keine Trading-, Wallet-, Preis-,
//! Entscheidungs-, Signatur- oder Journal-Logik. Nur Transport, Cache, Liveness.

pub mod config;
pub mod engine;
pub mod http;
pub mod hub;
pub mod overview;
pub mod proto;
pub mod session;
pub mod topics;
pub mod ws;

pub const PROTOCOL_MAJOR: u32 = 1;
pub const PROTOCOL_MINOR: u32 = 0;
/// Muss mit frontend/src/core/realtime/schemaId.ts übereinstimmen (protocol/scripts/schema-id.sh).
pub const SCHEMA_ID: &str = "4470271cbe2ba6095f09941aeb586dff660b01499f996ac4b0859752639abc18";
