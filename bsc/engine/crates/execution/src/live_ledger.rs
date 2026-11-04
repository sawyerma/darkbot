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
//!   resolved and never on chain. An attempt whose every fact is WITHDRAWN,
//!   its BUY's own among them, gets its slots back
//!   ([`Book::withdrawn_attempts`], BSC3-068), none after a quarantine.
//! - The operator decides a lot (REV.1.0 R3, by request, once each):
//!   RESUMED lets its stopped exits go on (the reverts before it stop
//!   nothing) and its curve be read again, WRITTEN_OFF ends it as lost
//!   (its input and gas; no exit, no BUY held back by it), EXIT_REQUESTED
//!   wants its exit now (the strategy's, at its price); ADOPTED takes a
//!   G0.7 BUY's acquisition (its G0.8 result) as an included lot.
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
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
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
    /// 01.11. (REV.1.0 R2): where an included C3 BUY landed against its
    /// launch, from its block (`Fact::placement`).
    Placed,
    /// REV.1.0 R3, the operator's (`Fact::operator`), of a lot (its BUY's
    /// hash): its stopped exits go on, its curve is read again.
    Resumed,
    /// The lot is lost: closed at its input and gas, never exited.
    WrittenOff,
    /// The lot's exit is wanted now.
    ExitRequested,
    /// A G0.7 BUY's acquisition as an included lot (a first fact).
    Adopted,
    /// R3b, of an unresolved transaction whose bytes may still land: its
    /// nonce is to be burnt (the buyer's plain transfer of nothing to
    /// itself at it, through the armed path).
    BurnRequested,
}

/// REV.1.0 R3: who decided an operator's fact, and on what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operator {
    /// The request it answers: once in the ledger.
    pub request_id: String,
    pub decided_by: String,
    /// `Adopted`: the SHA-256 of the G0.8 result it rests on.
    pub evidence: Option<[u8; 32]>,
    /// When it was written (the entry's time, Unix ms; the ledger sets it
    /// on append): a lot written off is lost on that day.
    pub at_ms: u64,
}

/// REV.1.0 R3: where a chain fact comes from when not from the canonical
/// follower's block (BSC3-024/041).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactSource {
    /// R3b: a block the receipt recovery found (final, the second source's;
    /// its receipt's, or where the sender's nonce passes the
    /// transaction's).
    Receipt,
}

impl FactSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::Receipt => "RECEIPT",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [Self::Receipt].into_iter().find(|s| s.name() == text)
    }
}

/// 01.11. (REV.1.0 R2): where an included C3 BUY landed against its launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub verdict: PlacementVerdict,
    /// The launch's transaction index in the BUY's block (none: the launch
    /// is not in it).
    pub launch_index: Option<u64>,
    pub own_index: u64,
    /// Other traders' BUYs of the token between the launch and it.
    pub buys_before: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementVerdict {
    /// Right behind its launch: no other trader's BUY between.
    First,
    /// Another trader's BUY of the token came between.
    Late,
    /// Its block holds no launch of its token.
    OtherBlock,
    /// Included, but no token reached the buyer.
    NoTokens,
}

impl PlacementVerdict {
    pub const ALL: [Self; 4] = [Self::First, Self::Late, Self::OtherBlock, Self::NoTokens];

    pub fn name(self) -> &'static str {
        match self {
            Self::First => "FIRST",
            Self::Late => "LATE",
            Self::OtherBlock => "OTHER_BLOCK",
            Self::NoTokens => "NO_TOKENS",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == text)
    }
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
            Self::Placed => "PLACED",
            Self::Resumed => "RESUMED",
            Self::WrittenOff => "WRITTEN_OFF",
            Self::ExitRequested => "EXIT_REQUESTED",
            Self::Adopted => "ADOPTED",
            Self::BurnRequested => "BURN_REQUESTED",
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
            Self::Placed,
            Self::Resumed,
            Self::WrittenOff,
            Self::ExitRequested,
            Self::Adopted,
            Self::BurnRequested,
        ]
        .into_iter()
        .find(|k| k.name() == text)
    }

    /// A transaction's first fact: what its release (or admission) made of
    /// it, or the operator's adoption of a G0.7 BUY.
    fn first(self) -> bool {
        matches!(
            self,
            Self::Released | Self::Unknown | Self::Withdrawn | Self::Adopted
        )
    }

    /// The operator's facts (`Fact::operator`).
    pub fn operators(self) -> bool {
        matches!(
            self,
            Self::Resumed
                | Self::WrittenOff
                | Self::ExitRequested
                | Self::Adopted
                | Self::BurnRequested
        )
    }
}

fn intent_of(text: &str) -> Option<LiveIntentKind> {
    [
        LiveIntentKind::Buy,
        LiveIntentKind::Approve,
        LiveIntentKind::Sell,
        LiveIntentKind::Burn,
    ]
    .into_iter()
    .find(|k| k.name() == text)
}

