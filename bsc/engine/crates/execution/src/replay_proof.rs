//! The capture→replay identity proof of one finished runtime run (Phase F
//! contract §9): the run is closed and hashed (its manifest), read again
//! through the same Rust API (`RunReader`, every frame and sequence
//! verified, every sealed segment against the manifest), and what it holds
//! is held against what the runtime decided while it ran:
//!
//! - every journaled launch sighting decodes again as a launch of its venue
//!   (`ingress::launch_venue`), every other pending sighting does not;
//! - every launch the follower joined as seen in full before its block has
//!   its journaled sighting (C1 scope always; every one under VENUES/ALL);
//! - the runtime's own counters of this run (its last status: the Flap
//!   launch counter and the canonical join counters) equal the census of
//!   this run replayed alone, and that census equals the run's window among
//!   all journals;
//! - the replay runs with the run's configuration and build (manifest).
//!
//! No Python, no normalization: the journal's binary frames are the input.

use crate::canonical_join::Venue;
use crate::g04_census::{Census, census, census_of_run, launch_record, manifest_matches};
use crate::ingress::launch_venue;
use crate::runtime_journal::RUN_MANIFEST;
use darkbot_bsc_journal::RecordKind;
use darkbot_bsc_journal::storage::run::RunReader;
use darkbot_bsc_record::schema::bsc_capture::{capture_run as cr, replay_proof as rp};
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
use darkbot_bsc_record::{Draft, Record, Value, decode};
use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_g01_129842711;
use std::collections::HashSet;
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
    /// Other pending first sightings kept by the capture scope, and those
    /// addressed to a venue contract.
    pub pending: u64,
    pub pending_to_venues: u64,
    /// Other pending sightings the decoders take for a launch.
    pub pending_as_launch: u64,
    /// Launches joined as seen in full before their block without their
    /// journaled sighting.
    pub sightings_missing: u64,
    pub census_state: String,
    /// The run's census alone equals its window among all journals.
    pub census_deterministic: bool,
    pub status_launches: Option<u64>,
    pub status_match: Option<bool>,
    pub status_counters_match: Option<bool>,
    pub config_match: Option<bool>,
    pub build_match: Option<bool>,
    pub verdict: &'static str,
    pub reasons: Vec<String>,
}

/// The runtime's canonical join counters against the census of the run.
fn counter_mismatches(canonical: &Record, c: &Census) -> Vec<String> {
    use rs::canonical as k;
    [
        (k::FLAP_LAUNCHES, "flap_launches", c.flap_launches),
        (k::FOUR_LAUNCHES, "four_launches", c.four_launches),
        (
            k::ORPHANED_LAUNCHES,
            "orphaned_launches",
            c.orphaned_launches,
        ),
        (k::SEEN_FULL_BEFORE, "seen_full_before", c.full_before),
        (
            k::SEEN_HASH_ONLY_BEFORE,
            "seen_hash_only_before",
            c.hash_only_before,
        ),
        (k::SEEN_AFTER, "seen_after", c.after),
        (k::MISSED, "missed", c.missed),
        (k::CLOCK_UNKNOWN, "clock_unknown", c.clock_unknown),
        (
            k::C1_SCOPE_LAUNCHES,
            "c1_scope_launches",
            c.c1_scope_launches,
        ),
        (
            k::C1_SCOPE_FULL_BEFORE,
            "c1_scope_full_before",
            c.c1_scope_full_before,
        ),
        (k::TX_UNVERIFIABLE, "tx_unverifiable", c.tx_unverifiable),
        (
            k::TX_ANSWER_REFUSED,
            "tx_answer_refused",
            c.tx_answer_refused,
        ),
        (k::EXTERNAL_BUYS, "external_buys", c.external_buys),
        (k::BUYS_UNKNOWN, "buys_unknown", c.buys_unknown),
        (
            k::FOUR_PENDING_DECODED,
            "four_pending_decoded",
            c.four_derived,
        ),
        (
            k::FOUR_PENDING_UNKNOWN,
            "four_pending_unknown",
            c.four_no_derivation,
        ),
        (
            k::FOUR_PENDING_MISMATCH,
            "four_pending_mismatch",
            c.four_mismatch,
        ),
        (k::FOUR_ACTIONABLE, "four_actionable", c.four_actionable),
        (
            k::REVERTED_LAUNCHES,
            "reverted_launches",
            c.reverted_flap + c.reverted_classic + c.reverted_openfour,
        ),
    ]
    .into_iter()
    .filter_map(|(tag, name, replayed)| {
        let runtime = canonical.u64(tag).ok()?;
        (runtime != replayed).then(|| format!("runtime {name} {runtime} != replayed {replayed}"))
    })
    .collect()
}

