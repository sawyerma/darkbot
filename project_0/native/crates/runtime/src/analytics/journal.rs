//! PERSIST-01: one binary append-only contract for runtime and durable financial events.
//! LE integers, bounded frames, typed value payload, CRC32 + chained SHA256.
//! JSON is supported only as an external value model / explicit offline export.
use serde_json::{Map, Number, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const MAGIC: &[u8; 8] = b"P0JRNL01";
pub const VERSION: u16 = 1;
pub const HEADER_LEN: u64 = 60;
pub const MAX_FRAME: usize = 8 * 1024 * 1024;
pub const MANIFEST: u16 = u16::MAX;
const FIXED: usize = 120;

/// Event kinds of the one journal contract. Every writer takes its kind from here, so a kind
/// is never reused for a different payload. Unknown kinds are rejected by the typed readers.
pub mod kind {
    pub const OWNER_RECORD: u16 = 100;
    pub const OWNER_OPPORTUNITY: u16 = 101;
    pub const OWNER_RECONCILE: u16 = 102;
    pub const OWNER_SUMMARY: u16 = 103;
    /// Owner diagnostic line `{line}`; the writer thread also prints it to stderr afterwards.
    pub const OWNER_DIAGNOSTIC: u16 = 104;
    /// FE-04 single-flight automaton: one full state version per durable transition.
    pub const FE04_TRANSITION: u16 = 200;
    /// FE-04 import of a pre-binary JSON wallet journal; the JSON file itself stays unchanged.
    pub const FE04_LEGACY_IMPORT: u16 = 201;
    /// Market capture rows, exactly the legacy JSONL row values (`transactions.jsonl`,
    /// `liquidations.jsonl`, `coverage.jsonl` are renders of these kinds).
    pub const MARKET_TRANSACTION: u16 = 300;
    pub const MARKET_LIQUIDATION: u16 = 301;
    pub const MARKET_COVERAGE: u16 = 302;
    /// C0-E1 shadow evaluation `{previous_target_offset, record}`; the per-target chain of
    /// in-window frames is the bounded-memory index of the analytics worker.
    pub const ECON_SHADOW_EVALUATION: u16 = 400;
    /// C0-E1 snapshot `{index, finalizing, report}`; the last frame per index is `hour_0N.json`.
    pub const ECON_SNAPSHOT: u16 = 410;
    /// C0-E1 final report (`economics_final.json`).
    pub const ECON_FINAL: u16 = 411;
    /// One row of `opportunity_economics.jsonl`.
    pub const ECON_OPPORTUNITY: u16 = 412;
    /// One row of `canonical_market_shadow.jsonl`.
    pub const ECON_CANONICAL: u16 = 413;
    /// C0-E1 phase boundary `{phase, at_ns, unix_ns, details}` (supervisor, market, analytics).
    pub const MEASUREMENT_PHASE: u16 = 500;
    /// MT0 measurement snapshot (cumulative counters), off-owner reporter thread.
    pub const MT0_SNAPSHOT: u16 = 600;
    /// FE-01 read-only observation: effective configuration, each saved RPC read
    /// `{method, params, result|error}` (fsynced before it is decoded), and the result.
    pub const FE01_CONFIG: u16 = 700;
    pub const FE01_READ: u16 = 701;
    pub const FE01_RESULT: u16 = 702;
    /// C0 account census rows (`c0_census.py` via `p0-journal import`): program accounts and
    /// group indexer accounts of one context slot.
    pub const C0_CENSUS_ACCOUNT: u16 = 900;
    pub const C0_CENSUS_GROUP_ACCOUNT: u16 = 901;
    /// Kinds an explicit offline import may write; runtime and financial kinds never.
    pub const IMPORTABLE: &[u16] = &[C0_CENSUS_ACCOUNT, C0_CENSUS_GROUP_ACCOUNT];
    /// Owner events that could not reach the primary journal (recovery journal only).
    pub const OWNER_RECOVERY: u16 = 1000;
    pub const ALL: &[u16] = &[
        OWNER_RECORD,
        OWNER_OPPORTUNITY,
        OWNER_RECONCILE,
        OWNER_SUMMARY,
        OWNER_DIAGNOSTIC,
        FE04_TRANSITION,
        FE04_LEGACY_IMPORT,
        MARKET_TRANSACTION,
        MARKET_LIQUIDATION,
        MARKET_COVERAGE,
        ECON_SHADOW_EVALUATION,
        ECON_SNAPSHOT,
        ECON_FINAL,
        ECON_OPPORTUNITY,
        ECON_CANONICAL,
        MEASUREMENT_PHASE,
        MT0_SNAPSHOT,
        FE01_CONFIG,
        FE01_READ,
        FE01_RESULT,
        C0_CENSUS_ACCOUNT,
        C0_CENSUS_GROUP_ACCOUNT,
        OWNER_RECOVERY,
        super::MANIFEST,
    ];
}
/// Provenance sources of the one journal contract.
pub mod source {
    pub const OWNER: u32 = 1;
    pub const FE04: u32 = 2;
    pub const MARKET: u32 = 3;
    pub const MT0: u32 = 4;
    pub const FE01: u32 = 5;
    pub const ECON: u32 = 6;
    pub const MEASUREMENT: u32 = 7;
    pub const C0_CENSUS: u32 = 8;
}
fn bad(code: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, code)
}
/// Directory to fsync after creating `path`; a bare file name lives in the working directory.
fn parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}
pub fn binary_path(path: &Path) -> PathBuf {
    path.with_extension("p0j")
}
/// 16 fresh bytes from the operating system. A journal is never created without them.
fn random16() -> io::Result<[u8; 16]> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn process_once(cell: &'static std::sync::OnceLock<[u8; 16]>) -> io::Result<[u8; 16]> {
    if let Some(v) = cell.get() {
        return Ok(*v);
    }
    let fresh = random16()?;
    Ok(*cell.get_or_init(|| fresh))
}
/// Header identities: run = this process, session = this writer, boot = the operating
/// system boot (Linux `boot_id`; elsewhere unknown, so a process-scoped random value).
fn identities() -> io::Result<[[u8; 16]; 3]> {
    static RUN: std::sync::OnceLock<[u8; 16]> = std::sync::OnceLock::new();
    static BOOT: std::sync::OnceLock<[u8; 16]> = std::sync::OnceLock::new();
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .and_then(|text| {
            let hex: String = text.trim().chars().filter(|c| *c != '-').collect();
            (hex.len() == 32)
                .then(|| u128::from_str_radix(&hex, 16).ok())
                .flatten()
        })
        .map(u128::to_be_bytes);
    let boot = match boot {
        Some(id) => id,
        None => process_once(&BOOT)?,
    };
    Ok([process_once(&RUN)?, random16()?, boot])
}
fn sha(bytes: &[u8]) -> [u8; 32] {
    solana_sdk::hash::hash(bytes).to_bytes()
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// IEEE CRC32 (reflected, polynomial 0xedb88320), table driven; same values as bitwise.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb88320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};
fn crc(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for b in bytes {
        crc = CRC_TABLE[((crc ^ *b as u32) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

/// UNKNOWN provenance is explicit (None); it is never invented from journal order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Provenance {
    pub source: u32,
    pub source_seq: u64,
    pub slot: Option<u64>,
    pub fork: Option<[u8; 32]>,
    pub rx_ns: Option<u64>,
    pub unix_ns: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct Frame {
    pub kind: u16,
    pub schema: u16,
    pub journal_seq: u64,
    pub provenance: Provenance,
    pub value: Value,
    pub offset: u64,
    pub bytes: u64,
    pub hash: [u8; 32],
}
/// Position of a frame just appended (the payload stays with the caller, it is not copied).
#[derive(Clone, Debug)]
pub struct Appended {
    pub journal_seq: u64,
    pub offset: u64,
    pub bytes: u64,
    pub hash: [u8; 32],
}
#[derive(Clone, Debug)]
pub struct Durable {
    pub journal_seq: u64,
    pub hash: [u8; 32],
}

/// Numbers are tagged LE i128/u128, or exact decimal text for nonintegral / wider
/// external JSON numbers. No f64 coercion, including values outside IEEE precision.
pub fn encode(value: &Value) -> io::Result<Vec<u8>> {
    fn sized(out: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
        let n = u32::try_from(bytes.len()).map_err(|_| bad("PAYLOAD_LIMIT"))?;
        out.extend(n.to_le_bytes());
        out.extend(bytes);
        Ok(())
    }
    fn node(v: &Value, o: &mut Vec<u8>, depth: usize) -> io::Result<()> {
        if depth > 64 {
            return Err(bad("PAYLOAD_DEPTH"));
        }
        match v {
            Value::Null => o.push(0),
            Value::Bool(false) => o.push(1),
            Value::Bool(true) => o.push(2),
            Value::Number(n) => {
                let text = n.to_string();
                if let Ok(n) = text.parse::<i128>() {
                    o.push(3);
                    o.extend(n.to_le_bytes());
                } else if let Ok(n) = text.parse::<u128>() {
                    o.push(4);
                    o.extend(n.to_le_bytes());
                } else {
                    o.push(5);
                    sized(o, text.as_bytes())?;
                }
            }
            Value::String(s) => {
                o.push(6);
                sized(o, s.as_bytes())?;
            }
            Value::Array(a) => {
                o.push(7);
                o.extend(
                    u32::try_from(a.len())
                        .map_err(|_| bad("PAYLOAD_LIMIT"))?
                        .to_le_bytes(),
                );
                for v in a {
                    node(v, o, depth + 1)?;
                }
            }
            Value::Object(m) => {
                o.push(8);
                o.extend(
                    u32::try_from(m.len())
                        .map_err(|_| bad("PAYLOAD_LIMIT"))?
                        .to_le_bytes(),
                );
                // Canonical key order whatever the map implementation; decode requires it.
                let mut entries: Vec<_> = m.iter().collect();
                entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
                for (k, v) in entries {
                    sized(o, k.as_bytes())?;
                    node(v, o, depth + 1)?;
                }
            }
        }
        if o.len() > MAX_FRAME - FIXED - 36 {
            return Err(bad("PAYLOAD_LIMIT"));
        }
        Ok(())
    }
    let mut out = Vec::new();
    node(value, &mut out, 0)?;
    Ok(out)
}
pub fn decode(bytes: &[u8]) -> io::Result<Value> {
    struct Cursor<'a> {
        b: &'a [u8],
        p: usize,
    }
    impl<'a> Cursor<'a> {
        fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
            let end = self
                .p
                .checked_add(n)
                .filter(|e| *e <= self.b.len())
                .ok_or_else(|| bad("PAYLOAD_TRUNCATED"))?;
            let b = &self.b[self.p..end];
            self.p = end;
            Ok(b)
        }
        fn size(&mut self) -> io::Result<usize> {
            Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
        }
        fn text(&mut self) -> io::Result<String> {
            let n = self.size()?;
            String::from_utf8(self.take(n)?.to_vec()).map_err(|_| bad("PAYLOAD_UTF8"))
        }
        fn value(&mut self, depth: usize) -> io::Result<Value> {
            if depth > 64 {
                return Err(bad("PAYLOAD_DEPTH"));
            }
            Ok(match self.take(1)?[0] {
                0 => Value::Null,
                1 => Value::Bool(false),
                2 => Value::Bool(true),
                3 => Value::Number(
                    i128::from_le_bytes(self.take(16)?.try_into().unwrap())
                        .to_string()
                        .parse::<Number>()
                        .map_err(|_| bad("PAYLOAD_INTEGER"))?,
                ),
                4 => Value::Number(
                    u128::from_le_bytes(self.take(16)?.try_into().unwrap())
                        .to_string()
                        .parse::<Number>()
                        .map_err(|_| bad("PAYLOAD_INTEGER"))?,
                ),
                5 => Value::Number(
                    self.text()?
                        .parse::<Number>()
                        .map_err(|_| bad("PAYLOAD_NUMBER"))?,
                ),
                6 => Value::String(self.text()?),
                7 => {
                    let n = self.size()?;
                    if n > self.b.len().saturating_sub(self.p) {
                        return Err(bad("PAYLOAD_COUNT"));
                    }
                    let mut a = Vec::new();
                    for _ in 0..n {
                        a.push(self.value(depth + 1)?);
                    }
                    Value::Array(a)
                }
                8 => {
                    let n = self.size()?;
                    if n > self.b.len().saturating_sub(self.p) / 5 {
                        return Err(bad("PAYLOAD_COUNT"));
                    }
                    let mut m = Map::new();
                    let mut previous = None;
                    for _ in 0..n {
                        let k = self.text()?;
                        if previous.as_ref().is_some_and(|p: &String| p >= &k) {
                            return Err(bad("PAYLOAD_MAP_ORDER"));
                        }
                        previous = Some(k.clone());
                        m.insert(k, self.value(depth + 1)?);
                    }
                    Value::Object(m)
                }
                _ => return Err(bad("PAYLOAD_TAG")),
            })
        }
    }
    if bytes.len() > MAX_FRAME {
        return Err(bad("PAYLOAD_LIMIT"));
    }
    let mut c = Cursor { b: bytes, p: 0 };
    let v = c.value(0)?;
    if c.p != bytes.len() {
        return Err(bad("PAYLOAD_TRAILING"));
    }
    Ok(v)
}

pub struct Writer {
    file: File,
    pub path: PathBuf,
    next: u64,
    last: [u8; 32],
    position: u64,
    sealed: bool,
    poisoned: bool,
}
impl Writer {
    pub fn create(path: &Path) -> io::Result<Self> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(path)?;
        let mut header = MAGIC.to_vec();
        header.extend(VERSION.to_le_bytes());
        header.extend((HEADER_LEN as u16).to_le_bytes());
        // Independent run/session/boot identities. Source/fork UNKNOWN remain explicit.
        for id in identities()? {
            header.extend(id);
        }
        file.write_all(&header)?;
        file.sync_all()?;
        File::open(parent(path))?.sync_all()?;
        Ok(Self {
            file,
            path: path.into(),
            next: 0,
            last: sha(&header),
            position: HEADER_LEN,
            sealed: false,
            poisoned: false,
        })
    }
    pub fn resume(path: &Path) -> io::Result<Self> {
        let mut reader = Reader::open(path)?;
        while reader.next_frame()?.is_some() {}
        if reader.sealed {
            return Err(bad("JOURNAL_ALREADY_SEALED"));
        }
        let file = OpenOptions::new().read(true).append(true).open(path)?;
        Ok(Self {
            file,
            path: path.into(),
            next: reader.next,
            last: reader.last,
            position: reader.position,
            sealed: false,
            poisoned: false,
        })
    }
    #[cfg(test)]
    pub(crate) fn test_read_only(&mut self) {
        self.file = File::open(&self.path).unwrap();
    }
    pub fn next_seq(&self) -> u64 {
        self.next
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn append(
        &mut self,
        kind: u16,
        schema: u16,
        p: Provenance,
        value: &Value,
    ) -> io::Result<Appended> {
        if self.poisoned || self.sealed {
            return Err(bad("JOURNAL_BLOCKED"));
        }
        if kind == 0 || schema != 1 {
            return Err(bad("EVENT_SCHEMA_UNSUPPORTED"));
        }
        let payload = encode(value)?;
        // Length prefix first (filled in below), then the body; one buffer, no second copy.
        let mut wire = Vec::with_capacity(4 + FIXED + payload.len() + 36);
        wire.extend([0u8; 4]);
        wire.extend(kind.to_le_bytes());
        wire.extend(schema.to_le_bytes());
        wire.extend(self.next.to_le_bytes());
        wire.extend(p.source_seq.to_le_bytes());
        wire.extend(p.source.to_le_bytes());
        let flags = (p.slot.is_some() as u32)
            | ((p.fork.is_some() as u32) << 1)
            | ((p.rx_ns.is_some() as u32) << 2)
            | ((p.unix_ns.is_some() as u32) << 3);
        wire.extend(flags.to_le_bytes());
        wire.extend(p.slot.unwrap_or(0).to_le_bytes());
        wire.extend(p.fork.unwrap_or([0; 32]));
        wire.extend(p.rx_ns.unwrap_or(0).to_le_bytes());
        wire.extend(p.unix_ns.unwrap_or(0).to_le_bytes());
        wire.extend(self.last);
        wire.extend((payload.len() as u32).to_le_bytes());
        wire.extend(payload);
        let checksum = crc(&wire[4..]);
        wire.extend(checksum.to_le_bytes());
        let digest = sha(&wire[4..]);
        wire.extend(digest);
        let body = wire.len() - 4;
        if body > MAX_FRAME {
            return Err(bad("FRAME_LIMIT"));
        }
        wire[..4].copy_from_slice(&(body as u32).to_le_bytes());
        if let Err(e) = self.file.write_all(&wire) {
            self.poisoned = true;
            return Err(e);
        }
        let f = Appended {
            journal_seq: self.next,
            offset: self.position,
            bytes: wire.len() as u64,
            hash: digest,
        };
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| bad("SEQUENCE_EXHAUSTED"))?;
        self.position += wire.len() as u64;
        self.last = digest;
        Ok(f)
    }
    pub fn append_value(&mut self, kind: u16, value: &Value) -> io::Result<Appended> {
        self.append(
            kind,
            1,
            Provenance {
                source_seq: self.next,
                ..Default::default()
            },
            value,
        )
    }
    pub fn sync(&mut self) -> io::Result<Durable> {
        if self.poisoned {
            return Err(bad("JOURNAL_BLOCKED"));
        }
        if let Err(e) = self.file.sync_all() {
            self.poisoned = true;
            return Err(e);
        }
        Ok(Durable {
            journal_seq: self.next.saturating_sub(1),
            hash: self.last,
        })
    }
    /// The manifest is itself a hash-chained binary frame, never a parallel JSON file.
    pub fn seal(&mut self, complete: bool) -> io::Result<Durable> {
        let v = serde_json::json!({"frames":self.next,"root_sha256":hex(&self.last),"status":if complete{"COMPLETE"}else{"INCOMPLETE"}});
        self.append_value(MANIFEST, &v)?;
        let durable = self.sync()?;
        self.sealed = true;
        Ok(durable)
    }
}

pub struct Reader {
    file: File,
    next: u64,
    last: [u8; 32],
    position: u64,
    pub sealed: bool,
    pub complete: bool,
    pub identities: [u8; 48],
}
impl Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut header = [0u8; HEADER_LEN as usize];
        file.read_exact(&mut header)
            .map_err(|_| bad("HEADER_TRUNCATED"))?;
        if &header[..8] != MAGIC
            || u16::from_le_bytes(header[8..10].try_into().unwrap()) != VERSION
            || u16::from_le_bytes(header[10..12].try_into().unwrap()) as u64 != HEADER_LEN
        {
            return Err(bad("JOURNAL_VERSION_OR_MAGIC"));
        }
        Ok(Self {
            file,
            next: 0,
            last: sha(&header),
            position: HEADER_LEN,
            sealed: false,
            complete: false,
            identities: header[12..60].try_into().unwrap(),
        })
    }
    /// Frames read so far (including a manifest frame).
    pub fn frames(&self) -> u64 {
        self.next
    }
    /// Chain head after the frames read so far: the header hash before the first frame.
    pub fn root(&self) -> [u8; 32] {
        self.last
    }
    pub fn next_frame(&mut self) -> io::Result<Option<Frame>> {
        let Some((f, previous)) = read_frame(&mut self.file, self.position)? else {
            return Ok(None);
        };
        if self.sealed {
            return Err(bad("DATA_AFTER_MANIFEST"));
        }
        if f.journal_seq != self.next || previous != self.last {
            return Err(bad("JOURNAL_SEQUENCE_OR_CHAIN"));
        }
        if f.kind == MANIFEST {
            if f.value["frames"] != self.next
                || f.value["root_sha256"] != hex(&self.last)
                || !matches!(f.value["status"].as_str(), Some("COMPLETE" | "INCOMPLETE"))
            {
                return Err(bad("MANIFEST_MISMATCH"));
            }
            self.sealed = true;
            self.complete = f.value["status"] == "COMPLETE";
        }
        self.next += 1;
        self.last = f.hash;
        self.position += f.bytes;
        Ok(Some(f))
    }
}
fn read_frame(file: &mut File, offset: u64) -> io::Result<Option<(Frame, [u8; 32])>> {
    let mut length = [0; 4];
    match file.read(&mut length[..1])? {
        0 => return Ok(None),
        _ => file
            .read_exact(&mut length[1..])
            .map_err(|_| bad("FRAME_TRUNCATED"))?,
    };
    let length = u32::from_le_bytes(length) as usize;
    if !(FIXED + 36..=MAX_FRAME).contains(&length) {
        return Err(bad("FRAME_LENGTH"));
    }
    let mut b = vec![0; length];
    file.read_exact(&mut b)
        .map_err(|_| bad("FRAME_TRUNCATED"))?;
    let end = length - 36;
    if crc(&b[..end]) != u32::from_le_bytes(b[end..end + 4].try_into().unwrap()) {
        return Err(bad("FRAME_CRC"));
    }
    if sha(&b[..end + 4]) != b[end + 4..] {
        return Err(bad("FRAME_SHA256"));
    }
    let kind = u16::from_le_bytes(b[..2].try_into().unwrap());
    let schema = u16::from_le_bytes(b[2..4].try_into().unwrap());
    if kind == 0 || schema != 1 {
        return Err(bad("EVENT_SCHEMA_UNSUPPORTED"));
    }
    let u64at = |i| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
    let flags = u32::from_le_bytes(b[24..28].try_into().unwrap());
    if flags & !15 != 0 {
        return Err(bad("PROVENANCE_FLAGS"));
    }
    let payload_len = u32::from_le_bytes(b[116..120].try_into().unwrap()) as usize;
    if payload_len + 120 != end {
        return Err(bad("PAYLOAD_LENGTH"));
    }
    let f = Frame {
        kind,
        schema,
        journal_seq: u64at(4),
        provenance: Provenance {
            source_seq: u64at(12),
            source: u32::from_le_bytes(b[20..24].try_into().unwrap()),
            slot: (flags & 1 != 0).then(|| u64at(28)),
            fork: (flags & 2 != 0).then(|| b[36..68].try_into().unwrap()),
            rx_ns: (flags & 4 != 0).then(|| u64at(68)),
            unix_ns: (flags & 8 != 0).then(|| u64at(76)),
        },
        value: decode(&b[120..end])?,
        offset,
        bytes: length as u64 + 4,
        hash: b[end + 4..].try_into().unwrap(),
    };
    Ok(Some((f, b[84..116].try_into().unwrap())))
}
pub fn read_at(file: &mut File, offset: u64) -> io::Result<Frame> {
    file.seek(SeekFrom::Start(offset))?;
    read_frame(file, offset)?
        .map(|x| x.0)
        .ok_or_else(|| bad("FRAME_MISSING"))
}
pub fn values(path: &Path) -> io::Result<Vec<Value>> {
    let mut r = Reader::open(path)?;
    let mut values = Vec::new();
    while let Some(f) = r.next_frame()? {
        if f.kind != MANIFEST {
            values.push(f.value);
        }
    }
    Ok(values)
}
pub fn write_value(path: &Path, kind: u16, value: &Value) -> io::Result<()> {
    let mut w = Writer::create(&binary_path(path))?;
    w.append_value(kind, value)?;
    w.seal(true)?;
    Ok(())
}
pub fn read_value(path: &Path) -> io::Result<Value> {
    let binary = binary_path(path);
    if binary.exists() {
        let mut rows = values(&binary)?;
        if rows.len() != 1 {
            return Err(bad("VALUE_COUNT"));
        }
        Ok(rows.remove(0))
    } else {
        serde_json::from_reader(File::open(path)?).map_err(io::Error::other)
    }
}
/// Explicit offline export. A fresh destination is mandatory; primary bytes are never changed.
pub fn export(path: &Path, destination: &Path, kind: Option<u16>, single: bool) -> io::Result<()> {
    export_views(path, &[(kind, destination)], single).map(|_| ())
}
/// Explicit offline export of several views (JSONL, one value per line) in one verified
/// pass. The views appear only complete and together: each is built beside its destination
/// and linked into place without overwriting after the whole journal was read without
/// error; a failed or damaged journal leaves no file behind. Returns the rows per view.
pub fn export_views(
    path: &Path,
    views: &[(Option<u16>, &Path)],
    single: bool,
) -> io::Result<Vec<u64>> {
    if views.iter().any(|(_, destination)| destination.exists()) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "EXPORT_EXISTS",
        ));
    }
    let mut reader = Reader::open(path)?;
    let token = hex(&random16()?);
    let partials: Vec<PathBuf> = views
        .iter()
        .map(|(_, destination)| {
            destination.with_file_name(format!(
                ".{}.partial-{}-{token}",
                destination
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("export"),
                std::process::id()
            ))
        })
        .collect();
    let mut linked = Vec::new();
    let written = (|| {
        let mut outs = partials
            .iter()
            .map(|p| {
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(p)
                    .map(BufWriter::new)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let mut rows = vec![0u64; views.len()];
        while let Some(f) = reader.next_frame()? {
            if f.kind == MANIFEST {
                continue;
            }
            for (i, (kind, _)) in views.iter().enumerate() {
                if kind.is_some_and(|k| f.kind != k) {
                    continue;
                }
                if single && rows[i] > 0 {
                    return Err(bad("VALUE_COUNT"));
                }
                serde_json::to_writer(&mut outs[i], &f.value).map_err(io::Error::other)?;
                outs[i].write_all(b"\n")?;
                rows[i] += 1;
            }
        }
        for out in outs {
            out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        }
        for (partial, (_, destination)) in partials.iter().zip(views) {
            fs::hard_link(partial, destination)?;
            linked.push(destination.to_path_buf());
        }
        for (_, destination) in views {
            File::open(parent(destination))?.sync_all()?;
        }
        Ok(rows)
    })();
    for partial in &partials {
        let _ = fs::remove_file(partial);
    }
    if written.is_err() {
        for destination in &linked {
            let _ = fs::remove_file(destination);
        }
    }
    written
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
