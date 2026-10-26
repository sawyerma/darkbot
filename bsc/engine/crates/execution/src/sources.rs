//! The native owner of the five BSC signal sources (plan §5a.2, order V2
//! §5): each source has a user-requested mode `OFF | SHADOW | LIVE` and the
//! mode this owner makes effective. No build of this program authorizes
//! economic LIVE, so [`Effective`] has no LIVE at all: a requested LIVE is
//! `BLOCKED` (`ECONOMIC_LIVE_NOT_AUTHORIZED`), and a source whose access is
//! not qualified is `BLOCKED` even in SHADOW. Health is the runtime's, not
//! this configuration's.
//!
//! State: one binary record per generation, `generations/<n>.dbrec`, created
//! create-new: two writers of the same next generation cannot both succeed,
//! so the generation is a compare-and-swap without a lock. Every request
//! leaves one receipt, `receipts/<request_id>.dbrec`; the same request again
//! gets the same receipt, another payload under its ID is refused. A crash
//! between the new generation and its receipt is recovered from the
//! generation, which names the request it applied.

use darkbot_bsc_record::schema::bsc_runtime::{source_mode_receipt as rc, source_modes as sm};
use darkbot_bsc_record::{Draft, Record, decode};
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// The five independently switchable inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceId {
    Geth,
    PublicNode,
    Drpc,
    BlockReq,
    Club48,
}

impl SourceId {
    pub const ALL: [SourceId; 5] = [
        SourceId::Geth,
        SourceId::PublicNode,
        SourceId::Drpc,
        SourceId::BlockReq,
        SourceId::Club48,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SourceId::Geth => "BSC_INGRESS_GETH",
            SourceId::PublicNode => "BSC_INGRESS_PUBLICNODE",
            SourceId::Drpc => "BSC_INGRESS_DRPC",
            SourceId::BlockReq => "BSC_INGRESS_BLOCKREQ",
            SourceId::Club48 => "BSC_INGRESS_48CLUB",
        }
    }

    pub fn parse(name: &str) -> Option<SourceId> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// What the user requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Shadow,
    Live,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Off => "OFF",
            Mode::Shadow => "SHADOW",
            Mode::Live => "LIVE",
        }
    }

    pub fn parse(name: &str) -> Option<Mode> {
        [Mode::Off, Mode::Shadow, Mode::Live]
            .into_iter()
            .find(|m| m.name() == name)
    }
}

/// What the owner makes of a request. There is no LIVE: nothing in this
/// program can make a source economically live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effective {
    Off,
    Shadow,
    Blocked(&'static str),
}

impl Effective {
    pub fn name(self) -> &'static str {
        match self {
            Effective::Off => "OFF",
            Effective::Shadow => "SHADOW",
            Effective::Blocked(_) => "BLOCKED",
        }
    }

    pub fn reason(self) -> Option<&'static str> {
        match self {
            Effective::Blocked(reason) => Some(reason),
            _ => None,
        }
    }
}

pub const ECONOMIC_LIVE_NOT_AUTHORIZED: &str = "ECONOMIC_LIVE_NOT_AUTHORIZED";
/// C2 may only run operationally after a real G0.8 acquisition PASS and the
/// evaluated G0.4 measurement (PHASENPLAN §10 offline override).
pub const C2_OPERATIONAL_NOT_AUTHORIZED: &str = "C2_OPERATIONAL_NOT_AUTHORIZED";
pub const SOURCE_ACCESS_UNQUALIFIED: &str = "SOURCE_ACCESS_UNQUALIFIED";

/// The owner's rule. 48Club's auction feed needs credentials and an
/// eligibility nobody proved, so it is BLOCKED even in SHADOW (plan §5a.2).
pub fn effective(source: SourceId, requested: Mode) -> Effective {
    match (source, requested) {
        (_, Mode::Off) => Effective::Off,
        (_, Mode::Live) => Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED),
        (SourceId::Club48, Mode::Shadow) => Effective::Blocked(SOURCE_ACCESS_UNQUALIFIED),
        (_, Mode::Shadow) => Effective::Shadow,
    }
}

