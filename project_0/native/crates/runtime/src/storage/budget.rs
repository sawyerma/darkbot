//! SK-03: hard, configurable byte budget for capture-owned buffers.
//!
//! Every queued frame reserves its bytes before admission and releases them when written. The
//! admission limit starts at `initial` and grows only while a backlog needs it, never above
//! `max`, and only while the process (peak RSS, a conservative bound of the current RSS) plus the
//! growth stays below physical memory minus the configured system reserve. When the backlog is
//! gone the limit returns to `initial`. A request that cannot be admitted within its wait fails:
//! the caller records INCOMPLETE; nothing is dropped silently and `max` is never exceeded.
//! A byte counter is not a whole-process RSS limit; RSS is observed and reported separately.
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const MIB: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct BudgetSnapshot {
    pub initial_bytes: u64,
    pub max_bytes: u64,
    pub system_reserve_bytes: u64,
    pub physical_bytes: Option<u64>,
    pub limit_bytes: u64,
    pub used_bytes: u64,
    pub peak_bytes: u64,
    pub growths: u64,
    pub waits: u64,
    pub wait_ns_max: u64,
    pub refused: u64,
}

struct State {
    used: u64,
    limit: u64,
    peak: u64,
    growths: u64,
    waits: u64,
    wait_ns_max: u64,
    refused: u64,
}

struct Shared {
    state: Mutex<State>,
    released: Condvar,
    initial: u64,
    max: u64,
    reserve: u64,
    physical: Option<u64>,
}

#[derive(Clone)]
pub struct MemoryBudget(Arc<Shared>);

/// Reserved bytes; released on drop.
pub struct BytePermit {
    budget: Arc<Shared>,
    bytes: u64,
}

impl Drop for BytePermit {
    fn drop(&mut self) {
        let mut s = self.budget.state.lock().unwrap_or_else(|p| p.into_inner());
        s.used = s.used.saturating_sub(self.bytes);
        if s.used == 0 {
            // Backlog gone: give unused capacity back.
            s.limit = self.budget.initial;
        }
        drop(s);
        self.budget.released.notify_all();
    }
}

impl MemoryBudget {
    pub fn new(initial: u64, max: u64, reserve: u64) -> Result<Self, &'static str> {
        Self::with_physical(initial, max, reserve, physical_memory_bytes())
    }

    fn with_physical(
        initial: u64,
        max: u64,
        reserve: u64,
        physical: Option<u64>,
    ) -> Result<Self, &'static str> {
        if initial == 0 || initial > max {
            return Err("STORAGE_BUDGET_INVALID");
        }
        if physical.is_some_and(|p| max.saturating_add(reserve) > p) {
            return Err("STORAGE_BUDGET_EXCEEDS_PHYSICAL_MINUS_RESERVE");
        }
        Ok(Self(Arc::new(Shared {
            state: Mutex::new(State {
                used: 0,
                limit: initial,
                peak: 0,
                growths: 0,
                waits: 0,
                wait_ns_max: 0,
                refused: 0,
            }),
            released: Condvar::new(),
            initial,
            max,
            reserve,
            physical,
        })))
    }

    /// Reserve `bytes` before admission, waiting at most `wait` for releases.
    pub fn acquire(&self, bytes: u64, wait: Duration) -> Result<BytePermit, &'static str> {
        let b = &self.0;
        let mut s = b.state.lock().map_err(|_| "STORAGE_BUDGET_POISONED")?;
        if bytes > b.max {
            s.refused += 1;
            return Err("BYTE_PERMIT_EXCEEDS_MAX");
        }
        let started = Instant::now();
        let deadline = started + wait;
        let mut waited = false;
        loop {
            if s.used + bytes > s.limit && s.used + bytes <= b.max {
                let wanted = s.limit.saturating_mul(2).max(s.used + bytes).min(b.max);
                if self.headroom_allows(wanted - s.limit) {
                    s.limit = wanted;
                    s.growths += 1;
                }
            }
            if s.used + bytes <= s.limit {
                s.used += bytes;
                s.peak = s.peak.max(s.used);
                if waited {
                    let ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                    s.wait_ns_max = s.wait_ns_max.max(ns);
                }
                return Ok(BytePermit {
                    budget: b.clone(),
                    bytes,
                });
            }
            let now = Instant::now();
            if now >= deadline {
                s.refused += 1;
                return Err("CAPTURE_MEMORY_BUDGET_EXHAUSTED");
            }
            if !waited {
                waited = true;
                s.waits += 1;
            }
            s = b
                .released
                .wait_timeout(s, deadline - now)
                .map_err(|_| "STORAGE_BUDGET_POISONED")?
                .0;
        }
    }

    /// Growth only while peak RSS + growth stays below physical memory minus the reserve.
    /// Unknown physical memory or RSS permits no growth beyond `initial`.
    fn headroom_allows(&self, growth: u64) -> bool {
        match (self.0.physical, peak_rss_bytes()) {
            (Some(physical), Some(rss)) => {
                rss.saturating_add(growth) <= physical.saturating_sub(self.0.reserve)
            }
            _ => false,
        }
    }

    pub fn snapshot(&self) -> BudgetSnapshot {
        let b = &self.0;
        let s = b.state.lock().unwrap_or_else(|p| p.into_inner());
        BudgetSnapshot {
            initial_bytes: b.initial,
            max_bytes: b.max,
            system_reserve_bytes: b.reserve,
            physical_bytes: b.physical,
            limit_bytes: s.limit,
            used_bytes: s.used,
            peak_bytes: s.peak,
            growths: s.growths,
            waits: s.waits,
            wait_ns_max: s.wait_ns_max,
            refused: s.refused,
        }
    }
}

