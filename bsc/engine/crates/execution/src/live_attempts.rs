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
mod tests {
    //! Synthetic cases: they test the verdict rules only (master plan §13
    //! J); none of them is an acquisition sample.
    use super::*;
    use crate::canonical_join::{FourToken, Joined, LaunchRef, LaunchTx, Venue, Visibility};
    use crate::live_ledger::Placement;
    use crate::runtime_journal::ReleasedAdapter;
    use crate::sources::{NonceOrigin, SourceId};

    const BUYER: [u8; 20] = [0xB0; 20];
    /// The operator's own launcher wallet (another key than the buyer).
    const OWN_LAUNCHER: [u8; 20] = [0xD1; 20];
    const RUN: u64 = T0_MS - 3_600_000;
    const CREATOR: [u8; 20] = [0xC1; 20];
    const TOKEN: [u8; 20] = [0xA1; 20];
    const LAUNCH_BLOCK: u64 = 100;
    const T0_MS: u64 = 1_793_000_000_000;
    const WINDOW_MS: u64 = MARKET_WINDOW_MS;

    fn hash(n: u64) -> [u8; 32] {
        let mut h = [0x5a; 32];
        h[..8].copy_from_slice(&n.to_le_bytes());
        h
    }

    fn launch_tx(i: u8) -> [u8; 32] {
        [0x10 + i; 32]
    }

    fn buy_tx(i: u8) -> [u8; 32] {
        [0x60 + i; 32]
    }

    fn token(i: u8) -> [u8; 20] {
        let mut t = TOKEN;
        t[0] = 0xA0 + i;
        t
    }

    fn fact(kind: FactKind, tx: [u8; 32], tok: [u8; 20]) -> Fact {
        Fact {
            kind,
            tx_hash: tx,
            intent: LiveIntentKind::Buy,
            component: ComponentId::ExecC3,
            token: tok,
            nonce: u64::from(tx[0]),
            grant_generation: 1,
            input_wei: Some(U256::from(1_000_000u64)),
            max_wei: Some(U256::from(2_000_000u64)),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(LAUNCH_BLOCK + 2),
            by_tx: None,
            response: None,
            origin: Some(NonceOrigin {
                chain_id: 56,
                sender: BUYER,
            }),
            launcher: None,
            placement: None,
            attempt: None,
            operator: None,
            source: None,
        }
    }

    /// An attempt's facts: released, and (when `block` is given) included
    /// there with tokens and placed as `verdict`.
    fn facts_of(
        i: u8,
        block: Option<u64>,
        tokens: u64,
        verdict: Option<PlacementVerdict>,
        written_ms: u64,
    ) -> Vec<(Fact, u64)> {
        let tx = buy_tx(i);
        let released = fact(FactKind::Released, tx, token(i));
        let mut out = vec![(released.clone(), written_ms)];
        if let Some(b) = block {
            let at = Some((b, hash(b), T0_MS + (b - LAUNCH_BLOCK) * 450));
            out.push((
                Fact {
                    block: at,
                    tokens: Some(U256::from(tokens)),
                    eth_wei: Some(U256::from(1_000_000u64)),
                    gas_wei: Some(U256::from(30_000u64)),
                    ..Fact::about(&released, FactKind::Included)
                },
                written_ms + 1,
            ));
            if let Some(v) = verdict {
                out.push((
                    Fact {
                        block: at,
                        launcher: Some(CREATOR),
                        placement: Some(Placement {
                            verdict: v,
                            launch_index: Some(0),
                            own_index: 1,
                            buys_before: 0,
                        }),
                        ..Fact::about(&released, FactKind::Placed)
                    },
                    written_ms + 2,
                ));
            }
        }
        out
    }

    fn adapter(boundary_ns: u64) -> ReleasedAdapter {
        ReleasedAdapter {
            adapter: 0,
            builder: Some(0),
            outcome: 0,
            boundary: 0,
            record_qualifies: true,
            transport_qualified: true,
            prepermit_ns: Some(boundary_ns - 30_000),
            write_check_ns: Some(boundary_ns - 20_000),
            boundary_ns: Some(boundary_ns),
            queue_ns: None,
        }
    }

    fn release(i: u8, misses: u8) -> LiveRelease {
        LiveRelease {
            kind: 0,
            trigger: Some(SourceId::PublicNode),
            tx_hash: buy_tx(i),
            attempt: [0xE0 + i; 32],
            launch_hash: launch_tx(i),
            token: token(i),
            block: 0,
            clock_id: 1,
            times: [1_000, 2_000, 3_000, 4_000, 5_000, 6_000, 7_000, 300_000],
            deadline_misses: misses,
            adapters: vec![adapter(300_000)],
            raw: vec![0xf8],
        }
    }

    fn launch_ref(i: u8, block: u64) -> LaunchRef {
        LaunchRef {
            venue: Venue::Flap,
            block_number: block,
            block_hash: hash(block),
            tx_index: 0,
            tx_hash: launch_tx(i),
            token: Some(token(i)),
            external_buys: None,
            c1_scope: true,
            venue_target: true,
            four_token: FourToken::NotApplicable,
            tx: LaunchTx::Verified,
            block_ms: T0_MS + (block - LAUNCH_BLOCK) * 450,
            block_seen_ns: 0,
            block_seen_unix_ms: None,
        }
    }

    fn trade(
        block: u64,
        tx_index: u64,
        tx: [u8; 32],
        tok: [u8; 20],
        sell: bool,
        amount: u64,
        eth: u64,
    ) -> Vec<u8> {
        let word = |v: u64| U256::from(v).to_be_bytes::<32>();
        rj::flap_trade_payload(&crate::chain_follower::CanonicalTrade {
            block_number: block,
            block_hash: hash(block),
            tx_index,
            tx_hash: tx,
            log_index: tx_index,
            origin: [0x77; 20],
            sell,
            token: tok,
            trader: [0x77; 20],
            amount: word(amount),
            eth: word(eth),
            fee: word(0),
            post_price: word(0),
        })
    }

    /// Blocks `LAUNCH_BLOCK..LAUNCH_BLOCK + n` 450 ms apart, the launches
    /// of `attempts` in the first, their own BUYs at index 1.
    fn journal(n: u64, attempts: &[u8]) -> Journaled {
        let mut j = Journaled::default();
        for b in LAUNCH_BLOCK..LAUNCH_BLOCK + n {
            let ms = T0_MS + (b - LAUNCH_BLOCK) * 450;
            j.record(
                RecordKind::Canonical,
                &crate::runtime_strategy::block_payload(b, &hash(b), ms),
                ms,
            );
        }
        for &i in attempts {
            j.record(
                RecordKind::Canonical,
                &rj::canonical_launch_payload(&Joined {
                    launch: launch_ref(i, LAUNCH_BLOCK),
                    visibility: Visibility::Missed,
                }),
                T0_MS,
            );
            j.record(
                RecordKind::Canonical,
                &crate::runtime_strategy::flap_launch_payload(LAUNCH_BLOCK, 0, &token(i), &CREATOR),
                T0_MS,
            );
            j.record(
                RecordKind::Canonical,
                &trade(
                    LAUNCH_BLOCK,
                    1,
                    buy_tx(i),
                    token(i),
                    false,
                    1_000,
                    1_000_000,
                ),
                T0_MS,
            );
            j.releases.insert(
                buy_tx(i),
                Release {
                    run: RUN,
                    l: release(i, 0),
                },
            );
        }
        j.last_ms = Some(T0_MS + n * 450);
        // One run, sealed whole, of the plan's build, configuration and
        // host, from before the commitment to the journal's end.
        j.runs.push(Run {
            id: RUN,
            manifest: Some(sealed()),
            manifest_match: Some(true),
            records: n,
            last_ms: j.last_ms,
            blocks: Some((LAUNCH_BLOCK, LAUNCH_BLOCK + n - 1)),
            ..Run::default()
        });
        j
    }

    fn sealed() -> RunManifest {
        RunManifest {
            complete: true,
            started_ms: RUN,
            ended_ms: Some(T0_MS + 40 * 450),
            lost_records: 0,
            ingress_dropped: Some(0),
            build_sha256: Some([1; 32]),
            config_sha256: Some([2; 32]),
            host: Some("test-host".into()),
        }
    }

