//! BSC-V8-02/03: the native owner of the Strategy C training runs: one
//! profile revision run on the one Strategy C core, never with real funds,
//! the LIVE wallet, nonce or budget. A REPLAY run takes the market records
//! of one recorded runtime run (the catalogue: [`replay_sources`]) through
//! Strategy C with the profile's values ([`crate::runtime_strategy::train`]);
//! SHADOW and PAPER runs need the live inputs and are refused until a
//! runtime path feeds them (no stand-in).
//!
//! State `<sources>/training/`: `runs/<run_id>/<seq>.dbrec`, every state of
//! a run created create-new and kept (1 = PENDING at its start request,
//! then RUNNING, then one end: COMPLETED, STOPPED, FAILED or INTERRUPTED),
//! `runs/<run_id>/stop.dbrec` a stop request, `receipts/<request_id>.dbrec`
//! per request with its payload's hash. Run IDs are sequential (`t000001`
//! …): a start claims the next one create-new, and only the newest run may
//! be active (one run at a time), so two concurrent starts cannot both run.
//! The run's job (its own launchd program, away from the runtime's hot
//! path) executes it; a job that finds its run RUNNING already was
//! restarted after a crash and ends it INTERRUPTED.

use crate::runtime_strategy::{Training, TrainingEnd, TrainingStep, train};
use crate::sources::{
    VenueId, create_durable_dir, private_directory, read_private, safe_id, write_new,
};
use crate::strategy_profiles::{self as profiles, Status as ProfileStatus};
use crate::strategy_settings::{self as settings, KEYS};
use darkbot_bsc_record::schema::bsc_runtime::{
    strategy_settings as ss, training_receipt as tr, training_run as rr,
};
use darkbot_bsc_record::{Draft, Value, decode};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The owner's directory under the source-mode owner's.
pub const DIR: &str = "training";

/// How a run executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A recorded runtime run's market records.
    Replay,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Replay => "REPLAY",
        }
    }

    /// `REPLAY`; SHADOW and PAPER are refused with their reason.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "REPLAY" => Ok(Mode::Replay),
            "SHADOW" | "PAPER" => Err(format!(
                "a {name} training run needs the runtime's live inputs: not implemented yet (BSC-V8-02); REPLAY runs on a recorded run"
            )),
            other => Err(format!("{other} is no training mode")),
        }
    }
}

/// What ends a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plan {
    /// This much journal time of the recorded run (ms).
    Duration(u64),
    /// This many launches of its venues.
    Launches(u64),
    /// The recorded run's end or a stop.
    UntilStopped,
}

impl Plan {
    pub fn kind(self) -> &'static str {
        match self {
            Plan::Duration(_) => "DURATION",
            Plan::Launches(_) => "LAUNCH_COUNT",
            Plan::UntilStopped => "UNTIL_STOPPED",
        }
    }

    /// `kind` with exactly its bound (a positive one; none for
    /// UNTIL_STOPPED).
    pub fn parse(kind: &str, bound: Option<u64>) -> Result<Self, String> {
        match (kind, bound) {
            ("DURATION", Some(n)) if n > 0 => Ok(Plan::Duration(n)),
            ("LAUNCH_COUNT", Some(n)) if n > 0 => Ok(Plan::Launches(n)),
            ("UNTIL_STOPPED", None) => Ok(Plan::UntilStopped),
            _ => Err(format!(
                "{kind}: DURATION and LAUNCH_COUNT take one positive bound, UNTIL_STOPPED none"
            )),
        }
    }

    fn bound(self) -> Option<u64> {
        match self {
            Plan::Duration(n) | Plan::Launches(n) => Some(n),
            Plan::UntilStopped => None,
        }
    }
}

/// A run's lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Completed,
    Stopped,
    Failed,
    Interrupted,
}

const STATUSES: [(Status, &str); 6] = [
    (Status::Pending, "PENDING"),
    (Status::Running, "RUNNING"),
    (Status::Completed, "COMPLETED"),
    (Status::Stopped, "STOPPED"),
    (Status::Failed, "FAILED"),
    (Status::Interrupted, "INTERRUPTED"),
];

impl Status {
    pub fn name(self) -> &'static str {
        STATUSES
            .iter()
            .find(|(s, _)| *s == self)
            .map_or("", |(_, n)| n)
    }

    fn of(name: &str) -> Option<Self> {
        STATUSES.iter().find(|(_, n)| *n == name).map(|(s, _)| *s)
    }

    pub fn active(self) -> bool {
        matches!(self, Status::Pending | Status::Running)
    }
}

/// One state of a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub run_id: String,
    pub seq: u64,
    pub profile_id: String,
    pub profile_revision: u64,
    pub config_sha256: [u8; 32],
    /// The SHA-256 of the configuration file its job ran on (review L4);
    /// absent before it ran.
    pub config_file_sha256: Option<[u8; 32]>,
    pub mode: Mode,
    pub venues: Vec<VenueId>,
    pub plan: Plan,
    pub replay_source: String,
    pub status: Status,
    pub stop_reason: Option<String>,
    pub started_unix_ms: Option<u64>,
    pub ended_unix_ms: Option<u64>,
    pub request_id: String,
    /// The profile revision's values (every settable key, [`KEYS`] order).
    pub values: Vec<(&'static str, String)>,
    pub result: Option<Training>,
}

fn word(r: &darkbot_bsc_record::Record, tag: u16) -> Result<&str, String> {
    r.text(tag).map_err(|e| e.0)
}

