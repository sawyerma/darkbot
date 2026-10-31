//! The LIVE key (plan 28.10.): the buyer wallet's secp256k1 key, in a file
//! only its owner can read, outside the repository. It is loaded only under
//! an owner's [`LiveNow`] (an effective LIVE), must be the buyer address
//! the operator configured and never the PAPER identity, and is held by the
//! LIVE end alone: the bytes read are zeroized at once, the signer's key
//! when the end is dropped. The file is only read, never written, moved or
//! deleted.
//!
//! BSC3-026: the loopback stand-in signs only with its own test identity,
//! a key drawn for it in the test's owner state (`loopback-identity.key`),
//! never a wallet's key: the route refuses any other key file before it is
//! read, and any of the refused production accounts.

use crate::paper::{IdentityError, PaperIdentity, load_private_key};
use crate::sources::LiveNow;
use darkbot_bsc_trading::signer::Signer;
use std::path::{Path, PathBuf};

/// The loopback stand-in's test identity in the test's owner state.
pub const LOOPBACK_IDENTITY: &str = "loopback-identity.key";

/// Where the loopback test identity of the owner state `state` is.
pub fn loopback_identity(state: &Path) -> PathBuf {
    state.join(LOOPBACK_IDENTITY)
}

/// Draws the loopback test identity of the owner state `state` (never the
/// account's, `refused` says why) and returns its address; refuses an
/// existing one.
pub fn create_loopback_identity(
    state: &Path,
    repo_root: &Path,
    refused: Option<String>,
) -> Result<[u8; 20], String> {
    if let Some(why) = refused {
        return Err(why);
    }
    PaperIdentity::create(&loopback_identity(state), repo_root).map_err(|e| e.to_string())
}

/// BSC3-026: the stand-in route signs only with the test identity of its
/// owner state; `key_path` is refused (before any key is read) unless it
/// names that very file: both names regular files (never a link to another
/// key) of one inode. The route then loads the identity's own name.
pub fn loopback_key_refused(key_path: &Path, state: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let file = |p: &Path| {
        std::fs::symlink_metadata(p)
            .ok()
            .filter(|m| m.file_type().is_file())
            .map(|m| (m.dev(), m.ino()))
    };
    let identity = loopback_identity(state);
    match (file(key_path), file(&identity)) {
        (Some(k), Some(i)) if k == i => None,
        _ => Some(format!(
            "LOOPBACK_KEY_NOT_TEST_IDENTITY: the stand-in signs only with {} (bsc_runtime live loopback-identity init --state DIR)",
            identity.display()
        )),
    }
}

/// The buyer wallet's signer for one LIVE arm. `Debug` shows the address
/// only.
pub struct LiveKey {
    signer: Signer,
}

impl core::fmt::Debug for LiveKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LiveKey")
            .field("address", &self.address())
            .finish_non_exhaustive()
    }
}

fn refused(e: IdentityError) -> String {
    match e {
        IdentityError::Location(m) => format!("LIVE key location refused: {m}"),
        IdentityError::Permissions(m) => format!("LIVE key permissions refused: {m}"),
        IdentityError::Malformed(m) => format!("LIVE key malformed: {m}"),
        IdentityError::Exists(m) | IdentityError::ProductionAccount(m) | IdentityError::Io(m) => {
            format!("LIVE key: {m}")
        }
    }
}

impl LiveKey {
    /// Loads the key at `path` for the LIVE `armed`: a private key file
    /// outside `repo_root` whose address is `buyer`, never one of `refused`
    /// (the PAPER identity; on the stand-in route also the production
    /// accounts).
    pub fn load(
        path: &Path,
        repo_root: &Path,
        buyer: [u8; 20],
        refused_accounts: &[[u8; 20]],
        _armed: &LiveNow,
    ) -> Result<Self, String> {
        let signer = load_private_key(path, repo_root).map_err(refused)?;
        let address = signer.address().into_array();
        if address != buyer {
            return Err("LIVE key is not the configured buyer wallet".into());
        }
        if refused_accounts.contains(&address) {
            return Err("LIVE key is a refused account".into());
        }
        Ok(LiveKey { signer })
    }

    pub fn address(&self) -> [u8; 20] {
        self.signer.address().into_array()
    }

