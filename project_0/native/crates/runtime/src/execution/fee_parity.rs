//! P6-MT.10 / S4: the parity of the local fee model (`tx::local_fee_lamports`) with every RPC fee
//! quote of the pipeline, per class of message — its format, whether it pays a priority fee, its
//! part of a bundle. The local model may replace `getFeeForMessage` only after 100 % parity over at
//! least `PARITY_MIN_MESSAGES` qualified messages of a class (PHASENPLAN P6-MT.10: "Lokales
//! Fee-Modell ersetzt `getFeeForMessage` erst nach 100 % Parität über >=10.000 qualifizierte
//! v0/v1-Messages mit/ohne Priority und Bundle-Bestandteile. Mismatch → RPC-Fallback +
//! Gate-FAIL"); one mismatch fails its class for good (the RPC quote stays). A qualified message is
//! a distinct one: the same message quoted again (a candidate re-evaluated under the same
//! blockhash) is compared again but counts once. Counted in the process, journaled by the owner
//! (`FEE_PARITY_STATS`, cumulative over a resumed run, at once after a mismatch), summed across the
//! saved runs of a project by `saved_runs`/`qualification` (an on-demand report, never an owner's
//! start; a run whose journal does not read to its end qualifies nothing). Every owner runs on the
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
/// The messages compared under the current blockhash (a message names its blockhash, so one of
/// an earlier blockhash never recurs): the set starts afresh with each blockhash.
static SEEN: std::sync::Mutex<Option<(Hash, std::collections::HashSet<Hash>)>> =
    std::sync::Mutex::new(None);

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
            let hash = tx.message.hash();
            let blockhash = *tx.message.recent_blockhash();
            let fresh = SEEN.lock().map_or(true, |mut seen| {
                let s = seen.get_or_insert_with(|| (blockhash, Default::default()));
                if s.0 != blockhash {
                    *s = (blockhash, Default::default());
                }
                s.1.insert(hash)
            });
            counts[if fresh { 0 } else { 1 }].fetch_add(1, Ordering::Relaxed);
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

/// The last `FEE_PARITY_STATS` record of the owner journal (chain) `journal`, and whether the
/// chain is clean — read to its sealed, complete end with no torn tail (`false`: what it lost or
/// still writes is unknown).
pub fn last_record(journal: &std::path::Path) -> (Option<FeeParityStatsRecord>, bool) {
    use crate::analytics::payload::Payload;
    let Ok(mut reader) = crate::storage::segments::ChainReader::open(journal) else {
        return (None, false);
    };
    let mut last = None;
    loop {
        match reader.next_frame() {
            Ok(Some(frame)) if frame.kind == FeeParityStatsRecord::KIND => {
                match frame.typed::<FeeParityStatsRecord>() {
                    Ok(r) => last = Some(r),
                    Err(_) => return (last, false),
                }
            }
            Ok(Some(_)) => {}
            // Clean only sealed, complete and without a torn tail (a crash's lost frames could
            // have held a mismatch).
            Ok(None) => {
                return (
                    last,
                    reader.sealed && reader.complete && reader.torn_bytes == 0,
                )
            }
            Err(_) => return (last, false),
        }
    }
}

/// The saved runs' parity: each run's last record, and how many runs' journals did not read to
/// their end (none of their classes, and so none at all, can be known clean).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedRuns {
    pub records: Vec<FeeParityStatsRecord>,
    pub incomplete: usize,
}

/// The saved runs of `project`: every run's owner journal (`until_stopped_view::SHADOW_RECORD` in
/// the run or its capture directory) read to its end — an on-demand report (`p0ctl
/// fee-parity`), never on an owner's start.
pub fn saved_runs(project: &std::path::Path) -> SavedRuns {
    let record = crate::orchestration::until_stopped_view::SHADOW_RECORD;
    let mut out = SavedRuns::default();
    for run in crate::run_layout::runs(project) {
        let Some(journal) = [run.path.join(record), run.path.join("capture").join(record)]
            .into_iter()
            .find(|p| p.is_file())
        else {
            continue;
        };
        let (last, complete) = last_record(&journal);
        if !complete {
            out.incomplete += 1;
        }
        out.records.extend(last);
    }
    out
}

/// The parity of one class over its records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassParity {
    pub class: FeeClass,
    pub counts: ParityCounts,
    /// Runs whose journal did not read to its end: nothing qualifies while one is.
    pub incomplete_runs: usize,
}

impl ClassParity {
    /// At least `PARITY_MIN_MESSAGES` distinct matched, none mismatched, every run read clean.
    pub fn qualified(&self) -> bool {
        self.counts.matched >= PARITY_MIN_MESSAGES
            && self.counts.mismatched == 0
            && self.incomplete_runs == 0
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
            incomplete_runs: runs.incomplete,
        })
        .collect()
}

/// The status of the local fee model over `classes` (`scope`: what they sum): how many are
/// qualified, the largest matched count, every mismatch and every incomplete run; FAIL once a
/// class failed. The RPC quote decides in this build (replacing it in a qualified class is not
/// built yet).
pub fn status_line(classes: &[ClassParity], scope: &str) -> String {
    let qualified = classes.iter().filter(|c| c.qualified()).count();
    let failed = classes.iter().filter(|c| c.failed()).count();
    let matched = classes.iter().map(|c| c.counts.matched).max().unwrap_or(0);
    let mismatched: u64 = classes.iter().map(|c| c.counts.mismatched).sum();
    let unsupported: u64 = classes.iter().map(|c| c.counts.unsupported).sum();
    let incomplete = classes.first().map_or(0, |c| c.incomplete_runs);
    format!(
        "{}: {scope}: {qualified}/{} classes qualified (≥{PARITY_MIN_MESSAGES} distinct matched, none mismatched), {failed} failed · most matched {matched} · mismatched {mismatched} · outside the model {unsupported} · incomplete runs {incomplete}; the RPC quote decides (v1: no v1 transaction is built)",
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
    /// capture directory); a run without a record adds nothing; a journal that does not read to
    /// its end is an incomplete run, and while one is nothing qualifies.
    #[test]
    fn saved_runs_read_each_runs_last_record_and_an_incomplete_one_qualifies_nothing() {
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
        let runs = saved_runs(&project);
        let mut matched: Vec<u64> = runs.records.iter().map(|r| of(r, class).matched).collect();
        matched.sort();
        assert_eq!(
            (matched, runs.incomplete),
            (vec![4, PARITY_MIN_MESSAGES], 0)
        );
        let row = |runs: &SavedRuns| {
            qualification(runs)
                .into_iter()
                .find(|c| c.class == class)
                .unwrap()
        };
        assert!(row(&runs).qualified());
        // A torn journal: what followed its last readable record is unknown.
        let torn = journal(project.join("data/shadow/torn"), vec![at(1)]);
        let mut bytes = std::fs::read(&torn).unwrap();
        bytes.truncate(bytes.len() - 7);
        std::fs::write(&torn, bytes).unwrap();
        let runs = saved_runs(&project);
        assert_eq!(runs.incomplete, 1);
        assert!(
            !row(&runs).qualified(),
            "an incomplete run qualifies nothing"
        );
        let _ = std::fs::remove_dir_all(project);
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
            incomplete: 0,
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