impl Run {
    /// The next state of this run (its number one higher).
    fn next(&self, status: Status) -> Run {
        Run {
            seq: self.seq + 1,
            status,
            ..self.clone()
        }
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&rr::SCHEMA);
        d.set(rr::RUN_ID, self.run_id.as_str())
            .set(rr::SEQ, self.seq)
            .set(rr::PROVENANCE_TOOL, "bsc_runtime")
            .set(rr::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(rr::PROFILE_ID, self.profile_id.as_str())
            .set(rr::PROFILE_REVISION, self.profile_revision)
            .set(rr::CONFIG_SHA256, Value::B256(self.config_sha256))
            .set_opt(
                rr::CONFIG_FILE_SHA256,
                self.config_file_sha256.map(Value::B256),
            )
            .set(rr::MODE, self.mode.name())
            .set(rr::STOP_KIND, self.plan.kind())
            .set_opt(
                rr::DURATION_MS,
                matches!(self.plan, Plan::Duration(_))
                    .then(|| self.plan.bound())
                    .flatten(),
            )
            .set_opt(
                rr::MAX_LAUNCHES,
                matches!(self.plan, Plan::Launches(_))
                    .then(|| self.plan.bound())
                    .flatten(),
            )
            .set(rr::REPLAY_SOURCE, self.replay_source.as_str())
            .set(rr::STATUS, self.status.name())
            .set_opt(rr::STOP_REASON, self.stop_reason.as_deref())
            .set_opt(rr::STARTED_UNIX_MS, self.started_unix_ms)
            .set_opt(rr::ENDED_UNIX_MS, self.ended_unix_ms)
            .set(rr::REQUEST_ID, self.request_id.as_str());
        for v in &self.venues {
            d.push(rr::VENUES, v.name());
        }
        for (k, v) in &self.values {
            let mut e = Draft::new(&ss::setting::SCHEMA);
            e.set(ss::setting::ID, *k)
                .set(ss::setting::VALUE, v.as_str());
            d.push(rr::VALUES, e.into_nested());
        }
        if let Some(t) = &self.result {
            d.set(rr::RESULT, result_value(t));
        }
        d.encode().map_err(|e| e.to_string())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&rr::SCHEMA, bytes).map_err(|e| e.to_string())?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let mut venues = Vec::new();
        for v in r.texts(rr::VENUES).map_err(e)? {
            venues.push(VenueId::parse(v).ok_or_else(|| format!("{v} is no venue"))?);
        }
        let mut values = Vec::new();
        for row in r.nested_all(rr::VALUES).map_err(e)? {
            let id = row.text(ss::setting::ID).map_err(e)?;
            let k = settings::key(id).ok_or_else(|| format!("{id} is no Strategy C setting"))?;
            values.push((k, row.text(ss::setting::VALUE).map_err(e)?.to_string()));
        }
        if values.iter().map(|(k, _)| *k).ne(KEYS) {
            return Err("a training run holds every setting once in order".into());
        }
        let kind = word(&r, rr::STOP_KIND)?;
        let bound = r
            .opt_u64(rr::DURATION_MS)
            .map_err(e)?
            .or(r.opt_u64(rr::MAX_LAUNCHES).map_err(e)?);
        let status = word(&r, rr::STATUS)?;
        Ok(Run {
            run_id: word(&r, rr::RUN_ID)?.to_string(),
            seq: r.u64(rr::SEQ).map_err(e)?,
            profile_id: word(&r, rr::PROFILE_ID)?.to_string(),
            profile_revision: r.u64(rr::PROFILE_REVISION).map_err(e)?,
            config_sha256: r.b256(rr::CONFIG_SHA256).map_err(e)?,
            config_file_sha256: r.opt_b256(rr::CONFIG_FILE_SHA256).map_err(e)?,
            mode: Mode::parse(word(&r, rr::MODE)?)?,
            venues,
            plan: Plan::parse(kind, bound)?,
            replay_source: word(&r, rr::REPLAY_SOURCE)?.to_string(),
            status: Status::of(status).ok_or_else(|| format!("{status}: no run status"))?,
            stop_reason: r.opt_text(rr::STOP_REASON).map_err(e)?.map(str::to_string),
            started_unix_ms: r.opt_u64(rr::STARTED_UNIX_MS).map_err(e)?,
            ended_unix_ms: r.opt_u64(rr::ENDED_UNIX_MS).map_err(e)?,
            request_id: word(&r, rr::REQUEST_ID)?.to_string(),
            values,
            result: r
                .opt_nested(rr::RESULT)
                .map_err(e)?
                .map(|t| result_of(&t))
                .transpose()?,
        })
    }
}

fn result_value(t: &Training) -> Value {
    use rr::result as x;
    let c = &t.counters;
    let mut d = Draft::new(&x::SCHEMA);
    for (tag, n) in [
        (x::RECORDS, t.records),
        (x::LAUNCHES, c.launches),
        (x::PASSED, c.passed),
        (x::REJECTED, c.rejected),
        (x::DATA_INCOMPLETE, c.data_incomplete),
        (x::NOT_TRACKED, c.not_tracked),
        (x::SLOT_LIMIT, c.slot_limit),
        (x::CREATOR_SOLD, c.creator_sold),
        (x::ENTERED, c.entered),
        (x::CANCELLED, c.cancelled),
        (x::CANCELLED_CREATOR_SOLD, c.cancelled_creator_sold),
        (x::CANCELLED_NOT_TRACKED, c.cancelled_not_tracked),
        (x::CANCELLED_QUOTE, c.cancelled_quote),
        (x::CANCELLED_COST, c.cancelled_cost),
        (x::EXITS_CREATOR_SELL, c.exits_creator_sell),
        (x::EXITS_TARGET, c.exits_target),
        (x::EXITS_STOP, c.exits_stop),
        (x::EXITS_TIMEOUT, c.exits_timeout),
        (x::GAINS, c.gains),
        (x::LOSSES, c.losses),
        (x::UNRESOLVED, c.unresolved),
        (x::SUPERSEDED, c.superseded),
        (x::PAPER_UNPRICED, c.paper_unpriced),
        (x::OPEN_AT_END, t.open_at_end),
        (x::LOST_RECORDS, t.lost),
        (x::TRADES, c.trades),
        (x::TERMS, c.terms),
        (x::MIGRATIONS, c.migrations),
        (x::INCOMPLETE, c.incomplete),
        (x::PAPER_UNARMED, c.paper_unarmed),
        (x::FOLLOWED_AT_END, t.followed_at_end),
    ] {
        d.set(tag, n);
    }
    d.set(x::MANIFEST, t.manifest);
    d.set(x::GAIN_WEI, Value::U256(c.gain_wei.to_be_bytes()))
        .set(x::LOSS_WEI, Value::U256(c.loss_wei.to_be_bytes()))
        .set_opt(x::FIRST_NS, t.first_ns)
        .set_opt(x::LAST_NS, t.last_ns)
        .set_opt(x::READ_ERROR, t.read_error.as_deref())
        .set(
            x::END,
            match t.end {
                Some(TrainingEnd::Bound) => "BOUND",
                Some(TrainingEnd::Stopped) => "STOPPED",
                _ => "JOURNAL_END",
            },
        );
    d.into_nested()
}

