//! BSC-V8-01: the native owner of the Strategy C profiles: named, versioned
//! Strategy C configurations (every settable key of the settings owner,
//! [`settings::KEYS`], and the venues it trains on) for training runs and the
//! LIVE selection. One strategy, many profiles: a profile is checked by the
//! runtime configuration's own parser with its values in place, as a
//! settings generation is, and never changes what runs by itself.
//!
//! State `<sources>/strategy-profiles/`: `catalog/<n>.dbrec`, the whole
//! catalog at version `n` (every profile at its revision then), created
//! create-new and every version kept: a stale request always resolves, every
//! profile revision reads back (a clone of an old one, a run's). A deleted
//! profile stays as such with its values. `receipts/<request_id>.dbrec` per
//! request, with its payload's hash: the same request again gets its
//! receipt, another payload under its ID is refused. A crash between a
//! catalog version and its receipt is recovered from the version, which
//! names its request and payload hash.
//!
//! The compare-and-swap is the catalog version for a create or a clone and
//! the profile's revision for an update, archive or delete (two operators
//! editing two profiles both succeed; one profile, the second is stale).

use crate::runtime::RuntimeConfig;
use crate::sources::{
    VenueId, create_durable_dir, private_directory, read_private, safe_id, write_new,
};
use crate::strategy_settings::{self as settings, KEYS, Settings};
use darkbot_bsc_record::schema::bsc_runtime::{
    strategy_profile_receipt as pr, strategy_profiles as sp, strategy_settings as ss,
};
use darkbot_bsc_record::{Draft, Value, decode};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The owner's directory under the source-mode owner's.
pub const DIR: &str = "strategy-profiles";

/// A profile's lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Active,
    /// Kept and readable, not offered for new runs.
    Archived,
    /// Gone for the operator; its values stay for the runs that used it.
    Deleted,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Active => "ACTIVE",
            Status::Archived => "ARCHIVED",
            Status::Deleted => "DELETED",
        }
    }

    fn of(name: &str) -> Option<Self> {
        [Status::Active, Status::Archived, Status::Deleted]
            .into_iter()
            .find(|s| s.name() == name)
    }
}

/// One profile at one revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub profile_id: String,
    pub name: String,
    pub revision: u64,
    pub status: Status,
    /// Each once, in [`VenueId::ALL`] order.
    pub venues: Vec<VenueId>,
    /// Every settable key with its value, in [`KEYS`] order.
    pub values: Vec<(&'static str, String)>,
    pub created_unix_ms: u64,
    pub updated_unix_ms: u64,
    /// The request that wrote this revision.
    pub request_id: String,
}

impl Profile {
    /// The configuration hash of its values (the settings owner's rule).
    pub fn config_sha256(&self) -> [u8; 32] {
        settings::config_sha256(&self.values)
    }

    /// Its values as a settings basis (what a run of it or its LIVE
    /// selection takes over the configuration file).
    pub fn settings(&self) -> Settings {
        Settings {
            generation: self.revision,
            request_id: Some(self.request_id.clone()),
            values: self.values.clone(),
        }
    }

    fn encode(&self) -> Value {
        let mut d = Draft::new(&sp::profile::SCHEMA);
        d.set(sp::profile::PROFILE_ID, self.profile_id.as_str())
            .set(sp::profile::NAME, self.name.as_str())
            .set(sp::profile::REVISION, self.revision)
            .set(sp::profile::STATUS, self.status.name())
            .set(sp::profile::CREATED_UNIX_MS, self.created_unix_ms)
            .set(sp::profile::UPDATED_UNIX_MS, self.updated_unix_ms)
            .set(sp::profile::REQUEST_ID, self.request_id.as_str());
        for v in &self.venues {
            d.push(sp::profile::VENUES, v.name());
        }
        for (k, v) in &self.values {
            let mut e = Draft::new(&ss::setting::SCHEMA);
            e.set(ss::setting::ID, *k)
                .set(ss::setting::VALUE, v.as_str());
            d.push(sp::profile::VALUES, e.into_nested());
        }
        d.into_nested()
    }

