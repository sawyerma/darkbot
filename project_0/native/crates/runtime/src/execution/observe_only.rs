//! Submission boundary used by Shadow and combined market/economics captures.
use darkbot_solana::submit::{
    FeeQuoteError, MessageFeeQuote, SimResult, SubmissionAdapter, SubmissionPath, SubmitAck,
    SubmitError,
};
use solana_sdk::transaction::VersionedTransaction;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// Adapter-level defence: even an accidental Live pipeline cannot forward a submission.
/// Simulations and fee quotes are delegated unchanged, with the caller's deadline (N2).
pub struct ObserveOnly<S> {
    inner: S,
    pub attempts: Arc<AtomicU64>,
}
impl<S> ObserveOnly<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            attempts: Arc::new(AtomicU64::new(0)),
        }
    }
    /// The wrapped adapter, to reconfigure (DEC-11: the bundle path); submissions stay refused.
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}
impl<S: SubmissionAdapter> SubmissionAdapter for ObserveOnly<S> {
    fn path(&self) -> SubmissionPath {
        self.inner.path()
    }
    fn supports_bundle(&self) -> bool {
        self.inner.supports_bundle()
    }
    fn supports_tx_version(&self, v: darkbot_solana::tx::BuildVersion) -> bool {
        self.inner.supports_tx_version(v)
    }
    fn fee_for_message(
        &self,
        tx: &VersionedTransaction,
        deadline: std::time::Instant,
    ) -> Result<MessageFeeQuote, FeeQuoteError> {
        self.inner.fee_for_message(tx, deadline)
    }
    fn simulate(&self, tx: &VersionedTransaction, deadline: std::time::Instant) -> SimResult {
        self.inner.simulate(tx, deadline)
    }
    fn simulate_bundle(
        &self,
        txs: &[VersionedTransaction],
        deadline: std::time::Instant,
    ) -> SimResult {
        self.inner.simulate_bundle(txs, deadline)
    }
    fn submit(
        &self,
        _: &VersionedTransaction,
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        Err(SubmitError::Rejected("C0-E1 submission prohibited".into()))
    }
    fn submit_bundle(
        &self,
        _: &[VersionedTransaction],
        _: u64,
        _: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        Err(SubmitError::Rejected(
            "C0-E1 bundle submission prohibited".into(),
        ))
    }
}
