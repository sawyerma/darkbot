//! Source-derived liquidation classification for finalized transactions.
//! Layout authority: marginfi-v2 33c67987a661f59c6683a65485ebb4f0ef030b77,
//! events.rs and liquidate.rs/liquidate_end.rs/handle_bankruptcy.rs.
use super::*;
use crate::winner::{resolve_account_keys, ChainInstruction, ChainTx, TxStatus};
use serde::{Deserialize, Serialize};

const CLASSIC_IX: [u8; 8] = [214, 169, 151, 213, 251, 167, 86, 219];
const DELEVERAGE_IX: [u8; 8] = [114, 14, 250, 143, 252, 104, 214, 209];
const BANKRUPTCY_IX: [u8; 8] = [162, 11, 56, 139, 90, 128, 70, 173];
const CLASSIC_EVENT: [u8; 8] = [166, 160, 249, 154, 183, 39, 23, 242];
const DELEVERAGE_EVENT: [u8; 8] = [161, 8, 108, 204, 209, 198, 12, 30];
const BANKRUPTCY_EVENT: [u8; 8] = [166, 77, 41, 140, 36, 94, 10, 57];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LiquidationType {
    Classic,
    Receivership,
    ForcedDeleverage,
    BankruptcySettlement,
}
impl LiquidationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "CLASSIC",
            Self::Receivership => "RECEIVERSHIP",
            Self::ForcedDeleverage => "FORCED_DELEVERAGE",
            Self::BankruptcySettlement => "BANKRUPTCY_SETTLEMENT",
        }
    }
}

/// Native protocol amounts of one event, exactly as logged (f64 in the event layout). The
/// serialized keys are the report contract; `provenance`/`scope`/`units` state the limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NativeEvidence {
    Classic {
        provenance: String,
        pre_balances: [f64; 5],
        post_balances: [f64; 5],
        pre_health: f64,
        post_health: f64,
        liquidator_liability_cost_native_f64: f64,
        target_debt_reduction_native_f64: f64,
        insurance_component_native_f64: f64,
        asset_decimals: Option<u8>,
        liability_decimals: Option<u8>,
        historical_asset_price: Option<f64>,
        historical_liability_price: Option<f64>,
    },
    Bankruptcy {
        provenance: String,
        bad_debt_native_f64: f64,
        covered_native_f64: f64,
        socialized_native_f64: f64,
        liability_decimals: Option<u8>,
    },
    Flows {
        scope: String,
        flows: Vec<NativeFlow>,
        units: String,
    },
}

