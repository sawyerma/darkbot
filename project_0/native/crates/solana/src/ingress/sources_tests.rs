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
/// observes, OFF units and sensors get no stamp; two different copies of the same write are a
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
    assert_eq!(
        SourceArbiter::new(&[b.clone(), b.clone()]).err(),
        Some("ARBITER_DUPLICATE_UNIT")
    );
    let mut arb = SourceArbiter::new(&[a.clone(), b.clone(), off.clone(), sensor.clone()]).unwrap();
    let (sa, sb) = (arb.stamp(&a.unit).unwrap(), arb.stamp(&b.unit).unwrap());
    let k = Pubkey::new_unique();
    // b sees the write first (100 ns), the canonical a later (150 ns): b led by 50 ns.
    assert!(matches!(
        arb.route(&sb, account(SourceKind::Geyser, k, 5, Some(1), 10, 100)),
        Route::Observed(_)
    ));
    assert!(matches!(
        arb.route(&sa, account(SourceKind::Geyser, k, 5, Some(1), 10, 150)),
        Route::Canonical(_)
    ));
    assert_eq!(
        arb.leads.recent().copied().collect::<Vec<_>>(),
        [Lead {
            pubkey: k,
            slot: 5,
            lead_ns: 50
        }]
    );
    // The canonical unit contradicting an observed copy of the same write: conflict, not applied.
    let k2 = Pubkey::new_unique();
    arb.route(&sb, account(SourceKind::Geyser, k2, 6, Some(2), 10, 1));
    assert_eq!(
        arb.route(&sa, account(SourceKind::Geyser, k2, 6, Some(2), 11, 2)),
        Route::Conflict {
            pubkey: k2,
            slot: 6,
            write_version: 2
        }
    );
    assert_eq!(arb.conflicts, 1);
    assert_eq!(arb.stamp(&off.unit), Err("SOURCE_OFF"));
    assert_eq!(arb.stamp(&sensor.unit), Err("SENSOR_NEVER_FEEDS_STATE"));
    assert_eq!(arb.stamp(&geyser("unlisted")), Err("SOURCE_UNKNOWN"));
    // A forged stamp of an OFF unit is still refused.
    let forged = Stamp {
        unit: off.unit.clone(),
        generation: 1,
    };
    assert_eq!(
        arb.route(&forged, account(SourceKind::Geyser, k, 7, Some(1), 1, 1)),
        Route::Rejected("SOURCE_OFF")
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
    let sb = observing.stamp(&b.unit).unwrap();
    assert!(matches!(observing.route(&sb, slot), Route::Observed(_)));
    assert_eq!(observing.canonical(), None);
}

fn slot_event(slot: u64) -> IngressEvent {
    IngressEvent::Slot(SlotUpdate {
        slot,
        parent: None,
        status: SlotStatus::Processed,
        source: SourceKind::Geyser,
        t_rx_ns: 1,
    })
}

/// A23-03: ACTIVE→OBSERVE revokes canonical authority at once — events queued under the old
/// stamp are refused (counted), a new session only observes; ACTIVE→OFF the same, and OFF gets
/// no stamp. Promotion OBSERVE→ACTIVE needs a new stamp too: queued observe events never
/// become canonical retroactively.
#[test]
fn a23_03_mode_change_revokes_canonical_and_refuses_queued_events() {
    let a = SourceControl::new(geyser("a"), OperatingMode::Active, qualified()).unwrap();
    let observe = |q| SourceControl::new(geyser("a"), OperatingMode::Observe, q).unwrap();
    let mut arb = SourceArbiter::new(std::slice::from_ref(&a)).unwrap();
    let active = arb.stamp(&a.unit).unwrap();
    assert!(matches!(
        arb.route(&active, slot_event(1)),
        Route::Canonical(_)
    ));
    assert_eq!(
        arb.change(observe(qualified())),
        Ok(Handoff::CanonicalRevoked {
            unit: a.unit.clone()
        })
    );
    assert_eq!(arb.canonical(), None);
    assert_eq!(
        arb.route(&active, slot_event(2)),
        Route::Rejected("SOURCE_GENERATION_STALE")
    );
    let observing = arb.stamp(&a.unit).unwrap();
    assert!(matches!(
        arb.route(&observing, slot_event(3)),
        Route::Observed(_)
    ));
    assert_eq!(
        arb.change(a.clone()),
        Ok(Handoff::CanonicalGranted {
            unit: a.unit.clone()
        })
    );
    assert_eq!(
        arb.route(&observing, slot_event(4)),
        Route::Rejected("SOURCE_GENERATION_STALE"),
        "an observe-session event never becomes canonical"
    );
    let active = arb.stamp(&a.unit).unwrap();
    assert!(matches!(
        arb.route(&active, slot_event(5)),
        Route::Canonical(_)
    ));
    let off = SourceControl::new(geyser("a"), OperatingMode::Off, Qualification::NotRun).unwrap();
    assert_eq!(
        arb.change(off),
        Ok(Handoff::CanonicalRevoked {
            unit: a.unit.clone()
        })
    );
    assert_eq!(arb.stamp(&a.unit), Err("SOURCE_OFF"));
    assert_eq!(
        arb.route(&active, slot_event(6)),
        Route::Rejected("SOURCE_GENERATION_STALE")
    );
    assert_eq!(arb.stale, 3);
}

