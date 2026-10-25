//! Inputs of the controlled attempt: environment, the frozen DRY evidence,
//! the producer-witness source contract and the builder gas-floor policy as
//! binary DBREC001 records (the audited import of the legacy JSON, which
//! G0.7 never reads), plus the attempt's local state (single-use latch,
//! journal location, evidence file).

use super::*;
use std::cell::RefCell;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Component;

pub(crate) fn req_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

pub(crate) fn env_u64(name: &str) -> u64 {
    req_env(name)
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{name} must be decimal u64"))
}

pub(crate) fn env_u128(name: &str) -> u128 {
    req_env(name)
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("{name} must be decimal u128"))
}

fn parse_private_key(raw: &str) -> Result<SigningKey, &'static str> {
    let mut bytes = zeroize::Zeroizing::new([0u8;32]);
    // Fixed zeroizing buffer also wipes partially decoded malformed input.
    hex::decode_to_slice(raw.trim().trim_start_matches("0x"), &mut bytes[..])
        .map_err(|_| "private key must be exactly 32 bytes of hex")?;
    SigningKey::from_slice(&bytes[..]).map_err(|_| "invalid secp256k1 private key")
}

/// # Safety
/// Standalone G0.7 main only, before any thread, TLS, child process or library
/// worker is started. Not callable from a concurrent embedding/test harness.
pub(crate) unsafe fn take_startup_keys() -> (SigningKey, SigningKey) {
    read_and_clear_keys(
        |name| env::var(name).ok(),
        // SAFETY: caller guarantees exclusive process environment at startup.
        |name| unsafe { env::remove_var(name) },
    ).expect("invalid or missing startup private key")
}

// Callbacks let offline tests exercise the complete cleanup/error ordering
// without mutating the multithreaded test harness's process environment.
fn read_and_clear_keys(
    read: impl Fn(&str) -> Option<String>,
    clear: impl Fn(&str),
) -> Result<(SigningKey, SigningKey), &'static str> {
    let launcher = read("DARKBOT_G07_LAUNCHER_PRIVATE_KEY").map(zeroize::Zeroizing::new);
    let buyer = read("DARKBOT_G07_BUYER_PRIVATE_KEY").map(zeroize::Zeroizing::new);
    for name in ["DARKBOT_G07_LAUNCHER_PRIVATE_KEY", "DARKBOT_G07_BUYER_PRIVATE_KEY", "BSC_LAUNCHER_PRIVATE_KEY", "BSC_BUYER_PRIVATE_KEY"] {
        clear(name);
    }
    let launcher = launcher.ok_or("launcher private key required")?;
    let buyer = buyer.ok_or("buyer private key required")?;
    Ok((parse_private_key(&launcher)?, parse_private_key(&buyer)?))
}

pub(crate) fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

/// A binary input record, decoded fail-closed against one of the accepted
/// schemas, and its binding: the SHA-256 of the exact file bytes that were
/// checked and, for an audited import, the legacy SHA-256 it carries.
pub(crate) fn load_record(
    path: &str,
    accepted: &[&'static darkbot_bsc_record::Schema],
) -> (Record, InputHashes) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let record = schema::decode_any(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    assert!(
        accepted.contains(&record.schema()),
        "{path}: {} is not an accepted input here",
        record.schema().name
    );
    let legacy = if record
        .schema()
        .field(schema::IMPORT_LEGACY_SHA256)
        .is_some()
    {
        record
            .opt_b256(schema::IMPORT_LEGACY_SHA256)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
    } else {
        None
    };
    let hashes = InputHashes {
        record_sha256: sha256_hex(&bytes),
        legacy_sha256: legacy.map(|w| darkbot_bsc_record::hex(&w)),
    };
    (record, hashes)
}

/// The value of a record field read before the attempt is latched: a
/// missing or mistyped field stops the run.
pub(crate) fn field<T>(value: Result<T, darkbot_bsc_record::RecordError>) -> T {
    value.unwrap_or_else(|e| panic!("{e}"))
}

/// Local state of each attempt: latch and capture journal. Git-ignored by
/// bsc/.gitignore (`/.local/`); relative to the repository root.
pub(crate) const ATTEMPT_STATE_ROOT: &str = "bsc/.local/g07";
pub(crate) const ATTEMPT_LATCH_FILE: &str = "ATTEMPT_LATCH";
pub(crate) const CAPTURE_JOURNAL_FILE: &str = "capture.journal";
/// Written into the attempt directory right after the release: from then on
/// the latch is never released.
pub(crate) const RELEASED_MARKER_FILE: &str = "RELEASED";

pub(crate) fn validate_attempt_id(attempt_id: &str) -> Result<(), String> {
    if attempt_id.is_empty()
        || attempt_id.len() > 128
        || attempt_id.starts_with('.')
        || !attempt_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(
            "DARKBOT_G07_ATTEMPT_ID must be 1..=128 characters of [A-Za-z0-9._-] not starting with '.'"
                .into(),
        );
    }
    Ok(())
}

