//! DEC-17b: the owner's wallets — one per configured signer, each with its own reservation book,
//! Safety (limits, day, HALT), FE-04 journal lane (`<wallet>.p0j` in the one journal directory),
//! admission, approval and ATAs — and the deterministic choice of the wallet a candidate runs
//! with: among the wallets that may run it, the most SOL available after every hold (exit
//! reserves included) and the policy reserve, the lowest key on a tie; never round robin. An
//! error, UNKNOWN or HALT of a wallet blocks that wallet only.
//!
//! DEC-17b B4: a liquidator without the FEE_PAYER role runs with a fee payer — a wallet with that
//! role alone, chosen the same way (the most free SOL, the lowest key on a tie): its SOL pays the
//! attempt's fee, tip, record rent and exit reserve and is held on it (`WalletReservations::
//! held_for`), the liquidator's key signs beside it. A wallet with both roles pays only itself.
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

    /// A wallet that liquidates — with its own fees, or (DEC-17b B4) paid by a fee payer.
    pub(super) fn runs_candidates(&self) -> bool {
        self.roles
            .contains(&darkbot_solana::runtime_config::roles::LIQUIDATOR)
    }

    /// DEC-17b B4: it liquidates and pays its own fees.
    pub(super) fn pays_itself(&self) -> bool {
        self.roles == darkbot_solana::runtime_config::roles::SIGNING
    }

    /// DEC-17b B4: a fee payer alone — it pays the liquidators without their own.
    pub(super) fn pays_others(&self) -> bool {
        self.roles == [darkbot_solana::runtime_config::roles::FEE_PAYER]
    }

    /// The wallet as a grant's scope names it (`<pubkey>:<ROLE>+<ROLE>`).
    pub(super) fn scope_entry(&self) -> String {
        format!("{}:{}", self.liquidator, self.roles.join("+"))
    }
}

/// DEC-17b B4: why a candidate whose pilot has no own fees is admitted nothing.
pub(super) const NONE_ELIGIBLE: &str = "WALLET_NONE_ELIGIBLE";

/// DEC-17b B4: wallet `w` to lend to a candidate's evaluation and, when another pays it, that fee
/// payer (read only; never `w` itself).
pub(super) fn with_payer(
    wallets: &mut [OwnerWallet],
    w: usize,
    payer: Option<usize>,
) -> (&mut OwnerWallet, Option<&OwnerWallet>) {
    match payer.filter(|p| *p != w) {
        None => (&mut wallets[w], None),
        Some(p) if p < w => {
            let (low, high) = wallets.split_at_mut(w);
            (&mut high[0], Some(&low[p]))
        }
        Some(p) => {
            let (low, high) = wallets.split_at_mut(p);
            (&mut low[w], Some(&high[0]))
        }
    }
}

