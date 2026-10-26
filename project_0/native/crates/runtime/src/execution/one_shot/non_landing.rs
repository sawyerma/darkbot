//! DEC-03 (V2-15/P7-02, N-2): the non-landing verdict of one FE-04 attempt or exit.
//!
//! * **B** — every configured FREE_ONLY source, one per operator, is classified on its own:
//!   *usable* (its finalized block height is beyond the signed blockhash's
//!   `lastValidBlockHeight`, its history floor `minimumLedgerSlot` is at or before the intent's
//!   state slot, every bound signature is absent with history search and
//!   `getTransaction(finalized)` is null for each), *unusable* (unreachable, rate limited, pruned,
//!   not yet expired, a pending status or otherwise faulty — no confirmation) or a *positive
//!   sighting* (a bound signature finalized present).
//! * **C** — always: a gap-free finalized canonical walk from the state slot to the first block
//!   whose height is beyond `lastValidBlockHeight`. Every block is read; its parent slot and
//!   parent hash continue the previous block (skipped slots are proven by the parent link, the
//!   first block's parent lies before the state slot), its height is known and one above its
//!   parent's, and no bound signature is in it. Never concluded from a pruned or null answer.
//! * **Verdict** — exactly one of five classes: a positive sighting anywhere is a veto
//!   (landing confirmed by the regular finalized receipts, or a contradiction left open);
//!   otherwise a complete negative C releases non-landing — with two usable B sources of
//!   different operators as `B+C`, else as C alone; anything else is UNKNOWN (holds stay).
//!
//! The reads go through the budgeted deadline transport (`Workload::Reconcile`): C over the
//! reconciliation endpoint and the provider budget, each B source over its own budget of the
//! same limits (another operator: its 429 never cools the provider), with calls and bytes.
use darkbot_solana::provider::{Budget, CallError, DeadlineRpc, Workload};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_client::rpc_request::RpcRequest;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Why a B source gives no confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UnusableReason {
    Unreachable,
    RateLimited,
    /// `minimumLedgerSlot` after the state slot: the history does not reach back.
    Pruned,
    /// The finalized height is not beyond `lastValidBlockHeight` yet.
    NotExpired,
    /// A bound signature has a status that is not finalized.
    SignaturePending,
    Faulty,
}

impl UnusableReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "UNREACHABLE",
            Self::RateLimited => "RATE_LIMITED",
            Self::Pruned => "PRUNED",
            Self::NotExpired => "NOT_EXPIRED",
            Self::SignaturePending => "SIGNATURE_PENDING",
            Self::Faulty => "FAULTY",
        }
    }
}

/// A failed read of a source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    Unreachable,
    RateLimited,
    Faulty,
}

impl From<ReadError> for UnusableReason {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Unreachable => Self::Unreachable,
            ReadError::RateLimited => Self::RateLimited,
            ReadError::Faulty => Self::Faulty,
        }
    }
}

/// A bound signature's status at one source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureStatus {
    Absent,
    Finalized,
    /// Processed or confirmed, not finalized.
    Pending,
}

/// The four reads of a B source (one operator's FREE_ONLY endpoint).
pub trait BReads {
    fn finalized_block_height(&self) -> Result<u64, ReadError>;
    fn minimum_ledger_slot(&self) -> Result<u64, ReadError>;
    /// One status per signature, in order, searched with history.
    fn signature_statuses(&self, signatures: &[String]) -> Result<Vec<SignatureStatus>, ReadError>;
    /// `getTransaction(finalized)` is non-null.
    fn finalized_transaction_present(&self, signature: &str) -> Result<bool, ReadError>;
}

/// What is proven about one attempt: its bound signatures, the expiry of its blockhash and the
/// slot of the state it was evaluated on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub signatures: Vec<String>,
    pub last_valid_block_height: u64,
    pub state_slot: u64,
}

/// One B source's class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BClass {
    Usable,
    Unusable(UnusableReason),
    /// The bound signature the source reports finalized present.
    Sighting(String),
}

impl BClass {
    /// `USABLE`, `UNUSABLE_<reason>` or `SIGHTING`.
    pub fn code(&self) -> String {
        match self {
            Self::Usable => "USABLE".into(),
            Self::Unusable(r) => format!("UNUSABLE_{}", r.code()),
            Self::Sighting(_) => "SIGHTING".into(),
        }
    }
}

/// One B source's class under its operator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BSource {
    pub operator: String,
    pub class: BClass,
}

/// Classifies one B source. Every read it can make is made (a sighting is a veto whatever the
/// other criteria say); a failed read ends it as unusable.
pub fn classify_b(reads: &dyn BReads, s: &Subject) -> BClass {
    let mut unusable: Option<UnusableReason> = None;
    let mut note = |r: UnusableReason| {
        unusable.get_or_insert(r);
    };
    match reads.finalized_block_height() {
        Ok(h) if h > s.last_valid_block_height => {}
        Ok(_) => note(UnusableReason::NotExpired),
        Err(e) => return BClass::Unusable(e.into()),
    }
    match reads.minimum_ledger_slot() {
        Ok(floor) if floor <= s.state_slot => {}
        Ok(_) => note(UnusableReason::Pruned),
        Err(e) => return BClass::Unusable(e.into()),
    }
    let statuses = match reads.signature_statuses(&s.signatures) {
        Ok(v) if v.len() == s.signatures.len() => v,
        Ok(_) => return BClass::Unusable(UnusableReason::Faulty),
        Err(e) => return BClass::Unusable(e.into()),
    };
    for (signature, status) in s.signatures.iter().zip(&statuses) {
        match status {
            SignatureStatus::Finalized => return BClass::Sighting(signature.clone()),
            SignatureStatus::Pending => note(UnusableReason::SignaturePending),
            SignatureStatus::Absent => {}
        }
    }
    for signature in &s.signatures {
        match reads.finalized_transaction_present(signature) {
            Ok(true) => return BClass::Sighting(signature.clone()),
            Ok(false) => {}
            Err(e) => return BClass::Unusable(unusable.unwrap_or(e.into())),
        }
    }
    match unusable {
        Some(r) => BClass::Unusable(r),
        None => BClass::Usable,
    }
}

