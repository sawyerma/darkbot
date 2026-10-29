//! H0 (PHASENPLAN Track H0 — H0-01 extractor, H0-03 execution classes, H0-05 implied competitor
//! minimum-profit bands, H0-06 report): historical liquidation intelligence over the saved runs of
//! a window. Each run's selected verified result (`report::select_result`, the result a report may
//! use) yields its finalized canonical events (`market_truth`) and their decoded transactions
//! (`report::verified_events`); across runs every event is taken once — by signature and event
//! index — with the attributes of its transaction (version, serialized bytes, accounts, lookup
//! tables, compute, priority fee, visible Jito tips, swap, integration and oracle instructions).
//! Nothing is read from the network. A band is an inference from observed net edges, never a
//! competitor's configuration: each win bounds its winner's threshold from above, and a win whose
//! costs are incomplete is named as such instead of narrowing the band. Tags (H0-04) need the
//! 0.1.12 deployment and stay BLOCKED.
use super::journal::{self, kind};
use super::payload::Payload;
use crate::market::observer::rows::MarketTransaction;
use crate::market::observer::truth::{LiquidationType, TruthEvent};
use crate::market::observer::verification::StrictStatus;
use crate::winner::ChainTx;
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

/// What one saved run contributed: its selected result's status and its events in the window
/// (before the cross-run de-duplication), or why it contributed nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCoverage {
    pub run_id: String,
    /// CANONICAL_VERIFIED | HISTORICAL_INCOMPLETE (events up to the last verified batch) |
    /// NO_VERIFIED_RESULT | UNREADABLE.
    pub status: String,
    pub reason: Option<String>,
    pub events: u64,
}

/// The attributes of a liquidation's transaction (H0-01); `None` where the run kept no decoded
/// transaction or the attribute is not derivable from it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxAttributes {
    pub version: Option<String>,
    /// The exact legacy/v0 wire length (signatures and message); `None` for v1 or an incomplete
    /// message.
    pub serialized_bytes: Option<u64>,
    /// Static and loaded account keys.
    pub account_count: Option<u64>,
    pub lookup_tables: u64,
    pub instruction_count: u64,
    pub compute_units_consumed: Option<u64>,
    pub compute_unit_limit: Option<u32>,
    pub compute_unit_price_micro_lamports: Option<u64>,
    /// Allowlisted swap programs among the top-level instructions.
    pub swap_programs: Vec<String>,
    /// Top-level Kamino/Drift/JupLend instructions (integration refreshes and withdrawals).
    pub integration_instructions: u64,
    /// Top-level Pyth push-oracle instructions (oracle updates in the transaction).
    pub oracle_instructions: u64,
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
    /// `TYPE:ROUTE:CARRIER` (H0-03): ROUTE SWAP | NO_SWAP | UNKNOWN; CARRIER TIPPED | UNTIPPED |
    /// BUNDLE_UNOBSERVABLE (a receivership TX holds no tip: it sits in another bundle TX) |
    /// TIP_UNKNOWN (no tip-account set).
    pub execution_class: String,
}
impl Payload for H0Liquidation {
    const KIND: u16 = kind::H0_LIQUIDATION;
}

/// One winner's share of the window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinnerStats {
    pub winner: String,
    pub wins: u64,
    /// Share of all liquidations of the window (bps).
    pub share_bps: u64,
    /// Distinct transaction shapes (program sequences) the winner used.
    pub shapes: u64,
}

/// H0-05: the implied minimum-profit band of one recurring winner. Every win's observed net edge
/// bounds the winner's threshold from above (it took that edge), so the band is an upper bound:
/// its lowest complete edge, with the 10th percentile and the median for context. Wins whose
/// costs are incomplete (unknown tips, route or fees) are counted, never folded in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinProfitBand {
    pub winner: String,
    pub wins: u64,
    pub complete: u64,
    pub incomplete: u64,
    pub upper_bound_usd_micro: Option<i64>,
    pub p10_usd_micro: Option<i64>,
    pub p50_usd_micro: Option<i64>,
    /// UPPER_BOUND_ONLY (always: the take time is unknown before 0.1.12 tags) and, with
    /// incomplete wins, INCOMPLETE_COSTS (their true edge may be lower).
    pub uncertainty: Vec<String>,
}