fn result_of(r: &darkbot_bsc_record::Record) -> Result<Training, String> {
    use rr::result as x;
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let n = |tag| r.u64(tag).map_err(e);
    let wei = |tag| {
        r.u256(tag)
            .map(|b| alloy_primitives::U256::from_be_bytes(b))
            .map_err(e)
    };
    let mut t = Training {
        records: n(x::RECORDS)?,
        open_at_end: n(x::OPEN_AT_END)?,
        followed_at_end: n(x::FOLLOWED_AT_END)?,
        manifest: r.bool(x::MANIFEST).map_err(e)?,
        lost: n(x::LOST_RECORDS)?,
        read_error: r.opt_text(x::READ_ERROR).map_err(e)?.map(str::to_string),
        first_ns: r.opt_u64(x::FIRST_NS).map_err(e)?,
        last_ns: r.opt_u64(x::LAST_NS).map_err(e)?,
        end: Some(match word(r, x::END)? {
            "BOUND" => TrainingEnd::Bound,
            "STOPPED" => TrainingEnd::Stopped,
            "JOURNAL_END" => TrainingEnd::JournalEnd,
            other => return Err(format!("{other}: no training end")),
        }),
        ..Training::default()
    };
    let c = &mut t.counters;
    for (tag, field) in [
        (x::LAUNCHES, &mut c.launches),
        (x::PASSED, &mut c.passed),
        (x::REJECTED, &mut c.rejected),
        (x::DATA_INCOMPLETE, &mut c.data_incomplete),
        (x::NOT_TRACKED, &mut c.not_tracked),
        (x::SLOT_LIMIT, &mut c.slot_limit),
        (x::CREATOR_SOLD, &mut c.creator_sold),
        (x::ENTERED, &mut c.entered),
        (x::CANCELLED, &mut c.cancelled),
        (x::CANCELLED_CREATOR_SOLD, &mut c.cancelled_creator_sold),
        (x::CANCELLED_NOT_TRACKED, &mut c.cancelled_not_tracked),
        (x::CANCELLED_QUOTE, &mut c.cancelled_quote),
        (x::CANCELLED_COST, &mut c.cancelled_cost),
        (x::EXITS_CREATOR_SELL, &mut c.exits_creator_sell),
        (x::EXITS_TARGET, &mut c.exits_target),
        (x::EXITS_STOP, &mut c.exits_stop),
        (x::EXITS_TIMEOUT, &mut c.exits_timeout),
        (x::GAINS, &mut c.gains),
        (x::LOSSES, &mut c.losses),
        (x::UNRESOLVED, &mut c.unresolved),
        (x::SUPERSEDED, &mut c.superseded),
        (x::PAPER_UNPRICED, &mut c.paper_unpriced),
        (x::TRADES, &mut c.trades),
        (x::TERMS, &mut c.terms),
        (x::MIGRATIONS, &mut c.migrations),
        (x::INCOMPLETE, &mut c.incomplete),
        (x::PAPER_UNARMED, &mut c.paper_unarmed),
    ] {
        *field = n(tag)?;
    }
    c.gain_wei = wei(x::GAIN_WEI)?;
    c.loss_wei = wei(x::LOSS_WEI)?;
    Ok(t)
}

/// One recorded runtime run a REPLAY can take (its manifest's view; the
/// running runtime's own run included, read to where it is).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplaySource {
    /// The run's ID (its start, unix ms): the catalogue's source ID.
    pub run_id: String,
    pub started_unix_ms: Option<u64>,
    pub ended_unix_ms: Option<u64>,
    pub records: Option<u64>,
    pub lost_records: Option<u64>,
    pub config_sha256: Option<[u8; 32]>,
    /// Without a manifest (a run still recording, or one cut off): its
    /// counts are unknown until it is read.
    pub manifest: bool,
}

/// The recorded runtime runs under `<rundir>/bsc-runtime/journal`, newest
/// first.
pub fn replay_sources(rundir: &Path) -> Result<Vec<ReplaySource>, String> {
    use darkbot_bsc_record::schema::bsc_capture::capture_run as cr;
    let journal = rundir.join("bsc-runtime").join("journal");
    let entries = match std::fs::read_dir(&journal) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", journal.display())),
        Ok(entries) => entries,
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir()
            || name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let manifest = std::fs::read(entry.path().join(crate::runtime_journal::RUN_MANIFEST))
            .ok()
            .and_then(|b| cr::decode_run(&b).ok());
        let m = manifest.as_ref();
        out.push(ReplaySource {
            started_unix_ms: m
                .and_then(|m| m.u64(cr::STARTED_UNIX_MS).ok())
                .or_else(|| name.parse().ok()),
            ended_unix_ms: m.and_then(|m| m.opt_u64(cr::ENDED_UNIX_MS).ok().flatten()),
            records: m.and_then(|m| m.u64(cr::RECORDS).ok()),
            lost_records: m.and_then(|m| m.u64(cr::LOST_RECORDS).ok()),
            config_sha256: m.and_then(|m| m.opt_b256(cr::CONFIG_SHA256).ok().flatten()),
            manifest: m.is_some(),
            run_id: name,
        });
    }
    out.sort_by(|a, b| {
        b.run_id
            .len()
            .cmp(&a.run_id.len())
            .then(b.run_id.cmp(&a.run_id))
    });
    Ok(out)
}

fn state_path(dir: &Path, run_id: &str, seq: u64) -> PathBuf {
    dir.join("runs")
        .join(run_id)
        .join(format!("{seq:020}.dbrec"))
}

fn stop_path(dir: &Path, run_id: &str) -> PathBuf {
    dir.join("runs").join(run_id).join("stop.dbrec")
}

