//! P1A/P1B: Protokoll-Konstanten — gepinnt auf `marginfi-v2` `mrgn-0.1.12-rc2` (Programm 0.1.12,
//! P1B-Promotion; bis 23.10. `mrgn-0.1.11-rc1`), gegen das Type-Crate getestet. Mainnet-Program-ID
//! aus `id-crate` (Feature mainnet-beta). Der deployte ProgramData-Stand ist für diese Quelle noch
//! nicht qualifiziert (P1A BLOCKED): Upgrade-Watch und Startup bleiben gesperrt.

use solana_sdk::hash::{hash, Hash};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// ---- Programme (id-crate / constants.rs / type-crate pdas.rs) ----
pub const PROGRAM_ID_MAINNET: &str = "MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA";
/// BOOT-12 cluster pin: genesis hash of Solana mainnet-beta (the public, documented value). Not
/// re-read from a live endpoint for this pin: Mainnet reads need separate permission (NOT_RUN).
pub const GENESIS_HASH_MAINNET: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";
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

/// B14 latency: a PDA is a pure function of its seeds, but each derivation hashes and
/// curve-checks per bump (microseconds); the receivership builders need several per plan. A
/// bounded per-thread memo keyed by (seed, program, key) — cleared when full, never stale.
const PDA_MEMO_MAX: usize = 16_384;
type PdaMemo = std::collections::HashMap<(&'static [u8], Pubkey, Option<Pubkey>), Pubkey>;
thread_local!(static PDAS: std::cell::RefCell<PdaMemo> = Default::default());

fn memo_pda(seed: &'static [u8], program: &Pubkey, key: Option<&Pubkey>) -> Pubkey {
    PDAS.with(|memo| {
        let id = (seed, *program, key.copied());
        if let Some(pda) = memo.borrow().get(&id) {
            return *pda;
        }
        let pda = match key {
            Some(k) => Pubkey::find_program_address(&[seed, k.as_ref()], program),
            None => Pubkey::find_program_address(&[seed], program),
        }
        .0;
        let mut memo = memo.borrow_mut();
        if memo.len() >= PDA_MEMO_MAX {
            memo.clear();
        }
        memo.insert(id, pda);
        pda
    })
}

pub fn liquidation_record_pda(program: &Pubkey, marginfi_account: &Pubkey) -> Pubkey {
    memo_pda(LIQUIDATION_RECORD_SEED, program, Some(marginfi_account))
}
pub fn liquidity_vault_authority_pda(program: &Pubkey, bank: &Pubkey) -> Pubkey {
    memo_pda(LIQUIDITY_VAULT_AUTHORITY_SEED, program, Some(bank))
}
pub fn fee_state_pda(program: &Pubkey) -> Pubkey {
    memo_pda(FEE_STATE_SEED, program, None)
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

/// Bank-Payload (ohne Discriminator): 1856 (0.1.11) / 3904 (0.1.12, pinned). Rohlänge = +8.
pub const BANK_PAYLOAD_LEN_0_1_11: usize = 1856;
pub const BANK_PAYLOAD_LEN_0_1_12: usize = 3904;
/// The Bank payload of the pinned source (`size_of::<Bank>()`, tested by
/// `source_pin_parity_with_type_crate`), for code without the type crate.
pub const BANK_PAYLOAD_LEN_PINNED: usize = BANK_PAYLOAD_LEN_0_1_12;

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

/// P1B-02: why a Bank payload is not decodable by the pinned crate (0.1.12 since the promotion).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankLayoutError {
    Discriminator,
    /// A known layout of another program version than the pinned crate. Pinned 0.1.12 (now) +
    /// non-resized 0.1.11 bank: the program fails every TX touching it. (Pinned 0.1.11 + resized
    /// bank: the 1856 B prefix is byte-identical, but its premium state would be invisible, so a
    /// prefix decode would drop premium debt from Health.) Never a prefix decode either way.
    OtherVersion(&'static str),
    Length(usize),
}

impl std::fmt::Display for BankLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discriminator => write!(f, "UNKNOWN_LAYOUT: not a Bank discriminator"),
            Self::OtherVersion(v) => {
                write!(
                    f,
                    "UNKNOWN_LAYOUT: bank layout {v} is not the pinned crate layout"
                )
            }
            Self::Length(n) => write!(f, "UNKNOWN_LAYOUT: bank raw length {n}"),
        }
    }
}

