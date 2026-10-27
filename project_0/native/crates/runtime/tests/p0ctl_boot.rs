//! DEC-07: `p0ctl boot` over the real binary, offline only: the offline step records a flow
//! with the run's duration, the registry's TTL and its source; a TTL outside 10..=120 s is
//! refused before any flow; each approval must name its own token; a start needs the flow's
//! READY network audit and exactly the flow's duration; the flow's identity is the one `run
//! start` audits; a flow id is used once. No network read happens.
use darkbot_runner::orchestration::boot_control;
use std::process::Command;

fn p0ctl(project: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
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

fn offline<'a>(flow: &'a str, duration: &'a str, ttl: Option<&'a str>) -> Vec<&'a str> {
    let mut args = vec!["boot", "offline", "--flow", flow, "--duration", duration];
    if let Some(ttl) = ttl {
        args.extend(["--", "--boot-token-ttl-seconds", ttl]);
    }
    args
}

#[test]
fn dec07_p0ctl_boot_records_a_flow_and_refuses_unapproved_steps() {
    let d = std::env::temp_dir().join(format!("p0ctl_boot_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    for bad in ["9", "121"] {
        let (code, _, err) = p0ctl(&d, &offline("f0", "10m", Some(bad)));
        assert_eq!(code, 2, "{err}");
        assert!(err.contains("boot-token-ttl-seconds"), "{err}");
    }
    assert_eq!(
        boot_control::status(&d, "f0").unwrap_err(),
        "BOOT_FLOW_UNKNOWN"
    );
    let (code, _, err) = p0ctl(&d, &["boot", "offline", "--flow", "f9"]);
    assert_eq!(code, 2);
    assert!(err.contains("--duration required"), "{err}");
    let (_, out, err) = p0ctl(&d, &offline("f1", "10m", Some("90")));
    let s = boot_control::status(&d, "f1").unwrap_or_else(|e| panic!("{e}\n{out}\n{err}"));
    assert_eq!(
        (
            s.flow.ttl_seconds,
            s.flow.ttl_source.as_str(),
            s.flow.duration.as_str()
        ),
        (90, "cli", "10m")
    );
    assert_eq!(s.flow.operational, ["--boot-token-ttl-seconds", "90"]);
    let token = s.flow.offline.readiness_token[..16].to_string();
    let (code, _, err) = p0ctl(
        &d,
        &[
            "boot",
            "network",
            "--flow",
            "f1",
            "--approve",
            "0000000000000000",
            "--genesis-hash",
            "unused",
            "--provider-evidence",
            "/nonexistent.p0j",
        ],
    );
    assert_eq!(code, 2);
    assert!(
        err.contains("BOOT_NETWORK_APPROVAL_MISMATCH") || err.contains("BOOT_OFFLINE_BLOCKED"),
        "{err}"
    );
    // The start is exactly what was audited: another duration is refused before any worker;
    // the flow's own duration still needs the READY network audit.
    let start = |duration: &str| {
        p0ctl(
            &d,
            &[
                "boot",
                "start",
                "--flow",
                "f1",
                "--approve",
                &token,
                "--duration",
                duration,
            ],
        )
    };
    let (code, _, err) = start("1h");
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_DURATION_MISMATCH"), "{err}");
    let (code, _, err) = start("10m");
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_NETWORK_NOT_READY"), "{err}");
    let s = boot_control::status(&d, "f1").unwrap();
    assert_eq!((s.network.name(), s.start.name()), ("NOT_RUN", "NOT_RUN"));
    let (code, _, err) = p0ctl(&d, &offline("f1", "10m", None));
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_FLOW_EXISTS"), "{err}");
    let (code, out, _) = p0ctl(&d, &["boot", "status"]);
    assert_eq!(code, 0);
    assert!(out.contains("flow f1 mode=econ4h ttl=90s[cli]"), "{out}");
    // Review HIGH: a flow's offline identity is exactly the one `run start` audits for the
    // flow's duration (one resolution, `start_config`); another duration is another identity.
    let (_, out, _) = p0ctl(&d, &["run", "start", "--duration", "10m"]);
    let started = out
        .lines()
        .find_map(|l| l.split("token=").nth(1))
        .map(|t| t[..16].to_string())
        .unwrap_or_else(|| panic!("{out}"));
    let token_of = |flow: &str, duration: &str| {
        let _ = p0ctl(&d, &offline(flow, duration, None));
        boot_control::status(&d, flow)
            .unwrap()
            .flow
            .offline
            .readiness_token[..16]
            .to_string()
    };
    assert_eq!(token_of("f10", "10m"), started);
    assert_ne!(token_of("f1h", "1h"), started);
    let _ = std::fs::remove_dir_all(&d);
}

/// DEC-07: a raw-capture flow (mode market) takes no operational arguments and audits the
/// configured capture source; its start hands the desktop launcher exactly the admission of the
/// flow's record, token, approval time, mode and duration (`darkbotctl capture start SECONDS`),
/// and a refusal of the launcher is the step's FAILED outcome.
#[test]
fn dec07_a_raw_capture_flow_starts_through_its_launcher_with_its_admission() {
    use std::os::unix::fs::PermissionsExt;
    let d = std::env::temp_dir().join(format!("p0ctl_boot_market_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    let run = |args: &[&str], ctl: Option<&std::path::Path>| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_p0ctl"));
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("P0_MARKET_SOURCE", "logs")
            .arg("--project")
            .arg(&d)
            .args(args);
        if let Some(ctl) = ctl {
            cmd.env("DARKBOT_CTL", ctl);
        }
        let out = cmd.output().unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (code, err) = run(
        &[
            "boot",
            "offline",
            "--flow",
            "m0",
            "--mode",
            "market",
            "--duration",
            "10m",
            "--",
            "--x",
            "1",
        ],
        None,
    );
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_MARKET_ARGS"), "{err}");
    for flow in ["m1", "m2"] {
        let _ = run(
            &[
                "boot",
                "offline",
                "--flow",
                flow,
                "--mode",
                "market",
                "--duration",
                "10m",
            ],
            None,
        );
        assert_eq!(boot_control::status(&d, flow).unwrap().flow.mode, "market");
    }
    let (code, err) = run(
        &[
            "boot",
            "network",
            "--flow",
            "m1",
            "--approve",
            "0000000000000000",
            "--genesis-hash",
            "unused",
            "--provider-evidence",
            "/unused.p0j",
            "--capture-source",
            "transactions",
        ],
        None,
    );
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_CAPTURE_SOURCE_MISMATCH"), "{err}");
    // The start worker (internal `boot run`) with a recording launcher.
    let script = |name: &str, body: &str| {
        let p = d.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    };
    let calls = d.join("calls.log");
    let ok = script(
        "ok-ctl",
        &format!(
            "printf '%s|%s|%s|%s|%s|%s\\n' \"$*\" \"$P0_MODE\" \"$P0_MEASUREMENT_DURATION\" \"$P0_NETWORK_AUDIT\" \"$P0_NETWORK_AUDIT_TOKEN\" \"$P0_NETWORK_AUDIT_APPROVED_AT\" >> '{}'; echo 'Capture gestartet pid=1 dauer=600 launch=l1'",
            calls.display()
        ),
    );
    let token = "0123456789abcdef";
    let (code, err) = run(
        &[
            "boot",
            "run",
            "--flow",
            "m1",
            "--step",
            "START",
            "--approve",
            token,
        ],
        Some(&ok),
    );
    assert_eq!(code, 0, "{err}");
    let call = std::fs::read_to_string(&calls).unwrap();
    // p0ctl resolves the project directory canonically.
    let record = boot_control::network_record_path(&d.canonicalize().unwrap(), "m1").unwrap();
    let fields: Vec<&str> = call.trim().split('|').collect();
    assert_eq!(
        fields[..5],
        [
            "capture start 600",
            "market",
            "10m",
            record.display().to_string().as_str(),
            token
        ]
    );
    assert!(
        fields[5].parse::<u64>().unwrap() > 0,
        "approved now: {call}"
    );
    // The step names the capture its launcher started.
    match boot_control::status(&d, "m1").unwrap().start {
        boot_control::StepState::Done(o) => assert_eq!(
            (o.state.as_str(), o.run_id.as_deref()),
            ("COMPLETED", Some("app-l1"))
        ),
        other => panic!("{other:?}"),
    }
    let refusing = script("no-ctl", "echo 'BOOT_AUDIT_REQUIRED: refused' >&2; exit 1");
    let (code, _) = run(
        &[
            "boot",
            "run",
            "--flow",
            "m2",
            "--step",
            "START",
            "--approve",
            token,
        ],
        Some(&refusing),
    );
    assert_eq!(code, 2);
    let s = boot_control::status(&d, "m2").unwrap();
    match &s.start {
        boot_control::StepState::Done(o) => assert_eq!(
            (o.state.as_str(), o.code.as_str()),
            (
                "FAILED",
                "BOOT_RAW_CAPTURE_REFUSED: BOOT_AUDIT_REQUIRED: refused"
            )
        ),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn dec07_provider_evidence_is_named_by_its_imported_record_id() {
    let d = std::env::temp_dir().join(format!("p0ctl_boot_evidence_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("operator")).unwrap();
    let store = d.join("data/provider_evidence");
    // The operator's typed record: a measured JSON through p0-journal's explicit import.
    let (json, record) = (
        d.join("operator/measured.json"),
        d.join("operator/evidence.p0j"),
    );
    std::fs::write(&json, br#"{"policy":"FREE_ONLY","passed":true,"provider_binding":"b","measured_at_unix_s":1.0,"evidence_sha256":null,"allowed_rps":1.0,"available_calls":1}"#).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_p0-journal"))
        .arg("import-provider-evidence")
        .args([&json, &record])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for flow in ["n1", "n2", "n3", "n4"] {
        let _ = p0ctl(&d, &offline(flow, "10m", None));
    }
    // An unapproved step imports nothing.
    let network = |flow: &str, evidence: &str| {
        p0ctl(
            &d,
            &[
                "boot",
                "network",
                "--flow",
                flow,
                "--approve",
                "0000000000000000",
                "--genesis-hash",
                "unused",
                "--provider-evidence",
                evidence,
            ],
        )
    };
    let (code, _, err) = network("n4", record.to_str().unwrap());
    assert_eq!(code, 2);
    assert!(
        err.contains("BOOT_NETWORK_APPROVAL_MISMATCH") || err.contains("BOOT_OFFLINE_BLOCKED"),
        "{err}"
    );
    assert!(!store.exists(), "nothing imported for an unapproved step");
    // The import: the id names the record's bytes; a second import is the same record.
    let import =
        |file: &std::path::Path| p0ctl(&d, &["boot", "evidence", "import", file.to_str().unwrap()]);
    let (code, out, err) = import(&record);
    assert_eq!(code, 0, "{err}");
    let id = out.split_whitespace().nth(1).unwrap().to_string();
    assert!(
        out.starts_with("PROVIDER_EVIDENCE_ID ") && id.len() == 16,
        "{out}"
    );
    let stored = store.join(format!("{id}.p0j"));
    assert_eq!(
        std::fs::read(&stored).unwrap(),
        std::fs::read(&record).unwrap()
    );
    assert_eq!(import(&record).1, out);
    // A file that is no PROVIDER_EVIDENCE record is refused and leaves nothing behind.
    let (code, _, err) = import(&json);
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_PROVIDER_EVIDENCE_FORMAT"), "{err}");
    assert_eq!(std::fs::read_dir(&store).unwrap().count(), 1);
    // The worker resolves ids only: a path, an id never imported and a changed record are each
    // refused before any read (the step's outcome names it).
    let worker = |flow: &str, evidence: &str| {
        p0ctl(
            &d,
            &[
                "boot",
                "run",
                "--flow",
                flow,
                "--step",
                "NETWORK",
                "--approve",
                "0000000000000000",
                "--genesis-hash",
                "unused",
                "--provider-evidence",
                evidence,
                "--capture-source",
                "logs",
            ],
        )
    };
    let failed = |flow: &str| match boot_control::status(&d, flow).unwrap().network {
        boot_control::StepState::Done(o) => (o.state, o.code),
        other => panic!("{other:?}"),
    };
    let _ = worker("n1", stored.to_str().unwrap());
    let (state, code) = failed("n1");
    assert!(
        state == "FAILED" && code.starts_with("BOOT_PROVIDER_EVIDENCE_ID_INVALID"),
        "{code}"
    );
    let _ = worker("n2", "0123456789abcdef");
    assert_eq!(
        failed("n2"),
        ("FAILED".into(), "BOOT_PROVIDER_EVIDENCE_UNKNOWN".into())
    );
    std::fs::write(&stored, b"other bytes").unwrap();
    let _ = worker("n3", &id);
    assert_eq!(
        failed("n3"),
        ("FAILED".into(), "BOOT_PROVIDER_EVIDENCE_CHANGED".into())
    );
    // A stored id never takes other bytes.
    let (code, _, err) = import(&record);
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_PROVIDER_EVIDENCE_ID_COLLISION"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

/// DEC-16: an unbounded raw-capture flow (`until-stopped:W:R`): the offline step records it, the
/// start needs its qualification (missing or unknown: refused before any worker), and the start
/// worker runs the capture `until-stopped` admitted with exactly that qualification.
#[test]
fn dec16_an_unbounded_raw_capture_flow_starts_until_stopped_with_its_qualification() {
    use std::os::unix::fs::PermissionsExt;
    let d = std::env::temp_dir().join(format!("p0ctl_boot_unbounded_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("data")).unwrap();
    // The binding names p0ctl and the capture beside it: a copied p0ctl with a stand-in capture
    // keeps the test hermetic (the capture is built by the reporter crate).
    let bin = d.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_p0ctl"), bin.join("p0ctl")).unwrap();
    std::fs::write(bin.join("p0-market-capture-reported"), b"capture stand-in").unwrap();
    let run = |args: &[&str], ctl: Option<&std::path::Path>| {
        let mut cmd = Command::new(bin.join("p0ctl"));
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("P0_MARKET_SOURCE", "logs")
            .arg("--project")
            .arg(&d)
            .args(args);
        if let Some(ctl) = ctl {
            cmd.env("DARKBOT_CTL", ctl);
        }
        let out = cmd.output().unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let horizon = "until-stopped:1m:2m";
    let (code, err) = run(
        &[
            "boot",
            "offline",
            "--flow",
            "u1",
            "--mode",
            "market",
            "--duration",
            horizon,
        ],
        None,
    );
    assert!(code == 0 || code == 2, "{err}");
    assert_eq!(
        boot_control::status(&d, "u1").unwrap().flow.duration,
        horizon
    );
    let start = |extra: &[&str]| {
        let mut args = vec![
            "boot",
            "start",
            "--flow",
            "u1",
            "--approve",
            "0123456789abcdef",
        ];
        args.extend(extra);
        run(&args, None)
    };
    let (code, err) = start(&[]);
    assert_eq!(code, 2);
    assert!(
        err.contains("UNTIL_STOPPED_QUALIFICATION_REQUIRED"),
        "{err}"
    );
    let (code, err) = start(&["--qualification", "0123456789abcdef"]);
    assert_eq!(code, 2);
    assert!(err.contains("UNTIL_STOPPED_QUALIFICATION_UNKNOWN"), "{err}");
    // The start worker (internal `boot run`) with a recording launcher.
    let calls = d.join("calls.log");
    let ctl = d.join("ok-ctl");
    std::fs::write(
        &ctl,
        format!(
            "#!/bin/sh\nprintf '%s|%s|%s|%s\\n' \"$*\" \"$P0_MEASUREMENT_DURATION\" \"$P0_UNTIL_STOPPED_QUALIFICATION\" \"$P0_UNTIL_STOPPED_AUTOSTART\" >> '{}'; echo 'Capture gestartet pid=1 dauer=until-stopped launch=l9'\n",
            calls.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&ctl, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (code, err) = run(
        &[
            "boot",
            "run",
            "--flow",
            "u1",
            "--step",
            "START",
            "--approve",
            "0123456789abcdef",
            "--qualification",
            "fedcba9876543210",
            "--autostart",
            "89abcdef01234567",
        ],
        Some(&ctl),
    );
    assert_eq!(code, 0, "{err}");
    // An autostarted capture names the standing approval it runs under (it admits itself).
    assert_eq!(
        std::fs::read_to_string(&calls).unwrap().trim(),
        format!("capture start until-stopped|{horizon}|fedcba9876543210|89abcdef01234567")
    );
    // A standing approval autostarts only an unbounded run.
    let (code, err) = run(
        &[
            "boot",
            "offline",
            "--flow",
            "f1",
            "--mode",
            "market",
            "--duration",
            "1m",
        ],
        None,
    );
    assert!(code == 0 || code == 2, "{err}");
    let (code, err) = run(
        &[
            "boot",
            "start",
            "--flow",
            "f1",
            "--approve",
            "0123456789abcdef",
            "--autostart",
            "89abcdef01234567",
        ],
        None,
    );
    assert_eq!(code, 2);
    assert!(err.contains("AUTOSTART_UNBOUNDED_ONLY"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}
