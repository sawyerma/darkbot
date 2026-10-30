//! H0 before its CTRL-10 review (9a41e808): the layouts of kinds 360/361 and of its job result as
//! that commit wrote them — read-only, so its evidence stays readable (payload.rs). Nothing writes
//! them: the reviewed layouts are kinds 362/363 (`h0`). Verbatim copies; never change a field.
use super::journal::kind;
use super::payload::Payload;
use crate::market::observer::truth::TruthEvent;
use serde::{Deserialize, Serialize};

/// The window of an extraction: chain block time in `[start_unix_s, end_unix_s)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start_unix_s: i64,
    pub end_unix_s: i64,
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

impl Payload for H0Liquidation {
    const KIND: u16 = kind::H0_LIQUIDATION_PRE_REVIEW;
}
impl Payload for H0Report {
    const KIND: u16 = kind::H0_REPORT_PRE_REVIEW;
}
