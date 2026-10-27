//! DEC-16: `p0ctl autostart` over the real binary, offline only: the standing approval of an
//! unbounded raw capture's autostart is given only for a run its qualification admits, records
//! exactly the previewed binding (its digest), is revoked at once, and an autostart without one
//! starts nothing. No network read: the autostart under a grant stops at the network step,
//! whose provider evidence was never imported.
use darkbot_runner::orchestration::{autostart, boot_control, until_stopped as us};
use std::process::Command;

const GENESIS: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";

struct Ctl {
    bin: std::path::PathBuf,
    project: std::path::PathBuf,
}
impl Ctl {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("p0ctl_autostart_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (bin, project) = (root.join("bin"), root.join("project"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(project.join("data")).unwrap();
        // The binding names p0ctl and the capture beside it (a stand-in keeps it hermetic).
        std::fs::copy(env!("CARGO_BIN_EXE_p0ctl"), bin.join("p0ctl")).unwrap();
        std::fs::write(bin.join("p0-market-capture-reported"), b"capture stand-in").unwrap();
        Self { bin, project }
    }
    fn run(&self, args: &[&str]) -> (i32, String) {
        self.run_env(args, &[])
    }
    fn run_env(&self, args: &[&str], env: &[(&str, &std::path::Path)]) -> (i32, String) {
        let out = Command::new(self.bin.join("p0ctl"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("P0_MARKET_SOURCE", "logs")
            .envs(env.iter().copied())
            .arg("--project")
            .arg(&self.project)
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    }
    /// A qualification of an unbounded raw capture of 1-min windows (2-min runway) as p0ctl
    /// binds it; `complete` turns its open parts into passes (the gating under test, never a
    /// production path).
    fn qualification(&self, complete: bool) -> String {
        let (_, out) = self.run(&[
            "horizon",
            "qualify",
            "--mode",
            "market",
            "--checkpoint",
            "1m",
            "--minimum-runway",
            "2m",
        ]);
        let id = out
            .lines()
            .find_map(|l| l.strip_prefix("UNTIL_STOPPED_QUALIFICATION "))
            .and_then(|l| l.split_whitespace().next())
            .unwrap_or_else(|| panic!("{out}"))
            .to_string();
        if !complete {
            return id;
        }
        let mut q = us::get(&self.project, &id).unwrap();
        let pass = us::Part {
            state: us::PartState::Pass,
            reason: "fixture".into(),
        };
        (
            q.runtime,
            q.capacity,
            q.provider_budget,
            q.provider_evidence,
        ) = (pass.clone(), pass.clone(), pass.clone(), pass);
        q.evidence_id = Some("0123456789abcdef".into());
        q.evidence_valid_until_unix_s = Some(now() + 86_400);
        us::put(&self.project, &q).unwrap()
    }
    fn preview(&self, qualification: &str, extra: &[&str]) -> (i32, String) {
        let mut args = vec![
            "autostart",
            "preview",
            "--qualification",
            qualification,
            "--checkpoint",
            "1m",
            "--minimum-runway",
            "2m",
            "--genesis-hash",
            GENESIS,
            "--max-runtime",
            "1h",
        ];
        args.extend(extra);
        self.run(&args)
    }
}
impl Drop for Ctl {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.bin.parent().unwrap());
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn dec16_without_a_standing_approval_an_autostart_starts_nothing() {
    let ctl = Ctl::new("none");
    let (code, out) = ctl.run(&["autostart", "status"]);
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_NONE"), "{out}");
    let (code, out) = ctl.run(&["autostart", "revoke"]);
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_NOT_GRANTED"), "{out}");
    let (code, out) = ctl.run(&["autostart", "start"]);
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_NOT_GRANTED"), "{out}");
    assert!(!ctl.project.join(boot_control::BOOT_DIR).exists());
    // An incomplete qualification gets no standing approval; nothing is recorded.
    let open = ctl.qualification(false);
    let (code, out) = ctl.preview(&open, &["--valid-for", "1h"]);
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_UNQUALIFIED"), "{out}");
    assert!(!ctl.project.join(autostart::STORE).exists());
}

#[test]
fn dec16_a_standing_approval_records_exactly_its_previewed_binding_until_revoked() {
    let ctl = Ctl::new("grant");
    let qid = ctl.qualification(true);
    let expires = (now() + 3_600).to_string();
    let fixed = ["--expires-unix-s", expires.as_str()];
    let digest_of = |out: &str| {
        out.lines()
            .find_map(|l| l.strip_prefix("AUTOSTART_PREVIEW "))
            .and_then(|l| l.split(':').next())
            .unwrap_or_else(|| panic!("{out}"))
            .to_string()
    };
    let (code, out) = ctl.preview(&qid, &fixed);
    assert_eq!(code, 0, "{out}");
    for shown in [
        "mode               market",
        "horizon            until-stopped:1m:2m",
        "provider_evidence  0123456789abcdef",
        "max_runtime        3600 s",
        GENESIS,
    ] {
        assert!(out.contains(shown), "{shown}\n{out}");
    }
    let digest = digest_of(&out);
    assert!(
        out.contains(&format!("p0ctl autostart grant --confirm {digest}")),
        "{out}"
    );
    assert!(
        autostart::current(&ctl.project).unwrap().is_none(),
        "a preview grants nothing"
    );
    // A grant confirms exactly the preview shown: another digest, or one a newer preview
    // replaced, is refused.
    let grant = |d: &str| ctl.run(&["autostart", "grant", "--confirm", d]);
    let (code, out) = grant("0000000000000000");
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_CONFIRM_MISMATCH"), "{out}");
    let mut longer = fixed.to_vec();
    longer.extend(["--max-runtime", "2h"]);
    let (code, out) = ctl.run(
        &[
            &[
                "autostart",
                "preview",
                "--qualification",
                &qid,
                "--checkpoint",
                "1m",
                "--minimum-runway",
                "2m",
                "--genesis-hash",
                GENESIS,
            ][..],
            &longer[..],
        ]
        .concat(),
    );
    assert_eq!(code, 0, "{out}");
    assert_ne!(digest_of(&out), digest);
    let (code, out) = grant(&digest);
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_CONFIRM_MISMATCH"), "{out}");
    let (code, out) = ctl.preview(&qid, &fixed);
    assert_eq!((code, digest_of(&out)), (0, digest.clone()), "{out}");
    let (code, out) = grant(&digest);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!("AUTOSTART_GRANTED {digest}")),
        "{out}"
    );
    assert_eq!(autostart::preview(&ctl.project), Ok(None));
    let (_, grant, _) = autostart::current(&ctl.project).unwrap().unwrap();
    assert_eq!(grant.digest().unwrap(), digest);
    let (code, out) = ctl.run(&["autostart", "status"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!("AUTOSTART_PERMITTED {digest}")),
        "{out}"
    );
    // The autostart runs the boot flow under it: offline, then the network step with the
    // grant's evidence (never imported here: refused before any read).
    let (code, out) = ctl.run(&["autostart", "start"]);
    assert_eq!(code, 2, "{out}");
    let events = autostart::events(&ctl.project).unwrap();
    if out.contains("AUTOSTART_OFFLINE_BLOCKED") {
        assert_eq!(events.len(), 1, "a blocked offline audit is no use");
    } else {
        assert!(out.contains("BOOT_PROVIDER_EVIDENCE_UNKNOWN"), "{out}");
        let Some((
            _,
            autostart::Event::Used {
                digest: used,
                flow_id,
                ..
            },
        )) = events.last()
        else {
            panic!("{events:?}")
        };
        assert_eq!(used, &digest);
        let flow = boot_control::status(&ctl.project, flow_id).unwrap();
        assert_eq!(flow.flow.duration, "until-stopped:1m:2m");
        assert_eq!(flow.network, boot_control::StepState::NotRun);
    }
    // A revocation ends it at once.
    let (code, out) = ctl.run(&["autostart", "revoke"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains(&format!("AUTOSTART_REVOKED {digest}")),
        "{out}"
    );
    let (code, out) = ctl.run(&["autostart", "status"]);
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_NONE"), "{out}");
    let (code, out) = ctl.run(&["autostart", "start"]);
    assert_eq!(code, 2);
    assert!(out.contains("AUTOSTART_NOT_GRANTED"), "{out}");
}

/// DEC-16 (user decision 27.10.): an approved unbounded raw-capture start continues the crashed
/// run of exactly its binding and qualification — the launcher gets that run id and the start
/// names it; a newer run that halted is never continued, the start is then a new run.
#[test]
fn dec16_an_unbounded_start_names_the_crashed_run_it_continues() {
    use std::os::unix::fs::PermissionsExt;
    let ctl = Ctl::new("resume");
    let qid = ctl.qualification(true);
    let binding = us::get(&ctl.project, &qid).unwrap().binding;
    let market = ctl.project.join("data/market");
    std::fs::create_dir_all(market.join("crashed")).unwrap();
    // As a killed capture leaves it: the windows journal never sealed.
    drop(
        us::RollingWindows::start(&market.join("crashed"), binding.clone(), &qid, now(), 0)
            .unwrap(),
    );
    let calls = ctl.project.join("calls.log");
    let launcher = ctl.project.join("ok-ctl");
    std::fs::write(
        &launcher,
        format!(
            "#!/bin/sh\nprintf '%s|%s\\n' \"$*\" \"$P0_UNTIL_STOPPED_RESUME\" >> '{}'; echo 'Capture gestartet pid=1 dauer=until-stopped launch=l9'\n",
            calls.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    let start = |flow: &str| {
        let (code, out) = ctl.run(&[
            "boot",
            "offline",
            "--flow",
            flow,
            "--mode",
            "market",
            "--duration",
            "until-stopped:1m:2m",
        ]);
        assert!(code == 0 || code == 2, "{out}");
        let (code, out) = ctl.run_env(
            &[
                "boot",
                "run",
                "--flow",
                flow,
                "--step",
                "START",
                "--approve",
                "0123456789abcdef",
                "--qualification",
                &qid,
            ],
            &[("DARKBOT_CTL", &launcher)],
        );
        assert_eq!(code, 0, "{out}");
        let line = std::fs::read_to_string(&calls)
            .unwrap()
            .lines()
            .last()
            .unwrap()
            .to_string();
        let run = match boot_control::status(&ctl.project, flow).unwrap().start {
            boot_control::StepState::Done(o) => o.run_id,
            other => panic!("{other:?}"),
        };
        (line, run, out)
    };
    let (line, run, out) = start("u1");
    assert_eq!(line, "capture start until-stopped|crashed");
    assert_eq!(run.as_deref(), Some("crashed"));
    assert!(
        out.contains("continues the crashed unbounded run crashed"),
        "{out}"
    );
    // A newer run that halted: never continued — the next start is a new run.
    let halted = market.join("halted");
    std::fs::create_dir_all(&halted).unwrap();
    let mut w = us::RollingWindows::start(&halted, binding.clone(), &qid, now(), 0).unwrap();
    let empty = ctl.project.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let check = w
        .close(
            &darkbot_solana::runtime_config::RuntimeConfig::default(),
            "logs",
            us::Measured {
                now: now() + 60,
                bytes: 0,
                run_volume: &empty,
                environment: darkbot_runner::storage::preflight::Environment::default(),
                provider: darkbot_solana::provider::WindowUsage {
                    requests: 0,
                    bytes: 0,
                },
                evidence: None,
                sealed: None,
            },
        )
        .unwrap();
    assert_ne!(check.verdict, us::CONTINUE);
    drop(w);
    let (line, run, _) = start("u2");
    assert_eq!(line, "capture start until-stopped|");
    assert_eq!(run.as_deref(), Some("app-l9"));
}
