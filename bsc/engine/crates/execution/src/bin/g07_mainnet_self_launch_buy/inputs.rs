//! Inputs of the controlled attempt: environment, frozen JSON evidence, the
//! producer-witness source contract and the builder gas-floor policy, plus the
//! attempt's local state (single-use latch, journal location, evidence file).

use super::*;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};

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

pub(crate) fn norm(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

pub(crate) fn key_from_env(name: &str) -> SigningKey {
    let raw = req_env(name);
    let bytes = hex::decode(raw.trim().trim_start_matches("0x"))
        .unwrap_or_else(|_| panic!("{name} is not valid hex"));
    assert_eq!(bytes.len(), 32, "{name} must be exactly 32 bytes");
    SigningKey::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("{name} is not a valid secp256k1 private key"))
}

pub(crate) fn signer_address(key: &SigningKey) -> Address {
    let point = key.verifying_key().to_encoded_point(false);
    let h = keccak256(&point.as_bytes()[1..]);
    Address::from_slice(&h.as_slice()[12..])
}

/// The parsed JSON and the exact bytes it was parsed from, so the evidence
/// hash bound by the guard describes what was checked.
pub(crate) fn load_json(path: &str) -> (Value, Vec<u8>) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let value = serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {path}: {e}"));
    (value, bytes)
}

/// Local state of each attempt: latch and capture journal. Git-ignored by
/// bsc/.gitignore (`/.local/`); relative to the repository root.
pub(crate) const ATTEMPT_STATE_ROOT: &str = "bsc/.local/g07";
pub(crate) const ATTEMPT_LATCH_FILE: &str = "ATTEMPT_LATCH";
pub(crate) const CAPTURE_JOURNAL_FILE: &str = "capture.journal";

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
pub(crate) fn assert_repository_root() {
    assert!(
        Path::new(V2_RELATIVE_PATH).is_file(),
        "run G0.7 from the repository root: {V2_RELATIVE_PATH} not found"
    );
    assert!(
        fs::symlink_metadata(".git").is_ok_and(|m| m.file_type().is_dir()),
        "run G0.7 from the main checkout: .git must be a directory, not a linked worktree"
    );
    let ignored = fs::read_to_string("bsc/.gitignore").is_ok_and(|rules| {
        rules.lines().any(|rule| rule.trim() == "/.local/")
            && !rules
                .lines()
                .any(|rule| rule.trim_start().starts_with('!') && rule.contains(".local"))
    });
    assert!(
        ignored,
        "bsc/.gitignore must ignore /.local/ (without negation) before G0.7 writes its journal there"
    );
}

/// Single-use latch: atomically creates `<root>/<attempt_id>/` (0700) and the
/// latch file in it (0600). A second start of the same attempt fails here,
/// before the guard, whatever ATTEMPT_NUMBER and SPENT_BUDGET_WEI claim.
pub(crate) fn latch_attempt(root: &Path, attempt_id: &str, latch_body: &str) -> Result<PathBuf, String> {
    validate_attempt_id(attempt_id)?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(root)
        .map_err(|e| format!("create {}: {e}", root.display()))?;
    // A symlinked state directory could put the journal outside the ignored tree.
    for dir in root.parent().into_iter().chain([root]) {
        let real_dir = fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_dir());
        if !real_dir {
            return Err(format!("{} must be a real directory, not a symlink", dir.display()));
        }
    }
    let mode = fs::metadata(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(format!(
            "{} must be private (mode 0700), found {:o}",
            root.display(),
            mode & 0o777
        ));
    }
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
    Ok(dir)
}

/// Whether `path` (not following a symlink) is the very file `file` is open on.
pub(crate) fn same_file(path: &str, file: &fs::File) -> bool {
    match (fs::symlink_metadata(path), file.metadata()) {
        (Ok(at_path), Ok(open)) => at_path.dev() == open.dev() && at_path.ino() == open.ino(),
        _ => false,
    }
}