fn run_number(id: &str) -> Option<u64> {
    id.strip_prefix('t')
        .filter(|n| n.len() == 6)
        .and_then(|n| n.parse().ok())
}

/// The run IDs on file, oldest first.
fn run_ids(dir: &Path) -> Result<Vec<String>, String> {
    let mut ids: Vec<(u64, String)> = Vec::new();
    match std::fs::read_dir(dir.join("runs")) {
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
        Ok(entries) => {
            for entry in entries {
                let name = entry.map_err(|e| e.to_string())?.file_name();
                let name = name.to_string_lossy().to_string();
                if let Some(n) = run_number(&name) {
                    ids.push((n, name));
                }
            }
        }
    }
    ids.sort();
    Ok(ids.into_iter().map(|(_, id)| id).collect())
}

/// The newest state of `run_id` (its highest number, read as numbered).
fn latest(dir: &Path, owner: u32, run_id: &str) -> Result<Option<Run>, String> {
    let mut highest: Option<u64> = None;
    let rd = match std::fs::read_dir(dir.join("runs").join(run_id)) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(rd) => rd,
    };
    for entry in rd {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        if let Some(n) = name
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        {
            highest = highest.max(Some(n));
        }
    }
    let Some(n) = highest else {
        return Ok(None);
    };
    let path = state_path(dir, run_id, n);
    let bytes = read_private(&path, owner).map_err(|e| format!("{}: {e}", path.display()))?;
    let run = Run::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    match run.seq == n && run.run_id == run_id {
        true => Ok(Some(run)),
        false => Err(format!(
            "{} holds {} state {}",
            path.display(),
            run.run_id,
            run.seq
        )),
    }
}

/// Every run's newest state under the source-mode owner's directory
/// `sources`, newest first, without creating anything; a store another
/// user can change is refused.
pub fn runs(sources: &Path) -> Result<Vec<Run>, String> {
    let dir = sources.join(DIR);
    match std::fs::symlink_metadata(dir.join("runs")) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let owner = private_directory(&dir).map_err(|r| r.to_string())?;
    let mut out = Vec::new();
    for id in run_ids(&dir)?.iter().rev() {
        out.extend(latest(&dir, owner, id)?);
    }
    Ok(out)
}

/// Why `profile` is in use by a training run (a run of it still active).
pub fn in_use(sources: &Path, profile: &profiles::Profile) -> Result<Option<String>, String> {
    Ok(runs(sources)?
        .into_iter()
        .find(|r| r.status.active() && r.profile_id == profile.profile_id)
        .map(|r| format!("training run {} is {}", r.run_id, r.status.name())))
}

/// A start request (`bsc.training.run.start`): the profile at the expected
/// revision, its mode, venues (a subset of the profile's), bound and the
/// recorded run it replays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    pub profile_id: String,
    pub mode: String,
    pub venues: Vec<String>,
    pub stop_kind: String,
    pub bound: Option<u64>,
    pub replay_source: String,
}

impl Start {
    fn sha256(&self, expected: u64) -> [u8; 32] {
        let text = format!(
            "action=START\nprofile_id={}\nrevision={expected}\nmode={}\nvenues={}\nstop_kind={}\nbound={}\nreplay_source={}\n",
            self.profile_id,
            self.mode,
            self.venues.join(","),
            self.stop_kind,
            self.bound.map_or_else(String::new, |b| b.to_string()),
            self.replay_source
        );
        darkbot_bsc_record::sha256(text.as_bytes())
    }
}

/// The outcome of one training request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub request_id: String,
    /// `START` or `STOP`.
    pub action: &'static str,
    pub request_sha256: [u8; 32],
    pub expected: u64,
    /// `APPLIED`, `REJECTED_STALE`, `REJECTED_INVALID`, `REJECTED_NOT_FOUND`,
    /// `REJECTED_ACTIVE` or `REJECTED_ENDED`.
    pub outcome: &'static str,
    pub run_id: Option<String>,
    pub reason: Option<String>,
}

/// `PENDING`: a request's intent (kept before any state it writes).
const OUTCOMES: [&str; 7] = [
    "PENDING",
    "APPLIED",
    "REJECTED_STALE",
    "REJECTED_INVALID",
    "REJECTED_NOT_FOUND",
    "REJECTED_ACTIVE",
    "REJECTED_ENDED",
];

impl Receipt {
    pub fn applied(&self) -> bool {
        self.outcome == "APPLIED"
    }

    /// One line for the controller and the gateway's receipt detail.
    pub fn line(&self) -> String {
        format!(
            "outcome={} action={} run_id={} request_id={}{}",
            self.outcome,
            self.action,
            self.run_id.as_deref().unwrap_or("-"),
            self.request_id,
            self.reason
                .as_deref()
                .map(|r| format!(" reason={r}"))
                .unwrap_or_default()
        )
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&tr::SCHEMA);
        d.set(tr::REQUEST_ID, self.request_id.as_str())
            .set(tr::ACTION, self.action)
            .set(tr::REQUEST_SHA256, Value::B256(self.request_sha256))
            .set(tr::EXPECTED, self.expected)
            .set(tr::OUTCOME, self.outcome)
            .set_opt(tr::RUN_ID, self.run_id.as_deref())
            .set_opt(tr::REASON, self.reason.as_deref())
            .set(tr::OBSERVED_UNIX_MS, observed_unix_ms);
        d.encode().map_err(|e| e.to_string())
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&tr::SCHEMA, bytes).map_err(|e| e.to_string())?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let outcome = word(&r, tr::OUTCOME)?;
        Ok(Receipt {
            request_id: word(&r, tr::REQUEST_ID)?.to_string(),
            action: match word(&r, tr::ACTION)? {
                "START" => "START",
                "STOP" => "STOP",
                other => return Err(format!("{other}: no training action")),
            },
            request_sha256: r.b256(tr::REQUEST_SHA256).map_err(e)?,
            expected: r.u64(tr::EXPECTED).map_err(e)?,
            outcome: OUTCOMES
                .iter()
                .copied()
                .find(|o| *o == outcome)
                .ok_or_else(|| format!("{outcome}: unknown training outcome"))?,
            run_id: r.opt_text(tr::RUN_ID).map_err(e)?.map(str::to_string),
            reason: r.opt_text(tr::REASON).map_err(e)?.map(str::to_string),
        })
    }
}

