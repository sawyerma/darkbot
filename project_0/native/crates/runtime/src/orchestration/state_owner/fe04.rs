//! FE-04/C3: settlement of the durable FE-04 attempt in the running owner — the wallet journal is
//! the one financial record (the RAM P8 reconcile path is telemetry only). The settlement worker
//! reads (finalized receipts or the strict non-landing proof) off the owner; the owner applies the
//! evidence to the held journal, values every wallet delta conservatively from canonical state (a
//! gain at the low-biased, a loss at the high-biased price) and books the step that settles the
//! attempt into Safety exactly once, as a restart replays it (`book_settlement`). Only a
//! successful booking rebuilds the reservation book from the journal; a refused booking (ledger
//! violation: Safety halts, applies nothing) keeps every hold and the capital to release, the
//! settlement stays in the journal for the restart replay. UNKNOWN keeps every hold and is read
//! again after `fe04_settle_retry_ms`; a finalized attempt whose valuation is still unknown is
//! settled again (locally, no RPC) after the same wait. The one-shot HALT stays. An exit of the
//! open inventory is read, applied and booked the same way (`apply_fe04_exit_evidence`): its
//! booking is the restart replay's (its day, realized net, released mark); a read applies only
//! to the exit it was made for. DEC-03: every applied read is a typed `NonLanding` record with
//! the subject's N-2 class; an open contradiction is also an alarm (holds stay, read again).
use super::*;
use crate::execution::one_shot::exit::ExitStep;
use crate::execution::one_shot::non_landing::{NonLandingRead, Verdict};
use crate::execution::one_shot::settlement::{
    conservative_value, NonLandingStatus, SettleStep, Settlement, SettlementEvidence,
};
use crate::orchestration::remote_workers::Fe04SettlementWork;
use darkbot_project0::safety::{LedgerViolation, SettledAttempt};

/// DEC-03: the latest non-landing read of this run with the exit it was made for (`None`: the
/// attempt), and how many reads of that subject this run made.
#[derive(Debug, Default)]
pub(super) struct Fe04NonLanding {
    pub(super) latest: Option<(Option<String>, NonLandingRead)>,
    pub(super) reads: u64,
}

/// What Safety has still to book for the held attempt: the capital the startup replay or the
/// in-run admission's `check()` reserved (`None`: finalized before the restart, none was
/// reserved), and the worst-case inventory that `check()` reserved (DEC-13b; 0 after a restart).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Fe04Unbooked {
    pub(super) reserved_capital: Option<u64>,
    pub(super) reserved_inventory: u64,
}

/// FE-04/C3: books a settled attempt into Safety the way a restart replays it — the realized net
/// and the outcome into the persisted booking day, the open inventory as held inventory — and
/// releases the capital (and the in-run admission's inventory hold) reserved for it, exactly once
/// (none reserved: the replay booking itself).
/// One checked transaction: a violation halts Safety and applies nothing.
pub(super) fn book_settlement(
    safety: &mut Safety,
    unbooked: Fe04Unbooked,
    s: &Settlement,
    settled: SettledAttempt,
) -> Result<(), LedgerViolation> {
    match unbooked.reserved_capital {
        Some(capital) => {
            safety.record_outcome(
                settled.unix_s,
                s.landed_success,
                Some(s.realized_net_usd_micro),
                capital,
                s.unrealized_inventory_usd_micro,
                unbooked.reserved_inventory,
            )?;
            // A consumed one-shot halts as its restart replay does (review L3).
            safety.halt(darkbot_common::error::HaltReason::LimitBreached);
            Ok(())
        }
        None => safety.restore_attempt(settled.unix_s, None, Some(settled)),
    }
}

/// The conservative USD-micro value of a signed wallet delta from canonical state at the chain
/// clock: every bank of the asset must price it (a gain at the lowest, a loss at the highest);
/// no clock, bank or price is unknown (`None`).
fn delta_value(o: &ShadowOwner, asset: &str, delta: i128) -> Option<i128> {
    conservative_value(
        &|a, n| o.asset_value_low(a, n),
        &|a, n| o.asset_value_high(a, n),
        asset,
        delta,
    )
}

