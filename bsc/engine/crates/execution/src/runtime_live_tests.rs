use super::*;
use std::os::unix::fs::PermissionsExt;

/// BSC3-026: the stand-in route runs only on a state that is not the
/// account's owner state (also through another spelling of its path),
/// and not when that state is unknown.
#[test]
fn loopback_never_runs_on_the_accounts_owner_state() {
    let base = std::env::temp_dir().join(format!("bsc-loopback-state-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let account = base.join("home/.darkbot/run/bsc-sources");
    let test = base.join("test/bsc-sources");
    std::fs::create_dir_all(&account).unwrap();
    std::fs::create_dir_all(&test).unwrap();
    assert_eq!(loopback_refused(&test, Ok(account.clone())), None);
    let same = loopback_refused(&account, Ok(account.clone())).unwrap();
    assert!(same.starts_with("LOOPBACK_ON_ACCOUNT_STATE"), "{same}");
    let spelled = base.join("test/../home/.darkbot/run/bsc-sources");
    assert!(loopback_refused(&spelled, Ok(account.clone())).is_some());
    let unknown = loopback_refused(&test, Err("HOME differs".into())).unwrap();
    assert!(
        unknown.starts_with("LOOPBACK_STATE_UNVERIFIED"),
        "{unknown}"
    );
    std::fs::remove_dir_all(&base).unwrap();
}
use crate::chain_follower::{CanonicalEvent, CanonicalTx};
use crate::live_end::tests::{club48_findings, key_file, stand_in};

/// R10: the BlockRazor authorization is read under the key's rules (a
/// private file outside the repository, through its own name only).
#[test]
fn the_blockrazor_authorization_is_read_as_a_private_file() {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let dir = std::env::temp_dir().join(format!("bsc-live-token-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let token = dir.join("token");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&token)
        .and_then(|mut f| f.write_all(b"secret-token\n"))
        .unwrap();
    let repo = Path::new("/nonexistent-repo");
    assert_eq!(token_from(&token, repo).unwrap().expose(), "secret-token");
    let e = token_from(&token, &dir).unwrap_err();
    assert!(
        e.contains("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE location refused"),
        "{e}"
    );
    std::fs::hard_link(&token, dir.join("second")).unwrap();
    let e = token_from(&token, repo).unwrap_err();
    assert!(
        e.contains("BSC_LIVE_BLOCKRAZOR_TOKEN_FILE permissions refused"),
        "{e}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// BSC3-026: the stand-in's test identity in the test's owner state
/// `state` (the synthetic buyer's key), and its address.
fn loopback_key(state: &Path) -> (PathBuf, [u8; 20]) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = crate::live_key::loopback_identity(state);
    let secret = crate::live_qualification::tests::BUYER_SECRET;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(&secret))
        .unwrap();
    let buyer = darkbot_bsc_trading::signer::Signer::from_secret_bytes(&secret)
        .unwrap()
        .address();
    (path, buyer.into_array())
}
use crate::live_qualification::tests::{acquisition, fixture, venue_evidence};
use crate::live_qualification::{LIVE_QUALIFICATION_PATH, RiskSpec, Spec, build};
use crate::paper_exec::FnClock;
use crate::runtime::exec::tests::{HEAD, launch, limits, observation, owner, setup};
use crate::sources::tests::{grant_of, live_ready_granting};
use crate::sources::{
    Entity, LIVE_GRANT_MISSING, LiveAdapter, LiveGrant, Mode, VenueId, buy_usage,
};
use darkbot_bsc_record::Record;
use darkbot_bsc_trading::venues::flap::deployment::FlapDeployment;
use std::cell::Cell;
use std::fs;

const NOW: u64 = 1_793_059_200_000;

fn fixed() -> u64 {
    NOW
}

fn funded(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    let mut o = observation(d.portal_version, d.portal_implementation);
    o.caller_balance_wei = U256::from(10_000_000_000_000_000_000u128);
    Ok(o)
}

fn empty(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    Ok(observation(d.portal_version, d.portal_implementation))
}

/// One BUY at the setup's gas (1.15e15 wei) fits, its exit (8e13) not.
fn short(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    let mut o = observation(d.portal_version, d.portal_implementation);
    o.caller_balance_wei = U256::from(1_200_000_000_000_000u128);
    Ok(o)
}

/// One C3-sized BUY with its exit fits, a Strategy C BUY of 1e16 with
/// its exit (1.023e16) not.
fn mid(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    let mut o = observation(d.portal_version, d.portal_implementation);
    o.caller_balance_wei = U256::from(10_200_000_000_000_000u128);
    Ok(o)
}

/// One Strategy C BUY of 1e16 with its exit fits, a second one after it
/// with both exits not.
fn two(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    let mut o = observation(d.portal_version, d.portal_implementation);
    o.caller_balance_wei = U256::from(20_400_000_000_000_000u128);
    Ok(o)
}

/// One nonce burn at the setup's gas (5e7 wei x 21 000).
const ONE_BURN_WEI: u64 = 1_050_000_000_000;

/// One nonce burn fits, an exit step (5e12) not.
fn one_burn(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    let mut o = observation(d.portal_version, d.portal_implementation);
    o.caller_balance_wei = U256::from(ONE_BURN_WEI);
    Ok(o)
}

/// The exit of one open position (8e13 wei) fits, a BUY not.
fn exits_only(
    _: &str,
    _: &FollowerLimits,
    d: &FlapDeployment,
    _: Address,
) -> Result<VenueObservation, String> {
    let mut o = observation(d.portal_version, d.portal_implementation);
    o.caller_balance_wei = U256::from(100_000_000_000_000u128);
    Ok(o)
}

fn record(x: &LiveRuntime) -> Record {
    darkbot_bsc_record::decode(&l::SCHEMA, &x.draft().encode().unwrap()).unwrap()
}

/// Ticks until the state is not `PROBING` (the probe thread answers).
fn settle(x: &mut LiveRuntime, owner: &StateOwner) {
    let lim = limits();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        x.tick(Some(owner), Some(("https://probe.invalid", &lim)), 7, NOW);
        if record(x).text(l::STATE).unwrap() != "PROBING" || Instant::now() > deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// BSC3-025 review: before every decision the armed end installs the
/// connections established meanwhile (a replaced one is usable once its
/// handshake completed, not only after the next maintenance pass).
#[test]
fn every_decision_installs_the_connections_established_meanwhile() {
    let source = include_str!("runtime_live.rs");
    let body = &source[source
        .find("    pub fn poll_responses(&mut self, clock: &dyn Clock) {")
        .unwrap()..];
    let body = &body[..body.find("\n    }\n").unwrap()];
    let install = body.find("x.end_mut().install_ready();").expect("installs");
    assert!(install < body.find("x.poll_responses(clock);").unwrap());
    for decision in ["pub fn on_launch(", "pub fn on_strategy("] {
        let at = source.find(decision).unwrap();
        let next = source[at..].find("self.poll_responses(clock);").unwrap();
        let end = source[at..].find("\n    }\n").unwrap();
        assert!(next < end, "{decision} polls before deciding");
    }
}

/// Audit E 5.6: the waiting launches keep their bound and their order
/// (the oldest held one goes first), take and insert are O(1), a taken
/// one leaves no gap in the bound, and the order never holds more than
/// twice the bound.
#[test]
fn the_awaiting_launches_keep_their_bound_and_order() {
    let h = |n: u8| [n; 32];
    let mut a = Awaiting::default();
    for n in 1..=3 {
        a.insert(h(n), 3);
    }
    a.insert(h(2), 3);
    assert_eq!(a.len(), 3);
    // At the bound the oldest goes: 1.
    a.insert(h(4), 3);
    assert!(!a.take(&h(1)) && a.len() == 3);
    // A taken one frees its place: 2 goes, 5 comes, nothing else goes.
    assert!(a.take(&h(2)));
    a.insert(h(5), 3);
    assert!(a.take(&h(3)) && a.take(&h(4)) && a.take(&h(5)));
    assert_eq!(a.len(), 0);
    // Many taken entries: the order stays within twice the bound.
    for n in 0..=255u8 {
        a.insert(h(n), 4);
        a.take(&h(n));
    }
    assert!(a.order.len() <= 8, "{}", a.order.len());
    a.insert(h(9), 4);
    a.clear();
    assert!(!a.take(&h(9)));
    // Review LOW-1: a hash taken and held again is as young as its new
    // arrival; its old entry lets nothing go (bound 2: A taken, B, A
    // again, then C lets B go, the oldest held).
    a.insert(h(1), 2);
    assert!(a.take(&h(1)));
    a.insert(h(2), 2);
    a.insert(h(1), 2);
    a.insert(h(3), 2);
    assert!(!a.take(&h(2)) && a.take(&h(1)) && a.take(&h(3)));
    // Review LOW-2: a hash held again keeps one entry when the order
    // is compacted, so the order stays within twice the bound.
    a.clear();
    for _ in 0..3 {
        a.insert(h(7), 2);
        a.take(&h(7));
    }
    a.insert(h(7), 2);
    a.insert(h(8), 2);
    assert!(a.order.len() <= 2, "{}", a.order.len());
    assert!(a.take(&h(7)) && a.take(&h(8)) && a.len() == 0);
}

/// BSC3-012: the LIVE path arms only in the build its qualification
/// names: another build's qualification (or one naming none) keeps it
/// BLOCKED with LIVE_BUILD_UNQUALIFIED before any key is read (with no
/// key file it says so, not KEY_REFUSED); qualified for this build, it
/// arms.
#[test]
fn the_live_path_arms_only_in_the_qualified_build() {
    let r = Rig::new("live-build", LiveStrategies::C3);
    let spec = |runtime| rig_spec(runtime, LiveStrategies::C3);
    let write = |runtime: [u8; 32]| {
        fs::write(
            r.root.join(LIVE_QUALIFICATION_PATH),
            build(&r.root, &spec(Some(runtime)), 1).unwrap(),
        )
        .unwrap();
    };
    assert!(
        build(&r.root, &spec(None), 1).is_err(),
        "a qualification without its build"
    );
    write([0x77; 32]);
    let mut x = LiveRuntime::with_prober(
        LiveSetup {
            key_path: r.d.join("no-key-here"),
            ..r.live.clone()
        },
        r.exec.clone(),
        &r.root,
        r.state.clone(),
        r.d.join("fences"),
        None,
        funded,
    )
    .in_session("boot-1", fixed);
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    let (state_now, why) = reason(&x);
    assert_eq!(state_now, "BLOCKED");
    let why = why.unwrap_or_default();
    assert!(
        why.starts_with(crate::sources::LIVE_BUILD_UNQUALIFIED),
        "{why}"
    );
    assert!(
        why.contains(&alloy_primitives::hex::encode([0x77; 32])),
        "{why}"
    );
    assert!(!x.armed());
    // This build qualified: it arms.
    write(crate::live_qualification::running_build().unwrap());
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    x.shutdown();
}

/// BSC3-025: arming over the builders never waits for their TLS
/// connection on the decision loop (it did, up to Q5A's 5 s connect
/// and 5 s handshake): the pass begins it and returns CONNECTING; the
/// passes after it wait for that one end, never build another; a pass
/// that does not come back to arming drops it (its key with it); a
/// connection that fails makes a later pass CONNECT_FAILED.
#[test]
fn arming_over_the_builders_never_waits_for_their_connection() {
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    // A liveness bound, not a timing claim.
    fn wait_until(mut done: impl FnMut() -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(30) {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        done()
    }
    let (state, _store, now) = live_ready_granting("live-rt-025", |now| {
        rig_grant(now + 3_600_000, LiveStrategies::C3)
    });
    assert_eq!(now, NOW);
    let root = fixture("live-rt-025", "PASS");
    let spec = rig_spec(
        Some(crate::live_qualification::running_build().unwrap()),
        LiveStrategies::C3,
    );
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::write(
        root.join(LIVE_QUALIFICATION_PATH),
        build(&root, &spec, 1).unwrap(),
    )
    .unwrap();
    let d = std::env::temp_dir().join(format!("bsc-live-rt-025-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let (key, buyer) = key_file(&d);
    // A builder that takes the TCP connection and never answers the
    // handshake until told to close.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let close = Arc::new(AtomicBool::new(false));
    let (a, c) = (Arc::clone(&accepted), Arc::clone(&close));
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let mut held = Vec::new();
        while !c.load(Ordering::SeqCst) {
            if let Ok((stream, _)) = listener.accept() {
                a.fetch_add(1, Ordering::SeqCst);
                held.push(stream);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    });
    crate::live_end::BUILDERS_AT.with(|at| at.set(Some(address)));
    let live_setup = LiveSetup {
        key_path: key,
        buyer,
        route: LiveRouteConfig::Builders {
            blockrazor_token: None,
        },
        connect_timeout: Duration::from_secs(2),
    };
    let exec = setup(d.join("unused.key"), Duration::from_secs(3_600));
    let mut x = LiveRuntime::with_prober(
        live_setup,
        exec,
        &root,
        state.clone(),
        d.join("fences"),
        None,
        funded,
    )
    .in_session("boot-1", fixed);
    let mut c2 = owner(&d, buyer);
    settle(&mut x, &c2);
    assert_eq!(record(&x).text(l::STATE).unwrap(), "WAITING_NONCE");
    c2.canonical(&CanonicalEvent::Block {
        number: HEAD,
        hash: [0xb1; 32],
        parent_hash: [0xb0; 32],
        timestamp_s: 1_793_000_000,
        timestamp_ms: 1_793_000_000_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: Vec::new(),
    });
    c2.canonical(&CanonicalEvent::NonceFact {
        sender: buyer,
        next_nonce: 5,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = c2.take_evidence();
    let lim = limits();
    // One pass, or the passes until the deployment probe settled; each
    // returns at once. The longest one.
    let pass = |x: &mut LiveRuntime, owner: Option<&StateOwner>| {
        let mut longest = Duration::ZERO;
        for _ in 0..2_000 {
            let t = Instant::now();
            x.tick(owner, Some(("https://probe.invalid", &lim)), 7, NOW);
            longest = longest.max(t.elapsed());
            if record(x).text(l::STATE).unwrap() != "PROBING" {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        longest
    };
    // A liveness margin, not a timing claim: the handshake would hold
    // the pass 5 s.
    let took = pass(&mut x, Some(&c2));
    assert!(took < Duration::from_secs(2), "{took:?}");
    let state_now = |x: &LiveRuntime| record(x).text(l::STATE).unwrap().to_string();
    assert_eq!(state_now(&x), "CONNECTING");
    assert!(!x.armed());
    assert!(wait_until(|| accepted.load(Ordering::SeqCst) == 1));
    assert!(pass(&mut x, Some(&c2)) < Duration::from_secs(2));
    assert_eq!(state_now(&x), "CONNECTING");
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "a second end was built");
    // A pass that does not come back to arming (C2 not watching) keeps
    // the end (review LOW-3): the next arming pass waits for the same
    // connection, it does not begin another.
    pass(&mut x, None);
    assert_ne!(state_now(&x), "CONNECTING");
    assert!(x.connecting.is_some());
    assert!(pass(&mut x, Some(&c2)) < Duration::from_secs(2));
    assert_eq!(state_now(&x), "CONNECTING");
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    // Review MEDIUM-2: another grant (a new generation) or another
    // qualification record is another end; the old one never arms.
    let owner_store = Store::open_in_session(
        &state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let regrant = |name: &str| {
        let grant = LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            adapters: vec![LiveAdapter::Club48BuilderV1],
            ..grant_of(NOW + 3_600_000, LiveStrategies::C3)
        };
        let g = owner_store.current().unwrap().generation;
        let r = owner_store
            .grant_live(name, grant.clone(), grant.scope_sha256(), g, NOW)
            .unwrap();
        assert!(r.applied(), "{}", r.line());
    };
    regrant("regrant-1");
    assert!(pass(&mut x, Some(&c2)) < Duration::from_secs(2));
    assert_eq!(state_now(&x), "CONNECTING");
    assert!(wait_until(|| accepted.load(Ordering::SeqCst) == 2));
    fs::write(
        root.join(LIVE_QUALIFICATION_PATH),
        build(&root, &spec, 2).unwrap(),
    )
    .unwrap();
    assert!(pass(&mut x, Some(&c2)) < Duration::from_secs(2));
    assert_eq!(state_now(&x), "CONNECTING");
    assert!(wait_until(|| accepted.load(Ordering::SeqCst) == 3));
    assert!(!x.armed());
    // A closed gate (revoked) drops it at once, its key with it.
    let g = owner_store.current().unwrap().generation;
    assert!(
        owner_store
            .request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
            .unwrap()
            .applied()
    );
    pass(&mut x, Some(&c2));
    assert!(x.connecting.is_none());
    assert_eq!(state_now(&x), "BLOCKED");
    // Granted again: a new end; the stop drops it.
    regrant("regrant-2");
    assert!(pass(&mut x, Some(&c2)) < Duration::from_secs(2));
    assert_eq!(state_now(&x), "CONNECTING");
    assert!(wait_until(|| accepted.load(Ordering::SeqCst) == 4));
    // The builder closes: the handshake fails, a later pass says so.
    close.store(true, Ordering::SeqCst);
    server.join().unwrap();
    let failed = wait_until(|| {
        pass(&mut x, Some(&c2));
        state_now(&x) == "CONNECT_FAILED"
    });
    assert!(failed, "{}", state_now(&x));
    assert!(x.connecting.is_none());
    assert!(!x.armed());
    // A stop while connecting drops the end.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    crate::live_end::BUILDERS_AT.with(|at| at.set(Some(listener.local_addr().unwrap())));
    regrant("regrant-3");
    x.last_attempt = None;
    pass(&mut x, Some(&c2));
    assert_eq!(state_now(&x), "CONNECTING");
    x.shutdown();
    assert!(x.connecting.is_none());
    drop(listener);
    crate::live_end::BUILDERS_AT.with(|at| at.set(None));
    fs::remove_dir_all(&d).unwrap();
}

/// Plan 28.10.: the LIVE path arms only under the owner's clear gate
/// with its qualification on file, a funded buyer and C2's clean nonce;
/// it takes only launches first seen by a granted source; C2's
/// inclusion of its BUY resolves the lane; a revoked grant ends it and
/// its key goes.
#[test]
fn the_live_path_arms_under_the_gate_and_ends_with_it() {
    let (state, store, now) = live_ready_granting("live-rt", |now| {
        rig_grant(now + 3_600_000, LiveStrategies::C3)
    });
    assert_eq!(now, NOW);
    let root = fixture("live-rt", "PASS");
    let spec = rig_spec(
        Some(crate::live_qualification::running_build().unwrap()),
        LiveStrategies::C3,
    );
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::write(
        root.join(LIVE_QUALIFICATION_PATH),
        build(&root, &spec, 1).unwrap(),
    )
    .unwrap();
    let d = std::env::temp_dir().join(format!("bsc-live-rt-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let (key, buyer) = loopback_key(&state);
    let (port, requests) = stand_in();
    let live_setup = LiveSetup {
        key_path: key,
        buyer,
        route: LiveRouteConfig::Loopback { port },
        connect_timeout: Duration::from_secs(2),
    };
    let exec = setup(d.join("unused.key"), Duration::from_secs(3_600));
    let new = |prober: Prober| {
        LiveRuntime::with_prober(
            live_setup.clone(),
            exec.clone(),
            &root,
            state.clone(),
            d.join("fences"),
            None,
            prober,
        )
        .in_session("boot-1", fixed)
    };
    // Not watched by C2, then unfunded: nothing arms.
    let mut x = new(funded);
    let foreign = owner(&d.join("c2-other"), [7; 20]);
    settle(&mut x, &foreign);
    assert_eq!(record(&x).text(l::STATE).unwrap(), "NOT_WATCHED");
    let mut c2 = owner(&d, buyer);
    let mut poor = new(empty);
    settle(&mut poor, &c2);
    assert_eq!(record(&poor).text(l::STATE).unwrap(), "UNFUNDED");
    assert!(!poor.armed());
    // Funded: it waits for C2's clean nonce of the buyer, then arms.
    settle(&mut x, &c2);
    assert_eq!(record(&x).text(l::STATE).unwrap(), "WAITING_NONCE");
    c2.canonical(&CanonicalEvent::Block {
        number: HEAD,
        hash: [0xb1; 32],
        parent_hash: [0xb0; 32],
        timestamp_s: 1_793_000_000,
        timestamp_ms: 1_793_000_000_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: Vec::new(),
    });
    c2.canonical(&CanonicalEvent::NonceFact {
        sender: buyer,
        next_nonce: 5,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = c2.take_evidence();
    settle(&mut x, &c2);
    let r = record(&x);
    assert_eq!(
        r.text(l::STATE).unwrap(),
        "ARMED",
        "{:?}",
        r.opt_text(l::REASON)
    );
    assert_eq!(r.opt_u64(l::NEXT_NONCE).unwrap(), Some(5));
    assert!(x.armed());
    // Only a granted source's verified full sighting triggers.
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    assert!(
        x.on_launch(SourceId::Drpc, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_none()
    );
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .unwrap();
    let request = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        String::from_utf8(request)
            .unwrap()
            .contains("eth_sendBundle")
    );
    assert_eq!(
        buy_usage(
            &state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    let r = record(&x);
    assert_eq!(r.u64(l::RELEASED).unwrap(), 1);
    assert!(
        r.texts(l::REFUSALS)
            .unwrap()
            .contains(&"SOURCE_NOT_GRANTED=1")
    );
    // BSC3-043: the released C3 BUY is the LIVE path's own latency
    // sample (ingress to handoff), apart from PAPER's.
    assert_eq!(r.u64(l::LATENCY_SAMPLES).unwrap(), 1);
    assert_eq!(
        r.opt_u64(l::LATENCY_P50_NS).unwrap(),
        r.opt_u64(l::LATENCY_MAX_NS).unwrap()
    );
    assert!(r.opt_u64(l::LATENCY_P50_NS).unwrap().is_some());
    // C2 includes the BUY at the buyer's nonce: the lane resolves it.
    c2.canonical(&CanonicalEvent::Block {
        number: HEAD + 1,
        hash: [0xb2; 32],
        parent_hash: [0xb1; 32],
        timestamp_s: 1_793_000_001,
        timestamp_ms: 1_793_000_001_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: vec![CanonicalTx {
            hash: buy.handoff.tx_hash,
            index: 0,
            sender: buyer,
            nonce: 5,
            succeeded: true,
            cost: None,
        }],
    });
    let evidence = c2.take_evidence();
    assert_eq!(evidence.len(), 1);
    assert_eq!(
        x.apply_evidence(&evidence),
        vec![Some(IntentState::Included)]
    );
    assert_eq!(record(&x).u64(l::INCLUDED).unwrap(), 1);
    // BSC3-042: a second qualified launch after the C3 nonce was
    // included: the arm's C3 lanes all stand on that nonce. Under the
    // default cap it takes nothing: no owner slot, nothing sent.
    let (hash_b, raw_b, launcher_b) = crate::runtime::exec::tests::launch_n(1);
    c2.pending(hash_b, launcher_b, 4, 0, 20);
    let used = buy_usage(
        &state,
        NOW,
        crate::sources::tests::shipped_limits().buy_attempts_total_max,
    )
    .unwrap()
    .used;
    let second = x.on_launch(
        SourceId::PublicNode,
        hash_b,
        [0; 20],
        &raw_b,
        20,
        &c2,
        &clock,
    );
    assert!(second.is_none(), "{:?}", reason(&x));
    assert_eq!(
        buy_usage(
            &state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        used
    );
    assert!(requests.recv_timeout(Duration::from_millis(300)).is_err());
    assert!(
        record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"SESSION_CAP=1")
    );
    // Revoked: the path ends at the next pass, the key goes.
    let owner_store = Store::open_in_session(
        &state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = owner_store.current().unwrap().generation;
    assert!(
        owner_store
            .request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
            .unwrap()
            .applied()
    );
    settle(&mut x, &c2);
    let r = record(&x);
    assert_eq!(
        (r.text(l::STATE).unwrap(), r.opt_text(l::REASON).unwrap()),
        ("ENDED", Some(LIVE_GRANT_MISSING))
    );
    assert!(!x.armed());
    drop(store);
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-042 under a synthetic session cap ten times the default: after
/// C3 BUY A's inclusion at the arm's nonce, a second qualified C3
/// launch B passes the cap and is refused at the buyer's nonce (the
/// arm's C3 lanes all stand on A's): no owner admission (no slot, no
/// spend entry), no bytes on the wire.
#[test]
fn a_second_c3_launch_after_inclusion_takes_nothing_under_a_raised_cap() {
    second_c3_launch("live-rt-cap", false, true);
}

/// R4 (BSC3-042): with BSC_EXEC_LIVE_C3_REARM ON, A's inclusion renews
/// the arm at the next pass: B goes at the buyer's next nonce, admitted
/// by the owner (its own slot and spend entry).
#[test]
fn a_c3_rearm_takes_the_second_c3_launch_at_the_next_nonce() {
    second_c3_launch("live-rt-rearm", true, true);
}

/// Review of R4: a reverted C3 BUY renews no arm (ON): the second C3
/// launch is refused at the buyer's nonce as with OFF.
#[test]
fn a_reverted_c3_buy_renews_no_arm() {
    second_c3_launch("live-rt-reverted", true, false);
}

fn second_c3_launch(tag: &str, rearm: bool, lands: bool) {
    let (state, store, now) =
        live_ready_granting(tag, |now| rig_grant(now + 3_600_000, LiveStrategies::C3));
    assert_eq!(now, NOW);
    let root = fixture(tag, "PASS");
    let spec = rig_spec(
        Some(crate::live_qualification::running_build().unwrap()),
        LiveStrategies::C3,
    );
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::write(
        root.join(LIVE_QUALIFICATION_PATH),
        build(&root, &spec, 1).unwrap(),
    )
    .unwrap();
    let d = std::env::temp_dir().join(format!("bsc-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let (key, buyer) = loopback_key(&state);
    let (port, requests) = stand_in();
    let live_setup = LiveSetup {
        key_path: key,
        buyer,
        route: LiveRouteConfig::Loopback { port },
        connect_timeout: Duration::from_secs(2),
    };
    let mut exec = setup(d.join("unused.key"), Duration::from_secs(3_600));
    exec.exec.session_cap_wei *= 10;
    exec.exec.live_c3_rearm = rearm;
    let mut x = LiveRuntime::with_prober(
        live_setup,
        exec,
        &root,
        state.clone(),
        d.join("fences"),
        None,
        funded,
    )
    .in_session("boot-1", fixed);
    let mut c2 = owner(&d, buyer);
    c2.canonical(&CanonicalEvent::Block {
        number: HEAD,
        hash: [0xb1; 32],
        parent_hash: [0xb0; 32],
        timestamp_s: 1_793_000_000,
        timestamp_ms: 1_793_000_000_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: Vec::new(),
    });
    c2.canonical(&CanonicalEvent::NonceFact {
        sender: buyer,
        next_nonce: 5,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = c2.take_evidence();
    // As the runtime tells once its strategy holds the lots (R1).
    x.set_exit_deciders(Vec::new(), Vec::new(), 0, None);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .unwrap();
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok());
    // A is included at the arm's nonce: C2 and the LIVE path see it
    // (its lanes, its ledger); the buyer's next nonce is C2's n+1, past
    // the arm's C3 lanes.
    let included = CanonicalEvent::Block {
        number: HEAD + 1,
        hash: [0xb2; 32],
        parent_hash: [0xb1; 32],
        timestamp_s: 1_793_000_001,
        timestamp_ms: 1_793_000_001_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: vec![
            CanonicalTx {
                hash: [0x1a; 32],
                index: 0,
                sender: launcher,
                nonce: 3,
                succeeded: true,
                cost: None,
            },
            CanonicalTx {
                hash: buy.handoff.tx_hash,
                index: 1,
                sender: buyer,
                nonce: 5,
                succeeded: lands,
                cost: None,
            },
        ],
    };
    c2.canonical(&included);
    let evidence = c2.take_evidence();
    assert_eq!(
        x.apply_evidence(&evidence),
        vec![Some(if lands {
            IntentState::Included
        } else {
            IntentState::Reverted
        })]
    );
    // Right behind its launch in the block (R2: placed FIRST).
    let _ = x.on_canonical(&included);
    let _ = x.on_canonical(&launch_event(HEAD + 1, 0xb2, 0, buy.token, launcher));
    let _ = lands
        && x.on_canonical(&buy_trade(
            HEAD + 1,
            0xb2,
            1,
            buy.handoff.tx_hash,
            buy.token,
            buyer,
        ))
        .is_empty();
    let _ = x.on_canonical(&CanonicalEvent::BlockComplete {
        number: HEAD + 1,
        hash: [0xb2; 32],
    });
    assert_eq!(x.free_nonce(&c2), Some(6));
    let (used, spent) = (
        buy_usage(
            &state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max,
        )
        .unwrap()
        .used,
        crate::sources::admitted_intents(&state).unwrap(),
    );
    assert_eq!((used, spent.len()), (1, 1));
    let (hash_b, raw_b, launcher_b) = crate::runtime::exec::tests::launch_n(1);
    c2.pending(hash_b, launcher_b, 4, 0, 20);
    // The next pass: renewed (ON), or no lease for a C3 BUY this arm
    // cannot take (OFF).
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let second = x.on_launch(
        SourceId::PublicNode,
        hash_b,
        [0; 20],
        &raw_b,
        20,
        &c2,
        &clock,
    );
    if rearm && lands {
        let second = second.expect("B goes after the renewed arm");
        assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok());
        let facts = crate::live_ledger::Ledger::open(&state).facts().unwrap();
        assert!(
            facts
                .iter()
                .any(|f| f.tx_hash == second.handoff.tx_hash && f.nonce == 6),
            "{facts:?}"
        );
        assert_eq!(
            buy_usage(
                &state,
                NOW,
                crate::sources::tests::shipped_limits().buy_attempts_total_max
            )
            .unwrap()
            .used,
            used + 1
        );
        assert_eq!(
            crate::sources::admitted_intents(&state).unwrap().len(),
            spent.len() + 1
        );
        drop(store);
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state).unwrap();
        return;
    }
    assert!(second.is_none(), "{:?}", reason(&x));
    assert_eq!(
        buy_usage(
            &state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        used
    );
    assert_eq!(crate::sources::admitted_intents(&state).unwrap(), spent);
    assert!(requests.recv_timeout(Duration::from_millis(300)).is_err());
    let r = record(&x);
    let refusals = r.texts(l::REFUSALS).unwrap();
    assert!(refusals.contains(&"SENDER_NONCE=1"), "{refusals:?}");
    assert!(!refusals.iter().any(|r| r.starts_with("SESSION_CAP")));
    assert_eq!(record(&x).u64(l::RELEASED).unwrap(), 1);
    drop(store);
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// Plan 28.10. package 6: a Strategy C BUY released over loopback, its
/// receipt (gas, the Portal's trade) opens the position in the ledger;
/// a reorg takes it back and a new inclusion opens it again; the exit's
/// approval and SELL, signed by the buyer key and admitted by the
/// owner, close it; PnL = proceeds - input - every gas. A restart reads
/// the same book.
#[test]
fn a_real_position_opens_closes_and_is_accounted() {
    use crate::chain_follower::{CanonicalTrade, TxCost};
    use crate::live_ledger::PositionState;
    use crate::paper_exec::{IntentKind, StrategyCurve};
    use crate::runtime_strategy::StrategyAction;
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
        quote_native_sell,
    };
    let (state, _store, _) = live_ready_granting("live-pos", |now| {
        rig_grant(now + 3_600_000, LiveStrategies::StrategyC)
    });
    let root = fixture("live-pos", "PASS");
    let spec = rig_spec(
        Some(crate::live_qualification::running_build().unwrap()),
        LiveStrategies::StrategyC,
    );
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::write(
        root.join(LIVE_QUALIFICATION_PATH),
        build(&root, &spec, 1).unwrap(),
    )
    .unwrap();
    let d = std::env::temp_dir().join(format!("bsc-live-pos-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let (key, buyer) = loopback_key(&state);
    let (port, requests) = stand_in();
    let mut x = LiveRuntime::with_prober(
        LiveSetup {
            key_path: key,
            buyer,
            route: LiveRouteConfig::Loopback { port },
            connect_timeout: Duration::from_secs(2),
        },
        setup(d.join("unused.key"), Duration::from_secs(3_600)),
        &root,
        state.clone(),
        d.join("fences"),
        None,
        funded,
    )
    .in_session("boot-1", fixed);
    let mut c2 = owner(&d, buyer);
    let block = |n: u64, h: u8, parent: u8, txs| CanonicalEvent::Block {
        number: n,
        hash: [h; 32],
        parent_hash: [parent; 32],
        timestamp_s: 1_793_000_000 + (n - HEAD),
        timestamp_ms: 1_793_000_000_000 + (n - HEAD) * 1_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs,
    };
    c2.canonical(&block(HEAD, 0xb1, 0xb0, Vec::new()));
    c2.canonical(&CanonicalEvent::NonceFact {
        sender: buyer,
        next_nonce: 0,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = c2.take_evidence();
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", record(&x).opt_text(l::REASON));
    let reconnect = |x: &mut LiveRuntime| {
        let failed = x
            .executor
            .as_mut()
            .unwrap()
            .end_mut()
            .connect(Duration::from_secs(2));
        assert!(failed.is_empty());
    };
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let token = [0xAB; 20];
    let input = U256::from(10_000_000_000_000_000u64);
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let fresh = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(1_000_000_000_000_000_000u128),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let curve = StrategyCurve {
        terms,
        reserve: fresh.reserve_after,
        circulating: fresh.supply_after,
    };
    let buy = StrategyAction::Buy {
        token,
        creator: [0; 20],
        curve,
        input_wei: input,
        decided_ns: 0,
    };
    let (intents, refused) = x.on_strategy(&buy, &c2, &clock, now_ms).unwrap();
    assert_eq!((intents.len(), refused), (1, None));
    let buy_tx = intents[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(x.book().unresolved().count(), 1);
    // Its receipt: included with its gas and the Portal's trade.
    let bought = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: curve.reserve,
        circulating_before: curve.circulating,
        input_amount: input,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let tokens = bought.output_amount;
    let tx = |hash, nonce, gas: u64| CanonicalTx {
        hash,
        index: 0,
        sender: buyer,
        nonce,
        succeeded: true,
        cost: Some(TxCost {
            gas_used: gas,
            effective_gas_price: 50_000_000,
        }),
    };
    let trade = |hash, n: u64, h: u8, sell, amount: U256, eth: U256| {
        CanonicalEvent::FlapTrade(CanonicalTrade {
            block_number: n,
            block_hash: [h; 32],
            tx_index: 0,
            tx_hash: hash,
            log_index: 0,
            origin: buyer,
            sell,
            token,
            trader: buyer,
            amount: amount.to_be_bytes(),
            eth: eth.to_be_bytes(),
            fee: [0; 32],
            post_price: [0; 32],
        })
    };
    // As the runtime applies a canonical event: C2 first, its evidence
    // to the LIVE path, then the follower's facts.
    let apply = |x: &mut LiveRuntime, c2: &mut StateOwner, events: Vec<CanonicalEvent>| {
        let mut changes = Vec::new();
        for e in &events {
            c2.canonical(e);
            let evidence = c2.take_evidence();
            let _ = x.apply_evidence(&evidence);
            changes.extend(x.on_canonical(e));
        }
        changes
    };
    let complete = |n: u64, h: u8| CanonicalEvent::BlockComplete {
        number: n,
        hash: [h; 32],
    };
    let opened = PositionChange::Opened {
        token,
        tokens,
        spent_wei: input + U256::from(200_000u64 * 50_000_000),
        filled_ms: 1_793_000_001_000,
    };
    let changes = apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 1, 0xb2, 0xb1, vec![tx(buy_tx, 0, 200_000)]),
            trade(buy_tx, HEAD + 1, 0xb2, false, tokens, input),
            complete(HEAD + 1, 0xb2),
        ],
    );
    assert_eq!(changes, vec![opened]);
    // A reorg takes the block: the position is not open; included
    // again in the new chain's block, it is.
    let changes = apply(
        &mut x,
        &mut c2,
        vec![CanonicalEvent::Reorg {
            orphaned: vec![(HEAD + 1, [0xb2; 32])],
            new_head: (HEAD + 1, [0xc2; 32]),
        }],
    );
    assert_eq!(changes, vec![PositionChange::Ended { token }]);
    let changes = apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 1, 0xc2, 0xb1, vec![tx(buy_tx, 0, 200_000)]),
            trade(buy_tx, HEAD + 1, 0xc2, false, tokens, input),
            complete(HEAD + 1, 0xc2),
        ],
    );
    assert_eq!(changes, vec![opened]);
    // The exit: approval, then (on a ready connection) the SELL of the
    // real tokens, both signed by the buyer key.
    let held = StrategyCurve {
        reserve: bought.reserve_after,
        circulating: bought.supply_after,
        ..curve
    };
    let exit = StrategyAction::Exit {
        token,
        tokens,
        curve: held,
        decided_ns: 0,
    };
    // BSC3-056: a second source that does not confirm the chain holds
    // no exit back.
    x.set_confirmation(Some(crate::canonical_confirm::Confirmation {
        verdict: crate::canonical_confirm::Verdict::Disagrees { height: HEAD + 1 },
        max_lag_blocks: 2,
    }));
    // R7: unless the divergence quarantine is ON: then nothing goes.
    x.executor
        .as_mut()
        .unwrap()
        .config_mut()
        .live_divergence_quarantine = true;
    reconnect(&mut x);
    let (out, _) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
    assert!(out.is_empty());
    assert!(
        refused_for(&x, "UNCONFIRMED"),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    assert!(requests.recv_timeout(Duration::from_millis(300)).is_err());
    x.executor
        .as_mut()
        .unwrap()
        .config_mut()
        .live_divergence_quarantine = false;
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, IntentKind::Approve);
    // BSC3-055: the SELL waits for the approval to be canonical.
    assert_eq!(refused, None);
    let approve_tx = out[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    let changes = apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 2, 0xc3, 0xc2, vec![tx(approve_tx, 1, 50_000)]),
            complete(HEAD + 2, 0xc3),
        ],
    );
    assert_eq!(changes, Vec::new());
    // The approval is canonical: the exit continues with the SELL.
    assert_eq!(x.exits_to_continue(&c2), vec![token]);
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    let sell_tx = out[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    let positions = &x.book().positions;
    assert_eq!(positions[0].state, PositionState::Exiting);
    let proceeds = quote_native_sell(
        &darkbot_bsc_trading::economics::flap::native_curve20::NativeSellInputs {
            token: terms,
            reserve_before: held.reserve,
            circulating_before: held.circulating,
            input_amount: tokens,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
        },
    )
    .unwrap()
    .output_amount;
    let changes = apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 3, 0xc4, 0xc3, vec![tx(sell_tx, 2, 150_000)]),
            trade(sell_tx, HEAD + 3, 0xc4, true, tokens, proceeds),
            complete(HEAD + 3, 0xc4),
        ],
    );
    assert_eq!(changes, vec![PositionChange::Ended { token }]);
    let gas = U256::from((200_000u64 + 50_000 + 150_000) * 50_000_000);
    let p = &x.book().positions[0];
    assert_eq!(p.state, PositionState::Closed);
    assert_eq!(p.gas_wei, gas);
    let pnl = i128::try_from(proceeds).unwrap()
        - i128::try_from(input).unwrap()
        - i128::try_from(gas).unwrap();
    assert_eq!(p.pnl_wei, Some(pnl));
    let r = record(&x);
    assert_eq!(r.u64(l::POSITIONS_CLOSED).unwrap(), 1);
    assert_eq!(r.text(l::REALIZED_PNL_WEI).unwrap(), pnl.to_string());
    // After a restart the ledger gives the same book.
    let restarted =
        crate::live_ledger::fold(&crate::live_ledger::Ledger::open(&state).facts().unwrap());
    assert_eq!(&restarted, x.book());
    x.shutdown();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-030: an exit step of the arm that reverted (included: its
/// nonce taken, its gas paid, nothing done) leaves the position open
/// and its exit open again: a reverted approval approved nothing, so
/// the exit approves again; a reverted SELL sold nothing, so it sells
/// again; each a new decision at the next nonce through its own owner
/// admission, never the reverted bytes. Before, the arm took the token
/// as exited for good (its tokens held, no exit until a restart) and
/// sold after a reverted approval without one.
#[test]
fn a_reverted_exit_step_goes_on_at_the_next_nonce() {
    reverted_exit_steps("live-rev-go", 3, false);
}

/// BSC3-030 review MEDIUM-1: an exit whose steps keep reverting is no
/// automatic repetition: after `BSC_EXEC_LIVE_EXIT_REVERTS_MAX` reverted
/// steps of the position (here an APPROVE and a SELL, limit 2) its exits
/// stop for the operator: none continues, the strategy's exit is
/// refused EXIT_STOPPED with the reason in the status, also after a
/// restart (counted in the ledger).
#[test]
fn an_exit_that_keeps_reverting_stops_for_the_operator() {
    reverted_exit_steps("live-rev-stop", 2, false);
}

/// BSC3-030 review MEDIUM-1: a SELL that reverted using all its gas
/// stops the position's exits at once (the same limit fails again),
/// below the revert limit too.
#[test]
fn an_exit_step_out_of_gas_stops_the_exits_at_once() {
    reverted_exit_steps("live-rev-oog", 3, true);
}

/// An open Strategy C position whose exit's APPROVE reverts, then its
/// SELL (all its gas: `sell_out_of_gas`), under a revert limit of
/// `reverts_max`; what follows is the case's.
fn reverted_exit_steps(tag: &str, reverts_max: u64, sell_out_of_gas: bool) {
    use crate::chain_follower::{CanonicalTrade, TxCost};
    use crate::live_ledger::PositionState;
    use crate::paper_exec::{IntentKind, StrategyCurve};
    use crate::runtime_strategy::StrategyAction;
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
        quote_native_sell,
    };
    let (state, _store, _) = live_ready_granting(tag, |now| {
        rig_grant(now + 3_600_000, LiveStrategies::StrategyC)
    });
    let root = fixture(tag, "PASS");
    let spec = rig_spec(
        Some(crate::live_qualification::running_build().unwrap()),
        LiveStrategies::StrategyC,
    );
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::write(
        root.join(LIVE_QUALIFICATION_PATH),
        build(&root, &spec, 1).unwrap(),
    )
    .unwrap();
    let d = std::env::temp_dir().join(format!("bsc-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    let (key, buyer) = loopback_key(&state);
    let (port, requests) = stand_in();
    let live_setup = LiveSetup {
        key_path: key,
        buyer,
        route: LiveRouteConfig::Loopback { port },
        connect_timeout: Duration::from_secs(2),
    };
    let mut exec = setup(d.join("unused.key"), Duration::from_secs(3_600));
    exec.exec.live_exit_reverts_max = reverts_max;
    let runtime = |boot: &str| {
        LiveRuntime::with_prober(
            live_setup.clone(),
            exec.clone(),
            &root,
            state.clone(),
            d.join("fences"),
            None,
            funded,
        )
        .in_session(boot, fixed)
    };
    let mut x = runtime("boot-1");
    let mut c2 = owner(&d, buyer);
    let block = |n: u64, h: u8, parent: u8, txs| CanonicalEvent::Block {
        number: n,
        hash: [h; 32],
        parent_hash: [parent; 32],
        timestamp_s: 1_793_000_000 + (n - HEAD),
        timestamp_ms: 1_793_000_000_000 + (n - HEAD) * 1_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs,
    };
    c2.canonical(&block(HEAD, 0xb1, 0xb0, Vec::new()));
    c2.canonical(&CanonicalEvent::NonceFact {
        sender: buyer,
        next_nonce: 0,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = c2.take_evidence();
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", record(&x).opt_text(l::REASON));
    let reconnect = |x: &mut LiveRuntime| {
        let failed = x
            .executor
            .as_mut()
            .unwrap()
            .end_mut()
            .connect(Duration::from_secs(2));
        assert!(failed.is_empty());
    };
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let token = [0xAB; 20];
    let input = U256::from(10_000_000_000_000_000u64);
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let fresh = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(1_000_000_000_000_000_000u128),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let curve = StrategyCurve {
        terms,
        reserve: fresh.reserve_after,
        circulating: fresh.supply_after,
    };
    let buy = StrategyAction::Buy {
        token,
        creator: [0; 20],
        curve,
        input_wei: input,
        decided_ns: 0,
    };
    let (intents, refused) = x.on_strategy(&buy, &c2, &clock, now_ms).unwrap();
    assert_eq!((intents.len(), refused), (1, None));
    let buy_tx = intents[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(x.book().unresolved().count(), 1);
    // Its receipt: included with its gas and the Portal's trade.
    let bought = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: curve.reserve,
        circulating_before: curve.circulating,
        input_amount: input,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let tokens = bought.output_amount;
    let tx = |hash, nonce, gas: u64| CanonicalTx {
        hash,
        index: 0,
        sender: buyer,
        nonce,
        succeeded: true,
        cost: Some(TxCost {
            gas_used: gas,
            effective_gas_price: 50_000_000,
        }),
    };
    let trade = |hash, n: u64, h: u8, sell, amount: U256, eth: U256| {
        CanonicalEvent::FlapTrade(CanonicalTrade {
            block_number: n,
            block_hash: [h; 32],
            tx_index: 0,
            tx_hash: hash,
            log_index: 0,
            origin: buyer,
            sell,
            token,
            trader: buyer,
            amount: amount.to_be_bytes(),
            eth: eth.to_be_bytes(),
            fee: [0; 32],
            post_price: [0; 32],
        })
    };
    // As the runtime applies a canonical event: C2 first, its evidence
    // to the LIVE path, then the follower's facts.
    let apply = |x: &mut LiveRuntime, c2: &mut StateOwner, events: Vec<CanonicalEvent>| {
        let mut changes = Vec::new();
        for e in &events {
            c2.canonical(e);
            let evidence = c2.take_evidence();
            let _ = x.apply_evidence(&evidence);
            changes.extend(x.on_canonical(e));
        }
        changes
    };
    let complete = |n: u64, h: u8| CanonicalEvent::BlockComplete {
        number: n,
        hash: [h; 32],
    };
    let opened = PositionChange::Opened {
        token,
        tokens,
        spent_wei: input + U256::from(200_000u64 * 50_000_000),
        filled_ms: 1_793_000_001_000,
    };
    let changes = apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 1, 0xc2, 0xb1, vec![tx(buy_tx, 0, 200_000)]),
            trade(buy_tx, HEAD + 1, 0xc2, false, tokens, input),
            complete(HEAD + 1, 0xc2),
        ],
    );
    assert_eq!(changes, vec![opened]);
    // The exit: approval, then (on a ready connection) the SELL of the
    // real tokens, both signed by the buyer key.
    let held = StrategyCurve {
        reserve: bought.reserve_after,
        circulating: bought.supply_after,
        ..curve
    };
    let exit = StrategyAction::Exit {
        token,
        tokens,
        curve: held,
        decided_ns: 0,
    };
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, IntentKind::Approve);
    // BSC3-055: the SELL waits for the approval to be canonical.
    assert_eq!(refused, None);
    let approve_tx = out[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let reverted = |hash, nonce, gas| CanonicalTx {
        succeeded: false,
        ..tx(hash, nonce, gas)
    };
    // The approval reverts: approved nothing; the exit approves again.
    apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 2, 0xc3, 0xc2, vec![reverted(approve_tx, 1, 50_000)]),
            complete(HEAD + 2, 0xc3),
        ],
    );
    assert!(!x.book().positions[0].approved);
    assert_eq!(x.exits_to_continue(&c2), vec![token]);
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Approve);
    let approve_again = out[0].handoff.tx_hash;
    assert_ne!(approve_again, approve_tx);
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 3, 0xc4, 0xc3, vec![tx(approve_again, 2, 50_000)]),
            complete(HEAD + 3, 0xc4),
        ],
    );
    assert_eq!(x.exits_to_continue(&c2), vec![token]);
    reconnect(&mut x);
    let spent = |x: &LiveRuntime| x.executor.as_ref().unwrap().strategy_lanes().3;
    let before_sell = spent(&x);
    let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    let sell_tx = out[0].handoff.tx_hash;
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let at_handoff = spent(&x);
    // The SELL's gas in, the position's input out of the exposure.
    let sell_gas = at_handoff + input - before_sell;
    // The SELL reverts: sold nothing; the position stays open.
    let reverted_sell = match sell_out_of_gas {
        false => reverted(sell_tx, 3, 150_000),
        true => CanonicalTx {
            succeeded: false,
            cost: Some(TxCost {
                gas_used: x.exec.exec.sell_gas_limit,
                effective_gas_price: x.exec.exec.gas_price_wei,
            }),
            ..tx(sell_tx, 3, 0)
        },
    };
    let changes = apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 4, 0xc5, 0xc4, vec![reverted_sell]),
            complete(HEAD + 4, 0xc5),
        ],
    );
    assert_eq!(changes, Vec::new());
    let p = &x.book().positions[0];
    // Open again: no SELL of it is on its way any more.
    assert_eq!(p.state, PositionState::Open);
    assert!(p.approved);
    assert_eq!(p.pnl_wei, None);
    let stop = match (reverts_max, sell_out_of_gas) {
        (_, true) => Some(crate::sources::LIVE_EXIT_OUT_OF_GAS),
        (2, false) => Some(crate::sources::LIVE_EXIT_REVERTS_EXHAUSTED),
        _ => None,
    };
    if let Some(why) = stop {
        // Stopped for the operator: nothing continues, the strategy's
        // exit is refused, and so after a restart.
        assert!(x.executor.as_ref().unwrap().exited(&token), "not reopened");
        assert!(!x.exits_wanted.contains(&token), "not wanted");
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        // Even wanted and reopened (another way in), it does not go on.
        x.exits_wanted.push(token);
        x.executor.as_mut().unwrap().reopen_exit(token, false);
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        x.exits_wanted.retain(|t| *t != token);
        reconnect(&mut x);
        let (out, refused) = x.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::ExitStopped)));
        assert!(
            record(&x)
                .texts(l::REFUSALS)
                .unwrap()
                .contains(&format!("{why}=1").as_str()),
            "{:?}",
            record(&x).texts(l::REFUSALS)
        );
        assert!(requests.try_recv().is_err(), "nothing left");
        // Review MEDIUM: no new BUY while its exits wait for the operator.
        let other = StrategyAction::Buy {
            token: [0xCD; 20],
            creator: [0; 20],
            curve,
            input_wei: input,
            decided_ns: 0,
        };
        let (out, refused) = x.on_strategy(&other, &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::ExitStopped)));
        assert!(requests.try_recv().is_err(), "no BUY left");
        x.shutdown();
        // A new runtime (a new process of the same boot session: a new
        // boot first needs the operator again) reads the same ledger.
        let mut again = runtime("boot-1");
        settle(&mut again, &c2);
        assert!(again.armed(), "{:?}", reason(&again));
        assert_eq!(again.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
        let (out, refused) = again.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
        assert_eq!((out.len(), refused), (0, Some(Refusal::ExitStopped)));
        again.shutdown();
        if sell_out_of_gas {
            // A raised SELL gas limit lets the exit go on (review LOW).
            let mut raised = exec.clone();
            raised.exec.sell_gas_limit += 1;
            let mut lifted = LiveRuntime::with_prober(
                live_setup.clone(),
                raised,
                &root,
                state.clone(),
                d.join("fences"),
                None,
                funded,
            )
            .in_session("boot-1", fixed);
            settle(&mut lifted, &c2);
            assert!(lifted.armed(), "{:?}", reason(&lifted));
            reconnect(&mut lifted);
            let (out, refused) = lifted.on_strategy(&exit, &c2, &clock, now_ms).unwrap();
            assert_eq!((out.len(), refused), (1, None));
            assert_eq!(out[0].kind, IntentKind::Sell);
            lifted.shutdown();
        }
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(state).unwrap();
        return;
    }
    // Its exit goes on with a new SELL at the next nonce, quoted on the
    // curve as it is now: other holders sold meanwhile (a crash).
    assert_eq!(x.exits_to_continue(&c2), vec![token]);
    let dump = quote_native_sell(
        &darkbot_bsc_trading::economics::flap::native_curve20::NativeSellInputs {
            token: terms,
            reserve_before: held.reserve,
            circulating_before: held.circulating,
            input_amount: tokens,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
        },
    )
    .unwrap();
    let crashed = StrategyCurve {
        reserve: dump.reserve_after,
        circulating: dump.supply_after,
        ..held
    };
    let exit_again = StrategyAction::Exit {
        token,
        tokens,
        curve: crashed,
        decided_ns: 0,
    };
    // Review LOW: open again, its input is exposure again.
    let reopened = spent(&x);
    assert_eq!(reopened, at_handoff + input);
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit_again, &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    let sell_again = out[0].handoff.tx_hash;
    assert_ne!(sell_again, sell_tx);
    requests.recv_timeout(Duration::from_secs(2)).unwrap();
    // The position's input leaves the exposure once more, its gas adds.
    assert_eq!(spent(&x), reopened + sell_gas - input);
    let proceeds = quote_native_sell(
        &darkbot_bsc_trading::economics::flap::native_curve20::NativeSellInputs {
            token: terms,
            reserve_before: crashed.reserve,
            circulating_before: crashed.circulating,
            input_amount: tokens,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
        },
    )
    .unwrap()
    .output_amount;
    assert!(
        proceeds < dump.output_amount,
        "the crash lowered the proceeds"
    );
    let changes = apply(
        &mut x,
        &mut c2,
        vec![
            block(HEAD + 5, 0xc6, 0xc5, vec![tx(sell_again, 4, 150_000)]),
            trade(sell_again, HEAD + 5, 0xc6, true, tokens, proceeds),
            complete(HEAD + 5, 0xc6),
        ],
    );
    assert_eq!(changes, vec![PositionChange::Ended { token }]);
    // Every step's gas counts, the reverted ones too.
    let gas = U256::from((200_000u64 + 2 * 50_000 + 2 * 150_000) * 50_000_000);
    let p = &x.book().positions[0];
    assert_eq!(p.state, PositionState::Closed);
    assert_eq!(p.gas_wei, gas);
    let pnl = i128::try_from(proceeds).unwrap()
        - i128::try_from(input).unwrap()
        - i128::try_from(gas).unwrap();
    assert_eq!(p.pnl_wei, Some(pnl));
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    // After a restart the ledger gives the same book.
    let restarted =
        crate::live_ledger::fold(&crate::live_ledger::Ledger::open(&state).facts().unwrap());
    assert_eq!(&restarted, x.book());
    x.shutdown();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// A C3 LIVE scope on the loopback stand-in: the owner's state, the
/// qualification's repository, the run directory, the setup and what
/// the stand-in receives.
struct Rig {
    state: PathBuf,
    root: PathBuf,
    d: PathBuf,
    buyer: [u8; 20],
    strategies: LiveStrategies,
    live: LiveSetup,
    exec: ExecSetup,
    requests: std::sync::mpsc::Receiver<Vec<u8>>,
}

/// The LIVE path tests' grant of `strategies`, valid until `expires`.
fn rig_grant(expires: u64, strategies: LiveStrategies) -> LiveGrant {
    LiveGrant {
        max_input_wei: 10_000_000_000_000_000,
        max_spend_wei: 100_000_000_000_000_000,
        adapters: vec![LiveAdapter::Club48BuilderV1],
        ..grant_of(expires, strategies)
    }
}

/// The qualification of the fixture's evidence (`ev/`) for the scope
/// of [`rig_grant`] (BSC3-049: its risk decided for that scope),
/// naming the build `runtime`.
fn rig_spec(runtime: Option<[u8; 32]>, strategies: LiveStrategies) -> Spec {
    rig_spec_for(runtime, rig_grant(0, strategies).risk_scope_sha256())
}

/// [`rig_spec`] with the risk decided for the grant risk scope `scope`.
fn rig_spec_for(runtime: Option<[u8; 32]>, scope: [u8; 32]) -> Spec {
    Spec {
        runtime_sha256: runtime,
        venues: vec![(VenueId::Flap, venue_evidence())],
        sources: vec![(SourceId::PublicNode, vec!["ev/g04.dbrec".into()])],
        adapters: vec![(
            LiveAdapter::Club48BuilderV1,
            club48_findings(),
            vec!["ev/g07.dbrec".into()],
        )],
        acquisition: acquisition(),
        risk: Some(RiskSpec {
            decided_by: "operator".into(),
            decided_unix_ms: 1,
            statement: "synthetic scope".into(),
            scope_sha256: scope,
            evidence: vec!["ev/g08.dbrec".into()],
        }),
    }
}

impl Rig {
    fn new(tag: &str, strategies: LiveStrategies) -> Rig {
        Rig::granting(tag, strategies, |now| {
            rig_grant(now + 3_600_000, strategies)
        })
    }

    /// [`Rig::new`] under the grant `grant` makes (its risk decided for
    /// its own scope).
    fn granting(tag: &str, strategies: LiveStrategies, grant: impl Fn(u64) -> LiveGrant) -> Rig {
        let scope = grant(0).risk_scope_sha256();
        let (state, _store, now) = live_ready_granting(tag, &grant);
        assert_eq!(now, NOW);
        let root = fixture(tag, "PASS");
        let spec = rig_spec_for(
            Some(crate::live_qualification::running_build().unwrap()),
            scope,
        );
        fs::create_dir_all(root.join("bsc/config")).unwrap();
        fs::write(
            root.join(LIVE_QUALIFICATION_PATH),
            build(&root, &spec, 1).unwrap(),
        )
        .unwrap();
        let d = std::env::temp_dir().join(format!("bsc-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let (key, buyer) = loopback_key(&state);
        let (port, requests) = stand_in();
        Rig {
            exec: setup(d.join("unused.key"), Duration::from_secs(3_600)),
            live: LiveSetup {
                key_path: key,
                buyer,
                route: LiveRouteConfig::Loopback { port },
                connect_timeout: Duration::from_secs(2),
            },
            state,
            root,
            d,
            buyer,
            strategies,
            requests,
        }
    }

    /// [`Rig::new`] whose builder stand-in answers with `answers`.
    fn answering(
        tag: &str,
        strategies: LiveStrategies,
        answers: Vec<crate::live_end::tests::Answer>,
    ) -> Rig {
        let mut r = Rig::new(tag, strategies);
        let (port, requests) = crate::live_end::tests::answering(answers);
        r.live.route = LiveRouteConfig::Loopback { port };
        r.requests = requests;
        r
    }

    /// The LIVE path of one process in the boot session `boot`.
    fn runtime(&self, boot: &str) -> LiveRuntime {
        self.probed(boot, funded)
    }

    /// [`Rig::runtime`] whose probe sees what `prober` answers. As the
    /// runtime tells once its strategy holds every open lot (R1; a test
    /// of the deciders themselves resets them).
    fn probed(&self, boot: &str, prober: Prober) -> LiveRuntime {
        let mut x = LiveRuntime::with_prober(
            self.live.clone(),
            self.exec.clone(),
            &self.root,
            self.state.clone(),
            self.d.join("fences"),
            None,
            prober,
        )
        .in_session(boot, fixed);
        x.set_exit_deciders(Vec::new(), Vec::new(), 0, None);
        x
    }

    /// The operator authorizes LIVE again in `boot` (after a restart
    /// reset the switches or a revoke took the grant): every switch
    /// ENABLED and a new grant.
    fn authorize(&self, boot: &str, request: &str) {
        let s = Store::open_in_session(
            &self.state,
            crate::sources::tests::keeping(8),
            Ok(boot.into()),
        )
        .unwrap();
        s.restart(NOW).unwrap();
        for (i, id) in crate::sources::SwitchId::ALL.into_iter().enumerate() {
            let g = s.current().unwrap().generation;
            let r = s
                .request(
                    &format!("{request}-{i}"),
                    Entity::Switch(id),
                    Mode::Enabled,
                    g,
                    NOW,
                )
                .unwrap();
            assert!(r.applied(), "{}", r.line());
        }
        let grant = LiveGrant {
            max_input_wei: 10_000_000_000_000_000,
            max_spend_wei: 100_000_000_000_000_000,
            adapters: vec![LiveAdapter::Club48BuilderV1],
            ..grant_of(NOW + 3_600_000, self.strategies)
        };
        let g = s.current().unwrap().generation;
        let r = s
            .grant_live(request, grant.clone(), grant.scope_sha256(), g, NOW)
            .unwrap();
        assert!(r.applied(), "{}", r.line());
    }

    fn fences(&self, suffix: &str) -> Vec<String> {
        let mut out: Vec<String> = fs::read_dir(self.d.join("fences"))
            .map(|e| {
                e.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.ends_with(suffix))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    fn clean(self) {
        fs::remove_dir_all(self.d).unwrap();
        fs::remove_dir_all(self.root).unwrap();
        fs::remove_dir_all(self.state).unwrap();
    }
}

/// REV.1.0 R2: the canonical launch of `token` by `creator` at `index`
/// of block `n` (hash `[h; 32]`).
fn launch_event(n: u64, h: u8, index: u64, token: [u8; 20], creator: [u8; 20]) -> CanonicalEvent {
    use crate::canonical_join::Venue;
    use crate::chain_follower::CanonicalLaunch;
    CanonicalEvent::Launch(CanonicalLaunch {
        venue: Venue::Flap,
        block_number: n,
        block_hash: [h; 32],
        tx_index: index,
        tx_hash: [0x1a; 32],
        token: Some(token),
        creator: Some(creator),
        external_buys_after: None,
        first_external_buy_index: None,
        sender: Some(creator),
        nonce: Some(0),
        c1_scope: true,
        venue_target: true,
        four_token: crate::canonical_join::FourToken::NotApplicable,
        four_raw: None,
        unverifiable: None,
    })
}

/// REV.1.0 R2: a Portal BUY of `token` by `trader` in transaction
/// `tx` at `index` of block `n`.
fn buy_trade(
    n: u64,
    h: u8,
    index: u64,
    tx: [u8; 32],
    token: [u8; 20],
    trader: [u8; 20],
) -> CanonicalEvent {
    CanonicalEvent::FlapTrade(crate::chain_follower::CanonicalTrade {
        block_number: n,
        block_hash: [h; 32],
        tx_index: index,
        tx_hash: tx,
        log_index: 0,
        origin: trader,
        sell: false,
        token,
        trader,
        amount: U256::from(1_000_000u64).to_be_bytes(),
        eth: U256::from(1_000u64).to_be_bytes(),
        fee: [0; 32],
        post_price: [0; 32],
    })
}

/// C2 at `HEAD` with the buyer's next nonce `next`.
fn c2_at(dir: &Path, buyer: [u8; 20], next: u64) -> StateOwner {
    let mut c2 = owner(dir, buyer);
    c2.canonical(&CanonicalEvent::Block {
        number: HEAD,
        hash: [0xb1; 32],
        parent_hash: [0xb0; 32],
        timestamp_s: 1_793_000_000,
        timestamp_ms: 1_793_000_000_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: Vec::new(),
    });
    c2.canonical(&CanonicalEvent::NonceFact {
        sender: buyer,
        next_nonce: next,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = c2.take_evidence();
    c2
}

/// Block `n` (hash `[h; 32]`, parent `[parent; 32]`) with `txs`.
fn block_of(n: u64, h: u8, parent: u8, txs: Vec<CanonicalTx>) -> CanonicalEvent {
    CanonicalEvent::Block {
        number: n,
        hash: [h; 32],
        parent_hash: [parent; 32],
        timestamp_s: 1_793_000_000 + (n - HEAD),
        timestamp_ms: 1_793_000_000_000 + (n - HEAD) * 1_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs,
    }
}

fn complete_of(n: u64, h: u8) -> CanonicalEvent {
    CanonicalEvent::BlockComplete {
        number: n,
        hash: [h; 32],
    }
}

/// `events` as the runtime applies them: C2 first, its evidence to the
/// LIVE path, then the follower's facts.
fn feed(
    x: &mut LiveRuntime,
    c2: &mut StateOwner,
    events: Vec<CanonicalEvent>,
) -> Vec<PositionChange> {
    let mut changes = Vec::new();
    for event in &events {
        c2.canonical(event);
        let evidence = c2.take_evidence();
        let _ = x.apply_evidence(&evidence);
        changes.extend(x.on_canonical(event));
    }
    changes
}

/// Block `HEAD + 1` with `txs`, as the runtime applies it.
fn include(x: &mut LiveRuntime, c2: &mut StateOwner, txs: Vec<CanonicalTx>) -> Vec<PositionChange> {
    feed(
        x,
        c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, txs),
            complete_of(HEAD + 1, 0xb2),
        ],
    )
}

fn reason(x: &LiveRuntime) -> (String, Option<String>) {
    let r = record(x);
    (
        r.text(l::STATE).unwrap().to_string(),
        r.opt_text(l::REASON).unwrap().map(str::to_string),
    )
}

/// BSC3-024/041: the process dies right after its BUY was handed to the
/// builders. The restart knows the BUY (BSC3-076: the owner's lease of
/// it, its nonce and window on the medium before the launch was seen),
/// refuses a new arm while the BUY's nonce may still be taken by it,
/// adopts the buyer's transaction at that nonce as the BUY when the
/// chain includes it (its token the Portal's trade) and opens the
/// position, and only then arms again, at the next nonce. Nothing
/// leaves in between.
#[test]
fn a_release_lost_in_a_crash_is_recovered_and_blocks_the_next_arm() {
    let r = Rig::new("live-crash", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (hash, raw, launcher) = launch();
    let mut c2 = c2;
    c2.pending(hash, launcher, 3, 0, 10);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    // The BUY leaves; the process is killed before anything else runs.
    let next = x.free_nonce(&c2);
    let reserve = Some(x.exit_reserve());
    let buy = launch_to(
        x.executor.as_mut().unwrap(),
        hash,
        [0; 20],
        &raw,
        10,
        &c2,
        next,
        reserve,
        &clock,
    )
    .unwrap();
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    std::mem::forget(x);
    // The kill came between the dispatch and the outcome's fact: only
    // the owner's lease, durable before the launch was seen, holds the
    // BUY.
    for e in fs::read_dir(r.state.join("live-ledger")).unwrap() {
        fs::remove_file(e.unwrap().path()).unwrap();
    }
    let leases: Vec<_> = crate::sources::admitted_intents(&r.state)
        .unwrap()
        .into_iter()
        .filter(|a| a.lease)
        .map(|a| a.tx_hash)
        .collect();
    assert_eq!(leases.len(), 1);
    // The restart: the lease is a fact, UNKNOWN and unresolved at the
    // BUY's nonce.
    r.authorize("boot-2", "after-crash");
    let mut x = r.runtime("boot-2");
    let unresolved: Vec<_> = x.book().unresolved().map(|t| t.released.tx_hash).collect();
    assert_eq!(unresolved, leases);
    let tx = &x.book().tx(&leases[0]).unwrap().released;
    assert_eq!(
        (tx.kind, tx.token, tx.nonce),
        (crate::live_ledger::FactKind::Unknown, [0; 20], 5)
    );
    // C2 restarted, the nonce still 5: no new arm while the BUY may
    // take it.
    let mut c2 = c2_at(&r.d.join("c2-restarted"), r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!x.armed());
    let (state, why) = reason(&x);
    assert_eq!(state, "BLOCKED");
    assert!(
        why.as_deref()
            .unwrap()
            .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED),
        "{why:?}"
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    // The chain includes the buyer's transaction at that nonce, the
    // Portal's trade in it: adopted as the lease's BUY, the position
    // opens; the next arm takes the next nonce.
    let changes = feed(
        &mut x,
        &mut c2,
        vec![
            block_of(
                HEAD + 1,
                0xb2,
                0xb1,
                vec![CanonicalTx {
                    hash: buy.handoff.tx_hash,
                    index: 0,
                    sender: r.buyer,
                    nonce: 5,
                    succeeded: true,
                    cost: None,
                }],
            ),
            CanonicalEvent::FlapTrade(crate::chain_follower::CanonicalTrade {
                block_number: HEAD + 1,
                block_hash: [0xb2; 32],
                tx_index: 0,
                tx_hash: buy.handoff.tx_hash,
                log_index: 0,
                origin: r.buyer,
                sell: false,
                token: buy.token,
                trader: r.buyer,
                amount: U256::from(1_000u64).to_be_bytes(),
                eth: U256::from(1_000_000_000_000_000u64).to_be_bytes(),
                fee: [0; 32],
                post_price: [0; 32],
            }),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    assert!(
        matches!(changes.as_slice(), [PositionChange::Opened { token, .. }] if *token == buy.token),
        "{changes:?}"
    );
    assert_eq!(x.book().unresolved().count(), 0);
    let adopted = x.book().tx(&buy.handoff.tx_hash).unwrap();
    assert_eq!(
        (
            adopted.state,
            adopted.released.token,
            adopted.released.nonce
        ),
        (crate::live_ledger::FactKind::Included, buy.token, 5)
    );
    assert_eq!(
        x.book().tx(&leases[0]).unwrap().state,
        crate::live_ledger::FactKind::Replaced
    );
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(6));
    // The crashed arm's fence is settled, not deleted.
    assert_eq!(r.fences(".fence").len(), 1);
    assert_eq!(r.fences(".fence.settled"), vec!["live-7-0.fence.settled"]);
    x.shutdown();
    r.clean();
}

/// BSC3-041: the gate closes while a released BUY is unresolved. The
/// arm's fence stays; a new grant does not arm while the BUY may take
/// its nonce; another transaction on that nonce (C2's inclusion
/// evidence, applied while nothing is armed) resolves it as replaced,
/// and then the path arms again.
#[test]
fn a_disarm_with_an_unresolved_release_keeps_its_fence_and_blocks_the_next_arm() {
    let r = Rig::new("live-unresolved", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .unwrap();
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    // Revoked with the BUY unresolved: the arm ends, its fence stays.
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    assert!(
        s.request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
            .unwrap()
            .applied()
    );
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(r.fences(".fence"), vec!["live-7-0.fence"]);
    // The next pass with the gate closed.
    settle(&mut x, &c2);
    assert_eq!(
        reason(&x),
        ("BLOCKED".to_string(), Some(LIVE_GRANT_MISSING.to_string()))
    );
    // A new grant: still no arm.
    r.authorize("boot-1", "regrant");
    settle(&mut x, &c2);
    assert!(!x.armed());
    let (state, why) = reason(&x);
    assert_eq!(state, "BLOCKED");
    assert!(
        why.as_deref()
            .unwrap()
            .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED),
        "{why:?}"
    );
    // Another transaction of the buyer takes nonce 5: the BUY never
    // happened; nothing opens.
    let changes = include(
        &mut x,
        &mut c2,
        vec![CanonicalTx {
            hash: [0xee; 32],
            index: 0,
            sender: r.buyer,
            nonce: 5,
            succeeded: true,
            cost: None,
        }],
    );
    assert_eq!(changes, Vec::new());
    let tx = x.book().tx(&buy.handoff.tx_hash).unwrap();
    assert_eq!(tx.state, crate::live_ledger::FactKind::Replaced);
    // That block leaves the chain: nonce 5 is open again, and so is
    // the BUY; still no arm.
    let changes = feed(
        &mut x,
        &mut c2,
        vec![CanonicalEvent::Reorg {
            orphaned: vec![(HEAD + 1, [0xb2; 32])],
            new_head: (HEAD + 1, [0xc2; 32]),
        }],
    );
    assert_eq!(changes, Vec::new());
    let tx = x.book().tx(&buy.handoff.tx_hash).unwrap();
    assert_eq!(tx.state, crate::live_ledger::FactKind::Released);
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert!(
        reason(&x)
            .1
            .unwrap()
            .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
    );
    // The new chain's block takes nonce 5 by the other one again.
    let other = CanonicalTx {
        hash: [0xee; 32],
        index: 0,
        sender: r.buyer,
        nonce: 5,
        succeeded: true,
        cost: None,
    };
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xc2, 0xb1, vec![other]),
            CanonicalEvent::NonceFact {
                sender: r.buyer,
                next_nonce: 6,
                block_number: HEAD + 1,
                block_hash: [0xc2; 32],
            },
            complete_of(HEAD + 1, 0xc2),
        ],
    );
    let tx = x.book().tx(&buy.handoff.tx_hash).unwrap();
    assert_eq!(tx.state, crate::live_ledger::FactKind::Replaced);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(6));
    assert_eq!(r.fences(".fence.settled"), vec!["live-7-0.fence.settled"]);
    x.shutdown();
    r.clean();
}

/// BSC3-055 (review HIGH): an exit refused for the sender's nonce order
/// is not lost. Position A is open; B's BUY is out at nonce 1 and not
/// canonical; A's exit would take nonce 2: nothing goes, the exit stays
/// wanted. Once B is canonical it continues: A's approval, and once
/// that is canonical, its SELL.
#[test]
fn an_exit_refused_for_the_nonce_order_continues_once_it_may() {
    use crate::chain_follower::{CanonicalTrade, TxCost};
    use crate::paper_exec::{IntentKind, Refusal, StrategyCurve};
    use crate::runtime_strategy::StrategyAction;
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
    };
    let r = Rig::new("live-exit-order", LiveStrategies::StrategyC);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let input = U256::from(10_000_000_000_000_000u64);
    let quote = |reserve, circulating, amount| {
        quote_native_buy(&NativeBuyInputs {
            token: terms,
            reserve_before: reserve,
            circulating_before: circulating,
            input_amount: amount,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap()
    };
    let fresh = quote(
        U256::ZERO,
        U256::ZERO,
        U256::from(1_000_000_000_000_000_000u128),
    );
    let curve = StrategyCurve {
        terms,
        reserve: fresh.reserve_after,
        circulating: fresh.supply_after,
    };
    let bought = quote(curve.reserve, curve.circulating, input);
    let tokens = bought.output_amount;
    let held = StrategyCurve {
        reserve: bought.reserve_after,
        circulating: bought.supply_after,
        ..curve
    };
    let buy = |token| StrategyAction::Buy {
        token,
        creator: [0; 20],
        curve,
        input_wei: input,
        decided_ns: 0,
    };
    let exit = |token| StrategyAction::Exit {
        token,
        tokens,
        curve: held,
        decided_ns: 0,
    };
    let buyer = r.buyer;
    let receipt = |hash, nonce| CanonicalTx {
        hash,
        index: 0,
        sender: buyer,
        nonce,
        succeeded: true,
        cost: Some(TxCost {
            gas_used: 200_000,
            effective_gas_price: 50_000_000,
        }),
    };
    let trade = |hash, n: u64, h: u8, token| {
        CanonicalEvent::FlapTrade(CanonicalTrade {
            block_number: n,
            block_hash: [h; 32],
            tx_index: 0,
            tx_hash: hash,
            log_index: 0,
            origin: buyer,
            sell: false,
            token,
            trader: buyer,
            amount: tokens.to_be_bytes(),
            eth: input.to_be_bytes(),
            fee: [0; 32],
            post_price: [0; 32],
        })
    };
    let reconnect = |x: &mut LiveRuntime| {
        let failed = x
            .executor
            .as_mut()
            .unwrap()
            .end_mut()
            .connect(Duration::from_secs(2));
        assert!(failed.is_empty());
    };
    let wait = Duration::from_secs(2);
    let silent = Duration::from_millis(300);
    let (a, b) = ([0xA1; 20], [0xB2; 20]);
    // A: bought at nonce 0 and included.
    let (out, refused) = x.on_strategy(&buy(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    let a_tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    let changes = feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![receipt(a_tx, 0)]),
            trade(a_tx, HEAD + 1, 0xb2, a),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    assert!(
        matches!(changes.as_slice(), [PositionChange::Opened { token, .. }] if *token == a),
        "{changes:?}"
    );
    // B: bought at nonce 1, not canonical yet.
    reconnect(&mut x);
    let (out, _) = x.on_strategy(&buy(b), &c2, &clock, now_ms).unwrap();
    assert_eq!(out.len(), 1);
    let b_tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    // A's exit triggers now: nothing goes, the exit stays wanted.
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
    assert!(r.requests.recv_timeout(silent).is_err());
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    // B canonical: A's exit goes on, its approval at nonce 2.
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 2, 0xb3, 0xb2, vec![receipt(b_tx, 1)]),
            trade(b_tx, HEAD + 2, 0xb3, b),
            complete_of(HEAD + 2, 0xb3),
        ],
    );
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    let (out, refused) = x.on_strategy(&exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Approve);
    let approve = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    // The approval not canonical: the SELL waits.
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 3, 0xb4, 0xb3, vec![receipt(approve, 2)]),
            complete_of(HEAD + 3, 0xb4),
        ],
    );
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    r.requests.recv_timeout(wait).unwrap();
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    x.shutdown();
    r.clean();
}

/// BSC3-048: when an armed path is renewed at the free nonce, and when
/// not (no loop: right after a renewal the cursor is the free nonce
/// and nothing is unresolved at it or stopped).
#[test]
fn an_arm_is_renewed_only_where_it_cannot_go_on() {
    // Taken by an earlier arm's transaction that landed late.
    assert!(rearm_due(7, 5, false, false));
    // The window of the unresolved transaction at the free nonce over.
    assert!(rearm_due(5, 6, false, true));
    // An UNKNOWN hand-over resolved: the free nonce is the next again.
    assert!(rearm_due(6, 6, true, false));
    // Going on: the cursor is the free nonce, nothing stopped.
    assert!(!rearm_due(6, 6, false, false));
    // Its own transaction out (the free nonce below, nothing
    // unresolved at it: C2 not clean yet) or none free: nothing.
    assert!(!rearm_due(5, 6, false, false));
}

/// Strategy C on one fresh curve: its BUY and exit actions, a buyer's
/// receipts and the Portal's BUY trades.
struct Sc {
    curve: crate::paper_exec::StrategyCurve,
    held: crate::paper_exec::StrategyCurve,
    tokens: U256,
    input: U256,
    buyer: [u8; 20],
}

impl Sc {
    fn new(buyer: [u8; 20]) -> Sc {
        use darkbot_bsc_trading::economics::flap::native_curve20::{
            HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
        };
        let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
        let input = U256::from(10_000_000_000_000_000u64);
        let quote = |reserve, circulating, amount| {
            quote_native_buy(&NativeBuyInputs {
                token: terms,
                reserve_before: reserve,
                circulating_before: circulating,
                input_amount: amount,
                protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                tax_on_bonding_curve_enabled: true,
                caller_fee_exempt: false,
                holding_cap: HoldingCap::None,
            })
            .unwrap()
        };
        let fresh = quote(
            U256::ZERO,
            U256::ZERO,
            U256::from(1_000_000_000_000_000_000u128),
        );
        let curve = crate::paper_exec::StrategyCurve {
            terms,
            reserve: fresh.reserve_after,
            circulating: fresh.supply_after,
        };
        let bought = quote(curve.reserve, curve.circulating, input);
        Sc {
            curve,
            held: crate::paper_exec::StrategyCurve {
                reserve: bought.reserve_after,
                circulating: bought.supply_after,
                ..curve
            },
            tokens: bought.output_amount,
            input,
            buyer,
        }
    }

    fn buy(&self, token: [u8; 20]) -> crate::runtime_strategy::StrategyAction {
        crate::runtime_strategy::StrategyAction::Buy {
            token,
            creator: [0; 20],
            curve: self.curve,
            input_wei: self.input,
            decided_ns: 0,
        }
    }

    fn exit(&self, token: [u8; 20]) -> crate::runtime_strategy::StrategyAction {
        crate::runtime_strategy::StrategyAction::Exit {
            token,
            tokens: self.tokens,
            curve: self.held,
            decided_ns: 0,
        }
    }

    fn receipt(&self, hash: [u8; 32], nonce: u64) -> CanonicalTx {
        CanonicalTx {
            hash,
            index: 0,
            sender: self.buyer,
            nonce,
            succeeded: true,
            cost: Some(crate::chain_follower::TxCost {
                gas_used: 200_000,
                effective_gas_price: 50_000_000,
            }),
        }
    }

    fn trade(&self, hash: [u8; 32], n: u64, h: u8, token: [u8; 20]) -> CanonicalEvent {
        CanonicalEvent::FlapTrade(crate::chain_follower::CanonicalTrade {
            block_number: n,
            block_hash: [h; 32],
            tx_index: 0,
            tx_hash: hash,
            log_index: 0,
            origin: self.buyer,
            sell: false,
            token,
            trader: self.buyer,
            amount: self.tokens.to_be_bytes(),
            eth: self.input.to_be_bytes(),
            fee: [0; 32],
            post_price: [0; 32],
        })
    }
}

/// The exit reserve the owner's admission entry of `tx` records
/// (BSC3-057).
fn reserve_of(state: &Path, tx: [u8; 32]) -> U256 {
    use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
    let name = format!("{}.dbrec", alloy_primitives::hex::encode(tx));
    let bytes = fs::read(state.join("live-spend").join(name)).unwrap();
    let entry = darkbot_bsc_record::decode(&ls::SCHEMA, &bytes).unwrap();
    U256::from_be_bytes(entry.opt_u256(ls::EXIT_RESERVE_WEI).unwrap().unwrap())
}

fn reconnect(x: &mut LiveRuntime) {
    let failed = x
        .executor
        .as_mut()
        .unwrap()
        .end_mut()
        .connect(Duration::from_secs(2));
    assert!(failed.is_empty());
}

/// BSC3-048: position A is open; B's BUY at nonce 1 is handed over and
/// its bundle is lost. While B's window (its block plus
/// `BSC_EXEC_VALIDITY_BLOCKS`) runs, nothing takes nonce 1 and A's exit
/// waits; no new BUY goes first. After the window the exit takes the
/// same nonce. Then B lands late after all, in place of the approval: B
/// is included and its position opens, the approval is REPLACED (by
/// B's hash, B's block) and A's exit goes on at the next nonce. Every
/// attempt stays in the ledger.
#[test]
fn a_lost_bundle_frees_its_nonce_after_its_window_and_the_exit_takes_it() {
    use crate::live_ledger::FactKind;
    use crate::paper_exec::{IntentKind, Refusal};
    let r = Rig::new("live-window", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let wait = Duration::from_secs(2);
    let silent = Duration::from_millis(300);
    let (a, b, c) = ([0xA1; 20], [0xB2; 20], [0xC3; 20]);
    // A: bought at nonce 0, included at HEAD + 1.
    let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
    let a_tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(a_tx, 0)]),
            sc.trade(a_tx, HEAD + 1, 0xb2, a),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    // B: handed over at nonce 1 (head HEAD + 1, window to HEAD + 3); its
    // bundle is lost.
    reconnect(&mut x);
    let (out, _) = x.on_strategy(&sc.buy(b), &c2, &clock, now_ms).unwrap();
    assert_eq!(out.len(), 1);
    let b_tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    // BSC3-057: B left the grant room for A's exit and its own.
    let (approval, sell) = x.exit_gas();
    assert_eq!(
        reserve_of(&r.state, b_tx),
        (approval + sell) * U256::from(2u8)
    );
    // A's exit: nonce 1 is B's while its window runs.
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
    for (n, h, p) in [(HEAD + 2, 0xb3, 0xb2), (HEAD + 3, 0xb4, 0xb3)] {
        feed(
            &mut x,
            &mut c2,
            vec![block_of(n, h, p, Vec::new()), complete_of(n, h)],
        );
        settle(&mut x, &c2);
        assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    }
    assert!(r.requests.recv_timeout(silent).is_err());
    // HEAD + 4: B's window is over. The exit takes nonce 1; a new BUY
    // does not go first.
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 4, 0xb5, 0xb4, Vec::new()),
            complete_of(HEAD + 4, 0xb5),
        ],
    );
    // Measured: the local part of the wait, the new arm (key, link)
    // up to the exit's hand-over.
    let local = Instant::now();
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Approve);
    let approve = out[0].handoff.tx_hash;
    eprintln!(
        "BSC3-048 window over -> exit handed over (local): {:?}",
        local.elapsed()
    );
    let signed = darkbot_bsc_trading::transactions::legacy::decode_signed(&out[0].raw).unwrap();
    assert_eq!(signed.tx.nonce, 1);
    r.requests.recv_timeout(wait).unwrap();
    // B lands late, in place of the approval.
    let changes = feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 5, 0xb6, 0xb5, vec![sc.receipt(b_tx, 1)]),
            sc.trade(b_tx, HEAD + 5, 0xb6, b),
            complete_of(HEAD + 5, 0xb6),
        ],
    );
    assert!(
        matches!(changes.as_slice(), [PositionChange::Opened { token, .. }] if *token == b),
        "{changes:?}"
    );
    let book = x.book();
    assert_eq!(book.tx(&b_tx).unwrap().state, FactKind::Included);
    let replaced = book.tx(&approve).unwrap();
    assert_eq!(replaced.state, FactKind::Replaced);
    let by = replaced.inclusion.as_ref().unwrap();
    assert_eq!(
        (by.by_tx, by.block.map(|b| b.0)),
        (Some(b_tx), Some(HEAD + 5))
    );
    // A's exit goes on at nonce 2.
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Approve);
    let signed = darkbot_bsc_trading::transactions::legacy::decode_signed(&out[0].raw).unwrap();
    assert_eq!(signed.tx.nonce, 2);
    r.requests.recv_timeout(wait).unwrap();
    // Every attempt stays in the ledger: A, B, the replaced approval,
    // the new one.
    assert_eq!(x.book().txs.len(), 4);
    x.shutdown();
    r.clean();
}

