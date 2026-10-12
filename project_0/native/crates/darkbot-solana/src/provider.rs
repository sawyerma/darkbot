//! C0-E1 process-wide Solana provider budget. All endpoints share a conservative
//! account-level ceiling: distinct URLs are NOT evidence of independent quotas.
//! FIFO admission + per-workload spacing, 2 in flight, 5 starts/sec aggregate.
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
    fn spacing(self) -> Duration {
        Duration::from_millis(match self {
            Self::History | Self::Simulation | Self::Winner => 500,
            Self::Websocket => 1000,
            _ => 200,
        })
    }
}
struct State {
    queue: VecDeque<u64>,
    next_ticket: u64,
    active: usize,
    next_global: Instant,
    next_workload: [Instant; 7],
    cooldown: Instant,
    rate_limits: u32,
    requests: u64,
    retry_after_overflow: bool,
}
pub struct Budget {
    state: Mutex<State>,
    changed: Condvar,
}
pub struct Permit {
    budget: Arc<Budget>,
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
        let now = Instant::now();
        Self {
            state: Mutex::new(State {
                queue: VecDeque::new(),
                next_ticket: 0,
                active: 0,
                next_global: now,
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
        if s.queue.len() >= 64 {
            return Err("RPC_ADMISSION_QUEUE_FULL");
        }
        let ticket = s.next_ticket;
        s.next_ticket += 1;
        s.queue.push_back(ticket);
        loop {
            let now = Instant::now();
            let ready = s
                .next_global
                .max(s.next_workload[work.index()])
                .max(s.cooldown);
            let cancelled = shutdown.is_some_and(|s| s.is_requested());
            if s.retry_after_overflow || cancelled || now >= deadline || ready >= deadline {
                s.queue.retain(|t| *t != ticket);
                self.changed.notify_all();
                return Err(if cancelled {
                    "RPC_ADMISSION_CANCELLED"
                } else if s.retry_after_overflow || s.cooldown > now {
                    "RPC_RATE_LIMIT"
                } else {
                    "RPC_ADMISSION_BUDGET"
                });
            }
            if s.queue.front() == Some(&ticket) && s.active < 2 && now >= ready {
                s.queue.pop_front();
                s.active += 1;
                s.requests += 1;
                s.next_global = now + Duration::from_millis(200);
                s.next_workload[work.index()] = now + work.spacing();
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
        let backoff = Duration::from_secs(1u64 << s.rate_limits.min(5));
        let circuit = if s.rate_limits >= 3 {
            Duration::from_secs(60)
        } else {
            backoff
        };
        let jitter = Duration::from_millis((s.requests.wrapping_mul(137)) % 251);
        // Never clamp a server's longer Retry-After to our local maximum.
        if let Some(until) = Instant::now().checked_add(retry.max(circuit + jitter)) {
            s.cooldown = s.cooldown.max(until);
        } else {
            s.retry_after_overflow = true;
        }
        self.changed.notify_all();
    }
    pub fn snapshot(&self) -> Value {
        let s = self.state.lock().unwrap();
        serde_json::json!({"scope":"PROCESS_SOLANA_PROVIDER_ACCOUNT","requests":s.requests,"http_429_count":s.rate_limits,"in_flight":s.active,"queued":s.queue.len(),"max_rps":5,"max_concurrency":2,"quota_qualified":false})
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

/// Only C0-E1 opts in; normal submission/client construction remains unchanged.
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
            http: reqwest::Client::new(),
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
        let deadline = Instant::now() + self.timeout;
        // Repeating submission can spend funds. This transport never retries sendTransaction.
        let attempts = if request == RpcRequest::SendTransaction {
            1
        } else {
            3
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
            let mut response = self
                .http
                .post(&self.url)
                .timeout(remaining)
                .json(&request.build_request_json(1, params.clone()))
                .send()
                .await
                .map_err(|_| rpc_error("RPC_TRANSPORT"))?;
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
                return Err(rpc_error("RPC_HTTP_ERROR"));
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| rpc_error("RPC_BODY_READ"))?
            {
                if bytes.len().saturating_add(chunk.len())
                    > match self.work {
                        Workload::Bootstrap => 512 * 1024 * 1024,
                        _ => 64 * 1024 * 1024,
                    }
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
