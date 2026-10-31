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

/// B01 (CTRL-03): the capability of inventory rebalancing, requested OFF|OBSERVE|ACTIVE through
/// the one controller (`p0ctl capability set INVENTORY_REBALANCING …`, CAS on the capability
/// configuration version) — its start value is `P0_REBALANCE_MODE`; the owner decides what is
/// effective.
pub const INVENTORY_REBALANCING: &str = "INVENTORY_REBALANCING";
/// The values rebalancing may be requested with.
pub const REBALANCE_VALUES: [&str; 3] = ["OFF", "OBSERVE", "ACTIVE"];
/// ACTIVE is not effective: a rebalance trade has no authorization of its own (no FE-06 purpose,
/// no egress permit for it — a user decision); the owner observes.
pub const ACTIVE_UNAUTHORIZED: &str = "REBALANCE_ACTIVE_UNAUTHORIZED: a rebalance trade needs its own authorization (an FE-06 purpose and egress permit, a user decision); OBSERVE plans, never trades";
/// A refused request: there is nothing to observe without bands.
pub const TARGETS_REQUIRED: &str =
    "REBALANCE_TARGETS_REQUIRED: OBSERVE plans against P0_INVENTORY_TARGETS";

/// B01 (CTRL-03): the effective rebalancing mode. OBSERVE plans from the wallet's real balances,
/// real quotes and conservative costs and never trades; ACTIVE trades and needs the LIVE gate
/// (B10) and its own authorization, so it is never effective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RebalanceMode {
    Off,
    Observe,
}

impl RebalanceMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Observe => "OBSERVE",
        }
    }
}

pub fn parse_mode(text: &str) -> Result<RebalanceMode, String> {
    match text.trim() {
        "" | "off" => Ok(RebalanceMode::Off),
        "observe" => Ok(RebalanceMode::Observe),
        "active" => Err(ACTIVE_UNAUTHORIZED.into()),
        other => Err(format!("REBALANCE_MODE_INVALID: {other} (off|observe)")),
    }
}

/// A rebalance quote's key: `amount_in` of the asset `sell` for the asset `buy`.
pub type QuoteKey = (String, String, u64);

/// B01 OBSERVE: one planning pass over cached quotes (`Some(minimum_out)`, or `None` for no
/// route). A quote not cached yet is never guessed: its asset is blocked as cost unknown in this
/// pass and the quote is returned as wanted. `cost` values selling `amount_in` of `sell` for
/// `minimum_out` of `buy` conservatively. A held asset outside the bands (collateral) funds a
/// trade only when `collateral_known` (its conservative value is known) and with the answered
/// quote of that very trade — its realization.
pub fn plan_with_quotes(
    available: &BTreeMap<String, u64>,
    targets: &BTreeMap<String, Target>,
    collateral_known: &dyn Fn(&str) -> bool,
    quotes: &BTreeMap<QuoteKey, Option<u64>>,
    cost: &dyn Fn(&str, u64, &str, u64) -> Option<u64>,
    budget_usd_micro: u64,
) -> (Vec<Action>, Vec<QuoteKey>) {
    let wanted = std::cell::RefCell::new(Vec::new());
    let quote = |sell: &str, buy: &str, amount_in: u64| -> RebalanceQuote {
        let key = (sell.to_string(), buy.to_string(), amount_in);
        match quotes.get(&key) {
            Some(Some(out)) => RebalanceQuote {
                minimum_out: Some(*out),
                cost_usd_micro: cost(sell, amount_in, buy, *out),
            },
            Some(None) => RebalanceQuote {
                minimum_out: None,
                cost_usd_micro: None,
            },
            None => {
                wanted.borrow_mut().push(key);
                RebalanceQuote {
                    minimum_out: None,
                    cost_usd_micro: None,
                }
            }
        }
    };
    let actions = rebalance(
        available,
        targets,
        collateral_known,
        &quote,
        budget_usd_micro,
    );
    (actions, unique_in_order(wanted.into_inner()))
}

/// Review 72b74fc1: one request per quote, in first-asked order (the same quote can be wanted by
/// several bands, not only adjacently; `Vec::dedup` drops adjacent repeats only).
fn unique_in_order(mut keys: Vec<QuoteKey>) -> Vec<QuoteKey> {
    let mut seen = std::collections::BTreeSet::new();
    keys.retain(|k| seen.insert(k.clone()));
    keys
}