    fn decode(r: &darkbot_bsc_record::Record) -> Result<Self, String> {
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let mut venues = Vec::new();
        for v in r.texts(sp::profile::VENUES).map_err(e)? {
            venues.push(VenueId::parse(v).ok_or_else(|| format!("{v} is no venue"))?);
        }
        let mut values = Vec::new();
        for row in r.nested_all(sp::profile::VALUES).map_err(e)? {
            let id = row.text(ss::setting::ID).map_err(e)?;
            let k = settings::key(id).ok_or_else(|| format!("{id} is no Strategy C setting"))?;
            let v = row.text(ss::setting::VALUE).map_err(e)?;
            if let Some(why) = settings::value_refused(v) {
                return Err(format!("{k}: {why}"));
            }
            values.push((k, v.to_string()));
        }
        let profile_id = r.text(sp::profile::PROFILE_ID).map_err(e)?.to_string();
        if values.iter().map(|(k, _)| *k).ne(KEYS) {
            return Err(format!("{profile_id}: not every setting once in order"));
        }
        if !in_order(&venues) {
            return Err(format!(
                "{profile_id}: its venues are not each once in order"
            ));
        }
        let status = r.text(sp::profile::STATUS).map_err(e)?;
        Ok(Profile {
            name: r.text(sp::profile::NAME).map_err(e)?.to_string(),
            revision: r.u64(sp::profile::REVISION).map_err(e)?,
            status: Status::of(status).ok_or_else(|| format!("{status}: no profile status"))?,
            venues,
            values,
            created_unix_ms: r.u64(sp::profile::CREATED_UNIX_MS).map_err(e)?,
            updated_unix_ms: r.u64(sp::profile::UPDATED_UNIX_MS).map_err(e)?,
            request_id: r.text(sp::profile::REQUEST_ID).map_err(e)?.to_string(),
            profile_id,
        })
    }
}

fn in_order(venues: &[VenueId]) -> bool {
    venues.windows(2).all(|w| w[0].index() < w[1].index())
}

/// The catalog at one version.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    /// 0: none on file.
    pub version: u64,
    /// The request that wrote it and its payload's hash.
    pub request_id: Option<String>,
    pub request_sha256: Option<[u8; 32]>,
    /// In creation order.
    pub profiles: Vec<Profile>,
}

impl Catalog {
    pub fn profile(&self, profile_id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.profile_id == profile_id)
    }

    fn written_by(&self, request_id: &str, sha: &[u8; 32]) -> Result<bool, String> {
        match (self.request_id.as_deref(), &self.request_sha256) {
            (Some(r), Some(s)) if r == request_id => match s == sha {
                true => Ok(true),
                false => Err("the request ID already belongs to another request".into()),
            },
            _ => Ok(false),
        }
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&sp::SCHEMA);
        d.set(sp::VERSION, self.version)
            .set(
                sp::REQUEST_ID,
                self.request_id.as_deref().unwrap_or_default(),
            )
            .set(sp::PROVENANCE_TOOL, "bsc_runtime")
            .set(sp::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms)
            .set(
                sp::REQUEST_SHA256,
                Value::B256(self.request_sha256.unwrap_or_default()),
            );
        for p in &self.profiles {
            d.push(sp::PROFILES, p.encode());
        }
        d.encode().map_err(|e| e.to_string())
    }

    /// Strict: every profile whole, each ID once.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&sp::SCHEMA, bytes).map_err(|e| e.to_string())?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let mut profiles: Vec<Profile> = Vec::new();
        for row in r.nested_all(sp::PROFILES).map_err(e)? {
            let p = Profile::decode(&row)?;
            if profiles.iter().any(|q| q.profile_id == p.profile_id) {
                return Err(format!("{} twice in one catalog", p.profile_id));
            }
            profiles.push(p);
        }
        let request_id = r.text(sp::REQUEST_ID).map_err(e)?;
        Ok(Catalog {
            version: r.u64(sp::VERSION).map_err(e)?,
            request_id: (!request_id.is_empty()).then(|| request_id.to_string()),
            request_sha256: Some(r.b256(sp::REQUEST_SHA256).map_err(e)?),
            profiles,
        })
    }
}

/// One profile request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// A new profile (the CAS: the catalog version).
    Create {
        name: String,
        venues: Vec<String>,
        values: Vec<(String, String)>,
    },
    /// A new revision of `profile_id` (the CAS: its revision).
    Update {
        profile_id: String,
        name: String,
        venues: Vec<String>,
        values: Vec<(String, String)>,
    },
    /// A new profile from one revision of another (the CAS: the catalog
    /// version).
    Clone {
        source_profile_id: String,
        source_revision: u64,
        name: String,
    },
    /// Archives or deletes `profile_id` (the CAS: its revision); refused
    /// while it is in use.
    Delete { profile_id: String, archive: bool },
}

