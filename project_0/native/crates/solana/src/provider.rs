//! C0-E1 process-wide Solana provider budget. All endpoints share a conservative
//! account-level ceiling: distinct URLs are NOT evidence of independent quotas.
//! Bounded priority admission, per-workload spacing and adaptive shared ceilings.
//! This is a client ceiling, NOT qualification of purchased provider capacity.
use serde_json::Value;
use solana_client::{
    rpc_client::{RpcClient, RpcClientConfig},
    rpc_request::{RpcError, RpcRequest, RpcResponseErrorData},
    rpc_sender::{RpcSender, RpcTransportStats},
};
use solana_commitment_config::CommitmentConfig;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
    },
    time::{Duration, Instant, SystemTime},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
pub fn enable_measurement_budget() {
    ENABLED.store(true, Ordering::Release);
}
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}
#[derive(Clone, Copy, Debug)]
pub enum Workload {
    Bootstrap,
    LiveRead,
    Simulation,
    Winner,
    Deployment,
    History,
    Websocket,
    /// C5: financial reconciliation — the highest admission priority, paced in the analytics
    /// slot it shares with `Winner` (their combined rate stays `analytics_rps`) and always inside
    /// the same global RPS/burst/call/byte budget: no additional request budget.
    Reconcile,
}
impl Workload {
    /// Pacing slot. `Reconcile` shares `Winner`'s, so it gains precedence, never extra rate.
    fn index(self) -> usize {
        match self {
            Workload::Reconcile => Workload::Winner as usize,
            w => w as usize,
        }
    }
}
struct State {
    failed: bool,
    /// N4: the active configuration differs from the one this budget was built for (sticky).
    drift: bool,
    queue: VecDeque<(u64, Workload)>,
    next_ticket: u64,
    active: usize,
    sockets: u64,
    subscriptions: u64,
    tokens: f64,
    refill_at: Instant,
    effective_rps: f64,
    bytes: u64,
    successes: u32,
    next_workload: [Instant; 7],
    cooldown: Instant,
    rate_limits: u32,
    requests: u64,
    retry_after_overflow: bool,
}
pub struct Budget {
    limits: crate::runtime_config::Limits,
    adaptive: bool,
    fallback: bool,
    rpc_endpoints: Vec<String>,
    state: Mutex<State>,
    changed: Condvar,
}
pub struct Permit {
    budget: Arc<Budget>,
}
pub struct SocketPermit {
    budget: Arc<Budget>,
    subscriptions: u64,
}
impl Drop for SocketPermit {
    fn drop(&mut self) {
        let mut s = self.budget.lock_state();
        match (
            s.sockets.checked_sub(1),
            s.subscriptions.checked_sub(self.subscriptions),
        ) {
            (Some(a), Some(b)) => {
                s.sockets = a;
                s.subscriptions = b;
            }
            _ => s.failed = true,
        }
        self.budget.changed.notify_all();
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut s = self.budget.lock_state();
        if let Some(n) = s.active.checked_sub(1) {
            s.active = n;
        } else {
            s.failed = true;
        }
        self.budget.changed.notify_all();
    }
}
impl Default for Budget {
    fn default() -> Self {
        Self::configured(crate::runtime_config::active())
    }
}
impl Budget {
    fn lock_state(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(s) => s,
            Err(p) => {
                let mut s = p.into_inner();
                s.failed = true;
                self.changed.notify_all();
                s
            }
        }
    }
    /// Configuration changes cannot silently keep a default budget or reset consumed quota. N4: a
    /// drift is its own sticky halt (`PROVIDER_BUDGET_CONFIG_DRIFT`), reported once with the first
    /// differing field (endpoints only as counts), and never reinitializes the budget.
    fn check_configuration(&self, c: &crate::runtime_config::RuntimeConfig) {
        let differing = crate::runtime_config::PARAMETERS
            .iter()
            .find(|p| self.limits.get(p.field) != c.limits.get(p.field))
            .map(|p| p.field)
            .or((self.rpc_endpoints != c.rpc_endpoints).then_some("rpc_endpoints"))
            .or((self.adaptive != c.adaptive).then_some("adaptive"))
            .or((self.fallback != c.fallback).then_some("fallback"));
        let Some(field) = differing else {
            return;
        };
        let mut s = self.lock_state();
        if !s.drift {
            s.drift = true;
            eprintln!(
                "PROVIDER_BUDGET_CONFIG_DRIFT field={field} rpc_endpoints={}->{}",
                self.rpc_endpoints.len(),
                c.rpc_endpoints.len()
            );
        }
        drop(s);
        self.changed.notify_all();
    }
    pub fn configured(config: &crate::runtime_config::RuntimeConfig) -> Self {
        let now = Instant::now();
        Self {
            limits: config.limits.clone(),
            adaptive: config.adaptive,
            fallback: config.fallback,
            rpc_endpoints: config.rpc_endpoints.clone(),
            state: Mutex::new(State {
                failed: false,
                drift: false,
                queue: VecDeque::new(),
                next_ticket: 0,
                active: 0,
                sockets: 0,
                subscriptions: 0,
                tokens: config.limits.rpc_burst as f64,
                refill_at: now,
                effective_rps: config.limits.rpc_rps as f64,
                bytes: 0,
                successes: 0,
                next_workload: [now; 7],
                cooldown: now,
                rate_limits: 0,
                requests: 0,
                retry_after_overflow: false,
            }),
            changed: Condvar::new(),
        }
    }
}
/// The sticky halt of a budget: configuration drift (N4) or a poisoned/unwound state (H3).
fn halted(s: &State) -> Option<&'static str> {
    if s.drift {
        Some("PROVIDER_BUDGET_CONFIG_DRIFT")
    } else if s.failed {
        Some("PROVIDER_BUDGET_POISONED")
    } else {
        None
    }
}
/// N2: a refused admission: the code `acquire` reports and whether only the deadline made it
/// impossible (the permit would be granted later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) code: &'static str,
    pub(crate) deadline_bound: bool,
}