/// A visible System transfer in the transaction (context only: never attributed as a cost).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisiblePayment {
    pub from: Option<String>,
    pub to: Option<String>,
    pub lamports: u64,
    pub cost_attribution: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TruthEvent {
    pub signature: String,
    pub slot: u64,
    pub transaction_index: u32,
    pub event_index: u32,
    pub liquidation_type: LiquidationType,
    pub target: String,
    pub liquidator: String,
    pub liquidator_marginfi_account: Option<String>,
    pub asset_bank: Option<String>,
    pub liability_bank: Option<String>,
    pub asset_mint: Option<String>,
    pub liability_mint: Option<String>,
    pub seized_asset_native_exact: Option<String>,
    pub native_evidence: Option<NativeEvidence>,
    pub seized_value_usd: Option<f64>,
    pub repaid_value_usd: Option<f64>,
    pub gross_edge_usd: Option<f64>,
    pub protocol_fee_lamports: Option<u64>,
    pub transaction_fee_lamports: Option<u64>,
    pub fee_payer: Option<String>,
    pub visible_payments: Option<Vec<VisiblePayment>>,
    pub incremental_rent_lamports: Option<u64>,
    pub priority_fee_lamports: Option<u64>,
    pub winner_net_edge_usd: Option<f64>,
    pub winner_net_edge_complete: bool,
    #[serde(skip_deserializing)]
    pub economic_provenance: &'static str,
    pub block_time: Option<i64>,
    pub live_t_rx_ns: Option<u64>,
    #[serde(skip_deserializing)]
    pub discovery_source: &'static str,
    pub permissionless_profit_opportunity: bool,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], &'static str> {
        let end = self.pos.checked_add(N).ok_or("EVENT_LENGTH")?;
        let out = self
            .bytes
            .get(self.pos..end)
            .ok_or("EVENT_LENGTH")?
            .try_into()
            .map_err(|_| "EVENT_LENGTH")?;
        self.pos = end;
        Ok(out)
    }
    fn key(&mut self) -> Result<String, &'static str> {
        Ok(Pubkey::new_from_array(self.take()?).to_string())
    }
    fn number(&mut self) -> Result<f64, &'static str> {
        let n = f64::from_le_bytes(self.take()?);
        if n.is_finite() {
            Ok(n)
        } else {
            Err("EVENT_NONFINITE")
        }
    }
    fn header(&mut self) -> Result<(Option<String>, String), &'static str> {
        let signer = match self.take::<1>()?[0] {
            0 => None,
            1 => Some(self.key()?),
            _ => return Err("EVENT_OPTION_TAG"),
        };
        let account = self.key()?;
        self.key()?;
        self.key()?; // authority and group retained in raw payload
        Ok((signer, account))
    }
}
#[derive(Clone, Debug)]
struct Decoded {
    kind: LiquidationType,
    target: String,
    actor: String,
    liquidator_account: Option<String>,
    asset_bank: Option<String>,
    liability_bank: Option<String>,
    asset_mint: Option<String>,
    liability_mint: Option<String>,
    native: Option<NativeEvidence>,
    seized: Option<f64>,
    repaid: Option<f64>,
    flat_fee: Option<u64>,
}