impl ShadowOwner {
    /// The banks of the group that price an asset (`SOL`: the SOL bank; a mint: every bank of
    /// that mint among the owner's dependencies); `None` when none does.
    fn pricing_banks(&self, asset: &str) -> Option<Vec<Pubkey>> {
        if asset == "SOL" {
            return Some(vec![self.sol_bank]);
        }
        let mint: Pubkey = asset.parse().ok()?;
        let banks: Vec<Pubkey> = self
            .required_dependency_keys
            .iter()
            .copied()
            .filter(|k| {
                self.st.get(k).is_some_and(|r| {
                    r.owner == self.program
                        && darkbot_project0::protocol::decode_bank(&r.data)
                            .is_ok_and(|b| b.group == self.group && b.mint == mint)
                })
            })
            .collect();
        (!banks.is_empty()).then_some(banks)
    }

    /// `amount` of `asset` in USD-micro at the chain clock, over every pricing bank: `high`
    /// the highest cost, else the lowest value; unknown clock, bank or price: `None`.
    fn asset_value(&self, asset: &str, amount: u64, high: bool) -> Option<u64> {
        let clock = chain_clock(&self.st).ok()?;
        let (price_s, slot) = (clock.unix_s() as i64, clock.slot);
        let view = StateView(&self.st);
        let sizer = darkbot_project0::sizing::ProtocolSizer {
            state: &view,
            program: self.program,
            group: self.group,
            headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
        };
        let values = self
            .pricing_banks(asset)?
            .iter()
            .map(|b| {
                if high {
                    sizer.native_amount_conservative_cost_usd_micro(b, amount, price_s, slot)
                } else {
                    sizer.native_amount_conservative_value_usd_micro(b, amount, price_s, slot)
                }
                .ok()
            })
            .collect::<Option<Vec<_>>>()?;
        if high {
            values.into_iter().max()
        } else {
            values.into_iter().min()
        }
    }

    /// The lowest conservative USD-micro value of `amount` of `asset` (a gain, an output).
    pub(super) fn asset_value_low(&self, asset: &str, amount: u64) -> Option<u64> {
        self.asset_value(asset, amount, false)
    }

    /// The highest conservative USD-micro cost of `amount` of `asset` (a loss, an input).
    pub(super) fn asset_value_high(&self, asset: &str, amount: u64) -> Option<u64> {
        self.asset_value(asset, amount, true)
    }
}

impl ShadowOwner {
    /// One owner pass: apply a finished settlement read, then schedule the next step when due —
    /// the attempt first, then its open exit.
    pub(super) fn fe04_settlement(&mut self) {
        let Some(held) = self.fe04_held.as_ref() else {
            return;
        };
        let open_exit = held.open_exit_id();
        // One read in flight at most (capacity 1).
        if let Some(done) = self.remote.fe04_settlement_results.try_pop() {
            self.fe04_settle_due = Some(std::time::Instant::now() + self.fe04_settle_retry);
            let read = done.evidence.non_landing.clone();
            match done.exit {
                None => {
                    self.apply_fe04_evidence(done.evidence);
                    self.fe04_non_landing_step(None, read);
                }
                Some(id) if open_exit.as_ref() == Some(&id) => {
                    self.apply_fe04_exit_evidence(done.evidence);
                    self.fe04_non_landing_step(Some(id), read);
                }
                Some(id) => owner_log!(self, "fe04_settlement: read of exit {id} is stale"),
            }
            return;
        }
        if self
            .fe04_settle_due
            .is_none_or(|at| std::time::Instant::now() < at)
        {
            return;
        }
        let retry = Some(std::time::Instant::now() + self.fe04_settle_retry);
        let work = match (held.settlement_job(), held.exit_job()) {
            (Some((intent, observed)), _) => Some(Fe04SettlementWork {
                subject: (&intent).into(),
                observed,
                exit: None,
            }),
            (None, Some((intent, observed))) => Some(Fe04SettlementWork {
                subject: (&intent).into(),
                observed,
                exit: open_exit,
            }),
            (None, None) => None,
        };
        let (awaits_booking, exit_awaits_booking) =
            (held.awaits_booking(), held.exit_awaits_booking());
        if let Some(work) = work {
            self.fe04_settle_due = match self.remote.fe04_settlement_jobs.try_push(work) {
                // In flight: the next read is scheduled when its result arrives.
                Ok(()) => None,
                Err(_) => retry,
            };
        } else if awaits_booking {
            self.fe04_settle_due = retry;
            self.apply_fe04_evidence(Default::default());
        } else if exit_awaits_booking {
            self.fe04_settle_due = retry;
            self.apply_fe04_exit_evidence(Default::default());
        } else {
            self.fe04_settle_due = None;
        }
    }

