//! DEC-18 §3 (P0-022, approval C-11): the provider pre-measurement `p0ctl provider measure`.
//! Before the network audit, each RPC method and WSS subscription the approved modes' code calls
//! — the BOOT audit, the 600-s econ4h calibration and the 30-min C0-MO observation, derived from
//! their call sites and pinned by a drift test — is read once, minimally, against the one
//! selected FREE_ONLY pair, inside the C-11 bounds (`measure_*`), after the operator approved
//! exactly the plan's offline token. A plan is used once. The result is a typed measurement
//! record and the `PROVIDER_EVIDENCE` derived from it (origin `PROVIDER_MEASUREMENT`); a read a
//! mode needs and that was not positively proven blocks that mode only (its BOOT audit refuses).
//!
//! * `<project>/data/provider_measure/<plan_id>/plan.p0j`: the plan (its token: the first 16 hex
//!   digits of its bytes' SHA-256); `run.started` marks its one run.
//! * `<project>/data/provider_measure/results/<id>.p0j`: a measurement, content-addressed.
pub use super::provider_measurement::*;
use crate::analytics::journal;
use darkbot_solana::providers::ProviderKind;
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, Instant};

const PLAN: &str = "plan.p0j";
const STARTED: &str = "run.started";
const RESULTS: &str = "results";

fn sha(bytes: &[u8]) -> String {
    journal::hex(&solana_sdk::hash::hash(bytes).to_bytes())
}

/// The RPC methods each mode's production path calls (DEC-18 §3.2; the test
/// `p0_022_the_probe_spec_is_the_code_s` pins them to the call sites). `sendTransaction`,
/// signing and value-moving reads are never probed; econ4h runs with `--chain-verify off` (no
/// history reads).
fn rpc_methods(mode: Mode) -> &'static [&'static str] {
    match mode {
        Mode::Boot => &[
            "getGenesisHash",
            "getVersion",
            "getHealth",
            "getSlot",
            "getMultipleAccounts",
        ],
        Mode::Econ4h => &[
            "getGenesisHash",
            "getAccountInfo",
            "getProgramAccounts",
            "getMultipleAccounts",
            "getLatestBlockhash",
            "getFeeForMessage",
            "simulateTransaction",
            "getSignaturesForAddress",
            "getTransaction",
        ],
        Mode::C0Mo => &["getAccountInfo"],
    }
}

/// The subscription messages each mode's sessions send — built by the modes' own builders, so
/// the probe sends exactly these (BOOT: the audit's liveness; econ4h: the owner's dependency set
/// and the market capture; C0-MO: the market capture). The owner's one dependency account is the
/// Clock sysvar: a required dependency of every owner, notified every slot.
fn wss_messages(mode: Mode, program: Pubkey, group: &Pubkey) -> Vec<Value> {
    let account = solana_sdk::sysvar::clock::ID;
    use crate::market_observer::{market_subscription, MarketSource};
    use darkbot_solana::ingress::ws_rpc::{audit_subscription, subscribe_messages};
    let state = darkbot_solana::state::STATE_COMMITMENT;
    let market = crate::market_observer::MARKET_COMMITMENT_DEFAULT;
    let owner = || super::state_owner::owner_subscription(program, group, vec![account]);
    match mode {
        Mode::Boot => subscribe_messages(&audit_subscription(program, false), state),
        Mode::Econ4h => {
            let mut m = subscribe_messages(&owner(), state);
            m.extend(subscribe_messages(
                &market_subscription(MarketSource::Logs, program),
                market,
            ));
            m
        }
        Mode::C0Mo => subscribe_messages(&market_subscription(MarketSource::Logs, program), market),
    }
}

fn wss_read(message: &Value) -> Read {
    let method = message["method"].as_str().unwrap_or_default().to_string();
    let params = message["params"].as_array();
    let commitment = params
        .and_then(|p| p.iter().find_map(|v| v["commitment"].as_str()))
        .unwrap_or("-")
        .to_string();
    Read::Wss(method, commitment)
}

fn now_ms() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// The measurement's configuration: the project's registry selection, one RPC and one WSS, no
/// fallback, its rate, attempts and bytes the C-11 bounds (`measure_*`).
fn configure(
    project: &Path,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<
    (
        darkbot_solana::runtime_config::Resolved,
        darkbot_solana::providers::ProviderRegistry,
    ),
    String,
> {
    let mut resolved = darkbot_solana::runtime_config::Resolved::parse(&[], get)?;
    let registry = super::provider_registry::bind(project, &mut resolved)?;
    let c = &mut resolved.config;
    if c.fallback || c.rpc_endpoints.len() != 1 || c.wss_endpoints.len() != 1 {
        return Err("MEASURE_PAIR_REQUIRED: exactly one selected RPC and one WSS provider, no fallback (p0ctl provider select)".into());
    }
    let l = &mut c.limits;
    l.rpc_rps = l.measure_rps;
    l.bootstrap_rps = l.measure_rps;
    l.rpc_burst = 1;
    l.rpc_concurrency = 1;
    l.rpc_attempts = 1;
    l.max_requests = l.measure_attempts;
    l.max_total_mib = l.measure_mib;
    l.max_response_mib = l.max_response_mib.min(l.measure_mib);
    l.wss_message_mib = l.wss_message_mib.min(l.measure_mib);
    for kind in [ProviderKind::Rpc, ProviderKind::Wss] {
        let endpoint = match kind {
            ProviderKind::Rpc => &c.rpc_endpoints[0],
            _ => &c.wss_endpoints[0],
        };
        super::provider_evidence::free_registry_provider(c.bound(kind, endpoint)?)?;
    }
    Ok((resolved, registry))
}

fn provider_ref(
    c: &darkbot_solana::runtime_config::RuntimeConfig,
    kind: ProviderKind,
) -> Result<ProviderRef, String> {
    let endpoint = match kind {
        ProviderKind::Rpc => &c.rpc_endpoints[0],
        _ => &c.wss_endpoints[0],
    };
    let b = c.bound(kind, endpoint)?;
    Ok(ProviderRef {
        id: b.id.clone(),
        binding: b.binding.clone(),
    })
}

fn program_and_group(
    c: &darkbot_solana::runtime_config::RuntimeConfig,
) -> Result<(Pubkey, Pubkey), String> {
    let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET)
        .map_err(|_| "MEASURE_PROGRAM")?;
    let group = Pubkey::from_str(&c.fe_group).map_err(|_| "MEASURE_GROUP: fe_group")?;
    Ok((program, group))
}

