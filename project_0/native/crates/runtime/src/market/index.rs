//! DEC-08 (PHASENPLAN DEC-08, ENTSCHEIDUNGEN §8): the native INDEX verifier. The finalized
//! signature index of the program over the capture window, read through the one
//! signature-history reader (`market::history`), every transaction of the window fetched and
//! decoded by the one decoder (`chain::decode_transaction`) and classified by the one classifier
//! (`truth::classify`), then compared with the capture's own records (live transactions, the
//! observer's and the canonical liquidation rows, live payloads, slot-boundary coverage). The
//! rules are the pinned Python verifier's (`audit_market_quick.py`), ported one to one.
//!
//! Every read goes through the budgeted chain client of the verifiers (one wire edge, typed
//! cache of every read, call/byte/time/disk/RAM budgets); 429, gaps, pruning and exhausted
//! budgets end an attempt `HISTORICAL_INCOMPLETE` with its reason, and the cache is the
//! checkpoint: a resumed attempt replays every page and transaction it holds without the network.
//! `INDEX_VERIFIED` is conditional index-scope evidence: never `CANONICAL_VERIFIED`, never the
//! C0-E1 master.
use super::reconcile::{ChainAnswer, ChainRead};
use super::truth::{self, LiquidationType, NativeEvidence, TruthEvent};
use super::verification::{Client, Job};
use super::{decode_receiver_events, LogEventRef};
use crate::analytics::journal::{self, kind, source};
use crate::analytics::payload::{self, Payload};
use crate::market::history::{PageEnd, SignaturePager};
use crate::market::winner::WinnerIdentity;
use darkbot_solana::chain::{ChainTx, SignatureEntry, TxStatus};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

pub const SCHEMA: &str = "Project0/IndexVerification/1";
const NS: i128 = 1_000_000_000;
/// The four liquidation kinds in report order.
pub const KINDS: [LiquidationType; 4] = [
    LiquidationType::Classic,
    LiquidationType::Receivership,
    LiquidationType::ForcedDeleverage,
    LiquidationType::BankruptcySettlement,
];

fn kind_slot(k: LiquidationType) -> usize {
    KINDS.iter().position(|x| *x == k).unwrap_or(0)
}

/// Where a capture-side event comes from (the Python verifier's sources).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum BotSource {
    /// The observer's receivership row with a live receipt.
    Live,
    /// The observer's row recovered from a finalized block (no live receipt).
    Backfill,
    /// The capture's canonical full-slot classification.
    Canonical,
    /// Decoded from the payload of a live transaction.
    LivePayload,
}
impl BotSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::Live => "BOT_LIVE",
            Self::Backfill => "BOT_BACKFILL",
            Self::Canonical => "BOT_CANONICAL",
            Self::LivePayload => "LIVE_PAYLOAD",
        }
    }
    /// The canonical event index counts every kind; the observer's receivership rows count
    /// receiverships only.
    fn all_kind_index(self) -> bool {
        matches!(self, Self::Canonical | Self::LivePayload)
    }
}

/// One capture-side liquidation event of a transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct BotEvent {
    pub source: BotSource,
    pub index: u32,
    pub kind: LiquidationType,
    pub target: String,
    pub actor: String,
    pub seized: Option<f64>,
    pub repaid: Option<f64>,
    pub protocol_fee: Option<u64>,
    pub native: Option<NativeEvidence>,
}

/// The capture's live slot-boundary coverage and its proven outages (the Python verifier's rule:
/// a counted gap opens an outage, a reconnect acknowledges it, the next slot boundary closes it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    pub first_live_ns: Option<i128>,
    pub last_live_ns: Option<i128>,
    pub last_live_slot: Option<u64>,
    pub gap_open: Option<i128>,
    reconnect_ack: Option<i128>,
    gaps: Vec<(i128, i128)>,
}
impl Coverage {
    pub fn slot_boundary(&mut self, at_ns: i128, slot: u64) {
        self.first_live_ns.get_or_insert(at_ns);
        self.last_live_ns = Some(at_ns);
        self.last_live_slot = Some(slot);
        if let (Some(open), Some(ack)) = (self.gap_open, self.reconnect_ack) {
            if at_ns >= ack {
                self.gaps.push((open, at_ns));
                self.gap_open = None;
                self.reconnect_ack = None;
            }
        }
    }
    pub fn gap(&mut self, at_ns: i128) {
        self.gap_open.get_or_insert(at_ns);
        self.reconnect_ack = None;
    }
    pub fn reconnected(&mut self, at_ns: i128) {
        self.reconnect_ack = Some(at_ns);
    }
    /// The live coverage at a chain second (`coverage_at`): block times are coarse chain
    /// evidence, so one second of guard either side.
    pub fn at(&self, block_time: Option<i64>, boundary: bool) -> &'static str {
        let Some(bt) = block_time.filter(|_| !boundary) else {
            return "TIME_BOUNDARY_UNKNOWN";
        };
        let at = i128::from(bt) * NS;
        if self.gap_open.is_some_and(|open| at > open + NS)
            || self.gaps.iter().any(|(s, e)| *s < at - NS && *e > at + NS)
        {
            return "PROVEN_WEBSOCKET_OUTAGE_INTERVAL";
        }
        if let (Some(first), Some(last)) = (self.first_live_ns, self.last_live_ns) {
            if first + NS < at && at < last - NS {
                if self
                    .gaps
                    .iter()
                    .any(|(s, e)| *s <= at + NS && *e >= at - NS)
                {
                    return "TIME_BOUNDARY_UNKNOWN";
                }
                return "WITHIN_OBSERVED_SLOT_INTERVAL";
            }
        }
        "LIVE_COVERAGE_UNPROVEN"
    }
}