/// R5 review M1: the re-price is above the step's price and the arm's,
/// at most the grant's maximum.
#[test]
fn a_reprice_is_above_the_step_and_the_arm_at_most_the_maximum() {
    assert_eq!(
        repriced(50_000_000, 1_000, 50_000_000, 55_000_000),
        Some(55_000_000)
    );
    assert_eq!(
        repriced(50_000_000, 1_000, 50_000_000, 52_000_000),
        Some(52_000_000)
    );
    assert_eq!(repriced(55_000_000, 1_000, 50_000_000, 55_000_000), None);
    // A higher arm price is new bytes already.
    assert_eq!(repriced(50_000_000, 1_000, 60_000_000, 100_000_000), None);
    // A prior derived low (a raised gas limit) never goes below the arm.
    assert_eq!(repriced(33_333_334, 1_000, 50_000_000, 100_000_000), None);
    assert_eq!(repriced(1, 1, 0, u64::MAX), Some(2));
    assert_eq!(repriced(u64::MAX - 1, 9_999, 0, u64::MAX), Some(u64::MAX));
    assert_eq!(repriced(50_000_000, 0, 0, u64::MAX), None);
}

/// R5 (BSC3-030): with BSC_EXEC_LIVE_EXIT_REPRICE_BPS a lost exit step
/// (an approval, then a SELL) goes again at its nonce at its price
/// raised by the bps (another transaction the owner admits), at most
/// the grant's maximum; at it, the arm's price again: the same bytes,
/// refused (LIVE_INTENT_SEEN). Every lot's exit is reserved at that
/// maximum (review LOW 3).
#[test]
fn a_lost_exit_step_is_repriced_up_to_the_grants_maximum() {
    use crate::paper_exec::{IntentKind, Refusal};
    use crate::sources::GrantRisk;
    use crate::sources::tests::TEST_RISK;
    let mut r = Rig::granting("live-exit-reprice", LiveStrategies::StrategyC, |now| {
        LiveGrant {
            risk: Some(GrantRisk {
                max_gas_price_wei: 55_000_000,
                ..TEST_RISK
            }),
            ..rig_grant(now + 3_600_000, LiveStrategies::StrategyC)
        }
    });
    assert_eq!(r.exec.exec.gas_price_wei, 50_000_000);
    r.exec.exec.live_exit_reprice_bps = 1_000;
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let wait = Duration::from_secs(2);
    let a = [0xA1; 20];
    let signed = |raw: &[u8]| {
        let tx = darkbot_bsc_trading::transactions::legacy::decode_signed(raw)
            .unwrap()
            .tx;
        (tx.nonce, tx.gas_price)
    };
    let empty = |x: &mut LiveRuntime, c2: &mut StateOwner, n: u64, h: u8| {
        feed(
            x,
            c2,
            vec![block_of(n, h, h - 1, Vec::new()), complete_of(n, h)],
        );
        settle(x, c2);
    };
    let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
    let a_tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(a_tx, 0)]),
            sc.trade(a_tx, HEAD + 1, 0xb2, a),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    // Every lot's exit reserved at the grant's maximum: the open one's,
    // and the BUY's own in its admission (review LOW 3).
    let e = &r.exec.exec;
    let at_max = U256::from(55_000_000u64) * U256::from(e.approve_gas_limit + e.sell_gas_limit);
    assert_eq!(x.exit_reserve(), at_max);
    {
        use darkbot_bsc_record::schema::bsc_runtime::live_spend as ls;
        let entry = r
            .state
            .join("live-spend")
            .join(format!("{}.dbrec", alloy_primitives::hex::encode(a_tx)));
        let entry = darkbot_bsc_record::decode(&ls::SCHEMA, &fs::read(entry).unwrap()).unwrap();
        assert_eq!(
            U256::from_be_bytes(entry.u256(ls::EXIT_RESERVE_WEI).unwrap()),
            at_max
        );
    }
    // The approval P1 at nonce 1, the arm's price, is lost.
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(signed(&out[0].raw), (1, 50_000_000));
    let p1 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    for (n, h) in [(HEAD + 2, 0xb3), (HEAD + 3, 0xb4), (HEAD + 4, 0xb5)] {
        empty(&mut x, &mut c2, n, h);
    }
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    // P2: nonce 1 again at 10 % more, another transaction.
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Approve);
    assert_eq!(signed(&out[0].raw), (1, 55_000_000));
    assert_ne!(out[0].handoff.tx_hash, p1);
    let p2 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    // P2 lands: the SELL S1 at nonce 2, the arm's price, is lost.
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 5, 0xb6, 0xb5, vec![sc.receipt(p2, 1)]),
            complete_of(HEAD + 5, 0xb6),
        ],
    );
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    assert_eq!(signed(&out[0].raw), (2, 50_000_000));
    r.requests.recv_timeout(wait).unwrap();
    for (n, h) in [(HEAD + 6, 0xb7), (HEAD + 7, 0xb8), (HEAD + 8, 0xb9)] {
        empty(&mut x, &mut c2, n, h);
    }
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    // S2: nonce 2 again at 10 % more (its SELL's own gas limit).
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    assert_eq!(signed(&out[0].raw), (2, 55_000_000));
    r.requests.recv_timeout(wait).unwrap();
    // S2 lost too: at the grant's maximum, nothing higher; the arm's
    // price is S1's bytes, refused.
    for (n, h) in [(HEAD + 9, 0xba), (HEAD + 10, 0xbb), (HEAD + 11, 0xbc)] {
        empty(&mut x, &mut c2, n, h);
    }
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Owner)));
    assert_eq!(
        x.executor.as_ref().unwrap().end().last_refusal(),
        Some(crate::sources::LIVE_INTENT_SEEN)
    );
    x.shutdown();
    r.clean();
}