/// The RPC probes: each method once with minimal parameters fitting its calls — where the modes
/// read a method with different encodings, the most demanding one (base64+zstd); a fixture a
/// later probe needs comes from an earlier one (a blockhash, a signature).
fn rpc_params(method: &str, program: &Pubkey, group: &Pubkey) -> String {
    let filters = super::state_owner::bank_filters(group).len();
    match method {
        "getGenesisHash" | "getVersion" | "getHealth" => "[]".into(),
        "getSlot" => "[{commitment:finalized}]".into(),
        "getMultipleAccounts" => format!("[[{program}],{{base64+zstd,confirmed,dataSlice 0..0}}]"),
        "getAccountInfo" => "[ProgramData,{base64+zstd,confirmed,dataSlice 0..0}]".into(),
        "getProgramAccounts" => {
            format!("[{program},{{base64+zstd,confirmed,withContext,{filters} bank filters,dataSlice 0..0}}]")
        }
        "getLatestBlockhash" => "[{commitment:confirmed}]".into(),
        "getFeeForMessage" => "[an empty message of the group as payer at the latest blockhash,{confirmed}]".into(),
        "simulateTransaction" => "[that message unsigned,{sigVerify:false,replaceRecentBlockhash:true,confirmed,base64}] (UNSIGNED_SIMULATION)".into(),
        "getSignaturesForAddress" => format!("[{program},{{limit:1,confirmed}}]"),
        "getTransaction" => "[the newest signature,{json,confirmed,maxSupportedTransactionVersion:1}]".into(),
        other => format!("[{other}: no probe]"),
    }
}

/// The reads of every mode, deduplicated in order: the plan's probes.
fn reads_of(program: Pubkey, group: &Pubkey) -> (Vec<ModeReads>, Vec<Read>, Vec<Value>) {
    let mut modes = Vec::new();
    let mut order: Vec<Read> = Vec::new();
    for mode in Mode::ALL {
        let mut reads: Vec<Read> = rpc_methods(mode)
            .iter()
            .map(|m| Read::Rpc((*m).into()))
            .collect();
        for m in wss_messages(mode, program, group) {
            let read = wss_read(&m);
            if !reads.contains(&read) {
                reads.push(read.clone());
            }
            if !order.contains(&read) {
                order.push(read);
            }
        }
        modes.push(ModeReads { mode, reads });
    }
    let mut rpc: Vec<Read> = Vec::new();
    for m in &modes {
        for r in &m.reads {
            if matches!(r, Read::Rpc(_)) && !rpc.contains(r) {
                rpc.push(r.clone());
            }
        }
    }
    rpc.extend(order);
    let messages = wss_probes(program, group)
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    (modes, rpc, messages)
}

/// The subscription messages of every mode, each once (its request id aside), in order. Two
/// messages of one read — the owner's two program filters — are both probed: the read is
/// SUPPORTED once either delivers an event (the session needs either's accounts).
fn wss_probes(program: Pubkey, group: &Pubkey) -> Vec<(Read, Value)> {
    let mut probes: Vec<(Read, Value)> = Vec::new();
    for mode in Mode::ALL {
        for m in wss_messages(mode, program, group) {
            if !probes
                .iter()
                .any(|(_, o)| o["method"] == m["method"] && o["params"] == m["params"])
            {
                probes.push((wss_read(&m), m));
            }
        }
    }
    probes
}

