//! Native backtest and optimizer (frontend STARTUP §4 C–F and §6; PHASENPLAN L.3–L.6, SK-09,
//! JFI-01/03): one bounded pass over a finalized capture's own typed `.p0j` evidence.
//!
//! Input (fail closed on anything missing, changed or incomplete): the run's capture binding
//! (`strict_driver::bind_capture`), a complete capture clock (`MarketCompletion`), the selected
//! CANONICAL_VERIFIED strict result's verified chain events in the capture window, the live
//! market journal's commitment and slot receipts (`report::RaceInputs`) and the sealed typed
//! Shadow evaluation chain `capture/shadow_evaluations.p0j`. Processing: the production join
//! (`canonical_join`), its typed row facts (`joined_facts`), the production executability rule
//! (`technical`) and admission (`economics::actionable`) — no second engine, no JSON or JSONL,
//! no historical input, no RPC, no re-simulation and no `f64` in any amount. Output: a sealed
//! typed journal under `reports/backtests/` (never part of the capture evidence); its result
//! hash covers the parameters, the rows, the attempts and the summary, so the same inputs and
//! parameters give the same hash.
//!
//! * OBSERVED_MARKET: verified chain liquidations only (counts, exact native lamport fees).
//! * COUNTERFACTUAL_SHADOW, never realized, landing UNPROVEN, safety NOT_EVALUATED (a Shadow
//!   record carries no safety decision): every executable evaluation (signed, simulation PASS,
//!   not handed off, known economics) the parameters admit is an attempt. Matched to a verified
//!   liquidation and signed in a slot before the winner's it is an admissible win; signed in or
//!   after the winner's slot it lost the race; without any verified liquidation it is a false
//!   positive. Both of the latter cost their recorded failed-attempt reserve; an unknown cost
//!   makes the scenario net UNKNOWN, never 0. Only the outcome of what was decided before the
//!   chain outcome counts — no evaluation is selected by what later landed.
//! * REALIZED_ONCHAIN: own finalized attempts only (`p0ctl fe04 accounting`); NOT_RUN here.
//!
//! Scenario sums assume no concurrent capital use and no interference between events of one
//! episode (neither proven). The optimizer searches a bounded explicit grid of these parameters
//! — never below the run's own minimum profit, under which nothing was evaluated — chooses on
//! the training part of the capture window only and scores the choice once on the later
//! holdout part. Its result is an offline Shadow candidate profile, never a LIVE setting.
use super::econ::{
    canonical_join, executable_costs, joined_facts, record_digest, CaptureFilter, JoinContext,
    JoinedFacts, ShadowEvaluation,
};
use super::journal::{self, kind, Provenance};
use super::measurement::RunManifest;
use super::payload::Payload;
use super::report::{self, RaceInputs};
use crate::market::observer::rows::{MarketCompletion, MARKET_COMPLETION_JOURNAL};
use crate::market::observer::strict_driver::{self, CaptureBinding};
use crate::market::observer::MARKET_JOURNAL;
use darkbot_project0::economics::{self, Evaluation, BPS};
use darkbot_project0::funding::FundingClass;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Result journals of backtests and optimizations (under `reports/`: never capture evidence).
pub const BACKTEST_DIR: &str = "reports/backtests";
pub const OPTIMIZE_DIR: &str = "reports/optimize";
/// Offline Shadow candidate profiles of a project (`p0ctl optimize --save-profile`).
pub const PROFILE_DIR: &str = "data/profiles";
/// USD-micro per USD (the unit of every amount).
const MICRO_PER_USD: u64 = 1_000_000;
/// The landing status of every counterfactual Shadow execution.
pub const LANDING: &str = "UNPROVEN";
/// The safety status of every counterfactual Shadow execution.
pub const SAFETY: &str =
    "NOT_EVALUATED: a Shadow record carries no safety decision (Shadow evaluates without a send gate)";
/// The realized section of a backtest: own finalized attempts live in the FE-04 journals.
pub const REALIZED_NOT_RUN: &str =
    "NOT_RUN: no own finalized attempt in a capture; realized results: p0ctl fe04 accounting";
/// Origins of a saved profile.
pub const ORIGIN_OPTIMIZER: &str = "OPTIMIZER_CANDIDATE";
pub const ORIGIN_OPERATOR: &str = "OPERATOR";

/// Explicit resource budget of one job (STARTUP §4 B `resource_budget`); no hidden default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub max_seconds: u64,
    /// Bytes held in memory: events, joined rows, receipts, rows and attempts (fail closed).
    pub max_ram_bytes: u64,
}

impl Budget {
    fn deadline(&self) -> Result<Instant, String> {
        if self.max_seconds == 0 || self.max_ram_bytes == 0 {
            return Err("BACKTEST_BUDGET_INVALID".into());
        }
        Instant::now()
            .checked_add(Duration::from_secs(self.max_seconds))
            .ok_or_else(|| "BACKTEST_BUDGET_INVALID".into())
    }
}

/// The bytes and the deadline of one job.
struct Meter {
    deadline: Instant,
    held: u64,
    max: u64,
}

impl Meter {
    fn time(&self) -> Result<(), String> {
        if Instant::now() >= self.deadline {
            return Err("BACKTEST_TIME_BUDGET".into());
        }
        Ok(())
    }
    fn charge<T: Serialize>(&mut self, v: &T) -> Result<(), String> {
        use bincode::Options;
        let n = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .serialized_size(v)
            .map_err(|e| e.to_string())?;
        self.charge_bytes(n)
    }
    fn charge_bytes(&mut self, n: u64) -> Result<(), String> {
        self.held = self.held.saturating_add(n);
        if self.held > self.max {
            return Err("BACKTEST_RAM_BUDGET".into());
        }
        self.time()
    }
}

/// The economic parameters applied to the recorded production evaluations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Params {
    /// `economics::actionable`: a known net must exceed it (USD-micro, never negative).
    pub min_profit_usd_micro: i128,
    /// Capital scenarios (USD-micro), strictly ascending.
    pub capital_usd_micro: Vec<u64>,
    /// Admitted funding classes (`FundingClass::as_str`), strictly ascending; empty: every class.
    pub funding: Vec<String>,
}

impl Params {
    pub fn validate(&self) -> Result<(), String> {
        if self.min_profit_usd_micro < 0 {
            return Err("BACKTEST_MIN_PROFIT_NEGATIVE".into());
        }
        if self.capital_usd_micro.is_empty()
            || self.capital_usd_micro.windows(2).any(|w| w[0] >= w[1])
            || self.capital_usd_micro[0] == 0
        {
            return Err("BACKTEST_CAPITAL_INVALID".into());
        }
        if self.funding.windows(2).any(|w| w[0] >= w[1])
            || self
                .funding
                .iter()
                .any(|f| !FundingClass::ALL.iter().any(|c| c.as_str() == f))
        {
            return Err("BACKTEST_FUNDING_INVALID".into());
        }
        Ok(())
    }
}

