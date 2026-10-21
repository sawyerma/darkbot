use super::*;
fn event(sequence: u64) -> CaptureRecord {
    CaptureRecord { sequence, kind: RecordKind::Runtime, timestamp_ns: 1, payload: Box::from(&b"x"[..]) }
}
fn gap(sequence: u64, span: u64) -> CaptureRecord {
    CaptureRecord { sequence, kind: RecordKind::GapSpan, timestamp_ns: 0, payload: span.to_le_bytes().to_vec().into_boxed_slice() }
}
#[test]
fn full_queue_gaps_roundtrip_in_order_including_trailing_losses() {
    let (mut recorder, mut drain) = Recorder::bounded_spanned(1, 64).unwrap();
    assert_eq!(recorder.try_record(RecordKind::Runtime, 1, b"a"), Ok(0));
    for sequence in 1..4 {
        assert_eq!(recorder.try_record(RecordKind::Runtime, 1, b"lost"), Err(RecordError::QueueFull { sequence }));
    }
    let mut writer = JournalWriter::new_v2(Vec::new());
    writer.append(&drain.recv_spanned().unwrap()).unwrap();
    assert_eq!(recorder.try_record(RecordKind::Runtime, 1, b"b"), Ok(4));
    let lost = drain.recv_spanned().unwrap();
    assert_eq!(lost, gap(1, 3)); writer.append(&lost).unwrap();
    writer.append(&drain.recv_spanned().unwrap()).unwrap();
    recorder.try_record(RecordKind::Runtime, 1, b"c").unwrap(); // 5
    assert!(matches!(recorder.try_record(RecordKind::Runtime, 1, b"lost"), Err(RecordError::QueueFull { sequence: 6 })));
    assert_eq!(recorder.stats().dropped_full, 4);
    drop(recorder);
    writer.append(&drain.recv_spanned().unwrap()).unwrap();
    let tail = drain.recv_spanned().unwrap(); assert_eq!(tail, gap(6, 1));
    writer.append(&tail).unwrap();
    assert!(matches!(drain.recv_spanned(), Err(JournalError::QueueDisconnected)));
    let manifest = writer.manifest();
    assert_eq!((manifest.records, manifest.logical_records, manifest.lost_records), (5, 7, 4));
    assert_eq!((manifest.first_sequence, manifest.last_sequence), (Some(0), Some(6)));
    let bytes = writer.into_inner();
    let mut hash = Sha256::new(); hash.update(&bytes); assert_eq!(manifest.sha256, hash.finalize());
    let mut reader = JournalReader::new(bytes.as_slice(), 64).unwrap();
    for sequence in [0, 1, 4, 5, 6] { assert_eq!(reader.next_record().unwrap().unwrap().sequence, sequence); }
    assert!(reader.next_record().unwrap().is_none());
}
#[test]
fn legacy_gap_bytes_are_unchanged_and_v2_must_not_mix_versions() {
    // Pinned synthetic compatibility vector for the historical V1 semantics.
    // No historical production journal is available in this repository.
    let expected = include_bytes!("../testdata/v1-gap.bin");
    let record = CaptureRecord { sequence: 17, kind: RecordKind::Gap, timestamp_ns: 42, payload: Box::from(&b"drop=1"[..]) };
    let mut writer = JournalWriter::new(Vec::new()); writer.append(&record).unwrap();
    assert_eq!(writer.into_inner(), expected);
    let mut reader = JournalReader::new(expected.as_slice(), 64).unwrap();
    assert_eq!(reader.next_record().unwrap(), Some(record));
    assert!(reader.next_record().unwrap().is_none());
    let mut old = JournalWriter::new(Vec::new());
    assert!(matches!(old.append(&gap(0, 2)), Err(JournalError::InvalidSpan)));
    let mut new = JournalWriter::new_v2(Vec::new()); new.append(&event(18)).unwrap();
    let mixed = [expected.as_slice(), new.into_inner().as_slice()].concat();
    let mut reader = JournalReader::new(mixed.as_slice(), 64).unwrap();
    reader.next_record().unwrap();
    assert!(matches!(reader.next_record(), Err(JournalError::BadVersion(2))));
}
#[test]
fn invalid_spans_duplicates_and_corrupt_tails_are_rejected() {
    let mut writer = JournalWriter::new_v2(Vec::new());
    for invalid in [gap(0, 0), gap(2, u64::MAX), CaptureRecord { payload: Box::from(&b"short"[..]), ..gap(0, 1) }] {
        assert!(writer.append(&invalid).is_err());
        assert!(!writer.manifest().poisoned);
    }
    writer.append(&gap(0, 2)).unwrap();
    assert!(writer.append(&event(1)).is_err());
    writer.append(&event(2)).unwrap();
    let bytes = writer.into_inner();
    for mode in 0..3 {
        let mut broken = bytes.clone();
        match mode {
            0 => broken[HEADER_LEN] ^= 1,
            1 => { broken.pop(); },
            _ => {
                broken[HEADER_LEN..HEADER_LEN + 8].fill(0);
                let mut header = [0; HEADER_LEN]; header.copy_from_slice(&broken[..HEADER_LEN]);
                let crc = frame_crc(&header, &broken[HEADER_LEN..HEADER_LEN+8]);
                broken[32..36].copy_from_slice(&crc.to_le_bytes());
            }
        }
        let mut reader = JournalReader::new(broken.as_slice(), 64).unwrap();
        if mode == 1 { reader.next_record().unwrap(); }
        assert!(reader.next_record().is_err());
        assert!(matches!(reader.next_record(), Err(JournalError::Poisoned)));
    }
}
#[test]
fn empty_oversize_disconnected_and_exclusive_handoffs_do_not_invent_loss() {
    let (mut producer, mut drain) = Recorder::bounded_spanned(2, 2).unwrap();
    assert!(matches!(producer.try_record(RecordKind::Runtime, 0, b"big"), Err(RecordError::PayloadTooLarge { .. })));
    assert_eq!(producer.stats().next_sequence, 0);
    // Ownership transfer, not concurrent immutable access; mutable-owner
    // compile-fail doctest forbids the old reverse-enqueue race altogether.
    let producer = std::thread::spawn(move || { producer.try_record(RecordKind::Runtime, 0, b"a").unwrap(); producer }).join().unwrap();
    drop(producer);
    assert_eq!(drain.recv_spanned().unwrap().kind, RecordKind::Runtime);
    assert!(matches!(drain.recv_spanned(), Err(JournalError::QueueDisconnected)));
    let (producer, mut drain) = Recorder::bounded_spanned(1, 2).unwrap(); drop(producer);
    assert!(matches!(drain.recv_spanned(), Err(JournalError::QueueDisconnected)));
    let (mut producer, drain) = Recorder::bounded_spanned(1, 2).unwrap(); drop(drain);
    assert!(matches!(producer.try_record(RecordKind::Runtime, 0, b"x"), Err(RecordError::Disconnected { sequence: 0 })));
    assert_eq!(producer.stats().disconnected, 1);
    assert_eq!(producer.stats().dropped_full, 0);
}
