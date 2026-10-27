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
    /// Its disconnects, rate limits and stops inside the capture window.
    pub native_interruptions: u64,
    /// Phase H: the Flap curve trades journaled (`T`) and the Portal trade
    /// logs the decoder refused (`W`).
    pub flap_trades: u64,
    pub trade_gaps: u64,
    pub state: &'static str,
}

/// The census of all journals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Census {
    pub windows: Vec<Window>,
    pub covered_ms: u64,
    pub flap_launches: u64,
    pub four_launches: u64,
    pub orphaned_launches: u64,
    pub outage_launches: u64,
    pub full_before: u64,
    pub hash_only_before: u64,
    pub after: u64,
    pub missed: u64,
    pub clock_unknown: u64,
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
}

/// One canonical launch record (`b'C'`); class 0 marks an orphaned one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LaunchRecord {
    venue: Venue,
    block_hash: [u8; 32],
    pub(crate) tx_hash: [u8; 32],
    external_buys: Option<u32>,
    pub(crate) class: u8,
    source: Option<usize>,
    delta_us: u64,
    pub(crate) c1_scope: bool,
    tx_unverifiable: bool,
    tx_answer_refused: bool,
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
    if !(1..=5).contains(&class) {
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
        block_hash: hash_at(p, 10)?,
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

    /// After [`Connectivity::sort`]: was any source connected at `at`?
    fn any_connected_at(&self, at: u64) -> bool {
        self.events.iter().any(|events| {
            let n = events.partition_point(|(t, _)| *t <= at);
            n > 0 && events[n - 1].1
        })
    }
}

/// What one journal contributes.
struct Replay {
    window: Window,
    launches: Vec<LaunchRecord>,
    /// Launch calls included and reverted (not taken out by a reorg).
    reverted: Vec<Venue>,
    /// Journaled launch sightings never included by the window's end.
    not_included: u64,
    connectivity: Connectivity,
    /// The lag of every later sighting of a launch behind its first (`L`).
    later_lags_ns: Vec<u64>,
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
    let mut reader = if path.is_dir() {
        // A sealed run is read with at least the bound it was written with
        // (the configuration may have lowered it since).
        let max_payload = fs::read(path.join(RUN_MANIFEST))
            .ok()
            .and_then(|b| cr::decode_run(&b).ok())
            .and_then(|m| m.opt_u64(cr::MAX_PAYLOAD).ok().flatten())
            .and_then(|v| usize::try_from(v).ok())
            .map_or(max_payload, |written| written.max(max_payload));
        // A missing or reordered segment is a corrupt run: nothing counts.
        match RunReader::open(path, max_payload) {
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
    let mut launches: Vec<LaunchRecord> = Vec::new();
    let mut connectivity = Connectivity::default();
    let mut later_lags_ns: Vec<u64> = Vec::new();
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
                    later_lags_ns.push(lag);
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
                let (Some(block), Some(tx), Some(venue)) =
                    (hash_at(p, 10), hash_at(p, 42), venue_of_byte(p[1]))
                else {
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
                        block_hash: block,
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
            }
            (RecordKind::Canonical, Some(b'W')) if p.len() == 57 => window.trade_gaps += 1,
            (RecordKind::Canonical, Some(b'F'))
                if crate::runtime_journal::flap_term_of(p).is_some() => {}
            (RecordKind::Canonical, Some(b'M')) if p.len() == 77 => {}
            // Phase H: the strategy's blocks, Flap launches and resyncs.
            (RecordKind::Canonical, Some(b'B'))
                if p.len() == crate::runtime_strategy::BLOCK_LEN => {}
            (RecordKind::Canonical, Some(b'N'))
                if p.len() == crate::runtime_strategy::LAUNCH_LEN => {}
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
            (RecordKind::Runtime, Some(b'S' | b'L' | b'D' | b'X' | b'Q' | b'v')) => {
                window.unparsed_records += 1
            }
            (RecordKind::Canonical | RecordKind::Reorg, _) => window.unparsed_records += 1,
            _ => {}
        }
    };
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
    Ok(Replay {
        window,
        launches,
        reverted,
        not_included,
        connectivity,
        later_lags_ns,
    })
}