    /// DEC-04 (OBSERVE): the typed record of a settled attempt's holds against its finalized
    /// flows (an attempt admitted in OFF has none).
    fn fe04_hold_flows(&mut self, s: &Settlement) {
        use crate::execution::hold_b::{a_flows, compare, BHold};
        let Some(held) = self.fe04_held.as_ref().and_then(|j| j.attempt_holds()) else {
            return;
        };
        let (Some(hold), a_wallet) = (held.hold, held.a_wallet) else {
            return;
        };
        let record = RecordEvent::HoldFlows {
            approval_id: held.approval_id.to_string(),
            mode: hold.mode,
            a_wallet: a_wallet.clone(),
            a_inventory_usd_micro: hold.a_inventory_usd_micro,
            b: hold.b.clone(),
            actual: s.wallet_deltas.clone(),
            actual_inventory_usd_micro: s.unrealized_inventory_usd_micro,
            a_checks: compare(&a_flows(a_wallet), &s.wallet_deltas),
            b_checks: match &hold.b {
                BHold::Exact(f) => compare(&f.flows, &s.wallet_deltas),
                BHold::Unqualified(_) => vec![],
            },
        };
        if let Err(e) = self.evidence.emit(OwnerEvidence::Record(record)) {
            owner_log!(self, "fe04_settlement: hold record not written {e}");
        }
    }

    /// DEC-03: the non-landing status of the held journal's latest subject with this run's
    /// latest read of it.
    pub(super) fn fe04_non_landing_status(&self) -> Option<NonLandingStatus> {
        let latest = self
            .fe04_non_landing
            .latest
            .as_ref()
            .map(|(exit, read)| (exit.as_deref(), read));
        self.fe04_held.as_ref()?.non_landing_status(latest)
    }

    /// DEC-03: after an applied read of `exit` (`None`: the attempt) — keeps its non-landing
    /// read, writes the typed record with the subject's class and raises the alarm of an open
    /// contradiction (holds stay; the next read follows after `fe04_settle_retry`).
    fn fe04_non_landing_step(
        &mut self,
        exit: Option<String>,
        read: Option<Result<NonLandingRead, String>>,
    ) {
        let (read, read_error) = match read {
            Some(Ok(r)) => (Some(r), None),
            Some(Err(e)) => (None, Some(e)),
            None => (None, None),
        };
        if let Some(r) = &read {
            let same = self
                .fe04_non_landing
                .latest
                .as_ref()
                .is_some_and(|(e, _)| *e == exit);
            self.fe04_non_landing.reads = if same {
                self.fe04_non_landing.reads.saturating_add(1)
            } else {
                1
            };
            self.fe04_non_landing.latest = Some((exit.clone(), r.clone()));
        }
        let class = self
            .fe04_non_landing_status()
            .filter(|s| s.exit == exit)
            .and_then(|s| s.class);
        if class == Some(Verdict::VetoContradiction) {
            owner_log!(
                self,
                "fe04_settlement: ALARM {} subject={} holds_retained retry_ms={}",
                Verdict::VetoContradiction.code(),
                exit.as_deref().unwrap_or("attempt"),
                self.fe04_settle_retry.as_millis()
            );
        }
        if let Err(e) = self
            .evidence
            .emit(OwnerEvidence::Record(RecordEvent::NonLanding {
                exit,
                class,
                read,
                read_error,
            }))
        {
            owner_log!(self, "fe04_settlement: non-landing record not written {e}");
        }
    }

    /// The reservation book from the held journal (after a successful booking only); the
    /// per-account SUBMITTED ownership follows it (MT5b).
    fn fe04_rebuild_book(&mut self) {
        if let Some(j) = self.fe04_held.as_ref() {
            self.reservations =
                crate::execution::reservation::WalletReservations::rebuild(j.active_hold())
                    .unwrap_or_else(|_| {
                        crate::execution::reservation::WalletReservations::unreadable()
                    });
            self.sync_ownership();
        }
    }

