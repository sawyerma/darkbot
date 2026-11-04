//! REV.1.0 R2: the LIVE attempts' evaluator (master plan §11; §13 J
//! "Frozen qualification campaign", "Attempt, campaign and release
//! verdicts", "Acceptance evaluator regressions"). Offline, from what the
//! runtime wrote: every LIVE BUY the LIVE ledger holds as written
//! (`RELEASED` or `UNKNOWN`; a `WITHDRAWN` one never left), joined with its
//! journaled release (`l`), its providers' answers (`a`), the canonical
//! blocks, launches and trades and the run manifests of every run under
//! the run directory. Each attempt gets its component verdicts and the
//! market window from its launch's block; the campaign is rated against
//! its manifest (`bsc/config/live_campaign.env`): only a reviewed,
//! complete manifest whose build, configuration, host, builders and
//! sources every campaign run and attempt holds, committed before the
//! campaign's attempts, rates anything; a run not sealed whole, a gap in
//! the followed chain or a launch not verifiable leaves it open. A
//! runtime attempt has no producer witness unless a producer timing
//! verdict bound to its own G0.8 result is given: its T0→T_buy is UNKNOWN
//! then, never PASS (§11). Synthetic inputs test the rules only; they are
//! never acquisition samples.

use crate::g04_census::launch_record;
use crate::live_ledger::{Fact, FactKind, PlacementVerdict, Tx, fold};
use crate::runtime_journal::{self as rj, LiveRelease};
use crate::sources::{ComponentId, LiveAdapter, LiveIntentKind, SourceId};
use alloy_primitives::U256;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_journal::storage::run::RunReader;
use darkbot_bsc_record::schema::bsc_capture::capture_run as cr;
use darkbot_bsc_record::schema::bsc_runtime::{
    live_attempt as la, live_campaign_precommit as lp, live_campaign_verdict as lv,
};
use darkbot_bsc_record::schema::g08::{result as gr, timing_verdict as tv};
use darkbot_bsc_record::{Draft, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub const PASS: &str = "PASS";
pub const FAIL: &str = "FAIL";
pub const UNKNOWN: &str = "UNKNOWN";
pub const NOT_APPLICABLE: &str = "NOT_APPLICABLE";

pub const CAMPAIGN_NOT_READY: &str = "CAMPAIGN_NOT_READY";
pub const CAMPAIGN_OPEN: &str = "CAMPAIGN_OPEN";
pub const CAMPAIGN_FAIL: &str = "CAMPAIGN_FAIL";
pub const CAMPAIGN_PASS: &str = "CAMPAIGN_PASS";
/// Only the campaign's part of a release (§13 J "Release PASS" also needs
/// the implementation matrix, runtime enforcement and LIVE authorization).
pub const RELEASE_EVIDENCE_NOT_READY: &str = "RELEASE_EVIDENCE_NOT_READY";
pub const RELEASE_EVIDENCE_PASS: &str = "RELEASE_EVIDENCE_PASS";

/// The campaign cells this runtime's attempts fall in (§13 J 2): a launch
/// by a creator the operator does not control, or by the operator's own
/// launcher wallet (or the buyer itself).
pub const EXTERNAL_FULL_LAUNCH: &str = "EXTERNAL_FULL_LAUNCH";
pub const SELF_LAUNCH: &str = "SELF_LAUNCH";
pub const CELLS: [&str; 2] = [EXTERNAL_FULL_LAUNCH, SELF_LAUNCH];
/// A Strategy C entry: no launch race, no T0, in no cell.
pub const STRATEGY_ENTRY: &str = "STRATEGY_ENTRY";
/// A C3 BUY whose launch's creator is not known, or not known to be
/// another's (no manifest names the operator's launchers).
pub const CELL_UNKNOWN: &str = "UNKNOWN";

/// The strict producer bound (§9, §11): PASS only strictly below it. A
/// requirement of the plan, never a tunable.
pub const STRICT_LIMIT_NS: u64 = 1_000_000;

/// The market window from the launch (master plan §11: `[T0, T0+15 s]`,
/// a requirement of the plan, never a tunable); G0.8 captures the same.
pub const MARKET_WINDOW_MS: u64 = 15_000;

/// The only placement predicate the runtime judges (`PLACED`).
pub const FIRST_AFTER_LAUNCH: &str = "FIRST_AFTER_LAUNCH";
/// The only launch eligibility the C3 path has: a canonical Flap launch of
/// the C1 scope (`newTokenV6` verified).
pub const ELIGIBILITY_C3: &str = "CANONICAL_FLAP_C1_SCOPE";

/// Where the evaluation is written in the run directory.
pub const ATTEMPTS_DIR: &str = "bsc-runtime/live-attempts";
pub const VERDICT_PATH: &str = "bsc-runtime/live-campaign-verdict.dbrec";
/// What every record name in [`ATTEMPTS_DIR`] ends with (the writer's
/// temporary file adds `.tmp`).
pub const ATTEMPT_FILE_SUFFIX: &str = ".dbrec";

/// The name of an attempt's record in [`ATTEMPTS_DIR`]: its transaction,
/// `0x<lowercase hex>.dbrec`.
pub fn attempt_file_name(tx_hash: &[u8; 32]) -> String {
    format!(
        "0x{}{ATTEMPT_FILE_SUFFIX}",
        alloy_primitives::hex::encode(tx_hash)
    )
}

/// The transaction an attempt record's `name` names: only a name exactly
/// of [`attempt_file_name`]'s form has one.
pub fn attempt_file_tx(name: &str) -> Option<[u8; 32]> {
    let digits = name.strip_prefix("0x")?.strip_suffix(ATTEMPT_FILE_SUFFIX)?;
    let tx = <[u8; 32]>::try_from(alloy_primitives::hex::decode(digits).ok()?).ok()?;
    (attempt_file_name(&tx) == name).then_some(tx)
}

/// The decision a pre-commitment binds.
pub const SCOPE: &str = "REV1/LIVE-CAMPAIGN";

/// How a launch's first sighting was classified (the `C` record's class
/// byte, 1-based).
pub const SIGHTING_CLASSES: [&str; 6] = [
    "FULL_BEFORE",
    "HASH_ONLY_BEFORE",
    "AFTER",
    "MISSED",
    "CLOCK_UNKNOWN",
    "EVICTED",
];

const TOOL: &str = "bsc_runtime live attempts";
const E18: u64 = 1_000_000_000_000_000_000;

// ---- the journals ----

/// One canonical Portal trade of a journal.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Trade {
    block: u64,
    block_hash: [u8; 32],
    tx_index: u64,
    log_index: u64,
    tx_hash: [u8; 32],
    sell: bool,
    token: [u8; 20],
    amount: U256,
    eth: U256,
}

/// One canonical launch of a journal (`C`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Launch {
    flap: bool,
    c1_scope: bool,
    /// Its transaction could not be verified (not rebuildable, or every
    /// answer refused): whether it was in scope is unknown.
    unverifiable: bool,
    /// An index of [`SIGHTING_CLASSES`] plus one.
    class: u8,
    block: u64,
    block_hash: [u8; 32],
    block_ms: u64,
    tx_index: Option<u64>,
    tx_hash: [u8; 32],
    token: Option<[u8; 20]>,
}

/// A run's manifest, as its writer sealed it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunManifest {
    pub complete: bool,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub lost_records: u64,
    /// `None` in a manifest of a layout before it was named.
    pub ingress_dropped: Option<u64>,
    pub build_sha256: Option<[u8; 32]>,
    pub config_sha256: Option<[u8; 32]>,
    pub host: Option<String>,
}

/// One run of the run directory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub id: u64,
    /// Absent while the run is written (or after its writer died).
    pub manifest: Option<RunManifest>,
    pub manifest_error: Option<String>,
    /// Whether the sealed segments read are the manifest's (none, its
    /// count, records, sequences and hashes; review N1).
    pub manifest_match: Option<bool>,
    pub read_error: Option<String>,
    pub records: u64,
    pub last_ms: Option<u64>,
    /// The lowest and highest canonical height this run journaled.
    pub blocks: Option<(u64, u64)>,
}

/// One journaled LIVE release, with its run and Unix ms.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Release {
    run: u64,
    l: LiveRelease,
}

/// What the run directory's journals hold for the evaluation.
#[derive(Clone, Debug, Default)]
pub struct Journaled {
    releases: HashMap<[u8; 32], Release>,
    /// `l` records not read, and why (a layout before R2, malformed).
    pub releases_refused: Vec<(u64, &'static str)>,
    /// Each provider's answer: (transaction, adapter) → (answer, time).
    answers: HashMap<([u8; 32], u8), (u8, Option<u64>)>,
    /// The canonical block at each height: the last one journaled (a
    /// reorg's replacement comes after the orphan).
    blocks: BTreeMap<u64, ([u8; 32], u64)>,
    launches: Vec<Launch>,
    /// Each Flap token's launch creator (`N`).
    creators: HashMap<[u8; 20], [u8; 20]>,
    trades: Vec<Trade>,
    /// Blocks a refused Portal log left incomplete.
    refused: HashSet<(u64, [u8; 32])>,
    pub runs: Vec<Run>,
    /// The latest Unix ms a journal reached.
    pub last_ms: Option<u64>,
}

fn run_manifest(bytes: &[u8]) -> Result<RunManifest, String> {
    let r = cr::decode_run(bytes).map_err(|e| e.0)?;
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    Ok(RunManifest {
        complete: r.text(cr::STATE).map_err(e)? == "COMPLETE",
        started_ms: r.u64(cr::STARTED_UNIX_MS).map_err(e)?,
        ended_ms: r.opt_u64(cr::ENDED_UNIX_MS).map_err(e)?,
        lost_records: r.u64(cr::LOST_RECORDS).map_err(e)?,
        ingress_dropped: r.opt_u64(cr::INGRESS_DROPPED).ok().flatten(),
        build_sha256: r.opt_b256(cr::BUILD_SHA256).ok().flatten(),
        config_sha256: r.opt_b256(cr::CONFIG_SHA256).ok().flatten(),
        host: r.opt_text(cr::HOST).ok().flatten().map(str::to_string),
    })
}

impl Journaled {
    /// Every run under `journal_dir`, oldest first, with its manifest.
    pub fn read(journal_dir: &Path, max_payload: usize) -> Result<Self, String> {
        let mut dirs: Vec<(u64, std::path::PathBuf)> = match std::fs::read_dir(journal_dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter_map(|e| {
                    let ms = e.file_name().to_str()?.parse::<u64>().ok()?;
                    e.path().is_dir().then(|| (ms, e.path()))
                })
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("{}: {e}", journal_dir.display())),
        };
        dirs.sort();
        let mut out = Journaled::default();
        for (id, dir) in dirs {
            let mut run = Run {
                id,
                ..Run::default()
            };
            let manifest = match std::fs::read(dir.join(rj::RUN_MANIFEST)) {
                Ok(bytes) => {
                    match run_manifest(&bytes) {
                        Ok(m) => run.manifest = Some(m),
                        Err(e) => run.manifest_error = Some(e),
                    }
                    Some(bytes)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    run.manifest_error = Some(e.to_string());
                    None
                }
            };
            match RunReader::open(&dir, max_payload) {
                Ok(mut reader) => {
                    loop {
                        match reader.next_record() {
                            Ok(Some(r)) => {
                                let ms = id.saturating_add(r.timestamp_ns / 1_000_000);
                                run.records += 1;
                                run.last_ms = Some(run.last_ms.map_or(ms, |m| m.max(ms)));
                                out.last_ms = Some(out.last_ms.map_or(ms, |m| m.max(ms)));
                                if let Some(n) = out.record(r.kind, &r.payload, id) {
                                    run.blocks = Some(
                                        run.blocks.map_or((n, n), |(a, b)| (a.min(n), b.max(n))),
                                    );
                                }
                            }
                            Ok(None) => break,
                            Err(e) => {
                                run.read_error = Some(e.to_string());
                                break;
                            }
                        }
                    }
                    run.manifest_match = manifest
                        .as_deref()
                        .map(|b| crate::g04_census::manifest_matches(b, reader.segments()));
                }
                Err(e) => run.read_error = Some(e.to_string()),
            }
            out.runs.push(run);
        }
        Ok(out)
    }

