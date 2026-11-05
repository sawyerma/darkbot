use super::*;
use crate::paper_exec::tests::{
    HEAD, c2_next, clock, config, curve_after, dir, flap, launch, strategy_c2_next, validated,
};
use crate::paper_exec::{Arming, Executor, IntentKind, PendingLaunch, StrategyCurve};
use crate::sources::tests::{grant_of, live_ready_granting};
use crate::sources::{
    Entity, LIVE_GRANT_CHANGED, LiveGrant, LiveStrategies, Mode, buy_usage, spent_by_grant,
};
use alloy_primitives::keccak256;
use darkbot_bsc_trading::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, WITNESSED_MAINNET_TERMS, quote_native_buy,
};
use darkbot_bsc_trading::signer::recover_legacy_sender;
use darkbot_bsc_trading::submission_policy::capability::{CapabilityId, Knowledge, SubmissionMode};
use darkbot_bsc_trading::submission_policy::qualification::{
    AdapterQualification, FeatureName, Finding, PropertyName,
};
use darkbot_bsc_trading::transactions::legacy::decode_signed;
use darkbot_bsc_trading::venues::flap::abi::decode_swap_exact_input_native;
use darkbot_bsc_trading::venues::flap::deployment::bsc_mainnet_v5_27_2;
use std::fs;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

/// How the stand-in answers one request (BSC3-051).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Answer {
    /// HTTP 200, the request's id, a bundle hash.
    Ack,
    /// HTTP 200, the request's id, a JSON-RPC error.
    Reject,
    /// HTTP 429.
    RateLimited,
    /// HTTP 200 answering another request.
    WrongId,
    Silent,
}

/// The answer to `request` (its JSON-RPC id echoed), if any.
fn reply(answer: Answer, request: &[u8]) -> Option<Vec<u8>> {
    let text = String::from_utf8_lossy(request);
    let id = text
        .split("\"id\":\"")
        .nth(1)?
        .split('"')
        .next()?
        .to_string();
    let hash = format!("0x{}", "1f".repeat(32));
    let json = |body: String| {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    };
    match answer {
        Answer::Ack => Some(json(format!(
            r#"{{"jsonrpc":"2.0","id":"{id}","result":"{hash}"}}"#
        ))),
        Answer::Reject => Some(json(format!(
            r#"{{"jsonrpc":"2.0","id":"{id}","error":{{"code":-32000,"message":"bundle refused"}}}}"#
        ))),
        Answer::RateLimited => {
            Some(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".to_vec())
        }
        Answer::WrongId => Some(json(format!(
            r#"{{"jsonrpc":"2.0","id":"0x{}","result":"{hash}"}}"#,
            "00".repeat(32)
        ))),
        Answer::Silent => None,
    }
}

/// The builder stand-in on 127.0.0.1: every complete HTTP request it
/// reads, whole. It answers nothing.
pub(crate) fn stand_in() -> (u16, mpsc::Receiver<Vec<u8>>) {
    answering(Vec::new())
}

/// [`stand_in`] answering its `n`th request with `answers[n]` (after
/// them: nothing).
pub(crate) fn answering(answers: Vec<Answer>) -> (u16, mpsc::Receiver<Vec<u8>>) {
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    let answers = std::sync::Arc::new(answers);
    let next = std::sync::Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let tx = tx.clone();
            let (answers, next) = (answers.clone(), next.clone());
            std::thread::spawn(move || {
                let (mut buf, mut chunk) = (Vec::new(), [0u8; 4096]);
                loop {
                    if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..end]).to_string();
                        let len: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("Content-Length: "))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            let rest = buf.split_off(end + 4 + len);
                            let request = std::mem::replace(&mut buf, rest);
                            let n = next.fetch_add(1, Ordering::SeqCst);
                            let answer = answers.get(n).copied().unwrap_or(Answer::Silent);
                            if let Some(r) = reply(answer, &request) {
                                let _ = stream.write_all(&r);
                            }
                            if tx.send(request).is_err() {
                                return;
                            }
                            continue;
                        }
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
            });
        }
    });
    (port, rx)
}

/// 48Club measured for the birth entry and the single transaction.
pub(crate) fn club48_findings() -> Vec<Finding> {
    let properties = [
        (PropertyName::PreservesOrder, Knowledge::Yes),
        (PropertyName::Atomic, Knowledge::Yes),
        (PropertyName::SameBlock, Knowledge::Yes),
        (PropertyName::ExtractsTransactions, Knowledge::No),
        (PropertyName::ProviderRebroadcast, Knowledge::No),
        (PropertyName::ProviderPublicFallback, Knowledge::No),
        (PropertyName::ReachesOtherBuilders, Knowledge::No),
        (PropertyName::PublicMempoolExposure, Knowledge::No),
    ];
    properties
        .into_iter()
        .map(|(p, k)| Finding::Property(p, k))
        .chain([
            Finding::Feature(FeatureName::NoMerge),
            Finding::Feature(FeatureName::PositionFirst),
            Finding::Feature(FeatureName::MaxBlockNumber),
            Finding::Mode(SubmissionMode::SingleRawBuy),
            Finding::SendTime,
        ])
        .collect()
}

/// [`verified`] for the scope of the grant `s` holds (BSC3-049: its
/// risk decided for it).
pub(crate) fn verified_in(s: &Store) -> Verified {
    verified_as(crate::sources::tests::qualified_in(s))
}

