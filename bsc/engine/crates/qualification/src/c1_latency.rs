//! C1.9 native latency report (`c19_report`) from the raw evidence of
//! `bsc/tools/c19_native_latency.sh`.
//!
//! Every number is derived from raw per-sample timestamps of one clock
//! domain. No end-to-end figure is built by adding component percentiles,
//! the maximum is always reported and never replaced by a percentile, and
//! p99.9 / p99.99 are reported only when the sample count supports them.
//! Warmup rows stay in the raw evidence and are excluded from the
//! statistics; the window is the one the manifest fixed before the run. The
//! report states measurements and integrity. It does not decide the C1.9
//! gate.
//!
//! Inputs: the run manifest (`key=value`, written by the Go producer), the
//! raw ingress and outbound tables and the summary record of the Rust
//! consumer, the Go producer snapshot (binary, see [`GoSnapshot`]) and the
//! C1.6 host profile record. Output: one `darkbot.c1.native-latency.v1`
//! record and a derived text view on stdout.

use crate::c1_common::{self as common, Args, MISS_NS, P999_MIN_SAMPLES, P9999_MIN_SAMPLES};
use darkbot_bsc_geth_ipc::outbound::OutboundStatus;
use darkbot_bsc_record::schema::c1_qualification::{
    c19_rust_summary as rs, host_profile as hp, native_latency as nl, table,
};
use darkbot_bsc_record::{Draft, Value, decode, observed_now};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub const NON_CLAIMS: [&str; 6] = [
    "T0_TO_T_BUY_LT_1MS = NOT_PROVEN",
    "MAINNET_INGRESS_COVERAGE = NOT_PROVEN",
    "PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED",
    "LIVE = BLOCKED",
    "a local geth handoff is not acquisition",
    "macOS/M4 evidence does not qualify the Linux C3 target",
];
pub const PRODUCTION_MODE: &str = "production_devp2p_selected_peer_handoff";
pub const LOOPBACK_MODE: &str = "loopback_microbenchmark";
const PREPARED_BUY_SCOPE: &str = "harness path with a prepared signed BUY: scope match, one launch \
     copy, seal, egress commit. It excludes venue decode beyond the scope match, state, economics, \
     admission, build and sign, and it is not the complete local reaction.";
const HANDOFF_BOUNDARY: &str = "return of the worker's send call (harness: one write to a loopback \
     TCP socket with TCP_NODELAY); not remote receipt, inclusion or execution";

/// The rules every report states.
pub fn rules() -> Vec<String> {
    vec![
        "every statistic is over directly measured per-sample differences of one clock".into(),
        "no end-to-end figure is built by adding component percentiles".into(),
        "max is reported for every quantity; no percentile stands in for it".into(),
        format!("a miss is a value >= {MISS_NS} ns"),
        format!("p99.9 needs >= {P999_MIN_SAMPLES} samples, p99.99 >= {P9999_MIN_SAMPLES}"),
    ]
}

/// The run manifest: `key=value` lines; a later key wins.
#[derive(Clone, Debug, Default)]
pub struct Manifest(pub BTreeMap<String, String>);

impl Manifest {
    pub fn parse(text: &str) -> Self {
        let pairs = text.lines().filter_map(|line| line.split_once('='));
        Self(pairs.map(|(k, v)| (k.to_string(), v.to_string())).collect())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    fn number(&self, key: &str) -> Result<u64, String> {
        let value = self
            .get(key)
            .ok_or_else(|| format!("manifest has no {key}"))?;
        value
            .parse()
            .map_err(|_| format!("manifest {key}={value:?} is not a number"))
    }
}

// ---------------------------------------------------------------------------
// The Go producer snapshot
// ---------------------------------------------------------------------------

/// The Go producer's counters, as the harness's Go test writes them
/// (`go_snapshot.bin`, little-endian, `encoding/binary`):
///
/// ```text
/// "DBC19SNP" | u32 version (1) | u32 flags (bit 0: outbound present)
/// 15 u64 ingress | 11 u64 peer counters | 9 u64 chain | 12 u64 outbound | u64 worker count
/// worker count x 12 u64 per worker
/// SHA-256 over every preceding byte
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GoSnapshot {
    pub ingress: [u64; 15],
    pub peers: [u64; 11],
    pub chain: [u64; 9],
    pub outbound: Option<GoOutbound>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GoOutbound {
    pub counters: [u64; 12],
    pub per_worker: Vec<[u64; 12]>,
}

pub const GO_SNAPSHOT_MAGIC: [u8; 8] = *b"DBC19SNP";
pub const GO_SNAPSHOT_VERSION: u32 = 1;
/// Ingress: live_peers, live_covered, live_uncovered, covered_binds,
/// uncovered_binds, pending_release, reclaimed, release_failures,
/// close_failures, gap_total, gap_clock_errors, txpool_published,
/// txpool_refused, txpool_serialize_errors, txpool_clock_errors.
pub const GO_INGRESS_GAP_TOTAL: usize = 9;
/// Peers: packets, rejected_packets, matched, filtered, published,
/// queue_full, oversize, refused, serialize_errors, clock_errors,
/// uncovered_candidates.
pub const GO_PEERS_PUBLISHED: usize = 4;
pub const GO_PEERS_QUEUE_FULL: usize = 5;
pub const GO_PEERS_OVERSIZE: usize = 6;
pub const GO_PEERS_REFUSED: usize = 7;
pub const GO_PEERS_UNCOVERED_CANDIDATES: usize = 10;
/// Chain: lane_id, published, gap_records, lost_records,
/// lost_records_not_yet_reported, clock_errors, ownership_violations,
/// lane_dropped_full, lane_oversize.
pub const GO_CHAIN_PUBLISHED: usize = 1;
pub const GO_CHAIN_LOST: usize = 3;
pub const GO_CHAIN_OWNERSHIP_VIOLATIONS: usize = 6;
/// Outbound: workers, running, egress_records, invalid_egress, dispatched,
/// queued, queue_full, no_target, coordinator_evidence_lost,
/// egress_lane_errors, target_reassignments, clock_errors. Per worker:
/// index, evidence_lane, has_target, handed_off, send_errors_unknown,
/// no_target, already_known, evidence_lost, abandoned_at_stop, queue_depth,
/// clock_errors, clock_refused_sends.
pub const GO_OUTBOUND_WORKERS: usize = 0;
pub const GO_OUTBOUND_INVALID_EGRESS: usize = 3;
pub const GO_OUTBOUND_QUEUE_FULL: usize = 6;
pub const GO_OUTBOUND_EVIDENCE_LOST: usize = 8;
const GO_FIXED_WORDS: usize = 15 + 11 + 9 + 12 + 1;

impl GoSnapshot {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let fixed = 16 + GO_FIXED_WORDS * 8;
        if bytes.len() < fixed + 32 {
            return Err("go snapshot truncated".into());
        }
        if bytes[..8] != GO_SNAPSHOT_MAGIC {
            return Err("not a C1.9 Go snapshot".into());
        }
        let u32_at = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let version = u32_at(8);
        if version != GO_SNAPSHOT_VERSION {
            return Err(format!("go snapshot version {version} is not supported"));
        }
        let end = bytes.len() - 32;
        if common::sha256(&bytes[..end]) != bytes[end..] {
            return Err("go snapshot checksum mismatch".into());
        }
        let flags = u32_at(12);
        if flags > 1 {
            return Err(format!("go snapshot flags {flags:#x}"));
        }
        let words: Vec<u64> = bytes[16..end]
            .chunks_exact(8)
            .map(|w| u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]]))
            .collect();
        if !(end - 16).is_multiple_of(8) || words.len() < GO_FIXED_WORDS {
            return Err("go snapshot body is not whole words".into());
        }
        let take = |from: usize, to: usize| words[from..to].to_vec();
        let array = |v: Vec<u64>| -> [u64; 12] {
            let mut out = [0u64; 12];
            out.copy_from_slice(&v);
            out
        };
        let workers = words[GO_FIXED_WORDS - 1];
        let expected = usize::try_from(workers)
            .ok()
            .and_then(|w| w.checked_mul(12))
            .and_then(|w| w.checked_add(GO_FIXED_WORDS));
        if expected != Some(words.len()) {
            return Err(format!("go snapshot does not hold {workers} worker blocks"));
        }
        let mut snapshot = Self::default();
        snapshot.ingress.copy_from_slice(&take(0, 15));
        snapshot.peers.copy_from_slice(&take(15, 26));
        snapshot.chain.copy_from_slice(&take(26, 35));
        let outbound = array(take(35, 47));
        if flags == 1 {
            if outbound[GO_OUTBOUND_WORKERS] != workers {
                return Err("go snapshot worker count disagrees with its blocks".into());
            }
            let per_worker = words[GO_FIXED_WORDS..]
                .chunks_exact(12)
                .map(|block| array(block.to_vec()))
                .collect();
            snapshot.outbound = Some(GoOutbound {
                counters: outbound,
                per_worker,
            });
        } else if outbound.iter().any(|w| *w != 0) || workers != 0 {
            return Err("go snapshot carries outbound counters without the outbound flag".into());
        }
        Ok(snapshot)
    }
}

