//! P8-INV-01: inventory target policy. Every held asset — liability inventory, funding, SOL with
//! its fee and safety reserve — has a band (`reserve ≤ min ≤ target ≤ max`, base units, FE-04 key:
//! `"SOL"` or the mint). The reserve is the LIVE gate: no attempt and no offer ever spends it
//! (FE-04 `Current.policy_reserve`, owner offers). Only a balance below its band is rebalanced,
//! to its target, from surplus above another band or from collateral with a proven realization,
//! never within dust and only at fully quoted costs inside the plan budget; collateral whose
//! realization is unknown is neither sold nor counted as funding. Quotes come from the existing
//! route at the call site; this is no second economics engine.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// Never spent (SOL: fees and safety reserve).
    pub reserve: u64,
    pub min: u64,
    pub target: u64,
    pub max: u64,
    /// Smaller moves are not worth their fixed costs.
    pub dust: u64,
}

/// A quoted ExactIn rebalance swap: the on-chain enforced output and its full cost (swap fees,
/// impact, network fee, rent); `None` when unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RebalanceQuote {
    pub minimum_out: Option<u64>,
    pub cost_usd_micro: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Sell `amount_in` of `sell` for at least `minimum_out` of `buy`.
    Rebalance {
        sell: String,
        buy: String,
        amount_in: u64,
        minimum_out: u64,
        cost_usd_micro: u64,
    },
    /// No safe rebalance for `asset` (stable code).
    Blocked { asset: String, reason: &'static str },
}

/// `P0_INVENTORY_TARGETS`: `KEY=reserve:min:target:max:dust` entries separated by `;`, KEY `SOL`
/// or a mint. Empty is no policy; any malformed, duplicate or inconsistent entry is an error.
pub fn parse_targets(text: &str) -> Result<BTreeMap<String, Target>, String> {
    let mut out = BTreeMap::new();
    for entry in text.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let bad = || format!("INVENTORY_TARGETS_INVALID: {entry}");
        let (key, band) = entry.split_once('=').ok_or_else(bad)?;
        let key = key.trim();
        if key != "SOL" && key.parse::<solana_sdk::pubkey::Pubkey>().is_err() {
            return Err(bad());
        }
        let n: Vec<u64> = band
            .split(':')
            .map(|v| v.trim().parse().map_err(|_| bad()))
            .collect::<Result<_, _>>()?;
        let [reserve, min, target, max, dust] = n[..] else {
            return Err(bad());
        };
        if !(reserve <= min && min <= target && target <= max)
            || out
                .insert(
                    key.to_string(),
                    Target {
                        reserve,
                        min,
                        target,
                        max,
                        dust,
                    },
                )
                .is_some()
        {
            return Err(bad());
        }
    }
    Ok(out)
}

/// The never-spent reserve per asset (the LIVE gate of FE-04 and of owner offers).
pub fn reserves(targets: &BTreeMap<String, Target>) -> BTreeMap<String, u64> {
    targets
        .iter()
        .map(|(k, t)| (k.clone(), t.reserve))
        .collect()
}

