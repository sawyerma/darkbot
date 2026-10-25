//! The G0.4/V10-8 market census: a replay of the runtime journals
//! (`runtime_journal`) into one `darkbot.bsc.g04-census.v1` record.
//!
//! Every number comes from verified journal frames. A journal whose frames
//! end in a partial one (still written, or cut) counts up to its last
//! verified frame and says `TAIL_INCOMPLETE`; a corrupt frame makes the
//! whole journal `INVALID` and nothing of it counts. Lost records (gap
//! spans) are reported per window, never filled in.
//!
//! A launch counts in `outage_launches` when no source was connected at the
//! moment the follower had its block (the canonical record's time); it still
//! counts in the denominator and its class. Lead percentiles use the nearest
//! rank and are reported only when the sample count reaches `1 / (1 - q)`.
//! No verdict: `criteria` stays `CRITERIA_UNCONFIRMED` until the user
//! confirmed versioned criteria (order V2 §8).

use crate::sources::SourceId;
use darkbot_bsc_journal::{JournalError, JournalReader, RecordKind};
use darkbot_bsc_record::Draft;
use darkbot_bsc_record::schema::bsc_census::census as cs;
use std::collections::HashMap;
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
    /// Nearest-rank percentiles of the lead (µs) over `full_before`.
    pub lead_p50_us: Option<u64>,
    pub lead_p95_us: Option<u64>,
    pub lead_p99_us: Option<u64>,
    pub lead_max_us: Option<u64>,
    pub contested_launches: u64,
    pub external_buys: u64,
    /// Flap launches whose external buys are unknown (not in the two above).
    pub buys_unknown: u64,
    /// Per source: actionable launches it delivered first.
    pub first_full_before: Vec<(SourceId, u64)>,
}

/// One canonical launch record (`b'C'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LaunchRecord {
    flap: bool,
    block_hash: [u8; 32],
    tx_hash: [u8; 32],
    external_buys: Option<u32>,
    class: u8,
    source: Option<usize>,
    delta_ns: u64,
    seen_ns: u64,
}

fn u64_at(p: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(p.get(at..at + 8)?.try_into().ok()?))
}

fn hash_at(p: &[u8], at: usize) -> Option<[u8; 32]> {
    p.get(at..at + 32)?.try_into().ok()
}

