//! The LIVE ledger (plan 28.10., package 6): every LIVE transaction whose
//! bytes may have left, and what the canonical chain made of it, as facts
//! written in order under the owner state (`live-ledger/`) and never
//! changed. An entry is published whole or not at all (BSC3-045). The
//! positions, their costs and the net result are the fold of these facts
//! ([`fold`]): after a crash or restart exactly as before it.
//!
//! - A BUY is a lot of its own, named by its hash (BSC3-047): included, it
//!   opens with the tokens the Portal traded to it; reverted, it failed and
//!   its gas is lost; replaced, it never happened (no lot). A token bought
//!   again after its exit is a new lot.
//! - An approval's and a SELL's gas count against their token's latest
//!   lot; a SELL included closes it with the native amount the Portal paid.
//! - A block that left the canonical chain takes its inclusions back: the
//!   transaction is unresolved again until it is included anew.
//! - An UNKNOWN transaction (its handoff did not complete) is unresolved,
//!   and visible, until the chain includes or replaces it.
//! - A WITHDRAWN transaction was admitted, but no byte of it left: it is
//!   resolved and never on chain.
//!
//! Every transaction the owner admitted is a fact here (BSC3-024): its
//! admission entry, durable before any byte can leave, names its token and
//! nonce, and one the ledger has no fact of (the process ended between the
//! admission and its outcome) is recovered as UNKNOWN ([`Ledger::recover`]).
//!
//! Realized PnL = proceeds − input − all gas of the position's
//! transactions; a failed BUY's gas is a loss of its own. A cost the
//! receipt did not state counts at its most (the signed gas limit).

use crate::sources::{Admitted, ComponentId, LiveIntentKind, NonceKey, NonceOrigin};
use alloy_primitives::U256;
use darkbot_bsc_record::schema::bsc_runtime::live_ledger as k;
use darkbot_bsc_record::{Draft, Value, decode};
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// What a fact says about its transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactKind {
    Released,
    Unknown,
    Withdrawn,
    Included,
    Reverted,
    Replaced,
    Orphaned,
    /// A provider answered the release (BSC3-051): no word on the chain.
    Responded,
}

impl FactKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Released => "RELEASED",
            Self::Unknown => "UNKNOWN",
            Self::Withdrawn => "WITHDRAWN",
            Self::Included => "INCLUDED",
            Self::Reverted => "REVERTED",
            Self::Replaced => "REPLACED",
            Self::Orphaned => "ORPHANED",
            Self::Responded => "RESPONDED",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [
            Self::Released,
            Self::Unknown,
            Self::Withdrawn,
            Self::Included,
            Self::Reverted,
            Self::Replaced,
            Self::Orphaned,
            Self::Responded,
        ]
        .into_iter()
        .find(|k| k.name() == text)
    }

    /// A transaction's first fact: what its release (or admission) made of
    /// it.
    fn first(self) -> bool {
        matches!(self, Self::Released | Self::Unknown | Self::Withdrawn)
    }
}

fn intent_of(text: &str) -> Option<LiveIntentKind> {
    [
        LiveIntentKind::Buy,
        LiveIntentKind::Approve,
        LiveIntentKind::Sell,
    ]
    .into_iter()
    .find(|k| k.name() == text)
}

/// One fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub kind: FactKind,
    pub tx_hash: [u8; 32],
    pub intent: LiveIntentKind,
    pub component: ComponentId,
    pub token: [u8; 20],
    pub nonce: u64,
    pub grant_generation: u64,
    pub input_wei: Option<U256>,
    pub max_wei: Option<U256>,
    /// Number, hash and time of the block a fact of inclusion names.
    pub block: Option<(u64, [u8; 32], u64)>,
    pub gas_wei: Option<U256>,
    pub tokens: Option<U256>,
    pub eth_wei: Option<U256>,
    /// A released transaction's window: the last block its bundle may be
    /// included in (BSC3-048). None: not known.
    pub max_block: Option<u64>,
    /// `Replaced`: the transaction that took its nonce, in `block`.
    pub by_tx: Option<[u8; 32]>,
    /// `Responded`: the provider and its answer, `ADAPTER:OUTCOME`.
    pub response: Option<String>,
    /// The sender and chain of its nonce (absent in a fact written before
    /// them: its transaction is grouped with no other).
    pub origin: Option<NonceOrigin>,
}

impl Fact {
    /// The nonce its transaction is one of the alternatives of (none: its
    /// sender and chain are not known).
    pub fn key(&self) -> Option<NonceKey> {
        Some(NonceKey {
            origin: self.origin?,
            nonce: self.nonce,
        })
    }

    /// What became of the released `of`: `kind`, with nothing else known.
    pub fn about(of: &Fact, kind: FactKind) -> Fact {
        Fact {
            kind,
            input_wei: None,
            max_wei: None,
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: None,
            by_tx: None,
            response: None,
            ..of.clone()
        }
    }

