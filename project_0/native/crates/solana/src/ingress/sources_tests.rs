use super::*;
use crate::ingress::{AccountUpdate, SlotStatus, SlotUpdate, SourceKind};

fn geyser(provider: &str) -> SourceUnit {
    SourceUnit::BinaryGeyser {
        provider: provider.into(),
    }
}

fn qualified() -> Qualification {
    Qualification::Qualified {
        evidence: "ledger entry".into(),
    }
}

/// Operating mode and qualification are separate: OFF never connects, OBSERVE never feeds
/// canonical state, ACTIVE needs a qualification, a sensor never feeds state, an unimplemented
/// unit can only be OFF; the mode text is parsed strictly.
#[test]
fn operating_mode_and_qualification_are_separate() {
    assert_eq!("observe".parse(), Ok(OperatingMode::Observe));
    assert_eq!("on".parse::<OperatingMode>(), Err("SOURCE_MODE_INVALID"));
    let off = SourceControl::new(
        geyser("publicnode"),
        OperatingMode::Off,
        Qualification::NotRun,
    )
    .unwrap();
    assert!(!off.may_connect() && !off.feeds_canonical_state());
    let observe = SourceControl::new(
        geyser("publicnode"),
        OperatingMode::Observe,
        Qualification::LocalPass,
    )
    .unwrap();
    assert!(observe.may_connect() && !observe.feeds_canonical_state());
    for q in [
        Qualification::NotRun,
        Qualification::LocalPass,
        Qualification::Blocked("X"),
    ] {
        assert_eq!(
            SourceControl::new(geyser("publicnode"), OperatingMode::Active, q),
            Err("SOURCE_ACTIVE_REQUIRES_QUALIFICATION")
        );
    }
    let active =
        SourceControl::new(geyser("publicnode"), OperatingMode::Active, qualified()).unwrap();
    assert!(active.feeds_canonical_state());
    let deshred = SourceUnit::Deshred {
        provider: "publicnode".into(),
    };
    assert_eq!(
        SourceControl::new(
            deshred.clone(),
            OperatingMode::Observe,
            Qualification::NotRun
        ),
        Err("SOURCE_NOT_IMPLEMENTED")
    );
    assert!(SourceControl::new(deshred, OperatingMode::Off, Qualification::NotRun).is_ok());
    let sensor = SourceControl {
        unit: SourceUnit::LocalShred,
        mode: OperatingMode::Active,
        qualification: qualified(),
    };
    assert_eq!(sensor.unit.role(), SourceRole::PreExecutionSensor);
    assert!(
        !sensor.feeds_canonical_state(),
        "a sensor never feeds canonical state"
    );
}

/// S1-08 admission with stated reasons: OBSERVE runs with unproven capabilities but needs the
/// token from the environment; ACTIVE needs every used capability proven; an absent capability,
/// an exceeded quota, a plaintext remote endpoint or a logs subscription reject in any mode.
#[test]
fn providers_are_admitted_with_reasons() {
    let p = ProviderConfig::publicnode();
    let token = |k: &str| (k == "P0_GEYSER_PUBLICNODE_TOKEN").then(|| "t".to_string());
    let none = |_: &str| None;
    let empty = |_: &str| Some(" ".to_string());
    let s = Subscription {
        programs: vec![Pubkey::new_unique()],
        slots: true,
        roots: true,
        ..Default::default()
    };
    assert!(p.rejections(OperatingMode::Observe, &s, &token).is_empty());
    assert!(p.rejections(OperatingMode::Off, &s, &none).is_empty());
    assert_eq!(
        p.rejections(OperatingMode::Observe, &s, &none),
        ["PROVIDER_AUTH_TOKEN_MISSING"]
    );
    assert_eq!(
        p.rejections(OperatingMode::Observe, &s, &empty),
        ["PROVIDER_AUTH_TOKEN_MISSING"]
    );
    assert_eq!(
        p.rejections(OperatingMode::Active, &s, &token),
        ["PROVIDER_CAPABILITY_UNPROVEN"]
    );
    let mut absent = p.clone();
    absent.capabilities.slots = Capability::Absent;
    assert_eq!(
        absent.rejections(OperatingMode::Observe, &s, &token),
        ["PROVIDER_CAPABILITY_ABSENT"]
    );
    let mut proven = p.clone();
    for c in [
        &mut proven.capabilities.accounts,
        &mut proven.capabilities.slots,
        &mut proven.capabilities.roots,
    ] {
        *c = Capability::Proven;
    }
    assert!(proven
        .rejections(OperatingMode::Active, &s, &token)
        .is_empty());
    let many = Subscription {
        programs: (0..17).map(|_| Pubkey::new_unique()).collect(),
        logs_mentioning: vec![Pubkey::new_unique()],
        ..Default::default()
    };
    assert_eq!(
        p.rejections(OperatingMode::Observe, &many, &token),
        [
            "PROVIDER_LOGS_NOT_A_GEYSER_STREAM",
            "PROVIDER_QUOTA_OWNER_FILTERS"
        ]
    );
    let mut plaintext = p.clone();
    plaintext.endpoint = "http://solana.publicnode.com".into();
    plaintext.quota.max_streams = 0;
    assert_eq!(
        plaintext.rejections(OperatingMode::Observe, &s, &token),
        [
            "PROVIDER_ENDPOINT_OR_TLS_INVALID",
            "PROVIDER_QUOTA_NO_STREAM"
        ]
    );
}

