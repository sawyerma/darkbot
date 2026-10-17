//! RELATED_NON_LIQUIDATION (PHASENPLAN L.5): the four Marginfi instructions that accompany
//! liquidation paths but are never liquidations, never winners and never profit. Each one is
//! recorded with its signature, slot, exact instruction position (outer index, CPI index, stack
//! height), transaction status and the account roles of the pinned source (`33c67987`
//! Accounts structs: `DriftClaimBadDebt`, `LendingAccountPurgeDelevBalance`, `KeeperCloseOrder`,
//! `InitLiquidationRecord`). Detection is by exact pinned discriminator only — no name or log
//! text heuristic. These rows feed reports only; nothing here reaches classification, economics
//! or the winner path.
use super::*;
use crate::analytics::journal;
use crate::analytics::payload::Payload;
use crate::winner::{resolve_account_keys, ChainTx, TxStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RelatedKind {
    DriftClaimBadDebt,
    PurgeDeleverageBalance,
    KeeperCloseOrder,
    InitLiqRecord,
}

/// Account positions of one instruction's roles in its pinned Accounts struct.
struct Roles {
    target: Option<usize>,
    actor: Option<usize>,
    bank: Option<usize>,
    fee_recipient: Option<usize>,
}

impl RelatedKind {
    pub const ALL: [RelatedKind; 4] = [
        RelatedKind::DriftClaimBadDebt,
        RelatedKind::PurgeDeleverageBalance,
        RelatedKind::KeeperCloseOrder,
        RelatedKind::InitLiqRecord,
    ];
    /// Program entrypoint name in the pinned source (also the report filter value).
    pub fn name(self) -> &'static str {
        match self {
            Self::DriftClaimBadDebt => "drift_claim_bad_debt",
            Self::PurgeDeleverageBalance => "purge_deleverage_balance",
            Self::KeeperCloseOrder => "marginfi_account_keeper_close_order",
            Self::InitLiqRecord => "marginfi_account_init_liq_record",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
    /// Anchor rule `sha256("global:<name>")[..8]` of the pinned entrypoint (bound by a test).
    pub fn discriminator(self) -> [u8; 8] {
        match self {
            Self::DriftClaimBadDebt => [163, 67, 144, 231, 119, 20, 220, 33],
            Self::PurgeDeleverageBalance => [132, 187, 25, 149, 181, 59, 253, 136],
            Self::KeeperCloseOrder => [128, 114, 71, 46, 194, 71, 186, 106],
            Self::InitLiqRecord => [236, 213, 238, 126, 147, 251, 164, 8],
        }
    }
    fn roles(self) -> Roles {
        let r = |target, actor, bank, fee_recipient| Roles {
            target,
            actor,
            bank,
            fee_recipient,
        };
        match self {
            // payer (signer), bank, fee_state, …: affects a bank, not a liquidated account.
            Self::DriftClaimBadDebt => r(None, Some(0), Some(1), None),
            // group, marginfi_account, risk_admin (signer), bank.
            Self::PurgeDeleverageBalance => r(Some(1), Some(2), Some(3), None),
            // marginfi_account, fee_recipient (not a signer), order.
            Self::KeeperCloseOrder => r(Some(0), None, None, Some(1)),
            // marginfi_account, fee_payer (signer), liquidation_record, system_program.
            Self::InitLiqRecord => r(Some(0), Some(1), None, None),
        }
    }
    fn of(data: &[u8]) -> Option<Self> {
        let d = data.get(..8)?;
        Self::ALL.into_iter().find(|k| k.discriminator() == d)
    }
}

/// Transaction outcome of the instruction's transaction (never relabelled as success).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelatedStatus {
    Success,
    Failed,
    Unknown(String),
}
impl RelatedStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
            Self::Unknown(_) => "UNKNOWN",
        }
    }
}

/// One related instruction (`MARKET_RELATED` frame).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketRelated {
    pub signature: String,
    pub slot: u64,
    /// Index in the finalized block when known.
    pub transaction_index: Option<u64>,
    pub kind: RelatedKind,
    /// Top-level instruction index; for a CPI the index of its outer instruction.
    pub outer_index: u16,
    /// Position inside the outer instruction's CPI group (`None` = top level).
    pub inner_index: Option<u16>,
    pub stack_height: Option<u32>,
    pub status: RelatedStatus,
    pub target: Option<Pubkey>,
    pub actor: Option<Pubkey>,
    pub bank: Option<Pubkey>,
    pub fee_recipient: Option<Pubkey>,
    /// Every account of the instruction in pinned order.
    pub accounts: Vec<Pubkey>,
}
impl Payload for MarketRelated {
    const KIND: u16 = journal::kind::MARKET_RELATED;
}
impl MarketRelated {
    /// `name@outer.inner` (same position notation as the A4 marker comparison).
    pub fn position(&self) -> String {
        format!(
            "{}@{}.{}",
            self.kind.name(),
            self.outer_index,
            self.inner_index.map_or("-".to_string(), |i| i.to_string())
        )
    }
}

