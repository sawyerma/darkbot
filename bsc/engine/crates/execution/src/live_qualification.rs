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
//! BSC3-049: each role takes only its own record schemas, bound to what it
//! qualifies: a venue the G0.1 freeze of the pinned deployment, a source
//! the G0.4 verdict `CONTINUE` for Flap on confirmed, pre-committed
//! criteria in whose census it delivered an actionable launch first, an
//! adapter a G0.7 attempt it accepted, the acquisition the G0.8 PASS of the
//! G0.7 attempt it names, of the pinned deployment, with its buyer and the
//! builders it was sent through, the risk decision the economics snapshot
//! of the pinned chain or the acquisition's own G0.8 result it rests on,
//! made for one grant risk scope
//! ([`LiveGrant::risk_scope_sha256`](crate::sources::LiveGrant::risk_scope_sha256)).
//! An adapter's findings stay what the qualification states: one attempt
//! proves no provider's ordering, atomicity or send time.
//!
//! No record: nothing is qualified, so nothing is live and no plan is
//! released. A record that does not verify is refused as a whole.

use crate::sources::{LiveAdapter, LiveQualification, SourceId, VenueId};
use darkbot_bsc_record::schema::bsc_census::verdict as gv;
use darkbot_bsc_record::schema::bsc_economics::economics_snapshot;
use darkbot_bsc_record::schema::bsc_flap_freeze::flap_freeze as ff;
use darkbot_bsc_record::schema::bsc_runtime::live_qualification as lq;
use darkbot_bsc_record::schema::{g06a, g07, g08};
use darkbot_bsc_record::{Draft, Record, Schema, decode};
use darkbot_bsc_trading::submission_policy::capability::CapabilityId;
use darkbot_bsc_trading::submission_policy::qualification::{AdapterQualification, Finding};
use darkbot_bsc_trading::venues::flap::deployment::{FlapDeployment, bsc_mainnet_v5_27_0};
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
    /// The buyer of the proven acquisition: the LIVE key must be it.
    pub acquisition_buyer: Option<[u8; 20]>,
    /// BSC3-012: the SHA-256 of the `bsc_runtime` build qualified (none
    /// before v3: no build is).
    pub runtime_sha256: Option<[u8; 32]>,
}

/// The schemas of a venue's evidence: the G0.1 freeze (v2, with its
/// upgrade check).
const VENUE_EVIDENCE: &[&Schema] = &[&ff::V2];
/// A source's: the G0.4 verdict (v3, with the rated census's sources).
const SOURCE_EVIDENCE: &[&Schema] = &[&gv::SCHEMA];
/// The acquisition: the G0.8 result, and the G0.7 attempt it rates; an
/// adapter's: a G0.7 attempt.
const ACQUISITION_EVIDENCE: &[&Schema] = &[&g08::result::SCHEMA];
const ATTEMPT_EVIDENCE: &[&Schema] = &[&g07::evidence::SCHEMA];
/// The risk decision's: the economics snapshot and the G0.8 result.
const RISK_EVIDENCE: &[&Schema] = &[&economics_snapshot::SCHEMA, &g08::result::SCHEMA];