pub fn shared() -> Arc<Budget> {
    static B: OnceLock<Arc<Budget>> = OnceLock::new();
    let b = B.get_or_init(|| Arc::new(Budget::default())).clone();
    b.check_configuration(crate::runtime_config::active());
    b
}
impl Budget {
    pub fn acquire_socket(
        self: &Arc<Self>,
        subscriptions: u64,
    ) -> Result<SocketPermit, &'static str> {
        let mut s = self.lock_state();
        if let Some(code) = halted(&s) {
            return Err(code);
        }
        if s.sockets >= self.limits.wss_connections {
            return Err("WSS_CONNECTION_LIMIT");
        }
        if subscriptions
            > self
                .limits
                .wss_subscriptions
                .saturating_sub(s.subscriptions)
        {
            return Err("WSS_SUBSCRIPTION_LIMIT");
        }
        s.sockets += 1;
        s.subscriptions += subscriptions;
        Ok(SocketPermit {
            budget: self.clone(),
            subscriptions,
        })
    }
    pub fn acquire(
        self: &Arc<Self>,
        work: Workload,
        deadline: Instant,
    ) -> Result<Permit, &'static str> {
        self.acquire_cancellable(work, deadline, None)
    }
    pub fn acquire_cancellable(
        self: &Arc<Self>,
        work: Workload,
        deadline: Instant,
        shutdown: Option<&darkbot_common::lifecycle::Shutdown>,
    ) -> Result<Permit, &'static str> {
        self.admit(work, deadline, shutdown).map_err(|r| r.code)
    }
    /// N2: the admission with a typed refusal — `deadline_bound` when the permit cannot be
    /// granted before `deadline` (pacing, cooldown, concurrency), as opposed to a refusal by the
    /// budget itself. The codes are exactly those `acquire_cancellable` reports.
    pub(crate) fn admit(
        self: &Arc<Self>,
        work: Workload,
        deadline: Instant,
        shutdown: Option<&darkbot_common::lifecycle::Shutdown>,
    ) -> Result<Permit, Refusal> {
        let refused = |code: &'static str| Refusal {
            code,
            deadline_bound: false,
        };
        let mut s = self.lock_state();
        if let Some(code) = halted(&s) {
            return Err(refused(code));
        }
        if s.retry_after_overflow {
            return Err(refused("RPC_RATE_LIMIT"));
        }
        if s.queue.len() as u64 >= self.limits.rpc_queue {
            return Err(refused("RPC_ADMISSION_QUEUE_FULL"));
        }
        let ticket = s.next_ticket;
        s.next_ticket += 1;
        s.queue.push_back((ticket, work));
        loop {
            if let Some(code) = halted(&s) {
                s.queue.retain(|(t, _)| *t != ticket);
                self.changed.notify_all();
                return Err(refused(code));
            }
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(s.refill_at).as_secs_f64();
            s.tokens = (s.tokens + elapsed * s.effective_rps).min(self.limits.rpc_burst as f64);
            s.refill_at = now;
            let token_ready = if s.tokens >= 1.0 {
                now
            } else {
                now + Duration::from_secs_f64((1.0 - s.tokens) / s.effective_rps)
            };
            let ready = token_ready
                .max(s.next_workload[work.index()])
                .max(s.cooldown);
            let cancelled = shutdown.is_some_and(|s| s.is_requested());
            if s.retry_after_overflow || cancelled || now >= deadline || ready >= deadline {
                s.queue.retain(|(t, _)| *t != ticket);
                self.changed.notify_all();
                return Err(Refusal {
                    code: if cancelled {
                        "RPC_ADMISSION_CANCELLED"
                    } else if s.retry_after_overflow || s.cooldown > now {
                        "RPC_RATE_LIMIT"
                    } else {
                        "RPC_ADMISSION_BUDGET"
                    },
                    deadline_bound: !cancelled && !s.retry_after_overflow,
                });
            }
            if s.bytes >= self.limits.max_total_mib * 1024 * 1024 {
                s.queue.retain(|(t, _)| *t != ticket);
                self.changed.notify_all();
                return Err(refused("RPC_BYTE_BUDGET"));
            }
            if s.requests >= self.limits.max_requests {
                s.queue.retain(|(t, _)| *t != ticket);
                self.changed.notify_all();
                return Err(refused("RPC_CALL_BUDGET"));
            }
            // Only a waiter whose own workload pacing is ready can be selected, so a paced
            // workload never holds the queue (C5). Every eighth admission goes to the oldest
            // such waiter; otherwise reconciliation, then health/state/simulation, then the rest.
            let paced = |w: &Workload| s.next_workload[w.index()] <= now;
            let selected = if s.requests % 8 == 7 {
                s.queue.iter().find(|(_, w)| paced(w)).map(|(t, _)| *t)
            } else {
                s.queue
                    .iter()
                    .filter(|(_, w)| paced(w))
                    .min_by_key(|(t, w)| {
                        (
                            match w {
                                Workload::Reconcile => 0,
                                Workload::LiveRead | Workload::Simulation => 1,
                                Workload::Bootstrap | Workload::Websocket => 2,
                                _ => 3,
                            },
                            *t,
                        )
                    })
                    .map(|(t, _)| *t)
            };
            if selected == Some(ticket)
                && (s.active as u64) < self.limits.rpc_concurrency
                && now >= ready
            {
                s.queue.retain(|(t, _)| *t != ticket);
                s.active += 1;
                s.requests += 1;
                s.tokens -= 1.0;
                let rate = match work {
                    Workload::Bootstrap => self.limits.bootstrap_rps,
                    Workload::LiveRead | Workload::Deployment => self.limits.hotpath_rps,
                    Workload::Simulation => self.limits.simulation_rps,
                    Workload::History => self.limits.history_rps,
                    // C5: the configured analytics rate, but a pacing slot of its own.
                    Workload::Winner | Workload::Reconcile => self.limits.analytics_rps,
                    Workload::Websocket => self.limits.wss_rps,
                };
                s.next_workload[work.index()] = now + Duration::from_secs_f64(1.0 / rate as f64);
                self.changed.notify_all();
                return Ok(Permit {
                    budget: self.clone(),
                });
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(50));
            s = match self.changed.wait_timeout(s, wait) {
                Ok((s, _)) => s,
                Err(p) => {
                    let (mut s, _) = p.into_inner();
                    s.failed = true;
                    self.changed.notify_all();
                    s
                }
            };
        }
    }
    pub fn rate_limited(&self, retry: Duration) {
        let mut s = self.lock_state();
        s.rate_limits = s.rate_limits.saturating_add(1);
        if self.adaptive {
            s.effective_rps = (s.effective_rps / 2.0).max(1.0);
            s.tokens = s.tokens.min(1.0);
            s.successes = 0;
        }
        let circuit = Duration::from_millis(
            self.limits
                .rpc_backoff_ms
                .saturating_mul(1u64 << s.rate_limits.min(20))
                .min(self.limits.rpc_backoff_max_ms),
        );
        let jitter = Duration::from_millis((s.requests.wrapping_mul(137)) % 251);
        // Never clamp a server's longer Retry-After to our local maximum.
        if let Some(until) = Instant::now().checked_add(retry.max(circuit + jitter)) {
            s.cooldown = s.cooldown.max(until);
        } else {
            s.retry_after_overflow = true;
        }
        self.changed.notify_all();
    }
    pub fn account_bytes(&self, bytes: usize) -> Result<(), &'static str> {
        let mut s = self.lock_state();
        if let Some(code) = halted(&s) {
            return Err(code);
        }
        s.bytes = s.bytes.checked_add(bytes as u64).ok_or("RPC_BYTE_BUDGET")?;
        if s.bytes > self.limits.max_total_mib * 1024 * 1024 {
            return Err("RPC_BYTE_BUDGET");
        }
        Ok(())
    }
    pub fn degraded(&self) {
        let mut s = self.lock_state();
        if self.adaptive {
            s.effective_rps = (s.effective_rps / 2.0).max(1.0);
            s.tokens = s.tokens.min(1.0);
            s.successes = 0;
        }
        s.cooldown = s
            .cooldown
            .max(Instant::now() + Duration::from_millis(self.limits.rpc_backoff_ms));
        self.changed.notify_all();
    }
    pub fn succeeded(&self) {
        let mut s = self.lock_state();
        s.successes = s.successes.saturating_add(1);
        if self.adaptive && s.successes >= 8 {
            s.effective_rps = (s.effective_rps + 1.0).min(self.limits.rpc_rps as f64);
            s.successes = 0;
        }
    }
    pub fn snapshot(&self) -> ProviderSnapshot {
        let s = self.lock_state();
        ProviderSnapshot {
            scope: "PROCESS_SOLANA_PROVIDER_ACCOUNT".into(),
            requests: s.requests,
            http_429_count: u64::from(s.rate_limits),
            in_flight: s.active as u64,
            queued: s.queue.len() as u64,
            max_rps: self.limits.rpc_rps,
            effective_rps: s.effective_rps,
            max_concurrency: self.limits.rpc_concurrency,
            burst: self.limits.rpc_burst,
            bytes: s.bytes,
            quota_qualified: false,
        }
    }
}
/// Process-wide provider account counters at one instant (typed; serialized only at the edge).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderSnapshot {
    pub scope: String,
    pub requests: u64,
    pub http_429_count: u64,
    pub in_flight: u64,
    pub queued: u64,
    pub max_rps: u64,
    pub effective_rps: f64,
    pub max_concurrency: u64,
    pub burst: u64,
    pub bytes: u64,
    /// Provider quota is never qualified by these local counters.
    pub quota_qualified: bool,
}
/// Meter blocking read paths chunk-by-chunk against the same process budget.
pub fn read_body(
    mut reader: impl std::io::Read,
    limit: u64,
    budget: &Budget,
) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let remaining = limit
            .saturating_sub(bytes.len() as u64)
            .saturating_add(1)
            .min(chunk.len() as u64) as usize;
        let n = reader
            .read(&mut chunk[..remaining])
            .map_err(|_| "RPC_BODY_READ")?;
        if n == 0 {
            return Ok(bytes);
        }
        budget.account_bytes(n)?;
        if bytes.len() as u64 + n as u64 > limit {
            return Err("RPC_BODY_LIMIT");
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
}
pub fn retry_after(raw: Option<&str>, now: SystemTime) -> Duration {
    let Some(raw) = raw else {
        return Duration::ZERO;
    };
    if let Ok(s) = raw.trim().parse::<u64>() {
        return Duration::from_secs(s);
    }
    if let Ok(date) =
        time::OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc2822)
    {
        let now = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        return Duration::from_secs(date.unix_timestamp().saturating_sub(now).max(0) as u64);
    }
    Duration::ZERO
}
pub fn observe_response(
    response: &reqwest::blocking::Response,
    method: &str,
) -> Result<(), &'static str> {
    if !response.status().is_success() {
        let method = match method {
            "getAccountInfo" | "getTransaction" | "getBlock" | "getBlocks" | "getSlot" => method,
            _ => "OTHER",
        };
        eprintln!(
            "[PROVIDER/RPC] method={method} status={} phase=MEASUREMENT",
            response.status().as_u16()
        );
    }
    if response.status().as_u16() == 429 {
        shared().rate_limited(retry_after(
            response
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok()),
            SystemTime::now(),
        ));
        Err("RPC_RATE_LIMIT")
    } else if !response.status().is_success() {
        Err("RPC_HTTP_ERROR")
    } else {
        Ok(())
    }
}