/// Proves the run `<journal_dir>/<run_id>/`. `status` is the bytes of the
/// runtime's status record, if any (compared only when it is of this run);
/// `config_sha256` and `own_build` are the replaying configuration's and
/// binary's hashes, held against the run manifest's.
pub fn prove(
    journal_dir: &Path,
    run_id: &str,
    status: Option<&[u8]>,
    max_payload: usize,
    max_calldata: usize,
    config_sha256: Option<[u8; 32]>,
    own_build: Option<[u8; 32]>,
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
    let mut venues_or_all = false;
    match manifest.as_deref().map(cr::decode_run) {
        Some(Ok(m)) => {
            p.manifest_sha256 = m.file_sha256();
            p.manifest_state = m.text(cr::STATE).ok().map(str::to_string);
            let field = |tag| m.opt_b256(tag).ok().flatten();
            p.config_match = field(cr::CONFIG_SHA256)
                .zip(config_sha256)
                .map(|(a, b)| a == b);
            p.build_match = field(cr::BUILD_SHA256).zip(own_build).map(|(a, b)| a == b);
            venues_or_all = matches!(
                m.opt_text(cr::CAPTURE_PENDING).ok().flatten(),
                Some("VENUES" | "ALL")
            );
        }
        Some(Err(e)) => p.reasons.push(format!("run manifest does not decode: {e}")),
        None => p
            .reasons
            .push("no run manifest: the run is not sealed".into()),
    }
    let four = bsc_mainnet_g01_129842711();
    // Hashes of every journaled first sighting, and the joined launches.
    let mut sighted: HashSet<[u8; 32]> = HashSet::new();
    let mut joined: Vec<([u8; 32], u8, bool)> = Vec::new();
    match RunReader::open(&run, max_payload) {
        Ok(mut reader) => {
            loop {
                match reader.next_record() {
                    Ok(Some(r)) => {
                        p.records += 1;
                        // `source u8 | hash [32] | signed bytes`.
                        let sighting =
                            matches!(r.kind, RecordKind::TxpoolAccepted | RecordKind::RawSignedTx);
                        if let Some(h) = r
                            .payload
                            .get(1..33)
                            .filter(|_| sighting)
                            .and_then(|h| <[u8; 32]>::try_from(h).ok())
                        {
                            sighted.insert(h);
                        }
                        let venue = || {
                            r.payload
                                .get(33..)
                                .and_then(|raw| launch_venue(&four, raw, max_calldata))
                        };
                        match r.kind {
                            RecordKind::TxpoolAccepted => {
                                p.pending += 1;
                                p.pending_to_venues +=
                                    u64::from(r.payload.get(33..).is_some_and(|raw| {
                                        crate::ingress::venue_target(&four, raw)
                                    }));
                                p.pending_as_launch += u64::from(venue().is_some());
                            }
                            RecordKind::RawSignedTx => match venue() {
                                Some(Venue::Flap) => p.sightings[0] += 1,
                                Some(Venue::Four) => p.sightings[1] += 1,
                                Some(Venue::OpenFour) => p.sightings[2] += 1,
                                None => p.undecodable += 1,
                            },
                            RecordKind::Canonical if r.payload.first() == Some(&b'C') => {
                                if let Some(l) = launch_record(&r.payload, r.timestamp_ns) {
                                    joined.push((l.tx_hash, l.class, l.c1_scope));
                                }
                            }
                            _ => {}
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
    // Class 1: the first full sighting came before the block time.
    p.sightings_missing = joined
        .iter()
        .filter(|(tx, class, c1)| *class == 1 && (venues_or_all || *c1) && !sighted.contains(tx))
        .count() as u64;
    let alone = census_of_run(journal_dir, run_id, max_payload)?;
    let all = census(journal_dir, max_payload, None)?;
    let window = all.windows.iter().find(|w| w.journal == run_id);
    p.census_deterministic = alone.windows.first() == window;
    p.census_state = window.map_or("ABSENT", |w| w.state).to_string();
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
            let mismatches = match s.opt_nested(rs::CANONICAL).ok().flatten() {
                Some(canonical) => counter_mismatches(&canonical, &alone),
                // Without a follower nothing may be joined.
                None if alone.flap_launches + alone.four_launches == 0 => Vec::new(),
                None => vec!["launches joined without a follower in the status".into()],
            };
            p.status_counters_match = Some(mismatches.is_empty());
            p.reasons.extend(mismatches);
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
        p.reasons
            .push("the run's census alone differs from its window among all journals".into());
    }
    if p.undecodable > 0 {
        p.reasons.push(format!(
            "{} journaled launch sightings are no launch to the decoders",
            p.undecodable
        ));
    }
    if p.pending_as_launch > 0 {
        p.reasons.push(format!(
            "{} pending sightings journaled as other are launches to the decoders",
            p.pending_as_launch
        ));
    }
    if p.sightings_missing > 0 {
        p.reasons.push(format!(
            "{} launches seen in full before their block have no journaled sighting",
            p.sightings_missing
        ));
    }
    if p.status_match == Some(false) {
        p.reasons.push(format!(
            "runtime Flap launch counter {} != replayed Flap sightings {}",
            p.status_launches.unwrap_or(0),
            p.sightings[0]
        ));
    }
    if p.config_match == Some(false) {
        p.reasons
            .push("replayed with another configuration than the run's".into());
    }
    if p.build_match == Some(false) {
        p.reasons
            .push("replayed by another build than the run's".into());
    }
    p.verdict = if !p.reasons.is_empty() {
        "FAIL"
    } else {
        for (known, what) in [
            (
                p.status_match,
                "no runtime status of this run: its counters are not compared",
            ),
            (
                p.config_match,
                "the manifest or the replay names no configuration hash",
            ),
            (
                p.build_match,
                "the manifest or the replay names no build hash",
            ),
        ] {
            if known.is_none() {
                p.reasons.push(what.into());
            }
        }
        if p.reasons.is_empty() {
            "PASS"
        } else {
            "PARTIAL"
        }
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
        let mut d = Draft::new(&rp::V3);
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
            .set(rp::PENDING_SIGHTINGS, self.pending)
            .set(rp::PENDING_TO_VENUES, self.pending_to_venues)
            .set(rp::PENDING_AS_LAUNCH, self.pending_as_launch)
            .set(rp::SIGHTINGS_MISSING, self.sightings_missing)
            .set(rp::CENSUS_STATE, self.census_state.as_str())
            .set(rp::CENSUS_DETERMINISTIC, self.census_deterministic)
            .set(rp::VERDICT, self.verdict)
            .push(
                rp::NON_CLAIMS,
                "LOCAL_REPLAY: identity of this host's capture and its replay; the Phase-F quality verdict is the capture-quality record",
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
        for (tag, v) in [
            (rp::STATUS_MATCH, self.status_match),
            (rp::STATUS_COUNTERS_MATCH, self.status_counters_match),
            (rp::CONFIG_MATCH, self.config_match),
            (rp::BUILD_MATCH, self.build_match),
        ] {
            if let Some(v) = v {
                d.set(tag, v);
            }
        }
        for r in &self.reasons {
            d.push(rp::REASONS, r.as_str());
        }
        d.encode().map_err(|e| format!("replay proof: {e}"))
    }

    pub fn summary(&self) -> String {
        format!(
            "replay proof of run {}: {} (segments {}, records {}, last sequence {}, manifest match {}, sightings Flap {} Four {} OpenFour {} undecodable {}, other pending {} (to venues {}), census {} deterministic {}, runtime Flap counter {})\n{}",
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
            self.pending,
            self.pending_to_venues,
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

    fn openfour_buy() -> Vec<u8> {
        let calls = include_str!("../../trading/fixtures/fourmeme/freeze_20261026_calls.txt");
        let row: Vec<&str> = calls
            .lines()
            .find(|l| l.starts_with("OPENFOUR BUY"))
            .unwrap()
            .split_whitespace()
            .collect();
        alloy_primitives::hex::decode(row[10]).unwrap()
    }

    /// The configuration and build every test run is written and replayed
    /// with.
    const CONFIG: Option<[u8; 32]> = Some([0xC0; 32]);
    const BUILD: Option<[u8; 32]> = Some([0xB0; 32]);

    fn identity() -> crate::runtime_journal::RunIdentity {
        crate::runtime_journal::RunIdentity {
            host: "test".into(),
            build: "test".into(),
            config_sha256: CONFIG.unwrap(),
            build_sha256: BUILD,
            build_dirty: Some(false),
            capture_pending: "VENUES",
        }
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
        j.set_identity(identity());
        let mut at = 0;
        let mut put = |j: &mut RuntimeJournal, kind, payload: &[u8]| {
            at += 1;
            assert!(j.record(kind, at, payload.to_vec()));
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
        // One other pending sighting the capture scope kept: a venue BUY.
        put(
            &mut j,
            RecordKind::TxpoolAccepted,
            &first_sighting_payload(SourceId::Drpc, &[hashes + 9; 32], &openfour_buy()),
        );
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
        let p = prove(
            &base,
            "7",
            Some(&status(7, 2)),
            MAX + 64,
            MAX,
            CONFIG,
            BUILD,
        )
        .unwrap();
        assert_eq!(p.verdict, "PASS", "{}", p.summary());
        assert_eq!((p.sightings, p.undecodable), ([2, 0, 1], 0));
        assert_eq!((p.pending, p.pending_to_venues), (1, 1));
        assert!(p.segments > 1 && p.manifest_match && p.census_deterministic);
        assert_eq!(p.status_match, Some(true));
        let r = rp::decode_proof(&p.encode(1).unwrap()).unwrap();
        assert_eq!(r.text(rp::VERDICT).unwrap(), "PASS");
        // The status of another run is not compared: PARTIAL.
        let other = prove(
            &base,
            "7",
            Some(&status(8, 2)),
            MAX + 64,
            MAX,
            CONFIG,
            BUILD,
        )
        .unwrap();
        assert_eq!((other.verdict, other.status_launches), ("PARTIAL", None));
        fs::remove_dir_all(&base).unwrap();
    }

    /// The runtime's status of run `started` with a canonical block whose
    /// required counters are zero but `flap_launches`.
    fn status_with_canonical(started: u64, flap_launches: u64) -> Vec<u8> {
        use darkbot_bsc_record::Kind;
        use rs::canonical as k;
        let mut c = Draft::new(&k::SCHEMA);
        for f in k::SCHEMA
            .fields
            .iter()
            .filter(|f| f.presence == darkbot_bsc_record::Presence::Required)
        {
            match f.kind {
                Kind::U64 => c.set(f.tag, 0u64),
                Kind::I64 => c.set(f.tag, 0i64),
                Kind::Bool => c.set(f.tag, false),
                Kind::Text => c.set(f.tag, "STOPPED"),
                other => panic!("canonical field kind {other:?}"),
            };
        }
        c.set(k::FLAP_LAUNCHES, flap_launches);
        let mut d = Draft::new(&rs::SCHEMA);
        d.set(rs::GENERATION, 1u64)
            .set(rs::PROVENANCE_TOOL, "bsc_runtime")
            .set(rs::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(rs::STARTED_UNIX_MS, started)
            .set(rs::LAUNCHES, 2u64)
            .set(rs::JOURNAL_PATH, "j")
            .set(rs::JOURNAL_RECORDS, 0u64)
            .set(rs::JOURNAL_LOST, 0u64)
            .set(rs::MODE, "READ_ONLY")
            .set(rs::STATUS_INTERVAL_MS, 1000u64)
            .set(rs::CANONICAL, c.into_nested());
        d.encode().unwrap()
    }

    /// Phase F gate H2/M5: the replay holds the runtime's decisions. Each
    /// of these fails (before: PASS): another configuration or build, a
    /// launch journaled as other pending, a launch seen in full before its
    /// block without its sighting, a canonical counter the replay does not
    /// reproduce.
    #[test]
    fn the_replay_holds_the_runtime_decisions() {
        let base = std::env::temp_dir().join(format!("bsc-replay-proof-h-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (a, b, c) = (base.join("a"), base.join("b"), base.join("c"));
        write(&a, 7, 0);
        let fail = |p: &Proof, why: &str| {
            assert_eq!(p.verdict, "FAIL", "{}", p.summary());
            assert!(p.reasons.iter().any(|r| r.contains(why)), "{}", p.summary());
        };
        let p = prove(
            &a,
            "7",
            Some(&status(7, 2)),
            MAX + 64,
            MAX,
            Some([1; 32]),
            BUILD,
        )
        .unwrap();
        fail(&p, "another configuration");
        let p = prove(
            &a,
            "7",
            Some(&status(7, 2)),
            MAX + 64,
            MAX,
            CONFIG,
            Some([1; 32]),
        )
        .unwrap();
        fail(&p, "another build");
        // The canonical counters of the runtime against the replayed census.
        let p = prove(
            &a,
            "7",
            Some(&status_with_canonical(7, 0)),
            MAX + 64,
            MAX,
            CONFIG,
            BUILD,
        )
        .unwrap();
        assert_eq!(
            (p.verdict, p.status_counters_match),
            ("PASS", Some(true)),
            "{}",
            p.summary()
        );
        let p = prove(
            &a,
            "7",
            Some(&status_with_canonical(7, 5)),
            MAX + 64,
            MAX,
            CONFIG,
            BUILD,
        )
        .unwrap();
        fail(&p, "runtime flap_launches 5 != replayed 0");
        // A Flap launch's signed bytes journaled as other pending.
        {
            let setup = JournalSetup::whole(64, MAX + 64);
            let mut j = RuntimeJournal::open(&b, 7, 1, setup).unwrap();
            j.set_identity(identity());
            j.record(
                RecordKind::Canonical,
                1,
                window_payload(b'H', 10, &[10; 32]),
            );
            j.record(
                RecordKind::TxpoolAccepted,
                2,
                first_sighting_payload(SourceId::Drpc, &[1; 32], &flap_launch()),
            );
            j.record(
                RecordKind::Canonical,
                3,
                window_payload(b'E', 11, &[11; 32]),
            );
            j.record(RecordKind::Runtime, 4, EXIT_PAYLOAD.to_vec());
            j.finish().unwrap();
        }
        let p = prove(&b, "7", Some(&status(7, 0)), MAX + 64, MAX, CONFIG, BUILD).unwrap();
        fail(&p, "journaled as other are launches");
        // A launch joined as seen in full before its block, its sighting not
        // in the journal.
        {
            use crate::canonical_join::{FourToken, Joined, LaunchRef, LaunchTx, Visibility};
            let launch = LaunchRef {
                venue: Venue::Flap,
                block_number: 10,
                block_hash: [10; 32],
                tx_index: 0,
                tx_hash: [0x55; 32],
                token: Some([4; 20]),
                external_buys: Some(0),
                c1_scope: true,
                four_token: FourToken::NotApplicable,
                tx: LaunchTx::Verified,
                block_ms: 1_000,
                block_seen_ns: 3,
                block_seen_unix_ms: Some(1_100),
            };
            let setup = JournalSetup::whole(64, MAX + 64);
            let mut j = RuntimeJournal::open(&c, 7, 1, setup).unwrap();
            j.set_identity(identity());
            j.record(
                RecordKind::Canonical,
                1,
                window_payload(b'H', 10, &[10; 32]),
            );
            j.record(
                RecordKind::Canonical,
                3,
                crate::runtime_journal::canonical_launch_payload(&Joined {
                    launch,
                    visibility: Visibility::FullBefore {
                        source: SourceId::Drpc,
                        lead_us: 9,
                    },
                }),
            );
            j.record(
                RecordKind::Canonical,
                4,
                window_payload(b'E', 11, &[11; 32]),
            );
            j.record(RecordKind::Runtime, 5, EXIT_PAYLOAD.to_vec());
            j.finish().unwrap();
        }
        let p = prove(&c, "7", Some(&status(7, 0)), MAX + 64, MAX, CONFIG, BUILD).unwrap();
        fail(&p, "have no journaled sighting");
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
        let p = prove(&a, "7", Some(&status(7, 3)), MAX + 64, MAX, CONFIG, BUILD).unwrap();
        assert_eq!(p.verdict, "FAIL");
        assert!(p.reasons[0].contains("counter 3 != replayed Flap sightings 2"));

        write(&c, 7, 100);
        let seg = |d: &Path| d.join("7").join("segment_000001.dbjnl");
        fs::copy(seg(&c), seg(&a)).unwrap();
        let p = prove(&a, "7", Some(&status(7, 2)), MAX + 64, MAX, CONFIG, BUILD).unwrap();
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
        let p = prove(&b, "7", Some(&status(7, 2)), MAX + 64, MAX, CONFIG, BUILD).unwrap();
        assert_eq!(p.verdict, "FAIL");
        assert!(p.reasons.iter().any(|r| r.contains("not sealed")));
        fs::remove_dir_all(&base).unwrap();
    }
}
