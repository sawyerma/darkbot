//! P2-06/P2-07: Prepared-Candidate-Cache mit expliziter Invalidation.
//!
//! Vorab (sicher): Account-Listen, Instruction-Skelette ohne Beträge, LUT-Plan, Ressourcenprofil.
//! Spät zu patchen (nie cachen): Blockhash, seize/repay-Beträge, Tip/Fee, Preis-Updates,
//! v1-Config, Signatur.
//! Invalidation-Schlüssel: Account-Version (slot), Oracle-Slot je Bank, Tag-Zustand,
//! PREMIUM_ACTIVE-Flags, LUT-Version, FeeState-Version. Ändert sich einer, ist der
//! Eintrag ungültig und wird beim nächsten Zugriff neu gebaut — kein stiller Reuse.

use solana_sdk::instruction::Instruction;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct InvalidationKey {
    pub account_slot: u64,
    pub oracle_slots: Vec<(Pubkey, u64)>, // sortiert nach Pubkey
    pub tagged_at: Option<u64>,
    pub premium_flags: u64, // Bitmaske der beteiligten Banks (PREMIUM_ACTIVE)
    pub lut_version: u64,
    pub fee_state_version: u64,
    /// ProgramData-Hash-Version (P1A-Manifest), Integrations-/Route-Stand.
    pub program_version: u64,
    pub integration_version: u64,
    pub route_version: u64,
    pub funding: crate::funding::FundingClass,
}

impl InvalidationKey {
    pub fn normalized(mut self) -> Self {
        self.oracle_slots
            .sort_by(|a, b| a.0.to_bytes().cmp(&b.0.to_bytes()));
        self
    }
}

/// Skelett ohne spätes Patchen. `amount_slots` nennt Instruction-Index + Byte-Offset
/// der Betragsfelder, die bei T_detect gepatcht werden.
#[derive(Clone, Debug)]
pub struct PreparedCandidate {
    pub account: Pubkey,
    pub key: InvalidationKey,
    pub instructions: Vec<Instruction>,
    pub amount_slots: Vec<(usize, usize)>,
    pub lookup_tables: Vec<Pubkey>,
    pub locked_accounts: usize,
    pub estimated_bytes: usize,
    pub compute_unit_limit: u32,
    pub loaded_accounts_data_size: u32,
}

#[derive(Default)]
pub struct PreparedCache {
    entries: HashMap<Pubkey, PreparedCandidate>,
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
}

impl PreparedCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Liefert den Eintrag nur, wenn der Schlüssel exakt übereinstimmt.
    pub fn get(
        &mut self,
        account: &Pubkey,
        current: &InvalidationKey,
    ) -> Option<&PreparedCandidate> {
        // [FIX-51] Validität zuerst ohne Referenz-Rückgabe bestimmen. Dadurch endet der
        // immutable Borrow vor einem möglichen remove(), und wir geraten nicht in E0502.
        let is_valid = match self.entries.get(account) {
            Some(e) => &e.key == current,
            None => {
                self.misses += 1;
                return None;
            }
        };

        if is_valid {
            self.hits += 1;
            self.entries.get(account)
        } else {
            self.invalidations += 1;
            self.entries.remove(account);
            self.misses += 1;
            None
        }
    }

    pub fn put(&mut self, c: PreparedCandidate) {
        self.entries.insert(c.account, c);
    }

    pub fn remove(&mut self, account: &Pubkey) {
        if self.entries.remove(account).is_some() {
            self.invalidations += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(slot: u64, tag: Option<u64>) -> InvalidationKey {
        InvalidationKey {
            account_slot: slot,
            oracle_slots: vec![],
            tagged_at: tag,
            premium_flags: 0,
            lut_version: 1,
            fee_state_version: 1,
            program_version: 1,
            integration_version: 1,
            route_version: 1,
            funding: crate::funding::FundingClass::NoSwapInventory,
        }
        .normalized()
    }

    #[test]
    fn exact_key_or_rebuild() {
        let a = Pubkey::new_unique();
        let mut c = PreparedCache::new();
        assert!(c.get(&a, &key(1, None)).is_none());
        c.put(PreparedCandidate {
            account: a,
            key: key(1, None),
            instructions: vec![],
            amount_slots: vec![],
            lookup_tables: vec![],
            locked_accounts: 3,
            estimated_bytes: 300,
            compute_unit_limit: 1,
            loaded_accounts_data_size: 0,
        });
        assert!(c.get(&a, &key(1, None)).is_some());
        assert!(
            c.get(&a, &key(2, None)).is_none(),
            "Account-Update invalidiert"
        );
        assert_eq!(c.invalidations, 1);
        c.put(PreparedCandidate {
            account: a,
            key: key(2, Some(5)),
            instructions: vec![],
            amount_slots: vec![],
            lookup_tables: vec![],
            locked_accounts: 3,
            estimated_bytes: 300,
            compute_unit_limit: 1,
            loaded_accounts_data_size: 0,
        });
        assert!(
            c.get(&a, &key(2, None)).is_none(),
            "Tag-Zustand invalidiert"
        );
    }
}
