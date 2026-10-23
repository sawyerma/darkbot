//! Typed acceptance of a requalification DRY run (R0, DRY-0 .. DRY-4)
//! against the frozen DRY: every record is decoded fail-closed, every field
//! of every new record is compared with its frozen counterpart, and only the
//! differences listed here with their reason are accepted. The signed
//! transaction hashes, token, salt, minimum output, economics, budget and
//! safety parameters must be identical; the Portal upgrade scan must cover
//! every block from the frozen DRY's block to the preflight's block with a
//! verified hash chain anchored at both block hashes and hold a gapless chain
//! of verified upgrades from the frozen implementation to the current one; the
//! builder floor must be a native re-determination from fetched sources,
//! unchanged and bound by the simulation; the launch-path snapshot (DRY-5,
//! v9.1 C1/R1) must be of the preflight's block, Portal and implementation
//! code and the DRY's token. Writes `requalification_status.dbrec` (DBREC001) and prints
//! `FLAP_REQUALIFICATION=PASS` or `=STOP`; exit 0 for PASS, 1 for STOP,
//! 2 for unusable input. Offline: reads local records only.
//!
//! With `--candidate <record>` of a secret-salt SELF_B candidate (plan v8,
//! v9.1 B2) the run is B's own DRY: token and signed hashes may (and must)
//! differ from the frozen A DRY, the cross-check is the private record and
//! DRY6 binds the candidate (ID, deployment, launcher, token, salt
//! commitment) to this run; every other comparison is unchanged.
//!
//! usage: g06a_requal_check <requal_dir> <frozen_dir> <builder_policy.dbrec> [--candidate <b.candidate.dbrec>]

use darkbot_bsc_execution::acceptance_guard::{V3_RELATIVE_PATH, sha256_hex};
use darkbot_bsc_record::schema::g06a::{
    builder_policy, dry, economics, local_simulation as sim, portal_upgrade as up, preflight as pf,
    quote, requalification as rq,
};
use darkbot_bsc_record::schema::{
    IMPORT_LEGACY_PATH, IMPORT_LEGACY_SHA256, PROVENANCE_OBSERVED_UNIX_MS, PROVENANCE_TOOL,
    decode_any,
};
use darkbot_bsc_record::{Draft, Record, Schema, Value, hex, plain, sha256, u256_from_u128};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

/// The head of the frozen DRY (the shared R0 check reads it from the frozen
/// preflight; the stand-in records use it).
#[cfg(test)]
const FROZEN_HEAD: u64 = 127_865_760;
/// The signed transactions of the approved candidate.
const LAUNCH_TX: &str = "db468c01529ee4c9597eae056f420f0d4d6d0753a1f785f8dbb3163028be5146";
const BUY_TX: &str = "b086f683550d1aa221db2add1aa1426444b7ef716de98594c40b9b6da765db32";
/// The Portal version the requalification targets: current Mainnet since
/// block 128914179 (readiness 2026-10-21, `native/evidence/g07/readiness_20261021T201733Z`).
const TARGET_VERSION: &str = "v5.26.0";
const SIGNED_GAS_PRICE: u64 = 50_000_000;
const MAX_TOTAL_PER_ATTEMPT: u128 = 481_163_500_000_000;

/// The launch-path snapshot of the requalification run (DRY-5).
const LAUNCH_PATH: &str = "launch_path.dbrec";

/// Record files of the requalification run and of the frozen DRY.
const FILES: [&str; 6] = [
    "preflight.dbrec",
    "native_quote.dbrec",
    "economics_crosscheck.dbrec",
    "local_simulation.dbrec",
    "dry_result.dbrec",
    "portal_upgrade.dbrec",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Check {
    name: &'static str,
    pass: bool,
    detail: String,
}

fn check(name: &'static str, problems: Vec<String>, pass_detail: &str) -> Check {
    Check {
        name,
        pass: problems.is_empty(),
        detail: if problems.is_empty() {
            pass_detail.to_string()
        } else {
            problems.join("; ")
        },
    }
}

fn load(path: &Path, accepted: &[&'static Schema]) -> Result<(Record, [u8; 32]), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let record = decode_any(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if !accepted.contains(&record.schema()) {
        return Err(format!(
            "{}: {} is not accepted here",
            path.display(),
            record.schema().name
        ));
    }
    Ok((record, sha256(&bytes)))
}

fn tag_of(schema: &Schema, name: &str) -> u16 {
    schema
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.tag)
        .unwrap_or_else(|| panic!("{} has no field {name}", schema.name))
}

fn values(r: &Record, name: &str) -> Vec<Value> {
    r.schema()
        .fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| r.all(f.tag).cloned().collect())
        .unwrap_or_default()
}

fn one(r: &Record, name: &str) -> Option<Value> {
    values(r, name).into_iter().next()
}

fn shown(v: &[Value]) -> String {
    if v.is_empty() {
        "(absent)".into()
    } else {
        v.iter().map(plain).collect::<Vec<_>>().join(" ; ")
    }
}

/// Every field name of either record outside provenance whose values differ
/// and which is not allowed to differ (`allowed`) or, in the new record, to
/// be absent (`absent_ok`, legacy-only prose of the frozen import).
fn unexpected_differences(
    frozen: &Record,
    new: &Record,
    allowed: &[&str],
    absent_ok: &[&str],
) -> Vec<String> {
    let names: BTreeSet<&str> = frozen
        .schema()
        .fields
        .iter()
        .chain(new.schema().fields)
        .map(|f| f.name)
        .filter(|n| !n.starts_with("_import.") && !n.starts_with("_provenance."))
        .collect();
    names
        .into_iter()
        .filter(|name| !allowed.contains(name))
        .filter_map(|name| {
            let (a, b) = (values(frozen, name), values(new, name));
            let absent_allowed = b.is_empty() && absent_ok.contains(&name);
            (a != b && !absent_allowed).then(|| format!("{name}: {} -> {}", shown(&a), shown(&b)))
        })
        .collect()
}

fn word(v: Option<Value>) -> Option<[u8; 32]> {
    match v {
        Some(Value::B256(w)) | Some(Value::U256(w)) => Some(w),
        _ => None,
    }
}

fn int(v: Option<Value>) -> Option<u64> {
    match v {
        Some(Value::U64(n)) => Some(n),
        _ => None,
    }
}

fn addr(v: Option<Value>) -> Option<[u8; 20]> {
    match v {
        Some(Value::Address(a)) => Some(a),
        _ => None,
    }
}

fn hex32(text: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).unwrap();
    }
    out
}

fn u128_of(w: [u8; 32]) -> Option<u128> {
    w[..16]
        .iter()
        .all(|b| *b == 0)
        .then(|| u128::from_be_bytes(w[16..].try_into().unwrap()))
}

/// The records and the verdict's facts.
struct Evaluation {
    checks: Vec<Check>,
    bound: Vec<(String, [u8; 32])>,
    facts: Draft,
}

