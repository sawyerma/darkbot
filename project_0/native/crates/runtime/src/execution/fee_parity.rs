//! P6-MT.10 / S4: the parity of the local fee model (`tx::local_fee_lamports`) with the node's
//! fee of every message an owner's evaluations ask one for, per class of message — its format,
//! whether it pays a priority fee, its part of a bundle. The local model may replace
//! `getFeeForMessage` only after 100 % parity over at least `PARITY_MIN_MESSAGES` qualified
//! messages of a class (PHASENPLAN P6-MT.10: "Lokales Fee-Modell ersetzt `getFeeForMessage` erst
//! nach 100 % Parität über >=10.000 qualifizierte v0/v1-Messages mit/ohne Priority und
//! Bundle-Bestandteile. Mismatch → RPC-Fallback + Gate-FAIL"); one mismatch fails its class for
//! good. A qualified message is a distinct one: the same message quoted again (a candidate
//! re-evaluated under the same blockhash) is compared again but counts once. Counted in the
//! owner's own tally (`FeeParity`, lent to each evaluation), journaled by the owner
//! (`FEE_PARITY_STATS`: at its start, when the counts changed, at shutdown, and synced at once
//! after a new mismatch — so a mismatch survives a crash once one owner pass and one sync
//! passed), summed across the saved runs of a project by `saved_runs`/`qualification` (an
//! on-demand report, never an owner's start). A crashed or still running run counts with its last
//! record and is named: its matches are a lower bound; a mismatch it journaled is durable, but one
//! quoted in the owner pass it crashed in (bounded by that pass's candidate network deadlines, the
//! evidence queued before it and one sync) can be lost. A run whose journal cannot be read (an
//! error, a cut chain, an undecodable record, a recovery journal from a failed write) qualifies
//! nothing until it is resolved. Every owner runs on the Mainnet cluster only (BOOT-12 genesis
//! pin), so every record is Mainnet's. No v1 transaction is built yet (S3-02): v1 has no class
//! until it is.
//!
//! Phase 2 (`FeeModel`, the controller capability `LOCAL_FEE_MODEL`): `write_seal` seals the saved
//! runs' parity (`p0ctl fee-parity --seal`); ACTIVE decides a single transaction's fee in a class
//! the newest seal qualifies and no mismatch of the run failed (`decide`), and the simulation that
//! precedes every send verifies it with the node's fee of that simulation (`FeeParity::verify`,
//! counted like a quote): a mismatch stops the candidate unsent and fails the class (the RPC quote
//! again), a simulation without the node's fee stops it and ends ACTIVE for the run. A bundle's
//! parts are always quoted: a bundle simulation names no part's fee.
use serde::{Deserialize, Serialize};
use solana_sdk::hash::Hash;
use solana_sdk::message::VersionedMessage;
use solana_sdk::transaction::VersionedTransaction;
use std::sync::atomic::{AtomicU64, Ordering};

/// The local fee model's rules (`tx::local_fee_lamports`): a change starts a new tally (pinned by
/// `the_model_version_pins_its_outputs`).
pub const FEE_MODEL_VERSION: u32 = 1;

/// PHASENPLAN P6-MT.10: the distinct qualified messages of a class the local model needs, all
/// matched.
pub const PARITY_MIN_MESSAGES: u64 = 10_000;

/// A message's part: a single transaction, or a bundle's prerequisite, receivership or tip TX.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FeeRole {
    Single,
    BundlePrerequisite,
    BundleReceivership,
    BundleTip,
}

/// The message format (v1: none is built yet, S3-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FeeFormat {
    Legacy,
    V0,
}

/// One class of messages the parity is counted for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FeeClass {
    pub format: FeeFormat,
    /// It pays a priority fee in the model (or is outside it).
    pub priority: bool,
    pub role: FeeRole,
}

const FORMATS: [FeeFormat; 2] = [FeeFormat::Legacy, FeeFormat::V0];
const ROLES: [FeeRole; 4] = [
    FeeRole::Single,
    FeeRole::BundlePrerequisite,
    FeeRole::BundleReceivership,
    FeeRole::BundleTip,
];
const CLASS_COUNT: usize = FORMATS.len() * 2 * ROLES.len();

impl FeeClass {
    /// Every class, in a fixed order.
    pub fn all() -> impl Iterator<Item = FeeClass> {
        (0..CLASS_COUNT).map(Self::at)
    }

    fn at(i: usize) -> FeeClass {
        FeeClass {
            format: FORMATS[i / (2 * ROLES.len())],
            priority: (i / ROLES.len()) % 2 == 1,
            role: ROLES[i % ROLES.len()],
        }
    }

    fn index(self) -> usize {
        let format = FORMATS.iter().position(|f| *f == self.format).unwrap_or(0);
        let role = ROLES.iter().position(|r| *r == self.role).unwrap_or(0);
        (format * 2 + usize::from(self.priority)) * ROLES.len() + role
    }

    /// The class of `tx` in `role` with its local fee `local`: it pays a priority fee when its
    /// local fee is above its signatures' (outside the model counts as paying one).
    fn of(tx: &VersionedTransaction, role: FeeRole, local: Option<u64>) -> FeeClass {
        let signatures = u64::from(tx.message.header().num_required_signatures)
            .saturating_mul(darkbot_solana::tx::LAMPORTS_PER_SIGNATURE);
        FeeClass {
            format: match tx.message {
                VersionedMessage::Legacy(_) => FeeFormat::Legacy,
                VersionedMessage::V0(_) => FeeFormat::V0,
            },
            priority: local.is_none_or(|fee| fee > signatures),
            role,
        }
    }
}

/// The counts of one class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityCounts {
    /// Distinct messages the local model gave the RPC quote's lamports for.
    pub matched: u64,
    /// A message matched again (the same message quoted again): never a qualified message.
    pub repeated: u64,
    /// The local model gave other lamports (the class fails), a repeated message too.
    pub mismatched: u64,
    /// The message is outside the local model (`None`): counted, never qualified.
    pub unsupported: u64,
}