/// One verified chain liquidation of the capture window with the Shadow facts joined to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacktestRow {
    pub signature: String,
    pub slot: u64,
    pub transaction_index: u32,
    pub event_index: u32,
    pub liquidation_type: String,
    pub target: String,
    pub liquidator: String,
    pub block_time: Option<i64>,
    pub protocol_fee_lamports: Option<u64>,
    pub transaction_fee_lamports: Option<u64>,
    pub priority_fee_lamports: Option<u64>,
    /// The production join's typed facts (Shadow evidence, never a chain fact).
    pub facts: JoinedFacts,
    /// Funding class, profit per capital and failed-attempt reserve of the matched evaluation's
    /// selected candidate.
    pub funding: Option<String>,
    pub profit_per_capital_bps: Option<i128>,
    pub failed_attempt_reserve_usd_micro: Option<u64>,
}
impl Payload for BacktestRow {
    const KIND: u16 = kind::BACKTEST_ROW;
}

/// An executable evaluation of the capture window that matched no verified liquidation: had
/// it been sent it would have failed (a false positive) and cost its failed-attempt reserve.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub account: String,
    pub sign_slot: Option<u64>,
    /// Monotone receipt time of the evaluation (the capture clock).
    pub t_rx_ns: u64,
    pub funding: String,
    pub net_usd_micro: i128,
    pub profit_per_capital_bps: Option<i128>,
    pub required_total_budget_usd_micro: Option<u64>,
    pub failed_attempt_reserve_usd_micro: Option<u64>,
    pub record_digest: [u8; 32],
}
impl Payload for Attempt {
    const KIND: u16 = kind::BACKTEST_ATTEMPT;
}

/// What one parameter set makes of an evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// No exact-plan-match evaluation of a verified liquidation.
    NotMatched,
    /// Matched, but not executable, or its economics or budget UNKNOWN.
    Unknown,
    FundingExcluded,
    BelowMinimum,
    /// Signed in a slot before the winner's: an admissible win (net, required budget).
    Win {
        net: i128,
        budget: u64,
    },
    /// Signed in or after the winner's slot: a lost race at its failed-attempt cost.
    LostRace {
        budget: u64,
        cost: Option<u64>,
    },
    /// No verified liquidation: a false positive at its failed-attempt cost.
    NoChainEvent {
        budget: u64,
        cost: Option<u64>,
    },
}

/// The production admission of one executable evaluation (economics only: the record's
/// execution and state facts are its own; safety is NOT_EVALUATED, see `SAFETY`). `Ok`: the
/// required total budget.
fn admit(
    funding: Option<&str>,
    net: i128,
    profit_per_capital_bps: Option<i128>,
    budget: Option<u64>,
    p: &Params,
) -> Result<u64, Outcome> {
    if !p.funding.is_empty() && !funding.is_some_and(|c| p.funding.iter().any(|f| f == c)) {
        return Err(Outcome::FundingExcluded);
    }
    let Some(profit_per_capital_bps) = profit_per_capital_bps else {
        return Err(Outcome::Unknown);
    };
    let evaluation = Evaluation::Known {
        net,
        profit_per_capital_bps,
    };
    if !economics::actionable(evaluation, p.min_profit_usd_micro, true, true, true) {
        return Err(Outcome::BelowMinimum);
    }
    budget.ok_or(Outcome::Unknown)
}

/// One verified liquidation under `p`.
pub fn outcome(row: &BacktestRow, p: &Params) -> Outcome {
    let f = &row.facts;
    if f.record_digest.is_none() {
        return Outcome::NotMatched;
    }
    let Some(net) = f.executable_net_usd_micro else {
        return Outcome::Unknown;
    };
    let budget = match admit(
        row.funding.as_deref(),
        net,
        row.profit_per_capital_bps,
        f.required_total_budget_usd_micro,
        p,
    ) {
        Ok(b) => b,
        Err(o) => return o,
    };
    if f.race_class == "PRE_WINNER_SLOT_PROVEN" {
        Outcome::Win { net, budget }
    } else {
        Outcome::LostRace {
            budget,
            cost: row.failed_attempt_reserve_usd_micro,
        }
    }
}

/// One false-positive evaluation under `p`.
pub fn attempt_outcome(a: &Attempt, p: &Params) -> Outcome {
    match admit(
        Some(&a.funding),
        a.net_usd_micro,
        a.profit_per_capital_bps,
        a.required_total_budget_usd_micro,
        p,
    ) {
        Ok(budget) => Outcome::NoChainEvent {
            budget,
            cost: a.failed_attempt_reserve_usd_micro,
        },
        Err(o) => o,
    }
}

/// OBSERVED_MARKET: verified chain liquidations of the window.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observed {
    pub liquidations: u64,
    pub by_type: BTreeMap<String, u64>,
    /// Exact sums over the rows that carry the fee; the others are counted as unknown.
    pub protocol_fee_lamports: u128,
    pub protocol_fee_unknown: u64,
    pub transaction_fee_lamports: u128,
    pub transaction_fee_unknown: u64,
}

/// One capital scenario of the counterfactual Shadow executions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    pub capital_usd_micro: u64,
    pub wins: u64,
    pub win_net_usd_micro: i128,
    /// Lost races and false positives within the capital.
    pub failed_attempts: u64,
    pub failed_cost_usd_micro: u128,
    /// Failed attempts within the capital whose cost is unknown.
    pub failed_cost_unknown: u64,
    pub over_budget: u64,
    /// Wins minus failed costs; `None` when a failed cost is unknown (never 0).
    pub net_usd_micro: Option<i128>,
}

/// COUNTERFACTUAL_SHADOW: hypothetical, landing UNPROVEN, safety NOT_EVALUATED, never realized.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counterfactual {
    pub matched: u64,
    pub unknown: u64,
    pub funding_excluded: u64,
    pub below_minimum: u64,
    pub wins: u64,
    pub lost_races: u64,
    pub no_chain_event: u64,
    pub scenarios: Vec<Scenario>,
    /// Liquidations per production loss reason (`JoinedFacts::loss_reason`).
    pub loss_attribution: BTreeMap<String, u64>,
    pub loss_attribution_model: String,
    pub landing: String,
    pub safety: String,
    pub concurrent_capital_allocation_proven: bool,
}

