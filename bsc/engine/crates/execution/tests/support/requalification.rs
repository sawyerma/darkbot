//! Offline wiring fixtures only. Never a real chain/requalification attestation.
#![allow(dead_code)]
use std::path::Path;
use darkbot_bsc_execution::acceptance_contract::{
    Authorization, DRY_RECORD, FROZEN_DIR, REQUAL_B_CANDIDATE_CHECK, REQUAL_CHECKS,
    REQUAL_LAUNCH_PATH, REQUAL_LAUNCH_PATH_CHECK, REQUAL_STATUS, attempt1_v4, attempt1_v4_evidence,
};
use darkbot_bsc_execution::acceptance_guard::V4_RELATIVE_PATH;
use darkbot_bsc_record::schema::{
    PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL, g06a::requalification as r,
};
use darkbot_bsc_record::{Draft, Schema, Value, decode, sha256};

/// The real launch-path snapshot of the frozen A DRY state: it checks
/// against the frozen preflight the stand-in records copy.
pub const LAUNCH_PATH_EVIDENCE: &str =
    "bsc/native/evidence/g06a/launch_path_frozen_a_20261023/launch_path.dbrec";
/// Where the stand-in public record of candidate A is written.
pub const CANDIDATE_RECORD: &str = "bsc/native/state/g07_candidates/a.candidate.dbrec";
/// Where the stand-in public record of candidate B is written.
pub const B_CANDIDATE_RECORD: &str = "bsc/native/state/g07_candidates/b.candidate.dbrec";
/// The stand-in SELF_B token (Tax-V3 vanity suffix) and salt commitment.
pub const B_TOKEN: [u8; 20] = [
    0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb,
    0xbb, 0xbb, 0x77, 0x77,
];
pub const B_COMMITMENT: [u8; 32] = [0x5c; 32];

/// A native record of `native` with a frozen import record's fields (same
/// tags), provenance instead of the import: offline stand-ins for a
/// requalification DRY that exercise the wiring only, never qualify.
pub fn native_copy(
    repo: &Path,
    import: &str,
    schema: &'static Schema,
    native: &'static Schema,
) -> Vec<u8> {
    let bytes = std::fs::read(repo.join(FROZEN_DIR).join(import)).unwrap();
    let record = decode(schema, &bytes).unwrap();
    let mut d = Draft::new(native);
    for (tag, value) in record.fields() {
        if *tag > 2 {
            d.push(*tag, value.clone());
        }
    }
    if native.field(PROVENANCE_TOOL).is_some() {
        d.set(PROVENANCE_TOOL, "offline-test")
            .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64);
    }
    d.encode().unwrap()
}

/// The stand-in records of a requalification run in `root/dir`: the frozen
/// DRY, preflight, simulation and cross-check as native records and an R0
/// record from the frozen implementation to itself.
pub fn stand_in_records(root: &Path, dir: &str, repo: &Path) {
    use darkbot_bsc_record::schema::g06a::{
        dry, economics, local_simulation, portal_upgrade as up, preflight,
    };
    std::fs::create_dir_all(root.join(dir)).unwrap();
    let write =
        |name: &str, bytes: Vec<u8>| std::fs::write(root.join(dir).join(name), bytes).unwrap();
    write(
        "dry_result.dbrec",
        native_copy(repo, "dry_result.dbrec", &dry::SCHEMA, &dry::NATIVE),
    );
    write(
        "preflight.dbrec",
        native_copy(
            repo,
            "preflight.dbrec",
            &preflight::SCHEMA,
            &preflight::SCHEMA,
        ),
    );
    write(
        "local_simulation.dbrec",
        native_copy(
            repo,
            "local_simulation.dbrec",
            &local_simulation::SCHEMA,
            &local_simulation::NATIVE,
        ),
    );
    write(
        "economics_crosscheck.dbrec",
        native_copy(
            repo,
            "economics_crosscheck.dbrec",
            &economics::SCHEMA,
            &economics::NATIVE,
        ),
    );
    let implementation = decode(
        &preflight::SCHEMA,
        &std::fs::read(root.join(dir).join("preflight.dbrec")).unwrap(),
    )
    .unwrap()
    .address(preflight::PORTAL_IMPLEMENTATION)
    .unwrap();
    let mut scan = Draft::new(&up::SCHEMA);
    scan.set(PROVENANCE_TOOL, "offline-test")
        .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(up::PROXY, Value::Address([0xe2; 20]))
        .set(up::SCANNED_FROM_BLOCK, 1u64)
        .set(up::SCANNED_TO_BLOCK, 2u64)
        .set(up::CURRENT_IMPLEMENTATION, Value::Address(implementation))
        .set(up::BASELINE_IMPLEMENTATION, Value::Address(implementation))
        .set(up::COVERAGE_METHOD, up::METHOD)
        .set(up::CHAIN_SOURCE, "offline-test")
        .set(up::ARCHIVE_SOURCE, "offline-test")
        .set(up::HEADERS_VERIFIED, 2u64)
        .set(up::BLOOM_CANDIDATE_BLOCKS, 0u64)
        .set(
            up::CURRENT_IMPLEMENTATION_CODE_SHA256,
            Value::B256([0x42; 32]),
        )
        .set(up::CURRENT_IMPLEMENTATION_CODE_BYTES, 1u64)
        .set(up::FIRST_PARENT_HASH, Value::B256([1; 32]))
        .set(up::LAST_BLOCK_HASH, Value::B256([2; 32]));
    write("portal_upgrade.dbrec", scan.encode().unwrap());
}

