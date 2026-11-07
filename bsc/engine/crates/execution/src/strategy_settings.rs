//! Frontend package (06.11., plan 04.11.): the native owner of the
//! operator's Strategy C settings, the keys the LIVE tab sets ([`KEYS`]).
//! A request is checked by the runtime configuration's own parser on the
//! configuration file with the settings in place ([`effective`]): a value
//! the runtime would refuse is never stored. Stored at once, one
//! generation per applied request (its number the next request's
//! compare-and-swap) and a receipt per request; effective at the next
//! start of Strategy C (its switch on, a runtime restart): the runtime
//! journals the settings it starts the strategy with ([`Settings::journal`])
//! and a replay starts it with them; while it stays on (a restart for other
//! venues) it keeps them. A real position Strategy C bought keeps its
//! entry's exit rules across restarts and new settings (decision 06.11.:
//! the ledger keeps them with its BUY's release); a real lot without them
//! (C3's, adopted, one recovered from its admission) is decided by the rules
//! of each start; a PAPER position ends with every stop or restart of the
//! strategy. A key not set is the configuration file's.
//!
//! State `<sources>/strategy-c/`: `generations/<n>.dbrec` created
//! create-new (two writers of one next generation cannot both succeed),
//! every generation kept, so a stale request always resolves;
//! `receipts/<request_id>.dbrec`. A crash between a generation and its
//! receipt is recovered from the generation, which names its request.

use crate::runtime::RuntimeConfig;
use crate::sources::{create_durable_dir, private_directory, read_private, safe_id, write_new};
use darkbot_bsc_record::schema::bsc_runtime::{
    strategy_setting_receipt as sr, strategy_settings as ss,
};
use darkbot_bsc_record::{Draft, decode};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// The settable keys (the user's scope of 06.11.; BSC-V8-05 the first-buyer
/// window), in their order; every other key stays the configuration file's.
/// New keys go last: a journal `p` record names a key by its index here.
pub const KEYS: [&str; 10] = [
    "BSC_STRATEGY_C_OBSERVATION_MS",
    "BSC_STRATEGY_C_FILTERS",
    "BSC_STRATEGY_C_INPUT_WEI",
    "BSC_STRATEGY_C_TARGET_NET_MULTIPLE",
    "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
    "BSC_STRATEGY_C_HOLD_MS",
    "BSC_STRATEGY_C_MAX_OPEN",
    "BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL",
    "BSC_STRATEGY_C_CREATOR_SELL_EXIT",
    "BSC_STRATEGY_C_FIRST_BUYER_WINDOW",
];

/// The owner's directory under the source-mode owner's.
pub const DIR: &str = "strategy-c";

/// The journal record of the settings a start of Strategy C took (a
/// `Runtime` record right before its `U` switch on).
pub const JOURNAL_TAG: u8 = b'p';

/// The settable key `id` names.
pub fn key(id: &str) -> Option<&'static str> {
    KEYS.iter().copied().find(|k| *k == id)
}

/// The longest value the records hold (the settings', the receipts' and
/// the status's text bound; review LOW 2).
pub fn max_value_len() -> usize {
    ss::setting::SCHEMA
        .field(ss::setting::VALUE)
        .map_or(0, |f| f.max_len as usize)
}

/// `text` cut to the record text bound of `field` (a reason, diagnostic
/// only), at a character boundary.
pub fn bounded(mut text: String, field: Option<&darkbot_bsc_record::Field>) -> String {
    let max = field.map_or(0, |f| f.max_len as usize);
    if text.len() > max {
        let mut cut = max;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    text
}

/// Why `value` cannot stand as a configuration value: one word of
/// printable ASCII or nothing (the parser trims a line, so a value with a
/// space or a control character would not read back as set), its length
/// within the records' ([`max_value_len`]). Whether the key takes it the
/// runtime configuration's parser decides.
pub fn value_refused(value: &str) -> Option<&'static str> {
    if !value.bytes().all(|b| b.is_ascii_graphic()) {
        Some("the value is not printable ASCII without spaces")
    } else if value.len() > max_value_len() {
        Some("the value is longer than the records hold")
    } else {
        None
    }
}