/// The probes of `order`: each RPC method's params, each subscription as its session sends it,
/// bound to the SHA-256 of its exact messages (method and params, request id aside) — a changed
/// subscription is another probe (review 02.11.2026).
fn probe_spec(order: &[Read], messages: &[Value], program: &Pubkey, group: &Pubkey) -> Vec<Probe> {
    order
        .iter()
        .map(|read| Probe {
            read: read.clone(),
            params: match read {
                Read::Rpc(m) => rpc_params(m, program, group),
                Read::Wss(m, c) => {
                    let own: Vec<Value> = messages
                        .iter()
                        .filter(|v| wss_read(v) == *read)
                        .map(|v| json!([v["method"], v["params"]]))
                        .collect();
                    let note = if m == "accountSubscribe" {
                        ", the Clock sysvar (an owner dependency, notified every slot)".to_string()
                    } else if own.len() > 1 {
                        format!(
                            ", its {} messages one after another (SUPPORTED once one delivers an event and none is refused)",
                            own.len()
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "as its session sends it, commitment {c}{note}; messages sha256 {}",
                        &sha(Value::Array(own).to_string().as_bytes())[..16]
                    )
                }
            },
        })
        .collect()
}

/// A subscription acknowledged without an event in its window.
const NO_EVENT: &str = "NO_EVENT_IN_WINDOW";

fn subscription_types(reads: &[Read]) -> usize {
    let mut types: Vec<&str> = reads
        .iter()
        .filter_map(|r| match r {
            Read::Wss(m, _) => Some(m.as_str()),
            _ => None,
        })
        .collect();
    types.sort_unstable();
    types.dedup();
    types.len()
}

fn plan_dir(project: &Path, plan_id: &str) -> Result<PathBuf, String> {
    if !super::provider_evidence::is_id(plan_id) {
        return Err("MEASURE_PLAN_ID_INVALID: a plan id is 16 lowercase hex digits".into());
    }
    Ok(project.join(STORE).join(plan_id))
}

/// The plan's token: the first 16 hex digits of its record's SHA-256.
pub fn token(record: &[u8]) -> String {
    sha(record)[..16].to_string()
}

/// `p0ctl provider measure plan`: the offline step. Resolves the selected pair and the bounds,
/// derives the probes and refuses a plan beyond the bounds; nothing is read from the network and
/// no credential is opened. Returns the plan and its token.
pub fn plan(
    project: &Path,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<(ProbePlan, String), String> {
    let (resolved, registry) = configure(project, get)?;
    let c = &resolved.config;
    let (program, group) = program_and_group(c)?;
    let (modes, order, messages) = reads_of(program, &group);
    let limits = MeasureLimits::of(&c.limits);
    let rpc_probes = order.iter().filter(|r| matches!(r, Read::Rpc(_))).count() as u64;
    let subscriptions = messages.len() as u64;
    let types = subscription_types(&order) as u64;
    // The probes fit the bounds: one attempt each, paced, every subscription in its window.
    let seconds =
        rpc_probes.div_ceil(limits.rps.max(1)) + subscriptions * limits.subscription_seconds;
    if rpc_probes > limits.attempts
        || types > limits.subscription_types
        || limits.subscriptions_active < 1
        || seconds > limits.seconds
    {
        return Err(format!(
            "MEASURE_PLAN_EXCEEDS_BOUNDS: {rpc_probes} RPC probes, {types} subscription types in {subscriptions} subscriptions, about {seconds} s against {}",
            limits.text()
        ));
    }
    let probes = probe_spec(&order, &messages, &program, &group);
    let plan_id =
        sha(crate::orchestration::provider_registry::fresh_nonce()?.as_bytes())[..16].to_string();
    let plan = ProbePlan {
        plan_id: plan_id.clone(),
        created_unix_ms: now_ms(),
        registry_version: registry.version,
        rpc: provider_ref(c, ProviderKind::Rpc)?,
        wss: provider_ref(c, ProviderKind::Wss)?,
        config_digest: sha(
            &bincode::serialize(&resolved.effective()).map_err(|_| "MEASURE_CONFIG")?
        ),
        binary_sha256: executable_sha256()?,
        limits,
        program: program.to_string(),
        group: group.to_string(),
        modes,
        probes,
    };
    let dir = plan_dir(project, &plan_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("MEASURE_STORE: {e}"))?;
    let path = dir.join(PLAN);
    journal::create_typed_value(&path, journal::source::PROVIDER_MEASUREMENT, &plan)
        .map_err(|e| format!("MEASURE_STORE: {e}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("MEASURE_STORE: {e}"))?;
    Ok((plan, token(&bytes)))
}

/// SHA-256 of the running executable (a plan binds the code that runs it).
pub fn executable_sha256() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|_| "MEASURE_BINARY")?;
    journal::file_sha256(&exe)
        .map(|h| journal::hex(&h))
        .map_err(|_| "MEASURE_BINARY".into())
}

/// The plan of `plan_id` and its bytes.
pub fn load_plan(project: &Path, plan_id: &str) -> Result<(ProbePlan, Vec<u8>), String> {
    let path = plan_dir(project, plan_id)?.join(PLAN);
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "MEASURE_PLAN_UNKNOWN".to_string(),
        _ => format!("MEASURE_STORE: {e}"),
    })?;
    let plan: ProbePlan =
        journal::read_typed_value(&path).map_err(|_| "MEASURE_PLAN_FORMAT".to_string())?;
    // The bytes decoded are the ones named, and a plan is its own directory's (a copy is no
    // second plan).
    if std::fs::read(&path).map_err(|e| format!("MEASURE_STORE: {e}"))? != bytes
        || plan.plan_id != plan_id
    {
        return Err("MEASURE_PLAN_CHANGED".into());
    }
    Ok((plan, bytes))
}

/// One RPC probe's outcome from its call's answer (DEC-18 §3.3).
fn rpc_outcome(r: &Result<Value, darkbot_solana::provider::CallError>) -> Outcome {
    use darkbot_solana::provider::CallError;
    match r {
        Ok(_) => Outcome::Supported,
        Err(CallError::Remote(e)) => {
            let code = match e.kind.as_ref() {
                solana_client::client_error::ClientErrorKind::RpcError(
                    solana_client::rpc_request::RpcError::RpcResponseError { code, .. },
                ) => Some(*code),
                _ => None,
            };
            match code {
                Some(-32601) => Outcome::MethodUnsupported("RPC -32601 method not found".into()),
                Some(-32429) => Outcome::Blocked("RPC_RATE_LIMIT".into()),
                Some(c) => Outcome::NotProven(format!("RPC_ERROR {c}")),
                None => Outcome::NotProven("RPC_ERROR".into()),
            }
        }
        Err(CallError::Budget("RPC_RATE_LIMIT") | CallError::Deadline("RPC_RATE_LIMIT")) => {
            Outcome::Blocked("RPC_RATE_LIMIT".into())
        }
        Err(CallError::Transport("RPC_UNAUTHORIZED")) => {
            Outcome::Blocked("RPC_UNAUTHORIZED".into())
        }
        Err(CallError::Budget(code)) => Outcome::NotProven(format!("BUDGET_EXHAUSTED {code}")),
        Err(CallError::Deadline(code)) => Outcome::NotProven(format!("TIMEOUT {code}")),
        Err(CallError::Transport(code)) => Outcome::NotProven((*code).into()),
    }
}

/// The JSON-RPC parameters of a probe (the external wire), with the fixtures found so far.
struct Fixtures {
    signature: Option<String>,
    blockhash: Option<solana_sdk::hash::Hash>,
}