/// The training store, opened for requests and for a run's job.
pub struct Owner {
    sources: PathBuf,
    dir: PathBuf,
    owner: u32,
}

/// Whether the controller sees the training job running (its launchd
/// program has a process): an active run without one is left behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    Running,
    None,
}

impl Owner {
    /// Opens (creating, owner-only) `<sources>/training`.
    pub fn open(sources: &Path) -> Result<Self, String> {
        let dir = sources.join(DIR);
        let mut owner = None;
        for d in [
            dir.clone(),
            dir.join("runs"),
            dir.join("intents"),
            dir.join("receipts"),
        ] {
            create_durable_dir(&d)?;
            if !std::fs::symlink_metadata(&d).is_ok_and(|m| m.is_dir()) {
                return Err(format!("{} is not a real directory", d.display()));
            }
            let o = private_directory(&d).map_err(|r| r.to_string())?;
            if owner.is_some_and(|x| x != o) {
                return Err(format!("{} has another owner", d.display()));
            }
            owner = Some(o);
        }
        Ok(Owner {
            sources: sources.to_path_buf(),
            dir,
            owner: owner.ok_or("no store directory")?,
        })
    }

    pub fn run(&self, run_id: &str) -> Result<Option<Run>, String> {
        latest(&self.dir, self.owner, run_id)
    }

    /// The newest run with a state (a run directory a crash left without
    /// one is no run): only it may be active.
    fn newest(&self) -> Result<Option<Run>, String> {
        for id in run_ids(&self.dir)?.iter().rev() {
            if let Some(r) = latest(&self.dir, self.owner, id)? {
                return Ok(Some(r));
            }
        }
        Ok(None)
    }

    /// The request record (`receipts` or `intents`) of `request_id` if it
    /// has one: this request's (the same payload) or a refusal of another
    /// payload under its ID.
    fn record_of(
        &self,
        place: &str,
        request_id: &str,
        sha: &[u8; 32],
        expected: u64,
    ) -> Result<Option<Receipt>, String> {
        let path = self.dir.join(place).join(format!("{request_id}.dbrec"));
        match read_private(&path, self.owner) {
            Ok(bytes) => {
                let r = Receipt::decode(&bytes)?;
                match r.request_sha256 == *sha && r.expected == expected {
                    true => Ok(Some(r)),
                    false => Err("the request ID already belongs to another request".into()),
                }
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{place} unreadable: {e}")),
        }
    }

    /// Review M1 (as the profiles'): the request's intent, kept before any
    /// state it writes, so its ID stays this request's even if its receipt
    /// is lost. Whether one was kept before (an earlier attempt).
    fn intent(&self, receipt: &Receipt, now_ms: u64) -> Result<bool, String> {
        let path = self
            .dir
            .join("intents")
            .join(format!("{}.dbrec", receipt.request_id));
        let mut pending = receipt.clone();
        pending.outcome = "PENDING";
        match write_new(&path, &pending.encode(now_ms)?) {
            Ok(()) => Ok(false),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => self
                .record_of(
                    "intents",
                    &receipt.request_id,
                    &receipt.request_sha256,
                    receipt.expected,
                )
                .map(|_| true),
            Err(e) => Err(format!("intent not written: {e}")),
        }
    }

    fn keep(&self, receipt: Receipt, now_ms: u64) -> Result<Receipt, String> {
        let path = self
            .dir
            .join("receipts")
            .join(format!("{}.dbrec", receipt.request_id));
        match write_new(&path, &receipt.encode(now_ms)?) {
            Ok(()) => Ok(receipt),
            // A concurrent identical request wrote it first: its receipt counts.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => self
                .record_of(
                    "receipts",
                    &receipt.request_id,
                    &receipt.request_sha256,
                    receipt.expected,
                )?
                .ok_or_else(|| "receipt vanished".into()),
            Err(e) => Err(format!("receipt not written: {e}")),
        }
    }

    fn write(&self, run: &Run, now_ms: u64) -> std::io::Result<()> {
        let bytes = run
            .encode(now_ms)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;
        write_new(&state_path(&self.dir, &run.run_id, run.seq), &bytes)
    }

    /// Starts a run of `start.profile_id` at revision `expected` (a PENDING
    /// state its job takes up) if no run is active, the profile is ACTIVE
    /// at that revision, the venues are its, the replay source is in the
    /// catalogue of `rundir` and the runtime configuration `config_text`
    /// takes its values. An active run of another request whose job the
    /// controller sees gone (`job`) ends first (review M2: never this
    /// request's own run, which a retry gets back). Starts are serialized
    /// by the owner's lock (review L1). Every request leaves a receipt.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        request_id: &str,
        start: &Start,
        expected: u64,
        job: Job,
        rundir: &Path,
        config_text: &str,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if !safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        let mut venues: Vec<VenueId> = Vec::new();
        for n in &start.venues {
            let v = VenueId::parse(n).ok_or_else(|| format!("{n} is no venue"))?;
            if venues.contains(&v) {
                return Err(format!("{n} is given twice"));
            }
            venues.push(v);
        }
        venues.sort_by_key(|v| v.index());
        if venues.is_empty() {
            return Err("a training run trains on at least one venue".into());
        }
        let plan = Plan::parse(&start.stop_kind, start.bound)?;
        if !safe_id(&start.profile_id) || !safe_id(&start.replay_source) {
            return Err("the profile and the replay source are safe identifiers".into());
        }
        let sha = start.sha256(expected);
        if let Some(r) = self.record_of("receipts", request_id, &sha, expected)? {
            return Ok(r);
        }
        let receipt = Receipt {
            request_id: request_id.into(),
            action: "START",
            request_sha256: sha,
            expected,
            outcome: "REJECTED_INVALID",
            run_id: None,
            reason: None,
        };
        let lock = std::fs::File::create(self.dir.join("start.lock"))
            .and_then(|f| f.lock().map(|()| f))
            .map_err(|e| format!("the training owner's lock: {e}"))?;
        let earlier = self.intent(&receipt, now_ms)?;
        let refuse = |mut r: Receipt, outcome, why: String| {
            r.outcome = outcome;
            r.reason = Some(settings::bounded(why, tr::SCHEMA.field(tr::REASON)));
            r
        };
        // Crash recovery: a run this request started (its receipt lost)
        // names it.
        if earlier
            && let Some(run) = runs(&self.sources)?
                .into_iter()
                .find(|r| r.request_id == request_id)
        {
            let mut r = receipt;
            r.outcome = "APPLIED";
            r.run_id = Some(run.run_id);
            drop(lock);
            return self.keep(r, now_ms);
        }
        let decided = (|| -> Result<Receipt, String> {
            let mode = match Mode::parse(&start.mode) {
                Ok(m) => m,
                Err(why) => return Ok(refuse(receipt.clone(), "REJECTED_INVALID", why)),
            };
            if let Some(r) = self.newest()?.filter(|r| r.status.active()) {
                if job == Job::Running {
                    let why = format!("training run {} is {}", r.run_id, r.status.name());
                    return Ok(refuse(receipt.clone(), "REJECTED_ACTIVE", why));
                }
                self.end_orphan(&r.run_id, "its job was gone at the next start", now_ms)?;
            }
            let (catalog, _) = profiles::current_file(&self.sources)?;
            let profile = match catalog
                .profile(&start.profile_id)
                .filter(|p| p.status != ProfileStatus::Deleted)
            {
                None => {
                    let why = format!("no profile {}", start.profile_id);
                    return Ok(refuse(receipt.clone(), "REJECTED_NOT_FOUND", why));
                }
                Some(p) if p.status == ProfileStatus::Archived => {
                    let why = format!("{} is archived", p.profile_id);
                    return Ok(refuse(receipt.clone(), "REJECTED_INVALID", why));
                }
                Some(p) if p.revision != expected => {
                    let why = format!("{} is at revision {}", p.profile_id, p.revision);
                    return Ok(refuse(receipt.clone(), "REJECTED_STALE", why));
                }
                Some(p) => p.clone(),
            };
            if let Some(v) = venues.iter().find(|v| !profile.venues.contains(v)) {
                let why = format!("{} is not a venue of {}", v.name(), profile.profile_id);
                return Ok(refuse(receipt.clone(), "REJECTED_INVALID", why));
            }
            if !replay_sources(rundir)?
                .iter()
                .any(|s| s.run_id == start.replay_source)
            {
                let why = format!("no recorded run {}", start.replay_source);
                return Ok(refuse(receipt.clone(), "REJECTED_NOT_FOUND", why));
            }
            if let Err(why) = settings::effective(config_text, &profile.settings()) {
                return Ok(refuse(receipt.clone(), "REJECTED_INVALID", why));
            }
            // The next number after every run directory (a stateless one a
            // crash left is skipped, never reused).
            let n = run_ids(&self.dir)?
                .last()
                .and_then(|id| run_number(id))
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("run IDs exhausted")?;
            let run_id = format!("t{n:06}");
            create_durable_dir(&self.dir.join("runs").join(&run_id))?;
            let run = Run {
                run_id: run_id.clone(),
                seq: 1,
                profile_id: profile.profile_id.clone(),
                profile_revision: profile.revision,
                config_sha256: profile.config_sha256(),
                config_file_sha256: None,
                mode,
                venues: venues.clone(),
                plan,
                replay_source: start.replay_source.clone(),
                status: Status::Pending,
                stop_reason: None,
                started_unix_ms: None,
                ended_unix_ms: None,
                request_id: request_id.into(),
                values: profile.values.clone(),
                result: None,
            };
            self.write(&run, now_ms)
                .map_err(|e| format!("{run_id}: state not written: {e}"))?;
            let mut r = receipt.clone();
            r.outcome = "APPLIED";
            r.run_id = Some(run_id);
            Ok(r)
        })()?;
        drop(lock);
        self.keep(decided, now_ms)
    }