// ---------------------------------------------------------------------------
// Integrity of the ingress samples
// ---------------------------------------------------------------------------

fn phase(qualification: bool) -> &'static str {
    if qualification {
        "qualification"
    } else {
        "warmup"
    }
}

/// Checks the ingress samples against the manifest and returns the
/// qualification window (indices into `rows`, lane by lane). An unusable
/// manifest is an error; every integrity problem is a failure.
pub fn check_ingress(
    rows: &[table::IngressSample],
    manifest: &Manifest,
    failures: &mut Vec<String>,
) -> Result<Vec<usize>, String> {
    let peers = manifest.number("peers")?;
    let warmup = manifest.number("warmup_per_peer")?;
    let samples = manifest.number("samples_per_peer")?;
    if manifest.get("clock_id") != Some("1") {
        failures.push(format!(
            "clock_id {:?} is not CLOCK_MONOTONIC (1)",
            manifest.get("clock_id")
        ));
    }
    let lane_text = manifest.get("lane_ids").ok_or("manifest has no lane_ids")?;
    let lanes: Vec<u16> = lane_text
        .split(',')
        .map(|v| v.parse().map_err(|_| format!("manifest lane id {v:?}")))
        .collect::<Result<_, String>>()?;
    if lanes.len() as u64 != peers {
        failures.push("manifest lane_ids does not list one lane per peer".into());
    }
    // One entry per distinct lane, in the manifest's order.
    let mut by_lane: Vec<(u16, Vec<usize>)> = Vec::new();
    for lane in &lanes {
        if !by_lane.iter().any(|(id, _)| id == lane) {
            by_lane.push((*lane, Vec::new()));
        }
    }
    for (index, row) in rows.iter().enumerate() {
        match by_lane.iter_mut().find(|(id, _)| *id == row.lane_id) {
            Some((_, list)) => list.push(index),
            None => failures.push(format!(
                "sample {index} on lane {} outside the manifest",
                row.lane_id
            )),
        }
    }
    let per_lane = warmup.saturating_add(samples);
    let mut window = Vec::new();
    for (lane, list) in &by_lane {
        if list.len() as u64 != per_lane {
            failures.push(format!(
                "lane {lane}: {} samples, manifest fixes {per_lane}",
                list.len()
            ));
        }
        for (position, index) in list.iter().enumerate() {
            let row = &rows[*index];
            if row.lane_sequence != position as u64 {
                failures.push(format!(
                    "lane {lane}: sequence {} at position {position} (gap or reorder)",
                    row.lane_sequence
                ));
                break;
            }
            let expected = (position as u64) >= warmup;
            if row.qualification != expected {
                failures.push(format!(
                    "lane {lane} sequence {position}: phase {}, window fixes {}",
                    phase(row.qualification),
                    phase(expected)
                ));
                break;
            }
            if expected {
                window.push(*index);
            }
        }
    }
    let hashes: BTreeSet<[u8; 32]> = rows.iter().map(|row| row.tx_hash).collect();
    if hashes.len() != rows.len() {
        failures.push("duplicate transaction hash among the samples".into());
    }
    for (index, row) in rows.iter().enumerate() {
        let ordered = 0 < row.t_packet_decoded_ns
            && row.t_packet_decoded_ns <= row.t_ingress_ns
            && row.t_ingress_ns <= row.l1_pre_commit_ns
            && row.l1_pre_commit_ns <= row.l1_post_commit_ns;
        if !ordered || row.l1_pre_commit_ns > row.l2_acquire_ns {
            failures.push(format!(
                "sample {index}: boundary order violated (decoded={} ingress={} l1_pre={} \
                 l1_post={} l2={})",
                row.t_packet_decoded_ns,
                row.t_ingress_ns,
                row.l1_pre_commit_ns,
                row.l1_post_commit_ns,
                row.l2_acquire_ns
            ));
            break;
        }
    }
    Ok(window)
}

