//! BSC-V8 (`POST /api/v1/bsc/query`): one bounded read-only query, a
//! `BscQueryRequest` in and a `BscQueryResponse` out, answered from the native
//! owners' files (never a write). A query no native owner answers yet is
//! `NOT_IMPLEMENTED` and names the missing BSC-V8 owner.
use super::wire::bsc as w;
use darkbot_bsc_execution::strategy_settings as settings;
use prost::Message;
use std::path::Path;

/// The answer to `request` (its bytes), for the runtime under `rundir` and
/// the configuration under `root`.
pub fn answer(request: &[u8], root: &Path, rundir: &Path) -> Vec<u8> {
    let response = match w::BscQueryRequest::decode(request) {
        Ok(req) => respond(req, root, rundir),
        Err(e) => w::BscQueryResponse {
            availability: w::Availability::Unavailable as i32,
            unavailable_reason: format!("the query does not decode: {e}"),
            ..Default::default()
        },
    };
    response.encode_to_vec()
}

fn respond(req: w::BscQueryRequest, root: &Path, rundir: &Path) -> w::BscQueryResponse {
    use w::bsc_query_request::Query;
    let missing = |owner: &str| w::BscQueryResponse {
        request_id: req.request_id.clone(),
        availability: w::Availability::NotImplemented as i32,
        unavailable_reason: format!("no native owner yet: {owner}"),
        ..Default::default()
    };
    match &req.query {
        Some(Query::StrategySettings(q)) => {
            let state = strategy_settings_state(q.include_features, root, rundir);
            w::BscQueryResponse {
                request_id: req.request_id.clone(),
                availability: state.availability,
                unavailable_reason: state.unavailable_reason.clone(),
                result: Some(w::bsc_query_response::Result::StrategySettings(state)),
                ..Default::default()
            }
        }
        Some(Query::Trades(_)) | Some(Query::Trade(_)) => {
            missing("BSC-V8-07..10 trade-history projection")
        }
        Some(Query::TrainingRuns(_)) | Some(Query::ReplaySources(_)) => {
            missing("BSC-V8-02/03 training runs")
        }
        Some(Query::TrainingProfiles(q)) => {
            let profiles = training_profiles(q.include_archived, rundir);
            w::BscQueryResponse {
                request_id: req.request_id.clone(),
                availability: profiles.availability,
                unavailable_reason: profiles.unavailable_reason.clone(),
                result: Some(w::bsc_query_response::Result::TrainingProfiles(profiles)),
                ..Default::default()
            }
        }
        Some(Query::WalletMovements(_)) | Some(Query::Treasury(_)) => {
            missing("BSC-V8-13..17 wallets and treasury")
        }
        Some(Query::Series(_)) => missing("BSC-V8-11/14 performance series"),
        Some(Query::Watch(_)) => missing("BSC-V8-18 WATCH (REV.2.0)"),
        None => w::BscQueryResponse {
            request_id: req.request_id.clone(),
            availability: w::Availability::Unavailable as i32,
            unavailable_reason: "the request names no query".into(),
            ..Default::default()
        },
    }
}

