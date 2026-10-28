//! P2-02: Reverse-Indices (Oracle→Banks, Bank→Accounts, Account→Banks, Integration→Banks).
//! P2-03: Schwellen-Index pro Bank — sortierte Liquidationspreis-Schwelle.
//!
//! Oracle-Update → betroffene Banks → Schwellen-Lookup → nur Accounts nahe der
//! Schwelle exakt (über das Crate) nachrechnen. Kein Full-Scan.
//!
//! Die Schwelle (`liq_price`) je (Bank, Account) wird von der Health-Engine geliefert
//! (P3): der Oracle-Preis der Bank, bei dem Maintenance-Health = 0 wird, alle anderen
//! Preise konstant. Für Liability-Banks steigt Risiko bei steigendem Preis, für
//! Asset-Banks bei fallendem; `Side` hält das fest.
//!
//! Preise als positive rohe I80F48-Bits in u128. Keine Dezimalrundung und kein f64.

use solana_sdk::pubkey::Pubkey;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// Account hält die Bank als Asset: liquidierbar, wenn Preis ≤ Schwelle.
    Asset,
    /// Account schuldet die Bank: liquidierbar, wenn Preis ≥ Schwelle.
    Liability,
}

#[derive(Default, Debug)]
pub struct ReverseIndex {
    oracle_to_banks: HashMap<Pubkey, HashSet<Pubkey>>,
    bank_to_accounts: HashMap<Pubkey, HashSet<Pubkey>>,
    account_to_banks: HashMap<Pubkey, HashSet<Pubkey>>,
    bank_oracles: HashMap<Pubkey, Vec<Pubkey>>,
}

impl ReverseIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ersetzt die Oracle-Menge einer Bank; alte Zuordnungen werden entfernt (Invalidation).
    pub fn set_bank_oracles(&mut self, bank: Pubkey, oracles: &[Pubkey]) {
        if let Some(old) = self.bank_oracles.remove(&bank) {
            for o in old {
                if let Some(set) = self.oracle_to_banks.get_mut(&o) {
                    set.remove(&bank);
                    if set.is_empty() {
                        self.oracle_to_banks.remove(&o);
                    }
                }
            }
        }
        for o in oracles {
            self.oracle_to_banks.entry(*o).or_default().insert(bank);
        }
        self.bank_oracles.insert(bank, oracles.to_vec());
    }

    /// Setzt die Bank-Menge eines Accounts (bei jedem Account-Update aufrufen;
    /// alte Zuordnungen werden entfernt).
    pub fn set_account_banks(&mut self, account: Pubkey, banks: &[Pubkey]) {
        // V2-14 P5c-05: an unchanged bank set (the common account update) changes nothing.
        if self.account_to_banks.get(&account).is_some_and(|old| {
            banks.iter().all(|b| old.contains(b)) && old.iter().all(|b| banks.contains(b))
        }) {
            return;
        }
        if let Some(old) = self.account_to_banks.remove(&account) {
            for b in old {
                if let Some(set) = self.bank_to_accounts.get_mut(&b) {
                    set.remove(&account);
                    // E10: a bank without accounts leaves no empty set behind.
                    if set.is_empty() {
                        self.bank_to_accounts.remove(&b);
                    }
                }
            }
        }
        let mut set = HashSet::with_capacity(banks.len());
        for b in banks {
            set.insert(*b);
            self.bank_to_accounts.entry(*b).or_default().insert(account);
        }
        self.account_to_banks.insert(account, set);
    }

    /// Indexed accounts.
    pub fn account_count(&self) -> usize {
        self.account_to_banks.len()
    }

    pub fn remove_account(&mut self, account: &Pubkey) {
        self.set_account_banks(*account, &[]);
        self.account_to_banks.remove(account);
    }

    pub fn banks_for_oracle(&self, oracle: &Pubkey) -> impl Iterator<Item = &Pubkey> {
        self.oracle_to_banks
            .get(oracle)
            .into_iter()
            .flat_map(|s| s.iter())
    }

    pub fn accounts_for_bank(&self, bank: &Pubkey) -> impl Iterator<Item = &Pubkey> {
        self.bank_to_accounts
            .get(bank)
            .into_iter()
            .flat_map(|s| s.iter())
    }

    /// DEC-06: the number of accounts holding `bank` — O(1), no bank-wide list.
    pub fn bank_account_count(&self, bank: &Pubkey) -> usize {
        self.bank_to_accounts.get(bank).map_or(0, HashSet::len)
    }

    /// V2-13: whether `account` holds `bank` — O(1), no bank-wide list.
    pub fn bank_has_account(&self, bank: &Pubkey, account: &Pubkey) -> bool {
        self.bank_to_accounts
            .get(bank)
            .is_some_and(|accounts| accounts.contains(account))
    }

    pub fn banks_for_account(&self, account: &Pubkey) -> impl Iterator<Item = &Pubkey> {
        self.account_to_banks
            .get(account)
            .into_iter()
            .flat_map(|s| s.iter())
    }
}