/// The ingress segments of the window, each measured per sample.
pub fn ingress_metrics(
    rows: &[table::IngressSample],
    window: &[usize],
) -> Vec<(&'static str, common::Stats)> {
    type Pick = fn(&table::IngressSample) -> i64;
    let segments: [(&'static str, Pick); 7] = [
        ("T_packet_decoded_to_T_ingress", |r| {
            common::diff(r.t_ingress_ns, r.t_packet_decoded_ns)
        }),
        ("T_ingress_to_L1_pre_commit_lower_bound", |r| {
            common::diff(r.l1_pre_commit_ns, r.t_ingress_ns)
        }),
        ("T_ingress_to_L1_post_commit_upper_bound", |r| {
            common::diff(r.l1_post_commit_ns, r.t_ingress_ns)
        }),
        ("L1_commit_bracket_width", |r| {
            common::diff(r.l1_post_commit_ns, r.l1_pre_commit_ns)
        }),
        ("L1_to_L2_lower_bound", |r| {
            common::diff(r.l2_acquire_ns, r.l1_post_commit_ns).max(0)
        }),
        ("L1_to_L2_upper_bound", |r| {
            common::diff(r.l2_acquire_ns, r.l1_pre_commit_ns)
        }),
        ("T_ingress_to_L2", |r| {
            common::diff(r.l2_acquire_ns, r.t_ingress_ns)
        }),
    ];
    segments
        .iter()
        .map(|(name, pick)| {
            let values: Vec<i64> = window.iter().map(|i| pick(&rows[*i])).collect();
            (*name, common::describe(&values))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Outbound handoffs
// ---------------------------------------------------------------------------

/// Per-BUY handoff quantities of the qualification window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutboundMetrics {
    pub enabled: bool,
    pub workers: usize,
    pub buys_in_window: usize,
    pub slot_outcomes: BTreeMap<String, u64>,
    pub buys_without_all_selected: u64,
    /// The five handoff segments, in report order.
    pub metrics: Vec<(&'static str, common::Stats)>,
    /// `T_ingress_to_egress_commit`, `T_ingress_to_first_selected_peer_handoff`.
    pub prepared: Vec<(&'static str, common::Stats)>,
}

impl OutboundMetrics {
    pub fn metric(&self, name: &str) -> Option<&common::Stats> {
        self.metrics
            .iter()
            .chain(&self.prepared)
            .find(|(n, _)| *n == name)
            .map(|(_, s)| s)
    }
}

fn status_name(code: u8) -> Option<&'static str> {
    OutboundStatus::from_raw(code).map(OutboundStatus::name)
}

pub fn outbound_metrics(
    rows: &[table::OutboundEvidence],
    ingress: &[table::IngressSample],
    window: &[usize],
    workers: usize,
    failures: &mut Vec<String>,
) -> OutboundMetrics {
    if workers == 0 {
        return OutboundMetrics::default();
    }
    let window_indices: BTreeSet<usize> = window.iter().copied().collect();
    let mut by_buy: BTreeMap<u64, Vec<&table::OutboundEvidence>> = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        if status_name(row.status).is_none() {
            failures.push(format!(
                "outbound row {index}: unknown status code {}",
                row.status
            ));
        }
        by_buy.entry(row.egress_sequence).or_default().push(row);
    }
    let dispatched = OutboundStatus::Dispatched as u8;
    let handed_off = OutboundStatus::HandedOff as u8;
    let (mut observed, mut first, mut each, mut all_selected, mut send) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut ingress_to_commit, mut ingress_to_first) = (Vec::new(), Vec::new());
    let mut outcomes: BTreeMap<String, u64> = BTreeMap::new();
    let mut partial = 0u64;
    for index in &window_indices {
        let Some(buy) = by_buy.get(&(*index as u64)) else {
            failures.push(format!("BUY {index}: no outbound evidence"));
            continue;
        };
        let summaries: Vec<_> = buy.iter().filter(|r| r.status == dispatched).collect();
        let [summary] = summaries.as_slice() else {
            failures.push(format!(
                "BUY {index}: {} DISPATCHED records",
                summaries.len()
            ));
            continue;
        };
        let slots: Vec<_> = buy.iter().filter(|r| r.status != dispatched).collect();
        let mut slot_workers: Vec<i64> = slots
            .iter()
            .map(|r| r.worker.map_or(-1, i64::from))
            .collect();
        slot_workers.sort_unstable();
        if slots.len() != workers || !slot_workers.iter().copied().eq(0..workers as i64) {
            failures.push(format!(
                "BUY {index}: evidence does not cover every worker slot exactly once"
            ));
            continue;
        }
        let commit = summary.egress_commit_ns;
        if commit > summary.coordinator_observed_ns {
            failures.push(format!(
                "BUY {index}: coordinator observed before the egress commit"
            ));
            continue;
        }
        observed.push(common::diff(summary.coordinator_observed_ns, commit));
        let handed: Vec<_> = slots.iter().filter(|r| r.status == handed_off).collect();
        for row in &slots {
            let name = status_name(row.status)
                .map_or_else(|| format!("UNKNOWN_{}", row.status), str::to_string);
            *outcomes.entry(name).or_default() += 1;
        }
        for row in &handed {
            each.push(common::diff(row.event_ns, commit));
            send.push(common::diff(row.event_ns, row.third_ns));
        }
        let launch = &ingress[*index];
        ingress_to_commit.push(common::diff(commit, launch.t_ingress_ns));
        if let Some(first_event) = handed.iter().map(|r| r.event_ns).min() {
            first.push(common::diff(first_event, commit));
            ingress_to_first.push(common::diff(first_event, launch.t_ingress_ns));
        }
        match handed.iter().map(|r| r.event_ns).max() {
            Some(last) if summary.selected > 0 && handed.len() == usize::from(summary.selected) => {
                all_selected.push(common::diff(last, commit));
            }
            _ => partial += 1,
        }
    }
    OutboundMetrics {
        enabled: true,
        workers,
        buys_in_window: window_indices.len(),
        slot_outcomes: outcomes,
        buys_without_all_selected: partial,
        metrics: vec![
            (
                "egress_commit_to_coordinator_observed",
                common::describe(&observed),
            ),
            (
                "egress_commit_to_first_selected_peer_handoff",
                common::describe(&first),
            ),
            (
                "egress_commit_to_each_selected_peer_handoff",
                common::describe(&each),
            ),
            (
                "egress_commit_to_all_selected_peer_handoffs",
                common::describe(&all_selected),
            ),
            ("worker_send_call_duration", common::describe(&send)),
        ],
        prepared: vec![
            (
                "T_ingress_to_egress_commit",
                common::describe(&ingress_to_commit),
            ),
            (
                "T_ingress_to_first_selected_peer_handoff",
                common::describe(&ingress_to_first),
            ),
        ],
    }
}

// ---------------------------------------------------------------------------
// Counters
// ---------------------------------------------------------------------------

/// The Rust consumer's summary counters used by the report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RustCounters {
    pub lane_faults: u64,
    pub invalid_records: u64,
    pub dropped: u64,
    pub chain_gaps: u64,
    pub allocations_in_window: u64,
    pub raw_ingress_sha256: [u8; 32],
    pub raw_outbound_sha256: [u8; 32],
}

impl RustCounters {
    pub fn from_record(bytes: &[u8]) -> Result<Self, String> {
        let record = decode(&rs::SCHEMA, bytes).map_err(|e| e.0)?;
        let get = |tag| record.u64(tag).map_err(|e| e.0);
        Ok(Self {
            lane_faults: get(rs::HOT0_LANE_FAULTS)?,
            invalid_records: get(rs::HOT0_INVALID_RECORDS)?,
            dropped: get(rs::HOT0_DROPPED)?,
            chain_gaps: get(rs::HOT0_CHAIN_GAPS)?,
            allocations_in_window: get(rs::HOT0_ALLOCATIONS_IN_WINDOW)?,
            raw_ingress_sha256: record.b256(rs::RAW_INGRESS_SHA256).map_err(|e| e.0)?,
            raw_outbound_sha256: record.b256(rs::RAW_OUTBOUND_SHA256).map_err(|e| e.0)?,
        })
    }
}

/// The integrity counters (absent: the producer did not report it) and the
/// failures among them.
pub fn check_counters(
    go: &GoSnapshot,
    rust: &RustCounters,
    total: usize,
    failures: &mut Vec<String>,
) -> Vec<(&'static str, Option<u64>)> {
    let outbound = |index: usize| go.outbound.as_ref().map(|o| o.counters[index]);
    let counters = vec![
        ("peer_published", Some(go.peers[GO_PEERS_PUBLISHED])),
        ("peer_queue_full", Some(go.peers[GO_PEERS_QUEUE_FULL])),
        ("peer_oversize", Some(go.peers[GO_PEERS_OVERSIZE])),
        ("peer_refused", Some(go.peers[GO_PEERS_REFUSED])),
        (
            "uncovered_candidates",
            Some(go.peers[GO_PEERS_UNCOVERED_CANDIDATES]),
        ),
        ("gap_total", Some(go.ingress[GO_INGRESS_GAP_TOTAL])),
        ("chain_published", Some(go.chain[GO_CHAIN_PUBLISHED])),
        ("chain_lost", Some(go.chain[GO_CHAIN_LOST])),
        (
            "chain_ownership_violations",
            Some(go.chain[GO_CHAIN_OWNERSHIP_VIOLATIONS]),
        ),
        ("outbound_queue_full_hol", outbound(GO_OUTBOUND_QUEUE_FULL)),
        (
            "outbound_invalid_egress",
            outbound(GO_OUTBOUND_INVALID_EGRESS),
        ),
        (
            "outbound_evidence_lost",
            outbound(GO_OUTBOUND_EVIDENCE_LOST),
        ),
        ("hot0_lane_faults", Some(rust.lane_faults)),
        ("hot0_invalid_records", Some(rust.invalid_records)),
        ("hot0_dropped", Some(rust.dropped)),
        ("hot0_chain_gaps", Some(rust.chain_gaps)),
        (
            "hot0_allocations_in_qualification_window",
            Some(rust.allocations_in_window),
        ),
    ];
    if go.peers[GO_PEERS_PUBLISHED] != total as u64 {
        failures.push(format!(
            "geth published {} peer records, samples {total}",
            go.peers[GO_PEERS_PUBLISHED]
        ));
    }
    let must_be_zero = [
        "peer_queue_full",
        "peer_oversize",
        "peer_refused",
        "uncovered_candidates",
        "gap_total",
        "chain_lost",
        "chain_ownership_violations",
        "hot0_lane_faults",
        "hot0_invalid_records",
        "hot0_dropped",
        "hot0_chain_gaps",
    ];
    for (name, value) in &counters {
        if must_be_zero.contains(name) && *value != Some(0) {
            let shown = value.map_or_else(|| "absent".to_string(), |v| v.to_string());
            failures.push(format!("{name} = {shown}, must be 0"));
        }
    }
    counters
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

/// The files a report is built from.
#[derive(Clone, Debug)]
pub struct Inputs {
    pub raw_ingress: PathBuf,
    pub raw_outbound: PathBuf,
    pub manifest: PathBuf,
    pub go_snapshot: PathBuf,
    pub rust_summary: PathBuf,
    pub host_profile: PathBuf,
    pub repo: PathBuf,
    pub geth: PathBuf,
    pub go_bin: PathBuf,
    pub rust_bin: PathBuf,
}

/// Identity: commits, binaries and every input, by SHA-256.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Identity {
    pub darkbot_commit: String,
    pub darkbot_tree: String,
    pub pinned_geth_commit: String,
    /// (record tag, SHA-256), in tag order.
    pub hashes: Vec<(u16, [u8; 32])>,
    pub topology_fingerprint_crc32c: u64,
    pub profile_selection: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub outbound_mode: String,
    pub purpose: String,
    pub production_path_present: bool,
    pub eligible: bool,
    pub failures: Vec<String>,
    pub clock_id: Option<String>,
    pub peers: u64,
    pub warmup_per_peer: u64,
    pub samples_per_peer: u64,
    pub warmup_retained: u64,
    pub qualification_samples: u64,
    pub load_model: Option<String>,
    pub ingress: Vec<(&'static str, common::Stats)>,
    pub queue_depth_max: u64,
    pub outbound: OutboundMetrics,
    pub counters: Vec<(&'static str, Option<u64>)>,
    pub identity: Identity,
    pub observed_unix_ms: u64,
}

fn git_tree(repo: &Path) -> String {
    match common::git(repo, &["status", "--porcelain", "--untracked-files=no"]) {
        Ok(out) if out.iter().all(u8::is_ascii_whitespace) => "clean".into(),
        Ok(_) => "dirty".into(),
        Err(_) => "unavailable".into(),
    }
}

fn hash(path: &Path) -> Result<[u8; 32], String> {
    common::sha256_file(path).map_err(|e| format!("cannot hash {}: {e}", path.display()))
}

fn identity(inputs: &Inputs, host: &[u8]) -> Result<Identity, String> {
    let repo = &inputs.repo;
    let tools = repo.join("bsc/tools");
    let tree = |path: PathBuf, suffix: &str| {
        common::sha256_tree(&path, suffix)
            .map_err(|e| format!("cannot hash {}: {e}", path.display()))
    };
    let profile = decode(&hp::SCHEMA, host).map_err(|e| format!("host profile: {e}"))?;
    let selection = match (
        profile.opt_text(hp::SELECTION_PROFILE).map_err(|e| e.0)?,
        profile.opt_text(hp::SELECTION_SOURCE).map_err(|e| e.0)?,
        profile.opt_text(hp::SELECTION_ERROR).map_err(|e| e.0)?,
    ) {
        (Some(profile), Some(source), None) => format!("{profile} ({source})"),
        (None, None, Some(error)) => format!("refused: {error}"),
        _ => return Err("host profile: profile selection is inconsistent".into()),
    };
    let hashes = vec![
        (nl::ID_GO_TEST_BINARY_SHA256, hash(&inputs.go_bin)?),
        (nl::ID_RUST_CONSUMER_BINARY_SHA256, hash(&inputs.rust_bin)?),
        (
            nl::ID_GO_OVERLAY_SOURCES_SHA256,
            tree(repo.join("bsc/geth/overlay/darkbot/fastlane"), ".go")?,
        ),
        (
            nl::ID_RUST_FASTLANE_SOURCES_SHA256,
            tree(repo.join("bsc/engine/crates/geth_ipc"), ".rs")?,
        ),
        (
            nl::ID_OVERLAY_TOOL_SHA256,
            hash(&tools.join("c1_apply_geth_overlay.py"))?,
        ),
        (
            nl::ID_HARNESS_SCRIPT_SHA256,
            hash(&tools.join("c19_native_latency.sh"))?,
        ),
        (nl::ID_ANALYZER_SHA256, common::own_sha256()?),
        (
            nl::ID_PRODUCTION_BRIDGE_SHA256,
            hash(&tools.join("c19_prepare_production.py"))?,
        ),
        (
            nl::ID_PRODUCTION_PEER_HARNESS_SHA256,
            hash(&tools.join("c19_production_peer.go.txt"))?,
        ),
        (nl::ID_CONFIG_MANIFEST_SHA256, hash(&inputs.manifest)?),
        (nl::ID_HOST_PROFILE_SHA256, common::sha256(host)),
        (nl::ID_RAW_INGRESS_SHA256, hash(&inputs.raw_ingress)?),
        (nl::ID_RAW_OUTBOUND_SHA256, hash(&inputs.raw_outbound)?),
        (nl::ID_GO_SNAPSHOT_SHA256, hash(&inputs.go_snapshot)?),
        (nl::ID_RUST_SUMMARY_SHA256, hash(&inputs.rust_summary)?),
    ];
    Ok(Identity {
        darkbot_commit: common::git_head(repo).unwrap_or_else(|| "unavailable".into()),
        darkbot_tree: git_tree(repo),
        pinned_geth_commit: common::git_head(&inputs.geth).unwrap_or_else(|| "unavailable".into()),
        hashes,
        topology_fingerprint_crc32c: profile
            .u64(hp::TOPOLOGY_FINGERPRINT_CRC32C)
            .map_err(|e| e.0)?,
        profile_selection: selection,
    })
}

/// Builds the report. An unreadable or malformed input is an error (no
/// report); every integrity problem of readable inputs is a failure in it.
pub fn build_report(inputs: &Inputs) -> Result<Report, String> {
    let manifest = Manifest::parse(
        &String::from_utf8(common::read(&inputs.manifest)?)
            .map_err(|_| "manifest is not UTF-8".to_string())?,
    );
    let ingress_bytes = common::read(&inputs.raw_ingress)?;
    let outbound_bytes = common::read(&inputs.raw_outbound)?;
    let ingress = table::read_all(
        &table::C19_INGRESS,
        &ingress_bytes,
        table::IngressSample::read,
    )
    .map_err(|e| format!("{}: {e}", inputs.raw_ingress.display()))?;
    let outbound = table::read_all(
        &table::C19_OUTBOUND,
        &outbound_bytes,
        table::OutboundEvidence::read,
    )
    .map_err(|e| format!("{}: {e}", inputs.raw_outbound.display()))?;
    let go = GoSnapshot::decode(&common::read(&inputs.go_snapshot)?)?;
    let rust = RustCounters::from_record(&common::read(&inputs.rust_summary)?)
        .map_err(|e| format!("rust summary: {e}"))?;
    let host = common::read(&inputs.host_profile)?;
    let identity = identity(inputs, &host)?;

    let mut failures = Vec::new();
    if rust.raw_ingress_sha256 != common::sha256(&ingress_bytes) {
        failures.push("the Rust summary does not bind this raw ingress table".into());
    }
    if rust.raw_outbound_sha256 != common::sha256(&outbound_bytes) {
        failures.push("the Rust summary does not bind this raw outbound table".into());
    }
    let window = check_ingress(&ingress, &manifest, &mut failures)?;
    let counters = check_counters(&go, &rust, ingress.len(), &mut failures);
    let workers = match manifest.get("outbound_workers") {
        None => 0,
        Some(_) => usize::try_from(manifest.number("outbound_workers")?)
            .map_err(|_| "outbound_workers is too large".to_string())?,
    };
    let outbound = outbound_metrics(&outbound, &ingress, &window, workers, &mut failures);
    let mode = manifest.get("outbound_mode").unwrap_or("unclassified");
    let production = mode == PRODUCTION_MODE && workers > 0;
    if mode != PRODUCTION_MODE && mode != LOOPBACK_MODE {
        failures.push("missing or unknown outbound measurement mode".into());
    }
    let purpose = manifest.get("purpose").unwrap_or("unclassified");
    Ok(Report {
        outbound_mode: mode.to_string(),
        purpose: purpose.to_string(),
        production_path_present: production,
        eligible: production && failures.is_empty() && purpose == "qualification",
        clock_id: manifest.get("clock_id").map(str::to_string),
        peers: manifest.number("peers")?,
        warmup_per_peer: manifest.number("warmup_per_peer")?,
        samples_per_peer: manifest.number("samples_per_peer")?,
        warmup_retained: ingress.iter().filter(|r| !r.qualification).count() as u64,
        qualification_samples: window.len() as u64,
        load_model: manifest.get("load_model").map(str::to_string),
        ingress: ingress_metrics(&ingress, &window),
        queue_depth_max: window
            .iter()
            .map(|i| ingress[*i].queue_depth)
            .max()
            .unwrap_or(0),
        outbound,
        counters,
        identity,
        observed_unix_ms: observed_now()?,
        failures,
    })
}

impl Report {
    pub fn integrity(&self) -> &'static str {
        if self.failures.is_empty() {
            "PASS"
        } else {
            "FAIL"
        }
    }

    pub fn to_draft(&self) -> Draft {
        let mut d = Draft::new(&nl::SCHEMA);
        d.set(nl::OUTBOUND_MODE, self.outbound_mode.as_str())
            .set(nl::PRODUCTION_PATH_PRESENT, self.production_path_present)
            .set(nl::PROVENANCE_TOOL, "darkbot-bsc-qualification c19_report")
            .set(nl::PROVENANCE_OBSERVED_UNIX_MS, self.observed_unix_ms)
            .set(nl::PURPOSE, self.purpose.as_str())
            .set(nl::QUALIFICATION_ELIGIBLE, self.eligible)
            .set(nl::RESULT, "MEASURED")
            .set(nl::INTEGRITY, self.integrity())
            .set(nl::GATE, "NOT_DECIDED_BY_THIS_TOOL")
            .set_opt(nl::CLOCK_ID, self.clock_id.as_deref())
            .set(
                nl::CLOCK_DOMAIN,
                "CLOCK_MONOTONIC of one host; geth (Go) and Rust read the same clock",
            )
            .set(nl::WINDOW_FIXED_BEFORE_RUN, true)
            .set(nl::WINDOW_PEERS, self.peers)
            .set(nl::WINDOW_WARMUP_PER_PEER, self.warmup_per_peer)
            .set(nl::WINDOW_SAMPLES_PER_PEER, self.samples_per_peer)
            .set(nl::WINDOW_WARMUP_RETAINED, self.warmup_retained)
            .set(nl::WINDOW_QUALIFICATION_SAMPLES, self.qualification_samples)
            .set_opt(nl::WINDOW_LOAD_MODEL, self.load_model.as_deref())
            .set(nl::INGRESS_QUEUE_DEPTH_MAX, self.queue_depth_max)
            .set(nl::OUTBOUND_ENABLED, self.outbound.enabled)
            .set(
                nl::COUNTERS_HOT0_LOCKS,
                "none in the receive path by construction; not instrumented",
            )
            .set(nl::ID_DARKBOT_COMMIT, self.identity.darkbot_commit.as_str())
            .set(nl::ID_DARKBOT_TREE, self.identity.darkbot_tree.as_str())
            .set(
                nl::ID_PINNED_GETH_COMMIT,
                self.identity.pinned_geth_commit.as_str(),
            )
            .set(
                nl::ID_TOPOLOGY_FINGERPRINT_CRC32C,
                self.identity.topology_fingerprint_crc32c,
            )
            .set(
                nl::ID_PROFILE_SELECTION,
                self.identity.profile_selection.as_str(),
            );
        for failure in &self.failures {
            d.push(nl::INTEGRITY_FAILURES, failure.as_str());
        }
        for line in [
            "pre_commit: clock read after the slot commit, before write_sequence Release",
            "post_commit: clock read after write_sequence Release",
            "note: L1 lies inside this bracket; T_ingress -> L2 is measured directly",
        ] {
            d.push(nl::L1_SEMANTICS, line);
        }
        for (name, stats) in &self.ingress {
            d.push(nl::INGRESS_METRICS, stats.to_value(name));
        }
        if self.outbound.enabled {
            d.set(nl::OUTBOUND_WORKERS, self.outbound.workers as u64)
                .set(
                    nl::OUTBOUND_BUYS_IN_WINDOW,
                    self.outbound.buys_in_window as u64,
                )
                .set(
                    nl::OUTBOUND_BUYS_WITHOUT_ALL_SELECTED,
                    self.outbound.buys_without_all_selected,
                )
                .set(nl::PREPARED_BUY_PATH_SCOPE, PREPARED_BUY_SCOPE)
                .set(nl::OUTBOUND_HANDOFF_BOUNDARY, HANDOFF_BOUNDARY);
            for (status, count) in &self.outbound.slot_outcomes {
                let mut outcome = Draft::new(&nl::slot_outcome::SCHEMA);
                outcome
                    .set(nl::slot_outcome::STATUS, status.as_str())
                    .set(nl::slot_outcome::COUNT, *count);
                d.push(nl::OUTBOUND_SLOT_OUTCOMES, outcome.into_nested());
            }
            for (name, stats) in &self.outbound.metrics {
                d.push(nl::OUTBOUND_METRICS, stats.to_value(name));
            }
            for (name, stats) in &self.outbound.prepared {
                d.push(nl::PREPARED_BUY_PATH_METRICS, stats.to_value(name));
            }
        }
        for (name, value) in &self.counters {
            let mut counter = Draft::new(&nl::counter::SCHEMA);
            counter
                .set(nl::counter::NAME, *name)
                .set_opt(nl::counter::VALUE, *value);
            d.push(nl::COUNTERS, counter.into_nested());
        }
        for (tag, digest) in &self.identity.hashes {
            d.set(*tag, Value::B256(*digest));
        }
        for rule in rules() {
            d.push(nl::RULES, rule);
        }
        for claim in NON_CLAIMS {
            d.push(nl::NON_CLAIMS, claim);
        }
        d
    }

    /// The derived text view.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "C1.9 native latency: integrity={} eligible={} mode={} purpose={} samples={} \
             (warmup retained {})",
            self.integrity(),
            self.eligible,
            self.outbound_mode,
            self.purpose,
            self.qualification_samples,
            self.warmup_retained
        );
        for (name, stats) in &self.ingress {
            let _ = writeln!(out, "  {}", stats.line(name));
        }
        let _ = writeln!(out, "  ingress queue_depth_max={}", self.queue_depth_max);
        if self.outbound.enabled {
            for (name, stats) in self.outbound.metrics.iter().chain(&self.outbound.prepared) {
                let _ = writeln!(out, "  {}", stats.line(name));
            }
            let outcomes: Vec<String> = self
                .outbound
                .slot_outcomes
                .iter()
                .map(|(status, count)| format!("{status}={count}"))
                .collect();
            let _ = writeln!(
                out,
                "  worker slots: {} buys_without_all_selected={}",
                outcomes.join(" "),
                self.outbound.buys_without_all_selected
            );
        }
        let counters: Vec<String> = self
            .counters
            .iter()
            .map(|(name, value)| {
                format!(
                    "{name}={}",
                    value.map_or_else(|| "absent".to_string(), |v| v.to_string())
                )
            })
            .collect();
        let _ = writeln!(out, "  counters: {}", counters.join(" "));
        let _ = writeln!(
            out,
            "  identity: darkbot {} ({}) geth {} topology {:#010x} profile {}",
            self.identity.darkbot_commit,
            self.identity.darkbot_tree,
            self.identity.pinned_geth_commit,
            self.identity.topology_fingerprint_crc32c,
            self.identity.profile_selection
        );
        let _ = writeln!(out, "  gate: NOT_DECIDED_BY_THIS_TOOL");
        out
    }
}