/// The capture's own records the index is compared with.
#[derive(Clone, Debug, Default)]
pub struct BotEvidence {
    pub live: HashSet<(String, u64)>,
    pub live_signatures: HashSet<String>,
    pub events: HashMap<(String, u64), Vec<BotEvent>>,
    /// Materialized Shadow evidence per (signature, slot, target): the joined market event's
    /// index and kind, `None` when the evidence names no event identity (an opportunity row).
    /// Scope: the capture's `ECON_CANONICAL`/`ECON_OPPORTUNITY` rows; absence is no proof that no
    /// evaluation existed.
    pub shadow: HashMap<(String, u64, String), Option<(u32, LiquidationType)>>,
    pub coverage: Coverage,
    pub complete: bool,
}

impl BotEvidence {
    /// One event per source, index and kind (a later row replaces an earlier one).
    fn event(&mut self, signature: &str, slot: u64, e: BotEvent) {
        let list = self
            .events
            .entry((signature.to_string(), slot))
            .or_default();
        match list
            .iter_mut()
            .find(|b| (b.source, b.index, b.kind) == (e.source, e.index, e.kind))
        {
            Some(b) => *b = e,
            None => list.push(e),
        }
    }
    /// The events of a live payload's logs (only a successful, completely decoded log).
    fn payload(&mut self, signature: &str, slot: u64, logs: &[String], program: &Pubkey) {
        let d = decode_receiver_events(logs, program);
        if d.decoder_errors != 0 || d.truncated {
            return;
        }
        for (index, r) in d.sequence.iter().enumerate() {
            let e = match *r {
                LogEventRef::Receiver(i) => {
                    let ev = &d.events[i];
                    BotEvent {
                        source: BotSource::LivePayload,
                        index: index as u32,
                        kind: LiquidationType::Receivership,
                        target: ev.target.to_string(),
                        actor: ev.receiver.to_string(),
                        seized: Some(ev.seized_equity_usd),
                        repaid: Some(ev.repaid_equity_usd),
                        protocol_fee: Some(u64::from(ev.protocol_flat_fee_lamports)),
                        native: None,
                    }
                }
                LogEventRef::Additional(i) => {
                    let ev = &d.additional_events[i];
                    BotEvent {
                        source: BotSource::LivePayload,
                        index: index as u32,
                        kind: ev.kind,
                        target: ev.target.clone(),
                        actor: ev.actor.clone(),
                        seized: None,
                        repaid: None,
                        protocol_fee: None,
                        native: None,
                    }
                }
            };
            self.event(signature, slot, e);
        }
    }