/// Pro Bank: Schwellen sortiert nach Preis. Zwei Seiten getrennt.
#[derive(Default, Debug)]
pub struct ThresholdIndex {
    /// Asset-Seite: liquidierbar wenn price <= key. Sortiert aufsteigend → Kandidaten = range(price..)
    asset: HashMap<Pubkey, BTreeMap<u128, HashSet<Pubkey>>>,
    /// Liability-Seite: liquidierbar wenn price >= key → Kandidaten = range(..=price)
    liability: HashMap<Pubkey, BTreeMap<u128, HashSet<Pubkey>>>,
    /// Rückwärts, damit ein Account-Update seine alten Einträge löschen kann.
    entries: HashMap<Pubkey, Vec<(Pubkey, Side, u128)>>,
}

impl ThresholdIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ersetzt alle Schwellen eines Accounts. `thresholds`: (bank, side, liq_price).
    /// Accounts ohne Schwelle (z. B. keine Liabilities) übergeben eine leere Liste.
    pub fn set_account(&mut self, account: Pubkey, thresholds: &[(Pubkey, Side, u128)]) {
        self.remove_account(&account);
        for (bank, side, price) in thresholds {
            let map = match side {
                Side::Asset => self.asset.entry(*bank).or_default(),
                Side::Liability => self.liability.entry(*bank).or_default(),
            };
            map.entry(*price).or_default().insert(account);
        }
        if !thresholds.is_empty() {
            self.entries.insert(account, thresholds.to_vec());
        }
    }

    pub fn remove_account(&mut self, account: &Pubkey) {
        if let Some(old) = self.entries.remove(account) {
            for (bank, side, price) in old {
                let map = match side {
                    Side::Asset => self.asset.get_mut(&bank),
                    Side::Liability => self.liability.get_mut(&bank),
                };
                if let Some(map) = map {
                    if let Some(set) = map.get_mut(&price) {
                        set.remove(account);
                        if set.is_empty() {
                            map.remove(&price);
                        }
                    }
                }
            }
        }
    }

    /// Kandidaten für die zwei Maintenance-Risk-Preisachsen einer Bank:
    /// collateral uses confidence-biased Low, liabilities use confidence-biased High.
    /// Die beiden Achsen dürfen nicht zusammengelegt werden.
    pub fn candidates(
        &self,
        bank: &Pubkey,
        asset_price: u128,
        liability_price: u128,
        margin: u128,
    ) -> Vec<Pubkey> {
        let mut out: HashSet<Pubkey> = HashSet::new();
        if let Some(m) = self.asset.get(bank) {
            // Asset-Halter wird liquidierbar wenn Low-price <= threshold.
            let lo = asset_price.saturating_sub(margin);
            for (_, set) in m.range(lo..) {
                out.extend(set.iter().copied());
            }
        }
        if let Some(m) = self.liability.get(bank) {
            // Schuldner wird liquidierbar wenn High-price >= threshold.
            let hi = liability_price.saturating_add(margin);
            for (_, set) in m.range(..=hi) {
                out.extend(set.iter().copied());
            }
        }
        out.into_iter().collect()
    }

    pub fn bank_keys(&self) -> HashSet<Pubkey> {
        self.asset
            .keys()
            .chain(self.liability.keys())
            .copied()
            .collect()
    }

    pub fn tracked_accounts(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E10: moving and removing accounts leaves no empty bank entry; lookups are unchanged.
    #[test]
    fn emptied_bank_entries_are_removed() {
        let mut idx = ReverseIndex::new();
        let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
        let (x, y, z) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        idx.set_account_banks(a, &[x, y]);
        idx.set_account_banks(b, &[y]);
        idx.set_account_banks(a, &[z]); // move: x empties, y keeps b
        assert_eq!(idx.bank_to_accounts.len(), 2);
        assert_eq!(idx.accounts_for_bank(&x).count(), 0);
        assert_eq!(
            idx.accounts_for_bank(&y).copied().collect::<Vec<_>>(),
            vec![b]
        );
        assert_eq!(
            idx.accounts_for_bank(&z).copied().collect::<Vec<_>>(),
            vec![a]
        );
        idx.remove_account(&b);
        idx.remove_account(&a);
        assert!(idx.bank_to_accounts.is_empty() && idx.account_to_banks.is_empty());
        // Long alternation does not grow the map.
        for i in 0..1_000 {
            let bank = Pubkey::new_unique();
            idx.set_account_banks(a, &[bank]);
            if i % 2 == 0 {
                idx.remove_account(&a);
            }
        }
        assert!(idx.bank_to_accounts.len() <= 1);
    }

    /// V2-13: the O(1) membership agrees with the bank's account list, also after a replacement.
    #[test]
    fn bank_has_account_matches_the_bank_list() {
        let (a, b, x, y) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut idx = ReverseIndex::new();
        idx.set_account_banks(x, &[a, b]);
        idx.set_account_banks(y, &[b]);
        assert!(idx.bank_has_account(&a, &x) && idx.bank_has_account(&b, &y));
        assert!(!idx.bank_has_account(&a, &y));
        idx.set_account_banks(x, &[b]);
        assert!(!idx.bank_has_account(&a, &x) && idx.bank_has_account(&b, &x));
        idx.remove_account(&y);
        assert!(!idx.bank_has_account(&b, &y));
    }

    #[test]
    fn reverse_index_fanout_and_replacement() {
        let mut ix = ReverseIndex::new();
        let (o, b1, b2, a1, a2) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        ix.set_bank_oracles(b1, &[o]);
        ix.set_bank_oracles(b2, &[o]);
        ix.set_account_banks(a1, &[b1]);
        ix.set_account_banks(a2, &[b1, b2]);
        let fanout: HashSet<Pubkey> = ix
            .banks_for_oracle(&o)
            .flat_map(|b| ix.accounts_for_bank(b))
            .copied()
            .collect();
        assert_eq!(fanout.len(), 2);
        ix.set_account_banks(a1, &[b2]);
        assert!(!ix.accounts_for_bank(&b1).any(|x| x == &a1));
        assert!(ix.accounts_for_bank(&b2).any(|x| x == &a1));
        // V2-14 P5c-05: the same set again (reordered, repeated) keeps every mapping.
        ix.set_account_banks(a2, &[b2, b1, b2]);
        assert!(ix.bank_has_account(&b1, &a2) && ix.bank_has_account(&b2, &a2));
        assert_eq!(ix.banks_for_account(&a2).count(), 2);
        ix.remove_account(&a2);
        assert_eq!(ix.accounts_for_bank(&b1).count(), 0);
        // Bank wechselt Oracle: alte Zuordnung weg
        let o2 = Pubkey::new_unique();
        ix.set_bank_oracles(b1, &[o2]);
        assert!(!ix.banks_for_oracle(&o).any(|x| x == &b1));
        assert!(ix.banks_for_oracle(&o2).any(|x| x == &b1));
    }

    #[test]
    fn threshold_lookup_both_sides() {
        let mut t = ThresholdIndex::new();
        let bank = Pubkey::new_unique();
        let (a_asset, a_liab, far) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        // Asset-Halter wird liquidierbar, wenn Preis <= 100
        t.set_account(a_asset, &[(bank, Side::Asset, 100)]);
        // Schuldner wird liquidierbar, wenn Preis >= 200
        t.set_account(a_liab, &[(bank, Side::Liability, 200)]);
        t.set_account(far, &[(bank, Side::Asset, 10)]);
        let c = t.candidates(&bank, 150, 150, 0);
        assert!(c.is_empty(), "150 liegt zwischen beiden Schwellen: {:?}", c);
        let c = t.candidates(&bank, 95, 95, 0);
        assert_eq!(c, vec![a_asset]);
        let c = t.candidates(&bank, 250, 250, 0);
        assert_eq!(c, vec![a_liab]);
        // margin zieht nahe Accounts hinein
        let c = t.candidates(&bank, 105, 105, 10);
        assert_eq!(c, vec![a_asset]);
        // Low/High sind getrennte Risk-Preisachsen: nur High überschreitet Liability.
        let c = t.candidates(&bank, 150, 250, 0);
        assert_eq!(c, vec![a_liab]);
        // Update ersetzt alte Schwelle
        t.set_account(a_asset, &[(bank, Side::Asset, 50)]);
        assert!(t.candidates(&bank, 95, 95, 0).is_empty());
        assert_eq!(t.candidates(&bank, 40, 40, 0), vec![a_asset]);
        t.remove_account(&a_asset);
        assert_eq!(t.tracked_accounts(), 2);
    }
}
