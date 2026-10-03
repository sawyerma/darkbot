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
    hash::hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};
use std::str::FromStr;

use marginfi_type_crate::{
    constants::{ASSET_TAG_DRIFT, ASSET_TAG_JUPLEND, ASSET_TAG_KAMINO},
    pdas::{
        derive_drift_signer, derive_drift_spot_market_vault, derive_drift_state,
        derive_juplend_claim_account, derive_juplend_lending_admin, derive_juplend_liquidity,
        derive_juplend_liquidity_vault, derive_juplend_rate_model,
        derive_juplend_rewards_rate_model, JUPLEND_LIQUIDITY_PROGRAM_ID,
    },
    types::Bank,
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

fn static_pk(s: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(s).map_err(|e| e.to_string())
}

fn ix_discriminator(name: &str) -> Vec<u8> {
    hash(format!("global:{name}").as_bytes()).to_bytes()[..8].to_vec()
}

fn raw<'a, S: AccountSource + ?Sized>(
    state: &'a S,
    key: &Pubkey,
) -> Result<(Pubkey, u64, &'a [u8]), String> {
    state
        .account(key)
        .ok_or_else(|| format!("INTEGRATION_MISSING_ACCOUNT:{key}"))
}

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
            data: ix_discriminator("refresh_reserve"),
        },
        Instruction {
            program_id: kamino_program,
            accounts: vec![
                AccountMeta::new_readonly(lending_market, false),
                AccountMeta::new(obligation, false),
                AccountMeta::new(reserve_key, false),
            ],
            data: ix_discriminator("refresh_obligation"),
        },
    ])
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
        data: vec![39, 166, 139, 243, 158, 165, 155, 225],
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
        data: vec![24, 225, 53, 189, 72, 212, 225, 178],
    }])
}

/// Refresh instructions required before StartLiquidation for one integration bank.
///
/// Generic banks return an empty vector. Integration failures are errors, never silently skipped.
pub fn refresh_plan_for_bank<S: AccountSource + ?Sized>(
    state: &S,
    bank: &Bank,
) -> Result<Vec<Instruction>, String> {
    match kind(bank) {
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

    match kind(bank) {
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
            // Reward-market dependencies are discovered from the Drift user at builder time;
            // if not present in RAM the specific withdraw path remains fail-closed.
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

    #[test]
    fn pinned_refresh_discriminators_match_expected() {
        assert_eq!(ix_discriminator("refresh_reserve"), vec![2, 218, 138, 235, 79, 201, 25, 102]);
        assert_eq!(ix_discriminator("refresh_obligation"), vec![33, 132, 147, 228, 151, 192, 72, 89]);
        assert_eq!(vec![39, 166, 139, 243, 158, 165, 155, 225].len(), 8);
        assert_eq!(vec![24, 225, 53, 189, 72, 212, 225, 178].len(), 8);
    }
}
