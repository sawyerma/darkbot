//! B01 (P8-INV-01, CTRL-03): inventory rebalancing in OBSERVE. Every `rebalance_observe_ms` the
//! owner plans the minimal rebalancing of its bands (`P0_INVENTORY_TARGETS`) from the wallet's
//! book-adjusted balances in canonical state (validated ATAs, the liquidator's lamports, minus
//! every FE-04 hold), real ExactIn quotes and conservative costs (selling at the high, receiving
//! at the low price, plus the transaction's planned network fee), inside
//! `rebalance_budget_usd_micro`. Quotes are asked off the owner (the rebalance quote worker) in at
//! most `rebalance_quote_rounds` rounds; a quote not answered is never guessed. Nothing is ever
//! traded: the mode is the controller's capability `INVENTORY_REBALANCING` (start value
//! `P0_REBALANCE_MODE`), ACTIVE is recorded as requested and never effective. The plan reaches
//! the Liquidator tab with the status projection and each completed observation the journal.
use super::*;
use crate::execution::inventory::{self, Action, QuoteKey, RebalanceMode};
use crate::status_tap::{OwnerRebalance, RebalanceTrade};
use std::collections::{BTreeMap, BTreeSet};

/// The owner's rebalancing observation.
pub(super) struct Observer {
    /// The operator's request (`INVENTORY_REBALANCING`): OFF, OBSERVE or ACTIVE.
    pub(super) requested: &'static str,
    /// What is effective: ACTIVE is never (`inventory::ACTIVE_UNAUTHORIZED`).
    pub(super) mode: RebalanceMode,
    /// When the next observation starts (`None`: at once).
    due: Option<std::time::Instant>,
    /// Quote rounds of the running observation (0: none running).
    round: u64,
    /// The running observation's answered quotes: the minimum output, `None` for no route.
    quotes: BTreeMap<QuoteKey, Option<u64>>,
    /// Asked and not answered yet.
    pending: BTreeSet<QuoteKey>,
    /// The running observation (`RebalanceQueues::generation`): new on every mode change and
    /// every observation start; an answer of another one is dropped.
    generation: u64,
    /// The newest plan.
    pub(super) plan: Option<OwnerRebalance>,
    /// `rebalance_budget_usd_micro`, `rebalance_quote_rounds`, `rebalance_observe_ms`.
    budget_usd_micro: u64,
    rounds: u64,
    interval: std::time::Duration,
}

impl Observer {
    pub(super) fn new(
        mode: RebalanceMode,
        limits: &darkbot_solana::runtime_config::Limits,
    ) -> Self {
        Self {
            requested: mode.name(),
            mode,
            due: None,
            round: 0,
            quotes: BTreeMap::new(),
            pending: BTreeSet::new(),
            generation: 0,
            plan: None,
            budget_usd_micro: limits.rebalance_budget_usd_micro,
            rounds: limits.rebalance_quote_rounds,
            interval: std::time::Duration::from_millis(limits.rebalance_observe_ms),
        }
    }
}

impl Observer {
    /// B01 (CTRL-03): the operator's request — OFF stops the observation (its plan with it);
    /// OBSERVE and ACTIVE observe, ACTIVE never trades (`block`); without bands nothing can be
    /// observed (refused, the request unchanged). A change starts afresh: no quote of before
    /// (`true`: the mode changed).
    pub(super) fn request(&mut self, requested: &str, banded: bool) -> Result<bool, &'static str> {
        let value = inventory::REBALANCE_VALUES
            .into_iter()
            .find(|v| *v == requested)
            .ok_or("CAPABILITY_VALUE_INVALID: INVENTORY_REBALANCING (OFF|OBSERVE|ACTIVE)")?;
        let mode = if value == "OFF" {
            RebalanceMode::Off
        } else if banded {
            RebalanceMode::Observe
        } else {
            return Err(inventory::TARGETS_REQUIRED);
        };
        let changed = mode != self.mode;
        if changed {
            self.mode = mode;
            self.due = None;
            self.round = 0;
            self.quotes.clear();
            self.pending.clear();
            if mode == RebalanceMode::Off {
                self.plan = None;
            }
        }
        self.requested = value;
        Ok(changed)
    }

    /// Why the request is not what is effective (`""`: it is).
    pub(super) fn block(&self) -> &'static str {
        if self.requested == "ACTIVE" {
            inventory::ACTIVE_UNAUTHORIZED
        } else {
            ""
        }
    }

    /// The projection: the newest plan, or before the first one an empty plan of the mode
    /// (no time: nothing planned yet); `None` while OFF.
    pub(super) fn projection(&self, slot: u64) -> Option<OwnerRebalance> {
        if self.mode == RebalanceMode::Off {
            return None;
        }
        Some(self.plan.clone().unwrap_or(OwnerRebalance {
            mode: self.mode.name(),
            at_unix_ms: None,
            slot,
            trades: vec![],
            blocked: vec![],
            quotes_pending: 0,
            budget_usd_micro: self.budget_usd_micro,
        }))
    }
}

