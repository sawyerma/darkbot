use super::*;
use crate::canonical_join::{Joined, LaunchRef, Visibility};
use crate::runtime_journal::{
    EXIT_PAYLOAD, RuntimeJournal, SourceEvent, canonical_launch_payload, orphaned_launch_payload,
    pending_launch_payload, reorg_payload, resync_payload, source_event_payload, window_payload,
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
fn write(dir: &Path, started: u64, records: &[(RecordKind, u64, Vec<u8>)]) -> std::path::PathBuf {
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

/// BSC3-033: a run names the G0.4 pre-commitment on file at its start
/// (its journal's first record, also after a crash); a manifest of
/// another start than the run's name (review L1: the name dates the
/// window) makes nothing of it count.
#[test]
fn a_run_names_the_precommitment_it_started_under() {
    let dir = std::env::temp_dir().join(format!("bsc-g04-precommit-run-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let run = |g: &[[u8; 32]]| {
        let mut r: Vec<(RecordKind, u64, Vec<u8>)> = g
            .iter()
            .map(|h| {
                (
                    RecordKind::Runtime,
                    1,
                    crate::runtime_journal::g04_precommit_payload(h),
                )
            })
            .collect();
        r.extend([
            (
                RecordKind::Canonical,
                2,
                window_payload(b'H', 10, &[10; 32]),
            ),
            (
                RecordKind::Canonical,
                3,
                window_payload(b'E', 11, &[11; 32]),
            ),
            (RecordKind::Runtime, 4, EXIT_PAYLOAD.to_vec()),
        ]);
        r
    };
    write_run(&dir, 1_000, &run(&[[7; 32]]), 1 << 20);
    let unbound = write_run(&dir, 2_000, &run(&[]), 1 << 20);
    write_run(&dir, 4_000, &run(&[[7; 32], [8; 32]]), 1 << 20);
    // A crashed run (no manifest) keeps it.
    let crashed = write_run(&dir, 5_000, &run(&[[7; 32]])[..2], 1 << 20);
    fs::remove_file(crashed.join(RUN_MANIFEST)).unwrap();
    let c = census(&dir, 4096, None).unwrap();
    assert_eq!(
        c.windows
            .iter()
            .map(|w| (w.state, w.g04_precommit_sha256))
            .collect::<Vec<_>>(),
        [
            ("COMPLETE", Some([7; 32])),
            ("COMPLETE", None),
            // Two different ones bind neither.
            ("COMPLETE", None),
            ("UNTERMINATED", Some([7; 32])),
        ]
    );
    // A manifest moved under another start: nothing of it counts.
    fs::rename(&unbound, dir.join("3000")).unwrap();
    let c = census(&dir, 4096, None).unwrap();
    assert_eq!(c.windows[1].journal, "3000");
    assert_eq!(c.windows[1].state, "INVALID");
    assert_eq!(c.windows[1].first_block, None);
    fs::remove_dir_all(&dir).unwrap();
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
    // Two later sightings: the spread across sources (R6: one each).
    for (i, (source, lag)) in [
        (SourceId::PublicNode, 3_000_000u64),
        (SourceId::Drpc, 1_000_000),
    ]
    .into_iter()
    .enumerate()
    {
        records.push((
            RecordKind::Runtime,
            200,
            crate::runtime_journal::later_sighting_payload(source, &[i as u8; 32], lag),
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
    // R6: each source's own spread, in the record's source rows.
    let spread = |s: SourceId| {
        let x = c.source_spread.iter().find(|x| x.source == s).unwrap();
        (x.later_sightings, x.later_lag_p50_us, x.later_lag_max_us)
    };
    assert_eq!(spread(SourceId::PublicNode), (1, None, Some(3_000)));
    assert_eq!(spread(SourceId::Drpc), (1, None, Some(1_000)));
    assert_eq!(spread(SourceId::Geth), (0, None, None));
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
    // R6: per source, from the valid window only.
    let dropped: Vec<_> = c
        .source_spread
        .iter()
        .map(|s| (s.source, s.dropped))
        .collect();
    assert!(dropped.contains(&(SourceId::PublicNode, 5)) && dropped.contains(&(SourceId::Drpc, 2)));
    let rows = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap())
        .unwrap()
        .nested_all(cs::SOURCES)
        .unwrap();
    let public = rows
        .iter()
        .find(|r| r.text(cs::source::SOURCE_ID).unwrap() == SourceId::PublicNode.name())
        .unwrap();
    assert_eq!(public.opt_u64(cs::source::DROPPED).unwrap(), Some(5));
    assert_eq!((c.windows[0].unparsed_records, c.unparsed_records), (1, 1));
    assert_eq!(c.covered_ms, 2_000);
    let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
    assert_eq!(record.u64(cs::INGRESS_DROPPED).unwrap(), 7);
    fs::remove_dir_all(&dir).unwrap();
}

/// BSC3-064: covered time is the journals' run time; observable time is
/// the part of a capture window with a source connected, over an
/// unbroken canonical chain. A 10 s window with a source connected for
/// 1 s observes 1 s; a window that skipped blocks observes nothing,
/// however long a source was connected.
#[test]
fn observable_time_is_connected_time_over_an_unbroken_chain() {
    let dir = std::env::temp_dir().join(format!("g04-observable-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    write(
        &dir,
        1_000,
        &[
            (
                RecordKind::Canonical,
                1,
                window_payload(b'H', 10, &[10; 32]),
            ),
            (RecordKind::Runtime, 2_000_000_000, vec![b'S', 1, 3]),
            (RecordKind::Runtime, 3_000_000_000, vec![b'S', 1, 4]),
            (
                RecordKind::Canonical,
                10_000_000_001,
                window_payload(b'E', 11, &[11; 32]),
            ),
            (RecordKind::Runtime, 10_000_000_002, EXIT_PAYLOAD.to_vec()),
        ],
    );
    write(
        &dir,
        20_000,
        &[
            (RecordKind::Runtime, 1, vec![b'S', 2, 3]),
            (
                RecordKind::Canonical,
                2,
                window_payload(b'H', 20, &[20; 32]),
            ),
            (RecordKind::Reorg, 3, resync_payload(21, 25)),
            (
                RecordKind::Canonical,
                5_000_000_002,
                window_payload(b'E', 30, &[30; 32]),
            ),
            (RecordKind::Runtime, 5_000_000_003, EXIT_PAYLOAD.to_vec()),
        ],
    );
    let c = census(&dir, 4096, None).unwrap();
    let states: Vec<_> = c.windows.iter().map(|w| w.state).collect();
    assert_eq!(states, ["COMPLETE", "COMPLETE"]);
    assert_eq!(c.windows[0].observable_ms, Some(1_000));
    assert_eq!(c.windows[1].blocks_skipped, 5);
    assert_eq!(c.windows[1].observable_ms, None);
    assert_eq!(c.observable_ms, 1_000);
    assert_eq!(c.covered_ms, 10_000 + 5_000);
    let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
    assert_eq!(record.u64(cs::OBSERVABLE_MS).unwrap(), 1_000);
    let windows = record.nested_all(cs::WINDOWS).unwrap();
    assert_eq!(
        windows[0].opt_u64(cs::window::OBSERVABLE_MS).unwrap(),
        Some(1_000)
    );
    assert_eq!(windows[1].opt_u64(cs::window::OBSERVABLE_MS).unwrap(), None);
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
            crate::runtime_journal::first_sighting_payload(SourceId::PublicNode, &[tx; 32], b"raw"),
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

/// G0.4 placement: the first external BUY of the token in the launch
/// block at N+1, later, none (the launch's own creator purchase is not
/// external); unknown without a token or with the block's trades
/// incomplete.
#[test]
fn the_first_external_buy_of_a_launch_block_is_placed() {
    let dir = std::env::temp_dir().join(format!("bsc-g04-placement-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let p = SourceId::PublicNode;
    let joined = |block: u8, tx: u8, index: u64, token: Option<[u8; 20]>, full: bool| {
        let mut l = launch(Venue::Flap, block, tx, Some(1), 100);
        l.tx_index = index;
        l.token = token;
        (
            RecordKind::Canonical,
            100,
            canonical_launch_payload(&Joined {
                launch: l,
                visibility: if full {
                    Visibility::FullBefore {
                        source: p,
                        lead_us: 9,
                    }
                } else {
                    Visibility::Missed
                },
            }),
        )
    };
    let buy = |block: u8, index: u64, tx: u8, token: [u8; 20], sell: bool| {
        (
            RecordKind::Canonical,
            90,
            crate::runtime_journal::flap_trade_payload(&crate::chain_follower::CanonicalTrade {
                block_number: u64::from(block),
                block_hash: [block; 32],
                tx_index: index,
                tx_hash: [tx; 32],
                log_index: 0,
                origin: [tx; 20],
                sell,
                token,
                trader: [tx; 20],
                amount: [1; 32],
                eth: [1; 32],
                fee: [0; 32],
                post_price: [1; 32],
            }),
        )
    };
    let records = vec![
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
        // Block 10: launch at 5 (seen in full), a SELL at 6, a BUY at 6
        // of another token, the first BUY of its token at 6: N+1.
        buy(10, 6, 61, [1; 20], false),
        buy(10, 6, 62, [9; 20], false),
        buy(10, 8, 63, [1; 20], false),
        joined(10, 1, 5, Some([1; 20]), true),
        // Block 11: launch at 3, its own creator purchase (same tx), the
        // first external BUY at 7: later. BSC3-037: the creator buys
        // again at 9: an external BUY, also counted apart.
        (
            RecordKind::Canonical,
            90,
            crate::runtime_strategy::flap_launch_payload(11, 3, &[2; 20], &[2; 20]),
        ),
        buy(11, 3, 2, [2; 20], false),
        buy(11, 4, 64, [2; 20], true),
        buy(11, 7, 65, [2; 20], false),
        buy(11, 9, 2, [2; 20], false),
        joined(11, 2, 3, Some([2; 20]), false),
        // Block 12: only the creator purchase: none.
        (
            RecordKind::Canonical,
            90,
            crate::runtime_strategy::flap_launch_payload(12, 0, &[3; 20], &[4; 20]),
        ),
        buy(12, 0, 4, [3; 20], false),
        joined(12, 4, 0, Some([3; 20]), false),
        // Block 13: trades incomplete; block 14: token unknown.
        (
            RecordKind::Canonical,
            95,
            crate::runtime_journal::trade_gap_payload(13, &[13; 32], 2, 0),
        ),
        joined(13, 5, 1, Some([5; 20]), false),
        joined(14, 6, 1, None, false),
        (
            RecordKind::Canonical,
            500,
            window_payload(b'E', 14, &[14; 32]),
        ),
        (RecordKind::Runtime, 501, EXIT_PAYLOAD.to_vec()),
    ];
    write(&dir, 1_000, &records);
    let c = census(&dir, 4096, None).unwrap();
    assert_eq!(
        (
            c.adjacent_buy_launches,
            c.adjacent_buy_full_before,
            c.later_buy_launches,
            c.placement_unknown,
            c.flap_launches
        ),
        (1, 1, 1, 2, 5)
    );
    let record = darkbot_bsc_record::decode(&cs::SCHEMA, &c.encode(1).unwrap()).unwrap();
    assert_eq!(record.u64(cs::ADJACENT_BUY_LAUNCHES).unwrap(), 1);
    assert_eq!(c.creator_buys_after, 1);
    assert_eq!(record.u64(cs::CREATOR_BUYS_AFTER).unwrap(), 1);
    assert!(c.summary().contains("(creator later 1)"));
    assert!(
        c.summary()
            .contains("at N+1 1 (seen in full before the block 1) later 1 unknown 2")
    );
    fs::remove_dir_all(&dir).unwrap();
}

/// BSC3-037: the creator's later buys are those of `external_buys`
/// (its launch block, after the launch transaction) its own account
/// sent: the creator word as sender, or the launch transaction's sender
/// when the creator word is a launcher contract. Not another sender
/// routed through that contract, nor a foreign buyer, a later block, a
/// launch whose buys are not known or one whose block left the chain;
/// a re-included launch counts once, in its new block; each log once.
#[test]
fn the_creator_buys_after_are_its_own_buys_of_external_buys() {
    let dir = std::env::temp_dir().join(format!("bsc-g04-creator-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let (launcher, sender, other, foreign) = ([0xc0; 20], [0xe0; 20], [0xf1; 20], [0xf2; 20]);
    let joined = |hash: u8, number: u8, tx: u8, index: u64, token: [u8; 20], buys: Option<u32>| {
        let mut l = launch(Venue::Flap, number, tx, buys, 100);
        l.block_hash = [hash; 32];
        l.tx_index = index;
        l.token = Some(token);
        (
            RecordKind::Canonical,
            100,
            canonical_launch_payload(&Joined {
                launch: l,
                visibility: Visibility::Missed,
            }),
        )
    };
    let creator = |number: u8, index: u64, token: [u8; 20], word: [u8; 20]| {
        (
            RecordKind::Canonical,
            90,
            crate::runtime_strategy::flap_launch_payload(u64::from(number), index, &token, &word),
        )
    };
    let buy = |hash: u8,
               number: u8,
               index: u64,
               log: u64,
               token: [u8; 20],
               origin: [u8; 20],
               trader: [u8; 20]| {
        (
            RecordKind::Canonical,
            90,
            crate::runtime_journal::flap_trade_payload(&crate::chain_follower::CanonicalTrade {
                block_number: u64::from(number),
                block_hash: [hash; 32],
                tx_index: index,
                tx_hash: [index as u8; 32],
                log_index: log,
                origin,
                sell: false,
                token,
                trader,
                amount: [1; 32],
                eth: [1; 32],
                fee: [0; 32],
                post_price: [1; 32],
            }),
        )
    };
    let (a, b, c) = ([0xa1; 20], [0xb1; 20], [0xc1; 20]);
    let records = vec![
        (
            RecordKind::Canonical,
            1,
            window_payload(b'H', 20, &[20; 32]),
        ),
        // Block 20: a launcher contract's launch at 2 for the sender
        // (its creator purchase names the sender); the sender buys
        // again at 5 (two logs, one journaled twice); another sender
        // through the launcher at 6; a foreign buyer at 7.
        creator(20, 2, a, launcher),
        buy(20, 20, 2, 0, a, sender, launcher),
        buy(20, 20, 5, 0, a, sender, launcher),
        buy(20, 20, 5, 1, a, sender, sender),
        buy(20, 20, 5, 1, a, sender, sender),
        buy(20, 20, 6, 0, a, other, launcher),
        buy(20, 20, 7, 0, a, foreign, foreign),
        joined(20, 20, 1, 2, a, Some(4)),
        // The creator buys again in block 21: not of `external_buys`.
        buy(21, 21, 0, 0, a, sender, sender),
        // Block 30 (hash 30): a direct launch at 1, its creator buys at
        // 4; the block leaves the chain, its replacement (hash 31)
        // includes the launch again at 1 and the creator's buy at 3.
        creator(30, 1, b, sender),
        buy(30, 30, 4, 0, b, sender, sender),
        joined(30, 30, 2, 1, b, Some(1)),
        (
            RecordKind::Canonical,
            200,
            crate::runtime_journal::launch_id_payload(b'O', Venue::Flap, 30, &[30; 32], &[2; 32]),
        ),
        creator(30, 1, b, sender),
        buy(31, 30, 3, 0, b, sender, sender),
        joined(31, 30, 2, 1, b, Some(1)),
        // Block 40: a malformed buy log voided the count: unknown.
        creator(40, 0, c, sender),
        buy(40, 40, 3, 0, c, sender, sender),
        joined(40, 40, 3, 0, c, None),
        (
            RecordKind::Canonical,
            500,
            window_payload(b'E', 40, &[40; 32]),
        ),
        (RecordKind::Runtime, 501, EXIT_PAYLOAD.to_vec()),
    ];
    write(&dir, 1_000, &records);
    let c = census(&dir, 4096, None).unwrap();
    // Block 20: the sender's two logs at 5; block 31: one.
    assert_eq!(c.creator_buys_after, 3);
    assert!(c.creator_buys_after <= c.external_buys);
    fs::remove_dir_all(&dir).unwrap();
}

/// BSC3-037 on real mainnet blocks (G0.4 campaign journal 27.10.,
/// checked against the chain over a free RPC: each buy's
/// `eth_getTransactionReceipt` log is the Portal's `TokenBought` of the
/// token, the launch transaction's `TokenCreated` names the creator):
/// block 130058628, Flap launch of 0x6388…7777 at index 2 by its
/// creator 0x5334…b74b, who buys again at index 3 (log 54): a creator
/// buy; block 130054762, launch of 0x3433…7777 at 2 by 0x4236…1c17
/// with its own purchase in the launch (log 54, not later), two other
/// accounts buy at 3 (log 82) and 4 (log 110): none.
#[test]
fn the_creator_buys_after_of_real_mainnet_blocks() {
    use alloy_primitives::hex;
    let dir = std::env::temp_dir().join(format!("bsc-g04-creator-real-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let joined = |number: u64, hash: [u8; 32], tx: [u8; 32], index: u64, token: [u8; 20], buys| {
        let mut l = launch(Venue::Flap, 1, 1, Some(buys), 100);
        l.block_number = number;
        l.block_hash = hash;
        l.tx_hash = tx;
        l.tx_index = index;
        l.token = Some(token);
        (
            RecordKind::Canonical,
            100,
            canonical_launch_payload(&Joined {
                launch: l,
                visibility: Visibility::Missed,
            }),
        )
    };
    let buy = |number: u64,
               hash: [u8; 32],
               index: u64,
               tx: [u8; 32],
               log: u64,
               token: [u8; 20],
               origin: [u8; 20]| {
        (
            RecordKind::Canonical,
            90,
            crate::runtime_journal::flap_trade_payload(&crate::chain_follower::CanonicalTrade {
                block_number: number,
                block_hash: hash,
                tx_index: index,
                tx_hash: tx,
                log_index: log,
                origin,
                sell: false,
                token,
                trader: origin,
                amount: [1; 32],
                eth: [1; 32],
                fee: [0; 32],
                post_price: [1; 32],
            }),
        )
    };
    let creator = |number: u64, index: u64, token: [u8; 20], word: [u8; 20]| {
        (
            RecordKind::Canonical,
            90,
            crate::runtime_strategy::flap_launch_payload(number, index, &token, &word),
        )
    };
    let (b1, h1) = (
        130_058_628,
        hex!("84f8e010a8640ae8dc098a0d4353e925b188dbf488be921c8ee85f04842d588a"),
    );
    let t1 = hex!("638885fddda965a2b8f01ca48889af439f887777");
    let c1 = hex!("5334897a2d2a3b92e3e1676c78f3f1222bd1b74b");
    let (b2, h2) = (
        130_054_762,
        hex!("99f08d7a5a8dcc19ec8d3e97f3dfea9b9aafe877cc5b1f4c1b1827d07f67e4d4"),
    );
    let t2 = hex!("3433f5fa3a946496a9c056727e03570cff927777");
    let records = vec![
        (RecordKind::Canonical, 1, window_payload(b'H', 0, &[0; 32])),
        creator(b2, 2, t2, hex!("423608a04556f71a7cfe2c6bcbec7fdcc74f1c17")),
        // The launch transaction's own purchase (log 54): not later.
        buy(
            b2,
            h2,
            2,
            hex!("92edcd2bb5d7d184657eb8b6154347d9081bdd0afb97105a465f69c75c54b0dd"),
            54,
            t2,
            hex!("423608a04556f71a7cfe2c6bcbec7fdcc74f1c17"),
        ),
        buy(
            b2,
            h2,
            3,
            hex!("f4f13f2ee3748e24f9d3852c426740a42cbfe296d6c7b068baea8bbd8d25eeaf"),
            82,
            t2,
            hex!("3ef6e4d369c787abf8f2924122173b8781258830"),
        ),
        buy(
            b2,
            h2,
            4,
            hex!("0bf5063064a3a35d3a19758ba667d3f729c804745e61c86fd54ca66b06cb1a72"),
            110,
            t2,
            hex!("562f78167473800b73810e9373ed79d77d36414f"),
        ),
        joined(
            b2,
            h2,
            hex!("92edcd2bb5d7d184657eb8b6154347d9081bdd0afb97105a465f69c75c54b0dd"),
            2,
            t2,
            2,
        ),
        creator(b1, 2, t1, c1),
        buy(
            b1,
            h1,
            3,
            hex!("2a0148a284d7badf847268e7e331d5dc281a9b4b7692a2dce3d6d3dbecda718b"),
            54,
            t1,
            c1,
        ),
        joined(
            b1,
            h1,
            hex!("8e5588ec763581ece621c1f5185d574e64b2f40deb15ebff262fc17930374941"),
            2,
            t1,
            1,
        ),
        (RecordKind::Runtime, 501, EXIT_PAYLOAD.to_vec()),
    ];
    write(&dir, 1_000, &records);
    let c = census(&dir, 4096, None).unwrap();
    assert_eq!(c.external_buys, 3);
    // Both launches' first external buy right after them (from the
    // trades, not the stored counts).
    assert_eq!(c.adjacent_buy_launches, 2);
    assert_eq!(c.creator_buys_after, 1);
    fs::remove_dir_all(&dir).unwrap();
}

/// BSC3-074: a run with a PAPER BUY (its `Q` handoff record, as the
/// runtime journals it) is unbroken: no record unparsed, its observable
/// time counted; a `Q` of another length stays damaged.
#[test]
fn a_paper_handoff_is_no_damaged_record() {
    let dir = std::env::temp_dir().join(format!("bsc-g04-paper-q-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let buy = crate::paper_exec::PaperBuy {
        launch_hash: [1; 32],
        token: [2; 20],
        handoff: crate::paper::PaperHandoff {
            sequence: 1,
            tx_hash: [3; 32],
            raw_len: 110,
            at_ns: 600,
        },
        times: crate::paper_exec::StageTimes {
            ingress_ns: 100,
            decoded_ns: 200,
            admitted_ns: 300,
            signed_ns: 400,
            prepared_ns: 450,
            planned_ns: 500,
            sink_ns: 600,
        },
        raw: vec![0xf8; 110],
    };
    let handoff = crate::runtime_journal::paper_handoff_payload(&buy);
    let records = |q: Vec<u8>| {
        vec![
            (
                RecordKind::Canonical,
                1,
                window_payload(b'H', 20, &[20; 32]),
            ),
            (RecordKind::Runtime, 600, q),
            (RecordKind::Runtime, 900, EXIT_PAYLOAD.to_vec()),
        ]
    };
    write(&dir, 1_000, &records(handoff.clone()));
    let c = census(&dir, 4096, None).unwrap();
    assert_eq!(c.windows[0].unparsed_records, 0);
    assert!(c.windows[0].observable_ms.is_some());
    fs::remove_dir_all(&dir).unwrap();
    write(&dir, 1_000, &records(handoff[..20].to_vec()));
    let c = census(&dir, 4096, None).unwrap();
    assert_eq!(c.windows[0].unparsed_records, 1);
    fs::remove_dir_all(&dir).unwrap();
}

/// G0.4 criteria inputs per venue: launches while a source was
/// connected, those in the tradable profile, those of the profile
/// actionable before the block and their leads.
#[test]
fn criteria_inputs_count_connected_scope_and_actionable_per_venue() {
    let dir = std::env::temp_dir().join(format!("bsc-g04-criteria-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let p = SourceId::PublicNode;
    let joined = |venue, tx, four, visibility, seen| {
        let mut l = launch(venue, 10, tx, Some(0), seen);
        if venue != Venue::Flap {
            l.four_token = four;
        }
        (
            RecordKind::Canonical,
            seen,
            canonical_launch_payload(&Joined {
                launch: l,
                visibility,
            }),
        )
    };
    let full = |lead_us| Visibility::FullBefore { source: p, lead_us };
    let records = vec![
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
        // Flap: in the C1 scope and seen in full (actionable), outside
        // the scope (tx 3), in the scope but missed.
        joined(Venue::Flap, 1, FourToken::NotApplicable, full(4_000), 100),
        joined(Venue::Flap, 3, FourToken::NotApplicable, full(5_000), 100),
        joined(
            Venue::Flap,
            4,
            FourToken::NotApplicable,
            Visibility::Missed,
            100,
        ),
        // BSC3-062: in the scope, its sighting maybe forgotten before
        // the join: not evaluable, in no criteria input, never missed.
        joined(
            Venue::Flap,
            5,
            FourToken::NotApplicable,
            Visibility::Evicted,
            100,
        ),
        // Four Classic: only a candidate token, never actionable.
        joined(Venue::Four, 6, FourToken::Candidate, full(6_000), 100),
        // OpenFour: derived (actionable) and not recognized (no scope).
        joined(Venue::OpenFour, 7, FourToken::Matched, full(7_000), 100),
        joined(
            Venue::OpenFour,
            8,
            FourToken::NotRecognized,
            full(8_000),
            100,
        ),
        (
            RecordKind::Runtime,
            300,
            source_event_payload(p, SourceEvent::Disconnected),
        ),
        // No source connected: outside every denominator.
        joined(
            Venue::Flap,
            9,
            FourToken::NotApplicable,
            Visibility::Missed,
            400,
        ),
        (
            RecordKind::Canonical,
            500,
            window_payload(b'E', 11, &[11; 32]),
        ),
        (RecordKind::Runtime, 501, EXIT_PAYLOAD.to_vec()),
    ];
    write(&dir, 1_000, &records);
    let c = census(&dir, 4096, None).unwrap();
    let rows: Vec<_> = c
        .criteria_inputs
        .iter()
        .map(|v| (v.connected, v.scope, v.actionable, v.leads_us.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (3, 2, 1, vec![4_000]),
            (1, 1, 0, vec![]),
            (2, 1, 1, vec![7_000]),
        ]
    );
    assert_eq!((c.flap_launches, c.outage_launches), (5, 1));
    assert_eq!((c.evicted, c.missed), (1, 2));
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
