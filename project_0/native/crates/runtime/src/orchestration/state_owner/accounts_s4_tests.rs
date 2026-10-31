//! S4-02 (PHASENPLAN S4: Account Layout against the official crates): the core's own reading of
//! the liquidator's token accounts (`validated_liquidator_ata`) and its ATA derivation
//! (`associated_token_address`) against the official SPL interfaces — spl-token-interface 2.0.0,
//! spl-token-2022-interface 2.1.0 and spl-associated-token-account 8.0.0, the versions the graph
//! already builds — on accounts the official packers write.
//!
//! Explained deviation E-FROZEN: the official layout reads a frozen account; the core takes none
//! (a frozen account cannot transfer what it holds, C11).
use super::*;
use solana_program_option::COption;
use solana_program_pack::Pack;
use spl_token_2022_interface::extension::{
    immutable_owner::ImmutableOwner, BaseStateWithExtensions, BaseStateWithExtensionsMut,
    ExtensionType, StateWithExtensions, StateWithExtensionsMut,
};

struct One(Pubkey, Pubkey, Vec<u8>);
impl AccountSource for One {
    fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        (*k == self.0).then_some((self.1, 0, self.2.as_slice()))
    }
}

fn programs() -> (Pubkey, Pubkey) {
    (
        Pubkey::from_str(protocol::TOKEN_PROGRAM_ID).unwrap(),
        Pubkey::from_str(protocol::TOKEN_2022_PROGRAM_ID).unwrap(),
    )
}

/// An SPL Token account as the official packer writes it.
fn token_account(
    mint: Pubkey,
    owner: Pubkey,
    amount: u64,
    state: spl_token_interface::state::AccountState,
) -> Vec<u8> {
    let a = spl_token_interface::state::Account {
        mint,
        owner,
        amount,
        delegate: COption::Some(Pubkey::new_unique()),
        state,
        is_native: COption::Some(2_039_280),
        delegated_amount: 17,
        close_authority: COption::Some(Pubkey::new_unique()),
    };
    let mut data = vec![0u8; spl_token_interface::state::Account::LEN];
    a.pack_into_slice(&mut data);
    data
}

/// A Token-2022 account as the official packer writes it: the base, its account type and, as
/// every Token-2022 ATA, the ImmutableOwner extension (none: the bare 165-byte base).
fn token_2022_account(mint: Pubkey, owner: Pubkey, amount: u64, extended: bool) -> Vec<u8> {
    use spl_token_2022_interface::state::{Account, AccountState};
    let extensions: &[ExtensionType] = if extended {
        &[ExtensionType::ImmutableOwner]
    } else {
        &[]
    };
    let len = ExtensionType::try_calculate_account_len::<Account>(extensions).unwrap();
    let mut data = vec![0u8; len];
    let mut s = StateWithExtensionsMut::<Account>::unpack_uninitialized(&mut data).unwrap();
    if extended {
        s.init_extension::<ImmutableOwner>(true).unwrap();
    }
    s.base = Account {
        mint,
        owner,
        amount,
        state: AccountState::Initialized,
        ..Default::default()
    };
    s.pack_base();
    if extended {
        s.init_account_type().unwrap();
    }
    data
}

/// The core derives exactly the official ATA of both token programs.
#[test]
fn s4_02_the_liquidator_atas_are_the_official_addresses() {
    let (token, token_2022) = programs();
    for _ in 0..8 {
        let (liquidator, mint) = (Pubkey::new_unique(), Pubkey::new_unique());
        let atas = LiquidatorAtas::new(liquidator);
        let official = |tp: &Pubkey| {
            spl_associated_token_account::get_associated_token_address_with_program_id(
                &liquidator,
                &mint,
                tp,
            )
        };
        assert_eq!(
            atas.of(&mint),
            [
                (token, official(&token)),
                (token_2022, official(&token_2022))
            ]
        );
    }
}

/// Every official encoding the core takes, it reads as the official layout does (mint, owner,
/// amount); every one the official layout refuses, the core refuses too.
#[test]
fn s4_02_token_accounts_read_as_the_official_layout() {
    use spl_token_interface::state::{Account, AccountState};
    let (token, token_2022) = programs();
    let (liquidator, mint) = (Pubkey::new_unique(), Pubkey::new_unique());
    let atas = LiquidatorAtas::new(liquidator);
    let [(_, classic_ata), (_, ata_2022)] = atas.of(&mint);
    let read = |ata: Pubkey, program: Pubkey, data: Vec<u8>| {
        validated_liquidator_ata(&One(ata, program, data), &mint, &atas)
    };

    // SPL Token: the official layout and the core read the same amount.
    let data = token_account(mint, liquidator, 123_456_789, AccountState::Initialized);
    let official = Account::unpack(&data).unwrap();
    assert_eq!(
        read(classic_ata, token, data),
        Some((classic_ata, official.amount))
    );
    // Token-2022, with and without extensions.
    for extended in [true, false] {
        let data = token_2022_account(mint, liquidator, 987_654_321, extended);
        let official =
            StateWithExtensions::<spl_token_2022_interface::state::Account>::unpack(&data).unwrap();
        assert_eq!(
            official.get_extension_types().unwrap().is_empty(),
            !extended
        );
        let amount = official.base.amount;
        assert_eq!(read(ata_2022, token_2022, data), Some((ata_2022, amount)));
    }

    // What the official layout refuses: an uninitialized account, a classic account with bytes
    // after its base, a Token-2022 account whose type is not an account.
    let uninitialized = token_account(mint, liquidator, 1, AccountState::Uninitialized);
    assert!(Account::unpack(&uninitialized).is_err());
    assert_eq!(read(classic_ata, token, uninitialized), None);
    let mut longer = token_account(mint, liquidator, 1, AccountState::Initialized);
    longer.push(2);
    assert!(Account::unpack(&longer).is_err());
    assert_eq!(read(classic_ata, token, longer), None);
    let mut not_account = token_2022_account(mint, liquidator, 1, true);
    not_account[Account::LEN] = 1; // AccountType::Mint
    assert!(
        StateWithExtensions::<spl_token_2022_interface::state::Account>::unpack(&not_account)
            .is_err()
    );
    assert_eq!(read(ata_2022, token_2022, not_account), None);

    // Read but not the liquidator's: another mint or owner, or the other token program's account.
    let other_mint = token_account(
        Pubkey::new_unique(),
        liquidator,
        1,
        AccountState::Initialized,
    );
    assert!(Account::unpack(&other_mint).is_ok());
    assert_eq!(read(classic_ata, token, other_mint), None);
    let other_owner = token_account(mint, Pubkey::new_unique(), 1, AccountState::Initialized);
    assert_eq!(read(classic_ata, token, other_owner), None);
    let classic = token_account(mint, liquidator, 1, AccountState::Initialized);
    assert_eq!(read(classic_ata, token_2022, classic), None);

    // E-FROZEN.
    let frozen = token_account(mint, liquidator, 5, AccountState::Frozen);
    assert!(Account::unpack(&frozen).unwrap().is_frozen());
    assert_eq!(read(classic_ata, token, frozen), None);
}
