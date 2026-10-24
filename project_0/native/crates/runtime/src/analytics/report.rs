//! Native C0-E1 report (B7; PHASENPLAN L.3–L.6, SK-01/SK-09 (5), JFI-03).
//!
//! One normalized dataset per capture and selected verification result, read directly from the
//! run's `.p0j` evidence, plus display-only queries over it:
//!
//! * Sources: the live capture (`capture/market/market.p0j` chain), the native strict
//!   verification (`verification/native-<spec>/strict`, only the verified batches of the selected
//!   result) and Shadow (the production canonical join over the strict events and
//!   `shadow_evaluations.p0j`, else the capture-time `ECON_CANONICAL` rows; `ECON_OPPORTUNITY`
//!   rows). Index provenance and runs without native records stay with the pinned Python report.
//! * One entity per signature, slot, record kind, instruction position / event index and type:
//!   `provenance=all` lists every source of an entity once. `best` takes facts STRICT → LIVE.
//!   SHADOW is never chain evidence; Shadow-only rows are HYPOTHETICAL and counted separately,
//!   never as a chain liquidation, winner or realized PnL. RELATED_NON_LIQUIDATION rows are never
//!   liquidations, winners or profit; a marker alone is never a liquidation.
//! * Filters act on the view only: the dataset, its digest and every capture/verifier file stay
//!   unchanged by any query. `report_query_identity` binds the capture, the selected result, the
//!   dataset digest, the query and the renderer.
//! * Output is text: table or CSV with identical cells. JSON output is refused (JFI-01/03 take
//!   precedence over the older `--format json` example of L.5).
use super::econ::{
    canonical_join, joined_facts, record_digest, CanonicalJoin, CaptureFilter, JoinContext,
    JoinedFacts, OpportunityRow, REPORT_JOURNAL as ECON_REPORT_JOURNAL,
};
use super::journal::{self, kind};
use super::measurement::utc_ns;
use super::payload::Payload;
use crate::market::observer::markers::{self, Marker, MarkerHit, Pair, Position};
use crate::market::observer::related::{self, MarketRelated, RelatedKind};
use crate::market::observer::rows::{
    MarketCompletion, MarketCoverage, MarketTransaction, TxOrigin, WindowEvent,
    MARKET_COMPLETION_JOURNAL,
};
use crate::market::observer::strict_driver::{self, CaptureBinding, NATIVE_PREFIX};
use crate::market::observer::truth::{LiquidationType, TruthEvent};
use crate::market::observer::verification::{self as strict, StrictResult, StrictStatus};
use crate::market::observer::{decode_receiver_events, enrich, LogEventRef, MARKET_JOURNAL};
use crate::storage::segments::ChainReader;
use crate::winner::{ChainTx, TxStatus};
use bincode::Options as _;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};

/// Record, dataset and renderer format (part of every dataset and query identity).
pub const RENDERER: &str = "Project0/NativeReport/2";
/// Output directory of the native report inside a run.
pub const REPORT_DIR: &str = "reports/native";
const NS: i128 = 1_000_000_000;
/// Accounting overhead per retained record (keys, map nodes, vectors).
const RECORD_OVERHEAD: u64 = 256;

// ------------------------------------------------------------------------------------------
// Query (`ReportQuery`, L.3/L.5): view-only selection
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    Summary,
    Liquidations,
    Winners,
    Competition,
    Economics,
}

/// `--report-provenance`; `index` is read by the pinned Python report only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceView {
    Best,
    Live,
    Strict,
    Shadow,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    Table,
    Csv,
}

const TYPES: [LiquidationType; 4] = [
    LiquidationType::Classic,
    LiquidationType::Receivership,
    LiquidationType::ForcedDeleverage,
    LiquidationType::BankruptcySettlement,
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportQuery {
    pub types: Vec<LiquidationType>,
    pub markers: Vec<Marker>,
    pub related_events: bool,
    pub related_types: Vec<RelatedKind>,
    pub view: View,
    pub provenance: ProvenanceView,
    /// Half-open display window `[from, to)` in unix ns.
    pub from_unix_ns: Option<i128>,
    pub to_unix_ns: Option<i128>,
    /// Hides rows with a KNOWN executable net below this value; UNKNOWN stays and is counted.
    pub min_net_usd_micro: Option<i128>,
    pub winners: bool,
    pub competition: bool,
    pub economics: bool,
    pub format: Format,
}

impl Default for ReportQuery {
    fn default() -> Self {
        Self {
            types: TYPES.to_vec(),
            markers: Marker::ALL.to_vec(),
            related_events: false,
            related_types: RelatedKind::ALL.to_vec(),
            view: View::Summary,
            provenance: ProvenanceView::Best,
            from_unix_ns: None,
            to_unix_ns: None,
            min_net_usd_micro: None,
            winners: true,
            competition: true,
            economics: true,
            format: Format::Table,
        }
    }
}

/// Normalization budgets and the explicit result selection.
#[derive(Clone, Debug)]
pub struct Options {
    /// `--verification-result`: an exact `verification_result_id`, else the latest bound one.
    pub verification_result: Option<String>,
    pub max_seconds: u64,
    /// Byte budget of the normalized records held in memory (fail closed above it).
    pub max_ram_bytes: u64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            verification_result: None,
            max_seconds: 600,
            max_ram_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// `all`, `none`, names, `+name`, `-name` (comma separated, applied in order; contract order).
fn selections<T: Copy + PartialEq>(
    text: &str,
    all: &[T],
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, String> {
    let mut active: Vec<T> = Vec::new();
    for item in text.split(',') {
        match item {
            "all" => active = all.to_vec(),
            "none" => active.clear(),
            _ => {
                let (remove, name) = match item.strip_prefix('-') {
                    Some(n) => (true, n),
                    None => (false, item.trim_start_matches('+')),
                };
                let t = parse(name).ok_or_else(|| format!("REPORT_SELECTOR_UNKNOWN: {name}"))?;
                if remove {
                    active.retain(|x| *x != t);
                } else if !active.contains(&t) {
                    active.push(t);
                }
            }
        }
    }
    Ok(all.iter().copied().filter(|t| active.contains(t)).collect())
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `YYYY-MM-DDTHH:MM:SS[.f{1,9}]` followed by `Z` or `+00:00` → unix ns.
pub fn parse_utc_ns(text: &str) -> Result<i128, String> {
    let bad = || format!("REPORT_UTC_INVALID: {text}");
    let body = text
        .strip_suffix('Z')
        .or_else(|| text.strip_suffix("+00:00"))
        .ok_or_else(bad)?;
    let (date, time) = body.split_once('T').ok_or_else(bad)?;
    let num = |s: &str, len: usize| -> Result<i64, String> {
        if s.len() != len || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        s.parse().map_err(|_| bad())
    };
    let d: Vec<&str> = date.split('-').collect();
    let [y, mo, da] = d.as_slice() else {
        return Err(bad());
    };
    let (y, mo, da) = (num(y, 4)?, num(mo, 2)?, num(da, 2)?);
    let (clock, fraction) = match time.split_once('.') {
        Some((c, f)) => (c, Some(f)),
        None => (time, None),
    };
    let t: Vec<&str> = clock.split(':').collect();
    let [h, mi, s] = t.as_slice() else {
        return Err(bad());
    };
    let (h, mi, s) = (num(h, 2)?, num(mi, 2)?, num(s, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) || h > 23 || mi > 59 || s > 59 {
        return Err(bad());
    }
    let frac = match fraction {
        None => 0,
        Some(f) if (1..=9).contains(&f.len()) && f.bytes().all(|b| b.is_ascii_digit()) => {
            f.parse::<i128>().map_err(|_| bad())? * 10i128.pow(9 - f.len() as u32)
        }
        Some(_) => return Err(bad()),
    };
    let days = days_from_civil(y, mo, da);
    Ok((i128::from(days) * 86_400 + i128::from(h * 3600 + mi * 60 + s)) * NS + frac)
}

/// Decimal USD with at most six fraction digits → micro USD (no float).
pub fn parse_usd_micro(text: &str) -> Result<i128, String> {
    let bad = || format!("REPORT_NET_INVALID: {text}");
    let (neg, body) = match text.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, text),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if int.is_empty()
        || frac.len() > 6
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
        || int.len() > 18
    {
        return Err(bad());
    }
    let micro = int.parse::<i128>().map_err(|_| bad())? * 1_000_000
        + format!("{frac:0<6}").parse::<i128>().map_err(|_| bad())?;
    Ok(if neg { -micro } else { micro })
}

/// A parsed `report` invocation (the contract flag names of `measurement_contract.py`).
#[derive(Clone, Debug)]
pub struct Invocation {
    pub run: String,
    pub options: Options,
    pub query: ReportQuery,
}

pub fn parse_args(args: &[String]) -> Result<Invocation, String> {
    let mut q = ReportQuery::default();
    let mut o = Options::default();
    let mut run = None;
    let mut it = args.iter();
    let on_off = |v: &str| match v {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("REPORT_ENUM_INVALID: {v}")),
    };
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("REPORT_FLAG_VALUE_MISSING: {flag}"))
        };
        match flag.as_str() {
            "--run" => run = Some(value()?),
            "--verification-result" => o.verification_result = Some(value()?),
            "--report-max-seconds" => {
                o.max_seconds = value()?
                    .parse()
                    .ok()
                    .filter(|n| (1..=7200).contains(n))
                    .ok_or("REPORT_BUDGET_INVALID")?
            }
            "--report-max-ram-bytes" => {
                o.max_ram_bytes = value()?
                    .parse()
                    .ok()
                    .filter(|n| *n >= 1024 * 1024)
                    .ok_or("REPORT_BUDGET_INVALID")?
            }
            "--report-types" => {
                q.types = selections(&value()?, &TYPES, |n| {
                    TYPES.into_iter().find(|t| t.as_str() == n)
                })?
            }
            "--report-markers" => q.markers = selections(&value()?, &Marker::ALL, Marker::parse)?,
            "--report-related-events" => q.related_events = on_off(&value()?)?,
            "--report-related-types" => {
                q.related_types = selections(&value()?, &RelatedKind::ALL, RelatedKind::parse)?
            }
            "--report-view" => {
                q.view = match value()?.as_str() {
                    "summary" => View::Summary,
                    "liquidations" => View::Liquidations,
                    "winners" => View::Winners,
                    "competition" => View::Competition,
                    "economics" => View::Economics,
                    v => return Err(format!("REPORT_ENUM_INVALID: {v}")),
                }
            }
            "--report-provenance" => {
                q.provenance = match value()?.as_str() {
                    "best" => ProvenanceView::Best,
                    "live" => ProvenanceView::Live,
                    "strict" => ProvenanceView::Strict,
                    "shadow" => ProvenanceView::Shadow,
                    "all" => ProvenanceView::All,
                    "index" => return Err("REPORT_PROVENANCE_INDEX_NOT_NATIVE: index evidence is read by the pinned Python report (scripts/report.sh)".into()),
                    v => return Err(format!("REPORT_ENUM_INVALID: {v}")),
                }
            }
            "--report-from" => q.from_unix_ns = Some(parse_utc_ns(&value()?)?),
            "--report-to" => q.to_unix_ns = Some(parse_utc_ns(&value()?)?),
            "--report-min-net-usd" => q.min_net_usd_micro = Some(parse_usd_micro(&value()?)?),
            "--report-winners" => q.winners = on_off(&value()?)?,
            "--report-competition" => q.competition = on_off(&value()?)?,
            "--report-economics" => q.economics = on_off(&value()?)?,
            "--format" => {
                q.format = match value()?.as_str() {
                    "table" => Format::Table,
                    "csv" => Format::Csv,
                    "json" => return Err("REPORT_FORMAT_JSON_REFUSED: native report output is text (table|csv); JFI-01/03".into()),
                    v => return Err(format!("REPORT_ENUM_INVALID: {v}")),
                }
            }
            f => return Err(format!("REPORT_FLAG_UNKNOWN: {f}")),
        }
    }
    if let (Some(a), Some(b)) = (q.from_unix_ns, q.to_unix_ns) {
        if a >= b {
            return Err("REPORT_WINDOW_INVALID".into());
        }
    }
    Ok(Invocation {
        run: run.ok_or("REPORT_RUN_REQUIRED: --run ID|PATH")?,
        options: o,
        query: q,
    })
}

