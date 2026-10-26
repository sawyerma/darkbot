//! The capture→replay identity proof of one finished runtime run (Phase F
//! contract §9): the run is closed and hashed (its manifest), read again
//! through the same Rust API (`RunReader`, every frame and sequence
//! verified, every sealed segment against the manifest), every journaled
//! first sighting decoded again by the same promoted decoders
//! (`ingress::launch_venue`), the census replayed twice (it must be the
//! same), and the runtime's own Flap launch counter (its last status of
//! this run) compared with the replay's. No Python, no normalization: the
//! journal's binary frames are the only input.

use crate::canonical_join::Venue;
use crate::g04_census::{census, manifest_matches};
use crate::ingress::launch_venue;
use crate::runtime_journal::RUN_MANIFEST;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_journal::storage::run::RunReader;
use darkbot_bsc_record::schema::bsc_capture::{capture_run as cr, replay_proof as rp};
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{Draft, Value, decode};
use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_g01_129842711;
use std::fs;
use std::path::Path;

/// What the replay of one run proved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Proof {
    pub run_id: String,
    pub manifest_sha256: Option<[u8; 32]>,
    pub manifest_state: Option<String>,
    pub segments: u64,
    pub records: u64,
    pub last_sequence: Option<u64>,
    pub manifest_match: bool,
    pub read_error: Option<String>,
    /// First sightings decoded again: Flap, Four Classic, OpenFour.
    pub sightings: [u64; 3],
    pub undecodable: u64,
    pub census_state: String,
    pub census_deterministic: bool,
    pub status_launches: Option<u64>,
    pub status_match: Option<bool>,
    pub verdict: &'static str,
    pub reasons: Vec<String>,
}

/// Proves the run `<journal_dir>/<run_id>/`; `status` is the bytes of the
/// runtime's status record, if any (compared only when it is of this run).
pub fn prove(
    journal_dir: &Path,
    run_id: &str,
    status: Option<&[u8]>,
    max_payload: usize,
    max_calldata: usize,
) -> Result<Proof, String> {
    let run = journal_dir.join(run_id);
    if !run.is_dir() {
        return Err(format!("{}: no run directory", run.display()));
    }
    let mut p = Proof {
        run_id: run_id.to_string(),
        ..Proof::default()
    };
    let manifest = match fs::read(run.join(RUN_MANIFEST)) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", run.join(RUN_MANIFEST).display())),
    };
    match manifest.as_deref().map(cr::decode_run) {
        Some(Ok(m)) => {
            p.manifest_sha256 = m.file_sha256();
            p.manifest_state = m.text(cr::STATE).ok().map(str::to_string);
        }
        Some(Err(e)) => p.reasons.push(format!("run manifest does not decode: {e}")),
        None => p
            .reasons
            .push("no run manifest: the run is not sealed".into()),
    }
    let four = bsc_mainnet_g01_129842711();
    match RunReader::open(&run, max_payload) {
        Ok(mut reader) => {
            loop {
                match reader.next_record() {
                    Ok(Some(r)) => {
                        p.records += 1;
                        if r.kind != RecordKind::RawSignedTx {
                            continue;
                        }
                        // `source u8 | hash [32] | signed bytes`.
                        match r
                            .payload
                            .get(33..)
                            .and_then(|raw| launch_venue(&four, raw, max_calldata))
                        {
                            Some(Venue::Flap) => p.sightings[0] += 1,
                            Some(Venue::Four) => p.sightings[1] += 1,
                            Some(Venue::OpenFour) => p.sightings[2] += 1,
                            None => p.undecodable += 1,
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        p.read_error = Some(e.to_string());
                        break;
                    }
                }
            }
            p.segments = reader.segments().len() as u64;
            p.last_sequence = reader.last_sequence();
            p.manifest_match = manifest
                .as_deref()
                .is_some_and(|b| manifest_matches(b, reader.segments()));
        }
        Err(e) => p.read_error = Some(e.to_string()),
    }
    let (a, b) = (
        census(journal_dir, max_payload, None)?,
        census(journal_dir, max_payload, None)?,
    );
    p.census_deterministic = a == b;
    p.census_state = a
        .windows
        .iter()
        .find(|w| w.journal == run_id)
        .map_or("ABSENT", |w| w.state)
        .to_string();
    if let Some(bytes) = status {
        let s = decode(&rs::SCHEMA, bytes).map_err(|e| format!("runtime status: {e}"))?;
        let of_this_run = s
            .u64(rs::STARTED_UNIX_MS)
            .is_ok_and(|started| started.to_string() == run_id);
        if of_this_run {
            let n = s
                .u64(rs::LAUNCHES)
                .map_err(|e| format!("runtime status: {e}"))?;
            p.status_launches = Some(n);
            p.status_match = Some(n == p.sightings[0]);
        }
    }
    if manifest.is_some() && !p.manifest_match {
        p.reasons
            .push("the sealed segments differ from the run manifest".into());
    }
    if let Some(state) = p.manifest_state.as_deref().filter(|s| *s != "COMPLETE") {
        p.reasons.push(format!("run manifest state {state}"));
    }
    if let Some(e) = &p.read_error {
        p.reasons.push(format!("replay stopped: {e}"));
    }
    if p.census_state != "COMPLETE" {
        p.reasons
            .push(format!("census window state {}", p.census_state));
    }
    if !p.census_deterministic {
        p.reasons.push("two replays gave different censuses".into());
    }
    if p.undecodable > 0 {
        p.reasons.push(format!(
            "{} journaled sightings are no launch to the decoders",
            p.undecodable
        ));
    }
    if p.status_match == Some(false) {
        p.reasons.push(format!(
            "runtime Flap launch counter {} != replayed Flap sightings {}",
            p.status_launches.unwrap_or(0),
            p.sightings[0]
        ));
    }
    p.verdict = if !p.reasons.is_empty() {
        "FAIL"
    } else if p.status_match.is_none() {
        p.reasons
            .push("no runtime status of this run: its launch counter is not compared".into());
        "PARTIAL"
    } else {
        "PASS"
    };
    Ok(p)
}

