//! Fast-Lane directory trust contract (E7-5), identical to the Go overlay's
//! `trust.go`.
//!
//! geth and the Rust runtime run as the same Unix user. The Fast-Lane
//! directory is owned by that user and is not writable by group/others (0700
//! recommended, 0755 accepted: listing names grants no access to 0600 files).
//! Every ancestor of its resolved path is owned by root or that user and is not
//! writable by group/others unless it is sticky. Role leases, arenas and V1
//! rings are opened relative to a descriptor of that verified directory with
//! `O_NOFOLLOW | O_CLOEXEC`, and the opened descriptor must be a regular file
//! of that user without group/other permission bits and with exactly one link.
//! A symlink, foreign owner, open mode or extra hard link fails closed before
//! any lock, truncate or mmap. Separate geth/Rust users are not supported and
//! fail closed; ACLs are not inspected; a compromised process of the same user
//! is outside this boundary.

use std::ffi::CString;
use std::fs::{self, File};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};

const DIR_FLAGS: libc::c_int = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW;

/// The marker of every trust-contract refusal, as `io::ErrorKind::PermissionDenied`.
pub const UNTRUSTED_PATH: &str = "fastlane: untrusted path";

fn untrusted(path: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{UNTRUSTED_PATH}: {}: {reason}", path.display()),
    )
}

/// True for an error of this contract (not a plain I/O failure).
pub fn is_untrusted(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::PermissionDenied && error.to_string().starts_with(UNTRUSTED_PATH)
}

fn fstat(fd: &OwnedFd) -> io::Result<libc::stat> {
    let mut st = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `fd` is an open descriptor and `st` is a writable stat buffer.
    if unsafe { libc::fstat(fd.as_raw_fd(), st.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstat succeeded and initialized the buffer.
    Ok(unsafe { st.assume_init() })
}

fn openat(base: libc::c_int, name: &[u8], flags: libc::c_int, mode: u32) -> io::Result<OwnedFd> {
    let name = CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "fastlane: NUL in path"))?;
    // SAFETY: `name` is NUL-terminated; `base` is AT_FDCWD or an open
    // descriptor; the mode is passed as the promoted variadic argument.
    let fd = unsafe {
        libc::openat(
            base,
            name.as_ptr(),
            flags | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a new descriptor that nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn euid() -> u32 {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() }
}

fn check_dir(fd: &OwnedFd, euid: u32, last: bool) -> Result<(), &'static str> {
    let st = fstat(fd).map_err(|_| "fstat failed")?;
    let mode = st.st_mode as u32;
    if mode & libc::S_IFMT as u32 != libc::S_IFDIR as u32 {
        return Err("not a directory");
    }
    if last {
        if st.st_uid != euid {
            return Err("directory owned by another user");
        }
        if mode & 0o022 != 0 {
            return Err("directory writable by group/other");
        }
        return Ok(());
    }
    if st.st_uid != 0 && st.st_uid != euid {
        return Err("ancestor owned by another user");
    }
    if mode & 0o022 != 0 && mode & libc::S_ISVTX as u32 == 0 {
        return Err("ancestor writable by group/other without sticky bit");
    }
    Ok(())
}

/// Walks the resolved path of `dir` from `/` with `O_NOFOLLOW`, checks every
/// component and returns a descriptor of `dir`.
pub(crate) fn open_trusted_dir(dir: &Path) -> io::Result<OwnedFd> {
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let real = fs::canonicalize(dir)?;
    let names: Vec<&[u8]> = real
        .components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name.as_bytes()),
            _ => None,
        })
        .collect();
    let euid = euid();
    let mut fd = openat(libc::AT_FDCWD, b"/", DIR_FLAGS, 0)?;
    check_dir(&fd, euid, names.is_empty()).map_err(|r| untrusted(&real, r))?;
    for (index, name) in names.iter().enumerate() {
        let next = openat(fd.as_raw_fd(), name, DIR_FLAGS, 0)
            .map_err(|e| untrusted(&real, &e.to_string()))?;
        fd = next;
        check_dir(&fd, euid, index + 1 == names.len()).map_err(|r| untrusted(&real, r))?;
    }
    Ok(fd)
}

/// Opens the last component of `path` relative to the verified directory.
/// Errors of the open itself keep their OS error, so `NotFound` and
/// `AlreadyExists` stay recognizable.
pub(crate) fn open_trusted_file(path: &Path, flags: libc::c_int, mode: u32) -> io::Result<File> {
    let name = path
        .file_name()
        .ok_or_else(|| untrusted(path, "no file name"))?;
    let parent = path.parent().unwrap_or(Path::new("."));
    let dir = open_trusted_dir(parent)?;
    let fd = openat(
        dir.as_raw_fd(),
        name.as_bytes(),
        flags | libc::O_NOFOLLOW,
        mode,
    )?;
    let st = fstat(&fd)?;
    let file_mode = st.st_mode as u32;
    let reason = if file_mode & libc::S_IFMT as u32 != libc::S_IFREG as u32 {
        Some("not a regular file")
    } else if st.st_uid != euid() {
        Some("file owned by another user")
    } else if file_mode & 0o077 != 0 {
        Some("file grants group/other access")
    } else if st.st_nlink as u64 != 1 {
        Some("file has more than one link")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(untrusted(path, reason)),
        None => Ok(File::from(fd)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};

    static ID: AtomicU64 = AtomicU64::new(0);

    pub(crate) fn private_dir(name: &str) -> std::path::PathBuf {
        let id = ID.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("darkbot-trust-{}-{id}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[test]
    fn directory_and_file_rules_fail_closed() {
        let dir = private_dir("rules");
        assert!(open_trusted_dir(&dir).is_ok());
        for mode in [0o770, 0o702, 0o775, 0o777] {
            fs::set_permissions(&dir, Permissions::from_mode(mode)).unwrap();
            let error = open_trusted_dir(&dir).unwrap_err();
            assert!(is_untrusted(&error), "mode {mode:o}: {error}");
        }
        fs::set_permissions(&dir, Permissions::from_mode(0o755)).unwrap();
        assert!(open_trusted_dir(&dir).is_ok());

        let parent = private_dir("ancestor");
        let child = parent.join("fastlane");
        fs::create_dir(&child).unwrap();
        fs::set_permissions(&parent, Permissions::from_mode(0o777)).unwrap();
        assert!(is_untrusted(&open_trusted_dir(&child).unwrap_err()));
        fs::set_permissions(&parent, Permissions::from_mode(0o1777)).unwrap();
        assert!(open_trusted_dir(&child).is_ok());

        let file = dir.join("f");
        fs::write(&file, b"keep").unwrap();
        fs::set_permissions(&file, Permissions::from_mode(0o640)).unwrap();
        assert!(is_untrusted(
            &open_trusted_file(&file, libc::O_RDONLY, 0).unwrap_err()
        ));
        fs::set_permissions(&file, Permissions::from_mode(0o600)).unwrap();
        assert!(open_trusted_file(&file, libc::O_RDONLY, 0).is_ok());
        fs::hard_link(&file, dir.join("g")).unwrap();
        assert!(is_untrusted(
            &open_trusted_file(&file, libc::O_RDONLY, 0).unwrap_err()
        ));
        let missing = open_trusted_file(&dir.join("missing"), libc::O_RDONLY, 0).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
        std::os::unix::fs::symlink(dir.join("f"), dir.join("link")).unwrap();
        assert!(open_trusted_file(&dir.join("link"), libc::O_RDONLY, 0).is_err());
    }
}