    /// A producer timing verdict for attempt `i` with its own G0.8 result
    /// (the BUY in the launch's block).
    fn witness(i: u8, verdict: &str, bound: u64) -> (String, Result<Witness, String>) {
        let id = attempt_id(&[0xE0 + i; 32]);
        (
            id.clone(),
            Ok(Witness {
                attempt_id: id.clone(),
                verdict: verdict.into(),
                strict_limit_ns: STRICT_LIMIT_NS,
                upper_bound_ns: bound,
                remaining_uncertainty_ns: 100,
                g08_sha256: [5; 32],
                g08: Some(Ok(G08Binding {
                    sha256: [5; 32],
                    verdict: "PASS".into(),
                    attempt_id: id,
                    buy_tx: buy_tx(i),
                    launch_tx: launch_tx(i),
                    buy_block_hash: hash(LAUNCH_BLOCK),
                })),
            }),
        )
    }

    fn plan(min_attempts: u64) -> Plan {
        Plan {
            build_sha256: [1; 32],
            config_sha256: [2; 32],
            chain_id: 56,
            host: "test-host".into(),
            adapters: vec![LiveAdapter::Club48BuilderV1],
            sources: vec![SourceId::PublicNode],
            self_launchers: vec![OWN_LAUNCHER],
            cells: vec![CellPlan {
                cell: EXTERNAL_FULL_LAUNCH,
                min_attempts,
                min_duration_s: 1,
            }],
            max_skip_ppm: 1_000_000,
            max_consecutive_misses: 10,
            min_covered_ppm: 900_000,
            uncovered_bounded: false,
            clock_error_bound_ns: 1_000,
            finality_blocks: 3,
            reorg_horizon_blocks: 2,
            amount_limit_wei: U256::from(10_000_000u64),
            cost_limit_wei: U256::from(100_000_000u64),
            release_scope: EXTERNAL_FULL_LAUNCH,
        }
    }

    fn evaluate_one(
        facts: &[(Fact, u64)],
        j: &Journaled,
        plan: Option<&Plan>,
        w: &Witnesses,
    ) -> Vec<Attempt> {
        attempts(
            facts,
            j,
            &Inputs {
                window_ms: WINDOW_MS,
                plan,
                witnesses: w,
                own_accounts: &[],
            },
        )
    }

    /// §13 J: 999,999 ns after the uncertainty is accepted, 1,000,000 ns
    /// rejected; a PASS verdict whose bound is not below the limit is no
    /// witness.
    #[test]
    fn the_strict_bound_passes_999_999_and_refuses_1_000_000() {
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let j = journal(40, &[1]);
        let p = plan(1);
        let timing = |verdict: &str, bound| {
            let w = HashMap::from([witness(1, verdict, bound)]);
            let a = evaluate_one(&facts, &j, Some(&p), &w).remove(0);
            (a.verdicts.timing, a.verdicts.verdict)
        };
        assert_eq!(timing("PASS", 999_999), (PASS, PASS));
        assert_eq!(timing("FAIL", 1_000_000), (FAIL, FAIL));
        assert_eq!(timing("PASS", 1_000_000), (UNKNOWN, UNKNOWN));
        assert_eq!(timing("FAIL", 999_999), (UNKNOWN, UNKNOWN));
    }

    /// A missing witness, one of another attempt, of another limit or with
    /// more uncertainty than the manifest allows: UNKNOWN, never PASS.
    #[test]
    fn a_missing_or_foreign_witness_leaves_timing_unknown() {
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let j = journal(40, &[1]);
        let p = plan(1);
        let run = |w: Witnesses| {
            let a = evaluate_one(&facts, &j, Some(&p), &w).remove(0);
            (a.verdicts.timing, a.reasons)
        };
        let (t, why) = run(HashMap::new());
        assert_eq!(t, UNKNOWN);
        assert!(why.contains(&"NO_PRODUCER_WITNESS".to_string()));
        let (id, w) = witness(1, "PASS", 1);
        let foreign = Witness {
            attempt_id: attempt_id(&[9; 32]),
            ..w.clone().unwrap()
        };
        assert_eq!(run(HashMap::from([(id.clone(), Ok(foreign))])).0, UNKNOWN);
        let limit = Witness {
            strict_limit_ns: 2_000_000,
            ..w.clone().unwrap()
        };
        assert_eq!(run(HashMap::from([(id.clone(), Ok(limit))])).0, UNKNOWN);
        let loose = Witness {
            remaining_uncertainty_ns: 1_001,
            ..w.clone().unwrap()
        };
        assert_eq!(run(HashMap::from([(id.clone(), Ok(loose))])).0, UNKNOWN);
        assert_eq!(run(HashMap::from([(id, Err("torn".into()))])).0, UNKNOWN);
    }