    /// One record of run `run`; a canonical block's height when it is one.
    fn record(&mut self, kind: RecordKind, p: &[u8], run: u64) -> Option<u64> {
        match (kind, p.first()) {
            (RecordKind::Runtime, Some(b'l')) => match rj::live_release_of(p) {
                Ok(l) => {
                    self.releases.entry(l.tx_hash).or_insert(Release { run, l });
                }
                Err(why) => self.releases_refused.push((run, why)),
            },
            // Review P2: the chain the follower caught up on before a run's
            // window (no run was up), and its launches: no sighting could
            // see one (MISSED).
            (RecordKind::Runtime, Some(b'b')) => {
                if let Some((number, hash, time_ms)) = rj::catchup_block_of(p) {
                    self.blocks.insert(number, (hash, time_ms));
                    return Some(number);
                }
            }
            // Review Q1: the catch-up's trades and refused trade logs, read
            // as the window's own.
            (RecordKind::Runtime, Some(b'd' | b'e')) => {
                let mut as_canonical = p.to_vec();
                as_canonical[0] = if p[0] == b'd' { b'T' } else { b'W' };
                return self.record(RecordKind::Canonical, &as_canonical, run);
            }
            (RecordKind::Runtime, Some(b'c')) => {
                if let Some(l) = rj::catchup_launch_of(p) {
                    self.launches.push(Launch {
                        flap: l.venue == crate::canonical_join::Venue::Flap,
                        // Review LOW 2: in scope only with a verified
                        // transaction, as a `C` record.
                        c1_scope: l.c1_scope && !l.unverifiable,
                        unverifiable: l.unverifiable,
                        class: 4,
                        block: l.block,
                        block_hash: l.block_hash,
                        block_ms: l.block_ms,
                        tx_index: Some(l.tx_index),
                        tx_hash: l.tx_hash,
                        token: l.token,
                    });
                }
            }
            (RecordKind::Runtime, Some(b'a')) => {
                if let Some((tx, adapter, answer, at)) = rj::live_answer_of(p) {
                    self.answers.entry((tx, adapter)).or_insert((answer, at));
                }
            }
            (RecordKind::Canonical, Some(b'B'))
                if p.len() == crate::runtime_strategy::BLOCK_LEN =>
            {
                let (Some(number), Some(hash), Some(time_ms)) = (
                    p.get(1..9)
                        .and_then(|b| b.try_into().ok())
                        .map(u64::from_le_bytes),
                    p.get(9..41).and_then(|b| <[u8; 32]>::try_from(b).ok()),
                    p.get(41..49)
                        .and_then(|b| b.try_into().ok())
                        .map(u64::from_le_bytes),
                ) else {
                    return None;
                };
                self.blocks.insert(number, (hash, time_ms));
                return Some(number);
            }
            (RecordKind::Canonical, Some(b'C')) => {
                if let Some(l) = launch_record(p, 0) {
                    self.launches.push(Launch {
                        flap: l.venue == crate::canonical_join::Venue::Flap,
                        c1_scope: l.c1_scope,
                        unverifiable: l.tx_unverifiable || l.tx_answer_refused,
                        class: l.class,
                        block: l.block_number,
                        block_hash: l.block_hash,
                        block_ms: l.block_ms,
                        tx_index: l.tx_index,
                        tx_hash: l.tx_hash,
                        token: l.token,
                    });
                }
            }
            (RecordKind::Canonical, Some(b'N')) => {
                if let Some(darkbot_bsc_trading::strategy::engine::Input::Launch {
                    token,
                    creator,
                    ..
                }) = crate::runtime_strategy::input_of_record(kind, p)
                {
                    self.creators.insert(token, creator);
                }
            }
            (RecordKind::Canonical, Some(b'T')) => {
                if let Some(t) = rj::flap_trade_of(p) {
                    self.trades.push(Trade {
                        block: t.block_number,
                        block_hash: t.block_hash,
                        tx_index: t.tx_index,
                        log_index: t.log_index,
                        tx_hash: t.tx_hash,
                        sell: t.sell,
                        token: t.token,
                        amount: U256::from_be_bytes(t.amount),
                        eth: U256::from_be_bytes(t.eth),
                    });
                }
            }
            (RecordKind::Canonical, Some(b'W')) if p.len() == 57 => {
                if let (Some(number), Some(hash)) = (
                    p.get(1..9)
                        .and_then(|b| b.try_into().ok())
                        .map(u64::from_le_bytes),
                    p.get(9..41).and_then(|b| <[u8; 32]>::try_from(b).ok()),
                ) {
                    self.refused.insert((number, hash));
                }
            }
            _ => {}
        }
        None
    }

    /// The highest canonical block journaled.
    pub fn head(&self) -> Option<u64> {
        self.blocks.keys().next_back().copied()
    }

    fn canonical(&self, number: u64, hash: &[u8; 32]) -> bool {
        self.blocks.get(&number).is_some_and(|(h, _)| h == hash)
    }

    /// The launch `tx_hash` in its canonical block, if journaled.
    fn launch(&self, tx_hash: &[u8; 32]) -> Option<&Launch> {
        self.launches
            .iter()
            .rev()
            .find(|l| l.tx_hash == *tx_hash && self.canonical(l.block, &l.block_hash))
    }

    /// The canonical Flap launches whose block is not older than
    /// `from_ms`, in chain order (a launch once).
    fn canonical_flap(&self, from_ms: Option<u64>) -> Vec<&Launch> {
        let mut seen = HashSet::new();
        let mut out: Vec<&Launch> = self
            .launches
            .iter()
            .rev()
            .filter(|l| l.flap && self.canonical(l.block, &l.block_hash))
            .filter(|l| from_ms.is_none_or(|m| l.block_ms >= m))
            .filter(|l| seen.insert(l.tx_hash))
            .collect();
        out.sort_by_key(|l| (l.block, l.tx_index));
        out
    }

    /// The heights in `from..=to` the journals hold no canonical block of:
    /// how many, and the first span.
    fn gaps(&self, from: u64, to: u64) -> (u64, Option<(u64, u64)>) {
        let (mut missing, mut first) = (0u64, None);
        let mut expected = from;
        for n in self.blocks.range(from..=to).map(|(n, _)| *n) {
            if n > expected {
                missing += n - expected;
                first.get_or_insert((expected, n - 1));
            }
            expected = n + 1;
        }
        if expected <= to {
            missing += to - expected + 1;
            first.get_or_insert((expected, to));
        }
        (missing, first)
    }
}

// ---- the market window ----

/// An attempt's market window from its launch's block (§11): the trades of
/// its token in the blocks whose header time is at most the window after
/// the launch's. T0 is the header's time here, never the producer's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub why_incomplete: Option<&'static str>,
    pub window_ms: u64,
    pub launch_block: Option<u64>,
    pub launch_ms: Option<u64>,
    pub end_block: Option<u64>,
    pub trades: u64,
    pub buys: u64,
    pub sells: u64,
    pub buy_wei: U256,
    pub sell_wei: U256,
    /// The first BUY of another trader (not the launch's own transaction):
    /// its transaction, block, index and price.
    pub first_external: Option<([u8; 32], u64, u64, U256)>,
    pub own_price: Option<U256>,
    pub price_vs_first_external_ppm: Option<u64>,
}

impl Window {
    pub fn complete(&self) -> bool {
        self.why_incomplete.is_none()
    }
}

/// Native wei per 10^18 token units of a trade (`None` without tokens or
/// beyond 256 bits).
pub fn price(eth: U256, amount: U256) -> Option<U256> {
    (!amount.is_zero()).then(|| eth.checked_mul(U256::from(E18)).map(|v| v / amount))?
}

/// The price per token of one trade against another's, ppm (`None` when
/// the other's is zero or beyond 64 bits).
pub fn price_ppm(eth: U256, amount: U256, other_eth: U256, other_amount: U256) -> Option<u64> {
    let num = eth
        .checked_mul(other_amount)?
        .checked_mul(U256::from(1_000_000u64))?;
    let den = amount.checked_mul(other_eth)?;
    (!den.is_zero()).then(|| u64::try_from(num / den).ok())?
}

fn window(
    j: &Journaled,
    launch: Option<&Launch>,
    token: [u8; 20],
    own: [u8; 32],
    window_ms: u64,
    horizon: Option<u64>,
) -> Window {
    let mut w = Window {
        window_ms,
        ..Window::default()
    };
    let Some(launch) = launch else {
        w.why_incomplete = Some("LAUNCH_NOT_JOURNALED");
        return w;
    };
    w.launch_block = Some(launch.block);
    let Some(&(_, launch_ms)) = j.blocks.get(&launch.block) else {
        w.why_incomplete = Some("LAUNCH_BLOCK_NOT_JOURNALED");
        return w;
    };
    w.launch_ms = Some(launch_ms);
    let end_ms = launch_ms.saturating_add(window_ms);
    let mut last = launch.block;
    let mut height = launch.block;
    loop {
        match j.blocks.get(&height) {
            Some((_, ms)) if *ms > end_ms => {
                w.end_block = Some(last);
                break;
            }
            Some((hash, _)) => {
                if j.refused.contains(&(height, *hash)) {
                    w.why_incomplete.get_or_insert("TRADE_LOG_REFUSED");
                }
                last = height;
            }
            None if j.head().is_some_and(|h| h > height) => {
                w.why_incomplete.get_or_insert("BLOCK_NOT_JOURNALED");
                last = height;
            }
            None => {
                w.why_incomplete.get_or_insert("WINDOW_NOT_OVER");
                break;
            }
        }
        height += 1;
    }
    if let (Some(end), Some(h)) = (w.end_block, horizon)
        && j.head().is_none_or(|head| head < end.saturating_add(h))
    {
        w.why_incomplete.get_or_insert("REORG_HORIZON_NOT_REACHED");
    }
    let mut seen = HashSet::new();
    let mut trades: Vec<&Trade> = j
        .trades
        .iter()
        .filter(|t| t.token == token && (launch.block..=last).contains(&t.block))
        .filter(|t| j.canonical(t.block, &t.block_hash))
        .filter(|t| seen.insert((t.block_hash, t.tx_index, t.log_index)))
        .collect();
    trades.sort_by_key(|t| (t.block, t.tx_index, t.log_index));
    let mut own_trade = None;
    for t in &trades {
        w.trades += 1;
        if t.sell {
            w.sells += 1;
            w.sell_wei = w.sell_wei.saturating_add(t.eth);
            continue;
        }
        w.buys += 1;
        w.buy_wei = w.buy_wei.saturating_add(t.eth);
        if t.tx_hash == own {
            own_trade.get_or_insert(*t);
        } else if t.tx_hash != launch.tx_hash && w.first_external.is_none() {
            w.first_external = price(t.eth, t.amount).map(|p| (t.tx_hash, t.block, t.tx_index, p));
        }
    }
    let first = w
        .first_external
        .and_then(|(tx, ..)| trades.iter().find(|t| t.tx_hash == tx && !t.sell));
    if let Some(o) = own_trade {
        w.own_price = price(o.eth, o.amount);
        w.price_vs_first_external_ppm =
            first.and_then(|f| price_ppm(o.eth, o.amount, f.eth, f.amount));
    }
    w
}

// ---- the campaign manifest ----