/// R5 (BSC3-030): the DEX exit of a lot whose token migrated: OFF,
/// nothing. ON, through the runtime's quotes (`exit_quoted_lots`): a
/// stale, overtaken or failed quote takes nothing (a failed one waits
/// for the backoff); a fresh one hands over the approval. Lost, it goes
/// again after its window (re-priced); included, the Portal SELL of
/// exactly the quoted tokens with the quote minus the exit slippage as
/// its minimum output.
#[test]
fn a_migrated_lot_exits_at_the_portals_quote() {
    for on in [false, true] {
        dex_exit(on);
    }
}

fn dex_exit(on: bool) {
    use crate::paper_exec::IntentKind;
    use crate::runtime::{
        Canonical, QuoteRead, RuntimeConfig, exit_quoted_lots, journal_max_payload,
    };
    use crate::runtime_journal::{JournalSetup, RuntimeJournal};
    use crate::sources::GrantRisk;
    use crate::sources::tests::TEST_RISK;
    let mut r = Rig::granting(
        &format!("live-dex-exit-{on}"),
        LiveStrategies::StrategyC,
        |now| LiveGrant {
            risk: Some(GrantRisk {
                max_gas_price_wei: 55_000_000,
                ..TEST_RISK
            }),
            ..rig_grant(now + 3_600_000, LiveStrategies::StrategyC)
        },
    );
    r.exec.exec.live_dex_exit = on;
    r.exec.exec.live_exit_reprice_bps = 1_000;
    let sc = Sc::new(r.buyer);
    let mut live = Some(r.runtime("boot-1"));
    let mut owner = Some(c2_at(&r.d, r.buyer, 0));
    settle(live.as_mut().unwrap(), owner.as_ref().unwrap());
    assert!(
        live.as_ref().unwrap().armed(),
        "{:?}",
        reason(live.as_ref().unwrap())
    );
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let (wait, silent) = (Duration::from_secs(2), Duration::from_millis(300));
    let a = [0xA1; 20];
    let (out, _) = live
        .as_mut()
        .unwrap()
        .on_strategy(&sc.buy(a), owner.as_ref().unwrap(), &clock, now_ms)
        .unwrap();
    let a_tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        live.as_mut().unwrap(),
        owner.as_mut().unwrap(),
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(a_tx, 0)]),
            sc.trade(a_tx, HEAD + 1, 0xb2, a),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    let quoted = U256::from(5_000_000_000_000_000u64);
    if !on {
        let x = live.as_mut().unwrap();
        reconnect(x);
        assert!(!x.dex_exit_due(&a, HEAD + 1));
        assert!(
            x.on_dex_exit(
                a,
                sc.tokens,
                quoted,
                owner.as_ref().unwrap(),
                &clock,
                now_ms
            )
            .is_none()
        );
        live.take().unwrap().shutdown();
        r.clean();
        return;
    }
    let config =
        RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env")).unwrap();
    let mut c = Canonical::new(&config.canonical.clone().unwrap(), 0);
    let base = std::env::temp_dir().join(format!("bsc-dex-exit-journal-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let amount = live.as_ref().unwrap().dex_exit_amount(&a).unwrap();
    assert!(amount > U256::ZERO && amount <= sc.tokens);
    let quote = |block: u64, answer: Result<u64, &str>| {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        tx.send(
            answer
                .map(|o| crate::venue_probe::DexQuote {
                    block_number: block,
                    block_hash: [block as u8; 32],
                    output: U256::from(o),
                })
                .map_err(str::to_string),
        )
        .unwrap();
        QuoteRead {
            token: a,
            amount,
            block,
            hash: [block as u8; 32],
            rx,
            overtaken: false,
            handle: None,
        }
    };
    let pass =
        |c: &mut Canonical, live: &mut Option<LiveRuntime>, journal: &mut RuntimeJournal, block| {
            exit_quoted_lots(c, live, &owner, &clock, journal, block, now_ms)
        };
    reconnect(live.as_mut().unwrap());
    // Stale, overtaken: nothing; failed: counted, waits for the backoff.
    c.lot.quotes = vec![quote(HEAD - 5, Ok(5_000_000_000_000_000))];
    pass(&mut c, &mut live, &mut journal, HEAD + 1);
    c.lot.quotes = vec![quote(HEAD + 1, Ok(5_000_000_000_000_000))];
    c.lot.overtake(&CanonicalEvent::Gap {
        from: HEAD,
        to: HEAD + 1,
    });
    pass(&mut c, &mut live, &mut journal, HEAD + 1);
    c.lot.quotes = vec![quote(HEAD + 1, Err("execution reverted"))];
    pass(&mut c, &mut live, &mut journal, HEAD + 1);
    assert!(r.requests.recv_timeout(silent).is_err());
    assert!(c.lot.quotes.is_empty() && c.lot.retry_at.iter().any(|(t, _)| *t == a));
    assert!(
        c.lot
            .fault
            .as_deref()
            .is_some_and(|f| f.starts_with("DEX quote"))
    );
    // A fresh quote: the approval at nonce 1, the arm's price.
    c.lot.quotes = vec![quote(HEAD + 1, Ok(5_000_000_000_000_000))];
    pass(&mut c, &mut live, &mut journal, HEAD + 1);
    r.requests.recv_timeout(wait).unwrap();
    let signed = |raw: &[u8]| {
        let tx = darkbot_bsc_trading::transactions::legacy::decode_signed(raw)
            .unwrap()
            .tx;
        (tx.nonce, tx.gas_price)
    };
    let x = live.as_mut().unwrap();
    assert!(!x.dex_exit_due(&a, HEAD + 1), "its approval is out");
    // Lost: after its window it goes again, re-priced.
    for (n, h) in [(HEAD + 2, 0xb3), (HEAD + 3, 0xb4), (HEAD + 4, 0xb5)] {
        feed(
            x,
            owner.as_mut().unwrap(),
            vec![block_of(n, h, h - 1, Vec::new()), complete_of(n, h)],
        );
        settle(x, owner.as_ref().unwrap());
    }
    assert!(x.armed(), "{:?}", reason(x));
    assert!(x.dex_exit_due(&a, HEAD + 4));
    reconnect(x);
    let (out, refused) = x
        .on_dex_exit(a, amount, quoted, owner.as_ref().unwrap(), &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Approve);
    assert_eq!(signed(&out[0].raw), (1, 55_000_000));
    let approval = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        x,
        owner.as_mut().unwrap(),
        vec![
            block_of(HEAD + 5, 0xb6, 0xb5, vec![sc.receipt(approval, 1)]),
            complete_of(HEAD + 5, 0xb6),
        ],
    );
    settle(x, owner.as_ref().unwrap());
    assert!(x.armed(), "{:?}", reason(x));
    reconnect(x);
    // Another amount than the quoted one: refused (its minimum is not
    // that amount's).
    let (out, refused) = x
        .on_dex_exit(
            a,
            amount + U256::from(1u8),
            quoted,
            owner.as_ref().unwrap(),
            &clock,
            now_ms,
        )
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Admission)));
    let amount = x.dex_exit_amount(&a).unwrap();
    let (out, refused) = x
        .on_dex_exit(a, amount, quoted, owner.as_ref().unwrap(), &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    let tx = darkbot_bsc_trading::transactions::legacy::decode_signed(&out[0].raw)
        .unwrap()
        .tx;
    assert_eq!(tx.nonce, 2);
    assert_eq!(&tx.input[4 + 32 + 12..4 + 64], a.as_slice());
    assert_eq!(
        U256::from_be_slice(&tx.input[4 + 3 * 32..4 + 4 * 32]),
        amount
    );
    assert_eq!(
        U256::from_be_slice(&tx.input[4 + 4 * 32..4 + 5 * 32]),
        darkbot_bsc_trading::economics::slippage::min_output(quoted, r.exec.exec.exit_slippage_bps)
            .unwrap()
    );
    live.take().unwrap().shutdown();
    let _ = fs::remove_dir_all(base);
    r.clean();
}

/// R5 (BSC3-030): with the public exit route ON (on the loopback route
/// the stand-in), an exit step handed to the builders also goes as
/// `eth_sendRawTransaction` of its own bytes, counted; a BUY never; OFF:
/// nothing public.
#[test]
fn the_public_exit_route_takes_exit_steps_only() {
    for on in [true, false] {
        public_exit_route(on);
    }
}

fn public_exit_route(on: bool) {
    let mut r = Rig::new(&format!("live-public-exit-{on}"), LiveStrategies::StrategyC);
    r.exec.exec.live_public_exit = on;
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let a = [0xA1; 20];
    // Every request the stand-in reads within `d`, and the public ones.
    let public = |d: Duration| {
        let mut all = Vec::new();
        while let Ok(req) = r.requests.recv_timeout(d) {
            all.push(String::from_utf8_lossy(&req).to_string());
        }
        let public: Vec<String> = all
            .iter()
            .filter(|q| q.contains("eth_sendRawTransaction"))
            .cloned()
            .collect();
        (all.len(), public)
    };
    let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
    let a_tx = out[0].handoff.tx_hash;
    let (requests, publics) = public(Duration::from_millis(500));
    assert!(
        requests > 0 && publics.is_empty(),
        "a BUY never goes public"
    );
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(a_tx, 0)]),
            sc.trade(a_tx, HEAD + 1, 0xb2, a),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    let raw = alloy_primitives::hex::encode(&out[0].raw);
    let (requests, publics) = public(Duration::from_millis(500));
    assert!(requests > 0);
    let sent = record(&x)
        .texts(l::RESPONSES)
        .unwrap()
        .contains(&"PUBLIC_EXIT_SENT=1");
    match on {
        true => {
            assert_eq!(publics.len(), 1, "{publics:?}");
            assert!(publics[0].contains(&format!(r#""params":["0x{raw}"]"#)));
            assert!(sent);
        }
        false => assert!(publics.is_empty() && !sent),
    }
    x.shutdown();
    r.clean();
}

/// BSC3-048 (review HIGH/MEDIUM): an exit step whose bundle is lost.
/// While it is out and after its window, no BUY takes its nonce; the
/// exit stays wanted. The same signed approval is never handed over a
/// second time (no retry: the owner refuses it). It lands after all:
/// the SELL follows without another approval. The SELL S1 is lost:
/// after its window, on a new arm that knows the included approval,
/// the same SELL is refused again, a SELL on the curve as it is now
/// (another transaction) goes at S1's nonce; it closes the position and
/// S1 is REPLACED by it.
#[test]
fn a_lost_exit_step_goes_again_at_its_nonce_before_any_buy() {
    use crate::chain_follower::CanonicalTrade;
    use crate::live_ledger::{FactKind, PositionState};
    use crate::paper_exec::{IntentKind, Refusal};
    let r = Rig::new("live-exit-retry", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let wait = Duration::from_secs(2);
    let silent = Duration::from_millis(300);
    let (a, c) = ([0xA1; 20], [0xC3; 20]);
    let nonce = |raw: &[u8]| {
        darkbot_bsc_trading::transactions::legacy::decode_signed(raw)
            .unwrap()
            .tx
            .nonce
    };
    let empty = |x: &mut LiveRuntime, c2: &mut StateOwner, n: u64, h: u8| {
        feed(
            x,
            c2,
            vec![block_of(n, h, h - 1, Vec::new()), complete_of(n, h)],
        );
        settle(x, c2);
    };
    // A: bought at nonce 0, included at HEAD + 1.
    let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
    let a_tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(a_tx, 0)]),
            sc.trade(a_tx, HEAD + 1, 0xb2, a),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    // The exit's approval P1 at nonce 1 (window to HEAD + 3) is lost.
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    let p1 = out[0].handoff.tx_hash;
    assert_eq!(nonce(&out[0].raw), 1);
    r.requests.recv_timeout(wait).unwrap();
    // While an exit step is out, no BUY goes.
    let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
    empty(&mut x, &mut c2, HEAD + 2, 0xb3);
    empty(&mut x, &mut c2, HEAD + 3, 0xb4);
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    // P1's window is over: a new arm; the exit is wanted, no BUY goes;
    // the same approval is not handed over again.
    empty(&mut x, &mut c2, HEAD + 4, 0xb5);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Owner)));
    assert_eq!(
        x.executor.as_ref().unwrap().end().last_refusal(),
        Some(crate::sources::LIVE_INTENT_SEEN)
    );
    assert!(r.requests.recv_timeout(silent).is_err());
    // P1 lands after all: the SELL follows at nonce 2, no approval.
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 5, 0xb6, 0xb5, vec![sc.receipt(p1, 1)]),
            complete_of(HEAD + 5, 0xb6),
        ],
    );
    assert_eq!(x.book().tx(&p1).unwrap().state, FactKind::Included);
    // The next pass: this arm never planned nonce 1 taken; a new arm
    // takes nonce 2 and knows the included approval.
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(2));
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!((out[0].kind, nonce(&out[0].raw)), (IntentKind::Sell, 2));
    let s1 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    // S1 (window to HEAD + 7) is lost; no BUY meanwhile.
    empty(&mut x, &mut c2, HEAD + 6, 0xb7);
    empty(&mut x, &mut c2, HEAD + 7, 0xb8);
    assert!(r.requests.recv_timeout(silent).is_err());
    empty(&mut x, &mut c2, HEAD + 8, 0xb9);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.exits_to_continue(&c2), vec![a]);
    let (out, refused) = x.on_strategy(&sc.buy(c), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::ExitFirst)));
    // The same SELL again: refused; on the curve as it is now (a later
    // trade moved it), another SELL goes at nonce 2, no approval.
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Owner)));
    let moved = crate::runtime_strategy::StrategyAction::Exit {
        token: a,
        tokens: sc.tokens,
        curve: crate::paper_exec::StrategyCurve {
            reserve: sc.held.reserve + U256::from(1_000_000_000_000_000u64),
            ..sc.held
        },
        decided_ns: 0,
    };
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&moved, &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!((out[0].kind, nonce(&out[0].raw)), (IntentKind::Sell, 2));
    let s2 = out[0].handoff.tx_hash;
    assert_ne!(s2, s1);
    r.requests.recv_timeout(wait).unwrap();
    // S2 included: the position closes, S1 is REPLACED by S2.
    let changes = feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 9, 0xba, 0xb9, vec![sc.receipt(s2, 2)]),
            CanonicalEvent::FlapTrade(CanonicalTrade {
                block_number: HEAD + 9,
                block_hash: [0xba; 32],
                tx_index: 0,
                tx_hash: s2,
                log_index: 0,
                origin: r.buyer,
                sell: true,
                token: a,
                trader: r.buyer,
                amount: sc.tokens.to_be_bytes(),
                eth: sc.input.to_be_bytes(),
                fee: [0; 32],
                post_price: [0; 32],
            }),
            complete_of(HEAD + 9, 0xba),
        ],
    );
    assert_eq!(changes, vec![PositionChange::Ended { token: a }]);
    assert_eq!(x.book().positions[0].state, PositionState::Closed);
    let replaced = x.book().tx(&s1).unwrap();
    assert_eq!(replaced.state, FactKind::Replaced);
    assert_eq!(replaced.inclusion.as_ref().unwrap().by_tx, Some(s2));
    assert_eq!(x.exits_to_continue(&c2), Vec::<[u8; 20]>::new());
    x.shutdown();
    r.clean();
}

/// BSC3-048 (review MEDIUM): a transaction whose block left the chain
/// was public in that block and may be included at any height: its
/// window bounds nothing, its nonce stays locked until it is taken.
#[test]
fn an_inclusion_that_left_the_chain_has_no_window() {
    let r = Rig::new("live-orphan-window", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .unwrap();
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let tx = CanonicalTx {
        hash: buy.handoff.tx_hash,
        index: 0,
        sender: r.buyer,
        nonce: 5,
        succeeded: true,
        cost: None,
    };
    include(&mut x, &mut c2, vec![tx]);
    feed(
        &mut x,
        &mut c2,
        vec![CanonicalEvent::Reorg {
            orphaned: vec![(HEAD + 1, [0xb2; 32])],
            new_head: (HEAD + 1, [0xc2; 32]),
        }],
    );
    let mut parent = 0xb1;
    for k in 1..=6u8 {
        let h = 0xc1 + k;
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + u64::from(k), h, parent, Vec::new()),
                complete_of(HEAD + u64::from(k), h),
            ],
        );
        parent = h;
    }
    let t = x.book().tx(&buy.handoff.tx_hash).unwrap();
    assert!(t.orphaned && !t.window_over(HEAD + 6));
    let why = x.nonce_free(5, Some(HEAD + 6)).unwrap_err();
    assert!(
        why.contains("public since its block left the chain"),
        "{why}"
    );
    assert_eq!(x.free_nonce(&c2), None);
    x.shutdown();
    r.clean();
}

/// BSC3-071: the day's BUYs are used up with positions open. The gate
/// refuses BUYs, not the exits under the same grant: the path stays
/// armed, a new BUY is refused (BUYS_CLOSED) before anything is signed,
/// an exit goes. A revoked grant still ends everything.
#[test]
fn used_up_buys_close_buys_not_the_exits_of_open_positions() {
    use crate::paper_exec::{IntentKind, Refusal};
    let r = Rig::new("live-buys-closed", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let wait = Duration::from_secs(2);
    let (a, b) = ([0xA1; 20], [0xB2; 20]);
    // Two BUYs (the day's limit), both included.
    let mut parent = 0xb1;
    for (k, token) in [(1u8, a), (2u8, b)] {
        let (out, _) = x.on_strategy(&sc.buy(token), &c2, &clock, now_ms).unwrap();
        assert_eq!(out.len(), 1, "{:?}", reason(&x));
        let tx = out[0].handoff.tx_hash;
        r.requests.recv_timeout(wait).unwrap();
        let (n, h) = (HEAD + u64::from(k), 0xb1 + k);
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(n, h, parent, vec![sc.receipt(tx, u64::from(k - 1))]),
                sc.trade(tx, n, h, token),
                complete_of(n, h),
            ],
        );
        parent = h;
        reconnect(&mut x);
    }
    // The gate refuses BUYs now; the path stays armed for the exits.
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (out, refused) = x
        .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Owner)));
    assert!(
        record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"BUYS_CLOSED=1")
    );
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Approve);
    r.requests.recv_timeout(wait).unwrap();
    // Revoked: everything ends.
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    assert!(
        s.request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
            .unwrap()
            .applied()
    );
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(
        reason(&x),
        ("ENDED".to_string(), Some(LIVE_GRANT_MISSING.to_string()))
    );
    r.clean();
}

/// BSC3-048: a restart inside B's window. The window is the admission
/// entry's: the recovered fact knows it, nothing arms while it runs, and
/// after it the arm takes nonce 1 again.
#[test]
fn a_restart_inside_the_window_waits_for_it() {
    let r = Rig::new("live-window-restart", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let (out, _) = x
        .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!(out.len(), 1);
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    // Killed before anything else; the outcome's fact is lost too.
    std::mem::forget(x);
    for e in fs::read_dir(r.state.join("live-ledger")).unwrap() {
        fs::remove_file(e.unwrap().path()).unwrap();
    }
    r.authorize("boot-2", "after-restart");
    let mut x = r.runtime("boot-2");
    let mut c2 = c2_at(&r.d.join("c2-restarted"), r.buyer, 0);
    // HEAD + 1, HEAD + 2: the window (HEAD + 2) runs: no arm.
    for (n, h, p) in [(HEAD + 1, 0xb2, 0xb1), (HEAD + 2, 0xb3, 0xb2)] {
        feed(
            &mut x,
            &mut c2,
            vec![block_of(n, h, p, Vec::new()), complete_of(n, h)],
        );
        settle(&mut x, &c2);
        assert!(!x.armed());
        assert!(
            reason(&x)
                .1
                .unwrap()
                .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
        );
    }
    // HEAD + 3: over; the arm takes nonce 0 again.
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 3, 0xb4, 0xb3, Vec::new()),
            complete_of(HEAD + 3, 0xb4),
        ],
    );
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(record(&x).opt_u64(l::NEXT_NONCE).unwrap(), Some(0));
    // The BUY stays unresolved in the ledger until its nonce is taken.
    assert_eq!(x.book().unresolved().count(), 1);
    x.shutdown();
    r.clean();
}

/// BSC3-048: a transaction whose window is not known (an entry from
/// before the window was written) keeps the arm blocked, however many
/// blocks pass.
#[test]
fn an_unknown_window_keeps_the_arm_blocked() {
    let r = Rig::new("live-window-unknown", LiveStrategies::C3);
    crate::live_ledger::Ledger::open(&r.state)
        .append(
            &crate::live_ledger::Fact {
                kind: crate::live_ledger::FactKind::Released,
                tx_hash: [0x77; 32],
                intent: crate::sources::LiveIntentKind::Buy,
                component: ComponentId::ExecC3,
                token: [0x55; 20],
                nonce: 5,
                grant_generation: 1,
                input_wei: Some(U256::from(1u64)),
                max_wei: Some(U256::from(2u64)),
                block: None,
                gas_wei: None,
                tokens: None,
                eth_wei: None,
                max_block: None,
                by_tx: None,
                response: None,
                origin: None,
                launcher: None,
                placement: None,
                attempt: None,
                operator: None,
                source: None,
            },
            1,
        )
        .unwrap();
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    let mut parent = 0xb1;
    for k in 1..=8u8 {
        let h = 0xb1 + k;
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + u64::from(k), h, parent, Vec::new()),
                complete_of(HEAD + u64::from(k), h),
            ],
        );
        parent = h;
    }
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert!(
        reason(&x)
            .1
            .unwrap()
            .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
    );
    r.clean();
}

