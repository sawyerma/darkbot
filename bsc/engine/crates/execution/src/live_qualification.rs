//! The LIVE qualification of a scope (plan 28.10., `0a647175`): the
//! evidence the native owner's LIVE gate needs besides the switches and the
//! operator's grant. One binary record, [`LIVE_QUALIFICATION_PATH`] in the
//! repository or snapshot root, names what was qualified (venues, sources,
//! inclusion adapters with their measured findings, the acquisition, the
//! operator's risk decision) and binds each to evidence records by path and
//! SHA-256. Every load verifies every reference again: a moved, changed or
//! undecodable evidence file, an acquisition that is not a G0.8 PASS or a
//! risk decision without evidence counts as not qualified.
//!
//! No record: nothing is qualified, so nothing is live and no plan is
//! released. A record that does not verify is refused as a whole.

use crate::sources::{LiveAdapter, LiveQualification, SourceId, VenueId};
use darkbot_bsc_record::schema::bsc_runtime::live_qualification as lq;
use darkbot_bsc_record::schema::g08;
use darkbot_bsc_record::{Draft, Record, decode};
use darkbot_bsc_trading::submission_policy::capability::CapabilityId;
use darkbot_bsc_trading::submission_policy::qualification::{AdapterQualification, Finding};
use std::fs;
use std::path::{Component, Path};

/// Where the LIVE qualification lies in the repository or snapshot root.
pub const LIVE_QUALIFICATION_PATH: &str = "bsc/config/live_qualification.dbrec";

/// The frozen capability record of a LIVE adapter.
pub fn capability(adapter: LiveAdapter) -> CapabilityId {
    match adapter {
        LiveAdapter::Club48BuilderV1 => CapabilityId::Club48BuilderV1,
        LiveAdapter::BlockRazorBuilderV1 => CapabilityId::BlockRazorBuilderV1,
    }
}

/// A verified LIVE qualification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    /// What the owner's LIVE gate counts.
    pub view: LiveQualification,
    /// What a release plan may rely on, per adapter.
    pub adapters: Vec<AdapterQualification>,
    /// SHA-256 of the qualification record itself.
    pub sha256: [u8; 32],
}

/// One evidence reference, verified: the file at `path` (relative, inside
/// `root`) has this SHA-256 and decodes as the named schema.
fn verified_evidence(root: &Path, r: &Record) -> Result<Record, String> {
    use lq::evidence as k;
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let path = r.text(k::PATH).map_err(e)?;
    let relative = Path::new(path);
    if path.is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(format!(
            "evidence path {path:?} is not a plain path inside the root"
        ));
    }
    let bytes = fs::read(root.join(relative)).map_err(|err| format!("evidence {path}: {err}"))?;
    if darkbot_bsc_record::sha256(&bytes) != r.b256(k::SHA256).map_err(e)? {
        return Err(format!("evidence {path} changed since it was qualified"));
    }
    let name = r.text(k::SCHEMA_NAME).map_err(e)?;
    let decoded = darkbot_bsc_record::schema::all()
        .into_iter()
        .filter(|s| s.name == name)
        .find_map(|s| decode(s, &bytes).ok())
        .ok_or_else(|| format!("evidence {path} is not a {name} record"))?;
    Ok(decoded)
}

/// Every evidence reference of `row` under `tag`, verified; at least one.
fn verified_all(root: &Path, row: &Record, tag: u16) -> Result<Vec<Record>, String> {
    let refs = row.nested_all(tag).map_err(|x| x.0)?;
    if refs.is_empty() {
        return Err("a qualification without evidence".into());
    }
    refs.iter().map(|r| verified_evidence(root, r)).collect()
}

