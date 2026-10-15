//! P1A: Protokoll-Konstanten — **BELEGT aus `marginfi-v2` main (Programm-Version 0.1.11)**,
//! gelesen in Sitzung 4. Mainnet-Program-ID aus `id-crate` (Feature mainnet-beta).
//! On-chain ProgramData-Hash wird trotzdem in P1A-01 verifiziert (Upgrade-Watch).

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// ---- Programme (id-crate / constants.rs / type-crate pdas.rs) ----
pub const PROGRAM_ID_MAINNET: &str = "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA";
pub const PROGRAM_ID_STAGING: &str = "stag8sTKds2h4KzjUw3zKTsxbqvT4XKHdaR9X9E6Rct"; // VERIFY: Feature-Zuordnung "staging" in id-crate
pub const KAMINO_PROGRAM_ID: &str = "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD";
pub const DRIFT_PROGRAM_ID: &str = "dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH";
pub const JUPLEND_PROGRAM_ID: &str = "jup3YeL8QhtSx1e253b2FDvsMNC87fDrgQZivbrndc9";
pub const JUP_SWAP_PROGRAM_ID: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const TITAN_SWAP_PROGRAM_ID: &str = "T1TANpTeScyeqVzzgNViGDNrkQ6qHz9KrSBS4aNXvGT";
pub const COMPUTE_BUDGET_PROGRAM_ID: &str = "ComputeBudget111111111111111111111111111111";
pub const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const NATIVE_SOL_MINT: &str = "So11111111111111111111111111111111111111112"; // [FIX-66]
/// Pyth Push-Oracle-Programm (PriceUpdateV2-PDAs). VERIFY: Konstante aus pyth-solana-receiver-sdk.
pub const PYTH_PUSH_ORACLE_ID: &str = "pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT";

/// Receivership-TX: **nur** diese Programme dürfen Top-Level-Instructions stellen
/// (`validate_instructions` in liquidate_start.rs). Kein System-Programm → **kein SOL-Transfer
/// (Jito-Tip) in der Receivership-TX**; Tip gehört in eine separate Bundle-TX.
/// Kein Token-Programm. Associated Token is listed by Marginfi's outer program allowlist, but
/// the pinned validate_ix_first rejects its 1-byte instruction encoding while scanning the TX;
/// therefore ATA creation is also excluded here and must be prewarmed outside receivership.
pub fn receivership_allowed_programs() -> Vec<Pubkey> {
    [
        COMPUTE_BUDGET_PROGRAM_ID,
        PROGRAM_ID_MAINNET,
        KAMINO_PROGRAM_ID,
        DRIFT_PROGRAM_ID,
        JUPLEND_PROGRAM_ID,
        JUP_SWAP_PROGRAM_ID,
        TITAN_SWAP_PROGRAM_ID,
    ]
    .iter()
    .map(|s| Pubkey::from_str(s).expect("static"))
    .collect()
}

// ---- Instruction-Discriminators (type-crate constants::ix_discriminators, BELEGT) ----
pub mod ix {
    pub const INIT_LIQUIDATION_RECORD: [u8; 8] = [236, 213, 238, 126, 147, 251, 164, 8];
    pub const START_LIQUIDATION: [u8; 8] = [244, 93, 90, 214, 192, 166, 191, 21];
    pub const END_LIQUIDATION: [u8; 8] = [110, 11, 244, 54, 229, 181, 22, 184];
    pub const LENDING_ACCOUNT_WITHDRAW: [u8; 8] = [36, 72, 74, 19, 210, 210, 192, 192];
    pub const LENDING_ACCOUNT_REPAY: [u8; 8] = [79, 209, 172, 177, 222, 51, 173, 151];
    pub const KAMINO_WITHDRAW: [u8; 8] = [199, 101, 41, 45, 213, 98, 224, 200];
    pub const DRIFT_WITHDRAW: [u8; 8] = [86, 59, 186, 123, 183, 181, 234, 137];
    pub const JUPLEND_WITHDRAW: [u8; 8] = [245, 164, 253, 202, 53, 77, 251, 221];
}

/// Erlaubte Instructions **vor** StartLiquidation (validate_ix_first): Kamino RefreshReserve /
/// RefreshReservesBatch / RefreshObligation, INIT_LIQUIDATION_RECORD, Drift
/// UpdateSpotMarketCumulativeInterest, JupLend UpdateRate. Zwischen Start und End ausschließlich:
/// start, end, init_liquidation_record, withdraw, repay, kamino/drift/juplend withdraw (+ allowlisted
/// Fremdprogramme). `end` ist die letzte Instruction; `start` darf nicht die letzte sein.
pub const RECEIVERSHIP_EXCLUSIVE_MARGINFI_IXS: [[u8; 8]; 8] = [
    ix::START_LIQUIDATION,
    ix::END_LIQUIDATION,
    ix::INIT_LIQUIDATION_RECORD,
    ix::LENDING_ACCOUNT_WITHDRAW,
    ix::LENDING_ACCOUNT_REPAY,
    ix::KAMINO_WITHDRAW,
    ix::DRIFT_WITHDRAW,
    ix::JUPLEND_WITHDRAW,
];

