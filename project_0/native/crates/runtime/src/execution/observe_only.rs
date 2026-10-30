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
/// Simulations and fee quotes are delegated unchanged, with the caller's deadline (N2). DEC-13b:
/// only an FE-06 authorization opens it (`open`), and this build constructs none.
pub struct ObserveOnly<S> {
    inner: S,
    pub attempts: Arc<AtomicU64>,
    opened: bool,
}
impl<S> ObserveOnly<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            attempts: Arc::new(AtomicU64::new(0)),
            opened: false,
        }
    }
    /// DEC-13b: submissions are forwarded to the wrapped adapter from now on — only with the
    /// separate FE-06 authorization (no production path holds one).
    pub fn open(&mut self, _fe06: &crate::orchestration::capability_control::Fe06Authorization) {
        self.opened = true;
    }
    /// P8-CTRL.3: submissions are refused again (the grant was revoked or ended).
    pub fn close(&mut self) {
        self.opened = false;
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
        tx: &VersionedTransaction,
        t_handoff_ns: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        if self.opened {
            return self.inner.submit(tx, t_handoff_ns, deadline);
        }
        self.attempts.fetch_add(1, Ordering::Relaxed);
        Err(SubmitError::Rejected("C0-E1 submission prohibited".into()))
    }
    fn submit_bundle(
        &self,
        txs: &[VersionedTransaction],
        t_handoff_ns: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, SubmitError> {
        if self.opened {
            return self.inner.submit_bundle(txs, t_handoff_ns, deadline);
        }
        self.attempts.fetch_add(1, Ordering::Relaxed);
        Err(SubmitError::Rejected(
            "C0-E1 bundle submission prohibited".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wrapped adapter whose every submit succeeds and is counted.
    struct Forwarding(AtomicU64);
    impl SubmissionAdapter for Forwarding {
        fn path(&self) -> SubmissionPath {
            SubmissionPath::Rpc
        }
        fn fee_for_message(
            &self,
            _: &VersionedTransaction,
            _: std::time::Instant,
        ) -> Result<MessageFeeQuote, FeeQuoteError> {
            Err(FeeQuoteError::Unavailable("TEST"))
        }
        fn simulate(&self, _: &VersionedTransaction, _: std::time::Instant) -> SimResult {
            SimResult::Unavailable("TEST".into())
        }
        fn simulate_bundle(&self, _: &[VersionedTransaction], _: std::time::Instant) -> SimResult {
            SimResult::Unavailable("TEST".into())
        }
        fn submit(
            &self,
            tx: &VersionedTransaction,
            t_handoff_ns: u64,
            _: std::time::Instant,
        ) -> Result<SubmitAck, SubmitError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(SubmitAck {
                path: SubmissionPath::Rpc,
                signature: tx.signatures[0],
                signatures: tx.signatures.clone(),
                submission_id: None,
                t_handoff_ns,
                serialized_len: 0,
            })
        }
    }

    /// DEC-13b: closed, every submission is refused and counted, nothing reaches the wrapped
    /// adapter; opened by an FE-06 authorization, a submission is forwarded once.
    #[test]
    fn dec13b_only_fe06_opens_the_submission() {
        let tx = VersionedTransaction {
            signatures: vec![solana_sdk::signature::Signature::new_unique()],
            message: solana_sdk::message::VersionedMessage::Legacy(Default::default()),
        };
        let later = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let mut gate = ObserveOnly::new(Forwarding(AtomicU64::new(0)));
        assert!(gate.submit(&tx, 1, later).is_err());
        assert!(gate
            .submit_bundle(std::slice::from_ref(&tx), 1, later)
            .is_err());
        assert_eq!(gate.attempts.load(Ordering::Relaxed), 2);
        assert_eq!(gate.inner.0.load(Ordering::Relaxed), 0);
        gate.open(&crate::orchestration::capability_control::Fe06Authorization::offline_test());
        assert_eq!(
            gate.submit(&tx, 1, later).unwrap().signature,
            tx.signatures[0]
        );
        assert_eq!(gate.inner.0.load(Ordering::Relaxed), 1);
        assert_eq!(gate.attempts.load(Ordering::Relaxed), 2);
    }
}