#[cfg(test)]
thread_local! {
    /// The next this many appends of this thread fail (a medium error).
    pub(crate) static FAIL_APPENDS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
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
    /// v3 (REV.1.0 R2): the creator of the launch of a BUY's token (a
    /// Strategy C BUY's at its release, a C3 BUY's in its `Placed` fact):
    /// the creator rule of its exit after a restart.
    pub launcher: Option<[u8; 20]>,
    /// v3: `Placed`: where the C3 BUY landed.
    pub placement: Option<Placement>,
    /// v4 (REV.1.0 R3): a BUY's attempt (a launch's C3 BUY: its lease's,
    /// the lease's own fact the same; another BUY: its own hash).
    pub attempt: Option<[u8; 32]>,
    /// v4: exactly in an operator's fact.
    pub operator: Option<Operator>,
    /// v4: a chain fact not from the follower's block.
    pub source: Option<FactSource>,
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
            operator: None,
            source: None,
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
            launcher: None,
            placement: None,
            // UNKNOWN: its attempt's slots stay (BSC3-068).
            attempt: None,
            operator: None,
            source: None,
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

/// The entries a ledger handle has decoded (BSC3-046): their sequence
/// numbers, ascending, and their facts in the same order.
#[derive(Debug, Default)]
struct Read {
    seqs: Vec<u64>,
    facts: Vec<Fact>,
    /// When each was written (its provenance, Unix ms).
    stamps: Vec<u64>,
}

/// The ledger under an owner state directory.
#[derive(Clone, Debug)]
pub struct Ledger {
    dir: PathBuf,
    /// What [`Ledger::facts`] decoded so far (shared by its clones):
    /// published entries never change, so only new ones are read.
    read: std::sync::Arc<std::sync::Mutex<Read>>,
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
            read: Default::default(),
            #[cfg(test)]
            cut: None,
        }
    }

    /// `bytes` into `file`, synced (tests: cut off as a crash would).
    fn write_entry(&self, file: &mut fs::File, bytes: &[u8]) -> std::io::Result<()> {
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
    /// An operator's fact is written at `now_ms` (its `at_ms`).
    pub fn append(&self, fact: &Fact, now_ms: u64) -> Result<u64, String> {
        #[cfg(test)]
        if FAIL_APPENDS.with(|f| f.get().checked_sub(1).map(|n| f.set(n)).is_some()) {
            return Err("the ledger append failed (test)".into());
        }
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
            .set_opt(k::SENDER, fact.origin.map(|o| Value::Address(o.sender)))
            .set_opt(k::LAUNCHER, fact.launcher.map(Value::Address))
            .set_opt(k::PLACEMENT, fact.placement.map(|p| p.verdict.name()))
            .set_opt(k::LAUNCH_INDEX, fact.placement.and_then(|p| p.launch_index))
            .set_opt(k::OWN_INDEX, fact.placement.map(|p| p.own_index))
            .set_opt(k::BUYS_BEFORE, fact.placement.map(|p| p.buys_before))
            .set_opt(k::ATTEMPT, fact.attempt.map(Value::B256))
            .set_opt(
                k::REQUEST_ID,
                fact.operator.as_ref().map(|o| o.request_id.as_str()),
            )
            .set_opt(
                k::DECIDED_BY,
                fact.operator.as_ref().map(|o| o.decided_by.as_str()),
            )
            .set_opt(
                k::EVIDENCE_SHA256,
                fact.operator
                    .as_ref()
                    .and_then(|o| o.evidence)
                    .map(Value::B256),
            )
            .set_opt(k::SOURCE, fact.source.map(FactSource::name));
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
                Ok(f) => {
                    // R3 (BSC3-045): held locked until it is published and
                    // gone, so a start's cleanup takes only a dead writer's
                    // file; one it took before this lock is no longer at
                    // its path: another one is written.
                    f.lock().map_err(e)?;
                    let mine = fs::metadata(&path)
                        .and_then(|at| Ok(at.ino() == f.metadata()?.ino()))
                        .unwrap_or(false);
                    if mine {
                        break (f, path);
                    }
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(e(err)),
            }
        };
        let mut file = file;
        if let Err(err) = self.write_entry(&mut file, &bytes) {
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

    /// R3 (BSC3-045): the temporary files of appends whose writer ended
    /// before publishing (a crash) are removed: each one whose lock this
    /// takes (a live writer holds its own). How many.
    pub fn clear_pending(&self) -> Result<usize, String> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(0),
            Err(err) => return Err(format!("{}: {err}", self.dir.display())),
        };
        let mut cleared = 0;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("pending-") && name.ends_with(".tmp")) {
                continue;
            }
            let Ok(file) = fs::File::open(entry.path()) else {
                continue;
            };
            if file.try_lock().is_ok() {
                fs::remove_file(entry.path()).map_err(|e| format!("{name}: {e}"))?;
                cleared += 1;
            }
        }
        if cleared > 0 {
            fs::File::open(&self.dir)
                .and_then(|d| d.sync_all())
                .map_err(|e| format!("{}: {e}", self.dir.display()))?;
        }
        Ok(cleared)
    }

    /// R3 (BSC3-045, `bsc_runtime live ledger check`): every published
    /// entry decoded on its own: how many, and each one that does not
    /// decode (its sequence number and why).
    pub fn check(&self) -> Result<(usize, Vec<(u64, String)>), String> {
        let names = self.names()?;
        let bad = names
            .iter()
            .filter_map(|(seq, path)| Self::entry(path).err().map(|why| (*seq, why)))
            .collect();
        Ok((names.len(), bad))
    }

    fn quarantine_dir(&self) -> PathBuf {
        self.dir.with_file_name("live-ledger-quarantine")
    }

    /// R3 (BSC3-045, `bsc_runtime live ledger quarantine`): the entry `seq`
    /// that does not decode moves out of the ledger (a readable fact never
    /// does), once per request: its record first (published whole), then
    /// the entry whole beside it, both directories synced. The ledger then
    /// reads, but what the entry said is not known to it: an admission
    /// without its first fact comes back as UNKNOWN, but a launch's C3 BUY
    /// has no admission of its own (its lease's) and an inclusion only
    /// from the chain. So the BUYs of every grant written at or before the
    /// owner generation (`generation`, read under the operator's lock: the
    /// one the operator expects, LIVE_REQUEST_STALE otherwise) stay refused
    /// ([`Self::quarantined`]) and no slot goes back any more (BSC3-068);
    /// what the entry said is the operator's reconstruction. The
    /// generation recorded (the request's again: its replay).
    pub fn quarantine(
        &self,
        seq: u64,
        request_id: &str,
        decided_by: &str,
        expected_generation: u64,
        generation: impl FnOnce() -> Result<u64, String>,
        now_ms: u64,
    ) -> Result<u64, String> {
        use darkbot_bsc_record::schema::bsc_runtime::live_ledger_quarantine as q;
        let _deciding = self.operator_lock()?;
        if !crate::sources::safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        if decided_by.trim().is_empty() || decided_by.chars().any(char::is_control) {
            return Err("--decided-by names who decided".into());
        }
        let qdir = self.quarantine_dir();
        let record = qdir.join(format!("{seq:020}.dbrec"));
        let moved = qdir.join(format!("{seq:020}.entry"));
        let entry = self.dir.join(format!("{seq:020}.dbrec"));
        let e = |p: &Path, err: std::io::Error| format!("{}: {err}", p.display());
        let (recorded, at) = match fs::read(&record) {
            Ok(bytes) => {
                let r = decode(&q::SCHEMA, &bytes).map_err(|x| x.0)?;
                if r.text(q::REQUEST_ID).map_err(|x| x.0)? != request_id {
                    return Err(format!(
                        "{LIVE_REQUEST_REUSED}: entry {seq} was quarantined by another request"
                    ));
                }
                (
                    r.b256(q::SHA256).map_err(|x| x.0)?,
                    r.u64(q::GENERATION).map_err(|x| x.0)?,
                )
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                let bytes = fs::read(&entry).map_err(|err| e(&entry, err))?;
                let why = match Self::entry(&entry) {
                    Ok(_) => {
                        return Err(format!("entry {seq} decodes: a fact is never quarantined"));
                    }
                    Err(why) => why,
                };
                // R3b: at the owner generation the operator saw (the
                // grants it keeps refused), read under this lock.
                let generation = generation()?;
                if generation != expected_generation {
                    return Err(format!(
                        "{LIVE_REQUEST_STALE}: the owner generation is {generation}, not {expected_generation}"
                    ));
                }
                let mut d = Draft::new(&q::SCHEMA);
                d.set(q::SEQ, seq)
                    .set(q::SHA256, Value::B256(darkbot_bsc_record::sha256(&bytes)))
                    .set(q::PROVENANCE_TOOL, "bsc_runtime")
                    .set(q::PROVENANCE_OBSERVED_UNIX_MS, now_ms)
                    .set(q::REQUEST_ID, request_id)
                    .set(q::DECIDED_BY, decided_by)
                    .set(q::WHY, why.as_str())
                    .set(q::GENERATION, generation);
                let encoded = d.encode().map_err(|x| x.to_string())?;
                fs::DirBuilder::new()
                    .mode(0o700)
                    .recursive(true)
                    .create(&qdir)
                    .map_err(|err| e(&qdir, err))?;
                // Review F4: published whole or not at all (a crash leaves
                // no torn record that would refuse its own rerun).
                crate::sources::write_new(&record, &encoded).map_err(|err| e(&record, err))?;
                (darkbot_bsc_record::sha256(&bytes), generation)
            }
            Err(err) => return Err(e(&record, err)),
        };
        // Run again (a crash in between), only the recorded bytes move,
        // never another entry, never over the moved one.
        let unmoved = fs::read(&entry)
            .ok()
            .is_some_and(|b| darkbot_bsc_record::sha256(&b) == recorded);
        if unmoved && !moved.exists() {
            fs::rename(&entry, &moved).map_err(|err| e(&entry, err))?;
        }
        for d in [&qdir, &self.dir] {
            fs::File::open(d)
                .and_then(|f| f.sync_all())
                .map_err(|err| e(d, err))?;
        }
        Ok(at)
    }

    /// R3: the highest owner generation an entry of this ledger was
    /// quarantined at (none: never): the BUYs of a grant written at or
    /// before it stay refused. A quarantine record that does not decode
    /// counts as one at every generation.
    pub fn quarantined(&self) -> Result<Option<u64>, String> {
        use darkbot_bsc_record::schema::bsc_runtime::live_ledger_quarantine as q;
        let qdir = self.quarantine_dir();
        let entries = match fs::read_dir(&qdir) {
            Ok(e) => e,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(format!("{}: {err}", qdir.display())),
        };
        let mut highest = None;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_name().to_string_lossy().ends_with(".dbrec") {
                continue;
            }
            let generation = fs::read(entry.path())
                .ok()
                .and_then(|b| decode(&q::SCHEMA, &b).ok())
                .and_then(|r| r.u64(q::GENERATION).ok())
                .unwrap_or(u64::MAX);
            highest = highest.max(Some(generation));
        }
        Ok(highest)
    }

    /// R3b: the numbers of the quarantined entries, ascending (a record
    /// named otherwise counts as none).
    pub fn quarantined_seqs(&self) -> Result<Vec<u64>, String> {
        let qdir = self.quarantine_dir();
        let entries = match fs::read_dir(&qdir) {
            Ok(e) => e,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(format!("{}: {err}", qdir.display())),
        };
        let mut seqs = Vec::new();
        for entry in entries {
            let name = entry.map_err(|e| e.to_string())?.file_name();
            if let Some(seq) = name
                .to_str()
                .and_then(|n| n.strip_suffix(".dbrec"))
                .and_then(|n| n.parse::<u64>().ok())
            {
                seqs.push(seq);
            }
        }
        seqs.sort_unstable();
        Ok(seqs)
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

    /// The next sequence number: after every entry, a quarantined one too
    /// (R3: its number is never taken again).
    fn count(&self) -> Result<u64, String> {
        let published = self.names()?.last().map_or(0, |(seq, _)| seq + 1);
        let quarantined = match fs::read_dir(self.quarantine_dir()) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    name.strip_suffix(".dbrec")?.parse::<u64>().ok()
                })
                .max()
                .map_or(0, |seq| seq + 1),
            Err(err) if err.kind() == ErrorKind::NotFound => 0,
            Err(err) => return Err(format!("{}: {err}", self.quarantine_dir().display())),
        };
        Ok(published.max(quarantined))
    }

    /// R3b: the ledger's version, its next entry's sequence number (never
    /// reused; a quarantined one counted): an operator's lot decision or
    /// nonce burn expects the version it was decided on (its
    /// compare-and-swap; the LIVE tab shows it from R3b-2b-2).
    pub fn version(&self) -> Result<u64, String> {
        self.count()
    }

    /// R3b: refused (LIVE_REQUEST_STALE) unless the ledger is still at
    /// `expected`; under the operator's lock, after the request's replay.
    fn expect_version(&self, expected: u64) -> Result<(), String> {
        let version = self.count()?;
        match version == expected {
            true => Ok(()),
            false => Err(format!(
                "{LIVE_REQUEST_STALE}: the ledger's version is {version}, not {expected}"
            )),
        }
    }

    /// Every fact in order. One that does not decode refuses the whole
    /// ledger: a fold never under-counts. Only entries this handle has not
    /// decoded yet are read (BSC3-046: published entries never change);
    /// when the entries on disk are not the ones it read followed by new
    /// ones (one appeared before them, or went), it reads all of them.
    pub fn facts(&self) -> Result<Vec<Fact>, String> {
        Ok(self.read()?.facts.clone())
    }

    /// REV.1.0 R3: how many facts the ledger holds (the new ones read, no
    /// copy): another process's are folded when it grew.
    pub fn fact_count(&self) -> Result<usize, String> {
        Ok(self.read()?.facts.len())
    }

    /// REV.1.0 R2: each fact with the time it was written (Unix ms), in
    /// ledger order.
    pub fn stamped(&self) -> Result<Vec<(Fact, u64)>, String> {
        let read = self.read()?;
        Ok(read
            .facts
            .iter()
            .cloned()
            .zip(read.stamps.iter().copied())
            .collect())
    }

    /// The entries decoded so far, the ones published since added.
    fn read(&self) -> Result<std::sync::MutexGuard<'_, Read>, String> {
        let names = self.names()?;
        let mut read = self
            .read
            .lock()
            .map_err(|_| format!("{}: the read entries are poisoned", self.dir.display()))?;
        let known = read.seqs.len();
        let prefix = names.len() >= known
            && names[..known]
                .iter()
                .zip(&read.seqs)
                .all(|((seq, _), known)| seq == known);
        if !prefix {
            *read = Read::default();
        }
        let from = read.seqs.len();
        for (seq, path) in &names[from..] {
            let (fact, stamp) = Self::entry(path)?;
            read.seqs.push(*seq);
            read.facts.push(fact);
            read.stamps.push(stamp);
        }
        Ok(read)
    }

    /// One published entry's fact and when it was written.
    fn entry(path: &Path) -> Result<(Fact, u64), String> {
        let bad = |why: &str| format!("{}: {why}", path.display());
        let bytes = fs::read(path).map_err(|e| bad(&e.to_string()))?;
        let r = decode(&k::SCHEMA, &bytes)
            .or_else(|_| decode(&k::V3, &bytes))
            .or_else(|_| decode(&k::V2, &bytes))
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
        let placement = match (
            r.opt_text(k::PLACEMENT).map_err(e)?,
            r.opt_u64(k::OWN_INDEX).map_err(e)?,
            r.opt_u64(k::BUYS_BEFORE).map_err(e)?,
        ) {
            (Some(v), Some(own_index), Some(buys_before)) => Some(Placement {
                verdict: PlacementVerdict::parse(v).ok_or_else(|| bad("placement"))?,
                launch_index: r.opt_u64(k::LAUNCH_INDEX).map_err(e)?,
                own_index,
                buys_before,
            }),
            (None, None, None) if r.opt_u64(k::LAUNCH_INDEX).map_err(e)?.is_none() => None,
            _ => return Err(bad("a placement is named whole or not at all")),
        };
        let kind = FactKind::parse(r.text(k::KIND).map_err(e)?).ok_or_else(|| bad("kind"))?;
        // Review L9: a placement exactly in a PLACED fact.
        if (kind == FactKind::Placed) != placement.is_some() {
            return Err(bad("a placement is named exactly in a PLACED fact"));
        }
        // R3: the operator exactly in an operator's fact, its evidence
        // exactly in an adoption; a source only on a chain fact.
        let operator = match (
            r.opt_text(k::REQUEST_ID).map_err(e)?,
            r.opt_text(k::DECIDED_BY).map_err(e)?,
        ) {
            (Some(request_id), Some(decided_by)) => Some(Operator {
                request_id: request_id.to_string(),
                decided_by: decided_by.to_string(),
                evidence: r.opt_b256(k::EVIDENCE_SHA256).map_err(e)?,
                at_ms: r.u64(k::PROVENANCE_OBSERVED_UNIX_MS).map_err(e)?,
            }),
            (None, None) if r.opt_b256(k::EVIDENCE_SHA256).map_err(e)?.is_none() => None,
            _ => return Err(bad("an operator is named whole or not at all")),
        };
        if kind.operators() != operator.is_some()
            || (kind == FactKind::Adopted)
                != operator.as_ref().is_some_and(|o| o.evidence.is_some())
        {
            return Err(bad("an operator is named exactly in an operator's fact"));
        }
        let source = match r.opt_text(k::SOURCE).map_err(e)? {
            Some(s) => Some(FactSource::parse(s).ok_or_else(|| bad("source"))?),
            None => None,
        };
        let chain = matches!(
            kind,
            FactKind::Included | FactKind::Reverted | FactKind::Replaced
        );
        if source.is_some() && !chain {
            return Err(bad("a source is named only on a chain fact"));
        }
        let fact = Fact {
            kind,
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
            launcher: r.opt_address(k::LAUNCHER).map_err(e)?,
            placement,
            attempt: r.opt_b256(k::ATTEMPT).map_err(e)?,
            operator,
            source,
        };
        Ok((fact, r.u64(k::PROVENANCE_OBSERVED_UNIX_MS).map_err(e)?))
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
    /// REV.1.0 R2: where its inclusion landed against its launch (a C3
    /// BUY), and its token's launch creator.
    pub placement: Option<Placement>,
    pub launcher: Option<[u8; 20]>,
    /// REV.1.0 R3: what the operator decided of the lot this BUY opened.
    pub decided: Decided,
}