// ------------------------------------------------------------------------------------------
// Normalized records
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RecordKind {
    Transaction,
    Liquidation,
    InstructionMarker,
    RelatedNonLiquidation,
    ShadowOnly,
}
impl RecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transaction => "TRANSACTION",
            Self::Liquidation => "LIQUIDATION",
            Self::InstructionMarker => "INSTRUCTION_MARKER",
            Self::RelatedNonLiquidation => "RELATED_NON_LIQUIDATION",
            Self::ShadowOnly => "SHADOW_ONLY",
        }
    }
}

/// Evidence source, ordered by priority for `best` (Shadow is never chain evidence).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Source {
    Shadow,
    Unverified,
    Live,
    Strict,
}
impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
            Self::Unverified => "unverified",
            Self::Live => "live",
            Self::Strict => "strict",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Success,
    Failed,
    Unknown,
}
impl Outcome {
    fn of(s: &TxStatus) -> Self {
        match s {
            TxStatus::Success => Self::Success,
            TxStatus::Failed(_) => Self::Failed,
            TxStatus::Unknown(_) => Self::Unknown,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Identity of one entity (L.4 correlation key: signature + slot + position / event).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityKey {
    pub signature: Option<String>,
    pub slot: Option<u64>,
    pub kind: RecordKind,
    pub position: Option<Position>,
    pub event_index: Option<u32>,
    pub liquidation_type: Option<LiquidationType>,
    pub related: Option<RelatedKind>,
    /// Shadow-only rows: digest of the evaluation.
    pub shadow_record: Option<[u8; 32]>,
}

fn digest<T: Serialize>(v: &T) -> [u8; 32] {
    let bytes = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize(v)
        .expect("in-memory serialization");
    solana_sdk::hash::hash(&bytes).to_bytes()
}
fn size<T: Serialize>(v: &T) -> u64 {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialized_size(v)
        .unwrap_or(u64::MAX / 4)
}

impl EntityKey {
    fn chain(signature: &str, slot: u64, kind: RecordKind) -> Self {
        Self {
            signature: Some(signature.into()),
            slot: Some(slot),
            kind,
            position: None,
            event_index: None,
            liquidation_type: None,
            related: None,
            shadow_record: None,
        }
    }
    pub fn entity_id(&self) -> String {
        match (self.kind, self.shadow_record) {
            (RecordKind::ShadowOnly, Some(d)) => format!("shadow-{}", journal::hex(&d)),
            _ => journal::hex(&digest(self)),
        }
    }
}

/// One source's facts about an entity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub source: Source,
    pub outcome: Outcome,
    /// `CLASSIFIED`, `FAILED_TRANSACTION`, `UNKNOWN`, `RELATED_NON_LIQUIDATION` or `HYPOTHETICAL`.
    pub classification: String,
    pub transaction_index: Option<u64>,
    pub block_time: Option<i64>,
    pub markers: Vec<Marker>,
    pub pairs: Vec<Pair>,
    pub related_status: Option<String>,
    pub target: Option<String>,
    pub actor: Option<String>,
    pub asset_mint: Option<String>,
    pub liability_mint: Option<String>,
    pub transaction_fee_lamports: Option<u64>,
    pub priority_fee_lamports: Option<u64>,
    pub protocol_fee_lamports: Option<u64>,
    pub market_gross_edge_usd: Option<f64>,
    pub realized_pnl_usd: Option<f64>,
    pub permissionless_profit_opportunity: Option<bool>,
    /// Only a successful classified event of verified strict evidence.
    pub chain_liquidation_confirmed: bool,
    pub winner: Option<String>,
    pub missing_reasons: Vec<String>,
}
impl Evidence {
    fn new(source: Source, outcome: Outcome, classification: &str) -> Self {
        Self {
            source,
            outcome,
            classification: classification.into(),
            transaction_index: None,
            block_time: None,
            markers: vec![],
            pairs: vec![],
            related_status: None,
            target: None,
            actor: None,
            asset_mint: None,
            liability_mint: None,
            transaction_fee_lamports: None,
            priority_fee_lamports: None,
            protocol_fee_lamports: None,
            market_gross_edge_usd: None,
            realized_pnl_usd: None,
            permissionless_profit_opportunity: None,
            chain_liquidation_confirmed: false,
            winner: None,
            missing_reasons: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowOrigin {
    /// Production canonical join over the selected strict events (offline, typed).
    StrictReplay,
    /// The capture's own `ECON_CANONICAL` rows (no strict result selected).
    CaptureCanonical,
    /// An `ECON_OPPORTUNITY` row without a chain entity (Shadow-only).
    Opportunity,
}
impl ShadowOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StrictReplay => "STRICT_REPLAY",
            Self::CaptureCanonical => "CAPTURE_CANONICAL",
            Self::Opportunity => "OPPORTUNITY",
        }
    }
}

/// Hypothetical Shadow evidence attached to an entity (never a chain fact).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowLink {
    pub origin: ShadowOrigin,
    pub facts: JoinedFacts,
}

/// One normalized entity with every source's evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub entity_id: String,
    pub key: EntityKey,
    /// Strongest source, missing factual fields filled from weaker actual sources.
    pub best: Evidence,
    /// Every source once, strongest first.
    pub sources: Vec<Evidence>,
    /// First live receipt of the transaction (shared process clock projected to UTC).
    pub live_t_rx_unix_ns: Option<u128>,
    pub shadow: Option<ShadowLink>,
    /// Shadow-only: UTC projection of the evaluation's receipt.
    pub shadow_unix_ns: Option<u128>,
    pub missing_reasons: Vec<String>,
}

impl Record {
    fn at_unix_ns(&self, ev: &Evidence) -> Option<(i128, &'static str)> {
        ev.block_time
            .map(|s| (i128::from(s) * NS, "BLOCK_TIME"))
            .or(self.live_t_rx_unix_ns.map(|n| (n as i128, "LIVE_RECEIPT")))
            .or(self.shadow_unix_ns.map(|n| (n as i128, "SHADOW_RECEIPT")))
    }
}