fn evaluate(
    dir: &Path,
    frozen_dir: &Path,
    policy_path: &Path,
    candidate_path: Option<&Path>,
) -> Result<Evaluation, String> {
    let new = |name: &str, accepted: &[&'static Schema]| load(&dir.join(name), accepted);
    let old = |name: &str, accepted: &[&'static Schema]| load(&frozen_dir.join(name), accepted);
    let candidate = candidate_path
        .map(|p| {
            load(
                p,
                &[&darkbot_bsc_record::schema::g07_candidate::candidate::SCHEMA],
            )
        })
        .transpose()?;
    let b = candidate.is_some();
    if let Some((c, _)) = &candidate
        && one(c, "profile") != Some(Value::Text("SELF_B".into()))
    {
        return Err(
            "--candidate takes a SELF_B record; A is requalified against the frozen DRY".into(),
        );
    }
    let (pre, pre_sha) = new(FILES[0], &[&pf::SCHEMA])?;
    let (q, q_sha) = new(FILES[1], &[&quote::SCHEMA])?;
    let (econ, econ_sha) = new(
        FILES[2],
        if b {
            &[&economics::PRIVATE]
        } else {
            &[&economics::NATIVE]
        },
    )?;
    let (ls, ls_sha) = new(FILES[3], &[&sim::NATIVE])?;
    let (d, d_sha) = new(FILES[4], &[&dry::NATIVE])?;
    let (scan, scan_sha) = new(FILES[5], &[&up::SCHEMA])?;
    let (f_pre, _) = old("preflight.dbrec", &[&pf::SCHEMA])?;
    let (f_econ, _) = old("economics_crosscheck.dbrec", &[&economics::SCHEMA])?;
    let (f_ls, _) = old("local_simulation.dbrec", &[&sim::SCHEMA])?;
    let (f_d, _) = old("dry_result.dbrec", &[&dry::SCHEMA])?;
    let (policy, policy_sha) = load(policy_path, &[&builder_policy::NATIVE])?;
    let mut checks = Vec::new();
    let head = int(one(&pre, "latest_block_number")).unwrap_or(0);

    // R0: the record's own anchored, contiguous chain of upgrades (shared
    // with the PREPARE -> SIGN handoff, v9.1 D3), closed at this run's
    // preflight block, hash and implementation.
    let mut p = darkbot_bsc_execution::r0::chain_problems(&scan, &f_pre);
    let upgrades = scan.nested_all(up::UPGRADES).map_err(|e| e.0)?;
    let to = int(one(&scan, "scanned_to_block"));
    if addr(one(&pre, "portal")) != addr(one(&f_pre, "portal")) {
        p.push("the preflight's Portal is not the frozen Portal".into());
    }
    if to != Some(head) {
        p.push(format!("scan ends at {to:?}, not at the preflight head {head}"));
    }
    if word(one(&scan, "last_block_hash")) != word(one(&pre, "latest_block_hash")) {
        p.push("the hash chain does not end at the preflight's block hash".into());
    }
    if upgrades.is_empty() {
        p.push("no upgrade since the frozen head".into());
    }
    let frozen_impl = addr(one(&f_pre, "portal_implementation"));
    let current_impl = addr(one(&pre, "portal_implementation"));
    if addr(one(&scan, "current_implementation")) != current_impl {
        p.push("current implementation of the scan differs from the preflight".into());
    }
    if word(one(&scan, "current_implementation_code_sha256"))
        != word(one(&pre, "portal_implementation_code_sha256"))
        || int(one(&scan, "current_implementation_code_bytes"))
            != int(one(&pre, "portal_implementation_code_bytes"))
    {
        p.push("current implementation code differs between scan and preflight".into());
    }
    let mut facts = Draft::new(&rq::SCHEMA);
    if let Some(u) = upgrades.last() {
        let g = |name| one(u, name);
        let block = int(g("block")).unwrap_or(0);
        facts
            .set(rq::UPGRADE_BLOCK, block)
            .set_opt(rq::UPGRADE_TRANSACTION, word(g("transaction_hash")).map(Value::B256))
            .set_opt(rq::OLD_IMPLEMENTATION, frozen_impl.map(Value::Address))
            .set_opt(rq::NEW_IMPLEMENTATION, current_impl.map(Value::Address))
            .set_opt(
                rq::NEW_IMPLEMENTATION_CODE_SHA256,
                word(one(&pre, "portal_implementation_code_sha256")).map(Value::B256),
            );
    }
    checks.push(check(
        "R0_UPGRADE",
        p,
        "every block from the frozen DRY's block to the preflight's block covered by a verified, anchored header hash chain; a gapless chain of Upgraded events from the frozen to the current implementation, receipts verified against receiptsRoot, the implementation slot before and after each on an archive endpoint",
    ));

    // DRY-0: deployment, wallets, nonces, funding, gas.
    let mut p = unexpected_differences(
        &f_pre,
        &pre,
        &[
            "latest_block_number",
            "latest_block_hash",
            "portal_version",
            "portal_implementation",
            "portal_implementation_code_bytes",
            "portal_implementation_code_sha256",
            "gas_price_wei",
            "launcher_balance_wei",
            "buyer_balance_wei",
        ],
        &["schema", "tax_v3_impl_source", "dependencies"],
    );
    if one(&pre, "portal_version") != Some(Value::Text(TARGET_VERSION.into())) {
        p.push(format!("portal_version is not {TARGET_VERSION}"));
    }
    if word(one(&pre, "gas_price_wei")).and_then(u128_of).is_none_or(|g| g > u128::from(SIGNED_GAS_PRICE)) {
        p.push("network gas price exceeds the signed gas price".into());
    }
    if one(&pre, "wallet_qualification.funding_pass") != Some(Value::Bool(true)) {
        p.push("funding_pass is not true".into());
    }
    checks.push(check(
        "DRY0_PREFLIGHT",
        p,
        "deployment, economics, wallets, nonces, funding and reserves equal the frozen DRY except the documented Portal implementation upgrade",
    ));

    // The DRY record: everything identical but the projection and contract.
    let v3_sha = std::fs::read(V3_RELATIVE_PATH)
        .map(|b| sha256(&b))
        .map_err(|e| format!("{V3_RELATIVE_PATH}: {e}"))?;
    // B's own token and signed bytes; everything else as the frozen DRY.
    let b_identity: &[&str] = if b {
        &[
            "transactions.token",
            "transactions.launch_tx_hash",
            "transactions.buy_tx_hash",
        ]
    } else {
        &[]
    };
    let mut p = unexpected_differences(
        &f_d,
        &d,
        &[
            &[
                "qualification_note",
                "contract.path",
                "contract.sha256",
                "bundle_projection.send_head",
                "bundle_projection.max_block",
                "bundle_projection.blockrazor_payload_sha256",
                "bundle_projection.fortyeightclub_payload_sha256",
            ][..],
            b_identity,
        ]
        .concat(),
        &[],
    );
    match one(&d, "qualification_note") {
        Some(Value::Text(note)) if note == "eligible for DRY-1/DRY-4 qualification if all other predicates pass" || note == darkbot_bsc_qualification::DRY_STRUCTURAL_SCOPE_NOTE => {},
        _ => p.push("unexpected DRY structural scope note".into()),
    }
    if one(&d, "contract.path") != Some(Value::Text(V3_RELATIVE_PATH.into()))
        || word(one(&d, "contract.sha256")) != Some(v3_sha)
    {
        p.push("the DRY was not evaluated against the committed v3 proposal".into());
    }
    let launch = word(one(&d, "transactions.launch_tx_hash"));
    let buy = word(one(&d, "transactions.buy_tx_hash"));
    if !b && (launch != Some(hex32(LAUNCH_TX)) || buy != Some(hex32(BUY_TX))) {
        p.push("signed launch/BUY hashes differ from the approved candidate".into());
    }
    if b && (launch.is_none()
        || buy.is_none()
        || launch == Some(hex32(LAUNCH_TX))
        || buy == Some(hex32(BUY_TX)))
    {
        p.push("B must sign its own launch/BUY bytes, never A's".into());
    }
    if one(&d, "dry_pass.DRY_1_bytes_and_signers") != Some(Value::Bool(true)) {
        p.push("DRY-1 did not pass".into());
    }
    if one(&ls, "launcher") != one(&d, "wallets.launcher") || one(&ls, "buyer") != one(&d, "wallets.buyer") {
        p.push("simulation signers differ from the DRY wallets".into());
    }
    facts
        .set_opt(rq::LAUNCH_TX_HASH, launch.map(Value::B256))
        .set_opt(rq::BUY_TX_HASH, buy.map(Value::B256))
        .set_opt(rq::PORTAL_VERSION, one(&pre, "portal_version"));
    checks.push(check(
        "DRY1_BYTES_AND_SIGNERS",
        p,
        "signed launch/BUY hashes, signers, token, values and limits identical to the frozen DRY",
    ));

    // DRY-2: independent EVM and exact local simulation on the new code.
    let mut p = unexpected_differences(
        &f_ls,
        &ls,
        &[
            &[
                "state_source.block_number",
                "state_source.block_hash",
                "launch_execution_gas",
                "buy_execution_gas",
                "launch_gas_used",
                "buy_gas_used",
                "builder_floor_evidence_sha256",
            ][..],
            if b {
                &["launch_tx_hash", "buy_tx_hash", "token"][..]
            } else {
                &[][..]
            },
        ]
        .concat(),
        &[],
    );
    if word(one(&ls, "builder_floor_evidence_sha256")) != Some(policy_sha) {
        p.push("builder floor evidence of the simulation is not the re-determined builder policy".into());
    }
    // B's private record: its token and the salt commitment instead of the
    // salt and the remote checks, which a secret salt never makes.
    let (econ_allowed, econ_absent): (&[&str], &[&str]) = if b {
        (
            &["token", "salt_commitment", "network_actions", "limitations"],
            &[
                "schema",
                "salt",
                "salt_counter",
                "mainnet_estimate_gas_launch_success",
                "mainnet_eth_call_launch_success",
                "mainnet_eth_call_launch_return",
                "mainnet_eth_call_predicted_token_match",
            ],
        )
    } else {
        (&[], &[])
    };
    p.extend(unexpected_differences(
        &f_econ,
        &econ,
        &[
            &[
                "preflight_path",
                "state_source.block_number",
                "state_source.block_hash",
                "mainnet_launch_gas_estimate",
                "mainnet_launch_gas_limit_120pct",
                "launch_execution_gas",
                "buy_execution_gas",
            ][..],
            econ_allowed,
        ]
        .concat(),
        econ_absent,
    ));
    for (name, r) in [("simulation", &ls), ("cross-check", &econ)] {
        if int(one(r, "state_source.block_number")) != Some(head) {
            p.push(format!("{name} state is not the preflight head"));
        }
    }
    if one(&d, "dry_pass.DRY_2_independent_local_simulation") != Some(Value::Bool(true)) {
        p.push("DRY-2 did not pass".into());
    }
    checks.push(check(
        "DRY2_SIMULATION",
        p,
        "exact launch->BUY simulation and independent EVM sequence on the current Portal code: same token, salt, output and success",
    ));

    // DRY-3: gas, economics, slippage, budget.
    let mut p = Vec::new();
    if word(one(&q, "expected_output")) != word(one(&f_d, "transactions.local_expected_output"))
        || word(one(&q, "min_output")) != word(one(&f_d, "transactions.min_output"))
    {
        p.push("quote output or minimum output differs from the frozen candidate".into());
    }
    if word(one(&q, "preflight_record_sha256")) != Some(pre_sha) {
        p.push("the quote was not computed from this preflight".into());
    }
    let limit = |r: &Record, n: &str| int(one(r, n)).unwrap_or(0);
    let (lg, bg) = (limit(&d, "transactions.launch_gas_limit"), limit(&d, "transactions.buy_gas_limit"));
    let (lu, bu) = (limit(&ls, "launch_gas_used"), limit(&ls, "buy_gas_used"));
    if lu > lg || bu > bg || limit(&econ, "mainnet_launch_gas_estimate") > lg {
        p.push(format!("gas exceeds the signed limits (launch {lu}/{lg}, BUY {bu}/{bg})"));
    }
    let value = word(one(&d, "transactions.buy_value_wei")).and_then(u128_of).unwrap_or(u128::MAX);
    let cost = u128::from(lu + bu)
        .saturating_mul(u128::from(SIGNED_GAS_PRICE))
        .saturating_add(value);
    if cost > MAX_TOTAL_PER_ATTEMPT {
        p.push(format!("simulated cost {cost} exceeds the attempt ceiling"));
    }
    if word(one(&d, "budget.worst_case_reserved_wei")) != Some(u256_from_u128(MAX_TOTAL_PER_ATTEMPT)) {
        p.push("worst-case reserve differs from the approved per-attempt total".into());
    }
    if one(&d, "dry_pass.DRY_3_gas_floor_and_budget") != Some(Value::Bool(true)) {
        p.push("DRY-3 did not pass".into());
    }
    if one(&policy, "dry3_ready") != Some(Value::Bool(true)) {
        p.push("the bound builder policy is not DRY-3 ready".into());
    }
    let floor = word(one(&policy, "strictest_explicit_verified_floor_wei")).and_then(u128_of);
    if floor != Some(u128::from(SIGNED_GAS_PRICE))
        || word(one(&policy, "stage1_signed_gas_price_policy_wei")).and_then(u128_of)
            != Some(u128::from(SIGNED_GAS_PRICE))
    {
        p.push(format!("re-determined builder floor {floor:?} or stage-1 policy is not the signed {SIGNED_GAS_PRICE} wei"));
    }
    let sources = policy.nested_all(builder_policy::SOURCES).map_err(|e| e.0)?;
    if sources.is_empty() || sources.iter().any(|s| s.u64(builder_policy::source::HTTP_STATUS).ok() != Some(200)) {
        p.push("the builder policy carries no complete set of fetched sources".into());
    }
    checks.push(check(
        "DRY3_GAS_ECONOMICS_BUDGET",
        p,
        "quote and minimum output equal the candidate; gas within the signed limits; simulated cost and worst-case reserve within the approved budget; builder floor re-determined from fetched sources and unchanged",
    ));

    // DRY-4: expiry, nonces, single attempt, zero egress.
    let mut p = Vec::new();
    let send_head = int(one(&d, "bundle_projection.send_head"));
    if send_head != Some(head) || int(one(&d, "bundle_projection.max_block")) != Some(head + 2) {
        p.push("expiry projection is not preflight head .. head+2".into());
    }
    for (name, r) in [("preflight", &pre), ("frozen", &f_pre)] {
        if int(one(r, "launcher_pending_nonce")) != Some(0) || int(one(r, "buyer_pending_nonce")) != Some(0) {
            p.push(format!("{name} nonces are not 0/0"));
        }
    }
    for (name, expected) in [
        ("dry_pass.DRY_4_expiry_nonces_zero_egress", Value::Bool(true)),
        ("dry_pass.zero_egress_structural", Value::Bool(true)),
        ("dry_pass.all", Value::Bool(true)),
        ("network_write_capability", Value::Text("NONE_IN_THIS_BINARY".into())),
        ("contract.approved", Value::Bool(false)),
        ("contract.execution_ready", Value::Bool(false)),
        ("contract.compiled_live_hash", Value::Text("UNARMED".into())),
    ] {
        if one(&d, name) != Some(expected) {
            p.push(format!("{name} is not as required"));
        }
    }
    for (name, r) in [("simulation", &ls), ("cross-check", &econ)] {
        for field in [
            "state_source.valid_signed_tx_bytes_submitted",
            "signed_transactions_submitted",
            "bundle_submitted",
            "raw_signed_transactions_persisted",
            "raw_bundle_persisted",
        ] {
            if r.schema().fields.iter().any(|f| f.name == field) && one(r, field) != Some(Value::Bool(false)) {
                p.push(format!("{name} {field} is not false"));
            }
        }
    }
    checks.push(check(
        "DRY4_EXPIRY_NONCES_ZERO_EGRESS",
        p,
        "expiry head..head+2, nonces 0/0, single attempt under the UNARMED proposal, nothing submitted or persisted raw",
    ));

    // DRY-5 (v9.1 C1/R1): the launch-path code identity of this run.
    let lp_path = dir.join(LAUNCH_PATH);
    let lp_bytes = std::fs::read(&lp_path).map_err(|e| format!("{}: {e}", lp_path.display()))?;
    let mut p = Vec::new();
    match darkbot_bsc_execution::launch_path::check(&lp_bytes, &pre) {
        Ok(snapshot) if Some(snapshot.token) == addr(one(&d, "transactions.token")) => {}
        Ok(_) => p.push("the launch path traces another token than the DRY".into()),
        Err(e) => p.push(e),
    }
    checks.push(check(
        "DRY5_LAUNCH_PATH",
        p,
        "every executed contract's code hash and every storage selector of the candidate's launch and first BUY, traced at the preflight's block, bound to its Portal, implementation and Tax-V3 code",
    ));

    // DRY-6 (B only): this run is exactly the SELF_B candidate's.
    if let Some((c, _)) = &candidate {
        let mut p = Vec::new();
        let id = darkbot_bsc_record::schema::g07_candidate::candidate_id_of(c).map_err(|e| e.0)?;
        if word(one(c, "candidate_id")) != Some(id) {
            p.push("the candidate record fails its identity".into());
        }
        for (field, record, name) in [
            ("portal", &pre, "portal"),
            ("deployment.portal_version", &pre, "portal_version"),
            (
                "deployment.portal_implementation",
                &pre,
                "portal_implementation",
            ),
            (
                "deployment.portal_implementation_code_sha256",
                &pre,
                "portal_implementation_code_sha256",
            ),
            ("deployment.tax_v3_implementation", &pre, "tax_v3_impl"),
            (
                "deployment.tax_v3_implementation_code_sha256",
                &pre,
                "tax_v3_impl_code_sha256",
            ),
            ("launcher", &d, "wallets.launcher"),
            ("token", &d, "transactions.token"),
            ("token", &ls, "token"),
            ("token", &econ, "token"),
            ("salt_commitment", &econ, "salt_commitment"),
        ] {
            if one(c, field).is_none() || one(c, field) != one(record, name) {
                p.push(format!("candidate {field} differs from {name}"));
            }
        }
        if one(c, "qualification") != Some(Value::Text("UNQUALIFIED".into()))
            || one(c, "evidence.dry_record_sha256").is_some()
        {
            p.push("the candidate record is not an unqualified SELF_B record".into());
        }
        checks.push(check(
            "DRY6_B_CANDIDATE",
            p,
            "the SELF_B candidate (ID recomputed) is of this preflight's deployment; its launcher, token and salt commitment are this run's DRY, simulation and private cross-check",
        ));
    }

    let bound = vec![
        ("preflight.dbrec".to_string(), pre_sha),
        ("native_quote.dbrec".to_string(), q_sha),
        ("economics_crosscheck.dbrec".to_string(), econ_sha),
        ("local_simulation.dbrec".to_string(), ls_sha),
        ("dry_result.dbrec".to_string(), d_sha),
        ("portal_upgrade.dbrec".to_string(), scan_sha),
        (policy_path.display().to_string(), policy_sha),
        (LAUNCH_PATH.to_string(), sha256(&lp_bytes)),
    ];
    let mut bound = bound;
    if let (Some(path), Some((_, sha))) = (candidate_path, &candidate) {
        bound.push((path.display().to_string(), *sha));
    }
    let _ = (IMPORT_LEGACY_PATH, IMPORT_LEGACY_SHA256, tag_of);
    Ok(Evaluation { checks, bound, facts })
}

fn bounded_detail(text: &str) -> &str {
    let mut end = text.len().min(4000);
    while !text.is_char_boundary(end) { end -= 1; }
    &text[..end]
}

fn status_record(dir: &Path, frozen: &Path, e: Evaluation) -> (bool, Vec<u8>) {
    let pass = e.checks.iter().all(|c| c.pass);
    let mut d = e.facts;
    let observed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_millis() as u64)
        .unwrap_or(0);
    d.set(PROVENANCE_TOOL, "g06a_requal_check")
        .set(PROVENANCE_OBSERVED_UNIX_MS, observed)
        .set(rq::VERDICT, if pass { "PASS" } else { "STOP" })
        .set(rq::REQUAL_DIR, dir.display().to_string())
        .set(rq::FROZEN_DIR, frozen.display().to_string());
    for c in &e.checks {
        let mut row = Draft::new(&rq::check::SCHEMA);
        row.set(rq::check::NAME, c.name)
            .set(rq::check::STATUS, if c.pass { "PASS" } else { "STOP" })
            .set(rq::check::DETAIL, bounded_detail(&c.detail));
        d.push(rq::CHECKS, row.into_nested());
    }
    for (name, sha) in &e.bound {
        let mut row = Draft::new(&rq::bound::SCHEMA);
        row.set(rq::bound::NAME, name.as_str()).set(rq::bound::SHA256, Value::B256(*sha));
        d.push(rq::RECORDS, row.into_nested());
    }
    (pass, d.encode().expect("encode requalification status"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, frozen, policy, candidate) = match args.as_slice() {
        [dir, frozen, policy] => (dir, frozen, policy, None),
        [dir, frozen, policy, flag, candidate] if flag == "--candidate" => {
            (dir, frozen, policy, Some(Path::new(candidate)))
        }
        _ => {
            eprintln!(
                "usage: g06a_requal_check <requal_dir> <frozen_dir> <builder_policy.dbrec> [--candidate <b.candidate.dbrec>]"
            );
            return ExitCode::from(2);
        }
    };
    let (dir, frozen, policy) = (Path::new(dir), Path::new(frozen), Path::new(policy));
    let evaluation = match evaluate(dir, frozen, policy, candidate) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("STOP: {e}");
            println!("FLAP_REQUALIFICATION=STOP");
            return ExitCode::from(2);
        }
    };
    let (pass, bytes) = status_record(dir, frozen, evaluation);
    let out = dir.join("requalification_status.dbrec");
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&out)
        .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()));
    if let Err(e) = written {
        eprintln!("STOP: write {}: {e}", out.display());
        println!("FLAP_REQUALIFICATION=STOP");
        return ExitCode::from(2);
    }
    let record = darkbot_bsc_record::decode(&rq::SCHEMA, &bytes).expect("decode status");
    print!("{}", record.render());
    println!("STATUS_SHA256={}", sha256_hex(&bytes));
    println!("FLAP_REQUALIFICATION={}", if pass { "PASS" } else { "STOP" });
    let _ = hex;
    if pass { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v9.1 D5: the A hashes this check pins are the frozen DRY's.
    #[test]
    fn the_pinned_a_signed_hashes_are_the_frozen_dry_hashes() {
        let d = frozen("dry_result.dbrec");
        assert_eq!(d.b256(dry::LAUNCH_TX_HASH).unwrap(), hex32(LAUNCH_TX));
        assert_eq!(d.b256(dry::BUY_TX_HASH).unwrap(), hex32(BUY_TX));
    }

    #[test]
    fn utf8_status_details_are_byte_bounded_and_preserve_stop() {
        for detail in ["a".repeat(4000), "a".repeat(4001), "é".repeat(4000), "€".repeat(4000), "🦀".repeat(4000), format!("{}€🦀", "a".repeat(3999))] {
            let evaluation = Evaluation { checks: vec![Check { name: "offline-test", pass: false, detail }], bound: vec![], facts: Draft::new(&rq::SCHEMA) };
            let (pass, bytes) = status_record(Path::new("offline-test"), Path::new("offline-test"), evaluation);
            let record = darkbot_bsc_record::decode(&rq::SCHEMA, &bytes).unwrap();
            assert!(!pass);
            assert_eq!(record.text(rq::VERDICT).unwrap(), "STOP");
            assert!(record.nested_all(rq::CHECKS).unwrap()[0].text(rq::check::DETAIL).unwrap().len() <= 4000);
        }
    }

    /// The frozen import, relative to the repository root.
    const FROZEN: &str = "bsc/native/evidence/g06a/g07_binary_import_20261016";

    fn repo(rel: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..").join(rel)
    }

    fn frozen(name: &str) -> Record {
        let bytes = std::fs::read(repo(FROZEN).join(name)).unwrap();
        decode_any(&bytes).unwrap()
    }

    /// A native copy of a frozen import record with `edit` applied.
    fn native(import: &str, to: &'static Schema, edit: impl FnOnce(&mut Draft)) -> Vec<u8> {
        let r = frozen(import);
        let mut d = Draft::new(to);
        for (tag, v) in r.fields() {
            if *tag > 2 {
                d.push(*tag, v.clone());
            }
        }
        if to.field(PROVENANCE_TOOL).is_some() {
            d.set(PROVENANCE_TOOL, "test").set(PROVENANCE_OBSERVED_UNIX_MS, 1u64);
        }
        edit(&mut d);
        d.encode().unwrap()
    }

    const HEAD: u64 = 128_200_000;
    const HEAD_HASH: [u8; 32] = [0x77; 32];
    const NEW_IMPL: [u8; 20] = [0xe1; 20];

    /// Stand-in requalification records consistent with the frozen DRY and a
    /// single upgrade (or the variant's chain); `breaks` names one deliberate
    /// defect.
    fn stand_in(tag: &str, breaks: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("requal-check-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f_pre = frozen("preflight.dbrec");
        let old_impl = f_pre.address(pf::PORTAL_IMPLEMENTATION).unwrap();
        let code = [0x42; 32];
        let pre = native("preflight.dbrec", &pf::SCHEMA, |d| {
            d.clear(tag_of(&pf::SCHEMA, "schema"))
                .clear(tag_of(&pf::SCHEMA, "tax_v3_impl_source"))
                .clear(tag_of(&pf::SCHEMA, "dependencies"))
                .set(pf::LATEST_BLOCK_NUMBER, HEAD)
                .set(pf::LATEST_BLOCK_HASH, Value::B256(HEAD_HASH))
                .set(pf::PORTAL_VERSION, if breaks == "version" { "v5.25.0" } else { TARGET_VERSION })
                .set(pf::PORTAL_IMPLEMENTATION, Value::Address(NEW_IMPL))
                .set(pf::PORTAL_IMPLEMENTATION_CODE_SHA256, Value::B256(code));
            if breaks == "fee" {
                d.set(pf::FEE_BUY_BPS, Value::u128(101));
            }
        });
        std::fs::write(dir.join("preflight.dbrec"), &pre).unwrap();
        let f_d = frozen("dry_result.dbrec");
        let mut q = Draft::new(&quote::SCHEMA);
        for f in quote::SCHEMA.fields {
            let v = match f.kind {
                darkbot_bsc_record::Kind::Text => Value::Text("x".into()),
                darkbot_bsc_record::Kind::U64 => Value::U64(1),
                darkbot_bsc_record::Kind::Bool => Value::Bool(true),
                darkbot_bsc_record::Kind::U256 => Value::u128(1),
                _ => Value::B256([0; 32]),
            };
            q.set(f.tag, v);
        }
        q.set(
            quote::EXPECTED_OUTPUT,
            f_d.get(dry::LOCAL_EXPECTED_OUTPUT).unwrap().clone(),
        )
        .set(
            quote::MIN_OUTPUT,
            if breaks == "min_output" {
                Value::u128(1)
            } else {
                f_d.get(dry::MIN_OUTPUT).unwrap().clone()
            },
        )
        .set(
            quote::PREFLIGHT_RECORD_SHA256,
            Value::B256(if breaks == "h1_quote" {
                [6; 32]
            } else {
                sha256(&pre)
            }),
        );
        std::fs::write(dir.join("native_quote.dbrec"), q.encode().unwrap()).unwrap();
        let state = |d: &mut Draft, schema: &'static Schema| {
            d.set(tag_of(schema, "state_source.block_number"), HEAD);
        };
        std::fs::write(
            dir.join("economics_crosscheck.dbrec"),
            native("economics_crosscheck.dbrec", &economics::NATIVE, |d| {
                state(d, &economics::NATIVE);
                if breaks == "salt" {
                    d.set(tag_of(&economics::NATIVE, "salt"), Value::B256([9; 32]));
                }
                // v9.1 D4: H2 curve state changed without any drift field,
                // or an H1 cross-check carried into the H2 run.
                if breaks == "h2_curve" {
                    d.set(
                        tag_of(&economics::NATIVE, "simulated_received_amount"),
                        Value::u128(1),
                    );
                }
                if breaks == "h1_econ" {
                    d.set(
                        tag_of(&economics::NATIVE, "state_source.block_number"),
                        HEAD - 1,
                    );
                }
            }),
        )
        .unwrap();
        let policy = dir.join("builder_policy.dbrec");
        std::fs::write(
            &policy,
            native("builder_policy.dbrec", &builder_policy::NATIVE, |d| {
                let mut src = Draft::new(&builder_policy::source::SCHEMA);
                src.set(builder_policy::source::URL, "https://example.test/")
                    .set(builder_policy::source::HTTP_STATUS, 200u64)
                    .set(builder_policy::source::FETCHED_UNIX_MS, 1u64)
                    .set(builder_policy::source::BODY_SHA256, Value::B256([5; 32]))
                    .set(builder_policy::source::BODY_BYTES, 1u64)
                    .set(builder_policy::source::FINDING, "test");
                d.push(builder_policy::SOURCES, src.into_nested());
                if breaks == "floor" {
                    d.set(builder_policy::STRICTEST_FLOOR_WEI, Value::u128(100_000_000));
                }
            }),
        )
        .unwrap();
        std::fs::write(
            dir.join("local_simulation.dbrec"),
            native("local_simulation.dbrec", &sim::NATIVE, |d| {
                state(d, &sim::NATIVE);
                let sha = sha256(&std::fs::read(&policy).unwrap());
                d.set(tag_of(&sim::NATIVE, "builder_floor_evidence_sha256"), Value::B256(sha));
            }),
        )
        .unwrap();
        let v3 = sha256(&std::fs::read(repo(V3_RELATIVE_PATH)).unwrap());
        std::fs::write(
            dir.join("dry_result.dbrec"),
            native("dry_result.dbrec", &dry::NATIVE, |d| {
                d.set(tag_of(&dry::NATIVE, "contract.path"), V3_RELATIVE_PATH)
                    .set(tag_of(&dry::NATIVE, "contract.sha256"), Value::B256(v3))
                    .set(tag_of(&dry::NATIVE, "bundle_projection.send_head"), HEAD)
                    .set(tag_of(&dry::NATIVE, "bundle_projection.max_block"), HEAD + 2);
                if breaks == "scope_note" {
                    d.set(tag_of(&dry::NATIVE, "qualification_note"), darkbot_bsc_qualification::DRY_STRUCTURAL_SCOPE_NOTE);
                }
                if breaks == "bad_note" {
                    d.set(tag_of(&dry::NATIVE, "qualification_note"), "unqualified claim");
                }
                if breaks == "launch" {
                    d.set(dry::LAUNCH_TX_HASH, Value::B256([7; 32]));
                }
            }),
        )
        .unwrap();
        let mut s = Draft::new(&up::SCHEMA);
        // (block offset, slot before, event target, slot after) per upgrade.
        type Step = (u64, [u8; 20], [u8; 20], [u8; 20]);
        const MID: [u8; 20] = [0xd5; 20];
        let steps: Vec<Step> = match breaks {
            "zero" => vec![],
            "two" => vec![(1000, old_impl, NEW_IMPL, NEW_IMPL), (1001, old_impl, NEW_IMPL, NEW_IMPL)],
            "ten" => (0..10).map(|n| (1000 + n, old_impl, NEW_IMPL, NEW_IMPL)).collect(),
            "chain" => vec![(1000, old_impl, MID, MID), (1001, MID, NEW_IMPL, NEW_IMPL)],
            "chain_gap" => vec![(1000, old_impl, MID, MID), (1001, [0xd6; 20], NEW_IMPL, NEW_IMPL)],
            "chain_order" => vec![(1001, old_impl, MID, MID), (1000, MID, NEW_IMPL, NEW_IMPL)],
            "chain_short" => vec![(1000, old_impl, MID, MID)],
            "target" => vec![(1000, old_impl, MID, NEW_IMPL)],
            _ => vec![(1000, old_impl, NEW_IMPL, NEW_IMPL)],
        };
        for (offset, before, target, after) in steps {
            let mut u = Draft::new(&up::upgrade::SCHEMA);
            u.set(up::upgrade::BLOCK, FROZEN_HEAD + offset)
                .set(up::upgrade::BLOCK_HASH, Value::B256([3; 32]))
                .set(up::upgrade::TRANSACTION_HASH, Value::B256([4; 32]))
                .set(up::upgrade::LOG_INDEX, 1u64)
                .set(up::upgrade::IMPLEMENTATION, Value::Address(target))
                .set(up::upgrade::RECEIPT_STATUS, if breaks == "receipt" { 0u64 } else { 1u64 })
                .set(up::upgrade::SLOT_BEFORE, Value::Address(before))
                .set(up::upgrade::SLOT_AFTER, Value::Address(after));
            s.push(up::UPGRADES, u.into_nested());
        }
        let to = if breaks == "short_scan" { HEAD - 10 } else { HEAD };
        let headers = to - FROZEN_HEAD - u64::from(breaks == "headers");
        s.set(PROVENANCE_TOOL, "test")
            .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(up::PROXY, f_pre.get(pf::PORTAL).unwrap().clone())
            .set(up::SCANNED_FROM_BLOCK, FROZEN_HEAD + 1)
            .set(up::SCANNED_TO_BLOCK, to)
            .set(up::CURRENT_IMPLEMENTATION, Value::Address(NEW_IMPL))
            .set(up::BASELINE_IMPLEMENTATION, Value::Address(old_impl))
            .set(up::COVERAGE_METHOD, if breaks == "method" { "eth_getLogs" } else { up::METHOD })
            .set(up::CHAIN_SOURCE, "test")
            .set(up::ARCHIVE_SOURCE, "test")
            .set(up::HEADERS_VERIFIED, headers)
            .set(up::BLOOM_CANDIDATE_BLOCKS, 7u64)
            .set(up::CURRENT_IMPLEMENTATION_CODE_SHA256, Value::B256(code))
            .set(up::CURRENT_IMPLEMENTATION_CODE_BYTES, f_pre.u64(pf::PORTAL_IMPLEMENTATION_CODE_BYTES).unwrap())
            .set(
                up::FIRST_PARENT_HASH,
                if breaks == "anchor" { Value::B256([1; 32]) } else { f_pre.get(pf::LATEST_BLOCK_HASH).unwrap().clone() },
            )
            .set(up::LAST_BLOCK_HASH, Value::B256(HEAD_HASH));
        std::fs::write(dir.join("portal_upgrade.dbrec"), s.encode().unwrap()).unwrap();
        // DRY-5: a launch path consistent with this preflight.
        use darkbot_bsc_record::schema::g06a::launch_path as lp;
        let p = decode_any(&pre).unwrap();
        let token = if breaks == "lp_token" {
            [0x11; 20]
        } else {
            f_d.address(dry::TOKEN).unwrap()
        };
        let mut l = Draft::new(&lp::SCHEMA);
        l.set(PROVENANCE_TOOL, "test")
            .set(PROVENANCE_OBSERVED_UNIX_MS, 1u64)
            .set(
                lp::BLOCK_NUMBER,
                if breaks == "lp_stale" { HEAD - 1 } else { HEAD },
            )
            .set(lp::BLOCK_HASH, Value::B256(HEAD_HASH))
            .set(lp::PORTAL, p.get(pf::PORTAL).unwrap().clone())
            .set(lp::METHOD, lp::TRACE_METHOD)
            .set(lp::TOKEN, Value::Address(token))
            .set(lp::LAUNCH_SUCCESS, true)
            .set(lp::BUY_SUCCESS, true)
            .set(lp::LAUNCH_RETURN_TOKEN, Value::Address(token));
        for (address, sha) in [
            (pf::PORTAL, pf::PORTAL_CODE_SHA256),
            (
                pf::PORTAL_IMPLEMENTATION,
                pf::PORTAL_IMPLEMENTATION_CODE_SHA256,
            ),
            (pf::TAX_V3_IMPL, pf::TAX_V3_IMPL_CODE_SHA256),
        ] {
            l.push(lp::CODE_ADDRESS, p.get(address).unwrap().clone())
                .push(lp::CODE_SHA256, p.get(sha).unwrap().clone())
                .push(lp::CODE_BYTES, 1u64)
                .push(lp::CODE_ROLES, "launch:CALL");
        }
        let mut selects = [0u8; 32];
        selects[12..].copy_from_slice(&NEW_IMPL);
        l.push(lp::STORAGE_ADDRESS, p.get(pf::PORTAL).unwrap().clone())
            .push(
                lp::STORAGE_SLOT,
                Value::B256(darkbot_bsc_execution::launch_path::EIP1967_IMPLEMENTATION_SLOT),
            )
            .push(lp::STORAGE_VALUE, Value::B256(selects))
            .push(lp::STORAGE_SELECTS, Value::Address(NEW_IMPL));
        std::fs::write(dir.join(LAUNCH_PATH), l.encode().unwrap()).unwrap();
        dir
    }

    /// A SELF_B run: the A stand-ins with B's token and signed hashes, the
    /// private cross-check (salt only as its commitment) and the public B
    /// candidate record derived against the stand-in preflight; `breaks`
    /// names one deliberate defect.
    fn b_stand_in(tag: &str, breaks: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        use darkbot_bsc_record::schema::g06a::launch_path as lp;
        use darkbot_bsc_record::schema::g07_candidate::{candidate as c, candidate_id_of};
        let dir = stand_in(tag, "none");
        let mut token = [0xbb; 20];
        token[18..].copy_from_slice(&[0x77, 0x77]);
        let commitment = [0x5c; 32];
        let edit = |name: &str, schema: &'static Schema, f: &dyn Fn(&mut Draft)| {
            let path = dir.join(name);
            let r = decode_any(&std::fs::read(&path).unwrap()).unwrap();
            let mut d = Draft::new(schema);
            for (t, v) in r.fields() {
                d.push(*t, v.clone());
            }
            f(&mut d);
            std::fs::write(&path, d.encode().unwrap()).unwrap();
        };
        edit("dry_result.dbrec", &dry::NATIVE, &|d| {
            let launch = if breaks == "b_reuses_a" {
                hex32(LAUNCH_TX)
            } else {
                [0x1b; 32]
            };
            d.set(
                dry::TOKEN,
                Value::Address(if breaks == "b_token" {
                    [0xcc; 20]
                } else {
                    token
                }),
            )
            .set(dry::LAUNCH_TX_HASH, Value::B256(launch))
            .set(dry::BUY_TX_HASH, Value::B256([0x2b; 32]));
        });
        edit("local_simulation.dbrec", &sim::NATIVE, &|d| {
            d.set(tag_of(&sim::NATIVE, "token"), Value::Address(token))
                .set(
                    tag_of(&sim::NATIVE, "launch_tx_hash"),
                    Value::B256([0x1b; 32]),
                )
                .set(tag_of(&sim::NATIVE, "buy_tx_hash"), Value::B256([0x2b; 32]));
        });
        if breaks != "b_native_econ" {
            let native =
                decode_any(&std::fs::read(dir.join("economics_crosscheck.dbrec")).unwrap())
                    .unwrap();
            let mut private = Draft::new(&economics::PRIVATE);
            for (t, v) in native.fields() {
                if economics::PRIVATE.field(*t).is_some() {
                    private.push(*t, v.clone());
                }
            }
            private
                .set(tag_of(&economics::PRIVATE, "token"), Value::Address(token))
                .set(economics::SALT_COMMITMENT, Value::B256(commitment));
            std::fs::write(
                dir.join("economics_crosscheck.dbrec"),
                private.encode().unwrap(),
            )
            .unwrap();
        }
        edit("launch_path.dbrec", &lp::SCHEMA, &|d| {
            d.set(lp::TOKEN, Value::Address(token))
                .set(lp::LAUNCH_RETURN_TOKEN, Value::Address(token));
        });
        let pre_bytes = std::fs::read(dir.join("preflight.dbrec")).unwrap();
        let p = decode_any(&pre_bytes).unwrap();
        let f_d = frozen("dry_result.dbrec");
        let mut x = Draft::new(&c::SCHEMA);
        let profile = if breaks == "b_profile_a" {
            "SELF_A"
        } else {
            "SELF_B"
        };
        x.set(c::PROFILE, profile)
            .set(c::CANDIDATE_ID, Value::B256([0; 32]))
            .set(c::CHAIN_ID, 56u64)
            .set(c::PORTAL, p.get(pf::PORTAL).unwrap().clone())
            .set(
                c::PORTAL_VERSION,
                p.get(pf::PORTAL_VERSION).unwrap().clone(),
            )
            .set(
                c::PORTAL_IMPLEMENTATION,
                p.get(pf::PORTAL_IMPLEMENTATION).unwrap().clone(),
            )
            .set(
                c::PORTAL_IMPLEMENTATION_CODE_SHA256,
                p.get(pf::PORTAL_IMPLEMENTATION_CODE_SHA256)
                    .unwrap()
                    .clone(),
            )
            .set(
                c::TAX_V3_IMPLEMENTATION,
                p.get(pf::TAX_V3_IMPL).unwrap().clone(),
            )
            .set(
                c::TAX_V3_IMPLEMENTATION_CODE_SHA256,
                p.get(pf::TAX_V3_IMPL_CODE_SHA256).unwrap().clone(),
            )
            .set(
                c::DEPLOYMENT_RECORD_SHA256,
                Value::B256(if breaks == "b_h1_candidate" {
                    [0x41; 32]
                } else {
                    sha256(&pre_bytes)
                }),
            )
            .set(c::LAUNCHER, f_d.get(dry::LAUNCHER).unwrap().clone())
            .set(c::TOKEN, Value::Address(token))
            .set(
                c::SALT_COMMITMENT,
                Value::B256(if breaks == "b_commitment" {
                    [0x5d; 32]
                } else {
                    commitment
                }),
            )
            .set(c::SALT_ORIGIN, "OS_CSPRNG")
            .set(c::LAUNCH_CALLDATA_SHA256, Value::B256([0x5b; 32]))
            .set(c::QUALIFICATION, "UNQUALIFIED")
            .set(c::PROVENANCE, "test")
            .set(c::CREATED_UNIX_MS, 1u64);
        let id = candidate_id_of(&decode_any(&x.encode().unwrap()).unwrap()).unwrap();
        x.set(
            c::CANDIDATE_ID,
            Value::B256(if breaks == "b_id" { [1; 32] } else { id }),
        );
        let candidate = dir.join("b.candidate.dbrec");
        std::fs::write(&candidate, x.encode().unwrap()).unwrap();
        (dir, candidate)
    }

    #[test]
    fn a_secret_salt_b_run_qualifies_only_as_its_own_candidate() {
        std::env::set_current_dir(repo("")).unwrap();
        let frozen = repo(FROZEN);
        let b_verdict = |breaks: &str| {
            let (dir, candidate) = b_stand_in(&format!("b-{breaks}"), breaks);
            let result = evaluate(&dir, &frozen, &policy_of(&dir), Some(&candidate)).map(|e| {
                let checks = e.checks.clone();
                (status_record(&dir, &frozen, e).0, checks)
            });
            std::fs::remove_dir_all(&dir).unwrap();
            result
        };
        let (pass, checks) = b_verdict("none").unwrap();
        assert!(pass, "{checks:#?}");
        // Created at an earlier preflight of the same deployment (H1 -> H2).
        let (pass, checks) = b_verdict("b_h1_candidate").unwrap();
        assert!(pass, "{checks:#?}");
        assert!(
            checks
                .iter()
                .any(|c| c.name == "DRY6_B_CANDIDATE" && c.pass)
        );
        for (breaks, failing) in [
            ("b_token", "DRY6_B_CANDIDATE"),
            ("b_reuses_a", "DRY1_BYTES_AND_SIGNERS"),
            ("b_commitment", "DRY6_B_CANDIDATE"),
            ("b_id", "DRY6_B_CANDIDATE"),
        ] {
            let (pass, checks) = b_verdict(breaks).unwrap();
            assert!(!pass, "{breaks} passed");
            let failed: Vec<_> = checks.iter().filter(|c| !c.pass).map(|c| c.name).collect();
            assert!(failed.contains(&failing), "{breaks}: {failed:?}");
        }
        // A's candidate is not a B run; a cross-check with the salt in clear
        // is unusable for B.
        assert!(b_verdict("b_profile_a").is_err());
        assert!(b_verdict("b_native_econ").is_err());
        // Without --candidate, B's files are a STOP of A's requalification.
        let (dir, _) = b_stand_in("b-as-a", "b_native_econ");
        let e = evaluate(&dir, &frozen, &policy_of(&dir), None).unwrap();
        assert!(!status_record(&dir, &frozen, e).0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn policy_of(dir: &Path) -> std::path::PathBuf {
        dir.join("builder_policy.dbrec")
    }

    fn verdict(breaks: &str) -> (bool, Vec<Check>) {
        let dir = stand_in(breaks, breaks);
        let root = repo("");
        std::env::set_current_dir(&root).unwrap();
        let frozen = repo(FROZEN);
        let policy = dir.join("builder_policy.dbrec");
        let e = evaluate(&dir, &frozen, &policy, None).unwrap();
        let checks = e.checks.clone();
        let (pass, _) = status_record(&dir, &frozen, e);
        std::fs::remove_dir_all(&dir).unwrap();
        (pass, checks)
    }

    #[test]
    fn consistent_requalification_records_pass_and_every_defect_stops() {
        for variant in ["none", "scope_note", "chain"] {
            let (pass, checks) = verdict(variant);
            assert!(pass, "{checks:#?}");
        }
        for (breaks, failing) in [
            ("zero", "R0_UPGRADE"),
            ("two", "R0_UPGRADE"),
            ("ten", "R0_UPGRADE"),
            ("chain_gap", "R0_UPGRADE"),
            ("chain_order", "R0_UPGRADE"),
            ("chain_short", "R0_UPGRADE"),
            ("target", "R0_UPGRADE"),
            ("receipt", "R0_UPGRADE"),
            ("short_scan", "R0_UPGRADE"),
            ("headers", "R0_UPGRADE"),
            ("anchor", "R0_UPGRADE"),
            ("method", "R0_UPGRADE"),
            ("floor", "DRY3_GAS_ECONOMICS_BUDGET"),
            ("version", "DRY0_PREFLIGHT"),
            ("fee", "DRY0_PREFLIGHT"),
            ("launch", "DRY1_BYTES_AND_SIGNERS"),
            ("bad_note", "DRY1_BYTES_AND_SIGNERS"),
            ("salt", "DRY2_SIMULATION"),
            ("min_output", "DRY3_GAS_ECONOMICS_BUDGET"),
            ("lp_stale", "DRY5_LAUNCH_PATH"),
            ("h2_curve", "DRY2_SIMULATION"),
            ("h1_econ", "DRY2_SIMULATION"),
            ("h1_quote", "DRY3_GAS_ECONOMICS_BUDGET"),
            ("lp_token", "DRY5_LAUNCH_PATH"),
        ] {
            let (pass, checks) = verdict(breaks);
            assert!(!pass, "{breaks} passed");
            let failed: Vec<_> = checks.iter().filter(|c| !c.pass).map(|c| c.name).collect();
            assert!(failed.contains(&failing), "{breaks}: {failed:?}");
        }
    }

    #[test]
    fn an_unreadable_or_foreign_record_is_unusable_input() {
        let dir = stand_in("foreign", "none");
        std::env::set_current_dir(repo("")).unwrap();
        let frozen = repo(FROZEN);
        let policy = dir.join("builder_policy.dbrec");
        // The frozen import is history, not a re-determination.
        assert!(evaluate(&dir, &frozen, &frozen.join("builder_policy.dbrec"), None).is_err());
        std::fs::copy(frozen.join("dry_result.dbrec"), dir.join("preflight.dbrec")).unwrap();
        assert!(evaluate(&dir, &frozen, &policy, None).is_err());
        std::fs::write(dir.join("preflight.dbrec"), b"garbage").unwrap();
        assert!(evaluate(&dir, &frozen, &policy, None).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
        // Without its launch-path snapshot a run is unusable for v5.
        let dir = stand_in("no-launch-path", "none");
        std::fs::remove_file(dir.join(LAUNCH_PATH)).unwrap();
        let error = evaluate(&dir, &frozen, &policy_of(&dir), None)
            .err()
            .unwrap();
        assert!(error.contains(LAUNCH_PATH), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
