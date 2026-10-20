use super::wire::bsc::*;
use darkbot_bsc_execution::acceptance_guard::{FROZEN_APPROVED_V4_SHA256, V4_RELATIVE_PATH};
use darkbot_bsc_record::{self as record, Record, Value, schema};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Snapshot {
    pub status: BscStatus,
    pub launch: BscLaunchState,
    pub monitoring: BscMonitoringState,
    pub wallets: BscWalletsState,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn read(path: &Path) -> Option<Vec<u8>> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > record::MAX_RECORD_BYTES as u64 {
        return None;
    }
    fs::read(path).ok()
}
fn load(path: &Path) -> Option<Record> {
    schema::decode_any(&read(path)?).ok()
}
fn value(v: &Value) -> Option<String> {
    Some(match v {
        Value::Text(t) => safe_text(t),
        Value::Bool(b) => b.to_string(),
        Value::U64(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U256(w) => record::u256_to_decimal(w),
        Value::Address(a) => format!("0x{}", record::hex(a)),
        Value::B256(b) => format!("0x{}", record::hex(b)),
        _ => return None,
    })
}
/// Details may contain external errors. URLs and credential-bearing lines never leave the reporter.
pub fn safe_text(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if [
        "api-key",
        "api_key",
        "private_key",
        "authorization",
        "bearer ",
        "password",
        "secret=",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return "[redacted]".into();
    }
    text.split_whitespace()
        .map(|s| if s.contains("://") { "[endpoint]" } else { s })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(2048)
        .collect()
}
fn field(r: &Record, name: &str) -> Option<String> {
    r.fields().iter().find_map(|(tag, v)| {
        (r.schema().field(*tag)?.name == name)
            .then(|| value(v))
            .flatten()
    })
}
fn fields(r: &Record, allowed: &[&str]) -> Vec<PublicField> {
    r.fields()
        .iter()
        .filter_map(|(tag, v)| {
            let name = r.schema().field(*tag)?.name;
            if !allowed.contains(&name) {
                return None;
            }
            Some(PublicField {
                name: name.into(),
                value: value(v)?,
            })
        })
        .collect()
}
fn time_ms(r: &Record) -> u64 {
    ["observed_unix_ms", "_provenance.observed_unix_ms"]
        .iter()
        .find_map(|n| field(r, n)?.parse::<u64>().ok())
        .unwrap_or(0)
}
fn time(r: &Record) -> u64 {
    time_ms(r) / 1000
}

fn evidence(root: &Path, path: &Path, r: Option<&Record>) -> Evidence {
    Evidence {
        path: path
            .strip_prefix(root)
            .ok()
            .or_else(|| {
                path.ancestors()
                    .find(|p| p.file_name().is_some_and(|n| n == "bsc-operational"))
                    .and_then(|base| path.strip_prefix(base).ok())
            })
            .unwrap_or(path)
            .display()
            .to_string(),
        sha256: read(path)
            .map(|b| record::hex(&record::sha256(&b)))
            .unwrap_or_default(),
        observed_unix_s: r.map(time).unwrap_or(0),
    }
}
fn walk(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || out.len() >= 2048 {
        return;
    }
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_symlink() || e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            if kind.is_dir() {
                walk(&e.path(), depth - 1, out);
            } else if kind.is_file() && e.path().extension().is_some_and(|s| s == "dbrec") {
                out.push(e.path());
            }
        }
    }
}
fn latest<'a>(records: &'a [(PathBuf, Record)], names: &[&str]) -> Option<&'a (PathBuf, Record)> {
    records
        .iter()
        .filter(|(_, r)| names.iter().any(|n| r.schema().name == *n))
        .max_by_key(|(p, r)| (time_ms(r), p.clone()))
}
fn split_drift(detail: &str, key: &str) -> Option<(String, String)> {
    let rest = detail
        .split_once(&format!("{key}: "))?
        .1
        .split(';')
        .next()?;
    let (a, b) = rest.split_once(" -> ")?;
    Some((a.trim().into(), b.trim().into()))
}
fn env_value(path: &Path, key: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .find_map(|l| {
            l.strip_prefix(&format!("{key}="))
                .map(|v| safe_text(v.trim()))
        })
        .unwrap_or_default()
}
fn optional_u64(r: &Record, name: &str) -> Option<u64> {
    field(r, name)?.parse().ok()
}
fn optional_bool(r: &Record, name: &str) -> Option<bool> {
    field(r, name)?.parse().ok()
}
fn gate(id: &str, state: &str, evidence: Option<Evidence>, reasons: Vec<String>) -> GateStatus {
    GateStatus {
        gate_id: id.into(),
        state: state.into(),
        evidence,
        reason_codes: reasons,
    }
}

