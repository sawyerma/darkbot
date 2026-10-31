//! P6-MT.10 / S4: the parity of the local fee model (`tx::local_fee_lamports`) with every RPC fee
//! quote of the pipeline, per class of message — its format, whether it pays a priority fee, its
//! part of a bundle. The local model may replace `getFeeForMessage` only after 100 % parity over at
//! least `PARITY_MIN_MESSAGES` qualified messages of a class (PHASENPLAN P6-MT.10: "Lokales
//! Fee-Modell ersetzt `getFeeForMessage` erst nach 100 % Parität über >=10.000 qualifizierte
//! v0/v1-Messages mit/ohne Priority und Bundle-Bestandteile. Mismatch → RPC-Fallback +
//! Gate-FAIL"); one mismatch fails its class for good (the RPC quote stays). A qualified message is
//! a distinct one: the same message quoted again (a candidate re-evaluated under the same
//! blockhash) is compared again but counts once. Counted in the process, journaled by the owner
//! (`FEE_PARITY_STATS`: at its start, when the counts changed, at shutdown, and synced at once
//! after a new mismatch — so a mismatch survives a crash once one owner pass and one sync passed),
//! summed across the saved runs of a project by `saved_runs`/`qualification` (an on-demand report,
//! never an owner's start). A crashed or still running run counts with its last record and is
//! named: its matches are a lower bound; a mismatch it journaled is durable, but one quoted in the
//! owner pass it crashed in (bounded by that pass's candidate network deadlines, the evidence
//! queued before it and one sync) can be lost — phase 2 re-checks every landed fee. A run whose
//! journal cannot be read (an error, a cut chain, an undecodable record, a recovery journal from a
//! failed write) qualifies nothing until it is resolved. Every owner runs on the
//! Mainnet cluster only (BOOT-12 genesis pin), so every record is Mainnet's. No v1 transaction is
//! built yet (S3-02): v1 has no class until it is.
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

    fn since(self, start: ParityCounts) -> ParityCounts {
        ParityCounts {
            matched: self.matched.saturating_sub(start.matched),
            repeated: self.repeated.saturating_sub(start.repeated),
            mismatched: self.mismatched.saturating_sub(start.mismatched),
            unsupported: self.unsupported.saturating_sub(start.unsupported),
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

/// This process's counts per class: matched, repeated, mismatched, unsupported.
static COUNTS: [[AtomicU64; 4]; CLASS_COUNT] =
    [const { [const { AtomicU64::new(0) }; 4] }; CLASS_COUNT];
static FIRST_MISMATCH: std::sync::Mutex<Option<FeeMismatch>> = std::sync::Mutex::new(None);
/// The messages compared per blockhash and when that blockhash was last seen: a set is dropped
/// once its blockhash is older than `blockhash_max_age_ms` (no fresh message names it any more), so
/// evaluations around a refresh, under the old and the new blockhash, never count a message twice.
type Seen = std::collections::HashMap<Hash, (std::time::Instant, std::collections::HashSet<Hash>)>;
static SEEN: std::sync::Mutex<Option<Seen>> = std::sync::Mutex::new(None);

/// Whether `tx` is compared for the first time (a poisoned record of what was seen: never first).
fn first_seen(tx: &VersionedTransaction) -> bool {
    let Ok(mut seen) = SEEN.lock() else {
        return false;
    };
    let now = std::time::Instant::now();
    let max_age = std::time::Duration::from_millis(
        darkbot_solana::runtime_config::active()
            .limits
            .blockhash_max_age_ms,
    );
    let seen = seen.get_or_insert_with(Default::default);
    seen.retain(|_, (at, _)| now.saturating_duration_since(*at) <= max_age);
    let (at, hashes) = seen
        .entry(*tx.message.recent_blockhash())
        .or_insert_with(|| (now, Default::default()));
    *at = now;
    hashes.insert(tx.message.hash())
}

/// This process's counts, per class (an owner's start: what was not its own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts([ParityCounts; CLASS_COUNT]);

/// The counts now.
pub fn counts() -> Counts {
    let mut out = [ParityCounts::default(); CLASS_COUNT];
    for (o, c) in out.iter_mut().zip(COUNTS.iter()) {
        *o = ParityCounts {
            matched: c[0].load(Ordering::Relaxed),
            repeated: c[1].load(Ordering::Relaxed),
            mismatched: c[2].load(Ordering::Relaxed),
            unsupported: c[3].load(Ordering::Relaxed),
        };
    }
    Counts(out)
}

impl Counts {
    /// Every mismatch so far (an owner journals at once when it grows).
    pub fn mismatched(&self) -> u64 {
        self.0.iter().map(|c| c.mismatched).sum()
    }
}

/// Counts one RPC fee quote of `tx` in `role` against the local model (and the MT0 totals).
pub fn observe(tx: &VersionedTransaction, role: FeeRole, rpc_lamports: u64) {
    use darkbot_common::mt0::{Counter, MT0};
    let local = darkbot_solana::tx::local_fee_lamports(tx);
    let class = FeeClass::of(tx, role, local);
    let counts = &COUNTS[class.index()];
    match local {
        Some(local) if local == rpc_lamports => {
            counts[if first_seen(tx) { 0 } else { 1 }].fetch_add(1, Ordering::Relaxed);
            MT0.add(Counter::FeeParityMatch, 1);
        }
        Some(local) => {
            counts[2].fetch_add(1, Ordering::Relaxed);
            MT0.add(Counter::FeeParityMismatch, 1);
            if let Ok(mut first) = FIRST_MISMATCH.lock() {
                first.get_or_insert(FeeMismatch {
                    class,
                    message_hash: tx.message.hash().to_bytes(),
                    rpc_lamports,
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

/// The run's record: `base` (what a resumed run journaled before) and what this process counted
/// since `start` (the owner's start).
pub fn record(base: Option<&FeeParityStatsRecord>, start: &Counts) -> FeeParityStatsRecord {
    let base = base.filter(|b| b.model_version == FEE_MODEL_VERSION);
    let now = counts();
    let classes = FeeClass::all()
        .map(|class| {
            let earlier = base
                .and_then(|b| b.classes.iter().find(|(c, _)| *c == class))
                .map(|(_, n)| *n)
                .unwrap_or_default();
            let i = class.index();
            (class, earlier.add(now.0[i].since(start.0[i])))
        })
        .collect();
    let first_mismatch = base.and_then(|b| b.first_mismatch.clone()).or_else(|| {
        (now.mismatched() > start.mismatched())
            .then(|| FIRST_MISMATCH.lock().ok().and_then(|f| f.clone()))
            .flatten()
    });
    FeeParityStatsRecord {
        model_version: FEE_MODEL_VERSION,
        classes,
        first_mismatch,
    }
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

/// The status of the local fee model over `classes` (`scope`: what they sum): how many are
/// qualified, the largest matched count and every mismatch; FAIL once a class failed. The RPC
/// quote decides in this build (replacing it in a qualified class is not built yet).
pub fn status_line(classes: &[ClassParity], scope: &str) -> String {
    let qualified = classes.iter().filter(|c| c.qualified()).count();
    let failed = classes.iter().filter(|c| c.failed()).count();
    let matched = classes.iter().map(|c| c.counts.matched).max().unwrap_or(0);
    let mismatched: u64 = classes.iter().map(|c| c.counts.mismatched).sum();
    let unsupported: u64 = classes.iter().map(|c| c.counts.unsupported).sum();
    format!(
        "{}: {scope}: {qualified}/{} classes qualified (≥{PARITY_MIN_MESSAGES} distinct matched, none mismatched), {failed} failed · most matched {matched} · mismatched {mismatched} · outside the model {unsupported}; the RPC quote decides (v1: no v1 transaction is built)",
        if failed > 0 {
            "LOCAL_FEE_MODEL_FAIL"
        } else {
            "LOCAL_FEE_MODEL_OBSERVE"
        },
        classes.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A legacy message (the pipeline builds none: these classes are the tests' own) under
    /// `blockhash`, paying `price` µLamports/CU under an explicit limit.
    fn legacy(price: u64, blockhash: Hash) -> VersionedTransaction {
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

    /// Every RPC quote is counted in its class: a distinct matched message once, the same
    /// message again as repeated (never qualified), mismatched otherwise (a repeated one too,
    /// the first mismatch kept), outside the model when it has none; the run's record is what
    /// was counted since the owner's start, a continued run's earlier counts added.
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
        let start = counts();
        let blockhash = Hash::new_unique();
        let tip = legacy(0, blockhash);
        observe(&tip, FeeRole::BundleTip, 5_000);
        observe(&tip, FeeRole::BundleTip, 5_000);
        observe(&tip, FeeRole::BundleTip, 5_001);
        // ⌈200 000 × 1 500 / 10⁶⌉ = 300 on top of the signature.
        observe(&legacy(1_500, blockhash), FeeRole::BundleTip, 5_300);
        // The same instructions under a new blockhash are a new message.
        observe(&legacy(0, Hash::new_unique()), FeeRole::BundleTip, 5_000);
        let run = record(None, &start);
        let (plain_now, paid_now) = (of(&run, plain), of(&run, paid));
        assert!(
            plain_now.matched >= 2 && plain_now.repeated >= 1,
            "{plain_now:?}"
        );
        assert!(
            plain_now.mismatched >= 1 && paid_now.matched >= 1,
            "{plain_now:?}"
        );
        assert!(run.first_mismatch.is_some(), "a mismatch is kept");
        // A resumed run's base adds to what was counted since the start; another model version
        // does not.
        let base = FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION,
            classes: vec![(
                plain,
                ParityCounts {
                    matched: 7,
                    ..Default::default()
                },
            )],
            first_mismatch: None,
        };
        let now = counts();
        let resumed = record(Some(&base), &now);
        assert_eq!(of(&resumed, plain).matched, 7, "nothing since this start");
        let other = FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION + 1,
            ..base
        };
        assert_eq!(of(&record(Some(&other), &now), plain).matched, 0);
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
        let zero = record(None, &counts());
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
