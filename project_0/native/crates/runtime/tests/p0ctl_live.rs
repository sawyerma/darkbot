//! P8-CTRL-02: `p0ctl live` over the real binary. A scope is previewed natively against the
//! registry's ceilings (an operator text, never JSON, never through a symlink) and stored under its
//! digest; a grant names exactly a previewed digest with its version, epoch and request id and
//! reaches the running owner's queue (this test process stands in for the runtime: owner lock,
//! epoch, control thread); `live status` names the grant state. Offline only.
use darkbot_common::channel::bounded;
use darkbot_common::lifecycle::Shutdown;
use darkbot_runner::orchestration::control;
use darkbot_runner::orchestration::{capability_control, source_control};
use solana_sdk::pubkey::Pubkey;
use source_control::ControlMessage;
use std::process::Command;
use std::time::{Duration, Instant};

/// The registry ceilings of this test's operator (the defaults grant nothing).
const CEILINGS: [(&str, &str); 5] = [
    ("P0_MAX_TOTAL_CAPITAL", "500000000"),
    ("P0_MAX_EXPOSURE", "100000000"),
    ("P0_MAX_DAILY_LOSS", "50000000"),
    ("P0_MAX_TIP", "1000000"),
    ("P0_MAX_SLIPPAGE_BPS", "100"),
];