/// The production entry: OFF constructs and starts nothing; OBSERVE builds the source for its
/// provider (replay only when proven) without connecting; a refused provider or a unit of
/// another provider builds nothing. No test calls `run` against a remote endpoint.
#[test]
fn geyser_source_is_built_only_for_an_admitted_unit() {
    let p = ProviderConfig::publicnode();
    let token = |k: &str| (k == "P0_GEYSER_PUBLICNODE_TOKEN").then(|| "t".to_string());
    let s = Subscription {
        slots: true,
        ..Default::default()
    };
    let control =
        |mode| SourceControl::new(geyser("publicnode"), mode, Qualification::NotRun).unwrap();
    assert!(
        geyser_source(&control(OperatingMode::Off), &p, s.clone(), &token)
            .unwrap()
            .is_none()
    );
    let built = geyser_source(&control(OperatingMode::Observe), &p, s.clone(), &token)
        .unwrap()
        .unwrap();
    assert_eq!(built.config.endpoint, "https://solana.publicnode.com");
    assert_eq!(built.config.max_reconnects, 5);
    assert!(!built.config.replay_from_slot, "replay unproven");
    assert!(
        format!("{:?}", built.transport).contains("<redacted>"),
        "token never printed"
    );
    assert_eq!(
        geyser_source(&control(OperatingMode::Observe), &p, s.clone(), &|_| None).err(),
        Some(vec!["PROVIDER_AUTH_TOKEN_MISSING"])
    );
    let other = SourceControl::new(
        geyser("other"),
        OperatingMode::Observe,
        Qualification::NotRun,
    )
    .unwrap();
    assert_eq!(
        geyser_source(&other, &p, s, &token).err(),
        Some(vec!["SOURCE_UNIT_PROVIDER_MISMATCH"])
    );
}

fn account(
    source: SourceKind,
    k: Pubkey,
    slot: u64,
    wv: Option<u64>,
    lamports: u64,
    t: u64,
) -> IngressEvent {
    IngressEvent::Account(AccountUpdate {
        pubkey: k,
        owner: Pubkey::new_from_array([1; 32]),
        lamports,
        data: vec![1],
        slot,
        write_version: wv,
        source,
        t_rx_ns: t,
    })
}

