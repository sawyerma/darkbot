//! SK-07: segmented journals of the one `.p0j` contract (no second codec, no format change).
//!
//! A long journal is a chain of complete journals: the first segment keeps the base name
//! (`market.p0j`), every following one is `market.000002.p0j`, `market.000003.p0j`, … Before a
//! segment is sealed it gets a typed `SEGMENT_NEXT` frame naming its successor; the successor's
//! first frame is a typed `SEGMENT_LINK` binding the predecessor's file name, frame count and
//! sealed chain root (the hash of its manifest frame, which transitively covers every byte) plus
//! the cumulative event count, all under one random set id. Sealed segments are never changed.
//!
//! `ChainReader` replays the whole chain in order and returns only event frames (plus the last
//! segment's manifest; continued segments' manifests are chain-internal). It fails closed
//! on an interior missing successor, a broken or foreign link, an unsealed intermediate segment, corrupt
//! frames (CRC, chain, sequence) and orphan segment files beyond the chain end. An unsealed last
//! segment or physically torn tail (crash/power loss) is reported as such (`sealed = false`): INCOMPLETE, never repaired.
//! A reader that does not know segments stops at the first segment's control frame or kind
//! filter; every market reader of this crate goes through the chain reader.
//!
//! Recovery (DEC-16, unbounded runs): a run restarted after a crash continues its chain without
//! touching the crashed segment — it is never sealed, appended to or truncated afterwards. The
//! successor's first frame is a typed `SEGMENT_RECOVERY` binding the crashed segment's readable
//! frames, their chain head and the unreadable tail bytes; the chain reader accepts an unsealed
//! segment only so continued and reports every recovery (`ChainSummary::recovered`). A rotation
//! interrupted after its seal is finished with the announced successor and its normal link.
use crate::analytics::journal::{self, kind, Frame, Provenance, Writer};
use crate::analytics::payload::Payload;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

fn bad(code: &'static str) -> io::Error {
    crate::analytics::journal::coded(code)
}

/// The sealed predecessor as its successor binds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentRef {
    pub file: String,
    pub index: u32,
    /// Frames of the segment including its control frames and manifest.
    pub frames: u64,
    /// Chain head after the manifest frame.
    pub root_sha256: [u8; 32],
    /// Event frames in all segments up to and including this one.
    pub events_through: u64,
}

/// First frame of every segment after the first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentLink {
    pub set_id: [u8; 16],
    pub index: u32,
    pub previous: SegmentRef,
}
impl Payload for SegmentLink {
    const KIND: u16 = kind::SEGMENT_LINK;
}

/// Last event-position frame of a segment that continues.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentNext {
    pub set_id: [u8; 16],
    pub index: u32,
    pub next_file: String,
}
impl Payload for SegmentNext {
    const KIND: u16 = kind::SEGMENT_NEXT;
}

/// First frame of the segment that continues a chain after a crash (DEC-16): `previous` names
/// the crashed segment's readable frames (`frames`, `root_sha256` = chain head after them) and
/// `torn_bytes` its unreadable tail; the crashed file itself stays unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentRecovery {
    pub set_id: [u8; 16],
    pub index: u32,
    pub previous: SegmentRef,
    pub torn_bytes: u64,
}
impl Payload for SegmentRecovery {
    const KIND: u16 = kind::SEGMENT_RECOVERY;
}

/// How [`SegmentedWriter::recover`] continued a chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovered {
    /// The crashed (unsealed) segment `segment` is continued by a `SEGMENT_RECOVERY` successor.
    Crash { segment: u32, torn_bytes: u64 },
    /// A rotation interrupted after sealing `segment` is finished with its announced successor.
    Rotation { segment: u32 },
}

/// Path of segment `index` (1-based) of the chain starting at `first`.
pub fn segment_path(first: &Path, index: u32) -> PathBuf {
    if index <= 1 {
        return first.to_path_buf();
    }
    let stem = first
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("journal");
    first.with_file_name(format!("{stem}.{index:06}.p0j"))
}

/// Whether `path` names a continuation segment (`stem.NNNNNN.p0j`), not a chain head.
pub fn is_continuation(path: &Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.rsplit_once('.'))
        .is_some_and(|(_, n)| n.len() == 6 && n.bytes().all(|b| b.is_ascii_digit()))
        && path.extension().and_then(|x| x.to_str()) == Some("p0j")
}

/// Creates segment `path` with its first control frame atomically (DEC-16): written and synced
/// under a temporary name, then linked into place — never over an existing file — so a crash
/// leaves no header-only segment that would block the chain's recovery.
fn create_with<P: Payload>(path: &Path, first: &P) -> io::Result<Writer> {
    let temp = temp_path(path);
    let _ = std::fs::remove_file(&temp);
    let mut w = Writer::create(&temp)?;
    let created = (|| {
        w.append_typed(
            Provenance {
                source: journal::source::STORAGE,
                ..Default::default()
            },
            first,
        )?;
        w.sync()?;
        std::fs::hard_link(&temp, path)?;
        std::fs::File::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?
        .sync_all()
    })();
    let _ = std::fs::remove_file(&temp);
    created?;
    w.path = path.to_path_buf();
    Ok(w)
}

/// The temporary name a segment is written under before it is linked into place.
fn temp_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(".{}.tmp", file_name(path)))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Result of sealing a segmented journal.
#[derive(Clone, Debug)]
pub struct SealedSet {
    pub durable: journal::Durable,
    pub segments: u32,
    pub last_file: PathBuf,
    pub bytes: u64,
}

/// One logical journal written as rotating segments.
pub struct SegmentedWriter {
    first: PathBuf,
    writer: Writer,
    index: u32,
    set_id: [u8; 16],
    /// Event frames in the current segment and in all sealed ones.
    events_current: u64,
    events_sealed: u64,
    sealed_bytes: u64,
    max_segment_bytes: u64,
    /// DEC-16: the chain was continued after a crash; its seal is never complete.
    recovered: bool,
}