/// Verifies the qualification record `bytes` against the evidence in `root`.
pub fn verify(root: &Path, bytes: &[u8]) -> Result<Verified, String> {
    let r = decode(&lq::SCHEMA, bytes).map_err(|e| e.to_string())?;
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let mut view = LiveQualification::default();
    for row in r.nested_all(lq::ITEMS).map_err(e)? {
        use lq::item as k;
        let id = row.text(k::ID).map_err(e)?;
        verified_all(root, &row, k::EVIDENCE)?;
        match row.text(k::KIND).map_err(e)? {
            "VENUE" => {
                let venue = VenueId::parse(id).ok_or(format!("unknown venue {id}"))?;
                if venue != VenueId::Flap {
                    return Err(format!("{id} has no execution path to qualify"));
                }
                if view.venues.contains(&venue) {
                    return Err(format!("{id} is qualified twice"));
                }
                view.venues.push(venue);
            }
            "SOURCE" => {
                let source = SourceId::parse(id).ok_or(format!("unknown source {id}"))?;
                if view.sources.contains(&source) {
                    return Err(format!("{id} is qualified twice"));
                }
                view.sources.push(source);
            }
            other => return Err(format!("unknown qualification kind {other}")),
        }
    }
    let mut adapters = Vec::new();
    for row in r.nested_all(lq::ADAPTERS).map_err(e)? {
        use lq::adapter as k;
        let name = row.text(k::ADAPTER).map_err(e)?;
        let adapter = LiveAdapter::parse(name).ok_or(format!("unknown adapter {name}"))?;
        if view.adapters.contains(&adapter) {
            return Err(format!("{name} is qualified twice"));
        }
        verified_all(root, &row, k::EVIDENCE)?;
        let findings = row
            .texts(k::FINDINGS)
            .map_err(e)?
            .into_iter()
            .map(|f| Finding::parse(f).ok_or(format!("{name}: unknown finding {f}")))
            .collect::<Result<Vec<_>, _>>()?;
        if findings.is_empty() {
            return Err(format!("{name}: a qualification without findings"));
        }
        adapters.push(AdapterQualification::from_findings(
            capability(adapter),
            &findings,
        )?);
        view.adapters.push(adapter);
    }
    if let Some(row) = r.opt_nested(lq::ACQUISITION).map_err(e)? {
        let result = verified_evidence(root, &row)?;
        if result.schema().name != g08::result::SCHEMA.name {
            return Err("the acquisition is not a G0.8 result".into());
        }
        if result.text(g08::result::VERDICT).map_err(e)? != "PASS" {
            return Err("the G0.8 result is not an acquisition PASS".into());
        }
        view.acquisition = true;
    }
    if let Some(row) = r.opt_nested(lq::RISK).map_err(e)? {
        use lq::risk as k;
        if row.text(k::DECIDED_BY).map_err(e)?.trim().is_empty()
            || row.text(k::STATEMENT).map_err(e)?.trim().is_empty()
            || row.u64(k::DECIDED_UNIX_MS).map_err(e)? == 0
        {
            return Err("a risk decision names who decided what, and when".into());
        }
        verified_all(root, &row, k::EVIDENCE)?;
        view.risk_decision = true;
    }
    Ok(Verified {
        view,
        adapters,
        sha256: darkbot_bsc_record::sha256(bytes),
    })
}

/// The LIVE qualification on file in `root`: `None` when there is none.
pub fn load(root: &Path) -> Result<Option<Verified>, String> {
    match fs::read(root.join(LIVE_QUALIFICATION_PATH)) {
        Ok(bytes) => verify(root, &bytes).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{LIVE_QUALIFICATION_PATH}: {e}")),
    }
}

/// What the LIVE gate counts in `root`, and why it counts nothing when the
/// record does not verify (none: nothing on file or it verified).
pub fn view(root: &Path) -> (Verified, Option<String>) {
    let none = Verified {
        view: LiveQualification::default(),
        adapters: Vec::new(),
        sha256: [0; 32],
    };
    match load(root) {
        Ok(Some(v)) => (v, None),
        Ok(None) => (none, None),
        Err(e) => (none, Some(e)),
    }
}

/// What a qualification record names, for [`build`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spec {
    pub venues: Vec<(VenueId, Vec<String>)>,
    pub sources: Vec<(SourceId, Vec<String>)>,
    pub adapters: Vec<(LiveAdapter, Vec<Finding>, Vec<String>)>,
    /// The G0.8 result record.
    pub acquisition: Option<String>,
    /// Who decided, when, what, and the evidence it rests on.
    pub risk: Option<(String, u64, String, Vec<String>)>,
}