// ------------------------------------------------------------------------------------------
// Dataset identity and persisted records (REPORT_DATASET / REPORT_QUERY)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetIdentity {
    pub renderer: String,
    pub capture_run_id: String,
    pub capture_evidence_digest: String,
    pub verification_spec_id: Option<String>,
    pub verification_result_id: Option<String>,
    /// The report binary is the normalizer/decoder pin.
    pub report_binary_sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetRecord {
    pub identity: DatasetIdentity,
    pub record_count: u64,
    pub records_sha256: [u8; 32],
    /// `NOT_RUN`, `CANONICAL_VERIFIED` or `HISTORICAL_INCOMPLETE`.
    pub verification_status: String,
    /// `FINAL_VERIFICATION_RESULT` only for a canonical result, else `PROVISIONAL/INCOMPLETE`.
    pub report_status: String,
    pub live_coverage_status: String,
    pub capture_clock_complete: bool,
    pub economics_replay: String,
    /// Always false: a report is never the C0-E1 master gate.
    pub c0_e1_master_pass: bool,
}
impl Payload for DatasetRecord {
    const KIND: u16 = kind::REPORT_DATASET;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryRecord {
    pub report_query_identity: String,
    pub capture_run_id: String,
    pub verification_result_id: Option<String>,
    pub dataset_sha256: [u8; 32],
    pub query: ReportQuery,
    pub renderer: String,
    pub report_status: String,
}
impl Payload for QueryRecord {
    const KIND: u16 = kind::REPORT_QUERY;
}

/// The normalized dataset of one capture + selected result.
#[derive(Debug)]
pub struct Dataset {
    pub record: DatasetRecord,
    pub records: Vec<Record>,
    pub path: PathBuf,
    /// The typed canonical economics replay behind `record.economics_replay` (its text).
    pub economics: Economics,
    /// The run's measurement plan (its typed pre-capture manifest).
    pub plan: crate::analytics::measurement::MeasurementPlan,
    /// The capture's completion axes (`MARKET_COMPLETION`).
    pub completion: MarketCompletion,
    /// The market journal is sealed COMPLETE (D1: no gap, overflow, fatal, decoder or retention
    /// reason, full clock, drain and no write error).
    pub capture_journal_complete: bool,
    /// The selected result's last scoped slot (the deployment proof's slot bound).
    pub verification_scope_end: Option<u64>,
}

/// Canonical economics replay over the selected strict result (C0E1-P4-VC reads it typed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Economics {
    /// The canonical events in the capture window were joined to the Shadow evaluations.
    Replayed { events: u64, shadow_available: bool },
    /// A strict result exists but its canonical join did not complete (the reason).
    Incomplete(String),
    /// No strict result: only the capture-time canonical join.
    NotRun,
}

// ------------------------------------------------------------------------------------------
// Normalization
// ------------------------------------------------------------------------------------------

type ShadowKey = (String, u64, u32, LiquidationType, String);
/// Shadow-only candidate: link, receipt time, target, own signature and slot.
type ShadowOnly = (
    ShadowLink,
    Option<u128>,
    String,
    Option<String>,
    Option<u64>,
);

/// A2: inputs of the canonical race margins from the market journal: the capture's commitment
/// (its start record) and the first in-window local receipt of each slot (its `SlotBoundary`
/// rows, the same receipts the live analytics keep).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RaceInputs {
    pub market_commitment: Option<String>,
    pub slot_first_seen: BTreeMap<u64, u64>,
}
impl RaceInputs {
    /// Notes one typed coverage frame; `true` when it added a slot.
    pub(crate) fn note(&mut self, frame: &journal::Frame) -> Result<bool, String> {
        match frame.typed().map_err(|_| "LIVE_FRAME_INVALID")? {
            MarketCoverage::SlotBoundary { slot, t_rx_ns, .. } => {
                let new = !self.slot_first_seen.contains_key(&slot);
                let first = self.slot_first_seen.entry(slot).or_insert(t_rx_ns);
                *first = (*first).min(t_rx_ns);
                Ok(new)
            }
            MarketCoverage::Window {
                event: WindowEvent::Start(start),
                ..
            } => {
                self.market_commitment = Some(start.commitment.clone());
                Ok(false)
            }
            _ => Ok(false),
        }
    }
}

struct Builder {
    program: Pubkey,
    clock_origin_unix_ns: u128,
    entities: BTreeMap<EntityKey, Vec<Evidence>>,
    receipts: BTreeMap<(String, u64), u128>,
    shadow: BTreeMap<ShadowKey, ShadowLink>,
    shadow_only: BTreeMap<[u8; 32], ShadowOnly>,
    attached: BTreeSet<[u8; 32]>,
    /// Marker hits and pairs of strict transactions (attached to their canonical events).
    strict_markers: BTreeMap<(String, u64), (Vec<MarkerHit>, Vec<Pair>)>,
    race: RaceInputs,
    bytes: u64,
    max_bytes: u64,
    deadline: Instant,
}

impl Builder {
    fn charge(&mut self, n: u64) -> Result<(), String> {
        self.bytes = self.bytes.saturating_add(n).saturating_add(RECORD_OVERHEAD);
        if self.bytes > self.max_bytes {
            return Err(format!(
                "REPORT_RAM_BUDGET: {} > {} bytes",
                self.bytes, self.max_bytes
            ));
        }
        if Instant::now() > self.deadline {
            return Err("REPORT_TIME_BUDGET".into());
        }
        Ok(())
    }

    fn add(&mut self, key: EntityKey, ev: Evidence) -> Result<(), String> {
        self.charge(size(&key) + size(&ev))?;
        let list = self.entities.entry(key).or_default();
        match list.iter_mut().find(|e| e.source == ev.source) {
            // The same fact received twice (e.g. a duplicate live notification) is kept once;
            // a differing duplicate is kept as the first and marked, never merged silently.
            Some(existing) if *existing != ev => {
                let code = "SOURCE_EVIDENCE_CONFLICT".to_string();
                if !existing.missing_reasons.contains(&code) {
                    existing.missing_reasons.push(code);
                }
            }
            Some(_) => {}
            None => list.push(ev),
        }
        Ok(())
    }

    fn receipt(&mut self, signature: &str, slot: u64, t_rx_ns: u64) {
        let at = self.clock_origin_unix_ns + u128::from(t_rx_ns);
        let e = self
            .receipts
            .entry((signature.to_string(), slot))
            .or_insert(at);
        *e = (*e).min(at);
    }

    /// TRANSACTION, INSTRUCTION_MARKER and RELATED evidence of one decoded transaction; for
    /// non-strict sources also its log-decoded (unconfirmed) liquidation events.
    fn chain_tx(
        &mut self,
        source: Source,
        signature: &str,
        slot: u64,
        tx: &ChainTx,
        transaction_index: Option<u64>,
        block_time: Option<i64>,
    ) -> Result<(), String> {
        let status = tx.status();
        let outcome = Outcome::of(&status);
        let mut base = Evidence::new(source, outcome, "CLASSIFIED");
        base.transaction_index = transaction_index;
        base.block_time = block_time.or(tx.block_time);
        base.transaction_fee_lamports = tx.meta.as_ref().and_then(|m| m.fee);
        base.priority_fee_lamports = enrich(tx, &self.program).priority_fee_lamports;
        match &status {
            TxStatus::Failed(_) => base.classification = "FAILED_TRANSACTION".into(),
            TxStatus::Unknown(code) => {
                base.classification = "UNKNOWN".into();
                base.missing_reasons.push((*code).into());
            }
            TxStatus::Success => {}
        }
        let hits = match markers::detect(tx, &self.program) {
            Ok(h) => h,
            Err(code) => {
                base.classification = "UNKNOWN".into();
                base.missing_reasons.push(code.into());
                vec![]
            }
        };
        let pairs = markers::pairs(&hits, &status);
        for p in &pairs {
            if p.status == markers::PairStatus::IncompleteOrAmbiguous && outcome == Outcome::Success
            {
                base.missing_reasons.push(format!(
                    "START_END_PAIR_UNPROVEN_{}",
                    p.liquidation_type.as_str()
                ));
            }
        }
        base.pairs = pairs.clone();
        for h in &hits {
            let mut key = EntityKey::chain(signature, slot, RecordKind::InstructionMarker);
            key.position = Some(h.position);
            key.liquidation_type = Some(h.marker.liquidation_type());
            let mut ev = base.clone();
            ev.markers = vec![h.marker];
            ev.pairs = pairs
                .iter()
                .filter(|p| p.liquidation_type == h.marker.liquidation_type())
                .cloned()
                .collect();
            ev.target = Some(h.target.to_string());
            ev.actor = Some(h.actor.to_string());
            self.add(key, ev)?;
        }
        if source == Source::Strict {
            // Strict related rows and events come from the verified batch facts.
            if !hits.is_empty() {
                self.strict_markers
                    .insert((signature.to_string(), slot), (hits, pairs));
            }
        } else {
            match related::decode(tx, &self.program, slot, transaction_index) {
                Ok(rows) => {
                    for r in &rows {
                        self.related(source, r)?;
                    }
                }
                Err(code) => base.missing_reasons.push(code.into()),
            }
            let logs = tx.meta.as_ref().and_then(|m| m.log_messages.as_deref());
            self.log_events(source, signature, slot, &base, logs, &hits)?;
        }
        self.add(
            EntityKey::chain(signature, slot, RecordKind::Transaction),
            base,
        )
    }