fn p0ctl(project: &std::path::Path, env: &[(&str, &str)], args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .envs(env.iter().copied())
        .arg("--project")
        .arg(project)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn scope(signer: &Pubkey) -> String {
    let key = "11111111111111111111111111111112";
    [
        "schema = Project0/FE06Scope/1".to_string(),
        "scope_id = pilot-1".into(),
        "mode = ALL".into(),
        "cluster = mainnet-beta".into(),
        format!(
            "genesis_hash = {}",
            darkbot_project0::protocol::GENESIS_HASH_MAINNET
        ),
        format!(
            "programs = {}",
            darkbot_project0::protocol::PROGRAM_ID_MAINNET
        ),
        format!(
            "program_data_hash = {}",
            darkbot_project0::protocol::PROGRAM_DATA_REFERENCE_HASH
        ),
        "code_sha = c0de".into(),
        format!("binary_sha256 = {}", "b".repeat(64)),
        format!("runtime_config_sha256 = {}", "c".repeat(64)),
        format!("signers = {signer}:LIQUIDATOR+FEE_PAYER"),
        format!("fee_payer = {signer}"),
        "execution_classes = RECEIVERSHIP".into(),
        "funding_classes = ZERO_PREFUND_SAME_TX".into(),
        format!("banks = {key}"),
        format!("mints = {key}"),
        "exit_positions = NONE".into(),
        "submission_adapters = rpc".into(),
        "data_sources = wss".into(),
        "evidence = provider:0123456789abcdef,qualification:run-1,fe01:frozen-1".into(),
        "max_input_per_attempt_usd_micro = 10000000".into(),
        "max_exposure_usd_micro = 10000000".into(),
        "max_loss_usd_micro = 1000000".into(),
        "max_spend_lamports = 50000000".into(),
        "max_fee_lamports = 100000".into(),
        "max_priority_fee_lamports = 50000".into(),
        "max_tip_lamports = 10000".into(),
        "max_rent_lamports = 3000000".into(),
        "max_protocol_fee_lamports = 10000".into(),
        "max_exit_cost_lamports = 100000".into(),
        "reserve_lamports = 10000000".into(),
        "max_slippage_bps = 50".into(),
        "min_out_bps = 9950".into(),
        "route = FREE_PUBLIC".into(),
        "campaign_id = first-profit".into(),
        "max_attempts_per_utc_day = 1".into(),
        "max_attempts_campaign = 1".into(),
        "max_attempts_grant = 1".into(),
        "max_exits_grant = 1".into(),
        "not_before_unix_s = 1000".into(),
        "expires_unix_s = 4600".into(),
        "owner_epoch = 1".into(),
    ]
    .join("\n")
}

#[test]
fn p8ctrl02_p0ctl_live_previews_a_scope_and_sends_its_grant_to_the_owner() {
    let d = std::env::temp_dir().join(format!("p0ctl_live_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    let file = d.join("scope.conf");
    std::fs::write(&file, scope(&Pubkey::new_unique())).unwrap();
    let file = file.to_str().unwrap();
    // The registry's defaults grant nothing: the scope is above them.
    let (code, _, err) = p0ctl(&d, &[], &["live", "scope", "--file", file]);
    assert_eq!(code, 2);
    assert!(err.contains("FE06_SCOPE_ABOVE_CEILING"), "{err}");
    // Within the operator's ceilings: previewed in full under its digest, the same twice.
    let (code, out, err) = p0ctl(&d, &CEILINGS, &["live", "scope", "--file", file]);
    assert_eq!(code, 0, "{out}{err}");
    let digest = out
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("PROJECT 0 FE-06 SCOPE digest="))
        .unwrap()
        .to_string();
    assert_eq!(digest.len(), 64);
    assert!(
        out.contains("attempts ≤ 1/UTC day") && out.contains("never a command"),
        "{out}"
    );
    let (_, again, _) = p0ctl(&d, &CEILINGS, &["live", "scope", "--file", file]);
    assert!(again.starts_with(&format!("PROJECT 0 FE-06 SCOPE digest={digest}")));
    // JSON and a symlinked file are refused.
    let json = d.join("scope.json");
    std::fs::write(&json, b"{\"mode\":\"ALL\"}").unwrap();
    let (code, _, err) = p0ctl(
        &d,
        &CEILINGS,
        &["live", "scope", "--file", json.to_str().unwrap()],
    );
    assert_eq!(code, 2);
    assert!(err.contains("FE06_SCOPE_JSON_REFUSED"), "{err}");
    let link = d.join("scope.link");
    std::os::unix::fs::symlink(file, &link).unwrap();
    let (code, _, err) = p0ctl(
        &d,
        &CEILINGS,
        &["live", "scope", "--file", link.to_str().unwrap()],
    );
    assert_eq!(code, 2);
    assert!(err.contains("FE06_SCOPE_UNREADABLE"), "{err}");
    // A grant names a previewed digest and states its binding.
    let bound = [
        "--expect-version",
        "0",
        "--expect-epoch",
        "1",
        "--request-id",
        "g-1",
    ];
    let grant = |digest: &str, flags: &[&str]| {
        let mut args = vec!["live", "grant", "--digest", digest];
        args.extend_from_slice(flags);
        p0ctl(&d, &[], &args)
    };
    let (code, _, err) = grant(&"0".repeat(64), &bound);
    assert_eq!(code, 2);
    assert!(err.contains("FE06_SCOPE_UNKNOWN"), "{err}");
    let (code, _, err) = grant(&digest, &[]);
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_BINDING_REQUIRED"), "{err}");
    let (code, _, err) = grant(&digest, &bound);
    assert_eq!(code, 2);
    assert!(err.contains("CAPABILITY_NO_RUNTIME"), "{err}");

    // The stand-in runtime: lock, epoch, doorbell, control thread with both logs.
    let dir = capability_control::request_dir(&d);
    let _lock =
        control::acquire_owner(&control::default_lock_path(&d), "live_test", "shadow").unwrap();
    assert_eq!(
        capability_control::open_epoch(&dir, std::process::id(), None).unwrap(),
        1
    );
    let (tx, rx) = bounded::<ControlMessage>("source-control", 8);
    let shutdown = Shutdown::new();
    let sources = source_control::request_dir(&d);
    let thread = source_control::spawn(
        sources.clone(),
        tx,
        shutdown.clone(),
        source_control::head(&sources).unwrap(),
        Some((
            dir.clone(),
            capability_control::head(&dir).unwrap(),
            Default::default(),
        )),
    )
    .unwrap();
    let (code, out, err) = grant(&digest, &bound);
    assert_eq!(code, 0, "{out}{err}");
    let t = Instant::now();
    let message = loop {
        if let Some(m) = rx.try_pop() {
            break m;
        }
        assert!(t.elapsed() < Duration::from_secs(5), "no control message");
        std::thread::sleep(Duration::from_millis(5));
    };
    match message {
        ControlMessage::Capability {
            seq: 1, request, ..
        } => {
            let r = request.unwrap();
            assert_eq!(
                (
                    r.capability.as_str(),
                    r.requested.as_str(),
                    r.request_id.as_str(),
                    r.owner_epoch
                ),
                (capability_control::FE06_GRANT, digest.as_str(), "g-1", 1)
            );
        }
        other => panic!("{other:?}"),
    }
    let (code, out, _) = p0ctl(&d, &[], &["live", "status"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("owner epoch: 1")
            && out.contains("grant in force: none")
            && out.contains("FE06_OPERATOR_AUTHORIZATION_UNDECIDED"),
        "{out}"
    );
    // `live attempts`: a wallet's bookings against the previewed scope's quotas (none booked).
    let journals = d.join("fe04");
    let wallet = Pubkey::new_unique().to_string();
    let attempts = |dir: &std::path::Path, extra: &[&str]| {
        let mut args = vec![
            "live",
            "attempts",
            "--journal-dir",
            dir.to_str().unwrap(),
            "--wallet",
            &wallet,
        ];
        args.extend_from_slice(extra);
        p0ctl(&d, &[], &args)
    };
    let (code, _, err) = attempts(&journals, &[]);
    assert_eq!(code, 2);
    assert!(err.contains("FE04_JOURNAL_DIR_MISSING"), "{err}");
    std::fs::create_dir_all(&journals).unwrap();
    let (code, out, err) = attempts(&journals, &["--digest", &digest]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("0 booked, 0 without a grant")
            && out.contains("UTC_DAY day ")
            && out.contains(&format!("GRANT {digest}: 0 of 1")),
        "{out}"
    );
    let (code, _, err) = attempts(&journals, &["--digest", &"0".repeat(64)]);
    assert_eq!(code, 2);
    assert!(err.contains("FE06_SCOPE_UNKNOWN"), "{err}");
    // Strictly typed (review L2): a flag twice, an unknown flag, a wallet that is no key.
    for extra in [
        &["--wallet", wallet.as_str()][..],
        &["--from", "x"][..],
        &["--digest"][..],
    ] {
        let (code, _, err) = attempts(&journals, extra);
        assert_eq!(code, 2, "{extra:?}");
        assert!(err.contains("usage: p0ctl live"), "{extra:?}: {err}");
    }
    let (code, _, err) = p0ctl(
        &d,
        &[],
        &[
            "live",
            "attempts",
            "--journal-dir",
            journals.to_str().unwrap(),
            "--wallet",
            "not-a-key",
        ],
    );
    assert_eq!(code, 2);
    assert!(err.contains("FE06_WALLET_INVALID"), "{err}");
    shutdown.request();
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(&d);
}