/// Native event units are deliberately preserved: e.g. Kamino logs collateral
/// units while JupLend logs underlying. Never convert these using today's decimals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeFlow {
    pub kind: String,
    pub target: String,
    pub bank: String,
    pub mint: String,
    pub amount: String,
}
fn native_flow(bytes: &[u8]) -> Result<Option<NativeFlow>, &'static str> {
    let kind = match bytes.get(..8) {
        Some([3, 220, 148, 243, 33, 249, 54, 88]) => "WITHDRAW",
        Some([16, 220, 55, 111, 7, 80, 16, 25]) => "REPAY",
        _ => return Ok(None),
    };
    let mut c = Cursor { bytes, pos: 8 };
    let (_, target) = c.header()?;
    let bank = c.key()?;
    let mint = c.key()?;
    let amount = u64::from_le_bytes(c.take()?).to_string();
    if c.take::<1>()?[0] > 1 {
        return Err("NATIVE_FLOW_BOOL");
    }
    c.take::<16>()?; // WrappedI80F48 share_amount
    if c.pos != bytes.len() {
        return Err("NATIVE_FLOW_LAYOUT");
    }
    Ok(Some(NativeFlow {
        kind: kind.into(),
        target,
        bank,
        mint,
        amount,
    }))
}
fn decode_payload(bytes: &[u8]) -> Result<Option<Decoded>, &'static str> {
    let Some(discriminator) = bytes.get(..8) else {
        return Err("EVENT_DISCRIMINATOR_MISSING");
    };
    let mut c = Cursor { bytes, pos: 8 };
    let mut e = Decoded {
        kind: LiquidationType::Classic,
        target: String::new(),
        actor: String::new(),
        liquidator_account: None,
        asset_bank: None,
        liability_bank: None,
        asset_mint: None,
        liability_mint: None,
        native: None,
        seized: None,
        repaid: None,
        flat_fee: None,
    };
    if discriminator == CLASSIC_EVENT {
        let (signer, account) = c.header()?;
        e.actor = signer.ok_or("CLASSIC_SIGNER_MISSING")?;
        e.liquidator_account = Some(account);
        e.target = c.key()?;
        c.key()?;
        e.asset_bank = Some(c.key()?);
        e.asset_mint = Some(c.key()?);
        e.liability_bank = Some(c.key()?);
        e.liability_mint = Some(c.key()?);
        let pre_health = c.number()?;
        let post_health = c.number()?;
        let mut pre = [0.; 5];
        let mut post = [0.; 5];
        for n in &mut pre {
            *n = c.number()?;
        }
        for n in &mut post {
            *n = c.number()?;
        }
        // The liquidator can consume an existing deposit in the liability bank before
        // assuming debt. Target repayment excludes insurance; using it alone overstates edge.
        let assumed = post[3] - pre[3] + pre[4] - post[4];
        let target_repaid = pre[1] - post[1];
        if !assumed.is_finite() || !target_repaid.is_finite() {
            return Err("ECONOMIC_OVERFLOW");
        }
        e.native = Some(NativeEvidence::Classic {
            provenance: "LendingAccountLiquidateEvent f64 native balances; NOT exact integers"
                .into(),
            pre_balances: pre,
            post_balances: post,
            pre_health,
            post_health,
            liquidator_liability_cost_native_f64: assumed,
            target_debt_reduction_native_f64: target_repaid,
            insurance_component_native_f64: assumed - target_repaid,
            asset_decimals: None,
            liability_decimals: None,
            historical_asset_price: None,
            historical_liability_price: None,
        });
    } else if discriminator == DELEVERAGE_EVENT {
        e.kind = LiquidationType::ForcedDeleverage;
        e.target = c.key()?;
        e.actor = c.key()?;
        e.seized = Some(c.number()?);
        e.repaid = Some(c.number()?);
        e.flat_fee = Some(0); // end_deleverage source explicitly takes no protocol SOL fee.
    } else if discriminator == BANKRUPTCY_EVENT {
        e.kind = LiquidationType::BankruptcySettlement;
        let (signer, account) = c.header()?;
        e.actor = signer.ok_or("BANKRUPTCY_SIGNER_MISSING")?;
        e.target = account;
        e.liability_bank = Some(c.key()?);
        e.liability_mint = Some(c.key()?);
        e.native = Some(NativeEvidence::Bankruptcy {
            provenance:
                "LendingPoolBankHandleBankruptcyEvent f64 native amounts; NOT liquidator profit"
                    .into(),
            bad_debt_native_f64: c.number()?,
            covered_native_f64: c.number()?,
            socialized_native_f64: c.number()?,
            liability_decimals: None,
        });
    } else {
        return Ok(None);
    }
    if c.pos != bytes.len() {
        return Err("EVENT_LAYOUT_TRAILING_BYTES");
    }
    Ok(Some(e))
}

/// Provisional event identified from the authenticated Marginfi program log.
#[derive(Clone, Debug, PartialEq)]
pub struct ProvisionalLogEvent {
    pub kind: LiquidationType,
    pub target: String,
    pub actor: String,
    pub gross_edge_usd: Option<f64>,
}

/// Reuse the audited canonical event payload decoder for a WSS-only read.
/// All-path instruction/CPI consistency still belongs to final reconciliation.
pub(super) fn provisional_event(bytes: &[u8]) -> Result<Option<ProvisionalLogEvent>, &'static str> {
    let discriminator = bytes.get(..8);
    if discriminator != Some(CLASSIC_EVENT.as_slice())
        && discriminator != Some(DELEVERAGE_EVENT.as_slice())
        && discriminator != Some(BANKRUPTCY_EVENT.as_slice())
    {
        return Ok(None);
    }
    let event = decode_payload(bytes)?.ok_or("PROVISIONAL_KNOWN_EVENT_UNDECODED")?;
    let gross_edge_usd = event
        .seized
        .zip(event.repaid)
        .map(|(seized, repaid)| seized - repaid);
    if gross_edge_usd.is_some_and(|value| !value.is_finite()) {
        return Err("PROVISIONAL_NONFINITE_EDGE");
    }
    Ok(Some(ProvisionalLogEvent {
        kind: event.kind,
        target: event.target,
        actor: event.actor,
        gross_edge_usd,
    }))
}

