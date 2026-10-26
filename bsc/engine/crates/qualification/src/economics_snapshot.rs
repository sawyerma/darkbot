//! Phase G (Economics gate §15): freezing and verifying the versioned
//! economics snapshot of `darkbot_bsc_trading::economics::snapshot`.
//!
//! The snapshot hash is the sha256 of the canonical manifest: the model
//! version, the bytes of every bound file (read from the repository at
//! `root`), the scopes and the policies. A frozen snapshot verifies only
//! while all of them are unchanged; any change of the economics needs a
//! new snapshot (`g_economics_snapshot`), so replay never binds silently
//! rewritten semantics.

use crate::c1_common::{sha256, sha256_file};
use darkbot_bsc_record::schema::bsc_economics::economics_snapshot as es;
use darkbot_bsc_record::{Draft, Value, decode};
use darkbot_bsc_trading::economics::snapshot::{
    BOUND_FILES, CHAIN_ID, MODEL_VERSION, POLICIES, SCOPES, manifest,
};
use std::path::Path;

/// The bound files of the tree at `root` with their sha256.
pub fn bound_files(root: &Path) -> Result<Vec<(&'static str, [u8; 32])>, String> {
    BOUND_FILES
        .iter()
        .map(|p| {
            sha256_file(&root.join(p))
                .map(|h| (*p, h))
                .map_err(|e| format!("{p}: {e}"))
        })
        .collect()
}

/// The snapshot hash of the tree at `root`.
pub fn current(root: &Path) -> Result<[u8; 32], String> {
    Ok(sha256(manifest(&bound_files(root)?).as_bytes()))
}

/// The snapshot record of the tree at `root` and its hash.
pub fn freeze(
    root: &Path,
    observed_unix_ms: u64,
    git_head: &str,
) -> Result<(Vec<u8>, [u8; 32]), String> {
    let files = bound_files(root)?;
    let hash = sha256(manifest(&files).as_bytes());
    let mut d = Draft::new(&es::SCHEMA);
    d.set(es::SNAPSHOT_SHA256, Value::B256(hash))
        .set(es::CHAIN_ID, CHAIN_ID)
        .set(es::PROVENANCE_TOOL, "g_economics_snapshot")
        .set(es::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
        .set(es::MODEL_VERSION, MODEL_VERSION)
        .set(es::GIT_HEAD, git_head);
    for (path, sha) in &files {
        let mut f = Draft::new(&es::file::SCHEMA);
        f.set(es::file::PATH, *path)
            .set(es::file::SHA256, Value::B256(*sha));
        d.push(es::FILES, f.into_nested());
    }
    for s in SCOPES {
        let mut row = Draft::new(&es::scope::SCHEMA);
        row.set(es::scope::VENUE, s.venue)
            .set(es::scope::SURFACE, s.surface)
            .set(es::scope::PATH, s.path)
            .set(es::scope::SUPPORT, s.support.name())
            .set(es::scope::RULE, s.rule);
        d.push(es::SCOPES, row.into_nested());
    }
    for (name, rule) in POLICIES {
        let mut row = Draft::new(&es::policy::SCHEMA);
        row.set(es::policy::NAME, *name)
            .set(es::policy::RULE, *rule);
        d.push(es::POLICIES, row.into_nested());
    }
    d.push(
        es::NON_CLAIMS,
        "NO_ECONOMIC_CLAIM: what is priced and costed, never an expected value; no strategy tuning",
    );
    let bytes = d.encode().map_err(|e| format!("economics snapshot: {e}"))?;
    Ok((bytes, hash))
}

/// Whether the frozen snapshot `bytes` is the snapshot of the tree at
/// `root`; otherwise the bound files that changed (none when only the
/// scopes, policies or model version did).
pub fn verify(root: &Path, bytes: &[u8]) -> Result<Result<[u8; 32], Vec<String>>, String> {
    let r = decode(&es::SCHEMA, bytes).map_err(|e| format!("economics snapshot: {e}"))?;
    let recorded = r
        .b256(es::SNAPSHOT_SHA256)
        .map_err(|e| format!("economics snapshot: {e}"))?;
    let files = bound_files(root)?;
    if sha256(manifest(&files).as_bytes()) == recorded {
        return Ok(Ok(recorded));
    }
    let frozen = r
        .nested_all(es::FILES)
        .map_err(|e| format!("economics snapshot: {e}"))?;
    let changed = files
        .iter()
        .filter(|(path, sha)| {
            !frozen.iter().any(|f| {
                f.text(es::file::PATH).ok() == Some(*path)
                    && f.b256(es::file::SHA256).ok() == Some(*sha)
            })
        })
        .map(|(path, _)| (*path).to_string())
        .collect();
    Ok(Err(changed))
}
