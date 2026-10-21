//! The single-sender intent lane. See the module documentation of
//! [`crate::intent`] for the model and its invariants.

use alloy_primitives::{Address, B256};

use super::{
    AdapterId, AdapterSet, ArmedFence, Dispatch, EgressWrite, HaltReason, IntentState, LaneError,
    LaneStats, MAX_ADAPTERS, MAX_LANE_CAPACITY, Outcome, SubmissionState, Ticket,
};
use crate::chain_state::SnapshotId;
use crate::chain_state::canonical::{
    ExecutionStatus, FinalityProof, InclusionProof, OrphanProof, ReconciliationProof,
};
use crate::transactions::legacy::max_signed_len;
use crate::venues::flap::abi::SWAP_EXACT_INPUT_NATIVE_LEN;
use crate::venues::flap::binding::{ExpectedBuy, verify_final_buy};

/// Size of the per-intent buffer for the signed BUY.
const SIGNED_SLOT_LEN: usize = max_signed_len(SWAP_EXACT_INPUT_NATIVE_LEN);

/// Capacity the scratch buffer passed to [`IntentLane::prepare`] must have.
///
/// The final binding check re-encodes the signing payload of the bytes it
/// is given. That payload is never longer than the signed bytes themselves
/// (it carries `chainId, 0, 0` where they carry `v, r, s`), and the lane
/// accepts at most `SIGNED_SLOT_LEN` signed bytes. A buffer of this
/// capacity therefore never grows, for valid and for malformed input. It is
/// not smaller than `max_signing_payload_len` of a BUY.
pub const PREPARE_SCRATCH_CAPACITY: usize = SIGNED_SLOT_LEN;

#[derive(Clone, Copy, Debug)]
struct Entry {
    token: Address,
    state: IntentState,
    outcome: Option<Outcome>,
    /// Hash of the bound signed bytes; zero while `Reserved`.
    tx_hash: B256,
    /// Snapshot the preparation is valid for.
    snapshot: Option<SnapshotId>,
    signed_len: usize,
    submissions: [SubmissionState; MAX_ADAPTERS],
    /// Transaction and block of the latest inclusion evidence.
    included_tx: B256,
    block_hash: B256,
}

impl Entry {
    fn reserved(token: Address) -> Self {
        Self {
            token,
            state: IntentState::Reserved,
            outcome: None,
            tx_hash: B256::ZERO,
            snapshot: None,
            signed_len: 0,
            submissions: [SubmissionState::NotSelected; MAX_ADAPTERS],
            included_tx: B256::ZERO,
            block_hash: B256::ZERO,
        }
    }

    /// Withdraws every permit that has not been consumed.
    fn suppress_unstarted(&mut self) {
        for submission in &mut self.submissions {
            if *submission == SubmissionState::NotStarted {
                *submission = SubmissionState::Suppressed;
            }
        }
    }
}

/// Bounded single-sender lane of economic intents.
///
/// All methods need exclusive access; the lane is owned by one decision
/// core and shared with nobody.
#[derive(Debug)]
pub struct IntentLane {
    chain_id: u64,
    sender: Address,
    first_nonce: u64,
    capacity: usize,
    /// Every token that ever got an intent in this session. Never shrinks.
    seen: Vec<Address>,
    /// Intents in nonce order; index = nonce - `first_nonce`.
    entries: Vec<Entry>,
    /// `capacity` buffers of `SIGNED_SLOT_LEN` bytes, one per entry index.
    signed: Vec<u8>,
    /// Index of the first intent whose nonce is not consumed.
    head: usize,
    halt: Option<HaltReason>,
    stats: LaneStats,
}

impl IntentLane {
    /// Creates an admitting lane behind a durably established fence. The
    /// attestation is consumed; it has no production constructor yet.
    pub fn armed(fence: ArmedFence, capacity: usize) -> Result<Self, LaneError> {
        Self::allocate(
            fence.chain_id,
            fence.sender,
            fence.first_nonce,
            capacity,
            None,
        )
    }

