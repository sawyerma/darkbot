//! C1.10 freeze (`c1_freeze`).
//!
//! Binds everything that defines the C1 candidate into one freeze record:
//! the pinned geth base commit, the C1 overlay sources and tool (the patch),
//! the geth binary, the Wire V2 specification, the Rust binaries, the C1
//! configuration, the host/profile evidence, the accepted test inventory,
//! the accepted raw measurement evidence, the exact build commands, the
//! exact non-claims and every tracked native source file.
//!
//! The inputs are a `darkbot.c1.freeze-inputs.v1` record built only through
//! `c1_freeze inputs ...`. The tool refuses to freeze, and reports
//! `INCOMPLETE` with every reason, when an input is missing, when a gate is
//! not locally accepted, when a later gate is accepted before an earlier
//! one, when a template is offered as evidence, when C1.9 has no eligible
//! production report, when a required non-claim is absent, when HEAD or the
//! source binding do not match, or when the working tree is not clean.
//! `READY_TO_FREEZE` only states that the inputs are complete.
//! `IMPLEMENTED_TESTED/FROZEN` is written only with `--freeze`, by the
//! person who performs the freeze, and can be checked again with `--verify`.
//!
//! Existing code and passing code review are not acceptance: every gate
//! entry is a person's statement of local acceptance with its evidence.

use crate::c1_common::{self as common, Args};
use darkbot_bsc_record::schema::c1_qualification::{
    freeze_bindings as fb, freeze_inputs as fi, freeze_record as fr, native_latency as nl,
};
use darkbot_bsc_record::{Draft, Record, Value, decode, observed_now};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const PIN: &str = "cdb7548b5baacfdae92f9f63437d6456411665f3";
/// Qualified strictly in this order; a later gate needs every earlier one.
pub const GATES: [&str; 6] = ["C1.5", "C1.6", "C1.7", "C1.8", "C1.9", "C1.9b"];
/// Evidence of the gates accepted before this tranche.
pub const PRIOR_EVIDENCE: [&str; 5] = [
    "bsc/native/evidence/c1/c1-1-v1-baseline-20261006.json",
    "bsc/native/evidence/c1/c1-2-wire-v2-20261006.json",
    "bsc/native/evidence/c1/c1-3-lane-lifecycle-20261007.json",
    "bsc/native/evidence/c1/c1-4-geth-ingress-20261007.json",
    "bsc/native/evidence/c1/c1-4a-early-latency-20261008.json",
];
pub const ARTIFACTS: [&str; 6] = [
    "geth_binary",
    "wire_v2_spec",
    "overlay_sources",
    "overlay_tool",
    "host_profile_evidence",
    "build_commands",
];
pub const ARTIFACT_LISTS: [&str; 3] = ["rust_binaries", "c1_config", "test_inventory"];
pub const RAW_MEASUREMENT_EVIDENCE: &str = "raw_measurement_evidence";
pub const REQUIRED_NON_CLAIMS: [&str; 4] = [
    "T0_TO_T_BUY_LT_1MS = NOT_PROVEN",
    "MAINNET_INGRESS_COVERAGE = NOT_PROVEN",
    "PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED",
    "LIVE = BLOCKED",
];
/// The non-claims a new inputs record starts with.
pub const DEFAULT_NON_CLAIMS: [&str; 4] = [
    "T0_TO_T_BUY_LT_1MS = NOT_PROVEN",
    "MAINNET_INGRESS_COVERAGE = NOT_PROVEN unless separately qualified",
    "PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED unless separately qualified",
    "LIVE = BLOCKED",
];
pub const DEFAULT_GETH_CHECKOUT: &str = "bsc/vendor/bsc-geth";
/// A file carrying this mark in its first MiB is a template, never evidence.
pub const TEMPLATE_MARK: &[u8] = b"TEMPLATE_NOT_EVIDENCE";
pub const FROZEN: &str = "IMPLEMENTED_TESTED/FROZEN";
pub const READY: &str = "READY_TO_FREEZE";
pub const INCOMPLETE: &str = "INCOMPLETE";
const SOURCE_SCOPE: [&str; 4] = ["bsc/engine", "bsc/geth", "bsc/tools", "bsc/native"];
const SOURCE_ALGORITHM: &str =
    "sha256(mode SP repo_path NUL sha256(file_bytes)), sorted git tree order";
const SOURCE_SCOPE_TEXT: &str =
    "all tracked bsc/engine, bsc/geth, bsc/tools and bsc/native (evidence) files";
/// The bound of the `git status` excerpt in a binding.
const STATUS_EXCERPT_BYTES: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GateEntry {
    pub gate: String,
    pub verdict: String,
    pub accepted_by: Option<String>,
    pub accepted_at_unix_ms: Option<u64>,
    pub statement: Option<String>,
    pub evidence: Vec<String>,
}