    /// The admitted transaction `a` as `kind` (none without its token and
    /// nonce: an entry written before BSC3-024).
    pub fn admitted(a: &Admitted, kind: FactKind) -> Option<Fact> {
        Some(Fact {
            kind,
            tx_hash: a.tx_hash,
            intent: a.kind,
            component: a.component,
            token: a.token?,
            nonce: a.nonce?,
            grant_generation: a.grant_generation,
            input_wei: Some(U256::from(a.input_wei)),
            max_wei: Some(U256::from(a.max_wei)),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: a.max_block,
            by_tx: None,
            response: None,
            origin: a.origin,
        })
    }
}

/// What [`Ledger::recover`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recovered {
    /// Admitted transactions now facts, as UNKNOWN.
    pub unknown: usize,
    /// Admitted transactions that cannot be facts (no token or nonce in
    /// their entry): unresolved for good.
    pub unrecoverable: Vec<[u8; 32]>,
}

/// The ledger under an owner state directory.
#[derive(Clone, Debug)]
pub struct Ledger {
    dir: PathBuf,
    /// Tests: an entry's write ends after this many bytes, as a crash
    /// would end it.
    #[cfg(test)]
    cut: Option<usize>,
}

fn word(v: U256) -> Value {
    Value::U256(v.to_be_bytes())
}

impl Ledger {
    pub fn open(state_dir: &Path) -> Self {
        Ledger {
            dir: state_dir.join("live-ledger"),
            #[cfg(test)]
            cut: None,
        }
    }

    /// `bytes` into `file`, synced (tests: cut off as a crash would).
    fn write_entry(&self, mut file: fs::File, bytes: &[u8]) -> std::io::Result<()> {
        #[cfg(test)]
        if let Some(n) = self.cut {
            file.write_all(&bytes[..n.min(bytes.len())])?;
            return Err(std::io::Error::other("cut off"));
        }
        file.write_all(bytes)?;
        file.sync_all()
    }