impl ParityCounts {
    fn add(self, o: ParityCounts) -> ParityCounts {
        ParityCounts {
            matched: self.matched.saturating_add(o.matched),
            repeated: self.repeated.saturating_add(o.repeated),
            mismatched: self.mismatched.saturating_add(o.mismatched),
            unsupported: self.unsupported.saturating_add(o.unsupported),
        }
    }
}

/// The first mismatch of a run: what the RPC charged and what the local model said.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeMismatch {
    pub class: FeeClass,
    pub message_hash: [u8; 32],
    pub rpc_lamports: u64,
    pub local_lamports: u64,
}

/// P6-MT.10 (kind `FEE_PARITY_STATS`): the cumulative parity counts of the owner's run per
/// class (every class, zero counts too) under the model version, and its first mismatch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeParityStatsRecord {
    pub model_version: u32,
    pub classes: Vec<(FeeClass, ParityCounts)>,
    pub first_mismatch: Option<FeeMismatch>,
}

impl crate::analytics::payload::Payload for FeeParityStatsRecord {
    const KIND: u16 = crate::analytics::journal::kind::FEE_PARITY_STATS;
}

/// The messages compared per blockhash and when that blockhash was last seen.
type Seen = std::collections::HashMap<Hash, (std::time::Instant, std::collections::HashSet<Hash>)>;

/// One owner's tally: every node fee its evaluations compared with the local model, per class —
/// held by the owner and lent to each evaluation, so nothing else in the process counts into it.
/// The messages compared are kept per blockhash and dropped once the blockhash is older than
/// `blockhash_max_age_ms` (no fresh message names it any more), so evaluations around a refresh,
/// under the old and the new blockhash, never count a message twice.
#[derive(Debug, Default)]
pub struct FeeParity {
    /// Per class: matched, repeated, mismatched, unsupported.
    counts: [[AtomicU64; 4]; CLASS_COUNT],
    first_mismatch: std::sync::Mutex<Option<FeeMismatch>>,
    seen: std::sync::Mutex<Seen>,
}

impl FeeParity {
    /// Whether `tx` is compared for the first time (a poisoned record of what was seen: never
    /// first).
    fn first_seen(&self, tx: &VersionedTransaction) -> bool {
        let Ok(mut seen) = self.seen.lock() else {
            return false;
        };
        let now = std::time::Instant::now();
        let max_age = std::time::Duration::from_millis(
            darkbot_solana::runtime_config::active()
                .limits
                .blockhash_max_age_ms,
        );
        seen.retain(|_, (at, _)| now.saturating_duration_since(*at) <= max_age);
        let (at, hashes) = seen
            .entry(*tx.message.recent_blockhash())
            .or_insert_with(|| (now, Default::default()));
        *at = now;
        hashes.insert(tx.message.hash())
    }

    fn counts(&self) -> [ParityCounts; CLASS_COUNT] {
        let mut out = [ParityCounts::default(); CLASS_COUNT];
        for (o, c) in out.iter_mut().zip(self.counts.iter()) {
            *o = ParityCounts {
                matched: c[0].load(Ordering::Relaxed),
                repeated: c[1].load(Ordering::Relaxed),
                mismatched: c[2].load(Ordering::Relaxed),
                unsupported: c[3].load(Ordering::Relaxed),
            };
        }
        out
    }

    /// Every mismatch counted (an owner journals at once when it grows).
    pub fn mismatched(&self) -> u64 {
        self.counts
            .iter()
            .map(|c| c[2].load(Ordering::Relaxed))
            .sum()
    }

    /// Counts the node's fee `node_lamports` of `tx` in `role` (an RPC quote, or the simulation
    /// of a fee the model decided) against the local model (and the MT0 totals).
    pub fn observe(&self, tx: &VersionedTransaction, role: FeeRole, node_lamports: u64) {
        use darkbot_common::mt0::{Counter, MT0};
        let local = darkbot_solana::tx::local_fee_lamports(tx);
        let class = FeeClass::of(tx, role, local);
        let counts = &self.counts[class.index()];
        match local {
            Some(local) if local == node_lamports => {
                counts[if self.first_seen(tx) { 0 } else { 1 }].fetch_add(1, Ordering::Relaxed);
                MT0.add(Counter::FeeParityMatch, 1);
            }
            Some(local) => {
                counts[2].fetch_add(1, Ordering::Relaxed);
                MT0.add(Counter::FeeParityMismatch, 1);
                if let Ok(mut first) = self.first_mismatch.lock() {
                    first.get_or_insert(FeeMismatch {
                        class,
                        message_hash: tx.message.hash().to_bytes(),
                        rpc_lamports: node_lamports,
                        local_lamports: local,
                    });
                }
            }
            None => {
                counts[3].fetch_add(1, Ordering::Relaxed);
                MT0.add(Counter::FeeParityUnsupported, 1);
            }
        }
    }