/// The run must start at the root of the main checkout, where bsc/.local/ is
/// git-ignored (the capture journal holds broadcastable signed bytes) and
/// every run shares one latch directory (a linked worktree has its own).
pub(crate) fn repository_root_check(root: &Path) -> Result<(), String> {
    if !root.join("bsc/engine/Cargo.toml").is_file() { return Err("not the repository root (bsc/engine/Cargo.toml missing)".into()); }
    if !fs::symlink_metadata(root.join(".git")).is_ok_and(|m| m.file_type().is_dir()) { return Err("main checkout .git directory required".into()); }
    let ignored = fs::read_to_string(root.join("bsc/.gitignore")).is_ok_and(|rules| {
        rules.lines().any(|rule| rule.trim() == "/.local/") && !rules.lines().any(|rule| rule.trim_start().starts_with('!') && rule.contains(".local"))
    });
    if !ignored { return Err("bsc/.gitignore must ignore /.local/ without negation".into()); }
    Ok(())
}

pub(crate) fn assert_repository_root() {
    repository_root_check(Path::new(".")).expect("unsafe G0.7 repository root");
}

/// Read-only: reject unknown roots and symlinked/non-directory state ancestors.
#[allow(dead_code)] // Shared with readiness; runner retains atomic latch creation.
pub(crate) fn absent_attempt_latch(root: &Path, attempt: &str) -> Result<(), String> {
    repository_root_check(root)?;
    validate_attempt_id(attempt)?;
    let mut path = root.to_path_buf();
    for part in ["bsc", ".local", "g07", attempt] {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("attempt state inspection failed: {e}")),
            Ok(m) if !m.file_type().is_dir() => return Err("attempt state is not a real directory".into()),
            Ok(_) => {}
        }
    }
    Err("existing attempt state requires reconciliation; no retry".into())
}

/// Syncs a directory, making its entries (a created, renamed or removed file)
/// durable.
pub(crate) fn sync_dir(dir: &Path) -> Result<(), String> {
    fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("sync directory {}: {e}", dir.display()))
}

/// A directory fit for private attempt files: a real directory reached
/// without any symlink or `..`, with no group or other permission bits (0700).
pub(crate) fn check_private_dir(dir: &Path) -> Result<fs::Metadata, String> {
    if dir.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!("{} must not contain '..'", dir.display()));
    }
    let meta = fs::symlink_metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if !meta.file_type().is_dir() {
        return Err(format!(
            "{} must be a real directory, not a symlink",
            dir.display()
        ));
    }
    let mode = meta.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(format!(
            "{} must be private (mode 0700), found {:o}",
            dir.display(),
            mode & 0o777
        ));
    }
    let absolute: PathBuf = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|e| format!("current directory: {e}"))?
            .join(dir)
    }
    .components()
    .filter(|c| !matches!(c, Component::CurDir))
    .collect();
    let canonical = fs::canonicalize(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if canonical != absolute {
        return Err(format!(
            "{} runs through a symlink (resolves to {})",
            dir.display(),
            canonical.display()
        ));
    }
    Ok(meta)
}

