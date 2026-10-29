//! DEC-08 (ENTSCHEIDUNGEN §8.6): the state of each chain verification method of a run, read
//! from its typed verification records only — for the Capture tab (method, status, progress,
//! provider, requests, bytes, checkpoint, block_reason) and the Analytics tab (the methods side
//! by side: four liquidation kinds, six markers, found and missing events, deviations, costs).
//! One entry per method: its spec with the newest attempt. Nothing is promoted: the status is
//! the method's own (only STRICT reaches CANONICAL_VERIFIED). Counts are shown for a complete
//! result only; an unknown or partial count is absent, never 0.
//!
//! Bounded per projection tick: directory listings and immutable results are memoized (a cache
//! directory by its modification time, a content-addressed result by its path, STRICT's
//! verified evidence by its file set).
use super::index::{CrosscheckResult, IndexResult, KINDS};
use super::parity::MARKERS;
use super::strict_driver::{AttemptRecord, VerificationSpec, NATIVE_PREFIX};
use super::verification::{read_report_with, StrictResult};
use crate::analytics::journal;
use std::collections::{BTreeMap, HashMap};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// One chain verification method of a run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MethodState {
    /// off | index | strict | crosscheck.
    pub method: String,
    pub spec_id: String,
    pub attempts: u64,
    /// The newest attempt has started, not ended, and holds the spec's writer lock.
    pub running: bool,
    /// RUNNING; INTERRUPTED (started, not ended, no longer running); else the newest attempt's
    /// end status (NOT_RUN, INDEX_VERIFIED, CROSSCHECK_VERIFIED, CANONICAL_VERIFIED,
    /// HISTORICAL_INCOMPLETE).
    pub status: String,
    /// The newest attempt's block or incompleteness reason.
    pub reason: Option<String>,
    /// The newest finished attempt's result (the counts below are its).
    pub result_id: Option<String>,
    pub offline: bool,
    /// SHA-256 of the provider endpoint(s) the spec binds (never the endpoint); `None` offline.
    pub provider_binding: Option<String>,
    /// Typed chain reads held in the checkpoint (a resume reads them without the network).
    pub checkpoint_reads: u64,
    /// Requests and response bytes of the newest result, and of every attempt of the spec.
    pub calls: Option<u64>,
    pub bytes: Option<u64>,
    pub calls_total: Option<u64>,
    pub bytes_total: Option<u64>,
    /// `pages` (INDEX, CROSSCHECK: both sources) or `slots` (STRICT).
    pub progress_unit: &'static str,
    pub progress_done: u64,
    pub progress_total: Option<u64>,
    /// Qualified events per liquidation kind (empty: unknown or incomplete).
    pub qualified: Vec<(String, u64)>,
    /// Program events per kind including unqualified transactions (empty: unknown).
    pub observed: Vec<(String, u64)>,
    /// The six markers (empty: unknown).
    pub markers: Vec<(String, u64)>,
    /// Detection statuses of the window's liquidation events against the capture (MATCH,
    /// MISSING_*, *_MISMATCH, …; INDEX/CROSSCHECK).
    pub comparison: Vec<(String, u64)>,
    /// Transaction statuses of the window against the capture (MATCH, SLOT_MISMATCH,
    /// MISSING_LIVE) and the capture's live transactions the index lacks
    /// (BOT_ONLY_INDEX_MISSING).
    pub transactions: Vec<(String, u64)>,
    /// CROSSCHECK: transactions the two sources read differently.
    pub disagreements: Option<u64>,
    pub unresolved: Option<u64>,
    pub unknown: Option<u64>,
}

fn kinds(n: &[u64; 4]) -> Vec<(String, u64)> {
    KINDS
        .iter()
        .map(|k| k.as_str().to_string())
        .zip(n.iter().copied())
        .collect()
}
fn markers(n: &[u64; 6]) -> Vec<(String, u64)> {
    MARKERS
        .iter()
        .map(|(_, name)| name.to_string())
        .zip(n.iter().copied())
        .collect()
}

