use super::*;

/// BSC3-002/021: each owner enum's ALL lists every value at its own
/// fixed index, each index once (no value reads another's, none index
/// 0 by default).
#[test]
fn every_value_has_its_own_index() {
    fn check<T: Copy>(all: &[T], index: impl Fn(T) -> usize) {
        let mut seen = vec![false; all.len()];
        for (i, x) in all.iter().enumerate() {
            assert_eq!(index(*x), i);
            assert!(!std::mem::replace(&mut seen[i], true));
        }
    }
    check(&SourceId::ALL, SourceId::index);
    check(&ComponentId::ALL, ComponentId::index);
    check(&VenueId::ALL, VenueId::index);
    check(&SwitchId::ALL, SwitchId::index);
}

/// The owner's bounds of the shipped configuration.
pub(crate) fn shipped_limits() -> OwnerLimits {
    OwnerLimits::of_config(include_str!("../../../../../config/ingress.public.env")).unwrap()
}

/// The shipped attempts' maximum.
pub(crate) fn max() -> u32 {
    shipped_limits().buy_attempts_total_max
}

/// The shipped bounds keeping `keep` generations.
pub(crate) fn keeping(keep: u64) -> OwnerLimits {
    OwnerLimits {
        keep_generations: keep,
        ..shipped_limits()
    }
}