    /// The capture side of a native run, the pinned verifier's import over the typed records:
    /// the market journal's live transactions (with their payload events), the observer's
    /// receivership rows (live or recovered) and its coverage; the run's `ECON_CANONICAL` rows
    /// (canonical events and their exact Shadow joins) and `ECON_OPPORTUNITY` rows (Shadow
    /// evidence without an event identity; they replace a join of the same target, as in the
    /// pinned verifier's import order). Canonical truth rows of the market journal carry no
    /// row identity of their own there and are not imported (the joins are their import).
    pub fn from_run(run: &Path, program: &Pubkey) -> Result<Self, String> {
        use super::rows::{
            MarketCoverage, MarketLiquidation, MarketTransaction, TxOrigin, WindowEvent,
        };
        let paths = crate::run_layout::RunPaths::new(run, crate::run_layout::RunLayout::V2);
        let market = paths.market_dir().join(super::MARKET_JOURNAL);
        if !market.exists() {
            return Err("RUN_ARTIFACT_MISSING".into());
        }
        let mut bot = BotEvidence::default();
        read(&market, |frame| {
            match frame.kind {
                kind::MARKET_TRANSACTION => {
                    let row: MarketTransaction =
                        frame.typed().map_err(|_| "BOT_TRANSACTION_IDENTITY")?;
                    // A live row's payload logs (successful transactions only).
                    let logs = match &row.origin {
                        TxOrigin::TransactionSubscribe { tx, .. } => Some(
                            tx.as_ref()
                                .ok()
                                .filter(|t| t.status().is_success())
                                .and_then(|t| t.meta.as_ref())
                                .and_then(|m| m.log_messages.clone()),
                        ),
                        TxOrigin::LogsSubscribe { logs, .. } => {
                            Some(logs.logs.clone().filter(|_| logs.status.is_success()))
                        }
                        TxOrigin::GapBackfill { .. } | TxOrigin::FullSlot { .. } => None,
                    };
                    if let Some(logs) = logs {
                        bot.live.insert((row.signature.clone(), row.slot));
                        bot.live_signatures.insert(row.signature.clone());
                        if let Some(logs) = logs {
                            bot.payload(&row.signature, row.slot, &logs, program);
                        }
                    }
                }
                kind::MARKET_LIQUIDATION => {
                    let row: MarketLiquidation = frame.typed().map_err(|_| "BOT_EVENT_IDENTITY")?;
                    if let MarketLiquidation::Receiver(r) = row {
                        bot.event(
                            &r.signature,
                            r.slot,
                            BotEvent {
                                source: if r.t_rx_ns.is_some() {
                                    BotSource::Live
                                } else {
                                    BotSource::Backfill
                                },
                                index: r.event.event_index,
                                kind: LiquidationType::Receivership,
                                target: r.event.target.to_string(),
                                actor: r.event.receiver.to_string(),
                                seized: Some(r.event.seized_equity_usd),
                                repaid: Some(r.event.repaid_equity_usd),
                                protocol_fee: Some(u64::from(r.event.protocol_flat_fee_lamports)),
                                native: None,
                            },
                        );
                    }
                }
                kind::MARKET_COVERAGE => {
                    let row: MarketCoverage = frame.typed().map_err(|_| "BOT_COVERAGE")?;
                    let ns = |s: u64| i128::from(s) * NS;
                    match row {
                        MarketCoverage::SlotBoundary {
                            slot,
                            observed_unix_s,
                            ..
                        } => bot.coverage.slot_boundary(ns(observed_unix_s), slot),
                        MarketCoverage::Window {
                            observed_unix_s,
                            counted,
                            event,
                            ..
                        } => match event {
                            WindowEvent::Gap { .. } if counted => {
                                bot.coverage.gap(ns(observed_unix_s))
                            }
                            WindowEvent::Reconnect { .. } => {
                                bot.coverage.reconnected(ns(observed_unix_s))
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
                _ => {}
            }
            Ok(())
        })?;
        let reports = paths.snapshot(crate::analytics::econ::REPORT_JOURNAL);
        let mut opportunities = Vec::new();
        if reports.exists() {
            read(&reports, |frame| {
                // Schema 1 is a historical value frame (the pinned report skips it too).
                if frame.schema == 1 {
                    return Ok(());
                }
                match frame.kind {
                    kind::ECON_CANONICAL => {
                        let row: crate::analytics::econ::CanonicalJoin =
                            frame.typed().map_err(|_| "ECON_CANONICAL_INVALID")?;
                        let e = &row.market;
                        bot.event(
                            &e.signature,
                            e.slot,
                            BotEvent {
                                source: BotSource::Canonical,
                                index: e.event_index,
                                kind: e.liquidation_type,
                                target: e.target.clone(),
                                actor: e.liquidator.clone(),
                                seized: e.seized_value_usd,
                                repaid: e.repaid_value_usd,
                                protocol_fee: e.protocol_fee_lamports,
                                native: e.native_evidence.clone(),
                            },
                        );
                        if row.our.is_some() {
                            bot.shadow.insert(
                                (e.signature.clone(), e.slot, e.target.clone()),
                                Some((e.event_index, e.liquidation_type)),
                            );
                        }
                    }
                    kind::ECON_OPPORTUNITY => {
                        let row: crate::analytics::econ::OpportunityRow =
                            frame.typed().map_err(|_| "ECON_OPPORTUNITY_INVALID")?;
                        let (_, strong) = row.race_join();
                        let market = row.market.as_ref().filter(|_| strong);
                        let winner = row.winner.as_ref();
                        let signature = market.map(|m| m.signature.clone()).or_else(|| {
                            winner.and_then(|w| match &w.identity {
                                WinnerIdentity::Unique { signature, .. } => Some(signature.clone()),
                                _ => None,
                            })
                        });
                        let slot = market.map(|m| m.slot).or(winner.map(|w| w.slot));
                        if let (Some(signature), Some(slot)) = (signature, slot) {
                            if !row.rec.account.is_empty() {
                                opportunities.push((signature, slot, row.rec.account.clone()));
                            }
                        }
                    }
                    _ => {}
                }
                Ok(())
            })?;
        }
        for key in opportunities {
            bot.shadow.insert(key, None);
        }
        bot.complete = true;
        Ok(bot)
    }
}

/// Every typed frame of a journal chain (segments followed; the manifest frame skipped).
fn read(
    path: &Path,
    mut f: impl FnMut(&journal::Frame) -> Result<(), &'static str>,
) -> Result<(), String> {
    let mut r = crate::storage::segments::ChainReader::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    while let Some(frame) = r
        .next_frame()
        .map_err(|e| format!("{}: {e}", path.display()))?
    {
        if frame.kind != journal::MANIFEST {
            f(&frame)?;
        }
    }
    Ok(())
}

/// Where a transaction lies relative to the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Membership {
    InWindow,
    Outside,
    Unknown,
}

/// The index's own reading of one in-scope transaction.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexedTx {
    pub signature: String,
    pub slot: u64,
    pub block_time: Option<i64>,
    pub membership: Membership,
    pub boundary_second: bool,
    /// CLASSIFIED | UNKNOWN | FAILED_TRANSACTION.
    pub classification: &'static str,
    pub unknown_reason: Option<&'static str>,
    pub events: Vec<TruthEvent>,
    /// Program events per kind, of unknown transactions too (the log decoder's events).
    pub program_events: [u64; 4],
    pub markers: [u64; 6],
}

/// One event's comparison with the capture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceMatch {
    pub source: BotSource,
    pub status: String,
    pub unknown_amounts: Vec<String>,
    pub different_amounts: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventComparison {
    pub event_index: u32,
    pub kind: LiquidationType,
    pub target: String,
    pub actor: String,
    pub detection: String,
    pub matched: Vec<SourceMatch>,
    pub shadow_status: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TxComparison {
    pub signature: String,
    pub slot: u64,
    pub transaction_status: String,
    pub live_coverage: String,
    pub classification: String,
    pub events: Vec<EventComparison>,
}

/// The comparison of one in-window transaction with the capture (`compare`).
pub fn compare(tx: &IndexedTx, bot: &BotEvidence) -> TxComparison {
    let key = (tx.signature.clone(), tx.slot);
    let live = bot.live.contains(&key);
    let same_signature = bot.live_signatures.contains(&tx.signature);
    let coverage = bot.coverage.at(tx.block_time, tx.boundary_second);
    let transaction_status = if live {
        "MATCH"
    } else if same_signature {
        "SLOT_MISMATCH"
    } else {
        "MISSING_LIVE"
    };
    let candidates = bot.events.get(&key).map(Vec::as_slice).unwrap_or(&[]);
    let mut events = Vec::new();
    for ev in &tx.events {
        let receiver_rank = tx
            .events
            .iter()
            .filter(|e| {
                e.event_index < ev.event_index
                    && e.liquidation_type == LiquidationType::Receivership
            })
            .count() as u32;
        let mut matched = Vec::new();
        for b in candidates {
            let position = if b.source.all_kind_index() {
                b.index == ev.event_index
            } else {
                ev.liquidation_type == LiquidationType::Receivership && b.index == receiver_rank
            };
            if !position {
                continue;
            }
            let (mut mismatch, mut unknown) = (Vec::new(), Vec::new());
            let amounts: [(&str, Option<f64>, Option<f64>); 2] = [
                ("seized_value_usd", ev.seized_value_usd, b.seized),
                ("repaid_value_usd", ev.repaid_value_usd, b.repaid),
            ];
            for (name, a, c) in amounts {
                match (a, c) {
                    (Some(a), Some(c)) if a != c => mismatch.push(name.to_string()),
                    (Some(_), Some(_)) => {}
                    _ => unknown.push(name.to_string()),
                }
            }
            match (ev.protocol_fee_lamports, b.protocol_fee) {
                (Some(a), Some(c)) if a != c => mismatch.push("protocol_fee_lamports".into()),
                (Some(_), Some(_)) => {}
                _ => unknown.push("protocol_fee_lamports".into()),
            }
            if ev.native_evidence.is_some() {
                match &b.native {
                    // The live payload decoder carries native amounts of receiverships only.
                    Some(native) if b.source == BotSource::LivePayload => {
                        if Some(native) != ev.native_evidence.as_ref() {
                            mismatch.push("native_evidence".into());
                        }
                    }
                    _ => unknown.push("native_amount_semantics".into()),
                }
            }
            let status = if b.kind != ev.liquidation_type {
                "TYPE_MISMATCH"
            } else if b.target != ev.target || b.actor != ev.liquidator {
                "TARGET_OR_ACTOR_MISMATCH"
            } else if !mismatch.is_empty() {
                "AMOUNT_MISMATCH"
            } else {
                "MATCH"
            };
            matched.push(SourceMatch {
                source: b.source,
                status: status.into(),
                unknown_amounts: unknown,
                different_amounts: mismatch,
            });
        }
        let bot_live = matched.iter().find(|m| m.source == BotSource::Live);
        let detection = if !live {
            match coverage {
                "PROVEN_WEBSOCKET_OUTAGE_INTERVAL" => "MISSING_DURING_OUTAGE",
                "WITHIN_OBSERVED_SLOT_INTERVAL" => "MISSING_LIQUIDATION_WITHIN_OBSERVED_INTERVAL",
                _ => "MISSING_LIVE_LIQUIDATION_COVERAGE_UNPROVEN",
            }
            .to_string()
        } else if let Some(m) = bot_live {
            m.status.clone()
        } else if ev.liquidation_type == LiquidationType::Receivership {
            "MISSING_LIQUIDATION_RECORD_WITH_LIVE_TX".into()
        } else if matched.iter().any(|m| m.source == BotSource::LivePayload) {
            "LIVE_PAYLOAD_PRESENT_DETECTION_UNPROVEN".into()
        } else {
            "DETECTION_UNPROVEN".into()
        };
        let shadow = bot
            .shadow
            .get(&(tx.signature.clone(), tx.slot, ev.target.clone()));
        let shadow_status = match shadow {
            Some(Some((index, kind)))
                if *index == ev.event_index && *kind == ev.liquidation_type =>
            {
                "SHADOW_MATCH"
            }
            Some(_) => "SHADOW_IDENTITY_UNKNOWN",
            None => "SHADOW_NO_EVIDENCE",
        };
        events.push(EventComparison {
            event_index: ev.event_index,
            kind: ev.liquidation_type,
            target: ev.target.clone(),
            actor: ev.liquidator.clone(),
            detection,
            matched,
            shadow_status: shadow_status.into(),
        });
    }
    TxComparison {
        signature: tx.signature.clone(),
        slot: tx.slot,
        transaction_status: transaction_status.into(),
        live_coverage: coverage.into(),
        classification: tx.classification.into(),
        events,
    }
}

/// The classification of a fetched transaction (`detect`): a failed transaction has no events,
/// a successful one is the classifier's, anything else is UNKNOWN with its reason.
pub fn classify(tx: &ChainTx, program: &Pubkey, block_time: Option<i64>) -> IndexedTx {
    let mut markers = [0u64; 6];
    for (m, _, _) in super::parity::markers(tx, program) {
        markers[m] += 1;
    }
    let mut out = IndexedTx {
        signature: tx.signature().unwrap_or_default().to_string(),
        slot: tx.slot,
        block_time,
        membership: Membership::Unknown,
        boundary_second: false,
        classification: "UNKNOWN",
        unknown_reason: None,
        events: Vec::new(),
        program_events: [0; 4],
        markers,
    };
    // The A4 parity mapping (`parity::transaction_line`): a failed transaction has no events,
    // otherwise the classifier's events or its reason.
    match (tx.status(), truth::classify(tx, program, block_time, None)) {
        (TxStatus::Failed(_), _) => out.classification = "FAILED_TRANSACTION",
        (_, Ok(events)) => {
            for e in &events {
                out.program_events[kind_slot(e.liquidation_type)] += 1;
            }
            out.events = events;
            out.classification = "CLASSIFIED";
        }
        (_, Err(code)) => {
            out.unknown_reason = Some(code);
            // Observed only: the events the logs carry, though the transaction is unqualified
            // (never compared: their identity is the classifier's).
            if let Some(logs) = tx.meta.as_ref().and_then(|m| m.log_messages.as_ref()) {
                let d = decode_receiver_events(logs, program);
                out.program_events[kind_slot(LiquidationType::Receivership)] +=
                    d.events.len() as u64;
                for e in &d.additional_events {
                    out.program_events[kind_slot(e.kind)] += 1;
                }
            }
        }
    }
    out
}

/// The verified INDEX result of one attempt (kind `INDEX_RESULT`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IndexResult {
    pub schema: String,
    pub verification_spec_id: String,
    /// INDEX_VERIFIED | HISTORICAL_INCOMPLETE.
    pub status: String,
    pub reason: Option<String>,
    pub incomplete_reasons: Vec<String>,
    pub window_start_ns: u64,
    pub window_end_ns: u64,
    pub crossed_start: bool,
    pub head_after_end: bool,
    pub pages: u64,
    pub indexed_signatures: u64,
    pub in_window_transactions: u64,
    pub unresolved_transactions: u64,
    pub unknown_transactions: u64,
    /// Qualified (classified) events per kind, and every observed event incl. unknown
    /// transactions, in `KINDS` order.
    pub qualified: [u64; 4],
    pub observed: [u64; 4],
    /// The six markers of the window's transactions (`parity::MARKERS` order).
    pub markers: [u64; 6],
    pub comparison_counts: BTreeMap<String, u64>,
    /// The comparisons of the window's liquidation transactions, and the capture's live
    /// transactions the index does not hold.
    pub comparisons: Vec<TxComparison>,
    pub bot_only: Vec<(String, u64)>,
    pub bot_import_complete: bool,
    pub zero_liquidations_proven: bool,
    pub first_live_ns: Option<i128>,
    pub last_live_ns: Option<i128>,
    pub last_live_slot: Option<u64>,
    pub open_gap_start_ns: Option<i128>,
    /// This attempt's provider cost, and its calls per method.
    pub calls: u64,
    pub bytes: u64,
    pub signature_calls: u64,
    pub transaction_calls: u64,
    /// The attempt's page bound (`INDEX_PAGE_BUDGET`: new pages, cached pages are free).
    pub max_pages: u64,
    /// Never promoted: INDEX evidence is not canonical and never the C0-E1 master.
    pub canonical_verified: bool,
    pub c0_e1_master_pass: bool,
}
impl Payload for IndexResult {
    const KIND: u16 = kind::INDEX_RESULT;
}

/// The INDEX attempt's processing state over the cached and fetched history.
struct Reading {
    pager: SignaturePager,
    head_after_end: bool,
    crossed_start: bool,
    fetched: HashMap<String, Result<IndexedTx, &'static str>>,
    signature_calls: u64,
    transaction_calls: u64,
}

fn crosses(page: &[SignatureEntry], start_ns: u64) -> bool {
    page.iter().all(|r| {
        r.block_time
            .is_some_and(|bt| i128::from(bt) * NS < i128::from(start_ns))
    })
}

/// Pages the program's history back over the window start: cached pages are replayed, at most
/// `max_pages` new ones are fetched (`INDEX_PAGE_BUDGET`); an empty page before the start is
/// `INDEX_HISTORY_EXHAUSTED_BEFORE_START`. Both leave the attempt resumable.
fn paginate(
    c: &mut Client,
    program: &Pubkey,
    job: &Job,
    max_pages: u64,
    r: &mut Reading,
) -> Result<(), &'static str> {
    let mut fetched = 0u64;
    loop {
        let before = r
            .pager
            .before()
            .map(|b| Signature::from_str(b).map_err(|_| "INDEX_CURSOR_INVALID"))
            .transpose()?;
        let read = ChainRead::Signatures {
            address: *program,
            before,
        };
        if !c.is_cached(&read) {
            if fetched >= max_pages {
                return Err("INDEX_PAGE_BUDGET");
            }
            fetched += 1;
        }
        let calls = c.calls;
        let answer = c.read(&read);
        r.signature_calls += c.calls - calls;
        let page = match answer? {
            ChainAnswer::Signatures(Ok(page)) => page,
            _ => return Err("INDEX_PAGE_LAYOUT"),
        };
        if before.is_none() {
            r.head_after_end = page.iter().any(|row| {
                row.block_time
                    .is_some_and(|bt| i128::from(bt) * NS >= i128::from(job.end_ns))
            });
        }
        let start = job.start_ns;
        match r.pager.accept(page, |p| crosses(p, start))? {
            PageEnd::More => {}
            PageEnd::Stopped => {
                r.crossed_start = true;
                return Ok(());
            }
            PageEnd::Exhausted => return Err("INDEX_HISTORY_EXHAUSTED_BEFORE_START"),
        }
    }
}

/// Fetches and classifies every indexed transaction that may lie in the window (boundary
/// seconds included: chain time is coarse). Unavailable (pruned) and unsupported transactions
/// stay unresolved; a transaction that is not the indexed one fails the attempt.
fn fetch_transactions(
    c: &mut Client,
    program: &Pubkey,
    job: &Job,
    r: &mut Reading,
) -> Result<(), &'static str> {
    let (start_s, end_s) = (job.start_ns / 1_000_000_000, job.end_ns / 1_000_000_000);
    let in_window = |bt: i64| {
        let at = i128::from(bt) * NS;
        i128::from(job.start_ns) <= at && at < i128::from(job.end_ns)
    };
    let boundary = |bt: i64| bt == start_s as i64 || bt == end_s as i64;
    let rows: Vec<SignatureEntry> = r.pager.rows().to_vec();
    for row in rows {
        if r.fetched.contains_key(&row.signature) {
            continue;
        }
        if row
            .block_time
            .is_some_and(|bt| !in_window(bt) && !boundary(bt))
        {
            continue;
        }
        let signature = Signature::from_str(&row.signature).map_err(|_| "INDEX_ROW_LAYOUT")?;
        let calls = c.calls;
        let answer = c.read(&ChainRead::Transaction { signature });
        r.transaction_calls += c.calls - calls;
        let tx = match answer {
            Ok(ChainAnswer::Transaction(tx)) => *tx,
            Ok(_) => return Err("TRANSACTION_IDENTITY_MISMATCH"),
            // A null answer, or the provider's pruned/unavailable history: unresolved and retried
            // by the next attempt (neither is cached).
            Err("RPC_MISSING_BLOCK") => {
                r.fetched
                    .insert(row.signature.clone(), Err("TRANSACTION_UNAVAILABLE"));
                continue;
            }
            Err("RPC_PRUNED_OR_UNAVAILABLE") => {
                r.fetched.insert(
                    row.signature.clone(),
                    Err("TRANSACTION_PRUNED_OR_UNAVAILABLE"),
                );
                continue;
            }
            Err("RPC_UNSUPPORTED_VERSION") => {
                r.fetched
                    .insert(row.signature.clone(), Err("RPC_UNSUPPORTED_TX_VERSION"));
                continue;
            }
            Err(e) => return Err(e),
        };
        let indexed = match tx {
            Ok(tx) => {
                if tx.slot != row.slot || tx.signature() != Some(row.signature.as_str()) {
                    return Err("TRANSACTION_IDENTITY_MISMATCH");
                }
                if let (Some(a), Some(b)) = (row.block_time, tx.block_time) {
                    if a != b {
                        return Err("INDEX_TX_TIME_MISMATCH");
                    }
                }
                if tx.status() != row.status {
                    return Err("INDEX_TX_STATUS_MISMATCH");
                }
                let time = tx.block_time.or(row.block_time);
                let mut indexed = classify(&tx, program, time);
                indexed.membership = match time {
                    None => Membership::Unknown,
                    Some(bt) if in_window(bt) => Membership::InWindow,
                    Some(_) => Membership::Outside,
                };
                indexed.boundary_second = time.is_some_and(boundary);
                indexed
            }
            // An undecodable transaction: its identity and time are the index row's, its content
            // unknown.
            Err(e) => IndexedTx {
                signature: row.signature.clone(),
                slot: row.slot,
                block_time: row.block_time,
                membership: match row.block_time {
                    None => Membership::Unknown,
                    Some(bt) if in_window(bt) => Membership::InWindow,
                    Some(_) => Membership::Outside,
                },
                boundary_second: row.block_time.is_some_and(boundary),
                classification: "UNKNOWN",
                unknown_reason: Some(e.0),
                events: Vec::new(),
                program_events: [0; 4],
                markers: [0; 6],
            },
        };
        r.fetched.insert(row.signature.clone(), Ok(indexed));
    }
    Ok(())
}