/// Releases the latch of a run that provably wrote no bundle byte (guard
/// refusal or pre-egress stop): the attempt directory is renamed with its
/// journal, never deleted, and this run's own evidence file is removed if it
/// is still empty (`evidence` is the path and the handle this run created;
/// `None` when it created none), so a corrected run can start without
/// deleting anything by hand. Returns what happened, for the terminal message.
pub(crate) fn release_unsent_attempt(
    attempt_dir: &Path,
    evidence: Option<(&str, &fs::File)>,
    outcome: &str,
) -> String {
    let name = attempt_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stamp = unix_time_ms();
    let released = (0u32..)
        .map(|n| attempt_dir.with_file_name(format!("{name}.{outcome}-{stamp}-{n}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| attempt_dir.with_file_name(format!("{name}.{outcome}-{stamp}")));
    let latch = match fs::rename(attempt_dir, &released) {
        Ok(()) => format!(
            "nothing was sent; latch released, attempt state kept at {}",
            released.display()
        ),
        Err(e) => format!(
            "nothing was sent, but the latch could not be released ({e}); this run stopped before the release, so {} may be renamed by hand",
            attempt_dir.display()
        ),
    };
    let evidence = match evidence {
        None => "this run created no evidence file".to_string(),
        Some((path, file)) if !same_file(path, file) => {
            format!("{path} is no longer this run's evidence file and was left alone")
        }
        Some((path, file)) => match file.metadata() {
            Ok(m) if m.len() == 0 => match fs::remove_file(path) {
                Ok(()) => format!("empty evidence file {path} removed"),
                Err(e) => format!("empty evidence file {path} not removed: {e}"),
            },
            Ok(_) => format!("evidence file {path} is not empty and was left alone"),
            Err(e) => format!("evidence file {path}: {e}"),
        },
    };
    format!("{latch}; {evidence}")
}

/// The mandatory evidence file must not exist yet and its directory must;
/// checked before anything is latched.
pub(crate) fn validate_new_file_target(path: &str) -> Result<(), String> {
    let target = Path::new(path);
    match fs::symlink_metadata(target) {
        Ok(_) => return Err(format!("{path} already exists")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{path}: {e}")),
    }
    let dir = target
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !dir.is_dir() {
        return Err(format!("the directory of {path} does not exist"));
    }
    Ok(())
}

pub(crate) fn req_str<'a>(v: &'a Value, ptr: &str) -> &'a str {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing/bad string at {ptr}"))
}

pub(crate) fn req_bool(v: &Value, ptr: &str) -> bool {
    v.pointer(ptr)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("missing/bad bool at {ptr}"))
}

pub(crate) fn req_u64(v: &Value, ptr: &str) -> u64 {
    v.pointer(ptr)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing/bad u64 at {ptr}"))
}

pub(crate) fn req_u64_string(v: &Value, ptr: &str) -> u64 {
    req_str(v, ptr)
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("bad decimal u64 at {ptr}"))
}

pub(crate) fn req_u128_string(v: &Value, ptr: &str) -> u128 {
    req_str(v, ptr)
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("bad decimal u128 at {ptr}"))
}

pub(crate) fn validate_producer_witness_source(path: &str, attempt_id: &str) -> (Value, String) {
    let bytes = fs::read(path)
        .unwrap_or_else(|e| panic!("read producer witness source contract {path}: {e}"));
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse producer witness source contract {path}: {e}"));
    assert_eq!(
        req_str(&value, "/schema"),
        "darkbot.producer-witness-source.v1",
        "wrong producer witness source contract schema"
    );
    assert_eq!(
        req_str(&value, "/attempt_id"),
        attempt_id,
        "producer witness source contract belongs to another attempt"
    );
    let _source_ready = req_bool(&value, "/source_ready");
    assert!(req_bool(
        &value,
        "/original_successful_producer_execution_only"
    ));
    assert_eq!(
        req_str(&value, "/required_witness_schema"),
        PRODUCER_WITNESS_SCHEMA_PATH
    );
    assert!(req_bool(&value, "/delivery_may_be_asynchronous"));
    assert!(req_bool(&value, "/canonical_identity_binding_required"));
    assert!(req_bool(&value, "/clock_and_uncertainty_metadata_required"));
    assert!(
        !req_str(&value, "/source_description").trim().is_empty(),
        "producer witness source description is empty"
    );
    (value, sha256_hex(&bytes))
}

