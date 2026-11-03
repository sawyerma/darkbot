//! DEC-17e: a corrupt journal is archived — never repaired, never deleted. `p0ctl journal
//! archive --journal PATH --reason TEXT` moves a journal of a run (under the project's run roots)
//! that is not sealed — unsealed or undecodable — byte for byte into `data/archive/<digest>/`
//! beside a manifest naming its former path, SHA-256, size, seal screen, reason and date (status
//! ARCHIVED). Outside the run roots it counts toward no qualification and no boot screening; a
//! sealed journal is evidence and is never archived.
use crate::analytics::journal;
use crate::analytics::payload::Payload;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// The store of archived journals (under a project; git-ignored, P0-026).
pub const STORE: &str = "data/archive";
const MANIFEST: &str = "manifest.p0j";
/// The status of an archived journal.
pub const ARCHIVED: &str = "ARCHIVED";

/// The manifest of one archived journal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalArchive {
    /// Its former path under the project.
    pub original: String,
    pub sha256: String,
    pub bytes: u64,
    /// Its seal screen when archived (`Unsealed`, `Undecodable(..)`).
    pub screen: String,
    pub reason: String,
    pub archived_unix_s: u64,
    pub status: String,
}
impl Payload for JournalArchive {
    const KIND: u16 = journal::kind::JOURNAL_ARCHIVE;
}

fn sha(bytes: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

/// The journal's path under the project — inside a run root, a regular `.p0j` file, no symlink.
fn run_journal(project: &Path, journal: &Path) -> Result<PathBuf, String> {
    let relative = journal.strip_prefix(project).unwrap_or(journal);
    let plain = relative.is_relative()
        && relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && relative.extension().and_then(|e| e.to_str()) == Some("p0j");
    let in_run = super::boot_audit::RUN_ROOTS
        .iter()
        .any(|root| relative.starts_with(root));
    if !plain || !in_run {
        return Err(format!(
            "JOURNAL_ARCHIVE_PATH: a .p0j journal of a run under {} (project-relative)",
            super::boot_audit::RUN_ROOTS.join(", ")
        ));
    }
    let full = project.join(relative);
    match std::fs::symlink_metadata(&full) {
        Ok(m) if m.file_type().is_file() => Ok(relative.to_path_buf()),
        Ok(_) => Err("JOURNAL_ARCHIVE_PATH: a journal is a regular file".into()),
        Err(e) => Err(format!("JOURNAL_ARCHIVE_PATH: {e}")),
    }
}

/// Archives `journal` (DEC-17e) for `reason` at `now_unix_s`. Refused: no reason, a path outside
/// the run roots, a sealed journal, one archived before. The journal is moved, never rewritten;
/// should the manifest not be written it is moved back.
pub fn archive(
    project: &Path,
    journal: &Path,
    reason: &str,
    now_unix_s: u64,
) -> Result<(JournalArchive, PathBuf), String> {
    let reason = reason.trim();
    if reason.is_empty() || reason.contains('\n') {
        return Err("JOURNAL_ARCHIVE_REASON: one line naming why the journal is archived".into());
    }
    let relative = run_journal(project, journal)?;
    let full = project.join(&relative);
    let screen = journal::screen_seal(&full);
    if let journal::SealScreen::Sealed { complete } = screen {
        return Err(format!(
            "JOURNAL_ARCHIVE_SEALED: a sealed journal (complete: {complete}) is evidence, not archived"
        ));
    }
    let bytes = std::fs::read(&full).map_err(|e| format!("JOURNAL_ARCHIVE_IO: {e}"))?;
    let digest = sha(&bytes);
    let dir = project.join(STORE).join(&digest[..16]);
    std::fs::create_dir_all(project.join(STORE)).map_err(|e| format!("JOURNAL_ARCHIVE_IO: {e}"))?;
    std::fs::create_dir(&dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => {
            "JOURNAL_ARCHIVED_ALREADY: a journal of these bytes is archived".to_string()
        }
        _ => format!("JOURNAL_ARCHIVE_IO: {e}"),
    })?;
    let name = relative.file_name().ok_or("JOURNAL_ARCHIVE_PATH")?;
    let target = dir.join(name);
    std::fs::rename(&full, &target).map_err(|e| format!("JOURNAL_ARCHIVE_IO: {e}"))?;
    let record = JournalArchive {
        original: relative.to_string_lossy().into(),
        sha256: digest,
        bytes: bytes.len() as u64,
        screen: format!("{screen:?}"),
        reason: reason.into(),
        archived_unix_s: now_unix_s,
        status: ARCHIVED.into(),
    };
    let moved = std::fs::read(&target).map(|b| sha(&b) == record.sha256);
    let written =
        journal::create_typed_value(&dir.join(MANIFEST), journal::source::BOOT_CONTROL, &record);
    match (moved, written) {
        (Ok(true), Ok(())) => Ok((record, target)),
        (moved, written) => {
            // Nothing half-archived: the journal goes back where it was.
            let _ = std::fs::rename(&target, &full);
            Err(format!(
                "JOURNAL_ARCHIVE_IO: not archived (moved intact: {moved:?}, manifest: {:?})",
                written.err()
            ))
        }
    }
}