/// A finalized block as C reads it: its links, its height and the first signature of each of
/// its transactions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSignatures {
    pub slot: u64,
    pub parent_slot: u64,
    pub blockhash: String,
    pub previous_blockhash: String,
    /// `None`: the node gave no height (unproven).
    pub block_height: Option<u64>,
    pub signatures: Vec<String>,
}

/// The finalized canonical chain as C reads it.
pub trait CReads {
    fn finalized_block_height(&self) -> Result<u64, ReadError>;
    /// Up to `limit` finalized block slots from `start` on, ascending.
    fn blocks_from(&self, start: u64, limit: u64) -> Result<Vec<u64>, ReadError>;
    /// The block at `slot`; `None`: no block (skipped or missing).
    fn block(&self, slot: u64) -> Result<Option<BlockSignatures>, ReadError>;
}

/// Why the C walk is incomplete (never a proof).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CIncomplete {
    Unreachable,
    RateLimited,
    Faulty,
    /// The finalized chain is not beyond the expiry height yet.
    NotExpired,
    /// A listed block could not be read, or no block was listed.
    BlockMissing,
    /// A block does not continue its predecessor (parent slot or hash).
    Gap,
    /// A block's height is not one above its predecessor's.
    HeightGap,
    HeightUnknown,
}

impl CIncomplete {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreachable => "C_UNREACHABLE",
            Self::RateLimited => "C_RATE_LIMITED",
            Self::Faulty => "C_FAULTY",
            Self::NotExpired => "C_NOT_EXPIRED",
            Self::BlockMissing => "C_BLOCK_MISSING",
            Self::Gap => "C_GAP",
            Self::HeightGap => "C_HEIGHT_GAP",
            Self::HeightUnknown => "C_HEIGHT_UNKNOWN",
        }
    }
}

impl From<ReadError> for CIncomplete {
    fn from(e: ReadError) -> Self {
        match e {
            ReadError::Unreachable => Self::Unreachable,
            ReadError::RateLimited => Self::RateLimited,
            ReadError::Faulty => Self::Faulty,
        }
    }
}

/// C's result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CResult {
    /// Every block from the state slot through `through_slot` read and linked, none carries a
    /// bound signature, and `through_slot`'s height is beyond the blockhash's expiry.
    Negative {
        blocks: u64,
        through_slot: u64,
    },
    Sighting {
        signature: String,
        slot: u64,
    },
    Incomplete(CIncomplete),
}

impl CResult {
    /// `NEGATIVE`, `SIGHTING` or the incomplete code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Negative { .. } => "NEGATIVE",
            Self::Sighting { .. } => "SIGHTING",
            Self::Incomplete(i) => i.code(),
        }
    }
}

/// The C walk (N-2): from the state slot to the first finalized block beyond the expiry height,
/// only once the finalized chain is beyond it. Block heights rise by one per block, so each
/// listing asks for exactly the blocks still needed.
pub fn scan_c(reads: &dyn CReads, s: &Subject) -> CResult {
    use CIncomplete as I;
    let bound: BTreeSet<&str> = s.signatures.iter().map(String::as_str).collect();
    match reads.finalized_block_height() {
        Ok(h) if h > s.last_valid_block_height => {}
        Ok(_) => return CResult::Incomplete(I::NotExpired),
        Err(e) => return CResult::Incomplete(e.into()),
    }
    let mut previous: Option<(u64, String, u64)> = None;
    let mut start = s.state_slot;
    let mut read = 0u64;
    loop {
        // The first block, then every block up to the height beyond the expiry.
        let limit = previous.as_ref().map_or(1, |(_, _, height)| {
            s.last_valid_block_height
                .saturating_add(1)
                .saturating_sub(*height)
        });
        let slots = match reads.blocks_from(start, limit) {
            Ok(v) if !v.is_empty() => v,
            Ok(_) => return CResult::Incomplete(I::BlockMissing),
            Err(e) => return CResult::Incomplete(e.into()),
        };
        for slot in slots {
            let block = match reads.block(slot) {
                Ok(Some(b)) if b.slot == slot => b,
                Ok(_) => return CResult::Incomplete(I::BlockMissing),
                Err(e) => return CResult::Incomplete(e.into()),
            };
            // A bound signature in any finalized block is a veto, linked or not.
            if let Some(signature) = block.signatures.iter().find(|x| bound.contains(x.as_str())) {
                return CResult::Sighting {
                    signature: signature.clone(),
                    slot,
                };
            }
            let linked = match &previous {
                // The first block's parent lies before the state slot: every slot between is
                // skipped.
                None => block.parent_slot < s.state_slot,
                Some((p_slot, p_hash, _)) => {
                    block.parent_slot == *p_slot && block.previous_blockhash == *p_hash
                }
            };
            if !linked {
                return CResult::Incomplete(I::Gap);
            }
            read += 1;
            let Some(height) = block.block_height else {
                return CResult::Incomplete(I::HeightUnknown);
            };
            if previous
                .as_ref()
                .is_some_and(|(_, _, p)| p.checked_add(1) != Some(height))
            {
                return CResult::Incomplete(I::HeightGap);
            }
            if height > s.last_valid_block_height {
                return CResult::Negative {
                    blocks: read,
                    through_slot: slot,
                };
            }
            start = slot.saturating_add(1);
            previous = Some((slot, block.blockhash, height));
        }
    }
}