    /// Appends `fact` as the next entry. It is written whole and synced
    /// under a temporary name no reader takes, then published by a link
    /// that never replaces an entry (BSC3-045): a process that ends in the
    /// middle leaves at most an unpublished temporary file, never a torn
    /// entry. Nothing here removes or skips an entry.
    pub fn append(&self, fact: &Fact, now_ms: u64) -> Result<u64, String> {
        let mut d = Draft::new(&k::SCHEMA);
        d.set(k::KIND, fact.kind.name())
            .set(k::TX_HASH, Value::B256(fact.tx_hash))
            .set(k::PROVENANCE_TOOL, "bsc_runtime")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, now_ms)
            .set(k::INTENT, fact.intent.name())
            .set(k::COMPONENT, fact.component.name())
            .set(k::TOKEN, Value::Address(fact.token))
            .set(k::NONCE, fact.nonce)
            .set(k::GRANT_GENERATION, fact.grant_generation)
            .set_opt(k::INPUT_WEI, fact.input_wei.map(word))
            .set_opt(k::MAX_WEI, fact.max_wei.map(word))
            .set_opt(k::BLOCK_NUMBER, fact.block.map(|b| b.0))
            .set_opt(k::BLOCK_HASH, fact.block.map(|b| Value::B256(b.1)))
            .set_opt(k::BLOCK_MS, fact.block.map(|b| b.2))
            .set_opt(k::GAS_WEI, fact.gas_wei.map(word))
            .set_opt(k::TOKENS, fact.tokens.map(word))
            .set_opt(k::ETH_WEI, fact.eth_wei.map(word))
            .set_opt(k::MAX_BLOCK, fact.max_block)
            .set_opt(k::BY_TX_HASH, fact.by_tx.map(Value::B256))
            .set_opt(k::RESPONSE, fact.response.as_deref())
            .set_opt(k::CHAIN_ID, fact.origin.map(|o| o.chain_id))
            .set_opt(k::SENDER, fact.origin.map(|o| Value::Address(o.sender)));
        let bytes = d.encode().map_err(|e| e.to_string())?;
        let e = |err: std::io::Error| format!("{}: {err}", self.dir.display());
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&self.dir)
            .map_err(e)?;
        let (file, tmp) = loop {
            static TEMPORARY: AtomicU64 = AtomicU64::new(0);
            let n = TEMPORARY.fetch_add(1, Ordering::Relaxed);
            let path = self
                .dir
                .join(format!("pending-{}-{n}.tmp", std::process::id()));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(f) => break (f, path),
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(e(err)),
            }
        };
        if let Err(err) = self.write_entry(file, &bytes) {
            let _ = fs::remove_file(&tmp);
            return Err(e(err));
        }
        let published = self.publish(&tmp);
        // The published entry is a link of its own.
        let _ = fs::remove_file(&tmp);
        let seq = published.map_err(e)?;
        fs::File::open(&self.dir)
            .and_then(|d| d.sync_all())
            .map_err(e)?;
        Ok(seq)
    }

    /// Links the written entry `tmp` as the next free sequence number.
    fn publish(&self, tmp: &Path) -> std::io::Result<u64> {
        let mut seq = self.count().map_err(std::io::Error::other)?;
        loop {
            let path = self.dir.join(format!("{seq:020}.dbrec"));
            match fs::hard_link(tmp, &path) {
                Ok(()) => return Ok(seq),
                Err(err) if err.kind() == ErrorKind::AlreadyExists => seq += 1,
                Err(err) => return Err(err),
            }
        }
    }

    /// Every admitted transaction the ledger has no fact of becomes one, as
    /// UNKNOWN: its bytes may have left (the process ended between its
    /// admission and its outcome). Run with no LIVE end armed (startup,
    /// before an arm), so no admission is in flight.
    pub fn recover(&self, admitted: &[Admitted], now_ms: u64) -> Result<Recovered, String> {
        let facts = self.facts()?;
        let mut out = Recovered::default();
        for a in admitted {
            if facts
                .iter()
                .any(|f| f.kind.first() && f.tx_hash == a.tx_hash)
            {
                continue;
            }
            match Fact::admitted(a, FactKind::Unknown) {
                Some(f) => {
                    self.append(&f, now_ms)?;
                    out.unknown += 1;
                }
                None => out.unrecoverable.push(a.tx_hash),
            }
        }
        Ok(out)
    }

    /// The published entries in order (an unpublished temporary file is
    /// none).
    fn names(&self) -> Result<Vec<(u64, PathBuf)>, String> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(format!("{}: {err}", self.dir.display())),
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            if let Some(seq) = name
                .to_string_lossy()
                .strip_suffix(".dbrec")
                .and_then(|n| n.parse::<u64>().ok())
            {
                out.push((seq, entry.path()));
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    fn count(&self) -> Result<u64, String> {
        Ok(self.names()?.last().map_or(0, |(seq, _)| seq + 1))
    }

    /// Every fact in order. One that does not decode refuses the whole
    /// ledger: a fold never under-counts.
    pub fn facts(&self) -> Result<Vec<Fact>, String> {
        let mut out = Vec::new();
        for (_, path) in self.names()? {
            let bad = |why: &str| format!("{}: {why}", path.display());
            let bytes = fs::read(&path).map_err(|e| bad(&e.to_string()))?;
            let r = decode(&k::SCHEMA, &bytes)
                .or_else(|_| decode(&k::V1, &bytes))
                .map_err(|e| bad(&e.0))?;
            let e = |x: darkbot_bsc_record::RecordError| bad(&x.0);
            let u = |tag| -> Result<Option<U256>, String> {
                Ok(r.opt_u256(tag).map_err(e)?.map(U256::from_be_bytes))
            };
            let block = match (
                r.opt_u64(k::BLOCK_NUMBER).map_err(e)?,
                r.opt_b256(k::BLOCK_HASH).map_err(e)?,
                r.opt_u64(k::BLOCK_MS).map_err(e)?,
            ) {
                (Some(n), Some(h), Some(ms)) => Some((n, h, ms)),
                (None, None, None) => None,
                _ => return Err(bad("a block is named whole or not at all")),
            };
            let origin = match (
                r.opt_u64(k::CHAIN_ID).map_err(e)?,
                r.opt_address(k::SENDER).map_err(e)?,
            ) {
                (Some(chain_id), Some(sender)) => Some(NonceOrigin { chain_id, sender }),
                (None, None) => None,
                _ => return Err(bad("a nonce's sender and chain are named together")),
            };
            out.push(Fact {
                kind: FactKind::parse(r.text(k::KIND).map_err(e)?).ok_or_else(|| bad("kind"))?,
                tx_hash: r.b256(k::TX_HASH).map_err(e)?,
                intent: intent_of(r.text(k::INTENT).map_err(e)?).ok_or_else(|| bad("intent"))?,
                component: ComponentId::parse(r.text(k::COMPONENT).map_err(e)?)
                    .ok_or_else(|| bad("component"))?,
                token: r.address(k::TOKEN).map_err(e)?,
                nonce: r.u64(k::NONCE).map_err(e)?,
                grant_generation: r.u64(k::GRANT_GENERATION).map_err(e)?,
                input_wei: u(k::INPUT_WEI)?,
                max_wei: u(k::MAX_WEI)?,
                block,
                gas_wei: u(k::GAS_WEI)?,
                tokens: u(k::TOKENS)?,
                eth_wei: u(k::ETH_WEI)?,
                max_block: r.opt_u64(k::MAX_BLOCK).map_err(e)?,
                by_tx: r.opt_b256(k::BY_TX_HASH).map_err(e)?,
                response: r.opt_text(k::RESPONSE).map_err(e)?.map(str::to_string),
                origin,
            });
        }
        Ok(out)
    }
}