/// The manifest's keys (master plan §13 J "Frozen qualification
/// campaign"), every one required in a reviewed manifest (the minimums of
/// a cell only when it is enabled).
pub const MANIFEST_KEYS: [&str; 32] = [
    "BSC_CAMPAIGN_ID",
    "BSC_CAMPAIGN_BUILD_SHA256",
    "BSC_CAMPAIGN_CONFIG_SHA256",
    "BSC_CAMPAIGN_CHAIN_ID",
    "BSC_CAMPAIGN_HOST",
    "BSC_CAMPAIGN_ADAPTERS",
    "BSC_CAMPAIGN_SOURCES",
    "BSC_CAMPAIGN_SELF_LAUNCHERS",
    "BSC_CAMPAIGN_CELLS",
    "BSC_CAMPAIGN_EXTERNAL_FULL_LAUNCH_MIN_ATTEMPTS",
    "BSC_CAMPAIGN_EXTERNAL_FULL_LAUNCH_MIN_DURATION_S",
    "BSC_CAMPAIGN_SELF_LAUNCH_MIN_ATTEMPTS",
    "BSC_CAMPAIGN_SELF_LAUNCH_MIN_DURATION_S",
    "BSC_CAMPAIGN_SAMPLE_RATIONALE",
    "BSC_CAMPAIGN_WORKLOAD",
    "BSC_CAMPAIGN_FAILURE_SCENARIOS",
    "BSC_CAMPAIGN_ELIGIBILITY",
    "BSC_CAMPAIGN_MAX_SKIP_PPM",
    "BSC_CAMPAIGN_MAX_CONSECUTIVE_MISSES",
    "BSC_CAMPAIGN_MIN_COVERED_PPM",
    "BSC_CAMPAIGN_UNCOVERED",
    "BSC_CAMPAIGN_PRODUCER_WITNESS",
    "BSC_CAMPAIGN_CLOCK_ERROR_BOUND_NS",
    "BSC_CAMPAIGN_PLACEMENT",
    "BSC_CAMPAIGN_FINALITY_BLOCKS",
    "BSC_CAMPAIGN_REORG_HORIZON_BLOCKS",
    "BSC_CAMPAIGN_AUTHORIZATION_REF",
    "BSC_CAMPAIGN_AMOUNT_LIMIT_WEI",
    "BSC_CAMPAIGN_COST_LIMIT_WEI",
    "BSC_CAMPAIGN_STOP_CONDITIONS",
    "BSC_CAMPAIGN_REVIEWER",
    "BSC_CAMPAIGN_RELEASE_SCOPE",
];

/// One enabled cell's frozen minimums.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellPlan {
    pub cell: &'static str,
    pub min_attempts: u64,
    pub min_duration_s: u64,
}

/// A reviewed, complete manifest's values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub build_sha256: [u8; 32],
    pub config_sha256: [u8; 32],
    pub chain_id: u64,
    /// The host every campaign run's manifest names.
    pub host: String,
    /// The builders (the permitted sinks) and the sources an attempt may
    /// have gone through and been triggered by.
    pub adapters: Vec<LiveAdapter>,
    pub sources: Vec<SourceId>,
    /// The operator's own launcher wallets: a launch by one of them (or by
    /// the buyer) is a self-launch.
    pub self_launchers: Vec<[u8; 20]>,
    pub cells: Vec<CellPlan>,
    /// The coverage floor: the share of the observed launches left
    /// without an attempt (ppm of them), and how many in a row.
    pub max_skip_ppm: u64,
    pub max_consecutive_misses: u64,
    /// The share of the campaign's time its sealed runs must cover (ppm):
    /// launches while no run was up are no skips, the time is bounded
    /// (review N3).
    pub min_covered_ppm: u64,
    /// What chain no sealed run followed does: holds the campaign open
    /// (`OPEN`, master plan §13 J: no launch leaves the denominator
    /// unseen) or only counts against the coverage floor (`BOUNDED`, the
    /// operator's explicit decision, a floor above zero).
    pub uncovered_bounded: bool,
    pub clock_error_bound_ns: u64,
    pub finality_blocks: u64,
    pub reorg_horizon_blocks: u64,
    pub amount_limit_wei: U256,
    pub cost_limit_wei: U256,
    pub release_scope: &'static str,
}

/// The manifest file, parsed strictly: every key known and set once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub reviewed: bool,
    /// The keys left empty.
    pub missing: Vec<&'static str>,
    /// Its values, when it is reviewed and complete.
    pub plan: Option<Result<Plan, String>>,
    pub sha256: [u8; 32],
}

fn hex32(v: &str) -> Result<[u8; 32], String> {
    let b = alloy_primitives::hex::decode(v.trim_start_matches("0x")).map_err(|e| e.to_string())?;
    b.try_into().map_err(|_| "not 32 bytes".to_string())
}

fn cell_of(name: &str) -> Option<&'static str> {
    CELLS.into_iter().find(|c| *c == name)
}

/// The comma-separated entries of `text`, each once.
fn list<T: PartialEq>(
    key: &str,
    text: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Vec<T>, String> {
    let mut out = Vec::new();
    for item in text.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        let v = parse(item).ok_or_else(|| format!("{key}: {item} is not known"))?;
        if out.contains(&v) {
            return Err(format!("{key}: {item} is named twice"));
        }
        out.push(v);
    }
    Ok(out)
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "the manifest is not UTF-8")?;
        let mut values: HashMap<&str, &str> = HashMap::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("not KEY=VALUE: {line}"))?;
            if values.insert(k.trim(), v.trim()).is_some() {
                return Err(format!("{} is set twice", k.trim()));
            }
        }
        let reviewed = match values.remove("BSC_CAMPAIGN_STATE") {
            Some("PROPOSED") => false,
            Some("REVIEWED") => true,
            Some(other) => {
                return Err(format!(
                    "BSC_CAMPAIGN_STATE={other} is neither PROPOSED nor REVIEWED"
                ));
            }
            None => return Err("BSC_CAMPAIGN_STATE is required".into()),
        };
        let mut missing = Vec::new();
        let mut v: HashMap<&'static str, &str> = HashMap::new();
        for key in MANIFEST_KEYS {
            let value = values
                .remove(key)
                .ok_or_else(|| format!("{key} is required (it may be empty while PROPOSED)"))?;
            if value.is_empty() {
                missing.push(key);
            }
            v.insert(key, value);
        }
        if let Some(k) = values.keys().next() {
            return Err(format!("{k} is not a manifest key"));
        }
        // A cell not enabled needs no minimums.
        let enabled: Vec<&str> = v["BSC_CAMPAIGN_CELLS"]
            .split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .collect();
        missing.retain(|k| {
            CELLS
                .iter()
                .find(|c| k.starts_with(&format!("BSC_CAMPAIGN_{c}_")))
                .is_none_or(|c| enabled.contains(c))
        });
        let plan = (reviewed && missing.is_empty()).then(|| Self::plan(&v, &enabled));
        Ok(Manifest {
            reviewed,
            missing,
            plan,
            sha256: darkbot_bsc_record::sha256(bytes),
        })
    }

    fn plan(v: &HashMap<&'static str, &str>, enabled: &[&str]) -> Result<Plan, String> {
        let num = |k: &str, min: u64, max: u64| -> Result<u64, String> {
            let n: u64 = v[k].parse().map_err(|_| format!("{k} is not a number"))?;
            (min..=max)
                .contains(&n)
                .then_some(n)
                .ok_or_else(|| format!("{k}={n} is outside {min}..={max}"))
        };
        let wei = |k: &str| -> Result<U256, String> {
            let n = U256::from_str_radix(v[k], 10).map_err(|_| format!("{k} is not wei"))?;
            (!n.is_zero())
                .then_some(n)
                .ok_or_else(|| format!("{k} is zero"))
        };
        let mut cells = Vec::new();
        for name in enabled {
            let cell = cell_of(name).ok_or_else(|| format!("{name} is not a campaign cell"))?;
            if cells.iter().any(|c: &CellPlan| c.cell == cell) {
                return Err(format!("{cell} is named twice"));
            }
            cells.push(CellPlan {
                cell,
                min_attempts: num(&format!("BSC_CAMPAIGN_{cell}_MIN_ATTEMPTS"), 1, u64::MAX)?,
                min_duration_s: num(&format!("BSC_CAMPAIGN_{cell}_MIN_DURATION_S"), 1, u64::MAX)?,
            });
        }
        if cells.is_empty() {
            return Err("BSC_CAMPAIGN_CELLS names no cell".into());
        }
        if v["BSC_CAMPAIGN_PLACEMENT"] != FIRST_AFTER_LAUNCH {
            return Err(format!(
                "BSC_CAMPAIGN_PLACEMENT={} is not {FIRST_AFTER_LAUNCH}",
                v["BSC_CAMPAIGN_PLACEMENT"]
            ));
        }
        if v["BSC_CAMPAIGN_ELIGIBILITY"] != ELIGIBILITY_C3 {
            return Err(format!(
                "BSC_CAMPAIGN_ELIGIBILITY={} is not {ELIGIBILITY_C3}",
                v["BSC_CAMPAIGN_ELIGIBILITY"]
            ));
        }
        let adapters = list(
            "BSC_CAMPAIGN_ADAPTERS",
            v["BSC_CAMPAIGN_ADAPTERS"],
            LiveAdapter::parse,
        )?;
        let sources = list(
            "BSC_CAMPAIGN_SOURCES",
            v["BSC_CAMPAIGN_SOURCES"],
            SourceId::parse,
        )?;
        if adapters.is_empty() || sources.is_empty() {
            return Err("BSC_CAMPAIGN_ADAPTERS and BSC_CAMPAIGN_SOURCES name one at least".into());
        }
        let self_launchers = match v["BSC_CAMPAIGN_SELF_LAUNCHERS"] {
            "NONE" => Vec::new(),
            text => list("BSC_CAMPAIGN_SELF_LAUNCHERS", text, |a| {
                let b = alloy_primitives::hex::decode(a.strip_prefix("0x")?).ok()?;
                <[u8; 20]>::try_from(b).ok()
            })?,
        };
        let release_scope = cell_of(v["BSC_CAMPAIGN_RELEASE_SCOPE"])
            .ok_or("BSC_CAMPAIGN_RELEASE_SCOPE is not a campaign cell")?;
        Ok(Plan {
            build_sha256: hex32(v["BSC_CAMPAIGN_BUILD_SHA256"])
                .map_err(|e| format!("BSC_CAMPAIGN_BUILD_SHA256: {e}"))?,
            config_sha256: hex32(v["BSC_CAMPAIGN_CONFIG_SHA256"])
                .map_err(|e| format!("BSC_CAMPAIGN_CONFIG_SHA256: {e}"))?,
            chain_id: num("BSC_CAMPAIGN_CHAIN_ID", 1, u64::MAX)?,
            host: v["BSC_CAMPAIGN_HOST"].to_string(),
            adapters,
            sources,
            self_launchers,
            cells,
            max_skip_ppm: num("BSC_CAMPAIGN_MAX_SKIP_PPM", 0, 1_000_000)?,
            max_consecutive_misses: num("BSC_CAMPAIGN_MAX_CONSECUTIVE_MISSES", 0, u64::MAX)?,
            min_covered_ppm: num("BSC_CAMPAIGN_MIN_COVERED_PPM", 0, 1_000_000)?,
            uncovered_bounded: match v["BSC_CAMPAIGN_UNCOVERED"] {
                "OPEN" => false,
                "BOUNDED" if num("BSC_CAMPAIGN_MIN_COVERED_PPM", 1, 1_000_000).is_ok() => true,
                "BOUNDED" => {
                    return Err("BSC_CAMPAIGN_UNCOVERED=BOUNDED needs a floor above zero".into());
                }
                other => {
                    return Err(format!(
                        "BSC_CAMPAIGN_UNCOVERED={other} is neither OPEN nor BOUNDED"
                    ));
                }
            },
            clock_error_bound_ns: num("BSC_CAMPAIGN_CLOCK_ERROR_BOUND_NS", 0, STRICT_LIMIT_NS)?,
            finality_blocks: num("BSC_CAMPAIGN_FINALITY_BLOCKS", 1, u64::MAX)?,
            reorg_horizon_blocks: num("BSC_CAMPAIGN_REORG_HORIZON_BLOCKS", 1, u64::MAX)?,
            amount_limit_wei: wei("BSC_CAMPAIGN_AMOUNT_LIMIT_WEI")?,
            cost_limit_wei: wei("BSC_CAMPAIGN_COST_LIMIT_WEI")?,
            release_scope,
        })
    }
}