/// C0-E1 and explicit operational profiles opt in. Legacy clients remain unchanged.
pub fn rpc_client(
    url: String,
    timeout: Duration,
    commitment: CommitmentConfig,
    work: Workload,
) -> RpcClient {
    if !enabled() {
        return RpcClient::new_with_timeout_and_commitment(url, timeout, commitment);
    }
    RpcClient::new_sender(
        BudgetedSender {
            url,
            timeout,
            work,
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_millis(
                    crate::runtime_config::active()
                        .limits
                        .rpc_connect_timeout_ms,
                ))
                .build()
                .expect("HTTP client configuration"),
            budget: shared(),
            admission: true,
            stats: Mutex::new(RpcTransportStats::default()),
        },
        RpcClientConfig::with_commitment(commitment),
    )
}
struct BudgetedSender {
    url: String,
    timeout: Duration,
    work: Workload,
    http: reqwest::Client,
    budget: Arc<Budget>,
    /// N2: `false` — the provider budget is not enabled for this process: no permit, no byte
    /// accounting, no provider degradation/backoff, no fallback endpoint and a single attempt,
    /// exactly as the unbudgeted client; the deadline-bound request path is the same.
    admission: bool,
    stats: Mutex<RpcTransportStats>,
}
struct CallTimer<'a> {
    stats: &'a Mutex<RpcTransportStats>,
    started: Instant,
}
impl Drop for CallTimer<'_> {
    fn drop(&mut self) {
        let mut s = self.stats.lock().unwrap_or_else(|p| p.into_inner());
        s.elapsed_time = s.elapsed_time.saturating_add(self.started.elapsed());
    }
}
fn rpc_error(code: &'static str) -> solana_client::client_error::ClientError {
    RpcError::RpcRequestError(code.into()).into()
}
/// N2: why a deadline-bound call failed — every case distinguishable. The code is the
/// transport's own failure code, the same text `RpcSender::send` reports for it.
#[derive(Debug)]
pub enum CallError {
    /// The deadline passed before or during the call (a permit not obtainable before it
    /// included); nothing is started after the deadline.
    Deadline(&'static str),
    /// The provider budget refused the call (queue, call/byte budget, rate limit, halt).
    Budget(&'static str),
    /// Transport, HTTP status, body or decode failure before the deadline.
    Transport(&'static str),
    /// The node answered with a JSON-RPC error.
    Remote(solana_client::client_error::ClientError),
}
impl CallError {
    fn into_client_error(self) -> solana_client::client_error::ClientError {
        match self {
            Self::Deadline(code) | Self::Budget(code) | Self::Transport(code) => rpc_error(code),
            Self::Remote(e) => e,
        }
    }
}
impl BudgetedSender {
    /// N2: one JSON-RPC call bounded by `deadline` (capped by the configured per-call timeout):
    /// the permit wait, every attempt, the HTTP request and the response read. A failure at or
    /// after the deadline is `Deadline`; no attempt starts after it (the permit is refused).
    /// `caller_deadline`: the deadline is a caller's (a candidate's) bound — a call it cuts before
    /// the transport's own limit is no provider failure (no degradation, no backoff for other
    /// workloads). Ordinary calls (`RpcSender::send`) pass `false` and keep their behaviour.
    async fn send_until(
        &self,
        request: RpcRequest,
        params: Value,
        deadline: Instant,
        caller_deadline: bool,
    ) -> Result<Value, CallError> {
        let now = Instant::now();
        let _timer = CallTimer {
            stats: &self.stats,
            started: now,
        };
        let own_limit = now + Duration::from_millis(self.budget.limits.rpc_timeout_ms);
        let caller_bound = caller_deadline && deadline < own_limit;
        let deadline = deadline.min(own_limit);
        let late = |code: &'static str| {
            if Instant::now() >= deadline {
                CallError::Deadline(code)
            } else {
                CallError::Transport(code)
            }
        };
        // Repeating submission can spend funds. This transport never retries sendTransaction.
        let attempts = if request == RpcRequest::SendTransaction || !self.admission {
            1
        } else {
            self.budget.limits.rpc_attempts
        };
        for attempt in 0..attempts {
            let work = self.work;
            let budget = self.budget.clone();
            let permit = if self.admission {
                Some(
                    tokio::task::spawn_blocking(move || budget.admit(work, deadline, None))
                        .await
                        .map_err(|_| CallError::Transport("RPC_ADMISSION_JOIN"))?
                        .map_err(|r| {
                            if r.deadline_bound || Instant::now() >= deadline {
                                CallError::Deadline(r.code)
                            } else {
                                CallError::Budget(r.code)
                            }
                        })?,
                )
            } else {
                None
            };
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(CallError::Deadline("RPC_REQUEST_BUDGET"));
            }
            self.stats
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .request_count += 1;
            let body = request.build_request_json(1, params.clone());
            let request_bytes = serde_json::to_vec(&body)
                .map_err(|_| CallError::Transport("RPC_ENCODE"))?
                .len();
            if self.admission {
                self.budget
                    .account_bytes(request_bytes)
                    .map_err(CallError::Budget)?;
            }
            let config = &self.budget;
            let endpoint = if self.admission
                && config.fallback
                && request != RpcRequest::SendTransaction
                && config.rpc_endpoints.first() == Some(&self.url)
            {
                config
                    .rpc_endpoints
                    .get((attempt as usize).min(config.rpc_endpoints.len().saturating_sub(1)))
                    .unwrap_or(&self.url)
            } else {
                &self.url
            };
            let response = self
                .http
                .post(endpoint)
                .timeout(remaining)
                .json(&body)
                .send()
                .await;
            let mut response = match response {
                Ok(r) => r,
                Err(_) => {
                    if caller_bound && Instant::now() >= deadline {
                        return Err(CallError::Deadline("RPC_TRANSPORT"));
                    }
                    if self.admission {
                        self.budget.degraded();
                    }
                    drop(permit);
                    if attempt + 1 == attempts {
                        return Err(late("RPC_TRANSPORT"));
                    }
                    continue;
                }
            };
            // Unbudgeted (as the plain client it replaces) writes no stderr of its own.
            if self.admission && !response.status().is_success() {
                eprintln!(
                    "[PROVIDER/RPC] method={request} status={} workload={:?} attempt={}",
                    response.status().as_u16(),
                    self.work,
                    attempt + 1
                );
            }
            if response.status().as_u16() == 429 {
                if self.admission {
                    self.budget.rate_limited(retry_after(
                        response
                            .headers()
                            .get("retry-after")
                            .and_then(|h| h.to_str().ok()),
                        SystemTime::now(),
                    ));
                }
                drop(permit);
                if attempt + 1 == attempts {
                    // Without a local budget the provider's own 429 is a transport answer.
                    return Err(if self.admission {
                        CallError::Budget("RPC_RATE_LIMIT")
                    } else {
                        CallError::Transport("RPC_RATE_LIMIT")
                    });
                }
                continue;
            }
            if !response.status().is_success() {
                if response.status().is_server_error() {
                    if self.admission {
                        self.budget.degraded();
                    }
                    drop(permit);
                    if attempt + 1 < attempts {
                        continue;
                    }
                }
                return Err(late("RPC_HTTP_ERROR"));
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| late("RPC_BODY_READ"))? {
                if self.admission {
                    self.budget
                        .account_bytes(chunk.len())
                        .map_err(CallError::Budget)?;
                }
                if bytes.len().saturating_add(chunk.len())
                    > (self.budget.limits.max_response_mib * 1024 * 1024) as usize
                {
                    return Err(CallError::Transport("RPC_BODY_LIMIT"));
                }
                bytes.extend_from_slice(&chunk);
            }
            let mut value: Value =
                serde_json::from_slice(&bytes).map_err(|_| CallError::Transport("RPC_JSON"))?;
            if value["error"].is_object() {
                let code = value["error"]["code"]
                    .as_i64()
                    .ok_or(CallError::Transport("RPC_ERROR_CODE"))?;
                let data = match code {
                    -32002 => serde_json::from_value(value["error"]["data"].clone())
                        .map(RpcResponseErrorData::SendTransactionPreflightFailure)
                        .unwrap_or(RpcResponseErrorData::Empty),
                    -32005 => RpcResponseErrorData::NodeUnhealthy {
                        num_slots_behind: value["error"]["data"]["numSlotsBehind"].as_u64(),
                    },
                    _ => RpcResponseErrorData::Empty,
                };
                return Err(CallError::Remote(
                    RpcError::RpcResponseError {
                        code,
                        message: "RPC_REMOTE_ERROR".into(),
                        data,
                    }
                    .into(),
                ));
            }
            if self.admission {
                self.budget.succeeded();
            }
            return value
                .get_mut("result")
                .map(Value::take)
                .ok_or(CallError::Transport("RPC_MISSING_RESULT"));
        }
        Err(late("RPC_RETRY_EXHAUSTED"))
    }
}
#[async_trait::async_trait]
impl RpcSender for BudgetedSender {
    async fn send(
        &self,
        request: RpcRequest,
        params: Value,
    ) -> solana_client::client_error::Result<Value> {
        let deadline = Instant::now()
            + self
                .timeout
                .min(Duration::from_millis(self.budget.limits.rpc_timeout_ms));
        self.send_until(request, params, deadline, false)
            .await
            .map_err(CallError::into_client_error)
    }
    fn get_transport_stats(&self) -> RpcTransportStats {
        self.stats.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn url(&self) -> String {
        self.url.clone()
    }
}

/// N2: blocking JSON-RPC calls bounded by a caller deadline through the budgeted transport —
/// the same permits, attempts, HTTP client settings and accounting as `rpc_client`, no second
/// RPC stack. Built once per adapter (its runtime and connection pool are reused).
pub struct DeadlineRpc {
    sender: BudgetedSender,
    runtime: tokio::runtime::Runtime,
}
impl DeadlineRpc {
    /// Like `rpc_client`: against the process-wide provider budget when it is enabled
    /// (`enable_measurement_budget`), otherwise unbudgeted — the deadline binds in both.
    pub fn new(url: String, work: Workload) -> Result<Self, String> {
        Self::build(url, work, shared(), enabled())
    }
    /// Against an explicit budget (an isolated profile; tests that hold its permits).
    pub fn with_budget(url: String, work: Workload, budget: Arc<Budget>) -> Result<Self, String> {
        Self::build(url, work, budget, true)
    }
    /// Unbudgeted (the provider budget not enabled), with `budget` only for its limits.
    #[cfg(test)]
    fn unbudgeted(url: String, work: Workload, budget: Arc<Budget>) -> Result<Self, String> {
        Self::build(url, work, budget, false)
    }
    fn build(
        url: String,
        work: Workload,
        budget: Arc<Budget>,
        admission: bool,
    ) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_millis(
                crate::runtime_config::active()
                    .limits
                    .rpc_connect_timeout_ms,
            ))
            .build()
            .map_err(|_| "DEADLINE_RPC_HTTP_CLIENT".to_string())?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "DEADLINE_RPC_RUNTIME".to_string())?;
        Ok(Self {
            sender: BudgetedSender {
                url,
                timeout: Duration::from_millis(budget.limits.rpc_timeout_ms),
                work,
                http,
                budget,
                admission,
                stats: Mutex::new(RpcTransportStats::default()),
            },
            runtime,
        })
    }
    /// One call; an expired deadline starts nothing.
    pub fn call(
        &self,
        request: RpcRequest,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, CallError> {
        if Instant::now() >= deadline {
            return Err(CallError::Deadline("RPC_DEADLINE"));
        }
        self.runtime
            .block_on(self.sender.send_until(request, params, deadline, true))
    }
    /// Requests this transport sent so far.
    pub fn request_count(&self) -> usize {
        self.sender.get_transport_stats().request_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_capacity_and_exhaustion_release_all_tickets() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.rpc_rps = 100;
        c.limits.rpc_burst = 4;
        c.limits.rpc_concurrency = 4;
        c.limits.max_requests = 4;
        let b = Arc::new(Budget::configured(&c));
        let deadline = Instant::now() + Duration::from_secs(1);
        let permits = [
            Workload::Bootstrap,
            Workload::LiveRead,
            Workload::Simulation,
            Workload::Winner,
        ]
        .map(|w| b.acquire(w, deadline).unwrap());
        assert_eq!(b.snapshot().in_flight, 4);
        drop(permits);
        assert!(matches!(
            b.acquire(Workload::History, deadline),
            Err("RPC_CALL_BUDGET")
        ));
        assert_eq!(b.snapshot().queued, 0);
        let b = Arc::new(Budget::configured(&c));
        assert_eq!(
            b.account_bytes((c.limits.max_total_mib * 1024 * 1024 + 1) as usize),
            Err("RPC_BYTE_BUDGET")
        );
        assert!(matches!(
            b.acquire(Workload::History, deadline),
            Err("RPC_BYTE_BUDGET")
        ));
        assert_eq!(b.snapshot().requests, 0);
    }
    #[test]
    fn adaptive_recovery_is_bounded_and_keeps_provider_unknown() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.rpc_rps = 20;
        let b = Budget::configured(&c);
        b.rate_limited(Duration::ZERO);
        assert_eq!(b.snapshot().effective_rps, 10.0);
        for _ in 0..800 {
            b.succeeded();
        }
        assert_eq!(b.snapshot().effective_rps, 20.0);
        assert!(!b.snapshot().quota_qualified);
        b.degraded();
        assert_eq!(b.snapshot().effective_rps, 10.0);
        assert_eq!(b.snapshot().http_429_count, 1);
    }
    #[test]
    fn socket_capacity_is_local_bounded_and_released() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.wss_connections = 1;
        c.limits.wss_subscriptions = 2;
        let b = Arc::new(Budget::configured(&c));
        assert!(matches!(b.acquire_socket(3), Err("WSS_SUBSCRIPTION_LIMIT")));
        let lease = b.acquire_socket(2).unwrap();
        assert!(matches!(b.acquire_socket(1), Err("WSS_CONNECTION_LIMIT")));
        let other = Arc::new(Budget::configured(&c));
        assert!(other.acquire_socket(1).is_ok());
        drop(lease);
        assert!(b.acquire_socket(2).is_ok());
    }
    #[test]
    fn blocking_body_read_is_metered_and_bounded_before_json_decode() {
        let b = Budget::default();
        assert_eq!(
            read_body(std::io::Cursor::new(b"12345"), 4, &b),
            Err("RPC_BODY_LIMIT")
        );
        assert_eq!(b.snapshot().bytes, 5);
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.max_total_mib = 1;
        let b = Budget::configured(&c);
        assert_eq!(
            read_body(std::io::repeat(0), 2 * 1024 * 1024, &b),
            Err("RPC_BYTE_BUDGET")
        );
        assert!(b.snapshot().bytes <= 1024 * 1024 + 8192);
    }
    #[test]
    fn queue_backpressure_does_not_allocate_more_tickets() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.rpc_queue = 1;
        let b = Arc::new(Budget::configured(&c));
        b.state
            .lock()
            .unwrap()
            .queue
            .push_back((99, Workload::History));
        assert!(matches!(
            b.acquire(Workload::LiveRead, Instant::now() + Duration::from_secs(1)),
            Err("RPC_ADMISSION_QUEUE_FULL")
        ));
        assert_eq!(b.snapshot().queued, 1);
    }
    #[test]
    fn retry_after_seconds_dates_and_invalid_are_safe() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784111777);
        assert_eq!(retry_after(Some("120"), now), Duration::from_secs(120));
        assert_eq!(
            retry_after(Some("Sun, 06 Nov 1994 08:49:47 GMT"), now),
            Duration::from_secs(10)
        );
        assert_eq!(retry_after(Some("bogus"), now), Duration::ZERO);
        assert_eq!(retry_after(None, now), Duration::ZERO);
    }
    #[test]
    fn shared_429_circuit_prevents_requests_across_workloads() {
        let b = Arc::new(Budget::default());
        b.rate_limited(Duration::from_secs(86400));
        for w in [
            Workload::History,
            Workload::Bootstrap,
            Workload::Simulation,
            Workload::Deployment,
            Workload::Websocket,
        ] {
            assert!(matches!(
                b.acquire(w, Instant::now() + Duration::from_millis(50)),
                Err("RPC_RATE_LIMIT")
            ));
        }
        assert_eq!(b.snapshot().requests, 0);
        assert_eq!(b.snapshot().queued, 0);
    }
    /// C5: a paced reconciliation never holds back hot-path reads: while its (shared
    /// analytics) pacing slot is not ready, a queued LiveRead is admitted at once.
    #[test]
    fn c5_paced_reconcile_never_blocks_live_reads() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.rpc_rps = 1_000;
        c.limits.rpc_burst = 1_000;
        c.limits.rpc_concurrency = 4;
        c.limits.analytics_rps = 1;
        let b = Arc::new(Budget::configured(&c));
        let deadline = Instant::now() + Duration::from_secs(5);
        drop(b.acquire(Workload::Winner, deadline).unwrap()); // analytics slot paced ~1 s
        let budget = b.clone();
        let reconcile = std::thread::spawn(move || {
            let started = Instant::now();
            drop(budget.acquire(Workload::Reconcile, deadline).unwrap());
            started.elapsed()
        });
        while b.snapshot().queued < 1 {
            std::thread::sleep(Duration::from_millis(1));
        }
        let started = Instant::now();
        drop(b.acquire(Workload::LiveRead, deadline).unwrap());
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "{:?}",
            started.elapsed()
        );
        let waited = reconcile.join().unwrap();
        assert!(
            waited >= Duration::from_millis(500),
            "shared pacing slot kept: {waited:?}"
        );
    }
    /// C5: a reconciliation waiting behind many analytics jobs is admitted first, inside the
    /// same concurrency and call budget.
    #[test]
    fn c5_reconcile_is_admitted_before_queued_winner_jobs_within_the_budget() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.rpc_rps = 1_000;
        c.limits.rpc_burst = 1_000;
        c.limits.rpc_concurrency = 1;
        c.limits.max_requests = 100;
        let b = Arc::new(Budget::configured(&c));
        let deadline = Instant::now() + Duration::from_secs(5);
        let held = b.acquire(Workload::LiveRead, deadline).unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut threads = Vec::new();
        for (i, w) in std::iter::repeat_n(Workload::Winner, 6)
            .chain([Workload::Reconcile])
            .enumerate()
        {
            let (budget, order) = (b.clone(), order.clone());
            threads.push(std::thread::spawn(move || {
                let p = budget.acquire(w, deadline);
                order
                    .lock()
                    .unwrap()
                    .push((i, matches!(w, Workload::Reconcile)));
                drop(p);
            }));
            while b.snapshot().queued < (i + 1) as u64 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        drop(held);
        for t in threads {
            t.join().unwrap();
        }
        let order = order.lock().unwrap();
        assert_eq!(order.len(), 7);
        assert!(order[0].1, "reconcile first: {order:?}");
        let s = b.snapshot();
        assert!(s.requests <= c.limits.max_requests && s.in_flight == 0 && s.queued == 0);
    }
    #[test]
    fn aggregate_concurrency_is_bounded_and_cancelled_tickets_do_not_block() {
        let b = Arc::new(Budget::default());
        let first = b
            .acquire(Workload::Bootstrap, Instant::now() + Duration::from_secs(2))
            .unwrap();
        let second = b
            .acquire(Workload::LiveRead, Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(b.snapshot().in_flight, 2);
        assert!(b
            .acquire(Workload::Winner, Instant::now() + Duration::from_millis(50))
            .is_err());
        drop(first);
        drop(second);
        let _next = b
            .acquire(Workload::History, Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(b.snapshot().queued, 0);
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use std::io::{Read, Write};
    fn server(reply: String) -> (String, std::thread::JoinHandle<()>) {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let t = std::thread::spawn(move || {
            let (mut c, _) = socket.accept().unwrap();
            c.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut data = Vec::new();
            let mut b = [0u8; 1024];
            loop {
                let n = c.read(&mut b).unwrap();
                assert!(n > 0);
                data.extend_from_slice(&b[..n]);
                if let Some(end) = data.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&data[..end]);
                    let len = headers
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap();
                    if data.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            c.write_all(reply.as_bytes()).unwrap();
        });
        (url, t)
    }
    fn sender(url: String) -> BudgetedSender {
        BudgetedSender {
            url,
            timeout: Duration::from_millis(500),
            work: Workload::Simulation,
            http: reqwest::Client::new(),
            budget: Arc::new(Budget::default()),
            admission: true,
            stats: Mutex::new(RpcTransportStats::default()),
        }
    }
    /// N2: without an enabled provider budget the candidate transport behaves like the
    /// unbudgeted client — no permit (a fully held budget does not block it), no degradation —
    /// and the candidate deadline still ends a hanging request.
    #[test]
    fn n2_unbudgeted_candidate_transport_needs_no_permit_and_keeps_the_deadline() {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let hold = std::thread::spawn(move || {
            let (c, _) = socket.accept().unwrap();
            std::thread::sleep(Duration::from_secs(1));
            drop(c);
        });
        let mut config = crate::runtime_config::RuntimeConfig::default();
        config.limits.rpc_concurrency = 1;
        config.limits.rpc_backoff_ms = 5_000;
        let budget = Arc::new(Budget::configured(&config));
        let held = budget
            .acquire(Workload::LiveRead, Instant::now() + Duration::from_secs(5))
            .unwrap();
        let rpc = DeadlineRpc::unbudgeted(url, Workload::Simulation, budget.clone()).unwrap();
        let started = Instant::now();
        let deadline = started + Duration::from_millis(300);
        let result = rpc.call(RpcRequest::GetSlot, serde_json::json!([]), deadline);
        assert!(matches!(result, Err(CallError::Deadline(_))), "{result:?}");
        assert!(
            started.elapsed() <= Duration::from_millis(400),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(rpc.request_count(), 1, "sent without a permit");
        drop(held);
        assert!(
            budget
                .acquire(
                    Workload::LiveRead,
                    Instant::now() + Duration::from_millis(100)
                )
                .is_ok(),
            "no degradation without the budget"
        );
        hold.join().unwrap();
    }
    /// N2 legacy contract: an ordinary call (`RpcSender::send`) that hangs until its own limit is
    /// a provider failure as before — the provider is degraded (backoff for every workload) and
    /// the retry the backoff refuses reports the old code `RPC_RATE_LIMIT`. Only a candidate
    /// deadline (`DeadlineRpc`) cuts without degrading.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn n2_legacy_send_still_degrades_a_hanging_provider() {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let hold = std::thread::spawn(move || {
            let (c, _) = socket.accept().unwrap();
            std::thread::sleep(Duration::from_secs(2));
            drop(c);
        });
        let mut config = crate::runtime_config::RuntimeConfig::default();
        config.limits.rpc_rps = 100;
        config.limits.simulation_rps = 100;
        config.limits.rpc_backoff_ms = 5_000;
        config.limits.rpc_attempts = 3;
        let mut s = sender(url);
        s.timeout = Duration::from_millis(300);
        s.budget = Arc::new(Budget::configured(&config));
        let error = s
            .send(RpcRequest::GetSlot, serde_json::json!([]))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("RPC_RATE_LIMIT"), "{error}");
        assert!(
            matches!(
                s.budget.acquire(
                    Workload::LiveRead,
                    Instant::now() + Duration::from_millis(200)
                ),
                Err("RPC_RATE_LIMIT")
            ),
            "degraded: every workload backs off"
        );
        hold.join().unwrap();
    }
    #[tokio::test]
    async fn explicit_read_fallback_shares_budget_and_send_is_never_retried() {
        let (primary, a) = server(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .into(),
        );
        let (backup,b)=server("HTTP/1.1 200 OK\r\nContent-Length: 36\r\nConnection: close\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":42}".into());
        let mut config = crate::runtime_config::RuntimeConfig {
            rpc_endpoints: vec![primary.clone(), backup],
            fallback: true,
            ..Default::default()
        };
        config.limits.rpc_backoff_ms = 1;
        config.limits.rpc_rps = 100;
        config.limits.simulation_rps = 100;
        let mut s = sender(primary);
        s.timeout = Duration::from_secs(2);
        s.budget = Arc::new(Budget::configured(&config));
        assert_eq!(
            s.send(RpcRequest::GetSlot, serde_json::json!([]))
                .await
                .unwrap(),
            42
        );
        assert_eq!(s.budget.snapshot().requests, 2);
        a.join().unwrap();
        b.join().unwrap();
        let (url, t) = server(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .into(),
        );
        config.rpc_endpoints = vec![url.clone(), "http://127.0.0.1:1".into()];
        let mut s = sender(url);
        s.budget = Arc::new(Budget::configured(&config));
        assert!(s
            .send(RpcRequest::SendTransaction, serde_json::json!([]))
            .await
            .is_err());
        assert_eq!(s.budget.snapshot().requests, 1);
        t.join().unwrap();
    }
    #[tokio::test]
    async fn actual_http_429_blocks_other_workload_and_does_not_leak_body() {
        let (url,t)=server("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 86400\r\nContent-Length: 7\r\nConnection: close\r\n\r\nPRIVATE".into());
        let s = sender(url);
        let err = s
            .send(RpcRequest::GetSlot, serde_json::json!([]))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("RPC_RATE_LIMIT"));
        assert!(!err.to_string().contains("PRIVATE"));
        assert_eq!(s.get_transport_stats().request_count, 1);
        assert!(s
            .budget
            .acquire(Workload::History, Instant::now() + Duration::from_millis(5))
            .is_err());
        t.join().unwrap();
    }
    #[tokio::test]
    async fn sdk_preserves_success_response_and_structured_simulation_failure() {
        let (url,t)=server("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 37\r\nConnection: close\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":123}".into());
        assert_eq!(
            sender(url)
                .send(RpcRequest::GetSlot, serde_json::json!([]))
                .await
                .unwrap(),
            serde_json::json!(123)
        );
        t.join().unwrap();
        // The protocol error code must survive the wrapper. No remote free-text body is logged.
        let body=serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32002,"message":"PRIVATE","data":{"err":null,"logs":[],"accounts":null,"unitsConsumed":7,"returnData":null,"innerInstructions":null,"replacementBlockhash":null}}}).to_string();
        let reply=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
        let (url, t) = server(reply);
        let err = sender(url)
            .send(RpcRequest::SimulateTransaction, serde_json::json!([]))
            .await
            .unwrap_err();
        assert!(matches!(
            err.kind(),
            solana_client::client_error::ClientErrorKind::RpcError(RpcError::RpcResponseError {
                code: -32002,
                data: RpcResponseErrorData::SendTransactionPreflightFailure(_),
                ..
            })
        ));
        assert!(!err.to_string().contains("PRIVATE"));
        t.join().unwrap();
    }
}

