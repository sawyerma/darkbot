//! The six Marginfi liquidation instruction markers (PHASENPLAN L.1) and their start/end pairs.
//!
//! Six markers are not six liquidations: `start_liquidation`/`end_liquidation` and
//! `start_deleverage`/`end_deleverage` are one RECEIVERSHIP resp. FORCED_DELEVERAGE path, while
//! `lending_account_liquidate` (CLASSIC) and `lending_pool_handle_bankruptcy`
//! (BANKRUPTCY_SETTLEMENT) complete in one instruction. Detection is by exact Anchor
//! discriminator of the pinned source (`33c67987`), roles by its Accounts order — the same table
//! as the pinned audit (`audit_market_quick.py`) and the canonical classifier. Markers are
//! report evidence only: hiding one never changes a classification, and a marker alone is never
//! a liquidation, a winner or profit.
use super::truth::LiquidationType;
use super::*;
use crate::winner::{resolve_account_keys, ChainTx, TxStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Marker {
    LendingAccountLiquidate,
    StartLiquidation,
    EndLiquidation,
    StartDeleverage,
    EndDeleverage,
    LendingPoolHandleBankruptcy,
}

impl Marker {
    /// Contract order (`--report-markers`).
    pub const ALL: [Marker; 6] = [
        Marker::LendingAccountLiquidate,
        Marker::StartLiquidation,
        Marker::EndLiquidation,
        Marker::StartDeleverage,
        Marker::EndDeleverage,
        Marker::LendingPoolHandleBankruptcy,
    ];
    /// Program entrypoint name in the pinned source (also the report filter value).
    pub fn name(self) -> &'static str {
        match self {
            Self::LendingAccountLiquidate => "lending_account_liquidate",
            Self::StartLiquidation => "start_liquidation",
            Self::EndLiquidation => "end_liquidation",
            Self::StartDeleverage => "start_deleverage",
            Self::EndDeleverage => "end_deleverage",
            Self::LendingPoolHandleBankruptcy => "lending_pool_handle_bankruptcy",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }
    /// The liquidation class this marker belongs to (a start marker included).
    pub fn liquidation_type(self) -> LiquidationType {
        match self {
            Self::LendingAccountLiquidate => LiquidationType::Classic,
            Self::StartLiquidation | Self::EndLiquidation => LiquidationType::Receivership,
            Self::StartDeleverage | Self::EndDeleverage => LiquidationType::ForcedDeleverage,
            Self::LendingPoolHandleBankruptcy => LiquidationType::BankruptcySettlement,
        }
    }
    pub fn is_start(self) -> bool {
        matches!(self, Self::StartLiquidation | Self::StartDeleverage)
    }
    /// `(target, actor)` positions in the pinned Accounts struct.
    fn roles(self) -> (usize, usize) {
        match self {
            Self::LendingAccountLiquidate => (5, 4),
            Self::LendingPoolHandleBankruptcy => (3, 1),
            Self::StartLiquidation
            | Self::EndLiquidation
            | Self::StartDeleverage
            | Self::EndDeleverage => (0, 3),
        }
    }
    /// Anchor rule `sha256("global:<name>")[..8]`.
    pub fn discriminator(self) -> [u8; 8] {
        solana_sdk::hash::hash(format!("global:{}", self.name()).as_bytes()).to_bytes()[..8]
            .try_into()
            .expect("8 bytes")
    }
    fn of(data: &[u8]) -> Option<Self> {
        use std::sync::OnceLock;
        static TABLE: OnceLock<[([u8; 8], Marker); 6]> = OnceLock::new();
        let d = data.get(..8)?;
        TABLE
            .get_or_init(|| Marker::ALL.map(|m| (m.discriminator(), m)))
            .iter()
            .find(|(disc, _)| disc == d)
            .map(|(_, m)| *m)
    }
}

/// `(outer instruction index, CPI index)`; `None` = top level.
pub type Position = (u16, Option<u16>);

/// One marker instruction of a transaction, in execution order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkerHit {
    pub marker: Marker,
    pub position: Position,
    pub target: Pubkey,
    pub actor: Pubkey,
}

