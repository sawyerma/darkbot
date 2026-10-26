//! The PAPER identity and the PAPER sink (order V2 §7.4, PHASENPLAN §16).
//!
//! PAPER runs the production path up to and including signing and
//! serialization, then hands the signed bytes to [`PaperSink`]: the
//! `T_shadow_sink` boundary. It is a real operating mode, not a stand-in
//! for missing code, and it is never `T_send`.
//!
//! The identity is a dedicated key generated locally from the operating
//! system's randomness, stored outside the repository in a file only its
//! owner can read, never a production or G0.7 wallet and never funded.
//!
//! The sink has no transport: this module names no socket, client,
//! adapter or provider (a test reads this source and refuses any), so
//! signed PAPER bytes have no way to a Mainnet provider. They stay in the
//! local journal as test artefacts, not as a release for Mainnet egress.

use darkbot_bsc_trading::signer::Signer;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

/// Why an identity was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// The path is relative or inside the repository.
    Location(String),
    /// A file already exists where a new identity was to be created.
    Exists(String),
    /// Group or other may access the file or its directory.
    Permissions(String),
    /// The file is not exactly 32 bytes of a valid secp256k1 key.
    Malformed(String),
    /// The key belongs to a production account.
    ProductionAccount(String),
    Io(String),
}

impl core::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Location(m) => write!(f, "paper identity location refused: {m}"),
            Self::Exists(m) => write!(f, "paper identity exists: {m}"),
            Self::Permissions(m) => write!(f, "paper identity permissions refused: {m}"),
            Self::Malformed(m) => write!(f, "paper identity malformed: {m}"),
            Self::ProductionAccount(m) => {
                write!(f, "paper identity is a production account: {m}")
            }
            Self::Io(m) => write!(f, "paper identity: {m}"),
        }
    }
}

impl std::error::Error for IdentityError {}

/// The PAPER signing identity. `Debug` shows the address only.
pub struct PaperIdentity {
    signer: Signer,
    path: PathBuf,
}

impl core::fmt::Debug for PaperIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PaperIdentity")
            .field("address", &self.address())
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// `path` must be absolute and outside `repo_root` (also through links of
/// its directory).
fn check_location(path: &Path, repo_root: &Path) -> Result<(), IdentityError> {
    if !path.is_absolute() {
        return Err(IdentityError::Location(format!(
            "{} is not absolute",
            path.display()
        )));
    }
    let root = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let dir = path.parent().unwrap_or(Path::new("/"));
    let dir = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if dir.starts_with(&root) || path.starts_with(repo_root) {
        return Err(IdentityError::Location(format!(
            "{} is inside the repository",
            path.display()
        )));
    }
    Ok(())
}

fn private(meta: &fs::Metadata) -> bool {
    meta.permissions().mode() & 0o077 == 0
}

impl PaperIdentity {
    /// Creates a new identity at `path` from the operating system's
    /// randomness and returns its address. Refuses an existing file; the
    /// directory is created owner-only.
    pub fn create(path: &Path, repo_root: &Path) -> Result<[u8; 20], IdentityError> {
        check_location(path, repo_root)?;
        let io = |e: std::io::Error| IdentityError::Io(format!("{}: {e}", path.display()));
        if let Some(dir) = path.parent() {
            fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(dir)
                .map_err(io)?;
            if !private(&fs::metadata(dir).map_err(io)?) {
                return Err(IdentityError::Permissions(format!(
                    "{} is accessible to others",
                    dir.display()
                )));
            }
        }
        if path.exists() {
            return Err(IdentityError::Exists(path.display().to_string()));
        }
        let mut secret = [0u8; 32];
        let signer = loop {
            fs::File::open("/dev/urandom")
                .and_then(|mut f| f.read_exact(&mut secret))
                .map_err(io)?;
            // Zero or above the curve order is not a key: draw again.
            if let Ok(signer) = Signer::from_secret_bytes(&secret) {
                break signer;
            }
        };
        let written = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut f| f.write_all(&secret).and_then(|()| f.sync_all()));
        secret.zeroize();
        written.map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => IdentityError::Exists(path.display().to_string()),
            _ => io(e),
        })?;
        Ok(signer.address().into_array())
    }

    /// Loads the identity at `path`: owner-only file and directory, exactly
    /// 32 bytes of a valid key, never one of the `production` accounts.
    pub fn load(
        path: &Path,
        repo_root: &Path,
        production: &[[u8; 20]],
    ) -> Result<Self, IdentityError> {
        check_location(path, repo_root)?;
        let io = |e: std::io::Error| IdentityError::Io(format!("{}: {e}", path.display()));
        let meta = fs::symlink_metadata(path).map_err(io)?;
        if !meta.is_file() || !private(&meta) {
            return Err(IdentityError::Permissions(format!(
                "{} is not a private regular file",
                path.display()
            )));
        }
        if let Some(dir) = path.parent()
            && !private(&fs::metadata(dir).map_err(io)?)
        {
            return Err(IdentityError::Permissions(format!(
                "{} is accessible to others",
                dir.display()
            )));
        }
        let mut bytes = fs::read(path).map_err(io)?;
        let mut secret = [0u8; 32];
        let len = bytes.len();
        if len == 32 {
            secret.copy_from_slice(&bytes);
        }
        bytes.zeroize();
        if len != 32 {
            return Err(IdentityError::Malformed(format!("{len} bytes, not 32")));
        }
        let signer = Signer::from_secret_bytes(&secret);
        secret.zeroize();
        let signer = signer.map_err(|_| IdentityError::Malformed("not a secp256k1 key".into()))?;
        let address = signer.address().into_array();
        if production.contains(&address) {
            return Err(IdentityError::ProductionAccount(format!(
                "0x{}",
                alloy_primitives::hex::encode(address)
            )));
        }
        Ok(PaperIdentity {
            signer,
            path: path.to_path_buf(),
        })
    }

    pub fn address(&self) -> [u8; 20] {
        self.signer.address().into_array()
    }

    /// The signer of the PAPER path.
    pub fn signer(&self) -> &Signer {
        &self.signer
    }
}