    /// The run's record: `base` (what a resumed run journaled before, of this model version) and
    /// what this owner counted.
    pub fn record(&self, base: Option<&FeeParityStatsRecord>) -> FeeParityStatsRecord {
        let base = base.filter(|b| b.model_version == FEE_MODEL_VERSION);
        let now = self.counts();
        let classes = FeeClass::all()
            .map(|class| {
                let earlier = base
                    .and_then(|b| b.classes.iter().find(|(c, _)| *c == class))
                    .map(|(_, n)| *n)
                    .unwrap_or_default();
                (class, earlier.add(now[class.index()]))
            })
            .collect();
        let first_mismatch = base
            .and_then(|b| b.first_mismatch.clone())
            .or_else(|| self.first_mismatch.lock().ok().and_then(|f| f.clone()));
        FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION,
            classes,
            first_mismatch,
        }
    }

    /// The classes a mismatch failed in this run (`base` included): failed for good under this
    /// model version. Asked per evaluation: it allocates nothing.
    pub fn failed(&self, base: Option<&FeeParityStatsRecord>) -> ClassSet {
        let base = base.filter(|b| b.model_version == FEE_MODEL_VERSION);
        FeeClass::all()
            .filter(|c| {
                self.counts[c.index()][2].load(Ordering::Relaxed) > 0
                    || base.is_some_and(|b| {
                        b.classes.iter().any(|(bc, n)| bc == c && n.mismatched > 0)
                    })
            })
            .fold(ClassSet::default(), ClassSet::with)
    }

    /// Verifies the fee `decided` for the simulated `tx` with the node's fee of that simulation
    /// (`node`), counted as a comparison of its class; `Err`: the fee cannot stand — the
    /// candidate stops before anything is sent.
    pub fn verify(
        &self,
        tx: &VersionedTransaction,
        decided: &LocalFee,
        node: Option<u64>,
    ) -> Result<(), Unverified> {
        if tx.message.hash() != decided.message_hash {
            return Err(Unverified::OtherMessage);
        }
        let node = node.ok_or(Unverified::NoNodeFee)?;
        self.observe(tx, FeeRole::Single, node);
        if node == decided.lamports {
            Ok(())
        } else {
            Err(Unverified::Mismatch {
                node,
                local: decided.lamports,
            })
        }
    }
}

/// A set of classes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClassSet(u16);

const _: () = assert!(CLASS_COUNT <= u16::BITS as usize);

impl ClassSet {
    pub fn with(self, class: FeeClass) -> ClassSet {
        ClassSet(self.0 | 1 << class.index())
    }

    pub fn contains(self, class: FeeClass) -> bool {
        self.0 & 1 << class.index() != 0
    }

    pub fn without(self, other: ClassSet) -> ClassSet {
        ClassSet(self.0 & !other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }
}

/// P8-CTRL: the local fee model is a capability of the one controller.
pub const LOCAL_FEE_MODEL: &str = "LOCAL_FEE_MODEL";
/// OFF: not computed; OBSERVE: compared with every RPC quote, the quote decides; ACTIVE: the model
/// decides a single transaction's fee in a class its seal qualifies (`FeeModel`).
pub const MODE_VALUES: [&str; 3] = ["OFF", "OBSERVE", "ACTIVE"];

/// The requested mode of the local fee model (ordered OFF < OBSERVE < ACTIVE).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeeModelMode {
    Off,
    /// Phase 1's behaviour: the RPC quote decides.
    #[default]
    Observe,
    Active,
}

impl FeeModelMode {
    /// A controller value (`MODE_VALUES`).
    pub fn parse(value: &str) -> Option<FeeModelMode> {
        match value {
            "OFF" => Some(FeeModelMode::Off),
            "OBSERVE" => Some(FeeModelMode::Observe),
            "ACTIVE" => Some(FeeModelMode::Active),
            _ => None,
        }
    }

    /// The start value `P0_LOCAL_FEE_MODEL`: off|observe|active (empty: observe).
    pub fn from_env(text: &str) -> Result<FeeModelMode, String> {
        match text.trim() {
            "" | "observe" => Ok(FeeModelMode::Observe),
            "off" => Ok(FeeModelMode::Off),
            "active" => Ok(FeeModelMode::Active),
            other => Err(format!(
                "LOCAL_FEE_MODEL_INVALID: {other} (off|observe|active)"
            )),
        }
    }

    pub fn name(self) -> &'static str {
        MODE_VALUES[self as usize]
    }
}

/// What one evaluation does with the local model: compare it with every RPC quote (`observe`)
/// and decide a single transaction's fee, without an RPC quote, in the classes it is active in.
/// A bundle's parts are never decided: a bundle simulation names no fee of a part, so nothing
/// would verify one before the send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeModel {
    pub observe: bool,
    active: ClassSet,
}

impl Default for FeeModel {
    fn default() -> Self {
        FeeModel::OBSERVE
    }
}

impl FeeModel {
    pub const OFF: FeeModel = FeeModel {
        observe: false,
        active: ClassSet(0),
    };
    pub const OBSERVE: FeeModel = FeeModel {
        observe: true,
        active: ClassSet(0),
    };

    /// ACTIVE in the single-transaction classes of `classes`.
    pub fn active(classes: ClassSet) -> FeeModel {
        let single = FeeClass::all()
            .filter(|c| c.role == FeeRole::Single)
            .fold(ClassSet::default(), ClassSet::with);
        FeeModel {
            observe: true,
            active: ClassSet(classes.0 & single.0),
        }
    }

    /// The classes it decides in.
    pub fn classes(&self) -> ClassSet {
        self.active
    }
}

/// P6-MT.10: a fee the local model decided (ACTIVE) for exactly one message in place of its RPC
/// quote; its simulation verifies it (`FeeParity::verify`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFee {
    #[serde(with = "darkbot_solana::submit::message_hash_serde")]
    pub message_hash: Hash,
    pub lamports: u64,
    pub model_version: u32,
}

/// The local model's fee of `tx` in `role` where `model` decides its class; `None`: the RPC quote
/// decides.
pub fn decide(tx: &VersionedTransaction, role: FeeRole, model: &FeeModel) -> Option<LocalFee> {
    let lamports = darkbot_solana::tx::local_fee_lamports(tx)?;
    model
        .active
        .contains(FeeClass::of(tx, role, Some(lamports)))
        .then(|| LocalFee {
            message_hash: tx.message.hash(),
            lamports,
            model_version: FEE_MODEL_VERSION,
        })
}

/// Why a decided fee cannot stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unverified {
    /// The node charged another fee for the message: the class fails for good.
    Mismatch { node: u64, local: u64 },
    /// The simulation named no fee: nothing verifies the model with this provider.
    NoNodeFee,
    /// The simulated message is not the one decided for.
    OtherMessage,
}

/// How a run's owner journal read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunRead {
    /// Sealed, complete, nothing torn.
    Clean,
    /// Its last segment unsealed or torn, or continued after a crash: it crashed (or still runs).
    Crashed,
    /// An unreadable frame or record, a cut chain, or a journal that does not open.
    Unreadable,
}

