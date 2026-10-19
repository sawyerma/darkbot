//! [FIX-109][R6] Deterministic integration refresh planning.
//!
//! Pinned source: 0dotxyz/marginfi-v2@mrgn-0.1.11-rc1.
//! These refresh instructions are the exact pre-Start instructions admitted by Marginfi's
//! receivership introspection. This module does not fake freshness or mutate projected state.
//! If required integration state cannot be decoded, planning fails closed.

use crate::health::AccountSource;
use crate::protocol;
use anchor_lang::AccountDeserialize;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;

use marginfi_type_crate::{
    constants::{ASSET_TAG_DRIFT, ASSET_TAG_JUPLEND, ASSET_TAG_KAMINO},
    pdas::{
        derive_drift_signer, derive_drift_spot_market, derive_drift_spot_market_vault,
        derive_drift_state, derive_juplend_claim_account, derive_juplend_lending_admin,
        derive_juplend_liquidity, derive_juplend_liquidity_vault, derive_juplend_rate_model,
        derive_juplend_rewards_rate_model, JUPLEND_LIQUIDITY_PROGRAM_ID,
    },
    types::{Bank, MarginfiAccount},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegrationKind {
    Kamino,
    Drift,
    JupLend,
}

pub fn kind(bank: &Bank) -> Option<IntegrationKind> {
    match bank.config.asset_tag {
        ASSET_TAG_KAMINO => Some(IntegrationKind::Kamino),
        ASSET_TAG_DRIFT => Some(IntegrationKind::Drift),
        ASSET_TAG_JUPLEND => Some(IntegrationKind::JupLend),
        _ => None,
    }
}

/// Venue used as an oracle/exchange-rate wrapper. This is intentionally distinct from
/// `kind(bank)`: a bank may require Kamino/Drift/JupLend freshness for pricing while still
/// using a generic Marginfi withdraw path (asset_tag != integration tag).
pub fn pricing_kind(bank: &Bank) -> Option<IntegrationKind> {
    use marginfi_type_crate::types::OracleSetup;
    match bank.config.oracle_setup {
        OracleSetup::KaminoPythPush
        | OracleSetup::KaminoSwitchboardPull
        | OracleSetup::FixedKamino
        | OracleSetup::KaminoMSOL
        | OracleSetup::KaminoLST => Some(IntegrationKind::Kamino),

        OracleSetup::DriftPythPull
        | OracleSetup::DriftSwitchboardPull
        | OracleSetup::FixedDrift => Some(IntegrationKind::Drift),

        OracleSetup::JuplendPythPull
        | OracleSetup::JuplendSwitchboardPull
        | OracleSetup::FixedJuplend
        | OracleSetup::JuplendMSOL
        | OracleSetup::JuplendLST => Some(IntegrationKind::JupLend),

        _ => None,
    }
}

/// Venue whose state must be refreshed before evaluating this bank.
/// Pricing/oracle integration is authoritative for freshness. Asset-tag integration is used as a
/// conservative fallback for layouts that encode the venue only in the bank tag.
fn refresh_kind(bank: &Bank) -> Option<IntegrationKind> {
    pricing_kind(bank).or_else(|| kind(bank))
}

/// [FIX-112][R6] Exact separation between Marginfi bank units and the underlying tokens
/// that the external venue sends to the liquidator.
///
/// `bank_units` is the amount removed from Marginfi's balance accounting.
/// `instruction_amount` is the amount encoded in the integration withdraw ix.
/// `underlying_out` is the conservative native token amount available to Jupiter afterwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegrationExecutionAmount {
    pub kind: IntegrationKind,
    pub bank_units: u64,
    pub instruction_amount: u64,
    pub underlying_out: u64,
}