/// Every marker instruction of `tx` (top level and CPI, execution order). Fails closed when the
/// CPI groups or an account index are missing, because absence could not be proven.
pub fn detect(tx: &ChainTx, program: &Pubkey) -> Result<Vec<MarkerHit>, &'static str> {
    let keys = resolve_account_keys(tx).map_err(|e| e.code())?;
    let inner = tx
        .meta
        .as_ref()
        .and_then(|m| m.inner_instructions.as_ref())
        .ok_or("INNER_INSTRUCTIONS_UNAVAILABLE")?;
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
            let Some(marker) = Marker::of(&ix.data) else {
                continue;
            };
            let (t, a) = marker.roles();
            let account = |i: usize| -> Result<Pubkey, &'static str> {
                let index = *ix.accounts.get(i).ok_or("MARKER_ACCOUNT_LAYOUT")?;
                keys.get(usize::from(index))
                    .copied()
                    .ok_or("MARKER_ACCOUNT_INDEX")
            };
            out.push(MarkerHit {
                marker,
                position: (
                    u16::try_from(outer).map_err(|_| "MARKER_POSITION")?,
                    inner_index
                        .map(u16::try_from)
                        .transpose()
                        .map_err(|_| "MARKER_POSITION")?,
                ),
                target: account(t)?,
                actor: account(a)?,
            });
        }
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairStatus {
    Complete,
    FailedTransaction,
    IncompleteOrAmbiguous,
}
impl PairStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "COMPLETE",
            Self::FailedTransaction => "FAILED_TRANSACTION",
            Self::IncompleteOrAmbiguous => "INCOMPLETE_OR_AMBIGUOUS",
        }
    }
}

/// Start/end evidence of one two-instruction path in one transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    pub liquidation_type: LiquidationType,
    pub status: PairStatus,
    pub starts: Vec<Position>,
    pub ends: Vec<Position>,
}