/// DEC-17b: the wallet a candidate runs with, the wallet paying it (B4; `None`: itself) and,
/// when no wallet may run it, why each may not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Choice {
    pub(super) wallet: usize,
    pub(super) payer: Option<usize>,
    pub(super) none_eligible: Option<String>,
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

    /// The SOL held on wallet `i`: its book's own SOL holds (open positions with their exit
    /// reserve included) and (DEC-17b B4) what the attempts of the liquidators it pays hold on
    /// it — `None` while its book, or the book of a liquidator it may pay, is unreadable.
    pub(super) fn sol_held(&self, i: usize) -> Option<u64> {
        self.sol_reserved(i, i, &[])
    }

    /// The SOL held on wallet `payer` (`sol_held`) but the holds of the attempts `except` in
    /// `lane`'s book (an admission's own approval, an exit's own attempt) — the SOL its next
    /// attempt or exit is checked against, whichever wallet's lane that is (review M2: a wallet
    /// paying itself counts what other books hold on it too).
    pub(super) fn sol_reserved(&self, payer: usize, lane: usize, except: &[&str]) -> Option<u64> {
        let w = &self.wallets[payer];
        if w.reservations.is_unreadable() {
            return None;
        }
        let own: &[&str] = if lane == payer { except } else { &[] };
        let mut held = w
            .reservations
            .held_by_others(own)
            .get("SOL")
            .copied()
            .unwrap_or(0);
        let key = w.liquidator.to_string();
        for (j, o) in self.wallets.iter().enumerate().filter(|(j, _)| *j != payer) {
            if o.reservations.is_unreadable() {
                if w.pays_others() && o.runs_candidates() && !o.pays_itself() {
                    return None;
                }
                continue;
            }
            let ids: &[&str] = if j == lane { except } else { &[] };
            held = held.saturating_add(o.reservations.held_for(&key, ids));
        }
        Some(held)
    }

    /// DEC-17b B4: the wallet with key `key` (a held attempt's fee payer).
    pub(super) fn wallet_of(&self, key: &str) -> Option<usize> {
        self.wallets
            .iter()
            .position(|w| w.liquidator.to_string() == key)
    }

    /// The wallet's SOL available to a new attempt: its reconciled lamports less every SOL hold
    /// on it (`sol_held`) and the policy reserve; `None` when its lamports or holds are unknown
    /// or held beyond them.
    pub(super) fn wallet_free_sol(&self, i: usize) -> Option<u64> {
        let w = &self.wallets[i];
        let held = self.sol_held(i)?;
        let balances: std::collections::BTreeMap<String, u64> = self
            .st
            .get(&w.liquidator)
            .map(|r| [("SOL".to_string(), r.lamports)].into())
            .unwrap_or_default();
        let held: std::collections::BTreeMap<String, u64> = [("SOL".to_string(), held)].into();
        crate::execution::one_shot::free_sol(
            &balances,
            &held,
            &crate::execution::inventory::reserves(&self.inventory_targets),
        )
    }

    /// The switch, scope and halt gates every signer of a candidate passes: its Safety is not
    /// halted and its book readable; for LIVE also its switch is ENABLED and the grant's scope
    /// admits it.
    fn signer_gates(&self, i: usize, live: bool) -> Result<(), String> {
        use crate::orchestration::capability_control as cap;
        let w = &self.wallets[i];
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
        }
        Ok(())
    }

    /// DEC-17b B4: for LIVE, the grant's scope names wallet `i` as its one fee payer (none: no
    /// scope binds it).
    fn scope_fee_payer(&self, i: usize, live: bool) -> Result<(), String> {
        let named = |s: &crate::orchestration::fe06::Scope| {
            s.fee_payer == self.wallets[i].liquidator.to_string()
        };
        if live
            && self
                .sources
                .capabilities
                .fe06
                .as_ref()
                .and_then(|g| g.scope())
                .is_some_and(|s| !named(s))
        {
            return Err("FE06_SCOPE_FEE_PAYER_NOT_ADMITTED".into());
        }
        Ok(())
    }

    /// DEC-17b B4: whether wallet `p` may pay a candidate of a liquidator without its own fees
    /// now, and its free SOL; else why not. A fee payer alone, through the signer gates; for LIVE
    /// also the grant's scope names it as the fee payer.
    pub(super) fn payer_eligible(&self, p: usize, live: bool) -> Result<u64, String> {
        let w = &self.wallets[p];
        if !w.pays_others() {
            return Err("WALLET_NOT_A_FEE_PAYER".into());
        }
        self.signer_gates(p, live)?;
        self.scope_fee_payer(p, live)?;
        self.wallet_free_sol(p)
            .ok_or_else(|| "WALLET_SOL_UNKNOWN".to_string())
    }

    /// DEC-17b B4: the fee payer the liquidators without their own run with now — among the
    /// eligible ones the most free SOL, the lowest key on a tie — and its free SOL; else why none.
    pub(super) fn choose_payer(&self, live: bool) -> Result<(usize, u64), String> {
        let mut best: Option<(usize, u64)> = None;
        let mut refusals = vec![];
        for p in (0..self.wallets.len()).filter(|p| self.wallets[*p].pays_others()) {
            match self.payer_eligible(p, live) {
                Ok(free) => {
                    if best.is_none_or(|(b, f)| self.better(p, free, b, f)) {
                        best = Some((p, free));
                    }
                }
                Err(e) => refusals.push(format!("{}={e}", self.wallets[p].liquidator)),
            }
        }
        best.ok_or_else(|| {
            if refusals.is_empty() {
                "NO_FEE_PAYER_CONFIGURED".into()
            } else {
                format!("NO_FEE_PAYER_ELIGIBLE: {}", refusals.join(", "))
            }
        })
    }

    /// The deterministic order of the choice: more free SOL, then the lower key.
    fn better(&self, i: usize, free: u64, than: usize, than_free: u64) -> bool {
        free > than_free
            || (free == than_free
                && self.wallets[i].liquidator.to_bytes() < self.wallets[than].liquidator.to_bytes())
    }

    /// Whether wallet `i` may run a candidate now, the SOL free for it and the wallet paying it
    /// (`None`: itself); else why not. A wallet liquidates — paying its own fees, or (DEC-17b
    /// B4) paid by `payer`, the fee payer of this candidate — through the signer gates; for LIVE
    /// also no attempt holds it (single flight per wallet) and its FE-04 approval (bound to one
    /// wallet) is VALID.
    pub(super) fn wallet_eligible(
        &self,
        i: usize,
        live: bool,
        payer: &Result<(usize, u64), String>,
    ) -> Result<(u64, Option<usize>), String> {
        let w = &self.wallets[i];
        if !w.runs_candidates() {
            return Err("WALLET_NOT_A_LIQUIDATOR".into());
        }
        self.signer_gates(i, live)?;
        if live {
            if let Some(state) = w.fe04_held.as_ref().and_then(|j| j.step_state()) {
                return Err(format!("{}:{}", fe04::ATTEMPT_HELD, state.name()));
            }
            let approval = self.approval_status(i);
            if approval.state != "VALID" {
                return Err(format!("FE04_APPROVAL_{}", approval.state));
            }
        }
        if w.pays_itself() {
            // B4 review M1: a wallet paying itself is the scope's one fee payer, or runs nothing.
            self.scope_fee_payer(i, live)?;
            return self
                .wallet_free_sol(i)
                .map(|free| (free, None))
                .ok_or_else(|| "WALLET_SOL_UNKNOWN".to_string());
        }
        payer
            .as_ref()
            .map(|(p, free)| (*free, Some(*p)))
            .map_err(Clone::clone)
    }

    /// DEC-17b: the wallet a candidate runs with — among the eligible ones the most free SOL
    /// (B4: a liquidator without its own fees counts its fee payer's), the lowest key on a tie
    /// (deterministic, never round robin) — with the wallet paying it (`None`: itself) and, when
    /// none is eligible, why each is not: the pilot then, whose own refusal (its HALT, its book,
    /// its admission) is the candidate's, with the payer it would run with. Counted with
    /// `SelectionStats::note`.
    pub(super) fn choose_wallet(&self, live: bool) -> Choice {
        let payer = self.choose_payer(live);
        let mut best: Option<(usize, u64, Option<usize>)> = None;
        let mut refusals = vec![];
        for i in 0..self.wallets.len() {
            if self.wallets[i].pays_others() {
                continue;
            }
            match self.wallet_eligible(i, live, &payer) {
                Ok((free, by)) => {
                    if best.is_none_or(|(b, f, _)| self.better(i, free, b, f)) {
                        best = Some((i, free, by));
                    }
                }
                Err(e) => refusals.push(format!("{}={e}", self.wallets[i].liquidator)),
            }
        }
        match best {
            Some((wallet, _, payer)) => Choice {
                wallet,
                payer,
                none_eligible: None,
            },
            None => Choice {
                wallet: 0,
                // The pilot without its own fees runs with the lowest-keyed fee payer (its
                // evaluation is the candidate's; nothing of it is admitted: `none_eligible`).
                payer: (!self.pilot().pays_itself())
                    .then(|| {
                        (0..self.wallets.len())
                            .filter(|p| self.wallets[*p].pays_others())
                            .min_by_key(|p| self.wallets[*p].liquidator.to_bytes())
                    })
                    .flatten(),
                none_eligible: Some(refusals.join(", ")),
            },
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