/// A request checked for its form: the name, the venues and the values as
/// the owner stores them.
struct Checked {
    action: &'static str,
    name: Option<String>,
    venues: Vec<VenueId>,
    values: Vec<(&'static str, String)>,
    sha256: [u8; 32],
}

/// Why `name` cannot be a profile's: printable, not blank, within the
/// record's bound, no surrounding space.
fn name_refused(name: &str) -> Option<String> {
    let max = sp::profile::SCHEMA
        .field(sp::profile::NAME)
        .map_or(0, |f| f.max_len as usize);
    if name.trim().is_empty() || name.trim() != name {
        Some("the name is blank or has surrounding space".into())
    } else if name.chars().any(char::is_control) {
        Some("the name has a control character".into())
    } else if name.len() > max {
        Some(format!("the name is longer than {max} bytes"))
    } else {
        None
    }
}

impl Request {
    fn check(&self) -> Result<Checked, String> {
        // Review L6: the IDs it names are the owner's shape (they are part
        // of the canonical payload's lines).
        let max = sp::profile::SCHEMA
            .field(sp::profile::PROFILE_ID)
            .map_or(0, |f| f.max_len as usize);
        let named = match self {
            Request::Clone {
                source_profile_id, ..
            } => Some(source_profile_id.as_str()),
            other => other.target(),
        };
        if let Some(id) = named.filter(|id| !safe_id(id) || id.len() > max) {
            return Err(format!("{id:?} is no profile ID"));
        }
        let venues_of = |names: &[String]| -> Result<Vec<VenueId>, String> {
            let mut out: Vec<VenueId> = Vec::new();
            for n in names {
                let v = VenueId::parse(n).ok_or_else(|| format!("{n} is no venue"))?;
                if out.contains(&v) {
                    return Err(format!("{n} is given twice"));
                }
                out.push(v);
            }
            if out.is_empty() {
                return Err("a profile trains on at least one venue".into());
            }
            out.sort_by_key(|v| v.index());
            Ok(out)
        };
        // A profile carries every value: no key is the file's by omission.
        let values_of =
            |pairs: &[(String, String)]| -> Result<Vec<(&'static str, String)>, String> {
                let mut out: Vec<(&'static str, String)> = Vec::new();
                for (id, value) in pairs {
                    let k = settings::key(id)
                        .ok_or_else(|| format!("{id} is no Strategy C setting"))?;
                    if out.iter().any(|(s, _)| *s == k) {
                        return Err(format!("{k} is given twice"));
                    }
                    if let Some(why) = settings::value_refused(value) {
                        return Err(format!("{k}: {why}"));
                    }
                    out.push((k, value.clone()));
                }
                out.sort_by_key(|(k, _)| KEYS.iter().position(|x| x == k));
                if let Some(missing) = KEYS.iter().find(|k| !out.iter().any(|(s, _)| s == *k)) {
                    return Err(format!("{missing}: a profile sets every value"));
                }
                Ok(out)
            };
        let name_ok = |name: &str| match name_refused(name) {
            Some(why) => Err(why),
            None => Ok(Some(name.to_string())),
        };
        let (action, name, venues, values, mut text) = match self {
            Request::Create {
                name,
                venues,
                values,
            } => (
                "CREATE",
                name_ok(name)?,
                venues_of(venues)?,
                values_of(values)?,
                String::new(),
            ),
            Request::Update {
                profile_id,
                name,
                venues,
                values,
            } => (
                "UPDATE",
                name_ok(name)?,
                venues_of(venues)?,
                values_of(values)?,
                format!("profile_id={profile_id}\n"),
            ),
            Request::Clone {
                source_profile_id,
                source_revision,
                name,
            } => (
                "CLONE",
                name_ok(name)?,
                Vec::new(),
                Vec::new(),
                format!("source={source_profile_id}\nsource_revision={source_revision}\n"),
            ),
            Request::Delete {
                profile_id,
                archive,
            } => (
                if *archive { "ARCHIVE" } else { "DELETE" },
                None,
                Vec::new(),
                Vec::new(),
                format!("profile_id={profile_id}\n"),
            ),
        };
        // The canonical payload: the action, its target, the name, the
        // venues and the values as stored.
        text.insert_str(0, &format!("action={action}\n"));
        if let Some(n) = &name {
            text.push_str(&format!("name={n}\n"));
        }
        for v in &venues {
            text.push_str(&format!("venue={}\n", v.name()));
        }
        text.push_str(&settings::canonical_text(&values));
        Ok(Checked {
            action,
            name,
            venues,
            values,
            sha256: darkbot_bsc_record::sha256(text.as_bytes()),
        })
    }

    /// The profile it changes (an update, archive or delete).
    fn target(&self) -> Option<&str> {
        match self {
            Request::Update { profile_id, .. } | Request::Delete { profile_id, .. } => {
                Some(profile_id)
            }
            Request::Create { .. } | Request::Clone { .. } => None,
        }
    }
}

/// The outcome of one profile request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub request_id: String,
    pub action: &'static str,
    pub request_sha256: [u8; 32],
    pub expected: u64,
    /// `APPLIED`, `REJECTED_STALE`, `REJECTED_INVALID`, `REJECTED_NOT_FOUND`
    /// or `REJECTED_IN_USE`.
    pub outcome: &'static str,
    /// The catalog version now current (the new one when applied).
    pub catalog_version: u64,
    pub profile_id: Option<String>,
    pub profile_revision: Option<u64>,
    pub reason: Option<String>,
}

/// `PENDING`: a request's intent (kept before any catalog version).
const OUTCOMES: [&str; 6] = [
    "PENDING",
    "APPLIED",
    "REJECTED_STALE",
    "REJECTED_INVALID",
    "REJECTED_NOT_FOUND",
    "REJECTED_IN_USE",
];
const ACTIONS: [&str; 5] = ["CREATE", "UPDATE", "CLONE", "ARCHIVE", "DELETE"];

impl Receipt {
    pub fn applied(&self) -> bool {
        self.outcome == "APPLIED"
    }