/// What the runtime status says of the running strategy's settings.
enum Running {
    /// No status, or Strategy C does not run.
    None,
    /// A status older than its own stale bound (a stopped or killed runtime).
    Stale,
    /// A status that does not read.
    Invalid(String),
    /// The strategy refused its settings at its start (`<state>: <reason>`).
    Refused(String),
    /// The settings generation and values the running strategy took.
    Took(u64, Vec<(&'static str, String)>),
}

/// The running strategy's settings from the runtime status, only while it
/// is current (review H1: the runtime never removes it).
fn running(rundir: &Path, now_ms: u64) -> Running {
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
    let Ok(bytes) = std::fs::read(rundir.join("bsc-runtime/status.dbrec")) else {
        return Running::None;
    };
    let r = match darkbot_bsc_record::decode(&rs::SCHEMA, &bytes) {
        Ok(r) => r,
        Err(e) => return Running::Invalid(e.to_string()),
    };
    if !darkbot_bsc_execution::sources::status_is_current(&r, now_ms) {
        return Running::Stale;
    }
    let read = || -> Result<Running, String> {
        let e = |x: darkbot_bsc_record::RecordError| x.0;
        let Some(st) = r.opt_nested(rs::STRATEGY).map_err(e)? else {
            return Ok(Running::None);
        };
        let state = st.text(rs::strategy::STATE).map_err(e)?;
        if matches!(state, "SETTINGS_REFUSED" | "GAS_EVIDENCE_REFUSED") {
            let reason = st
                .opt_text(rs::strategy::REASON)
                .map_err(e)?
                .unwrap_or_default();
            return Ok(Running::Refused(format!("{state}: {reason}")));
        }
        let Some(generation) = st.opt_u64(rs::strategy::SETTINGS_GENERATION).map_err(e)? else {
            return Ok(Running::None);
        };
        let mut values = Vec::new();
        for row in st.nested_all(rs::strategy::SETTINGS).map_err(e)? {
            let id = row.text(rs::strategy::setting::ID).map_err(e)?;
            let k = settings::key(id).ok_or_else(|| format!("{id} is no Strategy C setting"))?;
            values.push((
                k,
                row.text(rs::strategy::setting::VALUE)
                    .map_err(e)?
                    .to_string(),
            ));
        }
        Ok(Running::Took(generation, values))
    };
    read().unwrap_or_else(Running::Invalid)
}

/// The settings basis of the native owner's stored generation, the
/// configuration file under it (effective: what the next start takes) and
/// the running strategy's (from a current runtime status), with the
/// native bounds.
pub fn strategy_settings_state(
    include_features: bool,
    root: &Path,
    rundir: &Path,
) -> w::StrategyCSettingsState {
    strategy_settings_state_at(
        include_features,
        root,
        rundir,
        darkbot_bsc_record::observed_now().unwrap_or(0),
    )
}

fn strategy_settings_state_at(
    include_features: bool,
    root: &Path,
    rundir: &Path,
    observed: u64,
) -> w::StrategyCSettingsState {
    use darkbot_bsc_record::schema::bsc_runtime::strategy_settings as ss;
    let unavailable = |why: String| w::StrategyCSettingsState {
        availability: w::Availability::Unavailable as i32,
        unavailable_reason: why,
        ..Default::default()
    };
    let config_path = root.join(darkbot_bsc_execution::sources::RUNTIME_CONFIG_PATH);
    let text = match std::fs::read_to_string(&config_path) {
        Ok(t) => t,
        Err(e) => return unavailable(format!("{}: {e}", config_path.display())),
    };
    let (stored, file) = match settings::current_file(&rundir.join("bsc-sources")) {
        Ok(s) => s,
        Err(e) => return unavailable(format!("the Strategy C settings owner: {e}")),
    };
    let config = match settings::effective(&text, &stored) {
        Ok(c) => c,
        Err(e) => {
            return unavailable(format!(
                "the stored settings are refused by the runtime configuration: {e}"
            ));
        }
    };
    let effective_values = settings::effective_values(&text, &stored);
    let stored_sha = darkbot_bsc_record::hex(&settings::config_sha256(&effective_values));
    let mut state = w::StrategyCSettingsState {
        availability: w::Availability::Available as i32,
        stored: Some(values_proto(&stored.values, None)),
        stored_revision: stored.generation,
        stored_config_sha256: stored_sha.clone(),
        apply_status: w::SettingsApplyStatus::NotRunning as i32,
        request_id: stored.request_id.clone().unwrap_or_default(),
        features: if include_features {
            feature_specs(config.strategy.engine.ratio_places)
        } else {
            Vec::new()
        },
        effective: Some(values_proto(&effective_values, Some(&config))),
        setting_specs: setting_specs(config.strategy.engine.ratio_places),
        ..Default::default()
    };
    let (mut quality, mut reasons) = (w::DataQuality::Verified, Vec::new());
    match running(rundir, observed) {
        Running::None => {}
        Running::Stale => reasons.push("RUNTIME_STATUS_STALE".to_string()),
        Running::Invalid(e) => {
            quality = w::DataQuality::Incomplete;
            reasons.push(format!("RUNTIME_STATUS_INVALID: {e}"));
            state.apply_status = w::SettingsApplyStatus::Unspecified as i32;
        }
        Running::Refused(why) => {
            state.apply_status = w::SettingsApplyStatus::Refused as i32;
            state.refusal_reason = why;
        }
        Running::Took(generation, values) => {
            // The status holds the start's whole effective basis (review L2).
            let sha = darkbot_bsc_record::hex(&settings::config_sha256(&values));
            state.apply_status = if generation == stored.generation && sha == stored_sha {
                w::SettingsApplyStatus::InEffect
            } else {
                w::SettingsApplyStatus::SavedAppliesNextStart
            } as i32;
            state.running = Some(values_proto(&values, None));
            state.running_revision = Some(generation);
            state.running_config_sha256 = sha;
        }
    }
    state.provenance = Some(w::OwnerProvenance {
        owner: "bsc_runtime strategy settings".into(),
        owner_version: stored.generation,
        observed_unix_ms: observed,
        evidence: file.as_ref().and_then(|p| {
            let bytes = std::fs::read(p).ok()?;
            let written = darkbot_bsc_record::decode(&ss::SCHEMA, &bytes)
                .ok()?
                .u64(ss::PROVENANCE_OBSERVED_UNIX_MS)
                .ok()?;
            Some(w::Evidence {
                path: p.strip_prefix(rundir).unwrap_or(p).display().to_string(),
                sha256: darkbot_bsc_record::hex(&darkbot_bsc_record::sha256(&bytes)),
                observed_unix_s: written / 1_000,
            })
        }),
        quality: quality as i32,
        quality_reasons: reasons,
        provenance: "NATIVE_CREATED".into(),
    });
    state
}

/// BSC-V8-01: the Strategy C profiles of the native owner's current
/// catalog (the deleted ones never, the archived ones on request), each at
/// its revision with its configuration hash. No run or LIVE selection uses
/// one yet (BSC-V8-02, -09): `in_use` false, no runs.
fn training_profiles(include_archived: bool, rundir: &Path) -> w::TrainingProfiles {
    use darkbot_bsc_execution::strategy_profiles::{self as profiles, Status};
    use darkbot_bsc_record::schema::bsc_runtime::strategy_profiles as sp;
    let observed = darkbot_bsc_record::observed_now().unwrap_or(0);
    let (catalog, file) = match profiles::current_file(&rundir.join("bsc-sources")) {
        Ok(c) => c,
        Err(e) => {
            return w::TrainingProfiles {
                availability: w::Availability::Unavailable as i32,
                unavailable_reason: format!("the Strategy C profile owner: {e}"),
                ..Default::default()
            };
        }
    };
    let list = catalog
        .profiles
        .iter()
        .filter(|p| match p.status {
            Status::Active => true,
            Status::Archived => include_archived,
            Status::Deleted => false,
        })
        .map(|p| w::TrainingProfile {
            profile_id: p.profile_id.clone(),
            name: p.name.clone(),
            revision: p.revision,
            config_sha256: darkbot_bsc_record::hex(&p.config_sha256()),
            venues: p.venues.iter().map(|v| v.name().to_string()).collect(),
            created_unix_ms: p.created_unix_ms,
            updated_unix_ms: p.updated_unix_ms,
            status: match p.status {
                Status::Active => w::TrainingProfileStatus::Active,
                Status::Archived => w::TrainingProfileStatus::Archived,
                Status::Deleted => w::TrainingProfileStatus::Deleted,
            } as i32,
            values: Some(settable_proto(&p.values)),
            request_id: p.request_id.clone(),
            ..Default::default()
        })
        .collect();
    w::TrainingProfiles {
        availability: w::Availability::Available as i32,
        provenance: Some(w::OwnerProvenance {
            owner: "bsc_runtime strategy profiles".into(),
            owner_version: catalog.version,
            observed_unix_ms: observed,
            evidence: file.as_ref().and_then(|f| {
                let bytes = std::fs::read(f).ok()?;
                let written = darkbot_bsc_record::decode(&sp::SCHEMA, &bytes)
                    .ok()?
                    .u64(sp::PROVENANCE_OBSERVED_UNIX_MS)
                    .ok()?;
                Some(w::Evidence {
                    path: f.strip_prefix(rundir).unwrap_or(f).display().to_string(),
                    sha256: darkbot_bsc_record::hex(&darkbot_bsc_record::sha256(&bytes)),
                    observed_unix_s: written / 1_000,
                })
            }),
            quality: w::DataQuality::Verified as i32,
            provenance: "NATIVE_CREATED".into(),
            ..Default::default()
        }),
        profiles: list,
        catalog_version: catalog.version,
        ..Default::default()
    }
}

/// A profile's values (every settable key) as a command's typed values.
fn settable_proto(values: &[(&'static str, String)]) -> w::StrategyCSettableValues {
    let s = values_proto(values, None);
    w::StrategyCSettableValues {
        input_wei: s.input_wei,
        target_net_multiple: s.target_net_multiple,
        stop_net_multiple: s.stop_net_multiple,
        observation_ms: s.observation_ms,
        hold_ms: s.hold_ms,
        max_open: s.max_open,
        creator_sold_before_fill: s.creator_sold_before_fill,
        creator_sell_exit: s.creator_sell_exit,
        filters: s.filters,
        first_buyer_window: s.first_buyer_window,
    }
}

/// A settings basis as its typed values (`config`: the effective one, with
/// the configuration's read-only values).
fn values_proto(
    values: &[(&'static str, String)],
    config: Option<&darkbot_bsc_execution::runtime::RuntimeConfig>,
) -> w::StrategyCSettings {
    use darkbot_bsc_trading::strategy::engine::CreatorSoldBeforeFill as CreatorSold;
    let get = |k: &str| {
        values
            .iter()
            .find(|(s, _)| *s == k)
            .map(|(_, v)| v.as_str())
    };
    let num = |k: &str| get(k).and_then(|v| v.parse::<u64>().ok());
    w::StrategyCSettings {
        input_wei: get("BSC_STRATEGY_C_INPUT_WEI").map(String::from),
        target_net_multiple: get("BSC_STRATEGY_C_TARGET_NET_MULTIPLE").map(String::from),
        stop_net_multiple: get("BSC_STRATEGY_C_STOP_NET_MULTIPLE").map(String::from),
        observation_ms: num("BSC_STRATEGY_C_OBSERVATION_MS"),
        hold_ms: num("BSC_STRATEGY_C_HOLD_MS"),
        max_open: num("BSC_STRATEGY_C_MAX_OPEN").and_then(|n| u32::try_from(n).ok()),
        // The effective basis: the typed configuration's (a key the file
        // leaves out parses to its default; review L6).
        creator_sold_before_fill: match (config, get("BSC_STRATEGY_C_CREATOR_SOLD_BEFORE_FILL")) {
            (Some(c), _) => match c.strategy.engine.creator_sold_before_fill {
                CreatorSold::FreeCancel => w::CreatorSoldBeforeFill::FreeCancel as i32,
                CreatorSold::TxLands => w::CreatorSoldBeforeFill::TxLands as i32,
            },
            (None, Some("FREE_CANCEL")) => w::CreatorSoldBeforeFill::FreeCancel as i32,
            (None, Some("TX_LANDS")) => w::CreatorSoldBeforeFill::TxLands as i32,
            (None, _) => w::CreatorSoldBeforeFill::Unspecified as i32,
        },
        creator_sell_exit: match (config, get("BSC_STRATEGY_C_CREATOR_SELL_EXIT")) {
            (Some(c), _) if c.strategy.engine.creator_sell_exit => {
                w::CreatorSellExit::Enabled as i32
            }
            (Some(_), _) => w::CreatorSellExit::Disabled as i32,
            (None, Some("ENABLED")) => w::CreatorSellExit::Enabled as i32,
            (None, Some("DISABLED")) => w::CreatorSellExit::Disabled as i32,
            (None, _) => w::CreatorSellExit::Unspecified as i32,
        },
        filters: get("BSC_STRATEGY_C_FILTERS")
            .map(rules_proto)
            .unwrap_or_default(),
        first_buyer_window: num("BSC_STRATEGY_C_FIRST_BUYER_WINDOW")
            .and_then(|n| u32::try_from(n).ok()),
        ratio_places: config.map(|c| c.strategy.engine.ratio_places),
        fill_delay_blocks: config.map(|c| c.strategy.engine.fill_delay_blocks as u32),
        gas_price_wei: config.map(|c| c.strategy.engine.gas_price_wei.to_string()),
    }
}

/// The native filter text `feature:min:max,...` (the enabled rules, in their
/// order) as typed rules.
fn rules_proto(text: &str) -> Vec<w::StrategyFilterRule> {
    text.split(',')
        .filter(|r| !r.trim().is_empty())
        .filter_map(|r| {
            let mut parts = r.trim().splitn(3, ':');
            let (feature, min, max) = (parts.next()?, parts.next()?, parts.next()?);
            let bound = |b: &str| (!b.is_empty()).then(|| b.to_string());
            Some(w::StrategyFilterRule {
                feature: feature.to_string(),
                enabled: true,
                min: bound(min),
                max: bound(max),
            })
        })
        .collect()
}

/// The settable keys' units (the query's words: wei, ratio, ms, count, enum;
/// `rules` for the filter list) and the extreme values the native parser
/// accepts.
fn setting_specs(ratio_places: u32) -> Vec<w::StrategySettingSpec> {
    use darkbot_bsc_execution::runtime::STRATEGY_SETTING_BOUNDS;
    use darkbot_bsc_trading::strategy::{engine::ExitTerms, fixed};
    let (target_min, stop_max) = ExitTerms::accepted_extremes(ratio_places)
        .map(|(t, s)| (fixed::text(t, ratio_places), fixed::text(s, ratio_places)))
        .unzip();
    settings::KEYS
        .iter()
        .map(|k| {
            let bounded = STRATEGY_SETTING_BOUNDS.iter().find(|(b, ..)| b == k);
            let (unit, min, max, places) = match (*k, bounded) {
                (_, Some((_, unit, lo, hi))) => {
                    (*unit, Some(lo.to_string()), Some(hi.to_string()), 0)
                }
                ("BSC_STRATEGY_C_TARGET_NET_MULTIPLE", _) => {
                    ("ratio", target_min.clone(), None, ratio_places)
                }
                ("BSC_STRATEGY_C_STOP_NET_MULTIPLE", _) => (
                    "ratio",
                    Some(fixed::text(0, ratio_places)),
                    stop_max.clone(),
                    ratio_places,
                ),
                ("BSC_STRATEGY_C_FILTERS", _) => ("rules", None, None, 0),
                (_, None) => ("enum", None, None, 0),
            };
            w::StrategySettingSpec {
                key: (*k).to_string(),
                unit: unit.to_string(),
                min,
                max,
                places,
            }
        })
        .collect()
}

/// The features a filter rule may name, with their places.
fn feature_specs(ratio_places: u32) -> Vec<w::StrategyFeatureSpec> {
    use darkbot_bsc_trading::strategy::features::{GAP_FLAG, NAMES, places, signed, unit};
    NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| w::StrategyFeatureSpec {
            name: (*name).to_string(),
            index: i as u32,
            unit: unit(i).into(),
            places: places(i, ratio_places),
            signed: signed(i),
            filterable: i != GAP_FLAG,
        })
        .collect()
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
