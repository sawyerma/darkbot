//! Marginfi bank/account and token-account helpers of the Shadow runtime (split from main.rs,
//! structure iteration 29). Private to the p0-longtail binary.
use super::*;

#[cfg(feature = "crate_health")]
pub(crate) fn bank_from_state<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    bank_key: &Pubkey,
) -> Option<Bank> {
    let (owner, _, data) = state.account(bank_key)?;
    if owner != *program || classify_account(data) != AccountType::Bank {
        return None;
    }
    let need = 8 + std::mem::size_of::<Bank>();
    if data.len() < need {
        return None;
    }
    bytemuck::try_from_bytes::<Bank>(&data[8..need])
        .ok()
        .copied()
}

#[cfg(feature = "crate_health")]
pub(crate) fn associated_token_address(
    owner: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> Pubkey {
    let ata_program =
        Pubkey::from_str(protocol::ASSOCIATED_TOKEN_PROGRAM_ID).expect("static ATA program");
    Pubkey::find_program_address(
        &[owner.as_ref(), token_program.as_ref(), mint.as_ref()],
        &ata_program,
    )
    .0
}

#[cfg(feature = "crate_health")]
pub(crate) fn ata_candidates_for_bank<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    bank_key: &Pubkey,
    liquidator: &Pubkey,
) -> Vec<Pubkey> {
    let Some(bank) = bank_from_state(state, program, bank_key) else {
        return Vec::new();
    };
    [protocol::TOKEN_PROGRAM_ID, protocol::TOKEN_2022_PROGRAM_ID]
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .map(|tp| associated_token_address(liquidator, &bank.mint, &tp))
        .collect()
}

#[cfg(feature = "crate_health")]
pub(crate) fn validated_liquidator_ata<S: AccountSource + ?Sized>(
    state: &S,
    mint: &Pubkey,
    liquidator: &Pubkey,
) -> Option<(Pubkey, u64)> {
    for token_program in [protocol::TOKEN_PROGRAM_ID, protocol::TOKEN_2022_PROGRAM_ID]
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
    {
        let ata = associated_token_address(liquidator, mint, &token_program);
        let Some((owner, _, data)) = state.account(&ata) else {
            continue;
        };
        if owner != token_program || data.len() < 72 {
            continue;
        }
        let account_mint = Pubkey::new_from_array(data[0..32].try_into().ok()?);
        let account_owner = Pubkey::new_from_array(data[32..64].try_into().ok()?);
        if account_mint != *mint || account_owner != *liquidator {
            continue;
        }
        let amount = u64::from_le_bytes(data[64..72].try_into().ok()?);
        return Some((ata, amount));
    }
    None
}

#[cfg(feature = "crate_health")]
pub(crate) fn find_supported_cost_bank_by_mint<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    banks: &[Pubkey],
    mint: &Pubkey,
) -> Option<(Pubkey, Bank)> {
    // [FIX-94] Cost valuation must not accidentally select an integration bank merely because
    // it shares the same mint. Mirror ProtocolSizer's supported-generic-bank policy and pick
    // deterministically by pubkey.
    let mut candidates: Vec<(Pubkey, Bank)> = banks
        .iter()
        .filter_map(|bk| {
            let bank = bank_from_state(state, program, bk)?;
            let supported_tag = matches!(bank.config.asset_tag, ASSET_TAG_DEFAULT | ASSET_TAG_SOL);
            let supported_state = matches!(
                bank.config.operational_state,
                BankOperationalState::Operational
                    | BankOperationalState::ReduceOnly
                    | BankOperationalState::ReduceOnlyWithBorrowingPower
            );
            (bank.mint == *mint && supported_tag && supported_state).then_some((*bk, bank))
        })
        .collect();
    candidates.sort_unstable_by_key(|(pk, _)| pk.to_bytes());
    candidates.into_iter().next()
}

#[cfg(feature = "crate_health")]
pub(crate) fn marginfi_account_in_group(data: &[u8], group: &Pubkey) -> bool {
    if classify_account(data) != AccountType::MarginfiAccount {
        return false;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return false;
    }
    bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need])
        .map(|a| a.group == *group)
        .unwrap_or(false)
}