    /// Creates a stopped lane for a restart with an existing fence or any
    /// unresolved previous run. It admits nothing, and nothing in this unit
    /// can release it: [`IntentLane::resume`] refuses a restart quarantine.
    pub fn quarantined(chain_id: u64, sender: Address, capacity: usize) -> Result<Self, LaneError> {
        let halt = Some(HaltReason::RestartQuarantine);
        Self::allocate(chain_id, sender, 0, capacity, halt)
    }

    /// Test-only lane holding one dispatched intent: `raw` with `tx_hash`,
    /// in `Sending`, with a permit for every adapter of `adapters`. The final
    /// binding check is skipped, so the bytes need not be a Portal BUY. For
    /// the egress tests and measurement harnesses of dependent crates behind
    /// the `test-support` feature, which is enabled only under
    /// `[dev-dependencies]`; never a production path.
    #[cfg(any(test, feature = "test-support"))]
    pub fn dispatched_for_tests(
        fence: ArmedFence,
        token: Address,
        raw: &[u8],
        tx_hash: B256,
        adapters: AdapterSet,
    ) -> Result<(Self, Dispatch), LaneError> {
        if raw.len() > SIGNED_SLOT_LEN {
            return Err(LaneError::Oversize);
        }
        let mut lane = Self::armed(fence, 1)?;
        let ticket = lane.reserve(token)?;
        let snapshot = SnapshotId {
            canonical_block_hash: B256::ZERO,
            ordered_state_cursor: 0,
            pending_candidate: tx_hash,
            policy_generation: 0,
            codehash_generation: 0,
        };
        lane.signed[..raw.len()].copy_from_slice(raw);
        let entry = &mut lane.entries[0];
        entry.tx_hash = tx_hash;
        entry.snapshot = Some(snapshot);
        entry.signed_len = raw.len();
        entry.state = IntentState::Prepared;
        let dispatch = lane.begin_send(&snapshot, adapters)?;
        debug_assert_eq!(dispatch.ticket, ticket);
        Ok((lane, dispatch))
    }

    fn allocate(
        chain_id: u64,
        sender: Address,
        first_nonce: u64,
        capacity: usize,
        halt: Option<HaltReason>,
    ) -> Result<Self, LaneError> {
        if capacity == 0 || capacity > MAX_LANE_CAPACITY {
            return Err(LaneError::InvalidCapacity);
        }
        let signed_len = capacity
            .checked_mul(SIGNED_SLOT_LEN)
            .ok_or(LaneError::InvalidCapacity)?;
        Ok(Self {
            chain_id,
            sender,
            first_nonce,
            capacity,
            seen: Vec::with_capacity(capacity),
            entries: Vec::with_capacity(capacity),
            signed: vec![0u8; signed_len],
            head: 0,
            halt,
            stats: LaneStats::default(),
        })
    }

    // -----------------------------------------------------------------
    // Read access
    // -----------------------------------------------------------------

    /// Chain and sender of the lane.
    #[inline]
    pub fn identity(&self) -> (u64, Address) {
        (self.chain_id, self.sender)
    }

    /// Why the lane is stopped, if it is.
    #[inline]
    pub fn halt_reason(&self) -> Option<HaltReason> {
        self.halt
    }

    /// Counters of refused and blocked work.
    #[inline]
    pub fn stats(&self) -> LaneStats {
        self.stats
    }

    /// State of an intent.
    pub fn state(&self, ticket: Ticket) -> Result<IntentState, LaneError> {
        Ok(self.entries[self.index_of(ticket)?].state)
    }

    /// What consumed the nonce of an intent, once evidence said so.
    pub fn outcome(&self, ticket: Ticket) -> Result<Option<Outcome>, LaneError> {
        Ok(self.entries[self.index_of(ticket)?].outcome)
    }

    /// State of one adapter's submission of an intent.
    pub fn submission(
        &self,
        ticket: Ticket,
        adapter: AdapterId,
    ) -> Result<SubmissionState, LaneError> {
        Ok(self.entries[self.index_of(ticket)?].submissions[adapter.index()])
    }

    fn nonce_at(&self, index: usize) -> Option<u64> {
        let offset = u64::try_from(index).ok()?;
        self.first_nonce.checked_add(offset)
    }