pub fn counterfactual(
    rows: &[&BacktestRow],
    attempts: &[&Attempt],
    p: &Params,
    model: &str,
) -> Counterfactual {
    let mut c = Counterfactual {
        matched: 0,
        unknown: 0,
        funding_excluded: 0,
        below_minimum: 0,
        wins: 0,
        lost_races: 0,
        no_chain_event: 0,
        scenarios: p
            .capital_usd_micro
            .iter()
            .map(|capital| Scenario {
                capital_usd_micro: *capital,
                wins: 0,
                win_net_usd_micro: 0,
                failed_attempts: 0,
                failed_cost_usd_micro: 0,
                failed_cost_unknown: 0,
                over_budget: 0,
                net_usd_micro: None,
            })
            .collect(),
        loss_attribution: BTreeMap::new(),
        loss_attribution_model: model.into(),
        landing: LANDING.into(),
        safety: SAFETY.into(),
        concurrent_capital_allocation_proven: false,
    };
    let row_outcomes = rows.iter().map(|row| {
        *c.loss_attribution
            .entry(row.facts.loss_reason.clone())
            .or_default() += 1;
        (true, outcome(row, p))
    });
    // `true`: a verified liquidation's outcome; `false`: a false positive's.
    let outcomes: Vec<(bool, Outcome)> = row_outcomes
        .chain(attempts.iter().map(|a| (false, attempt_outcome(a, p))))
        .collect();
    for (is_row, o) in outcomes {
        match o {
            Outcome::NotMatched => continue,
            Outcome::Unknown => c.unknown += 1,
            Outcome::FundingExcluded => c.funding_excluded += 1,
            Outcome::BelowMinimum => c.below_minimum += 1,
            Outcome::Win { .. } => c.wins += 1,
            Outcome::LostRace { .. } => c.lost_races += 1,
            Outcome::NoChainEvent { .. } => c.no_chain_event += 1,
        }
        // Review 3: matched = verified liquidations joined with an evaluation (never a false
        // positive, whatever its outcome).
        if is_row {
            c.matched += 1;
        }
        for s in &mut c.scenarios {
            match o {
                Outcome::Win { net, budget } if budget <= s.capital_usd_micro => {
                    s.wins += 1;
                    s.win_net_usd_micro = s.win_net_usd_micro.saturating_add(net);
                }
                Outcome::LostRace { budget, cost } | Outcome::NoChainEvent { budget, cost }
                    if budget <= s.capital_usd_micro =>
                {
                    s.failed_attempts += 1;
                    match cost {
                        Some(cost) => {
                            s.failed_cost_usd_micro =
                                s.failed_cost_usd_micro.saturating_add(u128::from(cost))
                        }
                        None => s.failed_cost_unknown += 1,
                    }
                }
                Outcome::Win { .. } | Outcome::LostRace { .. } | Outcome::NoChainEvent { .. } => {
                    s.over_budget += 1
                }
                _ => {}
            }
        }
    }
    for s in &mut c.scenarios {
        s.net_usd_micro = (s.failed_cost_unknown == 0)
            .then(|| i128::try_from(s.failed_cost_usd_micro).ok())
            .flatten()
            .map(|cost| s.win_net_usd_micro.saturating_sub(cost));
    }
    c
}

pub fn observed(rows: &[&BacktestRow]) -> Observed {
    let mut o = Observed::default();
    for row in rows {
        o.liquidations += 1;
        *o.by_type.entry(row.liquidation_type.clone()).or_default() += 1;
        match row.protocol_fee_lamports {
            Some(f) => o.protocol_fee_lamports += u128::from(f),
            None => o.protocol_fee_unknown += 1,
        }
        match row.transaction_fee_lamports {
            Some(f) => o.transaction_fee_lamports += u128::from(f),
            None => o.transaction_fee_unknown += 1,
        }
    }
    o
}

/// Identity of a backtest: what it read, with which parameters, by which engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacktestHeader {
    pub job_id: String,
    pub capture_run_id: String,
    pub capture_evidence_digest: String,
    pub verification_spec_id: String,
    pub verification_result_id: String,
    /// Always CANONICAL_VERIFIED (anything else is refused).
    pub verification_status: String,
    pub verified_through_slot: Option<u64>,
    /// Unix ns, half-open.
    pub capture_window: (u64, u64),
    pub live_coverage: String,
    /// The live capture's commitment (`UNKNOWN`: no typed start record).
    pub market_commitment: String,
    /// Earlier typed Shadow writer frames joined (V9/V17 attribution).
    pub earlier_attribution_records: u64,
    pub params: Params,
    pub engine: String,
    pub binary_sha256: [u8; 32],
}
impl Payload for BacktestHeader {
    const KIND: u16 = kind::BACKTEST_HEADER;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacktestSummary {
    pub rows: u64,
    pub attempts: u64,
    pub observed: Observed,
    pub counterfactual: Counterfactual,
    pub realized: String,
    /// `sha256` over the parameters, rows, attempts and this summary without the hash.
    pub result_sha256: [u8; 32],
}
impl Payload for BacktestSummary {
    const KIND: u16 = kind::BACKTEST_SUMMARY;
}

/// What the engine is: crate version, the join layout and the loss model.
pub fn engine() -> String {
    format!(
        "darkbot-runner {} join {} loss {}",
        env!("CARGO_PKG_VERSION"),
        <super::econ::CanonicalJoin as Payload>::SCHEMA,
        super::econ::LOSS_ATTRIBUTION_MODEL
    )
}

/// The loss model of a join with `earlier` evaluations of the earlier typed writer (V9/V17).
pub fn loss_model(earlier: u64) -> &'static str {
    if earlier > 0 {
        super::econ::LOSS_ATTRIBUTION_EARLIER_RECORDS
    } else {
        super::econ::LOSS_ATTRIBUTION_MODEL
    }
}

/// One joined row as a backtest row (its typed facts and the selected candidate's figures).
pub fn row_of(j: &super::econ::CanonicalJoin) -> BacktestRow {
    let costs = j.our.as_ref().and_then(|r| r.selected_costs.as_ref());
    BacktestRow {
        signature: j.market.signature.clone(),
        slot: j.market.slot,
        transaction_index: j.market.transaction_index,
        event_index: j.market.event_index,
        liquidation_type: j.market.liquidation_type.as_str().into(),
        target: j.market.target.clone(),
        liquidator: j.market.liquidator.clone(),
        block_time: j.market.block_time,
        protocol_fee_lamports: j.market.protocol_fee_lamports,
        transaction_fee_lamports: j.market.transaction_fee_lamports,
        priority_fee_lamports: j.market.priority_fee_lamports,
        facts: joined_facts(j),
        funding: costs.map(|c| c.funding.clone()),
        profit_per_capital_bps: costs.and_then(|c| c.profit_per_capital_bps),
        failed_attempt_reserve_usd_micro: costs.and_then(|c| c.failed_attempt_reserve_usd_micro),
    }
}

/// The typed inputs of one run, read once.
pub struct Inputs {
    pub capture: CaptureBinding,
    pub verification_spec_id: String,
    pub verification_result_id: String,
    pub verification_status: String,
    pub verified_through_slot: Option<u64>,
    pub live_coverage: String,
    pub market_commitment: String,
    pub rows: Vec<BacktestRow>,
    pub attempts: Vec<Attempt>,
    pub earlier_attribution_records: u64,
    meter: Meter,
}

impl Inputs {
    /// The loss model the joined evaluations were attributed under.
    pub fn loss_model(&self) -> &'static str {
        loss_model(self.earlier_attribution_records)
    }
}

/// The live market journal's commitment and slot receipts (A2), every frame kind known.
fn race_inputs(market: &Path, meter: &mut Meter) -> Result<RaceInputs, String> {
    let mut race = RaceInputs::default();
    let mut r = crate::storage::segments::ChainReader::open(market)
        .map_err(|_| "BACKTEST_BLOCKED: LIVE_MARKET_JOURNAL_MISSING")?;
    while let Some(frame) = r.next_frame().map_err(|e| format!("LIVE_READ: {e}"))? {
        match frame.kind {
            kind::MARKET_COVERAGE if frame.schema != 1 => {
                if race.note(&frame)? {
                    // One receipt per slot (the report's charge).
                    meter.charge_bytes(16)?;
                }
            }
            kind::MARKET_COVERAGE
            | kind::MARKET_TRANSACTION
            | kind::MARKET_LIQUIDATION
            | kind::MARKET_RELATED
            | journal::MANIFEST => meter.time()?,
            k => return Err(format!("LIVE_FRAME_KIND {k}")),
        }
    }
    Ok(race)
}

