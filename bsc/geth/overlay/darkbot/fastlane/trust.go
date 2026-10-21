package fastlane

// Fast-Lane directory trust contract (E7-5).
//
// geth and the Rust runtime run as the same Unix user. The Fast-Lane
// directory is owned by that user and is not writable by group/others (0700
// recommended, 0755 accepted: listing names grants no access to 0600 files).
// Every ancestor of its resolved path is owned by root or that user and is not
// writable by group/others unless it is sticky. Role leases, arenas and V1
// rings are opened relative to a descriptor of that verified directory with
// O_NOFOLLOW|O_CLOEXEC, and the opened descriptor must be a regular file of
// that user without group/other permission bits and with exactly one link.
// A symlink, foreign owner, open mode or extra hard link fails closed before
// any lock, truncate or mmap. Separate geth/Rust users are not supported and
// fail closed; ACLs are not inspected; a compromised process of the same user
// is outside this boundary.

import (
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"strings"

	"golang.org/x/sys/unix"
)

var errV2UntrustedPath = errors.New("fastlane untrusted path")

const trustedDirFlagsV2 = unix.O_RDONLY | unix.O_DIRECTORY | unix.O_NOFOLLOW | unix.O_CLOEXEC

func untrustedV2(path, reason string) error {
	return fmt.Errorf("%w: %s: %s", errV2UntrustedPath, path, reason)
}

// openTrustedDirV2 walks the resolved path of dir from "/" with
// O_NOFOLLOW, checks every component and returns a descriptor of dir.
func openTrustedDirV2(dir string) (int, error) {
	abs, err := filepath.Abs(dir)
	if err != nil {
		return -1, err
	}
	real, err := filepath.EvalSymlinks(abs)
	if err != nil {
		return -1, err
	}
	var names []string
	if trimmed := strings.Trim(real, "/"); trimmed != "" {
		names = strings.Split(trimmed, "/")
	}
	euid := uint32(os.Geteuid())
	fd, err := unix.Open("/", trustedDirFlagsV2, 0)
	if err != nil {
		return -1, err
	}
	if reason := checkTrustedDirV2(fd, euid, len(names) == 0); reason != "" {
		_ = unix.Close(fd)
		return -1, untrustedV2(real, reason)
	}
	for i, name := range names {
		next, err := unix.Openat(fd, name, trustedDirFlagsV2, 0)
		_ = unix.Close(fd)
		if err != nil {
			return -1, untrustedV2(real, err.Error())
		}
		fd = next
		if reason := checkTrustedDirV2(fd, euid, i == len(names)-1); reason != "" {
			_ = unix.Close(fd)
			return -1, untrustedV2(real, reason)
		}
	}
	return fd, nil
}

func checkTrustedDirV2(fd int, euid uint32, last bool) string {
	var st unix.Stat_t
	if err := unix.Fstat(fd, &st); err != nil {
		return err.Error()
	}
	mode := uint32(st.Mode)
	if mode&unix.S_IFMT != unix.S_IFDIR {
		return "not a directory"
	}
	if last {
		if st.Uid != euid {
			return "directory owned by another user"
		}
		if mode&0o022 != 0 {
			return "directory writable by group/other"
		}
		return ""
	}
	if st.Uid != 0 && st.Uid != euid {
		return "ancestor owned by another user"
	}
	if mode&0o022 != 0 && mode&unix.S_ISVTX == 0 {
		return "ancestor writable by group/other without sticky bit"
	}
	return ""
}

// checkTrustedFastLaneDirV2 verifies dir against the trust contract.
func checkTrustedFastLaneDirV2(dir string) error {
	fd, err := openTrustedDirV2(dir)
	if err != nil {
		return err
	}
	return unix.Close(fd)
}

// openTrustedFileV2 opens the last component of path relative to the
// verified directory. Errors of the open itself keep their errno, so
// fs.ErrExist and fs.ErrNotExist stay recognizable.
func openTrustedFileV2(path string, flags int, perm uint32) (*os.File, error) {
	dir, name := filepath.Split(path)
	if name == "" || name == "." || name == ".." {
		return nil, untrustedV2(path, "no file name")
	}
	if dir == "" {
		dir = "."
	}
	dfd, err := openTrustedDirV2(dir)
	if err != nil {
		return nil, err
	}
	defer unix.Close(dfd)
	fd, err := unix.Openat(dfd, name, flags|unix.O_NOFOLLOW|unix.O_CLOEXEC, perm)
	if err != nil {
		return nil, &fs.PathError{Op: "openat", Path: path, Err: err}
	}
	var st unix.Stat_t
	reason := ""
	if err := unix.Fstat(fd, &st); err != nil {
		reason = err.Error()
	} else if uint32(st.Mode)&unix.S_IFMT != unix.S_IFREG {
		reason = "not a regular file"
	} else if st.Uid != uint32(os.Geteuid()) {
		reason = "file owned by another user"
	} else if uint32(st.Mode)&0o077 != 0 {
		reason = "file grants group/other access"
	} else if uint64(st.Nlink) != 1 {
		reason = "file has more than one link"
	}
	if reason != "" {
		_ = unix.Close(fd)
		return nil, untrustedV2(path, reason)
	}
	return os.NewFile(uintptr(fd), path), nil
}

// readTrustedDirV2 lists dir through a verified descriptor.
func readTrustedDirV2(dir string) ([]os.DirEntry, error) {
	fd, err := openTrustedDirV2(dir)
	if err != nil {
		return nil, err
	}
	file := os.NewFile(uintptr(fd), dir)
	defer file.Close()
	return file.ReadDir(-1)
}