/// One evidence reference, verified: the file at `path` (relative, inside
/// `root`) has this SHA-256 and decodes as the named schema, one of
/// `allowed` (none: any known record).
fn verified_evidence(
    root: &Path,
    r: &Record,
    allowed: Option<&[&'static Schema]>,
) -> Result<Record, String> {
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
    let schemas = allowed.map_or_else(darkbot_bsc_record::schema::all, <[_]>::to_vec);
    if !schemas.iter().any(|s| s.name == name) {
        return Err(format!(
            "evidence {path} is a {name} record, not {}",
            schemas
                .iter()
                .map(|s| s.name)
                .collect::<Vec<_>>()
                .join(" or ")
        ));
    }
    let decoded = schemas
        .into_iter()
        .filter(|s| s.name == name)
        .find_map(|s| decode(s, &bytes).ok())
        .ok_or_else(|| format!("evidence {path} is not a {name} record"))?;
    Ok(decoded)
}

/// Every evidence reference of `row` under `tag`, verified against
/// `allowed`; at least one.
fn verified_all(
    root: &Path,
    row: &Record,
    tag: u16,
    allowed: Option<&[&'static Schema]>,
) -> Result<Vec<Record>, String> {
    let refs = row.nested_all(tag).map_err(|x| x.0)?;
    if refs.is_empty() {
        return Err("a qualification without evidence".into());
    }
    refs.iter()
        .map(|r| verified_evidence(root, r, allowed))
        .collect()
}

fn re(x: darkbot_bsc_record::RecordError) -> String {
    x.0
}

/// A G0.1 freeze of `pin`: its chain, Portal and version, the pin read
/// again, one implementation over the scanned window, the pinned one.
fn freeze_binds(f: &Record, pin: &FlapDeployment) -> Result<(), String> {
    let check = f.nested_all(ff::UPGRADE_CHECK).map_err(re)?;
    let upgrade = check.first().ok_or("a freeze without its upgrade check")?;
    let bound = f.u64(ff::CHAIN_ID).map_err(re)? == pin.chain_id
        && f.address(ff::PORTAL).map_err(re)? == pin.portal.into_array()
        && f.opt_text(ff::PORTAL_VERSION).map_err(re)? == Some(pin.portal_version)
        && f.bool(ff::PIN_RECHECKED).map_err(re)?
        && upgrade.text(ff::upgrade_check::STATUS).map_err(re)? == "ONE_IMPLEMENTATION"
        && upgrade
            .opt_address(ff::upgrade_check::LAST_IMPLEMENTATION)
            .map_err(re)?
            == Some(pin.portal_implementation.into_array());
    if !bound {
        return Err(format!(
            "the freeze is not of the pinned deployment (Portal {} at {})",
            pin.portal_version, pin.portal
        ));
    }
    Ok(())
}

/// A G0.4 verdict that qualifies `source`: confirmed criteria committed
/// before the campaign, `CONTINUE` for the campaign and for Flap, and in the
/// census it rated `source` delivered at least one actionable launch first.
fn source_binds(v: &Record, source: SourceId) -> Result<(), String> {
    let mut flap = Vec::new();
    for row in v.nested_all(gv::VENUES).map_err(re)? {
        if row.text(gv::venue::VENUE).map_err(re)? == gv::RATED_VENUES[0] {
            flap.push(row.text(gv::venue::VERDICT).map_err(re)? == gv::CONTINUE);
        }
    }
    let mut firsts = Vec::new();
    for row in v.nested_all(gv::SOURCES).map_err(re)? {
        use darkbot_bsc_record::schema::bsc_census::census::source as cs;
        if row.text(cs::SOURCE_ID).map_err(re)? == source.name() {
            firsts.push(row.u64(cs::FIRST_FULL_BEFORE).map_err(re)?);
        }
    }
    // One row each, as the verdict writes them.
    let (flap, first) = match (flap.as_slice(), firsts.as_slice()) {
        ([flap], [first]) => (*flap, *first),
        _ => (false, 0),
    };
    let bound = v.text(gv::CRITERIA_STATE).map_err(re)? == gv::CONFIRMED
        && v.text(gv::VERDICT).map_err(re)? == gv::CONTINUE
        && v.opt_b256(gv::PRECOMMIT_SHA256).map_err(re)?.is_some()
        && flap
        && first > 0;
    if !bound {
        return Err(
            "no G0.4 verdict CONTINUE for Flap on committed criteria in whose census it delivered an actionable launch first"
                .into(),
        );
    }
    Ok(())
}

/// The builders that accepted the G0.7 attempt `attempt` (bytes may have
/// left, a 2xx answer with a result hash, no error, error code or message
/// and nothing undecodable). Accepted is not included: which builder
/// delivered is not on chain.
fn accepted(attempt: &Record) -> Result<Vec<LiveAdapter>, String> {
    use g07::evidence as a;
    let mut sent = Vec::new();
    if let Some(send) = attempt.opt_nested(a::SEND).map_err(re)? {
        use g07::provider_result as pr;
        for (tag, adapter) in [
            (a::send::FORTYEIGHTCLUB, LiveAdapter::Club48BuilderV1),
            (a::send::BLOCKRAZOR, LiveAdapter::BlockRazorBuilderV1),
        ] {
            if let Some(p) = send.opt_nested(tag).map_err(re)?
                && p.bool(pr::BYTES_MAY_HAVE_LEFT).map_err(re)?
                && p.opt_u64(pr::HTTP_STATUS_CODE)
                    .map_err(re)?
                    .is_some_and(|c| (200..300).contains(&c))
                && p.opt_b256(pr::RPC_RESULT_HASH).map_err(re)?.is_some()
                && p.opt_text(pr::ERROR).map_err(re)?.is_none()
                && p.opt_i64(pr::RPC_ERROR_CODE).map_err(re)?.is_none()
                && p.opt_text(pr::RPC_ERROR_MESSAGE).map_err(re)?.is_none()
                && p.opt_text(pr::RESPONSE_UNDECODABLE).map_err(re)?.is_none()
            {
                sent.push(adapter);
            }
        }
    }
    Ok(sent)
}

/// A G0.7 attempt of the pinned deployment: its chain and Portal, its
/// preflight's chain, Portal, version, implementation and code.
fn attempt_of(attempt: &Record, pin: &FlapDeployment) -> Result<(), String> {
    use g06a::preflight as pf;
    use g07::evidence as a;
    if attempt.text(a::GATE).map_err(re)? != "G0.7" {
        return Err("not a G0.7 attempt".into());
    }
    let preflight = attempt
        .opt_nested(a::PREFLIGHT)
        .map_err(re)?
        .ok_or("a G0.7 attempt without its preflight")?;
    if attempt.u64(a::CHAIN_ID).map_err(re)? != pin.chain_id
        || attempt.address(a::PORTAL).map_err(re)? != pin.portal.into_array()
        || preflight.u64(pf::CHAIN_ID).map_err(re)? != pin.chain_id
        || preflight.address(pf::PORTAL).map_err(re)? != pin.portal.into_array()
        || preflight.text(pf::PORTAL_VERSION).map_err(re)? != pin.portal_version
        || preflight.address(pf::PORTAL_IMPLEMENTATION).map_err(re)?
            != pin.portal_implementation.into_array()
        || preflight
            .b256(pf::PORTAL_IMPLEMENTATION_CODE_SHA256)
            .map_err(re)?
            != pin.portal_implementation_runtime_sha256
    {
        return Err(format!(
            "the G0.7 attempt is not of the pinned deployment (Portal {} at {})",
            pin.portal_version, pin.portal
        ));
    }
    Ok(())
}

/// The acquisition: a G0.8 PASS of the G0.7 attempt `attempt` (its file's
/// SHA-256 `attempt_sha256`), the same attempt, buy, token and buyer, of
/// the pinned deployment. Returns the attempt's buyer and the builders that
/// accepted it ([`accepted`]).
fn acquisition_binds(
    result: &Record,
    attempt: &Record,
    attempt_sha256: [u8; 32],
    pin: &FlapDeployment,
) -> Result<([u8; 20], Vec<LiveAdapter>), String> {
    use g07::evidence as a;
    use g08::result as r;
    if result.text(r::GATE).map_err(re)? != "G0.8" {
        return Err("the acquisition is not a G0.8 result".into());
    }
    if result.text(r::VERDICT).map_err(re)? != "PASS" {
        return Err("the G0.8 result is not an acquisition PASS".into());
    }
    if result.b256(r::G07_EVIDENCE_SHA256).map_err(re)? != attempt_sha256
        || attempt.text(a::GATE).map_err(re)? != "G0.7"
        || result.text(r::ATTEMPT_ID).map_err(re)? != attempt.text(a::ATTEMPT_ID).map_err(re)?
        || result.b256(r::BUY_TX).map_err(re)? != attempt.b256(a::BUY_TX_HASH).map_err(re)?
        || result.address(r::TOKEN).map_err(re)? != attempt.address(a::TOKEN).map_err(re)?
        || result.address(r::BUYER).map_err(re)? != attempt.address(a::BUYER).map_err(re)?
    {
        return Err("the G0.8 result does not rate this G0.7 attempt".into());
    }
    attempt_of(attempt, pin)?;
    Ok((attempt.address(a::BUYER).map_err(re)?, accepted(attempt)?))
}

/// Verifies the qualification record `bytes` against the evidence in `root`.
pub fn verify(root: &Path, bytes: &[u8]) -> Result<Verified, String> {
    // A v1 record names no G0.7 attempt: its acquisition verifies nothing;
    // a v1 or v2 record names no runtime build: it qualifies none; before
    // v4 a risk decision names no grant scope: it decides for none.
    let (r, v1, runtime_sha256, scoped) = match decode(&lq::SCHEMA, bytes) {
        Ok(r) => {
            let build = r.b256(lq::RUNTIME_SHA256).map_err(|x| x.0)?;
            (r, false, Some(build), true)
        }
        Err(e4) => match decode(&lq::V3, bytes) {
            Ok(r) => {
                let build = r.b256(lq::RUNTIME_SHA256).map_err(|x| x.0)?;
                (r, false, Some(build), false)
            }
            Err(_) => match decode(&lq::V2, bytes) {
                Ok(r) => (r, false, None, false),
                Err(_) => (
                    decode(&lq::V1, bytes).map_err(|_| e4.to_string())?,
                    true,
                    None,
                    false,
                ),
            },
        },
    };
    let e = |x: darkbot_bsc_record::RecordError| x.0;
    let pin = bsc_mainnet_v5_27_0();
    let mut view = LiveQualification::default();
    let mut acquisition_buyer = None;
    for row in r.nested_all(lq::ITEMS).map_err(e)? {
        use lq::item as k;
        let id = row.text(k::ID).map_err(e)?;
        match row.text(k::KIND).map_err(e)? {
            "VENUE" => {
                let venue = VenueId::parse(id).ok_or(format!("unknown venue {id}"))?;
                if venue != VenueId::Flap {
                    return Err(format!("{id} has no execution path to qualify"));
                }
                if view.venues.contains(&venue) {
                    return Err(format!("{id} is qualified twice"));
                }
                for f in verified_all(root, &row, k::EVIDENCE, Some(VENUE_EVIDENCE))? {
                    freeze_binds(&f, &pin).map_err(|why| format!("{id}: {why}"))?;
                }
                view.venues.push(venue);
            }
            "SOURCE" => {
                let source = SourceId::parse(id).ok_or(format!("unknown source {id}"))?;
                if view.sources.contains(&source) {
                    return Err(format!("{id} is qualified twice"));
                }
                for v in verified_all(root, &row, k::EVIDENCE, Some(SOURCE_EVIDENCE))? {
                    source_binds(&v, source).map_err(|why| format!("{id}: {why}"))?;
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
        for attempt in verified_all(root, &row, k::EVIDENCE, Some(ATTEMPT_EVIDENCE))? {
            attempt_of(&attempt, &pin).map_err(|why| format!("{name}: {why}"))?;
            if !accepted(&attempt)?.contains(&adapter) {
                return Err(format!("{name}: a G0.7 attempt it did not accept"));
            }
        }
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
    let mut acquisition_sha256 = None;
    if let Some(row) = r.opt_nested(lq::ACQUISITION).map_err(e)? {
        let result = verified_evidence(root, &row, Some(ACQUISITION_EVIDENCE))?;
        acquisition_sha256 = Some(row.b256(lq::evidence::SHA256).map_err(e)?);
        let attempt_ref = match v1 {
            true => None,
            false => r.opt_nested(lq::ACQUISITION_ATTEMPT).map_err(e)?,
        }
        .ok_or("the acquisition names no G0.7 attempt: qualify again")?;
        let attempt = verified_evidence(root, &attempt_ref, Some(ATTEMPT_EVIDENCE))?;
        let attempt_sha256 = attempt_ref.b256(lq::evidence::SHA256).map_err(e)?;
        let (buyer, sent) = acquisition_binds(&result, &attempt, attempt_sha256, &pin)?;
        view.acquisition = true;
        view.acquisition_adapters = sent;
        acquisition_buyer = Some(buyer);
    }
    if let Some(row) = r.opt_nested(lq::RISK).map_err(e)? {
        use lq::risk as k;
        if row.text(k::DECIDED_BY).map_err(e)?.trim().is_empty()
            || row.text(k::STATEMENT).map_err(e)?.trim().is_empty()
            || row.u64(k::DECIDED_UNIX_MS).map_err(e)? == 0
        {
            return Err("a risk decision names who decided what, and when".into());
        }
        // BSC3-049 review M: the G0.8 result it rests on is the
        // acquisition's own (a PASS bound above), the economics snapshot one
        // of the pinned chain.
        let refs = row.nested_all(k::EVIDENCE).map_err(e)?;
        if refs.is_empty() {
            return Err("a qualification without evidence".into());
        }
        for ev in refs {
            let record = verified_evidence(root, &ev, Some(RISK_EVIDENCE))?;
            let bound =
                if ev.text(lq::evidence::SCHEMA_NAME).map_err(e)? == g08::result::SCHEMA.name {
                    Some(ev.b256(lq::evidence::SHA256).map_err(e)?) == acquisition_sha256
                } else {
                    record.u64(economics_snapshot::CHAIN_ID).map_err(e)? == pin.chain_id
                };
            if !bound {
                return Err(
                    "the risk decision rests on a G0.8 result other than the acquisition's, or on another chain's economics"
                        .into(),
                );
            }
        }
        view.risk_decision = true;
        view.risk_scope_sha256 = match scoped {
            true => Some(row.b256(k::SCOPE_SHA256).map_err(e)?),
            false => None,
        };
    }
    Ok(Verified {
        view,
        adapters,
        sha256: darkbot_bsc_record::sha256(bytes),
        acquisition_buyer,
        runtime_sha256,
    })
}

/// BSC3-012: the SHA-256 of this process's executable, read once (at the
/// first call, the runtime's start): the build a LIVE qualification must
/// name. A binary replaced on disk later is not what runs.
pub fn running_build() -> Result<[u8; 32], String> {
    static BUILD: std::sync::OnceLock<Result<[u8; 32], String>> = std::sync::OnceLock::new();
    BUILD
        .get_or_init(|| {
            let exe = std::env::current_exe().map_err(|e| format!("executable: {e}"))?;
            let bytes = fs::read(&exe).map_err(|e| format!("{}: {e}", exe.display()))?;
            Ok(darkbot_bsc_record::sha256(&bytes))
        })
        .clone()
}

/// BSC3-012: why the LIVE path may not arm in build `own` under `q`, if
/// it may not.
pub fn build_refusal(own: &Result<[u8; 32], String>, q: &Verified) -> Option<String> {
    use crate::sources::LIVE_BUILD_UNQUALIFIED as WHY;
    use alloy_primitives::hex::encode;
    match (own, q.runtime_sha256) {
        (Ok(own), Some(qualified)) if *own == qualified => None,
        (Ok(own), Some(qualified)) => Some(format!(
            "{WHY}: this build {} is not the qualified {}",
            encode(own),
            encode(qualified)
        )),
        (Ok(_), None) => Some(format!(
            "{WHY}: the qualification names no runtime build: qualify again"
        )),
        (Err(e), _) => Some(format!("{WHY}: this build cannot be read: {e}")),
    }
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
        acquisition_buyer: None,
        runtime_sha256: None,
    };
    match load(root) {
        Ok(Some(v)) => (v, None),
        Ok(None) => (none, None),
        Err(e) => (none, Some(e)),
    }
}

/// The operator's risk decision a qualification names: who decided, when,
/// what, for which grant risk scope (BSC3-049), and the evidence it rests
/// on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskSpec {
    pub decided_by: String,
    pub decided_unix_ms: u64,
    pub statement: String,
    /// [`LiveGrant::risk_scope_sha256`](crate::sources::LiveGrant::risk_scope_sha256)
    /// of the scope it is decided for.
    pub scope_sha256: [u8; 32],
    pub evidence: Vec<String>,
}

/// What a qualification record names, for [`build`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spec {
    pub venues: Vec<(VenueId, Vec<String>)>,
    pub sources: Vec<(SourceId, Vec<String>)>,
    pub adapters: Vec<(LiveAdapter, Vec<Finding>, Vec<String>)>,
    /// The G0.8 result record and the G0.7 attempt record it rates.
    pub acquisition: Option<(String, String)>,
    pub risk: Option<RiskSpec>,
    /// BSC3-012: the SHA-256 of the `bsc_runtime` build it qualifies.
    pub runtime_sha256: Option<[u8; 32]>,
}

/// The qualification record of `spec`: each evidence path is read in
/// `root`, its SHA-256 and schema bound; the result verifies as it is.
pub fn build(root: &Path, spec: &Spec, now_ms: u64) -> Result<Vec<u8>, String> {
    let evidence = |path: &str,
                    allowed: Option<&[&'static Schema]>|
     -> Result<darkbot_bsc_record::Value, String> {
        let bytes = fs::read(root.join(path)).map_err(|e| format!("evidence {path}: {e}"))?;
        let schema = allowed
            .map_or_else(darkbot_bsc_record::schema::all, <[_]>::to_vec)
            .into_iter()
            .find(|s| decode(s, &bytes).is_ok())
            .ok_or_else(|| format!("evidence {path} is no record this role takes"))?;
        let mut d = Draft::new(&lq::evidence::SCHEMA);
        d.set(lq::evidence::PATH, path)
            .set(
                lq::evidence::SHA256,
                darkbot_bsc_record::Value::B256(darkbot_bsc_record::sha256(&bytes)),
            )
            .set(lq::evidence::SCHEMA_NAME, schema.name);
        Ok(d.into_nested())
    };
    let runtime = spec
        .runtime_sha256
        .ok_or("a LIVE qualification names the runtime build it qualifies")?;
    let mut d = Draft::new(&lq::SCHEMA);
    d.set(lq::PROVENANCE_TOOL, "bsc_runtime")
        .set(lq::PROVENANCE_OBSERVED_UNIX_MS, now_ms)
        .set(lq::RUNTIME_SHA256, darkbot_bsc_record::Value::B256(runtime));
    let items = spec
        .venues
        .iter()
        .map(|(v, p)| ("VENUE", v.name(), p, Some(VENUE_EVIDENCE)))
        .chain(
            spec.sources
                .iter()
                .map(|(s, p)| ("SOURCE", s.name(), p, Some(SOURCE_EVIDENCE))),
        );
    for (kind, id, paths, allowed) in items {
        let mut row = Draft::new(&lq::item::SCHEMA);
        row.set(lq::item::KIND, kind).set(lq::item::ID, id);
        for path in paths {
            row.push(lq::item::EVIDENCE, evidence(path, allowed)?);
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
            row.push(
                lq::adapter::EVIDENCE,
                evidence(path, Some(ATTEMPT_EVIDENCE))?,
            );
        }
        d.push(lq::ADAPTERS, row.into_nested());
    }
    if let Some((result, attempt)) = &spec.acquisition {
        d.set(
            lq::ACQUISITION,
            evidence(result, Some(ACQUISITION_EVIDENCE))?,
        )
        .set(
            lq::ACQUISITION_ATTEMPT,
            evidence(attempt, Some(ATTEMPT_EVIDENCE))?,
        );
    }
    if let Some(risk) = &spec.risk {
        let mut row = Draft::new(&lq::risk::SCHEMA);
        row.set(lq::risk::DECIDED_BY, risk.decided_by.as_str())
            .set(lq::risk::DECIDED_UNIX_MS, risk.decided_unix_ms)
            .set(lq::risk::STATEMENT, risk.statement.as_str())
            .set(
                lq::risk::SCOPE_SHA256,
                darkbot_bsc_record::Value::B256(risk.scope_sha256),
            );
        for path in &risk.evidence {
            row.push(lq::risk::EVIDENCE, evidence(path, Some(RISK_EVIDENCE))?);
        }
        d.set(lq::RISK, row.into_nested());
    }
    let bytes = d.encode().map_err(|e| e.to_string())?;
    verify(root, &bytes)?;
    Ok(bytes)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use darkbot_bsc_record::{Kind, Presence, Schema, Value};
    use darkbot_bsc_trading::submission_policy::capability::Knowledge;
    use darkbot_bsc_trading::submission_policy::qualification::{FeatureName, PropertyName};
    use std::path::PathBuf;

    /// Every required field of `s` with a fixed value.
    pub(crate) fn filled(s: &'static Schema) -> Draft {
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

    /// The secret of the tests' synthetic buyer key (`live_end` key_file).
    pub(crate) const BUYER_SECRET: [u8; 32] = [0x33; 32];

    pub(crate) fn buyer() -> [u8; 20] {
        darkbot_bsc_trading::signer::Signer::from_secret_bytes(&BUYER_SECRET)
            .unwrap()
            .address()
            .into_array()
    }

    /// The G0.1 freeze of the pinned deployment.
    pub(crate) fn freeze() -> Draft {
        let pin = bsc_mainnet_v5_27_0();
        let mut upgrade = filled(&ff::upgrade_check::SCHEMA);
        upgrade
            .set(ff::upgrade_check::STATUS, "ONE_IMPLEMENTATION")
            .set(
                ff::upgrade_check::LAST_IMPLEMENTATION,
                Value::Address(pin.portal_implementation.into_array()),
            );
        let mut f = filled(&ff::V2);
        f.set(ff::CHAIN_ID, pin.chain_id)
            .set(ff::PORTAL, Value::Address(pin.portal.into_array()))
            .set(ff::PORTAL_VERSION, pin.portal_version)
            .set(ff::PIN_RECHECKED, true)
            .set(ff::UPGRADE_CHECK, upgrade.into_nested());
        f
    }

    /// A G0.7 attempt of the pinned deployment by `buyer`, sent through
    /// `sent`.
    pub(crate) fn attempt(buyer: [u8; 20], sent: &[LiveAdapter]) -> Draft {
        use g06a::preflight as pf;
        use g07::evidence as a;
        let pin = bsc_mainnet_v5_27_0();
        let mut preflight = filled(&pf::SCHEMA);
        preflight
            .set(pf::CHAIN_ID, pin.chain_id)
            .set(pf::PORTAL, Value::Address(pin.portal.into_array()))
            .set(pf::PORTAL_VERSION, pin.portal_version)
            .set(
                pf::PORTAL_IMPLEMENTATION,
                Value::Address(pin.portal_implementation.into_array()),
            )
            .set(
                pf::PORTAL_IMPLEMENTATION_CODE_SHA256,
                Value::B256(pin.portal_implementation_runtime_sha256),
            );
        let mut send = filled(&a::send::SCHEMA);
        for adapter in sent {
            let mut r = filled(&g07::provider_result::SCHEMA);
            r.set(g07::provider_result::BYTES_MAY_HAVE_LEFT, true)
                .set(g07::provider_result::HTTP_STATUS_CODE, 200u64)
                .set(g07::provider_result::RPC_RESULT_HASH, Value::B256([7; 32]));
            let tag = match adapter {
                LiveAdapter::Club48BuilderV1 => a::send::FORTYEIGHTCLUB,
                LiveAdapter::BlockRazorBuilderV1 => a::send::BLOCKRAZOR,
            };
            send.set(tag, r.into_nested());
        }
        let mut g = filled(&a::SCHEMA);
        g.set(a::GATE, "G0.7")
            .set(a::ATTEMPT_ID, "attempt-1")
            .set(a::CHAIN_ID, pin.chain_id)
            .set(a::PORTAL, Value::Address(pin.portal.into_array()))
            .set(a::BUYER, Value::Address(buyer))
            .set(a::BUY_TX_HASH, Value::B256([5; 32]))
            .set(a::TOKEN, Value::Address([6; 20]))
            .set(a::PREFLIGHT, preflight.into_nested())
            .set(a::SEND, send.into_nested());
        g
    }

    /// The G0.8 result of `verdict` rating `attempt` (its bytes).
    pub(crate) fn result(attempt: &[u8], verdict: &str) -> Draft {
        use g08::result as r;
        let a = decode(&g07::evidence::SCHEMA, attempt).unwrap();
        let mut g = filled(&r::SCHEMA);
        g.set(r::GATE, "G0.8")
            .set(r::VERDICT, verdict)
            .set(
                r::G07_EVIDENCE_SHA256,
                Value::B256(darkbot_bsc_record::sha256(attempt)),
            )
            .set(r::ATTEMPT_ID, "attempt-1")
            .set(r::BUY_TX, Value::B256([5; 32]))
            .set(r::TOKEN, Value::Address([6; 20]))
            .set(
                r::BUYER,
                Value::Address(a.address(g07::evidence::BUYER).unwrap()),
            );
        g
    }

    /// A G0.4 verdict (v3) `CONTINUE` for Flap on confirmed, committed
    /// criteria, in whose census PublicNode delivered `first` actionable
    /// launches first.
    pub(crate) fn g04(first: u64) -> Draft {
        use darkbot_bsc_record::schema::bsc_census::census::source as cs;
        let mut flap = filled(&gv::venue::SCHEMA);
        flap.set(gv::venue::VENUE, gv::RATED_VENUES[0])
            .set(gv::venue::VERDICT, gv::CONTINUE);
        let mut source = filled(&cs::SCHEMA);
        source
            .set(cs::SOURCE_ID, SourceId::PublicNode.name())
            .set(cs::FIRST_FULL_BEFORE, first);
        let mut v = filled(&gv::SCHEMA);
        v.set(gv::CRITERIA_STATE, gv::CONFIRMED)
            .set(gv::VERDICT, gv::CONTINUE)
            .set(gv::PRECOMMIT_SHA256, Value::B256([3; 32]))
            .push(gv::VENUES, flap.into_nested())
            .push(gv::SOURCES, source.into_nested());
        v
    }

    /// The grant risk scope the tests' risk decision is made for
    /// ([`crate::sources::tests::grant`]'s).
    pub(crate) fn risk_scope() -> [u8; 32] {
        crate::sources::tests::grant(0).risk_scope_sha256()
    }

    /// A root with typed evidence: the freeze of the pinned deployment, the
    /// G0.4 verdict of [`g04`], an economics snapshot of BSC, a G0.7 attempt
    /// of the tests' buyer accepted by Club48 and its G0.8 result of
    /// `verdict`, and one record of no role (`measure`).
    pub(crate) fn fixture(tag: &str, verdict: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bsc-live-q-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("ev")).unwrap();
        fs::write(root.join("ev/freeze.dbrec"), freeze().encode().unwrap()).unwrap();
        fs::write(root.join("ev/g04.dbrec"), g04(1).encode().unwrap()).unwrap();
        let mut economics = filled(&economics_snapshot::SCHEMA);
        economics.set(economics_snapshot::CHAIN_ID, 56u64);
        fs::write(root.join("ev/economics.dbrec"), economics.encode().unwrap()).unwrap();
        let g07 = attempt(buyer(), &[LiveAdapter::Club48BuilderV1])
            .encode()
            .unwrap();
        fs::write(root.join("ev/g07.dbrec"), &g07).unwrap();
        let g08 = result(&g07, verdict).encode().unwrap();
        fs::write(root.join("ev/g08.dbrec"), g08).unwrap();
        let other = filled(&darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot::SCHEMA);
        fs::write(root.join("ev/measure.dbrec"), other.encode().unwrap()).unwrap();
        root
    }

    /// The qualification spec of the fixture's evidence.
    pub(crate) fn acquisition() -> Option<(String, String)> {
        Some(("ev/g08.dbrec".into(), "ev/g07.dbrec".into()))
    }

    pub(crate) fn venue_evidence() -> Vec<String> {
        vec!["ev/freeze.dbrec".into()]
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
        Spec {
            runtime_sha256: Some(running_build().unwrap()),
            venues: vec![(VenueId::Flap, venue_evidence())],
            sources: vec![(SourceId::PublicNode, vec!["ev/g04.dbrec".into()])],
            adapters: vec![(
                LiveAdapter::Club48BuilderV1,
                findings(),
                vec!["ev/g07.dbrec".into()],
            )],
            acquisition: acquisition(),
            risk: Some(RiskSpec {
                decided_by: "operator".into(),
                decided_unix_ms: 1,
                statement: "accepts the measured scope".into(),
                scope_sha256: risk_scope(),
                evidence: vec!["ev/g08.dbrec".into()],
            }),
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
        assert_eq!(v.view, crate::sources::tests::qualified());
        assert_eq!(v.acquisition_buyer, Some(buyer()));
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
        fs::write(root.join("ev/g04.dbrec"), b"changed").unwrap();
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
        four.venues = vec![(VenueId::FourClassic, venue_evidence())];
        assert!(
            build(&root2, &four, 1)
                .unwrap_err()
                .contains("no execution path")
        );
        let mut escape = spec();
        escape.sources = vec![(SourceId::Drpc, vec!["../outside.dbrec".into()])];
        assert!(build(&root2, &escape, 1).is_err());
        let mut anonymous = spec();
        anonymous.risk = Some(RiskSpec {
            decided_by: " ".into(),
            statement: "x".into(),
            ..spec().risk.unwrap()
        });
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
        acquisition.acquisition = Some(("ev/measure.dbrec".into(), "ev/g07.dbrec".into()));
        assert!(
            build(&root2, &acquisition, 1)
                .unwrap_err()
                .contains("no record this role takes")
        );
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(root2).unwrap();
    }

    /// BSC3-049: a source takes only a G0.4 verdict that rates it, an
    /// adapter only a G0.7 attempt it accepted, the risk decision only the
    /// economics snapshot or the G0.8 result, decided for one grant risk
    /// scope; a v3 record's risk decision names no scope (decides for none).
    #[test]
    fn sources_adapters_and_risk_take_only_their_bound_records() {
        let root = fixture("roles-sar", "PASS");
        // A correctly hashed record of no role, as each.
        let measure = || vec!["ev/measure.dbrec".to_string()];
        let mut source = spec();
        source.sources = vec![(SourceId::PublicNode, measure())];
        let mut adapter = spec();
        adapter.adapters = vec![(LiveAdapter::Club48BuilderV1, findings(), measure())];
        let mut risk = spec();
        if let Some(r) = risk.risk.as_mut() {
            r.evidence = measure();
        }
        for s in [source, adapter, risk] {
            let e = build(&root, &s, 1).unwrap_err();
            assert!(e.contains("no record this role takes"), "{e}");
        }
        // A verdict that does not rate the source: none delivered first,
        // proposed criteria, another verdict, no pre-commitment, Flap not
        // CONTINUE, only another source delivered.
        let with = |f: &dyn Fn(&mut Draft)| {
            let mut v = g04(1);
            f(&mut v);
            v
        };
        let mut adapt = filled(&gv::venue::SCHEMA);
        adapt
            .set(gv::venue::VENUE, gv::RATED_VENUES[0])
            .set(gv::venue::VERDICT, "ADAPT");
        let mut drpc = filled(&darkbot_bsc_record::schema::bsc_census::census::source::SCHEMA);
        drpc.set(
            darkbot_bsc_record::schema::bsc_census::census::source::SOURCE_ID,
            SourceId::Drpc.name(),
        );
        for v in [
            g04(0),
            with(&|v| {
                v.set(gv::CRITERIA_STATE, "PROPOSED");
            }),
            with(&|v| {
                v.set(gv::VERDICT, "ADAPT");
            }),
            with(&|v| {
                v.clear(gv::PRECOMMIT_SHA256);
            }),
            with(&|v| {
                v.set(gv::VENUES, adapt.clone().into_nested());
            }),
            with(&|v| {
                v.set(gv::SOURCES, drpc.clone().into_nested());
            }),
        ] {
            fs::write(root.join("ev/g04.dbrec"), v.encode().unwrap()).unwrap();
            let e = build(&root, &spec(), 1).unwrap_err();
            assert!(e.contains("no G0.4 verdict CONTINUE"), "{e}");
        }
        // A v2 verdict names no source: it qualifies none.
        let mut v2 = filled(&gv::V2);
        v2.set(gv::CRITERIA_STATE, gv::CONFIRMED)
            .set(gv::VERDICT, gv::CONTINUE);
        fs::write(root.join("ev/g04.dbrec"), v2.encode().unwrap()).unwrap();
        let e = build(&root, &spec(), 1).unwrap_err();
        assert!(e.contains("no record this role takes"), "{e}");
        // Review L: one row each, as the verdict writes them.
        let mut twice = g04(1);
        let mut drpc_row = g04(1);
        drpc_row.set(gv::CRITERIA_STATE, gv::CONFIRMED);
        let mut flap_adapt = filled(&gv::venue::SCHEMA);
        flap_adapt
            .set(gv::venue::VENUE, gv::RATED_VENUES[0])
            .set(gv::venue::VERDICT, "ADAPT");
        twice.push(gv::VENUES, flap_adapt.into_nested());
        let mut again = g04(1);
        let mut source_again =
            filled(&darkbot_bsc_record::schema::bsc_census::census::source::SCHEMA);
        source_again.set(
            darkbot_bsc_record::schema::bsc_census::census::source::SOURCE_ID,
            SourceId::PublicNode.name(),
        );
        again.push(gv::SOURCES, source_again.into_nested());
        for v in [twice, again] {
            fs::write(root.join("ev/g04.dbrec"), v.encode().unwrap()).unwrap();
            let e = build(&root, &spec(), 1).unwrap_err();
            assert!(e.contains("no G0.4 verdict CONTINUE"), "{e}");
        }
        fs::write(root.join("ev/g04.dbrec"), g04(1).encode().unwrap()).unwrap();
        // Review L: an attempt of another Portal version, though Club48
        // accepted it, as Club48's evidence.
        let mut old = filled(&g06a::preflight::SCHEMA);
        old.set(g06a::preflight::PORTAL_VERSION, "5.24.0");
        let mut drifted = attempt(buyer(), &[LiveAdapter::Club48BuilderV1]);
        drifted.set(g07::evidence::PREFLIGHT, old.into_nested());
        fs::write(root.join("ev/drifted.dbrec"), drifted.encode().unwrap()).unwrap();
        let mut stale = spec();
        stale.adapters = vec![(
            LiveAdapter::Club48BuilderV1,
            findings(),
            vec!["ev/drifted.dbrec".into()],
        )];
        let e = build(&root, &stale, 1).unwrap_err();
        assert!(e.contains("not of the pinned deployment"), "{e}");
        // An attempt Club48 did not accept, as Club48's evidence.
        let other = attempt(buyer(), &[LiveAdapter::BlockRazorBuilderV1]);
        fs::write(root.join("ev/other.dbrec"), other.encode().unwrap()).unwrap();
        let mut refused = spec();
        refused.adapters = vec![(
            LiveAdapter::Club48BuilderV1,
            findings(),
            vec!["ev/other.dbrec".into()],
        )];
        let e = build(&root, &refused, 1).unwrap_err();
        assert!(e.contains("did not accept"), "{e}");
        // The risk decision on the economics snapshot, for its scope.
        let snapshot = filled(&economics_snapshot::SCHEMA);
        fs::write(root.join("ev/snapshot.dbrec"), snapshot.encode().unwrap()).unwrap();
        let mut on_snapshot = spec();
        if let Some(r) = on_snapshot.risk.as_mut() {
            r.evidence = vec!["ev/snapshot.dbrec".into()];
        }
        let mut bsc = filled(&economics_snapshot::SCHEMA);
        bsc.set(economics_snapshot::CHAIN_ID, 56u64);
        fs::write(root.join("ev/snapshot.dbrec"), bsc.encode().unwrap()).unwrap();
        let v = verify(&root, &build(&root, &on_snapshot, 1).unwrap()).unwrap();
        assert!(v.view.risk_decision);
        assert_eq!(v.view.risk_scope_sha256, Some(risk_scope()));
        // Review M: another chain's economics, a G0.8 result that is not
        // the acquisition's (an ADAPT of the same attempt), or no
        // acquisition at all.
        fs::write(root.join("ev/snapshot.dbrec"), snapshot.encode().unwrap()).unwrap();
        let g07 = fs::read(root.join("ev/g07.dbrec")).unwrap();
        fs::write(
            root.join("ev/adapt.dbrec"),
            result(&g07, "ADAPT").encode().unwrap(),
        )
        .unwrap();
        let mut on_adapt = spec();
        if let Some(r) = on_adapt.risk.as_mut() {
            r.evidence = vec!["ev/adapt.dbrec".into()];
        }
        let mut unacquired = spec();
        unacquired.acquisition = None;
        for s in [on_snapshot, on_adapt, unacquired] {
            let e = build(&root, &s, 1).unwrap_err();
            assert!(e.contains("other than the acquisition's"), "{e}");
        }
        // A v3 record's risk decision names no scope: it decides for none.
        let economics = fs::read(root.join("ev/economics.dbrec")).unwrap();
        let mut ev = Draft::new(&lq::evidence::SCHEMA);
        ev.set(lq::evidence::PATH, "ev/economics.dbrec")
            .set(
                lq::evidence::SHA256,
                Value::B256(darkbot_bsc_record::sha256(&economics)),
            )
            .set(lq::evidence::SCHEMA_NAME, economics_snapshot::SCHEMA.name);
        let mut row = Draft::new(&lq::risk::V1);
        row.set(lq::risk::DECIDED_BY, "operator")
            .set(lq::risk::DECIDED_UNIX_MS, 1u64)
            .set(lq::risk::STATEMENT, "accepts")
            .push(lq::risk::EVIDENCE, ev.into_nested());
        let mut d = Draft::new(&lq::V3);
        d.set(lq::PROVENANCE_TOOL, "test")
            .set(lq::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(lq::RUNTIME_SHA256, Value::B256([0x12; 32]))
            .set(lq::RISK, row.into_nested());
        let v3 = verify(&root, &d.encode().unwrap()).unwrap();
        assert!(v3.view.risk_decision);
        assert_eq!(v3.view.risk_scope_sha256, None);
        fs::remove_dir_all(root).unwrap();
    }

    /// A qualification (v4) of `role` alone (`SOURCE`, `ADAPTER` or
    /// `RISK`) naming `path` as its evidence, with the schema name
    /// `declared`, hand-encoded (not through `build`).
    fn hand_role(root: &Path, role: &str, path: &str, declared: &str) -> Vec<u8> {
        let bytes = fs::read(root.join(path)).unwrap();
        let mut ev = Draft::new(&lq::evidence::SCHEMA);
        ev.set(lq::evidence::PATH, path)
            .set(
                lq::evidence::SHA256,
                Value::B256(darkbot_bsc_record::sha256(&bytes)),
            )
            .set(lq::evidence::SCHEMA_NAME, declared);
        let mut d = Draft::new(&lq::SCHEMA);
        d.set(lq::PROVENANCE_TOOL, "test")
            .set(lq::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(lq::RUNTIME_SHA256, Value::B256([0x12; 32]));
        match role {
            "SOURCE" => {
                let mut item = Draft::new(&lq::item::SCHEMA);
                item.set(lq::item::KIND, "SOURCE")
                    .set(lq::item::ID, SourceId::PublicNode.name())
                    .push(lq::item::EVIDENCE, ev.into_nested());
                d.push(lq::ITEMS, item.into_nested());
            }
            "ADAPTER" => {
                let mut row = Draft::new(&lq::adapter::SCHEMA);
                row.set(lq::adapter::ADAPTER, LiveAdapter::Club48BuilderV1.name());
                for f in findings() {
                    row.push(lq::adapter::FINDINGS, f.name());
                }
                row.push(lq::adapter::EVIDENCE, ev.into_nested());
                d.push(lq::ADAPTERS, row.into_nested());
            }
            _ => {
                let mut row = Draft::new(&lq::risk::SCHEMA);
                row.set(lq::risk::DECIDED_BY, "operator")
                    .set(lq::risk::DECIDED_UNIX_MS, 1u64)
                    .set(lq::risk::STATEMENT, "accepts")
                    .set(lq::risk::SCOPE_SHA256, Value::B256(risk_scope()))
                    .push(lq::risk::EVIDENCE, ev.into_nested());
                d.set(lq::RISK, row.into_nested());
            }
        }
        d.encode().unwrap()
    }

    /// BSC3-049: a correctly hashed record of no role, named as what it
    /// is, never verifies as a source's, an adapter's or the risk
    /// decision's evidence; their own records do.
    #[test]
    fn a_record_of_no_role_verifies_for_none() {
        let root = fixture("roles-hand", "PASS");
        let slot = darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot::SCHEMA.name;
        for (role, own, name, refused) in [
            (
                "SOURCE",
                "ev/g04.dbrec",
                gv::SCHEMA.name,
                "not darkbot.bsc.g04-verdict.v1",
            ),
            (
                "ADAPTER",
                "ev/g07.dbrec",
                g07::evidence::SCHEMA.name,
                "not darkbot.g07",
            ),
            (
                "RISK",
                "ev/economics.dbrec",
                economics_snapshot::SCHEMA.name,
                "not darkbot.bsc.economics-snapshot.v1 or",
            ),
        ] {
            let e = verify(&root, &hand_role(&root, role, "ev/measure.dbrec", slot)).unwrap_err();
            assert!(e.contains(refused), "{role}: {e}");
            verify(&root, &hand_role(&root, role, own, name)).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }

    /// A qualification naming `path` as the Flap venue's evidence, with the
    /// schema name `declared`, hand-encoded (not through `build`).
    fn hand_venue(root: &Path, path: &str, declared: &str) -> Vec<u8> {
        let bytes = fs::read(root.join(path)).unwrap();
        let mut ev = Draft::new(&lq::evidence::SCHEMA);
        ev.set(lq::evidence::PATH, path)
            .set(
                lq::evidence::SHA256,
                Value::B256(darkbot_bsc_record::sha256(&bytes)),
            )
            .set(lq::evidence::SCHEMA_NAME, declared);
        let mut item = Draft::new(&lq::item::SCHEMA);
        item.set(lq::item::KIND, "VENUE")
            .set(lq::item::ID, VenueId::Flap.name())
            .push(lq::item::EVIDENCE, ev.into_nested());
        let mut d = Draft::new(&lq::SCHEMA);
        d.set(lq::PROVENANCE_TOOL, "test")
            .set(lq::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(lq::RUNTIME_SHA256, Value::B256([0x12; 32]))
            .push(lq::ITEMS, item.into_nested());
        d.encode().unwrap()
    }

    /// BSC3-012: a qualification names the runtime build it qualifies (v3):
    /// none is built without it; a v2 record verifies naming none; the
    /// refusal holds for another build, none named, and an unreadable own
    /// build, and never for the named one.
    #[test]
    fn a_qualification_names_the_runtime_build_it_qualifies() {
        let root = fixture("lq-build", "PASS");
        let bytes = build(&root, &spec(), 1).unwrap();
        let v = verify(&root, &bytes).unwrap();
        let own = running_build().unwrap();
        assert_eq!(v.runtime_sha256, Some(own));
        assert_eq!(build_refusal(&Ok(own), &v), None);
        let other = build_refusal(&Ok([0x55; 32]), &v).unwrap();
        assert!(
            other.starts_with(crate::sources::LIVE_BUILD_UNQUALIFIED),
            "{other}"
        );
        let unreadable = build_refusal(&Err("gone".into()), &v).unwrap();
        assert!(unreadable.contains("cannot be read"), "{unreadable}");
        assert!(
            build(
                &root,
                &Spec {
                    runtime_sha256: None,
                    ..spec()
                },
                1
            )
            .unwrap_err()
            .contains("runtime build")
        );
        // A v2 record (before the build was named) qualifies no build.
        let mut d = Draft::new(&lq::V2);
        d.set(lq::PROVENANCE_TOOL, "test")
            .set(lq::PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        let v2 = verify(&root, &d.encode().unwrap()).unwrap();
        assert_eq!(v2.runtime_sha256, None);
        let none = build_refusal(&Ok(own), &v2).unwrap();
        assert!(none.contains("names no runtime build"), "{none}");
        fs::remove_dir_all(root).unwrap();
    }

    /// BSC3-049: each role takes only its own records, bound to what it
    /// qualifies: a venue the freeze of the pinned deployment, the
    /// acquisition the G0.8 PASS of the G0.7 attempt it names, of that
    /// deployment, its buyer and builders carried to the gate.
    #[test]
    fn each_role_takes_only_its_own_bound_records() {
        let root = fixture("roles", "PASS");
        // The adapter's evidence apart from the attempt the acquisition
        // cases below change.
        fs::copy(root.join("ev/g07.dbrec"), root.join("ev/club48.dbrec")).unwrap();
        let spec = || {
            let mut s = spec();
            s.adapters[0].2 = vec!["ev/club48.dbrec".into()];
            s
        };
        // A correctly hashed record of no role as the venue's freeze, built
        // or hand-encoded under its own or a borrowed schema name.
        let mut venue = spec();
        venue.venues = vec![(VenueId::Flap, vec!["ev/measure.dbrec".into()])];
        assert!(
            build(&root, &venue, 1)
                .unwrap_err()
                .contains("no record this role takes")
        );
        let slot = darkbot_bsc_record::schema::bsc_runtime::buy_attempt_slot::SCHEMA.name;
        for (declared, why) in [
            (slot, "not darkbot.g01.flap-freeze.v2"),
            (ff::V2.name, "is not a"),
        ] {
            let e = verify(&root, &hand_venue(&root, "ev/measure.dbrec", declared)).unwrap_err();
            assert!(e.contains(why), "{e}");
        }
        assert!(verify(&root, &hand_venue(&root, "ev/freeze.dbrec", ff::V2.name)).is_ok());
        // A freeze of another version, a Portal that was upgraded, a pin
        // not read again.
        let freeze = || freeze();
        let mut upgraded = filled(&ff::upgrade_check::SCHEMA);
        upgraded.set(ff::upgrade_check::STATUS, "UPGRADED");
        for f in [
            {
                let mut f = freeze();
                f.set(ff::PORTAL_VERSION, "5.27.2");
                f
            },
            {
                let mut f = freeze();
                f.set(ff::UPGRADE_CHECK, upgraded.into_nested());
                f
            },
            {
                let mut f = freeze();
                f.set(ff::PIN_RECHECKED, false);
                f
            },
        ] {
            fs::write(root.join("ev/freeze.dbrec"), f.encode().unwrap()).unwrap();
            let e = build(&root, &spec(), 1).unwrap_err();
            assert!(e.contains("not of the pinned deployment"), "{e}");
        }
        fs::write(root.join("ev/freeze.dbrec"), freeze().encode().unwrap()).unwrap();
        // A G0.8 PASS of another attempt (its bytes changed), of an attempt
        // of another Portal version, or of a v1 record without the attempt.
        let g07 = attempt(buyer(), &[LiveAdapter::Club48BuilderV1]);
        let mut other = attempt([9; 20], &[LiveAdapter::Club48BuilderV1]);
        other.set(g07::evidence::ATTEMPT_ID, "attempt-2");
        fs::write(root.join("ev/g07.dbrec"), other.encode().unwrap()).unwrap();
        let e = build(&root, &spec(), 1).unwrap_err();
        assert!(e.contains("does not rate this G0.7 attempt"), "{e}");
        // The same attempt, buy, token and buyer, but other bytes (another
        // builder claimed): only the G0.8 result's G0.7 hash tells.
        let swapped = attempt(buyer(), &[LiveAdapter::BlockRazorBuilderV1]);
        fs::write(root.join("ev/g07.dbrec"), swapped.encode().unwrap()).unwrap();
        let e = build(&root, &spec(), 1).unwrap_err();
        assert!(e.contains("does not rate this G0.7 attempt"), "{e}");
        let mut old = filled(&g06a::preflight::SCHEMA);
        old.set(g06a::preflight::PORTAL_VERSION, "5.24.0");
        let mut drifted = g07.clone();
        drifted.set(g07::evidence::PREFLIGHT, old.into_nested());
        let bytes = drifted.encode().unwrap();
        fs::write(root.join("ev/g07.dbrec"), &bytes).unwrap();
        fs::write(
            root.join("ev/g08.dbrec"),
            result(&bytes, "PASS").encode().unwrap(),
        )
        .unwrap();
        let e = build(&root, &spec(), 1).unwrap_err();
        assert!(e.contains("not of the pinned deployment"), "{e}");
        // Another buyer, sent through BlockRazor only: verified, carried to
        // the LIVE end (buyer) and the gate (builders).
        let theirs = attempt([9; 20], &[LiveAdapter::BlockRazorBuilderV1])
            .encode()
            .unwrap();
        fs::write(root.join("ev/g07.dbrec"), &theirs).unwrap();
        fs::write(
            root.join("ev/g08.dbrec"),
            result(&theirs, "PASS").encode().unwrap(),
        )
        .unwrap();
        let v = verify(&root, &build(&root, &spec(), 1).unwrap()).unwrap();
        assert_eq!(v.acquisition_buyer, Some([9; 20]));
        assert_eq!(
            v.view.acquisition_adapters,
            [LiveAdapter::BlockRazorBuilderV1]
        );
        // Review M: a builder that refused, answered nothing a client can
        // read or no result hash did not accept the attempt.
        use g07::provider_result as pr;
        for refused in [
            (pr::HTTP_STATUS_CODE, Value::U64(401)),
            (
                pr::RESPONSE_UNDECODABLE,
                Value::Text("401 Unauthorized".into()),
            ),
            (pr::RPC_ERROR_MESSAGE, Value::Text("bundle refused".into())),
            (pr::ERROR, Value::Text("connection reset".into())),
        ] {
            let mut g = attempt([9; 20], &[LiveAdapter::BlockRazorBuilderV1]);
            let mut r = filled(&pr::SCHEMA);
            r.set(pr::BYTES_MAY_HAVE_LEFT, true)
                .set(pr::HTTP_STATUS_CODE, 200u64)
                .set(pr::RPC_RESULT_HASH, Value::B256([7; 32]))
                .set(refused.0, refused.1);
            let mut send = filled(&g07::evidence::send::SCHEMA);
            send.set(g07::evidence::send::BLOCKRAZOR, r.into_nested());
            g.set(g07::evidence::SEND, send.into_nested());
            let bytes = g.encode().unwrap();
            fs::write(root.join("ev/g07.dbrec"), &bytes).unwrap();
            fs::write(
                root.join("ev/g08.dbrec"),
                result(&bytes, "PASS").encode().unwrap(),
            )
            .unwrap();
            let v = verify(&root, &build(&root, &spec(), 1).unwrap()).unwrap();
            assert!(v.view.acquisition_adapters.is_empty());
        }
        // Review L: the right G0.7 hash, but the G0.8 result names another
        // buyer (or attempt, buy, token): no rating of this attempt.
        let mine = attempt(buyer(), &[LiveAdapter::Club48BuilderV1])
            .encode()
            .unwrap();
        fs::write(root.join("ev/g07.dbrec"), &mine).unwrap();
        for (tag, value) in [
            (g08::result::BUYER, Value::Address([9; 20])),
            (g08::result::ATTEMPT_ID, Value::Text("attempt-2".into())),
            (g08::result::BUY_TX, Value::B256([8; 32])),
            (g08::result::TOKEN, Value::Address([8; 20])),
        ] {
            let mut g = result(&mine, "PASS");
            g.set(tag, value);
            fs::write(root.join("ev/g08.dbrec"), g.encode().unwrap()).unwrap();
            let e = build(&root, &spec(), 1).unwrap_err();
            assert!(e.contains("does not rate this G0.7 attempt"), "{e}");
        }
        let mut g08_ref = Draft::new(&lq::evidence::SCHEMA);
        g08_ref
            .set(lq::evidence::PATH, "ev/g08.dbrec")
            .set(
                lq::evidence::SHA256,
                Value::B256(darkbot_bsc_record::sha256(
                    &fs::read(root.join("ev/g08.dbrec")).unwrap(),
                )),
            )
            .set(lq::evidence::SCHEMA_NAME, g08::result::SCHEMA.name);
        let mut d = Draft::new(&lq::V1);
        d.set(lq::PROVENANCE_TOOL, "test")
            .set(lq::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(lq::ACQUISITION, g08_ref.into_nested());
        let e = verify(&root, &d.encode().unwrap()).unwrap_err();
        assert!(e.contains("names no G0.7 attempt"), "{e}");
        fs::remove_dir_all(root).unwrap();
    }
}
