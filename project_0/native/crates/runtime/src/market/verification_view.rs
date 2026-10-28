//! DEC-08 (ENTSCHEIDUNGEN §8.6): the state of each chain verification method of a run, read
//! from its typed verification records only — for the Capture tab (method, status, progress,
//! provider, requests, bytes, checkpoint, block_reason) and the Analytics tab (the methods side
//! by side: four liquidation kinds, six markers, found and missing events, deviations, costs).
//! One entry per method: its spec with the newest attempt. Nothing is promoted: the status is
//! the method's own (only STRICT reaches CANONICAL_VERIFIED); an unknown count is absent, never 0.
use super::index::{CrosscheckResult, IndexResult, KINDS};
use super::parity::MARKERS;
use super::strict_driver::{AttemptRecord, VerificationSpec, NATIVE_PREFIX};
use super::verification::{read_report_with, StrictResult};
use crate::analytics::journal;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One chain verification method of a run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MethodState {
    /// off | index | strict | crosscheck.
    pub method: String,
    pub spec_id: String,
    pub attempts: u64,
    /// The newest attempt has started and not ended.
    pub running: bool,
    /// RUNNING, else the newest attempt's end status (NOT_RUN, INDEX_VERIFIED,
    /// CROSSCHECK_VERIFIED, CANONICAL_VERIFIED, HISTORICAL_INCOMPLETE).
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
    pub calls: Option<u64>,
    pub bytes: Option<u64>,
    /// `pages` (INDEX, CROSSCHECK: both sources) or `slots` (STRICT).
    pub progress_unit: &'static str,
    pub progress_done: u64,
    pub progress_total: Option<u64>,
    /// Qualified events per liquidation kind (empty: unknown).
    pub qualified: Vec<(String, u64)>,
    /// Program events per kind including unqualified transactions (empty: unknown).
    pub observed: Vec<(String, u64)>,
    /// The six markers (empty: unknown).
    pub markers: Vec<(String, u64)>,
    /// Found and missing events and deviations against the capture (INDEX/CROSSCHECK).
    pub comparison: Vec<(String, u64)>,
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
fn files(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map_or(0, |d| d.flatten().count() as u64)
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

/// STRICT's verified evidence as shown: slots done and expected (`None`: scope unresolved),
/// kinds and markers of the verified batches.
#[derive(Clone, Copy)]
struct StrictView {
    done: u64,
    total: Option<u64>,
    kinds: [u64; 4],
    markers: [u64; 6],
}

/// Memoized per verified checkpoint set: checkpoints and their batches are immutable, a new
/// batch is a new set (the evidence is read once per change, not per projection tick).
#[allow(clippy::type_complexity)]
static STRICT_VIEWS: Mutex<Option<HashMap<PathBuf, (Vec<String>, StrictView)>>> = Mutex::new(None);

fn strict_view(dir: &Path) -> StrictView {
    let mut checkpoints: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.starts_with("batch_") && n.ends_with(".p0j"))
                .collect()
        })
        .unwrap_or_default();
    checkpoints.sort();
    let mut memo = STRICT_VIEWS.lock().unwrap_or_else(|p| p.into_inner());
    let memo = memo.get_or_insert_with(HashMap::new);
    if let Some((set, view)) = memo.get(dir) {
        if *set == checkpoints {
            return *view;
        }
    }
    let program =
        solana_sdk::pubkey::Pubkey::from_str_const(darkbot_project0::protocol::PROGRAM_ID_MAINNET);
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
    let view = StrictView {
        done: report.slots_fetched + report.slots_skipped_confirmed,
        total: report.scope.as_ref().map(|_| report.slots_expected),
        kinds,
        markers: marks,
    };
    memo.insert(dir.to_path_buf(), (checkpoints, view));
    view
}

/// The state of one spec (its newest attempt and newest finished result).
fn state(s: &Spec) -> MethodState {
    let newest = s.attempts.last();
    let finished = s.attempts.iter().rev().find_map(|(_, end)| end.as_ref());
    let running = newest.is_some_and(|(_, end)| end.is_none());
    let mut m = MethodState {
        method: s.spec.mode.clone(),
        spec_id: s.id.clone(),
        attempts: s.attempts.len() as u64,
        running,
        status: match newest {
            Some((_, None)) => "RUNNING".into(),
            Some((_, Some(end))) => end.status.clone().unwrap_or_default(),
            None => "NOT_RUN".into(),
        },
        reason: newest
            .and_then(|(_, end)| end.as_ref())
            .and_then(|e| e.reason.clone()),
        result_id: finished.and_then(|e| e.verification_result_id.clone()),
        offline: newest.is_some_and(|(start, _)| start.offline),
        provider_binding: s.spec.provider_binding_sha256.map(|b| journal::hex(&b)),
        ..Default::default()
    };
    let result = |dir: &str| {
        m.result_id
            .as_ref()
            .map(|id| s.dir.join(dir).join("results").join(format!("{id}.p0j")))
    };
    match s.spec.mode.as_str() {
        "index" => {
            m.progress_unit = "pages";
            m.checkpoint_reads = files(&s.dir.join("index/cache"));
            if let Some(r) =
                result("index").and_then(|p| journal::read_typed_value::<IndexResult>(&p).ok())
            {
                index_counts(&mut m, &r);
                (m.calls, m.bytes) = (Some(r.calls), Some(r.bytes));
                m.progress_done = r.pages;
            }
        }
        "crosscheck" => {
            m.progress_unit = "pages";
            m.checkpoint_reads = files(&s.dir.join("crosscheck/primary/cache"))
                + files(&s.dir.join("crosscheck/secondary/cache"));
            if let Some(r) = result("crosscheck")
                .and_then(|p| journal::read_typed_value::<CrosscheckResult>(&p).ok())
            {
                index_counts(&mut m, &r.primary);
                m.disagreements = Some(r.disagreements.len() as u64);
                m.unresolved =
                    Some(r.primary.unresolved_transactions + r.secondary.unresolved_transactions);
                m.unknown = Some(r.primary.unknown_transactions + r.secondary.unknown_transactions);
                m.calls = Some(r.primary.calls + r.secondary.calls);
                m.bytes = Some(r.primary.bytes + r.secondary.bytes);
                m.progress_done = r.primary.pages + r.secondary.pages;
            }
        }
        "strict" => {
            m.progress_unit = "slots";
            let dir = s.dir.join("strict");
            m.checkpoint_reads = files(&dir.join("cache"));
            let view = strict_view(&dir);
            m.progress_done = view.done;
            m.progress_total = view.total;
            if view.total.is_some() {
                m.qualified = kinds(&view.kinds);
                m.markers = markers(&view.markers);
            }
            if let Some(r) =
                result("strict").and_then(|p| journal::read_typed_value::<StrictResult>(&p).ok())
            {
                (m.calls, m.bytes) = (Some(r.rpc_calls), Some(r.successful_response_bytes));
            }
        }
        _ => m.progress_unit = "none",
    }
    m
}

fn index_counts(m: &mut MethodState, r: &IndexResult) {
    m.qualified = kinds(&r.qualified);
    m.observed = kinds(&r.observed);
    m.markers = markers(&r.markers);
    m.comparison = r
        .comparison_counts
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    m.unresolved = Some(r.unresolved_transactions);
    m.unknown = Some(r.unknown_transactions);
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
