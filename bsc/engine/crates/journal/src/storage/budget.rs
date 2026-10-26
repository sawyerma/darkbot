//! The one accounting owner of capture memory (master plan §11, storage
//! contract): [`MemoryBudget`] admits bytes up to a hard maximum and every
//! admitted byte is held by a [`BytePermit`] until the memory it stands for
//! is released. A refused reservation is the caller's to count; nothing
//! waits.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::format::JournalError;

/// A hard byte bound shared by the producers and the writer of one capture.
#[derive(Debug)]
pub struct MemoryBudget {
    max: usize,
    used: AtomicUsize,
    peak: AtomicUsize,
    refused: AtomicU64,
}

/// Bytes reserved in a [`MemoryBudget`]; released when dropped.
#[derive(Debug)]
pub struct BytePermit {
    budget: Arc<MemoryBudget>,
    bytes: usize,
}

impl MemoryBudget {
    /// A budget of `max_bytes` (at least one).
    pub fn new(max_bytes: usize) -> Result<Arc<Self>, JournalError> {
        if max_bytes == 0 {
            return Err(JournalError::InvalidBound);
        }
        Ok(Arc::new(Self {
            max: max_bytes,
            used: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            refused: AtomicU64::new(0),
        }))
    }

    /// Reserves `bytes`, or refuses (counted) when the total would exceed
    /// the maximum. Never waits.
    pub fn try_reserve(self: &Arc<Self>, bytes: usize) -> Option<BytePermit> {
        let mut used = self.used.load(Ordering::Acquire);
        loop {
            let next = match used.checked_add(bytes) {
                Some(next) if next <= self.max => next,
                _ => {
                    self.refused.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            };
            match self
                .used
                .compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    self.peak.fetch_max(next, Ordering::AcqRel);
                    return Some(BytePermit {
                        budget: Arc::clone(self),
                        bytes,
                    });
                }
                Err(observed) => used = observed,
            }
        }
    }

    pub fn max(&self) -> usize {
        self.max
    }

    /// Bytes held by live permits.
    pub fn in_use(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }

    /// The highest `in_use` so far.
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::Acquire)
    }

    /// Reservations refused.
    pub fn refused(&self) -> u64 {
        self.refused.load(Ordering::Acquire)
    }
}

impl BytePermit {
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for BytePermit {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_admits_up_to_its_maximum_and_releases_on_drop() {
        assert!(MemoryBudget::new(0).is_err());
        let budget = MemoryBudget::new(100).unwrap();
        let a = budget.try_reserve(60).unwrap();
        assert!(budget.try_reserve(41).is_none());
        let b = budget.try_reserve(40).unwrap();
        assert_eq!(
            (budget.in_use(), budget.peak(), budget.refused()),
            (100, 100, 1)
        );
        assert!(budget.try_reserve(usize::MAX).is_none());
        drop(a);
        assert_eq!(budget.in_use(), 40);
        drop(b);
        assert_eq!(
            (budget.in_use(), budget.peak(), budget.refused()),
            (0, 100, 2)
        );
    }
}