fn launch_record(p: &[u8], seen_ns: u64) -> Option<LaunchRecord> {
    if p.len() != 118 || p[0] != b'C' || !(1..=2).contains(&p[1]) || p[103] > 1 {
        return None;
    }
    let class = p[108];
    if !(1..=4).contains(&class) {
        return None;
    }
    let source = match p[109] {
        0xFF => None,
        i if usize::from(i) < SourceId::ALL.len() => Some(usize::from(i)),
        _ => return None,
    };
    Some(LaunchRecord {
        flap: p[1] == 1,
        block_hash: hash_at(p, 10)?,
        tx_hash: hash_at(p, 50)?,
        external_buys: (p[103] == 1)
            .then(|| p.get(104..108)?.try_into().ok().map(u32::from_le_bytes))
            .flatten(),
        class,
        source,
        delta_ns: u64_at(p, 110)?,
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

/// What one journal contributes; `None` for its launches when it is invalid.
struct Replay {
    window: Window,
    launches: Vec<LaunchRecord>,
    connectivity: Connectivity,
}

fn replay(path: &Path, name: String, started: u64, max_payload: usize) -> Result<Replay, String> {
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = JournalReader::new(std::io::BufReader::new(file), max_payload)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut window = Window {
        journal: name,
        started_unix_ms: started,
        state: "COMPLETE",
        ..Window::default()
    };
    let mut launches: Vec<LaunchRecord> = Vec::new();
    let mut connectivity = Connectivity::default();
    let mut last_ns = 0u64;
    loop {
        let record = match reader.next_record() {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(JournalError::Truncated) => {
                window.state = "TAIL_INCOMPLETE";
                break;
            }
            Err(_) => {
                window.state = "INVALID";
                launches.clear();
                connectivity = Connectivity::default();
                break;
            }
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
            (RecordKind::Canonical, Some(b'C')) => {
                if let Some(l) = launch_record(p, record.timestamp_ns) {
                    launches.push(l);
                }
            }
            (RecordKind::Canonical, Some(b'O')) if p.len() == 74 => {
                if let (Some(block), Some(tx)) = (hash_at(p, 10), hash_at(p, 42)) {
                    launches.retain(|l| !(l.block_hash == block && l.tx_hash == tx));
                    // Counted as orphaned (class 0), joined or not.
                    launches.push(LaunchRecord {
                        flap: p[1] == 1,
                        block_hash: block,
                        tx_hash: tx,
                        external_buys: None,
                        class: 0,
                        source: None,
                        delta_ns: 0,
                        seen_ns: record.timestamp_ns,
                    });
                }
            }
            (RecordKind::Canonical, Some(tag @ (b'H' | b'E'))) if p.len() == 41 => {
                let number = u64_at(p, 1);
                if tag == b'H' {
                    window.first_block = window.first_block.or(number);
                } else {
                    window.last_block = number;
                }
            }
            (RecordKind::Reorg, Some(b'R')) => window.reorgs += 1,
            (RecordKind::Reorg, Some(b'G')) => window.resyncs += 1,
            _ => {}
        }
    }
    if window.state == "INVALID" {
        window.first_block = None;
        window.last_block = None;
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
fn percentile(sorted: &[u64], q_per_mille: u64) -> Option<u64> {
    let n = sorted.len() as u64;
    if n == 0 || n * (1000 - q_per_mille) < 1000 {
        return None;
    }
    let rank = (q_per_mille * n).div_ceil(1000).max(1);
    sorted.get(usize::try_from(rank - 1).ok()?).copied()
}

/// Replays every `*.dbjnl` of `dir` (name `<started_unix_ms>.dbjnl`).
pub fn census(dir: &Path, max_payload: usize) -> Result<Census, String> {
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
    let mut firsts: HashMap<usize, u64> = HashMap::new();
    for (started, name) in names {
        let r = replay(&dir.join(&name), name, started, max_payload)?;
        out.covered_ms = out.covered_ms.saturating_add(r.window.duration_ms);
        for l in &r.launches {
            if l.class == 0 {
                out.orphaned_launches += 1;
                continue;
            }
            if !l.flap {
                out.four_launches += 1;
                continue;
            }
            out.flap_launches += 1;
            if !r.connectivity.any_connected_at(l.seen_ns) {
                out.outage_launches += 1;
            }
            match l.class {
                1 => {
                    out.full_before += 1;
                    leads.push(l.delta_ns / 1_000);
                    if let Some(s) = l.source {
                        *firsts.entry(s).or_default() += 1;
                    }
                }
                2 => out.hash_only_before += 1,
                3 => out.after += 1,
                _ => out.missed += 1,
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
                .set(e::STATE, w.state);
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
            .set(cs::EXTERNAL_BUYS, self.external_buys)
            .set(cs::BUYS_UNKNOWN, self.buys_unknown);
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
        d.set(cs::CRITERIA, CRITERIA_UNCONFIRMED);
        d.encode().map_err(|e| e.to_string())
    }

    /// The text view printed by the CLI.
    pub fn summary(&self) -> String {
        let opt = |v: Option<u64>| v.map_or("n/a".to_string(), |v| v.to_string());
        let mut s = format!(
            "covered_ms={} windows={} flap_launches={} four_launches={} orphaned={} outage={}\n\
             full_before={} hash_only_before={} after={} missed={}\n\
             lead_us p50={} p95={} p99={} max={} n={}\n\
             contested={} external_buys={} buys_unknown={} criteria={}\n",
            self.covered_ms,
            self.windows.len(),
            self.flap_launches,
            self.four_launches,
            self.orphaned_launches,
            self.outage_launches,
            self.full_before,
            self.hash_only_before,
            self.after,
            self.missed,
            opt(self.lead_p50_us),
            opt(self.lead_p95_us),
            opt(self.lead_p99_us),
            opt(self.lead_max_us),
            self.full_before,
            self.contested_launches,
            self.external_buys,
            self.buys_unknown,
            CRITERIA_UNCONFIRMED,
        );
        for w in &self.windows {
            s.push_str(&format!(
                "window {} {} blocks {}..{} duration_ms={} reorgs={} resyncs={} lost={}\n",
                w.journal,
                w.state,
                opt(w.first_block),
                opt(w.last_block),
                w.duration_ms,
                w.reorgs,
                w.resyncs,
                w.lost_records
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_join::{Joined, LaunchRef, Venue, Visibility};
    use crate::runtime_journal::{
        RuntimeJournal, SourceEvent, canonical_launch_payload, orphaned_launch_payload,
        reorg_payload, resync_payload, source_event_payload, window_payload,
    };

    fn launch(venue: Venue, block: u8, tx: u8, buys: Option<u32>, seen: u64) -> LaunchRef {
        LaunchRef {
            venue,
            block_number: u64::from(block),
            block_hash: [block; 32],
            tx_index: 0,
            tx_hash: [tx; 32],
            token: None,
            external_buys: buys,
            block_seen_ns: seen,
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

    #[test]
    fn the_census_replays_classes_orphans_outages_and_windows() {
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
        let full = |lead| Visibility::FullBefore {
            source: p,
            lead_ns: lead,
        };
        let mut records = vec![
            (
                RecordKind::Canonical,
                1,
                window_payload(true, 10, &[10; 32]),
            ),
            (
                RecordKind::Runtime,
                5,
                source_event_payload(p, SourceEvent::Connected),
            ),
            joined(launch(Venue::Flap, 10, 1, Some(2), 100), full(3_000_000)),
            joined(launch(Venue::Flap, 11, 2, Some(0), 200), full(1_000_000)),
            joined(
                launch(Venue::Flap, 11, 3, Some(1), 200),
                Visibility::HashOnlyBefore {
                    source: p,
                    lead_ns: 5,
                },
            ),
            joined(launch(Venue::Four, 11, 4, None, 200), Visibility::Missed),
            (
                RecordKind::Runtime,
                300,
                source_event_payload(p, SourceEvent::Disconnected),
            ),
            joined(launch(Venue::Flap, 12, 5, None, 400), Visibility::Missed),
            joined(
                launch(Venue::Flap, 13, 6, Some(0), 500),
                Visibility::After {
                    source: p,
                    lag_ns: 9,
                },
            ),
            (RecordKind::Reorg, 600, reorg_payload(1, 13, &[99; 32])),
            (
                RecordKind::Canonical,
                600,
                orphaned_launch_payload(&launch(Venue::Flap, 13, 6, Some(0), 500)),
            ),
            (RecordKind::Reorg, 700, resync_payload(13, 20)),
            (
                RecordKind::Canonical,
                2_000_000_000,
                window_payload(false, 20, &[20; 32]),
            ),
        ];
        write(&dir, 1_000, &records);
        // A second process window whose tail was cut mid-frame.
        records.truncate(4);
        let cut = write(&dir, 2_000, &records);
        let bytes = fs::read(&cut).unwrap();
        fs::write(&cut, &bytes[..bytes.len() - 5]).unwrap();
        // A third whose frame is corrupt: nothing of it counts.
        let bad = write(&dir, 3_000, &records);
        let mut bytes = fs::read(&bad).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&bad, &bytes).unwrap();
        fs::write(dir.join("notes.txt"), b"ignored").unwrap();

        let c = census(&dir, 4096).unwrap();
        let states: Vec<_> = c.windows.iter().map(|w| w.state).collect();
        assert_eq!(states, ["COMPLETE", "TAIL_INCOMPLETE", "INVALID"]);
        let w = &c.windows[0];
        assert_eq!(
            (
                w.first_block,
                w.last_block,
                w.reorgs,
                w.resyncs,
                w.duration_ms
            ),
            (Some(10), Some(20), 1, 1, 2_000)
        );
        // Window 1: four Flap launches remain (one orphaned); window 2: its
        // first launch, the second one is in the cut frame.
        assert_eq!(
            (c.flap_launches, c.four_launches, c.orphaned_launches),
            (5, 1, 1)
        );
        assert_eq!(
            (c.full_before, c.hash_only_before, c.after, c.missed),
            (3, 1, 0, 1)
        );
        // Block 12 came while PublicNode was disconnected.
        assert_eq!(c.outage_launches, 1);
        // Block 12's buys are unknown: in neither contested nor external_buys.
        assert_eq!(
            (c.contested_launches, c.external_buys, c.buys_unknown),
            (3, 5, 1)
        );
        assert_eq!(c.lead_max_us, Some(3_000));
        assert_eq!(c.lead_p50_us, Some(3_000));
        assert_eq!(c.lead_p95_us, None, "3 samples cannot carry a p95");
        assert!(c.first_full_before.contains(&(p, 3)));
        let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
        assert_eq!(record.text(cs::CRITERIA).unwrap(), CRITERIA_UNCONFIRMED);
        assert!(c.summary().contains("flap_launches=5"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn percentiles_need_their_sample_count() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 500), Some(50));
        assert_eq!(percentile(&v, 990), Some(99));
        assert_eq!(percentile(&v[..99], 990), None);
        assert_eq!(percentile(&v[..20], 950), Some(19));
        assert_eq!(percentile(&[], 500), None);
    }
}
