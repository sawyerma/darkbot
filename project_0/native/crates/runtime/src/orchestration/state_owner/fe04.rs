//! FE-04/C3: settlement of the durable FE-04 attempt in the running owner — the wallet journal is
//! the one financial record. The settlement worker reads (finalized receipts or the strict
//! non-landing proof) off the owner; the owner applies the evidence to the held journal, values
//! every wallet delta conservatively from canonical state (a gain at the low-biased, a loss at the
//! high-biased price), books a settled attempt into Safety exactly once (the capital the startup
//! replay reserved is released once; an attempt finalized before the restart reserved none and
//! books its day only; one settled before was booked by the replay; the one-shot HALT stays) and
//! rebuilds the reservation book from the journal. UNKNOWN keeps every hold and is read again
//! after `FE04_SETTLE_RETRY`; a finalized attempt whose valuation is still unknown is settled
//! again (locally, no RPC) after the same wait.
use super::*;
use crate::execution::one_shot::settlement::{conservative_value, SettleStep};
use crate::orchestration::remote_workers::Fe04SettlementWork;

/// Wait between two settlement reads of a still UNKNOWN attempt.
pub(super) const FE04_SETTLE_RETRY: Duration = Duration::from_secs(10);

impl ShadowOwner {
    /// One owner pass: apply finished settlement reads, then schedule the next step when due.
    pub(super) fn fe04_settlement(&mut self) {
        let Some(held) = self.fe04_held.as_ref() else {
            return;
        };
        let (job, awaits_booking) = (held.settlement_job(), held.awaits_booking());
        // One read in flight at most (capacity 1).
        if let Some(done) = self.remote.fe04_settlement_results.try_pop() {
            self.fe04_settle_due = Some(std::time::Instant::now() + FE04_SETTLE_RETRY);
            self.apply_fe04_evidence(done.evidence);
            return;
        }
        if self
            .fe04_settle_due
            .is_none_or(|at| std::time::Instant::now() < at)
        {
            return;
        }
        let retry = Some(std::time::Instant::now() + FE04_SETTLE_RETRY);
        if let Some((intent, observed)) = job {
            self.fe04_settle_due = match self
                .remote
                .fe04_settlement_jobs
                .try_push(Fe04SettlementWork { intent, observed })
            {
                // In flight: the next read is scheduled when its result arrives.
                Ok(()) => None,
                Err(_) => retry,
            };
        } else if awaits_booking {
            self.fe04_settle_due = retry;
            self.apply_fe04_evidence(Default::default());
        } else {
            self.fe04_settle_due = None;
        }
    }

    fn apply_fe04_evidence(
        &mut self,
        evidence: crate::execution::one_shot::settlement::SettlementEvidence,
    ) {
        // Prices at the canonical chain clock (none: every price is unknown); the Safety day by
        // the wall clock, as every other outcome booking.
        let clock = chain_clock(&self.st).ok();
        let (price_s, slot) = clock.map_or((0, 0), |c| (c.unix_s() as i64, c.slot));
        let now_s = wall_unix_s().unwrap_or(0);
        // Booked by the startup replay already (a settled journal replays into its day).
        let settled_before = self
            .fe04_held
            .as_ref()
            .and_then(|j| j.safety_replay())
            .is_some_and(|r| r.settled.is_some());
        let step = {
            let view = StateView(&self.st);
            let sizer = darkbot_project0::sizing::ProtocolSizer {
                state: &view,
                program: self.program,
                group: self.group,
                headroom_bps: darkbot_project0::sizing::DEFAULT_HEADROOM_BPS,
            };
            let banks = |asset: &str| -> Option<Vec<Pubkey>> {
                clock.as_ref()?;
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
            };
            // Every bank of the asset must price it: the lowest proceeds, the highest cost.
            let low = |asset: &str, amount: u64| -> Option<u64> {
                banks(asset)?
                    .iter()
                    .map(|b| {
                        sizer
                            .native_amount_conservative_value_usd_micro(b, amount, price_s, slot)
                            .ok()
                    })
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .min()
            };
            let high = |asset: &str, amount: u64| -> Option<u64> {
                banks(asset)?
                    .iter()
                    .map(|b| {
                        sizer
                            .native_amount_conservative_cost_usd_micro(b, amount, price_s, slot)
                            .ok()
                    })
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .max()
            };
            let value = |asset: &str, delta: i128| conservative_value(&low, &high, asset, delta);
            match self.fe04_held.as_mut() {
                Some(journal) => journal.apply_settlement_evidence(evidence, &value),
                None => return,
            }
        };
        match step {
            Ok(SettleStep::Settled(s)) => {
                // Inventory the attempt left in the wallet: its token deltas' conservative value.
                let inventory: i128 = s
                    .valuation
                    .iter()
                    .filter(|(asset, _)| asset.as_str() != "SOL")
                    .map(|(_, v)| *v)
                    .sum();
                let booked = match self.fe04_reserved_capital.take() {
                    Some(capital) => self.safety.record_outcome(
                        now_s,
                        s.landed_success,
                        Some(s.realized_net_usd_micro),
                        capital,
                        inventory,
                        0,
                    ),
                    // Finalized before the restart: no capital was reserved, the day is booked.
                    None if !settled_before => self.safety.restore_attempt(
                        now_s,
                        None,
                        self.fe04_held
                            .as_ref()
                            .and_then(|j| j.safety_replay())
                            .and_then(|r| r.settled),
                    ),
                    None => Ok(()),
                };
                if let Some(j) = self.fe04_held.as_ref() {
                    self.reservations =
                        crate::execution::reservation::WalletReservations::rebuild(j.active_hold())
                            .unwrap_or_else(|_| {
                                crate::execution::reservation::WalletReservations::unreadable()
                            });
                }
                owner_log!(
                    self,
                    "fe04_settlement: settled landed={} landed_success={} fee_lamports={} realized_net_usd_micro={} safety={}",
                    s.landed,
                    s.landed_success,
                    s.fee_lamports,
                    s.realized_net_usd_micro,
                    booked.map_or_else(|v| v.code().to_string(), |()| "booked".into())
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
}