/// Several sources at once: one canonical unit (a second is refused), every other unit only
/// observes, OFF units and sensors are rejected; two different copies of the same write are a
/// conflict — also against the canonical unit, never resolved by arrival order; the lead of an
/// observed source over the canonical one is measured.
#[test]
fn arbiter_keeps_one_canonical_source_and_reports_conflicts() {
    let wss =
        SourceControl::new(SourceUnit::WsRpcControl, OperatingMode::Active, qualified()).unwrap();
    let a = SourceControl::new(geyser("a"), OperatingMode::Active, qualified()).unwrap();
    let b = SourceControl::new(geyser("b"), OperatingMode::Observe, Qualification::NotRun).unwrap();
    let off = SourceControl::new(geyser("c"), OperatingMode::Off, Qualification::NotRun).unwrap();
    let sensor = SourceControl {
        unit: SourceUnit::LocalShred,
        mode: OperatingMode::Observe,
        qualification: Qualification::NotRun,
    };
    assert_eq!(
        SourceArbiter::new(&[wss.clone(), a.clone()]).err(),
        Some("ARBITER_SECOND_CANONICAL_SOURCE")
    );
    let mut arb = SourceArbiter::new(&[a.clone(), b.clone(), off.clone()]).unwrap();
    let k = Pubkey::new_unique();
    // b sees the write first (100 ns), the canonical a later (150 ns): b led by 50 ns.
    assert!(matches!(
        arb.route(&b, account(SourceKind::Geyser, k, 5, Some(1), 10, 100)),
        Route::Observed(_)
    ));
    assert!(matches!(
        arb.route(&a, account(SourceKind::Geyser, k, 5, Some(1), 10, 150)),
        Route::Canonical(_)
    ));
    assert_eq!(
        arb.leads,
        [Lead {
            pubkey: k,
            slot: 5,
            lead_ns: 50
        }]
    );
    // The canonical unit contradicting an observed copy of the same write: conflict, not applied.
    let k2 = Pubkey::new_unique();
    arb.route(&b, account(SourceKind::Geyser, k2, 6, Some(2), 10, 1));
    assert_eq!(
        arb.route(&a, account(SourceKind::Geyser, k2, 6, Some(2), 11, 2)),
        Route::Conflict {
            pubkey: k2,
            slot: 6,
            write_version: 2
        }
    );
    assert_eq!(arb.conflicts, 1);
    assert_eq!(
        arb.route(&off, account(SourceKind::Geyser, k, 7, Some(1), 1, 1)),
        Route::Rejected("SOURCE_OFF")
    );
    assert_eq!(
        arb.route(&sensor, account(SourceKind::Geyser, k, 7, Some(1), 1, 1)),
        Route::Rejected("SENSOR_NEVER_FEEDS_STATE")
    );
    // Without a qualified unit nothing is canonical: OBSERVE never feeds state.
    let mut observing = SourceArbiter::new(std::slice::from_ref(&b)).unwrap();
    let slot = IngressEvent::Slot(SlotUpdate {
        slot: 9,
        parent: None,
        status: SlotStatus::Processed,
        source: SourceKind::Geyser,
        t_rx_ns: 1,
    });
    assert!(matches!(observing.route(&b, slot), Route::Observed(_)));
}

/// Health never hides a gap: a gap or reconnect stays visible through later data until an
/// explicit recovery; a fatal source stays unhealthy.
#[test]
fn health_never_hides_a_gap() {
    let mut h = SourceHealth::default();
    let slot = |s| {
        IngressEvent::Slot(SlotUpdate {
            slot: s,
            parent: None,
            status: SlotStatus::Processed,
            source: SourceKind::Geyser,
            t_rx_ns: 1,
        })
    };
    h.observe(&slot(1));
    assert!(h.healthy());
    h.observe(&IngressEvent::Gap {
        source: SourceKind::Geyser,
        from_slot: Some(1),
        to_slot: None,
        t_rx_ns: 2,
    });
    h.observe(&slot(2));
    assert!(!h.healthy(), "later data does not close a gap");
    h.recovered();
    assert!(h.healthy());
    h.observe(&IngressEvent::Reconnected {
        source: SourceKind::Geyser,
        attempt: 2,
        t_rx_ns: 3,
    });
    assert!(!h.healthy(), "a reconnect needs a recovery");
    h.recovered();
    h.observe(&IngressEvent::Fatal {
        source: SourceKind::Geyser,
        detail: "x".into(),
        t_rx_ns: 4,
    });
    h.recovered();
    assert!(!h.healthy(), "fatal stays failed");
    assert_eq!((h.gaps, h.reconnects, h.last_slot), (1, 1, Some(2)));
}
