//! Bind the durable journal to the existing adapter's exact signed payload.
//! This module neither loads a signer nor enables the normal LIVE entrypoint.
use super::{err, Approval, Current, Intent, Journal};
use darkbot_solana::submit::{
    serialize_tx, SimResult, SubmissionAdapter, SubmissionPath, SubmitAck, SubmitError,
};
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use std::collections::BTreeSet;

/// Immutable owned transactions: no caller can mutate them after simulation.
pub struct SignedPayload {
    txs: Vec<VersionedTransaction>,
    path: SubmissionPath,
    wallet: String,
    signatures: Vec<String>,
    digest: String,
    wire_len: usize,
}

impl SignedPayload {
    pub fn new(
        txs: Vec<VersionedTransaction>,
        path: SubmissionPath,
        wallet: Pubkey,
    ) -> Result<Self, String> {
        if txs.is_empty() || txs.len() > 5 || (path != SubmissionPath::JitoBundle && txs.len() != 1)
        {
            return Err(err("SIGNED_PAYLOAD_COUNT_INVALID"));
        }
        // Domain separation binds transport, count, order and length as well as
        // every signature/message byte. A one-member bundle is not an RPC TX.
        let mut framed = b"Project0/OneShot/SignedPayload/1\0".to_vec();
        framed.extend_from_slice(path.as_str().as_bytes());
        framed.extend_from_slice(&(txs.len() as u64).to_le_bytes());
        let mut signatures = Vec::new();
        let mut wire_len = 0usize;
        for tx in &txs {
            if tx.message.static_account_keys().first() != Some(&wallet)
                || tx.signatures.is_empty()
                || tx.signatures.len() != usize::from(tx.message.header().num_required_signatures)
            {
                return Err(err("PAYLOAD_PAYER_OR_SIGNATURES_INVALID"));
            }
            tx.sanitize().map_err(|_| err("PAYLOAD_MALFORMED"))?;
            tx.verify_and_hash_message()
                .map_err(|_| err("PAYLOAD_SIGNATURE_INVALID"))?;
            let wire = serialize_tx(tx).map_err(|_| err("PAYLOAD_SERIALIZE"))?;
            wire_len = wire_len
                .checked_add(wire.len())
                .ok_or_else(|| err("PAYLOAD_SIZE_OVERFLOW"))?;
            framed.extend_from_slice(&(wire.len() as u64).to_le_bytes());
            framed.extend_from_slice(&wire);
            signatures.push(tx.signatures[0].to_string());
        }
        if signatures.iter().collect::<BTreeSet<_>>().len() != signatures.len() {
            return Err(err("PAYLOAD_DUPLICATE_TRANSACTION"));
        }
        let digest = solana_sdk::hash::hash(&framed)
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self {
            txs,
            path,
            wallet: wallet.to_string(),
            signatures,
            digest,
            wire_len,
        })
    }

    /// These are transaction IDs (first signatures); all co-signatures are
    /// additionally covered by the framed wire digest.
    pub fn signatures(&self) -> &[String] {
        &self.signatures
    }
    pub fn wire_sha256(&self) -> &str {
        &self.digest
    }

    fn matches(&self, i: &Intent) -> bool {
        self.wallet == i.wallet
            && self.signatures == i.signatures
            && self.digest == i.signed_wire_sha256
    }

    fn matches_exit(&self, i: &super::exit::ExitIntent) -> bool {
        self.wallet == i.wallet
            && self.signatures == i.signatures
            && self.digest == i.signed_wire_sha256
    }

    /// Reuse the production adapter's full single-TX or sequential bundle
    /// simulation. Unavailable/failed simulation never yields a sendable token.
    /// N2: the simulation ends by `deadline`; a deadline or a refused call is no PASS.
    pub fn simulate(
        self,
        adapter: &dyn SubmissionAdapter,
        deadline: std::time::Instant,
    ) -> Result<SimulatedPayload<'_>, String> {
        let compatible = match self.path {
            SubmissionPath::JitoBundle => {
                adapter.supports_bundle()
                    && matches!(
                        adapter.path(),
                        SubmissionPath::JitoSingle | SubmissionPath::JitoBundle
                    )
            }
            path => adapter.path() == path,
        };
        if !compatible {
            return Err(err("SUBMISSION_PATH_MISMATCH"));
        }
        let result = if self.path == SubmissionPath::JitoBundle {
            adapter.simulate_bundle(&self.txs, deadline)
        } else {
            adapter.simulate(&self.txs[0], deadline)
        };
        if !matches!(result, SimResult::Ok { .. }) {
            return Err(err("EXACT_PAYLOAD_SIMULATION_NOT_PASSED"));
        }
        Ok(SimulatedPayload {
            payload: self,
            adapter,
        })
    }
}

/// Cannot be deserialized or constructed from an asserted boolean/hash.
pub struct SimulatedPayload<'a> {
    payload: SignedPayload,
    adapter: &'a dyn SubmissionAdapter,
}