pub fn execution_amount_for_asset<S: AccountSource + ?Sized>(
    state: &S,
    bank: &Bank,
    bank_units: u64,
) -> Result<Option<IntegrationExecutionAmount>, String> {
    if bank_units == 0 {
        return Err("INTEGRATION_WITHDRAW_ZERO".into());
    }

    let Some(kind) = kind(bank) else {
        return Ok(None);
    };

    let out = match kind {
        IntegrationKind::Kamino => {
            let reserve: &kamino_mocks::state::MinimalReserve =
                pod_after_disc(state, &bank.integration_acc_1)?;
            let underlying = reserve
                .collateral_to_liquidity(bank_units)
                .map_err(|e| format!("KAMINO_COLLATERAL_TO_LIQUIDITY:{e}"))?;
            if underlying == 0 {
                return Err("KAMINO_UNDERLYING_ZERO".into());
            }
            IntegrationExecutionAmount {
                kind,
                bank_units,
                // Marginfi KaminoWithdraw takes collateral-token units.
                instruction_amount: bank_units,
                underlying_out: underlying,
            }
        }
        IntegrationKind::Drift => {
            let market: &drift_mocks::state::MinimalSpotMarket =
                pod_after_disc(state, &bank.integration_acc_1)?;
            // Marginfi balance units are Drift scaled-balance units. DriftWithdraw takes
            // underlying native token units, then rounds the scaled decrement up.
            let mut token_amount = market
                .get_withdraw_token_amount(bank_units)
                .map_err(|e| format!("DRIFT_SCALED_TO_TOKEN:{e}"))?;
            if token_amount == 0 {
                return Err("DRIFT_UNDERLYING_ZERO".into());
            }
            let mut decrement = market
                .get_scaled_balance_decrement(token_amount)
                .map_err(|e| format!("DRIFT_TOKEN_TO_SCALED:{e}"))?;
            // Pinned Marginfi handles the one-unit round-up edge by reducing output one token.
            if decrement > bank_units {
                token_amount = token_amount.saturating_sub(1);
                if token_amount == 0 {
                    return Err("DRIFT_ROUNDING_TO_ZERO".into());
                }
                decrement = market
                    .get_scaled_balance_decrement(token_amount)
                    .map_err(|e| format!("DRIFT_TOKEN_TO_SCALED_AFTER_ADJUST:{e}"))?;
            }
            if decrement > bank_units {
                return Err(format!(
                    "DRIFT_DECREMENT_EXCEEDS_SIZED_BALANCE:{}>{}",
                    decrement, bank_units
                ));
            }
            IntegrationExecutionAmount {
                kind,
                bank_units,
                instruction_amount: token_amount,
                underlying_out: token_amount,
            }
        }
        IntegrationKind::JupLend => {
            let lending: &juplend_mocks::state::Lending =
                pod_after_disc(state, &bank.integration_acc_1)?;
            // Marginfi balance units are fToken shares. JuplendWithdraw takes underlying
            // assets and burns ceil(assets / exchange_rate) fTokens.
            let token_exchange_price = lending.token_exchange_price;
            let underlying = juplend_mocks::state::expected_assets_for_redeem_from_rate(
                bank_units,
                token_exchange_price,
            )
            .ok_or_else(|| "JUPLEND_SHARES_TO_UNDERLYING".to_string())?;
            if underlying == 0 {
                return Err("JUPLEND_UNDERLYING_ZERO".into());
            }
            let shares_to_burn = juplend_mocks::state::expected_shares_for_withdraw_from_rate(
                underlying,
                token_exchange_price,
            )
            .ok_or_else(|| "JUPLEND_UNDERLYING_TO_SHARES".to_string())?;
            if shares_to_burn == 0 || shares_to_burn > bank_units {
                return Err(format!(
                    "JUPLEND_BURN_EXCEEDS_SIZED_SHARES:{}>{}",
                    shares_to_burn, bank_units
                ));
            }
            IntegrationExecutionAmount {
                kind,
                bank_units,
                instruction_amount: underlying,
                underlying_out: underlying,
            }
        }
    };

    Ok(Some(out))
}

fn static_pk(s: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(s).map_err(|e| e.to_string())
}

/// E3/E11: refresh instruction discriminators come from the pinned IDLs (marginfi source
/// 33c67987, `idls/{kamino_lending,drift,juplend_earn}.json` through `declare_program!` in the
/// pinned mock crates) — exact source bytes, never a literal guess and never hashed per call.
fn pinned_discriminator<D: anchor_lang::Discriminator>() -> Vec<u8> {
    D::DISCRIMINATOR.to_vec()
}

fn disc8<D: anchor_lang::Discriminator>() -> [u8; 8] {
    D::DISCRIMINATOR
        .try_into()
        .expect("anchor discriminators are 8 bytes")
}

/// E2: every pinned integration program with the refresh discriminators this module builds
/// (the same IDL constants as `kamino_refreshes`/`drift_refresh`/`juplend_refresh`).
/// An integration program and its pinned refresh discriminators.
pub type PinnedRefreshes = (Pubkey, Vec<[u8; 8]>);

pub fn pinned_refresh_discriminators() -> Result<Vec<PinnedRefreshes>, String> {
    Ok(vec![
        (
            static_pk(protocol::KAMINO_PROGRAM_ID)?,
            vec![
                disc8::<kamino_mocks::kamino_lending::client::args::RefreshReserve>(),
                disc8::<kamino_mocks::kamino_lending::client::args::RefreshObligation>(),
            ],
        ),
        (
            static_pk(protocol::DRIFT_PROGRAM_ID)?,
            vec![disc8::<
                drift_mocks::drift::client::args::UpdateSpotMarketCumulativeInterest,
            >()],
        ),
        (
            static_pk(protocol::JUPLEND_PROGRAM_ID)?,
            vec![disc8::<juplend_mocks::juplend_earn::client::args::UpdateRate>()],
        ),
    ])
}

fn raw<'a, S: AccountSource + ?Sized>(
    state: &'a S,
    key: &Pubkey,
) -> Result<(Pubkey, u64, &'a [u8]), String> {
    state
        .account(key)
        .ok_or_else(|| format!("INTEGRATION_MISSING_ACCOUNT:{key}"))
}

