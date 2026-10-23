
// N2 acceptance (included in `fix100_tests`): one candidate deadline over all external work.
// The production pieces — `RpcSubmitter` behind `ObserveOnly` (as in the owner), the Jupiter
// route provider, `P0ReceivershipBuilder::plans` and `Pipeline::evaluate` — run against loopback
// endpoints that hang, refuse, retry or trickle; each evaluation returns within the deadline plus
// 100 ms with `RpcDeadline`, and no request starts after the deadline.

/// N2 test endpoint: a loopback HTTP server whose answer per JSON-RPC method (or Jupiter path)
/// is scripted; every request's arrival instant is recorded, so a test can prove that no request
/// started after a deadline.
struct Endpoint {
    url: String,
    /// (arrival, method or path, JSON request body or `null`)
    arrivals: std::sync::Arc<
        std::sync::Mutex<Vec<(std::time::Instant, String, serde_json::Value)>>,
    >,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone)]
enum Answer {
    /// `200` with this JSON-RPC `result`.
    Result(serde_json::Value),
    /// `200` with this raw JSON body (a REST answer such as Jupiter `/build`).
    Raw(serde_json::Value),
    /// `200` with this JSON-RPC `error` object.
    Error(serde_json::Value),
    /// A status code with an empty body.
    Status(u16),
    /// Accept, read the request, never answer (until the test ends).
    Hang,
    /// Headers of a 200 response, then one body byte per `every` (a slow response read).
    Trickle(std::time::Duration),
}

impl Endpoint {
    /// `answer(method_or_path, n)` decides the answer of the n-th request (0-based) of that kind.
    fn start(answer: impl Fn(&str, usize) -> Answer + Send + Sync + 'static) -> Self {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let arrivals = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let answer = std::sync::Arc::new(answer);
        let (log, halt) = (arrivals.clone(), stop.clone());
        std::thread::spawn(move || {
            let counts = std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::<String, usize>::new(),
            ));
            while !halt.load(std::sync::atomic::Ordering::Acquire) {
                let Ok((stream, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                };
                let (log, halt, answer, counts) =
                    (log.clone(), halt.clone(), answer.clone(), counts.clone());
                std::thread::spawn(move || {
                    stream.set_nonblocking(false).unwrap();
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut stream = stream;
                    loop {
                        let mut request_line = String::new();
                        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                            return;
                        }
                        let arrived = std::time::Instant::now();
                        let mut length = 0usize;
                        loop {
                            let mut h = String::new();
                            if reader.read_line(&mut h).unwrap_or(0) == 0 {
                                return;
                            }
                            if h == "\r\n" {
                                break;
                            }
                            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                                length = v.trim().parse().unwrap_or(0);
                            }
                        }
                        let mut body = vec![0u8; length];
                        if reader.read_exact(&mut body).is_err() {
                            return;
                        }
                        let path = request_line.split(' ').nth(1).unwrap_or("").to_string();
                        // A request without a JSON body (a REST GET) records its path and query.
                        let json = serde_json::from_slice::<serde_json::Value>(&body)
                            .unwrap_or_else(|_| serde_json::Value::String(path.clone()));
                        let kind = json["method"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| path.split('?').next().unwrap_or("").to_string());
                        let n = {
                            let mut c = counts.lock().unwrap();
                            let e = c.entry(kind.clone()).or_default();
                            *e += 1;
                            *e - 1
                        };
                        log.lock().unwrap().push((arrived, kind.clone(), json));
                        match answer(&kind, n) {
                            Answer::Result(result) => {
                                let body = serde_json::json!({"jsonrpc":"2.0","id":1,"result":result})
                                    .to_string();
                                let _ = write!(
                                    stream,
                                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                                    body.len(),
                                    body
                                );
                            }
                            Answer::Raw(raw) => {
                                let body = raw.to_string();
                                let _ = stream.write_all(
                                    format!(
                                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                                        body.len(),
                                        body
                                    )
                                    .as_bytes(),
                                );
                            }
                            Answer::Error(error) => {
                                let body = serde_json::json!({"jsonrpc":"2.0","id":1,"error":error})
                                    .to_string();
                                let _ = write!(
                                    stream,
                                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                                    body.len(),
                                    body
                                );
                            }
                            Answer::Status(code) => {
                                let _ = write!(
                                    stream,
                                    "HTTP/1.1 {code} X\r\ncontent-length: 0\r\n\r\n"
                                );
                            }
                            Answer::Hang => {
                                while !halt.load(std::sync::atomic::Ordering::Acquire) {
                                    std::thread::sleep(std::time::Duration::from_millis(5));
                                }
                                return;
                            }
                            Answer::Trickle(every) => {
                                let _ = write!(
                                    stream,
                                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 100000\r\n\r\n"
                                );
                                while !halt.load(std::sync::atomic::Ordering::Acquire) {
                                    if stream.write_all(b" ").is_err() {
                                        return;
                                    }
                                    std::thread::sleep(every);
                                }
                                return;
                            }
                        }
                    }
                });
            }
        });
        Self {
            url,
            arrivals,
            stop,
        }
    }
    /// Arrival instants of all requests of `kind` (method or path).
    fn arrivals_of(&self, kind: &str) -> Vec<std::time::Instant> {
        self.arrivals
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, k, _)| k == kind)
            .map(|(t, _, _)| *t)
            .collect()
    }
    /// JSON request bodies of all requests of `kind`.
    fn bodies_of(&self, kind: &str) -> Vec<serde_json::Value> {
        self.arrivals
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, k, _)| k == kind)
            .map(|(_, _, b)| b.clone())
            .collect()
    }
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
    }
}


