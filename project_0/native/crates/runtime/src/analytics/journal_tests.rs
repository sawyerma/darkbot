use super::*;
fn path() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "p0-journal-{}",
        solana_sdk::pubkey::Pubkey::new_unique()
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
        .append(20, 1, p.clone(), &serde_json::json!({"amount":i128::MIN}))
        .unwrap();
    let ack = w.sync().unwrap();
    assert_eq!(ack.journal_seq, 0);
    assert_eq!(ack.hash, f.hash);
    w.seal(true).unwrap();
    let mut r = Reader::open(&path).unwrap();
    let read = r.next_frame().unwrap().unwrap();
    assert_eq!(read.provenance, p);
    assert_eq!(read.value["amount"].to_string(), i128::MIN.to_string());
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
    w.append_value(1, &serde_json::json!({"a":123})).unwrap();
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
    w.append_value(1, &serde_json::json!({"intent":"RESERVED"}))
        .unwrap();
    w.sync().unwrap();
    drop(w);
    let mut w = Writer::resume(&path).unwrap();
    assert_eq!(w.next_seq(), 1);
    w.append_value(1, &serde_json::json!({"intent":"UNKNOWN"}))
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
    w.append_value(1, &Value::Null).unwrap();
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
    w.append_value(9, &value).unwrap();
    w.seal(true).unwrap();
    super::export(&path, &export, None, false).unwrap();
    assert_eq!(fs::read_to_string(&export).unwrap(), format!("{value}\n"));
    assert!(super::export(&path, &export, None, false).is_err());
    fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
