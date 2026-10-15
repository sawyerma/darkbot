//! [FIX-124] Published P0 group LUTs, decoded from the reconciled account state.
//! Public addresses: 0dotxyz/p0-ts-sdk src/constants/luts.consts.ts (2026-10-04).
//! No hotpath RPC and no creation/funding of lookup tables.
use darkbot_project0::health::AccountSource;
use solana_address_lookup_table_interface::state::AddressLookupTable;
use solana_message::AddressLookupTableAccount;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub fn published_keys(group: &Pubkey) -> Vec<Pubkey> {
    if group.to_string() != "4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8" {
        return vec![];
    }
    [
        "8dk6oxKXC4iMLMa8Q4AVtPJPkEfitZuHSB4JqUsLLV2R",
        "43xneYSZYLFkaksMcfBMJANibRiKsWgxkPbsrmcYjqF6",
        "67cS6yqmAy5D1QRJwAXcFv9GLvrMbe88GFGDJcntkL7C",
        "FhVyrX2gfHmxKHq8wWWBJB3ZvHZcAQ6UAcjWxUyfyXuE",
        "GYEJ3BcgwHfNSL2BAuDcth3KKKt9eiK14EHwLCjU9Eoo",
    ].iter().map(|s| Pubkey::from_str(s).expect("published LUT key")).collect()
}

/// Optional optimization: malformed, absent, deactivating or future-state tables
/// are excluded. The compiler can still use static addresses and enforce limits.
fn decode(key: Pubkey, owner: Pubkey, data: &[u8], slot: u64) -> Option<AddressLookupTableAccount> {
    if owner.to_bytes() != solana_address_lookup_table_interface::program::id().to_bytes() {
        return None;
    }
    let table = AddressLookupTable::deserialize(data).ok()?;
    if table.meta.deactivation_slot != u64::MAX || table.meta.last_extended_slot > slot
        || table.addresses.len() > 256 {
        return None;
    }
    let len = if slot == table.meta.last_extended_slot {
        usize::from(table.meta.last_extended_slot_start_index)
    } else { table.addresses.len() };
    let addresses = table.addresses.get(..len)?.iter()
        .map(|key| Pubkey::new_from_array(key.to_bytes())).collect();
    Some(AddressLookupTableAccount { key, addresses })
}

pub fn from_state<S: AccountSource + ?Sized>(state: &S, group: &Pubkey, slot: u64) -> Vec<AddressLookupTableAccount> {
    published_keys(group).into_iter().filter_map(|key| {
        let (owner, _, data) = state.account(&key)?;
        decode(key, owner, data, slot)
    }).collect()
}

/// State-backed protocol tables take precedence on duplicate keys. Any omitted
/// newer entries remain static, preserving the compiler's index/address mapping.
pub fn merge(protocol: &[AddressLookupTableAccount], route: &[AddressLookupTableAccount]) -> Vec<AddressLookupTableAccount> {
    let mut merged = protocol.to_vec();
    for table in route {
        if !merged.iter().any(|t| t.key == table.key) { merged.push(table.clone()); }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_address_lookup_table_interface::state::{LookupTableMeta, LOOKUP_TABLE_META_SIZE};
    fn fixture(meta: LookupTableMeta) -> (Pubkey, Vec<u8>) {
        let mut data = vec![0; LOOKUP_TABLE_META_SIZE];
        AddressLookupTable::overwrite_meta_data(&mut data, meta).unwrap();
        data.extend_from_slice(Pubkey::new_unique().as_ref());
        data.extend_from_slice(Pubkey::new_unique().as_ref());
        (Pubkey::new_from_array(solana_address_lookup_table_interface::program::id().to_bytes()), data)
    }
    #[test]
    fn extension_slot_only_exposes_old_prefix() {
        let (owner, data) = fixture(LookupTableMeta { last_extended_slot: 100,
            last_extended_slot_start_index: 1, ..LookupTableMeta::default() });
        let key = Pubkey::new_unique();
        assert!(decode(key, owner, &data, 99).is_none());
        assert_eq!(decode(key, owner, &data, 100).unwrap().addresses.len(), 1);
        assert_eq!(decode(key, owner, &data, 101).unwrap().addresses.len(), 2);
    }
    #[test]
    fn invalid_and_deactivating_tables_are_excluded() {
        let (owner, data) = fixture(LookupTableMeta::default());
        let key = Pubkey::new_unique();
        assert!(decode(key, Pubkey::new_unique(), &data, 101).is_none());
        assert!(decode(key, owner, &data[..20], 101).is_none());
        let (_, data) = fixture(LookupTableMeta { deactivation_slot: 101, ..LookupTableMeta::default() });
        assert!(decode(key, owner, &data, 101).is_none());
        let (_, data) = fixture(LookupTableMeta { last_extended_slot: 100,
            last_extended_slot_start_index: 3, ..LookupTableMeta::default() });
        assert!(decode(key, owner, &data, 100).is_none());
    }
    #[test]
    fn tables_reduce_compiled_size_and_merge_preserves_canonical_indices() {
        use darkbot_solana::tx::{self, TxPlan, BuildVersion, Resources, TxLimits};
        use solana_sdk::{hash::Hash, instruction::{Instruction, AccountMeta}};
        let (owner, data) = fixture(LookupTableMeta::default());
        let table = decode(Pubkey::new_unique(), owner, &data, 101).unwrap();
        let mut conflicting = table.clone(); conflicting.addresses.reverse();
        let tables = merge(&[table.clone()], &[conflicting]);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].addresses, table.addresses);
        let mut plan = TxPlan { version: BuildVersion::V0, payer: Pubkey::new_unique(),
            instructions: vec![Instruction { program_id: Pubkey::new_unique(),
                accounts: table.addresses.iter().map(|a| AccountMeta::new_readonly(*a, false)).collect(), data: vec![] }],
            lookup_tables: vec![], resources: Resources { compute_unit_limit: 1_400_000, priority_micro_lamports_per_cu: 0, loaded_accounts_data_size: None } };
        let before = tx::build(&plan, Hash::default(), &TxLimits::default()).unwrap();
        plan.lookup_tables = tables;
        let after = tx::build(&plan, Hash::default(), &TxLimits::default()).unwrap();
        assert!(after.serialized_len < before.serialized_len);
        assert_eq!(after.locked_accounts, before.locked_accounts);
    }
}
