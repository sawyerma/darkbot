//! P8-RES-01: global atomic wallet reservation. Every attempt holds its exact base units of SOL
//! (fee + tip + rent debit) and of each mint it may spend — the FE-04 `Intent.holds` convention,
//! `"SOL"` or the mint key. A reservation is all-or-nothing against the reconciled balances minus
//! the holds of every other attempt, so the same lamports or tokens are never allocated twice;
//! an unknown balance is never available.
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalletReservations {
    attempts: BTreeMap<String, BTreeMap<String, u64>>,
    /// P8-RES-02: a target belongs to at most one attempt (contending targets).
    targets: BTreeMap<String, String>,
}

impl WalletReservations {
    /// What all attempts hold of `asset`.
    pub fn held(&self, asset: &str) -> u64 {
        self.held_except(asset, None)
    }

    /// What every attempt except `attempt` holds, per asset (FE-04 `reserved_elsewhere`).
    pub fn held_by_others(&self, attempt: &str) -> BTreeMap<String, u64> {
        let mut out: BTreeMap<String, u64> = BTreeMap::new();
        for (id, holds) in &self.attempts {
            if id != attempt {
                for (asset, n) in holds {
                    let total = out.entry(asset.clone()).or_default();
                    *total = total.saturating_add(*n);
                }
            }
        }
        out
    }

    fn held_except(&self, asset: &str, attempt: Option<&str>) -> u64 {
        self.attempts
            .iter()
            .filter(|(id, _)| Some(id.as_str()) != attempt)
            .filter_map(|(_, h)| h.get(asset))
            .fold(0u64, |s, n| s.saturating_add(*n))
    }

    /// The reconciled balance of `asset` that no attempt holds; `None` for an unknown balance.
    pub fn available(&self, balances: &BTreeMap<String, u64>, asset: &str) -> Option<u64> {
        balances
            .get(asset)
            .map(|b| b.saturating_sub(self.held(asset)))
    }

    /// Reserves every hold of `attempt` at once, or nothing.
    pub fn reserve(
        &mut self,
        attempt: &str,
        holds: &BTreeMap<String, u64>,
        balances: &BTreeMap<String, u64>,
    ) -> Result<(), &'static str> {
        if attempt.is_empty() {
            return Err("RESERVATION_ATTEMPT_ID_MISSING");
        }
        if self.attempts.contains_key(attempt) {
            return Err("RESERVATION_EXISTS");
        }
        for (asset, need) in holds {
            let available = self
                .available(balances, asset)
                .ok_or("RESERVATION_BALANCE_UNKNOWN")?;
            if *need > available {
                return Err("RESERVATION_INSUFFICIENT");
            }
        }
        self.attempts.insert(attempt.into(), holds.clone());
        Ok(())
    }

    /// Reserves `attempt` for `target`: refused while another attempt holds the same target.
    pub fn reserve_target(
        &mut self,
        attempt: &str,
        target: &str,
        holds: &BTreeMap<String, u64>,
        balances: &BTreeMap<String, u64>,
    ) -> Result<(), &'static str> {
        if self.targets.get(target).is_some_and(|a| a != attempt) {
            return Err("RESERVATION_TARGET_CONTENDED");
        }
        self.reserve(attempt, holds, balances)?;
        self.targets.insert(target.into(), attempt.into());
        Ok(())
    }

    /// P8-RES-02: restores a hold a durable journal still carries after a restart — as it was,
    /// without a balance check (its units were committed before the crash); a duplicate attempt
    /// or a contended target is corrupt evidence.
    pub fn restore(
        &mut self,
        attempt: &str,
        target: &str,
        holds: &BTreeMap<String, u64>,
    ) -> Result<(), &'static str> {
        if attempt.is_empty() {
            return Err("RESERVATION_ATTEMPT_ID_MISSING");
        }
        if self.attempts.contains_key(attempt) {
            return Err("RESERVATION_EXISTS");
        }
        if self.targets.contains_key(target) {
            return Err("RESERVATION_TARGET_CONTENDED");
        }
        self.attempts.insert(attempt.into(), holds.clone());
        self.targets.insert(target.into(), attempt.into());
        Ok(())
    }

    /// P8-RES-02: the book after a restart, from every not finalized FE-04 hold (reserved or with
    /// an UNKNOWN outcome — both stay held until their finalized reconciliation).
    pub fn rebuild(
        holds: impl IntoIterator<Item = crate::execution::one_shot::ActiveHold>,
    ) -> Result<Self, &'static str> {
        let mut book = Self::default();
        for h in holds {
            book.restore(&h.attempt, &h.target, &h.holds)?;
        }
        Ok(book)
    }

    /// Releases `attempt` (after its finalized reconciliation) and returns what it held.
    pub fn release(&mut self, attempt: &str) -> Result<BTreeMap<String, u64>, &'static str> {
        let held = self.attempts.remove(attempt).ok_or("RESERVATION_UNKNOWN")?;
        self.targets.retain(|_, a| a != attempt);
        Ok(held)
    }

    pub fn attempts(&self) -> impl Iterator<Item = (&str, &BTreeMap<String, u64>)> {
        self.attempts.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &[(&str, u64)]) -> BTreeMap<String, u64> {
        entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    /// P8-RES-01: attempts never share lamports or tokens; a reservation is all-or-nothing, an
    /// unknown balance is never available, release returns exactly what was held.
    #[test]
    fn reservations_never_allocate_the_same_units_twice() {
        let usdc = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
        let balances = map(&[("SOL", 100), (usdc, 1_000)]);
        let mut book = WalletReservations::default();
        book.reserve("a", &map(&[("SOL", 60), (usdc, 700)]), &balances)
            .unwrap();
        assert_eq!(book.available(&balances, "SOL"), Some(40));
        assert_eq!(book.available(&balances, usdc), Some(300));
        // The second attempt fits SOL but not the mint: nothing of it is reserved.
        assert_eq!(
            book.reserve("b", &map(&[("SOL", 30), (usdc, 301)]), &balances),
            Err("RESERVATION_INSUFFICIENT")
        );
        assert_eq!(book.held("SOL"), 60);
        assert_eq!(
            book.reserve("b", &map(&[("SOL", 40), (usdc, 300)]), &balances),
            Ok(())
        );
        assert_eq!(
            (
                book.available(&balances, "SOL"),
                book.available(&balances, usdc)
            ),
            (Some(0), Some(0))
        );
        assert_eq!(book.held_by_others("b"), map(&[("SOL", 60), (usdc, 700)]));
        // Unknown balance, duplicate attempt, missing id.
        let other = "So11111111111111111111111111111111111111112";
        assert_eq!(
            book.reserve("c", &map(&[(other, 1)]), &balances),
            Err("RESERVATION_BALANCE_UNKNOWN")
        );
        assert_eq!(book.available(&balances, other), None);
        assert_eq!(
            book.reserve("a", &map(&[]), &balances),
            Err("RESERVATION_EXISTS")
        );
        assert_eq!(
            book.reserve("", &map(&[]), &balances),
            Err("RESERVATION_ATTEMPT_ID_MISSING")
        );
        // A reconciled lower balance never turns holds into negative availability.
        let lower = map(&[("SOL", 50), (usdc, 1_000)]);
        assert_eq!(book.available(&lower, "SOL"), Some(0));
        assert_eq!(book.release("a"), Ok(map(&[("SOL", 60), (usdc, 700)])));
        assert_eq!(book.release("a"), Err("RESERVATION_UNKNOWN"));
        assert_eq!(book.available(&balances, "SOL"), Some(60));
    }
}