    /// Liquidation events of a live/unverified transaction from its logs, in log order (the
    /// canonical classifier's `event_index`). Never confirmed: no instruction/CPI proof.
    fn log_events(
        &mut self,
        source: Source,
        signature: &str,
        slot: u64,
        base: &Evidence,
        logs: Option<&[String]>,
        hits: &[MarkerHit],
    ) -> Result<(), String> {
        let _ = source;
        if base.outcome != Outcome::Success {
            return Ok(());
        }
        let Some(logs) = logs else {
            return Ok(());
        };
        let decoded = decode_receiver_events(logs, &self.program);
        let mut caveats = vec!["CHAIN_EXECUTION_UNQUALIFIED".to_string()];
        if decoded.decoder_errors > 0 {
            caveats.push("LOG_DECODER_ERROR".into());
        }
        if decoded.truncated {
            caveats.push("LOG_TRUNCATED".into());
        }
        for (index, r) in decoded.sequence.iter().enumerate() {
            let (kind, target, actor, edge, flat) = match *r {
                LogEventRef::Receiver(i) => {
                    let e = &decoded.events[i];
                    (
                        LiquidationType::Receivership,
                        e.target.to_string(),
                        e.receiver.to_string(),
                        Some(e.gross_edge_usd()),
                        Some(u64::from(e.protocol_flat_fee_lamports)),
                    )
                }
                LogEventRef::Additional(i) => {
                    let e = &decoded.additional_events[i];
                    (
                        e.kind,
                        e.target.clone(),
                        e.actor.clone(),
                        e.gross_edge_usd,
                        None,
                    )
                }
            };
            let mut key = EntityKey::chain(signature, slot, RecordKind::Liquidation);
            key.event_index = Some(index as u32);
            key.liquidation_type = Some(kind);
            let mut ev = base.clone();
            ev.markers = completion_marker(hits, kind, &target).into_iter().collect();
            ev.pairs.retain(|p| p.liquidation_type == kind);
            ev.target = Some(target);
            ev.actor = Some(actor);
            ev.market_gross_edge_usd = edge.filter(|v| v.is_finite());
            ev.protocol_fee_lamports = flat;
            ev.permissionless_profit_opportunity = Some(matches!(
                kind,
                LiquidationType::Classic | LiquidationType::Receivership
            ));
            ev.missing_reasons.extend(caveats.iter().cloned());
            self.add(key, ev)?;
        }
        Ok(())
    }

    /// A canonical event of verified strict evidence.
    fn canonical(&mut self, e: &TruthEvent) -> Result<(), String> {
        let mut key = EntityKey::chain(&e.signature, e.slot, RecordKind::Liquidation);
        key.event_index = Some(e.event_index);
        key.liquidation_type = Some(e.liquidation_type);
        let mut ev = Evidence::new(Source::Strict, Outcome::Success, "CLASSIFIED");
        ev.transaction_index = Some(u64::from(e.transaction_index));
        ev.block_time = e.block_time;
        if let Some((hits, pairs)) = self.strict_markers.get(&(e.signature.clone(), e.slot)) {
            ev.markers = completion_marker(hits, e.liquidation_type, &e.target)
                .into_iter()
                .collect();
            ev.pairs = pairs
                .iter()
                .filter(|p| p.liquidation_type == e.liquidation_type)
                .cloned()
                .collect();
        }
        if ev.markers.is_empty() {
            ev.missing_reasons.push("MARKER_POSITION_UNRESOLVED".into());
        }
        ev.target = Some(e.target.clone());
        ev.actor = Some(e.liquidator.clone());
        ev.asset_mint = e.asset_mint.clone();
        ev.liability_mint = e.liability_mint.clone();
        ev.transaction_fee_lamports = e.transaction_fee_lamports;
        ev.priority_fee_lamports = e.priority_fee_lamports;
        ev.protocol_fee_lamports = e.protocol_fee_lamports;
        ev.market_gross_edge_usd = e.gross_edge_usd.filter(|v| v.is_finite());
        ev.permissionless_profit_opportunity = Some(e.permissionless_profit_opportunity);
        ev.chain_liquidation_confirmed = true;
        if e.permissionless_profit_opportunity {
            ev.winner = Some(e.liquidator.clone());
        }
        if e.winner_net_edge_complete {
            ev.realized_pnl_usd = e.winner_net_edge_usd.filter(|v| v.is_finite());
        }
        self.add(key, ev)
    }

    fn related(&mut self, source: Source, r: &MarketRelated) -> Result<(), String> {
        let mut key = EntityKey::chain(&r.signature, r.slot, RecordKind::RelatedNonLiquidation);
        key.position = Some((r.outer_index, r.inner_index));
        key.related = Some(r.kind);
        let outcome = match r.status {
            related::RelatedStatus::Success => Outcome::Success,
            related::RelatedStatus::Failed => Outcome::Failed,
            related::RelatedStatus::Unknown(_) => Outcome::Unknown,
        };
        let mut ev = Evidence::new(source, outcome, "RELATED_NON_LIQUIDATION");
        ev.transaction_index = r.transaction_index;
        ev.related_status = Some(r.status.as_str().into());
        ev.target = r.target.map(|k| k.to_string());
        ev.actor = r.actor.map(|k| k.to_string());
        self.add(key, ev)
    }

    fn shadow_link(&mut self, row: &CanonicalJoin, origin: ShadowOrigin) -> Result<(), String> {
        let facts = joined_facts(row);
        self.charge(size(&facts))?;
        let m = &row.market;
        let key = (
            m.signature.clone(),
            m.slot,
            m.event_index,
            m.liquidation_type,
            m.target.clone(),
        );
        self.shadow
            .entry(key)
            .or_insert(ShadowLink { origin, facts });
        Ok(())
    }

    fn finish(mut self, live_coverage: &str) -> Vec<Record> {
        let coverage_caveat: Vec<String> = (live_coverage != "LIVE_COVERAGE_COMPLETE")
            .then(|| live_coverage.to_string())
            .into_iter()
            .collect();
        let mut out = Vec::new();
        let entities = std::mem::take(&mut self.entities);
        for (key, mut sources) in entities {
            sources.sort_by(|a, b| b.source.cmp(&a.source));
            let factual: Vec<&Evidence> = sources
                .iter()
                .filter(|e| e.source != Source::Unverified)
                .collect();
            let fallback: Vec<&Evidence> = if factual.is_empty() {
                sources.iter().collect()
            } else {
                factual
            };
            let mut best = sources[0].clone();
            // Weaker ACTUAL evidence fills missing facts; it never upgrades an outcome,
            // classification or confirmation of the stronger source. Shadow is separate.
            macro_rules! fill {
                ($($f:ident),*) => {$(
                    if best.$f.is_none() {
                        best.$f = fallback.iter().find_map(|e| e.$f.clone());
                    }
                )*};
            }
            fill!(
                transaction_index,
                block_time,
                asset_mint,
                liability_mint,
                transaction_fee_lamports,
                priority_fee_lamports,
                protocol_fee_lamports,
                market_gross_edge_usd
            );
            let (sig, slot) = (
                key.signature.clone().unwrap_or_default(),
                key.slot.unwrap_or(0),
            );
            let shadow = match (
                key.kind,
                key.event_index,
                key.liquidation_type,
                &best.target,
            ) {
                (RecordKind::Liquidation, Some(i), Some(t), Some(target)) => self
                    .shadow
                    .get(&(sig.clone(), slot, i, t, target.clone()))
                    .cloned(),
                _ => None,
            };
            if let Some(d) = shadow.as_ref().and_then(|s| s.facts.record_digest) {
                self.attached.insert(d);
            }
            let mut missing = vec![];
            if key.kind == RecordKind::Liquidation {
                for (name, unknown) in [
                    ("UNKNOWN_ASSET_MINT", best.asset_mint.is_none()),
                    ("UNKNOWN_LIABILITY_MINT", best.liability_mint.is_none()),
                    ("UNKNOWN_REALIZED_PNL_USD", best.realized_pnl_usd.is_none()),
                    (
                        "UNKNOWN_EXECUTABLE_NET_EDGE",
                        shadow
                            .as_ref()
                            .and_then(|s| s.facts.executable_net_usd_micro)
                            .is_none(),
                    ),
                ] {
                    if unknown {
                        missing.push(name.to_string());
                    }
                }
            }
            missing.extend(coverage_caveat.iter().cloned());
            out.push(Record {
                entity_id: key.entity_id(),
                live_t_rx_unix_ns: self.receipts.get(&(sig, slot)).copied(),
                key,
                best,
                sources,
                shadow,
                shadow_unix_ns: None,
                missing_reasons: missing,
            });
        }
        // Shadow-only: evaluations not attached to any chain entity (HYPOTHETICAL).
        let shadow_only = std::mem::take(&mut self.shadow_only);
        for (d, (link, at, target, signature, slot)) in shadow_only {
            if self.attached.contains(&d) {
                continue;
            }
            let mut best = Evidence::new(Source::Shadow, Outcome::Unknown, "HYPOTHETICAL");
            best.target = Some(target);
            best.missing_reasons.push("NO_CHAIN_ENTITY_PROOF".into());
            let key = EntityKey {
                signature,
                slot,
                kind: RecordKind::ShadowOnly,
                position: None,
                event_index: None,
                liquidation_type: None,
                related: None,
                shadow_record: Some(d),
            };
            out.push(Record {
                entity_id: key.entity_id(),
                key,
                best,
                sources: vec![],
                live_t_rx_unix_ns: None,
                shadow: Some(link),
                shadow_unix_ns: at,
                missing_reasons: coverage_caveat.clone(),
            });
        }
        out.sort_by(|a, b| a.entity_id.cmp(&b.entity_id));
        out
    }
}