/// External integration accounts (Kamino/Drift/JupLend mock layouts) only. E1: never for
/// Marginfi `Bank`/`MarginfiAccount`/`MarginfiGroup`/`FeeState`; those use the strict pinned
/// decoders of `protocol`.
fn pod_after_disc<'a, T: bytemuck::Pod, S: AccountSource + ?Sized>(
    state: &'a S,
    key: &Pubkey,
) -> Result<&'a T, String> {
    let (_, _, data) = raw(state, key)?;
    let need = 8 + std::mem::size_of::<T>();
    if data.len() < need {
        return Err(format!(
            "INTEGRATION_LAYOUT_SHORT:{}:{}<{}",
            key,
            data.len(),
            need
        ));
    }
    bytemuck::try_from_bytes::<T>(&data[8..need])
        .map_err(|e| format!("INTEGRATION_LAYOUT_CAST:{}:{e:?}", key))
}

fn kamino_reserve<S: AccountSource + ?Sized>(
    state: &S,
    reserve: &Pubkey,
) -> Result<kamino_mocks::kamino_lending_complete::accounts::Reserve, String> {
    let (_, _, data) = raw(state, reserve)?;
    let mut slice = data;
    kamino_mocks::kamino_lending_complete::accounts::Reserve::try_deserialize(&mut slice)
        .map_err(|e| format!("KAMINO_RESERVE_DECODE:{}:{e}", reserve))
}

fn none_if_default(pk: Pubkey) -> Option<Pubkey> {
    (pk != Pubkey::default()).then_some(pk)
}

fn kamino_oracles(
    reserve: &kamino_mocks::kamino_lending_complete::accounts::Reserve,
) -> [Option<Pubkey>; 4] {
    [
        none_if_default(reserve.config.token_info.pyth_configuration.price),
        none_if_default(
            reserve
                .config
                .token_info
                .switchboard_configuration
                .price_aggregator,
        ),
        none_if_default(
            reserve
                .config
                .token_info
                .switchboard_configuration
                .twap_aggregator,
        ),
        none_if_default(reserve.config.token_info.scope_configuration.price_feed),
    ]
}

fn kamino_refreshes<S: AccountSource + ?Sized>(
    state: &S,
    bank: &Bank,
) -> Result<Vec<Instruction>, String> {
    let reserve_key = bank.integration_acc_1;
    let obligation = bank.integration_acc_2;
    if reserve_key == Pubkey::default() || obligation == Pubkey::default() {
        return Err("KAMINO_INTEGRATION_ACCOUNT_DEFAULT".into());
    }

    let reserve = kamino_reserve(state, &reserve_key)?;
    let lending_market = reserve.lending_market;
    let kamino_program = static_pk(protocol::KAMINO_PROGRAM_ID)?;
    let [pyth, sb_price, sb_twap, scope] = kamino_oracles(&reserve);

    let mut reserve_accounts = vec![
        AccountMeta::new(reserve_key, false),
        AccountMeta::new_readonly(lending_market, false),
    ];
    for optional in [pyth, sb_price, sb_twap, scope] {
        reserve_accounts.push(AccountMeta::new_readonly(
            optional.unwrap_or(kamino_program),
            false,
        ));
    }

    Ok(vec![
        Instruction {
            program_id: kamino_program,
            accounts: reserve_accounts,
            data: pinned_discriminator::<kamino_mocks::kamino_lending::client::args::RefreshReserve>(
            ),
        },
        Instruction {
            program_id: kamino_program,
            accounts: vec![
                AccountMeta::new_readonly(lending_market, false),
                AccountMeta::new(obligation, false),
                AccountMeta::new(reserve_key, false),
            ],
            data: pinned_discriminator::<
                kamino_mocks::kamino_lending::client::args::RefreshObligation,
            >(),
        },
    ])
}

pub fn drift_reward_market_keys<S: AccountSource + ?Sized>(
    state: &S,
    bank: &Bank,
) -> Result<Vec<Pubkey>, String> {
    let market: &drift_mocks::state::MinimalSpotMarket =
        pod_after_disc(state, &bank.integration_acc_1)?;
    let user: &drift_mocks::state::MinimalUser = pod_after_disc(state, &bank.integration_acc_2)?;

    let mut indexes: Vec<u16> = user
        .spot_positions
        .iter()
        .filter(|p| {
            p.scaled_balance > 0
                && p.balance_type == drift_mocks::state::SpotBalanceType::Deposit
                && p.market_index != market.market_index
        })
        .map(|p| p.market_index)
        .collect();
    indexes.sort_unstable();
    indexes.dedup();

    if indexes.len() > 2 {
        return Err(format!("DRIFT_TOO_MANY_REWARD_MARKETS:{}", indexes.len()));
    }

    Ok(indexes
        .into_iter()
        .map(|i| derive_drift_spot_market(i).0)
        .collect())
}

