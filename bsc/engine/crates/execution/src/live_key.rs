//! The LIVE key (plan 28.10.): the buyer wallet's secp256k1 key, in a file
//! only its owner can read, outside the repository. It is loaded only under
//! an owner's [`LiveNow`] (an effective LIVE), must be the buyer address
//! the operator configured and never the PAPER identity, and is held by the
//! LIVE end alone: the bytes read are zeroized at once, the signer's key
//! when the end is dropped. The file is only read, never written, moved or
//! deleted.

use crate::paper::{IdentityError, load_private_key};
use crate::sources::LiveNow;
use darkbot_bsc_trading::signer::Signer;
use std::path::Path;

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
    /// (the PAPER identity).
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
            return Err("LIVE key is a refused account (the PAPER identity)".into());
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
}