fn rpc_request(
    method: &str,
    program: &Pubkey,
    group: &Pubkey,
    f: &Fixtures,
) -> Result<(solana_client::rpc_request::RpcRequest, Value), String> {
    use base64::Engine;
    use solana_client::rpc_request::RpcRequest as R;
    let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    let empty =
        |blockhash| solana_sdk::message::Message::new_with_blockhash(&[], Some(group), &blockhash);
    let none = || "FIXTURE_MISSING".to_string();
    Ok(match method {
        "getGenesisHash" => (R::GetGenesisHash, json!([])),
        "getVersion" => (R::GetVersion, json!([])),
        "getHealth" => (R::GetHealth, json!([])),
        "getSlot" => (R::GetSlot, json!([{"commitment":"finalized"}])),
        "getMultipleAccounts" => (
            R::GetMultipleAccounts,
            json!([[program.to_string()], {"encoding":"base64+zstd","commitment":"confirmed","dataSlice":{"offset":0,"length":0}}]),
        ),
        "getAccountInfo" => (
            R::GetAccountInfo,
            json!([darkbot_project0::protocol::program_data_address(program).to_string(), {"encoding":"base64+zstd","commitment":"confirmed","dataSlice":{"offset":0,"length":0}}]),
        ),
        "getProgramAccounts" => {
            let filters: Vec<Value> = super::state_owner::bank_filters(group)
                .iter()
                .map(darkbot_solana::ingress::ProgramFilter::wire)
                .collect();
            (
                R::GetProgramAccounts,
                json!([program.to_string(), {"encoding":"base64+zstd","commitment":"confirmed","withContext":true,"filters":filters,"dataSlice":{"offset":0,"length":0}}]),
            )
        }
        "getLatestBlockhash" => (R::GetLatestBlockhash, json!([{"commitment":"confirmed"}])),
        "getFeeForMessage" => {
            let message = empty(f.blockhash.ok_or_else(none)?);
            (
                R::GetFeeForMessage,
                json!([b64(&message.serialize()), {"commitment":"confirmed"}]),
            )
        }
        "simulateTransaction" => {
            // UNSIGNED_SIMULATION (DEC-18 §4.3): a default signature, never verified, never sent.
            let tx = solana_sdk::transaction::Transaction {
                signatures: vec![solana_sdk::signature::Signature::default()],
                message: empty(f.blockhash.ok_or_else(none)?),
            };
            let wire = bincode::serialize(&tx).map_err(|_| "MEASURE_ENCODE")?;
            (
                R::SimulateTransaction,
                json!([b64(&wire), {"sigVerify":false,"replaceRecentBlockhash":true,"commitment":"confirmed","encoding":"base64"}]),
            )
        }
        "getSignaturesForAddress" => (
            R::GetSignaturesForAddress,
            json!([program.to_string(), {"limit":1,"commitment":"confirmed"}]),
        ),
        "getTransaction" => (
            R::GetTransaction,
            json!([f.signature.clone().ok_or_else(none)?, {"encoding":"json","commitment":"confirmed","maxSupportedTransactionVersion":1}]),
        ),
        other => return Err(format!("MEASURE_NO_PROBE {other}")),
    })
}

/// Fixtures a later probe takes from an earlier answer.
fn take_fixtures(method: &str, value: &Value, f: &mut Fixtures) {
    match method {
        "getLatestBlockhash" => {
            f.blockhash = value["value"]["blockhash"]
                .as_str()
                .and_then(|h| solana_sdk::hash::Hash::from_str(h).ok());
        }
        "getSignaturesForAddress" => {
            f.signature = value
                .as_array()
                .and_then(|a| a.first())
                .and_then(|s| s["signature"].as_str())
                .map(String::from);
        }
        _ => {}
    }
}

/// `p0ctl provider measure run --plan ID --approve TOKEN`: the measurement of exactly the
/// approved plan, once. Refused before any read when the token is another plan's, the plan ran
/// before, or the pair, its credential revisions, the configuration or the executable moved
/// since the plan. Writes the measurement and imports its evidence; returns both ids.
pub fn run(
    project: &Path,
    plan_id: &str,
    approve: &str,
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<(ProviderMeasurement, String, String), String> {
    let (plan, bytes) = load_plan(project, plan_id)?;
    if approve.len() != 16 || token(&bytes) != approve {
        return Err(
            "MEASURE_APPROVAL_MISMATCH: the approval names another plan's token; nothing read"
                .into(),
        );
    }
    let (resolved, registry) = configure(project, get)?;
    let c = resolved.config.clone();
    let drift = |what: &str| {
        format!("MEASURE_PLAN_DRIFT: {what} changed since the plan; a new plan is required (nothing read)")
    };
    if registry.version != plan.registry_version
        || provider_ref(&c, ProviderKind::Rpc)? != plan.rpc
        || provider_ref(&c, ProviderKind::Wss)? != plan.wss
    {
        return Err(drift("the provider pair or its credential revisions"));
    }
    if sha(&bincode::serialize(&resolved.effective()).map_err(|_| "MEASURE_CONFIG")?)
        != plan.config_digest
    {
        return Err(drift("the configuration"));
    }
    if executable_sha256()? != plan.binary_sha256 {
        return Err(drift("the measuring executable"));
    }
    // One run per plan: an aborted or crashed run leaves its mark (a new attempt is a new plan).
    let dir = plan_dir(project, plan_id)?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(STARTED))
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                "MEASURE_PLAN_USED: this plan ran before (a new attempt is a new plan)".to_string()
            }
            _ => format!("MEASURE_STORE: {e}"),
        })?;
    resolved.activate()?;
    let c = darkbot_solana::runtime_config::active();
    super::provider_registry::install_credentials(&registry, c)?;
    let measurement = measure(&plan, &bytes, c)?;
    store(project, &plan, measurement)
}

