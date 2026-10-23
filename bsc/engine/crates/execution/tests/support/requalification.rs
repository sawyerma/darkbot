//! Offline wiring fixtures only. Never a real chain/requalification attestation.
#![allow(dead_code)]
use std::path::Path;
use darkbot_bsc_execution::acceptance_contract::{
    Authorization, DRY_RECORD, FROZEN_DIR, REQUAL_CHECKS, REQUAL_LAUNCH_PATH,
    REQUAL_LAUNCH_PATH_CHECK, REQUAL_STATUS, attempt1_v4, attempt1_v4_evidence,
};
use darkbot_bsc_execution::acceptance_guard::V4_RELATIVE_PATH;
use darkbot_bsc_record::schema::{
    PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL, g06a::requalification as r,
};
use darkbot_bsc_record::{Draft, Value, decode, sha256};

/// The real launch-path snapshot of the frozen A DRY state: it checks
/// against the frozen preflight the stand-in records copy.
pub const LAUNCH_PATH_EVIDENCE: &str =
    "bsc/native/evidence/g06a/launch_path_frozen_a_20261023/launch_path.dbrec";
/// Where the stand-in public record of candidate A is written.
pub const CANDIDATE_RECORD: &str = "bsc/native/state/g07_candidates/a.candidate.dbrec";

pub fn fixture(root: &Path, dir: &str, policy: &str, repo: &Path) {
    std::fs::copy(repo.join(DRY_RECORD), root.join(DRY_RECORD)).unwrap();
    std::fs::write(root.join(dir).join("native_quote.dbrec"), b"offline-test quote stand-in; wiring only").unwrap();
    refresh(root, dir, policy);
}
pub fn refresh(root: &Path, dir: &str, policy: &str) {
    status(root, dir, policy, false);
}
/// The v5 shape: DRY5 and the launch-path record bound as well.
pub fn refresh_v5(root: &Path, dir: &str, policy: &str) {
    status(root, dir, policy, true);
}
fn status(root: &Path, dir: &str, policy: &str, v5: bool) {
    let mut d = Draft::new(&r::SCHEMA);
    d.set(PROVENANCE_TOOL, "g06a_requal_check")
        .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(r::VERDICT, "PASS")
        .set(r::REQUAL_DIR, dir)
        .set(r::FROZEN_DIR, FROZEN_DIR);
    let extra = if v5 {
        Some(REQUAL_LAUNCH_PATH_CHECK)
    } else {
        None
    };
    for name in REQUAL_CHECKS.iter().copied().chain(extra) {
        let mut row = Draft::new(&r::check::SCHEMA);
        row.set(r::check::NAME, name)
            .set(r::check::STATUS, "PASS")
            .set(
                r::check::DETAIL,
                "offline-test synthetic wiring fixture; no chain authority",
            );
        d.push(r::CHECKS, row.into_nested());
    }
    let extra = if v5 { Some(REQUAL_LAUNCH_PATH) } else { None };
    for name in [
        "preflight.dbrec",
        "native_quote.dbrec",
        "economics_crosscheck.dbrec",
        "local_simulation.dbrec",
        "dry_result.dbrec",
        "portal_upgrade.dbrec",
        policy,
    ]
    .into_iter()
    .chain(extra)
    {
        let path = if name == policy {
            root.join(name)
        } else {
            root.join(dir).join(name)
        };
        let mut row = Draft::new(&r::bound::SCHEMA);
        row.set(r::bound::NAME, name).set(r::bound::SHA256, Value::B256(sha256(&std::fs::read(path).unwrap())));
        d.push(r::RECORDS, row.into_nested());
    }
    std::fs::write(root.join(dir).join(REQUAL_STATUS), d.encode().unwrap()).unwrap();
}

