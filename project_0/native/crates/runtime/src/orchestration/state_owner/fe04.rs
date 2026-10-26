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

/// What Safety has still to book for the held attempt: the capital the startup replay reserved
/// (`None`: finalized before the restart, none was reserved).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Fe04Unbooked {
    pub(super) reserved_capital: Option<u64>,
}

/// FE-04/C3: books a settled attempt into Safety the way a restart replays it — the realized net
/// and the outcome into the persisted booking day, the open inventory as held inventory — and
/// releases the capital the startup replay reserved (none reserved: the replay booking itself).
/// One checked transaction: a violation halts Safety and applies nothing.
pub(super) fn book_settlement(
    safety: &mut Safety,
    unbooked: Fe04Unbooked,
    s: &Settlement,
    settled: SettledAttempt,
) -> Result<(), LedgerViolation> {
    match unbooked.reserved_capital {
        Some(capital) => safety.record_outcome(
            settled.unix_s,
            s.landed_success,
            Some(s.realized_net_usd_micro),
            capital,
            s.unrealized_inventory_usd_micro,
            0,
        ),
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
