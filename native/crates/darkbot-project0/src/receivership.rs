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

use solana_sdk::instruction::Instruction;
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
    /// Observation-Accounts in Programm-Reihenfolge (HealthEngine::observation_accounts).
    pub observation: &'a [Pubkey],
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
    use solana_sdk::instruction::AccountMeta;
    use std::str::FromStr;

    fn pk(s: &str) -> Pubkey {
        Pubkey::from_str(s).expect("static")
    }

    fn obs(observation: &[Pubkey]) -> Vec<AccountMeta> {
        observation.iter().map(|k| AccountMeta::new_readonly(*k, false)).collect()
    }

    pub fn init_liquidation_record(program: &Pubkey, marginfi_account: &Pubkey, fee_payer: &Pubkey) -> Instruction {
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

    pub fn start_liquidation(program: &Pubkey, marginfi_account: &Pubkey, group: &Pubkey, receiver: &Pubkey, observation: &[Pubkey]) -> Instruction {
        let mut accounts = marginfi::accounts::StartLiquidation {
            marginfi_account: *marginfi_account,
            liquidation_record: protocol::liquidation_record_pda(program, marginfi_account),
            group: *group,
            liquidation_receiver: *receiver,
            instruction_sysvar: pk(INSTRUCTIONS_SYSVAR),
        }
        .to_account_metas(Some(true));
        accounts.extend(obs(observation));
        Instruction { program_id: *program, accounts, data: marginfi::instruction::StartLiquidation.data() }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn withdraw(program: &Pubkey, group: &Pubkey, marginfi_account: &Pubkey, authority: &Pubkey, bank: &Pubkey,
                    destination_token_account: &Pubkey, liquidity_vault: &Pubkey, token_program: &Pubkey,
                    amount: u64, withdraw_all: Option<bool>, observation: &[Pubkey]) -> Instruction {
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
        accounts.extend(obs(observation));
        Instruction { program_id: *program, accounts, data: marginfi::instruction::LendingAccountWithdraw { amount, withdraw_all }.data() }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn repay(program: &Pubkey, group: &Pubkey, marginfi_account: &Pubkey, authority: &Pubkey, bank: &Pubkey,
                 signer_token_account: &Pubkey, liquidity_vault: &Pubkey, token_program: &Pubkey,
                 amount: u64, repay_all: Option<bool>) -> Instruction {
        Instruction {
            program_id: *program,
            accounts: marginfi::accounts::LendingAccountRepay {
                group: *group,
                marginfi_account: *marginfi_account,
                authority: *authority,
                bank: *bank,
                signer_token_account: *signer_token_account,
                liquidity_vault: *liquidity_vault,
                token_program: *token_program,
            }
            .to_account_metas(Some(true)),
            data: marginfi::instruction::LendingAccountRepay { amount, repay_all }.data(),
        }
    }

    pub fn end_liquidation(program: &Pubkey, marginfi_account: &Pubkey, group: &Pubkey, receiver: &Pubkey,
                           global_fee_wallet: &Pubkey, observation: &[Pubkey]) -> Instruction {
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
        accounts.extend(obs(observation));
        Instruction { program_id: *program, accounts, data: marginfi::instruction::EndLiquidation.data() }
    }

    pub fn build_receivership(p: ReceivershipParams<'_>) -> Vec<Instruction> {
        let mut v = Vec::with_capacity(4 + p.pre_refreshes.len() + p.withdraws.len() + p.swaps.len() + p.repays.len());
        v.extend(p.pre_refreshes);
        if p.init_record {
            v.push(init_liquidation_record(p.program, p.marginfi_account, p.liquidator));
        }
        v.push(start_liquidation(p.program, p.marginfi_account, p.group, p.liquidator, p.observation));
        v.extend(p.withdraws);
        v.extend(p.swaps);
        v.extend(p.repays);
        v.push(end_liquidation(p.program, p.marginfi_account, p.group, p.liquidator, p.global_fee_wallet, p.observation));
        v
    }
}
