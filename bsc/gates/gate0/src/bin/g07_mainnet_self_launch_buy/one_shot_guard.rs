//! Durable local G0.7 attempt latch. A submission or ambiguous transport outcome
//! must not become a second economic attempt merely by restarting the process.
use serde_json::json;
use std::{fs::{self, OpenOptions}, io::Write, path::Path};

pub const GUARD_PATH: &str = "bsc/local_diagnostics/g07_mainnet_one_shot.json";

pub fn reserve_once(
    path: &Path,
    attempt_id: &str,
    contract_sha256: &str,
    launch_hash: &str,
    buy_hash: &str,
) -> Result<(), String> {
    if attempt_id.is_empty() || contract_sha256.is_empty()
        || launch_hash.is_empty() || buy_hash.is_empty()
    {
        return Err("G07_ONE_SHOT_INVALID_IDENTITY".into());
    }
    let parent = path.parent().ok_or("G07_ONE_SHOT_NO_PARENT")?;
    fs::create_dir_all(parent).map_err(|e| format!("G07_ONE_SHOT_DIR: {e}"))?;
    // O_EXCL-style atomic creation; never truncate or replace a prior attempt.
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)
        .map_err(|e| format!(
            "G07_ONE_SHOT_ALREADY_RESERVED_OR_UNSAFE: {}: {e}; reconcile before any other attempt",
            path.display()
        ))?;
    let record = json!({
        "schema": "darkbot.g07.one-shot.v1",
        "attempt_id": attempt_id,
        "approved_contract_sha256": contract_sha256,
        "launch_tx_hash": launch_hash,
        "buy_tx_hash": buy_hash,
        "status": "POSSIBLY_SUBMITTED_RECONCILIATION_REQUIRED",
        "auto_retry": false
    });
    let contents = serde_json::to_vec_pretty(&record)
        .map_err(|e| format!("G07_ONE_SHOT_SERIALIZE: {e}"))?;
    file.write_all(&contents)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("G07_ONE_SHOT_SYNC_FAILED: {e}; do not send"))?;
    // Keep the newly created filename durable across a crash as well.
    fs::File::open(parent).and_then(|directory| directory.sync_all())
        .map_err(|e| format!("G07_ONE_SHOT_DIRECTORY_SYNC_FAILED: {e}; do not send"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_attempt_is_rejected_even_with_changed_attempt_id() {
        let root = std::env::temp_dir().join(format!(
            "darkbot-g07-guard-{}-{}", std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let path = root.join("one_shot.json");
        reserve_once(&path, "first", "approved", "launch", "buy").unwrap();
        let before = fs::read(&path).unwrap();
        let error = reserve_once(&path, "second", "approved", "other", "other").unwrap_err();
        assert!(error.contains("G07_ONE_SHOT_ALREADY_RESERVED"));
        assert_eq!(before, fs::read(&path).unwrap());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }
    #[test]
    fn identity_fields_are_mandatory() {
        let path = std::env::temp_dir().join("darkbot-g07-guard-invalid");
        assert!(reserve_once(&path, "", "contract", "launch", "buy").is_err());
        assert!(!path.exists());
    }
}
