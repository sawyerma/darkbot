//! The G0.4/V10-8 market census: a replay of the runtime journals
//! (`runtime_journal`) into one `darkbot.bsc.g04-census.v1` record.
//!
//! Every number comes from verified journal frames. A journal whose frames
//! end in a partial one (still written, or cut) counts up to its last
//! verified frame and says `TAIL_INCOMPLETE`; one without the clean end
//! record (`X`) says `UNTERMINATED` (killed or crashed) and its covered
//! window ends at its last checkpoint; a corrupt frame makes the whole
//! journal `INVALID` and nothing of it counts. A journal is one file of an
//! older build (`<started_ms>.dbjnl`) or one segmented run directory
//! (`<started_ms>/`, read across its segments as one sequence); a run with
//! a clean end but no run manifest says `UNSEALED`, and one whose sealed
//! segments differ from its manifest (count, names, records, sequence
//! range, SHA-256) is `INVALID`. The journal the running
//! runtime still writes is `RUNNING`: its launches not joined yet are
//! `pending_join`, not lost. Lost records (gap spans), skipped canonical
//! blocks (gap/resync ranges, and the heights between two process windows),
//! launches received but never joined (`P` without `C`/`O`) and canonical
//! records that do not parse are counted, never filled in.
//!
//! "Before the block" is the join's: the runtime's wall-clock estimate of
//! the first sighting against the header's millisecond block time. The
//! header fetch lag of the launch blocks bounds the local clock's offset; a
//! negative minimum sets `clock_suspect`.
//!
//! A launch counts in `outage_launches` when no source was connected at the
//! moment the follower had its block (the canonical record's time); it still
//! counts in the denominator and its class. Percentiles use the nearest rank
//! and are reported only when the sample count reaches `1 / (1 - q)`. No
//! verdict: `criteria` stays `CRITERIA_UNCONFIRMED` until the user confirmed
//! versioned criteria (order V2 §8).

use crate::canonical_join::FourToken;
use crate::canonical_join::Venue;
use crate::runtime_journal::{
    CANONICAL_LAUNCH_LEN, CANONICAL_LAUNCH_LEN_V1, CANONICAL_LAUNCH_LEN_V2, RUN_MANIFEST,
    four_token_of_byte, venue_of_byte,
};
use crate::sources::SourceId;
use darkbot_bsc_journal::storage::run::{RunReader, SegmentRead};
use darkbot_bsc_journal::{CaptureRecord, JournalError, JournalReader, RecordKind};
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_capture::capture_run as cr;
use darkbot_bsc_record::schema::bsc_census::census as cs;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub const CRITERIA_UNCONFIRMED: &str = "CRITERIA_UNCONFIRMED";

/// One journal's part of the census.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub journal: String,
    pub started_unix_ms: u64,
    pub duration_ms: u64,
    pub first_block: Option<u64>,
    pub last_block: Option<u64>,
    pub reorgs: u64,
    pub resyncs: u64,
    pub lost_records: u64,
    pub blocks_skipped: u64,
    pub unjoined: u64,
    /// The running journal's launches still inside their join delay.
    pub pending_join: u64,
    /// Canonical/reorg records that do not parse (an older layout, damage).
    pub unparsed_records: u64,
    /// Phase E review L3: Four.meme drift guard probes that found the live
    /// deployment drifted from the freeze, and those that found it as
    /// frozen (Paket 8 review L1: a window without one is unverified).
    pub four_drift_probes: u64,
    pub four_match_probes: u64,
    /// Pending events the ingress queue dropped before this journal.
    pub ingress_dropped: u64,
    /// The capture window: from the first canonical record to the stop
    /// request (`Q`), or to the last canonical record without one.
    pub capture_ms: u64,
    /// The native geth source's connected time inside the capture window.
    pub native_connected_ms: u64,
    /// BSC3-064: the capture window's time in which any source was
    /// connected, over an unbroken canonical chain (a complete or running
    /// window with no skipped blocks, lost, dropped or unparsed records);
    /// none when its continuity is not shown.
    pub observable_ms: Option<u64>,
    /// Its disconnects, rate limits and stops inside the capture window.
    pub native_interruptions: u64,
    /// Phase H: the Flap curve trades journaled (`T`) and the Portal trade
    /// logs the decoder refused (`W`).
    pub flap_trades: u64,
    pub trade_gaps: u64,
    pub state: &'static str,
    /// BSC3-033: the G0.4 pre-commitment its run was started under (its
    /// journal's `g` record: the record hash); none for a run without one,
    /// an older journal or two different ones. Not part of the census
    /// record: the verdict compares it.
    pub g04_precommit_sha256: Option<[u8; 32]>,
}

/// The census of all journals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub windows: Vec<Window>,
    pub covered_ms: u64,
    /// BSC3-064: the windows' observable time ([`Window::observable_ms`]):
    /// not the run time `covered_ms` counts.
    pub observable_ms: u64,
    /// BSC3-037: of `external_buys` (a Flap launch's buys in its block
    /// after the launch transaction), those its creator's own account sent:
    /// from the creator word or from the launch transaction's sender (a
    /// launcher contract's creator word sends none). They stay in
    /// `external_buys` and the placement (a real competitor at N+1).
    pub creator_buys_after: u64,
    pub flap_launches: u64,
    pub four_launches: u64,
    pub orphaned_launches: u64,
    pub outage_launches: u64,
    pub full_before: u64,
    pub hash_only_before: u64,
    pub after: u64,
    pub missed: u64,
    pub clock_unknown: u64,
    /// BSC3-062: the sighting may have been forgotten before the join (not
    /// evaluable, never a miss).
    pub evicted: u64,
    /// Launches (any venue) whose transaction type/form is not rebuilt, and
    /// those whose provider answers were refused every time.
    pub tx_unverifiable: u64,
    pub tx_answer_refused: u64,
    /// Four.meme launches by the pre-execution verdict; `unrecorded`: a
    /// record of an older build.
    pub four_derived: u64,
    /// Paket 8 review M1: Four Classic launches whose token is one of the
    /// frozen pairs' candidates (not derivable, not actionable).
    pub four_candidate: u64,
    pub four_no_derivation: u64,
    pub four_mismatch: u64,
    pub four_not_recognized: u64,
    pub four_verdict_unrecorded: u64,
    /// Four.meme launches by pending coverage (Phase F §10, per venue),
    /// with the same classes as the Flap ones above.
    pub four_full_before: u64,
    pub four_hash_only_before: u64,
    pub four_after: u64,
    pub four_missed: u64,
    pub four_clock_unknown: u64,
    pub four_evicted: u64,
    /// Later sightings of a launch by another source (Phase F §7, the
    /// first-seen spread across sources) and their lag behind the first.
    pub later_sightings: u64,
    pub later_lag_p50_us: Option<u64>,
    pub later_lag_p95_us: Option<u64>,
    pub later_lag_max_us: Option<u64>,
    /// Pending events the ingress queue dropped before the journals.
    pub ingress_dropped: u64,
    /// Phase F §10 per venue: Four Classic and OpenFour launches; those
    /// actionable before execution (seen in full before the block with the
    /// token derived from the launch bytes); Four launches during a source
    /// outage and the lead of those seen in full before the block; launch
    /// calls included and reverted; journaled launch sightings never
    /// included by their window's end.
    pub classic_launches: u64,
    pub openfour_launches: u64,
    pub four_actionable: u64,
    pub classic_actionable: u64,
    pub openfour_actionable: u64,
    pub four_outage_launches: u64,
    pub four_lead_p50_us: Option<u64>,
    pub four_lead_max_us: Option<u64>,
    pub reverted_flap: u64,
    pub reverted_classic: u64,
    pub reverted_openfour: u64,
    pub launches_not_included: u64,
    pub c1_scope_launches: u64,
    pub c1_scope_full_before: u64,
    /// Nearest-rank percentiles of the lead (µs) over `full_before`.
    pub lead_p50_us: Option<u64>,
    pub lead_p95_us: Option<u64>,
    pub lead_p99_us: Option<u64>,
    pub lead_max_us: Option<u64>,
    pub contested_launches: u64,
    pub external_buys: u64,
    /// G0.4 placement (Flap): launches whose first external BUY in the
    /// launch block sits at the launch index + 1 (the earliest valid
    /// position after the launch, taken by a foreign BUY), later in the
    /// block; of the first those seen in full before the block; launches
    /// whose placement is unknown (token unknown, trades incomplete).
    pub adjacent_buy_launches: u64,
    pub later_buy_launches: u64,
    pub adjacent_buy_full_before: u64,
    pub placement_unknown: u64,
    /// Flap launches whose external buys are unknown (not in the two above).
    pub buys_unknown: u64,
    pub unjoined: u64,
    pub blocks_skipped: u64,
    /// Canonical heights between two process windows (not followed by any).
    pub blocks_between_windows: u64,
    pub unparsed_records: u64,
    pub fetch_lag_ms_min: Option<i64>,
    pub fetch_lag_ms_p50: Option<i64>,
    pub fetch_lag_ms_max: Option<i64>,
    pub clock_suspect: bool,
    /// Per source: actionable launches it delivered first.
    pub first_full_before: Vec<(SourceId, u64)>,
    /// REV.1.0 R6 (O05/§3a): per source, in [`SourceId::ALL`] order, its
    /// later sightings, their lag, and its dropped events.
    pub source_spread: Vec<SourceSpread>,
    /// The inputs of the G0.4 criteria (`g04_criteria`) per venue, in
    /// [`CRITERIA_VENUES`] order. Not part of the census record.
    pub criteria_inputs: [VenueInputs; 3],
}