/// The completion marker of an event: the one marker instruction of its class and target in the
/// same transaction; none when absent or ambiguous (a label, never a classification input).
fn completion_marker(hits: &[MarkerHit], kind: LiquidationType, target: &str) -> Option<Marker> {
    let mut found = hits.iter().filter(|h| {
        !h.marker.is_start()
            && h.marker.liquidation_type() == kind
            && h.target.to_string() == target
    });
    match (found.next(), found.next()) {
        (Some(h), None) => Some(h.marker),
        _ => None,
    }
}

/// A verification result of this capture, bound by an attempt to the current capture evidence.
/// The strict verification result a report or backtest reads, bound to its capture.
pub(crate) struct Selected {
    pub(crate) spec_id: String,
    pub(crate) result_id: String,
    pub(crate) result: StrictResult,
    pub(crate) dir: PathBuf,
    /// Last slot of the result's verified batches (later batches belong to other results).
    pub(crate) last_slot: Option<u64>,
}

/// `wanted`, else the latest result bound to the capture's evidence (`None`: no result).
pub(crate) fn select_result(
    c: &CaptureBinding,
    wanted: Option<&str>,
) -> Result<Option<Selected>, String> {
    let root = c.run.join("verification");
    let mut found: Vec<(u64, Selected)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return match wanted {
            Some(_) => Err("RESULT_NOT_FOUND_OR_AMBIGUOUS".into()),
            None => Ok(None),
        };
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for dir in dirs {
        let Some(spec_id) = dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(NATIVE_PREFIX))
            .map(str::to_string)
        else {
            continue;
        };
        let spec: strict_driver::VerificationSpec =
            journal::read_typed_value(&dir.join("spec.p0j"))
                .map_err(|_| "RESULT_SPEC_BINDING_MISSING")?;
        if spec.id()? != spec_id {
            return Err("RESULT_SPEC_DIGEST_MISMATCH".into());
        }
        if spec.capture_run_id != c.manifest.capture_run_id {
            return Err("RESULT_CAPTURE_MISMATCH".into());
        }
        // Attempts bind a result to the capture evidence it verified.
        let mut bound: BTreeMap<String, u64> = BTreeMap::new();
        if let Ok(attempts) = std::fs::read_dir(dir.join("attempts")) {
            for a in attempts.flatten() {
                let Ok(end) = journal::read_typed_value::<strict_driver::AttemptRecord>(
                    &a.path().join("end.p0j"),
                ) else {
                    continue;
                };
                if let Some(rid) = end.verification_result_id {
                    if end.capture_evidence_digest == c.evidence_digest
                        && end.verification_spec_id == spec_id
                    {
                        let at = bound.entry(rid).or_insert(0);
                        *at = (*at).max(end.at_unix_ns);
                    }
                }
            }
        }
        let strict_dir = dir.join("strict");
        let Ok(results) = std::fs::read_dir(strict_dir.join("results")) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = results.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let Some(rid) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.strip_suffix(".p0j"))
                .map(str::to_string)
            else {
                continue;
            };
            if wanted.is_some_and(|w| w != rid) {
                continue;
            }
            let result: StrictResult =
                journal::read_typed_value(&path).map_err(|_| "RESULT_INVALID")?;
            let bytes = super::payload::encode(&result).map_err(|e| e.to_string())?;
            if journal::hex(&solana_sdk::hash::hash(&bytes).to_bytes()) != rid
                || result.verification_spec_id != spec_id
            {
                return Err("RESULT_DIGEST_MISMATCH".into());
            }
            let Some(at) = bound.get(&rid).copied() else {
                if wanted.is_some() {
                    return Err("RESULT_CAPTURE_MISMATCH".into());
                }
                continue;
            };
            let mut last_slot = None;
            for b in &result.batches {
                let p = strict_dir.join(&b.checkpoint);
                if b.checkpoint.contains('/') || journal::file_sha256(&p).ok() != Some(b.sha256) {
                    return Err("RESULT_EVIDENCE_CHANGED".into());
                }
                let end = b
                    .checkpoint
                    .strip_prefix("batch_")
                    .and_then(|s| s.strip_suffix(".p0j"))
                    .and_then(|s| s.split_once('_'))
                    .and_then(|(_, e)| e.parse::<u64>().ok())
                    .ok_or("RESULT_EVIDENCE_CHANGED")?;
                last_slot = Some(last_slot.map_or(end, |l: u64| l.max(end)));
            }
            found.push((
                at,
                Selected {
                    spec_id: spec_id.clone(),
                    result_id: rid,
                    result,
                    dir: strict_dir.clone(),
                    last_slot,
                },
            ));
        }
    }
    if wanted.is_some() && found.len() != 1 {
        return Err("RESULT_NOT_FOUND_OR_AMBIGUOUS".into());
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.result_id.cmp(&b.1.result_id)));
    Ok(found.pop().map(|(_, s)| s))
}

fn read_chain<F: FnMut(journal::Frame) -> Result<(), String>>(
    path: &Path,
    mut f: F,
) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let mut r = ChainReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    while let Some(frame) = r
        .next_frame()
        .map_err(|e| format!("{}: {e}", path.display()))?
    {
        if frame.kind != journal::MANIFEST {
            f(frame)?;
        }
    }
    Ok(true)
}

fn load_live(b: &mut Builder, market: &Path) -> Result<(), String> {
    let found = read_chain(market, |frame| {
        match frame.kind {
            kind::MARKET_TRANSACTION => {
                let row: MarketTransaction = frame.typed().map_err(|_| "LIVE_FRAME_INVALID")?;
                if let Some(t) = row.t_rx_ns() {
                    b.receipt(&row.signature, row.slot, t);
                }
                match &row.origin {
                    TxOrigin::TransactionSubscribe { tx: Ok(tx), .. } => {
                        b.chain_tx(Source::Live, &row.signature, row.slot, tx, None, None)?
                    }
                    TxOrigin::TransactionSubscribe { tx: Err(code), .. } => {
                        let mut ev = Evidence::new(Source::Live, Outcome::Unknown, "UNKNOWN");
                        ev.missing_reasons.push(code.clone());
                        b.add(
                            EntityKey::chain(&row.signature, row.slot, RecordKind::Transaction),
                            ev,
                        )?
                    }
                    TxOrigin::LogsSubscribe { logs, .. } => {
                        let mut ev =
                            Evidence::new(Source::Live, Outcome::of(&logs.status), "UNKNOWN");
                        ev.missing_reasons
                            .push("LIVE_INSTRUCTION_VIEW_UNAVAILABLE".into());
                        b.log_events(
                            Source::Live,
                            &row.signature,
                            row.slot,
                            &ev,
                            logs.logs.as_deref(),
                            &[],
                        )?;
                        b.add(
                            EntityKey::chain(&row.signature, row.slot, RecordKind::Transaction),
                            ev,
                        )?
                    }
                    // Finalized block entries recovered by the capture are not live receipts and
                    // not verified evidence of the selected result.
                    TxOrigin::GapBackfill {
                        transaction_index,
                        tx,
                    } => b.chain_tx(
                        Source::Unverified,
                        &row.signature,
                        row.slot,
                        tx,
                        Some(*transaction_index),
                        None,
                    )?,
                    TxOrigin::FullSlot {
                        transaction_index,
                        block_time,
                        tx,
                        ..
                    } => b.chain_tx(
                        Source::Unverified,
                        &row.signature,
                        row.slot,
                        tx,
                        Some(*transaction_index),
                        *block_time,
                    )?,
                }
            }
            // A2: of the coverage only the capture's commitment and its slot receipts.
            kind::MARKET_COVERAGE if frame.schema != 1 => {
                if b.race.note(&frame)? {
                    b.charge(16)?;
                }
            }
            // Receiver rows are derived from the same logs; related rows of the capture are not
            // needed: the report decodes the transactions themselves.
            kind::MARKET_LIQUIDATION | kind::MARKET_COVERAGE | kind::MARKET_RELATED => {}
            k => return Err(format!("LIVE_FRAME_KIND {k}")),
        }
        Ok(())
    })?;
    if !found {
        return Err("LIVE_MARKET_JOURNAL_MISSING".into());
    }
    Ok(())
}