/// The qualification record of `spec`: each evidence path is read in
/// `root`, its SHA-256 and schema bound; the result verifies as it is.
pub fn build(root: &Path, spec: &Spec, now_ms: u64) -> Result<Vec<u8>, String> {
    let evidence = |path: &str| -> Result<darkbot_bsc_record::Value, String> {
        let bytes = fs::read(root.join(path)).map_err(|e| format!("evidence {path}: {e}"))?;
        let schema = darkbot_bsc_record::schema::all()
            .into_iter()
            .find(|s| decode(s, &bytes).is_ok())
            .ok_or_else(|| format!("evidence {path} is no known record"))?;
        let mut d = Draft::new(&lq::evidence::SCHEMA);
        d.set(lq::evidence::PATH, path)
            .set(
                lq::evidence::SHA256,
                darkbot_bsc_record::Value::B256(darkbot_bsc_record::sha256(&bytes)),
            )
            .set(lq::evidence::SCHEMA_NAME, schema.name);
        Ok(d.into_nested())
    };
    let mut d = Draft::new(&lq::SCHEMA);
    d.set(lq::PROVENANCE_TOOL, "bsc_runtime")
        .set(lq::PROVENANCE_OBSERVED_UNIX_MS, now_ms);
    let items = spec
        .venues
        .iter()
        .map(|(v, p)| ("VENUE", v.name(), p))
        .chain(spec.sources.iter().map(|(s, p)| ("SOURCE", s.name(), p)));
    for (kind, id, paths) in items {
        let mut row = Draft::new(&lq::item::SCHEMA);
        row.set(lq::item::KIND, kind).set(lq::item::ID, id);
        for path in paths {
            row.push(lq::item::EVIDENCE, evidence(path)?);
        }
        d.push(lq::ITEMS, row.into_nested());
    }
    for (adapter, findings, paths) in &spec.adapters {
        let mut row = Draft::new(&lq::adapter::SCHEMA);
        row.set(lq::adapter::ADAPTER, adapter.name());
        for f in findings {
            row.push(lq::adapter::FINDINGS, f.name());
        }
        for path in paths {
            row.push(lq::adapter::EVIDENCE, evidence(path)?);
        }
        d.push(lq::ADAPTERS, row.into_nested());
    }
    if let Some(path) = &spec.acquisition {
        d.set(lq::ACQUISITION, evidence(path)?);
    }
    if let Some((by, at, statement, paths)) = &spec.risk {
        let mut row = Draft::new(&lq::risk::SCHEMA);
        row.set(lq::risk::DECIDED_BY, by.as_str())
            .set(lq::risk::DECIDED_UNIX_MS, *at)
            .set(lq::risk::STATEMENT, statement.as_str());
        for path in paths {
            row.push(lq::risk::EVIDENCE, evidence(path)?);
        }
        d.set(lq::RISK, row.into_nested());
    }
    let bytes = d.encode().map_err(|e| e.to_string())?;
    verify(root, &bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkbot_bsc_record::{Kind, Presence, Schema, Value};
    use darkbot_bsc_trading::submission_policy::capability::Knowledge;
    use darkbot_bsc_trading::submission_policy::qualification::{FeatureName, PropertyName};
    use std::path::PathBuf;

    /// Every required field of `s` with a fixed value.
    fn filled(s: &'static Schema) -> Draft {
        let mut d = Draft::new(s);
        for f in s.fields.iter().filter(|f| f.presence == Presence::Required) {
            let v = match f.kind {
                Kind::Bool => Value::Bool(false),
                Kind::U64 => Value::U64(1),
                Kind::I64 => Value::I64(1),
                Kind::U256 => Value::U256([0; 32]),
                Kind::Address => Value::Address([1; 20]),
                Kind::B256 => Value::B256([2; 32]),
                Kind::Bytes => Value::Bytes(Vec::new()),
                Kind::Text => Value::Text("fixture".into()),
                Kind::Nested => match f.nested {
                    Some(n) => filled(n).into_nested(),
                    None => continue,
                },
            };
            d.set(f.tag, v);
        }
        d
    }

    /// A root with a G0.8 result of `verdict` and one other evidence record.
    fn fixture(tag: &str, verdict: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bsc-live-q-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("ev")).unwrap();
        let mut g08 = filled(&g08::result::SCHEMA);
        g08.set(g08::result::VERDICT, verdict);
        fs::write(root.join("ev/g08.dbrec"), g08.encode().unwrap()).unwrap();
        let other = filled(&darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot::SCHEMA);
        fs::write(root.join("ev/measure.dbrec"), other.encode().unwrap()).unwrap();
        root
    }

    fn findings() -> Vec<Finding> {
        [
            (PropertyName::PreservesOrder, Knowledge::Yes),
            (PropertyName::Atomic, Knowledge::Yes),
            (PropertyName::SameBlock, Knowledge::Yes),
            (PropertyName::ExtractsTransactions, Knowledge::No),
            (PropertyName::ProviderRebroadcast, Knowledge::No),
            (PropertyName::ProviderPublicFallback, Knowledge::No),
            (PropertyName::ReachesOtherBuilders, Knowledge::No),
            (PropertyName::PublicMempoolExposure, Knowledge::No),
        ]
        .into_iter()
        .map(|(p, k)| Finding::Property(p, k))
        .chain([
            Finding::Feature(FeatureName::NoMerge),
            Finding::Feature(FeatureName::PositionFirst),
            Finding::Feature(FeatureName::MaxBlockNumber),
            Finding::SendTime,
        ])
        .collect()
    }

    fn spec() -> Spec {
        let ev = || vec!["ev/measure.dbrec".to_string()];
        Spec {
            venues: vec![(VenueId::Flap, ev())],
            sources: vec![(SourceId::PublicNode, ev())],
            adapters: vec![(LiveAdapter::Club48BuilderV1, findings(), ev())],
            acquisition: Some("ev/g08.dbrec".into()),
            risk: Some((
                "operator".into(),
                1,
                "accepts the measured scope".into(),
                ev(),
            )),
        }
    }

    /// Plan 28.10.: a qualification verified against its evidence gives the
    /// gate exactly its scope and the release plans its findings.
    #[test]
    fn a_verified_qualification_names_exactly_its_scope() {
        let root = fixture("ok", "PASS");
        let bytes = build(&root, &spec(), 1).unwrap();
        fs::create_dir_all(root.join("bsc/config")).unwrap();
        fs::write(root.join(LIVE_QUALIFICATION_PATH), &bytes).unwrap();
        let v = load(&root).unwrap().unwrap();
        assert_eq!(
            v.view,
            LiveQualification {
                venues: vec![VenueId::Flap],
                sources: vec![SourceId::PublicNode],
                adapters: vec![LiveAdapter::Club48BuilderV1],
                acquisition: true,
                risk_decision: true,
            }
        );
        assert_eq!(v.adapters.len(), 1);
        assert_eq!(v.adapters[0].id(), CapabilityId::Club48BuilderV1);
        assert!(v.adapters[0].send_time_established());
        assert_eq!(v.sha256, darkbot_bsc_record::sha256(&bytes));
        // Nothing on file: nothing qualified, and that is no error.
        let empty = std::env::temp_dir().join(format!("bsc-live-q-none-{}", std::process::id()));
        assert_eq!(load(&empty).unwrap(), None);
        assert_eq!(view(&empty).0.view, LiveQualification::default());
        fs::remove_dir_all(root).unwrap();
    }

    /// Changed or moved evidence, an acquisition that is not a G0.8 PASS, a
    /// venue without an execution path or a risk decision without its
    /// author: the whole record counts nothing.
    #[test]
    fn evidence_that_does_not_verify_qualifies_nothing() {
        let root = fixture("bad", "PASS");
        let bytes = build(&root, &spec(), 1).unwrap();
        // Evidence changed after the qualification.
        fs::write(root.join("ev/measure.dbrec"), b"changed").unwrap();
        assert!(
            verify(&root, &bytes)
                .unwrap_err()
                .contains("changed since it was qualified")
        );
        fs::create_dir_all(root.join("bsc/config")).unwrap();
        fs::write(root.join(LIVE_QUALIFICATION_PATH), &bytes).unwrap();
        let (v, why) = view(&root);
        assert_eq!(v.view, LiveQualification::default());
        assert!(why.is_some());
        fs::remove_dir_all(&root).unwrap();
        // A G0.8 result that is not a PASS.
        let root = fixture("adapt", "ADAPT");
        assert!(
            build(&root, &spec(), 1)
                .unwrap_err()
                .contains("not an acquisition PASS")
        );
        // Four has no execution path; a path leaving the root; no author.
        let root2 = fixture("scope", "PASS");
        let mut four = spec();
        four.venues = vec![(VenueId::FourClassic, vec!["ev/measure.dbrec".into()])];
        assert!(
            build(&root2, &four, 1)
                .unwrap_err()
                .contains("no execution path")
        );
        let mut escape = spec();
        escape.sources = vec![(SourceId::Drpc, vec!["../outside.dbrec".into()])];
        assert!(build(&root2, &escape, 1).is_err());
        let mut anonymous = spec();
        anonymous.risk = Some((" ".into(), 1, "x".into(), vec!["ev/measure.dbrec".into()]));
        assert!(
            build(&root2, &anonymous, 1)
                .unwrap_err()
                .contains("who decided")
        );
        let mut bare = spec();
        bare.adapters = vec![(LiveAdapter::Club48BuilderV1, findings(), Vec::new())];
        assert!(
            build(&root2, &bare, 1)
                .unwrap_err()
                .contains("without evidence")
        );
        let mut acquisition = spec();
        acquisition.acquisition = Some("ev/measure.dbrec".into());
        assert!(
            build(&root2, &acquisition, 1)
                .unwrap_err()
                .contains("not a G0.8 result")
        );
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(root2).unwrap();
    }
}