/// Every probe of `plan`, within its bounds (the active configuration is the plan's).
fn measure(
    plan: &ProbePlan,
    bytes: &[u8],
    c: &darkbot_solana::runtime_config::RuntimeConfig,
) -> Result<ProviderMeasurement, String> {
    let (program, group) = (
        Pubkey::from_str(&plan.program).map_err(|_| "MEASURE_PROGRAM")?,
        Pubkey::from_str(&plan.group).map_err(|_| "MEASURE_GROUP")?,
    );
    let started_unix_ms = now_ms();
    let start = Instant::now();
    let deadline = start + Duration::from_secs(plan.limits.seconds);
    let rpc = darkbot_solana::provider::DeadlineRpc::new(
        c.rpc_endpoints[0].clone(),
        darkbot_solana::provider::Workload::Bootstrap,
    )?;
    let mut f = Fixtures {
        signature: None,
        blockhash: None,
    };
    let mut results = Vec::new();
    let (mut served, mut rate_limited) = (0u64, false);
    for probe in plan
        .probes
        .iter()
        .filter(|p| matches!(p.read, Read::Rpc(_)))
    {
        let Read::Rpc(method) = &probe.read else {
            continue;
        };
        let (attempts, bytes_before, t) = (rpc.request_count(), rpc.bytes(), Instant::now());
        let outcome = if Instant::now() >= deadline {
            Outcome::NotProven("MEASURE_DEADLINE".into())
        } else {
            match rpc_request(method, &program, &group, &f) {
                Err(reason) => Outcome::NotProven(reason),
                Ok((request, params)) => {
                    let answer = rpc.call(request, params, deadline);
                    if let Ok(v) = &answer {
                        served += 1;
                        take_fixtures(method, v, &mut f);
                    }
                    let o = rpc_outcome(&answer);
                    rate_limited |= o == Outcome::Blocked("RPC_RATE_LIMIT".into());
                    o
                }
            }
        };
        results.push(ProbeResult {
            read: probe.read.clone(),
            outcome,
            attempts: (rpc.request_count() - attempts) as u64,
            bytes: rpc.bytes() - bytes_before,
            elapsed_ms: t.elapsed().as_millis() as u64,
        });
    }
    // The subscriptions over one connection, one at a time.
    let probes = wss_probes(program, &group);
    let messages: Vec<Value> = probes.iter().map(|(_, m)| m.clone()).collect();
    let window = Duration::from_secs(plan.limits.subscription_seconds);
    let report =
        darkbot_solana::ingress::ws_probe::probe(&c.wss_endpoints[0], &messages, window, deadline);
    use darkbot_solana::ingress::ws_probe::{Stop, WsProbeOutcome as W};
    rate_limited |= report.retry_after_ms.is_some();
    let mut wss: Vec<ProbeResult> = Vec::new();
    for (k, ((read, _), o)) in probes.iter().zip(&report.outcomes).enumerate() {
        let outcome = match (o, report.connect_error) {
            (W::Supported { .. }, _) => Outcome::Supported,
            (W::AckOnly { .. }, _) => Outcome::NotProven(NO_EVENT.into()),
            (W::Refused { code: -32601 }, _) => {
                Outcome::MethodUnsupported("WSS -32601 method not found".into())
            }
            (W::Refused { code }, _) => Outcome::NotProven(format!("WSS_ERROR {code}")),
            (W::NoAck, _) => Outcome::NotProven("NO_ACK_IN_WINDOW".into()),
            (W::Transport(code), _) => Outcome::NotProven((*code).into()),
            (W::NotRun, Some("WSS_UNAUTHORIZED")) => Outcome::Blocked("WSS_UNAUTHORIZED".into()),
            (W::NotRun, Some(_)) if report.retry_after_ms.is_some() => {
                Outcome::Blocked("WSS_RATE_LIMIT".into())
            }
            (W::NotRun, Some(code)) => Outcome::NotProven(code.into()),
            (W::NotRun, None) => match report.stopped {
                Some(Stop::Transport) => Outcome::NotProven("WSS_STOPPED_AFTER_TRANSPORT".into()),
                Some(Stop::Unacknowledged) => Outcome::Blocked(
                    "WSS_STOPPED: an unacknowledged subscription (one at a time)".into(),
                ),
                Some(Stop::UnconfirmedUnsubscribe) => Outcome::Blocked(
                    "WSS_STOPPED: an unconfirmed unsubscribe (one at a time)".into(),
                ),
                None => Outcome::NotProven("MEASURE_DEADLINE".into()),
            },
        };
        let elapsed_ms = match o {
            W::Supported { event_ms, .. } => *event_ms,
            W::AckOnly { ack_ms } => *ack_ms,
            _ => 0,
        };
        let (attempts, bytes) = (
            u64::from(!matches!(o, W::NotRun)),
            report.probe_bytes.get(k).copied().unwrap_or(0),
        );
        match wss.iter_mut().find(|p| &p.read == read) {
            // A further message of the read. The session sends every one: a refusal, a missing
            // acknowledgement, a transport error or a stop of any is the read's verdict; only no
            // event in the window yields to another message's event (SUPPORTED, timed by it).
            Some(p) => {
                p.attempts += attempts;
                p.bytes += bytes;
                let quiet = |o: &Outcome| *o == Outcome::NotProven(NO_EVENT.into());
                let soft = |o: &Outcome| *o == Outcome::Supported || quiet(o);
                if soft(&p.outcome) && (!soft(&outcome) || outcome == Outcome::Supported) {
                    p.outcome = outcome;
                    p.elapsed_ms = elapsed_ms;
                }
            }
            None => wss.push(ProbeResult {
                read: read.clone(),
                outcome,
                attempts,
                bytes,
                elapsed_ms,
            }),
        }
    }
    results.extend(wss);
    let modes = plan
        .modes
        .iter()
        .map(|m| ModeVerdict {
            mode: m.mode,
            outcome: m
                .reads
                .iter()
                .find_map(|r| {
                    results
                        .iter()
                        .find(|p| &p.read == r)
                        .map(|p| p.outcome.clone())
                        .filter(|o| *o != Outcome::Supported)
                        .or_else(|| {
                            (!results.iter().any(|p| &p.read == r))
                                .then(|| Outcome::NotProven(format!("NOT_PROBED {}", r.text())))
                        })
                })
                .unwrap_or(Outcome::Supported),
        })
        .collect();
    Ok(ProviderMeasurement {
        plan_id: plan.plan_id.clone(),
        plan_sha256: sha(bytes),
        rpc: plan.rpc.clone(),
        wss: plan.wss.clone(),
        started_unix_ms,
        finished_unix_ms: now_ms(),
        results,
        mode_reads: plan.modes.clone(),
        modes,
        capture_source: CAPTURE_SOURCE.into(),
        simulation_signing: "UNSIGNED_SIMULATION".into(),
        rpc_attempts: rpc.request_count() as u64,
        rpc_bytes: rpc.bytes(),
        wss_frames: report.frames,
        wss_bytes: report.bytes,
        rate_limited,
        // The rate the reads were served at; nothing served proves no rate.
        allowed_rps: if rate_limited || served == 0 {
            0
        } else {
            plan.limits.rps
        },
        served_calls: served,
    })
}