#[derive(Debug)]
struct InstructionEvidence {
    kind: LiquidationType,
    target: String,
    actor: String,
    asset_bank: Option<String>,
    liability_bank: Option<String>,
    liquidator_account: Option<String>,
    asset_amount: Option<u64>,
}
fn instructions(tx: &ChainTx, program: &Pubkey) -> Result<Vec<InstructionEvidence>, &'static str> {
    let keys = resolve_account_keys(tx).map_err(|e| e.code())?;
    let mut all: Vec<(bool, &ChainInstruction)> = Vec::new();
    // Interleave each outer instruction with its CPI group in execution order.
    // Otherwise equal target/bank pairs in nested and later outer liquidations
    // could receive each other's exact instruction amount.
    let inner = tx
        .meta
        .as_ref()
        .and_then(|m| m.inner_instructions.as_ref())
        .ok_or("INNER_INSTRUCTIONS_UNAVAILABLE")?;
    for (index, ix) in tx.message.instructions.iter().enumerate() {
        all.push((false, ix));
        if let Some(group) = inner.iter().find(|g| usize::from(g.index) == index) {
            all.extend(group.instructions.iter().map(|ix| (true, ix)));
        }
    }
    let mut out = Vec::new();
    for (inner, ix) in all {
        if keys.get(usize::from(ix.program_id_index)) != Some(program) {
            continue;
        }
        let b = &ix.data;
        let Some(d) = b.get(..8) else {
            return Err("MARGINFI_INSTRUCTION_DISCRIMINATOR");
        };
        if !paths::instruction_known(d) {
            return Err("UNAUDITED_MARGINFI_INSTRUCTION");
        }
        let key = |i: usize| -> Result<String, &'static str> {
            keys.get(usize::from(
                *ix.accounts.get(i).ok_or("LIQUIDATION_ACCOUNT_LAYOUT")?,
            ))
            .map(ToString::to_string)
            .ok_or("LIQUIDATION_ACCOUNT_INDEX")
        };
        if d == CLASSIC_IX {
            if b.len() != 18 {
                return Err("CLASSIC_INSTRUCTION_LAYOUT");
            }
            out.push(InstructionEvidence {
                kind: LiquidationType::Classic,
                target: key(5)?,
                actor: key(4)?,
                asset_bank: Some(key(1)?),
                liability_bank: Some(key(2)?),
                liquidator_account: Some(key(3)?),
                asset_amount: Some(u64::from_le_bytes(b[8..16].try_into().unwrap())),
            });
        } else if d == DELEVERAGE_IX {
            if inner {
                return Err("DELEVERAGE_CPI_FORBIDDEN_BY_SOURCE");
            }
            out.push(InstructionEvidence {
                kind: LiquidationType::ForcedDeleverage,
                target: key(0)?,
                actor: key(3)?,
                asset_bank: None,
                liability_bank: None,
                liquidator_account: None,
                asset_amount: None,
            });
        } else if d == BANKRUPTCY_IX {
            out.push(InstructionEvidence {
                kind: LiquidationType::BankruptcySettlement,
                target: key(3)?,
                actor: key(1)?,
                asset_bank: None,
                liability_bank: Some(key(2)?),
                liquidator_account: None,
                asset_amount: None,
            });
        }
    }
    Ok(out)
}

