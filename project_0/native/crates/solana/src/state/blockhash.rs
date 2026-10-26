//! S2-05: Recent-Blockhash-Cache. Kein RPC im Decision-Hotpath: der Cache wird
//! außerhalb (Poll-Thread) erneuert; der Builder liest nur.
//!
//! Status: UNTESTED / VERIFY (get_latest_blockhash_with_commitment Signatur).

use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig; // [FIX-13]
use solana_sdk::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedBlockhash {
    pub hash: Hash,
    pub last_valid_block_height: u64,
    /// Monotone Zeit der Aktualisierung (Clock::now_ns) für Staleness-Prüfung.
    pub refreshed_ns: u64,
}

/// DEC-01: the cache with its generation (+1 per stored blockhash) and the owner's coalesced
/// refresh request to the one budgeted refresh worker (no RPC, sleep, lock wait or I/O on the
/// requesting side: an atomic flag and an unpark).
#[derive(Clone, Default)]
pub struct BlockhashCache {
    inner: Arc<RwLock<Option<CachedBlockhash>>>,
    generation: Arc<AtomicU64>,
    wake: Arc<AtomicBool>,
    worker: Arc<OnceLock<std::thread::Thread>>,
}

impl BlockhashCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self) -> Option<CachedBlockhash> {
        *self.inner.read().expect("blockhash lock")
    }

    pub fn set(&self, v: CachedBlockhash) {
        *self.inner.write().expect("blockhash lock") = Some(v);
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// How many blockhashes were stored so far (DEC-01: a parked candidate waits for a newer one).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// The refresh worker registers its thread once, so a request wakes it at once.
    pub fn register_worker(&self, thread: std::thread::Thread) {
        let _ = self.worker.set(thread);
    }

    /// DEC-01: asks the worker for an immediate refresh; `true` for a new request, `false` when
    /// one is already pending (coalesced). Never blocks.
    pub fn request_refresh(&self) -> bool {
        let new = !self.wake.swap(true, Ordering::AcqRel);
        if new {
            if let Some(worker) = self.worker.get() {
                worker.unpark();
            }
        }
        new
    }

    /// The worker takes a pending request (`true` once per request).
    pub fn take_refresh_request(&self) -> bool {
        self.wake.swap(false, Ordering::AcqRel)
    }

    /// Einmalige Erneuerung per RPC (Poll-Thread ruft das periodisch, z. B. alle 10 s).
    pub fn refresh(&self, rpc: &RpcClient, now_ns: u64) -> Result<CachedBlockhash, String> {
        let (hash, last_valid_block_height) = rpc
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| e.to_string())?;
        let v = CachedBlockhash {
            hash,
            last_valid_block_height,
            refreshed_ns: now_ns,
        };
        self.set(v);
        Ok(v)
    }

    /// Stale, wenn älter als `max_age_ns` oder leer. Ein staler Blockhash ist ein
    /// Build-Fehler, kein Grund zum synchronen RPC im Hotpath.
    pub fn fresh(&self, now_ns: u64, max_age_ns: u64) -> Option<CachedBlockhash> {
        let v = self.get()?;
        if now_ns.saturating_sub(v.refreshed_ns) > max_age_ns {
            None
        } else {
            Some(v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staleness() {
        let c = BlockhashCache::new();
        assert!(c.fresh(0, 1).is_none());
        c.set(CachedBlockhash {
            hash: Hash::default(),
            last_valid_block_height: 5,
            refreshed_ns: 100,
        });
        assert!(c.fresh(150, 100).is_some());
        assert!(c.fresh(300, 100).is_none());
    }

    /// DEC-01: every stored blockhash is a new generation; refresh requests coalesce until the
    /// worker takes them, and a request unparks the registered worker.
    #[test]
    fn generation_and_coalesced_refresh_requests() {
        let c = BlockhashCache::new();
        assert_eq!(c.generation(), 0);
        let v = CachedBlockhash {
            hash: Hash::default(),
            last_valid_block_height: 1,
            refreshed_ns: 1,
        };
        c.set(v);
        c.set(v);
        assert_eq!(c.generation(), 2);
        assert!(!c.take_refresh_request());
        c.register_worker(std::thread::current());
        assert!(c.request_refresh());
        assert!(!c.request_refresh(), "a pending request is coalesced");
        // The unpark of the request is the park token: this park returns at once.
        std::thread::park_timeout(std::time::Duration::from_secs(5));
        assert!(c.take_refresh_request());
        assert!(!c.take_refresh_request());
        assert!(c.request_refresh());
    }
}