    fn index_of(&self, ticket: Ticket) -> Result<usize, LaneError> {
        if ticket.chain_id != self.chain_id || ticket.sender != self.sender {
            return Err(LaneError::UnknownTicket);
        }
        let offset = ticket
            .nonce
            .checked_sub(self.first_nonce)
            .ok_or(LaneError::UnknownTicket)?;
        let index = usize::try_from(offset).map_err(|_| LaneError::UnknownTicket)?;
        match self.entries.get(index) {
            Some(entry) if entry.token == ticket.token => Ok(index),
            _ => Err(LaneError::UnknownTicket),
        }
    }

    fn evidence_index(
        &self,
        chain_id: u64,
        sender: Address,
        nonce: u64,
    ) -> Result<usize, LaneError> {
        if chain_id != self.chain_id || sender != self.sender {
            return Err(LaneError::ForeignEvidence);
        }
        let offset = nonce
            .checked_sub(self.first_nonce)
            .ok_or(LaneError::UnknownNonce)?;
        let index = usize::try_from(offset).map_err(|_| LaneError::UnknownNonce)?;
        if index < self.entries.len() {
            Ok(index)
        } else {
            Err(LaneError::UnknownNonce)
        }
    }

    // -----------------------------------------------------------------
    // Reservation and preparation
    // -----------------------------------------------------------------

    /// Reserves the one intent of `token` and assigns its nonce.
    pub fn reserve(&mut self, token: Address) -> Result<Ticket, LaneError> {
        if self.halt.is_some() {
            return Err(LaneError::Halted);
        }
        if self.seen.contains(&token) {
            self.stats.duplicates += 1;
            return Err(LaneError::Duplicate);
        }
        if self.seen.len() >= self.capacity {
            self.stats.capacity_rejections += 1;
            return Err(LaneError::CapacityExceeded);
        }
        let nonce = self
            .nonce_at(self.entries.len())
            .ok_or(LaneError::NonceExhausted)?;
        if nonce == u64::MAX {
            // The successor of every assigned nonce must exist.
            return Err(LaneError::NonceExhausted);
        }
        // Both vectors were allocated with `capacity`; these never grow.
        self.seen.push(token);
        self.entries.push(Entry::reserved(token));
        Ok(Ticket {
            chain_id: self.chain_id,
            sender: self.sender,
            nonce,
            token,
        })
    }

    /// Binds the signed BUY of a reserved intent.
    ///
    /// The bytes are accepted only if `expected` describes this ticket on
    /// this lane and the final binding check passes for the current
    /// snapshot. The lane keeps its own copy; these are the bytes every
    /// adapter will send. Returns the transaction hash.
    ///
    /// `scratch` must have a capacity of at least
    /// [`PREPARE_SCRATCH_CAPACITY`]; it is cleared and reused, never grown.
    pub fn prepare(
        &mut self,
        ticket: Ticket,
        raw_signed: &[u8],
        expected: &ExpectedBuy,
        current: &SnapshotId,
        scratch: &mut Vec<u8>,
    ) -> Result<B256, LaneError> {
        if self.halt.is_some() {
            return Err(LaneError::Halted);
        }
        let index = self.index_of(ticket)?;
        if self.entries[index].state != IntentState::Reserved {
            return Err(LaneError::InvalidState);
        }
        if expected.chain_id() != self.chain_id
            || expected.sender() != self.sender
            || expected.nonce() != ticket.nonce
            || expected.token() != ticket.token
        {
            return Err(LaneError::BindingMismatch);
        }
        if raw_signed.len() > SIGNED_SLOT_LEN {
            return Err(LaneError::Oversize);
        }
        if scratch.capacity() < PREPARE_SCRATCH_CAPACITY {
            // Refused rather than grown: this call must not allocate.
            return Err(LaneError::ScratchTooSmall);
        }
        let tx_hash =
            verify_final_buy(raw_signed, expected, current, scratch).map_err(LaneError::Binding)?;

        let start = index * SIGNED_SLOT_LEN;
        self.signed[start..start + raw_signed.len()].copy_from_slice(raw_signed);
        let entry = &mut self.entries[index];
        entry.tx_hash = tx_hash;
        entry.snapshot = Some(*current);
        entry.signed_len = raw_signed.len();
        entry.state = IntentState::Prepared;
        Ok(tx_hash)
    }