    fn apply_fe04_evidence(&mut self, evidence: SettlementEvidence) {
        // The booking second by the wall clock (unreadable: no step, the attempt is read again).
        let Ok(now_s) = wall_unix_s() else {
            owner_log!(
                self,
                "fe04_settlement: wall clock unavailable; holds retained"
            );
            return;
        };
        let Some(mut journal) = self.fe04_held.take() else {
            return;
        };
        // Settled by an earlier step: booked then (or refused then, retained for the restart).
        let settled_before = journal.safety_replay().is_some_and(|r| r.settled.is_some());
        let step = journal.apply_settlement_evidence(
            evidence,
            &|asset, delta| delta_value(self, asset, delta),
            now_s,
        );
        let settled = journal.safety_replay().and_then(|r| r.settled);
        self.fe04_held = Some(journal);
        match step {
            Ok(SettleStep::Settled(_)) if settled_before => {}
            Ok(SettleStep::Settled(s)) => {
                let (Some(unbooked), Some(settled)) = (self.fe04_unbooked, settled) else {
                    return;
                };
                let booked = book_settlement(&mut self.safety, unbooked, &s, settled);
                if booked.is_ok() {
                    self.fe04_unbooked = None;
                    self.fe04_rebuild_book();
                }
                // DEC-04 (OBSERVE): A, B and the finalized flows of the attempt, once.
                self.fe04_hold_flows(&s);
                // C02/F-04: the settlement is the one landing-telemetry path (exactly once: a
                // repeated or restarted settlement takes the `settled_before` branch).
                let released = if booked.is_ok() {
                    unbooked.reserved_capital.unwrap_or(0)
                } else {
                    0
                };
                if let Some(record) = self
                    .fe04_held
                    .as_ref()
                    .and_then(|j| j.landing_record(released))
                {
                    if let Err(e) = self.evidence.emit(OwnerEvidence::Reconcile(record)) {
                        owner_log!(self, "fe04_settlement: landing telemetry not written {e}");
                    }
                }
                owner_log!(
                    self,
                    "fe04_settlement: settled landed={} landed_success={} fee_lamports={} realized_net_usd_micro={} unrealized_inventory_usd_micro={} booked_unix_s={} safety={}",
                    s.landed,
                    s.landed_success,
                    s.fee_lamports,
                    s.realized_net_usd_micro,
                    s.unrealized_inventory_usd_micro,
                    settled.unix_s,
                    booked.map_or_else(
                        |v| format!("{}; holds retained, replayed at restart", v.code()),
                        |()| "booked".into()
                    )
                );
            }
            Ok(SettleStep::Unknown(reasons)) => owner_log!(
                self,
                "fe04_settlement: UNKNOWN holds_retained reasons={}",
                reasons.join(",")
            ),
            Err(e) => owner_log!(self, "fe04_settlement: refused {e}; holds retained"),
        }
    }

    /// FE-04 exit: applies the read to the open exit; the step that settles it books it as the
    /// restart replay does (its day, realized net, the released mark) and, only when booked,
    /// rebuilds the book. A refused booking keeps the book; the restart replays the exit.
    fn apply_fe04_exit_evidence(&mut self, evidence: SettlementEvidence) {
        let Ok(now_s) = wall_unix_s() else {
            owner_log!(self, "fe04_exit: wall clock unavailable; holds retained");
            return;
        };
        let Some(mut journal) = self.fe04_held.take() else {
            return;
        };
        let booked_before = journal.safety_replay().map_or(0, |r| r.exits.len());
        let step = journal.apply_exit_evidence(
            evidence,
            &|asset, delta| delta_value(self, asset, delta),
            now_s,
        );
        let booking = journal
            .safety_replay()
            .filter(|r| r.exits.len() > booked_before)
            .and_then(|r| r.exits.last().copied());
        self.fe04_held = Some(journal);
        match (step, booking) {
            (Ok(ExitStep::Settled(x)), Some(booking)) => {
                let booked = self
                    .safety
                    .restore_attempt(booking.unix_s, None, Some(booking));
                if booked.is_ok() {
                    self.fe04_rebuild_book();
                }
                owner_log!(
                    self,
                    "fe04_exit: settled landed={} landed_success={} sold_units={} realized_usd_micro={} released_mark_usd_micro={} booked_unix_s={} safety={}",
                    x.landed,
                    x.landed_success,
                    x.sold_units,
                    x.realized_usd_micro,
                    x.released_mark_usd_micro,
                    x.booked_unix_s,
                    booked.map_or_else(
                        |v| format!("{}; holds retained, replayed at restart", v.code()),
                        |()| "booked".into()
                    )
                );
            }
            (Ok(ExitStep::Settled(_)), None) => {}
            (Ok(ExitStep::Unknown(reasons)), _) => owner_log!(
                self,
                "fe04_exit: UNKNOWN holds_retained reasons={}",
                reasons.join(",")
            ),
            (Err(e), _) => owner_log!(self, "fe04_exit: refused {e}; holds retained"),
        }
    }
}