/// P5-JIT-01: `P0_JIT_FUNDING_BANK`, the bank whose inventory funds a JIT buy. Empty is none (no
/// JIT plan); anything but one bank key is an error.
pub fn parse_funding_bank(text: &str) -> Result<Option<solana_sdk::pubkey::Pubkey>, String> {
    match text.trim() {
        "" => Ok(None),
        key => key
            .parse()
            .map(Some)
            .map_err(|_| format!("JIT_FUNDING_BANK_INVALID: {key}")),
    }
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
        // The first funding source whose trade has a known output and cost; one without (no
        // route, or not answered yet) leaves the next source to try — never a guess.
        let mut tried = false;
        let mut found = None;
        for (i, (sell, spare)) in surplus.iter().enumerate() {
            if sell == asset || *spare == 0 {
                continue;
            }
            tried = true;
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
            if let (Some(minimum_out), Some(cost)) = (q.minimum_out, q.cost_usd_micro) {
                found = Some((i, amount_in, minimum_out, cost));
                break;
            }
        }
        let Some((source, amount_in, minimum_out, cost)) = found else {
            out.push(blocked(
                asset,
                if tried {
                    "REBALANCE_COST_UNKNOWN"
                } else {
                    "REBALANCE_NO_SURPLUS"
                },
            ));
            continue;
        };
        let (sell, spare) = &mut surplus[source];
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
    fn jit_funding_bank_parses_strictly() {
        let bank = solana_sdk::pubkey::Pubkey::new_unique();
        assert_eq!(parse_funding_bank(""), Ok(None));
        assert_eq!(parse_funding_bank(&format!(" {bank} ")), Ok(Some(bank)));
        for bad in ["SOL", "x", &format!("{bank};{bank}")] {
            assert!(parse_funding_bank(bad).is_err(), "{bad}");
        }
    }

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

    /// B01: OFF and OBSERVE are the modes an operator may choose; ACTIVE trades and is refused
    /// until the LIVE gate (B10); anything else is invalid.
    #[test]
    fn rebalance_mode_parses_strictly_and_active_needs_live() {
        assert_eq!(parse_mode(""), Ok(RebalanceMode::Off));
        assert_eq!(parse_mode("off"), Ok(RebalanceMode::Off));
        assert_eq!(parse_mode(" observe "), Ok(RebalanceMode::Observe));
        assert!(parse_mode("active")
            .unwrap_err()
            .starts_with("REBALANCE_ACTIVE_UNAUTHORIZED"));
        assert!(parse_mode("on")
            .unwrap_err()
            .starts_with("REBALANCE_MODE_INVALID"));
    }

    #[test]
    fn wanted_quotes_are_asked_once_even_when_not_adjacent() {
        let key = |s: &str, b: &str, n: u64| (s.to_string(), b.to_string(), n);
        assert_eq!(
            unique_in_order(vec![
                key("B", "A", 800),
                key("B", "C", 500),
                key("B", "A", 800),
                key("B", "C", 500),
                key("B", "A", 300),
            ]),
            vec![key("B", "A", 800), key("B", "C", 500), key("B", "A", 300)]
        );
    }

    /// B01 OBSERVE: a pass plans from cached quotes only — a missing quote is wanted (never
    /// guessed) and blocks its asset as cost unknown; the next pass with the quote plans the
    /// rebalance at its quoted output and conservative cost; a refused quote stays unknown.
    #[test]
    fn observe_plans_from_cached_quotes_and_asks_for_the_missing_ones() {
        let targets = BTreeMap::from([
            ("A".to_string(), band(100, 200, 300)),
            ("B".to_string(), band(100, 200, 300)),
        ]);
        let available = BTreeMap::from([("A".to_string(), 50u64), ("B".to_string(), 1_000)]);
        let cost = |sell: &str, amount_in: u64, buy: &str, out: u64| -> Option<u64> {
            assert_eq!((sell, buy), ("B", "A"));
            Some(amount_in - out)
        };
        let mut quotes = BTreeMap::new();
        let (actions, wanted) =
            plan_with_quotes(&available, &targets, &|_| false, &quotes, &cost, 1_000);
        assert_eq!(wanted, vec![("B".to_string(), "A".to_string(), 800)]);
        assert_eq!(
            actions,
            vec![Action::Blocked {
                asset: "A".into(),
                reason: "REBALANCE_COST_UNKNOWN"
            }]
        );
        // The full surplus quoted: 800 B for at least 400 A; the shortfall needs 150 A, so the
        // plan asks for the scaled quote (300 B) next, then trades it.
        quotes.insert(("B".to_string(), "A".to_string(), 800), Some(400));
        let (_, wanted) = plan_with_quotes(&available, &targets, &|_| false, &quotes, &cost, 1_000);
        assert_eq!(wanted, vec![("B".to_string(), "A".to_string(), 300)]);
        quotes.insert(("B".to_string(), "A".to_string(), 300), Some(150));
        let (actions, wanted) =
            plan_with_quotes(&available, &targets, &|_| false, &quotes, &cost, 1_000);
        assert!(wanted.is_empty());
        assert_eq!(
            actions,
            vec![Action::Rebalance {
                sell: "B".into(),
                buy: "A".into(),
                amount_in: 300,
                minimum_out: 150,
                cost_usd_micro: 150
            }]
        );
        // No route for the scaled amount: unknown, never traded.
        quotes.insert(("B".to_string(), "A".to_string(), 300), None);
        let (actions, wanted) =
            plan_with_quotes(&available, &targets, &|_| false, &quotes, &cost, 1_000);
        assert!(wanted.is_empty());
        assert!(matches!(
            actions[..],
            [Action::Blocked {
                reason: "REBALANCE_COST_UNKNOWN",
                ..
            }]
        ));
    }

    /// B01: collateral of a known value funds first, with the answered quote of its trade (its
    /// realization); without a route the next source (a band's surplus) funds instead; with no
    /// source answered the asset stays cost unknown; an unvalued collateral never funds.
    #[test]
    fn collateral_funds_with_its_realization_else_the_next_source_does() {
        let targets = BTreeMap::from([
            ("A".to_string(), band(100, 200, 300)),
            ("B".to_string(), band(100, 200, 300)),
        ]);
        let available = BTreeMap::from([
            ("A".to_string(), 50u64),
            ("B".to_string(), 1_000),
            ("C".to_string(), 600),
        ]);
        let cost = |_: &str, amount_in: u64, _: &str, out: u64| Some(amount_in - out);
        let valued = |asset: &str| asset == "C";
        let key = |s: &str, n| (s.to_string(), "A".to_string(), n);
        let mut quotes = BTreeMap::new();
        let (_, wanted) = plan_with_quotes(&available, &targets, &valued, &quotes, &cost, 1_000);
        assert_eq!(
            wanted,
            vec![key("C", 600), key("B", 800)],
            "every source is asked"
        );
        quotes.insert(key("C", 600), Some(300));
        quotes.insert(key("C", 300), Some(150));
        quotes.insert(key("B", 800), Some(400));
        quotes.insert(key("B", 300), Some(150));
        let trade = |sell: &str, amount_in| Action::Rebalance {
            sell: sell.into(),
            buy: "A".into(),
            amount_in,
            minimum_out: 150,
            cost_usd_micro: amount_in - 150,
        };
        let (actions, _) = plan_with_quotes(&available, &targets, &valued, &quotes, &cost, 1_000);
        assert_eq!(actions, vec![trade("C", 300)], "the collateral first");
        quotes.insert(key("C", 300), None);
        let (actions, wanted) =
            plan_with_quotes(&available, &targets, &valued, &quotes, &cost, 1_000);
        assert!(wanted.is_empty());
        assert_eq!(
            actions,
            vec![trade("B", 300)],
            "no route: the surplus funds"
        );
        quotes.insert(key("B", 300), None);
        let (actions, _) = plan_with_quotes(&available, &targets, &valued, &quotes, &cost, 1_000);
        assert!(matches!(
            actions[..],
            [Action::Blocked {
                reason: "REBALANCE_COST_UNKNOWN",
                ..
            }]
        ));
        let (actions, _) =
            plan_with_quotes(&available, &targets, &|_| false, &quotes, &cost, 1_000);
        assert_eq!(
            actions[0],
            Action::Blocked {
                asset: "C".into(),
                reason: "COLLATERAL_REALIZATION_UNKNOWN"
            }
        );
    }
}