/// Every executable evaluation of the capture window (A3) that is no exact match of a verified
/// liquidation, once per account and signing slot: the false positives.
fn false_positives(
    shadow: &Path,
    capture: CaptureFilter<'_>,
    matched: &BTreeSet<[u8; 32]>,
    meter: &mut Meter,
) -> Result<Vec<Attempt>, String> {
    let mut matched_keys = BTreeSet::new();
    let mut found = Vec::new();
    let mut r = crate::storage::segments::ChainReader::open(shadow).map_err(|_| "SHADOW_INPUT")?;
    while let Some(frame) = r.next_frame().map_err(|_| "SHADOW_READ")? {
        meter.time()?;
        match frame.kind {
            kind::ECON_SHADOW_EVALUATION if frame.schema == 1 => {
                return Err("SHADOW_HISTORICAL_SCHEMA_1".into())
            }
            kind::ECON_SHADOW_EVALUATION => {}
            journal::MANIFEST => continue,
            _ => return Err("SHADOW_DECODE".into()),
        }
        let rec = frame
            .typed::<ShadowEvaluation>()
            .map_err(|_| "SHADOW_DECODE")?
            .record;
        // The opportunity of an exact match (its account and signing slot), wherever it appears
        // in the stream: a re-evaluation of it is no false positive.
        if record_digest(&rec).is_some_and(|d| matched.contains(&d)) {
            matched_keys.insert((rec.account.clone(), rec.sign_slot));
            continue;
        }
        let (Some(costs), Some(t_rx_ns)) = (executable_costs(&rec), rec.timing.t_rx_ns) else {
            continue;
        };
        let (Some(net), true) = (costs.net_profit_usd_micro, capture.admits(&rec)) else {
            continue;
        };
        let digest = record_digest(&rec).ok_or("SHADOW_DECODE")?;
        let attempt = Attempt {
            account: rec.account.clone(),
            sign_slot: rec.sign_slot,
            t_rx_ns,
            funding: costs.funding.clone(),
            net_usd_micro: net,
            profit_per_capital_bps: costs.profit_per_capital_bps,
            required_total_budget_usd_micro: costs.total_budget(),
            failed_attempt_reserve_usd_micro: costs.failed_attempt_reserve_usd_micro,
            record_digest: digest,
        };
        meter.charge(&attempt)?;
        found.push(attempt);
    }
    Ok(select_false_positives(found, &matched_keys))
}

/// The false positives among the executable evaluations that are no exact match: once per
/// account and signing slot, and none for the opportunity of an exact match.
fn select_false_positives(
    found: Vec<Attempt>,
    matched_keys: &BTreeSet<(String, Option<u64>)>,
) -> Vec<Attempt> {
    let mut seen = BTreeSet::new();
    found
        .into_iter()
        .filter(|a| {
            let key = (a.account.clone(), a.sign_slot);
            !matched_keys.contains(&key) && seen.insert(key)
        })
        .collect()
}

/// Reads one finalized V2 run: the capture binding, a complete capture clock, the selected
/// CANONICAL_VERIFIED result, its verified events of the capture window, their production join
/// with the sealed typed Shadow chain and the false positives of that chain. Fails closed on
/// every missing, changed or incomplete input; every byte held is charged to the budget.
pub fn load(
    run: &Path,
    verification_result: Option<&str>,
    budget: Budget,
) -> Result<Inputs, String> {
    let mut meter = Meter {
        deadline: budget.deadline()?,
        held: 0,
        max: budget.max_ram_bytes,
    };
    let capture = strict_driver::bind_capture(run)?;
    meter.time()?;
    let market_dir = capture.run.join("capture/market");
    let completion: MarketCompletion =
        journal::read_typed_value(&market_dir.join(MARKET_COMPLETION_JOURNAL))
            .map_err(|_| "BACKTEST_BLOCKED: CAPTURE_NOT_FINALIZED")?;
    if !completion.capture_clock_complete() {
        return Err("BACKTEST_BLOCKED: CAPTURE_CLOCK_INCOMPLETE".into());
    }
    let selected = report::select_result(&capture, verification_result)?
        .ok_or("BACKTEST_BLOCKED: NO_VERIFIED_RESULT")?;
    let status = selected.result.status.as_str();
    if status != "CANONICAL_VERIFIED" {
        return Err(format!("BACKTEST_BLOCKED: VERIFICATION_{status}"));
    }
    meter.time()?;
    let deadline = meter.deadline;
    let (events, _) = report::verified_events(&selected, &mut |_| {
        if Instant::now() >= deadline {
            return Err("BACKTEST_TIME_BUDGET".into());
        }
        Ok(())
    })?;
    for e in &events {
        meter.charge(e)?;
    }
    if events.iter().any(|e| e.block_time.is_none()) {
        return Err("BACKTEST_BLOCKED: EVENT_TIME_UNKNOWN".into());
    }
    let window = report::window_events(events, capture.window);
    let race = race_inputs(&market_dir.join(MARKET_JOURNAL), &mut meter)?;
    let shadow = capture.run.join("capture/shadow_evaluations.p0j");
    let chain = crate::storage::segments::verify_chain(&shadow)
        .map_err(|_| "BACKTEST_BLOCKED: SHADOW_JOURNAL_MISSING")?;
    if !(chain.sealed && chain.complete) {
        return Err("BACKTEST_BLOCKED: SHADOW_JOURNAL_INCOMPLETE".into());
    }
    let market_commitment = race
        .market_commitment
        .clone()
        .unwrap_or_else(|| "UNKNOWN".into());
    // A3: only the evaluations the live chain admits.
    let filter = CaptureFilter {
        plan: &capture.manifest.plan,
        start_ns: capture.monotonic_window.0,
        cutoff_ns: capture.monotonic_window.1,
    };
    let joined = canonical_join(
        window,
        &shadow,
        meter.deadline,
        &JoinContext {
            market_commitment: &market_commitment,
            slot_first_seen: Some(&race.slot_first_seen),
            capture: Some(filter),
            // Capital scenarios are the job's (USD-micro), not part of the joined facts.
            capital: &[],
        },
    )?;
    let mut rows = Vec::with_capacity(joined.rows.len());
    for j in &joined.rows {
        meter.charge(j)?;
        let row = row_of(j);
        meter.charge(&row)?;
        rows.push(row);
    }
    let matched: BTreeSet<[u8; 32]> = rows.iter().filter_map(|r| r.facts.record_digest).collect();
    let attempts = false_positives(&shadow, filter, &matched, &mut meter)?;
    capture.recheck("CAPTURE_EVIDENCE_CHANGED_DURING_BACKTEST")?;
    Ok(Inputs {
        verification_spec_id: selected.spec_id,
        verification_result_id: selected.result_id,
        verification_status: status.into(),
        verified_through_slot: selected.last_slot,
        live_coverage: completion.live_coverage_status().into(),
        market_commitment,
        capture,
        rows,
        attempts,
        earlier_attribution_records: joined.earlier_attribution_records,
        meter,
    })
}