fn drift_refresh<S: AccountSource + ?Sized>(
    state: &S,
    bank: &Bank,
) -> Result<Vec<Instruction>, String> {
    let market_key = bank.integration_acc_1;
    if market_key == Pubkey::default() {
        return Err("DRIFT_SPOT_MARKET_DEFAULT".into());
    }
    let market: &drift_mocks::state::MinimalSpotMarket = pod_after_disc(state, &market_key)?;
    let state_key = derive_drift_state().0;
    let vault = if market.vault != Pubkey::default() {
        market.vault
    } else {
        derive_drift_spot_market_vault(market.market_index).0
    };
    let oracle = if market.oracle == Pubkey::default() {
        static_pk(crate::receivership::SYSTEM_PROGRAM)?
    } else {
        market.oracle
    };

    Ok(vec![Instruction {
        program_id: static_pk(protocol::DRIFT_PROGRAM_ID)?,
        accounts: vec![
            AccountMeta::new_readonly(state_key, false),
            AccountMeta::new(market_key, false),
            AccountMeta::new_readonly(oracle, false),
            AccountMeta::new_readonly(vault, false),
        ],
        data: pinned_discriminator::<
            drift_mocks::drift::client::args::UpdateSpotMarketCumulativeInterest,
        >(),
    }])
}

fn juplend_refresh<S: AccountSource + ?Sized>(
    state: &S,
    bank: &Bank,
) -> Result<Vec<Instruction>, String> {
    let lending_key = bank.integration_acc_1;
    if lending_key == Pubkey::default() {
        return Err("JUPLEND_LENDING_DEFAULT".into());
    }
    let lending: &juplend_mocks::state::Lending = pod_after_disc(state, &lending_key)?;

    // Copy packed fields to aligned locals before formatting/building metas.
    let mint = lending.mint;
    let f_token_mint = lending.f_token_mint;
    let reserves = lending.token_reserves_liquidity;
    let rewards_rate_model = lending.rewards_rate_model;

    if mint != bank.mint {
        return Err(format!(
            "JUPLEND_MINT_MISMATCH:lending_mint={} bank_mint={}",
            mint, bank.mint
        ));
    }

    Ok(vec![Instruction {
        program_id: static_pk(protocol::JUPLEND_PROGRAM_ID)?,
        accounts: vec![
            AccountMeta::new(lending_key, false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new_readonly(f_token_mint, false),
            AccountMeta::new_readonly(reserves, false),
            AccountMeta::new_readonly(rewards_rate_model, false),
        ],
        data: pinned_discriminator::<juplend_mocks::juplend_earn::client::args::UpdateRate>(),
    }])
}

/// Integration kinds present on active balances of one Marginfi account.
/// Result order is stable (Kamino, Drift, JupLend) and duplicate-free.
pub fn kinds_for_account<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    account_key: &Pubkey,
) -> Result<Vec<IntegrationKind>, String> {
    let (owner, _, data) = raw(state, account_key)?;
    if owner != *program || protocol::classify(data) != protocol::AccountType::MarginfiAccount {
        return Err("INTEGRATION_KIND_NOT_MARGINFI_ACCOUNT".into());
    }
    // B2: strict pinned layout (discriminator, exact length), never a prefix decode.
    let account = protocol::decode_marginfi_account(data)
        .map_err(|e| format!("INTEGRATION_KIND_ACCOUNT_LAYOUT:{e}"))?;
    let account: &MarginfiAccount = &account;

    let mut kamino = false;
    let mut drift = false;
    let mut juplend = false;
    for bal in account
        .lending_account
        .balances
        .iter()
        .filter(|b| b.is_active())
    {
        let (bank_owner, _, bank_data) = raw(state, &bal.bank_pk)?;
        if bank_owner != *program || protocol::classify(bank_data) != protocol::AccountType::Bank {
            return Err(format!("INTEGRATION_KIND_INVALID_BANK:{}", bal.bank_pk));
        }
        let bank = protocol::decode_bank(bank_data)
            .map_err(|e| format!("INTEGRATION_BANK_LAYOUT:{}:{e}", bal.bank_pk))?;
        let bank: &Bank = &bank;
        match kind(bank) {
            Some(IntegrationKind::Kamino) => kamino = true,
            Some(IntegrationKind::Drift) => drift = true,
            Some(IntegrationKind::JupLend) => juplend = true,
            None => {}
        }
    }

    let mut out = Vec::with_capacity(3);
    if kamino {
        out.push(IntegrationKind::Kamino);
    }
    if drift {
        out.push(IntegrationKind::Drift);
    }
    if juplend {
        out.push(IntegrationKind::JupLend);
    }
    Ok(out)
}

pub fn kind_name(kind: IntegrationKind) -> &'static str {
    match kind {
        IntegrationKind::Kamino => "Kamino",
        IntegrationKind::Drift => "Drift",
        IntegrationKind::JupLend => "JupLend",
    }
}