    /// A send without acquisition never passes: included without tokens
    /// FAIL, unresolved UNKNOWN, its window over without inclusion FAIL.
    #[test]
    fn a_send_without_acquisition_never_passes() {
        let j = journal(40, &[1]);
        let w = HashMap::from([witness(1, "PASS", 1)]);
        let p = plan(1);
        let no_tokens = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            0,
            Some(PlacementVerdict::NoTokens),
            T0_MS,
        );
        let a = evaluate_one(&no_tokens, &j, Some(&p), &w).remove(0);
        assert_eq!((a.verdicts.acquisition, a.verdicts.verdict), (FAIL, FAIL));
        let pending = facts_of(1, None, 0, None, T0_MS);
        let mut short = journal(2, &[1]);
        short.blocks.retain(|n, _| *n <= LAUNCH_BLOCK + 1);
        let a = evaluate_one(&pending, &short, Some(&p), &w).remove(0);
        assert_eq!(
            (a.state, a.verdicts.acquisition, a.verdicts.verdict),
            ("RELEASED", UNKNOWN, UNKNOWN)
        );
        // Its window (to LAUNCH_BLOCK + 2) is over at the journal's head:
        // still unresolved, its bytes may execute later (review L1).
        let a = evaluate_one(&pending, &j, Some(&p), &w).remove(0);
        assert_eq!(
            (a.state, a.verdicts.acquisition, a.verdicts.verdict),
            ("DROPPED", UNKNOWN, UNKNOWN)
        );
        assert!(a.unresolved() && a.reasons.contains(&"WINDOW_OVER_UNRESOLVED".to_string()));
    }

    /// A same-block acquisition without its producer timing: UNKNOWN.
    #[test]
    fn a_same_block_acquisition_without_timing_is_unknown() {
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let a = evaluate_one(&facts, &journal(40, &[1]), Some(&plan(1)), &HashMap::new()).remove(0);
        let v = a.verdicts;
        assert_eq!(
            [
                v.acquisition,
                v.canonical,
                v.placement,
                v.capture,
                v.local,
                v.economics
            ],
            [PASS; 6]
        );
        assert_eq!((v.timing, v.verdict), (UNKNOWN, UNKNOWN));
    }

    /// A fast acquisition behind another trader's BUY fails; so does one
    /// acknowledged and then included only after its launch's block.
    #[test]
    fn a_fast_acquisition_in_the_wrong_place_fails() {
        let j = journal(40, &[1]);
        // Its G0.8 result names the block the BUY landed in.
        let (id, mut wit) = witness(1, "PASS", 1);
        if let Ok(Witness {
            g08: Some(Ok(g)), ..
        }) = &mut wit
        {
            g.buy_block_hash = hash(LAUNCH_BLOCK + 1);
        }
        let w = HashMap::from([(id, wit)]);
        for v in [PlacementVerdict::Late, PlacementVerdict::OtherBlock] {
            let facts = facts_of(1, Some(LAUNCH_BLOCK + 1), 1_000, Some(v), T0_MS);
            let mut j = j.clone();
            j.answers.insert((buy_tx(1), 0), (0, Some(310_000)));
            let a = evaluate_one(&facts, &j, Some(&plan(1)), &w).remove(0);
            assert_eq!(
                (a.verdicts.timing, a.verdicts.placement, a.verdicts.verdict),
                (PASS, FAIL, FAIL)
            );
            assert_eq!(a.local.unwrap().ack_ns, Some(310_000));
            assert_eq!(a.adapters[0].answer, Some("ACK"));
        }
    }

    /// A violated local start deadline fails the attempt; no qualified
    /// send time leaves it UNKNOWN.
    #[test]
    fn the_local_deadline_and_send_time_are_judged() {
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let w = HashMap::from([witness(1, "PASS", 1)]);
        let mut j = journal(40, &[1]);
        j.releases.insert(
            buy_tx(1),
            Release {
                run: RUN,
                l: release(1, 1),
            },
        );
        assert_eq!(
            evaluate_one(&facts, &j, Some(&plan(1)), &w)[0]
                .verdicts
                .local,
            FAIL
        );
        let mut r = release(1, 0);
        r.adapters[0].transport_qualified = false;
        j.releases.insert(buy_tx(1), Release { run: RUN, l: r });
        let a = evaluate_one(&facts, &j, Some(&plan(1)), &w).remove(0);
        assert_eq!(
            (a.verdicts.local, a.local.unwrap().send_ns),
            (UNKNOWN, None)
        );
        j.releases.clear();
        assert_eq!(
            evaluate_one(&facts, &j, Some(&plan(1)), &w)[0]
                .verdicts
                .local,
            UNKNOWN
        );
    }

    /// The market window: every trade of the token from the launch's
    /// block to the last block within the window, the first other
    /// trader's BUY (never the launch's own) and the price against it.
    #[test]
    fn the_window_counts_trades_and_prices_against_the_first_external_buy() {
        let mut j = journal(40, &[1]);
        // The launch's own dev BUY, an external BUY at twice our price, a
        // SELL, a trade after the window and one of another token.
        for p in [
            trade(
                LAUNCH_BLOCK,
                0,
                launch_tx(1),
                token(1),
                false,
                5_000,
                1_000_000,
            ),
            trade(
                LAUNCH_BLOCK + 1,
                0,
                [0x99; 32],
                token(1),
                false,
                500,
                1_000_000,
            ),
            trade(
                LAUNCH_BLOCK + 2,
                0,
                [0x98; 32],
                token(1),
                true,
                100,
                150_000,
            ),
            trade(LAUNCH_BLOCK + 35, 0, [0x97; 32], token(1), false, 1, 1),
            trade(LAUNCH_BLOCK + 1, 1, [0x96; 32], token(2), false, 1, 1),
        ] {
            j.record(RecordKind::Canonical, &p, T0_MS);
        }
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let w = evaluate_one(&facts, &j, Some(&plan(1)), &HashMap::new())
            .remove(0)
            .window;
        assert_eq!(w.why_incomplete, None);
        // 15 s at 450 ms: blocks 100..=133.
        assert_eq!(
            (w.launch_block, w.end_block),
            (Some(LAUNCH_BLOCK), Some(LAUNCH_BLOCK + 33))
        );
        assert_eq!((w.trades, w.buys, w.sells), (4, 3, 1));
        assert_eq!(w.buy_wei, U256::from(3_000_000u64));
        assert_eq!(w.sell_wei, U256::from(150_000u64));
        let (tx, block, index, price) = w.first_external.unwrap();
        assert_eq!((tx, block, index), ([0x99; 32], LAUNCH_BLOCK + 1, 0));
        assert_eq!(price, U256::from(2_000u64) * U256::from(E18));
        assert_eq!(w.own_price, Some(U256::from(1_000u64) * U256::from(E18)));
        assert_eq!(w.price_vs_first_external_ppm, Some(500_000));
    }

    /// A window is incomplete while a block in it is missing, a Portal log
    /// in it was refused, it is not over, or its end is within the reorg
    /// horizon; a reorg's replacement block replaces the orphan's trades.
    #[test]
    fn a_window_with_a_gap_or_an_unresolved_reorg_is_incomplete() {
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let why = |j: &Journaled| {
            evaluate_one(&facts, j, Some(&plan(1)), &HashMap::new())[0]
                .window
                .why_incomplete
        };
        let mut gap = journal(40, &[1]);
        gap.blocks.remove(&(LAUNCH_BLOCK + 5));
        assert_eq!(why(&gap), Some("BLOCK_NOT_JOURNALED"));
        let mut refused = journal(40, &[1]);
        refused
            .refused
            .insert((LAUNCH_BLOCK + 3, hash(LAUNCH_BLOCK + 3)));
        assert_eq!(why(&refused), Some("TRADE_LOG_REFUSED"));
        assert_eq!(why(&journal(20, &[1])), Some("WINDOW_NOT_OVER"));
        // Block 133 ends it, 134 is past it: the horizon (2) needs 135.
        assert_eq!(why(&journal(35, &[1])), Some("REORG_HORIZON_NOT_REACHED"));
        assert_eq!(why(&journal(36, &[1])), None);
        // A trade of an orphaned block 101 is not counted once its
        // replacement (another hash) is journaled.
        let mut reorg = journal(40, &[1]);
        reorg.record(
            RecordKind::Canonical,
            &trade(
                LAUNCH_BLOCK + 1,
                0,
                [0x99; 32],
                token(1),
                false,
                500,
                1_000_000,
            ),
            T0_MS,
        );
        let mut other = hash(LAUNCH_BLOCK + 1);
        other[31] ^= 1;
        reorg.blocks.insert(LAUNCH_BLOCK + 1, (other, T0_MS + 450));
        let w = evaluate_one(&facts, &reorg, Some(&plan(1)), &HashMap::new())
            .remove(0)
            .window;
        assert_eq!((w.trades, w.first_external), (1, None));
        let a = evaluate_one(&facts, &journal(10, &[1]), Some(&plan(1)), &HashMap::new()).remove(0);
        assert_eq!(a.verdicts.capture, UNKNOWN);
    }

    fn manifest_for(p: &Plan) -> Manifest {
        Manifest {
            reviewed: true,
            missing: Vec::new(),
            plan: Some(Ok(p.clone())),
            sha256: [7; 32],
        }
    }

    fn precommit(at: u64) -> Precommit {
        Precommit {
            manifest_sha256: [7; 32],
            decided_by: "operator".into(),
            committed_unix_ms: at,
            sha256: [8; 32],
        }
    }

    fn rate(
        attempts: &[Attempt],
        facts: &[(Fact, u64)],
        j: &Journaled,
        m: &Result<Manifest, String>,
        c: Option<&Result<Precommit, String>>,
    ) -> Campaign {
        campaign(
            attempts,
            facts,
            j,
            &Rating {
                manifest: Some(m),
                precommit: c,
                build_sha256: Some([1; 32]),
                config_sha256: [2; 32],
                follow_tolerance_ms: 30_000,
            },
        )
    }

    /// Three attempts, each PASS but the third (`third`'s bound): their
    /// facts, the journal and the witnesses.
    fn three(third: u64) -> (Vec<(Fact, u64)>, Journaled, Witnesses) {
        let mut facts = Vec::new();
        let mut w = HashMap::new();
        for (i, bound) in [(1, 1), (2, 1), (3, third)] {
            facts.extend(facts_of(
                i,
                Some(LAUNCH_BLOCK),
                1_000,
                Some(PlacementVerdict::First),
                T0_MS,
            ));
            let v = if bound < STRICT_LIMIT_NS {
                "PASS"
            } else {
                "FAIL"
            };
            w.extend([witness(i, v, bound)]);
        }
        (facts, journal(40, &[1, 2, 3]), w)
    }

    /// A synthetic campaign meeting every rule passes (rules only); one
    /// slow attempt among fast ones fails it.
    #[test]
    fn one_slow_attempt_fails_the_campaign() {
        let p = plan(3);
        let m = Ok(manifest_for(&p));
        let c = Ok(precommit(T0_MS - 10_000));
        let (facts, j, w) = three(1);
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        assert_eq!(
            a.last().unwrap().counters,
            Counters {
                attempts: 3,
                acquired: 3,
                passed: 3,
                failed: 0,
                undecided: 0
            }
        );
        let v = rate(&a, &facts, &j, &m, Some(&c));
        assert_eq!(
            (v.verdict, v.release),
            (CAMPAIGN_PASS, RELEASE_EVIDENCE_PASS),
            "{:?}",
            v.reasons
        );
        let (facts, j, w) = three(1_000_000);
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let v = rate(&a, &facts, &j, &m, Some(&c));
        assert_eq!(
            (v.verdict, v.release),
            (CAMPAIGN_FAIL, RELEASE_EVIDENCE_NOT_READY)
        );
        assert_eq!((v.passed, v.failed), (2, 1));
    }

    /// No admitted attempt never passes; nor does a campaign short of a
    /// cell's attempts or duration; misses in a row above the limit fail.
    #[test]
    fn a_campaign_without_its_samples_never_passes() {
        let p = plan(3);
        let m = Ok(manifest_for(&p));
        let c = Ok(precommit(T0_MS - 10_000));
        let j = journal(40, &[1, 2]);
        let v = rate(&[], &[], &j, &m, Some(&c));
        assert_eq!(v.verdict, CAMPAIGN_OPEN);
        assert!(v.reasons.contains(&"NO_ADMITTED_ATTEMPT".to_string()));
        assert_eq!((v.observed, v.skipped, v.consecutive_misses), (2, 2, 2));
        let strict = Plan {
            max_consecutive_misses: 1,
            ..p.clone()
        };
        let v = rate(&[], &[], &j, &Ok(manifest_for(&strict)), Some(&c));
        assert_eq!(v.verdict, CAMPAIGN_FAIL);
        let (facts, j, w) = three(1);
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let more = Plan {
            cells: vec![CellPlan {
                cell: EXTERNAL_FULL_LAUNCH,
                min_attempts: 4,
                min_duration_s: 1,
            }],
            ..p.clone()
        };
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&more)), Some(&c));
        assert_eq!(v.verdict, CAMPAIGN_OPEN);
        let longer = Plan {
            cells: vec![CellPlan {
                cell: EXTERNAL_FULL_LAUNCH,
                min_attempts: 3,
                min_duration_s: 3_600,
            }],
            ..p
        };
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&longer)), Some(&c));
        assert_eq!(
            (v.verdict, v.cells[0].verdict),
            (CAMPAIGN_OPEN, CAMPAIGN_OPEN)
        );
    }

    /// No manifest, an unreviewed or incomplete one, one of another build
    /// or configuration, no pre-commitment or a manifest changed after it:
    /// CAMPAIGN_NOT_READY; attempts before the commitment are not the
    /// campaign's.
    #[test]
    fn a_manifest_not_frozen_before_the_campaign_rates_nothing() {
        let p = plan(3);
        let (facts, j, w) = three(1);
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let c = Ok(precommit(T0_MS - 10_000));
        let none = campaign(
            &a,
            &facts,
            &j,
            &Rating {
                manifest: None,
                precommit: Some(&c),
                build_sha256: Some([1; 32]),
                config_sha256: [2; 32],
                follow_tolerance_ms: 30_000,
            },
        );
        assert_eq!(none.verdict, CAMPAIGN_NOT_READY);
        let proposed = Ok(Manifest {
            reviewed: false,
            plan: None,
            missing: vec!["BSC_CAMPAIGN_REVIEWER"],
            ..manifest_for(&p)
        });
        assert_eq!(
            rate(&a, &facts, &j, &proposed, Some(&c)).verdict,
            CAMPAIGN_NOT_READY
        );
        let other_build = Ok(manifest_for(&Plan {
            build_sha256: [9; 32],
            ..p.clone()
        }));
        assert_eq!(
            rate(&a, &facts, &j, &other_build, Some(&c)).verdict,
            CAMPAIGN_NOT_READY
        );
        let other_config = Ok(manifest_for(&Plan {
            config_sha256: [9; 32],
            ..p.clone()
        }));
        assert_eq!(
            rate(&a, &facts, &j, &other_config, Some(&c)).verdict,
            CAMPAIGN_NOT_READY
        );
        let m = Ok(manifest_for(&p));
        assert_eq!(rate(&a, &facts, &j, &m, None).verdict, CAMPAIGN_NOT_READY);
        let changed = Ok(Precommit {
            manifest_sha256: [6; 32],
            ..precommit(T0_MS - 10_000)
        });
        let v = rate(&a, &facts, &j, &m, Some(&changed));
        assert_eq!(v.verdict, CAMPAIGN_NOT_READY);
        assert!(
            v.reasons
                .contains(&"MANIFEST_CHANGED_AFTER_PRECOMMIT".to_string())
        );
        // Committed after the attempts this run directory journaled: its
        // commitment came after its results (review N2).
        let late = Ok(precommit(T0_MS + 1));
        let v = rate(&a, &facts, &j, &m, Some(&late));
        assert_eq!((v.verdict, v.written), (CAMPAIGN_NOT_READY, 0));
        assert!(
            v.reasons
                .contains(&"EARLIER_ATTEMPTS_IN_THIS_RUN_DIRECTORY: 3".to_string())
        );
    }

    /// Self-launch evidence never releases a third-party-launch product.
    #[test]
    fn self_launch_evidence_does_not_release_third_party_launches() {
        let (mut facts, j, w) = three(1);
        // Review H3: the operator's own launcher wallet, not the buyer's.
        for (f, _) in &mut facts {
            if f.launcher.is_some() {
                f.launcher = Some(OWN_LAUNCHER);
            }
        }
        let unlisted = Plan {
            self_launchers: Vec::new(),
            ..plan(3)
        };
        let cells = |p: Option<&Plan>| {
            evaluate_one(&facts, &j, p, &w)
                .iter()
                .map(|a| a.cell)
                .collect::<Vec<_>>()
        };
        assert_eq!(cells(Some(&unlisted)), [EXTERNAL_FULL_LAUNCH; 3]);
        assert_eq!(
            cells(None),
            [CELL_UNKNOWN; 3],
            "no manifest: not known to be another's"
        );
        let p = Plan {
            cells: vec![CellPlan {
                cell: SELF_LAUNCH,
                min_attempts: 3,
                min_duration_s: 1,
            }],
            ..plan(3)
        };
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        assert!(a.iter().all(|a| a.cell == SELF_LAUNCH));
        let c = Ok(precommit(T0_MS - 10_000));
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
        assert_eq!(
            (v.verdict, v.release),
            (CAMPAIGN_PASS, RELEASE_EVIDENCE_NOT_READY)
        );
        let claims_self = Plan {
            release_scope: SELF_LAUNCH,
            ..p
        };
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&claims_self)), Some(&c));
        assert_eq!(v.release, RELEASE_EVIDENCE_PASS);
    }

    /// Spending beyond the manifest's limits fails the campaign.
    #[test]
    fn spending_beyond_the_manifests_limits_fails_the_campaign() {
        let (facts, j, w) = three(1);
        let c = Ok(precommit(T0_MS - 10_000));
        for p in [
            Plan {
                amount_limit_wei: U256::from(999_999u64),
                ..plan(3)
            },
            Plan {
                cost_limit_wei: U256::from(3_000_000u64),
                ..plan(3)
            },
        ] {
            let a = evaluate_one(&facts, &j, Some(&p), &w);
            assert_eq!(
                rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c)).verdict,
                CAMPAIGN_FAIL
            );
        }
    }

    const MANIFEST: &str = "BSC_CAMPAIGN_STATE=REVIEWED