fn sha256<T: Serialize>(v: &T) -> Result<[u8; 32], String> {
    use bincode::Options;
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .serialize(v)
        .map(|b| solana_sdk::hash::hash(&b).to_bytes())
        .map_err(|e| e.to_string())
}

fn binary_sha256() -> Result<[u8; 32], String> {
    std::env::current_exe()
        .and_then(|p| journal::file_sha256(&p))
        .map_err(|e| format!("BACKTEST_BINARY_IDENTITY: {e}"))
}

/// The typed frames of a result journal, sealed and complete, or an error.
fn frames(path: &Path) -> Result<Vec<journal::Frame>, String> {
    let mut r = journal::Reader::open(path).map_err(|e| format!("RESULT_READ: {e}"))?;
    let mut out = Vec::new();
    while let Some(f) = r.next_frame().map_err(|e| format!("RESULT_READ: {e}"))? {
        if f.kind != journal::MANIFEST {
            out.push(f);
        }
    }
    if !(r.sealed && r.complete) {
        return Err("RESULT_READ: unsealed".into());
    }
    Ok(out)
}

/// A typed result journal, written atomically once: a sealed staging file linked to its final
/// name. An existing result of the same job must be identical frame for frame, else refused.
fn write_once(path: &Path, frames_of: &[(u16, u16, Vec<u8>)]) -> Result<(), String> {
    let dir = path.parent().ok_or("RESULT_PATH")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let same = |p: &Path| -> Result<bool, String> {
        let kept = frames(p)?;
        Ok(kept.len() == frames_of.len()
            && kept
                .iter()
                .zip(frames_of)
                .all(|(f, (k, s, b))| f.kind == *k && f.schema == *s && f.payload == *b))
    };
    if path.exists() {
        return if same(path)? {
            Ok(())
        } else {
            Err("RESULT_CHANGED_FOR_THE_SAME_JOB".into())
        };
    }
    let name = path.file_name().ok_or("RESULT_PATH")?.to_string_lossy();
    let staging = dir.join(format!(".{name}.partial-{}", std::process::id()));
    let _ = std::fs::remove_file(&staging);
    let written = (|| -> Result<(), String> {
        let mut w = journal::Writer::create(&staging).map_err(|e| format!("RESULT_WRITE: {e}"))?;
        for (kind, schema, bytes) in frames_of {
            w.append(
                *kind,
                *schema,
                Provenance {
                    source: journal::source::BACKTEST,
                    ..Default::default()
                },
                bytes,
            )
            .map_err(|e| e.to_string())?;
        }
        w.seal(true).map_err(|e| e.to_string())?;
        Ok(())
    })();
    let linked = written.and_then(|()| match std::fs::hard_link(&staging, path) {
        Ok(()) => Ok(()),
        // A concurrent identical job won the link.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if same(path)? {
                Ok(())
            } else {
                Err("RESULT_CHANGED_FOR_THE_SAME_JOB".into())
            }
        }
        Err(e) => Err(format!("RESULT_WRITE: {e}")),
    });
    let _ = std::fs::remove_file(&staging);
    linked
}

fn encoded<P: Payload>(v: &P) -> Result<(u16, u16, Vec<u8>), String> {
    super::payload::encode(v)
        .map(|b| (P::KIND, P::SCHEMA, b))
        .map_err(|e| e.to_string())
}

pub struct Backtest {
    pub header: BacktestHeader,
    pub rows: Vec<BacktestRow>,
    pub attempts: Vec<Attempt>,
    pub summary: BacktestSummary,
    pub path: PathBuf,
}

/// The summary under `p`, hashed over the parameters, rows, attempts and the summary.
pub fn summarize(
    rows: &[BacktestRow],
    attempts: &[Attempt],
    p: &Params,
    model: &str,
) -> Result<BacktestSummary, String> {
    let refs: Vec<&BacktestRow> = rows.iter().collect();
    let attempt_refs: Vec<&Attempt> = attempts.iter().collect();
    let mut s = BacktestSummary {
        rows: rows.len() as u64,
        attempts: attempts.len() as u64,
        observed: observed(&refs),
        counterfactual: counterfactual(&refs, &attempt_refs, p, model),
        realized: REALIZED_NOT_RUN.into(),
        result_sha256: [0; 32],
    };
    s.result_sha256 = sha256(&(p, rows, attempts, &s))?;
    Ok(s)
}

/// `p0ctl backtest`: one job over one run.
pub fn backtest(
    run: &Path,
    verification_result: Option<&str>,
    params: &Params,
    budget: Budget,
) -> Result<Backtest, String> {
    params.validate()?;
    let inputs = load(run, verification_result, budget)?;
    // Review 4 (as the optimizer): never below the run's recorded minimum profit — the run
    // evaluated nothing there, so a result would only look valid.
    let recorded = recorded_min_profit(&inputs.capture.manifest)
        .ok_or("BACKTEST_BLOCKED: BASELINE_MIN_PROFIT_UNRECORDED")?;
    if params.min_profit_usd_micro < recorded {
        return Err(format!(
            "BACKTEST_PARAMS_BELOW_EVIDENCE: the run evaluated nothing below its minimum profit {recorded} USD-micro"
        ));
    }
    let summary = summarize(&inputs.rows, &inputs.attempts, params, inputs.loss_model())?;
    inputs.meter.time()?;
    let binary = binary_sha256()?;
    let job_id = journal::hex(&sha256(&(
        &inputs.capture.evidence_digest,
        &inputs.verification_result_id,
        params,
        binary,
    ))?);
    let header = BacktestHeader {
        job_id: job_id.clone(),
        capture_run_id: inputs.capture.manifest.capture_run_id.clone(),
        capture_evidence_digest: inputs.capture.evidence_digest.clone(),
        verification_spec_id: inputs.verification_spec_id.clone(),
        verification_result_id: inputs.verification_result_id.clone(),
        verification_status: inputs.verification_status.clone(),
        verified_through_slot: inputs.verified_through_slot,
        capture_window: inputs.capture.window,
        live_coverage: inputs.live_coverage.clone(),
        market_commitment: inputs.market_commitment.clone(),
        earlier_attribution_records: inputs.earlier_attribution_records,
        params: params.clone(),
        engine: engine(),
        binary_sha256: binary,
    };
    let path = inputs
        .capture
        .run
        .join(BACKTEST_DIR)
        .join(format!("{job_id}.p0j"));
    let mut all = vec![encoded(&header)?];
    for row in &inputs.rows {
        all.push(encoded(row)?);
    }
    for a in &inputs.attempts {
        all.push(encoded(a)?);
    }
    all.push(encoded(&summary)?);
    write_once(&path, &all)?;
    Ok(Backtest {
        header,
        rows: inputs.rows,
        attempts: inputs.attempts,
        summary,
        path,
    })
}

