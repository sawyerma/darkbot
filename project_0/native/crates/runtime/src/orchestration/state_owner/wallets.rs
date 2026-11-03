//! DEC-17b: the owner's wallets — one per configured signer, each with its own reservation book,
//! Safety (limits, day, HALT), FE-04 journal lane (`<wallet>.p0j` in the one journal directory),
//! admission, approval and ATAs — and the deterministic choice of the wallet a candidate runs
//! with: among the wallets that may run it, the most SOL available after every hold (exit
//! reserves included) and the policy reserve, the lowest key on a tie; never round robin. An
//! error, UNKNOWN or HALT of a wallet blocks that wallet only.
use super::*;
use crate::execution::reservation::WalletReservations;

/// One configured signer of the owner with everything that is its own.
pub(super) struct OwnerWallet {
    /// The liquidator (the marginfi account authority) — in production the payer's key.
    pub(super) liquidator: Pubkey,
    /// The validated roles (`runtime_config::roles`).
    pub(super) roles: Vec<&'static str>,
    /// DEC-18 §4.3 (P0-024): the keypair, or the public identity only (a treasury, the econ4h
    /// calibration).
    pub(super) payer: crate::execution::payer::OwnerPayer,
    /// C11: the wallet's ATA addresses per mint, derived once.
    pub(super) atas: accounts::LiquidatorAtas,
    /// The wallet's limits, day and HALT (DEC-17g: a HALT blocks new attempts, never exits).
    pub(super) safety: Safety,
    /// P8-RES-01: the wallet's reservation book; every offer and attempt of it sees its holds.
    pub(super) reservations: WalletReservations,
    /// FE-04/C3: the wallet journal of an existing attempt, held for the run (one runtime per
    /// project): the reservation book follows it and its UNKNOWN attempt is settled here.
    pub(super) fe04_held: Option<crate::execution::one_shot::Journal>,
    /// DEC-10: the configured FE-04 approval (`P0_FE04_APPROVAL`) bound to this wallet, re-read
    /// with the journal.
    pub(super) fe04_approval: Option<ApprovalSource>,
    /// When the next settlement step is due (`None`: a read in flight, or nothing to settle).
    pub(super) fe04_settle_due: Option<std::time::Instant>,
    /// What Safety has still to book for that attempt (`None`: nothing, or booked).
    pub(super) fe04_unbooked: Option<fe04::Fe04Unbooked>,
    /// DEC-03: the latest non-landing read of this run (the Wallets projection).
    pub(super) fe04_non_landing: fe04::Fe04NonLanding,
    /// DEC-13b: the open FE-04 admission of LIVE candidates (journal, attempt approval, current
    /// wallet facts); `None` while LIVE is not effective, no VALID attempt approval exists or an
    /// attempt holds the wallet. Handed to the pipeline per detection and taken back.
    pub(super) fe04_admission: Option<crate::execution::one_shot::admission::Fe04Admission>,
    /// S4-04: when the admission may be opened ahead of a candidate again after it could not be
    /// (`None`: at once).
    pub(super) fe04_prewarm_after: Option<std::time::Instant>,
    /// DEC-13b/DEC-02: this run's admission counters (the Liquidator tab).
    pub(super) fe04_admissions: fe04::Fe04AdmissionStats,
    /// P0-PROD-CTRL-04: the FE-04 accounting of the wallet's journals, read at the journal
    /// refresh cadence.
    pub(super) ledger_view: Option<Result<status_view::LedgerView, String>>,
    /// DEC-17b: the wallet's attempt records as last read (its journals, or its held journal) —
    /// the grant quotas count every wallet's (`None`: never read in this run).
    pub(super) records: Option<Vec<crate::execution::accounting::AttemptRecord>>,
    /// DEC-04: the B evidence of the wallet's journals (the hold gate sums every wallet's;
    /// `None`: unreadable — then none at all, an underestimation could be missed).
    pub(super) hold_evidence: Option<crate::execution::hold_b::BEvidence>,
    /// Candidates of this run evaluated with this wallet.
    pub(super) selected: u64,
}