/// Every related instruction of `tx` in execution order (top level and CPI). Fails closed when
/// the CPI groups or an account index are missing, because absence could not be proven.
pub fn decode(
    tx: &ChainTx,
    program: &Pubkey,
    slot: u64,
    transaction_index: Option<u64>,
) -> Result<Vec<MarketRelated>, &'static str> {
    let keys = resolve_account_keys(tx).map_err(|e| e.code())?;
    let inner = tx
        .meta
        .as_ref()
        .and_then(|m| m.inner_instructions.as_ref())
        .ok_or("INNER_INSTRUCTIONS_UNAVAILABLE")?;
    let status = match tx.status() {
        TxStatus::Success => RelatedStatus::Success,
        TxStatus::Failed(_) => RelatedStatus::Failed,
        TxStatus::Unknown(code) => RelatedStatus::Unknown(code.to_string()),
    };
    let signature = tx.signatures.first().ok_or("SIGNATURE_MISSING")?.clone();
    let mut out = Vec::new();
    for (outer, ix) in tx.message.instructions.iter().enumerate() {
        let group = inner.iter().find(|g| usize::from(g.index) == outer);
        let all = std::iter::once((None, ix)).chain(group.into_iter().flat_map(|g| {
            g.instructions
                .iter()
                .enumerate()
                .map(|(j, ix)| (Some(j), ix))
        }));
        for (inner_index, ix) in all {
            if keys.get(usize::from(ix.program_id_index)) != Some(program) {
                continue;
            }
            if ix.data.len() < 8 {
                return Err("MARGINFI_INSTRUCTION_DISCRIMINATOR");
            }
            let Some(kind) = RelatedKind::of(&ix.data) else {
                continue;
            };
            let accounts = ix
                .accounts
                .iter()
                .map(|i| keys.get(usize::from(*i)).copied())
                .collect::<Option<Vec<Pubkey>>>()
                .ok_or("RELATED_ACCOUNT_INDEX")?;
            let roles = kind.roles();
            let role = |i: Option<usize>| -> Result<Option<Pubkey>, &'static str> {
                i.map(|i| accounts.get(i).copied().ok_or("RELATED_ACCOUNT_LAYOUT"))
                    .transpose()
            };
            out.push(MarketRelated {
                signature: signature.clone(),
                slot,
                transaction_index,
                kind,
                outer_index: u16::try_from(outer).map_err(|_| "RELATED_POSITION")?,
                inner_index: inner_index
                    .map(u16::try_from)
                    .transpose()
                    .map_err(|_| "RELATED_POSITION")?,
                stack_height: ix.stack_height,
                status: status.clone(),
                target: role(roles.target)?,
                actor: role(roles.actor)?,
                bank: role(roles.bank)?,
                fee_recipient: role(roles.fee_recipient)?,
                accounts,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_solana::chain::{ChainInstruction, ChainMessage, ChainMeta, InnerInstructions};

    fn ix(program: u8, accounts: Vec<u8>, data: Vec<u8>, stack: Option<u32>) -> ChainInstruction {
        ChainInstruction {
            program_id_index: program,
            accounts,
            data,
            stack_height: stack,
        }
    }
    fn tx(
        keys: &[Pubkey],
        outer: Vec<ChainInstruction>,
        inner: Option<Vec<InnerInstructions>>,
        failed: bool,
    ) -> ChainTx {
        ChainTx {
            slot: 9,
            block_time: None,
            transaction_index: None,
            version: None,
            signatures: vec!["sig".into()],
            message: ChainMessage {
                header: None,
                account_keys: keys.to_vec(),
                recent_blockhash: None,
                instructions: outer,
                address_table_lookups: None,
                transaction_config: None,
            },
            meta: Some(ChainMeta {
                status: if failed {
                    TxStatus::Failed(solana_sdk::transaction::TransactionError::AccountInUse)
                } else {
                    TxStatus::Success
                },
                fee: None,
                pre_balances: None,
                post_balances: None,
                pre_token_balances: None,
                post_token_balances: None,
                inner_instructions: inner,
                log_messages: None,
                loaded_addresses: None,
                compute_units_consumed: None,
                cost_units: None,
            }),
        }
    }

    /// Every discriminator is the Anchor rule of a pinned program entrypoint (the audited
    /// instruction namespace of the pinned IDL) and none collides with a liquidation marker.
    #[test]
    fn discriminators_are_pinned_and_disjoint_from_markers() {
        for k in RelatedKind::ALL {
            let d = solana_sdk::hash::hash(format!("global:{}", k.name()).as_bytes()).to_bytes();
            assert_eq!(&d[..8], &k.discriminator(), "{}", k.name());
            assert!(
                super::super::paths::instruction_known(&k.discriminator()),
                "{}",
                k.name()
            );
            assert_eq!(RelatedKind::parse(k.name()), Some(k));
            assert!(super::super::parity::MARKERS
                .iter()
                .all(|(m, _)| *m != k.discriminator()));
        }
    }

    /// Top-level and CPI positions, pinned roles, failed status kept as FAILED, foreign
    /// programs and markers ignored, and fail-closed decoding without CPI groups.
    #[test]
    fn positions_roles_status_and_fail_closed() {
        let p = Pubkey::new_unique();
        let other = Pubkey::new_unique();
        let a: Vec<Pubkey> = (0..5).map(|_| Pubkey::new_unique()).collect();
        // keys: 0..4 accounts, 5 marginfi, 6 foreign program
        let keys: Vec<Pubkey> = a.iter().copied().chain([p, other]).collect();
        let data = |k: RelatedKind| {
            let mut d = k.discriminator().to_vec();
            d.extend([1, 2, 3]);
            d
        };
        let marker = super::super::parity::MARKERS[1].0.to_vec();
        let outer = vec![
            ix(5, vec![0, 1, 2], data(RelatedKind::InitLiqRecord), Some(1)),
            ix(6, vec![0], data(RelatedKind::DriftClaimBadDebt), Some(1)),
            ix(5, vec![0, 1], marker, Some(1)),
        ];
        let inner = vec![InnerInstructions {
            index: 2,
            instructions: vec![
                ix(
                    5,
                    vec![0, 1, 2, 3],
                    data(RelatedKind::PurgeDeleverageBalance),
                    Some(2),
                ),
                ix(
                    5,
                    vec![4, 3, 2],
                    data(RelatedKind::KeeperCloseOrder),
                    Some(2),
                ),
            ],
        }];
        let t = tx(&keys, outer.clone(), Some(inner.clone()), false);
        let rows = decode(&t, &p, 9, Some(4)).unwrap();
        let positions: Vec<_> = rows.iter().map(|r| r.position()).collect();
        assert_eq!(
            positions,
            [
                "marginfi_account_init_liq_record@0.-",
                "purge_deleverage_balance@2.0",
                "marginfi_account_keeper_close_order@2.1"
            ]
        );
        assert_eq!((rows[0].target, rows[0].actor), (Some(a[0]), Some(a[1])));
        assert_eq!(
            (rows[1].target, rows[1].actor, rows[1].bank),
            (Some(a[1]), Some(a[2]), Some(a[3]))
        );
        assert_eq!(
            (rows[2].target, rows[2].actor, rows[2].fee_recipient),
            (Some(a[4]), None, Some(a[3]))
        );
        assert!(rows
            .iter()
            .all(|r| r.status == RelatedStatus::Success && r.transaction_index == Some(4)));
        assert_eq!(rows[1].stack_height, Some(2));
        let failed = decode(&tx(&keys, outer.clone(), Some(inner), true), &p, 9, None).unwrap();
        assert!(failed.iter().all(|r| r.status == RelatedStatus::Failed));
        assert_eq!(
            decode(&tx(&keys, outer.clone(), None, false), &p, 9, None),
            Err("INNER_INSTRUCTIONS_UNAVAILABLE")
        );
        // Too few accounts for the pinned layout fails closed.
        let short = vec![ix(
            5,
            vec![0, 1],
            data(RelatedKind::PurgeDeleverageBalance),
            Some(1),
        )];
        assert_eq!(
            decode(&tx(&keys, short, Some(vec![]), false), &p, 9, None),
            Err("RELATED_ACCOUNT_LAYOUT")
        );
        // Round trip through the typed journal payload.
        let bytes = crate::analytics::payload::encode(&rows[1]).unwrap();
        let back: MarketRelated =
            crate::analytics::payload::decode(MarketRelated::KIND, MarketRelated::SCHEMA, &bytes)
                .unwrap();
        assert_eq!(back, rows[1]);
    }
}