fn add<const N: usize>(sum: &mut [u64; N], counts: &[u64; N]) {
    for (s, n) in sum.iter_mut().zip(counts) {
        *s += n;
    }
}

/// The result of an attempt (`report`): the index's counts, the comparisons and the verdict.
fn report(
    job: &Job,
    max_pages: u64,
    r: &Reading,
    bot: &BotEvidence,
    reason: Option<&'static str>,
    cost: (u64, u64),
) -> IndexResult {
    let (mut qualified, mut observed, mut markers) = ([0u64; 4], [0u64; 4], [0u64; 6]);
    let (mut unknown, mut missing, mut in_window) = (0u64, 0u64, 0u64);
    let mut statuses: BTreeMap<String, u64> = BTreeMap::new();
    let mut comparisons = Vec::new();
    let mut indexed: HashSet<(String, u64)> = HashSet::new();
    let (start_s, end_s) = (job.start_ns / 1_000_000_000, job.end_ns / 1_000_000_000);
    for row in r.pager.rows() {
        indexed.insert((row.signature.clone(), row.slot));
        let in_scope = row.block_time.is_none_or(|bt| {
            let at = i128::from(bt) * NS;
            (i128::from(job.start_ns) <= at && at < i128::from(job.end_ns))
                || bt == start_s as i64
                || bt == end_s as i64
        });
        if !in_scope {
            continue;
        }
        let tx = match r.fetched.get(&row.signature) {
            Some(Ok(tx)) => tx,
            _ => {
                missing += 1;
                continue;
            }
        };
        match tx.membership {
            Membership::Unknown => {
                unknown += 1;
                continue;
            }
            Membership::Outside => continue,
            Membership::InWindow => {}
        }
        in_window += 1;
        add(&mut observed, &tx.program_events);
        add(&mut markers, &tx.markers);
        match tx.classification {
            "UNKNOWN" => unknown += 1,
            "CLASSIFIED" => add(&mut qualified, &tx.program_events),
            _ => {}
        }
        let comparison = compare(tx, bot);
        *statuses
            .entry(comparison.transaction_status.clone())
            .or_default() += 1;
        for e in &comparison.events {
            *statuses.entry(e.detection.clone()).or_default() += 1;
        }
        if !comparison.events.is_empty() {
            comparisons.push(comparison);
        }
    }
    let mut bot_only: Vec<(String, u64)> = bot
        .live
        .iter()
        .filter(|k| !indexed.contains(*k))
        .cloned()
        .collect();
    bot_only.sort();
    if !bot_only.is_empty() {
        statuses.insert("BOT_ONLY_INDEX_MISSING".into(), bot_only.len() as u64);
    }
    let complete =
        r.crossed_start && r.head_after_end && missing == 0 && unknown == 0 && reason.is_none();
    let mut incomplete = Vec::new();
    if let Some(reason) = reason {
        incomplete.push(reason.to_string());
    }
    if !r.crossed_start {
        incomplete.push("INDEX_START_NOT_CROSSED".into());
    }
    if !r.head_after_end {
        incomplete.push("INDEX_HEAD_BEFORE_END".into());
    }
    if unknown > 0 || missing > 0 {
        incomplete.push("HISTORICAL_PAYLOAD_UNKNOWN".into());
    }
    IndexResult {
        schema: SCHEMA.into(),
        verification_spec_id: job.verification_spec_id.clone(),
        status: if complete {
            "INDEX_VERIFIED"
        } else {
            "HISTORICAL_INCOMPLETE"
        }
        .into(),
        reason: reason.map(String::from),
        incomplete_reasons: incomplete,
        window_start_ns: job.start_ns,
        window_end_ns: job.end_ns,
        crossed_start: r.crossed_start,
        head_after_end: r.head_after_end,
        pages: r.pager.pages(),
        indexed_signatures: r.pager.rows().len() as u64,
        in_window_transactions: in_window,
        unresolved_transactions: missing,
        unknown_transactions: unknown,
        qualified,
        observed,
        markers,
        comparison_counts: statuses,
        comparisons,
        bot_only,
        bot_import_complete: bot.complete,
        zero_liquidations_proven: complete && qualified.iter().all(|n| *n == 0),
        first_live_ns: bot.coverage.first_live_ns,
        last_live_ns: bot.coverage.last_live_ns,
        last_live_slot: bot.coverage.last_live_slot,
        open_gap_start_ns: bot.coverage.gap_open,
        calls: cost.0,
        bytes: cost.1,
        signature_calls: r.signature_calls,
        transaction_calls: r.transaction_calls,
        max_pages,
        canonical_verified: false,
        c0_e1_master_pass: false,
    }
}