impl OwnerWallet {
    pub(super) fn new(
        liquidator: Pubkey,
        roles: Vec<&'static str>,
        payer: crate::execution::payer::OwnerPayer,
        safety: Safety,
    ) -> Self {
        Self {
            liquidator,
            roles,
            payer,
            atas: accounts::LiquidatorAtas::new(liquidator),
            safety,
            reservations: Default::default(),
            fe04_held: None,
            fe04_approval: None,
            fe04_settle_due: None,
            fe04_unbooked: None,
            fe04_non_landing: Default::default(),
            fe04_admission: None,
            fe04_prewarm_after: None,
            fe04_admissions: Default::default(),
            ledger_view: None,
            records: None,
            hold_evidence: Some(Default::default()),
            selected: 0,
        }
    }

    /// A wallet that liquidates and pays its own fees — the one kind a candidate runs with.
    pub(super) fn runs_candidates(&self) -> bool {
        self.roles == darkbot_solana::runtime_config::roles::SIGNING
    }

    /// The wallet as a grant's scope names it (`<pubkey>:<ROLE>+<ROLE>`).
    pub(super) fn scope_entry(&self) -> String {
        format!("{}:{}", self.liquidator, self.roles.join("+"))
    }
}

/// DEC-17b: this run's wallet choices — how many candidates ran with a wallet that may not run
/// them (none could: its refusal is the candidate's) and why the last one could not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SelectionStats {
    pub(super) selections: u64,
    pub(super) none_eligible: u64,
    pub(super) last_none_eligible: String,
}

impl SelectionStats {
    /// One candidate ran with `wallet` (`none_eligible`: no wallet could, and why).
    pub(super) fn note(&mut self, wallet: &mut OwnerWallet, none_eligible: Option<String>) {
        self.selections = self.selections.saturating_add(1);
        if let Some(why) = none_eligible {
            self.none_eligible = self.none_eligible.saturating_add(1);
            self.last_none_eligible = why;
        }
        wallet.selected = wallet.selected.saturating_add(1);
    }
}

impl ShadowOwner {
    /// The pilot wallet: the first that is its own fee payer (`payer::owner_signers`) — the
    /// wallet of what is not a candidate's (projections, the run's identity).
    pub(super) fn pilot(&self) -> &OwnerWallet {
        &self.wallets[0]
    }

    /// The wallet's SOL available to a new attempt: its reconciled lamports less every hold of
    /// its book (open positions with their exit reserve included) and the policy reserve;
    /// `None` when its lamports are unknown or held beyond them.
    pub(super) fn wallet_free_sol(&self, i: usize) -> Option<u64> {
        let w = &self.wallets[i];
        if w.reservations.is_unreadable() {
            return None;
        }
        let balances: std::collections::BTreeMap<String, u64> = self
            .st
            .get(&w.liquidator)
            .map(|r| [("SOL".to_string(), r.lamports)].into())
            .unwrap_or_default();
        let held: std::collections::BTreeMap<String, u64> =
            [("SOL".to_string(), w.reservations.held("SOL"))].into();
        crate::execution::one_shot::free_sol(
            &balances,
            &held,
            &crate::execution::inventory::reserves(&self.inventory_targets),
        )
    }