    pub fn signer(&self) -> &Signer {
        &self.signer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::ComponentId;
    use crate::sources::tests::{live_ready, qualified};
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

    /// Plan 28.10.: the LIVE key loads only as a private file outside the
    /// repository, as the configured buyer, never as the PAPER identity;
    /// its file stays as it was.
    #[test]
    fn the_live_key_is_the_private_buyer_key_outside_the_repository() {
        let (state, s, now) = live_ready("live-key");
        let armed = s
            .live_now(&[ComponentId::ExecC3], &qualified(), now)
            .unwrap();
        let home = std::env::temp_dir().join(format!("bsc-live-key-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let dir = home.join("keys");
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&dir)
            .unwrap();
        let path = dir.join("buyer.key");
        let secret = [0x22u8; 32];
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut f| f.write_all(&secret))
            .unwrap();
        let buyer = Signer::from_secret_bytes(&secret)
            .unwrap()
            .address()
            .into_array();
        let repo = Path::new("/nonexistent-repo");
        let key = LiveKey::load(&path, repo, buyer, &[], &armed).unwrap();
        assert_eq!(key.address(), buyer);
        assert!(!format!("{key:?}").contains("22222222"));
        assert!(
            LiveKey::load(&path, repo, [9; 20], &[], &armed)
                .unwrap_err()
                .contains("not the configured buyer")
        );
        assert!(
            LiveKey::load(&path, repo, buyer, &[buyer], &armed)
                .unwrap_err()
                .contains("refused account")
        );
        assert!(
            LiveKey::load(&path, &home, buyer, &[], &armed)
                .unwrap_err()
                .contains("inside the repository")
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            LiveKey::load(&path, repo, buyer, &[], &armed)
                .unwrap_err()
                .contains("permissions refused")
        );
        // Only read: the file is still there, unchanged.
        assert_eq!(fs::read(&path).unwrap(), secret);
        fs::remove_dir_all(home).unwrap();
        fs::remove_dir_all(state).unwrap();
    }

    /// BSC3-026: the stand-in's test identity is drawn once in a test's
    /// owner state, a private key file outside the repository; never in a
    /// state the route refuses (the account's), never twice; only that file
    /// passes the route's key check.
    #[test]
    fn the_loopback_identity_is_drawn_once_in_a_test_state() {
        let state = std::env::temp_dir().join(format!("bsc-loopback-id-{}", std::process::id()));
        let _ = fs::remove_dir_all(&state);
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&state)
            .unwrap();
        let repo = Path::new("/nonexistent-repo");
        // The account's state: nothing is written.
        let why = create_loopback_identity(&state, repo, Some("LOOPBACK_ON_ACCOUNT_STATE".into()))
            .unwrap_err();
        assert!(why.starts_with("LOOPBACK_ON_ACCOUNT_STATE"), "{why}");
        assert!(!loopback_identity(&state).exists());
        // A key file elsewhere, or none yet: refused.
        assert!(loopback_key_refused(&state.join("buyer.key"), &state).is_some());
        assert!(loopback_key_refused(&loopback_identity(&state), &state).is_some());
        let address = create_loopback_identity(&state, repo, None).unwrap();
        let path = loopback_identity(&state);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(
            PaperIdentity::load(&path, repo, &[]).unwrap().address(),
            address
        );
        assert_eq!(loopback_key_refused(&path, &state), None);
        // Spelled another way, the same file.
        let spelled = state.join(".").join(LOOPBACK_IDENTITY);
        assert_eq!(loopback_key_refused(&spelled, &state), None);
        // A link to it is no key file of its own.
        let link = state.join("link.key");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(loopback_key_refused(&link, &state).is_some());
        // Never drawn again over it.
        assert!(create_loopback_identity(&state, repo, None).is_err());
        assert_eq!(
            PaperIdentity::load(&path, repo, &[]).unwrap().address(),
            address
        );
        // Inside the repository: refused.
        let inside = state.join("repo");
        fs::DirBuilder::new().mode(0o700).create(&inside).unwrap();
        assert!(create_loopback_identity(&inside, &inside, None).is_err());
        fs::remove_dir_all(state).unwrap();
    }
}