/// The switchable runtime components besides the sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentId {
    /// The C2 canonical/pending state owner (observes in SHADOW).
    StateC2,
    /// The C3 execution path: SHADOW runs it to the PAPER sink.
    ExecC3,
}

impl ComponentId {
    pub const ALL: [ComponentId; 2] = [ComponentId::StateC2, ComponentId::ExecC3];

    pub fn name(self) -> &'static str {
        match self {
            ComponentId::StateC2 => "BSC_STATE_C2",
            ComponentId::ExecC3 => "BSC_EXEC",
        }
    }

    pub fn parse(name: &str) -> Option<ComponentId> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// The owner's rule for a component: SHADOW runs it without any economic
/// effect; LIVE (operational use of its state) is never effective here.
pub fn component_effective(component: ComponentId, requested: Mode) -> Effective {
    match (component, requested) {
        (_, Mode::Off) => Effective::Off,
        (_, Mode::Shadow) => Effective::Shadow,
        (ComponentId::StateC2, Mode::Live) => Effective::Blocked(C2_OPERATIONAL_NOT_AUTHORIZED),
        // LIVE would sign and send economically: never without its own
        // authorization (G0.7/G0.8, J).
        (ComponentId::ExecC3, Mode::Live) => Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED),
    }
}

/// Anything this owner switches: a source or a component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entity {
    Source(SourceId),
    Component(ComponentId),
}

impl Entity {
    pub fn name(self) -> &'static str {
        match self {
            Entity::Source(s) => s.name(),
            Entity::Component(c) => c.name(),
        }
    }

    pub fn parse(name: &str) -> Option<Entity> {
        SourceId::parse(name)
            .map(Entity::Source)
            .or_else(|| ComponentId::parse(name).map(Entity::Component))
    }

    pub fn effective(self, requested: Mode) -> Effective {
        match self {
            Entity::Source(s) => effective(s, requested),
            Entity::Component(c) => component_effective(c, requested),
        }
    }
}

impl From<SourceId> for Entity {
    fn from(s: SourceId) -> Self {
        Entity::Source(s)
    }
}

impl From<ComponentId> for Entity {
    fn from(c: ComponentId) -> Self {
        Entity::Component(c)
    }
}

/// One component of a generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentMode {
    pub component: ComponentId,
    pub requested: Mode,
    pub effective: Effective,
}

/// One source of a generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceMode {
    pub source: SourceId,
    pub requested: Mode,
    pub effective: Effective,
}

/// One generation of all five sources and every component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generation {
    pub generation: u64,
    pub sources: [SourceMode; 5],
    pub components: [ComponentMode; 2],
    pub request_id: Option<String>,
}

impl Generation {
    /// Generation 0: every source OFF (nothing was ever requested).
    pub fn initial() -> Self {
        Generation {
            generation: 0,
            sources: SourceId::ALL.map(|source| SourceMode {
                source,
                requested: Mode::Off,
                effective: Effective::Off,
            }),
            components: ComponentId::ALL.map(|component| ComponentMode {
                component,
                requested: Mode::Off,
                effective: Effective::Off,
            }),
            request_id: None,
        }
    }

    pub fn component(&self, id: ComponentId) -> ComponentMode {
        self.components[ComponentId::ALL.iter().position(|c| *c == id).unwrap_or(0)]
    }

    /// The effective mode of any entity.
    pub fn effective_of(&self, entity: Entity) -> Effective {
        match entity {
            Entity::Source(s) => self.source(s).effective,
            Entity::Component(c) => self.component(c).effective,
        }
    }

    fn set(&mut self, entity: Entity, requested: Mode) {
        match entity {
            Entity::Source(source) => {
                let at = SourceId::ALL.iter().position(|s| *s == source).unwrap_or(0);
                self.sources[at] = SourceMode {
                    source,
                    requested,
                    effective: effective(source, requested),
                };
            }
            Entity::Component(component) => {
                let at = ComponentId::ALL
                    .iter()
                    .position(|c| *c == component)
                    .unwrap_or(0);
                self.components[at] = ComponentMode {
                    component,
                    requested,
                    effective: component_effective(component, requested),
                };
            }
        }
    }