    /// Discards the preparation of an intent that was never dispatched, so
    /// that it can be prepared again for another snapshot. The nonce stays
    /// assigned to the intent.
    pub fn invalidate(&mut self, ticket: Ticket) -> Result<(), LaneError> {
        if self.halt.is_some() {
            return Err(LaneError::Halted);
        }
        let index = self.index_of(ticket)?;
        let entry = &mut self.entries[index];
        if entry.state != IntentState::Prepared {
            return Err(LaneError::InvalidState);
        }
        entry.state = IntentState::Reserved;
        entry.tx_hash = B256::ZERO;
        entry.snapshot = None;
        entry.signed_len = 0;
        Ok(())
    }

    /// Withdraws the newest intent if it was never dispatched and returns
    /// its nonce to the lane. The token stays deduplicated for the session.
    ///
    /// An intent with later reservations behind it cannot be withdrawn:
    /// their nonces would no longer be contiguous.
    pub fn withdraw(&mut self, ticket: Ticket) -> Result<(), LaneError> {
        if self.halt.is_some() {
            return Err(LaneError::Halted);
        }
        let index = self.index_of(ticket)?;
        if index + 1 != self.entries.len() {
            return Err(LaneError::NotTail);
        }
        match self.entries[index].state {
            IntentState::Reserved | IntentState::Prepared => {}
            _ => return Err(LaneError::InvalidState),
        }
        self.entries.truncate(index);
        self.stats.withdrawn += 1;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Dispatch and submissions
    // -----------------------------------------------------------------

    /// Consumes the one economic admission of the head intent and mints one
    /// permit for every adapter in `adapters`.
    ///
    /// Call immediately before egress. Fails, without changing state, if
    /// the head is not prepared or was prepared for another snapshot than
    /// `current`.
    pub fn begin_send(
        &mut self,
        current: &SnapshotId,
        adapters: AdapterSet,
    ) -> Result<Dispatch, LaneError> {
        if self.halt.is_some() {
            return Err(LaneError::Halted);
        }
        let index = self.head;
        let state = self
            .entries
            .get(index)
            .map(|entry| entry.state)
            .ok_or(LaneError::NotReady)?;
        if state != IntentState::Prepared {
            if self.entries.len() > index + 1 {
                self.stats.head_of_line_blocks += 1;
            }
            return Err(LaneError::NotReady);
        }
        if self.entries[index].snapshot != Some(*current) {
            return Err(LaneError::StalePreparation);
        }
        let nonce = self.nonce_at(index).ok_or(LaneError::NonceExhausted)?;

        let entry = &mut self.entries[index];
        entry.state = IntentState::Sending;
        for (slot, submission) in entry.submissions.iter_mut().enumerate() {
            *submission = if adapters.has_index(slot) {
                SubmissionState::NotStarted
            } else {
                SubmissionState::NotSelected
            };
        }
        Ok(Dispatch {
            ticket: Ticket {
                chain_id: self.chain_id,
                sender: self.sender,
                nonce,
                token: entry.token,
            },
            tx_hash: entry.tx_hash,
            adapters,
        })
    }

    /// Consumes `adapter`'s permit and releases the signed bytes of the
    /// intent to it as an [`EgressWrite`]. Must be called before the
    /// adapter's first transport write of this intent; it is the only way to
    /// obtain the bytes and the only constructor of the egress capability.
    ///
    /// `tx_hash` is the hash of the transaction the adapter is about to
    /// send. Another hash than the intent's stops the lane. A permit is
    /// refused, and marked suppressed, once the lane stopped or the intent
    /// is no longer being sent. Every refusal returns no bytes.
    pub fn start_write(
        &mut self,
        ticket: Ticket,
        adapter: AdapterId,
        tx_hash: B256,
    ) -> Result<EgressWrite<'_>, LaneError> {
        let index = self.index_of(ticket)?;
        let slot = adapter.index();
        match self.entries[index].submissions[slot] {
            SubmissionState::NotStarted => {}
            SubmissionState::NotSelected => return Err(LaneError::AdapterNotSelected),
            SubmissionState::Suppressed => return Err(LaneError::PermitSuppressed),
            _ => return Err(LaneError::PermitConsumed),
        }
        if self.halt.is_some() || self.entries[index].state != IntentState::Sending {
            self.entries[index].submissions[slot] = SubmissionState::Suppressed;
            return Err(LaneError::PermitSuppressed);
        }
        if tx_hash != self.entries[index].tx_hash {
            self.entries[index].suppress_unstarted();
            self.stop(HaltReason::WrongHash);
            return Err(LaneError::WrongHash);
        }
        self.entries[index].submissions[slot] = SubmissionState::Writing;
        let start = index * SIGNED_SLOT_LEN;
        let len = self.entries[index].signed_len;
        Ok(EgressWrite {
            ticket,
            adapter,
            tx_hash,
            bytes: &self.signed[start..start + len],
        })
    }