/// REV.1.0 R6 (O05/§3a): one source's spread behind the first sightings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpread {
    pub source: SourceId,
    /// Its sightings of a launch another source delivered first.
    pub later_sightings: u64,
    /// Their lag behind the first (µs; p50 only from two on).
    pub later_lag_p50_us: Option<u64>,
    pub later_lag_max_us: Option<u64>,
    /// Pending events of it the ingress queue dropped.
    pub dropped: u64,
}

/// The venues of the G0.4 criteria, in [`Census::criteria_inputs`] order.
pub const CRITERIA_VENUES: [&str; 3] =
    darkbot_bsc_record::schema::bsc_census::verdict::RATED_VENUES;

/// One venue's G0.4 criteria inputs (order V2 §8, `bsc/config/g04_criteria.env`):
/// its launches while a source was connected (`connected`), those of them in
/// the venue's tradable profile (`scope`: Flap the C1 scope of the strict
/// decoder, Four a recognized launch), those of the scope actionable before
/// the block (seen in full before the block time with a verified
/// transaction; Four also with the token derived from its bytes) and the
/// leads (µs) of the actionable ones, sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VenueInputs {
    pub connected: u64,
    pub scope: u64,
    pub actionable: u64,
    pub leads_us: Vec<u64>,
}

/// One canonical launch record (`b'C'`); class 0 marks an orphaned one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LaunchRecord {
    pub(crate) venue: Venue,
    /// REV.1.0 R2: its block's number and header time (ms).
    pub(crate) block_number: u64,
    pub(crate) block_ms: u64,
    pub(crate) block_hash: [u8; 32],
    /// The launch's index in its block; `None` for an orphaned one.
    pub(crate) tx_index: Option<u64>,
    /// The created token, where known.
    pub(crate) token: Option<[u8; 20]>,
    pub(crate) tx_hash: [u8; 32],
    external_buys: Option<u32>,
    pub(crate) class: u8,
    source: Option<usize>,
    delta_us: u64,
    pub(crate) c1_scope: bool,
    pub(crate) tx_unverifiable: bool,
    pub(crate) tx_answer_refused: bool,
    fetch_lag_ms: Option<i64>,
    /// `None` in a record of an older build.
    four: Option<FourToken>,
    /// Addressed to a venue contract; `None` in a record of an older build.
    pub(crate) venue_target: Option<bool>,
    seen_ns: u64,
}

fn u64_at(p: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(p.get(at..at + 8)?.try_into().ok()?))
}

fn hash_at(p: &[u8], at: usize) -> Option<[u8; 32]> {
    p.get(at..at + 32)?.try_into().ok()
}