/// p50/p95 of one quantity over the window's liquidations that know it (`None`: none does).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Percentiles {
    pub samples: u64,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
}

/// H0-06: the typed report of one extraction — the one source of its text render.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct H0Report {
    pub window: Window,
    pub runs: Vec<RunCoverage>,
    pub liquidations: u64,
    /// Events seen again in another run (taken once).
    pub duplicates: u64,
    pub by_type: Vec<(String, u64)>,
    pub by_execution_class: Vec<(String, u64)>,
    pub by_tx_version: Vec<(String, u64)>,
    /// Winners by wins (descending), then id.
    pub winners: Vec<WinnerStats>,
    pub top_winner_share_bps: Option<u64>,
    /// Herfindahl–Hirschman index of the winners' shares (bps of 10 000²/10 000).
    pub winner_hhi_bps: Option<u64>,
    pub tipped: u64,
    pub tip_unobservable: u64,
    pub tip_unknown: u64,
    pub tip_lamports: Percentiles,
    pub priority_fee_lamports: Percentiles,
    pub compute_units: Percentiles,
    pub serialized_bytes: Percentiles,
    pub premium_bps: Percentiles,
    /// Recurring winners (at least `recurring_min_wins` wins).
    pub bands: Vec<MinProfitBand>,
    pub recurring_min_wins: u64,
    /// The comparison of H0-05: our configured minimum profit (`None`: not given).
    pub our_min_profit_usd_micro: Option<u64>,
    /// SUFFICIENT | SMALL_SAMPLE (below the configured minimum; the plan's longer window applies)
    /// | MIN_SAMPLE_UNDECIDED (no minimum configured: the extension is not decided).
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
}

/// H0-01: the verified liquidations of `runs` (id, directory) whose block time lies in `window`,
/// each event once, with its transaction's attributes; `tips` are the Jito tip accounts (empty:
/// tips unknown).
pub fn extract(
    runs: &[(String, PathBuf)],
    window: Window,
    tips: &BTreeSet<Pubkey>,
) -> Result<Extract, String> {
    extract_with(runs, window, tips, &|| Ok(()))
}

/// `extract` that asks `stop` before every run (a cancel or an exhausted budget ends it).
pub fn extract_with(
    runs: &[(String, PathBuf)],
    window: Window,
    tips: &BTreeSet<Pubkey>,
    stop: &dyn Fn() -> Result<(), String>,
) -> Result<Extract, String> {
    let mut sorted: Vec<&(String, PathBuf)> = runs.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut seen: BTreeMap<(String, u32), ()> = BTreeMap::new();
    let mut out = Extract {
        liquidations: vec![],
        runs: vec![],
        duplicates: 0,
    };
    for (run_id, dir) in sorted {
        stop()?;
        let coverage = |status: &str, reason: Option<String>, events| RunCoverage {
            run_id: run_id.clone(),
            status: status.into(),
            reason,
            events,
        };
        let capture = match crate::market::observer::strict_driver::bind_capture(dir) {
            Ok(c) => c,
            Err(e) => {
                out.runs.push(coverage("UNREADABLE", Some(e), 0));
                continue;
            }
        };
        let selected = match super::report::select_result(&capture, None) {
            Ok(Some(s)) => s,
            Ok(None) => {
                out.runs.push(coverage("NO_VERIFIED_RESULT", None, 0));
                continue;
            }
            Err(e) => {
                out.runs.push(coverage("UNREADABLE", Some(e), 0));
                continue;
            }
        };
        let mut txs: BTreeMap<String, ChainTx> = BTreeMap::new();
        let (events, _) =
            match super::report::verified_events(&selected, &mut |row: &MarketTransaction| {
                if let Some(tx) = row.chain_tx() {
                    txs.entry(row.signature.clone())
                        .or_insert_with(|| tx.clone());
                }
                Ok(())
            }) {
                Ok(v) => v,
                Err(e) => {
                    out.runs.push(coverage("UNREADABLE", Some(e), 0));
                    continue;
                }
            };
        let mut n = 0u64;
        for event in events.into_iter().filter(|e| window.contains(e.block_time)) {
            n += 1;
            if seen
                .insert((event.signature.clone(), event.event_index), ())
                .is_some()
            {
                out.duplicates += 1;
                continue;
            }
            let tx = txs.get(&event.signature);
            out.liquidations.push(liquidation(run_id, event, tx, tips));
        }
        let status = selected.result.status.as_str();
        let reason = (selected.result.status == StrictStatus::HistoricalIncomplete)
            .then(|| selected.result.reason.clone())
            .flatten();
        out.runs.push(coverage(status, reason, n));
    }
    out.liquidations.sort_by(|a, b| {
        (a.event.slot, a.event.transaction_index, a.event.event_index).cmp(&(
            b.event.slot,
            b.event.transaction_index,
            b.event.event_index,
        ))
    });
    Ok(out)
}

