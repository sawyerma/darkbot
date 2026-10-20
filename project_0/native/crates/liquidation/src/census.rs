//! C0-02..04: per-account market census over the canonical state.
//!
//! Classes follow PHASENPLAN C0 (healthy / race / long-tail / premium-driven / unsupported), with
//! bad debt as its own row because no receivership premium applies to it. Pure and typed: the
//! caller supplies Health, premium decay, account facts, the FeeState fee and a validated cost model.
//!
//! **Model classes, not executable economics** [C0-FIX-05]: `model_race_now`, `model_long_tail`
//! and `model_premium_driven` use a stated potential model — a receiver repays all equity
//! liabilities and seizes `repay × (1 + premium)` of equity assets, capped by the asset surplus,
//! with `realizable_bps` as exit haircut and `fixed_cost_usd_micro` as per-attempt cost. Executable
//! net edge is only ever proven by the existing CandidateInput/`economics::evaluate` pipeline.
//! Long-tail assumes tagging now (0.1.12 premium ramp); under the 0.1.11 program it is potential.
//! Every failed input is a visible reason, never a zero. [C0-FIX-02]

use crate::economics::{t_profitable, ProfitInputs, BPS, PREMIUM_MAX_BPS};
use crate::health::{HealthError, HealthView};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Validated cost model; every field comes from configuration with stated provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CensusModel {
    pub fee_state_max_bps: u64,
    pub realizable_bps: u64,
    pub fixed_cost_usd_micro: u64,
    pub min_profit_usd_micro: i128,
}

impl CensusModel {
    /// Rejects values that could turn an unknown into a gain: a zero or >100 % realizable share,
    /// a zero per-attempt cost, a negative profit floor, an out-of-range FeeState fee.
    pub fn new(
        fee_state_max_bps: u64,
        realizable_bps: u64,
        fixed_cost_usd_micro: u64,
        min_profit_usd_micro: i128,
    ) -> Result<Self, &'static str> {
        if fee_state_max_bps > BPS {
            return Err("C0_MODEL_FEE_BPS_RANGE");
        }
        if realizable_bps == 0 || realizable_bps > BPS {
            return Err("C0_MODEL_REALIZABLE_BPS_RANGE");
        }
        if fixed_cost_usd_micro == 0 {
            return Err("C0_MODEL_FIXED_COST_ZERO");
        }
        if !(0..=i64::MAX as i128).contains(&min_profit_usd_micro) {
            return Err("C0_MODEL_MIN_PROFIT_RANGE");
        }
        Ok(Self {
            fee_state_max_bps,
            realizable_bps,
            fixed_cost_usd_micro,
            min_profit_usd_micro,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Unsupported {
    UnknownLayout,
    Unsupported,
    Stale,
    MissingAccount,
    Crate,
    Unavailable,
    /// Premium decay could not be determined; never treated as zero.
    Premium,
    /// Account facts (banks, oracles, integrations, observation) could not be read.
    Facts,
    /// A value is not representable as USD-micro (negative or beyond u64); never saturated.
    ValueRange,
    /// Group member whose account layout cannot be decoded by the pinned crate.
    AccountLayout,
}

impl Unsupported {
    /// Q3: the name in the census line (`unsupported_<name>=n`) — explicit, never the derived
    /// Debug layout; the names every census line so far carries.
    pub const fn record_name(self) -> &'static str {
        match self {
            Unsupported::UnknownLayout => "UnknownLayout",
            Unsupported::Unsupported => "Unsupported",
            Unsupported::Stale => "Stale",
            Unsupported::MissingAccount => "MissingAccount",
            Unsupported::Crate => "Crate",
            Unsupported::Unavailable => "Unavailable",
            Unsupported::Premium => "Premium",
            Unsupported::Facts => "Facts",
            Unsupported::ValueRange => "ValueRange",
            Unsupported::AccountLayout => "AccountLayout",
        }
    }

    pub fn of(e: &HealthError) -> Self {
        match e {
            HealthError::UnknownLayout(_) => Self::UnknownLayout,
            HealthError::Unsupported(_) => Self::Unsupported,
            HealthError::Stale(_) | HealthError::IntegrationStale(_) => Self::Stale,
            HealthError::MissingAccount(_) => Self::MissingAccount,
            HealthError::Crate(_) => Self::Crate,
            HealthError::Unavailable => Self::Unavailable,
        }
    }
}

/// Facts of one account read from the canonical state (C0-FIX-04). Protocol-version features the
/// pinned program does not have are `None` (not applicable), never `false`/`0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountFacts {
    pub active_banks: u8,
    pub liability_banks: u8,
    /// Bit `n` set ⇔ an active bank uses `OracleSetup` discriminant `n` (pinned crate, < 64).
    pub oracle_setups: u64,
    pub kamino: bool,
    pub drift: bool,
    pub juplend: bool,
    /// Active banks with a non-zero e-mode tag.
    pub emode_banks: u8,
    /// Health observation accounts (banks + oracle/integration keys) a receivership TX must load.
    pub observation_accounts: u16,
    /// 0.1.12 `MAX_COSTLY_POSITIONS` accounting; `None` on 0.1.11.
    pub costly_positions: Option<u8>,
    /// 0.1.12 `PREMIUM_ACTIVE` on any liability bank; `None` on 0.1.11.
    pub premium_active: Option<bool>,
}

/// One account's census input, evaluated against one state view and one clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CensusRow {
    pub health: Result<HealthView, Unsupported>,
    pub premium_decay_bits_per_s: Result<u128, Unsupported>,
    pub facts: Result<AccountFacts, Unsupported>,
}