fn flag(byte: u8) -> Option<bool> {
    match byte {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

pub(crate) fn launch_record(p: &[u8], seen_ns: u64) -> Option<LaunchRecord> {
    if ![
        CANONICAL_LAUNCH_LEN,
        CANONICAL_LAUNCH_LEN_V2,
        CANONICAL_LAUNCH_LEN_V1,
    ]
    .contains(&p.len())
        || p[0] != b'C'
    {
        return None;
    }
    let venue_target = match p.get(CANONICAL_LAUNCH_LEN_V2) {
        Some(b) => Some(flag(*b)?),
        None => None,
    };
    let four = match p.get(CANONICAL_LAUNCH_LEN_V1) {
        Some(b) => Some(four_token_of_byte(*b)?),
        None => None,
    };
    // Every venue the writer knows (Phase E: OpenFour is byte 3).
    let venue = venue_of_byte(p[1])?;
    // A Flap launch has no Four verdict and a Four launch always has one:
    // anything else is an inconsistent record (Phase E review pass 3).
    if let Some(t) = four
        && (venue == Venue::Flap) != (t == FourToken::NotApplicable)
    {
        return None;
    }
    let buys_known = flag(p[103])?;
    let class = p[108];
    if !(1..=6).contains(&class) {
        return None;
    }
    let source = match p[109] {
        0xFF => None,
        i if usize::from(i) < SourceId::ALL.len() => Some(usize::from(i)),
        _ => return None,
    };
    let lag_known = flag(p[127])?;
    let tx = p[126];
    if tx > 3 {
        return None;
    }
    Some(LaunchRecord {
        venue,
        block_number: u64_at(p, 2)?,
        block_ms: u64_at(p, 118)?,
        block_hash: hash_at(p, 10)?,
        tx_index: Some(u64_at(p, 42)?),
        token: if flag(p[82])? {
            Some(p.get(83..103)?.try_into().ok()?)
        } else {
            None
        },
        tx_hash: hash_at(p, 50)?,
        external_buys: if buys_known {
            Some(u32::from_le_bytes(p.get(104..108)?.try_into().ok()?))
        } else {
            None
        },
        class,
        source,
        delta_us: u64_at(p, 110)?,
        c1_scope: tx == 1,
        tx_unverifiable: tx == 2,
        tx_answer_refused: tx == 3,
        fetch_lag_ms: if lag_known {
            Some(i64::from_le_bytes(p.get(128..136)?.try_into().ok()?))
        } else {
            None
        },
        four,
        venue_target,
        seen_ns,
    })
}

/// Source connectivity over the journal's monotonic time.
#[derive(Default)]
struct Connectivity {
    /// Per source: (time, connected) transitions in journal order.
    events: [Vec<(u64, bool)>; 5],
}

impl Connectivity {
    fn event(&mut self, source: usize, event: u8, at: u64) {
        // 3 Connected; 2 Stopped, 4 Disconnected, 5 RateLimited end it.
        let connected = match event {
            3 => true,
            2 | 4 | 5 => false,
            _ => return,
        };
        if let Some(e) = self.events.get_mut(source) {
            e.push((at, connected));
        }
    }

    /// Orders each source's transitions by time (stable: journal order on ties).
    fn sort(&mut self) {
        for e in &mut self.events {
            e.sort_by_key(|(t, _)| *t);
        }
    }

    /// After [`Connectivity::sort`]: the connected time of `source` in
    /// `[from, to]` and how often it went from connected to disconnected
    /// inside it.
    fn continuity(&self, source: usize, from: u64, to: u64) -> (u64, u64) {
        let Some(events) = self.events.get(source) else {
            return (0, 0);
        };
        let n = events.partition_point(|(t, _)| *t <= from);
        let mut since = (n > 0 && events[n - 1].1).then_some(from);
        let (mut connected, mut interruptions) = (0u64, 0u64);
        for &(t, up) in events[n..].iter().take_while(|(t, _)| *t <= to) {
            match (since, up) {
                (None, true) => since = Some(t),
                (Some(s), false) => {
                    connected = connected.saturating_add(t - s);
                    interruptions += 1;
                    since = None;
                }
                _ => {}
            }
        }
        if let Some(s) = since {
            connected = connected.saturating_add(to - s);
        }
        (connected, interruptions)
    }

    /// After [`Connectivity::sort`]: the time in `[from, to]` in which any
    /// source was connected.
    fn any_connected(&self, from: u64, to: u64) -> u64 {
        let mut up: Vec<bool> = self
            .events
            .iter()
            .map(|events| {
                let n = events.partition_point(|(t, _)| *t <= from);
                n > 0 && events[n - 1].1
            })
            .collect();
        let mut changes: Vec<(u64, usize, bool)> = self
            .events
            .iter()
            .enumerate()
            .flat_map(|(source, events)| {
                events
                    .iter()
                    .filter(|(t, _)| *t > from && *t <= to)
                    .map(move |&(t, connected)| (t, source, connected))
            })
            .collect();
        changes.sort_by_key(|(t, _, _)| *t);
        let mut since = up.iter().any(|u| *u).then_some(from);
        let mut total = 0u64;
        for (t, source, connected) in changes {
            up[source] = connected;
            match (since, up.iter().any(|u| *u)) {
                (None, true) => since = Some(t),
                (Some(s), false) => {
                    total = total.saturating_add(t - s);
                    since = None;
                }
                _ => {}
            }
        }
        if let Some(s) = since {
            total = total.saturating_add(to - s);
        }
        total
    }

    /// After [`Connectivity::sort`]: was any source connected at `at`?
    fn any_connected_at(&self, at: u64) -> bool {
        self.events.iter().any(|events| {
            let n = events.partition_point(|(t, _)| *t <= at);
            n > 0 && events[n - 1].1
        })
    }
}

/// Flap BUYs by (block hash, token): (tx index, tx hash).
type BlockBuys = HashMap<([u8; 32], [u8; 20]), Vec<(u64, [u8; 32])>>;
/// BSC3-037: each Flap BUY's (index, log, sender), by block and token.
type BlockSenders = HashMap<([u8; 32], [u8; 20]), Vec<(u64, u64, [u8; 20])>>;

/// What one journal contributes.
struct Replay {
    window: Window,
    launches: Vec<LaunchRecord>,
    /// Launch calls included and reverted (not taken out by a reorg).
    reverted: Vec<Venue>,
    /// Journaled launch sightings never included by the window's end.
    not_included: u64,
    connectivity: Connectivity,
    /// The lag of every later sighting of a launch behind its first (`L`),
    /// with its source's byte.
    later_lags_ns: Vec<(u8, u64)>,
    /// Each source's pending events the ingress queue dropped (`D`).
    dropped: HashMap<u8, u64>,
    /// Flap BUYs (`T`).
    buys: BlockBuys,
    /// Blocks whose trades are incomplete (`W`).
    trade_gap_blocks: HashSet<[u8; 32]>,
    /// BSC3-037: of the launches' external buys, their creators' own.
    creator_buys_after: u64,
}

/// One journal: a single file of an older build or a segmented run.
enum Reader {
    File(JournalReader<std::io::BufReader<fs::File>>),
    Run(Box<RunReader>),
}

impl Reader {
    fn next_record(&mut self) -> Result<Option<CaptureRecord>, JournalError> {
        match self {
            Reader::File(r) => r.next_record(),
            Reader::Run(r) => r.next_record(),
        }
    }
}

/// Whether the run manifest names exactly the sealed segments read.
pub(crate) fn manifest_matches(bytes: &[u8], read: &[SegmentRead]) -> bool {
    let Ok(segments) = cr::decode_run(bytes).and_then(|m| m.nested_all(cr::SEGMENTS)) else {
        return false;
    };
    let sealed: Vec<&SegmentRead> = read.iter().filter(|r| r.sealed).collect();
    segments.len() == sealed.len()
        && segments.iter().zip(sealed).all(|(m, r)| {
            let file = r.path.file_name().and_then(|n| n.to_str());
            m.u64(cr::segment::INDEX).ok() == Some(u64::from(r.index))
                && m.text(cr::segment::FILE).ok() == file
                && m.u64(cr::segment::RECORDS).ok() == Some(r.records)
                && m.opt_u64(cr::segment::FIRST_SEQUENCE).ok() == Some(r.first_sequence)
                && m.opt_u64(cr::segment::LAST_SEQUENCE).ok() == Some(r.last_sequence)
                && m.b256(cr::segment::SHA256).ok() == Some(r.sha256)
        })
}

fn replay(
    path: &Path,
    name: String,
    started: u64,
    max_payload: usize,
    active: bool,
) -> Result<Replay, String> {
    let manifest = path
        .is_dir()
        .then(|| fs::read(path.join(RUN_MANIFEST)).ok())
        .flatten()
        .and_then(|b| cr::decode_run(&b).ok());
    let mut reader = if path.is_dir() {
        // A sealed run is read with at least the bound it was written with
        // (the configuration may have lowered it since).
        let max_payload = manifest
            .as_ref()
            .and_then(|m| m.opt_u64(cr::MAX_PAYLOAD).ok().flatten())
            .and_then(|v| usize::try_from(v).ok())
            .map_or(max_payload, |written| written.max(max_payload));
        // A missing or reordered segment is a corrupt run, and so is a
        // manifest of another start than its name (BSC3-033 review L1: the
        // name dates the window): nothing counts.
        match RunReader::open(path, max_payload) {
            _ if manifest
                .as_ref()
                .is_some_and(|m| m.u64(cr::STARTED_UNIX_MS).ok() != Some(started)) =>
            {
                None
            }
            Ok(r) => Some(Reader::Run(Box::new(r))),
            Err(JournalError::FileMismatch) => None,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    } else {
        let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Some(Reader::File(
            JournalReader::new(std::io::BufReader::new(file), max_payload)
                .map_err(|e| format!("{}: {e}", path.display()))?,
        ))
    };
    let mut window = Window {
        journal: name,
        started_unix_ms: started,
        ..Window::default()
    };
    // The `g` records read: the first, and whether another differed.
    let mut precommit: Option<[u8; 32]> = None;
    let mut precommit_conflict = false;
    let mut launches: Vec<LaunchRecord> = Vec::new();
    let mut connectivity = Connectivity::default();
    let mut later_lags_ns: Vec<(u8, u64)> = Vec::new();
    let mut buys: BlockBuys = HashMap::new();
    let mut trade_gap_blocks: HashSet<[u8; 32]> = HashSet::new();
    // BSC3-037: each Flap launch's creator word (`N`), by token, and every
    // buy (`T`) by block and token: (index, log, sender).
    let mut creators: HashMap<[u8; 20], [u8; 20]> = HashMap::new();
    let mut senders: BlockSenders = HashMap::new();
    // Reverted launch calls (`V`), by block and transaction.
    let mut reverted: HashMap<([u8; 32], [u8; 32]), Venue> = HashMap::new();
    // Every journaled launch sighting (`RawSignedTx`), by transaction hash.
    let mut sighted: HashSet<[u8; 32]> = HashSet::new();
    // `D` records: each source's cumulative dropped-event total.
    let mut dropped: HashMap<u8, u64> = HashMap::new();
    // Launches received (`P`) and resolved (`C`/`O`), by (block, tx).
    let mut received: HashSet<([u8; 32], [u8; 32])> = HashSet::new();
    let mut resolved: HashSet<([u8; 32], [u8; 32])> = HashSet::new();
    let mut exited = false;
    let mut checkpoint = None;
    let mut last_ns = 0u64;
    // The capture window's bounds: canonical records and the stop request.
    let mut canonical_ns: Option<(u64, u64)> = None;
    let mut stop_ns: Option<u64> = None;
    let mut state = loop {
        let Some(reader) = reader.as_mut() else {
            break "INVALID";
        };
        let record = match reader.next_record() {
            Ok(Some(r)) => r,
            Ok(None) if exited => break "COMPLETE",
            Ok(None) | Err(JournalError::Truncated) if active => break "RUNNING",
            Ok(None) => break "UNTERMINATED",
            Err(JournalError::Truncated) => break "TAIL_INCOMPLETE",
            Err(_) => break "INVALID",
        };
        last_ns = last_ns.max(record.timestamp_ns);
        if record.kind == RecordKind::Canonical {
            let t = record.timestamp_ns;
            canonical_ns = Some(canonical_ns.map_or((t, t), |(a, b)| (a.min(t), b.max(t))));
        }
        let p = &record.payload;
        match (record.kind, p.first().copied()) {
            (RecordKind::GapSpan, _) => {
                window.lost_records = window
                    .lost_records
                    .saturating_add(u64_at(p, 0).unwrap_or(1));
            }
            (RecordKind::Runtime, Some(b'S')) if p.len() == 3 => {
                connectivity.event(usize::from(p[1]), p[2], record.timestamp_ns);
            }
            (RecordKind::Runtime, Some(b'X')) if p.len() == 1 => exited = true,
            (RecordKind::Runtime, Some(b'g')) if p.len() == 33 => {
                let mut h = [0u8; 32];
                h.copy_from_slice(&p[1..]);
                precommit_conflict |= precommit.is_some_and(|x| x != h);
                precommit = precommit.or(Some(h));
            }
            (RecordKind::Runtime, Some(b'v'))
                if p.len() == crate::runtime_journal::FOUR_PROBE_LEN =>
            {
                match p[1] {
                    1 => window.four_match_probes += 1,
                    2 => window.four_drift_probes += 1,
                    _ => {}
                }
            }
            (RecordKind::Runtime, Some(b'Q')) if p.len() == 1 => {
                stop_ns = stop_ns.or(Some(record.timestamp_ns));
            }
            // BSC3-074: a PAPER BUY's handoff is no census input and no
            // damaged record (it was counted unparsed: a run with PAPER
            // BUYs had no observable time).
            (RecordKind::Runtime, Some(b'Q'))
                if p.len() >= crate::runtime_journal::PAPER_HANDOFF_HEAD => {}
            (RecordKind::Runtime, Some(b'D')) if p.len() == 10 => match u64_at(p, 2) {
                Some(total) => {
                    let e = dropped.entry(p[1]).or_default();
                    *e = (*e).max(total);
                }
                None => window.unparsed_records += 1,
            },
            // `L | source u8 | hash [32] | lag_ns u64`: the spread across sources.
            (RecordKind::Runtime, Some(b'L')) if p.len() == 42 => {
                if let Some(lag) = u64_at(p, 34) {
                    later_lags_ns.push((p[1], lag));
                }
            }
            (RecordKind::Canonical, Some(b'P')) if p.len() == 74 => {
                match (hash_at(p, 10), hash_at(p, 42), venue_of_byte(p[1])) {
                    (Some(block), Some(tx), Some(_)) => {
                        received.insert((block, tx));
                    }
                    // An unknown venue: counted, never silently skipped.
                    _ => window.unparsed_records += 1,
                }
            }
            (RecordKind::Canonical, Some(b'C')) => match launch_record(p, record.timestamp_ns) {
                Some(l) => {
                    resolved.insert((l.block_hash, l.tx_hash));
                    launches.push(l);
                }
                None => window.unparsed_records += 1,
            },
            (RecordKind::RawSignedTx, _) => {
                if let Some(h) = hash_at(p, 1) {
                    sighted.insert(h);
                }
            }
            (RecordKind::Canonical, Some(b'V')) if p.len() == 74 => {
                match (hash_at(p, 10), hash_at(p, 42), venue_of_byte(p[1])) {
                    (Some(block), Some(tx), Some(venue)) => {
                        reverted.insert((block, tx), venue);
                    }
                    _ => window.unparsed_records += 1,
                }
            }
            (RecordKind::Canonical, Some(b'O')) if p.len() == 74 => {
                let (Some(number), Some(block), Some(tx), Some(venue)) = (
                    u64_at(p, 2),
                    hash_at(p, 10),
                    hash_at(p, 42),
                    venue_of_byte(p[1]),
                ) else {
                    window.unparsed_records += 1;
                    continue;
                };
                // A reverted launch call of an orphaned block: taken out.
                if reverted.remove(&(block, tx)).is_some() {
                    continue;
                }
                {
                    resolved.insert((block, tx));
                    launches.retain(|l| !(l.block_hash == block && l.tx_hash == tx));
                    // Counted as orphaned (class 0), joined or not.
                    launches.push(LaunchRecord {
                        venue,
                        block_number: number,
                        block_ms: 0,
                        block_hash: block,
                        tx_index: None,
                        token: None,
                        tx_hash: tx,
                        external_buys: None,
                        class: 0,
                        source: None,
                        delta_us: 0,
                        c1_scope: false,
                        tx_unverifiable: false,
                        tx_answer_refused: false,
                        fetch_lag_ms: None,
                        four: None,
                        venue_target: None,
                        seen_ns: record.timestamp_ns,
                    });
                }
            }
            (RecordKind::Canonical, Some(b'T'))
                if crate::runtime_journal::flap_trade_of(p).is_some() =>
            {
                window.flap_trades += 1;
                if let Some(t) = crate::runtime_journal::flap_trade_of(p)
                    && !t.sell
                {
                    buys.entry((t.block_hash, t.token))
                        .or_default()
                        .push((t.tx_index, t.tx_hash));
                    senders.entry((t.block_hash, t.token)).or_default().push((
                        t.tx_index,
                        t.log_index,
                        t.origin,
                    ));
                }
            }
            (RecordKind::Canonical, Some(b'W')) if p.len() == 57 => {
                window.trade_gaps += 1;
                if let Some(block) = hash_at(p, 9) {
                    trade_gap_blocks.insert(block);
                }
            }
            (RecordKind::Canonical, Some(b'F'))
                if crate::runtime_journal::flap_term_of(p).is_some() => {}
            // R12 SC4: the Four.meme curve records (a trade, a phase, a
            // refused log) are no Flap market events: parsed, not counted;
            // damaged ones are unparsed.
            (RecordKind::Canonical, Some(b'Z'))
                if crate::runtime_journal::four_trade_of(p).is_some() => {}
            (RecordKind::Canonical, Some(b'y')) if p.len() == 79 => {}
            (RecordKind::Canonical, Some(b'z')) if p.len() == 57 => {}
            (RecordKind::Canonical, Some(b'M')) if p.len() == 77 => {}
            // Phase H: the strategy's blocks, Flap launches and resyncs.
            (RecordKind::Canonical, Some(b'B'))
                if p.len() == crate::runtime_strategy::BLOCK_LEN => {}
            (RecordKind::Canonical, Some(b'N'))
                if p.len() == crate::runtime_strategy::LAUNCH_LEN =>
            {
                let a20 =
                    |from: usize| -> Option<[u8; 20]> { p.get(from..from + 20)?.try_into().ok() };
                if let (Some(_), Some(_), Some(token), Some(creator)) =
                    (u64_at(p, 1), u64_at(p, 9), a20(17), a20(37))
                {
                    creators.insert(token, creator);
                }
            }
            (RecordKind::Canonical, Some(b'I')) if p.len() == 9 => {}
            (RecordKind::Canonical, Some(tag @ (b'H' | b'E' | b'K'))) if p.len() == 41 => {
                let number = u64_at(p, 1);
                match tag {
                    b'H' => window.first_block = window.first_block.or(number),
                    b'E' => window.last_block = number,
                    _ => checkpoint = number,
                }
            }
            (RecordKind::Reorg, Some(b'R')) if p.len() == 45 => window.reorgs += 1,
            (RecordKind::Reorg, Some(b'G')) if p.len() == 17 => {
                window.resyncs += 1;
                if let (Some(from), Some(to)) = (u64_at(p, 1), u64_at(p, 9))
                    && to >= from
                {
                    window.blocks_skipped = window.blocks_skipped.saturating_add(to - from + 1);
                }
            }
            // A known runtime tag of another length: damaged, counted (L6).
            (RecordKind::Runtime, Some(b'S' | b'L' | b'D' | b'X' | b'Q' | b'v' | b'g')) => {
                window.unparsed_records += 1
            }
            (RecordKind::Canonical | RecordKind::Reorg, _) => window.unparsed_records += 1,
            _ => {}
        }
    };
    window.g04_precommit_sha256 = precommit.filter(|_| !precommit_conflict);
    if let Some(Reader::Run(run)) = &reader
        && !active
    {
        match fs::read(path.join(RUN_MANIFEST)) {
            Ok(bytes) if !manifest_matches(&bytes, run.segments()) => state = "INVALID",
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if state == "COMPLETE" {
                    state = "UNSEALED";
                }
            }
            Err(e) => return Err(format!("{}: {e}", path.join(RUN_MANIFEST).display())),
        }
    }
    window.state = state;
    if state == "INVALID" {
        window.first_block = None;
        window.last_block = None;
        launches.clear();
        connectivity = Connectivity::default();
        later_lags_ns.clear();
        dropped.clear();
        buys.clear();
        trade_gap_blocks.clear();
    } else {
        window.ingress_dropped = dropped.values().fold(0u64, |a, b| a.saturating_add(*b));
        window.last_block = window.last_block.or(checkpoint);
        let open = received.difference(&resolved).count() as u64;
        if state == "RUNNING" {
            window.pending_join = open;
        } else {
            window.unjoined = open;
        }
    }
    window.duration_ms = last_ns / 1_000_000;
    connectivity.sort();
    if state != "INVALID"
        && let Some((first, last)) = canonical_ns
    {
        let end = stop_ns.map_or(last, |s| s.clamp(first, last));
        let native = usize::from(crate::runtime_journal::source_byte(SourceId::Geth));
        let (connected, interruptions) = connectivity.continuity(native, first, end);
        window.capture_ms = (end - first) / 1_000_000;
        window.native_connected_ms = connected / 1_000_000;
        window.native_interruptions = interruptions;
        let unbroken = matches!(state, "COMPLETE" | "RUNNING")
            && window.blocks_skipped == 0
            && window.lost_records == 0
            && window.ingress_dropped == 0
            && window.unparsed_records == 0;
        if unbroken {
            window.observable_ms = Some(connectivity.any_connected(first, end) / 1_000_000);
        }
    }
    // A sighting whose transaction no canonical record names by the end of a
    // finished window was never included in it (replaced, dropped, or later).
    let not_included = if state == "INVALID" || state == "RUNNING" {
        0
    } else {
        let included: HashSet<[u8; 32]> = received
            .iter()
            .chain(resolved.iter())
            .chain(reverted.keys())
            .map(|(_, tx)| *tx)
            .collect();
        sighted.difference(&included).count() as u64
    };
    let reverted: Vec<Venue> = if state == "INVALID" {
        Vec::new()
    } else {
        reverted.into_values().collect()
    };
    // BSC3-037: per canonical Flap launch with its buys known, its block's
    // later buys of its token whose sender is the creator word or the launch
    // transaction's own sender (its creator purchase names it), each log
    // once. Joined by the block hash: an orphaned launch is out, one
    // included again counts in its new block.
    let creator_buys_after = launches
        .iter()
        .filter(|l| l.venue == Venue::Flap && l.external_buys.is_some())
        .filter_map(|l| {
            let (token, index) = (l.token?, l.tx_index?);
            let buys = senders.get(&(l.block_hash, token))?;
            let creator = creators.get(&token);
            let launcher = buys.iter().find(|b| b.0 == index).map(|b| &b.2);
            let own: HashSet<(u64, u64)> = buys
                .iter()
                .filter(|(at, _, from)| {
                    *at > index && (Some(from) == creator || Some(from) == launcher)
                })
                .map(|(at, log, _)| (*at, *log))
                .collect();
            Some(own.len() as u64)
        })
        .sum();
    Ok(Replay {
        window,
        launches,
        reverted,
        not_included,
        connectivity,
        later_lags_ns,
        dropped,
        buys,
        trade_gap_blocks,
        creator_buys_after,
    })
}

/// Nearest rank of quantile `q` (per mille) when `n >= 1000 / (1000 - q)`.
pub(crate) fn percentile<T: Copy>(sorted: &[T], q_per_mille: u64) -> Option<T> {
    let n = sorted.len() as u64;
    if n == 0 || n * (1000 - q_per_mille) < 1000 {
        return None;
    }
    let rank = (q_per_mille * n).div_ceil(1000).max(1);
    sorted.get(usize::try_from(rank - 1).ok()?).copied()
}

/// Replays every journal of `dir`: `<started_unix_ms>.dbjnl` files and
/// `<started_unix_ms>/` run directories; `active` names the journal the
/// running runtime still writes.
pub fn census(dir: &Path, max_payload: usize, active: Option<&Path>) -> Result<Census, String> {
    let mut names: Vec<(u64, String)> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Hidden entries are no journal (Phase F gate M2).
        if name.starts_with('.') {
            continue;
        }
        let is_run = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", entry.path().display()))?
            .is_dir();
        if is_run {
            let started: u64 = name
                .parse()
                .map_err(|_| format!("{name}: not a <started_unix_ms> run directory"))?;
            names.push((started, name));
            continue;
        }
        let Some(stem) = name.strip_suffix(".dbjnl") else {
            continue;
        };
        let started: u64 = stem
            .parse()
            .map_err(|_| format!("{name}: not <started_unix_ms>.dbjnl"))?;
        names.push((started, name));
    }
    names.sort();
    census_of(dir, names, max_payload, active)
}

/// The census of one journal of `dir` alone (a run directory or a file of
/// an older build, by its name): what the replay proof compares with the
/// runtime's own counters of that run.
pub fn census_of_run(dir: &Path, name: &str, max_payload: usize) -> Result<Census, String> {
    let started: u64 = name
        .trim_end_matches(".dbjnl")
        .parse()
        .map_err(|_| format!("{name}: not a <started_unix_ms> journal"))?;
    census_of(dir, vec![(started, name.to_string())], max_payload, None)
}

fn census_of(
    dir: &Path,
    names: Vec<(u64, String)>,
    max_payload: usize,
    active: Option<&Path>,
) -> Result<Census, String> {
    let mut out = Census::default();
    let mut leads: Vec<u64> = Vec::new();
    let mut four_leads: Vec<u64> = Vec::new();
    let mut later: Vec<u64> = Vec::new();
    let mut lags: Vec<i64> = Vec::new();
    let mut firsts: HashMap<usize, u64> = HashMap::new();
    // R6: each source's later sightings (µs) and dropped events.
    let mut per_later: HashMap<u8, Vec<u64>> = HashMap::new();
    let mut per_dropped: HashMap<u8, u64> = HashMap::new();
    for (started, name) in names {
        let path = dir.join(&name);
        let is_active = active.is_some_and(|a| a == path.as_path());
        let r = replay(&path, name, started, max_payload, is_active)?;
        // Nothing of an INVALID window counts (Phase F gate L3): its frames
        // are not trusted.
        if r.window.state != "INVALID" {
            out.covered_ms = out.covered_ms.saturating_add(r.window.duration_ms);
            out.observable_ms = out
                .observable_ms
                .saturating_add(r.window.observable_ms.unwrap_or(0));
            out.creator_buys_after = out.creator_buys_after.saturating_add(r.creator_buys_after);
            out.unjoined = out.unjoined.saturating_add(r.window.unjoined);
            out.blocks_skipped = out.blocks_skipped.saturating_add(r.window.blocks_skipped);
            out.unparsed_records = out
                .unparsed_records
                .saturating_add(r.window.unparsed_records);
            out.ingress_dropped = out.ingress_dropped.saturating_add(r.window.ingress_dropped);
        }
        // Heights no process followed: after the previous window's last block.
        if let (Some(prev), Some(first)) = (
            out.windows.iter().rev().find_map(|w| w.last_block),
            r.window.first_block,
        ) && first > prev.saturating_add(1)
        {
            out.blocks_between_windows += first - prev - 1;
        }
        for l in &r.launches {
            if l.class == 0 {
                out.orphaned_launches += 1;
                continue;
            }
            out.tx_unverifiable += u64::from(l.tx_unverifiable);
            out.tx_answer_refused += u64::from(l.tx_answer_refused);
            // BSC3-062: a launch whose sighting may have been forgotten
            // before its join (class 6) is not evaluable: in no criteria
            // input, neither the share's numerator nor its denominator.
            if r.connectivity.any_connected_at(l.seen_ns) && l.class != 6 {
                let (i, scope, token) = match l.venue {
                    Venue::Flap => (0, l.c1_scope, true),
                    venue => (
                        if venue == Venue::OpenFour { 2 } else { 1 },
                        !matches!(
                            l.four,
                            None | Some(FourToken::NotRecognized | FourToken::NotApplicable)
                        ),
                        l.four == Some(FourToken::Matched),
                    ),
                };
                let v = &mut out.criteria_inputs[i];
                v.connected += 1;
                if scope {
                    v.scope += 1;
                    if l.class == 1 && !l.tx_unverifiable && token {
                        v.actionable += 1;
                        v.leads_us.push(l.delta_us);
                    }
                }
            }
            if l.venue != Venue::Flap {
                out.four_launches += 1;
                *match l.venue {
                    Venue::OpenFour => &mut out.openfour_launches,
                    _ => &mut out.classic_launches,
                } += 1;
                // Seen in full before the block with the token derived from
                // the launch bytes: actionable before execution (§7/§10).
                if l.class == 1 && l.four == Some(FourToken::Matched) {
                    out.four_actionable += 1;
                    *match l.venue {
                        Venue::OpenFour => &mut out.openfour_actionable,
                        _ => &mut out.classic_actionable,
                    } += 1;
                }
                if l.class == 1 {
                    four_leads.push(l.delta_us);
                }
                if !r.connectivity.any_connected_at(l.seen_ns) {
                    out.four_outage_launches += 1;
                }
                let n = match l.four {
                    Some(FourToken::Matched) => &mut out.four_derived,
                    Some(FourToken::Unknown) => &mut out.four_no_derivation,
                    Some(FourToken::Mismatch) => &mut out.four_mismatch,
                    Some(FourToken::Candidate) => &mut out.four_candidate,
                    // `NotApplicable` on a Four record is refused by `launch_record`.
                    Some(FourToken::NotRecognized | FourToken::NotApplicable) => {
                        &mut out.four_not_recognized
                    }
                    None => &mut out.four_verdict_unrecorded,
                };
                *n += 1;
                *match l.class {
                    1 => &mut out.four_full_before,
                    2 => &mut out.four_hash_only_before,
                    3 => &mut out.four_after,
                    4 => &mut out.four_missed,
                    5 => &mut out.four_clock_unknown,
                    _ => &mut out.four_evicted,
                } += 1;
                continue;
            }
            out.flap_launches += 1;
            if l.c1_scope {
                out.c1_scope_launches += 1;
            }
            if let Some(lag) = l.fetch_lag_ms {
                lags.push(lag);
            }
            if !r.connectivity.any_connected_at(l.seen_ns) {
                out.outage_launches += 1;
            }
            match l.class {
                1 => {
                    out.full_before += 1;
                    if l.c1_scope {
                        out.c1_scope_full_before += 1;
                    }
                    leads.push(l.delta_us);
                    if let Some(s) = l.source {
                        *firsts.entry(s).or_default() += 1;
                    }
                }
                2 => out.hash_only_before += 1,
                3 => out.after += 1,
                4 => out.missed += 1,
                5 => out.clock_unknown += 1,
                _ => out.evicted += 1,
            }
            match l.external_buys {
                Some(n) => {
                    out.contested_launches += u64::from(n > 0);
                    out.external_buys += u64::from(n);
                }
                None => out.buys_unknown += 1,
            }
            // G0.4 placement: the first external BUY of the token in the
            // launch block, after the launch (its own creator purchase is
            // the launch transaction).
            match (l.tx_index, l.token) {
                (Some(index), Some(token)) if !r.trade_gap_blocks.contains(&l.block_hash) => {
                    let first = r
                        .buys
                        .get(&(l.block_hash, token))
                        .into_iter()
                        .flatten()
                        .filter(|(i, h)| *i > index && *h != l.tx_hash)
                        .map(|(i, _)| *i)
                        .min();
                    match first {
                        Some(i) if i == index + 1 => {
                            out.adjacent_buy_launches += 1;
                            out.adjacent_buy_full_before += u64::from(l.class == 1);
                        }
                        Some(_) => out.later_buy_launches += 1,
                        None => {}
                    }
                }
                _ => out.placement_unknown += 1,
            }
        }
        later.extend(r.later_lags_ns.iter().map(|(_, ns)| ns / 1_000));
        for (source, ns) in &r.later_lags_ns {
            per_later.entry(*source).or_default().push(ns / 1_000);
        }
        for (source, n) in &r.dropped {
            *per_dropped.entry(*source).or_default() += n;
        }
        for v in &r.reverted {
            *match v {
                Venue::Flap => &mut out.reverted_flap,
                Venue::Four => &mut out.reverted_classic,
                Venue::OpenFour => &mut out.reverted_openfour,
            } += 1;
        }
        out.launches_not_included += r.not_included;
        out.windows.push(r.window);
    }
    for v in &mut out.criteria_inputs {
        v.leads_us.sort_unstable();
    }
    later.sort_unstable();
    four_leads.sort_unstable();
    out.four_lead_p50_us = percentile(&four_leads, 500);
    out.four_lead_max_us = four_leads.last().copied();
    out.later_sightings = later.len() as u64;
    out.later_lag_p50_us = percentile(&later, 500);
    out.later_lag_p95_us = percentile(&later, 950);
    out.later_lag_max_us = later.last().copied();
    leads.sort_unstable();
    out.lead_p50_us = percentile(&leads, 500);
    out.lead_p95_us = percentile(&leads, 950);
    out.lead_p99_us = percentile(&leads, 990);
    out.lead_max_us = leads.last().copied();
    lags.sort_unstable();
    out.fetch_lag_ms_min = lags.first().copied();
    out.fetch_lag_ms_p50 = percentile(&lags, 500);
    out.fetch_lag_ms_max = lags.last().copied();
    out.clock_suspect = out.fetch_lag_ms_min.is_some_and(|m| m < 0);
    out.first_full_before = SourceId::ALL
        .iter()
        .enumerate()
        .map(|(i, s)| (*s, firsts.get(&i).copied().unwrap_or(0)))
        .collect();
    out.source_spread = SourceId::ALL
        .iter()
        .map(|s| {
            let byte = crate::runtime_journal::source_byte(*s);
            let mut later = per_later.remove(&byte).unwrap_or_default();
            later.sort_unstable();
            SourceSpread {
                source: *s,
                later_sightings: later.len() as u64,
                later_lag_p50_us: percentile(&later, 500),
                later_lag_max_us: later.last().copied(),
                dropped: per_dropped.get(&byte).copied().unwrap_or(0),
            }
        })
        .collect();
    Ok(out)
}

impl Census {
    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&cs::SCHEMA);
        for w in &self.windows {
            use cs::window as e;
            let mut row = Draft::new(&e::SCHEMA);
            row.set(e::JOURNAL, w.journal.as_str())
                .set(e::STARTED_UNIX_MS, w.started_unix_ms)
                .set(e::DURATION_MS, w.duration_ms)
                .set(e::REORGS, w.reorgs)
                .set(e::RESYNCS, w.resyncs)
                .set(e::LOST_RECORDS, w.lost_records)
                .set(e::STATE, w.state)
                .set(e::BLOCKS_SKIPPED, w.blocks_skipped)
                .set(e::UNJOINED, w.unjoined)
                .set(e::PENDING_JOIN, w.pending_join)
                .set(e::UNPARSED_RECORDS, w.unparsed_records)
                .set(e::INGRESS_DROPPED, w.ingress_dropped)
                .set_opt(e::OBSERVABLE_MS, w.observable_ms);
            if let Some(b) = w.first_block {
                row.set(e::FIRST_BLOCK, b);
            }
            if let Some(b) = w.last_block {
                row.set(e::LAST_BLOCK, b);
            }
            d.push(cs::WINDOWS, row.into_nested());
        }
        d.set(cs::PROVENANCE_TOOL, "bsc_runtime census")
            .set(cs::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(cs::COVERED_MS, self.covered_ms)
            .set(cs::OBSERVABLE_MS, self.observable_ms)
            .set(cs::CREATOR_BUYS_AFTER, self.creator_buys_after)
            .set(cs::EVICTED, self.evicted)
            .set(cs::FOUR_EVICTED, self.four_evicted)
            .set(cs::FLAP_LAUNCHES, self.flap_launches)
            .set(cs::FOUR_LAUNCHES, self.four_launches)
            .set(cs::FOUR_DERIVED, self.four_derived)
            .set(cs::FOUR_CANDIDATE, self.four_candidate)
            .set(cs::FOUR_NO_DERIVATION, self.four_no_derivation)
            .set(cs::FOUR_MISMATCH, self.four_mismatch)
            .set(cs::FOUR_NOT_RECOGNIZED, self.four_not_recognized)
            .set(cs::FOUR_VERDICT_UNRECORDED, self.four_verdict_unrecorded)
            .set(cs::FOUR_FULL_BEFORE, self.four_full_before)
            .set(cs::FOUR_HASH_ONLY_BEFORE, self.four_hash_only_before)
            .set(cs::FOUR_AFTER, self.four_after)
            .set(cs::FOUR_MISSED, self.four_missed)
            .set(cs::FOUR_CLOCK_UNKNOWN, self.four_clock_unknown)
            .set(cs::LATER_SIGHTINGS, self.later_sightings)
            .set(cs::INGRESS_DROPPED, self.ingress_dropped)
            .set(cs::CLASSIC_LAUNCHES, self.classic_launches)
            .set(cs::OPENFOUR_LAUNCHES, self.openfour_launches)
            .set(cs::FOUR_ACTIONABLE, self.four_actionable)
            .set(cs::CLASSIC_ACTIONABLE, self.classic_actionable)
            .set(cs::OPENFOUR_ACTIONABLE, self.openfour_actionable)
            .set(cs::FOUR_OUTAGE_LAUNCHES, self.four_outage_launches)
            .set(cs::REVERTED_FLAP, self.reverted_flap)
            .set(cs::REVERTED_CLASSIC, self.reverted_classic)
            .set(cs::REVERTED_OPENFOUR, self.reverted_openfour)
            .set(cs::LAUNCHES_NOT_INCLUDED, self.launches_not_included)
            .set(cs::ORPHANED_LAUNCHES, self.orphaned_launches)
            .set(cs::OUTAGE_LAUNCHES, self.outage_launches)
            .set(cs::FULL_BEFORE, self.full_before)
            .set(cs::HASH_ONLY_BEFORE, self.hash_only_before)
            .set(cs::AFTER, self.after)
            .set(cs::MISSED, self.missed)
            .set(cs::CONTESTED_LAUNCHES, self.contested_launches)
            .set(cs::EXTERNAL_BUYS, self.external_buys)
            .set(cs::ADJACENT_BUY_LAUNCHES, self.adjacent_buy_launches)
            .set(cs::LATER_BUY_LAUNCHES, self.later_buy_launches)
            .set(cs::ADJACENT_BUY_FULL_BEFORE, self.adjacent_buy_full_before)
            .set(cs::PLACEMENT_UNKNOWN, self.placement_unknown);
        for (tag, v) in [
            (cs::LEAD_P50_US, self.lead_p50_us),
            (cs::LEAD_P95_US, self.lead_p95_us),
            (cs::LEAD_P99_US, self.lead_p99_us),
            (cs::LEAD_MAX_US, self.lead_max_us),
            (cs::LATER_LAG_P50_US, self.later_lag_p50_us),
            (cs::LATER_LAG_P95_US, self.later_lag_p95_us),
            (cs::LATER_LAG_MAX_US, self.later_lag_max_us),
            (cs::FOUR_LEAD_P50_US, self.four_lead_p50_us),
            (cs::FOUR_LEAD_MAX_US, self.four_lead_max_us),
        ] {
            if let Some(v) = v {
                d.set(tag, v);
            }
        }
        for (source, n) in &self.first_full_before {
            use cs::source as e;
            let mut row = Draft::new(&e::SCHEMA);
            row.set(e::SOURCE_ID, source.name())
                .set(e::FIRST_FULL_BEFORE, *n);
            // R6: its spread (a census of this build: every source's).
            if let Some(s) = self.source_spread.iter().find(|s| s.source == *source) {
                row.set(e::LATER_SIGHTINGS, s.later_sightings)
                    .set(e::DROPPED, s.dropped)
                    .set_opt(e::LATER_LAG_P50_US, s.later_lag_p50_us)
                    .set_opt(e::LATER_LAG_MAX_US, s.later_lag_max_us);
            }
            d.push(cs::SOURCES, row.into_nested());
        }
        d.set(cs::CRITERIA, CRITERIA_UNCONFIRMED)
            .set(cs::BUYS_UNKNOWN, self.buys_unknown)
            .set(cs::CLOCK_UNKNOWN, self.clock_unknown)
            .set(cs::C1_SCOPE_LAUNCHES, self.c1_scope_launches)
            .set(cs::C1_SCOPE_FULL_BEFORE, self.c1_scope_full_before)
            .set(cs::UNJOINED, self.unjoined)
            .set(cs::BLOCKS_SKIPPED, self.blocks_skipped);
        for (tag, v) in [
            (cs::FETCH_LAG_MS_MIN, self.fetch_lag_ms_min),
            (cs::FETCH_LAG_MS_P50, self.fetch_lag_ms_p50),
            (cs::FETCH_LAG_MS_MAX, self.fetch_lag_ms_max),
        ] {
            if let Some(v) = v {
                d.set(tag, v);
            }
        }
        d.set(cs::CLOCK_SUSPECT, self.clock_suspect)
            .set(cs::TX_UNVERIFIABLE, self.tx_unverifiable)
            .set(cs::TX_ANSWER_REFUSED, self.tx_answer_refused)
            .set(cs::BLOCKS_BETWEEN_WINDOWS, self.blocks_between_windows)
            .set(cs::UNPARSED_RECORDS, self.unparsed_records);
        d.encode().map_err(|e| e.to_string())
    }

    /// The text view printed by the CLI.
    pub fn summary(&self) -> String {
        let opt = |v: Option<u64>| v.map_or("n/a".to_string(), |v| v.to_string());
        let opti = |v: Option<i64>| v.map_or("n/a".to_string(), |v| v.to_string());
        let mut s = format!(
            "covered_ms={} windows={} flap_launches={} c1_scope={} tx_unverifiable={} four_launches={} orphaned={} outage={} unjoined={} blocks_skipped={} between_windows={} unparsed={}\n\
             four pre-execution token: derived={} candidate={} no_derivation={} mismatch={} not_recognized={} unrecorded={}\n\
             four before the block time: full={} hash_only={} after={} missed={} clock_unknown={} evicted={}\n\
             later sightings={} lag_us p50={} p95={} max={}\n\
             before the block time: full={} (c1_scope {}) hash_only={} after={} missed={} clock_unknown={} evicted={}\n\
             lead_us p50={} p95={} p99={} max={} n={}\n\
             fetch_lag_ms min={} p50={} max={} clock_suspect={}\n\
             contested={} external_buys={} (creator later {}) buys_unknown={} criteria={}\n\
             first external BUY in the launch block: at N+1 {} (seen in full before the block {}) later {} unknown {}\n",
            self.covered_ms,
            self.windows.len(),
            self.flap_launches,
            self.c1_scope_launches,
            self.tx_unverifiable,
            self.four_launches,
            self.orphaned_launches,
            self.outage_launches,
            self.unjoined,
            self.blocks_skipped,
            self.blocks_between_windows,
            self.unparsed_records,
            self.four_derived,
            self.four_candidate,
            self.four_no_derivation,
            self.four_mismatch,
            self.four_not_recognized,
            self.four_verdict_unrecorded,
            self.four_full_before,
            self.four_hash_only_before,
            self.four_after,
            self.four_missed,
            self.four_clock_unknown,
            self.four_evicted,
            self.later_sightings,
            opt(self.later_lag_p50_us),
            opt(self.later_lag_p95_us),
            opt(self.later_lag_max_us),
            self.full_before,
            self.c1_scope_full_before,
            self.hash_only_before,
            self.after,
            self.missed,
            self.clock_unknown,
            self.evicted,
            opt(self.lead_p50_us),
            opt(self.lead_p95_us),
            opt(self.lead_p99_us),
            opt(self.lead_max_us),
            self.full_before,
            opti(self.fetch_lag_ms_min),
            opti(self.fetch_lag_ms_p50),
            opti(self.fetch_lag_ms_max),
            self.clock_suspect,
            self.contested_launches,
            self.external_buys,
            self.creator_buys_after,
            self.buys_unknown,
            CRITERIA_UNCONFIRMED,
            self.adjacent_buy_launches,
            self.adjacent_buy_full_before,
            self.later_buy_launches,
            self.placement_unknown,
        );
        for w in &self.windows {
            s.push_str(&format!(
                "window {} {} blocks {}..{} skipped={} duration_ms={} reorgs={} resyncs={} lost={} unjoined={}\n",
                w.journal,
                w.state,
                opt(w.first_block),
                opt(w.last_block),
                w.blocks_skipped,
                w.duration_ms,
                w.reorgs,
                w.resyncs,
                w.lost_records,
                w.unjoined
            ));
        }
        s
    }
}

#[cfg(test)]
#[path = "g04_census_tests.rs"]
mod tests;