/// Minimal rebalancing. `available` is the book-adjusted balance (P8-RES); held assets outside
/// `targets` are collateral and fund rebalancing only with a proven realization.
pub fn rebalance(
    available: &BTreeMap<String, u64>,
    targets: &BTreeMap<String, Target>,
    collateral_known: &dyn Fn(&str) -> bool,
    quote: &dyn Fn(&str, &str, u64) -> RebalanceQuote,
    budget_usd_micro: u64,
) -> Vec<Action> {
    let mut out = Vec::new();
    let blocked = |asset: &str, reason| Action::Blocked {
        asset: asset.into(),
        reason,
    };
    // Funding sources: proven collateral first (all of it), then surplus above a band's target.
    let mut surplus: Vec<(String, u64)> = Vec::new();
    for (asset, amount) in available {
        if *amount == 0 || targets.contains_key(asset) {
            continue;
        }
        if collateral_known(asset) {
            surplus.push((asset.clone(), *amount));
        } else {
            out.push(blocked(asset, "COLLATERAL_REALIZATION_UNKNOWN"));
        }
    }
    surplus.extend(targets.iter().filter_map(|(k, t)| {
        let a = *available.get(k)?;
        (a > t.max && a - t.target > t.dust).then(|| (k.clone(), a - t.target))
    }));
    let mut spent = 0u64;
    for (asset, t) in targets {
        let Some(have) = available.get(asset) else {
            out.push(blocked(asset, "INVENTORY_BALANCE_UNKNOWN"));
            continue;
        };
        let need = t.target.saturating_sub(*have);
        if *have >= t.min || need <= t.dust {
            continue;
        }
        let Some((sell, spare)) = surplus.iter_mut().find(|(k, s)| k != asset && *s > 0) else {
            out.push(blocked(asset, "REBALANCE_NO_SURPLUS"));
            continue;
        };
        // Sell only what the shortfall needs: scale the full ExactIn quote down to `need`.
        let full = quote(sell, asset, *spare);
        let amount_in = match full.minimum_out {
            Some(o) if o > need => {
                (u128::from(*spare) * u128::from(need)).div_ceil(u128::from(o)) as u64
            }
            _ => *spare,
        };
        let q = if amount_in == *spare {
            full
        } else {
            quote(sell, asset, amount_in)
        };
        let (Some(minimum_out), Some(cost)) = (q.minimum_out, q.cost_usd_micro) else {
            out.push(blocked(asset, "REBALANCE_COST_UNKNOWN"));
            continue;
        };
        if spent.saturating_add(cost) > budget_usd_micro {
            out.push(blocked(asset, "REBALANCE_COST_ABOVE_BUDGET"));
            continue;
        }
        if minimum_out <= t.dust {
            out.push(blocked(asset, "REBALANCE_OUTPUT_DUST"));
            continue;
        }
        spent += cost;
        *spare -= amount_in;
        out.push(Action::Rebalance {
            sell: sell.clone(),
            buy: asset.clone(),
            amount_in,
            minimum_out,
            cost_usd_micro: cost,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band(min: u64, target: u64, max: u64) -> Target {
        Target {
            reserve: min / 2,
            min,
            target,
            max,
            dust: 5,
        }
    }

    /// P8-INV-01: only an asset below its band trades, from proven collateral or surplus above a
    /// target, selling only what its shortfall needs, at known costs inside the budget; dust,
    /// unknown balances, unknown costs and unproven collateral never trade.
    #[test]
    fn only_needed_rebalancing_at_known_bounded_cost() {
        let (usdc, liab) = ("USDC".to_string(), "LIAB".to_string());
        let targets: BTreeMap<String, Target> = [
            ("SOL".to_string(), band(50, 100, 200)),
            (usdc.clone(), band(500, 1_000, 2_000)),
            (liab.clone(), band(100, 300, 600)),
        ]
        .into_iter()
        .collect();
        let quote = |_: &str, _: &str, amount: u64| RebalanceQuote {
            minimum_out: Some(amount / 4),
            cost_usd_micro: Some(10),
        };
        let known = |_: &str| true;
        let avail = |sol, usdc_n, liab_n| -> BTreeMap<String, u64> {
            [
                ("SOL".to_string(), sol),
                ("USDC".to_string(), usdc_n),
                ("LIAB".to_string(), liab_n),
            ]
            .into_iter()
            .collect()
        };
        assert!(rebalance(&avail(100, 1_000, 300), &targets, &known, &quote, 50).is_empty());
        // LIAB 50 < min 100 needs 250: 1000 of the 1400 USDC surplus, not all of it.
        let buy_liab = |amount_in, minimum_out| Action::Rebalance {
            sell: usdc.clone(),
            buy: liab.clone(),
            amount_in,
            minimum_out,
            cost_usd_micro: 10,
        };
        assert_eq!(
            rebalance(&avail(100, 2_400, 50), &targets, &known, &quote, 50),
            [buy_liab(1_000, 250)]
        );
        // A surplus worth less than the need is sold whole.
        let poor = |_: &str, _: &str, amount: u64| RebalanceQuote {
            minimum_out: Some(amount / 8),
            cost_usd_micro: Some(10),
        };
        assert_eq!(
            rebalance(&avail(100, 2_200, 50), &targets, &known, &poor, 50),
            [buy_liab(1_200, 150)]
        );
        // Above min but below target: no trade (hysteresis); below min without surplus: blocked.
        assert!(rebalance(&avail(100, 1_000, 150), &targets, &known, &quote, 50).is_empty());
        assert_eq!(
            rebalance(&avail(100, 1_000, 50), &targets, &known, &quote, 50),
            [Action::Blocked {
                asset: liab.clone(),
                reason: "REBALANCE_NO_SURPLUS"
            }]
        );
        // The budget covers the whole plan; an unknown output or cost never trades.
        let block = |reason| Action::Blocked {
            asset: liab.clone(),
            reason,
        };
        assert_eq!(
            rebalance(&avail(100, 2_400, 50), &targets, &known, &quote, 9),
            [block("REBALANCE_COST_ABOVE_BUDGET")]
        );
        let unknown = |_: &str, _: &str, _: u64| RebalanceQuote {
            minimum_out: None,
            cost_usd_micro: Some(1),
        };
        assert_eq!(
            rebalance(&avail(100, 2_400, 50), &targets, &known, &unknown, 50),
            [block("REBALANCE_COST_UNKNOWN")]
        );
        let dust = |_: &str, _: &str, _: u64| RebalanceQuote {
            minimum_out: Some(5),
            cost_usd_micro: Some(1),
        };
        assert_eq!(
            rebalance(&avail(100, 2_400, 50), &targets, &known, &dust, 50),
            [block("REBALANCE_OUTPUT_DUST")]
        );
        // Collateral funds only with a proven realization; a targeted asset needs a balance.
        let mut partial = avail(100, 1_000, 50);
        partial.insert("COLLATERAL".into(), 4_000);
        assert_eq!(
            rebalance(&partial, &targets, &known, &quote, 50),
            [Action::Rebalance {
                sell: "COLLATERAL".into(),
                buy: liab.clone(),
                amount_in: 1_000,
                minimum_out: 250,
                cost_usd_micro: 10,
            }]
        );
        partial.remove("SOL");
        assert_eq!(
            rebalance(&partial, &targets, &|_| false, &quote, 50),
            [
                Action::Blocked {
                    asset: "COLLATERAL".into(),
                    reason: "COLLATERAL_REALIZATION_UNKNOWN"
                },
                block("REBALANCE_NO_SURPLUS"),
                Action::Blocked {
                    asset: "SOL".into(),
                    reason: "INVENTORY_BALANCE_UNKNOWN"
                },
            ]
        );
    }

    /// P8-INV-01: the config is strict; its reserves are the LIVE gate floors.
    #[test]
    fn inventory_targets_parse_strictly() {
        let mint = solana_sdk::pubkey::Pubkey::new_unique().to_string();
        let t = parse_targets(&format!("SOL=5:50:100:200:1; {mint}=0:10:20:40:2")).unwrap();
        assert_eq!(reserves(&t)["SOL"], 5);
        assert_eq!(t[&mint].target, 20);
        assert!(parse_targets("").unwrap().is_empty());
        for bad in [
            "SOL=5:50:100:200",
            "SOL=60:50:100:200:1",
            "SOL=5:50:300:200:1",
            "X=0:0:0:0:0",
            "SOL=1:1:1:1:1;SOL=1:1:1:1:1",
            "SOL=a:1:1:1:1",
        ] {
            assert!(parse_targets(bad).is_err(), "{bad}");
        }
    }
}