/// REV.1.0 R3b: the operator's nonce burn of an unresolved transaction
/// whose window is unknown (it keeps the arm blocked). Refused while
/// `BSC_EXEC_LIVE_NONCE_BURN` is OFF; a request written is held back,
/// counted, by a runtime with the switch OFF. ON: the request lets the path arm at that nonce, no BUY
/// goes first, the burn goes as the buyer's plain transfer of nothing
/// to itself at that nonce (the owner admits it, the stand-in builder
/// receives it), once while its window runs. Included, its transaction
/// is REPLACED by it, the burn costs its gas (no lot), and no burn is
/// wanted any more.
#[test]
fn a_nonce_burn_takes_the_nonce_of_a_transaction_that_may_still_land() {
    use crate::live_ledger::LIVE_NONCE_BURN_OFF;
    use darkbot_bsc_trading::transactions::legacy::decode_signed;
    use darkbot_bsc_trading::venues::flap::binding::TRANSFER_GAS;
    const TARGET: [u8; 32] = [0x77; 32];
    let mut r = Rig::new("live-burn", LiveStrategies::C3);
    let ledger = Ledger::open(&r.state);
    let target = Fact {
        kind: FactKind::Released,
        tx_hash: TARGET,
        intent: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        token: [0x55; 20],
        nonce: 5,
        grant_generation: 1,
        input_wei: Some(U256::from(1u64)),
        max_wei: Some(U256::from(2u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: None,
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    ledger.append(&target, 1).unwrap();
    assert_eq!(
        ledger.request_burn(
            TARGET,
            Some(r.buyer),
            "burn-1",
            "operator",
            false,
            ledger.version().unwrap(),
            NOW
        ),
        Err(LIVE_NONCE_BURN_OFF.into())
    );
    let written = ledger
        .request_burn(
            TARGET,
            Some(r.buyer),
            "burn-1",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    assert!(written.is_some());
    assert_eq!(
        ledger.request_burn(
            TARGET,
            Some(r.buyer),
            "burn-1",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW
        ),
        Ok(None),
        "once per request: written before"
    );
    // A runtime with the switch OFF holds it back.
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert!(refused_by(&x, "NONCE_BURN_LOCKED"));
    x.shutdown();
    // ON: the path arms at nonce 5; nothing goes before the burn.
    r.exec.exec.live_nonce_burn = true;
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d.join("c2-burn"), r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(
        x.buy_refusal(U256::ZERO, Some(&c2), Some(5)),
        Some((
            Refusal::ExitFirst,
            Some(crate::sources::LIVE_NONCE_BURN_FIRST)
        ))
    );
    while r.requests.try_recv().is_ok() {}
    let clock = a_clock();
    let intents = x.burn(&c2, &clock, DECIDED_MS);
    assert_eq!(intents.len(), 1, "{:?}", reason(&x));
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    let signed = decode_signed(&intents[0].raw).unwrap();
    assert_eq!(
        (signed.tx.nonce, signed.tx.to, signed.tx.gas_limit),
        (5, Address::from(r.buyer), TRANSFER_GAS)
    );
    assert!(signed.tx.value.is_zero() && signed.tx.input.is_empty());
    let burn = intents[0].handoff.tx_hash;
    let released = x.book().tx(&burn).unwrap();
    assert_eq!(
        (released.released.intent, released.state),
        (LiveIntentKind::Burn, FactKind::Released)
    );
    // Its most: its transfer gas at the signed price (what tells a burn
    // of this price spent).
    assert_eq!(
        released.released.max_wei,
        Some(U256::from(r.exec.exec.gas_price_wei) * U256::from(TRANSFER_GAS))
    );
    // Review MEDIUM-1: its journaled release reads back (kind 4).
    let i = &intents[0];
    let record = crate::runtime_journal::live_release_payload(
        i.kind.code(),
        None,
        &i.handoff,
        [0; 32],
        i.token,
        i.block,
        &i.times,
        &i.raw,
    );
    let read = crate::runtime_journal::live_release_of(&record).unwrap();
    assert_eq!((read.kind, read.tx_hash), (4, burn));
    // The owner admitted it as a burn (its spend entry reads).
    let admitted = crate::sources::admitted_intents(&r.state).unwrap();
    assert!(
        admitted
            .iter()
            .any(|a| a.tx_hash == burn && a.kind == LiveIntentKind::Burn),
        "{admitted:?}"
    );
    // Once while its window runs.
    assert!(x.burn(&c2, &clock, DECIDED_MS).is_empty());
    // Included: its transaction is replaced by it; the burn costs its
    // gas and holds no lot.
    let gas = crate::chain_follower::TxCost {
        gas_used: TRANSFER_GAS,
        effective_gas_price: 1_000_000_000,
    };
    include(
        &mut x,
        &mut c2,
        vec![CanonicalTx {
            hash: burn,
            index: 0,
            sender: r.buyer,
            nonce: 5,
            succeeded: true,
            cost: Some(gas),
        }],
    );
    let book = x.book();
    assert_eq!(book.tx(&TARGET).unwrap().state, FactKind::Replaced);
    assert_eq!(
        book.tx(&TARGET).unwrap().inclusion.as_ref().unwrap().by_tx,
        Some(burn)
    );
    assert_eq!(book.tx(&burn).unwrap().state, FactKind::Included);
    assert!(
        book.positions
            .iter()
            .all(|p| p.lot != burn && p.token != [0; 20])
    );
    assert_eq!(book.gas_wei, U256::from(gas.wei()));
    assert_eq!(book.realized_pnl_wei, -(gas.wei() as i128));
    settle(&mut x, &c2);
    assert!(x.burn_target().is_none());
    assert!(x.burn(&c2, &clock, DECIDED_MS).is_empty());
    // The path goes on at the next nonce: nothing unresolved holds it.
    assert!(x.armed(), "{:?}", reason(&x));
    assert!(x.book().unresolved().next().is_none());
    x.shutdown();
    r.clean();
}

/// REV.1.0 R4 (PHASENPLAN §20): with every LIVE source public and
/// `BSC_EXEC_LIVE_PUBLIC_ONLY` OFF, a sighting triggers no C3 BUY
/// (counted SOURCE_PUBLIC_ONLY) and the arm holds no lease for one;
/// ON, the same sighting is its signal.
#[test]
fn a_public_source_alone_triggers_no_c3_buy() {
    let mut r = Rig::new("live-public-only", LiveStrategies::C3);
    r.exec.exec.live_public_only = false;
    let leased = |x: &LiveRuntime| x.executor.as_ref().unwrap().end().lease().is_some();
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert!(!leased(&x), "no lease for a BUY that may not come");
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(
            SourceId::PublicNode,
            hash,
            [0; 20],
            &raw,
            10,
            &c2,
            &a_clock()
        )
        .is_none()
    );
    assert!(refused_by(&x, "SOURCE_PUBLIC_ONLY"), "{:?}", reason(&x));
    x.shutdown();
    r.exec.exec.live_public_only = true;
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(leased(&x));
    assert!(
        x.on_launch(
            SourceId::PublicNode,
            hash,
            [0; 20],
            &raw,
            10,
            &c2,
            &a_clock()
        )
        .is_some()
    );
    x.shutdown();
    r.clean();
}

/// REV.1.0 R6: with `BSC_EXEC_LIVE_PUBLIC_ONLY` OFF a non-public LIVE
/// source unlocks public sightings only while it is healthy and
/// delivers full transactions (the runtime's last status pass; before
/// one: down): a sighting meanwhile triggers no C3 BUY, counted
/// SOURCE_UNHEALTHY; a public source's health unlocks nothing.
#[test]
fn a_non_public_source_unlocks_public_sightings_only_while_it_signals() {
    use crate::ingress::Health;
    use crate::sources::SourceCapability as C;
    let mut r = Rig::new("live-source-health", LiveStrategies::C3);
    r.exec.exec.live_public_only = false;
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    x.sources_live = vec![SourceId::Geth, SourceId::PublicNode];
    assert!(x.public_only_locked(), "no status pass yet");
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(
            SourceId::PublicNode,
            hash,
            [0; 20],
            &raw,
            10,
            &c2,
            &a_clock()
        )
        .is_none()
    );
    assert!(refused_by(&x, "SOURCE_UNHEALTHY"), "{:?}", reason(&x));
    assert!(!refused_by(&x, "SOURCE_PUBLIC_ONLY"));
    for (h, c) in [
        (Health::Degraded, C::Full),
        (Health::Disconnected, C::Full),
        (Health::RateLimited, C::Full),
        (Health::Healthy, C::HashOnly),
        (Health::Healthy, C::Unknown),
    ] {
        x.set_source_signals(vec![(SourceId::Geth, h, c)]);
        assert!(x.public_only_locked(), "{h:?} {c:?}");
    }
    x.set_source_signals(vec![(SourceId::Geth, Health::Healthy, C::Full)]);
    assert!(!x.public_only_locked());
    x.set_source_signals(vec![(SourceId::PublicNode, Health::Healthy, C::Full)]);
    assert!(x.public_only_locked());
    x.shutdown();
    r.clean();
}

/// REV.1.0 R4 (BSC3-059): a grant running longer than
/// `BSC_EXEC_LIVE_GRANT_MAX_DURATION_MS` admits no BUY (nothing to exit:
/// nothing arms, LIVE_GRANT_DURATION_ABOVE_MAX; a lot open: the path arms
/// for its exit only); within it the path arms.
#[test]
fn a_grant_beyond_the_longest_duration_admits_no_buy() {
    let mut r = Rig::new("live-grant-duration", LiveStrategies::C3);
    // The rig's grant runs an hour.
    r.exec.exec.live_grant_max_duration_ms = 3_599_000;
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!x.armed());
    let (_, why) = reason(&x);
    assert_eq!(
        why.as_deref(),
        Some(crate::sources::LIVE_GRANT_DURATION_ABOVE_MAX)
    );
    x.shutdown();
    // A lot open: the path arms for its exit, no BUY (review LOW).
    let buy = Fact {
        max_block: Some(HEAD),
        ..unknown_window([0x7c; 32], 3, ComponentId::ExecC3)
    };
    let ledger = Ledger::open(&r.state);
    ledger.append(&buy, 1).unwrap();
    ledger
        .append(
            &Fact {
                block: Some((HEAD - 1, [0x41; 32], NOW)),
                gas_wei: Some(U256::from(1u64)),
                tokens: Some(U256::from(7u64)),
                eth_wei: Some(U256::from(1u64)),
                ..Fact::about(&buy, FactKind::Included)
            },
            2,
        )
        .unwrap();
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(
        x.buys_closed.as_deref(),
        Some(crate::sources::LIVE_GRANT_DURATION_ABOVE_MAX)
    );
    assert!(x.buy_refusal(U256::ZERO, Some(&c2), Some(5)).is_some());
    x.shutdown();
    r.exec.exec.live_grant_max_duration_ms = 3_600_000;
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.buys_closed, None, "exactly the longest admits BUYs");
    x.shutdown();
    r.clean();
}

/// REV.1.0 R9 (BSC3-001): with a presence anchor, an unsigned grant (or
/// one signed by another key, or for another generation) admits no BUY
/// (nothing to exit: nothing arms, LIVE_PRESENCE_UNSIGNED); the grant
/// signed by the anchor's key for its generation arms.
#[test]
fn a_presence_anchor_admits_only_a_grant_its_key_signed() {
    use crate::sources::tests::{presence_operator, presence_signed};
    let mut r = Rig::new("live-presence", LiveStrategies::C3);
    r.exec.exec.live_presence_anchor = Some(presence_operator());
    let c2 = c2_at(&r.d, r.buyer, 5);
    let unsigned = |x: &LiveRuntime| {
        assert!(!x.armed());
        assert_eq!(
            reason(x).1.as_deref(),
            Some(crate::sources::LIVE_PRESENCE_UNSIGNED)
        );
    };
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    unsigned(&x);
    x.shutdown();
    let store = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let state = store.current().unwrap();
    let grant = state.live.clone().unwrap();
    let signature = presence_signed(&grant, state.generation + 1);
    store
        .grant_live_present(
            "g-presence",
            grant.clone(),
            grant.scope_sha256(),
            Some(signature),
            state.generation,
            NOW,
        )
        .unwrap();
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    x.shutdown();
    // Another key's anchor: the same signature is no presence.
    r.exec.exec.live_presence_anchor = Some([0x77; 20]);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    unsigned(&x);
    x.shutdown();
    // Review M1: the old signature on the same grant written again
    // (after a revocation) is no presence for its new generation.
    r.exec.exec.live_presence_anchor = Some(presence_operator());
    let revoked = store
        .request(
            "r-revoke",
            Entity::LiveGrant,
            Mode::Disabled,
            state.generation + 1,
            NOW,
        )
        .unwrap();
    store
        .grant_live_present(
            "g-replayed",
            grant.clone(),
            grant.scope_sha256(),
            Some(signature),
            revoked.generation,
            NOW,
        )
        .unwrap();
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    unsigned(&x);
    x.shutdown();
    r.clean();
}

/// REV.1.0 R8 (BSC3-049): a qualification older than
/// `BSC_EXEC_LIVE_QUALIFICATION_MAX_AGE_MS` admits no BUY (the rig's
/// is made 1 ms after the epoch: nothing to exit, nothing arms); no
/// bound (0) arms.
#[test]
fn a_qualification_older_than_its_bound_admits_no_buy() {
    let mut r = Rig::new("live-qualification-age", LiveStrategies::C3);
    r.exec.exec.live_qualification_max_age_ms = 31_536_000_000;
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(
        reason(&x).1.as_deref(),
        Some(crate::sources::LIVE_QUALIFICATION_STALE)
    );
    x.shutdown();
    r.exec.exec.live_qualification_max_age_ms = 0;
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    x.shutdown();
    r.clean();
}

/// REV.1.0 R4: the campaign's budget (`BSC_EXEC_LIVE_SPEND_TOTAL_WEI`):
/// no room for one BUY closes BUYs at the gate (nothing open: nothing
/// arms); room for a BUY's most but not its exits: the runtime's owner
/// store refuses its lease (review M1: the wiring itself).
#[test]
fn the_campaign_budget_closes_buys_and_bounds_the_lease() {
    let mut r = Rig::new("live-campaign-budget", LiveStrategies::C3);
    r.exec.exec.live_spend_total_wei = 0;
    let leased = |x: &LiveRuntime| x.executor.as_ref().unwrap().end().lease().is_some();
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(
        reason(&x).1.as_deref(),
        Some(crate::sources::LIVE_CAMPAIGN_SPEND_EXHAUSTED)
    );
    x.shutdown();
    let e = &r.exec.exec;
    r.exec.exec.live_spend_total_wei =
        e.input_wei + u128::from(e.gas_price_wei) * u128::from(e.gas_limit);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert!(!leased(&x));
    assert!(
        x.lease_error
            .as_deref()
            .is_some_and(|e| e.contains(crate::sources::LIVE_CAMPAIGN_SPEND_EXHAUSTED)),
        "{:?}",
        x.lease_error
    );
    x.shutdown();
    r.clean();
}

/// REV.1.0 R4 (BSC3-069): a grant whose risk is above the runtime's soft
/// bounds is an outlier: unconfirmed in its scope it admits no BUY
/// (nothing open: nothing arms, LIVE_RISK_OUTLIER_UNACKED); confirmed,
/// only with BSC_EXEC_LIVE_RISK_OUTLIER ON (LIVE_RISK_OUTLIER_LOCKED).
#[test]
fn an_outlier_risk_arms_only_confirmed_and_unlocked() {
    use crate::sources::tests::TEST_RISK;
    use crate::sources::{GrantRisk, LIVE_RISK_OUTLIER_LOCKED, LIVE_RISK_OUTLIER_UNACKED};
    let mut r = Rig::new("live-outlier", LiveStrategies::C3);
    r.exec.exec.live_risk_soft_buy_slippage_bps = 100;
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(reason(&x).1.as_deref(), Some(LIVE_RISK_OUTLIER_UNACKED));
    x.shutdown();
    r.clean();
    let mut r = Rig::granting("live-outlier-ack", LiveStrategies::C3, |now| LiveGrant {
        risk: Some(GrantRisk {
            outlier_ack: true,
            ..TEST_RISK
        }),
        ..rig_grant(now + 3_600_000, LiveStrategies::C3)
    });
    r.exec.exec.live_risk_soft_buy_slippage_bps = 100;
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(reason(&x).1.as_deref(), Some(LIVE_RISK_OUTLIER_LOCKED));
    x.shutdown();
    r.exec.exec.live_risk_outlier = true;
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    x.shutdown();
    r.clean();
}

/// R4 review M1: each soft bound makes an outlier on its own; equal is
/// none; an exit-only grant (no BUY) none; an ack without an outlier
/// changes nothing; an acked outlier only with the switch ON.
#[test]
fn every_soft_bound_makes_an_outlier_on_its_own() {
    use crate::sources::tests::TEST_RISK;
    use crate::sources::{GrantRisk, LIVE_RISK_OUTLIER_LOCKED, LIVE_RISK_OUTLIER_UNACKED};
    let r = Rig::new("live-soft-bounds", LiveStrategies::C3);
    let mut x = r.exec.exec.clone();
    x.live_risk_soft_buy_slippage_bps = TEST_RISK.max_buy_slippage_bps;
    x.live_risk_soft_exit_slippage_bps = TEST_RISK.max_exit_slippage_bps;
    x.live_risk_soft_gas_price_wei = TEST_RISK.max_gas_price_wei;
    let grant = |risk: GrantRisk, exit_only: bool| LiveGrant {
        risk: Some(risk),
        exit_only,
        ..rig_grant(NOW + 3_600_000, LiveStrategies::C3)
    };
    let acked = GrantRisk {
        outlier_ack: true,
        ..TEST_RISK
    };
    let unacked = Some(LIVE_RISK_OUTLIER_UNACKED.to_string());
    assert_eq!(outlier_refusal(&grant(TEST_RISK, false), &x), None);
    assert_eq!(outlier_refusal(&grant(acked, false), &x), None);
    let below = |f: &dyn Fn(&mut crate::paper_exec::ExecConfig)| {
        let mut y = x.clone();
        f(&mut y);
        y
    };
    let buy = below(&|y| y.live_risk_soft_buy_slippage_bps -= 1);
    let exit = below(&|y| y.live_risk_soft_exit_slippage_bps -= 1);
    let gas = below(&|y| y.live_risk_soft_gas_price_wei -= 1);
    for y in [&buy, &exit, &gas] {
        assert_eq!(outlier_refusal(&grant(TEST_RISK, false), y), unacked);
    }
    assert_eq!(outlier_refusal(&grant(TEST_RISK, true), &buy), None);
    assert_eq!(outlier_refusal(&grant(TEST_RISK, true), &exit), None);
    assert_eq!(
        outlier_refusal(&grant(acked, false), &gas),
        Some(LIVE_RISK_OUTLIER_LOCKED.to_string())
    );
    let on = below(&|y| {
        y.live_risk_soft_gas_price_wei -= 1;
        y.live_risk_outlier = true;
    });
    assert_eq!(outlier_refusal(&grant(acked, false), &on), None);
    r.clean();
}

/// The tests' unresolved transaction at `nonce` whose window is unknown
/// (its bytes may land at any height).
fn unknown_window(hash: [u8; 32], nonce: u64, component: ComponentId) -> Fact {
    Fact {
        kind: FactKind::Released,
        tx_hash: hash,
        intent: LiveIntentKind::Buy,
        component,
        token: [0x55; 20],
        nonce,
        grant_generation: 1,
        input_wei: Some(U256::from(1u64)),
        max_wei: Some(U256::from(2u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: None,
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    }
}

/// R3b review MEDIUM-2: a burn whose window ended without its inclusion
/// is spent: its bytes are never handed over again (named
/// NONCE_BURN_SPENT, no admission tried); another goes only at another
/// gas price, and that one replaces the transaction.
#[test]
fn a_burn_that_never_landed_is_spent_until_another_gas_price() {
    const TARGET: [u8; 32] = [0x7a; 32];
    let mut r = Rig::new("live-burn-spent", LiveStrategies::C3);
    r.exec.exec.live_nonce_burn = true;
    let ledger = Ledger::open(&r.state);
    ledger
        .append(&unknown_window(TARGET, 5, ComponentId::ExecC3), 1)
        .unwrap();
    ledger
        .request_burn(
            TARGET,
            Some(r.buyer),
            "burn-1",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let first = x.burn(&c2, &clock, DECIDED_MS);
    assert_eq!(first.len(), 1);
    let first = first[0].handoff.tx_hash;
    let last = x.book().tx(&first).unwrap().released.max_block.unwrap();
    // Its window ends: no block includes it.
    let mut parent = 0xb1;
    for n in HEAD + 1..=last + 1 {
        let h = (n - HEAD) as u8 + 0xb1;
        feed(
            &mut x,
            &mut c2,
            vec![block_of(n, h, parent, Vec::new()), complete_of(n, h)],
        );
        parent = h;
    }
    settle(&mut x, &c2);
    assert!(x.burn(&c2, &clock, DECIDED_MS).is_empty());
    assert!(refused_by(&x, "NONCE_BURN_SPENT"), "{:?}", reason(&x));
    assert!(refused_by(&x, "NONCE_BURN_WANTED"));
    assert!(!refused_by(&x, "NONCE_BURN_REFUSED"), "no admission tried");
    x.shutdown();
    // Another gas price: another burn, other bytes.
    r.exec.exec.gas_price_wei -= 1;
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let again = x.burn(&c2, &clock, DECIDED_MS);
    assert_eq!(again.len(), 1, "{:?}", reason(&x));
    let second = again[0].handoff.tx_hash;
    assert_ne!(second, first);
    include(
        &mut x,
        &mut c2,
        vec![CanonicalTx {
            hash: second,
            index: 0,
            sender: r.buyer,
            nonce: 5,
            succeeded: true,
            cost: None,
        }],
    );
    let book = x.book();
    assert_eq!(book.tx(&TARGET).unwrap().state, FactKind::Replaced);
    assert_eq!(book.tx(&first).unwrap().state, FactKind::Replaced);
    x.shutdown();
    r.clean();
}

/// R3b review MEDIUM-3: a burn is admitted as an exit step is: under a
/// grant that admits no BUY (EXIT-ONLY) the path arms for it all the
/// same, and the burn goes.
#[test]
fn a_burn_arms_the_path_where_no_buy_may_go() {
    const TARGET: [u8; 32] = [0x7b; 32];
    let mut r = Rig::new("live-burn-exit-only", LiveStrategies::StrategyC);
    r.exec.exec.live_nonce_burn = true;
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let grant = LiveGrant {
        max_input_wei: 10_000_000_000_000_000,
        max_spend_wei: 100_000_000_000_000_000,
        adapters: vec![LiveAdapter::Club48BuilderV1],
        exit_only: true,
        ..grant_of(NOW + 3_600_000, LiveStrategies::StrategyC)
    };
    let g = s.current().unwrap().generation;
    assert!(
        s.grant_live("exit-only", grant.clone(), grant.scope_sha256(), g, NOW)
            .unwrap()
            .applied()
    );
    let ledger = Ledger::open(&r.state);
    ledger
        .append(&unknown_window(TARGET, 0, ComponentId::StrategyC), 1)
        .unwrap();
    let c2 = c2_at(&r.d, r.buyer, 0);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(!x.armed(), "no position, no burn: nothing to arm for");
    ledger
        .request_burn(
            TARGET,
            Some(r.buyer),
            "burn-1",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let burnt = x.burn(&c2, &a_clock(), DECIDED_MS);
    assert_eq!(burnt.len(), 1, "{:?}", reason(&x));
    assert_eq!(burnt[0].kind, crate::paper_exec::IntentKind::Burn);
    x.shutdown();
    r.clean();
}

/// R3b review LOW-2, LOW-3: a burn requested of another sender's
/// transaction (a fact written outside `request_burn`) is never the
/// buyer's to burn; a burn's gas is a realized loss of its grant (and
/// day) as any position's: the grant's loss bound holds BUYs back.
#[test]
fn a_burn_is_the_buyers_only_and_its_gas_counts_in_the_loss_bounds() {
    use crate::sources::{LIVE_REALIZED_LOSS_MAX, NonceOrigin};
    let limits = crate::sources::GrantLimits {
        max_realized_loss_wei: 1_000,
        max_daily_realized_loss_wei: 1_000,
        ..crate::sources::tests::TEST_LIMITS
    };
    let mut r = limited("live-burn-loss", limits);
    r.exec.exec.live_nonce_burn = true;
    let g = granted_generation(&r.state);
    let ledger = Ledger::open(&r.state);
    // Another sender's transaction, a burn requested of it.
    let other = Fact {
        origin: Some(NonceOrigin {
            chain_id: crate::wss::BSC_MAINNET_CHAIN_ID,
            sender: [0x0e; 20],
        }),
        ..unknown_window([0x7c; 32], 3, ComponentId::StrategyC)
    };
    ledger.append(&other, 1).unwrap();
    ledger
        .append(
            &Fact {
                operator: Some(crate::live_ledger::Operator {
                    request_id: "burn-o".into(),
                    decided_by: "operator".into(),
                    evidence: None,
                    at_ms: NOW,
                }),
                ..Fact::about(&other, FactKind::BurnRequested)
            },
            2,
        )
        .unwrap();
    // OFF: not the buyer's request to hold back either.
    r.exec.exec.live_nonce_burn = false;
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!refused_by(&x, "NONCE_BURN_LOCKED"), "{:?}", reason(&x));
    x.shutdown();
    r.exec.exec.live_nonce_burn = true;
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.burn_target().is_none());
    let t = x.book().tx(&[0x7c; 32]).unwrap().clone();
    assert!(!x.burnable(&t));
    x.shutdown();
    // A burn of this grant included at nonce 4, its gas above the bound.
    ledger
        .append(
            &Fact {
                block: Some((HEAD - 2, [0x42; 32], NOW)),
                ..Fact::about(&other, FactKind::Replaced)
            },
            3,
        )
        .unwrap();
    let burn = Fact {
        tx_hash: [0x7d; 32],
        intent: LiveIntentKind::Burn,
        token: [0; 20],
        nonce: 4,
        grant_generation: g,
        max_block: Some(HEAD),
        origin: None,
        ..unknown_window([0; 32], 4, ComponentId::StrategyC)
    };
    ledger.append(&burn, 4).unwrap();
    ledger
        .append(
            &Fact {
                block: Some((HEAD - 1, [0x41; 32], NOW)),
                gas_wei: Some(U256::from(5_000u64)),
                ..Fact::about(&burn, FactKind::Included)
            },
            5,
        )
        .unwrap();
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.book().burn_gas(|_| true), U256::from(5_000u64));
    assert_eq!(
        x.buy_refusal(U256::ZERO, Some(&c2), Some(5)),
        Some((Refusal::GrantLimit, Some(LIVE_REALIZED_LOSS_MAX)))
    );
    x.shutdown();
    r.clean();
    // Review NIT-2: the day's bound alone (the grant's far): the burn's
    // gas, included today, holds BUYs back.
    let mut r = limited(
        "live-burn-day",
        crate::sources::GrantLimits {
            max_realized_loss_wei: 1_000_000,
            ..limits
        },
    );
    r.exec.exec.live_nonce_burn = true;
    let ledger = Ledger::open(&r.state);
    let burn = Fact {
        grant_generation: granted_generation(&r.state),
        ..burn
    };
    ledger.append(&burn, 1).unwrap();
    ledger
        .append(
            &Fact {
                block: Some((HEAD - 1, [0x41; 32], NOW)),
                gas_wei: Some(U256::from(5_000u64)),
                ..Fact::about(&burn, FactKind::Included)
            },
            2,
        )
        .unwrap();
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(
        x.buy_refusal(U256::ZERO, Some(&c2), Some(5)),
        Some((
            Refusal::GrantLimit,
            Some(crate::sources::LIVE_DAILY_LOSS_MAX)
        ))
    );
    x.shutdown();
    r.clean();
}

/// R3b review LOW-N1, NIT-1: a burn the operator requests between two
/// ticks (the book folded again as a block's facts are written) holds
/// a BUY back at once, as it frees the nonce; every requested
/// transaction of the buyer at the next nonce is free, one not
/// requested there blocks. The burn replaces them all.
#[test]
fn a_burn_requested_between_ticks_holds_a_buy_at_once() {
    const A: [u8; 32] = [0x7e; 32];
    const B: [u8; 32] = [0x7f; 32];
    let mut r = Rig::new("live-burn-between", LiveStrategies::C3);
    r.exec.exec.live_nonce_burn = true;
    let ledger = Ledger::open(&r.state);
    for (tx, at) in [(A, 1), (B, 2)] {
        ledger
            .append(&unknown_window(tx, 5, ComponentId::ExecC3), at)
            .unwrap();
    }
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!x.armed(), "both block");
    ledger
        .request_burn(
            A,
            Some(r.buyer),
            "burn-a",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    x.refold();
    assert_eq!(x.free_nonce(&c2), None, "B, not requested, blocks");
    ledger
        .request_burn(
            B,
            Some(r.buyer),
            "burn-b",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    x.refold();
    assert_eq!(x.free_nonce(&c2), Some(5));
    assert_eq!(
        x.buy_refusal(U256::ZERO, Some(&c2), Some(5)),
        Some((
            Refusal::ExitFirst,
            Some(crate::sources::LIVE_NONCE_BURN_FIRST)
        ))
    );
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let burnt = x.burn(&c2, &a_clock(), DECIDED_MS);
    assert_eq!(burnt.len(), 1, "{:?}", reason(&x));
    include(
        &mut x,
        &mut c2,
        vec![CanonicalTx {
            hash: burnt[0].handoff.tx_hash,
            index: 0,
            sender: r.buyer,
            nonce: 5,
            succeeded: true,
            cost: None,
        }],
    );
    for tx in [A, B] {
        assert_eq!(x.book().tx(&tx).unwrap().state, FactKind::Replaced);
    }
    assert!(x.burn_target().is_none());
    x.shutdown();
    r.clean();
}

/// R3b review NIT-4: a burn of no lot (an earlier burn's, its window
/// unknown) with nothing open arms the path on its own gas; short of
/// that, UNFUNDED names the burn.
#[test]
fn a_burn_of_no_lot_arms_on_its_own_gas() {
    const TARGET: [u8; 32] = [0x70; 32];
    let mut r = Rig::new("live-burn-funds", LiveStrategies::StrategyC);
    r.exec.exec.live_nonce_burn = true;
    let ledger = Ledger::open(&r.state);
    let earlier = Fact {
        intent: LiveIntentKind::Burn,
        token: [0; 20],
        input_wei: None,
        ..unknown_window(TARGET, 0, ComponentId::StrategyC)
    };
    ledger.append(&earlier, 1).unwrap();
    ledger
        .request_burn(
            TARGET,
            Some(r.buyer),
            "burn-1",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    let c2 = c2_at(&r.d, r.buyer, 0);
    let mut x = r.probed("boot-1", one_burn);
    assert_eq!(x.burn_cost(), U256::from(ONE_BURN_WEI));
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert!(x.book().positions.is_empty());
    let burnt = x.burn(&c2, &a_clock(), DECIDED_MS);
    assert_eq!(burnt.len(), 1, "{:?}", reason(&x));
    x.shutdown();
    let mut x = r.probed("boot-1", empty);
    settle(&mut x, &c2);
    let (state, why) = reason(&x);
    assert_eq!(state, "UNFUNDED");
    assert!(why.as_deref().unwrap().contains("nonce burn"), "{why:?}");
    x.shutdown();
    r.clean();
}

/// BSC3-024 (review MEDIUM): the end cannot write its BUY's fact (the
/// ledger refuses writes): the arm ends at once and keeps its fence;
/// this process does not arm again (a fact it wrote may be missing);
/// the restart recovers the BUY from the owner's admission entry as
/// UNKNOWN, and nothing arms while it is unresolved.
#[test]
fn a_fact_the_end_cannot_write_ends_the_arm_and_is_recovered() {
    let r = Rig::new("live-unwritable", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let ledger = r.state.join("live-ledger");
    fs::create_dir_all(&ledger).unwrap();
    fs::set_permissions(&ledger, fs::Permissions::from_mode(0o500)).unwrap();
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .unwrap();
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!x.armed());
    let (state, why) = reason(&x);
    assert_eq!(state, "ENDED");
    assert!(
        why.as_deref()
            .unwrap()
            .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE),
        "{why:?}"
    );
    assert_eq!(r.fences(".fence"), vec!["live-7-0.fence"]);
    // Writable again: this process still does not arm.
    fs::set_permissions(&ledger, fs::Permissions::from_mode(0o700)).unwrap();
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    assert!(
        s.request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
            .unwrap()
            .applied()
    );
    settle(&mut x, &c2);
    r.authorize("boot-1", "regrant");
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert!(
        reason(&x)
            .1
            .unwrap()
            .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE)
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    std::mem::forget(x);
    // The restart: the BUY's lease (BSC3-076: its fact and the BUY's
    // were not written) is a fact, UNKNOWN at the BUY's nonce, and
    // blocks the arm.
    r.authorize("boot-2", "after-restart");
    let mut x = r.runtime("boot-2");
    assert!(x.book().tx(&buy.handoff.tx_hash).is_none());
    let unresolved: Vec<_> = x.book().unresolved().map(|t| &t.released).collect();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(
        (unresolved[0].kind, unresolved[0].token, unresolved[0].nonce),
        (crate::live_ledger::FactKind::Unknown, [0; 20], 5)
    );
    let c2 = c2_at(&r.d.join("c2-restarted"), r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert!(
        reason(&x)
            .1
            .unwrap()
            .starts_with(crate::sources::LIVE_RELEASE_UNRESOLVED)
    );
    r.clean();
}

/// BSC3-041/045: an entry of the ledger that does not read blocks the
/// arm with its reason; nothing is folded from a partial ledger.
#[test]
fn an_unreadable_ledger_blocks_the_arm() {
    let r = Rig::new("live-unreadable", LiveStrategies::C3);
    let ledger = r.state.join("live-ledger");
    fs::create_dir_all(&ledger).unwrap();
    fs::write(ledger.join("00000000000000000000.dbrec"), b"torn").unwrap();
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(!x.armed());
    let (state, why) = reason(&x);
    assert_eq!(state, "BLOCKED");
    assert!(
        why.as_deref()
            .unwrap()
            .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE),
        "{why:?}"
    );
    assert!(r.fences(".fence").is_empty());
    r.clean();
}

/// BSC3-027: the buyer's balance must hold a BUY, its exit and the
/// exits of what is open (approval and SELL at the signed gas). Enough
/// for a BUY but not for its exit: nothing arms (UNFUNDED, the need
/// named). With a position open and enough for its exit only: the path
/// arms for the exit, a BUY is refused, the exit goes.
#[test]
fn a_buyer_short_of_the_exits_buys_nothing() {
    use crate::paper_exec::{IntentKind, Refusal};
    let r = Rig::new("live-short", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let mut x = r.probed("boot-1", short);
    let c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(!x.armed());
    let (state, why) = reason(&x);
    assert_eq!(state, "UNFUNDED");
    assert!(why.as_deref().unwrap().contains("exit"), "{why:?}");
    x.shutdown();
    // A position of A is open (bought at nonce 0, included).
    let a = [0xA1; 20];
    let ledger = crate::live_ledger::Ledger::open(&r.state);
    let buy = crate::live_ledger::Fact {
        kind: crate::live_ledger::FactKind::Released,
        tx_hash: [0x31; 32],
        intent: crate::sources::LiveIntentKind::Buy,
        component: ComponentId::StrategyC,
        token: a,
        nonce: 0,
        grant_generation: 1,
        input_wei: Some(sc.input),
        max_wei: Some(sc.input),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD + 2),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    ledger.append(&buy, 1).unwrap();
    let included = crate::live_ledger::Fact {
        block: Some((HEAD, [0xb1; 32], 1_793_000_000_000)),
        gas_wei: Some(U256::from(1u8)),
        tokens: Some(sc.tokens),
        eth_wei: Some(sc.input),
        ..crate::live_ledger::Fact::about(&buy, crate::live_ledger::FactKind::Included)
    };
    ledger.append(&included, 1).unwrap();
    // And one of B (nonce 1): both exits (1.6e14) are more than the
    // balance (1e14), one exit step is not; the path arms for them.
    let b = crate::live_ledger::Fact {
        tx_hash: [0x32; 32],
        token: [0xB2; 20],
        nonce: 1,
        ..buy.clone()
    };
    ledger.append(&b, 1).unwrap();
    let b_included = crate::live_ledger::Fact {
        tx_hash: b.tx_hash,
        token: b.token,
        nonce: 1,
        ..included.clone()
    };
    ledger.append(&b_included, 1).unwrap();
    let mut x = r.probed("boot-1", exits_only);
    let c2 = c2_at(&r.d.join("c2-open"), r.buyer, 2);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    // Each open lot's exit: approval 5e12 + SELL 7.5e13 at the setup's
    // gas.
    let (approval, sell) = x.exit_gas();
    assert_eq!(
        (approval, sell),
        (
            U256::from(5_000_000_000_000u64),
            U256::from(75_000_000_000_000u64)
        )
    );
    assert_eq!(x.exit_reserve(), (approval + sell) * U256::from(2u8));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let (out, refused) = x
        .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
    assert!(
        record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"BUYS_UNFUNDED=1")
    );
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
    assert_eq!(out[0].kind, IntentKind::Approve);
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    // A's approval is out: its SELL and B's exit are left to reserve.
    assert_eq!(x.exit_reserve(), sell + approval + sell);
    x.shutdown();
    r.clean();
}

/// REV.1.0 R2 review L6: a provider's answer read before a disarm is
/// kept for the attempt's `a` record (the end that read it is gone).
#[test]
fn an_answer_read_before_a_disarm_is_kept_for_its_record() {
    let r = Rig::answering(
        "live-answer-disarm",
        LiveStrategies::StrategyC,
        vec![crate::live_end::tests::Answer::Ack],
    );
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (out, _) = x
        .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, 1_793_000_000_500)
        .unwrap();
    let tx = out[0].handoff.tx_hash;
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while x.answers.is_empty() && Instant::now() < deadline {
        x.poll_responses(&clock);
        std::thread::sleep(Duration::from_millis(5));
    }
    x.disarm();
    let answers = x.take_answers();
    assert_eq!(answers.len(), 1);
    assert_eq!((answers[0].tx_hash, answers[0].outcome), (tx, "ACK"));
    assert!(answers[0].at_ns.is_some());
    std::mem::forget(x);
    r.clean();
}

/// REV.1.0 R2 review M2: an included BUY whose Portal trade went to
/// another trader acquired nothing: no tokens in its inclusion or its
/// lot, its placement NO_TOKENS (the halt follows).
#[test]
fn a_buy_whose_trade_went_to_another_trader_acquired_nothing() {
    use crate::live_ledger::PlacementVerdict;
    let r = Rig::new("live-buy-other-recipient", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .expect("the BUY");
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    let tx = |hash, index, sender, nonce| CanonicalTx {
        hash,
        index,
        sender,
        nonce,
        succeeded: true,
        cost: None,
    };
    let block = CanonicalEvent::Block {
        number: HEAD + 1,
        hash: [0xb2; 32],
        parent_hash: [0xb1; 32],
        timestamp_s: 1_793_000_001,
        timestamp_ms: 1_793_000_001_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: vec![
            tx([0x1a; 32], 0, launcher, 3),
            tx(buy.handoff.tx_hash, 1, r.buyer, 5),
        ],
    };
    c2.canonical(&block);
    let _ = x.apply_evidence(&c2.take_evidence());
    let _ = x.on_canonical(&block);
    let _ = x.on_canonical(&launch_event(HEAD + 1, 0xb2, 0, buy.token, launcher));
    // The buyer's transaction, its Portal BUY's trader another one.
    let mut other = buy_trade(
        HEAD + 1,
        0xb2,
        1,
        buy.handoff.tx_hash,
        buy.token,
        [0x77; 20],
    );
    if let CanonicalEvent::FlapTrade(t) = &mut other {
        t.origin = r.buyer;
    }
    let _ = x.on_canonical(&other);
    let _ = x.on_canonical(&CanonicalEvent::BlockComplete {
        number: HEAD + 1,
        hash: [0xb2; 32],
    });
    let t = x.book.tx(&buy.handoff.tx_hash).unwrap();
    assert_eq!(
        t.inclusion.as_ref().and_then(|f| f.tokens),
        Some(U256::ZERO)
    );
    assert_eq!(t.placement.unwrap().verdict, PlacementVerdict::NoTokens);
    assert!(
        x.book
            .open_lot(&buy.token)
            .is_none_or(|p| p.tokens.is_zero())
    );
    x.shutdown();
    r.clean();
}

/// BSC3-051: a provider refuses a BUY. The answer is read before the
/// next decision: the next BUY is refused before anything is signed
/// (no attempt, nothing sent again), the status counts the answer and
/// the refusal, the refused BUY stays unresolved in the ledger.
#[test]
fn a_buy_a_provider_refused_closes_the_arms_buys() {
    use crate::paper_exec::Refusal;
    let r = Rig::answering(
        "live-provider-reject",
        LiveStrategies::StrategyC,
        vec![crate::live_end::tests::Answer::Reject],
    );
    let sc = Sc::new(r.buyer);
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let (out, _) = x
        .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
        .unwrap();
    let first = out[0].handoff.tx_hash;
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while x.executor.as_ref().unwrap().end().buy_refused().is_none() && Instant::now() < deadline {
        x.poll_responses(&clock);
        std::thread::sleep(Duration::from_millis(5));
    }
    let used = buy_usage(
        &r.state,
        NOW,
        crate::sources::tests::shipped_limits().buy_attempts_total_max,
    )
    .unwrap()
    .used;
    let (out, refused) = x
        .on_strategy(&sc.buy([0xB2; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Dispatch)));
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        used
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    let status = record(&x);
    assert!(status.texts(l::RESPONSES).unwrap().contains(&"REJECT=1"));
    assert!(
        status
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"BUYS_PROVIDER_REFUSED=1")
    );
    x.record_responses();
    let tx = x.book().tx(&first).unwrap();
    assert_eq!(tx.state, crate::live_ledger::FactKind::Released);
    assert_eq!(tx.responses, vec!["CLUB48_BUILDER_V1:REJECT".to_string()]);
    std::mem::forget(x);
    // Another process under the same grant, once A's window is over
    // (it arms at nonce 0 again): the ledger keeps the BUYs closed
    // until the operator grants anew.
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d.join("c2-again"), r.buyer, 0);
    let mut parent = 0xb1;
    for k in 1..=3u8 {
        let h = 0xd0 + k;
        feed(
            &mut x,
            &mut c2,
            vec![
                block_of(HEAD + u64::from(k), h, parent, Vec::new()),
                complete_of(HEAD + u64::from(k), h),
            ],
        );
        parent = h;
    }
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (out, refused) = x
        .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Dispatch)));
    x.shutdown();
    r.clean();
}

