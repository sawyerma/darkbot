//! Native read-only readiness. Default: zero network egress, no secrets,
//! signatures, latch creation or probe. --current-rpc explicitly enables only
//! the existing read-only preflight/head logic; never a builder submission.
#![allow(dead_code, unused_imports)]
#[path = "g07_mainnet_self_launch_buy/builder_submission.rs"]
mod builder_submission;
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
    FROZEN_APPROVED_V4_SHA256, V4_RELATIVE_PATH, inspect_v4,
};
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

/// A bound input of the v4 contract: decoded against one of the accepted
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

// A date-only observation can guarantee freshness only from its earliest
// possible time (00:00 UTC). Never extend it using today's file mtime.
fn observation_seconds(date: &str) -> Option<u64> {
    let parts: Vec<_> = date.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: u64 = parts[0].parse().ok()?;
    let month: usize = parts[1].parse().ok()?;
    let day: u64 = parts[2].parse().ok()?;
    if !(1970..=9999).contains(&year) || !(1..=12).contains(&month) {
        return None;
    }
    let leap = |y: u64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || day > months[month - 1] {
        return None;
    }
    let days: u64 = (1970..year).map(|y| if leap(y) { 366 } else { 365 }).sum();
    Some((days + months[..month - 1].iter().sum::<u64>() + day - 1) * 86400)
}
fn freshness(observed: Option<u64>, now: u64) -> bool {
    observed.is_some_and(|t| now.checked_sub(t).is_some_and(|age| age <= 86400))
}

fn evaluate(current_rpc: bool, report: &mut Draft) -> Result<(), String> {
    use schema::g06a::{
        builder_policy as b, dry as d, economics, local_simulation, portal_upgrade, preflight as f,
    };
    if !Path::new(V4_RELATIVE_PATH).is_file() {
        check(
            report,
            "V4_CONTRACT",
            "BLOCKED",
            "No v4 contract yet: the v3 proposal is superseded by the Portal v5.25.0 drift; run the requalification DRY (tools/g06a_final_dry.sh), then write the v4 proposal",
        );
        return Err("requalification pending: no v4 contract".into());
    }
    let snap = inspect_v4(Path::new(V4_RELATIVE_PATH))?;
    if FROZEN_APPROVED_V4_SHA256 != "UNARMED"
        || snap.approved
        || snap.execution_ready
        || snap.max_attempts != 1
    {
        return Err(
            "readiness requires an inactive v4 proposal, UNARMED and exactly one attempt".into(),
        );
    }
    report.set(r::CONTRACT_SHA256, snap.sha256.as_str());
    check(
        report,
        "V4_GUARD",
        "PASS",
        "UNARMED; approved=false; execution_ready=false; attempts.maximum=1",
    );
    let contract = decode(
        &c::V4,
        &fs::read(V4_RELATIVE_PATH).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
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
    for (path, hash, accepted) in [
        (
            c::LOCAL_SIMULATION_RECORD_PATH,
            c::LOCAL_SIMULATION_RECORD_SHA256,
            &local_simulation::NATIVE,
        ),
        (
            c::ECONOMICS_RECORD_PATH,
            c::ECONOMICS_RECORD_SHA256,
            &economics::NATIVE,
        ),
        (
            c::PORTAL_UPGRADE_RECORD_PATH,
            c::PORTAL_UPGRADE_RECORD_SHA256,
            &portal_upgrade::SCHEMA,
        ),
    ] {
        bound_input(&contract, path, hash, None, &[accepted])?;
    }
    check(
        report,
        "V4_INPUT_BINDINGS",
        "PASS",
        "Four G0.7 inputs plus local simulation, economics cross-check and Portal upgrade scan match their bound SHA-256 (legacy source hashes where an input is an import)",
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
        "Requalification DRY records verified (bound by v4)",
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
        return Err("DRY identity differs from v4".into());
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
        return Err("exact DRY parameters exceed v4 budget".into());
    }
    let _ = validate_builder_policy(field(contract.text(c::BUILDER_POLICY_RECORD_PATH)), gas);
    let _ = validate_producer_witness_source(
        field(contract.text(c::WITNESS_SOURCE_RECORD_PATH)),
        &snap.attempt_id,
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
    validate_attempt_id(&snap.attempt_id)?;
    let state = fs::symlink_metadata(Path::new(ATTEMPT_STATE_ROOT).join(&snap.attempt_id));
    if state
        .as_ref()
        .is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound)
    {
        return Err("attempt state cannot be inspected".into());
    }
    if state.is_ok() {
        check(
            report,
            "ATTEMPT_LATCH",
            "BLOCKED",
            "Existing attempt state requires reconciliation; no reset/retry",
        );
    } else {
        check(
            report,
            "ATTEMPT_LATCH",
            "PASS",
            "No attempt directory; no latch created; maximum one",
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
    check(
        report,
        "BUILDER_FRESHNESS",
        if freshness(observation_seconds(date), now) {
            "PASS"
        } else {
            "BLOCKED"
        },
        &format!("Bound observation {date}; requires current evidence <=24h before arming"),
    );
    check(
        report,
        "BUILDER_SEMANTICS",
        "BLOCKED",
        "Existing probe proves submission checks only; inclusion/noMerge/positionFirst remain unqualified",
    );
    check(
        report,
        "PRODUCER_WITNESS",
        if field(witness.bool(schema::g07::witness_source::SOURCE_READY)) {
            "PASS"
        } else {
            "BLOCKED"
        },
        "Original producer execution witness is separately required for T0-to-BUY timing",
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
        if ln != field(baseline.u64(f::LAUNCHER_PENDING_NONCE))
            || bn != field(baseline.u64(f::BUYER_PENDING_NONCE))
        {
            return Err("nonce drift from signed DRY".into());
        }
        check(
            report,
            "CURRENT_DEPLOYMENTS_NONCES_FUNDING_GAS",
            "PASS",
            "Existing native preflight, exact DRY parameters",
        );
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

fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let current_rpc = args.iter().any(|a| a == "--current-rpc");
    let binary = args.iter().any(|a| a == "--binary");
    if args.iter().any(|a| a != "--current-rpc" && a != "--binary") {
        eprintln!("usage: g07_pre_live_readiness [--current-rpc] [--binary]");
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
    let bytes = report.encode().expect("encode readiness");
    let record = decode(&r::SCHEMA, &bytes).expect("decode readiness");
    if binary {
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .expect("write binary readiness");
    } else {
        print!("{}", record.render());
        println!(
            "PRE_LIVE_READINESS=BLOCKED\nLIVE_GUARD={FROZEN_APPROVED_V4_SHA256}\nLIVE=BLOCKED"
        );
    }
    std::process::exit(2);
}

#[cfg(test)]
mod readiness_tests {
    use super::*;

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
    #[test]
    fn freshness_is_bounded_and_never_accepts_future_or_invalid_dates() {
        assert_eq!(observation_seconds("1970-01-01"), Some(0));
        assert_eq!(observation_seconds("2024-02-30"), None);
        assert!(freshness(Some(10), 86410));
        assert!(!freshness(Some(10), 86411));
        assert!(!freshness(Some(11), 10));
        assert!(!freshness(None, 10));
    }
}