fn store(tag: &str) -> (PathBuf, Store) {
    let dir = std::env::temp_dir().join(format!("bsc-sources-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let s = Store::open(&dir, keeping(8)).unwrap();
    (dir, s)
}

/// PHASENPLAN §10 / V2 §5: the C2 state component runs in SHADOW, and a
/// requested LIVE is recorded but never effective.
#[test]
fn the_c2_component_is_switchable_and_never_operationally_live() {
    let (dir, s) = store("c2");
    let c2 = ComponentId::StateC2;
    assert_eq!(Entity::parse("BSC_STATE_C2"), Some(Entity::Component(c2)));
    assert_eq!(
        component_effective(c2, Mode::Off, LiveGate::Disabled),
        Effective::Off
    );
    assert_eq!(
        component_effective(c2, Mode::Shadow, LiveGate::Disabled),
        Effective::Shadow
    );
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
    assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
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

/// Phase H: Strategy C runs as a PAPER ledger in SHADOW; LIVE is
/// recorded and blocked; a generation written before it reads it OFF.
#[test]
fn the_strategy_component_is_paper_only_and_never_live() {
    let (dir, s) = store("strategy");
    let strategy = ComponentId::StrategyC;
    assert_eq!(
        Entity::parse("BSC_STRATEGY_C"),
        Some(Entity::Component(strategy))
    );
    let r = s
        .request("s-1", ComponentId::ExecC3, Mode::Shadow, 0, 1)
        .unwrap();
    assert!(r.applied());
    assert_eq!(
        s.current().unwrap().component(strategy).effective,
        Effective::Off
    );
    // A generation of the two earlier components: the strategy reads OFF.
    let (_, two) = current_record(&dir).unwrap().unwrap();
    let mut d = Draft::new(&sm::SCHEMA);
    d.set(sm::GENERATION, 2u64)
        .set(sm::PROVENANCE_TOOL, "bsc_runtime")
        .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 2u64);
    for row in two.nested_all(sm::SOURCES).unwrap() {
        let mut e = Draft::new(&sm::entry::SCHEMA);
        for tag in [
            sm::entry::SOURCE_ID,
            sm::entry::REQUESTED,
            sm::entry::EFFECTIVE,
        ] {
            e.set(tag, row.text(tag).unwrap());
        }
        d.push(sm::SOURCES, e.into_nested());
    }
    for row in two.nested_all(sm::COMPONENTS).unwrap() {
        if row.text(sm::component::COMPONENT_ID).unwrap() == "BSC_STRATEGY_C" {
            continue;
        }
        let mut e = Draft::new(&sm::component::SCHEMA);
        for tag in [
            sm::component::COMPONENT_ID,
            sm::component::REQUESTED,
            sm::component::EFFECTIVE,
        ] {
            e.set(tag, row.text(tag).unwrap());
        }
        d.push(sm::COMPONENTS, e.into_nested());
    }
    fs::write(
        dir.join("generations").join(format!("{:020}.dbrec", 2)),
        d.encode().unwrap(),
    )
    .unwrap();
    let g = s.current().unwrap();
    assert_eq!(g.generation, 2);
    assert_eq!(
        g.component(ComponentId::ExecC3).effective,
        Effective::Shadow
    );
    assert_eq!(g.component(strategy).effective, Effective::Off);
    let r = s.request("s-2", strategy, Mode::Shadow, 2, 3).unwrap();
    assert_eq!(r.effective, Effective::Shadow);
    let r = s.request("s-3", strategy, Mode::Live, 3, 4).unwrap();
    assert!(r.applied());
    assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
    assert!(
        r.line()
            .contains("source=BSC_STRATEGY_C requested=LIVE effective=BLOCKED")
    );
    assert_eq!(
        s.current()
            .unwrap()
            .component(ComponentId::ExecC3)
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
        assert_eq!(effective(s, Mode::Off, LiveGate::Disabled), Effective::Off);
        assert_eq!(
            effective(s, Mode::Live, LiveGate::Disabled),
            Effective::Blocked(ECONOMIC_LIVE_DISABLED)
        );
        assert_eq!(
            effective(s, Mode::Live, LiveGate::Enabled),
            Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
        );
    }
    assert_eq!(
        effective(SourceId::PublicNode, Mode::Shadow, LiveGate::Disabled),
        Effective::Shadow
    );
    assert_eq!(
        effective(SourceId::Club48, Mode::Shadow, LiveGate::Disabled),
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
    assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
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
    let again = Store::open(&dir, keeping(8)).unwrap();
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

/// BSC3-007: the account home is the real user's: the first `Uid:` field
/// (not the effective second), the passwd record of exactly that UID
/// (none: refused), a record without a shell counts.
#[test]
fn the_account_home_is_the_real_users() {
    let status = "Name:\tbsc\nUid:\t1000\t0\t0\t0\nGid:\t1000\t1000\t1000\t1000\n";
    assert_eq!(real_uid_of_status(status), Some("1000"));
    assert_eq!(real_uid_of_status("Name:\tbsc\n"), None);
    let passwd = "root:x:0:0:root:/root:/bin/sh\nbsc:x:1000:1000::/home/bsc:\n";
    assert_eq!(home_of_passwd(passwd, "1000"), Some("/home/bsc"));
    assert_eq!(home_of_passwd(passwd, "0"), Some("/root"));
    assert_eq!(home_of_passwd(passwd, "1001"), None);
    assert_eq!(home_of_passwd("bsc:x:1000:1000::/home/bsc\n", "1000"), None);
    assert_eq!(
        home_of_dscache("name: bsc\npassword: *\nuid: 501\ndir: /Users/bsc\n"),
        Some("/Users/bsc")
    );
    assert_eq!(home_of_dscache(""), None);
    // This host's account home: an absolute directory of the real user.
    let home = account_home().unwrap();
    assert!(home.is_absolute() && home.is_dir(), "{}", home.display());
}

/// BSC3-003: the owner's state is taken only from owner-only directories
/// on a way no other user can change, and its files only as private
/// files of that owner read through their own name.
#[test]
fn an_exposed_or_linked_store_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let base = std::env::temp_dir().join(format!("bsc-store-exposed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&base)
        .unwrap();
    let dir = base.join("bsc-sources");
    let s = Store::open_in_session(&dir, keeping(8), Ok("boot-1".into())).unwrap();
    s.request("a", SwitchId::Sign, Mode::Enabled, 0, 1).unwrap();
    assert_eq!(require_enabled(&dir, "boot-1", &[SwitchId::Sign]), Ok(1));
    let mode = |p: &Path, m: u32| fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
    // An existing directory others may read, or a way others may change:
    // no store opens on it; a switch is required only from state no
    // other user can change.
    for (p, m, required) in [
        (dir.clone(), 0o755, true),
        (dir.join("generations"), 0o750, false),
        (base.clone(), 0o777, false),
    ] {
        mode(&p, m);
        let e = Store::open_in_session(&dir, keeping(8), Ok("boot-1".into()))
            .err()
            .unwrap();
        assert!(
            e.contains("accessible to others") || e.contains("changed by another user"),
            "{e}"
        );
        let r = require_enabled(&dir, "boot-1", &[SwitchId::Sign]);
        match required {
            true => assert_eq!(r, Ok(1)),
            false => assert!(r.unwrap_err().contains("CONTROL_STATE_UNREADABLE")),
        }
        mode(&p, 0o700);
    }
    mode(&base, 0o1777);
    assert!(Store::open_in_session(&dir, keeping(8), Ok("boot-1".into())).is_ok());
    mode(&base, 0o700);
    // The current generation writable by others, or a link to another file.
    let current = s.generation_path(1);
    mode(&current, 0o666);
    assert!(s.current().unwrap_err().contains("not a private file"));
    mode(&current, 0o600);
    let elsewhere = base.join("elsewhere.dbrec");
    fs::copy(&current, &elsewhere).unwrap();
    fs::remove_file(&current).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &current).unwrap();
    assert!(s.current().unwrap_err().contains("not a private file"));
    assert!(current_record_and_generation(&dir).is_err());
    fs::remove_file(&current).unwrap();
    fs::rename(&elsewhere, &current).unwrap();
    assert_eq!(s.current().unwrap().generation, 1);
    // A receipt that is a link: no answer from it.
    let receipt = dir.join("receipts/a.dbrec");
    let copy = base.join("receipt.dbrec");
    fs::copy(&receipt, &copy).unwrap();
    fs::remove_file(&receipt).unwrap();
    std::os::unix::fs::symlink(&copy, &receipt).unwrap();
    assert!(s.request("a", SwitchId::Sign, Mode::Enabled, 0, 2).is_err());
    fs::remove_dir_all(base).unwrap();
}

/// BSC3-006: a lost receipt is recovered from the generation its request
/// wrote, also after later requests moved the generation on; another
/// payload under its ID is refused; a request that met a generation no
/// longer kept is UNRESOLVED, a stale one REJECTED_STALE.
#[test]
fn a_lost_receipt_is_recovered_after_later_generations() {
    let (dir, s) = store("crash-later");
    s.request("a", SourceId::Geth, Mode::Shadow, 0, 1).unwrap();
    fs::remove_file(dir.join("receipts/a.dbrec")).unwrap();
    s.request("b", SourceId::Drpc, Mode::Shadow, 1, 2).unwrap();
    let r = s.request("a", SourceId::Geth, Mode::Shadow, 0, 3).unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
    assert_eq!(r.effective, Effective::Shadow);
    // The same again: its receipt now.
    assert_eq!(
        s.request("a", SourceId::Geth, Mode::Shadow, 0, 4).unwrap(),
        r
    );
    // Another payload under a lost receipt's ID is no recovery.
    s.request("c", SourceId::Geth, Mode::Off, 2, 5).unwrap();
    fs::remove_file(dir.join("receipts/c.dbrec")).unwrap();
    s.request("d", SourceId::Drpc, Mode::Off, 3, 6).unwrap();
    assert!(
        s.request("c", SourceId::Geth, Mode::Shadow, 2, 7)
            .unwrap_err()
            .contains("another request")
    );
    // A request that met generation 1 but never wrote: stale.
    let stale = s.request("e", SourceId::Geth, Mode::Off, 1, 8).unwrap();
    assert_eq!(stale.outcome, "REJECTED_STALE");
    // The generation it would have written no longer kept: unresolved.
    fs::remove_file(s.generation_path(1)).unwrap();
    fs::remove_file(dir.join("receipts/a.dbrec")).unwrap();
    let unknown = s.request("a", SourceId::Geth, Mode::Shadow, 0, 9).unwrap();
    assert_eq!(unknown.outcome, "UNRESOLVED");
    assert!(!unknown.applied());
    fs::remove_dir_all(dir).unwrap();
}

pub(crate) fn session(tag: &str, boot: &str) -> (PathBuf, Store) {
    let dir = std::env::temp_dir().join(format!("bsc-switch-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let s = Store::open_in_session(&dir, keeping(8), Ok(boot.into())).unwrap();
    (dir, s)
}

/// Messplan §3b C10: the six switches the G0.7 runner requires before
/// it signs, consumes its authority and sends (both wallets, SIGN, ARM,
/// SEND, ATTEMPT; `g07_no_send` pins that set and the three checks):
/// all six ENABLED pass; each one alone DISABLED refuses, naming it.
#[test]
fn each_of_the_six_attempt_switches_alone_refuses_the_attempt() {
    let ids = [
        SwitchId::WalletLauncher,
        SwitchId::WalletBuyer,
        SwitchId::Sign,
        SwitchId::Arm,
        SwitchId::Send,
        SwitchId::Attempt,
    ];
    for off in 0..ids.len() {
        let (dir, s) = session(&format!("c10-{off}"), "boot-1");
        for (i, id) in ids.iter().enumerate() {
            s.request(&format!("on-{i}"), *id, Mode::Enabled, i as u64, 1)
                .unwrap();
        }
        assert!(require_enabled(&dir, "boot-1", &ids).is_ok());
        s.request("off", ids[off], Mode::Disabled, ids.len() as u64, 1)
            .unwrap();
        let e = require_enabled(&dir, "boot-1", &ids).unwrap_err();
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(
                e.contains(&format!("{} DISABLED", id.name())),
                i == off,
                "{e}"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
}

/// Messplan §3b C02–C07 and the sub-combination: the isolated SIGN guard
/// over LAUNCHER, BUYER and SIGN passes for exactly one of the eight
/// settings and names every switch that is off; from all three ENABLED,
/// any one disabled again refuses it.
#[test]
fn the_sign_guard_holds_for_exactly_one_of_eight_settings() {
    let ids = [
        SwitchId::WalletLauncher,
        SwitchId::WalletBuyer,
        SwitchId::Sign,
    ];
    let named = |e: &str, id: SwitchId| e.contains(&format!("{} DISABLED", id.name()));
    for bits in 0u8..8 {
        let (dir, s) = session(&format!("lbs-{bits}"), "boot-1");
        let mut generation = 0;
        for (i, id) in ids.iter().enumerate() {
            if bits & (1 << i) != 0 {
                s.request(&format!("on-{i}"), *id, Mode::Enabled, generation, 1)
                    .unwrap();
                generation += 1;
            }
        }
        match require_enabled(&dir, "boot-1", &ids) {
            Ok(_) => assert_eq!(bits, 7),
            Err(e) => {
                assert_ne!(bits, 7, "{e}");
                for (i, id) in ids.iter().enumerate() {
                    assert_eq!(named(&e, *id), bits & (1 << i) == 0, "{bits:03b}: {e}");
                }
            }
        }
        if bits == 7 {
            for (i, id) in ids.iter().enumerate() {
                s.request(&format!("off-{i}"), *id, Mode::Disabled, generation, 1)
                    .unwrap();
                let e = require_enabled(&dir, "boot-1", &ids).unwrap_err();
                assert!(named(&e, *id), "{e}");
                s.request(&format!("again-{i}"), *id, Mode::Enabled, generation + 1, 1)
                    .unwrap();
                generation += 2;
                assert!(require_enabled(&dir, "boot-1", &ids).is_ok());
            }
        }
        fs::remove_dir_all(dir).unwrap();
    }
}

/// Plan 27.10.: LAUNCHER and BUYER are independent switches, DISABLED
/// initially, taking only DISABLED or ENABLED; each records the request
/// that set it.
#[test]
fn the_wallet_switches_are_independent_and_disabled_initially() {
    let (dir, s) = session("wallets", "boot-1");
    let g = s.current().unwrap();
    for id in SwitchId::ALL {
        assert_eq!(g.switch(id).requested, Mode::Disabled);
        assert_eq!(g.switch(id).effective, Effective::Disabled);
    }
    for m in [Mode::Off, Mode::Shadow, Mode::Live] {
        let e = s
            .request("w-bad", SwitchId::WalletLauncher, m, 0, 1)
            .unwrap_err();
        assert!(e.contains("takes DISABLED or ENABLED"), "{e}");
    }
    assert!(
        s.request("w-bad2", SourceId::Drpc, Mode::Enabled, 0, 1)
            .unwrap_err()
            .contains("takes OFF, SHADOW or LIVE")
    );
    let r = s
        .request("w-1", SwitchId::WalletLauncher, Mode::Enabled, 0, 1)
        .unwrap();
    assert_eq!((r.outcome, r.effective), ("APPLIED", Effective::Enabled));
    assert!(
        r.line().contains(
            "source=BSC_WALLET_LAUNCHER requested=ENABLED effective=ENABLED generation=1"
        )
    );
    let g = s.current().unwrap();
    assert_eq!(
        g.switch(SwitchId::WalletLauncher).set_by,
        Some(SetBy {
            request_id: "w-1".into(),
            generation: 1
        })
    );
    assert_eq!(
        g.switch(SwitchId::WalletBuyer).effective,
        Effective::Disabled
    );
    assert_eq!(g.boot.as_deref(), Some("boot-1"));
    assert_eq!(
        require_enabled(&dir, "boot-1", &[SwitchId::WalletLauncher]),
        Ok(1)
    );
    let refused = require_enabled(
        &dir,
        "boot-1",
        &[SwitchId::WalletLauncher, SwitchId::WalletBuyer],
    )
    .unwrap_err();
    assert!(
        refused.contains("BSC_WALLET_BUYER DISABLED") && !refused.contains("BSC_WALLET_LAUNCHER"),
        "{refused}"
    );
    let r = s
        .request("w-2", SwitchId::WalletLauncher, Mode::Disabled, 1, 2)
        .unwrap();
    assert_eq!(r.effective, Effective::Disabled);
    assert!(require_enabled(&dir, "boot-1", &[SwitchId::WalletLauncher]).is_err());
    fs::remove_dir_all(dir).unwrap();
}

/// An ENABLED wallet or economic switch makes nothing else effective:
/// a requested LIVE stays BLOCKED; the global LIVE switch only names
/// the next missing condition.
#[test]
fn a_switch_enables_nothing_else() {
    let (dir, s) = session("nothing-else", "boot-1");
    let mut g = 0;
    for id in SwitchId::ALL {
        if id == SwitchId::Live {
            continue;
        }
        s.request(&format!("e-{g}"), id, Mode::Enabled, g, 1)
            .unwrap();
        g += 1;
    }
    let r = s
        .request("exec-live", ComponentId::ExecC3, Mode::Live, g, 2)
        .unwrap();
    assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
    let r = s
        .request("flap-live", VenueId::Flap, Mode::Live, g + 1, 3)
        .unwrap();
    assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
    let now = s.current().unwrap();
    assert!(now.sources.iter().all(|m| m.effective == Effective::Off));
    s.request("live-on", SwitchId::Live, Mode::Enabled, g + 2, 4)
        .unwrap();
    let now = s.current().unwrap();
    assert_eq!(
        now.component(ComponentId::ExecC3).effective,
        Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
    );
    assert_eq!(
        now.venue(VenueId::Flap).effective,
        Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED)
    );
    // The receipt of the earlier LIVE request keeps the reason it met.
    let again = s
        .request("exec-live", ComponentId::ExecC3, Mode::Live, g, 5)
        .unwrap();
    assert_eq!(again.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
    s.request("live-off", SwitchId::Live, Mode::Disabled, g + 3, 6)
        .unwrap();
    assert_eq!(
        s.current()
            .unwrap()
            .component(ComponentId::ExecC3)
            .effective,
        Effective::Blocked(ECONOMIC_LIVE_DISABLED)
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Review L5: after a reboot nobody restarted the owner for, a requested
/// LIVE names the LIVE switch as it holds in this boot, not as written.
#[test]
fn a_live_reason_reads_the_live_switch_of_this_boot() {
    let (dir, s) = session("live-boot", "boot-1");
    s.request("l-on", SwitchId::Live, Mode::Enabled, 0, 1)
        .unwrap();
    s.request("v-live", VenueId::Flap, Mode::Live, 1, 2)
        .unwrap();
    s.request("r-live", Entity::Runtime, Mode::Live, 2, 3)
        .unwrap();
    let g = s.current().unwrap();
    let written = Effective::Blocked(ECONOMIC_LIVE_NOT_AUTHORIZED);
    for e in [Entity::Venue(VenueId::Flap), Entity::Runtime] {
        assert_eq!(g.effective_of(e), written);
        assert_eq!(g.effective_now(e, Some("boot-1")), written);
        for boot in [Some("boot-2"), None] {
            assert_eq!(
                g.effective_now(e, boot),
                Effective::Blocked(ECONOMIC_LIVE_DISABLED)
            );
        }
    }
    assert_eq!(
        g.effective_now(Entity::Switch(SwitchId::Live), Some("boot-2")),
        Effective::Blocked(RESET_BY_RESTART)
    );
    assert_eq!(
        g.live_reason_now(FOUR_EXECUTION_PATH_MISSING, Some("boot-2")),
        FOUR_EXECUTION_PATH_MISSING
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Review pass 3 L3a: a lost receipt replayed as a source or component
/// request the generation happens to hold is not that request's change
/// when the request set something else; the genuine replay still is.
#[test]
fn a_replay_as_an_unchanged_source_is_not_the_request() {
    let (dir, s) = session("replay-source", "boot-1");
    s.request("x", SwitchId::Arm, Mode::Enabled, 0, 1).unwrap();
    fs::remove_file(dir.join("receipts/x.dbrec")).unwrap();
    let e = s.request("x", SourceId::Drpc, Mode::Off, 0, 2).unwrap_err();
    assert!(e.contains("belongs to another request"), "{e}");
    let e = s
        .request("x", ComponentId::ExecC3, Mode::Off, 0, 3)
        .unwrap_err();
    assert!(e.contains("belongs to another request"), "{e}");
    let genuine = s.request("x", SwitchId::Arm, Mode::Enabled, 0, 4).unwrap();
    assert_eq!((genuine.outcome, genuine.generation), ("APPLIED", 1));
    s.request("y", SourceId::Drpc, Mode::Shadow, 1, 5).unwrap();
    fs::remove_file(dir.join("receipts/y.dbrec")).unwrap();
    let again = s.request("y", SourceId::Drpc, Mode::Shadow, 1, 6).unwrap();
    assert_eq!((again.outcome, again.generation), ("APPLIED", 2));
    fs::remove_dir_all(dir).unwrap();
}

/// Plan v9 §2.3: Flap, (R12c) Four.meme Classic and (R12d) OpenFour are
/// admitted in SHADOW; a requested LIVE of a Four venue is stored BLOCKED
/// for its missing LIVE path (the receipt the operator sees), the LIVE
/// switch off or not.
#[test]
fn the_venues_are_switchable_in_shadow() {
    let (dir, s) = session("venues", "boot-1");
    assert_eq!(
        Entity::parse("BSC_VENUE_FLAP"),
        Some(Entity::Venue(VenueId::Flap))
    );
    let r = s.request("v-1", VenueId::Flap, Mode::Shadow, 0, 1).unwrap();
    assert_eq!(r.effective, Effective::Shadow);
    // R12c/R12d: both Four venues have their C3 PAPER path.
    for (i, v, effective) in [
        (0, VenueId::FourClassic, Effective::Shadow),
        (1, VenueId::OpenFour, Effective::Shadow),
    ] {
        let r = s
            .request(&format!("v-four-{i}"), v, Mode::Shadow, 1 + i as u64, 2)
            .unwrap();
        assert_eq!(r.effective, effective, "{v:?}");
    }
    let g = s.current().unwrap();
    assert_eq!(g.venue(VenueId::Flap).effective, Effective::Shadow);
    assert_eq!(
        g.venue(VenueId::OpenFour).set_by,
        Some(SetBy {
            request_id: "v-four-1".into(),
            generation: 3
        })
    );
    for (i, v) in [VenueId::FourClassic, VenueId::OpenFour]
        .into_iter()
        .enumerate()
    {
        let r = s
            .request(&format!("v-live-{i}"), v, Mode::Live, 3 + i as u64, 2)
            .unwrap();
        // SC4 d: a Four.meme venue's LIVE is the LIVE gate's, as Flap's.
        assert_eq!(
            (r.outcome, r.effective),
            ("APPLIED", Effective::Blocked(ECONOMIC_LIVE_DISABLED)),
            "{v:?}"
        );
    }
    // The Flap venue's LIVE is the LIVE gate's (the switch is off).
    let r = s
        .request("v-live-flap", VenueId::Flap, Mode::Live, 5, 2)
        .unwrap();
    assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
    fs::remove_dir_all(dir).unwrap();
}

/// Plan 27.10.: every restart disables the switches; sources, components
/// and venues keep their modes; a restart with nothing ENABLED writes
/// nothing.
#[test]
fn a_restart_disables_every_switch_and_keeps_the_modes() {
    let (dir, s) = session("restart", "boot-1");
    s.request("r-1", SourceId::PublicNode, Mode::Shadow, 0, 1)
        .unwrap();
    s.request("r-2", SwitchId::WalletBuyer, Mode::Enabled, 1, 2)
        .unwrap();
    s.request("r-3", SwitchId::Send, Mode::Enabled, 2, 3)
        .unwrap();
    let g = s.restart(4).unwrap();
    assert_eq!(g.generation, 4);
    assert_eq!(g.request_id.as_deref(), Some(RESTART_RESET));
    assert!(!g.any_enabled());
    assert_eq!(
        g.switch(SwitchId::WalletBuyer).set_by,
        Some(SetBy {
            request_id: RESTART_RESET.into(),
            generation: 4
        })
    );
    assert_eq!(g.source(SourceId::PublicNode).effective, Effective::Shadow);
    assert_eq!(s.restart(5).unwrap().generation, 4, "nothing to reset");
    // A stale request from before the restart changes nothing.
    let stale = s
        .request("r-old", SwitchId::WalletBuyer, Mode::Enabled, 3, 6)
        .unwrap();
    assert_eq!(stale.outcome, "REJECTED_STALE");
    assert!(require_enabled(&dir, "boot-1", &[SwitchId::WalletBuyer]).is_err());
    // A first start writes generation 0 and nothing else.
    let (fresh, f) = session("restart-fresh", "boot-1");
    assert_eq!(f.restart(1).unwrap(), Generation::initial());
    assert!(current_record(&fresh).unwrap().is_some());
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(fresh).unwrap();
}

/// An ENABLED switch counts only in the boot session it was written in:
/// after a reboot nobody restarted the owner for, it is refused, and the
/// next write carries it over DISABLED.
#[test]
fn a_switch_never_outlives_its_boot_session() {
    let (dir, s) = session("boot", "boot-1");
    s.request("b-1", SwitchId::WalletLauncher, Mode::Enabled, 0, 1)
        .unwrap();
    assert_eq!(
        require_enabled(&dir, "boot-1", &[SwitchId::WalletLauncher]),
        Ok(1)
    );
    let e = require_enabled(&dir, "boot-2", &[SwitchId::WalletLauncher]).unwrap_err();
    assert!(e.contains("BLOCKED (RESET_BY_RESTART)"), "{e}");
    assert_eq!(
        s.current()
            .unwrap()
            .switch_now(SwitchId::WalletLauncher, None),
        Effective::Blocked(RESET_BY_RESTART)
    );
    let after = Store::open_in_session(&dir, keeping(8), Ok("boot-2".into())).unwrap();
    after
        .request("b-2", SourceId::Drpc, Mode::Shadow, 1, 2)
        .unwrap();
    let g = after.current().unwrap();
    assert_eq!(
        g.switch(SwitchId::WalletLauncher).effective,
        Effective::Disabled
    );
    assert_eq!(g.boot.as_deref(), Some("boot-2"));
    assert!(require_enabled(&dir, "boot-2", &[SwitchId::WalletLauncher]).is_err());
    // Without a boot session nothing can be written.
    let blind = Store::open_in_session(&dir, keeping(8), Err("no boot".into())).unwrap();
    assert!(
        blind
            .request("b-3", SwitchId::WalletLauncher, Mode::Enabled, 2, 3)
            .is_err()
    );
    fs::remove_dir_all(dir).unwrap();
}

/// A missing state is everything DISABLED; an unreadable or tampered one
/// refuses every switch.
#[test]
fn a_missing_unreadable_or_tampered_state_refuses_every_switch() {
    let missing = std::env::temp_dir().join(format!("bsc-switch-none-{}", std::process::id()));
    let _ = fs::remove_dir_all(&missing);
    let e = require_enabled(&missing, "boot-1", &[SwitchId::Sign]).unwrap_err();
    assert!(e.contains("BSC_ECON_SIGN DISABLED"), "{e}");
    assert!(!missing.exists(), "a check creates nothing");
    let (dir, s) = session("tampered", "boot-1");
    s.request("t-1", SwitchId::Sign, Mode::Enabled, 0, 1)
        .unwrap();
    // Stored ENABLED for a requested DISABLED: not the owner's.
    let mut g = s.current().unwrap();
    g.generation = 2;
    g.switches[2].requested = Mode::Disabled;
    let mut bytes = g.encode(2).unwrap();
    fs::write(dir.join(format!("generations/{:020}.dbrec", 2)), &bytes).unwrap();
    let e = require_enabled(&dir, "boot-1", &[SwitchId::Sign]).unwrap_err();
    assert!(e.starts_with("CONTROL_STATE_UNREADABLE"), "{e}");
    assert!(e.contains("BSC_ECON_SIGN: stored effective state"), "{e}");
    // A torn file refuses as well.
    bytes.truncate(bytes.len() / 2);
    fs::write(dir.join(format!("generations/{:020}.dbrec", 2)), &bytes).unwrap();
    assert!(
        require_enabled(&dir, "boot-1", &[SwitchId::Sign])
            .unwrap_err()
            .starts_with("CONTROL_STATE_UNREADABLE")
    );
    assert!(
        s.request("t-2", SwitchId::Sign, Mode::Enabled, 2, 3)
            .is_err()
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Concurrent switch requests and a restart reset race on the CAS: one
/// writer per generation, and a reset is never lost.
#[test]
fn switch_requests_and_a_restart_race_on_the_generation() {
    let (dir, s) = session("switch-race", "boot-1");
    let outcomes: Vec<&str> = std::thread::scope(|scope| {
        let runs: Vec<_> = (0..6)
            .map(|i| {
                let s = &s;
                scope.spawn(move || {
                    s.request(
                        &format!("sw{i}"),
                        SwitchId::WalletBuyer,
                        Mode::Enabled,
                        0,
                        1,
                    )
                    .unwrap()
                    .outcome
                })
            })
            .collect();
        runs.into_iter().map(|r| r.join().unwrap()).collect()
    });
    assert_eq!(outcomes.iter().filter(|o| **o == "APPLIED").count(), 1);
    std::thread::scope(|scope| {
        let s = &s;
        let reset = scope.spawn(move || s.restart(2).unwrap());
        let enable = scope.spawn(move || {
            s.request("sw-late", SwitchId::WalletLauncher, Mode::Enabled, 1, 2)
                .unwrap()
        });
        reset.join().unwrap();
        enable.join().unwrap();
    });
    let g = s.current().unwrap();
    // The reset ran after (or instead of) every ENABLE it met.
    assert!(
        !g.switch(SwitchId::WalletBuyer)
            .effective
            .eq(&Effective::Enabled)
    );
    if g.switch(SwitchId::WalletLauncher).effective == Effective::Enabled {
        assert_eq!(g.request_id.as_deref(), Some("sw-late"));
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Plan v9 §2.5: the global runtime mode, OFF initially; SHADOW and
/// PAPER effective as requested, LIVE BLOCKED like every LIVE; a
/// generation from before it reads OFF with a daily BUY limit of 0.
#[test]
fn the_runtime_mode_is_off_initially_and_never_live() {
    let (dir, s) = session("runtime", "boot-1");
    let g = s.current().unwrap();
    assert_eq!(
        (g.runtime.effective, g.buy_attempts_per_day(max())),
        (Effective::Off, 0)
    );
    assert_eq!(Entity::parse(RUNTIME_MODE), Some(Entity::Runtime));
    assert!(
        s.request("rt-bad", Entity::Runtime, Mode::Enabled, 0, 1)
            .unwrap_err()
            .contains("takes OFF, SHADOW, PAPER or LIVE")
    );
    let r = s
        .request("rt-1", Entity::Runtime, Mode::Paper, 0, 1)
        .unwrap();
    assert_eq!(r.effective, Effective::Paper);
    assert!(
        r.line()
            .contains("source=BSC_RUNTIME_MODE requested=PAPER effective=PAPER generation=1")
    );
    let r = s
        .request("rt-2", Entity::Runtime, Mode::Live, 1, 2)
        .unwrap();
    assert_eq!(r.effective, Effective::Blocked(ECONOMIC_LIVE_DISABLED));
    // Survives a restart (a mode, not a permission).
    assert_eq!(s.restart(3).unwrap().runtime.requested, Mode::Live);
    // Stored decode: a v3 generation (before settings) reads OFF / 0.
    let mut d = Draft::new(&sm::SCHEMA_V3);
    d.set(sm::GENERATION, 3u64)
        .set(sm::PROVENANCE_TOOL, "bsc_runtime")
        .set(sm::PROVENANCE_OBSERVED_UNIX_MS, 4u64);
    for source in SourceId::ALL {
        let mut e = Draft::new(&sm::entry::SCHEMA);
        e.set(sm::entry::SOURCE_ID, source.name())
            .set(sm::entry::REQUESTED, "OFF")
            .set(sm::entry::EFFECTIVE, "OFF");
        d.push(sm::SOURCES, e.into_nested());
    }
    fs::write(
        dir.join(format!("generations/{:020}.dbrec", 3)),
        d.encode().unwrap(),
    )
    .unwrap();
    let g = s.current().unwrap();
    assert_eq!(
        (g.runtime.effective, g.buy_attempts_per_day(max())),
        (Effective::Off, 0)
    );
    fs::remove_dir_all(dir).unwrap();
}

/// The contract's day is the UTC calendar day.
#[test]
fn the_buy_day_is_the_utc_calendar_day() {
    for (ms, day) in [
        (0, "1970-01-01"),
        (951_782_400_000, "2000-02-29"),
        (1_709_164_800_000, "2024-02-29"),
        (1_709_251_199_999, "2024-02-29"),
        (1_793_059_200_000, "2026-10-27"),
    ] {
        assert_eq!(BuyDay::at(ms).utc, day, "{ms}");
    }
    assert_eq!(BuyDay::at(DAY_MS - 1).index, 0);
    assert_eq!(BuyDay::at(DAY_MS).index, 1);
}

/// Plan 27.10. (8b4721bb): the persisted daily BUY limit, 0 initially,
/// counts every attempt once per UTC day: refused when used up, the
/// same attempt again keeps its slot, a new day starts empty, a restart
/// keeps both the limit and the count, and decimal requests only.
#[test]
fn the_daily_buy_limit_counts_every_attempt_once_per_utc_day() {
    let (dir, s) = session("buy-limit", "boot-1");
    let now = 1_793_059_200_000;
    let e = s.consume_buy_attempt("a1", now).unwrap_err();
    assert!(
        e.starts_with("BUY_ATTEMPT_LIMIT_REACHED: 0 of 0 BUY attempts on 2026-10-27 (UTC)"),
        "{e}"
    );
    for bad in ["-1", "01", "x", "4294967296", ""] {
        assert_eq!(Mode::parse(bad), None, "{bad}");
    }
    assert!(
        s.request("bl-bad", Entity::BuyLimit, Mode::Enabled, 0, 1)
            .is_err()
    );
    // The campaign's total leaves room; the daily limit is what binds.
    s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
        .unwrap();
    let r = s
        .request("bl-1", Entity::BuyLimit, Mode::parse("2").unwrap(), 1, 1)
        .unwrap();
    assert!(
        r.line()
            .contains("source=BSC_BUY_ATTEMPTS_PER_DAY requested=2 effective=2")
    );
    assert_eq!(s.consume_buy_attempt("a1", now).unwrap().slot, 0);
    assert_eq!(
        s.consume_buy_attempt("a1", now + 5).unwrap().slot,
        0,
        "retry keeps its slot"
    );
    assert_eq!(s.consume_buy_attempt("a2", now + 6).unwrap().slot, 1);
    let e = s.consume_buy_attempt("a3", now + 7).unwrap_err();
    assert!(e.starts_with("BUY_ATTEMPT_LIMIT_REACHED: 2 of 2"), "{e}");
    let usage = buy_usage(&dir, now, max()).unwrap();
    assert_eq!((usage.used, usage.limit, usage.remaining()), (2, 2, 0));
    // The next UTC day starts empty.
    assert_eq!(s.consume_buy_attempt("a3", now + DAY_MS).unwrap().slot, 0);
    // A restart keeps the limit and the count.
    let again = Store::open_in_session(&dir, keeping(8), Ok("boot-2".into())).unwrap();
    again.restart(now + 8).unwrap();
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 2);
    assert_eq!(again.current().unwrap().buy_attempts_per_day(max()), 2);
    // A lower limit leaves nothing remaining, never a negative count.
    let g = again.current().unwrap().generation;
    again
        .request("bl-2", Entity::BuyLimit, Mode::PerDay(1), g, now + 9)
        .unwrap();
    let usage = buy_usage(&dir, now, max()).unwrap();
    assert_eq!((usage.used, usage.limit, usage.remaining()), (2, 1, 0));
    assert!(again.consume_buy_attempt("a4", now + 10).is_err());
    assert!(s.consume_buy_attempt("../x", now).is_err());
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-004: a day never allows more than the campaign; a clock set back
/// takes no slot; an attempt tried again after midnight UTC is the same
/// attempt (no second daily slot); a limit lowered between its read and
/// the slot's write holds.
#[test]
fn the_daily_limit_holds_across_days_clocks_and_changes() {
    const DAY: u64 = 86_400_000;
    let (dir, s) = session("buy-days", "boot-1");
    assert!(
        s.request("bl-11", Entity::BuyLimit, Mode::PerDay(11), 0, 1)
            .is_err()
    );
    s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
        .unwrap();
    s.request("bl", Entity::BuyLimit, Mode::PerDay(10), 1, 1)
        .unwrap();
    let (day1, _) = s.consume_attempt("a", 0, DAY + 5).unwrap();
    // The same attempt after midnight: its slot of the first day.
    let (again, total) = s.consume_attempt("a", 0, 2 * DAY + 5).unwrap();
    assert_eq!((again.day, again.slot), (day1.day.clone(), day1.slot));
    assert_eq!(total.slot, 0);
    assert_eq!(buy_usage(&dir, 2 * DAY + 5, max()).unwrap().used, 0);
    // A clock set back before the last attempt: refused, nothing taken.
    let e = s.consume_attempt("b", 0, DAY + 4).unwrap_err();
    assert!(e.contains("BUY_ATTEMPT_CLOCK_BEHIND"), "{e}");
    assert_eq!(total_usage(&dir, max()).unwrap().used, 1);
    // The daily limit lowered to 1 after it was read for "c": c's slot
    // 1 is taken (counted) but refused.
    let g = s.current().unwrap().generation;
    let at = dir.clone();
    let lower = move || {
        let owner = Store::open_in_session(&at, keeping(8), Ok("boot-1".into())).unwrap();
        owner
            .request("bl-1", Entity::BuyLimit, Mode::PerDay(1), g, DAY + 6)
            .unwrap();
    };
    BEFORE_SLOT_WRITE.with(|h| *h.borrow_mut() = Some(Box::new(lower)));
    let e = s.consume_attempt("c", 0, DAY + 7).unwrap_err();
    BEFORE_SLOT_WRITE.with(|h| *h.borrow_mut() = None);
    assert!(e.contains("limit fell to 1"), "{e}");
    assert_eq!(buy_usage(&dir, DAY + 7, max()).unwrap().used, 2);
    // Review M2: tried again, its slot is still beyond the limit.
    let e = s.consume_attempt("c", 0, DAY + 8).unwrap_err();
    assert!(e.contains("beyond the limit"), "{e}");
    // Review M1: a daily limit stored above the campaign's (an older
    // build took any) still decodes; the limit in force is capped.
    let mut stored = s.current().unwrap();
    stored.set(
        Entity::BuyLimit,
        Mode::PerDay(20),
        SetBy {
            request_id: "old".into(),
            generation: stored.generation,
        },
    );
    let back = Generation::decode(&stored.encode(1).unwrap()).unwrap();
    assert_eq!(back.buy_attempts_per_day(max()), max());
    // Audit E 3.1: a lowered maximum caps it.
    assert_eq!(back.buy_attempts_per_day(3), 3);
    // Review L4: a stray file among the days blocks nothing.
    fs::write(dir.join("buy-attempts/.DS_Store"), b"x").unwrap();
    assert!(s.consume_attempt("a", 0, DAY + 9).is_ok());
    fs::remove_dir_all(dir).unwrap();
}

/// Concurrent attempts race for the slots: never more than the limit.
#[test]
fn concurrent_buy_attempts_never_exceed_the_limit() {
    let (dir, s) = session("buy-race", "boot-1");
    s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
        .unwrap();
    s.request("bl", Entity::BuyLimit, Mode::PerDay(3), 1, 1)
        .unwrap();
    let ok = std::thread::scope(|scope| {
        let runs: Vec<_> = (0..8)
            .map(|i| {
                let s = &s;
                scope.spawn(move || s.consume_buy_attempt(&format!("r{i}"), 1).is_ok())
            })
            .collect();
        runs.into_iter()
            .map(|r| r.join().unwrap())
            .filter(|ok| *ok)
            .count()
    });
    assert_eq!(ok, 3);
    assert_eq!(buy_usage(&dir, 1, max()).unwrap().used, 3);
    fs::remove_dir_all(dir).unwrap();
}

/// A LIVE grant of Flap / C3 from PublicNode through 48Club.
pub(crate) fn grant(expires: u64) -> LiveGrant {
    grant_of(expires, LiveStrategies::C3)
}

/// The buyer and chain of the LIVE intents of these tests.
pub(crate) const ORIGIN: NonceOrigin = NonceOrigin {
    chain_id: 56,
    sender: [0xb0; 20],
};

pub(crate) fn grant_of(expires: u64, strategies: LiveStrategies) -> LiveGrant {
    LiveGrant {
        venue: VenueId::Flap,
        strategies,
        sources: vec![SourceId::PublicNode],
        adapters: vec![LiveAdapter::Club48BuilderV1],
        max_input_wei: 1_000_000_000_000,
        max_spend_wei: 10_000_000_000_000_000,
        max_attempts: 2,
        expires_unix_ms: expires,
        exit_only: false,
        risk: Some(TEST_RISK),
        limits: Some(TEST_LIMITS),
    }
}

/// Limits no test BUY reaches unless it means to (31.10. order).
pub(crate) const TEST_LIMITS: GrantLimits = GrantLimits {
    max_open_positions: 16,
    max_realized_loss_wei: 1_000_000_000_000_000_000,
    max_daily_realized_loss_wei: 1_000_000_000_000_000_000,
    min_buy_interval_ms: 0,
};

/// A granted risk the tests' executor signs within.
pub(crate) const TEST_RISK: GrantRisk = GrantRisk {
    max_buy_slippage_bps: 1_000,
    max_exit_slippage_bps: 1_000,
    max_gas_price_wei: 1_000_000_000_000,
    max_buy_gas_limit: 10_000_000,
    max_approve_gas_limit: 1_000_000,
    max_sell_gas_limit: 5_000_000,
    outlier_ack: false,
};

/// Evidence for exactly that scope ([`grant`]'s).
pub(crate) fn qualified() -> LiveQualification {
    qualified_for(&grant(0))
}

/// Evidence for exactly the scope of the grant `s` holds.
pub(crate) fn qualified_in(s: &Store) -> LiveQualification {
    qualified_for(s.current().unwrap().live.as_ref().unwrap())
}

/// Evidence for exactly the scope of `g`, its risk decided for it.
pub(crate) fn qualified_for(g: &LiveGrant) -> LiveQualification {
    LiveQualification {
        venues: vec![VenueId::Flap],
        sources: vec![SourceId::PublicNode],
        adapters: vec![LiveAdapter::Club48BuilderV1],
        acquisition: true,
        acquisition_adapters: vec![LiveAdapter::Club48BuilderV1],
        risk_decision: true,
        risk_scope_sha256: Some(g.risk_scope_sha256()),
    }
}

/// A store of boot session `boot-1` with the whole scope requested
/// LIVE, every switch ENABLED, both limits set and the grant written.
pub(crate) fn live_ready(tag: &str) -> (PathBuf, Store, u64) {
    live_ready_with(tag, LiveStrategies::C3)
}

/// [`live_ready`] with a grant of `strategies`.
pub(crate) fn live_ready_with(tag: &str, strategies: LiveStrategies) -> (PathBuf, Store, u64) {
    live_ready_granting(tag, |now| grant_of(now + 3_600_000, strategies))
}

/// [`live_ready`] with the grant `grant(now)`, its scope requested.
pub(crate) fn live_ready_granting(
    tag: &str,
    grant: impl FnOnce(u64) -> LiveGrant,
) -> (PathBuf, Store, u64) {
    live_ready_limited(tag, (2, 3), grant)
}

/// [`live_ready_granting`] with the daily and campaign BUY limits.
pub(crate) fn live_ready_limited(
    tag: &str,
    (per_day, in_total): (u32, u32),
    grant: impl FnOnce(u64) -> LiveGrant,
) -> (PathBuf, Store, u64) {
    let (dir, s) = session(tag, "boot-1");
    let now = 1_793_059_200_000;
    let grant = grant(now);
    let strategies = grant.strategies;
    let mut g = 0;
    let mut req = |entity: Entity, mode: Mode| {
        let r = s.request(&format!("r-{g}"), entity, mode, g, now).unwrap();
        assert!(r.applied(), "{}", r.line());
        g += 1;
    };
    for id in SwitchId::ALL {
        req(Entity::Switch(id), Mode::Enabled);
    }
    req(Entity::Runtime, Mode::Live);
    if strategies.c3() {
        req(ComponentId::ExecC3.into(), Mode::Live);
    }
    // Strategy C decides every grant's exits (R1).
    req(ComponentId::StrategyC.into(), Mode::Live);
    req(ComponentId::StateC2.into(), Mode::Live);
    req(VenueId::Flap.into(), Mode::Live);
    // SC4 d: a Four.meme grant's venue (its Flap lots exit on Flap's).
    if grant.venue != VenueId::Flap {
        req(grant.venue.into(), Mode::Live);
    }
    req(SourceId::PublicNode.into(), Mode::Live);
    req(Entity::BuyLimit, Mode::PerDay(per_day));
    req(Entity::BuyTotal, Mode::PerDay(in_total));
    let r = s
        .grant_live("grant-1", grant.clone(), grant.scope_sha256(), g, now)
        .unwrap();
    assert!(r.applied(), "{}", r.line());
    (dir, s, now)
}

/// Plan 28.10.: a requested LIVE is effective LIVE only with every
/// condition of the gate met and the grant's scope naming it; each
/// condition alone blocks with its own reason. LIVE is never stored.
#[test]
fn live_is_effective_only_when_every_condition_holds() {
    let (dir, _s, now) = live_ready("live-gate");
    let (g, daily, total) = live_inputs(&dir, now, max()).unwrap();
    let q = qualified();
    let ctx = LiveContext {
        boot: Some("boot-1"),
        now_unix_ms: now,
        daily: Some(&daily),
        total: Some(&total),
        spent_wei: Some(0),
        qualification: &q,
    };
    assert_eq!(g.live_blockers(&ctx), Vec::<&str>::new());
    let scope: [Entity; 5] = [
        Entity::Runtime,
        ComponentId::ExecC3.into(),
        ComponentId::StateC2.into(),
        VenueId::Flap.into(),
        SourceId::PublicNode.into(),
    ];
    for e in scope {
        assert_eq!(
            g.effective_live_now(e, &ctx),
            Effective::Live,
            "{}",
            e.name()
        );
        assert!(
            matches!(g.effective_of(e), Effective::Blocked(_)),
            "stored never LIVE"
        );
    }
    assert_eq!(
        g.effective_live_now(SwitchId::Sign.into(), &ctx),
        Effective::Enabled
    );
    // Outside the scope: dRPC, Four. Strategy C is LIVE under a C3 grant
    // (01.11., R1: it decides every lot's exits; the owner refuses its
    // BUYs, `each_live_intent_is_admitted_by_the_owner_as_it_holds_now`).
    let mut wider = g.clone();
    let by = SetBy {
        request_id: "w".into(),
        generation: g.generation,
    };
    for e in [
        Entity::from(ComponentId::StrategyC),
        SourceId::Drpc.into(),
        VenueId::FourClassic.into(),
    ] {
        wider.set(e, Mode::Live, by.clone());
    }
    assert_eq!(
        wider.effective_live_now(ComponentId::StrategyC.into(), &ctx),
        Effective::Live
    );
    assert_eq!(
        wider.effective_live_now(SourceId::Drpc.into(), &ctx),
        Effective::Blocked(LIVE_NOT_IN_SCOPE)
    );
    // SC4 d: a venue the grant does not name is out of its scope.
    assert_eq!(
        wider.effective_live_now(VenueId::FourClassic.into(), &ctx),
        Effective::Blocked(LIVE_NOT_IN_SCOPE)
    );
    // Each switch alone.
    for (id, reason) in [
        (SwitchId::Live, ECONOMIC_LIVE_DISABLED),
        (SwitchId::WalletLauncher, WALLET_LAUNCHER_DISABLED),
        (SwitchId::WalletBuyer, WALLET_BUYER_DISABLED),
        (SwitchId::Sign, ECON_SIGN_DISABLED),
        (SwitchId::Arm, ECON_ARM_DISABLED),
        (SwitchId::Send, ECON_SEND_DISABLED),
        (SwitchId::Attempt, ECON_ATTEMPT_DISABLED),
    ] {
        let mut off = g.clone();
        off.set(id.into(), Mode::Disabled, by.clone());
        assert_eq!(off.live_blockers(&ctx), vec![reason], "{}", id.name());
        assert_eq!(
            off.effective_live_now(ComponentId::ExecC3.into(), &ctx),
            Effective::Blocked(reason)
        );
    }
    // Messplan §3b C: the eight ARM/SEND/ATTEMPT settings, every other
    // condition held: LIVE only with all three, else each one off named.
    for bits in 0u8..8 {
        let mut m = g.clone();
        let mut want = Vec::new();
        for (i, (id, reason)) in [
            (SwitchId::Arm, ECON_ARM_DISABLED),
            (SwitchId::Send, ECON_SEND_DISABLED),
            (SwitchId::Attempt, ECON_ATTEMPT_DISABLED),
        ]
        .into_iter()
        .enumerate()
        {
            if bits & (1 << i) == 0 {
                m.set(id.into(), Mode::Disabled, by.clone());
                want.push(reason);
            }
        }
        let mut got = m.live_blockers(&ctx);
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(got, want, "{bits:03b}");
        assert_eq!(
            m.effective_live_now(ComponentId::ExecC3.into(), &ctx) == Effective::Live,
            bits == 7,
            "{bits:03b}"
        );
    }
    // The runtime not asked for LIVE, no grant.
    let mut paper = g.clone();
    paper.set(Entity::Runtime, Mode::Paper, by.clone());
    assert_eq!(paper.live_blockers(&ctx), vec![LIVE_RUNTIME_NOT_REQUESTED]);
    assert_eq!(
        paper.effective_live_now(Entity::Runtime, &ctx),
        Effective::Paper
    );
    let mut revoked = g.clone();
    revoked.set(Entity::LiveGrant, Mode::Disabled, by.clone());
    assert_eq!(revoked.live_blockers(&ctx), vec![LIVE_GRANT_MISSING]);
    // The context: expiry, limits, spend, evidence.
    let grant = g.live.clone().unwrap();
    let used_up = |u: &BuyUsage| BuyUsage {
        used: u64::from(u.limit),
        ..u.clone()
    };
    let daily_out = used_up(&daily);
    let total_out = TotalUsage {
        used: u64::from(total.limit),
        ..total.clone()
    };
    let mut no_venue = qualified();
    no_venue.venues.clear();
    let mut no_source = qualified();
    no_source.sources = vec![SourceId::Drpc];
    let mut no_adapter = qualified();
    no_adapter.adapters = vec![LiveAdapter::BlockRazorBuilderV1];
    let mut no_acquisition = qualified();
    no_acquisition.acquisition = false;
    // BSC3-049: an acquisition sent through another builder only.
    let mut other_builder = qualified();
    other_builder.acquisition_adapters = vec![LiveAdapter::BlockRazorBuilderV1];
    let mut no_risk = qualified();
    no_risk.risk_decision = false;
    // BSC3-049: a risk decision for another grant scope, or (a v3
    // record) for none.
    let mut other_scope = qualified();
    other_scope.risk_scope_sha256 = Some([0x0e; 32]);
    let mut unscoped = qualified();
    unscoped.risk_scope_sha256 = None;
    let cases: Vec<(LiveContext<'_>, &str)> = vec![
        (
            LiveContext {
                now_unix_ms: grant.expires_unix_ms,
                ..ctx
            },
            LIVE_GRANT_EXPIRED,
        ),
        (LiveContext { daily: None, ..ctx }, BUY_LIMIT_UNREADABLE),
        (
            LiveContext {
                daily: Some(&daily_out),
                ..ctx
            },
            BUY_LIMIT_EXHAUSTED,
        ),
        (LiveContext { total: None, ..ctx }, ATTEMPT_TOTAL_UNREADABLE),
        (
            LiveContext {
                total: Some(&total_out),
                ..ctx
            },
            ATTEMPT_TOTAL_EXHAUSTED,
        ),
        (
            LiveContext {
                spent_wei: None,
                ..ctx
            },
            LIVE_SPEND_UNREADABLE,
        ),
        (
            LiveContext {
                spent_wei: Some(grant.max_spend_wei - grant.max_input_wei + 1),
                ..ctx
            },
            LIVE_GRANT_SPEND_EXHAUSTED,
        ),
        (
            LiveContext {
                qualification: &no_venue,
                ..ctx
            },
            LIVE_VENUE_UNQUALIFIED,
        ),
        (
            LiveContext {
                qualification: &no_source,
                ..ctx
            },
            LIVE_SOURCE_UNQUALIFIED,
        ),
        (
            LiveContext {
                qualification: &no_adapter,
                ..ctx
            },
            LIVE_ADAPTER_UNQUALIFIED,
        ),
        (
            LiveContext {
                qualification: &no_acquisition,
                ..ctx
            },
            LIVE_ACQUISITION_UNPROVEN,
        ),
        (
            LiveContext {
                qualification: &other_builder,
                ..ctx
            },
            LIVE_ACQUISITION_BUILDER_UNPROVEN,
        ),
        (
            LiveContext {
                qualification: &no_risk,
                ..ctx
            },
            LIVE_RISK_DECISION_MISSING,
        ),
        (
            LiveContext {
                qualification: &other_scope,
                ..ctx
            },
            LIVE_RISK_DECISION_OTHER_SCOPE,
        ),
        (
            LiveContext {
                qualification: &unscoped,
                ..ctx
            },
            LIVE_RISK_DECISION_OTHER_SCOPE,
        ),
    ];
    for (c, reason) in cases {
        assert_eq!(g.live_blockers(&c), vec![reason]);
        assert_eq!(
            g.effective_live_now(VenueId::Flap.into(), &c),
            Effective::Blocked(reason)
        );
    }
    // BSC3-049 review: a builder without the acquisition, or a risk
    // decision for another scope, still takes the exits of what the
    // grant holds; no risk decision at all does not.
    for q in [&other_builder, &other_scope] {
        assert!(
            g.exit_blockers(&LiveContext {
                qualification: q,
                ..ctx
            })
            .is_empty()
        );
    }
    assert_eq!(
        g.exit_blockers(&LiveContext {
            qualification: &no_risk,
            ..ctx
        }),
        vec![LIVE_RISK_DECISION_MISSING]
    );
    // Another boot session: the switches and the grant are reset.
    let reboot = LiveContext {
        boot: Some("boot-2"),
        ..ctx
    };
    let blockers = g.live_blockers(&reboot);
    assert_eq!(blockers.first(), Some(&ECONOMIC_LIVE_DISABLED));
    assert!(
        blockers.contains(&LIVE_GRANT_RESET_BY_RESTART),
        "{blockers:?}"
    );
    assert!(blockers.contains(&WALLET_BUYER_DISABLED), "{blockers:?}");
    let blind = LiveContext { boot: None, ..ctx };
    assert!(
        g.live_blockers(&blind)
            .contains(&LIVE_GRANT_RESET_BY_RESTART)
    );
    fs::remove_dir_all(dir).unwrap();
}

/// The grant's own attempt limit counts only its attempts.
#[test]
fn a_grant_takes_no_more_attempts_than_it_allows() {
    let (dir, s, now) = live_ready("live-attempts");
    let grant_generation = s.current().unwrap().live_set_by.unwrap().generation;
    s.consume_attempt("l1", grant_generation, now).unwrap();
    s.consume_attempt("l2", grant_generation, now).unwrap();
    let (g, daily, total) = live_inputs(&dir, now, max()).unwrap();
    assert_eq!(total.used_by_grant(grant_generation), 2);
    let q = qualified();
    let ctx = LiveContext {
        boot: Some("boot-1"),
        now_unix_ms: now,
        daily: Some(&daily),
        total: Some(&total),
        spent_wei: Some(0),
        qualification: &q,
    };
    // Two of three campaign attempts are left... none of the grant's.
    let blockers = g.live_blockers(&ctx);
    assert!(
        blockers.contains(&LIVE_GRANT_ATTEMPTS_EXHAUSTED),
        "{blockers:?}"
    );
    assert!(!blockers.contains(&ATTEMPT_TOTAL_EXHAUSTED), "{blockers:?}");
    fs::remove_dir_all(dir).unwrap();
}

/// The `personal_sign` of a grant's presence digest for owner generation
/// `generation` by the key [0x31; 32] (R9 tests).
pub(crate) fn presence_signed(grant: &LiveGrant, generation: u64) -> [u8; 65] {
    let key = k256::ecdsa::SigningKey::from_slice(&[0x31; 32]).unwrap();
    let mut message = b"\x19Ethereum Signed Message:\n32".to_vec();
    message.extend_from_slice(&presence_digest(grant, generation));
    let (sig, id) = key
        .sign_prehash_recoverable(alloy_primitives::keccak256(&message).as_slice())
        .unwrap();
    let mut out = [0u8; 65];
    out[..64].copy_from_slice(&sig.to_bytes());
    out[64] = 27 + u8::from(id.is_y_odd());
    out
}

/// The address of the key [`presence_signed`] signs with.
pub(crate) fn presence_operator() -> [u8; 20] {
    darkbot_bsc_trading::signer::Signer::from_secret_bytes(&[0x31; 32])
        .unwrap()
        .address()
        .into_array()
}

/// REV.1.0 R9 (BSC3-001): a grant's presence signature goes with it
/// (stored, read back); its signer is the key that signed the grant's
/// digest for the generation it is written as (its expiry or exit-only
/// flag changed, another generation: not; an unknown `v`: none, refused
/// at the grant); a later grant without one and a revocation carry none;
/// a retry of the same request with another signature is refused.
#[test]
fn a_presence_signature_goes_with_its_grant() {
    let (dir, s) = session("presence-store", "boot-1");
    let now = 1_793_059_200_000;
    let grant = grant(now + 1_000);
    let signature = presence_signed(&grant, 1);
    let operator = Some(presence_operator());
    assert_eq!(presence_signer(&grant, 1, &signature), operator);
    assert_ne!(presence_signer(&grant, 4, &signature), operator);
    let mut exit_only = grant.clone();
    exit_only.exit_only = true;
    let mut later = grant.clone();
    later.expires_unix_ms += 1;
    for other in [exit_only, later] {
        assert_ne!(presence_signer(&other, 1, &signature), operator);
    }
    let mut v = signature;
    v[64] = 29;
    assert_eq!(presence_signer(&grant, 1, &v), None);
    let present = |id: &str, signature, generation| {
        s.grant_live_present(
            id,
            grant.clone(),
            grant.scope_sha256(),
            Some(signature),
            generation,
            now,
        )
    };
    let e = present("g0", v, 0).unwrap_err();
    assert!(e.contains("recovers to no key"), "{e}");
    let first = present("g1", signature, 0).unwrap();
    assert!(first.applied(), "{}", first.line());
    assert_eq!(s.current().unwrap().live_presence, Some(signature));
    // Review LOW-2: the same request, another signature.
    let e = present("g1", presence_signed(&grant, 2), 0).unwrap_err();
    assert!(e.contains("another presence signature"), "{e}");
    s.grant_live("g2", grant.clone(), grant.scope_sha256(), 1, now)
        .unwrap();
    assert_eq!(s.current().unwrap().live_presence, None);
    assert!(
        present("g3", presence_signed(&grant, 3), 2)
            .unwrap()
            .applied()
    );
    s.request("r1", Entity::LiveGrant, Mode::Disabled, 3, now)
        .unwrap();
    assert_eq!(s.current().unwrap().live_presence, None);
    fs::remove_dir_all(dir).unwrap();
}

/// The operator's confirmation is the scope: what was not confirmed is
/// never stored; the same request is its receipt; DISABLED, a restart
/// and a write in another boot session revoke it.
#[test]
fn a_live_grant_needs_its_confirmed_scope_and_ends_with_revoke_or_restart() {
    let (dir, s) = session("live-grant", "boot-1");
    let now = 1_793_059_200_000;
    let grant = grant(now + 1_000);
    let e = s
        .grant_live("g1", grant.clone(), [0; 32], 0, now)
        .unwrap_err();
    assert!(e.contains("is not this grant's scope"), "{e}");
    // SC4 d: a grant may name a Four.meme venue (live only with its
    // qualification).
    let mut four = grant.clone();
    four.venue = VenueId::FourClassic;
    // Review M3: Strategy C's alone (C3 BUYs a Flap launch).
    for strategies in [LiveStrategies::C3, LiveStrategies::Both] {
        let e = LiveGrant {
            strategies,
            ..four.clone()
        }
        .validate()
        .unwrap_err();
        assert!(e.contains("takes STRATEGY_C alone"), "{e}");
    }
    four.strategies = LiveStrategies::StrategyC;
    four.validate().unwrap();
    assert_ne!(four.risk_scope_sha256(), grant.risk_scope_sha256());
    let mut twice = grant.clone();
    twice.sources.push(SourceId::PublicNode);
    let mut many = grant.clone();
    many.max_attempts = max() + 1;
    let mut nothing = grant.clone();
    nothing.adapters.clear();
    let mut over = grant.clone();
    over.max_input_wei = over.max_spend_wei + 1;
    for bad in [twice, many, nothing, over] {
        assert!(
            s.grant_live("g1", bad.clone(), bad.scope_sha256(), 0, now)
                .is_err()
        );
    }
    assert!(
        s.request(
            "g1",
            Entity::LiveGrant,
            Mode::Granted(grant.scope_sha256()),
            0,
            now
        )
        .unwrap_err()
        .contains("live grant")
    );
    let r = s
        .grant_live("g1", grant.clone(), grant.scope_sha256(), 0, now)
        .unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
    assert!(
        r.line().contains(&format!(
            "source=BSC_LIVE_GRANT requested=GRANTED:{} effective=ENABLED",
            hex32(&grant.scope_sha256())
        )),
        "{}",
        r.line()
    );
    assert_eq!(s.current().unwrap().live.as_ref(), Some(&grant));
    assert_eq!(
        s.grant_live("g1", grant.clone(), grant.scope_sha256(), 0, now)
            .unwrap(),
        r
    );
    let mut other = grant.clone();
    other.max_input_wei += 1;
    assert!(
        s.grant_live("g1", other.clone(), other.scope_sha256(), 0, now)
            .unwrap_err()
            .contains("another request")
    );
    s.request("g2", Entity::LiveGrant, Mode::Disabled, 1, now)
        .unwrap();
    let g = s.current().unwrap();
    assert_eq!(g.live, None);
    assert_eq!(g.live_set_by.map(|b| b.request_id), Some("g2".into()));
    s.grant_live("g3", grant.clone(), grant.scope_sha256(), 2, now)
        .unwrap();
    let after = s.restart(now).unwrap();
    assert_eq!(after.live, None);
    assert_eq!(
        after.live_set_by.as_ref().unwrap().request_id,
        RESTART_RESET
    );
    s.grant_live(
        "g4",
        grant.clone(),
        grant.scope_sha256(),
        after.generation,
        now,
    )
    .unwrap();
    let other_boot = Store::open_in_session(&dir, keeping(8), Ok("boot-2".into())).unwrap();
    let g = other_boot.current().unwrap();
    assert!(g.live.is_some());
    other_boot
        .request("g5", SourceId::Drpc, Mode::Shadow, g.generation, now)
        .unwrap();
    assert_eq!(other_boot.current().unwrap().live, None);
    fs::remove_dir_all(dir).unwrap();
}

/// A stored grant that is not exactly one granted scope or one
/// revocation never decodes (no more permissive state is accepted).
#[test]
fn a_tampered_or_partial_live_grant_never_decodes() {
    use darkbot_bsc_record::{Presence, Value};
    use sm::live as k;
    let (dir, s, now) = live_ready("live-tamper");
    let g = s.current().unwrap();
    let bytes = g.encode(now).unwrap();
    type Edit<'a> = &'a dyn Fn(&mut Vec<(u16, Value)>);
    let tampered = |edit: Edit<'_>| -> Vec<u8> {
        let r = decode_generation(&bytes).unwrap();
        let mut d = Draft::new(r.schema());
        for (tag, v) in r.fields() {
            let mut v = v.clone();
            if *tag == sm::LIVE
                && let Value::Nested(inner) = &mut v
            {
                edit(inner);
            }
            if r.schema()
                .field(*tag)
                .is_some_and(|f| f.presence == Presence::Repeated)
            {
                d.push(*tag, v);
            } else {
                d.set(*tag, v);
            }
        }
        d.encode().unwrap()
    };
    assert!(
        Generation::decode(&tampered(&|_| {})).is_ok(),
        "the copy itself decodes"
    );
    let digest = |inner: &mut Vec<(u16, Value)>| {
        for (t, v) in inner.iter_mut() {
            if *t == k::SCOPE_SHA256 {
                *v = Value::B256([7; 32]);
            }
        }
    };
    let partial = |inner: &mut Vec<(u16, Value)>| inner.retain(|(t, _)| *t != k::VENUE);
    let revoked_with_scope = |inner: &mut Vec<(u16, Value)>| {
        for (t, v) in inner.iter_mut() {
            if *t == k::STATE {
                *v = Value::Text("REVOKED".into());
            }
        }
    };
    for (edit, why) in [
        (&digest as Edit<'_>, "not the one it was granted for"),
        (&partial, "either the whole scope or a revocation"),
        (
            &revoked_with_scope,
            "either the whole scope or a revocation",
        ),
    ] {
        let e = Generation::decode(&tampered(edit)).unwrap_err();
        assert!(e.contains(why), "{e}");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Audit E 3.3: a runtime status is current within its own number of
/// its own intervals; without either it never is.
#[test]
fn a_status_is_current_within_its_own_factor() {
    use crate::live_qualification::tests::filled;
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
    let read = |factor: Option<u64>| {
        let mut d = filled(&rs::SCHEMA);
        d.set(rs::PROVENANCE_OBSERVED_UNIX_MS, 10_000u64)
            .set(rs::STATUS_INTERVAL_MS, 1_000u64);
        if let Some(f) = factor {
            d.set(rs::STATUS_STALE_INTERVALS, f);
        }
        darkbot_bsc_record::decode(&rs::SCHEMA, &d.encode().unwrap()).unwrap()
    };
    let five = read(Some(5));
    assert!(status_is_current(&five, 15_000) && !status_is_current(&five, 15_001));
    let two = read(Some(2));
    assert!(status_is_current(&two, 12_000) && !status_is_current(&two, 12_001));
    assert!(!status_is_current(&read(None), 10_000));
}

/// Audit E 3.1/3.4: the owner's bounds are the configuration's: both
/// keys required and in range; a request above the configured maximum
/// is refused, a stored limit above a lowered one is capped where it
/// counts (the slots, the gate), and the generations kept are its.
#[test]
fn the_configured_bounds_refuse_cap_and_prune() {
    let text = |max: &str, keep: &str| {
        format!("A=1\nBSC_BUY_ATTEMPTS_TOTAL_MAX={max}\nBSC_OWNER_KEEP_GENERATIONS={keep}\n")
    };
    assert_eq!(
        OwnerLimits::of_config(&text("3", "2")),
        Ok(OwnerLimits {
            buy_attempts_total_max: 3,
            keep_generations: 2
        })
    );
    for (max, keep) in [("0", "2"), ("11", "2"), ("3", "0"), ("x", "2")] {
        assert!(
            OwnerLimits::of_config(&text(max, keep)).is_err(),
            "{max} {keep}"
        );
    }
    assert!(OwnerLimits::of_config("BSC_OWNER_KEEP_GENERATIONS=2\n").is_err());
    // Review E3 L5: a key set twice is refused, as the runtime refuses it.
    let twice = format!("{}BSC_BUY_ATTEMPTS_TOTAL_MAX=3\n", text("3", "2"));
    assert!(OwnerLimits::of_config(&twice).is_err());
    assert!(
        OwnerLimits::of_config(&format!(
            "#BSC_BUY_ATTEMPTS_TOTAL_MAX=9\n{}",
            text("3", "2")
        ))
        .is_ok()
    );
    assert_eq!(shipped_limits().buy_attempts_total_max, 10);
    // A total of ten on file, read under a maximum of three.
    let (dir, ten) = session("owner-limits", "boot-1");
    ten.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
        .unwrap();
    ten.request("bl", Entity::BuyLimit, Mode::PerDay(10), 1, 1)
        .unwrap();
    let three = OwnerLimits {
        buy_attempts_total_max: 3,
        keep_generations: 2,
    };
    let s = Store::open_in_session(&dir, three, Ok("boot-1".into())).unwrap();
    let e = s
        .request("bt-4", Entity::BuyTotal, Mode::PerDay(4), 2, 1)
        .unwrap_err();
    assert!(e.contains("at most BSC_BUY_ATTEMPTS_TOTAL_MAX"), "{e}");
    assert!(
        s.request("bl-4", Entity::BuyLimit, Mode::PerDay(4), 2, 1)
            .is_err()
    );
    assert_eq!(total_usage(&dir, 3).unwrap().limit, 3);
    assert_eq!(buy_usage(&dir, 1, 3).unwrap().limit, 3);
    for a in ["a1", "a2", "a3"] {
        s.consume_attempt(a, 0, 1).unwrap();
    }
    let e = s.consume_attempt("a4", 0, 1).unwrap_err();
    assert!(
        e.starts_with("BUY_ATTEMPT_") && e.contains("REACHED"),
        "{e}"
    );
    // The gate reads them under the same maximum: nothing left.
    let g = s.current().unwrap();
    let reads = GateReads::read(&dir, &g, 1, 3);
    assert_eq!(
        (
            reads.total.as_ref().map(TotalUsage::remaining),
            reads.daily.as_ref().map(BuyUsage::remaining)
        ),
        (Some(0), Some(0))
    );
    // Review E3 M1: a store without bounds takes the reducing requests
    // (init, reset, a revocation, a switch) and refuses what needs them.
    let reducing = Store::open_reducing(&dir).unwrap();
    let g = reducing.current().unwrap().generation;
    let e = reducing
        .request("bt-r", Entity::BuyTotal, Mode::PerDay(1), g, 1)
        .unwrap_err();
    assert!(e.contains("not configured"), "{e}");
    let e = reducing.consume_attempt("a5", 0, 1).unwrap_err();
    assert!(e.contains("not configured"), "{e}");
    reducing
        .request("off", Entity::Switch(SwitchId::Sign), Mode::Disabled, g, 1)
        .unwrap();
    reducing.restart(1).unwrap();
    let mut grant = grant(u64::MAX);
    grant.max_attempts = 4;
    assert!(grant.validate().is_ok());
    assert!(grant.validate_within(3).is_err() && grant.validate_within(4).is_ok());
    // Two generations kept besides the current one, however many are
    // written.
    for (i, mode) in [Mode::Enabled, Mode::Disabled]
        .repeat(3)
        .into_iter()
        .enumerate()
    {
        let g = s.current().unwrap().generation;
        s.request(
            &format!("sign-{i}"),
            Entity::Switch(SwitchId::Sign),
            mode,
            g,
            2,
        )
        .unwrap();
    }
    let current = s.current().unwrap().generation;
    let mut kept: Vec<u64> = fs::read_dir(dir.join("generations"))
        .unwrap()
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .to_str()?
                .strip_suffix(".dbrec")?
                .parse()
                .ok()
        })
        .collect();
    kept.sort();
    assert_eq!(kept, [current - 2, current - 1, current]);
}

/// Plan 28.10.: the campaign's total BUY limit, 0 initially and at most
/// ten, bounds every attempt over every day and restart; the same
/// attempt keeps its slot; concurrent attempts never exceed it.
#[test]
fn the_campaign_total_bounds_every_attempt_across_days_and_restarts() {
    let (dir, s) = session("buy-total", "boot-1");
    let now = 1_793_059_200_000;
    s.request("bl", Entity::BuyLimit, Mode::PerDay(10), 0, now)
        .unwrap();
    let e = s.consume_buy_attempt("a1", now).unwrap_err();
    assert!(e.starts_with("BUY_ATTEMPT_TOTAL_REACHED: 0 of 0"), "{e}");
    assert!(
        s.request("bt-bad", Entity::BuyTotal, Mode::PerDay(11), 1, now)
            .is_err(),
        "at most ten"
    );
    s.request("bt", Entity::BuyTotal, Mode::PerDay(2), 1, now)
        .unwrap();
    // The refused attempt kept its daily slot (over-, never under-counted).
    assert_eq!(s.consume_buy_attempt("a1", now).unwrap().slot, 0);
    let (day, total) = s.consume_attempt("a1", 0, now + 1).unwrap();
    assert_eq!(
        (day.slot, total.slot),
        (0, 0),
        "the same attempt, the same slots"
    );
    s.consume_attempt("a2", 7, now + 2).unwrap();
    let e = s.consume_buy_attempt("a3", now + DAY_MS).unwrap_err();
    assert!(e.starts_with("BUY_ATTEMPT_TOTAL_REACHED: 2 of 2"), "{e}");
    let usage = total_usage(&dir, max()).unwrap();
    assert_eq!(
        (
            usage.used,
            usage.limit,
            usage.remaining(),
            usage.used_by_grant(7)
        ),
        (2, 2, 0, 1)
    );
    let again = Store::open_in_session(&dir, keeping(8), Ok("boot-2".into())).unwrap();
    again.restart(now).unwrap();
    assert_eq!(total_usage(&dir, max()).unwrap().used, 2);
    assert_eq!(again.current().unwrap().buy_attempts_total(max()), 2);
    fs::remove_dir_all(dir).unwrap();
    let (dir, s) = session("buy-total-race", "boot-1");
    s.request("bl", Entity::BuyLimit, Mode::PerDay(10), 0, 1)
        .unwrap();
    s.request("bt", Entity::BuyTotal, Mode::PerDay(3), 1, 1)
        .unwrap();
    let ok = std::thread::scope(|scope| {
        let runs: Vec<_> = (0..8)
            .map(|i| {
                let s = &s;
                scope.spawn(move || s.consume_attempt(&format!("t{i}"), 0, 1).is_ok())
            })
            .collect();
        runs.into_iter()
            .map(|r| r.join().unwrap())
            .filter(|ok| *ok)
            .count()
    });
    assert_eq!(ok, 3);
    assert_eq!(total_usage(&dir, max()).unwrap().used, 3);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-008: a receipt goes with the generations its request names
/// (they grew by one per request for ever); its request never applies a
/// second time: the same request again reads UNRESOLVED (its expected
/// generation stale, the generation it wrote gone) or REJECTED_STALE,
/// the generation stays; a request still kept gets its receipt; one
/// that expected a later generation keeps its refusal until that
/// generation is past (review M: else it would apply when the state
/// reached it). An ID whose receipt went may name a new request.
#[test]
fn receipts_go_with_their_generations_and_never_apply_again() {
    let (dir, s) = session("receipt-prune", "boot-1");
    let receipts = || fs::read_dir(dir.join("receipts")).unwrap().count() as u64;
    let mode = |n: u64| {
        if n.is_multiple_of(2) {
            Mode::Enabled
        } else {
            Mode::Disabled
        }
    };
    for n in 0..40u64 {
        let r = s
            .request(&format!("r{n}"), SwitchId::Send, mode(n), n, n + 1)
            .unwrap();
        assert_eq!((r.outcome, r.generation), ("APPLIED", n + 1));
    }
    // Another request meeting a kept generation another one wrote.
    let stale = s
        .request("late", SwitchId::Arm, Mode::Enabled, 35, 50)
        .unwrap();
    assert_eq!(stale.outcome, "REJECTED_STALE");
    // Generations 32..=40 kept (8 besides the current), their nine
    // requests' receipts and the stale one's.
    assert_eq!(receipts(), 10);
    let again = s
        .request("r0", SwitchId::Send, Mode::Enabled, 0, 60)
        .unwrap();
    assert_eq!(again.outcome, "UNRESOLVED");
    assert!(!again.applied());
    assert_eq!(s.current().unwrap().generation, 40);
    let kept = s.request("r39", SwitchId::Send, mode(39), 39, 61).unwrap();
    assert_eq!((kept.outcome, kept.generation), ("APPLIED", 40));
    // A refused request whose generation went: refused again.
    for n in 40..50u64 {
        s.request(&format!("r{n}"), SwitchId::Send, mode(n), n, 70 + n)
            .unwrap();
    }
    let late = s
        .request("late", SwitchId::Arm, Mode::Enabled, 35, 120)
        .unwrap();
    assert!(!late.applied(), "{}", late.outcome);
    // A request expecting a far later generation (a mistyped one).
    let ahead = s
        .request("ahead", SwitchId::Arm, Mode::Enabled, 70, 121)
        .unwrap();
    assert_eq!(ahead.outcome, "REJECTED_STALE");
    for n in 50..70u64 {
        s.request(&format!("r{n}"), SwitchId::Send, mode(n), n, 80 + n)
            .unwrap();
    }
    assert_eq!(s.current().unwrap().generation, 70);
    // At generation 70 the same request again: still its refusal, never
    // applied, also after the generations passed 70 by more than kept.
    assert_eq!(
        s.request("ahead", SwitchId::Arm, Mode::Enabled, 70, 160)
            .unwrap(),
        ahead
    );
    for n in 70..90u64 {
        s.request(&format!("r{n}"), SwitchId::Send, mode(n), n, 100 + n)
            .unwrap();
    }
    let again = s
        .request("ahead", SwitchId::Arm, Mode::Enabled, 70, 200)
        .unwrap();
    assert!(!again.applied(), "{}", again.outcome);
    assert_eq!(s.current().unwrap().generation, 90);
    // An ID whose receipt went names a new request, at its own CAS.
    let reused = s
        .request("r0", SwitchId::Arm, Mode::Enabled, 90, 201)
        .unwrap();
    assert_eq!((reused.outcome, reused.generation), ("APPLIED", 91));
    fs::remove_dir_all(dir).unwrap();
}

/// Review 27.10. L4: a lost receipt is recovered only for the same
/// payload; an identical twin request that loses the generation race
/// still reports APPLIED.
#[test]
fn receipts_are_recovered_only_for_the_same_request() {
    let (dir, s) = session("receipt-recovery", "boot-1");
    s.request("x", SwitchId::Send, Mode::Enabled, 0, 1).unwrap();
    fs::remove_file(dir.join("receipts/x.dbrec")).unwrap();
    assert!(
        s.request("x", SwitchId::Send, Mode::Disabled, 0, 2)
            .unwrap_err()
            .contains("another request")
    );
    // Another entity that happens to hold the mode is not this request.
    assert!(
        s.request("x", SwitchId::Arm, Mode::Disabled, 0, 2)
            .unwrap_err()
            .contains("another request")
    );
    let r = s.request("x", SwitchId::Send, Mode::Enabled, 0, 3).unwrap();
    assert_eq!((r.outcome, r.generation), ("APPLIED", 1));
    let outcomes: Vec<&str> = std::thread::scope(|scope| {
        let runs: Vec<_> = (0..6)
            .map(|_| {
                let s = &s;
                scope.spawn(move || {
                    s.request("twin", SwitchId::Arm, Mode::Enabled, 1, 4)
                        .unwrap()
                        .outcome
                })
            })
            .collect();
        runs.into_iter().map(|r| r.join().unwrap()).collect()
    });
    assert!(outcomes.iter().all(|o| *o == "APPLIED"), "{outcomes:?}");
    assert_eq!(s.current().unwrap().generation, 2);
    fs::remove_dir_all(dir).unwrap();
}

/// Review 27.10. L6: the daily limit takes the G0.7 runner's attempt IDs.
#[test]
fn the_daily_limit_takes_the_runners_attempt_ids() {
    let (dir, s) = session("attempt-ids", "boot-1");
    s.request("bt", Entity::BuyTotal, Mode::PerDay(10), 0, 1)
        .unwrap();
    s.request("bl", Entity::BuyLimit, Mode::PerDay(3), 1, 1)
        .unwrap();
    assert!(s.consume_buy_attempt("g07.attempt_1-a", 1).is_ok());
    assert!(s.consume_buy_attempt("-x", 1).is_ok(), "G0.7 takes it");
    for bad in ["", ".x", "a/b", &"a".repeat(129)] {
        assert!(s.consume_buy_attempt(bad, 1).is_err(), "{bad}");
    }
    assert!(s.consume_buy_attempt(&"a".repeat(128), 1).is_ok());
    fs::remove_dir_all(dir).unwrap();
}

/// Review 27.10. pass 2: the legitimate path. The account home from
/// the user database is this test process's HOME (when set), and the
/// canonical owner state lies under it.
#[test]
fn the_canonical_state_is_under_the_account_home() {
    let home = account_home().unwrap();
    assert!(home.is_absolute() && home.is_dir(), "{}", home.display());
    if let Some(h) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        assert_eq!(PathBuf::from(h), home);
    }
    assert_eq!(
        canonical_state_dir().unwrap(),
        home.join(".darkbot/run/bsc-sources")
    );
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

/// A grant from its canonical scope text (tests only; the owner builds a
/// grant from the CLI's arguments).
fn grant_of_text(text: &str) -> LiveGrant {
    let mut g = grant(0);
    let (mut risk, mut limits) = (TEST_RISK, TEST_LIMITS);
    for line in text.lines().skip(1) {
        let (key, v) = line.split_once('=').unwrap();
        let list = |v: &str| {
            v.split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        match key {
            "venue" => g.venue = VenueId::parse(v).unwrap(),
            "strategies" => g.strategies = LiveStrategies::parse(v).unwrap(),
            "sources" => {
                g.sources = list(v)
                    .iter()
                    .map(|s| SourceId::parse(s).unwrap())
                    .collect()
            }
            "adapters" => {
                g.adapters = list(v)
                    .iter()
                    .map(|a| LiveAdapter::parse(a).unwrap())
                    .collect()
            }
            "max_input_wei" => g.max_input_wei = v.parse().unwrap(),
            "max_spend_wei" => g.max_spend_wei = v.parse().unwrap(),
            "max_attempts" => g.max_attempts = v.parse().unwrap(),
            "expires_unix_ms" => g.expires_unix_ms = v.parse().unwrap(),
            "max_buy_slippage_bps" => risk.max_buy_slippage_bps = v.parse().unwrap(),
            "max_exit_slippage_bps" => risk.max_exit_slippage_bps = v.parse().unwrap(),
            "max_gas_price_wei" => risk.max_gas_price_wei = v.parse().unwrap(),
            "max_buy_gas_limit" => risk.max_buy_gas_limit = v.parse().unwrap(),
            "max_approve_gas_limit" => risk.max_approve_gas_limit = v.parse().unwrap(),
            "max_sell_gas_limit" => risk.max_sell_gas_limit = v.parse().unwrap(),
            "risk_outlier_ack" => risk.outlier_ack = v == "YES",
            "max_open_positions" => limits.max_open_positions = v.parse().unwrap(),
            "max_realized_loss_wei" => limits.max_realized_loss_wei = v.parse().unwrap(),
            "max_daily_realized_loss_wei" => {
                limits.max_daily_realized_loss_wei = v.parse().unwrap()
            }
            "min_buy_interval_ms" => limits.min_buy_interval_ms = v.parse().unwrap(),
            "exit_only" => g.exit_only = v == "true",
            other => panic!("unknown scope key {other}"),
        }
    }
    g.risk = Some(risk);
    g.limits = Some(limits);
    g
}

/// Audit E 4.9: the shared gateway's command allowlists (its boundary
/// against a browser request, which it checks before any owner call) are
/// exactly this owner's IDs and grant choices, in its order: they cannot
/// drift from the one authority. The frontend takes them from the
/// projection (`bsc.live.state`, the native source record).
#[test]
fn the_gateway_allowlists_are_the_owners_ids() {
    const GATEWAY: &str = include_str!("../../../../../../gateway/src/control.rs");
    let list = |name: &str| -> Vec<String> {
        let at = GATEWAY
            .find(&format!("const {name}: [&str; "))
            .unwrap_or_else(|| panic!("{name} not in gateway/src/control.rs"));
        let body = &GATEWAY[at..];
        let body = &body[body.find("= [").unwrap() + 3..body.find("];").unwrap()];
        body.split(',')
            .map(|s| s.trim().trim_matches('"').to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let names = |v: Vec<&str>| v.into_iter().map(str::to_owned).collect::<Vec<_>>();
    let sources: Vec<&str> = SourceId::ALL
        .iter()
        .map(|s| s.name())
        .chain(ComponentId::ALL.iter().map(|c| c.name()))
        .collect();
    assert_eq!(list("BSC_SOURCE_IDS"), names(sources));
    assert_eq!(
        list("BSC_VENUE_IDS"),
        names(VenueId::ALL.iter().map(|v| v.name()).collect())
    );
    let switches: Vec<&str> = SwitchId::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(list("BSC_WALLET_IDS"), names(switches[..2].to_vec()));
    assert_eq!(list("BSC_ECONOMIC_IDS"), names(switches[2..].to_vec()));
    assert_eq!(
        list("BSC_LIVE_STRATEGIES"),
        names(LiveStrategies::ALL.iter().map(|s| s.name()).collect())
    );
    assert_eq!(
        list("BSC_LIVE_ADAPTERS"),
        names(LiveAdapter::ALL.iter().map(|a| a.name()).collect())
    );
}

/// Audit E 4.8: one grant vector for the owner and the LIVE tab
/// (`tests/fixtures/live_grant/`, which `live.test.ts` reads too): the
/// canonical scope text and its digest byte for byte, and every variant
/// of `refused.txt` refused by the owner (the form refuses the same).
#[test]
fn the_grant_vector_is_the_owners() {
    const SCOPE: &str = include_str!("../../tests/fixtures/live_grant/scope.txt");
    const REFUSED: &str = include_str!("../../tests/fixtures/live_grant/refused.txt");
    let g = grant_of_text(SCOPE);
    assert_eq!(g.scope_text(), SCOPE);
    assert_eq!(
        hex32(&g.scope_sha256()),
        "567cec77f585f80ab0ab9408bcdb2ccc7db7ba2f3d70d770c379195dea6afceb"
    );
    g.validate_within(10).unwrap();
    let exits = LiveGrant {
        exit_only: true,
        ..g.clone()
    };
    assert_eq!(exits.scope_text(), format!("{SCOPE}exit_only=true\n"));
    assert_eq!(
        hex32(&exits.scope_sha256()),
        "e012c2fd97397c20f75972c2435989cac4ed0843260534b7d020a812f25a2162"
    );
    let mut acked = g.clone();
    acked.risk.as_mut().unwrap().outlier_ack = true;
    assert_eq!(
        acked.scope_text(),
        SCOPE.replace(
            "max_sell_gas_limit=1500000\n",
            "max_sell_gas_limit=1500000\nrisk_outlier_ack=YES\n"
        )
    );
    let mut refused = 0;
    for line in REFUSED
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let key = format!("{}=", line.split_once('=').unwrap().0);
        let text: String = SCOPE
            .lines()
            .map(|l| format!("{}\n", if l.starts_with(&key) { line } else { l }))
            .collect();
        assert!(text.contains(line), "{line}");
        assert!(grant_of_text(&text).validate_within(10).is_err(), "{line}");
        refused += 1;
    }
    assert_eq!(refused, 17);
}

/// BSC3-049: the risk scope is the grant's scope without its validity
/// and exit-only: the same scope again, or for its exits alone, keeps
/// it; any other limit, strategy, source, builder or risk bound changes
/// it. The scope text itself is unchanged by it (the frontend builds the
/// same).
#[test]
fn the_risk_scope_is_the_grant_scope_without_its_validity() {
    let g = grant(1);
    assert_eq!(
        g.scope_text(),
        "darkbot.bsc.live-grant.v1\nvenue=BSC_VENUE_FLAP\nstrategies=C3\n\
             sources=BSC_INGRESS_PUBLICNODE\nadapters=CLUB48_BUILDER_V1\n\
             max_input_wei=1000000000000\nmax_spend_wei=10000000000000000\n\
             max_attempts=2\nexpires_unix_ms=1\nmax_buy_slippage_bps=1000\n\
             max_exit_slippage_bps=1000\nmax_gas_price_wei=1000000000000\n\
             max_buy_gas_limit=10000000\nmax_approve_gas_limit=1000000\n\
             max_sell_gas_limit=5000000\n\
             max_open_positions=16\nmax_realized_loss_wei=1000000000000000000\n\
             max_daily_realized_loss_wei=1000000000000000000\nmin_buy_interval_ms=0\n"
    );
    let pin = darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_2();
    assert_eq!(
        g.risk_scope_text(),
        g.scope_text()
            .replace(
                "darkbot.bsc.live-grant.v1",
                "darkbot.bsc.live-risk-scope.v1"
            )
            .replace("expires_unix_ms=1\n", "")
            + &format!(
                "venue_code={}:{}\n",
                pin.portal_implementation,
                hex32(&pin.portal_implementation_runtime_sha256)
            )
    );
    let mut again = grant(2);
    again.exit_only = true;
    assert_ne!(again.scope_sha256(), g.scope_sha256());
    assert_eq!(again.risk_scope_sha256(), g.risk_scope_sha256());
    let others = [
        LiveGrant {
            max_spend_wei: g.max_spend_wei + 1,
            ..g.clone()
        },
        LiveGrant {
            max_input_wei: g.max_input_wei + 1,
            ..g.clone()
        },
        LiveGrant {
            max_attempts: g.max_attempts + 1,
            ..g.clone()
        },
        grant_of(1, LiveStrategies::StrategyC),
        LiveGrant {
            sources: vec![SourceId::Drpc],
            ..g.clone()
        },
        LiveGrant {
            adapters: vec![LiveAdapter::BlockRazorBuilderV1],
            ..g.clone()
        },
        LiveGrant {
            risk: Some(GrantRisk {
                max_gas_price_wei: 1,
                ..TEST_RISK
            }),
            ..g.clone()
        },
        LiveGrant {
            risk: Some(GrantRisk {
                max_exit_slippage_bps: 1,
                ..TEST_RISK
            }),
            ..g.clone()
        },
        LiveGrant {
            risk: Some(GrantRisk {
                max_sell_gas_limit: 1,
                ..TEST_RISK
            }),
            ..g.clone()
        },
    ];
    for other in others {
        assert_ne!(other.risk_scope_sha256(), g.risk_scope_sha256());
    }
}

/// BSC3-069: a grant names the execution risk the operator confirms:
/// its scope text carries every bound (another risk, another digest),
/// the owner record keeps it, a bound no transaction is signed within
/// is refused; a new grant without one is refused, one held without it
/// admits nothing, exits neither (GRANT_RISK); every bound an executor
/// is beyond is named (a grant of exits only: not the BUY's).
#[test]
fn a_grant_names_the_execution_risk_it_admits() {
    let (dir, s, now) = live_ready_granting("live-risk", |now| {
        grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified();
    let g = s.current().unwrap();
    let grant = g.live.clone().unwrap();
    assert_eq!(grant.risk, Some(TEST_RISK));
    assert!(grant.scope_text().ends_with(
            "max_buy_slippage_bps=1000\nmax_exit_slippage_bps=1000\nmax_gas_price_wei=1000000000000\n\
             max_buy_gas_limit=10000000\nmax_approve_gas_limit=1000000\nmax_sell_gas_limit=5000000\n\
             max_open_positions=16\nmax_realized_loss_wei=1000000000000000000\n\
             max_daily_realized_loss_wei=1000000000000000000\nmin_buy_interval_ms=0\n"
        ));
    let tighter = LiveGrant {
        risk: Some(GrantRisk {
            max_buy_slippage_bps: 999,
            ..TEST_RISK
        }),
        ..grant.clone()
    };
    assert_ne!(tighter.scope_sha256(), grant.scope_sha256());
    assert!(s.live_now(&[ComponentId::ExecC3], &q, now).is_ok());
    for bad in [
        GrantRisk {
            max_buy_slippage_bps: 10_000,
            ..TEST_RISK
        },
        GrantRisk {
            max_exit_slippage_bps: 10_000,
            ..TEST_RISK
        },
        GrantRisk {
            max_gas_price_wei: 0,
            ..TEST_RISK
        },
        GrantRisk {
            max_approve_gas_limit: 20_999,
            ..TEST_RISK
        },
    ] {
        let refused = LiveGrant {
            risk: Some(bad),
            ..grant.clone()
        };
        assert!(refused.validate().is_err(), "{bad:?}");
    }
    // Every bound on its own: at the granted value it admits, one above
    // it it names that bound; several at once are all named; a grant
    // of exits only ignores the BUY's two.
    let signed = SignedRisk {
        buy_slippage_bps: 1_000,
        exit_slippage_bps: 1_000,
        gas_price_wei: 1_000_000_000_000,
        buy_gas_limit: 10_000_000,
        approve_gas_limit: 1_000_000,
        sell_gas_limit: 5_000_000,
    };
    assert_eq!(TEST_RISK.admits(&signed, false), Ok(()));
    let cases: [(&str, SignedRisk, bool); 6] = [
        (
            "buy_slippage_bps 1001 above 1000",
            SignedRisk {
                buy_slippage_bps: 1_001,
                ..signed
            },
            true,
        ),
        (
            "exit_slippage_bps 1001 above 1000",
            SignedRisk {
                exit_slippage_bps: 1_001,
                ..signed
            },
            false,
        ),
        (
            "gas_price_wei 1000000000001 above 1000000000000",
            SignedRisk {
                gas_price_wei: 1_000_000_000_001,
                ..signed
            },
            false,
        ),
        (
            "buy_gas_limit 10000001 above 10000000",
            SignedRisk {
                buy_gas_limit: 10_000_001,
                ..signed
            },
            true,
        ),
        (
            "approve_gas_limit 1000001 above 1000000",
            SignedRisk {
                approve_gas_limit: 1_000_001,
                ..signed
            },
            false,
        ),
        (
            "sell_gas_limit 5000001 above 5000000",
            SignedRisk {
                sell_gas_limit: 5_000_001,
                ..signed
            },
            false,
        ),
    ];
    for (named, over, buy_only) in cases {
        assert_eq!(
            TEST_RISK.admits(&over, false),
            Err(format!("{LIVE_RISK_OUTSIDE_GRANT}: {named}"))
        );
        assert_eq!(TEST_RISK.admits(&over, true).is_ok(), buy_only, "{named}");
    }
    assert_eq!(
        TEST_RISK.admits(
            &SignedRisk {
                buy_slippage_bps: 1_001,
                sell_gas_limit: 5_000_001,
                ..signed
            },
            false
        ),
        Err(format!(
            "{LIVE_RISK_OUTSIDE_GRANT}: buy_slippage_bps 1001 above 1000; sell_gas_limit 5000001 above 5000000"
        ))
    );
    for bad in [
        GrantRisk {
            max_buy_gas_limit: 20_999,
            ..TEST_RISK
        },
        GrantRisk {
            max_sell_gas_limit: 20_999,
            ..TEST_RISK
        },
    ] {
        let refused = LiveGrant {
            risk: Some(bad),
            ..grant.clone()
        };
        assert!(refused.validate().is_err(), "{bad:?}");
    }
    // A new grant without the risk is refused, nothing written.
    let none = LiveGrant {
        risk: None,
        ..grant.clone()
    };
    let before = s.current().unwrap().generation;
    assert!(
        s.grant_live("no-risk", none.clone(), none.scope_sha256(), before, now)
            .unwrap_err()
            .starts_with(LIVE_GRANT_RISK_MISSING)
    );
    assert_eq!(s.current().unwrap().generation, before);
    // A grant held without one (written before it): nothing is LIVE,
    // exits neither.
    let mut g = s.current().unwrap();
    g.live.as_mut().unwrap().risk = None;
    let (_, daily, total) = live_inputs(&dir, now, max()).unwrap();
    let ctx = LiveContext {
        boot: Some("boot-1"),
        now_unix_ms: now,
        daily: Some(&daily),
        total: Some(&total),
        spent_wei: Some(0),
        qualification: &q,
    };
    assert!(
        g.live_checklist(&ctx)
            .contains(&("GRANT_RISK", Some(LIVE_GRANT_RISK_MISSING)))
    );
    assert!(g.live_blockers(&ctx).contains(&LIVE_GRANT_RISK_MISSING));
    assert!(g.exit_blockers(&ctx).contains(&LIVE_GRANT_RISK_MISSING));
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-058: the operator's separate EXIT-ONLY grant. Its scope says
/// so (another digest than the same scope with BUYs; a grant with BUYs
/// keeps its text) and survives the owner's record; under it no BUY is
/// admitted (LIVE_GRANT_EXIT_ONLY, nothing consumed), the gate for BUYs
/// is blocked and names it, the exits are admitted. A restart and its
/// expiry still end it: nothing economic goes by itself.
#[test]
fn an_exit_only_grant_admits_exits_and_no_buy() {
    let (dir, s, now) = live_ready_granting("live-exit-only", |now| LiveGrant {
        exit_only: true,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified();
    let g = s.current().unwrap();
    let grant = g.live.clone().unwrap();
    assert!(grant.exit_only);
    assert!(grant.scope_text().ends_with("exit_only=true\n"));
    let with_buys = LiveGrant {
        exit_only: false,
        ..grant.clone()
    };
    assert!(!with_buys.scope_text().contains("exit_only"));
    assert_ne!(grant.scope_sha256(), with_buys.scope_sha256());
    assert_eq!(
        s.live_now(&[ComponentId::ExecC3], &q, now).unwrap_err(),
        LIVE_GRANT_EXIT_ONLY
    );
    let (_, daily, total) = live_inputs(&dir, now, max()).unwrap();
    let checklist = g.live_checklist(&LiveContext {
        boot: Some("boot-1"),
        now_unix_ms: now,
        daily: Some(&daily),
        total: Some(&total),
        spent_wei: Some(0),
        qualification: &q,
    });
    assert!(checklist.contains(&("GRANT_BUYS", Some(LIVE_GRANT_EXIT_ONLY))));
    assert_eq!(checklist.iter().filter(|(_, r)| r.is_some()).count(), 1);
    let armed = s.live_now_exits(&[ComponentId::ExecC3], &q, now).unwrap();
    let ids: Vec<String> = (0..3u8).map(|n| hex32(&[n; 32])).collect();
    let intent = |n: usize, kind: LiveIntentKind| LiveIntent {
        id: &ids[n],
        component: ComponentId::ExecC3,
        kind,
        input_wei: if kind == LiveIntentKind::Buy {
            1_000
        } else {
            0
        },
        max_wei: 2_000,
        token: [0x10; 20],
        nonce: 40 + n as u64,
        origin: ORIGIN,
        max_block: 900,
        exit_reserve_wei: 0,
        venue: Some(VenueId::Flap),
    };
    assert_eq!(
        s.admit_live(&armed, &intent(0, LiveIntentKind::Buy), &q, now)
            .unwrap_err(),
        LIVE_GRANT_EXIT_ONLY
    );
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 0);
    assert_eq!(spent_by_grant(&dir, armed.grant_generation()).unwrap(), 0);
    assert!(
        s.admit_live(&armed, &intent(1, LiveIntentKind::Approve), &q, now)
            .is_ok()
    );
    // Expired: no exit either.
    let late = grant.expires_unix_ms;
    assert_eq!(
        s.live_now_exits(&[ComponentId::ExecC3], &q, late)
            .unwrap_err(),
        LIVE_GRANT_EXPIRED
    );
    // A restart (another boot session): the switches and the grant are
    // reset, no exit until the operator authorizes again.
    let restarted = Store::open_in_session(&dir, keeping(8), Ok("boot-2".into())).unwrap();
    assert!(
        restarted
            .live_now_exits(&[ComponentId::ExecC3], &q, now)
            .is_err()
    );
    let after = g.exit_blockers(&LiveContext {
        boot: Some("boot-2"),
        now_unix_ms: now,
        daily: Some(&daily),
        total: Some(&total),
        spent_wei: Some(0),
        qualification: &q,
    });
    assert!(after.contains(&LIVE_GRANT_RESET_BY_RESTART), "{after:?}");
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-057: a BUY leaves the grant the room the exits of what is open
/// and of its own position need (approval and SELL at the signed gas);
/// a BUY that would not is refused before any byte leaves (before: the
/// BUY was admitted and its SELL refused LIVE_GRANT_SPEND_EXHAUSTED).
#[test]
fn a_buy_leaves_the_grant_room_for_its_exit() {
    let (dir, s, now) = live_ready("live-exit-reserve");
    let q = qualified();
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let most = armed.grant().max_spend_wei;
    let ids: Vec<String> = (0..4u8).map(|n| hex32(&[n; 32])).collect();
    let (approve, sell) = (100u128, 500u128);
    let intent = |n: usize, kind: LiveIntentKind, max: u128| LiveIntent {
        id: &ids[n],
        component: ComponentId::ExecC3,
        kind,
        input_wei: if kind == LiveIntentKind::Buy {
            1_000
        } else {
            0
        },
        max_wei: max,
        token: [0x10; 20],
        nonce: 40 + n as u64,
        origin: ORIGIN,
        max_block: 900,
        exit_reserve_wei: if kind == LiveIntentKind::Buy {
            approve + sell
        } else {
            0
        },
        venue: Some(VenueId::Flap),
    };
    // A BUY that leaves less than its exit costs is refused, nothing
    // consumed.
    let short = intent(0, LiveIntentKind::Buy, most - approve - sell + 1);
    assert_eq!(
        s.admit_live(&armed, &short, &q, now).unwrap_err(),
        LIVE_EXIT_RESERVE_MISSING
    );
    assert_eq!(spent_by_grant(&dir, armed.grant_generation()).unwrap(), 0);
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 0);
    // One that leaves exactly that: it, its approval and its SELL go.
    let buy = intent(1, LiveIntentKind::Buy, most - approve - sell);
    assert!(s.admit_live(&armed, &buy, &q, now).is_ok());
    let approval = s.admit_live(
        &armed,
        &intent(2, LiveIntentKind::Approve, approve),
        &q,
        now,
    );
    assert!(approval.is_ok(), "{approval:?}");
    let sold = s.admit_live(&armed, &intent(3, LiveIntentKind::Sell, sell), &q, now);
    assert_eq!(sold.unwrap().spent_wei, most);
    fs::remove_dir_all(dir).unwrap();
}

/// Plan 28.10.: the owner admits each LIVE intent as its state holds
/// at that moment: a BUY takes its daily and campaign slot and its spend
/// entry before any byte can leave; an exit needs no BUY attempt; the
/// same transaction never twice; another grant, a restart or a scope
/// that does not name the component admits nothing.
#[test]
fn each_live_intent_is_admitted_by_the_owner_as_it_holds_now() {
    let (dir, s, now) = live_ready("live-admit");
    let q = qualified();
    // Strategy C, the exit decider of the C3 grant's lots (R1).
    assert!(s.live_now(&[ComponentId::StrategyC], &q, now).is_ok());
    assert_eq!(
        s.live_now(&[ComponentId::ExecC3], &LiveQualification::default(), now)
            .unwrap_err(),
        LIVE_VENUE_UNQUALIFIED
    );
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let grant = armed.grant().clone();
    let generation = armed.grant_generation();
    let id = |n: u8| hex32(&[n; 32]);
    let ids: Vec<String> = (0..8).map(id).collect();
    let intent = |n: usize, kind: LiveIntentKind, input: u128, most: u128| LiveIntent {
        id: &ids[n],
        component: ComponentId::ExecC3,
        kind,
        input_wei: input,
        max_wei: most,
        token: [0x10 + n as u8; 20],
        nonce: 40 + n as u64,
        origin: ORIGIN,
        max_block: 900 + n as u64,
        exit_reserve_wei: 0,
        venue: Some(VenueId::Flap),
    };
    let buy = |n| intent(n, LiveIntentKind::Buy, 1_000, 2_000);
    let first = s.admit_live(&armed, &buy(0), &q, now).unwrap();
    assert_eq!(first.spent_wei, 2_000);
    let (day, total) = first.slots.unwrap();
    assert_eq!((day.slot, total.slot), (0, 0));
    assert_eq!(spent_by_grant(&dir, generation).unwrap(), 2_000);
    // BSC3-024: the admission is the transaction's write-ahead record,
    // with its token and nonce.
    assert_eq!(
        admitted_intents(&dir).unwrap(),
        vec![Admitted {
            tx_hash: [0; 32],
            kind: LiveIntentKind::Buy,
            component: ComponentId::ExecC3,
            grant_generation: generation,
            input_wei: 1_000,
            max_wei: 2_000,
            token: Some([0x10; 20]),
            nonce: Some(40),
            max_block: Some(900),
            origin: Some(ORIGIN),
            lease: false,
            venue: Some(VenueId::Flap),
        }]
    );
    assert_eq!(
        s.admit_live(&armed, &buy(0), &q, now).unwrap_err(),
        LIVE_INTENT_SEEN
    );
    assert_eq!(
        s.admit_live(
            &armed,
            &intent(1, LiveIntentKind::Buy, grant.max_input_wei + 1, 1),
            &q,
            now
        )
        .unwrap_err(),
        LIVE_INPUT_ABOVE_GRANT
    );
    assert_eq!(
        s.admit_live(
            &armed,
            &intent(1, LiveIntentKind::Buy, 1, grant.max_spend_wei),
            &q,
            now
        )
        .unwrap_err(),
        LIVE_GRANT_SPEND_EXHAUSTED
    );
    assert!(s.admit_live(&armed, &buy(1), &q, now).is_ok());
    // The day's two BUYs are taken: no third BUY, the exit still.
    assert_eq!(
        s.admit_live(&armed, &buy(2), &q, now).unwrap_err(),
        BUY_LIMIT_EXHAUSTED
    );
    let sell = s
        .admit_live(&armed, &intent(3, LiveIntentKind::Sell, 0, 500), &q, now)
        .unwrap();
    assert_eq!((sell.slots, sell.spent_wei), (None, 4_500));
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 2);
    // Not the grant armed under; not in the scope; evidence missing.
    let other = LiveNow {
        grant_generation: generation + 1,
        ..armed.clone()
    };
    let exit = intent(4, LiveIntentKind::Sell, 0, 1);
    assert_eq!(
        s.admit_live(&other, &exit, &q, now).unwrap_err(),
        LIVE_GRANT_CHANGED
    );
    // 01.11. (R1): Strategy C decides the C3 grant's exits (admitted);
    // its BUY is outside the scope.
    let strategy_exit = LiveIntent {
        component: ComponentId::StrategyC,
        ..intent(5, LiveIntentKind::Sell, 0, 1)
    };
    assert!(s.admit_live(&armed, &strategy_exit, &q, now).is_ok());
    let strategy_buy = LiveIntent {
        component: ComponentId::StrategyC,
        ..intent(6, LiveIntentKind::Buy, 1, 1)
    };
    assert_eq!(
        s.admit_live(&armed, &strategy_buy, &q, now).unwrap_err(),
        LIVE_NOT_IN_SCOPE
    );
    let none = LiveQualification::default();
    assert_eq!(
        s.admit_live(&armed, &exit, &none, now).unwrap_err(),
        LIVE_VENUE_UNQUALIFIED
    );
    // After a restart nothing is LIVE, an exit neither.
    let restarted = Store::open_in_session(&dir, keeping(8), Ok("boot-2".into())).unwrap();
    assert_eq!(
        restarted.admit_live(&armed, &exit, &q, now).unwrap_err(),
        ECONOMIC_LIVE_DISABLED
    );
    assert_eq!(
        restarted
            .live_now(&[ComponentId::ExecC3], &q, now)
            .unwrap_err(),
        ECONOMIC_LIVE_DISABLED
    );
    assert_eq!(spent_by_grant(&dir, generation).unwrap(), 4_501);
    fs::remove_dir_all(dir).unwrap();
}

/// A LIVE BUY of the tests' buyer at `nonce`, named `id`, of at most
/// `most`.
fn nonce_buy(id: &str, nonce: u64, most: u128) -> LiveIntent<'_> {
    LiveIntent {
        id,
        component: ComponentId::ExecC3,
        kind: LiveIntentKind::Buy,
        input_wei: 1_000,
        max_wei: most,
        token: [0x10; 20],
        nonce,
        origin: ORIGIN,
        max_block: 900,
        exit_reserve_wei: 0,
        venue: Some(VenueId::Flap),
    }
}

/// An admitted intent `n` of grant `grant` as an entry holds it.
fn admitted_of(
    n: u8,
    grant: u64,
    nonce: Option<u64>,
    origin: Option<NonceOrigin>,
    most: u128,
) -> Admitted {
    Admitted {
        tx_hash: [n; 32],
        kind: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        grant_generation: grant,
        input_wei: 1,
        max_wei: most,
        token: Some([0x10; 20]),
        nonce,
        max_block: Some(900),
        origin,
        lease: false,
        venue: None,
    }
}

/// `a`'s spend entry as the owner writes it, or (no origin) as an older
/// build wrote it (v1, no sender and chain).
fn spend_entry(dir: &Path, a: &Admitted) {
    use darkbot_bsc_record::Value;
    use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
    let mut d = Draft::new(match a.origin {
        Some(_) => &ls::V2,
        None => &ls::V1,
    });
    d.set(ls::INTENT_ID, hex32(&a.tx_hash).as_str())
        .set(ls::KIND, a.kind.name())
        .set(ls::COMPONENT, a.component.name())
        .set(ls::PROVENANCE_TOOL, "test")
        .set(ls::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(ls::GRANT_GENERATION, a.grant_generation)
        .set(ls::INPUT_WEI, Value::u128(a.input_wei))
        .set(ls::MAX_WEI, Value::u128(a.max_wei))
        .set_opt(ls::TOKEN, a.token.map(Value::Address))
        .set_opt(ls::NONCE, a.nonce)
        .set_opt(ls::MAX_BLOCK, a.max_block);
    if let Some(o) = a.origin {
        d.set(ls::CHAIN_ID, o.chain_id)
            .set(ls::SENDER, Value::Address(o.sender));
    }
    create_durable_dir(&spend_dir(dir)).unwrap();
    let path = spend_dir(dir).join(format!("{}.dbrec", hex32(&a.tx_hash)));
    fs::write(path, d.encode().unwrap()).unwrap();
}

/// SC4 d review L4: the owner's admission names the intent's venue (a
/// lot recovered after a crash is its venue's) and admits a BUY on the
/// grant's venue alone; an exit of a lot on another venue goes on. An
/// entry before the venue (v2) reads without one.
#[test]
fn an_admission_names_its_venue_and_a_buy_is_the_grants() {
    let (dir, s, now) = live_ready_limited("live-venue-admit", (6, 6), |now| LiveGrant {
        max_attempts: 6,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let ids: Vec<String> = (0..4u8).map(|n| hex32(&[n; 32])).collect();
    let on = |n: usize, nonce: u64, venue: VenueId| LiveIntent {
        venue: Some(venue),
        ..nonce_buy(&ids[n], nonce, 2_000)
    };
    assert_eq!(
        s.admit_live(&armed, &on(0, 7, VenueId::OpenFour), &q, now)
            .unwrap_err(),
        LIVE_NOT_IN_SCOPE
    );
    // Review L4: a BUY naming no venue is none of the grant's.
    let unnamed = LiveIntent {
        venue: None,
        ..on(0, 7, VenueId::Flap)
    };
    assert_eq!(
        s.admit_live(&armed, &unnamed, &q, now).unwrap_err(),
        LIVE_NOT_IN_SCOPE
    );
    s.admit_live(&armed, &on(1, 7, VenueId::Flap), &q, now)
        .unwrap();
    let exit = LiveIntent {
        kind: LiveIntentKind::Sell,
        input_wei: 0,
        ..on(2, 8, VenueId::OpenFour)
    };
    s.admit_live(&armed, &exit, &q, now).unwrap();
    let mut read: Vec<(u8, Option<VenueId>)> = admitted_intents(&dir)
        .unwrap()
        .iter()
        .map(|a| (a.tx_hash[0], a.venue))
        .collect();
    read.sort_by_key(|(n, _)| *n);
    assert_eq!(
        read,
        [(1, Some(VenueId::Flap)), (2, Some(VenueId::OpenFour))]
    );
    // The ledger's recovery of an admitted Four exit keeps its venue.
    let recovered = admitted_intents(&dir)
        .unwrap()
        .into_iter()
        .find(|a| a.tx_hash[0] == 2)
        .and_then(|a| crate::live_ledger::Fact::admitted(&a, crate::live_ledger::FactKind::Unknown))
        .unwrap();
    assert_eq!(recovered.venue, Some(VenueId::OpenFour));
    // v2: no venue.
    spend_entry(&dir, &admitted_of(3, 1, Some(9), Some(ORIGIN), 1));
    assert!(
        admitted_intents(&dir)
            .unwrap()
            .iter()
            .any(|a| a.tx_hash[0] == 3 && a.venue.is_none())
    );
    fs::remove_dir_all(dir).unwrap();
}

/// Budget fix (tests A, M, G): the alternatives of one (chain, sender,
/// nonce) spend their most once, whichever lands: a higher one adds only
/// its difference, an equal or lower one nothing; another nonce adds its
/// own most; the grant is checked against all of it. Every entry stays,
/// and every BUY still takes its own daily, campaign and grant slot.
#[test]
fn alternatives_of_one_nonce_spend_only_their_most() {
    let (dir, s, now) = live_ready_limited("live-nonce-alt", (6, 6), |now| LiveGrant {
        max_attempts: 6,
        max_input_wei: 1_000,
        max_spend_wei: 21_000,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let generation = armed.grant_generation();
    let ids: Vec<String> = (0..10u8).map(|n| hex32(&[n; 32])).collect();
    let admit = |n: usize, nonce: u64, most: u128| {
        s.admit_live(&armed, &nonce_buy(&ids[n], nonce, most), &q, now)
            .map(|a| a.spent_wei)
    };
    assert_eq!(admit(0, 7, 10_000), Ok(10_000));
    assert_eq!(admit(1, 7, 12_000), Ok(12_000));
    assert_eq!(admit(2, 7, 11_000), Ok(12_000));
    assert_eq!(admit(3, 7, 12_000), Ok(12_000));
    assert_eq!(spent_by_grant(&dir, generation).unwrap(), 12_000);
    // Nonce 8: its own most on top; 12k + 9,001 is past the grant.
    assert_eq!(admit(4, 8, 9_001), Err(LIVE_GRANT_SPEND_EXHAUSTED.into()));
    assert_eq!(admit(5, 8, 7_999), Ok(19_999));
    // Above nonce 7's most only the difference counts: 1,002 more is
    // past the grant, 1,001 fills it.
    assert_eq!(admit(6, 7, 13_002), Err(LIVE_GRANT_SPEND_EXHAUSTED.into()));
    assert_eq!(admit(6, 7, 13_001), Ok(21_000));
    // An exit alternative of nonce 8 within its most still fits the
    // full grant.
    let exit = LiveIntent {
        kind: LiveIntentKind::Approve,
        input_wei: 0,
        ..nonce_buy(&ids[7], 8, 7_999)
    };
    assert_eq!(
        s.admit_live(&armed, &exit, &q, now).unwrap().spent_wei,
        21_000
    );
    // Each BUY took its slots, whatever it added (two added nothing):
    // the day's six are gone.
    assert_eq!(admit(8, 7, 1), Err(BUY_LIMIT_EXHAUSTED.into()));
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 6);
    let total = total_usage(&dir, max()).unwrap();
    assert_eq!((total.used, total.grants), (6, vec![generation; 6]));
    // Every admitted entry stays, refused ones were never written.
    assert_eq!(admitted_intents(&dir).unwrap().len(), 7);
    assert_eq!(spent_by_grant(&dir, generation).unwrap(), 21_000);
    fs::remove_dir_all(dir).unwrap();
}

/// REV.1.0 R4: the campaign's cumulative budget bounds the BUYs of every
/// grant together (an older grant's admissions count, an alternative
/// of one nonce adds its difference only); an exit still goes beyond it;
/// a store without it bounds no campaign.
#[test]
fn the_campaign_budget_bounds_the_buys_of_every_grant() {
    let (dir, s, now) = live_ready_limited("live-campaign-spend", (6, 6), |now| LiveGrant {
        max_attempts: 6,
        max_input_wei: 1_000,
        max_spend_wei: 21_000,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let older = armed.grant_generation() - 1;
    spend_entry(&dir, &admitted_of(9, older, Some(3), Some(ORIGIN), 5_000));
    let ids: Vec<String> = (0..5u8).map(|n| hex32(&[n; 32])).collect();
    let unbounded = s.admit_live(&armed, &nonce_buy(&ids[4], 6, 10_000), &q, now);
    assert_eq!(unbounded.map(|a| a.spent_wei), Ok(10_000));
    let s = s.with_campaign_spend(17_000);
    let admit = |n: usize, nonce: u64, most: u128| {
        s.admit_live(&armed, &nonce_buy(&ids[n], nonce, most), &q, now)
            .map(|a| a.spent_wei)
    };
    // 5,000 + 10,000 of the campaign: 2,001 more is past it (the
    // grant's 21,000 not).
    assert_eq!(
        admit(0, 7, 2_001),
        Err(LIVE_CAMPAIGN_SPEND_EXHAUSTED.into())
    );
    assert_eq!(admit(0, 7, 1_000), Ok(11_000));
    // An alternative of nonce 7 adds its difference only.
    assert_eq!(admit(1, 7, 2_000), Ok(12_000));
    assert_eq!(
        admit(2, 7, 2_001),
        Err(LIVE_CAMPAIGN_SPEND_EXHAUSTED.into())
    );
    assert_eq!(spent_total(&dir).unwrap(), 17_000);
    // Review M2: a BUY keeps the campaign's room for its exits.
    let s = s.with_campaign_spend(18_000);
    let reserved = |most: u128, reserve: u128| LiveIntent {
        exit_reserve_wei: reserve,
        ..nonce_buy(&ids[2], 9, most)
    };
    assert_eq!(
        s.admit_live(&armed, &reserved(500, 501), &q, now)
            .map(|a| a.spent_wei),
        Err(LIVE_CAMPAIGN_SPEND_EXHAUSTED.into())
    );
    assert_eq!(
        s.admit_live(&armed, &reserved(500, 500), &q, now)
            .map(|a| a.spent_wei),
        Ok(12_500)
    );
    assert_eq!(spent_total(&dir).unwrap(), 17_500);
    // An exit beyond it still goes.
    let exit = LiveIntent {
        kind: LiveIntentKind::Approve,
        input_wei: 0,
        ..nonce_buy(&ids[3], 8, 3_000)
    };
    assert!(s.admit_live(&armed, &exit, &q, now).is_ok());
    assert_eq!(spent_total(&dir).unwrap(), 20_500);
    fs::remove_dir_all(dir).unwrap();
}

/// The spend lock is released though a child forked during an admission
/// holds a copy of its file until its exec: the next admission takes it
/// (the Linux CI flake "held by another admission" of nobody's). A
/// second admission meanwhile, and another description's lock (another
/// process's), still refuse it; a replaced state's file is the one
/// locked.
#[test]
fn the_spend_lock_outlives_a_forked_copy_of_its_file() {
    let (dir, s) = session("spend-lock-fork", "boot-1");
    let held = |r: Result<SpendGuard, String>| {
        r.err()
            .is_some_and(|e| e.contains("held by another admission"))
    };
    let g = s.spend_lock().unwrap();
    let child = g.file.try_clone().unwrap();
    assert!(held(s.spend_lock()));
    drop(g);
    let again = s.spend_lock();
    assert!(again.is_ok(), "{:?}", again.err());
    drop(again);
    drop(child);
    let other = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("live-spend.lock"))
        .unwrap();
    other.try_lock().unwrap();
    assert!(held(s.spend_lock()));
    drop(other);
    // The state replaced: its new file is locked, not the old one.
    fs::remove_file(dir.join("live-spend.lock")).unwrap();
    let g = s.spend_lock().unwrap();
    let other = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("live-spend.lock"))
        .unwrap();
    assert!(other.try_lock().is_err());
    drop(g);
    fs::remove_dir_all(dir).unwrap();
}

/// REV.1.0 R4 (BSC3-069): a risk confirmed as an outlier says so in
/// both scopes (a line only then: every other grant's scope stays the
/// same) and is kept by the owner's record.
#[test]
fn an_outlier_risk_is_confirmed_in_its_scope_and_kept() {
    let plain = grant(1);
    let acked = LiveGrant {
        risk: Some(GrantRisk {
            outlier_ack: true,
            ..TEST_RISK
        }),
        ..plain.clone()
    };
    let line = "max_sell_gas_limit=5000000\n";
    assert_eq!(
        acked.scope_text(),
        plain
            .scope_text()
            .replace(line, &format!("{line}risk_outlier_ack=YES\n"))
    );
    assert!(acked.risk_scope_text().contains("risk_outlier_ack=YES\n"));
    assert!(!plain.risk_scope_text().contains("risk_outlier_ack"));
    assert_ne!(acked.risk_scope_sha256(), plain.risk_scope_sha256());
    let (dir, s, _) = live_ready_limited("outlier-ack", (2, 2), |now| LiveGrant {
        risk: Some(GrantRisk {
            outlier_ack: true,
            ..TEST_RISK
        }),
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let kept = s.current().unwrap().live.unwrap().risk.unwrap();
    assert!(kept.outlier_ack);
    fs::remove_dir_all(dir).unwrap();
}

/// Budget fix (tests I, J, K): only one (chain, sender, nonce) groups.
/// The same nonce under another grant counts there alone; of another
/// sender or chain, in full; an entry without its sender and chain (an
/// older build's) in full, and never with another of its nonce.
#[test]
fn only_one_chain_sender_and_nonce_groups() {
    let dir = std::env::temp_dir().join(format!("bsc-spend-groups-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let sender = NonceOrigin {
        sender: [0xb1; 20],
        ..ORIGIN
    };
    let chain = NonceOrigin {
        chain_id: 97,
        ..ORIGIN
    };
    for a in [
        admitted_of(1, 5, Some(7), Some(ORIGIN), 10_000),
        admitted_of(2, 6, Some(7), Some(ORIGIN), 12_000),
        admitted_of(3, 5, Some(7), Some(sender), 3_000),
        admitted_of(4, 5, Some(7), Some(chain), 4_000),
        admitted_of(5, 5, Some(7), None, 5_000),
        admitted_of(6, 5, Some(7), None, 6_000),
        admitted_of(7, 5, None, None, 700),
    ] {
        spend_entry(&dir, &a);
    }
    let old = 5_000 + 6_000 + 700;
    assert_eq!(
        spent_by_grant(&dir, 5).unwrap(),
        10_000 + 3_000 + 4_000 + old
    );
    assert_eq!(spent_by_grant(&dir, 6).unwrap(), 12_000);
    let read = admitted_intents(&dir).unwrap();
    assert_eq!(read.iter().filter(|a| a.origin.is_none()).count(), 3);
    assert_eq!(read.iter().filter(|a| a.key().is_some()).count(), 4);
    // An alternative of grant 5's nonce 7 above its most adds the
    // difference only.
    spend_entry(&dir, &admitted_of(8, 5, Some(7), Some(ORIGIN), 11_000));
    assert_eq!(
        spent_by_grant(&dir, 5).unwrap(),
        11_000 + 3_000 + 4_000 + old
    );
    assert_eq!(spent_by_grant(&dir, 6).unwrap(), 12_000);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-076: a lease is the owner's admission of a C3 BUY before its
/// launch (its slots under its attempt, its spend at its nonce): only of
/// a C3 BUY; another lease under the same attempt takes no second slot;
/// a lease returned unused spends and recovers as nothing (its slots
/// stay taken); a return is idempotent, and a record that is not named
/// by the lease it returns returns none.
#[test]
fn a_lease_admits_ahead_and_goes_back_unused() {
    let (dir, s, now) = live_ready_limited("live-lease", (5, 5), |now| LiveGrant {
        max_attempts: 5,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let attempt = hex32(&[0xa0; 32]);
    let (one, two) = (hex32(&[0xa1; 32]), hex32(&[0xa2; 32]));
    let lease = |id| LiveIntent {
        token: [0; 20],
        ..nonce_buy(id, 7, 1_000)
    };
    // Only a C3 BUY is leased.
    let exit = LiveIntent {
        kind: LiveIntentKind::Approve,
        ..lease(&one)
    };
    assert!(s.lease_live(&armed, &exit, &attempt, &q, now).is_err());
    let strategy = LiveIntent {
        component: ComponentId::StrategyC,
        ..lease(&one)
    };
    assert!(s.lease_live(&armed, &strategy, &attempt, &q, now).is_err());
    assert!(
        s.lease_live(&armed, &lease(&one), "not-hex", &q, now)
            .is_err()
    );
    s.lease_live(&armed, &lease(&one), &attempt, &q, now)
        .unwrap();
    // Renewed under the same attempt: its one slot, the nonce's spend once.
    s.lease_live(&armed, &lease(&two), &attempt, &q, now)
        .unwrap();
    assert_eq!(total_usage(&dir, max()).unwrap().used, 1);
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 1);
    let generation = armed.grant_generation;
    assert_eq!(spent_by_grant(&dir, generation).unwrap(), 1_000);
    let leased = admitted_intents(&dir).unwrap();
    assert_eq!(leased.len(), 2);
    assert!(leased.iter().all(|a| a.lease && a.token == Some([0; 20])));
    // Returned: moved whole out of the spend, no spend, no recovery; the
    // slots stay taken; again: nothing more.
    let returns = dir.join("live-lease-returns");
    s.return_leases(&[[0xa1; 32], [0xa2; 32]]).unwrap();
    s.return_leases(&[[0xa1; 32], [0xa2; 32]]).unwrap();
    assert!(admitted_intents(&dir).unwrap().is_empty());
    assert_eq!(spent_by_grant(&dir, generation).unwrap(), 0);
    assert_eq!(total_usage(&dir, max()).unwrap().used, 1);
    assert!(returns.join(format!("{one}.dbrec")).exists());
    assert!(returns.join(format!("{two}.dbrec")).exists());
    // Never a transaction's entry: a BUY entry is refused, kept.
    let tx = hex32(&[0xb1; 32]);
    s.admit_live(&armed, &nonce_buy(&tx, 8, 2_000), &q, now)
        .unwrap();
    assert!(s.return_leases(&[[0xb1; 32]]).is_err());
    assert_eq!(admitted_intents(&dir).unwrap().len(), 1);
    assert_eq!(spent_by_grant(&dir, generation).unwrap(), 2_000);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-078: the tabs read the owner's gate besides the attempt the
/// running LIVE path reports holding, and only while its status is
/// fresh, armed and of the current grant; else every slot counts.
#[test]
fn the_tabs_read_the_gate_besides_the_reported_lease_attempt() {
    use crate::live_qualification::tests::filled;
    use darkbot_bsc_record::Value;
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
    let (dir, s, now) = live_ready_limited("live-lease-tab", (1, 1), |now| LiveGrant {
        max_attempts: 1,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let attempt = [0xa0; 32];
    let id = hex32(&[0xa1; 32]);
    let lease = LiveIntent {
        token: [0; 20],
        ..nonce_buy(&id, 7, 1_000)
    };
    s.lease_live(&armed, &lease, &hex32(&attempt), &q, now)
        .unwrap();
    let g = s.current().unwrap();
    let grant = g.live_set_by.as_ref().unwrap().generation;
    let status = dir.join("status.dbrec");
    let write = |state: &str, generation: u64, written: u64| {
        let mut live = filled(&rs::live::SCHEMA);
        live.set(rs::live::STATE, state)
            .set(rs::live::GRANT_GENERATION, generation)
            .set(rs::live::LEASE_ATTEMPT, Value::B256(attempt));
        let mut r = filled(&rs::SCHEMA);
        r.set(rs::PROVENANCE_OBSERVED_UNIX_MS, written)
            .set(rs::STATUS_INTERVAL_MS, 1_000u64)
            .set(rs::STATUS_STALE_INTERVALS, 3u64)
            .set(rs::LIVE, live.into_nested());
        fs::write(&status, r.encode().unwrap()).unwrap();
    };
    let runtime = |held: Option<String>| {
        let reads = GateReads::read_holding(&dir, &g, now, held.as_deref(), max());
        let ctx = LiveContext {
            boot: Some("boot-1"),
            ..reads.ctx(&q)
        };
        g.effective_live_now(Entity::Runtime, &ctx)
    };
    write("ARMED", grant, now - 2_000);
    let held = reported_held_attempt(&status, &g, now);
    assert_eq!(held, Some(hex32(&attempt)));
    assert_eq!(runtime(held), Effective::Live);
    assert!(matches!(runtime(None), Effective::Blocked(_)));
    // Stale, not armed, of another grant, unreadable: every slot counts.
    for (state, generation, written) in [
        ("ARMED", grant, now - 3_001),
        ("PROBING", grant, now),
        ("ARMED", grant + 1, now),
    ] {
        write(state, generation, written);
        assert_eq!(reported_held_attempt(&status, &g, now), None, "{state}");
    }
    fs::write(&status, b"no record").unwrap();
    assert_eq!(reported_held_attempt(&status, &g, now), None);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-076 review H1/M1: a lease's own attempt refuses nothing with
/// the slots it already holds (the last slot of a day, of the campaign,
/// of the grant: renewed, the gate stays clear); another attempt is
/// refused there. Renewed on a later UTC day it takes that day's slot,
/// within that day's limit; the campaign's stays its one.
#[test]
fn a_lease_holds_its_last_slot_and_takes_each_days() {
    let (dir, s, now) = live_ready_limited("live-lease-last", (1, 2), |now| LiveGrant {
        max_attempts: 1,
        ..grant_of(now + 3 * 86_400_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let attempt = hex32(&[0xa0; 32]);
    let (id_a1, id_a2, id_a3, id_a4) = (
        hex32(&[0xa1; 32]),
        hex32(&[0xa2; 32]),
        hex32(&[0xa3; 32]),
        hex32(&[0xa4; 32]),
    );
    let lease = |id| LiveIntent {
        token: [0; 20],
        ..nonce_buy(id, 7, 1_000)
    };
    s.lease_live(&armed, &lease(&id_a1), &attempt, &q, now)
        .unwrap();
    // The day's, the grant's last slot taken: the gate refuses another
    // attempt, never this one.
    assert!(s.live_now(&[ComponentId::ExecC3], &q, now).is_err());
    assert!(
        s.live_now_holding(&[ComponentId::ExecC3], &q, now, Some(&attempt))
            .is_ok()
    );
    s.lease_live(&armed, &lease(&id_a2), &attempt, &q, now)
        .unwrap();
    let other = hex32(&[0xb0; 32]);
    assert!(
        s.lease_live(&armed, &lease(&id_a3), &other, &q, now)
            .is_err()
    );
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 1);
    assert_eq!(total_usage(&dir, max()).unwrap().used, 1);
    // The next UTC day: renewed, it takes that day's slot.
    let tomorrow = now + 86_400_000;
    s.lease_live(&armed, &lease(&id_a4), &attempt, &q, tomorrow)
        .unwrap();
    assert_eq!(buy_usage(&dir, tomorrow, max()).unwrap().used, 1);
    assert_eq!(total_usage(&dir, max()).unwrap().used, 1);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-023: an admission hands its slots and its entry to the drive,
/// each record's content ordered before its link (a barrier, not a
/// plain fsync), and puts them on the medium with one flush, its last
/// step before it returns (it was a medium flush per file and
/// directory, six for a BUY); a retry's slots found taken have their
/// directory synced again; a G0.7 attempt's slots stay durable each on
/// its own when written.
#[test]
fn an_admission_flushes_the_medium_once_before_it_returns() {
    use darkbot_bsc_journal::durable::{Op, hooks};
    let (dir, s, now) = live_ready_limited("live-flush-once", (5, 5), |now| LiveGrant {
        max_attempts: 5,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let ids: Vec<String> = (0..4u8).map(|n| hex32(&[n; 32])).collect();
    let medium = || MEDIUM.with(|m| std::mem::take(&mut *m.borrow_mut()));
    let spend = spend_dir(&dir);
    let entry = |id: &str| spend.join(format!("{id}.dbrec"));
    let day = buy_day_dir(&dir, &BuyDay::at(now));
    let total = total_dir(&dir);
    let slot = |d: &Path, n: u64| d.join(format!("{n:010}.dbrec"));
    hooks::record();
    // The first creates the directories.
    s.admit_live(&armed, &nonce_buy(&ids[0], 7, 1_000), &q, now)
        .unwrap();
    medium();
    hooks::take();
    s.admit_live(&armed, &nonce_buy(&ids[1], 8, 1_000), &q, now)
        .unwrap();
    // Per slot kind its own slot (the one taken before costs no write)
    // and its directory; then the entry and the spend directory; then
    // the one flush, for both slot directories.
    assert_eq!(
        medium(),
        [
            Medium::Ordered(slot(&day, 1)),
            Medium::Linked(slot(&day, 1)),
            Medium::Handed(day.clone()),
            Medium::Ordered(slot(&total, 1)),
            Medium::Linked(slot(&total, 1)),
            Medium::Handed(total.clone()),
            Medium::Ordered(entry(&ids[1])),
            Medium::Linked(entry(&ids[1])),
            Medium::Handed(spend.clone()),
            Medium::Flush(spend.clone(), vec![day.clone(), total.clone()]),
        ]
    );
    // What the drive was asked, in that order.
    let handed = [Op::Barrier, Op::Fsync];
    assert_eq!(
        hooks::take(),
        [handed, handed, handed]
            .concat()
            .into_iter()
            .chain([Op::FullFlush])
            .collect::<Vec<_>>()
    );
    let exit = LiveIntent {
        kind: LiveIntentKind::Approve,
        input_wei: 0,
        ..nonce_buy(&ids[2], 9, 1_000)
    };
    s.admit_live(&armed, &exit, &q, now).unwrap();
    assert_eq!(
        medium(),
        [
            Medium::Ordered(entry(&ids[2])),
            Medium::Linked(entry(&ids[2])),
            Medium::Handed(spend.clone()),
            Medium::Flush(spend.clone(), vec![]),
        ]
    );
    assert_eq!(hooks::take(), [Op::Barrier, Op::Fsync, Op::FullFlush]);
    // A retry whose slots a failed admission took: their directories
    // synced again before the entry.
    s.consume_attempt_as(&ids[3], armed.grant_generation(), now, Durability::Handed)
        .unwrap();
    medium();
    hooks::take();
    s.admit_live(&armed, &nonce_buy(&ids[3], 10, 1_000), &q, now)
        .unwrap();
    assert_eq!(
        medium(),
        [
            Medium::Handed(day.clone()),
            Medium::Handed(total.clone()),
            Medium::Ordered(entry(&ids[3])),
            Medium::Linked(entry(&ids[3])),
            Medium::Handed(spend.clone()),
            Medium::Flush(spend.clone(), vec![day.clone(), total.clone()]),
        ]
    );
    assert_eq!(
        hooks::take(),
        [Op::Fsync, Op::Fsync, Op::Barrier, Op::Fsync, Op::FullFlush]
    );
    s.consume_buy_attempt("g07-attempt", now).unwrap();
    // Durable each on its own: no write handed over, no device flush.
    assert_eq!(
        medium(),
        [
            Medium::Full(slot(&day, 3)),
            Medium::Linked(slot(&day, 3)),
            Medium::Full(day.clone()),
            Medium::Full(slot(&total, 3)),
            Medium::Linked(slot(&total, 3)),
            Medium::Full(total.clone()),
        ]
    );
    assert_eq!(hooks::take(), []);
    assert_eq!(admitted_intents(&dir).unwrap().len(), 4);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-023: a device flush that fails leaves no admission; its entry
/// stays (it may be on the medium), so the same intent is never
/// admitted again and nothing of it leaves.
#[test]
fn an_admission_whose_flush_fails_is_refused_for_good() {
    let (dir, s, now) = live_ready_granting("live-flush-fails", |now| {
        grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified();
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let id = hex32(&[0x42; 32]);
    let buy = nonce_buy(&id, 7, 1_000);
    FLUSH_FAILS.with(|f| f.set(true));
    let refused = s.admit_live(&armed, &buy, &q, now).unwrap_err();
    assert!(
        refused.starts_with(&format!("{LIVE_SPEND_UNREADABLE}: not on the medium")),
        "{refused}"
    );
    assert!(!FLUSH_FAILS.with(std::cell::Cell::get));
    assert!(matches!(
        MEDIUM.with(|m| m.borrow().last().cloned()),
        Some(Medium::Flush(d, _)) if d == spend_dir(&dir)
    ));
    assert_eq!(
        s.admit_live(&armed, &buy, &q, now).unwrap_err(),
        LIVE_INTENT_SEEN
    );
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-023: one flush covers one device; a written directory on
/// another is refused before the flush.
#[test]
fn a_flush_refuses_a_directory_on_another_device() {
    let dir = std::env::temp_dir().join(format!("bsc-flush-device-{}", std::process::id()));
    create_durable_dir(&dir).unwrap();
    flush_device(&dir, std::slice::from_ref(&dir)).unwrap();
    let other = Path::new("/dev");
    assert_ne!(
        fs::metadata(other).unwrap().dev(),
        fs::metadata(&dir).unwrap().dev()
    );
    let refused = flush_device(&dir, &[other.to_path_buf()]).unwrap_err();
    assert!(refused.to_string().contains("is not on the device of"));
    fs::remove_dir_all(dir).unwrap();
}

/// Budget fix (test N): a damaged entry, one naming its sender and chain
/// but not its nonce, and a spend past 128 bits leave the spend
/// unreadable; an admission then, or with its lock not to be opened,
/// is refused with nothing consumed and nothing written.
#[test]
fn an_unreadable_spend_refuses_the_admission() {
    let (dir, s, now) = live_ready_granting("live-spend-bad", |now| {
        grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified();
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let generation = armed.grant_generation();
    let id = hex32(&[0x99; 32]);
    let admit = || s.admit_live(&armed, &nonce_buy(&id, 7, 1_000), &q, now);
    let lock = dir.join("live-spend.lock");
    fs::create_dir_all(&lock).unwrap();
    assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
    fs::remove_dir(&lock).unwrap();
    create_durable_dir(&spend_dir(&dir)).unwrap();
    let damaged = spend_dir(&dir).join(format!("{}.dbrec", hex32(&[1; 32])));
    fs::write(&damaged, b"not a record").unwrap();
    assert!(
        spent_by_grant(&dir, generation)
            .unwrap_err()
            .starts_with(LIVE_SPEND_UNREADABLE)
    );
    assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
    fs::remove_file(&damaged).unwrap();
    spend_entry(&dir, &admitted_of(2, generation, None, Some(ORIGIN), 1));
    assert!(
        spent_by_grant(&dir, generation)
            .unwrap_err()
            .starts_with(LIVE_SPEND_UNREADABLE)
    );
    assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
    fs::remove_file(spend_dir(&dir).join(format!("{}.dbrec", hex32(&[2; 32])))).unwrap();
    let half = u128::MAX / 2 + 1;
    spend_entry(
        &dir,
        &admitted_of(3, generation, Some(1), Some(ORIGIN), half),
    );
    spend_entry(
        &dir,
        &admitted_of(4, generation, Some(2), Some(ORIGIN), half),
    );
    assert!(
        spent_by_grant(&dir, generation)
            .unwrap_err()
            .contains("overflows")
    );
    assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
    for n in [3u8, 4] {
        fs::remove_file(spend_dir(&dir).join(format!("{}.dbrec", hex32(&[n; 32])))).unwrap();
    }
    // Past 128 bits among entries without sender and chain.
    spend_entry(&dir, &admitted_of(5, generation, Some(1), None, half));
    spend_entry(&dir, &admitted_of(6, generation, Some(1), None, half));
    assert!(
        spent_by_grant(&dir, generation)
            .unwrap_err()
            .contains("overflows")
    );
    assert!(admit().unwrap_err().starts_with(LIVE_SPEND_UNREADABLE));
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 0);
    assert_eq!(total_usage(&dir, max()).unwrap().used, 0);
    assert!(!spend_dir(&dir).join(format!("{id}.dbrec")).exists());
    fs::remove_dir_all(dir).unwrap();
}

/// Budget fix (test L, threads): two admissions, each through its own
/// store, with room for one: while the first holds its read the
/// second admits nothing (never waits); after the first's write it
/// reads its entry and is refused; exactly one is admitted.
#[test]
fn concurrent_admissions_never_exceed_the_grant() {
    let (dir, _s, now) = live_ready_granting("live-spend-race", |now| LiveGrant {
        max_input_wei: 1_000,
        max_spend_wei: 10_000,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let (read, first_read) = std::sync::mpsc::channel();
    let (went_on, go_on) = std::sync::mpsc::channel::<()>();
    let at = dir.clone();
    let first = std::thread::spawn(move || {
        let s = Store::open_in_session(&at, keeping(8), Ok("boot-1".into())).unwrap();
        let q = qualified_in(&s);
        let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
        // Holds its read until the second admission has run.
        AFTER_SPEND_READ.with(|h| {
            *h.borrow_mut() = Some(Box::new(move || {
                let _ = read.send(());
                let _ = go_on.recv();
            }))
        });
        let id = hex32(&[1; 32]);
        s.admit_live(&armed, &nonce_buy(&id, 7, 6_000), &q, now)
            .map(|a| a.spent_wei)
    });
    first_read.recv().unwrap();
    let s = Store::open_in_session(&dir, keeping(8), Ok("boot-1".into())).unwrap();
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let ids = [hex32(&[2; 32]), hex32(&[3; 32])];
    let during = s.admit_live(&armed, &nonce_buy(&ids[0], 8, 6_000), &q, now);
    went_on.send(()).unwrap();
    assert_eq!(first.join().unwrap(), Ok(6_000));
    // While the first held its read, the second admitted nothing.
    let busy = during.unwrap_err();
    assert!(
        busy.starts_with(LIVE_SPEND_UNREADABLE) && busy.ends_with("held by another admission"),
        "{busy}"
    );
    // After it, the second reads the first's entry.
    let after = s.admit_live(&armed, &nonce_buy(&ids[1], 8, 6_000), &q, now);
    assert_eq!(after.unwrap_err(), LIVE_GRANT_SPEND_EXHAUSTED);
    assert_eq!(
        spent_by_grant(&dir, armed.grant_generation()).unwrap(),
        6_000
    );
    assert_eq!(admitted_intents(&dir).unwrap().len(), 1);
    assert_eq!(buy_usage(&dir, now, max()).unwrap().used, 1);
    fs::remove_dir_all(dir).unwrap();
}

const SPEND_CHILD_DIR: &str = "BSC_SPEND_CHILD_DIR";

/// The other process of
/// [`admissions_of_two_processes_never_exceed_the_grant`]: admits one
/// BUY, holding its read for a second.
#[test]
#[ignore = "run by its parent test"]
fn spend_admission_child() {
    let Ok(dir) = std::env::var(SPEND_CHILD_DIR) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let s = Store::open_in_session(&dir, keeping(8), Ok("boot-1".into())).unwrap();
    let q = qualified_in(&s);
    let now = 1_793_059_200_000;
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let (marker, parent) = (dir.join("child-read"), dir.join("parent-done"));
    // Holds its read until the parent's admission has run.
    AFTER_SPEND_READ.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            fs::write(&marker, b"read").unwrap();
            let started = std::time::Instant::now();
            while !parent.exists() && started.elapsed() < std::time::Duration::from_secs(60) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }))
    });
    let id = hex32(&[1; 32]);
    let spent = s
        .admit_live(&armed, &nonce_buy(&id, 7, 6_000), &q, now)
        .map(|a| a.spent_wei);
    let tmp = dir.join("child-result.tmp");
    fs::write(&tmp, format!("{spent:?}")).unwrap();
    fs::rename(&tmp, dir.join("child-result")).unwrap();
}

/// Budget fix (test L, processes): an admission of another process
/// that has read the spend holds the owner's lock: this one admits
/// nothing meanwhile; after the other's write it reads its entry and is
/// refused; exactly one is admitted.
#[test]
fn admissions_of_two_processes_never_exceed_the_grant() {
    let (dir, s, now) = live_ready_granting("live-spend-procs", |now| LiveGrant {
        max_input_wei: 1_000,
        max_spend_wei: 10_000,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    assert_eq!(now, 1_793_059_200_000);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "sources::tests::spend_admission_child",
        ])
        .env(SPEND_CHILD_DIR, &dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    while !dir.join("child-read").exists() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(60),
            "the child never read the spend"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let ids = [hex32(&[2; 32]), hex32(&[3; 32])];
    let during = s.admit_live(&armed, &nonce_buy(&ids[0], 8, 6_000), &q, now);
    fs::write(dir.join("parent-done"), b"done").unwrap();
    assert!(child.wait().unwrap().success());
    let busy = during.unwrap_err();
    assert!(busy.ends_with("held by another admission"), "{busy}");
    let second = s.admit_live(&armed, &nonce_buy(&ids[1], 8, 6_000), &q, now);
    assert_eq!(
        fs::read_to_string(dir.join("child-result")).unwrap(),
        "Ok(6000)"
    );
    assert_eq!(second.unwrap_err(), LIVE_GRANT_SPEND_EXHAUSTED);
    assert_eq!(
        spent_by_grant(&dir, armed.grant_generation()).unwrap(),
        6_000
    );
    assert_eq!(admitted_intents(&dir).unwrap().len(), 1);
    fs::remove_dir_all(dir).unwrap();
}

/// Plan 28.10.: the LIVE tab's checklist names every condition once,
/// in order, with the gate's own reason; without a grant the scope
/// conditions cannot hold.
#[test]
fn the_live_checklist_names_every_condition_with_its_reason() {
    let (dir, _s, now) = live_ready("live-checklist");
    let (g, daily, total) = live_inputs(&dir, now, max()).unwrap();
    let q = qualified();
    let ctx = LiveContext {
        boot: Some("boot-1"),
        now_unix_ms: now,
        daily: Some(&daily),
        total: Some(&total),
        spent_wei: Some(0),
        qualification: &q,
    };
    let all = g.live_checklist(&ctx);
    assert_eq!(all.len(), LIVE_CONDITIONS.len());
    assert!(all.iter().all(|(_, r)| r.is_none()));
    let by = SetBy {
        request_id: "x".into(),
        generation: g.generation,
    };
    let mut off = g.clone();
    off.set(SwitchId::Send.into(), Mode::Disabled, by.clone());
    off.set(Entity::LiveGrant, Mode::Disabled, by);
    let list = off.live_checklist(&LiveContext {
        qualification: &LiveQualification::default(),
        ..ctx
    });
    let reason = |id: &str| list.iter().find(|(c, _)| *c == id).unwrap().1;
    assert_eq!(reason("ECON_SEND"), Some(ECON_SEND_DISABLED));
    assert_eq!(reason("LIVE_GRANT"), Some(LIVE_GRANT_MISSING));
    assert_eq!(reason("VENUE_QUALIFIED"), Some(LIVE_GRANT_MISSING));
    assert_eq!(reason("WALLET_BUYER"), None);
    fs::remove_dir_all(dir).unwrap();
}

/// BSC3-068 (REV.1.0 R3): the slots of an attempt no byte of which
/// left go back whole (its campaign slot and each daily slot it took, a
/// lease's of every UTC day it was held on), once; another attempt then
/// takes them; the returned slots still hold the forward clock; a
/// return a crash cut off is completed; the spend entries stay; a held
/// spend lock refuses it, a name that is no attempt too.
#[test]
fn a_withdrawn_attempts_slots_go_back_and_the_clock_still_holds() {
    const DAY: u64 = 86_400_000;
    let (dir, s, now) = live_ready_limited("slot-return", (2, 2), |now| LiveGrant {
        max_attempts: 2,
        ..grant_of(now + 3 * DAY, LiveStrategies::Both)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let lease = |id| LiveIntent {
        token: [0; 20],
        ..nonce_buy(id, 7, 1_000)
    };
    let (a, c) = (hex32(&[0xa0; 32]), hex32(&[0xc0; 32]));
    let (one, tx) = (hex32(&[0xa1; 32]), hex32(&[0xb1; 32]));
    let ids: Vec<String> = [0xd1, 0xf1, 0xf2, 0x91].map(|b| hex32(&[b; 32])).to_vec();
    s.lease_live(&armed, &lease(&one), &a, &q, now).unwrap();
    s.admit_live(&armed, &nonce_buy(&tx, 8, 2_000), &q, now)
        .unwrap();
    let used = |at| {
        (
            buy_usage(&dir, at, max()).unwrap().used,
            total_usage(&dir, max()).unwrap().used,
        )
    };
    assert_eq!(used(now), (2, 2));
    let three = hex32(&[0xc1; 32]);
    assert!(s.lease_live(&armed, &lease(&three), &c, &q, now).is_err());
    let entries = admitted_intents(&dir).unwrap().len();
    // A's lease never sent its BUY: both its slots go back, once.
    assert_eq!(s.return_attempt(&a), Ok(2));
    assert_eq!(s.return_attempt(&a), Ok(0));
    assert_eq!(used(now), (1, 1));
    let returns = dir.join("buy-attempt-returns");
    let day = BuyDay::at(now).utc;
    assert!(returns.join(format!("{a}-total-{:010}.dbrec", 0)).exists());
    assert!(
        returns
            .join(format!("{a}-daily-{day}-{:010}.dbrec", 0))
            .exists()
    );
    // The freed slot is C's now; the BUY of its own goes back as well.
    s.lease_live(&armed, &lease(&three), &c, &q, now).unwrap();
    assert_eq!(used(now), (2, 2));
    assert_eq!(s.return_attempt(&tx), Ok(2));
    assert_eq!(s.return_attempt(&c), Ok(2));
    assert_eq!(used(now), (0, 0));
    assert_eq!(admitted_intents(&dir).unwrap().len(), entries + 1);
    // No slot is taken any more, yet a clock set back is still refused.
    let d = hex32(&[0xd0; 32]);
    let e = s
        .lease_live(&armed, &lease(&ids[0]), &d, &q, now - 1)
        .unwrap_err();
    assert!(e.contains("BUY_ATTEMPT_CLOCK_BEHIND"), "{e}");
    // A lease held across midnight: its slot of each day goes back.
    let next = now + DAY;
    let armed = s.live_now(&[ComponentId::ExecC3], &q, next).unwrap();
    let f = hex32(&[0xf0; 32]);
    s.lease_live(&armed, &lease(&ids[1]), &f, &q, now).unwrap();
    s.lease_live(&armed, &lease(&ids[2]), &f, &q, next).unwrap();
    assert_eq!((used(now).0, used(next).0, used(next).1), (1, 1, 1));
    assert_eq!(s.return_attempt(&f), Ok(3));
    assert_eq!((used(now).0, used(next)), (0, (0, 0)));
    // A return cut off after its first move is completed.
    let g = hex32(&[0x90; 32]);
    s.lease_live(&armed, &lease(&ids[3]), &g, &q, next).unwrap();
    let next_day = BuyDay::at(next).utc;
    let daily = buy_day_dir(&dir, &BuyDay::at(next));
    let (slot, _) = buy_slots(&daily)
        .unwrap()
        .into_iter()
        .find(|(_, of)| of.as_deref().is_some_and(|o| o.starts_with(&g)))
        .unwrap();
    fs::rename(
        daily.join(format!("{slot:010}.dbrec")),
        returns.join(format!("{g}-daily-{next_day}-{slot:010}.dbrec")),
    )
    .unwrap();
    assert_eq!(used(next), (0, 1));
    assert_eq!(s.return_attempt(&g), Ok(1));
    assert_eq!(used(next), (0, 0));
    // Never while an admission holds the spend lock; never a non-attempt.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dir.join("live-spend.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(
        s.return_attempt(&g)
            .unwrap_err()
            .contains("held by another admission")
    );
    drop(lock);
    assert!(s.return_attempt("not-an-attempt").is_err());
    fs::remove_dir_all(dir).unwrap();
}

/// bugs_v4 L2: a grant revoked while an admission holds its read (another
/// process's request takes no spend lock) refuses that admission, as a
/// limit lowered meanwhile does (BSC3-004); before: admitted on the grant
/// read before the revoke.
#[test]
fn a_grant_revoked_during_an_admission_refuses_it() {
    let (dir, _s, now) = live_ready_granting("live-spend-revoke", |now| LiveGrant {
        max_input_wei: 1_000,
        max_spend_wei: 10_000,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let s = Store::open_in_session(&dir, keeping(8), Ok("boot-1".into())).unwrap();
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let at = dir.clone();
    AFTER_SPEND_READ.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let other = Store::open_in_session(&at, keeping(8), Ok("boot-1".into())).unwrap();
            let g = other.current().unwrap().generation;
            other
                .request("revoke", Entity::LiveGrant, Mode::Disabled, g, now)
                .unwrap();
        }))
    });
    let id = hex32(&[1; 32]);
    let admitted = s.admit_live(&armed, &nonce_buy(&id, 7, 6_000), &q, now);
    AFTER_SPEND_READ.with(|h| *h.borrow_mut() = None);
    assert!(s.current().unwrap().live.is_none(), "the revoke applied");
    assert_eq!(
        admitted.map(|a| a.spent_wei),
        Err(LIVE_GRANT_CHANGED.to_string())
    );
    fs::remove_dir_all(dir).unwrap();
}

/// bugs_v4 L3: a lease returned while an admission holds its read (a
/// return takes no spend lock) only makes that admission still count it:
/// refused, never above the grant; afterwards the room is there.
#[test]
fn a_lease_returned_during_an_admission_only_over_counts() {
    let (dir, s, now) = live_ready_limited("live-lease-race", (5, 5), |now| LiveGrant {
        max_attempts: 5,
        max_input_wei: 1_000,
        max_spend_wei: 10_000,
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = qualified_in(&s);
    let armed = s.live_now(&[ComponentId::ExecC3], &q, now).unwrap();
    let leased = hex32(&[0xa1; 32]);
    let lease = LiveIntent {
        token: [0; 20],
        ..nonce_buy(&leased, 7, 6_000)
    };
    s.lease_live(&armed, &lease, &hex32(&[0xa0; 32]), &q, now)
        .unwrap();
    let at = dir.clone();
    AFTER_SPEND_READ.with(|h| {
        *h.borrow_mut() = Some(Box::new(move || {
            let other = Store::open_in_session(&at, keeping(8), Ok("boot-1".into())).unwrap();
            other.return_leases(&[[0xa1; 32]]).unwrap();
        }))
    });
    let (during, after) = (hex32(&[0xb1; 32]), hex32(&[0xb2; 32]));
    let refused = s.admit_live(&armed, &nonce_buy(&during, 8, 6_000), &q, now);
    AFTER_SPEND_READ.with(|h| *h.borrow_mut() = None);
    assert_eq!(
        refused.map(|a| a.spent_wei),
        Err(LIVE_GRANT_SPEND_EXHAUSTED.to_string())
    );
    assert_eq!(spent_by_grant(&dir, armed.grant_generation).unwrap(), 0);
    assert_eq!(
        s.admit_live(&armed, &nonce_buy(&after, 8, 6_000), &q, now)
            .map(|a| a.spent_wei),
        Ok(6_000)
    );
    fs::remove_dir_all(dir).unwrap();
}