/// The public record of candidate A derived against the run's preflight
/// (its ID recomputed; the salt commitment and calldata hash are stand-ins).
pub fn candidate(root: &Path, dir: &str) {
    use darkbot_bsc_record::schema::g06a::{dry as d, preflight as p};
    use darkbot_bsc_record::schema::g07_candidate::{candidate as c, candidate_id_of};
    let pre_bytes = std::fs::read(root.join(dir).join("preflight.dbrec")).unwrap();
    let pre = decode(&p::SCHEMA, &pre_bytes).unwrap();
    let dry_bytes = std::fs::read(root.join(dir).join("dry_result.dbrec")).unwrap();
    let dry = decode(&d::NATIVE, &dry_bytes).unwrap();
    let mut x = Draft::new(&c::SCHEMA);
    let copy = |x: &mut Draft, to: u16, from: &darkbot_bsc_record::Record, tag: u16| {
        x.set(to, from.get(tag).unwrap().clone());
    };
    x.set(c::PROFILE, "SELF_A")
        .set(c::CANDIDATE_ID, Value::B256([0; 32]))
        .set(c::CHAIN_ID, 56u64);
    copy(&mut x, c::PORTAL, &pre, p::PORTAL);
    copy(&mut x, c::PORTAL_VERSION, &pre, p::PORTAL_VERSION);
    copy(
        &mut x,
        c::PORTAL_IMPLEMENTATION,
        &pre,
        p::PORTAL_IMPLEMENTATION,
    );
    copy(
        &mut x,
        c::PORTAL_IMPLEMENTATION_CODE_SHA256,
        &pre,
        p::PORTAL_IMPLEMENTATION_CODE_SHA256,
    );
    copy(&mut x, c::TAX_V3_IMPLEMENTATION, &pre, p::TAX_V3_IMPL);
    copy(
        &mut x,
        c::TAX_V3_IMPLEMENTATION_CODE_SHA256,
        &pre,
        p::TAX_V3_IMPL_CODE_SHA256,
    );
    copy(&mut x, c::LAUNCHER, &dry, d::LAUNCHER);
    copy(&mut x, c::TOKEN, &dry, d::TOKEN);
    copy(&mut x, c::LAUNCH_TX_HASH, &dry, d::LAUNCH_TX_HASH);
    copy(&mut x, c::BUY_TX_HASH, &dry, d::BUY_TX_HASH);
    x.set(c::DEPLOYMENT_RECORD_SHA256, Value::B256(sha256(&pre_bytes)))
        .set(c::SALT_COMMITMENT, Value::B256([0x5a; 32]))
        .set(c::SALT_ORIGIN, "FROZEN_PUBLIC_ECONOMICS_RECORD")
        .set(c::LAUNCH_CALLDATA_SHA256, Value::B256([0x5b; 32]))
        .set(c::ECONOMICS_RECORD_SHA256, Value::B256([0x5c; 32]))
        .set(c::DRY_RECORD_SHA256, Value::B256(sha256(&dry_bytes)))
        .set(c::QUALIFICATION, "UNQUALIFIED")
        .set(c::PROVENANCE, "offline-test")
        .set(c::CREATED_UNIX_MS, 1u64);
    let id = candidate_id_of(&decode(&c::SCHEMA, &x.encode().unwrap()).unwrap()).unwrap();
    x.set(c::CANDIDATE_ID, Value::B256(id));
    let to = root.join(CANDIDATE_RECORD);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::write(to, x.encode().unwrap()).unwrap();
}

/// A v4-shaped stand-in run made v5-shaped: its v4 proposal written at the
/// v4 path (what v5 supersedes), the real launch-path snapshot, the
/// candidate record and the DRY5 status.
pub fn upgrade_to_v5(root: &Path, dir: &str, policy: &str, repo: &Path) {
    let v4 = attempt1_v4(
        &attempt1_v4_evidence(root, dir, policy).unwrap(),
        Authorization::Proposed,
    )
    .unwrap();
    let at = root.join(V4_RELATIVE_PATH);
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, v4).unwrap();
    std::fs::copy(
        repo.join(LAUNCH_PATH_EVIDENCE),
        root.join(dir).join(REQUAL_LAUNCH_PATH),
    )
    .unwrap();
    candidate(root, dir);
    refresh_v5(root, dir, policy);
}