#[cfg(feature = "crate_health")]
pub(crate) fn active_banks_in_group_account(data: &[u8], group: &Pubkey) -> Option<Vec<Pubkey>> {
    if classify_account(data) != AccountType::MarginfiAccount {
        return None;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return None;
    }
    let account = bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need]).ok()?;
    if account.group != *group {
        return None;
    }
    let mut banks: Vec<Pubkey> = account
        .lending_account
        .balances
        .iter()
        .filter(|b| b.is_active())
        .map(|b| b.bank_pk)
        .collect();
    banks.sort_unstable_by_key(|pk| pk.to_bytes());
    banks.dedup();
    Some(banks)
}

#[cfg(feature = "crate_health")]
pub(crate) fn liability_account_in_group(data: &[u8], group: &Pubkey) -> bool {
    if classify_account(data) != AccountType::MarginfiAccount {
        return false;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return false;
    }
    bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need])
        .map(|account| {
            account.group == *group
                && account.lending_account.balances.iter().any(|b| {
                    b.is_active() && matches!(b.get_side(), Some(BalanceSide::Liabilities))
                })
        })
        .unwrap_or(false)
}

#[cfg(feature = "crate_health")]
pub(crate) fn rebuild_reverse_index(
    st: &StateCore,
    program: &Pubkey,
    group: &Pubkey,
) -> (ReverseIndex, usize, usize, usize) {
    let view = StateView(st);
    let mut index = ReverseIndex::new();
    let mut bank_count = 0usize;
    let mut account_count = 0usize;
    let mut unsupported_banks = 0usize;

    for (pk, record) in st.iter_owned_by(program) {
        if classify_account(&record.data) != AccountType::Bank {
            continue;
        }
        let Some(bank) = bank_from_state(&view, program, pk) else {
            continue;
        };
        if bank.group != *group {
            continue;
        }
        bank_count += 1;
        match bank_observation_keys(&bank) {
            Ok(_) => {
                // [R127b] Long-Tail admission must react to every subscribed dependency that
                // can change effective bank pricing/health, not only the primary oracle key.
                // Reuse the same source-backed dependency derivation as snapshot/subscription.
                let deps = darkbot_project0::health::dependency_keys(&view, program, &[*pk]);
                index.set_bank_oracles(*pk, &deps);
            }
            Err(_) => {
                unsupported_banks += 1;
                index.set_bank_oracles(*pk, &[]);
            }
        }
    }

    for (pk, record) in st.iter_owned_by(program) {
        if let Some(banks) = active_banks_in_group_account(&record.data, group) {
            index.set_account_banks(*pk, &banks);
            account_count += 1;
        }
    }

    (index, bank_count, account_count, unsupported_banks)
}

#[cfg(feature = "crate_health")]
pub(crate) fn bootstrap_probe_candidate(data: &[u8], group: &Pubkey) -> bool {
    if classify_account(data) != AccountType::MarginfiAccount {
        return false;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    if data.len() < need {
        return false;
    }
    bytemuck::try_from_bytes::<MarginfiAccount>(&data[8..need])
        .map(|a| a.group == *group && a.indexer_flags.was_liquidatable == 1)
        .unwrap_or(false)
}

#[cfg(feature = "crate_health")]
pub(crate) fn bank_filters(group: &Pubkey) -> Vec<serde_json::Value> {
    vec![
        // [FIX-130] Do not pin Bank account size in RPC/WSS filters. 0.1.12 grows the
        // Bank payload from 1856 to 3904 bytes while preserving the legacy prefix.
        // Exact supported lengths remain enforced by protocol classification/decoding.
        serde_json::json!({"memcmp": {
            "offset": 0,
            "bytes": bs58::encode(protocol::DISC_BANK).into_string()
        }}),
        serde_json::json!({"memcmp": {"offset": 41, "bytes": group.to_string()}}),
    ]
}

#[cfg(feature = "crate_health")]
pub(crate) fn liability_account_filters(group: &Pubkey) -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"dataSize": 8 + std::mem::size_of::<MarginfiAccount>()}),
        serde_json::json!({"memcmp": {"offset": 8, "bytes": group.to_string()}}),
        // IndexerFlags.is_lending_only == 0; one zero byte is base58 "1".
        serde_json::json!({"memcmp": {"offset": 8 + 2248, "bytes": "1"}}),
    ]
}