/// The last `FEE_PARITY_STATS` record of the owner journal (chain) `journal` and how it read.
pub fn last_record(journal: &std::path::Path) -> (Option<FeeParityStatsRecord>, RunRead) {
    use crate::analytics::payload::Payload;
    let Ok(mut reader) = crate::storage::segments::ChainReader::open(journal) else {
        return (None, RunRead::Unreadable);
    };
    let mut last = None;
    loop {
        match reader.next_frame() {
            Ok(Some(frame)) if frame.kind == FeeParityStatsRecord::KIND => {
                match frame.typed::<FeeParityStatsRecord>() {
                    Ok(r) => last = Some(r),
                    Err(_) => return (last, RunRead::Unreadable),
                }
            }
            Ok(Some(_)) => {}
            Ok(None) if reader.successor_missing() => return (last, RunRead::Unreadable),
            Ok(None) if reader.sealed && reader.complete && reader.torn_bytes == 0 => {
                return (last, RunRead::Clean)
            }
            Ok(None) => return (last, RunRead::Crashed),
            Err(_) => return (last, RunRead::Unreadable),
        }
    }
}

/// The saved runs' parity: each run's last record of the current model version, the runs (by id)
/// that crashed or still run (counted with their last record) and those that cannot be read
/// (nothing qualifies while one is).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedRuns {
    /// Every saved run read (with a record or not).
    pub runs: Vec<String>,
    pub records: Vec<FeeParityStatsRecord>,
    pub crashed: Vec<String>,
    pub unreadable: Vec<String>,
}

/// The saved runs of `project`: every run's owner journal (`until_stopped_view::SHADOW_RECORD` in
/// the run or its capture directory) read to its end — an on-demand report (`p0ctl
/// fee-parity`), never on an owner's start. A recovery journal beside it (a primary write
/// failed: events went there, unread here) makes the run unreadable.
pub fn saved_runs(project: &std::path::Path) -> SavedRuns {
    let record = crate::orchestration::until_stopped_view::SHADOW_RECORD;
    let stem = record.trim_end_matches(".p0j");
    let mut out = SavedRuns::default();
    for run in crate::run_layout::runs(project) {
        out.runs.push(run.run_id.clone());
        let Some(journal) = [run.path.join(record), run.path.join("capture").join(record)]
            .into_iter()
            .find(|p| p.is_file())
        else {
            continue;
        };
        let recovered = journal
            .parent()
            .and_then(|d| std::fs::read_dir(d).ok())
            .is_some_and(|d| {
                d.flatten().any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(&format!("{stem}.recovery-"))
                })
            });
        let (last, read) = last_record(&journal);
        if read == RunRead::Unreadable || recovered {
            out.unreadable.push(run.run_id.clone());
            continue;
        }
        let Some(last) = last.filter(|r| r.model_version == FEE_MODEL_VERSION) else {
            continue;
        };
        if read == RunRead::Crashed {
            out.crashed.push(run.run_id.clone());
        }
        out.records.push(last);
    }
    out
}

/// The parity of one class over its records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassParity {
    pub class: FeeClass,
    pub counts: ParityCounts,
    /// Saved runs that cannot be read: nothing qualifies while one is.
    pub unreadable_runs: usize,
}

impl ClassParity {
    /// At least `PARITY_MIN_MESSAGES` distinct matched, none mismatched, every run readable.
    pub fn qualified(&self) -> bool {
        self.counts.matched >= PARITY_MIN_MESSAGES
            && self.counts.mismatched == 0
            && self.unreadable_runs == 0
    }

    /// A mismatch failed the class (under this model version, for good).
    pub fn failed(&self) -> bool {
        self.counts.mismatched > 0
    }
}

/// P6-MT.10: the parity per class summed over `runs` (each run's last record) of the current
/// model version — a record of another version counts nothing.
pub fn qualification(runs: &SavedRuns) -> Vec<ClassParity> {
    FeeClass::all()
        .map(|class| ClassParity {
            class,
            counts: runs
                .records
                .iter()
                .filter(|r| r.model_version == FEE_MODEL_VERSION)
                .flat_map(|r| r.classes.iter().filter(|(c, _)| *c == class))
                .fold(ParityCounts::default(), |sum, (_, n)| sum.add(*n)),
            unreadable_runs: runs.unreadable.len(),
        })
        .collect()
}

/// The parity of the local fee model over `classes` (`scope`: what they sum): how many are
/// qualified, the largest matched count and every mismatch; FAIL once a class failed.
pub fn status_line(classes: &[ClassParity], scope: &str) -> String {
    let qualified = classes.iter().filter(|c| c.qualified()).count();
    let failed = classes.iter().filter(|c| c.failed()).count();
    let matched = classes.iter().map(|c| c.counts.matched).max().unwrap_or(0);
    let mismatched: u64 = classes.iter().map(|c| c.counts.mismatched).sum();
    let unsupported: u64 = classes.iter().map(|c| c.counts.unsupported).sum();
    format!(
        "{}: {scope}: {qualified}/{} classes qualified (≥{PARITY_MIN_MESSAGES} distinct matched, none mismatched), {failed} failed · most matched {matched} · mismatched {mismatched} · outside the model {unsupported} (v1: no v1 transaction is built)",
        if failed > 0 {
            "LOCAL_FEE_MODEL_FAIL"
        } else {
            "LOCAL_FEE_MODEL_OBSERVE"
        },
        classes.len()
    )
}

/// P6-MT.10 (kind `FEE_PARITY_SEAL`): the saved runs' parity at one instant, sealed by `p0ctl
/// fee-parity --seal` — what an owner's ACTIVE rests on, read at its start and on each request
/// (never the journals themselves).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeParitySeal {
    pub model_version: u32,
    /// Every saved run at the seal.
    pub runs: Vec<String>,
    pub crashed: Vec<String>,
    pub unreadable: Vec<String>,
    pub classes: Vec<(FeeClass, ParityCounts)>,
}