/// The fixture with the evaluation's signing payer as liquidator.
fn n2_fixture() -> (PlansFixture, solana_sdk::signature::Keypair) {
    use solana_sdk::signature::Signer;
    let payer = solana_sdk::signature::Keypair::new();
    let mut f = PlansFixture::new();
    f.liquidator = payer.pubkey();
    (f, payer)
}

/// A provider budget that admits the test's calls at once (unless a test holds its permits).
fn n2_budget(concurrency: u64) -> std::sync::Arc<darkbot_solana::provider::Budget> {
    let mut c = darkbot_solana::runtime_config::RuntimeConfig::default();
    c.limits.rpc_rps = 1_000;
    c.limits.rpc_burst = 100;
    c.limits.rpc_concurrency = concurrency;
    c.limits.simulation_rps = 1_000;
    c.limits.hotpath_rps = 1_000;
    c.limits.rpc_attempts = 3;
    // Retries after a server error fit inside the deadline (the default backoff is 500 ms).
    c.limits.rpc_backoff_ms = 10;
    std::sync::Arc::new(darkbot_solana::provider::Budget::configured(&c))
}

const N2_DEADLINE: std::time::Duration = std::time::Duration::from_millis(300);
const N2_SLACK: std::time::Duration = std::time::Duration::from_millis(100);

fn fee_ok() -> Answer {
    Answer::Result(serde_json::json!({"context": {"slot": 1}, "value": 5000}))
}
fn simulation_ok() -> Answer {
    Answer::Result(serde_json::json!({"context": {"slot": 1}, "value": {
        "err": null, "logs": [], "accounts": null, "unitsConsumed": 100,
        "returnData": null, "innerInstructions": null, "replacementBlockhash": null}}))
}

/// One Shadow evaluation of the fixture account through the production pipeline.
fn n2_evaluate<R: RouteProvider, S: darkbot_solana::submit::SubmissionAdapter>(
    f: &PlansFixture,
    payer: &solana_sdk::signature::Keypair,
    routes: &R,
    zero_prefund: bool,
    submitter: &S,
) -> (crate::analytics::record::OpportunityRecord, std::time::Duration) {
    n2_evaluate_record(f, payer, routes, zero_prefund, submitter, Pubkey::default())
}