/// Memo tables, each bounded: a full table is cleared (recomputed on the next tick).
const MEMO_ENTRIES: usize = 4096;
fn memo<K: std::hash::Hash + Eq, V: Clone>(
    table: &Mutex<Option<HashMap<K, V>>>,
    key: K,
    valid: impl Fn(&V) -> bool,
    compute: impl FnOnce() -> V,
) -> V {
    let mut guard = table.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(v) = map.get(&key).filter(|v| valid(v)) {
        return v.clone();
    }
    if map.len() >= MEMO_ENTRIES {
        map.clear();
    }
    let v = compute();
    map.insert(key, v.clone());
    v
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Entries of a cache directory, recounted only when the directory changed.
#[allow(clippy::type_complexity)]
static FILE_COUNTS: Mutex<Option<HashMap<PathBuf, (Option<SystemTime>, u64)>>> = Mutex::new(None);
fn files(dir: &Path) -> u64 {
    let at = modified(dir);
    memo(
        &FILE_COUNTS,
        dir.to_path_buf(),
        |(seen, _)| *seen == at,
        || {
            (
                at,
                std::fs::read_dir(dir).map_or(0, |d| d.flatten().count() as u64),
            )
        },
    )
    .1
}

/// What the view shows of one immutable result (decoded once per content-addressed path).
#[derive(Clone, Default)]
struct Summary {
    complete: bool,
    calls: u64,
    bytes: u64,
    pages: u64,
    qualified: [u64; 4],
    observed: [u64; 4],
    markers: [u64; 6],
    events: BTreeMap<String, u64>,
    transactions: BTreeMap<String, u64>,
    disagreements: Option<u64>,
    unresolved: u64,
    unknown: u64,
}

/// The index's detection statuses of liquidation events, and its transaction statuses (the
/// result's comparison counts are both, as the pinned report counts them).
fn split(r: &IndexResult) -> (BTreeMap<String, u64>, BTreeMap<String, u64>) {
    let mut events: BTreeMap<String, u64> = BTreeMap::new();
    for e in r.comparisons.iter().flat_map(|c| &c.events) {
        *events.entry(e.detection.clone()).or_default() += 1;
    }
    let transactions = r
        .comparison_counts
        .iter()
        .filter_map(|(k, v)| {
            let n = v.saturating_sub(events.get(k).copied().unwrap_or(0));
            (n > 0).then(|| (k.clone(), n))
        })
        .collect();
    (events, transactions)
}

fn index_summary(r: &IndexResult) -> Summary {
    let (events, transactions) = split(r);
    Summary {
        complete: r.status == "INDEX_VERIFIED",
        calls: r.calls,
        bytes: r.bytes,
        pages: r.pages,
        qualified: r.qualified,
        observed: r.observed,
        markers: r.markers,
        events,
        transactions,
        disagreements: None,
        unresolved: r.unresolved_transactions,
        unknown: r.unknown_transactions,
    }
}

#[allow(clippy::type_complexity)]
static SUMMARIES: Mutex<Option<HashMap<PathBuf, Option<Summary>>>> = Mutex::new(None);
fn summary(mode: &str, path: &Path) -> Option<Summary> {
    memo(
        &SUMMARIES,
        path.to_path_buf(),
        |_| true,
        || match mode {
            "index" => journal::read_typed_value::<IndexResult>(path)
                .ok()
                .map(|r| index_summary(&r)),
            "crosscheck" => journal::read_typed_value::<CrosscheckResult>(path)
                .ok()
                .map(|r| {
                    let (p, s) = (index_summary(&r.primary), index_summary(&r.secondary));
                    Summary {
                        complete: r.status == "CROSSCHECK_VERIFIED",
                        calls: p.calls + s.calls,
                        bytes: p.bytes + s.bytes,
                        pages: p.pages + s.pages,
                        disagreements: Some(r.disagreements.len() as u64),
                        unresolved: p.unresolved + s.unresolved,
                        unknown: p.unknown + s.unknown,
                        ..p
                    }
                }),
            "strict" => journal::read_typed_value::<StrictResult>(path)
                .ok()
                .map(|r| Summary {
                    complete: r.status.as_str() == "CANONICAL_VERIFIED",
                    calls: r.rpc_calls,
                    bytes: r.successful_response_bytes,
                    ..Default::default()
                }),
            _ => None,
        },
    )
}

/// The results of a mode directory, every attempt's (content-addressed, immutable).
fn results(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir.join("results"))
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "p0j"))
                .collect()
        })
        .unwrap_or_default()
}