/// DEC-13b/DEC-02: the owner's FE-04 admissions of this run — attempts and exits handed off,
/// refused and UNKNOWN with the last refusal, the pipeline simulations at the handoff, and why
/// no admission is open now (the Liquidator tab).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Fe04AdmissionStats {
    pub(super) submitted: u64,
    pub(super) refused: u64,
    pub(super) unknown: u64,
    pub(super) last_refusal: String,
    pub(super) simulation: crate::execution::one_shot::admission::SimulationStats,
    pub(super) exits_submitted: u64,
    pub(super) exits_refused: u64,
    pub(super) exits_unknown: u64,
    pub(super) last_exit_refusal: String,
    /// Why no admission is open for LIVE candidates ("" while one is).
    pub(super) block_reason: String,
}

/// DEC-13b: the approval, journal and current wallet facts of one admission.
pub(super) struct Fe04AdmissionFacts {
    approval: crate::execution::one_shot::Approval,
    dir: std::path::PathBuf,
    wallet: String,
    state_generation: u64,
    program_data_hash: String,
    balances: std::collections::BTreeMap<String, u64>,
    reserved_elsewhere: std::collections::BTreeMap<String, u64>,
    policy_reserve: std::collections::BTreeMap<String, u64>,
    valid_until_slot: u64,
}

/// DEC-13b: the admission a LIVE candidate is handed to — the open one of the same approval
/// refreshed with `facts`, else a new one on the wallet's journal (a replaced approval's journal
/// is closed first; a held journal without an attempt is that journal) — or `None` with its
/// reason; the pipeline then stops at its send boundary (`LIVE_SEND_REQUIRES_FE04`). A journal
/// holding an attempt is never admitted over: it is closed again (review M3). The admission
/// comes back through `fe04_admission_back`.
pub(super) fn admission_for(
    open: &mut Option<crate::execution::one_shot::admission::Fe04Admission>,
    held: &mut Option<crate::execution::one_shot::Journal>,
    stats: &mut Fe04AdmissionStats,
    facts: Result<Fe04AdmissionFacts, (String, bool)>,
) -> Option<crate::execution::one_shot::admission::Fe04Admission> {
    let f = match facts {
        Ok(f) => f,
        Err((reason, close)) => {
            if close {
                *open = None;
            }
            stats.block_reason = reason;
            return None;
        }
    };
    let mut admission = match open.take() {
        Some(a) if a.approval == f.approval => a,
        other => {
            drop(other);
            let journal = match held.take_if(|j| j.step_state().is_none()) {
                Some(j) => Ok(j),
                None => crate::execution::one_shot::Journal::open(&f.dir, &f.wallet),
            };
            let journal = match journal {
                Ok(j) => j,
                Err(e) => {
                    stats.block_reason = e;
                    return None;
                }
            };
            if let Some(state) = journal.step_state() {
                stats.block_reason = format!("FE04_JOURNAL_HAS_ATTEMPT:{}", state.name());
                return None;
            }
            crate::execution::one_shot::admission::Fe04Admission {
                journal,
                approval: f.approval,
                state_generation: 0,
                program_data_hash: String::new(),
                balances: Default::default(),
                reserved_elsewhere: Default::default(),
                policy_reserve: Default::default(),
                valid_until_slot: 0,
                simulation: Default::default(),
                reserved: None,
            }
        }
    };
    admission.state_generation = f.state_generation;
    admission.program_data_hash = f.program_data_hash;
    admission.balances = f.balances;
    admission.reserved_elsewhere = f.reserved_elsewhere;
    admission.policy_reserve = f.policy_reserve;
    admission.valid_until_slot = f.valid_until_slot;
    stats.block_reason.clear();
    Some(admission)
}