fn verified_as(view: crate::sources::LiveQualification) -> Verified {
    let findings = club48_findings();
    Verified {
        runtime_sha256: crate::live_qualification::running_build().ok(),
        view,
        adapters: vec![
            AdapterQualification::from_findings(CapabilityId::Club48BuilderV1, &findings).unwrap(),
        ],
        sha256: [0; 32],
        acquisition_buyer: Some(crate::live_qualification::tests::buyer()),
        observed_unix_ms: 0,
    }
}

/// A private buyer key file in `d`, and its address.
pub(crate) fn key_file(d: &Path) -> (PathBuf, [u8; 20]) {
    let keys = d.join("keys");
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&keys)
        .unwrap();
    let path = keys.join("buyer.key");
    let secret = crate::live_qualification::tests::BUYER_SECRET;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(&secret))
        .unwrap();
    let buyer = Signer::from_secret_bytes(&secret).unwrap().address();
    (path, buyer.into_array())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// BSC3-076: the owner's lease of the next C3 BUY at `nonce`, as the
/// LIVE runtime takes it before a launch (the economics of
/// [`config`]: the configured input, its gas and its own exit).
pub(crate) fn lease_c3(e: &mut Executor<LiveEnd>, nonce: u64) {
    let c = config(2, 2);
    let gas = |limit: u64| u128::from(c.gas_price_wei) * u128::from(limit);
    let want = LeaseWant {
        nonce,
        input_wei: c.input_wei,
        max_wei: c.input_wei + gas(c.gas_limit),
        exit_reserve_wei: gas(c.approve_gas_limit) + gas(c.sell_gas_limit),
        head: HEAD.number,
        validity_blocks: c.validity_blocks,
        lease_blocks: c.live_lease_blocks,
        attempt: None,
    };
    e.end_mut().hold_lease(&want).unwrap();
}