/// STRICT's verified evidence as shown: slots done and expected (`None`: scope unresolved),
/// whether the whole scope is verified, kinds and markers of the verified batches.
#[derive(Clone, Copy)]
struct StrictView {
    done: u64,
    total: Option<u64>,
    complete: bool,
    kinds: [u64; 4],
    markers: [u64; 6],
}

/// Memoized per file set of the strict directory (checkpoints and batches are immutable; a new
/// batch or a newly resolved scope is a new set).
#[allow(clippy::type_complexity)]
static STRICT_VIEWS: Mutex<Option<HashMap<PathBuf, (Vec<String>, StrictView)>>> = Mutex::new(None);

fn strict_view(dir: &Path) -> StrictView {
    let mut set: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    set.sort();
    let key = set.clone();
    memo(
        &STRICT_VIEWS,
        dir.to_path_buf(),
        |(seen, _)| *seen == key,
        || {
            let program = solana_sdk::pubkey::Pubkey::from_str_const(
                darkbot_project0::protocol::PROGRAM_ID_MAINNET,
            );
            let mut marks = [0u64; 6];
            let report = read_report_with(dir, &mut |row| {
                if let Some(tx) = row.chain_tx() {
                    for (m, _, _) in super::parity::markers(tx, &program) {
                        marks[m] += 1;
                    }
                }
            });
            let mut kinds = [0u64; 4];
            for e in &report.canonical {
                if let Some(i) = KINDS.iter().position(|k| *k == e.liquidation_type) {
                    kinds[i] += 1;
                }
            }
            (
                set,
                StrictView {
                    done: report.slots_fetched + report.slots_skipped_confirmed,
                    total: report.scope.as_ref().map(|_| report.slots_expected),
                    complete: report.complete(),
                    kinds,
                    markers: marks,
                },
            )
        },
    )
    .1
}

/// A spec of the run with its attempts (start, end) in start order.
struct Spec {
    id: String,
    dir: PathBuf,
    spec: VerificationSpec,
    attempts: Vec<(AttemptRecord, Option<AttemptRecord>)>,
}

