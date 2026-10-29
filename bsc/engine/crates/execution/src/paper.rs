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
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
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

/// `path` as the file system will resolve it: its nearest existing
/// ancestor canonicalized (links resolved), the rest appended with `.` and
/// `..` applied. No directory is created.
fn resolved(path: &Path) -> PathBuf {
    let mut existing = path;
    let mut rest = Vec::new();
    while !existing.exists() {
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                existing = parent;
            }
            // A `..` or `.` tail: resolve it from its parent.
            (Some(parent), None) => {
                rest.push(
                    existing
                        .components()
                        .next_back()
                        .map_or_else(std::ffi::OsString::new, |c| c.as_os_str().to_owned()),
                );
                existing = parent;
            }
            (None, _) => break,
        }
    }
    let mut out = fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    for part in rest.iter().rev() {
        match part.to_str() {
            Some("..") => {
                out.pop();
            }
            Some(".") => {}
            _ => out.push(part),
        }
    }
    out
}

/// `path` must be absolute and outside `repo_root` (also through links or
/// `..` of its directories, existing or not).
fn check_location(path: &Path, repo_root: &Path) -> Result<(), IdentityError> {
    if !path.is_absolute() {
        return Err(IdentityError::Location(format!(
            "{} is not absolute",
            path.display()
        )));
    }
    let root = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    if resolved(path).starts_with(&root) || path.starts_with(repo_root) {
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

/// The key's directory `dir`: owner-only, and no directory on its way
/// (as written and as resolved) can be changed by another user: each
/// belongs to the key directory's owner or root and is not writable by
/// group or others unless sticky. Returns that owner: a process that opens
/// a file inside an owner-only directory is its owner (or root).
fn key_directory(dir: &Path) -> Result<u32, IdentityError> {
    let io = |e: std::io::Error| IdentityError::Io(format!("{}: {e}", dir.display()));
    let real = fs::canonicalize(dir).map_err(io)?;
    let meta = fs::metadata(&real).map_err(io)?;
    if !meta.is_dir() || !private(&meta) {
        return Err(IdentityError::Permissions(format!(
            "{} is accessible to others",
            dir.display()
        )));
    }
    let owner = meta.uid();
    for a in dir.ancestors().chain(real.ancestors()) {
        // A link on the written path is the owner's or root's (its target
        // is on the resolved path); a directory is checked itself.
        let m = fs::symlink_metadata(a).map_err(io)?;
        let others_write =
            !m.file_type().is_symlink() && m.mode() & 0o022 != 0 && m.mode() & 0o1000 == 0;
        if (m.uid() != owner && m.uid() != 0) || others_write {
            return Err(IdentityError::Permissions(format!(
                "{} can be changed by another user",
                a.display()
            )));
        }
    }
    Ok(owner)
}

/// The signer of the key file at `path`: absolute and outside `repo_root`,
/// in an owner-only directory no other user can change (BSC3-067), an
/// owner-only regular file of that owner with no other name, exactly 32
/// bytes of a valid secp256k1 key. The name is checked, then the file opened
/// once and checked on its descriptor: the same inode as the name, so a link
/// swapped in between is never read. The bytes read are zeroized at once;
/// the signer's key is zeroized when it is dropped. The file is only read.
pub(crate) fn load_private_key(path: &Path, repo_root: &Path) -> Result<Signer, IdentityError> {
    check_location(path, repo_root)?;
    let io = |e: std::io::Error| IdentityError::Io(format!("{}: {e}", path.display()));
    let refused = || {
        IdentityError::Permissions(format!(
            "{} is not a private regular file of the directory's owner",
            path.display()
        ))
    };
    let owner = key_directory(path.parent().ok_or_else(refused)?)?;
    let named = fs::symlink_metadata(path).map_err(io)?;
    if !named.is_file() {
        return Err(refused());
    }
    let mut file = fs::File::open(path).map_err(io)?;
    let opened = file.metadata().map_err(io)?;
    if (opened.dev(), opened.ino()) != (named.dev(), named.ino())
        || !opened.is_file()
        || !private(&opened)
        || opened.uid() != owner
        || opened.nlink() != 1
    {
        return Err(refused());
    }
    if opened.len() != 32 {
        return Err(IdentityError::Malformed(format!(
            "{} bytes, not 32",
            opened.len()
        )));
    }
    let mut secret = [0u8; 32];
    let mut more = [0u8; 1];
    let read = file
        .read_exact(&mut secret)
        .and_then(|()| file.read(&mut more));
    if !matches!(read, Ok(0)) {
        secret.zeroize();
        more.zeroize();
        return Err(match read {
            Err(e) => io(e),
            Ok(_) => IdentityError::Malformed("more than 32 bytes".into()),
        });
    }
    let signer = Signer::from_secret_bytes(&secret);
    secret.zeroize();
    signer.map_err(|_| IdentityError::Malformed("not a secp256k1 key".into()))
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
            // Once the directory exists, its real location is checked again.
            check_location(path, repo_root)?;
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
        let signer = load_private_key(path, repo_root)?;
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

    /// BSC3-067: the key is read only through its own name: never through a
    /// symbolic link, a second name or a directory another user can change,
    /// and never a file swapped in between the check and the read.
    #[test]
    fn a_key_is_read_only_through_its_own_name() {
        let path = temp("links");
        let address = PaperIdentity::create(&path, &repo()).unwrap();
        let dir = path.parent().unwrap();
        let base = dir.parent().unwrap();
        // Another identity elsewhere, reached through a link.
        let other = base.join("other/paper.key");
        PaperIdentity::create(&other, &repo()).unwrap();
        let linked = dir.join("linked.key");
        std::os::unix::fs::symlink(&other, &linked).unwrap();
        assert!(matches!(
            PaperIdentity::load(&linked, &repo(), &[]),
            Err(IdentityError::Permissions(_))
        ));
        // A second name of the key.
        let second = base.join("other/second.key");
        fs::hard_link(&path, &second).unwrap();
        assert!(matches!(
            PaperIdentity::load(&path, &repo(), &[]),
            Err(IdentityError::Permissions(_))
        ));
        fs::remove_file(&second).unwrap();
        assert_eq!(
            PaperIdentity::load(&path, &repo(), &[]).unwrap().address(),
            address
        );
        // Through a link of the owner on the written path: its directory.
        let alias = base.join("alias");
        std::os::unix::fs::symlink(dir, &alias).unwrap();
        assert_eq!(
            PaperIdentity::load(&alias.join("paper.key"), &repo(), &[])
                .unwrap()
                .address(),
            address
        );
        // A directory on the way that others may change; sticky is fine.
        fs::set_permissions(base, fs::Permissions::from_mode(0o777)).unwrap();
        let e = PaperIdentity::load(&path, &repo(), &[]).unwrap_err();
        assert!(
            e.to_string().contains("can be changed by another user"),
            "{e}"
        );
        fs::set_permissions(base, fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(PaperIdentity::load(&path, &repo(), &[]).is_ok());
        fs::set_permissions(base, fs::Permissions::from_mode(0o700)).unwrap();
        // More than 32 bytes.
        let long = dir.join("long.key");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&long)
            .and_then(|mut f| f.write_all(&[7u8; 33]))
            .unwrap();
        assert!(matches!(
            PaperIdentity::load(&long, &repo(), &[]),
            Err(IdentityError::Malformed(_))
        ));
        // The name swapped between a regular copy of the key and a link to
        // the other identity while it is loaded: only the key itself loads.
        let mine = fs::read(&path).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let swapper = {
            let (stop, path, other, dir) =
                (stop.clone(), path.clone(), other.clone(), dir.to_path_buf());
            std::thread::spawn(move || {
                let mut n = 0u64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let tmp = dir.join(format!("swap-{n}"));
                    if n.is_multiple_of(2) {
                        std::os::unix::fs::symlink(&other, &tmp).unwrap();
                    } else {
                        fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .mode(0o600)
                            .open(&tmp)
                            .and_then(|mut f| f.write_all(&mine))
                            .unwrap();
                    }
                    fs::rename(&tmp, &path).unwrap();
                    n += 1;
                }
            })
        };
        for _ in 0..2000 {
            if let Ok(id) = PaperIdentity::load(&path, &repo(), &[]) {
                assert_eq!(id.address(), address);
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        swapper.join().unwrap();
        fs::remove_dir_all(base).unwrap();
    }

    /// Gate review L1: a path that leaves the repository and returns to it
    /// through `..` of directories that do not exist yet is refused before
    /// anything is created (before: the key was written inside it).
    #[test]
    fn a_path_back_into_the_repository_is_refused_before_creation() {
        let base = std::env::temp_dir().join(format!("bsc-paper-l1-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        // A sibling that leads back: its text does not start with the
        // repository, its resolution does.
        let sneaky = base.join("other/../repo/newdir/paper.key");
        assert!(matches!(
            PaperIdentity::create(&sneaky, &repo),
            Err(IdentityError::Location(_))
        ));
        assert!(!repo.join("newdir").exists());
        assert!(!base.join("other").exists());
        // Outside it is fine.
        let outside = base.join("keys/../keys2/paper.key");
        PaperIdentity::create(&outside, &repo).unwrap();
        assert!(base.join("keys2/paper.key").exists());
        fs::remove_dir_all(&base).unwrap();
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