/// One signed transaction handed to the sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaperHandoff {
    pub sequence: u64,
    pub tx_hash: [u8; 32],
    pub raw_len: usize,
    /// When the sink took it, on the runtime's monotonic clock
    /// (`T_shadow_sink`).
    pub at_ns: u64,
}

/// The end of the PAPER path. It owns no transport and holds no bytes: the
/// caller journals the handoff locally.
#[derive(Debug, Default)]
pub struct PaperSink {
    handoffs: u64,
    bytes: u64,
}

impl PaperSink {
    /// Takes one signed transaction (its hash and raw bytes) at `at_ns`.
    pub fn handoff(&mut self, tx_hash: [u8; 32], raw: &[u8], at_ns: u64) -> PaperHandoff {
        self.handoffs += 1;
        self.bytes = self.bytes.saturating_add(raw.len() as u64);
        PaperHandoff {
            sequence: self.handoffs,
            tx_hash,
            raw_len: raw.len(),
            at_ns,
        }
    }

    pub fn handoffs(&self) -> u64 {
        self.handoffs
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("bsc-paper-{name}-{}", std::process::id()))
            .join("identity");
        let _ = fs::remove_dir_all(dir.parent().unwrap());
        dir.join("paper.key")
    }

    fn repo() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// V2 §7.4: the sink has no way to any provider. Its source names no
    /// transport, socket, client or adapter of this crate.
    #[test]
    fn the_sink_module_has_no_transport() {
        let source = include_str!("paper.rs");
        let code = &source[..source.find("#[cfg(test)]").unwrap()];
        for forbidden in [
            "TcpStream",
            "UdpSocket",
            "UnixStream",
            "std::net",
            "crate::wss",
            "crate::submission",
            "crate::chain_follower",
            "geth_ipc",
            "tungstenite",
            "rpc_wire",
            "Command::new",
        ] {
            assert!(!code.contains(forbidden), "{forbidden}");
        }
    }

    #[test]
    fn a_created_identity_loads_privately_and_never_twice() {
        let path = temp("create");
        let address = PaperIdentity::create(&path, &repo()).unwrap();
        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.len(), 32);
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let id = PaperIdentity::load(&path, &repo(), &[]).unwrap();
        assert_eq!(id.address(), address);
        assert!(
            !format!("{id:?}").contains(&alloy_primitives::hex::encode(fs::read(&path).unwrap()))
        );
        assert!(matches!(
            PaperIdentity::create(&path, &repo()),
            Err(IdentityError::Exists(_))
        ));
        // Never a production account.
        assert!(matches!(
            PaperIdentity::load(&path, &repo(), &[address]),
            Err(IdentityError::ProductionAccount(_))
        ));
        fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn exposed_malformed_or_misplaced_identities_are_refused() {
        let path = temp("refuse");
        PaperIdentity::create(&path, &repo()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            PaperIdentity::load(&path, &repo(), &[]),
            Err(IdentityError::Permissions(_))
        ));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            PaperIdentity::load(&path, &repo(), &[]),
            Err(IdentityError::Permissions(_))
        ));
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(&path, [7u8; 31]).unwrap();
        assert!(matches!(
            PaperIdentity::load(&path, &repo(), &[]),
            Err(IdentityError::Malformed(_))
        ));
        fs::write(&path, [0u8; 32]).unwrap();
        assert!(matches!(
            PaperIdentity::load(&path, &repo(), &[]),
            Err(IdentityError::Malformed(_))
        ));
        // Inside the repository, or relative: refused before any read.
        let inside = repo().join("bsc/.local/paper.key");
        assert!(matches!(
            PaperIdentity::create(&inside, &repo()),
            Err(IdentityError::Location(_))
        ));
        assert!(matches!(
            PaperIdentity::load(Path::new("paper.key"), &repo(), &[]),
            Err(IdentityError::Location(_))
        ));
        fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn the_sink_counts_handoffs_in_order() {
        let mut sink = PaperSink::default();
        let a = sink.handoff([1; 32], &[0u8; 110], 5);
        let b = sink.handoff([2; 32], &[0u8; 120], 9);
        assert_eq!((a.sequence, b.sequence, b.raw_len, b.at_ns), (1, 2, 120, 9));
        assert_eq!((sink.handoffs(), sink.bytes()), (2, 230));
    }
}