/// The mint a quote names for an inventory key (`SOL` trades as wrapped SOL).
fn mint_of(key: &str) -> Option<Pubkey> {
    if key == "SOL" {
        return darkbot_project0::protocol::NATIVE_SOL_MINT.parse().ok();
    }
    key.parse().ok()
}

impl ShadowOwner {
    /// B01 (CTRL-03): the operator's `INVENTORY_REBALANCING` request, decided by the owner's
    /// observation (`Observer::request`).
    pub(super) fn rebalance_request(&mut self, requested: &str) -> Result<String, String> {
        let banded = !self.inventory_targets.is_empty();
        if self.rebalance.request(requested, banded)? {
            self.rebalance_generation();
        }
        Ok(requested.to_string())
    }

    /// A new observation generation, shared with the quote worker: what an earlier one asked is
    /// neither quoted any more nor taken.
    fn rebalance_generation(&mut self) {
        self.rebalance.generation += 1;
        if let Some(queues) = &self.remote.rebalance {
            queues.generation.store(
                self.rebalance.generation,
                std::sync::atomic::Ordering::Release,
            );
        }
    }

    /// The request, what is effective and why not as requested.
    pub(super) fn rebalance_state(&self) -> (&'static str, &'static str, String) {
        (
            self.rebalance.requested,
            self.rebalance.mode.name(),
            self.rebalance.block().into(),
        )
    }

    /// One owner pass of the observation: take answered quotes, plan when due and no quote is
    /// pending, ask the quotes the plan lacks (next round) or schedule the next observation.
    pub(super) fn rebalance_observe(&mut self) {
        if self.rebalance.mode != RebalanceMode::Observe || self.inventory_targets.is_empty() {
            return;
        }
        if let Some(queues) = &self.remote.rebalance {
            while let Some(r) = queues.results.try_pop() {
                if r.generation != self.rebalance.generation {
                    continue;
                }
                self.rebalance.pending.remove(&r.key);
                // An unreachable provider is an unknown quote too (never a guessed output).
                self.rebalance
                    .quotes
                    .insert(r.key, r.minimum_out.ok().flatten());
            }
        }
        if !self.rebalance.pending.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        if self.rebalance.round == 0 {
            // Balances and prices come from canonical state only: nothing before Ready.
            if self.rebalance.due.is_some_and(|due| now < due) || !self.st.run_state().is_tradable()
            {
                return;
            }
            // Quotes are as old as their observation: every observation asks afresh.
            self.rebalance.quotes.clear();
            self.rebalance_generation();
            // L2: an unreadable FE-04 book leaves no balance known — nothing is planned or
            // recorded (never "no surplus" from a book that could not be read).
            if self.reservations.is_unreadable() {
                self.rebalance.plan = Some(OwnerRebalance {
                    mode: self.rebalance.mode.name(),
                    at_unix_ms: crate::status_tap::unix_ms_now(),
                    slot: self.st.highest_slot(),
                    trades: vec![],
                    blocked: self
                        .inventory_targets
                        .keys()
                        .map(|k| (k.clone(), "RESERVATION_BOOK_UNREADABLE"))
                        .collect(),
                    quotes_pending: 0,
                    budget_usd_micro: self.rebalance.budget_usd_micro,
                });
                self.rebalance.due = Some(now + self.rebalance.interval);
                return;
            }
        }
        let (budget, rounds, interval) = (
            self.rebalance.budget_usd_micro,
            self.rebalance.rounds,
            self.rebalance.interval,
        );
        let available = self.rebalance_balances();
        let fee_lamports = crate::execution::pipeline::planned_fee_lamports(1, &self.resources);
        let cost = |sell: &str, amount_in: u64, buy: &str, out: u64| -> Option<u64> {
            let paid = self.asset_value_high(sell, amount_in)?;
            let got = self.asset_value_low(buy, out)?;
            let fee = self.asset_value_high("SOL", fee_lamports?)?;
            paid.saturating_sub(got).checked_add(fee)
        };
        // Collateral funds only at a known conservative value (and its trade's answered quote).
        let collateral_known = |asset: &str| {
            available
                .get(asset)
                .is_some_and(|a| self.asset_value_low(asset, *a).is_some())
        };
        // A key naming no mint, or one mint on both sides (WSOL against the SOL band: an
        // unwrap, no swap), has no route.
        let routable = |sell: &str, buy: &str| match (mint_of(sell), mint_of(buy)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        };
        let (actions, wanted) = inventory::plan_with_quotes(
            &available,
            &self.inventory_targets,
            &collateral_known,
            &routable,
            &self.rebalance.quotes,
            &cost,
            budget,
        );
        let (mut asked, mut refused) = (0u64, 0u64);
        if self.rebalance.round < rounds {
            if let Some(queues) = &self.remote.rebalance {
                for key in wanted {
                    // Only routable keys are wanted (`routable`).
                    let (Some(sell_mint), Some(buy_mint)) = (mint_of(&key.0), mint_of(&key.1))
                    else {
                        self.rebalance.quotes.insert(key, None);
                        continue;
                    };
                    let work = crate::orchestration::remote_workers::RebalanceQuoteWork {
                        key: key.clone(),
                        sell_mint,
                        buy_mint,
                        generation: self.rebalance.generation,
                    };
                    if queues.jobs.try_push(work).is_ok() {
                        self.rebalance.pending.insert(key);
                        asked += 1;
                    } else {
                        refused += 1;
                    }
                }
            }
        }
        let mut trades = Vec::new();
        let mut blocked = Vec::new();
        for action in actions {
            match action {
                Action::Rebalance {
                    sell,
                    buy,
                    amount_in,
                    minimum_out,
                    cost_usd_micro,
                } => trades.push(RebalanceTrade {
                    sell,
                    buy,
                    amount_in,
                    minimum_out,
                    cost_usd_micro,
                }),
                Action::Blocked { asset, reason } => blocked.push((asset, reason)),
            }
        }
        self.rebalance.plan = Some(OwnerRebalance {
            mode: self.rebalance.mode.name(),
            at_unix_ms: crate::status_tap::unix_ms_now(),
            slot: self.st.highest_slot(),
            trades,
            blocked,
            // A quote the full queue refused is as unanswered as a pending one.
            quotes_pending: asked + refused,
            budget_usd_micro: budget,
        });
        if asked > 0 {
            self.rebalance.round += 1;
        } else if refused > 0 {
            // Not completed: the next pass asks again (the queue holds only an earlier
            // generation's jobs, which the worker skips); nothing is recorded.
        } else {
            self.rebalance.round = 0;
            self.rebalance.due = Some(now + interval);
            self.rebalance_record();
        }
    }