pub fn pricing_kinds_for_account<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    account_key: &Pubkey,
) -> Result<Vec<IntegrationKind>, String> {
    let (owner, _, data) = raw(state, account_key)?;
    if owner != *program || protocol::classify(data) != protocol::AccountType::MarginfiAccount {
        return Err("PRICING_KIND_NOT_MARGINFI_ACCOUNT".into());
    }
    // B2: strict pinned layout (discriminator, exact length), never a prefix decode.
    let account = protocol::decode_marginfi_account(data)
        .map_err(|e| format!("PRICING_KIND_ACCOUNT_LAYOUT:{e}"))?;
    let account: &MarginfiAccount = &account;

    let mut kamino = false;
    let mut drift = false;
    let mut juplend = false;
    for bal in account
        .lending_account
        .balances
        .iter()
        .filter(|b| b.is_active())
    {
        let (bank_owner, _, bank_data) = raw(state, &bal.bank_pk)?;
        if bank_owner != *program || protocol::classify(bank_data) != protocol::AccountType::Bank {
            return Err(format!("PRICING_KIND_INVALID_BANK:{}", bal.bank_pk));
        }
        let bank = protocol::decode_bank(bank_data)
            .map_err(|e| format!("INTEGRATION_BANK_LAYOUT:{}:{e}", bal.bank_pk))?;
        let bank: &Bank = &bank;
        match pricing_kind(bank) {
            Some(IntegrationKind::Kamino) => kamino = true,
            Some(IntegrationKind::Drift) => drift = true,
            Some(IntegrationKind::JupLend) => juplend = true,
            None => {}
        }
    }

    let mut out = Vec::with_capacity(3);
    if kamino {
        out.push(IntegrationKind::Kamino);
    }
    if drift {
        out.push(IntegrationKind::Drift);
    }
    if juplend {
        out.push(IntegrationKind::JupLend);
    }
    Ok(out)
}

/// Build the deterministic pre-Start refresh prefix for every active integration balance.
///
/// Banks are processed in descending pubkey order, matching the canonical Marginfi observation
/// order. Duplicate external instructions are removed by (program, data, account-meta) identity.
pub fn refresh_plan_for_account<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    account_key: &Pubkey,
) -> Result<Vec<Instruction>, String> {
    let (owner, _, data) = raw(state, account_key)?;
    if owner != *program || protocol::classify(data) != protocol::AccountType::MarginfiAccount {
        return Err("INTEGRATION_REFRESH_NOT_MARGINFI_ACCOUNT".into());
    }
    // B2: strict pinned layout (discriminator, exact length), never a prefix decode.
    let account = protocol::decode_marginfi_account(data)
        .map_err(|e| format!("INTEGRATION_MARGINFI_ACCOUNT_LAYOUT:{e}"))?;
    let account: &MarginfiAccount = &account;

    let mut bank_keys: Vec<Pubkey> = account
        .lending_account
        .balances
        .iter()
        .filter(|b| b.is_active())
        .map(|b| b.bank_pk)
        .collect();
    bank_keys.sort_by(|a, b| b.cmp(a));
    bank_keys.dedup();

    let mut out = Vec::new();
    for bank_key in bank_keys {
        let (bank_owner, _, bank_data) = raw(state, &bank_key)?;
        if bank_owner != *program || protocol::classify(bank_data) != protocol::AccountType::Bank {
            return Err(format!("INTEGRATION_REFRESH_INVALID_BANK:{bank_key}"));
        }
        let bank = protocol::decode_bank(bank_data)
            .map_err(|e| format!("INTEGRATION_BANK_LAYOUT:{bank_key}:{e}"))?;
        let bank: &Bank = &bank;
        out.extend(refresh_plan_for_bank(state, bank)?);
    }

    let mut deduped: Vec<Instruction> = Vec::with_capacity(out.len());
    'outer: for ix in out {
        for existing in &deduped {
            if existing.program_id == ix.program_id
                && existing.data == ix.data
                && existing.accounts.len() == ix.accounts.len()
                && existing
                    .accounts
                    .iter()
                    .zip(ix.accounts.iter())
                    .all(|(a, b)| {
                        a.pubkey == b.pubkey
                            && a.is_signer == b.is_signer
                            && a.is_writable == b.is_writable
                    })
            {
                continue 'outer;
            }
        }
        deduped.push(ix);
    }
    Ok(deduped)
}

/// Accounts whose post-refresh bytes must be returned by simulation.
///
/// Only writable accounts can change during the refresh prefix. Every readonly Bank/oracle/input
/// remains valid through the base-state fallback of `ProjectedState`; requesting those unchanged
/// accounts again is unnecessary and can exceed provider post-account response limits.
///
/// The candidate account itself is validated against its canonical slot/hash before accepting the
/// projection result; it is not mutated by these external refresh instructions and therefore is not
/// requested as a simulated post-account.
pub fn projection_targets_for_account<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    account_key: &Pubkey,
    refreshes: &[Instruction],
) -> Result<Vec<Pubkey>, String> {
    if refreshes.is_empty() {
        return Ok(Vec::new());
    }

    // Keep strict account identity/layout validation at dispatch time.
    let (owner, _, data) = raw(state, account_key)?;
    if owner != *program || protocol::classify(data) != protocol::AccountType::MarginfiAccount {
        return Err("PROJECTION_NOT_MARGINFI_ACCOUNT".into());
    }
    // B2: strict pinned layout (discriminator, exact length), never a prefix decode.
    protocol::decode_marginfi_account(data)
        .map_err(|e| format!("PROJECTION_MARGINFI_ACCOUNT_LAYOUT:{e}"))?;

    let out = crate::projection::projection_targets(refreshes);
    if out.is_empty() {
        return Err("PROJECTION_NO_WRITABLE_REFRESH_ACCOUNTS".into());
    }
    Ok(out)
}

