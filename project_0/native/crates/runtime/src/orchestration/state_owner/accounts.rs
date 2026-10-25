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
    darkbot_project0::protocol::decode_bank(data)
        .ok()
        .map(std::borrow::Cow::into_owned)
}

#[cfg(feature = "crate_health")]
pub(crate) fn associated_token_address(
    owner: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
) -> Pubkey {
    const ATA_PROGRAM: Pubkey = Pubkey::from_str_const(protocol::ASSOCIATED_TOKEN_PROGRAM_ID);
    let ata_program = ATA_PROGRAM;
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

/// C11 latency: the liquidator's two possible ATAs (Token, Token-2022) per mint, derived once —
/// `find_program_address` hashes and curve-checks per bump on every call otherwise. Mints come
/// from bank state; beyond `MAX_MINTS` an address is derived without being kept.
pub(crate) struct LiquidatorAtas {
    liquidator: Pubkey,
    programs: [Pubkey; 2],
    known: std::cell::RefCell<HashMap<Pubkey, [Pubkey; 2]>>,
}

impl LiquidatorAtas {
    const MAX_MINTS: usize = 4_096;

    pub(crate) fn new(liquidator: Pubkey) -> Self {
        let program = |s: &str| Pubkey::from_str(s).expect("static token program");
        Self {
            liquidator,
            programs: [
                program(protocol::TOKEN_PROGRAM_ID),
                program(protocol::TOKEN_2022_PROGRAM_ID),
            ],
            known: Default::default(),
        }
    }

    /// `(token program, ATA)` of `mint` for both token programs.
    pub(crate) fn of(&self, mint: &Pubkey) -> [(Pubkey, Pubkey); 2] {
        let cached = self.known.borrow().get(mint).copied();
        let atas = match cached {
            Some(a) => a,
            None => {
                let a = self
                    .programs
                    .map(|tp| associated_token_address(&self.liquidator, mint, &tp));
                let mut known = self.known.borrow_mut();
                if known.len() < Self::MAX_MINTS {
                    known.insert(*mint, a);
                }
                a
            }
        };
        [(self.programs[0], atas[0]), (self.programs[1], atas[1])]
    }
}

#[cfg(feature = "crate_health")]
/// P8-RES-01: the liquidator's validated ATA amount of `mint` minus what every attempt holds of it
/// in the global reservation book and minus its never-spent policy reserve (P8-INV-01) — the
/// units an offer may use; no ATA is nothing.
pub(crate) fn available_liquidator_inventory<S: AccountSource + ?Sized>(
    state: &S,
    mint: &Pubkey,
    atas: &LiquidatorAtas,
    reservations: &crate::execution::reservation::WalletReservations,
    reserve: u64,
) -> u64 {
    if reservations.is_unreadable() {
        return 0;
    }
    let Some((_, amount)) = validated_liquidator_ata(state, mint, atas) else {
        return 0;
    };
    // The FE-04 keys are base58 strings: encoded only while some attempt holds anything.
    let held = if reservations.is_empty() {
        0
    } else {
        reservations.held(&mint.to_string())
    };
    amount.saturating_sub(held.saturating_add(reserve))
}

/// C11: the liquidator's ATA of `mint` and its amount — only an initialized (not frozen) token
/// account of the token program, exact base layout (Token-2022: base plus the `Account` type tag
/// before its extensions), of this mint and owned by the liquidator; anything else is no ATA.
pub(crate) fn validated_liquidator_ata<S: AccountSource + ?Sized>(
    state: &S,
    mint: &Pubkey,
    atas: &LiquidatorAtas,
) -> Option<(Pubkey, u64)> {
    const BASE: usize = 165;
    const STATE: usize = 108;
    const INITIALIZED: u8 = 1;
    const ACCOUNT_TYPE: u8 = 2;
    for (i, (token_program, ata)) in atas.of(mint).into_iter().enumerate() {
        let Some((owner, _, data)) = state.account(&ata) else {
            continue;
        };
        let layout = data.len() == BASE || (i == 1 && data.get(BASE) == Some(&ACCOUNT_TYPE));
        if owner != token_program || !layout || data[STATE] != INITIALIZED {
            continue;
        }
        if data[0..32] != mint.to_bytes() || data[32..64] != atas.liquidator.to_bytes() {
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
    // B2: strict pinned layout; an unknown or resized account is no group member.
    darkbot_project0::protocol::decode_marginfi_account(data)
        .map(|a| a.group == *group)
        .unwrap_or(false)
}

#[cfg(feature = "crate_health")]
pub(crate) fn active_banks_in_group_account(data: &[u8], group: &Pubkey) -> Option<Vec<Pubkey>> {
    // B2: strict pinned layout, never a prefix decode.
    let account = darkbot_project0::protocol::decode_marginfi_account(data).ok()?;
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
    // B2: strict pinned layout, never a prefix decode.
    darkbot_project0::protocol::decode_marginfi_account(data)
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
    // B2: only the exact pinned layout decodes; a resized account is an undecodable member.
    match darkbot_project0::protocol::decode_marginfi_account(data).ok() {
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
    // B2: strict pinned layout, never a prefix decode.
    darkbot_project0::protocol::decode_marginfi_account(data)
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
    /// F-15: `unix_timestamp` validated positive by `decode_clock`, the only constructor.
    unix_secs: u64,
}

impl ChainClock {
    /// The chain time in seconds (validated at decode; never a fallback 0).
    pub(crate) fn unix_s(&self) -> u64 {
        self.unix_secs
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
    let unix_secs = u64::try_from(unix_timestamp)
        .ok()
        .filter(|s| *s > 0)
        .ok_or("CHAIN_CLOCK_TIME")?;
    if slot > record_slot {
        return Err("CHAIN_CLOCK_SLOT_AHEAD_OF_RECORD");
    }
    Ok(ChainClock {
        slot,
        unix_timestamp,
        record_slot,
        unix_secs,
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

#[cfg(test)]
mod reservation_tests {
    use super::*;

    struct Accounts(std::collections::HashMap<Pubkey, (Pubkey, Vec<u8>)>);
    impl AccountSource for Accounts {
        fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.0.get(key).map(|(o, d)| (*o, 0, d.as_slice()))
        }
    }

    /// P8-RES-01: an offer uses only the liquidator's ATA units no attempt holds; a mint without
    /// a validated ATA offers nothing.
    #[test]
    fn offers_use_only_units_no_attempt_holds() {
        let (liquidator, mint) = (Pubkey::new_unique(), Pubkey::new_unique());
        let token = Pubkey::from_str(protocol::TOKEN_PROGRAM_ID).unwrap();
        let ata = associated_token_address(&liquidator, &mint, &token);
        let mut data = vec![0u8; 165];
        data[0..32].copy_from_slice(mint.as_ref());
        data[32..64].copy_from_slice(liquidator.as_ref());
        data[64..72].copy_from_slice(&1_000u64.to_le_bytes());
        data[108] = 1;
        let atas = LiquidatorAtas::new(liquidator);
        let state = Accounts([(ata, (token, data))].into_iter().collect());
        let mut book = crate::execution::reservation::WalletReservations::default();
        assert_eq!(
            available_liquidator_inventory(&state, &mint, &atas, &book, 0),
            1_000
        );
        let balances = [(mint.to_string(), 1_000)].into_iter().collect();
        book.reserve(
            "a",
            &[(mint.to_string(), 700)].into_iter().collect(),
            &balances,
        )
        .unwrap();
        assert_eq!(
            available_liquidator_inventory(&state, &mint, &atas, &book, 0),
            300
        );
        // P8-INV-01: the policy reserve is never offered.
        assert_eq!(
            available_liquidator_inventory(&state, &mint, &atas, &book, 120),
            180
        );
        book.reserve(
            "b",
            &[(mint.to_string(), 300)].into_iter().collect(),
            &balances,
        )
        .unwrap();
        assert_eq!(
            available_liquidator_inventory(&state, &mint, &atas, &book, 0),
            0
        );
        assert_eq!(
            available_liquidator_inventory(&state, &Pubkey::new_unique(), &atas, &book, 0),
            0
        );
        // A book whose durable journal cannot be read offers nothing.
        let unreadable = crate::execution::reservation::WalletReservations::unreadable();
        assert_eq!(
            available_liquidator_inventory(&state, &mint, &atas, &unreadable, 0),
            0
        );
    }

    fn token_account(mint: &Pubkey, owner: &Pubkey, amount: u64, len: usize) -> Vec<u8> {
        let mut data = vec![0u8; len];
        data[0..32].copy_from_slice(mint.as_ref());
        data[32..64].copy_from_slice(owner.as_ref());
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = 1;
        data
    }

    /// C11: only an initialized, unfrozen token account of the right program, exact layout, mint
    /// and owner is inventory; a balance change is seen at once, minus every hold; the ATA
    /// addresses are derived once and equal the derivation.
    #[test]
    fn c11_only_a_spendable_validated_ata_is_inventory() {
        let (liquidator, mint, other) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let token = Pubkey::from_str(protocol::TOKEN_PROGRAM_ID).unwrap();
        let token22 = Pubkey::from_str(protocol::TOKEN_2022_PROGRAM_ID).unwrap();
        let atas = LiquidatorAtas::new(liquidator);
        let [(p0, a0), (p1, a1)] = atas.of(&mint);
        assert_eq!((p0, p1), (token, token22));
        assert_eq!(a0, associated_token_address(&liquidator, &mint, &token));
        assert_eq!(a1, associated_token_address(&liquidator, &mint, &token22));
        assert_eq!(atas.of(&mint), [(p0, a0), (p1, a1)]);
        assert_eq!(atas.known.borrow().len(), 1);
        let book = crate::execution::reservation::WalletReservations::default();
        let offer = |key: Pubkey, program: Pubkey, data: Vec<u8>| {
            let state = Accounts([(key, (program, data))].into_iter().collect());
            available_liquidator_inventory(&state, &mint, &atas, &book, 0)
        };
        let valid = token_account(&mint, &liquidator, 500, 165);
        assert_eq!(offer(a0, token, valid.clone()), 500);
        for (state, name) in [(0u8, "uninitialized"), (2, "frozen")] {
            let mut d = valid.clone();
            d[108] = state;
            assert_eq!(offer(a0, token, d), 0, "{name}");
        }
        assert_eq!(
            offer(a0, token, token_account(&mint, &other, 500, 165)),
            0,
            "foreign owner"
        );
        assert_eq!(
            offer(a0, token, token_account(&other, &liquidator, 500, 165)),
            0,
            "mint"
        );
        assert_eq!(offer(a0, token22, valid.clone()), 0, "program");
        assert_eq!(
            offer(a0, token, token_account(&mint, &liquidator, 500, 170)),
            0,
            "layout"
        );
        // Token-2022: the base plus the `Account` type tag before its extensions.
        let mut extended = token_account(&mint, &liquidator, 700, 175);
        assert_eq!(offer(a1, token22, extended.clone()), 0, "no type tag");
        extended[165] = 2;
        assert_eq!(offer(a1, token22, extended), 700);
        // A balance change after a reservation is seen at once, minus the hold.
        let mut book = crate::execution::reservation::WalletReservations::default();
        let holds = [(mint.to_string(), 200)].into_iter().collect();
        book.reserve("a", &holds, &holds).unwrap();
        let mut state = Accounts([(a0, (token, valid))].into_iter().collect());
        assert_eq!(
            available_liquidator_inventory(&state, &mint, &atas, &book, 0),
            300
        );
        state.0.get_mut(&a0).unwrap().1[64..72].copy_from_slice(&250u64.to_le_bytes());
        assert_eq!(
            available_liquidator_inventory(&state, &mint, &atas, &book, 0),
            50
        );
    }
}
