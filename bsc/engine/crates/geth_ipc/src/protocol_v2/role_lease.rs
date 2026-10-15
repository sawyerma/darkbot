// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// Runtime role leases (geth owner / Rust consumer) on the arena directory.

/// Exclusive runtime roles of one Fast-Lane directory. geth and Rust each
/// hold their own role at the same time; a second process of the same role
/// is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeRoleV2 {
    GethOwner,
    RustOwner,
}

impl RuntimeRoleV2 {
    pub fn name(self) -> &'static str {
        match self {
            Self::GethOwner => "geth-owner",
            Self::RustOwner => "rust-owner",
        }
    }
}

/// An exclusive role lease: `flock(LOCK_EX | LOCK_NB)` on
/// `<dir>/<role>.lock`, held through the open descriptor for the lease's
/// lifetime. The kernel lock is the only source of exclusivity; the file's
/// content (version, role, generation, pid) is diagnostic. Dropping the lease
/// or the death of the process closes the descriptor and releases the lock.
/// The lock file is never deleted, so no second process can lock a fresh
/// inode while the first still holds the old one.
pub struct RoleLeaseV2 {
    _file: File,
    path: PathBuf,
    role: RuntimeRoleV2,
}

impl RoleLeaseV2 {
    pub fn acquire(dir: &Path, role: RuntimeRoleV2, generation: u64) -> Result<Self, V2Error> {
        if generation == 0 {
            return Err(V2Error::ZeroGeneration);
        }
        let path = dir.join(format!("{}.lock", role.name()));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(V2Error::Io)?;
        // SAFETY: `file` owns a valid descriptor for the duration of the call.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
                return Err(V2Error::RoleLeaseHeld { role: role.name() });
            }
            return Err(V2Error::Io(error));
        }
        let content = format!(
            "version={ROLE_LEASE_VERSION_V2}\nrole={}\ngeneration={generation}\npid={}\n",
            role.name(),
            std::process::id()
        );
        file.set_len(0).map_err(V2Error::Io)?;
        file.write_all_at(content.as_bytes(), 0)
            .map_err(V2Error::Io)?;
        Ok(Self {
            _file: file,
            path,
            role,
        })
    }

    pub fn role(&self) -> RuntimeRoleV2 {
        self.role
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
