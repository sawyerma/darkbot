//! The LIVE ledger (plan 28.10., package 6): every LIVE transaction whose
//! bytes may have left, and what the canonical chain made of it, as facts
//! written create-new in order under the owner state (`live-ledger/`) and
//! never changed. The positions, their costs and the net result are the
//! fold of these facts ([`fold`]): after a crash or restart exactly as
//! before it.
//!
//! - A BUY included opens its token's position with the tokens the Portal
//!   traded to it; reverted, it failed and its gas is lost; replaced, it
//!   never happened.
//! - An approval's and a SELL's gas count against the position; a SELL
//!   included closes it with the native amount the Portal paid.
//! - A block that left the canonical chain takes its inclusions back: the
//!   transaction is unresolved again until it is included anew.
//! - An UNKNOWN transaction (its handoff did not complete) is unresolved,
//!   and visible, until the chain includes or replaces it.
//!
//! Realized PnL = proceeds − input − all gas of the position's
//! transactions; a failed BUY's gas is a loss of its own. A cost the
//! receipt did not state counts at its most (the signed gas limit).

use crate::sources::{ComponentId, LiveIntentKind};
use alloy_primitives::U256;
use darkbot_bsc_record::schema::bsc_runtime::live_ledger as k;
use darkbot_bsc_record::{Draft, Value, decode};
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// What a fact says about its transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactKind {
    Released,
    Unknown,
    Included,
    Reverted,
    Replaced,
    Orphaned,
}

impl FactKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Released => "RELEASED",
            Self::Unknown => "UNKNOWN",
            Self::Included => "INCLUDED",
            Self::Reverted => "REVERTED",
            Self::Replaced => "REPLACED",
            Self::Orphaned => "ORPHANED",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [
            Self::Released,
            Self::Unknown,
            Self::Included,
            Self::Reverted,
            Self::Replaced,
            Self::Orphaned,
        ]
        .into_iter()
        .find(|k| k.name() == text)
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
}

impl Fact {
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
            ..of.clone()
        }
    }
}

/// The ledger under an owner state directory.
#[derive(Clone, Debug)]
pub struct Ledger {
    dir: PathBuf,
}

fn word(v: U256) -> Value {
    Value::U256(v.to_be_bytes())
}

impl Ledger {
    pub fn open(state_dir: &Path) -> Self {
        Ledger {
            dir: state_dir.join("live-ledger"),
        }
    }

    /// Appends `fact` as the next entry, create-new and synced.
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
            .set_opt(k::ETH_WEI, fact.eth_wei.map(word));
        let bytes = d.encode().map_err(|e| e.to_string())?;
        let e = |err: std::io::Error| format!("{}: {err}", self.dir.display());
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&self.dir)
            .map_err(e)?;
        let mut seq = self.count()?;
        loop {
            let path = self.dir.join(format!("{seq:020}.dbrec"));
            let written = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()));
            match written {
                Ok(()) => break,
                Err(err) if err.kind() == ErrorKind::AlreadyExists => seq += 1,
                Err(err) => return Err(e(err)),
            }
        }
        fs::File::open(&self.dir)
            .and_then(|d| d.sync_all())
            .map_err(e)?;
        Ok(seq)
    }

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
            let r = decode(&k::SCHEMA, &bytes).map_err(|e| bad(&e.0))?;
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
    /// after an orphan), else `Included`, `Reverted` or `Replaced`.
    pub state: FactKind,
    pub inclusion: Option<Fact>,
}

impl Tx {
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

/// One token's LIVE position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
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
}

fn signed(v: U256) -> i128 {
    i128::try_from(v).unwrap_or(i128::MAX)
}

/// The positions, costs and net result the facts make, in order.
pub fn fold(facts: &[Fact]) -> Book {
    let mut txs: Vec<Tx> = Vec::new();
    for f in facts {
        match f.kind {
            FactKind::Released | FactKind::Unknown => {
                if txs.iter().all(|t| t.released.tx_hash != f.tx_hash) {
                    txs.push(Tx {
                        released: f.clone(),
                        state: f.kind,
                        inclusion: None,
                    });
                }
            }
            kind => {
                let Some(t) = txs.iter_mut().find(|t| t.released.tx_hash == f.tx_hash) else {
                    continue;
                };
                match kind {
                    FactKind::Orphaned => {
                        t.state = t.released.kind;
                        t.inclusion = None;
                    }
                    FactKind::Replaced => {
                        t.state = kind;
                        t.inclusion = None;
                    }
                    _ => {
                        t.state = kind;
                        t.inclusion = Some(f.clone());
                    }
                }
            }
        }
    }
    let mut book = Book::default();
    for t in &txs {
        if matches!(t.state, FactKind::Included | FactKind::Reverted) {
            book.gas_wei += t.gas();
        }
        let token = t.released.token;
        let at = match book.positions.iter().position(|p| p.token == token) {
            Some(i) => i,
            None => {
                book.positions.push(Position {
                    token,
                    component: t.released.component,
                    state: PositionState::Pending,
                    tokens: U256::ZERO,
                    input_wei: U256::ZERO,
                    proceeds_wei: U256::ZERO,
                    gas_wei: U256::ZERO,
                    opened_ms: None,
                    pnl_wei: None,
                });
                book.positions.len() - 1
            }
        };
        let p = &mut book.positions[at];
        if matches!(t.state, FactKind::Included | FactKind::Reverted) {
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
            (LiveIntentKind::Buy, FactKind::Reverted) if p.state == PositionState::Pending => {
                p.state = PositionState::Failed;
            }
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
        }
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
}
