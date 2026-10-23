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

/// A public candidate record whose ID is computed from its public fields.
fn candidate_record(
    profile: &str,
    token: [u8; 20],
    forced: Option<[u8; 32]>,
) -> (Vec<u8>, [u8; 32]) {
    use darkbot_bsc_record::{Value, decode, schema::g07_candidate as gc};
    let c = &gc::candidate::SCHEMA;
    let draft = |id: [u8; 32]| {
        let mut d = Draft::new(c);
        d.set(gc::candidate::PROFILE, profile)
            .set(gc::candidate::CANDIDATE_ID, Value::B256(id))
            .set(gc::candidate::CHAIN_ID, 56u64)
            .set(gc::candidate::PORTAL, Value::Address([1; 20]))
            .set(gc::candidate::PORTAL_VERSION, "v5.24.0")
            .set(
                gc::candidate::PORTAL_IMPLEMENTATION,
                Value::Address([2; 20]),
            )
            .set(
                gc::candidate::PORTAL_IMPLEMENTATION_CODE_SHA256,
                Value::B256([3; 32]),
            )
            .set(
                gc::candidate::TAX_V3_IMPLEMENTATION,
                Value::Address([4; 20]),
            )
            .set(
                gc::candidate::TAX_V3_IMPLEMENTATION_CODE_SHA256,
                Value::B256([5; 32]),
            )
            .set(
                gc::candidate::DEPLOYMENT_RECORD_SHA256,
                Value::B256([6; 32]),
            )
            .set(gc::candidate::LAUNCHER, Value::Address([7; 20]))
            .set(gc::candidate::TOKEN, Value::Address(token))
            .set(gc::candidate::SALT_COMMITMENT, Value::B256([8; 32]))
            .set(gc::candidate::SALT_ORIGIN, "OS_CSPRNG")
            .set(gc::candidate::LAUNCH_CALLDATA_SHA256, Value::B256([9; 32]))
            .set(gc::candidate::QUALIFICATION, "UNQUALIFIED")
            .set(gc::candidate::PROVENANCE, "fixture")
            .set(gc::candidate::CREATED_UNIX_MS, 1u64);
        d.encode().unwrap()
    };
    let id = forced
        .unwrap_or_else(|| gc::candidate_id_of(&decode(c, &draft([0; 32])).unwrap()).unwrap());
    (draft(id), id)
}