/// Strict Bank decode for the pinned crate: discriminator and exact raw length
/// `8 + size_of::<Bank>()`. Promotion (P1B → P1A) is the crate bump; this check follows it.
/// The owner check stays with the caller, which knows the configured program.
/// D11: decodes independent of the buffer's alignment (see `pod_exact`).
#[cfg(feature = "crate_health")]
pub fn decode_bank(
    data: &[u8],
) -> Result<std::borrow::Cow<'_, marginfi_type_crate::types::Bank>, BankLayoutError> {
    use marginfi_type_crate::types::Bank;
    if classify(data) != AccountType::Bank {
        return Err(BankLayoutError::Discriminator);
    }
    if data.len() != 8 + std::mem::size_of::<Bank>() {
        return Err(bank_layout(data.len()).map_or(
            BankLayoutError::Length(data.len()),
            BankLayoutError::OtherVersion,
        ));
    }
    Ok(pod_exact(&data[8..]))
}

/// D11: a pinned type from an exactly length-checked payload, independent of where the buffer
/// happens to lie in memory: borrowed when aligned, otherwise an owned copy. Alignment is a
/// property of the local buffer, never of the on-chain layout, so it is never a decode error.
#[cfg(feature = "crate_health")]
fn pod_exact<T: bytemuck::Pod>(payload: &[u8]) -> std::borrow::Cow<'_, T> {
    match bytemuck::try_from_bytes(payload) {
        Ok(t) => std::borrow::Cow::Borrowed(t),
        Err(_) => std::borrow::Cow::Owned(bytemuck::pod_read_unaligned(payload)),
    }
}

/// B2: why a MarginfiAccount, MarginfiGroup or FeeState is not decodable by the pinned crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    Discriminator(&'static str),
    Length {
        kind: &'static str,
        found: usize,
        expected: usize,
    },
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discriminator(kind) => write!(f, "UNKNOWN_LAYOUT: not a {kind} discriminator"),
            Self::Length {
                kind,
                found,
                expected,
            } => write!(
                f,
                "UNKNOWN_LAYOUT: {kind} raw length {found} is not the pinned {expected}"
            ),
        }
    }
}

/// B2: discriminator and exact raw length `8 + size_of::<T>()` of the pinned crate; a longer
/// (resized/upgraded) or shorter account is UNKNOWN_LAYOUT, never a prefix decode. The owner
/// check stays with the caller, which knows the configured program.
#[cfg(feature = "crate_health")]
fn decode_exact<'a, T: bytemuck::Pod>(
    data: &'a [u8],
    disc: [u8; 8],
    kind: &'static str,
) -> Result<std::borrow::Cow<'a, T>, LayoutError> {
    if data.get(..8) != Some(disc.as_slice()) {
        return Err(LayoutError::Discriminator(kind));
    }
    let expected = 8 + std::mem::size_of::<T>();
    if data.len() != expected {
        return Err(LayoutError::Length {
            kind,
            found: data.len(),
            expected,
        });
    }
    Ok(pod_exact(&data[8..]))
}

#[cfg(feature = "crate_health")]
pub fn decode_marginfi_account(
    data: &[u8],
) -> Result<std::borrow::Cow<'_, marginfi_type_crate::types::MarginfiAccount>, LayoutError> {
    decode_exact(data, DISC_MARGINFI_ACCOUNT, "MarginfiAccount")
}

#[cfg(feature = "crate_health")]
pub fn decode_group(
    data: &[u8],
) -> Result<std::borrow::Cow<'_, marginfi_type_crate::types::MarginfiGroup>, LayoutError> {
    decode_exact(data, DISC_MARGINFI_GROUP, "MarginfiGroup")
}

