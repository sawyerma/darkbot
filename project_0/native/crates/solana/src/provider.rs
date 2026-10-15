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
        Arc, Condvar, Mutex, OnceLock,
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
}
impl Workload {
    fn index(self) -> usize {
        self as usize
    }
}
struct State {
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
        let mut s = self.budget.state.lock().unwrap();
        s.sockets -= 1;
        s.subscriptions -= self.subscriptions;
        self.budget.changed.notify_all();
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut s = self.budget.state.lock().unwrap();
        s.active -= 1;
        self.budget.changed.notify_all();
    }
}
impl Default for Budget {
    fn default() -> Self {
        Self::configured(crate::runtime_config::active())
    }
}
impl Budget {
    pub fn configured(config: &crate::runtime_config::RuntimeConfig) -> Self {
        let now = Instant::now();
        Self {
            limits: config.limits.clone(),
            adaptive: config.adaptive,
            fallback: config.fallback,
            rpc_endpoints: config.rpc_endpoints.clone(),
            state: Mutex::new(State {
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
pub fn shared() -> Arc<Budget> {
    static B: OnceLock<Arc<Budget>> = OnceLock::new();
    B.get_or_init(|| Arc::new(Budget::default())).clone()
}
impl Budget {
    pub fn acquire_socket(
        self: &Arc<Self>,
        subscriptions: u64,
    ) -> Result<SocketPermit, &'static str> {
        let mut s = self.state.lock().unwrap();
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
        let mut s = self.state.lock().unwrap();
        if s.retry_after_overflow {
            return Err("RPC_RATE_LIMIT");
        }
        if s.queue.len() as u64 >= self.limits.rpc_queue {
            return Err("RPC_ADMISSION_QUEUE_FULL");
        }
        let ticket = s.next_ticket;
        s.next_ticket += 1;
        s.queue.push_back((ticket, work));
        loop {
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
                return Err(if cancelled {
                    "RPC_ADMISSION_CANCELLED"
                } else if s.retry_after_overflow || s.cooldown > now {
                    "RPC_RATE_LIMIT"
                } else {
                    "RPC_ADMISSION_BUDGET"
                });
            }
            if s.bytes >= self.limits.max_total_mib * 1024 * 1024 {
                s.queue.retain(|(t, _)| *t != ticket);
                self.changed.notify_all();
                return Err("RPC_BYTE_BUDGET");
            }
            if s.requests >= self.limits.max_requests {
                s.queue.retain(|(t, _)| *t != ticket);
                self.changed.notify_all();
                return Err("RPC_CALL_BUDGET");
            }
            // Reserve every eighth admission for the oldest waiter; otherwise
            // prioritize health/state/simulation without starving reconciliation.
            let selected = if s.requests % 8 == 7 {
                s.queue.front().map(|(t, _)| *t)
            } else {
                s.queue
                    .iter()
                    .min_by_key(|(t, w)| {
                        (
                            match w {
                                Workload::LiveRead | Workload::Simulation => 0,
                                Workload::Bootstrap | Workload::Websocket => 1,
                                _ => 2,
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
                    Workload::Winner => self.limits.analytics_rps,
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
            s = self.changed.wait_timeout(s, wait).unwrap().0;
        }
    }
    pub fn rate_limited(&self, retry: Duration) {
        let mut s = self.state.lock().unwrap();
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
        let mut s = self.state.lock().unwrap();
        s.bytes = s.bytes.checked_add(bytes as u64).ok_or("RPC_BYTE_BUDGET")?;
        if s.bytes > self.limits.max_total_mib * 1024 * 1024 {
            return Err("RPC_BYTE_BUDGET");
        }
        Ok(())
    }
    pub fn degraded(&self) {
        let mut s = self.state.lock().unwrap();
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
        let mut s = self.state.lock().unwrap();
        s.successes = s.successes.saturating_add(1);
        if self.adaptive && s.successes >= 8 {
            s.effective_rps = (s.effective_rps + 1.0).min(self.limits.rpc_rps as f64);
            s.successes = 0;
        }
    }
    pub fn snapshot(&self) -> Value {
        let s = self.state.lock().unwrap();
        serde_json::json!({"scope":"PROCESS_SOLANA_PROVIDER_ACCOUNT","requests":s.requests,"http_429_count":s.rate_limits,"in_flight":s.active,"queued":s.queue.len(),"max_rps":self.limits.rpc_rps,"effective_rps":s.effective_rps,"max_concurrency":self.limits.rpc_concurrency,"burst":self.limits.rpc_burst,"bytes":s.bytes,"quota_qualified":false})
    }
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
    stats: Mutex<RpcTransportStats>,
}
struct CallTimer<'a> {
    stats: &'a Mutex<RpcTransportStats>,
    started: Instant,
}
impl Drop for CallTimer<'_> {
    fn drop(&mut self) {
        self.stats.lock().unwrap().elapsed_time += self.started.elapsed();
    }
}
fn rpc_error(code: &'static str) -> solana_client::client_error::ClientError {
    RpcError::RpcRequestError(code.into()).into()
}
#[async_trait::async_trait]
impl RpcSender for BudgetedSender {
    async fn send(
        &self,
        request: RpcRequest,
        params: Value,
    ) -> solana_client::client_error::Result<Value> {
        let _timer = CallTimer {
            stats: &self.stats,
            started: Instant::now(),
        };
        let deadline = Instant::now()
            + self
                .timeout
                .min(Duration::from_millis(self.budget.limits.rpc_timeout_ms));
        // Repeating submission can spend funds. This transport never retries sendTransaction.
        let attempts = if request == RpcRequest::SendTransaction {
            1
        } else {
            self.budget.limits.rpc_attempts
        };
        for attempt in 0..attempts {
            let work = self.work;
            let budget = self.budget.clone();
            let permit = tokio::task::spawn_blocking(move || budget.acquire(work, deadline))
                .await
                .map_err(|_| rpc_error("RPC_ADMISSION_JOIN"))?
                .map_err(rpc_error)?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(rpc_error("RPC_REQUEST_BUDGET"));
            }
            self.stats.lock().unwrap().request_count += 1;
            let body = request.build_request_json(1, params.clone());
            self.budget
                .account_bytes(
                    serde_json::to_vec(&body)
                        .map_err(|_| rpc_error("RPC_ENCODE"))?
                        .len(),
                )
                .map_err(rpc_error)?;
            let config = &self.budget;
            let endpoint = if config.fallback
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
                    self.budget.degraded();
                    drop(permit);
                    if attempt + 1 == attempts {
                        return Err(rpc_error("RPC_TRANSPORT"));
                    }
                    continue;
                }
            };
            if !response.status().is_success() {
                eprintln!(
                    "[PROVIDER/RPC] method={request} status={} workload={:?} attempt={}",
                    response.status().as_u16(),
                    self.work,
                    attempt + 1
                );
            }
            if response.status().as_u16() == 429 {
                self.budget.rate_limited(retry_after(
                    response
                        .headers()
                        .get("retry-after")
                        .and_then(|h| h.to_str().ok()),
                    SystemTime::now(),
                ));
                drop(permit);
                if attempt + 1 == attempts {
                    return Err(rpc_error("RPC_RATE_LIMIT"));
                }
                continue;
            }
            if !response.status().is_success() {
                if response.status().is_server_error() {
                    self.budget.degraded();
                    drop(permit);
                    if attempt + 1 < attempts {
                        continue;
                    }
                }
                return Err(rpc_error("RPC_HTTP_ERROR"));
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| rpc_error("RPC_BODY_READ"))?
            {
                self.budget.account_bytes(chunk.len()).map_err(rpc_error)?;
                if bytes.len().saturating_add(chunk.len())
                    > (self.budget.limits.max_response_mib * 1024 * 1024) as usize
                {
                    return Err(rpc_error("RPC_BODY_LIMIT"));
                }
                bytes.extend_from_slice(&chunk);
            }
            let mut value: Value =
                serde_json::from_slice(&bytes).map_err(|_| rpc_error("RPC_JSON"))?;
            if value["error"].is_object() {
                let code = value["error"]["code"]
                    .as_i64()
                    .ok_or_else(|| rpc_error("RPC_ERROR_CODE"))?;
                let data = match code {
                    -32002 => serde_json::from_value(value["error"]["data"].clone())
                        .map(RpcResponseErrorData::SendTransactionPreflightFailure)
                        .unwrap_or(RpcResponseErrorData::Empty),
                    -32005 => RpcResponseErrorData::NodeUnhealthy {
                        num_slots_behind: value["error"]["data"]["numSlotsBehind"].as_u64(),
                    },
                    _ => RpcResponseErrorData::Empty,
                };
                return Err(RpcError::RpcResponseError {
                    code,
                    message: "RPC_REMOTE_ERROR".into(),
                    data,
                }
                .into());
            }
            self.budget.succeeded();
            return value
                .get_mut("result")
                .map(Value::take)
                .ok_or_else(|| rpc_error("RPC_MISSING_RESULT"));
        }
        Err(rpc_error("RPC_RETRY_EXHAUSTED"))
    }
    fn get_transport_stats(&self) -> RpcTransportStats {
        self.stats.lock().unwrap().clone()
    }
    fn url(&self) -> String {
        self.url.clone()
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
        assert_eq!(b.snapshot()["in_flight"], 4);
        drop(permits);
        assert!(matches!(
            b.acquire(Workload::History, deadline),
            Err("RPC_CALL_BUDGET")
        ));
        assert_eq!(b.snapshot()["queued"], 0);
        let b = Arc::new(Budget::configured(&c));
        assert_eq!(
            b.account_bytes((c.limits.max_total_mib * 1024 * 1024 + 1) as usize),
            Err("RPC_BYTE_BUDGET")
        );
        assert!(matches!(
            b.acquire(Workload::History, deadline),
            Err("RPC_BYTE_BUDGET")
        ));
        assert_eq!(b.snapshot()["requests"], 0);
    }
    #[test]
    fn adaptive_recovery_is_bounded_and_keeps_provider_unknown() {
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.rpc_rps = 20;
        let b = Budget::configured(&c);
        b.rate_limited(Duration::ZERO);
        assert_eq!(b.snapshot()["effective_rps"], 10.0);
        for _ in 0..800 {
            b.succeeded();
        }
        assert_eq!(b.snapshot()["effective_rps"], 20.0);
        assert_eq!(b.snapshot()["quota_qualified"], false);
        b.degraded();
        assert_eq!(b.snapshot()["effective_rps"], 10.0);
        assert_eq!(b.snapshot()["http_429_count"], 1);
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
        assert_eq!(b.snapshot()["bytes"], 5);
        let mut c = crate::runtime_config::RuntimeConfig::default();
        c.limits.max_total_mib = 1;
        let b = Budget::configured(&c);
        assert_eq!(
            read_body(std::io::repeat(0), 2 * 1024 * 1024, &b),
            Err("RPC_BYTE_BUDGET")
        );
        assert!(b.snapshot()["bytes"].as_u64().unwrap() <= 1024 * 1024 + 8192);
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
        assert_eq!(b.snapshot()["queued"], 1);
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
        assert_eq!(b.snapshot()["requests"], 0);
        assert_eq!(b.snapshot()["queued"], 0);
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
        assert_eq!(b.snapshot()["in_flight"], 2);
        assert!(b
            .acquire(Workload::Winner, Instant::now() + Duration::from_millis(50))
            .is_err());
        drop(first);
        drop(second);
        let _next = b
            .acquire(Workload::History, Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(b.snapshot()["queued"], 0);
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
            stats: Mutex::new(RpcTransportStats::default()),
        }
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
        assert_eq!(s.budget.snapshot()["requests"], 2);
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
        assert_eq!(s.budget.snapshot()["requests"], 1);
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