const USAGE: &str = "usage: c19_report --raw-ingress T --raw-outbound T --manifest F \
     --go-snapshot F --rust-summary R --host-profile R --repo DIR --geth DIR --go-bin F \
     --rust-bin F --output R";

/// The command line. Exit 0: report written, integrity PASS; 1: written,
/// integrity FAIL; 2: no report (unusable inputs); 64: usage.
pub fn main_with(args: Args) -> u8 {
    let names = [
        "raw-ingress",
        "raw-outbound",
        "manifest",
        "go-snapshot",
        "rust-summary",
        "host-profile",
        "repo",
        "geth",
        "go-bin",
        "rust-bin",
        "output",
    ];
    let parsed = (|| {
        args.only(&names)?;
        if !args.positional.is_empty() {
            return Err("unexpected positional argument".to_string());
        }
        let path = |name: &str| args.one(name).map(PathBuf::from);
        Ok((
            Inputs {
                raw_ingress: path("raw-ingress")?,
                raw_outbound: path("raw-outbound")?,
                manifest: path("manifest")?,
                go_snapshot: path("go-snapshot")?,
                rust_summary: path("rust-summary")?,
                host_profile: path("host-profile")?,
                repo: path("repo")?,
                geth: path("geth")?,
                go_bin: path("go-bin")?,
                rust_bin: path("rust-bin")?,
            },
            path("output")?,
        ))
    })();
    let (inputs, output) = match parsed {
        Ok(parsed) => parsed,
        Err(problem) => {
            eprintln!("c19_report: {problem}\n{USAGE}");
            return 64;
        }
    };
    let report = match build_report(&inputs) {
        Ok(report) => report,
        Err(problem) => {
            eprintln!("C19_REPORT_REFUSED {problem}");
            return 2;
        }
    };
    let bytes = match report.to_draft().encode() {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("C19_REPORT_REFUSED cannot encode the report: {e}");
            return 2;
        }
    };
    if let Err(problem) = common::write_new(&output, &bytes) {
        eprintln!("C19_REPORT_REFUSED {problem}");
        return 2;
    }
    print!("{}", report.summary());
    println!(
        "report={} sha256={}",
        output.display(),
        common::hex(&common::sha256(&bytes))
    );
    for failure in &report.failures {
        eprintln!("C19_INTEGRITY_FAIL {failure}");
    }
    u8::from(!report.failures.is_empty())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // Ported from bsc/tools/test_c1_qualification_tools.py
    // (IngressIntegrityTest, OutboundMetricsTest); synthetic inputs only.

    /// A consistent run: lanes 16.., one row per sample, in file order.
    pub(crate) fn ingress_rows(
        peers: u16,
        warmup: u64,
        samples: u64,
        l2_gap: u64,
    ) -> Vec<table::IngressSample> {
        let mut rows = Vec::new();
        let mut index = 0u64;
        for sample in 0..warmup + samples {
            for peer in 0..peers {
                let base = 1_000_000 + index * 100_000;
                let mut tx_hash = [0u8; 32];
                tx_hash[24..].copy_from_slice(&index.to_be_bytes());
                rows.push(table::IngressSample {
                    qualification: sample >= warmup,
                    lane_id: 16 + peer,
                    lane_sequence: sample,
                    tx_hash,
                    t_packet_decoded_ns: base,
                    t_ingress_ns: base + 1_000,
                    l1_pre_commit_ns: base + 3_000,
                    l1_post_commit_ns: base + 3_500,
                    l2_acquire_ns: base + 1_000 + l2_gap,
                    queue_depth: 1,
                });
                index += 1;
            }
        }
        rows
    }

    fn rows() -> Vec<table::IngressSample> {
        ingress_rows(2, 2, 3, 7_000)
    }

    fn manifest() -> Manifest {
        Manifest::parse(
            "peers=2\nwarmup_per_peer=2\nsamples_per_peer=3\nlane_ids=16,17\nclock_id=1\n\
             outbound_workers=2\n",
        )
    }

    fn check(rows: &[table::IngressSample], manifest: &Manifest) -> (Vec<usize>, Vec<String>) {
        let mut failures = Vec::new();
        let window = check_ingress(rows, manifest, &mut failures).unwrap();
        (window, failures)
    }

    fn outbound_rows(
        ingress: &[table::IngressSample],
        workers: usize,
        statuses: &BTreeMap<usize, Vec<OutboundStatus>>,
    ) -> Vec<table::OutboundEvidence> {
        let mut rows = Vec::new();
        for (index, row) in ingress.iter().enumerate() {
            let commit = row.l2_acquire_ns + 500;
            let slots = statuses
                .get(&index)
                .cloned()
                .unwrap_or_else(|| vec![OutboundStatus::HandedOff; workers]);
            let handed = slots
                .iter()
                .filter(|s| **s == OutboundStatus::HandedOff)
                .count() as u8;
            let base = table::OutboundEvidence {
                tx_hash: [0xbb; 32],
                peer: [0; 32],
                egress_sequence: index as u64,
                egress_commit_ns: commit,
                coordinator_observed_ns: commit + 2_000,
                third_ns: commit,
                event_ns: commit + 2_000,
                evidence_lane_sequence: 0,
                evidence_lane_id: 0,
                from_worker: false,
                status: OutboundStatus::Dispatched as u8,
                worker: None,
                selected: handed,
                queued: handed,
            };
            rows.push(base);
            for (worker, status) in slots.iter().enumerate() {
                let done = commit + 10_000 * (worker as u64 + 1);
                let mut peer = [0u8; 32];
                peer[31] = worker as u8;
                rows.push(table::OutboundEvidence {
                    from_worker: true,
                    status: *status as u8,
                    worker: Some(worker as u8),
                    third_ns: done - 4_000,
                    event_ns: done,
                    peer,
                    ..base
                });
            }
        }
        rows
    }

    fn metrics(
        statuses: BTreeMap<usize, Vec<OutboundStatus>>,
        keep: impl Fn(&table::OutboundEvidence) -> bool,
    ) -> (OutboundMetrics, Vec<String>) {
        let ingress = rows();
        let mut failures = Vec::new();
        let window = check_ingress(&ingress, &manifest(), &mut failures).unwrap();
        let outbound: Vec<_> = outbound_rows(&ingress, 2, &statuses)
            .into_iter()
            .filter(|r| keep(r))
            .collect();
        let metrics = outbound_metrics(&outbound, &ingress, &window, 2, &mut failures);
        (metrics, failures)
    }

    #[test]
    fn a_consistent_run_yields_the_fixed_window() {
        let rows = rows();
        let (window, failures) = check(&rows, &manifest());
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(window.len(), 6);
        assert!(window.iter().all(|i| rows[*i].qualification));
    }

    #[test]
    fn a_gap_a_moved_window_and_a_reordered_boundary_fail() {
        let has = |rows: &[table::IngressSample], needle: &str| {
            check(rows, &manifest())
                .1
                .iter()
                .any(|f| f.contains(needle))
        };
        let mut gap = rows();
        gap[4].lane_sequence = 9;
        assert!(has(&gap, "gap or reorder"));
        let mut moved = rows();
        moved[2].qualification = true;
        assert!(has(&moved, "window fixes warmup"));
        let mut late = rows();
        late[3].t_packet_decoded_ns = late[3].t_ingress_ns + 1;
        assert!(has(&late, "boundary order"));
        let missing = &rows()[..9];
        assert!(has(missing, "manifest fixes"));
        let mut duplicate = rows();
        duplicate[1].tx_hash = duplicate[0].tx_hash;
        assert!(has(&duplicate, "duplicate"));
        let mut other_clock = manifest();
        other_clock.0.insert("clock_id".into(), "2".into());
        let (_, failures) = check(&rows(), &other_clock);
        assert!(failures.iter().any(|f| f.contains("CLOCK_MONOTONIC")));
        // An unusable manifest is no report at all.
        let mut broken = manifest();
        broken.0.remove("peers");
        assert!(check_ingress(&rows(), &broken, &mut Vec::new()).is_err());
    }

    #[test]
    fn first_each_and_all_handoffs_are_separate_measurements() {
        let (metrics, failures) = metrics(BTreeMap::new(), |_| true);
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(metrics.buys_in_window, 6);
        let first = metrics
            .metric("egress_commit_to_first_selected_peer_handoff")
            .unwrap();
        let each = metrics
            .metric("egress_commit_to_each_selected_peer_handoff")
            .unwrap();
        let everyone = metrics
            .metric("egress_commit_to_all_selected_peer_handoffs")
            .unwrap();
        assert_eq!((first.samples, first.max_ns), (6, Some(10_000)));
        assert_eq!((each.samples, each.max_ns), (12, Some(20_000)));
        assert_eq!((everyone.samples, everyone.min_ns), (6, Some(20_000)));
        let send = metrics.metric("worker_send_call_duration").unwrap();
        assert_eq!(send.max_ns, Some(4_000));
        // T_ingress -> first handoff is measured per sample, not summed.
        let direct = metrics
            .metric("T_ingress_to_first_selected_peer_handoff")
            .unwrap();
        assert_eq!(direct.max_ns, Some(7_000 + 500 + 10_000));
    }

    #[test]
    fn a_failed_peer_is_a_partial_handoff_not_a_missing_sample() {
        let failed = BTreeMap::from([(
            4,
            vec![OutboundStatus::SendErrorUnknown, OutboundStatus::HandedOff],
        )]);
        let (metrics, failures) = metrics(failed, |_| true);
        assert!(failures.is_empty(), "{failures:?}");
        let outcomes: Vec<_> = metrics
            .slot_outcomes
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        assert_eq!(outcomes, [("HANDED_OFF", 11), ("SEND_ERROR_UNKNOWN", 1)]);
        assert_eq!(metrics.buys_without_all_selected, 0);
        let first = metrics
            .metric("egress_commit_to_first_selected_peer_handoff")
            .unwrap();
        assert_eq!(first.max_ns, Some(20_000));
    }

    #[test]
    fn missing_evidence_is_an_integrity_failure() {
        let (_, failures) = metrics(BTreeMap::new(), |r| {
            !(r.egress_sequence == 5 && r.worker == Some(1))
        });
        assert!(failures.iter().any(|f| f.contains("every worker slot")));
        let (_, failures) = metrics(BTreeMap::new(), |r| r.egress_sequence != 6);
        assert!(failures.iter().any(|f| f.contains("no outbound evidence")));
    }

    #[test]
    fn the_go_snapshot_is_read_strictly() {
        let mut body = Vec::new();
        body.extend_from_slice(&GO_SNAPSHOT_MAGIC);
        body.extend_from_slice(&1u32.to_le_bytes());
        body.extend_from_slice(&1u32.to_le_bytes());
        let mut words = vec![0u64; GO_FIXED_WORDS];
        words[15 + GO_PEERS_PUBLISHED] = 10;
        words[35 + GO_OUTBOUND_WORKERS] = 1;
        words[GO_FIXED_WORDS - 1] = 1;
        words.extend([7u64; 12]);
        for word in &words {
            body.extend_from_slice(&word.to_le_bytes());
        }
        let mut file = body.clone();
        file.extend_from_slice(&common::sha256(&body));
        let snapshot = GoSnapshot::decode(&file).unwrap();
        assert_eq!(snapshot.peers[GO_PEERS_PUBLISHED], 10);
        assert_eq!(
            snapshot.outbound.as_ref().unwrap().per_worker,
            vec![[7u64; 12]]
        );
        let mut flipped = file.clone();
        flipped[20] ^= 1;
        assert!(
            GoSnapshot::decode(&flipped)
                .unwrap_err()
                .contains("checksum")
        );
        assert!(GoSnapshot::decode(&file[..file.len() - 8]).is_err());

        // The counters a report checks: published vs samples, the zeros.
        let rust = RustCounters::default();
        let mut failures = Vec::new();
        let counters = check_counters(&snapshot, &rust, 10, &mut failures);
        assert!(failures.is_empty(), "{failures:?}");
        assert_eq!(counters[0], ("peer_published", Some(10)));
        let mut failures = Vec::new();
        let no_outbound = GoSnapshot {
            outbound: None,
            ..snapshot.clone()
        };
        let counters = check_counters(
            &no_outbound,
            &RustCounters { dropped: 1, ..rust },
            9,
            &mut failures,
        );
        assert!(counters.contains(&("outbound_queue_full_hol", None)));
        assert!(
            failures
                .iter()
                .any(|f| f.contains("geth published 10 peer records, samples 9"))
        );
        assert!(failures.iter().any(|f| f == "hot0_dropped = 1, must be 0"));
    }

    /// A report with every identity hash set (synthetic values).
    fn fixture(mode: &str, eligible: bool) -> Report {
        let tags = [
            nl::ID_GO_TEST_BINARY_SHA256,
            nl::ID_RUST_CONSUMER_BINARY_SHA256,
            nl::ID_GO_OVERLAY_SOURCES_SHA256,
            nl::ID_RUST_FASTLANE_SOURCES_SHA256,
            nl::ID_OVERLAY_TOOL_SHA256,
            nl::ID_HARNESS_SCRIPT_SHA256,
            nl::ID_ANALYZER_SHA256,
            nl::ID_PRODUCTION_BRIDGE_SHA256,
            nl::ID_PRODUCTION_PEER_HARNESS_SHA256,
            nl::ID_CONFIG_MANIFEST_SHA256,
            nl::ID_HOST_PROFILE_SHA256,
            nl::ID_RAW_INGRESS_SHA256,
            nl::ID_RAW_OUTBOUND_SHA256,
            nl::ID_GO_SNAPSHOT_SHA256,
            nl::ID_RUST_SUMMARY_SHA256,
        ];
        Report {
            outbound_mode: mode.into(),
            purpose: "qualification".into(),
            production_path_present: mode == PRODUCTION_MODE,
            eligible,
            identity: Identity {
                hashes: tags.iter().map(|tag| (*tag, [1; 32])).collect(),
                ..Identity::default()
            },
            ..Report::default()
        }
    }

    /// A report as `c1_freeze` sees it: integrity, mode and eligibility.
    pub(crate) fn report_bytes(mode: &str, eligible: bool) -> Vec<u8> {
        fixture(mode, eligible).to_draft().encode().unwrap()
    }

    #[test]
    fn the_report_record_is_typed_and_states_its_rules() {
        use darkbot_bsc_record::schema::c1_qualification::latency_stats as ls;
        let rows = rows();
        let mut failures = Vec::new();
        let window = check_ingress(&rows, &manifest(), &mut failures).unwrap();
        let report = Report {
            ingress: ingress_metrics(&rows, &window),
            ..fixture(PRODUCTION_MODE, true)
        };
        let bytes = report.to_draft().encode().unwrap();
        let record = decode(&nl::SCHEMA, &bytes).unwrap();
        assert_eq!(record.text(nl::INTEGRITY).unwrap(), "PASS");
        assert!(record.bool(nl::QUALIFICATION_ELIGIBLE).unwrap());
        assert_eq!(record.texts(nl::NON_CLAIMS).unwrap(), NON_CLAIMS);
        assert_eq!(record.texts(nl::RULES).unwrap().len(), 5);
        let metrics = record.nested_all(nl::INGRESS_METRICS).unwrap();
        assert_eq!(metrics.len(), 7);
        let l2 = &metrics[6];
        assert_eq!(l2.text(ls::SEGMENT).unwrap(), "T_ingress_to_L2");
        assert_eq!(l2.i64(ls::MAX_NS).unwrap(), 7_000);
        assert_eq!(l2.opt_i64(ls::P99_9_NS).unwrap(), None);
        assert!(
            l2.text(ls::P99_9_NOTE)
                .unwrap()
                .contains("6 samples < 1000")
        );
    }
}