/// The selected result's verified canonical events and related rows up to its last verified
/// batch, fail closed on changed or (for a CANONICAL_VERIFIED result) incomplete evidence;
/// `on_tx` sees each verified transaction row first.
pub(crate) fn verified_events(
    sel: &Selected,
    on_tx: &mut dyn FnMut(&MarketTransaction) -> Result<(), String>,
) -> Result<(Vec<TruthEvent>, Vec<MarketRelated>), String> {
    let Some(limit) = sel.last_slot else {
        return Ok((vec![], vec![]));
    };
    let mut failure: Option<String> = None;
    let report = strict::read_report_with(&sel.dir, &mut |row: &MarketTransaction| {
        if failure.is_none() && row.slot <= limit {
            failure = on_tx(row).err();
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    if let Some(reason) = report.reason {
        if report.next_unverified_slot.is_some_and(|s| s <= limit) {
            return Err(format!("STRICT_EVIDENCE_CHANGED: {reason}"));
        }
    }
    if sel.result.status == StrictStatus::CanonicalVerified && !report.complete() {
        return Err("STRICT_EVIDENCE_INCOMPLETE".into());
    }
    let events = report
        .canonical
        .into_iter()
        .filter(|e| e.slot <= limit)
        .collect();
    let related = report
        .related
        .into_iter()
        .filter(|r| r.slot <= limit)
        .collect();
    Ok((events, related))
}

/// The events whose chain block time lies in the capture window `[start, end)` (unix ns).
pub(crate) fn window_events(events: Vec<TruthEvent>, (start, end): (u64, u64)) -> Vec<TruthEvent> {
    events
        .into_iter()
        .filter(|e| {
            e.block_time.is_some_and(|t| {
                let ns = i128::from(t) * NS;
                ns >= i128::from(start) && ns < i128::from(end)
            })
        })
        .collect()
}

/// Facts of the selected result's verified batches; returns its canonical events.
fn load_strict(b: &mut Builder, sel: &Selected) -> Result<Vec<TruthEvent>, String> {
    let (events, related) = verified_events(sel, &mut |row| match &row.origin {
        TxOrigin::FullSlot {
            transaction_index,
            block_time,
            tx,
            ..
        } => b.chain_tx(
            Source::Strict,
            &row.signature,
            row.slot,
            tx,
            Some(*transaction_index),
            *block_time,
        ),
        _ => Ok(()),
    })?;
    for e in &events {
        b.canonical(e)?;
    }
    for r in &related {
        b.related(Source::Strict, r)?;
    }
    Ok(events)
}

fn load_capture_canonical(b: &mut Builder, reports: &Path) -> Result<(), String> {
    read_chain(reports, |frame| {
        if frame.kind == kind::ECON_CANONICAL && frame.schema != 1 {
            let row: CanonicalJoin = frame.typed().map_err(|_| "ECON_CANONICAL_INVALID")?;
            b.shadow_link(&row, ShadowOrigin::CaptureCanonical)?;
        }
        Ok(())
    })
    .map(|_| ())
}

fn load_opportunities(b: &mut Builder, reports: &Path) -> Result<(), String> {
    read_chain(reports, |frame| {
        if frame.kind == kind::ECON_OPPORTUNITY && frame.schema != 1 {
            let row: OpportunityRow = frame.typed().map_err(|_| "ECON_OPPORTUNITY_INVALID")?;
            let rec = &row.rec;
            let d = record_digest(rec).ok_or("ECON_OPPORTUNITY_INVALID")?;
            let facts = JoinedFacts {
                identity_status: "NO_CHAIN_ENTITY".into(),
                race_class: "UNRESOLVED".into(),
                local_observed_race_margin_ns: None,
                local_slot_race_margin_ns: None,
                loss_reason: "UNKNOWN".into(),
                loss_provenance: "SHADOW_ONLY".into(),
                signed: Some(rec.timing.t_sign_ns.is_some()),
                simulation_result: rec.simulation_result.clone(),
                economics_known: Some(rec.economics_known),
                // No chain entity proves an executable identity: UNKNOWN, never a profit.
                executable_net_usd_micro: None,
                required_working_capital_usd_micro: None,
                required_total_budget_usd_micro: None,
                record_digest: Some(d),
            };
            b.charge(size(&facts))?;
            let at = rec
                .timing
                .t_rx_ns
                .map(|t| b.clock_origin_unix_ns + u128::from(t));
            let entry = (
                ShadowLink {
                    origin: ShadowOrigin::Opportunity,
                    facts,
                },
                at,
                rec.account.clone(),
                rec.signature.clone(),
                rec.sign_slot.or(rec.rx_slot),
            );
            b.shadow_only.entry(d).or_insert(entry);
        }
        Ok(())
    })
    .map(|_| ())
}

/// Builds (or re-checks) the normalized dataset of a V2 run with native capture records.
pub fn normalize(run: &Path, opts: &Options) -> Result<Dataset, String> {
    let deadline = Instant::now() + Duration::from_secs(opts.max_seconds);
    let capture = strict_driver::bind_capture(run)?;
    let market_dir = capture.run.join("capture/market");
    let completion: MarketCompletion =
        journal::read_typed_value(&market_dir.join(MARKET_COMPLETION_JOURNAL)).map_err(|_| {
            "CAPTURE_NOT_FINALIZED: no MARKET_COMPLETION record (capture before B7: use the pinned report)"
        })?;
    let selected = select_result(&capture, opts.verification_result.as_deref())?;
    let verification_scope_end = selected
        .as_ref()
        .and_then(|s| s.result.scope.as_ref().map(|scope| scope.hi));
    let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET)
        .map_err(|e| e.to_string())?;
    let mut b = Builder {
        program,
        clock_origin_unix_ns: capture.manifest.clock_origin_unix_ns,
        entities: BTreeMap::new(),
        receipts: BTreeMap::new(),
        shadow: BTreeMap::new(),
        shadow_only: BTreeMap::new(),
        attached: BTreeSet::new(),
        strict_markers: BTreeMap::new(),
        race: RaceInputs::default(),
        bytes: 0,
        max_bytes: opts.max_ram_bytes,
        deadline,
    };
    load_live(&mut b, &market_dir.join(MARKET_JOURNAL))?;
    let reports = capture.run.join("snapshots").join(ECON_REPORT_JOURNAL);
    let (economics_replay, economics) = match &selected {
        Some(sel) => {
            let window = window_events(load_strict(&mut b, sel)?, capture.window);
            let n = window.len();
            let shadow = capture.run.join("capture/shadow_evaluations.p0j");
            let commitment = b
                .race
                .market_commitment
                .clone()
                .unwrap_or_else(|| "UNKNOWN".into());
            let slots = std::mem::take(&mut b.race.slot_first_seen);
            let joined = canonical_join(
                window,
                &shadow,
                deadline,
                &JoinContext {
                    market_commitment: &commitment,
                    slot_first_seen: Some(&slots),
                    // A3: only evaluations the live chain admits (`Worker::hour`).
                    capture: Some(CaptureFilter {
                        plan: &capture.manifest.plan,
                        start_ns: capture.monotonic_window.0,
                        cutoff_ns: capture.monotonic_window.1,
                    }),
                    // The native report renders no budget buckets of the joined rows.
                    capital: &crate::analytics::econ::CAPITAL_USD,
                },
            );
            match joined {
                Ok(j) => {
                    for row in &j.rows {
                        b.shadow_link(row, ShadowOrigin::StrictReplay)?;
                    }
                    (
                        format!("PASS events={n} shadow_available={}", j.shadow_available),
                        Economics::Replayed {
                            events: n as u64,
                            shadow_available: j.shadow_available,
                        },
                    )
                }
                Err(e) => (
                    format!("INCOMPLETE {e}"),
                    Economics::Incomplete(e.to_string()),
                ),
            }
        }
        None => {
            load_capture_canonical(&mut b, &reports)?;
            (
                "NOT_RUN: no strict verification result (capture-time canonical join only)".into(),
                Economics::NotRun,
            )
        }
    };
    load_opportunities(&mut b, &reports)?;
    let live_coverage = completion.live_coverage_status();
    let records = b.finish(live_coverage);
    capture.recheck("CAPTURE_EVIDENCE_CHANGED_DURING_REPORT")?;
    let mut h = solana_sdk::hash::Hasher::default();
    for r in &records {
        h.hash(
            &bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .serialize(r)
                .map_err(|e| e.to_string())?,
        );
    }
    let verification_status = selected
        .as_ref()
        .map_or("NOT_RUN", |s| s.result.status.as_str())
        .to_string();
    let identity = DatasetIdentity {
        renderer: RENDERER.into(),
        capture_run_id: capture.manifest.capture_run_id.clone(),
        capture_evidence_digest: capture.evidence_digest.clone(),
        verification_spec_id: selected.as_ref().map(|s| s.spec_id.clone()),
        verification_result_id: selected.as_ref().map(|s| s.result_id.clone()),
        report_binary_sha256: std::env::current_exe()
            .ok()
            .and_then(|p| journal::file_sha256(&p).ok())
            .unwrap_or([0; 32]),
    };
    let record = DatasetRecord {
        record_count: records.len() as u64,
        records_sha256: h.result().to_bytes(),
        report_status: if verification_status == "CANONICAL_VERIFIED" {
            "FINAL_VERIFICATION_RESULT"
        } else {
            "PROVISIONAL/INCOMPLETE"
        }
        .into(),
        verification_status,
        live_coverage_status: live_coverage.into(),
        capture_clock_complete: completion.capture_clock_complete(),
        economics_replay,
        c0_e1_master_pass: false,
        identity,
    };
    let dir = capture.run.join(REPORT_DIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!(
        "dataset-{}.p0j",
        journal::hex(&digest(&record.identity))
    ));
    journal::write_typed_value(&path, journal::source::REPORT, &record).map_err(|e| {
        if e.to_string().contains("IMMUTABLE_CONFLICT") {
            "NORMALIZED_DATASET_CHANGED".to_string()
        } else {
            format!("REPORT_DATASET: {e}")
        }
    })?;
    Ok(Dataset {
        record,
        records,
        path,
        economics,
        plan: capture.manifest.plan.clone(),
        capture_journal_complete: crate::storage::segments::verify_chain(
            &market_dir.join(MARKET_JOURNAL),
        )
        .map(|c| c.sealed && c.complete)?,
        completion,
        verification_scope_end,
    })
}

// ------------------------------------------------------------------------------------------
// View (filters on the display only) and renderers
// ------------------------------------------------------------------------------------------

/// One displayed row: a record seen through the query (the record itself is never changed).
pub struct Row<'a> {
    pub entity_id: String,
    pub kind: RecordKind,
    pub source: String,
    pub record: &'a Record,
    pub ev: Evidence,
    pub shadow: Option<&'a ShadowLink>,
    pub related_chain_entity: Option<&'a str>,
    pub markers: Vec<Marker>,
    pub net_known: bool,
}

pub fn visible<'a>(rec: &'a Record, q: &ReportQuery) -> Option<Row<'a>> {
    let mut kind = rec.key.kind;
    let mut entity_id = rec.entity_id.clone();
    let mut related_chain_entity = None;
    let (mut ev, source) = match q.provenance {
        ProvenanceView::Best | ProvenanceView::All => {
            if kind == RecordKind::ShadowOnly && q.provenance == ProvenanceView::Best {
                return None;
            }
            let label = if q.provenance == ProvenanceView::All && !rec.sources.is_empty() {
                rec.sources
                    .iter()
                    .map(|e| e.source.as_str())
                    .collect::<Vec<_>>()
                    .join("+")
            } else {
                rec.best.source.as_str().to_string()
            };
            (rec.best.clone(), label)
        }
        ProvenanceView::Live | ProvenanceView::Strict => {
            let want = if q.provenance == ProvenanceView::Live {
                Source::Live
            } else {
                Source::Strict
            };
            let ev = rec.sources.iter().find(|e| e.source == want)?.clone();
            (ev, want.as_str().to_string())
        }
        ProvenanceView::Shadow => {
            if kind != RecordKind::ShadowOnly {
                // The Shadow view of a chain entity: hypothetical, never its chain facts.
                rec.shadow.as_ref()?;
                related_chain_entity = Some(rec.entity_id.as_str());
                entity_id = format!("shadow-view-{}", rec.entity_id);
                kind = RecordKind::ShadowOnly;
                let mut ev = Evidence::new(Source::Shadow, Outcome::Unknown, "HYPOTHETICAL");
                ev.target = rec.best.target.clone();
                (ev, "shadow".to_string())
            } else {
                (rec.best.clone(), "shadow".to_string())
            }
        }
    };
    match kind {
        RecordKind::RelatedNonLiquidation => {
            if !q.related_events
                || !rec
                    .key
                    .related
                    .is_some_and(|r| q.related_types.contains(&r))
            {
                return None;
            }
        }
        RecordKind::Liquidation | RecordKind::InstructionMarker => {
            if !rec
                .key
                .liquidation_type
                .is_some_and(|t| q.types.contains(&t))
            {
                return None;
            }
            if kind == RecordKind::InstructionMarker
                && !ev.markers.iter().any(|m| q.markers.contains(m))
            {
                return None;
            }
        }
        RecordKind::ShadowOnly | RecordKind::Transaction => {}
    }
    // Hiding a marker label never changes the event's class or removes the event.
    let markers: Vec<Marker> = ev
        .markers
        .iter()
        .copied()
        .filter(|m| q.markers.contains(m))
        .collect();
    if q.from_unix_ns.is_some() || q.to_unix_ns.is_some() {
        let (at, _) = rec.at_unix_ns(&ev)?;
        if q.from_unix_ns.is_some_and(|f| at < f) || q.to_unix_ns.is_some_and(|t| at >= t) {
            return None;
        }
    }
    let shadow = rec.shadow.as_ref();
    let net = shadow.and_then(|s| s.facts.executable_net_usd_micro);
    if let (Some(min), Some(n)) = (q.min_net_usd_micro, net) {
        if n < min {
            return None;
        }
    }
    let shown = match q.view {
        View::Summary => true,
        View::Liquidations => matches!(
            kind,
            RecordKind::Liquidation
                | RecordKind::InstructionMarker
                | RecordKind::RelatedNonLiquidation
        ),
        View::Winners => kind == RecordKind::Liquidation,
        View::Competition => matches!(kind, RecordKind::Liquidation | RecordKind::ShadowOnly),
        View::Economics => matches!(
            kind,
            RecordKind::Transaction | RecordKind::Liquidation | RecordKind::ShadowOnly
        ),
    };
    if !shown {
        return None;
    }
    if kind == RecordKind::ShadowOnly {
        ev.chain_liquidation_confirmed = false;
        ev.winner = None;
        ev.realized_pnl_usd = None;
    }
    Some(Row {
        entity_id,
        kind,
        source,
        record: rec,
        ev,
        shadow,
        related_chain_entity,
        markers,
        net_known: net.is_some(),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Always,
    Winners,
    Economics,
    Competition,
}

const COLUMNS: &[(&str, Group)] = &[
    ("entity_id", Group::Always),
    ("record_kind", Group::Always),
    ("source", Group::Always),
    ("signature", Group::Always),
    ("slot", Group::Always),
    ("transaction_index", Group::Always),
    ("time_utc", Group::Always),
    ("time_source", Group::Always),
    ("outcome", Group::Always),
    ("classification", Group::Always),
    ("liquidation_type", Group::Always),
    ("markers", Group::Always),
    ("pair_status", Group::Always),
    ("related_type", Group::Always),
    ("related_status", Group::Always),
    ("target", Group::Always),
    ("actor", Group::Always),
    ("chain_liquidation_confirmed", Group::Always),
    ("winner", Group::Winners),
    ("live_received_utc", Group::Always),
    ("market_gross_edge_usd", Group::Economics),
    ("transaction_fee_lamports", Group::Economics),
    ("priority_fee_lamports", Group::Economics),
    ("protocol_fee_lamports", Group::Economics),
    ("executable_shadow_net_edge_usd", Group::Economics),
    ("required_capital_usd", Group::Economics),
    ("realized_pnl_usd", Group::Economics),
    ("net_filter_status", Group::Always),
    ("race_class", Group::Competition),
    ("local_race_margin_ns", Group::Competition),
    ("loss_reason", Group::Competition),
    ("shadow_origin", Group::Always),
    ("shadow_identity", Group::Always),
    ("related_chain_entity", Group::Always),
    ("missing_reasons", Group::Always),
];

fn shown_columns(q: &ReportQuery) -> Vec<&'static str> {
    COLUMNS
        .iter()
        .filter(|(_, g)| match g {
            Group::Always => true,
            Group::Winners => q.winners,
            Group::Economics => q.economics,
            Group::Competition => q.competition,
        })
        .map(|(c, _)| *c)
        .collect()
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map_or("-".into(), |v| v.to_string())
}
fn usd_micro(v: Option<i128>) -> String {
    v.map_or("UNKNOWN".into(), |n| {
        let sign = if n < 0 { "-" } else { "" };
        let a = n.unsigned_abs();
        format!("{sign}{}.{:06}", a / 1_000_000, a % 1_000_000)
    })
}

