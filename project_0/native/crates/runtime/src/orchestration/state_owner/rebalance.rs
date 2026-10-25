//! B01 (P8-INV-01, CTRL-03): inventory rebalancing in OBSERVE. Every `rebalance_observe_ms` the
//! owner plans the minimal rebalancing of its bands (`P0_INVENTORY_TARGETS`) from the wallet's
//! book-adjusted balances in canonical state (validated ATAs, the liquidator's lamports, minus
//! every FE-04 hold), real ExactIn quotes and conservative costs (selling at the high, receiving
//! at the low price, plus the transaction's planned network fee), inside
//! `rebalance_budget_usd_micro`. Quotes are asked off the owner (the rebalance quote worker) in at
//! most `rebalance_quote_rounds` rounds; a quote not answered is never guessed. Nothing is ever
//! traded: ACTIVE needs the LIVE gate (B10) and is refused at the start. The plan reaches the
//! Liquidator tab with the status projection.
use super::*;
use crate::execution::inventory::{self, Action, QuoteKey, RebalanceMode};
use crate::status_tap::{OwnerRebalance, RebalanceTrade};
use std::collections::{BTreeMap, BTreeSet};

/// The owner's rebalancing observation.
pub(super) struct Observer {
    pub(super) mode: RebalanceMode,
    /// When the next observation starts (`None`: at once).
    due: Option<std::time::Instant>,
    /// Quote rounds of the running observation (0: none running).
    round: u64,
    /// The running observation's answered quotes: the minimum output, `None` for no route.
    quotes: BTreeMap<QuoteKey, Option<u64>>,
    /// Asked and not answered yet.
    pending: BTreeSet<QuoteKey>,
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
            mode,
            due: None,
            round: 0,
            quotes: BTreeMap::new(),
            pending: BTreeSet::new(),
            plan: None,
            budget_usd_micro: limits.rebalance_budget_usd_micro,
            rounds: limits.rebalance_quote_rounds,
            interval: std::time::Duration::from_millis(limits.rebalance_observe_ms),
        }
    }
}

impl Observer {
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
    /// One owner pass of the observation: take answered quotes, plan when due and no quote is
    /// pending, ask the quotes the plan lacks (next round) or schedule the next observation.
    pub(super) fn rebalance_observe(&mut self) {
        if self.rebalance.mode != RebalanceMode::Observe || self.inventory_targets.is_empty() {
            return;
        }
        if let Some(queues) = &self.remote.rebalance {
            while let Some(r) = queues.results.try_pop() {
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
        let (actions, wanted) = inventory::plan_with_quotes(
            &available,
            &self.inventory_targets,
            &self.rebalance.quotes,
            &cost,
            budget,
        );
        let mut asked = 0u64;
        if self.rebalance.round < rounds {
            if let Some(queues) = &self.remote.rebalance {
                for key in wanted {
                    let (Some(sell_mint), Some(buy_mint)) = (mint_of(&key.0), mint_of(&key.1))
                    else {
                        self.rebalance.quotes.insert(key, None);
                        continue;
                    };
                    let work = crate::orchestration::remote_workers::RebalanceQuoteWork {
                        key: key.clone(),
                        sell_mint,
                        buy_mint,
                    };
                    if queues.jobs.try_push(work).is_ok() {
                        self.rebalance.pending.insert(key);
                        asked += 1;
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
            quotes_pending: asked,
            budget_usd_micro: budget,
        });
        if asked > 0 {
            self.rebalance.round += 1;
        } else {
            self.rebalance.round = 0;
            self.rebalance.due = Some(now + interval);
        }
    }

    /// The book-adjusted balance of every band's asset: the validated ATA amount of a mint or
    /// the liquidator's lamports for SOL, minus what FE-04 attempts hold (an unreadable book:
    /// nothing); an asset whose balance is not in canonical state is absent (unknown).
    fn rebalance_balances(&self) -> BTreeMap<String, u64> {
        let view = StateView(&self.st);
        let raw: BTreeMap<String, u64> = self
            .inventory_targets
            .keys()
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
