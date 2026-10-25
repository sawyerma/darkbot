//! PERSIST-01: one binary append-only contract for runtime and durable financial events.
//! LE integers, bounded frames, CRC32 + chained SHA256.
//! JFI-01: frames of schema ≥ 2 carry typed native payloads (`payload::Payload`). Schema 1, the
//! generic value tree, is historical: readable via `Frame::legacy_value`, never written (JFI-04
//! migrated the last kinds; the value encoder exists only for test fixtures of old journals).
use super::payload::{self, Payload};
use serde_json::{Map, Number, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

pub const MAGIC: &[u8; 8] = b"P0JRNL01";
pub const VERSION: u16 = 1;
pub const HEADER_LEN: u64 = 60;
pub const MAX_FRAME: usize = 8 * 1024 * 1024;
pub const MANIFEST: u16 = u16::MAX;
const FIXED: usize = 120;
/// Bytes of one frame beyond its payload: length prefix, fixed header, CRC and SHA-256.
pub const FRAME_WIRE: usize = 4 + FIXED + 36;

/// Event kinds of the one journal contract. Every writer takes its kind from here, so a kind
/// is never reused for a different payload. Unknown kinds are rejected by the typed readers.
pub mod kind {
    pub const OWNER_RECORD: u16 = 100;
    pub const OWNER_OPPORTUNITY: u16 = 101;
    pub const OWNER_RECONCILE: u16 = 102;
    pub const OWNER_SUMMARY: u16 = 103;
    /// Owner diagnostic line `{line}`; the writer thread also prints it to stderr afterwards.
    pub const OWNER_DIAGNOSTIC: u16 = 104;
    /// F1: bounded, redacted program logs of one failed simulation (`SimulationFailureRecord`).
    pub const OWNER_SIMULATION_FAILURE: u16 = 105;
    /// FE-04 single-flight automaton: one full state version per durable transition.
    pub const FE04_TRANSITION: u16 = 200;
    /// FE-04 import of a pre-binary JSON wallet journal; the JSON file itself stays unchanged.
    pub const FE04_LEGACY_IMPORT: u16 = 201;
    /// Market capture rows, exactly the legacy JSONL row values (`transactions.jsonl`,
    /// `liquidations.jsonl`, `coverage.jsonl` are renders of these kinds).
    pub const MARKET_TRANSACTION: u16 = 300;
    pub const MARKET_LIQUIDATION: u16 = 301;
    pub const MARKET_COVERAGE: u16 = 302;
    /// RELATED_NON_LIQUIDATION instruction (PHASENPLAN L.5): never a liquidation or profit.
    pub const MARKET_RELATED: u16 = 303;
    /// Qualification axes of a finished market capture (`market_completion.p0j`, B7): the
    /// values `summary.json` reports, typed for the native report.
    pub const MARKET_COMPLETION: u16 = 304;
    /// The finished capture's market summary (`market_summary.p0j`, JFI-FIX-05): typed record;
    /// `summary.json` is its explicit offline render for the pinned readers.
    pub const MARKET_SUMMARY: u16 = 305;
    /// Strict verifier (A3): resolved slot scope, batch checkpoint and result — each a sealed
    /// single-value journal. `STRICT_RPC_CACHE` is HISTORICAL (raw JSON-RPC wire text, before F10):
    /// read-only for earlier runs, never written (typed successor `STRICT_CHAIN_CACHE`).
    pub const STRICT_SCOPE: u16 = 310;
    pub const STRICT_RPC_CACHE: u16 = 311;
    pub const STRICT_CHECKPOINT: u16 = 312;
    pub const STRICT_RESULT: u16 = 313;
    /// A3 native driver: stable spec, immutable capture evidence, attempt start/end records.
    pub const STRICT_SPEC: u16 = 314;
    pub const CAPTURE_EVIDENCE: u16 = 315;
    pub const STRICT_ATTEMPT: u16 = 316;
    /// C0E1-P4-RESUME: post-capture deployment continuity of one canonical strict result.
    pub const DEPLOYMENT_PROOF: u16 = 317;
    /// Strict verifier typed chain-read cache (F10): read, decoder version, decoded answer.
    /// Replaces `STRICT_RPC_CACHE` (311), which stays readable for earlier runs only.
    pub const STRICT_CHAIN_CACHE: u16 = 318;
    /// SK-06 capacity preflight report (single-value journal before a capture).
    pub const CAPACITY_PREFLIGHT: u16 = 320;
    /// SK-07 segment chain control frames (first frame of a continuation / last before seal).
    pub const SEGMENT_LINK: u16 = 330;
    pub const SEGMENT_NEXT: u16 = 331;
    /// Native C0-E1 report (B7): normalized dataset identity and one reproducible query
    /// (`report_query_identity`), each a sealed single-value journal under `reports/native/`.
    pub const REPORT_DATASET: u16 = 340;
    pub const REPORT_QUERY: u16 = 341;
    /// C0E1-P4-VC: the separate C0-E1 master qualification after the verification (PHASENPLAN
    /// L.2), a sealed single-value journal under `qualification/`.
    pub const C0E1_QUALIFICATION: u16 = 342;
    /// Native backtest (STARTUP §4 C): one sealed journal per job under `reports/backtests/` —
    /// its header (inputs, parameters, engine), one row per verified liquidation, the summary.
    pub const BACKTEST_HEADER: u16 = 350;
    pub const BACKTEST_ROW: u16 = 351;
    pub const BACKTEST_SUMMARY: u16 = 352;
    /// Native optimizer (STARTUP §4 E): one sealed single-value journal under `reports/optimize/`.
    pub const OPTIMIZE_RESULT: u16 = 353;
    /// Offline Shadow candidate profile (STARTUP §4 F), a sealed single-value journal.
    pub const BACKTEST_PROFILE: u16 = 354;
    /// A backtest's false positive: an executable evaluation without a verified liquidation.
    pub const BACKTEST_ATTEMPT: u16 = 355;
    /// B03/U-9 analytics job controller: a job's typed request and its terminal outcome, each a
    /// sealed single-value journal under `data/analytics-jobs/<id>/`.
    pub const ANALYTICS_JOB_REQUEST: u16 = 356;
    pub const ANALYTICS_JOB_OUTCOME: u16 = 357;
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
    /// The run's measurement plan as handed from the plan compiler to the runtime (JFI-04).
    pub const MEASUREMENT_PLAN: u16 = 501;
    /// Secret-free effective operational configuration (JFI-04).
    pub const OPERATIONAL_CONFIG: u16 = 502;
    /// Pre-capture run manifest; `manifest.json` is its render for the pinned run store.
    pub const RUN_MANIFEST: u16 = 503;
    /// MT0 measurement snapshot (cumulative counters), off-owner reporter thread.
    pub const MT0_SNAPSHOT: u16 = 600;
    /// BOOT network audit result (`NetworkAudit`, typed; RPC answers are decoded at the
    /// `boot_network` wire edge and never stored as JSON).
    pub const BOOT_NETWORK: u16 = 710;
    /// Measured FREE_ONLY provider evidence read by the network boot audit, imported offline
    /// from the measured JSON (`p0-journal import-provider-evidence`).
    pub const PROVIDER_EVIDENCE: u16 = 711;
    pub const FE01_CONFIG: u16 = 700;
    pub const FE01_READ: u16 = 701;
    pub const FE01_RESULT: u16 = 702;
    /// FE-01 one-class offline freeze (`payload::Fe01Freeze`), its own journal `fe01_freeze.p0j`.
    pub const FE01_FREEZE: u16 = 703;
    /// FE-01 smoke result (`payload::Fe01Smoke`), its own journal `fe01_smoke.p0j` beside the
    /// typed reads (`--fe-mode smoke`, the P1A deployment/target observation).
    pub const FE01_SMOKE: u16 = 704;
    /// C0 account census rows (`c0_census.py` via `p0-journal import`): program accounts and
    /// group indexer accounts of one context slot, typed (`payload::C0CensusImportAccount`,
    /// `C0CensusImportGroupAccount`); historical schema-1 value frames stay readable.
    pub const C0_CENSUS_ACCOUNT: u16 = 900;
    pub const C0_CENSUS_GROUP_ACCOUNT: u16 = 901;
    /// C0 census (Shadow READY): one typed row per debtor of the group, then one result frame
    /// binding clock, model, coverage and totals (`payload::C0CensusAccount/C0CensusResult`).
    pub const C0_CENSUS_ROW: u16 = 902;
    pub const C0_CENSUS_RESULT: u16 = 903;
    /// Kinds an explicit offline import may write; runtime and financial kinds never.
    pub const IMPORTABLE: &[u16] = &[C0_CENSUS_ACCOUNT, C0_CENSUS_GROUP_ACCOUNT];
    /// Owner events that could not reach the primary journal (recovery journal only).
    pub const OWNER_RECOVERY: u16 = 1000;
    /// S1-02: one operator source-control request (`p0ctl source set`), a sealed single-value
    /// journal `data/source-control/request_<seq>.p0j`.
    pub const SOURCE_CONTROL_REQUEST: u16 = 1100;
    /// S1-02: the state owner's decision on one request, a sealed single-value journal
    /// `data/source-control/outcome_<seq>.p0j`.
    pub const SOURCE_CONTROL_OUTCOME: u16 = 1101;
    pub const ALL: &[u16] = &[
        OWNER_RECORD,
        OWNER_OPPORTUNITY,
        OWNER_RECONCILE,
        OWNER_SUMMARY,
        OWNER_DIAGNOSTIC,
        OWNER_SIMULATION_FAILURE,
        FE04_TRANSITION,
        FE04_LEGACY_IMPORT,
        MARKET_TRANSACTION,
        MARKET_LIQUIDATION,
        MARKET_COVERAGE,
        MARKET_RELATED,
        MARKET_COMPLETION,
        MARKET_SUMMARY,
        STRICT_SCOPE,
        STRICT_RPC_CACHE,
        STRICT_CHECKPOINT,
        STRICT_RESULT,
        STRICT_SPEC,
        CAPTURE_EVIDENCE,
        STRICT_ATTEMPT,
        DEPLOYMENT_PROOF,
        STRICT_CHAIN_CACHE,
        CAPACITY_PREFLIGHT,
        SEGMENT_LINK,
        SEGMENT_NEXT,
        REPORT_DATASET,
        REPORT_QUERY,
        C0E1_QUALIFICATION,
        BACKTEST_HEADER,
        BACKTEST_ROW,
        BACKTEST_SUMMARY,
        OPTIMIZE_RESULT,
        BACKTEST_PROFILE,
        BACKTEST_ATTEMPT,
        ANALYTICS_JOB_REQUEST,
        ANALYTICS_JOB_OUTCOME,
        ECON_SHADOW_EVALUATION,
        ECON_SNAPSHOT,
        ECON_FINAL,
        ECON_OPPORTUNITY,
        ECON_CANONICAL,
        MEASUREMENT_PHASE,
        MEASUREMENT_PLAN,
        OPERATIONAL_CONFIG,
        RUN_MANIFEST,
        MT0_SNAPSHOT,
        BOOT_NETWORK,
        PROVIDER_EVIDENCE,
        FE01_CONFIG,
        FE01_READ,
        FE01_RESULT,
        FE01_FREEZE,
        FE01_SMOKE,
        C0_CENSUS_ACCOUNT,
        C0_CENSUS_GROUP_ACCOUNT,
        C0_CENSUS_ROW,
        C0_CENSUS_RESULT,
        OWNER_RECOVERY,
        SOURCE_CONTROL_REQUEST,
        SOURCE_CONTROL_OUTCOME,
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
    /// C0 census worker of the Shadow runtime (typed rows and result).
    pub const C0_CENSUS_RUNTIME: u32 = 9;
    /// Post-capture strict verifier.
    pub const STRICT: u32 = 10;
    /// Storage lane: capacity preflight.
    pub const STORAGE: u32 = 11;
    /// Native C0-E1 report (B7).
    pub const REPORT: u32 = 12;
    /// S1-02 operator source control (`p0ctl source set`).
    pub const SOURCE_CONTROL: u32 = 13;
    /// Native backtest, optimizer, their profiles and the analytics job controller.
    pub const BACKTEST: u32 = 14;
}
/// M6: the static code of a journal integrity failure (FRAME_CRC, MANIFEST_MISMATCH, …),
/// carried as the typed source of its `io::Error`; callers classify by type (`code_of`), never
/// by message text. `Display` is the code itself, as before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Code(pub &'static str);
impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Code {}
/// The journal code of an error raised by the journal, segment or payload layer.
pub fn code_of(e: &io::Error) -> Option<&'static str> {
    e.get_ref()?.downcast_ref::<Code>().map(|c| c.0)
}
pub(crate) fn coded(code: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, Code(code))
}
fn bad(code: &'static str) -> io::Error {
    coded(code)
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
/// 16 fresh bytes from the operating system. A journal is never created without them; the
/// segment chain uses the same source for its set id (F5: one implementation).
pub(crate) fn random16() -> io::Result<[u8; 16]> {
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
    /// Raw payload bytes; decode with `typed` (schema ≥ 2) or `legacy_value` (schema 1).
    pub payload: Vec<u8>,
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
impl Frame {
    pub fn typed<P: Payload>(&self) -> io::Result<P> {
        payload::decode(self.kind, self.schema, &self.payload)
    }
    /// Historical schema-1 value tree: offline readers and not-yet-migrated kinds only.
    pub fn legacy_value(&self) -> io::Result<Value> {
        if self.schema != 1 {
            return Err(bad("TYPED_FRAME_NOT_LEGACY"));
        }
        decode(&self.payload)
    }
}
#[derive(Clone, Debug)]
pub struct Durable {
    pub journal_seq: u64,
    pub hash: [u8; 32],
}

/// The historical schema-1 value encoder, kept for test fixtures of journals written before
/// JFI-04 (production writes typed frames only). Numbers are tagged LE i128/u128, or exact
/// decimal text for nonintegral / wider external JSON numbers; no f64 coercion.
#[cfg(test)]
pub(crate) fn encode(value: &Value) -> io::Result<Vec<u8>> {
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
    #[cfg(test)]
    next_sync_error: Option<io::ErrorKind>,
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
            #[cfg(test)]
            next_sync_error: None,
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
            #[cfg(test)]
            next_sync_error: None,
        })
    }
    #[cfg(test)]
    pub(crate) fn test_read_only(&mut self) {
        self.file = File::open(&self.path).unwrap();
    }
    /// Deterministic per-writer IO-boundary injection, absent from production builds.
    /// The frame is really appended before sync fails; the normal poison/error path runs.
    #[cfg(test)]
    pub(crate) fn test_fail_next_sync(&mut self, kind: io::ErrorKind) {
        self.next_sync_error = Some(kind);
    }
    fn sync_file(&mut self) -> io::Result<()> {
        #[cfg(test)]
        if let Some(kind) = self.next_sync_error.take() {
            return Err(io::Error::new(kind, "injected fsync failure"));
        }
        self.file.sync_all()
    }
    pub fn next_seq(&self) -> u64 {
        self.next
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn append_typed<P: Payload>(&mut self, p: Provenance, value: &P) -> io::Result<Appended> {
        let payload = payload::encode(value)?;
        self.append(P::KIND, P::SCHEMA, p, &payload)
    }
    /// Raw frame: `payload` comes from `payload::encode` (schema ≥ 2) or the legacy value
    /// encoder (schema 1). Readers validate every frame against its registered type.
    pub(crate) fn append(
        &mut self,
        kind: u16,
        schema: u16,
        p: Provenance,
        payload: &[u8],
    ) -> io::Result<Appended> {
        if kind == 0 || schema == 0 {
            return Err(bad("EVENT_SCHEMA_UNSUPPORTED"));
        }
        // PERSIST-05: the manifest seals the chain; only `seal` writes it (a hand-made one would
        // end the journal for readers while this writer goes on).
        if kind == MANIFEST {
            return Err(bad("MANIFEST_ONLY_BY_SEAL"));
        }
        payload::check_write(kind, schema)?;
        self.write_frame(kind, schema, p, payload)
    }
    /// A historical schema-1 frame of any kind, bypassing write admission: only for tests
    /// that replay journals written before a kind was migrated.
    #[cfg(test)]
    pub(crate) fn test_append_historical(
        &mut self,
        kind: u16,
        value: &Value,
    ) -> io::Result<Appended> {
        self.test_append_historical_at(kind, Provenance::default(), value)
    }
    /// [`Self::test_append_historical`] with the frame's provenance.
    #[cfg(test)]
    pub(crate) fn test_append_historical_at(
        &mut self,
        kind: u16,
        p: Provenance,
        value: &Value,
    ) -> io::Result<Appended> {
        let payload = encode(value)?;
        self.write_frame(kind, 1, p, &payload)
    }
    /// A frame bypassing write admission: only for tests proving that readers reject what the
    /// writer would refuse.
    #[cfg(test)]
    pub(crate) fn test_append_raw(
        &mut self,
        kind: u16,
        schema: u16,
        payload: &[u8],
    ) -> io::Result<Appended> {
        self.write_frame(kind, schema, Provenance::default(), payload)
    }
    fn write_frame(
        &mut self,
        kind: u16,
        schema: u16,
        p: Provenance,
        payload: &[u8],
    ) -> io::Result<Appended> {
        if self.poisoned || self.sealed {
            return Err(bad("JOURNAL_BLOCKED"));
        }
        let mut wire = Vec::with_capacity(4 + FIXED + payload.len() + 36);
        let f = self.encode_frame(kind, schema, &p, payload, &mut wire)?;
        if let Err(e) = self.file.write_all(&wire) {
            self.poisoned = true;
            return Err(e);
        }
        Ok(f)
    }
    /// SK-04: several admitted frames, chained in order, written with one sequential write.
    /// The frames get consecutive sequence numbers; a failed write poisons the writer (the
    /// unsealed tail is INCOMPLETE for every reader), so no partial batch is ever continued.
    pub fn append_batch<'a, I>(&mut self, frames: I) -> io::Result<u64>
    where
        I: IntoIterator<Item = (u16, u16, &'a Provenance, &'a [u8])>,
        I::IntoIter: Clone,
    {
        if self.poisoned || self.sealed {
            return Err(bad("JOURNAL_BLOCKED"));
        }
        let frames = frames.into_iter();
        // Exact capacity: the batch buffer never over-allocates beyond its frames.
        let capacity: usize = frames
            .clone()
            .map(|(_, _, _, payload)| FRAME_WIRE + payload.len())
            .sum();
        let mut wire = Vec::with_capacity(capacity);
        let mut n = 0u64;
        for (kind, schema, p, payload) in frames {
            if kind == 0 || schema == 0 {
                self.poisoned = true;
                return Err(bad("EVENT_SCHEMA_UNSUPPORTED"));
            }
            if let Err(e) = payload::check_write(kind, schema)
                .and_then(|_| self.encode_frame(kind, schema, p, payload, &mut wire))
            {
                // The chain state may already include earlier frames of this batch.
                self.poisoned = true;
                return Err(e);
            }
            n += 1;
        }
        if let Err(e) = self.file.write_all(&wire) {
            self.poisoned = true;
            return Err(e);
        }
        Ok(n)
    }
    /// Appends one encoded frame to `out` and advances sequence, position and chain head.
    fn encode_frame(
        &mut self,
        kind: u16,
        schema: u16,
        p: &Provenance,
        payload: &[u8],
        out: &mut Vec<u8>,
    ) -> io::Result<Appended> {
        let start = out.len();
        // Length prefix first (filled in below), then the body; one buffer, no second copy.
        out.extend([0u8; 4]);
        out.extend(kind.to_le_bytes());
        out.extend(schema.to_le_bytes());
        out.extend(self.next.to_le_bytes());
        out.extend(p.source_seq.to_le_bytes());
        out.extend(p.source.to_le_bytes());
        let flags = (p.slot.is_some() as u32)
            | ((p.fork.is_some() as u32) << 1)
            | ((p.rx_ns.is_some() as u32) << 2)
            | ((p.unix_ns.is_some() as u32) << 3);
        out.extend(flags.to_le_bytes());
        out.extend(p.slot.unwrap_or(0).to_le_bytes());
        out.extend(p.fork.unwrap_or([0; 32]));
        out.extend(p.rx_ns.unwrap_or(0).to_le_bytes());
        out.extend(p.unix_ns.unwrap_or(0).to_le_bytes());
        out.extend(self.last);
        out.extend((payload.len() as u32).to_le_bytes());
        out.extend(payload);
        let checksum = crc(&out[start + 4..]);
        out.extend(checksum.to_le_bytes());
        let digest = sha(&out[start + 4..]);
        out.extend(digest);
        let body = out.len() - start - 4;
        if body > MAX_FRAME {
            out.truncate(start);
            return Err(bad("FRAME_LIMIT"));
        }
        out[start..start + 4].copy_from_slice(&(body as u32).to_le_bytes());
        let bytes = (out.len() - start) as u64;
        let f = Appended {
            journal_seq: self.next,
            offset: self.position,
            bytes,
            hash: digest,
        };
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| bad("SEQUENCE_EXHAUSTED"))?;
        self.position += bytes;
        self.last = digest;
        Ok(f)
    }
    pub fn sync(&mut self) -> io::Result<Durable> {
        if self.poisoned {
            return Err(bad("JOURNAL_BLOCKED"));
        }
        if let Err(e) = self.sync_file() {
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
        let manifest = payload::Manifest {
            frames: self.next,
            root_sha256: self.last,
            complete,
        };
        let p = Provenance {
            source_seq: self.next,
            ..Default::default()
        };
        let encoded = payload::encode(&manifest)?;
        self.write_frame(
            MANIFEST,
            <payload::Manifest as Payload>::SCHEMA,
            p,
            &encoded,
        )?;
        let durable = self.sync()?;
        self.sealed = true;
        Ok(durable)
    }
}