#[cfg(test)]
mod poison_tests {
    use super::*;
    #[test]
    fn poisoned_permits_drop_during_unwind_without_second_panic_and_block_admission() {
        let b = Arc::new(Budget::default());
        let p = b
            .acquire(Workload::LiveRead, Instant::now() + Duration::from_secs(1))
            .unwrap();
        let w = b.acquire_socket(1).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lock = b.state.lock().unwrap();
            panic!("injected")
        }));
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _p = p;
            let _w = w;
            panic!("unwind with permits")
        }));
        assert!(failed.is_err());
        assert_eq!(b.snapshot().in_flight, 0);
        assert!(matches!(
            b.acquire(Workload::LiveRead, Instant::now() + Duration::from_secs(1)),
            Err("PROVIDER_BUDGET_POISONED")
        ));
        assert!(matches!(
            b.acquire_socket(1),
            Err("PROVIDER_BUDGET_POISONED")
        ));
        assert_eq!(b.account_bytes(1), Err("PROVIDER_BUDGET_POISONED"));
    }
    #[test]
    fn poisoned_waiter_and_metrics_are_controlled() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.rpc_concurrency = 1;
        let b = Arc::new(Budget::configured(&c));
        let p = b
            .acquire(Workload::LiveRead, Instant::now() + Duration::from_secs(1))
            .unwrap();
        let other = b.clone();
        let t = std::thread::spawn(move || {
            other
                .acquire(Workload::LiveRead, Instant::now() + Duration::from_secs(2))
                .err()
        });
        while b.snapshot().queued == 0 {
            std::thread::yield_now();
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _s = b.state.lock().unwrap();
            panic!("injected")
        }));
        b.changed.notify_all();
        assert_eq!(t.join().unwrap(), Some("PROVIDER_BUDGET_POISONED"));
        drop(p);
        assert_eq!(b.snapshot().queued, 0);
        let stats = Mutex::new(RpcTransportStats::default());
        let _ = std::panic::catch_unwind(|| {
            let _s = stats.lock().unwrap();
            panic!("injected")
        });
        assert!(std::panic::catch_unwind(|| {
            let _timer = CallTimer {
                stats: &stats,
                started: Instant::now(),
            };
            panic!("unwind");
        })
        .is_err());
    }
}