/// The newest run directory (`<started_unix_ms>/`) of `journal_dir`.
pub fn latest_run(journal_dir: &Path) -> Result<String, String> {
    let mut newest: Option<u64> = None;
    for entry in fs::read_dir(journal_dir).map_err(|e| format!("{}: {e}", journal_dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", journal_dir.display()))?;
        if entry.path().is_dir()
            && let Ok(started) = entry.file_name().to_string_lossy().parse::<u64>()
        {
            newest = newest.max(Some(started));
        }
    }
    newest
        .map(|n| n.to_string())
        .ok_or_else(|| format!("{}: no run directory", journal_dir.display()))
}

/// Writes the proof next to the journals, replacing an earlier proof of the
/// same run (a temporary file, synced, then renamed).
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("dbrec.tmp");
    let mut f = fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

impl Proof {
    pub fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&rp::SCHEMA);
        d.set(rp::RUN_ID, self.run_id.as_str())
            .set(rp::CHAIN_ID, 56u64)
            .set(rp::PROVENANCE_TOOL, "bsc_runtime replay-proof")
            .set(rp::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(rp::SEGMENTS, self.segments)
            .set(rp::RECORDS, self.records)
            .set(rp::MANIFEST_MATCH, self.manifest_match)
            .set(rp::FLAP_SIGHTINGS, self.sightings[0])
            .set(rp::FOUR_SIGHTINGS, self.sightings[1])
            .set(rp::OPENFOUR_SIGHTINGS, self.sightings[2])
            .set(rp::UNDECODABLE_SIGHTINGS, self.undecodable)
            .set(rp::CENSUS_STATE, self.census_state.as_str())
            .set(rp::CENSUS_DETERMINISTIC, self.census_deterministic)
            .set(rp::VERDICT, self.verdict)
            .push(
                rp::NON_CLAIMS,
                "LOCAL_REPLAY: identity of this host's capture and its replay; no Phase-F quality verdict",
            );
        if let Some(h) = self.manifest_sha256 {
            d.set(rp::MANIFEST_SHA256, Value::B256(h));
        }
        if let Some(s) = &self.manifest_state {
            d.set(rp::MANIFEST_STATE, s.as_str());
        }
        if let Some(v) = self.last_sequence {
            d.set(rp::LAST_SEQUENCE, v);
        }
        if let Some(e) = &self.read_error {
            d.set(rp::READ_ERROR, e.as_str());
        }
        if let Some(v) = self.status_launches {
            d.set(rp::STATUS_LAUNCHES, v);
        }
        if let Some(v) = self.status_match {
            d.set(rp::STATUS_MATCH, v);
        }
        for r in &self.reasons {
            d.push(rp::REASONS, r.as_str());
        }
        d.encode().map_err(|e| format!("replay proof: {e}"))
    }

    pub fn summary(&self) -> String {
        format!(
            "replay proof of run {}: {} (segments {}, records {}, last sequence {}, manifest match {}, sightings Flap {} Four {} OpenFour {} undecodable {}, census {} deterministic {}, runtime Flap counter {})\n{}",
            self.run_id,
            self.verdict,
            self.segments,
            self.records,
            self.last_sequence
                .map_or("none".to_string(), |v| v.to_string()),
            self.manifest_match,
            self.sightings[0],
            self.sightings[1],
            self.sightings[2],
            self.undecodable,
            self.census_state,
            self.census_deterministic,
            self.status_launches
                .map_or("not of this run".to_string(), |v| v.to_string()),
            self.reasons.join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_journal::{
        EXIT_PAYLOAD, JournalSetup, RuntimeJournal, first_sighting_payload, window_payload,
    };
    use crate::sources::SourceId;
    use darkbot_bsc_journal::storage::segments::SegmentPolicy;

    const MAX: usize = 16_384;
    const PORTAL: [u8; 20] = [
        0xe2, 0xce, 0x6a, 0xb8, 0x08, 0x74, 0xfa, 0x9f, 0xa2, 0xaa, 0xe6, 0x5d, 0x27, 0x7d, 0xd6,
        0xb8, 0xe6, 0x5c, 0x9d, 0xe0,
    ];

    /// A legacy transaction to the Portal with the real launch calldata
    /// (the venue decision reads the envelope target, not the signature).
    fn flap_launch() -> Vec<u8> {
        use darkbot_bsc_tx_encoding::rlp::{encode_bytes, encode_list, encode_u64};
        let data = alloy_primitives::hex::decode(
            include_str!("../../trading/fixtures/flap/launch_v6_native_split_tax_pending.hex")
                .trim(),
        )
        .unwrap();
        encode_list(&[
            encode_u64(1),
            encode_u64(1_000_000_000),
            encode_u64(21_000),
            encode_bytes(&PORTAL),
            encode_u64(0),
            encode_bytes(&data),
            encode_u64(147),
            encode_bytes(&[1u8; 32]),
            encode_bytes(&[2u8; 32]),
        ])
    }

    /// The signed bytes of the first OpenFour launch fixture.
    fn openfour_launch() -> Vec<u8> {
        let calls = include_str!("../../trading/fixtures/fourmeme/freeze_20261026_calls.txt");
        let row: Vec<&str> = calls
            .lines()
            .find(|l| l.starts_with("OPENFOUR LAUNCH"))
            .unwrap()
            .split_whitespace()
            .collect();
        alloy_primitives::hex::decode(row[10]).unwrap()
    }

    fn status(started: u64, launches: u64) -> Vec<u8> {
        let mut d = Draft::new(&rs::SCHEMA);
        d.set(rs::GENERATION, 1u64)
            .set(rs::PROVENANCE_TOOL, "bsc_runtime")
            .set(rs::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(rs::STARTED_UNIX_MS, started)
            .set(rs::LAUNCHES, launches)
            .set(rs::JOURNAL_PATH, "j")
            .set(rs::JOURNAL_RECORDS, 0u64)
            .set(rs::JOURNAL_LOST, 0u64)
            .set(rs::MODE, "READ_ONLY")
            .set(rs::STATUS_INTERVAL_MS, 1000u64);
        d.encode().unwrap()
    }

    /// A clean run of two Flap sightings and one OpenFour sighting over
    /// several segments, sealed by its manifest; `hashes` names the
    /// sightings (the same sizes for any value).
    fn write(dir: &Path, started: u64, hashes: u8) {
        let setup = JournalSetup {
            segment: SegmentPolicy {
                max_segment_bytes: 600,
                max_segment_ns: u64::MAX,
            },
            ..JournalSetup::whole(64, MAX + 64)
        };
        let mut j = RuntimeJournal::open(dir, started, 1, setup).unwrap();
        let mut at = 0;
        let mut put = |j: &mut RuntimeJournal, kind, payload: &[u8]| {
            at += 1;
            assert!(j.record(kind, at, payload));
        };
        put(
            &mut j,
            RecordKind::Canonical,
            &window_payload(b'H', 10, &[10; 32]),
        );
        let (flap, open) = (flap_launch(), openfour_launch());
        for (i, raw) in [&flap, &open, &flap].into_iter().enumerate() {
            put(
                &mut j,
                RecordKind::RawSignedTx,
                &first_sighting_payload(SourceId::Drpc, &[hashes + i as u8; 32], raw),
            );
        }
        put(
            &mut j,
            RecordKind::Canonical,
            &window_payload(b'E', 11, &[11; 32]),
        );
        put(&mut j, RecordKind::Runtime, &EXIT_PAYLOAD);
        j.finish().unwrap();
    }

    #[test]
    fn a_clean_run_proves_its_replay_identity() {
        let base = std::env::temp_dir().join(format!("bsc-replay-proof-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        write(&base, 7, 0);
        let p = prove(&base, "7", Some(&status(7, 2)), MAX + 64, MAX).unwrap();
        assert_eq!(p.verdict, "PASS", "{}", p.summary());
        assert_eq!((p.sightings, p.undecodable), ([2, 0, 1], 0));
        assert!(p.segments > 1 && p.manifest_match && p.census_deterministic);
        assert_eq!(p.status_match, Some(true));
        let r = decode(&rp::SCHEMA, &p.encode(1).unwrap()).unwrap();
        assert_eq!(r.text(rp::VERDICT).unwrap(), "PASS");
        // The status of another run is not compared: PARTIAL.
        let other = prove(&base, "7", Some(&status(8, 2)), MAX + 64, MAX).unwrap();
        assert_eq!((other.verdict, other.status_launches), ("PARTIAL", None));
        fs::remove_dir_all(&base).unwrap();
    }

    /// A runtime counter the replay does not reproduce, a sealed segment
    /// replaced by another valid one (frames and sequence intact) and a
    /// missing manifest each fail.
    #[test]
    fn a_counter_or_segment_that_does_not_replay_fails() {
        let base = std::env::temp_dir().join(format!("bsc-replay-proof-f-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (a, b, c) = (base.join("a"), base.join("b"), base.join("c"));
        write(&a, 7, 0);
        let p = prove(&a, "7", Some(&status(7, 3)), MAX + 64, MAX).unwrap();
        assert_eq!(p.verdict, "FAIL");
        assert!(p.reasons[0].contains("counter 3 != replayed Flap sightings 2"));

        write(&c, 7, 100);
        let seg = |d: &Path| d.join("7").join("segment_000001.dbjnl");
        fs::copy(seg(&c), seg(&a)).unwrap();
        let p = prove(&a, "7", Some(&status(7, 2)), MAX + 64, MAX).unwrap();
        assert_eq!((p.verdict, p.manifest_match), ("FAIL", false));
        assert_eq!(p.read_error, None);
        assert_eq!(
            p.reasons,
            [
                "the sealed segments differ from the run manifest",
                "census window state INVALID"
            ]
        );

        write(&b, 7, 0);
        fs::remove_file(b.join("7").join(RUN_MANIFEST)).unwrap();
        let p = prove(&b, "7", Some(&status(7, 2)), MAX + 64, MAX).unwrap();
        assert_eq!(p.verdict, "FAIL");
        assert!(p.reasons.iter().any(|r| r.contains("not sealed")));
        fs::remove_dir_all(&base).unwrap();
    }
}
