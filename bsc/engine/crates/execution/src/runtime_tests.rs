use super::*;

/// BSC3-076 re-review HIGH: a lease that holds the grant's and the day's
/// last slot keeps the runtime's reconcile on (the venue, the runtime,
/// C2, C3 and the sources stay LIVE for its BUY); any other attempt
/// still finds them used.
#[test]
fn a_lease_on_the_last_slot_keeps_the_reconcile_on() {
    use crate::sources::tests::{ORIGIN, grant_of, live_ready_limited, qualified_in};
    use crate::sources::{
        BUY_LIMIT_EXHAUSTED, LIVE_GRANT_ATTEMPTS_EXHAUSTED, LiveGrant, LiveIntent, LiveIntentKind,
        LiveStrategies,
    };
    let (dir, s, now) = live_ready_limited("runtime-lease-last", (1, 1), |now| LiveGrant {
        max_attempts: 1,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let attempt = alloy_primitives::hex::encode([0xa0; 32]);
    let id = alloy_primitives::hex::encode([0xa1; 32]);
    let lease = LiveIntent {
        id: &id,
        component: ComponentId::ExecC3,
        kind: LiveIntentKind::Buy,
        input_wei: 1_000,
        max_wei: 1_000,
        token: [0; 20],
        nonce: 7,
        origin: ORIGIN,
        max_block: 900,
        exit_reserve_wei: 0,
        venue: Some(crate::sources::VenueId::Flap),
    };
    s.lease_live(&armed, &lease, &attempt, &q, now).unwrap();
    let g = s.current().unwrap();
    let entities = [
        crate::sources::Entity::Runtime,
        VenueId::Flap.into(),
        ComponentId::StateC2.into(),
        ComponentId::ExecC3.into(),
        SourceId::PublicNode.into(),
    ];
    // The owner state's session (the switches are enabled in it).
    let view = |held: Option<&str>| LiveView {
        boot: Some("boot-1".into()),
        ..LiveView::read(
            &g,
            &dir,
            Some(&q),
            now,
            held,
            crate::sources::tests::shipped_limits().buy_attempts_total_max,
        )
    };
    let other = alloy_primitives::hex::encode([0xb0; 32]);
    let (held, others, none) = (view(Some(&attempt)), view(Some(&other)), view(None));
    for e in entities {
        assert_eq!(held.effective(&g, e, false), Effective::Live, "{e:?}");
        for view in [&others, &none] {
            assert!(
                matches!(
                    view.effective(&g, e, false),
                    Effective::Blocked(r) if r == LIVE_GRANT_ATTEMPTS_EXHAUSTED || r == BUY_LIMIT_EXHAUSTED
                ),
                "{e:?}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Review of the limits (LOW): a grant without limits admits no BUY
/// but keeps the exit path of positions held under it (the reason is
/// exempt for exits).
#[test]
fn a_grant_without_limits_keeps_the_exit_path_on() {
    use crate::sources::tests::{grant_of, live_ready_granting, qualified};
    use crate::sources::{LIVE_GRANT_LIMITS_MISSING, LiveGrant, LiveStrategies};
    let (dir, s, now) = live_ready_granting("runtime-no-limits", |now| LiveGrant {
        limits: None,
        ..grant_of(now + 3_600_000, LiveStrategies::StrategyC)
    });
    let g = s.current().unwrap();
    let view = LiveView {
        boot: Some("boot-1".into()),
        now_ms: now,
        daily: crate::sources::buy_usage(
            &dir,
            now,
            crate::sources::tests::shipped_limits().buy_attempts_total_max,
        )
        .ok(),
        total: crate::sources::total_usage(
            &dir,
            crate::sources::tests::shipped_limits().buy_attempts_total_max,
        )
        .ok(),
        spent: Some(0),
        qualification: qualified(),
    };
    for e in [
        crate::sources::Entity::Runtime,
        ComponentId::StrategyC.into(),
        ComponentId::StateC2.into(),
        VenueId::Flap.into(),
    ] {
        assert_eq!(
            view.effective(&g, e, false),
            Effective::Blocked(LIVE_GRANT_LIMITS_MISSING),
            "{e:?}"
        );
        assert_eq!(view.effective(&g, e, true), Effective::Live, "{e:?}");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-058/071 (review MEDIUM): under a grant that admits no BUY the
/// runtime keeps the entities requested LIVE effective for the exits
/// of the positions the LIVE path holds (the strategy decides them, C2
/// follows them); without positions they stay blocked by that reason.
#[test]
fn a_grant_without_buys_keeps_the_exit_path_on_while_positions_are_held() {
    use crate::sources::tests::{grant_of, live_ready_granting, qualified};
    use crate::sources::{LIVE_GRANT_EXIT_ONLY, LiveGrant, LiveStrategies};
    let (dir, s, now) = live_ready_granting("runtime-exit-only", |now| LiveGrant {
        exit_only: true,
        ..grant_of(now + 3_600_000, LiveStrategies::StrategyC)
    });
    let g = s.current().unwrap();
    let view = LiveView {
        boot: Some("boot-1".into()),
        now_ms: now,
        daily: crate::sources::buy_usage(
            &dir,
            now,
            crate::sources::tests::shipped_limits().buy_attempts_total_max,
        )
        .ok(),
        total: crate::sources::total_usage(
            &dir,
            crate::sources::tests::shipped_limits().buy_attempts_total_max,
        )
        .ok(),
        spent: Some(0),
        qualification: qualified(),
    };
    for e in [
        crate::sources::Entity::Runtime,
        ComponentId::StrategyC.into(),
        ComponentId::StateC2.into(),
        VenueId::Flap.into(),
    ] {
        assert_eq!(
            view.effective(&g, e, false),
            Effective::Blocked(LIVE_GRANT_EXIT_ONLY),
            "{e:?}"
        );
        assert_eq!(view.effective(&g, e, true), Effective::Live, "{e:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

const CONFIG: &str = include_str!("../../../../config/ingress.public.env");
/// R4: the repository's bound builder policy, the frozen import.
const POLICY: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec";
/// The shipped refused accounts: the frozen G0.7 launcher and buyer.
const REFUSED: &str = "BSC_EXEC_REFUSED_ACCOUNTS=0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd,0x9c8a0Db41914beEE52Dd49790c4F087E252743DE\n";

/// Plan 28.10.: the LIVE keys go together, need the executor's keys
/// and a buyer C2 watches; the route is the builders or a loopback port.
/// R10: the shipped config names every key, empty (no LIVE path).
#[test]
fn the_live_keys_go_together_and_need_a_watched_buyer() {
    assert_eq!(RuntimeConfig::parse(CONFIG).unwrap().live, None);
    assert!(CONFIG.contains(
        "\nBSC_LIVE_KEY=\nBSC_LIVE_BUYER=\nBSC_LIVE_ROUTE=\nBSC_LIVE_BLOCKRAZOR_TOKEN_FILE=\n"
    ));
    let buyer = "0x00000000000000000000000000000000000000aB";
    let watched = CONFIG.replace(
        "BSC_STATE_WATCHED=\n",
        &format!("BSC_STATE_WATCHED={buyer}\n"),
    );
    let keys = |base: &str, route: &str| {
        base.replace(
            "BSC_LIVE_KEY=\nBSC_LIVE_BUYER=\nBSC_LIVE_ROUTE=\n",
            &format!(
                "BSC_LIVE_KEY=~/.darkbot/bsc/live/buyer.key\nBSC_LIVE_BUYER={buyer}\n\
                     BSC_LIVE_ROUTE={route}\n"
            ),
        )
    };
    let live = RuntimeConfig::parse(&keys(&watched, "LOOPBACK:18545"))
        .unwrap()
        .live
        .unwrap();
    assert_eq!(live.route, live::LiveRouteConfig::Loopback { port: 18545 });
    assert_eq!(live.buyer[19], 0xab);
    assert!(live.key_path.is_absolute() && live.key_path.ends_with("live/buyer.key"));
    assert_eq!(live.connect_timeout, Duration::from_millis(5000));
    let builders = RuntimeConfig::parse(&keys(&watched, "BUILDERS"))
        .unwrap()
        .live
        .unwrap();
    assert_eq!(
        builders.route,
        live::LiveRouteConfig::Builders {
            blockrazor_token: None
        }
    );
    let token = "BSC_LIVE_BLOCKRAZOR_TOKEN_FILE=/tmp/t\n";
    let slow = |c: String| {
        c.replace(
            "BSC_LIVE_CONNECT_TIMEOUT_MS=5000\n",
            "BSC_LIVE_CONNECT_TIMEOUT_MS=50\n",
        )
    };
    for bad in [
        // Not watched by C2.
        keys(CONFIG, "BUILDERS"),
        // Not all three.
        watched.replace("BSC_LIVE_BUYER=\n", &format!("BSC_LIVE_BUYER={buyer}\n")),
        keys(&watched, "LOOPBACK:0"),
        keys(&watched, "TLS"),
        keys(&watched, "LOOPBACK:1").replace("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE=\n", token),
        // A token without a route; the timeout out of bounds, LIVE or not.
        watched.replace("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE=\n", token),
        slow(keys(&watched, "BUILDERS")),
        slow(watched.clone()),
    ] {
        assert!(RuntimeConfig::parse(&bad).is_err(), "{bad}");
    }
    // BSC3-056: a LIVE path without an independent second source.
    let no_confirm = keys(&watched, "BUILDERS").replace(
        "BSC_CANONICAL_CONFIRM_RPC=https://bsc-rpc.publicnode.com\n",
        "BSC_CANONICAL_CONFIRM_RPC=\n",
    );
    let e = RuntimeConfig::parse(&no_confirm).unwrap_err();
    assert!(e.contains("BSC_CANONICAL_CONFIRM_RPC"), "{e}");
    // R1 review: a sender whose paths leave out Strategy C's could buy
    // and never sell.
    let no_exits = keys(&watched, "BUILDERS").replace(
        "BSC_EXEC_SENDER_PATHS=C3+STRATEGY_C\n",
        "BSC_EXEC_SENDER_PATHS=C3\n",
    );
    let e = RuntimeConfig::parse(&no_exits).unwrap_err();
    assert!(e.contains("BSC_EXEC_SENDER_PATHS with STRATEGY_C"), "{e}");
}

/// Messplan §3b B (offline, at the reconcile's gate): which paths the
/// effective modes let run. Not named components and venues are OFF; a
/// LIVE request without a grant is BLOCKED.
#[test]
fn the_runtime_mode_bounds_the_components_as_the_plan_says() {
    use crate::sources::Entity;
    use Effective::{Blocked, Live, Off, Paper, Shadow};
    #[derive(Clone, Copy)]
    struct Modes {
        runtime: Effective,
        flap: Effective,
        c2: Effective,
        exec: Effective,
        strategy: Effective,
    }
    let off = Modes {
        runtime: Off,
        flap: Off,
        c2: Off,
        exec: Off,
        strategy: Off,
    };
    let run = |m: Modes| {
        gates(
            |e| match e {
                Entity::Runtime => m.runtime,
                Entity::Venue(VenueId::Flap) => m.flap,
                Entity::Component(ComponentId::StateC2) => m.c2,
                Entity::Component(ComponentId::StrategyC) => m.strategy,
                Entity::Component(ComponentId::ExecC3) => m.exec,
                _ => Off,
            },
            m.exec,
        )
    };
    // (flap admitted, strategy, C3, C2)
    let g = |m: Modes| {
        let g = run(m);
        (g.flap_admitted, g.strategy_on, g.c3_wanted, g.c2_on)
    };
    let blocked = Blocked("ECONOMIC_LIVE_DISABLED");
    let cases = [
        // B01: everything OFF.
        ("B01", off, (false, false, false, false)),
        // B02: runtime OFF, C2 SHADOW (PublicNode SHADOW: a source).
        (
            "B02",
            Modes { c2: Shadow, ..off },
            (false, false, false, true),
        ),
        // B03: runtime SHADOW, Flap SHADOW, Strategy SHADOW: the ledger.
        (
            "B03",
            Modes {
                runtime: Shadow,
                flap: Shadow,
                strategy: Shadow,
                ..off
            },
            (true, true, false, false),
        ),
        // B04: runtime SHADOW, Flap, EXEC, C2 SHADOW: no C3 hand-off.
        (
            "B04",
            Modes {
                runtime: Shadow,
                flap: Shadow,
                exec: Shadow,
                c2: Shadow,
                ..off
            },
            (true, false, false, true),
        ),
        // B05: runtime PAPER, Flap, EXEC, C2 SHADOW, Strategy OFF: C3.
        (
            "B05",
            Modes {
                runtime: Paper,
                flap: Shadow,
                exec: Shadow,
                c2: Shadow,
                ..off
            },
            (true, false, true, true),
        ),
        // B06: runtime PAPER, Flap, Strategy SHADOW, EXEC OFF.
        (
            "B06",
            Modes {
                runtime: Paper,
                flap: Shadow,
                strategy: Shadow,
                ..off
            },
            (true, true, false, false),
        ),
        // B07: runtime PAPER, Flap, C2, EXEC, Strategy SHADOW: both.
        (
            "B07",
            Modes {
                runtime: Paper,
                flap: Shadow,
                c2: Shadow,
                exec: Shadow,
                strategy: Shadow,
            },
            (true, true, true, true),
        ),
        // B08: as B07, Flap OFF: no Flap candidate reaches either.
        (
            "B08",
            Modes {
                runtime: Paper,
                c2: Shadow,
                exec: Shadow,
                strategy: Shadow,
                ..off
            },
            (false, false, false, true),
        ),
        // B09: as B05, C2 OFF: C3 is wanted, it refuses without a C2
        // snapshot (WAITING_C2, runtime_exec).
        (
            "B09",
            Modes {
                runtime: Paper,
                flap: Shadow,
                exec: Shadow,
                ..off
            },
            (true, false, true, false),
        ),
        // B10: as B05, EXEC OFF: no C3.
        (
            "B10",
            Modes {
                runtime: Paper,
                flap: Shadow,
                c2: Shadow,
                ..off
            },
            (true, false, false, true),
        ),
        // B14/B15: a LIVE request without a grant is BLOCKED: nothing
        // of it runs.
        (
            "B14 runtime",
            Modes {
                runtime: blocked,
                flap: Shadow,
                c2: Shadow,
                exec: Shadow,
                strategy: Shadow,
            },
            (true, false, false, true),
        ),
        (
            "B14 flap",
            Modes {
                runtime: Paper,
                flap: blocked,
                c2: Shadow,
                exec: Shadow,
                strategy: Shadow,
            },
            (false, false, false, true),
        ),
        (
            "B14 exec",
            Modes {
                runtime: Paper,
                flap: Shadow,
                c2: Shadow,
                exec: blocked,
                strategy: Shadow,
            },
            (true, true, false, true),
        ),
        (
            "B14 c2",
            Modes {
                runtime: Paper,
                flap: Shadow,
                c2: blocked,
                exec: Shadow,
                strategy: Shadow,
            },
            (true, true, true, false),
        ),
        (
            "B14 strategy",
            Modes {
                runtime: Paper,
                flap: Shadow,
                c2: Shadow,
                exec: Shadow,
                strategy: blocked,
            },
            (true, false, true, true),
        ),
        // Under a verified grant the LIVE path's modes run (plan 28.10.).
        (
            "LIVE",
            Modes {
                runtime: Live,
                flap: Live,
                c2: Live,
                exec: Off,
                strategy: Live,
            },
            (true, true, false, true),
        ),
    ];
    for (id, m, want) in cases {
        assert_eq!(g(m), want, "{id}");
    }
    // The eight C2/EXEC/STRATEGY_C combinations under PAPER and Flap
    // SHADOW (Messplan §3b B, full sub-combination).
    for (c2, exec, strategy, want) in [
        (Off, Off, Off, (true, false, false, false)),
        (Off, Off, Shadow, (true, true, false, false)),
        (Off, Shadow, Off, (true, false, true, false)),
        (Off, Shadow, Shadow, (true, true, true, false)),
        (Shadow, Off, Off, (true, false, false, true)),
        (Shadow, Off, Shadow, (true, true, false, true)),
        (Shadow, Shadow, Off, (true, false, true, true)),
        (Shadow, Shadow, Shadow, (true, true, true, true)),
    ] {
        let m = Modes {
            runtime: Paper,
            flap: Shadow,
            c2,
            exec,
            strategy,
        };
        assert_eq!(g(m), want, "{c2:?} {exec:?} {strategy:?}");
    }
}

/// SC4 review: a Strategy C position on a Classic curve exits within the
/// blocks its word's tax is witnessed for: the longest hold the
/// configuration admits is fewer blocks than that window even at a
/// conservative 250 ms per block (BSC's are slower), so a tracked SELL's
/// tax is the witnessed one without reading the holding's age.
#[test]
fn the_longest_strategy_hold_is_within_the_classic_tax_window() {
    use darkbot_bsc_trading::venues::fourmeme::profile::CLASSIC_PROFILES;
    let longest = 86_400_000u64;
    let line = CONFIG
        .lines()
        .find(|l| l.starts_with("BSC_STRATEGY_C_HOLD_MS="))
        .unwrap();
    let hold = |ms: u64| {
        RuntimeConfig::parse(&CONFIG.replace(line, &format!("BSC_STRATEGY_C_HOLD_MS={ms}")))
    };
    assert!(hold(longest).is_ok());
    assert!(hold(longest + 1).is_err());
    // Review L4: a position's token age at its SELL is the observation, the
    // fill delay and the hold, each at the most the configuration admits.
    let set = |key: &str, value: u64| {
        let line = CONFIG
            .lines()
            .find(|l| l.starts_with(&format!("{key}=")))
            .unwrap();
        RuntimeConfig::parse(&CONFIG.replace(line, &format!("{key}={value}")))
    };
    let (observation, fill_blocks) = (3_600_000u64, 100u64);
    assert!(set("BSC_STRATEGY_C_OBSERVATION_MS", observation).is_ok());
    assert!(set("BSC_STRATEGY_C_OBSERVATION_MS", observation + 1).is_err());
    assert!(set("BSC_STRATEGY_C_FILL_DELAY_BLOCKS", fill_blocks).is_ok());
    assert!(set("BSC_STRATEGY_C_FILL_DELAY_BLOCKS", fill_blocks + 1).is_err());
    let oldest_sell = (observation + longest) / 250 + fill_blocks;
    for p in CLASSIC_PROFILES {
        assert!(oldest_sell < p.tax_witnessed_blocks, "{p:?}");
    }
    // SC4 d: a Classic lot read again: its SELL window at most the
    // witnessed tax age (the shipped one the longest hold at 250 ms).
    let window = CLASSIC_PROFILES
        .iter()
        .map(|p| p.tax_witnessed_blocks)
        .min()
        .unwrap();
    let shipped = RuntimeConfig::parse(CONFIG).unwrap();
    assert_eq!(
        shipped.canonical.unwrap().lot_sell_window_blocks,
        longest / 250
    );
    assert!(set("BSC_CANONICAL_LOT_SELL_WINDOW_BLOCKS", window).is_ok());
    assert!(set("BSC_CANONICAL_LOT_SELL_WINDOW_BLOCKS", window + 1).is_err());
}

/// A6: the exit multiples are checked once, natively, before anything runs:
/// `0 <= stop < 1x < target` at the configured places (the shipped 2x / 0.5x
/// pass; 1x either way, a negative stop or a target below 1x are refused
/// with the rule named).
#[test]
fn the_exit_multiples_are_refused_outside_stop_below_one_below_target() {
    let set = |target: &str, stop: &str| {
        let t = CONFIG
            .lines()
            .find(|l| l.starts_with("BSC_STRATEGY_C_TARGET_NET_MULTIPLE="))
            .unwrap();
        let s = CONFIG
            .lines()
            .find(|l| l.starts_with("BSC_STRATEGY_C_STOP_NET_MULTIPLE="))
            .unwrap();
        RuntimeConfig::parse(
            &CONFIG
                .replace(t, &format!("BSC_STRATEGY_C_TARGET_NET_MULTIPLE={target}"))
                .replace(s, &format!("BSC_STRATEGY_C_STOP_NET_MULTIPLE={stop}")),
        )
    };
    assert!(set("2", "0.5").is_ok());
    assert!(set("1.1", "0").is_ok());
    assert!(set("1.0001", "0.9999").is_ok());
    for (target, stop, why) in [
        (
            "2",
            "1",
            "the stop net multiple must be at least 0 and below 1x",
        ),
        (
            "2",
            "1.5",
            "the stop net multiple must be at least 0 and below 1x",
        ),
        ("1", "0.5", "the target net multiple must be above 1x"),
        ("0.9", "0.5", "the target net multiple must be above 1x"),
    ] {
        let e = set(target, stop).unwrap_err();
        assert!(
            e.contains(why) && e.contains("0 <= stop < 1x < target"),
            "{target}/{stop}: {e}"
        );
    }
    // A negative stop never parses as a multiple.
    assert!(set("2", "-0.5").is_err());
}

/// SC4: Strategy C starts with the admitted venues, starts over when
/// others are admitted while it runs (off, then on, as a replay reads it),
/// stops when no longer wanted; refused gas evidence starts nothing.
#[test]
fn strategy_c_starts_over_on_other_admitted_venues() {
    use crate::runtime::{StrategySwitch as S, strategy_switch};
    use crate::runtime_strategy::StrategyVenues;
    let flap = StrategyVenues::FLAP;
    let both = StrategyVenues { four: true, ..flap };
    for (on, running, venues, gas_ok, want) in [
        (false, None, flap, true, S::Off),
        (false, Some(flap), flap, true, S::Stop),
        (true, None, flap, true, S::Start),
        (true, None, flap, false, S::Refused),
        (true, Some(flap), flap, true, S::Keep),
        (true, Some(flap), both, true, S::Restart),
        (true, Some(both), flap, true, S::Restart),
    ] {
        assert_eq!(
            strategy_switch(on, running, venues, gas_ok),
            want,
            "{on} {running:?} {venues:?} {gas_ok}"
        );
    }
}

/// R12c: Four.meme Classic SHADOW admits its first sightings to the C3
/// PAPER path, alone or beside Flap (C3 is then wanted under PAPER and
/// EXEC SHADOW); a blocked Four request admits nothing. R12d: OpenFour
/// SHADOW likewise, on its own. SC4: and their launches to Strategy C.
#[test]
fn four_shadow_admits_its_launches_to_c3_and_strategy_c() {
    use crate::canonical_join::Venue;
    use crate::runtime::FourArmed;
    use crate::sources::Entity;
    use Effective::{Blocked, Off, Paper, Shadow};
    let run = |flap, four, openfour, runtime, exec| {
        gates(
            |e| match e {
                Entity::Runtime => runtime,
                Entity::Venue(VenueId::Flap) => flap,
                Entity::Venue(VenueId::FourClassic) => four,
                Entity::Venue(VenueId::OpenFour) => openfour,
                Entity::Component(
                    ComponentId::StateC2 | ComponentId::StrategyC | ComponentId::ExecC3,
                ) => Shadow,
                _ => Off,
            },
            exec,
        )
    };
    let blocked = Blocked("FOUR_EXECUTION_PATH_MISSING");
    // (flap, four, openfour, runtime, exec) -> (C3 wanted, strategy, admits
    // Flap, admits Four, admits OpenFour); SC4: Strategy C runs with any
    // venue admitted and follows exactly the admitted ones.
    let s = |flap, four, openfour| crate::runtime_strategy::StrategyVenues {
        flap,
        four,
        openfour,
    };
    for (flap, four, openfour, runtime, exec, want, venues) in [
        (
            Off,
            Shadow,
            Off,
            Paper,
            Shadow,
            (true, true, false, true, false),
            s(false, true, false),
        ),
        (
            Shadow,
            Shadow,
            Off,
            Paper,
            Shadow,
            (true, true, true, true, false),
            s(true, true, false),
        ),
        (
            Shadow,
            Off,
            Off,
            Paper,
            Shadow,
            (true, true, true, false, false),
            s(true, false, false),
        ),
        (
            Off,
            Shadow,
            Off,
            Shadow,
            Shadow,
            (false, true, false, true, false),
            s(false, true, false),
        ),
        (
            Off,
            Shadow,
            Off,
            Paper,
            Off,
            (false, true, false, true, false),
            s(false, true, false),
        ),
        (
            Off,
            blocked,
            Off,
            Paper,
            Shadow,
            (false, false, false, false, false),
            s(false, false, false),
        ),
        (
            Off,
            Off,
            Off,
            Paper,
            Shadow,
            (false, false, false, false, false),
            s(false, false, false),
        ),
        (
            Off,
            Off,
            Shadow,
            Paper,
            Shadow,
            (true, true, false, false, true),
            s(false, false, true),
        ),
        (
            Shadow,
            Shadow,
            Shadow,
            Paper,
            Shadow,
            (true, true, true, true, true),
            s(true, true, true),
        ),
        (
            Off,
            Off,
            Shadow,
            Paper,
            Off,
            (false, true, false, false, true),
            s(false, false, true),
        ),
        (
            Off,
            Off,
            blocked,
            Paper,
            Shadow,
            (false, false, false, false, false),
            s(false, false, false),
        ),
        // The runtime mode OFF: no strategy whatever is admitted.
        (
            Shadow,
            Shadow,
            Shadow,
            Off,
            Shadow,
            (false, false, true, true, true),
            s(true, true, true),
        ),
    ] {
        let g = run(flap, four, openfour, runtime, exec);
        assert_eq!(
            (
                g.c3_wanted,
                g.strategy_on,
                g.c3_admits(Venue::Flap),
                g.c3_admits(Venue::Four),
                g.c3_admits(Venue::OpenFour)
            ),
            want,
            "{flap:?} {four:?} {openfour:?} {runtime:?} {exec:?}"
        );
        assert_eq!(g.strategy_venues, venues);
    }
    // The run loop's two decisions on a verified sighting: C2's pending
    // candidate and the C3 hand-off (a first sighting only).
    use crate::ingress::Launch;
    let sighting = |first: bool, venue| match first {
        true => Some(Launch::First {
            hash: [7; 32],
            source: SourceId::PublicNode,
            at_ns: 1,
            venue,
        }),
        false => Some(Launch::Later {
            hash: [7; 32],
            source: SourceId::PublicNode,
            lag_ns: 1,
            venue,
        }),
    };
    // A Four sighting only while the path holds that venue's BUY (`armed`);
    // Flap's never depends on it.
    let four_only = Some(run(Off, Shadow, Off, Paper, Shadow));
    let openfour_only = Some(run(Off, Off, Shadow, Paper, Shadow));
    let flap_only = Some(run(Shadow, Off, Off, Paper, Shadow));
    let (both, classic, openfour, none) = (
        FourArmed {
            classic: true,
            openfour: true,
        },
        FourArmed {
            classic: true,
            openfour: false,
        },
        FourArmed {
            classic: false,
            openfour: true,
        },
        FourArmed::default(),
    );
    for (decided, gates, armed, candidate, c3) in [
        (
            sighting(true, Venue::Four),
            four_only,
            classic,
            true,
            Some([7; 32]),
        ),
        (
            sighting(true, Venue::Four),
            four_only,
            openfour,
            false,
            None,
        ),
        (sighting(true, Venue::Four), four_only, none, false, None),
        (sighting(true, Venue::Four), flap_only, both, false, None),
        (sighting(true, Venue::Four), None, both, false, None),
        (sighting(false, Venue::Four), four_only, both, true, None),
        (sighting(true, Venue::Flap), four_only, both, true, None),
        (
            sighting(true, Venue::Flap),
            flap_only,
            none,
            true,
            Some([7; 32]),
        ),
        (sighting(true, Venue::Flap), None, none, true, None),
        (
            sighting(true, Venue::OpenFour),
            openfour_only,
            openfour,
            true,
            Some([7; 32]),
        ),
        (
            sighting(true, Venue::OpenFour),
            openfour_only,
            classic,
            false,
            None,
        ),
        (
            sighting(true, Venue::OpenFour),
            four_only,
            both,
            false,
            None,
        ),
        (
            sighting(false, Venue::OpenFour),
            openfour_only,
            openfour,
            true,
            None,
        ),
        (None, four_only, both, false, None),
    ] {
        assert_eq!(
            (
                crate::runtime::c2_candidate(decided, gates, armed),
                crate::runtime::c3_launch(decided, gates, armed)
            ),
            (candidate, c3),
            "{decided:?} {armed:?}"
        );
    }
}

/// BSC3-079: the configured gas limits cover the frozen evidence the
/// configuration names; one below a class's largest gas (a BUY in its
/// token's launch block included) refuses the start.
#[test]
fn the_gas_limits_cover_the_frozen_evidence() {
    use darkbot_bsc_trading::economics::network::{Action, GasEvidence, PathClass, Venue};
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let gas = config.strategy.gas(&repo).unwrap();
    let exec = config.exec.as_ref().unwrap().exec.clone();
    gas_limits_cover(&exec, &gas).unwrap();
    for (action, limit) in [
        (Action::Buy, exec.gas_limit),
        (Action::BuyInLaunchBlock, exec.gas_limit),
        (Action::Sell, exec.sell_gas_limit),
        (Action::Approve, exec.approve_gas_limit),
    ] {
        let mut over = GasEvidence::default();
        over.add(
            PathClass {
                venue: Venue::Flap,
                action,
            },
            limit + 1,
        )
        .unwrap();
        let e = gas_limits_cover(&exec, &over).unwrap_err();
        assert!(e.contains(&format!("{action:?} gas {}", limit + 1)), "{e}");
    }
    // R12c: the Four Classic BUY's limit covers the frozen Classic BUYs,
    // and is held to them.
    let four = exec.four.unwrap();
    let classic = |action| PathClass {
        venue: Venue::FourClassic,
        action,
    };
    assert!(gas.class(classic(Action::Buy)).unwrap().gas_used_max <= four.gas_limit);
    for action in [Action::Buy, Action::BuyInLaunchBlock] {
        let mut over = GasEvidence::default();
        over.add(classic(action), four.gas_limit + 1).unwrap();
        let e = gas_limits_cover(&exec, &over).unwrap_err();
        assert!(
            e.contains(&format!(
                "BSC_EXEC_FOUR_GAS_LIMIT {} is below the largest {action:?} gas {}",
                four.gas_limit,
                four.gas_limit + 1
            )),
            "{e}"
        );
    }
    // R12d: the OpenFour BUY's limit covers the frozen OpenFour BUYs
    // (`buyByBudget`) and every executed estimate of its exact `Core.buy`
    // (the parity fixture's OFGAS rows), and is held to them.
    let openfour = exec.openfour.unwrap();
    let open = |action| PathClass {
        venue: Venue::OpenFour,
        action,
    };
    assert!(gas.class(open(Action::Buy)).unwrap().gas_used_max <= openfour.gas_limit);
    let estimates: Vec<u64> =
        include_str!("../../trading/fixtures/fourmeme/openfour_parity_20261104.txt")
            .lines()
            .filter(|l| l.starts_with("OFGAS "))
            .map(|l| l.split(' ').nth(6).unwrap().parse().unwrap())
            .collect();
    // R12d's 0.001/0.1 BNB and SC4's 0.01 BNB admissions.
    assert_eq!(estimates.len(), 20);
    assert!(estimates.iter().all(|g| *g <= openfour.gas_limit));
    for action in [Action::Buy, Action::BuyInLaunchBlock] {
        let mut over = GasEvidence::default();
        over.add(open(action), openfour.gas_limit + 1).unwrap();
        let e = gas_limits_cover(&exec, &over).unwrap_err();
        assert!(
            e.contains(&format!(
                "BSC_EXEC_OPENFOUR_GAS_LIMIT {} is below the largest {action:?} gas {}",
                openfour.gas_limit,
                openfour.gas_limit + 1
            )),
            "{e}"
        );
    }
    // SC4 c (B1/B3): each family's exit approval and SELL limits cover its
    // frozen APPROVE and SELL receipts (Classic 49313/193306, OpenFour
    // 48788/570922), and are held to them, each by its own key.
    for (venue, f, keys) in [
        (
            Venue::FourClassic,
            four,
            [
                "BSC_EXEC_FOUR_APPROVE_GAS_LIMIT",
                "BSC_EXEC_FOUR_SELL_GAS_LIMIT",
            ],
        ),
        (
            Venue::OpenFour,
            openfour,
            [
                "BSC_EXEC_OPENFOUR_APPROVE_GAS_LIMIT",
                "BSC_EXEC_OPENFOUR_SELL_GAS_LIMIT",
            ],
        ),
    ] {
        for (action, limit, key) in [
            (Action::Approve, f.approve_gas_limit, keys[0]),
            (Action::Sell, f.sell_gas_limit, keys[1]),
        ] {
            let class = PathClass { venue, action };
            assert!(gas.class(class).unwrap().gas_used_max <= limit, "{class:?}");
            let mut over = GasEvidence::default();
            over.add(class, limit + 1).unwrap();
            let e = gas_limits_cover(&exec, &over).unwrap_err();
            assert!(
                e.contains(&format!(
                    "{key} {limit} is below the largest {action:?} gas {}",
                    limit + 1
                )),
                "{e}"
            );
        }
    }
    // Without the Four BUYs their evidence binds nothing.
    let mut flap_only = exec.clone();
    flap_only.four = None;
    flap_only.openfour = None;
    let mut over = GasEvidence::default();
    over.add(classic(Action::Buy), u64::MAX).unwrap();
    over.add(open(Action::Buy), u64::MAX).unwrap();
    gas_limits_cover(&flap_only, &over).unwrap();
}

/// BSC3-079 review: the start's check reads the build's frozen evidence,
/// not the configured files: a configuration whose Strategy C evidence
/// does not load still has its executor's gas limits checked.
#[test]
fn the_start_checks_the_gas_limits_whatever_the_strategy_evidence() {
    let mut config = RuntimeConfig::parse(CONFIG).unwrap();
    exec_gas_covered(&config).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let configured = config.strategy.gas(&repo).unwrap();
    assert_eq!(configured, StrategyConfig::frozen_gas().unwrap());
    config.strategy.gas_evidence.clear();
    assert!(config.strategy.gas(&repo).is_err());
    exec_gas_covered(&config).unwrap();
    config.exec.as_mut().unwrap().exec.sell_gas_limit = 700_000;
    let e = exec_gas_covered(&config).unwrap_err();
    assert!(e.starts_with("BSC_EXEC_SELL_GAS_LIMIT 700000"), "{e}");
    config.exec = None;
    exec_gas_covered(&config).unwrap();
}

/// R5: a DEX quote prices an exit at its own block and the next, no
/// later one.
#[test]
fn a_dex_quote_is_fresh_for_its_block_and_the_next() {
    assert!(super::quote_fresh(10, 10) && super::quote_fresh(10, 11));
    assert!(!super::quote_fresh(10, 12) && !super::quote_fresh(11, 10));
    assert!(super::quote_fresh(u64::MAX, u64::MAX));
}

#[test]
fn the_config_is_strict() {
    let c = RuntimeConfig::parse(CONFIG).unwrap();
    assert_eq!(
        c.endpoint(SourceId::PublicNode),
        Some("wss://bsc-rpc.publicnode.com")
    );
    assert_eq!(c.endpoint(SourceId::Drpc), Some("wss://bsc.drpc.org"));
    assert_eq!(
        c.endpoint(SourceId::BlockReq),
        None,
        "no endpoint: UNQUALIFIED"
    );
    // R6: no geth arena in the repository (UNQUALIFIED); one configured
    // is the geth source's endpoint, with all of its keys.
    assert_eq!((c.endpoint(SourceId::Geth), c.geth.as_ref()), (None, None));
    let g = RuntimeConfig::parse(&format!(
            "{CONFIG}BSC_INGRESS_GETH_ARENA=/var/geth\nBSC_INGRESS_GETH_GENERATION=7\nBSC_INGRESS_GETH_PEER_LANES=8\nBSC_INGRESS_GETH_IDLE_US=50\nBSC_INGRESS_GETH_DEDUP_SETS=16\n"
        ))
        .unwrap();
    assert_eq!(g.endpoint(SourceId::Geth), Some("/var/geth"));
    assert_eq!(
        g.geth,
        Some(crate::geth_source::GethArena {
            dir: "/var/geth".into(),
            generation: 7,
            peer_lanes: 8,
            idle: Duration::from_micros(50),
            hot0: (1, 16),
            alive: Duration::from_millis(5_000),
            retry: Duration::from_millis(1_000),
        })
    );
    assert_eq!((c.reconcile_ms, c.stop_timeout_ms), (500, 15_000));
    // 500 + max(5000 + 5000 + 200, 5000 + 200)
    assert_eq!(c.stop_bound_ms(), 10_700);
    let canonical = c.canonical.as_ref().unwrap();
    assert_eq!(canonical.endpoint, "https://bsc-dataseed.bnbchain.org");
    // BSC3-036: the fallbacks in order, then the primary for any other index.
    assert_eq!(
        canonical.endpoints(),
        [
            "https://bsc-dataseed.bnbchain.org",
            "https://bsc-dataseed1.defibit.io",
            "https://bsc-dataseed1.ninicoin.io"
        ]
    );
    assert_eq!(
        canonical.endpoint_at(2),
        "https://bsc-dataseed1.ninicoin.io"
    );
    assert_eq!(
        canonical.endpoint_at(9),
        "https://bsc-dataseed.bnbchain.org"
    );
    assert_eq!(
        (
            canonical.limits.failover_after,
            canonical.limits.stall_after
        ),
        (3, Duration::from_millis(5_000))
    );
    let fallback = |list: &str| {
        RuntimeConfig::parse(&CONFIG.replace(
                "BSC_CANONICAL_RPC_FALLBACK=https://bsc-dataseed1.defibit.io,https://bsc-dataseed1.ninicoin.io\n",
                &format!("BSC_CANONICAL_RPC_FALLBACK={list}\n"),
            ))
            .map(|c| c.canonical.unwrap().fallbacks)
    };
    assert_eq!(fallback(""), Ok(Vec::new()));
    // R3b: the recovery's bounds; its search over a lease's window.
    assert_eq!(
        (
            canonical.recovery_reads_max,
            canonical.recovery_retry_ms,
            canonical.recovery_search_blocks
        ),
        (2, 60_000, 28_800)
    );
    let search = |blocks: &str| {
        RuntimeConfig::parse(&CONFIG.replace(
            "BSC_CANONICAL_RECOVERY_SEARCH_BLOCKS=28800\n",
            &format!("BSC_CANONICAL_RECOVERY_SEARCH_BLOCKS={blocks}\n"),
        ))
        .map(|c| c.canonical.unwrap().recovery_search_blocks)
    };
    assert_eq!(search("200"), Ok(200), "the lease's window exactly");
    assert!(
        search("199")
            .unwrap_err()
            .contains("BSC_EXEC_LIVE_LEASE_BLOCKS")
    );
    for bad in ["0", "10000001", "x"] {
        assert!(search(bad).is_err(), "{bad}");
    }
    // BSC3-056: the second source, of a host none of the followed has.
    assert_eq!(
        canonical.confirm,
        Some(ConfirmConfig {
            endpoint: "https://bsc-rpc.publicnode.com".into(),
            max_age_ms: 3_000,
            max_lag_blocks: 2,
        })
    );
    let confirm = |rpc: &str| {
        RuntimeConfig::parse(&CONFIG.replace(
            "BSC_CANONICAL_CONFIRM_RPC=https://bsc-rpc.publicnode.com\n",
            &format!("BSC_CANONICAL_CONFIRM_RPC={rpc}\n"),
        ))
        .map(|c| c.canonical.unwrap().confirm)
    };
    assert_eq!(confirm(""), Ok(None));
    for bad in [
        "https://bsc-dataseed1.ninicoin.io",
        "https://bsc-dataseed.bnbchain.org/other",
        "http://bsc-rpc.publicnode.com",
        // The same host in other spellings (review LOW-3).
        "https://BSC-DATASEED1.NINICOIN.IO",
        "https://bsc-dataseed1.defibit.io:443/",
    ] {
        assert!(confirm(bad).is_err(), "{bad}");
    }
    for bad in [
        "http://bsc-dataseed1.defibit.io",
        "https://bsc-dataseed.bnbchain.org",
        "https://a.example,https://a.example",
        "https://a.example,https://A.EXAMPLE:443/",
    ] {
        assert!(fallback(bad).is_err(), "{bad}");
    }
    // An error names no path (it may hold a provider's key).
    let e = fallback("http://a.example/secret-key").unwrap_err();
    assert!(!e.contains("secret-key"), "{e}");
    assert_eq!(canonical.limits.reorg_depth, 64);
    // The C2 window is the follower's; no own account in READ_ONLY.
    assert_eq!(canonical.state.limits.reorg_window, 64);
    assert_eq!(canonical.state.limits.finality_depth, 16);
    assert!(canonical.state.watched.is_empty());
    let one = CONFIG.replace(
            "BSC_STATE_WATCHED=\n",
            "BSC_STATE_WATCHED=0x00000000000000000000000000000000000000aB, 0x0000000000000000000000000000000000000001\n",
        );
    let watched = RuntimeConfig::parse(&one)
        .unwrap()
        .canonical
        .unwrap()
        .state
        .watched;
    assert_eq!(
        (watched.len(), watched[0][19], watched[1][19]),
        (2, 0xab, 1)
    );
    // Without the endpoint the canonical and C2 bounds are unknown keys.
    assert!(RuntimeConfig::parse(&CONFIG.replace("BSC_CANONICAL_RPC=", "#")).is_err());
    let without: String = CONFIG
        .lines()
        .filter(|l| {
            !l.starts_with("BSC_CANONICAL_")
                && !l.starts_with("BSC_STATE_")
                && !l.starts_with("BSC_EXEC_")
        })
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(RuntimeConfig::parse(&without).unwrap().canonical, None);
    // R9 review: the presence anchor never without the path.
    assert!(
        RuntimeConfig::parse(&format!(
            "{without}BSC_EXEC_LIVE_PRESENCE_ANCHOR=0x{}\n",
            "ab".repeat(20)
        ))
        .unwrap_err()
        .contains("BSC_EXEC_LIVE_PRESENCE_ANCHOR needs BSC_EXEC_PAPER_IDENTITY")
    );
    // R7 review L2: the divergence quarantine never without the path.
    assert!(
        RuntimeConfig::parse(&format!(
            "{without}BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE=ON\n"
        ))
        .unwrap_err()
        .contains("BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE needs BSC_EXEC_PAPER_IDENTITY")
    );
    // The PAPER path needs C2: without the follower it is refused.
    assert!(
        RuntimeConfig::parse(&format!(
            "{without}BSC_EXEC_PAPER_IDENTITY=/tmp/x\nBSC_EXEC_REFUSED_ACCOUNTS=\n"
        ))
        .is_err()
    );
    // R12c: the Four Classic BUY never without the path.
    assert!(
        RuntimeConfig::parse(&format!("{without}BSC_EXEC_FOUR_INPUT_WEI=1\n"))
            .unwrap_err()
            .contains("BSC_EXEC_FOUR_* needs BSC_EXEC_PAPER_IDENTITY")
    );
    // R12c: the shipped Four Classic BUY; its three keys go together, each
    // within its bounds, its input a witnessed one, under the session cap.
    let four = c.exec.as_ref().unwrap().exec.four.unwrap();
    assert_eq!(
        (
            four.input_wei,
            four.slippage_bps,
            four.gas_limit,
            four.approve_gas_limit,
            four.sell_gas_limit
        ),
        (1_000_000_000_000_000, 100, 1_000_000, 100_000, 350_000)
    );
    let unset: String = CONFIG
        .lines()
        .filter(|l| !l.starts_with("BSC_EXEC_FOUR_"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        RuntimeConfig::parse(&unset)
            .unwrap()
            .exec
            .unwrap()
            .exec
            .four,
        None
    );
    for (from, to, why) in [
        (
            "BSC_EXEC_FOUR_GAS_LIMIT=1000000\n",
            "",
            "BSC_EXEC_FOUR_INPUT_WEI, BSC_EXEC_FOUR_SLIPPAGE_BPS, BSC_EXEC_FOUR_GAS_LIMIT, \
             BSC_EXEC_FOUR_APPROVE_GAS_LIMIT and BSC_EXEC_FOUR_SELL_GAS_LIMIT go together",
        ),
        (
            "BSC_EXEC_FOUR_SELL_GAS_LIMIT=350000\n",
            "",
            "BSC_EXEC_FOUR_SELL_GAS_LIMIT go together",
        ),
        (
            "BSC_EXEC_FOUR_APPROVE_GAS_LIMIT=100000\n",
            "BSC_EXEC_FOUR_APPROVE_GAS_LIMIT=20999\n",
            "BSC_EXEC_FOUR_APPROVE_GAS_LIMIT=20999 is outside",
        ),
        (
            "BSC_EXEC_FOUR_SLIPPAGE_BPS=100\n",
            "BSC_EXEC_FOUR_SLIPPAGE_BPS=10000\n",
            "BSC_EXEC_FOUR_SLIPPAGE_BPS",
        ),
        (
            "BSC_EXEC_FOUR_GAS_LIMIT=1000000\n",
            "BSC_EXEC_FOUR_GAS_LIMIT=20999\n",
            "BSC_EXEC_FOUR_GAS_LIMIT",
        ),
        (
            "BSC_EXEC_FOUR_INPUT_WEI=1000000000000000\n",
            "BSC_EXEC_FOUR_INPUT_WEI=2000000000000000\n",
            "is none of the witnessed inputs",
        ),
        (
            "BSC_EXEC_SESSION_CAP_WEI=1150000000000000\n",
            "BSC_EXEC_SESSION_CAP_WEI=1049999999999999\n",
            "is below one Four BUY (1050000000000000)",
        ),
    ] {
        let e = RuntimeConfig::parse(&CONFIG.replace(from, to)).unwrap_err();
        assert!(e.contains(why), "{to}: {e}");
    }
    // R12d: the shipped OpenFour BUY likewise: never without the path, its
    // three keys together, each within its bounds, its input a witnessed
    // one, under the session cap.
    assert!(
        RuntimeConfig::parse(&format!("{without}BSC_EXEC_OPENFOUR_INPUT_WEI=1\n"))
            .unwrap_err()
            .contains("BSC_EXEC_OPENFOUR_* needs BSC_EXEC_PAPER_IDENTITY")
    );
    let openfour = c.exec.as_ref().unwrap().exec.openfour.unwrap();
    assert_eq!(
        (
            openfour.input_wei,
            openfour.slippage_bps,
            openfour.gas_limit,
            openfour.approve_gas_limit,
            openfour.sell_gas_limit
        ),
        (1_000_000_000_000_000, 100, 2_000_000, 100_000, 750_000)
    );
    let unset: String = CONFIG
        .lines()
        .filter(|l| !l.starts_with("BSC_EXEC_OPENFOUR_"))
        .map(|l| format!("{l}\n"))
        .collect();
    let parsed = RuntimeConfig::parse(&unset).unwrap().exec.unwrap().exec;
    assert_eq!((parsed.openfour, parsed.four.is_some()), (None, true));
    for (from, to, why) in [
        (
            "BSC_EXEC_OPENFOUR_GAS_LIMIT=2000000\n",
            "",
            "BSC_EXEC_OPENFOUR_INPUT_WEI, BSC_EXEC_OPENFOUR_SLIPPAGE_BPS, BSC_EXEC_OPENFOUR_GAS_LIMIT, \
             BSC_EXEC_OPENFOUR_APPROVE_GAS_LIMIT and BSC_EXEC_OPENFOUR_SELL_GAS_LIMIT go together",
        ),
        (
            "BSC_EXEC_OPENFOUR_SELL_GAS_LIMIT=750000\n",
            "BSC_EXEC_OPENFOUR_SELL_GAS_LIMIT=100000001\n",
            "BSC_EXEC_OPENFOUR_SELL_GAS_LIMIT=100000001 is outside",
        ),
        (
            "BSC_EXEC_OPENFOUR_SLIPPAGE_BPS=100\n",
            "BSC_EXEC_OPENFOUR_SLIPPAGE_BPS=10000\n",
            "BSC_EXEC_OPENFOUR_SLIPPAGE_BPS",
        ),
        (
            "BSC_EXEC_OPENFOUR_GAS_LIMIT=2000000\n",
            "BSC_EXEC_OPENFOUR_GAS_LIMIT=20999\n",
            "BSC_EXEC_OPENFOUR_GAS_LIMIT",
        ),
        (
            "BSC_EXEC_OPENFOUR_INPUT_WEI=1000000000000000\n",
            "BSC_EXEC_OPENFOUR_INPUT_WEI=2000000000000000\n",
            "BSC_EXEC_OPENFOUR_INPUT_WEI 2000000000000000 is none of the witnessed inputs",
        ),
        (
            "BSC_EXEC_SESSION_CAP_WEI=1150000000000000\n",
            "BSC_EXEC_SESSION_CAP_WEI=1099999999999999\n",
            "is below one OpenFour BUY (1100000000000000)",
        ),
    ] {
        let e = RuntimeConfig::parse(&CONFIG.replace(from, to)).unwrap_err();
        assert!(e.contains(why), "{to}: {e}");
    }
    let exec = c.exec.as_ref().unwrap();
    assert!(
        exec.identity_path
            .ends_with(".darkbot/bsc/paper_identity.key")
    );
    // Paket 8 review L4: the frozen G0.7 wallets are refused as PAPER
    // identity.
    assert_eq!(exec.refused_accounts.len(), 2);
    assert!(exec.identity_path.is_absolute());
    assert_eq!(
        (
            exec.exec.input_wei,
            exec.exec.gas_price_wei,
            exec.exec.gas_limit,
            exec.exec.slippage_bps
        ),
        (1_000_000_000_000_000, 50_000_000, 3_000_000, 100)
    );
    // BSC3-069: an exit's own slippage; the BUY's when not set; the
    // same bounds; only with the PAPER path.
    assert_eq!(exec.exec.exit_slippage_bps, exec.exec.slippage_bps);
    let exit = |v: &str| RuntimeConfig::parse(&format!("{CONFIG}BSC_EXEC_EXIT_SLIPPAGE_BPS={v}\n"));
    assert_eq!(
        exit("300").unwrap().exec.unwrap().exec.exit_slippage_bps,
        300
    );
    assert!(exit("10000").is_err());
    let no_exec: String = CONFIG
        .lines()
        .filter(|l| !l.starts_with("BSC_EXEC_"))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(RuntimeConfig::parse(&no_exec).unwrap().exec, None);
    assert!(RuntimeConfig::parse(&format!("{no_exec}BSC_EXEC_EXIT_SLIPPAGE_BPS=300\n")).is_err());
    assert!(RuntimeConfig::parse(&format!("{without}BSC_STATE_WATCHED=\n")).is_err());
    for bad in [
            CONFIG.replace("wss://bsc-rpc", "ws://bsc-rpc"),
            CONFIG.replace("wss://bsc-rpc", "wss://user:pw@bsc-rpc"),
            CONFIG.replace("QUEUE_EVENTS=65536", "QUEUE_EVENTS=1"),
            CONFIG.replace("BSC_RUNTIME_STATUS_MS=1000\n", ""),
            format!("{CONFIG}BSC_RUNTIME_SURPRISE=1\n"),
            format!("{CONFIG}BSC_RUNTIME_QUIET_MS=5000\n"),
            CONFIG
                .replace("BACKOFF_INITIAL_MS=1000", "BACKOFF_INITIAL_MS=60000")
                .replace("BACKOFF_MAX_MS=60000", "BACKOFF_MAX_MS=1000"),
            CONFIG.replace("MAX_FRAME_BYTES=1048576", "MAX_FRAME_BYTES=2097152"),
            CONFIG.replace("https://bsc-dataseed", "http://bsc-dataseed"),
            CONFIG.replace("BSC_CANONICAL_JOIN_DELAY_MS=2000\n", ""),
            CONFIG.replace("BACKOFF_MAX_MS=30000", "BACKOFF_MAX_MS=100"),
            CONFIG.replace("BSC_RUNTIME_CENSUS_MS=60000\n", ""),
            // What the adapters would refuse is refused here (review LOW-5).
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc.drpc.org?key=1"),
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc.drpc.org:99999"),
            CONFIG.replace("wss://bsc.drpc.org", "wss://bsc_rpc.example"),
            CONFIG.replace(
                "https://bsc-dataseed.bnbchain.org",
                "https://bsc-dataseed.bnbchain.org?k=1",
            ),
            // A stop wait below the derived stop path.
            CONFIG.replace("STOP_TIMEOUT_MS=15000", "STOP_TIMEOUT_MS=1000"),
            CONFIG.replace("BSC_RUNTIME_RECONCILE_MS=500\n", ""),
            // C2: final only inside the reorg window; a slot no larger than
            // the table; every key required; watched accounts exact, once.
            CONFIG.replace("BSC_STATE_FINALITY_DEPTH=16", "BSC_STATE_FINALITY_DEPTH=65"),
            CONFIG.replace("BSC_STATE_MAX_TXS=262144", "BSC_STATE_MAX_TXS=1024")
                .replace("CANDIDATES_PER_NONCE=16", "CANDIDATES_PER_NONCE=1024")
                .replace("BSC_STATE_MAX_TXS=1024", "BSC_STATE_MAX_TXS=1023"),
            CONFIG.replace("BSC_STATE_CHECKPOINT_MS=30000\n", ""),
            CONFIG.replace("NONCE_FACT_BLOCKS=20", "NONCE_FACT_BLOCKS=0"),
            // C3: every PAPER key required, bounded; refused accounts exact.
            CONFIG.replace("BSC_EXEC_LANES=64\n", ""),
            CONFIG.replace("BSC_RUNTIME_MAX_LAUNCH_CALLDATA=16384\n", ""),
            CONFIG.replace("MAX_LAUNCH_CALLDATA=16384", "MAX_LAUNCH_CALLDATA=1023"),
            CONFIG.replace(REFUSED, ""),
            CONFIG.replace(REFUSED, "BSC_EXEC_REFUSED_ACCOUNTS=0x12\n"),
            // Audit E 3.12: the frozen G0.7 launcher and buyer always among them.
            CONFIG.replace(
                REFUSED,
                "BSC_EXEC_REFUSED_ACCOUNTS=0x9c8a0Db41914beEE52Dd49790c4F087E252743DE\n",
            ),
            CONFIG.replace(
                REFUSED,
                "BSC_EXEC_REFUSED_ACCOUNTS=0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd,0x1111111111111111111111111111111111111111\n",
            ),
            // Audit E 3.1–3.4: the owner's bounds, the stale factor and the
            // parity floor, each required and in its range.
            CONFIG.replace("BSC_BUY_ATTEMPTS_TOTAL_MAX=10\n", ""),
            CONFIG.replace("BSC_BUY_ATTEMPTS_TOTAL_MAX=10", "BSC_BUY_ATTEMPTS_TOTAL_MAX=11"),
            CONFIG.replace("BSC_OWNER_KEEP_GENERATIONS=64\n", ""),
            CONFIG.replace("BSC_OWNER_KEEP_GENERATIONS=64", "BSC_OWNER_KEEP_GENERATIONS=0"),
            CONFIG.replace("BSC_RUNTIME_STATUS_STALE_INTERVALS=3\n", ""),
            CONFIG.replace(
                "BSC_RUNTIME_STATUS_STALE_INTERVALS=3",
                "BSC_RUNTIME_STATUS_STALE_INTERVALS=1",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_PARITY_MIN_EXECUTED_PER_PATH=30\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_PARITY_MIN_EXECUTED_PER_PATH=30",
                "BSC_EXEC_LIVE_PARITY_MIN_EXECUTED_PER_PATH=29",
            ),
            // Gate M2: the paths that take the sender, required and exact.
            CONFIG.replace("BSC_EXEC_SENDER_PATHS=C3+STRATEGY_C\n", ""),
            CONFIG.replace("BSC_EXEC_SENDER_PATHS=C3+STRATEGY_C", "BSC_EXEC_SENDER_PATHS=C3+C2"),
            CONFIG.replace("BSC_EXEC_SLIPPAGE_BPS=100", "BSC_EXEC_SLIPPAGE_BPS=10000"),
            CONFIG.replace("BSC_EXEC_GAS_LIMIT=3000000", "BSC_EXEC_GAS_LIMIT=20999"),
            // R3: the operator exit switch, required and exact.
            CONFIG.replace("BSC_EXEC_LIVE_OPERATOR_EXIT=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_OPERATOR_EXIT=OFF", "BSC_EXEC_LIVE_OPERATOR_EXIT=on"),
            // R3b: the nonce burn switch, required and exact.
            CONFIG.replace("BSC_EXEC_LIVE_NONCE_BURN=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_NONCE_BURN=OFF", "BSC_EXEC_LIVE_NONCE_BURN=1"),
            // R4: the public-only switch and the grant's longest, required and exact.
            CONFIG.replace("BSC_EXEC_LIVE_PUBLIC_ONLY=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_PUBLIC_ONLY=OFF", "BSC_EXEC_LIVE_PUBLIC_ONLY=yes"),
            CONFIG.replace("BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=0\n", ""),
            // R8: the qualification's age bound, required, at most a year.
            CONFIG.replace("BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS=0\n", ""),
            // R9: the presence anchor, required (empty: OFF), an address.
            CONFIG.replace("BSC_EXEC_LIVE_PRESENCE_ANCHOR=\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_PRESENCE_ANCHOR=\n",
                "BSC_EXEC_LIVE_PRESENCE_ANCHOR=0xab\n",
            ),
            CONFIG.replace(
                "BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS=0",
                "BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS=31536000001",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_SPEND_TOTAL_WEI=0\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_OUTLIER=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_OUTLIER=OFF", "BSC_EXEC_LIVE_RISK_OUTLIER=On"),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS=100\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS=100",
                "BSC_EXEC_LIVE_RISK_SOFT_BUY_SLIPPAGE_BPS=10000",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS=100\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS=100",
                "BSC_EXEC_LIVE_RISK_SOFT_EXIT_SLIPPAGE_BPS=-1",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI=50000000\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI=50000000",
                "BSC_EXEC_LIVE_RISK_SOFT_GAS_PRICE_WEI=0",
            ),
            CONFIG.replace(
                "BSC_EXEC_LIVE_SPEND_TOTAL_WEI=0",
                "BSC_EXEC_LIVE_SPEND_TOTAL_WEI=18446744073709551616",
            ),
            // R4: the C3 re-arm switch and the bound builder policy,
            // required and exact (a repository-relative path).
            CONFIG.replace("BSC_EXEC_LIVE_C3_REARM=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_EXIT_REPRICE_BPS=0\n", ""),
            // R6: the local geth's arena: an absolute path with all of its
            // keys, its dedup sets a power of two; its keys never alone.
            format!("{CONFIG}BSC_INGRESS_GETH_ARENA=relative/arena\n"),
            format!("{CONFIG}BSC_INGRESS_GETH_ARENA=/var/geth\n"),
            format!("{CONFIG}BSC_INGRESS_GETH_GENERATION=7\n"),
            format!(
                "{CONFIG}BSC_INGRESS_GETH_ARENA=/var/geth\nBSC_INGRESS_GETH_GENERATION=7\nBSC_INGRESS_GETH_PEER_LANES=8\nBSC_INGRESS_GETH_IDLE_US=50\nBSC_INGRESS_GETH_DEDUP_SETS=12\n"
            ),
            format!(
                "{CONFIG}BSC_INGRESS_GETH_ARENA=/var/geth\nBSC_INGRESS_GETH_GENERATION=7\nBSC_INGRESS_GETH_PEER_LANES=257\nBSC_INGRESS_GETH_IDLE_US=50\nBSC_INGRESS_GETH_DEDUP_SETS=16\n"
            ),
            // R5: the public exit route, required and exact; ON only with
            // an https:// endpoint.
            CONFIG.replace("BSC_EXEC_LIVE_PUBLIC_EXIT=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_DEX_EXIT=OFF\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_DEX_EXIT=OFF", "BSC_EXEC_LIVE_DEX_EXIT=yes"),
            // R7: the divergence quarantine: ON or OFF, required.
            CONFIG.replace("BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE=OFF\n", ""),
            CONFIG.replace(
                "BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE=OFF",
                "BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE=on",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_PUBLIC_EXIT_RPC=\n", ""),
            CONFIG.replace("BSC_EXEC_LIVE_PUBLIC_EXIT=OFF", "BSC_EXEC_LIVE_PUBLIC_EXIT=ON"),
            CONFIG.replace(
                "BSC_EXEC_LIVE_PUBLIC_EXIT_RPC=\n",
                "BSC_EXEC_LIVE_PUBLIC_EXIT_RPC=http://rpc.example.test/\n",
            ),
            CONFIG.replace(
                "BSC_EXEC_LIVE_EXIT_REPRICE_BPS=0",
                "BSC_EXEC_LIVE_EXIT_REPRICE_BPS=10000",
            ),
            CONFIG.replace("BSC_EXEC_LIVE_C3_REARM=OFF", "BSC_EXEC_LIVE_C3_REARM=off"),
            CONFIG.replace(&format!("BSC_EXEC_LIVE_BUILDER_POLICY={POLICY}\n"), ""),
            CONFIG.replace(POLICY, ""),
            CONFIG.replace(POLICY, "/tmp/builder_policy.dbrec"),
            CONFIG.replace(POLICY, "bsc/../builder_policy.dbrec"),
            CONFIG.replace(POLICY, "./builder_policy.dbrec"),
            CONFIG.replace(
                "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=0",
                "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=31536000001",
            ),
            CONFIG.replace("BSC_STATE_WATCHED=\n", ""),
            CONFIG.replace("BSC_STATE_WATCHED=\n", "BSC_STATE_WATCHED=0x12\n"),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=00000000000000000000000000000000000000000001\n",
            ),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x000000000000000000000000000000000000000g\n",
            ),
            CONFIG.replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000001\n",
            ),
            CONFIG.replace("MAX_WATCHED_SENDERS=8", "MAX_WATCHED_SENDERS=1").replace(
                "BSC_STATE_WATCHED=\n",
                "BSC_STATE_WATCHED=0x0000000000000000000000000000000000000001,0x0000000000000000000000000000000000000002\n",
            ),
        ] {
            assert!(RuntimeConfig::parse(&bad).is_err(), "{bad}");
        }
}

/// REV.1.0 R3: the operator's exit-now is held back unless the operator
/// switched `BSC_EXEC_LIVE_OPERATOR_EXIT` ON (the repository's OFF).
#[test]
fn the_operator_exit_is_off_unless_the_operator_sets_it_on() {
    let exit = |text: &str| {
        RuntimeConfig::parse(text)
            .unwrap()
            .exec
            .unwrap()
            .exec
            .live_operator_exit
    };
    assert!(!exit(CONFIG));
    assert!(exit(&CONFIG.replace(
        "BSC_EXEC_LIVE_OPERATOR_EXIT=OFF",
        "BSC_EXEC_LIVE_OPERATOR_EXIT=ON"
    )));
    // R3b: the nonce burn likewise.
    let burn = |text: &str| {
        RuntimeConfig::parse(text)
            .unwrap()
            .exec
            .unwrap()
            .exec
            .live_nonce_burn
    };
    assert!(!burn(CONFIG));
    assert!(burn(&CONFIG.replace(
        "BSC_EXEC_LIVE_NONCE_BURN=OFF",
        "BSC_EXEC_LIVE_NONCE_BURN=ON"
    )));
    // R4: a public source alone triggers no C3 BUY unless ON; no
    // grant's duration is bounded unless the operator names one.
    let exec = |text: &str| RuntimeConfig::parse(text).unwrap().exec.unwrap().exec;
    assert!(!exec(CONFIG).live_public_only);
    assert!(
        exec(&CONFIG.replace(
            "BSC_EXEC_LIVE_PUBLIC_ONLY=OFF",
            "BSC_EXEC_LIVE_PUBLIC_ONLY=ON"
        ))
        .live_public_only
    );
    assert_eq!(exec(CONFIG).live_grant_max_duration_ms, 0);
    assert_eq!(exec(CONFIG).live_qualification_max_age_ms, 0);
    // R9: no presence anchor (OFF); an address turns it on.
    assert_eq!(exec(CONFIG).live_presence_anchor, None);
    assert_eq!(
        exec(&CONFIG.replace(
            "BSC_EXEC_LIVE_PRESENCE_ANCHOR=\n",
            &format!("BSC_EXEC_LIVE_PRESENCE_ANCHOR=0x{}\n", "ab".repeat(20)),
        ))
        .live_presence_anchor,
        Some([0xab; 20])
    );
    // The campaign's budget: none in the repository (no BUY).
    assert_eq!(exec(CONFIG).live_spend_total_wei, 0);
    // R4 (BSC3-069): the soft bounds are the qualified values; an
    // outlier never arms in the repository.
    let x = exec(CONFIG);
    assert_eq!(
        (
            x.live_risk_soft_buy_slippage_bps,
            x.live_risk_soft_exit_slippage_bps,
            x.live_risk_soft_gas_price_wei,
            x.live_risk_outlier
        ),
        (100, 100, 50_000_000, false)
    );
    // R4: one C3 BUY per arm; the frozen import: no BUY.
    assert_eq!(
        (x.live_c3_rearm, x.live_builder_policy.as_str()),
        (false, POLICY)
    );
    // R5: a stalled exit is never re-priced in the repository; no
    // public exit route.
    assert_eq!(x.live_exit_reprice_bps, 0);
    assert_eq!(
        (x.live_public_exit, x.live_public_exit_rpc.as_str()),
        (false, "")
    );
    // R5: a migrated lot stays stranded for the operator.
    assert!(!x.live_dex_exit);
    assert!(!x.live_divergence_quarantine);
    assert!(
        exec(&CONFIG.replace(
            "BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE=OFF",
            "BSC_EXEC_LIVE_DIVERGENCE_QUARANTINE=ON",
        ))
        .live_divergence_quarantine
    );
    let on = exec(
        &CONFIG
            .replace(
                "BSC_EXEC_LIVE_PUBLIC_EXIT=OFF",
                "BSC_EXEC_LIVE_PUBLIC_EXIT=ON",
            )
            .replace(
                "BSC_EXEC_LIVE_PUBLIC_EXIT_RPC=\n",
                "BSC_EXEC_LIVE_PUBLIC_EXIT_RPC=https://rpc.example.test/\n",
            ),
    );
    assert_eq!(
        (on.live_public_exit, on.live_public_exit_rpc.as_str()),
        (true, "https://rpc.example.test/")
    );
    assert_eq!(
        exec(&CONFIG.replace(
            "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=0",
            "BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS=3600000"
        ))
        .live_grant_max_duration_ms,
        3_600_000
    );
}

/// REV.1.0 R3: a lot the operator resumed is stranded no more and is
/// read again at once (no backoff, no refused answer counted from
/// before); another lot keeps its state.
#[test]
fn a_resumed_lot_is_read_again_at_once() {
    let config = RuntimeConfig::parse(CONFIG).unwrap().canonical.unwrap();
    let mut lot = LotDeciders::new(&config);
    let (a, b) = ([0xA1; 20], [0xB1; 20]);
    for t in [a, b] {
        lot.stranded
            .push((t, crate::sources::LIVE_POSITION_UNREADABLE));
        lot.retry_later(t);
        lot.refused_in_a_row(t, true);
    }
    lot.resume(a);
    assert_eq!(
        lot.stranded,
        vec![(b, crate::sources::LIVE_POSITION_UNREADABLE)]
    );
    assert!(lot.retry_at.iter().all(|(t, _)| *t == b));
    assert_eq!(lot.refused, vec![(b, 1)]);
}

/// Phase E review L3: while the drift guard says DRIFT, a Four launch the
/// follower matched (or mismatched) against the frozen derivation counts
/// as UNKNOWN: the freeze proves nothing about a drifted deployment.
#[test]
fn a_drifted_four_deployment_proves_no_token() {
    use crate::chain_follower::{CanonicalLaunch, FourToken, Venue};
    const BLOCK_MS: u64 = 1_792_000_000_000;
    let base = std::env::temp_dir().join(format!("bsc-runtime-four-drift-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let cc = config.canonical.clone().unwrap();
    let mut c = Canonical::new(&cc, BLOCK_MS);
    assert_eq!(c.four_deployment, "UNPROBED");
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let core = IngressCore::new(64, config.max_launch_calldata);
    let block = CanonicalEvent::Block {
        number: 10,
        hash: [10; 32],
        parent_hash: [9; 32],
        timestamp_s: BLOCK_MS / 1_000,
        timestamp_ms: BLOCK_MS,
        txs: Vec::new(),
        observed: Instant::now(),
        observed_unix_ms: Some(BLOCK_MS + 300),
    };
    let four = |tx: u8| {
        CanonicalEvent::Launch(CanonicalLaunch {
            venue: Venue::Four,
            block_number: 10,
            block_hash: [10; 32],
            tx_index: u64::from(tx),
            tx_hash: [tx; 32],
            token: Some([tx; 20]),
            creator: None,
            external_buys_after: None,
            first_external_buy_index: None,
            sender: Some([tx; 20]),
            nonce: Some(0),
            c1_scope: false,
            venue_target: true,
            four_token: FourToken::Matched,
            four_raw: None,
            unverifiable: None,
        })
    };
    c.event(10, block, &mut journal);
    c.event(10, four(1), &mut journal);
    c.four_drift = true;
    c.event(10, four(2), &mut journal);
    c.join(u64::MAX, &core, &mut journal);
    let k = c.ledger.counters();
    assert_eq!(
        (
            k.four_launches,
            k.four_pending_decoded,
            k.four_pending_unknown
        ),
        (2, 1, 1)
    );
    let _ = journal.finish();
    fs::remove_dir_all(&base).unwrap();
}

/// SC4: a Four.meme launch of the window is journaled for Strategy C with
/// the curve its classification on the drift guard's validated deployment
/// names; unprobed or drifted, as not validated (never priced on an
/// unproven deployment).
#[test]
fn a_four_launch_is_journaled_with_its_classified_curve() {
    use crate::chain_follower::{CanonicalLaunch, FourToken, Venue};
    use crate::runtime::exec::tests::{four_observation, openfour_launch};
    use crate::runtime_strategy::{FOUR_LAUNCH_REFUSALS, four_launch_of};
    use alloy_primitives::Address;
    use darkbot_bsc_journal::storage::run::RunReader;
    use darkbot_bsc_trading::strategy::book::FourCurve;
    use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    use darkbot_bsc_trading::venues::fourmeme::recognize::{Recognized, recognize};
    const BLOCK_MS: u64 = 1_792_000_000_000;
    let base = std::env::temp_dir().join(format!("bsc-runtime-four-seed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let cc = config.canonical.clone().unwrap();
    let mut c = Canonical::new(&cc, BLOCK_MS);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let d = bsc_mainnet_r12d_131608421();
    let v = d.validate(&four_observation(&d).observed, 1).unwrap();
    let (hash, raw, launcher) = openfour_launch(1);
    let target = darkbot_bsc_geth_ipc::ingress::decode_tx_target(&raw).unwrap();
    let Recognized::OpenFourLaunch(l) =
        recognize(&d, target.to.map(Address::from), target.data, 16_384)
    else {
        panic!()
    };
    let token = d
        .openfour_token(
            l.preset_id,
            l.init.token_salt,
            l.request_id,
            Some(Address::from(launcher)),
        )
        .unwrap()
        .into_array();
    let launch = |tx: u64| {
        CanonicalEvent::Launch(CanonicalLaunch {
            venue: Venue::OpenFour,
            block_number: 10,
            block_hash: [10; 32],
            tx_index: tx,
            tx_hash: hash,
            token: Some(token),
            creator: Some(launcher),
            external_buys_after: None,
            first_external_buy_index: None,
            sender: Some(launcher),
            nonce: Some(1),
            c1_scope: true,
            venue_target: true,
            four_token: FourToken::Matched,
            four_raw: Some(raw.clone()),
            unverifiable: None,
        })
    };
    c.event(
        10,
        CanonicalEvent::Block {
            number: 10,
            hash: [10; 32],
            parent_hash: [9; 32],
            timestamp_s: BLOCK_MS / 1_000,
            timestamp_ms: BLOCK_MS,
            txs: Vec::new(),
            observed: Instant::now(),
            observed_unix_ms: Some(BLOCK_MS + 300),
        },
        &mut journal,
    );
    // Unprobed, validated, drifted.
    c.event(10, launch(2), &mut journal);
    c.four_validated = Some(v);
    c.event(10, launch(3), &mut journal);
    c.four_drift = true;
    c.event(10, launch(4), &mut journal);
    journal.finish().unwrap();
    let run = fs::read_dir(&base).unwrap().next().unwrap().unwrap().path();
    let mut reader = RunReader::open(&run, journal_max_payload(&config)).unwrap();
    let mut seeds = Vec::new();
    while let Some(r) = reader.next_record().unwrap() {
        if r.payload.first() == Some(&b'n') {
            seeds.push(four_launch_of(&r.payload).unwrap());
        }
    }
    let curves: Vec<_> = seeds.iter().map(|s| (s.2, s.3, s.4, s.5)).collect();
    let classified = crate::runtime_strategy::classify_four_launch(
        Some(v),
        Venue::OpenFour,
        Some(&raw),
        Some(launcher),
        (token, hash),
        16_384,
    )
    .unwrap();
    let FourCurve::OpenFour { start, fee, .. } = classified else {
        panic!()
    };
    assert_eq!(
        curves,
        [
            (2, token, launcher, Err(FOUR_LAUNCH_REFUSALS[0])),
            (3, token, launcher, Ok(FourCurve::openfour(start, fee))),
            (4, token, launcher, Err(FOUR_LAUNCH_REFUSALS[0])),
        ]
    );
    fs::remove_dir_all(&base).unwrap();
}

/// BSC3-050: the gate's view of a reconcile carries the one LIVE
/// qualification verified for it (the LIVE path ticks with the same);
/// without a LIVE path it reads none.
#[test]
fn the_reconcile_reads_its_one_verified_qualification() {
    let dir = std::env::temp_dir().join(format!("bsc-runtime-q-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let q = crate::sources::LiveQualification {
        venues: vec![crate::sources::VenueId::Flap],
        acquisition: true,
        ..Default::default()
    };
    let g = Generation::initial();
    assert_eq!(
        LiveView::read(
            &g,
            &dir,
            Some(&q),
            1,
            None,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .qualification,
        q
    );
    assert_eq!(
        LiveView::read(
            &g,
            &dir,
            None,
            1,
            None,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .qualification,
        crate::sources::LiveQualification::default()
    );
}

/// BSC3-014/038/016: a canonical launch of a block that is not the last
/// one reported is counted and journaled as received (the census shows
/// it unjoined), never joined; Four.meme launches are counted by
/// visibility class; the status names the last census refresh.
#[test]
fn a_launch_without_its_head_and_four_classes_are_counted() {
    use crate::chain_follower::{CanonicalLaunch, Venue};
    const BLOCK_MS: u64 = 1_792_000_000_000;
    let base = std::env::temp_dir().join(format!("bsc-runtime-headless-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let cc = config.canonical.clone().unwrap();
    let block_ms = |n: u8| BLOCK_MS + u64::from(n) * 450;
    let mut c = Canonical::new(&cc, block_ms(10));
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let core = IngressCore::new(64, config.max_launch_calldata);
    let clock = Clock(Instant::now());
    let block = |n: u8| CanonicalEvent::Block {
        number: u64::from(n),
        hash: [n; 32],
        parent_hash: [n - 1; 32],
        timestamp_s: block_ms(n) / 1_000,
        timestamp_ms: block_ms(n),
        txs: Vec::new(),
        observed: clock.0,
        observed_unix_ms: Some(block_ms(n) + 300),
    };
    let launch = |n: u8, tx: u8, venue| {
        CanonicalEvent::Launch(CanonicalLaunch {
            venue,
            block_number: u64::from(n),
            block_hash: [n; 32],
            tx_index: 0,
            tx_hash: [tx; 32],
            token: Some([tx; 20]),
            creator: None,
            external_buys_after: Some(0),
            first_external_buy_index: None,
            sender: Some([tx; 20]),
            nonce: Some(u64::from(tx)),
            c1_scope: false,
            venue_target: false,
            four_token: crate::chain_follower::FourToken::NotApplicable,
            four_raw: None,
            unverifiable: None,
        })
    };
    c.event(10, block(10), &mut journal);
    c.event(11, block(11), &mut journal);
    // Block 10's launch after block 11 was reported: no head to join it.
    c.event(11, launch(10, 7, Venue::Flap), &mut journal);
    assert_eq!((c.launch_head_missing, c.ledger.waiting()), (1, 0));
    // A Four launch never seen: missed, as a Four class.
    let CanonicalEvent::Launch(mut four) = launch(11, 8, Venue::Four) else {
        unreachable!()
    };
    four.four_token = crate::chain_follower::FourToken::NotRecognized;
    c.event(11, CanonicalEvent::Launch(four), &mut journal);
    c.join(u64::MAX, &core, &mut journal);
    let k = c.ledger.counters();
    assert_eq!(
        (
            k.four_launches,
            k.four_missed,
            k.four_seen_full_before,
            k.missed
        ),
        (1, 1, 0, 0)
    );
    let refresh = {
        let mut r = CensusRefresh::default();
        r.ended(Ok(Err("disk full".into())));
        assert_eq!(r.state.as_deref(), Some("FAILED: disk full"));
        r.ended(Ok(Ok(42)));
        r
    };
    let status = status_bytes(
        &Generation::initial(),
        &[],
        clock,
        1,
        1,
        0,
        "j",
        0,
        0,
        None,
        Some(&c),
        None,
        None,
        None,
        config.status_ms,
        (config.status_stale_intervals, config.owner),
        &refresh,
    )
    .unwrap();
    let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
    assert_eq!(r.text(rs::CENSUS_STATE).unwrap(), "OK");
    // BSC3-015: an unreadable clock is expired time, and counted.
    let before = r.u64(rs::CLOCK_UNREADABLE).unwrap();
    assert_eq!(wall_or_expired(Err("no clock".into())), u64::MAX);
    assert_eq!(wall_or_expired(Ok(7)), 7);
    assert!(CLOCK_UNREADABLE.load(std::sync::atomic::Ordering::Relaxed) > before);
    assert_eq!(r.u64(rs::CENSUS_WRITTEN_UNIX_MS).unwrap(), 42);
    let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
    assert_eq!(k.u64(rs::canonical::LAUNCH_HEAD_MISSING).unwrap(), 1);
    assert_eq!(k.u64(rs::canonical::FOUR_MISSED).unwrap(), 1);
    journal.finish().unwrap();
    let census = census(&base, journal_max_payload(&config), None).unwrap();
    assert_eq!(census.unjoined, 1);
    fs::remove_dir_all(base).unwrap();
}

/// BSC3-036: a failover makes the next endpoint the one the status
/// names and is counted; a stalled step is counted apart from the RPC
/// errors and shows as STALLED.
#[test]
fn a_failover_and_a_stall_reach_the_status() {
    let base = std::env::temp_dir().join(format!("bsc-runtime-failover-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let cc = config.canonical.clone().unwrap();
    let mut c = Canonical::new(&cc, 0);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let status = |c: &Canonical| {
        let bytes = status_bytes(
            &Generation::initial(),
            &[],
            Clock(Instant::now()),
            1,
            1,
            0,
            "j",
            0,
            0,
            None,
            Some(c),
            None,
            None,
            None,
            config.status_ms,
            (config.status_stale_intervals, config.owner),
            &CensusRefresh::default(),
        )
        .unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &bytes).unwrap();
        let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
        (
            k.text(rs::canonical::ENDPOINT).unwrap().to_string(),
            k.u64(rs::canonical::FAILOVERS).unwrap(),
            k.u64(rs::canonical::STALLS).unwrap(),
            k.u64(rs::canonical::RPC_ERRORS).unwrap(),
            k.text(rs::canonical::STATE).unwrap().to_string(),
        )
    };
    assert_eq!(
        status(&c),
        (
            "bsc-dataseed.bnbchain.org".into(),
            0,
            0,
            0,
            "CONNECTING".into()
        )
    );
    c.event(
        1,
        CanonicalEvent::Fault(FollowerFault::Stalled {
            head: 10,
            for_ms: 5_000,
        }),
        &mut journal,
    );
    c.event(
        2,
        CanonicalEvent::Failover {
            from: 0,
            to: 1,
            failed_steps: 3,
        },
        &mut journal,
    );
    assert_eq!(
        status(&c),
        ("bsc-dataseed1.defibit.io".into(), 1, 1, 0, "STALLED".into())
    );
    // BSC3-056: the second source: STALE before its first header, AGREED
    // once it holds the followed head's hash, DISAGREES on another.
    let confirm = |c: &Canonical| {
        let bytes = status_bytes(
            &Generation::initial(),
            &[],
            Clock(Instant::now()),
            1,
            1,
            0,
            "j",
            0,
            0,
            None,
            Some(c),
            None,
            None,
            None,
            config.status_ms,
            (config.status_stale_intervals, config.owner),
            &CensusRefresh::default(),
        )
        .unwrap();
        let r = darkbot_bsc_record::decode(&rs::SCHEMA, &bytes).unwrap();
        let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
        (
            k.opt_text(rs::canonical::CONFIRM_ENDPOINT)
                .unwrap()
                .map(str::to_string),
            k.opt_u64(rs::canonical::CONFIRM_HEAD).unwrap(),
            k.opt_text(rs::canonical::CONFIRM_STATE)
                .unwrap()
                .map(str::to_string),
        )
    };
    let host = Some("bsc-rpc.publicnode.com".to_string());
    assert_eq!(confirm(&c), (host.clone(), None, Some("STALE".into())));
    let now = observed_now().unwrap();
    c.event(
        3,
        CanonicalEvent::Block {
            number: 10,
            hash: [10; 32],
            parent_hash: [9; 32],
            timestamp_s: now / 1_000,
            timestamp_ms: now,
            txs: Vec::new(),
            observed: Instant::now(),
            observed_unix_ms: Some(now),
        },
        &mut journal,
    );
    let header = |hash: [u8; 32]| crate::chain_follower::ConfirmEvent::Header {
        number: 10,
        hash,
        parent_hash: [9; 32],
        block_ms: now,
    };
    c.confirm_event(header([10; 32]));
    assert_eq!(confirm(&c), (host.clone(), Some(10), Some("AGREED".into())));
    c.confirm_event(header([0xee; 32]));
    assert_eq!(confirm(&c), (host, Some(10), Some("DISAGREES".into())));
    // Without a second source: none of it.
    let mut none = cc.clone();
    none.confirm = None;
    assert_eq!(confirm(&Canonical::new(&none, 0)), (None, None, None));
    journal.finish().unwrap();
    fs::remove_dir_all(base).unwrap();
}

/// BSC3-062: a launch whose sighting the bounded ingress window forgot
/// before its join is EVICTED (not evaluable) in the counters and the
/// status, never MISSED; a launch never seen, with nothing forgotten
/// that could have been it, stays MISSED.
#[test]
fn a_forgotten_sighting_is_evicted_not_missed() {
    use crate::chain_follower::{CanonicalLaunch, Venue};
    const BLOCK_MS: u64 = 1_792_000_000_000;
    let base = std::env::temp_dir().join(format!("bsc-runtime-evict-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let cc = config.canonical.clone().unwrap();
    assert!(cc.join_horizon_ms > 0);
    let mut c = Canonical::new(&cc, BLOCK_MS);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    // A window of two sightings: launch 1's hash, seen 50 ms before its
    // block, leaves it before the join.
    let mut core = IngressCore::new(2, config.max_launch_calldata);
    for (n, hash) in [[1u8; 32], [7; 32], [8; 32]].into_iter().enumerate() {
        core.observe(
            SourceId::PublicNode,
            Observation::HashOnly { hash },
            n as u64,
            Some((BLOCK_MS - 50) * 1_000),
        );
    }
    assert_eq!(core.sighting(&[1; 32]), None);
    let clock = Clock(Instant::now());
    c.event(
        1,
        CanonicalEvent::Block {
            number: 10,
            hash: [10; 32],
            parent_hash: [9; 32],
            timestamp_s: BLOCK_MS / 1_000,
            timestamp_ms: BLOCK_MS,
            txs: Vec::new(),
            observed: clock.0,
            observed_unix_ms: Some(BLOCK_MS + 300),
        },
        &mut journal,
    );
    for tx in [1u8, 2] {
        c.event(
            1,
            CanonicalEvent::Launch(CanonicalLaunch {
                venue: Venue::Flap,
                block_number: 10,
                block_hash: [10; 32],
                tx_index: u64::from(tx),
                tx_hash: [tx; 32],
                token: Some([tx; 20]),
                creator: None,
                external_buys_after: Some(0),
                first_external_buy_index: None,
                sender: Some([tx; 20]),
                nonce: Some(u64::from(tx)),
                c1_scope: true,
                venue_target: true,
                four_token: crate::chain_follower::FourToken::NotApplicable,
                four_raw: None,
                unverifiable: None,
            }),
            &mut journal,
        );
    }
    c.join(u64::MAX, &core, &mut journal);
    let k = c.ledger.counters();
    assert_eq!((k.flap_launches, k.evicted, k.missed), (2, 1, 1));
    let status = status_bytes(
        &Generation::initial(),
        &[],
        clock,
        1,
        1,
        0,
        "j",
        0,
        0,
        None,
        Some(&c),
        None,
        None,
        None,
        config.status_ms,
        (config.status_stale_intervals, config.owner),
        &CensusRefresh::default(),
    )
    .unwrap();
    let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
    let k = r.opt_nested(rs::CANONICAL).unwrap().unwrap();
    assert_eq!(
        (
            k.u64(rs::canonical::EVICTED).unwrap(),
            k.u64(rs::canonical::MISSED).unwrap(),
            k.u64(rs::canonical::FOUR_EVICTED).unwrap()
        ),
        (1, 1, 0)
    );
    journal.finish().unwrap();
    fs::remove_dir_all(base).unwrap();
}

/// The follower's events, joined with the sightings, reach the status
/// counters, the journal and the census alike; a reorg takes its
/// launches out of all three.
#[test]
fn canonical_events_join_into_status_journal_and_census() {
    use crate::chain_follower::{CanonicalLaunch, Venue};
    use crate::wss::ProtocolFault;
    const BLOCK_MS: u64 = 1_792_000_000_000;
    let base = std::env::temp_dir().join(format!("bsc-runtime-canon-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let cc = config.canonical.clone().unwrap();
    let block_ms = |n: u8| BLOCK_MS + u64::from(n) * 450;
    // The process started at block 10's time.
    let mut c = Canonical::new(&cc, block_ms(10));
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let mut core = IngressCore::new(64, config.max_launch_calldata);
    let clock = Clock(Instant::now());
    let block = |n: u8| CanonicalEvent::Block {
        number: u64::from(n),
        hash: [n; 32],
        parent_hash: [n - 1; 32],
        timestamp_s: block_ms(n) / 1_000,
        timestamp_ms: block_ms(n),
        txs: Vec::new(),
        observed: clock.0,
        // The follower held the header 300 ms after the block time.
        observed_unix_ms: Some(block_ms(n) + 300),
    };
    let launch = |n: u8, tx: u8, buys, c1_scope| {
        CanonicalEvent::Launch(CanonicalLaunch {
            venue: Venue::Flap,
            block_number: u64::from(n),
            block_hash: [n; 32],
            tx_index: 0,
            tx_hash: [tx; 32],
            token: Some([tx; 20]),
            creator: None,
            external_buys_after: buys,
            first_external_buy_index: None,
            sender: Some([tx; 20]),
            nonce: Some(u64::from(tx)),
            c1_scope,
            venue_target: c1_scope,
            four_token: crate::chain_follower::FourToken::NotApplicable,
            four_raw: None,
            unverifiable: None,
        })
    };
    // Launch 1: a hash 50 ms before its block time; launch 2: a full
    // sighting 100 ms after its block time but before the header fetch
    // (counted after, never before); launch 3: unseen.
    core.observe(
        SourceId::PublicNode,
        Observation::HashOnly { hash: [1; 32] },
        5,
        Some((block_ms(10) - 50) * 1_000),
    );
    core.observe(
        SourceId::PublicNode,
        Observation::HashOnly { hash: [2; 32] },
        6,
        Some((block_ms(11) + 100) * 1_000),
    );
    journal.record(
        RecordKind::Runtime,
        1,
        source_event_payload(SourceId::PublicNode, SourceEvent::Connected),
    );
    // Mined before the process started (a C2 resume backfill): its
    // launch, the gap before it and the block are outside the window.
    c.event(9, CanonicalEvent::Gap { from: 5, to: 8 }, &mut journal);
    c.event(9, block(9), &mut journal);
    c.event(9, launch(9, 9, Some(0), true), &mut journal);
    assert_eq!(
        (
            c.backfill_blocks,
            c.backfill_launches,
            c.blocks,
            c.resyncs,
            c.blocks_skipped
        ),
        (1, 1, 0, 0, 0)
    );
    assert_eq!(c.ledger.waiting(), 0);
    c.event(10, block(10), &mut journal);
    c.event(10, launch(10, 1, Some(2), true), &mut journal);
    // A launch whose transaction the verifier refuses (e.g. a type it
    // does not rebuild): a counted per-launch fact, never a stall.
    let CanonicalEvent::Launch(mut refused) = launch(10, 4, Some(0), false) else {
        unreachable!()
    };
    refused.sender = None;
    refused.nonce = None;
    refused.unverifiable = Some(crate::wss::RejectReason::UnsupportedType(4));
    c.event(10, CanonicalEvent::Launch(refused), &mut journal);
    // A Four launch whose provider answers were refused every time:
    // counted for every venue.
    let CanonicalEvent::Launch(mut four) = launch(10, 5, None, false) else {
        unreachable!()
    };
    four.venue = Venue::Four;
    four.sender = None;
    four.nonce = None;
    four.unverifiable = Some(crate::wss::RejectReason::HashMismatch);
    // As the follower decides it: refused bytes are no decoded launch.
    four.four_token = crate::chain_follower::FourToken::NotRecognized;
    c.event(10, CanonicalEvent::Launch(four), &mut journal);
    c.event(20, block(11), &mut journal);
    c.event(20, launch(11, 2, None, false), &mut journal);
    c.event(20, launch(11, 3, Some(0), true), &mut journal);
    assert_eq!(c.ledger.waiting(), 5);
    c.event(
        25,
        CanonicalEvent::Fault(FollowerFault::Request {
            call: crate::chain_follower::Call::BlockNumber,
            error: RequestError::Transport(DisconnectReason::Protocol(
                ProtocolFault::MalformedJson,
            )),
        }),
        &mut journal,
    );
    assert_eq!(c.state, "FAILING");
    c.event(30, block(12), &mut journal);
    // Not yet due (join delay), then due.
    c.join(30, &core, &mut journal);
    assert_eq!(c.ledger.counters().flap_launches, 0);
    c.join(u64::MAX, &core, &mut journal);
    let k = c.ledger.counters();
    assert_eq!(
        (
            k.flap_launches,
            k.seen_hash_only_before,
            k.seen_after,
            k.missed,
            k.external_buys,
            k.buys_unknown,
            k.c1_scope_launches,
            k.tx_unverifiable,
            k.tx_answer_refused,
            k.four_launches
        ),
        (4, 1, 1, 2, 2, 1, 2, 1, 1, 1)
    );
    c.event(
        40,
        CanonicalEvent::Reorg {
            orphaned: vec![(11, [11; 32]), (12, [12; 32])],
            new_head: (12, [0xEE; 32]),
        },
        &mut journal,
    );
    c.event(50, CanonicalEvent::Gap { from: 13, to: 99 }, &mut journal);
    let k = c.ledger.counters();
    assert_eq!(
        (
            k.flap_launches,
            k.missed,
            k.orphaned_launches,
            k.c1_scope_launches,
            k.tx_unverifiable
        ),
        (2, 1, 2, 1, 1)
    );
    assert_eq!(
        (
            c.state,
            c.blocks,
            c.reorgs,
            c.resyncs,
            c.rpc_errors,
            c.blocks_skipped
        ),
        ("FOLLOWING", 3, 1, 1, 1, 87)
    );
    let status = status_bytes(
        &Generation::initial(),
        &[],
        clock,
        1,
        1,
        0,
        "j",
        0,
        0,
        Some(&CaptureStatus {
            budget_max: 64,
            budget_peak: 9,
            ..CaptureStatus::default()
        }),
        Some(&c),
        None,
        None,
        None,
        config.status_ms,
        (config.status_stale_intervals, config.owner),
        &CensusRefresh::default(),
    )
    .unwrap();
    let r = darkbot_bsc_record::decode(&rs::SCHEMA, &status).unwrap();
    assert!(
        r.opt_nested(rs::STATE).unwrap().is_none(),
        "no C2 owner, no state record"
    );
    assert_eq!(r.u64(rs::STATUS_INTERVAL_MS).unwrap(), config.status_ms);
    // Audit E 3.1/3.3: the stale factor every reader applies, and the
    // attempts' maximum the tabs offer.
    assert_eq!(
        (
            r.u64(rs::STATUS_STALE_INTERVALS).unwrap(),
            r.u64(rs::BUY_ATTEMPTS_TOTAL_MAX).unwrap()
        ),
        (
            config.status_stale_intervals,
            u64::from(config.owner.buy_attempts_total_max)
        )
    );
    let written = r.u64(rs::PROVENANCE_OBSERVED_UNIX_MS).unwrap();
    let bound = config.status_stale_intervals * config.status_ms;
    assert!(crate::sources::status_is_current(&r, written + bound));
    assert!(!crate::sources::status_is_current(&r, written + bound + 1));
    // Phase F: the capture's storage health rides along.
    let k = r.nested(rs::CAPTURE).unwrap();
    assert_eq!(
        (
            k.u64(rs::capture::BUDGET_MAX_BYTES).unwrap(),
            k.u64(rs::capture::BUDGET_PEAK_BYTES).unwrap(),
            k.text(rs::capture::STATE).unwrap()
        ),
        (64, 9, "HEALTHY")
    );
    assert_eq!(k.opt_u64(rs::capture::DURABLE_SEQUENCE).unwrap(), None);
    let n = r.nested(rs::CANONICAL).unwrap();
    assert_eq!(n.u64(rs::canonical::FLAP_LAUNCHES).unwrap(), 2);
    assert_eq!(n.u64(rs::canonical::TX_UNVERIFIABLE).unwrap(), 1);
    assert_eq!(n.u64(rs::canonical::TX_ANSWER_REFUSED).unwrap(), 1);
    assert_eq!(n.u64(rs::canonical::HEAD).unwrap(), 12);
    assert_eq!(n.u64(rs::canonical::BLOCKS_SKIPPED).unwrap(), 87);
    assert_eq!(n.u64(rs::canonical::C1_SCOPE_LAUNCHES).unwrap(), 1);
    assert_eq!(n.i64(rs::canonical::HEAD_FETCH_LAG_MS).unwrap(), 300);
    assert_eq!(n.u64(rs::canonical::BACKFILL_BLOCKS).unwrap(), 1);
    assert_eq!(n.u64(rs::canonical::BACKFILL_LAUNCHES).unwrap(), 1);
    journal.finish().unwrap();
    let census = census(&base, journal_max_payload(&config), None).unwrap();
    assert_eq!(
        (
            census.flap_launches,
            census.hash_only_before,
            census.orphaned_launches,
            census.c1_scope_launches,
            census.unjoined,
            census.blocks_skipped,
            census.tx_unverifiable,
            census.tx_answer_refused,
            census.four_launches
        ),
        (2, 1, 2, 1, 0, 87, 1, 1, 1)
    );
    let w = &census.windows[0];
    assert_eq!((w.first_block, w.reorgs, w.resyncs), (Some(10), 1, 1));
    // No clean end record in this unit test: a killed process's window.
    assert_eq!(w.state, "UNTERMINATED");
    assert_eq!(census.fetch_lag_ms_min, Some(300));
    fs::remove_dir_all(&base).unwrap();
}

/// A too-deep reorg or a gap reaching below the window (a resume
/// backfill) counts only the window's heights as skipped (review 4
/// LOWs: before, 9 was counted although it precedes the window).
#[test]
fn a_too_deep_reorg_never_counts_heights_before_the_window() {
    const BLOCK_MS: u64 = 1_792_000_000_000;
    let base = std::env::temp_dir().join(format!("bsc-runtime-deep-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let mut c = Canonical::new(config.canonical.as_ref().unwrap(), BLOCK_MS + 10);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let block = |n: u64| CanonicalEvent::Block {
        number: n,
        hash: [n as u8; 32],
        parent_hash: [n as u8 - 1; 32],
        timestamp_s: (BLOCK_MS + n) / 1_000,
        timestamp_ms: BLOCK_MS + n,
        txs: Vec::new(),
        observed: Instant::now(),
        observed_unix_ms: None,
    };
    c.event(1, block(9), &mut journal);
    c.event(2, block(10), &mut journal);
    c.event(3, block(11), &mut journal);
    assert_eq!(
        (c.backfill_blocks, c.blocks, c.window_first_block),
        (1, 2, 10)
    );
    c.event(
        4,
        CanonicalEvent::ReorgTooDeep {
            orphaned: vec![(9, [9; 32]), (10, [10; 32]), (11, [11; 32])],
            new_head: (14, [0xEE; 32]),
        },
        &mut journal,
    );
    // Heights 10..=13, not 9..=13.
    assert_eq!((c.resyncs, c.blocks_skipped), (1, 4));
    // A gap after a failed reorg can start below the window (the
    // follower restarts at the fork point): 9 is not counted either.
    c.event(5, CanonicalEvent::Gap { from: 9, to: 13 }, &mut journal);
    assert_eq!((c.resyncs, c.blocks_skipped), (2, 8));
    journal.finish().unwrap();
    fs::remove_dir_all(&base).unwrap();
}

/// BSC3-039: a due checkpoint is handed to the writer, the loop never
/// waits for its write (it was written here, 10–34 ms measured for
/// 479 KB, every 30 s); the next pass counts it; at the stop the loop
/// writes the last window itself, after the writer's and never beside
/// it.
#[test]
fn the_loop_hands_its_checkpoint_over_and_writes_only_the_last() {
    use darkbot_bsc_trading::chain_state::engine::StateLimits;
    let dir = std::env::temp_dir().join(format!("bsc-runtime-ckpt-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c2-checkpoint.bin");
    let config = StateConfig {
        limits: StateLimits {
            max_txs: 1_024,
            max_candidates_per_nonce: 4,
            max_watched_senders: 4,
            reorg_window: 8,
            finality_depth: 2,
        },
        watched: Vec::new(),
        checkpoint_ms: 1_000,
        nonce_fact_blocks: 1,
    };
    let mut o = StateOwner::fresh(&config, path.clone()).unwrap();
    static WRITING: AtomicBool = AtomicBool::new(false);
    static OVERLAPPED: AtomicBool = AtomicBool::new(false);
    fn slow(path: &Path, bytes: &[u8]) -> Result<(), String> {
        if WRITING.swap(true, Ordering::SeqCst) {
            OVERLAPPED.store(true, Ordering::SeqCst);
        }
        thread::sleep(Duration::from_millis(600));
        let written = replace(path, bytes);
        WRITING.store(false, Ordering::SeqCst);
        written
    }
    let writer = CheckpointWriter::spawn(path.clone(), Arc::new(slow)).unwrap();
    let t = Instant::now();
    checkpoint_pass(&mut o, &writer, true, false, slow);
    // A liveness margin, not a timing claim: the write takes 600 ms.
    assert!(
        t.elapsed() < Duration::from_millis(400),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(o.checkpoint_counts(), (0, 0));
    // Passes not due hand nothing over; one of them counts the write.
    let counted = Instant::now();
    while o.checkpoint_counts().0 == 0 {
        assert!(
            counted.elapsed() < Duration::from_secs(120),
            "never counted"
        );
        checkpoint_pass(&mut o, &writer, false, false, slow);
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(o.checkpoint_counts(), (1, 0));
    assert_eq!(fs::read(&path).unwrap(), o.checkpoint_bytes());
    // The stop: waits for the writer's write, then writes the last
    // window itself.
    checkpoint_pass(&mut o, &writer, true, false, slow);
    checkpoint_pass(&mut o, &writer, true, true, slow);
    assert_eq!(o.checkpoint_counts(), (3, 0));
    assert!(!OVERLAPPED.load(Ordering::SeqCst), "two writes at once");
    assert!(!dir.join("c2-checkpoint.tmp").exists());
    drop(writer);
    fs::remove_dir_all(&dir).unwrap();
}

/// BSC3-039: a durable replacement is flushed to the device, the
/// status's is not; both publish the whole new content.
#[test]
fn only_durable_records_are_flushed() {
    let dir = std::env::temp_dir().join(format!("bsc-runtime-flush-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("record.dbrec");
    use darkbot_bsc_journal::durable::{Op, hooks};
    let before = FLUSHED.with(std::cell::Cell::get);
    hooks::record();
    replace_volatile(&path, b"status").unwrap();
    assert_eq!(FLUSHED.with(std::cell::Cell::get), before);
    assert_eq!(hooks::take(), []);
    assert_eq!(fs::read(&path).unwrap(), b"status");
    replace(&path, b"census").unwrap();
    assert_eq!(FLUSHED.with(std::cell::Cell::get), before + 1);
    // Its rename too, on the medium (BSC3-039 review).
    assert_eq!(hooks::take(), [Op::Fsync, Op::FullFlush]);
    assert_eq!(fs::read(&path).unwrap(), b"census");
    assert!(!dir.join("record.tmp").exists());
    fs::remove_dir_all(&dir).unwrap();
}

/// With every source OFF the runtime connects nothing, writes its
/// status (all OFF, READ_ONLY) and stops on the stop file with a
/// committed journal.
#[test]
fn an_idle_runtime_writes_its_status_and_stops_cleanly() {
    let base = std::env::temp_dir().join(format!("bsc-runtime-idle-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    // No canonical endpoint: this test opens no connection at all.
    let text: String = CONFIG
        .replace("STATUS_MS=1000", "STATUS_MS=100")
        .lines()
        .filter(|l| {
            !l.starts_with("BSC_CANONICAL_")
                && !l.starts_with("BSC_STATE_")
                && !l.starts_with("BSC_EXEC_")
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let config = RuntimeConfig::parse(&text).unwrap();
    let sources = base.join("bsc-sources");
    Store::open(&sources, crate::sources::tests::keeping(8))
        .unwrap()
        .init(1)
        .unwrap();
    let rundir = base.clone();
    // BSC3-039: the loop writes its status every interval, never
    // flushing it to the device (counted on the loop's own thread).
    let runner = thread::spawn(move || {
        let ran = run(&config, &sources, &rundir, Path::new("/nonexistent-repo"));
        (ran, FLUSHED.with(std::cell::Cell::get))
    });
    let status = base.join("bsc-runtime/status.dbrec");
    let started = Instant::now();
    // A liveness bound, not a timing claim: a loaded host starts slowly.
    while !status.exists() {
        assert!(started.elapsed() < Duration::from_secs(120), "no status");
        thread::sleep(Duration::from_millis(20));
    }
    fs::write(base.join("bsc.runtime.stop"), b"stop\n").unwrap();
    let (ran, flushed) = runner.join().unwrap();
    ran.unwrap();
    // The census at the stop is durable; the status, written every
    // interval, never is.
    assert_eq!(flushed, 1, "the loop flushes only its final census");
    // Paket 8 review L7: the stop is consumed; the next start runs.
    assert!(!base.join("bsc.runtime.stop").exists());
    let r = darkbot_bsc_record::decode(&rs::SCHEMA, &fs::read(&status).unwrap()).unwrap();
    assert_eq!(r.text(rs::MODE).unwrap(), "READ_ONLY");
    let rows = r.nested_all(rs::SOURCES).unwrap();
    assert_eq!(rows.len(), 5);
    assert!(
        rows.iter()
            .all(|row| row.text(rs::source::HEALTH).unwrap() == "OFF")
    );
    // R6: each source's observed capability: nothing delivered yet,
    // UNKNOWN; 48Club QPT by design.
    assert_eq!(
        rows.iter()
            .map(|row| row.text(rs::source::CAPABILITY).unwrap())
            .collect::<Vec<_>>(),
        ["UNKNOWN", "UNKNOWN", "UNKNOWN", "UNKNOWN", "QPT"]
    );
    let journals: Vec<_> = fs::read_dir(base.join("bsc-runtime/journal"))
        .unwrap()
        .collect();
    assert_eq!(journals.len(), 1);
    fs::remove_dir_all(&base).unwrap();
}

/// REV.1.0 R6: each source has its own bounded ingress queue: a burst of
/// one source fills and drops only its own events, another source's still
/// go in (one shared queue dropped them); the runtime takes the event
/// received first among the heads, waits at most its bound for one, and a
/// push from an adapter thread wakes it.
#[test]
fn a_burst_of_one_source_drops_only_its_own_events() {
    let q = IngressQueues::new(2);
    assert!(q.push(SourceId::PublicNode, 5, Event::Alive));
    assert!(q.push(SourceId::PublicNode, 6, Event::Alive));
    assert!(!q.push(SourceId::PublicNode, 7, Event::Alive));
    assert!(q.push(SourceId::Drpc, 1, Event::Alive));
    let order: Vec<(SourceId, u64)> = std::iter::from_fn(|| q.pop(Duration::ZERO))
        .map(|(s, at, _)| (s, at))
        .collect();
    assert_eq!(
        order,
        [
            (SourceId::Drpc, 1),
            (SourceId::PublicNode, 5),
            (SourceId::PublicNode, 6)
        ]
    );
    let started = Instant::now();
    assert!(q.pop(Duration::from_millis(20)).is_none());
    assert!(started.elapsed() >= Duration::from_millis(20));
    let adapter = Arc::clone(&q);
    let pushed = thread::spawn(move || {
        thread::sleep(Duration::from_millis(10));
        adapter.push(SourceId::Geth, 9, Event::Alive)
    });
    // Woken by the push: back before its own bound (a lost wake-up returns
    // the event only at the bound).
    let bound = Duration::from_secs(30);
    let waited = Instant::now();
    assert_eq!(
        q.pop(bound).map(|(s, at, _)| (s, at)),
        Some((SourceId::Geth, 9))
    );
    assert!(waited.elapsed() < bound, "{:?}", waited.elapsed());
    assert!(pushed.join().unwrap());
}

/// SC4 d: a held Four.meme token read again is held on the curve its armed
/// deployment classifies (Classic on the priced word within its tax's
/// witnessed age for the SELL window, OpenFour on the executed preset: the
/// curve the strategy tracks); otherwise stranded with why (off its curve,
/// unpriced, no armed deployment).
#[test]
fn a_four_lot_read_again_is_classified_or_stranded() {
    use crate::sources::{LIVE_POSITION_OFF_CURVE, LIVE_POSITION_UNPRICED};
    use crate::venue_probe::FourHeld;
    use alloy_primitives::{Address, B256, U256};
    use canonical::four_held_curve;
    use darkbot_bsc_trading::economics::four_classic::{ClassicCurve, ClassicFee};
    use darkbot_bsc_trading::strategy::book::FourCurve;
    use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_r12d_131608421;
    use darkbot_bsc_trading::venues::fourmeme::profile::{
        CLASSIC_PROFILES, ClassicTokenRead, OpenFourTokenRead, WITNESSED_MAX_OFFERS,
        WITNESSED_TOTAL_SUPPLY, WITNESSED_VENUE_FEE,
    };
    const CLASSIC: &str =
        include_str!("../../trading/fixtures/fourmeme/classic_parity_20261104.txt");
    const OPENFOUR: &str =
        include_str!("../../trading/fixtures/fourmeme/openfour_parity_20261104.txt");
    let rows = |text: &'static str, kind: &'static str| {
        text.lines()
            .map(|l| l.split(' ').collect::<Vec<_>>())
            .filter(move |f| f[0] == kind)
    };
    let u = |s: &str| U256::from_str_radix(s, 10).unwrap();
    let d = bsc_mainnet_r12d_131608421();
    let v = d
        .validate(
            &crate::runtime::exec::tests::four_observation(&d).observed,
            1,
        )
        .unwrap();
    // Classic: an executed probe's state of the priced word.
    let profile = CLASSIC_PROFILES[0];
    let f = rows(CLASSIC, "PROBE")
        .find(|f| format!("0x{}", f[3]).parse::<B256>().ok() == Some(profile.template))
        .unwrap();
    let launch_block = CLASSIC
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .find_map(|l| match l[0] {
            "LAUNCH" if l[3].eq_ignore_ascii_case(f[2]) => l[1].parse::<u64>().ok(),
            "LAUNCHED" if l[1].eq_ignore_ascii_case(f[2]) => l[2].parse::<u64>().ok(),
            _ => None,
        })
        .unwrap();
    let classic = ClassicTokenRead {
        token: f[2].parse().unwrap(),
        block_number: f[1].parse().unwrap(),
        block_hash: B256::repeat_byte(0xb1),
        quote: Address::ZERO,
        template: profile.template,
        total_supply: WITNESSED_TOTAL_SUPPLY,
        max_offers: WITNESSED_MAX_OFFERS,
        offers: u(f[7]),
        k: u(f[5]),
        t: u(f[6]),
        status: U256::ZERO,
        fee_setting: u(f[4]),
        extra_fee: U256::ZERO,
        launch_block,
        venue_fee: WITNESSED_VENUE_FEE,
        liquidity_added: false,
    };
    let held = |read| four_held_curve(v, &FourHeld::Classic(read), 2);
    assert_eq!(
        held(classic),
        Ok(FourCurve::classic_at(
            ClassicCurve {
                k: u(f[5]),
                t: u(f[6]),
                offers: u(f[7]),
                sold: WITNESSED_MAX_OFFERS - u(f[7]),
            },
            ClassicFee {
                rate_bps: WITNESSED_VENUE_FEE.rate_bps,
                min_fee: WITNESSED_VENUE_FEE.min_fee,
                tax_bps: profile.tax_bps,
            },
        )
        .unwrap())
    );
    assert_eq!(
        held(ClassicTokenRead {
            liquidity_added: true,
            ..classic
        }),
        Err(LIVE_POSITION_OFF_CURVE)
    );
    // A SELL window past its tax's witnessed age: not priced.
    assert_eq!(
        four_held_curve(v, &FourHeld::Classic(classic), profile.tax_witnessed_blocks),
        Err(LIVE_POSITION_UNPRICED)
    );
    // OpenFour: the witnessed launch's state at a snapshot block is the
    // curve the strategy tracks there.
    let block = "131615000";
    let l = rows(OPENFOUR, "OFLAUNCH").next().unwrap();
    let s = rows(OPENFOUR, "OFSNAP").find(|s| s[1] == block).unwrap();
    let bonding = d.openfour_bonding.unwrap();
    let openfour = OpenFourTokenRead {
        token: l[4].parse().unwrap(),
        block_number: block.parse().unwrap(),
        block_hash: B256::repeat_byte(0xa1),
        preset_id: u(l[5]),
        max_supply: u(l[8]),
        sale_amount: u(l[9]),
        raise_amount: u(l[10]),
        quote_asset: l[12].parse().unwrap(),
        exists: true,
        paused: false,
        anti_sniper_enabled: false,
        custom_data: Address::ZERO,
        phase: 1,
        module_beacons: bonding.modules.map(|m| m.beacon),
        initial_quote: u(s[3]),
        initial_token: u(s[4]),
        current_quote: u(s[5]),
        current_token: u(s[6]),
        remaining: u(s[7]),
    };
    let (tracked, _, _) = crate::paper_exec::tests::openfour_strategy_state(block);
    assert_eq!(
        four_held_curve(v, &FourHeld::OpenFour(openfour), 2),
        Ok(tracked)
    );
    assert_eq!(
        four_held_curve(
            v,
            &FourHeld::OpenFour(OpenFourTokenRead {
                phase: 2,
                ..openfour
            }),
            2
        ),
        Err(LIVE_POSITION_OFF_CURVE)
    );
    assert_eq!(
        four_held_curve(
            v,
            &FourHeld::OpenFour(OpenFourTokenRead {
                paused: true,
                ..openfour
            }),
            2
        ),
        Err(LIVE_POSITION_UNPRICED)
    );
}

/// R13: the launches C3 BUYs behind are the operator's choice: SELF (the
/// shipped one) or FOREIGN_AUTO; any other value, or none, is refused.
#[test]
fn the_c3_candidates_are_self_or_foreign_auto() {
    use crate::paper_exec::C3Candidates;
    let line = CONFIG
        .lines()
        .find(|l| l.starts_with("BSC_EXEC_C3_CANDIDATES="))
        .unwrap();
    let with = |v: &str| {
        RuntimeConfig::parse(&CONFIG.replace(line, &format!("BSC_EXEC_C3_CANDIDATES={v}")))
    };
    let scope = |c: Result<RuntimeConfig, String>| c.unwrap().exec.unwrap().exec.c3_candidates;
    assert_eq!(
        scope(RuntimeConfig::parse(CONFIG)),
        C3Candidates::SelfLaunch
    );
    assert_eq!(scope(with("FOREIGN_AUTO")), C3Candidates::ForeignAuto);
    assert!(with("ALL").is_err());
    assert!(RuntimeConfig::parse(&CONFIG.replace(line, "")).is_err());
}

/// Frontend package (06.11.): Strategy C starts with the operator's
/// settings on file in place (none: the file's configuration); settings
/// the configuration no longer takes, or a store another user can change,
/// keep it off with the reason.
#[test]
fn strategy_c_starts_with_the_settings_on_file() {
    use crate::strategy_settings::Owner;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    const CONFIG: &str = include_str!("../../../../config/ingress.public.env");
    let dir =
        std::env::temp_dir().join(format!("bsc-runtime-strategy-start-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let (s, engine) = strategy_start(&config, &dir).unwrap();
    assert_eq!((s.generation, engine), (0, config.strategy.engine.clone()));
    let owner = Owner::open(&dir).unwrap();
    owner
        .set(
            "r1",
            "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
            "0.9",
            0,
            CONFIG,
            1,
        )
        .unwrap();
    let (s, engine) = strategy_start(&config, &dir).unwrap();
    assert_eq!(s.generation, 1);
    assert_eq!(engine.stop_net_multiple, 9_000);
    assert_eq!(engine.max_open, config.strategy.engine.max_open);
    // A file at 0 places: 0.9 is no multiple of it.
    let places: String = CONFIG
        .lines()
        .map(|l| match l {
            "BSC_STRATEGY_C_RATIO_PLACES=4" => "BSC_STRATEGY_C_RATIO_PLACES=0".to_string(),
            "BSC_STRATEGY_C_STOP_NET_MULTIPLE=0.5" => {
                "BSC_STRATEGY_C_STOP_NET_MULTIPLE=0".to_string()
            }
            l => l.to_string(),
        })
        .map(|l| l + "\n")
        .collect();
    let other = RuntimeConfig::parse(&places).unwrap();
    let why = strategy_start(&other, &dir).unwrap_err();
    assert!(
        why.starts_with("STRATEGY_SETTINGS_INVALID: generation 1"),
        "{why}"
    );
    let store = dir.join(crate::strategy_settings::DIR);
    fs::set_permissions(&store, fs::Permissions::from_mode(0o755)).unwrap();
    let why = strategy_start(&config, &dir).unwrap_err();
    assert!(why.starts_with("STRATEGY_SETTINGS_UNREADABLE"), "{why}");
    fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_dir_all(&dir).unwrap();
}

/// Frontend package (06.11.): a start of Strategy C journals the settings it
/// takes before its switch (`p`, then `U`), none without settings on file;
/// refused, it journals nothing and stays off with the reason.
#[test]
fn a_strategy_start_journals_its_settings_before_its_switch() {
    use crate::runtime_strategy::{StrategyVenues, switch_payload};
    use crate::strategy_settings::{Owner, Settings};
    use darkbot_bsc_journal::storage::run::RunReader;
    use std::os::unix::fs::DirBuilderExt;
    const CONFIG: &str = include_str!("../../../../config/ingress.public.env");
    let base = std::env::temp_dir().join(format!(
        "bsc-runtime-strategy-journal-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(base.join("sources"))
        .unwrap();
    let config = RuntimeConfig::parse(CONFIG).unwrap();
    let gas = config
        .strategy
        .gas(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.."))
        .unwrap();
    let records = |sources: &Path, run: u64, file: &RuntimeConfig, keep: Option<Settings>| {
        let dir = base.join(format!("journal-{run}"));
        let mut c = Canonical::new(config.canonical.as_ref().unwrap(), 0);
        let mut journal = RuntimeJournal::open(
            &dir,
            run,
            1,
            crate::runtime_journal::JournalSetup::whole(256, journal_max_payload(&config)),
        )
        .unwrap();
        start_strategy(
            &mut c,
            &mut journal,
            Clock(Instant::now()),
            file,
            sources,
            &gas,
            StrategyVenues::FLAP,
            keep,
        );
        journal.finish().unwrap();
        let run = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        let mut reader = RunReader::open(&run, journal_max_payload(&config)).unwrap();
        let mut out = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            if r.kind == RecordKind::Runtime && matches!(r.payload.first(), Some(b'p' | b'U')) {
                out.push(r.payload.to_vec());
            }
        }
        (c, out)
    };
    let sources = base.join("sources");
    let (c, out) = records(&sources, 1, &config, None);
    assert_eq!(out, [switch_payload(true, StrategyVenues::FLAP)]);
    assert!(c.strategy.is_some());
    assert_eq!(
        c.strategy_settings
            .as_ref()
            .map(|s| (s.generation, s.values.len())),
        Some((0, 9))
    );
    Owner::open(&sources)
        .unwrap()
        .set("r1", "BSC_STRATEGY_C_MAX_OPEN", "3", 0, CONFIG, 1)
        .unwrap();
    let (c, out) = records(&sources, 2, &config, None);
    let taken = Settings::of_journal(&out[0]).unwrap();
    assert_eq!(
        (taken.generation, taken.value("BSC_STRATEGY_C_MAX_OPEN")),
        (1, Some("3"))
    );
    assert_eq!(out[1..], [switch_payload(true, StrategyVenues::FLAP)]);
    assert_eq!(c.strategy_config.as_ref().map(|e| e.max_open), Some(3));
    let s = c.strategy_settings.as_ref().unwrap();
    assert_eq!(
        (s.generation, s.value("BSC_STRATEGY_C_MAX_OPEN")),
        (1, Some("3"))
    );
    // A configuration that no longer takes them: nothing journaled, off with the reason.
    let other: String = CONFIG
        .lines()
        .map(|l| match l {
            "BSC_STRATEGY_C_RATIO_PLACES=4" => "BSC_STRATEGY_C_RATIO_PLACES=0".into(),
            "BSC_STRATEGY_C_STOP_NET_MULTIPLE=0.5" => "BSC_STRATEGY_C_STOP_NET_MULTIPLE=0".into(),
            l => l.to_string(),
        })
        .map(|l| l + "\n")
        .collect();
    Owner::open(&sources)
        .unwrap()
        .set(
            "r2",
            "BSC_STRATEGY_C_STOP_NET_MULTIPLE",
            "0.9",
            1,
            CONFIG,
            2,
        )
        .unwrap();
    // Review M1: while it stays on (a restart for other venues) it keeps
    // the settings it ran with; generation 2 waits for a switch on.
    let ran = c.strategy_taken.clone();
    assert_eq!(ran.as_ref().map(|s| s.generation), Some(1));
    let (c, out) = records(&sources, 4, &config, ran);
    assert_eq!(Settings::of_journal(&out[0]).map(|s| s.generation), Some(1));
    assert_eq!(
        c.strategy_config
            .as_ref()
            .map(|e| (e.max_open, e.stop_net_multiple)),
        Some((3, config.strategy.engine.stop_net_multiple))
    );
    let (c, out) = records(&sources, 3, &RuntimeConfig::parse(&other).unwrap(), None);
    assert!(out.is_empty(), "{out:?}");
    assert!(c.strategy.is_none());
    assert!(
        matches!(&c.strategy_refused, Some(("SETTINGS_REFUSED", why)) if why.starts_with("STRATEGY_SETTINGS_INVALID"))
    );
    fs::remove_dir_all(&base).unwrap();
}