    /// Asks `run_id` to stop: its stop request kept for its job (which polls
    /// it, and the controller signals); a run still PENDING is STOPPED at
    /// once. Review M1: an intent first, and a lost receipt recovered from
    /// the stop request that names this request.
    pub fn stop(
        &self,
        request_id: &str,
        run_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if !safe_id(request_id) || !safe_id(run_id) {
            return Err("the request and the run are safe identifiers".into());
        }
        if reason.chars().any(char::is_control) {
            return Err("the reason has a control character".into());
        }
        let sha = darkbot_bsc_record::sha256(
            format!("action=STOP\nrun_id={run_id}\nreason={reason}\n").as_bytes(),
        );
        if let Some(r) = self.record_of("receipts", request_id, &sha, 0)? {
            return Ok(r);
        }
        let mut receipt = Receipt {
            request_id: request_id.into(),
            action: "STOP",
            request_sha256: sha,
            expected: 0,
            outcome: "APPLIED",
            run_id: Some(run_id.into()),
            reason: (!reason.is_empty())
                .then(|| settings::bounded(reason.to_string(), tr::SCHEMA.field(tr::REASON))),
        };
        let earlier = self.intent(&receipt, now_ms)?;
        let asked = read_private(&stop_path(&self.dir, run_id), self.owner)
            .ok()
            .and_then(|b| Receipt::decode(&b).ok());
        if earlier
            && asked
                .as_ref()
                .is_some_and(|a| a.request_id == request_id && a.request_sha256 == sha)
        {
            return self.keep(receipt, now_ms);
        }
        let mut run = match self.run(run_id)? {
            None => {
                receipt.outcome = "REJECTED_NOT_FOUND";
                return self.keep(receipt, now_ms);
            }
            Some(r) => r,
        };
        if !run.status.active() {
            receipt.outcome = "REJECTED_ENDED";
            receipt.reason = Some(format!("{run_id} is {}", run.status.name()));
            return self.keep(receipt, now_ms);
        }
        match write_new(&stop_path(&self.dir, run_id), &receipt.encode(now_ms)?) {
            Ok(()) => {}
            // A stop asked before stands.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("stop request not written: {e}")),
        }
        // PENDING: no job runs it yet; it ends here (a job that took it up
        // meanwhile wrote its RUNNING first and sees the request).
        while run.status == Status::Pending {
            let mut stopped = run.next(Status::Stopped);
            stopped.stop_reason = Some(self.stop_reason_of(run_id));
            stopped.ended_unix_ms = Some(now_ms);
            match self.write(&stopped, now_ms) {
                Ok(()) => break,
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    run = self.run(run_id)?.ok_or("the run vanished")?;
                }
                Err(e) => return Err(format!("{run_id}: state not written: {e}")),
            }
        }
        self.keep(receipt, now_ms)
    }

    /// Ends a run whose job is gone (the controller's view: no job of it):
    /// a PENDING one FAILED (its job never started), a RUNNING one
    /// INTERRUPTED. An ended run stays as it is.
    pub fn end_orphan(
        &self,
        run_id: &str,
        reason: &str,
        now_ms: u64,
    ) -> Result<Option<Run>, String> {
        loop {
            let Some(run) = self.run(run_id)? else {
                return Ok(None);
            };
            let status = match run.status {
                Status::Pending => Status::Failed,
                Status::Running => Status::Interrupted,
                _ => return Ok(Some(run)),
            };
            let mut end = run.next(status);
            end.stop_reason = Some(bounded_reason(reason.to_string()));
            end.ended_unix_ms = Some(now_ms);
            match self.write(&end, now_ms) {
                Ok(()) => return Ok(Some(end)),
                // Its job wrote first: decide again on its state.
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("{run_id}: state not written: {e}")),
            }
        }
    }

    /// [`Owner::end_orphan`] of the newest run (only it can be active).
    pub fn end_orphans(&self, reason: &str, now_ms: u64) -> Result<Option<Run>, String> {
        match self.newest()? {
            Some(r) => self.end_orphan(&r.run_id, reason, now_ms),
            None => Ok(None),
        }
    }

    /// The run's job: takes up its PENDING state (RUNNING), trains the
    /// profile's values on the recorded run until its bound, its end, a
    /// stop request (polled every `BSC_TRAINING_STOP_POLL_MS`) or `signal`
    /// (the job's SIGTERM), and writes its end with the result. A run found
    /// RUNNING was cut off before (the job restarted): INTERRUPTED; so is
    /// one signalled without a stop request (review L2).
    pub fn execute(
        &self,
        run_id: &str,
        root: &Path,
        rundir: &Path,
        config_text: &str,
        signal: &std::sync::atomic::AtomicBool,
        now: &dyn Fn() -> u64,
    ) -> Result<Run, String> {
        use std::sync::atomic::Ordering;
        let parsed = crate::runtime::RuntimeConfig::parse(config_text);
        let mut run = self
            .run(run_id)?
            .ok_or_else(|| format!("no training run {run_id}"))?;
        loop {
            let next = match run.status {
                Status::Pending if stop_path(&self.dir, run_id).exists() => {
                    let mut s = run.next(Status::Stopped);
                    s.stop_reason = Some(self.stop_reason_of(run_id));
                    s.ended_unix_ms = Some(now());
                    s
                }
                Status::Pending => {
                    let mut s = run.next(Status::Running);
                    s.started_unix_ms = Some(now());
                    // Review L4: the configuration file it runs on.
                    s.config_file_sha256 = parsed.as_ref().ok().map(|c| c.source_sha256);
                    s
                }
                Status::Running => {
                    let mut s = run.next(Status::Interrupted);
                    s.stop_reason = Some("its job restarted: the run was cut off".into());
                    s.ended_unix_ms = Some(now());
                    s
                }
                _ => return Ok(run),
            };
            match self.write(&next, now()) {
                Ok(()) if next.status == Status::Running => {
                    run = next;
                    break;
                }
                Ok(()) => return Ok(next),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    run = self.run(run_id)?.ok_or("the run vanished")?;
                }
                Err(e) => return Err(format!("{run_id}: state not written: {e}")),
            }
        }
        let end_with = |run: &Run, status: Status, why: String, t: Option<Training>| {
            let mut f = run.next(status);
            f.stop_reason = Some(bounded_reason(why));
            f.ended_unix_ms = Some(now());
            f.result = t;
            match self.write(&f, now()) {
                Ok(()) => Ok(f),
                // A stop or the controller ended it meanwhile (a PENDING
                // stop raced): its state stands.
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    self.run(run_id)?.ok_or_else(|| "the run vanished".into())
                }
                Err(e) => Err(format!("{run_id}: state not written: {e}")),
            }
        };
        let config = match settings::effective(
            config_text,
            &settings::Settings {
                generation: run.profile_revision,
                request_id: Some(run.request_id.clone()),
                values: run.values.clone(),
            },
        ) {
            Ok(c) => c,
            Err(why) => {
                let why = format!("the runtime configuration refuses the profile: {why}");
                return end_with(&run, Status::Failed, why, None);
            }
        };
        let gas = match config.strategy.gas(root) {
            Ok(g) => g,
            Err(why) => return end_with(&run, Status::Failed, why, None),
        };
        let venues = crate::runtime_strategy::StrategyVenues {
            flap: run.venues.contains(&VenueId::Flap),
            four: run.venues.contains(&VenueId::FourClassic),
            openfour: run.venues.contains(&VenueId::OpenFour),
        };
        let (plan, stop_file) = (run.plan, stop_path(&self.dir, run_id));
        let poll = std::time::Duration::from_millis(config.training_stop_poll_ms);
        let mut polled = std::time::Instant::now();
        let mut asked = false;
        let mut step = |p: &crate::runtime_strategy::TrainingProgress| {
            if !asked && polled.elapsed() >= poll {
                polled = std::time::Instant::now();
                asked = stop_file.exists();
            }
            if asked || signal.load(Ordering::Relaxed) {
                return TrainingStep::Stop;
            }
            match plan {
                Plan::Duration(ms) if p.elapsed_ns >= ms.saturating_mul(1_000_000) => {
                    TrainingStep::Bounded
                }
                // Each launch taken is decided once (the run ends only when
                // every one is): the decided count is the bound (review M4).
                Plan::Launches(n) if p.launches >= n => TrainingStep::Bounded,
                _ => TrainingStep::Continue,
            }
        };
        let trained = train(
            &rundir.join("bsc-runtime").join("journal"),
            &run.replay_source,
            crate::runtime::journal_max_payload(&config),
            &config.strategy.engine,
            &gas,
            venues,
            &mut step,
        );
        let t = match trained {
            Ok(t) if t.records == 0 && t.read_error.is_some() => {
                let why = t.read_error.clone().unwrap_or_default();
                let why = format!("the recorded run does not read: {why}");
                return end_with(&run, Status::Failed, why, None);
            }
            Ok(t) => t,
            Err(why) => return end_with(&run, Status::Failed, why, None),
        };
        // Review M3: why it ended, in the proto's words.
        let (status, why) = match (t.end, plan) {
            (Some(TrainingEnd::Stopped), _) if stop_file.exists() => {
                (Status::Stopped, self.stop_reason_of(run_id))
            }
            (Some(TrainingEnd::Stopped), _) => (
                Status::Interrupted,
                "its job was signalled without a stop request".to_string(),
            ),
            (Some(TrainingEnd::Bound), Plan::Duration(_)) => {
                (Status::Completed, "PLAN_DURATION".into())
            }
            (Some(TrainingEnd::Bound), Plan::Launches(_)) => {
                (Status::Completed, "PLAN_LAUNCH_COUNT".into())
            }
            _ => (Status::Completed, "JOURNAL_END".into()),
        };
        end_with(&run, status, why, Some(t))
    }

    /// The stop request's reason, in the proto's words.
    fn stop_reason_of(&self, run_id: &str) -> String {
        let reason = read_private(&stop_path(&self.dir, run_id), self.owner)
            .ok()
            .and_then(|b| Receipt::decode(&b).ok())
            .and_then(|r| r.reason)
            .unwrap_or_default();
        bounded_reason(match reason.is_empty() {
            true => "STOP_REQUESTED".into(),
            false => format!("STOP_REQUESTED: {reason}"),
        })
    }
}