impl CensusRow {
    pub fn new(
        health: Result<HealthView, HealthError>,
        premium_decay_bits_per_s: Result<u128, HealthError>,
        facts: Result<AccountFacts, String>,
    ) -> Self {
        Self {
            health: health.map_err(|e| Unsupported::of(&e)),
            premium_decay_bits_per_s: premium_decay_bits_per_s.map_err(|_| Unsupported::Premium),
            facts: facts.map_err(|_| Unsupported::Facts),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum C0Class {
    Healthy,
    /// Model: healthy now; projected borrow premium erodes maintenance health to zero after `eta_s`.
    ModelPremiumDriven {
        eta_s: u64,
    },
    /// Model: unhealthy and net-positive now at the untagged premium.
    ModelRaceNow,
    /// Model: unhealthy, not net-positive now; net-positive at `t_profitable` if tagged now.
    ModelLongTail {
        t_profitable: u64,
    },
    /// Model: unhealthy and not net-positive at any premium up to the cap.
    ModelUneconomic,
    /// Unweighted assets below liabilities: bankruptcy path.
    BadDebt,
    Unsupported(Unsupported),
}

/// I80F48 bits (48 fraction bits, USD) → USD-micro. `None` for negative values or values beyond
/// u64: such a value is unqualified, never saturated into a seemingly valid number.
pub fn usd_micro(bits: i128) -> Option<u64> {
    if bits < 0 {
        return None;
    }
    u64::try_from((bits as u128).checked_mul(1_000_000)? >> 48).ok()
}

pub fn classify(row: &CensusRow, now_unix_s: u64, m: &CensusModel) -> C0Class {
    let h = match row.health {
        Ok(h) => h,
        Err(r) => return C0Class::Unsupported(r),
    };
    let decay = match row.premium_decay_bits_per_s {
        Ok(d) => d,
        Err(r) => return C0Class::Unsupported(r),
    };
    if let Err(r) = row.facts {
        return C0Class::Unsupported(r);
    }
    let (Some(assets), Some(repay)) = (usd_micro(h.equity_assets), usd_micro(h.equity_liabilities))
    else {
        return C0Class::Unsupported(Unsupported::ValueRange);
    };
    if h.is_bad_debt {
        return C0Class::BadDebt;
    }
    if !h.liquidatable {
        if decay > 0 && h.maintenance_health > 0 {
            let eta_s = (h.maintenance_health as u128).div_ceil(decay);
            return C0Class::ModelPremiumDriven {
                eta_s: eta_s.min(u64::MAX as u128) as u64,
            };
        }
        return C0Class::Healthy;
    }
    if repay == 0 || assets <= repay {
        return C0Class::ModelUneconomic;
    }
    // Premium cannot exceed the asset surplus when all liabilities are repaid.
    let cap = ((assets - repay) as u128 * BPS as u128 / repay as u128).min(PREMIUM_MAX_BPS as u128)
        as u64;
    let inputs = ProfitInputs {
        fee_state_max_bps: m.fee_state_max_bps,
        growth_blocked: false,
        premium_cap_bps: cap,
        repay_value: repay,
        realizable_bps: m.realizable_bps,
        fixed_costs: m.fixed_cost_usd_micro,
        min_profit: m.min_profit_usd_micro,
    };
    let profitable = |tagged_at| t_profitable(now_unix_s, tagged_at, &inputs);
    // The premium is an upper bound: any level up to min(base, cap) may be taken now.
    if profitable(None).is_some() {
        return C0Class::ModelRaceNow;
    }
    match profitable(Some(now_unix_s)) {
        Some(t) => C0Class::ModelLongTail { t_profitable: t },
        None => C0Class::ModelUneconomic,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassTotals {
    pub accounts: u64,
    pub equity_assets_usd_micro: u128,
    pub equity_liabs_usd_micro: u128,
}

impl ClassTotals {
    fn add(&mut self, (assets, liabs): (u64, u64)) {
        self.accounts += 1;
        self.equity_assets_usd_micro += assets as u128;
        self.equity_liabs_usd_micro += liabs as u128;
    }
}

/// Maintenance-health headroom of healthy accounts as share of maintenance liabilities:
/// ≤ 1 %, ≤ 5 %, ≤ 10 %, > 10 %, and accounts without maintenance liabilities.
pub const DISTANCE_BUCKETS_BPS: [u64; 3] = [100, 500, 1_000];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Census {
    pub healthy: ClassTotals,
    pub model_premium_driven: ClassTotals,
    pub model_race_now: ClassTotals,
    pub model_long_tail: ClassTotals,
    pub model_uneconomic: ClassTotals,
    pub bad_debt: ClassTotals,
    pub unsupported: BTreeMap<Unsupported, u64>,
    /// Model long-tail accounts by delay until `t_profitable`: ≤ 1 h, ≤ 24 h, ≤ 7 d.
    pub long_tail_within: [u64; 3],
    /// Healthy accounts by distance to the liquidation threshold (`DISTANCE_BUCKETS_BPS`).
    pub healthy_distance: [u64; 5],
    /// Unhealthy (race/long-tail/uneconomic) accounts: oracle setup discriminant → accounts.
    pub unhealthy_oracle_setups: BTreeMap<u8, u64>,
    /// Unhealthy accounts with Kamino / Drift / JupLend positions.
    pub unhealthy_integrations: [u64; 3],
    /// Unhealthy accounts with at least one e-mode bank.
    pub unhealthy_emode: u64,
    /// Unhealthy accounts whose observation set alone exceeds the TX locked-account limit.
    pub unhealthy_over_account_limit: u64,
    pub max_observation_accounts: u16,
    /// Classified accounts whose costly-position / premium-active facts do not exist (0.1.11).
    pub version_not_applicable: u64,
}

impl Census {
    pub fn add(
        &mut self,
        row: &CensusRow,
        class: C0Class,
        now_unix_s: u64,
        max_locked_accounts: u16,
    ) {
        let (h, facts) = match (row.health, row.facts, class) {
            (_, _, C0Class::Unsupported(r)) => {
                *self.unsupported.entry(r).or_default() += 1;
                return;
            }
            (Ok(h), Ok(f), _) => (h, f),
            // classify() maps every Err input to Unsupported; count it as such, never drop it.
            _ => {
                *self.unsupported.entry(Unsupported::Facts).or_default() += 1;
                return;
            }
        };
        // Values are checked once; an unrepresentable value is unqualified, never a class total.
        let (Some(assets), Some(liabs)) =
            (usd_micro(h.equity_assets), usd_micro(h.equity_liabilities))
        else {
            *self.unsupported.entry(Unsupported::ValueRange).or_default() += 1;
            return;
        };
        let values = (assets, liabs);
        if facts.costly_positions.is_none() || facts.premium_active.is_none() {
            self.version_not_applicable += 1;
        }
        self.max_observation_accounts = self
            .max_observation_accounts
            .max(facts.observation_accounts);
        let unhealthy = match class {
            C0Class::Healthy => {
                self.healthy.add(values);
                let bucket = if h.maint_liabs <= 0 {
                    4
                } else {
                    let bps =
                        h.maintenance_health.max(0) as u128 * BPS as u128 / h.maint_liabs as u128;
                    DISTANCE_BUCKETS_BPS
                        .iter()
                        .position(|b| bps <= *b as u128)
                        .unwrap_or(3)
                };
                self.healthy_distance[bucket] += 1;
                false
            }
            C0Class::ModelPremiumDriven { .. } => {
                self.model_premium_driven.add(values);
                false
            }
            C0Class::BadDebt => {
                self.bad_debt.add(values);
                false
            }
            C0Class::ModelRaceNow => {
                self.model_race_now.add(values);
                true
            }
            C0Class::ModelLongTail { t_profitable } => {
                self.model_long_tail.add(values);
                let wait = t_profitable.saturating_sub(now_unix_s);
                for (i, limit) in [3_600u64, 86_400, 7 * 86_400].into_iter().enumerate() {
                    if wait <= limit {
                        self.long_tail_within[i] += 1;
                    }
                }
                true
            }
            C0Class::ModelUneconomic => {
                self.model_uneconomic.add(values);
                true
            }
            C0Class::Unsupported(_) => false,
        };
        if unhealthy {
            for bit in 0..64u8 {
                if facts.oracle_setups & (1 << bit) != 0 {
                    *self.unhealthy_oracle_setups.entry(bit).or_default() += 1;
                }
            }
            for (i, has) in [facts.kamino, facts.drift, facts.juplend]
                .into_iter()
                .enumerate()
            {
                self.unhealthy_integrations[i] += has as u64;
            }
            self.unhealthy_emode += (facts.emode_banks > 0) as u64;
            self.unhealthy_over_account_limit +=
                (facts.observation_accounts > max_locked_accounts) as u64;
        }
    }

    pub fn accounts(&self) -> u64 {
        [
            &self.healthy,
            &self.model_premium_driven,
            &self.model_race_now,
            &self.model_long_tail,
            &self.model_uneconomic,
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
            ("model_premium_driven", &self.model_premium_driven),
            ("model_race_now", &self.model_race_now),
            ("model_long_tail", &self.model_long_tail),
            ("model_uneconomic", &self.model_uneconomic),
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
        let [a, b, c, d, e] = self.healthy_distance;
        write!(
            f,
            " healthy_distance_le1pct={a} le5pct={b} le10pct={c} gt10pct={d} no_maint_liabs={e}"
        )?;
        let [k, dr, j] = self.unhealthy_integrations;
        write!(
            f,
            " unhealthy_kamino={k} unhealthy_drift={dr} unhealthy_juplend={j} unhealthy_emode={} unhealthy_over_account_limit={} max_observation_accounts={} version_not_applicable={}",
            self.unhealthy_emode, self.unhealthy_over_account_limit, self.max_observation_accounts, self.version_not_applicable
        )?;
        for (setup, n) in &self.unhealthy_oracle_setups {
            write!(f, " unhealthy_oracle_setup_{setup}={n}")?;
        }
        for (r, n) in &self.unsupported {
            write!(f, " unsupported_{}={n}", r.record_name())?;
        }
        Ok(())
    }
}

/// Facts of one account from the canonical state through the pinned crate (no RPC).
#[cfg(feature = "crate_health")]
pub fn account_facts<S: crate::health::AccountSource + ?Sized>(
    state: &S,
    program: &solana_sdk::pubkey::Pubkey,
    account: &solana_sdk::pubkey::Pubkey,
    observation_accounts: usize,
) -> Result<AccountFacts, String> {
    use crate::integration::{kinds_for_account, pricing_kinds_for_account, IntegrationKind};
    use crate::protocol;
    use marginfi_type_crate::types::{BalanceSide, MarginfiAccount};
    let (owner, _, data) = state.account(account).ok_or("C0_FACTS_ACCOUNT_MISSING")?;
    if owner != *program || protocol::classify(data) != protocol::AccountType::MarginfiAccount {
        return Err("C0_FACTS_NOT_MARGINFI_ACCOUNT".into());
    }
    // B2: strict pinned layout, never a prefix decode.
    let acc = protocol::decode_marginfi_account(data).map_err(|_| "C0_FACTS_ACCOUNT_LAYOUT")?;
    let acc: &MarginfiAccount = &acc;
    let mut f = AccountFacts {
        active_banks: 0,
        liability_banks: 0,
        oracle_setups: 0,
        kamino: false,
        drift: false,
        juplend: false,
        emode_banks: 0,
        observation_accounts: u16::try_from(observation_accounts)
            .map_err(|_| "C0_FACTS_OBSERVATION_RANGE")?,
        costly_positions: None,
        premium_active: None,
    };
    for b in acc
        .lending_account
        .balances
        .iter()
        .filter(|b| b.is_active())
    {
        f.active_banks += 1;
        f.liability_banks += (!b.is_empty(BalanceSide::Liabilities)) as u8;
        let (bank_owner, _, bank_data) =
            state.account(&b.bank_pk).ok_or("C0_FACTS_BANK_MISSING")?;
        if bank_owner != *program {
            return Err("C0_FACTS_BANK_OWNER".into());
        }
        let bank = protocol::decode_bank(bank_data).map_err(|e| e.to_string())?;
        let setup = bank.config.oracle_setup as u8;
        if setup >= 64 {
            return Err("C0_FACTS_ORACLE_SETUP_RANGE".into());
        }
        f.oracle_setups |= 1 << setup;
        f.emode_banks += (bank.emode.emode_tag != 0) as u8;
    }
    for kind in kinds_for_account(state, program, account)?
        .into_iter()
        .chain(pricing_kinds_for_account(state, program, account)?)
    {
        match kind {
            IntegrationKind::Kamino => f.kamino = true,
            IntegrationKind::Drift => f.drift = true,
            IntegrationKind::JupLend => f.juplend = true,
        }
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE_USD: i128 = 1 << 48;

    fn model(fixed_cost_usd_micro: u64) -> CensusModel {
        CensusModel::new(0, BPS, fixed_cost_usd_micro, 0).unwrap()
    }

    fn facts() -> AccountFacts {
        AccountFacts {
            active_banks: 2,
            liability_banks: 1,
            oracle_setups: 1 << 3,
            kamino: true,
            drift: false,
            juplend: false,
            emode_banks: 1,
            observation_accounts: 70,
            costly_positions: None,
            premium_active: None,
        }
    }

    fn row(assets_usd: i128, liabs_usd: i128, liquidatable: bool) -> CensusRow {
        let h = HealthView {
            maintenance_health: if liquidatable { -ONE_USD } else { ONE_USD },
            maint_assets: 0,
            maint_liabs: liabs_usd * ONE_USD,
            equity_assets: assets_usd * ONE_USD,
            equity_liabilities: liabs_usd * ONE_USD,
            is_bad_debt: assets_usd < liabs_usd,
            closeout_allowed: false,
            liquidatable,
        };
        CensusRow {
            health: Ok(h),
            premium_decay_bits_per_s: Ok(0),
            facts: Ok(facts()),
        }
    }

    #[test]
    fn classes_follow_health_and_model_economics_and_errors_stay_visible() {
        let now = 1_000_000;
        let m = model(1_000_000);
        assert_eq!(classify(&row(200, 100, false), now, &m), C0Class::Healthy);
        let premium = CensusRow {
            premium_decay_bits_per_s: Ok(ONE_USD as u128),
            ..row(200, 100, false)
        };
        assert_eq!(
            classify(&premium, now, &m),
            C0Class::ModelPremiumDriven { eta_s: 1 }
        );
        // 5 % base premium on 100 USD = 5 USD gross > 1 USD costs.
        assert_eq!(
            classify(&row(200, 100, true), now, &m),
            C0Class::ModelRaceNow
        );
        // 5 USD base premium < 6 USD costs: only the tagged ramp pays, after the grace hour.
        let tail = classify(&row(200, 100, true), now, &model(6_000_000));
        assert!(
            matches!(tail, C0Class::ModelLongTail { t_profitable } if t_profitable > now + 3_600)
        );
        // A 2 % surplus still covers 1.01 % now; a 1 % surplus caps below it at any time.
        assert_eq!(
            classify(&row(102, 100, true), now, &m),
            C0Class::ModelRaceNow
        );
        assert_eq!(
            classify(&row(101, 100, true), now, &m),
            C0Class::ModelUneconomic
        );
        assert_eq!(classify(&row(90, 100, true), now, &m), C0Class::BadDebt);
        // [C0-FIX-02] Errors are reasons, never zero premium or zero facts.
        let healthy = row(200, 100, false).health.unwrap();
        let r = CensusRow::new(Ok(healthy), Err(HealthError::Unavailable), Ok(facts()));
        assert_eq!(
            classify(&r, now, &m),
            C0Class::Unsupported(Unsupported::Premium)
        );
        let r = CensusRow::new(Ok(healthy), Ok(0), Err("C0_FACTS_BANK_MISSING".into()));
        assert_eq!(
            classify(&r, now, &m),
            C0Class::Unsupported(Unsupported::Facts)
        );
        let r = CensusRow::new(
            Err(HealthError::UnknownLayout(
                "UNKNOWN_LAYOUT: bank layout 0.1.12 is not the pinned crate layout".into(),
            )),
            Ok(0),
            Ok(facts()),
        );
        assert_eq!(
            classify(&r, now, &m),
            C0Class::Unsupported(Unsupported::UnknownLayout)
        );
        // Unrepresentable values are unqualified, never saturated into a valid-looking class.
        for (assets, liabs) in [(i128::MAX, 100 * ONE_USD), (-ONE_USD, 100 * ONE_USD)] {
            let mut r = row(200, 100, true);
            let mut h = r.health.unwrap();
            (h.equity_assets, h.equity_liabilities) = (assets, liabs);
            r.health = Ok(h);
            assert_eq!(
                classify(&r, now, &m),
                C0Class::Unsupported(Unsupported::ValueRange)
            );
            let mut census = Census::default();
            census.add(&r, C0Class::ModelRaceNow, now, 64);
            assert_eq!(census.unsupported[&Unsupported::ValueRange], 1);
            assert_eq!(census.model_race_now.accounts, 0);
        }
        assert_eq!(usd_micro(ONE_USD), Some(1_000_000));
    }

    #[test]
    fn model_rejects_values_that_could_invent_profit() {
        assert!(CensusModel::new(0, 0, 1, 0).is_err());
        assert!(CensusModel::new(0, BPS + 1, 1, 0).is_err());
        assert!(CensusModel::new(0, BPS, 0, 0).is_err());
        assert!(CensusModel::new(0, BPS, 1, -1).is_err());
        assert!(CensusModel::new(BPS + 1, BPS, 1, 0).is_err());
        assert!(CensusModel::new(500, 9_900, 1, 0).is_ok());
    }

    /// V19: the census class of a health error follows its variant: a strict decoder's refusal
    /// (`UnknownLayout`) is UNKNOWN_LAYOUT, an `Unsupported` error stays UNSUPPORTED even when its
    /// text names "UNKNOWN_LAYOUT"; the error text reads exactly as before the variant existed.
    #[test]
    fn v19_census_layout_class_is_typed_not_text() {
        assert_eq!(
            Unsupported::of(&HealthError::UnknownLayout("bank raw length 1".into())),
            Unsupported::UnknownLayout
        );
        assert_eq!(
            Unsupported::of(&HealthError::Unsupported(
                "CB_PRICE_GATE bank=x error=UNKNOWN_LAYOUT upstream".into()
            )),
            Unsupported::Unsupported
        );
        assert_eq!(
            HealthError::UnknownLayout("UNKNOWN_LAYOUT: bank raw length 1".into()).to_string(),
            "health unsupported (fail closed): UNKNOWN_LAYOUT: bank raw length 1"
        );
    }

    #[test]
    fn census_totals_count_every_account_once_with_facts() {
        let now = 1_000_000;
        let m = model(1_000_000);
        let mut census = Census::default();
        let rows = [
            row(200, 100, false),
            row(200, 100, true),
            row(90, 100, true),
            CensusRow::new(
                Err(HealthError::UnknownLayout("UNKNOWN_LAYOUT: x".into())),
                Ok(0),
                Ok(facts()),
            ),
            CensusRow::new(
                Err(HealthError::MissingAccount(
                    solana_sdk::pubkey::Pubkey::new_unique(),
                )),
                Ok(0),
                Ok(facts()),
            ),
        ];
        for r in &rows {
            census.add(r, classify(r, now, &m), now, 64);
        }
        assert_eq!(census.accounts(), rows.len() as u64);
        assert_eq!(census.model_race_now.equity_liabs_usd_micro, 100_000_000);
        assert_eq!(census.bad_debt.equity_assets_usd_micro, 90_000_000);
        assert_eq!(census.unsupported[&Unsupported::UnknownLayout], 1);
        // Healthy: 1 USD headroom on 100 USD maintenance liabilities = 1 % bucket.
        assert_eq!(census.healthy_distance, [1, 0, 0, 0, 0]);
        assert_eq!(census.unhealthy_oracle_setups.get(&3), Some(&1));
        assert_eq!(census.unhealthy_integrations, [1, 0, 0]);
        assert_eq!(
            (census.unhealthy_emode, census.unhealthy_over_account_limit),
            (1, 1)
        );
        assert_eq!(census.version_not_applicable, 3);
        let line = census.to_string();
        assert!(
            line.starts_with("accounts=5 healthy=1/200000000/100000000"),
            "{line}"
        );
        assert!(line.contains("unsupported_UnknownLayout=1"), "{line}");
    }
}