    pub fn source(&self, id: SourceId) -> SourceMode {
        self.sources[SourceId::ALL.iter().position(|s| *s == id).unwrap_or(0)]
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&sm::SCHEMA);
        d.set(sm::GENERATION, self.generation)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, observed_unix_ms);
        for s in &self.sources {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            e.set(sm::entry::SOURCE_ID, s.source.name())
                .set(sm::entry::REQUESTED, s.requested.name())
                .set(sm::entry::EFFECTIVE, s.effective.name());
            if let Some(reason) = s.effective.reason() {
                e.set(sm::entry::BLOCK_REASON, reason);
            }
            d.push(sm::SOURCES, e.into_nested());
        }
        for c in &self.components {
            use sm::component as k;
            let mut e = Draft::new(&k::SCHEMA);
            e.set(k::COMPONENT_ID, c.component.name())
                .set(k::REQUESTED, c.requested.name())
                .set(k::EFFECTIVE, c.effective.name());
            if let Some(reason) = c.effective.reason() {
                e.set(k::BLOCK_REASON, reason);
            }
            d.push(sm::COMPONENTS, e.into_nested());
        }
        if let Some(id) = &self.request_id {
            d.set(sm::REQUEST_ID, id.as_str());
        }
        d.encode().map_err(|e| e.to_string())
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode_generation(bytes)?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let mut sources = Generation::initial().sources;
        let rows = r.nested_all(sm::SOURCES).map_err(e)?;
        if rows.len() != 5 {
            return Err("a generation names exactly the five sources".into());
        }
        for row in rows {
            let source = SourceId::parse(row.text(sm::entry::SOURCE_ID).map_err(e)?)
                .ok_or("unknown source in a generation")?;
            let requested = Mode::parse(row.text(sm::entry::REQUESTED).map_err(e)?)
                .ok_or("unknown requested mode in a generation")?;
            let derived = effective(source, requested);
            // The stored effective mode must be this owner's rule, never more.
            if row.text(sm::entry::EFFECTIVE).map_err(e)? != derived.name()
                || row.opt_text(sm::entry::BLOCK_REASON).map_err(e)? != derived.reason()
            {
                return Err(format!(
                    "{}: stored effective mode is not the owner's",
                    source.name()
                ));
            }
            let at = SourceId::ALL.iter().position(|s| *s == source).unwrap_or(0);
            sources[at] = SourceMode {
                source,
                requested,
                effective: derived,
            };
        }
        // A generation from before a component existed has it OFF.
        let mut components = Generation::initial().components;
        let mut seen: Vec<ComponentId> = Vec::new();
        let rows = if r.schema().schema_version >= 2 {
            r.nested_all(sm::COMPONENTS).map_err(e)?
        } else {
            Vec::new()
        };
        for row in rows {
            use sm::component as k;
            let component = ComponentId::parse(row.text(k::COMPONENT_ID).map_err(e)?)
                .ok_or("unknown component in a generation")?;
            if seen.contains(&component) {
                return Err(format!("{} appears twice", component.name()));
            }
            seen.push(component);
            let requested = Mode::parse(row.text(k::REQUESTED).map_err(e)?)
                .ok_or("unknown requested mode in a generation")?;
            let derived = component_effective(component, requested);
            if row.text(k::EFFECTIVE).map_err(e)? != derived.name()
                || row.opt_text(k::BLOCK_REASON).map_err(e)? != derived.reason()
            {
                return Err(format!(
                    "{}: stored effective mode is not the owner's",
                    component.name()
                ));
            }
            let at = ComponentId::ALL
                .iter()
                .position(|c| *c == component)
                .unwrap_or(0);
            components[at] = ComponentMode {
                component,
                requested,
                effective: derived,
            };
        }
        Ok(Generation {
            generation: r.u64(sm::GENERATION).map_err(e)?,
            sources,
            components,
            request_id: r.opt_text(sm::REQUEST_ID).map_err(e)?.map(str::to_string),
        })
    }
}