/// The manifest's hash, committed before the campaign's first attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Precommit {
    pub manifest_sha256: [u8; 32],
    pub decided_by: String,
    pub committed_unix_ms: u64,
    /// SHA-256 of the record's bytes.
    pub sha256: [u8; 32],
}

impl Precommit {
    pub const PATH: &str = "bsc-runtime/live-campaign-precommit.dbrec";

    /// Commits `manifest` for the campaign in `rundir`, once: only a
    /// reviewed, complete one with valid values; never replaced.
    pub fn write(
        rundir: &Path,
        manifest: &Manifest,
        decided_by: &str,
        now_unix_ms: u64,
    ) -> Result<Self, String> {
        match &manifest.plan {
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(format!("the manifest is not valid: {e}")),
            None if !manifest.reviewed => {
                return Err("only a REVIEWED manifest is committed".into());
            }
            None => {
                return Err(format!(
                    "the manifest leaves {} empty",
                    manifest.missing.join(", ")
                ));
            }
        }
        if decided_by.trim().is_empty() {
            return Err("the decider is required".into());
        }
        let mut d = Draft::new(&lp::SCHEMA);
        d.set(lp::MANIFEST_SHA256, Value::B256(manifest.sha256))
            .set(lp::SCOPE, SCOPE)
            .set(lp::PROVENANCE_TOOL, "bsc_runtime live campaign-precommit")
            .set(lp::PROVENANCE_OBSERVED_UNIX_MS, now_unix_ms)
            .set(lp::DECIDED_BY, decided_by.trim());
        let bytes = d
            .encode()
            .map_err(|e| format!("live-campaign-precommit: {e:?}"))?;
        let path = rundir.join(Self::PATH);
        let e = |e: std::io::Error| format!("{}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::os::unix::fs::DirBuilderExt::mode(
                std::fs::DirBuilder::new().recursive(true),
                0o700,
            )
            .create(dir)
            .map_err(e)?;
        }
        crate::sources::write_new(&path, &bytes).map_err(e)?;
        Self::decode(&bytes)
    }

    pub fn read(rundir: &Path) -> Result<Option<Self>, String> {
        let path = rundir.join(Self::PATH);
        match std::fs::read(&path) {
            Ok(bytes) => Self::decode(&bytes)
                .map(Some)
                .map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let e = |e: darkbot_bsc_record::RecordError| format!("live-campaign-precommit: {e:?}");
        let r = darkbot_bsc_record::decode(&lp::SCHEMA, bytes).map_err(e)?;
        let scope = r.text(lp::SCOPE).map_err(e)?;
        if scope != SCOPE {
            return Err(format!(
                "live-campaign-precommit: scope {scope} is not {SCOPE}"
            ));
        }
        let decided_by = r.text(lp::DECIDED_BY).map_err(e)?;
        if decided_by.trim().is_empty() {
            return Err("live-campaign-precommit: no decider".into());
        }
        Ok(Precommit {
            manifest_sha256: r.b256(lp::MANIFEST_SHA256).map_err(e)?,
            decided_by: decided_by.to_string(),
            committed_unix_ms: r.u64(lp::PROVENANCE_OBSERVED_UNIX_MS).map_err(e)?,
            sha256: darkbot_bsc_record::sha256(bytes),
        })
    }
}

// ---- the attempts ----

/// One adapter of an attempt's dispatch, with its answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterRow {
    pub adapter: u8,
    pub builder: Option<LiveAdapter>,
    pub outcome: &'static str,
    pub boundary: &'static str,
    pub qualified: bool,
    pub write_check_ns: Option<u64>,
    pub boundary_ns: Option<u64>,
    pub queue_ns: Option<u64>,
    pub answer: Option<&'static str>,
    pub answer_ns: Option<u64>,
}

/// An attempt's local timing on the runtime's clock (§8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub clock_id: u32,
    pub ingress_ns: u64,
    pub select_ns: u64,
    /// The first qualified send time among the adapters.
    pub send_ns: Option<u64>,
    /// The first acknowledgement among them.
    pub ack_ns: Option<u64>,
    pub deadline_misses: u8,
}

/// Each component's verdict and the attempt's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdicts {
    pub acquisition: &'static str,
    pub canonical: &'static str,
    pub timing: &'static str,
    pub placement: &'static str,
    pub capture: &'static str,
    pub local: &'static str,
    pub economics: &'static str,
    pub verdict: &'static str,
}

/// The counters of every attempt up to one: BUYs and acquisitions of
/// every LIVE BUY (each rate's denominator the attempts), the verdicts of
/// the launch BUYs (a Strategy C entry has none).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub attempts: u64,
    pub acquired: u64,
    pub passed: u64,
    pub failed: u64,
    pub undecided: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub ordinal: u64,
    pub attempt: [u8; 32],
    pub tx_hash: [u8; 32],
    pub kind: &'static str,
    pub cell: &'static str,
    pub token: [u8; 20],
    pub launch_hash: Option<[u8; 32]>,
    pub trigger: Option<SourceId>,
    /// The run whose journal holds its release.
    pub run: Option<u64>,
    pub state: &'static str,
    pub block: Option<(u64, [u8; 32], u64)>,
    pub tokens: Option<U256>,
    pub eth_wei: Option<U256>,
    pub gas_wei: Option<U256>,
    pub input_wei: Option<U256>,
    pub placement: Option<crate::live_ledger::Placement>,
    pub chain_id: Option<u64>,
    /// When its first fact was written (Unix ms).
    pub written_ms: u64,
    pub local: Option<Local>,
    pub adapters: Vec<AdapterRow>,
    pub window: Window,
    pub verdicts: Verdicts,
    pub reasons: Vec<String>,
    pub counters: Counters,
}

impl Attempt {
    /// Not resolved by the chain (its window over or not).
    pub fn unresolved(&self) -> bool {
        matches!(self.state, "RELEASED" | "UNKNOWN" | "DROPPED")
    }
}

/// The G0.8 result a producer timing verdict is of: what binds it to one
/// canonical execution (master plan §11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct G08Binding {
    pub sha256: [u8; 32],
    /// Its own verdict: only a PASS is a canonical acquisition to time
    /// (review L2, as `g08_producer_timing` requires).
    pub verdict: String,
    pub attempt_id: String,
    pub buy_tx: [u8; 32],
    pub launch_tx: [u8; 32],
    pub buy_block_hash: [u8; 32],
}

/// A producer timing verdict (`g08_producer_timing`) given for an attempt,
/// with the G0.8 result it names (`<id>.g08.dbrec` beside it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Witness {
    pub attempt_id: String,
    pub verdict: String,
    pub strict_limit_ns: u64,
    pub upper_bound_ns: u64,
    pub remaining_uncertainty_ns: u64,
    pub g08_sha256: [u8; 32],
    pub g08: Option<Result<G08Binding, String>>,
}

impl Witness {
    pub fn decode(bytes: &[u8], g08: Option<&[u8]>) -> Result<Self, String> {
        let e = |e: darkbot_bsc_record::RecordError| format!("producer timing verdict: {e:?}");
        let r = darkbot_bsc_record::decode(&tv::SCHEMA, bytes).map_err(e)?;
        let g08 = g08.map(|b| {
            let e = |e: darkbot_bsc_record::RecordError| format!("G0.8 result: {e:?}");
            let g = darkbot_bsc_record::decode(&gr::SCHEMA, b).map_err(e)?;
            Ok(G08Binding {
                sha256: darkbot_bsc_record::sha256(b),
                verdict: g.text(gr::VERDICT).map_err(e)?.to_string(),
                attempt_id: g.text(gr::ATTEMPT_ID).map_err(e)?.to_string(),
                buy_tx: g.b256(gr::BUY_TX).map_err(e)?,
                launch_tx: g.b256(gr::LAUNCH_TX).map_err(e)?,
                buy_block_hash: g.b256(gr::BUY_BLOCK_HASH).map_err(e)?,
            })
        });
        Ok(Witness {
            attempt_id: r.text(tv::ATTEMPT_ID).map_err(e)?.to_string(),
            verdict: r.text(tv::VERDICT).map_err(e)?.to_string(),
            strict_limit_ns: r.u64(tv::STRICT_LIMIT_NS).map_err(e)?,
            upper_bound_ns: r.u64(tv::CONSERVATIVE_UPPER_BOUND_NS).map_err(e)?,
            remaining_uncertainty_ns: r.u64(tv::REMAINING_UNCERTAINTY_NS).map_err(e)?,
            g08_sha256: r.b256(tv::G08_SHA256).map_err(e)?,
            g08,
        })
    }
}

/// Producer timing verdicts by attempt id, as read.
pub type Witnesses = HashMap<String, Result<Witness, String>>;

/// The attempt id a producer timing verdict names for `attempt`.
pub fn attempt_id(attempt: &[u8; 32]) -> String {
    format!("0x{}", alloy_primitives::hex::encode(attempt))
}

/// The canonical execution an attempt's witness must be of.
struct Execution<'a> {
    attempt: &'a [u8; 32],
    tx_hash: &'a [u8; 32],
    launch_hash: Option<&'a [u8; 32]>,
    block_hash: Option<&'a [u8; 32]>,
}

/// The timing component of `x` from its witness, if one was given: bound
/// to its own G0.8 result (attempt, BUY and launch transaction, BUY block)
/// and that result to the verdict by hash; the bound alone decides.
fn timing(
    x: &Execution<'_>,
    witness: Option<&Result<Witness, String>>,
    clock_bound_ns: Option<u64>,
    reasons: &mut Vec<String>,
) -> &'static str {
    let (verdict, why) = judged(x, witness, clock_bound_ns);
    reasons.extend(why);
    verdict
}

fn judged(
    x: &Execution<'_>,
    witness: Option<&Result<Witness, String>>,
    clock_bound_ns: Option<u64>,
) -> (&'static str, Option<String>) {
    let unknown = |why: &str| (UNKNOWN, Some(why.to_string()));
    let w = match witness {
        None => return unknown("NO_PRODUCER_WITNESS"),
        Some(Err(e)) => return (UNKNOWN, Some(format!("PRODUCER_WITNESS_UNREADABLE: {e}"))),
        Some(Ok(w)) => w,
    };
    let id = attempt_id(x.attempt);
    if w.attempt_id != id {
        return unknown("PRODUCER_WITNESS_OF_ANOTHER_ATTEMPT");
    }
    let g = match &w.g08 {
        None => return unknown("PRODUCER_WITNESS_WITHOUT_ITS_G08_RESULT"),
        Some(Err(e)) => return (UNKNOWN, Some(format!("G08_RESULT_UNREADABLE: {e}"))),
        Some(Ok(g)) => g,
    };
    if g.sha256 != w.g08_sha256 {
        return unknown("PRODUCER_WITNESS_OF_ANOTHER_G08_RESULT");
    }
    if g.verdict != "PASS" {
        return unknown("G08_RESULT_NOT_PASS");
    }
    if g.attempt_id != id
        || g.buy_tx != *x.tx_hash
        || Some(&g.launch_tx) != x.launch_hash
        || Some(&g.buy_block_hash) != x.block_hash
    {
        return unknown("G08_RESULT_OF_ANOTHER_EXECUTION");
    }
    if w.strict_limit_ns != STRICT_LIMIT_NS {
        return unknown("PRODUCER_WITNESS_OTHER_LIMIT");
    }
    if clock_bound_ns.is_some_and(|b| w.remaining_uncertainty_ns > b) {
        return unknown("PRODUCER_WITNESS_UNCERTAINTY_ABOVE_BOUND");
    }
    match (w.verdict.as_str(), w.upper_bound_ns < STRICT_LIMIT_NS) {
        ("PASS", true) => (PASS, None),
        ("FAIL", false) => (FAIL, Some("PRODUCER_BOUND_NOT_BELOW_LIMIT".into())),
        _ => unknown("PRODUCER_WITNESS_INCONSISTENT"),
    }
}