#[cfg(test)]
mod configuration_tests {
    use super::*;
    #[test]
    fn early_default_budget_cannot_authorize_a_later_profile() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        let b = Arc::new(Budget::configured(&c));
        c.limits.rpc_rps += 1;
        b.check_configuration(&c);
        // N4: a drift has its own sticky code on every entry; it is not a poisoned mutex.
        assert!(matches!(
            b.acquire(Workload::Bootstrap, Instant::now() + Duration::from_secs(1)),
            Err("PROVIDER_BUDGET_CONFIG_DRIFT")
        ));
        assert!(matches!(
            b.acquire_socket(1),
            Err("PROVIDER_BUDGET_CONFIG_DRIFT")
        ));
        assert_eq!(b.account_bytes(1), Err("PROVIDER_BUDGET_CONFIG_DRIFT"));
        // Reverting the configuration does not reinitialize the budget.
        c.limits.rpc_rps -= 1;
        b.check_configuration(&c);
        assert_eq!(b.account_bytes(1), Err("PROVIDER_BUDGET_CONFIG_DRIFT"));
        // Endpoint drift: same code; the configured budget stays poison-free.
        let mut e = crate::runtime_config::RuntimeConfig::default();
        let b = Arc::new(Budget::configured(&e));
        e.rpc_endpoints = vec!["https://x.invalid/?api-key=SECRET".into()];
        b.check_configuration(&e);
        assert_eq!(b.account_bytes(1), Err("PROVIDER_BUDGET_CONFIG_DRIFT"));
        assert!(!b.lock_state().failed);
    }
}