/// A generation record of the current schema, or one written before
/// components existed (schema version 1).
fn decode_generation(bytes: &[u8]) -> Result<Record, String> {
    decode(&sm::SCHEMA, bytes)
        .or_else(|current| decode(&sm::SCHEMA_V1, bytes).map_err(|_| current))
        .map_err(|e| e.to_string())
}

/// The outcome of one request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub request_id: String,
    /// The source or component requested.
    pub entity: Entity,
    pub requested: Mode,
    pub expected_generation: u64,
    /// `APPLIED` or `REJECTED_STALE`.
    pub outcome: &'static str,
    pub generation: u64,
    pub effective: Effective,
}

impl Receipt {
    pub fn applied(&self) -> bool {
        self.outcome == "APPLIED"
    }

    /// One line for the controller and the gateway's receipt detail.
    pub fn line(&self) -> String {
        format!(
            "outcome={} source={} requested={} effective={}{} generation={} request_id={}",
            self.outcome,
            self.entity.name(),
            self.requested.name(),
            self.effective.name(),
            self.effective
                .reason()
                .map(|r| format!(" block_reason={r}"))
                .unwrap_or_default(),
            self.generation,
            self.request_id
        )
    }

    fn encode(&self, observed_unix_ms: u64) -> Result<Vec<u8>, String> {
        let mut d = Draft::new(&rc::SCHEMA);
        d.set(rc::REQUEST_ID, self.request_id.as_str())
            .set(rc::SOURCE_ID, self.entity.name())
            .set(rc::REQUESTED, self.requested.name())
            .set(rc::EXPECTED_GENERATION, self.expected_generation)
            .set(rc::OUTCOME, self.outcome)
            .set(rc::GENERATION, self.generation)
            .set(rc::EFFECTIVE, self.effective.name())
            .set(rc::OBSERVED_UNIX_MS, observed_unix_ms);
        if let Some(reason) = self.effective.reason() {
            d.set(rc::BLOCK_REASON, reason);
        }
        d.encode().map_err(|e| e.to_string())
    }

    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let r = decode(&rc::SCHEMA, bytes).map_err(|e| e.to_string())?;
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let entity = Entity::parse(r.text(rc::SOURCE_ID).map_err(e)?).ok_or("unknown source")?;
        let requested = Mode::parse(r.text(rc::REQUESTED).map_err(e)?).ok_or("unknown mode")?;
        let outcome = match r.text(rc::OUTCOME).map_err(e)? {
            "APPLIED" => "APPLIED",
            "REJECTED_STALE" => "REJECTED_STALE",
            _ => return Err("unknown receipt outcome".into()),
        };
        Ok(Receipt {
            request_id: r.text(rc::REQUEST_ID).map_err(e)?.to_string(),
            entity,
            requested,
            expected_generation: r.u64(rc::EXPECTED_GENERATION).map_err(e)?,
            outcome,
            generation: r.u64(rc::GENERATION).map_err(e)?,
            effective: entity.effective(requested),
        })
    }
}

/// The owner's state directory (`generations/`, `receipts/`).
pub struct Store {
    dir: PathBuf,
    /// Generations kept besides the current one (older ones are pruned).
    keep: u64,
}

/// A request ID is a single safe path component (the gateway's UUIDs fit).
fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Publishes `bytes` at `path` only if nothing is there: written and synced
/// in a private temporary file first, then hard-linked into place (fails
/// with `AlreadyExists` if another writer won), so a reader never sees a
/// partial record.
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut nonce = [0u8; 8];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut nonce))?;
    let tmp = dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        nonce.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let written = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::hard_link(&tmp, path)
    })();
    let _ = fs::remove_file(&tmp);
    written?;
    fs::File::open(dir)?.sync_all()
}