/// `n2_evaluate` of an account whose liquidation record is `record` (default: none, so the plan
/// creates one and pays its rent).
fn n2_evaluate_record<R: RouteProvider, S: darkbot_solana::submit::SubmissionAdapter>(
    f: &PlansFixture,
    payer: &solana_sdk::signature::Keypair,
    routes: &R,
    zero_prefund: bool,
    submitter: &S,
    record: Pubkey,
) -> (crate::analytics::record::OpportunityRecord, std::time::Duration) {
    use crate::execution::pipeline::{Mode, NetworkBudget, Pipeline};
    use darkbot_project0::{
        economics::FailedCostMatrix,
        execution::Freshness,
        planner::Allowlist,
        safety::{Allow, Limits, Safety},
    };
    use darkbot_solana::state::blockhash::{BlockhashCache, CachedBlockhash};
    let src = f.state(record, true);
    let clock = darkbot_common::timing::Clock::new();
    let bh = BlockhashCache::new();
    bh.set(CachedBlockhash {
        hash: Hash::new_unique(),
        last_valid_block_height: 100,
        refreshed_ns: clock.now_ns(),
    });
    let zero = Limits {
        max_total_capital: 0,
        max_outstanding_inventory: 0,
        max_concurrent_positions: 0,
        max_exposure_per_attempt: 0,
        max_daily_loss: 0,
        max_failed_attempts_per_day: 0,
        max_tip: 0,
        max_slippage_bps: 0,
        max_outstanding_attempts: 0,
    };
    let mut safety = Safety::new(zero, Allow::default(), false);
    // As in the owner's setup: the pinned swap program is the one allowed swap program.
    let mut allow = Allowlist::default();
    allow.swap_programs.insert(
        darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID
            .parse()
            .unwrap(),
    );
    let failed = FailedCostMatrix::default();
    let slots = darkbot_solana::ingress::SlotWatch::new();
    let mut network = NetworkBudget::new(4, N2_DEADLINE.as_nanos() as u64);
    let inventory = |_: &Pubkey| 10_000u64;
    let ata = |mint: &Pubkey| {
        Some(get_associated_token_address_with_program_id(
            &f.liquidator,
            mint,
            &f.token_program,
        ))
    };
    let lamports = |l: u64| Some(l);
    let builder = P0ReceivershipBuilder {
        state: &src,
        program: f.program,
        group: f.group,
        liquidator: f.liquidator,
        global_fee_wallet: Ok(Pubkey::new_unique()),
        sizing: &f.sizing,
        routes,
        inventory: &inventory,
        ata: &ata,
        lamports_to_usd_micro: &lamports,
        rent_lamports_to_usd_micro: &lamports,
        jito_tip_usd_micro: 0,
        jito_tip_lamports: 0,
        zero_prefund_qualified: zero_prefund,
        instructions: &f.table,
        v1_plans: false,
        prepared: None,
        path: Path::Rpc,
        resources: Resources {
            compute_unit_limit: 400_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        limits: TxLimits::default(),
    };
    let health = darkbot_project0::health::crate_impl::CrateHealth {
        state: &src,
        group: f.group,
    };
    // The production top-level allowlist: the protocol and the pinned swap program.
    let allowed = [
        f.program,
        darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID
            .parse()
            .unwrap(),
    ];
    let mut p = Pipeline {
        mode: Mode::Shadow,
        clock: &clock,
        health: &health,
        builder: &builder,
        submitter,
        safety: &mut safety,
        blockhash: &bh,
        payer,
        allowlist: &allow,
        failed_costs: &failed,
        limits: TxLimits::default(),
        resources: Resources {
            compute_unit_limit: 400_000,
            priority_micro_lamports_per_cu: 0,
            loaded_accounts_data_size: None,
        },
        min_profit: 0,
        blockhash_max_age_ns: u64::MAX,
        p7_passed: false,
        freshness: Freshness {
            max_opportunity_age_ns: u64::MAX,
            execution_deadline_ns: u64::MAX,
        },
        tip_account: None,
        global_allowed: &allowed,
        prereq_allowed: &[],
        slot_watch: &slots,
        network: &mut network,
        simulation_failure: None,
        fe04: None,
    };
    let started = std::time::Instant::now();
    let rec = p.evaluate(f.account, "n2", clock.now_ns(), 1_000, 10, true);
    (rec, started.elapsed())
}

/// The evaluation ended at its deadline with `RpcDeadline`, and no request of the endpoint
/// arrived after the deadline.
fn assert_bounded(
    case: &str,
    (rec, elapsed): &(crate::analytics::record::OpportunityRecord, std::time::Duration),
    endpoint: &Endpoint,
    started: std::time::Instant,
) {
    assert!(
        *elapsed <= N2_DEADLINE + N2_SLACK,
        "{case}: {elapsed:?} reason={:?}",
        rec.reason
    );
    assert_eq!(
        rec.loss,
        Some(crate::analytics::record::LossReason::RpcDeadline),
        "{case}: {:?}",
        rec.reason
    );
    let deadline = started + N2_DEADLINE;
    for (at, kind, _) in endpoint.arrivals.lock().unwrap().iter() {
        assert!(*at < deadline, "{case}: {kind} started after the deadline");
    }
}

fn rpc_submitter(
    url: &str,
    budget: std::sync::Arc<darkbot_solana::provider::Budget>,
) -> crate::execution::observe_only::ObserveOnly<darkbot_solana::submit::rpc::RpcSubmitter> {
    crate::execution::observe_only::ObserveOnly::new(
        darkbot_solana::submit::rpc::RpcSubmitter::with_budget(url.into(), true, budget).unwrap(),
    )
}

/// The working path: the real adapter quotes and simulates through the endpoint, so the
/// bounded cases below fail for their scripted reason only.
#[test]
fn n2_real_adapter_quotes_and_simulates_within_the_deadline() {
    let (f, payer) = n2_fixture();
    let endpoint = Endpoint::start(|kind, _| match kind {
        "getFeeForMessage" => fee_ok(),
        "simulateTransaction" => simulation_ok(),
        _ => Answer::Status(500),
    });
    let submitter = rpc_submitter(&endpoint.url, n2_budget(4));
    let (rec, elapsed) = n2_evaluate(&f, &payer, &TestRoutes(None), false, &submitter);
    assert_eq!(rec.simulation_result.as_deref(), Some("PASS"), "{:?}", rec.reason);
    assert!(rec.loss.is_none() || rec.loss != Some(crate::analytics::record::LossReason::RpcDeadline));
    assert!(elapsed < N2_DEADLINE);
    assert_eq!(endpoint.arrivals_of("getFeeForMessage").len(), 1);
    assert_eq!(endpoint.arrivals_of("simulateTransaction").len(), 1);
}

/// A hanging fee quote, a hanging simulation, retried server errors and a slowly read response
/// all end at the candidate deadline.
#[test]
fn n2_hanging_retried_and_slow_rpc_calls_end_at_the_deadline() {
    type Script = fn(&str, usize) -> Answer;
    let cases: [(&str, Script); 4] = [
        ("hanging fee quote", |kind, _| match kind {
            "getFeeForMessage" => Answer::Hang,
            _ => simulation_ok(),
        }),
        ("hanging simulation", |kind, _| match kind {
            "getFeeForMessage" => fee_ok(),
            _ => Answer::Hang,
        }),
        ("retried 503 then hanging", |_, n| {
            if n < 2 {
                Answer::Status(503)
            } else {
                Answer::Hang
            }
        }),
        ("slow response read", |_, _| {
            Answer::Trickle(std::time::Duration::from_millis(20))
        }),
    ];
    let (f, payer) = n2_fixture();
    for (case, script) in cases {
        let endpoint = Endpoint::start(script);
        let submitter = rpc_submitter(&endpoint.url, n2_budget(4));
        let started = std::time::Instant::now();
        let outcome = n2_evaluate(&f, &payer, &TestRoutes(None), false, &submitter);
        assert_bounded(case, &outcome, &endpoint, started);
        if case.starts_with("retried") {
            assert!(endpoint.arrivals_of("getFeeForMessage").len() >= 2, "{case}: retried");
        }
    }
}

/// A provider permit that never becomes free before the deadline: no request is sent at all.
#[test]
fn n2_blocked_provider_permit_ends_at_the_deadline_without_a_request() {
    let (f, payer) = n2_fixture();
    let endpoint = Endpoint::start(|_, _| fee_ok());
    let budget = n2_budget(1);
    let held = budget
        .acquire(
            darkbot_solana::provider::Workload::LiveRead,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .unwrap();
    let submitter = rpc_submitter(&endpoint.url, budget.clone());
    let started = std::time::Instant::now();
    let outcome = n2_evaluate(&f, &payer, &TestRoutes(None), false, &submitter);
    assert_bounded("blocked permit", &outcome, &endpoint, started);
    assert!(endpoint.arrivals.lock().unwrap().is_empty(), "no request without a permit");
    drop(held);
}

/// An expired deadline starts nothing: the real adapter answers `Deadline` without a request.
#[test]
fn n2_expired_deadline_starts_no_request() {
    use darkbot_solana::submit::{FeeQuoteError, SimResult, SubmissionAdapter};
    let endpoint = Endpoint::start(|_, _| fee_ok());
    let submitter = rpc_submitter(&endpoint.url, n2_budget(4));
    let tx = solana_sdk::transaction::VersionedTransaction::default();
    let past = std::time::Instant::now() - std::time::Duration::from_millis(1);
    assert_eq!(submitter.fee_for_message(&tx, past).err(), Some(FeeQuoteError::Deadline));
    assert_eq!(submitter.simulate(&tx, past), SimResult::Deadline);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(endpoint.arrivals.lock().unwrap().is_empty());
}

/// A hanging route provider during planning (ZERO_PREFUND quote): planning returns at the
/// deadline, the evaluation is `RpcDeadline`, and neither a re-quote nor a fee quote or
/// simulation starts after it.
#[test]
fn n2_hanging_jupiter_ends_planning_at_the_deadline() {
    let (f, payer) = n2_fixture();
    let jupiter = Endpoint::start(|_, _| Answer::Hang);
    let rpc = Endpoint::start(|kind, _| match kind {
        "getFeeForMessage" => fee_ok(),
        _ => simulation_ok(),
    });
    let routes = crate::execution::route::JupiterV2RouteProvider::new(
        f.liquidator,
        None,
        50,
        64,
        true,
        false,
    )
    .unwrap()
    .with_base_url(jupiter.url.clone());
    let submitter = rpc_submitter(&rpc.url, n2_budget(4));
    let started = std::time::Instant::now();
    let outcome = n2_evaluate(&f, &payer, &routes, true, &submitter);
    assert_bounded("hanging jupiter", &outcome, &jupiter, started);
    assert!(!jupiter.arrivals.lock().unwrap().is_empty(), "the quote was requested");
    for (at, kind, _) in rpc.arrivals.lock().unwrap().iter() {
        assert!(*at < started + N2_DEADLINE, "{kind} after the deadline");
    }
}

/// A provider budget with the default 500 ms backoff after a failure.
fn n2_budget_default_backoff() -> std::sync::Arc<darkbot_solana::provider::Budget> {
    let mut c = darkbot_solana::runtime_config::RuntimeConfig::default();
    c.limits.rpc_rps = 1_000;
    c.limits.rpc_burst = 100;
    c.limits.rpc_concurrency = 4;
    c.limits.simulation_rps = 1_000;
    c.limits.hotpath_rps = 1_000;
    c.limits.rpc_attempts = 3;
    std::sync::Arc::new(darkbot_solana::provider::Budget::configured(&c))
}

/// A genuine server error whose provider backoff runs past the candidate deadline: no retry is
/// possible before the deadline, which is the cause (`RpcDeadline`), not a budget verdict.
#[test]
fn n2_provider_backoff_past_the_deadline_is_the_deadline() {
    let (f, payer) = n2_fixture();
    let endpoint = Endpoint::start(|_, _| Answer::Status(503));
    let submitter = rpc_submitter(&endpoint.url, n2_budget_default_backoff());
    let started = std::time::Instant::now();
    let outcome = n2_evaluate(&f, &payer, &TestRoutes(None), false, &submitter);
    assert_bounded("backoff past the deadline", &outcome, &endpoint, started);
    assert_eq!(endpoint.arrivals_of("getFeeForMessage").len(), 1, "no retry");
}

/// A request cut by the candidate deadline is the caller's bound, not a provider failure: the
/// provider is not degraded, so other workloads get their next permit without a backoff.
#[test]
fn n2_a_deadline_cut_does_not_degrade_the_provider() {
    let (f, payer) = n2_fixture();
    let endpoint = Endpoint::start(|_, _| Answer::Hang);
    let budget = n2_budget_default_backoff();
    let submitter = rpc_submitter(&endpoint.url, budget.clone());
    let started = std::time::Instant::now();
    let outcome = n2_evaluate(&f, &payer, &TestRoutes(None), false, &submitter);
    assert_bounded("deadline cut", &outcome, &endpoint, started);
    let next = budget.acquire(
        darkbot_solana::provider::Workload::LiveRead,
        std::time::Instant::now() + std::time::Duration::from_millis(100),
    );
    assert!(next.is_ok(), "no backoff after a deadline cut: {:?}", next.err());
}

/// The Jito adapter honours the same contract: its bundle simulation ends at the deadline, an
/// expired deadline sends nothing, and it states that it has no fee quote and no single
/// simulation (no network call).
#[test]
fn n2_jito_bundle_simulation_ends_at_the_deadline() {
    use darkbot_solana::submit::{FeeQuoteError, SimResult, SubmissionAdapter};
    let endpoint = Endpoint::start(|_, _| Answer::Hang);
    let jito = darkbot_solana::submit::jito::JitoAdapter::new(
        endpoint.url.clone(),
        Some(endpoint.url.clone()),
        std::time::Duration::from_secs(30),
    )
    .unwrap();
    let tx = solana_sdk::transaction::VersionedTransaction::default();
    let started = std::time::Instant::now();
    let result = jito.simulate_bundle(std::slice::from_ref(&tx), started + N2_DEADLINE);
    assert_eq!(result, SimResult::Deadline);
    assert!(started.elapsed() <= N2_DEADLINE + N2_SLACK, "{:?}", started.elapsed());
    let requests = endpoint.arrivals_of("simulateBundle").len();
    assert_eq!(requests, 1);
    let past = std::time::Instant::now() - std::time::Duration::from_millis(1);
    assert_eq!(jito.simulate_bundle(std::slice::from_ref(&tx), past), SimResult::Deadline);
    assert_eq!(
        jito.fee_for_message(&tx, past).err(),
        Some(FeeQuoteError::Unavailable("JITO_PATH_NO_FEE_QUOTE"))
    );
    assert!(matches!(jito.simulate(&tx, past), SimResult::Unavailable(_)));
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(endpoint.arrivals_of("simulateBundle").len(), requests, "nothing after the deadline");
}

/// Route-fit re-quotes share the candidate deadline: with every quote taking 100 ms and never
/// fitting, the loop stops at the deadline and starts no quote after it.
#[test]
fn n2_route_fit_requotes_stop_at_the_deadline() {
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_millis(250);
    let mut quote_starts = Vec::new();
    let result = quote_fitting_route(
        64,
        deadline,
        |cap| {
            quote_starts.push(std::time::Instant::now());
            std::thread::sleep(std::time::Duration::from_millis(100));
            Ok(Some(route_for_budget(cap, 0, cap as usize)))
        },
        // Always one byte too large: the cap shrinks by 2 per attempt, six attempts available.
        |_| {
            Err::<(), _>(BuildError::TooLarge {
                bytes: 1_233,
                max: 1_232,
            })
        },
    );
    assert!(result.unwrap_err().contains("ROUTE_FIT_DEADLINE"));
    assert!(quote_starts.len() < 6, "bounded by the deadline, not the attempt count");
    assert!(quote_starts.iter().all(|t| *t < deadline));
    assert!(started.elapsed() <= std::time::Duration::from_millis(250) + N2_SLACK);
}

/// An adapter without a candidate transport (projection workers) answers fee quote and
/// simulation `Unavailable` without any network call, and is built without a runtime of its own.
#[test]
fn n2_adapter_without_candidate_transport_never_calls_the_network() {
    use darkbot_solana::submit::{FeeQuoteError, SimResult, SubmissionAdapter};
    let endpoint = Endpoint::start(|_, _| fee_ok());
    let plain = darkbot_solana::submit::rpc::RpcSubmitter::new(endpoint.url.clone(), true);
    let tx = solana_sdk::transaction::VersionedTransaction::default();
    let later = std::time::Instant::now() + N2_DEADLINE;
    assert_eq!(
        plain.fee_for_message(&tx, later).err(),
        Some(FeeQuoteError::Unavailable("NO_CANDIDATE_TRANSPORT"))
    );
    assert_eq!(
        plain.simulate(&tx, later),
        SimResult::Unavailable("NO_CANDIDATE_TRANSPORT".into())
    );
    assert_eq!(plain.candidate_requests(), 0);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(endpoint.arrivals.lock().unwrap().is_empty());
}

/// E9: a projection simulation asks the node for at least the newest canonical slot it was
/// requested from (`minContextSlot`) and refuses an older context (a lagging node would project
/// from older inputs than the canonical state); at the required slot it is accepted and its
/// projected slot is the simulation Clock's.
#[test]
fn e9_projection_simulation_is_not_older_than_its_inputs() {
    use base64::Engine;
    use solana_sdk::signature::Signer;
    let account = |data: Vec<u8>| {
        serde_json::json!({"lamports": 1, "owner": Pubkey::default().to_string(),
            "data": [base64::engine::general_purpose::STANDARD.encode(&data), "base64"],
            "executable": false, "rentEpoch": 0, "space": data.len()})
    };
    let at = move |slot: u64| {
        let clock = solana_sdk::clock::Clock {
            slot,
            unix_timestamp: 1_700_000_000,
            ..Default::default()
        };
        Answer::Result(serde_json::json!({"context": {"slot": slot}, "value": {
            "err": null, "logs": [], "unitsConsumed": 5, "returnData": null,
            "innerInstructions": null, "replacementBlockhash": null,
            "accounts": [account(vec![7; 8]), account(bincode::serialize(&clock).unwrap())]}}))
    };
    let endpoint = Endpoint::start(move |kind, n| match (kind, n) {
        ("simulateTransaction", 0) => at(99),
        ("simulateTransaction", 1) => Answer::Error(serde_json::json!({
            "code": -32016, "message": "Minimum context slot has not been reached",
            "data": {"contextSlot": 98}})),
        _ => at(100),
    });
    let payer = solana_sdk::signature::Keypair::new();
    let mut built = tx::build(
        &TxPlan {
            version: BuildVersion::V0,
            payer: payer.pubkey(),
            instructions: vec![Instruction {
                program_id: Pubkey::new_unique(),
                accounts: vec![],
                data: vec![1],
            }],
            lookup_tables: vec![].into(),
            resources: Resources {
                compute_unit_limit: 200_000,
                priority_micro_lamports_per_cu: 0,
                loaded_accounts_data_size: None,
            },
        },
        solana_sdk::hash::Hash::new_unique(),
        &TxLimits::default(),
    )
    .unwrap();
    tx::sign(&mut built, &[&payer]).unwrap();
    let submitter = darkbot_solana::submit::rpc::RpcSubmitter::new(endpoint.url.clone(), true);
    let target = [Pubkey::new_unique()];
    let behind = submitter
        .simulate_with_post_accounts(&built.tx, &target, 100)
        .unwrap_err();
    assert!(
        matches!(
            behind,
            darkbot_solana::submit::rpc::ProjectionError::ContextBehind {
                context: Some(99),
                required: 100
            }
        ),
        "{behind}"
    );
    assert!(
        behind.to_string().starts_with("PROJECTION_CONTEXT_BEHIND"),
        "{behind}"
    );
    // A node that has not reached the slot refuses with -32016: the same typed cause.
    let refused = submitter
        .simulate_with_post_accounts(&built.tx, &target, 100)
        .unwrap_err();
    assert!(
        matches!(
            refused,
            darkbot_solana::submit::rpc::ProjectionError::ContextBehind {
                context: None,
                required: 100
            }
        ),
        "{refused}"
    );
    let (slot, unix_s, accounts, _) = submitter
        .simulate_with_post_accounts(&built.tx, &target, 100)
        .unwrap();
    assert_eq!((slot, unix_s, accounts.len()), (100, 1_700_000_000, 1));
    let bodies = endpoint.bodies_of("simulateTransaction");
    assert_eq!(bodies.len(), 3);
    for body in bodies {
        assert_eq!(body["params"][1]["minContextSlot"], 100, "{body}");
    }
}

/// FE-02 end to end on the production path: the fixture account's ZERO_PREFUND plan through the
/// real Jupiter adapter (loopback `/build`, a synthetic response whose pinned ExactIn `route`
/// instruction moves the liquidator's own accounts), the real builder and signed-message check,
/// and fee quote plus simulation through the real RPC adapter (loopback), in Shadow. With a linked
/// record (no rent) the selected class is ZERO_PREFUND, simulated PASS, its net realized at the
/// instruction floor; the request carried the plan's exact mints, amount, taker and slippage.
/// Without a record the same route pays the record rent and is Known but negative — no plan is
/// actionable on a floor that does not cover every cost.
#[test]
fn fe02_zero_prefund_end_to_end_through_the_real_adapters() {
    use base64::Engine;
    let (f, payer) = n2_fixture();
    let (asset_mint, liab_mint) = (f.banks[0].1, f.banks[1].1);
    let jup: Pubkey = darkbot_project0::protocol::JUP_SWAP_PROGRAM_ID.parse().unwrap();
    let ata = |m: &Pubkey| {
        get_associated_token_address_with_program_id(&f.liquidator, m, &f.token_program)
    };
    // `route`: discriminator, a one-step plan, in 1 000, quoted 20 000, 50 bps, no platform fee.
    let mut data = vec![229, 23, 203, 151, 122, 227, 173, 42];
    data.extend(1u32.to_le_bytes());
    data.extend([7, 100, 0, 1]);
    data.extend(1_000u64.to_le_bytes());
    data.extend(20_000u64.to_le_bytes());
    data.extend(50u16.to_le_bytes());
    data.push(0);
    let acct = |k: &Pubkey, signer: bool, writable: bool| {
        serde_json::json!({"pubkey": k.to_string(), "isSigner": signer, "isWritable": writable})
    };
    let build = serde_json::json!({
        "inAmount": "1000", "outAmount": "20000", "otherAmountThreshold": "19900",
        "swapMode": "ExactIn", "setupInstructions": [], "cleanupInstruction": null,
        "otherInstructions": [], "addressesByLookupTableAddress": null,
        "swapInstruction": {
            "programId": jup.to_string(),
            "data": base64::engine::general_purpose::STANDARD.encode(&data),
            "accounts": [
                acct(&f.token_program, false, false), acct(&f.liquidator, true, false),
                acct(&ata(&asset_mint), false, true), acct(&ata(&liab_mint), false, true),
                acct(&jup, false, false), acct(&liab_mint, false, false),
                acct(&jup, false, false), acct(&Pubkey::new_unique(), false, false),
                acct(&jup, false, false)]}});
    let jupiter = Endpoint::start(move |_, _| Answer::Raw(build.clone()));
    let rpc = Endpoint::start(|kind, _| match kind {
        "getFeeForMessage" => fee_ok(),
        "simulateTransaction" => simulation_ok(),
        _ => Answer::Status(500),
    });
    let routes =
        crate::execution::route::JupiterV2RouteProvider::new(f.liquidator, None, 50, 64, false, false)
            .unwrap()
            .with_base_url(jupiter.url.clone());
    let submitter = rpc_submitter(&rpc.url, n2_budget(4));
    let record = darkbot_project0::protocol::liquidation_record_pda(&f.program, &f.account);
    let (rec, _) = n2_evaluate_record(&f, &payer, &routes, true, &submitter, record);
    assert_eq!(rec.funding.as_deref(), Some("ZERO_PREFUND_SAME_TX"), "{:?}", rec.reason);
    let zp = rec.selected_costs.as_ref().unwrap();
    // The floor 19 900 realizes: 500 repaid at its value plus 19 400 surplus (low = equity here).
    assert_eq!(zp.gross_seized_realizable_usd_micro, Some(19_900));
    assert_eq!(rec.simulation_result.as_deref(), Some("PASS"), "{:?}", rec.reason);
    assert!(rec.economics_known, "{:?}", rec.reason);
    assert!(rec.serialized_bytes.is_some_and(|b| b <= 1232));
    let requests = jupiter.bodies_of("/build");
    assert_eq!(requests.len(), 1, "one quote");
    let query = requests[0].as_str().unwrap();
    for part in [
        format!("inputMint={asset_mint}"),
        format!("outputMint={liab_mint}"),
        "amount=1000".to_string(),
        format!("taker={}", f.liquidator),
        "slippageBps=50".to_string(),
    ] {
        assert!(query.contains(&part), "{query} lacks {part}");
    }
    // One fee quote per distinct message (the two inventory plans share one, D2); only the
    // selected candidate is simulated.
    assert_eq!(rpc.arrivals_of("getFeeForMessage").len(), 2);
    assert_eq!(rpc.arrivals_of("simulateTransaction").len(), 1);
    // Without a linked record the plan creates one: the rent is bound, the floor does not cover
    // it, the class is Known but negative and never the selected actionable candidate.
    let (rec, _) = n2_evaluate(&f, &payer, &routes, true, &submitter);
    let zp = rec
        .candidate_costs
        .iter()
        .find(|c| c.funding == "ZERO_PREFUND_SAME_TX")
        .expect("evaluated");
    assert!(zp.net_profit_usd_micro.is_some_and(|n| n < 0), "{zp:?}");
    assert_ne!(rec.funding.as_deref(), Some("ZERO_PREFUND_SAME_TX"));
}
