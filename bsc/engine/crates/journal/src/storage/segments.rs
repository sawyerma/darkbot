//! Segmented capture (master plan §11, storage contract): one run is a
//! directory of `DBJNL001` V2 segments with one continuous sequence. A
//! segment is written as `segment_<index>.dbjnl.open`, rotated only at a
//! complete frame, and sealed by the existing commit (flush, fsync, exact
//! read-back hash), an atomic rename to `segment_<index>.dbjnl` and a
//! directory sync. [`recover`] validates a run after a crash: sealed
//! segments completely, an open one up to its last complete frame, never
//! accepting a torn or corrupt tail as complete.

use std::fs::{self, File, OpenOptions};
use std::io::BufReader;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::format::{CaptureRecord, JournalError, JournalManifest, JournalWriter};
use crate::replay::JournalReader;

const OPEN_SUFFIX: &str = ".dbjnl.open";
const SEALED_SUFFIX: &str = ".dbjnl";

/// When a segment is rotated: after this many bytes or this much record
/// time, at the next frame boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentPolicy {
    pub max_segment_bytes: u64,
    pub max_segment_ns: u64,
}

/// One sealed segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedSegment {
    pub index: u32,
    pub path: PathBuf,
    pub manifest: JournalManifest,
    pub first_timestamp_ns: Option<u64>,
    pub last_timestamp_ns: Option<u64>,
}

struct OpenSegment {
    index: u32,
    path: PathBuf,
    writer: JournalWriter<File>,
    first_timestamp_ns: Option<u64>,
    last_timestamp_ns: Option<u64>,
}

/// Writes one run's segments.
pub struct SegmentWriter {
    dir: PathBuf,
    policy: SegmentPolicy,
    next_index: u32,
    /// The sequence the next record must carry (continuity across
    /// segments); `None` before the first.
    next_sequence: Option<u64>,
    open: Option<OpenSegment>,
}

fn segment_path(dir: &Path, index: u32, suffix: &str) -> PathBuf {
    dir.join(format!("segment_{index:06}{suffix}"))
}

fn sync_dir(dir: &Path) -> Result<(), JournalError> {
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(JournalError::Io)
}

impl SegmentWriter {
    /// Starts a run in `dir`, which must not exist yet (created 0700).
    pub fn create(dir: &Path, policy: SegmentPolicy) -> Result<Self, JournalError> {
        if policy.max_segment_bytes == 0 || policy.max_segment_ns == 0 {
            return Err(JournalError::InvalidBound);
        }
        fs::DirBuilder::new()
            .mode(0o700)
            .create(dir)
            .map_err(JournalError::Io)?;
        if let Some(parent) = dir.parent() {
            sync_dir(parent)?;
        }
        Ok(Self {
            dir: dir.to_path_buf(),
            policy,
            next_index: 0,
            next_sequence: None,
            open: None,
        })
    }

    fn open_next(&mut self) -> Result<&mut OpenSegment, JournalError> {
        let index = self.next_index;
        let path = segment_path(&self.dir, index, OPEN_SUFFIX);
        // Read too: the seal reads the exact bytes back.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(JournalError::Io)?;
        sync_dir(&self.dir)?;
        self.next_index = index.checked_add(1).ok_or(JournalError::SequenceOverflow)?;
        Ok(self.open.insert(OpenSegment {
            index,
            path,
            writer: JournalWriter::new_v2(file),
            first_timestamp_ns: None,
            last_timestamp_ns: None,
        }))
    }

    fn seal_open(&mut self) -> Result<Option<SealedSegment>, JournalError> {
        let Some(open) = self.open.take() else {
            return Ok(None);
        };
        let manifest = open.writer.commit_file()?;
        let sealed = segment_path(&self.dir, open.index, SEALED_SUFFIX);
        fs::rename(&open.path, &sealed).map_err(JournalError::Io)?;
        sync_dir(&self.dir)?;
        Ok(Some(SealedSegment {
            index: open.index,
            path: sealed,
            manifest,
            first_timestamp_ns: open.first_timestamp_ns,
            last_timestamp_ns: open.last_timestamp_ns,
        }))
    }