/// One INDEX attempt of `job` against `url` (`None`: the cache only), compared with `bot`:
/// pages, transactions and the result; the result is stored content-addressed in the job's
/// output (`results/<sha256>.p0j`).
pub fn run(
    job: &Job,
    max_pages: u64,
    url: Option<String>,
    bot: &BotEvidence,
) -> Result<IndexResult, String> {
    if job.invalid() || max_pages == 0 {
        return Err("VERIFICATION_CONFIG_INVALID".into());
    }
    let program = Pubkey::from_str(&job.program).map_err(|_| "PROGRAM_INVALID")?;
    std::fs::create_dir_all(job.output.join("cache")).map_err(|_| "VERIFICATION_IO")?;
    let mut c = Client::new(job, url)?;
    let mut r = Reading {
        pager: SignaturePager::new(),
        head_after_end: false,
        crossed_start: false,
        fetched: HashMap::new(),
        signature_calls: 0,
        transaction_calls: 0,
    };
    let reason = (|| -> Result<(), &'static str> {
        c.guard()?;
        if let Err(e) = paginate(&mut c, &program, job, max_pages, &mut r) {
            if !matches!(
                e,
                "INDEX_PAGE_BUDGET" | "INDEX_HISTORY_EXHAUSTED_BEFORE_START"
            ) {
                return Err(e);
            }
            // The pages held so far are still worked; the reason stays the attempt's.
            fetch_transactions(&mut c, &program, job, &mut r)?;
            return Err(e);
        }
        fetch_transactions(&mut c, &program, job, &mut r)
    })()
    .err();
    let result = report(job, max_pages, &r, bot, reason, (c.calls, c.bytes));
    // Every attempt's result is kept, content-addressed and immutable (as the strict results).
    let bytes = payload::encode(&result).map_err(|e| e.to_string())?;
    let dir = job.output.join("results");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    journal::write_typed_value(
        &dir.join(format!(
            "{}.p0j",
            journal::hex(&solana_sdk::hash::hash(&bytes).to_bytes())
        )),
        source::STRICT,
        &result,
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}

