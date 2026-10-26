//! Reading one segmented run (master plan §11, storage contract; Phase F
//! contract §9): its segments in index order as one continuous sequence,
//! every frame verified by the one `DBJNL001` reader (seeded with the
//! previous segment's last sequence) and every segment hashed over its
//! exact bytes in the same pass. The census and the replay proof read a
//! run exactly as they read a single journal.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use crate::Sha256;
use crate::format::{CaptureRecord, JournalError};
use crate::replay::JournalReader;
use crate::storage::segments::list_segments;

/// Hashes every byte read through it.
struct Hashing<R> {
    inner: R,
    hasher: Sha256,
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

/// One segment read to its end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentRead {
    pub index: u32,
    pub path: PathBuf,
    pub sealed: bool,
    /// Frames, and the logical sequence span they carry.
    pub records: u64,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    /// SHA-256 of the exact file bytes (the seal's hash).
    pub sha256: [u8; 32],
}

struct Current {
    index: u32,
    path: PathBuf,
    sealed: bool,
    reader: JournalReader<BufReader<Hashing<File>>>,
    records: u64,
    first_sequence: Option<u64>,
}

/// Reads the records of one run directory across its segments.
pub struct RunReader {
    pending: std::vec::IntoIter<(u32, PathBuf, bool)>,
    current: Option<Current>,
    last_sequence: Option<u64>,
    max_payload: usize,
    read: Vec<SegmentRead>,
}

impl RunReader {
    /// Lists the segments of `dir`; a missing index (a segment taken away)
    /// or an open segment before the last is refused.
    pub fn open(dir: &Path, max_payload: usize) -> Result<Self, JournalError> {
        if max_payload == 0 {
            return Err(JournalError::InvalidBound);
        }
        let found = list_segments(dir)?;
        let last = found.len().saturating_sub(1);
        for (position, (index, _, sealed)) in found.iter().enumerate() {
            if usize::try_from(*index).ok() != Some(position) || (!sealed && position != last) {
                return Err(JournalError::FileMismatch);
            }
        }
        Ok(Self {
            pending: found.into_iter(),
            current: None,
            last_sequence: None,
            max_payload,
            read: Vec::new(),
        })
    }

    /// The next record of the run. A torn or corrupt sealed segment is a
    /// `FileMismatch` (its bytes differ from its seal); only the open last
    /// segment may end `Truncated` (a running or interrupted capture).
    pub fn next_record(&mut self) -> Result<Option<CaptureRecord>, JournalError> {
        loop {
            if self.current.is_none() {
                let Some((index, path, sealed)) = self.pending.next() else {
                    return Ok(None);
                };
                let file = File::open(&path).map_err(JournalError::Io)?;
                let hashing = Hashing {
                    inner: file,
                    hasher: Sha256::new(),
                };
                let reader = JournalReader::continuing(
                    BufReader::new(hashing),
                    self.max_payload,
                    self.last_sequence,
                )?;
                self.current = Some(Current {
                    index,
                    path,
                    sealed,
                    reader,
                    records: 0,
                    first_sequence: None,
                });
            }
            let current = self.current.as_mut().expect("opened above");
            match current.reader.next_record() {
                Ok(Some(record)) => {
                    current.records += 1;
                    current.first_sequence.get_or_insert(record.sequence);
                    return Ok(Some(record));
                }
                Ok(None) => {
                    let c = self.current.take().expect("opened above");
                    let last_sequence = c.reader.last_sequence();
                    let sha256 = c.reader.into_inner().into_inner().hasher.finalize();
                    self.last_sequence = last_sequence.or(self.last_sequence);
                    self.read.push(SegmentRead {
                        index: c.index,
                        path: c.path,
                        sealed: c.sealed,
                        records: c.records,
                        first_sequence: c.first_sequence,
                        last_sequence,
                        sha256,
                    });
                }
                Err(JournalError::Truncated) if !current.sealed => {
                    return Err(JournalError::Truncated);
                }
                Err(JournalError::Truncated) => return Err(JournalError::FileMismatch),
                Err(e) => return Err(e),
            }
        }
    }

    /// The segments read to their end so far, in order.
    pub fn segments(&self) -> &[SegmentRead] {
        &self.read
    }