impl Store {
    pub fn open(dir: impl Into<PathBuf>, keep: u64) -> Result<Self, String> {
        let dir = dir.into();
        for d in [dir.clone(), dir.join("generations"), dir.join("receipts")] {
            match fs::DirBuilder::new().mode(0o700).create(&d) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("{}: {e}", d.display())),
            }
            if !fs::symlink_metadata(&d).is_ok_and(|m| m.is_dir()) {
                return Err(format!("{} is not a real directory", d.display()));
            }
        }
        Ok(Store { dir, keep })
    }

    fn generation_path(&self, n: u64) -> PathBuf {
        self.dir.join("generations").join(format!("{n:020}.dbrec"))
    }

    /// The highest generation on file, or generation 0 when there is none.
    pub fn current(&self) -> Result<Generation, String> {
        match self.highest()? {
            None => Ok(Generation::initial()),
            Some(n) => {
                let bytes = fs::read(self.generation_path(n)).map_err(|e| e.to_string())?;
                let g = Generation::decode(&bytes)?;
                if g.generation != n {
                    return Err(format!(
                        "generation file {n} holds generation {}",
                        g.generation
                    ));
                }
                Ok(g)
            }
        }
    }

    /// Writes generation 0 (every source OFF) unless a generation exists:
    /// from then on the owner's state is on file for the status projection.
    pub fn init(&self, now_ms: u64) -> Result<Generation, String> {
        let current = self.current()?;
        if self.highest()?.is_none() {
            match write_new(&self.generation_path(0), &current.encode(now_ms)?) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("generation 0 not written: {e}")),
            }
        }
        self.current()
    }

    fn highest(&self) -> Result<Option<u64>, String> {
        highest_generation(&self.dir)
    }

    /// Applies one request (or replays its receipt). `now_ms` is the record
    /// time; a clock fault is the caller's refusal, never time 0.
    pub fn request(
        &self,
        request_id: &str,
        entity: impl Into<Entity>,
        requested: Mode,
        expected_generation: u64,
        now_ms: u64,
    ) -> Result<Receipt, String> {
        let entity: Entity = entity.into();
        if !safe_id(request_id) {
            return Err("the request ID is not a safe identifier".into());
        }
        let receipt_path = self
            .dir
            .join("receipts")
            .join(format!("{request_id}.dbrec"));
        let same = |r: &Receipt| {
            r.entity == entity
                && r.requested == requested
                && r.expected_generation == expected_generation
        };
        match fs::read(&receipt_path) {
            Ok(bytes) => {
                let r = Receipt::decode(&bytes)?;
                return if same(&r) {
                    Ok(r)
                } else {
                    Err("the request ID already belongs to another request".into())
                };
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("receipt unreadable: {e}")),
        }
        let current = self.current()?;
        let mut receipt = Receipt {
            request_id: request_id.into(),
            entity,
            requested,
            expected_generation,
            outcome: "REJECTED_STALE",
            generation: current.generation,
            effective: current.effective_of(entity),
        };
        if current.request_id.as_deref() == Some(request_id) {
            // Crash recovery: the generation names this request, the receipt was lost.
            receipt.outcome = "APPLIED";
        } else if current.generation == expected_generation {
            let mut next = current.clone();
            next.generation = current
                .generation
                .checked_add(1)
                .ok_or("generation exhausted")?;
            next.request_id = Some(request_id.into());
            next.set(entity, requested);
            match write_new(
                &self.generation_path(next.generation),
                &next.encode(now_ms)?,
            ) {
                Ok(()) => {
                    receipt.outcome = "APPLIED";
                    receipt.generation = next.generation;
                    receipt.effective = next.effective_of(entity);
                }
                // Another writer took this generation first: stale.
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    let now = self.current()?;
                    receipt.generation = now.generation;
                    receipt.effective = now.effective_of(entity);
                }
                Err(e) => return Err(format!("generation not written: {e}")),
            }
        }
        match write_new(&receipt_path, &receipt.encode(now_ms)?) {
            Ok(()) => {}
            // A concurrent identical request wrote it first: its receipt counts.
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                let r = Receipt::decode(&fs::read(&receipt_path).map_err(|e| e.to_string())?)?;
                return if same(&r) {
                    Ok(r)
                } else {
                    Err("the request ID already belongs to another request".into())
                };
            }
            Err(e) => return Err(format!("receipt not written: {e}")),
        }
        if receipt.applied() {
            self.prune(receipt.generation);
        }
        Ok(receipt)
    }

    /// Removes generations older than `keep` before `current` (best effort).
    fn prune(&self, current: u64) {
        let Some(below) = current.checked_sub(self.keep) else {
            return;
        };
        if let Ok(entries) = fs::read_dir(self.dir.join("generations")) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if let Some(n) = name
                    .to_string_lossy()
                    .strip_suffix(".dbrec")
                    .and_then(|n| n.parse::<u64>().ok())
                    && n < below
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

/// The highest generation number on file in `dir/generations`.
fn highest_generation(dir: &Path) -> Result<Option<u64>, String> {
    let mut highest: Option<u64> = None;
    for entry in fs::read_dir(dir.join("generations")).map_err(|e| e.to_string())? {
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

/// Read-only, for the status projection: the current generation's file and
/// record, validated against the owner's rule; `None` before `init`.
pub fn current_record(dir: &Path) -> Result<Option<(PathBuf, Record)>, String> {
    let Some(n) = highest_generation(dir)? else {
        return Ok(None);
    };
    let path = dir.join("generations").join(format!("{n:020}.dbrec"));
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    if Generation::decode(&bytes)?.generation != n {
        return Err(format!("generation file {n} holds another generation"));
    }
    let record = decode_generation(&bytes)?;
    Ok(Some((path, record)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("bsc-sources-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let s = Store::open(&dir, 8).unwrap();
        (dir, s)
    }

    /// PHASENPLAN §10 / V2 §5: the C2 state component runs in SHADOW, and a
    /// requested LIVE is recorded but never effective.
    #[test]
    fn the_c2_component_is_switchable_and_never_operationally_live() {
        let (dir, s) = store("c2");
        let c2 = ComponentId::StateC2;
        assert_eq!(Entity::parse("BSC_STATE_C2"), Some(Entity::Component(c2)));
        assert_eq!(component_effective(c2, Mode::Off), Effective::Off);
        assert_eq!(component_effective(c2, Mode::Shadow), Effective::Shadow);
        let r = s.request("c2-1", c2, Mode::Shadow, 0, 1).unwrap();
        assert!(r.applied());
        assert_eq!(
            s.current().unwrap().component(c2).effective,
            Effective::Shadow
        );
        let r = s.request("c2-2", c2, Mode::Live, 1, 2).unwrap();
        assert!(r.applied());
        assert_eq!(
            r.effective,
            Effective::Blocked(C2_OPERATIONAL_NOT_AUTHORIZED)
        );
        assert!(
            r.line()
                .contains("source=BSC_STATE_C2 requested=LIVE effective=BLOCKED")
        );
        // The sources are untouched by a component request.
        assert!(
            s.current()
                .unwrap()
                .sources
                .iter()
                .all(|m| m.requested == Mode::Off)
        );
        // A stale component request changes nothing.
        let stale = s.request("c2-3", c2, Mode::Off, 1, 3).unwrap();
        assert_eq!(stale.outcome, "REJECTED_STALE");
        fs::remove_dir_all(&dir).unwrap();
    }

    /// C3: the execution component runs only to the PAPER sink; a
    /// requested LIVE is recorded and never effective.
    #[test]
    fn the_exec_component_is_paper_only_and_never_live() {
        let (dir, s) = store("exec");
        let exec = ComponentId::ExecC3;
        assert_eq!(Entity::parse("BSC_EXEC"), Some(Entity::Component(exec)));
        let r = s
            .request("e-1", ComponentId::StateC2, Mode::Shadow, 0, 1)
            .unwrap();
        assert!(r.applied());
        // A C2 request leaves the exec component OFF.
        assert_eq!(
            s.current().unwrap().component(exec).effective,
            Effective::Off
        );
        let r = s.request("e-2", exec, Mode::Shadow, 1, 2).unwrap();
        assert_eq!(r.effective, Effective::Shadow);
        let r = s.request("e-3", exec, Mode::Live, 2, 3).unwrap();
        assert!(r.applied());
        assert_eq!(
            r.effective,
            Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
        );
        assert!(
            r.line()
                .contains("source=BSC_EXEC requested=LIVE effective=BLOCKED")
        );
        // C2 untouched by the exec request.
        assert_eq!(
            s.current()
                .unwrap()
                .component(ComponentId::StateC2)
                .effective,
            Effective::Shadow
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A generation written before components existed (schema version 1)
    /// still reads, with every component OFF.
    #[test]
    fn a_generation_from_before_components_reads_with_them_off() {
        let (dir, s) = store("v1");
        let mut d = Draft::new(&sm::SCHEMA_V1);
        d.set(sm::GENERATION, 0u64)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        for source in SourceId::ALL {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            e.set(sm::entry::SOURCE_ID, source.name())
                .set(sm::entry::REQUESTED, "OFF")
                .set(sm::entry::EFFECTIVE, "OFF");
            d.push(sm::SOURCES, e.into_nested());
        }
        fs::write(
            dir.join("generations").join(format!("{:020}.dbrec", 0)),
            d.encode().unwrap(),
        )
        .unwrap();
        let g = s.current().unwrap();
        assert_eq!(g.component(ComponentId::StateC2).effective, Effective::Off);
        assert!(current_record(&dir).unwrap().is_some());
        // The next request writes the current schema with the component.
        s.request("after-v1", ComponentId::StateC2, Mode::Shadow, 0, 2)
            .unwrap();
        let (_, r) = current_record(&dir).unwrap().unwrap();
        assert_eq!(r.schema().schema_version, sm::SCHEMA.schema_version);
        assert_eq!(
            r.nested_all(sm::COMPONENTS).unwrap().len(),
            ComponentId::ALL.len()
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// V2 §5: no request makes a source economically live; 48Club is
    /// blocked even in SHADOW; OFF and SHADOW are effective as requested.
    #[test]
    fn the_owner_never_makes_a_source_live() {
        for s in SourceId::ALL {
            assert_eq!(effective(s, Mode::Off), Effective::Off);
            assert_eq!(
                effective(s, Mode::Live),
                Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
            );
        }
        assert_eq!(
            effective(SourceId::PublicNode, Mode::Shadow),
            Effective::Shadow
        );
        assert_eq!(
            effective(SourceId::Club48, Mode::Shadow),
            Effective::Blocked(SOURCE_ACCESS_UNQUALIFIED)
        );
    }

    #[test]
    fn a_request_applies_with_its_generation_and_is_idempotent() {
        let (dir, s) = store("apply");
        assert_eq!(s.current().unwrap(), Generation::initial());
        let r = s
            .request("req-1", SourceId::PublicNode, Mode::Shadow, 0, 1)
            .unwrap();
        assert_eq!(
            (r.outcome, r.generation, r.effective),
            ("APPLIED", 1, Effective::Shadow)
        );
        // The same request again: the same receipt, no second generation.
        assert_eq!(
            s.request("req-1", SourceId::PublicNode, Mode::Shadow, 0, 2)
                .unwrap(),
            r
        );
        assert_eq!(s.current().unwrap().generation, 1);
        // Another payload under the same ID is refused.
        assert!(
            s.request("req-1", SourceId::Drpc, Mode::Shadow, 0, 3)
                .is_err()
        );
        // LIVE is recorded as requested and blocked as effective.
        let r = s
            .request("req-2", SourceId::PublicNode, Mode::Live, 1, 4)
            .unwrap();
        assert_eq!(
            r.effective,
            Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
        );
        let g = s.current().unwrap();
        assert_eq!(g.generation, 2);
        assert_eq!(g.source(SourceId::PublicNode).requested, Mode::Live);
        assert_eq!(g.source(SourceId::Drpc).requested, Mode::Off);
        fs::remove_dir_all(dir).unwrap();
    }

    /// `init` puts generation 0 on file once; the projection reads it.
    #[test]
    fn init_puts_generation_zero_on_file_once() {
        let (dir, s) = store("init");
        assert_eq!(current_record(&dir).unwrap().map(|(p, _)| p), None);
        assert_eq!(s.init(1).unwrap(), Generation::initial());
        let (path, record) = current_record(&dir).unwrap().unwrap();
        assert!(path.ends_with(format!("generations/{:020}.dbrec", 0)));
        assert_eq!(record.u64(sm::GENERATION).unwrap(), 0);
        s.request("i", SourceId::Drpc, Mode::Shadow, 0, 2).unwrap();
        assert_eq!(s.init(3).unwrap().generation, 1, "init never rewinds");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_stale_generation_is_refused_and_the_state_survives_a_restart() {
        let (dir, s) = store("stale");
        s.request("a", SourceId::Drpc, Mode::Shadow, 0, 1).unwrap();
        let r = s
            .request("b", SourceId::BlockReq, Mode::Shadow, 0, 2)
            .unwrap();
        assert_eq!((r.outcome, r.generation), ("REJECTED_STALE", 1));
        assert_eq!(r.effective, Effective::Off, "BlockReq unchanged");
        // A new process (restart) reads the same state.
        let again = Store::open(&dir, 8).unwrap();
        assert_eq!(again.current().unwrap(), s.current().unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    /// Two writers of the same next generation: create-new lets one win.
    #[test]
    fn concurrent_writers_of_one_generation_have_one_winner() {
        let (dir, s) = store("race");
        let outcomes: Vec<&str> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..6)
                .map(|i| {
                    let s = &s;
                    scope.spawn(move || {
                        s.request(&format!("r{i}"), SourceId::Geth, Mode::Shadow, 0, 1)
                            .unwrap()
                            .outcome
                    })
                })
                .collect();
            runs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert_eq!(
            outcomes.iter().filter(|o| **o == "APPLIED").count(),
            1,
            "{outcomes:?}"
        );
        assert_eq!(s.current().unwrap().generation, 1);
        fs::remove_dir_all(dir).unwrap();
    }

    /// A crash after the new generation but before its receipt: the replay
    /// of the same request reports APPLIED from the generation.
    #[test]
    fn a_lost_receipt_is_recovered_from_the_generation() {
        let (dir, s) = store("crash");
        s.request("x", SourceId::Geth, Mode::Shadow, 0, 1).unwrap();
        fs::remove_file(dir.join("receipts/x.dbrec")).unwrap();
        let r = s.request("x", SourceId::Geth, Mode::Shadow, 0, 2).unwrap();
        assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
        fs::remove_dir_all(dir).unwrap();
    }

    /// A generation whose stored effective mode claims more than the rule
    /// (e.g. a hand-edited SHADOW for 48Club) is refused, never trusted.
    #[test]
    fn a_tampered_generation_is_refused() {
        let (dir, s) = store("tamper");
        s.request("t", SourceId::Club48, Mode::Shadow, 0, 1)
            .unwrap();
        let path = dir.join(format!("generations/{:020}.dbrec", 1));
        let bytes = fs::read(&path).unwrap();
        let g = Generation::decode(&bytes).unwrap();
        let mut d = Draft::new(&sm::SCHEMA);
        d.set(sm::GENERATION, g.generation)
            .set(sm::PROVENANCE_TOOL, "bsc_runtime")
            .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        for m in g.sources {
            let mut e = Draft::new(&sm::entry::SCHEMA);
            e.set(sm::entry::SOURCE_ID, m.source.name())
                .set(sm::entry::REQUESTED, m.requested.name())
                .set(sm::entry::EFFECTIVE, "SHADOW");
            d.push(sm::SOURCES, e.into_nested());
        }
        fs::write(&path, d.encode().unwrap()).unwrap();
        assert!(s.current().unwrap_err().contains("not the owner's"));
        fs::remove_dir_all(dir).unwrap();
    }
}
