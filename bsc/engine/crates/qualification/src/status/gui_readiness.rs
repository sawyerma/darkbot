//! BSC-048: the typed provenance of one GUI readiness result
//! (`provenance.dbrec`, record type 37; formerly an internal
//! `provenance.json`). `seal` writes it next to the validated
//! `readiness.dbrec` of a job run; `check` is the import gate of the
//! desktop sync. Read-only operational evidence, never a qualification.

use darkbot_bsc_record::schema::g07::{gui_readiness_provenance as p, readiness};
use darkbot_bsc_record::{Draft, Record, Value, decode, hex, sha256};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

pub const RESULT: &str = "readiness.dbrec";
pub const PROVENANCE: &str = "provenance.dbrec";
const RESULT_DIR: &str = "bsc/native/evidence/g07";

fn readiness_bytes(dir: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(dir.join(RESULT)).map_err(|e| format!("{RESULT}: {e}"))?;
    decode(&readiness::SCHEMA, &bytes).map_err(|e| format!("{RESULT}: {e}"))?;
    Ok(bytes)
}

fn launch_id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The result directory a launch ID names, repository-relative.
fn relative_path(launch_id: &str) -> String {
    format!("{RESULT_DIR}/gui_readiness_{launch_id}")
}

/// Writes `dir/provenance.dbrec` (create-only, 0600, synced) for the
/// validated readiness result in `dir`, on the snapshot at `root`.
pub fn seal(root: &Path, dir: &Path, launch_id: &str, relative: &str) -> Result<Record, String> {
    if !launch_id_ok(launch_id) || relative != relative_path(launch_id) {
        return Err("launch ID or result path is not a GUI readiness result".into());
    }
    let result = readiness_bytes(dir)?;
    let snapshot = std::fs::read_to_string(root.join("SNAPSHOT_SHA256"))
        .map_err(|e| format!("SNAPSHOT_SHA256: {e}"))?;
    let snapshot: [u8; 32] = hex::decode(snapshot.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("SNAPSHOT_SHA256 is not a SHA-256")?;
    let commit =
        std::fs::read_to_string(root.join("COMMIT")).map_err(|e| format!("COMMIT: {e}"))?;
    let commit = commit.trim();
    if commit.is_empty() || commit.len() > 128 || commit.chars().any(char::is_control) {
        return Err("COMMIT is not a commit identity".into());
    }
    let observed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "the clock is before 1970")?
        .as_millis() as u64;
    let mut d = Draft::new(&p::SCHEMA);
    d.set(p::KIND, p::KIND_TEXT)
        .set(p::LAUNCH_ID, launch_id)
        .set(p::SOURCE_SNAPSHOT_SHA256, Value::B256(snapshot))
        .set(p::RESULT_SHA256, Value::B256(sha256(&result)))
        .set(p::SOURCE_COMMIT, commit)
        .set(p::RELATIVE_PATH, relative)
        .set(p::OBSERVED_UNIX_MS, observed)
        .set(p::QUALIFICATION_AUTHORITY, false);
    let bytes = d.encode().map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join(PROVENANCE))
        .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()))
        .map_err(|e| format!("{PROVENANCE}: {e}"))?;
    decode(&p::SCHEMA, &bytes).map_err(|e| e.to_string())
}

/// The import gate: `dir` holds a readiness result and exactly its own
/// read-only provenance (kind, no qualification authority, the result's
/// SHA-256, the directory's own launch ID and path).
pub fn check(dir: &Path) -> Result<Record, String> {
    let result = readiness_bytes(dir)?;
    let bytes = std::fs::read(dir.join(PROVENANCE)).map_err(|e| format!("{PROVENANCE}: {e}"))?;
    let r = decode(&p::SCHEMA, &bytes).map_err(|e| format!("{PROVENANCE}: {e}"))?;
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let launch_id = r.text(p::LAUNCH_ID).map_err(e)?;
    let own =
        dir.file_name().and_then(|n| n.to_str()) == Some(&format!("gui_readiness_{launch_id}"));
    if r.text(p::KIND).map_err(e)? != p::KIND_TEXT
        || r.bool(p::QUALIFICATION_AUTHORITY).map_err(e)?
        || r.b256(p::RESULT_SHA256).map_err(e)? != sha256(&result)
        || !launch_id_ok(launch_id)
        || !own
        || r.text(p::RELATIVE_PATH).map_err(e)? != relative_path(launch_id)
    {
        return Err(format!(
            "the provenance is not this read-only result's (result sha256 {})",
            hex(&sha256(&result))
        ));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = "../../../../protocol/golden/bsc/readiness.dbrec";

    fn fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("bsc-gui-prov-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("snapshot");
        let dir = base
            .join("run/bsc-operational")
            .join(relative_path("job-1"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            root.join("SNAPSHOT_SHA256"),
            format!("{}\n", "ab".repeat(32)),
        )
        .unwrap();
        std::fs::write(root.join("COMMIT"), "0123abc-dirty\n").unwrap();
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
        std::fs::copy(golden, dir.join(RESULT)).unwrap();
        (root, dir)
    }

    #[test]
    fn a_sealed_result_passes_the_import_gate_and_every_deviation_is_refused() {
        let (root, dir) = fixture("seal");
        let sealed = seal(&root, &dir, "job-1", &relative_path("job-1")).unwrap();
        assert_eq!(sealed.text(p::SOURCE_COMMIT).unwrap(), "0123abc-dirty");
        assert!(!sealed.bool(p::QUALIFICATION_AUTHORITY).unwrap());
        assert!(check(&dir).is_ok());
        // Create-only: a second seal never replaces it.
        assert!(seal(&root, &dir, "job-1", &relative_path("job-1")).is_err());
        // Another path or launch ID is refused before anything is written.
        assert!(seal(&root, &dir, "job-1", "bsc/native/evidence/g07/elsewhere").is_err());
        assert!(seal(&root, &dir, "../job", &relative_path("../job")).is_err());
        // A changed result no longer matches its provenance.
        let good = std::fs::read(dir.join(RESULT)).unwrap();
        let mut changed = good.clone();
        let n = changed.len();
        changed[n - 40] ^= 1;
        std::fs::write(dir.join(RESULT), &changed).unwrap();
        assert!(check(&dir).is_err());
        std::fs::write(dir.join(RESULT), &good).unwrap();
        // A provenance moved to another result directory is refused.
        let other = dir.with_file_name("gui_readiness_job-2");
        std::fs::create_dir_all(&other).unwrap();
        for name in [RESULT, PROVENANCE] {
            std::fs::copy(dir.join(name), other.join(name)).unwrap();
        }
        assert!(check(&other).is_err());
        // Not a readiness result: no seal.
        let (root2, dir2) = fixture("bad");
        std::fs::write(dir2.join(RESULT), b"DBREC001 not a record").unwrap();
        assert!(seal(&root2, &dir2, "job-1", &relative_path("job-1")).is_err());
    }
}
