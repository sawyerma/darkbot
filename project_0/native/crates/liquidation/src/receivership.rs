//! P5-06: Receivership-Instructions über die **offiziellen Crate-Builder** (Apache-2.0):
//! `marginfi::accounts::*{..}.to_account_metas(Some(true))` + `marginfi::instruction::*.data()`
//! — exakt wie `p0-cli/src/processor/account.rs` (BELEGT, Sitzung 5). Kein manueller
//! Discriminator-/Account-Meta-Nachbau mehr (DROP gemäß Merge-Entscheidung 3).
//!
//! Sequenz (Introspection erzwingt): [refreshes] [init_liq_record] start → withdraw… → (swap) → repay… → end.
//! Init darf **inline vor Start** liegen (p0-cli macht es so) — schneller als Evas separate Init-TX.
//! Kein System-Programm in der TX → Jito-Tip in separater Bundle-TX.
//!
//! Ohne Feature `crate_health`: keine Builder (fail closed). Status: UNTESTED / VERIFY (Staging-Simulation).

use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;

pub const INSTRUCTIONS_SYSVAR: &str = "Sysvar1nstructions1111111111111111111111111";
pub const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

/// Parameter einer vollständigen Receivership-TX.
pub struct ReceivershipParams<'a> {
    pub program: &'a Pubkey,
    pub group: &'a Pubkey,
    pub marginfi_account: &'a Pubkey,
    pub liquidator: &'a Pubkey,
    /// Aus FeeState (P1A-Manifest / RAM-State), nicht hart kodiert.
    pub global_fee_wallet: &'a Pubkey,
    /// StartLiquidation: complete bank+oracle layout; active banks writable because Start locks
    /// liquidation prices in each bank cache. Matches pinned p0-cli start_metas.
    pub start_observation: Vec<AccountMeta>,
    /// EndLiquidation uses HealthPriceMode::Cached and therefore takes banks only, all writable
    /// because End clears each bank's liquidation-price-cache lock.
    pub end_banks: Vec<Pubkey>,
    pub init_record: bool,
    pub pre_refreshes: Vec<Instruction>,
    pub withdraws: Vec<Instruction>,
    pub swaps: Vec<Instruction>,
    pub repays: Vec<Instruction>,
}

#[cfg(feature = "crate_health")]
pub mod builders {
    use super::*;
    use crate::protocol;
    use anchor_lang::{InstructionData, ToAccountMetas};
    use std::str::FromStr;

    fn pk(s: &str) -> Pubkey {
        Pubkey::from_str(s).expect("static")
    }

    fn obs(observation: &[Pubkey]) -> Vec<AccountMeta> {
        observation
            .iter()
            .map(|k| AccountMeta::new_readonly(*k, false))
            .collect()
    }

    pub fn init_liquidation_record(
        program: &Pubkey,
        marginfi_account: &Pubkey,
        fee_payer: &Pubkey,
    ) -> Instruction {
        Instruction {
            program_id: *program,
            accounts: marginfi::accounts::InitLiquidationRecord {
                marginfi_account: *marginfi_account,
                fee_payer: *fee_payer,
                liquidation_record: protocol::liquidation_record_pda(program, marginfi_account),
                system_program: pk(SYSTEM_PROGRAM),
            }
            .to_account_metas(Some(true)),
            data: marginfi::instruction::MarginfiAccountInitLiqRecord.data(),
        }
    }