impl ShadowOwner {
    /// DEC-13: LIVE is effective (the controller's request with an FE-06 authorization).
    pub(super) fn live_effective(&self) -> bool {
        crate::orchestration::capability_control::pipeline_mode(
            &self.sources.capabilities.live_gates(),
        ) == crate::execution::pipeline::Mode::Live
    }

    /// DEC-13b: what the FE-04 admission of a LIVE candidate for `target` is opened or refreshed
    /// with — the configured attempt approval (VALID: unexpired, bound, unspent) and the current
    /// wallet facts — or why there is none (`true`: an open admission is closed too, its wallet
    /// lock released). Single flight: none while an attempt holds the wallet.
    pub(super) fn fe04_admission_facts(
        &self,
        target: &Pubkey,
    ) -> Result<Fe04AdmissionFacts, (String, bool)> {
        use crate::execution::one_shot::approval_file::ApprovalFile;
        // Single flight: an attempt in the held journal blocks every admission; a journal without
        // one (an unused admission of an earlier run, an archived attempt) does not (review H1).
        if let Some(state) = self.fe04_held.as_ref().and_then(|j| j.step_state()) {
            return Err((format!("FE04_ATTEMPT_HELD:{}", state.name()), true));
        }
        let Some(dir) = self.fe04_journal.clone() else {
            return Err(("FE04_JOURNAL_NOT_CONFIGURED".into(), true));
        };
        let status = self.approval_status();
        if status.state != "VALID" {
            return Err((
                if status.reason.is_empty() {
                    format!("FE04_APPROVAL_{}", status.state)
                } else {
                    format!("FE04_APPROVAL_{}:{}", status.state, status.reason)
                },
                true,
            ));
        }
        let Some(ApprovalSource {
            read: Ok(ApprovalFile::Attempt(approval)),
            ..
        }) = &self.fe04_approval
        else {
            return Err(("FE04_APPROVAL_NOT_AN_ATTEMPT".into(), true));
        };
        if !approval.allowed_targets.contains(&target.to_string()) {
            return Err(("FE04_TARGET_NOT_APPROVED".into(), false));
        }
        if self.program_upgrade_latched {
            return Err(("FE04_PROGRAM_UPGRADED".into(), true));
        }
        // The blockhash the pipeline signs with is this one or a newer one: its validity bounds
        // the attempt conservatively.
        let Some(valid_until_slot) = self.blockhash.get().and_then(|b| b.valid_until_slot()) else {
            return Err(("FE04_BLOCKHASH_VALIDITY_UNKNOWN".into(), false));
        };
        Ok(Fe04AdmissionFacts {
            dir,
            wallet: self.liquidator.to_string(),
            state_generation: self.st.recovery_generation(),
            program_data_hash: self.program_data_baseline_hash.to_string(),
            balances: self.fe04_wallet_balances(target),
            reserved_elsewhere: self.reservations.held_by_others(&approval.approval_id),
            policy_reserve: crate::execution::inventory::reserves(&self.inventory_targets),
            valid_until_slot,
            approval: approval.clone(),
        })
    }

    /// DEC-13b (review M4/L5): an UNKNOWN handoff is read back from the disk the way a restart
    /// reads it — a reservation never handed off becomes UNKNOWN (the non-landing proof settles
    /// it), a failed write shows what reached the disk. A journal that cannot be read again leaves
    /// the book unreadable (every hold kept, no offer).
    pub(super) fn fe04_reopen_held(&mut self) {
        let (Some(journal), Some(dir)) = (self.fe04_held.take(), self.fe04_journal.clone()) else {
            return;
        };
        drop(journal);
        match crate::execution::one_shot::Journal::open(&dir, &self.liquidator.to_string()) {
            Ok(j) => self.fe04_held = Some(j),
            Err(e) => {
                self.reservations = crate::execution::reservation::WalletReservations::unreadable();
                owner_log!(
                    self,
                    "fe04_admission: journal not read again {e}; holds retained"
                );
            }
        }
    }

