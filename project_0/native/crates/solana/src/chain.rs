//! JFI-02 wire boundary: the one native model of Solana transactions and blocks as delivered by
//! JSON-RPC (`getTransaction`, `getBlock` entries) and WSS (`transactionNotification`).
//!
//! JSON exists only in the private `Wire*` structs of this module; everything public is typed
//! and validated: keys are `Pubkey`s, instruction data is decoded bytes, token amounts are exact
//! atomic units, the status keeps the protocol distinction between `meta.err: null` (success), a
//! concrete error (failed), an absent `err` or absent `meta` (UNKNOWN) and an undecodable error
//! value (UNKNOWN, never success). A block keeps every entry: an entry that cannot be decoded
//! stays a visible error at its index instead of disappearing.
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::TransactionError;
use solana_transaction_status_client_types::UiTransactionError;
use std::str::FromStr;

/// Bounded reason code of a decode or resolution failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainError(pub &'static str);

impl std::fmt::Display for ChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Version of this decoder's semantics (F10): every change of a decoded value or reason bumps it,
/// so a typed answer cached by another decoder version is refused instead of reused. The golden
/// test `decoded_block_is_pinned_to_the_decoder_version` forces the bump.
pub const WIRE_DECODER_VERSION: u16 = 1;

/// The closed set of reasons this module's JSON-RPC/WSS decoder produces; a stored `ChainError`
/// maps back onto it or fails to decode, so a typed cache never invents a reason.
pub const DECODER_CODES: [&str; 14] = [
    "BLOCK_DECODE",
    "BLOCK_HEADER_INVALID",
    "BLOCK_TRANSACTIONS_MISSING",
    "INSTRUCTION_DATA_INVALID",
    "INVALID_KEY",
    "LOADED_ADDRESSES_MISSING",
    "LOGS_NOTIFICATION_DECODE",
    "NOTIFICATION_DECODE",
    "NOTIFICATION_SIGNATURE_MISMATCH",
    "TOKEN_AMOUNT_INVALID",
    "TX_DECODE",
    "TX_SLOT_MISMATCH",
    "TX_SLOT_MISSING",
    "TX_VERSION_UNSUPPORTED",
];