    /// Whether wallet `i` may run a candidate now, and its free SOL; else why not. A wallet
    /// liquidates as its own fee payer, its Safety is not halted and its book readable; for LIVE
    /// also its switch is ENABLED, the grant's scope admits it, no attempt holds it (single
    /// flight per wallet) and its FE-04 approval (bound to one wallet) is VALID.
    pub(super) fn wallet_eligible(&self, i: usize, live: bool) -> Result<u64, String> {
        use crate::orchestration::capability_control as cap;
        let w = &self.wallets[i];
        if !w.runs_candidates() {
            return Err("WALLET_NOT_ITS_OWN_FEE_PAYER".into());
        }
        if let Some(halt) = w.safety.halted() {
            return Err(format!("SAFETY_HALTED:{}", halt.as_str()));
        }
        if w.reservations.is_unreadable() {
            return Err("RESERVATION_BOOK_UNREADABLE".into());
        }
        if live {
            let caps = &self.sources.capabilities;
            if !caps
                .wallets
                .iter()
                .any(|s| s.pubkey == w.liquidator && s.enabled)
            {
                return Err(format!("WALLET:{} {}", w.liquidator, cap::DISABLED));
            }
            let entry = w.scope_entry();
            if caps
                .fe06
                .as_ref()
                .and_then(|g| g.scope())
                .is_some_and(|s| !s.signers.contains(&entry))
            {
                return Err("FE06_SCOPE_WALLET_NOT_ADMITTED".into());
            }
            if let Some(state) = w.fe04_held.as_ref().and_then(|j| j.step_state()) {
                return Err(format!("{}:{}", fe04::ATTEMPT_HELD, state.name()));
            }
            let approval = self.approval_status(i);
            if approval.state != "VALID" {
                return Err(format!("FE04_APPROVAL_{}", approval.state));
            }
        }
        self.wallet_free_sol(i)
            .ok_or_else(|| "WALLET_SOL_UNKNOWN".to_string())
    }

    /// DEC-17b: the wallet a candidate runs with — among the eligible ones the most free SOL,
    /// the lowest key on a tie (deterministic, never round robin) — and, when none is eligible,
    /// why each is not: the pilot then, whose own refusal (its HALT, its book, its admission) is
    /// the candidate's. Counted with `SelectionStats::note`.
    pub(super) fn choose_wallet(&self, live: bool) -> (usize, Option<String>) {
        let mut best: Option<(usize, u64)> = None;
        let mut refusals = vec![];
        for i in 0..self.wallets.len() {
            match self.wallet_eligible(i, live) {
                Ok(free) => {
                    let better = best.is_none_or(|(b, f)| {
                        free > f
                            || (free == f
                                && self.wallets[i].liquidator.to_bytes()
                                    < self.wallets[b].liquidator.to_bytes())
                    });
                    if better {
                        best = Some((i, free));
                    }
                }
                Err(e) => refusals.push(format!("{}={e}", self.wallets[i].liquidator)),
            }
        }
        match best {
            Some((i, _)) => (i, None),
            None => (0, Some(refusals.join(", "))),
        }
    }

    /// DEC-17b: the wallet's attempt records as its journal shows them now — the held journal's
    /// (an attempt in this run), else its journals on disk; kept from the last read when they
    /// cannot be read (only this owner writes them).
    pub(super) fn refresh_records(&mut self, i: usize) {
        let w = &self.wallets[i];
        let read = match (&w.fe04_held, &w.fe04_admission, &self.fe04_journal) {
            (Some(j), _, _) => j.attempt_records().ok(),
            (None, Some(a), _) => a.journal.attempt_records().ok(),
            (None, None, Some(dir)) => {
                crate::execution::one_shot::archive::records(dir, &w.liquidator.to_string()).ok()
            }
            (None, None, None) => Some(vec![]),
        };
        if let Some(records) = read {
            self.wallets[i].records = Some(records);
        }
    }

    /// DEC-17b: the attempt records of every wallet but `i` the grant quotas count with its own
    /// (`Err`: a wallet's records were never read in this run — the quotas are unknown).
    pub(super) fn other_records(
        &self,
        i: usize,
    ) -> Result<Vec<crate::execution::accounting::AttemptRecord>, String> {
        let mut out = vec![];
        for (j, w) in self.wallets.iter().enumerate() {
            if j == i || !w.runs_candidates() {
                continue;
            }
            match &w.records {
                Some(r) => out.extend(r.iter().cloned()),
                None => {
                    return Err(format!(
                        "FE06_QUOTA_UNKNOWN: the journals of wallet {} were never read",
                        w.liquidator
                    ))
                }
            }
        }
        Ok(out)
    }
}