/// A reason within the record's bound (review L3).
fn bounded_reason(reason: String) -> String {
    settings::bounded(reason, rr::SCHEMA.field(rr::STOP_REASON))
}

/// A `bsc_runtime training <cmd> …` command line, each flag's value from
/// its own pair (as the profiles').
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Start {
        start: Start,
        expected: u64,
        /// `--job RUNNING|NONE`: whether the controller sees the job.
        job: Job,
        request_id: String,
    },
    Stop {
        run_id: String,
        reason: String,
        request_id: String,
    },
    /// The run's job.
    Execute { run_id: String },
    /// The controller found no job running: the newest run, if active,
    /// ends (PENDING FAILED, RUNNING INTERRUPTED).
    EndOrphans { reason: String },
}

/// The places every training command names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    pub state: String,
    pub rundir: String,
    pub config: String,
}

/// Parses `bsc_runtime training <cmd> <flags>`: each allowed flag exactly
/// once with its value (`--bound` only for a bounded plan).
pub fn command(cmd: &str, rest: &[String]) -> Result<(Command, Places), String> {
    let usage = || format!("training {cmd}: see bsc_runtime's usage");
    let mut allowed = vec!["--state", "--rundir", "--config"];
    let optional: &[&str] = match cmd {
        "start" => {
            allowed.extend([
                "--profile",
                "--expect",
                "--mode",
                "--venues",
                "--stop-kind",
                "--replay-source",
                "--request-id",
                "--job",
            ]);
            &["--bound"]
        }
        "stop" => {
            allowed.extend(["--run", "--reason", "--request-id"]);
            &[]
        }
        "execute" => {
            allowed.push("--run");
            &[]
        }
        "end-orphans" => {
            allowed.push("--reason");
            &[]
        }
        _ => return Err(usage()),
    };
    if !rest.len().is_multiple_of(2) {
        return Err(usage());
    }
    let mut set: Vec<(&str, &str)> = Vec::new();
    for f in rest.chunks(2) {
        let (k, v) = (f[0].as_str(), f[1].as_str());
        if !(allowed.contains(&k) || optional.contains(&k)) || set.iter().any(|(s, _)| *s == k) {
            return Err(usage());
        }
        set.push((k, v));
    }
    if allowed.iter().any(|a| !set.iter().any(|(s, _)| s == a)) {
        return Err(usage());
    }
    let get = |name: &str| {
        set.iter()
            .find(|(s, _)| *s == name)
            .map(|(_, v)| v.to_string())
    };
    let req = |name: &str| get(name).unwrap_or_default();
    let number = |text: String, name: &str| {
        text.parse::<u64>()
            .map_err(|_| format!("{name} is an unsigned integer"))
    };
    let places = Places {
        state: req("--state"),
        rundir: req("--rundir"),
        config: req("--config"),
    };
    let command = match cmd {
        "start" => Command::Start {
            start: Start {
                profile_id: req("--profile"),
                mode: req("--mode"),
                venues: req("--venues").split(',').map(String::from).collect(),
                stop_kind: req("--stop-kind"),
                bound: get("--bound").map(|b| number(b, "--bound")).transpose()?,
                replay_source: req("--replay-source"),
            },
            expected: number(req("--expect"), "--expect")?,
            job: match req("--job").as_str() {
                "RUNNING" => Job::Running,
                "NONE" => Job::None,
                other => return Err(format!("--job {other}: RUNNING or NONE")),
            },
            request_id: req("--request-id"),
        },
        "stop" => Command::Stop {
            run_id: req("--run"),
            reason: req("--reason"),
            request_id: req("--request-id"),
        },
        "execute" => Command::Execute {
            run_id: req("--run"),
        },
        _ => Command::EndOrphans {
            reason: req("--reason"),
        },
    };
    Ok((command, places))
}

#[cfg(test)]
#[path = "training_runs_tests.rs"]
mod tests;