impl SegmentedWriter {
    /// Creates the first segment (never overwrites). `max_segment_bytes == 0` disables rotation.
    pub fn create(first: &Path, max_segment_bytes: u64) -> io::Result<Self> {
        Ok(Self {
            first: first.into(),
            writer: Writer::create(first)?,
            index: 1,
            set_id: journal::random16()?,
            events_current: 0,
            events_sealed: 0,
            sealed_bytes: 0,
            max_segment_bytes,
            recovered: false,
        })
    }
    /// DEC-16: continues the chain at `first` after a crash (see the module docs). A chain that
    /// ended in order (its last segment sealed) is refused: a new run is a new chain.
    pub fn recover(first: &Path, max_segment_bytes: u64) -> io::Result<(Self, Recovered)> {
        let mut r = ChainReader::open(first)?;
        while r.next_frame()?.is_some() {}
        let index = r.segments();
        let crashed = segment_path(first, index);
        let previous = SegmentRef {
            file: file_name(&crashed),
            index,
            frames: r.reader.frames(),
            root_sha256: r.reader.root(),
            events_through: r.events,
        };
        let next_path = segment_path(first, index + 1);
        let (set_id, recovered, next) = if r.reader.sealed {
            // Sealed with an announced, missing successor: the interrupted rotation.
            let (Some(set_id), true) = (r.set_id, r.announced_missing) else {
                return Err(bad("SEGMENT_CHAIN_SEALED"));
            };
            let next = create_with(
                &next_path,
                &SegmentLink {
                    set_id,
                    index: index + 1,
                    previous,
                },
            )?;
            (set_id, Recovered::Rotation { segment: index }, next)
        } else {
            let torn_bytes = std::fs::metadata(&crashed)?
                .len()
                .saturating_sub(r.reader.position());
            let set_id = match r.set_id {
                Some(id) => id,
                None => journal::random16()?,
            };
            let next = create_with(
                &next_path,
                &SegmentRecovery {
                    set_id,
                    index: index + 1,
                    previous,
                    torn_bytes,
                },
            )?;
            (
                set_id,
                Recovered::Crash {
                    segment: index,
                    torn_bytes,
                },
                next,
            )
        };
        let mut sealed_bytes = 0;
        for i in 1..=index {
            sealed_bytes += std::fs::metadata(segment_path(first, i))?.len();
        }
        Ok((
            Self {
                first: first.into(),
                writer: next,
                index: index + 1,
                set_id,
                events_current: 0,
                events_sealed: r.events,
                sealed_bytes,
                max_segment_bytes,
                recovered: matches!(recovered, Recovered::Crash { .. }) || r.recovered > 0,
            },
            recovered,
        ))
    }
    pub fn segments(&self) -> u32 {
        self.index
    }
    /// Bytes of all segments written so far.
    pub fn bytes(&self) -> u64 {
        self.sealed_bytes + self.writer.position()
    }
    pub fn current_path(&self) -> &Path {
        &self.writer.path
    }
    pub fn append_batch<'a, I>(&mut self, frames: I) -> io::Result<u64>
    where
        I: IntoIterator<Item = (u16, u16, &'a Provenance, &'a [u8])>,
        I::IntoIter: Clone,
    {
        let n = self.writer.append_batch(frames)?;
        self.events_current += n;
        Ok(n)
    }
    /// One typed event frame; returns the segment it landed in with its position there.
    pub fn append_typed<P: Payload>(
        &mut self,
        p: Provenance,
        value: &P,
    ) -> io::Result<(u32, journal::Appended)> {
        let appended = self.writer.append_typed(p, value)?;
        self.events_current += 1;
        Ok((self.index, appended))
    }
    /// Byte position in the current segment.
    pub fn position(&self) -> u64 {
        self.writer.position()
    }
    pub fn sync(&mut self) -> io::Result<journal::Durable> {
        self.writer.sync()
    }
    /// DEC-16: rotates now (a rolling window's boundary; between batches only); returns the
    /// sealed segment as its successor binds it.
    pub fn rotate_now(&mut self) -> io::Result<SegmentRef> {
        self.rotate()
    }
    /// Rotates when the current segment reached the size limit (between batches only).
    pub fn rotate_if_due(&mut self) -> io::Result<bool> {
        if self.max_segment_bytes == 0 || self.writer.position() < self.max_segment_bytes {
            return Ok(false);
        }
        self.rotate()?;
        Ok(true)
    }
    fn rotate(&mut self) -> io::Result<SegmentRef> {
        let next_index = self.index + 1;
        let next_path = segment_path(&self.first, next_index);
        let control = Provenance {
            source: journal::source::STORAGE,
            ..Default::default()
        };
        self.writer.append_typed(
            control,
            &SegmentNext {
                set_id: self.set_id,
                index: self.index,
                next_file: file_name(&next_path),
            },
        )?;
        let durable = self.writer.seal(true)?;
        let previous = SegmentRef {
            file: file_name(&self.writer.path),
            index: self.index,
            frames: durable.journal_seq + 1,
            root_sha256: durable.hash,
            events_through: self.events_sealed + self.events_current,
        };
        let next = create_with(
            &next_path,
            &SegmentLink {
                set_id: self.set_id,
                index: next_index,
                previous: previous.clone(),
            },
        )?;
        self.sealed_bytes += self.writer.position();
        self.events_sealed += self.events_current;
        self.events_current = 0;
        self.writer = next;
        self.index = next_index;
        Ok(previous)
    }
    pub fn seal(&mut self, complete: bool) -> io::Result<SealedSet> {
        // A chain continued after a crash may have lost frames: its manifest says so too.
        let durable = self.writer.seal(complete && !self.recovered)?;
        Ok(SealedSet {
            durable,
            segments: self.index,
            last_file: self.writer.path.clone(),
            bytes: self.sealed_bytes + self.writer.position(),
        })
    }
}

/// Replays a segment chain in order, returning only event frames (see module docs).
pub struct ChainReader {
    first: PathBuf,
    ended: bool,
    reader: journal::Reader,
    index: u32,
    set_id: Option<[u8; 16]>,
    next_file: Option<String>,
    events: u64,
    /// The last segment announced a successor that does not exist (an interrupted rotation).
    announced_missing: bool,
    /// Segments continued after a crash and the unreadable tail bytes they left (DEC-16).
    pub recovered: u32,
    pub torn_bytes: u64,
    /// Seal state of the last segment read (meaningful after `next_frame` returned `None`).
    pub sealed: bool,
    pub complete: bool,
}