/// The market capture source the subscriptions are measured for.
const CAPTURE_SOURCE: &str = "logs";

/// Stores `m` content-addressed and imports the evidence derived from it
/// ([`derive_evidence`], origin `PROVIDER_MEASUREMENT`).
fn store(
    project: &Path,
    plan: &ProbePlan,
    m: ProviderMeasurement,
) -> Result<(ProviderMeasurement, String, String), String> {
    let results = project.join(STORE).join(RESULTS);
    std::fs::create_dir_all(&results).map_err(|e| format!("MEASURE_STORE: {e}"))?;
    let staged = results.join(format!(".{}.{}.p0j", plan.plan_id, std::process::id()));
    let _ = std::fs::remove_file(&staged);
    journal::create_typed_value(&staged, journal::source::PROVIDER_MEASUREMENT, &m)
        .map_err(|e| format!("MEASURE_STORE: {e}"))?;
    let bytes = std::fs::read(&staged).map_err(|e| format!("MEASURE_STORE: {e}"))?;
    let digest = sha(&bytes);
    let id = digest[..16].to_string();
    let target = results.join(format!("{id}.p0j"));
    let linked = std::fs::hard_link(&staged, &target);
    let _ = std::fs::remove_file(&staged);
    linked.map_err(|e| format!("MEASURE_STORE: {e}"))?;
    let evidence = derive_evidence(&m, &digest);
    let file = results.join(format!(".{}.evidence.p0j", plan.plan_id));
    let _ = std::fs::remove_file(&file);
    journal::create_typed_value(&file, journal::source::PROVIDER_MEASUREMENT, &evidence)
        .map_err(|e| format!("MEASURE_STORE: {e}"))?;
    let evidence_id = super::provider_evidence::import(project, &file);
    let _ = std::fs::remove_file(&file);
    Ok((m, id, evidence_id?))
}

/// The stored measurement whose record's SHA-256 is `digest` (its bytes the ones named, its
/// origin the native measurement).
pub fn measurement(project: &Path, digest: &str) -> Result<ProviderMeasurement, String> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("PROVIDER_MEASUREMENT_UNKNOWN".into());
    }
    let path = project
        .join(STORE)
        .join(RESULTS)
        .join(format!("{}.p0j", &digest[..16]));
    let bytes = std::fs::read(&path).map_err(|_| "PROVIDER_MEASUREMENT_UNKNOWN".to_string())?;
    if sha(&bytes) != digest {
        return Err("PROVIDER_MEASUREMENT_CHANGED".into());
    }
    let (provenance, m) = journal::read_typed_value_with_provenance::<ProviderMeasurement>(&path)
        .map_err(|_| "PROVIDER_MEASUREMENT_FORMAT".to_string())?;
    if provenance.source != journal::source::PROVIDER_MEASUREMENT {
        return Err("PROVIDER_EVIDENCE_NOT_MEASURED".into());
    }
    Ok(m)
}

/// DEC-18 §3.3: a boot flow of operating mode `mode` is admitted on `evidence` only as the
/// derivation of a stored measurement ([`derive_evidence`]) of exactly the audited pair (its RPC
/// and WSS bindings, so their credential revisions), for the capture source it measured, whose
/// reads cover what the current code of the BOOT audit and of the run reads — each SUPPORTED. A
/// record naming a measurement it was not derived from, a measurement of another pair or
/// source, or one an older code made (a read added since) is refused.
pub fn admits(
    project: &Path,
    evidence: &super::provider_evidence::ProviderEvidence,
    rpc_binding: &str,
    wss_binding: &str,
    group: &Pubkey,
    transactions: bool,
    mode: &str,
) -> Result<(), String> {
    let run = Mode::of_run(mode).ok_or_else(|| {
        format!("PROVIDER_MEASUREMENT_MODE_UNMEASURED: {mode} (no approved measurement covers it)")
    })?;
    if transactions {
        return Err(format!("PROVIDER_MEASUREMENT_SOURCE: the measurement covers capture source {CAPTURE_SOURCE}; transactions is unmeasured"));
    }
    let m = measurement(project, &evidence.source_sha256)?;
    if derive_evidence(&m, &evidence.source_sha256) != *evidence {
        return Err(
            "PROVIDER_EVIDENCE_NOT_DERIVED: the evidence is not its measurement's derivation"
                .into(),
        );
    }
    if m.rpc.binding != rpc_binding || m.wss.binding != wss_binding {
        return Err("PROVIDER_MEASUREMENT_PAIR_MISMATCH: measured for another provider pair or credential revision".into());
    }
    if m.capture_source != CAPTURE_SOURCE {
        return Err(format!(
            "PROVIDER_MEASUREMENT_SOURCE: measured for {}",
            m.capture_source
        ));
    }
    let program = Pubkey::from_str(darkbot_project0::protocol::PROGRAM_ID_MAINNET)
        .map_err(|_| "MEASURE_PROGRAM")?;
    let (current, order, messages) = reads_of(program, group);
    let spec = probe_spec(&order, &messages, &program, group);
    let (measured_plan, _) = load_plan(project, &m.plan_id)?;
    for mode in [Mode::Boot, run] {
        let reads = |of: &[ModeReads]| {
            of.iter()
                .find(|r| r.mode == mode)
                .map(|r| r.reads.clone())
                .unwrap_or_default()
        };
        let measured = reads(&m.mode_reads);
        if let Some(read) = reads(&current).iter().find(|r| !measured.contains(r)) {
            return Err(format!(
                "PROVIDER_MEASUREMENT_STALE: {} now reads {} the measurement did not probe",
                mode.name(),
                read.text()
            ));
        }
        if let Some(probe) = stale_probe(&measured_plan.probes, &spec, &reads(&current)) {
            return Err(format!(
                "PROVIDER_MEASUREMENT_STALE: {} now probes {} as {} (measured otherwise)",
                mode.name(),
                probe.read.text(),
                probe.params
            ));
        }
        match m.verdict(mode) {
            Some(Outcome::Supported) => {}
            Some(o) => {
                return Err(format!(
                    "PROVIDER_MEASUREMENT_MODE_BLOCKED: {} {}",
                    mode.name(),
                    o.text()
                ))
            }
            None => {
                return Err(format!(
                    "PROVIDER_MEASUREMENT_MODE_UNMEASURED: {}",
                    mode.name()
                ))
            }
        }
    }
    Ok(())
}