/// BSC3-058: under the operator's separate EXIT-ONLY grant nothing arms
/// without an open position (the reason names it); with one the path
/// arms for its exit: a BUY is refused before anything is signed, the
/// exit's approval goes.
#[test]
fn an_exit_only_grant_arms_for_the_exits_of_open_positions_only() {
    use crate::paper_exec::IntentKind;
    let r = Rig::new("live-rt-exit-only", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let grant = LiveGrant {
        max_input_wei: 10_000_000_000_000_000,
        max_spend_wei: 100_000_000_000_000_000,
        adapters: vec![LiveAdapter::Club48BuilderV1],
        exit_only: true,
        ..grant_of(NOW + 3_600_000, LiveStrategies::StrategyC)
    };
    let g = s.current().unwrap().generation;
    let receipt = s
        .grant_live("exit-only", grant.clone(), grant.scope_sha256(), g, NOW)
        .unwrap();
    assert!(receipt.applied(), "{}", receipt.line());
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(
        reason(&x).1.as_deref(),
        Some(crate::sources::LIVE_GRANT_EXIT_ONLY)
    );
    x.shutdown();
    // A position of A is open (bought at nonce 0 before, included).
    let a = [0xA1; 20];
    let ledger = crate::live_ledger::Ledger::open(&r.state);
    let buy = crate::live_ledger::Fact {
        kind: crate::live_ledger::FactKind::Released,
        tx_hash: [0x51; 32],
        intent: crate::sources::LiveIntentKind::Buy,
        component: ComponentId::StrategyC,
        token: a,
        nonce: 0,
        grant_generation: 1,
        input_wei: Some(sc.input),
        max_wei: Some(sc.input),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD + 2),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    ledger.append(&buy, 1).unwrap();
    let included = crate::live_ledger::Fact {
        block: Some((HEAD, [0xb1; 32], 1_793_000_000_000)),
        gas_wei: Some(U256::from(1u8)),
        tokens: Some(sc.tokens),
        eth_wei: Some(sc.input),
        ..crate::live_ledger::Fact::about(&buy, crate::live_ledger::FactKind::Included)
    };
    ledger.append(&included, 1).unwrap();
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d.join("c2-open"), r.buyer, 1);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let (out, refused) = x
        .on_strategy(&sc.buy([0xC3; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!(
        (out.len(), refused),
        (0, Some(crate::paper_exec::Refusal::Owner))
    );
    assert!(
        record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"BUYS_CLOSED=1")
    );
    let (out, refused) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
    assert_eq!(out[0].kind, IntentKind::Approve);
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    x.shutdown();
    r.clean();
}

/// BSC3-072 (review LOW): a C3 BUY on the LIVE path exits through the
/// strategy once its position opens: the strategy's cap must hold its
/// exit, or the BUY is refused before anything is signed.
#[test]
fn a_c3_buy_needs_room_for_its_exit_in_the_strategy_cap() {
    let mut r = Rig::new("live-c3-exit-room", LiveStrategies::C3);
    let x = &r.exec.exec;
    let price = U256::from(x.gas_price_wei);
    let exit = price * U256::from(x.approve_gas_limit + x.sell_gas_limit);
    r.exec.exec.strategy_session_cap_wei = u128::try_from(exit).unwrap() - 1;
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    assert!(
        x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_none()
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    assert!(
        record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"SESSION_CAP=1")
    );
    x.shutdown();
    r.clean();
}

/// BSC3-027 (review MEDIUM): the probe's balance is of its block. A
/// transaction the ledger holds included in a later block is not paid
/// in it yet: it counts against what is left.
#[test]
fn what_a_later_block_included_is_not_paid_in_the_probed_balance() {
    use crate::paper_exec::Refusal;
    let r = Rig::new("live-later", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let ledger = crate::live_ledger::Ledger::open(&r.state);
    let x = &r.exec.exec;
    let most = sc.input + U256::from(x.gas_price_wei) * U256::from(x.gas_limit);
    let buy = crate::live_ledger::Fact {
        kind: crate::live_ledger::FactKind::Released,
        tx_hash: [0x41; 32],
        intent: crate::sources::LiveIntentKind::Buy,
        component: ComponentId::StrategyC,
        token: [0xA1; 20],
        nonce: 0,
        grant_generation: 1,
        input_wei: Some(sc.input),
        max_wei: Some(most),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD + 2),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    ledger.append(&buy, 1).unwrap();
    let included = crate::live_ledger::Fact {
        block: Some((HEAD + 1, [0xb2; 32], 1_793_000_001_000)),
        gas_wei: Some(U256::from(1u8)),
        tokens: Some(sc.tokens),
        eth_wei: Some(sc.input),
        ..crate::live_ledger::Fact::about(&buy, crate::live_ledger::FactKind::Included)
    };
    ledger.append(&included, 1).unwrap();
    // The probe reads the balance at HEAD, before A's block.
    let mut x = r.probed("boot-1", two);
    let c2 = c2_at(&r.d, r.buyer, 1);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    // B with its exit and A's needs more than the balance less A.
    let (out, refused) = x
        .on_strategy(&sc.buy([0xB2; 20]), &c2, &clock, 1_793_000_000_500)
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    x.shutdown();
    r.clean();
}

/// BSC3-027 (review MEDIUM): each BUY is checked against the probed
/// balance with its own most (a Strategy C BUY is not C3-sized), its
/// exit and the open exits, and what left since the probe counts: a
/// balance that arms the path but does not hold this BUY refuses it
/// (UNFUNDED); after a first BUY lands, a second one is checked
/// against what is left.
#[test]
fn each_buy_needs_its_most_and_the_exits_in_what_is_left() {
    use crate::paper_exec::Refusal;
    let r = Rig::new("live-left", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let mut x = r.probed("boot-1", mid);
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (out, refused) = x
        .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    x.shutdown();
    // Enough for one: it goes and lands; the second is checked against
    // the balance less the first's most.
    let mut x = r.probed("boot-1", two);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (out, refused) = x
        .on_strategy(&sc.buy([0xA1; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
    let first = out[0].handoff.tx_hash;
    r.requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let (approval, sell) = x.exit_gas();
    assert_eq!(reserve_of(&r.state, first), approval + sell);
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(first, 0)]),
            sc.trade(first, HEAD + 1, 0xb2, [0xA1; 20]),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    reconnect(&mut x);
    let (out, refused) = x
        .on_strategy(&sc.buy([0xB2; 20]), &c2, &clock, now_ms)
        .unwrap();
    assert_eq!((out.len(), refused), (0, Some(Refusal::Unfunded)));
    assert!(
        record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"BUYS_UNFUNDED=1")
    );
    x.shutdown();
    r.clean();
}

/// BSC3-047: a token bought, exited and bought again in a later arm is
/// a second lot, with its own tokens. A reorg that takes the first
/// lot's SELL and the second lot's BUY reopens the first lot: the
/// second ends before the first is reported open, and back. After a
/// restart with the second lot open, its exit approves again (the
/// first lot's approval went with its SELL), its own tokens; the book
/// holds both lots, the first closed with its result.
#[test]
fn a_token_bought_again_approves_its_exit_again() {
    use crate::chain_follower::CanonicalTrade;
    use crate::live_ledger::PositionState;
    use crate::paper_exec::IntentKind;
    use darkbot_bsc_trading::transactions::legacy::decode_signed;
    use darkbot_bsc_trading::venues::flap::abi::decode_approve;
    use darkbot_bsc_trading::venues::flap::recognize::decode_swap_exact_input;
    let r = Rig::new("live-rebuy", LiveStrategies::StrategyC);
    let sc = Sc::new(r.buyer);
    let a = [0xA1; 20];
    let tokens2 = sc.tokens * U256::from(2u64);
    let t = Cell::new(5_000_000_000u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let now_ms = 1_793_000_000_500;
    let wait = Duration::from_secs(2);
    let trade = |hash, n, h, sell, amount: U256| {
        CanonicalEvent::FlapTrade(CanonicalTrade {
            block_number: n,
            block_hash: [h; 32],
            tx_index: 0,
            tx_hash: hash,
            log_index: 0,
            origin: r.buyer,
            sell,
            token: a,
            trader: r.buyer,
            amount: amount.to_be_bytes(),
            eth: sc.input.to_be_bytes(),
            fee: [0; 32],
            post_price: [0; 32],
        })
    };
    let sold = |hash, n, h| trade(hash, n, h, true, sc.tokens);
    // Lot 1: bought at nonce 0, approved at 1, sold at 2.
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 0);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (out, _) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
    let buy1 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xb2, 0xb1, vec![sc.receipt(buy1, 0)]),
            sc.trade(buy1, HEAD + 1, 0xb2, a),
            complete_of(HEAD + 1, 0xb2),
        ],
    );
    reconnect(&mut x);
    let (out, _) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!(out[0].kind, IntentKind::Approve);
    let approve1 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 2, 0xb3, 0xb2, vec![sc.receipt(approve1, 1)]),
            complete_of(HEAD + 2, 0xb3),
        ],
    );
    reconnect(&mut x);
    let (out, _) = x.on_strategy(&sc.exit(a), &c2, &clock, now_ms).unwrap();
    assert_eq!(out[0].kind, IntentKind::Sell);
    let sell1 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    let changes = feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 3, 0xb4, 0xb3, vec![sc.receipt(sell1, 2)]),
            sold(sell1, HEAD + 3, 0xb4),
            complete_of(HEAD + 3, 0xb4),
        ],
    );
    assert_eq!(changes, vec![PositionChange::Ended { token: a }]);
    x.shutdown();
    // Lot 2: the next process buys the token again, at nonce 3.
    r.authorize("boot-2", "rebuy");
    let mut x = r.runtime("boot-2");
    let mut c2 = c2_at(&r.d.join("c2-2"), r.buyer, 3);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (out, refused) = x.on_strategy(&sc.buy(a), &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
    let buy2 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    let changes = feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xd2, 0xb1, vec![sc.receipt(buy2, 3)]),
            trade(buy2, HEAD + 1, 0xd2, false, tokens2),
            complete_of(HEAD + 1, 0xd2),
        ],
    );
    let opened = |c: &PositionChange| match c {
        PositionChange::Opened { token, tokens, .. } if *token == a => Some(*tokens),
        _ => None,
    };
    assert_eq!(changes.len(), 1);
    assert_eq!(opened(&changes[0]), Some(tokens2));
    // A reorg takes SELL 1's and BUY 2's blocks: lot 2 ends, lot 1 is
    // open again (its SELL unresolved); included again, back.
    let changes = x.on_canonical(&CanonicalEvent::Reorg {
        orphaned: vec![(HEAD + 3, [0xb4; 32]), (HEAD + 1, [0xd2; 32])],
        new_head: (HEAD + 1, [0xe2; 32]),
    });
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert_eq!(changes[0], PositionChange::Ended { token: a });
    assert_eq!(opened(&changes[1]), Some(sc.tokens));
    assert_eq!(x.book().positions[0].state, PositionState::Exiting);
    assert_eq!(x.book().positions[1].state, PositionState::Pending);
    let mut changes = Vec::new();
    for e in [
        block_of(
            HEAD + 1,
            0xe2,
            0xb1,
            vec![sc.receipt(sell1, 2), sc.receipt(buy2, 3)],
        ),
        sold(sell1, HEAD + 1, 0xe2),
        trade(buy2, HEAD + 1, 0xe2, false, tokens2),
        complete_of(HEAD + 1, 0xe2),
    ] {
        changes.extend(x.on_canonical(&e));
    }
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert_eq!(changes[0], PositionChange::Ended { token: a });
    assert_eq!(opened(&changes[1]), Some(tokens2));
    x.shutdown();
    // A restart with lot 2 open: its exit approves first.
    r.authorize("boot-3", "lot-two");
    let mut x = r.runtime("boot-3");
    let mut c2 = c2_at(&r.d.join("c2-3"), r.buyer, 4);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let exit2 = crate::runtime_strategy::StrategyAction::Exit {
        token: a,
        tokens: tokens2,
        curve: sc.held,
        decided_ns: 0,
    };
    let (out, refused) = x.on_strategy(&exit2, &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
    assert_eq!(out[0].kind, IntentKind::Approve);
    let input = decode_signed(&out[0].raw).unwrap().tx.input;
    assert_eq!(decode_approve(input).unwrap().1, tokens2);
    let approve2 = out[0].handoff.tx_hash;
    r.requests.recv_timeout(wait).unwrap();
    feed(
        &mut x,
        &mut c2,
        vec![
            block_of(HEAD + 1, 0xf2, 0xb1, vec![sc.receipt(approve2, 4)]),
            complete_of(HEAD + 1, 0xf2),
        ],
    );
    reconnect(&mut x);
    let (out, refused) = x.on_strategy(&exit2, &c2, &clock, now_ms).unwrap();
    assert_eq!((out.len(), refused), (1, None), "{:?}", reason(&x));
    assert_eq!(out[0].kind, IntentKind::Sell);
    let input = decode_signed(&out[0].raw).unwrap().tx.input;
    assert_eq!(
        decode_swap_exact_input(input).unwrap().input_amount,
        tokens2
    );
    r.requests.recv_timeout(wait).unwrap();
    let book = x.book();
    assert_eq!(book.positions.len(), 2, "{:?}", book.positions);
    assert_eq!(book.positions[0].state, PositionState::Closed);
    assert_eq!(book.positions[1].state, PositionState::Exiting);
    assert_eq!(book.positions[1].tokens, tokens2);
    assert!(book.positions[1].approved && book.positions[0].approved);
    x.shutdown();
    r.clean();
}

/// The buyer's nonce `nonce` on chain 56.
fn nonce_of(buyer: [u8; 20], nonce: u64) -> NonceKey {
    NonceKey {
        origin: NonceOrigin {
            chain_id: 56,
            sender: buyer,
        },
        nonce,
    }
}

/// BUY `n` of `buyer` at `nonce` on chain 56, released, of at most
/// `most`.
fn alt_buy(n: u8, buyer: [u8; 20], nonce: u64, most: u64) -> Fact {
    Fact {
        kind: FactKind::Released,
        tx_hash: [n; 32],
        intent: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        token: [n; 20],
        nonce,
        grant_generation: 1,
        input_wei: Some(U256::from(most / 2)),
        max_wei: Some(U256::from(most)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD + 2),
        by_tx: None,
        response: None,
        origin: Some(NonceOrigin {
            chain_id: 56,
            sender: buyer,
        }),
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    }
}

/// Block `number` (hash `[h; 32]`) as a fact's block.
fn at_block(number: u64, h: u8) -> Option<(u64, [u8; 32], u64)> {
    Some((number, [h; 32], 1_793_000_000_000 + (number - HEAD) * 1_000))
}

/// `of` included in block `number` (hash `[h; 32]`).
fn landed(of: &Fact, number: u64, h: u8) -> Fact {
    Fact {
        block: at_block(number, h),
        gas_wei: Some(U256::from(1u8)),
        tokens: Some(U256::from(1_000u64)),
        eth_wei: of.input_wei,
        ..Fact::about(of, FactKind::Included)
    }
}

/// The LIVE path of `r` after `facts`, the buyer's balance `balance`
/// probed at block `block` (hash `[h; 32]`).
fn probed_book(r: &Rig, facts: &[Fact], balance: u64, block: u64, h: u8) -> LiveRuntime {
    let ledger = Ledger::open(&r.state);
    for f in facts {
        ledger.append(f, 1).unwrap();
    }
    let mut x = r.runtime("boot-1");
    x.funds = Some(Funds {
        balance: U256::from(balance),
        block,
        hash: [h; 32],
    });
    x
}

/// Budget fix (tests A, B, F, K at the balance): the alternatives of one
/// (chain, sender, nonce) come out of the balance once, at the most of
/// them, an UNKNOWN one included (no relief unproven); another nonce adds
/// its own most; a transaction without its sender and chain counts
/// alone. A new BUY at the nonce counts with its alternatives.
#[test]
fn alternatives_of_a_nonce_come_out_of_the_balance_once() {
    let r = Rig::new("live-alt-balance", LiveStrategies::C3);
    let b = r.buyer;
    let facts = [
        alt_buy(1, b, 7, 10_000),
        Fact {
            kind: FactKind::Unknown,
            ..alt_buy(2, b, 7, 12_000)
        },
        alt_buy(3, b, 7, 11_000),
        alt_buy(4, b, 8, 5_000),
        Fact {
            origin: None,
            ..alt_buy(5, b, 7, 3_000)
        },
    ];
    let x = probed_book(&r, &facts, 1_000_000, HEAD, 0xb1);
    let c2 = c2_at(&r.d, b, 7);
    let wei = |n: u64| U256::from(n);
    assert_eq!(
        x.available(Some(&c2)),
        Some(wei(1_000_000 - 12_000 - 5_000 - 3_000))
    );
    assert_eq!(
        x.left_besides(Some(&c2), Some(nonce_of(b, 7))),
        Some((wei(1_000_000 - 5_000 - 3_000), wei(12_000)))
    );
    // Of another sender the same nonce is no alternative.
    assert_eq!(
        x.left_besides(Some(&c2), Some(nonce_of([0xee; 20], 7))),
        Some((wei(1_000_000 - 12_000 - 5_000 - 3_000), U256::ZERO))
    );
    r.clean();
}

/// Budget fix (tests C, D): the old alternative lands late and the new
/// one is replaced by it: until the balance is probed in its block only
/// its most is to come, then nothing; a reorg of that block makes both
/// unresolved again and the nonce counts at its most, once; a restart
/// folds the same.
#[test]
fn a_late_alternative_and_a_reorg_keep_the_nonce_reserved() {
    let r = Rig::new("live-alt-late", LiveStrategies::C3);
    let b = r.buyer;
    let (old, new) = (alt_buy(1, b, 7, 10_000), alt_buy(2, b, 7, 12_000));
    let mut x = probed_book(&r, &[old.clone(), new.clone()], 1_000_000, HEAD, 0xb1);
    let c2 = c2_at(&r.d, b, 7);
    let wei = |n: u64| Some(U256::from(n));
    assert_eq!(x.available(Some(&c2)), wei(1_000_000 - 12_000));
    let ledger = Ledger::open(&r.state);
    let replaced = Fact {
        block: at_block(HEAD + 1, 0xb2),
        by_tx: Some(old.tx_hash),
        ..Fact::about(&new, FactKind::Replaced)
    };
    ledger.append(&landed(&old, HEAD + 1, 0xb2), 2).unwrap();
    x.refold();
    // Probed in the old one's block (C2 holds it): its nonce is paid,
    // the new one can no longer land, replaced or not yet.
    let mut c2b = c2_at(&r.d.join("b"), b, 7);
    c2b.canonical(&block_of(HEAD + 1, 0xb2, 0xb1, Vec::new()));
    let paid = Funds {
        balance: U256::from(990_000u64),
        block: HEAD + 1,
        hash: [0xb2; 32],
    };
    x.funds = Some(paid);
    assert_eq!(x.available(Some(&c2b)), wei(990_000));
    ledger.append(&replaced, 2).unwrap();
    x.refold();
    assert_eq!(x.available(Some(&c2b)), wei(990_000));
    // Probed before it: the old one's most is still to come, nothing
    // of the replaced one.
    x.funds = Some(Funds {
        balance: U256::from(1_000_000u64),
        block: HEAD,
        hash: [0xb1; 32],
    });
    assert_eq!(x.available(Some(&c2)), wei(1_000_000 - 10_000));
    for f in [&old, &new] {
        let orphaned = Fact {
            block: at_block(HEAD + 1, 0xb2),
            ..Fact::about(f, FactKind::Orphaned)
        };
        ledger.append(&orphaned, 3).unwrap();
    }
    x.refold();
    let before = Funds {
        balance: U256::from(1_000_000u64),
        block: HEAD,
        hash: [0xb1; 32],
    };
    x.funds = Some(before);
    assert_eq!(x.available(Some(&c2)), wei(1_000_000 - 12_000));
    x.shutdown();
    let mut y = r.runtime("boot-1");
    y.funds = Some(before);
    assert_eq!(y.available(Some(&c2)), wei(1_000_000 - 12_000));
    y.shutdown();
    r.clean();
}

/// Budget fix (test O): an inclusion at or below the probed block is
/// paid in the balance only while C2 holds that very block canonical:
/// a balance of another block at that number, of a block C2 does not
/// hold, or without C2 pays nothing.
#[test]
fn a_balance_of_another_block_pays_nothing() {
    let r = Rig::new("live-alt-snapshot", LiveStrategies::C3);
    let b = r.buyer;
    let buy = alt_buy(1, b, 7, 10_000);
    let mut x = probed_book(
        &r,
        &[buy.clone(), landed(&buy, HEAD, 0xb1)],
        1_000_000,
        HEAD,
        0xb1,
    );
    let c2 = c2_at(&r.d, b, 8);
    let (paid, unpaid) = (Some(U256::from(1_000_000u64)), Some(U256::from(990_000u64)));
    assert_eq!(x.available(Some(&c2)), paid);
    assert_eq!(x.available(None), unpaid);
    // Another block at HEAD, a block C2 does not hold; and (not about
    // C2) a balance from before the inclusion's block.
    for (block, h) in [(HEAD, 0xee), (HEAD + 3, 0xb4), (HEAD - 1, 0xb0)] {
        x.funds = Some(Funds {
            balance: U256::from(1_000_000u64),
            block,
            hash: [h; 32],
        });
        assert_eq!(x.available(Some(&c2)), unpaid, "{block}");
    }
    r.clean();
}

/// Budget fix (tests H, P): the exits of the pending alternative BUYs
/// of one nonce are reserved once, an open position (another nonce) in
/// full, a pending BUY without its sender and chain alone; a new BUY at
/// the alternatives' nonce covers them with its own exit. An included
/// BUY paid in the balance and a new nonce count once each.
#[test]
fn the_exits_of_alternative_buys_are_reserved_once() {
    let r = Rig::new("live-alt-exits", LiveStrategies::C3);
    let b = r.buyer;
    let open = alt_buy(3, b, 6, 9_000);
    let facts = [
        open.clone(),
        landed(&open, HEAD, 0xb1),
        alt_buy(1, b, 7, 10_000),
        alt_buy(2, b, 7, 12_000),
    ];
    let balance = 1_000_000_000_000_000u64;
    let mut x = probed_book(&r, &facts, balance, HEAD, 0xb1);
    x.observation = Some(funded("", &limits(), &bsc_mainnet_v5_27_2(), Address::ZERO).unwrap());
    let c2 = c2_at(&r.d, b, 7);
    let (approval, sell) = x.exit_gas();
    let lot = approval + sell;
    assert_eq!(x.book.count(PositionState::Open), 1);
    assert_eq!(x.exit_reserve(), lot * U256::from(2u8));
    assert_eq!(x.exit_reserve_besides(Some(nonce_of(b, 7))), lot);
    assert_eq!(
        x.exit_reserve_besides(Some(nonce_of(b, 6))),
        lot * U256::from(2u8)
    );
    // Paid in the balance: the open position's BUY; to come: nonce 7.
    let left = U256::from(balance - 12_000);
    assert_eq!(x.available(Some(&c2)), Some(left));
    // A BUY at nonce 7: its own most past the alternatives', its exit,
    // and the open position's.
    let room = left + U256::from(12_000u64) - lot - lot;
    assert_eq!(x.unfunded(room, Some(&c2), Some(7)), None);
    assert!(
        x.unfunded(room + U256::from(1u8), Some(&c2), Some(7))
            .is_some()
    );
    // A BUY at nonce 8: nonce 7 stays to come, and its exits.
    let room8 = left - lot - lot - lot;
    assert_eq!(x.unfunded(room8, Some(&c2), Some(8)), None);
    assert!(
        x.unfunded(room8 + U256::from(1u8), Some(&c2), Some(8))
            .is_some()
    );
    // A BUY at nonce 7 of less than the alternatives' most still needs
    // their most: one of them may land instead.
    x.funds = Some(Funds {
        balance: U256::from(12_000u64) + lot + lot,
        block: HEAD,
        hash: [0xb1; 32],
    });
    assert_eq!(x.unfunded(U256::from(5_000u64), Some(&c2), Some(7)), None);
    x.funds = Some(Funds {
        balance: U256::from(12_000u64) + lot + lot - U256::from(1u8),
        block: HEAD,
        hash: [0xb1; 32],
    });
    assert!(
        x.unfunded(U256::from(5_000u64), Some(&c2), Some(7))
            .is_some()
    );
    // A pending BUY without its sender and chain: its exit alone.
    let old = Fact {
        origin: None,
        ..alt_buy(4, b, 7, 8_000)
    };
    Ledger::open(&r.state).append(&old, 2).unwrap();
    x.refold();
    assert_eq!(x.exit_reserve(), lot * U256::from(3u8));
    r.clean();
}

/// Budget fix (test E): two alternatives of one nonce admitted by the
/// owner, the process ending before either outcome was written: the
/// restart recovers both as UNKNOWN with their sender and chain; the
/// owner's spend and the balance count the nonce once, at its most. One
/// lands, the other is replaced: the balance probed in that block pays
/// it; the grant's spend stays at the most (nothing is released).
#[test]
fn alternatives_admitted_before_a_crash_count_once_after_it() {
    use crate::sources::LiveIntent;
    let r = Rig::new("live-alt-crash", LiveStrategies::C3);
    let b = r.buyer;
    let store = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let q = crate::sources::tests::qualified_in(&store);
    let armed = store.live_now(&[ComponentId::ExecC3], &q, NOW).unwrap();
    let origin = NonceOrigin {
        chain_id: 56,
        sender: b,
    };
    let ids = [
        alloy_primitives::hex::encode([1u8; 32]),
        alloy_primitives::hex::encode([2u8; 32]),
    ];
    for (id, most) in [(&ids[0], 10_000u128), (&ids[1], 12_000)] {
        let intent = LiveIntent {
            id,
            component: ComponentId::ExecC3,
            kind: LiveIntentKind::Buy,
            input_wei: 1_000,
            max_wei: most,
            token: [0x10; 20],
            nonce: 7,
            origin,
            max_block: HEAD + 2,
            exit_reserve_wei: 0,
        };
        store.admit_live(&armed, &intent, &q, NOW).unwrap();
    }
    let generation = armed.grant_generation();
    assert_eq!(
        crate::sources::spent_by_grant(&r.state, generation).unwrap(),
        12_000
    );
    let mut x = r.runtime("boot-1");
    assert_eq!(x.book.txs.len(), 2);
    assert!(
        x.book
            .txs
            .iter()
            .all(|t| t.state == FactKind::Unknown && t.released.key() == Some(nonce_of(b, 7)))
    );
    x.funds = Some(Funds {
        balance: U256::from(1_000_000u64),
        block: HEAD,
        hash: [0xb1; 32],
    });
    let c2 = c2_at(&r.d, b, 7);
    assert_eq!(
        x.available(Some(&c2)),
        Some(U256::from(1_000_000u64 - 12_000))
    );
    let (first, second) = (
        x.book.txs[0].released.clone(),
        x.book.txs[1].released.clone(),
    );
    let ledger = Ledger::open(&r.state);
    ledger.append(&landed(&first, HEAD + 1, 0xb2), 2).unwrap();
    let replaced = Fact {
        block: at_block(HEAD + 1, 0xb2),
        by_tx: Some(first.tx_hash),
        ..Fact::about(&second, FactKind::Replaced)
    };
    ledger.append(&replaced, 2).unwrap();
    x.refold();
    let mut c2b = c2_at(&r.d.join("b"), b, 7);
    c2b.canonical(&block_of(HEAD + 1, 0xb2, 0xb1, Vec::new()));
    x.funds = Some(Funds {
        balance: U256::from(995_000u64),
        block: HEAD + 1,
        hash: [0xb2; 32],
    });
    assert_eq!(x.available(Some(&c2b)), Some(U256::from(995_000u64)));
    assert_eq!(
        crate::sources::spent_by_grant(&r.state, generation).unwrap(),
        12_000
    );
    assert_eq!(crate::sources::admitted_intents(&r.state).unwrap().len(), 2);
    x.shutdown();
    r.clean();
}

/// The wall clock of [`a_stalled_canonical_source_releases_nothing`].
static STALL_WALL_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(NOW);

fn stall_wall() -> u64 {
    STALL_WALL_MS.load(std::sync::atomic::Ordering::SeqCst)
}

/// BSC3-036: the canonical source stalls after the arm (no block
/// arrives, as when it is rate limited or cut off) while the clean
/// nonce stands: once its head is older than
/// BSC_EXEC_LIVE_MAX_HEAD_AGE_MS by the wall clock, a further launch is
/// refused STALE_HEAD before the owner (no slot, no spend entry,
/// nothing sent); a fresh head lets the next one go.
#[test]
fn a_stalled_canonical_source_releases_nothing() {
    let r = Rig::new("live-stall", LiveStrategies::C3);
    let mut exec = r.exec.clone();
    exec.exec.live_max_head_age_ms = 3_000;
    STALL_WALL_MS.store(NOW, std::sync::atomic::Ordering::SeqCst);
    let head = |number: u64, h: u8, parent: u8, at_ms: u64| CanonicalEvent::Block {
        number,
        hash: [h; 32],
        parent_hash: [parent; 32],
        timestamp_s: at_ms / 1_000,
        timestamp_ms: at_ms,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: Vec::new(),
    };
    let mut c2 = owner(&r.d, r.buyer);
    c2.canonical(&head(HEAD, 0xb1, 0xb0, NOW - 1_000));
    c2.canonical(&CanonicalEvent::NonceFact {
        sender: r.buyer,
        next_nonce: 5,
        block_number: HEAD,
        block_hash: [0xb1; 32],
    });
    let _ = c2.take_evidence();
    let mut x = LiveRuntime::with_prober(
        r.live.clone(),
        exec,
        &r.root,
        r.state.clone(),
        r.d.join("fences"),
        None,
        funded,
    )
    .in_session("boot-1", stall_wall);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    // The source stalls: 4 s pass and no block comes.
    STALL_WALL_MS.store(NOW + 4_000, std::sync::atomic::Ordering::SeqCst);
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let stalled = x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock);
    assert!(stalled.is_none());
    let refusals = record(&x);
    assert!(
        refusals
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"STALE_HEAD=1"),
        "{:?}",
        refusals.texts(l::REFUSALS)
    );
    // BSC3-076: only the lease's attempt and entry, no transaction.
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    assert!(
        crate::sources::admitted_intents(&r.state)
            .unwrap()
            .iter()
            .all(|a| a.lease)
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    // A fresh block: the next launch goes.
    c2.canonical(&head(HEAD + 1, 0xb2, 0xb1, NOW + 3_500));
    let _ = c2.take_evidence();
    let (hash_b, raw_b, launcher_b) = crate::runtime::exec::tests::launch_n(1);
    c2.pending(hash_b, launcher_b, 4, 0, 20);
    let fresh = x.on_launch(
        SourceId::PublicNode,
        hash_b,
        [0; 20],
        &raw_b,
        20,
        &c2,
        &clock,
    );
    assert!(fresh.is_some(), "{:?}", reason(&x));
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    x.shutdown();
    r.clean();
}

/// BSC3-069: each bound of the granted risk one below what the
/// executor signs with (its exit slippage apart from its BUY's) is the
/// one named; a grant of exits only is not refused for the BUY's
/// bounds, for an exit's it is.
#[test]
fn every_granted_bound_is_held_against_what_the_executor_signs() {
    use crate::sources::{GrantRisk, tests::TEST_RISK};
    let mut x = setup(
        std::path::PathBuf::from("unused.key"),
        Duration::from_secs(1),
    )
    .exec;
    x.exit_slippage_bps = 300;
    let grant = |risk: GrantRisk, exit_only: bool| LiveGrant {
        risk: Some(risk),
        exit_only,
        ..grant_of(NOW + 3_600_000, LiveStrategies::C3)
    };
    assert_eq!(risk_refusal(&grant(TEST_RISK, false), &x), None);
    let below: [(GrantRisk, &str); 6] = [
        (
            GrantRisk {
                max_buy_slippage_bps: 99,
                ..TEST_RISK
            },
            "buy_slippage_bps 100 above 99",
        ),
        (
            GrantRisk {
                max_exit_slippage_bps: 299,
                ..TEST_RISK
            },
            "exit_slippage_bps 300 above 299",
        ),
        (
            GrantRisk {
                max_gas_price_wei: 49_999_999,
                ..TEST_RISK
            },
            "gas_price_wei 50000000 above 49999999",
        ),
        (
            GrantRisk {
                max_buy_gas_limit: 2_999_999,
                ..TEST_RISK
            },
            "buy_gas_limit 3000000 above 2999999",
        ),
        (
            GrantRisk {
                max_approve_gas_limit: 99_999,
                ..TEST_RISK
            },
            "approve_gas_limit 100000 above 99999",
        ),
        (
            GrantRisk {
                max_sell_gas_limit: 1_499_999,
                ..TEST_RISK
            },
            "sell_gas_limit 1500000 above 1499999",
        ),
    ];
    for (risk, named) in below {
        assert_eq!(
            risk_refusal(&grant(risk, false), &x),
            Some(format!(
                "{}: {named}",
                crate::sources::LIVE_RISK_OUTSIDE_GRANT
            ))
        );
    }
    let no_buys = GrantRisk {
        max_buy_slippage_bps: 0,
        max_buy_gas_limit: 21_000,
        ..TEST_RISK
    };
    assert_eq!(risk_refusal(&grant(no_buys, true), &x), None);
    assert!(risk_refusal(&grant(no_buys, false), &x).is_some());
    assert!(
        risk_refusal(
            &grant(
                GrantRisk {
                    max_exit_slippage_bps: 299,
                    ..no_buys
                },
                true
            ),
            &x
        )
        .is_some_and(|why| why.ends_with("exit_slippage_bps 300 above 299"))
    );
    let none = LiveGrant {
        risk: None,
        ..grant(TEST_RISK, false)
    };
    assert_eq!(
        risk_refusal(&none, &x),
        Some(crate::sources::LIVE_GRANT_RISK_MISSING.into())
    );
}

/// BSC3-069: an executor configured beyond the execution risk the
/// grant names does not arm (its key is not loaded) and says which
/// bound; under a grant whose risk admits it, it arms.
#[test]
fn an_executor_beyond_the_granted_risk_does_not_arm() {
    use crate::sources::{GrantRisk, tests::TEST_RISK};
    let r = Rig::new("live-risk-arm", LiveStrategies::C3);
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let grant = |risk: GrantRisk| LiveGrant {
        max_input_wei: 10_000_000_000_000_000,
        max_spend_wei: 100_000_000_000_000_000,
        adapters: vec![LiveAdapter::Club48BuilderV1],
        risk: Some(risk),
        ..grant_of(NOW + 3_600_000, LiveStrategies::C3)
    };
    let tight = grant(GrantRisk {
        max_buy_slippage_bps: 50,
        ..TEST_RISK
    });
    let g = s.current().unwrap().generation;
    let receipt = s
        .grant_live("tight", tight.clone(), tight.scope_sha256(), g, NOW)
        .unwrap();
    assert!(receipt.applied(), "{}", receipt.line());
    // The risk decided for the tight scope (BSC3-049).
    let requalify = |g: &LiveGrant| {
        let spec = rig_spec_for(
            Some(crate::live_qualification::running_build().unwrap()),
            g.risk_scope_sha256(),
        );
        fs::write(
            r.root.join(LIVE_QUALIFICATION_PATH),
            build(&r.root, &spec, 1).unwrap(),
        )
        .unwrap();
    };
    requalify(&tight);
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    let (state, why) = reason(&x);
    assert!(!x.armed());
    assert_eq!(state, "BLOCKED", "{why:?}");
    assert_eq!(
        why.as_deref(),
        Some("LIVE_RISK_OUTSIDE_GRANT: buy_slippage_bps 100 above 50")
    );
    let fits = grant(TEST_RISK);
    let g = s.current().unwrap().generation;
    assert!(
        s.grant_live("fits", fits.clone(), fits.scope_sha256(), g, NOW)
            .unwrap()
            .applied()
    );
    // BSC3-049: the risk on file was decided for the tight scope.
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(
        reason(&x),
        (
            "BLOCKED".to_string(),
            Some(crate::sources::LIVE_RISK_DECISION_OTHER_SCOPE.to_string())
        )
    );
    requalify(&fits);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    x.shutdown();
    r.clean();
}

/// BSC3-001 (the authorization matrix of a process of the owner's own
/// account, without any presence artifact): the owner applies its
/// switch, limit and grant requests on the generation CAS alone (no
/// second trust anchor: the register's open decision), so the gate
/// comes clear; the LIVE path still arms only with the buyer's key
/// file and a verified qualification, and SEND off ends it.
#[test]
fn a_same_account_process_clears_the_gate_but_not_the_key_or_the_evidence() {
    use crate::sources::SwitchId;
    // Every switch, both limits and the grant: requests of this very
    // process, applied with no presence asked.
    let r = Rig::new("live-authz", LiveStrategies::C3);
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let q = crate::live_qualification::view(&r.root).0.view;
    assert!(s.live_now(&[ComponentId::ExecC3], &q, NOW).is_ok());
    let c2 = c2_at(&r.d, r.buyer, 5);
    // Without the buyer's key file (on the stand-in route its test
    // identity, BSC3-026): no arm.
    let identity = crate::live_key::loopback_identity(&r.state);
    let aside = r.d.join("identity.aside");
    fs::rename(&identity, &aside).unwrap();
    let mut x = LiveRuntime::with_prober(
        r.live.clone(),
        r.exec.clone(),
        &r.root,
        r.state.clone(),
        r.d.join("fences"),
        None,
        funded,
    )
    .in_session("boot-1", fixed);
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(reason(&x).0, "KEY_REFUSED", "{:?}", reason(&x));
    x.shutdown();
    fs::rename(&aside, &identity).unwrap();
    // Without a verified qualification: no arm.
    let record = r.root.join(LIVE_QUALIFICATION_PATH);
    let aside = r.root.join("qualification.aside");
    fs::rename(&record, &aside).unwrap();
    let mut y = r.runtime("boot-1");
    settle(&mut y, &c2);
    assert!(!y.armed());
    let why = reason(&y).1.unwrap_or_default();
    assert!(why.contains("UNQUALIFIED"), "{why}");
    y.shutdown();
    fs::rename(&aside, &record).unwrap();
    // All of it: it arms; SEND off (a request of the same process)
    // ends it at the next pass.
    let mut z = r.runtime("boot-1");
    settle(&mut z, &c2);
    assert!(z.armed(), "{:?}", reason(&z));
    let g = s.current().unwrap().generation;
    assert!(
        s.request(
            "send-off",
            Entity::Switch(SwitchId::Send),
            Mode::Disabled,
            g,
            NOW
        )
        .unwrap()
        .applied()
    );
    settle(&mut z, &c2);
    assert!(!z.armed(), "{:?}", reason(&z));
    z.shutdown();
    r.clean();
}

