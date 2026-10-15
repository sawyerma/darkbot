//! [FIX-111][R6] Read-only post-simulation overlay for exact refresh projection.
//!
//! The canonical reconciled state is never mutated by a simulation. Accounts returned by
//! `simulateTransaction(accounts=...)` are held in this overlay and take precedence only for
//! the candidate evaluation that explicitly opted into the projection.

use crate::health::AccountSource;
use solana_sdk::{account::Account, instruction::Instruction, pubkey::Pubkey};
use std::collections::HashMap;

pub struct ProjectedState<'a, S: AccountSource + ?Sized> {
    base: &'a S,
    overlay: HashMap<Pubkey, Account>,
    projection_slot: u64,
    projection_unix_s: u64,
}

impl<'a, S: AccountSource + ?Sized> ProjectedState<'a, S> {
    pub fn new(
        base: &'a S,
        projection_slot: u64,
        projection_unix_s: i64,
        accounts: impl IntoIterator<Item = (Pubkey, Account)>,
    ) -> Result<Self, String> {
        if projection_slot == 0 {
            return Err("PROJECTION_SLOT_ZERO".into());
        }
        let projection_unix_s = u64::try_from(projection_unix_s)
            .map_err(|_| "PROJECTION_TIMESTAMP_NEGATIVE".to_string())?;
        let mut overlay = HashMap::new();
        for (key, account) in accounts {
            if overlay.insert(key, account).is_some() {
                return Err(format!("PROJECTION_DUPLICATE_ACCOUNT:{key}"));
            }
        }
        if overlay.is_empty() {
            return Err("PROJECTION_EMPTY_OVERLAY".into());
        }
        Ok(Self {
            base,
            overlay,
            projection_slot,
            projection_unix_s,
        })
    }

    pub fn projection_slot(&self) -> u64 {
        self.projection_slot
    }

    /// Slot and timestamp come from the same simulation Clock, never the later host clock.
    pub fn evaluation_time(&self) -> (u64, u64) {
        (self.projection_unix_s, self.projection_slot)
    }

    pub fn overlay_len(&self) -> usize {
        self.overlay.len()
    }

    pub fn contains_projected(&self, key: &Pubkey) -> bool {
        self.overlay.contains_key(key)
    }
}

impl<'a, S: AccountSource + ?Sized> AccountSource for ProjectedState<'a, S> {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        if let Some(a) = self.overlay.get(key) {
            return Some((a.owner, a.lamports, a.data.as_slice()));
        }
        self.base.account(key)
    }
}