/// Where a transaction stands after the ledger's facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tx {
    pub released: Fact,
    /// The last word on it: `Released`/`Unknown` while unresolved (also
    /// after an orphan), else `Withdrawn`, `Included`, `Reverted` or
    /// `Replaced`.
    pub state: FactKind,
    pub inclusion: Option<Fact>,
    /// A block that included it (not one that replaced it) left the chain:
    /// its bytes were public in a block and may be included at any height;
    /// its window bounds nothing any more (BSC3-048).
    pub orphaned: bool,
    /// The lot it counts in (none: withdrawn or replaced, BSC3-047).
    pub lot: Option<[u8; 32]>,
    /// What the providers answered to its release (BSC3-051): never an
    /// inclusion.
    pub responses: Vec<String>,
}

impl Tx {
    /// Whether its bundle's window is over at `head`: no block after its
    /// last one includes it (BSC3-048). An unknown window is never over,
    /// nor is that of a transaction once included in a block that left the
    /// chain (its bytes are public).
    pub fn window_over(&self, head: u64) -> bool {
        !self.orphaned && self.released.max_block.is_some_and(|last| head > last)
    }

    /// Its gas: the receipt's, or the most it could cost.
    fn gas(&self) -> U256 {
        let stated = self.inclusion.as_ref().and_then(|f| f.gas_wei);
        stated.unwrap_or_else(|| {
            let most = self.released.max_wei.unwrap_or_default();
            most.saturating_sub(self.released.input_wei.unwrap_or_default())
        })
    }
}

/// Where a token's LIVE position stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionState {
    /// Its BUY is released and not included yet (or UNKNOWN).
    Pending,
    Open,
    /// A SELL is released and not included yet.
    Exiting,
    Closed,
    /// Its BUY reverted.
    Failed,
}

impl PositionState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Open => "OPEN",
            Self::Exiting => "EXITING",
            Self::Closed => "CLOSED",
            Self::Failed => "FAILED",
        }
    }
}

/// One lot of a token's LIVE position: what one BUY opened (BSC3-047).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    /// The BUY's hash (an exit step without a lot before it: its own).
    pub lot: [u8; 32],
    pub token: [u8; 20],
    pub component: ComponentId,
    pub state: PositionState,
    /// Tokens the BUY received.
    pub tokens: U256,
    pub input_wei: U256,
    /// The native amount the SELL received.
    pub proceeds_wei: U256,
    /// Every included or reverted transaction's gas of this token.
    pub gas_wei: U256,
    /// When the BUY's block was produced (unix ms).
    pub opened_ms: Option<u64>,
    /// An approval of this lot is included: its SELL needs no other.
    pub approved: bool,
    /// Proceeds − input − gas, once closed (failed: − gas).
    pub pnl_wei: Option<i128>,
}

/// The fold of the ledger.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Book {
    pub txs: Vec<Tx>,
    pub positions: Vec<Position>,
    pub realized_pnl_wei: i128,
    pub gas_wei: U256,
}

impl Book {
    pub fn count(&self, state: PositionState) -> u64 {
        self.positions.iter().filter(|p| p.state == state).count() as u64
    }

    /// Unresolved transactions (released or UNKNOWN, not yet included).
    pub fn unresolved(&self) -> impl Iterator<Item = &Tx> {
        self.txs
            .iter()
            .filter(|t| matches!(t.state, FactKind::Released | FactKind::Unknown))
    }

    pub fn tx(&self, tx_hash: &[u8; 32]) -> Option<&Tx> {
        self.txs.iter().find(|t| t.released.tx_hash == *tx_hash)
    }

    /// The lot `tx_hash` counts in.
    pub fn lot_of(&self, tx_hash: &[u8; 32]) -> Option<&Position> {
        let lot = self.tx(tx_hash)?.lot?;
        self.positions.iter().find(|p| p.lot == lot)
    }

    /// `token`'s open lot: its latest one open or exiting.
    pub fn open_lot(&self, token: &[u8; 20]) -> Option<&Position> {
        self.positions.iter().rev().find(|p| {
            p.token == *token && matches!(p.state, PositionState::Open | PositionState::Exiting)
        })
    }
}

fn signed(v: U256) -> i128 {
    i128::try_from(v).unwrap_or(i128::MAX)
}

