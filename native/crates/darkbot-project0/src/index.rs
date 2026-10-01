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
//! Preise als u64 in Fixed-Point (Skalierung = Oracle-Exponent aus P3), keine f64.

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
    integration_to_banks: HashMap<Pubkey, HashSet<Pubkey>>,
    bank_oracles: HashMap<Pubkey, Vec<Pubkey>>,
    bank_integration: HashMap<Pubkey, Pubkey>,
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

    /// Ersetzt die Integration einer Bank; alte Zuordnung wird entfernt.
    pub fn set_bank_integration(&mut self, bank: Pubkey, integration: Option<Pubkey>) {
        if let Some(old) = self.bank_integration.remove(&bank) {
            if let Some(set) = self.integration_to_banks.get_mut(&old) {
                set.remove(&bank);
                if set.is_empty() {
                    self.integration_to_banks.remove(&old);
                }
            }
        }
        if let Some(i) = integration {
            self.integration_to_banks.entry(i).or_default().insert(bank);
            self.bank_integration.insert(bank, i);
        }
    }

    /// Setzt die Bank-Menge eines Accounts (bei jedem Account-Update aufrufen;
    /// alte Zuordnungen werden entfernt).
    pub fn set_account_banks(&mut self, account: Pubkey, banks: &[Pubkey]) {
        if let Some(old) = self.account_to_banks.remove(&account) {
            for b in old {
                if let Some(set) = self.bank_to_accounts.get_mut(&b) {
                    set.remove(&account);
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

    pub fn remove_account(&mut self, account: &Pubkey) {
        self.set_account_banks(*account, &[]);
        self.account_to_banks.remove(account);
    }

    pub fn banks_for_oracle(&self, oracle: &Pubkey) -> impl Iterator<Item = &Pubkey> {
        self.oracle_to_banks.get(oracle).into_iter().flat_map(|s| s.iter())
    }

    pub fn accounts_for_bank(&self, bank: &Pubkey) -> impl Iterator<Item = &Pubkey> {
        self.bank_to_accounts.get(bank).into_iter().flat_map(|s| s.iter())
    }

    pub fn banks_for_account(&self, account: &Pubkey) -> impl Iterator<Item = &Pubkey> {
        self.account_to_banks.get(account).into_iter().flat_map(|s| s.iter())
    }

    pub fn banks_for_integration(&self, integration: &Pubkey) -> impl Iterator<Item = &Pubkey> {
        self.integration_to_banks.get(integration).into_iter().flat_map(|s| s.iter())
    }

    /// Oracle-Update → alle potenziell betroffenen Accounts (ohne Schwellen-Filter; Messgröße
    /// `affected_accounts_per_update` für das Ledger).
    pub fn accounts_for_oracle(&self, oracle: &Pubkey) -> HashSet<Pubkey> {
        let mut out = HashSet::new();
        for b in self.banks_for_oracle(oracle) {
            for a in self.accounts_for_bank(b) {
                out.insert(*a);
            }
        }
        out
    }
}

/// Pro Bank: Schwellen sortiert nach Preis. Zwei Seiten getrennt.
#[derive(Default, Debug)]
pub struct ThresholdIndex {
    /// Asset-Seite: liquidierbar wenn price <= key. Sortiert aufsteigend → Kandidaten = range(price..)
    asset: HashMap<Pubkey, BTreeMap<u64, HashSet<Pubkey>>>,
    /// Liability-Seite: liquidierbar wenn price >= key → Kandidaten = range(..=price)
    liability: HashMap<Pubkey, BTreeMap<u64, HashSet<Pubkey>>>,
    /// Rückwärts, damit ein Account-Update seine alten Einträge löschen kann.
    entries: HashMap<Pubkey, Vec<(Pubkey, Side, u64)>>,
}

impl ThresholdIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ersetzt alle Schwellen eines Accounts. `thresholds`: (bank, side, liq_price).
    /// Accounts ohne Schwelle (z. B. keine Liabilities) übergeben eine leere Liste.
    pub fn set_account(&mut self, account: Pubkey, thresholds: &[(Pubkey, Side, u64)]) {
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

    /// Kandidaten für einen neuen Bank-Preis: exakt nachzurechnen, nicht "liquidierbar".
    /// `margin` erweitert den Bereich (Preisfehler/Confidence), damit nahe Accounts
    /// mitgeprüft werden.
    pub fn candidates(&self, bank: &Pubkey, price: u64, margin: u64) -> Vec<Pubkey> {
        let mut out: HashSet<Pubkey> = HashSet::new();
        if let Some(m) = self.asset.get(bank) {
            // liquidierbar wenn price <= key  → alle keys >= price - margin
            let lo = price.saturating_sub(margin);
            for (_, set) in m.range(lo..) {
                out.extend(set.iter().copied());
            }
        }
        if let Some(m) = self.liability.get(bank) {
            // liquidierbar wenn price >= key → alle keys <= price + margin
            let hi = price.saturating_add(margin);
            for (_, set) in m.range(..=hi) {
                out.extend(set.iter().copied());
            }
        }
        out.into_iter().collect()
    }

    pub fn tracked_accounts(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_index_fanout_and_replacement() {
        let mut ix = ReverseIndex::new();
        let (o, b1, b2, a1, a2) = (Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique());
        ix.set_bank_oracles(b1, &[o]);
        ix.set_bank_oracles(b2, &[o]);
        ix.set_account_banks(a1, &[b1]);
        ix.set_account_banks(a2, &[b1, b2]);
        assert_eq!(ix.accounts_for_oracle(&o).len(), 2);
        ix.set_account_banks(a1, &[b2]);
        assert!(!ix.accounts_for_bank(&b1).any(|x| x == &a1));
        assert!(ix.accounts_for_bank(&b2).any(|x| x == &a1));
        ix.remove_account(&a2);
        assert_eq!(ix.accounts_for_bank(&b1).count(), 0);
        // Bank wechselt Oracle: alte Zuordnung weg
        let o2 = Pubkey::new_unique();
        ix.set_bank_oracles(b1, &[o2]);
        assert!(!ix.banks_for_oracle(&o).any(|x| x == &b1));
        assert!(ix.banks_for_oracle(&o2).any(|x| x == &b1));
        let i1 = Pubkey::new_unique();
        ix.set_bank_integration(b1, Some(i1));
        ix.set_bank_integration(b1, None);
        assert_eq!(ix.banks_for_integration(&i1).count(), 0);
    }

    #[test]
    fn threshold_lookup_both_sides() {
        let mut t = ThresholdIndex::new();
        let bank = Pubkey::new_unique();
        let (a_asset, a_liab, far) = (Pubkey::new_unique(), Pubkey::new_unique(), Pubkey::new_unique());
        // Asset-Halter wird liquidierbar, wenn Preis <= 100
        t.set_account(a_asset, &[(bank, Side::Asset, 100)]);
        // Schuldner wird liquidierbar, wenn Preis >= 200
        t.set_account(a_liab, &[(bank, Side::Liability, 200)]);
        t.set_account(far, &[(bank, Side::Asset, 10)]);
        let c = t.candidates(&bank, 150, 0);
        assert!(c.is_empty(), "150 liegt zwischen beiden Schwellen: {:?}", c);
        let c = t.candidates(&bank, 95, 0);
        assert_eq!(c, vec![a_asset]);
        let c = t.candidates(&bank, 250, 0);
        assert_eq!(c, vec![a_liab]);
        // margin zieht nahe Accounts hinein
        let c = t.candidates(&bank, 105, 10);
        assert_eq!(c, vec![a_asset]);
        // Update ersetzt alte Schwelle
        t.set_account(a_asset, &[(bank, Side::Asset, 50)]);
        assert!(t.candidates(&bank, 95, 0).is_empty());
        assert_eq!(t.candidates(&bank, 40, 0), vec![a_asset]);
        t.remove_account(&a_asset);
        assert_eq!(t.tracked_accounts(), 2);
    }
}
