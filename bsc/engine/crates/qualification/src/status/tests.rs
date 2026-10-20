use super::{projection, wire::control::*};
use darkbot_bsc_record::{Draft, schema};
use prost::Message;
use std::{fs, path::PathBuf};
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}
#[test]
fn read_only_projection_and_golden_frames() {
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-golden-{}-{}",
        std::process::id(),
        projection::now()
    ));
    fs::create_dir_all(root.join("bsc/native/evidence/g07/fixture")).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    fs::write(root.join("COMMIT"), "fixture-source").unwrap();
    fs::write(root.join("SYNCED_AT"), "1792232000").unwrap();
    let mut d = Draft::new(&schema::g07::readiness::SCHEMA);
    d.set(schema::g07::readiness::STATUS, "BLOCKED")
        .set(schema::g07::readiness::MODE, "OFFLINE_ZERO_EGRESS")
        .set(schema::g07::readiness::OBSERVED_UNIX_MS, 1792231935242u64)
        .set(schema::g07::readiness::LIVE_AUTHORIZED, false);
    let mut c = Draft::new(&schema::g07::readiness::check::SCHEMA);
    c.set(schema::g07::readiness::check::NAME, "VALIDATION")
        .set(schema::g07::readiness::check::STATUS, "BLOCKED")
        .set(
            schema::g07::readiness::check::DETAIL,
            "api-key=NEVER_PUBLIC",
        );
    d.push(schema::g07::readiness::CHECKS, c.into_nested());
    let bytes = d.encode().unwrap();
    fs::write(
        root.join("bsc/native/evidence/g07/fixture/readiness.dbrec"),
        &bytes,
    )
    .unwrap();
    let mut s = projection::project(&root, &root.join("run"));
    s.status.evaluated_unix_s = 0;
    assert_eq!(s.status.guard.as_ref().unwrap().guard_state, "UNARMED");
    assert_eq!(s.status.readiness.as_ref().unwrap().verdict, "STOP");
    assert!(!s.launch.contract_present);
    assert!(s.wallets.launcher_balance_wei.is_none());
    assert!(!format!("{:?}", s.status).contains("NEVER_PUBLIC"));
    let dir = repo().join("protocol/golden/bsc");
    let parts = [
        ("status", TopicId::BscStatus, s.status.encode_to_vec()),
        ("launch", TopicId::BscLaunchState, s.launch.encode_to_vec()),
        (
            "monitoring",
            TopicId::BscMonitoringState,
            s.monitoring.encode_to_vec(),
        ),
        (
            "wallets",
            TopicId::BscWalletsState,
            s.wallets.encode_to_vec(),
        ),
    ];
    let mut stream = Vec::new();
    let hello = EngineFrame {
        msg: Some(engine_frame::Msg::Hello(EngineHello {
            project: ProjectId::Bsc as i32,
            schema_id: include_str!("../../../../../../protocol/schema_id.txt")
                .trim()
                .into(),
            source_id: "bsc-golden".into(),
            generation: 1,
            topics: parts.iter().map(|(_, t, _)| *t as i32).collect(),
        })),
    }
    .encode_to_vec();
    stream.extend_from_slice(&(hello.len() as u32).to_be_bytes());
    stream.extend_from_slice(&hello);
    for (name, topic, payload) in parts {
        let hex = format!("{}\n", darkbot_bsc_record::hex(&payload));
        if std::env::var_os("UPDATE_BSC_GOLDENS").is_some() {
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(format!("{name}.hex")), &hex).unwrap();
        } else {
            assert_eq!(
                fs::read_to_string(dir.join(format!("{name}.hex"))).unwrap(),
                hex
            );
        }
        let frame = EngineFrame {
            msg: Some(engine_frame::Msg::State(StateFull {
                topic: topic as i32,
                generation: 1,
                seq: 1,
                source_timestamp_unix_ns: 1792232000000000000,
                quality: StateQuality::Ok as i32,
                source_id: "bsc-golden".into(),
                payload,
            })),
        }
        .encode_to_vec();
        stream.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        stream.extend_from_slice(&frame);
    }
    if std::env::var_os("UPDATE_BSC_GOLDENS").is_some() {
        fs::write(
            dir.join("engine.stream.hex"),
            format!("{}\n", darkbot_bsc_record::hex(&stream)),
        )
        .unwrap();
        fs::write(dir.join("readiness.dbrec"), bytes).unwrap();
    }
    assert_eq!(
        fs::read_to_string(dir.join("engine.stream.hex"))
            .unwrap()
            .trim(),
        darkbot_bsc_record::hex(&stream)
    );
    // Invalid records must never become PASS. Public keys/paths cannot bypass type validation.
    fs::write(
        root.join("bsc/native/evidence/g07/fixture/readiness.dbrec"),
        b"DBREC001 corrupt",
    )
    .unwrap();
    let bad = projection::project(&root, &root.join("run"));
    assert_eq!(bad.status.readiness.unwrap().verdict, "NOT_RUN");
    assert!(
        bad.status
            .warnings
            .iter()
            .any(|w| w.starts_with("INVALID_EVIDENCE"))
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn public_text_never_exports_endpoints_or_credentials() {
    for s in [
        "Bearer SECRET",
        "private_key=SECRET",
        "https://example.invalid/SECRET",
        "authorization: SECRET",
    ] {
        assert!(!projection::safe_text(s).contains("SECRET"));
    }
}
