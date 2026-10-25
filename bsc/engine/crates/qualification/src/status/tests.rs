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
    assert_eq!(
        field("sources[1].block_reason").as_deref(),
        Some("ECONOMIC_LIVE_NOT_AUTHORIZED")
    );
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