impl crate::analytics::payload::Payload for FeeParitySeal {
    const KIND: u16 = crate::analytics::journal::kind::FEE_PARITY_SEAL;
}

/// The seals of a project: `seal_<n>.p0j`, each sealed and immutable.
pub fn seal_dir(project: &std::path::Path) -> std::path::PathBuf {
    project.join("data").join("fee-parity")
}

/// Seals the saved runs' parity of `project` now as its next seal; its number and the seal.
pub fn write_seal(project: &std::path::Path) -> Result<(u64, FeeParitySeal), String> {
    let runs = saved_runs(project);
    let seal = FeeParitySeal {
        model_version: FEE_MODEL_VERSION,
        classes: qualification(&runs)
            .into_iter()
            .map(|c| (c.class, c.counts))
            .collect(),
        runs: runs.runs,
        crashed: runs.crashed,
        unreadable: runs.unreadable,
    };
    let dir = seal_dir(project);
    std::fs::create_dir_all(&dir).map_err(|e| format!("LOCAL_FEE_SEAL_IO: {e}"))?;
    let next = crate::orchestration::source_control::numbered(&dir, "seal_")
        .map_err(|e| e.replace("SOURCE_CONTROL_IO", "LOCAL_FEE_SEAL_IO"))?
        .last()
        .map_or(1, |n| n + 1);
    crate::analytics::journal::create_typed_value(
        &dir.join(format!("seal_{next:08}.p0j")),
        crate::analytics::journal::source::MEASUREMENT,
        &seal,
    )
    .map_err(|e| format!("LOCAL_FEE_SEAL_IO: {e}"))?;
    Ok((next, seal))
}

/// The newest seal of `project` (`None`: never sealed).
pub fn newest_seal(project: &std::path::Path) -> Result<Option<FeeParitySeal>, String> {
    let dir = seal_dir(project);
    let Some(n) = crate::orchestration::source_control::numbered(&dir, "seal_")
        .map_err(|e| e.replace("SOURCE_CONTROL_IO", "LOCAL_FEE_SEAL_IO"))?
        .last()
        .copied()
    else {
        return Ok(None);
    };
    crate::analytics::journal::read_typed_value(&dir.join(format!("seal_{n:08}.p0j")))
        .map(Some)
        .map_err(|e| format!("LOCAL_FEE_SEAL_INVALID: seal {n}: {e}"))
}

/// ACTIVE without a seal: the RPC quote decides.
pub const NOT_SEALED: &str =
    "LOCAL_FEE_NOT_SEALED: no seal of the saved runs (p0ctl fee-parity --seal)";

/// The classes ACTIVE may decide in by the newest seal of `project`: its qualified classes
/// (`ClassParity::qualified`) of this model version, the seal covering every saved run but
/// `current` (the owner's own run, whose mismatches it counts itself). `Err`: why none.
pub fn sealed_classes(project: &std::path::Path, current: &str) -> Result<ClassSet, String> {
    let seal = newest_seal(project)?.ok_or(NOT_SEALED)?;
    if seal.model_version != FEE_MODEL_VERSION {
        return Err(format!(
            "LOCAL_FEE_SEAL_MODEL: sealed under model {}, this build runs {FEE_MODEL_VERSION}",
            seal.model_version
        ));
    }
    if let Some(run) = crate::run_layout::runs(project)
        .into_iter()
        .find(|r| r.run_id != current && !seal.runs.contains(&r.run_id))
    {
        return Err(format!(
            "LOCAL_FEE_SEAL_STALE: run {} is not in the seal (p0ctl fee-parity --seal)",
            run.run_id
        ));
    }
    let qualified = seal
        .classes
        .iter()
        .filter(|(class, counts)| {
            ClassParity {
                class: *class,
                counts: *counts,
                unreadable_runs: seal.unreadable.len(),
            }
            .qualified()
        })
        .fold(ClassSet::default(), |set, (c, _)| set.with(*c));
    if qualified.is_empty() {
        return Err(format!(
            "LOCAL_FEE_NOT_QUALIFIED: no class qualified in the seal (≥{PARITY_MIN_MESSAGES} distinct matched, none mismatched, every run readable)"
        ));
    }
    Ok(qualified)
}

#[cfg(test)]
mod tests {
    use super::tests_support::legacy;
    use super::*;

    fn of(r: &FeeParityStatsRecord, class: FeeClass) -> ParityCounts {
        r.classes.iter().find(|(c, _)| *c == class).unwrap().1
    }

    #[test]
    fn every_class_has_its_own_index() {
        let indices: std::collections::BTreeSet<usize> =
            FeeClass::all().map(FeeClass::index).collect();
        assert_eq!(indices.len(), CLASS_COUNT);
        assert!(FeeClass::all().all(|c| FeeClass::at(c.index()) == c));
    }

    /// The model version pins the model's outputs: a change of `tx::local_fee_lamports` must
    /// bump `FEE_MODEL_VERSION` (and so start a new tally) and this pin.
    #[test]
    fn the_model_version_pins_its_outputs() {
        let blockhash = Hash::new_unique();
        let outputs = [0u64, 1, 1_500, 2_000_000]
            .map(|price| darkbot_solana::tx::local_fee_lamports(&legacy(price, blockhash)));
        assert_eq!(
            (FEE_MODEL_VERSION, outputs),
            (1, [Some(5_000), Some(5_001), Some(5_300), Some(405_000)])
        );
    }

