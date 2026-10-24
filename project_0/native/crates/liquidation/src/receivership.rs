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

    /// B14: a static program pin, parsed at compile time (an invalid pin does not build).
    macro_rules! pk {
        ($s:expr) => {{
            const KEY: Pubkey = Pubkey::from_str_const($s);
            KEY
        }};
    }

    fn disc8<D: anchor_lang::Discriminator>() -> [u8; 8] {
        D::DISCRIMINATOR
            .try_into()
            .expect("anchor discriminators are 8 bytes")
    }

    /// E2: the instruction table of the receivership TX these builders produce: the pinned
    /// E2: the amounts a built receivership transaction carries, decoded from its signed bytes:
    /// `(amount, all)` of its one withdraw (plain or Kamino/Drift/JupLend; Kamino's `flags = 1`
    /// is withdraw-all) and of its one repay.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct BuiltAmounts {
        pub withdraw: (u64, bool),
        pub repay: (u64, bool),
    }

    /// E2: decodes the receivership program's withdraw and repay instructions of `ixs` (anchor
    /// argument encoding of the pinned crate); anything but exactly one of each, an undecodable
    /// argument list or trailing bytes is an error.
    pub fn built_amounts(ixs: &[Instruction], program: &Pubkey) -> Result<BuiltAmounts, String> {
        use anchor_lang::AnchorDeserialize;
        use marginfi::instruction as mi;
        fn args<T: AnchorDeserialize>(data: &[u8]) -> Result<T, String> {
            T::try_from_slice(&data[8..]).map_err(|_| "E2_AMOUNTS_UNDECODABLE".to_string())
        }
        let (mut withdraws, mut repays) = (Vec::new(), Vec::new());
        for ix in ixs.iter().filter(|ix| ix.program_id == *program) {
            let Some(d) = ix.data.get(..8).and_then(|d| <[u8; 8]>::try_from(d).ok()) else {
                continue;
            };
            if d == disc8::<mi::LendingAccountWithdraw>() {
                let a: mi::LendingAccountWithdraw = args(&ix.data)?;
                withdraws.push((a.amount, a.withdraw_all == Some(true)));
            } else if d == disc8::<mi::KaminoWithdraw>() {
                let a: mi::KaminoWithdraw = args(&ix.data)?;
                withdraws.push((a.amount, a.flags == Some(1)));
            } else if d == disc8::<mi::DriftWithdraw>() {
                let a: mi::DriftWithdraw = args(&ix.data)?;
                withdraws.push((a.amount, a.withdraw_all == Some(true)));
            } else if d == disc8::<mi::JuplendWithdraw>() {
                let a: mi::JuplendWithdraw = args(&ix.data)?;
                withdraws.push((a.amount, a.withdraw_all == Some(true)));
            } else if d == disc8::<mi::LendingAccountRepay>() {
                let a: mi::LendingAccountRepay = args(&ix.data)?;
                repays.push((a.amount, a.repay_all == Some(true)));
            }
        }
        match (withdraws.as_slice(), repays.as_slice()) {
            ([withdraw], [repay]) => Ok(BuiltAmounts {
                withdraw: *withdraw,
                repay: *repay,
            }),
            _ => Err(format!(
                "E2_AMOUNTS_SHAPE: {} withdraw(s), {} repay(s)",
                withdraws.len(),
                repays.len()
            )),
        }
    }

    /// marginfi instruction discriminators (crate IDL) of exactly the instructions built here,
    /// the pinned integration refreshes and the swap programs of the receivership allowlist.
    pub fn instruction_table(program: &Pubkey) -> Result<crate::planner::InstructionTable, String> {
        use marginfi::instruction as mi;
        Ok(crate::planner::InstructionTable {
            program: *program,
            start: disc8::<mi::StartLiquidation>(),
            end: disc8::<mi::EndLiquidation>(),
            withdraw: vec![
                disc8::<mi::LendingAccountWithdraw>(),
                disc8::<mi::KaminoWithdraw>(),
                disc8::<mi::DriftWithdraw>(),
                disc8::<mi::JuplendWithdraw>(),
            ],
            repay: disc8::<mi::LendingAccountRepay>(),
            init_record: disc8::<mi::MarginfiAccountInitLiqRecord>(),
            refreshes: crate::integration::pinned_refresh_discriminators()?
                .into_iter()
                .collect(),
            swap_programs: [
                pk!(protocol::JUP_SWAP_PROGRAM_ID),
                pk!(protocol::TITAN_SWAP_PROGRAM_ID),
            ]
            .into_iter()
            .collect(),
        })
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
                system_program: pk!(SYSTEM_PROGRAM),
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
            instruction_sysvar: pk!(INSTRUCTIONS_SYSVAR),
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
        if *token_program == pk!(protocol::TOKEN_2022_PROGRAM_ID) {
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
        if *token_program == pk!(protocol::TOKEN_2022_PROGRAM_ID) {
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
            kamino_program: pk!(protocol::KAMINO_PROGRAM_ID),
            farms_program: marginfi_type_crate::pdas::FARMS_PROGRAM_ID,
            collateral_token_program: pk!(protocol::TOKEN_PROGRAM_ID),
            liquidity_token_program: *token_program,
            instruction_sysvar_account: pk!(INSTRUCTIONS_SYSVAR),
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
            drift_program: pk!(protocol::DRIFT_PROGRAM_ID),
            token_program: *token_program,
            system_program: pk!(SYSTEM_PROGRAM),
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
            juplend_program: pk!(protocol::JUPLEND_PROGRAM_ID),
            token_program: *token_program,
            associated_token_program: pk!(protocol::ASSOCIATED_TOKEN_PROGRAM_ID),
            system_program: pk!(SYSTEM_PROGRAM),
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
            system_program: pk!(SYSTEM_PROGRAM),
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

        /// E2: the steps of a TX built by these builders, derived through the pinned
        /// instruction table: integration refresh, init record, Start, Withdraw, Swap, Repay,
        /// End — exactly the built order, End last.
        #[test]
        fn e2_built_receivership_derives_its_true_steps() {
            use crate::planner::{steps_of, Step};
            let (program, account, liquidator, bank, mint) = (p(), p(), p(), p(), p());
            let table = instruction_table(&program).unwrap();
            let kamino = pk!(protocol::KAMINO_PROGRAM_ID);
            let jup = pk!(protocol::JUP_SWAP_PROGRAM_ID);
            let refresh = Instruction {
                program_id: kamino,
                accounts: vec![],
                data: table.refreshes[&kamino][0].to_vec(),
            };
            let swap = Instruction {
                program_id: jup,
                accounts: vec![],
                data: vec![0xe5; 16],
            };
            let token = pk!(protocol::TOKEN_PROGRAM_ID);
            let ixs = build_receivership(ReceivershipParams {
                program: &program,
                group: &p(),
                marginfi_account: &account,
                liquidator: &liquidator,
                global_fee_wallet: &p(),
                start_observation: vec![],
                end_banks: vec![bank],
                init_record: true,
                pre_refreshes: vec![refresh],
                withdraws: vec![withdraw(
                    &program,
                    &p(),
                    &account,
                    &liquidator,
                    &bank,
                    &mint,
                    &p(),
                    &p(),
                    &token,
                    13,
                    None,
                    &[],
                )],
                swaps: vec![swap],
                repays: vec![repay(
                    &program,
                    &p(),
                    &account,
                    &liquidator,
                    &bank,
                    &mint,
                    &p(),
                    &p(),
                    &token,
                    7,
                    None,
                    &[],
                )],
            });
            assert_eq!(
                steps_of(&ixs, &table).unwrap(),
                [
                    Step::Refresh(kamino),
                    Step::InitLiquidationRecord,
                    Step::StartLiquidation,
                    Step::Withdraw,
                    Step::Swap(jup),
                    Step::Repay,
                    Step::EndLiquidation,
                ]
            );
        }

        #[test]
        fn token_2022_withdraw_includes_mint_before_observation() {
            let program = p();
            let mint = p();
            let observation = p();
            let token_program = pk!(protocol::TOKEN_2022_PROGRAM_ID);
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
            let token_program = pk!(protocol::TOKEN_PROGRAM_ID);
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
            let token_program = pk!(protocol::TOKEN_PROGRAM_ID);
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
            let token_program = pk!(protocol::TOKEN_PROGRAM_ID);
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
            let token_program = pk!(protocol::TOKEN_2022_PROGRAM_ID);
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

#[cfg(all(test, feature = "crate_health"))]
mod built_amounts_tests {
    use super::builders::{built_amounts, BuiltAmounts};
    use anchor_lang::InstructionData;
    use marginfi::instruction as mi;
    use solana_sdk::instruction::Instruction;
    use solana_sdk::pubkey::Pubkey;

    fn ix(program: Pubkey, data: Vec<u8>) -> Instruction {
        Instruction {
            program_id: program,
            accounts: vec![],
            data,
        }
    }

    /// E2: the withdraw (plain, Kamino, Drift, JupLend) and repay amounts and all-flags are read
    /// back from the built bytes; anything but one withdraw and one repay of the program, or an
    /// undecodable argument list, is refused.
    #[test]
    fn e2_built_amounts_are_decoded_from_the_signed_bytes() {
        let program = Pubkey::new_unique();
        let repay = |amount, all: bool| {
            ix(
                program,
                mi::LendingAccountRepay {
                    amount,
                    repay_all: all.then_some(true),
                }
                .data(),
            )
        };
        let withdraws = [
            (
                mi::LendingAccountWithdraw {
                    amount: 7,
                    withdraw_all: None,
                }
                .data(),
                (7, false),
            ),
            (
                mi::KaminoWithdraw {
                    amount: 8,
                    flags: Some(1),
                }
                .data(),
                (8, true),
            ),
            (
                mi::DriftWithdraw {
                    amount: 9,
                    withdraw_all: Some(false),
                }
                .data(),
                (9, false),
            ),
            (
                mi::JuplendWithdraw {
                    amount: 10,
                    withdraw_all: Some(true),
                }
                .data(),
                (10, true),
            ),
        ];
        for (data, expected) in withdraws {
            let other = ix(Pubkey::new_unique(), vec![9; 16]);
            assert_eq!(
                built_amounts(&[ix(program, data), other, repay(5, true)], &program),
                Ok(BuiltAmounts {
                    withdraw: expected,
                    repay: (5, true)
                })
            );
        }
        let w = ix(
            program,
            mi::LendingAccountWithdraw {
                amount: 1,
                withdraw_all: None,
            }
            .data(),
        );
        assert!(built_amounts(std::slice::from_ref(&w), &program)
            .unwrap_err()
            .starts_with("E2_AMOUNTS_SHAPE"));
        assert!(
            built_amounts(&[w.clone(), w.clone(), repay(1, false)], &program)
                .unwrap_err()
                .starts_with("E2_AMOUNTS_SHAPE")
        );
        let mut truncated = repay(1, false);
        truncated.data.truncate(12);
        assert_eq!(
            built_amounts(&[w.clone(), truncated], &program).unwrap_err(),
            "E2_AMOUNTS_UNDECODABLE"
        );
        let mut trailing = repay(1, false);
        trailing.data.push(0);
        assert_eq!(
            built_amounts(&[w, trailing], &program).unwrap_err(),
            "E2_AMOUNTS_UNDECODABLE"
        );
    }
}