    /// Appends `record` (the next of the run's continuous sequence). When
    /// the open segment reached a bound, it is sealed first and returned.
    pub fn append(
        &mut self,
        record: &CaptureRecord,
    ) -> Result<Option<SealedSegment>, JournalError> {
        if let Some(expected) = self.next_sequence
            && record.sequence != expected
        {
            return Err(JournalError::SequenceGap {
                expected,
                observed: record.sequence,
            });
        }
        let due = self.open.as_ref().is_some_and(|o| {
            let m = o.writer.manifest();
            m.bytes >= self.policy.max_segment_bytes
                || o.first_timestamp_ns.is_some_and(|first| {
                    record.timestamp_ns.saturating_sub(first) >= self.policy.max_segment_ns
                })
        });
        let sealed = if due { self.seal_open()? } else { None };
        let open = match &mut self.open {
            Some(_) => self.open.as_mut().ok_or(JournalError::Poisoned)?,
            None => self.open_next()?,
        };
        open.writer.append(record)?;
        open.first_timestamp_ns.get_or_insert(record.timestamp_ns);
        open.last_timestamp_ns = Some(record.timestamp_ns);
        let last = open
            .writer
            .manifest()
            .last_sequence
            .ok_or(JournalError::Poisoned)?;
        self.next_sequence = Some(last.checked_add(1).ok_or(JournalError::SequenceOverflow)?);
        Ok(sealed)
    }

    /// Flushes and fsyncs the open segment: everything appended so far is
    /// durable (the durability watermark).
    pub fn sync(&mut self) -> Result<(), JournalError> {
        match &mut self.open {
            Some(open) => open.writer.sync_file(),
            None => Ok(()),
        }
    }

    /// The last sequence written, if any.
    pub fn last_sequence(&self) -> Option<u64> {
        self.next_sequence.map(|n| n - 1)
    }

    /// Seals the open segment (the run's end); `None` when none is open.
    pub fn finish(mut self) -> Result<Option<SealedSegment>, JournalError> {
        self.seal_open()
    }
}

/// What [`recover`] found of one segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveredSegment {
    pub index: u32,
    pub path: PathBuf,
    /// Sealed (renamed after commit) or found open (interrupted).
    pub sealed: bool,
    /// Complete frames read, their logical sequence span and bytes.
    pub records: u64,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub complete_bytes: u64,
    /// Why reading stopped before the end of the file, if it did: a torn
    /// or corrupt tail (never accepted as complete).
    pub tail_error: Option<String>,
}

/// The state of a run directory after a crash or a clean end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovery {
    pub segments: Vec<RecoveredSegment>,
    /// Every segment sealed, complete and continuous.
    pub complete: bool,
    /// A sequence break between consecutive segments.
    pub discontinuity: Option<(u32, u64, u64)>,
}

/// The segments of a run directory in index order: `(index, path, sealed)`.
pub(crate) fn list_segments(dir: &Path) -> Result<Vec<(u32, PathBuf, bool)>, JournalError> {
    let mut found: Vec<(u32, PathBuf, bool)> = Vec::new();
    for entry in fs::read_dir(dir).map_err(JournalError::Io)? {
        let path = entry.map_err(JournalError::Io)?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let (stem, sealed) = if let Some(stem) = name.strip_suffix(OPEN_SUFFIX) {
            (stem, false)
        } else if let Some(stem) = name.strip_suffix(SEALED_SUFFIX) {
            (stem, true)
        } else {
            continue;
        };
        let Some(index) = stem
            .strip_prefix("segment_")
            .and_then(|i| i.parse::<u32>().ok())
        else {
            continue;
        };
        found.push((index, path, sealed));
    }
    found.sort_by_key(|(index, _, _)| *index);
    Ok(found)
}