impl IndexResult {
    /// Human-readable terminal summary.
    pub fn terminal(&self) -> String {
        let mut out = String::from("PROJECT 0 INDEX VERIFICATION (native evidence)\n");
        out.push_str(&format!(
            "status={} reason={} incomplete={}\n",
            self.status,
            self.reason.as_deref().unwrap_or("none"),
            if self.incomplete_reasons.is_empty() {
                "none".to_string()
            } else {
                self.incomplete_reasons.join(",")
            }
        ));
        out.push_str(&format!(
            "pages={} signatures={} in_window={} unresolved={} unknown={} crossed_start={} head_after_end={}\n",
            self.pages,
            self.indexed_signatures,
            self.in_window_transactions,
            self.unresolved_transactions,
            self.unknown_transactions,
            self.crossed_start,
            self.head_after_end
        ));
        for (i, k) in KINDS.iter().enumerate() {
            out.push_str(&format!(
                "qualified {}={} observed={}\n",
                k.as_str(),
                self.qualified[i],
                self.observed[i]
            ));
        }
        for (i, (_, name)) in super::parity::MARKERS.iter().enumerate() {
            out.push_str(&format!("marker {name}={}\n", self.markers[i]));
        }
        for (k, v) in &self.comparison_counts {
            out.push_str(&format!("comparison {k}={v}\n"));
        }
        out.push_str(&format!("rpc calls={} bytes={}\n", self.calls, self.bytes));
        out.push_str("non_claims: INDEX scope only; CANONICAL_VERIFIED=false; C0_E1_MASTER_PASS=false; UNKNOWN is never zero; chain history does not prove DarkBot could have traded an event in time\n");
        out
    }
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