/// A23-03: a revoked qualification (even if a caller keeps the mode ACTIVE) ends canonical
/// authority; while one unit is canonical a second promotion is refused without any change
/// (its stamps stay valid); after the revocation the other unit can take over. A reconnect
/// under an unchanged control keeps its stamp (the owner's recovery handles it); a
/// re-qualification is a change and refuses the old session.
#[test]
fn a23_03_revocation_parallel_sources_and_reconnect() {
    let a = SourceControl::new(geyser("a"), OperatingMode::Active, qualified()).unwrap();
    let b = SourceControl::new(geyser("b"), OperatingMode::Observe, Qualification::NotRun).unwrap();
    let mut arb = SourceArbiter::new(&[a.clone(), b.clone()]).unwrap();
    let (sa, sb) = (arb.stamp(&a.unit).unwrap(), arb.stamp(&b.unit).unwrap());
    let b_active = SourceControl::new(geyser("b"), OperatingMode::Active, qualified()).unwrap();
    assert_eq!(
        arb.change(b_active.clone()),
        Err("ARBITER_SECOND_CANONICAL_SOURCE")
    );
    assert_eq!(arb.control(&b.unit), Some(&b));
    assert!(matches!(arb.route(&sb, slot_event(1)), Route::Observed(_)));
    let reconnected = IngressEvent::Reconnected {
        source: SourceKind::Geyser,
        attempt: 2,
        t_rx_ns: 1,
    };
    assert!(matches!(arb.route(&sa, reconnected), Route::Canonical(_)));
    let revoked = SourceControl {
        unit: a.unit.clone(),
        mode: OperatingMode::Active,
        qualification: Qualification::Blocked("QUALIFICATION_REVOKED"),
    };
    assert_eq!(
        arb.change(revoked),
        Ok(Handoff::CanonicalRevoked {
            unit: a.unit.clone()
        })
    );
    assert_eq!(
        arb.route(&sa, slot_event(2)),
        Route::Rejected("SOURCE_GENERATION_STALE")
    );
    let sa = arb.stamp(&a.unit).unwrap();
    assert!(matches!(arb.route(&sa, slot_event(3)), Route::Observed(_)));
    assert_eq!(
        arb.change(b_active),
        Ok(Handoff::CanonicalGranted {
            unit: b.unit.clone()
        })
    );
    let sb2 = arb.stamp(&b.unit).unwrap();
    assert!(matches!(
        arb.route(&sb2, slot_event(4)),
        Route::Canonical(_)
    ));
    assert_eq!(
        arb.route(&sb, slot_event(5)),
        Route::Rejected("SOURCE_GENERATION_STALE")
    );
    assert_eq!(
        arb.change(SourceControl::new(geyser("b"), OperatingMode::Active, qualified()).unwrap()),
        Ok(Handoff::None)
    );
    assert_eq!(
        arb.route(&sb2, slot_event(6)),
        Route::Rejected("SOURCE_GENERATION_STALE")
    );
    assert_eq!(
        arb.change(
            SourceControl::new(geyser("x"), OperatingMode::Off, Qualification::NotRun).unwrap()
        ),
        Err("SOURCE_UNKNOWN")
    );
}

/// A23-04: lead telemetry stays bounded far above its retention — the newest samples kept,
/// every evicted one counted, every lead in the histograms; the histogram quantiles bound the
/// exact quantiles of all samples from above within one bucket (≤ 12.5 %).
#[test]
fn a23_04_lead_telemetry_is_bounded_with_evicted_evidence() {
    let a = SourceControl::new(geyser("a"), OperatingMode::Active, qualified()).unwrap();
    let b = SourceControl::new(geyser("b"), OperatingMode::Observe, Qualification::NotRun).unwrap();
    let mut arb = SourceArbiter::new(&[a.clone(), b.clone()]).unwrap();
    let (sa, sb) = (arb.stamp(&a.unit).unwrap(), arb.stamp(&b.unit).unwrap());
    let n = 3 * LEAD_RETENTION as u64;
    let mut exact = Vec::new();
    for i in 0..n {
        let k = Pubkey::new_unique();
        let lead = 1_000 + (i * 7_919) % 2_000_000;
        exact.push(lead);
        arb.route(&sb, account(SourceKind::Geyser, k, 5, Some(1), 10, 0));
        arb.route(&sa, account(SourceKind::Geyser, k, 5, Some(1), 10, lead));
    }
    // One write the canonical unit saw first: the observed one is behind.
    let k = Pubkey::new_unique();
    arb.route(&sa, account(SourceKind::Geyser, k, 6, Some(1), 10, 0));
    arb.route(&sb, account(SourceKind::Geyser, k, 6, Some(1), 10, 40));
    let s = arb.leads.snapshot();
    assert_eq!(arb.leads.recent().count(), LEAD_RETENTION);
    assert_eq!(
        (s.samples, s.evicted),
        (n + 1, n + 1 - LEAD_RETENTION as u64)
    );
    assert_eq!((s.ahead_ns.count, s.behind_ns.count), (n, 1));
    assert_eq!(s.behind_ns.max, Some(40));
    assert_eq!(
        arb.leads.recent().last().map(|l| l.lead_ns),
        Some(-40),
        "the newest sample is retained"
    );
    exact.sort_unstable();
    for (q, got) in [
        (0.50, s.ahead_ns.p50),
        (0.95, s.ahead_ns.p95),
        (0.99, s.ahead_ns.p99),
    ] {
        let want = exact[((q * n as f64).ceil() as usize).clamp(1, exact.len()) - 1];
        let got = got.unwrap();
        assert!(
            got >= want && got <= want + want / 8 + 1,
            "q{q}: {got} vs {want}"
        );
    }
    assert_eq!(s.ahead_ns.max, exact.last().copied());
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