    /// One line for the controller and the gateway's receipt detail.
    pub fn line(&self) -> String {
        format!(
            "outcome={} action={} profile_id={} revision={} catalog={} request_id={}{}",
            self.outcome,
            self.action,
            self.profile_id.as_deref().unwrap_or("-"),
            self.profile_revision
                .map_or_else(|| "-".to_string(), |r| r.to_string()),
            self.catalog_version,
            self.request_id,
            self.reason
                .as_deref()
                .map(|r| format!(" reason={r}"))
                .unwrap_or_default()
        )
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&pr::SCHEMA);
        d.set(pr::REQUEST_ID, self.request_id.as_str())
            .set(pr::ACTION, self.action)
            .set(pr::REQUEST_SHA256, Value::B256(self.request_sha256))
            .set(pr::EXPECTED, self.expected)
            .set(pr::OUTCOME, self.outcome)
            .set(pr::CATALOG_VERSION, self.catalog_version)
            .set_opt(pr::PROFILE_ID, self.profile_id.as_deref())
            .set_opt(pr::PROFILE_REVISION, self.profile_revision)
            .set_opt(pr::REASON, self.reason.as_deref())
            .set(pr::OBSERVED_UNIX_MS, observed_unix_ms);
        d.encode().map_err(|e| e.to_string())
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&pr::SCHEMA, bytes).map_err(|e| e.to_string())?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let word = |tag: u16, of: &[&'static str]| -> Result<&'static str, String> {
            let t = r.text(tag).map_err(e)?;
            of.iter()
                .copied()
                .find(|w| *w == t)
                .ok_or_else(|| format!("{t}: unknown in a profile receipt"))
        };
        Ok(Receipt {
            request_id: r.text(pr::REQUEST_ID).map_err(e)?.to_string(),
            action: word(pr::ACTION, &ACTIONS)?,
            request_sha256: r.b256(pr::REQUEST_SHA256).map_err(e)?,
            expected: r.u64(pr::EXPECTED).map_err(e)?,
            outcome: word(pr::OUTCOME, &OUTCOMES)?,
            catalog_version: r.u64(pr::CATALOG_VERSION).map_err(e)?,
            profile_id: r.opt_text(pr::PROFILE_ID).map_err(e)?.map(str::to_string),
            profile_revision: r.opt_u64(pr::PROFILE_REVISION).map_err(e)?,
            reason: r.opt_text(pr::REASON).map_err(e)?.map(str::to_string),
        })
    }
}

fn catalog_path(dir: &Path, n: u64) -> PathBuf {
    dir.join("catalog").join(format!("{n:020}.dbrec"))
}

/// The highest catalog version on file in `dir/catalog`.
fn highest(dir: &Path) -> Result<Option<u64>, String> {
    let mut highest: Option<u64> = None;
    for entry in std::fs::read_dir(dir.join("catalog")).map_err(|e| e.to_string())? {
        let name = entry.map_err(|e| e.to_string())?.file_name();
        if let Some(n) = name
            .to_string_lossy()
            .strip_suffix(".dbrec")
            .and_then(|n| n.parse::<u64>().ok())
        {
            highest = highest.max(Some(n));
        }
    }
    Ok(highest)
}

/// Catalog version `n` in `dir` (its owner `owner`), as numbered.
fn read_version(dir: &Path, owner: u32, n: u64) -> Result<Catalog, String> {
    let path = catalog_path(dir, n);
    let bytes = read_private(&path, owner).map_err(|e| format!("{}: {e}", path.display()))?;
    let c = Catalog::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    match c.version == n {
        true => Ok(c),
        false => Err(format!("catalog file {n} holds version {}", c.version)),
    }
}

fn current_in(dir: &Path, owner: u32) -> Result<(Catalog, Option<PathBuf>), String> {
    match highest(dir)? {
        None => Ok((Catalog::default(), None)),
        Some(n) => Ok((read_version(dir, owner, n)?, Some(catalog_path(dir, n)))),
    }
}

