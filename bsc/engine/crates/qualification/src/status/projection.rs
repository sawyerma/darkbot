use super::{census, wire::bsc::*};
use darkbot_bsc_execution::acceptance_guard::{FROZEN_APPROVED_V5_SHA256, V5_RELATIVE_PATH};
use darkbot_bsc_record::{self as record, Kind, Presence, Record, Schema, Value, schema};
use std::{
    collections::HashMap,
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
/// Seconds since 1970; `None` when the clock is before it (BSC-050: a clock
/// fault is never time 0, which would make every deadline look unexpired).
pub fn clock_s() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}
/// [`clock_s`], 0 (the wire's "unset") without a clock.
pub fn now() -> u64 {
    clock_s().unwrap_or(0)
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
/// Details may contain external errors. URLs, credential-bearing lines and absolute local
/// paths never leave the reporter.
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
        "secret:",
        "token=",
        "token:",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return "[redacted]".into();
    }
    text.split_whitespace()
        .map(|s| {
            if s.contains("://") {
                "[endpoint]"
            } else if ["/Users/", "/home/", "/private/", "/var/folders/"]
                .iter()
                .any(|p| s.trim_start_matches(['"', '\'', '(']).starts_with(p))
            {
                "[local-path]"
            } else {
                s
            }
        })
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
/// A builder refresh is expired at `now`, and always without a clock.
pub(crate) fn refresh_expired(expires_unix_s: u64, now: Option<u64>) -> bool {
    now.is_none_or(|now| expires_unix_s < now)
}
fn time(r: &Record) -> u64 {
    time_ms(r) / 1000
}