impl GateEntry {
    /// A person accepted this gate locally: PASS, who, when and what.
    pub fn accepted(&self) -> bool {
        self.verdict == "PASS"
            && self.accepted_by.as_deref().is_some_and(|by| !by.is_empty())
            && self.accepted_at_unix_ms.is_some()
            && self.statement.as_deref().is_some_and(|s| !s.is_empty())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FreezeInputs {
    pub pinned_geth_commit: String,
    pub geth_checkout: String,
    pub darkbot_commit: Option<String>,
    pub final_c1_source_sha256: Option<[u8; 32]>,
    /// (role, path relative to the repository root).
    pub artifacts: Vec<(String, String)>,
    pub raw_measurement_evidence: Vec<String>,
    pub gates: Vec<GateEntry>,
    pub non_claims: Vec<String>,
}

impl FreezeInputs {
    /// What `inputs init` writes: the pin, the checkout and the non-claims.
    pub fn new(geth_checkout: &str) -> Self {
        Self {
            pinned_geth_commit: PIN.into(),
            geth_checkout: geth_checkout.into(),
            non_claims: DEFAULT_NON_CLAIMS.iter().map(|c| c.to_string()).collect(),
            ..Self::default()
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let record = decode(&fi::SCHEMA, bytes).map_err(|e| e.0)?;
        let e = |e: darkbot_bsc_record::RecordError| e.0;
        let texts = |r: &Record, tag| -> Result<Vec<String>, String> {
            Ok(r.texts(tag)
                .map_err(e)?
                .into_iter()
                .map(String::from)
                .collect())
        };
        let artifacts = record
            .nested_all(fi::ARTIFACTS)
            .map_err(e)?
            .iter()
            .map(|a| {
                Ok((
                    a.text(fi::artifact::ROLE).map_err(e)?.to_string(),
                    a.text(fi::artifact::PATH).map_err(e)?.to_string(),
                ))
            })
            .collect::<Result<_, String>>()?;
        let gates = record
            .nested_all(fi::GATES)
            .map_err(e)?
            .iter()
            .map(|g| {
                use fi::gate as t;
                Ok(GateEntry {
                    gate: g.text(t::GATE).map_err(e)?.into(),
                    verdict: g.text(t::VERDICT).map_err(e)?.into(),
                    accepted_by: g.opt_text(t::ACCEPTED_BY).map_err(e)?.map(String::from),
                    accepted_at_unix_ms: g.opt_u64(t::ACCEPTED_AT_UNIX_MS).map_err(e)?,
                    statement: g.opt_text(t::STATEMENT).map_err(e)?.map(String::from),
                    evidence: texts(g, t::EVIDENCE)?,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            pinned_geth_commit: record.text(fi::PINNED_GETH_COMMIT).map_err(e)?.into(),
            geth_checkout: record.text(fi::GETH_CHECKOUT).map_err(e)?.into(),
            darkbot_commit: record
                .opt_text(fi::DARKBOT_COMMIT)
                .map_err(e)?
                .map(String::from),
            final_c1_source_sha256: record.opt_b256(fi::FINAL_C1_SOURCE_SHA256).map_err(e)?,
            artifacts,
            raw_measurement_evidence: texts(&record, fi::RAW_MEASUREMENT_EVIDENCE)?,
            gates,
            non_claims: texts(&record, fi::NON_CLAIMS)?,
        })
    }

    pub fn to_draft(&self, observed_unix_ms: u64) -> Draft {
        let mut d = Draft::new(&fi::SCHEMA);
        d.set(fi::PINNED_GETH_COMMIT, self.pinned_geth_commit.as_str())
            .set(fi::GETH_CHECKOUT, self.geth_checkout.as_str())
            .set(
                fi::PROVENANCE_TOOL,
                "darkbot-bsc-qualification c1_freeze inputs",
            )
            .set(fi::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set_opt(fi::DARKBOT_COMMIT, self.darkbot_commit.as_deref())
            .set_opt(
                fi::FINAL_C1_SOURCE_SHA256,
                self.final_c1_source_sha256.map(Value::B256),
            );
        for (role, path) in &self.artifacts {
            let mut artifact = Draft::new(&fi::artifact::SCHEMA);
            artifact
                .set(fi::artifact::ROLE, role.as_str())
                .set(fi::artifact::PATH, path.as_str());
            d.push(fi::ARTIFACTS, artifact.into_nested());
        }
        for path in &self.raw_measurement_evidence {
            d.push(fi::RAW_MEASUREMENT_EVIDENCE, path.as_str());
        }
        for gate in &self.gates {
            use fi::gate as t;
            let mut entry = Draft::new(&t::SCHEMA);
            entry
                .set(t::GATE, gate.gate.as_str())
                .set(t::VERDICT, gate.verdict.as_str())
                .set_opt(t::ACCEPTED_BY, gate.accepted_by.as_deref())
                .set_opt(t::ACCEPTED_AT_UNIX_MS, gate.accepted_at_unix_ms)
                .set_opt(t::STATEMENT, gate.statement.as_deref());
            for path in &gate.evidence {
                entry.push(t::EVIDENCE, path.as_str());
            }
            d.push(fi::GATES, entry.into_nested());
        }
        for claim in &self.non_claims {
            d.push(fi::NON_CLAIMS, claim.as_str());
        }
        d
    }

    fn paths(&self, role: &str) -> Vec<&str> {
        self.artifacts
            .iter()
            .filter(|(r, _)| r == role)
            .map(|(_, p)| p.as_str())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Source binding
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBinding {
    pub files: u64,
    pub sha256: [u8; 32],
    pub head_sha256: [u8; 32],
    pub clean: bool,
    pub git_status: String,
}

/// The committed bytes of every blob `oids` names, via one
/// `git cat-file --batch`, in order.
fn committed_blobs(repo: &Path, oids: &[String]) -> Result<Vec<Vec<u8>>, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("git cat-file: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("git cat-file has no stdin")?;
    let requests: String = oids.iter().map(|oid| format!("{oid}\n")).collect();
    let writer = std::thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let stdout = child.stdout.take().ok_or("git cat-file has no stdout")?;
    let mut reader = BufReader::new(stdout);
    let mut blobs = Vec::with_capacity(oids.len());
    for oid in oids {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|e| format!("git cat-file: {e}"))?;
        let fields: Vec<&str> = header.split_whitespace().collect();
        let size = match fields.as_slice() {
            [got, "blob", size] if got == oid => size
                .parse::<usize>()
                .map_err(|_| format!("git cat-file: size {size:?}"))?,
            _ => return Err(format!("git cat-file: {oid} is not a readable blob")),
        };
        let mut blob = vec![0u8; size + 1];
        reader
            .read_exact(&mut blob)
            .map_err(|e| format!("git cat-file: {e}"))?;
        if blob.pop() != Some(b'\n') {
            return Err("git cat-file: malformed batch output".into());
        }
        blobs.push(blob);
    }
    let written = writer.join().map_err(|_| "git cat-file writer failed")?;
    written.map_err(|e| format!("git cat-file: {e}"))?;
    let status = child.wait().map_err(|e| format!("git cat-file: {e}"))?;
    if !status.success() {
        return Err("git cat-file failed".into());
    }
    Ok(blobs)
}

/// Binds every tracked native file (all crates, tools, overlay and build
/// inputs, evidence) and compares each byte to HEAD, independently of Git's
/// stat cache, assume-unchanged or skip-worktree bits. Ignored build outputs
/// are bound separately as binary artifacts. No caller-selected subset.
pub fn source_binding(repo: &Path) -> Result<SourceBinding, String> {
    let root = String::from_utf8(common::git(repo, &["rev-parse", "--show-toplevel"])?)
        .map_err(|_| "Git root is not UTF-8".to_string())?;
    let same = fs::canonicalize(root.trim())
        .ok()
        .zip(fs::canonicalize(repo).ok())
        .is_some_and(|(a, b)| a == b);
    if !same {
        return Err("DarkBot root is not the Git repository root".into());
    }
    let status = String::from_utf8(common::git(
        repo,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?)
    .map_err(|_| "git status is not UTF-8".to_string())?;
    let mut args = vec!["ls-tree", "-r", "-z", "HEAD", "--"];
    args.extend(SOURCE_SCOPE);
    let listing = common::git(repo, &args)?;
    let mut entries = Vec::new();
    for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let tab = entry
            .iter()
            .position(|b| *b == b'\t')
            .ok_or("malformed git ls-tree entry")?;
        let (meta, name) = (&entry[..tab], &entry[tab + 1..]);
        let meta = std::str::from_utf8(meta).map_err(|_| "malformed git ls-tree entry")?;
        let name_text = String::from_utf8_lossy(name).into_owned();
        let parts: Vec<&str> = meta.split(' ').collect();
        let [mode, kind, oid] = parts.as_slice() else {
            return Err("malformed git ls-tree entry".into());
        };
        if *kind != "blob" || (*mode != "100644" && *mode != "100755") {
            return Err(format!("unsupported C1 source entry: {name_text}"));
        }
        entries.push((mode.to_string(), name.to_vec(), name_text, oid.to_string()));
    }
    if entries.is_empty() {
        return Err("final C1 source tree is empty or absent".into());
    }
    let oids: Vec<String> = entries.iter().map(|e| e.3.clone()).collect();
    let originals = committed_blobs(repo, &oids)?;
    let mut actual = ring::digest::Context::new(&ring::digest::SHA256);
    let mut committed = ring::digest::Context::new(&ring::digest::SHA256);
    for ((mode, name, name_text, _), original) in entries.iter().zip(&originals) {
        let path = repo.join(name_text);
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("{name_text}: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!("C1 source symlink: {name_text}"));
        }
        let data = fs::read(&path).map_err(|e| format!("{name_text}: {e}"))?;
        for (digest, content) in [(&mut actual, &data), (&mut committed, original)] {
            digest.update(mode.as_bytes());
            digest.update(b" ");
            digest.update(name);
            digest.update(b"\0");
            digest.update(&common::sha256(content));
        }
    }
    let finish = |context: ring::digest::Context| {
        let mut out = [0u8; 32];
        out.copy_from_slice(context.finish().as_ref());
        out
    };
    Ok(SourceBinding {
        files: entries.len() as u64,
        sha256: finish(actual),
        head_sha256: finish(committed),
        clean: status.is_empty(),
        git_status: status,
    })
}

impl SourceBinding {
    fn to_value(&self) -> Value {
        use fb::source as s;
        let mut cut = self.git_status.len().min(STATUS_EXCERPT_BYTES);
        while !self.git_status.is_char_boundary(cut) {
            cut -= 1;
        }
        let excerpt = &self.git_status[..cut];
        let mut d = Draft::new(&s::SCHEMA);
        d.set(s::ALGORITHM, SOURCE_ALGORITHM)
            .set(s::SCOPE, SOURCE_SCOPE_TEXT)
            .set(s::FILES, self.files)
            .set(s::SHA256, Value::B256(self.sha256))
            .set(s::HEAD_SHA256, Value::B256(self.head_sha256))
            .set(s::WORKING_TREE, if self.clean { "clean" } else { "dirty" })
            .set(s::GIT_STATUS, excerpt)
            .set(
                s::GIT_STATUS_SHA256,
                Value::B256(common::sha256(self.git_status.as_bytes())),
            )
            .set(
                s::GIT_STATUS_ENTRIES,
                self.git_status.lines().count() as u64,
            )
            .set(s::GIT_STATUS_TRUNCATED, cut < self.git_status.len());
        d.into_nested()
    }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

fn is_template(path: &Path) -> std::io::Result<bool> {
    if path.is_dir() {
        return Ok(false);
    }
    let mut head = Vec::new();
    File::open(path)?.take(1 << 20).read_to_end(&mut head)?;
    Ok(head
        .windows(TEMPLATE_MARK.len())
        .any(|window| window == TEMPLATE_MARK))
}

/// An eligible C1.9 report: a native-latency record of the production
/// devp2p handoff mode, integrity PASS, qualification eligible.
pub fn is_eligible_c19_report(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(record) = decode(&nl::SCHEMA, &bytes) else {
        return false;
    };
    record.text(nl::OUTBOUND_MODE).ok() == Some(crate::c1_latency::PRODUCTION_MODE)
        && record.text(nl::INTEGRITY).ok() == Some("PASS")
        && record.bool(nl::QUALIFICATION_ELIGIBLE).ok() == Some(true)
}

struct Binder<'a> {
    repo: &'a Path,
    missing: Vec<String>,
}

impl Binder<'_> {
    /// Hashes one input; records why it cannot be bound.
    fn bind(&mut self, label: &str, relative: Option<&str>) -> Option<[u8; 32]> {
        let Some(relative) = relative.filter(|r| !r.is_empty()) else {
            self.missing.push(format!("{label}: not given"));
            return None;
        };
        let path = self.repo.join(relative);
        if !path.exists() {
            self.missing
                .push(format!("{label}: {relative} does not exist"));
            return None;
        }
        match is_template(&path) {
            Ok(false) => {}
            Ok(true) => {
                self.missing
                    .push(format!("{label}: {relative} is a template, not evidence"));
                return None;
            }
            Err(e) => {
                self.missing
                    .push(format!("{label}: {relative} cannot be read: {e}"));
                return None;
            }
        }
        match common::sha256_path(&path) {
            Ok(digest) => Some(digest),
            Err(e) => {
                self.missing
                    .push(format!("{label}: {relative} cannot be hashed: {e}"));
                None
            }
        }
    }

    fn file(role: &str, path: Option<&str>, digest: Option<[u8; 32]>) -> Value {
        let mut d = Draft::new(&fb::file::SCHEMA);
        d.set(fb::file::ROLE, role)
            .set_opt(fb::file::PATH, path)
            .set_opt(fb::file::SHA256, digest.map(Value::B256));
        d.into_nested()
    }

    /// Binds a list of inputs; an empty list is missing.
    fn bind_list(
        &mut self,
        label: &str,
        role: &str,
        paths: &[&str],
    ) -> Vec<(Value, Option<String>)> {
        if paths.is_empty() {
            self.missing.push(format!("{label}: no file given"));
        }
        paths
            .iter()
            .map(|path| {
                let digest = self.bind(label, Some(path));
                let bound = digest.map(|_| path.to_string());
                (Self::file(role, Some(path), digest), bound)
            })
            .collect()
    }
}

/// The freeze evaluation: the bindings and every reason it is incomplete.
#[derive(Clone, Debug)]
pub struct Evaluation {
    pub missing: Vec<String>,
    /// A `darkbot.c1.freeze-bindings.v1` draft.
    pub bindings: Draft,
    pub bindings_sha256: [u8; 32],
}

impl Evaluation {
    pub fn status(&self) -> &'static str {
        if self.missing.is_empty() {
            READY
        } else {
            INCOMPLETE
        }
    }
}

pub type GethHead<'a> = &'a dyn Fn(&Path) -> Option<String>;

pub fn evaluate(
    inputs: &FreezeInputs,
    repo: &Path,
    geth_head: GethHead<'_>,
) -> Result<Evaluation, String> {
    let mut b = Binder {
        repo,
        missing: Vec::new(),
    };
    let mut bindings = Draft::new(&fb::SCHEMA);
    if inputs.pinned_geth_commit != PIN {
        b.missing.push(format!("pinned_geth_commit is not {PIN}"));
    }
    let checkout = &inputs.geth_checkout;
    let observed = (!checkout.is_empty())
        .then(|| geth_head(&repo.join(checkout)))
        .flatten();
    if observed.as_deref() != Some(PIN) {
        b.missing
            .push(format!("geth checkout HEAD is {observed:?}, not the pin"));
    }
    bindings.set(fb::PINNED_GETH_COMMIT, PIN);
    let head = common::git_head(repo);
    bindings.set_opt(fb::DARKBOT_COMMIT, head.as_deref());
    if head.is_none() || inputs.darkbot_commit != head {
        b.missing
            .push("darkbot_commit: missing HEAD or exact expected commit mismatch".into());
    }
    match source_binding(repo) {
        Ok(source) => {
            bindings.set(fb::FINAL_C1_SOURCE, source.to_value());
            if !source.clean {
                b.missing.push("DarkBot working tree is dirty".into());
            }
            if source.sha256 != source.head_sha256 {
                b.missing
                    .push("final C1 source differs from committed source".into());
            }
            if inputs.final_c1_source_sha256 != Some(source.sha256) {
                b.missing
                    .push("final C1 source hash missing or mismatch".into());
            }
        }
        Err(e) => b
            .missing
            .push(format!("final C1 source/working tree cannot be bound: {e}")),
    }

    for (role, _) in &inputs.artifacts {
        if !ARTIFACTS.contains(&role.as_str()) && !ARTIFACT_LISTS.contains(&role.as_str()) {
            b.missing.push(format!("artifact role {role} is unknown"));
        }
    }
    for name in ARTIFACTS {
        let paths = inputs.paths(name);
        if paths.len() > 1 {
            b.missing.push(format!("{name}: more than one path given"));
        }
        let path = paths.last().copied();
        let digest = b.bind(name, path);
        bindings.push(fb::ARTIFACTS, Binder::file(name, path, digest));
    }
    for name in ARTIFACT_LISTS {
        for (value, _) in b.bind_list(name, name, &inputs.paths(name)) {
            bindings.push(fb::ARTIFACTS, value);
        }
    }
    for path in PRIOR_EVIDENCE {
        let digest = b.bind("prior gate evidence", Some(path));
        bindings.push(
            fb::PRIOR_GATE_EVIDENCE,
            Binder::file("prior_gate_evidence", Some(path), digest),
        );
    }
    let raw: Vec<&str> = inputs
        .raw_measurement_evidence
        .iter()
        .map(String::as_str)
        .collect();
    for (value, _) in b.bind_list(RAW_MEASUREMENT_EVIDENCE, RAW_MEASUREMENT_EVIDENCE, &raw) {
        bindings.push(fb::RAW_MEASUREMENT_EVIDENCE, value);
    }

    let mut gates: BTreeMap<&str, &GateEntry> = BTreeMap::new();
    for entry in &inputs.gates {
        if !GATES.contains(&entry.gate.as_str()) {
            b.missing.push(format!("unknown gate {}", entry.gate));
        } else if gates.insert(entry.gate.as_str(), entry).is_some() {
            b.missing
                .push(format!("{}: more than one acceptance entry", entry.gate));
        }
    }
    let mut blocked_by: Option<&str> = None;
    for gate in GATES {
        let Some(entry) = gates.get(gate) else {
            b.missing.push(format!("{gate}: no acceptance entry"));
            blocked_by = blocked_by.or(Some(gate));
            continue;
        };
        let accepted = entry.accepted();
        if !accepted {
            b.missing.push(format!(
                "{gate}: not locally accepted (verdict {:?})",
                entry.verdict
            ));
        } else if let Some(earlier) = blocked_by {
            b.missing.push(format!(
                "{gate}: accepted before {earlier}; gates are accepted in order"
            ));
        }
        let paths: Vec<&str> = entry.evidence.iter().map(String::as_str).collect();
        let evidence = b.bind_list(&format!("{gate} evidence"), "evidence", &paths);
        if gate == "C1.9"
            && !evidence
                .iter()
                .filter_map(|(_, bound)| bound.as_deref())
                .any(|path| is_eligible_c19_report(&repo.join(path)))
        {
            b.missing
                .push("C1.9: missing eligible production devp2p handoff report".into());
        }
        use fb::gate as t;
        let mut binding = Draft::new(&t::SCHEMA);
        binding
            .set(t::GATE, gate)
            .set(t::VERDICT, entry.verdict.as_str())
            .set_opt(t::ACCEPTED_BY, entry.accepted_by.as_deref())
            .set_opt(t::ACCEPTED_AT_UNIX_MS, entry.accepted_at_unix_ms)
            .set_opt(t::STATEMENT, entry.statement.as_deref());
        for (value, _) in evidence {
            binding.push(t::EVIDENCE, value);
        }
        bindings.push(fb::GATES, binding.into_nested());
        if !accepted {
            blocked_by = blocked_by.or(Some(gate));
        }
    }

    for claim in REQUIRED_NON_CLAIMS {
        if !inputs.non_claims.iter().any(|line| line.starts_with(claim)) {
            b.missing.push(format!("non-claim missing: {claim}"));
        }
    }
    for claim in &inputs.non_claims {
        bindings.push(fb::NON_CLAIMS, claim.as_str());
    }
    let encoded = bindings.encode().map_err(|e| format!("bindings: {e}"))?;
    Ok(Evaluation {
        missing: b.missing,
        bindings_sha256: common::sha256(&encoded),
        bindings,
    })
}

/// The freeze record of an evaluation with `status`.
pub fn record(evaluation: &Evaluation, status: &str, observed_unix_ms: u64) -> Draft {
    let mut d = Draft::new(&fr::SCHEMA);
    d.set(fr::STATUS, status)
        .set(fr::PROVENANCE_TOOL, "darkbot-bsc-qualification c1_freeze")
        .set(fr::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
        .set(fr::BINDINGS, evaluation.bindings.clone().into_nested())
        .set(fr::BINDINGS_SHA256, Value::B256(evaluation.bindings_sha256));
    for reason in &evaluation.missing {
        d.push(fr::MISSING, reason.as_str());
    }
    d
}

/// The SHA-256 of a freeze record's own bindings, recomputed.
pub fn recorded_bindings_sha256(frozen: &Record) -> Result<[u8; 32], String> {
    let nested = frozen.nested(fr::BINDINGS).map_err(|e| e.0)?;
    let mut draft = Draft::new(&fb::SCHEMA);
    for (tag, value) in nested.fields() {
        draft.push(*tag, value.clone());
    }
    let bytes = draft.encode().map_err(|e| e.0)?;
    Ok(common::sha256(&bytes))
}

// ---------------------------------------------------------------------------
// The command line
// ---------------------------------------------------------------------------

const USAGE: &str = "usage: c1_freeze --repo DIR --inputs R [--output R] [--freeze]\n\
       c1_freeze --repo DIR --inputs R --verify R\n\
       c1_freeze inputs init --out R [--geth-checkout PATH]\n\
       c1_freeze inputs set-artifact ROLE --inputs R --path P...\n\
       c1_freeze inputs accept-gate GATE --inputs R --repo DIR --accepted-by NAME --statement TEXT --evidence P...\n\
       c1_freeze inputs set-commit --inputs R --repo DIR";

fn read_inputs(path: &Path) -> Result<FreezeInputs, String> {
    FreezeInputs::decode(&common::read(path)?).map_err(|e| format!("{}: {e}", path.display()))
}

fn save_inputs(path: &Path, inputs: &FreezeInputs) -> Result<(), String> {
    let bytes = inputs.to_draft(observed_now()?).encode().map_err(|e| e.0)?;
    common::write_replace(path, &bytes)
}

/// `c1_freeze inputs ...`: the only way the inputs record changes.
fn inputs_command(args: &Args) -> Result<String, String> {
    let sub = args.positional.get(1).map(String::as_str);
    match (sub, args.positional.len()) {
        (Some("init"), 2) => {
            args.only(&["out", "geth-checkout"])?;
            let out = PathBuf::from(args.one("out")?);
            let checkout = args.opt("geth-checkout")?.unwrap_or(DEFAULT_GETH_CHECKOUT);
            let inputs = FreezeInputs::new(checkout);
            let bytes = inputs.to_draft(observed_now()?).encode().map_err(|e| e.0)?;
            common::write_new(&out, &bytes)?;
            Ok(format!("inputs={} (nothing accepted yet)", out.display()))
        }
        (Some("set-artifact"), 3) => {
            args.only(&["inputs", "path"])?;
            let role = args.positional[2].as_str();
            let paths = args.many("path");
            let known = ARTIFACTS.contains(&role)
                || ARTIFACT_LISTS.contains(&role)
                || role == RAW_MEASUREMENT_EVIDENCE;
            if !known {
                return Err(format!("unknown artifact role {role}"));
            }
            if paths.is_empty() || (ARTIFACTS.contains(&role) && paths.len() != 1) {
                return Err(format!(
                    "{role} takes {} --path",
                    if ARTIFACTS.contains(&role) {
                        "exactly one"
                    } else {
                        "at least one"
                    }
                ));
            }
            let path = PathBuf::from(args.one("inputs")?);
            let mut inputs = read_inputs(&path)?;
            if role == RAW_MEASUREMENT_EVIDENCE {
                inputs.raw_measurement_evidence = paths.iter().map(|p| p.to_string()).collect();
            } else {
                inputs.artifacts.retain(|(r, _)| r != role);
                inputs
                    .artifacts
                    .extend(paths.iter().map(|p| (role.to_string(), p.to_string())));
            }
            save_inputs(&path, &inputs)?;
            Ok(format!("{role}: {} path(s) set", paths.len()))
        }
        (Some("accept-gate"), 3) => {
            args.only(&["inputs", "repo", "accepted-by", "statement", "evidence"])?;
            let gate = args.positional[2].as_str();
            let position = GATES
                .iter()
                .position(|g| *g == gate)
                .ok_or_else(|| format!("unknown gate {gate}"))?;
            let accepted_by = args.one("accepted-by")?;
            let statement = args.one("statement")?;
            if accepted_by.is_empty() || statement.is_empty() {
                return Err("an acceptance names who accepted it and what was accepted".into());
            }
            let evidence = args.many("evidence");
            if evidence.is_empty() {
                return Err(format!("{gate}: no evidence given"));
            }
            let repo = PathBuf::from(args.one("repo")?);
            let path = PathBuf::from(args.one("inputs")?);
            let mut inputs = read_inputs(&path)?;
            for earlier in &GATES[..position] {
                let ok = inputs
                    .gates
                    .iter()
                    .any(|entry| entry.gate == *earlier && entry.accepted());
                if !ok {
                    return Err(format!(
                        "{earlier} is not accepted; gates are accepted in order"
                    ));
                }
            }
            let mut binder = Binder {
                repo: &repo,
                missing: Vec::new(),
            };
            for file in &evidence {
                binder.bind(&format!("{gate} evidence"), Some(file));
            }
            if let Some(problem) = binder.missing.first() {
                return Err(problem.clone());
            }
            if gate == "C1.9"
                && !evidence
                    .iter()
                    .any(|f| is_eligible_c19_report(&repo.join(f)))
            {
                return Err("C1.9: missing eligible production devp2p handoff report".into());
            }
            inputs.gates.retain(|entry| entry.gate != gate);
            inputs.gates.push(GateEntry {
                gate: gate.into(),
                verdict: "PASS".into(),
                accepted_by: Some(accepted_by.into()),
                accepted_at_unix_ms: Some(observed_now()?),
                statement: Some(statement.into()),
                evidence: evidence.iter().map(|f| f.to_string()).collect(),
            });
            inputs
                .gates
                .sort_by_key(|entry| GATES.iter().position(|g| *g == entry.gate));
            save_inputs(&path, &inputs)?;
            Ok(format!("{gate}: accepted by {accepted_by}"))
        }
        (Some("set-commit"), 2) => {
            args.only(&["inputs", "repo"])?;
            let repo = PathBuf::from(args.one("repo")?);
            let path = PathBuf::from(args.one("inputs")?);
            let mut inputs = read_inputs(&path)?;
            let head = common::git_head(&repo).ok_or("the repository has no HEAD")?;
            let source = source_binding(&repo)?;
            if !source.clean {
                return Err("the working tree is dirty: commit the final sources first".into());
            }
            if source.sha256 != source.head_sha256 {
                return Err("the source differs from HEAD".into());
            }
            inputs.darkbot_commit = Some(head.clone());
            inputs.final_c1_source_sha256 = Some(source.sha256);
            save_inputs(&path, &inputs)?;
            Ok(format!(
                "darkbot_commit={head} final_c1_source_sha256={} files={}",
                common::hex(&source.sha256),
                source.files
            ))
        }
        _ => Err("inputs takes init, set-artifact ROLE, accept-gate GATE or set-commit".into()),
    }
}

/// The derived text view of an evaluation.
pub fn summary(evaluation: &Evaluation, status: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "C1.10 freeze: status={status} missing={} bindings_sha256={}",
        evaluation.missing.len(),
        common::hex(&evaluation.bindings_sha256)
    );
    for reason in &evaluation.missing {
        let _ = writeln!(out, "  missing: {reason}");
    }
    out
}

/// The command line. Exit 0: complete (or frozen, or verified); 2:
/// incomplete or refused; 3: verification failed; 64: usage.
pub fn main_with(args: Args, geth_head: GethHead<'_>) -> u8 {
    if args.positional.first().map(String::as_str) == Some("inputs") {
        return match inputs_command(&args) {
            Ok(done) => {
                println!("{done}");
                0
            }
            Err(problem) => {
                eprintln!("C1_10_INPUTS_REFUSED {problem}");
                2
            }
        };
    }
    let parsed = (|| {
        args.only(&["repo", "inputs", "output", "freeze", "verify"])?;
        if !args.positional.is_empty() {
            return Err("unexpected positional argument".to_string());
        }
        let freeze = args.switch("freeze")?;
        let verify = args.opt("verify")?.map(PathBuf::from);
        let output = args.opt("output")?.map(PathBuf::from);
        if verify.is_some() && (freeze || output.is_some()) {
            return Err("--verify takes neither --freeze nor --output".to_string());
        }
        Ok((
            PathBuf::from(args.one("repo")?),
            PathBuf::from(args.one("inputs")?),
            output,
            freeze,
            verify,
        ))
    })();
    let (repo, inputs_path, output, freeze, verify) = match parsed {
        Ok(parsed) => parsed,
        Err(problem) => {
            eprintln!("c1_freeze: {problem}\n{USAGE}");
            return 64;
        }
    };
    let repo = fs::canonicalize(&repo).unwrap_or(repo);
    let evaluation = match read_inputs(&inputs_path).and_then(|i| evaluate(&i, &repo, geth_head)) {
        Ok(evaluation) => evaluation,
        Err(problem) => {
            eprintln!("C1_10_INCOMPLETE inputs cannot be evaluated: {problem}");
            return if verify.is_some() { 3 } else { 2 };
        }
    };

    if let Some(path) = verify {
        let frozen =
            common::read(&path).and_then(|bytes| decode(&fr::SCHEMA, &bytes).map_err(|e| e.0));
        let frozen = match frozen {
            Ok(frozen) => frozen,
            Err(problem) => {
                eprintln!("C1_10_VERIFY_FAIL {problem}");
                return 3;
            }
        };
        if frozen.text(fr::STATUS).ok() != Some(FROZEN) {
            eprintln!("C1_10_VERIFY_FAIL the record is not a frozen record");
            return 3;
        }
        let recorded = frozen.b256(fr::BINDINGS_SHA256).ok();
        let recomputed = recorded_bindings_sha256(&frozen).ok();
        if !evaluation.missing.is_empty()
            || recomputed.is_none()
            || recomputed != recorded
            || recorded != Some(evaluation.bindings_sha256)
        {
            for reason in &evaluation.missing {
                eprintln!("C1_10_VERIFY_FAIL {reason}");
            }
            eprintln!("C1_10_VERIFY_FAIL the files no longer match the frozen record");
            return 3;
        }
        println!(
            "C1_10_FROZEN_RECORD_VERIFIED {}",
            common::hex(&evaluation.bindings_sha256)
        );
        return 0;
    }

    let mut status = evaluation.status();
    if !evaluation.missing.is_empty() {
        for reason in &evaluation.missing {
            eprintln!("C1_10_INCOMPLETE {reason}");
        }
        if freeze {
            eprintln!("C1_10_FREEZE_REFUSED");
        }
    } else if freeze {
        status = FROZEN;
    }
    if let Some(output) = output {
        let written = observed_now().and_then(|now| {
            let bytes = record(&evaluation, status, now).encode().map_err(|e| e.0)?;
            common::write_new(&output, &bytes)
        });
        if let Err(problem) = written {
            eprintln!("C1_10_RECORD_NOT_WRITTEN {problem}");
            return 2;
        }
    }
    print!("{}", summary(&evaluation, status));
    println!(
        "status={status} bindings_sha256={}",
        common::hex(&evaluation.bindings_sha256)
    );
    if evaluation.missing.is_empty() { 0 } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Ported from bsc/tools/test_c1_qualification_tools.py (FreezeTest):
    // a synthetic Git repository per test, no real evidence.

    static FIXTURES: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        repo: PathBuf,
        head: String,
        source: [u8; 32],
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.repo);
        }
    }

    fn git_in(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    impl Fixture {
        fn new() -> Self {
            let id = FIXTURES.fetch_add(1, Ordering::Relaxed);
            let base = std::env::temp_dir().join(format!("c1-freeze-{}-{id}", std::process::id()));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(&base).unwrap();
            let repo = fs::canonicalize(&base).unwrap();
            let put = |relative: &str, bytes: &[u8]| {
                let path = repo.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            };
            for relative in PRIOR_EVIDENCE {
                put(relative, b"{}");
            }
            for name in [
                "geth",
                "spec.md",
                "tool.py",
                "host.dbrec",
                "build.txt",
                "rust-a",
                "config.env",
                "tests.log",
                "raw.dbtab",
                "e5.dbrec",
                "e6.dbrec",
                "e7.dbrec",
                "e8.dbrec",
                "e9b.dbrec",
            ] {
                put(&format!("in/{name}"), name.as_bytes());
            }
            put(
                "in/e9.dbrec",
                &crate::c1_latency::tests::report_bytes(crate::c1_latency::PRODUCTION_MODE, true),
            );
            put("in/overlay/v2.go", b"package fastlane");
            put("bsc/native/crates/test/src/lib.rs", b"// synthetic source");
            put(
                ".gitignore",
                b"/inputs.dbrec\n/record*.dbrec\n/.inputs.dbrec.tmp-*\n",
            );
            git_in(&repo, &["init", "-q"]);
            git_in(&repo, &["add", "."]);
            git_in(
                &repo,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                ],
            );
            let head = common::git_head(&repo).unwrap();
            let source = source_binding(&repo).unwrap().sha256;
            Self { repo, head, source }
        }

        fn put(&self, relative: &str, bytes: &[u8]) {
            fs::write(self.repo.join(relative), bytes).unwrap();
        }

        fn inputs(&self) -> FreezeInputs {
            let gate = |name: &str, evidence: &str| GateEntry {
                gate: name.into(),
                verdict: "PASS".into(),
                accepted_by: Some("reviewer".into()),
                accepted_at_unix_ms: Some(1_792_454_400_000),
                statement: Some("accepted locally".into()),
                evidence: vec![format!("in/{evidence}")],
            };
            let artifacts = [
                ("geth_binary", "in/geth"),
                ("wire_v2_spec", "in/spec.md"),
                ("overlay_sources", "in/overlay"),
                ("overlay_tool", "in/tool.py"),
                ("host_profile_evidence", "in/host.dbrec"),
                ("build_commands", "in/build.txt"),
                ("rust_binaries", "in/rust-a"),
                ("c1_config", "in/config.env"),
                ("test_inventory", "in/tests.log"),
            ];
            FreezeInputs {
                pinned_geth_commit: PIN.into(),
                geth_checkout: "vendor".into(),
                darkbot_commit: Some(self.head.clone()),
                final_c1_source_sha256: Some(self.source),
                artifacts: artifacts
                    .iter()
                    .map(|(r, p)| (r.to_string(), p.to_string()))
                    .collect(),
                raw_measurement_evidence: vec!["in/raw.dbtab".into()],
                gates: vec![
                    gate("C1.5", "e5.dbrec"),
                    gate("C1.6", "e6.dbrec"),
                    gate("C1.7", "e7.dbrec"),
                    gate("C1.8", "e8.dbrec"),
                    gate("C1.9", "e9.dbrec"),
                    gate("C1.9b", "e9b.dbrec"),
                ],
                non_claims: REQUIRED_NON_CLAIMS.iter().map(|c| c.to_string()).collect(),
            }
        }

        fn evaluate_with(&self, inputs: &FreezeInputs, head: &str) -> Evaluation {
            let head = head.to_string();
            evaluate(inputs, &self.repo, &move |_| Some(head.clone())).unwrap()
        }

        fn evaluate(&self, inputs: &FreezeInputs) -> Evaluation {
            self.evaluate_with(inputs, PIN)
        }
    }

    fn has(evaluation: &Evaluation, needle: &str) -> bool {
        evaluation.missing.iter().any(|m| m.contains(needle))
    }

    #[test]
    fn source_binding_refuses_missing_head_dirty_and_missing_hash() {
        let f = Fixture::new();
        let inputs = f.inputs();
        for broken in [
            FreezeInputs {
                darkbot_commit: None,
                ..inputs.clone()
            },
            FreezeInputs {
                final_c1_source_sha256: None,
                ..inputs.clone()
            },
            FreezeInputs {
                final_c1_source_sha256: Some([0; 32]),
                ..inputs.clone()
            },
        ] {
            assert_eq!(f.evaluate(&broken).status(), INCOMPLETE);
        }
        f.put("untracked.rs", b"// unbound source");
        assert!(has(&f.evaluate(&inputs), "dirty"));
        fs::remove_file(f.repo.join("untracked.rs")).unwrap();
        fs::rename(f.repo.join(".git"), f.repo.join("saved-git")).unwrap();
        assert_eq!(f.evaluate(&inputs).status(), INCOMPLETE);
    }

    #[test]
    fn source_change_detected_even_if_git_is_told_to_ignore_it() {
        let f = Fixture::new();
        let path = "bsc/native/crates/test/src/lib.rs";
        git_in(&f.repo, &["update-index", "--assume-unchanged", path]);
        f.put(path, b"// modified despite identical HEAD");
        assert_eq!(common::git_head(&f.repo).as_deref(), Some(f.head.as_str()));
        let evaluation = f.evaluate(&f.inputs());
        assert!(has(&evaluation, "differs from committed"));
        assert!(has(&evaluation, "hash missing or mismatch"));
    }

    #[test]
    fn complete_inputs_are_ready_and_bind_every_artifact() {
        let f = Fixture::new();
        let evaluation = f.evaluate(&f.inputs());
        assert_eq!(evaluation.missing, Vec::<String>::new());
        assert_eq!(evaluation.status(), READY);
        let bytes = evaluation.bindings.encode().unwrap();
        let bindings = decode(&fb::SCHEMA, &bytes).unwrap();
        assert_eq!(bindings.text(fb::PINNED_GETH_COMMIT).unwrap(), PIN);
        assert_eq!(
            bindings.nested_all(fb::PRIOR_GATE_EVIDENCE).unwrap().len(),
            5
        );
        let gates = bindings.nested_all(fb::GATES).unwrap();
        let names: Vec<_> = gates
            .iter()
            .map(|g| g.text(fb::gate::GATE).unwrap())
            .collect();
        assert_eq!(names, GATES);
        let artifacts = bindings.nested_all(fb::ARTIFACTS).unwrap();
        for name in ARTIFACTS {
            let artifact = artifacts
                .iter()
                .find(|a| a.text(fb::file::ROLE).unwrap() == name)
                .unwrap();
            assert!(
                artifact.opt_b256(fb::file::SHA256).unwrap().is_some(),
                "{name}"
            );
        }
        // The evaluation never calls itself frozen on its own.
        assert_ne!(evaluation.status(), FROZEN);
    }

    #[test]
    fn the_binding_changes_with_any_bound_file() {
        let f = Fixture::new();
        let before = f.evaluate(&f.inputs()).bindings_sha256;
        f.put("in/raw.dbtab", b"another measurement");
        assert_ne!(f.evaluate(&f.inputs()).bindings_sha256, before);
        f.put("in/raw.dbtab", b"raw.dbtab");
        assert_eq!(f.evaluate(&f.inputs()).bindings_sha256, before);
        f.put("in/overlay/extra.go", b"package fastlane");
        assert_ne!(f.evaluate(&f.inputs()).bindings_sha256, before);
    }

    #[test]
    fn missing_or_unaccepted_inputs_leave_it_incomplete() {
        let f = Fixture::new();
        let mut cases: Vec<(&str, FreezeInputs)> = Vec::new();
        let mut missing_file = f.inputs();
        missing_file.artifacts[0].1 = "in/absent".into();
        cases.push(("does not exist", missing_file));
        let mut no_gate = f.inputs();
        no_gate.gates.pop();
        cases.push(("C1.9b: no acceptance entry", no_gate));
        let mut failed = f.inputs();
        failed.gates[3].verdict = "FAIL".into();
        cases.push(("C1.8: not locally accepted", failed));
        let mut unsigned = f.inputs();
        unsigned.gates[0].accepted_by = Some(String::new());
        cases.push(("C1.5: not locally accepted", unsigned));
        let mut unstated = f.inputs();
        unstated.gates[2].statement = None;
        cases.push(("C1.7: not locally accepted", unstated));
        let mut no_evidence = f.inputs();
        no_evidence.gates[1].evidence.clear();
        cases.push(("C1.6 evidence: no file given", no_evidence));
        let mut no_raw = f.inputs();
        no_raw.raw_measurement_evidence.clear();
        cases.push(("raw_measurement_evidence: no file given", no_raw));
        let mut weak = f.inputs();
        weak.non_claims.remove(0);
        cases.push(("non-claim missing: T0_TO_T_BUY_LT_1MS", weak));
        let mut other_pin = f.inputs();
        other_pin.pinned_geth_commit = "0".repeat(40);
        cases.push(("pinned_geth_commit is not", other_pin));
        let mut twice = f.inputs();
        twice.gates.push(twice.gates[0].clone());
        cases.push(("C1.5: more than one acceptance entry", twice));
        for (expected, inputs) in cases {
            let evaluation = f.evaluate(&inputs);
            assert_eq!(evaluation.status(), INCOMPLETE, "{expected}");
            assert!(
                has(&evaluation, expected),
                "{expected}: {:?}",
                evaluation.missing
            );
        }
        let wrong_head = f.evaluate_with(&f.inputs(), &"1".repeat(40));
        assert!(has(&wrong_head, "geth checkout HEAD"));
    }

    #[test]
    fn a_later_gate_cannot_be_accepted_before_an_earlier_one() {
        let f = Fixture::new();
        let mut inputs = f.inputs();
        inputs.gates[1].verdict = "PENDING".into();
        let evaluation = f.evaluate(&inputs);
        assert_eq!(evaluation.status(), INCOMPLETE);
        assert!(has(&evaluation, "C1.6: not locally accepted"));
        for later in ["C1.7", "C1.8", "C1.9", "C1.9b"] {
            assert!(has(&evaluation, &format!("{later}: accepted before C1.6")));
        }
    }

    #[test]
    fn loopback_cannot_qualify_c19_for_freeze() {
        let f = Fixture::new();
        f.put(
            "in/e9.dbrec",
            &crate::c1_latency::tests::report_bytes(crate::c1_latency::LOOPBACK_MODE, false),
        );
        assert!(has(&f.evaluate(&f.inputs()), "eligible production devp2p"));
        // A legacy JSON report is not a native-latency record either.
        f.put("in/e9.dbrec", br#"{"c1_9_qualification_eligible": true}"#);
        assert!(has(&f.evaluate(&f.inputs()), "eligible production devp2p"));
    }

    #[test]
    fn a_template_is_never_evidence() {
        let f = Fixture::new();
        f.put("in/e9.dbrec", br#"{"status": "TEMPLATE_NOT_EVIDENCE"}"#);
        let evaluation = f.evaluate(&f.inputs());
        assert_eq!(evaluation.status(), INCOMPLETE);
        assert!(has(&evaluation, "is a template, not evidence"));
    }

    /// The command line with a geth checkout `vendor` at the pin.
    fn run(_: &Fixture, tokens: &[&str]) -> u8 {
        let args = Args::parse(tokens.iter().map(|t| t.to_string()));
        main_with(args, &|path: &Path| {
            if path.ends_with("vendor") {
                Some(PIN.to_string())
            } else {
                common::git_head(path)
            }
        })
    }

    #[test]
    fn the_command_line_freezes_only_complete_inputs_and_verifies_them() {
        let f = Fixture::new();
        let repo = f.repo.to_str().unwrap().to_string();
        let inputs = f.repo.join("inputs.dbrec");
        let inputs_s = inputs.to_str().unwrap().to_string();
        let record = |n: u32| f.repo.join(format!("record-{n}.dbrec"));
        let write_inputs = |value: &FreezeInputs| {
            let _ = fs::remove_file(&inputs);
            fs::write(&inputs, value.to_draft(1).encode().unwrap()).unwrap();
        };
        let evaluate = |extra: &[&str]| {
            let mut tokens = vec!["--repo", &repo, "--inputs", &inputs_s];
            tokens.extend_from_slice(extra);
            run(&f, &tokens)
        };
        let status_of = |path: &Path| {
            let record = decode(&fr::SCHEMA, &fs::read(path).unwrap()).unwrap();
            record.text(fr::STATUS).unwrap().to_string()
        };

        let mut incomplete = f.inputs();
        incomplete.gates[5].verdict = "PENDING".into();
        write_inputs(&incomplete);
        let r1 = record(1);
        assert_eq!(evaluate(&["--freeze", "--output", r1.to_str().unwrap()]), 2);
        assert_eq!(status_of(&r1), INCOMPLETE);

        write_inputs(&f.inputs());
        let r2 = record(2);
        assert_eq!(evaluate(&["--output", r2.to_str().unwrap()]), 0);
        assert_eq!(status_of(&r2), READY);
        assert_eq!(evaluate(&["--verify", r2.to_str().unwrap()]), 3);

        let frozen = record(3);
        let frozen_s = frozen.to_str().unwrap().to_string();
        assert_eq!(evaluate(&["--freeze", "--output", &frozen_s]), 0);
        assert_eq!(status_of(&frozen), FROZEN);
        assert_eq!(evaluate(&["--verify", &frozen_s]), 0);
        // An existing record is never overwritten.
        assert_eq!(evaluate(&["--freeze", "--output", &frozen_s]), 2);
        // Every source refusal also applies to --verify.
        let good = f.inputs();
        for bad in [
            FreezeInputs {
                darkbot_commit: None,
                ..good.clone()
            },
            FreezeInputs {
                final_c1_source_sha256: None,
                ..good.clone()
            },
            FreezeInputs {
                final_c1_source_sha256: Some([0; 32]),
                ..good.clone()
            },
        ] {
            write_inputs(&bad);
            assert_eq!(evaluate(&["--verify", &frozen_s]), 3);
        }
        write_inputs(&good);
        let source = "bsc/native/crates/test/src/lib.rs";
        f.put(source, b"// changed after freeze");
        assert_eq!(evaluate(&["--verify", &frozen_s]), 3);
        f.put(source, b"// synthetic source");
        fs::rename(f.repo.join(".git"), f.repo.join("saved-git")).unwrap();
        assert_eq!(evaluate(&["--verify", &frozen_s]), 3);
        fs::rename(f.repo.join("saved-git"), f.repo.join(".git")).unwrap();
        assert_eq!(evaluate(&["--verify", &frozen_s]), 0);
        f.put("in/geth", b"a different binary");
        assert_eq!(evaluate(&["--verify", &frozen_s]), 3);
    }

    #[test]
    fn the_inputs_are_built_by_subcommands_in_gate_order() {
        let f = Fixture::new();
        let repo = f.repo.to_str().unwrap().to_string();
        let inputs = f.repo.join("inputs.dbrec");
        let i = inputs.to_str().unwrap().to_string();
        let ok = |tokens: &[&str]| run(&f, tokens) == 0;
        assert!(ok(&[
            "inputs",
            "init",
            "--out",
            &i,
            "--geth-checkout",
            "vendor"
        ]));
        // Nothing accepted: INCOMPLETE, with the reasons.
        assert_eq!(run(&f, &["--repo", &repo, "--inputs", &i]), 2);
        for (role, path) in [
            ("geth_binary", "in/geth"),
            ("wire_v2_spec", "in/spec.md"),
            ("overlay_sources", "in/overlay"),
            ("overlay_tool", "in/tool.py"),
            ("host_profile_evidence", "in/host.dbrec"),
            ("build_commands", "in/build.txt"),
            ("rust_binaries", "in/rust-a"),
            ("c1_config", "in/config.env"),
            ("test_inventory", "in/tests.log"),
            ("raw_measurement_evidence", "in/raw.dbtab"),
        ] {
            assert!(ok(&[
                "inputs",
                "set-artifact",
                role,
                "--inputs",
                &i,
                "--path",
                path
            ]));
        }
        assert!(!ok(&[
            "inputs",
            "set-artifact",
            "unknown",
            "--inputs",
            &i,
            "--path",
            "in/geth"
        ]));
        let accept = |gate: &str, evidence: &str| {
            run(
                &f,
                &[
                    "inputs",
                    "accept-gate",
                    gate,
                    "--inputs",
                    &i,
                    "--repo",
                    &repo,
                    "--accepted-by",
                    "reviewer",
                    "--statement",
                    "accepted locally",
                    "--evidence",
                    evidence,
                ],
            )
        };
        // Out of order, a missing file, a template: refused.
        assert_eq!(accept("C1.6", "in/e6.dbrec"), 2);
        assert_eq!(accept("C1.5", "in/absent"), 2);
        f.put("in/e5.dbrec", b"TEMPLATE_NOT_EVIDENCE");
        assert_eq!(accept("C1.5", "in/e5.dbrec"), 2);
        f.put("in/e5.dbrec", b"e5.dbrec");
        for (gate, evidence) in [
            ("C1.5", "in/e5.dbrec"),
            ("C1.6", "in/e6.dbrec"),
            ("C1.7", "in/e7.dbrec"),
            ("C1.8", "in/e8.dbrec"),
        ] {
            assert_eq!(accept(gate, evidence), 0, "{gate}");
        }
        // C1.9 needs an eligible production report.
        assert_eq!(accept("C1.9", "in/e8.dbrec"), 2);
        assert_eq!(accept("C1.9", "in/e9.dbrec"), 0);
        assert_eq!(accept("C1.9b", "in/e9b.dbrec"), 0);
        assert!(ok(&[
            "inputs",
            "set-commit",
            "--inputs",
            &i,
            "--repo",
            &repo
        ]));
        let built = read_inputs(&inputs).unwrap();
        assert_eq!(built.darkbot_commit.as_deref(), Some(f.head.as_str()));
        assert!(built.gates.iter().all(GateEntry::accepted));
        assert_eq!(run(&f, &["--repo", &repo, "--inputs", &i]), 0);
    }
}