/// The positions, costs and net result the facts make, in order.
pub fn fold(facts: &[Fact]) -> Book {
    let mut txs: Vec<Tx> = Vec::new();
    for f in facts {
        match f.kind {
            kind if kind.first() => {
                if txs.iter().all(|t| t.released.tx_hash != f.tx_hash) {
                    txs.push(Tx {
                        released: f.clone(),
                        state: f.kind,
                        inclusion: None,
                        orphaned: false,
                        lot: None,
                        responses: Vec::new(),
                    });
                }
            }
            kind => {
                let Some(t) = txs.iter_mut().find(|t| t.released.tx_hash == f.tx_hash) else {
                    continue;
                };
                match kind {
                    FactKind::Responded => t.responses.extend(f.response.clone()),
                    FactKind::Orphaned => {
                        // Its own inclusion left the chain (not one that
                        // replaced it): its bytes were public.
                        t.orphaned |= matches!(t.state, FactKind::Included | FactKind::Reverted);
                        t.state = t.released.kind;
                        t.inclusion = None;
                    }
                    // Included, reverted, or replaced: the fact names the
                    // block (and, replaced, the transaction that took the
                    // nonce); a reorg of that block opens it again.
                    _ => {
                        t.state = kind;
                        t.inclusion = Some(f.clone());
                    }
                }
            }
        }
    }
    let mut book = Book::default();
    for t in &mut txs {
        // A withdrawn transaction never left, a replaced one never
        // happened: no lot, no cost.
        if matches!(t.state, FactKind::Withdrawn | FactKind::Replaced) {
            continue;
        }
        let spent = matches!(t.state, FactKind::Included | FactKind::Reverted);
        if spent {
            book.gas_wei += t.gas();
        }
        let token = t.released.token;
        // A BUY opens a lot of its own; an exit step counts in its token's
        // latest lot (BSC3-047).
        let latest = book.positions.iter().rposition(|p| p.token == token);
        let at = match latest {
            Some(i) if t.released.intent != LiveIntentKind::Buy => i,
            _ => {
                book.positions.push(Position {
                    lot: t.released.tx_hash,
                    token,
                    component: t.released.component,
                    state: PositionState::Pending,
                    tokens: U256::ZERO,
                    input_wei: U256::ZERO,
                    proceeds_wei: U256::ZERO,
                    gas_wei: U256::ZERO,
                    opened_ms: None,
                    approved: false,
                    pnl_wei: None,
                });
                book.positions.len() - 1
            }
        };
        let p = &mut book.positions[at];
        t.lot = Some(p.lot);
        if spent {
            p.gas_wei += t.gas();
        }
        let inclusion = t.inclusion.as_ref();
        match (t.released.intent, t.state) {
            (LiveIntentKind::Buy, FactKind::Included) => {
                p.state = PositionState::Open;
                p.tokens = inclusion.and_then(|f| f.tokens).unwrap_or_default();
                p.input_wei = t.released.input_wei.unwrap_or_default();
                p.opened_ms = inclusion.and_then(|f| f.block).map(|b| b.2);
            }
            (LiveIntentKind::Buy, FactKind::Reverted) => p.state = PositionState::Failed,
            (LiveIntentKind::Approve, FactKind::Included) => p.approved = true,
            (LiveIntentKind::Sell, FactKind::Included) => {
                p.state = PositionState::Closed;
                p.proceeds_wei = inclusion.and_then(|f| f.eth_wei).unwrap_or_default();
            }
            (LiveIntentKind::Sell, FactKind::Released | FactKind::Unknown)
                if p.state == PositionState::Open =>
            {
                p.state = PositionState::Exiting;
            }
            _ => {}
        }
    }
    for p in &mut book.positions {
        p.pnl_wei = match p.state {
            PositionState::Closed => Some(
                signed(p.proceeds_wei)
                    .saturating_sub(signed(p.input_wei))
                    .saturating_sub(signed(p.gas_wei)),
            ),
            PositionState::Failed => Some(-signed(p.gas_wei)),
            _ => None,
        };
        book.realized_pnl_wei = book.realized_pnl_wei.saturating_add(p.pnl_wei.unwrap_or(0));
    }
    book.txs = txs;
    book
}

#[cfg(test)]
mod tests {
    use super::*;

    fn released(n: u8, intent: LiveIntentKind, token: u8, input: u64, most: u64) -> Fact {
        Fact {
            kind: FactKind::Released,
            tx_hash: [n; 32],
            intent,
            component: ComponentId::StrategyC,
            token: [token; 20],
            nonce: u64::from(n),
            grant_generation: 7,
            input_wei: Some(U256::from(input)),
            max_wei: Some(U256::from(most)),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(100),
            by_tx: None,
            response: None,
            origin: Some(crate::sources::tests::ORIGIN),
        }
    }