/// Accounts whose post-refresh bytes must be requested from simulation.
///
/// Only writable accounts of the refresh prefix can have changed. Program ids, sysvars and
/// readonly oracle inputs are intentionally excluded. Ordering is canonical and stable.
pub fn projection_targets(refreshes: &[Instruction]) -> Vec<Pubkey> {
    let mut keys: Vec<Pubkey> = refreshes
        .iter()
        .flat_map(|ix| ix.accounts.iter())
        .filter(|m| m.is_writable)
        .map(|m| m.pubkey)
        .collect();
    keys.sort_unstable_by_key(|k| k.to_bytes());
    keys.dedup();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Base {
        accounts: HashMap<Pubkey, Account>,
    }

    impl AccountSource for Base {
        fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.accounts
                .get(key)
                .map(|a| (a.owner, a.lamports, a.data.as_slice()))
        }
    }

    fn account(owner: Pubkey, lamports: u64, data: &[u8]) -> Account {
        Account {
            lamports,
            data: data.to_vec(),
            owner,
            executable: false,
            rent_epoch: 0,
        }
    }

    #[test]
    fn projected_account_overrides_base_without_mutating_it() {
        let key = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let base_account = account(owner, 1, &[1, 2, 3]);
        let projected_account = account(owner, 2, &[9, 8, 7]);
        let base = Base {
            accounts: HashMap::from([(key, base_account)]),
        };

        let projected =
            ProjectedState::new(&base, 42, 1_000, [(key, projected_account)]).expect("projection");
        let (_, lamports, data) = projected.account(&key).expect("overlay");
        assert_eq!(lamports, 2);
        assert_eq!(data, &[9, 8, 7]);

        let (_, base_lamports, base_data) = base.account(&key).expect("base");
        assert_eq!(base_lamports, 1);
        assert_eq!(base_data, &[1, 2, 3]);
        assert_eq!(projected.projection_slot(), 42);
    }

    #[test]
    fn non_projected_account_falls_through_to_base() {
        let key = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let base = Base {
            accounts: HashMap::from([(key, account(owner, 5, &[4]))]),
        };
        let projected = ProjectedState::new(
            &base,
            7,
            1_000,
            [(other, account(owner, 9, &[8]))],
        )
        .expect("projection");

        let (_, lamports, data) = projected.account(&key).expect("base fallback");
        assert_eq!(lamports, 5);
        assert_eq!(data, &[4]);
    }

    #[test]
    fn writable_refresh_accounts_are_canonical_projection_targets() {
        let a = Pubkey::new_unique();
        let b = Pubkey::new_unique();
        let readonly = Pubkey::new_unique();
        let p = Pubkey::new_unique();
        let ixs = vec![
            Instruction {
                program_id: p,
                accounts: vec![
                    solana_sdk::instruction::AccountMeta::new(b, false),
                    solana_sdk::instruction::AccountMeta::new_readonly(readonly, false),
                ],
                data: vec![1],
            },
            Instruction {
                program_id: p,
                accounts: vec![
                    solana_sdk::instruction::AccountMeta::new(a, false),
                    solana_sdk::instruction::AccountMeta::new(b, false),
                ],
                data: vec![2],
            },
        ];

        let got = projection_targets(&ixs);
        let mut expected = vec![a, b];
        expected.sort_unstable_by_key(|k| k.to_bytes());
        assert_eq!(got, expected);
    }

    #[test]
    fn empty_and_zero_slot_projection_fail_closed() {
        let base = Base {
            accounts: HashMap::new(),
        };
        assert!(ProjectedState::new(&base, 0, 1_000, [(Pubkey::new_unique(), Account::default())])
            .is_err());
        assert!(ProjectedState::new(
            &base,
            1,
            1_000,
            std::iter::empty::<(Pubkey, Account)>(),
        )
        .is_err());
    }
    #[test]
    fn negative_simulation_timestamp_fails_closed() {
        let base = Base { accounts: HashMap::new() };
        assert!(ProjectedState::new(&base, 42, -1,
            [(Pubkey::new_unique(), Account::default())]).is_err());
    }

    #[cfg(feature = "crate_health")]
    #[test]
    fn delayed_evaluation_uses_exact_simulation_clock_for_both_venues() {
        use bytemuck::Zeroable;
        // Pinned marginfi mrgn-0.1.11-rc1: JupLend is timestamp-based;
        // Kamino is slot-based. Exercise the actual dependency freshness methods.
        let mut lending = juplend_mocks::state::Lending::zeroed();
        lending.last_update_timestamp = 1_000;
        let mut reserve = kamino_mocks::state::MinimalReserve::zeroed();
        reserve.slot = 42;
        let base = Base { accounts: HashMap::new() };
        let projected = ProjectedState::new(&base, 42, 1_000,
            [(Pubkey::new_unique(), Account::default())]).unwrap();
        let (unix_s, slot) = projected.evaluation_time();
        assert_eq!((unix_s, slot), (1_000, 42));
        assert!(!lending.is_stale(unix_s as i64));
        assert!(!reserve.is_stale(slot));
        assert!(lending.is_stale(1_001));
        assert!(reserve.is_stale(43));
        assert_eq!(projected.evaluation_time(), (1_000, 42));
    }

}