    pub fn start_liquidation(
        program: &Pubkey,
        marginfi_account: &Pubkey,
        group: &Pubkey,
        receiver: &Pubkey,
        observation: &[AccountMeta],
    ) -> Instruction {
        let mut accounts = marginfi::accounts::StartLiquidation {
            marginfi_account: *marginfi_account,
            liquidation_record: protocol::liquidation_record_pda(program, marginfi_account),
            group: *group,
            liquidation_receiver: *receiver,
            instruction_sysvar: pk(INSTRUCTIONS_SYSVAR),
        }
        .to_account_metas(Some(true));
        accounts.extend_from_slice(observation);
        Instruction {
            program_id: *program,
            accounts,
            data: marginfi::instruction::StartLiquidation.data(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn withdraw(
        program: &Pubkey,
        group: &Pubkey,
        marginfi_account: &Pubkey,
        authority: &Pubkey,
        bank: &Pubkey,
        mint: &Pubkey,
        destination_token_account: &Pubkey,
        liquidity_vault: &Pubkey,
        token_program: &Pubkey,
        amount: u64,
        withdraw_all: Option<bool>,
        observation: &[Pubkey],
    ) -> Instruction {
        let mut accounts = marginfi::accounts::LendingAccountWithdraw {
            group: *group,
            marginfi_account: *marginfi_account,
            authority: *authority,
            bank: *bank,
            destination_token_account: *destination_token_account,
            bank_liquidity_vault_authority: protocol::liquidity_vault_authority_pda(program, bank),
            liquidity_vault: *liquidity_vault,
            token_program: *token_program,
        }
        .to_account_metas(Some(true));

        // [FIX-128f] Pinned p0-cli/test-utils parity: Token-2022 withdraw requires the
        // bank mint as the first remaining account, before the health observation accounts.
        // Classic SPL Token withdraw does not carry this mint account.
        if *token_program == pk(protocol::TOKEN_2022_PROGRAM_ID) {
            accounts.push(AccountMeta::new_readonly(*mint, false));
        }
        accounts.extend(obs(observation));

        Instruction {
            program_id: *program,
            accounts,
            data: marginfi::instruction::LendingAccountWithdraw {
                amount,
                withdraw_all,
            }
            .data(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn repay(
        program: &Pubkey,
        group: &Pubkey,
        marginfi_account: &Pubkey,
        authority: &Pubkey,
        bank: &Pubkey,
        mint: &Pubkey,
        signer_token_account: &Pubkey,
        liquidity_vault: &Pubkey,
        token_program: &Pubkey,
        amount: u64,
        repay_all: Option<bool>,
        observation: &[Pubkey],
    ) -> Instruction {
        let mut accounts = marginfi::accounts::LendingAccountRepay {
            group: *group,
            marginfi_account: *marginfi_account,
            authority: *authority,
            bank: *bank,
            signer_token_account: *signer_token_account,
            liquidity_vault: *liquidity_vault,
            token_program: *token_program,
        }
        .to_account_metas(Some(true));

        // Pinned p0-cli/test-utils parity:
        // - Token-2022 repay requires the mint as the first remaining account so transfer-fee
        //   semantics can calculate the exact pre-fee SPL deposit.
        // - repay_all performs a health observation because it closes the balance. During
        //   receivership the closing bank remains observable for this instruction even though
        //   it must be omitted from EndLiquidation after the balance is closed.
        if *token_program == pk(protocol::TOKEN_2022_PROGRAM_ID) {
            accounts.push(AccountMeta::new_readonly(*mint, false));
        }
        if repay_all.unwrap_or(false) {
            accounts.extend(obs(observation));
        }

        Instruction {
            program_id: *program,
            accounts,
            data: marginfi::instruction::LendingAccountRepay { amount, repay_all }.data(),
        }
    }

    // [FIX-110][R6] Integration-specific withdraw builders, reproduced from the
    // pinned marginfi p0-cli account layouts. No RPC or account discovery happens here.

    #[allow(clippy::too_many_arguments)]
    pub fn kamino_withdraw_from_bank(
        program: &Pubkey,
        group: &Pubkey,
        marginfi_account: &Pubkey,
        authority: &Pubkey,
        bank_key: &Pubkey,
        bank: &marginfi_type_crate::types::Bank,
        destination_token_account: &Pubkey,
        token_program: &Pubkey,
        lending_market: &Pubkey,
        reserve_liquidity_supply: &Pubkey,
        reserve_collateral_mint: &Pubkey,
        reserve_source_collateral: &Pubkey,
        obligation_farm_user_state: Option<Pubkey>,
        reserve_farm_state: Option<Pubkey>,
        amount: u64,
        withdraw_all: bool,
        observation: &[Pubkey],
    ) -> Instruction {
        let mut accounts = marginfi::accounts::KaminoWithdraw {
            group: *group,
            marginfi_account: *marginfi_account,
            authority: *authority,
            bank: *bank_key,
            destination_token_account: *destination_token_account,
            liquidity_vault_authority: protocol::liquidity_vault_authority_pda(program, bank_key),
            liquidity_vault: bank.liquidity_vault,
            integration_acc_2: bank.integration_acc_2,
            lending_market: *lending_market,
            lending_market_authority:
                marginfi_type_crate::pdas::derive_kamino_lending_market_authority(lending_market).0,
            integration_acc_1: bank.integration_acc_1,
            mint: bank.mint,
            reserve_liquidity_supply: *reserve_liquidity_supply,
            reserve_collateral_mint: *reserve_collateral_mint,
            reserve_source_collateral: *reserve_source_collateral,
            obligation_farm_user_state,
            reserve_farm_state,
            kamino_program: pk(protocol::KAMINO_PROGRAM_ID),
            farms_program: marginfi_type_crate::pdas::FARMS_PROGRAM_ID,
            collateral_token_program: pk(protocol::TOKEN_PROGRAM_ID),
            liquidity_token_program: *token_program,
            instruction_sysvar_account: pk(INSTRUCTIONS_SYSVAR),
        }
        .to_account_metas(Some(true));
        accounts.extend(obs(observation));
        Instruction {
            program_id: *program,
            accounts,
            data: marginfi::instruction::KaminoWithdraw {
                amount,
                flags: withdraw_all.then_some(1u8),
            }
            .data(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn drift_withdraw_from_bank(
        program: &Pubkey,
        group: &Pubkey,
        marginfi_account: &Pubkey,
        authority: &Pubkey,
        bank_key: &Pubkey,
        bank: &marginfi_type_crate::types::Bank,
        destination_token_account: &Pubkey,
        token_program: &Pubkey,
        drift_oracle: Option<Pubkey>,
        drift_state: Pubkey,
        drift_spot_market_vault: Pubkey,
        drift_reward_oracle: Option<Pubkey>,
        drift_reward_spot_market: Option<Pubkey>,
        drift_reward_mint: Option<Pubkey>,
        drift_reward_oracle_2: Option<Pubkey>,
        drift_reward_spot_market_2: Option<Pubkey>,
        drift_reward_mint_2: Option<Pubkey>,
        drift_signer: Pubkey,
        amount: u64,
        withdraw_all: bool,
        observation: &[Pubkey],
    ) -> Instruction {
        let mut accounts = marginfi::accounts::DriftWithdraw {
            group: *group,
            marginfi_account: *marginfi_account,
            authority: *authority,
            bank: *bank_key,
            drift_oracle,
            liquidity_vault_authority: protocol::liquidity_vault_authority_pda(program, bank_key),
            liquidity_vault: bank.liquidity_vault,
            destination_token_account: *destination_token_account,
            drift_state,
            integration_acc_2: bank.integration_acc_2,
            integration_acc_3: bank.integration_acc_3,
            integration_acc_1: bank.integration_acc_1,
            drift_spot_market_vault,
            drift_reward_oracle,
            drift_reward_spot_market,
            drift_reward_mint,
            drift_reward_oracle_2,
            drift_reward_spot_market_2,
            drift_reward_mint_2,
            drift_signer,
            mint: bank.mint,
            drift_program: pk(protocol::DRIFT_PROGRAM_ID),
            token_program: *token_program,
            system_program: pk(SYSTEM_PROGRAM),
        }
        .to_account_metas(Some(true));
        accounts.extend(obs(observation));
        Instruction {
            program_id: *program,
            accounts,
            data: marginfi::instruction::DriftWithdraw {
                amount,
                withdraw_all: withdraw_all.then_some(true),
            }
            .data(),
        }
    }

    pub struct JuplendCpiAccounts {
        pub f_token_mint: Pubkey,
        pub lending_admin: Pubkey,
        pub supply_token_reserves_liquidity: Pubkey,
        pub lending_supply_position_on_liquidity: Pubkey,
        pub rate_model: Pubkey,
        pub vault: Pubkey,
        pub claim_account: Pubkey,
        pub liquidity: Pubkey,
        pub liquidity_program: Pubkey,
        pub rewards_rate_model: Pubkey,
    }

    #[allow(clippy::too_many_arguments)]
    pub fn juplend_withdraw_from_bank(
        program: &Pubkey,
        group: &Pubkey,
        marginfi_account: &Pubkey,
        authority: &Pubkey,
        bank_key: &Pubkey,
        bank: &marginfi_type_crate::types::Bank,
        destination_token_account: &Pubkey,
        token_program: &Pubkey,
        cpi: &JuplendCpiAccounts,
        amount: u64,
        withdraw_all: bool,
        observation: &[Pubkey],
    ) -> Instruction {
        let mut accounts = marginfi::accounts::JuplendWithdraw {
            group: *group,
            marginfi_account: *marginfi_account,
            authority: *authority,
            bank: *bank_key,
            destination_token_account: *destination_token_account,
            liquidity_vault_authority: protocol::liquidity_vault_authority_pda(program, bank_key),
            mint: bank.mint,
            integration_acc_1: bank.integration_acc_1,
            f_token_mint: cpi.f_token_mint,
            integration_acc_2: bank.integration_acc_2,
            integration_acc_3: bank.integration_acc_3,
            lending_admin: cpi.lending_admin,
            supply_token_reserves_liquidity: cpi.supply_token_reserves_liquidity,
            lending_supply_position_on_liquidity: cpi.lending_supply_position_on_liquidity,
            rate_model: cpi.rate_model,
            vault: cpi.vault,
            claim_account: cpi.claim_account,
            liquidity: cpi.liquidity,
            liquidity_program: cpi.liquidity_program,
            rewards_rate_model: cpi.rewards_rate_model,
            juplend_program: pk(protocol::JUPLEND_PROGRAM_ID),
            token_program: *token_program,
            associated_token_program: pk(protocol::ASSOCIATED_TOKEN_PROGRAM_ID),
            system_program: pk(SYSTEM_PROGRAM),
        }
        .to_account_metas(Some(true));
        accounts.extend(obs(observation));
        Instruction {
            program_id: *program,
            accounts,
            data: marginfi::instruction::JuplendWithdraw {
                amount,
                withdraw_all: withdraw_all.then_some(true),
            }
            .data(),
        }
    }

    pub fn end_liquidation(
        program: &Pubkey,
        marginfi_account: &Pubkey,
        group: &Pubkey,
        receiver: &Pubkey,
        global_fee_wallet: &Pubkey,
        banks_only: &[Pubkey],
    ) -> Instruction {
        let mut accounts = marginfi::accounts::EndLiquidation {
            marginfi_account: *marginfi_account,
            liquidation_record: protocol::liquidation_record_pda(program, marginfi_account),
            group: *group,
            liquidation_receiver: *receiver,
            fee_state: protocol::fee_state_pda(program),
            global_fee_wallet: *global_fee_wallet,
            system_program: pk(SYSTEM_PROGRAM),
            fee_payer: None,
        }
        .to_account_metas(Some(true));
        accounts.extend(banks_only.iter().map(|k| AccountMeta::new(*k, false)));
        Instruction {
            program_id: *program,
            accounts,
            data: marginfi::instruction::EndLiquidation.data(),
        }
    }

    pub fn build_receivership(p: ReceivershipParams<'_>) -> Vec<Instruction> {
        let mut v = Vec::with_capacity(
            4 + p.pre_refreshes.len() + p.withdraws.len() + p.swaps.len() + p.repays.len(),
        );
        v.extend(p.pre_refreshes);
        if p.init_record {
            v.push(init_liquidation_record(
                p.program,
                p.marginfi_account,
                p.liquidator,
            ));
        }
        v.push(start_liquidation(
            p.program,
            p.marginfi_account,
            p.group,
            p.liquidator,
            &p.start_observation,
        ));
        v.extend(p.withdraws);
        v.extend(p.swaps);
        v.extend(p.repays);
        v.push(end_liquidation(
            p.program,
            p.marginfi_account,
            p.group,
            p.liquidator,
            p.global_fee_wallet,
            &p.end_banks,
        ));
        v
    }

    #[cfg(test)]
    mod repay_all_tests {
        use super::*;
        fn p() -> Pubkey {
            Pubkey::new_unique()
        }

        #[test]
        fn token_2022_withdraw_includes_mint_before_observation() {
            let program = p();
            let mint = p();
            let observation = p();
            let token_program = pk(protocol::TOKEN_2022_PROGRAM_ID);
            let ix = withdraw(
                &program,
                &p(),
                &p(),
                &p(),
                &p(),
                &mint,
                &p(),
                &p(),
                &token_program,
                13,
                None,
                &[observation],
            );
            let mint_index = ix.accounts.iter().position(|m| m.pubkey == mint).unwrap();
            let obs_index = ix
                .accounts
                .iter()
                .position(|m| m.pubkey == observation)
                .unwrap();
            assert!(mint_index < obs_index);
            assert!(!ix.accounts[mint_index].is_writable);
            assert!(!ix.accounts[mint_index].is_signer);
        }

        #[test]
        fn classic_withdraw_does_not_append_mint() {
            let program = p();
            let mint = p();
            let observation = p();
            let token_program = pk(protocol::TOKEN_PROGRAM_ID);
            let ix = withdraw(
                &program,
                &p(),
                &p(),
                &p(),
                &p(),
                &mint,
                &p(),
                &p(),
                &token_program,
                13,
                None,
                &[observation],
            );
            assert!(!ix.accounts.iter().any(|m| m.pubkey == mint));
            assert!(ix.accounts.iter().any(|m| m.pubkey == observation));
        }

        #[test]
        fn repay_all_carries_observation_accounts() {
            let program = p();
            let group = p();
            let account = p();
            let authority = p();
            let bank = p();
            let mint = p();
            let token_account = p();
            let vault = p();
            let token_program = pk(protocol::TOKEN_PROGRAM_ID);
            let obs_keys = vec![p(), p(), p()];

            let ix = repay(
                &program,
                &group,
                &account,
                &authority,
                &bank,
                &mint,
                &token_account,
                &vault,
                &token_program,
                11,
                Some(true),
                &obs_keys,
            );

            assert_eq!(ix.program_id, program);
            assert!(obs_keys.iter().all(|key| ix
                .accounts
                .iter()
                .any(|m| m.pubkey == *key && !m.is_writable && !m.is_signer)));
            assert!(
                ix.data.len() >= 8,
                "anchor instruction discriminator missing"
            );
        }

        #[test]
        fn partial_repay_does_not_append_health_observation() {
            let program = p();
            let token_program = pk(protocol::TOKEN_PROGRAM_ID);
            let obs_key = p();
            let ix = repay(
                &program,
                &p(),
                &p(),
                &p(),
                &p(),
                &p(),
                &p(),
                &p(),
                &token_program,
                7,
                None,
                &[obs_key],
            );
            assert!(!ix.accounts.iter().any(|m| m.pubkey == obs_key));
        }

        #[test]
        fn token_2022_repay_includes_mint_before_repay_all_observation() {
            let program = p();
            let mint = p();
            let observation = p();
            let token_program = pk(protocol::TOKEN_2022_PROGRAM_ID);
            let ix = repay(
                &program,
                &p(),
                &p(),
                &p(),
                &p(),
                &mint,
                &p(),
                &p(),
                &token_program,
                9,
                Some(true),
                &[observation],
            );
            let mint_index = ix.accounts.iter().position(|m| m.pubkey == mint).unwrap();
            let obs_index = ix
                .accounts
                .iter()
                .position(|m| m.pubkey == observation)
                .unwrap();
            assert!(mint_index < obs_index);
            assert!(!ix.accounts[mint_index].is_writable);
        }
    }
}
