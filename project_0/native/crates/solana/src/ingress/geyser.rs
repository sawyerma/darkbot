//! S1-02/S1-08: native `BinaryGeyserSource` — Yellowstone gRPC `Subscribe` updates decoded with
//! the protobuf layout of the pinned source (rpcpool/yellowstone-grpc commit
//! `2a5486693a7ccf900ff7ccc2de1e6c6570899245`: `yellowstone-grpc-proto/proto/geyser.proto`
//! SHA-256 `80ee337f361198a0…`, `solana-storage.proto` SHA-256 `e5d16ba9a9e83f6e…`) and mapped onto
//! the one `IngressEvent` model of the canonical owner (no JSON, no second runtime). Account
//! updates keep `write_version`; slots, roots, transactions (legacy/v0/v1, ALT, meta) map onto the
//! existing types. Duplicates are dropped; a dead slot, an ambiguous bank/fork context, a missed
//! parent slot or a reconnect without proven replay is a `Gap`; an unknown update kind, an
//! undecodable frame or a broken field is `Fatal` (fail closed).
//! Offline only: the transport is a trait (`GeyserTransport`); no network transport is built and
//! no Mainnet connection is made here. PublicNode is the first configurable FREE_ONLY candidate;
//! its capabilities, latency and coverage are NOT_RUN. `SubscribeDeshred` is K-only (version gate).
use super::{
    AccountUpdate, IngressError, IngressEvent, IngressHalt, IngressSource, SlotStatus, SlotUpdate,
    SlotWatch, SourceKind, Subscription, TransactionObserved,
};
use crate::chain::{
    ChainError, ChainInstruction, ChainLookup, ChainMessage, ChainMeta, ChainTx, InnerInstructions,
    LoadedAddresses, TokenBalance, TransactionConfig, TxStatus, TxWireVersion,
};
use darkbot_common::channel::{ChannelError, Producer};
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use prost::Message;
use solana_sdk::pubkey::Pubkey;
use std::collections::{BTreeMap, HashMap, VecDeque};

/// The pinned Yellowstone source of the message layout below.
pub const PROTO_SOURCE_COMMIT: &str = "2a5486693a7ccf900ff7ccc2de1e6c6570899245";