BSC_CAMPAIGN_ID=c1
BSC_CAMPAIGN_BUILD_SHA256=0101010101010101010101010101010101010101010101010101010101010101
BSC_CAMPAIGN_CONFIG_SHA256=0x0202020202020202020202020202020202020202020202020202020202020202
BSC_CAMPAIGN_CHAIN_ID=56
BSC_CAMPAIGN_HOST=test-host
BSC_CAMPAIGN_ADAPTERS=CLUB48_BUILDER_V1
BSC_CAMPAIGN_SOURCES=BSC_INGRESS_PUBLICNODE
BSC_CAMPAIGN_SELF_LAUNCHERS=0xd1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1
BSC_CAMPAIGN_CELLS=EXTERNAL_FULL_LAUNCH
BSC_CAMPAIGN_EXTERNAL_FULL_LAUNCH_MIN_ATTEMPTS=3
BSC_CAMPAIGN_EXTERNAL_FULL_LAUNCH_MIN_DURATION_S=1
BSC_CAMPAIGN_SELF_LAUNCH_MIN_ATTEMPTS=
BSC_CAMPAIGN_SELF_LAUNCH_MIN_DURATION_S=
BSC_CAMPAIGN_SAMPLE_RATIONALE=r
BSC_CAMPAIGN_WORKLOAD=one launch at a time
BSC_CAMPAIGN_FAILURE_SCENARIOS=restart, reorg, provider refusal
BSC_CAMPAIGN_ELIGIBILITY=CANONICAL_FLAP_C1_SCOPE
BSC_CAMPAIGN_MAX_SKIP_PPM=1000000
BSC_CAMPAIGN_MAX_CONSECUTIVE_MISSES=10
BSC_CAMPAIGN_MIN_COVERED_PPM=900000
BSC_CAMPAIGN_UNCOVERED=OPEN
BSC_CAMPAIGN_PRODUCER_WITNESS=w
BSC_CAMPAIGN_CLOCK_ERROR_BOUND_NS=1000
BSC_CAMPAIGN_PLACEMENT=FIRST_AFTER_LAUNCH
BSC_CAMPAIGN_FINALITY_BLOCKS=3
BSC_CAMPAIGN_REORG_HORIZON_BLOCKS=2
BSC_CAMPAIGN_AUTHORIZATION_REF=a
BSC_CAMPAIGN_AMOUNT_LIMIT_WEI=10000000
BSC_CAMPAIGN_COST_LIMIT_WEI=100000000
BSC_CAMPAIGN_STOP_CONDITIONS=s
BSC_CAMPAIGN_REVIEWER=x
BSC_CAMPAIGN_RELEASE_SCOPE=EXTERNAL_FULL_LAUNCH
";

    /// The manifest file is read strictly: every key once, none unknown; a
    /// reviewed one names every value of an enabled cell; the repository's
    /// file is a proposal that rates nothing.
    #[test]
    fn the_manifest_is_parsed_strictly() {
        let m = Manifest::parse(MANIFEST.as_bytes()).unwrap();
        assert!(m.missing.is_empty(), "{:?}", m.missing);
        assert_eq!(m.plan, Some(Ok(plan(3))));
        let edit = |from: &str, to: &str| Manifest::parse(MANIFEST.replace(from, to).as_bytes());
        assert!(
            edit(
                "BSC_CAMPAIGN_ID=c1\n",
                "BSC_CAMPAIGN_ID=c1\nBSC_CAMPAIGN_ID=c2\n"
            )
            .is_err()
        );
        assert!(edit("BSC_CAMPAIGN_ID=c1\n", "BSC_CAMPAIGN_ID=c1\nBSC_OTHER=1\n").is_err());
        assert!(edit("BSC_CAMPAIGN_ID=c1\n", "").is_err());
        let empty = edit("BSC_CAMPAIGN_REVIEWER=x", "BSC_CAMPAIGN_REVIEWER=").unwrap();
        assert_eq!(
            (empty.missing, empty.plan),
            (vec!["BSC_CAMPAIGN_REVIEWER"], None)
        );
        let both = edit(
            "CELLS=EXTERNAL_FULL_LAUNCH",
            "CELLS=EXTERNAL_FULL_LAUNCH,SELF_LAUNCH",
        )
        .unwrap();
        assert_eq!(both.missing.len(), 2);
        for (from, to) in [
            ("PLACEMENT=FIRST_AFTER_LAUNCH", "PLACEMENT=ANY"),
            ("MIN_ATTEMPTS=3", "MIN_ATTEMPTS=0"),
            ("MAX_SKIP_PPM=1000000", "MAX_SKIP_PPM=1000001"),
            ("AMOUNT_LIMIT_WEI=10000000", "AMOUNT_LIMIT_WEI=0"),
            ("CLOCK_ERROR_BOUND_NS=1000", "CLOCK_ERROR_BOUND_NS=1000001"),
            (
                "RELEASE_SCOPE=EXTERNAL_FULL_LAUNCH",
                "RELEASE_SCOPE=PRIVATE",
            ),
            ("UNCOVERED=OPEN", "UNCOVERED=SOMETIMES"),
        ] {
            assert!(matches!(edit(from, to).unwrap().plan, Some(Err(_))), "{to}");
        }
        // BOUNDED needs a floor above zero.
        let bounded = MANIFEST
            .replace("UNCOVERED=OPEN", "UNCOVERED=BOUNDED")
            .replace("MIN_COVERED_PPM=900000", "MIN_COVERED_PPM=0");
        let bounded = Manifest::parse(bounded.as_bytes()).unwrap().plan;
        assert!(matches!(bounded, Some(Err(_))), "{bounded:?}");
        let bounded = MANIFEST.replace("UNCOVERED=OPEN", "UNCOVERED=BOUNDED");
        let plan = Manifest::parse(bounded.as_bytes()).unwrap().plan;
        assert!(plan.unwrap().unwrap().uncovered_bounded);
        let proposed = edit("STATE=REVIEWED", "STATE=PROPOSED").unwrap();
        assert_eq!((proposed.reviewed, proposed.plan), (false, None));
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../config/live_campaign.env");
        let repo = Manifest::parse(&std::fs::read(repo).unwrap()).unwrap();
        assert!(!repo.reviewed && repo.plan.is_none() && !repo.missing.is_empty());
    }

    /// A pre-commitment is written once, only for a reviewed and complete
    /// manifest with valid values, and reads back.
    #[test]
    fn a_precommit_is_written_once_for_a_reviewed_manifest() {
        let dir = std::env::temp_dir().join(format!("bsc-live-precommit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let m = Manifest::parse(MANIFEST.as_bytes()).unwrap();
        let proposed = Manifest::parse(
            MANIFEST
                .replace("STATE=REVIEWED", "STATE=PROPOSED")
                .as_bytes(),
        )
        .unwrap();
        assert!(Precommit::write(&dir, &proposed, "op", 5).is_err());
        assert!(Precommit::write(&dir, &m, " ", 5).is_err());
        let p = Precommit::write(&dir, &m, "op", 5).unwrap();
        assert_eq!((p.manifest_sha256, p.committed_unix_ms), (m.sha256, 5));
        assert!(Precommit::write(&dir, &m, "op", 6).is_err());
        assert_eq!(Precommit::read(&dir).unwrap(), Some(p));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An attempt's record and the campaign's verdict are whole records of
    /// their schemas.
    #[test]
    fn the_records_round_trip() {
        let p = plan(3);
        let (facts, mut j, w) = three(1);
        j.answers.insert((buy_tx(1), 0), (0, Some(310_000)));
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let bytes = a[0].encode(9).unwrap();
        let r = darkbot_bsc_record::decode(&la::SCHEMA, &bytes).unwrap();
        assert_eq!(r.text(la::VERDICT).unwrap(), PASS);
        assert_eq!(r.u64(la::SELECT_TO_SEND_NS).unwrap(), 293_000);
        assert_eq!(r.u64(la::SEND_TO_ACK_NS).unwrap(), 10_000);
        assert_eq!(r.nested_all(la::ADAPTERS).unwrap().len(), 1);
        assert_eq!(r.u64(la::ATTEMPTS).unwrap(), 1);
        let v = rate(
            &a,
            &facts,
            &j,
            &Ok(manifest_for(&p)),
            Some(&Ok(precommit(T0_MS - 10_000))),
        );
        let r = darkbot_bsc_record::decode(&lv::SCHEMA, &v.encode(9).unwrap()).unwrap();
        assert_eq!(r.text(lv::VERDICT).unwrap(), CAMPAIGN_PASS);
        assert_eq!(r.nested_all(lv::CELLS).unwrap().len(), CELLS.len());
    }

    /// The green synthetic campaign of [`one_slow_attempt_fails_the_campaign`]
    /// with `edit` applied to its journal and plan: its verdict.
    fn green_with(edit: impl Fn(&mut Journaled, &mut Plan, &mut Vec<(Fact, u64)>)) -> Campaign {
        let (mut facts, mut j, w) = three(1);
        let mut p = plan(3);
        edit(&mut j, &mut p, &mut facts);
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let c = Ok(precommit(T0_MS - 10_000));
        rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c))
    }

    fn reasons(c: &Campaign) -> String {
        c.reasons.join(" | ")
    }

    /// Review H1: a run not sealed whole (still written, incomplete, with
    /// lost or dropped records, drops unknown), a gap in the followed chain
    /// or a launch not verifiable leaves the campaign open; its duration is
    /// the time its sealed runs cover, not the wall time.
    #[test]
    fn the_campaign_needs_every_run_sealed_whole_and_the_chain_without_a_gap() {
        assert_eq!(green_with(|_, _, _| {}).verdict, CAMPAIGN_PASS);
        let open = |why: &str, edit: &dyn Fn(&mut Journaled)| {
            let c = green_with(|j, _, _| edit(j));
            assert_eq!(c.verdict, CAMPAIGN_OPEN, "{why}: {}", reasons(&c));
            assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
        };
        open("RUN_NOT_SEALED", &|j| j.runs[0].manifest = None);
        open("RUN_INCOMPLETE", &|j| {
            j.runs[0].manifest.as_mut().unwrap().complete = false;
        });
        open("RUN_LOST_RECORDS", &|j| {
            j.runs[0].manifest.as_mut().unwrap().lost_records = 1;
        });
        open("RUN_INGRESS_DROPPED", &|j| {
            j.runs[0].manifest.as_mut().unwrap().ingress_dropped = Some(2);
        });
        open("RUN_INGRESS_DROPS_UNKNOWN", &|j| {
            j.runs[0].manifest.as_mut().unwrap().ingress_dropped = None;
        });
        open("JOURNAL_UNREADABLE", &|j| {
            j.runs[0].read_error = Some("torn".into())
        });
        // A block after every window is missing: the chain has a gap.
        open("CANONICAL_GAP: run", &|j| {
            j.blocks.remove(&(LAUNCH_BLOCK + 37));
        });
        // A launch whose transaction could not be verified.
        open("LAUNCHES_UNVERIFIABLE: 1", &|j| {
            j.record(
                RecordKind::Canonical,
                &rj::canonical_launch_payload(&Joined {
                    launch: LaunchRef {
                        tx: LaunchTx::Unrebuildable,
                        c1_scope: false,
                        tx_hash: [0x44; 32],
                        ..launch_ref(9, LAUNCH_BLOCK + 2)
                    },
                    visibility: Visibility::Missed,
                }),
                RUN,
            );
        });
        // Covered time is the chain time the run followed (17.55 s of
        // blocks), not its wall time: short of a two-minute cell.
        let c = green_with(|_, p, _| p.cells[0].min_duration_s = 120);
        assert_eq!((c.verdict, c.covered_s), (CAMPAIGN_OPEN, 17));
        assert!(reasons(&c).contains("CELL_MINIMUMS_NOT_MET"));
    }

    /// Review H2: a campaign run of another build, configuration or host,
    /// or an attempt through a builder or from a source the manifest does
    /// not name, rates nothing; an attempt without its release record
    /// stays open.
    #[test]
    fn every_run_and_attempt_is_bound_to_the_frozen_scope() {
        let not_ready = |why: &str, edit: &dyn Fn(&mut Journaled, &mut Plan)| {
            let c = green_with(|j, p, _| edit(j, p));
            assert_eq!(c.verdict, CAMPAIGN_NOT_READY, "{why}: {}", reasons(&c));
            assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
        };
        not_ready("RUN_OF_ANOTHER_BUILD", &|j, _| {
            j.runs[0].manifest.as_mut().unwrap().build_sha256 = Some([9; 32]);
        });
        not_ready("RUN_BUILD_UNKNOWN", &|j, _| {
            j.runs[0].manifest.as_mut().unwrap().build_sha256 = None;
        });
        not_ready("RUN_OF_ANOTHER_CONFIG", &|j, _| {
            j.runs[0].manifest.as_mut().unwrap().config_sha256 = Some([9; 32]);
        });
        not_ready("RUN_ON_ANOTHER_HOST", &|j, _| {
            j.runs[0].manifest.as_mut().unwrap().host = Some("other".into());
        });
        not_ready("ATTEMPT_OUTSIDE_FROZEN_SCOPE", &|_, p| {
            p.adapters = vec![LiveAdapter::BlockRazorBuilderV1];
        });
        not_ready("ATTEMPT_OUTSIDE_FROZEN_SCOPE", &|_, p| {
            p.sources = vec![SourceId::Drpc];
        });
        let c = green_with(|j, _, _| {
            j.releases.remove(&buy_tx(2));
        });
        assert_eq!(c.verdict, CAMPAIGN_OPEN, "{}", reasons(&c));
        assert!(reasons(&c).contains("ATTEMPT_WITHOUT_RELEASE_RECORD"));
    }

    /// Review M3: an inclusion the journaled chain does not hold at its
    /// height is not canonical, however deep.
    #[test]
    fn an_inclusion_outside_the_journaled_chain_is_not_canonical() {
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let mut j = journal(40, &[1]);
        let mut other = hash(LAUNCH_BLOCK);
        other[31] ^= 1;
        j.blocks.insert(LAUNCH_BLOCK, (other, T0_MS));
        let a = evaluate_one(&facts, &j, Some(&plan(1)), &HashMap::new()).remove(0);
        assert_eq!(a.verdicts.canonical, UNKNOWN);
        assert!(
            a.reasons
                .contains(&"INCLUSION_NOT_IN_THE_JOURNALED_CHAIN".to_string())
        );
    }

    /// Review M4: a witness is of this execution only through its own
    /// G0.8 result: without it, with another one, or with one of another
    /// transaction, launch or block, the timing stays UNKNOWN.
    #[test]
    fn a_witness_counts_only_with_its_own_g08_result() {
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let j = journal(40, &[1]);
        let p = plan(1);
        let timing = |edit: &dyn Fn(&mut Witness)| {
            let (id, mut w) = witness(1, "PASS", 1);
            edit(w.as_mut().unwrap());
            let a = evaluate_one(&facts, &j, Some(&p), &HashMap::from([(id, w)])).remove(0);
            (a.verdicts.timing, a.reasons)
        };
        assert_eq!(timing(&|_| {}).0, PASS);
        let reason = |edit: &dyn Fn(&mut Witness), why: &str| {
            let (t, reasons) = timing(edit);
            assert_eq!(t, UNKNOWN, "{why}");
            assert!(reasons.contains(&why.to_string()), "{why}: {reasons:?}");
        };
        reason(&|w| w.g08 = None, "PRODUCER_WITNESS_WITHOUT_ITS_G08_RESULT");
        reason(
            &|w| w.g08_sha256 = [6; 32],
            "PRODUCER_WITNESS_OF_ANOTHER_G08_RESULT",
        );
        for field in 0..3 {
            reason(
                &|w| {
                    let Some(Ok(g)) = w.g08.as_mut() else {
                        unreachable!()
                    };
                    match field {
                        0 => g.buy_tx = [9; 32],
                        1 => g.launch_tx = [9; 32],
                        _ => g.buy_block_hash = [9; 32],
                    }
                },
                "G08_RESULT_OF_ANOTHER_EXECUTION",
            );
        }
    }

    /// Review L4, L5, L10: the report names the attempts before the
    /// commitment, the planned and the selected count and why launches
    /// were skipped; a Strategy C entry has no attempt verdict and counts
    /// in no verdict counter.
    #[test]
    fn the_report_names_what_the_campaign_does_not_rate() {
        let (mut facts, mut j, w) = three(1);
        // One launch more, seen in full before its block, never attempted.
        j.record(
            RecordKind::Canonical,
            &rj::canonical_launch_payload(&Joined {
                launch: launch_ref(7, LAUNCH_BLOCK + 3),
                visibility: Visibility::FullBefore {
                    source: SourceId::PublicNode,
                    lead_us: 100,
                },
            }),
            RUN,
        );
        let strategy = Fact {
            component: ComponentId::StrategyC,
            ..fact(FactKind::Released, [0x99; 32], token(9))
        };
        facts.push((strategy, T0_MS));
        let p = plan(3);
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let s = a.iter().find(|a| a.kind == "STRATEGY_BUY").unwrap();
        assert_eq!(
            (s.verdicts.verdict, s.cell),
            (NOT_APPLICABLE, STRATEGY_ENTRY)
        );
        assert_eq!(
            s.counters,
            Counters {
                attempts: 4,
                acquired: 3,
                passed: 3,
                failed: 0,
                undecided: 0
            }
        );
        let late = Ok(precommit(T0_MS + 1));
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&late));
        assert_eq!((v.before_precommit, v.written), (3, 0));
        let c = Ok(precommit(T0_MS - 10_000));
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
        assert_eq!((v.planned, v.observed, v.selected, v.skipped), (3, 4, 1, 1));
        assert_eq!(v.skip_reasons, ["FULL_BEFORE=1"]);
    }

    /// Review N1: a campaign run without a readable record (empty, its
    /// reader refused) or whose segments are not its manifest's is no
    /// covered run: the campaign stays open.
    #[test]
    fn a_run_without_readable_records_or_its_manifests_segments_is_no_coverage() {
        let open = |why: &str, edit: &dyn Fn(&mut Journaled)| {
            let c = green_with(|j, _, _| edit(j));
            assert_eq!(c.verdict, CAMPAIGN_OPEN, "{why}: {}", reasons(&c));
            assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
        };
        open("RUN_EMPTY", &|j| {
            j.runs.push(Run {
                id: T0_MS + 30_000,
                manifest: Some(sealed()),
                manifest_match: Some(true),
                ..Run::default()
            });
        });
        open("JOURNAL_UNREADABLE", &|j| {
            j.runs.push(Run {
                id: T0_MS + 30_000,
                read_error: Some("no segment index".into()),
                ..Run::default()
            });
        });
        open("RUN_SEGMENTS_DIFFER_FROM_MANIFEST", &|j| {
            j.runs[0].manifest_match = Some(false);
        });
    }

    /// Review N3: heights no run followed (a restart) are no gap but time
    /// no sealed run covers, bounded by the plan's coverage floor; a gap
    /// while a run followed the chain stays one.
    #[test]
    fn a_restart_is_uncovered_time_and_a_gap_while_running_stays_open() {
        let split = |j: &mut Journaled| {
            // Run A to block 134 (every market window and the block after
            // it), down until 137, run B from 138.
            for n in 135..138 {
                j.blocks.remove(&(LAUNCH_BLOCK + n - 100));
            }
            let a_end = T0_MS + 34 * 450;
            let b_start = T0_MS + 38 * 450;
            j.runs[0].blocks = Some((LAUNCH_BLOCK, LAUNCH_BLOCK + 34));
            j.runs[0].last_ms = Some(a_end);
            j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(a_end);
            j.runs.push(Run {
                id: b_start,
                manifest: Some(RunManifest {
                    started_ms: b_start,
                    ..sealed()
                }),
                manifest_match: Some(true),
                records: 2,
                last_ms: j.last_ms,
                blocks: Some((LAUNCH_BLOCK + 38, LAUNCH_BLOCK + 39)),
                ..Run::default()
            });
        };
        let c = green_with(|j, p, _| {
            split(j);
            p.min_covered_ppm = 0;
        });
        assert!(!reasons(&c).contains("CANONICAL_GAP"), "{}", reasons(&c));
        assert!(c.uncovered_s > 0 && c.covered_s > 0);
        // 26.2 of 28 s covered: below a floor of 95 %.
        let c = green_with(|j, p, _| {
            split(j);
            p.min_covered_ppm = 950_000;
        });
        assert_eq!(c.verdict, CAMPAIGN_FAIL, "{}", reasons(&c));
        assert!(reasons(&c).contains("COVERAGE_BELOW_FLOOR"));
    }

    /// Review N2: an earlier campaign's attempts (another run directory,
    /// resolved) hold no new one: counted only; unresolved ones hold it
    /// open.
    #[test]
    fn an_earlier_campaigns_attempts_hold_a_new_one_only_while_unresolved() {
        let (mut facts, mut j, w) = three(1);
        let p = plan(3);
        // A resolved attempt of an earlier campaign, journaled elsewhere.
        facts.extend(facts_of(
            5,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS - 600_000,
        ));
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let c = Ok(precommit(T0_MS - 10_000));
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
        assert_eq!(
            (v.verdict, v.before_precommit),
            (CAMPAIGN_PASS, 1),
            "{}",
            reasons(&v)
        );
        // Unresolved, it may still execute.
        let mut pending = facts.clone();
        pending.retain(|(f, at)| !(*at < T0_MS - 10_000 && f.kind != FactKind::Released));
        let a = evaluate_one(&pending, &j, Some(&p), &w);
        let v = rate(&a, &pending, &j, &Ok(manifest_for(&p)), Some(&c));
        assert!(
            reasons(&v).contains("EARLIER_ATTEMPTS_UNRESOLVED: 1"),
            "{}",
            reasons(&v)
        );
        // Journaled in this run directory: its commitment came after it.
        j.releases.insert(
            buy_tx(5),
            Release {
                run: RUN,
                l: release(5, 0),
            },
        );
        let a = evaluate_one(&facts, &j, Some(&p), &w);
        let v = rate(&a, &facts, &j, &Ok(manifest_for(&p)), Some(&c));
        assert_eq!(v.verdict, CAMPAIGN_NOT_READY);
    }

    /// Review LOW 1–5: the configuration's own accounts are self-launchers;
    /// a witness of a G0.8 result other than PASS is none; a refused
    /// release record of an earlier run is not the campaign's; an attempt
    /// on an ineligible launch or through a part of the frozen builders
    /// rates nothing.
    #[test]
    fn the_scope_checks_of_the_re_review_hold() {
        let (mut facts, j, w) = three(1);
        for (f, _) in &mut facts {
            if f.launcher.is_some() {
                f.launcher = Some([0x3E; 20]);
            }
        }
        let p = Plan {
            self_launchers: Vec::new(),
            ..plan(3)
        };
        let own = [[0x3E; 20]];
        let a = attempts(
            &facts,
            &j,
            &Inputs {
                window_ms: WINDOW_MS,
                plan: Some(&p),
                witnesses: &w,
                own_accounts: &own,
            },
        );
        assert!(a.iter().all(|a| a.cell == SELF_LAUNCH));
        let facts = facts_of(
            1,
            Some(LAUNCH_BLOCK),
            1_000,
            Some(PlacementVerdict::First),
            T0_MS,
        );
        let (id, mut wit) = witness(1, "PASS", 1);
        if let Ok(Witness {
            g08: Some(Ok(g)), ..
        }) = &mut wit
        {
            g.verdict = "ADAPT".into();
        }
        let a = evaluate_one(
            &facts,
            &journal(40, &[1]),
            Some(&plan(1)),
            &HashMap::from([(id, wit)]),
        );
        assert!(a[0].reasons.contains(&"G08_RESULT_NOT_PASS".to_string()));
        let c = green_with(|j, _, _| {
            j.releases_refused
                .push((RUN - 1, rj::LIVE_RELEASE_OTHER_LAYOUT));
            j.runs.insert(
                0,
                Run {
                    id: RUN - 1,
                    last_ms: Some(RUN),
                    records: 1,
                    ..Run::default()
                },
            );
        });
        assert!(
            !reasons(&c).contains("RELEASE_RECORD_REFUSED"),
            "{}",
            reasons(&c)
        );
        let c = green_with(|_, p, _| {
            p.adapters = vec![
                LiveAdapter::Club48BuilderV1,
                LiveAdapter::BlockRazorBuilderV1,
            ];
        });
        assert!(
            reasons(&c).contains("ATTEMPT_OUTSIDE_FROZEN_SCOPE"),
            "{}",
            reasons(&c)
        );
        let c = green_with(|j, _, _| {
            for l in &mut j.launches {
                if l.tx_hash == launch_tx(2) {
                    l.c1_scope = false;
                }
            }
        });
        assert!(
            reasons(&c).contains("ATTEMPT_ON_AN_INELIGIBLE_LAUNCH"),
            "{}",
            reasons(&c)
        );
    }

    /// Review P1: coverage is the chain time a run followed: a sealed run
    /// whose follower delivered no block, stopped long before the run
    /// ended, or a later run starting after unfollowed heights holds an
    /// OPEN campaign open (and is uncovered time for a BOUNDED one).
    #[test]
    fn coverage_is_the_chain_a_run_followed_not_its_wall_time() {
        // OPEN mode, no floor: what holds the campaign is the chain itself.
        let open = |why: &str, edit: &dyn Fn(&mut Journaled)| {
            let c = green_with(|j, p, _| {
                edit(j);
                p.min_covered_ppm = 0;
            });
            assert_eq!(c.verdict, CAMPAIGN_OPEN, "{why}: {}", reasons(&c));
            assert!(reasons(&c).contains(why), "{why}: {}", reasons(&c));
            c
        };
        // A whole run whose follower never delivered a block.
        open("RUN_FOLLOWED_NO_BLOCK", &|j| {
            j.runs.push(Run {
                id: T0_MS + 30_000,
                manifest: Some(sealed()),
                manifest_match: Some(true),
                records: 5,
                last_ms: Some(T0_MS + 3_600_000),
                ..Run::default()
            });
        });
        // The follower stopped nine hours before the run ended.
        let tail = |j: &mut Journaled| {
            let end = T0_MS + 9 * 3_600_000;
            j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(end);
            j.runs[0].last_ms = Some(end);
            j.last_ms = Some(end);
        };
        open("RUN_FOLLOWER_STOPPED", &tail);
        // A later run whose follower began after heights no run followed.
        let head = |j: &mut Journaled| {
            j.runs.push(Run {
                id: T0_MS + 60_000,
                manifest: Some(RunManifest {
                    started_ms: T0_MS + 60_000,
                    ended_ms: Some(T0_MS + 70_000),
                    ..sealed()
                }),
                manifest_match: Some(true),
                records: 1,
                last_ms: Some(T0_MS + 70_000),
                blocks: Some((LAUNCH_BLOCK + 150, LAUNCH_BLOCK + 150)),
                ..Run::default()
            });
            j.blocks.insert(
                LAUNCH_BLOCK + 150,
                (hash(LAUNCH_BLOCK + 150), T0_MS + 69_000),
            );
        };
        open("UNCOVERED_HEIGHTS: 110", &head);
        // BOUNDED: both are uncovered time, below the floor the campaign
        // fails.
        for edit in [&tail as &dyn Fn(&mut Journaled), &head] {
            let c = green_with(|j, p, _| {
                edit(j);
                p.uncovered_bounded = true;
            });
            assert_eq!(c.verdict, CAMPAIGN_FAIL, "{}", reasons(&c));
            assert!(
                reasons(&c).contains("COVERAGE_BELOW_FLOOR"),
                "{}",
                reasons(&c)
            );
        }
    }

    /// Review P2: the launches of the chain the follower caught up on
    /// before a run's window (no run was up) are observed and skipped
    /// (MISSED), so a restart within the catch-up keeps the chain whole.
    #[test]
    fn a_restarts_caught_up_launches_are_skips() {
        let c = green_with(|j, _, _| {
            let run_b = T0_MS + 50 * 450;
            // Run A to 134; run B caught up 135..=139 before its window.
            j.runs[0].blocks = Some((LAUNCH_BLOCK, LAUNCH_BLOCK + 34));
            for n in 135..140 {
                let b = LAUNCH_BLOCK + n - 100;
                j.blocks.remove(&b);
            }
            let mut b_run = Run {
                id: run_b,
                manifest: Some(RunManifest {
                    started_ms: run_b,
                    ..sealed()
                }),
                manifest_match: Some(true),
                ..Run::default()
            };
            for n in 135..140u64 {
                let b = LAUNCH_BLOCK + n - 100;
                let ms = T0_MS + (b - LAUNCH_BLOCK) * 450;
                let rec = rj::catchup_block_payload(b, &hash(b), ms);
                if let Some(h) = j.record(RecordKind::Runtime, &rec, run_b) {
                    b_run.blocks = Some(b_run.blocks.map_or((h, h), |(x, y)| (x.min(h), y.max(h))));
                }
                b_run.records += 1;
            }
            let launch = crate::chain_follower::CanonicalLaunch {
                venue: Venue::Flap,
                block_number: LAUNCH_BLOCK + 36,
                block_hash: hash(LAUNCH_BLOCK + 36),
                tx_index: 0,
                tx_hash: [0x45; 32],
                token: Some(token(8)),
                creator: Some(CREATOR),
                external_buys_after: None,
                first_external_buy_index: None,
                sender: Some(CREATOR),
                nonce: Some(0),
                c1_scope: true,
                venue_target: true,
                four_token: FourToken::NotApplicable,
                unverifiable: None,
            };
            let rec = rj::catchup_launch_payload(&launch, T0_MS + 36 * 450);
            j.record(RecordKind::Runtime, &rec, run_b);
            b_run.last_ms = j.last_ms;
            j.runs.push(b_run);
        });
        assert!(
            !reasons(&c).contains("UNCOVERED_HEIGHTS"),
            "{}",
            reasons(&c)
        );
        assert_eq!((c.observed, c.skipped), (4, 1));
        assert_eq!(c.skip_reasons, ["MISSED=1"]);
        assert_eq!(c.verdict, CAMPAIGN_PASS, "{}", reasons(&c));
    }

    /// Review Q1: a market window reaching into the chain a later run
    /// caught up on is complete with the catch-up's own trades (`d`), read
    /// as the window's; a refused log of it (`e`) leaves it incomplete.
    /// Review LOW 1: a run whose follower began long after it started is
    /// named (OPEN).
    #[test]
    fn a_window_into_the_catch_up_uses_its_trades_and_a_late_follower_is_named() {
        let split = |j: &mut Journaled, refused: bool| {
            // Run A to block 120 (inside every window); run B caught up
            // 121..=139 with an external BUY in 125.
            let run_b = T0_MS + 60_000;
            j.runs[0].blocks = Some((LAUNCH_BLOCK, LAUNCH_BLOCK + 20));
            let a_end = T0_MS + 20 * 450;
            j.runs[0].last_ms = Some(a_end);
            j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(a_end);
            let mut b_run = Run {
                id: run_b,
                manifest: Some(RunManifest {
                    started_ms: run_b,
                    ..sealed()
                }),
                manifest_match: Some(true),
                ..Run::default()
            };
            for n in 21..40u64 {
                let b = LAUNCH_BLOCK + n;
                j.blocks.remove(&b);
                let rec = rj::catchup_block_payload(b, &hash(b), T0_MS + n * 450);
                if let Some(h) = j.record(RecordKind::Runtime, &rec, run_b) {
                    b_run.blocks = Some(b_run.blocks.map_or((h, h), |(x, y)| (x.min(h), y.max(h))));
                }
                b_run.records += 1;
            }
            let mut buy = trade(
                LAUNCH_BLOCK + 25,
                0,
                [0x99; 32],
                token(1),
                false,
                500,
                1_000_000,
            );
            buy[0] = b'd';
            j.record(RecordKind::Runtime, &buy, run_b);
            if refused {
                let mut gap =
                    rj::trade_gap_payload(LAUNCH_BLOCK + 26, &hash(LAUNCH_BLOCK + 26), 0, 0);
                gap[0] = b'e';
                j.record(RecordKind::Runtime, &gap, run_b);
            }
            j.runs[0].manifest.as_mut().unwrap().ended_ms = Some(a_end);
            b_run.last_ms = Some(run_b);
            j.runs.push(b_run);
        };
        let (facts, mut j, w) = three(1);
        split(&mut j, false);
        let a = evaluate_one(&facts, &j, Some(&plan(3)), &w);
        let window = &a[0].window;
        assert_eq!(window.why_incomplete, None);
        assert_eq!(window.first_external.map(|f| f.1), Some(LAUNCH_BLOCK + 25));
        let (facts, mut j, w) = three(1);
        split(&mut j, true);
        let a = evaluate_one(&facts, &j, Some(&plan(3)), &w);
        assert_eq!(a[0].window.why_incomplete, Some("TRADE_LOG_REFUSED"));
        // The follower of a later run began a minute after its start.
        let c = green_with(|j, _, _| {
            j.runs.push(Run {
                id: T0_MS + 20_000,
                manifest: Some(RunManifest {
                    started_ms: T0_MS + 20_000,
                    ended_ms: Some(T0_MS + 90_000),
                    ..sealed()
                }),
                manifest_match: Some(true),
                records: 1,
                last_ms: Some(T0_MS + 90_000),
                blocks: Some((LAUNCH_BLOCK + 40, LAUNCH_BLOCK + 40)),
                ..Run::default()
            });
            j.blocks
                .insert(LAUNCH_BLOCK + 40, (hash(LAUNCH_BLOCK + 40), T0_MS + 85_000));
        });
        assert!(reasons(&c).contains("RUN_FOLLOWER_LATE"), "{}", reasons(&c));
    }
}