/// BSC3-026: the stand-in signs only with its owner state's test
/// identity. A key file anywhere else (even one of the same key) is
/// refused before it is read; the identity is never a production
/// account; with it the path arms.
#[test]
fn the_stand_in_signs_only_with_its_test_identity() {
    use std::os::unix::fs::PermissionsExt;
    let r = Rig::new("live-test-identity", LiveStrategies::C3);
    let c2 = c2_at(&r.d, r.buyer, 5);
    let refused = |live: LiveSetup, exec: ExecSetup| {
        let mut x = LiveRuntime::with_prober(
            live,
            exec,
            &r.root,
            r.state.clone(),
            r.d.join("fences"),
            None,
            funded,
        )
        .in_session("boot-1", fixed);
        settle(&mut x, &c2);
        assert!(!x.armed());
        let (state, why) = reason(&x);
        x.shutdown();
        assert_eq!(state, "KEY_REFUSED", "{why:?}");
        why.unwrap_or_default()
    };
    // The same key in a private file outside the state: refused.
    let (elsewhere, buyer) = crate::live_end::tests::key_file(&r.d);
    assert_eq!(buyer, r.buyer);
    let other = LiveSetup {
        key_path: elsewhere.clone(),
        ..r.live.clone()
    };
    let why = refused(other.clone(), r.exec.clone());
    assert!(why.starts_with("LOOPBACK_KEY_NOT_TEST_IDENTITY"), "{why}");
    // Refused before it is read: an unreadable file says the same.
    fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o000)).unwrap();
    let why = refused(other, r.exec.clone());
    assert!(why.starts_with("LOOPBACK_KEY_NOT_TEST_IDENTITY"), "{why}");
    // BSC3-026 review: a link at the identity's name to another key file
    // (named as the key, or the identity's name itself): refused.
    fs::set_permissions(&elsewhere, fs::Permissions::from_mode(0o600)).unwrap();
    let identity = crate::live_key::loopback_identity(&r.state);
    let aside = r.d.join("identity.aside");
    fs::rename(&identity, &aside).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &identity).unwrap();
    for key_path in [elsewhere.clone(), identity.clone()] {
        let linked = LiveSetup {
            key_path,
            ..r.live.clone()
        };
        let why = refused(linked, r.exec.clone());
        assert!(why.starts_with("LOOPBACK_KEY_NOT_TEST_IDENTITY"), "{why}");
    }
    fs::remove_file(&identity).unwrap();
    fs::rename(&aside, &identity).unwrap();
    // The test identity of a production account: refused.
    let mut production = r.exec.clone();
    production.refused_accounts = vec![[0x3e; 20], r.buyer];
    let why = refused(r.live.clone(), production);
    assert!(why.contains("refused account"), "{why}");
    // The state's test identity: it arms.
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    x.shutdown();
    r.clean();
}

/// The fresh attempts a real-clock start-limit test has (CI 31.10.: a
/// shared or throttled CPU can delay the transport thread's wake-up
/// past the 1 ms by itself). The path holds the limit when one of them
/// starts in time; before BSC3-076 none could: the owner's durable
/// admission alone took 9-12 ms inside it.
const REAL_CLOCK_ATTEMPTS: usize = 5;

/// BSC3-076: on the real monotonic clock a launch's LIVE BUY starts its
/// transport within the 1 ms after its ingress (`BSC_EXEC_START_LIMIT_NS`):
/// nothing durable is left between the ingress and the first byte (the
/// owner admitted it before). The limit holds in an optimized build only.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "the 1 ms transport start limit needs a release build"
)]
fn a_live_buy_starts_within_its_deadline_on_the_real_clock() {
    let mut missed = Vec::new();
    for attempt in 0..REAL_CLOCK_ATTEMPTS {
        let r = Rig::new(&format!("live-real-clock-{attempt}"), LiveStrategies::C3);
        let mut x = r.runtime("boot-1");
        let mut c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let base = Instant::now();
        let clock = FnClock {
            id: 1,
            now: || 1_000_000_000 + base.elapsed().as_nanos() as u64,
        };
        let (hash, raw, launcher) = launch();
        let at = clock.now_ns();
        c2.pending(hash, launcher, 3, 0, at);
        let buy = x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, at, &c2, &clock);
        let sent = buy.is_some() && r.requests.recv_timeout(Duration::from_secs(2)).is_ok();
        if !sent {
            missed.push(format!("{:?}", record(&x).texts(l::REFUSALS)));
        }
        x.shutdown();
        r.clean();
        if sent {
            return;
        }
    }
    panic!("no attempt started within the limit: {missed:?}");
}