#[cfg(feature = "crate_health")]
pub fn decode_fee_state(
    data: &[u8],
) -> Result<std::borrow::Cow<'_, marginfi_type_crate::types::FeeState>, LayoutError> {
    decode_exact(data, DISC_FEE_STATE, "FeeState")
}

// [FIX-27] Observation-Layout kommt ausschließlich aus `health::crate_impl::bank_observation_keys`
// (OracleSetup-Enum des Type-Crates) — keine numerische Rekonstruktion mehr.

// ---- P1A-01/P1A-04: Loader-v3 deployment freeze and upgrade watch ----
pub const BPF_LOADER_UPGRADEABLE_ID: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
/// Protocol source pin of the marginfi crates (`native/Cargo.toml`, `Cargo.lock`). P1B promotion
/// (UPCOMING_0_1_12.md P1B-03): the checked RC2 candidate of the upgraded Mainnet deployment.
pub const SOURCE_TAG: &str = "mrgn-0.1.12-rc2";
pub const SOURCE_COMMIT: &str = "9fcb729ae3a6de67544fe6f8147da8d86bdf3692";
/// SHA-256 (base58) of the complete Mainnet ProgramData account recorded by the qualified 0.1.11
/// Shadow upgrade watch (c0e1_4h_20261008T222326Z). Continuity evidence for its source
/// (`PROGRAM_DATA_REFERENCE_SOURCE_TAG`), not a reproducible-build proof; historical captures of
/// that deployment are still verified against it.
pub const PROGRAM_DATA_REFERENCE_HASH: &str = "H57mqzeof3mnCmVTPJzmzeTfs7G6zbpaApQP4yNycr3f";
/// The source the reference deployment was qualified for. It is not the current source pin, so
/// no deployment is qualified for the pinned source until P1A passes: the observed upgraded
/// ProgramData (FE-01 smoke 2026-10-24, SHA-256 `3e3626a9…`) is evidence only, never a pin.
pub const PROGRAM_DATA_REFERENCE_SOURCE_TAG: &str = "mrgn-0.1.11-rc1";
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

/// Startup half of the upgrade watch: only the Mainnet deployment qualified for the pinned
/// source may run unhalted.
pub fn deployment_pin(program: &Pubkey, d: &ProgramDeployment) -> DeploymentPin {
    pin_against(
        program,
        d,
        (
            PROGRAM_DATA_REFERENCE_HASH,
            PROGRAM_DATA_REFERENCE_SOURCE_TAG,
        ),
    )
}

/// `qualified`: the reference deployment hash and the source it was qualified for; a reference
/// of another source than `SOURCE_TAG` qualifies nothing.
fn pin_against(program: &Pubkey, d: &ProgramDeployment, qualified: (&str, &str)) -> DeploymentPin {
    if program.to_string() != PROGRAM_ID_MAINNET {
        DeploymentPin::Unpinned
    } else if qualified.1 == SOURCE_TAG && d.hash.to_string() == qualified.0 {
        DeploymentPin::Reference
    } else {
        DeploymentPin::NotReference
    }
}