    /// B01: a completed observation's plan as its typed record (a provisional plan, still
    /// waiting for quotes, has none).
    fn rebalance_record(&mut self) {
        use crate::analytics::evidence::{OwnerEvidence, RebalanceTradeRecord, RecordEvent};
        let Some(plan) = &self.rebalance.plan else {
            return;
        };
        let record = RecordEvent::RebalancePlan {
            slot: plan.slot,
            at_unix_ms: plan.at_unix_ms,
            trades: plan
                .trades
                .iter()
                .map(|t| RebalanceTradeRecord {
                    sell: t.sell.clone(),
                    buy: t.buy.clone(),
                    amount_in: t.amount_in,
                    minimum_out: t.minimum_out,
                    cost_usd_micro: t.cost_usd_micro,
                })
                .collect(),
            blocked: plan
                .blocked
                .iter()
                .map(|(asset, reason)| (asset.clone(), (*reason).into()))
                .collect(),
            budget_usd_micro: plan.budget_usd_micro,
        };
        if let Err(e) = self.evidence.emit(OwnerEvidence::Record(record)) {
            owner_log!(self, "rebalance: plan record not written {e}");
        }
    }

    /// The book-adjusted balance of every band's asset and of the wallet's other tokens of the
    /// group's bank mints (collateral): the validated ATA amount of a mint or the liquidator's
    /// lamports for SOL, minus what FE-04 attempts and their open positions hold (an unreadable
    /// book: nothing); an asset whose balance is not in canonical state is absent (unknown), a
    /// collateral without tokens too. A band's mint without its ATA is absent as well, so no plan
    /// buys into an account it would have to create (rent): ATAs are prewarmed outside
    /// (`p0-prewarm-atas`), as for liquidations.
    pub(super) fn rebalance_balances(&self) -> BTreeMap<String, u64> {
        let view = StateView(&self.st);
        let collateral: BTreeSet<String> = self
            .required_dependency_keys
            .iter()
            .filter_map(|k| {
                let r = self.st.get(k)?;
                let bank = darkbot_project0::protocol::decode_bank(&r.data).ok()?;
                (r.owner == self.program && bank.group == self.group).then(|| bank.mint.to_string())
            })
            .filter(|mint| !self.inventory_targets.contains_key(mint))
            .collect();
        let raw: BTreeMap<String, u64> = self
            .inventory_targets
            .keys()
            .chain(&collateral)
            .filter_map(|key| {
                let amount = if key == "SOL" {
                    self.st.get(&self.liquidator).map(|r| r.lamports)
                } else {
                    let mint: Pubkey = key.parse().ok()?;
                    super::accounts::validated_liquidator_ata(&view, &mint, &self.atas)
                        .map(|(_, a)| a)
                };
                amount.map(|a| (key.clone(), a))
            })
            .collect();
        raw.keys()
            .filter_map(|k| self.reservations.available(&raw, k).map(|a| (k.clone(), a)))
            .filter(|(k, a)| *a > 0 || !collateral.contains(k))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_keys_name_their_mints() {
        assert_eq!(
            mint_of("SOL").map(|m| m.to_string()).as_deref(),
            Some(darkbot_project0::protocol::NATIVE_SOL_MINT)
        );
        let mint = Pubkey::new_unique();
        assert_eq!(mint_of(&mint.to_string()), Some(mint));
        assert_eq!(mint_of("not-a-key"), None);
    }
}