fn liquidation(
    run_id: &str,
    event: TruthEvent,
    tx: Option<&ChainTx>,
    tips: &BTreeSet<Pubkey>,
) -> H0Liquidation {
    let attributes = tx.map(attributes);
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
    let route = match &attributes {
        Some(a) if !a.swap_programs.is_empty() => "SWAP",
        Some(_) => "NO_SWAP",
        None => "UNKNOWN",
    };
    let carrier = match (event.liquidation_type, jito_tip_lamports) {
        (_, Some(t)) if t > 0 => "TIPPED",
        (LiquidationType::Receivership, _) => "BUNDLE_UNOBSERVABLE",
        (_, Some(_)) => "UNTIPPED",
        (_, None) => "TIP_UNKNOWN",
    };
    H0Liquidation {
        run_id: run_id.into(),
        execution_class: format!("{}:{route}:{carrier}", event.liquidation_type.as_str()),
        event,
        tx: attributes,
        jito_tip_lamports,
        premium_bps,
    }
}

/// The transaction attributes H0 records (`enrich` for version and compute budget).
pub fn attributes(tx: &ChainTx) -> TxAttributes {
    use darkbot_project0::protocol as p;
    let keys = tx.account_keys().ok();
    let program = |i: u8| keys.as_ref().and_then(|k| k.get(usize::from(i)).copied());
    let is = |key: Option<Pubkey>, ids: &[&str]| {
        key.is_some_and(|k| {
            ids.iter()
                .any(|id| Pubkey::from_str(id).is_ok_and(|p| p == k))
        })
    };
    let sequence: Vec<Option<Pubkey>> = tx
        .message
        .instructions
        .iter()
        .map(|ix| program(ix.program_id_index))
        .collect();
    let enrichment = crate::market::observer::enrich(tx, &Pubkey::default());
    let mut swaps: Vec<String> = sequence
        .iter()
        .flatten()
        .filter(|k| {
            is(
                Some(**k),
                &[p::JUP_SWAP_PROGRAM_ID, p::TITAN_SWAP_PROGRAM_ID],
            )
        })
        .map(ToString::to_string)
        .collect();
    swaps.sort();
    swaps.dedup();
    let count = |ids: &[&str]| sequence.iter().filter(|k| is(**k, ids)).count() as u64;
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
        swap_programs: swaps,
        integration_instructions: count(&[
            p::KAMINO_PROGRAM_ID,
            p::DRIFT_PROGRAM_ID,
            p::JUPLEND_PROGRAM_ID,
        ]),
        oracle_instructions: count(&[p::PYTH_PUSH_ORACLE_ID]),
        program_sequence: sequence
            .iter()
            .map(|k| k.map_or_else(|| "UNRESOLVED".into(), |k| k.to_string()))
            .collect(),
    }
}

