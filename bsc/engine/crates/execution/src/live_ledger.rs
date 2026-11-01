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
    /// Its receipt by hash, the block confirmed by the second source.
    Receipt,
    /// `Replaced`: no receipt, and the sender's confirmed nonce past it.
    Nonce,
}

impl FactSource {
    pub fn name(self) -> &'static str {
        match self {
            Self::Receipt => "RECEIPT",
            Self::Nonce => "NONCE",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [Self::Receipt, Self::Nonce]
            .into_iter()
            .find(|s| s.name() == text)
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
            Self::Resumed | Self::WrittenOff | Self::ExitRequested | Self::Adopted
        )
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
    /// owner `generation` stay refused ([`Self::quarantined`]) and no slot
    /// goes back any more (BSC3-068); what the entry said is the
    /// operator's reconstruction.
    pub fn quarantine(
        &self,
        seq: u64,
        request_id: &str,
        decided_by: &str,
        generation: u64,
        now_ms: u64,
    ) -> Result<(), String> {
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
        let recorded = match fs::read(&record) {
            Ok(bytes) => {
                let r = decode(&q::SCHEMA, &bytes).map_err(|x| x.0)?;
                if r.text(q::REQUEST_ID).map_err(|x| x.0)? != request_id {
                    return Err(format!(
                        "{LIVE_REQUEST_REUSED}: entry {seq} was quarantined by another request"
                    ));
                }
                r.b256(q::SHA256).map_err(|x| x.0)?
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                let bytes = fs::read(&entry).map_err(|err| e(&entry, err))?;
                let why = match Self::entry(&entry) {
                    Ok(_) => {
                        return Err(format!("entry {seq} decodes: a fact is never quarantined"));
                    }
                    Err(why) => why,
                };
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
                darkbot_bsc_record::sha256(&bytes)
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
        Ok(())
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
                                && t.released.intent != LiveIntentKind::Buy
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
                    grant_generation: t.released.grant_generation,
                    closed_ms: None,
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
            (LiveIntentKind::Buy, FactKind::Reverted) => {
                p.state = PositionState::Failed;
                p.closed_ms = inclusion.and_then(|f| f.block).map(|b| b.2);
            }
            (LiveIntentKind::Approve, FactKind::Included) => p.approved = true,
            (LiveIntentKind::Sell, FactKind::Included) => {
                p.state = PositionState::Closed;
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
/// A G0.8 result whose BUY cannot be adopted as a lot.
pub const LIVE_ADOPTION_REFUSED: &str = "LIVE_ADOPTION_REFUSED";

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

    fn kind(self) -> FactKind {
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
    /// runtime acts on it at its next reconcile. The fact's sequence number
    /// (`None`: the same request was written before).
    pub fn decide(
        &self,
        lot: [u8; 32],
        what: LotDecision,
        request_id: &str,
        decided_by: &str,
        operator_exit: bool,
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
        let book = fold(&facts);
        let buy = book
            .tx(&lot)
            .filter(|t| t.released.intent == LiveIntentKind::Buy)
            .ok_or_else(|| {
                format!(
                    "{LIVE_LOT_NOT_OPEN}: no BUY of the ledger is 0x{}",
                    hex(&lot)
                )
            })?;
        let p = book
            .open_lot(&buy.released.token)
            .filter(|p| p.lot == lot)
            .ok_or_else(|| {
                format!(
                    "{LIVE_LOT_NOT_OPEN}: 0x{} is not its token's open lot",
                    hex(&lot)
                )
            })?;
        let out = book
            .unresolved()
            .any(|t| t.lot == Some(lot) && t.released.intent != LiveIntentKind::Buy);
        if what != LotDecision::Resume && (out || p.state != PositionState::Open) {
            return Err(format!("{LIVE_LOT_EXIT_OUT}: a step of its exit is out"));
        }
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
            launcher: None,
            placement: None,
            attempt: None,
            operator: None,
            source: None,
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

    /// REV.1.0 R2: a `PLACED` fact keeps the placement and the launch's
    /// creator and reads them back; the fold puts them on its transaction
    /// (an orphaned inclusion takes the placement back); a v2 entry (before
    /// them) reads without them; a placement named in part refuses the
    /// ledger.
    #[test]
    fn a_placement_and_its_launcher_read_back_and_fold() {
        let dir = std::env::temp_dir().join(format!("bsc-live-placed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ledger = Ledger::open(&dir);
        let buy = Fact {
            component: ComponentId::ExecC3,
            ..released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300)
        };
        ledger.append(&buy, 1).unwrap();
        let block = Some((100, [0x10; 32], 5));
        let included = Fact {
            block,
            tokens: Some(U256::from(7u8)),
            eth_wei: Some(U256::from(1_000u64)),
            gas_wei: Some(U256::ZERO),
            ..Fact::about(&buy, FactKind::Included)
        };
        let placed = Fact {
            block,
            launcher: Some([0xC1; 20]),
            placement: Some(Placement {
                verdict: PlacementVerdict::Late,
                launch_index: Some(0),
                own_index: 2,
                buys_before: 1,
            }),
            ..Fact::about(&buy, FactKind::Placed)
        };
        ledger.append(&included, 2).unwrap();
        ledger.append(&placed, 3).unwrap();
        let facts = ledger.facts().unwrap();
        assert_eq!(facts[2], placed);
        let book = fold(&facts);
        let t = book.tx(&buy.tx_hash).unwrap();
        assert_eq!(t.placement, placed.placement);
        // The BUY path's question: this grant has a misplaced BUY.
        assert_eq!(book.misplaced, [buy.grant_generation].into());
        assert_eq!(t.launcher, Some([0xC1; 20]));
        assert_eq!(t.state, FactKind::Included);
        let orphaned = Fact {
            block,
            ..Fact::about(&buy, FactKind::Orphaned)
        };
        let mut again = facts.clone();
        again.push(orphaned);
        let again = fold(&again);
        assert_eq!(again.tx(&buy.tx_hash).unwrap().placement, None);
        // The halt stays: re-enabling is the operator's new grant (L2).
        assert_eq!(again.misplaced, [buy.grant_generation].into());
        // A v2 entry (no launcher, no placement) still reads.
        let mut d = Draft::new(&k::V2);
        d.set(k::KIND, "RELEASED")
            .set(k::TX_HASH, Value::B256([9; 32]))
            .set(k::PROVENANCE_TOOL, "test")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(k::INTENT, "BUY")
            .set(k::COMPONENT, "BSC_EXEC")
            .set(k::TOKEN, Value::Address([0xB; 20]))
            .set(k::NONCE, 2u64)
            .set(k::GRANT_GENERATION, 7u64);
        fs::write(
            dir.join("live-ledger").join(format!("{:020}.dbrec", 4)),
            d.encode().unwrap(),
        )
        .unwrap();
        let facts = ledger.facts().unwrap();
        assert_eq!((facts[3].launcher, facts[3].placement), (None, None));
        // A placement named in part refuses the ledger.
        let mut d = Draft::new(&k::SCHEMA);
        d.set(k::KIND, "PLACED")
            .set(k::TX_HASH, Value::B256([1; 32]))
            .set(k::PROVENANCE_TOOL, "test")
            .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(k::INTENT, "BUY")
            .set(k::COMPONENT, "BSC_EXEC")
            .set(k::TOKEN, Value::Address([0xA; 20]))
            .set(k::NONCE, 1u64)
            .set(k::GRANT_GENERATION, 7u64)
            .set(k::PLACEMENT, "LATE");
        fs::write(
            dir.join("live-ledger").join(format!("{:020}.dbrec", 5)),
            d.encode().unwrap(),
        )
        .unwrap();
        assert!(ledger.facts().unwrap_err().contains("placement"));
        // A PLACED fact without its placement, or a placement in another
        // fact, refuses the ledger too (review L9).
        for (kind, placed) in [("PLACED", false), ("INCLUDED", true)] {
            let mut d = Draft::new(&k::SCHEMA);
            d.set(k::KIND, kind)
                .set(k::TX_HASH, Value::B256([1; 32]))
                .set(k::PROVENANCE_TOOL, "test")
                .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
                .set(k::INTENT, "BUY")
                .set(k::COMPONENT, "BSC_EXEC")
                .set(k::TOKEN, Value::Address([0xA; 20]))
                .set(k::NONCE, 1u64)
                .set(k::GRANT_GENERATION, 7u64);
            if placed {
                d.set(k::PLACEMENT, "FIRST")
                    .set(k::OWN_INDEX, 1u64)
                    .set(k::BUYS_BEFORE, 0u64);
            }
            fs::write(
                dir.join("live-ledger").join(format!("{:020}.dbrec", 5)),
                d.encode().unwrap(),
            )
            .unwrap();
            let read = Ledger::open(&dir).facts();
            assert!(
                read.unwrap_err().contains("exactly in a PLACED fact"),
                "{kind}"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-046: a handle decodes each published entry once; a later read
    /// decodes only the new ones (published entries never change: one
    /// rewritten after it was read is not read again). When the entries on
    /// disk are not the ones it read followed by new ones (one went, or
    /// appeared before them), it reads all of them again, and an entry
    /// that does not decode refuses the ledger as before.
    #[test]
    fn the_ledger_decodes_only_entries_it_has_not_read() {
        let dir = std::env::temp_dir().join(format!("bsc-live-incremental-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let ledger = Ledger::open(&dir);
        let facts: Vec<Fact> = (1..=3u8)
            .map(|n| released(n, LiveIntentKind::Buy, n, 1_000, 1_300))
            .collect();
        ledger.append(&facts[0], 1).unwrap();
        ledger.append(&facts[1], 1).unwrap();
        assert_eq!(ledger.facts().unwrap(), facts[..2]);
        let entry = |seq: u64| dir.join("live-ledger").join(format!("{seq:020}.dbrec"));
        fs::write(entry(0), b"not a record").unwrap();
        ledger.append(&facts[2], 1).unwrap();
        assert_eq!(ledger.facts().unwrap(), facts);
        // A clone shares what was read; a new handle reads it all.
        assert_eq!(ledger.clone().facts().unwrap(), facts);
        assert!(Ledger::open(&dir).facts().is_err());
        // An entry went: all is read again, the damaged one refuses.
        fs::remove_file(entry(1)).unwrap();
        assert!(ledger.facts().is_err());
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
            lease: false,
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

    // ---- REV.1.0 R3 ----

    fn r3_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bsc-live-r3-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn operator(request_id: &str, at_ms: u64) -> Option<Operator> {
        Some(Operator {
            request_id: request_id.into(),
            decided_by: "operator".into(),
            evidence: None,
            at_ms,
        })
    }

    /// The operator's `kind` of the lot `of`, as `bsc_runtime live lot`
    /// writes it, at `at_ms`.
    fn decision(of: &Fact, kind: FactKind, request_id: &str, at_ms: u64) -> Fact {
        Fact {
            operator: operator(request_id, at_ms),
            ..Fact::about(of, kind)
        }
    }

    fn reverted(of: &Fact, block: u8, gas: u64) -> Fact {
        Fact {
            block: Some((u64::from(block), [block; 32], 1_000 * u64::from(block))),
            gas_wei: Some(U256::from(gas)),
            ..Fact::about(of, FactKind::Reverted)
        }
    }

    /// R3: a v4 fact keeps its attempt, its operator (with the entry's
    /// time) and its source; a v3 entry reads without them; an operator on
    /// a fact not the operator's, evidence outside an adoption (or an
    /// adoption without it) and a source on no chain fact refuse the
    /// ledger.
    #[test]
    fn a_v4_fact_keeps_its_attempt_operator_and_source_and_a_v3_entry_still_reads() {
        let dir = r3_dir("v4");
        let ledger = Ledger::open(&dir);
        let buy = Fact {
            attempt: Some([9; 32]),
            ..released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300)
        };
        let found = Fact {
            source: Some(FactSource::Receipt),
            ..included(&buy, 5, 100, 7, 1_000)
        };
        let resumed = decision(&buy, FactKind::Resumed, "req-1", 40);
        ledger.append(&buy, 10).unwrap();
        ledger.append(&found, 20).unwrap();
        ledger.append(&resumed, 40).unwrap();
        assert_eq!(ledger.facts().unwrap(), vec![buy.clone(), found, resumed]);
        let raw = |seq: u64,
                   schema: &'static darkbot_bsc_record::Schema,
                   kind: &str,
                   more: &dyn Fn(&mut Draft)| {
            let mut d = Draft::new(schema);
            d.set(k::KIND, kind)
                .set(k::TX_HASH, Value::B256([1; 32]))
                .set(k::PROVENANCE_TOOL, "test")
                .set(k::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
                .set(k::INTENT, "BUY")
                .set(k::COMPONENT, "BSC_EXEC")
                .set(k::TOKEN, Value::Address([0xA; 20]))
                .set(k::NONCE, 1u64)
                .set(k::GRANT_GENERATION, 7u64);
            more(&mut d);
            let path = dir.join("live-ledger").join(format!("{seq:020}.dbrec"));
            fs::write(&path, d.encode().unwrap()).unwrap();
            path
        };
        // A fresh handle each time: a handle keeps what it read (BSC3-046).
        let v3 = raw(3, &k::V3, "RELEASED", &|_| {});
        let facts = Ledger::open(&dir).facts().unwrap();
        assert_eq!(
            (facts[3].attempt, &facts[3].operator, facts[3].source),
            (None, &None, None)
        );
        fs::remove_file(v3).unwrap();
        type Case<'a> = (&'a str, &'a dyn Fn(&mut Draft), &'a str);
        let refused: [Case<'_>; 5] = [
            (
                "RELEASED",
                &|d| {
                    d.set(k::REQUEST_ID, "r").set(k::DECIDED_BY, "o");
                },
                "operator's fact",
            ),
            ("RESUMED", &|_| {}, "operator's fact"),
            (
                "RESUMED",
                &|d| {
                    d.set(k::REQUEST_ID, "r")
                        .set(k::DECIDED_BY, "o")
                        .set(k::EVIDENCE_SHA256, Value::B256([3; 32]));
                },
                "operator's fact",
            ),
            (
                "ADOPTED",
                &|d| {
                    d.set(k::REQUEST_ID, "r").set(k::DECIDED_BY, "o");
                },
                "operator's fact",
            ),
            (
                "RELEASED",
                &|d| {
                    d.set(k::SOURCE, "RECEIPT");
                },
                "chain fact",
            ),
        ];
        for (kind, more, why) in refused {
            let path = raw(3, &k::SCHEMA, kind, more);
            let e = Ledger::open(&dir).facts().unwrap_err();
            assert!(e.contains(why), "{kind}: {e}");
            fs::remove_file(path).unwrap();
        }
        // A request named without who decided it is no operator at all.
        let path = raw(3, &k::SCHEMA, "RESUMED", &|d| {
            d.set(k::REQUEST_ID, "r");
        });
        assert!(
            Ledger::open(&dir)
                .facts()
                .unwrap_err()
                .contains("whole or not at all")
        );
        fs::remove_file(path).unwrap();
        assert_eq!(Ledger::open(&dir).facts().unwrap().len(), 3);
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-068: an attempt is returned only when every BUY fact of it (its
    /// transaction's and its lease's) is WITHDRAWN: one released, unknown,
    /// or included later keeps its slots; an exit is no attempt; a v3 BUY
    /// without its attempt is its own (a leased one names no slot).
    #[test]
    fn an_attempt_whose_every_fact_is_withdrawn_is_returned_and_no_other() {
        let withdrawn = |n: u8, attempt: Option<[u8; 32]>, token: u8| Fact {
            kind: FactKind::Withdrawn,
            component: ComponentId::ExecC3,
            attempt,
            ..released(n, LiveIntentKind::Buy, token, 1_000, 1_300)
        };
        let (a, b, c) = ([0xA1; 32], [0xB1; 32], [0xC1; 32]);
        let late = withdrawn(7, Some(c), 0xC);
        let public = withdrawn(11, Some([0xE1; 32]), 0xF);
        let facts = vec![
            // A: the BUY and its lease's own fact, both withdrawn.
            withdrawn(1, Some(a), 0xA),
            withdrawn(2, Some(a), 0),
            // B: the BUY released, its lease's fact withdrawn.
            Fact {
                attempt: Some(b),
                ..released(3, LiveIntentKind::Buy, 0xB, 1_000, 1_300)
            },
            withdrawn(4, Some(b), 0),
            // A v3 BUY of its own, withdrawn.
            withdrawn(5, None, 0xD),
            // An admission recovered as UNKNOWN.
            Fact {
                kind: FactKind::Unknown,
                attempt: Some([6; 32]),
                ..released(6, LiveIntentKind::Buy, 0xE, 1_000, 1_300)
            },
            // C: withdrawn, then included (never: it would keep its slots).
            late.clone(),
            included(&late, 9, 100, 7, 1_000),
            // Re-check RC2: E withdrawn, included, its block orphaned: its
            // bytes were public, it keeps its slots.
            public.clone(),
            included(&public, 12, 100, 7, 1_000),
            Fact::about(&public, FactKind::Orphaned),
            // A withdrawn SELL.
            Fact {
                kind: FactKind::Withdrawn,
                ..released(8, LiveIntentKind::Sell, 0xA, 0, 300)
            },
            // Review F1: a lease's fact alone (its BUY's quarantined).
            withdrawn(10, Some([0xD1; 32]), 0),
        ];
        let returned = fold(&facts).withdrawn_attempts();
        assert_eq!(returned, vec![a, [5; 32]]);
    }

    /// R3 (`bsc_runtime live lot`): each decision of an open lot is written
    /// once per request (the same again changes nothing, another under its
    /// ID is refused); write-off and exit-now refuse while a step of the
    /// exit is out, exit-now while the operator exit is OFF; a lot not open
    /// and a request without a safe ID or a decider are refused.
    #[test]
    fn the_operator_decides_a_lot_once_per_request() {
        let dir = r3_dir("decide");
        let ledger = Ledger::open(&dir);
        let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        ledger.append(&buy, 1).unwrap();
        ledger.append(&included(&buy, 5, 100, 7, 1_000), 2).unwrap();
        let lot = [1; 32];
        let seq = ledger
            .decide(lot, LotDecision::Resume, "req-1", "operator", false, 10)
            .unwrap();
        assert_eq!(seq, Some(2));
        assert_eq!(
            ledger.decide(lot, LotDecision::Resume, "req-1", "operator", false, 11),
            Ok(None)
        );
        let reused = ledger
            .decide(lot, LotDecision::WriteOff, "req-1", "operator", false, 12)
            .unwrap_err();
        assert!(reused.starts_with(LIVE_REQUEST_REUSED), "{reused}");
        assert_eq!(
            ledger.decide(lot, LotDecision::ExitNow, "req-2", "operator", false, 13),
            Err(LIVE_OPERATOR_EXIT_OFF.to_string())
        );
        assert_eq!(
            ledger
                .decide(lot, LotDecision::ExitNow, "req-2", "operator", true, 14)
                .unwrap(),
            Some(3)
        );
        let facts = ledger.facts().unwrap();
        assert_eq!(facts[2].kind, FactKind::Resumed);
        assert_eq!(facts[2].operator, operator("req-1", 10));
        assert_eq!(facts[3].kind, FactKind::ExitRequested);
        assert_eq!((facts[3].tx_hash, facts[3].token), (lot, [0xA; 20]));
        // Its APPROVE is out (the lot still open): a write-off waits for it.
        ledger
            .append(&released(5, LiveIntentKind::Approve, 0xA, 0, 300), 15)
            .unwrap();
        let e = ledger
            .decide(lot, LotDecision::WriteOff, "req-8", "operator", false, 16)
            .unwrap_err();
        assert!(e.starts_with(LIVE_LOT_EXIT_OUT), "{e}");
        // Its SELL is out: write-off and exit-now wait for it, resume not.
        ledger
            .append(&released(2, LiveIntentKind::Sell, 0xA, 0, 300), 20)
            .unwrap();
        for (what, id) in [
            (LotDecision::WriteOff, "req-3"),
            (LotDecision::ExitNow, "req-4"),
        ] {
            let e = ledger
                .decide(lot, what, id, "operator", true, 21)
                .unwrap_err();
            assert!(e.starts_with(LIVE_LOT_EXIT_OUT), "{e}");
        }
        assert!(
            ledger
                .decide(lot, LotDecision::Resume, "req-5", "operator", false, 22)
                .unwrap()
                .is_some()
        );
        for not_open in [[9; 32], [2; 32]] {
            let e = ledger
                .decide(
                    not_open,
                    LotDecision::Resume,
                    "req-6",
                    "operator",
                    false,
                    23,
                )
                .unwrap_err();
            assert!(e.starts_with(LIVE_LOT_NOT_OPEN), "{e}");
        }
        assert!(
            ledger
                .decide(lot, LotDecision::Resume, "bad id!", "operator", false, 24)
                .is_err()
        );
        assert!(
            ledger
                .decide(lot, LotDecision::Resume, "req-7", " ", false, 24)
                .is_err()
        );
        assert_eq!(ledger.facts().unwrap().len(), 7);
        fs::remove_dir_all(dir).unwrap();
    }

    /// R3: a lot written off with nothing out is lost on the day of its
    /// fact (input and gas, no exit, no open lot); one whose SELL is out
    /// is decided by that SELL (a race with the decision): included, it
    /// closes with its proceeds.
    #[test]
    fn a_written_off_lot_is_lost_on_its_day_and_a_sell_out_decides_first() {
        let dir = r3_dir("writeoff");
        let ledger = Ledger::open(&dir);
        let a = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        let b = released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300);
        let sell = released(3, LiveIntentKind::Sell, 0xB, 0, 300);
        for (f, at) in [
            (a.clone(), 1),
            (included(&a, 5, 100, 7, 1_000), 2),
            (b.clone(), 3),
            (included(&b, 6, 100, 9, 1_000), 4),
            (sell.clone(), 5),
            (decision(&a, FactKind::WrittenOff, "off-a", 50_000), 50_000),
            (decision(&b, FactKind::WrittenOff, "off-b", 50_001), 50_001),
        ] {
            ledger.append(&f, at).unwrap();
        }
        let book = fold(&ledger.facts().unwrap());
        let lot_a = &book.positions[0];
        assert_eq!(lot_a.state, PositionState::WrittenOff);
        assert_eq!(
            (lot_a.pnl_wei, lot_a.closed_ms),
            (Some(-1_100), Some(50_000))
        );
        assert!(book.open_lot(&[0xA; 20]).is_none());
        assert_eq!(book.positions[1].state, PositionState::Exiting);
        assert_eq!(book.realized_pnl_wei, -1_100);
        ledger
            .append(&included(&sell, 8, 50, 9, 2_500), 60_000)
            .unwrap();
        let book = fold(&ledger.facts().unwrap());
        let lot_b = &book.positions[1];
        assert_eq!(lot_b.state, PositionState::Closed);
        assert_eq!(lot_b.pnl_wei, Some(2_500 - 1_000 - 150));
        fs::remove_dir_all(dir).unwrap();
    }

    /// R3: a RESUMED lot's exit steps reverted before it stop nothing; one
    /// reverted after it counts.
    #[test]
    fn a_resumed_lot_excuses_only_the_reverts_before_it() {
        let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        let approve = released(2, LiveIntentKind::Approve, 0xA, 0, 300);
        let sell = released(3, LiveIntentKind::Sell, 0xA, 0, 300);
        let again = released(4, LiveIntentKind::Sell, 0xA, 0, 300);
        let book = fold(&[
            buy.clone(),
            included(&buy, 5, 100, 7, 1_000),
            approve.clone(),
            reverted(&approve, 6, 50),
            sell.clone(),
            reverted(&sell, 7, 50),
            decision(&buy, FactKind::Resumed, "resume-1", 10),
            again.clone(),
            reverted(&again, 8, 50),
        ]);
        let decided = &book.tx(&[1; 32]).unwrap().decided;
        assert_eq!(decided.resumes, 1);
        assert_eq!(decided.excused, vec![[2; 32], [3; 32]]);
        assert_eq!(book.positions[0].state, PositionState::Open);
    }

    /// A G0.8 result (v7) of the buyer's G0.7 BUY of `token` and its chain
    /// capture: the BUY (nonce 3, value 1 000, gas limit 200 000 at 2 wei)
    /// and its launch in block 50, 20 confirmations, `tokens` received;
    /// `edit` changes the result last.
    fn g08_evidence(buyer: [u8; 20], tokens: u64, edit: &dyn Fn(&mut Draft)) -> (Vec<u8>, Vec<u8>) {
        use darkbot_bsc_journal::{CaptureRecord, JournalWriter, RecordKind};
        use darkbot_bsc_record::schema::g08::{
            chain_block as cb, chain_transaction as ct, result as g,
        };
        let filled = crate::live_qualification::tests::filled;
        let word = |n: u64| Value::U256(U256::from(n).to_be_bytes());
        let mut block = filled(&cb::SCHEMA);
        block
            .set(cb::NUMBER, 50u64)
            .set(cb::HASH, Value::B256([0x50; 32]))
            .set(cb::TIMESTAMP, 1_700u64)
            .set(cb::MILLI_TIMESTAMP, 1_700_250u64);
        let tx = |hash: [u8; 32], from: [u8; 20], nonce: u64| {
            let mut t = filled(&ct::SCHEMA);
            t.set(ct::BLOCK_NUMBER, 50u64)
                .set(ct::BLOCK_HASH, Value::B256([0x50; 32]))
                .set(ct::HASH, Value::B256(hash))
                .set(ct::FROM, Value::Address(from))
                .set(ct::NONCE, nonce)
                .set(ct::VALUE, word(1_000))
                .set(ct::GAS, 200_000u64)
                .set(ct::GAS_PRICE, word(2));
            t
        };
        let mut writer = JournalWriter::new(Vec::new());
        for (n, d) in [
            block,
            tx([0x1A; 32], [0xC1; 20], 0),
            tx([0xB0; 32], buyer, 3),
        ]
        .iter()
        .enumerate()
        {
            writer
                .append(&CaptureRecord {
                    sequence: n as u64,
                    kind: RecordKind::Canonical,
                    timestamp_ns: 1,
                    payload: d.encode().unwrap().into_boxed_slice(),
                })
                .unwrap();
        }
        let capture = writer.into_inner();
        let mut journal = filled(&g::chain_journal::SCHEMA);
        journal.set(
            g::chain_journal::SHA256,
            Value::B256(darkbot_bsc_record::sha256(&capture)),
        );
        let mut r = filled(&g::SCHEMA);
        r.set(g::BUY_STATUS, 1u64)
            .set(g::CANONICAL, true)
            .set(g::ACQUISITION_EVIDENCE_COMPLETE, true)
            .set(g::POSITIVE_ATTRIBUTABLE_ACQUISITION, true)
            .set(g::BUYER, Value::Address(buyer))
            .set(g::TOKEN, Value::Address([0xA; 20]))
            .set(g::CONFIRMATIONS_AFTER_LAUNCH_BLOCK, 20u64)
            .set(g::CURRENT_HEAD, 70u64)
            .set(g::BUY_SCOPE_OK, true)
            .set(g::BUY_RECEIPT_TOKEN_NET, word(tokens))
            .set(g::BUY_TX, Value::B256([0xB0; 32]))
            .set(g::LAUNCH_TX, Value::B256([0x1A; 32]))
            .set(g::BUY_BLOCK, 50u64)
            .set(g::BUY_BLOCK_HASH, Value::B256([0x50; 32]))
            .set(g::BUY_VALUE_WEI, word(1_000))
            .set(g::BUY_GAS_COST_WEI, word(150_000))
            .set(g::CHAIN_JOURNAL, journal.into_nested());
        edit(&mut r);
        (r.encode().unwrap(), capture)
    }

    /// R3 (`bsc_runtime live adopt-g07`): the buyer's G0.7 BUY of a G0.8
    /// result, its capture matching, becomes an included lot of no grant
    /// (its tokens, input, gas, the launch's creator, the result's hash as
    /// evidence), once per request; another buyer, a launch not final, a
    /// capture not the result's, no tokens, a failed BUY or a value the
    /// capture does not show are refused.
    #[test]
    fn a_g07_buy_is_adopted_from_its_g08_result_and_capture_once() {
        let dir = r3_dir("adopt");
        let ledger = Ledger::open(&dir);
        let buyer = [0xBB; 20];
        let (result, capture) = g08_evidence(buyer, 7, &|_| {});
        let buy = g07_buy(&result, &capture, buyer, 15).unwrap();
        assert_eq!(
            buy,
            G07Buy {
                tx_hash: [0xB0; 32],
                token: [0xA; 20],
                buyer,
                launcher: [0xC1; 20],
                nonce: 3,
                value_wei: U256::from(1_000u64),
                max_wei: U256::from(1_000u64 + 400_000),
                gas_wei: U256::from(150_000u64),
                tokens: U256::from(7u64),
                block: (50, [0x50; 32], 1_700_250),
                evidence: darkbot_bsc_record::sha256(&result),
            }
        );
        assert_eq!(
            ledger.adopt(&buy, 56, "adopt-1", "operator", 100),
            Ok(Some(0))
        );
        assert_eq!(ledger.adopt(&buy, 56, "adopt-1", "operator", 101), Ok(None));
        let again = ledger
            .adopt(&buy, 56, "adopt-2", "operator", 102)
            .unwrap_err();
        assert!(again.contains("in the ledger already"), "{again}");
        // Another BUY of a token with an open lot would hide that lot.
        let other = G07Buy {
            tx_hash: [0xB1; 32],
            ..buy.clone()
        };
        let hidden = ledger
            .adopt(&other, 56, "adopt-3", "operator", 103)
            .unwrap_err();
        assert!(hidden.contains("open lot already"), "{hidden}");
        let book = fold(&ledger.facts().unwrap());
        let p = &book.positions[0];
        assert_eq!(
            (
                p.state,
                p.tokens,
                p.input_wei,
                p.gas_wei,
                p.grant_generation,
                p.component
            ),
            (
                PositionState::Open,
                U256::from(7u64),
                U256::from(1_000u64),
                U256::from(150_000u64),
                0,
                ComponentId::ExecC3
            )
        );
        assert_eq!(book.tx(&[0xB0; 32]).unwrap().launcher, Some([0xC1; 20]));
        assert!(book.unresolved().next().is_none());
        // Its block left the chain (never beyond finality): not known again.
        let mut orphaned = ledger.facts().unwrap();
        orphaned.push(Fact::about(&orphaned[0], FactKind::Orphaned));
        let book = fold(&orphaned);
        assert_eq!(book.tx(&[0xB0; 32]).unwrap().state, FactKind::Unknown);
        assert_eq!(book.unresolved().count(), 1);
        let refused = |result: &[u8], capture: &[u8], buyer, finality| {
            g07_buy(result, capture, buyer, finality).unwrap_err()
        };
        assert!(refused(&result, &capture, [0xCC; 20], 15).contains("buyer"));
        assert!(refused(&result, &capture, buyer, 21).contains("launch's block is not final"));
        let mut torn = capture.clone();
        torn.push(0);
        assert!(refused(&result, &torn, buyer, 15).contains("not the result's"));
        use darkbot_bsc_record::schema::g08::result as g;
        let word = |n: u64| Value::U256(U256::from(n).to_be_bytes());
        type Case<'a> = (&'a dyn Fn(&mut Draft), &'a str);
        let cases: [Case<'_>; 7] = [
            (
                &|d: &mut Draft| {
                    d.set(g::CURRENT_HEAD, 60u64);
                },
                "BUY's block is not final",
            ),
            (
                &|d: &mut Draft| {
                    d.set(g::BUY_SCOPE_OK, false);
                },
                "out of scope",
            ),
            (&|_: &mut Draft| {}, "no tokens"),
            (
                &|d: &mut Draft| {
                    d.set(g::BUY_STATUS, 0u64);
                },
                "did not succeed",
            ),
            (
                &|d: &mut Draft| {
                    d.set(g::BUY_VALUE_WEI, word(999));
                },
                "value",
            ),
            (
                &|d: &mut Draft| {
                    d.set(g::POSITIVE_ATTRIBUTABLE_ACQUISITION, false);
                },
                "attributable",
            ),
            (
                &|d: &mut Draft| {
                    d.set(g::CANONICAL, false);
                },
                "canonical",
            ),
        ];
        for (edit, why) in cases {
            let tokens = if why == "no tokens" { 0 } else { 7 };
            let (r, c) = g08_evidence(buyer, tokens, edit);
            let e = refused(&r, &c, buyer, 15);
            assert!(
                e.contains(why) && e.starts_with(LIVE_ADOPTION_REFUSED),
                "{why}: {e}"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-045: a start clears the temporary file a crashed append left,
    /// never one a living writer holds; appends go on.
    #[test]
    fn a_crashed_appends_temporary_file_is_cleared_and_a_living_writers_kept() {
        let dir = r3_dir("pending");
        let ledger = Ledger::open(&dir);
        ledger
            .append(&released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300), 1)
            .unwrap();
        let live = dir.join("live-ledger");
        let dead = live.join("pending-1-0.tmp");
        let held = live.join("pending-2-0.tmp");
        fs::write(&dead, b"cut").unwrap();
        fs::write(&held, b"half").unwrap();
        let writer = fs::File::open(&held).unwrap();
        writer.lock().unwrap();
        assert_eq!(ledger.clear_pending(), Ok(1));
        assert!(!dead.exists() && held.exists());
        drop(writer);
        assert_eq!(ledger.clear_pending(), Ok(1));
        assert!(!held.exists());
        ledger
            .append(&released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300), 2)
            .unwrap();
        assert_eq!(ledger.facts().unwrap().len(), 2);
        let names: Vec<String> = fs::read_dir(&live)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(names.is_empty(), "{names:?}");
        fs::remove_dir_all(dir).unwrap();
    }

    /// BSC3-045 (`bsc_runtime live ledger check|quarantine`): an entry that
    /// does not decode is named by the check and moved out by the
    /// operator, once per request (its record first, the entry whole beside
    /// it); the ledger then reads; a readable fact is never quarantined; the
    /// quarantine's generation is reported (an unreadable record: every
    /// one).
    #[test]
    fn an_unreadable_entry_is_quarantined_once_and_the_ledger_reads_again() {
        let dir = r3_dir("quarantine");
        let ledger = Ledger::open(&dir);
        let buy = released(1, LiveIntentKind::Buy, 0xA, 1_000, 1_300);
        ledger.append(&buy, 1).unwrap();
        let bad = dir.join("live-ledger").join(format!("{:020}.dbrec", 1));
        fs::write(&bad, b"torn entry").unwrap();
        assert!(ledger.facts().is_err());
        let (entries, unreadable) = ledger.check().unwrap();
        assert_eq!((entries, unreadable.len(), unreadable[0].0), (2, 1, 1));
        assert_eq!(ledger.quarantined(), Ok(None));
        let e = ledger.quarantine(0, "q-0", "operator", 7, 100).unwrap_err();
        assert!(e.contains("never quarantined"), "{e}");
        assert!(ledger.quarantine(5, "q-5", "operator", 7, 100).is_err());
        ledger.quarantine(1, "q-1", "operator", 7, 100).unwrap();
        assert_eq!(ledger.facts().unwrap(), vec![buy]);
        assert_eq!(ledger.check().unwrap(), (1, Vec::new()));
        assert_eq!(ledger.quarantined(), Ok(Some(7)));
        let qdir = dir.join("live-ledger-quarantine");
        assert_eq!(
            fs::read(qdir.join(format!("{:020}.entry", 1))).unwrap(),
            b"torn entry"
        );
        ledger.quarantine(1, "q-1", "operator", 7, 101).unwrap();
        // A crash after the record, before the move: the rerun moves it.
        let entry = qdir.join(format!("{:020}.entry", 1));
        fs::rename(&entry, &bad).unwrap();
        ledger.quarantine(1, "q-1", "operator", 7, 101).unwrap();
        assert!(entry.exists() && !bad.exists());
        let reused = ledger.quarantine(1, "q-2", "operator", 7, 102).unwrap_err();
        assert!(reused.starts_with(LIVE_REQUEST_REUSED), "{reused}");
        // The next append never takes the quarantined number.
        assert_eq!(
            ledger.append(&released(2, LiveIntentKind::Buy, 0xB, 1_000, 1_300), 3),
            Ok(2)
        );
        // Run again, the request moves only the bytes it recorded: an entry
        // of another writer at that number stays, the moved one is kept.
        let other = fs::read(dir.join("live-ledger").join(format!("{:020}.dbrec", 2))).unwrap();
        fs::write(&bad, &other).unwrap();
        ledger.quarantine(1, "q-1", "operator", 7, 103).unwrap();
        assert_eq!(fs::read(&bad).unwrap(), other);
        assert_eq!(
            fs::read(qdir.join(format!("{:020}.entry", 1))).unwrap(),
            b"torn entry"
        );
        fs::remove_file(&bad).unwrap();
        fs::write(qdir.join(format!("{:020}.dbrec", 9)), b"x").unwrap();
        assert_eq!(ledger.quarantined(), Ok(Some(u64::MAX)));
        fs::remove_dir_all(dir).unwrap();
    }
}