/// Pinned `33c6798` pairing: exactly one top-level start before exactly one end, same target and
/// actor, in a successful transaction. A missing start never erases an observed end, but it
/// cannot produce a complete pair; a failed transaction is never relabelled as a success.
pub fn pairs(hits: &[MarkerHit], status: &TxStatus) -> Vec<Pair> {
    let mut out = Vec::new();
    for (start, end, kind) in [
        (
            Marker::StartLiquidation,
            Marker::EndLiquidation,
            LiquidationType::Receivership,
        ),
        (
            Marker::StartDeleverage,
            Marker::EndDeleverage,
            LiquidationType::ForcedDeleverage,
        ),
    ] {
        let starts: Vec<(usize, &MarkerHit)> = hits
            .iter()
            .enumerate()
            .filter(|(_, h)| h.marker == start)
            .collect();
        let ends: Vec<(usize, &MarkerHit)> = hits
            .iter()
            .enumerate()
            .filter(|(_, h)| h.marker == end)
            .collect();
        if starts.is_empty() && ends.is_empty() {
            continue;
        }
        let valid = match (starts.as_slice(), ends.as_slice()) {
            ([(i, s)], [(j, e)]) => {
                i < j && s.target == e.target && s.actor == e.actor && s.position.1.is_none()
            }
            _ => false,
        };
        out.push(Pair {
            liquidation_type: kind,
            status: match status {
                TxStatus::Failed(_) => PairStatus::FailedTransaction,
                TxStatus::Success if valid => PairStatus::Complete,
                _ => PairStatus::IncompleteOrAmbiguous,
            },
            starts: starts.iter().map(|(_, h)| h.position).collect(),
            ends: ends.iter().map(|(_, h)| h.position).collect(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_solana::chain::{ChainInstruction, ChainMessage, ChainMeta, InnerInstructions};

    fn ix(program: u8, accounts: Vec<u8>, marker: Marker) -> ChainInstruction {
        let mut data = marker.discriminator().to_vec();
        data.extend_from_slice(&[0; 10]);
        ChainInstruction {
            program_id_index: program,
            accounts,
            data,
            stack_height: None,
        }
    }
    fn tx(keys: &[Pubkey], outer: Vec<ChainInstruction>, inner: Vec<InnerInstructions>) -> ChainTx {
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
                status: TxStatus::Success,
                fee: None,
                pre_balances: None,
                post_balances: None,
                pre_token_balances: None,
                post_token_balances: None,
                inner_instructions: Some(inner),
                log_messages: None,
                loaded_addresses: None,
                compute_units_consumed: None,
                cost_units: None,
            }),
        }
    }

    /// The discriminators are the pinned ones the canonical classifier and the audit use; the
    /// six names are unique and the four related instructions are not markers.
    #[test]
    fn discriminators_match_the_pinned_classifier_and_are_unique() {
        assert_eq!(
            Marker::LendingAccountLiquidate.discriminator(),
            [214, 169, 151, 213, 251, 167, 86, 219]
        );
        assert_eq!(
            Marker::EndDeleverage.discriminator(),
            [114, 14, 250, 143, 252, 104, 214, 209]
        );
        assert_eq!(
            Marker::LendingPoolHandleBankruptcy.discriminator(),
            [162, 11, 56, 139, 90, 128, 70, 173]
        );
        let all: std::collections::HashSet<_> =
            Marker::ALL.iter().map(|m| m.discriminator()).collect();
        assert_eq!(all.len(), 6);
        for k in super::super::related::RelatedKind::ALL {
            assert_eq!(Marker::of(&k.discriminator()), None, "{}", k.name());
        }
        for m in Marker::ALL {
            assert_eq!(Marker::parse(m.name()), Some(m));
        }
    }

    /// Start/end pairs: complete only for one top-level start before one end with the same
    /// target and actor; a CPI start, a lone end, swapped order or a failed TX never pair.
    #[test]
    fn start_end_pairs_follow_the_pinned_rules() {
        let k: Vec<Pubkey> = (0..8).map(|_| Pubkey::new_unique()).collect();
        let p = 7u8;
        let (target, actor) = (vec![0, 1, 2, 3], vec![0, 1, 2, 3]);
        let start = |acc: Vec<u8>| ix(p, acc, Marker::StartLiquidation);
        let end = |acc: Vec<u8>| ix(p, acc, Marker::EndLiquidation);
        let ok = tx(&k, vec![start(target.clone()), end(actor.clone())], vec![]);
        let hits = detect(&ok, &k[7]).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].target, hits[0].actor), (k[0], k[3]));
        let pair = &pairs(&hits, &TxStatus::Success)[0];
        assert_eq!(pair.liquidation_type, LiquidationType::Receivership);
        assert_eq!(pair.status, PairStatus::Complete);
        assert_eq!(
            (pair.starts.clone(), pair.ends.clone()),
            (vec![(0, None)], vec![(1, None)])
        );
        // Different target, swapped order, lone end, CPI start, failed transaction.
        let other = tx(
            &k,
            vec![start(vec![4, 1, 2, 3]), end(target.clone())],
            vec![],
        );
        let swapped = tx(&k, vec![end(target.clone()), start(target.clone())], vec![]);
        let lone = tx(&k, vec![end(target.clone())], vec![]);
        let cpi = tx(
            &k,
            vec![
                ix(p, vec![0, 1, 2, 3, 4, 5], Marker::LendingAccountLiquidate),
                end(target.clone()),
            ],
            vec![InnerInstructions {
                index: 0,
                instructions: vec![start(target.clone())],
            }],
        );
        for t in [&other, &swapped, &lone, &cpi] {
            let hits = detect(t, &k[7]).unwrap();
            let ps = pairs(&hits, &TxStatus::Success);
            assert_eq!(ps.len(), 1);
            assert_eq!(ps[0].status, PairStatus::IncompleteOrAmbiguous);
        }
        let failed = TxStatus::Failed(solana_sdk::transaction::TransactionError::AccountInUse);
        assert_eq!(
            pairs(&detect(&ok, &k[7]).unwrap(), &failed)[0].status,
            PairStatus::FailedTransaction
        );
        // Single-instruction completions form no pair; their class is never changed.
        let classic = tx(
            &k,
            vec![ix(
                p,
                vec![0, 1, 2, 3, 4, 5],
                Marker::LendingAccountLiquidate,
            )],
            vec![],
        );
        let hits = detect(&classic, &k[7]).unwrap();
        assert!(pairs(&hits, &TxStatus::Success).is_empty());
        assert_eq!(hits[0].marker.liquidation_type(), LiquidationType::Classic);
        assert_eq!((hits[0].target, hits[0].actor), (k[5], k[4]));
        // Missing CPI groups or a short account list fail closed.
        let mut blind = ok.clone();
        blind.meta.as_mut().unwrap().inner_instructions = None;
        assert_eq!(detect(&blind, &k[7]), Err("INNER_INSTRUCTIONS_UNAVAILABLE"));
        let short = tx(&k, vec![start(vec![0, 1])], vec![]);
        assert_eq!(detect(&short, &k[7]), Err("MARKER_ACCOUNT_LAYOUT"));
    }
}