/// Installed physical memory, `None` when the OS does not report it.
pub fn physical_memory_bytes() -> Option<u64> {
    // SAFETY: sysconf has no preconditions; negative results are errors.
    let (pages, size) = unsafe {
        (
            libc::sysconf(libc::_SC_PHYS_PAGES),
            libc::sysconf(libc::_SC_PAGESIZE),
        )
    };
    (pages > 0 && size > 0).then(|| pages as u64 * size as u64)
}

/// Peak resident set size of this process (an upper bound of the current RSS).
pub fn peak_rss_bytes() -> Option<u64> {
    // SAFETY: getrusage writes into the zeroed struct we own.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 || usage.ru_maxrss <= 0 {
        return None;
    }
    let raw = usage.ru_maxrss as u64;
    // macOS reports bytes, Linux KiB.
    Some(if cfg!(target_os = "macos") {
        raw
    } else {
        raw * 1024
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Admission never exceeds `max`; growth happens only within the headroom; a request that
    /// cannot be admitted in time is refused (never dropped silently); release restores capacity.
    #[test]
    fn byte_budget_is_hard_adaptive_and_releases() {
        assert!(MemoryBudget::with_physical(0, 10, 0, None).is_err());
        assert!(MemoryBudget::with_physical(20, 10, 0, None).is_err());
        assert_eq!(
            MemoryBudget::with_physical(1, 10, 5, Some(12)).err(),
            Some("STORAGE_BUDGET_EXCEEDS_PHYSICAL_MINUS_RESERVE")
        );
        // Unknown physical memory: no growth beyond initial.
        let fixed = MemoryBudget::with_physical(100, 1_000, 0, None).unwrap();
        let a = fixed.acquire(80, Duration::ZERO).unwrap();
        assert_eq!(
            fixed.acquire(40, Duration::from_millis(5)).err(),
            Some("CAPTURE_MEMORY_BUDGET_EXHAUSTED")
        );
        drop(a);
        assert!(fixed.acquire(100, Duration::ZERO).is_ok());
        assert_eq!(
            fixed.acquire(1_001, Duration::ZERO).err(),
            Some("BYTE_PERMIT_EXCEEDS_MAX")
        );
        // Ample physical memory: the limit grows up to max, never beyond, and returns to initial.
        let ample = 1u64 << 50;
        let growing = MemoryBudget::with_physical(100, 300, 0, Some(ample)).unwrap();
        let p1 = growing.acquire(100, Duration::ZERO).unwrap();
        let p2 = growing.acquire(150, Duration::ZERO).unwrap();
        let s = growing.snapshot();
        assert!(s.limit_bytes <= 300 && s.used_bytes == 250 && s.growths >= 1);
        assert!(growing.acquire(51, Duration::from_millis(5)).is_err());
        // A waiting request is admitted as soon as a release frees enough bytes.
        let waiter = {
            let g = growing.clone();
            std::thread::spawn(move || g.acquire(200, Duration::from_secs(5)).map(|_| ()))
        };
        std::thread::sleep(Duration::from_millis(20));
        drop(p2);
        assert_eq!(waiter.join().unwrap(), Ok(()));
        drop(p1);
        let s = growing.snapshot();
        assert_eq!((s.used_bytes, s.limit_bytes), (0, 100));
        assert!(s.peak_bytes <= 300 && s.refused == 1 && s.waits >= 1);
        assert!(physical_memory_bytes().is_some() && peak_rss_bytes().is_some());
    }
}
