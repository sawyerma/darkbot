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
        binding: SimulationBinding,
    ) -> Result<SimulatedPayload<'_>, String> {
        match self.simulate_bound(adapter, deadline, binding)? {
            (_, Some(simulated)) => Ok(simulated),
            (_, None) => Err(err("EXACT_PAYLOAD_SIMULATION_NOT_PASSED")),
        }
    }

    /// DEC-02: the one simulation of these exact bytes through the production adapter — its
    /// typed result, and the token only for a typed `Ok` (with the context slot, the time and
    /// what it was bound to). A path the adapter cannot simulate is refused before any call.
    pub fn simulate_bound(
        self,
        adapter: &dyn SubmissionAdapter,
        deadline: std::time::Instant,
        binding: SimulationBinding,
    ) -> Result<(SimResult, Option<SimulatedPayload<'_>>), String> {
        // DEC-11: a bundle needs an adapter with a bundle path (the owner's submitter keeps its
        // single TXs on RPC), a single TX the adapter's own path.
        let compatible = match self.path {
            SubmissionPath::JitoBundle => adapter.supports_bundle(),
            path => adapter.path() == path,
        };
        if !compatible {
            return Err(err("SUBMISSION_PATH_MISMATCH"));
        }
        // The simulation's age counts from its request (never younger than it is).
        let at = std::time::Instant::now();
        let result = if self.path == SubmissionPath::JitoBundle {
            adapter.simulate_bundle(&self.txs, deadline)
        } else {
            adapter.simulate(&self.txs[0], deadline)
        };
        let SimResult::Ok { context_slot, .. } = result else {
            return Ok((result, None));
        };
        let simulated = SimulatedPayload {
            payload: self,
            adapter,
            evidence: Evidence {
                result: result.clone(),
                context_slot,
                at,
                binding,
            },
        };
        Ok((result, Some(simulated)))
    }
}

/// DEC-02: what a simulation was bound to when it ran — the FE-04 state generation (a recovery
/// changes it), the observed ProgramData and whether the canonical state was fresh without gap
/// or fork. A simulation is reused only while all of them are unchanged at the handoff. They are
/// the owner's state: the handoff runs in the same owner step as the simulation (no ingress
/// event is applied in between), so a gap, a recovery, a ProgramData or lookup-table change
/// becomes visible only to a later step, which simulates anew; the chain's progress meanwhile is
/// bounded by the context slot, the age and the handoff's newest observed slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationBinding {
    pub state_generation: u64,
    pub program_data_hash: String,
    pub state_valid: bool,
}

/// DEC-02: the evidence of the one simulation behind a token.
struct Evidence {
    result: SimResult,
    context_slot: Option<u64>,
    at: std::time::Instant,
    binding: SimulationBinding,
}

/// Cannot be deserialized or constructed from an asserted boolean/hash.
pub struct SimulatedPayload<'a> {
    payload: SignedPayload,
    adapter: &'a dyn SubmissionAdapter,
    evidence: Evidence,
}