    /// The last logical sequence read so far.
    pub fn last_sequence(&self) -> Option<u64> {
        self.current
            .as_ref()
            .and_then(|c| c.reader.last_sequence())
            .or(self.last_sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::RecordKind;
    use crate::storage::segments::{SegmentPolicy, SegmentWriter};
    use std::fs;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bsc-run-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn record(sequence: u64) -> CaptureRecord {
        CaptureRecord {
            sequence,
            kind: RecordKind::Runtime,
            timestamp_ns: sequence,
            payload: vec![sequence as u8; 40].into_boxed_slice(),
        }
    }

    fn write(d: &Path, n: u64) -> Vec<crate::storage::segments::SealedSegment> {
        let mut w = SegmentWriter::create(
            d,
            SegmentPolicy {
                max_segment_bytes: 200,
                max_segment_ns: u64::MAX,
            },
        )
        .unwrap();
        let mut sealed = Vec::new();
        for i in 0..n {
            sealed.extend(w.append(&record(i)).unwrap());
        }
        sealed.extend(w.finish().unwrap());
        sealed
    }

    /// Every record of every segment in order, each segment's hash equal
    /// to its seal's.
    #[test]
    fn a_sealed_run_reads_as_one_sequence_with_the_seal_hashes() {
        let d = dir("sealed");
        let sealed = write(&d, 12);
        assert!(sealed.len() >= 3);
        let mut r = RunReader::open(&d, 1024).unwrap();
        let mut sequences = Vec::new();
        while let Some(rec) = r.next_record().unwrap() {
            sequences.push(rec.sequence);
        }
        assert_eq!(sequences, (0..12).collect::<Vec<_>>());
        assert_eq!(r.segments().len(), sealed.len());
        for (read, seal) in r.segments().iter().zip(&sealed) {
            assert_eq!(read.sha256, seal.manifest.sha256);
            assert_eq!(read.records, seal.manifest.records);
            assert_eq!(read.last_sequence, seal.manifest.last_sequence);
        }
        assert_eq!(r.last_sequence(), Some(11));
        fs::remove_dir_all(&d).unwrap();
    }

    /// A segment taken away, a torn sealed segment and a sequence break
    /// across segments are refused; a torn open last segment is a tail.
    #[test]
    fn missing_torn_and_discontinuous_runs_are_refused() {
        let d = dir("missing");
        write(&d, 12);
        fs::remove_file(d.join("segment_000001.dbjnl")).unwrap();
        assert!(matches!(
            RunReader::open(&d, 1024),
            Err(JournalError::FileMismatch)
        ));
        fs::remove_dir_all(&d).unwrap();

        let d = dir("torn");
        write(&d, 12);
        let first = d.join("segment_000000.dbjnl");
        let len = fs::metadata(&first).unwrap().len();
        fs::OpenOptions::new()
            .write(true)
            .open(&first)
            .unwrap()
            .set_len(len - 3)
            .unwrap();
        let mut r = RunReader::open(&d, 1024).unwrap();
        let end = loop {
            match r.next_record() {
                Ok(Some(_)) => {}
                other => break other,
            }
        };
        assert!(matches!(end, Err(JournalError::FileMismatch)));
        // The same tear in the open last segment is a capture's tail.
        let open = d.join("segment_000000.dbjnl.open");
        fs::rename(&first, &open).unwrap();
        for s in fs::read_dir(&d).unwrap() {
            let p = s.unwrap().path();
            if p != open {
                fs::remove_file(p).unwrap();
            }
        }
        let mut r = RunReader::open(&d, 1024).unwrap();
        let end = loop {
            match r.next_record() {
                Ok(Some(_)) => {}
                other => break other,
            }
        };
        assert!(matches!(end, Err(JournalError::Truncated)));
        fs::remove_dir_all(&d).unwrap();

        // Two runs' segments mixed: the second does not continue the first.
        let (a, b) = (dir("a"), dir("b"));
        write(&a, 12);
        write(&b, 12);
        fs::copy(
            b.join("segment_000000.dbjnl"),
            a.join("segment_000001.dbjnl"),
        )
        .unwrap();
        let mut r = RunReader::open(&a, 1024).unwrap();
        let end = loop {
            match r.next_record() {
                Ok(Some(_)) => {}
                other => break other,
            }
        };
        assert!(matches!(end, Err(JournalError::SequenceGap { .. })));
        fs::remove_dir_all(&a).unwrap();
        fs::remove_dir_all(&b).unwrap();
    }
}
