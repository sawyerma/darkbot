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
