//! FE-04 attempt archive. A wallet journal holds exactly one attempt (one-shot); a finished
//! attempt — settled, no exit open, no position left — is sealed and moved to
//! `attempts/<wallet>/<approval_id>.p0j`, so the next separately approved attempt starts a fresh
//! journal. The archive is the attempt history: an approval id is single-use across it, the
//! safety layer replays its bookings on every start and the accounting reads it. Nothing is
//! deleted, truncated or released here.
use super::{
    cause, err, exit, journal, replay, safety_replay_of, Journal, Phase, SafetyReplay, Saved,
};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// Sub-directory of the journal directory holding the archived attempts, per wallet.
pub const ATTEMPTS_DIR: &str = "attempts";

fn wallet_archive(directory: &Path, wallet: &str) -> PathBuf {
    directory.join(ATTEMPTS_DIR).join(wallet)
}

/// An approval id names its archived journal: `[A-Za-z0-9_-]{1,64}`.
pub(crate) fn approval_id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// An approval authorizes one attempt: refused when its id is not a valid archive name or an
/// archived attempt already used it.
pub(super) fn single_use(directory: &Path, wallet: &str, approval_id: &str) -> Result<(), String> {
    if !approval_id_valid(approval_id) {
        return Err(err("APPROVAL_ID_INVALID"));
    }
    if wallet_archive(directory, wallet)
        .join(format!("{approval_id}.p0j"))
        .exists()
    {
        return Err(err("APPROVAL_ALREADY_CONSUMED"));
    }
    Ok(())
}

/// Whether an archived attempt already used `approval_id` (the approval is spent).
pub fn is_consumed(directory: &Path, wallet: &str, approval_id: &str) -> bool {
    approval_id_valid(approval_id)
        && wallet_archive(directory, wallet)
            .join(format!("{approval_id}.p0j"))
            .exists()
}

fn sync_dir(dir: &Path) -> Result<(), String> {
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| cause("ARCHIVE_SYNC", &e))
}

impl Journal {
    /// Seals this finished attempt and moves it (and a historical JSON import of it) into the
    /// archive; the directories are synced before the wallet lock is released. Refused unless
    /// the attempt is settled with no exit open and no position left.
    pub fn archive(mut self) -> Result<PathBuf, String> {
        let s = self
            .saved
            .as_ref()
            .ok_or_else(|| err("ARCHIVE_NO_ATTEMPT"))?;
        if !matches!(s.phase, Phase::Settled(_)) {
            return Err(err("ARCHIVE_NOT_SETTLED"));
        }
        if exit::active(s).is_some() {
            return Err(err("ARCHIVE_EXIT_OPEN"));
        }
        if exit::open_positions(s).values().any(|p| p.units != 0) {
            return Err(err("ARCHIVE_POSITION_OPEN"));
        }
        let id = s.approval.approval_id.clone();
        if !approval_id_valid(&id) {
            return Err(err("APPROVAL_ID_INVALID"));
        }
        let dir = wallet_archive(&self.directory, &self.wallet);
        fs::create_dir_all(&dir).map_err(|e| cause("ARCHIVE_IO", &e))?;
        let target = dir.join(format!("{id}.p0j"));
        if target.exists() {
            return Err(err("ARCHIVE_EXISTS"));
        }
        self.writer
            .seal(true)
            .map_err(|e| cause("JOURNAL_WRITE", &e))?;
        let active = self.directory.join(format!("{}.p0j", self.wallet));
        fs::rename(&active, &target).map_err(|e| cause("ARCHIVE_IO", &e))?;
        let legacy = super::migrated_legacy(&self.directory, &self.wallet);
        if legacy.exists() {
            fs::rename(&legacy, dir.join(format!("{id}.json")))
                .map_err(|e| cause("ARCHIVE_IO", &e))?;
        }
        sync_dir(&dir)?;
        sync_dir(&self.directory)?;
        Ok(target)
    }
}

/// One archived attempt, read back from its sealed journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedAttempt {
    pub path: PathBuf,
    pub(super) saved: Saved,
}