impl serde::Serialize for ChainError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for ChainError {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = String::deserialize(d)?;
        DECODER_CODES
            .into_iter()
            .find(|c| *c == code)
            .map(ChainError)
            .ok_or_else(|| serde::de::Error::custom("unknown ChainError code"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TxWireVersion {
    Legacy,
    Number(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxStatus {
    /// `meta.err` present and `null`.
    Success,
    /// `meta.err` present with a decodable error.
    Failed(TransactionError),
    /// No reliable status: absent `meta`, absent `err` or an undecodable error value.
    Unknown(&'static str),
}

impl TxStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, TxStatus::Success)
    }
}

/// The closed set of UNKNOWN reasons; a stored status maps back onto it or fails to decode.
const UNKNOWN_CODES: [&str; 3] = [
    "TX_META_MISSING",
    "TX_META_STATUS_MISSING",
    "TX_ERROR_UNDECODABLE",
];

#[derive(serde::Serialize, serde::Deserialize)]
enum TxStatusStored {
    Success,
    Failed(TransactionError),
    Unknown(String),
}

impl serde::Serialize for TxStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            TxStatus::Success => TxStatusStored::Success,
            TxStatus::Failed(e) => TxStatusStored::Failed(e.clone()),
            TxStatus::Unknown(code) => TxStatusStored::Unknown((*code).to_string()),
        }
        .serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for TxStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match TxStatusStored::deserialize(d)? {
            TxStatusStored::Success => TxStatus::Success,
            TxStatusStored::Failed(e) => TxStatus::Failed(e),
            TxStatusStored::Unknown(code) => TxStatus::Unknown(
                UNKNOWN_CODES
                    .into_iter()
                    .find(|c| *c == code)
                    .ok_or_else(|| serde::de::Error::custom("unknown TxStatus code"))?,
            ),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainInstruction {
    pub program_id_index: u8,
    pub accounts: Vec<u8>,
    pub data: Vec<u8>,
    pub stack_height: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainLookup {
    pub account_key: Pubkey,
    pub writable_indexes: Vec<u8>,
    pub readonly_indexes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainMessage {
    /// `(num_required_signatures, num_readonly_signed, num_readonly_unsigned)`; `None` if absent.
    pub header: Option<(u8, u8, u8)>,
    pub account_keys: Vec<Pubkey>,
    pub recent_blockhash: Option<String>,
    pub instructions: Vec<ChainInstruction>,
    /// `None` when the field is absent (legacy), `Some(vec![])` when present and empty.
    pub address_table_lookups: Option<Vec<ChainLookup>>,
    /// v1 `transactionConfig`; `None` when absent (legacy, v0).
    pub transaction_config: Option<TransactionConfig>,
}

/// v1 message compute/priority configuration; each field `None` when not delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TransactionConfig {
    pub compute_unit_limit: Option<u32>,
    pub heap_size: Option<u32>,
    pub loaded_accounts_data_size_limit: Option<u32>,
    pub priority_fee: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InnerInstructions {
    pub index: u8,
    pub instructions: Vec<ChainInstruction>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TokenBalance {
    pub account_index: u8,
    pub mint: Pubkey,
    pub owner: Option<Pubkey>,
    pub program_id: Option<Pubkey>,
    /// Exact atomic units (`uiTokenAmount.amount`).
    pub amount: u64,
    pub decimals: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LoadedAddresses {
    pub writable: Vec<Pubkey>,
    pub readonly: Vec<Pubkey>,
}

/// Transaction status metadata. Every optional field is `None` exactly when the provider did
/// not deliver it; nothing is defaulted.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainMeta {
    pub status: TxStatus,
    pub fee: Option<u64>,
    pub pre_balances: Option<Vec<u64>>,
    pub post_balances: Option<Vec<u64>>,
    pub pre_token_balances: Option<Vec<TokenBalance>>,
    pub post_token_balances: Option<Vec<TokenBalance>>,
    pub inner_instructions: Option<Vec<InnerInstructions>>,
    pub log_messages: Option<Vec<String>>,
    pub loaded_addresses: Option<LoadedAddresses>,
    pub compute_units_consumed: Option<u64>,
    pub cost_units: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainTx {
    pub slot: u64,
    pub block_time: Option<i64>,
    /// Position in the block: the array index for `getBlock`, the provider field otherwise.
    pub transaction_index: Option<u64>,
    /// `None` when the provider omitted `version`.
    pub version: Option<TxWireVersion>,
    pub signatures: Vec<String>,
    pub message: ChainMessage,
    /// `None` when `meta` was absent or `null`.
    pub meta: Option<ChainMeta>,
}

impl ChainTx {
    pub fn signature(&self) -> Option<&str> {
        self.signatures.first().map(String::as_str)
    }

    pub fn status(&self) -> TxStatus {
        match &self.meta {
            Some(m) => m.status.clone(),
            None => TxStatus::Unknown("TX_META_MISSING"),
        }
    }

    pub fn version_label(&self) -> String {
        match self.version {
            Some(TxWireVersion::Legacy) => "legacy".into(),
            Some(TxWireVersion::Number(n)) => format!("v{n}"),
            None => "unknown".into(),
        }
    }

    /// Account keys in index order: static keys, then loaded writable, then loaded readonly.
    /// Lookups without loaded addresses cannot be resolved and fail closed.
    pub fn account_keys(&self) -> Result<Vec<Pubkey>, ChainError> {
        let mut keys = self.message.account_keys.clone();
        let has_lookups = self
            .message
            .address_table_lookups
            .as_ref()
            .is_some_and(|l| !l.is_empty());
        match self.meta.as_ref().and_then(|m| m.loaded_addresses.as_ref()) {
            Some(loaded) => {
                keys.extend(loaded.writable.iter().chain(&loaded.readonly).copied());
            }
            None if has_lookups => return Err(ChainError("LOADED_ADDRESSES_MISSING")),
            None => {}
        }
        Ok(keys)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainBlock {
    pub slot: u64,
    pub blockhash: String,
    /// `None` when the provider omitted it; required only where the parent chain is proven.
    pub previous_blockhash: Option<String>,
    pub parent_slot: u64,
    pub block_time: Option<i64>,
    pub block_height: Option<u64>,
    /// Every entry of `transactions` in block order; undecodable entries keep their reason.
    pub transactions: Vec<Result<ChainTx, ChainError>>,
}

// ---- wire (JSON) structs: private to this module ----

/// Distinguishes an absent field from a present one (including `null`).
#[derive(Debug, Default)]
enum Field<T> {
    #[default]
    Absent,
    Present(T),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Field::Present)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireErr {
    Known(UiTransactionError),
    Unknown(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireVersion {
    Number(u8),
    Text(String),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireInstruction {
    program_id_index: u8,
    accounts: Vec<u8>,
    data: String,
    #[serde(default)]
    stack_height: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLookup {
    account_key: String,
    writable_indexes: Vec<u8>,
    readonly_indexes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireHeader {
    num_required_signatures: u8,
    num_readonly_signed_accounts: u8,
    num_readonly_unsigned_accounts: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireMessage {
    #[serde(default)]
    header: Option<WireHeader>,
    account_keys: Vec<String>,
    #[serde(default)]
    recent_blockhash: Option<String>,
    instructions: Vec<WireInstruction>,
    #[serde(default)]
    address_table_lookups: Option<Vec<WireLookup>>,
    #[serde(default)]
    transaction_config: Option<WireTxConfig>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireTxConfig {
    #[serde(default)]
    compute_unit_limit: Option<u32>,
    #[serde(default)]
    heap_size: Option<u32>,
    #[serde(default)]
    loaded_accounts_data_size_limit: Option<u32>,
    #[serde(default)]
    priority_fee: Option<u64>,
}

#[derive(Deserialize)]
struct WireBody {
    signatures: Vec<String>,
    message: WireMessage,
}

#[derive(Deserialize)]
struct WireUiTokenAmount {
    amount: String,
    decimals: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireTokenBalance {
    account_index: u8,
    mint: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    program_id: Option<String>,
    ui_token_amount: WireUiTokenAmount,
}

#[derive(Deserialize)]
struct WireInner {
    index: u8,
    instructions: Vec<WireInstruction>,
}

#[derive(Deserialize)]
struct WireLoaded {
    writable: Vec<String>,
    readonly: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireMeta {
    #[serde(default)]
    err: Field<Option<WireErr>>,
    #[serde(default)]
    fee: Option<u64>,
    #[serde(default)]
    pre_balances: Option<Vec<u64>>,
    #[serde(default)]
    post_balances: Option<Vec<u64>>,
    #[serde(default)]
    pre_token_balances: Option<Vec<WireTokenBalance>>,
    #[serde(default)]
    post_token_balances: Option<Vec<WireTokenBalance>>,
    #[serde(default)]
    inner_instructions: Option<Vec<WireInner>>,
    #[serde(default)]
    log_messages: Option<Vec<String>>,
    #[serde(default)]
    loaded_addresses: Option<WireLoaded>,
    #[serde(default)]
    compute_units_consumed: Option<u64>,
    #[serde(default)]
    cost_units: Option<u64>,
}

/// `{transaction, meta, version}` plus the optional envelope fields of `getTransaction`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireTx {
    #[serde(default)]
    slot: Option<u64>,
    #[serde(default)]
    block_time: Option<i64>,
    #[serde(default)]
    transaction_index: Option<u64>,
    transaction: WireBody,
    #[serde(default)]
    meta: Option<WireMeta>,
    #[serde(default)]
    version: Field<Option<WireVersion>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireBlock {
    blockhash: String,
    #[serde(default)]
    previous_blockhash: Option<String>,
    parent_slot: u64,
    #[serde(default)]
    block_time: Option<i64>,
    #[serde(default)]
    block_height: Option<u64>,
    #[serde(default)]
    transactions: Option<Vec<serde_json::Value>>,
}

fn key(s: &str) -> Result<Pubkey, ChainError> {
    Pubkey::from_str(s).map_err(|_| ChainError("INVALID_KEY"))
}

fn instruction(w: WireInstruction) -> Result<ChainInstruction, ChainError> {
    Ok(ChainInstruction {
        program_id_index: w.program_id_index,
        accounts: w.accounts,
        data: bs58::decode(&w.data)
            .into_vec()
            .map_err(|_| ChainError("INSTRUCTION_DATA_INVALID"))?,
        stack_height: w.stack_height,
    })
}

fn token_balances(
    w: Option<Vec<WireTokenBalance>>,
) -> Result<Option<Vec<TokenBalance>>, ChainError> {
    w.map(|v| {
        v.into_iter()
            .map(|b| {
                Ok(TokenBalance {
                    account_index: b.account_index,
                    mint: key(&b.mint)?,
                    owner: b.owner.as_deref().map(key).transpose()?,
                    program_id: b.program_id.as_deref().map(key).transpose()?,
                    amount: b
                        .ui_token_amount
                        .amount
                        .parse()
                        .map_err(|_| ChainError("TOKEN_AMOUNT_INVALID"))?,
                    decimals: b.ui_token_amount.decimals,
                })
            })
            .collect()
    })
    .transpose()
}

fn meta(w: WireMeta) -> Result<ChainMeta, ChainError> {
    let status = status(w.err);
    let keys = |v: Vec<String>| v.iter().map(|s| key(s)).collect::<Result<Vec<_>, _>>();
    Ok(ChainMeta {
        status,
        fee: w.fee,
        pre_balances: w.pre_balances,
        post_balances: w.post_balances,
        pre_token_balances: token_balances(w.pre_token_balances)?,
        post_token_balances: token_balances(w.post_token_balances)?,
        inner_instructions: w
            .inner_instructions
            .map(|groups| {
                groups
                    .into_iter()
                    .map(|g| {
                        Ok(InnerInstructions {
                            index: g.index,
                            instructions: g
                                .instructions
                                .into_iter()
                                .map(instruction)
                                .collect::<Result<_, _>>()?,
                        })
                    })
                    .collect::<Result<Vec<_>, ChainError>>()
            })
            .transpose()?,
        log_messages: w.log_messages,
        loaded_addresses: w
            .loaded_addresses
            .map(|l| {
                Ok(LoadedAddresses {
                    writable: keys(l.writable)?,
                    readonly: keys(l.readonly)?,
                })
            })
            .transpose()?,
        compute_units_consumed: w.compute_units_consumed,
        cost_units: w.cost_units,
    })
}

fn tx(
    w: WireTx,
    slot: u64,
    index: Option<u64>,
    block_time: Option<i64>,
) -> Result<ChainTx, ChainError> {
    let version = match w.version {
        Field::Absent | Field::Present(None) => None,
        Field::Present(Some(WireVersion::Number(n))) => Some(TxWireVersion::Number(n)),
        Field::Present(Some(WireVersion::Text(t))) if t == "legacy" => Some(TxWireVersion::Legacy),
        Field::Present(Some(WireVersion::Text(_))) => {
            return Err(ChainError("TX_VERSION_UNSUPPORTED"))
        }
    };
    let m = w.transaction.message;
    Ok(ChainTx {
        slot,
        block_time,
        transaction_index: index,
        version,
        signatures: w.transaction.signatures,
        message: ChainMessage {
            header: m.header.map(|h| {
                (
                    h.num_required_signatures,
                    h.num_readonly_signed_accounts,
                    h.num_readonly_unsigned_accounts,
                )
            }),
            account_keys: m
                .account_keys
                .iter()
                .map(|s| key(s))
                .collect::<Result<_, _>>()?,
            recent_blockhash: m.recent_blockhash,
            instructions: m
                .instructions
                .into_iter()
                .map(instruction)
                .collect::<Result<_, _>>()?,
            address_table_lookups: m
                .address_table_lookups
                .map(|v| {
                    v.into_iter()
                        .map(|l| {
                            Ok(ChainLookup {
                                account_key: key(&l.account_key)?,
                                writable_indexes: l.writable_indexes,
                                readonly_indexes: l.readonly_indexes,
                            })
                        })
                        .collect::<Result<Vec<_>, ChainError>>()
                })
                .transpose()?,
            transaction_config: m.transaction_config.map(|c| TransactionConfig {
                compute_unit_limit: c.compute_unit_limit,
                heap_size: c.heap_size,
                loaded_accounts_data_size_limit: c.loaded_accounts_data_size_limit,
                priority_fee: c.priority_fee,
            }),
        },
        meta: w.meta.map(meta).transpose()?,
    })
}

/// `getTransaction` result (`encoding = "json"`): `{slot, blockTime, transaction, meta, version}`.
pub fn decode_transaction(value: &serde_json::Value) -> Result<ChainTx, ChainError> {
    let w = WireTx::deserialize(value).map_err(|_| ChainError("TX_DECODE"))?;
    let slot = w.slot.ok_or(ChainError("TX_SLOT_MISSING"))?;
    let (index, block_time) = (w.transaction_index, w.block_time);
    tx(w, slot, index, block_time)
}

/// A `{transaction, meta, version}` entry whose slot (and index / block time) the caller knows:
/// `getBlock` entries and notification payloads. An embedded `slot` must agree.
pub fn decode_entry(
    value: &serde_json::Value,
    slot: u64,
    transaction_index: Option<u64>,
    block_time: Option<i64>,
) -> Result<ChainTx, ChainError> {
    let w = WireTx::deserialize(value).map_err(|_| ChainError("TX_DECODE"))?;
    if w.slot.is_some_and(|s| s != slot) {
        return Err(ChainError("TX_SLOT_MISMATCH"));
    }
    let index = transaction_index.or(w.transaction_index);
    let block_time = block_time.or(w.block_time);
    tx(w, slot, index, block_time)
}

/// `transactionNotification` result: `{signature, slot, transaction: {transaction, meta, version}}`.
/// The outer signature must equal the transaction's first signature.
pub fn decode_notification(value: &serde_json::Value) -> Result<ChainTx, ChainError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        signature: String,
        slot: u64,
        #[serde(default)]
        transaction_index: Option<u64>,
        transaction: WireTx,
    }
    let e = Envelope::deserialize(value).map_err(|_| ChainError("NOTIFICATION_DECODE"))?;
    let decoded = tx(e.transaction, e.slot, e.transaction_index, None)?;
    if decoded.signature() != Some(e.signature.as_str()) {
        return Err(ChainError("NOTIFICATION_SIGNATURE_MISMATCH"));
    }
    Ok(decoded)
}

/// Header of a `getBlock` answer (any `transactionDetails`), validated against `slot`:
/// the parent must be older and the blockhash non-empty.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockHeader {
    pub blockhash: String,
    /// `None` when the provider omitted it; required only where the parent chain is proven.
    pub previous_blockhash: Option<String>,
    pub parent_slot: u64,
    pub block_time: Option<i64>,
    pub block_height: Option<u64>,
}

pub fn decode_block_header(
    value: &serde_json::Value,
    slot: u64,
) -> Result<BlockHeader, ChainError> {
    let w = WireBlock::deserialize(value).map_err(|_| ChainError("BLOCK_HEADER_INVALID"))?;
    if w.parent_slot >= slot || w.blockhash.is_empty() {
        return Err(ChainError("BLOCK_HEADER_INVALID"));
    }
    Ok(BlockHeader {
        blockhash: w.blockhash,
        previous_blockhash: w.previous_blockhash,
        parent_slot: w.parent_slot,
        block_time: w.block_time,
        block_height: w.block_height,
    })
}

/// `getBlock` result (`encoding = "json"`, `transactionDetails = "full"`) of `slot`. Each entry
/// gets its array index as `transaction_index` and the block's `blockTime`.
pub fn decode_block(value: &serde_json::Value, slot: u64) -> Result<ChainBlock, ChainError> {
    let w = WireBlock::deserialize(value).map_err(|_| ChainError("BLOCK_DECODE"))?;
    let entries = w
        .transactions
        .as_ref()
        .ok_or(ChainError("BLOCK_TRANSACTIONS_MISSING"))?;
    let transactions = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let e = WireTx::deserialize(entry).map_err(|_| ChainError("TX_DECODE"))?;
            tx(e, slot, Some(i as u64), w.block_time)
        })
        .collect();
    Ok(ChainBlock {
        slot,
        blockhash: w.blockhash,
        previous_blockhash: w.previous_blockhash,
        parent_slot: w.parent_slot,
        block_time: w.block_time,
        block_height: w.block_height,
        transactions,
    })
}

/// `logsNotification` result: signature, status and log lines only — no message, no meta.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChainLogs {
    pub slot: u64,
    pub signature: String,
    pub status: TxStatus,
    /// `None` when `logs` was absent or `null`.
    pub logs: Option<Vec<String>>,
}

/// `logsNotification` result `{context: {slot}, value: {signature, err, logs}}`. The status
/// follows the transaction table (`err: null` success, absent UNKNOWN); a non-string log line
/// fails the notification instead of being dropped.
pub fn decode_logs_notification(value: &serde_json::Value) -> Result<ChainLogs, ChainError> {
    #[derive(Deserialize)]
    struct Context {
        slot: u64,
    }
    #[derive(Deserialize)]
    struct Value {
        signature: String,
        #[serde(default)]
        err: Field<Option<WireErr>>,
        #[serde(default)]
        logs: Option<Vec<String>>,
    }
    #[derive(Deserialize)]
    struct Envelope {
        context: Context,
        value: Value,
    }
    let e = Envelope::deserialize(value).map_err(|_| ChainError("LOGS_NOTIFICATION_DECODE"))?;
    Ok(ChainLogs {
        slot: e.context.slot,
        signature: e.value.signature,
        status: status(e.value.err),
        logs: e.value.logs,
    })
}

fn status(err: Field<Option<WireErr>>) -> TxStatus {
    match err {
        Field::Absent => TxStatus::Unknown("TX_META_STATUS_MISSING"),
        Field::Present(None) => TxStatus::Success,
        Field::Present(Some(WireErr::Known(e))) => TxStatus::Failed(e.into()),
        Field::Present(Some(WireErr::Unknown(_))) => TxStatus::Unknown("TX_ERROR_UNDECODABLE"),
    }
}

// ---- wire re-encoding: offline compatibility renders of typed evidence only ----

/// Wire `err` of a status: `null`, the error, absent (`None`) for a missing status. An
/// undecodable error is not reproducible and renders as its non-null UNKNOWN code, so a
/// reader can never take it for success.
pub fn encode_status(status: &TxStatus) -> Option<serde_json::Value> {
    match status {
        TxStatus::Success => Some(serde_json::Value::Null),
        TxStatus::Failed(e) => serde_json::to_value(UiTransactionError::from(e.clone())).ok(),
        TxStatus::Unknown("TX_META_STATUS_MISSING" | "TX_META_MISSING") => None,
        TxStatus::Unknown(code) => Some(serde_json::Value::from(*code)),
    }
}

fn encode_instruction(ix: &ChainInstruction) -> serde_json::Value {
    let mut v = serde_json::json!({"programIdIndex": ix.program_id_index, "accounts": ix.accounts,
        "data": bs58::encode(&ix.data).into_string()});
    if let Some(h) = ix.stack_height {
        v["stackHeight"] = h.into();
    }
    v
}

/// `uiAmountString` exactly from atomic units (no float), trailing zeros trimmed.
fn ui_amount_string(amount: u64, decimals: u8) -> String {
    let digits = amount.to_string();
    let d = usize::from(decimals);
    if d == 0 {
        return digits;
    }
    let padded = format!("{digits:0>width$}", width = d + 1);
    let (int, frac) = padded.split_at(padded.len() - d);
    let frac = frac.trim_end_matches('0');
    if frac.is_empty() {
        int.to_string()
    } else {
        format!("{int}.{frac}")
    }
}

fn encode_token_balances(list: &[TokenBalance]) -> serde_json::Value {
    list.iter()
        .map(|b| {
            let mut v = serde_json::json!({"accountIndex": b.account_index, "mint": b.mint.to_string(),
                "uiTokenAmount": {"amount": b.amount.to_string(), "decimals": b.decimals,
                    "uiAmountString": ui_amount_string(b.amount, b.decimals)}});
            if let Some(o) = b.owner {
                v["owner"] = o.to_string().into();
            }
            if let Some(p) = b.program_id {
                v["programId"] = p.to_string().into();
            }
            v
        })
        .collect()
}

fn encode_meta(m: &ChainMeta) -> serde_json::Value {
    let mut v = serde_json::Map::new();
    if let Some(err) = encode_status(&m.status) {
        v.insert("err".into(), err);
    }
    let keys = |k: &[Pubkey]| k.iter().map(ToString::to_string).collect::<Vec<_>>();
    if let Some(x) = m.fee {
        v.insert("fee".into(), x.into());
    }
    if let Some(x) = &m.pre_balances {
        v.insert("preBalances".into(), x.clone().into());
    }
    if let Some(x) = &m.post_balances {
        v.insert("postBalances".into(), x.clone().into());
    }
    if let Some(x) = &m.pre_token_balances {
        v.insert("preTokenBalances".into(), encode_token_balances(x));
    }
    if let Some(x) = &m.post_token_balances {
        v.insert("postTokenBalances".into(), encode_token_balances(x));
    }
    if let Some(x) = &m.inner_instructions {
        let groups: Vec<_> = x
            .iter()
            .map(|g| {
                serde_json::json!({"index": g.index,
                "instructions": g.instructions.iter().map(encode_instruction).collect::<Vec<_>>()})
            })
            .collect();
        v.insert("innerInstructions".into(), groups.into());
    }
    if let Some(x) = &m.log_messages {
        v.insert("logMessages".into(), x.clone().into());
    }
    if let Some(x) = &m.loaded_addresses {
        v.insert(
            "loadedAddresses".into(),
            serde_json::json!({"writable": keys(&x.writable), "readonly": keys(&x.readonly)}),
        );
    }
    if let Some(x) = m.compute_units_consumed {
        v.insert("computeUnitsConsumed".into(), x.into());
    }
    if let Some(x) = m.cost_units {
        v.insert("costUnits".into(), x.into());
    }
    v.into()
}

/// `{transaction, meta, version}` in the `json` wire encoding, from the typed model. Fields the
/// model does not carry (rewards, returnData, `status`, float `uiAmount`) are not rendered.
pub fn encode_entry(tx: &ChainTx) -> serde_json::Value {
    let m = &tx.message;
    let mut message = serde_json::json!({
        "accountKeys": m.account_keys.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "instructions": m.instructions.iter().map(encode_instruction).collect::<Vec<_>>(),
    });
    if let Some((a, b, c)) = m.header {
        message["header"] = serde_json::json!({"numRequiredSignatures": a,
            "numReadonlySignedAccounts": b, "numReadonlyUnsignedAccounts": c});
    }
    if let Some(h) = &m.recent_blockhash {
        message["recentBlockhash"] = h.clone().into();
    }
    if let Some(l) = &m.address_table_lookups {
        message["addressTableLookups"] = l
            .iter()
            .map(|l| {
                serde_json::json!({"accountKey": l.account_key.to_string(),
                "writableIndexes": l.writable_indexes, "readonlyIndexes": l.readonly_indexes})
            })
            .collect();
    }
    if let Some(c) = &m.transaction_config {
        let mut config = serde_json::Map::new();
        for (name, value) in [
            ("computeUnitLimit", c.compute_unit_limit.map(u64::from)),
            ("heapSize", c.heap_size.map(u64::from)),
            (
                "loadedAccountsDataSizeLimit",
                c.loaded_accounts_data_size_limit.map(u64::from),
            ),
            ("priorityFee", c.priority_fee),
        ] {
            if let Some(v) = value {
                config.insert(name.into(), v.into());
            }
        }
        message["transactionConfig"] = config.into();
    }
    let mut v = serde_json::json!({
        "transaction": {"signatures": tx.signatures, "message": message},
        "meta": tx.meta.as_ref().map(encode_meta),
    });
    match tx.version {
        Some(TxWireVersion::Legacy) => v["version"] = "legacy".into(),
        Some(TxWireVersion::Number(n)) => v["version"] = n.into(),
        None => {}
    }
    v
}

/// `logsNotification` result from the typed logs.
pub fn encode_logs(l: &ChainLogs) -> serde_json::Value {
    let mut value = serde_json::json!({"signature": l.signature, "logs": l.logs});
    if let Some(err) = encode_status(&l.status) {
        value["err"] = err;
    }
    serde_json::json!({"context": {"slot": l.slot}, "value": value})
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const V1: &str = include_str!("../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json");
    const V0_LUT5: &str = include_str!("../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json");

    fn fixture(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    /// Real Mainnet `getTransaction` answers decode losslessly: v1 config, v0 lookups, exact token units.
    #[test]
    fn real_v1_and_v0_lookup_transactions_decode_losslessly() {
        let raw = fixture(V1);
        let t = decode_transaction(&raw).unwrap();
        assert_eq!(t.version_label(), "v1");
        assert_eq!(t.signature(), Some("2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf"));
        assert_eq!(t.status(), TxStatus::Success);
        let config = t.message.transaction_config.unwrap();
        assert_eq!(
            (config.compute_unit_limit, config.priority_fee),
            (Some(1_400_000), Some(14_000))
        );
        let m = t.meta.as_ref().unwrap();
        assert_eq!(
            (m.compute_units_consumed, m.cost_units),
            (Some(345_097), Some(362_017))
        );
        let post = &m.post_token_balances.as_ref().unwrap()[0];
        assert_eq!(
            (post.account_index, post.amount, post.decimals),
            (4, 135_224_452_408, 9)
        );
        assert!(m.inner_instructions.as_ref().is_some_and(|g| !g.is_empty()));
        assert!(m.log_messages.as_ref().is_some_and(|l| !l.is_empty()));
        assert_eq!(t.transaction_index, raw["transactionIndex"].as_u64());

        let raw = fixture(V0_LUT5);
        let t = decode_transaction(&raw).unwrap();
        assert_eq!(t.version_label(), "v0");
        assert_eq!(
            t.message.address_table_lookups.as_ref().map(Vec::len),
            Some(5)
        );
        let loaded = t.meta.as_ref().unwrap().loaded_addresses.as_ref().unwrap();
        assert_eq!(
            t.account_keys().unwrap().len(),
            t.message.account_keys.len() + loaded.writable.len() + loaded.readonly.len()
        );
        assert!(t.message.transaction_config.is_none());
    }

    /// The status table: `err: null` success, error failed, absent `err` / `meta` or an
    /// undecodable error UNKNOWN — never success.
    #[test]
    fn status_keeps_null_error_absent_and_undecodable_apart() {
        let base = fixture(V0_LUT5);
        let with = |f: &dyn Fn(&mut Value)| {
            let mut v = base.clone();
            f(&mut v);
            decode_transaction(&v).unwrap().status()
        };
        assert_eq!(with(&|v| v["meta"]["err"] = Value::Null), TxStatus::Success);
        assert!(matches!(
            with(&|v| v["meta"]["err"] = json!({"InstructionError":[2,{"Custom":6009}]})),
            TxStatus::Failed(TransactionError::InstructionError(2, _))
        ));
        assert_eq!(
            with(&|v| {
                v["meta"].as_object_mut().unwrap().remove("err");
            }),
            TxStatus::Unknown("TX_META_STATUS_MISSING")
        );
        assert_eq!(
            with(&|v| {
                v.as_object_mut().unwrap().remove("meta");
            }),
            TxStatus::Unknown("TX_META_MISSING")
        );
        assert_eq!(
            with(&|v| v["meta"] = Value::Null),
            TxStatus::Unknown("TX_META_MISSING")
        );
        assert_eq!(
            with(&|v| v["meta"]["err"] = json!({"FutureVariant":{"x":1}})),
            TxStatus::Unknown("TX_ERROR_UNDECODABLE")
        );
        assert!(!TxStatus::Unknown("x").is_success());
    }

    /// Fail-closed decoding: unresolvable lookups, unknown versions, invalid keys/data/amounts.
    #[test]
    fn unresolvable_or_unknown_structures_fail_closed() {
        let base = fixture(V0_LUT5);
        let mut v = base.clone();
        v["meta"].as_object_mut().unwrap().remove("loadedAddresses");
        assert_eq!(
            decode_transaction(&v).unwrap().account_keys(),
            Err(ChainError("LOADED_ADDRESSES_MISSING"))
        );
        for (edit, code) in [
            (json!(["version", "v9"]), "TX_VERSION_UNSUPPORTED"),
            (json!(["key", "not-a-key"]), "INVALID_KEY"),
            (json!(["data", "0OIl"]), "INSTRUCTION_DATA_INVALID"),
            (json!(["amount", "1.5"]), "TOKEN_AMOUNT_INVALID"),
        ] {
            let mut v = base.clone();
            match edit[0].as_str().unwrap() {
                "version" => v["version"] = edit[1].clone(),
                "key" => v["transaction"]["message"]["accountKeys"][0] = edit[1].clone(),
                "data" => v["transaction"]["message"]["instructions"][0]["data"] = edit[1].clone(),
                _ => v["meta"]["postTokenBalances"][0]["uiTokenAmount"]["amount"] = edit[1].clone(),
            }
            assert_eq!(decode_transaction(&v), Err(ChainError(code)));
        }
        let mut legacy = base.clone();
        legacy["version"] = json!("legacy");
        assert_eq!(
            decode_transaction(&legacy).unwrap().version,
            Some(TxWireVersion::Legacy)
        );
    }

    /// A block keeps every entry with its index; a broken entry stays visible as an error.
    #[test]
    fn block_entries_keep_index_and_visible_decode_errors() {
        let tx = fixture(V1);
        let entry =
            json!({"transaction": tx["transaction"], "meta": tx["meta"], "version": tx["version"]});
        let block = json!({"blockhash":"11111111111111111111111111111111","previousBlockhash":"11111111111111111111111111111111",
            "parentSlot":9,"blockTime":1_700_000_000,"blockHeight":5,"transactions":[entry, {"transaction":{}}]});
        let b = decode_block(&block, 10).unwrap();
        assert_eq!(b.transactions.len(), 2);
        let first = b.transactions[0].as_ref().unwrap();
        assert_eq!(
            (first.slot, first.transaction_index, first.block_time),
            (10, Some(0), Some(1_700_000_000))
        );
        assert_eq!(b.transactions[1], Err(ChainError("TX_DECODE")));
        let mut missing = block.clone();
        missing.as_object_mut().unwrap().remove("transactions");
        assert_eq!(
            decode_block(&missing, 10),
            Err(ChainError("BLOCK_TRANSACTIONS_MISSING"))
        );
        let note = json!({"signature": tx["transaction"]["signatures"][0], "slot": 10, "transaction": entry});
        assert_eq!(decode_notification(&note).unwrap().slot, 10);
        let mut wrong = note.clone();
        wrong["signature"] =
            json!("1111111111111111111111111111111111111111111111111111111111111111");
        assert_eq!(
            decode_notification(&wrong),
            Err(ChainError("NOTIFICATION_SIGNATURE_MISMATCH"))
        );
    }
}

#[cfg(test)]
mod stored_tests {
    use super::*;

    /// A typed transaction survives the binary journal encoding exactly, UNKNOWN codes included.
    #[test]
    fn chain_tx_round_trips_through_bincode() {
        let raw: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json"
        ))
        .unwrap();
        let mut tx = decode_transaction(&raw).unwrap();
        for status in [
            TxStatus::Success,
            TxStatus::Failed(TransactionError::AccountInUse),
            TxStatus::Unknown("TX_META_STATUS_MISSING"),
        ] {
            tx.meta.as_mut().unwrap().status = status;
            let bytes = bincode::serialize(&tx).unwrap();
            assert_eq!(bincode::deserialize::<ChainTx>(&bytes).unwrap(), tx);
        }
        let forged = bincode::serialize(&TxStatusStored::Unknown("NOT_A_CODE".into())).unwrap();
        assert!(bincode::deserialize::<TxStatus>(&forged).is_err());
    }

    /// A finalized block of a real v0 (5 LUT) transaction and an undecodable entry.
    fn fixture_block() -> serde_json::Value {
        let tx: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json"
        ))
        .unwrap();
        let slot = tx["slot"].as_u64().unwrap();
        serde_json::json!({
            "parentSlot": slot - 1, "previousBlockhash": "p", "blockhash": "h",
            "blockTime": tx["blockTime"], "blockHeight": 7,
            "transactions": [
                {"transaction": tx["transaction"], "meta": tx["meta"], "version": tx["version"]},
                {"transaction": 1}
            ]
        })
    }

    /// F10: every reason this decoder constructs is registered, a decoded block (entry errors
    /// included) survives the binary encoding exactly and a forged reason never decodes.
    #[test]
    fn decoded_block_round_trips_and_every_reason_is_registered() {
        let production = include_str!("chain.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let used: std::collections::BTreeSet<&str> = production
            .split("ChainError(\"")
            .skip(1)
            .map(|rest| &rest[..rest.find('"').unwrap()])
            .collect();
        assert_eq!(used, DECODER_CODES.into_iter().collect());
        let raw = fixture_block();
        let slot = raw["parentSlot"].as_u64().unwrap() + 1;
        let block = decode_block(&raw, slot).unwrap();
        assert!(block.transactions[0].is_ok());
        assert_eq!(block.transactions[1], Err(ChainError("TX_DECODE")));
        let bytes = bincode::serialize(&block).unwrap();
        assert_eq!(bincode::deserialize::<ChainBlock>(&bytes).unwrap(), block);
        let forged = bincode::serialize("NOT_A_CODE").unwrap();
        assert!(bincode::deserialize::<ChainError>(&forged).is_err());
    }

    /// The decoder's output is pinned to `WIRE_DECODER_VERSION`: a changed decode result or
    /// layout fails here until the version is bumped and the pin renewed (typed caches of the
    /// old version are then refused, never reused).
    #[test]
    fn decoded_block_is_pinned_to_the_decoder_version() {
        let raw = fixture_block();
        let slot = raw["parentSlot"].as_u64().unwrap() + 1;
        let bytes = bincode::serialize(&decode_block(&raw, slot).unwrap()).unwrap();
        assert_eq!(
            (
                WIRE_DECODER_VERSION,
                solana_sdk::hash::hash(&bytes).to_string()
            ),
            (
                1,
                "4mM6avd1hwbYYiooUmigRBue8g3F8ZxEcEtU9ySa2izu".to_string()
            )
        );
    }
}

#[cfg(test)]
mod wire_render_tests {
    use super::*;
    use serde_json::{json, Value};

    /// The offline re-encoding of real v1 / v0 (5 LUT) answers decodes back to the same typed
    /// transaction and reproduces every modelled wire field of the original.
    #[test]
    fn re_encoded_real_transactions_round_trip_and_match_the_wire() {
        for text in [
            include_str!("../../../../docs/evidence/AUDIT_20261005/2cebUJBWZYNW9ZNRweyV4NygKshP6dcnR4M8z3rTUHjNA6yEfkWWLRpXdqRtBPpTF68BGph964ebYtuEcxdvHJEf.json"),
            include_str!("../../../../docs/evidence/AUDIT_20261005/3ab7mnuTErWjRtS8iz4Nm619VNvm3FSw47VVvdjYnSLNzB2btCoreDsQqmzfb4Qt5oBA29saPQ98Rgy2nyAUExRB.json"),
        ] {
            let raw: Value = serde_json::from_str(text).unwrap();
            let tx = decode_transaction(&raw).unwrap();
            let entry = encode_entry(&tx);
            assert_eq!(
                decode_entry(&entry, tx.slot, tx.transaction_index, tx.block_time).unwrap(),
                tx
            );
            assert_eq!(entry["transaction"]["signatures"], raw["transaction"]["signatures"]);
            assert_eq!(entry["version"], raw["version"]);
            let (m, w) = (&entry["transaction"]["message"], &raw["transaction"]["message"]);
            for k in ["accountKeys", "recentBlockhash", "header", "addressTableLookups"] {
                assert_eq!(m.get(k), w.get(k), "{k}");
            }
            for (a, b) in m["instructions"].as_array().unwrap().iter().zip(w["instructions"].as_array().unwrap()) {
                for k in ["programIdIndex", "accounts", "data"] {
                    assert_eq!(a[k], b[k]);
                }
            }
            let (m, w) = (&entry["meta"], &raw["meta"]);
            for k in ["err", "fee", "preBalances", "postBalances", "logMessages", "loadedAddresses",
                "computeUnitsConsumed", "costUnits"] {
                assert_eq!(m.get(k), w.get(k), "{k}");
            }
            for k in ["preTokenBalances", "postTokenBalances"] {
                for (a, b) in m[k].as_array().unwrap().iter().zip(w[k].as_array().unwrap()) {
                    for f in ["accountIndex", "mint", "owner", "programId"] {
                        assert_eq!(a.get(f), b.get(f));
                    }
                    for f in ["amount", "decimals", "uiAmountString"] {
                        assert_eq!(a["uiTokenAmount"][f], b["uiTokenAmount"][f], "{k}.{f}");
                    }
                }
            }
        }
    }

    /// Logs notifications: status table, absent logs, non-string lines fail closed; the render
    /// never turns a missing or undecodable status into success.
    #[test]
    fn logs_notification_status_and_render() {
        let n = |value: Value| json!({"context": {"slot": 7}, "value": value});
        let ok =
            decode_logs_notification(&n(json!({"signature":"s","err":null,"logs":["a"]}))).unwrap();
        assert_eq!(
            (ok.status.clone(), ok.logs.clone()),
            (TxStatus::Success, Some(vec!["a".into()]))
        );
        assert_eq!(
            encode_logs(&ok),
            n(json!({"signature":"s","err":null,"logs":["a"]}))
        );
        let failed = decode_logs_notification(&n(
            json!({"signature":"s","err":{"InstructionError":[0,{"Custom":1}]},"logs":null}),
        ))
        .unwrap();
        assert!(matches!(failed.status, TxStatus::Failed(_)) && failed.logs.is_none());
        assert_eq!(
            encode_logs(&failed)["value"]["err"],
            json!({"InstructionError":[0,{"Custom":1}]})
        );
        let absent = decode_logs_notification(&n(json!({"signature":"s","logs":[]}))).unwrap();
        assert_eq!(absent.status, TxStatus::Unknown("TX_META_STATUS_MISSING"));
        assert!(encode_logs(&absent)["value"].get("err").is_none());
        let odd =
            decode_logs_notification(&n(json!({"signature":"s","err":{"Future":1},"logs":[]})))
                .unwrap();
        assert_eq!(encode_logs(&odd)["value"]["err"], "TX_ERROR_UNDECODABLE");
        assert_eq!(
            decode_logs_notification(&n(json!({"signature":"s","err":null,"logs":["a", 1]}))),
            Err(ChainError("LOGS_NOTIFICATION_DECODE"))
        );
        assert_eq!(ui_amount_string(135_224_452_408, 9), "135.224452408");
        assert_eq!(ui_amount_string(1_000_000, 6), "1");
        assert_eq!(ui_amount_string(5, 3), "0.005");
        assert_eq!(ui_amount_string(0, 9), "0");
        assert_eq!(ui_amount_string(42, 0), "42");
    }
}