    /// Records that `adapter`'s transport handoff completed.
    pub fn write_completed(&mut self, ticket: Ticket, adapter: AdapterId) -> Result<(), LaneError> {
        self.advance_submission(
            ticket,
            adapter,
            SubmissionState::Writing,
            SubmissionState::HandedOff,
        )
    }

    /// Records that `adapter`'s write or flush failed after its permit was
    /// consumed. Bytes may have left the process: the submission is
    /// `Unknown`, the intent `Halted`, the lane stops and every permit that
    /// has not started is suppressed.
    pub fn write_failed(&mut self, ticket: Ticket, adapter: AdapterId) -> Result<(), LaneError> {
        let index = self.index_of(ticket)?;
        let slot = adapter.index();
        if self.entries[index].submissions[slot] != SubmissionState::Writing {
            return Err(LaneError::InvalidState);
        }
        self.entries[index].submissions[slot] = SubmissionState::Unknown;
        if self.entries[index].state == IntentState::Sending {
            self.entries[index].state = IntentState::Halted;
        }
        self.entries[index].suppress_unstarted();
        self.stop(HaltReason::UnknownSubmission);
        Ok(())
    }

    /// Records an adapter acknowledgement. It changes the submission only;
    /// it is never an inclusion.
    pub fn acked(&mut self, ticket: Ticket, adapter: AdapterId) -> Result<(), LaneError> {
        self.advance_submission(
            ticket,
            adapter,
            SubmissionState::HandedOff,
            SubmissionState::Acked,
        )
    }

    /// Records an adapter rejection. The intent stays unresolved: a
    /// rejection does not prove that the disclosed bytes cannot execute.
    pub fn rejected(&mut self, ticket: Ticket, adapter: AdapterId) -> Result<(), LaneError> {
        let index = self.index_of(ticket)?;
        let slot = adapter.index();
        match self.entries[index].submissions[slot] {
            SubmissionState::HandedOff | SubmissionState::Acked => {
                self.entries[index].submissions[slot] = SubmissionState::Rejected;
                Ok(())
            }
            _ => Err(LaneError::InvalidState),
        }
    }

    fn advance_submission(
        &mut self,
        ticket: Ticket,
        adapter: AdapterId,
        from: SubmissionState,
        to: SubmissionState,
    ) -> Result<(), LaneError> {
        let index = self.index_of(ticket)?;
        let slot = adapter.index();
        if self.entries[index].submissions[slot] != from {
            return Err(LaneError::InvalidState);
        }
        self.entries[index].submissions[slot] = to;
        Ok(())
    }

    /// Stops the lane: timeout, drop, stale state or any other condition
    /// the runtime cannot resolve. Nothing is released, retried or replaced.
    pub fn halt(&mut self, reason: HaltReason) {
        self.stop(reason);
    }