#[test]
fn candidate_state_is_projected_only_from_a_verified_public_mirror() {
    use darkbot_bsc_record::{Value, schema::g07_candidate as gc};
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-candidate-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let run = root.join("run");
    let mirror = root.join("bsc/native/state/g07_candidates");
    fs::create_dir_all(&run).unwrap();
    let fields = |root: &PathBuf| -> Vec<(String, String)> {
        projection::project(root, &run)
            .launch
            .runtime_fields
            .into_iter()
            .map(|f| (f.name, f.value))
            .collect()
    };
    let get =
        |f: &[(String, String)], n: &str| f.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
    // No mirror: no candidate field, the GUI stays BACKEND_UNAVAILABLE.
    assert!(
        fields(&root)
            .iter()
            .all(|(k, _)| !k.starts_with("candidate."))
    );
    fs::create_dir_all(&mirror).unwrap();
    let (b, b_id) = candidate_record("SELF_B", [0x77; 20], None);
    fs::write(mirror.join("b.candidate.dbrec"), &b).unwrap();
    let mut sel = Draft::new(&gc::selection::SCHEMA);
    sel.set(gc::selection::GENERATION, 4u64)
        .set(gc::selection::PROFILE, "SELF_B")
        .set(gc::selection::CANDIDATE_ID, Value::B256(b_id))
        .set(gc::selection::ACTIVATED_UNIX_MS, 10u64)
        .set(gc::selection::INVALIDATED, "READINESS")
        .set(gc::selection::GUARD_AT_SWITCH, "UNARMED");
    fs::write(mirror.join("selection.dbrec"), sel.encode().unwrap()).unwrap();
    let f = fields(&root);
    assert_eq!(get(&f, "candidate.backend").as_deref(), Some("AVAILABLE"));
    assert_eq!(
        get(&f, "candidate.selection.generation").as_deref(),
        Some("4")
    );
    assert_eq!(
        get(&f, "candidate.selection.profile").as_deref(),
        Some("SELF_B")
    );
    assert_eq!(get(&f, "candidate.SELF_A.state").as_deref(), Some("ABSENT"));
    assert_eq!(
        get(&f, "candidate.SELF_B.qualification").as_deref(),
        Some("UNQUALIFIED")
    );
    assert_eq!(
        get(&f, "candidate.SELF_B.readiness").as_deref(),
        Some("NONE")
    );
    assert_eq!(get(&f, "candidate.request.state").as_deref(), Some("NONE"));
    // A runtime request is shown as requested, never as effective.
    let requests = run.join("bsc-operational/g07_candidates/requests");
    fs::create_dir_all(&requests).unwrap();
    let rid = "1b4e28ba-2fa1-41d2-883f-0016d3cca427";
    let mut rq = Draft::new(&gc::request::SCHEMA);
    rq.set(gc::request::REQUEST_ID, rid)
        .set(gc::request::PROFILE, "SELF_A")
        .set(gc::request::EXPECTED_GENERATION, 4u64)
        .set(gc::request::TARGET_CANDIDATE_ID, Value::B256([1; 32]))
        .set(gc::request::REQUESTED_UNIX_MS, 20u64);
    fs::write(
        requests.join(format!("{rid}.request.dbrec")),
        rq.encode().unwrap(),
    )
    .unwrap();
    let f = fields(&root);
    assert_eq!(
        get(&f, "candidate.request.state").as_deref(),
        Some("PENDING_DESKTOP_SYNC")
    );
    assert_eq!(
        get(&f, "candidate.selection.profile").as_deref(),
        Some("SELF_B")
    );
    let mut rs = Draft::new(&gc::request_result::SCHEMA);
    rs.set(gc::request_result::REQUEST_ID, rid)
        .set(gc::request_result::OUTCOME, "REJECTED")
        .set(
            gc::request_result::DETAIL,
            "candidate SELF_A does not exist",
        )
        .set(gc::request_result::DECIDED_UNIX_MS, 30u64);
    fs::write(
        requests.join(format!("{rid}.result.dbrec")),
        rs.encode().unwrap(),
    )
    .unwrap();
    let f = fields(&root);
    assert_eq!(
        get(&f, "candidate.request.state").as_deref(),
        Some("REJECTED")
    );
    assert!(
        get(&f, "candidate.request.detail")
            .unwrap()
            .contains("does not exist")
    );
    // A changed public identity or a salt record in the mirror is refused.
    // A validly encoded record whose stored ID does not match its fields.
    let (_, a_id) = candidate_record("SELF_A", [0x11; 20], None);
    let (forged, _) = candidate_record("SELF_A", [0x12; 20], Some(a_id));
    fs::write(mirror.join("a.candidate.dbrec"), &forged).unwrap();
    assert_eq!(
        get(&fields(&root), "candidate.backend").as_deref(),
        Some("INVALID")
    );
    fs::remove_file(mirror.join("a.candidate.dbrec")).unwrap();
    fs::write(mirror.join("b.salt.dbrec"), b"secret").unwrap();
    let s = projection::project(&root, &run);
    assert!(
        s.launch
            .runtime_fields
            .iter()
            .all(|f| f.name == "candidate.backend" && f.value == "INVALID")
    );
    assert!(
        s.status
            .warnings
            .iter()
            .any(|w| w.contains("CANDIDATE_MIRROR_INVALID"))
    );
    fs::remove_dir_all(root).unwrap();
}