/// Decode every audited completion path. Receivership is decoded and qualified by
/// the original decoder + F8 implementation rather than a second interpretation.
pub(super) fn classify(
    tx: &ChainTx,
    program: &Pubkey,
    block_time: Option<i64>,
    live: Option<u64>,
) -> Result<Vec<TruthEvent>, &'static str> {
    reconcile::validate_full_transaction(tx)?;
    match tx.status() {
        TxStatus::Success => {}
        TxStatus::Failed(_) => return Ok(vec![]),
        TxStatus::Unknown(code) => return Err(code),
    }
    let logs: Vec<String> = tx
        .meta
        .as_ref()
        .and_then(|m| m.log_messages.clone())
        .ok_or("LOG_MESSAGES_MISSING")?;
    let receiver = decode_receiver_events(&logs, program);
    if receiver.decoder_errors != 0 {
        return Err("RECEIVER_DECODER_ERROR");
    }
    let pairs = receiver
        .events
        .iter()
        .map(|e| (e.target, e.receiver))
        .collect::<Vec<_>>();
    match qualify_truncated(&tx, &tx.signatures[0], tx.slot, program, &pairs) {
        TruncationQualification::Qualified { .. } => {}
        TruncationQualification::Mismatch { .. } => {
            return Err("RECEIVER_INSTRUCTION_EVENT_MISMATCH")
        }
        TruncationQualification::Unqualified(code) => return Err(code),
    }
    let mut ixs = instructions(tx, program)?;
    if receiver.truncated && !ixs.is_empty() {
        // F8 proves only top-level receivership. A truncated CPI parent could
        // conceal rollback of Classic/settlement logs even if the outer TX succeeds.
        return Err("ADDITIONAL_PATH_TRUNCATION_UNQUALIFIED");
    }
    let mut decoded: Vec<(usize, Decoded, Option<u64>)> = Vec::new();
    let mut receiver_positions = Vec::new();
    let mut flows = Vec::new();
    let program_s = program.to_string();
    let mut stack: Vec<&str> = Vec::new();
    for (log_index, line) in logs.iter().enumerate() {
        if line.starts_with("Log truncated") {
            break;
        }
        if let Some(encoded) = line.strip_prefix(PROGRAM_DATA_PREFIX) {
            if stack.last().copied() != Some(program_s.as_str()) {
                continue;
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|_| "EVENT_BASE64")?;
            if bytes.get(..8) == Some(&LIQUIDATION_RECEIVER_EVENT_DISCRIMINATOR) {
                receiver_positions.push(log_index);
            }
            if let Some(flow) = native_flow(&bytes)? {
                flows.push(flow);
            }
            if let Some(ev) = decode_payload(&bytes)? {
                let pos = ixs
                    .iter()
                    .position(|ix| {
                        ix.kind == ev.kind
                            && ix.target == ev.target
                            && ix.actor == ev.actor
                            && ix.asset_bank == ev.asset_bank
                            && ix.liability_bank == ev.liability_bank
                            && ix.liquidator_account == ev.liquidator_account
                    })
                    .ok_or("LIQUIDATION_INSTRUCTION_EVENT_MISMATCH")?;
                let ix = ixs.remove(pos);
                decoded.push((log_index, ev, ix.asset_amount));
            }
        } else if let Some(rest) = line.strip_prefix("Program ") {
            let mut parts = rest.split_whitespace();
            if let (Some(id), Some(word)) = (parts.next(), parts.next()) {
                if word == "invoke" {
                    stack.push(id);
                } else if word == "success" || word.starts_with("failed") {
                    if word.starts_with("failed") && stack.contains(&program_s.as_str()) {
                        return Err("RELEVANT_CPI_FAILED");
                    }
                    if stack.last().copied() == Some(id) {
                        stack.pop();
                    }
                }
            }
        }
    }
    if !ixs.is_empty() {
        return Err("LIQUIDATION_INSTRUCTION_EVENT_MISMATCH");
    }
    if receiver_positions.len() != receiver.events.len() {
        return Err("RECEIVER_EVENT_ORDER_AMBIGUOUS");
    }
    for (position, ev) in receiver_positions.into_iter().zip(receiver.events) {
        let native: Vec<_> = flows
            .iter()
            .filter(|f| f.target == ev.target.to_string())
            .cloned()
            .collect();
        let unique = |kind| {
            let pairs: BTreeSet<_> = native
                .iter()
                .filter(|f| f.kind == kind)
                .map(|f| (f.bank.clone(), f.mint.clone()))
                .collect();
            (pairs.len() == 1).then(|| pairs.into_iter().next().unwrap())
        };
        let asset = unique("WITHDRAW");
        let liability = unique("REPAY");
        decoded.push((
            position,
            Decoded {
                kind: LiquidationType::Receivership,
                target: ev.target.to_string(),
                actor: ev.receiver.to_string(),
                liquidator_account: None,
                asset_bank: asset.as_ref().map(|p| p.0.clone()),
                liability_bank: liability.as_ref().map(|p| p.0.clone()),
                asset_mint: asset.map(|p| p.1),
                liability_mint: liability.map(|p| p.1),
                native: Some(NativeEvidence::Flows {
                    scope: "TRANSACTION_TARGET_FLOWS; not per-event allocation".into(),
                    flows: native,
                    units: "pinned protocol event amount (venue-dependent); no guessed decimals"
                        .into(),
                }),
                seized: Some(ev.seized_equity_usd),
                repaid: Some(ev.repaid_equity_usd),
                flat_fee: Some(u64::from(ev.protocol_flat_fee_lamports)),
            },
            None,
        ));
    }
    // Stable index in typed event order, never a signature-only identity. Receiver
    // indices remain in their original relative order (existing F8 semantics).
    let fee = tx.meta.as_ref().and_then(|m| m.fee);
    let enrichment = enrich(tx, program);
    let priority = enrichment.priority_fee_lamports;
    decoded.sort_by_key(|(position, _, _)| *position);
    decoded.into_iter().enumerate().map(|(index,(_,ev,amount))| {
        let edge=ev.seized.zip(ev.repaid).map(|(s,r)|s-r);
        if edge.is_some_and(|n|!n.is_finite()) {return Err("ECONOMIC_OVERFLOW");}
        Ok(TruthEvent{signature:tx.signatures[0].clone(),slot:tx.slot,
            transaction_index:u32::try_from(tx.transaction_index.ok_or("TRANSACTION_INDEX_MISSING")?).map_err(|_|"TRANSACTION_INDEX_OVERFLOW")?,
            event_index:index as u32,liquidation_type:ev.kind,target:ev.target,liquidator:ev.actor,
            liquidator_marginfi_account:ev.liquidator_account,asset_bank:ev.asset_bank,liability_bank:ev.liability_bank,
            asset_mint:ev.asset_mint,liability_mint:ev.liability_mint,seized_asset_native_exact:amount.map(|n|n.to_string()),
            native_evidence:ev.native,seized_value_usd:ev.seized,repaid_value_usd:ev.repaid,gross_edge_usd:edge,
            protocol_fee_lamports:ev.flat_fee,transaction_fee_lamports:fee,priority_fee_lamports:priority,
            fee_payer:enrichment.fee_payer.clone(),visible_payments:visible_payments(tx),incremental_rent_lamports:None,
            winner_net_edge_usd:None,winner_net_edge_complete:false,
            economic_provenance:match ev.kind {LiquidationType::Classic=>"NATIVE_INSTRUCTION_AND_EVENT; SLOT_PRICE_AND_DECIMALS_UNKNOWN; LIABILITY_COST_INCLUDES_INSURANCE",
                LiquidationType::Receivership|LiquidationType::ForcedDeleverage=>"CACHED_PROTOCOL_EQUITY_USD: SEIZED_MINUS_REPAID; SOL_USD_AND_EXTERNAL_COSTS_UNKNOWN",
                LiquidationType::BankruptcySettlement=>"DEBT_SETTLEMENT; NOT_LIQUIDATOR_PROFIT"},block_time,live_t_rx_ns:live,
            discovery_source:if live.is_some(){"BOTH"}else{"RECONCILIATION"},
            permissionless_profit_opportunity:matches!(ev.kind,LiquidationType::Classic|LiquidationType::Receivership)})
    }).collect()
}