/// The catalog on file under the source-mode owner's directory `sources`
/// without creating anything (version 0 when there is none) and the file
/// it read; a store another user can change is refused.
pub fn current_file(sources: &Path) -> Result<(Catalog, Option<PathBuf>), String> {
    let dir = sources.join(DIR);
    match std::fs::symlink_metadata(dir.join("catalog")) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok((Catalog::default(), None)),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let owner = private_directory(&dir).map_err(|r| r.to_string())?;
    if private_directory(&dir.join("catalog")).map_err(|r| r.to_string())? != owner {
        return Err(format!("{} has another owner", dir.display()));
    }
    current_in(&dir, owner)
}

/// The catalog version that created `profile_id`: its ID names it
/// (`p<version>`, [`Owner::apply`]).
fn created_in(profile_id: &str) -> Option<u64> {
    profile_id
        .strip_prefix('p')
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|n| n.parse().ok())
}

/// The oldest catalog version in `lo..=current` at which `profile_id`
/// has at least revision `at_least`, with that catalog. A profile's
/// revision never falls from one version to the next, and a deleted one
/// is left out of the versions after its deletion (counted here as above
/// every revision), so a binary search finds it (review L5).
fn first_reaching(
    dir: &Path,
    owner: u32,
    current: &Catalog,
    profile_id: &str,
    lo: u64,
    at_least: u64,
) -> Result<Option<Catalog>, String> {
    let read = |n: u64| match n == current.version {
        true => Ok(current.clone()),
        false => read_version(dir, owner, n),
    };
    let rank = |c: &Catalog| c.profile(profile_id).map_or(u64::MAX, |p| p.revision);
    let mut hi = current.version;
    if lo == 0 || lo > hi || rank(current) < at_least {
        return Ok(None);
    }
    let (mut lo, mut found) = (lo, current.clone());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let c = read(mid)?;
        if rank(&c) >= at_least {
            (hi, found) = (mid, c);
        } else {
            lo = mid + 1;
        }
    }
    Ok(Some(found))
}

/// `profile_id` at `revision` from the catalog versions under `sources`
/// (every version is kept); none if it never had that revision.
pub fn revision(
    sources: &Path,
    profile_id: &str,
    revision: u64,
) -> Result<Option<Profile>, String> {
    let dir = sources.join(DIR);
    let (current, _) = current_file(sources)?;
    if current.version == 0 {
        return Ok(None);
    }
    let owner = private_directory(&dir).map_err(|r| r.to_string())?;
    revision_in(&dir, owner, &current, profile_id, revision)
}

fn revision_in(
    dir: &Path,
    owner: u32,
    current: &Catalog,
    profile_id: &str,
    revision: u64,
) -> Result<Option<Profile>, String> {
    let held = |c: &Catalog| {
        c.profile(profile_id)
            .filter(|p| p.revision == revision)
            .cloned()
    };
    let Some(created) = created_in(profile_id) else {
        return Ok(None);
    };
    // The first version past it; the one before holds it, if any does.
    let past = match revision.checked_add(1) {
        Some(next) => first_reaching(dir, owner, current, profile_id, created, next)?,
        None => None,
    };
    match past {
        None => Ok(held(current)),
        Some(c) if c.version <= created => Ok(None),
        Some(c) => Ok(held(&read_version(dir, owner, c.version - 1)?)),
    }
}

/// The most profiles one catalog version holds within the records' bound
/// (every field at its longest, one deleted one besides): the parser's
/// upper bound of `BSC_STRATEGY_PROFILES_MAX` (review L4).
pub fn max_profiles_bound() -> u64 {
    static BOUND: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *BOUND.get_or_init(|| {
        let longest = |schema: &darkbot_bsc_record::Schema, tag: u16| {
            "x".repeat(schema.field(tag).map_or(0, |f| f.max_len as usize))
        };
        let p = &sp::profile::SCHEMA;
        let profile = Profile {
            profile_id: longest(p, sp::profile::PROFILE_ID),
            name: longest(p, sp::profile::NAME),
            revision: u64::MAX,
            status: Status::Archived,
            venues: VenueId::ALL.to_vec(),
            values: KEYS
                .iter()
                .map(|k| (*k, longest(&ss::setting::SCHEMA, ss::setting::VALUE)))
                .collect(),
            created_unix_ms: u64::MAX,
            updated_unix_ms: u64::MAX,
            request_id: longest(p, sp::profile::REQUEST_ID),
        };
        let size = |n: usize| {
            Catalog {
                version: u64::MAX,
                request_id: Some(longest(&sp::SCHEMA, sp::REQUEST_ID)),
                request_sha256: Some([0xff; 32]),
                profiles: vec![profile.clone(); n],
            }
            .encode(u64::MAX)
            .map_or(usize::MAX, |b| b.len())
        };
        let (empty, one) = (size(0), size(1));
        let per = one.saturating_sub(empty).max(1);
        let room = darkbot_bsc_record::MAX_RECORD_BYTES.saturating_sub(empty) / per;
        room.saturating_sub(1) as u64
    })
}