    /// Budget fix: a fact names its nonce's sender and chain and reads them
    /// back (its alternatives group by them); a fact of an older build
    /// reads without them (grouped with none); one naming only one of them
    /// refuses the ledger.
    #[test]
    fn a_fact_keeps_the_sender_and_chain_of_its_nonce() {
        let dir = std::env::temp_dir().join(format!("bsc-live-origin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ledger = Ledger::open(&dir);
        let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        ledger.append(&buy, 1).unwrap();
        let raw = |schema: &'static darkbot_bsc_record::Schema, seq: u64, chain: Option<u64>| {
            let mut d = Draft::new(schema);
            d.set(k::KIND, "RELEASED")
                .set(k::TX_HASH, Value::B256([seq as u8 + 1; 32]))
                .set(k::PROVENANCE_TOOL, "test")
                .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
                .set(k::INTENT, "BUY")
                .set(k::COMPONENT, "BSC_EXEC")
                .set(k::TOKEN, Value::Address([0xB; 20]))
                .set(k::NONCE, 1u64)
                .set(k::GRANT_GENERATION, 7u64)
                .set_opt(k::CHAIN_ID, chain);
            let path = dir.join("live-ledger").join(format!("{seq:020}.dbrec"));
            fs::write(path, d.encode().unwrap()).unwrap();
        };
        raw(&k::V1, 1, None);
        let facts = ledger.facts().unwrap();
        assert_eq!(facts[0], buy);
        assert_eq!(
            facts[0].key(),
            Some(NonceKey {
                origin: crate::sources::tests::ORIGIN,
                nonce: 1
            })
        );
        assert_eq!((facts[1].origin, facts[1].key()), (None, None));
        raw(&k::SCHEMA, 2, Some(56));
        assert!(ledger.facts().unwrap_err().contains("sender and chain"));
        // A sender without its chain neither.
        let mut d = Draft::new(&k::SCHEMA);
        d.set(k::KIND, "RELEASED")
            .set(k::TX_HASH, Value::B256([9; 32]))
            .set(k::PROVENANCE_TOOL, "test")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(k::INTENT, "BUY")
            .set(k::COMPONENT, "BSC_EXEC")
            .set(k::TOKEN, Value::Address([0xB; 20]))
            .set(k::NONCE, 1u64)
            .set(k::GRANT_GENERATION, 7u64)
            .set(k::SENDER, Value::Address([0xb0; 20]));
        let path = dir.join("live-ledger").join(format!("{:020}.dbrec", 2));
        fs::write(path, d.encode().unwrap()).unwrap();
        assert!(ledger.facts().unwrap_err().contains("sender and chain"));
        fs::remove_dir_all(dir).unwrap();
    }

    fn included(of: &Fact, block: u8, gas: u64, tokens: u64, eth: u64) -> Fact {
        Fact {
            block: Some((u64::from(block), [block; 32], 1_000 * u64::from(block))),
            gas_wei: Some(U256::from(gas)),
            tokens: (tokens > 0).then(|| U256::from(tokens)),
            eth_wei: (eth > 0).then(|| U256::from(eth)),
            ..Fact::about(of, FactKind::Included)
        }
    }

    /// Plan 28.10.: the facts, written in order and read back after a
    /// restart, fold to the same positions: a round trip's PnL is proceeds
    /// minus input minus every gas; an orphaned inclusion is unresolved
    /// again; a reverted BUY is a failed position whose gas is lost; an
    /// UNKNOWN stays visible; an undecodable entry refuses the ledger.
    #[test]
    fn the_ledger_folds_to_positions_and_net_result() {
        let dir = std::env::temp_dir().join(format!("bsc-live-ledger-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ledger = Ledger::open(&dir);
        let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        let approve = released(2, LiveIntentKind::Approve, 0xA, 0, 100);
        let sell = released(3, LiveIntentKind::Sell, 0xA, 0, 150);
        let failed = released(4, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
        let unknown = Fact {
            kind: FactKind::Unknown,
            ..released(5, LiveIntentKind::Buy, 0xC, 1_000, 1_300)
        };
        let facts = [
            buy.clone(),
            included(&buy, 10, 250, 5_000, 950),
            // A reorg takes the BUY's block; it is included anew.
            Fact {
                block: Some((10, [10; 32], 10_000)),
                ..Fact::about(&buy, FactKind::Orphaned)
            },
            included(&buy, 11, 260, 5_000, 950),
            approve.clone(),
            included(&approve, 12, 40, 0, 0),
            sell.clone(),
            included(&sell, 13, 90, 5_000, 1_500),
            failed.clone(),
            Fact {
                gas_wei: Some(U256::from(300)),
                block: Some((14, [14; 32], 14_000)),
                ..Fact::about(&failed, FactKind::Reverted)
            },
            unknown.clone(),
        ];
        for f in &facts {
            ledger.append(f, 1).unwrap();
        }
        // Read back (a restart): the same facts, the same fold.
        let read = Ledger::open(&dir).facts().unwrap();
        assert_eq!(read, facts);
        let book = fold(&read);
        let a = book
            .positions
            .iter()
            .find(|p| p.token == [0xA; 20])
            .unwrap();
        assert_eq!(a.state, PositionState::Closed);
        assert_eq!(
            (a.tokens, a.input_wei),
            (U256::from(5_000), U256::from(1_000))
        );
        assert_eq!(a.gas_wei, U256::from(260 + 40 + 90));
        assert_eq!(a.pnl_wei, Some(1_500 - 1_000 - 390));
        assert_eq!(a.opened_ms, Some(11_000));
        let b = book
            .positions
            .iter()
            .find(|p| p.token == [0xB; 20])
            .unwrap();
        assert_eq!((b.state, b.pnl_wei), (PositionState::Failed, Some(-300)));
        let c = book
            .positions
            .iter()
            .find(|p| p.token == [0xC; 20])
            .unwrap();
        assert_eq!(c.state, PositionState::Pending);
        assert_eq!(book.unresolved().count(), 1);
        assert_eq!(book.realized_pnl_wei, 110 - 300);
        assert_eq!(book.gas_wei, U256::from(260 + 40 + 90 + 300));
        // Open after the BUY only; EXITING once its SELL is released.
        let open = fold(&read[..4]);
        assert_eq!(open.positions[0].state, PositionState::Open);
        let exiting = fold(&read[..7]);
        assert_eq!(exiting.positions[0].state, PositionState::Exiting);
        // A cost the receipt did not state counts at its most.
        let unstated = fold(&[buy.clone(), Fact::about(&buy, FactKind::Included)]);
        assert_eq!(unstated.gas_wei, U256::from(300));
        // An entry that does not decode refuses the ledger.
        fs::write(dir.join("live-ledger/00000000000000000099.dbrec"), b"x").unwrap();
        assert!(Ledger::open(&dir).facts().is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-045: an entry's write ends in the middle (an I/O error, or the
    /// process ends and leaves its temporary file). It is no entry: the
    /// ledger reads as before, the next fact takes the next entry.
    #[test]
    fn an_entry_cut_off_in_its_write_is_no_entry() {
        let dir = std::env::temp_dir().join(format!("bsc-live-cut-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ledger = Ledger::open(&dir);
        let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        let lost = released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
        let next = released(3, LiveIntentKind::Buy, 0xC, 1_000, 1_300);
        ledger.append(&buy, 1).unwrap();
        let crashed = Ledger {
            cut: Some(9),
            ..Ledger::open(&dir)
        };
        assert!(crashed.append(&lost, 1).is_err());
        // What a process that ended there leaves.
        fs::write(dir.join("live-ledger/pending-1-0.tmp"), b"torn").unwrap();
        let files = || fs::read_dir(dir.join("live-ledger")).unwrap().count();
        let left = files();
        assert_eq!(ledger.facts().unwrap(), vec![buy.clone()]);
        assert_eq!(ledger.append(&next, 1).unwrap(), 1);
        assert_eq!(ledger.facts().unwrap(), vec![buy, next]);
        assert_eq!(files(), left + 1);
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-047: a token bought again after its exit is a lot of its own
    /// (the BUY's hash): each lot keeps its tokens, input, gas and result;
    /// the exit steps count in their lot; the net result is the closed
    /// lots'. A BUY another transaction replaced never happened: no lot.
    /// A reorg and a restart fold the same.
    #[test]
    fn a_token_bought_again_is_a_lot_of_its_own() {
        let dir = std::env::temp_dir().join(format!("bsc-live-lots-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ledger = Ledger::open(&dir);
        let buy1 = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        let approve1 = released(2, LiveIntentKind::Approve, 0xA, 0, 100);
        let sell1 = released(3, LiveIntentKind::Sell, 0xA, 0, 150);
        // A BUY of token B is lost; BUY 2 of A takes its nonce.
        let lost = released(4, LiveIntentKind::Buy, 0xB, 3_000, 3_300);
        let buy2 = released(5, LiveIntentKind::Buy, 0xA, 2_000, 2_300);
        let approve2 = released(6, LiveIntentKind::Approve, 0xA, 0, 100);
        let facts = [
            buy1.clone(),
            included(&buy1, 10, 250, 5_000, 1_000),
            approve1.clone(),
            included(&approve1, 11, 40, 0, 0),
            sell1.clone(),
            included(&sell1, 12, 90, 5_000, 1_500),
            lost.clone(),
            buy2.clone(),
            Fact {
                block: Some((13, [13; 32], 13_000)),
                by_tx: Some(buy2.tx_hash),
                ..Fact::about(&lost, FactKind::Replaced)
            },
            included(&buy2, 13, 260, 7_000, 2_000),
            // A reorg takes BUY 2's block; it is included anew.
            Fact {
                block: Some((13, [13; 32], 13_000)),
                ..Fact::about(&buy2, FactKind::Orphaned)
            },
            included(&buy2, 14, 270, 7_000, 2_000),
            approve2.clone(),
            included(&approve2, 15, 45, 0, 0),
        ];
        for f in &facts {
            ledger.append(f, 1).unwrap();
        }
        let book = fold(&Ledger::open(&dir).facts().unwrap());
        assert_eq!(book.positions.len(), 2, "{:?}", book.positions);
        let (first, second) = (&book.positions[0], &book.positions[1]);
        assert_eq!((first.lot, second.lot), (buy1.tx_hash, buy2.tx_hash));
        assert_eq!(first.state, PositionState::Closed);
        assert_eq!(
            (first.tokens, first.input_wei, first.gas_wei),
            (
                U256::from(5_000),
                U256::from(1_000),
                U256::from(250 + 40 + 90)
            )
        );
        assert_eq!(first.pnl_wei, Some(1_500 - 1_000 - 380));
        assert_eq!(second.state, PositionState::Open);
        assert_eq!(
            (second.tokens, second.input_wei, second.gas_wei),
            (U256::from(7_000), U256::from(2_000), U256::from(270 + 45))
        );
        assert_eq!((second.opened_ms, second.pnl_wei), (Some(14_000), None));
        // Each lot knows its own approval.
        assert!(first.approved && second.approved);
        let without = fold(&Ledger::open(&dir).facts().unwrap()[..12]);
        assert!(!without.positions[1].approved);
        assert_eq!(book.lot_of(&approve2.tx_hash).unwrap().lot, buy2.tx_hash);
        assert_eq!(book.lot_of(&approve1.tx_hash).unwrap().lot, buy1.tx_hash);
        assert!(book.lot_of(&lost.tx_hash).is_none());
        assert_eq!(book.realized_pnl_wei, 1_500 - 1_000 - 380);
        assert_eq!(book.gas_wei, U256::from(250 + 40 + 90 + 270 + 45));
        assert_eq!(book.count(PositionState::Pending), 0);
        fs::remove_dir_all(dir).unwrap();
    }

    fn admitted(n: u8, token: Option<[u8; 20]>) -> Admitted {
        Admitted {
            tx_hash: [n; 32],
            kind: LiveIntentKind::Buy,
            component: ComponentId::ExecC3,
            grant_generation: 7,
            input_wei: 1_000,
            max_wei: 1_300,
            token,
            nonce: token.map(|_| u64::from(n)),
            max_block: token.map(|_| 100),
            origin: token.map(|_| crate::sources::tests::ORIGIN),
        }
    }

    /// BSC3-024: an admitted transaction the ledger has no fact of (the
    /// process ended between its admission and its outcome) becomes an
    /// UNKNOWN fact, once; one without token and nonce cannot and is named.
    /// A withdrawn transaction (no byte left) is resolved, with no position
    /// and no cost.
    #[test]
    fn admitted_transactions_without_a_fact_are_recovered_as_unknown() {
        let dir = std::env::temp_dir().join(format!("bsc-live-recover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ledger = Ledger::open(&dir);
        let known = Fact::admitted(&admitted(1, Some([0xA; 20])), FactKind::Released).unwrap();
        let withdrawn = Fact::admitted(&admitted(2, Some([0xB; 20])), FactKind::Withdrawn).unwrap();
        ledger.append(&known, 1).unwrap();
        ledger.append(&withdrawn, 1).unwrap();
        let all = [
            admitted(1, Some([0xA; 20])),
            admitted(2, Some([0xB; 20])),
            admitted(3, Some([0xC; 20])),
            admitted(4, None),
        ];
        let r = ledger.recover(&all, 2).unwrap();
        assert_eq!(r.unknown, 1);
        assert_eq!(r.unrecoverable, vec![[4; 32]]);
        let facts = ledger.facts().unwrap();
        assert_eq!(facts.len(), 3);
        assert_eq!(
            facts[2],
            Fact::admitted(&admitted(3, Some([0xC; 20])), FactKind::Unknown).unwrap()
        );
        // Once only.
        assert_eq!(ledger.recover(&all, 3).unwrap().unknown, 0);
        let book = fold(&facts);
        let unresolved: Vec<[u8; 32]> = book.unresolved().map(|t| t.released.tx_hash).collect();
        assert_eq!(unresolved, vec![[1; 32], [3; 32]]);
        assert_eq!(book.tx(&[2; 32]).unwrap().state, FactKind::Withdrawn);
        assert!(book.positions.iter().all(|p| p.token != [0xB; 20]));
        assert_eq!(book.gas_wei, U256::ZERO);
        fs::remove_dir_all(dir).unwrap();
    }
}