impl ChainReader {
    pub fn open(first: &Path) -> io::Result<Self> {
        Ok(Self {
            first: first.into(),
            ended: false,
            reader: journal::Reader::open_capture(first)?,
            index: 1,
            set_id: None,
            next_file: None,
            events: 0,
            announced_missing: false,
            recovered: 0,
            torn_bytes: 0,
            sealed: false,
            complete: false,
        })
    }
    /// Segments opened so far.
    pub fn segments(&self) -> u32 {
        self.index
    }
    /// Event frames returned so far.
    pub fn events(&self) -> u64 {
        self.events
    }
    /// The last segment ended in a torn (partially written) frame: frames were lost.
    pub fn tail_truncated(&self) -> bool {
        self.reader.tail_truncated
    }
    pub fn next_frame(&mut self) -> io::Result<Option<Frame>> {
        if self.ended {
            return Ok(None);
        }
        loop {
            match self.reader.next_frame()? {
                Some(f) if f.kind == kind::SEGMENT_NEXT => {
                    let n: SegmentNext = f.typed()?;
                    if n.index != self.index || self.set_id.is_some_and(|s| s != n.set_id) {
                        return Err(bad("SEGMENT_NEXT_MISMATCH"));
                    }
                    if n.next_file != file_name(&segment_path(&self.first, self.index + 1)) {
                        return Err(bad("SEGMENT_NEXT_NAME"));
                    }
                    self.set_id = Some(n.set_id);
                    self.next_file = Some(n.next_file);
                }
                Some(f) if f.kind == kind::SEGMENT_LINK => {
                    return Err(bad("SEGMENT_LINK_MISPLACED"))
                }
                Some(f) if f.kind == kind::SEGMENT_RECOVERY => {
                    return Err(bad("SEGMENT_RECOVERY_MISPLACED"))
                }
                Some(f) => {
                    if self.next_file.is_some() {
                        if f.kind == journal::MANIFEST {
                            // Manifest of a continued segment: chain-internal, verified by the link.
                            continue;
                        }
                        return Err(bad("SEGMENT_DATA_AFTER_NEXT"));
                    }
                    if f.kind != journal::MANIFEST {
                        self.events += 1;
                    }
                    return Ok(Some(f));
                }
                None => {
                    // DEC-16: a chain continued after a crash may have lost frames: never
                    // complete, however its last segment was sealed.
                    self.sealed = self.reader.sealed;
                    self.complete = self.reader.complete && self.recovered == 0;
                    let announced = self.next_file.take();
                    let path = segment_path(&self.first, self.index + 1);
                    let successor = match std::fs::symlink_metadata(&path) {
                        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                        Err(e) => return Err(e),
                        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                            return Err(bad("SEGMENT_NOT_REGULAR"))
                        }
                        Ok(_) => Some(()),
                    };
                    // Without an announced successor only a crashed segment may continue (by
                    // its recovery successor); everything else ends the chain here.
                    if successor.is_none() || (announced.is_none() && self.reader.sealed) {
                        if announced.is_some() {
                            self.check_no_orphans()
                                .map_err(|_| bad("SEGMENT_SUCCESSOR_MISSING"))?;
                            self.announced_missing = true;
                            self.sealed = false;
                            self.complete = false;
                        } else {
                            self.check_no_orphans()?;
                        }
                        self.ended = true;
                        return Ok(None);
                    }
                    let previous = SegmentRef {
                        file: file_name(&segment_path(&self.first, self.index)),
                        index: self.index,
                        frames: self.reader.frames(),
                        root_sha256: self.reader.root(),
                        events_through: self.events,
                    };
                    // Without an announced successor any unreadable or foreign file there is an
                    // orphan.
                    let opened = journal::Reader::open_capture(&path)
                        .and_then(|mut n| n.next_frame().map(|f| (n, f)));
                    let (next, first) = match opened {
                        Ok(read) => read,
                        Err(_) if announced.is_none() => return Err(bad("SEGMENT_ORPHAN")),
                        Err(e) => return Err(e),
                    };
                    match first {
                        Some(f) if f.kind == kind::SEGMENT_RECOVERY && !self.reader.sealed => {
                            let rec = f.typed::<SegmentRecovery>()?;
                            let crashed =
                                std::fs::metadata(segment_path(&self.first, self.index))?.len();
                            if rec.index != self.index + 1
                                || self.set_id.is_some_and(|s| s != rec.set_id)
                                || rec.previous != previous
                                || rec.torn_bytes != crashed.saturating_sub(self.reader.position())
                            {
                                return Err(bad("SEGMENT_RECOVERY_MISMATCH"));
                            }
                            self.set_id = Some(rec.set_id);
                            self.recovered += 1;
                            self.torn_bytes += rec.torn_bytes;
                        }
                        _ if announced.is_none() => return Err(bad("SEGMENT_ORPHAN")),
                        _ if !self.reader.sealed || !self.reader.complete => {
                            return Err(bad("SEGMENT_INTERMEDIATE_UNSEALED"))
                        }
                        Some(f) if f.kind == kind::SEGMENT_LINK => {
                            let link = f.typed::<SegmentLink>()?;
                            if link.index != self.index + 1
                                || Some(link.set_id) != self.set_id
                                || link.previous != previous
                            {
                                return Err(bad("SEGMENT_LINK_MISMATCH"));
                            }
                        }
                        _ => return Err(bad("SEGMENT_LINK_MISSING")),
                    }
                    self.reader = next;
                    self.index += 1;
                    self.sealed = false;
                    self.complete = false;
                }
            }
        }
    }
    /// A continuation file beyond the chain end means a lost link: fail closed.
    fn check_no_orphans(&self) -> io::Result<()> {
        let stem = self
            .first
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| bad("SEGMENT_NAME"))?;
        for entry in std::fs::read_dir(
            self.first
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(n) = name
                .strip_prefix(&format!("{stem}."))
                .and_then(|n| n.strip_suffix(".p0j"))
            {
                if n.len() == 6
                    && n.bytes().all(|b| b.is_ascii_digit())
                    && n.parse::<u32>().unwrap_or(0) > self.index
                {
                    return Err(bad("SEGMENT_ORPHAN"));
                }
            }
        }
        Ok(())
    }
}

/// Verification of a whole chain (for `p0-journal`, reports and tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainSummary {
    pub segments: u32,
    pub events: u64,
    pub sealed: bool,
    pub complete: bool,
    /// DEC-16: segments continued after a crash and the unreadable tail bytes they left.
    pub recovered: u32,
    pub torn_bytes: u64,
}

