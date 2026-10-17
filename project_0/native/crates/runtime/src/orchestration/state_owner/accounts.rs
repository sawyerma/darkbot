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
    if owner != *program {
        return None;
    }
    darkbot_project0::protocol::decode_bank(data).ok().copied()
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

/// [C0 Ticket 2] Census membership of one program account: a debtor of the group (decodable,
/// in the group, at least one active liability), or a group member the pinned crate cannot
/// decode (group read from the layout-stable prefix) — never silently dropped.
#[cfg(feature = "crate_health")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CensusMember {
    Debtor,
    Undecodable,
}

#[cfg(feature = "crate_health")]
pub(crate) fn census_member(data: &[u8], group: &Pubkey) -> Option<CensusMember> {
    if classify_account(data) != AccountType::MarginfiAccount {
        return None;
    }
    let need = 8 + std::mem::size_of::<MarginfiAccount>();
    match data
        .get(8..need)
        .and_then(|b| bytemuck::try_from_bytes::<MarginfiAccount>(b).ok())
    {
        Some(account) => {
            (account.group == *group
                && account.lending_account.balances.iter().any(|b| {
                    b.is_active() && matches!(b.get_side(), Some(BalanceSide::Liabilities))
                }))
            .then_some(CensusMember::Debtor)
        }
        // `group` is the first field of every MarginfiAccount layout.
        None => (data.get(8..40) == Some(group.as_ref())).then_some(CensusMember::Undecodable),
    }
}

/// Debtors and undecodable members of the group, collected in the reverse-index scan.
#[cfg(feature = "crate_health")]
#[derive(Debug, Default)]
pub(crate) struct CensusPopulation {
    pub debtors: std::collections::HashSet<Pubkey>,
    pub undecodable: Vec<Pubkey>,
}

#[cfg(feature = "crate_health")]
pub(crate) fn rebuild_reverse_index(
    st: &StateCore,
    program: &Pubkey,
    group: &Pubkey,
    census: Option<&mut CensusPopulation>,
) -> (ReverseIndex, usize, usize, usize) {
    let mut census = census;
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
        if let Some(population) = census.as_deref_mut() {
            match census_member(&record.data, group) {
                Some(CensusMember::Debtor) => {
                    population.debtors.insert(*pk);
                }
                Some(CensusMember::Undecodable) => population.undecodable.push(*pk),
                None => {}
            }
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
pub(crate) fn bank_filters(group: &Pubkey) -> Vec<ProgramFilter> {
    vec![
        // [FIX-130] Do not pin Bank account size in RPC/WSS filters. 0.1.12 grows the
        // Bank payload from 1856 to 3904 bytes while preserving the legacy prefix.
        // Exact supported lengths remain enforced by protocol classification/decoding.
        ProgramFilter::Memcmp {
            offset: 0,
            bytes: protocol::DISC_BANK.to_vec(),
        },
        ProgramFilter::Memcmp {
            offset: 41,
            bytes: group.to_bytes().to_vec(),
        },
    ]
}

#[cfg(feature = "crate_health")]
pub(crate) fn liability_account_filters(group: &Pubkey) -> Vec<ProgramFilter> {
    vec![
        ProgramFilter::DataSize((8 + std::mem::size_of::<MarginfiAccount>()) as u64),
        ProgramFilter::Memcmp {
            offset: 8,
            bytes: group.to_bytes().to_vec(),
        },
        // IndexerFlags.is_lending_only == 0 (one zero byte).
        ProgramFilter::Memcmp {
            offset: 8 + 2248,
            bytes: vec![0],
        },
    ]
}

/// [C0-FIX-03] Canonical evaluation time: the Clock sysvar of the canonical state (a required
/// dependency). Health, premium and `t_profitable` evaluate at its slot and unix timestamp; a
/// missing, foreign or inconsistent clock is an error, never local or zero time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChainClock {
    pub slot: u64,
    pub unix_timestamp: i64,
    /// Slot of the state record that carried this clock (provenance).
    pub record_slot: u64,
}

impl ChainClock {
    pub(crate) fn unix_s(&self) -> u64 {
        self.unix_timestamp as u64
    }
}

/// `solana_sdk::clock::Clock` bincode layout: slot, epoch_start_timestamp, epoch,
/// leader_schedule_epoch, unix_timestamp (5 × 8 bytes).
pub(crate) fn decode_clock(
    owner: &Pubkey,
    data: &[u8],
    record_slot: u64,
) -> Result<ChainClock, &'static str> {
    if *owner != solana_sdk::sysvar::id() {
        return Err("CHAIN_CLOCK_OWNER");
    }
    let data: &[u8; 40] = data.try_into().map_err(|_| "CHAIN_CLOCK_LAYOUT")?;
    let slot = u64::from_le_bytes(data[0..8].try_into().expect("len"));
    let unix_timestamp = i64::from_le_bytes(data[32..40].try_into().expect("len"));
    if unix_timestamp <= 0 {
        return Err("CHAIN_CLOCK_TIME");
    }
    if slot > record_slot {
        return Err("CHAIN_CLOCK_SLOT_AHEAD_OF_RECORD");
    }
    Ok(ChainClock {
        slot,
        unix_timestamp,
        record_slot,
    })
}

pub(crate) fn chain_clock(state: &StateCore) -> Result<ChainClock, &'static str> {
    let r = state
        .get(&solana_sdk::sysvar::clock::id())
        .ok_or("CHAIN_CLOCK_MISSING")?;
    decode_clock(&r.owner, &r.data, r.slot)
}

/// Wall time for bookkeeping only (ledger days, TTLs, submission records), never for protocol
/// evaluation. A host clock before the Unix epoch is an error, not time zero.
pub(crate) fn wall_unix_s() -> Result<u64, &'static str> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| "WALL_CLOCK_BEFORE_UNIX_EPOCH")
}
