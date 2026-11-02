use super::{projection, publisher, wire::control::*};
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
    let readiness = s.status.readiness.as_ref().unwrap();
    assert_eq!(readiness.verdict, "BLOCKED");
    assert_eq!(readiness.mode, "OFFLINE_ZERO_EGRESS");
    assert_eq!(readiness.candidate_binding, "NO_SELECTION");
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
        ("live", TopicId::BscLiveState, s.live.encode_to_vec()),
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
        "token=SECRET",
        "access_token=SECRET",
        "secret: SECRET",
        "read /Users/someone/SECRET/state.dbrec",
        "(\"/private/tmp/SECRET\")",
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
    // Every public leaf of the mirror records, never a salt value.
    assert_eq!(
        get(&f, "candidate.SELF_B.salt_commitment").as_deref(),
        Some(format!("0x{}", "08".repeat(32)).as_str())
    );
    assert_eq!(
        get(&f, "candidate.SELF_B.created_unix_ms").as_deref(),
        Some("1")
    );
    assert_eq!(
        get(&f, "candidate.selection.guard_at_switch").as_deref(),
        Some("UNARMED")
    );
    assert!(f.iter().all(|(k, _)| !k.ends_with(".salt")));
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
    // A requalification bound to B's ID is shown with its deployment only.
    let mut q = Draft::new(&gc::qualification::SCHEMA);
    q.set(gc::qualification::CANDIDATE_ID, Value::B256(b_id))
        .set(gc::qualification::PROFILE, "SELF_B")
        .set(
            gc::qualification::REQUAL_RECORD_SHA256,
            Value::B256([1; 32]),
        )
        .set(gc::qualification::DRY_RECORD_SHA256, Value::B256([2; 32]))
        .set(
            gc::qualification::PREFLIGHT_RECORD_SHA256,
            Value::B256([3; 32]),
        )
        .set(gc::qualification::PORTAL_VERSION, "v5.26.0")
        .set(
            gc::qualification::PORTAL_IMPLEMENTATION,
            Value::Address([4; 20]),
        )
        .set(
            gc::qualification::PORTAL_IMPLEMENTATION_CODE_SHA256,
            Value::B256([5; 32]),
        )
        .set(gc::qualification::TOKEN, Value::Address([0x77; 20]))
        .set(gc::qualification::LAUNCH_TX_HASH, Value::B256([6; 32]))
        .set(gc::qualification::BUY_TX_HASH, Value::B256([7; 32]))
        .set(gc::qualification::QUALIFIED_UNIX_MS, 1u64)
        .set(gc::qualification::SCOPE, "fixture");
    fs::write(mirror.join("b.qualification.dbrec"), q.encode().unwrap()).unwrap();
    let f = fields(&root);
    assert_eq!(
        get(&f, "candidate.SELF_B.qualification").as_deref(),
        Some("QUALIFIED_FOR v5.26.0")
    );
    assert_eq!(
        f.iter()
            .filter(|(k, _)| k == "candidate.SELF_B.qualification")
            .count(),
        1
    );
    q.set(gc::qualification::CANDIDATE_ID, Value::B256([9; 32]));
    fs::write(mirror.join("b.qualification.dbrec"), q.encode().unwrap()).unwrap();
    assert_eq!(
        get(&fields(&root), "candidate.backend").as_deref(),
        Some("INVALID")
    );
    fs::remove_file(mirror.join("b.qualification.dbrec")).unwrap();
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
#[cfg(target_os = "macos")]
#[test]
fn a_completed_blocked_readiness_run_is_a_stopped_process_not_a_failure() {
    let run = std::env::temp_dir().join(format!(
        "darkbot-bsc-job-{}-{}",
        std::process::id(),
        projection::now()
    ));
    fs::create_dir_all(&run).unwrap();
    fs::write(
        run.join("bsc.readiness.launch"),
        "launch_id=job-1\nstarted_unix_s=1\n",
    )
    .unwrap();
    fs::write(
        run.join("bsc.readiness.result"),
        "launch_id=job-1\nended_unix_s=2\nresult_code=BLOCKED\nexit_code=2\n",
    )
    .unwrap();
    // No launchd service carries this label: the run has ended.
    let job = publisher::job(&run, "com.darkbot.test.absent-readiness-label");
    assert_eq!(job.state, "STOPPED");
    assert_eq!(job.result_code, "BLOCKED");
    fs::remove_dir_all(run).unwrap();
}
#[test]
fn readiness_is_current_only_for_the_selected_candidate_and_unknown_status_never_passes() {
    use darkbot_bsc_record::{Value, schema::g07::readiness as rr, schema::g07_candidate as gc};
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-binding-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let run = root.join("run");
    let mirror = root.join("bsc/native/state/g07_candidates");
    let evidence = root.join("bsc/native/evidence/g07/fixture");
    fs::create_dir_all(&run).unwrap();
    fs::create_dir_all(&mirror).unwrap();
    fs::create_dir_all(&evidence).unwrap();
    let (b, b_id) = candidate_record("SELF_B", [0x77; 20], None);
    fs::write(mirror.join("b.candidate.dbrec"), &b).unwrap();
    let mut sel = Draft::new(&gc::selection::SCHEMA);
    sel.set(gc::selection::GENERATION, 4u64)
        .set(gc::selection::PROFILE, "SELF_B")
        .set(gc::selection::CANDIDATE_ID, Value::B256(b_id))
        .set(gc::selection::ACTIVATED_UNIX_MS, 1000u64)
        .set(gc::selection::INVALIDATED, "READINESS")
        .set(gc::selection::GUARD_AT_SWITCH, "UNARMED");
    fs::write(mirror.join("selection.dbrec"), sel.encode().unwrap()).unwrap();
    let binding = gc::readiness_binding(&gc::SelectionView {
        generation: 4,
        profile: "SELF_B",
        candidate_id: b_id,
        activated_unix_ms: 1000,
    });
    let report = |status: &str, observed: u64| {
        let mut d = Draft::new(&rr::SCHEMA);
        d.set(rr::STATUS, status)
            .set(rr::MODE, "READ_ONLY_RPC")
            .set(rr::OBSERVED_UNIX_MS, observed)
            .set(rr::LIVE_AUTHORIZED, false);
        let mut c = Draft::new(&rr::check::SCHEMA);
        c.set(rr::check::NAME, "CANDIDATE_SELECTION")
            .set(rr::check::STATUS, "PASS")
            .set(rr::check::DETAIL, binding.as_str());
        d.push(rr::CHECKS, c.into_nested());
        fs::write(evidence.join("readiness.dbrec"), d.encode().unwrap()).unwrap();
        projection::project(&root, &run).status.readiness.unwrap()
    };
    // A readiness observed before the A/B switch is history, not the current candidate's.
    let r = report("PASS_READ_ONLY", 500);
    assert_eq!(
        (r.verdict.as_str(), r.candidate_binding.as_str()),
        ("PASS_READ_ONLY", "STALE")
    );
    let r = report("BLOCKED", 2000);
    assert_eq!(
        (r.verdict.as_str(), r.candidate_binding.as_str()),
        ("BLOCKED", "BOUND")
    );
    assert_eq!(r.mode, "READ_ONLY_RPC");
    // An unknown native status is never shown as a pass.
    assert_eq!(report("LIVE", 3000).verdict, "UNKNOWN");
    fs::remove_file(mirror.join("selection.dbrec")).unwrap();
    assert_eq!(report("STOP", 4000).candidate_binding, "NO_SELECTION");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_g08_record_without_its_g07_evidence_is_unbound() {
    use darkbot_bsc_record::{Value, schema::g08::unresolved as u};
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-g08-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let dir = root.join("bsc/native/evidence/g08/fixture");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    let mut d = Draft::new(&u::SCHEMA);
    d.set(u::GATE, "G0.8")
        .set(u::VERDICT, "UNRESOLVED")
        .set(u::REASON, "fixture")
        .set(u::ATTEMPT_ID, "attempt-1")
        .set(u::LAUNCH_TX, Value::B256([1; 32]))
        .set(u::BUY_TX, Value::B256([2; 32]))
        .set(u::LAUNCH_RECEIPT_RESOLVED, false)
        .set(u::BUY_RECEIPT_RESOLVED, false)
        .set(u::AUTOMATIC_RETRY, false)
        .set(u::T0_TO_BUY, "UNKNOWN");
    fs::write(dir.join("unresolved.dbrec"), d.encode().unwrap()).unwrap();
    let s = projection::project(&root, &root.join("run"));
    let g08 = s.status.gates.iter().find(|g| g.gate_id == "G0.8").unwrap();
    assert_eq!(g08.state, "UNRESOLVED");
    assert_eq!(
        g08.reason_codes,
        ["G07_EVIDENCE_UNBOUND", "CHAIN_UNKNOWN", "EVIDENCE_UNKNOWN"].map(String::from)
    );
    assert_eq!(s.monitoring.state, "UNRESOLVED");
    fs::remove_dir_all(root).unwrap();
}
/// A record with every required leaf set to a fixed fixture value.
fn filled(s: &'static darkbot_bsc_record::Schema) -> Draft {
    use darkbot_bsc_record::{Kind, Presence, Value};
    let mut d = Draft::new(s);
    for f in s.fields.iter().filter(|f| f.presence == Presence::Required) {
        let v = match f.kind {
            Kind::Bool => Value::Bool(false),
            Kind::U64 => Value::U64(1),
            Kind::I64 => Value::I64(1),
            Kind::U256 => Value::U256([0; 32]),
            Kind::Address => Value::Address([1; 20]),
            Kind::B256 => Value::B256([2; 32]),
            Kind::Bytes => Value::Bytes(Vec::new()),
            Kind::Text => Value::Text("fixture".into()),
            Kind::Nested => match f.nested {
                Some(n) => filled(n).into_nested(),
                None => continue,
            },
        };
        d.set(f.tag, v);
    }
    d
}
fn tag(s: &darkbot_bsc_record::Schema, name: &str) -> u16 {
    s.fields.iter().find(|f| f.name == name).unwrap().tag
}
#[test]
fn native_records_carry_public_leaves_with_kind_and_unit_but_never_a_salt() {
    use darkbot_bsc_record::{Value, schema::g06a};
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-records-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let dir = root.join("bsc/native/evidence/g06a/fixture");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    let mut e = filled(&g06a::economics::NATIVE);
    e.set(
        tag(&g06a::economics::NATIVE, "salt"),
        Value::B256([0xab; 32]),
    );
    fs::write(dir.join("economics.dbrec"), e.encode().unwrap()).unwrap();
    fs::write(
        dir.join("quote.dbrec"),
        filled(&g06a::quote::SCHEMA).encode().unwrap(),
    )
    .unwrap();
    let s = projection::project(&root, &root.join("run"));
    let record = |name: &str| {
        s.launch
            .records
            .iter()
            .find(|r| r.schema == name)
            .unwrap_or_else(|| panic!("{name} not projected"))
    };
    let economics = record(g06a::economics::NATIVE.name);
    assert!(economics.fields.iter().all(|f| f.name != "salt"));
    assert!(economics.fields.iter().any(|f| f.name == "salt_counter"));
    assert!(!format!("{:?}", s.launch).contains(&"ab".repeat(32)));
    let quote = record(g06a::quote::SCHEMA.name);
    assert_eq!(quote.provenance, "NATIVE_CREATED");
    let fee = quote
        .fields
        .iter()
        .find(|f| f.name == "protocol_fee_bps")
        .unwrap();
    assert_eq!(
        (fee.kind, fee.unit.as_str(), fee.value.as_str()),
        (
            super::wire::bsc::NativeKind::U64 as i32,
            "basis_points",
            "1"
        )
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn a_frame_larger_than_the_socket_buffer_arrives_whole() {
    use darkbot_bsc_record::{Value, schema::g06a::launch_path as lp};
    use std::io::Read;
    let root = std::env::temp_dir().join(format!("dbsc-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join("bsc/native/evidence/g06a/fixture");
    fs::create_dir_all(&dir).unwrap();
    let mut d = filled(&lp::SCHEMA);
    for i in 0..250u8 {
        d.push(lp::STORAGE_ADDRESS, Value::Address([i; 20]))
            .push(lp::STORAGE_SLOT, Value::B256([i; 32]))
            .push(lp::STORAGE_VALUE, Value::B256([i; 32]))
            .push(lp::STORAGE_SELECTS, Value::Address([i; 20]));
    }
    fs::write(dir.join("launch_path.dbrec"), d.encode().unwrap()).unwrap();
    let socket = root.join("s.sock");
    let (r, s) = (root.clone(), socket.clone());
    std::thread::spawn(move || {
        let _ = publisher::serve(&r, &r.join("run"), &s, "com.darkbot.test.absent", None);
    });
    let mut stream = (0..100)
        .find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            std::os::unix::net::UnixStream::connect(&socket).ok()
        })
        .unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let mut frame = || {
        let mut n = [0u8; 4];
        stream.read_exact(&mut n).unwrap();
        let mut bytes = vec![0u8; u32::from_be_bytes(n) as usize];
        stream.read_exact(&mut bytes).unwrap();
        EngineFrame::decode(bytes.as_slice()).unwrap()
    };
    frame(); // Hello
    frame(); // bsc.status
    let Some(engine_frame::Msg::State(launch)) = frame().msg else {
        panic!("launch state expected")
    };
    assert_eq!(launch.topic, TopicId::BscLaunchState as i32);
    assert!(launch.payload.len() > 32 * 1024, "{}", launch.payload.len());
    let _ = fs::remove_dir_all(root);
}
#[test]
fn every_projected_record_family_reaches_its_topic_with_all_public_required_leaves() {
    use super::census;
    use darkbot_bsc_execution::acceptance_guard::V5_RELATIVE_PATH;
    use darkbot_bsc_record::schema::{g06a, g07, g08};
    let launch: [&'static darkbot_bsc_record::Schema; 11] = [
        &g06a::dry::NATIVE,
        &g06a::preflight::SCHEMA,
        &g06a::quote::SCHEMA,
        &g06a::economics::NATIVE,
        &g06a::local_simulation::NATIVE,
        &g06a::launch_path::SCHEMA,
        &g06a::portal_upgrade::SCHEMA,
        &g06a::requalification::SCHEMA,
        &g06a::prepare_handoff::SCHEMA,
        &g07::readiness::SCHEMA,
        &g07::contract::V5,
    ];
    let monitoring: [&'static darkbot_bsc_record::Schema; 8] = [
        &g06a::builder_policy::NATIVE,
        &g07::probe::SCHEMA,
        &g07::witness_source::SCHEMA,
        &g07::evidence::SCHEMA,
        &g08::result::SCHEMA,
        &g08::chain_session::SCHEMA,
        &g08::producer_witness::SCHEMA,
        &g08::timing_verdict::SCHEMA,
    ];
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-families-{}-{}",
        std::process::id(),
        projection::now()
    ));
    fs::create_dir_all(root.join("run")).unwrap();
    for (i, s) in launch.iter().chain(monitoring.iter()).enumerate() {
        let bytes = filled(s).encode().unwrap();
        let path = if *s == &g07::contract::V5 {
            root.join(V5_RELATIVE_PATH)
        } else {
            root.join(format!("bsc/native/evidence/g07/family{i}/record.dbrec"))
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let snapshot = projection::project(&root, &root.join("run"));
    for (schemas, records) in [
        (&launch[..], &snapshot.launch.records),
        (&monitoring[..], &snapshot.monitoring.records),
    ] {
        for s in schemas {
            let record = records
                .iter()
                .find(|r| r.schema == s.name)
                .unwrap_or_else(|| panic!("{} not projected", s.name));
            let mut leaves = Vec::new();
            census::leaves(s, "", "", &mut leaves);
            let expected = leaves
                .iter()
                .filter(|(name, presence, f)| {
                    presence.split('>').all(|p| p == "Required")
                        && census::public(census::sensitivity(s, name, f.kind))
                        && !name.contains("journal_error")
                        && !name.starts_with("preflight[]")
                })
                .count();
            assert_eq!(record.fields.len(), expected, "{}", s.name);
            assert!(record.evidence.is_some(), "{}", s.name);
        }
    }
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn g08_and_g07_belong_together_only_with_the_same_attempt_and_g07_hash() {
    use darkbot_bsc_record::{Value, schema::g07, schema::g08};
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-pairing-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let g07_dir = root.join("bsc/native/evidence/g07/attempt");
    let g08_dir = root.join("bsc/native/evidence/g08/attempt");
    fs::create_dir_all(&g07_dir).unwrap();
    fs::create_dir_all(&g08_dir).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    let g07_bytes = filled(&g07::evidence::SCHEMA).encode().unwrap();
    fs::write(g07_dir.join("evidence.dbrec"), &g07_bytes).unwrap();
    let pass = |g07_sha: [u8; 32]| {
        let mut d = filled(&g08::result::SCHEMA);
        d.set(
            tag(&g08::result::SCHEMA, "g07_evidence_sha256"),
            Value::B256(g07_sha),
        )
        .set(g08::result::VERDICT, "PASS");
        d.encode().unwrap()
    };
    let g08_gate = || {
        projection::project(&root, &root.join("run"))
            .status
            .gates
            .into_iter()
            .find(|g| g.gate_id == "G0.8")
            .unwrap()
    };
    // Same attempt ID (fixture) and exactly the SHA-256 of the G0.7 record.
    // (The verdict dimensions follow the binding code; BSC-011 tests them.)
    let bound = pass(darkbot_bsc_record::sha256(&g07_bytes));
    fs::write(g08_dir.join("result.dbrec"), &bound).unwrap();
    let g = g08_gate();
    assert_eq!(
        (g.state.as_str(), g.reason_codes[0].as_str()),
        ("PASS", "G07_EVIDENCE_BOUND")
    );
    // Review MEDIUM-2: an unbound PASS is UNRESOLVED, the record's verdict a reason.
    fs::write(g08_dir.join("result.dbrec"), pass([0x55; 32])).unwrap();
    let g = g08_gate();
    assert_eq!(g.state, "UNRESOLVED");
    assert_eq!(g.reason_codes[0], "G07_EVIDENCE_UNBOUND");
    assert!(
        g.reason_codes
            .contains(&"RECORDED_VERDICT_PASS".to_string()),
        "{:?}",
        g.reason_codes
    );
    // Two G0.8 records (no time orders them): never the PASS of whichever path
    // sorts last, even when that one is bound.
    fs::write(g08_dir.join("result.dbrec"), &bound).unwrap();
    let other = root.join("bsc/native/evidence/g08/attempt-10");
    fs::create_dir_all(&other).unwrap();
    let mut u = filled(&g08::unresolved::SCHEMA);
    u.set(g08::unresolved::VERDICT, "UNRESOLVED");
    fs::write(other.join("unresolved.dbrec"), u.encode().unwrap()).unwrap();
    let g = g08_gate();
    assert_eq!(g.state, "UNRESOLVED", "{:?}", g.reason_codes);
    assert!(
        g.reason_codes
            .contains(&"G08_RECORDS_AMBIGUOUS".to_string())
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn the_reporter_refuses_a_socket_directory_another_user_could_write_or_redirect() {
    use std::os::unix::fs::PermissionsExt;
    let base = std::env::temp_dir().join(format!("dbsc-trust-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let open = base.join("open");
    fs::create_dir_all(&open).unwrap();
    fs::set_permissions(&open, fs::Permissions::from_mode(0o777)).unwrap();
    let target = base.join("target");
    fs::create_dir_all(&target).unwrap();
    std::os::unix::fs::symlink(&target, base.join("link")).unwrap();
    for dir in [open.clone(), base.join("link")] {
        let (tx, rx) = std::sync::mpsc::channel();
        let (run, socket) = (base.join("run"), dir.join("s.sock"));
        std::thread::spawn(move || {
            let _ = tx.send(publisher::serve(
                &run,
                &run,
                &socket,
                "com.darkbot.test.absent",
                None,
            ));
        });
        let result = rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap_or_else(|_| panic!("reporter served from untrusted {}", dir.display()));
        assert!(result.is_err(), "{}", dir.display());
        assert!(!dir.join("s.sock").exists(), "{}", dir.display());
    }
    fs::remove_dir_all(base).unwrap();
}
#[test]
fn an_oversized_topic_is_declared_too_large_and_the_next_topic_still_arrives() {
    use super::wire::bsc::{BscLaunchState, BscStatus};
    use std::io::Read;
    let (mut reporter, mut gateway) = std::os::unix::net::UnixStream::pair().unwrap();
    let big = BscLaunchState {
        reason: "x".repeat(300_000),
        ..Default::default()
    };
    let small = BscStatus {
        source_commit: "fixture-source".into(),
        ..Default::default()
    };
    publisher::state(&mut reporter, TopicId::BscLaunchState, 9, 4, &big).unwrap();
    publisher::state(&mut reporter, TopicId::BscStatus, 9, 4, &small).unwrap();
    let mut frame = || {
        let mut n = [0u8; 4];
        gateway.read_exact(&mut n).unwrap();
        let mut b = vec![0; u32::from_be_bytes(n) as usize];
        gateway.read_exact(&mut b).unwrap();
        assert!(b.len() <= 262_144 + 4096);
        EngineFrame::decode(b.as_slice()).unwrap().msg.unwrap()
    };
    let engine_frame::Msg::StateTooLarge(t) = frame() else {
        panic!("too-large notice expected")
    };
    assert_eq!(
        (t.topic, t.generation, t.seq),
        (TopicId::BscLaunchState as i32, 9, 4)
    );
    assert_eq!(t.payload_bytes as usize, big.encoded_len());
    let engine_frame::Msg::State(s) = frame() else {
        panic!("next topic expected")
    };
    assert_eq!((s.topic, s.seq), (TopicId::BscStatus as i32, 4));
    assert_eq!(BscStatus::decode(s.payload.as_slice()).unwrap(), small);
}

/// BSC-048: the builder probe and refresh come from native records only; the
/// legacy 2026-10-16 probe/refresh JSON is historical evidence, never status
/// input.
#[test]
fn legacy_builder_json_is_never_status_input_and_the_probe_is_native() {
    use darkbot_bsc_record::schema::g07::probe as p;
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-probe-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let g06a = root.join("bsc/native/evidence/g06a");
    fs::create_dir_all(g06a.join("builder_probe_legacy")).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    fs::write(
        g06a.join("builder_probe_legacy/probe.evidence.json"),
        r#"{"cases":[{"answer_unix_ms":4102444800000}]}"#,
    )
    .unwrap();
    fs::write(
        g06a.join("builder_requirements_refresh_20991231T000000Z.json"),
        r#"{"schema":"darkbot.g07.builder-requirements-refresh.v1","fetched_at_utc":"2099-12-31T00:00:00Z"}"#,
    )
    .unwrap();
    let s = projection::project(&root, &root.join("run"));
    let builder = s.status.builder.clone().unwrap();
    assert!(builder.probe.is_none(), "{:?}", builder.probe);
    assert_eq!(builder.refresh_observed_unix_s, 0);
    // A native probe record is the probe, timed by its latest answer.
    let mut case = filled(&p::case::SCHEMA);
    case.set(p::case::ANSWER_MS, 1_800_000_123_456u64);
    let mut d = filled(&p::SCHEMA);
    d.push(p::CASES, case.into_nested());
    let dir = g06a.join("builder_probe_native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("probe.evidence.dbrec"), d.encode().unwrap()).unwrap();
    let s = projection::project(&root, &root.join("run"));
    let probe = s.status.builder.unwrap().probe.unwrap();
    assert_eq!(probe.observed_unix_s, 1_800_000_123);
    assert!(
        probe
            .path
            .ends_with("builder_probe_native/probe.evidence.dbrec"),
        "{}",
        probe.path
    );
    fs::remove_dir_all(root).unwrap();
}

/// BSC-011: the G0.8 gate shows the chain outcome and the evidence
/// integrity next to the overall verdict; unknown values stay UNKNOWN.
#[test]
fn the_g08_gate_carries_the_chain_outcome_and_the_evidence_integrity() {
    use darkbot_bsc_record::schema::g08::result as res;
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-g08-dims-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let dir = root.join("bsc/native/evidence/g08/fixture");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    let write = |chain: &str, integrity: &str| {
        let mut d = filled(&res::SCHEMA);
        d.set(res::VERDICT, "UNRESOLVED")
            .set(res::CHAIN_OUTCOME, chain)
            .set(res::EVIDENCE_INTEGRITY, integrity);
        fs::write(dir.join("result.dbrec"), d.encode().unwrap()).unwrap();
        let s = projection::project(&root, &root.join("run"));
        s.status
            .gates
            .into_iter()
            .find(|g| g.gate_id == "G0.8")
            .unwrap()
    };
    let g08 = write("STOP", "LOCAL_CAPTURE_INCOMPLETE");
    assert_eq!(g08.state, "UNRESOLVED");
    assert!(
        g08.reason_codes.contains(&"CHAIN_STOP".to_string()),
        "{:?}",
        g08.reason_codes
    );
    assert!(
        g08.reason_codes
            .contains(&"EVIDENCE_LOCAL_CAPTURE_INCOMPLETE".to_string())
    );
    let g08 = write("MAYBE", "FINE");
    assert!(
        g08.reason_codes.contains(&"CHAIN_UNKNOWN".to_string()),
        "{:?}",
        g08.reason_codes
    );
    assert!(g08.reason_codes.contains(&"EVIDENCE_UNKNOWN".to_string()));
    // V2 §2.3: the record of missing canonical receipts is UNRESOLVED with
    // both dimensions UNKNOWN; nothing reads as acquired, sent or retryable.
    use darkbot_bsc_record::schema::g08::unresolved as un;
    fs::remove_file(dir.join("result.dbrec")).unwrap();
    let mut d = filled(&un::SCHEMA);
    d.set(un::VERDICT, "UNRESOLVED")
        .set(un::REASON, "SENT_TRANSACTION_UNRESOLVED")
        .set(un::LAUNCH_RECEIPT_RESOLVED, false)
        .set(un::BUY_RECEIPT_RESOLVED, false)
        .set(un::AUTOMATIC_RETRY, false);
    fs::write(dir.join("unresolved.dbrec"), d.encode().unwrap()).unwrap();
    let s = projection::project(&root, &root.join("run"));
    let g08 = s.status.gates.iter().find(|g| g.gate_id == "G0.8").unwrap();
    assert_eq!(g08.state, "UNRESOLVED");
    for code in ["G07_EVIDENCE_UNBOUND", "CHAIN_UNKNOWN", "EVIDENCE_UNKNOWN"] {
        assert!(
            g08.reason_codes.contains(&code.to_string()),
            "{:?}",
            g08.reason_codes
        );
    }
    assert!(
        g08.reason_codes
            .iter()
            .all(|c| !c.contains("PASS") && !c.contains("ADAPT"))
    );
    assert_eq!(s.monitoring.state, "UNRESOLVED");
    fs::remove_dir_all(root).unwrap();
}
/// BSC-050: a clock fault never makes the builder refresh look fresh.
#[test]
fn without_a_clock_the_builder_refresh_counts_as_expired() {
    assert!(projection::refresh_expired(u64::MAX, None));
    assert!(projection::refresh_expired(10, Some(11)));
    assert!(!projection::refresh_expired(12, Some(11)));
}
/// Order V2 §5: the source owner's current generation reaches the
/// monitoring topic as its native record; a generation the owner's rule
/// refuses is a warning, never a record.
#[test]
fn the_source_modes_reach_monitoring_from_their_native_owner() {
    use darkbot_bsc_execution::sources::{Mode, SourceId, Store};
    let root = std::env::temp_dir().join(format!(
        "darkbot-bsc-sources-{}-{}",
        std::process::id(),
        projection::now()
    ));
    let run = root.join("run");
    fs::create_dir_all(&run).unwrap();
    let names = |s: &projection::Snapshot| {
        s.monitoring
            .records
            .iter()
            .map(|r| r.schema.clone())
            .collect::<Vec<_>>()
    };
    let snap = projection::project(&root, &run);
    assert!(!names(&snap).contains(&"darkbot.bsc.source-modes.v1".to_string()));
    let store = Store::open(run.join("bsc-sources"), 8).unwrap();
    store.init(1).unwrap();
    store
        .request("r1", SourceId::PublicNode, Mode::Live, 0, 2)
        .unwrap();
    let snap = projection::project(&root, &run);
    let record = snap
        .monitoring
        .records
        .iter()
        .find(|r| r.schema == "darkbot.bsc.source-modes.v1")
        .expect("source modes record");
    let field = |name: &str| {
        record
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.value.clone())
    };
    assert_eq!(field("generation").as_deref(), Some("1"));
    assert_eq!(
        field("sources[1].source_id").as_deref(),
        Some("BSC_INGRESS_PUBLICNODE")
    );
    assert_eq!(field("sources[1].requested").as_deref(), Some("LIVE"));
    assert_eq!(field("sources[1].effective").as_deref(), Some("BLOCKED"));
    // The first missing condition: the global economic LIVE switch.
    assert_eq!(
        field("sources[1].block_reason").as_deref(),
        Some("ECONOMIC_LIVE_DISABLED")
    );
    // The running runtime's status record reaches monitoring as well.
    {
        use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
        let mut d = Draft::new(&rs::SCHEMA);
        d.set(rs::GENERATION, 1u64)
            .set(rs::PROVENANCE_TOOL, "bsc_runtime")
            .set(rs::PROVENANCE_OBSERVED_UNIX_MS, 5u64)
            .set(rs::STARTED_UNIX_MS, 4u64)
            .set(rs::LAUNCHES, 0u64)
            .set(rs::JOURNAL_PATH, "bsc-runtime/journal/4.dbjnl")
            .set(rs::JOURNAL_RECORDS, 0u64)
            .set(rs::JOURNAL_LOST, 0u64)
            .set(rs::MODE, "READ_ONLY")
            .set(rs::STATUS_INTERVAL_MS, 1_000u64);
        fs::create_dir_all(run.join("bsc-runtime")).unwrap();
        fs::write(run.join("bsc-runtime/status.dbrec"), d.encode().unwrap()).unwrap();
        let snap = projection::project(&root, &run);
        assert!(names(&snap).contains(&"darkbot.bsc.runtime-status.v1".to_string()));
        fs::write(run.join("bsc-runtime/status.dbrec"), b"DBREC001 broken").unwrap();
        let snap = projection::project(&root, &run);
        assert!(!names(&snap).contains(&"darkbot.bsc.runtime-status.v1".to_string()));
        assert!(
            snap.status
                .warnings
                .contains(&"RUNTIME_STATUS_INVALID".to_string())
        );
    }
    // Phase F: the capacity preflight and the newest replay proof reach
    // monitoring; a broken one is a warning, never a record.
    {
        use darkbot_bsc_record::schema::bsc_capture::{
            capture_preflight as cp, replay_proof as rp,
        };
        let proof = |run_id: &str| {
            let mut d = Draft::new(&rp::SCHEMA);
            d.set(rp::RUN_ID, run_id)
                .set(rp::CHAIN_ID, 56u64)
                .set(rp::PROVENANCE_TOOL, "bsc_runtime replay-proof")
                .set(rp::PROVENANCE_OBSERVED_UNIX_MS, 5u64)
                .set(rp::SEGMENTS, 1u64)
                .set(rp::RECORDS, 3u64)
                .set(rp::MANIFEST_MATCH, true)
                .set(rp::FLAP_SIGHTINGS, 1u64)
                .set(rp::FOUR_SIGHTINGS, 0u64)
                .set(rp::OPENFOUR_SIGHTINGS, 0u64)
                .set(rp::UNDECODABLE_SIGHTINGS, 0u64)
                .set(rp::PENDING_SIGHTINGS, 0u64)
                .set(rp::PENDING_TO_VENUES, 0u64)
                .set(rp::CENSUS_STATE, "COMPLETE")
                .set(rp::CENSUS_DETERMINISTIC, true)
                .set(rp::VERDICT, "PASS");
            d.encode().unwrap()
        };
        fs::write(run.join("bsc-runtime/replay-proof-5.dbrec"), proof("5")).unwrap();
        fs::write(run.join("bsc-runtime/replay-proof-9.dbrec"), proof("9")).unwrap();
        let mut d = Draft::new(&cp::SCHEMA);
        d.set(cp::DURATION_S, 1_800u64)
            .set(cp::CHAIN_ID, 56u64)
            .set(cp::PROVENANCE_TOOL, "bsc_runtime capture-preflight")
            .set(cp::PROVENANCE_OBSERVED_UNIX_MS, 5u64)
            .set(cp::AVERAGE_BYTES_PER_S, 0u64)
            .set(cp::PEAK_BYTES_PER_S, 0u64)
            .set(cp::BURST_S, 0u64)
            .set(cp::OVERHEAD_PPM, 0u64)
            .set(cp::PROBE_BYTES, 0u64)
            .set(cp::WRITER_BYTES_PER_S, 0u64)
            .set(cp::FREE_DISK_BYTES, 1u64)
            .set(cp::DISK_RESERVE_BYTES, 0u64)
            .set(cp::MEMORY_BUDGET_BYTES, 1u64)
            .set(cp::VERDICT, "BLOCKED")
            .set(cp::BLOCKING_RESOURCE, "UNMEASURED")
            .set(cp::ACHIEVABLE_S, 0u64);
        fs::write(
            run.join("bsc-runtime/capture-preflight.dbrec"),
            d.encode().unwrap(),
        )
        .unwrap();
        {
            use darkbot_bsc_record::schema::bsc_capture::capture_quality as cq;
            let mut q = Draft::new(&cq::SCHEMA);
            q.set(cq::RUN_ID, "9")
                .set(cq::CHAIN_ID, 56u64)
                .set(cq::PROVENANCE_TOOL, "bsc_runtime capture-quality")
                .set(cq::PROVENANCE_OBSERVED_UNIX_MS, 5u64)
                .set(cq::VERDICT, "CLEAN")
                .set(cq::PHASE_F_ACCEPTANCE, "NOT_MET")
                .set(cq::DURATION_S, 300u64)
                .set(cq::TARGET_S, 1_800u64)
                .set(cq::NATIVE_SOURCE, false)
                .set(cq::CENSUS_STATE, "COMPLETE")
                .set(cq::PROOF_VERDICT, "PASS")
                .set(cq::CLOCK_SUSPECT, false);
            for tag in [
                cq::LOST_RECORDS,
                cq::INGRESS_DROPPED,
                cq::WRITE_ERRORS,
                cq::REORGS,
                cq::RESYNCS,
                cq::BLOCKS_SKIPPED,
                cq::UNJOINED,
                cq::UNPARSED_RECORDS,
                cq::UNDECODABLE,
                cq::FOUR_MISMATCH,
                cq::FOUR_NO_DERIVATION,
            ] {
                q.set(tag, 0u64);
            }
            fs::write(
                run.join("bsc-runtime/capture-quality-9.dbrec"),
                q.encode().unwrap(),
            )
            .unwrap();
        }
        let snap = projection::project(&root, &run);
        let all = names(&snap);
        assert!(all.contains(&"darkbot.bsc.capture-preflight.v1".to_string()));
        assert!(all.contains(&"darkbot.bsc.capture-quality.v1".to_string()));
        let proofs: Vec<_> = snap
            .monitoring
            .records
            .iter()
            .filter(|r| r.schema == "darkbot.bsc.replay-proof.v1")
            .collect();
        assert_eq!(proofs.len(), 1);
        assert!(
            proofs[0]
                .fields
                .iter()
                .any(|f| f.name == "run_id" && f.value == "9")
        );
        fs::write(
            run.join("bsc-runtime/replay-proof-9.dbrec"),
            b"DBREC001 broken",
        )
        .unwrap();
        fs::write(
            run.join("bsc-runtime/capture-preflight.dbrec"),
            b"DBREC001 broken",
        )
        .unwrap();
        fs::write(
            run.join("bsc-runtime/capture-quality-9.dbrec"),
            b"DBREC001 broken",
        )
        .unwrap();
        let snap = projection::project(&root, &run);
        assert!(!names(&snap).contains(&"darkbot.bsc.capture-preflight.v1".to_string()));
        for w in [
            "REPLAY_PROOF_INVALID",
            "CAPTURE_PREFLIGHT_INVALID",
            "CAPTURE_QUALITY_INVALID",
        ] {
            assert!(snap.status.warnings.contains(&w.to_string()), "{w}");
        }
        fs::remove_file(run.join("bsc-runtime/replay-proof-5.dbrec")).unwrap();
        fs::remove_file(run.join("bsc-runtime/replay-proof-9.dbrec")).unwrap();
        fs::remove_file(run.join("bsc-runtime/capture-preflight.dbrec")).unwrap();
        fs::remove_file(run.join("bsc-runtime/capture-quality-9.dbrec")).unwrap();
    }
    // A tampered generation: no record, a warning.
    let generations = run.join("bsc-sources/generations");
    let newest = fs::read_dir(&generations)
        .unwrap()
        .map(|e| e.unwrap().path())
        .max()
        .unwrap();
    fs::write(&newest, b"DBREC001 broken").unwrap();
    let snap = projection::project(&root, &run);
    assert!(!names(&snap).contains(&"darkbot.bsc.source-modes.v1".to_string()));
    assert!(
        snap.status
            .warnings
            .contains(&"SOURCE_MODES_INVALID".to_string())
    );
    fs::remove_dir_all(root).unwrap();
}
/// BSC-023: the latest C1.9 report and its host profile and integrity
/// summary reach the monitoring topic from the committed binary evidence;
/// the older run and the legacy JSON files beside them are not projected.
#[test]
fn the_latest_c19_evidence_is_projected_from_binary_records() {
    let root = std::env::temp_dir().join(format!("darkbot-bsc-c19-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let c1 = root.join("bsc/native/evidence/c1");
    fs::create_dir_all(&c1).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    let source = repo().join("bsc/native/evidence/c1");
    let mut copied = 0;
    for e in fs::read_dir(&source).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with("c1-9-native-latency-20261025T2341")
            || name.ends_with("20261010T230358.json")
        {
            fs::copy(e.path(), c1.join(&name)).unwrap();
            copied += 1;
        }
    }
    assert!(copied >= 9, "the committed C1.9 evidence is present");
    let s = projection::project(&root, &root.join("run"));
    assert!(
        !s.status.warnings.iter().any(|w| w.contains("evidence/c1")),
        "{:?}",
        s.status.warnings
    );
    let path_of = |name: &str| {
        s.monitoring
            .records
            .iter()
            .find(|r| r.schema == name)
            .and_then(|r| r.evidence.as_ref())
            .map(|e| e.path.clone())
            .unwrap_or_default()
    };
    assert!(
        path_of("darkbot.c1.native-latency.v1")
            .ends_with("c1-9-native-latency-20261025T234144.dbrec")
    );
    assert!(path_of("darkbot.c1.host-profile.v1").ends_with("20261025T234144-host_profile.dbrec"));
    assert!(
        path_of("darkbot.c1.c19-rust-summary.v1").ends_with("20261025T234144-rust_summary.dbrec")
    );
    assert_eq!(
        path_of("darkbot.c1.hook-path.v1"),
        "",
        "no C1.9b run exists"
    );
    fs::remove_dir_all(&root).unwrap();
}
/// C1.9 and C1.10 come from the committed C1.10 freeze record (status,
/// accepted C1.9 gate, non-claims), not from constants; without a record
/// both are UNKNOWN.
#[test]
fn c1_gates_are_projected_from_the_freeze_record() {
    let root = std::env::temp_dir().join(format!("darkbot-bsc-c1gates-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("run")).unwrap();
    let gate = |s: &projection::Snapshot, id: &str| {
        let g = s.status.gates.iter().find(|g| g.gate_id == id).unwrap();
        (
            g.state.clone(),
            g.reason_codes.clone(),
            g.evidence.is_some(),
        )
    };
    let none = projection::project(&root, &root.join("run"));
    for id in ["C1.9", "C1.10"] {
        assert_eq!(
            gate(&none, id),
            ("UNKNOWN".into(), vec!["NO_C1_FREEZE_RECORD".into()], false)
        );
    }
    let freeze = root.join("bsc/native/evidence/c1/c1-10-freeze-818525b1");
    fs::create_dir_all(&freeze).unwrap();
    fs::copy(
        repo().join("bsc/native/evidence/c1/c1-10-freeze-818525b1/c1-10-freeze-record.dbrec"),
        freeze.join("c1-10-freeze-record.dbrec"),
    )
    .unwrap();
    let s = projection::project(&root, &root.join("run"));
    let (state, reasons, evidence) = gate(&s, "C1.10");
    assert_eq!(state, "IMPLEMENTED_TESTED/FROZEN");
    assert!(evidence);
    assert!(reasons.contains(&"T0_TO_T_BUY_LT_1MS = NOT_PROVEN".to_string()));
    let (state, reasons, _) = gate(&s, "C1.9");
    assert_eq!(state, "PASS");
    assert!(reasons.contains(&"LIVE = BLOCKED".to_string()));
    fs::remove_dir_all(&root).unwrap();
}
/// G0.4: CRITERIA_UNCONFIRMED from the committed proposal until a verdict
/// record exists; then the verdict and its reasons.
#[test]
fn the_g04_gate_shows_the_criteria_state_or_the_verdict() {
    let root = std::env::temp_dir().join(format!("darkbot-bsc-g04gate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run = root.join("run");
    fs::create_dir_all(run.join("bsc-runtime")).unwrap();
    let gate = |s: &projection::Snapshot| {
        let g = s.status.gates.iter().find(|g| g.gate_id == "G0.4").unwrap();
        (g.state.clone(), g.reason_codes.clone())
    };
    assert_eq!(
        gate(&projection::project(&root, &run)),
        ("UNKNOWN".into(), vec!["NO_G04_CRITERIA".into()])
    );
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::copy(
        repo().join("bsc/config/g04_criteria.env"),
        root.join("bsc/config/g04_criteria.env"),
    )
    .unwrap();
    assert_eq!(
        gate(&projection::project(&root, &run)),
        (
            "CRITERIA_UNCONFIRMED".into(),
            vec!["CRITERIA_PROPOSED".into()]
        )
    );
    use darkbot_bsc_execution::g04_criteria::{Criteria, evaluate};
    let text = fs::read_to_string(root.join("bsc/config/g04_criteria.env"))
        .unwrap()
        .replace("=PROPOSED", "=CONFIRMED");
    let verdict = evaluate(
        &Criteria::parse(text.as_bytes()).unwrap(),
        None,
        &darkbot_bsc_execution::g04_census::Census::default(),
        1,
    );
    fs::write(
        run.join("bsc-runtime/g04-verdict.dbrec"),
        verdict.encode(1).unwrap(),
    )
    .unwrap();
    assert_eq!(
        gate(&projection::project(&root, &run)),
        (
            "NOT_EVALUABLE".into(),
            vec![
                "CRITERIA_NOT_PRECOMMITTED".into(),
                "CAMPAIGN_RUNNING".into(),
                "COVERAGE_BELOW_MINIMUM".into()
            ]
        )
    );
    // BSC3-033 review M1: a rating that names no pre-commitment (an older
    // build's v2, or v1) binds nothing; one that names it is shown.
    use darkbot_bsc_execution::g04_criteria::{CONTINUE, Precommit};
    let mut rated = verdict.clone();
    rated.verdict = CONTINUE;
    rated.reasons.clear();
    let mut v1 = darkbot_bsc_record::Draft::new(&schema::bsc_census::verdict::V1);
    {
        use schema::bsc_census::verdict as gv;
        v1.set(
            gv::CRITERIA_SHA256,
            darkbot_bsc_record::Value::B256([0; 32]),
        )
        .set(gv::CRITERIA_STATE, "CONFIRMED")
        .set(gv::PROVENANCE_TOOL, "bsc_runtime g04-verdict")
        .set(gv::PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(gv::VERDICT, CONTINUE)
        .set(gv::CAMPAIGN_MS, 1u64)
        .set(gv::COVERED_MS, 1u64);
    }
    for bytes in [rated.encode(1).unwrap(), v1.encode().unwrap()] {
        fs::write(run.join("bsc-runtime/g04-verdict.dbrec"), bytes).unwrap();
        assert_eq!(
            gate(&projection::project(&root, &run)),
            (
                "NOT_EVALUABLE".into(),
                vec!["CRITERIA_NOT_PRECOMMITTED".into()]
            )
        );
    }
    rated.precommit = Some(Precommit {
        criteria_sha256: rated.criteria_sha256,
        decided_by: "operator".into(),
        committed_unix_ms: 0,
        sha256: [1; 32],
    });
    fs::write(
        run.join("bsc-runtime/g04-verdict.dbrec"),
        rated.encode(1).unwrap(),
    )
    .unwrap();
    assert_eq!(
        gate(&projection::project(&root, &run)),
        ("CONTINUE".into(), Vec::<String>::new())
    );
    fs::remove_dir_all(&root).unwrap();
}

/// Plan 27.10.: the two wallet switches reach Wallets and the five economic
/// ones Launch / BUY, typed: requested, effective in this boot session,
/// block reason, generation (CAS) and the request that set them. Absent
/// before the owner's state exists; BLOCKED when it does not decode; an
/// ENABLED switch of an earlier boot reads BLOCKED RESET_BY_RESTART.
#[test]
fn the_wallet_and_economic_switches_are_projected_typed() {
    use darkbot_bsc_execution::sources::{Mode, Store, SwitchId, boot_session};
    let root = std::env::temp_dir().join(format!("darkbot-bsc-switches-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run = root.join("run");
    fs::create_dir_all(&run).unwrap();
    let s = projection::project(&root, &run);
    assert_eq!(s.wallets.launcher_control, None);
    assert!(s.launch.economic_controls.is_empty());
    let store = Store::open(run.join("bsc-sources"), 8).unwrap();
    store.restart(1).unwrap();
    let s = projection::project(&root, &run);
    let launcher = s.wallets.launcher_control.clone().unwrap();
    assert_eq!(
        (
            launcher.id.as_str(),
            launcher.requested.as_str(),
            launcher.effective.as_str(),
            launcher.generation
        ),
        ("BSC_WALLET_LAUNCHER", "DISABLED", "DISABLED", 0)
    );
    assert_eq!(
        s.launch
            .economic_controls
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        [
            "BSC_ECON_SIGN",
            "BSC_ECON_ARM",
            "BSC_ECON_SEND",
            "BSC_ECON_ATTEMPT",
            "BSC_ECON_LIVE"
        ]
    );
    store
        .request("e2e-buyer", SwitchId::WalletBuyer, Mode::Enabled, 0, 2)
        .unwrap();
    let s = projection::project(&root, &run);
    let buyer = s.wallets.buyer_control.clone().unwrap();
    assert_eq!(
        (
            buyer.effective.as_str(),
            buyer.request_id.as_str(),
            buyer.set_generation,
            buyer.generation
        ),
        ("ENABLED", "e2e-buyer", Some(1), 1)
    );
    assert_eq!(
        s.wallets.launcher_control.unwrap().effective,
        "DISABLED",
        "independent"
    );
    // The same state in another boot session: BLOCKED, never ENABLED.
    let other = Store::open_in_session(
        run.join("bsc-sources"),
        8,
        Ok(format!("{}-other", boot_session().unwrap())),
    )
    .unwrap();
    other
        .request("e2e-other", SwitchId::Send, Mode::Enabled, 1, 3)
        .unwrap();
    let s = projection::project(&root, &run);
    let send = s
        .launch
        .economic_controls
        .iter()
        .find(|c| c.id == "BSC_ECON_SEND")
        .unwrap();
    assert_eq!(
        (send.effective.as_str(), send.block_reason.as_str()),
        ("BLOCKED", "RESET_BY_RESTART")
    );
    assert_eq!(s.wallets.buyer_control.unwrap().effective, "DISABLED");
    assert!(
        s.status
            .warnings
            .contains(&"CONTROL_SWITCHES_RESET_BY_RESTART".to_string())
    );
    // The raw switch and setting leaves never travel (typed only).
    let raw = s
        .monitoring
        .records
        .iter()
        .find(|r| r.schema == "darkbot.bsc.source-modes.v1")
        .unwrap();
    assert!(raw.fields.iter().any(|f| f.name.starts_with("sources[")));
    assert!(
        !raw.fields
            .iter()
            .any(|f| f.name.starts_with("switches[") || f.name.starts_with("settings["))
    );
    // A state that does not decode: every switch BLOCKED, never ENABLED.
    fs::write(
        run.join(format!("bsc-sources/generations/{:020}.dbrec", 3)),
        b"torn",
    )
    .unwrap();
    let s = projection::project(&root, &run);
    let launcher = s.wallets.launcher_control.unwrap();
    assert_eq!(
        (launcher.effective.as_str(), launcher.block_reason.as_str()),
        ("BLOCKED", "CONTROL_STATE_INVALID")
    );
    fs::remove_dir_all(&root).unwrap();
}

/// Plan v9 §2.5 and 27.10.: the runtime mode and the daily BUY limit reach
/// Launch / BUY typed, with today's (UTC) use of the limit.
#[test]
fn the_runtime_mode_and_the_daily_buy_limit_are_projected_typed() {
    use darkbot_bsc_execution::sources::{Entity, Mode, Store};
    let root = std::env::temp_dir().join(format!("darkbot-bsc-settings-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run = root.join("run");
    fs::create_dir_all(&run).unwrap();
    let s = projection::project(&root, &run);
    assert_eq!(
        (
            s.launch.runtime_mode.clone(),
            s.launch.buy_attempt_limit.clone()
        ),
        (None, None)
    );
    let store = Store::open(run.join("bsc-sources"), 8).unwrap();
    store.restart(1).unwrap();
    let s = projection::project(&root, &run);
    let mode = s.launch.runtime_mode.unwrap();
    assert_eq!(
        (
            mode.id.as_str(),
            mode.requested.as_str(),
            mode.effective.as_str()
        ),
        ("BSC_RUNTIME_MODE", "OFF", "OFF")
    );
    let limit = s.launch.buy_attempt_limit.unwrap();
    assert_eq!(
        (limit.per_day, limit.used, limit.remaining, limit.generation),
        (0, 0, 0, 0)
    );
    store
        .request("rt", Entity::Runtime, Mode::Paper, 0, 2)
        .unwrap();
    store
        .request("bl", Entity::BuyLimit, Mode::PerDay(3), 1, 3)
        .unwrap();
    // Plan 28.10.: the campaign's total leaves room for it.
    store
        .request("bt", Entity::BuyTotal, Mode::PerDay(3), 2, 3)
        .unwrap();
    store
        .consume_buy_attempt("attempt-1", projection::now() * 1_000)
        .unwrap();
    let s = projection::project(&root, &run);
    let mode = s.launch.runtime_mode.unwrap();
    assert_eq!(
        (
            mode.effective.as_str(),
            mode.request_id.as_str(),
            mode.set_generation
        ),
        ("PAPER", "rt", Some(1))
    );
    let limit = s.launch.buy_attempt_limit.unwrap();
    assert_eq!(
        (
            limit.per_day,
            limit.used,
            limit.remaining,
            limit.generation,
            limit.request_id.as_str()
        ),
        (3, 1, 2, 3, "bl")
    );
    assert_eq!(limit.day_utc.len(), 10);
    fs::remove_dir_all(&root).unwrap();
}

/// Review L5: a LIVE requested in an earlier boot session with the LIVE
/// switch ENABLED reads ECONOMIC_LIVE_DISABLED now, typed and in the raw
/// source-modes leaves, as the reset LIVE switch itself does.
#[test]
fn a_live_reason_of_an_earlier_boot_is_projected_as_it_holds_now() {
    use darkbot_bsc_execution::sources::{Entity, Mode, Store, SwitchId, VenueId};
    let root = std::env::temp_dir().join(format!("darkbot-bsc-live-boot-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let run = root.join("run");
    fs::create_dir_all(&run).unwrap();
    let earlier =
        Store::open_in_session(run.join("bsc-sources"), 8, Ok("an-earlier-boot".into())).unwrap();
    earlier
        .request("l-on", SwitchId::Live, Mode::Enabled, 0, 1)
        .unwrap();
    earlier
        .request("v-live", VenueId::Flap, Mode::Live, 1, 2)
        .unwrap();
    earlier
        .request("r-live", Entity::Runtime, Mode::Live, 2, 3)
        .unwrap();
    let s = projection::project(&root, &run);
    let mode = s.launch.runtime_mode.unwrap();
    assert_eq!(
        (mode.effective.as_str(), mode.block_reason.as_str()),
        ("BLOCKED", "ECONOMIC_LIVE_DISABLED")
    );
    let modes = s
        .monitoring
        .records
        .iter()
        .find(|r| r.schema == schema::bsc_runtime::source_modes::SCHEMA.name)
        .unwrap();
    let reasons: Vec<_> = modes
        .fields
        .iter()
        .filter(|f| f.name.ends_with(".block_reason"))
        .map(|f| f.value.as_str())
        .collect();
    assert!(reasons.contains(&"ECONOMIC_LIVE_DISABLED"), "{reasons:?}");
    assert!(
        !reasons.contains(&"ECONOMIC_LIVE_NOT_AUTHORIZED"),
        "{reasons:?}"
    );
    let live = s
        .launch
        .economic_controls
        .iter()
        .find(|c| c.id == "BSC_ECON_LIVE")
        .unwrap();
    assert_eq!(live.block_reason, "RESET_BY_RESTART");
    fs::remove_dir_all(&root).unwrap();
}

/// BSC3-078: the tabs count the owner's attempts as the running LIVE path
/// does: besides the attempt its fresh status reports holding (armed under
/// the current grant), every slot otherwise.
#[test]
fn the_tabs_count_the_running_paths_held_attempt_as_its_own() {
    use darkbot_bsc_execution::sources::{
        ComponentId, Entity, LiveAdapter, LiveGrant, LiveStrategies, Mode, SourceId, Store,
        SwitchId, VenueId,
    };
    use darkbot_bsc_record::Value;
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
    let root = std::env::temp_dir().join(format!("darkbot-bsc-held-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rundir = root.join("run");
    let dir = rundir.join("bsc-sources");
    fs::create_dir_all(rundir.join("bsc-runtime")).unwrap();
    let store = Store::open(&dir, 8).unwrap();
    let now = projection::clock_s().unwrap() * 1_000;
    let mut g = 0;
    let mut req = |e: Entity, m: Mode| {
        let r = store.request(&format!("r{g}"), e, m, g, now).unwrap();
        assert!(r.applied(), "{}", r.line());
        g += 1;
    };
    for id in SwitchId::ALL {
        req(Entity::Switch(id), Mode::Enabled);
    }
    req(Entity::Runtime, Mode::Live);
    req(ComponentId::ExecC3.into(), Mode::Live);
    req(Entity::BuyLimit, Mode::PerDay(1));
    req(Entity::BuyTotal, Mode::PerDay(1));
    let grant = LiveGrant {
        venue: VenueId::Flap,
        strategies: LiveStrategies::C3,
        sources: vec![SourceId::PublicNode],
        adapters: vec![LiveAdapter::Club48BuilderV1],
        max_input_wei: 1_000_000_000_000_000,
        max_spend_wei: 10_000_000_000_000_000,
        max_attempts: 1,
        expires_unix_ms: now + 3_600_000,
        exit_only: false,
        risk: Some(darkbot_bsc_execution::sources::GrantRisk {
            max_buy_slippage_bps: 300,
            max_exit_slippage_bps: 500,
            max_gas_price_wei: 1_000_000_000,
            max_buy_gas_limit: 3_000_000,
            max_approve_gas_limit: 100_000,
            max_sell_gas_limit: 1_500_000,
        }),
        limits: Some(darkbot_bsc_execution::sources::GrantLimits {
            max_open_positions: 4,
            max_realized_loss_wei: 50_000_000_000_000_000,
            max_daily_realized_loss_wei: 20_000_000_000_000_000,
            min_buy_interval_ms: 0,
        }),
    };
    let r = store
        .grant_live("grant", grant.clone(), grant.scope_sha256(), g, now)
        .unwrap();
    assert!(r.applied(), "{}", r.line());
    let generation = store.current().unwrap().live_set_by.unwrap().generation;
    let attempt = [0xa0u8; 32];
    let hex = darkbot_bsc_record::hex(&attempt);
    store.consume_attempt(&hex, generation, now).unwrap();
    let status = |armed: &str, written: u64| {
        let mut live = filled(&rs::live::SCHEMA);
        live.set(rs::live::STATE, armed)
            .set(rs::live::GRANT_GENERATION, generation)
            .set(rs::live::LEASE_ATTEMPT, Value::B256(attempt));
        let mut d = filled(&rs::SCHEMA);
        d.set(rs::PROVENANCE_OBSERVED_UNIX_MS, written)
            .set(rs::STATUS_INTERVAL_MS, 1_000u64)
            .set(rs::LIVE, live.into_nested());
        fs::write(rundir.join("bsc-runtime/status.dbrec"), d.encode().unwrap()).unwrap();
    };
    let counted = || {
        let config = super::live::runtime_config(&root);
        let live =
            super::live::live_state(&root, &rundir, config.as_ref().map_err(|w| *w), Some(now));
        let launch = projection::project(&root, &rundir).launch;
        (live.total_used, launch.buy_attempt_limit.unwrap().used)
    };
    // The total slot is the attempt's own, the day's another id: only the
    // grant's total holds it.
    assert_eq!(counted(), (1, 1));
    status("ARMED", now);
    assert_eq!(counted().0, 0);
    // Not armed, stale: every slot counts.
    status("PROBING", now);
    assert_eq!(counted().0, 1);
    status("ARMED", now - 10_000);
    assert_eq!(counted().0, 1);
    fs::remove_dir_all(&root).unwrap();
}

/// REV.1.0 R1/R3b: the LIVE tab shows who decides the open lots' exits and
/// the receipt recovery as the runtime's status names them, where each
/// transaction's chain fact comes from (RECEIPT: a recovered block; the follower's
/// block: empty), and what the operator quarantined of the ledger (counted,
/// the first named; an unreadable quarantine is a warning of its own).
/// R3b-2b-2: the ledger's version (an operator's CAS), the burns requested,
/// the switches that lock exit-now and the burn, the entries to quarantine.
#[test]
fn the_live_tab_shows_the_exit_deciders_the_recovery_and_the_quarantine() {
    use darkbot_bsc_execution::live_ledger::{Fact, FactKind, FactSource, Ledger};
    use darkbot_bsc_execution::sources::{ComponentId, LiveIntentKind};
    use darkbot_bsc_record::schema::bsc_runtime::runtime_status as rs;
    let root = std::env::temp_dir().join(format!("darkbot-bsc-live-r3b-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rundir = root.join("run");
    let dir = rundir.join("bsc-sources");
    fs::create_dir_all(rundir.join("bsc-runtime")).unwrap();
    fs::create_dir_all(&dir).unwrap();
    let now = projection::clock_s().unwrap() * 1_000;
    // TEST_ONLY values, each its own.
    let mut live = filled(&rs::live::SCHEMA);
    live.push(rs::live::EXIT_UNDECIDED, "0xundecided")
        .push(rs::live::STRANDED, "0xstranded LIVE_POSITION_OFF_CURVE")
        .set(rs::live::LOT_READ_FAULTS, 3u64)
        .set(rs::live::LOT_READ_FAULT, "curve fault")
        .push(rs::live::RECOVERY_PENDING, "0xc1 RECOVERY_NOT_FINAL")
        .push(rs::live::RECOVERY_PENDING, "0xd5 RECOVERY_FAILED")
        .set(rs::live::RECOVERED, 5u64)
        .set(rs::live::RECOVERY_FAULTS, 7u64)
        .set(rs::live::RECOVERY_FAULT, "recovery fault");
    let mut d = filled(&rs::SCHEMA);
    d.set(rs::PROVENANCE_OBSERVED_UNIX_MS, now)
        .set(rs::LIVE, live.into_nested());
    fs::write(rundir.join("bsc-runtime/status.dbrec"), d.encode().unwrap()).unwrap();
    let released = |n: u8| Fact {
        kind: FactKind::Released,
        tx_hash: [n; 32],
        intent: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        token: [0xAB; 20],
        nonce: u64::from(n),
        grant_generation: 1,
        input_wei: Some(1_000u64.try_into().unwrap()),
        max_wei: Some(2_000u64.try_into().unwrap()),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(100),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    let ledger = Ledger::open(&dir);
    let block = Some((90, [9; 32], 1_000));
    for (n, kind, source) in [
        (1u8, FactKind::Included, Some(FactSource::Receipt)),
        (2, FactKind::Replaced, Some(FactSource::Receipt)),
        (3, FactKind::Reverted, None),
    ] {
        let f = released(n);
        ledger.append(&f, now).unwrap();
        ledger
            .append(
                &Fact {
                    block,
                    gas_wei: Some(1u64.try_into().unwrap()),
                    source,
                    ..Fact::about(&f, kind)
                },
                now,
            )
            .unwrap();
    }
    let quarantine = dir.join("live-ledger-quarantine");
    fs::create_dir_all(&quarantine).unwrap();
    for seq in [7u64, 3] {
        fs::write(quarantine.join(format!("{seq:020}.dbrec")), b"record").unwrap();
        fs::write(quarantine.join(format!("{seq:020}.entry")), b"entry").unwrap();
    }
    let tab = || {
        let config = super::live::runtime_config(&root);
        super::live::live_state(&root, &rundir, config.as_ref().map_err(|w| *w), Some(now))
    };
    let l = tab();
    assert_eq!(l.exit_undecided, ["0xundecided"]);
    assert_eq!(l.stranded, ["0xstranded LIVE_POSITION_OFF_CURVE"]);
    assert_eq!(
        (l.lot_read_faults, l.lot_read_fault.as_str()),
        (3, "curve fault")
    );
    assert_eq!(
        l.recovery_pending,
        ["0xc1 RECOVERY_NOT_FINAL", "0xd5 RECOVERY_FAILED"]
    );
    assert_eq!(
        (l.recovered, l.recovery_faults, l.recovery_fault.as_str()),
        (5, 7, "recovery fault")
    );
    assert_eq!(
        l.transactions
            .iter()
            .map(|t| (t.state.as_str(), t.source.as_str()))
            .collect::<Vec<_>>(),
        [
            ("INCLUDED", "RECEIPT"),
            ("REPLACED", "RECEIPT"),
            ("REVERTED", "")
        ]
    );
    assert!(
        l.warnings
            .contains(&"LIVE_LEDGER_QUARANTINED 2, first seq 3".to_string()),
        "{:?}",
        l.warnings
    );
    // R3b: the ledger's version, the quarantined numbers counted (never the
    // facts read: 6).
    assert_eq!(l.ledger_version, Some(8));
    // An unreadable quarantine is named, never silent.
    fs::remove_dir_all(&quarantine).unwrap();
    fs::write(&quarantine, b"not a directory").unwrap();
    let l = tab();
    assert!(
        l.warnings
            .contains(&"LIVE_LEDGER_QUARANTINE_UNREADABLE".to_string()),
        "{:?}",
        l.warnings
    );
    fs::remove_file(&quarantine).unwrap();
    assert!(
        !tab()
            .warnings
            .iter()
            .any(|w| w.starts_with("LIVE_LEDGER_QUARANTINE"))
    );
    // R3b: a burn requested of an unresolved transaction; the switches
    // that lock exit-now and the burn (none without a configuration, else
    // as configured); an entry that does not decode, which the operator may
    // quarantine (no version then).
    // Its window unknown: its bytes may still land.
    let unresolved = Fact {
        max_block: None,
        ..released(4)
    };
    ledger.append(&unresolved, now).unwrap();
    ledger
        .append(
            &Fact {
                operator: Some(darkbot_bsc_execution::live_ledger::Operator {
                    request_id: "burn-1".into(),
                    decided_by: "operator".into(),
                    evidence: None,
                    at_ms: now,
                }),
                ..Fact::about(&unresolved, FactKind::BurnRequested)
            },
            now,
        )
        .unwrap();
    let l = tab();
    assert_eq!(l.ledger_version, Some(8));
    assert_eq!(
        l.transactions
            .iter()
            .map(|t| (t.intent.as_str(), t.burn_requested))
            .collect::<Vec<_>>(),
        [
            ("BUY", false),
            ("BUY", false),
            ("BUY", false),
            ("BUY", true)
        ]
    );
    assert_eq!((l.operator_exit, l.nonce_burn), (None, None));
    // The ledger's own rules, without a LIVE buyer: no burn.
    assert!(l.transactions.iter().all(|t| !t.burnable));
    // The open lot (its token's, nothing of its exit out) takes every
    // decision; the failed and the pending lots none.
    assert_eq!(
        l.positions
            .iter()
            .map(|p| (p.state.as_str(), p.decisions.join(",")))
            .collect::<Vec<_>>(),
        [
            ("OPEN", "RESUME,WRITE_OFF,EXIT_NOW".to_string()),
            ("FAILED", String::new()),
            ("PENDING", String::new()),
        ]
    );
    // A configuration with its LIVE buyer: the transaction whose window is
    // unknown is burnable, the resolved ones are not.
    const CONFIG: &str = include_str!("../../../../../config/ingress.public.env");
    let buyer = "0x00000000000000000000000000000000000000aB";
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::write(
        root.join(super::live::RUNTIME_CONFIG_PATH),
        CONFIG
            .replace(
                "BSC_EXEC_LIVE_NONCE_BURN=OFF\n",
                "BSC_EXEC_LIVE_NONCE_BURN=ON\n",
            )
            .replace(
                "BSC_STATE_WATCHED=\n",
                &format!("BSC_STATE_WATCHED={buyer}\n"),
            )
            + &format!(
                "BSC_LIVE_KEY=~/.darkbot/bsc/live/buyer.key\nBSC_LIVE_BUYER={buyer}\nBSC_LIVE_ROUTE=LOOPBACK:18545\nBSC_LIVE_CONNECT_TIMEOUT_MS=5000\n"
            ),
    )
    .unwrap();
    let l = tab();
    assert_eq!((l.operator_exit, l.nonce_burn), (Some(false), Some(true)));
    assert_eq!(
        l.transactions
            .iter()
            .map(|t| t.burnable)
            .collect::<Vec<_>>(),
        [false, false, false, true]
    );
    assert_eq!(l.first_unreadable, None);
    fs::write(
        dir.join("live-ledger").join(format!("{:020}.dbrec", 8)),
        b"torn",
    )
    .unwrap();
    fs::write(
        dir.join("live-ledger").join(format!("{:020}.dbrec", 9)),
        b"torn too",
    )
    .unwrap();
    let l = tab();
    assert!(
        l.warnings
            .contains(&"LIVE_LEDGER_INVALID 2 unreadable, first seq 8".to_string()),
        "{:?}",
        l.warnings
    );
    assert_eq!((l.ledger_version, l.first_unreadable), (None, Some(8)));
    fs::remove_dir_all(&root).unwrap();
}

/// Plan 28.10.: the LIVE tab projects the owner's gate with every
/// precondition and its reason, the grant's exact scope, the attempts and
/// spend, the scope's entities and the ledger's positions; without the
/// qualification on file the gate stays BLOCKED with that reason.
#[test]
fn the_live_tab_projects_the_gate_grant_and_ledger() {
    use darkbot_bsc_execution::live_ledger::{Fact, FactKind, Ledger};
    use darkbot_bsc_execution::sources::{
        ComponentId, Entity, LiveAdapter, LiveGrant, LiveIntentKind, LiveStrategies, Mode,
        SourceId, Store, SwitchId, VenueId,
    };
    let root = std::env::temp_dir().join(format!("darkbot-bsc-live-tab-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rundir = root.join("run");
    let dir = rundir.join("bsc-sources");
    fs::create_dir_all(&rundir).unwrap();
    let store = Store::open(&dir, 8).unwrap();
    let now = projection::clock_s().unwrap() * 1_000;
    let mut g = 0;
    let mut req = |e: Entity, m: Mode| {
        let r = store.request(&format!("r{g}"), e, m, g, now).unwrap();
        assert!(r.applied(), "{}", r.line());
        g += 1;
    };
    for id in SwitchId::ALL {
        req(Entity::Switch(id), Mode::Enabled);
    }
    req(Entity::Runtime, Mode::Live);
    req(ComponentId::ExecC3.into(), Mode::Live);
    // The exit decider of the grant's lots (R1).
    req(ComponentId::StrategyC.into(), Mode::Live);
    req(ComponentId::StateC2.into(), Mode::Live);
    req(VenueId::Flap.into(), Mode::Live);
    req(SourceId::PublicNode.into(), Mode::Live);
    req(Entity::BuyLimit, Mode::PerDay(2));
    req(Entity::BuyTotal, Mode::PerDay(10));
    let grant = LiveGrant {
        venue: VenueId::Flap,
        strategies: LiveStrategies::C3,
        sources: vec![SourceId::PublicNode],
        adapters: vec![LiveAdapter::Club48BuilderV1],
        max_input_wei: 1_000_000_000_000_000,
        max_spend_wei: 10_000_000_000_000_000,
        max_attempts: 2,
        expires_unix_ms: now + 3_600_000,
        exit_only: false,
        risk: Some(darkbot_bsc_execution::sources::GrantRisk {
            max_buy_slippage_bps: 300,
            max_exit_slippage_bps: 500,
            max_gas_price_wei: 1_000_000_000,
            max_buy_gas_limit: 3_000_000,
            max_approve_gas_limit: 100_000,
            max_sell_gas_limit: 1_500_000,
        }),
        limits: Some(darkbot_bsc_execution::sources::GrantLimits {
            max_open_positions: 4,
            max_realized_loss_wei: 50_000_000_000_000_000,
            max_daily_realized_loss_wei: 20_000_000_000_000_000,
            min_buy_interval_ms: 0,
        }),
    };
    let r = store
        .grant_live("grant", grant.clone(), grant.scope_sha256(), g, now)
        .unwrap();
    assert!(r.applied(), "{}", r.line());
    let buy = Fact {
        kind: FactKind::Released,
        tx_hash: [1; 32],
        intent: LiveIntentKind::Buy,
        component: ComponentId::ExecC3,
        token: [0xAB; 20],
        nonce: 0,
        grant_generation: g + 1,
        input_wei: Some(1_000u64.try_into().unwrap()),
        max_wei: Some(2_000u64.try_into().unwrap()),
        block: None,
        gas_wei: None,
        tokens: None,
        eth_wei: None,
        max_block: Some(100),
        by_tx: None,
        response: None,
        origin: None,
        launcher: None,
        placement: None,
        attempt: None,
        operator: None,
        source: None,
    };
    Ledger::open(&dir).append(&buy, now).unwrap();
    let config = super::live::runtime_config(&root);
    let live = super::live::live_state(&root, &rundir, config.as_ref().map_err(|w| *w), Some(now));
    assert_eq!(live.gate, "BLOCKED");
    assert_eq!(
        live.blockers.first().map(String::as_str),
        Some("LIVE_VENUE_UNQUALIFIED")
    );
    assert_eq!(live.qualification, "NONE");
    assert_eq!(live.preconditions.len(), 20);
    let met = |id: &str| live.preconditions.iter().find(|p| p.id == id).unwrap().met;
    assert!(met("ECONOMIC_LIVE") && met("LIVE_GRANT") && met("WALLET_BUYER"));
    // BSC3-069: the grant names its execution risk, shown as granted.
    assert!(met("GRANT_RISK"));
    assert!(!met("VENUE_QUALIFIED") && !met("RISK_DECISION"));
    let shown = live.grant.as_ref().unwrap();
    assert_eq!(shown.scope_text, grant.scope_text());
    let risk = shown.risk.as_ref().unwrap();
    assert_eq!(
        (
            risk.max_buy_slippage_bps,
            risk.max_gas_price_wei,
            risk.max_sell_gas_limit
        ),
        (300, 1_000_000_000, 1_500_000)
    );
    // 31.10. order: the granted limits, as granted.
    let limits = shown.limits.as_ref().unwrap();
    assert_eq!(
        (
            limits.max_open_positions,
            limits.max_realized_loss_wei.as_str(),
            limits.max_daily_realized_loss_wei.as_str(),
            limits.min_buy_interval_ms
        ),
        (4, "50000000000000000", "20000000000000000", 0)
    );
    assert_eq!(
        shown.scope_sha256,
        darkbot_bsc_record::hex(&grant.scope_sha256())
    );
    assert_eq!((shown.max_attempts, shown.set_generation), (2, g + 1));
    assert_eq!(live.generation, g + 1);
    assert_eq!((live.total_limit, live.total_used), (10, 0));
    assert_eq!(live.daily.as_ref().unwrap().per_day, 2);
    assert_eq!(live.spent_wei, "0");
    assert_eq!(live.runtime_mode.as_ref().unwrap().requested, "LIVE");
    assert_eq!(
        live.scope.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        [
            "BSC_RUNTIME_MODE",
            "BSC_STATE_C2",
            "BSC_EXEC",
            // 01.11. (R1): the exit decider of every grant's lots.
            "BSC_STRATEGY_C",
            "BSC_VENUE_FLAP",
            "BSC_INGRESS_PUBLICNODE"
        ]
    );
    assert!(
        live.scope
            .iter()
            .all(|s| s.effective == "BLOCKED" && s.block_reason == "LIVE_VENUE_UNQUALIFIED")
    );
    assert_eq!(live.positions.len(), 1);
    assert_eq!(live.positions[0].state, "PENDING");
    assert_eq!(live.transactions[0].state, "RELEASED");
    assert_eq!(live.path_state, "NOT_RUNNING");
    // Monitoring and Launch / BUY name the same decision as the LIVE tab,
    // not the owner record's stored placeholder (browser E2E 28.10.).
    let s = projection::project(&root, &rundir);
    let modes = s
        .monitoring
        .records
        .iter()
        .find(|r| r.schema == schema::bsc_runtime::source_modes::SCHEMA.name)
        .unwrap();
    let leaf = |id: &str, name: &str| {
        let row = modes
            .fields
            .iter()
            .find(|f| f.value == id && f.name.ends_with("_id"))
            .and_then(|f| f.name.rsplit_once('.'))
            .map(|(row, _)| format!("{row}.{name}"))
            .unwrap();
        modes
            .fields
            .iter()
            .find(|f| f.name == row)
            .map(|f| f.value.clone())
    };
    for id in [
        "BSC_VENUE_FLAP",
        "BSC_INGRESS_PUBLICNODE",
        "BSC_STATE_C2",
        "BSC_EXEC",
    ] {
        assert_eq!(
            (leaf(id, "effective"), leaf(id, "block_reason")),
            (
                Some("BLOCKED".into()),
                Some("LIVE_VENUE_UNQUALIFIED".into())
            ),
            "{id}"
        );
    }
    let mode = s.launch.runtime_mode.unwrap();
    assert_eq!(
        (mode.effective.as_str(), mode.block_reason.as_str()),
        ("BLOCKED", "LIVE_VENUE_UNQUALIFIED")
    );
    fs::remove_dir_all(root).unwrap();
}
/// Audit E 1.3/1.6: the Wallets tab names the LIVE buyer the runtime configuration names (C2
/// watches it, or the runtime refuses the configuration), never a hand-written status; the
/// grant form offers only the sources the configuration gives an adapter endpoint. Without a
/// readable configuration both are UNKNOWN/empty and the reason is a warning.
#[test]
fn the_wallets_and_the_grant_follow_the_runtime_configuration() {
    let root = std::env::temp_dir().join(format!("darkbot-bsc-config-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bsc/config")).unwrap();
    fs::create_dir_all(root.join("run")).unwrap();
    // A hand-written public wallet file is no input.
    fs::write(
        root.join("bsc/config/wallet.public.env"),
        "BSC_EXPECTED_ADDRESS=0x1111111111111111111111111111111111111111\nBSC_WALLET_STATUS=PROVISIONED\n",
    )
    .unwrap();
    let project = || projection::project(&root, &root.join("run"));
    let s = project();
    assert_eq!(
        (
            s.wallets.expected_address.as_str(),
            s.wallets.wallet_status.as_str()
        ),
        ("", "UNKNOWN")
    );
    assert!(s.live.grantable_sources.is_empty());
    assert!(
        s.live
            .warnings
            .contains(&"RUNTIME_CONFIG_UNREADABLE".to_string())
    );
    assert!(
        s.status
            .warnings
            .contains(&"RUNTIME_CONFIG_UNREADABLE".to_string())
    );
    // The shipped configuration: PublicNode and dRPC have an adapter endpoint, no LIVE path.
    const CONFIG: &str = include_str!("../../../../../config/ingress.public.env");
    let config = root.join(super::live::RUNTIME_CONFIG_PATH);
    fs::write(&config, CONFIG).unwrap();
    let s = project();
    assert_eq!(s.wallets.wallet_status, "UNKNOWN");
    assert_eq!(
        s.live.grantable_sources,
        ["BSC_INGRESS_PUBLICNODE", "BSC_INGRESS_DRPC"]
    );
    assert!(
        !s.status
            .warnings
            .iter()
            .any(|w| w.starts_with("RUNTIME_CONFIG"))
    );
    // A LIVE path: its buyer, watched by C2.
    let buyer = "0x00000000000000000000000000000000000000aB";
    let live = CONFIG.replace(
        "BSC_STATE_WATCHED=\n",
        &format!("BSC_STATE_WATCHED={buyer}\n"),
    ) + &format!(
        "BSC_LIVE_KEY=~/.darkbot/bsc/live/buyer.key\nBSC_LIVE_BUYER={buyer}\nBSC_LIVE_ROUTE=LOOPBACK:18545\nBSC_LIVE_CONNECT_TIMEOUT_MS=5000\n"
    );
    fs::write(&config, &live).unwrap();
    let s = project();
    assert_eq!(
        (
            s.wallets.expected_address.as_str(),
            s.wallets.wallet_status.as_str()
        ),
        ("0x00000000000000000000000000000000000000ab", "WATCHED")
    );
    // A configuration the runtime refuses (the buyer not watched) names nothing.
    fs::write(&config, CONFIG.to_string() + &live[CONFIG.len()..]).unwrap();
    let s = project();
    assert_eq!(
        (
            s.wallets.expected_address.as_str(),
            s.wallets.wallet_status.as_str()
        ),
        ("", "UNKNOWN")
    );
    assert!(s.live.grantable_sources.is_empty());
    assert!(
        s.status
            .warnings
            .contains(&"RUNTIME_CONFIG_INVALID".to_string())
    );
    fs::remove_dir_all(&root).unwrap();
}
/// Audit E 1.4: a snapshot without a readable COMMIT or SYNCED_AT is UNKNOWN with its warning,
/// never a stand-in revision ("working-tree") or a silent time 0.
#[test]
fn an_unreadable_snapshot_identity_is_unknown() {
    let root = std::env::temp_dir().join(format!("darkbot-bsc-identity-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("run")).unwrap();
    let s = projection::project(&root, &root.join("run")).status;
    assert_eq!(
        (s.source_commit.as_str(), s.data_as_of_unix_s),
        ("UNKNOWN", 0)
    );
    for w in ["SOURCE_COMMIT_UNKNOWN", "SNAPSHOT_SYNC_TIME_UNKNOWN"] {
        assert!(s.warnings.contains(&w.to_string()), "{w}");
    }
    fs::write(root.join("COMMIT"), "0159c1a7\n").unwrap();
    fs::write(root.join("SYNCED_AT"), "garbage").unwrap();
    let s = projection::project(&root, &root.join("run")).status;
    assert_eq!(
        (s.source_commit.as_str(), s.data_as_of_unix_s),
        ("0159c1a7", 0)
    );
    assert!(
        s.warnings
            .contains(&"SNAPSHOT_SYNC_TIME_UNKNOWN".to_string())
    );
    fs::write(root.join("SYNCED_AT"), "1792232000").unwrap();
    let s = projection::project(&root, &root.join("run")).status;
    assert_eq!(s.data_as_of_unix_s, 1792232000);
    assert!(!s.warnings.iter().any(|w| w.ends_with("_UNKNOWN")));
    fs::remove_dir_all(&root).unwrap();
}
/// REV.1.0 R2b: an attempt as `bsc_runtime live attempts` writes it. Synthetic values, every
/// field its own (a swapped field is visible); they test the projection only, no acquisition
/// sample.
fn evaluated_attempt(ordinal: u64) -> darkbot_bsc_execution::live_attempts::Attempt {
    use alloy_primitives::U256;
    use darkbot_bsc_execution::live_attempts::{
        AdapterRow, Attempt, Counters, Local, Verdicts, Window,
    };
    use darkbot_bsc_execution::live_ledger::{Placement, PlacementVerdict};
    use darkbot_bsc_execution::sources::{LiveAdapter, SourceId};
    Attempt {
        ordinal,
        attempt: [0xA0; 32],
        tx_hash: [ordinal as u8; 32],
        kind: "C3_BUY",
        cell: "EXTERNAL_FULL_LAUNCH",
        token: [0xAB; 20],
        launch_hash: Some([0x1A; 32]),
        trigger: Some(SourceId::PublicNode),
        run: Some(100 + ordinal),
        state: "INCLUDED",
        block: Some((130_000_010, [0xB1; 32], 1_793_000_000_000)),
        tokens: Some(U256::from(5_000u64)),
        eth_wei: Some(U256::from(1_000u64)),
        gas_wei: Some(U256::from(21u64)),
        input_wei: Some(U256::from(1_000u64)),
        placement: Some(Placement {
            verdict: PlacementVerdict::Late,
            launch_index: Some(4),
            own_index: 6,
            buys_before: 1,
        }),
        chain_id: Some(56),
        written_ms: 1_793_000_000_100,
        local: Some(Local {
            clock_id: 2,
            ingress_ns: 1_000,
            select_ns: 1_400,
            send_ns: Some(1_900),
            ack_ns: Some(9_900),
            deadline_misses: 3,
        }),
        adapters: vec![AdapterRow {
            adapter: 7,
            builder: Some(LiveAdapter::Club48BuilderV1),
            outcome: "SENT",
            boundary: "WRITE",
            qualified: true,
            write_check_ns: Some(1_800),
            boundary_ns: Some(1_905),
            queue_ns: Some(1_950),
            answer: Some("ACCEPTED"),
            answer_ns: Some(9_950),
        }],
        window: Window {
            why_incomplete: None,
            window_ms: 15_000,
            launch_block: Some(130_000_000),
            launch_ms: Some(1_792_999_990_000),
            end_block: Some(130_000_020),
            trades: 7,
            buys: 5,
            sells: 2,
            buy_wei: U256::from(9_000u64),
            sell_wei: U256::from(3_000u64),
            first_external: Some((
                [0xE1; 32],
                130_000_001,
                8,
                U256::from(250_000_000_000_000_000u64),
            )),
            own_price: Some(U256::from(200_000_000_000_000_000u64)),
            price_vs_first_external_ppm: Some(800_000),
        },
        verdicts: Verdicts {
            acquisition: "PASS",
            canonical: "FAIL",
            timing: "UNKNOWN",
            placement: "NOT_APPLICABLE",
            capture: "TEST_CAPTURE",
            local: "TEST_LOCAL",
            economics: "TEST_ECONOMICS",
            verdict: "TEST_VERDICT",
        },
        reasons: vec!["TIMING_WITNESS_MISSING".into(), "PLACEMENT_LATE".into()],
        counters: Counters {
            attempts: 20 + ordinal,
            acquired: 30 + ordinal,
            passed: 40 + ordinal,
            failed: 50 + ordinal,
            undecided: 60 + ordinal,
        },
    }
}
/// The name `bsc_runtime live attempts` gives the record of [`evaluated_attempt`]`(n)`: its
/// transaction.
fn attempt_file(n: u64) -> String {
    darkbot_bsc_execution::live_attempts::attempt_file_name(&[n as u8; 32])
}
/// The warning of `total` attempt records, more than the campaign's limit (re-check F2).
fn beyond_limit(total: usize) -> String {
    use super::campaign::MAX_ATTEMPTS;
    format!(
        "LIVE_ATTEMPTS_BEYOND_LIMIT {total}/{MAX_ATTEMPTS}, shown: the first {MAX_ATTEMPTS} by ordinal of the first {} by transaction",
        MAX_ATTEMPTS + 1
    )
}
/// [`evaluated_attempt`]`(n)` as the LIVE tab must show it from a record of the evaluation `ms`:
/// every field of the record (review H1), written out.
fn projected_attempt(n: u64, ms: u64) -> super::wire::bsc::LiveAttempt {
    use super::wire::bsc::{LiveAttempt, LiveAttemptAdapter};
    let hex = |b: u8, len: usize| format!("0x{}", format!("{b:02x}").repeat(len));
    LiveAttempt {
        ordinal: n,
        tx: hex(n as u8, 32),
        kind: "C3_BUY".into(),
        cell: "EXTERNAL_FULL_LAUNCH".into(),
        state: "INCLUDED".into(),
        verdict: "TEST_VERDICT".into(),
        acquisition: "PASS".into(),
        canonical: "FAIL".into(),
        timing: "UNKNOWN".into(),
        placement: "NOT_APPLICABLE".into(),
        capture: "TEST_CAPTURE".into(),
        local: "TEST_LOCAL".into(),
        economics: "TEST_ECONOMICS".into(),
        reasons: vec!["TIMING_WITNESS_MISSING".into(), "PLACEMENT_LATE".into()],
        window_why: String::new(),
        trades: 7,
        buys: 5,
        sells: 2,
        first_external_price: Some("250000000000000000".into()),
        price_vs_first_external_ppm: Some(800_000),
        ingress_to_send_ns: Some(900),
        select_to_send_ns: Some(500),
        send_to_ack_ns: Some(8_000),
        attempts: 20 + n,
        acquired: 30 + n,
        passed: 40 + n,
        failed: 50 + n,
        undecided: 60 + n,
        adapters: vec![LiveAttemptAdapter {
            builder: "CLUB48_BUILDER_V1".into(),
            outcome: "SENT".into(),
            answer: "ACCEPTED".into(),
            adapter: 7,
            boundary: "WRITE".into(),
            qualified: true,
            t_write_check_ns: Some(1_800),
            t_boundary_ns: Some(1_905),
            t_queue_ns: Some(1_950),
            t_answer_ns: Some(9_950),
        }],
        window: "COMPLETE".into(),
        attempt: hex(0xA0, 32),
        run: Some(100 + n),
        token: hex(0xAB, 20),
        launch_hash: hex(0x1A, 32),
        trigger: "BSC_INGRESS_PUBLICNODE".into(),
        block_number: Some(130_000_010),
        block_hash: hex(0xB1, 32),
        block_ms: Some(1_793_000_000_000),
        tokens: Some("5000".into()),
        eth_wei: Some("1000".into()),
        gas_wei: Some("21".into()),
        own_price: Some("200000000000000000".into()),
        placed: "LATE".into(),
        launch_index: Some(4),
        own_index: Some(6),
        buys_before: Some(1),
        clock_id: Some(2),
        t_ingress_ns: Some(1_000),
        t_select_ns: Some(1_400),
        t_send_ns: Some(1_900),
        t_ack_ns: Some(9_900),
        deadline_misses: Some(3),
        window_ms: 15_000,
        launch_block: Some(130_000_000),
        launch_ms: Some(1_792_999_990_000),
        window_end_block: Some(130_000_020),
        t0_exact: false,
        first_external_buy_tx: hex(0xE1, 32),
        first_external_buy_block: Some(130_000_001),
        first_external_buy_index: Some(8),
        buy_wei: "9000".into(),
        sell_wei: "3000".into(),
        evaluated_unix_ms: ms,
    }
}
/// The LIVE campaign warnings of a LIVE state, in their order.
fn campaign_warnings(l: &super::wire::bsc::BscLiveState) -> Vec<String> {
    l.warnings
        .iter()
        .filter(|w| w.starts_with("LIVE_"))
        .cloned()
        .collect()
}
/// REV.1.0 R2b: the campaign verdict and the attempt records reach the LIVE tab typed and
/// unchanged (a real `Campaign::encode` / `Attempt::encode` round trip, every field of both
/// records its own value, review H1/M5); absent records are no rows and no warning.
#[test]
fn the_live_tab_shows_the_campaign_verdict_and_the_attempts_as_recorded() {
    use super::wire::bsc::{LiveAttempt, LiveCampaign, LiveCampaignCell};
    use darkbot_bsc_execution::live_attempts::{ATTEMPTS_DIR, Campaign, CellRow, VERDICT_PATH};
    use darkbot_bsc_record::schema::bsc_runtime::live_attempt as la;
    use darkbot_bsc_record::{Value, hex};
    const MS: u64 = 1_793_000_050_000;
    let root = std::env::temp_dir().join(format!("darkbot-bsc-campaign-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rundir = root.join("run");
    let dir = rundir.join(ATTEMPTS_DIR);
    fs::create_dir_all(&dir).unwrap();
    let live =
        || super::live::live_state(&root, &rundir, Err("TEST_ONLY"), Some(1_793_000_100_000));
    let empty = live();
    assert!(empty.campaign.is_none() && empty.attempts.is_empty());
    assert_eq!(campaign_warnings(&empty), Vec::<String>::new());
    let campaign = Campaign {
        verdict: "CAMPAIGN_NOT_READY",
        reasons: vec!["NO_MANIFEST".into(), "NO_PRECOMMIT".into()],
        release: "RELEASE_EVIDENCE_NOT_READY",
        manifest_sha256: Some([0x11; 32]),
        precommit_sha256: Some([0x33; 32]),
        build_sha256: [0x22; 32],
        planned: 11,
        observed: 41,
        selected: 31,
        admitted: 5,
        skipped: 37,
        written: 9,
        acquired: 8,
        reverted: 2,
        dropped: 3,
        late: 4,
        unresolved: 6,
        passed: 12,
        failed: 13,
        undecided: 14,
        consecutive_misses: 15,
        before_precommit: 16,
        unverifiable: 17,
        skip_reasons: vec!["HASH_ONLY=7".into(), "LATE_SIGHTING=2".into()],
        covered_s: 3_600,
        uncovered_s: 400,
        covered_ppm: 900_000,
        cells: vec![
            CellRow {
                cell: "EXTERNAL_FULL_LAUNCH",
                enabled: true,
                min_attempts: Some(25),
                attempts: 21,
                passed: 22,
                failed: 23,
                undecided: 24,
                min_duration_s: Some(26),
                duration_s: 3_601,
                verdict: "CAMPAIGN_OPEN",
            },
            CellRow {
                cell: "SELF_LAUNCH",
                enabled: false,
                min_attempts: None,
                attempts: 0,
                passed: 0,
                failed: 0,
                undecided: 0,
                min_duration_s: None,
                duration_s: 0,
                verdict: "CAMPAIGN_NOT_READY",
            },
        ],
    };
    let bytes = campaign.encode(MS).unwrap();
    fs::write(rundir.join(VERDICT_PATH), &bytes).unwrap();
    for n in [2u64, 1] {
        fs::write(
            dir.join(attempt_file(n)),
            evaluated_attempt(n).encode(MS).unwrap(),
        )
        .unwrap();
    }
    let l = live();
    assert_eq!(campaign_warnings(&l), Vec::<String>::new());
    assert_eq!(
        l.campaign.unwrap(),
        LiveCampaign {
            verdict: "CAMPAIGN_NOT_READY".into(),
            release_evidence: "RELEASE_EVIDENCE_NOT_READY".into(),
            reasons: vec!["NO_MANIFEST".into(), "NO_PRECOMMIT".into()],
            planned: 11,
            observed: 41,
            selected: 31,
            admitted: 5,
            skipped: 37,
            written: 9,
            acquired: 8,
            reverted: 2,
            dropped: 3,
            late: 4,
            unresolved: 6,
            passed: 12,
            failed: 13,
            undecided: 14,
            consecutive_misses: 15,
            before_precommit: 16,
            unverifiable: 17,
            covered_s: 3_600,
            uncovered_s: 400,
            covered_ppm: 900_000,
            skip_reasons: vec!["HASH_ONLY=7".into(), "LATE_SIGHTING=2".into()],
            cells: vec![
                LiveCampaignCell {
                    cell: "EXTERNAL_FULL_LAUNCH".into(),
                    enabled: true,
                    min_attempts: Some(25),
                    attempts: 21,
                    passed: 22,
                    failed: 23,
                    undecided: 24,
                    min_duration_s: Some(26),
                    duration_s: 3_601,
                    verdict: "CAMPAIGN_OPEN".into(),
                },
                LiveCampaignCell {
                    cell: "SELF_LAUNCH".into(),
                    enabled: false,
                    min_attempts: None,
                    attempts: 0,
                    passed: 0,
                    failed: 0,
                    undecided: 0,
                    min_duration_s: None,
                    duration_s: 0,
                    verdict: "CAMPAIGN_NOT_READY".into(),
                },
            ],
            manifest_sha256: "11".repeat(32),
            precommit_sha256: "33".repeat(32),
            build_sha256: "22".repeat(32),
            evaluated_unix_ms: MS,
            record_sha256: hex(&darkbot_bsc_record::sha256(&bytes)),
        }
    );
    // In their order, every recorded field as the record holds it.
    assert_eq!(
        l.attempts,
        [projected_attempt(1, MS), projected_attempt(2, MS)]
    );
    // An incomplete window without a release, an inclusion, a placement or a launch: the
    // record's own words (review L9), nothing where it names nothing.
    let mut open = evaluated_attempt(3);
    open.window.why_incomplete = Some("WINDOW_NOT_FINAL");
    open.window.first_external = None;
    open.window.price_vs_first_external_ppm = None;
    open.window.own_price = None;
    open.window.launch_block = None;
    open.window.launch_ms = None;
    open.window.end_block = None;
    open.local = None;
    open.adapters.clear();
    open.block = None;
    open.tokens = None;
    open.eth_wei = None;
    open.gas_wei = None;
    open.placement = None;
    open.launch_hash = None;
    open.trigger = None;
    open.run = None;
    open.reasons.clear();
    fs::write(dir.join(attempt_file(3)), open.encode(MS).unwrap()).unwrap();
    assert_eq!(
        live().attempts.pop().unwrap(),
        LiveAttempt {
            window: "INCOMPLETE".into(),
            window_why: "WINDOW_NOT_FINAL".into(),
            first_external_price: None,
            price_vs_first_external_ppm: None,
            own_price: None,
            first_external_buy_tx: String::new(),
            first_external_buy_block: None,
            first_external_buy_index: None,
            launch_block: None,
            launch_ms: None,
            window_end_block: None,
            ingress_to_send_ns: None,
            select_to_send_ns: None,
            send_to_ack_ns: None,
            clock_id: None,
            t_ingress_ns: None,
            t_select_ns: None,
            t_send_ns: None,
            t_ack_ns: None,
            deadline_misses: None,
            adapters: Vec::new(),
            block_number: None,
            block_hash: String::new(),
            block_ms: None,
            tokens: None,
            eth_wei: None,
            gas_wei: None,
            placed: String::new(),
            launch_index: None,
            own_index: None,
            buys_before: None,
            launch_hash: String::new(),
            trigger: String::new(),
            run: None,
            reasons: Vec::new(),
            ..projected_attempt(3, MS)
        }
    );
    // T0 is the producer's exact time only where the record says so.
    let r =
        darkbot_bsc_record::decode(&la::SCHEMA, &evaluated_attempt(1).encode(MS).unwrap()).unwrap();
    let mut exact = Draft::new(&la::SCHEMA);
    for (tag, v) in r.fields() {
        let v = if *tag == la::T0_EXACT {
            Value::Bool(true)
        } else {
            v.clone()
        };
        exact.push(*tag, v);
    }
    fs::write(dir.join(attempt_file(1)), exact.encode().unwrap()).unwrap();
    assert_eq!(
        live().attempts[0],
        LiveAttempt {
            t0_exact: true,
            ..projected_attempt(1, MS)
        }
    );
    fs::remove_dir_all(&root).unwrap();
}
/// REV.1.0 R2b review (M2, M3, M4, L6, re-check L-2/L-3): a damaged record is a warning, never a
/// value, and every kind of damage is one counted warning. An attempt record is only the one named
/// by its own transaction: garbage, another schema, a copy, capitals, another name or one beyond a
/// record's size is not, and a link is not even to a valid record of that very transaction (nor
/// is a link to a valid verdict); the writer's `.dbrec.tmp` and other names are not read. The
/// attempts directory is a real directory. A tie in the order and a record of another evaluation
/// (than the verdict's, without one than the newest) stay as written, with a warning. At most the
/// limit and one more record are read, the first by transaction.
#[test]
fn damaged_campaign_records_are_warnings_and_the_attempts_stay_bounded() {
    use super::campaign::MAX_ATTEMPTS;
    use darkbot_bsc_execution::live_attempts::{ATTEMPTS_DIR, Campaign, VERDICT_PATH};
    use std::os::unix::fs::symlink;
    let root =
        std::env::temp_dir().join(format!("darkbot-bsc-campaign-bad-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rundir = root.join("run");
    let dir = rundir.join(ATTEMPTS_DIR);
    fs::create_dir_all(&dir).unwrap();
    let live = || super::live::live_state(&root, &rundir, Err("TEST_ONLY"), Some(1));
    let write = |n: u64, ms: u64| {
        fs::write(
            dir.join(attempt_file(n)),
            evaluated_attempt(n).encode(ms).unwrap(),
        )
        .unwrap()
    };
    let ordinals = |l: &super::wire::bsc::BscLiveState| {
        l.attempts.iter().map(|a| a.ordinal).collect::<Vec<_>>()
    };
    write(1, 7);
    let verdict = Campaign {
        verdict: "CAMPAIGN_OPEN",
        release: "RELEASE_EVIDENCE_NOT_READY",
        ..Campaign::default()
    }
    .encode(7)
    .unwrap();
    // A torn verdict.
    fs::write(rundir.join(VERDICT_PATH), &verdict[..verdict.len() - 1]).unwrap();
    // Garbage and another schema under attempts' names.
    fs::write(dir.join(attempt_file(2)), b"not a record").unwrap();
    fs::write(dir.join(attempt_file(3)), &verdict).unwrap();
    // A copy of the first attempt (its transaction is the first's).
    fs::copy(dir.join(attempt_file(1)), dir.join(attempt_file(4))).unwrap();
    // Review L-2: a link under the fifth's name to a valid record of the fifth, outside the
    // directory: refused as a link, nothing else.
    let outside = rundir.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(
        outside.join(attempt_file(5)),
        evaluated_attempt(5).encode(7).unwrap(),
    )
    .unwrap();
    symlink(outside.join(attempt_file(5)), dir.join(attempt_file(5))).unwrap();
    // Beyond a record's size.
    fs::File::create(dir.join(attempt_file(6)))
        .unwrap()
        .set_len(darkbot_bsc_record::MAX_RECORD_BYTES as u64 + 1)
        .unwrap();
    // Its transaction in capitals, and a short name.
    let mut capital = evaluated_attempt(0xAB);
    capital.tx_hash = [0xAB; 32];
    fs::write(
        dir.join(format!("0x{}.dbrec", "AB".repeat(32))),
        capital.encode(7).unwrap(),
    )
    .unwrap();
    fs::write(
        dir.join("0x01.dbrec"),
        evaluated_attempt(1).encode(7).unwrap(),
    )
    .unwrap();
    // Not an attempt record's name: never read.
    fs::write(dir.join("README"), b"x").unwrap();
    fs::write(dir.join(format!("{}.tmp", attempt_file(7))), b"torn").unwrap();
    let l = live();
    assert!(l.campaign.is_none());
    assert_eq!(ordinals(&l), [1]);
    assert_eq!(
        campaign_warnings(&l),
        [
            "LIVE_CAMPAIGN_VERDICT_INVALID",
            "LIVE_ATTEMPT_INVALID 7, first 0x01.dbrec"
        ]
    );
    // Review L-2: a link to a valid verdict is no verdict.
    fs::remove_file(rundir.join(VERDICT_PATH)).unwrap();
    fs::write(outside.join("verdict.dbrec"), &verdict).unwrap();
    symlink(outside.join("verdict.dbrec"), rundir.join(VERDICT_PATH)).unwrap();
    let l = live();
    assert!(l.campaign.is_none());
    assert_eq!(campaign_warnings(&l)[0], "LIVE_CAMPAIGN_VERDICT_INVALID");
    fs::remove_file(rundir.join(VERDICT_PATH)).unwrap();
    // Review L6: the attempts directory is a real directory, never a link to one or a file.
    let elsewhere = rundir.join("elsewhere");
    fs::rename(&dir, &elsewhere).unwrap();
    symlink(&elsewhere, &dir).unwrap();
    let l = live();
    assert!(l.attempts.is_empty());
    assert_eq!(campaign_warnings(&l), ["LIVE_ATTEMPTS_UNREADABLE"]);
    fs::remove_file(&dir).unwrap();
    fs::write(&dir, b"x").unwrap();
    assert_eq!(campaign_warnings(&live()), ["LIVE_ATTEMPTS_UNREADABLE"]);
    fs::remove_file(&dir).unwrap();
    fs::remove_dir_all(&elsewhere).unwrap();
    fs::create_dir_all(&dir).unwrap();
    // Review M3, M4: a tie in the order and a record of another evaluation than the verdict's
    // stay as written, each kind one warning.
    fs::write(rundir.join(VERDICT_PATH), &verdict).unwrap();
    write(1, 7);
    write(2, 8);
    let mut tie = evaluated_attempt(1);
    tie.tx_hash = [3; 32];
    fs::write(dir.join(attempt_file(3)), tie.encode(7).unwrap()).unwrap();
    let l = live();
    assert_eq!(
        l.attempts
            .iter()
            .map(|a| (a.ordinal, a.tx.as_str()))
            .collect::<Vec<_>>(),
        [
            (1, &attempt_file(1)[..66]),
            (1, &attempt_file(3)[..66]),
            (2, &attempt_file(2)[..66])
        ]
    );
    assert_eq!(
        campaign_warnings(&l),
        [
            format!(
                "LIVE_ATTEMPT_OF_ANOTHER_EVALUATION 1, first {}",
                attempt_file(2)
            ),
            format!(
                "LIVE_ATTEMPT_DUPLICATE_ORDINAL 1, first {}",
                attempt_file(3)
            ),
        ]
    );
    // Without a verdict, against the newest attempt's evaluation.
    fs::remove_file(rundir.join(VERDICT_PATH)).unwrap();
    fs::remove_file(dir.join(attempt_file(3))).unwrap();
    assert_eq!(
        campaign_warnings(&live()),
        [format!(
            "LIVE_ATTEMPT_OF_ANOTHER_EVALUATION 1, first {}",
            attempt_file(1)
        )]
    );
    fs::write(rundir.join(VERDICT_PATH), &verdict).unwrap();
    // More records than the campaign can have attempts: the first ones and a warning. Nothing
    // beyond the limit and one more is read: a damaged record after them stays unread, a short
    // name is never one of them.
    for n in 1..=MAX_ATTEMPTS as u64 + 1 {
        write(n, 7);
    }
    fs::write(dir.join(attempt_file(0xF0)), b"not a record").unwrap();
    fs::write(dir.join("0x01.dbrec"), b"not a record").unwrap();
    let l = live();
    assert_eq!(ordinals(&l), (1..=MAX_ATTEMPTS as u64).collect::<Vec<_>>());
    assert_eq!(
        campaign_warnings(&l),
        [
            "LIVE_ATTEMPT_INVALID 1, first 0x01.dbrec".to_string(),
            beyond_limit(MAX_ATTEMPTS + 2)
        ]
    );
    // Re-check F2, F4: ordinals against the transactions' order; of the limit and one more first
    // by transaction, the first by ordinal are shown, and the warnings are of those shown only
    // (the first by transaction, the one not shown, is of another evaluation).
    for n in 1..=MAX_ATTEMPTS as u64 + 1 {
        let mut a = evaluated_attempt(n);
        a.ordinal = MAX_ATTEMPTS as u64 + 2 - n;
        let ms = if n == 1 { 8 } else { 7 };
        fs::write(dir.join(attempt_file(n)), a.encode(ms).unwrap()).unwrap();
    }
    let l = live();
    assert_eq!(ordinals(&l), (1..=MAX_ATTEMPTS as u64).collect::<Vec<_>>());
    assert_eq!(
        l.attempts.iter().map(|a| a.tx.clone()).collect::<Vec<_>>(),
        (2..=MAX_ATTEMPTS as u64 + 1)
            .rev()
            .map(|n| attempt_file(n)[..66].to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        campaign_warnings(&l),
        [
            "LIVE_ATTEMPT_INVALID 1, first 0x01.dbrec".to_string(),
            beyond_limit(MAX_ATTEMPTS + 2)
        ]
    );
    fs::remove_dir_all(&root).unwrap();
}
/// REV.1.0 R2b review (M2, re-check L-1): whatever the records hold, the campaign section takes
/// only what the rest of the LIVE tab leaves of `bsc.live.state`, and it comes last. Every list is
/// cut at its record's share of that in the evaluator's order with one counted warning, a record
/// whose own fields alone are beyond it is a warning, and the worst case (the limit of attempts,
/// every text as long as a record allows) behind a LIVE tab filled at a large practical size
/// encodes below the publisher's bound; when the section as a whole does not fit, none of it is
/// shown, counted, and when not even that count fits, the rest alone (re-check F1).
#[test]
fn the_campaign_section_takes_only_what_the_rest_of_the_live_tab_leaves() {
    use super::campaign::{MAX_ATTEMPTS, share};
    use super::publisher::MAX_STATE_BYTES;
    use super::wire::bsc::{
        BscLiveState, LiveGrant, LivePosition, LivePrecondition, LiveTransaction,
    };
    use darkbot_bsc_execution::live_attempts::{
        ATTEMPTS_DIR, AdapterRow, Campaign, CellRow, VERDICT_PATH,
    };
    let root =
        std::env::temp_dir().join(format!("darkbot-bsc-campaign-share-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let rundir = root.join("run");
    let dir = rundir.join(ATTEMPTS_DIR);
    fs::create_dir_all(&dir).unwrap();
    // Texts as long as a record allows, four times what the whole state could hold of them per
    // record: far more than any share.
    let longest = darkbot_bsc_record::DEFAULT_MAX_LEN as usize;
    let many = 4 * MAX_STATE_BYTES / (MAX_ATTEMPTS + 2) / longest;
    let texts = |tag: char| {
        (0..many)
            .map(|i| format!("{i:04}{}", tag.to_string().repeat(longest - 4)))
            .collect::<Vec<String>>()
    };
    let campaign = Campaign {
        verdict: "CAMPAIGN_OPEN",
        release: "RELEASE_EVIDENCE_NOT_READY",
        reasons: texts('R'),
        skip_reasons: texts('S'),
        ..Campaign::default()
    };
    fs::write(rundir.join(VERDICT_PATH), campaign.encode(7).unwrap()).unwrap();
    // The first attempt's own fields alone are beyond any share: its adapters' answers.
    let answer: &'static str = "A".repeat(longest).leak();
    let mut over = evaluated_attempt(1);
    over.adapters = vec![
        AdapterRow {
            answer: Some(answer),
            ..over.adapters[0].clone()
        };
        many
    ];
    fs::write(dir.join(attempt_file(1)), over.encode(7).unwrap()).unwrap();
    for k in 2..=MAX_ATTEMPTS as u64 + 1 {
        let mut a = evaluated_attempt(k);
        a.reasons = texts('A');
        fs::write(dir.join(attempt_file(k)), a.encode(7).unwrap()).unwrap();
    }
    let is_section = |w: &String| w.starts_with("LIVE_CAMPAIGN_") || w.starts_with("LIVE_ATTEMPT");
    let first = |list: &[String], tag: char| {
        !list.is_empty()
            && list
                .iter()
                .enumerate()
                .all(|(i, t)| t.starts_with(&format!("{i:04}{tag}")))
    };
    // `l` behind `rest`: every record within its share of what `rest` leaves, its lists the
    // evaluator's first texts in their order, the section's warnings after the rest's.
    let check = |l: &BscLiveState, rest: &BscLiveState| {
        let s = share(MAX_STATE_BYTES - rest.encoded_len());
        assert_eq!(
            l.attempts.iter().map(|a| a.ordinal).collect::<Vec<_>>(),
            (2..=MAX_ATTEMPTS as u64 + 1).collect::<Vec<_>>()
        );
        let c = l.campaign.as_ref().unwrap();
        assert!(c.encoded_len() <= s && first(&c.reasons, 'R'));
        assert!(c.skip_reasons.iter().all(|t| t.contains('S')));
        let hidden = 2 * many - c.reasons.len() - c.skip_reasons.len();
        let mut hidden_attempts = 0;
        for a in &l.attempts {
            assert!(a.encoded_len() <= s && first(&a.reasons, 'A'));
            hidden_attempts += many - a.reasons.len();
        }
        assert!(hidden > 0 && hidden_attempts > 0);
        let (own, section) = l.warnings.split_at(rest.warnings.len());
        assert_eq!(own, rest.warnings);
        assert_eq!(
            section,
            [
                format!("LIVE_CAMPAIGN_REASONS_BEYOND_LIMIT {hidden}"),
                format!("LIVE_ATTEMPT_INVALID 1, first {}", attempt_file(1)),
                format!(
                    "LIVE_ATTEMPT_REASONS_BEYOND_LIMIT {hidden_attempts}, first {}",
                    attempt_file(2)
                ),
                beyond_limit(MAX_ATTEMPTS + 1),
            ]
        );
        assert!(
            l.encoded_len() <= MAX_STATE_BYTES,
            "{} of {MAX_STATE_BYTES}",
            l.encoded_len()
        );
    };
    // The LIVE tab of this run directory: the section last, behind everything else.
    let l = super::live::live_state(&root, &rundir, Err("TEST_ONLY"), Some(1));
    let rest = BscLiveState {
        campaign: None,
        attempts: Vec::new(),
        warnings: l
            .warnings
            .iter()
            .filter(|w| !is_section(w))
            .cloned()
            .collect(),
        ..l.clone()
    };
    check(&l, &rest);
    // Review L-1: behind a LIVE tab filled at a large practical size, its texts as long as the
    // reporter lets one be, its ledger several lots and transactions per attempt.
    let text = super::projection::safe_text(&"T".repeat(longest));
    let word = "9".repeat(78);
    let lots = 4 * MAX_ATTEMPTS;
    let full = BscLiveState {
        gate: "BLOCKED".into(),
        blockers: vec![text.clone(); 8],
        preconditions: (0..16)
            .map(|i| LivePrecondition {
                id: format!("PRECONDITION_{i}"),
                met: false,
                reason: text.clone(),
            })
            .collect(),
        grant: Some(LiveGrant {
            scope_text: text.clone(),
            ..Default::default()
        }),
        path_state: "BLOCKED".into(),
        path_reason: text.clone(),
        last_refusal: text.clone(),
        positions: (0..lots)
            .map(|i| LivePosition {
                lot: format!("0x{i:064x}"),
                token: format!("0x{i:040x}"),
                component: "EXEC_C3".into(),
                state: "CLOSED".into(),
                tokens: word.clone(),
                input_wei: word.clone(),
                proceeds_wei: word.clone(),
                gas_wei: word.clone(),
                pnl_wei: Some(format!("-{word}")),
                decisions: ["RESUME", "WRITE_OFF", "EXIT_NOW"]
                    .map(String::from)
                    .to_vec(),
            })
            .collect(),
        transactions: (0..4 * lots)
            .map(|i| LiveTransaction {
                tx_hash: format!("0x{i:064x}"),
                intent: "SELL".into(),
                token: format!("0x{i:040x}"),
                nonce: u64::MAX,
                state: "INCLUDED".into(),
                block_number: Some(u64::MAX),
                source: "RECEIPT".into(),
                burn_requested: true,
                burnable: true,
            })
            .collect(),
        realized_pnl_wei: format!("-{word}"),
        gas_wei: word.clone(),
        warnings: vec![text.clone(); 4],
        // R3b: every open lot undecided or stranded, every transaction
        // pending its recovery, the faults as long as a text may be.
        exit_undecided: (0..lots).map(|i| format!("0x{i:040x}")).collect(),
        stranded: (0..lots)
            .map(|i| format!("0x{i:040x} LIVE_POSITION_UNREADABLE"))
            .collect(),
        lot_read_faults: u64::MAX,
        lot_read_fault: text.clone(),
        recovery_pending: (0..4 * lots)
            .map(|i| format!("0x{i:064x} RECOVERY_SOURCES_DISAGREE"))
            .collect(),
        recovered: u64::MAX,
        recovery_faults: u64::MAX,
        recovery_fault: text.clone(),
        // R3b-2b-2: the operator's CAS, the switches, an entry to quarantine.
        ledger_version: Some(u64::MAX),
        operator_exit: Some(true),
        nonce_burn: Some(true),
        first_unreadable: Some(u64::MAX),
        ..Default::default()
    };
    let mut l = full.clone();
    super::campaign::project(&rundir, &mut l);
    check(&l, &full);
    // When the section as a whole does not fit what the rest leaves, none of it is shown and one
    // warning counts what was not.
    // The rest leaves exactly that one warning's bytes.
    let counted = "LIVE_CAMPAIGN_BEYOND_STATE 0 records, 3 warnings";
    let cost = BscLiveState {
        warnings: vec![counted.into()],
        ..Default::default()
    }
    .encoded_len();
    let framing = BscLiveState {
        path_reason: "T".repeat(MAX_STATE_BYTES),
        ..Default::default()
    }
    .encoded_len()
        - MAX_STATE_BYTES;
    let mut edge = BscLiveState {
        path_reason: "T".repeat(MAX_STATE_BYTES - framing - cost),
        ..Default::default()
    };
    super::campaign::project(&rundir, &mut edge);
    assert!(edge.campaign.is_none() && edge.attempts.is_empty());
    assert_eq!(edge.warnings, [counted]);
    assert_eq!(edge.encoded_len(), MAX_STATE_BYTES);
    // Re-check F1: one byte less left, not even that count fits: the rest alone, within the bound.
    let rest = BscLiveState {
        path_reason: "T".repeat(MAX_STATE_BYTES - framing - cost + 1),
        ..Default::default()
    };
    let mut beyond = rest.clone();
    super::campaign::project(&rundir, &mut beyond);
    assert_eq!(beyond, rest);
    assert!(beyond.encoded_len() <= MAX_STATE_BYTES);
    // A verdict whose own fields alone are beyond its share (its cells) is a warning, no value.
    let cell: &'static str = "C".repeat(longest).leak();
    let over = Campaign {
        cells: vec![
            CellRow {
                cell,
                enabled: true,
                min_attempts: None,
                attempts: 0,
                passed: 0,
                failed: 0,
                undecided: 0,
                min_duration_s: None,
                duration_s: 0,
                verdict: "CAMPAIGN_OPEN",
            };
            many
        ],
        ..campaign
    };
    fs::write(rundir.join(VERDICT_PATH), over.encode(7).unwrap()).unwrap();
    let l = super::live::live_state(&root, &rundir, Err("TEST_ONLY"), Some(1));
    assert!(l.campaign.is_none());
    assert!(
        l.warnings
            .contains(&"LIVE_CAMPAIGN_VERDICT_INVALID".to_string())
    );
    fs::remove_dir_all(&root).unwrap();
}