/// REV.1.0 R3: what the operator decided of a lot (facts on its BUY).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decided {
    /// Its RESUMED facts: each lets its stopped exits go on and its curve
    /// be read again.
    pub resumes: u64,
    /// Its exit steps reverted before its last RESUMED: they stop nothing.
    pub excused: Vec<[u8; 32]>,
    /// WRITTEN_OFF: when (its fact's time).
    pub written_off: Option<u64>,
    pub exit_requested: bool,
    /// R3b: BURN_REQUESTED of an unresolved transaction (its nonce).
    pub burn_requested: bool,
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
    /// REV.1.0 R3: the operator wrote it off (nothing out): its input and
    /// gas are lost, its tokens never exited.
    WrittenOff,
}

impl PositionState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Open => "OPEN",
            Self::Exiting => "EXITING",
            Self::Closed => "CLOSED",
            Self::Failed => "FAILED",
            Self::WrittenOff => "WRITTEN_OFF",
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
    /// The grant its BUY was admitted under (an exit step without a lot:
    /// its own).
    pub grant_generation: u64,
    /// When it closed: its SELL's block (closed), its BUY's (failed).
    pub closed_ms: Option<u64>,
    /// An approval of this lot is included: its SELL needs no other.
    pub approved: bool,
    /// REV.1.0 R5: its SELL is included without the Portal's trade event of
    /// the buyer (a DEX-routed SELL): the proceeds are unknown, counted as 0
    /// (its PnL a lower bound: the loss limits see the worst case).
    pub proceeds_unknown: bool,
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
    /// REV.1.0 R2: the grant generations with a C3 BUY placed other than
    /// first (what the BUY path asks, in place of every transaction).
    pub misplaced: std::collections::BTreeSet<u64>,
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

    /// R3b: the gas the nonce burns of `of` paid (included or reverted):
    /// of no lot, a loss of its grant and its day all the same.
    pub fn burn_gas(&self, of: impl Fn(&Tx) -> bool) -> U256 {
        self.txs
            .iter()
            .filter(|t| {
                t.released.intent == LiveIntentKind::Burn
                    && matches!(t.state, FactKind::Included | FactKind::Reverted)
                    && of(t)
            })
            .map(Tx::gas)
            .fold(U256::ZERO, |a, b| a + b)
    }

    /// The lot `tx_hash` counts in.
    pub fn lot_of(&self, tx_hash: &[u8; 32]) -> Option<&Position> {
        let lot = self.tx(tx_hash)?.lot?;
        self.positions.iter().find(|p| p.lot == lot)
    }

    /// BSC3-068: the attempts no byte of which left: every BUY fact of
    /// the attempt (its transaction's and its lease's) WITHDRAWN, nothing
    /// else of it, and the BUY's own fact among them (review F1: a lease's
    /// fact alone proves nothing of its BUY). Their slots may go back.
    pub fn withdrawn_attempts(&self) -> Vec<[u8; 32]> {
        // (attempt, every fact withdrawn, its BUY's own fact seen)
        let mut out: Vec<([u8; 32], bool, bool)> = Vec::new();
        for t in self
            .txs
            .iter()
            .filter(|t| t.released.intent == LiveIntentKind::Buy)
        {
            let attempt = t.released.attempt.unwrap_or(t.released.tx_hash);
            // Re-check RC2: one included once (its block orphaned later)
            // was public: never withdrawn again.
            let withdrawn = t.state == FactKind::Withdrawn
                && t.released.kind == FactKind::Withdrawn
                && !t.orphaned;
            let own = t.released.token != [0; 20];
            match out.iter_mut().find(|(a, _, _)| *a == attempt) {
                Some((_, all, buy)) => {
                    *all &= withdrawn;
                    *buy |= own;
                }
                None => out.push((attempt, withdrawn, own)),
            }
        }
        out.into_iter()
            .filter_map(|(a, all, buy)| (all && buy).then_some(a))
            .collect()
    }

    /// `token`'s open lot: its latest one open or exiting.
    pub fn open_lot(&self, token: &[u8; 20]) -> Option<&Position> {
        self.positions.iter().rev().find(|p| {
            p.token == *token && matches!(p.state, PositionState::Open | PositionState::Exiting)
        })
    }

    /// R3b: the BUY of the lot `lot` when the operator's decision `what`
    /// is taken now (exit-now's switch aside), else why not: the lot must be
    /// its token's open lot; write-off and exit-now need it OPEN with no
    /// step of its exit out. One rule for [`Ledger::decide`] and the LIVE tab.
    pub fn decidable(&self, lot: &[u8; 32], what: LotDecision) -> Result<&Tx, String> {
        let buy = self
            .tx(lot)
            .filter(|t| t.released.intent == LiveIntentKind::Buy)
            .ok_or_else(|| {
                format!(
                    "{LIVE_LOT_NOT_OPEN}: no BUY of the ledger is 0x{}",
                    hex(lot)
                )
            })?;
        let p = self
            .open_lot(&buy.released.token)
            .filter(|p| p.lot == *lot)
            .ok_or_else(|| {
                format!(
                    "{LIVE_LOT_NOT_OPEN}: 0x{} is not its token's open lot",
                    hex(lot)
                )
            })?;
        let out = self
            .unresolved()
            .any(|t| t.lot == Some(*lot) && t.released.intent.exit_step());
        if what != LotDecision::Resume && (out || p.state != PositionState::Open) {
            return Err(format!("{LIVE_LOT_EXIT_OUT}: a step of its exit is out"));
        }
        Ok(buy)
    }

    /// R3b: the unresolved transaction `tx` when `buyer` may have its nonce
    /// burnt, else why not: its signed bytes may still land (its block left
    /// the chain, or its window is unknown: one its window bounds resolves
    /// without a burn) and it is the buyer's (review LOW-2). One rule for
    /// [`Ledger::request_burn`] and the LIVE tab.
    pub fn burnable(&self, tx: &[u8; 32], buyer: [u8; 20]) -> Result<&Tx, String> {
        let target = self
            .unresolved()
            .find(|t| t.released.tx_hash == *tx)
            .ok_or_else(|| {
                format!(
                    "{LIVE_BURN_REFUSED}: 0x{} is no unresolved transaction of the ledger",
                    hex(tx)
                )
            })?;
        if !target.orphaned && target.released.max_block.is_some() {
            return Err(format!(
                "{LIVE_BURN_REFUSED}: 0x{}'s window bounds it (it resolves without a burn)",
                hex(tx)
            ));
        }
        if target.released.origin.is_some_and(|o| o.sender != buyer) {
            return Err(format!(
                "{LIVE_BURN_REFUSED}: 0x{} is another sender's than the buyer",
                hex(tx)
            ));
        }
        Ok(target)
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
                    // R3: an adopted G0.7 BUY is included from its fact.
                    let adopted = kind == FactKind::Adopted;
                    txs.push(Tx {
                        released: f.clone(),
                        state: match adopted {
                            true => FactKind::Included,
                            false => f.kind,
                        },
                        inclusion: adopted.then(|| f.clone()),
                        orphaned: false,
                        lot: None,
                        responses: Vec::new(),
                        placement: None,
                        launcher: f.launcher,
                        decided: Decided::default(),
                    });
                }
            }
            kind => {
                // R3: a RESUMED lot's exit steps reverted so far stop
                // nothing any more.
                let reverted: Vec<[u8; 32]> = match kind {
                    FactKind::Resumed => txs
                        .iter()
                        .filter(|t| {
                            t.released.token == f.token
                                && t.released.intent.exit_step()
                                && t.state == FactKind::Reverted
                        })
                        .map(|t| t.released.tx_hash)
                        .collect(),
                    _ => Vec::new(),
                };
                let Some(t) = txs.iter_mut().find(|t| t.released.tx_hash == f.tx_hash) else {
                    continue;
                };
                match kind {
                    FactKind::Resumed => {
                        t.decided.resumes += 1;
                        t.decided.excused = reverted;
                    }
                    FactKind::WrittenOff => {
                        t.decided.written_off = f.operator.as_ref().map(|o| o.at_ms);
                    }
                    FactKind::ExitRequested => t.decided.exit_requested = true,
                    FactKind::BurnRequested => t.decided.burn_requested = true,
                    FactKind::Responded => t.responses.extend(f.response.clone()),
                    FactKind::Placed => {
                        t.placement = f.placement;
                        t.launcher = f.launcher.or(t.launcher);
                    }
                    FactKind::Orphaned => {
                        // Its own inclusion left the chain (not one that
                        // replaced it): its bytes were public.
                        t.orphaned |= matches!(t.state, FactKind::Included | FactKind::Reverted);
                        // An adopted BUY whose block went is not known.
                        t.state = match t.released.kind {
                            FactKind::Adopted => FactKind::Unknown,
                            k => k,
                        };
                        t.inclusion = None;
                        t.placement = None;
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
    // R3b: what the nonce burns cost (of no lot).
    let mut burnt = U256::ZERO;
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
        // A nonce burn is of no token and no lot: its gas only.
        if t.released.intent == LiveIntentKind::Burn {
            if spent {
                burnt += t.gas();
            }
            continue;
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
                    grant_generation: t.released.grant_generation,
                    closed_ms: None,
                    approved: false,
                    proceeds_unknown: false,
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
            (LiveIntentKind::Buy, FactKind::Reverted) => {
                p.state = PositionState::Failed;
                p.closed_ms = inclusion.and_then(|f| f.block).map(|b| b.2);
            }
            (LiveIntentKind::Approve, FactKind::Included) => p.approved = true,
            (LiveIntentKind::Sell, FactKind::Included) => {
                p.state = PositionState::Closed;
                p.proceeds_unknown = inclusion.and_then(|f| f.eth_wei).is_none();
                p.proceeds_wei = inclusion.and_then(|f| f.eth_wei).unwrap_or_default();
                p.closed_ms = inclusion.and_then(|f| f.block).map(|b| b.2);
            }
            (LiveIntentKind::Sell, FactKind::Released | FactKind::Unknown)
                if p.state == PositionState::Open =>
            {
                p.state = PositionState::Exiting;
            }
            _ => {}
        }
    }
    // R3: a lot written off with nothing out is lost (a step still out
    // decides it first; one included later closes it).
    for p in &mut book.positions {
        let off = txs
            .iter()
            .find(|t| t.released.tx_hash == p.lot)
            .and_then(|t| t.decided.written_off);
        if off.is_some() && p.state == PositionState::Open {
            p.state = PositionState::WrittenOff;
            p.closed_ms = off;
        }
    }
    for p in &mut book.positions {
        p.pnl_wei = match p.state {
            PositionState::Closed | PositionState::WrittenOff => Some(
                signed(p.proceeds_wei)
                    .saturating_sub(signed(p.input_wei))
                    .saturating_sub(signed(p.gas_wei)),
            ),
            PositionState::Failed => Some(-signed(p.gas_wei)),
            _ => None,
        };
        book.realized_pnl_wei = book.realized_pnl_wei.saturating_add(p.pnl_wei.unwrap_or(0));
    }
    book.realized_pnl_wei = book.realized_pnl_wei.saturating_sub(signed(burnt));
    // Review L2: from every PLACED fact, also one whose block left the
    // chain later (re-enabling is the operator's new grant, never a reorg).
    book.misplaced = facts
        .iter()
        .filter(|f| {
            f.kind == FactKind::Placed
                && f.placement
                    .is_some_and(|p| p.verdict != PlacementVerdict::First)
        })
        .map(|f| f.grant_generation)
        .collect();
    book.txs = txs;
    book
}

// ---- REV.1.0 R3: the operator's decisions ----

/// An operator's decision refused: the lot is not the token's open lot.
pub const LIVE_LOT_NOT_OPEN: &str = "LIVE_LOT_NOT_OPEN";
/// A step of the lot's exit is out (unresolved): it decides first.
pub const LIVE_LOT_EXIT_OUT: &str = "LIVE_LOT_EXIT_OUT";
/// `BSC_EXEC_LIVE_OPERATOR_EXIT` is OFF.
pub const LIVE_OPERATOR_EXIT_OFF: &str = "LIVE_OPERATOR_EXIT_OFF";
/// The request ID answers another decision already.
pub const LIVE_REQUEST_REUSED: &str = "LIVE_REQUEST_REUSED";
/// R3b: `BSC_EXEC_LIVE_NONCE_BURN` is OFF.
pub const LIVE_NONCE_BURN_OFF: &str = "LIVE_NONCE_BURN_OFF";
/// R3b: a burn requested of a transaction no burn is for: not unresolved,
/// or its window bounds it (it resolves at its end without one).
pub const LIVE_BURN_REFUSED: &str = "LIVE_BURN_REFUSED";
/// A G0.8 result whose BUY cannot be adopted as a lot.
pub const LIVE_ADOPTION_REFUSED: &str = "LIVE_ADOPTION_REFUSED";
/// R3b: an operator's request expects another ledger version (a lot's
/// decision, a nonce burn) or owner generation (a quarantine) than the
/// current one: it was decided on another state (its compare-and-swap).
pub const LIVE_REQUEST_STALE: &str = "LIVE_REQUEST_STALE";

/// What the operator decides of a lot (`bsc_runtime live lot`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LotDecision {
    /// Its stopped exits go on, its curve is read again.
    Resume,
    /// It is lost: closed at its input and gas, never exited.
    WriteOff,
    /// Its exit is wanted now (the strategy's, at its price).
    ExitNow,
}