/// The attempt's verdict from its components: one FAIL fails it; it
/// passes only when every component passed (or does not apply).
fn overall(c: [&'static str; 7]) -> &'static str {
    if c.contains(&FAIL) {
        FAIL
    } else if c.iter().all(|v| *v == PASS || *v == NOT_APPLICABLE) {
        PASS
    } else {
        UNKNOWN
    }
}

/// What the evaluation reads besides the journals and the ledger.
pub struct Inputs<'a> {
    /// [`MARKET_WINDOW_MS`] (tests: their own).
    pub window_ms: u64,
    pub plan: Option<&'a Plan>,
    pub witnesses: &'a Witnesses,
    /// The operator's own accounts the configuration names
    /// (`BSC_EXEC_REFUSED_ACCOUNTS`): a launch by one is a self-launch
    /// whatever the manifest lists (review L1).
    pub own_accounts: &'a [[u8; 20]],
}

/// Every written LIVE BUY of `facts` (ledger order, with their write
/// times), evaluated against the journals.
pub fn attempts(facts: &[(Fact, u64)], j: &Journaled, inputs: &Inputs<'_>) -> Vec<Attempt> {
    let plain: Vec<Fact> = facts.iter().map(|(f, _)| f.clone()).collect();
    let book = fold(&plain);
    let head = j.head();
    let mut counters = Counters::default();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (fact, written_ms) in facts {
        let first = matches!(fact.kind, FactKind::Released | FactKind::Unknown)
            && fact.intent == LiveIntentKind::Buy
            && seen.insert(fact.tx_hash);
        let Some(tx) = first.then(|| book.tx(&fact.tx_hash)).flatten() else {
            continue;
        };
        let a = attempt(tx, *written_ms, j, inputs, head, out.len() as u64 + 1);
        counters.attempts += 1;
        counters.acquired += u64::from(a.verdicts.acquisition == PASS);
        match a.verdicts.verdict {
            PASS => counters.passed += 1,
            FAIL => counters.failed += 1,
            UNKNOWN => counters.undecided += 1,
            _ => {}
        }
        out.push(Attempt { counters, ..a });
    }
    out
}

fn attempt(
    tx: &Tx,
    written_ms: u64,
    j: &Journaled,
    inputs: &Inputs<'_>,
    head: Option<u64>,
    ordinal: u64,
) -> Attempt {
    let r = &tx.released;
    let c3 = r.component == ComponentId::ExecC3;
    let release = j.releases.get(&r.tx_hash);
    let mut reasons = Vec::new();
    let inclusion = tx.inclusion.as_ref().filter(|_| !tx.orphaned);
    let in_block =
        matches!(tx.state, FactKind::Included | FactKind::Reverted) && inclusion.is_some();
    let pending = matches!(tx.state, FactKind::Released | FactKind::Unknown);
    // Review L1: a window over proves nothing about the signed bytes: the
    // attempt stays unresolved until its nonce is taken.
    let dropped = pending && head.is_some_and(|h| tx.window_over(h));
    let state = match tx.state {
        _ if dropped => "DROPPED",
        s => s.name(),
    };
    let tokens = inclusion.and_then(|f| f.tokens);
    let acquisition = match tx.state {
        FactKind::Included if tokens.is_some_and(|t| !t.is_zero()) => PASS,
        FactKind::Included => {
            reasons.push("INCLUDED_WITHOUT_OWN_TOKENS".into());
            FAIL
        }
        FactKind::Reverted | FactKind::Replaced => FAIL,
        _ if dropped => {
            reasons.push("WINDOW_OVER_UNRESOLVED".into());
            UNKNOWN
        }
        _ => UNKNOWN,
    };
    let block = inclusion.and_then(|f| f.block);
    // Review M3: final only in the journaled chain, deep enough.
    let canonical = match (in_block, block) {
        (true, Some((number, hash, _))) if !j.canonical(number, &hash) => {
            reasons.push("INCLUSION_NOT_IN_THE_JOURNALED_CHAIN".into());
            UNKNOWN
        }
        (true, Some((number, ..))) => match (inputs.plan, head) {
            (Some(p), Some(h)) if h >= number.saturating_add(p.finality_blocks) => PASS,
            _ => UNKNOWN,
        },
        _ if pending => UNKNOWN,
        _ => NOT_APPLICABLE,
    };
    let launch_hash = release.map(|x| x.l.launch_hash).filter(|_| c3);
    let launch = launch_hash.and_then(|h| j.launch(&h));
    let launcher = tx.launcher.or_else(|| j.creators.get(&r.token).copied());
    let sender = r.origin.map(|o| o.sender);
    // Review H3: a self-launch is the operator's: by the buyer or by a
    // launcher wallet the manifest names; another creator is external only
    // when the manifest names the operator's launchers.
    let cell = match (c3, launcher) {
        (false, _) => STRATEGY_ENTRY,
        (true, Some(l))
            if Some(l) == sender
                || inputs.own_accounts.contains(&l)
                || inputs.plan.is_some_and(|p| p.self_launchers.contains(&l)) =>
        {
            SELF_LAUNCH
        }
        (true, Some(_)) if inputs.plan.is_some() => EXTERNAL_FULL_LAUNCH,
        (true, _) => CELL_UNKNOWN,
    };
    let horizon = inputs.plan.map(|p| p.reorg_horizon_blocks);
    let window = match c3 {
        true => window(j, launch, r.token, r.tx_hash, inputs.window_ms, horizon),
        false => Window {
            window_ms: inputs.window_ms,
            why_incomplete: Some("NO_LAUNCH"),
            ..Window::default()
        },
    };
    let attempt_id = release.map_or(r.tx_hash, |x| x.l.attempt);
    let (timing, placement, capture) = match c3 {
        false => (NOT_APPLICABLE, NOT_APPLICABLE, NOT_APPLICABLE),
        true => {
            let timing = timing(
                &Execution {
                    attempt: &attempt_id,
                    tx_hash: &r.tx_hash,
                    launch_hash: launch_hash.as_ref(),
                    block_hash: block.as_ref().map(|b| &b.1),
                },
                inputs.witnesses.get(&self::attempt_id(&attempt_id)),
                inputs.plan.map(|p| p.clock_error_bound_ns),
                &mut reasons,
            );
            let placement = match (tx.state, tx.placement.map(|p| p.verdict)) {
                (FactKind::Included, Some(PlacementVerdict::First)) => PASS,
                (FactKind::Included, Some(v)) => {
                    reasons.push(format!("PLACEMENT_{}", v.name()));
                    FAIL
                }
                (FactKind::Included, None) => UNKNOWN,
                _ if pending => UNKNOWN,
                _ => NOT_APPLICABLE,
            };
            let capture = match window.why_incomplete {
                None => PASS,
                Some(why) => {
                    reasons.push(format!("WINDOW_{why}"));
                    UNKNOWN
                }
            };
            (timing, placement, capture)
        }
    };
    let mut adapters = Vec::new();
    let local = release.map(|x| {
        let l = &x.l;
        let mut send_ns: Option<u64> = None;
        let mut ack_ns: Option<u64> = None;
        for a in &l.adapters {
            let answer = j.answers.get(&(l.tx_hash, a.adapter));
            if let Some(t) = a.t_send_ns() {
                send_ns = Some(send_ns.map_or(t, |x| x.min(t)));
            }
            if let Some((code, Some(at))) = answer
                && rj::ANSWERS.get(usize::from(*code)) == Some(&"ACK")
            {
                ack_ns = Some(ack_ns.map_or(*at, |x| x.min(*at)));
            }
            adapters.push(AdapterRow {
                adapter: a.adapter,
                builder: a
                    .builder
                    .and_then(|b| LiveAdapter::ALL.get(usize::from(b)).copied()),
                outcome: rj::SEND_OUTCOMES[usize::from(a.outcome)],
                boundary: rj::BOUNDARIES[usize::from(a.boundary)],
                qualified: a.record_qualifies && a.transport_qualified,
                write_check_ns: a.write_check_ns,
                boundary_ns: a.boundary_ns,
                queue_ns: a.queue_ns,
                answer: answer.and_then(|(c, _)| rj::ANSWERS.get(usize::from(*c)).copied()),
                answer_ns: answer.and_then(|(_, at)| *at),
            });
        }
        Local {
            clock_id: l.clock_id,
            ingress_ns: l.times[0],
            select_ns: l.times[6],
            send_ns,
            ack_ns,
            deadline_misses: l.deadline_misses,
        }
    });
    let local_verdict = match local {
        None => {
            reasons.push("NO_RELEASE_RECORD".into());
            UNKNOWN
        }
        Some(l) if l.deadline_misses > 0 => {
            reasons.push("LOCAL_DEADLINE_MISSED".into());
            FAIL
        }
        Some(Local { send_ns: None, .. }) => {
            reasons.push("NO_QUALIFIED_SEND_TIME".into());
            UNKNOWN
        }
        Some(_) => PASS,
    };
    let economics = match (in_block, inclusion) {
        (true, Some(f))
            if f.gas_wei.is_some()
                && (tx.state == FactKind::Reverted
                    || (f.eth_wei.is_some() && f.tokens.is_some())) =>
        {
            PASS
        }
        (true, _) => UNKNOWN,
        _ if pending => UNKNOWN,
        _ => NOT_APPLICABLE,
    };
    // Review L5: a Strategy C entry has no attempt verdict of §13 J.
    let verdict = match c3 {
        true => overall([
            acquisition,
            canonical,
            timing,
            placement,
            capture,
            local_verdict,
            economics,
        ]),
        false => NOT_APPLICABLE,
    };
    Attempt {
        ordinal,
        attempt: attempt_id,
        tx_hash: r.tx_hash,
        kind: if c3 { "C3_BUY" } else { "STRATEGY_BUY" },
        cell,
        token: r.token,
        launch_hash,
        trigger: release.and_then(|x| x.l.trigger),
        run: release.map(|x| x.run),
        state,
        block,
        tokens,
        eth_wei: inclusion.and_then(|f| f.eth_wei),
        gas_wei: inclusion.and_then(|f| f.gas_wei),
        input_wei: r.input_wei,
        placement: tx.placement,
        chain_id: r.origin.map(|o| o.chain_id),
        written_ms,
        local,
        adapters,
        window,
        verdicts: Verdicts {
            acquisition,
            canonical,
            timing,
            placement,
            capture,
            local: local_verdict,
            economics,
            verdict,
        },
        reasons,
        counters: Counters::default(),
    }
}

impl Attempt {
    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&la::SCHEMA);
        let w = &self.window;
        d.set(la::ATTEMPT, Value::B256(self.attempt))
            .set(la::TX_HASH, Value::B256(self.tx_hash))
            .set(la::PROVENANCE_TOOL, TOOL)
            .set(la::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(la::ORDINAL, self.ordinal)
            .set(la::KIND, self.kind)
            .set(la::CELL, self.cell)
            .set(la::TOKEN, Value::Address(self.token))
            .set(la::STATE, self.state)
            .set(
                la::WINDOW,
                if w.complete() {
                    "COMPLETE"
                } else {
                    "INCOMPLETE"
                },
            )
            .set(la::WINDOW_MS, w.window_ms)
            .set(la::T0_EXACT, false)
            .set(la::TRADES, w.trades)
            .set(la::BUYS, w.buys)
            .set(la::SELLS, w.sells)
            .set(la::BUY_WEI, Value::U256(w.buy_wei.to_be_bytes()))
            .set(la::SELL_WEI, Value::U256(w.sell_wei.to_be_bytes()))
            .set(la::ACQUISITION, self.verdicts.acquisition)
            .set(la::CANONICAL, self.verdicts.canonical)
            .set(la::TIMING, self.verdicts.timing)
            .set(la::PLACEMENT_VERDICT, self.verdicts.placement)
            .set(la::CAPTURE, self.verdicts.capture)
            .set(la::LOCAL, self.verdicts.local)
            .set(la::ECONOMICS, self.verdicts.economics)
            .set(la::VERDICT, self.verdicts.verdict)
            .set(la::ATTEMPTS, self.counters.attempts)
            .set(la::ACQUIRED, self.counters.acquired)
            .set(la::PASSED, self.counters.passed)
            .set(la::FAILED, self.counters.failed)
            .set(la::UNDECIDED, self.counters.undecided)
            .set_opt(la::RUN, self.run)
            .set_opt(la::LAUNCH_HASH, self.launch_hash.map(Value::B256))
            .set_opt(la::TRIGGER, self.trigger.map(SourceId::name));
        let word = |v: U256| Value::U256(v.to_be_bytes());
        if let Some((n, h, ms)) = self.block {
            d.set(la::BLOCK_NUMBER, n)
                .set(la::BLOCK_HASH, Value::B256(h))
                .set(la::BLOCK_MS, ms);
        }
        for (tag, v) in [
            (la::TOKENS, self.tokens),
            (la::ETH_WEI, self.eth_wei),
            (la::GAS_WEI, self.gas_wei),
            (la::OWN_PRICE, w.own_price),
        ] {
            d.set_opt(tag, v.map(word));
        }
        if let Some(p) = self.placement {
            d.set(la::PLACEMENT, p.verdict.name())
                .set(la::OWN_INDEX, p.own_index)
                .set(la::BUYS_BEFORE, p.buys_before)
                .set_opt(la::LAUNCH_INDEX, p.launch_index);
        }
        if let Some(l) = self.local {
            d.set(la::CLOCK_ID, u64::from(l.clock_id))
                .set(la::T_INGRESS_NS, l.ingress_ns)
                .set(la::T_SELECT_NS, l.select_ns)
                .set(la::DEADLINE_MISSES, u64::from(l.deadline_misses))
                .set_opt(la::T_SEND_NS, l.send_ns)
                .set_opt(la::T_ACK_NS, l.ack_ns)
                .set_opt(
                    la::INGRESS_TO_SEND_NS,
                    l.send_ns.and_then(|s| s.checked_sub(l.ingress_ns)),
                )
                .set_opt(
                    la::SELECT_TO_SEND_NS,
                    l.send_ns.and_then(|s| s.checked_sub(l.select_ns)),
                )
                .set_opt(
                    la::SEND_TO_ACK_NS,
                    l.send_ns.zip(l.ack_ns).and_then(|(s, a)| a.checked_sub(s)),
                );
        }
        for a in &self.adapters {
            let mut n = Draft::new(&la::adapter::SCHEMA);
            n.set(la::adapter::ADAPTER, u64::from(a.adapter))
                .set(la::adapter::OUTCOME, a.outcome)
                .set(la::adapter::BOUNDARY, a.boundary)
                .set(la::adapter::QUALIFIED, a.qualified)
                .set_opt(la::adapter::T_WRITE_CHECK_NS, a.write_check_ns)
                .set_opt(la::adapter::T_BOUNDARY_NS, a.boundary_ns)
                .set_opt(la::adapter::T_QUEUE_NS, a.queue_ns)
                .set_opt(la::adapter::T_ANSWER_NS, a.answer_ns)
                .set_opt(la::adapter::ANSWER, a.answer)
                .set_opt(la::adapter::BUILDER, a.builder.map(LiveAdapter::name));
            d.push(la::ADAPTERS, n.into_nested());
        }
        d.set_opt(la::WINDOW_WHY, w.why_incomplete)
            .set_opt(la::LAUNCH_BLOCK, w.launch_block)
            .set_opt(la::LAUNCH_MS, w.launch_ms)
            .set_opt(la::WINDOW_END_BLOCK, w.end_block)
            .set_opt(
                la::PRICE_VS_FIRST_EXTERNAL_PPM,
                w.price_vs_first_external_ppm,
            );
        if let Some((tx, block, index, price)) = w.first_external {
            d.set(la::FIRST_EXTERNAL_BUY_TX, Value::B256(tx))
                .set(la::FIRST_EXTERNAL_BUY_BLOCK, block)
                .set(la::FIRST_EXTERNAL_BUY_INDEX, index)
                .set(la::FIRST_EXTERNAL_PRICE, word(price));
        }
        for r in &self.reasons {
            d.push(la::REASONS, Value::Text(r.clone()));
        }
        d.encode().map_err(|e| format!("live-attempt: {e:?}"))
    }

    /// One line of the evaluation's text view.
    pub fn line(&self) -> String {
        let c = &self.counters;
        format!(
            "attempt {} tx=0x{} kind={} cell={} state={} verdict={} acquisition={} canonical={} timing={} placement={} capture={} local={} economics={} window={} trades={} first_external_ppm={} acquired={}/{}",
            self.ordinal,
            alloy_primitives::hex::encode(self.tx_hash),
            self.kind,
            self.cell,
            self.state,
            self.verdicts.verdict,
            self.verdicts.acquisition,
            self.verdicts.canonical,
            self.verdicts.timing,
            self.verdicts.placement,
            self.verdicts.capture,
            self.verdicts.local,
            self.verdicts.economics,
            self.window.why_incomplete.unwrap_or("COMPLETE"),
            self.window.trades,
            self.window
                .price_vs_first_external_ppm
                .map_or("-".into(), |v| v.to_string()),
            c.acquired,
            c.attempts,
        )
    }
}

