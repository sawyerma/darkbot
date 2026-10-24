use super::*;
fn path() -> PathBuf {
    // Unique across processes: `Pubkey::new_unique` restarts in every test binary run.
    let dir = std::env::temp_dir().join(format!(
        "p0-journal-{}-{}-{}",
        solana_sdk::pubkey::Pubkey::new_unique(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    dir.join("events.p0j")
}
#[test]
fn lossless_integer_decimal_and_nested_value_roundtrip() {
    let value = serde_json::json!({"min":i128::MIN,"max":i128::MAX,"unsigned":u128::MAX,"null":null,"nested":[true,false,"quote\"\n雪"],"decimal":serde_json::from_str::<Value>("1.0000000000000000000000000001").unwrap()});
    assert_eq!(decode(&encode(&value).unwrap()).unwrap(), value);
    assert_eq!(crc(b"123456789"), 0xcbf43926);
}
#[test]
fn ordered_frames_preserve_provenance_manifest_and_durable_boundary() {
    let path = path();
    let mut w = Writer::create(&path).unwrap();
    let p = Provenance {
        source: 7,
        source_seq: 12,
        slot: Some(123),
        fork: Some([4; 32]),
        rx_ns: Some(9),
        unix_ns: None,
    };
    let f = w
        .append_legacy(
            kind::FE01_READ,
            p.clone(),
            &serde_json::json!({"amount":i128::MIN}),
        )
        .unwrap();
    let ack = w.sync().unwrap();
    assert_eq!(ack.journal_seq, 0);
    assert_eq!(ack.hash, f.hash);
    w.seal(true).unwrap();
    let mut r = Reader::open(&path).unwrap();
    let read = r.next_frame().unwrap().unwrap();
    assert_eq!(read.provenance, p);
    assert_eq!(
        read.legacy_value().unwrap()["amount"].to_string(),
        i128::MIN.to_string()
    );
    r.next_frame().unwrap();
    assert!(r.next_frame().unwrap().is_none());
    assert!(r.complete);
    assert!(Writer::resume(&path).is_err());
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
#[test]
fn unsupported_version_corruption_checksum_and_every_truncation_fail_closed() {
    let path = path();
    let mut w = Writer::create(&path).unwrap();
    w.append_value(kind::FE01_READ, &serde_json::json!({"a":123}))
        .unwrap();
    w.seal(true).unwrap();
    drop(w);
    let good = fs::read(&path).unwrap();
    for offset in [0, 8, 60, 70, good.len() - 1] {
        let mut badbytes = good.clone();
        badbytes[offset] ^= 0x55;
        fs::write(&path, badbytes).unwrap();
        assert!(
            Reader::open(&path)
                .and_then(|mut r| {
                    while r.next_frame()?.is_some() {}
                    Ok(())
                })
                .is_err(),
            "{offset}"
        );
    }
    for end in 0..good.len() {
        fs::write(&path, &good[..end]).unwrap();
        let result = Reader::open(&path).and_then(|mut r| {
            while r.next_frame()?.is_some() {}
            Ok(r.complete)
        });
        assert_ne!(result.ok(), Some(true), "truncated at {end}");
    }
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
#[test]
fn restart_never_truncates_or_accepts_partial_tail() {
    let path = path();
    let mut w = Writer::create(&path).unwrap();
    w.append_value(kind::FE01_READ, &serde_json::json!({"intent":"RESERVED"}))
        .unwrap();
    w.sync().unwrap();
    drop(w);
    let mut w = Writer::resume(&path).unwrap();
    assert_eq!(w.next_seq(), 1);
    w.append_value(kind::FE01_READ, &serde_json::json!({"intent":"UNKNOWN"}))
        .unwrap();
    w.sync().unwrap();
    drop(w);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(&[1, 2])
        .unwrap();
    let before = fs::read(&path).unwrap();
    assert!(Writer::resume(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
#[test]
fn malformed_length_payload_tags_and_depth_are_rejected() {
    assert!(decode(&[99]).is_err());
    assert!(decode(&[7, 255, 255, 255, 255]).is_err());
    assert!(decode(&[0, 0]).is_err());
    let mut v = Value::Null;
    for _ in 0..66 {
        v = Value::Array(vec![v]);
    }
    assert!(encode(&v).is_err());
    let path = path();
    let mut w = Writer::create(&path).unwrap();
    w.append_value(kind::FE01_READ, &Value::Null).unwrap();
    drop(w);
    let mut f = OpenOptions::new().write(true).open(&path).unwrap();
    f.seek(SeekFrom::Start(HEADER_LEN)).unwrap();
    f.write_all(&u32::MAX.to_le_bytes()).unwrap();
    assert!(Reader::open(&path).unwrap().next_frame().is_err());
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
#[test]
fn explicit_export_matches_legacy_jsonl_and_never_overwrites() {
    let path = path();
    let export = path.with_extension("jsonl");
    let value = serde_json::json!({"integer":i128::MIN,"unknown":null});
    let mut w = Writer::create(&path).unwrap();
    w.append_value(kind::FE01_READ, &value).unwrap();
    w.seal(true).unwrap();
    super::export(&path, &export, None, false).unwrap();
    assert_eq!(fs::read_to_string(&export).unwrap(), format!("{value}\n"));
    assert!(super::export(&path, &export, None, false).is_err());
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
#[test]
fn header_identities_are_fresh_per_writer_and_shared_per_run_and_boot() {
    let (a, b) = (path(), path());
    let (wa, wb) = (Writer::create(&a).unwrap(), Writer::create(&b).unwrap());
    let (ra, rb) = (Reader::open(&a).unwrap(), Reader::open(&b).unwrap());
    assert_eq!(ra.identities[..16], rb.identities[..16]);
    assert_ne!(ra.identities[16..32], rb.identities[16..32]);
    assert_eq!(ra.identities[32..], rb.identities[32..]);
    // Not a process-local counter: the first writer of every process would repeat it.
    for id in ra.identities.chunks(16) {
        assert!(id.iter().any(|b| *b != 0));
        assert_ne!(
            id,
            &solana_sdk::pubkey::Pubkey::new_unique().to_bytes()[..16]
        );
    }
    drop((wa, wb));
    for p in [a, b] {
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
#[test]
fn export_of_a_damaged_journal_leaves_no_partial_file() {
    let path = path();
    let export = path.with_extension("jsonl");
    let mut w = Writer::create(&path).unwrap();
    for n in 0..3 {
        w.append_value(kind::FE01_READ, &serde_json::json!({"n":n}))
            .unwrap();
    }
    w.sync().unwrap();
    drop(w);
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(bytes.len() - 1);
    fs::write(&path, bytes).unwrap();
    assert!(super::export(&path, &export, None, false).is_err());
    let names: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["events.p0j"]);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
#[test]
fn multi_view_export_is_all_or_nothing_and_never_overwrites() {
    let path = path();
    let dir = path.parent().unwrap().to_path_buf();
    let mut w = Writer::create(&path).unwrap();
    w.append_value(kind::FE01_READ, &serde_json::json!({"a":1}))
        .unwrap();
    w.append_value(kind::FE01_RESULT, &serde_json::json!({"b":2}))
        .unwrap();
    w.seal(true).unwrap();
    let (a, b) = (dir.join("a.jsonl"), dir.join("b.jsonl"));
    fs::write(&b, "keep").unwrap();
    assert!(super::export_views(
        &path,
        &[(Some(kind::FE01_READ), &a), (Some(kind::FE01_RESULT), &b)],
        false
    )
    .is_err());
    assert!(!a.exists());
    assert_eq!(fs::read_to_string(&b).unwrap(), "keep");
    fs::remove_file(&b).unwrap();
    assert_eq!(
        super::export_views(
            &path,
            &[(Some(kind::FE01_READ), &a), (Some(kind::FE01_RESULT), &b)],
            false
        )
        .unwrap(),
        [1, 1]
    );
    assert_eq!(fs::read_to_string(&a).unwrap(), "{\"a\":1}\n");
    assert_eq!(fs::read_to_string(&b).unwrap(), "{\"b\":2}\n");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 3);
    fs::remove_dir_all(dir).unwrap();
}
/// PERSIST-05 (5): local writer throughput, append/fsync tails and storage against JSONL for
/// representative payloads. Measurement only, run explicitly in release mode; a local
/// synthetic load, not a mainnet capture under provider load.
#[test]
#[ignore]
fn persist05_local_writer_benchmark() {
    use std::time::Instant;
    let logs: Vec<String> = (0..24)
        .map(|i| {
            format!(
                "Program log: Instruction: LendingAccountLiquidate {i} {}",
                "x".repeat(60)
            )
        })
        .collect();
    let market = serde_json::json!({"observed_at_utc":"2026-10-16T00:00:00.000000000Z","t_rx_ns":1234567890123u64,
        "slot":371234567u64,"signature":"5".repeat(88),"tx_version":null,"success":true,"err":null,"source":"logsSubscribe",
        "raw":{"context":{"slot":371234567u64},"value":{"signature":"5".repeat(88),"err":null,"logs":logs}}});
    let opportunity = serde_json::json!({"account":"7".repeat(44),"trigger":"oracle_threshold","decision":"Skip",
        "net_profit":-123456789i128,"economics_known":false,"rx_slot":371234567u64,"detect_slot":371234568u64,
        "timing":{"t_rx_ns":1u64,"t_state_ns":2u64,"t_detect_ns":3u64,"t_ready_ns":null,"t_sign_ns":null,"t_handoff_ns":null},
        "candidate_costs":(0..6).map(|i| serde_json::json!({"class":i,"fee":5000+i,"tip":0,"rent":2039280,"usd_micro":1234567})).collect::<Vec<_>>(),
        "health":{"maint_health_bits":-170141183460469231731687303715884105728i128,"equity_assets_bits":12345678901234567890i128}});
    let mt0 = serde_json::json!({"mt0_version":2,"unix_s":1,"remote":(0..8).map(|i| (format!("class_{i}"),
        serde_json::json!({"requests":i,"ok":i,"latency_ns":{"count":i,"p50":1,"p95":2,"p99":3,"max":4}}))).collect::<serde_json::Map<_,_>>()});
    let pct = |v: &mut Vec<u64>, q: f64| {
        v.sort_unstable();
        v[((v.len() as f64 - 1.0) * q).round() as usize]
    };
    let mut results = serde_json::Map::new();
    for (name, value, frames) in [
        ("market_row", &market, 100_000u64),
        ("opportunity", &opportunity, 100_000),
        ("mt0_snapshot", &mt0, 20_000),
    ] {
        let path = path();
        let mut w = Writer::create(&path).unwrap();
        let mut lat = Vec::with_capacity(frames as usize);
        let started = Instant::now();
        for _ in 0..frames {
            let t = Instant::now();
            w.append_value(kind::FE01_READ, value).unwrap();
            lat.push(t.elapsed().as_nanos() as u64);
        }
        let write_s = started.elapsed().as_secs_f64();
        let t = Instant::now();
        w.seal(true).unwrap();
        let seal_ns = t.elapsed().as_nanos() as u64;
        let bytes = fs::metadata(&path).unwrap().len();
        let jsonl = (serde_json::to_vec(value).unwrap().len() as u64 + 1) * frames;
        let t = Instant::now();
        let mut r = Reader::open(&path).unwrap();
        let mut read = 0u64;
        while r.next_frame().unwrap().is_some() {
            read += 1;
        }
        let read_s = t.elapsed().as_secs_f64();
        assert_eq!(read, frames + 1);
        results.insert(name.into(), serde_json::json!({"frames":frames,
            "append_ns":{"p50":pct(&mut lat,0.5),"p95":pct(&mut lat,0.95),"p99":pct(&mut lat,0.99),"max":pct(&mut lat,1.0)},
            "write_frames_per_s":(frames as f64/write_s).round(),"write_mib_per_s":((bytes as f64/write_s)/1048576.0*10.0).round()/10.0,
            "seal_fsync_ns":seal_ns,"verify_read_frames_per_s":(read as f64/read_s).round(),
            "journal_bytes_per_frame":bytes/frames,"jsonl_bytes_per_row":jsonl/frames,
            "journal_to_jsonl_ratio":((bytes as f64/jsonl as f64)*1000.0).round()/1000.0}));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    // Durable transitions (FE-04 automaton, phase rows): append + fsync per frame.
    let path = path();
    let mut w = Writer::create(&path).unwrap();
    let mut lat = Vec::new();
    for _ in 0..500 {
        let t = Instant::now();
        w.append_value(kind::FE01_READ, &opportunity).unwrap();
        w.sync().unwrap();
        lat.push(t.elapsed().as_nanos() as u64);
    }
    results.insert("durable_append_fsync".into(), serde_json::json!({"frames":500,
        "ns":{"p50":pct(&mut lat,0.5),"p95":pct(&mut lat,0.95),"p99":pct(&mut lat,0.99),"max":pct(&mut lat,1.0)}}));
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
    println!(
        "PERSIST05_LOCAL_BENCHMARK {}",
        serde_json::Value::Object(results)
    );
}

#[test]
fn fsync_error_never_returns_durable_ack_and_poisoning_is_sticky() {
    for kind in [io::ErrorKind::Other, io::ErrorKind::StorageFull] {
        let path = path();
        let mut writer = Writer::create(&path).unwrap();
        writer
            .append_value(
                kind::FE01_READ,
                &serde_json::json!({"reservation":"RESERVED"}),
            )
            .unwrap();
        writer.test_fail_next_sync(kind);
        assert_eq!(writer.sync().unwrap_err().kind(), kind);
        assert!(writer.sync().is_err());
        assert!(writer.append_value(kind::FE01_READ, &Value::Null).is_err());
        assert!(writer.seal(true).is_err());
        let mut reader = Reader::open(&path).unwrap();
        assert!(reader.next_frame().unwrap().is_some());
        assert!(reader.next_frame().unwrap().is_none());
        assert!(!reader.sealed && !reader.complete);
        drop(writer);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

/// JFI-01: typed frames are read only through their registered type and version.
#[test]
fn typed_frames_fail_closed_on_unknown_kind_version_or_truncation() {
    use super::super::payload::{self, OwnerDiagnostic, TYPED};
    let dir = std::env::temp_dir().join(format!("p0-typed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    let line = payload::encode(&OwnerDiagnostic { line: "x".into() }).unwrap();
    for (name, kind, schema, bytes) in [
        ("untyped", 20u16, TYPED, line.clone()),
        ("version", kind::OWNER_DIAGNOSTIC, TYPED + 1, line.clone()),
        (
            "truncated",
            kind::OWNER_DIAGNOSTIC,
            TYPED,
            line[..line.len() - 1].to_vec(),
        ),
    ] {
        let path = dir.join(format!("{name}.p0j"));
        let mut w = Writer::create(&path).unwrap();
        w.test_append_raw(kind, schema, &bytes).unwrap();
        w.sync().unwrap();
        let mut r = Reader::open(&path).unwrap();
        assert!(r.next_frame().is_err(), "{name}");
    }
    // The valid typed frame and the typed manifest read back; the frame is not a value tree.
    let path = dir.join("valid.p0j");
    let mut w = Writer::create(&path).unwrap();
    w.append_typed(Provenance::default(), &OwnerDiagnostic { line: "x".into() })
        .unwrap();
    w.seal(true).unwrap();
    let mut r = Reader::open(&path).unwrap();
    let f = r.next_frame().unwrap().unwrap();
    assert_eq!(f.typed::<OwnerDiagnostic>().unwrap().line, "x");
    assert!(f.legacy_value().is_err());
    assert!(r.next_frame().unwrap().is_some() && r.sealed && r.complete);
    fs::remove_dir_all(dir).unwrap();
}

/// Ticket 1: a typed kind is written only natively; schema 1 only on a registered migration path.
#[test]
fn write_admission_forbids_legacy_rollback_and_unregistered_kinds() {
    use super::super::payload::{self, OwnerDiagnostic, LEGACY_WRITABLE, TYPED};
    let path = path();
    let mut w = Writer::create(&path).unwrap();
    let diag = OwnerDiagnostic { line: "x".into() };
    w.append_typed(Provenance::default(), &diag).unwrap();
    let rollback = w.append_legacy(
        kind::OWNER_DIAGNOSTIC,
        Provenance::default(),
        &serde_json::json!({"line":"x"}),
    );
    assert_eq!(
        rollback.unwrap_err().to_string(),
        "LEGACY_WRITE_OF_TYPED_KIND"
    );
    assert_eq!(
        w.append_value(MANIFEST, &Value::Null)
            .unwrap_err()
            .to_string(),
        "LEGACY_WRITE_OF_TYPED_KIND"
    );
    assert_eq!(
        w.append_value(20, &Value::Null).unwrap_err().to_string(),
        "LEGACY_WRITE_UNREGISTERED_KIND"
    );
    let bytes = payload::encode(&diag).unwrap();
    for (k, schema, code) in [
        (kind::OWNER_DIAGNOSTIC, TYPED + 1, "PAYLOAD_SCHEMA"),
        (kind::FE01_READ, TYPED, "PAYLOAD_KIND_UNTYPED"),
        // Migrated (JFI-04 / JFI-FIX-05): a schema-1 frame of these kinds is a refused rollback.
        (kind::MT0_SNAPSHOT, 1, "LEGACY_WRITE_OF_TYPED_KIND"),
        (kind::MEASUREMENT_PHASE, 1, "LEGACY_WRITE_OF_TYPED_KIND"),
        (kind::ECON_SNAPSHOT, 1, "LEGACY_WRITE_OF_TYPED_KIND"),
        (kind::ECON_FINAL, 1, "LEGACY_WRITE_OF_TYPED_KIND"),
    ] {
        let e = w
            .append(k, schema, Provenance::default(), &bytes)
            .unwrap_err();
        assert_eq!(e.to_string(), code);
    }
    // A refused write leaves the journal usable: sequence and chain continue unchanged.
    w.append_legacy(kind::FE01_READ, Provenance::default(), &Value::Null)
        .unwrap();
    w.seal(true).unwrap();
    let mut r = Reader::open(&path).unwrap();
    let mut frames = 0;
    while r.next_frame().unwrap().is_some() {
        frames += 1;
    }
    assert_eq!((frames, r.complete), (3, true));
    // Registry invariant: every journal kind is typed or a migration path, never both.
    for k in kind::ALL {
        let typed = payload::registered_schema(*k).is_some();
        assert!(typed != LEGACY_WRITABLE.contains(k), "kind {k}");
    }
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

/// Ticket 1: journals written before a kind was migrated replay through the offline reader.
#[test]
fn historical_schema_one_frames_of_migrated_kinds_stay_readable() {
    let path = path();
    let mut w = Writer::create(&path).unwrap();
    let historical =
        serde_json::json!({"account":"x","decision":"Skip","maint_health_bits":i128::MIN});
    w.test_append_historical(kind::OWNER_RECORD, &historical)
        .unwrap();
    w.seal(true).unwrap();
    let mut r = Reader::open(&path).unwrap();
    let f = r.next_frame().unwrap().unwrap();
    assert_eq!((f.kind, f.schema), (kind::OWNER_RECORD, 1));
    assert_eq!(f.legacy_value().unwrap(), historical);
    assert!(f
        .typed::<crate::analytics::evidence::OwnerRecord>()
        .is_err());
    assert!(r.next_frame().unwrap().is_some() && r.complete);
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
