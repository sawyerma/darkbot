//! S2-05: Recent-Blockhash-Cache. Kein RPC im Decision-Hotpath: der Cache wird
//! außerhalb (Poll-Thread) erneuert; der Builder liest nur.
//!
//! Status: UNTESTED / VERIFY (get_latest_blockhash_with_commitment Signatur).

use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig; // [FIX-13]
use solana_sdk::hash::Hash;
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedBlockhash {
    pub hash: Hash,
    pub last_valid_block_height: u64,
    /// Monotone Zeit der Aktualisierung (Clock::now_ns) für Staleness-Prüfung.
    pub refreshed_ns: u64,
}

#[derive(Clone, Default)]
pub struct BlockhashCache {
    inner: Arc<RwLock<Option<CachedBlockhash>>>,
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
}