// ---- Seeds (type-crate constants, BELEGT) ----
pub const LIQUIDATION_RECORD_SEED: &[u8] = b"liq_record";
pub const LIQUIDITY_VAULT_AUTHORITY_SEED: &[u8] = b"liquidity_vault_auth";
pub const FEE_STATE_SEED: &[u8] = b"feestate";

pub fn liquidation_record_pda(program: &Pubkey, marginfi_account: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[LIQUIDATION_RECORD_SEED, marginfi_account.as_ref()], program).0
}
pub fn liquidity_vault_authority_pda(program: &Pubkey, bank: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[LIQUIDITY_VAULT_AUTHORITY_SEED, bank.as_ref()], program).0
}
pub fn fee_state_pda(program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[FEE_STATE_SEED], program).0
}
/// Pyth-Push PriceUpdateV2-PDA: [shard(u16 LE)=0, feed_id] unter dem Push-Oracle-Programm.
/// VERIFY: Shard-Index 0 und Seed-Layout (eva01 pyth_cranker: `PDA([0, feed_id], pyth-push-oracle)`).
pub fn pyth_push_price_pda(feed_id: &[u8; 32]) -> Pubkey {
    let prog = Pubkey::from_str(PYTH_PUSH_ORACLE_ID).expect("static");
    Pubkey::find_program_address(&[&0u16.to_le_bytes(), feed_id], &prog).0
}

// ---- Liquidations-Ökonomie (constants.rs, BELEGT) ----
/// `seized <= repaid × (1 + max(FeeState.liquidation_max_fee, 0.05))` — in Equity-(EMA-)Werten.
pub const LIQUIDATION_BONUS_FEE_MINIMUM_BPS: u64 = 500;
/// Accounts mit Equity-Assets < 5 USD dürfen vollständig geschlossen werden (ignore_healthy).
pub const LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD: u64 = 5;

// ---- Account-Discriminators (sha256("account:<Name>")[..8]) ----
pub const DISC_MARGINFI_ACCOUNT: [u8; 8] = [0x43, 0xb2, 0x82, 0x6d, 0x7e, 0x72, 0x1c, 0x2a];
pub const DISC_BANK: [u8; 8] = [0x8e, 0x31, 0xa6, 0xf2, 0x32, 0x42, 0x61, 0xbc];
pub const DISC_MARGINFI_GROUP: [u8; 8] = [0xb6, 0x17, 0xad, 0xf0, 0x97, 0xce, 0xb6, 0x43];
pub const DISC_FEE_STATE: [u8; 8] = [0x3f, 0xe0, 0x10, 0x55, 0xc1, 0x24, 0xeb, 0xdc];
pub const DISC_LIQUIDATION_RECORD: [u8; 8] = [0x5f, 0x74, 0x17, 0x84, 0x59, 0xd2, 0xf5, 0xa2];

/// Bank-Payload (ohne Discriminator): 1856 (0.1.11) / 3904 (0.1.12). Rohlänge = +8.
pub const BANK_PAYLOAD_LEN_0_1_11: usize = 1856;
pub const BANK_PAYLOAD_LEN_0_1_12: usize = 3904;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountType { MarginfiAccount, Bank, MarginfiGroup, FeeState, LiquidationRecord, Unknown }

pub fn classify(data: &[u8]) -> AccountType {
    if data.len() < 8 { return AccountType::Unknown; }
    let d: [u8; 8] = data[..8].try_into().unwrap();
    match d {
        DISC_MARGINFI_ACCOUNT => AccountType::MarginfiAccount,
        DISC_BANK => AccountType::Bank,
        DISC_MARGINFI_GROUP => AccountType::MarginfiGroup,
        DISC_FEE_STATE => AccountType::FeeState,
        DISC_LIQUIDATION_RECORD => AccountType::LiquidationRecord,
        _ => AccountType::Unknown,
    }
}

pub fn bank_layout(raw_len: usize) -> Option<&'static str> {
    match raw_len.checked_sub(8)? {
        BANK_PAYLOAD_LEN_0_1_11 => Some("0.1.11"),
        BANK_PAYLOAD_LEN_0_1_12 => Some("0.1.12"),
        _ => None,
    }
}

/// [FIX-27] Observation-Layout kommt ausschließlich aus `health::crate_impl::bank_observation_keys`
/// (OracleSetup-Enum des Type-Crates) — keine numerische Rekonstruktion mehr.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_and_layouts() {
        let mut d = DISC_BANK.to_vec();
        d.extend(vec![0u8; BANK_PAYLOAD_LEN_0_1_11]);
        assert_eq!(classify(&d), AccountType::Bank);
        assert_eq!(bank_layout(d.len()), Some("0.1.11"));
        assert_eq!(bank_layout(100), None);
        assert_eq!(receivership_allowed_programs().len(), 7);
    }

    #[test]
    fn pdas_are_deterministic() {
        let p = Pubkey::from_str(PROGRAM_ID_MAINNET).unwrap();
        let a = Pubkey::new_unique();
        assert_eq!(liquidation_record_pda(&p, &a), liquidation_record_pda(&p, &a));
        assert_ne!(fee_state_pda(&p), liquidity_vault_authority_pda(&p, &a));
    }
}
