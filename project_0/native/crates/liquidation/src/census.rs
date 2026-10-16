//! C0-02..04: per-account market census over the canonical state.
//!
//! Classes follow PHASENPLAN C0 (healthy / race / long-tail / premium-driven / unsupported), with
//! bad debt as its own row because no receivership premium applies to it. Pure and typed: the
//! caller supplies Health, the FeeState fee and an explicit cost model; nothing is defaulted.
//!
//! Model (stated, not measured): a receiver repays all equity liabilities and seizes
//! `repay × (1 + premium)` of equity assets, capped by the asset surplus; `realizable_bps` is the
//! exit haircut on seized value. Values are protocol equity (EMA) USD, not realizable exit value.
//! Long-tail assumes tagging now (0.1.12 premium ramp); under the 0.1.11 program it is potential only.

use crate::economics::{t_profitable, BPS, PREMIUM_MAX_BPS};
use crate::health::{HealthError, HealthView};
use std::collections::BTreeMap;

/// Explicit cost model; every field must come from configuration or measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CensusModel {
    pub fee_state_max_bps: u64,
    pub realizable_bps: u64,
    pub fixed_cost_usd_micro: u64,
    pub min_profit_usd_micro: i128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unsupported {
    UnknownLayout,
    Unsupported,
    Stale,
    MissingAccount,
    Crate,
    Unavailable,
}