    /// Stops admission and suppresses every permit that has not started.
    ///
    /// The first reason is kept, with two exceptions that keep the most
    /// severe condition visible: conflicting evidence replaces an earlier
    /// reason, and nothing replaces a restart quarantine.
    fn stop(&mut self, reason: HaltReason) {
        match (self.halt, reason) {
            (None, _) => self.halt = Some(reason),
            (Some(HaltReason::RestartQuarantine), _) => {}
            (Some(_), HaltReason::ConflictingEvidence) => self.halt = Some(reason),
            (Some(_), _) => {}
        }
        // A reorg can rewind the head behind a later intent that is already
        // `Sending`. Suppress unstarted permits across the whole lane, not
        // only the (possibly rewound) head. Already-started submissions are
        // left untouched.
        for entry in &mut self.entries {
            entry.suppress_unstarted();
        }
    }

    // -----------------------------------------------------------------
    // Canonical evidence (produced by the chain-state owner)
    // -----------------------------------------------------------------

    /// Applies evidence that a transaction consumed a nonce of this lane.
    ///
    /// The intent's own transaction makes it `Included` or `Reverted`;
    /// any other transaction makes it `Replaced` and stops the lane. A
    /// revert stops the lane as well. Returns the new intent state.
    ///
    /// Evidence for a nonce that is already consumed is accepted only if it
    /// repeats what is recorded. Another transaction, another block or
    /// another execution result is conflicting evidence and stops the lane.
    pub fn apply_inclusion(&mut self, proof: &InclusionProof) -> Result<IntentState, LaneError> {
        let located = self.evidence_index(proof.chain_id(), proof.sender(), proof.nonce());
        let index = match located {
            Ok(index) => index,
            Err(LaneError::UnknownNonce) if proof.nonce() >= self.first_nonce => {
                // The sender consumed a nonce this lane never assigned.
                self.stop(HaltReason::NonceConsumedByOtherTx);
                return Err(LaneError::UnknownNonce);
            }
            Err(e) => return Err(e),
        };
        let entry = self.entries[index];
        match entry.state {
            IntentState::Included
            | IntentState::Reverted
            | IntentState::Replaced
            | IntentState::Canonical => {
                // Repeated evidence is accepted only if it says the same
                // thing: same transaction, same block and, for the intent's
                // own transaction, the same execution result. The recorded
                // outcome is what remains of that result once the intent is
                // `Canonical`.
                let same =
                    entry.included_tx == proof.tx_hash() && entry.block_hash == proof.block_hash();
                let consistent = match entry.outcome {
                    Some(Outcome::Included) => proof.status() == ExecutionStatus::Succeeded,
                    Some(Outcome::Reverted) => proof.status() == ExecutionStatus::Reverted,
                    // Another transaction consumed the nonce; whether it
                    // succeeded does not change this intent.
                    Some(Outcome::Replaced) => true,
                    None => false,
                };
                if same && consistent {
                    return Ok(entry.state);
                }
                self.stop(HaltReason::ConflictingEvidence);
                return Err(LaneError::ConflictingEvidence);
            }
            IntentState::Reserved
            | IntentState::Prepared
            | IntentState::Sending
            | IntentState::Halted
            | IntentState::Orphaned => {}
        }
        if index != self.head {
            // A nonce cannot be consumed before the one in front of it.
            self.stop(HaltReason::ConflictingEvidence);
            return Err(LaneError::ConflictingEvidence);
        }

        let ours = entry.state != IntentState::Reserved && entry.tx_hash == proof.tx_hash();
        let outcome = match (ours, proof.status()) {
            (true, ExecutionStatus::Succeeded) => Outcome::Included,
            (true, ExecutionStatus::Reverted) => Outcome::Reverted,
            (false, _) => Outcome::Replaced,
        };
        let state = match outcome {
            Outcome::Included => IntentState::Included,
            Outcome::Reverted => IntentState::Reverted,
            Outcome::Replaced => IntentState::Replaced,
        };
        let never_dispatched = matches!(entry.state, IntentState::Reserved | IntentState::Prepared);

        let slot = &mut self.entries[index];
        slot.state = state;
        slot.outcome = Some(outcome);
        slot.included_tx = proof.tx_hash();
        slot.block_hash = proof.block_hash();
        slot.suppress_unstarted();
        self.head = index + 1;

        match outcome {
            Outcome::Replaced => self.stop(HaltReason::NonceConsumedByOtherTx),
            Outcome::Reverted => self.stop(HaltReason::Reverted),
            Outcome::Included => {
                if never_dispatched {
                    // Bytes the lane never released were executed.
                    self.stop(HaltReason::ConflictingEvidence);
                }
            }
        }
        Ok(state)
    }

