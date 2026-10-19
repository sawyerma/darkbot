//! DarkBot Control Gateway (F3-Umfang).
//! Vertrag: frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md. Keine Trading-, Wallet-, Preis-,
//! Entscheidungs-, Signatur- oder Journal-Logik. Nur Transport, Cache, Liveness.

pub mod config;
pub mod engine;
pub mod feeds;
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
pub const SCHEMA_ID: &str = include_str!("../../protocol/schema_id.txt");

#[cfg(test)]
mod schema_id_guard {
    #[test]
    fn schema_id_is_a_bare_sha256() {
        assert_eq!(
            super::SCHEMA_ID.len(),
            64,
            "schema_id.txt must hold exactly the id, no newline"
        );
        assert!(super::SCHEMA_ID.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
