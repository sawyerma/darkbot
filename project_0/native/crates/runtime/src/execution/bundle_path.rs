//! DEC-11 (FS-B02/FS-B05/FS-F-20): the owner's candidate submitter — the RPC path, and with the
//! JITO_BUNDLE capability the bundle path through a constructed `JitoAdapter`.
//!
//! Message fees and single simulations are always the RPC path's (getFeeForMessage per
//! transaction: the bundle fee model is each transaction's fee plus the tip). A bundle is
//! simulated only by a qualified bundle simulator — a provider qualification this build does not
//! hold (FE-06/P8) — so the bundle simulation stays unavailable, no Jito endpoint is called and
//! FE-04 refuses bundles (`bundle_simulated = false`). Every submission is refused by
//! `ObserveOnly` regardless.
use darkbot_solana::submit::{
    jito::JitoAdapter, rpc::RpcSubmitter, FeeQuoteError, MessageFeeQuote, SimResult,
    SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError,
};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::VersionedTransaction;

/// The capability of the bundle path.
pub const JITO_BUNDLE: &str = "JITO_BUNDLE";
/// The values JITO_BUNDLE may be requested with (OFF at every start).
pub const JITO_BUNDLE_VALUES: [&str; 3] = ["OFF", "OBSERVE", "ACTIVE"];
/// The bundle path is not configured.
pub const JITO_UNCONFIGURED: &str = "P0_JITO_BLOCK_ENGINE_URL or P0_JITO_TIP_ACCOUNT unset";
/// Bundles are not admitted: no qualified bundle simulator.
pub const BUNDLE_SIMULATOR_UNQUALIFIED: &str =
    "bundle simulator not qualified (B02/B05: no provider qualification)";

/// The configured bundle path: the block engine, the bundle simulator and the tip account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleConfig {
    pub block_engine_url: String,
    pub bundle_sim_rpc_url: Option<String>,
    pub tip_account: Pubkey,
}

impl BundleConfig {
    /// From the runtime configuration: both the block engine and the tip account, else none.
    pub fn of(c: &darkbot_solana::runtime_config::RuntimeConfig) -> Option<Self> {
        Some(Self {
            block_engine_url: c.jito_block_engine_url.clone()?,
            bundle_sim_rpc_url: c.bundle_sim_rpc_url.clone(),
            tip_account: c.jito_tip_account.as_deref()?.parse().ok()?,
        })
    }
}

/// The effective bundle path of a request: OFF as requested or without a configuration; else
/// OBSERVE (the adapter is constructed, bundle plans are built and quoted); ACTIVE (bundles
/// admitted) needs a qualified bundle simulator and stays OBSERVE.
pub fn effective(requested: &str, configured: bool) -> (&'static str, &'static str) {
    match (requested, configured) {
        ("OFF", _) => ("OFF", ""),
        (_, false) => ("OFF", JITO_UNCONFIGURED),
        ("ACTIVE", true) => ("OBSERVE", BUNDLE_SIMULATOR_UNQUALIFIED),
        (_, true) => ("OBSERVE", ""),
    }
}

/// The owner's candidate submitter: the RPC path, with a `JitoAdapter` when the bundle path is
/// effective.
pub struct OwnerSubmitter {
    rpc: RpcSubmitter,
    jito: Option<JitoAdapter>,
}

impl OwnerSubmitter {
    pub fn new(rpc: RpcSubmitter) -> Self {
        Self { rpc, jito: None }
    }
    /// The bundle path: constructed from `config` (no network call), or removed.
    pub fn set_bundle_path(
        &mut self,
        config: Option<&BundleConfig>,
        timeout: std::time::Duration,
    ) -> Result<(), String> {
        self.jito = match config {
            None => None,
            Some(c) => Some(
                JitoAdapter::new(
                    c.block_engine_url.clone(),
                    c.bundle_sim_rpc_url.clone(),
                    timeout,
                )
                .map_err(|e| format!("JITO_ADAPTER: {e}"))?,
            ),
        };
        Ok(())
    }
    pub fn bundle_path(&self) -> bool {
        self.jito.is_some()
    }
}