impl LotDecision {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "resume" => Some(Self::Resume),
            "write-off" => Some(Self::WriteOff),
            "exit-now" => Some(Self::ExitNow),
            _ => None,
        }
    }

    /// R3b: its name in the LIVE tab and the gateway (`bsc.live.lot`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Resume => "RESUME",
            Self::WriteOff => "WRITE_OFF",
            Self::ExitNow => "EXIT_NOW",
        }
    }

    /// Every decision, in the LIVE tab's order.
    pub const ALL: [Self; 3] = [Self::Resume, Self::WriteOff, Self::ExitNow];

    /// Its fact's kind.
    pub fn kind(self) -> FactKind {
        match self {
            Self::Resume => FactKind::Resumed,
            Self::WriteOff => FactKind::WrittenOff,
            Self::ExitNow => FactKind::ExitRequested,
        }
    }
}

/// A G0.7 BUY's acquisition as `live adopt-g07` takes it from its G0.8
/// result and that result's chain capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct G07Buy {
    pub tx_hash: [u8; 32],
    pub token: [u8; 20],
    pub buyer: [u8; 20],
    /// The launch's creator (its transaction's sender).
    pub launcher: [u8; 20],
    pub nonce: u64,
    pub value_wei: U256,
    /// Its value and gas limit at its price: the most it could cost.
    pub max_wei: U256,
    pub gas_wei: U256,
    pub tokens: U256,
    /// Number, hash and time (ms) of its block.
    pub block: (u64, [u8; 32], u64),
    /// The G0.8 result's SHA-256.
    pub evidence: [u8; 32],
}