/// BSC3-025: a builder connection is begun and installed later, never
/// waited for: with a builder that never answers the handshake (Q5A
/// waits up to 5 s for it), beginning, the readiness check, the
/// maintenance and the install before a decision all return at once.
#[test]
fn no_pass_waits_for_a_builder_handshake() {
    use std::net::TcpListener;
    use std::time::Instant;
    let (state, store, now) = live_ready_granting("live-end-025", |now| LiveGrant {
        adapters: vec![LiveAdapter::Club48BuilderV1],
        ..grant_of(now + 3_600_000, LiveStrategies::C3)
    });
    let q = verified_in(&store);
    let armed = store
        .live_now(&[ComponentId::ExecC3], &q.view, now)
        .unwrap();
    let d = std::env::temp_dir().join(format!("bsc-live-end-025-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    let (key_path, buyer) = key_file(&d);
    let key = LiveKey::load(
        &key_path,
        Path::new("/nonexistent-repo"),
        buyer,
        &[],
        &armed,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    BUILDERS_AT.with(|at| at.set(Some(listener.local_addr().unwrap())));
    let mut end = LiveEnd::new(
        key,
        armed,
        store,
        q,
        LiveRoute::Builders {
            setup: TlsSetup::web_pki_roots().unwrap(),
            blockrazor: None,
        },
        56,
        1,
        Box::new(move || now),
    )
    .unwrap();
    let wait = Duration::from_secs(2);
    let started = Instant::now();
    assert!(end.begin_connect(wait).is_empty());
    assert_eq!(end.connected(), Connecting::Pending);
    assert!(end.maintain(wait).is_empty());
    end.install_ready();
    assert_eq!(end.connected(), Connecting::Pending);
    // A liveness margin, not a timing claim: the handshake takes 5 s.
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    BUILDERS_AT.with(|at| at.set(None));
    drop(end);
    drop(listener);
    let _ = fs::remove_dir_all(&d);
    let _ = fs::remove_dir_all(&state);
}

/// BSC3-031: the request id the release writes is the lowercase hex of
/// the transaction hash, byte for byte as before (one allocation now,
/// measured: see the commit).
#[test]
fn the_request_id_is_the_hash_in_lowercase_hex() {
    for hash in [[0u8; 32], [0xff; 32], {
        let mut h = [0u8; 32];
        for (i, b) in h.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37);
        }
        h
    }] {
        assert_eq!(alloy_primitives::hex::encode(hash), hex(&hash));
    }
}

/// Plan 28.10.: under an effective LIVE the one executor's C3 BUY leaves
/// as the builder bundle behind the launch, a Strategy C BUY and its
/// approval and SELL each as a one-transaction bundle, all signed by
/// the buyer key and each admitted by the owner first (attempt slots,
/// spend). A request goes only to the loopback stand-in. Once the grant
/// is revoked, the owner admits nothing and nothing leaves.
#[test]
fn live_intents_leave_through_the_release_only_as_the_owner_admits_them() {
    let (state, store, now) = live_ready_granting("live-end", |now| LiveGrant {
        max_input_wei: 10_000_000_000_000_000,
        max_spend_wei: 100_000_000_000_000_000,
        ..grant_of(now + 3_600_000, LiveStrategies::Both)
    });
    let q = verified_in(&store);
    let armed = store
        .live_now(&[ComponentId::ExecC3, ComponentId::StrategyC], &q.view, now)
        .unwrap();
    let generation = armed.grant_generation();
    let d = dir("live-end");
    let (key_path, buyer) = key_file(&d);
    let key = LiveKey::load(
        &key_path,
        Path::new("/nonexistent-repo"),
        buyer,
        &[],
        &armed,
    )
    .unwrap();
    let (port, requests) = stand_in();
    let mut cfg = config(2, 2);
    // The BUY, its approval and SELL, and one more BUY.
    cfg.strategy_lanes = 5;
    // BSC3-049: an acquisition proven for another buyer arms nothing.
    let foreign = Verified {
        acquisition_buyer: Some([9; 20]),
        ..q.clone()
    };
    let other = LiveKey::load(
        &key_path,
        Path::new("/nonexistent-repo"),
        buyer,
        &[],
        &armed,
    )
    .unwrap();
    let refused = LiveEnd::new(
        other,
        armed.clone(),
        Store::open_in_session(
            &state,
            crate::sources::tests::keeping(8),
            Ok("boot-1".into()),
        )
        .unwrap(),
        foreign,
        LiveRoute::Loopback { port },
        56,
        cfg.capability_generation,
        Box::new(move || now),
    );
    assert!(
        refused
            .err()
            .unwrap()
            .contains(crate::sources::LIVE_ACQUISITION_UNPROVEN)
    );
    let end = LiveEnd::new(
        key,
        armed,
        store,
        q,
        LiveRoute::Loopback { port },
        56,
        cfg.capability_generation,
        Box::new(move || now),
    )
    .unwrap();
    let mut exec = Executor::arm_end(
        end,
        validated(),
        WITNESSED_MAINNET_TERMS,
        cfg,
        Arming {
            next_nonce: 0,
            fence_dir: &d.join("fences"),
            session: 1,
            arm: 0,
        },
    )
    .unwrap();
    let wait = std::time::Duration::from_secs(2);
    let reconnect = |e: &mut Executor<LiveEnd>| {
        assert!(e.end_mut().connect(wait).is_empty());
    };
    reconnect(&mut exec);
    // C3: the bundle behind the launch.
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input,
        ingress_ns: 100,
    };
    lease_c3(&mut exec, 0);
    let buy = exec
        .on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
        .unwrap();
    let request = String::from_utf8(requests.recv_timeout(wait).unwrap()).unwrap();
    assert!(request.contains(&format!(
        "\"txs\":[\"0x{}\",\"0x{}\"]",
        hex(&raw),
        hex(&buy.raw)
    )));
    assert!(request.contains("\"noMerge\":true,\"positionFirst\":true"));
    assert_eq!(buy.handoff.tx_hash, keccak256(&buy.raw).0);
    assert_eq!(buy.handoff.adapters, 1);
    let signed = decode_signed(&buy.raw).unwrap();
    let from = recover_legacy_sender(&signed, 56, &mut Vec::new()).unwrap();
    assert_eq!(from.into_array(), buyer);
    let c3_most = 1_000_000_000_000_000u128 + 1_000_000_000 * 300_000;
    assert_eq!(
        buy_usage(
            &state,
            now,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    assert_eq!(spent_by_grant(&state, generation).unwrap(), c3_most);
    // Strategy C, once nonce 0 is canonical: the BUY alone, then its
    // approval and SELL.
    reconnect(&mut exec);
    let token = [0xAB; 20];
    let input = U256::from(10_000_000_000_000_000u64);
    let curve = curve_after(1_000_000_000_000_000_000);
    let entry = exec
        .on_strategy_buy(
            token,
            [0; 20],
            curve,
            input,
            &strategy_c2_next(Some(HEAD), 1),
            &clock(500, 3),
            500,
        )
        .unwrap();
    let request = String::from_utf8(requests.recv_timeout(wait).unwrap()).unwrap();
    assert!(request.contains(&format!("\"txs\":[\"0x{}\"]", hex(&entry.raw))));
    assert_eq!(
        buy_usage(
            &state,
            now,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        2
    );
    let call = decode_swap_exact_input_native(decode_signed(&entry.raw).unwrap().tx.input).unwrap();
    let tokens = call.min_output_amount;
    let q = quote_native_buy(&NativeBuyInputs {
        token: flap(curve).0,
        reserve_before: flap(curve).1,
        circulating_before: flap(curve).2,
        input_amount: input,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let held = StrategyCurve::Flap {
        reserve: q.reserve_after,
        circulating: q.supply_after,
        terms: flap(curve).0,
    };
    reconnect(&mut exec);
    // The day's BUYs are taken; the exit needs none. Once the BUY is
    // canonical its approval goes; its SELL waits for the approval to
    // be canonical, then goes alone.
    let ctx = strategy_c2_next(Some(HEAD), 2);
    let (out, refused) = exec.on_strategy_exit(token, tokens, held, &ctx, &clock(900, 3), 900);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, IntentKind::Approve);
    assert_eq!(refused, None);
    assert!(!exec.exited(&token));
    let approval = requests.recv_timeout(wait).unwrap();
    assert!(
        String::from_utf8(approval)
            .unwrap()
            .contains(&hex(&out[0].raw))
    );
    reconnect(&mut exec);
    let ctx = strategy_c2_next(Some(HEAD), 3);
    let (out, refused) = exec.on_strategy_exit(token, tokens, held, &ctx, &clock(950, 3), 950);
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    let sell = requests.recv_timeout(wait).unwrap();
    assert!(String::from_utf8(sell).unwrap().contains(&hex(&out[0].raw)));
    let released = exec.end().released();
    assert_eq!(released.len(), 4);
    assert!(released.iter().all(|r| r.handed_off));
    // Revoked: the owner admits nothing more, nothing leaves.
    let owner = Store::open_in_session(
        &state,
        crate::sources::tests::keeping(8),
        Ok("boot-1".into()),
    )
    .unwrap();
    let g = owner.current().unwrap().generation;
    let r = owner
        .request("revoke", Entity::LiveGrant, Mode::Disabled, g, now)
        .unwrap();
    assert!(r.applied());
    reconnect(&mut exec);
    let refused = exec.on_strategy_buy(
        [0xCD; 20],
        [0; 20],
        curve,
        input,
        &strategy_c2_next(Some(HEAD), 4),
        &clock(1_000, 3),
        1_000,
    );
    assert_eq!(refused.unwrap_err(), Refusal::Owner);
    assert_eq!(exec.end().last_refusal(), Some(LIVE_GRANT_CHANGED));
    assert!(
        requests
            .recv_timeout(std::time::Duration::from_millis(300))
            .is_err()
    );
    assert_eq!(
        buy_usage(
            &state,
            now,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        2
    );
    assert_eq!(exec.end().released().len(), 4);
    // The key file was only read.
    assert_eq!(fs::read(&key_path).unwrap(), [0x33u8; 32]);
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// An executor armed towards the LIVE end of a grant for both
/// strategies at nonce 0, on the loopback stand-in: the owner's state,
/// the run directory, the executor and what the stand-in receives.
fn armed_both(tag: &str) -> (PathBuf, PathBuf, Executor<LiveEnd>, mpsc::Receiver<Vec<u8>>) {
    armed_answering(tag, Vec::new())
}

/// [`armed_both`] on a stand-in that answers with `answers`.
fn armed_answering(
    tag: &str,
    answers: Vec<Answer>,
) -> (PathBuf, PathBuf, Executor<LiveEnd>, mpsc::Receiver<Vec<u8>>) {
    armed_clocked(tag, answers, 3_600_000, |now| Box::new(move || now))
}

/// [`armed_answering`] under a grant valid for `valid_ms`, its wall
/// clock what `wall` makes of the owner's `now`.
fn armed_clocked(
    tag: &str,
    answers: Vec<Answer>,
    valid_ms: u64,
    wall: impl FnOnce(u64) -> Box<dyn Fn() -> u64 + Send>,
) -> (PathBuf, PathBuf, Executor<LiveEnd>, mpsc::Receiver<Vec<u8>>) {
    let (state, store, now) = live_ready_granting(tag, |now| LiveGrant {
        max_input_wei: 10_000_000_000_000_000,
        max_spend_wei: 100_000_000_000_000_000,
        ..grant_of(now + valid_ms, LiveStrategies::Both)
    });
    let q = verified_in(&store);
    let armed = store
        .live_now(&[ComponentId::ExecC3, ComponentId::StrategyC], &q.view, now)
        .unwrap();
    let d = dir(tag);
    let (key_path, buyer) = key_file(&d);
    let key = LiveKey::load(
        &key_path,
        Path::new("/nonexistent-repo"),
        buyer,
        &[],
        &armed,
    )
    .unwrap();
    let (port, requests) = answering(answers);
    let mut cfg = config(2, 2);
    cfg.strategy_lanes = 4;
    let end = LiveEnd::new(
        key,
        armed,
        store,
        q,
        LiveRoute::Loopback { port },
        56,
        cfg.capability_generation,
        wall(now),
    )
    .unwrap();
    let mut exec = Executor::arm_end(
        end,
        validated(),
        WITNESSED_MAINNET_TERMS,
        cfg,
        Arming {
            next_nonce: 0,
            fence_dir: &d.join("fences"),
            session: 1,
            arm: 0,
        },
    )
    .unwrap();
    assert!(
        exec.end_mut()
            .connect(std::time::Duration::from_secs(2))
            .is_empty()
    );
    (state, d, exec, requests)
}

fn reconnect(e: &mut Executor<LiveEnd>) {
    assert!(
        e.end_mut()
            .connect(std::time::Duration::from_secs(2))
            .is_empty()
    );
}

/// BSC3-055: one sender's LIVE transactions reach the chain one nonce
/// after the other (Unit 4 invariant 3 across the sender's lanes).
/// While the C3 BUY at nonce 0 is not canonical, the strategy's BUY at
/// nonce 1 is refused and nothing of it leaves; once C2 shows nonce 0
/// consumed it goes. An exit's approval goes at its nonce; its SELL
/// waits for the approval to be canonical, and then goes alone. A
/// refusal for the order consumes nothing.
#[test]
fn one_sender_releases_one_nonce_after_the_other() {
    let (state, d, mut exec, requests) = armed_both("live-order");
    let wait = std::time::Duration::from_secs(2);
    let silent = std::time::Duration::from_millis(300);
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input,
        ingress_ns: 100,
    };
    // C2 not clean: no nonce is known, nothing goes.
    assert_eq!(
        exec.on_launch(
            pending,
            &crate::paper_exec::tests::c2(Some(HEAD)),
            &clock(100, 7)
        )
        .unwrap_err(),
        Refusal::SenderNonce
    );
    assert!(requests.recv_timeout(silent).is_err());
    lease_c3(&mut exec, 0);
    exec.on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
        .unwrap();
    requests.recv_timeout(wait).unwrap();
    reconnect(&mut exec);
    let token = [0xAB; 20];
    let input = U256::from(10_000_000_000_000_000u64);
    let curve = curve_after(1_000_000_000_000_000_000);
    let buy = |exec: &mut Executor<LiveEnd>, next: u64| {
        exec.on_strategy_buy(
            token,
            [0; 20],
            curve,
            input,
            &strategy_c2_next(Some(HEAD), next),
            &clock(500, 3),
            500,
        )
    };
    // Nonce 0 is not canonical yet: nonce 1 does not go.
    assert_eq!(buy(&mut exec, 0).unwrap_err(), Refusal::SenderNonce);
    assert!(requests.recv_timeout(silent).is_err());
    assert_eq!(exec.strategy_next_nonce(), 1);
    // Nonce 0 consumed: the BUY at nonce 1.
    let entry = buy(&mut exec, 1).unwrap();
    assert_eq!(decode_signed(&entry.raw).unwrap().tx.nonce, 1);
    requests.recv_timeout(wait).unwrap();
    reconnect(&mut exec);
    let tokens = decode_swap_exact_input_native(decode_signed(&entry.raw).unwrap().tx.input)
        .unwrap()
        .min_output_amount;
    let q = quote_native_buy(&NativeBuyInputs {
        token: flap(curve).0,
        reserve_before: flap(curve).1,
        circulating_before: flap(curve).2,
        input_amount: input,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        tax_on_bonding_curve_enabled: true,
        caller_fee_exempt: false,
        holding_cap: HoldingCap::None,
    })
    .unwrap();
    let held = StrategyCurve::Flap {
        reserve: q.reserve_after,
        circulating: q.supply_after,
        terms: flap(curve).0,
    };
    let exit = |exec: &mut Executor<LiveEnd>, next: u64| {
        exec.on_strategy_exit(
            token,
            tokens,
            held,
            &strategy_c2_next(Some(HEAD), next),
            &clock(900, 3),
            900,
        )
    };
    // The BUY at 1 is not canonical: the approval at 2 does not go.
    let (out, refused) = exit(&mut exec, 1);
    assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
    assert!(requests.recv_timeout(silent).is_err());
    // It is: the approval goes, the SELL waits for it; the exit is not
    // over.
    let (out, refused) = exit(&mut exec, 2);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, IntentKind::Approve);
    assert_eq!(decode_signed(&out[0].raw).unwrap().tx.nonce, 2);
    assert_eq!(refused, None);
    assert!(!exec.exited(&token));
    requests.recv_timeout(wait).unwrap();
    assert!(requests.recv_timeout(silent).is_err());
    assert_eq!(exec.exits_awaiting_sell(), vec![token]);
    // The approval not canonical yet: still no SELL.
    let (out, refused) = exit(&mut exec, 2);
    assert_eq!((out.len(), refused), (0, Some(Refusal::SenderNonce)));
    assert!(requests.recv_timeout(silent).is_err());
    // Canonical, but the connection still awaits the approval's
    // answer: refused before anything is consumed.
    let (out, refused) = exit(&mut exec, 3);
    assert_eq!((out.len(), refused), (0, Some(Refusal::Unqualified)));
    assert!(requests.recv_timeout(silent).is_err());
    assert_eq!(exec.strategy_next_nonce(), 3);
    reconnect(&mut exec);
    // On a ready connection: the SELL alone, at 3.
    let (out, refused) = exit(&mut exec, 3);
    assert_eq!((out.len(), refused), (1, None));
    assert_eq!(out[0].kind, IntentKind::Sell);
    assert_eq!(decode_signed(&out[0].raw).unwrap().tx.nonce, 3);
    requests.recv_timeout(wait).unwrap();
    assert_eq!(exec.exits_awaiting_sell(), Vec::<[u8; 20]>::new());
    // Four transactions, each handed off once, in nonce order.
    let nonces: Vec<u64> = exec.end().released().iter().map(|r| r.nonce).collect();
    assert_eq!(nonces, vec![0, 1, 2, 3]);
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-068: the transport start deadline is checked again right before
/// the owner's admission: an intent already late takes no BUY attempt,
/// no spend and leaves no fact. A deadline that passes after its
/// admission (before the write) still counts the attempt: no byte left,
/// the fact is WITHDRAWN, never on chain, no position, no cost.
/// BSC3-076: a launch's BUY late at its lease takes nothing from it (the
/// lease's one attempt stays held for the next launch); a block-driven
/// intent's limit runs from its admission.
#[test]
fn a_late_intent_takes_no_attempt_and_a_miss_after_its_admission_counts() {
    use crate::live_ledger::fold;
    use crate::sources::total_usage;
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let wait = std::time::Duration::from_secs(2);
    let silent = std::time::Duration::from_millis(300);
    let (hash, raw, input) = launch(0, 7);
    let pending = || PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input,
        ingress_ns: 100,
    };
    // A launch's BUY late before its lease is taken: nothing more taken
    // (the lease's attempt and spend only), nothing written.
    let (state, d, mut exec, requests) = armed_both("live-late");
    lease_c3(&mut exec, 0);
    let leased = crate::sources::admitted_intents(&state).unwrap();
    assert_eq!(leased.len(), 1);
    assert!(leased[0].lease);
    let late = clock(100 + 2_000_000, 1);
    let refused = exec.on_launch(pending(), &c2_next(Some(HEAD), 0), &late);
    assert_eq!(refused.unwrap_err(), Refusal::Deadline);
    assert!(requests.recv_timeout(silent).is_err());
    assert_eq!(
        total_usage(
            &state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    assert_eq!(crate::sources::admitted_intents(&state).unwrap(), leased);
    assert!(Ledger::open(&state).facts().unwrap().is_empty());
    // The lease is still held: the next launch in time (another token;
    // the late one's is used) takes it, under the same attempt.
    assert!(exec.end().lease().is_some());
    let (hash_b, raw_b, input_b) = launch(1, 7);
    let next = PendingLaunch {
        hash: hash_b,
        raw: &raw_b,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input_b,
        ingress_ns: 100,
    };
    let buy = exec
        .on_launch(next, &c2_next(Some(HEAD), 0), &clock(100, 7))
        .unwrap();
    requests.recv_timeout(wait).unwrap();
    assert_eq!(
        total_usage(
            &state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    assert!(exec.end().lease().is_none());
    let facts = Ledger::open(&state).facts().unwrap();
    assert_eq!(
        facts
            .iter()
            .map(|f| (f.kind, f.tx_hash))
            .collect::<Vec<_>>(),
        vec![
            (FactKind::Released, buy.handoff.tx_hash),
            (FactKind::Withdrawn, leased[0].tx_hash)
        ]
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
    // Strategy C's BUY late only once the owner admitted it (its entry
    // exists): its limit runs from the admission, which it misses too.
    let (state, d, mut exec, requests) = armed_both("live-late-after");
    let spend = state.join("live-spend");
    let t = std::cell::Cell::new(500u64);
    let after = crate::paper_exec::FnClock {
        id: 1,
        now: || {
            let admitted = fs::read_dir(&spend).is_ok_and(|mut e| e.next().is_some());
            t.set(t.get() + if admitted { 2_000_000 } else { 7 });
            t.get()
        },
    };
    let refused = exec.on_strategy_buy(
        [0xAB; 20],
        [0; 20],
        curve_after(1_000_000_000_000_000_000),
        U256::from(10_000_000_000_000_000u64),
        &strategy_c2_next(Some(HEAD), 0),
        &after,
        500,
    );
    assert_eq!(refused.unwrap_err(), Refusal::Deadline);
    assert!(requests.recv_timeout(silent).is_err());
    assert!(exec.end().released().is_empty());
    assert_eq!(
        total_usage(
            &state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    let facts = Ledger::open(&state).facts().unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(
        (facts[0].kind, facts[0].intent, facts[0].nonce),
        (FactKind::Withdrawn, LiveIntentKind::Buy, 0)
    );
    let book = fold(&facts);
    assert_eq!(book.unresolved().count(), 0);
    assert!(book.positions.is_empty());
    assert_eq!(
        Ledger::open(&state)
            .recover(&crate::sources::admitted_intents(&state).unwrap(), 1)
            .unwrap()
            .unknown,
        0
    );
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-076 review: a launch's BUY whose start limit passes after it
/// took its lease (before the write): no byte left, its fact and the
/// lease's WITHDRAWN, the lease's attempt counted (BSC3-068), the lease
/// gone from the end.
#[test]
fn a_buy_late_after_its_lease_was_taken_withdraws_both() {
    use crate::live_ledger::fold;
    let (state, d, mut exec, requests) = armed_both("live-lease-late");
    let silent = std::time::Duration::from_millis(300);
    lease_c3(&mut exec, 0);
    let leased = crate::sources::admitted_intents(&state).unwrap();
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input,
        ingress_ns: 100,
    };
    LEASE_BOUND.with(|b| b.set(false));
    let t = std::cell::Cell::new(100u64);
    let late = crate::paper_exec::FnClock {
        id: 1,
        now: || {
            let bound = LEASE_BOUND.with(|b| b.get());
            t.set(t.get() + if bound { 2_000_000 } else { 7 });
            t.get()
        },
    };
    let refused = exec.on_launch(pending, &c2_next(Some(HEAD), 0), &late);
    assert_eq!(refused.unwrap_err(), Refusal::Deadline);
    assert!(requests.recv_timeout(silent).is_err());
    assert!(exec.end().lease().is_none());
    let facts = Ledger::open(&state).facts().unwrap();
    assert_eq!(
        facts.iter().map(|f| f.kind).collect::<Vec<_>>(),
        vec![FactKind::Withdrawn, FactKind::Withdrawn]
    );
    assert_eq!(facts[1].tx_hash, leased[0].tx_hash);
    let book = fold(&facts);
    assert_eq!(book.unresolved().count(), 0);
    assert!(book.positions.is_empty());
    assert_eq!(
        crate::sources::total_usage(
            &state,
            crate::sources::tests::shipped_limits().buy_attempts_total_max
        )
        .unwrap()
        .used,
        1
    );
    LEASE_BOUND.with(|b| b.set(false));
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-076 re-review M1: a lease holds its own UTC day's slot only: at
/// the next day it is renewed (taking that day's slot) and never bound
/// to a BUY of another day.
#[test]
fn a_lease_holds_only_its_own_utc_day() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    const DAY: u64 = 86_400_000;
    let clock = Arc::new(AtomicU64::new(0));
    let wall = clock.clone();
    let (state, d, mut exec, _requests) =
        armed_clocked("live-lease-day", Vec::new(), 2 * DAY, move |now| {
            wall.store(now, Ordering::SeqCst);
            Box::new(move || wall.load(Ordering::SeqCst))
        });
    let start = clock.load(Ordering::SeqCst);
    let midnight = (start / DAY + 1) * DAY;
    let usage = |at: u64| {
        crate::sources::buy_usage(
            &state,
            at,
            crate::sources::tests::shipped_limits().buy_attempts_total_max,
        )
        .unwrap()
        .used
    };
    clock.store(midnight - 60_000, Ordering::SeqCst);
    lease_c3(&mut exec, 0);
    assert_eq!(usage(midnight - 60_000), 1);
    let lease = exec.end().lease().cloned().unwrap();
    let bind = |exec: &Executor<LiveEnd>, now_ms: u64| {
        LiveEnd::lease_for(
            &mut Some(lease.clone()),
            &exec.end().store,
            exec.end().armed(),
            now_ms,
            lease.nonce,
            lease.origin,
            (lease.input_wei, lease.max_wei, lease.exit_reserve_wei),
            lease.max_block,
        )
    };
    assert!(bind(&exec, midnight - 1).is_ok());
    let why = bind(&exec, midnight + 1_000).unwrap_err();
    assert!(why.contains("another UTC day"), "{why}");
    // After midnight the same lease is renewed: the new day's slot.
    clock.store(midnight + 1_000, Ordering::SeqCst);
    assert_eq!(usage(midnight + 1_000), 0);
    lease_c3(&mut exec, 0);
    assert_eq!(usage(midnight + 1_000), 1);
    assert_eq!(exec.end().lease().unwrap().attempt(), lease.attempt());
    assert!(bind(&exec, midnight + 2_000).is_err());
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-076 review H2: the lease's facts follow the BUY's only once it
/// is written: a BUY whose fact the ledger did not take leaves its lease
/// unresolved (a restart recovers it at the BUY's nonce), never the BUY
/// untracked behind a lease marked withdrawn.
#[test]
fn a_buy_whose_fact_is_not_written_keeps_its_lease_unresolved() {
    let (state, d, mut exec, requests) = armed_both("live-lease-unwritten");
    let wait = std::time::Duration::from_secs(2);
    lease_c3(&mut exec, 0);
    let leased = crate::sources::admitted_intents(&state).unwrap();
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input,
        ingress_ns: 100,
    };
    crate::live_ledger::FAIL_APPENDS.with(|f| f.set(1));
    let _ = exec.on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7));
    crate::live_ledger::FAIL_APPENDS.with(|f| f.set(0));
    // It left; neither its fact nor the lease's was written.
    requests.recv_timeout(wait).unwrap();
    assert!(exec.end().ledger_failure().is_some());
    assert!(Ledger::open(&state).facts().unwrap().is_empty());
    assert_eq!(crate::sources::admitted_intents(&state).unwrap(), leased);
    // A restart recovers the lease, UNKNOWN at the BUY's nonce.
    let recovered = Ledger::open(&state)
        .recover(&crate::sources::admitted_intents(&state).unwrap(), 1)
        .unwrap();
    assert_eq!(recovered.unknown, 1);
    let facts = Ledger::open(&state).facts().unwrap();
    assert_eq!(
        (facts[0].kind, facts[0].tx_hash, facts[0].nonce),
        (FactKind::Unknown, leased[0].tx_hash, 0)
    );
    drop(exec);
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-055: a token is bought once per sender and arm, whichever
/// strategy asks: Strategy C's BUY of the token C3 bought is refused.
#[test]
fn a_token_is_bought_once_by_one_sender() {
    let (state, d, mut exec, requests) = armed_both("live-dedup");
    let wait = std::time::Duration::from_secs(2);
    let silent = std::time::Duration::from_millis(300);
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input,
        ingress_ns: 100,
    };
    lease_c3(&mut exec, 0);
    let buy = exec
        .on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
        .unwrap();
    requests.recv_timeout(wait).unwrap();
    reconnect(&mut exec);
    // Nonce 0 canonical (the C3 BUY reverted, say): the strategy's BUY
    // of the same token is still not a second purchase.
    let again = exec.on_strategy_buy(
        buy.token,
        [0; 20],
        curve_after(1_000_000_000_000_000_000),
        U256::from(10_000_000_000_000_000u64),
        &strategy_c2_next(Some(HEAD), 1),
        &clock(500, 3),
        500,
    );
    assert_eq!(again.err(), Some(Refusal::DuplicateToken));
    assert!(requests.recv_timeout(silent).is_err());
    exec.disarm().unwrap();
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}

/// BSC3-051: each provider answer is read without waiting, bound to its
/// request and recorded apart from the chain: counted, a RESPONDED
/// fact, the transaction still unresolved. After an ACK or a REJECT the
/// connection takes the next request (no reconnect); a REJECT or 429
/// of a BUY closes the end's BUYs; another request's answer is a
/// protocol fault; a connection given up before answering is
/// NO_RESPONSE.
#[test]
fn a_providers_answer_is_read_bound_and_recorded_apart_from_the_chain() {
    use crate::live_ledger::fold;
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let wait = std::time::Duration::from_secs(2);
    for (n, answer, outcome) in [
        (0, Answer::Ack, "ACK"),
        (1, Answer::Reject, "REJECT"),
        (2, Answer::RateLimited, "RATE_LIMITED"),
        (3, Answer::WrongId, "PROTOCOL"),
        (4, Answer::Silent, "NO_RESPONSE"),
    ] {
        let (state, d, mut exec, requests) =
            armed_answering(&format!("live-answer-{n}"), vec![answer, Answer::Ack]);
        let (hash, raw, input) = launch(0, 7);
        let pending = PendingLaunch {
            hash,
            raw: &raw,
            from: [0; 20],
            to: Some(portal),
            value: &[],
            input: &input,
            ingress_ns: 100,
        };
        lease_c3(&mut exec, 0);
        let buy = exec
            .on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
            .unwrap();
        requests.recv_timeout(wait).unwrap();
        let read = |exec: &mut Executor<LiveEnd>| {
            let deadline = std::time::Instant::now() + wait;
            while std::time::Instant::now() < deadline {
                exec.poll_responses(&clock(200, 1));
                if exec.end().responses() != Responses::default() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        read(&mut exec);
        if let Answer::Silent = answer {
            assert_eq!(exec.end().responses(), Responses::default());
            // The maintenance gives the connection up: nothing answered.
            assert!(exec.end_mut().maintain(wait).is_empty());
            exec.poll_responses(&clock(300, 1));
        }
        let r = exec.end().responses();
        let counted = r.named().into_iter().find(|(_, c)| *c > 0);
        assert_eq!(counted, Some((outcome, 1)), "{answer:?}: {r:?}");
        // A poll only reads: the fact is written outside any hand-over.
        let facts = || Ledger::open(&state).facts().unwrap();
        assert!(facts().iter().all(|f| f.kind != FactKind::Responded));
        exec.end_mut().record_responses();
        // REV.1.0 R2: the answer with its adapter and time, once, as the
        // attempt's `a` record reads it back.
        let timed = exec.end_mut().take_answers();
        assert_eq!(timed.len(), 1, "{answer:?}");
        assert_eq!(
            (timed[0].tx_hash, timed[0].outcome),
            (buy.handoff.tx_hash, outcome)
        );
        assert_eq!(
            timed[0].at_ns.is_some(),
            outcome != "NO_RESPONSE",
            "{answer:?}"
        );
        let (tx, adapter, code, at) = crate::runtime_journal::live_answer_of(
            &crate::runtime_journal::live_answer_payload(&timed[0]),
        )
        .unwrap();
        assert_eq!(
            (tx, usize::from(adapter), at),
            (
                buy.handoff.tx_hash,
                timed[0].adapter.index(),
                timed[0].at_ns
            )
        );
        assert_eq!(crate::runtime_journal::ANSWERS[usize::from(code)], outcome);
        assert!(exec.end_mut().take_answers().is_empty());
        let book = fold(&facts());
        let tx = book.tx(&buy.handoff.tx_hash).unwrap();
        assert_eq!(tx.state, FactKind::Released, "{answer:?}");
        assert_eq!(tx.responses, vec![format!("CLUB48_BUILDER_V1:{outcome}")]);
        let refused = matches!(answer, Answer::Reject | Answer::RateLimited);
        assert_eq!(exec.end().buy_refused().is_some(), refused, "{answer:?}");
        if matches!(answer, Answer::Ack | Answer::Reject) {
            // The connection is ready again: the next request goes on it.
            let next = exec.on_strategy_buy(
                [0x5A; 20],
                [0; 20],
                curve_after(1_000_000_000_000_000_000),
                U256::from(10_000_000_000_000_000u64),
                &strategy_c2_next(Some(HEAD), 1),
                &clock(500, 3),
                500,
            );
            match answer {
                Answer::Ack => {
                    assert!(next.is_ok(), "{next:?}");
                    requests.recv_timeout(wait).unwrap();
                }
                _ => {
                    assert_eq!(next.err(), Some(Refusal::Dispatch));
                    assert!(
                        exec.end()
                            .last_refusal()
                            .unwrap()
                            .starts_with("PROVIDER_REFUSED")
                    );
                }
            }
        }
        drop(exec);
        fs::remove_dir_all(d).unwrap();
        fs::remove_dir_all(state).unwrap();
    }
}

/// BSC3-045: the ledger did not take a released transaction's fact.
/// The end hands nothing over after it: the next intent is refused
/// before the owner admits it (no attempt, no spend), nothing leaves.
#[test]
fn an_end_whose_fact_was_not_written_hands_nothing_more_over() {
    use std::os::unix::fs::PermissionsExt;
    let (state, d, mut exec, requests) = armed_both("live-unwritten");
    let wait = std::time::Duration::from_secs(2);
    let silent = std::time::Duration::from_millis(300);
    let ledger = state.join("live-ledger");
    fs::create_dir_all(&ledger).unwrap();
    fs::set_permissions(&ledger, fs::Permissions::from_mode(0o500)).unwrap();
    let portal = bsc_mainnet_v5_27_2().portal.into_array();
    let (hash, raw, input) = launch(0, 7);
    let pending = PendingLaunch {
        hash,
        raw: &raw,
        from: [0; 20],
        to: Some(portal),
        value: &[],
        input: &input,
        ingress_ns: 100,
    };
    lease_c3(&mut exec, 0);
    exec.on_launch(pending, &c2_next(Some(HEAD), 0), &clock(100, 7))
        .unwrap();
    requests.recv_timeout(wait).unwrap();
    assert!(exec.end().ledger_failure().is_some());
    reconnect(&mut exec);
    let next = exec.on_strategy_buy(
        [0x5A; 20],
        [0; 20],
        curve_after(1_000_000_000_000_000_000),
        U256::from(10_000_000_000_000_000u64),
        &strategy_c2_next(Some(HEAD), 1),
        &clock(500, 3),
        500,
    );
    assert_eq!(next.err(), Some(Refusal::Owner));
    assert!(
        exec.end()
            .last_refusal()
            .unwrap()
            .starts_with(crate::sources::LIVE_LEDGER_UNREADABLE)
    );
    assert!(requests.recv_timeout(silent).is_err());
    assert_eq!(crate::sources::admitted_intents(&state).unwrap().len(), 1);
    fs::set_permissions(&ledger, fs::Permissions::from_mode(0o700)).unwrap();
    drop(exec);
    fs::remove_dir_all(d).unwrap();
    fs::remove_dir_all(state).unwrap();
}