fn visible_payments(tx: &ChainTx) -> Option<Vec<VisiblePayment>> {
    // Transaction context only: a visible transfer is not necessarily a tip or
    // an incremental cost to the liquidator. Do not subtract it without attribution.
    let keys = tx.account_keys().ok()?;
    let system = Pubkey::default();
    let inner = tx
        .meta
        .as_ref()
        .and_then(|m| m.inner_instructions.as_ref())
        .into_iter()
        .flatten()
        .flat_map(|g| &g.instructions);
    Some(
        tx.message
            .instructions
            .iter()
            .chain(inner)
            .filter(|ix| keys.get(usize::from(ix.program_id_index)) == Some(&system))
            .filter(|ix| ix.data.len() == 12 && ix.data[..4] == [2, 0, 0, 0])
            .map(|ix| {
                let at = |i: usize| {
                    ix.accounts
                        .get(i)
                        .and_then(|a| keys.get(usize::from(*a)))
                        .map(ToString::to_string)
                };
                VisiblePayment {
                    from: at(0),
                    to: at(1),
                    lamports: u64::from_le_bytes(ix.data[4..12].try_into().expect("len")),
                    cost_attribution: "UNKNOWN".into(),
                }
            })
            .collect(),
    )
}

#[derive(Default)]
pub(super) struct TruthLedger {
    pub events: Vec<TruthEvent>,
    pub signatures: HashSet<String>,
    pub duplicate_transactions: u64,
    pub retention_error: bool,
    pub live_receipts: HashMap<String, (u64, u64)>,
}
impl TruthLedger {
    pub fn summary(&self, chain_seconds: Option<f64>) -> Value {
        let groups = [
            ("ALL", None),
            ("CLASSIC", Some(LiquidationType::Classic)),
            ("RECEIVERSHIP", Some(LiquidationType::Receivership)),
            ("FORCED_DELEVERAGE", Some(LiquidationType::ForcedDeleverage)),
            (
                "BANKRUPTCY_SETTLEMENT",
                Some(LiquidationType::BankruptcySettlement),
            ),
        ];
        let mut out = serde_json::Map::new();
        for (label, kind) in groups {
            let events: Vec<_> = self
                .events
                .iter()
                .filter(|e| kind.is_none_or(|k| k == e.liquidation_type))
                .collect();
            let txs: HashSet<_> = events.iter().map(|e| &e.signature).collect();
            let mut gross = events
                .iter()
                .filter_map(|e| e.gross_edge_usd)
                .collect::<Vec<_>>();
            gross.sort_by(f64::total_cmp);
            let mut actors: BTreeMap<&str, (u64, f64)> = BTreeMap::new();
            for e in &events {
                let a = actors.entry(&e.liquidator).or_default();
                a.0 += 1;
                if let Some(edge) = e.gross_edge_usd {
                    a.1 += edge;
                }
            }
            let total = if gross.is_empty() {
                None
            } else {
                Some(gross.iter().sum::<f64>())
            };
            out.insert(label.into(),json!({"liquidation_transactions":txs.len(),"liquidation_events":events.len(),
                "events_per_chain_hour":chain_seconds.filter(|s|*s>0.).map(|s|events.len() as f64*3600./s),
                "gross_edge_usd":{"count_known":gross.len(),"count_unknown":events.len()-gross.len(),"known_total":total,
                    "average":total.map(|t|t/gross.len() as f64),"median":quantile_nearest_rank(&gross,0.5),"p90":quantile_nearest_rank(&gross,0.9),
                    "p99":quantile_nearest_rank(&gross,0.99),"min":gross.first(),"max":gross.last()},
                "winner_net_edge_usd":{"count_known":0,"count_unknown":events.len(),"known_total":null},
                "liquidators":actors,"live_and_reconciled_events":events.iter().filter(|e|e.live_t_rx_ns.is_some()).count(),
                "reconciliation_only_events":events.iter().filter(|e|e.live_t_rx_ns.is_none()).count()}));
        }
        json!({"groups":out,"duplicate_transactions":self.duplicate_transactions,"retention_error":self.retention_error,
            "scope":"FINALIZED_CANONICAL_SLOT_WINDOW; includes administrative debt settlement as a separate class",
            "transaction_fee_semantics":"meta.fee includes priority; repeated on events as transaction context, never summed per event",
            "full_master_economics_and_shadow_join_qualified":false})
    }
}