/// The bounded explicit grid of an optimization (every point is evaluated).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub min_profit_usd_micro: Vec<i128>,
    /// Funding sets (each validated like `Params::funding`; empty: every class).
    pub funding: Vec<Vec<String>>,
    pub capital_usd_micro: Vec<u64>,
}

impl Space {
    pub fn points(&self) -> Vec<Params> {
        let mut out = Vec::new();
        for min in &self.min_profit_usd_micro {
            for funding in &self.funding {
                out.push(Params {
                    min_profit_usd_micro: *min,
                    capital_usd_micro: self.capital_usd_micro.clone(),
                    funding: funding.clone(),
                });
            }
        }
        out
    }
}

/// Score of one point on one part of the window: its counterfactual and the best return on
/// capital over its scenarios (bps of the scenario capital). `None`: no admitted evaluation, or
/// a failed cost of every admitting scenario is unknown (never 0).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score {
    pub counterfactual: Counterfactual,
    pub objective_bps: Option<i128>,
}

pub fn score(rows: &[&BacktestRow], attempts: &[&Attempt], p: &Params, model: &str) -> Score {
    let counterfactual = counterfactual(rows, attempts, p, model);
    let objective_bps = counterfactual
        .scenarios
        .iter()
        .filter(|s| s.wins + s.failed_attempts > 0)
        .filter_map(|s| {
            s.net_usd_micro
                .map(|n| n.saturating_mul(i128::from(BPS)) / i128::from(s.capital_usd_micro))
        })
        .max();
    Score {
        counterfactual,
        objective_bps,
    }
}

/// Deterministic order of two scores: higher objective, then fewer UNKNOWN evaluations.
fn better(a: &Score, b: &Score) -> bool {
    match (a.objective_bps, b.objective_bps) {
        (Some(x), Some(y)) if x != y => x > y,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        _ => a.counterfactual.unknown < b.counterfactual.unknown,
    }
}

/// Every grid point scored on the training part, and the one chosen (best objective; ties:
/// fewer UNKNOWN evaluations, then grid order). `None`: no point has an objective.
pub fn search(
    points: &[Params],
    train: (&[&BacktestRow], &[&Attempt]),
    model: &str,
    deadline: Instant,
) -> Result<(Vec<Score>, Option<usize>), String> {
    let mut scores: Vec<Score> = Vec::with_capacity(points.len());
    let mut chosen: Option<usize> = None;
    for (i, p) in points.iter().enumerate() {
        if Instant::now() >= deadline {
            return Err("BACKTEST_TIME_BUDGET".into());
        }
        let s = score(train.0, train.1, p, model);
        if s.objective_bps.is_some() && chosen.is_none_or(|c| better(&s, &scores[c])) {
            chosen = Some(i);
        }
        scores.push(s);
    }
    Ok((scores, chosen))
}

/// The training and holdout parts: liquidations by chain time in the unix window, false
/// positives by receipt time in the monotone window, both cut at the same share.
pub struct Split<'a> {
    pub split_unix_ns: u64,
    pub train: (Vec<&'a BacktestRow>, Vec<&'a Attempt>),
    pub holdout: (Vec<&'a BacktestRow>, Vec<&'a Attempt>),
}

fn cut(window: (u64, u64), holdout_bps: u64) -> u64 {
    let len = u128::from(window.1.saturating_sub(window.0));
    window.0 + (len * u128::from(BPS - holdout_bps) / u128::from(BPS)) as u64
}

pub fn split<'a>(
    rows: &'a [BacktestRow],
    attempts: &'a [Attempt],
    unix_window: (u64, u64),
    monotonic_window: (u64, u64),
    holdout_bps: u64,
) -> Result<Split<'a>, String> {
    if holdout_bps == 0 || holdout_bps >= BPS {
        return Err("OPTIMIZE_HOLDOUT_INVALID".into());
    }
    let (unix_cut, mono_cut) = (
        cut(unix_window, holdout_bps),
        cut(monotonic_window, holdout_bps),
    );
    let mut out = Split {
        split_unix_ns: unix_cut,
        train: (vec![], vec![]),
        holdout: (vec![], vec![]),
    };
    for row in rows {
        let ns = row
            .block_time
            .and_then(|t| u64::try_from(t).ok())
            .and_then(|t| t.checked_mul(1_000_000_000))
            .ok_or("OPTIMIZE_ROW_TIME_UNKNOWN")?;
        if ns < unix_cut {
            out.train.0.push(row);
        } else {
            out.holdout.0.push(row);
        }
    }
    for a in attempts {
        if a.t_rx_ns < mono_cut {
            out.train.1.push(a);
        } else {
            out.holdout.1.push(a);
        }
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizeResult {
    pub job_id: String,
    pub capture_run_id: String,
    pub capture_evidence_digest: String,
    pub verification_result_id: String,
    pub verification_status: String,
    pub verified_through_slot: Option<u64>,
    pub live_coverage: String,
    /// The run's own minimum profit: the grid never goes below it (nothing was evaluated there).
    pub recorded_min_profit_usd_micro: i128,
    pub baseline: Params,
    pub space: Space,
    /// Holdout share of the capture window (bps); training rows end at `split_unix_ns`.
    pub holdout_bps: u64,
    pub split_unix_ns: u64,
    pub train_rows: u64,
    pub train_attempts: u64,
    pub holdout_rows: u64,
    pub holdout_attempts: u64,
    /// Every point's training score, in grid order.
    pub train: Vec<Score>,
    /// The point chosen on the training part only, and its holdout score.
    pub chosen: Option<u64>,
    pub chosen_holdout: Option<Score>,
    pub baseline_train: Score,
    pub baseline_holdout: Score,
    pub engine: String,
    pub binary_sha256: [u8; 32],
    pub result_sha256: [u8; 32],
}
impl Payload for OptimizeResult {
    const KIND: u16 = kind::OPTIMIZE_RESULT;
}

/// The run's own minimum profit (`P0_MIN_PROFIT`, USD-micro, recorded in its manifest).
pub fn recorded_min_profit(manifest: &RunManifest) -> Option<i128> {
    manifest
        .risk_limits
        .get("P0_MIN_PROFIT")
        .copied()
        .flatten()
        .map(i128::from)
}

/// `p0ctl optimize`: every grid point scored on the training part, the best one scored once on
/// the holdout part, the baseline on both. The grid never goes below the run's recorded minimum
/// profit and uses the baseline's capital scenarios.
pub fn optimize(
    run: &Path,
    verification_result: Option<&str>,
    baseline: &Params,
    space: &Space,
    holdout_bps: u64,
    max_points: u64,
    budget: Budget,
) -> Result<(OptimizeResult, PathBuf), String> {
    baseline.validate()?;
    if space.capital_usd_micro != baseline.capital_usd_micro {
        return Err(
            "OPTIMIZE_CAPITAL_MISMATCH: grid and baseline compare on the same capital".into(),
        );
    }
    let points = space.points();
    if points.is_empty() || points.len() as u64 > max_points {
        return Err("OPTIMIZE_GRID_OUT_OF_BOUNDS".into());
    }
    for p in &points {
        p.validate()?;
    }
    let inputs = load(run, verification_result, budget)?;
    let recorded = recorded_min_profit(&inputs.capture.manifest)
        .ok_or("OPTIMIZE_BLOCKED: BASELINE_MIN_PROFIT_UNRECORDED")?;
    if points
        .iter()
        .chain([baseline])
        .any(|p| p.min_profit_usd_micro < recorded)
    {
        return Err(format!(
            "OPTIMIZE_GRID_BELOW_EVIDENCE: the run evaluated nothing below its minimum profit {recorded} USD-micro"
        ));
    }
    let model = inputs.loss_model();
    let parts = split(
        &inputs.rows,
        &inputs.attempts,
        inputs.capture.window,
        inputs.capture.monotonic_window,
        holdout_bps,
    )?;
    let train = (parts.train.0.as_slice(), parts.train.1.as_slice());
    let holdout = (parts.holdout.0.as_slice(), parts.holdout.1.as_slice());
    let (scores, chosen) = search(&points, train, model, inputs.meter.deadline)?;
    let binary = binary_sha256()?;
    let mut result = OptimizeResult {
        job_id: journal::hex(&sha256(&(
            &inputs.capture.evidence_digest,
            &inputs.verification_result_id,
            baseline,
            space,
            holdout_bps,
            binary,
        ))?),
        capture_run_id: inputs.capture.manifest.capture_run_id.clone(),
        capture_evidence_digest: inputs.capture.evidence_digest.clone(),
        verification_result_id: inputs.verification_result_id.clone(),
        verification_status: inputs.verification_status.clone(),
        verified_through_slot: inputs.verified_through_slot,
        live_coverage: inputs.live_coverage.clone(),
        recorded_min_profit_usd_micro: recorded,
        baseline: baseline.clone(),
        space: space.clone(),
        holdout_bps,
        split_unix_ns: parts.split_unix_ns,
        train_rows: train.0.len() as u64,
        train_attempts: train.1.len() as u64,
        holdout_rows: holdout.0.len() as u64,
        holdout_attempts: holdout.1.len() as u64,
        chosen_holdout: chosen.map(|c| score(holdout.0, holdout.1, &points[c], model)),
        chosen: chosen.map(|c| c as u64),
        train: scores,
        baseline_train: score(train.0, train.1, baseline, model),
        baseline_holdout: score(holdout.0, holdout.1, baseline, model),
        engine: engine(),
        binary_sha256: binary,
        result_sha256: [0; 32],
    };
    inputs.meter.time()?;
    let mut hashed = result.clone();
    hashed.binary_sha256 = [0; 32];
    hashed.job_id.clear();
    result.result_sha256 = sha256(&hashed)?;
    let path = inputs
        .capture
        .run
        .join(OPTIMIZE_DIR)
        .join(format!("{}.p0j", result.job_id));
    write_once(&path, &[encoded(&result)?])?;
    Ok((result, path))
}

/// An offline Shadow candidate profile (STARTUP §4 F): parameters only, never a LIVE setting.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub params: Params,
    /// `OPTIMIZER_CANDIDATE` (with its optimization job) or `OPERATOR` (without).
    pub origin: String,
    pub optimize_job_id: Option<String>,
}
impl Payload for Profile {
    const KIND: u16 = kind::BACKTEST_PROFILE;
}

