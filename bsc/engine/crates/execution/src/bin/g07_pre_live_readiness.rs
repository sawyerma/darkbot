//! Native read-only readiness. Default: zero network egress, no secrets,
//! signatures, latch creation or probe. --current-rpc explicitly enables only
//! the existing read-only preflight/head logic; never a builder submission.
#![allow(dead_code, unused_imports)]
#[path = "g07_mainnet_self_launch_buy/builder_submission.rs"]
mod builder_submission;
#[path = "g07_candidate/candidate.rs"]
mod candidate;
#[path = "g07_mainnet_self_launch_buy/chain_reads.rs"]
mod chain_reads;
#[path = "g07_mainnet_self_launch_buy/inputs.rs"]
mod inputs;
#[path = "g07_mainnet_self_launch_buy/shared.rs"]
mod shared;
#[path = "g07_mainnet_self_launch_buy/transactions.rs"]
mod transactions;
use builder_submission::*;
use chain_reads::*;
use darkbot_bsc_execution::acceptance_guard::{
    FROZEN_APPROVED_V5_SHA256, V5_RELATIVE_PATH, inspect_v5, verify_further_v5_evidence,
};
use darkbot_bsc_execution::builder_policy;
use darkbot_bsc_execution::launch_path::Current;
use darkbot_bsc_record::{
    Value, decode,
    schema::g07::{contract as c, readiness as r},
};
use inputs::*;
use shared::*;
use transactions::*;

fn check(report: &mut Draft, name: &str, status: &str, detail: &str) {
    let mut row = Draft::new(&r::check::SCHEMA);
    row.set(r::check::NAME, name)
        .set(r::check::STATUS, status)
        .set(r::check::DETAIL, detail);
    report.push(r::CHECKS, row.into_nested());
}

/// A record value as text for a readiness detail.
fn shown(value: Option<&Value>) -> String {
    match value {
        None => "absent".into(),
        Some(Value::Text(t)) => t.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::U64(n)) => n.to_string(),
        Some(Value::U256(w)) => darkbot_bsc_record::u256_to_decimal(w),
        Some(Value::Address(a)) => format!("0x{}", darkbot_bsc_record::hex(a)),
        Some(Value::B256(w)) => format!("0x{}", darkbot_bsc_record::hex(w)),
        Some(other) => format!("{other:?}"),
    }
}

/// Every deployment/economics field of the current preflight that differs
/// from the frozen DRY baseline, as `name: frozen -> current`.
fn deployment_drift(baseline: &Record, current: &Record) -> Vec<String> {
    use schema::g06a::preflight as f;
    f::DRIFT_CHECKED
        .iter()
        .filter(|tag| baseline.get(**tag) != current.get(**tag))
        .map(|tag| {
            format!(
                "{}: {} -> {}",
                f::SCHEMA.field(*tag).map_or("?", |field| field.name),
                shown(baseline.get(*tag)),
                shown(current.get(*tag))
            )
        })
        .collect()
}