    /// DEC-13b: the reconciled wallet balances an admission for `target` checks its holds
    /// against: the liquidator's lamports (`SOL`) and the validated ATA amount of each mint of the
    /// target's active banks and of the JIT funding bank. A balance not in canonical state is
    /// absent (unknown): a hold of it is refused.
    pub(super) fn fe04_wallet_balances(
        &self,
        target: &Pubkey,
    ) -> std::collections::BTreeMap<String, u64> {
        let view = StateView(&self.st);
        let mut balances = std::collections::BTreeMap::new();
        if let Some(r) = self.st.get(&self.liquidator) {
            balances.insert("SOL".to_string(), r.lamports);
        }
        let banks = self
            .st
            .get(target)
            .and_then(|r| accounts::active_banks_in_group_account(&r.data, &self.group))
            .unwrap_or_default();
        for bank in banks.iter().chain(self.jit_funding_bank.iter()) {
            let Some(b) = accounts::bank_from_state(&view, &self.program, bank) else {
                continue;
            };
            if let Some((_, amount)) =
                accounts::validated_liquidator_ata(&view, &b.mint, &self.atas)
            {
                balances.insert(b.mint.to_string(), amount);
            }
        }
        balances
    }

    /// DEC-13b: the admission after its candidate. A handed-off attempt (submitted, or UNKNOWN
    /// after its durable reservation) becomes the held attempt: its journal is the owner's, the
    /// capital and inventory `check()` reserved wait for the journal settlement's booking (never
    /// a second release), the book and the SUBMITTED ownership follow the journal and the first
    /// settlement read is due at once. Anything else keeps the admission open for the next
    /// candidate (nothing durable exists).
    pub(super) fn fe04_admission_back(
        &mut self,
        mut admission: crate::execution::one_shot::admission::Fe04Admission,
        rec: &crate::analytics::record::OpportunityRecord,
    ) {
        let s = &mut self.fe04_admissions.simulation;
        let a = std::mem::take(&mut admission.simulation);
        s.reused = s.reused.saturating_add(a.reused);
        s.resimulated = s.resimulated.saturating_add(a.resimulated);
        s.refused = s.refused.saturating_add(a.refused);
        if a.last_reuse_refusal.is_some() {
            s.last_reuse_refusal = a.last_reuse_refusal;
        }
        let Some(reserved) = admission.reserved.take() else {
            if let Some(crate::analytics::rejection::Rejection::SubmissionRefused {
                reason, ..
            }) = &rec.reason
            {
                self.fe04_admissions.refused = self.fe04_admissions.refused.saturating_add(1);
                self.fe04_admissions.last_refusal = reason.clone();
            }
            self.fe04_admission = Some(admission);
            return;
        };
        let submitted = matches!(rec.decision, Decision::Submitted);
        if submitted {
            self.fe04_admissions.submitted = self.fe04_admissions.submitted.saturating_add(1);
        } else {
            self.fe04_admissions.unknown = self.fe04_admissions.unknown.saturating_add(1);
        }
        self.fe04_held = Some(admission.journal);
        if !submitted {
            self.fe04_reopen_held();
        }
        self.fe04_unbooked = Some(Fe04Unbooked {
            reserved_capital: Some(reserved.capital),
            reserved_inventory: reserved.inventory,
        });
        self.fe04_settle_due = Some(std::time::Instant::now());
        self.fe04_rebuild_book();
        owner_log!(
            self,
            "fe04_admission: attempt {} handed off decision={:?} capital_usd_micro={} inventory_usd_micro={}",
            rec.signature.as_deref().unwrap_or("UNKNOWN"),
            rec.decision,
            reserved.capital,
            reserved.inventory
        );
    }
}