pub(crate) fn validate_builder_policy(path: &str, signed_gas_price: u64) -> (Value, String) {
    let bytes =
        fs::read(path).unwrap_or_else(|e| panic!("read builder policy evidence {path}: {e}"));
    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse builder policy evidence {path}: {e}"));
    assert_eq!(
        req_str(&value, "/schema"),
        "darkbot.g06a.builder-floor-evidence-addendum.v2"
    );
    assert!(req_bool(&value, "/dry3_ready"));
    assert!(req_bool(&value, "/blockrazor/verified"));
    assert!(req_bool(&value, "/fortyeightclub/verified"));
    let strict_floor = req_u64_string(&value, "/strictest_explicit_verified_floor_wei");
    let stage1_policy = req_u64_string(&value, "/stage1_signed_gas_price_policy_wei");
    assert!(
        signed_gas_price >= strict_floor,
        "signed gas price is below the frozen builder floor evidence"
    );
    assert_eq!(
        signed_gas_price, stage1_policy,
        "signed gas price differs from the frozen Stage-1 builder policy"
    );
    (value, sha256_hex(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        env::temp_dir().join(format!(
            "darkbot-g07-{tag}-{}-{}",
            std::process::id(),
            unix_time_ms()
        ))
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
        let root = scratch("latch");
        let dir = latch_attempt(&root, "attempt-1", "pid=1\n").unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join(ATTEMPT_LATCH_FILE)), 0o600);
        assert_eq!(fs::read_to_string(dir.join(ATTEMPT_LATCH_FILE)).unwrap(), "pid=1\n");
        let again = latch_attempt(&root, "attempt-1", "pid=2\n").unwrap_err();
        assert!(again.contains("already started"), "{again}");
        assert!(latch_attempt(&root, "attempt-2", "pid=3\n").is_ok());
        assert!(latch_attempt(&root, "../escape", "pid=4\n").is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn latch_refuses_symlinked_or_shared_state_roots() {
        let base = scratch("roots");
        fs::create_dir(&base).unwrap();
        let real = base.join("real");
        fs::create_dir(&real).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let e = latch_attempt(&link, "attempt-1", "").unwrap_err();
        assert!(e.contains("symlink"), "{e}");
        let open = base.join("open");
        fs::create_dir(&open).unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o755)).unwrap();
        let e = latch_attempt(&open, "attempt-1", "").unwrap_err();
        assert!(e.contains("private"), "{e}");
        assert!(!open.join("attempt-1").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn release_renames_the_latch_and_removes_only_this_runs_empty_evidence_file() {
        let root = scratch("release");
        let dir = latch_attempt(&root, "attempt-1", "").unwrap();
        fs::write(dir.join(CAPTURE_JOURNAL_FILE), b"journal").unwrap();
        let evidence = root.join("evidence.json");
        let evidence_str = evidence.to_str().unwrap();
        let own = fs::File::create(&evidence).unwrap();
        let note = release_unsent_attempt(&dir, Some((evidence_str, &own)), "refused");
        assert!(note.contains("latch released"), "{note}");
        assert!(!dir.exists() && !evidence.exists());
        let kept: Vec<_> = fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(kept.len(), 1);
        let kept = root.join(&kept[0]);
        assert!(kept.to_string_lossy().contains("attempt-1.refused-"));
        assert_eq!(fs::read(kept.join(CAPTURE_JOURNAL_FILE)).unwrap(), b"journal");

        // The attempt can start again. A non-empty own file is kept.
        let dir = latch_attempt(&root, "attempt-1", "").unwrap();
        let mut own = fs::File::create(&evidence).unwrap();
        own.write_all(b"{}").unwrap();
        let note = release_unsent_attempt(&dir, Some((evidence_str, &own)), "pre-egress");
        assert!(note.contains("is not empty and was left alone"), "{note}");
        assert!(evidence.exists());

        // An empty file at the path that is not this run's is never removed.
        fs::remove_file(&evidence).unwrap();
        let dir = latch_attempt(&root, "attempt-1", "").unwrap();
        let foreign = fs::File::create(root.join("other.json")).unwrap();
        fs::File::create(&evidence).unwrap();
        let note = release_unsent_attempt(&dir, Some((evidence_str, &foreign)), "pre-egress");
        assert!(note.contains("no longer this run's evidence file"), "{note}");
        assert!(evidence.exists());
        let note = release_unsent_attempt(&root.join("missing"), None, "pre-egress");
        assert!(note.contains("could not be released") && note.contains("created no evidence file"), "{note}");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn evidence_target_must_be_new_in_an_existing_directory() {
        let dir = scratch("evidence");
        fs::create_dir(&dir).unwrap();
        let target = dir.join("g07.json");
        let target = target.to_str().unwrap();
        assert!(validate_new_file_target(target).is_ok());
        fs::write(target, b"x").unwrap();
        assert!(validate_new_file_target(target).is_err());
        let orphan = dir.join("missing").join("g07.json");
        assert!(validate_new_file_target(orphan.to_str().unwrap()).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
