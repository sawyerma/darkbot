//! P6-MT.10 / S4: the parity of the local fee model (`tx::local_fee_lamports`) with every RPC fee
//! quote of the pipeline, per class of message — its format, whether it pays a priority fee, its
//! part of a bundle. The local model may replace `getFeeForMessage` only after 100 % parity over at
//! least `PARITY_MIN_MESSAGES` qualified messages of a class (PHASENPLAN P6-MT.10: "Lokales
//! Fee-Modell ersetzt `getFeeForMessage` erst nach 100 % Parität über >=10.000 qualifizierte
//! v0/v1-Messages mit/ohne Priority und Bundle-Bestandteile. Mismatch → RPC-Fallback +
//! Gate-FAIL"); one mismatch fails its class for good (the RPC quote stays). Counted in the
//! process, journaled by the owner (`FEE_PARITY_STATS`, cumulative over a resumed run), summed
//! across the saved runs of a project by `qualification` — never a stored running total beside
//! the journals. No v1 transaction is built yet (S3-02): v1 has no class until it is.
use serde::{Deserialize, Serialize};
use solana_sdk::message::VersionedMessage;
use solana_sdk::transaction::VersionedTransaction;
use std::sync::atomic::{AtomicU64, Ordering};

/// The local fee model's rules (`tx::local_fee_lamports`): a change starts a new tally.
pub const FEE_MODEL_VERSION: u32 = 1;

/// PHASENPLAN P6-MT.10: the qualified messages of a class the local model needs, all matched.
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
    /// It requests a priority fee (or states one the model cannot read exactly).
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

    /// The class of `tx` in `role`.
    pub fn of(tx: &VersionedTransaction, role: FeeRole) -> FeeClass {
        FeeClass {
            format: match tx.message {
                VersionedMessage::Legacy(_) => FeeFormat::Legacy,
                VersionedMessage::V0(_) => FeeFormat::V0,
            },
            priority: darkbot_solana::tx::requested_priority_lamports(tx) != Some(0),
            role,
        }
    }
}

/// The counts of one class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParityCounts {
    /// The local model gave the RPC quote's lamports.
    pub matched: u64,
    /// It gave other lamports (the class fails).
    pub mismatched: u64,
    /// The message is outside the local model (`None`): counted, never qualified.
    pub unsupported: u64,
}