impl Profile {
    pub fn id(&self) -> Result<String, String> {
        sha256(self).map(|h| journal::hex(&h))
    }
    fn validate(&self) -> Result<(), String> {
        self.params.validate()?;
        match (self.origin.as_str(), &self.optimize_job_id) {
            (ORIGIN_OPTIMIZER, Some(job)) if !job.is_empty() => Ok(()),
            (ORIGIN_OPERATOR, None) => Ok(()),
            _ => Err("PROFILE_ORIGIN_INVALID".into()),
        }
    }
    pub fn save(&self, dir: &Path) -> Result<PathBuf, String> {
        self.validate()?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.p0j", self.id()?));
        journal::write_typed_value(&path, journal::source::BACKTEST, self)
            .map_err(|e| format!("PROFILE_WRITE: {e}"))?;
        Ok(path)
    }
    /// A saved profile: valid, and named by its own content id.
    pub fn load(path: &Path) -> Result<Self, String> {
        let p: Self = journal::read_typed_value(path).map_err(|e| format!("PROFILE_READ: {e}"))?;
        p.validate()?;
        if path.file_stem().and_then(|s| s.to_str()) != Some(p.id()?.as_str()) {
            return Err("PROFILE_NAME_MISMATCH".into());
        }
        Ok(p)
    }
}

// ------------------------------------------------------------------------------------------
// Command line (`p0ctl backtest|optimize|profiles`)
// ------------------------------------------------------------------------------------------

/// `flag VALUE`; a flag given without its value is an error, never an absent flag.
fn flag(args: &[String], name: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .filter(|v| !v.starts_with("--"))
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{name} requires a value")),
    }
}

fn required(args: &[String], name: &str) -> Result<String, String> {
    flag(args, name)?.ok_or_else(|| format!("{name} required"))
}

fn number(args: &[String], name: &str) -> Result<u64, String> {
    required(args, name)?
        .parse()
        .map_err(|_| format!("{name}: positive integer required"))
}

/// `--max-seconds N --max-ram-bytes N` (both required: the job's explicit budget).
pub fn parse_budget(args: &[String]) -> Result<Budget, String> {
    Ok(Budget {
        max_seconds: number(args, "--max-seconds")?,
        max_ram_bytes: number(args, "--max-ram-bytes")?,
    })
}

/// `--holdout-bps N --max-points N` of an optimization (both required).
pub fn parse_search(args: &[String]) -> Result<(u64, u64), String> {
    Ok((
        number(args, "--holdout-bps")?,
        number(args, "--max-points")?,
    ))
}