/// The five result classes of N-2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Two usable B sources of different operators and C, all negative.
    BcConfirmed,
    /// Fewer than two usable B sources; C alone complete and negative.
    CAloneQualified,
    /// A positive sighting, and the landing is confirmed by the finalized receipts: the regular
    /// FE-04 reconciliation; capital only through the journal settlement.
    VetoLandingConfirmed,
    /// A positive sighting without a confirmed landing: UNKNOWN, holds stay, a contradiction.
    VetoContradiction,
    /// C incomplete (or any other missing evidence): UNKNOWN, holds stay.
    UnknownIncomplete,
}

impl Verdict {
    pub fn code(self) -> &'static str {
        match self {
            Self::BcConfirmed => "B_C_CONFIRMED",
            Self::CAloneQualified => "C_ALONE_QUALIFIED",
            Self::VetoLandingConfirmed => "VETO_LANDING_CONFIRMED",
            Self::VetoContradiction => "VETO_CONTRADICTION_OPEN",
            Self::UnknownIncomplete => "UNKNOWN_INCOMPLETE",
        }
    }
    /// Only these two release a non-landing (N-2 §3).
    pub fn proves_non_landing(self) -> bool {
        matches!(self, Self::BcConfirmed | Self::CAloneQualified)
    }
}

/// The verdict of one read (N-2 §2–§4). `landing_confirmed`: the regular finalized receipts
/// observed every bound signature with complete metadata.
pub fn verdict(b: &[BSource], c: &CResult, landing_confirmed: bool) -> Verdict {
    let sighting = matches!(c, CResult::Sighting { .. })
        || b.iter().any(|s| matches!(s.class, BClass::Sighting(_)));
    if sighting {
        return if landing_confirmed {
            Verdict::VetoLandingConfirmed
        } else {
            Verdict::VetoContradiction
        };
    }
    if !matches!(c, CResult::Negative { .. }) {
        return Verdict::UnknownIncomplete;
    }
    if usable_operators(b).len() >= 2 {
        Verdict::BcConfirmed
    } else {
        Verdict::CAloneQualified
    }
}

fn usable_operators(b: &[BSource]) -> BTreeSet<String> {
    b.iter()
        .filter(|s| s.class == BClass::Usable)
        .map(|s| s.operator.clone())
        .collect()
}

/// One non-landing read (evidence only; the journal decides): the subject it was made for,
/// every B source's class, C's result and what it cost.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonLandingRead {
    pub subject: Subject,
    pub b: Vec<BSource>,
    pub c: CResult,
    pub calls: u64,
    pub bytes: u64,
}

/// What a journal persists with a released non-landing: its class (`B+C` or C alone), the
/// independent usable B operators and C's walk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonLandingRelease {
    pub class: Verdict,
    pub usable_operators: BTreeSet<String>,
    pub c_blocks: u64,
    pub c_through_slot: u64,
}

impl NonLandingRead {
    /// Nothing observed: this read's class (a sighting here is an open contradiction).
    pub fn verdict(&self) -> Verdict {
        verdict(&self.b, &self.c, false)
    }
    /// The release this read qualifies — only for exactly `subject` and only when it proves
    /// non-landing (N-2 §3).
    pub fn release(&self, subject: &Subject) -> Option<NonLandingRelease> {
        let class = self.verdict();
        match self.c {
            CResult::Negative {
                blocks,
                through_slot,
            } if &self.subject == subject && class.proves_non_landing() => {
                Some(NonLandingRelease {
                    class,
                    usable_operators: usable_operators(&self.b),
                    c_blocks: blocks,
                    c_through_slot: through_slot,
                })
            }
            _ => None,
        }
    }
}

/// A non-landing source over the budgeted deadline transport.
pub struct RpcSource {
    rpc: DeadlineRpc,
}

/// The transport failure as a read failure: a 429 or the provider's rate limit, a connection
/// or HTTP failure or the deadline, anything else (a node error, a malformed answer).
fn read_error(e: CallError) -> ReadError {
    match e {
        // A 429, or the source's cooldown after one (also when it outlasts the deadline).
        CallError::Budget("RPC_RATE_LIMIT") | CallError::Deadline("RPC_RATE_LIMIT") => {
            ReadError::RateLimited
        }
        CallError::Deadline(_)
        | CallError::Transport(
            "RPC_TRANSPORT" | "RPC_HTTP_ERROR" | "RPC_BODY_READ" | "RPC_RETRY_EXHAUSTED",
        ) => ReadError::Unreachable,
        _ => ReadError::Faulty,
    }
}

impl RpcSource {
    pub fn new(rpc: DeadlineRpc) -> Self {
        Self { rpc }
    }
    /// One call bounded by the configured per-call timeout.
    fn call(&self, request: RpcRequest, params: Value) -> Result<Value, ReadError> {
        let timeout = Duration::from_millis(self.rpc.budget().limits().rpc_timeout_ms);
        self.rpc
            .call(request, params, Instant::now() + timeout)
            .map_err(read_error)
    }
    fn height(&self) -> Result<u64, ReadError> {
        self.call(
            RpcRequest::GetBlockHeight,
            json!([{"commitment": "finalized"}]),
        )?
        .as_u64()
        .ok_or(ReadError::Faulty)
    }
    /// Calls and bytes of this source so far.
    fn spent(&self) -> (u64, u64) {
        (self.rpc.request_count() as u64, self.rpc.bytes())
    }
}

