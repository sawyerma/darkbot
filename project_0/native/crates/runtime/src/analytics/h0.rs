//! H0 (PHASENPLAN Track H0 — H0-01 extractor, H0-03 execution classes, H0-05 implied competitor
//! minimum-profit bands, H0-06 report): historical liquidation intelligence over the saved runs of
//! a window. Each run's selected verified result (`report::select_result`, the result a report may
//! use) yields its finalized canonical events (`market_truth`); across runs every event is taken
//! once — by signature and event index. A second read of the run keeps, only for those events'
//! transactions, their attributes (version, exact serialized bytes, accounts, lookup tables,
//! compute, top-level and CPI swaps, integration withdrawals, refreshes and oracle updates) and
//! counts the window's failed liquidation attempts (the losing competition). Nothing is read from
//! the network; what the job holds is bounded by its memory budget. Winners, concentration and
//! bands use only permissionless profit opportunities (classic, receivership): a bankruptcy
//! settlement or a forced deleverage is no competitor's win. A band is an inference, never a
//! competitor's configuration: each win bounds its winner's threshold from above — by its net edge
//! where that is complete, else by its gross edge (seized − repaid; the evidence has no SOL/USD,
//! so lamport costs are not deducted and are reported beside it). Tags, take times and premium(t)
//! (H0-04) need the 0.1.12 deployment and stay BLOCKED.
use super::journal::{self, kind};
use super::payload::Payload;
use crate::market::observer::markers::{self, Marker};
use crate::market::observer::rows::MarketTransaction;
use crate::market::observer::truth::{LiquidationType, TruthEvent};
use crate::market::observer::verification::StrictStatus;
use crate::winner::{ChainInstruction, ChainTx, TxStatus};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The window of an extraction: chain block time in `[start_unix_s, end_unix_s)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start_unix_s: i64,
    pub end_unix_s: i64,
}

impl Window {
    /// The `days` before `end_unix_s`.
    pub fn days_before(end_unix_s: i64, days: u64) -> Result<Self, String> {
        let span = i64::try_from(days)
            .ok()
            .and_then(|d| d.checked_mul(86_400))
            .filter(|s| *s > 0)
            .ok_or_else(|| format!("H0_WINDOW_INVALID: {days} days"))?;
        Ok(Self {
            start_unix_s: end_unix_s
                .checked_sub(span)
                .ok_or("H0_WINDOW_INVALID: before the epoch")?,
            end_unix_s,
        })
    }

    fn contains(&self, t: Option<i64>) -> bool {
        t.is_some_and(|t| t >= self.start_unix_s && t < self.end_unix_s)
    }
}

/// What one saved run contributed: its selected result's status, its capture window and its
/// events and failed attempts in the window (before the cross-run de-duplication), or why it
/// contributed nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCoverage {
    pub run_id: String,
    /// CANONICAL_VERIFIED | HISTORICAL_INCOMPLETE (events up to the last verified batch) |
    /// NO_VERIFIED_RESULT | UNREADABLE.
    pub status: String,
    pub reason: Option<String>,
    pub events: u64,
    /// The run's capture window (unix s, half-open); `None` when its capture is unreadable.
    pub capture_unix_s: Option<(i64, i64)>,
    /// Failed classic or receivership attempts in the window (the competitors that lost).
    pub failed_attempts: u64,
}

/// The attributes of a liquidation's transaction (H0-01); `None` where the run kept no decoded
/// transaction or the attribute is not derivable from it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxAttributes {
    pub version: Option<String>,
    /// The exact wire length (signatures and message) rebuilt from the decoded message; `None`
    /// for an incomplete message (and for v1 in a build without `txv1`).
    pub serialized_bytes: Option<u64>,
    /// Static and loaded account keys.
    pub account_count: Option<u64>,
    pub lookup_tables: u64,
    pub instruction_count: u64,
    pub compute_units_consumed: Option<u64>,
    pub compute_unit_limit: Option<u32>,
    pub compute_unit_price_micro_lamports: Option<u64>,
    /// The inner instructions were delivered: CPI evidence exists. Without it swaps, withdrawals
    /// and refreshes are the top-level ones only and the route is UNKNOWN.
    pub cpi_known: bool,
    /// Allowlisted swap programs, top level or by CPI.
    pub swap_programs: Vec<String>,
    /// Integration positions: the marginfi Kamino/Drift/JupLend withdrawals (`KAMINO` | `DRIFT` |
    /// `JUPLEND`, one per instruction).
    pub integration_withdrawals: Vec<String>,
    /// Refresh instructions: Kamino/Drift/JupLend program instructions (a receivership admits only
    /// their refreshes before its start — `validate_ix_first`).
    pub refresh_instructions: u64,
    /// Oracle updates: Pyth push-oracle instructions (no other oracle type is provable from the
    /// transaction).
    pub oracle_updates: u64,
    /// Top-level programs in instruction order: the transaction's shape (the bot-pattern key).
    pub program_sequence: Vec<String>,
}

/// One historical liquidation (H0-01) with its execution class (H0-03).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct H0Liquidation {
    /// The first run (in id order) the event was taken from.
    pub run_id: String,
    pub event: TruthEvent,
    pub tx: Option<TxAttributes>,
    /// Lamports of the visible transfers to a configured Jito tip account; `None` without a
    /// tip-account set (never guessed).
    pub jito_tip_lamports: Option<u64>,
    /// premium = seized value / repaid value − 1 (bps); `None` while either value is unknown.
    pub premium_bps: Option<i64>,
    /// The edge that bounds the winner's threshold from above (USD-micro) and its basis: `NET`
    /// (a complete net edge) or `GROSS` (seized − repaid, costs not deducted); `None`: unknown.
    pub edge_usd_micro: Option<i64>,
    pub edge_basis: Option<String>,
    /// What the transaction visibly cost in lamports: its fee (base and priority), the protocol's
    /// flat fee (receivership; a classic liquidation has none) and its visible tip; `None` while a
    /// part is unknown. A receivership's tip sits in another bundle transaction and is not in it.
    pub visible_cost_lamports: Option<u64>,
    /// `TYPE:ROUTE:CARRIER` (H0-03): ROUTE SWAP | NO_KNOWN_SWAP (no allowlisted swap at top level
    /// or by CPI) | UNKNOWN (no transaction or no CPI evidence); CARRIER TIPPED | NO_VISIBLE_TIP
    /// (a tip in another bundle TX is just as invisible) | BUNDLE_UNOBSERVABLE (a receivership TX
    /// holds no tip) | TIP_UNKNOWN (no tip-account set).
    pub execution_class: String,
}
impl Payload for H0Liquidation {
    const KIND: u16 = kind::H0_LIQUIDATION;
}

/// One winner's share of the window's competitive liquidations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinnerStats {
    pub winner: String,
    pub wins: u64,
    /// Share of the competitive liquidations of the window (bps).
    pub share_bps: u64,
    /// Distinct transaction shapes (program sequences) the winner used.
    pub shapes: u64,
}

/// H0-05: the implied minimum-profit band of one recurring winner. Every win's edge bounds the
/// winner's threshold from above (it took that edge), so the band is an upper bound: its lowest
/// known edge, with the 10th percentile and the median for context. A gross edge is a looser bound
/// (net ≤ gross); wins without any edge are counted, never folded in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinProfitBand {
    pub winner: String,
    pub wins: u64,
    pub net_edges: u64,
    pub gross_edges: u64,
    pub unknown_edges: u64,
    pub upper_bound_usd_micro: Option<i64>,
    pub p10_usd_micro: Option<i64>,
    pub p50_usd_micro: Option<i64>,
    /// The median visible cost of the wins that know it (lamports; not in the USD edges).
    pub visible_cost_lamports_p50: Option<u64>,
    /// The comparison with our minimum profit: the winner took an edge below it (`true`), or every
    /// known edge reaches it (`false`); `None` without a bound.
    pub below_our_min_profit: Option<bool>,
    /// UPPER_BOUND_ONLY (always: no take time before 0.1.12 tags), GROSS_EDGE_COSTS_NOT_DEDUCTED
    /// (a gross edge in the band) and UNKNOWN_EDGES (wins without an edge).
    pub uncertainty: Vec<String>,
}

/// p50/p95 of one quantity over the items that know it (`None`: none does).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Percentiles {
    pub samples: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
}

/// p50/p95 of a signed quantity (a negative premium is a loss, never dropped).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedPercentiles {
    pub samples: u64,
    pub p50: Option<i64>,
    pub p95: Option<i64>,
}

/// H0-06: the typed report of one extraction — the one source of its text render.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct H0Report {
    pub window: Window,
    pub runs: Vec<RunCoverage>,
    /// Seconds of the window the runs' capture windows cover (their union).
    pub covered_seconds: u64,
    pub liquidations: u64,
    /// Permissionless profit opportunities (classic, receivership): the winners' sample.
    pub competitive: u64,
    /// Events seen again in another run (taken once).
    pub duplicates: u64,
    /// Events without a chain block time: in no window, counted here.
    pub without_block_time: u64,
    /// Distinct liquidation transactions (the per-transaction quantities count each once).
    pub transactions: u64,
    pub by_type: Vec<(String, u64)>,
    pub by_execution_class: Vec<(String, u64)>,
    /// Per transaction.
    pub by_tx_version: Vec<(String, u64)>,
    /// Transaction shapes (top-level program sequences, `UNKNOWN` without a transaction), per
    /// transaction: recurring shapes are the visible bot patterns.
    pub by_shape: Vec<(String, u64)>,
    /// Winners of the competitive liquidations by wins (descending), then id.
    pub winners: Vec<WinnerStats>,
    pub top_winner_share_bps: Option<u64>,
    /// Herfindahl–Hirschman index of the winners' shares (bps of 10 000²/10 000).
    pub winner_hhi_bps: Option<u64>,
    /// Failed classic or receivership attempts in the window, each transaction once, and their
    /// distinct actors: the competition that lost.
    pub failed_attempts: u64,
    pub failed_attempt_actors: u64,
    /// Configured Jito tip accounts (`P0_JITO_TIP_ACCOUNTS`; 0: tips unknown).
    pub tip_accounts: u64,
    /// Per transaction: a visible tip; a receivership TX (tip in another bundle TX); tips unknown.
    pub tipped: u64,
    pub tip_unobservable: u64,
    pub tip_unknown: u64,
    pub tip_lamports: Percentiles,
    pub priority_fee_lamports: Percentiles,
    pub compute_units: Percentiles,
    pub serialized_bytes: Percentiles,
    /// Competitive liquidations.
    pub premium_bps: SignedPercentiles,
    /// Transactions with integration withdrawals (integration positions), with refreshes, with
    /// oracle updates, and the costly ones (integration withdrawals or refreshes: more accounts and
    /// compute).
    pub integration_positions: u64,
    pub with_refreshes: u64,
    pub with_oracle_updates: u64,
    pub costly: u64,
    /// Recurring winners (at least `recurring_min_wins` wins).
    pub bands: Vec<MinProfitBand>,
    pub recurring_min_wins: u64,
    /// The comparison of H0-05: our minimum profit (the registry's `min_profit_usd_micro`).
    pub our_min_profit_usd_micro: u64,
    pub bands_bounded: u64,
    pub bands_below_our_min_profit: u64,
    /// SUFFICIENT | SMALL_SAMPLE (competitive liquidations below the configured minimum; the
    /// plan's longer window applies) | MIN_SAMPLE_UNDECIDED (no minimum configured: undecided).
    pub sample_status: String,
    /// H0-04: BLOCKED until 0.1.12 is deployed (no tag timeline exists before).
    pub tags: String,
}
impl Payload for H0Report {
    const KIND: u16 = kind::H0_REPORT;
}

/// The window's liquidations and what each run contributed.
#[derive(Clone, Debug, PartialEq)]
pub struct Extract {
    pub liquidations: Vec<H0Liquidation>,
    pub runs: Vec<RunCoverage>,
    pub duplicates: u64,
    pub without_block_time: u64,
    pub failed_attempts: u64,
    pub failed_attempt_actors: u64,
    pub covered_seconds: u64,
}

impl Extract {
    /// The permissionless profit opportunities among the liquidations.
    pub fn competitive(&self) -> u64 {
        self.liquidations
            .iter()
            .filter(|x| x.event.permissionless_profit_opportunity)
            .count() as u64
    }
}

/// What an extraction holds, charged against its memory budget.
struct Held {
    bytes: u64,
    max: u64,
}

impl Held {
    fn charge<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), String> {
        use bincode::Options;
        let n = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialized_size(v)
            .map_err(|e| e.to_string())?;
        self.bytes = self.bytes.saturating_add(n);
        if self.bytes > self.max {
            return Err("H0_RAM_BUDGET".into());
        }
        Ok(())
    }
}

/// H0-01: the verified liquidations of `runs` (id, directory) whose block time lies in `window`,
/// each event once, with its transaction's attributes; `tips` are the Jito tip accounts (empty:
/// tips unknown).
pub fn extract(
    runs: &[(String, PathBuf)],
    window: Window,
    tips: &BTreeSet<Pubkey>,
) -> Result<Extract, String> {
    extract_with(runs, window, tips, &|| Ok(()), u64::MAX)
}

/// `extract` that asks `stop` before every run, for every row it reads and after the last run (a
/// cancel or an exhausted time budget ends it), and holds at most `max_ram_bytes`.
pub fn extract_with(
    runs: &[(String, PathBuf)],
    window: Window,
    tips: &BTreeSet<Pubkey>,
    stop: &dyn Fn() -> Result<(), String>,
    max_ram_bytes: u64,
) -> Result<Extract, String> {
    let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET)
        .map_err(|e| e.to_string())?;
    let mut held = Held {
        bytes: 0,
        max: max_ram_bytes,
    };
    let mut sorted: Vec<&(String, PathBuf)> = runs.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut seen: BTreeSet<(String, u32)> = BTreeSet::new();
    let mut timeless: BTreeSet<(String, u32)> = BTreeSet::new();
    let mut failed_seen: BTreeSet<String> = BTreeSet::new();
    let mut actors: BTreeSet<Pubkey> = BTreeSet::new();
    let mut covered: Vec<(i64, i64)> = vec![];
    let mut out = Extract {
        liquidations: vec![],
        runs: vec![],
        duplicates: 0,
        without_block_time: 0,
        failed_attempts: 0,
        failed_attempt_actors: 0,
        covered_seconds: 0,
    };
    for (run_id, dir) in sorted {
        stop()?;
        let coverage =
            |status: &str, reason: Option<String>, capture, events, failed| RunCoverage {
                run_id: run_id.clone(),
                status: status.into(),
                reason,
                events,
                capture_unix_s: capture,
                failed_attempts: failed,
            };
        let capture = match crate::market::observer::strict_driver::bind_capture(dir) {
            Ok(c) => c,
            Err(e) => {
                out.runs.push(coverage("UNREADABLE", Some(e), None, 0, 0));
                continue;
            }
        };
        let span = Some((
            (capture.window.0 / 1_000_000_000) as i64,
            capture.window.1.div_ceil(1_000_000_000) as i64,
        ));
        let selected = match super::report::select_result(&capture, None) {
            Ok(Some(s)) => s,
            Ok(None) => {
                out.runs
                    .push(coverage("NO_VERIFIED_RESULT", None, span, 0, 0));
                continue;
            }
            Err(e) => {
                out.runs.push(coverage("UNREADABLE", Some(e), span, 0, 0));
                continue;
            }
        };
        // A stop or the memory budget ends the job; any other failure is the run's.
        let fatal: std::cell::RefCell<Option<String>> = Default::default();
        let check = |r: Result<(), String>| {
            r.inspect_err(|e| {
                fatal.borrow_mut().get_or_insert(e.clone());
            })
        };
        let events = match super::report::verified_events(&selected, &mut |_| check(stop())) {
            Ok((events, _)) => events,
            Err(e) => match fatal.take() {
                Some(f) => return Err(f),
                None => {
                    out.runs.push(coverage("UNREADABLE", Some(e), span, 0, 0));
                    continue;
                }
            },
        };
        let admitted = admit(events, window, &mut seen, &mut timeless);
        let (fresh, n) = (admitted.fresh, admitted.in_window);
        out.duplicates += admitted.duplicates;
        out.without_block_time += admitted.timeless;
        // The second read: the attributes of these events' transactions only, and the failed
        // attempts of the window.
        let wanted: BTreeSet<&str> = fresh.iter().map(|e| e.signature.as_str()).collect();
        let mut attrs: BTreeMap<String, TxAttributes> = BTreeMap::new();
        let mut failed_here: BTreeSet<String> = BTreeSet::new();
        let read = super::report::verified_events(&selected, &mut |row: &MarketTransaction| {
            check(stop())?;
            let Some(tx) = row.chain_tx() else {
                return Ok(());
            };
            if wanted.contains(row.signature.as_str()) && !attrs.contains_key(&row.signature) {
                let a = attributes(tx);
                check(held.charge(&a))?;
                attrs.insert(row.signature.clone(), a);
            }
            if matches!(tx.status(), TxStatus::Failed(_)) && window.contains(tx.block_time) {
                let attempts: Vec<Pubkey> = markers::detect(tx, &program)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|h| {
                        matches!(
                            h.marker,
                            Marker::LendingAccountLiquidate
                                | Marker::StartLiquidation
                                | Marker::EndLiquidation
                        )
                    })
                    .map(|h| h.actor)
                    .collect();
                if !attempts.is_empty() && failed_here.insert(row.signature.clone()) {
                    check(held.charge(&row.signature))?;
                    if failed_seen.insert(row.signature.clone()) {
                        out.failed_attempts += 1;
                        actors.extend(attempts);
                    }
                }
            }
            Ok(())
        });
        if let Err(e) = read {
            match fatal.take() {
                Some(f) => return Err(f),
                None => {
                    out.runs.push(coverage("UNREADABLE", Some(e), span, 0, 0));
                    continue;
                }
            }
        }
        attach(
            fresh,
            &attrs,
            run_id,
            tips,
            &mut held,
            &mut out.liquidations,
        )?;
        let status = selected.result.status.as_str();
        let reason = (selected.result.status == StrictStatus::HistoricalIncomplete)
            .then(|| selected.result.reason.clone())
            .flatten();
        out.runs
            .push(coverage(status, reason, span, n, failed_here.len() as u64));
        if let Some(s) = span {
            covered.push(s);
        }
    }
    stop()?;
    out.failed_attempt_actors = actors.len() as u64;
    out.covered_seconds = covered_seconds(&covered, window);
    out.liquidations.sort_by(|a, b| {
        (a.event.slot, a.event.transaction_index, a.event.event_index).cmp(&(
            b.event.slot,
            b.event.transaction_index,
            b.event.event_index,
        ))
    });
    Ok(out)
}

/// One run's events admitted to an extraction.
struct Admitted {
    /// In the window and new across runs.
    fresh: Vec<TruthEvent>,
    /// The run's events in the window (before the de-duplication).
    in_window: u64,
    duplicates: u64,
    /// New events without a block time (in no window).
    timeless: u64,
}

/// The events of one run in `window`, each (signature, event index) once across runs.
fn admit(
    events: Vec<TruthEvent>,
    window: Window,
    seen: &mut BTreeSet<(String, u32)>,
    timeless: &mut BTreeSet<(String, u32)>,
) -> Admitted {
    let mut a = Admitted {
        fresh: vec![],
        in_window: 0,
        duplicates: 0,
        timeless: 0,
    };
    for event in events {
        let key = (event.signature.clone(), event.event_index);
        if event.block_time.is_none() {
            if timeless.insert(key) {
                a.timeless += 1;
            }
            continue;
        }
        if !window.contains(event.block_time) {
            continue;
        }
        a.in_window += 1;
        if !seen.insert(key) {
            a.duplicates += 1;
            continue;
        }
        a.fresh.push(event);
    }
    a
}

/// The run's fresh events as liquidations, each with its transaction's attributes — every event
/// of a transaction carries them (review: a later event of a multi-event transaction kept none).
fn attach(
    fresh: Vec<TruthEvent>,
    attrs: &BTreeMap<String, TxAttributes>,
    run_id: &str,
    tips: &BTreeSet<Pubkey>,
    held: &mut Held,
    out: &mut Vec<H0Liquidation>,
) -> Result<(), String> {
    for event in fresh {
        let tx = attrs.get(&event.signature).cloned();
        let l = liquidation(run_id, event, tx, tips);
        held.charge(&l)?;
        out.push(l);
    }
    Ok(())
}

/// The seconds of `window` that the union of `spans` (unix s, half-open) covers.
fn covered_seconds(spans: &[(i64, i64)], window: Window) -> u64 {
    let mut clipped: Vec<(i64, i64)> = spans
        .iter()
        .map(|(a, b)| ((*a).max(window.start_unix_s), (*b).min(window.end_unix_s)))
        .filter(|(a, b)| a < b)
        .collect();
    clipped.sort_unstable();
    let (mut total, mut end) = (0u64, i64::MIN);
    for (a, b) in clipped {
        let a = a.max(end);
        if b > a {
            total += (b - a) as u64;
            end = b;
        }
    }
    total
}

fn liquidation(
    run_id: &str,
    event: TruthEvent,
    tx: Option<TxAttributes>,
    tips: &BTreeSet<Pubkey>,
) -> H0Liquidation {
    let jito_tip_lamports = (!tips.is_empty()).then(|| {
        event
            .visible_payments
            .iter()
            .flatten()
            .filter(|p| {
                p.to.as_deref()
                    .and_then(|t| Pubkey::from_str(t).ok())
                    .is_some_and(|t| tips.contains(&t))
            })
            .map(|p| p.lamports)
            .fold(0u64, u64::saturating_add)
    });
    let premium_bps = match (event.seized_value_usd, event.repaid_value_usd) {
        (Some(s), Some(r)) if r > 0.0 && s.is_finite() && r.is_finite() => {
            Some(((s / r - 1.0) * 10_000.0).round() as i64)
        }
        _ => None,
    };
    let (edge_usd_micro, edge_basis) = if event.winner_net_edge_complete {
        (event.winner_net_edge_usd.and_then(usd_micro), "NET")
    } else {
        (event.gross_edge_usd.and_then(usd_micro), "GROSS")
    };
    let flat_fee = match (event.liquidation_type, event.protocol_fee_lamports) {
        (_, Some(f)) => Some(f),
        (LiquidationType::Classic, None) => Some(0),
        _ => None,
    };
    let visible_cost_lamports = event
        .transaction_fee_lamports
        .zip(flat_fee)
        .zip(jito_tip_lamports)
        .map(|((fee, flat), tip)| fee.saturating_add(flat).saturating_add(tip));
    let route = match &tx {
        Some(a) if !a.swap_programs.is_empty() => "SWAP",
        Some(a) if a.cpi_known => "NO_KNOWN_SWAP",
        _ => "UNKNOWN",
    };
    let carrier = match (event.liquidation_type, jito_tip_lamports) {
        (_, Some(t)) if t > 0 => "TIPPED",
        (LiquidationType::Receivership, _) => "BUNDLE_UNOBSERVABLE",
        (_, Some(_)) => "NO_VISIBLE_TIP",
        (_, None) => "TIP_UNKNOWN",
    };
    H0Liquidation {
        run_id: run_id.into(),
        execution_class: format!("{}:{route}:{carrier}", event.liquidation_type.as_str()),
        edge_basis: edge_usd_micro.map(|_| edge_basis.to_string()),
        edge_usd_micro,
        visible_cost_lamports,
        event,
        tx,
        jito_tip_lamports,
        premium_bps,
    }
}

/// The transaction attributes H0 records (`enrich` for version and compute budget).
pub fn attributes(tx: &ChainTx) -> TxAttributes {
    use darkbot_project0::protocol::{self as p, ix};
    let keys = tx.account_keys().ok();
    let program_of = |ix: &ChainInstruction| {
        keys.as_ref()
            .and_then(|k| k.get(usize::from(ix.program_id_index)).copied())
    };
    let ids = |ids: &[&str]| -> Vec<Pubkey> {
        ids.iter()
            .filter_map(|id| Pubkey::from_str(id).ok())
            .collect()
    };
    let (swap_ids, integration_ids, oracle_ids, marginfi) = (
        ids(&[p::JUP_SWAP_PROGRAM_ID, p::TITAN_SWAP_PROGRAM_ID]),
        ids(&[
            p::KAMINO_PROGRAM_ID,
            p::DRIFT_PROGRAM_ID,
            p::JUPLEND_PROGRAM_ID,
        ]),
        ids(&[p::PYTH_PUSH_ORACLE_ID]),
        ids(&[p::PROGRAM_ID_MAINNET]),
    );
    let inner = tx.meta.as_ref().and_then(|m| m.inner_instructions.as_ref());
    let all: Vec<(Option<Pubkey>, &ChainInstruction)> = tx
        .message
        .instructions
        .iter()
        .chain(inner.into_iter().flatten().flat_map(|g| &g.instructions))
        .map(|ix| (program_of(ix), ix))
        .collect();
    let calls = |set: &[Pubkey]| {
        all.iter()
            .filter(|(k, _)| k.is_some_and(|k| set.contains(&k)))
            .count() as u64
    };
    let mut swaps: Vec<String> = all
        .iter()
        .filter_map(|(k, _)| *k)
        .filter(|k| swap_ids.contains(k))
        .map(|k| k.to_string())
        .collect();
    swaps.sort();
    swaps.dedup();
    let mut withdrawals: Vec<String> = all
        .iter()
        .filter(|(k, _)| k.is_some_and(|k| marginfi.contains(&k)))
        .filter_map(|(_, i)| match i.data.get(..8) {
            Some(d) if d == ix::KAMINO_WITHDRAW => Some("KAMINO"),
            Some(d) if d == ix::DRIFT_WITHDRAW => Some("DRIFT"),
            Some(d) if d == ix::JUPLEND_WITHDRAW => Some("JUPLEND"),
            _ => None,
        })
        .map(str::to_string)
        .collect();
    withdrawals.sort();
    let enrichment = crate::market::observer::enrich(tx, &Pubkey::default());
    TxAttributes {
        version: tx.version.map(|_| tx.version_label()),
        serialized_bytes: wire_len(tx),
        account_count: keys.as_ref().map(|k| k.len() as u64),
        lookup_tables: tx
            .message
            .address_table_lookups
            .as_ref()
            .map_or(0, |l| l.len() as u64),
        instruction_count: tx.message.instructions.len() as u64,
        compute_units_consumed: tx.meta.as_ref().and_then(|m| m.compute_units_consumed),
        compute_unit_limit: enrichment.cu_limit,
        compute_unit_price_micro_lamports: enrichment.cu_price_micro_lamports,
        cpi_known: inner.is_some(),
        swap_programs: swaps,
        integration_withdrawals: withdrawals,
        refresh_instructions: calls(&integration_ids),
        oracle_updates: calls(&oracle_ids),
        program_sequence: tx
            .message
            .instructions
            .iter()
            .map(|ix| program_of(ix).map_or_else(|| "UNRESOLVED".into(), |k| k.to_string()))
            .collect(),
    }
}

/// The exact wire length of a transaction rebuilt from its decoded message (the signatures and
/// the message as the official serializers write them: `VersionedTransaction` for legacy and v0,
/// the SIMD-0385 v1 message with its signatures after it); `None` for a missing header or
/// blockhash, a signature that does not decode, an incomplete v1 configuration or a version this
/// build cannot serialize.
pub fn wire_len(tx: &ChainTx) -> Option<u64> {
    use darkbot_solana::chain::TxWireVersion;
    use solana_sdk::message::{v0, Message as LegacyMessage, MessageHeader, VersionedMessage};
    let (required, readonly_signed, readonly_unsigned) = tx.message.header?;
    if tx.version? == TxWireVersion::Number(1) {
        return darkbot_solana::tx::v1_wire_len(&tx.message, tx.signatures.len()).map(|n| n as u64);
    }
    let header = MessageHeader {
        num_required_signatures: required,
        num_readonly_signed_accounts: readonly_signed,
        num_readonly_unsigned_accounts: readonly_unsigned,
    };
    let recent_blockhash = tx.message.recent_blockhash.as_deref()?.parse().ok()?;
    let instructions: Vec<solana_sdk::message::compiled_instruction::CompiledInstruction> = tx
        .message
        .instructions
        .iter()
        .map(
            |ix| solana_sdk::message::compiled_instruction::CompiledInstruction {
                program_id_index: ix.program_id_index,
                accounts: ix.accounts.clone(),
                data: ix.data.clone(),
            },
        )
        .collect();
    let message = match tx.version? {
        TxWireVersion::Legacy => VersionedMessage::Legacy(LegacyMessage {
            header,
            account_keys: tx.message.account_keys.clone(),
            recent_blockhash,
            instructions,
        }),
        TxWireVersion::Number(0) => VersionedMessage::V0(v0::Message {
            header,
            account_keys: tx.message.account_keys.clone(),
            recent_blockhash,
            instructions,
            address_table_lookups: tx
                .message
                .address_table_lookups
                .iter()
                .flatten()
                .map(|l| v0::MessageAddressTableLookup {
                    account_key: l.account_key,
                    writable_indexes: l.writable_indexes.clone(),
                    readonly_indexes: l.readonly_indexes.clone(),
                })
                .collect(),
        }),
        TxWireVersion::Number(_) => return None,
    };
    let signatures = tx
        .signatures
        .iter()
        .map(|s| s.parse().ok())
        .collect::<Option<Vec<solana_sdk::signature::Signature>>>()?;
    let wire =
        darkbot_solana::submit::serialize_tx(&solana_sdk::transaction::VersionedTransaction {
            signatures,
            message,
        })
        .ok()?;
    Some(wire.len() as u64)
}

/// The `q`-th percentile (nearest rank, `q` in percent) of sorted `v`.
fn percentile<T: Copy>(v: &[T], q: u64) -> Option<T> {
    if v.is_empty() {
        return None;
    }
    let rank = (v.len() as u64 * q).div_ceil(100).max(1);
    v.get(usize::try_from(rank - 1).ok()?).copied()
}

fn percentiles(mut v: Vec<u64>) -> Percentiles {
    v.sort_unstable();
    Percentiles {
        samples: v.len() as u64,
        p50: percentile(&v, 50),
        p95: percentile(&v, 95),
    }
}

fn signed_percentiles(mut v: Vec<i64>) -> SignedPercentiles {
    v.sort_unstable();
    SignedPercentiles {
        samples: v.len() as u64,
        p50: percentile(&v, 50),
        p95: percentile(&v, 95),
    }
}

fn usd_micro(x: f64) -> Option<i64> {
    x.is_finite().then(|| (x * 1_000_000.0).round() as i64)
}

fn counts(items: impl Iterator<Item = String>) -> Vec<(String, u64)> {
    let mut m: BTreeMap<String, u64> = BTreeMap::new();
    for i in items {
        *m.entry(i).or_insert(0) += 1;
    }
    m.into_iter().collect()
}

/// H0-03/H0-05/H0-06: the report of an extraction. `recurring_min_wins`: the wins that make a
/// recurring winner (a band); `min_sample`: the configured minimum sample of competitive
/// liquidations (`None`: undecided); `tip_accounts`: the configured tip accounts.
pub fn report(
    window: Window,
    e: &Extract,
    recurring_min_wins: u64,
    min_sample: Option<u64>,
    our_min_profit_usd_micro: u64,
    tip_accounts: u64,
) -> H0Report {
    let l = &e.liquidations;
    let competitive: Vec<&H0Liquidation> = l
        .iter()
        .filter(|x| x.event.permissionless_profit_opportunity)
        .collect();
    let total = competitive.len() as u64;
    let mut by_winner: BTreeMap<&str, Vec<&H0Liquidation>> = BTreeMap::new();
    for x in &competitive {
        by_winner
            .entry(x.event.liquidator.as_str())
            .or_default()
            .push(x);
    }
    let share = |n: u64| (total > 0).then(|| n * 10_000 / total);
    let mut winners: Vec<WinnerStats> = by_winner
        .iter()
        .map(|(w, wins)| WinnerStats {
            winner: (*w).into(),
            wins: wins.len() as u64,
            share_bps: share(wins.len() as u64).unwrap_or(0),
            shapes: wins
                .iter()
                .filter_map(|x| x.tx.as_ref().map(|t| &t.program_sequence))
                .collect::<BTreeSet<_>>()
                .len() as u64,
        })
        .collect();
    winners.sort_by(|a, b| b.wins.cmp(&a.wins).then(a.winner.cmp(&b.winner)));
    let winner_hhi_bps = (total > 0).then(|| {
        by_winner
            .values()
            .map(|w| {
                let s = w.len() as u128 * 10_000 / u128::from(total);
                s * s
            })
            .sum::<u128>() as u64
            / 10_000
    });
    let our_min = i64::try_from(our_min_profit_usd_micro).unwrap_or(i64::MAX);
    let mut bands: Vec<MinProfitBand> = by_winner
        .iter()
        .filter(|(_, wins)| wins.len() as u64 >= recurring_min_wins)
        .map(|(w, wins)| {
            let mut edges: Vec<i64> = wins.iter().filter_map(|x| x.edge_usd_micro).collect();
            edges.sort_unstable();
            let basis = |b: &str| {
                wins.iter()
                    .filter(|x| x.edge_basis.as_deref() == Some(b))
                    .count() as u64
            };
            let (net_edges, gross_edges) = (basis("NET"), basis("GROSS"));
            let unknown_edges = wins.len() as u64 - edges.len() as u64;
            let mut uncertainty = vec!["UPPER_BOUND_ONLY".to_string()];
            if gross_edges > 0 {
                uncertainty.push("GROSS_EDGE_COSTS_NOT_DEDUCTED".into());
            }
            if unknown_edges > 0 {
                uncertainty.push("UNKNOWN_EDGES".into());
            }
            let upper = edges.first().copied();
            MinProfitBand {
                winner: (*w).into(),
                wins: wins.len() as u64,
                net_edges,
                gross_edges,
                unknown_edges,
                upper_bound_usd_micro: upper,
                p10_usd_micro: percentile(&edges, 10),
                p50_usd_micro: percentile(&edges, 50),
                visible_cost_lamports_p50: percentiles(
                    wins.iter()
                        .filter_map(|x| x.visible_cost_lamports)
                        .collect(),
                )
                .p50,
                below_our_min_profit: upper.map(|u| u < our_min),
                uncertainty,
            }
        })
        .collect();
    bands.sort_by(|a, b| b.wins.cmp(&a.wins).then(a.winner.cmp(&b.winner)));
    // Per transaction: its first event carries its attributes, a receivership in it makes its
    // tip unobservable.
    let mut txs: BTreeMap<&str, (&H0Liquidation, bool)> = BTreeMap::new();
    for x in l {
        let r = x.event.liquidation_type == LiquidationType::Receivership;
        txs.entry(x.event.signature.as_str())
            .and_modify(|(_, any)| *any |= r)
            .or_insert((x, r));
    }
    let tipped: Vec<u64> = txs
        .values()
        .filter_map(|(x, _)| x.jito_tip_lamports.filter(|t| *t > 0))
        .collect();
    let unobservable = txs
        .values()
        .filter(|(x, r)| *r && x.jito_tip_lamports.is_none_or(|t| t == 0))
        .count() as u64;
    let with = |f: &dyn Fn(&TxAttributes) -> bool| {
        txs.values()
            .filter(|(x, _)| x.tx.as_ref().is_some_and(f))
            .count() as u64
    };
    H0Report {
        window,
        runs: e.runs.clone(),
        covered_seconds: e.covered_seconds,
        liquidations: l.len() as u64,
        competitive: total,
        duplicates: e.duplicates,
        without_block_time: e.without_block_time,
        transactions: txs.len() as u64,
        by_type: counts(
            l.iter()
                .map(|x| x.event.liquidation_type.as_str().to_string()),
        ),
        by_execution_class: counts(l.iter().map(|x| x.execution_class.clone())),
        by_tx_version: counts(txs.values().map(|(x, _)| {
            x.tx.as_ref()
                .and_then(|t| t.version.clone())
                .unwrap_or_else(|| "UNKNOWN".into())
        })),
        by_shape: counts(txs.values().map(|(x, _)| {
            x.tx.as_ref()
                .map_or_else(|| "UNKNOWN".into(), |t| t.program_sequence.join(" > "))
        })),
        top_winner_share_bps: winners.first().map(|w| w.share_bps),
        winners,
        winner_hhi_bps,
        failed_attempts: e.failed_attempts,
        failed_attempt_actors: e.failed_attempt_actors,
        tip_accounts,
        tipped: tipped.len() as u64,
        tip_unobservable: unobservable,
        tip_unknown: txs
            .values()
            .filter(|(x, r)| !r && x.jito_tip_lamports.is_none())
            .count() as u64,
        tip_lamports: percentiles(tipped),
        priority_fee_lamports: percentiles(
            txs.values()
                .filter_map(|(x, _)| x.event.priority_fee_lamports)
                .collect(),
        ),
        compute_units: percentiles(
            txs.values()
                .filter_map(|(x, _)| x.tx.as_ref().and_then(|t| t.compute_units_consumed))
                .collect(),
        ),
        serialized_bytes: percentiles(
            txs.values()
                .filter_map(|(x, _)| x.tx.as_ref().and_then(|t| t.serialized_bytes))
                .collect(),
        ),
        premium_bps: signed_percentiles(competitive.iter().filter_map(|x| x.premium_bps).collect()),
        integration_positions: with(&|t| !t.integration_withdrawals.is_empty()),
        with_refreshes: with(&|t| t.refresh_instructions > 0),
        with_oracle_updates: with(&|t| t.oracle_updates > 0),
        costly: with(&|t| !t.integration_withdrawals.is_empty() || t.refresh_instructions > 0),
        bands_bounded: bands
            .iter()
            .filter(|b| b.upper_bound_usd_micro.is_some())
            .count() as u64,
        bands_below_our_min_profit: bands
            .iter()
            .filter(|b| b.below_our_min_profit == Some(true))
            .count() as u64,
        bands,
        recurring_min_wins,
        our_min_profit_usd_micro,
        sample_status: match min_sample {
            None => "MIN_SAMPLE_UNDECIDED".into(),
            Some(n) if total < n => "SMALL_SAMPLE".into(),
            Some(_) => "SUFFICIENT".into(),
        },
        tags: "BLOCKED: H0-04 (tags, take times, premium(t), our t_profitable) needs the 0.1.12 deployment".into(),
    }
}

/// H0-06: the report as text (`H0_LIQUIDATIONS.md`), rendered only from the typed report.
pub fn render(r: &H0Report) -> String {
    let opt = |v: Option<u64>| v.map_or_else(|| "—".to_string(), |v| v.to_string());
    let sopt = |v: Option<i64>| v.map_or_else(|| "—".to_string(), |v| v.to_string());
    let usd = |v: Option<i64>| {
        v.map_or_else(
            || "—".to_string(),
            |v| {
                format!(
                    "{}{}.{:06}",
                    if v < 0 { "-" } else { "" },
                    v.unsigned_abs() / 1_000_000,
                    v.unsigned_abs() % 1_000_000
                )
            },
        )
    };
    let bps = |v: Option<u64>| {
        v.map_or_else(
            || "—".to_string(),
            |v| format!("{}.{:02} %", v / 100, v % 100),
        )
    };
    let pct = |p: &Percentiles| format!("{} / {} (n {})", opt(p.p50), opt(p.p95), p.samples);
    let window_seconds = r.window.end_unix_s.saturating_sub(r.window.start_unix_s);
    let mut s = String::new();
    s += "# H0 — historical liquidations\n\n";
    s += &format!(
        "Window: block time {} … {} (unix s); covered by the runs' captures: {} of {} s. Liquidations: {} ({} competitive; duplicates across runs: {}; without block time: {}). Transactions: {}. Sample: {}. Tags: {}.\n\n",
        r.window.start_unix_s, r.window.end_unix_s, r.covered_seconds, window_seconds, r.liquidations, r.competitive, r.duplicates, r.without_block_time, r.transactions, r.sample_status, r.tags
    );
    s += "## Runs\n\n| Run | Status | Capture (unix s) | Events | Failed attempts | Reason |\n|---|---|---|---|---|---|\n";
    for c in &r.runs {
        s += &format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            c.run_id,
            c.status,
            c.capture_unix_s
                .map_or_else(|| "—".to_string(), |(a, b)| format!("{a} … {b}")),
            c.events,
            c.failed_attempts,
            c.reason.as_deref().unwrap_or("")
        );
    }
    for (title, rows) in [
        ("Liquidation types", &r.by_type),
        ("Execution classes", &r.by_execution_class),
        ("Transaction versions (per transaction)", &r.by_tx_version),
        ("Transaction shapes (per transaction)", &r.by_shape),
    ] {
        s += &format!("\n## {title}\n\n| Class | Count |\n|---|---|\n");
        for (k, n) in rows {
            s += &format!("| {k} | {n} |\n");
        }
    }
    s += &format!(
        "\n## Winners and competition\n\nCompetitive liquidations only. Top winner share: {}. Concentration (HHI): {}. Failed attempts in the window: {} by {} actors.\n\n| Winner | Wins | Share | Shapes |\n|---|---|---|---|\n",
        bps(r.top_winner_share_bps),
        opt(r.winner_hhi_bps),
        r.failed_attempts,
        r.failed_attempt_actors
    );
    for w in &r.winners {
        s += &format!(
            "| {} | {} | {} | {} |\n",
            w.winner,
            w.wins,
            bps(Some(w.share_bps)),
            w.shapes
        );
    }
    s += &format!(
        "\n## Costs and shape (p50 / p95, per transaction)\n\n| Quantity | p50 / p95 |\n|---|---|\n| Jito tip (lamports, tipped only) | {} |\n| Priority fee (lamports) | {} |\n| Compute units | {} |\n| Serialized bytes | {} |\n| Premium (bps, competitive) | {} / {} (n {}) |\n\nTip accounts configured: {}. Tipped: {}; tip in another bundle TX (unobservable): {}; tip unknown (no tip accounts): {}.\nIntegration positions: {}; with refreshes: {}; with oracle updates: {}; costly (integration withdrawals or refreshes): {}.\n",
        pct(&r.tip_lamports),
        pct(&r.priority_fee_lamports),
        pct(&r.compute_units),
        pct(&r.serialized_bytes),
        sopt(r.premium_bps.p50),
        sopt(r.premium_bps.p95),
        r.premium_bps.samples,
        r.tip_accounts,
        r.tipped,
        r.tip_unobservable,
        r.tip_unknown,
        r.integration_positions,
        r.with_refreshes,
        r.with_oracle_updates,
        r.costly
    );
    s += &format!(
        "\n## Implied competitor minimum profit (H0-05, inference)\n\nRecurring: at least {} wins. Our minimum profit: {} USD; bands below it: {} of {} with a bound. A band bounds the winner's threshold from above; it is never its configuration.\n\n| Winner | Wins | Net | Gross | Unknown | Upper bound (USD) | p10 | p50 | Visible cost p50 (lamports) | Below ours | Uncertainty |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
        r.recurring_min_wins,
        usd(i64::try_from(r.our_min_profit_usd_micro).ok()),
        r.bands_below_our_min_profit,
        r.bands_bounded
    );
    for b in &r.bands {
        s += &format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            b.winner,
            b.wins,
            b.net_edges,
            b.gross_edges,
            b.unknown_edges,
            usd(b.upper_bound_usd_micro),
            usd(b.p10_usd_micro),
            usd(b.p50_usd_micro),
            opt(b.visible_cost_lamports_p50),
            b.below_our_min_profit
                .map_or_else(|| "—".to_string(), |v| v.to_string()),
            b.uncertainty.join(", ")
        );
    }
    s
}

/// The H0 output of one extraction under `dir`: the text render first, then every liquidation
/// and the report, sealed and published whole (staged, then linked; an existing output is never
/// overwritten). A failed render publishes nothing.
pub fn write(dir: &Path, e: &Extract, r: &H0Report) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("H0_IO: {e}"))?;
    let path = dir.join(H0_JOURNAL);
    if path.exists() {
        return Err("H0_OUTPUT_EXISTS".into());
    }
    std::fs::write(dir.join(H0_RENDER), render(r)).map_err(|e| format!("H0_IO: {e}"))?;
    let staging = dir.join(format!(".{H0_JOURNAL}.partial-{}", std::process::id()));
    let _ = std::fs::remove_file(&staging);
    let prov = || journal::Provenance {
        source: journal::source::BACKTEST,
        ..Default::default()
    };
    let written = (|| -> std::io::Result<()> {
        let mut w = journal::Writer::create(&staging)?;
        for l in &e.liquidations {
            w.append_typed(prov(), l)?;
        }
        w.append_typed(prov(), r)?;
        w.seal(true)?;
        Ok(())
    })()
    .map_err(|e| format!("H0_IO: {e}"))
    .and_then(|()| std::fs::hard_link(&staging, &path).map_err(|e| format!("H0_IO: {e}")));
    let _ = std::fs::remove_file(&staging);
    written?;
    Ok(path)
}

/// The H0 output directory of job `job_id` under the project.
pub fn output_dir(project: &Path, job_id: &str) -> PathBuf {
    project.join("data").join("h0").join(job_id)
}

/// The Jito tip accounts H0 attributes tips to (`P0_JITO_TIP_ACCOUNTS`, comma separated; empty
/// or unset: tips unknown). An invalid key is refused, never skipped.
pub fn tip_accounts(env: &dyn Fn(&str) -> Option<String>) -> Result<BTreeSet<Pubkey>, String> {
    env("P0_JITO_TIP_ACCOUNTS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(|k| Pubkey::from_str(k).map_err(|_| format!("P0_JITO_TIP_ACCOUNTS_INVALID: {k}")))
        .collect()
}

/// The H0 job (`p0ctl job start h0`): every saved run of the project, the window of
/// `window_days` before `end_unix_s` — the registry's longer window when a configured minimum
/// sample is not reached — its report and output under `output_dir`. The registry (profile <
/// environment < config file) gives the windows, the recurring wins and our minimum profit.
/// `stop` ends it, `max_ram_bytes` bounds what it holds.
#[allow(clippy::too_many_arguments)]
pub fn run_job(
    project: &Path,
    job_id: &str,
    end_unix_s: i64,
    window_days: u64,
    min_sample: Option<u64>,
    max_ram_bytes: u64,
    env: &dyn Fn(&str) -> Option<String>,
    stop: &dyn Fn() -> Result<(), String>,
) -> Result<(H0Report, PathBuf), String> {
    let limits = darkbot_solana::runtime_config::Resolved::parse(&[], env)?
        .config
        .limits;
    let tips = tip_accounts(env)?;
    let runs: Vec<(String, PathBuf)> = crate::run_layout::runs(project)
        .into_iter()
        .map(|r| (format!("{}/{}", r.root, r.run_id), r.path))
        .collect();
    let mut window = Window::days_before(end_unix_s, window_days)?;
    let mut e = extract_with(&runs, window, &tips, stop, max_ram_bytes)?;
    if min_sample.is_some_and(|n| e.competitive() < n)
        && limits.h0_extended_window_days > window_days
    {
        window = Window::days_before(end_unix_s, limits.h0_extended_window_days)?;
        e = extract_with(&runs, window, &tips, stop, max_ram_bytes)?;
    }
    let r = report(
        window,
        &e,
        limits.h0_recurring_min_wins,
        min_sample,
        limits.min_profit_usd_micro,
        tips.len() as u64,
    );
    let path = write(&output_dir(project, job_id), &e, &r)?;
    Ok((r, path))
}

/// The typed H0 journal and its text render inside an output directory.
pub const H0_JOURNAL: &str = "h0.p0j";
pub const H0_RENDER: &str = "H0_LIQUIDATIONS.md";

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_project0::protocol as p;
    use darkbot_solana::chain::{
        ChainInstruction, ChainLookup, ChainMessage, ChainMeta, LoadedAddresses, TxStatus,
        TxWireVersion,
    };
    use solana_sdk::message::{compiled_instruction::CompiledInstruction, v0, MessageHeader};

    fn key(s: &str) -> Pubkey {
        Pubkey::from_str(s).unwrap()
    }

    /// A signed-shape v0 transaction (two signatures, a lookup table) and the same transaction
    /// as the decoder delivers it.
    fn v0_pair(
        programs: &[Pubkey],
        data: Vec<Vec<u8>>,
    ) -> (solana_sdk::transaction::VersionedTransaction, ChainTx) {
        let payer = Pubkey::new_unique();
        let mut keys = vec![payer, Pubkey::new_unique()];
        keys.extend_from_slice(programs);
        let instructions: Vec<CompiledInstruction> = programs
            .iter()
            .enumerate()
            .map(|(i, _)| CompiledInstruction {
                program_id_index: (2 + i) as u8,
                accounts: vec![0, 1],
                data: data[i].clone(),
            })
            .collect();
        let lookup = v0::MessageAddressTableLookup {
            account_key: Pubkey::new_unique(),
            writable_indexes: vec![1, 2],
            readonly_indexes: vec![3],
        };
        let blockhash = solana_sdk::hash::Hash::new_unique();
        let message = v0::Message {
            header: MessageHeader {
                num_required_signatures: 2,
                num_readonly_signed_accounts: 0,
                num_readonly_unsigned_accounts: programs.len() as u8,
            },
            account_keys: keys.clone(),
            recent_blockhash: blockhash,
            instructions: instructions.clone(),
            address_table_lookups: vec![lookup.clone()],
        };
        let sigs = vec![
            solana_sdk::signature::Signature::new_unique(),
            solana_sdk::signature::Signature::new_unique(),
        ];
        let tx = solana_sdk::transaction::VersionedTransaction {
            signatures: sigs.clone(),
            message: solana_sdk::message::VersionedMessage::V0(message),
        };
        let chain = ChainTx {
            slot: 9,
            block_time: Some(1_000),
            transaction_index: Some(3),
            version: Some(TxWireVersion::Number(0)),
            signatures: sigs.iter().map(ToString::to_string).collect(),
            message: ChainMessage {
                header: Some((2, 0, programs.len() as u8)),
                account_keys: keys,
                recent_blockhash: Some(blockhash.to_string()),
                instructions: instructions
                    .iter()
                    .map(|ix| ChainInstruction {
                        program_id_index: ix.program_id_index,
                        accounts: ix.accounts.clone(),
                        data: ix.data.clone(),
                        stack_height: None,
                    })
                    .collect(),
                address_table_lookups: Some(vec![ChainLookup {
                    account_key: lookup.account_key,
                    writable_indexes: lookup.writable_indexes,
                    readonly_indexes: lookup.readonly_indexes,
                }]),
                transaction_config: None,
            },
            meta: Some(ChainMeta {
                status: TxStatus::Success,
                fee: Some(10_000),
                pre_balances: None,
                post_balances: None,
                pre_token_balances: None,
                post_token_balances: None,
                inner_instructions: None,
                log_messages: None,
                loaded_addresses: Some(LoadedAddresses {
                    writable: vec![Pubkey::new_unique(), Pubkey::new_unique()],
                    readonly: vec![Pubkey::new_unique()],
                }),
                compute_units_consumed: Some(180_000),
                cost_units: None,
            }),
        };
        (tx, chain)
    }

    /// H0-01: the serialized bytes are the exact wire length of the transaction as signed (v0
    /// with a lookup table, legacy); a v1 message without its configuration, a missing version,
    /// blockhash or header is unknown, never guessed.
    #[test]
    fn h0_wire_len_is_the_exact_serialized_length() {
        let (tx, chain) = v0_pair(&[Pubkey::new_unique()], vec![vec![1, 2, 3]]);
        let exact = darkbot_solana::submit::serialize_tx(&tx).unwrap().len() as u64;
        assert_eq!(wire_len(&chain), Some(exact));
        let solana_sdk::message::VersionedMessage::V0(m) = &tx.message else {
            unreachable!()
        };
        let legacy_tx = solana_sdk::transaction::VersionedTransaction {
            signatures: tx.signatures.clone(),
            message: solana_sdk::message::VersionedMessage::Legacy(solana_sdk::message::Message {
                header: m.header,
                account_keys: m.account_keys.clone(),
                recent_blockhash: m.recent_blockhash,
                instructions: m.instructions.clone(),
            }),
        };
        let mut legacy = chain.clone();
        legacy.version = Some(TxWireVersion::Legacy);
        legacy.message.address_table_lookups = None;
        assert_eq!(
            wire_len(&legacy),
            Some(
                darkbot_solana::submit::serialize_tx(&legacy_tx)
                    .unwrap()
                    .len() as u64
            )
        );
        for broken in [
            ChainTx {
                version: Some(TxWireVersion::Number(1)),
                ..chain.clone()
            },
            ChainTx {
                version: None,
                ..chain.clone()
            },
        ] {
            assert_eq!(wire_len(&broken), None);
        }
        let mut no_hash = chain.clone();
        no_hash.message.recent_blockhash = None;
        assert_eq!(wire_len(&no_hash), None);
        let mut no_header = chain;
        no_header.message.header = None;
        assert_eq!(wire_len(&no_header), None);
    }

    /// H0-01: compute budget, swaps (top level and by CPI), integration withdrawals (integration
    /// positions), refreshes, oracle updates and the top-level program shape.
    #[test]
    fn h0_attributes_name_compute_swaps_integrations_oracles_and_the_shape() {
        use darkbot_project0::protocol::ix;
        let mut limit = vec![2];
        limit.extend(400_000u32.to_le_bytes());
        let mut price = vec![3];
        price.extend(5_000u64.to_le_bytes());
        let programs = [
            key(p::COMPUTE_BUDGET_PROGRAM_ID),
            key(p::COMPUTE_BUDGET_PROGRAM_ID),
            key(p::PYTH_PUSH_ORACLE_ID),
            key(p::KAMINO_PROGRAM_ID),
            key(p::JUP_SWAP_PROGRAM_ID),
            key(p::PROGRAM_ID_MAINNET),
        ];
        let (_, mut chain) = v0_pair(
            &programs,
            vec![
                limit,
                price,
                vec![9],
                vec![9],
                vec![9],
                ix::KAMINO_WITHDRAW.to_vec(),
            ],
        );
        let top = attributes(&chain);
        assert!(!top.cpi_known);
        assert_eq!(top.swap_programs, vec![p::JUP_SWAP_PROGRAM_ID.to_string()]);
        // The marginfi instruction calls Titan and a Drift withdrawal by CPI.
        chain
            .message
            .account_keys
            .push(key(p::TITAN_SWAP_PROGRAM_ID));
        let titan = (chain.message.account_keys.len() - 1) as u8;
        let marginfi = chain.message.instructions[5].program_id_index;
        let cpi = |program_id_index, data: Vec<u8>| ChainInstruction {
            program_id_index,
            accounts: vec![0],
            data,
            stack_height: Some(2),
        };
        chain.meta.as_mut().unwrap().inner_instructions =
            Some(vec![darkbot_solana::chain::InnerInstructions {
                index: 5,
                instructions: vec![
                    cpi(titan, vec![1]),
                    cpi(marginfi, ix::DRIFT_WITHDRAW.to_vec()),
                ],
            }]);
        let a = attributes(&chain);
        assert_eq!(a.version.as_deref(), Some("v0"));
        assert_eq!(
            (a.compute_unit_limit, a.compute_unit_price_micro_lamports),
            (Some(400_000), Some(5_000))
        );
        assert_eq!(a.compute_units_consumed, Some(180_000));
        assert!(a.cpi_known);
        assert_eq!(
            a.swap_programs,
            vec![
                p::JUP_SWAP_PROGRAM_ID.to_string(),
                p::TITAN_SWAP_PROGRAM_ID.to_string()
            ]
        );
        assert_eq!(a.integration_withdrawals, vec!["DRIFT", "KAMINO"]);
        assert_eq!((a.refresh_instructions, a.oracle_updates), (1, 1));
        assert_eq!((a.lookup_tables, a.instruction_count), (1, 6));
        assert_eq!(a.account_count, Some(9 + 3), "static and loaded keys");
        assert_eq!(a.program_sequence.len(), 6, "the shape is the top level");
        assert_eq!(a.program_sequence[5], p::PROGRAM_ID_MAINNET);
        assert!(a.serialized_bytes.is_some());
    }

    fn event(
        sig: &str,
        kind: LiquidationType,
        winner: &str,
        net: Option<f64>,
        complete: bool,
    ) -> TruthEvent {
        TruthEvent {
            signature: sig.into(),
            slot: 9,
            transaction_index: 0,
            event_index: 0,
            liquidation_type: kind,
            target: "target".into(),
            liquidator: winner.into(),
            liquidator_marginfi_account: None,
            asset_bank: None,
            liability_bank: None,
            asset_mint: None,
            liability_mint: None,
            seized_asset_native_exact: None,
            native_evidence: None,
            seized_value_usd: Some(105.),
            repaid_value_usd: Some(100.),
            gross_edge_usd: Some(5.),
            protocol_fee_lamports: Some(5000),
            transaction_fee_lamports: Some(10_000),
            fee_payer: None,
            visible_payments: None,
            incremental_rent_lamports: None,
            priority_fee_lamports: Some(2_000),
            winner_net_edge_usd: net,
            winner_net_edge_complete: complete,
            economic_provenance: "FIXTURE".into(),
            block_time: Some(1_000),
            live_t_rx_ns: None,
            discovery_source: "STRICT".into(),
            permissionless_profit_opportunity: true,
        }
    }

    fn tipped(mut e: TruthEvent, tip: &Pubkey, lamports: u64) -> TruthEvent {
        e.visible_payments = Some(vec![crate::market::observer::truth::VisiblePayment {
            from: Some("payer".into()),
            to: Some(tip.to_string()),
            lamports,
            cost_attribution: "CONTEXT".into(),
        }]);
        e
    }

    /// H0-03: the execution class names the route and how a tip is carried — a swap at top level
    /// or by CPI, no known swap only with CPI evidence, else UNKNOWN; a receivership TX never
    /// holds its tip (another bundle TX), a classic one without a visible tip has none visible,
    /// and without a tip-account set tips are unknown. The edge is the net one where complete,
    /// else the gross one; the visible cost needs every part.
    #[test]
    fn h0_execution_classes_name_route_and_tip_carrier() {
        let tip = Pubkey::new_unique();
        let tips: BTreeSet<Pubkey> = [tip].into();
        let (_, swap_tx) = v0_pair(&[key(p::JUP_SWAP_PROGRAM_ID)], vec![vec![9]]);
        let (_, plain_tx) = v0_pair(&[key(p::PROGRAM_ID_MAINNET)], vec![vec![9]]);
        let mut cpi_known = plain_tx.clone();
        cpi_known.meta.as_mut().unwrap().inner_instructions = Some(vec![]);
        let classic = tipped(
            event("a", LiquidationType::Classic, "w", None, false),
            &tip,
            7_000,
        );
        let x = liquidation("run", classic.clone(), Some(attributes(&swap_tx)), &tips);
        assert_eq!(x.execution_class, "CLASSIC:SWAP:TIPPED");
        assert_eq!(
            (x.jito_tip_lamports, x.premium_bps),
            (Some(7_000), Some(500))
        );
        assert_eq!(
            (x.edge_usd_micro, x.edge_basis.as_deref()),
            (Some(5_000_000), Some("GROSS"))
        );
        assert_eq!(x.visible_cost_lamports, Some(10_000 + 5_000 + 7_000));
        // A payment to another account is no tip.
        let other = tipped(
            event("b", LiquidationType::Classic, "w", None, false),
            &Pubkey::new_unique(),
            9_000,
        );
        let x = liquidation("run", other.clone(), Some(attributes(&cpi_known)), &tips);
        assert_eq!(
            (x.execution_class.as_str(), x.jito_tip_lamports),
            ("CLASSIC:NO_KNOWN_SWAP:NO_VISIBLE_TIP", Some(0))
        );
        let x = liquidation("run", other, Some(attributes(&plain_tx)), &tips);
        assert_eq!(
            x.execution_class, "CLASSIC:UNKNOWN:NO_VISIBLE_TIP",
            "no CPI evidence"
        );
        let x = liquidation(
            "run",
            event("c", LiquidationType::Receivership, "w", None, false),
            Some(attributes(&cpi_known)),
            &tips,
        );
        assert_eq!(
            x.execution_class,
            "RECEIVERSHIP:NO_KNOWN_SWAP:BUNDLE_UNOBSERVABLE"
        );
        let x = liquidation("run", classic, None, &BTreeSet::new());
        assert_eq!(
            (
                x.execution_class.as_str(),
                x.jito_tip_lamports,
                x.visible_cost_lamports
            ),
            ("CLASSIC:UNKNOWN:TIP_UNKNOWN", None, None)
        );
        let net = event("d", LiquidationType::Classic, "w", Some(1.25), true);
        let x = liquidation("run", net, None, &tips);
        assert_eq!(
            (x.edge_usd_micro, x.edge_basis.as_deref()),
            (Some(1_250_000), Some("NET"))
        );
        let mut unknown = event("e", LiquidationType::Classic, "w", None, false);
        unknown.repaid_value_usd = None;
        unknown.gross_edge_usd = None;
        let x = liquidation("run", unknown, None, &tips);
        assert_eq!(
            (x.premium_bps, x.edge_usd_micro, x.edge_basis),
            (None, None, None)
        );
        // A classic liquidation has no protocol flat fee; a receivership's must be known.
        let mut flat = event("f", LiquidationType::Classic, "w", None, false);
        flat.protocol_fee_lamports = None;
        assert_eq!(
            liquidation("run", flat.clone(), None, &tips).visible_cost_lamports,
            Some(10_000)
        );
        flat.liquidation_type = LiquidationType::Receivership;
        assert_eq!(
            liquidation("run", flat, None, &tips).visible_cost_lamports,
            None
        );
    }

    /// H0-05/H0-06: winners of the competitive liquidations only (a bankruptcy settlement is no
    /// win) with shares and concentration; tips, fees and shapes once per transaction; premiums
    /// signed; a band per recurring winner bounded from above by its lowest known edge (net or
    /// gross, unknown edges counted, never folded in) and compared with our minimum profit; the
    /// sample status over the competitive liquidations.
    #[test]
    fn h0_report_aggregates_winners_tips_costs_and_upper_bound_bands() {
        let tip = Pubkey::new_unique();
        let tips: BTreeSet<Pubkey> = [tip].into();
        let alpha = tipped(
            event("1", LiquidationType::Classic, "alpha", Some(3.0), true),
            &tip,
            10_000,
        );
        let mut alpha_b = alpha.clone();
        alpha_b.event_index = 1;
        let mut gross = tipped(
            event("2", LiquidationType::Classic, "alpha", None, false),
            &tip,
            30_000,
        );
        (gross.seized_value_usd, gross.gross_edge_usd) = (Some(95.), Some(1.5));
        let mut unknown = event("3", LiquidationType::Classic, "alpha", None, false);
        unknown.gross_edge_usd = None;
        let mut events = vec![
            alpha,
            alpha_b,
            gross,
            unknown,
            event("4", LiquidationType::Receivership, "beta", None, false),
        ];
        for sig in ["5a", "5b", "5c"] {
            let mut s = event(
                sig,
                LiquidationType::BankruptcySettlement,
                "settler",
                None,
                false,
            );
            s.permissionless_profit_opportunity = false;
            events.push(s);
        }
        let extract = |tips: &BTreeSet<Pubkey>| Extract {
            liquidations: events
                .iter()
                .map(|e| liquidation("r1", e.clone(), None, tips))
                .collect(),
            runs: vec![],
            duplicates: 2,
            without_block_time: 1,
            failed_attempts: 3,
            failed_attempt_actors: 2,
            covered_seconds: 1_500,
        };
        let e = extract(&tips);
        assert_eq!(e.competitive(), 5, "the settlements are not competitive");
        let w = Window {
            start_unix_s: 0,
            end_unix_s: 2_000,
        };
        let r = report(w, &e, 2, None, 2_000_000, 1);
        assert_eq!(
            (r.liquidations, r.competitive, r.transactions, r.duplicates),
            (8, 5, 7, 2)
        );
        assert_eq!(
            (r.without_block_time, r.covered_seconds, r.tip_accounts),
            (1, 1_500, 1)
        );
        assert_eq!((r.failed_attempts, r.failed_attempt_actors), (3, 2));
        assert_eq!(r.winners.len(), 2, "the settler is no winner");
        assert_eq!(r.winners[0].winner, "alpha");
        assert_eq!((r.winners[0].wins, r.winners[0].share_bps), (4, 8_000));
        assert_eq!(r.top_winner_share_bps, Some(8_000));
        assert_eq!(
            r.winner_hhi_bps,
            Some((8_000 * 8_000 + 2_000 * 2_000) / 10_000)
        );
        // Per transaction: "1" holds two events and one tip.
        assert_eq!((r.tipped, r.tip_unobservable, r.tip_unknown), (2, 1, 0));
        assert_eq!(
            (
                r.tip_lamports.samples,
                r.tip_lamports.p50,
                r.tip_lamports.p95
            ),
            (2, Some(10_000), Some(30_000))
        );
        assert_eq!(r.priority_fee_lamports.samples, 7);
        assert_eq!(
            r.premium_bps,
            SignedPercentiles {
                samples: 5,
                p50: Some(500),
                p95: Some(500)
            }
        );
        assert_eq!(
            signed_percentiles(vec![-500, 500]).p50,
            Some(-500),
            "a loss is kept"
        );
        assert_eq!(r.by_shape, vec![("UNKNOWN".to_string(), 7)]);
        assert_eq!(r.bands.len(), 1, "beta has one win, the settler none");
        let b = &r.bands[0];
        assert_eq!(
            (b.wins, b.net_edges, b.gross_edges, b.unknown_edges),
            (4, 2, 1, 1)
        );
        assert_eq!(
            (b.upper_bound_usd_micro, b.p10_usd_micro, b.p50_usd_micro),
            (Some(1_500_000), Some(1_500_000), Some(3_000_000)),
            "the unknown edge is not folded in"
        );
        assert_eq!(b.visible_cost_lamports_p50, Some(25_000));
        assert_eq!(b.below_our_min_profit, Some(true));
        assert_eq!(
            b.uncertainty,
            vec![
                "UPPER_BOUND_ONLY",
                "GROSS_EDGE_COSTS_NOT_DEDUCTED",
                "UNKNOWN_EDGES"
            ]
        );
        assert_eq!((r.bands_bounded, r.bands_below_our_min_profit), (1, 1));
        assert_eq!(
            report(w, &e, 2, None, 1_500_000, 1).bands[0].below_our_min_profit,
            Some(false),
            "an edge equal to ours reaches it"
        );
        assert_eq!(r.sample_status, "MIN_SAMPLE_UNDECIDED");
        assert_eq!(
            report(w, &e, 2, Some(6), 0, 1).sample_status,
            "SMALL_SAMPLE",
            "settlements are not in the sample"
        );
        assert_eq!(report(w, &e, 2, Some(5), 0, 1).sample_status, "SUFFICIENT");
        assert_eq!(report(w, &e, 1, None, 0, 1).bands.len(), 2);
        assert!(r.tags.starts_with("BLOCKED"));
        // Without tip accounts a receivership is unobservable only, every other TX unknown.
        let blind = report(w, &extract(&BTreeSet::new()), 2, None, 0, 0);
        assert_eq!(
            (blind.tipped, blind.tip_unobservable, blind.tip_unknown),
            (0, 1, 6)
        );
        let text = render(&r);
        for part in [
            "Liquidations: 8 (5 competitive; duplicates across runs: 2; without block time: 1)",
            "covered by the runs' captures: 1500 of 2000 s",
            "| alpha | 4 | 80.00 % |",
            "| alpha | 4 | 2 | 1 | 1 | 1.500000 |",
            "Our minimum profit: 2.000000 USD; bands below it: 1 of 1 with a bound",
            "UPPER_BOUND_ONLY, GROSS_EDGE_COSTS_NOT_DEDUCTED, UNKNOWN_EDGES",
            "Failed attempts in the window: 3 by 2 actors",
            "| CLASSIC:UNKNOWN:TIPPED | 3 |",
        ] {
            assert!(text.contains(part), "{part}\n{text}");
        }
        let empty = report(
            w,
            &Extract {
                liquidations: vec![],
                runs: vec![],
                duplicates: 0,
                without_block_time: 0,
                failed_attempts: 0,
                failed_attempt_actors: 0,
                covered_seconds: 0,
            },
            2,
            None,
            0,
            0,
        );
        assert_eq!(
            (
                empty.top_winner_share_bps,
                empty.winner_hhi_bps,
                empty.tip_lamports.p50
            ),
            (None, None, None)
        );
    }

    /// H0-01: a run's events are admitted in the window only, each (signature, event index) once
    /// across runs — a second event of one transaction is its own liquidation; an event without
    /// a block time is counted once and in no window. A stop after the last run ends the
    /// extraction too.
    #[test]
    fn h0_events_are_admitted_once_per_signature_and_event_index() {
        let at = |sig: &str, index: u32, t: Option<i64>| {
            let mut e = event(sig, LiquidationType::Classic, "w", None, false);
            (e.event_index, e.block_time) = (index, t);
            e
        };
        let w = Window {
            start_unix_s: 100,
            end_unix_s: 200,
        };
        let (mut seen, mut timeless) = (BTreeSet::new(), BTreeSet::new());
        let run = || {
            vec![
                at("a", 0, Some(100)),
                at("a", 1, Some(100)),
                at("b", 0, Some(199)),
                at("c", 0, Some(200)),
                at("d", 0, Some(99)),
                at("e", 0, None),
            ]
        };
        let first = admit(run(), w, &mut seen, &mut timeless);
        let keys: Vec<(String, u32)> = first
            .fresh
            .iter()
            .map(|e| (e.signature.clone(), e.event_index))
            .collect();
        assert_eq!(
            keys,
            vec![("a".into(), 0), ("a".into(), 1), ("b".into(), 0)]
        );
        assert_eq!(
            (first.in_window, first.duplicates, first.timeless),
            (3, 0, 1)
        );
        let again = admit(run(), w, &mut seen, &mut timeless);
        assert!(again.fresh.is_empty());
        assert_eq!(
            (again.in_window, again.duplicates, again.timeless),
            (3, 3, 0)
        );
        assert_eq!(
            extract_with(
                &[],
                w,
                &BTreeSet::new(),
                &|| Err("JOB_CANCELLED".into()),
                u64::MAX
            )
            .unwrap_err(),
            "JOB_CANCELLED"
        );
        assert_eq!(
            extract(&[], w, &BTreeSet::new())
                .unwrap()
                .liquidations
                .len(),
            0
        );
    }

    /// Review (CTRL-10 re-check): every event of a multi-event transaction carries the
    /// transaction's attributes, not only the first one.
    #[test]
    fn h0_every_event_of_a_transaction_carries_its_attributes() {
        let at = |index: u32| {
            let mut e = event("multi", LiquidationType::Receivership, "w", None, false);
            e.event_index = index;
            e
        };
        let a = TxAttributes {
            version: Some("v0".into()),
            cpi_known: true,
            ..Default::default()
        };
        let attrs = BTreeMap::from([("multi".to_string(), a.clone())]);
        let mut held = Held {
            bytes: 0,
            max: u64::MAX,
        };
        let mut out = Vec::new();
        attach(
            vec![
                at(0),
                at(1),
                event("other", LiquidationType::Classic, "w", None, false),
            ],
            &attrs,
            "run",
            &BTreeSet::new(),
            &mut held,
            &mut out,
        )
        .unwrap();
        let txs: Vec<Option<&TxAttributes>> = out.iter().map(|l| l.tx.as_ref()).collect();
        assert_eq!(txs, vec![Some(&a), Some(&a), None]);
        assert_eq!(out[0].execution_class, out[1].execution_class);
        let mut tight = Held { bytes: 0, max: 1 };
        assert_eq!(
            attach(
                vec![at(0)],
                &attrs,
                "run",
                &BTreeSet::new(),
                &mut tight,
                &mut Vec::new()
            ),
            Err("H0_RAM_BUDGET".to_string())
        );
    }

    /// H0: a window is the given days before its end; zero days or one before the epoch is
    /// refused; the runs cover the union of their capture windows inside it.
    #[test]
    fn h0_windows_are_days_before_their_end() {
        assert_eq!(
            Window::days_before(10 * 86_400, 3).unwrap(),
            Window {
                start_unix_s: 7 * 86_400,
                end_unix_s: 10 * 86_400
            }
        );
        assert!(Window::days_before(10, 0).is_err());
        assert!(Window::days_before(i64::MIN + 1, 1).is_err());
        let w = Window::days_before(100 * 86_400, 30).unwrap();
        assert!(
            w.contains(Some(70 * 86_400)) && !w.contains(Some(100 * 86_400)) && !w.contains(None)
        );
        let w = Window {
            start_unix_s: 1,
            end_unix_s: 35,
        };
        assert_eq!(
            covered_seconds(&[(0, 10), (5, 20), (30, 40), (-5, 2), (50, 60)], w),
            19 + 5
        );
        assert_eq!(covered_seconds(&[], w), 0);
    }

    /// H0: the two record layouts round-trip and are pinned (a change is a deliberate schema bump).
    #[test]
    fn h0_record_layouts_round_trip_and_are_pinned() {
        use crate::analytics::payload;
        let mut e = event("pinned", LiquidationType::Classic, "alpha", Some(1.0), true);
        e.visible_payments = Some(vec![]);
        let l = H0Liquidation {
            run_id: "run".into(),
            event: e,
            tx: Some(TxAttributes {
                version: Some("v0".into()),
                serialized_bytes: Some(900),
                account_count: Some(20),
                lookup_tables: 1,
                instruction_count: 5,
                compute_units_consumed: Some(150_000),
                compute_unit_limit: Some(400_000),
                compute_unit_price_micro_lamports: Some(1),
                cpi_known: true,
                swap_programs: vec![p::JUP_SWAP_PROGRAM_ID.into()],
                integration_withdrawals: vec!["KAMINO".into()],
                refresh_instructions: 1,
                oracle_updates: 1,
                program_sequence: vec![p::PROGRAM_ID_MAINNET.into()],
            }),
            jito_tip_lamports: Some(0),
            premium_bps: Some(500),
            edge_usd_micro: Some(1_000_000),
            edge_basis: Some("NET".into()),
            visible_cost_lamports: Some(15_000),
            execution_class: "CLASSIC:SWAP:NO_VISIBLE_TIP".into(),
        };
        let e = Extract {
            liquidations: vec![l.clone()],
            runs: vec![RunCoverage {
                run_id: "run".into(),
                status: "CANONICAL_VERIFIED".into(),
                reason: None,
                events: 1,
                capture_unix_s: Some((0, 60)),
                failed_attempts: 1,
            }],
            duplicates: 0,
            without_block_time: 0,
            failed_attempts: 1,
            failed_attempt_actors: 1,
            covered_seconds: 60,
        };
        let r = report(
            Window {
                start_unix_s: 0,
                end_unix_s: 2_000,
            },
            &e,
            1,
            Some(1),
            1,
            1,
        );
        let pin =
            |kind, schema, bytes: &[u8]| (kind, schema, solana_sdk::hash::hash(bytes).to_string());
        let lb = payload::encode(&l).unwrap();
        assert_eq!(
            payload::decode::<H0Liquidation>(H0Liquidation::KIND, H0Liquidation::SCHEMA, &lb)
                .unwrap(),
            l
        );
        let rb = payload::encode(&r).unwrap();
        assert_eq!(
            payload::decode::<H0Report>(H0Report::KIND, H0Report::SCHEMA, &rb).unwrap(),
            r
        );
        assert_eq!(
            [
                pin(H0Liquidation::KIND, H0Liquidation::SCHEMA, &lb),
                pin(H0Report::KIND, H0Report::SCHEMA, &rb)
            ],
            [
                (
                    362,
                    2,
                    "7mBZKV2aUvnZ6twbLmNWKJq7zzwcjCE6Ew8KWaZ9Mjju".to_string()
                ),
                (
                    363,
                    2,
                    "92boB86YNXA7ornUBAthVwJKWBp73fnN1LCXqtKc67g7".to_string()
                )
            ]
        );
    }

    /// Review (CTRL-10 re-check): the frames 9a41e808 wrote stay readable — kind 360 decodes the
    /// liquidation layout of that commit exactly (its pinned hash), kind 361 its report; neither
    /// is the reviewed layout (kinds 362/363).
    #[test]
    fn h0_pre_review_frames_stay_readable() {
        use crate::analytics::{h0_pre as pre, payload};
        let mut e = event("pinned", LiquidationType::Classic, "alpha", Some(1.0), true);
        e.visible_payments = Some(vec![]);
        let l = pre::H0Liquidation {
            run_id: "run".into(),
            event: e,
            tx: Some(pre::TxAttributes {
                version: Some("v0".into()),
                serialized_bytes: Some(900),
                account_count: Some(20),
                lookup_tables: 1,
                instruction_count: 5,
                compute_units_consumed: Some(150_000),
                compute_unit_limit: Some(400_000),
                compute_unit_price_micro_lamports: Some(1),
                swap_programs: vec![p::JUP_SWAP_PROGRAM_ID.into()],
                integration_instructions: 1,
                oracle_instructions: 1,
                program_sequence: vec![p::PROGRAM_ID_MAINNET.into()],
            }),
            jito_tip_lamports: Some(0),
            premium_bps: Some(500),
            execution_class: "CLASSIC:SWAP:UNTIPPED".into(),
        };
        let lb = payload::encode(&l).unwrap();
        assert_eq!(
            (
                pre::H0Liquidation::KIND,
                pre::H0Liquidation::SCHEMA,
                solana_sdk::hash::hash(&lb).to_string()
            ),
            (
                360,
                2,
                "EYqS5VFvFKnB1zLPqdnKUz1NxqjaimxgbhyRvDovdTww".to_string()
            )
        );
        assert_eq!(
            payload::decode::<pre::H0Liquidation>(360, 2, &lb).unwrap(),
            l
        );
        assert!(payload::decode::<H0Liquidation>(360, 2, &lb).is_err());
        let r = pre::H0Report {
            window: pre::Window {
                start_unix_s: 0,
                end_unix_s: 2_000,
            },
            runs: vec![pre::RunCoverage {
                run_id: "run".into(),
                status: "CANONICAL_VERIFIED".into(),
                reason: None,
                events: 1,
            }],
            liquidations: 1,
            duplicates: 0,
            by_type: vec![("CLASSIC".into(), 1)],
            by_execution_class: vec![("CLASSIC:SWAP:UNTIPPED".into(), 1)],
            by_tx_version: vec![("v0".into(), 1)],
            winners: vec![pre::WinnerStats {
                winner: "alpha".into(),
                wins: 1,
                share_bps: 10_000,
                shapes: 1,
            }],
            top_winner_share_bps: Some(10_000),
            winner_hhi_bps: Some(10_000),
            tipped: 0,
            tip_unobservable: 0,
            tip_unknown: 0,
            tip_lamports: pre::Percentiles::default(),
            priority_fee_lamports: pre::Percentiles::default(),
            compute_units: pre::Percentiles::default(),
            serialized_bytes: pre::Percentiles::default(),
            premium_bps: pre::Percentiles::default(),
            bands: vec![pre::MinProfitBand {
                winner: "alpha".into(),
                wins: 1,
                complete: 1,
                incomplete: 0,
                upper_bound_usd_micro: Some(1_000_000),
                p10_usd_micro: Some(1_000_000),
                p50_usd_micro: Some(1_000_000),
                uncertainty: vec!["UPPER_BOUND_ONLY".into()],
            }],
            recurring_min_wins: 1,
            our_min_profit_usd_micro: Some(1),
            sample_status: "SUFFICIENT".into(),
            tags: "BLOCKED".into(),
        };
        let rb = payload::encode(&r).unwrap();
        assert_eq!(payload::decode::<pre::H0Report>(361, 2, &rb).unwrap(), r);
        assert!(payload::decode::<H0Report>(361, 2, &rb).is_err());
    }

    /// H0: the job's inputs — the Jito tip accounts from the environment (an invalid key is
    /// refused, never skipped; none: tips unknown), the registry's minimum profit and recurring
    /// wins (profile < environment < config file); the job over a project without runs reports
    /// an empty, undecided window and writes its output.
    #[test]
    fn h0_job_inputs_come_from_the_environment() {
        let tip = Pubkey::new_unique();
        let env = |vars: Vec<(&'static str, String)>| {
            move |k: &str| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone())
        };
        assert_eq!(
            tip_accounts(&env(vec![("P0_JITO_TIP_ACCOUNTS", format!(" {tip} ,"))])).unwrap(),
            [tip].into()
        );
        assert!(tip_accounts(&env(vec![])).unwrap().is_empty());
        assert_eq!(
            tip_accounts(&env(vec![("P0_JITO_TIP_ACCOUNTS", "not-a-key".into())])).unwrap_err(),
            "P0_JITO_TIP_ACCOUNTS_INVALID: not-a-key"
        );
        let project = std::env::temp_dir().join(format!("p0-h0-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let (r, path) = run_job(
            &project,
            "j",
            100 * 86_400,
            30,
            None,
            u64::MAX,
            &env(vec![
                ("P0_MIN_PROFIT", "2500000".into()),
                ("P0_H0_RECURRING_MIN_WINS", "5".into()),
                ("P0_JITO_TIP_ACCOUNTS", tip.to_string()),
            ]),
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(
            (
                r.liquidations,
                r.our_min_profit_usd_micro,
                r.recurring_min_wins,
                r.tip_accounts,
                r.sample_status.as_str()
            ),
            (0, 2_500_000, 5, 1, "MIN_SAMPLE_UNDECIDED")
        );
        assert_eq!(r.window, Window::days_before(100 * 86_400, 30).unwrap());
        assert!(path.is_file() && path.starts_with(output_dir(&project, "j")));
        let defaults = darkbot_solana::runtime_config::Limits::default();
        let (r, _) = run_job(
            &project,
            "d",
            100 * 86_400,
            30,
            None,
            u64::MAX,
            &env(vec![]),
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(
            (r.our_min_profit_usd_micro, r.recurring_min_wins),
            (
                defaults.min_profit_usd_micro,
                defaults.h0_recurring_min_wins
            )
        );
        let err = run_job(
            &project,
            "k",
            100 * 86_400,
            30,
            None,
            u64::MAX,
            &env(vec![("P0_MIN_PROFIT", "x".into())]),
            &|| Ok(()),
        )
        .unwrap_err();
        assert_eq!(
            err,
            "invalid numeric environment field min_profit_usd_micro"
        );
        assert!(!output_dir(&project, "k").exists());
        let _ = std::fs::remove_dir_all(project);
    }
}