    /// Applies evidence that the block which consumed a nonce is final
    /// under the acceptance rule. The intent becomes `Canonical`; what
    /// consumed the nonce stays available through [`IntentLane::outcome`].
    pub fn apply_finality(&mut self, proof: &FinalityProof) -> Result<(), LaneError> {
        let index = self.evidence_index(proof.chain_id(), proof.sender(), proof.nonce())?;
        let entry = self.entries[index];
        match entry.state {
            IntentState::Included
            | IntentState::Reverted
            | IntentState::Replaced
            | IntentState::Canonical => {
                if entry.block_hash == proof.block_hash() {
                    self.entries[index].state = IntentState::Canonical;
                    Ok(())
                } else {
                    self.stop(HaltReason::ConflictingEvidence);
                    Err(LaneError::ConflictingEvidence)
                }
            }
            _ => Err(LaneError::InvalidState),
        }
    }

    /// Applies evidence that the block which consumed a nonce left the
    /// canonical chain. The intent becomes `Orphaned`, its nonce counts as
    /// unconsumed again and the lane stops.
    pub fn apply_orphan(&mut self, proof: &OrphanProof) -> Result<(), LaneError> {
        let index = self.evidence_index(proof.chain_id(), proof.sender(), proof.nonce())?;
        let entry = self.entries[index];
        let same_block = entry.block_hash == proof.block_hash();
        match entry.state {
            IntentState::Included | IntentState::Reverted | IntentState::Replaced if same_block => {
                self.entries[index].state = IntentState::Orphaned;
                if index < self.head {
                    self.head = index;
                }
                self.stop(HaltReason::Reorg);
                Ok(())
            }
            IntentState::Orphaned if same_block => Ok(()),
            IntentState::Canonical if same_block => {
                // `Canonical` means accepted under the reviewed rule, not
                // physically impossible to reorg. C2 owns the later reorg
                // evidence; Unit 4 must preserve it and rewind the lane.
                self.entries[index].state = IntentState::Orphaned;
                if index < self.head {
                    self.head = index;
                }
                self.stop(HaltReason::Reorg);
                Ok(())
            }
            IntentState::Canonical => {
                self.stop(HaltReason::ConflictingEvidence);
                Err(LaneError::ConflictingEvidence)
            }
            _ => Err(LaneError::InvalidState),
        }
    }

    /// Resumes a lane that stopped while running, after a completed
    /// reconciliation.
    ///
    /// Accepted only if every consumed nonce is `Canonical`, no dispatched
    /// intent is unresolved, and the reconciled next nonce equals the
    /// lane's.
    ///
    /// A restart quarantine is never released here. The quarantined lane no
    /// longer knows the intents, dedup records and disclosed bytes of the
    /// previous run, and a next nonce does not account for them.
    pub fn resume(&mut self, proof: &ReconciliationProof) -> Result<(), LaneError> {
        if proof.chain_id() != self.chain_id || proof.sender() != self.sender {
            return Err(LaneError::ForeignEvidence);
        }
        match self.halt {
            None => return Err(LaneError::InvalidState),
            Some(HaltReason::RestartQuarantine) => {
                return Err(LaneError::QuarantineNotReleasable);
            }
            Some(_) => {}
        }
        for (index, entry) in self.entries.iter().enumerate() {
            let resolved = if index < self.head {
                entry.state == IntentState::Canonical
            } else {
                matches!(entry.state, IntentState::Reserved | IntentState::Prepared)
            };
            if !resolved {
                return Err(LaneError::UnresolvedIntents);
            }
        }
        if self.nonce_at(self.head) != Some(proof.next_nonce()) {
            return Err(LaneError::NonceMismatch);
        }
        self.halt = None;
        Ok(())
    }
}