pub fn fixture(root: &Path, dir: &str, policy: &str, repo: &Path) {
    std::fs::copy(repo.join(DRY_RECORD), root.join(DRY_RECORD)).unwrap();
    std::fs::write(root.join(dir).join("native_quote.dbrec"), b"offline-test quote stand-in; wiring only").unwrap();
    refresh(root, dir, policy);
}
pub fn refresh(root: &Path, dir: &str, policy: &str) {
    status(root, dir, policy, false, None);
}
/// The v5 shape: DRY5 and the launch-path record bound as well.
pub fn refresh_v5(root: &Path, dir: &str, policy: &str) {
    status(root, dir, policy, true, None);
}
/// The SELF_B shape: also DRY6 and the B candidate record at its path.
pub fn refresh_b(root: &Path, dir: &str, policy: &str) {
    status(root, dir, policy, true, Some(B_CANDIDATE_RECORD));
}
fn status(root: &Path, dir: &str, policy: &str, v5: bool, b: Option<&str>) {
    let mut d = Draft::new(&r::SCHEMA);
    d.set(PROVENANCE_TOOL, "g06a_requal_check")
        .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
        .set(r::VERDICT, "PASS")
        .set(r::REQUAL_DIR, dir)
        .set(r::FROZEN_DIR, FROZEN_DIR);
    let extra = [
        v5.then_some(REQUAL_LAUNCH_PATH_CHECK),
        b.map(|_| REQUAL_B_CANDIDATE_CHECK),
    ];
    for name in REQUAL_CHECKS
        .iter()
        .copied()
        .chain(extra.into_iter().flatten())
    {
        let mut row = Draft::new(&r::check::SCHEMA);
        row.set(r::check::NAME, name)
            .set(r::check::STATUS, "PASS")
            .set(
                r::check::DETAIL,
                "offline-test synthetic wiring fixture; no chain authority",
            );
        d.push(r::CHECKS, row.into_nested());
    }
    let extra = [v5.then_some(REQUAL_LAUNCH_PATH), b].into_iter().flatten();
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
        let path = if name == policy || Some(name) == b {
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
    candidate_at(root, dir, CANDIDATE_RECORD, "SELF_A");
}

/// The candidate record of `profile` at `at` (SELF_B: no A evidence, the
/// stand-in B commitment).
fn candidate_at(root: &Path, dir: &str, at: &str, profile: &str) {
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
    x.set(c::PROFILE, profile)
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
    x.set(c::DEPLOYMENT_RECORD_SHA256, Value::B256(sha256(&pre_bytes)))
        .set(c::LAUNCH_CALLDATA_SHA256, Value::B256([0x5b; 32]));
    if profile == "SELF_A" {
        copy(&mut x, c::LAUNCH_TX_HASH, &dry, d::LAUNCH_TX_HASH);
        copy(&mut x, c::BUY_TX_HASH, &dry, d::BUY_TX_HASH);
        x.set(c::SALT_COMMITMENT, Value::B256([0x5a; 32]))
            .set(c::SALT_ORIGIN, "FROZEN_PUBLIC_ECONOMICS_RECORD")
            .set(c::ECONOMICS_RECORD_SHA256, Value::B256([0x5c; 32]))
            .set(c::DRY_RECORD_SHA256, Value::B256(sha256(&dry_bytes)));
    } else {
        x.set(c::SALT_COMMITMENT, Value::B256(B_COMMITMENT))
            .set(c::SALT_ORIGIN, "OS_CSPRNG");
    }
    x.set(c::QUALIFICATION, "UNQUALIFIED")
        .set(c::PROVENANCE, "offline-test")
        .set(c::CREATED_UNIX_MS, 1u64);
    let id = candidate_id_of(&decode(&c::SCHEMA, &x.encode().unwrap()).unwrap()).unwrap();
    x.set(c::CANDIDATE_ID, Value::B256(id));
    let to = root.join(at);
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

/// `bytes` of `schema` with the named fields set, every other field kept.
pub fn with(bytes: &[u8], schema: &'static Schema, set: &[(&str, Value)]) -> Vec<u8> {
    let record = decode(schema, bytes).unwrap();
    let mut d = Draft::new(schema);
    for (tag, value) in record.fields() {
        d.push(*tag, value.clone());
    }
    for (name, value) in set {
        let tag = schema.fields.iter().find(|f| f.name == *name).unwrap().tag;
        d.set(tag, value.clone());
    }
    d.encode().unwrap()
}

/// A v5-shaped stand-in run (see [`upgrade_to_v5`]) made SELF_B's: its own
/// token and signed hashes in the DRY and the simulation, the private
/// cross-check (salt only as its commitment), the launch path of its token,
/// the public B candidate record and the DRY6 status.
pub fn make_b(root: &Path, dir: &str, policy: &str) {
    use darkbot_bsc_record::schema::g06a::{
        dry, economics, launch_path as lp, local_simulation as ls,
    };
    let file = |name: &str| root.join(dir).join(name);
    let token = Value::Address(B_TOKEN);
    let (launch, buy) = (Value::B256([0x1b; 32]), Value::B256([0x2b; 32]));
    let edit = |name: &str, schema: &'static Schema, set: &[(&str, Value)]| {
        let bytes = std::fs::read(file(name)).unwrap();
        std::fs::write(file(name), with(&bytes, schema, set)).unwrap();
    };
    edit(
        "dry_result.dbrec",
        &dry::NATIVE,
        &[
            ("transactions.token", token.clone()),
            ("transactions.launch_tx_hash", launch.clone()),
            ("transactions.buy_tx_hash", buy.clone()),
        ],
    );
    edit(
        "local_simulation.dbrec",
        &ls::NATIVE,
        &[
            ("token", token.clone()),
            ("launch_tx_hash", launch),
            ("buy_tx_hash", buy),
        ],
    );
    let native = decode(
        &economics::NATIVE,
        &std::fs::read(file("economics_crosscheck.dbrec")).unwrap(),
    )
    .unwrap();
    let mut private = Draft::new(&economics::PRIVATE);
    for (tag, value) in native.fields() {
        if economics::PRIVATE.field(*tag).is_some() {
            private.push(*tag, value.clone());
        }
    }
    let token_tag = economics::PRIVATE
        .fields
        .iter()
        .find(|f| f.name == "token")
        .unwrap()
        .tag;
    private
        .set(token_tag, token.clone())
        .set(economics::SALT_COMMITMENT, Value::B256(B_COMMITMENT));
    std::fs::write(
        file("economics_crosscheck.dbrec"),
        private.encode().unwrap(),
    )
    .unwrap();
    edit(
        REQUAL_LAUNCH_PATH,
        &lp::SCHEMA,
        &[("token", token.clone()), ("launch_return_token", token)],
    );
    candidate_at(root, dir, B_CANDIDATE_RECORD, "SELF_B");
    refresh_b(root, dir, policy);
}
