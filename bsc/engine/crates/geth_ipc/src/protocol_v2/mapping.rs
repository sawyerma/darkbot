// Part of the single Rust module `protocol_v2` (included by `mod.rs`):
// The shared-memory mapping (mmap) of a V2 arena file.

struct MappingV2 {
    _file: File,
    ptr: NonNull<u8>,
    len: usize,
}

unsafe impl Send for MappingV2 {}
unsafe impl Sync for MappingV2 {}

impl MappingV2 {
    fn map(file: File, len: usize) -> Result<Self, V2Error> {
        let raw = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if raw == libc::MAP_FAILED {
            return Err(V2Error::MapFailed(io::Error::last_os_error()));
        }
        let ptr = NonNull::new(raw.cast::<u8>())
            .ok_or_else(|| V2Error::MapFailed(io::Error::other("mmap returned null")))?;
        Ok(Self {
            _file: file,
            ptr,
            len,
        })
    }

    fn bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    fn atomic_u64(&self, offset: usize) -> &AtomicU64 {
        debug_assert_eq!(offset % 8, 0);
        unsafe { &*self.ptr.as_ptr().add(offset).cast::<AtomicU64>() }
    }

    fn atomic_u32(&self, offset: usize) -> &AtomicU32 {
        debug_assert_eq!(offset % 4, 0);
        unsafe { &*self.ptr.as_ptr().add(offset).cast::<AtomicU32>() }
    }

    fn sync(&self) -> Result<(), V2Error> {
        let rc = unsafe { libc::msync(self.ptr.as_ptr().cast(), self.len, libc::MS_SYNC) };
        if rc != 0 {
            return Err(V2Error::Io(io::Error::last_os_error()));
        }
        Ok(())
    }
}

impl Drop for MappingV2 {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}