impl BReads for RpcSource {
    fn finalized_block_height(&self) -> Result<u64, ReadError> {
        self.height()
    }
    fn minimum_ledger_slot(&self) -> Result<u64, ReadError> {
        self.call(RpcRequest::MinimumLedgerSlot, json!([]))?
            .as_u64()
            .ok_or(ReadError::Faulty)
    }
    fn signature_statuses(&self, signatures: &[String]) -> Result<Vec<SignatureStatus>, ReadError> {
        let r = self.call(
            RpcRequest::GetSignatureStatuses,
            json!([signatures, {"searchTransactionHistory": true}]),
        )?;
        r["value"]
            .as_array()
            .ok_or(ReadError::Faulty)?
            .iter()
            .map(|status| match status {
                Value::Null => Ok(SignatureStatus::Absent),
                Value::Object(o) => Ok(match o.get("confirmationStatus") {
                    Some(c) if c == "finalized" => SignatureStatus::Finalized,
                    Some(_) => SignatureStatus::Pending,
                    // A node without the field: rooted is `confirmations: null`.
                    None if o.get("confirmations") == Some(&Value::Null) => {
                        SignatureStatus::Finalized
                    }
                    None => SignatureStatus::Pending,
                }),
                _ => Err(ReadError::Faulty),
            })
            .collect()
    }
    fn finalized_transaction_present(&self, signature: &str) -> Result<bool, ReadError> {
        match self.call(
            RpcRequest::GetTransaction,
            json!([signature, {"encoding": "base64", "commitment": "finalized",
                "maxSupportedTransactionVersion": 0}]),
        )? {
            Value::Null => Ok(false),
            Value::Object(_) => Ok(true),
            _ => Err(ReadError::Faulty),
        }
    }
}

impl CReads for RpcSource {
    fn finalized_block_height(&self) -> Result<u64, ReadError> {
        self.height()
    }
    fn blocks_from(&self, start: u64, limit: u64) -> Result<Vec<u64>, ReadError> {
        self.call(
            RpcRequest::GetBlocksWithLimit,
            json!([start, limit, {"commitment": "finalized"}]),
        )?
        .as_array()
        .ok_or(ReadError::Faulty)?
        .iter()
        .map(|v| v.as_u64().ok_or(ReadError::Faulty))
        .collect()
    }
    fn block(&self, slot: u64) -> Result<Option<BlockSignatures>, ReadError> {
        let b = self.call(
            RpcRequest::GetBlock,
            json!([slot, {"encoding": "json", "transactionDetails": "signatures",
                "rewards": false, "commitment": "finalized",
                "maxSupportedTransactionVersion": 0}]),
        )?;
        if b.is_null() {
            return Ok(None);
        }
        let text = |k: &str| b[k].as_str().map(str::to_string).ok_or(ReadError::Faulty);
        Ok(Some(BlockSignatures {
            slot,
            parent_slot: b["parentSlot"].as_u64().ok_or(ReadError::Faulty)?,
            blockhash: text("blockhash")?,
            previous_blockhash: text("previousBlockhash")?,
            block_height: b["blockHeight"].as_u64(),
            signatures: b["signatures"]
                .as_array()
                .ok_or(ReadError::Faulty)?
                .iter()
                .map(|v| v.as_str().map(str::to_string).ok_or(ReadError::Faulty))
                .collect::<Result<_, _>>()?,
        }))
    }
}

/// The settlement worker's non-landing sources: C over the reconciliation endpoint, the B
/// sources per operator; C's final result of the latest subject (a negative walk or a sighting
/// over finalized blocks never changes, so a retry reads only B).
pub struct NonLandingSources {
    c: RpcSource,
    b: Vec<(String, RpcSource)>,
    c_final: std::cell::RefCell<Option<(Subject, CResult)>>,
}

impl NonLandingSources {
    /// From the configuration: C on `rpc_url` against the provider budget, each configured B
    /// source (`non_landing_sources`) against its own budget of the configured limits.
    pub fn configured(
        rpc_url: String,
        config: &darkbot_solana::runtime_config::RuntimeConfig,
    ) -> Result<Self, String> {
        let c = RpcSource::new(DeadlineRpc::new(rpc_url, Workload::Reconcile)?);
        // Its own endpoint only: no fallback to the provider's endpoints.
        let own = darkbot_solana::runtime_config::RuntimeConfig {
            rpc_endpoints: vec![],
            ..config.clone()
        };
        let b = config
            .non_landing_source_list()?
            .into_iter()
            .map(|(operator, url)| {
                let budget = Arc::new(Budget::configured(&own));
                DeadlineRpc::with_budget(url, Workload::Reconcile, budget)
                    .map(|rpc| (operator, RpcSource::new(rpc)))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            c,
            b,
            c_final: Default::default(),
        })
    }
    #[cfg(test)]
    pub(crate) fn of(c: RpcSource, b: Vec<(String, RpcSource)>) -> Self {
        Self {
            c,
            b,
            c_final: Default::default(),
        }
    }
    /// One read: every B source, then C (its final result of this subject is not read again);
    /// the calls and bytes it took.
    pub fn read(&self, subject: &Subject) -> NonLandingRead {
        let all = || std::iter::once(&self.c).chain(self.b.iter().map(|(_, r)| r));
        let before: Vec<(u64, u64)> = all().map(RpcSource::spent).collect();
        let b = self
            .b
            .iter()
            .map(|(operator, reads)| BSource {
                operator: operator.clone(),
                class: classify_b(reads, subject),
            })
            .collect();
        let cached = self
            .c_final
            .borrow()
            .as_ref()
            .filter(|(s, _)| s == subject)
            .map(|(_, c)| c.clone());
        let c = cached.unwrap_or_else(|| {
            let c = scan_c(&self.c, subject);
            if matches!(c, CResult::Negative { .. } | CResult::Sighting { .. }) {
                *self.c_final.borrow_mut() = Some((subject.clone(), c.clone()));
            }
            c
        });
        let (calls, bytes) = all()
            .map(RpcSource::spent)
            .zip(before)
            .fold((0, 0), |(c, b), ((c1, b1), (c0, b0))| {
                (c + c1.saturating_sub(c0), b + b1.saturating_sub(b0))
            });
        NonLandingRead {
            subject: subject.clone(),
            b,
            c,
            calls,
            bytes,
        }
    }
}