fn cell(row: &Row, column: &str) -> String {
    let ev = &row.ev;
    let rec = row.record;
    let facts = row.shadow.map(|s| &s.facts);
    match column {
        "entity_id" => row.entity_id.clone(),
        "record_kind" => row.kind.as_str().into(),
        "source" => row.source.clone(),
        "signature" => opt(rec.key.signature.clone()),
        "slot" => opt(rec.key.slot),
        "transaction_index" => opt(ev.transaction_index),
        "time_utc" => opt(rec.at_unix_ns(ev).map(|(n, _)| utc_ns(n.max(0) as u128))),
        "time_source" => opt(rec.at_unix_ns(ev).map(|(_, s)| s)),
        "outcome" => ev.outcome.as_str().into(),
        "classification" => ev.classification.clone(),
        "liquidation_type" => opt(rec.key.liquidation_type.map(|t| t.as_str())),
        "markers" => {
            if row.markers.is_empty() {
                "-".into()
            } else {
                row.markers
                    .iter()
                    .map(|m| m.name())
                    .collect::<Vec<_>>()
                    .join("|")
            }
        }
        "pair_status" => {
            if ev.pairs.is_empty() {
                "-".into()
            } else {
                ev.pairs
                    .iter()
                    .map(|p| format!("{}:{}", p.liquidation_type.as_str(), p.status.as_str()))
                    .collect::<Vec<_>>()
                    .join("|")
            }
        }
        "related_type" => opt(rec.key.related.map(|r| r.name())),
        "related_status" => opt(ev.related_status.clone()),
        "target" => opt(ev.target.clone()),
        "actor" => opt(ev.actor.clone()),
        "chain_liquidation_confirmed" => ev.chain_liquidation_confirmed.to_string(),
        "winner" => opt(ev.winner.clone()),
        "live_received_utc" => opt(rec.live_t_rx_unix_ns.map(utc_ns)),
        "market_gross_edge_usd" => ev
            .market_gross_edge_usd
            .map_or("UNKNOWN".into(), |v| v.to_string()),
        "transaction_fee_lamports" => opt(ev.transaction_fee_lamports),
        "priority_fee_lamports" => opt(ev.priority_fee_lamports),
        "protocol_fee_lamports" => opt(ev.protocol_fee_lamports),
        "executable_shadow_net_edge_usd" => {
            usd_micro(facts.and_then(|f| f.executable_net_usd_micro))
        }
        "required_capital_usd" => usd_micro(
            facts
                .and_then(|f| f.required_working_capital_usd_micro)
                .map(i128::from),
        ),
        "realized_pnl_usd" => ev
            .realized_pnl_usd
            .map_or("UNKNOWN".into(), |v| v.to_string()),
        "net_filter_status" => if row.net_known { "KNOWN" } else { "UNKNOWN" }.into(),
        "race_class" => opt(facts.map(|f| f.race_class.clone())),
        "local_race_margin_ns" => opt(facts.and_then(|f| f.local_observed_race_margin_ns)),
        "loss_reason" => opt(facts.map(|f| f.loss_reason.clone())),
        "shadow_origin" => opt(row.shadow.map(|s| s.origin.as_str())),
        "shadow_identity" => opt(facts.map(|f| f.identity_status.clone())),
        "related_chain_entity" => opt(row.related_chain_entity),
        "missing_reasons" => {
            let mut all: Vec<&str> = Vec::new();
            for m in ev.missing_reasons.iter().chain(rec.missing_reasons.iter()) {
                if !all.contains(&m.as_str()) {
                    all.push(m);
                }
            }
            if all.is_empty() {
                "-".into()
            } else {
                all.join("|")
            }
        }
        _ => "-".into(),
    }
}