impl ShadowOwner {
    /// DEC-13b: the exit of the held attempt's open inventory while LIVE is effective — with the
    /// configured exit approval (VALID: unexpired, bound, unspent, an open position of its
    /// attempt), a fresh blockhash and the route quoted now within the candidate network
    /// deadline; admitted once (`admit_exit`: reserve, UNKNOWN handoff, one submit) and then
    /// read and booked by the settlement like every exit. An UNKNOWN exit halts Safety and keeps
    /// every hold. Called at the FE-04 refresh cadence.
    pub(super) fn fe04_exit_admission(&mut self) {
        use crate::execution::one_shot::admission::Admission;
        use crate::execution::one_shot::approval_file::ApprovalFile;
        use crate::execution::one_shot::exit_admission::{admit_exit, ExitCandidate};
        // An UNKNOWN exit ends the exits of this run (review L4): its holds stay, it is settled.
        if !self.live_effective() || self.fe04_admissions.exits_unknown > 0 {
            return;
        }
        let Some(held) = self.fe04_held.as_ref() else {
            return;
        };
        if held.open_exit_id().is_some() {
            return;
        }
        let open = held.open_inventory();
        let Some(ApprovalSource {
            read: Ok(ApprovalFile::Exit(approval)),
            ..
        }) = &self.fe04_approval
        else {
            return;
        };
        let Some(units) = open
            .get(&approval.mint)
            .and_then(|p| u64::try_from(p.units).ok())
            .filter(|u| *u > 0)
        else {
            return;
        };
        let approval = approval.clone();
        let refuse = |o: &mut Self, reason: String| {
            o.fe04_admissions.last_exit_refusal = reason;
        };
        let status = self.approval_status();
        if status.state != "VALID" {
            return refuse(
                self,
                format!("EXIT_APPROVAL_{}:{}", status.state, status.reason),
            );
        }
        let (Ok(mint), Ok(wsol)) = (
            approval.mint.parse::<Pubkey>(),
            crate::execution::route::WSOL_MINT.parse::<Pubkey>(),
        ) else {
            return refuse(self, "EXIT_MINT_INVALID".into());
        };
        let now_ns = self.clock.now_ns();
        let Some(bh) = self.blockhash.fresh(now_ns, self.blockhash_max_age_ns) else {
            return refuse(self, "EXIT_BLOCKHASH_STALE".into());
        };
        let Some(deadline) =
            std::time::Instant::now().checked_add(Duration::from_nanos(self.network.deadline_ns))
        else {
            return refuse(self, "EXIT_NETWORK_DEADLINE_UNBOUNDED".into());
        };
        let route = match self.routes.exit_to_sol(&mint, units, deadline) {
            Ok(r) => r,
            Err(e) => {
                self.fe04_admissions.exits_refused =
                    self.fe04_admissions.exits_refused.saturating_add(1);
                return refuse(self, e);
            }
        };
        let view = StateView(&self.st);
        let mut balances = std::collections::BTreeMap::new();
        if let Some(r) = self.st.get(&self.liquidator) {
            balances.insert("SOL".to_string(), r.lamports);
        }
        for m in [mint, wsol] {
            if let Some((_, amount)) = accounts::validated_liquidator_ata(&view, &m, &self.atas) {
                balances.insert(m.to_string(), amount);
            }
        }
        let candidate = ExitCandidate {
            route: &route,
            payer: &self.payer,
            blockhash: bh.hash,
            last_valid_block_height: bh.last_valid_block_height,
            resources: self.resources,
            limits: darkbot_solana::tx::TxLimits::default(),
            slot: self.slot_watch.get(),
            now_unix_ms: (self.clock.unix_ns(now_ns) / 1_000_000) as u64,
            state_valid: self.st.run_state().is_tradable() && !self.program_upgrade_latched,
            balances,
            reserved_elsewhere: self.reservations.held_by_others(&approval.approval_id),
            policy_reserve: crate::execution::inventory::reserves(&self.inventory_targets),
            deadline,
            t_handoff_ns: self.clock.now_ns(),
        };
        let Some(mut journal) = self.fe04_held.take() else {
            return;
        };
        let admitted = admit_exit(&mut journal, approval, candidate, &self.submitter);
        self.fe04_held = Some(journal);
        let s = &mut self.fe04_admissions;
        match admitted {
            Admission::Submitted(ack) => {
                s.exits_submitted = s.exits_submitted.saturating_add(1);
                owner_log!(self, "fe04_exit: handed off signature={}", ack.signature);
            }
            Admission::Refused(reason) => {
                s.exits_refused = s.exits_refused.saturating_add(1);
                s.last_exit_refusal = reason;
                return;
            }
            Admission::Unknown(reason) => {
                s.exits_unknown = s.exits_unknown.saturating_add(1);
                s.last_exit_refusal = reason;
                self.safety
                    .halt(darkbot_common::error::HaltReason::UnknownEconomics);
                self.fe04_reopen_held();
            }
        }
        self.fe04_settle_due = Some(std::time::Instant::now());
        self.fe04_rebuild_book();
    }
}