/// A `bsc_runtime profile <cmd> …` command line: the request, its CAS and
/// the owner's places, each flag's value from its own pair (review M2: a
/// name like `--state` stays a name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub request: Request,
    pub expected: u64,
    pub request_id: String,
    pub state: String,
    pub config: String,
}

/// Parses `bsc_runtime profile <cmd> <rest>`: `KEY=VALUE` pairs first
/// (create and update only), then each allowed flag exactly once with its
/// value.
pub fn command(cmd: &str, rest: &[String]) -> Result<Command, String> {
    let usage = || format!("profile {cmd}: see bsc_runtime's usage");
    let mut allowed = vec!["--expect", "--request-id", "--state", "--config"];
    allowed.extend(match cmd {
        "create" => &["--name", "--venues"][..],
        "update" => &["--profile", "--name", "--venues"][..],
        "clone" => &["--source", "--source-revision", "--name"][..],
        "archive" | "delete" => &["--profile"][..],
        _ => return Err(usage()),
    });
    let given: Vec<&String> = rest.iter().take_while(|a| !a.starts_with("--")).collect();
    let flags = &rest[given.len()..];
    if !flags.len().is_multiple_of(2) || (!matches!(cmd, "create" | "update") && !given.is_empty())
    {
        return Err(usage());
    }
    let mut set: Vec<(&str, &str)> = Vec::new();
    for f in flags.chunks(2) {
        let (k, v) = (f[0].as_str(), f[1].as_str());
        if !allowed.contains(&k) || set.iter().any(|(s, _)| *s == k) {
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
            .map_or("", |(_, v)| *v)
            .to_string()
    };
    let number = |name: &str| {
        get(name)
            .parse::<u64>()
            .map_err(|_| format!("{name} is an unsigned integer"))
    };
    let values = given
        .iter()
        .map(|a| {
            a.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or(format!("{a}: KEY=VALUE expected"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let venues = || get("--venues").split(',').map(String::from).collect();
    let request = match cmd {
        "create" => Request::Create {
            name: get("--name"),
            venues: venues(),
            values,
        },
        "update" => Request::Update {
            profile_id: get("--profile"),
            name: get("--name"),
            venues: venues(),
            values,
        },
        "clone" => Request::Clone {
            source_profile_id: get("--source"),
            source_revision: number("--source-revision")?,
            name: get("--name"),
        },
        what => Request::Delete {
            profile_id: get("--profile"),
            archive: what == "archive",
        },
    };
    Ok(Command {
        request,
        expected: number("--expect")?,
        request_id: get("--request-id"),
        state: get("--state"),
        config: get("--config"),
    })
}

/// Why a request is refused: its receipt's outcome and reason, or an error
/// that leaves no receipt (an unreadable store: retry, review L2).
enum Refused {
    Outcome(&'static str, Option<String>),
    Error(String),
}

/// The profile store, opened for requests.
pub struct Owner {
    dir: PathBuf,
    owner: u32,
}

impl Owner {
    /// Opens (creating, owner-only) `<sources>/strategy-profiles`.
    pub fn open(sources: &Path) -> Result<Self, String> {
        let dir = sources.join(DIR);
        let mut owner = None;
        for d in [
            dir.clone(),
            dir.join("catalog"),
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
            dir,
            owner: owner.ok_or("no store directory")?,
        })
    }

    pub fn current(&self) -> Result<Catalog, String> {
        current_in(&self.dir, self.owner).map(|(c, _)| c)
    }

    /// Applies `request` if `expected` is current (the catalog version of a
    /// create or clone, the profile's revision of an update, archive or
    /// delete) and the runtime configuration `config_text` takes the
    /// values; `in_use` says why a profile can be neither changed nor
    /// archived or deleted (a run or the LIVE selection uses it: edits need
    /// a clone). Every request leaves a receipt.
    ///
    /// Review M1: before any catalog version, the request's intent (its
    /// payload's hash and its CAS) is kept create-new under its ID, so the
    /// ID stays this request's even if its receipt is lost: a retry with
    /// another payload or CAS is refused, the same one recovered.
    pub fn apply(
        &self,
        request_id: &str,
        request: &Request,
        expected: u64,
        config_text: &str,
        in_use: &dyn Fn(&Profile) -> Option<String>,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if !safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        let checked = request.check()?;
        let config = RuntimeConfig::parse(config_text)
            .map_err(|_| "the runtime configuration is refused (config-check names why)")?;
        let file = format!("{request_id}.dbrec");
        let receipt_path = self.dir.join("receipts").join(&file);
        let intent_path = self.dir.join("intents").join(&file);
        let mine = |path: &Path| -> Result<Receipt, String> {
            let r = Receipt::decode(&read_private(path, self.owner).map_err(|e| e.to_string())?)?;
            match r.request_sha256 == checked.sha256 && r.expected == expected {
                true => Ok(r),
                false => Err("the request ID already belongs to another request".into()),
            }
        };
        match read_private(&receipt_path, self.owner) {
            Ok(_) => return mine(&receipt_path),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("receipt unreadable: {e}")),
        }
        let mut receipt = Receipt {
            request_id: request_id.into(),
            action: checked.action,
            request_sha256: checked.sha256,
            expected,
            outcome: "PENDING",
            catalog_version: 0,
            profile_id: request.target().map(str::to_string),
            profile_revision: None,
            reason: None,
        };
        // A kept intent: an earlier attempt of this very request, which may
        // have written its version.
        let mut recover = match write_new(&intent_path, &receipt.encode(now_ms)?) {
            Ok(()) => false,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                mine(&intent_path)?;
                true
            }
            Err(e) => return Err(format!("intent not written: {e}")),
        };
        let receipt = loop {
            let current = self.current()?;
            receipt.catalog_version = current.version;
            receipt.profile_revision = request
                .target()
                .and_then(|t| current.profile(t))
                .map(|p| p.revision);
            if recover
                && let Some(mine) =
                    self.written(&current, request, request_id, &checked, expected)?
            {
                receipt.outcome = "APPLIED";
                receipt.catalog_version = mine.version;
                (receipt.profile_id, receipt.profile_revision) = changed(&mine, request);
                break receipt;
            }
            let next = match self.next(
                &current,
                (request_id, request, &checked),
                expected,
                (&config, config_text),
                in_use,
                now_ms,
            ) {
                Ok(next) => next,
                Err(Refused::Outcome(outcome, reason)) => {
                    receipt.outcome = outcome;
                    receipt.reason =
                        reason.map(|r| settings::bounded(r, pr::SCHEMA.field(pr::REASON)));
                    break receipt;
                }
                Err(Refused::Error(e)) => return Err(e),
            };
            let next = Catalog {
                version: current
                    .version
                    .checked_add(1)
                    .ok_or("catalog version exhausted")?,
                request_id: Some(request_id.into()),
                request_sha256: Some(checked.sha256),
                profiles: next,
            };
            match write_new(
                &catalog_path(&self.dir, next.version),
                &next.encode(now_ms)?,
            ) {
                Ok(()) => {
                    receipt.outcome = "APPLIED";
                    receipt.catalog_version = next.version;
                    (receipt.profile_id, receipt.profile_revision) = changed(&next, request);
                    break receipt;
                }
                // Another writer took this version first (perhaps this
                // request's twin): decide again on the catalog it wrote.
                Err(e) if e.kind() == ErrorKind::AlreadyExists => recover = true,
                Err(e) => return Err(format!("catalog not written: {e}")),
            }
        };
        match write_new(&receipt_path, &receipt.encode(now_ms)?) {
            Ok(()) => Ok(receipt),
            // A concurrent identical request wrote it first: its receipt counts.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => mine(&receipt_path),
            Err(e) => Err(format!("receipt not written: {e}")),
        }
    }

    /// The catalog version this very request wrote, if any: for a create or
    /// a clone the one after its expected version, for a change of a
    /// profile the one that gave it the revision after the expected one.
    fn written(
        &self,
        current: &Catalog,
        request: &Request,
        request_id: &str,
        checked: &Checked,
        expected: u64,
    ) -> Result<Option<Catalog>, String> {
        let Some(next) = expected.checked_add(1) else {
            return Ok(None);
        };
        let then = match request.target() {
            None if next > current.version => None,
            None if next == current.version => Some(current.clone()),
            None => Some(read_version(&self.dir, self.owner, next)?),
            Some(target) => match created_in(target) {
                Some(created) => {
                    first_reaching(&self.dir, self.owner, current, target, created, next)?
                }
                None => None,
            },
        };
        match then {
            Some(c) if c.written_by(request_id, &checked.sha256)? => Ok(Some(c)),
            _ => Ok(None),
        }
    }

    /// The profiles after `request` on `current` (a profile deleted before
    /// is left out: it lives on in the version its deletion wrote), or why
    /// it is refused.
    fn next(
        &self,
        current: &Catalog,
        (request_id, request, checked): (&str, &Request, &Checked),
        expected: u64,
        (config, config_text): (&RuntimeConfig, &str),
        in_use: &dyn Fn(&Profile) -> Option<String>,
        now_ms: u64,
    ) -> Result<Vec<Profile>, Refused> {
        let invalid = |why: String| Refused::Outcome("REJECTED_INVALID", Some(why));
        let not_found = |why: String| Refused::Outcome("REJECTED_NOT_FOUND", Some(why));
        let mut profiles: Vec<Profile> = current
            .profiles
            .iter()
            .filter(|p| p.status != Status::Deleted)
            .cloned()
            .collect();
        let name_taken = |profiles: &[Profile], name: &str, except: Option<&str>| {
            profiles
                .iter()
                .any(|p| p.name == name && Some(p.profile_id.as_str()) != except)
        };
        let parsed = |values: &[(&'static str, String)]| {
            settings::effective(
                config_text,
                &Settings {
                    generation: 0,
                    request_id: None,
                    values: values.to_vec(),
                },
            )
            .map(|_| ())
        };
        match request {
            Request::Create { .. } | Request::Clone { .. } => {
                if expected != current.version {
                    return Err(Refused::Outcome("REJECTED_STALE", None));
                }
                if profiles.len() >= config.strategy.profiles_max {
                    return Err(invalid(format!(
                        "the catalog holds its maximum of {} profiles (BSC_STRATEGY_PROFILES_MAX)",
                        config.strategy.profiles_max
                    )));
                }
                let name = checked.name.clone().unwrap_or_default();
                if name_taken(&profiles, &name, None) {
                    return Err(invalid(format!("the name {name} is taken")));
                }
                let (venues, values) = match request {
                    Request::Clone {
                        source_profile_id,
                        source_revision,
                        ..
                    } => {
                        if !profiles.iter().any(|p| p.profile_id == *source_profile_id) {
                            return Err(not_found(format!("no profile {source_profile_id}")));
                        }
                        let then = revision_in(
                            &self.dir,
                            self.owner,
                            current,
                            source_profile_id,
                            *source_revision,
                        )
                        .map_err(Refused::Error)?
                        .ok_or_else(|| {
                            not_found(format!(
                                "{source_profile_id} has no revision {source_revision}"
                            ))
                        })?;
                        (then.venues, then.values)
                    }
                    _ => (checked.venues.clone(), checked.values.clone()),
                };
                parsed(&values).map_err(invalid)?;
                let version = current
                    .version
                    .checked_add(1)
                    .ok_or_else(|| Refused::Error("catalog version exhausted".into()))?;
                profiles.push(Profile {
                    profile_id: format!("p{version:06}"),
                    name,
                    revision: 1,
                    status: Status::Active,
                    venues,
                    values,
                    created_unix_ms: now_ms,
                    updated_unix_ms: now_ms,
                    request_id: request_id.to_string(),
                });
            }
            Request::Update { profile_id, .. } | Request::Delete { profile_id, .. } => {
                let i = profiles
                    .iter()
                    .position(|p| p.profile_id == *profile_id)
                    .ok_or_else(|| not_found(format!("no profile {profile_id}")))?;
                if profiles[i].revision != expected {
                    return Err(Refused::Outcome("REJECTED_STALE", None));
                }
                let mut p = profiles[i].clone();
                // Review L3: a profile in use is neither changed (a clone
                // is) nor archived or deleted.
                if let Some(why) = in_use(&p) {
                    return Err(Refused::Outcome("REJECTED_IN_USE", Some(why)));
                }
                if let Request::Delete { archive, .. } = request {
                    if *archive && p.status == Status::Archived {
                        return Err(invalid(format!("{profile_id} is archived")));
                    }
                    p.status = match archive {
                        true => Status::Archived,
                        false => Status::Deleted,
                    };
                } else {
                    let name = checked.name.clone().unwrap_or_default();
                    if name_taken(&profiles, &name, Some(profile_id)) {
                        return Err(invalid(format!("the name {name} is taken")));
                    }
                    parsed(&checked.values).map_err(invalid)?;
                    (p.name, p.venues, p.values) =
                        (name, checked.venues.clone(), checked.values.clone());
                }
                p.revision = p
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("revision exhausted".into()))?;
                (p.updated_unix_ms, p.request_id) = (now_ms, request_id.to_string());
                profiles[i] = p;
            }
        }
        Ok(profiles)
    }
}

/// The profile `request` created or changed in `catalog` and its revision.
fn changed(catalog: &Catalog, request: &Request) -> (Option<String>, Option<u64>) {
    let p = match request.target() {
        Some(t) => catalog.profile(t),
        // A create or clone appends.
        None => catalog.profiles.last(),
    };
    (p.map(|p| p.profile_id.clone()), p.map(|p| p.revision))
}

#[cfg(test)]
#[path = "strategy_profiles_tests.rs"]
mod tests;
