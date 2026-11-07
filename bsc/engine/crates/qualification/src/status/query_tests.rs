use super::*;
use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn config() -> String {
    std::fs::read_to_string(root().join(darkbot_bsc_execution::sources::RUNTIME_CONFIG_PATH))
        .unwrap()
}

fn rundir(tag: &str) -> std::path::PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let dir = std::env::temp_dir().join(format!("bsc-query-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(dir.join("bsc-sources"))
        .unwrap();
    dir
}

fn ask(query: w::bsc_query_request::Query, rundir: &Path) -> w::BscQueryResponse {
    let req = w::BscQueryRequest {
        request_id: "q1".into(),
        query: Some(query),
    };
    w::BscQueryResponse::decode(answer(&req.encode_to_vec(), &root(), rundir).as_slice()).unwrap()
}

/// A runtime status written at `written` (rewritten every second, stale
/// after three), its strategy section from the runtime's own draft: running
/// with `took` (its settings generation and values), or refused.
fn status(
    rundir: &Path,
    written: u64,
    took: Option<(u64, &[(&str, &str)])>,
    refused: Option<&str>,
) {
    use darkbot_bsc_execution::runtime::RuntimeConfig;
    let cfg = RuntimeConfig::parse(&config()).unwrap();
    let why = refused.map(|r| ("SETTINGS_REFUSED", r.to_string()));
    let mut st = darkbot_bsc_execution::runtime_strategy::status_draft(
        None,
        why.as_ref(),
        &cfg.strategy.engine,
        None,
    );
    if let Some((generation, values)) = took {
        st.set(rs::strategy::STATE, "SHADOW")
            .set(rs::strategy::SETTINGS_GENERATION, generation);
        for (k, v) in values {
            let mut e = darkbot_bsc_record::Draft::new(&rs::strategy::setting::SCHEMA);
            e.set(rs::strategy::setting::ID, *k)
                .set(rs::strategy::setting::VALUE, *v);
            st.push(rs::strategy::SETTINGS, e.into_nested());
        }
    }
    let mut d = darkbot_bsc_record::Draft::new(&rs::SCHEMA);
    d.set(rs::GENERATION, 1u64)
        .set(rs::PROVENANCE_TOOL, "bsc_runtime")
        .set(rs::PROVENANCE_OBSERVED_UNIX_MS, written)
        .set(rs::STARTED_UNIX_MS, written)
        .set(rs::LAUNCHES, 0u64)
        .set(rs::JOURNAL_PATH, "bsc-runtime/journal/4.dbjnl")
        .set(rs::JOURNAL_RECORDS, 0u64)
        .set(rs::JOURNAL_LOST, 0u64)
        .set(rs::MODE, "READ_ONLY")
        .set(rs::STATUS_INTERVAL_MS, 1_000u64)
        .set(rs::STATUS_STALE_INTERVALS, 3u64)
        .set(rs::STRATEGY, st.into_nested());
    std::fs::create_dir_all(rundir.join("bsc-runtime")).unwrap();
    std::fs::write(rundir.join("bsc-runtime/status.dbrec"), d.encode().unwrap()).unwrap();
}

/// BSC-V8 (settings query): the stored generation, the effective basis
/// over the configuration file (all ten keys, the read-only values) and its
/// canonical hash, the parser's bounds; nothing runs: NOT_RUNNING.
#[test]
fn the_settings_state_is_the_owners_with_the_parsers_bounds() {
    use w::bsc_query_request::Query;
    let dir = rundir("settings");
    let owner = settings::Owner::open(&dir.join("bsc-sources")).unwrap();
    owner
        .set_all(
            "s1",
            &[
                ("BSC_STRATEGY_C_MAX_OPEN", "3"),
                ("BSC_STRATEGY_C_FIRST_BUYER_WINDOW", "7"),
                (
                    "BSC_STRATEGY_C_FILTERS",
                    "buy_count:2:,price_change:-0.2:0.5",
                ),
            ],
            0,
            &config(),
            1,
        )
        .unwrap();
    let r = ask(
        Query::StrategySettings(w::StrategyCSettingsQuery {
            include_features: true,
        }),
        &dir,
    );
    assert_eq!(
        (r.request_id.as_str(), r.availability),
        ("q1", w::Availability::Available as i32)
    );
    let Some(w::bsc_query_response::Result::StrategySettings(s)) = r.result else {
        panic!("no settings state")
    };
    assert_eq!((s.stored_revision, s.request_id.as_str()), (1, "s1"));
    let stored = s.stored.unwrap();
    assert_eq!(
        (stored.max_open, stored.first_buyer_window, stored.hold_ms),
        (Some(3), Some(7), None)
    );
    // The rules back as typed rules, in their order, an open bound absent.
    let rules: Vec<_> = stored
        .filters
        .iter()
        .map(|f| {
            (
                f.feature.as_str(),
                f.enabled,
                f.min.as_deref(),
                f.max.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        rules,
        [
            ("buy_count", true, Some("2"), None),
            ("price_change", true, Some("-0.2"), Some("0.5"))
        ]
    );
    let effective = s.effective.unwrap();
    assert_eq!(
        (effective.max_open, effective.first_buyer_window),
        (Some(3), Some(7))
    );
    assert!(effective.hold_ms.is_some() && effective.gas_price_wei.is_some());
    // The typed configuration's enums (the file: FREE_CANCEL, ENABLED).
    assert_eq!(
        (
            effective.creator_sold_before_fill,
            effective.creator_sell_exit
        ),
        (
            w::CreatorSoldBeforeFill::FreeCancel as i32,
            w::CreatorSellExit::Enabled as i32
        )
    );
    let values = settings::effective_values(
        &config(),
        &settings::current(&dir.join("bsc-sources")).unwrap(),
    );
    assert_eq!(
        s.stored_config_sha256,
        darkbot_bsc_record::hex(&settings::config_sha256(&values))
    );
    assert_eq!(s.apply_status, w::SettingsApplyStatus::NotRunning as i32);
    assert!(s.running.is_none() && s.running_revision.is_none());
    let spec = |k: &str| s.setting_specs.iter().find(|p| p.key == k).unwrap().clone();
    let window = spec("BSC_STRATEGY_C_FIRST_BUYER_WINDOW");
    assert_eq!(
        (
            window.unit.as_str(),
            window.min.as_deref(),
            window.max.as_deref()
        ),
        ("count", Some("1"), Some("100"))
    );
    // The extreme multiples the parser accepts (stop < 1x < target).
    let places = effective.ratio_places.unwrap();
    let one_unit = |sign: i128| {
        darkbot_bsc_trading::strategy::fixed::text(
            darkbot_bsc_trading::strategy::fixed::scale(places).unwrap() + sign,
            places,
        )
    };
    let (target, stop) = (
        spec("BSC_STRATEGY_C_TARGET_NET_MULTIPLE"),
        spec("BSC_STRATEGY_C_STOP_NET_MULTIPLE"),
    );
    assert_eq!(target.min, Some(one_unit(1)));
    assert_eq!(stop.max, Some(one_unit(-1)));
    for (key, value) in [
        (
            "BSC_STRATEGY_C_TARGET_NET_MULTIPLE",
            target.min.clone().unwrap(),
        ),
        (
            "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
            stop.max.clone().unwrap(),
        ),
    ] {
        let text = settings::overlay(&config(), &[(settings::key(key).unwrap(), value.clone())]);
        assert!(
            darkbot_bsc_execution::runtime::RuntimeConfig::parse(&text).is_ok(),
            "{key}={value}"
        );
    }
    assert_eq!(s.setting_specs.len(), settings::KEYS.len());
    assert!(
        s.setting_specs
            .iter()
            .all(|p| ["wei", "ratio", "ms", "count", "enum", "rules"].contains(&p.unit.as_str()))
    );
    let signed: Vec<_> = s
        .features
        .iter()
        .filter(|f| f.signed)
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(signed, ["net_buy_flow_bnb", "price_change"]);
    assert_eq!(s.features.iter().filter(|f| !f.filterable).count(), 1);
    assert!(s.features.iter().all(|f| {
        ["count", "bnb", "ratio", "percent", "milliseconds", "flag"].contains(&f.unit.as_str())
    }));
    let provenance = s.provenance.unwrap();
    assert_eq!(provenance.provenance, "NATIVE_CREATED");
    assert!(provenance.evidence.is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

/// Review H1/M1: the running strategy's basis only from a current runtime
/// status: in effect when it took the stored revision (the same hash),
/// otherwise saved for the next start; a stale status (a stopped or killed
/// runtime) is NOT_RUNNING with its reason; a refusal is REFUSED with its
/// reason; an unreadable status is incomplete.
#[test]
fn the_running_basis_is_a_current_runtime_status() {
    let dir = rundir("running");
    let text = config();
    let owner = settings::Owner::open(&dir.join("bsc-sources")).unwrap();
    owner
        .set("s1", "BSC_STRATEGY_C_MAX_OPEN", "3", 0, &text, 1)
        .unwrap();
    let stored = settings::current(&dir.join("bsc-sources")).unwrap();
    let took = settings::effective_values(&text, &stored);
    let took: Vec<(&str, &str)> = took.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let at = |now| strategy_settings_state_at(false, &root(), &dir, now);
    status(&dir, 1_000_000, Some((1, &took)), None);
    let s = at(1_001_000);
    assert_eq!(s.apply_status, w::SettingsApplyStatus::InEffect as i32);
    assert_eq!(s.running_revision, Some(1));
    assert_eq!(s.running_config_sha256, s.stored_config_sha256);
    assert_eq!(s.running.unwrap().max_open, Some(3));
    // Another revision running: the stored one applies at the next start.
    status(&dir, 1_000_000, Some((0, &took[1..])), None);
    let s = at(1_001_000);
    assert_eq!(
        s.apply_status,
        w::SettingsApplyStatus::SavedAppliesNextStart as i32
    );
    assert_eq!(s.running_revision, Some(0));
    // The same revision with another basis (a changed file): next start.
    let mut other = took.clone();
    other[0].1 = "1";
    status(&dir, 1_000_000, Some((1, &other)), None);
    assert_eq!(
        at(1_001_000).apply_status,
        w::SettingsApplyStatus::SavedAppliesNextStart as i32
    );
    // Stale: older than three intervals.
    let s = at(1_010_000);
    assert_eq!(s.apply_status, w::SettingsApplyStatus::NotRunning as i32);
    assert!(s.running.is_none());
    assert_eq!(
        s.provenance.unwrap().quality_reasons,
        ["RUNTIME_STATUS_STALE"]
    );
    // Refused at its start.
    status(
        &dir,
        1_000_000,
        None,
        Some("BSC_STRATEGY_C_MAX_OPEN out of range"),
    );
    let s = at(1_001_000);
    assert_eq!(s.apply_status, w::SettingsApplyStatus::Refused as i32);
    assert_eq!(
        s.refusal_reason,
        "SETTINGS_REFUSED: BSC_STRATEGY_C_MAX_OPEN out of range"
    );
    // Unreadable.
    std::fs::write(dir.join("bsc-runtime/status.dbrec"), b"DBREC001 broken").unwrap();
    let s = at(1_001_000);
    assert_eq!(s.apply_status, w::SettingsApplyStatus::Unspecified as i32);
    let p = s.provenance.unwrap();
    assert_eq!(p.quality, w::DataQuality::Incomplete as i32);
    assert!(p.quality_reasons[0].starts_with("RUNTIME_STATUS_INVALID"));
    std::fs::remove_dir_all(dir).unwrap();
}

/// The bounds table is the parser's: each bounded key's lowest and highest
/// value parse, one past them is refused.
#[test]
fn the_bounds_table_is_what_the_parser_takes() {
    use darkbot_bsc_execution::runtime::{RuntimeConfig, STRATEGY_SETTING_BOUNDS};
    let text = config();
    let parses = |key: &str, v: u64| {
        RuntimeConfig::parse(&settings::overlay(
            &text,
            &[(settings::key(key).unwrap(), v.to_string())],
        ))
        .map_err(|e| e.to_string())
    };
    for (key, _, lo, hi) in STRATEGY_SETTING_BOUNDS {
        assert!(parses(key, lo).is_ok(), "{key}={lo}: {:?}", parses(key, lo));
        if lo > 0 {
            assert!(parses(key, lo - 1).is_err(), "{key}={}", lo - 1);
        }
        if hi < u64::MAX {
            let at_hi = parses(key, hi);
            // A bound past another key's (the hold within its sell bound)
            // names that key, never this one's range.
            if let Err(e) = &at_hi {
                assert!(!e.contains(&format!("{key}: ")), "{key}={hi}: {e}");
            }
            let past = parses(key, hi + 1).unwrap_err();
            assert!(past.contains(key), "{key}={}: {past}", hi + 1);
        }
    }
}

/// BSC-V8-01: the profiles of the owner's current catalog at their revision
/// with their hash and every value; archived ones on request, deleted ones
/// never; the catalog version is the next create's CAS.
#[test]
fn the_profiles_are_the_owners_catalog() {
    use darkbot_bsc_execution::strategy_profiles::{Owner, Request};
    use w::bsc_query_request::Query;
    let dir = rundir("profiles");
    let ask_profiles = |include_archived| {
        let r = ask(
            Query::TrainingProfiles(w::TrainingProfilesQuery { include_archived }),
            &dir,
        );
        assert_eq!(r.availability, w::Availability::Available as i32);
        match r.result {
            Some(w::bsc_query_response::Result::TrainingProfiles(p)) => p,
            other => panic!("{other:?}"),
        }
    };
    let none = ask_profiles(false);
    assert_eq!((none.catalog_version, none.profiles.len()), (0, 0));
    let owner = Owner::open(&dir.join("bsc-sources")).unwrap();
    let values: Vec<(String, String)> = settings::effective_values(&config(), &Default::default())
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    for (i, name) in ["a", "b", "c"].into_iter().enumerate() {
        let create = Request::Create {
            name: name.into(),
            venues: vec!["BSC_VENUE_FLAP".into()],
            values: values.clone(),
        };
        let r = owner
            .apply(name, &create, i as u64, &config(), &|_| None, 1_000)
            .unwrap();
        assert!(r.applied(), "{}", r.line());
    }
    for (id, archive, req) in [("p000002", true, "x"), ("p000003", false, "y")] {
        let delete = Request::Delete {
            profile_id: id.into(),
            archive,
        };
        assert!(
            owner
                .apply(req, &delete, 1, &config(), &|_| None, 2_000)
                .unwrap()
                .applied()
        );
    }
    let p = ask_profiles(false);
    assert_eq!(p.catalog_version, 5);
    let a = &p.profiles[..];
    assert_eq!(a.len(), 1);
    assert_eq!(
        (
            a[0].profile_id.as_str(),
            a[0].name.as_str(),
            a[0].revision,
            a[0].request_id.as_str()
        ),
        ("p000001", "a", 1, "a")
    );
    assert_eq!(a[0].status, w::TrainingProfileStatus::Active as i32);
    assert_eq!(a[0].venues, ["BSC_VENUE_FLAP"]);
    let pairs: Vec<(&'static str, String)> = values
        .iter()
        .map(|(k, v)| (settings::key(k).unwrap(), v.clone()))
        .collect();
    assert_eq!(
        a[0].config_sha256,
        darkbot_bsc_record::hex(&settings::config_sha256(&pairs))
    );
    let v = a[0].values.clone().unwrap();
    assert_eq!(
        (v.max_open, v.first_buyer_window, v.creator_sell_exit),
        (Some(5), Some(5), w::CreatorSellExit::Enabled as i32)
    );
    assert_eq!(v.filters.len(), 2);
    let provenance = p.provenance.unwrap();
    assert_eq!(
        (provenance.owner_version, provenance.provenance.as_str()),
        (5, "NATIVE_CREATED")
    );
    assert!(
        provenance
            .evidence
            .unwrap()
            .path
            .ends_with("00000000000000000005.dbrec")
    );
    let with_archived = ask_profiles(true);
    let ids: Vec<_> = with_archived
        .profiles
        .iter()
        .map(|p| p.profile_id.as_str())
        .collect();
    assert_eq!(ids, ["p000001", "p000002"]);
    assert_eq!(
        with_archived.profiles[1].status,
        w::TrainingProfileStatus::Archived as i32
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// Every other query names its missing BSC-V8 owner; garbage is refused.
#[test]
fn a_query_without_an_owner_is_not_implemented() {
    use w::bsc_query_request::Query;
    let dir = rundir("missing");
    let r = ask(Query::Treasury(Default::default()), &dir);
    assert_eq!(r.availability, w::Availability::NotImplemented as i32);
    assert!(
        r.unavailable_reason.contains("BSC-V8-13"),
        "{}",
        r.unavailable_reason
    );
    assert!(r.result.is_none());
    let junk =
        w::BscQueryResponse::decode(answer(&[0xff, 0xff], &root(), &dir).as_slice()).unwrap();
    assert_eq!(junk.availability, w::Availability::Unavailable as i32);
    std::fs::remove_dir_all(dir).unwrap();
}