    /// Every node fee is counted in its class of the owner's tally: a distinct matched message
    /// once, the same message again as repeated (never qualified), mismatched otherwise (a
    /// repeated one too, the first mismatch kept), outside the model when it has none; a
    /// continued run's earlier counts are added, another model version's are not; a mismatch
    /// fails its class (`failed`). Another owner's tally is its own.
    #[test]
    fn every_quote_is_counted_once_per_message_in_its_class() {
        let plain = FeeClass {
            format: FeeFormat::Legacy,
            priority: false,
            role: FeeRole::BundleTip,
        };
        let paid = FeeClass {
            priority: true,
            ..plain
        };
        let parity = FeeParity::default();
        let blockhash = Hash::new_unique();
        let tip = legacy(0, blockhash);
        parity.observe(&tip, FeeRole::BundleTip, 5_000);
        parity.observe(&tip, FeeRole::BundleTip, 5_000);
        parity.observe(&tip, FeeRole::BundleTip, 5_001);
        // ⌈200 000 × 1 500 / 10⁶⌉ = 300 on top of the signature.
        parity.observe(&legacy(1_500, blockhash), FeeRole::BundleTip, 5_300);
        // The same instructions under a new blockhash are a new message.
        parity.observe(&legacy(0, Hash::new_unique()), FeeRole::BundleTip, 5_000);
        let run = parity.record(None);
        assert_eq!(
            (of(&run, plain), of(&run, paid)),
            (
                ParityCounts {
                    matched: 2,
                    repeated: 1,
                    mismatched: 1,
                    unsupported: 0
                },
                ParityCounts {
                    matched: 1,
                    ..Default::default()
                }
            )
        );
        assert_eq!(
            run.first_mismatch
                .as_ref()
                .map(|m| (m.rpc_lamports, m.local_lamports)),
            Some((5_001, 5_000)),
            "the first mismatch is kept"
        );
        assert_eq!(parity.mismatched(), 1);
        assert_eq!(parity.failed(None), ClassSet::default().with(plain));
        assert!(FeeParity::default()
            .record(None)
            .classes
            .iter()
            .all(|(_, n)| *n == ParityCounts::default()));
        // A resumed run's base adds to what this owner counted; another model version's does not.
        let base = FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION,
            classes: vec![(
                paid,
                ParityCounts {
                    matched: 7,
                    mismatched: 1,
                    ..Default::default()
                },
            )],
            first_mismatch: None,
        };
        assert_eq!(of(&parity.record(Some(&base)), paid).matched, 8);
        assert_eq!(
            parity.failed(Some(&base)),
            ClassSet::default().with(plain).with(paid),
            "a mismatch of the run's earlier owner fails its class too"
        );
        let other = FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION + 1,
            ..base
        };
        assert_eq!(of(&parity.record(Some(&other)), paid).matched, 1);
    }

    /// The saved runs' parity: the last record of each run's owner journal (in the run or its
    /// capture directory); a run without a record of this model version adds nothing; a torn run
    /// (a crash) counts with its last record and is named; a journal that cannot be read, or one
    /// with a recovery journal beside it, is named unreadable and nothing qualifies.
    #[test]
    fn saved_runs_name_crashed_and_unreadable_runs_and_an_unreadable_one_qualifies_nothing() {
        use crate::analytics::evidence::{OwnerEvidence, OwnerEvidenceWriter};
        let project =
            std::env::temp_dir().join(format!("p0_fee_parity_runs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let class = FeeClass {
            format: FeeFormat::V0,
            priority: false,
            role: FeeRole::Single,
        };
        let at = |matched| FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION,
            classes: vec![(
                class,
                ParityCounts {
                    matched,
                    ..Default::default()
                },
            )],
            first_mismatch: None,
        };
        let journal = |dir: std::path::PathBuf, records: Vec<FeeParityStatsRecord>| {
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("opportunities.p0j");
            let (mut w, _) = OwnerEvidenceWriter::start_rolling(
                path.clone(),
                dir.join("recovery.p0j"),
                64,
                std::time::Duration::from_millis(5),
                1 << 20,
                false,
            )
            .unwrap();
            for r in records {
                w.emit(OwnerEvidence::FeeParityStats(Box::new(r))).unwrap();
            }
            w.finish(None).unwrap();
            path
        };
        journal(
            project.join("data/shadow/a"),
            vec![at(3), at(PARITY_MIN_MESSAGES)],
        );
        journal(project.join("data/econ4h/b/capture"), vec![at(4)]);
        std::fs::create_dir_all(project.join("data/shadow/empty")).unwrap();
        let mut older = at(PARITY_MIN_MESSAGES);
        older.model_version = FEE_MODEL_VERSION + 1;
        journal(project.join("data/shadow/other_model"), vec![older]);
        let runs = saved_runs(&project);
        let mut matched: Vec<u64> = runs.records.iter().map(|r| of(r, class).matched).collect();
        matched.sort();
        assert_eq!(
            (matched, runs.crashed.len() + runs.unreadable.len()),
            (vec![4, PARITY_MIN_MESSAGES], 0)
        );
        let row = |runs: &SavedRuns| {
            qualification(runs)
                .into_iter()
                .find(|c| c.class == class)
                .unwrap()
        };
        assert!(row(&runs).qualified());
        // A torn journal (a crash): its last readable record counts, the run is named.
        let torn = journal(project.join("data/shadow/torn"), vec![at(1), at(2)]);
        let mut bytes = std::fs::read(&torn).unwrap();
        bytes.truncate(bytes.len() - 7);
        std::fs::write(&torn, bytes).unwrap();
        let runs = saved_runs(&project);
        assert_eq!(runs.crashed, vec!["torn".to_string()]);
        assert_eq!(
            row(&runs).counts.matched,
            4 + PARITY_MIN_MESSAGES + 2,
            "its last record"
        );
        assert!(row(&runs).qualified(), "a crashed run is a lower bound");
        // A journal that cannot be read blocks every class until it is resolved.
        let broken = project.join("data/shadow/broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("opportunities.p0j"), b"not a journal").unwrap();
        let runs = saved_runs(&project);
        assert_eq!(runs.unreadable, vec!["broken".to_string()]);
        assert!(
            !row(&runs).qualified(),
            "an unreadable run qualifies nothing"
        );
        std::fs::remove_dir_all(&broken).unwrap();
        // So does a recovery journal beside a run's journal (a primary write failed).
        std::fs::write(
            project.join("data/shadow/a/opportunities.recovery-1-2.p0j"),
            b"",
        )
        .unwrap();
        let runs = saved_runs(&project);
        assert_eq!(runs.unreadable, vec!["a".to_string()]);
        assert!(!row(&runs).qualified());
        let _ = std::fs::remove_dir_all(project);
    }

    /// A mismatch survives a crash once it is written: the writer syncs at once after a
    /// `FeeParityMismatch` record (here under an hour-long periodic sync), never after an
    /// ordinary one.
    #[test]
    fn a_mismatch_record_is_synced_at_once() {
        use crate::analytics::evidence::{OwnerEvidence, OwnerEvidenceWriter};
        let dir = std::env::temp_dir().join(format!("p0_fee_parity_sync_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (mut w, _) = OwnerEvidenceWriter::start_rolling(
            dir.join("opportunities.p0j"),
            dir.join("recovery.p0j"),
            64,
            std::time::Duration::from_secs(3_600),
            1 << 20,
            false,
        )
        .unwrap();
        let zero = FeeParity::default().record(None);
        let synced_after = |w: &OwnerEvidenceWriter, n: u64| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while w.syncs() < n && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            w.syncs()
        };
        w.emit(OwnerEvidence::FeeParityStats(Box::new(zero.clone())))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            w.syncs(),
            0,
            "an ordinary record waits for the periodic sync"
        );
        w.emit(OwnerEvidence::FeeParityMismatch(Box::new(zero)))
            .unwrap();
        assert_eq!(synced_after(&w, 1), 1, "a mismatch is synced at once");
        w.finish(None).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The qualification sums every run's last record of the current model version per class: a
    /// class qualifies at `PARITY_MIN_MESSAGES` distinct matched and no mismatch; one mismatch
    /// fails it for good (FAIL in the status); another model version counts nothing.
    #[test]
    fn a_class_qualifies_at_its_minimum_with_no_mismatch() {
        let class = FeeClass {
            format: FeeFormat::V0,
            priority: true,
            role: FeeRole::Single,
        };
        let run = |matched, mismatched, version| FeeParityStatsRecord {
            model_version: version,
            classes: vec![(
                class,
                ParityCounts {
                    matched,
                    mismatched,
                    ..Default::default()
                },
            )],
            first_mismatch: None,
        };
        let saved = |records: Vec<FeeParityStatsRecord>| SavedRuns {
            runs: Vec::new(),
            records,
            crashed: Vec::new(),
            unreadable: Vec::new(),
        };
        let of = |runs: &SavedRuns| {
            qualification(runs)
                .into_iter()
                .find(|c| c.class == class)
                .unwrap()
        };
        let half = PARITY_MIN_MESSAGES / 2;
        assert!(!of(&saved(vec![run(half, 0, FEE_MODEL_VERSION)])).qualified());
        let both = saved(vec![
            run(half, 0, FEE_MODEL_VERSION),
            run(PARITY_MIN_MESSAGES - half, 0, FEE_MODEL_VERSION),
        ]);
        assert!(of(&both).qualified());
        let line = status_line(&qualification(&both), "2 runs");
        assert!(
            line.starts_with("LOCAL_FEE_MODEL_OBSERVE: 2 runs: 1/16 classes"),
            "{line}"
        );
        let mut failed = both.clone();
        failed.records.push(run(5, 1, FEE_MODEL_VERSION));
        assert!(of(&failed).failed() && !of(&failed).qualified());
        let line = status_line(&qualification(&failed), "3 runs");
        assert!(
            line.starts_with("LOCAL_FEE_MODEL_FAIL: 3 runs: 0/16"),
            "{line}"
        );
        let other = saved(vec![run(PARITY_MIN_MESSAGES, 0, FEE_MODEL_VERSION + 1)]);
        assert_eq!(of(&other).counts, ParityCounts::default());
    }
}