/// Validates the segments of `dir` in index order: every frame, checksum
/// and sequence, and the continuity across segments.
pub fn recover(dir: &Path, max_payload: usize) -> Result<Recovery, JournalError> {
    let found = list_segments(dir)?;
    let mut segments = Vec::with_capacity(found.len());
    let mut complete = true;
    let mut discontinuity = None;
    let mut expected: Option<u64> = None;
    for (position, (index, path, sealed)) in found.into_iter().enumerate() {
        if usize::try_from(index).ok() != Some(position) {
            // A missing segment index.
            complete = false;
        }
        let file = File::open(&path).map_err(JournalError::Io)?;
        let length = file.metadata().map_err(JournalError::Io)?.len();
        let mut reader = JournalReader::new(BufReader::new(file), max_payload)?;
        let (mut records, mut first, mut last, mut bytes) = (0u64, None, None, 0u64);
        let mut tail_error = None;
        loop {
            match reader.next_record() {
                Ok(Some(record)) => {
                    records += 1;
                    first.get_or_insert(record.sequence);
                    let span = crate::format::logical_span(
                        crate::format::JOURNAL_SPAN_VERSION,
                        record.kind,
                        &record.payload,
                    )
                    .unwrap_or(1);
                    last = Some(record.sequence + span - 1);
                    bytes += (crate::format::HEADER_LEN + record.payload.len()) as u64;
                }
                Ok(None) => break,
                Err(e) => {
                    tail_error = Some(e.to_string());
                    break;
                }
            }
        }
        if bytes != length && tail_error.is_none() {
            tail_error = Some(format!("{} bytes after the last frame", length - bytes));
        }
        if !sealed || tail_error.is_some() {
            complete = false;
        }
        if let (Some(want), Some(got)) = (expected, first)
            && want != got
            && discontinuity.is_none()
        {
            discontinuity = Some((index, want, got));
            complete = false;
        }
        if let Some(last) = last {
            expected = last.checked_add(1);
        }
        segments.push(RecoveredSegment {
            index,
            path,
            sealed,
            records,
            first_sequence: first,
            last_sequence: last,
            complete_bytes: bytes,
            tail_error,
        });
    }
    Ok(Recovery {
        segments,
        complete,
        discontinuity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::RecordKind;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bsc-segments-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn record(sequence: u64, timestamp_ns: u64, len: usize) -> CaptureRecord {
        CaptureRecord {
            sequence,
            kind: RecordKind::Runtime,
            timestamp_ns,
            payload: vec![sequence as u8; len].into_boxed_slice(),
        }
    }

    /// Rotation at frame boundaries by bytes and by time; one continuous
    /// sequence across sealed segments; a clean end recovers complete.
    #[test]
    fn segments_rotate_seal_and_recover_complete() {
        let d = dir("rotate");
        let mut w = SegmentWriter::create(
            &d,
            SegmentPolicy {
                max_segment_bytes: 200,
                max_segment_ns: 1_000,
            },
        )
        .unwrap();
        let mut sealed = Vec::new();
        for i in 0..10u64 {
            // 36-byte header + 40: the third record of a segment crosses 200.
            if let Some(s) = w.append(&record(i, i * 10, 40)).unwrap() {
                sealed.push(s);
            }
        }
        // Time bound: 1000 ns after the segment's first record.
        if let Some(s) = w.append(&record(10, 5_000, 40)).unwrap() {
            sealed.push(s);
        }
        assert!(w.append(&record(12, 5_001, 1)).is_err(), "a gap is refused");
        w.sync().unwrap();
        sealed.extend(w.finish().unwrap());
        assert!(sealed.len() >= 4);
        for pair in sealed.windows(2) {
            assert_eq!(
                pair[0].manifest.last_sequence.unwrap() + 1,
                pair[1].manifest.first_sequence.unwrap()
            );
        }
        let r = recover(&d, 1024).unwrap();
        assert!(r.complete, "{r:?}");
        assert_eq!(r.segments.len(), sealed.len());
        assert_eq!(r.segments.last().unwrap().last_sequence, Some(10));
        fs::remove_dir_all(&d).unwrap();
    }

    /// A crash leaves an open segment with a torn last frame: recovery
    /// reports its complete prefix and the tail, never complete.
    #[test]
    fn an_interrupted_segment_is_recovered_up_to_its_last_frame() {
        let d = dir("crash");
        let mut w = SegmentWriter::create(
            &d,
            SegmentPolicy {
                max_segment_bytes: 1 << 20,
                max_segment_ns: u64::MAX,
            },
        )
        .unwrap();
        for i in 0..3 {
            w.append(&record(i, i, 20)).unwrap();
        }
        w.sync().unwrap();
        drop(w); // killed: never sealed
        let open = segment_path(&d, 0, OPEN_SUFFIX);
        let len = fs::metadata(&open).unwrap().len();
        let f = OpenOptions::new().write(true).open(&open).unwrap();
        f.set_len(len - 5).unwrap();
        let r = recover(&d, 1024).unwrap();
        assert!(!r.complete);
        let s = &r.segments[0];
        assert!(!s.sealed);
        assert_eq!((s.records, s.last_sequence), (2, Some(1)));
        assert!(s.tail_error.is_some());
        // A corrupt byte inside a sealed segment is found as well.
        let d2 = dir("corrupt");
        let mut w = SegmentWriter::create(
            &d2,
            SegmentPolicy {
                max_segment_bytes: 1 << 20,
                max_segment_ns: u64::MAX,
            },
        )
        .unwrap();
        w.append(&record(0, 0, 20)).unwrap();
        let s = w.finish().unwrap().unwrap();
        let mut bytes = fs::read(&s.path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&s.path, bytes).unwrap();
        let r = recover(&d2, 1024).unwrap();
        assert!(!r.complete && r.segments[0].tail_error.is_some());
        fs::remove_dir_all(&d).unwrap();
        fs::remove_dir_all(&d2).unwrap();
    }

    /// A run directory is never reused.
    #[test]
    fn a_run_directory_is_created_once() {
        let d = dir("once");
        let policy = SegmentPolicy {
            max_segment_bytes: 1,
            max_segment_ns: 1,
        };
        SegmentWriter::create(&d, policy).unwrap();
        assert!(SegmentWriter::create(&d, policy).is_err());
        fs::remove_dir_all(&d).unwrap();
    }
}