// ---- the campaign ----

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellRow {
    pub cell: &'static str,
    pub enabled: bool,
    pub min_attempts: Option<u64>,
    pub attempts: u64,
    pub passed: u64,
    pub failed: u64,
    pub undecided: u64,
    pub min_duration_s: Option<u64>,
    pub duration_s: u64,
    pub verdict: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Campaign {
    pub verdict: &'static str,
    pub reasons: Vec<String>,
    pub release: &'static str,
    pub manifest_sha256: Option<[u8; 32]>,
    pub precommit_sha256: Option<[u8; 32]>,
    pub build_sha256: [u8; 32],
    pub planned: u64,
    pub observed: u64,
    pub selected: u64,
    pub admitted: u64,
    pub skipped: u64,
    pub written: u64,
    pub acquired: u64,
    pub reverted: u64,
    pub dropped: u64,
    pub late: u64,
    pub unresolved: u64,
    pub passed: u64,
    pub failed: u64,
    pub undecided: u64,
    pub consecutive_misses: u64,
    pub before_precommit: u64,
    pub unverifiable: u64,
    /// The skipped launches by how they were seen, `CLASS=count`.
    pub skip_reasons: Vec<String>,
    /// The time the campaign's sealed runs cover (s), and the rest of its
    /// span (no run, or none sealed whole).
    pub covered_s: u64,
    pub uncovered_s: u64,
    /// The covered share of the campaign's span (ppm, from milliseconds).
    pub covered_ppm: u64,
    pub cells: Vec<CellRow>,
}

/// What the campaign is rated with besides the attempts.
pub struct Rating<'a> {
    pub manifest: Option<&'a Result<Manifest, String>>,
    pub precommit: Option<&'a Result<Precommit, String>>,
    /// The evaluating build's and the configuration's hashes.
    pub build_sha256: Option<[u8; 32]>,
    pub config_sha256: [u8; 32],
    /// How long a run's last followed block may precede its end: the
    /// configuration's follower stall limit (`BSC_CANONICAL_STALL_MS`) and
    /// stop limit (`BSC_RUNTIME_STOP_TIMEOUT_MS`).
    pub follow_tolerance_ms: u64,
}

/// What the campaign's runs cover.
struct Coverage {
    /// The chain time the sealed runs followed since the commitment
    /// (review P1: their journaled blocks' header times, never a run's
    /// wall time), and the campaign's span (its first followed block to
    /// the latest a run reached).
    covered_ms: u64,
    span_ms: u64,
    /// Heights between the campaign's first and last block no run
    /// journaled (a restart beyond the follower's catch-up, a gap).
    uncovered_heights: u64,
    /// The campaign's runs.
    runs: HashSet<u64>,
}