/// Refresh instructions required before StartLiquidation for one integration bank.
///
/// Generic banks return an empty vector. Integration failures are errors, never silently skipped.
pub fn refresh_plan_for_bank<S: AccountSource + ?Sized>(
    state: &S,
    bank: &Bank,
) -> Result<Vec<Instruction>, String> {
    match refresh_kind(bank) {
        None => Ok(Vec::new()),
        Some(IntegrationKind::Kamino) => kamino_refreshes(state, bank),
        Some(IntegrationKind::Drift) => drift_refresh(state, bank),
        Some(IntegrationKind::JupLend) => juplend_refresh(state, bank),
    }
}

/// State dependencies required for deterministic refresh planning and later integration withdraws.
///
/// This is deliberately a superset: subscribing to these accounts before planning avoids an RPC
/// lookup in the decision path. Optional reward/farm accounts that cannot be derived from canonical
/// state remain fail-closed in their specific withdraw builder.
pub fn dependency_keys_for_bank<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    bank_key: &Pubkey,
    bank: &Bank,
) -> Result<Vec<Pubkey>, String> {
    let mut out = Vec::new();

    for key in [
        bank.integration_acc_1,
        bank.integration_acc_2,
        bank.integration_acc_3,
    ] {
        if key != Pubkey::default() {
            out.push(key);
        }
    }

    match refresh_kind(bank) {
        None => {}
        Some(IntegrationKind::Kamino) => {
            let reserve = kamino_reserve(state, &bank.integration_acc_1)?;
            out.push(reserve.lending_market);
            for key in kamino_oracles(&reserve).into_iter().flatten() {
                out.push(key);
            }
            let minimal: &kamino_mocks::state::MinimalReserve =
                pod_after_disc(state, &bank.integration_acc_1)?;
            for key in [
                minimal.supply_vault,
                minimal.collateral_mint_pubkey,
                minimal.collateral_supply_vault,
                minimal.farm_collateral,
            ] {
                if key != Pubkey::default() {
                    out.push(key);
                }
            }
            out.push(
                marginfi_type_crate::pdas::derive_kamino_lending_market_authority(
                    &reserve.lending_market,
                )
                .0,
            );
            if minimal.farm_collateral != Pubkey::default() {
                out.push(
                    marginfi_type_crate::pdas::derive_kamino_user_state(
                        &minimal.farm_collateral,
                        &bank.integration_acc_2,
                    )
                    .0,
                );
            }
            out.push(marginfi_type_crate::pdas::FARMS_PROGRAM_ID);
        }
        Some(IntegrationKind::Drift) => {
            let market: &drift_mocks::state::MinimalSpotMarket =
                pod_after_disc(state, &bank.integration_acc_1)?;
            out.push(derive_drift_state().0);
            out.push(derive_drift_signer().0);
            out.push(if market.vault != Pubkey::default() {
                market.vault
            } else {
                derive_drift_spot_market_vault(market.market_index).0
            });
            if market.oracle != Pubkey::default() {
                out.push(market.oracle);
            }
            for reward_market in drift_reward_market_keys(state, bank)? {
                out.push(reward_market);
            }
        }
        Some(IntegrationKind::JupLend) => {
            let lending: &juplend_mocks::state::Lending =
                pod_after_disc(state, &bank.integration_acc_1)?;
            let mint = lending.mint;
            let f_token_mint = lending.f_token_mint;
            let reserves = lending.token_reserves_liquidity;
            let supply_position = lending.supply_position_on_liquidity;
            let rewards_rate_model = lending.rewards_rate_model;
            let token_program = raw(state, &bank.mint)?.0;
            let liquidity_vault_authority =
                protocol::liquidity_vault_authority_pda(program, bank_key);

            out.extend([
                f_token_mint,
                reserves,
                supply_position,
                rewards_rate_model,
                derive_juplend_lending_admin().0,
                derive_juplend_rate_model(&mint).0,
                derive_juplend_liquidity().0,
                derive_juplend_liquidity_vault(&mint, &token_program),
                derive_juplend_claim_account(&liquidity_vault_authority, &mint).0,
                derive_juplend_rewards_rate_model(&mint).0,
                JUPLEND_LIQUIDITY_PROGRAM_ID,
            ]);
        }
    }

    out.retain(|k| *k != Pubkey::default());
    out.sort_unstable_by_key(|k| k.to_bytes());
    out.dedup();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E3: byte-exact refresh discriminators. The expected bytes are the `discriminator` arrays
    /// of the pinned IDLs (marginfi 33c67987 `idls/`, also `idls-complete/`); the builders take
    /// them from the IDL-generated mock crates, and all four equal the Anchor rule
    /// sha256("global:<name>")[..8], so a single changed byte fails here.
    #[test]
    fn pinned_refresh_discriminators_match_expected() {
        use kamino_mocks::kamino_lending::client::args as kamino;
        let anchor = |name: &str| {
            solana_sdk::hash::hash(format!("global:{name}").as_bytes()).to_bytes()[..8].to_vec()
        };
        for (built, idl, name) in [
            (
                pinned_discriminator::<kamino::RefreshReserve>(),
                [2, 218, 138, 235, 79, 201, 25, 102],
                "refresh_reserve",
            ),
            (
                pinned_discriminator::<kamino::RefreshObligation>(),
                [33, 132, 147, 228, 151, 192, 72, 89],
                "refresh_obligation",
            ),
            (
                pinned_discriminator::<
                    drift_mocks::drift::client::args::UpdateSpotMarketCumulativeInterest,
                >(),
                [39, 166, 139, 243, 158, 165, 155, 225],
                "update_spot_market_cumulative_interest",
            ),
            (
                pinned_discriminator::<juplend_mocks::juplend_earn::client::args::UpdateRate>(),
                [24, 225, 53, 189, 72, 212, 225, 178],
                "update_rate",
            ),
        ] {
            assert_eq!(built, idl.to_vec(), "{name}");
            assert_eq!(anchor(name), idl.to_vec(), "{name}");
            let mut mutated = idl;
            mutated[7] ^= 1;
            assert_ne!(built, mutated.to_vec(), "{name}");
        }
    }

    /// E3: the live refresh builders emit exactly the pinned IDL instruction: program, data and
    /// the account order/writable flags of `update_spot_market_cumulative_interest`
    /// (state, spot_market W, oracle, spot_market_vault) and `update_rate` (lending W, mint,
    /// f_token_mint, supply_token_reserves_liquidity, rewards_rate_model).
    #[test]
    fn refresh_builders_emit_the_pinned_idl_instructions() {
        use bytemuck::Zeroable;
        use std::collections::HashMap;
        struct Source(HashMap<Pubkey, Vec<u8>>);
        impl AccountSource for Source {
            fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                self.0
                    .get(key)
                    .map(|d| (Pubkey::default(), 1, d.as_slice()))
            }
        }
        fn raw<T: bytemuck::Pod>(disc: [u8; 8], v: &T) -> Vec<u8> {
            [disc.as_slice(), bytemuck::bytes_of(v)].concat()
        }
        let (market_key, lending_key) = (Pubkey::new_unique(), Pubkey::new_unique());
        let state = Source(HashMap::from([
            (
                market_key,
                raw(
                    drift_mocks::state::SPOT_MARKET_DISCRIMINATOR,
                    &drift_mocks::state::MinimalSpotMarket::zeroed(),
                ),
            ),
            (
                lending_key,
                raw(
                    juplend_mocks::state::LENDING_DISCRIMINATOR,
                    &juplend_mocks::state::Lending::zeroed(),
                ),
            ),
        ]));
        let flags = |ix: &Instruction| -> Vec<(bool, bool)> {
            ix.accounts
                .iter()
                .map(|m| (m.is_writable, m.is_signer))
                .collect()
        };
        let mut bank = Bank::zeroed();
        bank.integration_acc_1 = market_key;
        let drift = drift_refresh(&state, &bank).unwrap();
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].program_id.to_string(), protocol::DRIFT_PROGRAM_ID);
        assert_eq!(drift[0].data, vec![39, 166, 139, 243, 158, 165, 155, 225]);
        assert_eq!(
            flags(&drift[0]),
            vec![
                (false, false),
                (true, false),
                (false, false),
                (false, false)
            ]
        );
        assert_eq!(drift[0].accounts[1].pubkey, market_key);
        bank.integration_acc_1 = lending_key;
        let juplend = juplend_refresh(&state, &bank).unwrap();
        assert_eq!(juplend.len(), 1);
        assert_eq!(
            juplend[0].program_id.to_string(),
            protocol::JUPLEND_PROGRAM_ID
        );
        assert_eq!(juplend[0].data, vec![24, 225, 53, 189, 72, 212, 225, 178]);
        assert_eq!(
            flags(&juplend[0]),
            vec![
                (true, false),
                (false, false),
                (false, false),
                (false, false),
                (false, false)
            ]
        );
        assert_eq!(juplend[0].accounts[0].pubkey, lending_key);
    }

    /// B2/E1: every Marginfi decoder on the integration, health and sizing paths accepts only the
    /// exact pinned layout: a resized (0.1.12) bank, a longer or shorter account, group or fee
    /// state and a foreign discriminator are UNKNOWN_LAYOUT, never a prefix decode.
    #[test]
    fn marginfi_layouts_are_exact_on_every_decoder_path() {
        use crate::health::{crate_impl::CrateHealth, HealthEngine, HealthError};
        use bytemuck::Zeroable;
        use marginfi_type_crate::types::{FeeState, MarginfiGroup};
        use std::collections::HashMap;
        struct Source(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
        impl AccountSource for Source {
            fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                self.0.get(key).map(|(o, d)| (*o, 1, d.as_slice()))
            }
        }
        // Raw account bytes: discriminator, pinned payload, then `extra` bytes added or removed.
        fn raw<T: bytemuck::Pod>(disc: [u8; 8], v: &T, extra: isize) -> Vec<u8> {
            let mut d = [disc.as_slice(), bytemuck::bytes_of(v)].concat();
            let n = (d.len() as isize + extra) as usize;
            d.resize(n, 0);
            d
        }
        let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
        let (group, bank_key, account_key) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let mut bank = Bank::zeroed();
        bank.group = group;
        let mut account = MarginfiAccount::zeroed();
        account.group = group;
        account.lending_account.balances[0].active = 1;
        account.lending_account.balances[0].bank_pk = bank_key;
        let state = |account_extra: isize, bank_bytes: Vec<u8>, group_extra: isize, fee_extra| {
            let mut m = HashMap::new();
            m.insert(
                account_key,
                (
                    program,
                    raw(protocol::DISC_MARGINFI_ACCOUNT, &account, account_extra),
                ),
            );
            m.insert(bank_key, (program, bank_bytes));
            m.insert(
                group,
                (
                    program,
                    raw(
                        protocol::DISC_MARGINFI_GROUP,
                        &MarginfiGroup::zeroed(),
                        group_extra,
                    ),
                ),
            );
            m.insert(
                protocol::fee_state_pda(&program),
                (
                    program,
                    raw(protocol::DISC_FEE_STATE, &FeeState::zeroed(), fee_extra),
                ),
            );
            Source(m)
        };
        let exact_bank = raw(protocol::DISC_BANK, &bank, 0);
        let resized_bank = raw(
            protocol::DISC_BANK,
            &bank,
            (protocol::BANK_PAYLOAD_LEN_0_1_12 - protocol::BANK_PAYLOAD_LEN_0_1_11) as isize,
        );
        let paths = |s: &Source| -> [Result<(), String>; 3] {
            [
                kinds_for_account(s, &program, &account_key).map(drop),
                pricing_kinds_for_account(s, &program, &account_key).map(drop),
                refresh_plan_for_account(s, &program, &account_key).map(drop),
            ]
        };
        // The exact pinned layouts decode on every path.
        for r in paths(&state(0, exact_bank.clone(), 0, 0)) {
            assert_eq!(r, Ok(()));
        }
        // Longer or shorter accounts and resized or longer banks are refused on every path.
        for (account_extra, bank_bytes) in [
            (1, exact_bank.clone()),
            (8, exact_bank.clone()),
            (-2, exact_bank.clone()),
            (0, resized_bank.clone()),
            (0, raw(protocol::DISC_BANK, &bank, 1)),
        ] {
            for r in paths(&state(account_extra, bank_bytes.clone(), 0, 0)) {
                let e = r.unwrap_err();
                assert!(e.contains("UNKNOWN_LAYOUT"), "{account_extra}: {e}");
            }
        }
        let e = kinds_for_account(
            &state(0, resized_bank.clone(), 0, 0),
            &program,
            &account_key,
        )
        .unwrap_err();
        assert!(
            e.starts_with("INTEGRATION_BANK_LAYOUT") && e.contains("0.1.12"),
            "{e}"
        );
        // A foreign discriminator is refused before any layout read.
        let mut foreign = state(0, exact_bank.clone(), 0, 0);
        foreign.0.get_mut(&account_key).unwrap().1[0] ^= 1;
        assert!(kinds_for_account(&foreign, &program, &account_key).is_err());
        assert!(crate::protocol::decode_marginfi_account(&foreign.0[&account_key].1).is_err());

        // Health: account and group follow the same exact contract (UNKNOWN_LAYOUT = Unsupported).
        let health = |s: &Source| CrateHealth { state: s, group }.health(&account_key, 1, 1);
        assert!(!matches!(
            health(&state(0, exact_bank.clone(), 0, 0)),
            Err(HealthError::Unsupported(_))
        ));
        for s in [
            state(8, exact_bank.clone(), 0, 0),
            state(0, exact_bank.clone(), 8, 0),
            state(0, resized_bank.clone(), 0, 0),
        ] {
            assert!(
                matches!(health(&s), Err(HealthError::Unsupported(ref e)) if e.contains("UNKNOWN_LAYOUT")),
                "{:?}",
                health(&s)
            );
        }
        // Sizing: the protocol FeeState is exact as well.
        let sizer = |s: &Source| {
            crate::sizing::ProtocolSizer {
                state: s,
                program,
                group,
                headroom_bps: 10_000,
            }
            .fee_snapshot()
            .map(drop)
        };
        assert_eq!(sizer(&state(0, exact_bank.clone(), 0, 0)), Ok(()));
        for fee_extra in [8, -1] {
            let e = sizer(&state(0, exact_bank.clone(), 0, fee_extra)).unwrap_err();
            assert!(e.contains("UNKNOWN_LAYOUT"), "{e}");
        }
    }
}