/// A bound input of the v5 contract: decoded against one of the accepted
/// schemas, its record SHA-256 equal to the binding and, where the contract
/// binds a legacy hash (an audited import), the carried legacy hash and the
/// immutable source file equal to it; a natively bound input must not be an
/// import.
fn bound_input(
    contract: &Record,
    path_tag: u16,
    hash_tag: u16,
    legacy_tag: Option<u16>,
    accepted: &[&'static darkbot_bsc_record::Schema],
) -> Result<Record, String> {
    let e = |x: darkbot_bsc_record::RecordError| x.to_string();
    let path = contract.text(path_tag).map_err(e)?;
    let bytes = fs::read(path).map_err(|x| format!("{path}: {x}"))?;
    let record = schema::decode_any(&bytes).map_err(e)?;
    if !accepted.contains(&record.schema()) {
        return Err(format!(
            "{path}: {} is not accepted here",
            record.schema().name
        ));
    }
    if darkbot_bsc_record::sha256(&bytes) != contract.b256(hash_tag).map_err(e)? {
        return Err(format!("record hash binding mismatch: {path}"));
    }
    let carried = if record
        .schema()
        .field(schema::IMPORT_LEGACY_SHA256)
        .is_some()
    {
        record.opt_b256(schema::IMPORT_LEGACY_SHA256).map_err(e)?
    } else {
        None
    };
    let bound = match legacy_tag {
        Some(tag) => contract.opt_b256(tag).map_err(e)?,
        None => None,
    };
    match (bound, carried) {
        (None, None) => {}
        (Some(bound), Some(carried)) if bound == carried => {
            // Hash the immutable source bytes, never parse legacy JSON.
            let legacy = fs::read(record.text(schema::IMPORT_LEGACY_PATH).map_err(e)?)
                .map_err(|x| x.to_string())?;
            if darkbot_bsc_record::sha256(&legacy) != carried {
                return Err(format!("historical source hash mismatch: {path}"));
            }
        }
        _ => return Err(format!("legacy hash binding mismatch: {path}")),
    }
    Ok(record)
}

/// The checkout whose attempt latch and authority remote count: this one,
/// or, for a GUI snapshot (no Git checkout of its own), the main checkout it
/// was synced from (`DARKBOT_BSC_CHECKOUT`, absolute; it passes the same
/// repository root check or the latch row stays BLOCKED).
fn attempt_checkout() -> PathBuf {
    if Path::new(".git").is_dir() {
        return PathBuf::from(".");
    }
    env::var_os("DARKBOT_BSC_CHECKOUT")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Every file the v5 contract binds (its records and the hashed legacy
/// sources of imports), repository-relative: exactly what a GUI snapshot
/// carries so that readiness evaluates the bound evidence.
fn bound_inputs() -> Result<Vec<String>, String> {
    let bytes = fs::read(V5_RELATIVE_PATH).map_err(|e| format!("{V5_RELATIVE_PATH}: {e}"))?;
    let contract = decode(&c::V5, &bytes).map_err(|e| e.to_string())?;
    let mut paths = Vec::new();
    for tag in [
        c::DRY_RECORD_PATH,
        c::PREFLIGHT_RECORD_PATH,
        c::BUILDER_POLICY_RECORD_PATH,
        c::WITNESS_SOURCE_RECORD_PATH,
        c::LOCAL_SIMULATION_RECORD_PATH,
        c::ECONOMICS_RECORD_PATH,
        c::PORTAL_UPGRADE_RECORD_PATH,
        c::LAUNCH_PATH_RECORD_PATH,
    ] {
        let path = contract.text(tag).map_err(|e| e.to_string())?.to_string();
        let record = schema::decode_any(&fs::read(&path).map_err(|e| format!("{path}: {e}"))?)
            .map_err(|e| format!("{path}: {e}"))?;
        // An audited import carries its hashed legacy source; native records do not.
        if let Some(darkbot_bsc_record::Value::Text(legacy)) =
            record.get(schema::IMPORT_LEGACY_PATH)
        {
            paths.push(legacy.clone());
        }
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn evaluate(current_rpc: bool, report: &mut Draft) -> Result<(), String> {
    use schema::g06a::{
        builder_policy as b, dry as d, economics, local_simulation, portal_upgrade, preflight as f,
    };
    if !Path::new(V5_RELATIVE_PATH).is_file() {
        check(
            report,
            "V5_CONTRACT",
            "BLOCKED",
            "No v5 contract yet: v4 binds no launch-path snapshot (C1/R1); run the requalification DRY (tools/g06a_final_dry.sh --prepare, then the separately authorized --sign), then write the v5 proposal (g07_acceptance_contract)",
        );
        return Err("requalification pending: no v5 contract".into());
    }
    let snap = inspect_v5(Path::new(V5_RELATIVE_PATH))?;
    if FROZEN_APPROVED_V5_SHA256 != "UNARMED"
        || snap.approved
        || snap.execution_ready
        || snap.max_attempts != 1
    {
        return Err(
            "readiness requires an inactive v5 proposal, UNARMED and exactly one attempt".into(),
        );
    }
    report.set(r::CONTRACT_SHA256, snap.sha256.as_str());
    check(
        report,
        "V5_GUARD",
        "PASS",
        "UNARMED; approved=false; execution_ready=false; attempts.maximum=1",
    );
    let contract_bytes = fs::read(V5_RELATIVE_PATH).map_err(|e| e.to_string())?;
    let contract = decode(&c::V5, &contract_bytes).map_err(|e| e.to_string())?;
    let dry = bound_input(
        &contract,
        c::DRY_RECORD_PATH,
        c::DRY_RECORD_SHA256,
        Some(c::DRY_LEGACY_SHA256),
        &[&d::NATIVE],
    )?;
    let baseline = bound_input(
        &contract,
        c::PREFLIGHT_RECORD_PATH,
        c::PREFLIGHT_RECORD_SHA256,
        Some(c::PREFLIGHT_LEGACY_SHA256),
        &[&f::SCHEMA],
    )?;
    let policy = bound_input(
        &contract,
        c::BUILDER_POLICY_RECORD_PATH,
        c::BUILDER_POLICY_RECORD_SHA256,
        Some(c::BUILDER_POLICY_LEGACY_SHA256),
        &[&b::NATIVE, &b::SCHEMA],
    )?;
    let witness = bound_input(
        &contract,
        c::WITNESS_SOURCE_RECORD_PATH,
        c::WITNESS_SOURCE_RECORD_SHA256,
        Some(c::WITNESS_SOURCE_LEGACY_SHA256),
        &[&schema::g07::witness_source::SCHEMA],
    )?;
    let further: [(u16, u16, &[&'static darkbot_bsc_record::Schema]); 4] = [
        (
            c::LOCAL_SIMULATION_RECORD_PATH,
            c::LOCAL_SIMULATION_RECORD_SHA256,
            &[&local_simulation::NATIVE],
        ),
        // SELF_B: the private cross-check (the guard pins it to the profile).
        (
            c::ECONOMICS_RECORD_PATH,
            c::ECONOMICS_RECORD_SHA256,
            &[&economics::NATIVE, &economics::PRIVATE],
        ),
        (
            c::PORTAL_UPGRADE_RECORD_PATH,
            c::PORTAL_UPGRADE_RECORD_SHA256,
            &[&portal_upgrade::SCHEMA],
        ),
        (
            c::LAUNCH_PATH_RECORD_PATH,
            c::LAUNCH_PATH_RECORD_SHA256,
            &[&schema::g06a::launch_path::SCHEMA],
        ),
    ];
    for (path, hash, accepted) in further {
        bound_input(&contract, path, hash, None, accepted)?;
    }
    check(
        report,
        "V5_INPUT_BINDINGS",
        "PASS",
        "Four G0.7 inputs plus local simulation, economics cross-check, Portal upgrade scan and launch-path snapshot match their bound SHA-256 (legacy source hashes where an input is an import)",
    );
    // The guard's own pre-egress binding, the launch path included.
    let (_, launch_path) = verify_further_v5_evidence(Path::new("."), &contract_bytes)?;
    let traced = launch_path.snapshot();
    check(
        report,
        "LAUNCH_PATH_BINDING",
        "PASS",
        &format!(
            "{} code entries and {} storage selectors traced at the bound preflight block {}, of the DRY's token",
            traced.code.len(),
            traced.storage.len(),
            traced.block_number
        ),
    );
    for tag in [
        d::DRY_1,
        d::DRY_2,
        d::DRY_3,
        d::DRY_4,
        d::DRY_ALL,
        d::ZERO_EGRESS_STRUCTURAL,
    ] {
        if !field(dry.bool(tag)) {
            return Err(format!("DRY check {tag} failed"));
        }
    }
    check(
        report,
        "DRY1_DRY4",
        "PASS",
        "Requalification DRY records verified (bound by v5)",
    );
    let launcher = Address::from(field(dry.address(d::LAUNCHER)));
    let buyer = Address::from(field(dry.address(d::BUYER)));
    if launcher.to_string().to_lowercase() != snap.launcher_address
        || buyer.to_string().to_lowercase() != snap.buyer_address
        || field(dry.u64(d::CHAIN_ID)) != snap.chain_id
        || snap.chain_id != CHAIN_ID
        || Address::from(field(dry.address(d::PORTAL)))
            .to_string()
            .to_lowercase()
            != snap.portal_address
    {
        return Err("DRY identity differs from v5".into());
    }
    let gas = field(dry.u64(d::GAS_PRICE_WEI));
    let lg = field(dry.u64(d::LAUNCH_GAS_LIMIT));
    let bg = field(dry.u64(d::BUY_GAS_LIMIT));
    let lv: u128 = U256::from_be_bytes(field(dry.u256(d::LAUNCH_VALUE_WEI)))
        .try_into()
        .map_err(|_| "launch overflow")?;
    let bv: u64 = U256::from_be_bytes(field(dry.u256(d::BUY_VALUE_WEI)))
        .try_into()
        .map_err(|_| "BUY overflow")?;
    let reserve = lv
        .checked_add(u128::from(bv))
        .and_then(|x| x.checked_add((u128::from(lg) + u128::from(bg)) * u128::from(gas)))
        .ok_or("reserve overflow")?;
    if lv > snap.max_launch_value_per_attempt
        || u128::from(bv) > snap.max_buy_value_per_attempt
        || lg > snap.max_launch_gas_limit
        || bg > snap.max_buy_gas_limit
        || u128::from(gas) > snap.max_gas_price_wei
        || reserve > snap.max_total_per_attempt
        || reserve > snap.absolute_budget_wei
    {
        return Err("exact DRY parameters exceed v5 budget".into());
    }
    let _ = validate_builder_policy(field(contract.text(c::BUILDER_POLICY_RECORD_PATH)), gas);
    let _ = validate_producer_witness_source(
        field(contract.text(c::WITNESS_SOURCE_RECORD_PATH)),
        &snap.attempt_id,
    );
    check(
        report,
        "WITNESS_SOURCE_BINDING",
        "PASS",
        "Witness source contract identity and integrity verified; not a producer timing measurement",
    );
    let output = U256::from_be_bytes(field(dry.u256(d::LOCAL_EXPECTED_OUTPUT)));
    if output == U256::ZERO
        || output * U256::from(9900) / U256::from(10000)
            != U256::from_be_bytes(field(dry.u256(d::MIN_OUTPUT)))
    {
        return Err("DRY slippage mismatch".into());
    }
    check(
        report,
        "BUDGET_GAS_SLIPPAGE",
        "PASS",
        "Exact signed DRY limits, 100-bps minimum output and imported builder floors",
    );
    check(
        report,
        "SIGNED_TX_HASHES",
        "PASS",
        &format!(
            "launch={} buy={}",
            B256::from(field(dry.b256(d::LAUNCH_TX_HASH))),
            B256::from(field(dry.b256(d::BUY_TX_HASH)))
        ),
    );
    // Readiness counts only for the selected candidate the v5 binds.
    let (status, detail) = candidate::contract_candidate_check(
        Path::new(candidate::CANDIDATE_STATE_ROOT),
        Path::new(candidate::PUBLIC_MIRROR_ROOT),
        snap.candidate_profile.as_deref().unwrap_or_default(),
        snap.candidate_id.as_deref().unwrap_or_default(),
        Address::from(field(dry.address(d::TOKEN))),
        launcher,
        B256::from(field(dry.b256(d::LAUNCH_TX_HASH))),
        B256::from(field(dry.b256(d::BUY_TX_HASH))),
    );
    check(report, "CANDIDATE_SELECTION", status, &detail);
    validate_attempt_id(&snap.attempt_id)?;
    let checkout = attempt_checkout();
    match absent_attempt_latch(&checkout, &snap.attempt_id) {
        Ok(()) => check(report, "ATTEMPT_LATCH", "PASS", "Verified main checkout; no attempt directory; no latch created"),
        Err(reason) => check(report, "ATTEMPT_LATCH", "BLOCKED", &reason),
    }
    // BSC-005: before the approval no shared authority of this attempt may
    // exist on the checkout's origin (any one is granted or consumed).
    if current_rpc {
        let git = darkbot_bsc_execution::attempt_authority::Git {
            repo: checkout.clone(),
            remote: "origin".into(),
            timeout: std::time::Duration::from_secs(30),
        };
        match darkbot_bsc_execution::attempt_authority::existing(&git, &snap.attempt_id) {
            Ok(None) => check(
                report,
                "ATTEMPT_AUTHORITY",
                "PASS",
                "No shared authority of this attempt on origin; the grant follows the approval (g07_attempt_authority grant)",
            ),
            Ok(Some(head)) => check(
                report,
                "ATTEMPT_AUTHORITY",
                "STOP",
                &format!(
                    "A shared authority of this attempt already exists on origin (granted or consumed, {head}); this attempt is never released again"
                ),
            ),
            Err(reason) => check(
                report,
                "ATTEMPT_AUTHORITY",
                "BLOCKED",
                &format!("Shared attempt authority unreadable: {reason}"),
            ),
        }
    } else {
        check(
            report,
            "ATTEMPT_AUTHORITY",
            "BLOCKED",
            "No remote read in zero-egress mode",
        );
    }
    let (ln, bn, _) = validate_current_preflight(&baseline, &baseline, launcher, buyer, gas);
    check(
        report,
        "FROZEN_PREFLIGHT",
        "PASS",
        &format!(
            "Requalification baseline deployment, quote and funding checks; nonces {ln}/{bn}; not a current-chain attestation"
        ),
    );
    let now = unix_time_ms() / 1000;
    let date = field(policy.text(b::OBSERVED_DATE));
    // Only a native re-determination carries fetched sources; its freshness
    // is the fetch time of its oldest source (BSC-004: the runner applies the
    // same rule right before its latch). An import's date is history.
    let fetched = builder_policy::oldest_fetch_s(&policy);
    check(
        report,
        "BUILDER_FRESHNESS",
        if builder_policy::age_ok(fetched, now) {
            "PASS"
        } else {
            "BLOCKED"
        },
        &match fetched {
            Some(t) => format!(
                "Bound re-determination {date}, oldest source fetched at unix {t}; requires sources <=24h old before arming"
            ),
            None => format!(
                "Bound policy {date} has no fetched sources (historical import); requires a re-determination <=24h before arming"
            ),
        },
    );
    check(
        report,
        "BUILDER_SEMANTICS",
        "UNQUALIFIED",
        "Existing probe proves submission checks only; inclusion, 48Club pricing and building-time noMerge/positionFirst effects remain unqualified economic risks requiring separate explicit acceptance",
    );
    check(
        report,
        "PRODUCER_WITNESS",
        "UNKNOWN",
        &format!(
            "T0-to-BUY timing is not measured by readiness; bound source_ready={}; original producer execution witness required for the later timing verdict",
            field(witness.bool(schema::g07::witness_source::SOURCE_READY))
        ),
    );
    if current_rpc {
        let current = run_current_preflight(launcher, buyer, gas, lg, bg, lv, bv)?;
        // All drifted fields at once (G0.7 itself stops at the first).
        let drift = deployment_drift(&baseline, &current);
        if !drift.is_empty() {
            check(
                report,
                "CURRENT_DEPLOYMENT_DRIFT",
                "STOP",
                &format!(
                    "Current Mainnet differs from the frozen DRY baseline; the bound DRY and signed bytes are not valid for it: {}",
                    drift.join("; ")
                ),
            );
            return Err("current deployment/economics drift from the frozen DRY baseline".into());
        }
        let (ln, bn, head) = validate_current_preflight(&baseline, &current, launcher, buyer, gas);
        let (signed_ln, signed_bn) = (
            field(baseline.u64(f::LAUNCHER_PENDING_NONCE)),
            field(baseline.u64(f::BUYER_PENDING_NONCE)),
        );
        if ln != signed_ln || bn != signed_bn {
            check(
                report,
                "CURRENT_NONCES",
                "STOP",
                &format!(
                    "Pending nonces {ln}/{bn} differ from the signed DRY's {signed_ln}/{signed_bn}; the signed bytes cannot execute as bound"
                ),
            );
            return Err("nonce drift from signed DRY".into());
        }
        check(
            report,
            "CURRENT_DEPLOYMENTS_NONCES_FUNDING_GAS",
            "PASS",
            "Existing native preflight, exact DRY parameters",
        );
        match current_launch_path(&launch_path, &current)? {
            Current::Unchanged(_) => check(
                report,
                "CURRENT_LAUNCH_PATH",
                "PASS",
                &format!(
                    "{} code hashes and {} storage selectors unchanged at the current preflight block {}",
                    traced.code.len(),
                    traced.storage.len(),
                    field(current.u64(f::LATEST_BLOCK_NUMBER))
                ),
            ),
            Current::Drift(drift) => {
                check(
                    report,
                    "CURRENT_LAUNCH_PATH",
                    "STOP",
                    &format!(
                        "The launch path differs from the v5-bound snapshot; the bound DRY is not valid for it: {}",
                        drift.join("; ")
                    ),
                );
                return Err("current launch-path drift from the v5-bound snapshot".into());
            }
        }
        let endpoint = parse_rpc_endpoint(&req_env("BSC_MAINNET_RPC_HTTPS"))?;
        let mut poller = HeadPoller::connect(&endpoint)?;
        let fresh = poller.next_head(HEAD_TRANSITION_TIMEOUT)?;
        let hash = format!(
            "0x{}",
            darkbot_bsc_record::hex(&field(current.b256(f::LATEST_BLOCK_HASH)))
        );
        bundle_window(
            head,
            &hash,
            &fresh.head,
            fresh.local_unix_ms,
            fresh.excess_staleness_ms,
        )?;
        check(
            report,
            "CLOCK_HEAD",
            "PASS",
            "Existing native head-window freshness rules",
        );
    } else {
        check(
            report,
            "CURRENT_DEPLOYMENTS_NONCES_FUNDING_GAS",
            "BLOCKED",
            "Offline mode cannot attest current chain state; --current-rpc required for fresh read-only checks",
        );
        check(
            report,
            "CURRENT_LAUNCH_PATH",
            "BLOCKED",
            "Offline mode cannot re-read the launch path; --current-rpc required",
        );
        check(
            report,
            "CLOCK_HEAD",
            "BLOCKED",
            "No live head observation in zero-egress mode",
        );
    }
    let clock_config = ["DARKBOT_G07_CLOCK_ID", "DARKBOT_G07_BOOT_ID"]
        .iter()
        .all(|k| env::var(k).is_ok_and(|v| !v.trim().is_empty()));
    check(
        report,
        "CLOCK_CONFIG",
        if clock_config { "PASS" } else { "BLOCKED" },
        "Attempt clock_id and boot_id must be configured before arming",
    );
    check(
        report,
        "SIGNED_EGRESS",
        "PASS",
        "No signing, builder send, secret loading or attempt mutation in readiness",
    );
    Ok(())
}

// Completeness is mandatory: an early return, panic or missing check can never
// become PASS. Future outcome evidence is kept separate from technical safety.
const REQUIRED_TECHNICAL_CHECKS: &[&str] = &[
    "V5_GUARD",
    "V5_INPUT_BINDINGS",
    "LAUNCH_PATH_BINDING",
    "DRY1_DRY4",
    "WITNESS_SOURCE_BINDING",
    "BUDGET_GAS_SLIPPAGE",
    "SIGNED_TX_HASHES",
    "CANDIDATE_SELECTION",
    "ATTEMPT_LATCH",
    "ATTEMPT_AUTHORITY",
    "FROZEN_PREFLIGHT",
    "BUILDER_FRESHNESS",
    "CURRENT_DEPLOYMENTS_NONCES_FUNDING_GAS",
    "CURRENT_LAUNCH_PATH",
    "CLOCK_HEAD",
    "CLOCK_CONFIG",
    "SIGNED_EGRESS",
];

fn technical_ready(report: &Record) -> bool {
    if report.text(r::MODE).ok() != Some("READ_ONLY_RPC") {
        return false;
    }
    let Ok(checks) = report.nested_all(r::CHECKS) else {
        return false;
    };
    let mut seen = std::collections::BTreeSet::new();
    for row in &checks {
        let (Ok(name), Ok(status)) = (row.text(r::check::NAME), row.text(r::check::STATUS)) else {
            return false;
        };
        if !seen.insert(name) {
            return false;
        }
        let accepted = match name {
            "BUILDER_SEMANTICS" => status == "UNQUALIFIED",
            "PRODUCER_WITNESS" => status == "UNKNOWN",
            _ => status == "PASS",
        };
        if !accepted {
            return false;
        }
    }
    REQUIRED_TECHNICAL_CHECKS
        .iter()
        .all(|name| seen.contains(name))
        && seen.contains("BUILDER_SEMANTICS")
        && seen.contains("PRODUCER_WITNESS")
}

// PASS_READ_ONLY only when technically ready; STOP when a check found a
// contradiction with the bound attempt (deployment or nonce drift); otherwise
// BLOCKED (a precondition is missing). Both refusals exit 2.
fn finish_report(report: &mut Draft) -> (Vec<u8>, i32) {
    let before = report.encode().expect("encode evaluated readiness");
    let evaluated = decode(&r::SCHEMA, &before).expect("decode evaluated readiness");
    let pass = technical_ready(&evaluated);
    let stop = evaluated.nested_all(r::CHECKS).is_ok_and(|checks| {
        checks
            .iter()
            .any(|row| row.text(r::check::STATUS).ok() == Some("STOP"))
    });
    let status = match (pass, stop) {
        (true, _) => "PASS_READ_ONLY",
        (false, true) => "STOP",
        (false, false) => "BLOCKED",
    };
    report.set(r::STATUS, status).set(r::LIVE_AUTHORIZED, false);
    (
        report.encode().expect("encode readiness verdict"),
        if pass { 0 } else { 2 },
    )
}

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--bound-inputs" {
        match bound_inputs() {
            Ok(paths) => {
                for path in paths {
                    println!("{path}");
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("STOP: {e}");
                std::process::exit(2);
            }
        }
    }
    let current_rpc = args.iter().any(|a| a == "--current-rpc");
    let binary = args.iter().any(|a| a == "--binary");
    if args.iter().any(|a| a != "--current-rpc" && a != "--binary") {
        eprintln!("usage: g07_pre_live_readiness [--current-rpc] [--binary] | --bound-inputs");
        std::process::exit(2);
    }
    let mut report = Draft::new(&r::SCHEMA);
    report
        .set(r::STATUS, "BLOCKED")
        .set(
            r::MODE,
            if current_rpc {
                "READ_ONLY_RPC"
            } else {
                "OFFLINE_ZERO_EGRESS"
            },
        )
        .set(r::OBSERVED_UNIX_MS, unix_time_ms())
        .set(r::LIVE_AUTHORIZED, false);
    let evaluated = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        evaluate(current_rpc, &mut report)
    }));
    match evaluated {
        Ok(Ok(())) => {}
        Ok(Err(e)) => check(&mut report, "VALIDATION", "BLOCKED", &e),
        Err(payload) => {
            let reason = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown");
            check(
                &mut report,
                "VALIDATION",
                "BLOCKED",
                &format!("Native validation failed ({reason}); no authority granted"),
            )
        }
    }
    let (bytes, exit_code) = finish_report(&mut report);
    let record = decode(&r::SCHEMA, &bytes).expect("decode readiness");
    if binary {
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .expect("write binary readiness");
    } else {
        print!("{}", record.render());
        println!(
            "PRE_LIVE_READINESS={}\nLIVE_GUARD={FROZEN_APPROVED_V5_SHA256}\nLIVE=BLOCKED",
            field(record.text(r::STATUS))
        );
    }
    std::process::exit(exit_code);
}

#[cfg(test)]
mod readiness_tests {
    use super::*;

    fn evaluated_report(omit: Option<&str>, failed: Option<(&str, &str)>) -> Draft {
        let mut report = Draft::new(&r::SCHEMA);
        report
            .set(r::STATUS, "BLOCKED")
            .set(r::MODE, "READ_ONLY_RPC")
            .set(r::OBSERVED_UNIX_MS, 1_u64)
            .set(r::LIVE_AUTHORIZED, false);
        for name in REQUIRED_TECHNICAL_CHECKS {
            if omit == Some(*name) {
                continue;
            }
            let status = failed
                .filter(|(target, _)| target == name)
                .map_or("PASS", |(_, status)| status);
            check(&mut report, name, status, "offline verdict fixture");
        }
        for (name, default_status) in [
            ("BUILDER_SEMANTICS", "UNQUALIFIED"),
            ("PRODUCER_WITNESS", "UNKNOWN"),
        ] {
            if omit == Some(name) {
                continue;
            }
            let status = failed
                .filter(|(target, _)| *target == name)
                .map_or(default_status, |(_, status)| status);
            check(
                &mut report,
                name,
                status,
                "later outcome evidence remains unqualified",
            );
        }
        report
    }

    #[test]
    fn pass_read_only_is_reachable_without_claiming_outcomes_or_live_authority() {
        let mut report = evaluated_report(None, None);
        let (bytes, code) = finish_report(&mut report);
        let result = decode(&r::SCHEMA, &bytes).unwrap();
        assert_eq!(code, 0);
        assert_eq!(result.text(r::STATUS).unwrap(), "PASS_READ_ONLY");
        assert!(!result.bool(r::LIVE_AUTHORIZED).unwrap());
        assert_eq!(FROZEN_APPROVED_V5_SHA256, "UNARMED");
        assert!(result.render().contains("UNQUALIFIED"));
        assert!(result.render().contains("UNKNOWN"));
    }

    #[test]
    fn missing_or_nonpassing_technical_checks_always_block() {
        for name in REQUIRED_TECHNICAL_CHECKS {
            assert_eq!(
                finish_report(&mut evaluated_report(Some(name), None)).1,
                2,
                "missing {name}"
            );
            for status in ["BLOCKED", "STOP", "UNKNOWN", "UNQUALIFIED"] {
                assert_eq!(
                    finish_report(&mut evaluated_report(None, Some((name, status)))).1,
                    2,
                    "{name}: {status}"
                );
            }
        }
    }

    #[test]
    fn offline_errors_duplicates_and_missing_risk_disclosures_cannot_pass() {
        let mut offline = evaluated_report(None, None);
        offline.set(r::MODE, "OFFLINE_ZERO_EGRESS");
        assert_eq!(finish_report(&mut offline).1, 2);
        for (name, status) in [
            ("VALIDATION", "BLOCKED"),
            ("CURRENT_DEPLOYMENT_DRIFT", "STOP"),
            ("V5_GUARD", "PASS"),
            ("BUILDER_SEMANTICS", "PASS"),
            ("PRODUCER_WITNESS", "PASS"),
        ] {
            let mut report = evaluated_report(None, None);
            check(
                &mut report,
                name,
                status,
                "must not turn an invalid result green",
            );
            assert_eq!(finish_report(&mut report).1, 2);
        }
        for name in ["BUILDER_SEMANTICS", "PRODUCER_WITNESS"] {
            assert_eq!(finish_report(&mut evaluated_report(Some(name), None)).1, 2);
            assert_eq!(
                finish_report(&mut evaluated_report(None, Some((name, "PASS")))).1,
                2
            );
        }
        let mut incomplete = evaluated_report(None, None);
        incomplete.clear(r::CHECKS);
        assert_eq!(finish_report(&mut incomplete).1, 2);
    }

    #[test]
    fn a_contradiction_stops_and_a_missing_precondition_blocks() {
        let status = |report: &mut Draft| {
            let (bytes, code) = finish_report(report);
            assert_eq!(code, 2);
            let record = decode(&r::SCHEMA, &bytes).unwrap();
            assert!(!record.bool(r::LIVE_AUTHORIZED).unwrap());
            record.text(r::STATUS).unwrap().to_string()
        };
        let mut blocked = evaluated_report(None, Some(("BUILDER_FRESHNESS", "BLOCKED")));
        assert_eq!(status(&mut blocked), "BLOCKED");
        for name in ["CURRENT_DEPLOYMENT_DRIFT", "CURRENT_NONCES"] {
            let mut stop = evaluated_report(None, Some(("BUILDER_FRESHNESS", "BLOCKED")));
            check(
                &mut stop,
                name,
                "STOP",
                "contradiction with the bound attempt",
            );
            check(&mut stop, "VALIDATION", "BLOCKED", "drift");
            assert_eq!(status(&mut stop), "STOP", "{name}");
        }
    }

    #[test]
    fn every_drifted_deployment_field_is_reported_with_both_values() {
        use schema::g06a::preflight as f;
        let baseline = decode(
            &f::SCHEMA,
            include_bytes!(
                "../../../../../native/evidence/g06a/g07_binary_import_20261016/preflight.dbrec"
            ),
        )
        .unwrap();
        assert!(deployment_drift(&baseline, &baseline).is_empty());
        let mut changed = Draft::new(&f::SCHEMA);
        for (tag, value) in baseline.fields() {
            changed.push(*tag, value.clone());
        }
        changed
            .set(f::PORTAL_VERSION, "v5.25.0")
            .set(f::FEE_BUY_BPS, Value::u128(101));
        let current = decode(&f::SCHEMA, &changed.encode().unwrap()).unwrap();
        assert_eq!(
            deployment_drift(&baseline, &current),
            [
                "portal_version: v5.24.0 -> v5.25.0",
                "protocol_fee_bps.buy: 100 -> 101"
            ]
        );
    }
}