impl Journal {
    /// Bind the reservation to the payload that actually passed simulation.
    /// Other current-state/economic gates must still come from production.
    pub fn reserve_payload(
        &mut self,
        approval: Approval,
        intent: Intent,
        now: &Current,
        simulated: &SimulatedPayload<'_>,
    ) -> Result<(), String> {
        if simulated.payload.path == SubmissionPath::JitoBundle && !now.bundle_simulated {
            return Err(err("CURRENT_BUNDLE_EVIDENCE_UNKNOWN"));
        }
        if !simulated.payload.matches(&intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        self.reserve(approval, intent, now)
    }

    /// Exactly one adapter call, only after the UNKNOWN state was fsynced, bounded by the
    /// candidate's monotonic `deadline` (DEC-12). No error (including Rejected, the deadline or
    /// a malformed ACK) releases the hold.
    pub fn submit_payload(
        &mut self,
        simulated: &SimulatedPayload<'_>,
        now: &Current,
        t_handoff_ns: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, String> {
        let p = &simulated.payload;
        if p.path == SubmissionPath::JitoBundle && !now.bundle_simulated {
            return Err(err("CURRENT_BUNDLE_EVIDENCE_UNKNOWN"));
        }
        let saved = self.saved.as_ref().ok_or_else(|| err("NOT_RESERVED"))?;
        if !p.matches(&saved.intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        let ack = self.handoff(&p.digest, now, || {
            let result = if p.path == SubmissionPath::JitoBundle {
                simulated
                    .adapter
                    .submit_bundle(&p.txs, t_handoff_ns, deadline)
            } else {
                simulated.adapter.submit(&p.txs[0], t_handoff_ns, deadline)
            };
            // M6: the adapter's typed class; whatever it is (a local or remote definitive refusal
            // included), the attempt stays UNKNOWN and keeps its hold. C06: one mapping.
            result.map_err(unknown)
        })?;
        let returned: Vec<_> = ack.signatures.iter().map(ToString::to_string).collect();
        if ack.path != p.path
            || returned != p.signatures
            || !p.signatures.contains(&ack.signature.to_string())
            || ack.serialized_len != p.wire_len
            || ack.t_handoff_ns != t_handoff_ns
            || (p.path == SubmissionPath::JitoBundle
                && ack.submission_id.as_ref().is_none_or(String::is_empty))
        {
            return Err(err("SUBMISSION_ACK_MISMATCH_UNKNOWN"));
        }
        Ok(ack) // ACK is deliberately not a receipt or a released hold.
    }
}

/// The adapter's typed submit error: whatever it is, the action stays UNKNOWN.
fn unknown(e: SubmitError) -> String {
    err(match e {
        SubmitError::Rejected(_) => "SUBMISSION_RESULT_UNKNOWN:REJECTED",
        SubmitError::Transport(_) => "SUBMISSION_RESULT_UNKNOWN:TRANSPORT_AMBIGUOUS",
        SubmitError::Serialize(_) => "SUBMISSION_RESULT_UNKNOWN:SERIALIZE",
        SubmitError::Deadline { sent: false } => "SUBMISSION_RESULT_UNKNOWN:DEADLINE_NOT_SENT",
        SubmitError::Deadline { sent: true } => "SUBMISSION_RESULT_UNKNOWN:DEADLINE_IN_FLIGHT",
    })
}

impl Journal {
    /// FE-04 exit: the reservation bound to the single-transaction payload that passed
    /// simulation (an exit is never a bundle).
    pub fn reserve_exit_payload(
        &mut self,
        approval: super::exit::ExitApproval,
        intent: super::exit::ExitIntent,
        now: &Current,
        simulated: &SimulatedPayload<'_>,
    ) -> Result<(), String> {
        if simulated.payload.path == SubmissionPath::JitoBundle {
            return Err(err("EXIT_BUNDLE_UNSUPPORTED"));
        }
        if !simulated.payload.matches_exit(&intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        self.reserve_exit(approval, intent, now)
    }

    /// FE-04 exit: exactly one adapter call, only after the exit's UNKNOWN state was fsynced and
    /// bounded by `deadline` (DEC-12); no error releases its hold; the ACK must be this payload's.
    pub fn submit_exit_payload(
        &mut self,
        simulated: &SimulatedPayload<'_>,
        now: &Current,
        t_handoff_ns: u64,
        deadline: std::time::Instant,
    ) -> Result<SubmitAck, String> {
        let p = &simulated.payload;
        let (intent, _) = self
            .exit_reserved()
            .ok_or_else(|| err("EXIT_NOT_RESERVED"))?;
        if p.path == SubmissionPath::JitoBundle || !p.matches_exit(&intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        let ack = self.handoff_exit(&p.digest, now, || {
            simulated
                .adapter
                .submit(&p.txs[0], t_handoff_ns, deadline)
                .map_err(unknown)
        })?;
        let returned: Vec<_> = ack.signatures.iter().map(ToString::to_string).collect();
        if ack.path != p.path
            || returned != p.signatures
            || ack.signature.to_string() != p.signatures[0]
            || ack.serialized_len != p.wire_len
            || ack.t_handoff_ns != t_handoff_ns
        {
            return Err(err("SUBMISSION_ACK_MISMATCH_UNKNOWN"));
        }
        Ok(ack)
    }
}

#[cfg(test)]
mod dec12_tests {
    use super::*;
    /// DEC-12: every adapter error of a handoff keeps the attempt UNKNOWN; the deadline class
    /// says whether a request left (in flight) or not, never a release.
    #[test]
    fn dec12_deadline_errors_stay_unknown_with_their_class() {
        assert_eq!(
            unknown(SubmitError::Deadline { sent: false }),
            err("SUBMISSION_RESULT_UNKNOWN:DEADLINE_NOT_SENT")
        );
        assert_eq!(
            unknown(SubmitError::Deadline { sent: true }),
            err("SUBMISSION_RESULT_UNKNOWN:DEADLINE_IN_FLIGHT")
        );
    }
}