/// Protobuf messages of the pinned `geyser.proto` / `solana-storage.proto`, only the fields this
/// adapter reads or writes (prost skips every other field of a known message).
pub mod proto {
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeUpdate {
        #[prost(string, repeated, tag = "1")]
        pub filters: Vec<String>,
        #[prost(
            oneof = "subscribe_update::UpdateOneof",
            tags = "2, 3, 4, 10, 5, 6, 9, 7, 8, 12, 13"
        )]
        pub update_oneof: Option<subscribe_update::UpdateOneof>,
    }
    pub mod subscribe_update {
        // Like prost-build output: the transaction variant is the large one by design.
        #[allow(clippy::large_enum_variant)]
        #[derive(Clone, PartialEq, prost::Oneof)]
        pub enum UpdateOneof {
            #[prost(message, tag = "2")]
            Account(super::SubscribeUpdateAccount),
            #[prost(message, tag = "3")]
            Slot(super::SubscribeUpdateSlot),
            #[prost(message, tag = "4")]
            Transaction(super::SubscribeUpdateTransaction),
            #[prost(message, tag = "10")]
            TransactionStatus(super::Unused),
            #[prost(message, tag = "5")]
            Block(super::Unused),
            #[prost(message, tag = "6")]
            Ping(super::Unused),
            #[prost(message, tag = "9")]
            Pong(super::Unused),
            #[prost(message, tag = "7")]
            BlockMeta(super::Unused),
            #[prost(message, tag = "8")]
            Entry(super::Unused),
            #[prost(message, tag = "12")]
            BlockFooter(super::Unused),
            #[prost(message, tag = "13")]
            EntryUpdateParent(super::SubscribeUpdateEntryUpdateParent),
        }
    }
    /// A pinned message whose fields this adapter does not use.
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Unused {}

    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeUpdateAccount {
        #[prost(message, optional, tag = "1")]
        pub account: Option<SubscribeUpdateAccountInfo>,
        #[prost(uint64, tag = "2")]
        pub slot: u64,
        #[prost(bool, tag = "3")]
        pub is_startup: bool,
        #[prost(uint64, optional, tag = "4")]
        pub bank_id: Option<u64>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeUpdateAccountInfo {
        #[prost(bytes = "vec", tag = "1")]
        pub pubkey: Vec<u8>,
        #[prost(uint64, tag = "2")]
        pub lamports: u64,
        #[prost(bytes = "vec", tag = "3")]
        pub owner: Vec<u8>,
        #[prost(bool, tag = "4")]
        pub executable: bool,
        #[prost(uint64, tag = "5")]
        pub rent_epoch: u64,
        #[prost(bytes = "vec", tag = "6")]
        pub data: Vec<u8>,
        #[prost(uint64, tag = "7")]
        pub write_version: u64,
        #[prost(bytes = "vec", optional, tag = "8")]
        pub txn_signature: Option<Vec<u8>>,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
    #[repr(i32)]
    pub enum SlotStatus {
        SlotProcessed = 0,
        SlotConfirmed = 1,
        SlotFinalized = 2,
        SlotFirstShredReceived = 3,
        SlotCompleted = 4,
        SlotCreatedBank = 5,
        SlotDead = 6,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeUpdateSlot {
        #[prost(uint64, tag = "1")]
        pub slot: u64,
        #[prost(uint64, optional, tag = "2")]
        pub parent: Option<u64>,
        #[prost(enumeration = "SlotStatus", tag = "3")]
        pub status: i32,
        #[prost(string, optional, tag = "4")]
        pub dead_error: Option<String>,
        #[prost(uint64, optional, tag = "5")]
        pub bank_id: Option<u64>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeUpdateTransaction {
        #[prost(message, optional, tag = "1")]
        pub transaction: Option<SubscribeUpdateTransactionInfo>,
        #[prost(uint64, tag = "2")]
        pub slot: u64,
        #[prost(uint64, tag = "3")]
        pub bank_id: u64,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeUpdateTransactionInfo {
        #[prost(bytes = "vec", tag = "1")]
        pub signature: Vec<u8>,
        #[prost(bool, tag = "2")]
        pub is_vote: bool,
        #[prost(message, optional, tag = "3")]
        pub transaction: Option<Transaction>,
        #[prost(message, optional, tag = "4")]
        pub meta: Option<TransactionStatusMeta>,
        #[prost(uint64, tag = "5")]
        pub index: u64,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeUpdateEntryUpdateParent {
        #[prost(uint64, tag = "1")]
        pub slot: u64,
        #[prost(uint64, tag = "2")]
        pub cleared_bank_id: u64,
        #[prost(uint64, tag = "3")]
        pub parent_slot: u64,
    }
    // ---- solana-storage.proto ----
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Transaction {
        #[prost(bytes = "vec", repeated, tag = "1")]
        pub signatures: Vec<Vec<u8>>,
        #[prost(message, optional, tag = "2")]
        pub message: Option<MessageBody>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct MessageBody {
        #[prost(message, optional, tag = "1")]
        pub header: Option<MessageHeader>,
        #[prost(bytes = "vec", repeated, tag = "2")]
        pub account_keys: Vec<Vec<u8>>,
        #[prost(bytes = "vec", tag = "3")]
        pub recent_blockhash: Vec<u8>,
        #[prost(message, repeated, tag = "4")]
        pub instructions: Vec<CompiledInstruction>,
        #[prost(bool, tag = "5")]
        pub versioned: bool,
        #[prost(message, repeated, tag = "6")]
        pub address_table_lookups: Vec<MessageAddressTableLookup>,
        #[prost(message, optional, tag = "7")]
        pub config: Option<TransactionConfigProto>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct TransactionConfigProto {
        #[prost(uint64, optional, tag = "1")]
        pub priority_fee: Option<u64>,
        #[prost(uint32, optional, tag = "2")]
        pub compute_unit_limit: Option<u32>,
        #[prost(uint32, optional, tag = "3")]
        pub loaded_accounts_data_size_limit: Option<u32>,
        #[prost(uint32, optional, tag = "4")]
        pub heap_size: Option<u32>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct MessageHeader {
        #[prost(uint32, tag = "1")]
        pub num_required_signatures: u32,
        #[prost(uint32, tag = "2")]
        pub num_readonly_signed_accounts: u32,
        #[prost(uint32, tag = "3")]
        pub num_readonly_unsigned_accounts: u32,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct MessageAddressTableLookup {
        #[prost(bytes = "vec", tag = "1")]
        pub account_key: Vec<u8>,
        #[prost(bytes = "vec", tag = "2")]
        pub writable_indexes: Vec<u8>,
        #[prost(bytes = "vec", tag = "3")]
        pub readonly_indexes: Vec<u8>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct CompiledInstruction {
        #[prost(uint32, tag = "1")]
        pub program_id_index: u32,
        #[prost(bytes = "vec", tag = "2")]
        pub accounts: Vec<u8>,
        #[prost(bytes = "vec", tag = "3")]
        pub data: Vec<u8>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct TransactionStatusMeta {
        #[prost(message, optional, tag = "1")]
        pub err: Option<TransactionError>,
        #[prost(uint64, tag = "2")]
        pub fee: u64,
        #[prost(uint64, repeated, tag = "3")]
        pub pre_balances: Vec<u64>,
        #[prost(uint64, repeated, tag = "4")]
        pub post_balances: Vec<u64>,
        #[prost(message, repeated, tag = "5")]
        pub inner_instructions: Vec<InnerInstructionsProto>,
        #[prost(bool, tag = "10")]
        pub inner_instructions_none: bool,
        #[prost(string, repeated, tag = "6")]
        pub log_messages: Vec<String>,
        #[prost(bool, tag = "11")]
        pub log_messages_none: bool,
        #[prost(message, repeated, tag = "7")]
        pub pre_token_balances: Vec<TokenBalanceProto>,
        #[prost(message, repeated, tag = "8")]
        pub post_token_balances: Vec<TokenBalanceProto>,
        #[prost(bytes = "vec", repeated, tag = "12")]
        pub loaded_writable_addresses: Vec<Vec<u8>>,
        #[prost(bytes = "vec", repeated, tag = "13")]
        pub loaded_readonly_addresses: Vec<Vec<u8>>,
        #[prost(uint64, optional, tag = "16")]
        pub compute_units_consumed: Option<u64>,
        #[prost(uint64, optional, tag = "17")]
        pub cost_units: Option<u64>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct TransactionError {
        #[prost(bytes = "vec", tag = "1")]
        pub err: Vec<u8>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct InnerInstructionsProto {
        #[prost(uint32, tag = "1")]
        pub index: u32,
        #[prost(message, repeated, tag = "2")]
        pub instructions: Vec<InnerInstruction>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct InnerInstruction {
        #[prost(uint32, tag = "1")]
        pub program_id_index: u32,
        #[prost(bytes = "vec", tag = "2")]
        pub accounts: Vec<u8>,
        #[prost(bytes = "vec", tag = "3")]
        pub data: Vec<u8>,
        #[prost(uint32, optional, tag = "4")]
        pub stack_height: Option<u32>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct TokenBalanceProto {
        #[prost(uint32, tag = "1")]
        pub account_index: u32,
        #[prost(string, tag = "2")]
        pub mint: String,
        #[prost(message, optional, tag = "3")]
        pub ui_token_amount: Option<UiTokenAmount>,
        #[prost(string, tag = "4")]
        pub owner: String,
        #[prost(string, tag = "5")]
        pub program_id: String,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct UiTokenAmount {
        #[prost(uint32, tag = "2")]
        pub decimals: u32,
        #[prost(string, tag = "3")]
        pub amount: String,
    }
    // ---- request side ----
    #[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
    #[repr(i32)]
    pub enum CommitmentLevel {
        Processed = 0,
        Confirmed = 1,
        Finalized = 2,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeRequest {
        #[prost(btree_map = "string, message", tag = "1")]
        pub accounts: std::collections::BTreeMap<String, SubscribeRequestFilterAccounts>,
        #[prost(btree_map = "string, message", tag = "2")]
        pub slots: std::collections::BTreeMap<String, SubscribeRequestFilterSlots>,
        #[prost(btree_map = "string, message", tag = "3")]
        pub transactions: std::collections::BTreeMap<String, SubscribeRequestFilterTransactions>,
        #[prost(enumeration = "CommitmentLevel", optional, tag = "6")]
        pub commitment: Option<i32>,
        #[prost(message, optional, tag = "9")]
        pub ping: Option<SubscribeRequestPing>,
        #[prost(uint64, optional, tag = "11")]
        pub from_slot: Option<u64>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeRequestFilterAccounts {
        #[prost(string, repeated, tag = "2")]
        pub account: Vec<String>,
        #[prost(string, repeated, tag = "3")]
        pub owner: Vec<String>,
        #[prost(message, repeated, tag = "4")]
        pub filters: Vec<SubscribeRequestFilterAccountsFilter>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeRequestFilterAccountsFilter {
        #[prost(oneof = "accounts_filter::Filter", tags = "1, 2")]
        pub filter: Option<accounts_filter::Filter>,
    }
    pub mod accounts_filter {
        #[derive(Clone, PartialEq, prost::Oneof)]
        pub enum Filter {
            #[prost(message, tag = "1")]
            Memcmp(super::Memcmp),
            #[prost(uint64, tag = "2")]
            Datasize(u64),
        }
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct Memcmp {
        #[prost(uint64, tag = "1")]
        pub offset: u64,
        #[prost(bytes = "vec", optional, tag = "2")]
        pub bytes: Option<Vec<u8>>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeRequestFilterSlots {
        #[prost(bool, optional, tag = "1")]
        pub filter_by_commitment: Option<bool>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeRequestFilterTransactions {
        #[prost(bool, optional, tag = "1")]
        pub vote: Option<bool>,
        #[prost(bool, optional, tag = "2")]
        pub failed: Option<bool>,
        #[prost(string, repeated, tag = "3")]
        pub account_include: Vec<String>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    pub struct SubscribeRequestPing {
        #[prost(int32, tag = "1")]
        pub id: i32,
    }
}

/// gRPC message framing (`[compressed: u8][length: u32 BE][protobuf]`), bounded: a compressed
/// frame (none was negotiated) or one larger than `max_message` is a protocol error.
pub struct FrameDecoder {
    buf: Vec<u8>,
    start: usize,
    max_message: usize,
}

impl FrameDecoder {
    pub fn new(max_message: usize) -> Self {
        Self {
            buf: Vec::new(),
            start: 0,
            max_message,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) {
        if self.start > 0 && self.start == self.buf.len() {
            self.buf.clear();
            self.start = 0;
        }
        self.buf.extend_from_slice(chunk);
    }

    /// The next complete message, decoded in place (no copy of the frame).
    pub fn next_message<T: Message + Default>(&mut self) -> Result<Option<T>, &'static str> {
        let pending = &self.buf[self.start..];
        if pending.len() < 5 {
            return Ok(None);
        }
        if pending[0] != 0 {
            return Err("GEYSER_FRAME_COMPRESSED");
        }
        let len = u32::from_be_bytes([pending[1], pending[2], pending[3], pending[4]]) as usize;
        if len > self.max_message {
            return Err("GEYSER_FRAME_TOO_LARGE");
        }
        if pending.len() < 5 + len {
            return Ok(None);
        }
        let message = T::decode(&pending[5..5 + len]).map_err(|_| "GEYSER_PROTOBUF_DECODE")?;
        self.start += 5 + len;
        if self.start > 1 << 20 {
            self.buf.drain(..self.start);
            self.start = 0;
        }
        Ok(Some(message))
    }
}

/// Encodes one gRPC frame (tests, request side).
pub fn frame(message: &impl Message) -> Vec<u8> {
    let body = message.encode_to_vec();
    let mut out = Vec::with_capacity(5 + body.len());
    out.push(0);
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

/// What one update became: events in order, and whether the server asked for a pong.
#[derive(Debug, Default, PartialEq)]
pub struct Mapped {
    pub events: Vec<IngressEvent>,
    pub ping: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapperStats {
    pub duplicates: u64,
    pub ignored: u64,
    pub fork_gaps: u64,
}

/// Stream state of one subscription: the bank of every open slot (fork provenance), recently seen
/// account versions (duplicates), the highest slot seen and the last root.
pub struct GeyserMapper {
    banks: BTreeMap<u64, u64>,
    /// (pubkey, slot, write version) → content digest of its first copy.
    seen: HashMap<(Pubkey, u64, u64), [u8; 32]>,
    seen_order: VecDeque<(Pubkey, u64, u64)>,
    pub max_slot: Option<u64>,
    pub root: Option<u64>,
    pub stats: MapperStats,
}

const SEEN_MAX: usize = 65_536;

fn key32(bytes: &[u8], code: &'static str) -> Result<Pubkey, &'static str> {
    <[u8; 32]>::try_from(bytes)
        .map(Pubkey::new_from_array)
        .map_err(|_| code)
}

impl Default for GeyserMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl GeyserMapper {
    pub fn new() -> Self {
        Self {
            banks: BTreeMap::new(),
            seen: HashMap::new(),
            seen_order: VecDeque::new(),
            max_slot: None,
            root: None,
            stats: MapperStats::default(),
        }
    }

    fn gap(&mut self, slot: u64, t_rx_ns: u64) -> IngressEvent {
        self.stats.fork_gaps += 1;
        IngressEvent::Gap {
            source: SourceKind::Geyser,
            from_slot: Some(slot.saturating_sub(1)),
            to_slot: Some(slot),
            t_rx_ns,
        }
    }

    /// The bank of `slot`: the first seen; another bank for the same slot is a fork the stream
    /// cannot disambiguate (`false`).
    fn bank(&mut self, slot: u64, bank: u64) -> bool {
        *self.banks.entry(slot).or_insert(bank) == bank
    }

    fn observe_slot(&mut self, slot: u64) {
        self.max_slot = Some(self.max_slot.map_or(slot, |m| m.max(slot)));
    }

    /// Maps one update; `Err` is a protocol error (fail closed).
    pub fn map(
        &mut self,
        update: proto::SubscribeUpdate,
        t_rx_ns: u64,
    ) -> Result<Mapped, &'static str> {
        use proto::subscribe_update::UpdateOneof as U;
        let mut out = Mapped::default();
        match update.update_oneof.ok_or("GEYSER_UPDATE_KIND_UNKNOWN")? {
            U::Account(a) => {
                let info = a.account.ok_or("GEYSER_ACCOUNT_INFO_MISSING")?;
                let pubkey = key32(&info.pubkey, "GEYSER_ACCOUNT_KEY_LENGTH")?;
                let owner = key32(&info.owner, "GEYSER_ACCOUNT_OWNER_LENGTH")?;
                if let Some(bank) = a.bank_id {
                    if !self.bank(a.slot, bank) {
                        out.events.push(self.gap(a.slot, t_rx_ns));
                        return Ok(out);
                    }
                }
                let id = (pubkey, a.slot, info.write_version);
                let digest = solana_sdk::hash::hashv(&[
                    &info.owner,
                    &info.lamports.to_le_bytes(),
                    &info.data,
                ])
                .to_bytes();
                match self.seen.get(&id) {
                    Some(d) if *d == digest => {
                        self.stats.duplicates += 1;
                        return Ok(out);
                    }
                    // The same write with other content: the stream contradicts itself.
                    Some(_) => {
                        out.events.push(self.gap(a.slot, t_rx_ns));
                        return Ok(out);
                    }
                    None => {}
                }
                self.seen.insert(id, digest);
                self.seen_order.push_back(id);
                if self.seen_order.len() > SEEN_MAX {
                    if let Some(old) = self.seen_order.pop_front() {
                        self.seen.remove(&old);
                    }
                }
                self.observe_slot(a.slot);
                out.events.push(IngressEvent::Account(AccountUpdate {
                    pubkey,
                    owner,
                    lamports: info.lamports,
                    data: info.data,
                    slot: a.slot,
                    write_version: Some(info.write_version),
                    source: SourceKind::Geyser,
                    t_rx_ns,
                }));
            }
            U::Slot(s) => {
                use proto::SlotStatus as P;
                let status = P::try_from(s.status).map_err(|_| "GEYSER_SLOT_STATUS_UNKNOWN")?;
                if let (Some(parent), Some(max)) = (s.parent, self.max_slot) {
                    if parent > max && status == P::SlotProcessed {
                        // The parent was never seen: updates of slots in between are missing.
                        out.events.push(IngressEvent::Gap {
                            source: SourceKind::Geyser,
                            from_slot: Some(max),
                            to_slot: Some(s.slot),
                            t_rx_ns,
                        });
                    }
                }
                let canonical = match status {
                    P::SlotProcessed => Some(SlotStatus::Processed),
                    P::SlotConfirmed => Some(SlotStatus::Confirmed),
                    P::SlotFinalized => Some(SlotStatus::Finalized),
                    P::SlotCreatedBank => {
                        let bank = s.bank_id.ok_or("GEYSER_CREATED_BANK_WITHOUT_ID")?;
                        if !self.bank(s.slot, bank) {
                            out.events.push(self.gap(s.slot, t_rx_ns));
                        }
                        None
                    }
                    P::SlotDead => {
                        self.banks.remove(&s.slot);
                        out.events.push(self.gap(s.slot, t_rx_ns));
                        None
                    }
                    P::SlotFirstShredReceived | P::SlotCompleted => None,
                };
                if let Some(status) = canonical {
                    self.observe_slot(s.slot);
                    out.events.push(IngressEvent::Slot(SlotUpdate {
                        slot: s.slot,
                        parent: s.parent,
                        status,
                        source: SourceKind::Geyser,
                        t_rx_ns,
                    }));
                    if status == SlotStatus::Finalized {
                        self.root = Some(self.root.map_or(s.slot, |r| r.max(s.slot)));
                        self.banks.retain(|slot, _| *slot > s.slot);
                        out.events.push(IngressEvent::Root {
                            slot: s.slot,
                            source: SourceKind::Geyser,
                            t_rx_ns,
                        });
                    }
                }
            }
            U::Transaction(t) => {
                if !self.bank(t.slot, t.bank_id) {
                    out.events.push(self.gap(t.slot, t_rx_ns));
                    return Ok(out);
                }
                let info = t.transaction.ok_or("GEYSER_TRANSACTION_INFO_MISSING")?;
                let signature = <[u8; 64]>::try_from(info.signature.as_slice())
                    .map_err(|_| "GEYSER_SIGNATURE_LENGTH")?;
                let tx_index = u32::try_from(info.index).ok();
                self.observe_slot(t.slot);
                // Base58 dominates the mapping cost: the envelope signature is encoded once and
                // reused for the identical first transaction signature.
                let signature_text = bs58::encode(signature).into_string();
                let tx = chain_tx_with(info, t.slot, Some((&signature, &signature_text)));
                out.events
                    .push(IngressEvent::Transaction(Box::new(TransactionObserved {
                        signature: signature_text,
                        slot: t.slot,
                        tx_index_in_slot: tx_index,
                        source: SourceKind::Geyser,
                        t_rx_ns,
                        tx,
                    })));
            }
            U::EntryUpdateParent(p) => {
                if self.banks.get(&p.slot) == Some(&p.cleared_bank_id) {
                    self.banks.remove(&p.slot);
                    out.events.push(self.gap(p.slot, t_rx_ns));
                }
            }
            U::Ping(_) => out.ping = true,
            U::Pong(_)
            | U::TransactionStatus(_)
            | U::Block(_)
            | U::BlockMeta(_)
            | U::Entry(_)
            | U::BlockFooter(_) => self.stats.ignored += 1,
        }
        Ok(out)
    }
}

/// One Geyser transaction as the shared `ChainTx` (S1-04 version order: config ⇒ v1, versioned ⇒
/// v0, else legacy). Every byte field is length-checked; a broken one makes the payload an error.
pub fn chain_tx(
    info: proto::SubscribeUpdateTransactionInfo,
    slot: u64,
) -> Result<ChainTx, ChainError> {
    chain_tx_with(info, slot, None)
}

/// `known`: an already encoded signature (raw bytes, text), reused where a signature equals it.
fn chain_tx_with(
    info: proto::SubscribeUpdateTransactionInfo,
    slot: u64,
    known: Option<(&[u8; 64], &String)>,
) -> Result<ChainTx, ChainError> {
    let tx = info.transaction.ok_or(ChainError("GEYSER_TX_MISSING"))?;
    let m = tx.message.ok_or(ChainError("GEYSER_TX_MESSAGE_MISSING"))?;
    if !m.versioned && (m.config.is_some() || !m.address_table_lookups.is_empty()) {
        return Err(ChainError("GEYSER_LEGACY_WITH_V0_V1_FIELDS"));
    }
    let version = if m.config.is_some() {
        TxWireVersion::Number(1)
    } else if m.versioned {
        TxWireVersion::Number(0)
    } else {
        TxWireVersion::Legacy
    };
    let u8_of = |v: u32| u8::try_from(v).map_err(|_| ChainError("GEYSER_U8_RANGE"));
    let key = |b: &Vec<u8>| key32(b, "GEYSER_KEY_LENGTH").map_err(ChainError);
    let signatures = tx
        .signatures
        .iter()
        .map(|s| {
            let raw = <[u8; 64]>::try_from(s.as_slice())
                .map_err(|_| ChainError("GEYSER_SIGNATURE_LENGTH"))?;
            Ok(match known {
                Some((bytes, text)) if *bytes == raw => text.clone(),
                _ => bs58::encode(raw).into_string(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let header = match &m.header {
        Some(h) => Some((
            u8_of(h.num_required_signatures)?,
            u8_of(h.num_readonly_signed_accounts)?,
            u8_of(h.num_readonly_unsigned_accounts)?,
        )),
        None => None,
    };
    let recent_blockhash = match m.recent_blockhash.len() {
        0 => None,
        32 => Some(bs58::encode(&m.recent_blockhash).into_string()),
        _ => return Err(ChainError("GEYSER_BLOCKHASH_LENGTH")),
    };
    let instructions = m
        .instructions
        .iter()
        .map(|i| {
            Ok(ChainInstruction {
                program_id_index: u8_of(i.program_id_index)?,
                accounts: i.accounts.clone(),
                data: i.data.clone(),
                stack_height: None,
            })
        })
        .collect::<Result<Vec<_>, ChainError>>()?;
    let lookups = m
        .address_table_lookups
        .iter()
        .map(|l| {
            Ok(ChainLookup {
                account_key: key(&l.account_key)?,
                writable_indexes: l.writable_indexes.clone(),
                readonly_indexes: l.readonly_indexes.clone(),
            })
        })
        .collect::<Result<Vec<_>, ChainError>>()?;
    let message = ChainMessage {
        header,
        account_keys: m.account_keys.iter().map(key).collect::<Result<_, _>>()?,
        recent_blockhash,
        instructions,
        address_table_lookups: m.versioned.then_some(lookups),
        transaction_config: m.config.map(|c| TransactionConfig {
            compute_unit_limit: c.compute_unit_limit,
            heap_size: c.heap_size,
            loaded_accounts_data_size_limit: c.loaded_accounts_data_size_limit,
            priority_fee: c.priority_fee,
        }),
    };
    let meta = match info.meta {
        None => None,
        Some(meta) => Some(chain_meta(meta, &key, &u8_of)?),
    };
    Ok(ChainTx {
        slot,
        block_time: None,
        transaction_index: Some(info.index),
        version: Some(version),
        signatures,
        message,
        meta,
    })
}

fn chain_meta(
    meta: proto::TransactionStatusMeta,
    key: &dyn Fn(&Vec<u8>) -> Result<Pubkey, ChainError>,
    u8_of: &dyn Fn(u32) -> Result<u8, ChainError>,
) -> Result<ChainMeta, ChainError> {
    let status = match meta.err {
        None => TxStatus::Success,
        Some(e) => bincode::deserialize(&e.err)
            .map(TxStatus::Failed)
            .unwrap_or(TxStatus::Unknown("TX_ERROR_UNDECODABLE")),
    };
    let token = |b: &proto::TokenBalanceProto| -> Result<TokenBalance, ChainError> {
        let ui = b
            .ui_token_amount
            .as_ref()
            .ok_or(ChainError("GEYSER_TOKEN_AMOUNT_MISSING"))?;
        let pubkey = |s: &str| -> Result<Option<Pubkey>, ChainError> {
            if s.is_empty() {
                return Ok(None);
            }
            s.parse()
                .map(Some)
                .map_err(|_| ChainError("GEYSER_TOKEN_KEY"))
        };
        Ok(TokenBalance {
            account_index: u8_of(b.account_index)?,
            mint: pubkey(&b.mint)?.ok_or(ChainError("GEYSER_TOKEN_MINT_MISSING"))?,
            owner: pubkey(&b.owner)?,
            program_id: pubkey(&b.program_id)?,
            amount: ui
                .amount
                .parse()
                .map_err(|_| ChainError("GEYSER_TOKEN_AMOUNT"))?,
            decimals: u8_of(ui.decimals)?,
        })
    };
    let inner = if meta.inner_instructions_none {
        None
    } else {
        Some(
            meta.inner_instructions
                .iter()
                .map(|i| {
                    Ok(InnerInstructions {
                        index: u8_of(i.index)?,
                        instructions: i
                            .instructions
                            .iter()
                            .map(|x| {
                                Ok(ChainInstruction {
                                    program_id_index: u8_of(x.program_id_index)?,
                                    accounts: x.accounts.clone(),
                                    data: x.data.clone(),
                                    stack_height: x.stack_height,
                                })
                            })
                            .collect::<Result<_, ChainError>>()?,
                    })
                })
                .collect::<Result<_, ChainError>>()?,
        )
    };
    Ok(ChainMeta {
        status,
        fee: Some(meta.fee),
        pre_balances: Some(meta.pre_balances),
        post_balances: Some(meta.post_balances),
        pre_token_balances: Some(
            meta.pre_token_balances
                .iter()
                .map(token)
                .collect::<Result<_, _>>()?,
        ),
        post_token_balances: Some(
            meta.post_token_balances
                .iter()
                .map(token)
                .collect::<Result<_, _>>()?,
        ),
        inner_instructions: inner,
        log_messages: (!meta.log_messages_none).then_some(meta.log_messages),
        loaded_addresses: Some(LoadedAddresses {
            writable: meta
                .loaded_writable_addresses
                .iter()
                .map(key)
                .collect::<Result<_, _>>()?,
            readonly: meta
                .loaded_readonly_addresses
                .iter()
                .map(key)
                .collect::<Result<_, _>>()?,
        }),
        compute_units_consumed: meta.compute_units_consumed,
        cost_units: meta.cost_units,
    })
}

/// The `Subscribe` request of a provider-neutral `Subscription` (protobuf bytes): program owners
/// (with their filters), single accounts, slots and transactions mentioning programs; votes and
/// failed transactions excluded; `from_slot` only when the source proves replay.
pub fn subscribe_request(
    s: &Subscription,
    commitment: proto::CommitmentLevel,
    from_slot: Option<u64>,
) -> Vec<u8> {
    use proto::*;
    let mut accounts = BTreeMap::new();
    if !s.programs.is_empty() || !s.accounts.is_empty() {
        accounts.insert(
            "accounts".to_string(),
            SubscribeRequestFilterAccounts {
                account: s.accounts.iter().map(ToString::to_string).collect(),
                owner: s.programs.iter().map(ToString::to_string).collect(),
                filters: vec![],
            },
        );
    }
    for (n, (program, filters)) in s.program_filters.iter().enumerate() {
        let filters = filters
            .iter()
            .map(|f| SubscribeRequestFilterAccountsFilter {
                filter: Some(match f {
                    super::ProgramFilter::DataSize(n) => accounts_filter::Filter::Datasize(*n),
                    super::ProgramFilter::Memcmp { offset, bytes } => {
                        accounts_filter::Filter::Memcmp(Memcmp {
                            offset: *offset as u64,
                            bytes: Some(bytes.clone()),
                        })
                    }
                }),
            })
            .collect();
        accounts.insert(
            format!("program_{n}"),
            SubscribeRequestFilterAccounts {
                account: vec![],
                owner: vec![program.to_string()],
                filters,
            },
        );
    }
    let mut slots = BTreeMap::new();
    if s.slots || s.roots {
        slots.insert(
            "slots".to_string(),
            SubscribeRequestFilterSlots {
                filter_by_commitment: Some(false),
            },
        );
    }
    let mut transactions = BTreeMap::new();
    if !s.transactions_mentioning.is_empty() {
        transactions.insert(
            "transactions".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: None,
                account_include: s
                    .transactions_mentioning
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
            },
        );
    }
    SubscribeRequest {
        accounts,
        slots,
        transactions,
        commitment: Some(commitment as i32),
        ping: None,
        from_slot,
    }
    .encode_to_vec()
}

/// K-INGRESS-PRE gate (`SubscribeDeshred`): only a server of Yellowstone geyser 14.2.0 or later
/// resolves ALT addresses in deshred transactions (plan pin, changelog `2a548669`). No runtime here.
pub fn deshred_supported(server_version: &str) -> bool {
    let mut parts = server_version
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .map(|p| p.parse::<u64>());
    match (parts.next(), parts.next()) {
        (Some(Ok(major)), Some(Ok(minor))) => (major, minor) >= (14, 2),
        _ => false,
    }
}

/// Where and how the source connects. The token only from the environment (never logged).
#[derive(Clone, Debug)]
pub struct GeyserConfig {
    pub endpoint: String,
    pub token_env: &'static str,
    pub commitment: proto::CommitmentLevel,
    pub max_message_bytes: usize,
    pub max_reconnects: u32,
    /// The source proved `from_slot` replay: a reconnect resumes instead of a gap.
    pub replay_from_slot: bool,
}

impl GeyserConfig {
    /// The first FREE_ONLY candidate (S1-08); its capability is NOT_RUN.
    pub fn publicnode() -> Self {
        Self {
            endpoint: "https://solana.publicnode.com".into(),
            token_env: "P0_GEYSER_TOKEN",
            commitment: proto::CommitmentLevel::Processed,
            max_message_bytes: 16 << 20,
            max_reconnects: 5,
            replay_from_slot: false,
        }
    }
}

/// Why a stream or connection failed: a retry may help, or the source is rejected with a reason
/// no retry fixes (auth, quota, capability, configuration).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamError {
    Retryable(String),
    Rejected(&'static str),
}

/// One gRPC `Subscribe` stream: response DATA chunks and the pong a server ping asks for.
pub trait GeyserStream {
    /// `Ok(None)`: the stream ended.
    fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, StreamError>;
    fn pong(&mut self, id: i32) -> Result<(), StreamError>;
}

/// Opens a `Subscribe` stream with the encoded request (`geyser_grpc::GrpcTransport`).
pub trait GeyserTransport {
    type Stream: GeyserStream;
    fn connect(&mut self, request: &[u8]) -> Result<Self::Stream, StreamError>;
}

pub struct BinaryGeyserSource<T: GeyserTransport> {
    pub config: GeyserConfig,
    pub subscription: Subscription,
    pub transport: T,
    pub clock: Clock,
}

/// Delivers one event; a full sink halts out of band (never a silent drop), like the WSS source.
fn deliver(
    sink: &Producer<IngressEvent>,
    halt: &IngressHalt,
    slots: &SlotWatch,
    last_slot: Option<u64>,
    ev: IngressEvent,
    t_rx_ns: u64,
) -> Result<bool, IngressError> {
    if let IngressEvent::Slot(s) = &ev {
        slots.observe(s.slot);
    }
    match sink.try_push(ev) {
        Ok(()) => Ok(true),
        Err(ChannelError::Full(_)) => {
            halt.raise();
            sink.give_up();
            if let Err(ChannelError::Full(_)) = sink.try_push(IngressEvent::Gap {
                source: SourceKind::Geyser,
                from_slot: last_slot,
                to_slot: None,
                t_rx_ns,
            }) {
                sink.give_up();
            }
            Err(IngressError::Protocol("GEYSER_SINK_OVERFLOW".into()))
        }
        Err(ChannelError::Disconnected(_)) => Ok(false),
    }
}

impl<T: GeyserTransport> IngressSource for BinaryGeyserSource<T> {
    fn kind(&self) -> SourceKind {
        SourceKind::Geyser
    }

    fn run(
        mut self,
        sink: Producer<IngressEvent>,
        shutdown: Shutdown,
        halt: IngressHalt,
        slots: SlotWatch,
    ) -> Result<(), IngressError> {
        let mut mapper = GeyserMapper::new();
        let fatal = |detail: &str, t: u64| {
            halt.raise();
            let _ = sink.try_push(IngressEvent::Fatal {
                source: SourceKind::Geyser,
                detail: detail.into(),
                t_rx_ns: t,
            });
            IngressError::Protocol(detail.into())
        };
        let mut attempt = 0u32;
        loop {
            if shutdown.is_requested() {
                return Ok(());
            }
            attempt += 1;
            if attempt > self.config.max_reconnects.saturating_add(1) {
                return Err(fatal("GEYSER_RECONNECTS_EXHAUSTED", self.clock.now_ns()));
            }
            let resume = (self.config.replay_from_slot && attempt > 1)
                .then_some(mapper.max_slot)
                .flatten();
            let request = subscribe_request(&self.subscription, self.config.commitment, resume);
            let mut stream = match self.transport.connect(&request) {
                Ok(s) => s,
                Err(StreamError::Rejected(code)) => return Err(fatal(code, self.clock.now_ns())),
                Err(StreamError::Retryable(_)) => continue,
            };
            let t = self.clock.now_ns();
            // StreamReady after every connect (the WSS contract: the owner's recovery starts on
            // it); a reconnect without proven replay also opens a gap.
            let ready = IngressEvent::Reconnected {
                source: SourceKind::Geyser,
                attempt,
                t_rx_ns: t,
            };
            if !deliver(&sink, &halt, &slots, mapper.max_slot, ready, t)? {
                return Ok(());
            }
            if attempt > 1 && !self.config.replay_from_slot {
                // No proven replay: everything since the last slot must be recovered.
                let gap = IngressEvent::Gap {
                    source: SourceKind::Geyser,
                    from_slot: mapper.max_slot,
                    to_slot: None,
                    t_rx_ns: t,
                };
                if !deliver(&sink, &halt, &slots, mapper.max_slot, gap, t)? {
                    return Ok(());
                }
            }
            let mut frames = FrameDecoder::new(self.config.max_message_bytes);
            let mut ping_id = 0i32;
            loop {
                if shutdown.is_requested() {
                    return Ok(());
                }
                match stream.next_chunk() {
                    Ok(Some(chunk)) => frames.push(&chunk),
                    Err(StreamError::Rejected(code)) => {
                        return Err(fatal(code, self.clock.now_ns()))
                    }
                    Ok(None) | Err(StreamError::Retryable(_)) => break,
                }
                loop {
                    let update = match frames.next_message::<proto::SubscribeUpdate>() {
                        Ok(Some(u)) => u,
                        Ok(None) => break,
                        Err(code) => return Err(fatal(code, self.clock.now_ns())),
                    };
                    let t_rx = self.clock.now_ns();
                    let mapped = match mapper.map(update, t_rx) {
                        Ok(m) => m,
                        Err(code) => return Err(fatal(code, t_rx)),
                    };
                    if mapped.ping {
                        ping_id = ping_id.wrapping_add(1);
                        match stream.pong(ping_id) {
                            Ok(()) => {}
                            Err(StreamError::Rejected(code)) => {
                                return Err(fatal(code, self.clock.now_ns()))
                            }
                            Err(StreamError::Retryable(_)) => break,
                        }
                    }
                    for ev in mapped.events {
                        if !deliver(&sink, &halt, &slots, mapper.max_slot, ev, t_rx)? {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "geyser_tests.rs"]
mod tests;
