//! S4-04: heap allocations per thread (PHASENPLAN S4 Performance-Baseline: "allocations" per core
//! stage). They are counted only in a binary whose global allocator is [`Counting`] (a baseline
//! build); every other build counts nothing and says so — `thread_allocations()` is `None`, never 0.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

/// Set by the first allocation through `Counting`: from then on this process counts.
static COUNTING: AtomicBool = AtomicBool::new(false);

thread_local! {
    // Constant-initialized without a destructor: reading it never allocates.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

/// The system allocator that counts every allocation (`alloc`, `alloc_zeroed`, `realloc`) of the
/// thread making it. Installed only by a baseline build (`#[global_allocator]`).
pub struct Counting;

#[inline]
fn count() {
    if !COUNTING.load(Relaxed) {
        COUNTING.store(true, Relaxed);
    }
    // A thread past its thread-local storage (its exit) is not counted any more.
    let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// The allocations the calling thread has made so far; `None` in a build whose global allocator
/// is not [`Counting`].
#[inline]
pub fn thread_allocations() -> Option<u64> {
    if !COUNTING.load(Relaxed) {
        return None;
    }
    ALLOCATIONS.try_with(Cell::get).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without `Counting` as the global allocator (this test binary) nothing is counted: unknown,
    /// never 0 — even after an allocation. Once `Counting` allocates, it counts each allocation of
    /// the calling thread once, and only that thread's. (One test: the switch is process-wide.)
    #[test]
    fn only_the_counting_allocator_counts_and_per_thread() {
        let v = std::hint::black_box(vec![1u8; 64]);
        assert_eq!(v.len(), 64);
        assert_eq!(thread_allocations(), None);
        let layout = Layout::from_size_align(32, 8).unwrap();
        let before = ALLOCATIONS.with(Cell::get);
        unsafe {
            let p = Counting.alloc(layout);
            let p = Counting.realloc(p, layout, 64);
            Counting.dealloc(p, Layout::from_size_align(64, 8).unwrap());
            let z = Counting.alloc_zeroed(layout);
            assert!(std::slice::from_raw_parts(z, 32).iter().all(|b| *b == 0));
            Counting.dealloc(z, layout);
        }
        assert_eq!(thread_allocations(), Some(before + 3));
        let other = std::thread::spawn(thread_allocations).join().unwrap();
        assert_eq!(other, Some(0), "another thread's count is its own");
    }
}