pub(crate) fn owner_uid(path: &Path) -> Result<u32, String> {
    fs::symlink_metadata(path)
        .map(|m| m.uid())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Single-use latch: atomically creates `<root>/<attempt_id>/` (0700) and the
/// latch file in it (0600), both durable (file and directory entries synced).
/// A second start of the same attempt fails here, before the guard, whatever
/// ATTEMPT_NUMBER and SPENT_BUDGET_WEI claim.
pub(crate) fn latch_attempt(
    root: &Path,
    attempt_id: &str,
    latch_body: &str,
) -> Result<PathBuf, String> {
    validate_attempt_id(attempt_id)?;
    // A symlinked or shared state directory could put the journal outside the
    // private, ignored tree: the parent is checked before anything is created
    // in it, the state root after.
    let parent = root
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| format!("{} has no parent directory", root.display()))?;
    check_private_dir(parent)?;
    match fs::DirBuilder::new().mode(0o700).create(root) {
        Ok(()) => sync_dir(parent)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(format!("create {}: {e}", root.display())),
    }
    check_private_dir(root)?;
    let dir = root.join(attempt_id);
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => format!(
                "attempt {attempt_id} was already started: {} exists; no second start",
                dir.display()
            ),
            _ => format!("create {}: {e}", dir.display()),
        })?;
    sync_dir(root)?;
    let latch_path = dir.join(ATTEMPT_LATCH_FILE);
    let mut latch = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&latch_path)
        .map_err(|e| format!("create-new {}: {e}", latch_path.display()))?;
    latch
        .write_all(latch_body.as_bytes())
        .and_then(|()| latch.sync_all())
        .map_err(|e| format!("write {}: {e}", latch_path.display()))?;
    sync_dir(&dir)?;
    Ok(dir)
}

/// Whether `path` (not following a symlink) is the very file `file` is open on.
pub(crate) fn same_file(path: &Path, file: &fs::File) -> bool {
    match (fs::symlink_metadata(path), file.metadata()) {
        (Ok(at_path), Ok(open)) => at_path.dev() == open.dev() && at_path.ino() == open.ino(),
        _ => false,
    }
}

fn evidence_dir(path: &Path) -> PathBuf {
    path.parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// The mandatory evidence file must not exist yet, have no `..`, and lie in
/// a private directory (see check_private_dir); checked before anything is
/// latched and again when it is created.
pub(crate) fn validate_evidence_target(path: &str) -> Result<(), String> {
    let target = Path::new(path);
    if path.is_empty() || target.file_name().is_none() {
        return Err(format!("{path:?} is not a file path"));
    }
    if target
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(format!("{path} must not contain '..'"));
    }
    match fs::symlink_metadata(target) {
        Ok(_) => return Err(format!("{path} already exists")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{path}: {e}")),
    }
    check_private_dir(&evidence_dir(target)).map(|_| ())
}

/// This run's evidence file: 0600 in a private directory owned by this user,
/// never followed through a symlink, and only ever replaced atomically: a
/// complete, fsynced private sibling is renamed over it and the directory is
/// synced, so the path holds the previous or the new complete evidence, never
/// a truncated one. A replacement refuses when the path no longer refers to
/// this run's file.
pub(crate) struct EvidenceFile {
    path: PathBuf,
    dir: PathBuf,
    /// The file the path refers to: the empty placeholder, then each
    /// replacement.
    current: RefCell<fs::File>,
}

impl EvidenceFile {
    pub(crate) fn create(path: &str, owner_uid: u32) -> Result<Self, String> {
        validate_evidence_target(path)?;
        let path = PathBuf::from(path);
        let dir = evidence_dir(&path);
        let meta = check_private_dir(&dir)?;
        if meta.uid() != owner_uid {
            return Err(format!("{} is not owned by this user", dir.display()));
        }
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("create-new {}: {e}", path.display()))?;
        sync_dir(&dir)?;
        let evidence = Self {
            path,
            dir,
            current: RefCell::new(file),
        };
        // Prove now, before the guard, that a replacement can be staged here
        // (directory writable, staging name not too long). If not, this run's
        // own empty placeholder goes again, so no evidence file is left.
        match evidence.stage(b"") {
            Ok((trial, _)) => {
                fs::remove_file(&trial).map_err(|e| format!("remove {}: {e}", trial.display()))?;
                Ok(evidence)
            }
            Err(e) => {
                let _ = fs::remove_file(&evidence.path);
                let _ = sync_dir(&evidence.dir);
                Err(e)
            }
        }
    }

    /// Writes `content` to a new private sibling and fsyncs it. The evidence
    /// path is not touched; a crash here leaves the previous evidence intact.
    pub(crate) fn stage(&self, content: &[u8]) -> Result<(PathBuf, fs::File), String> {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut staged = None;
        for n in 0..1000u32 {
            let tmp = self
                .dir
                .join(format!(".{name}.tmp-{}-{n}", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
            {
                Ok(file) => {
                    staged = Some((tmp, file));
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(format!(
                        "create evidence staging file {}: {e}",
                        tmp.display()
                    ));
                }
            }
        }
        let (tmp, mut file) = staged.ok_or("no free evidence staging file name")?;
        if let Err(e) = file.write_all(content).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&tmp);
            return Err(format!(
                "write evidence staging file {}: {e}",
                tmp.display()
            ));
        }
        Ok((tmp, file))
    }

    /// Renames a staged file over the evidence path and syncs the directory.
    /// When it cannot, the complete staged copy is kept and named in the error.
    pub(crate) fn commit(&self, (tmp, file): (PathBuf, fs::File)) -> Result<(), String> {
        if !same_file(&self.path, &self.current.borrow()) {
            return Err(format!(
                "{} no longer refers to this run's evidence file; not replaced, complete copy kept at {}",
                self.path.display(),
                tmp.display()
            ));
        }
        if let Err(e) = fs::rename(&tmp, &self.path) {
            return Err(format!(
                "rename over {}: {e}; complete copy kept at {}",
                self.path.display(),
                tmp.display()
            ));
        }
        *self.current.borrow_mut() = file;
        sync_dir(&self.dir).map_err(|e| format!("evidence replaced but not yet durable: {e}"))
    }

    pub(crate) fn replace(&self, content: &[u8]) -> Result<(), String> {
        self.commit(self.stage(content)?)
    }

    /// Removes the evidence file if it is still this run's empty placeholder.
    pub(crate) fn remove_if_unwritten(&self) -> String {
        let path = self.path.display();
        if !same_file(&self.path, &self.current.borrow()) {
            return format!("{path} is no longer this run's evidence file and was left alone");
        }
        match self.current.borrow().metadata() {
            Ok(m) if m.len() == 0 => match fs::remove_file(&self.path) {
                Ok(()) => {
                    let _ = sync_dir(&self.dir);
                    format!("empty evidence file {path} removed")
                }
                Err(e) => format!("empty evidence file {path} not removed: {e}"),
            },
            Ok(_) => format!("evidence file {path} is not empty and was left alone"),
            Err(e) => format!("evidence file {path}: {e}"),
        }
    }
}