#[cfg(test)]
mod phase2_tests {
    use super::tests_support::*;
    use super::*;

    /// ACTIVE decides only a single transaction's fee in a class it is active in (a bundle part
    /// never: nothing verifies it before the send), OBSERVE and OFF never; the simulation verifies
    /// a decided fee — the same lamports count as a match of its class, other lamports fail the
    /// class for good, none or another message verify nothing.
    #[test]
    fn a_local_fee_is_decided_only_where_active_and_verified_by_the_simulation() {
        let tx = legacy(0, Hash::new_unique());
        let single = FeeClass {
            format: FeeFormat::Legacy,
            priority: false,
            role: FeeRole::Single,
        };
        let tip = FeeClass {
            role: FeeRole::BundleTip,
            ..single
        };
        let both = ClassSet::default().with(single).with(tip);
        assert_eq!(
            FeeModel::active(both).classes(),
            ClassSet::default().with(single)
        );
        for model in [FeeModel::OFF, FeeModel::OBSERVE] {
            assert_eq!(decide(&tx, FeeRole::Single, &model), None);
        }
        let active = FeeModel::active(both);
        assert_eq!(decide(&tx, FeeRole::BundleTip, &active), None);
        let decided = decide(&tx, FeeRole::Single, &active).unwrap();
        assert_eq!(
            (
                decided.message_hash,
                decided.lamports,
                decided.model_version
            ),
            (tx.message.hash(), 5_000, FEE_MODEL_VERSION)
        );
        let paid = legacy(1_500, Hash::new_unique());
        assert_eq!(
            decide(&paid, FeeRole::Single, &active),
            None,
            "another class"
        );

        let parity = FeeParity::default();
        assert_eq!(parity.verify(&tx, &decided, Some(5_000)), Ok(()));
        assert_eq!(
            parity
                .record(None)
                .classes
                .iter()
                .find(|(c, _)| *c == single)
                .unwrap()
                .1
                .matched,
            1
        );
        assert_eq!(
            parity.verify(&tx, &decided, None),
            Err(Unverified::NoNodeFee)
        );
        assert_eq!(
            parity.verify(&paid, &decided, Some(5_000)),
            Err(Unverified::OtherMessage)
        );
        assert!(parity.failed(None).is_empty());
        assert_eq!(
            parity.verify(&tx, &decided, Some(5_001)),
            Err(Unverified::Mismatch {
                node: 5_001,
                local: 5_000
            })
        );
        assert_eq!(parity.failed(None), ClassSet::default().with(single));
    }