/// Nearest rank of quantile `q` (per mille) when `n >= 1000 / (1000 - q)`.
fn percentile<T: Copy>(sorted: &[T], q_per_mille: u64) -> Option<T> {
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
    for (started, name) in names {
        let path = dir.join(&name);
        let is_active = active.is_some_and(|a| a == path.as_path());
        let r = replay(&path, name, started, max_payload, is_active)?;
        // Nothing of an INVALID window counts (Phase F gate L3): its frames
        // are not trusted.
        if r.window.state != "INVALID" {
            out.covered_ms = out.covered_ms.saturating_add(r.window.duration_ms);
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
                    _ => &mut out.four_clock_unknown,
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
                _ => out.clock_unknown += 1,
            }
            match l.external_buys {
                Some(n) => {
                    out.contested_launches += u64::from(n > 0);
                    out.external_buys += u64::from(n);
                }
                None => out.buys_unknown += 1,
            }
        }
        later.extend(r.later_lags_ns.iter().map(|ns| ns / 1_000));
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
                .set(e::INGRESS_DROPPED, w.ingress_dropped);
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
            .set(cs::EXTERNAL_BUYS, self.external_buys);
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
             four before the block time: full={} hash_only={} after={} missed={} clock_unknown={}\n\
             later sightings={} lag_us p50={} p95={} max={}\n\
             before the block time: full={} (c1_scope {}) hash_only={} after={} missed={} clock_unknown={}\n\
             lead_us p50={} p95={} p99={} max={} n={}\n\
             fetch_lag_ms min={} p50={} max={} clock_suspect={}\n\
             contested={} external_buys={} buys_unknown={} criteria={}\n",
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
            self.buys_unknown,
            CRITERIA_UNCONFIRMED,
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
mod tests {
    use super::*;
    use crate::canonical_join::{Joined, LaunchRef, Visibility};
    use crate::runtime_journal::{
        EXIT_PAYLOAD, RuntimeJournal, SourceEvent, canonical_launch_payload,
        orphaned_launch_payload, pending_launch_payload, reorg_payload, resync_payload,
        source_event_payload, window_payload,
    };

    const BLOCK_MS: u64 = 1_792_000_000_000;

    /// Phase F gate N1: a source's connected time and interruptions inside
    /// a window, from its transitions (repeats and later events ignored).
    #[test]
    fn continuity_counts_the_connected_time_inside_the_window() {
        let mut c = Connectivity::default();
        // Connected before the window, lost at 30, back at 40 (twice), rate
        // limited at 70, back at 80; stopped at 120, after the window.
        for (at, event) in [
            (5, 3),
            (30, 4),
            (40, 3),
            (45, 3),
            (70, 5),
            (80, 3),
            (120, 2),
        ] {
            c.event(0, event, at);
        }
        c.sort();
        assert_eq!(c.continuity(0, 10, 100), (20 + 30 + 20, 2));
        // Never connected, or another source: nothing.
        assert_eq!(c.continuity(1, 10, 100), (0, 0));
        // A window inside one connected span.
        assert_eq!(c.continuity(0, 41, 69), (28, 0));
        // An unknown source index.
        assert_eq!(c.continuity(9, 0, 100), (0, 0));
    }

    fn launch(venue: Venue, block: u8, tx: u8, buys: Option<u32>, seen: u64) -> LaunchRef {
        LaunchRef {
            venue,
            block_number: u64::from(block),
            block_hash: [block; 32],
            tx_index: 0,
            tx_hash: [tx; 32],
            token: None,
            external_buys: buys,
            c1_scope: tx != 3,
            venue_target: true,
            four_token: if venue == Venue::Flap {
                crate::canonical_join::FourToken::NotApplicable
            } else {
                crate::canonical_join::FourToken::NotRecognized
            },
            // Unverifiable launches: covered by the runtime test's census.
            tx: crate::canonical_join::LaunchTx::Verified,
            block_ms: BLOCK_MS,
            block_seen_ns: seen,
            block_seen_unix_ms: Some(BLOCK_MS + u64::from(tx) * 100),
        }
    }

    /// A journal of an older build: one `<started>.dbjnl` file (the
    /// layout the census keeps reading).
    fn write(
        dir: &Path,
        started: u64,
        records: &[(RecordKind, u64, Vec<u8>)],
    ) -> std::path::PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{started}.dbjnl"));
        let mut w = darkbot_bsc_journal::JournalWriter::new_v2(
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap(),
        );
        for (sequence, (kind, at, payload)) in records.iter().enumerate() {
            w.append(&CaptureRecord {
                sequence: sequence as u64,
                kind: *kind,
                timestamp_ns: *at,
                payload: payload.clone().into_boxed_slice(),
            })
            .unwrap();
        }
        w.commit_file().unwrap();
        path
    }

    /// A segmented run of the production writer.
    fn write_run(
        dir: &Path,
        started: u64,
        records: &[(RecordKind, u64, Vec<u8>)],
        segment_bytes: u64,
    ) -> std::path::PathBuf {
        let setup = crate::runtime_journal::JournalSetup {
            segment: darkbot_bsc_journal::storage::segments::SegmentPolicy {
                max_segment_bytes: segment_bytes,
                max_segment_ns: u64::MAX,
            },
            ..crate::runtime_journal::JournalSetup::whole(64, 4096)
        };
        let mut j = RuntimeJournal::open(dir, started, 1, setup).unwrap();
        for (kind, at, payload) in records {
            assert!(j.record(*kind, *at, payload.clone()));
        }
        let path = j.path().to_path_buf();
        j.finish().unwrap();
        path
    }

    /// Phase E review L1: a short `C` record and launch records of an
    /// unknown venue are counted as unparsed, never a panic and never
    /// silently skipped (before: a one-byte `C` payload indexed past its end
    /// and panicked the census; unknown-venue `P`/`O` records vanished).
    #[test]
    fn short_and_unknown_venue_launch_records_are_unparsed() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-short-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut pending = pending_launch_payload(&launch(Venue::Flap, 10, 1, None, 100));
        pending[1] = 9;
        let mut orphan = orphaned_launch_payload(&launch(Venue::Flap, 10, 2, None, 100));
        orphan[1] = 9;
        let records = vec![
            (
                RecordKind::Canonical,
                1,
                window_payload(b'H', 10, &[10; 32]),
            ),
            (RecordKind::Canonical, 2, vec![b'C']),
            (RecordKind::Canonical, 3, pending),
            (RecordKind::Canonical, 4, orphan),
            (
                RecordKind::Canonical,
                5,
                window_payload(b'E', 10, &[10; 32]),
            ),
            (RecordKind::Runtime, 6, EXIT_PAYLOAD.to_vec()),
        ];
        write(&dir, 1_000, &records);
        let c = census(&dir, 4096, None).unwrap();
        assert_eq!(c.windows[0].unparsed_records, 3);
        assert_eq!(
            (c.flap_launches, c.four_launches, c.orphaned_launches),
            (0, 0, 0)
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase F §10 (Phase E review L2): the Four verdict rides in the `C`
    /// record and is counted per verdict; a 136-byte record of an older
    /// build counts as unrecorded, an unknown verdict byte as unparsed.
    #[test]
    fn four_verdicts_replay_from_the_canonical_record() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-verdict-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let with = |venue, tx, four| LaunchRef {
            four_token: four,
            ..launch(venue, 10, tx, None, 100)
        };
        let c = |l: LaunchRef| {
            canonical_launch_payload(&Joined {
                launch: l,
                visibility: Visibility::Missed,
            })
        };
        let mut old = c(with(Venue::Four, 5, FourToken::Matched));
        old.truncate(CANONICAL_LAUNCH_LEN_V1);
        // A record of the build before the venue-target flag: still read.
        let mut v2 = c(with(Venue::Four, 9, FourToken::Matched));
        v2.truncate(CANONICAL_LAUNCH_LEN_V2);
        let mut bad = c(with(Venue::Four, 6, FourToken::Matched));
        bad[CANONICAL_LAUNCH_LEN_V1] = 9;
        // Venue and verdict disagree: a Flap record with a Four verdict, a
        // Four record without one.
        let flap_with = c(with(Venue::Flap, 7, FourToken::Matched));
        let four_without = c(with(Venue::Four, 8, FourToken::NotApplicable));
        let mut records = vec![(
            RecordKind::Canonical,
            1,
            window_payload(b'H', 10, &[10; 32]),
        )];
        for p in [
            c(with(Venue::Four, 1, FourToken::Matched)),
            c(with(Venue::OpenFour, 2, FourToken::Unknown)),
            c(with(Venue::OpenFour, 3, FourToken::Mismatch)),
            c(with(Venue::Four, 4, FourToken::NotRecognized)),
            old,
            v2,
            bad,
            flap_with,
            four_without,
        ] {
            records.push((RecordKind::Canonical, 100, p));
        }
        // Two later sightings: the spread across sources.
        for (i, lag) in [3_000_000u64, 1_000_000].into_iter().enumerate() {
            records.push((
                RecordKind::Runtime,
                200,
                crate::runtime_journal::later_sighting_payload(
                    SourceId::PublicNode,
                    &[i as u8; 32],
                    lag,
                ),
            ));
        }
        // Phase E review L3: one drift guard probe found a drift, one a match.
        for (code, block) in [(2, 99), (1, 100)] {
            records.push((
                RecordKind::Runtime,
                300,
                crate::runtime_journal::four_probe_payload(code, block),
            ));
        }
        records.push((
            RecordKind::Canonical,
            400,
            window_payload(b'E', 10, &[10; 32]),
        ));
        records.push((RecordKind::Runtime, 401, EXIT_PAYLOAD.to_vec()));
        write(&dir, 1_000, &records);
        let c = census(&dir, 4096, None).unwrap();
        assert_eq!(
            (
                c.windows[0].four_drift_probes,
                c.windows[0].four_match_probes
            ),
            (1, 1)
        );
        assert_eq!(
            (
                c.four_launches,
                c.four_derived,
                c.four_no_derivation,
                c.four_mismatch,
                c.four_not_recognized,
                c.four_verdict_unrecorded,
                c.unparsed_records
            ),
            (6, 2, 1, 1, 1, 1, 3)
        );
        // Phase F §10: Four coverage per class (all six missed here).
        assert_eq!(
            (c.four_full_before, c.four_missed, c.full_before, c.missed),
            (0, 6, 0, 0)
        );
        assert_eq!(
            (c.later_sightings, c.later_lag_p50_us, c.later_lag_max_us),
            (2, Some(1_000), Some(3_000))
        );
        let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
        assert_eq!(record.u64(cs::FOUR_DERIVED).unwrap(), 2);
        assert_eq!(record.u64(cs::FOUR_MISSED).unwrap(), 6);
        assert!(
            c.summary()
                .contains("derived=2 candidate=0 no_derivation=1 mismatch=1")
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase E: OpenFour launches (journal venue byte 3) replay with the
    /// Four family, never as Flap; an orphaned one leaves the count.
    #[test]
    fn openfour_launches_replay_with_the_four_family() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-openfour-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let classic = launch(Venue::Four, 10, 1, None, 100);
        let open = launch(Venue::OpenFour, 10, 2, None, 100);
        let orphan = launch(Venue::OpenFour, 11, 4, None, 200);
        let joined = |l: LaunchRef| {
            (
                RecordKind::Canonical,
                l.block_seen_ns,
                canonical_launch_payload(&Joined {
                    launch: l,
                    visibility: Visibility::Missed,
                }),
            )
        };
        let mut records = vec![(
            RecordKind::Canonical,
            1,
            window_payload(b'H', 10, &[10; 32]),
        )];
        for l in [classic, open, orphan] {
            records.push((
                RecordKind::Canonical,
                l.block_seen_ns,
                pending_launch_payload(&l),
            ));
            records.push(joined(l));
        }
        records.push((RecordKind::Canonical, 300, orphaned_launch_payload(&orphan)));
        records.push((
            RecordKind::Canonical,
            400,
            window_payload(b'E', 11, &[11; 32]),
        ));
        records.push((RecordKind::Runtime, 401, EXIT_PAYLOAD.to_vec()));
        assert_eq!(
            canonical_launch_payload(&Joined {
                launch: open,
                visibility: Visibility::Missed
            })[1],
            3
        );
        write(&dir, 1_000, &records);
        let c = census(&dir, 4096, None).unwrap();
        assert_eq!(
            (
                c.flap_launches,
                c.four_launches,
                c.orphaned_launches,
                c.c1_scope_launches
            ),
            (0, 2, 1, 0)
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_census_replays_classes_scope_coverage_and_window_states() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-census-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let p = SourceId::PublicNode;
        let joined = |l: LaunchRef, v: Visibility| {
            (
                RecordKind::Canonical,
                l.block_seen_ns,
                canonical_launch_payload(&Joined {
                    launch: l,
                    visibility: v,
                }),
            )
        };
        let pending = |l: LaunchRef| {
            (
                RecordKind::Canonical,
                l.block_seen_ns,
                pending_launch_payload(&l),
            )
        };
        let full = |lead| Visibility::FullBefore {
            source: p,
            lead_us: lead,
        };
        let l1 = launch(Venue::Flap, 10, 1, Some(2), 100);
        let l2 = launch(Venue::Flap, 11, 2, Some(0), 200);
        let l3 = launch(Venue::Flap, 11, 3, Some(1), 200);
        let l4 = launch(Venue::Four, 11, 4, None, 200);
        let l5 = launch(Venue::Flap, 12, 5, None, 400);
        let l6 = launch(Venue::Flap, 13, 6, Some(0), 500);
        let l7 = launch(Venue::Flap, 14, 7, Some(0), 550);
        let mut records = vec![
            (
                RecordKind::Canonical,
                1,
                window_payload(b'H', 10, &[10; 32]),
            ),
            (
                RecordKind::Runtime,
                5,
                source_event_payload(p, SourceEvent::Connected),
            ),
            pending(l1),
            joined(l1, full(3_000)),
            pending(l2),
            pending(l3),
            pending(l4),
            joined(l2, full(1_000)),
            joined(
                l3,
                Visibility::HashOnlyBefore {
                    source: p,
                    lead_us: 5,
                },
            ),
            joined(l4, Visibility::Missed),
            (
                RecordKind::Runtime,
                300,
                source_event_payload(p, SourceEvent::Disconnected),
            ),
            pending(l5),
            joined(l5, Visibility::ClockUnknown { source: p }),
            pending(l6),
            joined(
                l6,
                Visibility::After {
                    source: p,
                    lag_us: 9,
                },
            ),
            (RecordKind::Reorg, 600, reorg_payload(1, 13, &[99; 32])),
            (RecordKind::Canonical, 600, orphaned_launch_payload(&l6)),
            (RecordKind::Reorg, 700, resync_payload(13, 20)),
            // Received, never joined: the process ends before its join.
            pending(l7),
            (
                RecordKind::Canonical,
                800,
                window_payload(b'K', 20, &[20; 32]),
            ),
            (
                RecordKind::Canonical,
                2_000_000_000,
                window_payload(b'E', 21, &[21; 32]),
            ),
            (RecordKind::Runtime, 2_000_000_001, EXIT_PAYLOAD.to_vec()),
        ];
        write(&dir, 1_000, &records);
        // A killed process: its records up to the checkpoint, no end record.
        records.truncate(20);
        write(&dir, 2_000, &records);
        // A window whose last frame was cut.
        let cut = write(&dir, 3_000, &records[..4]);
        let bytes = fs::read(&cut).unwrap();
        fs::write(&cut, &bytes[..bytes.len() - 5]).unwrap();
        // A corrupt frame: nothing of it counts.
        let bad = write(&dir, 4_000, &records[..4]);
        let mut bytes = fs::read(&bad).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&bad, &bytes).unwrap();
        fs::write(dir.join("notes.txt"), b"ignored").unwrap();

        let c = census(&dir, 4096, None).unwrap();
        let states: Vec<_> = c.windows.iter().map(|w| w.state).collect();
        assert_eq!(
            states,
            ["COMPLETE", "UNTERMINATED", "TAIL_INCOMPLETE", "INVALID"]
        );
        let (w0, w1) = (&c.windows[0], &c.windows[1]);
        assert_eq!(
            (
                w0.first_block,
                w0.last_block,
                w0.reorgs,
                w0.resyncs,
                w0.blocks_skipped,
                w0.unjoined
            ),
            (Some(10), Some(21), 1, 1, 8, 1)
        );
        // The killed window ends at its checkpoint.
        assert_eq!((w1.last_block, w1.unjoined), (Some(20), 1));
        // Flap: w0 l1 l2 l3 l5 (l6 orphaned), w1 the same, w2 nothing (l1's
        // record is the cut frame).
        assert_eq!(
            (c.flap_launches, c.four_launches, c.orphaned_launches),
            (8, 2, 2)
        );
        assert_eq!(
            (
                c.full_before,
                c.hash_only_before,
                c.after,
                c.missed,
                c.clock_unknown
            ),
            (4, 2, 0, 0, 2)
        );
        // l3 is not of the C1 scope.
        assert_eq!((c.c1_scope_launches, c.c1_scope_full_before), (6, 4));
        // One unjoined launch per window: l7 twice, and l1 of the cut window
        // (its join is the cut frame).
        assert_eq!((c.unjoined, c.blocks_skipped), (3, 16));
        // Block 12 came while PublicNode was disconnected (in both windows).
        assert_eq!(c.outage_launches, 2);
        assert_eq!(
            (c.contested_launches, c.external_buys, c.buys_unknown),
            (4, 6, 2)
        );
        assert_eq!(
            (c.lead_max_us, c.lead_p50_us, c.lead_p95_us),
            (Some(3_000), Some(1_000), None)
        );
        assert_eq!(
            (c.fetch_lag_ms_min, c.fetch_lag_ms_max),
            (Some(100), Some(500))
        );
        assert!(!c.clock_suspect);
        assert!(c.first_full_before.contains(&(p, 4)));
        let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
        assert_eq!(record.text(cs::CRITERIA).unwrap(), CRITERIA_UNCONFIRMED);
        assert!(c.summary().contains("c1_scope=6"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn running_unparsed_and_between_window_coverage_are_explicit() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-running-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        // A finished window ending at its checkpoint 15, with one record of
        // the older 118-byte `C` layout of 25.10. (it cannot be read).
        let mut old = vec![b'C', 1];
        old.resize(118, 0);
        write(
            &dir,
            1,
            &[
                (
                    RecordKind::Canonical,
                    1,
                    window_payload(b'H', 10, &[10; 32]),
                ),
                (RecordKind::Canonical, 2, old),
                (
                    RecordKind::Canonical,
                    3,
                    window_payload(b'K', 15, &[15; 32]),
                ),
                (RecordKind::Runtime, 4, EXIT_PAYLOAD.to_vec()),
            ],
        );
        // The journal a runtime still writes: from 40, one launch inside its
        // join delay.
        let l = launch(Venue::Flap, 40, 1, Some(0), 1);
        let running = write(
            &dir,
            2,
            &[
                (
                    RecordKind::Canonical,
                    1,
                    window_payload(b'H', 40, &[40; 32]),
                ),
                (RecordKind::Canonical, 2, pending_launch_payload(&l)),
            ],
        );
        let c = census(&dir, 4096, Some(&running)).unwrap();
        let states: Vec<_> = c.windows.iter().map(|w| w.state).collect();
        assert_eq!(states, ["COMPLETE", "RUNNING"]);
        assert_eq!(
            (c.windows[0].last_block, c.windows[0].unparsed_records),
            (Some(15), 1)
        );
        assert_eq!((c.windows[1].pending_join, c.windows[1].unjoined), (1, 0));
        assert_eq!(
            (c.unjoined, c.unparsed_records, c.blocks_between_windows),
            (0, 1, 24)
        );
        // Offline (no active journal) the same window reads as killed.
        let offline = census(&dir, 4096, None).unwrap();
        assert_eq!(offline.windows[1].state, "UNTERMINATED");
        assert_eq!(offline.unjoined, 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase F: a segmented run is read across its segments as one journal
    /// and checked against its manifest. Without a manifest a clean run is
    /// `UNSEALED`; a sealed segment replaced by another valid one (frames
    /// and sequence intact) differs from the manifest and makes the run
    /// `INVALID`; a crash's torn open last segment is `TAIL_INCOMPLETE`, as
    /// for one file.
    #[test]
    fn segmented_runs_are_read_across_segments_and_sealed_by_their_manifest() {
        let base = std::env::temp_dir().join(format!("bsc-g04-runs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (dir, donor) = (base.join("journal"), base.join("donor"));
        let records = |shift: u64| {
            let mut r = vec![(
                RecordKind::Canonical,
                1 + shift,
                window_payload(b'H', 10, &[10; 32]),
            )];
            for i in 0..20u64 {
                r.push((
                    RecordKind::Canonical,
                    2 + i + shift,
                    window_payload(b'K', 10 + i, &[10 + i as u8; 32]),
                ));
            }
            r.push((
                RecordKind::Canonical,
                30 + shift,
                window_payload(b'E', 30, &[30; 32]),
            ));
            r.push((RecordKind::Runtime, 31 + shift, EXIT_PAYLOAD.to_vec()));
            r
        };
        let complete = write_run(&dir, 1_000, &records(0), 200);
        let segments = |run: &Path| {
            let mut s: Vec<_> = fs::read_dir(run)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|e| e == "dbjnl"))
                .collect();
            s.sort();
            s
        };
        assert!(segments(&complete).len() >= 4);
        let unsealed = write_run(&dir, 2_000, &records(0), 200);
        fs::remove_file(unsealed.join(RUN_MANIFEST)).unwrap();
        let changed = write_run(&dir, 3_000, &records(0), 200);
        let other = write_run(&donor, 3_000, &records(1), 200);
        fs::copy(&segments(&other)[1], &segments(&changed)[1]).unwrap();
        let crashed = write_run(&dir, 4_000, &records(0)[..10], 200);
        fs::remove_file(crashed.join(RUN_MANIFEST)).unwrap();
        let last = segments(&crashed).pop().unwrap();
        let open = last.with_extension("dbjnl.open");
        fs::rename(&last, &open).unwrap();
        let len = fs::metadata(&open).unwrap().len();
        fs::OpenOptions::new()
            .write(true)
            .open(&open)
            .unwrap()
            .set_len(len - 3)
            .unwrap();

        let c = census(&dir, 4096, None).unwrap();
        let states: Vec<_> = c.windows.iter().map(|w| w.state).collect();
        assert_eq!(
            states,
            ["COMPLETE", "UNSEALED", "INVALID", "TAIL_INCOMPLETE"]
        );
        assert_eq!(
            (c.windows[0].first_block, c.windows[0].last_block),
            (Some(10), Some(30))
        );
        assert_eq!(c.windows[0].journal, "1000");
        // Nothing of the changed run counts.
        assert_eq!(
            (c.windows[2].first_block, c.windows[2].last_block),
            (None, None)
        );
        // The crashed run counts up to its last verified frame (checkpoint).
        assert_eq!(c.windows[3].first_block, Some(10));
        // Unchanged, the donor run reads COMPLETE: the segment copy alone
        // made the other INVALID.
        let d = census(&donor, 4096, None).unwrap();
        assert_eq!(d.windows[0].state, "COMPLETE");
        fs::remove_dir_all(&base).unwrap();
    }

    /// Phase F gate H1/L3/L6/M2: ingress drops (`D`, cumulative per
    /// source) reach the window and the census; a malformed runtime record
    /// is unparsed, never silently skipped; nothing of an INVALID window
    /// counts (covered time included); a hidden probe directory is no run.
    #[test]
    fn ingress_drops_and_damaged_records_are_counted_and_invalid_time_is_not() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-drops-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let d = |source, total| {
            (
                RecordKind::Runtime,
                5,
                crate::runtime_journal::ingress_drop_payload(source, total),
            )
        };
        write(
            &dir,
            1_000,
            &[
                (
                    RecordKind::Canonical,
                    1,
                    window_payload(b'H', 10, &[10; 32]),
                ),
                d(SourceId::PublicNode, 3),
                d(SourceId::PublicNode, 5),
                d(SourceId::Drpc, 2),
                (RecordKind::Runtime, 6, vec![b'S', 1]),
                (
                    RecordKind::Canonical,
                    2_000_000_000,
                    window_payload(b'E', 11, &[11; 32]),
                ),
                (RecordKind::Runtime, 2_000_000_001, EXIT_PAYLOAD.to_vec()),
            ],
        );
        // A corrupt window of 9 s: none of its time counts.
        let bad = write(
            &dir,
            2_000,
            &[
                (
                    RecordKind::Canonical,
                    1,
                    window_payload(b'H', 20, &[20; 32]),
                ),
                (
                    RecordKind::Canonical,
                    9_000_000_000,
                    window_payload(b'K', 21, &[21; 32]),
                ),
            ],
        );
        let mut bytes = fs::read(&bad).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&bad, &bytes).unwrap();
        fs::create_dir_all(dir.join(".capture-preflight-1-1")).unwrap();
        let c = census(&dir, 4096, None).unwrap();
        let states: Vec<_> = c.windows.iter().map(|w| w.state).collect();
        assert_eq!(states, ["COMPLETE", "INVALID"]);
        assert_eq!((c.windows[0].ingress_dropped, c.ingress_dropped), (7, 7));
        assert_eq!((c.windows[0].unparsed_records, c.unparsed_records), (1, 1));
        assert_eq!(c.covered_ms, 2_000);
        let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
        assert_eq!(record.u64(cs::INGRESS_DROPPED).unwrap(), 7);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase F gate M3/M4: Four Classic and OpenFour apart; actionable
    /// only when seen in full before the block with the derived token;
    /// reverted launch calls per venue, a reorg taking one out again (not an
    /// orphaned launch); a journaled launch sighting never included.
    #[test]
    fn per_venue_actionability_reverts_and_unincluded_sightings_are_counted() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-venues-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let joined = |venue, tx, four, full_before: bool| {
            let launch = LaunchRef {
                four_token: four,
                ..launch(venue, 10, tx, None, 100)
            };
            (
                RecordKind::Canonical,
                100,
                canonical_launch_payload(&Joined {
                    launch,
                    visibility: if full_before {
                        Visibility::FullBefore {
                            source: SourceId::PublicNode,
                            lead_us: 7,
                        }
                    } else {
                        Visibility::Missed
                    },
                }),
            )
        };
        let id = |tag, venue, tx: u8| {
            (
                RecordKind::Canonical,
                150,
                crate::runtime_journal::launch_id_payload(tag, venue, 10, &[10; 32], &[tx; 32]),
            )
        };
        let sighting = |tx: u8| {
            (
                RecordKind::RawSignedTx,
                50,
                crate::runtime_journal::first_sighting_payload(
                    SourceId::PublicNode,
                    &[tx; 32],
                    b"raw",
                ),
            )
        };
        let records = vec![
            (
                RecordKind::Canonical,
                1,
                window_payload(b'H', 10, &[10; 32]),
            ),
            sighting(1),
            sighting(9),
            joined(Venue::Four, 1, FourToken::Matched, true),
            joined(Venue::OpenFour, 2, FourToken::Matched, true),
            joined(Venue::OpenFour, 3, FourToken::Unknown, true),
            joined(Venue::OpenFour, 4, FourToken::Matched, false),
            id(b'V', Venue::Flap, 7),
            id(b'V', Venue::Four, 8),
            id(b'O', Venue::Four, 8),
            (
                RecordKind::Canonical,
                400,
                window_payload(b'E', 11, &[11; 32]),
            ),
            (RecordKind::Runtime, 401, EXIT_PAYLOAD.to_vec()),
        ];
        write(&dir, 1_000, &records);
        let c = census(&dir, 4096, None).unwrap();
        assert_eq!(
            (c.classic_launches, c.openfour_launches, c.four_launches),
            (1, 3, 4)
        );
        assert_eq!(
            (
                c.four_actionable,
                c.classic_actionable,
                c.openfour_actionable
            ),
            (2, 1, 1)
        );
        assert_eq!(
            (
                c.reverted_flap,
                c.reverted_classic,
                c.reverted_openfour,
                c.orphaned_launches
            ),
            (1, 0, 0, 0)
        );
        assert_eq!(c.launches_not_included, 1);
        assert_eq!(c.four_lead_max_us, Some(7));
        let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
        assert_eq!(record.u64(cs::FOUR_ACTIONABLE).unwrap(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase F gate L6: a sealed run is read with the payload bound it was
    /// written with, though the configuration lowered it since.
    #[test]
    fn a_sealed_run_reads_with_its_own_payload_bound() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-bound-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_run(
            &dir,
            1_000,
            &[
                (
                    RecordKind::Canonical,
                    1,
                    window_payload(b'H', 10, &[10; 32]),
                ),
                (RecordKind::Runtime, 2, vec![b'Z'; 3_000]),
                (
                    RecordKind::Canonical,
                    3,
                    window_payload(b'E', 11, &[11; 32]),
                ),
                (RecordKind::Runtime, 4, EXIT_PAYLOAD.to_vec()),
            ],
            u64::MAX,
        );
        let c = census(&dir, 1_024, None).unwrap();
        assert_eq!(c.windows[0].state, "COMPLETE");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_negative_fetch_lag_marks_the_local_clock_suspect() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-skew-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut l = launch(Venue::Flap, 1, 1, Some(0), 1);
        l.block_seen_unix_ms = Some(BLOCK_MS - 40);
        let payload = canonical_launch_payload(&Joined {
            launch: l,
            visibility: Visibility::Missed,
        });
        write(&dir, 1, &[(RecordKind::Canonical, 1, payload)]);
        let c = census(&dir, 4096, None).unwrap();
        assert_eq!((c.fetch_lag_ms_min, c.clock_suspect), (Some(-40), true));
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Phase H: the Flap trades (`T`) and refused trade logs (`W`) of a
    /// window are counted, never unparsed; a damaged `T` is.
    #[test]
    fn flap_trades_and_trade_gaps_are_counted() {
        let dir = std::env::temp_dir().join(format!("bsc-g04-trades-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let trade = crate::chain_follower::CanonicalTrade {
            block_number: 10,
            block_hash: [10; 32],
            tx_index: 2,
            tx_hash: [2; 32],
            log_index: 7,
            origin: [3; 20],
            sell: true,
            token: [4; 20],
            trader: [5; 20],
            amount: [6; 32],
            eth: [7; 32],
            fee: [8; 32],
            post_price: [9; 32],
        };
        let t = crate::runtime_journal::flap_trade_payload(&trade);
        assert_eq!(crate::runtime_journal::flap_trade_of(&t), Some(trade));
        let mut damaged = t.clone();
        damaged[1] = 2;
        assert_eq!(crate::runtime_journal::flap_trade_of(&damaged), None);
        // Every term kind reads back; a version with a dirty word does not.
        use crate::chain_follower::{CanonicalTerm, FlapTerm};
        for term in [
            FlapTerm::Version(6),
            FlapTerm::QuoteToken([1; 20]),
            FlapTerm::Curve {
                r: [1; 32],
                h: [2; 32],
                k: [3; 32],
            },
            FlapTerm::DexThreshold([4; 32]),
            FlapTerm::Migrator(1),
            FlapTerm::Tax {
                buy: [5; 32],
                sell: [6; 32],
            },
        ] {
            let t = CanonicalTerm {
                block_number: 10,
                block_hash: [10; 32],
                tx_index: 1,
                log_index: 3,
                token: [4; 20],
                term,
            };
            let p = crate::runtime_journal::flap_term_payload(&t);
            assert_eq!(p.len(), crate::runtime_journal::FLAP_TERM_LEN);
            assert_eq!(crate::runtime_journal::flap_term_of(&p), Some(t));
        }
        let mut dirty = crate::runtime_journal::flap_term_payload(&CanonicalTerm {
            block_number: 10,
            block_hash: [10; 32],
            tx_index: 1,
            log_index: 3,
            token: [4; 20],
            term: FlapTerm::Version(6),
        });
        dirty[78] = 1;
        assert_eq!(crate::runtime_journal::flap_term_of(&dirty), None);
        write(
            &dir,
            1_000,
            &[
                (
                    RecordKind::Canonical,
                    1,
                    window_payload(b'H', 10, &[10; 32]),
                ),
                (RecordKind::Canonical, 2, t),
                (
                    RecordKind::Canonical,
                    3,
                    crate::runtime_journal::trade_gap_payload(10, &[10; 32], 3, 9),
                ),
                (RecordKind::Canonical, 4, damaged),
                (
                    RecordKind::Canonical,
                    5,
                    window_payload(b'E', 10, &[10; 32]),
                ),
                (RecordKind::Runtime, 6, EXIT_PAYLOAD.to_vec()),
            ],
        );
        let c = census(&dir, 4096, None).unwrap();
        let w = &c.windows[0];
        assert_eq!((w.flap_trades, w.trade_gaps, w.unparsed_records), (1, 1, 1));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn percentiles_need_their_sample_count() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 500), Some(50));
        assert_eq!(percentile(&v, 990), Some(99));
        assert_eq!(percentile(&v[..99], 990), None);
        assert_eq!(percentile(&v[..20], 950), Some(19));
        assert_eq!(percentile::<u64>(&[], 500), None);
    }
}