impl ArchivedAttempt {
    pub fn approval_id(&self) -> &str {
        &self.saved.approval.approval_id
    }
    /// B3: its bookings for the safety layer (no held capital: it is settled).
    pub fn safety_replay(&self) -> SafetyReplay {
        safety_replay_of(&self.saved)
    }
}

/// Every archived attempt of `wallet` in approval-id order, read-only. Each journal must be
/// sealed, complete and a settled attempt of this wallet; anything else fails closed.
pub fn archived(directory: &Path, wallet: &str) -> Result<Vec<ArchivedAttempt>, String> {
    let dir = wallet_archive(directory, wallet);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| cause("ARCHIVE_IO", &e))?
        .map(|e| e.map(|e| e.path()).map_err(|e| cause("ARCHIVE_IO", &e)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "p0j"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let mut reader =
                journal::Reader::open(&path).map_err(|e| cause("ARCHIVE_CORRUPT", &e))?;
            while reader
                .next_frame()
                .map_err(|e| cause("ARCHIVE_CORRUPT", &e))?
                .is_some()
            {}
            if !(reader.sealed && reader.complete) {
                return Err(err("ARCHIVE_NOT_SEALED"));
            }
            let saved = replay(&path, wallet)?
                .0
                .ok_or_else(|| err("ARCHIVE_EMPTY"))?;
            let named = path.file_stem().and_then(|n| n.to_str());
            if !matches!(saved.phase, Phase::Settled(_))
                || named != Some(saved.approval.approval_id.as_str())
                || saved.approval.wallet != wallet
            {
                return Err(err("ARCHIVE_FOREIGN"));
            }
            Ok(ArchivedAttempt { path, saved })
        })
        .collect()
}

impl super::Journal {
    /// The accounting state of the journal's attempt; `None` without an attempt.
    pub fn step_state(&self) -> Option<crate::execution::accounting::StepState> {
        self.saved.as_ref().map(|s| record(s, false).state)
    }
}

/// The accounting view of one attempt (archived or active).
fn record(saved: &Saved, archived: bool) -> crate::execution::accounting::AttemptRecord {
    use crate::execution::accounting::{AttemptRecord, ExitRecord, StepState};
    let (state, settlement) = match &saved.phase {
        Phase::Reserved => (StepState::Reserved, None),
        Phase::HandoffUnknown => (StepState::Unknown, None),
        Phase::HaltFinalized => (StepState::Finalized, None),
        Phase::Settled(done) => (StepState::Settled, Some(done.clone())),
    };
    AttemptRecord {
        approval_id: saved.approval.approval_id.clone(),
        target: saved.intent.target.clone(),
        funding_class: saved.intent.funding_class.clone(),
        archived,
        state,
        settlement,
        exits: saved
            .exits
            .iter()
            .map(|e| {
                let (state, settlement) = match &e.phase {
                    exit::ExitPhase::Reserved => (StepState::Reserved, None),
                    exit::ExitPhase::HandoffUnknown => (StepState::Unknown, None),
                    exit::ExitPhase::Finalized => (StepState::Finalized, None),
                    exit::ExitPhase::Settled(x) => (StepState::Settled, Some(x.clone())),
                };
                ExitRecord {
                    approval_id: e.approval.approval_id.clone(),
                    mint: e.intent.mint.clone(),
                    state,
                    settlement,
                    fe06: e.fe06.clone(),
                }
            })
            .collect(),
        open_positions: exit::open_positions(saved),
        hold: saved.hold.clone(),
        fe06: saved.fe06.clone(),
    }
}

/// FE-07: every own attempt of `wallet` — archived (verified sealed) then the active journal,
/// read-only without the wallet lock. An unreadable journal is an error.
pub fn records(
    directory: &Path,
    wallet: &str,
) -> Result<Vec<crate::execution::accounting::AttemptRecord>, String> {
    let mut out: Vec<_> = archived(directory, wallet)?
        .iter()
        .map(|a| record(&a.saved, true))
        .collect();
    let active = directory.join(format!("{wallet}.p0j"));
    if active.exists() {
        if let Some(saved) = replay(&active, wallet)?.0 {
            out.push(record(&saved, false));
        }
    }
    Ok(out)
}
