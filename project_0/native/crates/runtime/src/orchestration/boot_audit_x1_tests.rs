//! X1: binding of a launched runtime to its approved network audit, offline with fixtures only
//! (valid record, drift per field class, missing record, expiry, tampering). The real Mainnet
//! case stays NOT_RUN.
use super::*;
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

fn fixture_identity() -> AuditedIdentity {
    network_record_fixture(vec![]).audit.identity
}

fn admit(n: &NetworkAudit, approved_at: u64, current: &AuditedIdentity) -> Result<(), String> {
    let token = n.audit.readiness_token[..16].to_string();
    n.validate_admission(approved_at, &token, current, "binary", "logs")
}

/// Record, token, approval time and every identity field class: only the exact approved record,
/// approved inside its TTL, by the same code, config, flags, schema, executables, capture source
/// and pinned genesis admits; each refusal carries its fixed code.
#[test]
fn x1_admission_refuses_expiry_tampering_and_every_identity_drift() {
    let n = network_record_fixture(vec![]);
    let id = fixture_identity();
    assert_eq!(admit(&n, 1000, &id), Ok(()));
    assert_eq!(admit(&n, 1059, &id), Ok(()));
    // Approval outside the TTL (the runtime itself never re-checks the wall clock).
    for at in [999, 1060, 5000] {
        assert_eq!(admit(&n, at, &id).unwrap_err(), "BOOT_NETWORK_EXPIRED");
    }
    let mut long = n.clone();
    long.expires_at = 1061;
    long.reseal();
    assert_eq!(admit(&long, 1001, &id).unwrap_err(), "BOOT_NETWORK_EXPIRED");
    // Another record's token, a short token, a tampered record.
    let other = "0".repeat(16);
    assert_eq!(
        n.validate_admission(1001, &other, &id, "binary", "logs")
            .unwrap_err(),
        "BOOT_NETWORK_TOKEN"
    );
    let short = n.audit.readiness_token[..15].to_string();
    assert_eq!(
        n.validate_admission(1001, &short, &id, "binary", "logs")
            .unwrap_err(),
        "BOOT_NETWORK_TOKEN"
    );
    let mut tampered = n.clone();
    tampered.slot += 1;
    assert_eq!(
        admit(&tampered, 1001, &id).unwrap_err(),
        "BOOT_NETWORK_TOKEN"
    );
    // A consistent (re-sealed) record that is not READY or lacks a network check.
    let mut blocked = n.clone();
    blocked.audit.verdict = AuditVerdict::Blocked;
    blocked.reseal();
    assert_eq!(
        admit(&blocked, 1001, &id).unwrap_err(),
        "BOOT_NETWORK_NOT_READY"
    );
    let mut missing = n.clone();
    missing.audit.checks.retain(|c| c.id != "BOOT-05");
    missing.reseal();
    assert_eq!(
        admit(&missing, 1001, &id).unwrap_err(),
        "BOOT_NETWORK_CHECK_MISSING"
    );

    // Identity drift, one field at a time.
    type Edit = fn(&mut AuditedIdentity);
    let drifted: [(&str, Edit); 7] = [
        ("source", |i| i.source_head = Some("other".into())),
        ("source", |i| i.source_dirty_sha256 = None),
        ("runtime_config", |i| {
            i.runtime_config_sha256 = "other".into()
        }),
        ("safety_flags", |i| i.live_enabled = true),
        ("safety_flags", |i| i.submission_enabled = true),
        ("schema", |i| i.journal_version += 1),
        ("schema", |i| i.discriminator_table_sha256 = "other".into()),
    ];
    for (field, edit) in drifted {
        let mut current = id.clone();
        edit(&mut current);
        let e = admit(&n, 1001, &current).unwrap_err();
        assert_eq!(e, format!("BOOT_IDENTITY_DRIFT: {field}"));
        assert_eq!(admission_code(&e), "BOOT_IDENTITY_DRIFT");
    }
    let token = &n.audit.readiness_token[..16];
    assert_eq!(
        n.validate_admission(1001, token, &id, "rebuilt", "logs")
            .unwrap_err(),
        "BOOT_IDENTITY_DRIFT: binary"
    );
    assert_eq!(
        n.validate_admission(1001, token, &id, "binary", "transactions")
            .unwrap_err(),
        "BOOT_IDENTITY_DRIFT: capture_source"
    );
    let mut genesis = n.clone();
    genesis.genesis = solana_sdk::hash::Hash::new_unique().to_string();
    genesis.reseal();
    assert_eq!(
        admit(&genesis, 1001, &id).unwrap_err(),
        "BOOT_IDENTITY_DRIFT: genesis"
    );
    let mut current = id.clone();
    current.runtime_config_sha256 = "other".into();
    assert_eq!(
        n.validate_admission(1001, token, &current, "rebuilt", "transactions")
            .unwrap_err(),
        "BOOT_IDENTITY_DRIFT: runtime_config,binary,capture_source"
    );
    for (e, code) in [
        ("BOOT_NETWORK_EXPIRED", "BOOT_NETWORK_EXPIRED"),
        ("BOOT_NETWORK_TOKEN", "BOOT_NETWORK_TOKEN"),
        ("BOOT_NETWORK_NOT_READY", "BOOT_NETWORK_NOT_READY"),
        ("BOOT_NETWORK_CHECK_MISSING", "BOOT_NETWORK_CHECK_MISSING"),
        (
            "BOOT_NETWORK_AUDIT_MISSING: x",
            "BOOT_NETWORK_AUDIT_MISSING",
        ),
        (
            "BOOT_NETWORK_EVIDENCE_INVALID",
            "BOOT_NETWORK_EVIDENCE_INVALID",
        ),
        ("RUNTIME_CONFIG_DIGEST: x", "BOOT_ADMISSION_REFUSED"),
    ] {
        assert_eq!(admission_code(e), code);
    }
}

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("p0_x1_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The launcher environment: a start by p0ctl needs record, token and approval time; anything
/// missing or unreadable refuses before any network read; without both launch marker and record
/// the legacy local path is unchanged.
#[test]
fn x1_admission_record_from_the_launcher_environment() {
    let d = temp("record");
    let n = network_record_fixture(vec![]);
    let path = d.join("boot_network.p0j");
    let mut w = journal::Writer::create(&path).unwrap();
    w.append_typed(journal::Provenance::default(), &n).unwrap();
    w.seal(true).unwrap();
    let garbage = d.join("garbage.p0j");
    std::fs::write(&garbage, b"not a journal").unwrap();
    let token = n.audit.readiness_token[..16].to_string();
    let full: HashMap<&str, String> = [
        (ENV_NETWORK_AUDIT, path.display().to_string()),
        (ENV_NETWORK_AUDIT_TOKEN, token.clone()),
        (ENV_NETWORK_AUDIT_APPROVED_AT, "1001".into()),
        ("P0_MARKET_SOURCE", "logs".into()),
    ]
    .into();
    let current = || Ok(fixture_identity());
    let binary = || Ok("binary".to_string());
    let run =
        |env: &HashMap<&str, String>| admission_record(&|k| env.get(k).cloned(), &current, &binary);
    assert_eq!(run(&full), Ok(n.clone()));
    // DEC-07: no start without its record — a start without the launch marker included.
    assert_eq!(
        run(&HashMap::new()).unwrap_err(),
        "BOOT_NETWORK_AUDIT_MISSING"
    );
    for drop in [ENV_NETWORK_AUDIT_TOKEN, ENV_NETWORK_AUDIT_APPROVED_AT] {
        let mut env = full.clone();
        env.remove(drop);
        assert_eq!(run(&env).unwrap_err(), "BOOT_NETWORK_AUDIT_MISSING");
    }
    let mut env = full.clone();
    env.insert(ENV_NETWORK_AUDIT_APPROVED_AT, "soon".into());
    assert_eq!(run(&env).unwrap_err(), "BOOT_NETWORK_AUDIT_MISSING");
    for bad in [d.join("absent.p0j"), garbage] {
        let mut env = full.clone();
        env.insert(ENV_NETWORK_AUDIT, bad.display().to_string());
        assert_eq!(run(&env).unwrap_err(), "BOOT_NETWORK_EVIDENCE_INVALID");
    }
    let mut env = full.clone();
    env.insert(ENV_NETWORK_AUDIT_APPROVED_AT, "1060".into());
    assert_eq!(run(&env).unwrap_err(), "BOOT_NETWORK_EXPIRED");
    let mut env = full.clone();
    env.insert("P0_MARKET_SOURCE", "transactions".into());
    assert_eq!(
        run(&env).unwrap_err(),
        "BOOT_IDENTITY_DRIFT: capture_source"
    );
    // Unset, the capture resolves transactions (its own default): a logs record is drift.
    let mut env = full.clone();
    env.remove("P0_MARKET_SOURCE");
    assert_eq!(
        run(&env).unwrap_err(),
        "BOOT_IDENTITY_DRIFT: capture_source"
    );
    // A Shadow run's source is fixed (its launcher exports none): the logs record admits it,
    // whatever P0_MARKET_SOURCE says.
    for source in [None, Some("transactions")] {
        let mut env = full.clone();
        env.insert("P0_MODE", "shadow".into());
        env.remove("P0_MARKET_SOURCE");
        if let Some(s) = source {
            env.insert("P0_MARKET_SOURCE", s.into());
        }
        assert_eq!(run(&env), Ok(n.clone()), "{source:?}");
    }
    let failing = || Err::<AuditedIdentity, _>("BOOT_NETWORK_AUDIT_MISSING: P0_PROJECT_DIR".into());
    assert_eq!(
        admission_record(&|k| full.get(k).cloned(), &failing, &binary).unwrap_err(),
        "BOOT_NETWORK_AUDIT_MISSING: P0_PROJECT_DIR"
    );
    std::fs::remove_dir_all(d).unwrap();
}

struct MapSource(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
impl darkbot_project0::health::AccountSource for MapSource {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
        self.0.get(key).map(|(o, d)| (*o, 1, d.as_slice()))
    }
}

struct Scope {
    program: Pubkey,
    program_data: Pubkey,
    group: Pubkey,
    oracle: Pubkey,
    mint: Pubkey,
    bank: Pubkey,
    vault: Pubkey,
    lending_target: Pubkey,
    liability_target: Pubkey,
    record: NetworkAudit,
    state: MapSource,
    required: HashSet<Pubkey>,
}

/// Audited accounts of each class and the state the runtime read itself.
fn scope() -> Scope {
    use darkbot_project0::protocol;
    let program = Pubkey::from_str(protocol::PROGRAM_ID_MAINNET).unwrap();
    let loader = Pubkey::from_str(protocol::BPF_LOADER_UPGRADEABLE_ID).unwrap();
    let token = Pubkey::from_str(protocol::TOKEN_PROGRAM_ID).unwrap();
    let pyth = Pubkey::from_str(protocol::PYTH_PUSH_ORACLE_ID).unwrap();
    let k = Pubkey::new_unique;
    let (program_data, group, oracle, mint, bank, vault, lending_target, liability_target) =
        (k(), k(), k(), k(), k(), k(), k(), k());
    let bank_len = 8 + protocol::BANK_PAYLOAD_LEN_PINNED;
    // (key, owner, data, executable, present in the runtime state)
    let accounts: Vec<(Pubkey, Pubkey, Vec<u8>, bool, bool)> = vec![
        (program, loader, vec![2; 36], true, false),
        (program_data, loader, vec![7; 64], false, true),
        (group, program, vec![1; 1056], false, true),
        (oracle, pyth, vec![3; 134], false, true),
        (mint, token, vec![4; 82], false, true),
        (bank, program, vec![5; bank_len], false, true),
        (vault, token, vec![6; 165], false, false),
        (lending_target, program, vec![9; 2312], false, false),
        (liability_target, program, vec![10; 2312], false, true),
    ];
    let proofs = accounts
        .iter()
        .map(|(key, owner, data, executable, _)| proof_of(key, owner, data, *executable))
        .collect();
    let state = MapSource(
        accounts
            .iter()
            .filter(|a| a.4)
            // Data other than ProgramData legitimately changes within the TTL.
            .map(|(key, owner, data, _, _)| {
                let mut data = data.clone();
                if *key != program_data {
                    data[0] ^= 0xFF;
                }
                (*key, (*owner, data))
            })
            .collect(),
    );
    Scope {
        program,
        program_data,
        group,
        oracle,
        mint,
        bank,
        vault,
        lending_target,
        liability_target,
        record: network_record_fixture(proofs),
        state,
        required: [group, oracle, mint].into(),
    }
}

impl Scope {
    fn bind(&self, program_data_executable: bool) -> Result<IdentityBinding, String> {
        self.record.bind_state(
            &self.program,
            &self.program_data,
            program_data_executable,
            &self.required,
            &self.state,
        )
    }
}

/// Bound: ProgramData (owner, length, hash, executable), the runtime's required dependencies
/// (group, oracle, mint), audited banks and present targets (owner and length; their data may
/// change within the TTL). Outside the runtime scope (reported, never bound): the program
/// account, the liquidity vault and the target without liabilities. Every deviation or missing
/// in-scope account is BOOT_IDENTITY_DRIFT naming the account and field class.
#[test]
fn x1_bind_state_binds_runtime_scope_and_reports_drift_per_field_class() {
    let s = scope();
    let b = s.bind(false).unwrap();
    assert_eq!(
        b.bound, 6,
        "program_data, group, oracle, mint, bank, liability target"
    );
    let mut outside = b.outside_scope.clone();
    outside.sort();
    let mut expected = vec![
        s.program.to_string(),
        s.vault.to_string(),
        s.lending_target.to_string(),
    ];
    expected.sort();
    assert_eq!(outside, expected);
    assert!(!outside.contains(&s.liability_target.to_string()));

    let drift = |edit: &dyn Fn(&mut Scope), executable: bool| {
        let mut s = scope();
        edit(&mut s);
        let e = s.bind(executable).unwrap_err();
        assert_eq!(admission_code(&e), "BOOT_IDENTITY_DRIFT");
        (s, e)
    };
    // Owner of a bank.
    let (s, e) = drift(
        &|s| s.state.0.get_mut(&s.bank).unwrap().0 = Pubkey::new_unique(),
        false,
    );
    assert_eq!(e, format!("BOOT_IDENTITY_DRIFT: {}:owner", s.bank));
    // Length of an oracle.
    let (s, e) = drift(&|s| s.state.0.get_mut(&s.oracle).unwrap().1.push(0), false);
    assert_eq!(e, format!("BOOT_IDENTITY_DRIFT: {}:length", s.oracle));
    // ProgramData hash and executable flag.
    let (s, e) = drift(
        &|s| s.state.0.get_mut(&s.program_data).unwrap().1[0] = 0,
        false,
    );
    assert_eq!(
        e,
        format!("BOOT_IDENTITY_DRIFT: {}:program_data", s.program_data)
    );
    let (s, e) = drift(&|_| {}, true);
    assert_eq!(
        e,
        format!("BOOT_IDENTITY_DRIFT: {}:program_data", s.program_data)
    );
    // A data account audited as executable.
    let (s, e) = drift(
        &|s| {
            let mint = s.mint.to_string();
            s.record
                .accounts
                .iter_mut()
                .find(|p| p.address == mint)
                .unwrap()
                .executable = true;
        },
        false,
    );
    assert_eq!(e, format!("BOOT_IDENTITY_DRIFT: {}:executable", s.mint));
    // Missing required dependency, missing bank, missing ProgramData: no fallback, no re-pin.
    for which in 0..3 {
        let (s, e) = drift(
            &|s| {
                let key = [s.group, s.bank, s.program_data][which];
                s.state.0.remove(&key);
            },
            false,
        );
        let key = [s.group, s.bank, s.program_data][which];
        assert_eq!(e, format!("BOOT_IDENTITY_DRIFT: {key}:missing"));
    }
    // An outside-scope account the runtime does hold is bound like any other.
    let (s, e) = drift(
        &|s| {
            s.state
                .0
                .insert(s.vault, (Pubkey::new_unique(), vec![6; 165]));
        },
        false,
    );
    assert_eq!(e, format!("BOOT_IDENTITY_DRIFT: {}:owner", s.vault));
    // Several deviations are all named, sorted.
    let (s, e) = drift(
        &|s| {
            s.state.0.remove(&s.group);
            s.state.0.get_mut(&s.oracle).unwrap().1.push(0);
        },
        false,
    );
    let mut names = [
        format!("{}:missing", s.group),
        format!("{}:length", s.oracle),
    ];
    names.sort();
    assert_eq!(e, format!("BOOT_IDENTITY_DRIFT: {}", names.join(",")));
    // An invalid address in the record is never skipped.
    let mut s = scope();
    s.record.accounts[2].address = "not-a-key".into();
    assert_eq!(s.bind(false).unwrap_err(), "BOOT_IDENTITY_RECORD_INVALID");
}

/// The config p0ctl audits (operational args + `--duration D`, operator environment, the mode's
/// profile) and the config the launched runtime re-resolves (same args, launcher environment with
/// `P0_MEASUREMENT_DURATION=D`, `P0_MODE` and its fixed exports, also after the runtime's own
/// activation exported the config back into its environment) have the same digest; a launcher
/// environment that changes an operational value does not. DEC-05: econ4h resolves `performance`
/// (one worker per CPU), shadow `standard`.
#[test]
fn x1_start_config_is_identical_in_p0ctl_and_the_launched_runtime() {
    let d = temp("config");
    let flags = SafetyFlags {
        live_enabled: false,
        submission_enabled: false,
    };
    let operator: HashMap<String, String> = [
        ("P0_RPC_URL", "http://127.0.0.1:8899"),
        ("P0_WS_URL", "ws://127.0.0.1:8900"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let launcher_exports = |mode: &str, duration: &str, secs: u64| -> HashMap<String, String> {
        let mut env = operator.clone();
        for (k, v) in [
            ("P0_MEASUREMENT_DURATION", duration.to_string()),
            ("P0_ECON_DURATION_SECS", secs.to_string()),
            ("P0_MODE", mode.into()),
            ("P0_LIVE_ENABLED", "false".into()),
            ("P0_SUBMISSION_ENABLED", "false".into()),
            ("P0_MARKET_SOURCE", "logs".into()),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            ),
            ("P0_CTL_LAUNCH_RUN_ID", "econ4h_x1".into()),
        ] {
            env.insert(k.into(), v);
        }
        env
    };
    let ctl_config = |mode: &str, op: &[String], duration: &str| {
        let get = |k: &str| operator.get(k).cloned();
        start_config(mode, op, duration, &get).unwrap().config
    };
    let ctl_identity = |mode: &str, op: &[String], duration: &str| {
        identity_with(&d, flags, &ctl_config(mode, op, duration)).unwrap()
    };
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for mode in ["econ4h", "shadow"] {
        for op in [
            args(&[]),
            args(&["--profile", "standard"]),
            args(&["--profile", "smoke"]),
            args(&["--profile", "performance"]),
            args(&["--rpc-rps", "3", "--fallback", "off"]),
            args(&["--max-slippage-bps", "50", "--threshold-trigger", "off"]),
        ] {
            for (duration, secs) in [("10m", 600), ("1h", 3600)] {
                let audited = ctl_identity(mode, &op, duration);
                let env = launcher_exports(mode, duration, secs);
                let get = |k: &str| env.get(k).cloned();
                assert_eq!(
                    launched_identity(&d, flags, &op, &get).unwrap(),
                    audited,
                    "{mode} {op:?} {duration}"
                );
                // The runtime's own activation (`startup::operational_config` of an operating
                // mode) exports its config into the environment first.
                let mut activated = env.clone();
                let own =
                    darkbot_solana::runtime_config::Resolved::for_mode(mode, &op, &get).unwrap();
                activated.extend(own.config.environment());
                let get = |k: &str| activated.get(k).cloned();
                assert_eq!(
                    launched_identity(&d, flags, &op, &get).unwrap(),
                    audited,
                    "{mode} {op:?} {duration} after activation"
                );
            }
        }
    }
    // DEC-05: each mode audits its own profile; a runtime of the other mode is drift wherever the
    // two profiles differ (the worker count on a machine with more than one CPU).
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get() as u64);
    assert_eq!(ctl_config("econ4h", &[], "10m").limits.workers, cpus);
    assert_eq!(ctl_config("shadow", &[], "10m").limits.workers, 1);
    let shadow_env = launcher_exports("shadow", "10m", 600);
    let get = |k: &str| shadow_env.get(k).cloned();
    assert_eq!(
        launched_identity(&d, flags, &[], &get).unwrap() == ctl_identity("econ4h", &[], "10m"),
        cpus == 1
    );
    // Operational values the launcher environment changes are drift, never admitted.
    let audited = ctl_identity("econ4h", &[], "10m");
    for (k, v) in [
        ("P0_RPC_URL", "http://127.0.0.2:8899"),
        ("P0_RPC_RPS", "9"),
        ("P0_PROFILE", "smoke"),
        ("P0_MAX_SLIPPAGE_BPS", "50"),
        ("P0_USE_THRESHOLD_TRIGGER", "true"),
    ] {
        let mut env = launcher_exports("econ4h", "10m", 600);
        env.insert(k.into(), v.into());
        let get = |k: &str| env.get(k).cloned();
        let current = launched_identity(&d, flags, &[], &get).unwrap();
        assert_ne!(
            current.runtime_config_sha256, audited.runtime_config_sha256,
            "{k}"
        );
    }
    let mut env = launcher_exports("econ4h", "10m", 600);
    env.remove("P0_MEASUREMENT_DURATION");
    let get = |k: &str| env.get(k).cloned();
    assert_eq!(
        launched_identity(&d, flags, &[], &get).unwrap_err(),
        "BOOT_NETWORK_AUDIT_MISSING: P0_MEASUREMENT_DURATION"
    );
    let mut env = launcher_exports("econ4h", "10m", 600);
    env.remove("P0_MODE");
    let get = |k: &str| env.get(k).cloned();
    assert_eq!(
        launched_identity(&d, flags, &[], &get).unwrap_err(),
        "BOOT_NETWORK_AUDIT_MISSING: P0_MODE"
    );
    std::fs::remove_dir_all(d).unwrap();
}

/// DEC-07: a record admits a start only within the TTL it was issued with — 10, 60 and 120 s
/// from its oldest network read (its last second inside, its end outside); 9 and 121 s are never
/// issued (a re-sealed record claiming them is refused), nor a TTL that differs from its own
/// expiry. A later configuration never extends a record: its TTL is under its token.
#[test]
fn dec07_admission_holds_the_record_ttl_of_10_to_120_seconds() {
    let id = fixture_identity();
    let with_ttl = |ttl: u64| {
        let mut n = network_record_fixture(vec![]);
        n.ttl_seconds = ttl;
        n.expires_at = n.observed_at + ttl;
        n.reseal();
        n
    };
    for ttl in [10, 60, 120] {
        let n = with_ttl(ttl);
        let last = n.observed_at + ttl - 1;
        assert_eq!(admit(&n, n.observed_at, &id), Ok(()), "{ttl}");
        assert_eq!(admit(&n, last, &id), Ok(()), "{ttl}");
        assert_eq!(
            admit(&n, last + 1, &id).unwrap_err(),
            "BOOT_NETWORK_EXPIRED",
            "{ttl}"
        );
        assert!(n.validate_start(&id, "binary", last, "logs").is_ok());
        assert_eq!(
            n.validate_start(&id, "binary", last + 1, "logs")
                .unwrap_err(),
            "BOOT_NETWORK_EXPIRED"
        );
    }
    for ttl in [9, 121] {
        let n = with_ttl(ttl);
        let inside = n.observed_at + 1;
        assert_eq!(
            admit(&n, inside, &id).unwrap_err(),
            "BOOT_NETWORK_EXPIRED",
            "{ttl}"
        );
    }
    let mut stretched = with_ttl(60);
    stretched.expires_at += 60;
    stretched.reseal();
    assert_eq!(
        admit(&stretched, stretched.observed_at + 90, &id).unwrap_err(),
        "BOOT_NETWORK_EXPIRED"
    );
    // The old token after a configuration change: a 60 s record stays expired after 60 s
    // whatever the registry says now; rewriting its TTL breaks its token.
    let n = with_ttl(60);
    let mut longer = n.clone();
    (longer.ttl_seconds, longer.expires_at) = (120, n.observed_at + 120);
    assert_eq!(
        admit(&longer, n.observed_at + 90, &id).unwrap_err(),
        "BOOT_NETWORK_TOKEN"
    );
    assert_eq!(
        admit(&n, n.observed_at + 90, &id).unwrap_err(),
        "BOOT_NETWORK_EXPIRED"
    );
}

/// DEC-07: a record of the layout before the TTL field (schema 2) stays readable with its fixed
/// 60 s and the source `pre_dec07`; its token covers that layout, so it never admits a start.
#[test]
fn dec07_an_earlier_network_record_is_readable_and_never_admits() {
    use crate::analytics::payload::{self, Payload};
    let n = network_record_fixture(vec![]);
    let mut earlier = payload::encode_wire(&n).unwrap();
    earlier.truncate(earlier.len() - 16 - n.ttl_source.len());
    let read: NetworkAudit = payload::decode(NetworkAudit::KIND, payload::TYPED, &earlier).unwrap();
    assert_eq!(
        (read.ttl_seconds, read.ttl_source.as_str()),
        (60, TTL_SOURCE_PRE_DEC07)
    );
    assert_eq!(read.state_hash, n.state_hash);
    assert_eq!(
        admit(&read, n.observed_at + 1, &fixture_identity()).unwrap_err(),
        "BOOT_NETWORK_TOKEN"
    );
}

/// DEC-07: a standalone raw capture is admitted only to its approved network audit of boot mode
/// market — the record, its token and approval time inside the TTL, its identity, executables
/// and capture source, and exactly the audited duration; anything missing is
/// BOOT_AUDIT_REQUIRED (never a read).
#[test]
fn dec07_a_raw_capture_is_admitted_only_to_its_market_flow() {
    let d = temp("market");
    let n = network_record_fixture(vec![]);
    let path = d.join("network.p0j");
    let mut w = journal::Writer::create(&path).unwrap();
    w.append_typed(journal::Provenance::default(), &n).unwrap();
    w.seal(true).unwrap();
    let full: HashMap<&str, String> = [
        ("P0_MODE", "market".to_string()),
        (ENV_NETWORK_AUDIT, path.display().to_string()),
        (
            ENV_NETWORK_AUDIT_TOKEN,
            n.audit.readiness_token[..16].to_string(),
        ),
        (ENV_NETWORK_AUDIT_APPROVED_AT, "1001".into()),
        ("P0_MEASUREMENT_DURATION", "1m".into()),
        ("P0_MARKET_DURATION_SECS", "60".into()),
        ("P0_MARKET_SOURCE", "logs".into()),
    ]
    .into();
    let current = || Ok(fixture_identity());
    let binary = || Ok("binary".to_string());
    let config = || Ok(darkbot_solana::runtime_config::RuntimeConfig::default());
    let admit_at = |env: &HashMap<&str, String>, now: u64| {
        market_admission_with(&|k| env.get(k).cloned(), &current, &binary, &config, now)
            .map(|a| a.record)
    };
    let admit = |env: &HashMap<&str, String>| admit_at(env, 1002);
    // The capture's own clock: inside the record's TTL (1000..1060) and not before its approval.
    for now in [999, 1000, 1060, 4000] {
        assert_eq!(admit_at(&full, now), Err("BOOT_NETWORK_EXPIRED"), "{now}");
    }
    let mut ahead = full.clone();
    ahead.insert(ENV_NETWORK_AUDIT_APPROVED_AT, "1003".into());
    assert_eq!(
        admit(&ahead),
        Err("BOOT_NETWORK_EXPIRED"),
        "approved after now"
    );
    // Once: the admission is consumed; a relaunch with the same values is refused.
    assert_eq!(admit(&full), Ok(n.clone()));
    assert!(path.with_extension("admitted").is_file());
    assert_eq!(admit(&full), Err(BOOT_ADMISSION_CONSUMED));
    std::fs::remove_file(path.with_extension("admitted")).unwrap();
    for missing in ["P0_MODE", ENV_NETWORK_AUDIT, "P0_MEASUREMENT_DURATION"] {
        let mut env = full.clone();
        env.remove(missing);
        assert_eq!(admit(&env), Err(BOOT_AUDIT_REQUIRED), "{missing}");
    }
    let mut econ = full.clone();
    econ.insert("P0_MODE", "econ4h".into());
    assert_eq!(admit(&econ), Err(BOOT_AUDIT_REQUIRED), "only a market flow");
    let mut longer = full.clone();
    longer.insert("P0_MARKET_DURATION_SECS", "120".into());
    assert_eq!(
        admit(&longer),
        Err("BOOT_IDENTITY_DRIFT"),
        "the audited duration"
    );
    let mut late = full.clone();
    late.insert(ENV_NETWORK_AUDIT_APPROVED_AT, "1060".into());
    assert_eq!(admit(&late), Err("BOOT_NETWORK_EXPIRED"));
    let other = || Ok("other-binary".to_string());
    assert_eq!(
        market_admission_with(&|k| full.get(k).cloned(), &current, &other, &config, 1002)
            .map(|a| a.record),
        Err("BOOT_IDENTITY_DRIFT")
    );
    assert!(
        !path.with_extension("admitted").exists(),
        "a refusal consumes nothing"
    );
    let _ = std::fs::remove_dir_all(d);
}

/// DEC-16: an unbounded raw capture (`until-stopped:W:R`, the capture `until-stopped`) is
/// admitted only by `RunHorizon::admit` with its stored qualification bound to exactly it; the
/// admission hands its windows their binding, qualification and configuration.
#[test]
fn dec16_an_unbounded_raw_capture_is_admitted_only_with_its_qualification() {
    use crate::orchestration::until_stopped::{self as us, Part, PartState, Qualification};
    let d = temp("market_until_stopped");
    let n = network_record_fixture(vec![]);
    let path = d.join("network.p0j");
    let mut w = journal::Writer::create(&path).unwrap();
    w.append_typed(journal::Provenance::default(), &n).unwrap();
    w.seal(true).unwrap();
    let binding = us::Binding {
        mode: MARKET_MODE.into(),
        identity: fixture_identity(),
        binary_sha256: "binary".into(),
        checkpoint_seconds: 60,
        minimum_runway_seconds: 120,
    };
    let pass = Part {
        state: PartState::Pass,
        reason: "ok".into(),
    };
    let complete = Qualification {
        schema: us::SCHEMA,
        binding: binding.clone(),
        runtime: pass.clone(),
        sk07: pass.clone(),
        capacity: pass.clone(),
        capacity_need_bytes: Some(1),
        provider_budget: pass.clone(),
        provider_evidence: pass.clone(),
        evidence_id: None,
        evidence_valid_until_unix_s: Some(5_000),
        qualified_at_unix_s: 1_000,
    };
    let id = us::put(&d, &complete).unwrap();
    let full: HashMap<&str, String> = [
        ("P0_MODE", "market".to_string()),
        (ENV_NETWORK_AUDIT, path.display().to_string()),
        (
            ENV_NETWORK_AUDIT_TOKEN,
            n.audit.readiness_token[..16].to_string(),
        ),
        (ENV_NETWORK_AUDIT_APPROVED_AT, "1001".into()),
        ("P0_MEASUREMENT_DURATION", "until-stopped:1m:2m".into()),
        ("P0_MARKET_DURATION_SECS", "until-stopped".into()),
        ("P0_MARKET_SOURCE", "logs".into()),
        ("P0_PROJECT_DIR", d.display().to_string()),
        (ENV_UNTIL_STOPPED_QUALIFICATION, id.clone()),
    ]
    .into();
    let current = || Ok(fixture_identity());
    let binary = || Ok("binary".to_string());
    let config = || Ok(darkbot_solana::runtime_config::RuntimeConfig::default());
    let admit = |env: &HashMap<&str, String>| {
        let admitted =
            market_admission_with(&|k| env.get(k).cloned(), &current, &binary, &config, 1002);
        let _ = std::fs::remove_file(path.with_extension("admitted"));
        admitted
    };
    let rolling = admit(&full).unwrap().rolling.unwrap();
    assert_eq!(
        (&rolling.binding, rolling.qualification_id.as_str()),
        (&binding, id.as_str())
    );
    assert_eq!(rolling.project, d);
    let gated = crate::analytics::measurement::UNTIL_STOPPED_GATED;
    let mut without = full.clone();
    without.remove(ENV_UNTIL_STOPPED_QUALIFICATION);
    assert_eq!(admit(&without).err(), Some(gated));
    let mut fixed = full.clone();
    fixed.insert("P0_MARKET_DURATION_SECS", "60".into());
    assert_eq!(admit(&fixed).err(), Some("BOOT_IDENTITY_DRIFT"));
    // Another window than the qualification's, and an open part: gated.
    let mut other = full.clone();
    other.insert("P0_MEASUREMENT_DURATION", "until-stopped:30s:2m".into());
    assert_eq!(admit(&other).err(), Some(gated));
    let mut open = complete.clone();
    open.runtime = Part {
        state: PartState::NotRun,
        reason: "no rolling windows".into(),
    };
    let mut gaps = full.clone();
    gaps.insert(ENV_UNTIL_STOPPED_QUALIFICATION, us::put(&d, &open).unwrap());
    assert_eq!(admit(&gaps).err(), Some(gated));
    // A fixed capture is admitted as before, without rolling windows.
    let mut bounded = full.clone();
    bounded.insert("P0_MEASUREMENT_DURATION", "1m".into());
    bounded.insert("P0_MARKET_DURATION_SECS", "60".into());
    assert!(admit(&bounded).unwrap().rolling.is_none());
    // An autostarted capture runs only under the standing approval in force for exactly this
    // run, and takes its maximum runtime; a revoked one admits nothing.
    use super::super::autostart as auto;
    assert_eq!(rolling.max_runtime_seconds, None);
    let grant = auto::Grant {
        binding: binding.clone(),
        operational: vec![],
        duration: "until-stopped:1m:2m".into(),
        qualification_id: id.clone(),
        provider_evidence_id: "0123456789abcdef".into(),
        genesis_hash: "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d".into(),
        capture_source: "logs".into(),
        budget: auto::WindowBudget::of(&config().unwrap()),
        max_runtime_seconds: 600,
        expires_unix_s: 5_000,
    };
    let mut autostarted = full.clone();
    autostarted.insert(auto::ENV_AUTOSTART, grant.digest().unwrap());
    assert_eq!(admit(&autostarted).err(), Some("AUTOSTART_NOT_PERMITTED"));
    auto::record(
        &d,
        &auto::Event::Granted {
            grant: Box::new(grant.clone()),
            granted_unix_s: 1_000,
        },
    )
    .unwrap();
    let rolling = admit(&autostarted).unwrap().rolling.unwrap();
    assert_eq!(rolling.max_runtime_seconds, Some(600));
    let mut drifted = autostarted.clone();
    drifted.insert(
        ENV_UNTIL_STOPPED_QUALIFICATION,
        us::put(&d, &{
            let mut q = complete.clone();
            q.qualified_at_unix_s += 1;
            q
        })
        .unwrap(),
    );
    assert_eq!(admit(&drifted).err(), Some("AUTOSTART_NOT_PERMITTED"));
    auto::record(
        &d,
        &auto::Event::Revoked {
            revoked_unix_s: 1_002,
        },
    )
    .unwrap();
    assert_eq!(admit(&autostarted).err(), Some("AUTOSTART_NOT_PERMITTED"));
    // DEC-16 (stage 3): the same admission for an unbounded econ4h and Shadow run — its own
    // complete qualification admits it, a crashed run of it is continued, and it never
    // autostarts (D-5: only the raw capture does).
    let mut econ_binding = binding.clone();
    econ_binding.mode = "econ4h".into();
    let econ = us::put(
        &d,
        &Qualification {
            binding: econ_binding.clone(),
            ..complete.clone()
        },
    )
    .unwrap();
    let mut econ_env = full.clone();
    econ_env.insert(ENV_UNTIL_STOPPED_QUALIFICATION, econ.clone());
    let horizon = darkbot_solana::runtime_config::horizon("until-stopped:1m:2m").unwrap();
    let rolling_for = |env: &HashMap<&str, String>| {
        rolling_admission(
            &|k| env.get(k).cloned(),
            "econ4h",
            horizon,
            fixture_identity(),
            "binary".into(),
            &|| Ok(darkbot_solana::runtime_config::RuntimeConfig::default()),
            1002,
        )
    };
    let admitted = rolling_for(&econ_env).unwrap();
    assert_eq!(
        (
            admitted.binding,
            admitted.qualification_id,
            admitted.resume,
            admitted.autostart
        ),
        (econ_binding, econ.clone(), None, None)
    );
    let mut resumed = econ_env.clone();
    resumed.insert(us::ENV_RESUME, "crashed".into());
    assert_eq!(
        rolling_for(&resumed).unwrap().resume.as_deref(),
        Some("crashed")
    );
    let mut autostarted = econ_env.clone();
    autostarted.insert(auto::ENV_AUTOSTART, "0123456789abcdef".into());
    assert_eq!(
        rolling_for(&autostarted).err(),
        Some(gated),
        "econ4h never autostarts"
    );
    let _ = std::fs::remove_dir_all(d);
}