/// The first current probe of `reads` the measured plan probed otherwise (or not at all).
fn stale_probe<'a>(measured: &[Probe], current: &'a [Probe], reads: &[Read]) -> Option<&'a Probe> {
    current
        .iter()
        .filter(|p| reads.contains(&p.read))
        .find(|p| !measured.contains(p))
}

/// The operator's view of a plan.
pub fn plan_text(plan: &ProbePlan, token: &str) -> String {
    let mut out = format!(
        "PROVIDER_MEASURE_PLAN id={} token={token}\nproviders rpc={} ({}) wss={} ({}) registry v{}\nbounds {}\nprogram {} group {}\n",
        plan.plan_id,
        plan.rpc.id,
        &plan.rpc.binding[..16.min(plan.rpc.binding.len())],
        plan.wss.id,
        &plan.wss.binding[..16.min(plan.wss.binding.len())],
        plan.registry_version,
        plan.limits.text(),
        plan.program,
        plan.group
    );
    for p in &plan.probes {
        out += &format!("probe {} {}\n", p.read.text(), p.params);
    }
    for m in &plan.modes {
        out += &format!(
            "mode {} reads {}\n",
            m.mode.name(),
            m.reads.iter().map(Read::text).collect::<Vec<_>>().join(" ")
        );
    }
    out += &format!("approve: p0ctl provider measure run --plan {} --approve {token}   (C-11: reads Mainnet through the selected pair)\n", plan.plan_id);
    out
}