/// The campaign's runs as the plan requires them (review H1, H2, N1, N3,
/// P1): every run since the commitment (also one without a readable
/// record) sealed whole, its segments the manifest's, of the plan's build,
/// configuration and host; while it ran, its follower without a gap and
/// not stopped longer than `tolerance_ms` before the run ended; the chain
/// time the sealed runs followed.
fn coverage(
    j: &Journaled,
    from_ms: u64,
    plan: &Plan,
    tolerance_ms: u64,
    not_ready: &mut Vec<String>,
    open: &mut Vec<String>,
) -> Coverage {
    let time = |n: u64| j.blocks.get(&n).map(|(_, ms)| *ms);
    let mut c = Coverage {
        covered_ms: 0,
        span_ms: 0,
        uncovered_heights: 0,
        runs: HashSet::new(),
    };
    let mut covered: Vec<(u64, u64)> = Vec::new();
    let (mut first, mut last): (Option<(u64, u64)>, Option<u64>) = (None, None);
    let mut heights: Option<(u64, u64)> = None;
    let campaign = j
        .runs
        .iter()
        .filter(|r| r.id >= from_ms || r.last_ms.is_none_or(|l| l >= from_ms));
    for run in campaign {
        let id = run.id;
        c.runs.insert(id);
        if let Some(e) = &run.read_error {
            open.push(format!("JOURNAL_UNREADABLE: {id}: {e}"));
        } else if run.records == 0 {
            open.push(format!("RUN_EMPTY: {id}"));
        }
        let end_wall = run
            .manifest
            .as_ref()
            .and_then(|m| m.ended_ms)
            .or(run.last_ms);
        last = last.max(end_wall);
        let span = run
            .blocks
            .and_then(|(a, b)| Some((a, b, time(a)?, time(b)?)));
        // OPEN mode: chain no run followed holds the campaign open;
        // BOUNDED: it is only time the floor bounds.
        match (span, end_wall) {
            _ if plan.uncovered_bounded => {}
            (Some((_, _, _, tb)), Some(end)) if end > tb.saturating_add(tolerance_ms) => {
                open.push(format!(
                    "RUN_FOLLOWER_STOPPED: {id}: {} ms before its end",
                    end - tb
                ));
            }
            (None, _) if run.records > 0 => open.push(format!("RUN_FOLLOWED_NO_BLOCK: {id}")),
            _ => {}
        }
        // Review LOW 1: and its follower began its chain no later than the
        // tolerance after the run (or the commitment) started.
        let start_wall = run
            .manifest
            .as_ref()
            .map_or(id, |m| m.started_ms)
            .max(from_ms);
        if let Some((_, _, ta, _)) = span
            && !plan.uncovered_bounded
            && ta > start_wall.saturating_add(tolerance_ms)
        {
            open.push(format!(
                "RUN_FOLLOWER_LATE: {id}: {} ms after its start",
                ta - start_wall
            ));
        }
        if let Some((a, b, ta, tb)) = span {
            // While it ran, every height it followed is journaled.
            let (missing, gap) = j.gaps(a, b);
            if let Some((x, y)) = gap {
                open.push(format!(
                    "CANONICAL_GAP: run {id}: {missing} blocks, first {x}..={y}"
                ));
            }
            if tb >= from_ms {
                let a_ms = ta.max(from_ms);
                first = Some(first.map_or((a, a_ms), |(h, t)| (h.min(a), t.min(a_ms))));
                last = last.max(Some(tb));
                heights = Some(heights.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
            }
        }
        let m = match (&run.manifest, &run.manifest_error) {
            (_, Some(e)) => {
                open.push(format!("RUN_MANIFEST_UNREADABLE: {id}: {e}"));
                continue;
            }
            (None, None) => {
                open.push(format!("RUN_NOT_SEALED: {id}"));
                continue;
            }
            (Some(m), None) => m,
        };
        match m.build_sha256 {
            Some(b) if b == plan.build_sha256 => {}
            Some(_) => not_ready.push(format!("RUN_OF_ANOTHER_BUILD: {id}")),
            None => not_ready.push(format!("RUN_BUILD_UNKNOWN: {id}")),
        }
        if m.config_sha256 != Some(plan.config_sha256) {
            not_ready.push(format!("RUN_OF_ANOTHER_CONFIG: {id}"));
        }
        if m.host.as_deref() != Some(plan.host.as_str()) {
            not_ready.push(format!("RUN_ON_ANOTHER_HOST: {id}"));
        }
        let whole = m.complete
            && m.lost_records == 0
            && m.ingress_dropped == Some(0)
            && run.manifest_match == Some(true)
            && run.read_error.is_none();
        if !m.complete {
            open.push(format!("RUN_INCOMPLETE: {id}"));
        }
        if m.lost_records > 0 {
            open.push(format!("RUN_LOST_RECORDS: {id}: {}", m.lost_records));
        }
        match m.ingress_dropped {
            Some(0) => {}
            Some(n) => open.push(format!("RUN_INGRESS_DROPPED: {id}: {n}")),
            None => open.push(format!("RUN_INGRESS_DROPS_UNKNOWN: {id}")),
        }
        if run.manifest_match != Some(true) {
            open.push(format!("RUN_SEGMENTS_DIFFER_FROM_MANIFEST: {id}"));
        }
        if let (true, Some((_, _, ta, tb))) = (whole, span)
            && tb >= from_ms
        {
            covered.push((ta.max(from_ms), tb));
        }
    }
    // The chain time the whole runs followed, overlaps once.
    covered.sort_unstable();
    let mut end = 0u64;
    for (a, b) in covered {
        let a = a.max(end);
        if b > a {
            c.covered_ms += b - a;
        }
        end = end.max(b);
    }
    if let (Some((_, start)), Some(last)) = (first, last) {
        c.span_ms = last.saturating_sub(start);
    }
    if let Some((x, y)) = heights {
        c.uncovered_heights = j.gaps(x, y).0;
    }
    c
}

/// The campaign and release verdicts of `attempts` (§13 J).
pub fn campaign(
    attempts: &[Attempt],
    facts: &[(Fact, u64)],
    j: &Journaled,
    rating: &Rating<'_>,
) -> Campaign {
    let mut not_ready: Vec<String> = Vec::new();
    let manifest = match rating.manifest {
        None => {
            not_ready.push("NO_MANIFEST".into());
            None
        }
        Some(Err(e)) => {
            not_ready.push(format!("MANIFEST_INVALID: {e}"));
            None
        }
        Some(Ok(m)) => Some(m),
    };
    let plan = manifest.and_then(|m| {
        if !m.reviewed {
            not_ready.push("MANIFEST_NOT_REVIEWED".into());
        }
        for k in &m.missing {
            not_ready.push(format!("MANIFEST_FIELD_EMPTY: {k}"));
        }
        match &m.plan {
            Some(Ok(p)) => Some(p),
            Some(Err(e)) => {
                not_ready.push(format!("MANIFEST_VALUE: {e}"));
                None
            }
            None => None,
        }
    });
    if let Some(p) = plan {
        match rating.build_sha256 {
            Some(b) if b == p.build_sha256 => {}
            Some(_) => not_ready.push("MANIFEST_BUILD_IS_NOT_THIS_BUILD".into()),
            None => not_ready.push("BUILD_UNKNOWN".into()),
        }
        if p.config_sha256 != rating.config_sha256 {
            not_ready.push("MANIFEST_CONFIG_IS_NOT_THIS_CONFIG".into());
        }
    }
    let precommit = match rating.precommit {
        None => {
            not_ready.push("NO_PRECOMMIT".into());
            None
        }
        Some(Err(e)) => {
            not_ready.push(format!("PRECOMMIT_INVALID: {e}"));
            None
        }
        Some(Ok(c)) => Some(c),
    };
    if let (Some(m), Some(c)) = (manifest, precommit)
        && m.sha256 != c.manifest_sha256
    {
        not_ready.push("MANIFEST_CHANGED_AFTER_PRECOMMIT".into());
    }
    let from_ms = precommit.map(|c| c.committed_unix_ms);
    let c3: Vec<&Attempt> = attempts.iter().filter(|a| a.kind == "C3_BUY").collect();
    // The campaign's attempts: the C3 BUYs written after its commitment.
    let members: Vec<&Attempt> = c3
        .iter()
        .copied()
        .filter(|a| from_ms.is_none_or(|m| a.written_ms >= m))
        .collect();
    let before_precommit = (c3.len() - members.len()) as u64;
    let flap = j.canonical_flap(from_ms);
    let unverifiable = flap.iter().filter(|l| l.unverifiable).count() as u64;
    let observed: Vec<&&Launch> = flap.iter().filter(|l| l.c1_scope).collect();
    let attempted: HashSet<[u8; 32]> = members.iter().filter_map(|a| a.launch_hash).collect();
    let mut run = 0u64;
    let mut consecutive = 0u64;
    let mut by_class = [0u64; SIGHTING_CLASSES.len()];
    for l in &observed {
        run = match attempted.contains(&l.tx_hash) {
            true => 0,
            false => {
                if let Some(n) = by_class.get_mut(usize::from(l.class).wrapping_sub(1)) {
                    *n += 1;
                }
                run + 1
            }
        };
        consecutive = consecutive.max(run);
    }
    let skipped = observed
        .iter()
        .filter(|l| !attempted.contains(&l.tx_hash))
        .count() as u64;
    let withdrawn = facts
        .iter()
        .filter(|(f, at)| {
            f.kind == FactKind::Withdrawn
                && f.intent == LiveIntentKind::Buy
                && f.component == ComponentId::ExecC3
                && f.token != [0; 20]
                && from_ms.is_none_or(|m| *at >= m)
        })
        .count() as u64;
    let count = |p: &dyn Fn(&Attempt) -> bool| members.iter().filter(|a| p(a)).count() as u64;
    let mut c = Campaign {
        release: RELEASE_EVIDENCE_NOT_READY,
        manifest_sha256: manifest.map(|m| m.sha256),
        precommit_sha256: precommit.map(|p| p.sha256),
        build_sha256: rating.build_sha256.unwrap_or_default(),
        planned: plan.map_or(0, |p| p.cells.iter().map(|x| x.min_attempts).sum()),
        observed: observed.len() as u64,
        selected: observed.iter().filter(|l| l.class == 1).count() as u64,
        written: members.len() as u64,
        admitted: members.len() as u64 + withdrawn,
        skipped,
        acquired: count(&|a| a.verdicts.acquisition == PASS),
        reverted: count(&|a| a.state == "REVERTED"),
        dropped: count(&|a| a.state == "DROPPED"),
        late: count(&|a| {
            a.placement.is_some_and(|p| {
                matches!(
                    p.verdict,
                    PlacementVerdict::Late | PlacementVerdict::OtherBlock
                )
            })
        }),
        unresolved: count(&Attempt::unresolved),
        passed: count(&|a| a.verdicts.verdict == PASS),
        failed: count(&|a| a.verdicts.verdict == FAIL),
        undecided: count(&|a| a.verdicts.verdict == UNKNOWN),
        consecutive_misses: consecutive,
        before_precommit,
        unverifiable,
        skip_reasons: SIGHTING_CLASSES
            .iter()
            .zip(by_class)
            .filter(|(_, n)| *n > 0)
            .map(|(class, n)| format!("{class}={n}"))
            .collect(),
        ..Campaign::default()
    };
    let mut open = Vec::new();
    // Review L4, N2: attempts before the commitment are not the
    // campaign's (an earlier campaign's, retained); they hold it open only
    // while unresolved (they may still execute); one this run directory
    // journaled means its commitment came after its attempts: a new
    // campaign starts in a run directory of its own.
    let earlier: Vec<&&Attempt> = c3
        .iter()
        .filter(|a| from_ms.is_some_and(|m| a.written_ms < m))
        .collect();
    let unresolved_before = earlier.iter().filter(|a| a.unresolved()).count();
    if unresolved_before > 0 {
        open.push(format!("EARLIER_ATTEMPTS_UNRESOLVED: {unresolved_before}"));
    }
    let mut campaign_runs = HashSet::new();
    if let (Some(p), Some(from)) = (plan, from_ms) {
        let cov = coverage(
            j,
            from,
            p,
            rating.follow_tolerance_ms,
            &mut not_ready,
            &mut open,
        );
        if !p.uncovered_bounded && cov.uncovered_heights > 0 {
            open.push(format!("UNCOVERED_HEIGHTS: {}", cov.uncovered_heights));
        }
        c.covered_s = cov.covered_ms / 1_000;
        c.uncovered_s = cov.span_ms.saturating_sub(cov.covered_ms) / 1_000;
        c.covered_ppm = match cov.span_ms {
            0 => 0,
            span => cov.covered_ms.min(span).saturating_mul(1_000_000) / span,
        };
        let mine = earlier.iter().filter(|a| a.run.is_some()).count();
        if mine > 0 {
            not_ready.push(format!("EARLIER_ATTEMPTS_IN_THIS_RUN_DIRECTORY: {mine}"));
        }
        campaign_runs = cov.runs;
        let eligible: HashSet<[u8; 32]> = observed.iter().map(|l| l.tx_hash).collect();
        let frozen: std::collections::BTreeSet<&str> =
            p.adapters.iter().map(|b| b.name()).collect();
        // Review H2, L4, L5: each attempt in a campaign run, on an eligible
        // launch, through exactly the plan's builders from its sources.
        for a in &members {
            let tx = alloy_primitives::hex::encode(a.tx_hash);
            if a.run.is_none() {
                open.push(format!("ATTEMPT_WITHOUT_RELEASE_RECORD: 0x{tx}"));
                continue;
            }
            if a.launch_hash.is_none_or(|h| !eligible.contains(&h)) {
                not_ready.push(format!("ATTEMPT_ON_AN_INELIGIBLE_LAUNCH: 0x{tx}"));
            }
            let builders: Option<std::collections::BTreeSet<&str>> = a
                .adapters
                .iter()
                .map(|r| r.builder.map(LiveAdapter::name))
                .collect();
            let outside = a.trigger.is_none_or(|s| !p.sources.contains(&s))
                || builders.as_ref() != Some(&frozen);
            if outside {
                not_ready.push(format!("ATTEMPT_OUTSIDE_FROZEN_SCOPE: 0x{tx}"));
            }
        }
    }
    // Review L3: of the campaign's runs only.
    for (run, why) in j
        .releases_refused
        .iter()
        .filter(|(r, _)| campaign_runs.contains(r))
    {
        open.push(format!("RELEASE_RECORD_REFUSED: {run}: {why}"));
    }
    if unverifiable > 0 {
        open.push(format!("LAUNCHES_UNVERIFIABLE: {unverifiable}"));
    }
    for cell in CELLS {
        let planned = plan.and_then(|p| p.cells.iter().find(|x| x.cell == cell));
        let of = |v: &str| {
            members
                .iter()
                .filter(|a| a.cell == cell && a.verdicts.verdict == v)
                .count() as u64
        };
        let attempts = members.iter().filter(|a| a.cell == cell).count() as u64;
        let (passed, failed) = (of(PASS), of(FAIL));
        let verdict = match planned {
            None => NOT_APPLICABLE,
            Some(_) if failed > 0 => FAIL,
            Some(p)
                if attempts >= p.min_attempts
                    && c.covered_s >= p.min_duration_s
                    && passed == attempts =>
            {
                PASS
            }
            Some(_) => CAMPAIGN_OPEN,
        };
        c.cells.push(CellRow {
            cell,
            enabled: planned.is_some(),
            min_attempts: planned.map(|p| p.min_attempts),
            attempts,
            passed,
            failed,
            undecided: of(UNKNOWN),
            min_duration_s: planned.map(|p| p.min_duration_s),
            duration_s: c.covered_s,
            verdict,
        });
    }
    let (Some(p), true) = (plan, not_ready.is_empty()) else {
        c.verdict = CAMPAIGN_NOT_READY;
        c.reasons = [not_ready, open].concat();
        if c.reasons.is_empty() {
            c.reasons.push("NO_PLAN".into());
        }
        return c;
    };
    let mut fail = Vec::new();
    if c.failed > 0 {
        fail.push(format!("ATTEMPTS_FAILED: {}", c.failed));
    }
    if members
        .iter()
        .any(|a| a.chain_id.is_some_and(|id| id != p.chain_id))
    {
        fail.push("ATTEMPT_ON_ANOTHER_CHAIN".into());
    }
    if members
        .iter()
        .any(|a| a.input_wei.is_some_and(|v| v > p.amount_limit_wei))
    {
        fail.push("AMOUNT_ABOVE_LIMIT".into());
    }
    let cost = members.iter().fold(U256::ZERO, |sum, a| {
        sum.saturating_add(a.input_wei.unwrap_or_default())
            .saturating_add(a.gas_wei.unwrap_or_default())
    });
    if cost > p.cost_limit_wei {
        fail.push("COST_ABOVE_LIMIT".into());
    }
    if consecutive > p.max_consecutive_misses {
        fail.push(format!("CONSECUTIVE_MISSES_ABOVE_LIMIT: {consecutive}"));
    }
    let outside = members
        .iter()
        .filter(|a| !p.cells.iter().any(|x| x.cell == a.cell))
        .count();
    if outside > 0 {
        open.push(format!("ATTEMPTS_OUTSIDE_ENABLED_CELLS: {outside}"));
    }
    if c.written == 0 {
        open.push("NO_ADMITTED_ATTEMPT".into());
    }
    if c.unresolved > 0 {
        open.push(format!("UNRESOLVED: {}", c.unresolved));
    }
    if c.undecided > 0 {
        open.push(format!("UNDECIDED: {}", c.undecided));
    }
    let cells_met = c
        .cells
        .iter()
        .filter(|r| r.enabled)
        .all(|r| r.verdict == PASS);
    if !cells_met {
        open.push("CELL_MINIMUMS_NOT_MET".into());
    }
    // Review N3: launches while no run was up are no skips: the time
    // without a sealed run is bounded by the plan's floor.
    let covered_ppm = c.covered_ppm;
    if covered_ppm < p.min_covered_ppm {
        let why = format!("COVERAGE_BELOW_FLOOR: {covered_ppm} ppm");
        match cells_met {
            true => fail.push(why),
            false => open.push(why),
        }
    }
    match c.observed {
        0 => open.push("NO_OBSERVED_LAUNCH".into()),
        n => {
            let skip_ppm = c.skipped.saturating_mul(1_000_000) / n;
            if skip_ppm > p.max_skip_ppm {
                let why = format!("SKIP_RATE_ABOVE_LIMIT: {skip_ppm} ppm of {n}");
                match cells_met {
                    true => fail.push(why),
                    false => open.push(why),
                }
            }
        }
    }
    (c.verdict, c.reasons) = match (fail.is_empty(), open.is_empty()) {
        (false, _) => (CAMPAIGN_FAIL, [fail, open].concat()),
        (true, false) => (CAMPAIGN_OPEN, open),
        (true, true) => (CAMPAIGN_PASS, Vec::new()),
    };
    // The campaign's release evidence: a passed campaign of the cell the
    // release claims (a self-launch campaign never releases third-party
    // launches).
    if c.verdict == CAMPAIGN_PASS
        && c.cells
            .iter()
            .any(|r| r.cell == p.release_scope && r.enabled && r.verdict == PASS)
    {
        c.release = RELEASE_EVIDENCE_PASS;
    }
    c
}

impl Campaign {
    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&lv::SCHEMA);
        d.set(lv::VERDICT, self.verdict)
            .set(lv::PROVENANCE_TOOL, TOOL)
            .set(lv::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(lv::RELEASE, self.release)
            .set(lv::BUILD_SHA256, Value::B256(self.build_sha256))
            .set(lv::OBSERVED, self.observed)
            .set(lv::ADMITTED, self.admitted)
            .set(lv::SKIPPED, self.skipped)
            .set(lv::WRITTEN, self.written)
            .set(lv::ACQUIRED, self.acquired)
            .set(lv::REVERTED, self.reverted)
            .set(lv::DROPPED, self.dropped)
            .set(lv::LATE, self.late)
            .set(lv::UNRESOLVED, self.unresolved)
            .set(lv::PASSED, self.passed)
            .set(lv::FAILED, self.failed)
            .set(lv::UNDECIDED, self.undecided)
            .set(lv::CONSECUTIVE_MISSES, self.consecutive_misses)
            .set(lv::PLANNED, self.planned)
            .set(lv::SELECTED, self.selected)
            .set(lv::BEFORE_PRECOMMIT, self.before_precommit)
            .set(lv::UNVERIFIABLE, self.unverifiable)
            .set(lv::COVERED_S, self.covered_s)
            .set(lv::UNCOVERED_S, self.uncovered_s)
            .set(lv::COVERED_PPM, self.covered_ppm)
            .set_opt(lv::MANIFEST_SHA256, self.manifest_sha256.map(Value::B256))
            .set_opt(lv::PRECOMMIT_SHA256, self.precommit_sha256.map(Value::B256));
        for r in &self.reasons {
            d.push(lv::REASONS, Value::Text(r.clone()));
        }
        for r in &self.skip_reasons {
            d.push(lv::SKIP_REASONS, Value::Text(r.clone()));
        }
        for r in &self.cells {
            let mut n = Draft::new(&lv::cell::SCHEMA);
            n.set(lv::cell::CELL, r.cell)
                .set(lv::cell::ENABLED, r.enabled)
                .set(lv::cell::ATTEMPTS, r.attempts)
                .set(lv::cell::PASSED, r.passed)
                .set(lv::cell::FAILED, r.failed)
                .set(lv::cell::UNDECIDED, r.undecided)
                .set(lv::cell::DURATION_S, r.duration_s)
                .set(lv::cell::VERDICT, r.verdict)
                .set_opt(lv::cell::MIN_ATTEMPTS, r.min_attempts)
                .set_opt(lv::cell::MIN_DURATION_S, r.min_duration_s);
            d.push(lv::CELLS, n.into_nested());
        }
        d.encode()
            .map_err(|e| format!("live-campaign-verdict: {e:?}"))
    }

    pub fn summary(&self) -> String {
        let mut out = format!(
            "campaign {} {} planned={} observed={} selected={} admitted={} written={} acquired={}/{} reverted={} dropped={} late={} unresolved={} skipped={}/{} consecutive_misses={} passed={}/{} failed={} undecided={} before_precommit={} unverifiable={} covered_s={} uncovered_s={}\n",
            self.verdict,
            self.release,
            self.planned,
            self.observed,
            self.selected,
            self.admitted,
            self.written,
            self.acquired,
            self.written,
            self.reverted,
            self.dropped,
            self.late,
            self.unresolved,
            self.skipped,
            self.observed,
            self.consecutive_misses,
            self.passed,
            self.written,
            self.failed,
            self.undecided,
            self.before_precommit,
            self.unverifiable,
            self.covered_s,
            self.uncovered_s,
        );
        for r in &self.cells {
            out.push_str(&format!(
                "cell {} enabled={} attempts={}/{} passed={} failed={} duration_s={}/{} verdict={}\n",
                r.cell,
                r.enabled,
                r.attempts,
                r.min_attempts.map_or("-".into(), |v| v.to_string()),
                r.passed,
                r.failed,
                r.duration_s,
                r.min_duration_s.map_or("-".into(), |v| v.to_string()),
                r.verdict,
            ));
        }
        for r in &self.skip_reasons {
            out.push_str(&format!("skip {r}\n"));
        }
        for r in &self.reasons {
            out.push_str(&format!("reason {r}\n"));
        }
        out
    }
}