/// The G0.7 BUY of the G0.8 result `result` (bytes as on file) whose chain
/// capture `capture` holds its transaction, its launch's and its block
/// (`capture_sha256` from the result must match), as a lot of `buyer`:
/// included and successful in the canonical chain, its acquisition
/// complete and positive and attributable to the buyer, confirmed by at
/// least `finality_blocks` after its launch's block (beyond any reorg the
/// follower follows). Each refusal names why; the economic verdict of
/// G0.8 is not asked (a late BUY's tokens are held all the same).
pub fn g07_buy(
    result: &[u8],
    capture: &[u8],
    buyer: [u8; 20],
    finality_blocks: u64,
) -> Result<G07Buy, String> {
    use darkbot_bsc_record::schema::g08::{chain_block, chain_transaction, result as g};
    let refused = |why: &str| format!("{LIVE_ADOPTION_REFUSED}: {why}");
    let r = decode(&g::SCHEMA, result).map_err(|e| refused(&e.0))?;
    let e = |x: darkbot_bsc_record::RecordError| refused(&x.0);
    let journal = r
        .nested_all(g::CHAIN_JOURNAL)
        .map_err(e)?
        .into_iter()
        .next()
        .ok_or_else(|| refused("the result has no chain capture"))?;
    if journal.b256(g::chain_journal::SHA256).map_err(e)? != darkbot_bsc_record::sha256(capture) {
        return Err(refused("the chain capture is not the result's"));
    }
    let checks = [
        (
            r.u64(g::BUY_STATUS).map_err(e)? == 1,
            "its BUY did not succeed",
        ),
        (r.bool(g::CANONICAL).map_err(e)?, "its BUY is not canonical"),
        (
            r.bool(g::ACQUISITION_EVIDENCE_COMPLETE).map_err(e)?,
            "its acquisition evidence is incomplete",
        ),
        (
            r.bool(g::POSITIVE_ATTRIBUTABLE_ACQUISITION).map_err(e)?,
            "no positive acquisition is attributable to it",
        ),
        (
            r.address(g::BUYER).map_err(e)? == buyer,
            "its buyer is not the LIVE buyer",
        ),
        (
            r.u64(g::CONFIRMATIONS_AFTER_LAUNCH_BLOCK).map_err(e)? >= finality_blocks,
            "its launch's block is not final",
        ),
        (
            r.u64(g::CURRENT_HEAD)
                .map_err(e)?
                .saturating_sub(r.u64(g::BUY_BLOCK).map_err(e)?)
                >= finality_blocks,
            "its BUY's block is not final",
        ),
        (
            r.bool(g::BUY_SCOPE_OK).map_err(e)?,
            "its BUY is out of scope",
        ),
    ];
    if let Some((_, why)) = checks.iter().find(|(ok, _)| !ok) {
        return Err(refused(why));
    }
    let tokens = r
        .opt_u256(g::BUY_RECEIPT_TOKEN_NET)
        .map_err(e)?
        .map(U256::from_be_bytes)
        .filter(|t| !t.is_zero())
        .ok_or_else(|| refused("its receipt names no tokens received"))?;
    let tx_hash = r.b256(g::BUY_TX).map_err(e)?;
    let launch_tx = r.b256(g::LAUNCH_TX).map_err(e)?;
    let number = r.u64(g::BUY_BLOCK).map_err(e)?;
    let hash = r.b256(g::BUY_BLOCK_HASH).map_err(e)?;
    let (mut buy, mut launcher, mut block_ms) = (None, None, None);
    let mut reader =
        darkbot_bsc_journal::JournalReader::new(capture, darkbot_bsc_journal::DEFAULT_MAX_PAYLOAD)
            .map_err(|x| refused(&format!("chain capture: {x}")))?;
    while let Some(rec) = reader
        .next_record()
        .map_err(|x| refused(&format!("chain capture: {x}")))?
    {
        if let Ok(t) = decode(&chain_transaction::SCHEMA, &rec.payload) {
            let h = t.b256(chain_transaction::HASH).map_err(e)?;
            if h == tx_hash {
                buy = Some(t);
            } else if h == launch_tx {
                launcher = Some(t.address(chain_transaction::FROM).map_err(e)?);
            }
        } else if let Ok(b) = decode(&chain_block::SCHEMA, &rec.payload)
            && b.u64(chain_block::NUMBER).map_err(e)? == number
            && b.b256(chain_block::HASH).map_err(e)? == hash
        {
            block_ms = Some(match b.opt_u64(chain_block::MILLI_TIMESTAMP).map_err(e)? {
                Some(ms) => ms,
                None => b
                    .u64(chain_block::TIMESTAMP)
                    .map_err(e)?
                    .saturating_mul(1_000),
            });
        }
    }
    let t = buy.ok_or_else(|| refused("the chain capture has no BUY transaction"))?;
    if t.address(chain_transaction::FROM).map_err(e)? != buyer
        || t.b256(chain_transaction::BLOCK_HASH).map_err(e)? != hash
    {
        return Err(refused("the captured BUY is not the result's"));
    }
    let value_wei = U256::from_be_bytes(t.u256(chain_transaction::VALUE).map_err(e)?);
    if value_wei != U256::from_be_bytes(r.u256(g::BUY_VALUE_WEI).map_err(e)?) {
        return Err(refused("the captured BUY's value is not the result's"));
    }
    let gas_wei = U256::from_be_bytes(r.u256(g::BUY_GAS_COST_WEI).map_err(e)?);
    let gas_limit = U256::from(t.u64(chain_transaction::GAS).map_err(e)?);
    let max_wei = match t.opt_u256(chain_transaction::GAS_PRICE).map_err(e)? {
        Some(price) => value_wei + gas_limit * U256::from_be_bytes(price),
        None => value_wei + gas_wei,
    };
    Ok(G07Buy {
        tx_hash,
        token: r.address(g::TOKEN).map_err(e)?,
        buyer,
        launcher: launcher.ok_or_else(|| refused("the chain capture has no launch transaction"))?,
        nonce: t.u64(chain_transaction::NONCE).map_err(e)?,
        value_wei,
        max_wei: max_wei.max(value_wei + gas_wei),
        gas_wei,
        tokens,
        block: (
            number,
            hash,
            block_ms.ok_or_else(|| refused("the chain capture has no BUY block"))?,
        ),
        evidence: darkbot_bsc_record::sha256(result),
    })
}

