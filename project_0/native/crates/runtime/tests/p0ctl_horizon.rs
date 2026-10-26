//! DEC-16: `p0ctl horizon` and `run start --until-stopped` over the real binary, offline only:
//! a qualification records every part and is stored by id; an unbounded start is admitted only
//! by `RunHorizon::admit` with a complete qualification bound to exactly the run — without one,
//! with an open part or another horizon it is refused before any start step. No network read.
use std::process::Command;

struct Ctl {
    bin: std::path::PathBuf,
    project: std::path::PathBuf,
}
impl Ctl {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("p0ctl_horizon_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (bin, project) = (root.join("bin"), root.join("project"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(project.join("data")).unwrap();
        // The binding names p0ctl and the runtime beside it: a copied p0ctl with a stand-in
        // runtime keeps the test hermetic (the runtimes are built by the reporter crate).
        std::fs::copy(env!("CARGO_BIN_EXE_p0ctl"), bin.join("p0ctl")).unwrap();
        std::fs::write(bin.join("p0-longtail"), b"runtime stand-in").unwrap();
        Self { bin, project }
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = Command::new(self.bin.join("p0ctl"))
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .arg("--project")
            .arg(&self.project)
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}
impl Drop for Ctl {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.bin.parent().unwrap());
    }
}

#[test]
fn dec16_an_unbounded_run_is_admitted_only_with_its_complete_qualification() {
    let ctl = Ctl::new("admit");
    let qualify = |extra: &[&str]| {
        let mut args = vec![
            "horizon",
            "qualify",
            "--mode",
            "econ4h",
            "--checkpoint",
            "1h",
            "--minimum-runway",
            "24h",
        ];
        args.extend(extra);
        ctl.run(&args)
    };
    let (code, out, err) = qualify(&[]);
    assert_eq!(code, 2, "{err}");
    for part in [
        "runtime            NOT_RUN",
        "sk07               PASS",
        "capacity           NOT_RUN",
        "provider_evidence  NOT_RUN",
    ] {
        assert!(out.contains(part), "{part}\n{out}");
    }
    let line = out
        .lines()
        .find(|l| l.starts_with("UNTIL_STOPPED_QUALIFICATION "))
        .unwrap();
    let id = line.split_whitespace().nth(1).unwrap().to_string();
    assert!(line.contains("RUN_HORIZON_UNTIL_STOPPED_GATED"), "{line}");
    assert!(ctl
        .project
        .join(format!("data/until_stopped/{id}.p0j"))
        .is_file());
    let (code, out, _) = ctl.run(&["horizon", "status", "--qualification", &id]);
    assert_eq!(code, 0);
    assert!(out.contains("sk07               PASS"), "{out}");
    // An evidence id never imported is refused before anything is stored.
    let (code, _, err) = qualify(&["--provider-evidence", "0123456789abcdef"]);
    assert_eq!(code, 2);
    assert!(err.contains("BOOT_PROVIDER_EVIDENCE_UNKNOWN"), "{err}");

    let start = |extra: &[&str]| {
        let mut args = vec![
            "run",
            "start",
            "--until-stopped",
            "--checkpoint",
            "1h",
            "--minimum-runway",
            "24h",
        ];
        args.extend(extra);
        ctl.run(&args)
    };
    let gated = |extra: &[&str], want: &str| {
        let (code, _, err) = start(extra);
        assert_eq!(code, 2);
        assert!(
            err.contains("RUN_HORIZON_UNTIL_STOPPED_GATED") && err.contains(want),
            "{want}\n{err}"
        );
    };
    gated(&[], "no qualification");
    gated(&["--qualification", &id], "runtime NOT_RUN");
    gated(&["--qualification", &id], "capacity NOT_RUN");
    // Another window is another horizon: the qualification does not bind it.
    let (code, _, err) = ctl.run(&[
        "run",
        "start",
        "--until-stopped",
        "--checkpoint",
        "2h",
        "--minimum-runway",
        "24h",
        "--qualification",
        &id,
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("drift: runtime_config,horizon"), "{err}");
    // Refused before any start step: no owner lock, no run directory.
    assert!(!ctl.project.join("data/runs").exists());
    let (code, _, err) = start(&["--duration", "1h"]);
    assert_eq!(code, 2);
    assert!(err.contains("exclusive"), "{err}");
    let (code, _, err) = ctl.run(&[
        "run",
        "start",
        "--mode",
        "shadow",
        "--until-stopped",
        "--checkpoint",
        "1h",
        "--minimum-runway",
        "24h",
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("RUN_MODE_OPTION_REFUSED"), "{err}");
}