/// The evaluation of the run directory `rundir` and the owner state
/// `state`: every attempt's record and the campaign's verdict, written.
pub struct Evaluation {
    pub attempts: Vec<Attempt>,
    pub campaign: Campaign,
}

/// Reads every producer timing verdict `<attempt id>.dbrec` of `dir` with
/// the G0.8 result `<attempt id>.g08.dbrec` it names.
pub fn witnesses(dir: Option<&Path>) -> Result<Witnesses, String> {
    let mut out = HashMap::new();
    let Some(dir) = dir else { return Ok(out) };
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = e.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|n| n.strip_suffix(".dbrec"))
            .filter(|n| !n.ends_with(".g08"))
        else {
            continue;
        };
        let g08 = std::fs::read(dir.join(format!("{id}.g08.dbrec"))).ok();
        let w = std::fs::read(e.path())
            .map_err(|x| x.to_string())
            .and_then(|b| Witness::decode(&b, g08.as_deref()));
        out.insert(id.to_string(), w);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    rundir: &Path,
    state: &Path,
    max_payload: usize,
    manifest: Option<&Result<Manifest, String>>,
    witness_dir: Option<&Path>,
    own_accounts: &[[u8; 20]],
    follow_tolerance_ms: u64,
    build_sha256: Option<[u8; 32]>,
    config_sha256: [u8; 32],
    now_unix_ms: u64,
) -> Result<Evaluation, String> {
    let j = Journaled::read(&rundir.join("bsc-runtime/journal"), max_payload)?;
    let facts = crate::live_ledger::Ledger::open(state).stamped()?;
    let precommit = Precommit::read(rundir).transpose();
    let witnesses = witnesses(witness_dir)?;
    let plan = manifest.and_then(|m| m.as_ref().ok()?.plan.as_ref()?.as_ref().ok());
    let attempts = attempts(
        &facts,
        &j,
        &Inputs {
            window_ms: MARKET_WINDOW_MS,
            plan,
            witnesses: &witnesses,
            own_accounts,
        },
    );
    let campaign = campaign(
        &attempts,
        &facts,
        &j,
        &Rating {
            manifest,
            precommit: precommit.as_ref(),
            build_sha256,
            config_sha256,
            follow_tolerance_ms,
        },
    );
    let dir = rundir.join(ATTEMPTS_DIR);
    std::os::unix::fs::DirBuilderExt::mode(std::fs::DirBuilder::new().recursive(true), 0o700)
        .create(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    for a in &attempts {
        crate::replay_proof::write(
            &dir.join(attempt_file_name(&a.tx_hash)),
            &a.encode(now_unix_ms)?,
        )?;
    }
    crate::replay_proof::write(&rundir.join(VERDICT_PATH), &campaign.encode(now_unix_ms)?)?;
    Ok(Evaluation { attempts, campaign })
}

#[cfg(test)]
#[path = "live_attempts_tests.rs"]
mod tests;