#[cfg(test)]
mod tests {
    /// B14: the memoized PDAs equal the derivation, on a miss, a hit and after the bounded memo
    /// was cleared.
    #[test]
    fn b14_memoized_pdas_equal_the_derivation() {
        let (p, a) = (Pubkey::new_unique(), Pubkey::new_unique());
        let derive = |seeds: &[&[u8]]| Pubkey::find_program_address(seeds, &p).0;
        for _ in 0..2 {
            assert_eq!(
                liquidation_record_pda(&p, &a),
                derive(&[LIQUIDATION_RECORD_SEED, a.as_ref()])
            );
            assert_eq!(
                liquidity_vault_authority_pda(&p, &a),
                derive(&[LIQUIDITY_VAULT_AUTHORITY_SEED, a.as_ref()])
            );
            assert_eq!(fee_state_pda(&p), derive(&[FEE_STATE_SEED]));
        }
        // Another program, same account: its own PDA.
        let q = Pubkey::new_unique();
        assert_eq!(
            liquidation_record_pda(&q, &a),
            Pubkey::find_program_address(&[LIQUIDATION_RECORD_SEED, a.as_ref()], &q).0
        );
        for _ in 0..PDA_MEMO_MAX {
            liquidation_record_pda(&p, &Pubkey::new_unique());
        }
        assert!(PDAS.with(|m| m.borrow().len()) <= PDA_MEMO_MAX, "bounded");
        assert_eq!(
            liquidation_record_pda(&p, &a),
            derive(&[LIQUIDATION_RECORD_SEED, a.as_ref()])
        );
    }

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
        // P1B promotion: the 0.1.11 reference qualifies nothing for the 0.1.12 source pin — the
        // runtime stays halted on it as on any other deployment until P1A passes.
        assert_eq!(
            deployment_pin(&program, &reference),
            DeploymentPin::NotReference
        );
        // A deployment qualified for the pinned source is the reference; of another source, not.
        let qualified = (PROGRAM_DATA_REFERENCE_HASH, SOURCE_TAG);
        assert_eq!(
            pin_against(&program, &reference, qualified),
            DeploymentPin::Reference
        );
        assert_eq!(
            pin_against(&program, &d, qualified),
            DeploymentPin::NotReference
        );
        assert_eq!(
            pin_against(
                &program,
                &reference,
                (
                    PROGRAM_DATA_REFERENCE_HASH,
                    PROGRAM_DATA_REFERENCE_SOURCE_TAG
                )
            ),
            DeploymentPin::NotReference
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

    /// P1B-02/03: only the pinned 0.1.12 size decodes; a non-resized 0.1.11 bank is named (every
    /// TX touching it fails on chain as well), everything else unknown.
    #[cfg(feature = "crate_health")]
    #[test]
    fn dual_size_bank_decoder_fails_closed() {
        // u64 backing keeps the payload 8-byte aligned like real account data.
        let raw = |payload: usize, out: &mut Vec<u64>| -> usize {
            *out = vec![0u64; (8 + payload).div_ceil(8)];
            bytemuck::cast_slice_mut::<u64, u8>(out)[..8].copy_from_slice(&DISC_BANK);
            8 + payload
        };
        let mut buf = Vec::new();
        let n = raw(BANK_PAYLOAD_LEN_0_1_12, &mut buf);
        assert!(decode_bank(&bytemuck::cast_slice(&buf)[..n]).is_ok());
        let n = raw(BANK_PAYLOAD_LEN_0_1_11, &mut buf);
        assert_eq!(bank_layout(n), Some("0.1.11"));
        assert_eq!(
            decode_bank(&bytemuck::cast_slice(&buf)[..n]).err(),
            Some(BankLayoutError::OtherVersion("0.1.11"))
        );
        for payload in [
            0,
            BANK_PAYLOAD_LEN_0_1_11 - 8,
            BANK_PAYLOAD_LEN_0_1_11 + 8,
            BANK_PAYLOAD_LEN_0_1_12 - 8,
            BANK_PAYLOAD_LEN_0_1_12 + 8,
        ] {
            let n = raw(payload, &mut buf);
            assert_eq!(
                decode_bank(&bytemuck::cast_slice(&buf)[..n]).err(),
                Some(BankLayoutError::Length(n))
            );
        }
        let n = raw(BANK_PAYLOAD_LEN_0_1_12, &mut buf);
        bytemuck::cast_slice_mut::<u64, u8>(&mut buf)[0] ^= 1;
        assert_eq!(
            decode_bank(&bytemuck::cast_slice(&buf)[..n]).err(),
            Some(BankLayoutError::Discriminator)
        );
    }

    /// D11: a buffer's place in memory is not a layout: every pinned decoder yields the same
    /// value from an unaligned copy (owned) as from an aligned one (borrowed); length and
    /// discriminator stay exact.
    #[cfg(feature = "crate_health")]
    #[test]
    fn pinned_decoders_are_alignment_independent() {
        use bytemuck::Zeroable;
        use marginfi_type_crate::types::{Bank, FeeState, MarginfiAccount, MarginfiGroup};
        let mut bank = Bank::zeroed();
        bank.group = Pubkey::new_unique();
        bank.mint_decimals = 9;
        let mut account = MarginfiAccount::zeroed();
        account.group = bank.group;
        account.lending_account.balances[0].active = 1;
        let mut group = MarginfiGroup::zeroed();
        group.admin = Pubkey::new_unique();
        let mut fee = FeeState::zeroed();
        fee.global_fee_wallet = Pubkey::new_unique();
        let bytes = |disc: [u8; 8], payload: &[u8]| [disc.as_slice(), payload].concat();
        // u64 backing, so offset 0 is 8-byte aligned and offsets 1..8 are not.
        let at = |raw: &[u8], offset: usize| -> (Vec<u64>, usize) {
            let mut buf = vec![0u64; (raw.len() + offset).div_ceil(8) + 1];
            bytemuck::cast_slice_mut::<u64, u8>(&mut buf)[offset..offset + raw.len()]
                .copy_from_slice(raw);
            (buf, offset)
        };
        let raws = [
            bytes(DISC_BANK, bytemuck::bytes_of(&bank)),
            bytes(DISC_MARGINFI_ACCOUNT, bytemuck::bytes_of(&account)),
            bytes(DISC_MARGINFI_GROUP, bytemuck::bytes_of(&group)),
            bytes(DISC_FEE_STATE, bytemuck::bytes_of(&fee)),
        ];
        for offset in 0..8 {
            let views: Vec<(Vec<u64>, usize)> = raws.iter().map(|r| at(r, offset)).collect();
            let slice = |i: usize| {
                let (buf, o) = &views[i];
                &bytemuck::cast_slice::<u64, u8>(buf)[*o..*o + raws[i].len()]
            };
            let b = decode_bank(slice(0)).unwrap();
            assert_eq!(
                (b.group, b.mint_decimals),
                (bank.group, 9),
                "offset {offset}"
            );
            let a = decode_marginfi_account(slice(1)).unwrap();
            assert_eq!(a.group, account.group);
            assert_eq!(decode_group(slice(2)).unwrap().admin, group.admin);
            assert_eq!(
                decode_fee_state(slice(3)).unwrap().global_fee_wallet,
                fee.global_fee_wallet
            );
        }
        // Exactness is unchanged: one byte more or less and a foreign discriminator fail.
        for (raw, kind) in raws[1..]
            .iter()
            .zip(["MarginfiAccount", "MarginfiGroup", "FeeState"])
        {
            let decode = |d: &[u8]| -> Result<(), LayoutError> {
                match kind {
                    "MarginfiAccount" => decode_marginfi_account(d).map(drop),
                    "MarginfiGroup" => decode_group(d).map(drop),
                    _ => decode_fee_state(d).map(drop),
                }
            };
            let mut longer = raw.clone();
            longer.push(0);
            assert_eq!(
                decode(&longer),
                Err(LayoutError::Length {
                    kind,
                    found: raw.len() + 1,
                    expected: raw.len()
                })
            );
            assert!(decode(&raw[..raw.len() - 1]).is_err());
            let mut foreign = raw.clone();
            foreign[0] ^= 1;
            assert_eq!(decode(&foreign), Err(LayoutError::Discriminator(kind)));
        }
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
        assert_eq!(std::mem::size_of::<Bank>(), BANK_PAYLOAD_LEN_0_1_12);
        assert_eq!(BANK_PAYLOAD_LEN_PINNED, std::mem::size_of::<Bank>());
        assert_eq!(marginfi_type_crate::ID.to_string(), PROGRAM_ID_MAINNET);
        let lock = include_str!("../../../Cargo.lock");
        assert!(lock.contains(&format!("tag={SOURCE_TAG}#{SOURCE_COMMIT}")));
    }
}