pub fn verify_chain(first: &Path) -> Result<ChainSummary, String> {
    let mut r = ChainReader::open(first).map_err(|e| format!("{}: {e}", first.display()))?;
    loop {
        match r.next_frame() {
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => return Err(format!("segment {}: {e}", r.segments())),
        }
    }
    Ok(ChainSummary {
        segments: r.segments(),
        events: r.events(),
        sealed: r.sealed,
        complete: r.complete,
        recovered: r.recovered,
        torn_bytes: r.torn_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::payload;
    use crate::market::observer::rows::MarketCoverage;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "p0_seg_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn rows(n: u64) -> Vec<(Provenance, Vec<u8>)> {
        (0..n)
            .map(|i| {
                let p = Provenance {
                    source_seq: i,
                    rx_ns: Some(i),
                    ..Default::default()
                };
                let row = MarketCoverage::SlotFailed {
                    slot: i,
                    error: "x".repeat(200),
                };
                (p, payload::encode(&row).unwrap())
            })
            .collect()
    }
    fn write(path: &Path, max: u64, rows: &[(Provenance, Vec<u8>)], seal: bool) -> u32 {
        let mut w = SegmentedWriter::create(path, max).unwrap();
        for chunk in rows.chunks(4) {
            w.append_batch(chunk.iter().map(|(p, b)| {
                (
                    MarketCoverage::KIND,
                    MarketCoverage::SCHEMA,
                    p,
                    b.as_slice(),
                )
            }))
            .unwrap();
            w.rotate_if_due().unwrap();
        }
        let n = w.segments();
        if seal {
            w.seal(true).unwrap();
        } else {
            std::mem::forget(w); // crash: last segment never sealed
        }
        n
    }
    fn replay(path: &Path) -> io::Result<Vec<(u64, Vec<u8>)>> {
        let mut r = ChainReader::open(path)?;
        let mut out = Vec::new();
        while let Some(f) = r.next_frame()? {
            if f.kind != journal::MANIFEST {
                out.push((f.provenance.source_seq, f.payload));
            }
        }
        Ok(out)
    }

    /// The segmented chain replays exactly the frames of an unsegmented journal (order, payload,
    /// provenance), files keep their names, and the summary counts segments and events.
    #[test]
    fn segmented_replay_equals_single_journal() {
        let d = dir("replay");
        let r = rows(40);
        let single = d.join("single.p0j");
        assert_eq!(write(&single, 0, &r, true), 1);
        let chained = d.join("market.p0j");
        let n = write(&chained, 4096, &r, true);
        assert!(n >= 3, "{n} segments");
        assert!(segment_path(&chained, 2).exists() && is_continuation(&segment_path(&chained, 2)));
        assert!(!is_continuation(&chained));
        assert_eq!(replay(&single).unwrap(), replay(&chained).unwrap());
        let s = verify_chain(&chained).unwrap();
        assert_eq!(
            (s.segments, s.events, s.sealed, s.complete),
            (n, 40, true, true)
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    /// N3: a zero-filled tail (power loss on ext4/XFS) of the crashed last segment is INCOMPLETE;
    /// zeros after the manifest of a sealed, linked intermediate segment fail closed.
    #[test]
    fn zero_filled_tail_of_the_last_segment_is_incomplete_but_not_of_an_intermediate() {
        use std::io::Write;
        let zeros = |p: &Path| {
            let mut f = std::fs::OpenOptions::new().append(true).open(p).unwrap();
            f.write_all(&[0; 4096]).unwrap();
        };
        let d = dir("zero_tail");
        let r = rows(40);
        let crashed = d.join("c/market.p0j");
        std::fs::create_dir_all(crashed.parent().unwrap()).unwrap();
        let n = write(&crashed, 4096, &r, false);
        assert!(n >= 3, "{n} segments");
        zeros(&segment_path(&crashed, n));
        let s = verify_chain(&crashed).unwrap();
        assert_eq!(
            (s.segments, s.events, s.sealed, s.complete),
            (n, 40, false, false)
        );
        let sealed = d.join("s/market.p0j");
        std::fs::create_dir_all(sealed.parent().unwrap()).unwrap();
        write(&sealed, 4096, &r, true);
        zeros(&segment_path(&sealed, 1));
        assert!(verify_chain(&sealed).unwrap_err().starts_with("segment 1"));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// PERSIST-05 (SK-04/SK-07): an fsync failure while rotating or sealing a market segment is
    /// an error of the lane and sticky for every later write; never a durable acknowledgement. A
    /// failed rotation leaves a chain that ends in a missing successor: INCOMPLETE, every
    /// admitted event still readable.
    #[test]
    fn segment_sync_failure_at_rotation_and_seal_is_sticky() {
        for at_seal in [false, true] {
            let d = dir("sync");
            let p = d.join("market.p0j");
            let mut w = SegmentedWriter::create(&p, 4096).unwrap();
            let r = rows(40);
            let batch = |w: &mut SegmentedWriter, rows: &[(Provenance, Vec<u8>)]| {
                w.append_batch(rows.iter().map(|(p, b)| {
                    (
                        MarketCoverage::KIND,
                        MarketCoverage::SCHEMA,
                        p,
                        b.as_slice(),
                    )
                }))
            };
            batch(&mut w, &r[..24]).unwrap();
            assert!(w.position() >= 4096, "rotation due");
            w.writer.test_fail_next_sync(io::ErrorKind::Other);
            if at_seal {
                assert!(w.seal(true).is_err());
            } else {
                assert!(w.rotate_if_due().is_err());
            }
            assert!(batch(&mut w, &r[24..28]).is_err(), "sticky");
            assert!(w.sync().is_err() && w.seal(true).is_err());
            assert_eq!(w.segments(), 1, "no successor after a failed rotation");
            if !at_seal {
                let chain = verify_chain(&p).unwrap();
                assert!(!(chain.sealed && chain.complete), "{chain:?}");
                assert_eq!(chain.events, 24);
            }
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    /// PERSIST-05 (SK-07): a successor whose link frame never reached disk (header only), an
    /// intermediate segment cut before its manifest and a successor that is a symlink are hard
    /// errors of the chain, never an INCOMPLETE pass.
    #[test]
    fn missing_link_unsealed_intermediate_and_symlinked_successor_fail_closed() {
        let d = dir("codes");
        let base = d.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let first = base.join("market.p0j");
        let n = write(&first, 4096, &rows(40), true);
        assert!(n >= 3, "{n}");
        let case = |name: &str, f: &dyn Fn(&Path)| {
            let c = d.join(name).join("market.p0j");
            std::fs::create_dir_all(c.parent().unwrap()).unwrap();
            for i in 1..=n {
                std::fs::copy(segment_path(&first, i), segment_path(&c, i)).unwrap();
            }
            f(&c);
            verify_chain(&c).unwrap_err()
        };
        let link_missing = case("link", &|p| {
            std::fs::remove_file(segment_path(p, 2)).unwrap();
            journal::Writer::create(&segment_path(p, 2)).unwrap();
        });
        assert!(
            link_missing.contains("SEGMENT_LINK_MISSING"),
            "{link_missing}"
        );
        let unsealed = case("unsealed", &|p| {
            let s1 = segment_path(p, 1);
            let mut r = journal::Reader::open(&s1).unwrap();
            let mut manifest = None;
            while let Some(f) = r.next_frame().unwrap() {
                if f.kind == journal::MANIFEST {
                    manifest = Some(f.offset);
                }
            }
            let file = std::fs::OpenOptions::new().write(true).open(&s1).unwrap();
            file.set_len(manifest.unwrap()).unwrap();
        });
        assert!(
            unsealed.contains("SEGMENT_INTERMEDIATE_UNSEALED"),
            "{unsealed}"
        );
        let symlink = case("symlink", &|p| {
            std::fs::remove_file(segment_path(p, 2)).unwrap();
            std::os::unix::fs::symlink(segment_path(&first, 2), segment_path(p, 2)).unwrap();
        });
        assert!(symlink.contains("SEGMENT_NOT_REGULAR"), "{symlink}");
        assert!(
            verify_chain(&first).is_ok(),
            "the source chain is untouched"
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    /// Crash in the last segment is INCOMPLETE; a missing, corrupt or swapped intermediate
    /// segment and an orphan beyond the chain end fail closed with their reason.
    #[test]
    fn crash_missing_corrupt_and_orphan_segments_fail_closed() {
        let d = dir("faults");
        let r = rows(40);
        let crashed = d.join("c/market.p0j");
        std::fs::create_dir_all(crashed.parent().unwrap()).unwrap();
        write(&crashed, 4096, &r, false);
        let s = verify_chain(&crashed).unwrap();
        assert!(!s.sealed && s.events == 40, "{s:?}");

        let base = d.join("base");
        std::fs::create_dir_all(&base).unwrap();
        let first = base.join("market.p0j");
        let n = write(&first, 4096, &r, true);
        let case = |name: &str, f: &dyn Fn(&Path)| {
            let c = d.join(name);
            std::fs::create_dir_all(&c).unwrap();
            for i in 1..=n {
                std::fs::copy(
                    segment_path(&first, i),
                    segment_path(&c.join("market.p0j"), i),
                )
                .unwrap();
            }
            f(&c.join("market.p0j"));
            verify_chain(&c.join("market.p0j")).unwrap_err()
        };
        assert!(
            case("missing", &|p| std::fs::remove_file(segment_path(p, 2))
                .unwrap())
            .contains("SEGMENT_SUCCESSOR_MISSING")
        );
        assert!(case("corrupt", &|p| {
            let s = segment_path(p, 2);
            let mut b = std::fs::read(&s).unwrap();
            let at = b.len() / 2;
            b[at] ^= 0xff;
            std::fs::write(&s, b).unwrap();
        })
        .starts_with("segment 2"));
        assert!(case("swapped", &|p| {
            // A foreign segment 2 (other set) in place of the real one.
            let other = d.join("other/market.p0j");
            std::fs::create_dir_all(other.parent().unwrap()).unwrap();
            write(&other, 4096, &rows(40), true);
            std::fs::copy(segment_path(&other, 2), segment_path(p, 2)).unwrap();
        })
        .contains("SEGMENT_LINK_MISMATCH"));
        assert!(case("orphan", &|p| {
            std::fs::copy(segment_path(p, 2), segment_path(p, n + 1)).unwrap();
        })
        .contains("SEGMENT_ORPHAN"));
        std::fs::remove_dir_all(d).unwrap();
    }

    fn append_rows(w: &mut SegmentedWriter, rows: &[(Provenance, Vec<u8>)]) {
        for chunk in rows.chunks(4) {
            w.append_batch(chunk.iter().map(|(p, b)| {
                (
                    MarketCoverage::KIND,
                    MarketCoverage::SCHEMA,
                    p,
                    b.as_slice(),
                )
            }))
            .unwrap();
        }
    }

    /// DEC-16 (SK-07 recovery): a run restarted after a crash continues its chain by a recovery
    /// successor; the crashed segment (readable frames plus a torn tail) is never changed, the
    /// chain replays every readable frame in order and reports the recovery and the torn bytes.
    /// A window boundary rotates on request.
    #[test]
    fn dec16_a_crashed_chain_continues_by_its_recovery_link_and_stays_unchanged() {
        for max in [4096, 0] {
            let d = dir("recover");
            let first = d.join("market.p0j");
            let before = rows(40);
            let n = write(&first, max, &before, false);
            let crashed = segment_path(&first, n);
            // A torn tail: the last frame's write never completed (row 39 is lost).
            let mut bytes = std::fs::read(&crashed).unwrap();
            bytes.truncate(bytes.len() - 5);
            std::fs::write(&crashed, &bytes).unwrap();
            let (mut w, how) = SegmentedWriter::recover(&first, max).unwrap();
            let Recovered::Crash {
                segment,
                torn_bytes,
            } = how
            else {
                panic!("{how:?}")
            };
            assert!(segment == n && torn_bytes > 0, "{how:?}");
            let after: Vec<_> = rows(50)[40..].to_vec();
            append_rows(&mut w, &after[..4]);
            w.rotate_now().unwrap();
            append_rows(&mut w, &after[4..]);
            w.seal(true).unwrap();
            assert_eq!(
                std::fs::read(&crashed).unwrap(),
                bytes,
                "the crashed segment is unchanged"
            );
            let s = verify_chain(&first).unwrap();
            assert_eq!(
                (
                    s.segments,
                    s.events,
                    s.sealed,
                    s.complete,
                    s.recovered,
                    s.torn_bytes
                ),
                (n + 2, 49, true, false, 1, torn_bytes),
                "{max}"
            );
            let replayed: Vec<u64> = replay(&first).unwrap().iter().map(|r| r.0).collect();
            assert_eq!(replayed, (0..39).chain(40..50).collect::<Vec<_>>());
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    /// DEC-16: nothing is recovered from a chain that ended in order, and a recovery link fails
    /// closed when the crashed segment it binds changed afterwards.
    #[test]
    fn dec16_recovery_refuses_a_sealed_chain_and_fails_closed_on_a_changed_crash() {
        let d = dir("recover_refused");
        let sealed = d.join("s/market.p0j");
        std::fs::create_dir_all(sealed.parent().unwrap()).unwrap();
        write(&sealed, 4096, &rows(20), true);
        assert_eq!(
            SegmentedWriter::recover(&sealed, 4096)
                .err()
                .unwrap()
                .to_string(),
            "SEGMENT_CHAIN_SEALED"
        );
        let first = d.join("c/market.p0j");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        let n = write(&first, 4096, &rows(20), false);
        let (mut w, _) = SegmentedWriter::recover(&first, 4096).unwrap();
        append_rows(&mut w, &rows(24)[20..]);
        w.seal(true).unwrap();
        assert_eq!(verify_chain(&first).unwrap().recovered, 1);
        let crashed = segment_path(&first, n);
        let original = std::fs::read(&crashed).unwrap();
        // More bytes behind the crash than the link binds.
        let mut grown = original.clone();
        grown.push(0);
        std::fs::write(&crashed, &grown).unwrap();
        assert!(verify_chain(&first)
            .unwrap_err()
            .contains("SEGMENT_RECOVERY_MISMATCH"));
        // A changed readable frame.
        let mut flipped = original.clone();
        let at = flipped.len() - 3;
        flipped[at] ^= 0xff;
        std::fs::write(&crashed, &flipped).unwrap();
        assert!(verify_chain(&first).is_err());
        std::fs::write(&crashed, &original).unwrap();
        assert!(verify_chain(&first).is_ok());
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-16: the recovery link binds exactly the crashed segment's readable frames and their
    /// chain head: another crashed segment of the same size and tail fails closed.
    #[test]
    fn dec16_a_recovery_link_binds_its_crashed_segment_exactly() {
        let d = dir("recover_binding");
        let crash = |name: &str, rows: &[(Provenance, Vec<u8>)]| {
            let first = d.join(name).join("market.p0j");
            std::fs::create_dir_all(first.parent().unwrap()).unwrap();
            write(&first, 0, rows, false);
            let bytes = std::fs::read(&first).unwrap();
            std::fs::write(&first, &bytes[..bytes.len() - 5]).unwrap();
            first
        };
        let a = crash("a", &rows(8));
        // Same frame count and sizes, other payloads (other provenance sequence numbers).
        let other: Vec<_> = rows(16)[8..].to_vec();
        let b = crash("b", &other);
        assert_eq!(
            std::fs::metadata(&a).unwrap().len(),
            std::fs::metadata(&b).unwrap().len()
        );
        let (mut w, _) = SegmentedWriter::recover(&a, 0).unwrap();
        append_rows(&mut w, &rows(2));
        w.seal(true).unwrap();
        assert!(verify_chain(&a).is_ok());
        std::fs::copy(&b, &a).unwrap();
        assert!(verify_chain(&a)
            .unwrap_err()
            .contains("SEGMENT_RECOVERY_MISMATCH"));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-16: a chain that ended in order is never continued — not even by a recovery link
    /// that binds its sealed last segment exactly: without an announced successor it is an
    /// orphan, in place of the announced successor's link it is no link.
    #[test]
    fn dec16_no_recovery_link_continues_a_sealed_segment() {
        for announced in [false, true] {
            let d = dir("recover_sealed");
            let first = d.join("market.p0j");
            let mut w = Writer::create(&first).unwrap();
            w.append_typed(
                Provenance::default(),
                &MarketCoverage::SlotFailed {
                    slot: 1,
                    error: "x".into(),
                },
            )
            .unwrap();
            if announced {
                w.append_typed(
                    Provenance::default(),
                    &SegmentNext {
                        set_id: [3; 16],
                        index: 1,
                        next_file: "market.000002.p0j".into(),
                    },
                )
                .unwrap();
            }
            w.seal(true).unwrap();
            let r = journal::Reader::open(&first).map(|mut r| {
                while r.next_frame().unwrap().is_some() {}
                r
            });
            let r = r.unwrap();
            let previous = SegmentRef {
                file: "market.p0j".into(),
                index: 1,
                frames: r.frames(),
                root_sha256: r.root(),
                events_through: 1,
            };
            let mut forged = Writer::create(&segment_path(&first, 2)).unwrap();
            forged
                .append_typed(
                    Provenance::default(),
                    &SegmentRecovery {
                        set_id: [3; 16],
                        index: 2,
                        previous,
                        torn_bytes: 0,
                    },
                )
                .unwrap();
            forged.seal(true).unwrap();
            let want = if announced {
                "SEGMENT_LINK_MISSING"
            } else {
                "SEGMENT_ORPHAN"
            };
            let e = verify_chain(&first).unwrap_err();
            assert!(e.contains(want), "{announced}: {e}");
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    /// DEC-16: a recovery link names its chain and position: another set id or index fails
    /// closed, and a recovery frame anywhere but a successor's first frame is refused.
    #[test]
    fn dec16_a_recovery_link_names_its_set_and_index_and_sits_only_first() {
        let d = dir("recover_forged");
        let first = d.join("market.p0j");
        let n = write(&first, 4096, &rows(20), false);
        assert!(n >= 2, "the chain rotated: its set id is known");
        let crashed = segment_path(&first, n);
        let bytes = std::fs::read(&crashed).unwrap();
        std::fs::write(&crashed, &bytes[..bytes.len() - 5]).unwrap();
        let mut r = ChainReader::open(&first).unwrap();
        while r.next_frame().unwrap().is_some() {}
        let previous = SegmentRef {
            file: file_name(&crashed),
            index: n,
            frames: r.reader.frames(),
            root_sha256: r.reader.root(),
            events_through: r.events(),
        };
        let torn_bytes = std::fs::metadata(&crashed).unwrap().len() - r.reader.position();
        let forge = |set_id: [u8; 16], index: u32| {
            let path = segment_path(&first, n + 1);
            let _ = std::fs::remove_file(&path);
            let mut w = Writer::create(&path).unwrap();
            w.append_typed(
                Provenance::default(),
                &SegmentRecovery {
                    set_id,
                    index,
                    previous: previous.clone(),
                    torn_bytes,
                },
            )
            .unwrap();
            w.seal(true).unwrap();
            verify_chain(&first)
        };
        let real = r.set_id.unwrap();
        assert!(forge(real, n + 1).is_ok());
        assert!(forge([9; 16], n + 1)
            .unwrap_err()
            .contains("SEGMENT_RECOVERY_MISMATCH"));
        assert!(forge(real, n + 5)
            .unwrap_err()
            .contains("SEGMENT_RECOVERY_MISMATCH"));
        // A recovery frame inside a segment is no event.
        let lone = d.join("lone.p0j");
        let mut w = Writer::create(&lone).unwrap();
        w.append_typed(
            Provenance::default(),
            &SegmentRecovery {
                set_id: real,
                index: 2,
                previous,
                torn_bytes: 0,
            },
        )
        .unwrap();
        w.seal(true).unwrap();
        assert!(verify_chain(&lone)
            .unwrap_err()
            .contains("SEGMENT_RECOVERY_MISPLACED"));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-16: a chain continued after a crash is never exported as the journal (a torn crash
    /// lost frames; a clean one may have lost what it never wrote), and its last segment's own
    /// manifest says incomplete.
    #[test]
    fn dec16_a_recovered_chain_is_never_exported_and_seals_incomplete() {
        for torn in [true, false] {
            let d = dir("recover_export");
            let first = d.join("market.p0j");
            let n = write(&first, 0, &rows(12), false);
            if torn {
                let crashed = segment_path(&first, n);
                let bytes = std::fs::read(&crashed).unwrap();
                std::fs::write(&crashed, &bytes[..bytes.len() - 5]).unwrap();
            }
            let (mut w, _) = SegmentedWriter::recover(&first, 0).unwrap();
            append_rows(&mut w, &rows(2));
            let sealed = w.seal(true).unwrap();
            let mut last = journal::Reader::open(&sealed.last_file).unwrap();
            while last.next_frame().unwrap().is_some() {}
            assert!(last.sealed && !last.complete, "{torn}");
            let out = d.join("view.jsonl");
            let e = journal::export_views(&first, &[(None, out.as_path())], false).unwrap_err();
            let want = if torn {
                "EXPORT_TORN_TAIL"
            } else {
                "EXPORT_RECOVERED_CHAIN"
            };
            assert_eq!(e.to_string(), want, "{torn}");
            assert!(!out.exists());
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    /// DEC-16: a successor is linked into place only complete with its first frame; a crash
    /// while it is written leaves only its temporary file, which never blocks the recovery.
    #[test]
    fn dec16_a_temporary_successor_never_blocks_the_recovery() {
        let d = dir("recover_temp");
        let first = d.join("market.p0j");
        write(&first, 0, &rows(4), false);
        let stale = temp_path(&segment_path(&first, 2));
        std::fs::write(&stale, b"a header and nothing more").unwrap();
        assert!(
            verify_chain(&first).is_ok(),
            "a temporary file is no segment"
        );
        let (mut w, how) = SegmentedWriter::recover(&first, 0).unwrap();
        assert!(
            matches!(how, Recovered::Crash { segment: 1, .. }),
            "{how:?}"
        );
        assert!(!stale.exists());
        append_rows(&mut w, &rows(2));
        w.seal(true).unwrap();
        let s = verify_chain(&first).unwrap();
        assert_eq!(
            (s.segments, s.events, s.sealed, s.complete, s.recovered),
            (2, 6, true, false, 1)
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    /// DEC-16: a rotation interrupted after its seal is finished with the announced successor
    /// (its normal link, no recovery); interrupted before the seal it is a crash.
    #[test]
    fn dec16_an_interrupted_rotation_is_finished_with_its_announced_successor() {
        for sealed in [true, false] {
            let d = dir("rotation");
            let p = d.join("market.p0j");
            let mut w = Writer::create(&p).unwrap();
            w.append_typed(
                Provenance::default(),
                &MarketCoverage::SlotFailed {
                    slot: 1,
                    error: "x".into(),
                },
            )
            .unwrap();
            w.append_typed(
                Provenance::default(),
                &SegmentNext {
                    set_id: [1; 16],
                    index: 1,
                    next_file: "market.000002.p0j".into(),
                },
            )
            .unwrap();
            if sealed {
                w.seal(true).unwrap();
            } else {
                w.sync().unwrap();
            }
            drop(w);
            let (mut w, how) = SegmentedWriter::recover(&p, 0).unwrap();
            let expected = if sealed {
                Recovered::Rotation { segment: 1 }
            } else {
                Recovered::Crash {
                    segment: 1,
                    torn_bytes: 0,
                }
            };
            assert_eq!(how, expected);
            append_rows(&mut w, &rows(4));
            w.seal(true).unwrap();
            let s = verify_chain(&p).unwrap();
            assert_eq!(
                (s.segments, s.events, s.complete, s.recovered),
                (2, 5, sealed, u32::from(!sealed))
            );
            std::fs::remove_dir_all(d).unwrap();
        }
    }
}

#[cfg(test)]
mod torn_tail_tests {
    use super::*;
    use crate::market::observer::rows::MarketCoverage;
    fn temp() -> PathBuf {
        // `Pubkey::new_unique` restarts in every process: bind the name to this process and time
        // so that directories left by an aborted run never collide.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let d = std::env::temp_dir().join(format!(
            "p0_torn_{}_{nanos}_{}",
            std::process::id(),
            solana_sdk::pubkey::Pubkey::new_unique()
        ));
        std::fs::create_dir(&d).unwrap();
        d
    }
    #[test]
    fn capture_torn_tail_is_incomplete_but_wallet_and_resume_stay_strict() {
        let d = temp();
        let p = d.join("market.p0j");
        let mut w = Writer::create(&p).unwrap();
        w.append_typed(
            Provenance::default(),
            &MarketCoverage::SlotFailed {
                slot: 1,
                error: "x".into(),
            },
        )
        .unwrap();
        let prefix = w.position();
        w.append_typed(
            Provenance::default(),
            &MarketCoverage::SlotFailed {
                slot: 2,
                error: "y".into(),
            },
        )
        .unwrap();
        w.sync().unwrap();
        drop(w);
        let full = std::fs::read(&p).unwrap();
        for len in prefix + 1..full.len() as u64 {
            std::fs::write(&p, &full[..len as usize]).unwrap();
            let before = std::fs::read(&p).unwrap();
            let s = verify_chain(&p).unwrap();
            assert_eq!(s.events, 1);
            assert!(!s.complete && !s.sealed);
            let mut strict = journal::Reader::open(&p).unwrap();
            assert!(strict.next_frame().unwrap().is_some());
            assert!(strict.next_frame().is_err());
            assert!(Writer::resume(&p).is_err());
            assert_eq!(before, std::fs::read(&p).unwrap());
        }
        let mut corrupt = full.clone();
        corrupt[prefix as usize + 10] ^= 1;
        std::fs::write(&p, corrupt).unwrap();
        assert!(verify_chain(&p).unwrap_err().contains("FRAME_CRC"));
        std::fs::remove_dir_all(d).unwrap();
    }
    /// Two frames, synced, unsealed: `(path, journal bytes, offset of the second frame)`.
    fn two_frames(d: &Path) -> (PathBuf, Vec<u8>, usize) {
        let p = d.join("market.p0j");
        let mut w = Writer::create(&p).unwrap();
        for slot in [1, 2] {
            if slot == 2 {
                let prefix = w.position() as usize;
                w.append_typed(
                    Provenance::default(),
                    &MarketCoverage::SlotFailed {
                        slot,
                        error: "y".into(),
                    },
                )
                .unwrap();
                w.sync().unwrap();
                drop(w);
                return (p.clone(), std::fs::read(&p).unwrap(), prefix);
            }
            w.append_typed(
                Provenance::default(),
                &MarketCoverage::SlotFailed {
                    slot,
                    error: "x".into(),
                },
            )
            .unwrap();
        }
        unreachable!()
    }
    /// N3: after power loss the file may be extended with zeros instead of truncated. Only a
    /// tail of zeros up to EOF at a frame boundary of an unsealed capture is INCOMPLETE; the
    /// strict readers (wallet journal, `Writer::resume`) refuse it and no byte is changed.
    #[test]
    fn capture_zero_filled_tail_is_incomplete_but_strict_readers_refuse() {
        let d = temp();
        let (p, full, _) = two_frames(&d);
        for zeros in [1usize, 3, 4, 5, 4096, 70_000] {
            std::fs::write(&p, [full.clone(), vec![0; zeros]].concat()).unwrap();
            let before = std::fs::read(&p).unwrap();
            let s = verify_chain(&p).unwrap_or_else(|e| panic!("{zeros} zeros: {e}"));
            assert_eq!(
                (s.events, s.sealed, s.complete),
                (2, false, false),
                "{zeros}"
            );
            let mut strict = journal::Reader::open(&p).unwrap();
            assert!(strict.next_frame().unwrap().is_some());
            assert!(strict.next_frame().unwrap().is_some());
            assert!(strict.next_frame().is_err(), "{zeros} zeros");
            assert!(Writer::resume(&p).is_err());
            assert_eq!(before, std::fs::read(&p).unwrap());
        }
        // A foreign byte after the zeros is not a torn tail.
        std::fs::write(&p, [full.clone(), vec![0; 4096], vec![1]].concat()).unwrap();
        assert!(verify_chain(&p).unwrap_err().contains("FRAME_LENGTH"));
        // Zeros after the manifest of a sealed journal stay an error, not INCOMPLETE.
        std::fs::write(&p, &full).unwrap();
        Writer::resume(&p).unwrap().seal(true).unwrap();
        let sealed = std::fs::read(&p).unwrap();
        std::fs::write(&p, [sealed, vec![0; 4096]].concat()).unwrap();
        assert!(verify_chain(&p).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn zero_bytes_inside_the_stream_stay_hard_errors() {
        let d = temp();
        let (p, full, prefix) = two_frames(&d);
        // 16 bytes inside the second frame's SHA-256 trailer (not already zero).
        let region = full.len() - 26..full.len() - 10;
        assert!(full[region.clone()].iter().any(|b| *b != 0));
        let mut inside = full.clone();
        inside[region].fill(0);
        std::fs::write(&p, &inside).unwrap();
        let e = verify_chain(&p).unwrap_err();
        assert!(e.contains("FRAME_CRC") || e.contains("FRAME_SHA256"), "{e}");
        // A zeroed length in front of a written frame body is no zero-filled tail.
        let mut length = full.clone();
        length[prefix..prefix + 4].fill(0);
        std::fs::write(&p, &length).unwrap();
        assert!(verify_chain(&p).unwrap_err().contains("FRAME_LENGTH"));
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn interrupted_rotate_missing_final_successor_is_incomplete_and_orphan_holes_fail() {
        for sealed in [false, true] {
            let d = temp();
            let p = d.join("market.p0j");
            let mut w = Writer::create(&p).unwrap();
            w.append_typed(
                Provenance::default(),
                &MarketCoverage::SlotFailed {
                    slot: 1,
                    error: "x".into(),
                },
            )
            .unwrap();
            w.append_typed(
                Provenance::default(),
                &SegmentNext {
                    set_id: [1; 16],
                    index: 1,
                    next_file: "market.000002.p0j".into(),
                },
            )
            .unwrap();
            if sealed {
                w.seal(true).unwrap();
            } else {
                w.sync().unwrap();
            }
            drop(w);
            let mut r = ChainReader::open(&p).unwrap();
            while r.next_frame().unwrap().is_some() {}
            assert!(!r.complete && !r.sealed);
            assert!(r.next_frame().unwrap().is_none());
            assert!(!r.complete && !r.sealed);
            std::fs::write(segment_path(&p, 9), b"orphan").unwrap();
            assert!(verify_chain(&p)
                .unwrap_err()
                .contains("SEGMENT_SUCCESSOR_MISSING"));
            std::fs::remove_dir_all(d).unwrap();
        }
    }
}