/// USD capital list (`econ::parse_capital`, the run's own rule) in USD-micro.
pub fn parse_capital_usd(v: &str) -> Result<Vec<u64>, String> {
    let mut out: Vec<u64> = super::econ::parse_capital(v)?
        .into_iter()
        .map(|usd| usd * MICRO_PER_USD)
        .collect();
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// `A,B` funding classes, sorted; `all`: every class (empty list).
pub fn parse_funding(v: &str) -> Result<Vec<String>, String> {
    if v == "all" {
        return Ok(vec![]);
    }
    let mut out: Vec<String> = v.split(',').map(|s| s.trim().to_string()).collect();
    out.sort();
    out.dedup();
    Ok(out)
}

/// A non-negative USD amount in USD-micro (exact decimal, `report::parse_usd_micro`).
pub fn parse_min_profit(v: &str) -> Result<i128, String> {
    let micro = report::parse_usd_micro(v)?;
    if micro < 0 {
        return Err("BACKTEST_MIN_PROFIT_NEGATIVE".into());
    }
    Ok(micro)
}

/// The run's manifest (typed), for its recorded minimum profit.
pub fn manifest_of(run: &Path) -> Result<RunManifest, String> {
    journal::read_typed_value(
        &run.join("capture")
            .join(super::measurement::RUN_MANIFEST_JOURNAL),
    )
    .map_err(|_| "NATIVE_MANIFEST_MISSING".into())
}

/// The parameters of an invocation: `--profile FILE` alone, or `--capital-usd LIST` with
/// `--min-profit-usd X` (default: the run's recorded minimum) and `--funding all|A,B`.
pub fn parse_params(run: &Path, args: &[String]) -> Result<Params, String> {
    let explicit = ["--capital-usd", "--min-profit-usd", "--funding"];
    if let Some(profile) = flag(args, "--profile")? {
        if explicit.iter().any(|f| args.iter().any(|a| a == f)) {
            return Err("--profile excludes --capital-usd, --min-profit-usd and --funding".into());
        }
        return Ok(Profile::load(Path::new(&profile))?.params);
    }
    let min_profit_usd_micro = match flag(args, "--min-profit-usd")? {
        Some(v) => parse_min_profit(&v)?,
        None => recorded_min_profit(&manifest_of(run)?).ok_or(
            "BACKTEST_BASELINE_MIN_PROFIT_UNRECORDED: the run's manifest has no P0_MIN_PROFIT; give --min-profit-usd",
        )?,
    };
    let params = Params {
        min_profit_usd_micro,
        capital_usd_micro: parse_capital_usd(&required(args, "--capital-usd")?)?,
        funding: parse_funding(&flag(args, "--funding")?.unwrap_or_else(|| "all".into()))?,
    };
    params.validate()?;
    Ok(params)
}

/// `--min-profit-usd-grid X,Y --funding-sets all;A,B` over the baseline's capital scenarios.
pub fn parse_space(args: &[String], baseline: &Params) -> Result<Space, String> {
    let mut min_profit_usd_micro = required(args, "--min-profit-usd-grid")?
        .split(',')
        .map(|v| parse_min_profit(v.trim()))
        .collect::<Result<Vec<_>, _>>()?;
    min_profit_usd_micro.sort_unstable();
    min_profit_usd_micro.dedup();
    let mut funding = required(args, "--funding-sets")?
        .split(';')
        .map(|v| parse_funding(v.trim()))
        .collect::<Result<Vec<_>, _>>()?;
    funding.sort();
    funding.dedup();
    Ok(Space {
        min_profit_usd_micro,
        funding,
        capital_usd_micro: baseline.capital_usd_micro.clone(),
    })
}

fn usd(micro: i128) -> String {
    let sign = if micro < 0 { "-" } else { "" };
    let a = micro.unsigned_abs();
    let unit = u128::from(MICRO_PER_USD);
    format!("{sign}{}.{:06}", a / unit, a % unit)
}

fn usd_or_unknown(micro: Option<i128>) -> String {
    micro.map_or_else(|| "UNKNOWN".into(), usd)
}

/// Terminal text of a counterfactual (exact USD amounts, never `f64`).
pub fn render_counterfactual(c: &Counterfactual) -> String {
    let mut out = format!(
        "COUNTERFACTUAL_SHADOW (never realized; landing {}; safety {}; concurrent capital use proven={}): matched={} wins={} lost_races={} no_chain_event={} unknown={} below_minimum={} funding_excluded={} loss_model={}\n",
        c.landing,
        c.safety,
        c.concurrent_capital_allocation_proven,
        c.matched,
        c.wins,
        c.lost_races,
        c.no_chain_event,
        c.unknown,
        c.below_minimum,
        c.funding_excluded,
        c.loss_attribution_model
    );
    for s in &c.scenarios {
        out += &format!(
            "  capital_usd={} wins={} win_net_usd={} failed_attempts={} failed_cost_usd={} (unknown {}) over_budget={} net_usd={}\n",
            usd(i128::from(s.capital_usd_micro)),
            s.wins,
            usd(s.win_net_usd_micro),
            s.failed_attempts,
            usd(i128::try_from(s.failed_cost_usd_micro).unwrap_or(i128::MAX)),
            s.failed_cost_unknown,
            s.over_budget,
            usd_or_unknown(s.net_usd_micro)
        );
    }
    out
}

pub fn render_backtest(b: &Backtest) -> String {
    let h = &b.header;
    let o = &b.summary.observed;
    format!(
        "BACKTEST job={} run={} verification={} {} through_slot={} live_coverage={} commitment={} rows={} attempts={} result_sha256={}\n\
         OBSERVED_MARKET: liquidations={} by_type={:?} protocol_fee_lamports={} (unknown {}) transaction_fee_lamports={} (unknown {})\n\
         {}REALIZED_ONCHAIN: {}\njournal: {}\n",
        h.job_id,
        h.capture_run_id,
        h.verification_result_id,
        h.verification_status,
        h.verified_through_slot
            .map_or("UNKNOWN".into(), |s| s.to_string()),
        h.live_coverage,
        h.market_commitment,
        b.summary.rows,
        b.summary.attempts,
        journal::hex(&b.summary.result_sha256),
        o.liquidations,
        o.by_type,
        o.protocol_fee_lamports,
        o.protocol_fee_unknown,
        o.transaction_fee_lamports,
        o.transaction_fee_unknown,
        render_counterfactual(&b.summary.counterfactual),
        b.summary.realized,
        b.path.display()
    )
}

pub fn render_optimize(r: &OptimizeResult, path: &Path) -> String {
    let points = r.space.points();
    let mut out = format!(
        "OPTIMIZE job={} run={} verification={} {} live_coverage={} recorded_min_profit_usd={} holdout_bps={} split_unix_ns={} train={}+{} holdout={}+{} points={} result_sha256={}\n\
         Offline Shadow candidate only: never realized profit, safety not evaluated, never a LIVE setting.\n",
        r.job_id,
        r.capture_run_id,
        r.verification_result_id,
        r.verification_status,
        r.live_coverage,
        usd(r.recorded_min_profit_usd_micro),
        r.holdout_bps,
        r.split_unix_ns,
        r.train_rows,
        r.train_attempts,
        r.holdout_rows,
        r.holdout_attempts,
        points.len(),
        journal::hex(&r.result_sha256)
    );
    let objective = |s: &Score| s.objective_bps.map_or("UNKNOWN".into(), |b| b.to_string());
    for (i, (p, s)) in points.iter().zip(&r.train).enumerate() {
        out += &format!(
            "  point {i}: min_profit_usd={} funding={} train_objective_bps={} wins={} failed={} unknown={}{}\n",
            usd(p.min_profit_usd_micro),
            if p.funding.is_empty() { "all".into() } else { p.funding.join(",") },
            objective(s),
            s.counterfactual.wins,
            s.counterfactual.lost_races + s.counterfactual.no_chain_event,
            s.counterfactual.unknown,
            if r.chosen == Some(i as u64) { "  <- chosen on training rows" } else { "" }
        );
    }
    out += &format!(
        "baseline: train_objective_bps={} holdout_objective_bps={}\n",
        objective(&r.baseline_train),
        objective(&r.baseline_holdout)
    );
    match &r.chosen_holdout {
        Some(s) => {
            out += &format!(
                "chosen holdout_objective_bps={}\n{}",
                objective(s),
                render_counterfactual(&s.counterfactual)
            )
        }
        None => out += "chosen: none (no point has a known objective on the training part)\n",
    }
    out + &format!("journal: {}\n", path.display())
}

#[cfg(test)]
#[path = "backtest_tests.rs"]
mod tests;
