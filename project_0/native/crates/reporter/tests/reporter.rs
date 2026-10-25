#![cfg(unix)]
use darkbot_p0_reporter::{capture_state, generation, schema_id};
use darkbot_runner::status_tap::{MarketSnapshot, MarketStats, StatusSnapshot, StorageSample};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn projected_fields_are_optional_and_not_fabricated() {
    let empty = capture_state(&StatusSnapshot::default());
    assert!(empty.writer.is_none() && empty.market.is_none());
    let snapshot = StatusSnapshot {
        market: Some(MarketSnapshot {
            stats: MarketStats {
                gap_count: 7,
                last_observed_slot: Some(123),
                ..Default::default()
            },
            ..Default::default()
        }),
        storage: Some(StorageSample {
            at_unix_ms: 1000,
            status: darkbot_runner::storage::writer::StorageStatus {
                queue_bytes: 1024,
                lag_frames: 2,
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    let p = capture_state(&snapshot);
    let counters = p.market.unwrap().counters;
    assert!(counters
        .iter()
        .any(|c| c.name == "gap_count" && c.value == 7));
    assert!(counters
        .iter()
        .any(|c| c.name == "last_observed_slot" && c.value == 123));
    let writer = p.writer.unwrap();
    assert_eq!(writer.queue_bytes, 1024);
    assert_eq!(writer.lag_frames, 2);
    assert_eq!(schema_id().len(), 64);
}
#[test]
fn generation_advances_and_rejects_corruption() {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("darkbot-p0-generation-{}-{id}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("generation");
    assert_eq!(generation::next(&path).unwrap(), 1);
    assert_eq!(generation::next(&path).unwrap(), 2);
    std::fs::write(&path, "corrupt").unwrap();
    assert!(generation::next(&path).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn env_config_defaults_off_and_validates_enabled() {
    use darkbot_p0_reporter::config_from_env;
    use std::time::Duration;
    assert!(config_from_env("r", &|_| None).unwrap().is_none());
    let missing_generation =
        |key: &str| (key == "P0_REPORT_SOCKET").then(|| "/tmp/p0.sock".to_string());
    assert!(config_from_env("r", &missing_generation).is_err());
    let ok = |key: &str| match key {
        "P0_REPORT_SOCKET" => Some("/tmp/p0.sock".to_owned()),
        "P0_REPORT_GENERATION_FILE" => Some("/tmp/p0.gen".to_owned()),
        "P0_REPORT_HEARTBEAT_MS" => Some("1500".to_owned()),
        _ => None,
    };
    let cfg = config_from_env("r", &ok).unwrap().unwrap();
    assert_eq!(cfg.heartbeat, Duration::from_millis(1500));
    assert_eq!(cfg.run_id, "r");
    // B03/U-9: the analytics topic only on the explicit publisher variable, never from the
    // project directory a controller passes to its children.
    assert_eq!(cfg.project_dir, None);
    let with_project = |key: &str| match key {
        "P0_PROJECT_DIR" => Some("/p".to_owned()),
        other => ok(other),
    };
    assert_eq!(
        config_from_env("r", &with_project)
            .unwrap()
            .unwrap()
            .project_dir,
        None
    );
    let analytics = |key: &str| match key {
        "P0_REPORT_ANALYTICS_PROJECT_DIR" => Some("/p".to_owned()),
        other => ok(other),
    };
    assert_eq!(
        config_from_env("r", &analytics)
            .unwrap()
            .unwrap()
            .project_dir,
        Some("/p".into())
    );
}

#[test]
fn configured_reporter_rate_is_bounded_and_leaves_final_handoff_time() {
    for value in ["0", "20", "99", "1001", "10000"] {
        let lookup = |key: &str| match key {
            "P0_REPORT_SOCKET" => Some("/tmp/p0-test.sock".into()),
            "P0_REPORT_GENERATION_FILE" => Some("/tmp/p0-test.gen".into()),
            "P0_REPORT_MIN_INTERVAL_MS" => Some(value.into()),
            _ => None,
        };
        assert!(
            darkbot_p0_reporter::config_from_env("TEST_ONLY", &lookup).is_err(),
            "{value}"
        );
    }
}
