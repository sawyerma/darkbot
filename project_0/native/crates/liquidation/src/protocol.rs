//! P1A: Protokoll-Konstanten — **BELEGT aus `marginfi-v2` main (Programm-Version 0.1.11)**,
//! gelesen in Sitzung 4. Mainnet-Program-ID aus `id-crate` (Feature mainnet-beta).
//! On-chain ProgramData-Hash wird trotzdem in P1A-01 verifiziert (Upgrade-Watch).

use solana_sdk::hash::{hash, Hash};
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
    Pubkey::find_program_address(
        &[LIQUIDATION_RECORD_SEED, marginfi_account.as_ref()],
        program,
    )
    .0
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
pub enum AccountType {
    MarginfiAccount,
    Bank,
    MarginfiGroup,
    FeeState,
    LiquidationRecord,
    Unknown,
}

pub fn classify(data: &[u8]) -> AccountType {
    if data.len() < 8 {
        return AccountType::Unknown;
    }
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

// ---- P1A-01/P1A-04: Loader-v3 deployment freeze and upgrade watch ----
pub const BPF_LOADER_UPGRADEABLE_ID: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
/// Protocol source pin of the marginfi crates (`native/Cargo.toml`, `Cargo.lock`).
pub const SOURCE_TAG: &str = "mrgn-0.1.11-rc1";
pub const SOURCE_COMMIT: &str = "33c67987a661f59c6683a65485ebb4f0ef030b77";
/// SHA-256 (base58) of the complete Mainnet ProgramData account recorded by the qualified 0.1.11
/// Shadow upgrade watch (c0e1_4h_20261008T222326Z). Continuity evidence for the source pin, not a
/// reproducible-build proof.
pub const PROGRAM_DATA_REFERENCE_HASH: &str = "H57mqzeof3mnCmVTPJzmzeTfs7G6zbpaApQP4yNycr3f";
/// `UpgradeableLoaderState::ProgramData` metadata: tag u32, slot u64, Option<Pubkey> (always 33 bytes).
pub const PROGRAM_DATA_METADATA_LEN: usize = 45;
const PROGRAM_ACCOUNT_LEN: usize = 36;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentError {
    Owner,
    Layout,
    Address,
}

/// Frozen deployment identity: everything a ProgramData change can alter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramDeployment {
    pub program_data: Pubkey,
    pub deployment_slot: u64,
    pub upgrade_authority: Option<Pubkey>,
    pub hash: Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentPin {
    /// Exactly the qualified, source-pinned deployment.
    Reference,
    /// Different bytes than the qualified deployment: upgrade since qualification → EXECUTION HALT.
    NotReference,
    /// No qualified deployment exists for this program (staging/devnet/local) → EXECUTION HALT.
    Unpinned,
}

pub fn program_data_address(program: &Pubkey) -> Pubkey {
    let loader = Pubkey::from_str(BPF_LOADER_UPGRADEABLE_ID).expect("static");
    Pubkey::find_program_address(&[program.as_ref()], &loader).0
}

/// `UpgradeableLoaderState::Program { programdata_address }` of an executable program account.
pub fn decode_program(
    program: &Pubkey,
    owner: &Pubkey,
    executable: bool,
    data: &[u8],
) -> Result<Pubkey, DeploymentError> {
    if owner.to_string() != BPF_LOADER_UPGRADEABLE_ID || !executable {
        return Err(DeploymentError::Owner);
    }
    if data.len() != PROGRAM_ACCOUNT_LEN || data[..4] != 2u32.to_le_bytes() {
        return Err(DeploymentError::Layout);
    }
    let pd = Pubkey::new_from_array(data[4..36].try_into().expect("len"));
    if pd != program_data_address(program) {
        return Err(DeploymentError::Address);
    }
    Ok(pd)
}

/// `UpgradeableLoaderState::ProgramData { slot, upgrade_authority_address }` plus the hash of the
/// complete account data (metadata and program bytes).
pub fn decode_program_data(
    program_data: &Pubkey,
    owner: &Pubkey,
    executable: bool,
    data: &[u8],
) -> Result<ProgramDeployment, DeploymentError> {
    if owner.to_string() != BPF_LOADER_UPGRADEABLE_ID || executable {
        return Err(DeploymentError::Owner);
    }
    if data.len() < PROGRAM_DATA_METADATA_LEN || data[..4] != 3u32.to_le_bytes() {
        return Err(DeploymentError::Layout);
    }
    let upgrade_authority = match data[12] {
        0 => None,
        1 => Some(Pubkey::new_from_array(
            data[13..45].try_into().expect("len"),
        )),
        _ => return Err(DeploymentError::Layout),
    };
    Ok(ProgramDeployment {
        program_data: *program_data,
        deployment_slot: u64::from_le_bytes(data[4..12].try_into().expect("len")),
        upgrade_authority,
        hash: hash(data),
    })
}

/// Startup half of the upgrade watch: only the qualified Mainnet deployment may run unhalted.
pub fn deployment_pin(program: &Pubkey, d: &ProgramDeployment) -> DeploymentPin {
    if program.to_string() != PROGRAM_ID_MAINNET {
        DeploymentPin::Unpinned
    } else if d.hash.to_string() == PROGRAM_DATA_REFERENCE_HASH {
        DeploymentPin::Reference
    } else {
        DeploymentPin::NotReference
    }
}

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
        assert_eq!(
            liquidation_record_pda(&p, &a),
            liquidation_record_pda(&p, &a)
        );
        assert_ne!(fee_state_pda(&p), liquidity_vault_authority_pda(&p, &a));
    }

    #[test]
    fn deployment_freeze_decodes_and_fails_closed() {
        let program = Pubkey::from_str(PROGRAM_ID_MAINNET).unwrap();
        let loader = Pubkey::from_str(BPF_LOADER_UPGRADEABLE_ID).unwrap();
        let pd = program_data_address(&program);
        let mut prog = 2u32.to_le_bytes().to_vec();
        prog.extend_from_slice(pd.as_ref());
        assert_eq!(decode_program(&program, &loader, true, &prog), Ok(pd));
        assert_eq!(
            decode_program(&program, &loader, false, &prog),
            Err(DeploymentError::Owner)
        );
        assert_eq!(
            decode_program(&program, &Pubkey::new_unique(), true, &prog),
            Err(DeploymentError::Owner)
        );
        assert_eq!(
            decode_program(&Pubkey::new_unique(), &loader, true, &prog),
            Err(DeploymentError::Address)
        );
        assert_eq!(
            decode_program(&program, &loader, true, &prog[..35]),
            Err(DeploymentError::Layout)
        );

        let authority = Pubkey::new_unique();
        let mut data = 3u32.to_le_bytes().to_vec();
        data.extend_from_slice(&123_456u64.to_le_bytes());
        data.push(1);
        data.extend_from_slice(authority.as_ref());
        data.extend_from_slice(&[0xAB; 64]);
        let d = decode_program_data(&pd, &loader, false, &data).unwrap();
        assert_eq!(d.deployment_slot, 123_456);
        assert_eq!(d.upgrade_authority, Some(authority));
        assert_eq!(d.hash, hash(&data));
        assert_eq!(deployment_pin(&program, &d), DeploymentPin::NotReference);
        assert_eq!(
            deployment_pin(&Pubkey::new_unique(), &d),
            DeploymentPin::Unpinned
        );
        let reference = ProgramDeployment {
            hash: Hash::from_str(PROGRAM_DATA_REFERENCE_HASH).unwrap(),
            ..d
        };
        assert_eq!(
            deployment_pin(&program, &reference),
            DeploymentPin::Reference
        );

        // Immutable program: authority tag 0, the 32 reserved bytes stay in the metadata.
        data[12] = 0;
        assert_eq!(
            decode_program_data(&pd, &loader, false, &data)
                .unwrap()
                .upgrade_authority,
            None
        );
        data[12] = 2;
        assert_eq!(
            decode_program_data(&pd, &loader, false, &data),
            Err(DeploymentError::Layout)
        );
        data[12] = 1;
        data[0] = 2;
        assert_eq!(
            decode_program_data(&pd, &loader, false, &data),
            Err(DeploymentError::Layout)
        );
        data[0] = 3;
        assert_eq!(
            decode_program_data(&pd, &loader, true, &data),
            Err(DeploymentError::Owner)
        );
        assert_eq!(
            decode_program_data(&pd, &loader, false, &data[..PROGRAM_DATA_METADATA_LEN - 1]),
            Err(DeploymentError::Layout)
        );
    }

    /// P1A-02: the hand-pinned constants equal the pinned type crate and lockfile.
    #[cfg(feature = "crate_health")]
    #[test]
    fn source_pin_parity_with_type_crate() {
        use marginfi_type_crate::constants::{discriminators as d, ix_discriminators as i};
        use marginfi_type_crate::types::Bank;
        assert_eq!(DISC_MARGINFI_ACCOUNT, d::ACCOUNT);
        assert_eq!(DISC_BANK, d::BANK);
        assert_eq!(DISC_MARGINFI_GROUP, d::GROUP);
        assert_eq!(DISC_FEE_STATE, d::FEE_STATE);
        assert_eq!(DISC_LIQUIDATION_RECORD, d::LIQUIDATION_RECORD);
        assert_eq!(
            RECEIVERSHIP_EXCLUSIVE_MARGINFI_IXS,
            [
                i::START_LIQUIDATION,
                i::END_LIQUIDATION,
                i::INIT_LIQUIDATION_RECORD,
                i::LENDING_ACCOUNT_WITHDRAW,
                i::LENDING_ACCOUNT_REPAY,
                i::KAMINO_WITHDRAW,
                i::DRIFT_WITHDRAW,
                i::JUPLEND_WITHDRAW,
            ]
        );
        assert_eq!(std::mem::size_of::<Bank>(), BANK_PAYLOAD_LEN_0_1_11);
        assert_eq!(marginfi_type_crate::ID.to_string(), PROGRAM_ID_MAINNET);
        let lock = include_str!("../../../Cargo.lock");
        assert!(lock.contains(&format!("tag={SOURCE_TAG}#{SOURCE_COMMIT}")));
    }
}