impl Ledger {
    /// One operator decision at a time (another process's refuses this).
    fn operator_lock(&self) -> Result<fs::File, String> {
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&self.dir)
            .map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let path = self.dir.join("operator.lock");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        file.try_lock().map_err(|_| {
            format!(
                "{}: another operator decision is being written",
                path.display()
            )
        })?;
        Ok(file)
    }

    /// The request `request_id` (a safe ID, as an owner request's) of
    /// `decided_by`, once: the same request again (`same` of its fact)
    /// changes nothing (`Some(None)`), one under its ID of another decision
    /// is refused; a new one is `None`.
    fn request_seen(
        facts: &[Fact],
        request_id: &str,
        decided_by: &str,
        same: impl Fn(&Fact) -> bool,
    ) -> Result<Option<Option<u64>>, String> {
        if !crate::sources::safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        if decided_by.trim().is_empty() || decided_by.chars().any(char::is_control) {
            return Err("--decided-by names who decided".into());
        }
        match facts.iter().find(|f| {
            f.operator
                .as_ref()
                .is_some_and(|o| o.request_id == request_id)
        }) {
            Some(f) if same(f) => Ok(Some(None)),
            Some(_) => Err(format!(
                "{LIVE_REQUEST_REUSED}: {request_id} answers another decision"
            )),
            None => Ok(None),
        }
    }

    /// REV.1.0 R3 (`bsc_runtime live lot`): the operator's decision `what`
    /// of the lot `lot` (its BUY's hash) as its fact, once per request:
    /// resume and exit-now need the token's open lot, write-off and
    /// exit-now with no step of its exit out, exit-now
    /// `BSC_EXEC_LIVE_OPERATOR_EXIT` ON (`operator_exit`). A running
    /// runtime acts on it at its next reconcile. A new request expects the
    /// ledger at `expected_version` ([`Self::version`];
    /// LIVE_REQUEST_STALE otherwise). The fact's sequence number (`None`:
    /// the same request was written before).
    #[allow(clippy::too_many_arguments)]
    pub fn decide(
        &self,
        lot: [u8; 32],
        what: LotDecision,
        request_id: &str,
        decided_by: &str,
        operator_exit: bool,
        expected_version: u64,
        now_ms: u64,
    ) -> Result<Option<u64>, String> {
        let _deciding = self.operator_lock()?;
        let facts = self.facts()?;
        let kind = what.kind();
        if let Some(seen) = Self::request_seen(&facts, request_id, decided_by, |f| {
            f.kind == kind && f.tx_hash == lot
        })? {
            return Ok(seen);
        }
        self.expect_version(expected_version)?;
        let book = fold(&facts);
        let buy = book.decidable(&lot, what)?;
        if what == LotDecision::ExitNow && !operator_exit {
            return Err(LIVE_OPERATOR_EXIT_OFF.into());
        }
        let fact = Fact {
            operator: Some(Operator {
                request_id: request_id.into(),
                decided_by: decided_by.into(),
                evidence: None,
                at_ms: now_ms,
            }),
            ..Fact::about(&buy.released, kind)
        };
        self.append(&fact, now_ms).map(Some)
    }

    /// REV.1.0 R3b (`bsc_runtime live nonce-burn`): the operator wants the
    /// nonce of the unresolved transaction `tx` burnt, once per request: a
    /// transaction whose signed bytes may still land at any height (its
    /// block left the chain, or its window is not known) and keep the LIVE
    /// path from arming. Refused while `BSC_EXEC_LIVE_NONCE_BURN` is OFF; a
    /// new request expects the ledger at `expected_version`
    /// ([`Self::version`]; LIVE_REQUEST_STALE otherwise). The running
    /// runtime burns it at its next pass, when its nonce is the buyer's
    /// next and the path is armed.
    #[allow(clippy::too_many_arguments)]
    pub fn request_burn(
        &self,
        tx: [u8; 32],
        buyer: Option<[u8; 20]>,
        request_id: &str,
        decided_by: &str,
        nonce_burn: bool,
        expected_version: u64,
        now_ms: u64,
    ) -> Result<Option<u64>, String> {
        let _deciding = self.operator_lock()?;
        let facts = self.facts()?;
        if let Some(seen) = Self::request_seen(&facts, request_id, decided_by, |f| {
            f.kind == FactKind::BurnRequested && f.tx_hash == tx
        })? {
            return Ok(seen);
        }
        if !nonce_burn {
            return Err(LIVE_NONCE_BURN_OFF.into());
        }
        self.expect_version(expected_version)?;
        let buyer = buyer.ok_or("BSC_LIVE_BUYER is not configured")?;
        let book = fold(&facts);
        let target = book.burnable(&tx, buyer)?;
        let fact = Fact {
            operator: Some(Operator {
                request_id: request_id.into(),
                decided_by: decided_by.into(),
                evidence: None,
                at_ms: now_ms,
            }),
            ..Fact::about(&target.released, FactKind::BurnRequested)
        };
        self.append(&fact, now_ms).map(Some)
    }

    /// REV.1.0 R3 (`bsc_runtime live adopt-g07`): the G0.7 BUY `buy` as an
    /// included lot of the LIVE buyer on `chain_id` (its first fact, of no
    /// LIVE grant: generation 0; its slot is the G0.7 attempt's), once per
    /// request and transaction: a transaction the ledger has already is
    /// refused. A running runtime holds it at its next reconcile (its curve
    /// read, then the strategy decides its exit).
    pub fn adopt(
        &self,
        buy: &G07Buy,
        chain_id: u64,
        request_id: &str,
        decided_by: &str,
        now_ms: u64,
    ) -> Result<Option<u64>, String> {
        let _deciding = self.operator_lock()?;
        let facts = self.facts()?;
        if let Some(seen) = Self::request_seen(&facts, request_id, decided_by, |f| {
            f.kind == FactKind::Adopted && f.tx_hash == buy.tx_hash
        })? {
            return Ok(seen);
        }
        if facts.iter().any(|f| f.tx_hash == buy.tx_hash) {
            return Err(format!(
                "{LIVE_ADOPTION_REFUSED}: 0x{} is in the ledger already",
                hex(&buy.tx_hash)
            ));
        }
        // An open lot of the token would hide behind the adopted one.
        if fold(&facts).open_lot(&buy.token).is_some() {
            return Err(format!(
                "{LIVE_ADOPTION_REFUSED}: its token has an open lot already"
            ));
        }
        let fact = Fact {
            kind: FactKind::Adopted,
            tx_hash: buy.tx_hash,
            intent: LiveIntentKind::Buy,
            component: ComponentId::ExecC3,
            token: buy.token,
            nonce: buy.nonce,
            grant_generation: 0,
            input_wei: Some(buy.value_wei),
            max_wei: Some(buy.max_wei),
            block: Some(buy.block),
            gas_wei: Some(buy.gas_wei),
            tokens: Some(buy.tokens),
            eth_wei: Some(buy.value_wei),
            max_block: None,
            by_tx: None,
            response: None,
            origin: Some(NonceOrigin {
                chain_id,
                sender: buy.buyer,
            }),
            launcher: Some(buy.launcher),
            placement: None,
            attempt: None,
            operator: Some(Operator {
                request_id: request_id.into(),
                decided_by: decided_by.into(),
                evidence: Some(buy.evidence),
                at_ms: now_ms,
            }),
            source: None,
        };
        self.append(&fact, now_ms).map(Some)
    }
}

fn hex(bytes: &[u8]) -> String {
    alloy_primitives::hex::encode(bytes)
}

#[cfg(test)]
#[path = "live_ledger_tests.rs"]
mod tests;