/// Loopback JSON-RPC endpoints of the non-landing reads (tests of the journal and the owner).
#[cfg(test)]
pub(crate) mod loopback {
    use super::*;
    use std::sync::Mutex;

    /// One answer: a JSON-RPC result, an HTTP status without body, or a JSON-RPC error.
    pub(crate) enum Reply {
        Result(Value),
        Status(u16),
        Error,
    }

    pub(crate) type Answer = Arc<dyn Fn(&str, &Value) -> Reply + Send + Sync>;

    /// A loopback endpoint answering every request through `answer`; the methods in call order.
    pub(crate) fn endpoint(answer: Answer) -> (String, Arc<Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let calls = Arc::new(Mutex::new(Vec::new()));
        let log = calls.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (answer, log) = (answer.clone(), log.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut stream = stream;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let mut length = 0usize;
                        loop {
                            let mut h = String::new();
                            if reader.read_line(&mut h).unwrap_or(0) == 0 {
                                return;
                            }
                            if h == "\r\n" {
                                break;
                            }
                            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:")
                            {
                                length = v.trim().parse().unwrap_or(0);
                            }
                        }
                        let mut body = vec![0u8; length];
                        if reader.read_exact(&mut body).is_err() {
                            return;
                        }
                        let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                        let method = request["method"].as_str().unwrap_or("").to_string();
                        log.lock().unwrap().push(method.clone());
                        let (status, body) = match answer(&method, &request["params"]) {
                            Reply::Result(r) => (
                                "200 OK",
                                json!({"jsonrpc": "2.0", "id": request["id"], "result": r})
                                    .to_string(),
                            ),
                            Reply::Status(429) => {
                                ("429 Too Many Requests\r\nretry-after: 5", String::new())
                            }
                            Reply::Status(_) => ("500 Internal Server Error", String::new()),
                            Reply::Error => (
                                "200 OK",
                                json!({"jsonrpc": "2.0", "id": request["id"],
                                    "error": {"code": -32601, "message": "unscripted"}})
                                .to_string(),
                            ),
                        };
                        let _ = stream.write_all(
                            format!(
                                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                                body.len()
                            )
                            .as_bytes(),
                        );
                    }
                });
            }
        });
        (url, calls)
    }

    /// A URL nothing listens on.
    pub(crate) fn closed() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    }

    /// A source over its own budget: one attempt per call (a 429 is final), a short timeout.
    pub(crate) fn source(url: String) -> RpcSource {
        source_with(url, 1, 2_000)
    }

    /// A source over its own budget of `attempts` per call and `timeout_ms`, paced for a
    /// loopback (the production pacing would only slow the tests down).
    pub(crate) fn source_with(url: String, attempts: u64, timeout_ms: u64) -> RpcSource {
        let mut config = darkbot_solana::runtime_config::RuntimeConfig::default();
        config.limits.rpc_attempts = attempts;
        config.limits.rpc_timeout_ms = timeout_ms;
        (config.limits.rpc_rps, config.limits.rpc_burst) = (1_000, 1_000);
        config.limits.analytics_rps = 1_000;
        RpcSource::new(DeadlineRpc::over(
            url,
            Workload::Reconcile,
            Arc::new(Budget::configured(&config)),
        ))
    }

    /// The finalized chain of the tests: from `first`, a block every second slot (the first's
    /// parent three slots before it), heights from `height`, `n` blocks; the finalized height
    /// is the last block's. `sighting`: a bound signature put into the block at that slot.
    /// Answers B's reads too: `floor` the history floor, every status null, no transaction.
    pub(crate) fn chain(
        first: u64,
        n: u64,
        height: u64,
        floor: u64,
        sighting: Option<(u64, String)>,
    ) -> Answer {
        Arc::new(move |method, params| {
            let slot_of = |i: u64| first + 2 * i;
            let block = |slot: u64| {
                let i = (slot - first) / 2;
                let parent = if i == 0 { first - 3 } else { slot - 2 };
                let mut signatures = vec![format!("other-{slot}")];
                if let Some((at, sig)) = &sighting {
                    if *at == slot {
                        signatures.push(sig.clone());
                    }
                }
                json!({"parentSlot": parent, "blockhash": format!("h-{slot}"),
                    "previousBlockhash": format!("h-{parent}"), "blockHeight": height + i,
                    "blockTime": null, "signatures": signatures})
            };
            Reply::Result(match method {
                "getBlockHeight" => json!(height + n - 1),
                "minimumLedgerSlot" => json!(floor),
                "getSignatureStatuses" => {
                    let n = params[0].as_array().map_or(0, Vec::len);
                    json!({"context": {"slot": first}, "value": vec![Value::Null; n]})
                }
                "getTransaction" => Value::Null,
                "getBlocksWithLimit" => {
                    let (start, limit) = (params[0].as_u64().unwrap(), params[1].as_u64().unwrap());
                    json!((0..n)
                        .map(slot_of)
                        .filter(|s| *s >= start)
                        .take(limit as usize)
                        .collect::<Vec<_>>())
                }
                "getBlock" => {
                    let slot = params[0].as_u64().unwrap();
                    if slot >= first && (slot - first).is_multiple_of(2) && (slot - first) / 2 < n {
                        block(slot)
                    } else {
                        Value::Null
                    }
                }
                _ => return Reply::Error,
            })
        })
    }

    /// The sources of a read: C over `c`, each B `(operator, url)`.
    pub(crate) fn sources(c: String, b: Vec<(&str, String)>) -> NonLandingSources {
        NonLandingSources::of(
            source(c),
            b.into_iter()
                .map(|(operator, url)| (operator.to_string(), source(url)))
                .collect(),
        )
    }

    /// A read of `subject` proving its non-landing: B+C with two operators, else C alone.
    pub(crate) fn proving(subject: &Subject, two_b: bool) -> NonLandingRead {
        NonLandingRead {
            subject: subject.clone(),
            b: if two_b {
                ["op-a", "op-b"]
                    .map(|operator| BSource {
                        operator: operator.into(),
                        class: BClass::Usable,
                    })
                    .into()
            } else {
                vec![]
            },
            c: CResult::Negative {
                blocks: 1,
                through_slot: subject.state_slot,
            },
            calls: 0,
            bytes: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct B {
        height: Result<u64, ReadError>,
        floor: Result<u64, ReadError>,
        statuses: Result<Vec<SignatureStatus>, ReadError>,
        transactions: BTreeMap<String, Result<bool, ReadError>>,
    }
    impl BReads for B {
        fn finalized_block_height(&self) -> Result<u64, ReadError> {
            self.height
        }
        fn minimum_ledger_slot(&self) -> Result<u64, ReadError> {
            self.floor
        }
        fn signature_statuses(&self, _: &[String]) -> Result<Vec<SignatureStatus>, ReadError> {
            self.statuses.clone()
        }
        fn finalized_transaction_present(&self, s: &str) -> Result<bool, ReadError> {
            self.transactions.get(s).copied().unwrap_or(Ok(false))
        }
    }
    fn subject() -> Subject {
        Subject {
            signatures: vec!["sig-a".into(), "sig-b".into()],
            last_valid_block_height: 1_000,
            state_slot: 500,
        }
    }
    fn negative() -> B {
        B {
            height: Ok(1_001),
            floor: Ok(400),
            statuses: Ok(vec![SignatureStatus::Absent; 2]),
            transactions: BTreeMap::new(),
        }
    }

    /// DEC-03 (N-2 §1): each B source has exactly one class — usable only with all four
    /// criteria, a finalized status or transaction is a sighting whatever else, every failed or
    /// unmet criterion is unusable with its reason.
    #[test]
    fn dec03_each_b_source_has_exactly_one_class() {
        let s = subject();
        assert_eq!(classify_b(&negative(), &s), BClass::Usable);
        let with = |f: fn(&mut B)| {
            let mut b = negative();
            f(&mut b);
            classify_b(&b, &s)
        };
        use UnusableReason as U;
        for (case, class) in [
            (
                with(|b| b.height = Ok(1_000)),
                BClass::Unusable(U::NotExpired),
            ),
            (with(|b| b.floor = Ok(501)), BClass::Unusable(U::Pruned)),
            (
                with(|b| b.height = Err(ReadError::RateLimited)),
                BClass::Unusable(U::RateLimited),
            ),
            (
                with(|b| b.floor = Err(ReadError::Unreachable)),
                BClass::Unusable(U::Unreachable),
            ),
            (
                with(|b| b.statuses = Ok(vec![SignatureStatus::Absent])),
                BClass::Unusable(U::Faulty),
            ),
            (
                with(|b| b.statuses = Ok(vec![SignatureStatus::Absent, SignatureStatus::Pending])),
                BClass::Unusable(U::SignaturePending),
            ),
            (
                with(|b| {
                    b.transactions
                        .insert("sig-b".into(), Err(ReadError::Faulty));
                }),
                BClass::Unusable(U::Faulty),
            ),
            (
                with(|b| {
                    b.statuses = Ok(vec![SignatureStatus::Absent, SignatureStatus::Finalized])
                }),
                BClass::Sighting("sig-b".into()),
            ),
            (
                with(|b| {
                    b.transactions.insert("sig-a".into(), Ok(true));
                }),
                BClass::Sighting("sig-a".into()),
            ),
            // A pruned source that still reports the signature finalized: a sighting (veto).
            (
                with(|b| {
                    b.floor = Ok(9_999);
                    b.statuses = Ok(vec![SignatureStatus::Finalized, SignatureStatus::Absent]);
                }),
                BClass::Sighting("sig-a".into()),
            ),
        ] {
            assert_eq!(case, class);
        }
    }

    struct C {
        height: Result<u64, ReadError>,
        blocks: BTreeMap<u64, BlockSignatures>,
        missing: BTreeSet<u64>,
        /// The listings asked for, `(start, limit)`.
        listings: std::cell::RefCell<Vec<(u64, u64)>>,
    }
    impl CReads for C {
        fn finalized_block_height(&self) -> Result<u64, ReadError> {
            self.height
        }
        fn blocks_from(&self, start: u64, limit: u64) -> Result<Vec<u64>, ReadError> {
            self.listings.borrow_mut().push((start, limit));
            Ok(self
                .blocks
                .keys()
                .copied()
                .chain(self.missing.iter().copied())
                .filter(|s| *s >= start)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(limit as usize)
                .collect())
        }
        fn block(&self, slot: u64) -> Result<Option<BlockSignatures>, ReadError> {
            Ok(self.blocks.get(&slot).cloned())
        }
    }
    /// A chain from `first` on: `n` blocks, every second slot skipped, heights from `height`.
    fn chain(first: u64, n: u64, height: u64) -> C {
        let mut blocks = BTreeMap::new();
        let mut parent = (first - 3, "h-before".to_string());
        for i in 0..n {
            let slot = first + 2 * i;
            let b = BlockSignatures {
                slot,
                parent_slot: parent.0,
                blockhash: format!("h-{slot}"),
                previous_blockhash: parent.1.clone(),
                block_height: Some(height + i),
                signatures: vec![format!("other-{slot}")],
            };
            parent = (slot, b.blockhash.clone());
            blocks.insert(slot, b);
        }
        C {
            height: Ok(height + n - 1),
            blocks,
            missing: BTreeSet::new(),
            listings: Default::default(),
        }
    }

    /// DEC-03 (N-2, C): the walk is negative only when every block from the state slot is read
    /// and linked (skips proven by the parent link, heights one apart) up to a height beyond
    /// the expiry; a bound signature is a sighting; a gap, a missing block, an unknown or
    /// jumping height, an unexpired chain or a failed read leaves it incomplete (never a
    /// proof). It lists exactly the blocks still needed.
    #[test]
    fn dec03_c_is_a_gap_free_canonical_walk_to_the_expiry() {
        use CIncomplete as I;
        let s = subject();
        // Heights 995.. from slot 500: the block at height 1001 is the 7th (slot 512).
        let walk = chain(500, 10, 995);
        assert_eq!(
            scan_c(&walk, &s),
            CResult::Negative {
                blocks: 7,
                through_slot: 512
            }
        );
        assert_eq!(*walk.listings.borrow(), [(500, 1), (501, 6)]);
        let mut seen = chain(500, 10, 995);
        seen.blocks
            .get_mut(&504)
            .unwrap()
            .signatures
            .push("sig-b".into());
        assert_eq!(
            scan_c(&seen, &s),
            CResult::Sighting {
                signature: "sig-b".into(),
                slot: 504
            }
        );
        let mut gap = chain(500, 10, 995);
        gap.blocks.remove(&504);
        assert_eq!(scan_c(&gap, &s), CResult::Incomplete(I::Gap));
        // A bound signature behind a gap is still a sighting (a veto, never only incomplete).
        gap.blocks
            .get_mut(&506)
            .unwrap()
            .signatures
            .push("sig-a".into());
        assert_eq!(
            scan_c(&gap, &s),
            CResult::Sighting {
                signature: "sig-a".into(),
                slot: 506
            }
        );
        let mut missing = chain(500, 10, 995);
        missing.blocks.remove(&504);
        missing.missing.insert(504);
        assert_eq!(scan_c(&missing, &s), CResult::Incomplete(I::BlockMissing));
        let mut forked = chain(500, 10, 995);
        forked.blocks.get_mut(&506).unwrap().previous_blockhash = "h-other".into();
        assert_eq!(scan_c(&forked, &s), CResult::Incomplete(I::Gap));
        // The first block's parent at or after the state slot: a block in between is missing.
        let mut late = chain(500, 10, 995);
        late.blocks.get_mut(&500).unwrap().parent_slot = 500;
        assert_eq!(scan_c(&late, &s), CResult::Incomplete(I::Gap));
        let mut heightless = chain(500, 10, 995);
        heightless.blocks.get_mut(&502).unwrap().block_height = None;
        assert_eq!(
            scan_c(&heightless, &s),
            CResult::Incomplete(I::HeightUnknown)
        );
        // A height that jumps past the expiry is no canonical successor.
        let mut jump = chain(500, 10, 995);
        jump.blocks.get_mut(&502).unwrap().block_height = Some(1_001);
        assert_eq!(scan_c(&jump, &s), CResult::Incomplete(I::HeightGap));
        let mut short = chain(500, 10, 995);
        short.height = Ok(1_000);
        assert_eq!(scan_c(&short, &s), CResult::Incomplete(I::NotExpired));
        assert!(
            short.listings.borrow().is_empty(),
            "no walk before the expiry"
        );
        // The finalized height is past the expiry, the node lists too few blocks.
        let mut cut = chain(500, 5, 995);
        cut.height = Ok(1_001);
        assert_eq!(scan_c(&cut, &s), CResult::Incomplete(I::BlockMissing));
        let mut down = chain(500, 10, 995);
        down.height = Err(ReadError::RateLimited);
        assert_eq!(scan_c(&down, &s), CResult::Incomplete(I::RateLimited));
    }

    /// DEC-03 (N-2 §7): the loopback matrix of the five result classes.
    #[test]
    fn dec03_the_n2_matrix_of_the_five_result_classes() {
        let s = subject();
        let source = |operator: &str, b: B| BSource {
            operator: operator.into(),
            class: classify_b(&b, &s),
        };
        let c_negative = scan_c(&chain(500, 10, 995), &s);
        let mut pruned = negative();
        pruned.floor = Ok(501);
        let mut limited = negative();
        limited.height = Err(ReadError::RateLimited);
        let mut down = negative();
        down.floor = Err(ReadError::Unreachable);
        let mut seeing = negative();
        seeing.statuses = Ok(vec![SignatureStatus::Finalized, SignatureStatus::Absent]);
        let mut c_seeing = chain(500, 10, 995);
        c_seeing
            .blocks
            .get_mut(&506)
            .unwrap()
            .signatures
            .push("sig-a".into());
        let c_seeing = scan_c(&c_seeing, &s);
        let mut cut = chain(500, 5, 995);
        cut.height = Ok(1_001);
        let c_incomplete = scan_c(&cut, &s);
        for (b, c, landed, expected) in [
            // Two B sources negative, C negative.
            (
                vec![source("op-a", negative()), source("op-b", negative())],
                &c_negative,
                false,
                Verdict::BcConfirmed,
            ),
            // One B pruned, rate limited or unreachable, C completely negative.
            (
                vec![source("op-a", negative()), source("op-b", pruned)],
                &c_negative,
                false,
                Verdict::CAloneQualified,
            ),
            (
                vec![source("op-a", negative()), source("op-b", limited)],
                &c_negative,
                false,
                Verdict::CAloneQualified,
            ),
            // Both B unusable, C completely negative.
            (
                vec![source("op-a", down)],
                &c_negative,
                false,
                Verdict::CAloneQualified,
            ),
            // Two usable B of one operator are not independent.
            (
                vec![source("op-a", negative()), source("op-a", negative())],
                &c_negative,
                false,
                Verdict::CAloneQualified,
            ),
            // C incomplete.
            (
                vec![source("op-a", negative()), source("op-b", negative())],
                &c_incomplete,
                false,
                Verdict::UnknownIncomplete,
            ),
            // A B source reports a signature, C negative, landing not confirmed.
            (
                vec![source("op-a", seeing), source("op-b", negative())],
                &c_negative,
                false,
                Verdict::VetoContradiction,
            ),
            // C finds the signature and the landing is confirmed.
            (
                vec![source("op-a", negative())],
                &c_seeing,
                true,
                Verdict::VetoLandingConfirmed,
            ),
        ] {
            let v = verdict(&b, c, landed);
            assert_eq!(v, expected, "{b:?} {c:?}");
            assert_eq!(
                v.proves_non_landing(),
                matches!(expected, Verdict::BcConfirmed | Verdict::CAloneQualified)
            );
        }
    }

    /// DEC-03: the reads over the budgeted transport (loopback) — the request order, one class
    /// per B source from the node's answers (a 429, a refused connection, a pruned history, a
    /// finalized or a pending status), C's walk listing exactly the blocks it needs, a C
    /// sighting, and the calls and bytes of the read.
    #[test]
    fn dec03_sources_read_over_the_transport() {
        use loopback::{chain, closed, endpoint, sources, Answer, Reply};
        let s = subject();
        let (c_url, c_calls) = endpoint(chain(500, 10, 995, 400, None));
        let (b1, b1_calls) = endpoint(chain(500, 10, 995, 400, None));
        let (b2, _) = endpoint(chain(500, 10, 995, 400, None));
        let both = sources(c_url.clone(), vec![("op-a", b1), ("op-b", b2)]);
        let read = both.read(&s);
        assert_eq!(read.verdict(), Verdict::BcConfirmed);
        assert_eq!(
            read.c,
            CResult::Negative {
                blocks: 7,
                through_slot: 512
            }
        );
        assert_eq!(
            *b1_calls.lock().unwrap(),
            [
                "getBlockHeight",
                "minimumLedgerSlot",
                "getSignatureStatuses",
                "getTransaction",
                "getTransaction"
            ]
        );
        let c = c_calls.lock().unwrap().clone();
        assert_eq!(
            c[..4],
            [
                "getBlockHeight",
                "getBlocksWithLimit",
                "getBlock",
                "getBlocksWithLimit"
            ]
        );
        assert_eq!(c.len(), 10, "{c:?}");
        assert!(c[4..].iter().all(|m| m == "getBlock"));
        // Two B sources of five calls, C of ten; every request and response byte counted.
        assert_eq!(read.calls, 20);
        assert!(read.bytes > 0);
        // C's final result of a subject is never walked again: a retry reads B only.
        let again = both.read(&s);
        assert_eq!((again.c, again.calls), (read.c.clone(), 10));
        assert_eq!(c_calls.lock().unwrap().len(), 10);
        let mut other = s.clone();
        other.state_slot = 502;
        both.read(&other);
        assert!(
            c_calls.lock().unwrap().len() > 10,
            "another subject is walked"
        );
        let status = |value: Value| -> Answer {
            let base = chain(500, 10, 995, 400, None);
            Arc::new(move |m, p| {
                if m == "getSignatureStatuses" {
                    Reply::Result(json!({"context": {"slot": 1}, "value": [null, value.clone()]}))
                } else {
                    base(m, p)
                }
            })
        };
        let (limited, _) = endpoint(Arc::new(|_, _| Reply::Status(429)));
        let (pruned, _) = endpoint(chain(500, 10, 995, 501, None));
        let (seeing, _) = endpoint(status(json!({"slot": 506, "confirmations": null,
            "err": null, "confirmationStatus": "finalized"})));
        let (pending, _) = endpoint(status(json!({"slot": 506, "confirmations": 3,
            "err": null, "confirmationStatus": "confirmed"})));
        let read = sources(
            c_url,
            vec![
                ("op-a", limited.clone()),
                ("op-b", closed()),
                ("op-c", pruned),
                ("op-d", seeing),
                ("op-e", pending),
            ],
        )
        .read(&s);
        let classes: Vec<BClass> = read.b.iter().map(|b| b.class.clone()).collect();
        assert_eq!(
            classes,
            [
                BClass::Unusable(UnusableReason::RateLimited),
                BClass::Unusable(UnusableReason::Unreachable),
                BClass::Unusable(UnusableReason::Pruned),
                BClass::Sighting("sig-b".into()),
                BClass::Unusable(UnusableReason::SignaturePending),
            ]
        );
        assert_eq!(read.verdict(), Verdict::VetoContradiction);
        // C: a bound signature in a block is a sighting; a rate-limited node is incomplete.
        let (seen, _) = endpoint(chain(500, 10, 995, 400, Some((506, "sig-a".into()))));
        assert_eq!(
            sources(seen, vec![]).read(&s).c,
            CResult::Sighting {
                signature: "sig-a".into(),
                slot: 506
            }
        );
        assert_eq!(
            sources(limited.clone(), vec![]).read(&s).c,
            CResult::Incomplete(CIncomplete::RateLimited)
        );
        // With retries, a cooldown (Retry-After) beyond the deadline is still a rate limit.
        let retrying = loopback::source_with(limited, 2, 500);
        assert_eq!(
            classify_b(&retrying, &s),
            BClass::Unusable(UnusableReason::RateLimited)
        );
    }
}