/// The archived journals of `project` with their manifests, oldest first.
pub fn archived(project: &Path) -> Result<Vec<(PathBuf, JournalArchive)>, String> {
    let root = project.join(STORE);
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(format!("JOURNAL_ARCHIVE_IO: {e}")),
    };
    for e in entries.flatten() {
        let manifest = e.path().join(MANIFEST);
        if let Ok(m) = journal::read_typed_value::<JournalArchive>(&manifest) {
            out.push((e.path(), m));
        }
    }
    out.sort_by_key(|(_, m)| (m.archived_unix_s, m.sha256.clone()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::journal::Writer;

    fn project(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "p0-archive-{tag}-{}-{}",
            std::process::id(),
            solana_sdk::pubkey::Pubkey::new_unique()
        ));
        std::fs::create_dir_all(p.join("data/runs/r1")).unwrap();
        p
    }

    fn journal(p: &Path, name: &str, seal: Option<bool>) -> PathBuf {
        let path = p.join("data/runs/r1").join(name);
        let mut w = Writer::create(&path).unwrap();
        w.test_append_historical(journal::kind::OWNER_RECORD, &serde_json::json!({"a": 1}))
            .unwrap();
        match seal {
            Some(c) => {
                w.seal(c).unwrap();
            }
            None => {
                w.sync().unwrap();
            }
        }
        path
    }

    /// DEC-17e: an unsealed journal is moved byte for byte beside its manifest (ARCHIVED), listed,
    /// gone from the run; a torn one too.
    #[test]
    fn a_corrupt_journal_is_archived_never_rewritten() {
        let p = project("move");
        let path = journal(&p, "owner.p0j", None);
        let bytes = std::fs::read(&path).unwrap();
        let (m, target) = archive(&p, &path, "torn after a crash", 5).unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert_eq!(
            (m.status.as_str(), m.original.as_str()),
            (ARCHIVED, "data/runs/r1/owner.p0j")
        );
        assert_eq!(m.sha256, sha(&bytes));
        let torn = journal(&p, "torn.p0j", None);
        let mut b = std::fs::read(&torn).unwrap();
        b.truncate(b.len() - 3);
        std::fs::write(&torn, &b).unwrap();
        let (m2, _) = archive(&p, &torn, "torn tail", 6).unwrap();
        assert!(m2.screen.starts_with("Undecodable"), "{}", m2.screen);
        let listed = archived(&p).unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|(_, m)| m.archived_unix_s)
                .collect::<Vec<_>>(),
            vec![5, 6]
        );
        let _ = std::fs::remove_dir_all(&p);
    }

    /// Refused, nothing moved: a sealed journal (complete or not), no reason, outside the run
    /// roots, a symlink.
    #[test]
    fn evidence_and_foreign_paths_are_never_archived() {
        let p = project("refuse");
        for complete in [true, false] {
            let sealed = journal(&p, &format!("sealed-{complete}.p0j"), Some(complete));
            let e = archive(&p, &sealed, "r", 1).unwrap_err();
            assert!(e.starts_with("JOURNAL_ARCHIVE_SEALED"), "{e}");
            assert!(sealed.exists());
        }
        let open = journal(&p, "open.p0j", None);
        assert!(archive(&p, &open, "  ", 1)
            .unwrap_err()
            .starts_with("JOURNAL_ARCHIVE_REASON"));
        std::fs::create_dir_all(p.join("data/other")).unwrap();
        let foreign = p.join("data/other/x.p0j");
        std::fs::copy(&open, &foreign).unwrap();
        assert!(archive(&p, &foreign, "r", 1)
            .unwrap_err()
            .starts_with("JOURNAL_ARCHIVE_PATH"));
        assert!(archive(&p, Path::new("data/runs/../other/x.p0j"), "r", 1)
            .unwrap_err()
            .starts_with("JOURNAL_ARCHIVE_PATH"));
        let link = p.join("data/runs/r1/link.p0j");
        std::os::unix::fs::symlink(&foreign, &link).unwrap();
        assert!(archive(&p, &link, "r", 1)
            .unwrap_err()
            .starts_with("JOURNAL_ARCHIVE_PATH"));
        assert!(open.exists() && foreign.exists());
        let _ = std::fs::remove_dir_all(&p);
    }
}
