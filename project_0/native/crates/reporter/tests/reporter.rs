#![cfg(unix)]
use darkbot_p0_reporter::{generation, projection, schema_id};
use darkbot_runner::status_tap::{MarketSnapshot, StatusSnapshot, StorageSnapshot};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn projected_fields_are_optional_and_not_fabricated() {
    let empty = projection("TEST_ONLY", &StatusSnapshot::default());
    assert_eq!(empty.queue_bytes, None);
    assert_eq!(empty.last_observed_slot, None);
    assert!(empty.market_verdict.is_empty());
    let snapshot = StatusSnapshot {
        market: Some(MarketSnapshot {
            gap_count: 7,
            last_observed_slot: Some(123),
            ..Default::default()
        }),
        storage: Some(StorageSnapshot {
            queue_bytes: 1024,
            lag_frames: 2,
        }),
        ..Default::default()
    };
    let p = projection("TEST_ONLY", &snapshot);
    assert_eq!(p.gap_count, Some(7));
    assert_eq!(p.last_observed_slot, Some(123));
    assert_eq!(p.queue_bytes, Some(1024));
    assert_eq!(p.lag_frames, Some(2));
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
}