impl Unsupported {
    pub fn of(e: &HealthError) -> Self {
        match e {
            HealthError::Unsupported(s) if s.contains("UNKNOWN_LAYOUT") => Self::UnknownLayout,
            HealthError::Unsupported(_) => Self::Unsupported,
            HealthError::Stale(_) => Self::Stale,
            HealthError::MissingAccount(_) => Self::MissingAccount,
            HealthError::Crate(_) => Self::Crate,
            HealthError::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C0Class {
    Healthy,
    /// Healthy now; projected borrow premium erodes maintenance health to zero after `eta_s`.
    PremiumDriven {
        eta_s: u64,
    },
    /// Unhealthy and net-positive at the premium available now (untagged base).
    RaceNow,
    /// Unhealthy, not net-positive now; net-positive at `t_profitable` if tagged now.
    LongTail {
        t_profitable: u64,
    },
    /// Unhealthy and not net-positive at any premium up to the cap.
    Uneconomic,
    /// Unweighted assets below liabilities: bankruptcy path.
    BadDebt,
    Unsupported(Unsupported),
}

/// I80F48 bits (48 fraction bits, USD) → USD-micro, negative clamped to 0.
pub fn usd_micro(bits: i128) -> u64 {
    if bits <= 0 {
        return 0;
    }
    let micro = (bits as u128).saturating_mul(1_000_000) >> 48;
    micro.min(u64::MAX as u128) as u64
}

/// `premium_decay_bits_per_s`: maintenance-health loss per second from borrow premium (0 on 0.1.11).
pub fn classify(
    health: &Result<HealthView, HealthError>,
    premium_decay_bits_per_s: u128,
    now_unix_s: u64,
    m: &CensusModel,
) -> C0Class {
    let h = match health {
        Ok(h) => h,
        Err(e) => return C0Class::Unsupported(Unsupported::of(e)),
    };
    if h.is_bad_debt {
        return C0Class::BadDebt;
    }
    if !h.liquidatable {
        if premium_decay_bits_per_s > 0 && h.maintenance_health > 0 {
            let eta_s = (h.maintenance_health as u128).div_ceil(premium_decay_bits_per_s);
            return C0Class::PremiumDriven {
                eta_s: eta_s.min(u64::MAX as u128) as u64,
            };
        }
        return C0Class::Healthy;
    }
    let assets = usd_micro(h.equity_assets);
    let repay = usd_micro(h.equity_liabilities);
    if repay == 0 || assets <= repay {
        return C0Class::Uneconomic;
    }
    // Premium cannot exceed the asset surplus when all liabilities are repaid.
    let surplus_bps = ((assets - repay) as u128 * BPS as u128 / repay as u128)
        .min(PREMIUM_MAX_BPS as u128) as u64;
    let cap = if h.closeout_allowed {
        PREMIUM_MAX_BPS.min(surplus_bps)
    } else {
        surplus_bps
    };
    let profitable = |tagged_at| {
        t_profitable(
            now_unix_s,
            tagged_at,
            m.fee_state_max_bps,
            false,
            cap,
            repay,
            m.realizable_bps,
            m.fixed_cost_usd_micro,
            m.min_profit_usd_micro,
        )
    };
    // The premium is an upper bound: any level up to min(base, cap) may be taken now.
    if profitable(None).is_some() {
        return C0Class::RaceNow;
    }
    match profitable(Some(now_unix_s)) {
        Some(t) => C0Class::LongTail { t_profitable: t },
        None => C0Class::Uneconomic,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClassTotals {
    pub accounts: u64,
    pub equity_assets_usd_micro: u128,
    pub equity_liabs_usd_micro: u128,
}

impl ClassTotals {
    fn add(&mut self, h: Option<&HealthView>) {
        self.accounts += 1;
        if let Some(h) = h {
            self.equity_assets_usd_micro += usd_micro(h.equity_assets) as u128;
            self.equity_liabs_usd_micro += usd_micro(h.equity_liabilities) as u128;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub healthy: ClassTotals,
    pub premium_driven: ClassTotals,
    pub race_now: ClassTotals,
    pub long_tail: ClassTotals,
    pub uneconomic: ClassTotals,
    pub bad_debt: ClassTotals,
    pub unsupported: BTreeMap<Unsupported, u64>,
    /// Long-tail accounts by delay until `t_profitable`: ≤ 1 h, ≤ 24 h, ≤ 7 d.
    pub long_tail_within: [u64; 3],
}

impl Census {
    pub fn add(&mut self, class: C0Class, health: Option<&HealthView>, now_unix_s: u64) {
        match class {
            C0Class::Healthy => self.healthy.add(health),
            C0Class::PremiumDriven { .. } => self.premium_driven.add(health),
            C0Class::RaceNow => self.race_now.add(health),
            C0Class::LongTail { t_profitable } => {
                self.long_tail.add(health);
                let wait = t_profitable.saturating_sub(now_unix_s);
                for (i, limit) in [3_600u64, 86_400, 7 * 86_400].into_iter().enumerate() {
                    if wait <= limit {
                        self.long_tail_within[i] += 1;
                    }
                }
            }
            C0Class::Uneconomic => self.uneconomic.add(health),
            C0Class::BadDebt => self.bad_debt.add(health),
            C0Class::Unsupported(r) => *self.unsupported.entry(r).or_default() += 1,
        }
    }

    pub fn accounts(&self) -> u64 {
        [
            &self.healthy,
            &self.premium_driven,
            &self.race_now,
            &self.long_tail,
            &self.uneconomic,
            &self.bad_debt,
        ]
        .iter()
        .map(|t| t.accounts)
        .sum::<u64>()
            + self.unsupported.values().sum::<u64>()
    }
}

impl std::fmt::Display for Census {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "accounts={}", self.accounts())?;
        for (name, t) in [
            ("healthy", &self.healthy),
            ("premium_driven", &self.premium_driven),
            ("race_now", &self.race_now),
            ("long_tail", &self.long_tail),
            ("uneconomic", &self.uneconomic),
            ("bad_debt", &self.bad_debt),
        ] {
            write!(
                f,
                " {name}={}/{}/{}",
                t.accounts, t.equity_assets_usd_micro, t.equity_liabs_usd_micro
            )?;
        }
        let [h1, d1, d7] = self.long_tail_within;
        write!(
            f,
            " long_tail_within_1h={h1} within_24h={d1} within_7d={d7}"
        )?;
        for (r, n) in &self.unsupported {
            write!(f, " unsupported_{r:?}={n}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_USD: i128 = 1 << 48;
    const MODEL: CensusModel = CensusModel {
        fee_state_max_bps: 0,
        realizable_bps: BPS,
        fixed_cost_usd_micro: 1_000_000,
        min_profit_usd_micro: 0,
    };

    fn view(assets_usd: i128, liabs_usd: i128, liquidatable: bool) -> HealthView {
        HealthView {
            maintenance_health: if liquidatable { -ONE_USD } else { ONE_USD },
            maint_assets: 0,
            maint_liabs: 0,
            equity_assets: assets_usd * ONE_USD,
            equity_liabilities: liabs_usd * ONE_USD,
            is_bad_debt: assets_usd < liabs_usd,
            closeout_allowed: false,
            liquidatable,
        }
    }

    #[test]
    fn classes_follow_health_and_deterministic_economics() {
        let now = 1_000_000;
        let c = |h: Result<HealthView, HealthError>, decay| classify(&h, decay, now, &MODEL);
        assert_eq!(c(Ok(view(200, 100, false)), 0), C0Class::Healthy);
        // 1 USD health, 1 USD/s of decay in I80F48 bits → one second.
        assert_eq!(
            c(Ok(view(200, 100, false)), ONE_USD as u128),
            C0Class::PremiumDriven { eta_s: 1 }
        );
        // 5 % base premium on 100 USD = 5 USD gross > 1 USD costs.
        assert_eq!(c(Ok(view(200, 100, true)), 0), C0Class::RaceNow);
        // 5 USD base premium < 6 USD costs: only the tagged ramp pays, after the grace hour.
        let tail = classify(
            &Ok(view(200, 100, true)),
            0,
            now,
            &CensusModel {
                fixed_cost_usd_micro: 6_000_000,
                ..MODEL
            },
        );
        assert!(matches!(tail, C0Class::LongTail { t_profitable } if t_profitable > now + 3_600));
        // A 2 % surplus still covers 1.01 % now; a 1 % surplus caps below it at any time.
        assert_eq!(c(Ok(view(102, 100, true)), 0), C0Class::RaceNow);
        assert_eq!(c(Ok(view(101, 100, true)), 0), C0Class::Uneconomic);
        assert_eq!(c(Ok(view(90, 100, true)), 0), C0Class::BadDebt);
        assert_eq!(
            c(
                Err(HealthError::Unsupported(
                    "UNKNOWN_LAYOUT: bank layout 0.1.12 is not the pinned crate layout".into()
                )),
                0
            ),
            C0Class::Unsupported(Unsupported::UnknownLayout)
        );
        assert_eq!(
            c(Err(HealthError::Stale("oracle".into())), 0),
            C0Class::Unsupported(Unsupported::Stale)
        );
    }

    #[test]
    fn census_totals_count_every_account_once() {
        let now = 1_000_000;
        let mut census = Census::default();
        let rows = [
            Ok(view(200, 100, false)),
            Ok(view(200, 100, true)),
            Ok(view(90, 100, true)),
            Err(HealthError::Unsupported("UNKNOWN_LAYOUT: x".into())),
            Err(HealthError::MissingAccount(
                solana_sdk::pubkey::Pubkey::new_unique(),
            )),
        ];
        for h in &rows {
            census.add(classify(h, 0, now, &MODEL), h.as_ref().ok(), now);
        }
        assert_eq!(census.accounts(), rows.len() as u64);
        assert_eq!(census.race_now.accounts, 1);
        assert_eq!(census.race_now.equity_liabs_usd_micro, 100_000_000);
        assert_eq!(census.bad_debt.equity_assets_usd_micro, 90_000_000);
        assert_eq!(census.unsupported[&Unsupported::UnknownLayout], 1);
        assert_eq!(census.unsupported[&Unsupported::MissingAccount], 1);
        let line = census.to_string();
        assert!(
            line.starts_with("accounts=5 healthy=1/200000000/100000000"),
            "{line}"
        );
        assert!(line.contains("unsupported_UnknownLayout=1"), "{line}");
    }
}