/// One generation of the settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// 0: none on file.
    pub generation: u64,
    /// The request that wrote it.
    pub request_id: Option<String>,
    /// The set keys with their values, in [`KEYS`] order.
    pub values: Vec<(&'static str, String)>,
}

fn order(key: &str) -> usize {
    KEYS.iter().position(|k| *k == key).unwrap_or(KEYS.len())
}

impl Settings {
    pub fn value(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Whether this generation sets every key of `values` to its value
    /// for `request_id`.
    fn applied(&self, request_id: &str, values: &[(&'static str, String)]) -> bool {
        self.request_id.as_deref() == Some(request_id)
            && values
                .iter()
                .all(|(k, v)| self.value(k) == Some(v.as_str()))
    }

    /// The values with every key of `set` set to its value.
    fn with(&self, set: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
        let mut values: Vec<_> = self
            .values
            .iter()
            .filter(|(k, _)| !set.iter().any(|(s, _)| s == k))
            .cloned()
            .collect();
        values.extend(set.iter().cloned());
        values.sort_by_key(|(k, _)| order(k));
        values
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&ss::SCHEMA);
        d.set(ss::GENERATION, self.generation)
            .set(
                ss::REQUEST_ID,
                self.request_id.as_deref().unwrap_or_default(),
            )
            .set(ss::PROVENANCE_TOOL, "bsc_runtime")
            .set(ss::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms);
        for (k, v) in &self.values {
            let mut e = Draft::new(&ss::setting::SCHEMA);
            e.set(ss::setting::ID, *k)
                .set(ss::setting::VALUE, v.as_str());
            d.push(ss::SETTINGS, e.into_nested());
        }
        d.encode().map_err(|e| e.to_string())
    }

    /// Strict: every key settable, once, in order, every value one the
    /// owner would store.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&ss::SCHEMA, bytes).map_err(|e| e.to_string())?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let mut values = Vec::new();
        for row in r.nested_all(ss::SETTINGS).map_err(e)? {
            let id = row.text(ss::setting::ID).map_err(e)?;
            let k = key(id).ok_or_else(|| format!("{id} is no Strategy C setting"))?;
            let v = row.text(ss::setting::VALUE).map_err(e)?;
            if let Some(why) = value_refused(v) {
                return Err(format!("{k}: {why}"));
            }
            values.push((k, v.to_string()));
        }
        if !values.windows(2).all(|w| order(w[0].0) < order(w[1].0)) {
            return Err("the settings are not one per key in order".into());
        }
        let request_id = r.text(ss::REQUEST_ID).map_err(e)?;
        Ok(Settings {
            generation: r.u64(ss::GENERATION).map_err(e)?,
            request_id: (!request_id.is_empty()).then(|| request_id.to_string()),
            values,
        })
    }

    /// The journal record: `b'p' | generation u64 LE | count u8 | (key
    /// index u8 | length u16 LE | value)*`.
    pub fn journal(&self) -> Vec<u8> {
        let mut out = vec![JOURNAL_TAG];
        out.extend_from_slice(&self.generation.to_le_bytes());
        out.push(self.values.len() as u8);
        for (k, v) in &self.values {
            out.push(order(k) as u8);
            out.extend_from_slice(&(v.len() as u16).to_le_bytes());
            out.extend_from_slice(v.as_bytes());
        }
        out
    }

    /// A journal record of [`Settings::journal`]; anything else is none.
    pub fn of_journal(p: &[u8]) -> Option<Self> {
        let (&tag, rest) = p.split_first()?;
        if tag != JOURNAL_TAG {
            return None;
        }
        let generation = u64::from_le_bytes(rest.get(..8)?.try_into().ok()?);
        let (&count, mut rest) = rest.get(8..)?.split_first()?;
        let mut values: Vec<(&'static str, String)> = Vec::new();
        for _ in 0..count {
            let k = *KEYS.get(usize::from(*rest.first()?))?;
            let len = usize::from(u16::from_le_bytes(rest.get(1..3)?.try_into().ok()?));
            let v = std::str::from_utf8(rest.get(3..3 + len)?).ok()?;
            if value_refused(v).is_some() || values.last().is_some_and(|l| order(l.0) >= order(k)) {
                return None;
            }
            values.push((k, v.to_string()));
            rest = &rest[3 + len..];
        }
        rest.is_empty().then_some(Settings {
            generation,
            request_id: None,
            values,
        })
    }
}

/// The configuration `text` with `values` in place: a key's line replaced,
/// a key the file does not set appended. Comments and every other line
/// stay as they are (a key set twice the parser refuses).
pub fn overlay(text: &str, values: &[(&'static str, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut placed = vec![false; values.len()];
    for line in text.lines() {
        let t = line.trim();
        let set = (!t.starts_with('#'))
            .then(|| t.split_once('='))
            .flatten()
            .and_then(|(k, _)| values.iter().position(|(s, _)| *s == k.trim()))
            .filter(|i| !placed[*i]);
        match set {
            Some(i) => {
                placed[i] = true;
                out.push_str(values[i].0);
                out.push('=');
                out.push_str(&values[i].1);
            }
            None => out.push_str(line),
        }
        out.push('\n');
    }
    for ((k, v), placed) in values.iter().zip(placed) {
        if !placed {
            out.push_str(k);
            out.push('=');
            out.push_str(v);
            out.push('\n');
        }
    }
    out
}

/// The runtime configuration of `text` with `settings` in place: the one
/// validation of a setting (bounds, the exit multiples' order, every
/// cross-check the runtime makes).
pub fn effective(text: &str, settings: &Settings) -> Result<RuntimeConfig, String> {
    RuntimeConfig::parse(&overlay(text, &settings.values))
}

/// Every settable key's value in the configuration `text` with `settings`
/// in place (a key neither sets is left out): what a start takes.
pub fn effective_values(text: &str, settings: &Settings) -> Vec<(&'static str, String)> {
    let text = overlay(text, &settings.values);
    KEYS.iter()
        .filter_map(|k| {
            crate::sources::config_value(&text, k)
                .ok()
                .flatten()
                .map(|v| (*k, v.to_string()))
        })
        .collect()
}

/// BSC-V8 (settings query, training): the canonical text of a settings
/// basis, `KEY=value\n` for every key of `values` in [`KEYS`] order (the
/// values the native parser takes, as their text: "2" and "2.0" are two
/// bases), and its SHA-256: what "the same configuration" means across
/// settings, profiles and runs.
pub fn canonical_text(values: &[(&'static str, String)]) -> String {
    let mut sorted: Vec<&(&'static str, String)> = values.iter().collect();
    sorted.sort_by_key(|(k, _)| order(k));
    sorted.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

pub fn config_sha256(values: &[(&'static str, String)]) -> [u8; 32] {
    darkbot_bsc_record::sha256(canonical_text(values).as_bytes())
}

/// The outcome of one setting request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub request_id: String,
    /// Every key the request set with its value, in [`KEYS`] order (one for
    /// a single setting; BSC-V8-04: several for "save all").
    pub values: Vec<(&'static str, String)>,
    pub expected_generation: u64,
    /// `APPLIED`, `REJECTED_STALE` or `REJECTED_INVALID`.
    pub outcome: &'static str,
    /// The generation now current (the new one when applied).
    pub generation: u64,
    /// Why the runtime configuration refuses the value.
    pub reason: Option<String>,
}

impl Receipt {
    pub fn applied(&self) -> bool {
        self.outcome == "APPLIED"
    }

    /// One line for the controller and the gateway's receipt detail (a
    /// single setting's as before; "save all" names its keys).
    pub fn line(&self) -> String {
        // Several keys: their count (the gateway keeps the first 300
        // characters of the line; review: the reason must fit).
        let set = match self.values.as_slice() {
            [(id, value)] => format!("id={id} value={value}"),
            values => format!("keys={}", values.len()),
        };
        format!(
            "outcome={} {set} generation={} request_id={}{}",
            self.outcome,
            self.generation,
            self.request_id,
            self.reason
                .as_deref()
                .map(|r| format!(" reason={r}"))
                .unwrap_or_default()
        )
    }

    fn same(&self, values: &[(&'static str, String)], expected_generation: u64) -> bool {
        self.values == values && self.expected_generation == expected_generation
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&sr::SCHEMA);
        d.set(sr::REQUEST_ID, self.request_id.as_str())
            .set(sr::EXPECTED_GENERATION, self.expected_generation)
            .set(sr::OUTCOME, self.outcome)
            .set(sr::GENERATION, self.generation)
            .set_opt(sr::REASON, self.reason.as_deref())
            .set(sr::OBSERVED_UNIX_MS, observed_unix_ms);
        for (k, v) in &self.values {
            let mut e = Draft::new(&ss::setting::SCHEMA);
            e.set(ss::setting::ID, *k)
                .set(ss::setting::VALUE, v.as_str());
            d.push(sr::VALUES, e.into_nested());
        }
        d.encode().map_err(|e| e.to_string())
    }

    /// v2, or a single setting's v1 receipt (06.11.).
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let settable = |id: &str| key(id).ok_or_else(|| format!("{id} is no Strategy C setting"));
        let (r, values) = match decode(&sr::SCHEMA, bytes) {
            Ok(r) => {
                let mut values = Vec::new();
                for row in r.nested_all(sr::VALUES).map_err(e)? {
                    let k = settable(row.text(ss::setting::ID).map_err(e)?)?;
                    let v = row.text(ss::setting::VALUE).map_err(e)?;
                    if let Some(why) = value_refused(v) {
                        return Err(format!("{k}: {why}"));
                    }
                    values.push((k, v.to_string()));
                }
                (r, values)
            }
            Err(v2) => {
                let r = decode(&sr::V1, bytes).map_err(|v1| format!("{v2}; {v1}"))?;
                let k = settable(r.text(sr::ID).map_err(e)?)?;
                let v = r.text(sr::VALUE).map_err(e)?.to_string();
                (r, vec![(k, v)])
            }
        };
        if values.is_empty() || !values.windows(2).all(|w| order(w[0].0) < order(w[1].0)) {
            return Err("the receipt's settings are not one per key in order".into());
        }
        Ok(Receipt {
            request_id: r.text(sr::REQUEST_ID).map_err(e)?.to_string(),
            values,
            expected_generation: r.u64(sr::EXPECTED_GENERATION).map_err(e)?,
            outcome: match r.text(sr::OUTCOME).map_err(e)? {
                "PENDING" => "PENDING",
                "APPLIED" => "APPLIED",
                "REJECTED_STALE" => "REJECTED_STALE",
                "REJECTED_INVALID" => "REJECTED_INVALID",
                _ => return Err("unknown receipt outcome".into()),
            },
            generation: r.u64(sr::GENERATION).map_err(e)?,
            reason: r.opt_text(sr::REASON).map_err(e)?.map(str::to_string),
        })
    }
}

fn generation_path(dir: &Path, n: u64) -> PathBuf {
    dir.join("generations").join(format!("{n:020}.dbrec"))
}

/// The highest generation on file in `dir/generations`.
fn highest(dir: &Path) -> Result<Option<u64>, String> {
    let mut highest: Option<u64> = None;
    for entry in std::fs::read_dir(dir.join("generations")).map_err(|e| e.to_string())? {
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

/// Generation `n` of the store in `dir` (its owner `owner`), as numbered.
fn read_generation(dir: &Path, owner: u32, n: u64) -> std::io::Result<Result<Settings, String>> {
    let bytes = read_private(&generation_path(dir, n), owner)?;
    Ok(
        Settings::decode(&bytes).and_then(|s| match s.generation == n {
            true => Ok(s),
            false => Err(format!(
                "settings file {n} holds generation {}",
                s.generation
            )),
        }),
    )
}

fn current_in(dir: &Path, owner: u32) -> Result<(Settings, Option<PathBuf>), String> {
    match highest(dir)? {
        None => Ok((Settings::default(), None)),
        Some(n) => Ok((
            read_generation(dir, owner, n).map_err(|e| e.to_string())??,
            Some(generation_path(dir, n)),
        )),
    }
}

/// The settings on file under the source-mode owner's directory `sources`
/// without creating anything: none (generation 0) when there is no store;
/// a store another user can change is refused.
pub fn current(sources: &Path) -> Result<Settings, String> {
    current_file(sources).map(|(s, _)| s)
}

/// [`current`] and the file it read (none without a generation).
pub fn current_file(sources: &Path) -> Result<(Settings, Option<PathBuf>), String> {
    let dir = sources.join(DIR);
    match std::fs::symlink_metadata(dir.join("generations")) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok((Settings::default(), None)),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let owner = private_directory(&dir).map_err(|r| r.to_string())?;
    if private_directory(&dir.join("generations")).map_err(|r| r.to_string())? != owner {
        return Err(format!("{} has another owner", dir.display()));
    }
    current_in(&dir, owner)
}

/// The settings store, opened for requests.
pub struct Owner {
    dir: PathBuf,
    owner: u32,
}

impl Owner {
    /// Opens (creating, owner-only) `<sources>/strategy-c`.
    pub fn open(sources: &Path) -> Result<Self, String> {
        let dir = sources.join(DIR);
        let mut owner = None;
        for d in [
            dir.clone(),
            dir.join("generations"),
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

    pub fn current(&self) -> Result<Settings, String> {
        current_in(&self.dir, self.owner).map(|(s, _)| s)
    }

    /// Sets `id` to `value` if `expected_generation` is current and the
    /// runtime configuration `config_text` takes it. Every request leaves
    /// a receipt; the same request again gets it, another payload under
    /// its ID is refused.
    pub fn set(
        &self,
        request_id: &str,
        id: &str,
        value: &str,
        expected_generation: u64,
        config_text: &str,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        self.set_all(
            request_id,
            &[(id, value)],
            expected_generation,
            config_text,
            now_ms,
        )
    }

    /// BSC-V8-04: sets every `(id, value)` of `pairs` (each a settable key,
    /// once) as one generation with one receipt, the same CAS, validation and
    /// recovery as [`Owner::set`]: all of them or none.
    pub fn set_all(
        &self,
        request_id: &str,
        pairs: &[(&str, &str)],
        expected_generation: u64,
        config_text: &str,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        if !safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        if pairs.is_empty() {
            return Err("no Strategy C setting given".into());
        }
        let mut values: Vec<(&'static str, String)> = Vec::with_capacity(pairs.len());
        for (id, value) in pairs {
            let k = key(id).ok_or_else(|| format!("{id} is no Strategy C setting"))?;
            if values.iter().any(|(s, _)| *s == k) {
                return Err(format!("{k} is given twice"));
            }
            if let Some(why) = value_refused(value) {
                return Err(format!("{k}: {why}"));
            }
            values.push((k, value.to_string()));
        }
        values.sort_by_key(|(k, _)| order(k));
        // The file alone must stand: a refusal names a setting, never the
        // file's other lines (`bsc_runtime config-check` names why).
        RuntimeConfig::parse(config_text)
            .map_err(|_| "the runtime configuration is refused (config-check names why)")?;
        let receipt_path = self
            .dir
            .join("receipts")
            .join(format!("{request_id}.dbrec"));
        let taken = || -> Result<Receipt, String> {
            let r = Receipt::decode(
                &read_private(&receipt_path, self.owner).map_err(|e| e.to_string())?,
            )?;
            match r.same(&values, expected_generation) {
                true => Ok(r),
                false => Err("the request ID already belongs to another request".into()),
            }
        };
        match read_private(&receipt_path, self.owner) {
            Ok(_) => return taken(),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("receipt unreadable: {e}")),
        }
        let current = self.current()?;
        let mut receipt = Receipt {
            request_id: request_id.into(),
            values: values.clone(),
            expected_generation,
            outcome: "PENDING",
            generation: current.generation,
            reason: None,
        };
        // V8-01 review M1: before any generation the request's intent (its
        // values and CAS) is kept under its ID, so a lost receipt never lets
        // the ID write again under another CAS or payload.
        let intent_path = self.dir.join("intents").join(format!("{request_id}.dbrec"));
        match write_new(&intent_path, &receipt.encode(now_ms)?) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                let i = Receipt::decode(
                    &read_private(&intent_path, self.owner).map_err(|e| e.to_string())?,
                )?;
                if !i.same(&values, expected_generation) {
                    return Err("the request ID already belongs to another request".into());
                }
            }
            Err(e) => return Err(format!("intent not written: {e}")),
        }
        receipt.outcome = "REJECTED_STALE";
        // A generation is this request's only if it is the one the request
        // would have written: at its expected generation + 1, exactly its
        // values over the generation before (review: never a subset).
        let mine = |s: &Settings| -> Result<bool, String> {
            if s.request_id.as_deref() != Some(request_id) {
                return Ok(false);
            }
            let before = match s.generation {
                0 | 1 => Settings::default(),
                n => read_generation(&self.dir, self.owner, n - 1).map_err(|e| {
                    format!("{}: {e}", generation_path(&self.dir, n - 1).display())
                })??,
            };
            match s.generation == expected_generation + 1
                && s.applied(request_id, &values)
                && s.values == before.with(&values)
            {
                true => Ok(true),
                false => Err("the request ID already belongs to another request".into()),
            }
        };
        if mine(&current)? {
            // Crash recovery: the generation names this request, its
            // receipt was lost.
            receipt.outcome = "APPLIED";
        } else if expected_generation < current.generation {
            // Later requests moved on: the generation this one would have
            // written tells whether it did (every generation is kept).
            let n = expected_generation + 1;
            let then = read_generation(&self.dir, self.owner, n)
                .map_err(|e| format!("{}: {e}", generation_path(&self.dir, n).display()))??;
            if mine(&then)? {
                receipt.outcome = "APPLIED";
                receipt.generation = then.generation;
            }
        } else if expected_generation == current.generation {
            let next = Settings {
                generation: current
                    .generation
                    .checked_add(1)
                    .ok_or("generation exhausted")?,
                request_id: Some(request_id.into()),
                values: current.with(&values),
            };
            match effective(config_text, &next) {
                Err(why) => {
                    receipt.outcome = "REJECTED_INVALID";
                    receipt.reason = Some(bounded(why, sr::SCHEMA.field(sr::REASON)));
                }
                Ok(_) => match write_new(
                    &generation_path(&self.dir, next.generation),
                    &next.encode(now_ms)?,
                ) {
                    Ok(()) => {
                        receipt.outcome = "APPLIED";
                        receipt.generation = next.generation;
                    }
                    // Another writer took this generation first: stale,
                    // unless it was this very request (an identical twin).
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                        // Review: the generation it would have written decides.
                        let taken = read_generation(&self.dir, self.owner, next.generation)
                            .map_err(|e| format!("generation {}: {e}", next.generation))??;
                        if mine(&taken)? {
                            receipt.outcome = "APPLIED";
                            receipt.generation = next.generation;
                        } else {
                            receipt.generation = self.current()?.generation;
                        }
                    }
                    Err(e) => return Err(format!("generation not written: {e}")),
                },
            }
        }
        match write_new(&receipt_path, &receipt.encode(now_ms)?) {
            Ok(()) => Ok(receipt),
            // A concurrent identical request wrote it first: its receipt counts.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => taken(),
            Err(e) => Err(format!("receipt not written: {e}")),
        }
    }
}

#[cfg(test)]
#[path = "strategy_settings_tests.rs"]
mod tests;