pub struct Reader {
    capture_tail: bool,
    pub tail_truncated: bool,
    file: File,
    next: u64,
    last: [u8; 32],
    position: u64,
    pub sealed: bool,
    pub complete: bool,
    pub identities: [u8; 48],
}
impl Reader {
    /// Capture-only prefix salvage. Financial journals and Writer::resume use strict open.
    pub fn open_capture(path: &Path) -> io::Result<Self> {
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err(bad("CAPTURE_NOT_REGULAR"));
        }
        let mut r = Self::open(path)?;
        r.capture_tail = true;
        Ok(r)
    }
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
            capture_tail: false,
            tail_truncated: false,
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
        if self.tail_truncated {
            return Ok(None);
        }
        let frame = match read_frame(&mut self.file, self.position) {
            Err(e)
                if self.capture_tail
                    && !self.sealed
                    && e.kind() == io::ErrorKind::UnexpectedEof =>
            {
                self.tail_truncated = true;
                self.complete = false;
                return Ok(None);
            }
            other => other?,
        };
        let Some((f, previous)) = frame else {
            return Ok(None);
        };
        if self.sealed {
            return Err(bad("DATA_AFTER_MANIFEST"));
        }
        if f.journal_seq != self.next || previous != self.last {
            return Err(bad("JOURNAL_SEQUENCE_OR_CHAIN"));
        }
        if f.kind == MANIFEST {
            let complete = if f.schema == 1 {
                // Historical manifest (value tree).
                let v = f.legacy_value()?;
                if v["frames"] != self.next
                    || v["root_sha256"] != hex(&self.last)
                    || !matches!(v["status"].as_str(), Some("COMPLETE" | "INCOMPLETE"))
                {
                    return Err(bad("MANIFEST_MISMATCH"));
                }
                v["status"] == "COMPLETE"
            } else {
                let m: payload::Manifest = f.typed()?;
                if m.frames != self.next || m.root_sha256 != self.last {
                    return Err(bad("MANIFEST_MISMATCH"));
                }
                m.complete
            };
            self.sealed = true;
            self.complete = complete;
        }
        self.next += 1;
        self.last = f.hash;
        self.position += f.bytes;
        Ok(Some(f))
    }
}
/// N3: after power loss ext4/XFS may extend a file with zeros instead of truncating it. A zero
/// frame length is a torn tail (`UnexpectedEof`, which only an unsealed capture tolerates) only
/// if every byte up to EOF is zero, checked in bounded 64 KiB blocks; any other byte keeps the
/// hard `FRAME_LENGTH` error. Strict readers propagate either error.
fn zero_filled_tail<R: Read>(file: &mut R) -> io::Result<Option<(Frame, [u8; 32])>> {
    let mut block = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut block)?;
        if n == 0 {
            // M6: typed code; the kind stays UnexpectedEof (torn-tail tolerance, H1/N3).
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                Code("TAIL_ZERO_FILLED"),
            ));
        }
        if block[..n].iter().any(|b| *b != 0) {
            return Err(bad("FRAME_LENGTH"));
        }
    }
}
fn read_frame<R: Read>(file: &mut R, offset: u64) -> io::Result<Option<(Frame, [u8; 32])>> {
    let mut length = [0; 4];
    match file.read(&mut length[..1])? {
        0 => return Ok(None),
        _ => file.read_exact(&mut length[1..]).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                io::Error::new(io::ErrorKind::UnexpectedEof, Code("FRAME_TRUNCATED"))
            } else {
                e
            }
        })?,
    };
    if length == [0; 4] {
        return zero_filled_tail(file);
    }
    let length = u32::from_le_bytes(length) as usize;
    if !(FIXED + 36..=MAX_FRAME).contains(&length) {
        return Err(bad("FRAME_LENGTH"));
    }
    let mut b = vec![0; length];
    file.read_exact(&mut b).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            io::Error::new(io::ErrorKind::UnexpectedEof, Code("FRAME_TRUNCATED"))
        } else {
            e
        }
    })?;
    let end = length - 36;
    if crc(&b[..end]) != u32::from_le_bytes(b[end..end + 4].try_into().unwrap()) {
        return Err(bad("FRAME_CRC"));
    }
    if sha(&b[..end + 4]) != b[end + 4..] {
        return Err(bad("FRAME_SHA256"));
    }
    let kind = u16::from_le_bytes(b[..2].try_into().unwrap());
    let schema = u16::from_le_bytes(b[2..4].try_into().unwrap());
    if kind == 0 || schema == 0 {
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
        payload: {
            let bytes = &b[120..end];
            if schema == 1 {
                decode(bytes)?;
            } else {
                payload::validate(kind, schema, bytes)?;
            }
            bytes.to_vec()
        },
        offset,
        bytes: length as u64 + 4,
        hash: b[end + 4..].try_into().unwrap(),
    };
    Ok(Some((f, b[84..116].try_into().unwrap())))
}
/// E5: result of the bounded structural screening of a journal's end (`screen_seal`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealScreen {
    /// The last frame is a CRC/SHA-valid manifest whose frame count and root match its own
    /// sequence and predecessor hash. Frames before it are not read: no chain verification.
    Sealed { complete: bool },
    /// The last frame is a regular frame, or the journal has no frame yet.
    Unsealed,
    /// No decodable last frame (torn, zero-filled, oversized or corrupt tail), an inconsistent
    /// manifest, or an unreadable header/file; the code names the class.
    Undecodable(&'static str),
}
/// Bytes at the end of a journal searched for its last frame; a manifest frame has a few hundred.
pub const SEAL_SCREEN_TAIL: u64 = 64 * 1024;
/// E5: seal state from the header and at most `SEAL_SCREEN_TAIL` trailing bytes, independent of
/// the journal size. A journal whose last frame is longer than the window reads as
/// `Undecodable("TAIL_UNDECODABLE")`, never as sealed; a sealed journal always ends with its
/// small manifest. The full chain check stays with `Reader`/the strict verifier.
pub fn screen_seal(path: &Path) -> SealScreen {
    use SealScreen::*;
    let Ok(mut file) = File::open(path) else {
        return Undecodable("OPEN");
    };
    let mut header = [0u8; HEADER_LEN as usize];
    if file.read_exact(&mut header).is_err() {
        return Undecodable("HEADER_TRUNCATED");
    }
    if &header[..8] != MAGIC
        || u16::from_le_bytes(header[8..10].try_into().unwrap()) != VERSION
        || u16::from_le_bytes(header[10..12].try_into().unwrap()) as u64 != HEADER_LEN
    {
        return Undecodable("JOURNAL_VERSION_OR_MAGIC");
    }
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return Undecodable("METADATA");
    };
    if len == HEADER_LEN {
        return Unsealed;
    }
    let start = len.saturating_sub(SEAL_SCREEN_TAIL).max(HEADER_LEN);
    let mut tail = vec![0u8; (len - start) as usize];
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut tail).is_err() {
        return Undecodable("TAIL_READ");
    }
    // The last frame starts where its length prefix covers exactly the rest of the file; a
    // coincidental match fails the frame's own CRC/SHA-256.
    let Some(last_start) = tail.len().checked_sub(FRAME_WIRE) else {
        return Undecodable("TAIL_UNDECODABLE");
    };
    for o in (0..=last_start).rev() {
        let length = u32::from_le_bytes(tail[o..o + 4].try_into().unwrap()) as usize;
        if length + 4 != tail.len() - o {
            continue;
        }
        let Ok(Some((f, previous))) = read_frame(&mut &tail[o..], start + o as u64) else {
            continue;
        };
        if f.kind != MANIFEST {
            return Unsealed;
        }
        let complete = if f.schema == 1 {
            match f.legacy_value() {
                Ok(v)
                    if v["frames"] == f.journal_seq
                        && v["root_sha256"] == hex(&previous)
                        && matches!(v["status"].as_str(), Some("COMPLETE" | "INCOMPLETE")) =>
                {
                    Some(v["status"] == "COMPLETE")
                }
                _ => None,
            }
        } else {
            match f.typed::<payload::Manifest>() {
                Ok(m) if m.frames == f.journal_seq && m.root_sha256 == previous => Some(m.complete),
                _ => None,
            }
        };
        return match complete {
            Some(complete) => Sealed { complete },
            None => Undecodable("MANIFEST_MISMATCH"),
        };
    }
    Undecodable("TAIL_UNDECODABLE")
}
pub fn read_at(file: &mut File, offset: u64) -> io::Result<Frame> {
    file.seek(SeekFrom::Start(offset))?;
    read_frame(file, offset)?
        .map(|x| x.0)
        .ok_or_else(|| bad("FRAME_MISSING"))
}
/// One typed value as a sealed single-frame journal. Immutable: an existing file must hold an
/// equal value (accepted) or the write is a conflict; a new file appears atomically (built
/// beside its destination, fsynced, hard-linked into place).
pub fn write_typed_value<P: Payload + PartialEq>(
    path: &Path,
    source: u32,
    value: &P,
) -> io::Result<()> {
    if path.exists() {
        return if read_typed_value::<P>(path)? == *value {
            Ok(())
        } else {
            Err(bad("IMMUTABLE_CONFLICT"))
        };
    }
    let partial = path.with_file_name(format!(
        ".{}.partial-{}-{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("value"),
        std::process::id(),
        hex(&random16()?)
    ));
    let written = (|| {
        let mut w = Writer::create(&partial)?;
        w.append_typed(
            Provenance {
                source,
                ..Default::default()
            },
            value,
        )?;
        w.seal(true)?;
        fs::hard_link(&partial, path)?;
        File::open(parent(path))?.sync_all()
    })();
    let _ = fs::remove_file(&partial);
    written
}
/// The value of a sealed, complete single-frame journal of `P`'s kind.
pub fn read_typed_value<P: Payload>(path: &Path) -> io::Result<P> {
    let mut r = Reader::open(path)?;
    let mut value = None;
    while let Some(f) = r.next_frame()? {
        if f.kind == MANIFEST {
            continue;
        }
        if value.is_some() {
            return Err(bad("VALUE_COUNT"));
        }
        value = Some(f.typed::<P>()?);
    }
    if !(r.sealed && r.complete) {
        return Err(bad("VALUE_JOURNAL_INCOMPLETE"));
    }
    value.ok_or_else(|| bad("VALUE_COUNT"))
}
/// SHA-256 of a file's bytes, streamed.
pub fn file_sha256(path: &Path) -> io::Result<[u8; 32]> {
    let mut f = File::open(path)?;
    let mut h = solana_sdk::hash::Hasher::default();
    let mut b = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            return Ok(h.result().to_bytes());
        }
        h.hash(&b[..n]);
    }
}
/// Test-only builder of historical-style JSONL views (one value per line) for fixtures of the
/// read-only historical view check; no production path writes a JSON view (JFI-01/03,
/// P0-PROD-CTRL-02). The views appear only complete and together; a damaged journal leaves no
/// file behind. Returns the rows per view.
#[cfg(test)]
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
    // SK-07: a segmented journal is exported across its whole verified chain.
    let mut reader = crate::storage::segments::ChainReader::open(path)?;
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
                    .map(std::io::BufWriter::new)
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
                // Offline JSON export: value-tree frames and typed market frames only.
                serde_json::to_writer(&mut outs[i], &payload::legacy_view(&f)?)
                    .map_err(io::Error::other)?;
                outs[i].write_all(b"\n")?;
                rows[i] += 1;
            }
        }
        // A torn tail lost frames: no view may present the prefix as the journal (H1 keeps such
        // a capture readable and INCOMPLETE for verification, not exportable).
        if reader.tail_truncated() {
            return Err(bad("EXPORT_TORN_TAIL"));
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