/// Releases the latch of a run that provably wrote no bundle byte (guard
/// refusal or pre-egress stop): the attempt directory is renamed with its
/// journal, never deleted, and this run's evidence placeholder is removed if
/// still empty, so a corrected run can start without deleting anything by
/// hand. Refuses unless the RELEASED marker is provably absent. Returns the
/// directory the latch was renamed to (none if it was not released) and what
/// happened.
pub(crate) fn release_unsent_attempt(
    attempt_dir: &Path,
    evidence: Option<&EvidenceFile>,
    outcome: &str,
) -> (Option<PathBuf>, String) {
    match fs::symlink_metadata(attempt_dir.join(RELEASED_MARKER_FILE)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => {
            return (
                None,
                format!(
                    "{} holds the RELEASED marker: the latch is never released",
                    attempt_dir.display()
                ),
            );
        }
        Err(e) => {
            return (
                None,
                format!(
                    "the RELEASED marker of {} is unclear ({e}): the latch is kept",
                    attempt_dir.display()
                ),
            );
        }
    }
    let name = attempt_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stamp = unix_time_ms();
    let released = (0u32..1000)
        .map(|n| attempt_dir.with_file_name(format!("{name}.{outcome}-{stamp}-{n}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| attempt_dir.with_file_name(format!("{name}.{outcome}-{stamp}")));
    let (renamed, latch) = match fs::rename(attempt_dir, &released) {
        Ok(()) => {
            if let Some(root) = attempt_dir.parent() {
                let _ = sync_dir(root);
            }
            let note = format!(
                "nothing was sent; latch released, attempt state kept at {}",
                released.display()
            );
            (Some(released), note)
        }
        Err(e) => (
            None,
            format!(
                "nothing was sent, but the latch could not be released ({e}); this run stopped before the release, so {} may be renamed by hand",
                attempt_dir.display()
            ),
        ),
    };
    let evidence = evidence.map_or_else(
        || "this run created no evidence file".to_string(),
        EvidenceFile::remove_if_unwritten,
    );
    (renamed, format!("{latch}; {evidence}"))
}

pub(crate) fn validate_producer_witness_source(
    path: &str,
    attempt_id: &str,
) -> (Record, InputHashes) {
    use schema::g07::witness_source as w;
    let (r, hashes) = load_record(path, &[&w::SCHEMA]);
    assert_eq!(
        field(r.text(w::LEGACY_SCHEMA)),
        "darkbot.producer-witness-source.v1",
        "wrong producer witness source contract schema"
    );
    assert_eq!(
        field(r.text(w::ATTEMPT_ID)),
        attempt_id,
        "producer witness source contract belongs to another attempt"
    );
    let _source_ready = field(r.bool(w::SOURCE_READY));
    assert!(field(r.bool(w::ORIGINAL_EXECUTION_ONLY)));
    assert_eq!(
        field(r.text(w::REQUIRED_WITNESS_SCHEMA)),
        PRODUCER_WITNESS_SCHEMA_PATH
    );
    assert!(field(r.bool(w::DELIVERY_MAY_BE_ASYNCHRONOUS)));
    assert!(field(r.bool(w::CANONICAL_IDENTITY_BINDING_REQUIRED)));
    assert!(field(r.bool(w::CLOCK_METADATA_REQUIRED)));
    assert!(
        !field(r.text(w::SOURCE_DESCRIPTION)).trim().is_empty(),
        "producer witness source description is empty"
    );
    (r, hashes)
}

pub(crate) fn validate_builder_policy(path: &str, signed_gas_price: u64) -> (Record, InputHashes) {
    use schema::g06a::builder_policy as b;
    let (r, hashes) = load_record(path, &[&b::NATIVE, &b::SCHEMA]);
    let expected_name = if r.schema() == &b::NATIVE {
        b::NATIVE.name
    } else {
        "darkbot.g06a.builder-floor-evidence-addendum.v2"
    };
    assert_eq!(field(r.text(b::LEGACY_SCHEMA)), expected_name);
    assert!(field(r.bool(b::DRY3_READY)));
    assert!(field(r.bool(b::BLOCKRAZOR_VERIFIED)));
    assert!(field(r.bool(b::FORTYEIGHTCLUB_VERIFIED)));
    let strict_floor = U256::from_be_bytes(field(r.u256(b::STRICTEST_FLOOR_WEI)));
    let stage1_policy = U256::from_be_bytes(field(r.u256(b::STAGE1_GAS_PRICE_WEI)));
    assert!(
        U256::from(signed_gas_price) >= strict_floor,
        "signed gas price is below the frozen builder floor evidence"
    );
    assert_eq!(
        U256::from(signed_gas_price),
        stage1_policy,
        "signed gas price differs from the frozen Stage-1 builder policy"
    );
    (r, hashes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_imported_builder_policy_keep_schema_and_gas_guards() {
        use schema::g06a::builder_policy as b;
        let imported_bytes = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"),
            "/../../../native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec"));
        let imported = schema::decode_any(imported_bytes).unwrap();
        let dir = scratch("builder-schema");
        let path = dir.join("policy.dbrec");
        let file = path.to_str().unwrap();
        fs::write(&path, imported_bytes).unwrap();
        validate_builder_policy(file, 50_000_000);
        let mut native = Draft::new(&b::NATIVE);
        native.set(schema::PROVENANCE_TOOL, "offline-builder-schema-fixture")
            .set(schema::PROVENANCE_OBSERVED_UNIX_MS, 1_u64);
        for (tag, value) in imported.fields() {
            if *tag >= 10 { native.push(*tag, value.clone()); }
        }
        native.set(b::LEGACY_SCHEMA, b::NATIVE.name);
        fs::write(&path, native.encode().unwrap()).unwrap();
        validate_builder_policy(file, 50_000_000);
        for gas in [49_999_999, 50_000_001] {
            assert!(std::panic::catch_unwind(|| validate_builder_policy(file, gas)).is_err());
        }
        native.set(b::LEGACY_SCHEMA, "darkbot.g06a.builder-floor-evidence-addendum.v2");
        fs::write(&path, native.encode().unwrap()).unwrap();
        assert!(std::panic::catch_unwind(|| validate_builder_policy(file, 50_000_000)).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    /// A fresh private directory under the canonical temp dir (on macOS the
    /// temp dir is reached through the /var -> /private/var symlink).
    fn scratch(tag: &str) -> PathBuf {
        let base = fs::canonicalize(env::temp_dir()).unwrap();
        let dir = base.join(format!(
            "darkbot-g07-{tag}-{}-{}",
            std::process::id(),
            unix_time_ms()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        dir
    }

    fn mode(p: &Path) -> u32 {
        fs::symlink_metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn evidence_in(dir: &Path) -> (EvidenceFile, PathBuf) {
        let path = dir.join("g07.dbrec");
        let evidence =
            EvidenceFile::create(path.to_str().unwrap(), owner_uid(dir).unwrap()).unwrap();
        (evidence, path)
    }

    #[test]
    fn attempt_id_is_a_single_safe_path_component() {
        assert!(validate_attempt_id("g07-mainnet-attempt-1-dry-20261015T222241Z").is_ok());
        for bad in ["", ".", "..", ".hidden", "a/b", "../x", "a b", "a\nb"] {
            assert!(validate_attempt_id(bad).is_err(), "{bad:?}");
        }
        assert!(validate_attempt_id(&"a".repeat(129)).is_err());
    }

    #[test]
    fn latch_admits_one_start_per_attempt_with_private_modes() {
        let base = scratch("latch");
        let root = base.join("g07");
        let dir = latch_attempt(&root, "attempt-1", "pid=1\n").unwrap();
        assert_eq!(mode(&root), 0o700);
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join(ATTEMPT_LATCH_FILE)), 0o600);
        assert_eq!(
            fs::read_to_string(dir.join(ATTEMPT_LATCH_FILE)).unwrap(),
            "pid=1\n"
        );
        let again = latch_attempt(&root, "attempt-1", "pid=2\n").unwrap_err();
        assert!(again.contains("already started"), "{again}");
        assert!(latch_attempt(&root, "attempt-2", "pid=3\n").is_ok());
        assert!(latch_attempt(&root, "../escape", "pid=4\n").is_err());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn private_dirs_refuse_symlinks_on_the_way_open_modes_and_dot_dot() {
        let base = scratch("roots");
        let real = base.join("real");
        fs::DirBuilder::new().mode(0o700).create(&real).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        // The directory itself a symlink.
        let e = latch_attempt(&link, "attempt-1", "").unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        // A symlink further up the path.
        let below = link.join("g07");
        let e = latch_attempt(&below, "attempt-1", "").unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        assert!(!real.join("g07").exists(), "created through a symlink");
        // Group/other permissions.
        let open = base.join("open");
        fs::create_dir(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        let e = latch_attempt(&open, "attempt-1", "").unwrap_err();
        assert!(e.contains("private"), "{e}");
        assert!(!open.join("attempt-1").exists());
        assert!(check_private_dir(&real.join("..").join("real")).is_err());
        assert!(check_private_dir(&real).is_ok());
        // A real, private directory behind a symlinked ancestor.
        let sub = real.join("sub");
        fs::DirBuilder::new().mode(0o700).create(&sub).unwrap();
        assert!(check_private_dir(&sub).is_ok());
        let e = check_private_dir(&link.join("sub")).unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn evidence_target_must_be_new_in_a_private_directory() {
        let dir = scratch("target");
        let target = dir.join("g07.dbrec");
        let target = target.to_str().unwrap();
        assert!(validate_evidence_target(target).is_ok());
        fs::write(target, b"x").unwrap();
        assert!(validate_evidence_target(target).is_err());
        let orphan = dir.join("missing").join("g07.dbrec");
        assert!(validate_evidence_target(orphan.to_str().unwrap()).is_err());
        let open = dir.join("open");
        fs::create_dir(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(validate_evidence_target(open.join("g07.dbrec").to_str().unwrap()).is_err());
        let link = dir.join("link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert!(validate_evidence_target(link.join("g07.dbrec").to_str().unwrap()).is_err());
        let dangling = dir.join("dangling.dbrec");
        std::os::unix::fs::symlink(dir.join("nowhere"), &dangling).unwrap();
        assert!(validate_evidence_target(dangling.to_str().unwrap()).is_err());
        assert!(validate_evidence_target(&format!("{}/../g07.dbrec", dir.display())).is_err());
        assert!(validate_evidence_target("").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn evidence_is_private_and_replaced_atomically_without_leftovers() {
        let dir = scratch("evidence");
        let (evidence, path) = evidence_in(&dir);
        assert_eq!(mode(&path), 0o600);
        assert_eq!(fs::read(&path).unwrap(), b"");
        evidence.replace(b"{\"v\":1}\n").unwrap();
        evidence.replace(b"{\"v\":2}\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"v\":2}\n");
        assert_eq!(mode(&path), 0o600);
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        // A second run on the same path is refused.
        assert!(EvidenceFile::create(path.to_str().unwrap(), owner_uid(&dir).unwrap()).is_err());
        // A directory owned by someone else is refused, and nothing is created.
        let other = dir.join("other.dbrec");
        let e = EvidenceFile::create(other.to_str().unwrap(), owner_uid(&dir).unwrap() + 1)
            .err()
            .unwrap();
        assert!(e.contains("not owned"), "{e}");
        assert!(!other.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_evidence_name_that_cannot_be_staged_is_refused_without_leftovers() {
        let dir = scratch("stagename");
        // Fits as a file name (<= 255 bytes), but its staging sibling does not.
        let path = dir.join("e".repeat(250));
        let e = EvidenceFile::create(path.to_str().unwrap(), owner_uid(&dir).unwrap())
            .err()
            .unwrap();
        assert!(e.contains("staging"), "{e}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_crash_or_failed_write_never_destroys_the_previous_evidence() {
        let dir = scratch("crash");
        let (evidence, path) = evidence_in(&dir);
        evidence.replace(b"{\"v\":1}\n").unwrap();
        // Crash after staging, before the rename: the path still holds v1.
        let staged = evidence.stage(b"{\"v\":2}\n").unwrap();
        drop(staged);
        assert_eq!(fs::read(&path).unwrap(), b"{\"v\":1}\n");
        // The next write still succeeds and leaves complete content.
        evidence.replace(b"{\"v\":3}\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"v\":3}\n");
        // A write that cannot even stage (read-only directory) fails visibly
        // and keeps v3.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).unwrap();
        let e = evidence.replace(b"{\"v\":4}\n").unwrap_err();
        assert!(e.contains("staging"), "{e}");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"{\"v\":3}\n");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_foreign_file_at_the_evidence_path_is_never_replaced_or_removed() {
        let dir = scratch("foreign");
        let (evidence, path) = evidence_in(&dir);
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"foreign").unwrap();
        let e = evidence.replace(b"{}\n").unwrap_err();
        assert!(
            e.contains("no longer refers") && e.contains("copy kept at"),
            "{e}"
        );
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        assert!(evidence.remove_if_unwritten().contains("left alone"));
        assert_eq!(fs::read(&path).unwrap(), b"foreign");
        // The refused content survives as the named private copy.
        let kept: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p != &path)
            .collect();
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert_eq!(fs::read(&kept[0]).unwrap(), b"{}\n");
        assert_eq!(mode(&kept[0]), 0o600);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn release_renames_the_latch_and_removes_only_this_runs_empty_placeholder() {
        let base = scratch("release");
        let root = base.join("g07");
        let dir = latch_attempt(&root, "attempt-1", "").unwrap();
        fs::write(dir.join(CAPTURE_JOURNAL_FILE), b"journal").unwrap();
        let (evidence, path) = evidence_in(&base);
        let (renamed, note) = release_unsent_attempt(&dir, Some(&evidence), "refused");
        assert!(
            note.contains("latch released") && note.contains("removed"),
            "{note}"
        );
        assert!(!dir.exists() && !path.exists());
        let kept: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(kept.len(), 1);
        let kept = root.join(&kept[0]);
        assert_eq!(renamed.as_deref(), Some(kept.as_path()));
        assert!(kept.to_string_lossy().contains("attempt-1.refused-"));
        assert_eq!(
            fs::read(kept.join(CAPTURE_JOURNAL_FILE)).unwrap(),
            b"journal"
        );

        // The attempt can start again; written evidence is kept.
        let dir = latch_attempt(&root, "attempt-1", "").unwrap();
        let (evidence, path) = evidence_in(&base);
        evidence.replace(b"{}\n").unwrap();
        let (_, note) = release_unsent_attempt(&dir, Some(&evidence), "pre-egress");
        assert!(note.contains("is not empty and was left alone"), "{note}");
        assert!(path.exists());

        // After the release marker the latch is never released.
        let dir = latch_attempt(&root, "attempt-1", "").unwrap();
        fs::write(dir.join(RELEASED_MARKER_FILE), b"released").unwrap();
        let (renamed, note) = release_unsent_attempt(&dir, None, "pre-egress");
        assert!(
            note.contains("never released") && renamed.is_none(),
            "{note}"
        );
        assert!(dir.join(ATTEMPT_LATCH_FILE).exists());
        assert!(latch_attempt(&root, "attempt-1", "").is_err());
        // A marker that cannot be inspected is not an absent one.
        fs::remove_file(dir.join(RELEASED_MARKER_FILE)).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o000)).unwrap();
        let (renamed, note) = release_unsent_attempt(&dir, None, "pre-egress");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(note.contains("unclear") && renamed.is_none(), "{note}");
        assert!(dir.join(ATTEMPT_LATCH_FILE).exists());

        let (renamed, note) = release_unsent_attempt(&root.join("missing"), None, "pre-egress");
        assert!(renamed.is_none());
        assert!(
            note.contains("could not be released") && note.contains("created no evidence file"),
            "{note}"
        );
        fs::remove_dir_all(&base).unwrap();
    }
}

#[cfg(test)]
mod root_regression {
    use super::*;
    #[test]
    fn latch_requires_safe_main_root_and_absent_real_state() {
        let root = std::env::temp_dir().join(format!("g07-root-check-{}", std::process::id()));
        fs::create_dir_all(root.join("bsc/engine")).unwrap();
        fs::write(root.join("bsc/engine/Cargo.toml"), b"offline-test").unwrap();
        let id = "offline-test";
        assert!(absent_attempt_latch(&root, id).is_err());
        fs::write(root.join(".git"), "gitdir: other").unwrap();
        fs::write(root.join("bsc/.gitignore"), "/.local/\n").unwrap();
        assert!(absent_attempt_latch(&root,id).is_err());
        fs::remove_file(root.join(".git")).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        assert!(absent_attempt_latch(&root,id).is_ok());
        fs::write(root.join("bsc/.gitignore"), "/.local/\n!/.local/\n").unwrap();
        assert!(absent_attempt_latch(&root,id).is_err());
        fs::write(root.join("bsc/.gitignore"), "/.local/\n").unwrap();
        fs::create_dir_all(root.join("bsc/.local/g07/offline-test")).unwrap();
        assert!(absent_attempt_latch(&root,id).is_err());
        fs::remove_dir(root.join("bsc/.local/g07/offline-test")).unwrap();
        std::os::unix::fs::symlink("/nonexistent-g07-state",root.join("bsc/.local/g07/offline-test")).unwrap();
        assert!(absent_attempt_latch(&root,id).is_err());
        assert!(absent_attempt_latch(&root,"../escape").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod key_regression {
    use super::*;
    #[test]
    fn startup_clears_all_key_names_before_success_or_error() {
        use std::collections::HashMap;
        for input in [Some("22".repeat(32)), None, Some("private-invalid-dummy".into())] {
            let environment = RefCell::new(HashMap::from([
                ("DARKBOT_G07_BUYER_PRIVATE_KEY", "33".repeat(32)),
                ("BSC_LAUNCHER_PRIVATE_KEY", "dummy".into()),
                ("BSC_BUYER_PRIVATE_KEY", "dummy".into()),
                ("PATH", "/usr/bin".into()),
            ]));
            if let Some(value) = &input {
                environment.borrow_mut().insert("DARKBOT_G07_LAUNCHER_PRIVATE_KEY", value.clone());
            }
            let result = read_and_clear_keys(
                |name| environment.borrow().get(name).cloned(),
                |name| { environment.borrow_mut().remove(name); },
            );
            assert_eq!(environment.borrow().len(), 1);
            assert!(environment.borrow().contains_key("PATH"));
            if input.as_deref() == Some("22".repeat(32).as_str()) {
                let (launcher, buyer) = result.unwrap();
                assert_eq!(signer_address(&launcher), signer_address(&parse_private_key(&"22".repeat(32)).unwrap()));
                assert_ne!(signer_address(&launcher), signer_address(&buyer));
            } else {
                assert!(!result.unwrap_err().contains("private-invalid-dummy"));
            }
        }
    }

    #[test]
    fn key_parse_is_redacted_and_matches_synthetic_signer() {
        let key = parse_private_key(&"22".repeat(32)).unwrap();
        let signer = darkbot_bsc_trading::signer::Signer::from_secret_bytes(&[0x22;32]).unwrap();
        assert_eq!(signer_address(&key), signer.address());
        for bad in [String::new(), "22".repeat(31), "22".repeat(33), "00".repeat(32), format!("{}secret", "22".repeat(30))] {
            let error = parse_private_key(&bad).unwrap_err();
            assert!(!error.contains("secret"));
        }
    }
}
