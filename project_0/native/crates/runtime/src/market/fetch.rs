//! Bounded post-capture transaction retrieval.
use super::*;

// ------------------------------------------------------------------------------------------
// Bounded post-capture getTransaction (logs source). No hidden retry, no RPC during capture.
// ------------------------------------------------------------------------------------------

pub(super) struct RpcFetcher {
    pub(super) client: reqwest::blocking::Client,
    pub(super) url: String,
    pub(super) commitment: &'static str,
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
        Ok(Self {
            client,
            url,
            commitment,
        })
    }

    pub(super) fn get_transaction(&self, signature: &str) -> Result<Option<Value>, String> {
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "getTransaction",
            "params": [signature, {"encoding": "json", "commitment": self.commitment, "maxSupportedTransactionVersion": 1}]});
        let _permit = darkbot_solana::provider::shared()
            .acquire(
                darkbot_solana::provider::Workload::History,
                Instant::now()
                    + Duration::from_millis(
                        darkbot_solana::runtime_config::active()
                            .limits
                            .rpc_timeout_ms,
                    ),
            )
            .map_err(str::to_string)?;
        let budget = darkbot_solana::provider::shared();
        budget
            .account_bytes(serde_json::to_vec(&body).map_err(|_| "RPC_ENCODE")?.len())
            .map_err(str::to_string)?;
        let resp = self
            .client
            .post(&self.url)
            .json(&body)
            .send()
            .map_err(|e| e.without_url().to_string())?;
        darkbot_solana::provider::observe_response(&resp, "getTransaction")
            .map_err(str::to_string)?;
        let bytes = darkbot_solana::provider::read_body(
            resp,
            darkbot_solana::runtime_config::active()
                .limits
                .max_response_mib
                * 1024
                * 1024,
            &budget,
        )
        .map_err(str::to_string)?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| "RPC_JSON")?;
        if let Some(err) = v.get("error") {
            let _ = err;
            return Err("RPC_REMOTE_ERROR".into());
        }
        Ok(v.get("result").filter(|r| !r.is_null()).cloned())
    }
}