pub fn project(root: &Path, rundir: &Path) -> Snapshot {
    let mut paths = Vec::new();
    for dir in [
        "bsc/native/evidence/g06a",
        "bsc/native/evidence/g07",
        "bsc/native/evidence/g08",
        "bsc/data/g06a_final_dry",
    ] {
        walk(&root.join(dir), 5, &mut paths);
    }
    let mut warnings = Vec::new();
    let mut records: Vec<_> = paths
        .into_iter()
        .filter_map(|p| match load(&p) {
            Some(r) => Some((p, r)),
            None => {
                warnings.push(format!(
                    "INVALID_EVIDENCE {}",
                    p.strip_prefix(root).unwrap_or(&p).display()
                ));
                None
            }
        })
        .collect();
    let mut operational = Vec::new();
    walk(
        &rundir.join("bsc-operational/bsc/native/evidence/g07"),
        3,
        &mut operational,
    );
    for p in operational {
        if let Some(r) = load(&p) {
            records.push((p, r));
        }
    }
    let contract_path = root.join(V4_RELATIVE_PATH);
    let contract = load(&contract_path);
    let mut guard = GuardStatus {
        frozen_approved_sha256: FROZEN_APPROVED_V4_SHA256.into(),
        guard_state: if FROZEN_APPROVED_V4_SHA256 == "UNARMED" {
            "UNARMED"
        } else {
            "ARMED_MISMATCH"
        }
        .into(),
        contract_path: V4_RELATIVE_PATH.into(),
        contract_present: contract_path.is_file(),
        contract_sha256: evidence(root, &contract_path, None).sha256,
        ..Default::default()
    };
    let mut launch = BscLaunchState {
        reason: "No active v4 contract".into(),
        ..Default::default()
    };
    if let Some(r) = contract
        .as_ref()
        .filter(|r| r.schema() == &schema::g07::contract::V4)
    {
        guard.approved = optional_bool(r, "authorization.approved");
        guard.execution_ready = optional_bool(r, "authorization.execution_ready");
        guard.attempt_id = field(r, "authorization.attempt_id").unwrap_or_default();
        guard.max_attempts = optional_u64(r, "attempts.maximum");
        if guard.guard_state != "UNARMED" && guard.contract_sha256 == FROZEN_APPROVED_V4_SHA256 {
            guard.guard_state = "ARMED".into();
        }
        launch.contract_present = true;
        launch.evidence = Some(evidence(root, &contract_path, Some(r)));
        launch.reason = "Read-only contract observation; no execution authority".into();
        // Contract schemas contain public identities, caps and binding hashes, never signed TX bytes.
        let names: Vec<_> = r.schema().fields.iter().map(|f| f.name).collect();
        launch.contract_fields = fields(r, &names);
    } else {
        warnings.push(
            if contract_path.exists() {
                "V4_CONTRACT_INVALID"
            } else {
                "V4_CONTRACT_MISSING"
            }
            .into(),
        );
    }
    let mut readiness = ReadinessStatus {
        verdict: "NOT_RUN".into(),
        funding_verdict: "UNKNOWN".into(),
        ..Default::default()
    };
    if let Some((p, r)) = latest(&records, &[schema::g07::readiness::SCHEMA.name]) {
        readiness.evidence = Some(evidence(root, p, Some(r)));
        readiness.verdict = if field(r, "status").as_deref() == Some("PASS_READ_ONLY") {
            "PASS_READ_ONLY"
        } else {
            "STOP"
        }
        .into();
        if let Ok(checks) = r.nested_all(schema::g07::readiness::CHECKS) {
            for c in checks {
                let name = field(&c, "name").unwrap_or_default();
                let status = field(&c, "status").unwrap_or_default();
                let detail = field(&c, "detail").unwrap_or_default();
                if status != "PASS" {
                    readiness.reasons.push(format!("{name}: {status}"));
                }
                if name == "CURRENT_DEPLOYMENT_DRIFT" {
                    if let Some((a, b)) = split_drift(&detail, "portal_version") {
                        readiness.portal_version_bound = a;
                        readiness.portal_version_observed = b;
                    }
                    if let Some((a, b)) = split_drift(&detail, "portal_implementation") {
                        readiness.portal_impl_bound = a;
                        readiness.portal_impl_observed = b;
                    }
                    readiness.code_hash_changed =
                        split_drift(&detail, "portal_implementation_code_sha256")
                            .map(|(a, b)| a != b);
                    if status != "PASS" {
                        warnings.push("PORTAL_DRIFT".into());
                    }
                }
                if name == "CURRENT_DEPLOYMENTS_NONCES_FUNDING_GAS" {
                    readiness.funding_verdict = status.clone();
                }
                readiness.checks.push(PublicField {
                    name,
                    value: format!("{status} · {detail}"),
                });
            }
        }
    }
    let mut dry = DryStatus {
        state: "UNKNOWN".into(),
        ..Default::default()
    };
    let dry_record = latest(
        &records,
        &[
            schema::g06a::dry::SCHEMA.name,
            schema::g06a::dry::NATIVE.name,
        ],
    );
    if let Some((p, r)) = dry_record {
        dry.evidence = Some(evidence(root, p, Some(r)));
        dry.dry_id = p
            .parent()
            .and_then(Path::file_name)
            .unwrap_or_default()
            .to_string_lossy()
            .into();
        dry.state = if optional_bool(r, "dry_pass.all") == Some(true) {
            "PASS"
        } else {
            "FAIL"
        }
        .into();
        dry.valid_for_current_portal = if readiness.code_hash_changed == Some(true) {
            Some(false)
        } else {
            None
        };
        dry.parameters = fields(
            r,
            &[
                "transactions.gas_price_wei",
                "transactions.buy_value_wei",
                "transactions.launch_value_wei",
                "transactions.launch_gas_limit",
                "transactions.buy_gas_limit",
                "transactions.local_expected_output",
                "transactions.min_output",
                "bundle_projection.send_head",
                "budget.worst_case_reserved_wei",
                "dry_pass.all",
                "dry_pass.zero_egress_structural",
            ],
        );
    }
    let mut wallets = BscWalletsState {
        expected_address: env_value(
            &root.join("bsc/config/wallet.public.env"),
            "BSC_EXPECTED_ADDRESS",
        ),
        wallet_status: env_value(
            &root.join("bsc/config/wallet.public.env"),
            "BSC_WALLET_STATUS",
        ),
        ..Default::default()
    };
    if let Some((p, r)) = latest(&records, &[schema::g06a::preflight::SCHEMA.name]) {
        wallets.evidence = Some(evidence(root, p, Some(r)));
        wallets.source = "HISTORICAL_PREFLIGHT".into();
        wallets.observed_unix_s = time(r);
        wallets.launcher_address = field(r, "launcher_address").unwrap_or_default();
        wallets.buyer_address = field(r, "buyer_address").unwrap_or_default();
        wallets.chain_id = optional_u64(r, "chain_id");
        wallets.launcher_balance_wei = field(r, "launcher_balance_wei");
        wallets.buyer_balance_wei = field(r, "buyer_balance_wei");
        wallets.observed_block = optional_u64(r, "latest_block_number");
        wallets.required_launcher_wei =
            field(r, "wallet_qualification.launcher_required_worst_case_wei");
        wallets.required_buyer_wei = field(r, "wallet_qualification.buyer_required_worst_case_wei");
        if readiness.portal_version_bound.is_empty() {
            readiness.portal_version_bound = field(r, "portal_version").unwrap_or_default();
        }
    }
    let mut witness = WitnessStatus {
        t0_to_buy_state: "UNKNOWN".into(),
        ..Default::default()
    };
    if let Some((p, r)) = latest(&records, &[schema::g07::witness_source::SCHEMA.name]) {
        witness.source_ready = optional_bool(r, "source_ready");
        witness.evidence = Some(evidence(root, p, Some(r)));
    }
    if witness.source_ready != Some(true) {
        warnings.push("WITNESS_SOURCE_NOT_READY".into());
    }
    let mut builder = BuilderStatus {
        price: "UNQUALIFIED".into(),
        no_merge: "UNQUALIFIED".into(),
        position_first: "UNQUALIFIED".into(),
        inclusion: "UNQUALIFIED".into(),
        ..Default::default()
    };
    if let Some((_, r)) = latest(
        &records,
        &[
            schema::g06a::builder_policy::SCHEMA.name,
            schema::g06a::builder_policy::NATIVE.name,
        ],
    ) {
        builder.floors = fields(
            r,
            &[
                "strictest_explicit_verified_floor_wei",
                "stage1_signed_gas_price_policy_wei",
                "blockrazor.explicit_minimum_bundle_gas_price_wei",
                "fortyeightclub.explicit_minimum_bundle_gas_price_wei",
            ],
        );
        if r.schema() == &schema::g06a::builder_policy::NATIVE {
            builder.refresh_observed_unix_s = r
                .nested_all(schema::g06a::builder_policy::SOURCES)
                .ok()
                .and_then(|rows| {
                    rows.iter()
                        .filter_map(|r| {
                            r.u64(schema::g06a::builder_policy::source::FETCHED_UNIX_MS)
                                .ok()
                        })
                        .min()
                })
                .unwrap_or(0)
                / 1000;
        }
        if builder.refresh_observed_unix_s > 0 {
            builder.refresh_expires_unix_s = builder.refresh_observed_unix_s.saturating_add(86400);
        }
    }
    // Legacy external observations are read at this reporting boundary only; never return request bodies.
    let mut json_paths = Vec::new();
    json_walk(&root.join("bsc/native/evidence/g06a"), 4, &mut json_paths);
    for p in json_paths {
        let Some(bytes) = read(&p) else { continue };
        let Ok(j) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if j.get("schema").and_then(|v| v.as_str())
            == Some("darkbot.g07.builder-requirements-refresh.v1")
        {
            let mut times = Vec::new();
            fetch_times(&j, &mut times);
            if let Some(oldest) = times.into_iter().min() {
                if oldest > builder.refresh_observed_unix_s {
                    builder.refresh_observed_unix_s = oldest;
                    builder.refresh_expires_unix_s = oldest.saturating_add(86400);
                }
            }
        }
        if p.file_name().is_some_and(|n| n == "probe.evidence.json") {
            let t = j
                .get("cases")
                .and_then(|v| v.as_array())
                .and_then(|rows| {
                    rows.iter()
                        .filter_map(|v| v.get("answer_unix_ms")?.as_u64())
                        .max()
                })
                .unwrap_or(0)
                / 1000;
            if builder.probe.as_ref().is_none_or(|e| t > e.observed_unix_s) {
                let mut e = evidence(root, &p, None);
                e.observed_unix_s = t;
                builder.probe = Some(e);
            }
        }
    }
    if builder.refresh_expires_unix_s < now() {
        warnings.push("BUILDER_REFRESH_EXPIRED_OR_UNQUALIFIED".into());
    }
    let mut monitoring = BscMonitoringState {
        state: if warnings.iter().any(|w| w.contains("g08")) {
            "UNKNOWN"
        } else {
            "NOT_EXECUTED"
        }
        .into(),
        ..Default::default()
    };
    if let Some((p, r)) = latest(
        &records,
        &[
            schema::g08::result::SCHEMA.name,
            schema::g08::unresolved::SCHEMA.name,
        ],
    ) {
        monitoring.state = field(r, "verdict").unwrap_or_else(|| "UNKNOWN".into());
        monitoring.evidence = Some(evidence(root, p, Some(r)));
        let names: Vec<_> = r
            .schema()
            .fields
            .iter()
            .filter(|f| !f.name.contains("journal_error"))
            .map(|f| f.name)
            .collect();
        monitoring.results = fields(r, &names);
        witness.t0_to_buy_state = field(r, "timing.T0_TO_BUY").unwrap_or_else(|| "UNKNOWN".into());
    }
    let execution = latest(&records, &[schema::g07::evidence::SCHEMA.name]);
    let g07_state = execution
        .and_then(|(_, r)| field(r, "classification"))
        .unwrap_or_else(|| {
            if warnings.iter().any(|w| w.contains("g07")) {
                "UNKNOWN"
            } else {
                "NOT_EXECUTED"
            }
            .into()
        });
    let g07_evidence = execution.map(|(p, r)| evidence(root, p, Some(r)));
    let gates = vec![
        gate(
            "G0.6a",
            &dry.state,
            dry.evidence.clone(),
            if dry.valid_for_current_portal == Some(false) {
                vec!["PORTAL_DRIFT_INVALIDATES_CURRENT_USE".into()]
            } else {
                vec![]
            },
        ),
        gate("G0.7", &g07_state, g07_evidence, vec![]),
        gate(
            "G0.8",
            &monitoring.state,
            monitoring.evidence.clone(),
            vec![],
        ),
        gate(
            "C1.9",
            "UNKNOWN",
            None,
            vec!["NOT_CONNECTED_TO_EXECUTION".into()],
        ),
        gate("C1.10", "UNKNOWN", None, vec!["NO_PHASE_ACCEPTANCE".into()]),
    ];
    Snapshot {
        status: BscStatus {
            source_commit: fs::read_to_string(root.join("COMMIT"))
                .unwrap_or_else(|_| "working-tree".into())
                .trim()
                .into(),
            evaluated_unix_s: now(),
            live_state: "BLOCKED".into(),
            guard: Some(guard),
            gates,
            readiness: Some(readiness),
            builder: Some(builder),
            witness: Some(witness),
            dry: Some(dry),
            jobs: vec![],
            warnings,
            data_as_of_unix_s: fs::read_to_string(root.join("SYNCED_AT"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0),
        },
        launch,
        monitoring,
        wallets,
    }
}

fn json_walk(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || out.len() >= 128 {
        return;
    }
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            let Ok(t) = e.file_type() else { continue };
            if t.is_symlink() {
                continue;
            }
            if t.is_dir() {
                json_walk(&e.path(), depth - 1, out);
            } else if t.is_file()
                && (e
                    .file_name()
                    .to_string_lossy()
                    .starts_with("builder_requirements_refresh_")
                    || e.file_name() == "probe.evidence.json")
            {
                out.push(e.path());
            }
        }
    }
}
fn fetch_times(v: &serde_json::Value, out: &mut Vec<u64>) {
    match v {
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                if k == "fetched_at_utc" {
                    if let Some(t) = v.as_str().and_then(utc_seconds) {
                        out.push(t);
                    }
                } else {
                    fetch_times(v, out);
                }
            }
        }
        serde_json::Value::Array(a) => {
            for v in a {
                fetch_times(v, out)
            }
        }
        _ => {}
    }
}
fn utc_seconds(s: &str) -> Option<u64> {
    // Strict UTC format emitted by the archived fetcher. Never parse a filename or mtime.
    if s.len() < 20 || !s.ends_with('Z') {
        return None;
    }
    let d: Vec<u64> = s
        .get(..19)?
        .split(['-', 'T', ':'])
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if d.len() != 6
        || !(1970..=2100).contains(&d[0])
        || !(1..=12).contains(&d[1])
        || !(1..=31).contains(&d[2])
        || d[3] > 23
        || d[4] > 59
        || d[5] > 59
    {
        return None;
    }
    let leap = |y: u64| y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let mut days = 0;
    for y in 1970..d[0] {
        days += if leap(y) { 366 } else { 365 };
    }
    let months = [
        31,
        if leap(d[0]) { 29 } else { 28 },
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
    if d[2] > months[(d[1] - 1) as usize] {
        return None;
    }
    days += months[..(d[1] - 1) as usize].iter().sum::<u64>() + d[2] - 1;
    Some(days * 86400 + d[3] * 3600 + d[4] * 60 + d[5])
}

#[cfg(test)]
mod ordering_tests {
    use super::*;
    use record::Draft;
    #[test]
    fn latest_keeps_native_millisecond_order_and_operational_paths() {
        fn report(ms: u64) -> Record {
            let mut d = Draft::new(&schema::g07::readiness::SCHEMA);
            d.set(schema::g07::readiness::STATUS, "BLOCKED")
                .set(schema::g07::readiness::MODE, "READ_ONLY_RPC")
                .set(schema::g07::readiness::OBSERVED_UNIX_MS, ms)
                .set(schema::g07::readiness::LIVE_AUTHORIZED, false);
            schema::decode_any(&d.encode().unwrap()).unwrap()
        }
        let rows = vec![
            (PathBuf::from("z-offline"), report(1792231935228)),
            (PathBuf::from("a-current-rpc"), report(1792231935242)),
        ];
        assert_eq!(
            latest(&rows, &[schema::g07::readiness::SCHEMA.name])
                .unwrap()
                .0,
            PathBuf::from("a-current-rpc")
        );
        let p = Path::new(
            "/tmp/test/run/bsc-operational/bsc/native/evidence/g07/gui_readiness_test/readiness.dbrec",
        );
        assert_eq!(
            evidence(Path::new("/tmp/source"), p, None).path,
            "bsc/native/evidence/g07/gui_readiness_test/readiness.dbrec"
        );
    }
}