impl SimulatedPayload<'_> {
    /// P8-CTRL.7: the signatures of the payload it simulated (what an egress permit binds).
    pub fn signatures(&self) -> &[String] {
        &self.payload.signatures
    }
    /// P8-CTRL.7: its submission path.
    pub fn path(&self) -> SubmissionPath {
        self.payload.path
    }
    /// DEC-02 (7): derived from the token — only a typed `Ok` simulation of these exact signed
    /// bytes makes one; never a literal.
    pub fn exact_signed_payload_simulated(&self) -> bool {
        matches!(self.evidence.result, SimResult::Ok { .. })
    }

    /// DEC-02 (2): a bundle's token comes only from the bundle simulation of these bytes.
    pub fn bundle_simulated(&self) -> bool {
        self.exact_signed_payload_simulated() && self.payload.path == SubmissionPath::JitoBundle
    }

    /// DEC-02 (4): the node simulated at or after `state_slot` (unknown is not after).
    pub fn context_reaches(&self, state_slot: u64) -> bool {
        self.evidence.context_slot.is_some_and(|s| s >= state_slot)
    }

    /// DEC-02: why this simulation may not stand for a handoff of `payload` now (`None`: every
    /// condition holds) — the identical wire digest (every signature and message byte, their
    /// order, the transport path and count are framed into it, so another path is another
    /// digest), a context slot at or after the intent's state slot, the binding unchanged since
    /// the simulation, an age within the approval's `max_state_age_ms`. A typed `Ok` (and a
    /// bundle simulation for a bundle) holds by construction: no other simulation makes a token.
    pub fn reuse_refusal(
        &self,
        payload: &SignedPayload,
        state_slot: u64,
        binding: &SimulationBinding,
        max_age: std::time::Duration,
    ) -> Option<&'static str> {
        if self.payload.digest != payload.digest {
            Some("SIM_REUSE_WIRE_DIGEST")
        } else if !self.context_reaches(state_slot) {
            Some("SIM_REUSE_CONTEXT_SLOT")
        } else if &self.evidence.binding != binding {
            Some("SIM_REUSE_STATE_CHANGED")
        } else if self.evidence.at.elapsed() > max_age {
            Some("SIM_REUSE_AGE")
        } else {
            None
        }
    }
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
        hold: Option<crate::execution::hold_b::HoldEvidence>,
        fe06: Option<super::Fe06Attempt>,
    ) -> Result<(), String> {
        if simulated.payload.path == SubmissionPath::JitoBundle && !now.bundle_simulated {
            return Err(err("CURRENT_BUNDLE_EVIDENCE_UNKNOWN"));
        }
        if !simulated.payload.matches(&intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        self.reserve_held(approval, intent, now, hold, fe06)
    }

    /// Exactly one adapter call, only after the UNKNOWN state was fsynced, bounded by the
    /// candidate's monotonic `deadline` (DEC-12). No error (including Rejected, the deadline or
    /// a malformed ACK) releases the hold.
    /// P8-CTRL.7: `permit` is consumed immediately before the handoff — another payload's, a
    /// passed deadline or an expired grant refuses the egress (the reservation stays UNKNOWN-safe).
    /// S4-04 (review H1): `T_handoff` is read on `clock` where it happens — after the durable
    /// writes, immediately before the adapter call — and returned with the ACK.
    pub fn submit_payload(
        &mut self,
        simulated: &SimulatedPayload<'_>,
        now: &Current,
        clock: &darkbot_common::timing::Clock,
        deadline: std::time::Instant,
        permit: crate::orchestration::capability_control::EgressPermit,
    ) -> Result<(SubmitAck, darkbot_common::timing::Point), String> {
        let p = &simulated.payload;
        if p.path == SubmissionPath::JitoBundle && !now.bundle_simulated {
            return Err(err("CURRENT_BUNDLE_EVIDENCE_UNKNOWN"));
        }
        let saved = self.saved.as_ref().ok_or_else(|| err("NOT_RESERVED"))?;
        if !p.matches(&saved.intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        permit.admit(
            crate::orchestration::capability_control::Purpose::NewAttempt,
            &p.signatures,
            p.path,
            now.now_unix_ms / 1000,
        )?;
        let (ack, handoff) = self.handoff(&p.digest, now, || {
            let handoff = darkbot_common::timing::Point::now(clock);
            let result = if p.path == SubmissionPath::JitoBundle {
                simulated
                    .adapter
                    .submit_bundle(&p.txs, handoff.ns, deadline)
            } else {
                simulated.adapter.submit(&p.txs[0], handoff.ns, deadline)
            };
            // M6: the adapter's typed class; whatever it is (a local or remote definitive refusal
            // included), the attempt stays UNKNOWN and keeps its hold. C06: one mapping.
            result.map(|ack| (ack, handoff)).map_err(unknown)
        })?;
        let returned: Vec<_> = ack.signatures.iter().map(ToString::to_string).collect();
        if ack.path != p.path
            || returned != p.signatures
            || !p.signatures.contains(&ack.signature.to_string())
            || ack.serialized_len != p.wire_len
            || ack.t_handoff_ns != handoff.ns
            || (p.path == SubmissionPath::JitoBundle
                && ack.submission_id.as_ref().is_none_or(String::is_empty))
        {
            return Err(err("SUBMISSION_ACK_MISMATCH_UNKNOWN"));
        }
        Ok((ack, handoff)) // ACK is deliberately not a receipt or a released hold.
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
        fe06: Option<super::Fe06Attempt>,
    ) -> Result<(), String> {
        if simulated.payload.path == SubmissionPath::JitoBundle {
            return Err(err("EXIT_BUNDLE_UNSUPPORTED"));
        }
        if !simulated.payload.matches_exit(&intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        self.reserve_exit_booked(approval, intent, now, fe06)
    }

    /// FE-04 exit: exactly one adapter call, only after the exit's UNKNOWN state was fsynced and
    /// bounded by `deadline` (DEC-12); no error releases its hold; the ACK must be this payload's.
    /// Its `T_handoff` is read on `clock` immediately before the adapter call (S4-04).
    pub fn submit_exit_payload(
        &mut self,
        simulated: &SimulatedPayload<'_>,
        now: &Current,
        clock: &darkbot_common::timing::Clock,
        deadline: std::time::Instant,
        permit: crate::orchestration::capability_control::EgressPermit,
    ) -> Result<(SubmitAck, darkbot_common::timing::Point), String> {
        let p = &simulated.payload;
        let (intent, _) = self
            .exit_reserved()
            .ok_or_else(|| err("EXIT_NOT_RESERVED"))?;
        if p.path == SubmissionPath::JitoBundle || !p.matches_exit(&intent) {
            return Err(err("SIGNED_PAYLOAD_CHANGED"));
        }
        permit.admit(
            crate::orchestration::capability_control::Purpose::Exit,
            &p.signatures,
            p.path,
            now.now_unix_ms / 1000,
        )?;
        let (ack, handoff) = self.handoff_exit(&p.digest, now, || {
            let handoff = darkbot_common::timing::Point::now(clock);
            simulated
                .adapter
                .submit(&p.txs[0], handoff.ns, deadline)
                .map(|ack| (ack, handoff))
                .map_err(unknown)
        })?;
        let returned: Vec<_> = ack.signatures.iter().map(ToString::to_string).collect();
        if ack.path != p.path
            || returned != p.signatures
            || ack.signature.to_string() != p.signatures[0]
            || ack.serialized_len != p.wire_len
            || ack.t_handoff_ns != handoff.ns
        {
            return Err(err("SUBMISSION_ACK_MISMATCH_UNKNOWN"));
        }
        Ok((ack, handoff))
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
