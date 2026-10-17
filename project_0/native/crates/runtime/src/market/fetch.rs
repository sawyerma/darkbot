//! Bounded JSON-RPC client of the post-capture strict verifier (never used during capture).
use super::*;

pub(super) struct RpcFetcher {
    pub(super) client: reqwest::blocking::Client,
    pub(super) url: String,
}

impl RpcFetcher {
    pub(super) fn new(
        url: String,
        timeout_ms: u64,
        commitment: &'static str,
    ) -> Result<Self, String> {
        let limits = &darkbot_solana::runtime_config::active().limits;
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_millis(limits.rpc_connect_timeout_ms))
            .timeout(Duration::from_millis(timeout_ms.min(limits.rpc_timeout_ms)))
            .build()
            .map_err(|e| format!("enrichment client: {}", e.without_url()))?;
        let _ = commitment; // every verifier request states its own commitment
        Ok(Self { client, url })
    }
}