/// Header + data cells of every visible row (the one semantic source of both renderers).
pub fn cells(records: &[Record], q: &ReportQuery) -> (Vec<&'static str>, Vec<Vec<String>>) {
    let columns = shown_columns(q);
    let rows = records
        .iter()
        .filter_map(|r| visible(r, q))
        .map(|row| columns.iter().map(|c| cell(&row, c)).collect())
        .collect();
    (columns, rows)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn render_csv(columns: &[&str], rows: &[Vec<String>]) -> String {
    let mut out = columns.join(",");
    out.push('\n');
    for r in rows {
        out.push_str(&r.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

/// Aligned columns; cells are never truncated (the same strings as the CSV).
pub fn render_table(columns: &[&str], rows: &[Vec<String>]) -> String {
    let mut width: Vec<usize> = columns.iter().map(|c| c.chars().count()).collect();
    for r in rows {
        for (w, c) in width.iter_mut().zip(r) {
            *w = (*w).max(c.chars().count());
        }
    }
    let line = |cells: Vec<&str>| {
        let mut s = cells
            .iter()
            .zip(&width)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect::<Vec<_>>()
            .join("  ");
        s.truncate(s.trim_end().len());
        s.push('\n');
        s
    };
    let mut out = line(columns.to_vec());
    for r in rows {
        out.push_str(&line(r.iter().map(String::as_str).collect()));
    }
    out
}

/// Counts over the visible rows; every entity once. Related, marker and Shadow-only rows are
/// never liquidations, winners or profit.
pub fn aggregates(records: &[Record], q: &ReportQuery) -> String {
    let mut confirmed: BTreeMap<&str, u64> = BTreeMap::new();
    let mut unconfirmed: BTreeMap<&str, u64> = BTreeMap::new();
    let mut related: BTreeMap<(&str, String), u64> = BTreeMap::new();
    let mut marker: BTreeMap<&str, u64> = BTreeMap::new();
    let (mut shadow_only, mut tx, mut net_known, mut net_unknown) = (0u64, 0u64, 0u64, 0u64);
    let mut winners = BTreeSet::new();
    let mut rows = 0u64;
    for row in records.iter().filter_map(|r| visible(r, q)) {
        rows += 1;
        match row.kind {
            RecordKind::Liquidation => {
                let t = row
                    .record
                    .key
                    .liquidation_type
                    .map_or("UNKNOWN", |t| t.as_str());
                if row.ev.chain_liquidation_confirmed {
                    *confirmed.entry(t).or_default() += 1;
                } else {
                    *unconfirmed.entry(t).or_default() += 1;
                }
                if let Some(w) = &row.ev.winner {
                    winners.insert(w.clone());
                }
                if row.net_known {
                    net_known += 1;
                } else {
                    net_unknown += 1;
                }
            }
            RecordKind::RelatedNonLiquidation => {
                let k = row.record.key.related.map_or("UNKNOWN", |r| r.name());
                let s = row
                    .ev
                    .related_status
                    .clone()
                    .unwrap_or_else(|| "UNKNOWN".into());
                *related.entry((k, s)).or_default() += 1;
            }
            RecordKind::InstructionMarker => {
                for m in &row.markers {
                    *marker.entry(m.name()).or_default() += 1;
                }
            }
            RecordKind::ShadowOnly => {
                shadow_only += 1;
                if row.net_known {
                    net_known += 1;
                } else {
                    net_unknown += 1;
                }
            }
            RecordKind::Transaction => tx += 1,
        }
    }
    let map = |m: &BTreeMap<&str, u64>| {
        TYPES
            .iter()
            .map(|t| format!("{}={}", t.as_str(), m.get(t.as_str()).copied().unwrap_or(0)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut out = format!("rows_visible={rows} transactions={tx}\n");
    out.push_str(&format!(
        "liquidations_chain_confirmed {}\n",
        map(&confirmed)
    ));
    out.push_str(&format!("liquidations_unconfirmed {}\n", map(&unconfirmed)));
    for m in Marker::ALL {
        if let Some(n) = marker.get(m.name()) {
            out.push_str(&format!(
                "instruction_marker {}={n} (label, not a liquidation)\n",
                m.name()
            ));
        }
    }
    for ((k, s), n) in &related {
        out.push_str(&format!(
            "related_non_liquidation {k} {s}={n} (never liquidation/winner/profit)\n"
        ));
    }
    out.push_str(&format!(
        "shadow_only_hypothetical={shadow_only} (never chain liquidation/winner/profit)\n"
    ));
    if q.winners {
        out.push_str(&format!("distinct_chain_winners={}\n", winners.len()));
    }
    out.push_str(&format!(
        "executable_net_known={net_known} executable_net_unknown={net_unknown}\n"
    ));
    out
}

/// Result of one report invocation.
pub struct Output {
    /// The rendered rows (stdout).
    pub data: String,
    /// Identity, status axes and aggregates (stdout before a table, stderr with CSV).
    pub meta: String,
    pub query: QueryRecord,
    pub dataset: DatasetRecord,
}

pub fn query_identity(dataset: &DatasetRecord, q: &ReportQuery) -> String {
    journal::hex(&digest(&(
        &dataset.identity.capture_run_id,
        &dataset.identity.verification_result_id,
        dataset.records_sha256,
        q,
        RENDERER,
    )))
}

/// Normalizes (or re-checks) the dataset, records the query identity and renders the view.
pub fn report(run: &Path, opts: &Options, q: &ReportQuery) -> Result<Output, String> {
    let dataset = normalize(run, opts)?;
    let d = &dataset.record;
    let qid = query_identity(d, q);
    let query = QueryRecord {
        report_query_identity: qid.clone(),
        capture_run_id: d.identity.capture_run_id.clone(),
        verification_result_id: d.identity.verification_result_id.clone(),
        dataset_sha256: d.records_sha256,
        query: q.clone(),
        renderer: RENDERER.into(),
        report_status: d.report_status.clone(),
    };
    let dir = dataset.path.parent().ok_or("REPORT_DIR")?.to_path_buf();
    journal::write_typed_value(
        &dir.join(format!("query-{qid}.p0j")),
        journal::source::REPORT,
        &query,
    )
    .map_err(|e| format!("REPORT_QUERY: {e}"))?;
    let (columns, rows) = cells(&dataset.records, q);
    let data = match q.format {
        Format::Table => render_table(&columns, &rows),
        Format::Csv => render_csv(&columns, &rows),
    };
    let mut meta = String::from("PROJECT 0 C0-E1 REPORT (native .p0j evidence)\n");
    for (k, v) in [
        ("capture_run_id", d.identity.capture_run_id.clone()),
        (
            "verification_result_id",
            opt(d.identity.verification_result_id.clone()),
        ),
        ("verification_status", d.verification_status.clone()),
        ("report_status", d.report_status.clone()),
        ("live_coverage_status", d.live_coverage_status.clone()),
        (
            "capture_clock_complete",
            d.capture_clock_complete.to_string(),
        ),
        ("economics_replay", d.economics_replay.clone()),
        ("dataset_sha256", journal::hex(&d.records_sha256)),
        ("record_count", d.record_count.to_string()),
        ("report_query_identity", qid),
        ("renderer", RENDERER.into()),
    ] {
        meta.push_str(&format!("{k}={v}\n"));
    }
    meta.push_str(&aggregates(&dataset.records, q));
    meta.push_str("non_claims: filters change the view only; SHADOW is hypothetical; c0_e1_master_pass=false live_enabled=false\n");
    Ok(Output {
        data,
        meta,
        query,
        dataset: dataset.record,
    })
}