impl SubmissionAdapter for OwnerSubmitter {
    /// A single transaction stays on the RPC path; bundles are the capability's.
    fn path(&self) -> SubmissionPath {
        self.rpc.path()
    }
    fn supports_bundle(&self) -> bool {
        self.jito.is_some()
    }
    /// [FIX-38] the bundle path's transaction versions (v1 not evidenced over Jito).
    fn supports_tx_version(&self, v: darkbot_solana::tx::BuildVersion) -> bool {
        match &self.jito {
            Some(jito) => jito.supports_tx_version(v),
            None => self.rpc.supports_tx_version(v),
        }
    }
    /// Each transaction's fee, also of a bundle's (the RPC path's quote).
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> Result<MessageFeeQuote, FeeQuoteError> {
        self.rpc.fee_for_message(tx, deadline)
    }
    fn simulate(&self, tx: &VersionedTransaction, deadline: std::time::Instant) -> SimResult {
        self.rpc.simulate(tx, deadline)
    }
    /// The simulation boundary: no qualified bundle simulator, no call.
    fn simulate_bundle(
        &self,
        txs: &[VersionedTransaction],
        deadline: std::time::Instant,
    ) -> SimResult {
        match &self.jito {
            Some(_) => SimResult::Unavailable(BUNDLE_SIMULATOR_UNQUALIFIED.into()),
            None => self.rpc.simulate_bundle(txs, deadline),
        }
    }
    fn submit(
        &self,
        tx: &VersionedTransaction,
        t_handoff_ns: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        self.rpc.submit(tx, t_handoff_ns, deadline)
    }
    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        t_handoff_ns: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        match &self.jito {
            Some(jito) => jito.submit_bundle(txs, t_handoff_ns, deadline),
            None => self.rpc.submit_bundle(txs, t_handoff_ns, deadline),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DEC-11: the effective bundle path — OFF as requested or unconfigured, OBSERVE when
    /// configured, ACTIVE held at OBSERVE without a qualified bundle simulator.
    #[test]
    fn dec11_the_bundle_path_is_effective_only_configured_and_never_active() {
        assert_eq!(effective("OFF", true), ("OFF", ""));
        assert_eq!(effective("OBSERVE", false), ("OFF", JITO_UNCONFIGURED));
        assert_eq!(effective("ACTIVE", false), ("OFF", JITO_UNCONFIGURED));
        assert_eq!(effective("OBSERVE", true), ("OBSERVE", ""));
        assert_eq!(
            effective("ACTIVE", true),
            ("OBSERVE", BUNDLE_SIMULATOR_UNQUALIFIED)
        );
        let mut c = darkbot_solana::runtime_config::RuntimeConfig::default();
        assert_eq!(BundleConfig::of(&c), None);
        c.jito_block_engine_url = Some("https://jito.invalid".into());
        assert_eq!(BundleConfig::of(&c), None, "no tip account");
        let tip = Pubkey::new_unique();
        c.jito_tip_account = Some(tip.to_string());
        assert_eq!(BundleConfig::of(&c).map(|b| b.tip_account), Some(tip));
    }

    /// DEC-11 (loopback): OFF builds no Jito adapter; the bundle path holds one and simulates
    /// no bundle without a qualified simulator — neither the block engine nor the configured
    /// simulator receives a single request; fees stay the RPC path's.
    #[test]
    fn dec11_the_bundle_path_calls_no_jito_endpoint() {
        use crate::execution::one_shot::non_landing::loopback::{endpoint, Reply};
        let (url, calls) = endpoint(std::sync::Arc::new(|_, _| Reply::Error));
        let mut s = OwnerSubmitter::new(RpcSubmitter::new(url.clone(), true));
        assert!(!s.bundle_path() && !s.supports_bundle());
        let config = BundleConfig {
            block_engine_url: url.clone(),
            bundle_sim_rpc_url: Some(url),
            tip_account: Pubkey::new_unique(),
        };
        let timeout = std::time::Duration::from_secs(1);
        s.set_bundle_path(Some(&config), timeout).unwrap();
        assert!(s.bundle_path() && s.supports_bundle());
        assert_eq!(
            s.path(),
            SubmissionPath::Rpc,
            "a single transaction stays on RPC"
        );
        assert!(!s.supports_tx_version(darkbot_solana::tx::BuildVersion::V1));
        let deadline = std::time::Instant::now() + timeout;
        assert_eq!(
            s.simulate_bundle(&[], deadline),
            SimResult::Unavailable(BUNDLE_SIMULATOR_UNQUALIFIED.into())
        );
        assert!(calls.lock().unwrap().is_empty(), "no Jito call");
        s.set_bundle_path(None, timeout).unwrap();
        assert!(!s.supports_bundle());
    }
}