impl ParityCounts {
    fn add(self, o: ParityCounts) -> ParityCounts {
        ParityCounts {
            matched: self.matched.saturating_add(o.matched),
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

/// This process's counts per class: matched, mismatched, unsupported.
static COUNTS: [[AtomicU64; 3]; CLASS_COUNT] =
    [const { [const { AtomicU64::new(0) }; 3] }; CLASS_COUNT];
static FIRST_MISMATCH: std::sync::Mutex<Option<FeeMismatch>> = std::sync::Mutex::new(None);

/// Counts one RPC fee quote of `tx` in `role` against the local model (and the MT0 totals).
pub fn observe(tx: &VersionedTransaction, role: FeeRole, rpc_lamports: u64) {
    use darkbot_common::mt0::{Counter, MT0};
    let class = FeeClass::of(tx, role);
    let counts = &COUNTS[class.index()];
    match darkbot_solana::tx::local_fee_lamports(tx) {
        Some(local) if local == rpc_lamports => {
            counts[0].fetch_add(1, Ordering::Relaxed);
            MT0.add(Counter::FeeParityMatch, 1);
        }
        Some(local) => {
            counts[1].fetch_add(1, Ordering::Relaxed);
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
            counts[2].fetch_add(1, Ordering::Relaxed);
            MT0.add(Counter::FeeParityUnsupported, 1);
        }
    }
}

/// The run's record: `base` (what a resumed run journaled before) and this process's counts.
pub fn record(base: Option<&FeeParityStatsRecord>) -> FeeParityStatsRecord {
    let base_of = |class: FeeClass| {
        base.filter(|b| b.model_version == FEE_MODEL_VERSION)
            .and_then(|b| b.classes.iter().find(|(c, _)| *c == class))
            .map(|(_, n)| *n)
            .unwrap_or_default()
    };
    let classes = FeeClass::all()
        .map(|class| {
            let c = &COUNTS[class.index()];
            let here = ParityCounts {
                matched: c[0].load(Ordering::Relaxed),
                mismatched: c[1].load(Ordering::Relaxed),
                unsupported: c[2].load(Ordering::Relaxed),
            };
            (class, base_of(class).add(here))
        })
        .collect();
    let first_mismatch = base
        .filter(|b| b.model_version == FEE_MODEL_VERSION)
        .and_then(|b| b.first_mismatch.clone())
        .or_else(|| FIRST_MISMATCH.lock().ok().and_then(|f| f.clone()));
    FeeParityStatsRecord {
        model_version: FEE_MODEL_VERSION,
        classes,
        first_mismatch,
    }
}

/// The last `FEE_PARITY_STATS` record of the owner journal (chain) `journal`; a torn tail ends
/// the read; `None` without one.
pub fn last_record(journal: &std::path::Path) -> Option<FeeParityStatsRecord> {
    use crate::analytics::payload::Payload;
    let mut reader = crate::storage::segments::ChainReader::open(journal).ok()?;
    let mut last = None;
    while let Ok(Some(frame)) = reader.next_frame() {
        if frame.kind == FeeParityStatsRecord::KIND {
            if let Ok(r) = frame.typed::<FeeParityStatsRecord>() {
                last = Some(r);
            }
        }
    }
    last
}

/// The parity of one class over every saved run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassParity {
    pub class: FeeClass,
    pub counts: ParityCounts,
}

impl ClassParity {
    /// At least `PARITY_MIN_MESSAGES` matched and none mismatched.
    pub fn qualified(&self) -> bool {
        self.counts.matched >= PARITY_MIN_MESSAGES && self.counts.mismatched == 0
    }
}

/// P6-MT.10: the parity per class summed over `records` (each run's last record) of the current
/// model version — a record of another version counts nothing.
pub fn qualification(records: &[FeeParityStatsRecord]) -> Vec<ClassParity> {
    FeeClass::all()
        .map(|class| ClassParity {
            class,
            counts: records
                .iter()
                .filter(|r| r.model_version == FEE_MODEL_VERSION)
                .flat_map(|r| r.classes.iter().filter(|(c, _)| *c == class))
                .fold(ParityCounts::default(), |sum, (_, n)| sum.add(*n)),
        })
        .collect()
}

/// The last record of every saved run of `project` (but the one journaling to `except`): its
/// owner journal, `opportunities.p0j` in the run or its capture directory, read to its end — an
/// on-demand report (`p0ctl fee-parity`), never on an owner's start.
pub fn saved_runs(
    project: &std::path::Path,
    except: Option<&std::path::Path>,
) -> Vec<FeeParityStatsRecord> {
    let except = except.and_then(|e| e.canonicalize().ok());
    crate::run_layout::runs(project)
        .iter()
        .filter_map(|run| {
            ["opportunities.p0j", "capture/opportunities.p0j"]
                .iter()
                .map(|p| run.path.join(p))
                .find(|p| p.is_file())
                .filter(|p| except.is_none() || p.canonicalize().ok() != except)
                .and_then(|p| last_record(&p))
        })
        .collect()
}

/// The status of the local fee model over `classes` (`scope`: what they sum): how many are
/// qualified, the largest matched count and every mismatch; the RPC quote decides in this build
/// (replacing it in a qualified class is not built yet).
pub fn status_line(classes: &[ClassParity], scope: &str) -> String {
    let qualified = classes.iter().filter(|c| c.qualified()).count();
    let matched = classes.iter().map(|c| c.counts.matched).max().unwrap_or(0);
    let mismatched: u64 = classes.iter().map(|c| c.counts.mismatched).sum();
    let unsupported: u64 = classes.iter().map(|c| c.counts.unsupported).sum();
    format!(
        "LOCAL_FEE_MODEL_OBSERVE: {scope}: {qualified}/{} classes qualified (≥{PARITY_MIN_MESSAGES} matched, none mismatched) · most matched {matched} · mismatched {mismatched} · outside the model {unsupported}; the RPC quote decides (v1: no v1 transaction is built)",
        classes.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A legacy message (the pipeline builds none: these classes are the tests' own) paying
    /// `price` µLamports/CU under an explicit limit.
    fn legacy(price: u64) -> VersionedTransaction {
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
            message: VersionedMessage::Legacy(solana_sdk::message::Message::new(
                &ixs,
                Some(&payer),
            )),
        }
    }

    fn counts(class: FeeClass) -> ParityCounts {
        record(None)
            .classes
            .into_iter()
            .find(|(c, _)| *c == class)
            .unwrap()
            .1
    }

    /// Every RPC quote is counted in its class: matched when the local model gives its lamports,
    /// mismatched otherwise (with the first mismatch kept), outside the model when it has none;
    /// a continued run's record adds its earlier counts.
    #[test]
    fn every_quote_is_counted_in_its_class_and_a_resumed_run_adds_its_base() {
        let plain = FeeClass {
            format: FeeFormat::Legacy,
            priority: false,
            role: FeeRole::BundleTip,
        };
        let paid = FeeClass {
            priority: true,
            ..plain
        };
        let (before_plain, before_paid) = (counts(plain), counts(paid));
        observe(&legacy(0), FeeRole::BundleTip, 5_000);
        observe(&legacy(0), FeeRole::BundleTip, 5_001);
        // ⌈200 000 × 1 500 / 10⁶⌉ = 300 on top of the signature.
        observe(&legacy(1_500), FeeRole::BundleTip, 5_300);
        let (plain_now, paid_now) = (counts(plain), counts(paid));
        assert_eq!(
            (
                plain_now.matched - before_plain.matched,
                plain_now.mismatched - before_plain.mismatched,
                paid_now.matched - before_paid.matched
            ),
            (1, 1, 1)
        );
        assert!(record(None).first_mismatch.is_some(), "a mismatch is kept");
        // A resumed run's base adds to this process's counts; another model version does not.
        let base = FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION,
            classes: vec![(
                plain,
                ParityCounts {
                    matched: 7,
                    mismatched: 0,
                    unsupported: 2,
                },
            )],
            first_mismatch: None,
        };
        let resumed = record(Some(&base));
        let of = |r: &FeeParityStatsRecord| r.classes.iter().find(|(c, _)| *c == plain).unwrap().1;
        assert!(of(&resumed).matched >= plain_now.matched + 7);
        let other = FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION + 1,
            ..base
        };
        assert!(of(&record(Some(&other))).matched < of(&resumed).matched);
    }

    /// The saved runs' parity: the last record of each run's owner journal (in the run or its
    /// capture directory) but the running one's own; a run without a record adds nothing.
    #[test]
    fn saved_runs_are_read_from_each_runs_last_record_but_the_running_one() {
        use crate::analytics::evidence::{OwnerEvidence, OwnerEvidenceWriter};
        let project =
            std::env::temp_dir().join(format!("p0_fee_parity_runs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&project);
        let at = |matched| FeeParityStatsRecord {
            model_version: FEE_MODEL_VERSION,
            classes: vec![(
                FeeClass {
                    format: FeeFormat::V0,
                    priority: false,
                    role: FeeRole::Single,
                },
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
        journal(project.join("data/shadow/a"), vec![at(3), at(10)]);
        journal(project.join("data/econ4h/b/capture"), vec![at(4)]);
        let running = journal(project.join("data/shadow/c"), vec![at(99)]);
        std::fs::create_dir_all(project.join("data/shadow/empty")).unwrap();
        let mut matched: Vec<u64> = saved_runs(&project, Some(&running))
            .iter()
            .map(|r| r.classes[0].1.matched)
            .collect();
        matched.sort();
        assert_eq!(
            matched,
            vec![4, 10],
            "each run's last record, not the running one"
        );
        assert_eq!(saved_runs(&project, None).len(), 3);
        assert_eq!(last_record(&running).unwrap().classes[0].1.matched, 99);
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn every_class_has_its_own_index() {
        let indices: std::collections::BTreeSet<usize> =
            FeeClass::all().map(FeeClass::index).collect();
        assert_eq!(indices.len(), CLASS_COUNT);
        assert!(FeeClass::all().all(|c| FeeClass::at(c.index()) == c));
    }

    /// The qualification sums every run's last record of the current model version per class: a
    /// class qualifies at `PARITY_MIN_MESSAGES` matched and no mismatch; one mismatch fails it
    /// for good; another model version counts nothing.
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
                    unsupported: 0,
                },
            )],
            first_mismatch: None,
        };
        let of = |records: &[FeeParityStatsRecord]| {
            qualification(records)
                .into_iter()
                .find(|c| c.class == class)
                .unwrap()
        };
        let half = PARITY_MIN_MESSAGES / 2;
        assert!(!of(&[run(half, 0, FEE_MODEL_VERSION)]).qualified());
        let both = [
            run(half, 0, FEE_MODEL_VERSION),
            run(PARITY_MIN_MESSAGES - half, 0, FEE_MODEL_VERSION),
        ];
        assert!(of(&both).qualified());
        let failed = [
            both[0].clone(),
            both[1].clone(),
            run(5, 1, FEE_MODEL_VERSION),
        ];
        assert!(!of(&failed).qualified(), "one mismatch fails the class");
        let other = [run(PARITY_MIN_MESSAGES, 0, FEE_MODEL_VERSION + 1)];
        assert_eq!(of(&other).counts, ParityCounts::default());
        let line = status_line(&qualification(&both), "2 runs");
        assert!(
            line.starts_with("LOCAL_FEE_MODEL_OBSERVE: 2 runs: 1/16 classes"),
            "{line}"
        );
    }
}
