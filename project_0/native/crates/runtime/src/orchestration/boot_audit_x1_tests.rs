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
    ]
    .into();
    let current = || Ok(fixture_identity());
    let binary = || Ok("binary".to_string());
    let run = |env: &HashMap<&str, String>, launched: bool| {
        admission_record(&|k| env.get(k).cloned(), launched, &current, &binary)
    };
    assert_eq!(run(&HashMap::new(), false), Ok(None));
    assert_eq!(run(&full, true), Ok(Some(n.clone())));
    assert_eq!(
        run(&full, false),
        Ok(Some(n.clone())),
        "a record is always checked"
    );
    assert_eq!(
        run(&HashMap::new(), true).unwrap_err(),
        "BOOT_NETWORK_AUDIT_MISSING"
    );
    for drop in [ENV_NETWORK_AUDIT_TOKEN, ENV_NETWORK_AUDIT_APPROVED_AT] {
        let mut env = full.clone();
        env.remove(drop);
        assert_eq!(run(&env, true).unwrap_err(), "BOOT_NETWORK_AUDIT_MISSING");
    }
    let mut env = full.clone();
    env.insert(ENV_NETWORK_AUDIT_APPROVED_AT, "soon".into());
    assert_eq!(run(&env, true).unwrap_err(), "BOOT_NETWORK_AUDIT_MISSING");
    for bad in [d.join("absent.p0j"), garbage] {
        let mut env = full.clone();
        env.insert(ENV_NETWORK_AUDIT, bad.display().to_string());
        assert_eq!(
            run(&env, true).unwrap_err(),
            "BOOT_NETWORK_EVIDENCE_INVALID"
        );
    }
    let mut env = full.clone();
    env.insert(ENV_NETWORK_AUDIT_APPROVED_AT, "1060".into());
    assert_eq!(run(&env, true).unwrap_err(), "BOOT_NETWORK_EXPIRED");
    let mut env = full.clone();
    env.insert("P0_MARKET_SOURCE", "transactions".into());
    assert_eq!(
        run(&env, true).unwrap_err(),
        "BOOT_IDENTITY_DRIFT: capture_source"
    );
    let failing = || Err::<AuditedIdentity, _>("BOOT_NETWORK_AUDIT_MISSING: P0_PROJECT_DIR".into());
    assert_eq!(
        admission_record(&|k| full.get(k).cloned(), true, &failing, &binary).unwrap_err(),
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

/// The config p0ctl audits (operational args + `--duration D`, operator environment) and the
/// config the launched runtime re-resolves (same args, launcher environment with
/// `P0_MEASUREMENT_DURATION=D` and its fixed exports, also after the runtime's own activation
/// exported the config back into its environment) have the same digest; a launcher environment
/// that changes an operational value does not.
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
    let launcher_exports = |duration: &str, secs: u64| -> HashMap<String, String> {
        let mut env = operator.clone();
        for (k, v) in [
            ("P0_MEASUREMENT_DURATION", duration.to_string()),
            ("P0_ECON_DURATION_SECS", secs.to_string()),
            ("P0_MODE", "econ4h".into()),
            ("P0_LIVE_ENABLED", "false".into()),
            ("P0_SUBMISSION_ENABLED", "false".into()),
            ("P0_MARKET_SOURCE", "logs".into()),
            (
                "P0_PROGRAM_ID",
                darkbot_project0::protocol::PROGRAM_ID_MAINNET.into(),
            ),
            ("P0_RECOVERY_ROOT_CATCHUP_SECS", "120".into()),
            ("P0_RECOVERY_ROOT_STALL_SECS", "60".into()),
            ("P0_RECOVERY_ROOT_HARD_MAX_SECS", "300".into()),
            ("P0_INGRESS_BUFFER", "65536".into()),
            ("P0_MIN_PROFIT", "0".into()),
            ("P0_MAX_CONCURRENT", "1".into()),
            ("P0_MAX_OUTSTANDING", "1".into()),
            ("P0_MAX_AGE_NS", "2000000000".into()),
            ("P0_DEADLINE_NS", "2000000000".into()),
            ("P0_JUPITER_MAX_ACCOUNTS", "64".into()),
            ("P0_CTL_LAUNCH_RUN_ID", "econ4h_x1".into()),
        ] {
            env.insert(k.into(), v);
        }
        env
    };
    let ctl_identity = |op: &[String], duration: &str| {
        let get = |k: &str| operator.get(k).cloned();
        let config = start_config(op, duration, &get).unwrap().config;
        identity_with(&d, flags, &config).unwrap()
    };
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for op in [
        args(&[]),
        args(&["--profile", "standard"]),
        args(&["--profile", "smoke"]),
        args(&["--profile", "performance"]),
        args(&["--rpc-rps", "3", "--fallback", "off"]),
    ] {
        for (duration, secs) in [("10m", 600), ("1h", 3600)] {
            let audited = ctl_identity(&op, duration);
            let env = launcher_exports(duration, secs);
            let get = |k: &str| env.get(k).cloned();
            assert_eq!(
                launched_identity(&d, flags, &op, &get).unwrap(),
                audited,
                "{op:?} {duration}"
            );
            // The runtime's own activation exports its config into the environment first.
            let mut activated = env.clone();
            let own = darkbot_solana::runtime_config::Resolved::parse(&op, &get).unwrap();
            activated.extend(own.config.environment());
            let get = |k: &str| activated.get(k).cloned();
            assert_eq!(
                launched_identity(&d, flags, &op, &get).unwrap(),
                audited,
                "{op:?} {duration} after activation"
            );
        }
    }
    // Operational values the launcher environment changes are drift, never admitted.
    let audited = ctl_identity(&[], "10m");
    for (k, v) in [
        ("P0_RPC_URL", "http://127.0.0.2:8899"),
        ("P0_RPC_RPS", "9"),
        ("P0_PROFILE", "smoke"),
    ] {
        let mut env = launcher_exports("10m", 600);
        env.insert(k.into(), v.into());
        let get = |k: &str| env.get(k).cloned();
        let current = launched_identity(&d, flags, &[], &get).unwrap();
        assert_ne!(
            current.runtime_config_sha256, audited.runtime_config_sha256,
            "{k}"
        );
    }
    let mut env = launcher_exports("10m", 600);
    env.remove("P0_MEASUREMENT_DURATION");
    let get = |k: &str| env.get(k).cloned();
    assert_eq!(
        launched_identity(&d, flags, &[], &get).unwrap_err(),
        "BOOT_NETWORK_AUDIT_MISSING: P0_MEASUREMENT_DURATION"
    );
    std::fs::remove_dir_all(d).unwrap();
}