/// The exact wire length of a legacy or v0 transaction rebuilt from its decoded message (the
/// signatures and the message as `VersionedTransaction` serializes them); `None` for v1, a
/// missing header or blockhash, or a signature that does not decode.
pub fn wire_len(tx: &ChainTx) -> Option<u64> {
    use darkbot_solana::chain::TxWireVersion;
    use solana_sdk::message::{v0, Message as LegacyMessage, MessageHeader, VersionedMessage};
    let (required, readonly_signed, readonly_unsigned) = tx.message.header?;
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
fn percentile(v: &[u64], q: u64) -> Option<u64> {
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

fn signed_percentile(v: &[i64], q: u64) -> Option<i64> {
    if v.is_empty() {
        return None;
    }
    let rank = (v.len() as u64 * q).div_ceil(100).max(1);
    v.get(usize::try_from(rank - 1).ok()?).copied()
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
/// recurring winner (a band); `min_sample`: the configured minimum sample (`None`: undecided).
pub fn report(
    window: Window,
    e: &Extract,
    recurring_min_wins: u64,
    min_sample: Option<u64>,
    our_min_profit_usd_micro: Option<u64>,
) -> H0Report {
    let l = &e.liquidations;
    let total = l.len() as u64;
    let mut by_winner: BTreeMap<&str, Vec<&H0Liquidation>> = BTreeMap::new();
    for x in l {
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
    let mut bands: Vec<MinProfitBand> = by_winner
        .iter()
        .filter(|(_, wins)| wins.len() as u64 >= recurring_min_wins)
        .map(|(w, wins)| {
            let mut edges: Vec<i64> = wins
                .iter()
                .filter(|x| x.event.winner_net_edge_complete)
                .filter_map(|x| x.event.winner_net_edge_usd.and_then(usd_micro))
                .collect();
            edges.sort_unstable();
            let complete = edges.len() as u64;
            let incomplete = wins.len() as u64 - complete;
            let mut uncertainty = vec!["UPPER_BOUND_ONLY".to_string()];
            if incomplete > 0 {
                uncertainty.push("INCOMPLETE_COSTS".into());
            }
            MinProfitBand {
                winner: (*w).into(),
                wins: wins.len() as u64,
                complete,
                incomplete,
                upper_bound_usd_micro: edges.first().copied(),
                p10_usd_micro: signed_percentile(&edges, 10),
                p50_usd_micro: signed_percentile(&edges, 50),
                uncertainty,
            }
        })
        .collect();
    bands.sort_by(|a, b| b.wins.cmp(&a.wins).then(a.winner.cmp(&b.winner)));
    let tips: Vec<u64> = l
        .iter()
        .filter_map(|x| x.jito_tip_lamports.filter(|t| *t > 0))
        .collect();
    H0Report {
        window,
        runs: e.runs.clone(),
        liquidations: total,
        duplicates: e.duplicates,
        by_type: counts(
            l.iter()
                .map(|x| x.event.liquidation_type.as_str().to_string()),
        ),
        by_execution_class: counts(l.iter().map(|x| x.execution_class.clone())),
        by_tx_version: counts(l.iter().map(|x| {
            x.tx.as_ref()
                .and_then(|t| t.version.clone())
                .unwrap_or_else(|| "UNKNOWN".into())
        })),
        top_winner_share_bps: winners.first().map(|w| w.share_bps),
        winners,
        winner_hhi_bps,
        tipped: tips.len() as u64,
        tip_unobservable: l
            .iter()
            .filter(|x| x.execution_class.ends_with(":BUNDLE_UNOBSERVABLE"))
            .count() as u64,
        tip_unknown: l.iter().filter(|x| x.jito_tip_lamports.is_none()).count() as u64,
        tip_lamports: percentiles(tips),
        priority_fee_lamports: percentiles(
            l.iter()
                .filter_map(|x| x.event.priority_fee_lamports)
                .collect(),
        ),
        compute_units: percentiles(
            l.iter()
                .filter_map(|x| x.tx.as_ref().and_then(|t| t.compute_units_consumed))
                .collect(),
        ),
        serialized_bytes: percentiles(
            l.iter()
                .filter_map(|x| x.tx.as_ref().and_then(|t| t.serialized_bytes))
                .collect(),
        ),
        premium_bps: percentiles(
            l.iter()
                .filter_map(|x| x.premium_bps.and_then(|p| u64::try_from(p).ok()))
                .collect(),
        ),
        bands,
        recurring_min_wins,
        our_min_profit_usd_micro,
        sample_status: match min_sample {
            None => "MIN_SAMPLE_UNDECIDED".into(),
            Some(n) if total < n => "SMALL_SAMPLE".into(),
            Some(_) => "SUFFICIENT".into(),
        },
        tags: "BLOCKED: H0-04 needs the 0.1.12 deployment".into(),
    }
}

/// H0-06: the report as text (`H0_LIQUIDATIONS.md`), rendered only from the typed report.
pub fn render(r: &H0Report) -> String {
    let opt = |v: Option<u64>| v.map_or_else(|| "—".to_string(), |v| v.to_string());
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
    let mut s = String::new();
    s += "# H0 — historical liquidations\n\n";
    s += &format!(
        "Window: block time {} … {} (unix s). Liquidations: {} (duplicates across runs: {}). Sample: {}. Tags: {}.\n\n",
        r.window.start_unix_s, r.window.end_unix_s, r.liquidations, r.duplicates, r.sample_status, r.tags
    );
    s += "## Runs\n\n| Run | Status | Events | Reason |\n|---|---|---|---|\n";
    for c in &r.runs {
        s += &format!(
            "| {} | {} | {} | {} |\n",
            c.run_id,
            c.status,
            c.events,
            c.reason.as_deref().unwrap_or("")
        );
    }
    for (title, rows) in [
        ("Liquidation types", &r.by_type),
        ("Execution classes", &r.by_execution_class),
        ("Transaction versions", &r.by_tx_version),
    ] {
        s += &format!("\n## {title}\n\n| Class | Count |\n|---|---|\n");
        for (k, n) in rows {
            s += &format!("| {k} | {n} |\n");
        }
    }
    s += &format!(
        "\n## Winners\n\nTop winner share: {}. Concentration (HHI): {}.\n\n| Winner | Wins | Share | Shapes |\n|---|---|---|---|\n",
        bps(r.top_winner_share_bps),
        opt(r.winner_hhi_bps)
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
        "\n## Costs and shape (p50 / p95)\n\n| Quantity | p50 / p95 |\n|---|---|\n| Jito tip (lamports, tipped only) | {} |\n| Priority fee (lamports) | {} |\n| Compute units | {} |\n| Serialized bytes | {} |\n| Premium (bps) | {} |\n\nTipped: {}; tip in another bundle TX (unobservable): {}; tip unknown (no tip accounts): {}.\n",
        pct(&r.tip_lamports),
        pct(&r.priority_fee_lamports),
        pct(&r.compute_units),
        pct(&r.serialized_bytes),
        pct(&r.premium_bps),
        r.tipped,
        r.tip_unobservable,
        r.tip_unknown
    );
    s += &format!(
        "\n## Implied competitor minimum profit (H0-05, inference)\n\nRecurring: at least {} wins. Our minimum profit: {} USD. A band bounds the winner's threshold from above; it is never its configuration.\n\n| Winner | Wins | Complete | Incomplete | Upper bound (USD) | p10 | p50 | Uncertainty |\n|---|---|---|---|---|---|---|---|\n",
        r.recurring_min_wins,
        usd(r.our_min_profit_usd_micro.and_then(|v| i64::try_from(v).ok()))
    );
    for b in &r.bands {
        s += &format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            b.winner,
            b.wins,
            b.complete,
            b.incomplete,
            usd(b.upper_bound_usd_micro),
            usd(b.p10_usd_micro),
            usd(b.p50_usd_micro),
            b.uncertainty.join(", ")
        );
    }
    s
}

/// The H0 output of one extraction under `dir`: every liquidation, then the report, sealed and
/// published whole (staged, then linked; an existing output is never overwritten), and its text.
pub fn write(dir: &Path, e: &Extract, r: &H0Report) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("H0_IO: {e}"))?;
    let path = dir.join(H0_JOURNAL);
    if path.exists() {
        return Err("H0_OUTPUT_EXISTS".into());
    }
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
    std::fs::write(dir.join(H0_RENDER), render(r)).map_err(|e| format!("H0_IO: {e}"))?;
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
/// `window_days` before `end_unix_s` — the extended window when a configured minimum sample is
/// not reached — its report and output under `output_dir`. `stop` ends it between runs.
#[allow(clippy::too_many_arguments)]
pub fn run_job(
    project: &Path,
    job_id: &str,
    end_unix_s: i64,
    window_days: u64,
    min_sample: Option<u64>,
    limits: &darkbot_solana::runtime_config::Limits,
    env: &dyn Fn(&str) -> Option<String>,
    stop: &dyn Fn() -> Result<(), String>,
) -> Result<(H0Report, PathBuf), String> {
    let tips = tip_accounts(env)?;
    let our_min_profit = match env("P0_MIN_PROFIT") {
        None => None,
        Some(v) => Some(
            v.trim()
                .parse::<u64>()
                .map_err(|_| format!("P0_MIN_PROFIT_INVALID: {v}"))?,
        ),
    };
    let runs: Vec<(String, PathBuf)> = crate::run_layout::runs(project)
        .into_iter()
        .map(|r| (format!("{}/{}", r.root, r.run_id), r.path))
        .collect();
    let mut window = Window::days_before(end_unix_s, window_days)?;
    let mut e = extract_with(&runs, window, &tips, stop)?;
    if min_sample.is_some_and(|n| (e.liquidations.len() as u64) < n)
        && limits.h0_extended_window_days > window_days
    {
        window = Window::days_before(end_unix_s, limits.h0_extended_window_days)?;
        e = extract_with(&runs, window, &tips, stop)?;
    }
    let r = report(
        window,
        &e,
        limits.h0_recurring_min_wins,
        min_sample,
        our_min_profit,
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
    /// with a lookup table, legacy); v1, a missing blockhash or header is unknown, never guessed.
    #[test]
    fn h0_wire_len_is_the_exact_serialized_length() {
        let (tx, chain) = v0_pair(&[Pubkey::new_unique()], vec![vec![1, 2, 3]]);
        let exact = darkbot_solana::submit::serialize_tx(&tx).unwrap().len() as u64;
        assert_eq!(wire_len(&chain), Some(exact));
        let mut legacy = chain.clone();
        legacy.version = Some(TxWireVersion::Legacy);
        legacy.message.address_table_lookups = None;
        assert!(
            wire_len(&legacy).is_some_and(|n| n < exact),
            "no lookup table bytes"
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

    /// H0-01: compute budget, swap, integration and oracle instructions and the program shape.
    #[test]
    fn h0_attributes_name_compute_swaps_integrations_oracles_and_the_shape() {
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
        let (_, chain) = v0_pair(
            &programs,
            vec![limit, price, vec![9], vec![9], vec![9], vec![9]],
        );
        let a = attributes(&chain);
        assert_eq!(a.version.as_deref(), Some("v0"));
        assert_eq!(
            (a.compute_unit_limit, a.compute_unit_price_micro_lamports),
            (Some(400_000), Some(5_000))
        );
        assert_eq!(a.compute_units_consumed, Some(180_000));
        assert_eq!(a.swap_programs, vec![p::JUP_SWAP_PROGRAM_ID.to_string()]);
        assert_eq!((a.integration_instructions, a.oracle_instructions), (1, 1));
        assert_eq!((a.lookup_tables, a.instruction_count), (1, 6));
        assert_eq!(a.account_count, Some(8 + 3), "static and loaded keys");
        assert_eq!(a.program_sequence.len(), 6);
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

    /// H0-03: the execution class names the route and how a tip is carried; a receivership TX
    /// never holds its tip (another bundle TX), and without a tip-account set tips are unknown.
    #[test]
    fn h0_execution_classes_name_route_and_tip_carrier() {
        let tip = Pubkey::new_unique();
        let tips: BTreeSet<Pubkey> = [tip].into();
        let (_, swap_tx) = v0_pair(&[key(p::JUP_SWAP_PROGRAM_ID)], vec![vec![9]]);
        let (_, plain_tx) = v0_pair(&[key(p::PROGRAM_ID_MAINNET)], vec![vec![9]]);
        let classic = tipped(
            event("a", LiquidationType::Classic, "w", None, false),
            &tip,
            7_000,
        );
        let x = liquidation("run", classic.clone(), Some(&swap_tx), &tips);
        assert_eq!(x.execution_class, "CLASSIC:SWAP:TIPPED");
        assert_eq!(
            (x.jito_tip_lamports, x.premium_bps),
            (Some(7_000), Some(500))
        );
        let other = tipped(
            event("b", LiquidationType::Classic, "w", None, false),
            &Pubkey::new_unique(),
            9_000,
        );
        let x = liquidation("run", other, Some(&plain_tx), &tips);
        assert_eq!(
            (x.execution_class.as_str(), x.jito_tip_lamports),
            ("CLASSIC:NO_SWAP:UNTIPPED", Some(0))
        );
        let x = liquidation(
            "run",
            event("c", LiquidationType::Receivership, "w", None, false),
            Some(&plain_tx),
            &tips,
        );
        assert_eq!(
            x.execution_class,
            "RECEIVERSHIP:NO_SWAP:BUNDLE_UNOBSERVABLE"
        );
        let x = liquidation("run", classic, None, &BTreeSet::new());
        assert_eq!(
            (x.execution_class.as_str(), x.jito_tip_lamports),
            ("CLASSIC:UNKNOWN:TIP_UNKNOWN", None)
        );
        let mut unknown = event("d", LiquidationType::Classic, "w", None, false);
        unknown.repaid_value_usd = None;
        assert_eq!(liquidation("run", unknown, None, &tips).premium_bps, None);
    }

    /// H0-05/H0-06: winners by wins with shares and concentration, tips and costs as p50/p95,
    /// a band per recurring winner bounding its threshold from above by its lowest complete
    /// edge (incomplete wins counted and named, never folded in), and the sample status.
    #[test]
    fn h0_report_aggregates_winners_tips_costs_and_upper_bound_bands() {
        let tip = Pubkey::new_unique();
        let tips: BTreeSet<Pubkey> = [tip].into();
        let liquidations = vec![
            liquidation(
                "r1",
                tipped(
                    event("1", LiquidationType::Classic, "alpha", Some(3.0), true),
                    &tip,
                    10_000,
                ),
                None,
                &tips,
            ),
            liquidation(
                "r1",
                tipped(
                    event("2", LiquidationType::Classic, "alpha", Some(1.5), true),
                    &tip,
                    30_000,
                ),
                None,
                &tips,
            ),
            liquidation(
                "r1",
                event("3", LiquidationType::Classic, "alpha", Some(0.2), false),
                None,
                &tips,
            ),
            liquidation(
                "r2",
                event("4", LiquidationType::Receivership, "beta", Some(9.0), true),
                None,
                &tips,
            ),
        ];
        let e = Extract {
            liquidations,
            runs: vec![],
            duplicates: 2,
        };
        let w = Window {
            start_unix_s: 0,
            end_unix_s: 2_000,
        };
        let r = report(w, &e, 2, None, Some(1_000_000));
        assert_eq!((r.liquidations, r.duplicates), (4, 2));
        assert_eq!(r.winners[0].winner, "alpha");
        assert_eq!((r.winners[0].wins, r.winners[0].share_bps), (3, 7_500));
        assert_eq!(r.top_winner_share_bps, Some(7_500));
        assert_eq!(
            r.winner_hhi_bps,
            Some((7_500 * 7_500 + 2_500 * 2_500) / 10_000)
        );
        assert_eq!((r.tipped, r.tip_unobservable, r.tip_unknown), (2, 1, 0));
        assert_eq!(
            (r.tip_lamports.p50, r.tip_lamports.p95),
            (Some(10_000), Some(30_000))
        );
        assert_eq!(r.premium_bps.p50, Some(500));
        assert_eq!(r.bands.len(), 1, "beta has one win: not recurring");
        let b = &r.bands[0];
        assert_eq!((b.wins, b.complete, b.incomplete), (3, 2, 1));
        assert_eq!(
            b.upper_bound_usd_micro,
            Some(1_500_000),
            "the incomplete 0.2 is not folded in"
        );
        assert_eq!(b.uncertainty, vec!["UPPER_BOUND_ONLY", "INCOMPLETE_COSTS"]);
        assert_eq!(r.sample_status, "MIN_SAMPLE_UNDECIDED");
        assert_eq!(
            report(w, &e, 2, Some(5), None).sample_status,
            "SMALL_SAMPLE"
        );
        assert_eq!(report(w, &e, 2, Some(4), None).sample_status, "SUFFICIENT");
        assert_eq!(report(w, &e, 1, None, None).bands.len(), 2);
        assert!(r.tags.starts_with("BLOCKED"));
        let text = render(&r);
        for part in [
            "Liquidations: 4 (duplicates across runs: 2)",
            "| alpha | 3 | 75.00 % |",
            "| alpha | 3 | 2 | 1 | 1.500000 |",
            "UPPER_BOUND_ONLY, INCOMPLETE_COSTS",
            "Our minimum profit: 1.000000 USD",
            "| CLASSIC:UNKNOWN:UNTIPPED | 1 |",
        ] {
            assert!(text.contains(part), "{part}\n{text}");
        }
        let empty = report(
            w,
            &Extract {
                liquidations: vec![],
                runs: vec![],
                duplicates: 0,
            },
            2,
            None,
            None,
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

    /// H0: a window is the given days before its end; zero days or one before the epoch is refused.
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
                swap_programs: vec![p::JUP_SWAP_PROGRAM_ID.into()],
                integration_instructions: 1,
                oracle_instructions: 1,
                program_sequence: vec![p::PROGRAM_ID_MAINNET.into()],
            }),
            jito_tip_lamports: Some(0),
            premium_bps: Some(500),
            execution_class: "CLASSIC:SWAP:UNTIPPED".into(),
        };
        let e = Extract {
            liquidations: vec![l.clone()],
            runs: vec![RunCoverage {
                run_id: "run".into(),
                status: "CANONICAL_VERIFIED".into(),
                reason: None,
                events: 1,
            }],
            duplicates: 0,
        };
        let r = report(
            Window {
                start_unix_s: 0,
                end_unix_s: 2_000,
            },
            &e,
            1,
            Some(1),
            Some(1),
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
                    360,
                    2,
                    "EYqS5VFvFKnB1zLPqdnKUz1NxqjaimxgbhyRvDovdTww".to_string()
                ),
                (
                    361,
                    2,
                    "HVNyrSQYk6THYPh8QgcUKBqNJmPsmxD8irsxP8Fz9mDB".to_string()
                )
            ]
        );
    }

    /// H0: the job's inputs come from the environment — the Jito tip accounts (an invalid key is
    /// refused, never skipped; none: tips unknown) and our minimum profit (`P0_MIN_PROFIT`); the
    /// job over a project without runs reports an empty, undecided window and writes its output.
    #[test]
    fn h0_job_inputs_come_from_the_environment() {
        let tip = Pubkey::new_unique();
        let env = |tips: String, min: Option<&'static str>| {
            move |k: &str| match k {
                "P0_JITO_TIP_ACCOUNTS" => Some(tips.clone()),
                "P0_MIN_PROFIT" => min.map(str::to_string),
                _ => None,
            }
        };
        assert_eq!(
            tip_accounts(&env(format!(" {tip} ,"), None)).unwrap(),
            [tip].into()
        );
        assert!(tip_accounts(&env(String::new(), None)).unwrap().is_empty());
        assert_eq!(
            tip_accounts(&env("not-a-key".into(), None)).unwrap_err(),
            "P0_JITO_TIP_ACCOUNTS_INVALID: not-a-key"
        );
        let project = std::env::temp_dir().join(format!("p0-h0-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let limits = darkbot_solana::runtime_config::Limits::default();
        let (r, path) = run_job(
            &project,
            "j",
            100 * 86_400,
            30,
            None,
            &limits,
            &env(String::new(), Some("2500000")),
            &|| Ok(()),
        )
        .unwrap();
        assert_eq!(
            (
                r.liquidations,
                r.our_min_profit_usd_micro,
                r.sample_status.as_str()
            ),
            (0, Some(2_500_000), "MIN_SAMPLE_UNDECIDED")
        );
        assert_eq!(r.window, Window::days_before(100 * 86_400, 30).unwrap());
        assert!(path.is_file() && path.starts_with(output_dir(&project, "j")));
        assert_eq!(
            run_job(
                &project,
                "k",
                100 * 86_400,
                30,
                None,
                &limits,
                &env(String::new(), Some("x")),
                &|| Ok(())
            )
            .unwrap_err(),
            "P0_MIN_PROFIT_INVALID: x"
        );
        let _ = std::fs::remove_dir_all(project);
    }
}