fn native_kind(k: Kind) -> NativeKind {
    match k {
        Kind::Bool => NativeKind::Bool,
        Kind::U64 => NativeKind::U64,
        Kind::I64 => NativeKind::I64,
        Kind::U256 => NativeKind::U256,
        Kind::Address => NativeKind::Address,
        Kind::B256 => NativeKind::B256,
        Kind::Text => NativeKind::Text,
        _ => NativeKind::Unspecified,
    }
}
/// Public leaves in schema order with kind and census unit. A leaf the census classifies as
/// secret, binary or private never leaves the reporter; text is redacted. Deliberately not
/// projected either: external journal errors and the preflight copy embedded in G0.7 evidence.
fn native_leaves(
    top: &Schema,
    s: &Schema,
    values: &[(u16, Value)],
    wire: &str,
    prefix: &str,
    out: &mut Vec<NativeField>,
) {
    let mut seen: HashMap<u16, usize> = HashMap::new();
    for (tag, v) in values {
        let Some(f) = s.field(*tag) else { continue };
        let n = seen.entry(*tag).or_insert(0);
        let i = *n;
        *n += 1;
        let name = format!("{prefix}{}", f.name);
        if name.contains("journal_error") || name.starts_with("preflight[]") {
            continue;
        }
        match (v, f.nested) {
            (Value::Nested(inner), Some(nested)) => native_leaves(
                top,
                nested,
                inner,
                &format!("{wire}{}[{i}].", f.name),
                &format!("{name}[]."),
                out,
            ),
            _ => {
                if !census::public(census::sensitivity(top, &name, f.kind)) {
                    continue;
                }
                let Some(value) = value(v) else { continue };
                out.push(NativeField {
                    name: if f.presence == Presence::Repeated {
                        format!("{wire}{}[{i}]", f.name)
                    } else {
                        format!("{wire}{}", f.name)
                    },
                    kind: native_kind(f.kind) as i32,
                    value,
                    unit: census::unit(&name, f.kind).into(),
                });
            }
        }
    }
}
/// All public leaves of a record, for a mirror that names them under its own prefix.
fn public_leaves(r: &Record) -> Vec<NativeField> {
    let mut fields = Vec::new();
    native_leaves(r.schema(), r.schema(), r.fields(), "", "", &mut fields);
    fields
}
fn native_record(root: &Path, path: &Path, r: &Record) -> NativeRecord {
    let mut fields = Vec::new();
    native_leaves(r.schema(), r.schema(), r.fields(), "", "", &mut fields);
    NativeRecord {
        schema: r.schema().name.into(),
        evidence: Some(evidence(root, path, Some(r))),
        provenance: if r.schema().name.contains(".import.")
            || field(r, "_import.legacy_sha256").is_some()
        {
            "LEGACY_IMPORTED_AUDITED"
        } else if field(r, "_provenance.tool").is_some() {
            "NATIVE_CREATED"
        } else {
            "UNKNOWN"
        }
        .into(),
        fields,
    }
}
/// The latest record of each family (legacy import and native variants share one family).
fn latest_records(
    root: &Path,
    records: &[(PathBuf, Record)],
    families: &[&[&Schema]],
) -> Vec<NativeRecord> {
    families
        .iter()
        .filter_map(|family| {
            let names: Vec<_> = family.iter().map(|s| s.name).collect();
            latest(records, &names).map(|(p, r)| native_record(root, p, r))
        })
        .collect()
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

/// The offline G0.7 A/B candidate state for the GUI: the public mirror of the
/// snapshot (every ID recomputed from its public fields, never a salt) and
/// the runtime selection requests. Absent mirror: no field (GUI shows
/// BACKEND_UNAVAILABLE); any inconsistency: `candidate.backend = INVALID`.
fn candidate_fields(
    root: &Path,
    rundir: &Path,
    readiness: Option<&Record>,
    warnings: &mut Vec<String>,
) -> (Vec<PublicField>, &'static str) {
    use schema::g07_candidate as gc;
    let dir = root.join("bsc/native/state/g07_candidates");
    if !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir()) {
        return (Vec::new(), "NO_SELECTION");
    }
    let mut out = Vec::new();
    let mut put = |name: String, value: String| out.push(PublicField { name, value });
    let invalid = |warnings: &mut Vec<String>, why: &str| {
        warnings.push(format!("CANDIDATE_MIRROR_INVALID {why}"));
        (
            vec![PublicField {
                name: "candidate.backend".into(),
                value: "INVALID".into(),
            }],
            "UNKNOWN",
        )
    };
    let salted = fs::read_dir(&dir).map_or(true, |entries| {
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".salt.dbrec"))
    });
    if salted {
        return invalid(warnings, "salt record or unreadable mirror");
    }
    let selection = match read(&dir.join("selection.dbrec")) {
        None => None,
        Some(bytes) => match record::decode(&gc::selection::SCHEMA, &bytes) {
            Ok(r) => Some(r),
            Err(_) => return invalid(warnings, "selection"),
        },
    };
    let view = selection.as_ref().map(|r| gc::SelectionView {
        generation: r.u64(gc::selection::GENERATION).unwrap_or(0),
        profile: r.text(gc::selection::PROFILE).unwrap_or(""),
        candidate_id: r.b256(gc::selection::CANDIDATE_ID).unwrap_or([0; 32]),
        activated_unix_ms: r.u64(gc::selection::ACTIVATED_UNIX_MS).unwrap_or(u64::MAX),
    });
    put("candidate.backend".into(), "AVAILABLE".into());
    put(
        "candidate.selection.generation".into(),
        view.as_ref().map_or(0, |v| v.generation).to_string(),
    );
    put(
        "candidate.selection.profile".into(),
        view.as_ref().map_or("NONE", |v| v.profile).into(),
    );
    if let Some(v) = &view {
        put(
            "candidate.selection.candidate_id".into(),
            format!("0x{}", record::hex(&v.candidate_id)),
        );
        put(
            "candidate.selection.activated_unix_ms".into(),
            v.activated_unix_ms.to_string(),
        );
    }
    // The remaining public selection leaves: previous candidate, invalidations, guard at switch.
    for f in selection.as_ref().map(public_leaves).unwrap_or_default() {
        if !["generation", "profile", "candidate_id", "activated_unix_ms"]
            .contains(&f.name.as_str())
        {
            put(format!("candidate.selection.{}", f.name), f.value);
        }
    }
    let mut selected_present = view.is_none();
    for (profile, file) in [
        ("SELF_A", "a.candidate.dbrec"),
        ("SELF_B", "b.candidate.dbrec"),
    ] {
        let Some(bytes) = read(&dir.join(file)) else {
            put(format!("candidate.{profile}.state"), "ABSENT".into());
            continue;
        };
        let Ok(r) = record::decode(&gc::candidate::SCHEMA, &bytes) else {
            return invalid(warnings, profile);
        };
        let id = r.b256(gc::candidate::CANDIDATE_ID).ok();
        if gc::candidate_id_of(&r).ok() != id
            || r.text(gc::candidate::PROFILE).ok() != Some(profile)
        {
            return invalid(warnings, profile);
        }
        let id = id.unwrap_or_default();
        if view.as_ref().is_some_and(|v| v.profile == profile) {
            if view.as_ref().map(|v| v.candidate_id) != Some(id) {
                return invalid(warnings, "selection names another candidate");
            }
            selected_present = true;
        }
        put(format!("candidate.{profile}.state"), "PRESENT".into());
        // A bound requalification replaces UNQUALIFIED by the deployment it
        // qualified; one bound to another ID makes the mirror invalid.
        let stem = if profile == "SELF_A" { "a" } else { "b" };
        let qualification = match read(&dir.join(format!("{stem}.qualification.dbrec"))) {
            None => None,
            Some(bytes) => match record::decode(&gc::qualification::SCHEMA, &bytes) {
                Ok(q) if q.b256(gc::qualification::CANDIDATE_ID).ok() == Some(id) => Some(q),
                _ => return invalid(warnings, "qualification of another candidate"),
            },
        };
        let qualified = qualification.as_ref().map(|q| {
            format!(
                "QUALIFIED_FOR {}",
                field(q, "portal_version").unwrap_or_default()
            )
        });
        // Every public identity leaf of the candidate (never the salt, which lives in its own
        // record); `qualification` is replaced by the bound requalification below.
        for f in public_leaves(&r) {
            if f.name != "qualification" {
                put(format!("candidate.{profile}.{}", f.name), f.value);
            }
        }
        for f in qualification
            .as_ref()
            .map(public_leaves)
            .unwrap_or_default()
        {
            put(
                format!("candidate.{profile}.qualification_record.{}", f.name),
                f.value,
            );
        }
        put(
            format!("candidate.{profile}.qualification"),
            qualified
                .unwrap_or_else(|| field(&r, "qualification").unwrap_or_else(|| "NONE".into())),
        );
        put(
            format!("candidate.{profile}.readiness"),
            readiness
                .map_or("NONE", |rd| gc::readiness_for(rd, view.as_ref(), &id))
                .into(),
        );
    }
    if !selected_present {
        return invalid(warnings, "selected candidate missing");
    }
    // The latest runtime request and, once the desktop sync decided it, its
    // native result; requested is never shown as effective.
    let requests = rundir.join("bsc-operational/g07_candidates/requests");
    let latest_request = fs::read_dir(&requests)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".request.dbrec"))
        .filter_map(|e| record::decode(&gc::request::SCHEMA, &read(&e.path())?).ok())
        .max_by_key(|r| r.u64(gc::request::REQUESTED_UNIX_MS).unwrap_or(0));
    // The global readiness counts as current only for exactly this selection.
    let binding = match (&view, readiness) {
        (None, _) => "NO_SELECTION",
        (Some(_), None) => "UNKNOWN",
        (Some(v), Some(rd)) => match gc::readiness_for(rd, Some(v), &v.candidate_id) {
            "PASS_READ_ONLY" | "BLOCKED" | "STOP" => "BOUND",
            "STALE" => "STALE",
            _ => "UNKNOWN",
        },
    };
    match latest_request {
        None => put("candidate.request.state".into(), "NONE".into()),
        Some(r) => {
            let id = field(&r, "request_id").unwrap_or_default();
            let result = read(&requests.join(format!("{id}.result.dbrec")))
                .and_then(|b| record::decode(&gc::request_result::SCHEMA, &b).ok());
            put(
                "candidate.request.state".into(),
                result
                    .as_ref()
                    .and_then(|x| field(x, "outcome"))
                    .unwrap_or_else(|| "PENDING_DESKTOP_SYNC".into()),
            );
            put("candidate.request.request_id".into(), id);
            for f in public_leaves(&r) {
                if f.name != "request_id" {
                    put(format!("candidate.request.{}", f.name), f.value);
                }
            }
            if let Some(detail) = result.as_ref().and_then(|x| field(x, "detail")) {
                put("candidate.request.detail".into(), detail);
            }
            for f in result.as_ref().map(public_leaves).unwrap_or_default() {
                if !["request_id", "outcome", "detail"].contains(&f.name.as_str()) {
                    put(format!("candidate.request.result.{}", f.name), f.value);
                }
            }
        }
    }
    (out, binding)
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
    let contract_path = root.join(V5_RELATIVE_PATH);
    let contract = load(&contract_path);
    let mut guard = GuardStatus {
        frozen_approved_sha256: FROZEN_APPROVED_V5_SHA256.into(),
        guard_state: if FROZEN_APPROVED_V5_SHA256 == "UNARMED" {
            "UNARMED"
        } else {
            "ARMED_MISMATCH"
        }
        .into(),
        contract_path: V5_RELATIVE_PATH.into(),
        contract_present: contract_path.is_file(),
        contract_sha256: evidence(root, &contract_path, None).sha256,
        ..Default::default()
    };
    let mut launch = BscLaunchState {
        reason: "No active V5 contract".into(),
        ..Default::default()
    };
    if let Some(r) = contract
        .as_ref()
        .filter(|r| r.schema() == &schema::g07::contract::V5)
    {
        guard.approved = optional_bool(r, "authorization.approved");
        guard.execution_ready = optional_bool(r, "authorization.execution_ready");
        guard.attempt_id = field(r, "authorization.attempt_id").unwrap_or_default();
        guard.max_attempts = optional_u64(r, "attempts.maximum");
        if guard.guard_state != "UNARMED" && guard.contract_sha256 == FROZEN_APPROVED_V5_SHA256 {
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
                "V5_CONTRACT_INVALID"
            } else {
                "V5_CONTRACT_MISSING"
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
        // Native vocabulary unchanged; anything else is UNKNOWN, never a pass.
        let known = |name: &str, vocabulary: &[&str]| {
            field(r, name)
                .filter(|v| vocabulary.contains(&v.as_str()))
                .unwrap_or_else(|| "UNKNOWN".into())
        };
        readiness.verdict = known("status", &["PASS_READ_ONLY", "BLOCKED", "STOP"]);
        readiness.mode = known("mode", &["READ_ONLY_RPC", "OFFLINE_ZERO_EGRESS"]);
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
    // BSC-048: the builder probe from its native record only, timed by its
    // latest answer; the legacy probe/refresh JSON stays historical evidence,
    // never status input.
    if let Some((p, r)) = latest(&records, &[schema::g07::probe::SCHEMA.name]) {
        let answered = r
            .nested_all(schema::g07::probe::CASES)
            .ok()
            .and_then(|cases| {
                cases
                    .iter()
                    .filter_map(|c| c.u64(schema::g07::probe::case::ANSWER_MS).ok())
                    .max()
            })
            .unwrap_or(0)
            / 1000;
        let mut e = evidence(root, p, Some(r));
        e.observed_unix_s = answered;
        builder.probe = Some(e);
    }
    // Without a clock no refresh counts as fresh (fail closed).
    let clock = clock_s();
    if clock.is_none() {
        warnings.push("CLOCK_UNAVAILABLE".into());
    }
    if refresh_expired(builder.refresh_expires_unix_s, clock) {
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
    let g08_names = [
        schema::g08::result::SCHEMA.name,
        schema::g08::unresolved::SCHEMA.name,
    ];
    let g08_records = records
        .iter()
        .filter(|(_, r)| g08_names.contains(&r.schema().name))
        .count();
    let g08 = latest(&records, &g08_names);
    if let Some((p, r)) = g08 {
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
    let candidate_readiness =
        latest(&records, &[schema::g07::readiness::SCHEMA.name]).map(|(_, r)| r);
    let (candidate, binding) = candidate_fields(root, rundir, candidate_readiness, &mut warnings);
    launch.runtime_fields.extend(candidate);
    if candidate_readiness.is_some() {
        readiness.candidate_binding = binding.into();
    } else {
        readiness.candidate_binding = "UNKNOWN".into();
    }
    // G0.7 and G0.8 only belong together with the same attempt and, for a result, the hash
    // of exactly that G0.7 record; otherwise both stay independent observations.
    let mut g07_reasons = Vec::new();
    if let Some((_, g07)) = execution
        && !guard.attempt_id.is_empty()
        && field(g07, "attempt_id").as_deref() != Some(&guard.attempt_id)
    {
        g07_reasons.push("ATTEMPT_ID_NOT_V5_BOUND".into());
    }
    let mut g08_reasons = Vec::new();
    if let Some((_, r)) = g08 {
        let bound = execution.is_some_and(|(_, g07)| {
            field(g07, "attempt_id").is_some()
                && field(g07, "attempt_id") == field(r, "attempt_id")
                && (r.schema() == &schema::g08::unresolved::SCHEMA
                    || g07_evidence.as_ref().is_some_and(|e| {
                        field(r, "g07_evidence_sha256") == Some(format!("0x{}", e.sha256))
                    }))
        });
        g08_reasons.push(
            if bound {
                "G07_EVIDENCE_BOUND"
            } else {
                "G07_EVIDENCE_UNBOUND"
            }
            .into(),
        );
        // V2 §2.3 (review MEDIUM-2): a G0.8 verdict counts only alone and bound
        // to its G0.7 evidence; an unbound one, or one of several G0.8 records
        // (no record orders them in time), is UNRESOLVED, never PASS/ADAPT/STOP.
        if g08_records > 1 {
            g08_reasons.push("G08_RECORDS_AMBIGUOUS".into());
        }
        if (!bound || g08_records > 1) && monitoring.state != "UNRESOLVED" {
            let recorded = ["PASS", "ADAPT", "STOP"]
                .into_iter()
                .find(|v| *v == monitoring.state)
                .unwrap_or("UNKNOWN");
            g08_reasons.push(format!("RECORDED_VERDICT_{recorded}"));
            monitoring.state = "UNRESOLVED".into();
        }
        // BSC-011: the chain outcome and the evidence integrity next to the
        // verdict; anything unknown stays UNKNOWN. An UNRESOLVED record (no
        // canonical receipts) proves neither: both UNKNOWN, never a PASS,
        // ADAPT or "not sent".
        if r.schema() == &schema::g08::unresolved::SCHEMA {
            g08_reasons.extend(["CHAIN_UNKNOWN".into(), "EVIDENCE_UNKNOWN".into()]);
        }
        if r.schema() == &schema::g08::result::SCHEMA {
            for (name, prefix, vocabulary) in [
                (
                    "verdict_dimensions.chain_outcome",
                    "CHAIN_",
                    &["PASS", "ADAPT", "STOP", "UNKNOWN"][..],
                ),
                (
                    "verdict_dimensions.evidence_integrity",
                    "EVIDENCE_",
                    &[
                        "COMPLETE",
                        "LOCAL_CAPTURE_INCOMPLETE",
                        "CHAIN_CAPTURE_INCOMPLETE",
                        "LOCAL_AND_CHAIN_CAPTURE_INCOMPLETE",
                    ][..],
                ),
            ] {
                let value = field(r, name)
                    .filter(|v| vocabulary.contains(&v.as_str()))
                    .unwrap_or_else(|| "UNKNOWN".into());
                g08_reasons.push(format!("{prefix}{value}"));
            }
        }
    }
    {
        use schema::{g06a, g07, g08};
        launch.records = latest_records(
            root,
            &records,
            &[
                &[&g06a::dry::SCHEMA, &g06a::dry::NATIVE],
                &[&g06a::preflight::SCHEMA],
                &[&g06a::quote::SCHEMA],
                &[&g06a::economics::SCHEMA, &g06a::economics::NATIVE],
                &[
                    &g06a::local_simulation::SCHEMA,
                    &g06a::local_simulation::NATIVE,
                ],
                &[&g06a::launch_path::SCHEMA],
                &[&g06a::portal_upgrade::SCHEMA],
                &[&g06a::requalification::SCHEMA],
                &[&g06a::prepare_handoff::SCHEMA],
                // Typed in ReadinessStatus; the record carries the remaining leaves.
                &[&g07::readiness::SCHEMA],
            ],
        );
        if let Some(r) = contract
            .as_ref()
            .filter(|r| r.schema() == &g07::contract::V5)
        {
            launch.records.push(native_record(root, &contract_path, r));
        }
        monitoring.records = latest_records(
            root,
            &records,
            &[
                &[&g06a::builder_policy::SCHEMA, &g06a::builder_policy::NATIVE],
                &[&g07::probe::SCHEMA],
                &[&g07::witness_source::SCHEMA],
                &[&g07::evidence::SCHEMA],
                &[&g08::result::SCHEMA, &g08::unresolved::SCHEMA],
                &[&g08::chain_session::SCHEMA],
                &[&g08::producer_witness::SCHEMA],
                &[&g08::timing_verdict::SCHEMA],
            ],
        );
    }
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
        gate("G0.7", &g07_state, g07_evidence, g07_reasons),
        gate(
            "G0.8",
            &monitoring.state,
            monitoring.evidence.clone(),
            g08_reasons,
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