fn specs(run: &Path) -> Vec<Spec> {
    let Ok(entries) = std::fs::read_dir(run.join("verification")) else {
        return vec![];
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let dir = e.path();
        let Some(id) = dir
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(NATIVE_PREFIX))
            .map(str::to_string)
        else {
            continue;
        };
        let Ok(spec) = journal::read_typed_value::<VerificationSpec>(&dir.join("spec.p0j")) else {
            continue;
        };
        let mut attempts: Vec<(AttemptRecord, Option<AttemptRecord>)> =
            std::fs::read_dir(dir.join("attempts"))
                .map(|d| {
                    d.flatten()
                        .filter_map(|a| {
                            let start =
                                journal::read_typed_value(&a.path().join("start.p0j")).ok()?;
                            Some((
                                start,
                                journal::read_typed_value(&a.path().join("end.p0j")).ok(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
        attempts.sort_by_key(|(s, _)| s.at_unix_ns);
        out.push(Spec {
            id,
            dir,
            spec,
            attempts,
        });
    }
    out
}

/// The driver holds the spec's writer lock (exclusive) for an attempt's whole duration: an
/// attempt without its end record is running only while that lock is held.
fn lock_held(dir: &Path) -> bool {
    let Ok(f) = std::fs::File::open(dir.join("writer.lock")) else {
        return false;
    };
    // SAFETY: flock on an owned descriptor; a shared lock taken here is released on drop.
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) != 0 }
}

/// The state of one spec (its newest attempt and newest finished result).
fn state(s: &Spec) -> MethodState {
    let newest = s.attempts.last();
    let finished = s.attempts.iter().rev().find_map(|(_, end)| end.as_ref());
    let open = newest.is_some_and(|(_, end)| end.is_none());
    let running = open && lock_held(&s.dir);
    let mut m = MethodState {
        method: s.spec.mode.clone(),
        spec_id: s.id.clone(),
        attempts: s.attempts.len() as u64,
        running,
        status: match newest {
            Some((_, None)) if running => "RUNNING".into(),
            Some((_, None)) => "INTERRUPTED".into(),
            Some((_, Some(end))) => end.status.clone().unwrap_or_default(),
            None => "NOT_RUN".into(),
        },
        reason: newest
            .and_then(|(_, end)| end.as_ref())
            .and_then(|e| e.reason.clone()),
        result_id: finished.and_then(|e| e.verification_result_id.clone()),
        offline: newest.is_some_and(|(start, _)| start.offline),
        provider_binding: s.spec.provider_binding_sha256.map(|b| journal::hex(&b)),
        progress_unit: "none",
        ..Default::default()
    };
    let mode = s.spec.mode.as_str();
    let dir = s.dir.join(mode);
    let caches: Vec<PathBuf> = match mode {
        "crosscheck" => vec![dir.join("primary/cache"), dir.join("secondary/cache")],
        _ => vec![dir.join("cache")],
    };
    m.checkpoint_reads = caches.iter().map(|c| files(c)).sum();
    if !matches!(mode, "index" | "crosscheck" | "strict") {
        return m;
    }
    m.progress_unit = if mode == "strict" { "slots" } else { "pages" };
    let all: Vec<Summary> = results(&dir)
        .iter()
        .filter_map(|p| summary(mode, p))
        .collect();
    if !all.is_empty() {
        m.calls_total = Some(all.iter().map(|x| x.calls).sum());
        m.bytes_total = Some(all.iter().map(|x| x.bytes).sum());
    }
    let newest_result = m
        .result_id
        .as_ref()
        .and_then(|id| summary(mode, &dir.join("results").join(format!("{id}.p0j"))));
    if let Some(r) = &newest_result {
        (m.calls, m.bytes) = (Some(r.calls), Some(r.bytes));
        if mode != "strict" {
            m.progress_done = r.pages;
            // Counts of an incomplete reading are no facts: shown only when it verified.
            if r.complete {
                m.qualified = kinds(&r.qualified);
                m.observed = kinds(&r.observed);
                m.markers = markers(&r.markers);
                m.comparison = r.events.iter().map(|(k, v)| (k.clone(), *v)).collect();
                m.transactions = r
                    .transactions
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                (m.unresolved, m.unknown) = (Some(r.unresolved), Some(r.unknown));
                m.disagreements = r.disagreements;
            }
        }
    }
    if mode == "strict" {
        let view = strict_view(&dir);
        m.progress_done = view.done;
        m.progress_total = view.total;
        if view.complete {
            m.qualified = kinds(&view.kinds);
            m.markers = markers(&view.markers);
        }
    }
    m
}

/// Every method of `run` with a native spec: per method its spec with the newest attempt, in
/// method order off, index, crosscheck, strict.
pub fn methods(run: &Path) -> Vec<MethodState> {
    let mut newest: BTreeMap<u8, Spec> = BTreeMap::new();
    for s in specs(run) {
        let rank = match s.spec.mode.as_str() {
            "off" => 0,
            "index" => 1,
            "crosscheck" => 2,
            "strict" => 3,
            _ => continue,
        };
        let at = |s: &Spec| s.attempts.last().map(|(a, _)| a.at_unix_ns);
        if newest.get(&rank).is_none_or(|kept| at(&s) > at(kept)) {
            newest.insert(rank, s);
        }
    }
    newest.values().map(state).collect()
}