    /// The mode's controller values and start value.
    #[test]
    fn the_mode_takes_its_controller_values_and_its_start_value() {
        for (value, mode) in [
            ("OFF", FeeModelMode::Off),
            ("OBSERVE", FeeModelMode::Observe),
            ("ACTIVE", FeeModelMode::Active),
        ] {
            assert_eq!(FeeModelMode::parse(value), Some(mode));
            assert_eq!(mode.name(), value);
        }
        assert_eq!(FeeModelMode::parse("on"), None);
        assert_eq!(FeeModelMode::from_env(""), Ok(FeeModelMode::Observe));
        assert_eq!(FeeModelMode::from_env("active"), Ok(FeeModelMode::Active));
        assert!(FeeModelMode::from_env("on")
            .unwrap_err()
            .starts_with("LOCAL_FEE_MODEL_INVALID"));
    }

    /// The seal: none before `write_seal`; it qualifies the classes the saved runs qualify (a
    /// bundle class too — `FeeModel::active` decides only single ones); a run saved after it
    /// makes it stale (the owner's own run excepted) until the next seal; a seal without a
    /// qualified class names that.
    #[test]
    fn the_seal_names_the_qualified_classes_of_every_saved_run() {
        let project =
            std::env::temp_dir().join(format!("p0_fee_parity_seal_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let single = FeeClass {
            format: FeeFormat::V0,
            priority: true,
            role: FeeRole::Single,
        };
        let tip = FeeClass {
            role: FeeRole::BundleTip,
            ..single
        };
        let at = |classes: Vec<(FeeClass, u64, u64)>| FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION,
            classes: classes
                .into_iter()
                .map(|(c, matched, mismatched)| {
                    (
                        c,
                        ParityCounts {
                            matched,
                            mismatched,
                            ..Default::default()
                        },
                    )
                })
                .collect(),
            first_mismatch: None,
        };
        let err = |r: Result<ClassSet, String>| r.unwrap_err();
        assert!(err(sealed_classes(&project, "")).starts_with("LOCAL_FEE_NOT_SEALED"));
        run_journal(
            &project.join("data/shadow/a"),
            vec![at(vec![
                (single, PARITY_MIN_MESSAGES, 0),
                (tip, PARITY_MIN_MESSAGES, 0),
            ])],
        );
        let (n, seal) = write_seal(&project).unwrap();
        assert_eq!((n, seal.runs.clone()), (1, vec!["a".to_string()]));
        assert_eq!(
            sealed_classes(&project, ""),
            Ok(ClassSet::default().with(single).with(tip))
        );
        run_journal(&project.join("data/shadow/b"), vec![at(vec![])]);
        assert!(err(sealed_classes(&project, "")).starts_with("LOCAL_FEE_SEAL_STALE: run b"));
        assert!(sealed_classes(&project, "b").is_ok(), "the owner's own run");
        run_journal(
            &project.join("data/shadow/c"),
            vec![at(vec![(single, 1, 1), (tip, 1, 1)])],
        );
        assert_eq!(write_seal(&project).unwrap().0, 2);
        assert!(err(sealed_classes(&project, "")).starts_with("LOCAL_FEE_NOT_QUALIFIED"));
        let _ = std::fs::remove_dir_all(project);
    }
}

/// Test fixtures shared by the parity tests.
#[cfg(test)]
mod tests_support {
    use super::*;

    /// A legacy message (the pipeline builds none: these classes are the tests' own) under
    /// `blockhash`, paying `price` µLamports/CU under an explicit limit.
    pub(super) fn legacy(price: u64, blockhash: Hash) -> VersionedTransaction {
        use solana_sdk::instruction::Instruction;
        let payer = solana_sdk::pubkey::Pubkey::new_unique();
        let cb = |data: Vec<u8>| Instruction {
            program_id: solana_sdk::pubkey::Pubkey::from_str_const(
                darkbot_project0::protocol::COMPUTE_BUDGET_PROGRAM_ID,
            ),
            accounts: vec![],
            data,
        };
        let mut ixs = vec![Instruction {
            program_id: solana_sdk::pubkey::Pubkey::new_unique(),
            accounts: vec![],
            data: vec![1],
        }];
        if price > 0 {
            // SetComputeUnitLimit (2) and SetComputeUnitPrice (3).
            ixs.insert(0, cb([vec![2], 200_000u32.to_le_bytes().to_vec()].concat()));
            ixs.insert(1, cb([vec![3], price.to_le_bytes().to_vec()].concat()));
        }
        VersionedTransaction {
            signatures: vec![solana_sdk::signature::Signature::default()],
            message: VersionedMessage::Legacy(solana_sdk::message::Message::new_with_blockhash(
                &ixs,
                Some(&payer),
                &blockhash,
            )),
        }
    }

    /// A saved run's sealed owner journal at `dir` with `records`.
    pub(super) fn run_journal(dir: &std::path::Path, records: Vec<FeeParityStatsRecord>) {
        use crate::analytics::evidence::{OwnerEvidence, OwnerEvidenceWriter};
        std::fs::create_dir_all(dir).unwrap();
        let (mut w, _) = OwnerEvidenceWriter::start_rolling(
            dir.join("opportunities.p0j"),
            dir.join("recovery.p0j"),
            64,
            std::time::Duration::from_millis(5),
            1 << 20,
            false,
        )
        .unwrap();
        for r in records {
            w.emit(OwnerEvidence::FeeParityStats(Box::new(r))).unwrap();
        }
        w.finish(None).unwrap();
    }
}
