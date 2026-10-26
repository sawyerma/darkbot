//! The G0.4/V10-8 market census: a replay of the runtime journals
//! (`runtime_journal`) into one `darkbot.bsc.g04-census.v1` record.
//!
//! Every number comes from verified journal frames. A journal whose frames
//! end in a partial one (still written, or cut) counts up to its last
//! verified frame and says `TAIL_INCOMPLETE`; one without the clean end
//! record (`X`) says `UNTERMINATED` (killed or crashed) and its covered
//! window ends at its last checkpoint; a corrupt frame makes the whole
//! journal `INVALID` and nothing of it counts. The journal the running
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

use crate::canonical_join::Venue;
use crate::runtime_journal::{CANONICAL_LAUNCH_LEN, venue_of_byte};
use crate::sources::SourceId;
use darkbot_bsc_journal::{JournalError, JournalReader, RecordKind};
use darkbot_bsc_record::Draft;
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
struct LaunchRecord {
    flap: bool,
    block_hash: [u8; 32],
    tx_hash: [u8; 32],
    external_buys: Option<u32>,
    class: u8,
    source: Option<usize>,
    delta_us: u64,
    c1_scope: bool,
    tx_unverifiable: bool,
    tx_answer_refused: bool,
    fetch_lag_ms: Option<i64>,
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

fn launch_record(p: &[u8], seen_ns: u64) -> Option<LaunchRecord> {
    // Every venue the writer knows (Phase E: OpenFour is byte 3).
    let venue = venue_of_byte(p[1]).filter(|_| p.len() == CANONICAL_LAUNCH_LEN && p[0] == b'C')?;
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
        flap: venue == Venue::Flap,
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
    connectivity: Connectivity,
}

fn replay(
    path: &Path,
    name: String,
    started: u64,
    max_payload: usize,
    active: bool,
) -> Result<Replay, String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = JournalReader::new(std::io::BufReader::new(file), max_payload)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut window = Window {
        journal: name,
        started_unix_ms: started,
        ..Window::default()
    };
    let mut launches: Vec<LaunchRecord> = Vec::new();
    let mut connectivity = Connectivity::default();
    // Launches received (`P`) and resolved (`C`/`O`), by (block, tx).
    let mut received: HashSet<([u8; 32], [u8; 32])> = HashSet::new();
    let mut resolved: HashSet<([u8; 32], [u8; 32])> = HashSet::new();
    let mut exited = false;
    let mut checkpoint = None;
    let mut last_ns = 0u64;
    let state = loop {
        let record = match reader.next_record() {
            Ok(Some(r)) => r,
            Ok(None) if exited => break "COMPLETE",
            Ok(None) | Err(JournalError::Truncated) if active => break "RUNNING",
            Ok(None) => break "UNTERMINATED",
            Err(JournalError::Truncated) => break "TAIL_INCOMPLETE",
            Err(_) => break "INVALID",
        };
        last_ns = last_ns.max(record.timestamp_ns);
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
            (RecordKind::Canonical, Some(b'P')) if p.len() == 74 => {
                if let (Some(block), Some(tx), Some(_)) =
                    (hash_at(p, 10), hash_at(p, 42), venue_of_byte(p[1]))
                {
                    received.insert((block, tx));
                }
            }
            (RecordKind::Canonical, Some(b'C')) => match launch_record(p, record.timestamp_ns) {
                Some(l) => {
                    resolved.insert((l.block_hash, l.tx_hash));
                    launches.push(l);
                }
                None => window.unparsed_records += 1,
            },
            (RecordKind::Canonical, Some(b'O')) if p.len() == 74 => {
                if let (Some(block), Some(tx), Some(venue)) =
                    (hash_at(p, 10), hash_at(p, 42), venue_of_byte(p[1]))
                {
                    resolved.insert((block, tx));
                    launches.retain(|l| !(l.block_hash == block && l.tx_hash == tx));
                    // Counted as orphaned (class 0), joined or not.
                    launches.push(LaunchRecord {
                        flap: venue == Venue::Flap,
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
                        seen_ns: record.timestamp_ns,
                    });
                }
            }
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
            (RecordKind::Canonical | RecordKind::Reorg, _) => window.unparsed_records += 1,
            _ => {}
        }
    };
    window.state = state;
    if state == "INVALID" {
        window.first_block = None;
        window.last_block = None;
        launches.clear();
        connectivity = Connectivity::default();
    } else {
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
    Ok(Replay {
        window,
        launches,
        connectivity,
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

/// Replays every `*.dbjnl` of `dir` (name `<started_unix_ms>.dbjnl`);
/// `active` names the journal the running runtime still writes.
pub fn census(dir: &Path, max_payload: usize, active: Option<&Path>) -> Result<Census, String> {
    let mut names: Vec<(u64, String)> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".dbjnl") else {
            continue;
        };
        let started: u64 = stem
            .parse()
            .map_err(|_| format!("{name}: not <started_unix_ms>.dbjnl"))?;
        names.push((started, name));
    }
    names.sort();
    let mut out = Census::default();
    let mut leads: Vec<u64> = Vec::new();
    let mut lags: Vec<i64> = Vec::new();
    let mut firsts: HashMap<usize, u64> = HashMap::new();
    for (started, name) in names {
        let path = dir.join(&name);
        let is_active = active.is_some_and(|a| a == path.as_path());
        let r = replay(&path, name, started, max_payload, is_active)?;
        out.covered_ms = out.covered_ms.saturating_add(r.window.duration_ms);
        out.unjoined = out.unjoined.saturating_add(r.window.unjoined);
        out.blocks_skipped = out.blocks_skipped.saturating_add(r.window.blocks_skipped);
        out.unparsed_records = out
            .unparsed_records
            .saturating_add(r.window.unparsed_records);
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
            if !l.flap {
                out.four_launches += 1;
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
        out.windows.push(r.window);
    }
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
                .set(e::UNPARSED_RECORDS, w.unparsed_records);
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
            // Unverifiable launches: covered by the runtime test's census.
            tx: crate::canonical_join::LaunchTx::Verified,
            block_ms: BLOCK_MS,
            block_seen_ns: seen,
            block_seen_unix_ms: Some(BLOCK_MS + u64::from(tx) * 100),
        }
    }

    fn write(
        dir: &Path,
        started: u64,
        records: &[(RecordKind, u64, Vec<u8>)],
    ) -> std::path::PathBuf {
        let mut j = RuntimeJournal::open(dir, started, 64, 4096).unwrap();
        for (kind, at, payload) in records {
            assert!(j.record(*kind, *at, payload));
        }
        let path = j.path().to_path_buf();
        j.finish().unwrap();
        path
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