/// BSC3-076: on the real monotonic clock a block-driven intent (here
/// Strategy C's BUY) is admitted by the owner, durably, and then starts
/// its transport within the limit. Release build only.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "the 1 ms transport start limit needs a release build"
)]
fn a_strategy_buy_starts_within_its_deadline_after_its_admission() {
    use crate::paper_exec::StrategyCurve;
    use crate::runtime_strategy::StrategyAction;
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
    };
    let mut missed = Vec::new();
    for attempt in 0..REAL_CLOCK_ATTEMPTS {
        let r = Rig::new(
            &format!("live-real-clock-sc-{attempt}"),
            LiveStrategies::StrategyC,
        );
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let base = Instant::now();
        let clock = FnClock {
            id: 1,
            now: || 1_000_000_000 + base.elapsed().as_nanos() as u64,
        };
        let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
        let fresh = quote_native_buy(&NativeBuyInputs {
            token: terms,
            reserve_before: U256::ZERO,
            circulating_before: U256::ZERO,
            input_amount: U256::from(1_000_000_000_000_000_000u128),
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: true,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
        .unwrap();
        let buy = StrategyAction::Buy {
            token: [0xAB; 20],
            creator: [0; 20],
            curve: StrategyCurve {
                terms,
                reserve: fresh.reserve_after,
                circulating: fresh.supply_after,
            },
            input_wei: U256::from(10_000_000_000_000_000u64),
            decided_ns: clock.now_ns(),
        };
        // The decision's wall clock: half a second after C2's head.
        let now_ms = 1_793_000_000_500;
        let (intents, refused) = x.on_strategy(&buy, &c2, &clock, now_ms).unwrap();
        let sent = (intents.len(), refused) == (1, None)
            && r.requests.recv_timeout(Duration::from_secs(2)).is_ok();
        if !sent {
            missed.push(format!("{:?}", record(&x).texts(l::REFUSALS)));
        }
        x.shutdown();
        r.clean();
        if sent {
            return;
        }
    }
    panic!("no attempt started within the limit: {missed:?}");
}

fn leases(state: &Path) -> Vec<crate::sources::Admitted> {
    crate::sources::admitted_intents(state)
        .unwrap()
        .into_iter()
        .filter(|a| a.lease)
        .collect()
}

/// BSC3-076: the owner's state changes between the lease and the launch
/// (here an unrelated source switched, as a revoke or a switch off
/// would): the launch's BUY is refused before anything is taken or sent
/// (BSC3-010: the owner as it holds at the hand-over decides); the next
/// pass leases again under the same attempt and the next launch goes.
#[test]
fn a_c3_buy_after_an_owner_change_waits_for_its_lease_again() {
    let r = Rig::new("live-lease-mark", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let held = leases(&r.state);
    assert_eq!((held.len(), held[0].nonce), (1, Some(5)));
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    // Passes with nothing changed keep it (no write).
    settle(&mut x, &c2);
    settle(&mut x, &c2);
    assert_eq!(leases(&r.state), held);
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    let applied = s
        .request("drpc", Entity::Source(SourceId::Drpc), Mode::Shadow, g, NOW)
        .unwrap();
    assert!(applied.applied(), "{}", applied.line());
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_none()
    );
    let why = x.executor.as_ref().unwrap().end().last_refusal().unwrap();
    assert!(
        why.starts_with(crate::sources::LIVE_LEASE_STALE) && why.contains("owner"),
        "{why}"
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    // The next pass: a new lease, the old one back, one attempt.
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let again = leases(&r.state);
    assert_eq!(again.len(), 1);
    assert_ne!(again[0].tx_hash, held[0].tx_hash);
    let (hash_b, raw_b, launcher_b) = crate::runtime::exec::tests::launch_n(1);
    c2.pending(hash_b, launcher_b, 4, 0, 20);
    assert!(
        x.on_launch(
            SourceId::PublicNode,
            hash_b,
            [0; 20],
            &raw_b,
            20,
            &c2,
            &clock
        )
        .is_some(),
        "{:?}",
        x.executor.as_ref().unwrap().end().last_refusal()
    );
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    x.shutdown();
    r.clean();
}

/// BSC3-076: Strategy C's BUY at the nonce the C3 lease holds takes it
/// back first, unused: the lease then spends and recovers as nothing
/// (the strategy BUY's spend alone); its attempt stays its one slot,
/// for the next lease. An arm that ends gives its lease back too.
#[test]
fn the_c3_lease_yields_its_nonce_and_goes_back_with_the_arm() {
    use crate::paper_exec::StrategyCurve;
    use crate::runtime_strategy::StrategyAction;
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
    };
    let r = Rig::new("live-lease-yield", LiveStrategies::Both);
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let held = leases(&r.state);
    assert_eq!((held.len(), held[0].nonce), (1, Some(5)));
    let generation = x
        .executor
        .as_ref()
        .unwrap()
        .end()
        .armed()
        .grant_generation();
    let spent = |s: &Path| crate::sources::spent_by_grant(s, generation).unwrap();
    assert_eq!(spent(&r.state), held[0].max_wei);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let fresh = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(1_000_000_000_000_000_000u128),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let buy = StrategyAction::Buy {
        token: [0xAB; 20],
        creator: [0; 20],
        curve: StrategyCurve {
            terms,
            reserve: fresh.reserve_after,
            circulating: fresh.supply_after,
        },
        input_wei: U256::from(10_000_000_000_000_000u64),
        decided_ns: 0,
    };
    let (intents, refused) = x.on_strategy(&buy, &c2, &clock, 1_793_000_000_500).unwrap();
    assert_eq!((intents.len(), refused), (1, None));
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    assert_eq!(
        darkbot_bsc_trading::transactions::legacy::decode_signed(&intents[0].raw)
            .unwrap()
            .tx
            .nonce,
        5
    );
    // The lease went back before: no spend, no recovery of its own.
    assert!(leases(&r.state).is_empty());
    let admitted = crate::sources::admitted_intents(&r.state).unwrap();
    assert_eq!(admitted.len(), 1);
    assert_eq!(spent(&r.state), admitted[0].max_wei);
    // Its attempt is the lease's slot; the strategy BUY took its own.
    assert_eq!(
        crate::sources::total_usage(
            &r.state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        2
    );
    // The arm ends (revoke): a lease it held goes back with it.
    settle(&mut x, &c2);
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    assert!(
        s.request("revoke", Entity::LiveGrant, Mode::Disabled, g, NOW)
            .unwrap()
            .applied()
    );
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert!(leases(&r.state).is_empty());
    assert_eq!(
        crate::sources::total_usage(
            &r.state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        2
    );
    x.shutdown();
    r.clean();
}

/// BSC3-076: a lease is renewed before half its window is gone: a new
/// entry under the same attempt (no second slot), the old one back; the
/// launch's BUY never takes a lease whose window ends before its own.
#[test]
fn the_c3_lease_is_renewed_before_its_window_runs_out() {
    let mut r = Rig::new("live-lease-renew", LiveStrategies::C3);
    r.exec.exec.live_lease_blocks = 4;
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let first = leases(&r.state);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].max_block, Some(HEAD + 4));
    // One block on: HEAD + 1 + 2 (its window) + 2 (half) > HEAD + 4.
    c2.canonical(&block_of(HEAD + 1, 0xb2, 0xb1, Vec::new()));
    let _ = c2.take_evidence();
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let renewed = leases(&r.state);
    assert_eq!(renewed.len(), 1);
    assert_ne!(renewed[0].tx_hash, first[0].tx_hash);
    assert_eq!(renewed[0].max_block, Some(HEAD + 1 + 4));
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    // The renewed one moved whole out of the spend (it grows not).
    let name = format!("{}.dbrec", alloy_primitives::hex::encode(first[0].tx_hash));
    assert!(r.state.join("live-lease-returns").join(&name).exists());
    assert!(!r.state.join("live-spend").join(&name).exists());
    x.shutdown();
    r.clean();
}

/// BSC3-076: an arm that ends gives its lease back (the gate closes, a
/// revoke here): no spend, no recovery of it; its attempt's slot stays
/// taken, the next lease of the same grant takes no other.
#[test]
fn an_arm_that_ends_returns_its_lease() {
    let r = Rig::new("live-lease-end", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(leases(&r.state).len(), 1);
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    assert!(
        s.request(
            "send-off",
            Entity::Switch(crate::sources::SwitchId::Send),
            Mode::Disabled,
            g,
            NOW
        )
        .unwrap()
        .applied()
    );
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert!(leases(&r.state).is_empty());
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    x.shutdown();
    r.clean();
}

/// BSC3-076: a launch's BUY whose bundle window would end after the
/// lease's takes nothing from it (a lease not renewed in time): refused
/// before anything leaves.
#[test]
fn a_c3_buy_never_outlasts_its_lease() {
    let mut r = Rig::new("live-lease-window", LiveStrategies::C3);
    r.exec.exec.live_lease_blocks = 4;
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(leases(&r.state)[0].max_block, Some(HEAD + 4));
    // Three blocks on, no pass in between: its window HEAD + 3 + 2.
    for (n, h) in [(1u64, 0xb2u8), (2, 0xb3), (3, 0xb4)] {
        c2.canonical(&block_of(HEAD + n, h, h - 1, Vec::new()));
    }
    let _ = c2.take_evidence();
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_none()
    );
    let why = x.executor.as_ref().unwrap().end().last_refusal().unwrap();
    assert!(why.contains("window"), "{why}");
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    x.shutdown();
    r.clean();
}

/// BSC3-076 review H1: a grant of one BUY attempt: the arm's lease takes
/// that last slot and the arm stays (its own attempt refuses nothing);
/// the launch's BUY goes under it.
#[test]
fn a_lease_of_the_last_attempt_keeps_the_arm_and_buys() {
    let r = Rig::granting("live-lease-one", LiveStrategies::C3, |now| LiveGrant {
        max_attempts: 1,
        ..rig_grant(now + 3_600_000, LiveStrategies::C3)
    });
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(leases(&r.state).len(), 1);
    settle(&mut x, &c2);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    // Re-review HIGH: the runtime's reconcile reads the gate as this arm
    // does: the venue, the runtime, C2, C3 and the source stay LIVE.
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap();
    let q = crate::live_qualification::view(&r.root).0.view;
    let view = crate::runtime::LiveView {
        boot: Some("boot-1".into()),
        ..crate::runtime::LiveView::of(
            &g,
            &r.state,
            Some(&q),
            NOW,
            Some(&x),
            crate::sources::tests::max(),
        )
    };
    for e in [
        Entity::Runtime,
        crate::sources::VenueId::Flap.into(),
        ComponentId::StateC2.into(),
        ComponentId::ExecC3.into(),
        SourceId::PublicNode.into(),
    ] {
        assert_eq!(
            view.effective(&g, e, false),
            crate::sources::Effective::Live,
            "{e:?}"
        );
    }
    // BSC3-078: the status names the held attempt, for the tabs.
    let held = x
        .executor
        .as_ref()
        .unwrap()
        .end()
        .lease()
        .unwrap()
        .attempt();
    assert_eq!(record(&x).opt_b256(l::LEASE_ATTEMPT).unwrap(), Some(held));
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_some(),
        "{:?}",
        x.executor.as_ref().unwrap().end().last_refusal()
    );
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    assert_eq!(
        crate::sources::total_usage(
            &r.state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    x.shutdown();
    r.clean();
}

/// BSC3-076 review H3: after a crash with only a lease held, a
/// transaction of the buyer's key at the lease's nonce that is no BUY
/// of it (no Portal BUY of a token by the buyer, or after its window)
/// is never adopted: the lease is replaced by it, counted for the
/// operator, no position.
#[test]
fn only_the_leases_own_buy_is_adopted() {
    // (sell, token, trader is the buyer, eth): the canonical BUY, and
    // each condition broken alone.
    let buy = (false, [0xAB; 20], true, U256::from(1_000u64));
    for (tag, trade, past_window) in [
        ("live-adopt-none", None, false),
        ("live-adopt-late", Some(buy), true),
        ("live-adopt-sell", Some((true, buy.1, buy.2, buy.3)), false),
        (
            "live-adopt-zero",
            Some((buy.0, [0; 20], buy.2, buy.3)),
            false,
        ),
        (
            "live-adopt-trader",
            Some((buy.0, buy.1, false, buy.3)),
            false,
        ),
        (
            "live-adopt-over",
            Some((buy.0, buy.1, buy.2, U256::MAX)),
            false,
        ),
    ] {
        let mut r = Rig::new(tag, LiveStrategies::C3);
        r.exec.exec.live_lease_blocks = 4;
        let mut x = r.runtime("boot-1");
        let c2 = c2_at(&r.d, r.buyer, 5);
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let held = leases(&r.state);
        assert_eq!(held.len(), 1);
        std::mem::forget(x);
        r.authorize("boot-2", "after-crash");
        let mut x = r.runtime("boot-2");
        assert_eq!(x.book().unresolved().count(), 1);
        let mut c2 = c2_at(&r.d.join("c2-restarted"), r.buyer, 5);
        let at = if past_window { HEAD + 10 } else { HEAD + 1 };
        let tx = [0x5e; 32];
        let mut events = vec![block_of(
            at,
            0xd2,
            0xd1,
            vec![CanonicalTx {
                hash: tx,
                index: 0,
                sender: r.buyer,
                nonce: 5,
                succeeded: true,
                cost: None,
            }],
        )];
        if let Some((sell, token, by_buyer, eth)) = trade {
            events.push(CanonicalEvent::FlapTrade(
                crate::chain_follower::CanonicalTrade {
                    block_number: at,
                    block_hash: [0xd2; 32],
                    tx_index: 0,
                    tx_hash: tx,
                    log_index: 0,
                    origin: r.buyer,
                    sell,
                    token,
                    trader: if by_buyer { r.buyer } else { [0x77; 20] },
                    amount: U256::from(1_000u64).to_be_bytes(),
                    eth: eth.to_be_bytes(),
                    fee: [0; 32],
                    post_price: [0; 32],
                },
            ));
        }
        events.push(complete_of(at, 0xd2));
        let changes = feed(&mut x, &mut c2, events);
        assert!(changes.is_empty(), "{tag}: {changes:?}");
        assert!(x.book().tx(&tx).is_none(), "{tag}");
        assert_eq!(
            x.book().tx(&held[0].tx_hash).unwrap().state,
            FactKind::Replaced,
            "{tag}"
        );
        assert!(
            x.book()
                .positions
                .iter()
                .all(|p| p.state != PositionState::Open),
            "{tag}"
        );
        assert!(refused_by(&x, "LEASE_NONCE_TAKEN_BY_OTHER"), "{tag}");
        x.shutdown();
        r.clean();
    }
}

fn refused_by(x: &LiveRuntime, why: &str) -> bool {
    record(x)
        .texts(l::REFUSALS)
        .unwrap()
        .iter()
        .any(|r| r.starts_with(&format!("{why}=")))
}

/// Strategy C's BUY of `token` on a fresh curve (the tests' economics).
fn fresh_buy(token: [u8; 20]) -> crate::runtime_strategy::StrategyAction {
    use crate::paper_exec::StrategyCurve;
    use darkbot_bsc_trading::economics::flap::native_curve20::{
        HoldingCap, NativeBuyInputs, PricedToken, profile_terms, quote_native_buy,
    };
    let terms = PricedToken::new(&profile_terms(100, 100)).unwrap();
    let fresh = quote_native_buy(&NativeBuyInputs {
        token: terms,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount: U256::from(1_000_000_000_000_000_000u128),
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    crate::runtime_strategy::StrategyAction::Buy {
        token,
        creator: [0; 20],
        curve: StrategyCurve {
            terms,
            reserve: fresh.reserve_after,
            circulating: fresh.supply_after,
        },
        input_wei: U256::from(10_000_000_000_000_000u64),
        decided_ns: 0,
    }
}

/// The ledger facts of one closed position of `token` bought under the
/// grant written at `generation`: BUY and SELL included, the SELL's
/// block produced at `closed_ms`, proceeds `eth` for an input `input`.
fn closed_position(
    state: &Path,
    generation: u64,
    token: [u8; 20],
    (buy, sell): ([u8; 32], [u8; 32]),
    (input, eth): (u128, u128),
    closed_ms: u64,
) {
    let ledger = Ledger::open(state);
    let released = |tx, intent, nonce| Fact {
        kind: FactKind::Released,
        tx_hash: tx,
        intent,
        component: ComponentId::StrategyC,
        token,
        nonce,
        grant_generation: generation,
        input_wei: Some(U256::from(input)),
        max_wei: Some(U256::from(input)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD - 10),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    let b = released(buy, LiveIntentKind::Buy, 1);
    let s = released(sell, LiveIntentKind::Sell, 2);
    for f in [
        b.clone(),
        Fact {
            block: Some((HEAD - 20, [0x91; 32], closed_ms - 60_000)),
            gas_wei: Some(U256::ZERO),
            tokens: Some(U256::from(1_000u64)),
            eth_wei: Some(U256::from(input)),
            ..Fact::about(&b, FactKind::Included)
        },
        s.clone(),
        Fact {
            block: Some((HEAD - 19, [0x92; 32], closed_ms)),
            gas_wei: Some(U256::ZERO),
            eth_wei: Some(U256::from(eth)),
            ..Fact::about(&s, FactKind::Included)
        },
    ] {
        ledger.append(&f, NOW).unwrap();
    }
}

fn granted_generation(state: &Path) -> u64 {
    Store::open(state, crate::sources::tests::keeping(8))
        .unwrap()
        .current()
        .unwrap()
        .live_set_by
        .unwrap()
        .generation
}

fn limited(tag: &str, limits: crate::sources::GrantLimits) -> Rig {
    Rig::granting(tag, LiveStrategies::Both, move |now| LiveGrant {
        limits: Some(limits),
        ..rig_grant(now + 3_600_000, LiveStrategies::Both)
    })
}

fn refused_for(x: &LiveRuntime, why: &str) -> bool {
    record(x)
        .texts(l::REFUSALS)
        .unwrap()
        .iter()
        .any(|r| r.starts_with(&format!("{why}=")))
}

fn a_clock() -> FnClock<impl Fn() -> u64> {
    let t = Cell::new(10u64);
    FnClock {
        id: 1,
        now: move || {
            t.set(t.get() + 3);
            t.get()
        },
    }
}

/// The decision's wall clock: half a second after C2's head.
const DECIDED_MS: u64 = 1_793_000_000_500;

/// 31.10. order: as many positions open as the grant allows (a BUY
/// pending counts): the next BUY is refused, counted with its reason,
/// nothing sent.
#[test]
fn a_buy_beyond_the_granted_open_positions_is_refused() {
    use crate::sources::{GrantLimits, LIVE_OPEN_POSITIONS_MAX};
    let r = limited(
        "live-limit-open",
        GrantLimits {
            max_open_positions: 1,
            ..crate::sources::tests::TEST_LIMITS
        },
    );
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (1, None));
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA2; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (0, Some(Refusal::GrantLimit)));
    assert!(refused_for(&x, LIVE_OPEN_POSITIONS_MAX));
    assert!(r.requests.recv_timeout(Duration::from_millis(200)).is_err());
    x.shutdown();
    r.clean();
}

/// Review of the limits (LOW): a position open on chain (its BUY
/// included, no SELL) counts against the bound, as a pending one does.
#[test]
fn an_open_position_on_chain_counts_against_the_bound() {
    use crate::sources::{GrantLimits, LIVE_OPEN_POSITIONS_MAX};
    let r = limited(
        "live-limit-open-chain",
        GrantLimits {
            max_open_positions: 1,
            ..crate::sources::tests::TEST_LIMITS
        },
    );
    let g = granted_generation(&r.state);
    let buy = Fact {
        kind: FactKind::Released,
        tx_hash: [0xe1; 32],
        intent: LiveIntentKind::Buy,
        component: ComponentId::StrategyC,
        token: [0xE1; 20],
        nonce: 1,
        grant_generation: g,
        input_wei: Some(U256::from(10_000u64)),
        max_wei: Some(U256::from(10_000u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD - 10),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    let ledger = Ledger::open(&r.state);
    ledger.append(&buy, NOW).unwrap();
    let included = Fact {
        block: Some((HEAD - 20, [0x91; 32], NOW - 60_000)),
        gas_wei: Some(U256::ZERO),
        tokens: Some(U256::from(1_000u64)),
        eth_wei: Some(U256::from(10_000u64)),
        ..Fact::about(&buy, FactKind::Included)
    };
    ledger.append(&included, NOW).unwrap();
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert!(
        x.book()
            .positions
            .iter()
            .any(|p| p.state == PositionState::Open)
    );
    let clock = a_clock();
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (0, Some(Refusal::GrantLimit)));
    assert!(
        refused_for(&x, LIVE_OPEN_POSITIONS_MAX),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    x.shutdown();
    r.clean();
}

/// Review of the limits (LOW): the C3 path holds the limits too: at a
/// realized loss at its bound no lease is taken and the launch is
/// refused by the limit, nothing leaves.
#[test]
fn a_c3_launch_is_refused_at_the_granted_loss() {
    use crate::sources::{GrantLimits, LIVE_REALIZED_LOSS_MAX};
    let r = limited(
        "live-limit-c3-loss",
        GrantLimits {
            max_realized_loss_wei: 3_000,
            max_daily_realized_loss_wei: 3_000,
            ..crate::sources::tests::TEST_LIMITS
        },
    );
    // 4000 lost under the grant.
    let g = granted_generation(&r.state);
    closed_position(
        &r.state,
        g,
        [0xB1; 20],
        ([0xc1; 32], [0xc2; 32]),
        (10_000, 6_000),
        NOW,
    );
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(leases(&r.state).is_empty());
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_none()
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    assert!(
        refused_for(&x, LIVE_REALIZED_LOSS_MAX),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    x.shutdown();
    r.clean();
}

/// BSC3-056 (review LOW-6): the C3 BUY on a launch takes the same gate:
/// while the second source does not confirm the chain nothing is sent;
/// once it does, the next launch's BUY goes.
#[test]
fn a_c3_launch_waits_for_the_second_source_too() {
    use crate::canonical_confirm::{Confirmation, Verdict};
    let r = limited("live-confirm-c3", crate::sources::tests::TEST_LIMITS);
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    x.set_confirmation(Some(Confirmation {
        verdict: Verdict::Disagrees { height: HEAD },
        max_lag_blocks: 2,
    }));
    settle(&mut x, &c2);
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_none()
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    assert!(
        refused_for(&x, "UNCONFIRMED"),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    x.set_confirmation(Some(Confirmation {
        verdict: Verdict::Agreed { height: HEAD },
        max_lag_blocks: 2,
    }));
    let (hash_b, raw_b, launcher_b) = crate::runtime::exec::tests::launch_n(1);
    c2.pending(hash_b, launcher_b, 4, 0, 20);
    assert!(
        x.on_launch(
            SourceId::PublicNode,
            hash_b,
            [0; 20],
            &raw_b,
            20,
            &c2,
            &clock
        )
        .is_some(),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    x.shutdown();
    r.clean();
}

/// 31.10. order: the realized net loss of the grant's positions, and of
/// the positions that closed on this UTC day (any grant's), each at its
/// bound refuses a new BUY; a loss of another day counts for the grant
/// only, another grant's for the day only. Read from the ledger when
/// the runtime starts: a restart keeps them.
#[test]
fn the_granted_losses_refuse_buys_from_the_ledger() {
    use crate::sources::{GrantLimits, LIVE_DAILY_LOSS_MAX, LIVE_REALIZED_LOSS_MAX};
    let limits = GrantLimits {
        max_open_positions: 16,
        max_realized_loss_wei: 3_000,
        max_daily_realized_loss_wei: 2_000,
        min_buy_interval_ms: 0,
    };
    // Within both: 2500 lost under the grant, yesterday.
    let r = limited("live-limit-loss", limits);
    let g = granted_generation(&r.state);
    let yesterday = NOW - 86_400_000;
    closed_position(
        &r.state,
        g,
        [0xB1; 20],
        ([0xc1; 32], [0xc2; 32]),
        (10_000, 7_500),
        yesterday,
    );
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (1, None));
    x.shutdown();
    r.clean();
    // Then also 1200 lost under another grant today and 900 under this
    // one: the grant's 3400 is at its bound (the day's 2100 too: the
    // grant's is named first).
    let r = limited("live-limit-loss-2", limits);
    let g = granted_generation(&r.state);
    closed_position(
        &r.state,
        g,
        [0xB1; 20],
        ([0xc1; 32], [0xc2; 32]),
        (10_000, 7_500),
        yesterday,
    );
    closed_position(
        &r.state,
        g - 1,
        [0xB2; 20],
        ([0xc3; 32], [0xc4; 32]),
        (10_000, 8_800),
        NOW,
    );
    closed_position(
        &r.state,
        g,
        [0xB3; 20],
        ([0xc5; 32], [0xc6; 32]),
        (10_000, 9_100),
        NOW,
    );
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA2; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (0, Some(Refusal::GrantLimit)));
    assert!(
        refused_for(&x, LIVE_REALIZED_LOSS_MAX),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    x.shutdown();
    r.clean();
    // The day's alone: 1200 under another grant and 900 under this one,
    // both today, the grant's bound far.
    let r = limited(
        "live-limit-day",
        GrantLimits {
            max_realized_loss_wei: 1_000_000,
            ..limits
        },
    );
    let g = granted_generation(&r.state);
    closed_position(
        &r.state,
        g - 1,
        [0xB2; 20],
        ([0xc3; 32], [0xc4; 32]),
        (10_000, 8_800),
        NOW,
    );
    closed_position(
        &r.state,
        g,
        [0xB3; 20],
        ([0xc5; 32], [0xc6; 32]),
        (10_000, 9_100),
        NOW,
    );
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA2; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (0, Some(Refusal::GrantLimit)));
    assert!(
        refused_for(&x, LIVE_DAILY_LOSS_MAX),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    x.shutdown();
    r.clean();
}

/// 31.10. order: the least time between two BUYs: from the last BUY
/// opened on chain (the ledger's), or handed over here (one still
/// pending from before this process: at the latest when it began).
#[test]
fn a_buy_within_the_granted_interval_is_refused() {
    use crate::sources::{GrantLimits, LIVE_BUY_INTERVAL};
    let limits = GrantLimits {
        min_buy_interval_ms: 60_000,
        ..crate::sources::tests::TEST_LIMITS
    };
    // The last BUY opened 30 s ago: refused.
    let r = limited("live-limit-gap", limits);
    let g = granted_generation(&r.state);
    closed_position(
        &r.state,
        g,
        [0xB1; 20],
        ([0xc1; 32], [0xc2; 32]),
        (10_000, 10_000),
        NOW + 30_000,
    );
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (0, Some(Refusal::GrantLimit)));
    assert!(refused_for(&x, LIVE_BUY_INTERVAL));
    x.shutdown();
    r.clean();
    // Two minutes ago: it goes; the next at once is refused (this
    // process's last BUY, pending).
    let r = limited("live-limit-gap-2", limits);
    let g = granted_generation(&r.state);
    closed_position(
        &r.state,
        g,
        [0xB1; 20],
        ([0xc1; 32], [0xc2; 32]),
        (10_000, 10_000),
        NOW - 60_000,
    );
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!(
        (out.len(), refusal),
        (1, None),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA2; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (0, Some(Refusal::GrantLimit)));
    x.shutdown();
    r.clean();
}

/// BSC3-056: with a second source a BUY goes only while it confirms the
/// followed chain, else it is refused UNCONFIRMED before the owner (no
/// attempt); the end armed takes the verdict set before it.
#[test]
fn a_buy_waits_for_the_second_source_to_confirm_the_chain() {
    use crate::canonical_confirm::{Confirmation, Verdict};
    let lagging = |verdict| {
        Some(Confirmation {
            verdict,
            max_lag_blocks: 2,
        })
    };
    let r = limited("live-confirm", crate::sources::tests::TEST_LIMITS);
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    x.set_confirmation(lagging(Verdict::Stale { age_ms: None }));
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let unconfirmed = [
        Verdict::Stale { age_ms: None },
        Verdict::Stale {
            age_ms: Some(9_000),
        },
        Verdict::Behind { by: 3 },
        Verdict::Disagrees { height: 9 },
        Verdict::NoCommonHeight,
        // Agreed, but below the head the BUY plans on by more than the
        // lag (review M-1).
        Verdict::Agreed { height: HEAD - 3 },
    ];
    for (i, v) in unconfirmed.into_iter().enumerate() {
        if i > 0 {
            x.set_confirmation(lagging(v));
        }
        let (out, refusal) = x
            .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
            .unwrap();
        assert_eq!(
            (out.len(), refusal),
            (0, Some(Refusal::Unconfirmed)),
            "{v:?}"
        );
    }
    assert!(
        record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .contains(&"UNCONFIRMED=6")
    );
    x.set_confirmation(lagging(Verdict::Agreed { height: HEAD - 2 }));
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!(
        (out.len(), refusal),
        (1, None),
        "{:?}",
        record(&x).texts(l::REFUSALS)
    );
    x.shutdown();
    r.clean();
}

/// REV.1.0 R2: the placement verdicts: no token received NO_TOKENS
/// (first), no launch in the block OTHER_BLOCK, another trader's BUY
/// between LATE, the buyer's own and the creator's in the launch never
/// count, else FIRST.
#[test]
fn a_c3_buys_placement_is_judged_against_its_launch() {
    use crate::live_ledger::PlacementVerdict::*;
    let (token, other, buyer, trader) = ([0xA1; 20], [0xA2; 20], [0xB0; 20], [0x77; 20]);
    let buys = [(token, 2, buyer), (other, 3, trader), (token, 9, trader)];
    let verdict = |launch, own, received| placement_of(launch, own, &buys, token, buyer, received);
    assert_eq!(verdict(Some(1), 4, true).verdict, First);
    assert_eq!(verdict(Some(1), 4, true).buys_before, 0);
    assert_eq!(verdict(Some(1), 10, true).verdict, Late);
    assert_eq!(verdict(Some(1), 10, true).buys_before, 1);
    assert_eq!(verdict(None, 4, true).verdict, OtherBlock);
    assert_eq!(verdict(Some(1), 10, false).verdict, NoTokens);
    assert_eq!(verdict(Some(1), 10, true).launch_index, Some(1));
}

/// REV.1.0 R2 (PHASENPLAN 27.10.): a C3 BUY that landed behind another
/// trader's BUY of its token is a durable LATE placement in the ledger
/// (with its launch's creator): no new BUY under the grant, also after
/// a restart; its lot keeps its exit.
#[test]
fn a_c3_buy_behind_another_traders_buy_stops_new_buys() {
    use crate::live_ledger::PlacementVerdict;
    let r = Rig::new("live-placed-late", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .expect("the BUY");
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    let trader = [0x77; 20];
    let tx = |hash, index, sender, nonce| CanonicalTx {
        hash,
        index,
        sender,
        nonce,
        succeeded: true,
        cost: None,
    };
    let block = CanonicalEvent::Block {
        number: HEAD + 1,
        hash: [0xb2; 32],
        parent_hash: [0xb1; 32],
        timestamp_s: 1_793_000_001,
        timestamp_ms: 1_793_000_001_000,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: vec![
            tx([0x1a; 32], 0, launcher, 3),
            tx([0x2b; 32], 1, trader, 0),
            tx(buy.handoff.tx_hash, 2, r.buyer, 5),
        ],
    };
    c2.canonical(&block);
    let _ = x.apply_evidence(&c2.take_evidence());
    let _ = x.on_canonical(&block);
    let _ = x.on_canonical(&launch_event(HEAD + 1, 0xb2, 0, buy.token, launcher));
    let _ = x.on_canonical(&buy_trade(HEAD + 1, 0xb2, 1, [0x2b; 32], buy.token, trader));
    let _ = x.on_canonical(&buy_trade(
        HEAD + 1,
        0xb2,
        2,
        buy.handoff.tx_hash,
        buy.token,
        r.buyer,
    ));
    let _ = x.on_canonical(&CanonicalEvent::BlockComplete {
        number: HEAD + 1,
        hash: [0xb2; 32],
    });
    let placed = x.book.tx(&buy.handoff.tx_hash).unwrap().placement.unwrap();
    assert_eq!(placed.verdict, PlacementVerdict::Late);
    assert_eq!(
        (placed.launch_index, placed.own_index, placed.buys_before),
        (Some(0), 2, 1)
    );
    assert_eq!(x.launcher_of(&buy.token), Some(launcher));
    let refused = |x: &LiveRuntime| x.buy_refusal(U256::ZERO, Some(&c2), Some(6));
    // Before any other reason: unfunded too, the halt is what it names
    // (review L3).
    assert_eq!(
        x.buy_refusal(U256::MAX, Some(&c2), Some(6)),
        Some((
            Refusal::GrantLimit,
            Some(crate::sources::LIVE_PLACEMENT_VIOLATED)
        ))
    );
    assert_eq!(
        refused(&x),
        Some((
            Refusal::GrantLimit,
            Some(crate::sources::LIVE_PLACEMENT_VIOLATED)
        ))
    );
    // Its lot keeps its exit (exits are no BUY).
    assert!(x.book.open_lot(&buy.token).is_some());
    x.shutdown();
    // A restart reads the placement from the ledger.
    let mut y = r.runtime("boot-1");
    settle(&mut y, &c2);
    assert!(y.armed(), "{:?}", reason(&y));
    assert_eq!(
        refused(&y),
        Some((
            Refusal::GrantLimit,
            Some(crate::sources::LIVE_PLACEMENT_VIOLATED)
        ))
    );
    assert_eq!(y.launcher_of(&buy.token), Some(launcher));
    y.shutdown();
    r.clean();
}

/// REV.1.0 R2: a C3 BUY's release is journaled as its attempt (`l`:
/// the attempt, trigger, clock, frame selection and each adapter's
/// outcome and boundaries, from the dispatch's own report) and
/// evaluated with the ledger and the canonical journal: its placement,
/// acquisition and market window; no producer witness, so its timing
/// is UNKNOWN, and without a manifest the campaign is not ready.
#[test]
fn a_live_release_is_journaled_and_evaluated_as_its_attempt() {
    use crate::live_attempts::{CAMPAIGN_NOT_READY, PASS, UNKNOWN, evaluate};
    use crate::runtime_journal::{
        JournalSetup, RuntimeJournal, live_release_of, live_release_payload,
    };
    use darkbot_bsc_journal::RecordKind;
    let r = Rig::new("live-attempt-record", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let buy = x
        .on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
        .expect("the BUY");
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    let tx = |hash, index, sender, nonce| CanonicalTx {
        hash,
        index,
        sender,
        nonce,
        succeeded: true,
        cost: None,
    };
    let t0_ms = 1_793_000_001_000;
    let block = CanonicalEvent::Block {
        number: HEAD + 1,
        hash: [0xb2; 32],
        parent_hash: [0xb1; 32],
        timestamp_s: t0_ms / 1_000,
        timestamp_ms: t0_ms,
        observed: Instant::now(),
        observed_unix_ms: None,
        txs: vec![
            tx(hash, 0, launcher, 3),
            tx(buy.handoff.tx_hash, 1, r.buyer, 5),
        ],
    };
    c2.canonical(&block);
    let _ = x.apply_evidence(&c2.take_evidence());
    let _ = x.on_canonical(&block);
    let _ = x.on_canonical(&launch_event(HEAD + 1, 0xb2, 0, buy.token, launcher));
    let own = buy_trade(HEAD + 1, 0xb2, 1, buy.handoff.tx_hash, buy.token, r.buyer);
    let _ = x.on_canonical(&own);
    let _ = x.on_canonical(&CanonicalEvent::BlockComplete {
        number: HEAD + 1,
        hash: [0xb2; 32],
    });
    x.shutdown();
    // The runtime's records of it, as `run` journals them.
    let l = live_release_payload(
        0,
        Some(SourceId::PublicNode),
        &buy.handoff,
        buy.launch_hash,
        buy.token,
        0,
        &buy.times,
        &buy.raw,
    );
    let back = live_release_of(&l).unwrap();
    assert_eq!(
        (back.tx_hash, back.attempt, back.trigger),
        (
            buy.handoff.tx_hash,
            buy.handoff.attempt,
            Some(SourceId::PublicNode)
        )
    );
    assert_eq!(back.clock_id, buy.handoff.report.clock_id);
    assert_eq!(
        (back.times[6], back.raw.as_slice()),
        (buy.handoff.report.t_select_ns, buy.raw.as_slice())
    );
    let reported = back
        .adapters
        .iter()
        .map(|a| {
            darkbot_bsc_trading::intent::AdapterId::new(a.adapter)
                .and_then(|id| buy.handoff.report.adapter(id))
                .map(|r| r.t_boundary_ns)
                == Some(a.boundary_ns)
        })
        .collect::<Vec<_>>();
    assert_eq!(reported, vec![true; buy.handoff.adapters as usize]);
    let rundir = r.d.join("rundir");
    let mut j = RuntimeJournal::open(
        &rundir.join("bsc-runtime/journal"),
        t0_ms - 60_000,
        1,
        JournalSetup::whole(1024, 1 << 16),
    )
    .unwrap();
    let mut launch_ref = crate::canonical_join::LaunchRef {
        venue: crate::canonical_join::Venue::Flap,
        block_number: HEAD + 1,
        block_hash: [0xb2; 32],
        tx_index: 0,
        tx_hash: buy.launch_hash,
        token: Some(buy.token),
        external_buys: None,
        c1_scope: true,
        venue_target: true,
        four_token: crate::canonical_join::FourToken::NotApplicable,
        tx: crate::canonical_join::LaunchTx::Verified,
        block_ms: t0_ms,
        block_seen_ns: 0,
        block_seen_unix_ms: None,
    };
    for k in 0..40u64 {
        let mut h = [0xc0; 32];
        h[..8].copy_from_slice(&k.to_le_bytes());
        if k == 0 {
            h = [0xb2; 32];
        }
        assert!(j.record(
            RecordKind::Canonical,
            k,
            crate::runtime_strategy::block_payload(HEAD + 1 + k, &h, t0_ms + k * 450),
        ));
    }
    launch_ref.block_ms = t0_ms;
    assert!(j.record(
        RecordKind::Canonical,
        41,
        crate::runtime_journal::canonical_launch_payload(&crate::canonical_join::Joined {
            launch: launch_ref,
            visibility: crate::canonical_join::Visibility::Missed,
        }),
    ));
    let CanonicalEvent::FlapTrade(own) = own else {
        unreachable!()
    };
    assert!(j.record(
        RecordKind::Canonical,
        42,
        crate::runtime_journal::flap_trade_payload(&own)
    ));
    assert!(j.record(RecordKind::Runtime, buy.times.sink_ns, l));
    j.finish().unwrap();
    let e = evaluate(
        &rundir,
        &r.state,
        1 << 16,
        None,
        None,
        &[],
        0,
        Some([1; 32]),
        [2; 32],
        t0_ms + 60_000,
    )
    .unwrap();
    assert_eq!(e.attempts.len(), 1);
    let a = &e.attempts[0];
    assert_eq!(
        (a.kind, a.cell, a.state),
        (
            "C3_BUY",
            // No manifest names the operator's launchers: not known to
            // be another's (review H3).
            crate::live_attempts::CELL_UNKNOWN,
            "INCLUDED"
        )
    );
    assert_eq!(
        (a.attempt, a.trigger, a.run),
        (
            buy.handoff.attempt,
            Some(SourceId::PublicNode),
            Some(t0_ms - 60_000)
        )
    );
    // Each adapter names the granted builder it was (review H2).
    assert_eq!(a.adapters[0].builder, Some(LiveAdapter::Club48BuilderV1));
    assert_eq!(a.local.unwrap().clock_id, buy.handoff.report.clock_id);
    assert_eq!(a.adapters.len(), buy.handoff.adapters as usize);
    assert_eq!(a.adapters[0].outcome, "HANDED_OFF");
    assert_eq!(
        (
            a.verdicts.acquisition,
            a.verdicts.placement,
            a.verdicts.capture,
            a.verdicts.timing
        ),
        (PASS, PASS, PASS, UNKNOWN)
    );
    assert_eq!(
        (a.window.trades, a.window.buys, a.window.end_block),
        (1, 1, Some(HEAD + 34))
    );
    assert_eq!(e.campaign.verdict, CAMPAIGN_NOT_READY);
    let file = rundir.join(format!(
        "bsc-runtime/live-attempts/0x{}.dbrec",
        alloy_primitives::hex::encode(buy.handoff.tx_hash)
    ));
    let rec = darkbot_bsc_record::decode(
        &darkbot_bsc_record::schema::bsc_runtime::live_attempt::SCHEMA,
        &std::fs::read(file).unwrap(),
    )
    .unwrap();
    assert_eq!(
        rec.text(darkbot_bsc_record::schema::bsc_runtime::live_attempt::TIMING)
            .unwrap(),
        UNKNOWN
    );
    assert!(rundir.join(crate::live_attempts::VERDICT_PATH).is_file());
    r.clean();
}

/// 01.11. (R1): under a C3-only grant Strategy C decides the exits of
/// the grant's lots (before: LIVE_NOT_IN_SCOPE, a C3 lot could never be
/// sold): its exit goes to the builders; its own BUY stays refused
/// (STRATEGY_BUYS_NOT_GRANTED), nothing of it reaches the owner.
#[test]
fn a_c3_grants_lots_are_exited_by_strategy_c_and_its_buys_refused() {
    use crate::runtime_strategy::StrategyAction;
    let r = Rig::new("live-c3-exits", LiveStrategies::C3);
    let g = granted_generation(&r.state);
    let token = [0xE1; 20];
    let buy = Fact {
        kind: FactKind::Released,
        tx_hash: [0xe1; 32],
        intent: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        token,
        nonce: 4,
        grant_generation: g,
        input_wei: Some(U256::from(10_000u64)),
        max_wei: Some(U256::from(10_000u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD - 10),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    let ledger = Ledger::open(&r.state);
    ledger.append(&buy, NOW).unwrap();
    let tokens = U256::from(1_000_000_000_000_000_000u128);
    ledger
        .append(
            &Fact {
                block: Some((HEAD - 20, [0x91; 32], NOW - 60_000)),
                gas_wei: Some(U256::ZERO),
                tokens: Some(tokens),
                eth_wei: Some(U256::from(10_000u64)),
                ..Fact::about(&buy, FactKind::Included)
            },
            NOW,
        )
        .unwrap();
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let clock = a_clock();
    let StrategyAction::Buy { curve, .. } = fresh_buy([0xE3; 20]) else {
        unreachable!()
    };
    let exit = StrategyAction::Exit {
        token,
        tokens,
        curve,
        decided_ns: 0,
    };
    // R1 review: an exit of a token without a LIVE lot (the strategy's
    // own PAPER entry) gives nothing back: the C3 BUY's lease stays.
    let leased = |x: &LiveRuntime| x.executor.as_ref().unwrap().end().lease().is_some();
    assert!(leased(&x), "the arm leases its C3 BUY");
    let phantom = StrategyAction::Exit {
        token: [0xE9; 20],
        tokens,
        curve,
        decided_ns: 0,
    };
    let (handed, refusal) = x.on_strategy(&phantom, &c2, &clock, DECIDED_MS).unwrap();
    assert!(handed.is_empty());
    assert_eq!(refusal, Some(Refusal::NotHeld));
    assert!(leased(&x), "the lease stays");
    let (handed, refusal) = x.on_strategy(&exit, &c2, &clock, DECIDED_MS).unwrap();
    assert_eq!(refusal, None, "{:?}", reason(&x));
    assert!(!handed.is_empty(), "the exit's first step went");
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    let admitted = crate::sources::admitted_intents(&r.state).unwrap().len();
    let (handed, refusal) = x
        .on_strategy(&fresh_buy([0xE3; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert!(handed.is_empty());
    assert_eq!(refusal, Some(Refusal::Owner));
    assert!(refused_for(&x, "STRATEGY_BUYS_NOT_GRANTED"));
    assert_eq!(
        crate::sources::admitted_intents(&r.state).unwrap().len(),
        admitted
    );
    x.shutdown();
    r.clean();
}

/// 01.11. (R1): the open lots of an earlier process get their exit
/// decider again through the runtime's canonical events. Before the
/// runtime told, every open lot is undecided (no new BUY, EXIT_FIRST).
/// At a block's end each lot the strategy does not hold is read (its
/// curve at that block); before the next block's events it is held
/// again (journaled `k`), unless a trade of its token overtook the read
/// (read again later). Off the curve, outside the priced scope or
/// refused by its answer a lot is stranded (no new BUY, named); a read
/// that failed in transit counts and is read again. All of it shows in
/// the status.
#[test]
fn the_runtime_holds_an_earlier_processs_open_lots_again() {
    use crate::chain_follower::CanonicalTrade;
    use crate::runtime::{Canonical, Clock, RuntimeConfig, apply_canonical, journal_max_payload};
    use crate::runtime_journal::{JournalSetup, RuntimeJournal};
    use crate::sources::{
        LIVE_POSITION_MIGRATED, LIVE_POSITION_OFF_CURVE, LIVE_POSITION_UNPRICED,
        LIVE_POSITION_UNREADABLE,
    };
    use crate::venue_probe::{AbiFault, FlapCurve, ProbeError, ProbeRead};
    use darkbot_bsc_trading::economics::flap::native_curve20::{TokenTerms, profile_terms};
    let config =
        RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env")).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let mut c = Canonical::new(&config.canonical.clone().unwrap(), 0);
    c.strategy = Some(crate::runtime_strategy::StrategyRuntime::new(
        config.strategy.engine.clone(),
        config.strategy.gas(&repo).unwrap(),
        crate::runtime_strategy::StrategyVenues::FLAP,
    ));
    c.lot.config.lot_reads_max = 2;
    c.lot.config.lot_read_retry_ms = 0;
    c.lot.config.lot_read_refused_max = 2;
    const KEPT: [u8; 20] = [0xE1; 20];
    const OFF: [u8; 20] = [0xE2; 20];
    const UNPRICED: [u8; 20] = [0xE3; 20];
    const REFUSED: [u8; 20] = [0xE4; 20];
    const FLAKY: [u8; 20] = [0xE5; 20];
    const MIGRATED: [u8; 20] = [0xE6; 20];
    c.lot.reader = |_, _, _, token, number, hash| {
        let curve = |terms: TokenTerms| {
            Ok(FlapCurve {
                block_number: number,
                block_hash: hash,
                terms,
                reserve: U256::from(2_000_000_000_000_000_000u128),
                circulating: U256::from(300_000_000_000_000_000_000_000u128),
                dex_listed: false,
            })
        };
        match token.into_array() {
            KEPT => curve(profile_terms(100, 100)),
            OFF => curve(TokenTerms {
                tradable: false,
                ..profile_terms(100, 100)
            }),
            // R5: off the curve, listed on its DEX.
            MIGRATED => curve(TokenTerms {
                tradable: false,
                ..profile_terms(100, 100)
            })
            .map(|c| FlapCurve {
                dex_listed: true,
                ..c
            }),
            UNPRICED => curve(TokenTerms {
                version: 7,
                ..profile_terms(100, 100)
            }),
            REFUSED => Err(ProbeError::Abi {
                read: ProbeRead::TokenState,
                fault: AbiFault::FeeBps,
            }),
            _ => Err(ProbeError::Reorg { number }),
        }
    };
    let base =
        std::env::temp_dir().join(format!("bsc-runtime-rehold-journal-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let mut r = limited("live-rehold", crate::sources::tests::TEST_LIMITS);
    // R5: the DEX exit ON, its quotes a stub's.
    r.exec.exec.live_dex_exit = true;
    c.lot.quoter = |_, _, _, _, _, number, hash| {
        Ok(crate::venue_probe::DexQuote {
            block_number: number,
            block_hash: hash,
            output: U256::from(1u8),
        })
    };
    let g = granted_generation(&r.state);
    let ledger = Ledger::open(&r.state);
    for (n, token) in [
        (1u64, KEPT),
        (2, OFF),
        (3, UNPRICED),
        (4, REFUSED),
        (5, FLAKY),
        (6, MIGRATED),
    ] {
        let buy = Fact {
            kind: FactKind::Released,
            tx_hash: [0xe0 + n as u8; 32],
            intent: LiveIntentKind::Buy,
            component: ComponentId::ExecC3,
            token,
            nonce: n,
            grant_generation: g,
            input_wei: Some(U256::from(10_000u64)),
            max_wei: Some(U256::from(10_000u64)),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(HEAD - 10),
            by_tx: None,
            response: None,
            origin: None,
            launcher: None,
            placement: None,
            attempt: None,
            operator: None,
            source: None,
        };
        ledger.append(&buy, NOW).unwrap();
        let included = Fact {
            block: Some((HEAD - 20, [0x91; 32], NOW - 60_000)),
            gas_wei: Some(U256::ZERO),
            tokens: Some(U256::from(1_000_000u64)),
            eth_wei: Some(U256::from(10_000u64)),
            ..Fact::about(&buy, FactKind::Included)
        };
        ledger.append(&included, NOW).unwrap();
    }
    let c2 = c2_at(&r.d, r.buyer, 7);
    let mut live = Some(r.runtime("boot-1"));
    // A fresh process: the runtime has not told who decides yet.
    live.as_mut().unwrap().exit_deciders = None;
    settle(live.as_mut().unwrap(), &c2);
    assert!(
        live.as_ref().unwrap().armed(),
        "{:?}",
        reason(live.as_ref().unwrap())
    );
    let refusal = |live: &Option<LiveRuntime>| {
        live.as_ref()
            .unwrap()
            .buy_refusal(U256::ZERO, Some(&c2), Some(7))
    };
    // Armed, before any canonical event: undecided, no new BUY.
    assert_eq!(refusal(&live), Some((Refusal::ExitFirst, None)));
    let now = darkbot_bsc_record::observed_now().unwrap();
    let apply =
        |c: &mut Canonical, live: &mut Option<LiveRuntime>, journal: &mut RuntimeJournal, event| {
            apply_canonical(
                c,
                &mut None,
                &mut None,
                live,
                journal,
                Clock(Instant::now()),
                1,
                event,
            )
        };
    let block = |number: u64| CanonicalEvent::Block {
        number,
        hash: [number as u8; 32],
        parent_hash: [(number - 1) as u8; 32],
        timestamp_s: now / 1_000,
        timestamp_ms: now,
        txs: Vec::new(),
        observed: Instant::now(),
        observed_unix_ms: Some(now),
    };
    let complete = |number: u64| CanonicalEvent::BlockComplete {
        number,
        hash: [number as u8; 32],
    };
    apply(&mut c, &mut live, &mut journal, block(HEAD));
    apply(&mut c, &mut live, &mut journal, complete(HEAD));
    assert_eq!(refusal(&live), Some((Refusal::ExitFirst, None)));
    assert_eq!(
        c.lot.reads.len(),
        2,
        "at most BSC_CANONICAL_LOT_READS_MAX at a time"
    );
    c.lot.config.lot_reads_max = 8;
    apply(&mut c, &mut live, &mut journal, complete(HEAD));
    assert_eq!(c.lot.reads.len(), 6, "each lot read once, none twice");
    c.lot.settle();
    // R1 re-check LOW 1: an overtaken read takes no backoff; with one
    // set, only the failed reads wait for it.
    c.lot.config.lot_read_retry_ms = 600_000;
    // A trade of KEPT after the block it was read at: its curve moved,
    // the read is overtaken (read again); the others are answered.
    let trade = CanonicalEvent::FlapTrade(CanonicalTrade {
        block_number: HEAD + 1,
        block_hash: [(HEAD + 1) as u8; 32],
        tx_index: 0,
        tx_hash: [0x77; 32],
        log_index: 0,
        origin: [0x99; 20],
        sell: false,
        token: KEPT,
        trader: [0x99; 20],
        amount: [0; 32],
        eth: [0; 32],
        fee: [0; 32],
        post_price: [0; 32],
    });
    apply(&mut c, &mut live, &mut journal, trade);
    apply(&mut c, &mut live, &mut journal, block(HEAD + 1));
    let s = c.strategy.as_ref().unwrap();
    assert!(!s.holds(&KEPT), "never a moved curve");
    apply(&mut c, &mut live, &mut journal, complete(HEAD + 1));
    // One refused answer is read again, not stranded yet; the overtaken
    // read is asked again at once.
    let d = live.as_ref().unwrap().exit_deciders.clone().unwrap();
    assert!(d.undecided.contains(&REFUSED), "{d:?}");
    assert!(c.lot.reads.iter().any(|r| r.token == KEPT));
    assert!(
        !c.lot
            .reads
            .iter()
            .any(|r| r.token == REFUSED || r.token == FLAKY),
        "the failed reads wait for the backoff"
    );
    // The backoff over, the failed ones are read again.
    c.lot.config.lot_read_retry_ms = 0;
    c.lot.retry_at.clear();
    apply(&mut c, &mut live, &mut journal, complete(HEAD + 1));
    c.lot.settle();
    apply(&mut c, &mut live, &mut journal, block(HEAD + 2));
    let s = c.strategy.as_ref().unwrap();
    assert!(s.holds(&KEPT), "held again on its read curve");
    for t in [OFF, UNPRICED, REFUSED, FLAKY, MIGRATED] {
        assert!(!s.holds(&t));
    }
    apply(&mut c, &mut live, &mut journal, complete(HEAD + 2));
    let x = live.as_ref().unwrap();
    let d = x.exit_deciders.as_ref().unwrap();
    assert_eq!(d.undecided, [FLAKY], "a transport failure is read again");
    let first = d.stranded[0].1;
    let mut stranded = d.stranded.clone();
    stranded.sort();
    assert_eq!(
        stranded,
        [
            (OFF, LIVE_POSITION_OFF_CURVE),
            (UNPRICED, LIVE_POSITION_UNPRICED),
            (REFUSED, LIVE_POSITION_UNREADABLE),
            (MIGRATED, LIVE_POSITION_MIGRATED)
        ]
    );
    assert!(d.faults >= 3, "{}", d.faults);
    assert_eq!(refusal(&live), Some((Refusal::ExitStopped, Some(first))));
    // R5: the migrated lot alone is quoted for its DEX exit.
    assert_eq!(
        c.lot.quotes.iter().map(|q| q.token).collect::<Vec<_>>(),
        [MIGRATED]
    );
    c.lot.settle();
    let status = record(x);
    assert_eq!(
        status.texts(l::EXIT_UNDECIDED).unwrap(),
        [format!("0x{}", alloy_primitives::hex::encode(FLAKY))]
    );
    assert_eq!(status.texts(l::STRANDED).unwrap().len(), 4);
    assert!(status.u64(l::LOT_READ_FAULTS).unwrap() >= 3);
    assert!(status.opt_text(l::LOT_READ_FAULT).unwrap().is_some());
    // Without a strategy no lot has its exit decider.
    c.strategy = None;
    apply(&mut c, &mut live, &mut journal, block(HEAD + 3));
    let d = live.as_ref().unwrap().exit_deciders.clone().unwrap();
    assert!(d.undecided.contains(&KEPT) && d.undecided.contains(&FLAKY));
    live.take().unwrap().shutdown();
    r.clean();
    let _ = fs::remove_dir_all(&base);
}

/// REV.1.0 R3b (BSC3-024/041): the runtime's receipt recovery through
/// its canonical events. At a block's end every unresolved transaction
/// whose window is over or unknown is asked about (at most
/// `BSC_CANONICAL_RECOVERY_READS_MAX` at a time, none twice); before
/// the next block's events: a final block found resolves it as the
/// follower's block would, its facts of source RECEIPT (INCLUDED; a
/// lease's BUY found in its window adopted; REPLACED by the other
/// transaction at its nonce), a block not final or a failed read
/// leaves it unresolved, named, asked again after the backoff.
/// Without a second source nothing is asked. The facts survive a
/// restart, a block found again changes nothing, and the status names
/// all of it.
#[test]
fn unresolved_transactions_are_recovered_from_final_confirmed_blocks() {
    use crate::chain_follower::{
        CanonicalTrade, Recovered, RecoveryPending, RecoveryTarget, TxCost,
    };
    use crate::runtime::{Canonical, Clock, RuntimeConfig, apply_canonical, journal_max_payload};
    use crate::runtime_journal::{JournalSetup, RuntimeJournal};
    const A: [u8; 32] = [0xa1; 32];
    const B: [u8; 32] = [0xb5; 32];
    const C: [u8; 32] = [0xc1; 32];
    const D: [u8; 32] = [0xd5; 32];
    const LEASE_TX: [u8; 32] = [0xe7; 32];
    const TAKER: [u8; 32] = [0xb6; 32];
    const TOKEN_A: [u8; 20] = [0xaa; 20];
    const TOKEN_E: [u8; 20] = [0xee; 20];
    /// The buyer's Portal BUY `tx` of `token` in block `n` (hash `h`).
    fn bought(
        n: u64,
        h: u8,
        tx: [u8; 32],
        sender: [u8; 20],
        nonce: u64,
        token: [u8; 20],
    ) -> Vec<CanonicalEvent> {
        vec![
            block_of(
                n,
                h,
                h - 1,
                vec![CanonicalTx {
                    hash: tx,
                    index: 0,
                    sender,
                    nonce,
                    succeeded: true,
                    cost: Some(TxCost {
                        gas_used: 21_000,
                        effective_gas_price: 1,
                    }),
                }],
            ),
            CanonicalEvent::FlapTrade(CanonicalTrade {
                block_number: n,
                block_hash: [h; 32],
                tx_index: 0,
                tx_hash: tx,
                log_index: 0,
                origin: sender,
                sell: false,
                token,
                trader: sender,
                amount: U256::from(5_000u64).to_be_bytes(),
                eth: U256::from(1_000u64).to_be_bytes(),
                fee: [0; 32],
                post_price: [0; 32],
            }),
            complete_of(n, h),
        ]
    }
    let config =
        RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env")).unwrap();
    let mut c = Canonical::new(&config.canonical.clone().unwrap(), 0);
    c.recovery.reads_max = 2;
    c.recovery.reader = |_, _, _, _, _, target| match target {
        RecoveryTarget::Hash {
            tx_hash: A,
            sender,
            nonce,
            ..
        } => Ok(Recovered::Block(bought(
            HEAD + 3,
            0xa3,
            A,
            sender,
            nonce,
            TOKEN_A,
        ))),
        // Another transaction of the buyer at B's nonce (2).
        RecoveryTarget::Hash {
            tx_hash: B, sender, ..
        } => Ok(Recovered::Block(vec![
            block_of(
                HEAD + 4,
                0xb4,
                0xb3,
                vec![CanonicalTx {
                    hash: TAKER,
                    index: 0,
                    sender,
                    nonce: 2,
                    succeeded: true,
                    cost: None,
                }],
            ),
            complete_of(HEAD + 4, 0xb4),
        ])),
        RecoveryTarget::Hash { tx_hash: C, .. } => {
            Ok(Recovered::Pending(RecoveryPending::NotFinal {
                block: HEAD + 9,
            }))
        }
        RecoveryTarget::Hash { .. } => Err("TEST_ONLY transport".into()),
        RecoveryTarget::Window {
            sender,
            nonce,
            from,
            to,
        } => {
            assert_eq!((from, to), (HEAD, HEAD + 4), "the lease's window");
            Ok(Recovered::Block(bought(
                HEAD + 2,
                0xe2,
                LEASE_TX,
                sender,
                nonce,
                TOKEN_E,
            )))
        }
    };
    // A process that held only its C3 lease ended (BSC3-076).
    let mut r = Rig::new("live-recover", LiveStrategies::C3);
    r.exec.exec.live_lease_blocks = 4;
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2_at(&r.d, r.buyer, 5));
    assert!(x.armed(), "{:?}", reason(&x));
    let held = leases(&r.state);
    assert_eq!(held.len(), 1);
    std::mem::forget(x);
    // Its earlier transactions the follower never resolved.
    let ledger = Ledger::open(&r.state);
    for (hash, nonce, max_block) in [
        (A, 1, Some(HEAD + 3)),
        (B, 2, Some(HEAD + 3)),
        (C, 3, None),
        (D, 4, Some(HEAD + 3)),
    ] {
        let buy = Fact {
            kind: FactKind::Released,
            tx_hash: hash,
            intent: LiveIntentKind::Buy,
            component: ComponentId::ExecC3,
            token: TOKEN_A,
            nonce,
            grant_generation: held[0].grant_generation,
            input_wei: Some(U256::from(10_000u64)),
            max_wei: Some(U256::from(20_000u64)),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block,
            by_tx: None,
            response: None,
            origin: None,
            launcher: None,
            placement: None,
            attempt: Some(hash),
            operator: None,
            source: None,
        };
        ledger.append(&buy, NOW).unwrap();
    }
    r.authorize("boot-2", "after-crash");
    let mut live = Some(r.runtime("boot-2"));
    assert_eq!(live.as_ref().unwrap().book().unresolved().count(), 5);
    let base = std::env::temp_dir().join(format!(
        "bsc-runtime-recover-journal-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let apply =
        |c: &mut Canonical, live: &mut Option<LiveRuntime>, journal: &mut RuntimeJournal, event| {
            apply_canonical(
                c,
                &mut None,
                &mut None,
                live,
                journal,
                Clock(Instant::now()),
                1,
                event,
            )
        };
    let at = |n: u64| block_of(n, n as u8, (n - 1) as u8, Vec::new());
    let end = |n: u64| complete_of(n, n as u8);
    // Head HEAD + 4: no window is over yet (the lease's ends there, the
    // others before; C's is unknown).
    apply(&mut c, &mut live, &mut journal, at(HEAD + 4));
    apply(&mut c, &mut live, &mut journal, end(HEAD + 4));
    let asked = |c: &Canonical| c.recovery.reads.iter().map(|r| r.key).collect::<Vec<_>>();
    assert_eq!(asked(&c).len(), 2, "at most the bound at a time");
    let queued = |c: &Canonical| {
        c.recovery
            .pending
            .iter()
            .filter(|(_, why)| *why == crate::sources::LIVE_RECOVERY_QUEUED)
            .count()
    };
    assert_eq!(queued(&c), 2, "the others wait, named");
    assert!(
        asked(&c).iter().all(|k| [A, B, C, D].contains(k)),
        "the lease's window runs"
    );
    c.recovery.reads_max = 8;
    apply(&mut c, &mut live, &mut journal, end(HEAD + 4));
    assert_eq!(asked(&c).len(), 4, "each once, none twice");
    // Their answers before the next block's events; at its end the
    // lease's window is over too: only it is asked (the others answered,
    // within the backoff or resolved).
    c.recovery.settle();
    apply(&mut c, &mut live, &mut journal, at(HEAD + 10));
    apply(&mut c, &mut live, &mut journal, end(HEAD + 10));
    assert_eq!(asked(&c), [held[0].tx_hash]);
    c.recovery.settle();
    apply(&mut c, &mut live, &mut journal, at(HEAD + 11));
    let x = live.as_ref().unwrap();
    let book = x.book();
    let a = book.tx(&A).unwrap();
    assert_eq!(a.state, FactKind::Included);
    assert_eq!(
        a.inclusion.as_ref().unwrap().source,
        Some(FactSource::Receipt)
    );
    assert_eq!(a.inclusion.as_ref().unwrap().block.unwrap().0, HEAD + 3);
    assert!(book.open_lot(&TOKEN_A).is_some(), "its lot is open");
    let b = book.tx(&B).unwrap();
    assert_eq!(b.state, FactKind::Replaced);
    let replaced = ledger
        .facts()
        .unwrap()
        .into_iter()
        .find(|f| f.tx_hash == B && f.kind == FactKind::Replaced)
        .unwrap();
    assert_eq!(replaced.source, Some(FactSource::Receipt));
    assert_eq!(replaced.by_tx, Some(TAKER));
    let e = book.tx(&LEASE_TX).unwrap();
    assert_eq!(e.state, FactKind::Included, "the lease's BUY adopted");
    assert_eq!(
        e.inclusion.as_ref().unwrap().source,
        Some(FactSource::Receipt)
    );
    assert!(book.open_lot(&TOKEN_E).is_some());
    let unresolved: Vec<[u8; 32]> = book.unresolved().map(|t| t.released.tx_hash).collect();
    assert_eq!(unresolved, [C, D]);
    let status = record(x);
    let named = |tx: [u8; 32], why: &str| format!("0x{} {why}", alloy_primitives::hex::encode(tx));
    let mut pending = status.texts(l::RECOVERY_PENDING).unwrap().to_vec();
    pending.sort();
    assert_eq!(
        pending,
        [
            named(C, "RECOVERY_NOT_FINAL"),
            named(D, crate::sources::LIVE_RECOVERY_FAILED)
        ]
    );
    assert_eq!(status.u64(l::RECOVERED).unwrap(), 3);
    assert_eq!(status.u64(l::RECOVERY_FAULTS).unwrap(), 1);
    assert!(
        status
            .opt_text(l::RECOVERY_FAULT)
            .unwrap()
            .unwrap()
            .contains("TEST_ONLY transport")
    );
    // Within the backoff nothing is asked again; after it, the rest.
    apply(&mut c, &mut live, &mut journal, end(HEAD + 11));
    assert!(asked(&c).is_empty(), "{:?}", asked(&c));
    c.recovery.retry_at.clear();
    apply(&mut c, &mut live, &mut journal, end(HEAD + 11));
    let mut again = asked(&c);
    again.sort();
    assert_eq!(again, [C, D]);
    c.recovery.settle();
    // A recovered block is applied only between the follower's blocks;
    // found again it changes nothing.
    let facts = ledger.facts().unwrap().len();
    let x = live.as_mut().unwrap();
    let _ = x.on_canonical(&at(HEAD + 12));
    assert_eq!(
        x.recover_block(&C, &bought(HEAD + 3, 0xa3, A, r.buyer, 1, TOKEN_A)),
        None
    );
    let _ = x.on_canonical(&end(HEAD + 12));
    assert_eq!(
        x.recover_block(&A, &bought(HEAD + 3, 0xa3, A, r.buyer, 1, TOKEN_A)),
        Some(Vec::new())
    );
    assert_eq!(ledger.facts().unwrap().len(), facts);
    // A recovered block resolves only what is not resolved yet: C's
    // facts (its inclusion, its placement), never A's again.
    let both = vec![
        block_of(
            HEAD + 6,
            0xc6,
            0xc5,
            [(A, 1), (C, 3)]
                .map(|(hash, nonce)| CanonicalTx {
                    hash,
                    index: nonce,
                    sender: r.buyer,
                    nonce,
                    succeeded: true,
                    cost: None,
                })
                .to_vec(),
        ),
        complete_of(HEAD + 6, 0xc6),
    ];
    assert!(x.recover_block(&C, &both).is_some());
    assert_eq!(x.book().tx(&C).unwrap().state, FactKind::Included);
    let after = ledger.facts().unwrap();
    assert_eq!(after.len(), facts + 2, "C's INCLUDED and PLACED only");
    assert!(after[facts..].iter().all(|f| f.tx_hash == C));
    // Without a second source nothing is asked, each named.
    c.confirm = None;
    c.recovery.reads.clear();
    c.recovery.retry_at.clear();
    apply(&mut c, &mut live, &mut journal, at(HEAD + 13));
    apply(&mut c, &mut live, &mut journal, end(HEAD + 13));
    assert!(asked(&c).is_empty());
    let status = record(live.as_ref().unwrap());
    assert!(
        status
            .texts(l::RECOVERY_PENDING)
            .unwrap()
            .iter()
            .all(|p| p.ends_with(crate::sources::LIVE_RECOVERY_NO_SECOND_SOURCE)),
        "{:?}",
        status.texts(l::RECOVERY_PENDING)
    );
    // A restart reads the recovered facts as written.
    live.take().unwrap().shutdown();
    r.authorize("boot-3", "restart");
    let mut x = r.runtime("boot-3");
    assert_eq!(
        x.book().tx(&A).unwrap().inclusion.as_ref().unwrap().source,
        Some(FactSource::Receipt)
    );
    assert_eq!(x.book().tx(&B).unwrap().state, FactKind::Replaced);
    x.shutdown();
    r.clean();
    let _ = fs::remove_dir_all(&base);
}

/// REV.1.0 R3b review: what the recovery never asks is named (another
/// sender than the buyer, a lease without its window); a recovered
/// block resolves a reverted transaction as REVERTED (source RECEIPT),
/// adopts the BUY of the lease whose window holds it (of two at one
/// nonce), and replaces a lease whose nonce another transaction took
/// (counted); a reorg deeper than the follower's window takes a
/// recovered inclusion back, and the transaction is asked again, its
/// window bounding nothing.
#[test]
fn the_recovery_names_what_it_never_asks_and_resolves_reverts_takers_and_orphans() {
    use crate::chain_follower::{CanonicalTrade, RecoveryTarget, TxCost};
    use crate::sources::NonceOrigin;
    const R: [u8; 32] = [0x71; 32];
    const O: [u8; 32] = [0x72; 32];
    const L1: [u8; 32] = [0x73; 32];
    const L2: [u8; 32] = [0x74; 32];
    const NW: [u8; 32] = [0x75; 32];
    const L3: [u8; 32] = [0x76; 32];
    const BUY: [u8; 32] = [0x77; 32];
    const FX: [u8; 32] = [0x78; 32];
    const L4: [u8; 32] = [0x79; 32];
    const RV: [u8; 32] = [0x7a; 32];
    const TOKEN: [u8; 20] = [0xab; 20];
    let mut r = Rig::new("live-recover-names", LiveStrategies::C3);
    r.exec.exec.live_lease_blocks = 4;
    let ledger = Ledger::open(&r.state);
    let fact = |kind, hash: [u8; 32], token, nonce, max_block, origin| Fact {
        kind,
        tx_hash: hash,
        intent: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        token,
        nonce,
        grant_generation: 1,
        input_wei: Some(U256::from(10_000u64)),
        max_wei: Some(U256::from(20_000u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block,
        by_tx: None,
        response: None,
        origin,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    let other = Some(NonceOrigin {
        chain_id: crate::wss::BSC_MAINNET_CHAIN_ID,
        sender: [0x0e; 20],
    });
    for f in [
        fact(FactKind::Released, R, TOKEN, 1, Some(HEAD + 1), None),
        fact(FactKind::Released, O, TOKEN, 2, Some(HEAD + 1), other),
        fact(FactKind::Unknown, L1, [0; 20], 9, Some(HEAD + 2), None),
        fact(FactKind::Unknown, L2, [0; 20], 9, Some(HEAD + 20), None),
        fact(FactKind::Unknown, NW, [0; 20], 10, None, None),
        fact(FactKind::Unknown, L3, [0; 20], 11, Some(HEAD + 21), None),
        fact(FactKind::Unknown, L4, [0; 20], 12, Some(HEAD + 22), None),
    ] {
        ledger.append(&f, NOW).unwrap();
    }
    let mut x = r.runtime("boot-1");
    let buyer = r.buyer;
    let targets = x.recovery_targets(HEAD + 30);
    let of = |k: [u8; 32]| targets.iter().find(|(h, _)| *h == k).unwrap().1;
    assert_eq!(of(O), Err(crate::sources::LIVE_RECOVERY_OTHER_SENDER));
    assert_eq!(
        of(NW),
        Err(crate::sources::LIVE_RECOVERY_LEASE_WINDOW_UNKNOWN)
    );
    assert_eq!(
        of(R),
        Ok(RecoveryTarget::Hash {
            tx_hash: R,
            sender: buyer,
            nonce: 1,
            window_end: Some(HEAD + 1)
        })
    );
    assert_eq!(
        of(L2),
        Ok(RecoveryTarget::Window {
            sender: buyer,
            nonce: 9,
            from: HEAD + 16,
            to: HEAD + 20
        })
    );
    let tx = |hash, nonce, succeeded| CanonicalTx {
        hash,
        index: 0,
        sender: buyer,
        nonce,
        succeeded,
        cost: Some(TxCost {
            gas_used: 21_000,
            effective_gas_price: 1,
        }),
    };
    // R reverted in its receipt's block.
    let reverted = [
        block_of(HEAD + 1, 0x81, 0x80, vec![tx(R, 1, false)]),
        complete_of(HEAD + 1, 0x81),
    ];
    assert!(x.recover_block(&R, &reverted).is_some());
    let t = x.book().tx(&R).unwrap();
    assert_eq!(t.state, FactKind::Reverted);
    assert_eq!(
        t.inclusion.as_ref().unwrap().source,
        Some(FactSource::Receipt)
    );
    // Two leases at nonce 9: the BUY in the later one's window is its.
    let trade = |n: u64, h: u8, hash| {
        CanonicalEvent::FlapTrade(CanonicalTrade {
            block_number: n,
            block_hash: [h; 32],
            tx_index: 0,
            tx_hash: hash,
            log_index: 0,
            origin: buyer,
            sell: false,
            token: TOKEN,
            trader: buyer,
            amount: U256::from(5_000u64).to_be_bytes(),
            eth: U256::from(1_000u64).to_be_bytes(),
            fee: [0; 32],
            post_price: [0; 32],
        })
    };
    let adopted = [
        block_of(HEAD + 10, 0x8a, 0x89, vec![tx(BUY, 9, true)]),
        trade(HEAD + 10, 0x8a, BUY),
        complete_of(HEAD + 10, 0x8a),
    ];
    assert!(
        !x.recover_block(&L2, &adopted).unwrap().is_empty(),
        "its lot opened"
    );
    assert_eq!(x.book().tx(&BUY).unwrap().state, FactKind::Included);
    assert!(x.book().open_lot(&TOKEN).is_some());
    assert!(!refused_by(&x, "LEASE_NONCE_TAKEN_BY_OTHER"));
    for l in [L1, L2] {
        assert_eq!(x.book().tx(&l).unwrap().state, FactKind::Replaced);
    }
    // Another transaction of the buyer at L3's nonce (no BUY of it).
    let foreign = [
        block_of(HEAD + 12, 0x8c, 0x8b, vec![tx(FX, 11, true)]),
        complete_of(HEAD + 12, 0x8c),
    ];
    assert!(x.recover_block(&L3, &foreign).is_some());
    assert_eq!(x.book().tx(&L3).unwrap().state, FactKind::Replaced);
    assert!(x.book().tx(&FX).is_none());
    assert!(refused_by(&x, "LEASE_NONCE_TAKEN_BY_OTHER"));
    // L4's BUY reverted (adopted, no token: no trade).
    let lease_reverted = [
        block_of(HEAD + 13, 0x8d, 0x8c, vec![tx(RV, 12, false)]),
        complete_of(HEAD + 13, 0x8d),
    ];
    assert!(x.recover_block(&L4, &lease_reverted).is_some());
    assert_eq!(x.book().tx(&RV).unwrap().state, FactKind::Reverted);
    // A reorg deeper than the follower's window takes R's and RV's
    // blocks back: each asked again by its hash, its window bounding
    // nothing (RV no lease although of no token, review LOW 4).
    let _ = x.on_canonical(&CanonicalEvent::ReorgTooDeep {
        orphaned: vec![(HEAD + 1, [0x81; 32]), (HEAD + 13, [0x8d; 32])],
        new_head: (HEAD + 31, [0x99; 32]),
    });
    let t = x.book().tx(&R).unwrap();
    assert!(t.orphaned && t.state == FactKind::Released, "{t:?}");
    let targets = x.recovery_targets(HEAD + 31);
    let of = |k: [u8; 32]| targets.iter().find(|(h, _)| *h == k).unwrap().1;
    assert_eq!(
        of(R),
        Ok(RecoveryTarget::Hash {
            tx_hash: R,
            sender: buyer,
            nonce: 1,
            window_end: None
        })
    );
    assert_eq!(
        of(RV),
        Ok(RecoveryTarget::Hash {
            tx_hash: RV,
            sender: buyer,
            nonce: 12,
            window_end: None
        })
    );
    x.shutdown();
    r.clean();
}

/// R2 review L7: a lot is stranded only by refused answers in a row:
/// any other outcome in between (an answer, a failure in transit, an
/// overtaken read) starts the count again.
#[test]
fn refused_answers_count_only_in_a_row() {
    let config =
        crate::runtime::RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env"))
            .unwrap();
    let mut c = crate::runtime::Canonical::new(&config.canonical.clone().unwrap(), 0);
    let t = [0xE4; 20];
    assert_eq!(c.lot.refused_in_a_row(t, true), 1);
    assert_eq!(c.lot.refused_in_a_row(t, true), 2);
    assert_eq!(c.lot.refused_in_a_row(t, false), 0);
    assert_eq!(c.lot.refused_in_a_row(t, true), 1);
    assert_eq!(c.lot.refused_in_a_row([0xE5; 20], true), 1, "per lot");
}

/// R12 SC4: the window's Four.meme curve events are canonical records (a
/// trade `Z` that reads back, a phase `y`, a refused log `z`); the
/// catch-up's are not journaled (each Four trade carries the curve's state
/// after it).
#[test]
fn the_windows_four_curve_events_are_journaled_and_the_catch_ups_not() {
    use crate::canonical_join::Venue;
    use crate::chain_follower::CanonicalFourTrade;
    use crate::runtime::{Canonical, Clock, RuntimeConfig, apply_canonical, journal_max_payload};
    use crate::runtime_journal::{JournalSetup, RuntimeJournal, four_trade_of};
    use darkbot_bsc_journal::RecordKind;
    use darkbot_bsc_journal::storage::run::RunReader;
    let config =
        RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env")).unwrap();
    let now = darkbot_bsc_record::observed_now().unwrap();
    let trade = CanonicalFourTrade {
        venue: Venue::OpenFour,
        block_number: HEAD,
        block_hash: [HEAD as u8; 32],
        tx_index: 3,
        tx_hash: [0x3b; 32],
        log_index: 2,
        origin: [0x77; 20],
        sell: true,
        token: [0xAA; 20],
        trader: [0x77; 20],
        amount: [1; 32],
        quote: [2; 32],
        remaining: [3; 32],
        raised: [4; 32],
    };
    for (tag, window_from_ms, journaled) in [
        ("window", now - 1_000, true),
        ("catchup", now + 1_000, false),
    ] {
        let mut c = Canonical::new(&config.canonical.clone().unwrap(), window_from_ms);
        let base =
            std::env::temp_dir().join(format!("bsc-runtime-four-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let mut journal = RuntimeJournal::open(
            &base,
            1,
            1,
            JournalSetup::whole(64, journal_max_payload(&config)),
        )
        .unwrap();
        for event in [
            CanonicalEvent::Block {
                number: HEAD,
                hash: [HEAD as u8; 32],
                parent_hash: [(HEAD - 1) as u8; 32],
                timestamp_s: now / 1_000,
                timestamp_ms: now,
                txs: Vec::new(),
                observed: Instant::now(),
                observed_unix_ms: Some(now),
            },
            CanonicalEvent::FourPhase {
                block_number: HEAD,
                block_hash: [HEAD as u8; 32],
                tx_index: 3,
                log_index: 1,
                token: [0xAA; 20],
                from: 0,
                to: 1,
            },
            CanonicalEvent::FourTrade(trade),
            CanonicalEvent::FourTradeUndecodable {
                block_number: HEAD,
                block_hash: [HEAD as u8; 32],
                tx_index: 4,
                log_index: 1,
            },
        ] {
            apply_canonical(
                &mut c,
                &mut None,
                &mut None,
                &mut None,
                &mut journal,
                Clock(Instant::now()),
                1,
                event,
            );
        }
        journal.finish().unwrap();
        let run = fs::read_dir(&base).unwrap().next().unwrap().unwrap().path();
        let mut reader = RunReader::open(&run, journal_max_payload(&config)).unwrap();
        let mut four = Vec::new();
        while let Some(r) = reader.next_record().unwrap() {
            if let Some(tag @ (b'Z' | b'y' | b'z')) = r.payload.first().copied() {
                assert_eq!(r.kind, RecordKind::Canonical);
                four.push((tag, r.payload.to_vec()));
            }
        }
        if journaled {
            let tags: Vec<u8> = four.iter().map(|(t, _)| *t).collect();
            assert_eq!(tags, [b'y', b'Z', b'z']);
            assert_eq!(four_trade_of(&four[1].1), Some(trade));
            assert_eq!((four[0].1.len(), four[2].1.len()), (79, 57));
        } else {
            assert!(four.is_empty(), "{tag}");
        }
        fs::remove_dir_all(&base).unwrap();
    }
}

/// REV.1.0 R2 review P2: the blocks and launches the follower catches
/// up on before a run's window are journaled as runtime records (no
/// G0.4 count reads them): the LIVE campaign's chain while no run was
/// up; the window's own blocks stay canonical records.
#[test]
fn the_catch_up_before_a_runs_window_is_journaled_for_the_campaign() {
    use crate::runtime::{Canonical, Clock, RuntimeConfig, apply_canonical, journal_max_payload};
    use crate::runtime_journal::{
        JournalSetup, RuntimeJournal, catchup_block_of, catchup_launch_of,
    };
    use darkbot_bsc_journal::RecordKind;
    use darkbot_bsc_journal::storage::run::RunReader;
    let config =
        RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env")).unwrap();
    let now = darkbot_bsc_record::observed_now().unwrap();
    // The window starts one second after the first block.
    let mut c = Canonical::new(&config.canonical.clone().unwrap(), now + 1_000);
    let base = std::env::temp_dir().join(format!("bsc-runtime-catchup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let block = |number: u64, at_ms: u64| CanonicalEvent::Block {
        number,
        hash: [number as u8; 32],
        parent_hash: [(number - 1) as u8; 32],
        timestamp_s: at_ms / 1_000,
        timestamp_ms: at_ms,
        txs: Vec::new(),
        observed: Instant::now(),
        observed_unix_ms: Some(at_ms),
    };
    let mut launch = launch_event(HEAD, HEAD as u8, 2, [0xAA; 20], [0xC1; 20]);
    if let CanonicalEvent::Launch(l) = &mut launch {
        l.block_number = HEAD;
    }
    let trade = buy_trade(HEAD, HEAD as u8, 3, [0x3b; 32], [0xAA; 20], [0x77; 20]);
    let refused = CanonicalEvent::FlapTradeUndecodable {
        block_number: HEAD,
        block_hash: [HEAD as u8; 32],
        tx_index: 4,
        log_index: 1,
    };
    for event in [
        block(HEAD, now),
        launch,
        trade,
        refused,
        block(HEAD + 1, now + 2_000),
    ] {
        apply_canonical(
            &mut c,
            &mut None,
            &mut None,
            &mut None,
            &mut journal,
            Clock(Instant::now()),
            1,
            event,
        );
    }
    journal.finish().unwrap();
    let run = fs::read_dir(&base).unwrap().next().unwrap().unwrap().path();
    let mut reader = RunReader::open(&run, journal_max_payload(&config)).unwrap();
    let (mut blocks, mut launches, mut canonical) = (Vec::new(), Vec::new(), Vec::new());
    let (mut trades, mut gaps) = (Vec::new(), Vec::new());
    while let Some(r) = reader.next_record().unwrap() {
        match (r.kind, r.payload.first()) {
            (RecordKind::Runtime, Some(b'b')) => blocks.push(catchup_block_of(&r.payload).unwrap()),
            (RecordKind::Runtime, Some(b'c')) => {
                launches.push(catchup_launch_of(&r.payload).unwrap())
            }
            (RecordKind::Canonical, Some(b'B')) => canonical.push(r.payload.to_vec()),
            // Review Q1: the catch-up's trades and refused logs, the `T`
            // and `W` layouts under runtime tags.
            (RecordKind::Runtime, Some(b'd')) => {
                let mut t = r.payload.to_vec();
                t[0] = b'T';
                trades.push(crate::runtime_journal::flap_trade_of(&t).unwrap());
            }
            (RecordKind::Runtime, Some(b'e')) => gaps.push(r.payload.len()),
            _ => {}
        }
    }
    assert_eq!(blocks, [(HEAD, [HEAD as u8; 32], now)]);
    assert_eq!(launches.len(), 1);
    let l = launches[0];
    assert_eq!(
        (l.block, l.block_ms, l.tx_index, l.token),
        (HEAD, now, 2, Some([0xAA; 20]))
    );
    assert!(l.c1_scope && !l.unverifiable);
    assert_eq!(canonical.len(), 1, "the window's block stays canonical");
    assert_eq!(trades.len(), 1);
    assert_eq!((trades[0].block_number, trades[0].tx_index), (HEAD, 3));
    assert_eq!(gaps, [57]);
    let _ = fs::remove_dir_all(&base);
}

/// BSC3-056 (review M-3): the runtime hands the second source's verdict
/// to the LIVE path with every canonical event, before a block's BUY
/// is planned: a head the second source has not confirmed refuses the
/// BUY, its header lets the BUY go.
#[test]
fn the_runtime_hands_the_verdict_to_the_live_path_before_a_blocks_buy() {
    use crate::chain_follower::ConfirmEvent;
    use crate::runtime::{Canonical, Clock, RuntimeConfig, apply_canonical, journal_max_payload};
    use crate::runtime_journal::{JournalSetup, RuntimeJournal};
    let config =
        RuntimeConfig::parse(include_str!("../../../../config/ingress.public.env")).unwrap();
    let mut c = Canonical::new(&config.canonical.clone().unwrap(), 0);
    let base =
        std::env::temp_dir().join(format!("bsc-runtime-hand-journal-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let mut journal = RuntimeJournal::open(
        &base,
        1,
        1,
        JournalSetup::whole(64, journal_max_payload(&config)),
    )
    .unwrap();
    let r = limited("live-hand", crate::sources::tests::TEST_LIMITS);
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut live = Some(r.runtime("boot-1"));
    settle(live.as_mut().unwrap(), &c2);
    let now = darkbot_bsc_record::observed_now().unwrap();
    let mut apply = |c: &mut Canonical, live: &mut Option<LiveRuntime>, event| {
        apply_canonical(
            c,
            &mut None,
            &mut None,
            live,
            &mut journal,
            Clock(Instant::now()),
            1,
            event,
        )
    };
    apply(
        &mut c,
        &mut live,
        CanonicalEvent::Block {
            number: HEAD,
            hash: [0xb1; 32],
            parent_hash: [0xb0; 32],
            timestamp_s: now / 1_000,
            timestamp_ms: now,
            txs: Vec::new(),
            observed: Instant::now(),
            observed_unix_ms: Some(now),
        },
    );
    let clock = a_clock();
    let x = live.as_mut().unwrap();
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!((out.len(), refusal), (0, Some(Refusal::Unconfirmed)));
    c.confirm_event(ConfirmEvent::Header {
        number: HEAD,
        hash: [0xb1; 32],
        parent_hash: [0xb0; 32],
        block_ms: now,
    });
    apply(
        &mut c,
        &mut live,
        CanonicalEvent::BlockComplete {
            number: HEAD,
            hash: [0xb1; 32],
        },
    );
    let x = live.as_mut().unwrap();
    let (out, refusal) = x
        .on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS)
        .unwrap();
    assert_eq!(
        (out.len(), refusal),
        (1, None),
        "{:?}",
        record(x).texts(l::REFUSALS)
    );
    x.shutdown();
    r.clean();
    journal.finish().unwrap();
    fs::remove_dir_all(base).unwrap();
}

/// Review of the limits (MEDIUM): a BUY whose bundle was lost (its
/// window over, still pending at the next nonce) is the next BUY's
/// alternative: under one open position or an interval it never blocks
/// the BUY that takes its nonce (before: refused for ever).
#[test]
fn a_lost_bundle_never_blocks_the_buy_that_takes_its_nonce() {
    use crate::sources::GrantLimits;
    for (tag, limits) in [
        (
            "live-lost-interval",
            GrantLimits {
                min_buy_interval_ms: 60_000,
                ..crate::sources::tests::TEST_LIMITS
            },
        ),
        (
            "live-lost-open",
            GrantLimits {
                max_open_positions: 1,
                ..crate::sources::tests::TEST_LIMITS
            },
        ),
    ] {
        let r = limited(tag, limits);
        let g = granted_generation(&r.state);
        let lost = Fact {
            kind: FactKind::Released,
            tx_hash: [0xd1; 32],
            intent: LiveIntentKind::Buy,
            component: ComponentId::StrategyC,
            token: [0xD1; 20],
            nonce: 5,
            grant_generation: g,
            input_wei: Some(U256::from(10_000u64)),
            max_wei: Some(U256::from(10_000u64)),
            block: None,
            gas_wei: None,
            tokens: None,
            eth_wei: None,
            max_block: Some(HEAD - 10),
            by_tx: None,
            response: None,
            origin: None,
            launcher: None,
            placement: None,
            attempt: None,
            operator: None,
            source: None,
        };
        Ledger::open(&r.state).append(&lost, NOW).unwrap();
        let c2 = c2_at(&r.d, r.buyer, 5);
        let mut x = r.runtime("boot-1");
        settle(&mut x, &c2);
        assert!(x.armed(), "{tag}: {:?}", reason(&x));
        let clock = a_clock();
        let handed = x.on_strategy(&fresh_buy([0xA1; 20]), &c2, &clock, DECIDED_MS);
        assert!(
            handed.is_some(),
            "{tag}: {:?}",
            record(&x).texts(l::REFUSALS)
        );
        assert!(
            r.requests.recv_timeout(Duration::from_secs(2)).is_ok(),
            "{tag}"
        );
        assert!(!refused_for(&x, crate::sources::LIVE_BUY_INTERVAL), "{tag}");
        assert!(
            !refused_for(&x, crate::sources::LIVE_OPEN_POSITIONS_MAX),
            "{tag}"
        );
        x.shutdown();
        r.clean();
    }
}

/// 31.10. order: a grant without its limits (written before them)
/// arms no LIVE path for BUYs: the gate names it.
#[test]
fn a_grant_without_limits_arms_no_buy() {
    let r = Rig::granting("live-limit-none", LiveStrategies::Both, |now| LiveGrant {
        limits: None,
        ..rig_grant(now + 3_600_000, LiveStrategies::Both)
    });
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(!x.armed());
    let why = reason(&x).1.unwrap_or_default();
    assert!(
        why.contains(crate::sources::LIVE_GRANT_LIMITS_MISSING),
        "{why}"
    );
    x.shutdown();
    r.clean();
}

/// PHASENPLAN v9 §2.3: a granted source the operator holds in SHADOW
/// triggers no LIVE BUY (counted SOURCE_NOT_LIVE); LIVE again, its next
/// launch does.
#[test]
fn a_granted_source_the_operator_holds_in_shadow_triggers_no_buy() {
    use crate::ingress::{IngressCore, Observation};
    let r = Rig::new("live-granted-shadow", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    // The operator holds the granted PublicNode in SHADOW: the grant
    // still names it, the path stays armed (its exits need no source).
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    let shadow = s
        .request("pn-shadow", SourceId::PublicNode, Mode::Shadow, g, NOW)
        .unwrap();
    assert!(shadow.applied(), "{}", shadow.line());
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let mut core = IngressCore::new(64, 16_384);
    let mut sighted = |x: &mut LiveRuntime, c2: &StateOwner, raw: &[u8], at: u64| {
        let hash = alloy_primitives::keccak256(raw).0;
        let decided = core.observe(
            SourceId::PublicNode,
            Observation::Full { hash, raw },
            at,
            None,
        );
        let (by, h, first) = crate::runtime::live_sighting(decided)?;
        x.on_sighting(by, h, [0; 20], first, raw, at, c2, &clock)
    };
    let refused = |x: &LiveRuntime, name: &str| {
        record(x)
            .texts(l::REFUSALS)
            .unwrap()
            .iter()
            .find_map(|t| t.strip_prefix(&format!("{name}=")).map(str::to_string))
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    // (The arm's lease holds its attempt already, BSC3-076.)
    let used = buy_usage(
        &r.state,
        NOW,
        crate::sources::tests::shipped_limits().buy_attempts_total_max,
    )
    .unwrap()
    .used;
    let admitted = crate::sources::admitted_intents(&r.state).unwrap();
    assert!(sighted(&mut x, &c2, &raw, 10).is_none());
    assert!(r.requests.recv_timeout(Duration::from_millis(200)).is_err());
    assert_eq!(refused(&x, "SOURCE_NOT_LIVE").as_deref(), Some("1"));
    assert_eq!(refused(&x, "SOURCE_NOT_GRANTED"), None);
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        used
    );
    assert_eq!(
        crate::sources::admitted_intents(&r.state).unwrap(),
        admitted
    );
    // A launch an ungranted source saw first, later seen by the granted
    // one while it is held in SHADOW: no BUY (BSC3-075's later path too).
    let (hash, raw, launcher) = crate::runtime::exec::tests::launch_n(1);
    c2.pending(hash, launcher, 4, 0, 20);
    assert!(
        x.on_launch(SourceId::Drpc, hash, [0; 20], &raw, 20, &c2, &clock)
            .is_none()
    );
    assert!(
        x.on_later(SourceId::PublicNode, hash, [0; 20], &raw, 21, &c2, &clock)
            .is_none()
    );
    assert!(r.requests.recv_timeout(Duration::from_millis(200)).is_err());
    // LIVE again: its later sighting triggers the BUY.
    let g = s.current().unwrap().generation;
    let live = s
        .request("pn-live", SourceId::PublicNode, Mode::Live, g, NOW)
        .unwrap();
    assert!(live.applied(), "{}", live.line());
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let buy = x
        .on_later(SourceId::PublicNode, hash, [0; 20], &raw, 22, &c2, &clock)
        .expect("the BUY");
    assert_eq!(buy.launch_hash, hash);
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    x.shutdown();
    r.clean();
}

/// BSC3-075: a source the grant does not name that sees a launch first
/// neither takes nor blocks its BUY: the first granted source that sees
/// it after triggers it, once (the production ingress core decides
/// first and later, the runtime's rule which may trigger).
#[test]
fn a_source_outside_the_grant_seen_first_never_blocks_the_buy() {
    use crate::ingress::{IngressCore, Observation};
    let r = Rig::new("live-ungranted-first", LiveStrategies::C3);
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            t.set(t.get() + 3);
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    let mut core = IngressCore::new(64, 16_384);
    // As the run loop hands an armed LIVE path a sighting.
    let mut sighted = |source: SourceId, x: &mut LiveRuntime, at: u64| {
        let decided = core.observe(source, Observation::Full { hash, raw: &raw }, at, None);
        let (by, h, first) = crate::runtime::live_sighting(decided)?;
        x.on_sighting(by, h, [0; 20], first, &raw, at, &c2, &clock)
    };
    let ungranted = |x: &LiveRuntime| {
        let r = record(x);
        r.texts(l::REFUSALS)
            .unwrap()
            .iter()
            .find_map(|t| t.strip_prefix("SOURCE_NOT_GRANTED=").map(str::to_string))
    };
    // dRPC (not granted) first: refused, nothing sent.
    assert!(sighted(SourceId::Drpc, &mut x, 10).is_none());
    assert!(r.requests.recv_timeout(Duration::from_millis(200)).is_err());
    assert_eq!(ungranted(&x).as_deref(), Some("1"));
    // BlockReq (not granted either) later: nothing, and no refusal of
    // its own (it never triggers).
    assert!(sighted(SourceId::BlockReq, &mut x, 11).is_none());
    assert_eq!(ungranted(&x).as_deref(), Some("1"));
    // PublicNode (granted) later: the BUY goes.
    let buy = sighted(SourceId::PublicNode, &mut x, 12).expect("the BUY");
    assert!(r.requests.recv_timeout(Duration::from_secs(2)).is_ok());
    assert_eq!(buy.launch_hash, hash);
    // PublicNode again (a resubscription): no second BUY, no refusal.
    let used = buy_usage(
        &r.state,
        NOW,
        crate::sources::tests::shipped_limits().buy_attempts_total_max,
    )
    .unwrap()
    .used;
    let refusals = |x: &LiveRuntime| {
        let r = record(x);
        let texts: Vec<String> = r
            .texts(l::REFUSALS)
            .unwrap()
            .iter()
            .map(|t| t.to_string())
            .collect();
        texts
    };
    let before = refusals(&x);
    assert!(sighted(SourceId::PublicNode, &mut x, 13).is_none());
    assert_eq!(
        buy_usage(
            &r.state,
            NOW,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        used
    );
    assert_eq!(refusals(&x), before);
    assert!(r.requests.recv_timeout(Duration::from_millis(200)).is_err());
    x.shutdown();
    r.clean();
}

/// BSC3-012: an armed path rests on the one qualification record it
/// was armed with. Another record verified on file (a same-account
/// process rebuilt the chain while it was armed) ends the arm at the
/// next pass, before any further intent; a damaged one too.
#[test]
fn a_qualification_changed_under_an_armed_path_ends_it() {
    let r = Rig::new("live-q-changed", LiveStrategies::C3);
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let mut other = rig_spec(
        Some(crate::live_qualification::running_build().unwrap()),
        LiveStrategies::C3,
    );
    if let Some(risk) = other.risk.as_mut() {
        risk.decided_unix_ms = 2;
        risk.statement = "another synthetic scope".into();
    }
    fs::write(
        r.root.join(LIVE_QUALIFICATION_PATH),
        build(&r.root, &other, 2).unwrap(),
    )
    .unwrap();
    // It verifies (a whole, consistent record), yet it is not the one
    // the arm rests on.
    let (verified, invalid) = crate::live_qualification::view(&r.root);
    assert!(invalid.is_none() && verified.view.risk_decision);
    settle(&mut x, &c2);
    assert!(!x.armed());
    assert_eq!(
        reason(&x),
        (
            "ENDED".to_string(),
            Some(crate::sources::LIVE_QUALIFICATION_CHANGED.to_string())
        )
    );
    r.clean();
}

/// R4: a LIVE BUY rests on the bound builder policy as it holds at each
/// pass: under the repository's frozen import nothing arms; with a fresh
/// one the path arms; its age past 24 h ends a path that holds nothing
/// open (with a lot open: `with_a_lot_open_r4_refusals_close_only_buys`).
#[test]
fn a_live_arm_rests_on_a_fresh_bound_builder_policy() {
    let r = Rig::new("live-builder-policy", LiveStrategies::C3);
    let c2 = c2_at(&r.d, r.buyer, 5);
    let path = r.root.join(crate::builder_policy::TEST_POLICY);
    let fresh = fs::read(&path).unwrap();
    // The repository's: the frozen import, never fresh.
    fs::write(
        &path,
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../../../native/evidence/g06a/g07_binary_import_20261016/builder_policy.dbrec",
            ),
        )
        .unwrap(),
    )
    .unwrap();
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(!x.armed());
    let why = reason(&x).1.unwrap_or_default();
    assert!(
        why.starts_with(crate::sources::LIVE_BUILDER_POLICY_STALE),
        "{why}"
    );
    fs::write(&path, &fresh).unwrap();
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let old = NOW - crate::builder_policy::MAX_AGE_S * 1000 - 1000;
    fs::write(&path, crate::builder_policy::native_bytes(&[old])).unwrap();
    settle(&mut x, &c2);
    assert!(!x.armed());
    let why = reason(&x).1.unwrap_or_default();
    assert!(
        why.starts_with(crate::sources::LIVE_BUILDER_POLICY_STALE),
        "{why}"
    );
    x.shutdown();
    r.clean();
}

/// The tests' lot open from before: a C3 BUY at nonce 3 included.
fn r4_lot(state: &Path) -> [u8; 20] {
    let buy = Fact {
        max_block: Some(HEAD),
        ..unknown_window([0x7c; 32], 3, ComponentId::ExecC3)
    };
    let ledger = Ledger::open(state);
    ledger.append(&buy, 1).unwrap();
    ledger
        .append(
            &Fact {
                block: Some((HEAD - 1, [0x41; 32], NOW)),
                gas_wei: Some(U256::from(1u64)),
                tokens: Some(U256::from(7u64)),
                eth_wei: Some(U256::from(1u64)),
                ..Fact::about(&buy, FactKind::Included)
            },
            2,
        )
        .unwrap();
    buy.token
}

/// Review of R4: with a lot open, an outlier grant and a stale builder
/// policy close BUYs only: the path arms for its exit (BSC3-071).
#[test]
fn with_a_lot_open_r4_refusals_close_only_buys() {
    let mut r = Rig::new("live-r4-exits", LiveStrategies::C3);
    r4_lot(&r.state);
    let c2 = c2_at(&r.d, r.buyer, 5);
    let closed = |r: &Rig, why: &str| {
        let mut x = r.runtime("boot-1");
        settle(&mut x, &c2);
        assert!(x.armed(), "{:?}", reason(&x));
        let b = x.buys_closed.clone().unwrap_or_default();
        assert!(b.starts_with(why), "{b}");
        x.shutdown();
    };
    r.exec.exec.live_risk_soft_buy_slippage_bps = 100;
    closed(&r, crate::sources::LIVE_RISK_OUTLIER_UNACKED);
    r.exec.exec.live_risk_soft_buy_slippage_bps = 9_999;
    let old = NOW - crate::builder_policy::MAX_AGE_S * 1000 - 1000;
    fs::write(
        r.root.join(crate::builder_policy::TEST_POLICY),
        crate::builder_policy::native_bytes(&[old]),
    )
    .unwrap();
    closed(&r, crate::sources::LIVE_BUILDER_POLICY_STALE);
    r.clean();
}

/// Review of R4 (MEDIUM): an exit the strategy decides while the LIVE
/// path is not armed (a re-arm, a reconnect), of a lot the ledger holds,
/// stays wanted and goes on at the next arm; armed, or another token's,
/// it is not held back.
#[test]
fn an_exit_decided_while_unarmed_goes_on_at_the_next_arm() {
    let r = Rig::new("live-unarmed-exit", LiveStrategies::C3);
    let token = r4_lot(&r.state);
    let c2 = c2_at(&r.d, r.buyer, 5);
    let mut x = r.runtime("boot-1");
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert!(!x.want_unarmed_exit(token));
    x.disarm();
    assert!(!x.want_unarmed_exit([0xee; 20]));
    assert!(x.want_unarmed_exit(token));
    assert!(x.exits_wanted.contains(&token));
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    assert_eq!(x.exits_to_continue(&c2), vec![token]);
    x.shutdown();
    r.clean();
}

// ---- REV.1.0 R3 ----

/// BSC3-068: a launch's C3 BUY withdrawn before any byte left (its
/// transport start passed once its lease was taken): both facts name
/// the lease's attempt, and at the next reconcile that attempt's slots
/// go back to the owner (kept beside, never deleted); the arm's next
/// lease is another attempt and takes the freed slot.
#[test]
fn a_withdrawn_c3_buys_slots_go_back_at_the_next_reconcile() {
    withdrawn_c3_buy("live-slot-return", false);
}

/// Review F1: after a quarantine (an entry that may have been a BUY's
/// own fact) no slot goes back, a withdrawn BUY's neither.
#[test]
fn no_slot_goes_back_after_a_quarantine() {
    withdrawn_c3_buy("live-slot-keep", true);
}

fn withdrawn_c3_buy(tag: &str, quarantined: bool) {
    use crate::live_end::LEASE_BOUND;
    use crate::live_ledger::{FactKind, Ledger, fold};
    let r = Rig::granting(tag, LiveStrategies::C3, |now| LiveGrant {
        max_attempts: 1,
        ..rig_grant(now + 3_600_000, LiveStrategies::C3)
    });
    let mut x = r.runtime("boot-1");
    let mut c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let attempt = x
        .executor
        .as_ref()
        .unwrap()
        .end()
        .lease()
        .unwrap()
        .attempt();
    assert_eq!(
        crate::sources::total_usage(
            &r.state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    LEASE_BOUND.with(|b| b.set(false));
    let t = Cell::new(10u64);
    let clock = FnClock {
        id: 1,
        now: || {
            let bound = LEASE_BOUND.with(|b| b.get());
            t.set(t.get() + if bound { 2_000_000_000 } else { 3 });
            t.get()
        },
    };
    let (hash, raw, launcher) = launch();
    c2.pending(hash, launcher, 3, 0, 10);
    assert!(
        x.on_launch(SourceId::PublicNode, hash, [0; 20], &raw, 10, &c2, &clock)
            .is_none()
    );
    LEASE_BOUND.with(|b| b.set(false));
    assert!(r.requests.recv_timeout(Duration::from_millis(300)).is_err());
    let facts = Ledger::open(&r.state).facts().unwrap();
    assert_eq!(
        facts
            .iter()
            .map(|f| (f.kind, f.attempt))
            .collect::<Vec<_>>(),
        vec![
            (FactKind::Withdrawn, Some(attempt)),
            (FactKind::Withdrawn, Some(attempt))
        ]
    );
    assert_eq!(fold(&facts).withdrawn_attempts(), vec![attempt]);
    let a = alloy_primitives::hex::encode(attempt);
    let returns = r.state.join("buy-attempt-returns");
    if quarantined {
        let bad = r.state.join("live-ledger").join(format!("{:020}.dbrec", 2));
        fs::write(&bad, b"torn").unwrap();
        Ledger::open(&r.state)
            .quarantine(2, "q-2", "operator", 0, || Ok(0), NOW)
            .unwrap();
        settle(&mut x, &c2);
        assert!(!returns.exists());
        assert_eq!(
            crate::sources::total_usage(
                &r.state,
                crate::sources::tests::shipped_limits().buy_attempts_total_max
            )
            .unwrap()
            .used,
            1
        );
        x.shutdown();
        r.clean();
        return;
    }
    settle(&mut x, &c2);
    assert!(returns.join(format!("{a}-total-{:010}.dbrec", 0)).exists());
    assert!(x.armed(), "{:?}", reason(&x));
    let next = x
        .executor
        .as_ref()
        .unwrap()
        .end()
        .lease()
        .unwrap()
        .attempt();
    assert_ne!(next, attempt);
    assert_eq!(
        crate::sources::total_usage(
            &r.state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    assert!(
        !record(&x)
            .texts(l::REFUSALS)
            .unwrap()
            .iter()
            .any(|r| r.starts_with("SLOT_RETURN_FAILED"))
    );
    x.shutdown();
    r.clean();
}

/// One released fact of the buyer's `intent` of `token` at `nonce`.
fn r3_fact(n: u8, intent: LiveIntentKind, token: [u8; 20], nonce: u64) -> crate::live_ledger::Fact {
    crate::live_ledger::Fact {
        kind: FactKind::Released,
        tx_hash: [n; 32],
        intent,
        component: ComponentId::StrategyC,
        token,
        nonce,
        grant_generation: 1,
        input_wei: Some(U256::from(1_000u64)),
        max_wei: Some(U256::from(1_300u64)),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(HEAD + 2),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    }
}

/// What became of `f` in a block before the head.
fn r3_chain(f: &crate::live_ledger::Fact, kind: FactKind, n: u8) -> crate::live_ledger::Fact {
    crate::live_ledger::Fact {
        block: Some((HEAD - 20 + u64::from(n), [n; 32], 1_793_000_000_000)),
        gas_wei: Some(U256::from(1u8)),
        tokens: (f.intent == LiveIntentKind::Buy).then(|| U256::from(7u8)),
        eth_wei: (f.intent == LiveIntentKind::Buy).then(|| U256::from(1_000u64)),
        ..crate::live_ledger::Fact::about(f, kind)
    }
}

/// R3: the operator's decisions written by another process (`bsc_runtime
/// live lot`) reach the running LIVE path at its next reconcile: a
/// resumed lot's stopped exit opens again and is wanted, its token named
/// for a new curve read; an exit request is held back (counted) while
/// the operator exit is OFF and wanted by a process with it ON; a lot
/// written off ends for the strategy, also one never reported (held
/// again after a restart).
#[test]
fn the_operators_lot_decisions_reach_the_running_runtime() {
    use crate::live_ledger::{Ledger, LotDecision};
    let mut r = Rig::new("live-decide", LiveStrategies::Both);
    let ledger = Ledger::open(&r.state);
    let (a, b) = ([0xA1; 20], [0xB1; 20]);
    let buy_a = r3_fact(1, LiveIntentKind::Buy, a, 0);
    let approve = r3_fact(2, LiveIntentKind::Approve, a, 1);
    let sell = r3_fact(3, LiveIntentKind::Sell, a, 2);
    let buy_b = r3_fact(4, LiveIntentKind::Buy, b, 3);
    for f in [
        buy_a.clone(),
        r3_chain(&buy_a, FactKind::Included, 1),
        approve.clone(),
        r3_chain(&approve, FactKind::Reverted, 2),
        sell.clone(),
        r3_chain(&sell, FactKind::Reverted, 3),
        buy_b.clone(),
        r3_chain(&buy_b, FactKind::Included, 4),
    ] {
        ledger.append(&f, 1).unwrap();
    }
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 4);
    settle(&mut x, &c2);
    assert_eq!(
        x.exit_stopped(&a),
        Some(crate::sources::LIVE_EXIT_REVERTS_EXHAUSTED)
    );
    // The lots open from before count as reported: nothing to tell.
    assert_eq!(x.take_decided(), (Vec::new(), Vec::new()));
    ledger
        .decide(
            [1; 32],
            LotDecision::Resume,
            "resume-a",
            "operator",
            false,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    settle(&mut x, &c2);
    let (_, resumed) = x.take_decided();
    assert_eq!(resumed, vec![a]);
    assert_eq!(x.exit_stopped(&a), None);
    assert!(x.exits_wanted.contains(&a));
    // Exit-now written while this process runs with the switch OFF.
    ledger
        .decide(
            [4; 32],
            LotDecision::ExitNow,
            "exit-b",
            "operator",
            true,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    settle(&mut x, &c2);
    settle(&mut x, &c2);
    assert!(!x.exits_wanted.contains(&b));
    let status = record(&x);
    let refusals = status.texts(l::REFUSALS).unwrap();
    assert!(refusals.contains(&"OPERATOR_EXIT_LOCKED=1"), "{refusals:?}");
    x.shutdown();
    r.exec.exec.live_operator_exit = true;
    let mut y = r.runtime("boot-1");
    settle(&mut y, &c2);
    assert!(y.exits_wanted.contains(&b));
    y.shutdown();
    // A new process (B never reported to it) sees B written off.
    let lot_b = [4; 32];
    let mut z = r.runtime("boot-1");
    settle(&mut z, &c2);
    let _ = z.take_decided();
    // B's exit is wanted but nothing of it is out: the write-off holds.
    ledger
        .decide(
            lot_b,
            LotDecision::WriteOff,
            "off-b",
            "operator",
            false,
            ledger.version().unwrap(),
            NOW,
        )
        .unwrap();
    // Review F3: another path folds it first; the next fold of the
    // decisions still reports it (B, open from before, counts as
    // reported: it ends).
    z.refold();
    z.fold_decisions();
    let (changes, _) = z.take_decided();
    assert!(
        changes.contains(&PositionChange::Ended { token: b }),
        "{changes:?}"
    );
    assert!(
        z.open_positions()
            .iter()
            .all(|p| !matches!(p, PositionChange::Opened { token, .. } if *token == b))
    );
    assert!(!z.exits_wanted.contains(&b));
    z.shutdown();
    r.clean();
}

/// BSC3-045: an entry of the ledger the operator quarantined (it did
/// not decode) may have said what holds BUYs back: no BUY under a grant
/// written before it (no lease either); the operator's new grant
/// decides anew.
#[test]
fn a_quarantined_entry_refuses_the_buys_of_the_grants_before_it() {
    use crate::live_ledger::Ledger;
    let r = Rig::new("live-quarantine", LiveStrategies::C3);
    let live = r.state.join("live-ledger");
    fs::create_dir_all(&live).unwrap();
    fs::write(live.join(format!("{:020}.dbrec", 0)), b"torn").unwrap();
    let s = Store::open_in_session(
        &r.state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = s.current().unwrap().generation;
    Ledger::open(&r.state)
        .quarantine(0, "q-0", "operator", g, || Ok(g), NOW)
        .unwrap();
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 5);
    settle(&mut x, &c2);
    assert!(x.armed(), "{:?}", reason(&x));
    let refusal = x.buy_refusal(x.one_buy(), Some(&c2), x.free_nonce(&c2));
    assert_eq!(
        refusal,
        Some((
            crate::paper_exec::Refusal::GrantLimit,
            Some(crate::sources::LIVE_LEDGER_QUARANTINED)
        ))
    );
    assert!(leases(&r.state).is_empty());
    x.shutdown();
    r.authorize("boot-1", "after-quarantine");
    let mut y = r.runtime("boot-1");
    settle(&mut y, &c2);
    assert!(y.armed(), "{:?}", reason(&y));
    assert_eq!(
        y.buy_refusal(y.one_buy(), Some(&c2), y.free_nonce(&c2)),
        None
    );
    assert_eq!(leases(&r.state).len(), 1);
    y.shutdown();
    r.clean();
}

/// R3 re-check RC1: while the ledger does not read (its fold empty),
/// no lot ends for the strategy and no wanted exit is dropped; once it
/// reads again, nothing changed.
#[test]
fn an_unreadable_ledger_ends_no_lot_for_the_strategy() {
    use crate::live_ledger::Ledger;
    let r = Rig::new("live-r3-unreadable", LiveStrategies::Both);
    let ledger = Ledger::open(&r.state);
    let a = [0xA1; 20];
    let buy = r3_fact(1, LiveIntentKind::Buy, a, 0);
    ledger.append(&buy, 1).unwrap();
    ledger
        .append(&r3_chain(&buy, FactKind::Included, 1), 1)
        .unwrap();
    let mut x = r.runtime("boot-1");
    let c2 = c2_at(&r.d, r.buyer, 1);
    settle(&mut x, &c2);
    x.exits_wanted.push(a);
    let torn = r.state.join("live-ledger").join(format!("{:020}.dbrec", 2));
    fs::write(&torn, b"torn").unwrap();
    x.refold();
    assert!(x.ledger_error.is_some());
    x.fold_decisions();
    assert_eq!(x.take_decided(), (Vec::new(), Vec::new()));
    assert_eq!(x.exits_wanted, vec![a]);
    fs::remove_file(&torn).unwrap();
    x.refold();
    x.fold_decisions();
    assert_eq!(x.take_decided(), (Vec::new(), Vec::new()));
    assert_eq!(x.exits_wanted, vec![a]);
    x.shutdown();
    r.clean();
}
