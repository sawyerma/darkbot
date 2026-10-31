//! P6-MT.10: `p0ctl fee-parity` over the real binary sums each saved run's last fee parity record
//! per class and names the classes qualified (≥ the plan's minimum matched, none mismatched) —
//! read only, offline; `--seal` seals it for an owner's ACTIVE local fee model.
use darkbot_runner::analytics::evidence::{OwnerEvidence, OwnerEvidenceWriter};
use darkbot_runner::execution::fee_parity::{
    FeeClass, FeeFormat, FeeParityStatsRecord, FeeRole, ParityCounts, FEE_MODEL_VERSION,
    PARITY_MIN_MESSAGES,
};
use std::process::Command;

fn journal(dir: &std::path::Path, records: Vec<FeeParityStatsRecord>) {
    std::fs::create_dir_all(dir).unwrap();
    let (mut w, _) = OwnerEvidenceWriter::start_rolling(
        dir.join("opportunities.p0j"),
        dir.join("recovery.p0j"),
        64,
        std::time::Duration::from_millis(5),
        1 << 20,
        false,
    )
    .unwrap();
    for r in records {
        w.emit(OwnerEvidence::FeeParityStats(Box::new(r))).unwrap();
    }
    w.finish(None).unwrap();
}

#[test]
fn p6_mt10_fee_parity_report_sums_the_saved_runs_per_class() {
    let project = std::env::temp_dir().join(format!("p0ctl_fee_parity_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&project);
    let plain = FeeClass {
        format: FeeFormat::V0,
        priority: false,
        role: FeeRole::Single,
    };
    let paid = FeeClass {
        priority: true,
        ..plain
    };
    let run = |classes: Vec<(FeeClass, ParityCounts)>| FeeParityStatsRecord {
        model_version: FEE_MODEL_VERSION,
        classes,
        first_mismatch: None,
    };
    let matched = |n| ParityCounts {
        matched: n,
        ..Default::default()
    };
    let half = PARITY_MIN_MESSAGES / 2;
    journal(
        &project.join("data/shadow/a"),
        vec![
            run(vec![(plain, matched(1))]),
            run(vec![(plain, matched(half)), (paid, matched(3))]),
        ],
    );
    journal(
        &project.join("data/shadow/b"),
        vec![run(vec![
            (plain, matched(PARITY_MIN_MESSAGES - half)),
            (
                paid,
                ParityCounts {
                    matched: PARITY_MIN_MESSAGES,
                    mismatched: 1,
                    ..Default::default()
                },
            ),
        ])],
    );
    let out = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .arg("--project")
        .arg(&project)
        .arg("fee-parity")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("2 saved runs with a record"), "{stdout}");
    assert!(
        stdout.contains(&format!(
            "V0 Single priority=false matched={PARITY_MIN_MESSAGES} repeated=0 mismatched=0 outside_model=0 QUALIFIED"
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "V0 Single priority=true matched={} repeated=0 mismatched=1 outside_model=0 FAILED",
            PARITY_MIN_MESSAGES + 3
        )),
        "one mismatch fails the class: {stdout}"
    );
    assert!(
        stdout.contains("LOCAL_FEE_MODEL_FAIL: saved runs: 1/16 classes qualified"),
        "{stdout}"
    );
    // Phase 2: `--seal` seals it — the qualified class is what an owner's ACTIVE rests on.
    use darkbot_runner::execution::fee_parity::{sealed_classes, ClassSet};
    assert!(
        sealed_classes(&project, None).is_err(),
        "nothing sealed yet"
    );
    let out = Command::new(env!("CARGO_BIN_EXE_p0ctl"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .arg("--project")
        .arg(&project)
        .args(["fee-parity", "--seal"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("as seal 1: 2 saved runs"), "{stdout}");
    assert_eq!(
        sealed_classes(&project, None),
        Ok(ClassSet::default().with(plain))
    );
    let _ = std::fs::remove_dir_all(project);
}