/// The operator's view of a measurement.
pub fn measurement_text(m: &ProviderMeasurement, id: &str, evidence: &str) -> String {
    let mut out = format!(
        "PROVIDER_MEASUREMENT id={id} plan={} capture_source={} simulation={} rpc_attempts={} rpc_bytes={} wss_frames={} rate_limited={} allowed_rps={} served_calls={}\n",
        m.plan_id, m.capture_source, m.simulation_signing, m.rpc_attempts, m.rpc_bytes, m.wss_frames, m.rate_limited, m.allowed_rps, m.served_calls
    );
    for r in &m.results {
        out += &format!(
            "read {} {} attempts={} bytes={} ms={}\n",
            r.read.text(),
            r.outcome.text(),
            r.attempts,
            r.bytes,
            r.elapsed_ms
        );
    }
    for v in &m.modes {
        out += &format!("mode {} {}\n", v.mode.name(), v.outcome.text());
    }
    out += &format!("PROVIDER_EVIDENCE_ID {evidence}   (p0ctl provider qualify --id {} --evidence {evidence}; p0ctl boot network … --provider-evidence {evidence})\n", m.rpc.id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The JSON-RPC methods a source calls (outside its tests): `RpcRequest::X`, a method named
    /// as a string, the `RpcClient` helpers (each one JSON-RPC method).
    fn methods_in(text: &str) -> BTreeSet<String> {
        // Production code: every inline test module (`#[cfg(test)] mod x { … }`, brace-matched
        // outside line comments) removed; a declared test file (`mod x;`) is not read.
        let mut code = String::new();
        let mut depth = 0usize;
        let mut skipping = false;
        let mut pending = false;
        for line in text.lines() {
            let bare = line.split("//").next().unwrap_or("");
            if !skipping && line.trim() == "#[cfg(test)]" {
                pending = true;
                continue;
            }
            if pending {
                pending = false;
                let t = line.trim_start();
                if t.starts_with("mod ") && t.trim_end().ends_with('{') {
                    skipping = true;
                    depth = 0;
                } else {
                    code.push_str("#[cfg(test)]\n");
                }
            }
            if skipping {
                depth += bare.matches('{').count();
                depth = depth.saturating_sub(bare.matches('}').count());
                if depth == 0 {
                    skipping = false;
                }
                continue;
            }
            code.push_str(line);
            code.push('\n');
        }
        let code = code.as_str();
        let mut out = BTreeSet::new();
        for (i, _) in code.match_indices("RpcRequest::") {
            let name: String = code[i + 12..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if let Some(first) = name.chars().next() {
                out.insert(format!("{}{}", first.to_ascii_lowercase(), &name[1..]));
            }
        }
        for (i, _) in code.match_indices('"') {
            let word: String = code[i + 1..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let quoted = code[i + 1 + word.len()..].starts_with('"');
            let rpc = (word.starts_with("get")
                && word.len() > 3
                && word.as_bytes()[3].is_ascii_uppercase())
                || [
                    "simulateTransaction",
                    "sendTransaction",
                    "minimumLedgerSlot",
                ]
                .contains(&word.as_str());
            if quoted && rpc {
                out.insert(word);
            }
        }
        for (helper, method) in [
            (".get_genesis_hash(", "getGenesisHash"),
            (".get_account_with_commitment(", "getAccountInfo"),
            (".get_account(", "getAccountInfo"),
            (
                ".get_multiple_accounts_with_commitment(",
                "getMultipleAccounts",
            ),
            (".get_latest_blockhash(", "getLatestBlockhash"),
            (".get_fee_for_message(", "getFeeForMessage"),
            (".simulate_transaction_with_config(", "simulateTransaction"),
            (".get_transaction_with_config(", "getTransaction"),
            (".get_slot(", "getSlot"),
            (".get_balance(", "getBalance"),
            (".send_transaction", "sendTransaction"),
        ] {
            if code.contains(helper) {
                out.insert(method.into());
            }
        }
        out
    }

    fn scan(files: &[&str]) -> BTreeSet<String> {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut out = BTreeSet::new();
        for f in files {
            let path = crates.join(f);
            let paths: Vec<PathBuf> = if path.is_dir() {
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .filter(|p| {
                        p.extension().is_some_and(|e| e == "rs")
                            && !p.to_string_lossy().ends_with("tests.rs")
                    })
                    .collect()
            } else {
                vec![path]
            };
            for p in paths {
                out.extend(methods_in(
                    &std::fs::read_to_string(&p).unwrap_or_else(|_| panic!("{}", p.display())),
                ));
            }
        }
        out
    }

    /// DEC-18 §3.2 (P0-022): the probe spec is the code's — every JSON-RPC method the modules of a
    /// mode's production path call is probed, or excluded with its reason; nothing else is probed.
    /// A new call site fails here until the spec (and its approval) covers it.
    #[test]
    fn p0_022_the_probe_spec_is_the_code_s() {
        // Never probed: the read-only runs never send (`ObserveOnly` refuses, DEC-18 §3.2).
        let excluded: BTreeSet<String> = ["sendTransaction".to_string()].into();
        let boot = [
            "runtime/src/orchestration/boot_network.rs",
            "runtime/src/orchestration/fe_qualification.rs",
        ];
        let econ4h = [
            "runtime/src/orchestration/startup.rs",
            "runtime/src/orchestration/state_owner.rs",
            "runtime/src/orchestration/state_owner",
            "runtime/src/orchestration/remote_workers.rs",
            "runtime/src/execution/pipeline.rs",
            "runtime/src/execution/observe_only.rs",
            "runtime/src/execution/bundle_path.rs",
            "runtime/src/execution/route.rs",
            "runtime/src/market/capture.rs",
            "runtime/src/market/configuration.rs",
            "runtime/src/market/deployment.rs",
            "runtime/src/market/winner.rs",
            "runtime/src/market/history.rs",
            "solana/src/state/snapshot.rs",
            "solana/src/state/blockhash.rs",
            "solana/src/submit/rpc.rs",
        ];
        let c0_mo = [
            "runtime/src/market/capture.rs",
            "runtime/src/market/configuration.rs",
            "runtime/src/market/deployment.rs",
            "reporter/src/bin/p0_market_capture_reported.rs",
        ];
        for (mode, files) in [
            (Mode::Boot, &boot[..]),
            (Mode::Econ4h, &econ4h[..]),
            (Mode::C0Mo, &c0_mo[..]),
        ] {
            let called: BTreeSet<String> = scan(files).difference(&excluded).cloned().collect();
            let spec: BTreeSet<String> = rpc_methods(mode).iter().map(|m| m.to_string()).collect();
            assert_eq!(called, spec, "{}", mode.name());
        }
    }

    /// Review 02.11.2026: a probe the measured plan made otherwise (another message, another
    /// count) or not at all is stale for exactly the modes that read it.
    #[test]
    fn p0_022_a_changed_probe_is_stale_for_its_modes() {
        let program = Pubkey::new_unique();
        let group = Pubkey::new_unique();
        let (modes, order, messages) = reads_of(program, &group);
        let spec = probe_spec(&order, &messages, &program, &group);
        let of = |mode| modes.iter().find(|m| m.mode == mode).unwrap().reads.clone();
        assert!(stale_probe(&spec, &spec, &of(Mode::Econ4h)).is_none());
        // The former probe: one program filter only.
        let mut former = messages.clone();
        let first = former
            .iter()
            .position(|m| m["method"] == "programSubscribe")
            .unwrap();
        former.remove(first);
        let measured = probe_spec(&order, &former, &program, &group);
        let stale = stale_probe(&measured, &spec, &of(Mode::Econ4h)).unwrap();
        assert_eq!(stale.read.text(), "programSubscribe@confirmed");
        assert!(stale_probe(&measured, &spec, &of(Mode::Boot)).is_none());
        assert!(stale_probe(&measured, &spec, &of(Mode::C0Mo)).is_none());
        // Another filter, same count: another digest.
        let mut other = messages.clone();
        other[first]["params"][1]["filters"] = json!([]);
        let measured = probe_spec(&order, &other, &program, &group);
        assert!(stale_probe(&measured, &spec, &of(Mode::Econ4h)).is_some());
    }

    /// DEC-18 §3.2/§3.5: the subscriptions are the modes' own messages — five types in all (the
    /// user's decision of 02.11.2026), each mode's set pinned.
    #[test]
    fn p0_022_the_subscriptions_are_the_sessions_and_five_types_in_all() {
        let program = Pubkey::new_unique();
        let group = Pubkey::new_unique();
        let reads = |mode| {
            wss_messages(mode, program, &group)
                .iter()
                .map(|m| wss_read(m).text())
                .collect::<BTreeSet<_>>()
        };
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(
            reads(Mode::Boot),
            set(&["logsSubscribe@confirmed", "slotSubscribe@-"])
        );
        assert_eq!(
            reads(Mode::Econ4h),
            set(&[
                "accountSubscribe@confirmed",
                "logsSubscribe@finalized",
                "programSubscribe@confirmed",
                "rootSubscribe@-",
                "slotSubscribe@-",
            ])
        );
        assert_eq!(
            reads(Mode::C0Mo),
            set(&["logsSubscribe@finalized", "slotSubscribe@-"])
        );
        let (_, order, messages) = reads_of(program, &group);
        let rpc = order.iter().filter(|r| matches!(r, Read::Rpc(_))).count();
        assert_eq!(rpc, 12, "twelve RPC methods");
        assert_eq!(order.len() - rpc, 6, "six subscription reads");
        assert_eq!(subscription_types(&order), 5);
        assert_eq!(
            messages.len(),
            7,
            "seven subscription messages: logs at two commitments, the owner's two program filters"
        );
    }
}
