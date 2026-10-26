//! S1-03: `WsRpcControlSource` — JSON-WebSocket-Ingress (Long-Tail/Premium, Bootstrap, Evidence).
//!
//! Eigenschaften:
//! - T_rx wird **beim Frame-Empfang** gesetzt, vor jedem JSON-Parse.
//! - Kein Replay: Session-Fehler → OOB-Halt + `Gap`; `Reconnected` (= StreamReady) wird **erst**
//!   emittiert, wenn alle Subscription-ACKs der neuen Session eingegangen sind ([FIX-36]); S2 muss
//!   per Snapshot reconciliieren, bevor RunState wieder Ready wird.
//! - Overflow im Sink ist kein stiller Drop: Adapter emittiert `Gap` mit
//!   Slot-Provenienz und der Empfänger hält fail-closed an.
//! - Kein Provider im Code; nur `ws_url`.
//!
//! Status (F5): loopback-tested against fake WebSocket servers — ACK/StreamReady barrier,
//! ping/pong, 429/Retry-After, reconnect and recovery budget, stop in every session phase,
//! queue overflow (`ws_rpc_recovery_tests.rs`, `ws_rpc_stop_tests.rs`, unit tests here).
//! VERIFY, NOT_RUN: real provider (Helius) WSS behaviour and the programSubscribe credit cost
//! (Ledger S1-05).

use super::{
    AccountUpdate, IngressError, IngressEvent, IngressHalt, IngressSource, SlotStatus, SlotUpdate,
    SlotWatch, SourceKind, Subscription,
};
use base64::Engine;
use darkbot_common::channel::{ChannelError, Producer};
use darkbot_common::lifecycle::Shutdown;
use darkbot_common::timing::Clock;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

/// Continuous application slot progress after which a recovered stream counts as healthy again
/// (the outage budget resets only then; one frame or a flapping connection never resets it).
/// D01: the typed parameter `wss_healthy_reset_ms`.
pub fn healthy_reset() -> Duration {
    Duration::from_millis(crate::runtime_config::active().limits.wss_healthy_reset_ms)
}

#[derive(Clone, Debug)]
pub struct WsRpcConfig {
    /// z. B. wss://<provider>/?api-key=... — nur Konfiguration, nie im Code.
    pub ws_url: String,
    pub subscription: Subscription,
    /// Commitment für Account-/Program-Subscriptions.
    pub commitment: &'static str,
    pub reconnect_initial: Duration,
    pub reconnect_max: Duration,
    /// Without usable application frames / slot progress the stream is dead; ACK/Ping/Pong do not extend it.
    pub idle_timeout: Duration,
    /// [C0-MO] Optional transport heartbeat: send a WebSocket Ping at this interval. `None`
    /// (default, Shadow/Live) keeps the previous behaviour exactly; the market mode enables it.
    pub heartbeat_interval: Option<Duration>,
    /// Fixed outage budget, independent of capture duration (C0-E1 opt-in).
    pub recovery_budget: Option<Duration>,
    pub recovery_attempts: u32,
    /// N8: ACK allowance per subscription on top of the idle timeout.
    pub ack_per_subscription: Duration,
    /// Z19: interval of the stream health snapshot (5 s, ledger "stream health"), emitted with or
    /// without frames.
    pub health_interval: Duration,
}

/// N8: how long all subscription ACKs may take: the idle timeout plus a configured allowance per
/// subscription, never beyond what is left of the hard recovery budget.
pub(crate) fn ack_window(
    idle_timeout: Duration,
    subscriptions: usize,
    per_subscription: Duration,
    recovery_left: Option<Duration>,
) -> Duration {
    let window = idle_timeout.saturating_add(
        per_subscription.saturating_mul(u32::try_from(subscriptions).unwrap_or(u32::MAX)),
    );
    recovery_left.map_or(window, |left| window.min(left))
}

/// Wait before the next frame read. `None` heartbeat: exactly the idle timeout (previous
/// behaviour). With heartbeat: the shorter of the remaining idle budget (measured from the last
/// APPLICATION frame) and the time to the next heartbeat.
pub(crate) fn heartbeat_wait(
    idle_timeout: Duration,
    since_last_app_frame: Duration,
    until_heartbeat: Option<Duration>,
) -> Duration {
    match until_heartbeat {
        None => idle_timeout.saturating_sub(since_last_app_frame),
        Some(until) => idle_timeout.saturating_sub(since_last_app_frame).min(until),
    }
}

/// Application data (JSON-RPC text/binary). Ping/Pong/Close/raw frames are transport only.
#[cfg(test)]
pub(crate) fn is_application_frame(frame: &Message) -> bool {
    matches!(frame, Message::Text(_) | Message::Binary(_))
}

impl WsRpcConfig {
    /// The WSS bounds of the operational config (its registry defaults when none is installed,
    /// DEC-05: no second set of literals); the recovery budget only with an installed config.
    pub fn new(ws_url: String, subscription: Subscription) -> Self {
        let p = &crate::runtime_config::active().limits;
        WsRpcConfig {
            ws_url,
            subscription,
            commitment: crate::state::STATE_COMMITMENT,
            reconnect_initial: Duration::from_millis(p.wss_reconnect_ms),
            reconnect_max: Duration::from_millis(p.wss_reconnect_max_ms),
            idle_timeout: Duration::from_millis(p.wss_idle_timeout_ms),
            heartbeat_interval: None,
            recovery_budget: crate::runtime_config::configured()
                .then(|| Duration::from_millis(p.wss_recovery_ms)),
            recovery_attempts: p.wss_attempts as u32,
            ack_per_subscription: Duration::from_millis(p.wss_ack_ms_per_subscription),
            health_interval: Duration::from_secs(5),
        }
    }
}

struct SessionAttempt {
    attempt: u32,
    deadline: Option<tokio::time::Instant>,
    healthy: bool,
    retry_after: Duration,
}

pub struct WsRpcControlSource {
    cfg: WsRpcConfig,
    clock: Clock,
    provider_budget: std::sync::Arc<crate::provider::Budget>,
}

impl WsRpcControlSource {
    pub fn new(cfg: WsRpcConfig, clock: Clock) -> Self {
        WsRpcControlSource {
            cfg,
            clock,
            provider_budget: crate::provider::shared(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubKind {
    // [FIX-52] matches parse_notification visibility
    Program,
    Account,
    Slot,
    Root,
    Transaction,
    Logs,
}

/// Baut die Subscribe-Requests. Request-ID → Art, damit die Bestätigungen
/// (`result: <subscription id>`) zugeordnet werden können.
fn build_subscribe_requests(sub: &Subscription, commitment: &str) -> Vec<(u64, SubKind, Value)> {
    let mut out = Vec::new();
    let mut id = 1u64;
    for p in &sub.programs {
        out.push((
            id,
            SubKind::Program,
            json!({"jsonrpc":"2.0","id":id,"method":"programSubscribe",
                   "params":[p.to_string(), {"encoding":"base64","commitment":commitment}]}),
        ));
        id += 1;
    }
    for (p, filters) in &sub.program_filters {
        let filters: Vec<Value> = filters.iter().map(super::ProgramFilter::wire).collect();
        out.push((
            id,
            SubKind::Program,
            json!({"jsonrpc":"2.0","id":id,"method":"programSubscribe",
                   "params":[p.to_string(), {"encoding":"base64","commitment":commitment,"filters":filters}]}),
        ));
        id += 1;
    }
    for a in &sub.accounts {
        out.push((
            id,
            SubKind::Account,
            json!({"jsonrpc":"2.0","id":id,"method":"accountSubscribe",
                   "params":[a.to_string(), {"encoding":"base64","commitment":commitment}]}),
        ));
        id += 1;
    }
    if sub.slots {
        out.push((
            id,
            SubKind::Slot,
            json!({"jsonrpc":"2.0","id":id,"method":"slotSubscribe"}),
        ));
        id += 1;
    }
    if sub.roots {
        out.push((
            id,
            SubKind::Root,
            json!({"jsonrpc":"2.0","id":id,"method":"rootSubscribe"}),
        ));
        id += 1;
    }
    // VERIFY: `transactionSubscribe` ist eine Provider-Erweiterung (Helius Enhanced WSS, nicht Standard-RPC).
    // Für Stufe 1 optional; Standardweg für Shadow bleibt getBlock/getTransaction in P6-LT.
    for p in &sub.transactions_mentioning {
        out.push((
            id,
            SubKind::Transaction,
            json!({"jsonrpc":"2.0","id":id,"method":"transactionSubscribe",
                   // [C0-MO] No `failed`/`vote` key: on Helius/LaserStream a set `failed` flag selects
                   // one status class only; omitted means successful AND failed are delivered.
                   "params":[{"accountInclude":[p.to_string()]},
                             {"commitment":commitment, "encoding":"json", "transactionDetails":"full", "maxSupportedTransactionVersion": 1}]}),
        ));
        id += 1;
    }
    // [C0-MO] Standard Solana method (all provider plans). `mentions` accepts exactly one key.
    for p in &sub.logs_mentioning {
        out.push((
            id,
            SubKind::Logs,
            json!({"jsonrpc":"2.0","id":id,"method":"logsSubscribe",
                   "params":[{"mentions":[p.to_string()]}, {"commitment":commitment}]}),
        ));
        id += 1;
    }
    out
}

fn parse_account_value(
    v: &Value,
    pubkey: Pubkey,
    slot: u64,
    t_rx_ns: u64,
) -> Result<AccountUpdate, IngressError> {
    let acc = v;
    let lamports = acc
        .get("lamports")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| IngressError::Protocol("account.lamports missing".into()))?;
    let owner_s = acc
        .get("owner")
        .and_then(|x| x.as_str())
        .ok_or_else(|| IngressError::Protocol("account.owner missing".into()))?;
    let owner =
        Pubkey::from_str(owner_s).map_err(|e| IngressError::Protocol(format!("owner: {}", e)))?;
    let data_b64 = acc
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|x| x.as_str())
        .ok_or_else(|| IngressError::Protocol("account.data[0] missing".into()))?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| IngressError::Protocol(format!("base64: {}", e)))?;
    Ok(AccountUpdate {
        pubkey,
        owner,
        lamports,
        data,
        slot,
        write_version: None,
        source: SourceKind::WsRpcControl,
        t_rx_ns,
    })
}

/// Wandelt eine Notification in Events. Pure Funktion → offline testbar.
pub(crate) fn parse_notification(
    msg: &Value,
    sub_ids: &HashMap<u64, (SubKind, Option<Pubkey>)>,
    t_rx_ns: u64,
) -> Result<Vec<IngressEvent>, IngressError> {
    let method = match msg.get("method").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return Ok(Vec::new()), // Subscribe-Bestätigung o. ä.
    };
    let params = msg
        .get("params")
        .ok_or_else(|| IngressError::Protocol("params missing".into()))?;
    let sub_id = params.get("subscription").and_then(|s| s.as_u64());
    let result = params
        .get("result")
        .ok_or_else(|| IngressError::Protocol("result missing".into()))?;

    match method {
        "programNotification" => {
            let slot = result
                .pointer("/context/slot")
                .and_then(|s| s.as_u64())
                .ok_or_else(|| IngressError::Protocol("context.slot missing".into()))?;
            let pk_s = result
                .pointer("/value/pubkey")
                .and_then(|x| x.as_str())
                .ok_or_else(|| IngressError::Protocol("value.pubkey missing".into()))?;
            let pk = Pubkey::from_str(pk_s)
                .map_err(|e| IngressError::Protocol(format!("pubkey: {}", e)))?;
            let acc = result
                .pointer("/value/account")
                .ok_or_else(|| IngressError::Protocol("value.account missing".into()))?;
            Ok(vec![IngressEvent::Account(parse_account_value(
                acc, pk, slot, t_rx_ns,
            )?)])
        }
        "accountNotification" => {
            let slot = result
                .pointer("/context/slot")
                .and_then(|s| s.as_u64())
                .ok_or_else(|| IngressError::Protocol("context.slot missing".into()))?;
            let pk = sub_id
                .and_then(|id| sub_ids.get(&id))
                .and_then(|(_, pk)| *pk)
                .ok_or_else(|| {
                    IngressError::Protocol("accountNotification for unknown subscription".into())
                })?;
            let acc = result
                .get("value")
                .ok_or_else(|| IngressError::Protocol("value missing".into()))?;
            Ok(vec![IngressEvent::Account(parse_account_value(
                acc, pk, slot, t_rx_ns,
            )?)])
        }
        "slotNotification" => {
            let slot = result
                .get("slot")
                .and_then(|s| s.as_u64())
                .ok_or_else(|| IngressError::Protocol("slot".into()))?;
            let parent = result.get("parent").and_then(|s| s.as_u64());
            Ok(vec![IngressEvent::Slot(SlotUpdate {
                slot,
                parent,
                status: SlotStatus::Processed,
                source: SourceKind::WsRpcControl,
                t_rx_ns,
            })])
        }
        "transactionNotification" => {
            // {result:{signature, slot, transaction:{transaction, meta, version}}}; the identity
            // is required, the payload is decoded once into `ChainTx` (JFI-02).
            let signature = result
                .get("signature")
                .and_then(|s| s.as_str())
                .ok_or_else(|| IngressError::Protocol("signature missing".into()))?
                .to_string();
            let slot = result
                .get("slot")
                .and_then(|s| s.as_u64())
                .ok_or_else(|| IngressError::Protocol("slot missing".into()))?;
            Ok(vec![IngressEvent::Transaction(Box::new(
                super::TransactionObserved {
                    signature,
                    slot,
                    tx_index_in_slot: None,
                    source: SourceKind::WsRpcControl,
                    t_rx_ns,
                    tx: crate::chain::decode_notification(result),
                },
            ))])
        }
        "logsNotification" => {
            // [C0-MO] {result:{context:{slot}, value:{signature, err, logs}}}
            let logs = crate::chain::decode_logs_notification(result)
                .map_err(|e| IngressError::Protocol(e.0.into()))?;
            Ok(vec![IngressEvent::Logs(super::LogsObserved {
                source: SourceKind::WsRpcControl,
                t_rx_ns,
                logs,
            })])
        }
        "rootNotification" => {
            let slot = result
                .as_u64()
                .ok_or_else(|| IngressError::Protocol("root".into()))?;
            Ok(vec![IngressEvent::Root {
                slot,
                source: SourceKind::WsRpcControl,
                t_rx_ns,
            }])
        }
        _ => Ok(Vec::new()),
    }
}

impl IngressSource for WsRpcControlSource {
    fn kind(&self) -> SourceKind {
        SourceKind::WsRpcControl
    }

    fn run(
        self,
        sink: Producer<IngressEvent>,
        shutdown: Shutdown,
        halt: IngressHalt,
        slots: SlotWatch,
    ) -> Result<(), IngressError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| IngressError::Config(format!("tokio: {}", e)))?;
        rt.block_on(self.run_async(sink, shutdown, halt, slots))
    }
}

impl WsRpcControlSource {
    async fn run_async(
        self,
        sink: Producer<IngressEvent>,
        shutdown: Shutdown,
        halt: IngressHalt,
        slots: SlotWatch,
    ) -> Result<(), IngressError> {
        let mut attempt: u32 = 0;
        let mut failures: u32 = 0;
        let mut last_slot: Option<u64> = None;
        let mut outage = tokio::time::Instant::now();
        while !shutdown.is_requested() {
            attempt = attempt.saturating_add(1);
            self.transport(
                &sink,
                if attempt == 1 {
                    "STARTING"
                } else {
                    "RECOVERING"
                },
                "CONNECT",
                attempt,
                last_slot,
            );
            let deadline = self.cfg.recovery_budget.map(|b| outage + b);
            let mut recovery = SessionAttempt {
                attempt,
                deadline,
                healthy: false,
                retry_after: Duration::ZERO,
            };
            let result = tokio::select! {
                r = self.session(&sink, &shutdown, &halt, &slots, &mut last_slot, &mut recovery) => r,
                _ = wait_shutdown(&shutdown) => Ok(()),
            };
            match result {
                Ok(()) => {
                    self.transport(&sink, "FINISHED", "SHUTDOWN", attempt, last_slot);
                    return Ok(());
                }
                Err(e) => {
                    if shutdown.is_requested() {
                        return Ok(());
                    }
                    if recovery.healthy {
                        failures = 0;
                        outage = tokio::time::Instant::now();
                    }
                    failures = failures.saturating_add(1);
                    halt.raise();
                    let t = self.clock.now_ns();
                    // L3: a control/health event lost to a full queue is counted as dropped.
                    if let Err(ChannelError::Full(_)) = sink.try_push(IngressEvent::Gap {
                        source: SourceKind::WsRpcControl,
                        from_slot: last_slot,
                        to_slot: None,
                        t_rx_ns: t,
                    }) {
                        sink.give_up();
                    }
                    let code = if !recovery.retry_after.is_zero() {
                        "WSS_HTTP_429"
                    } else {
                        match &e {
                            IngressError::Connect(s) if s == "WSS_SLOT_PROGRESS_TIMEOUT" => {
                                "WSS_SLOT_PROGRESS_TIMEOUT"
                            }
                            IngressError::Connect(s) if s == "idle timeout" => {
                                "WSS_APPLICATION_IDLE"
                            }
                            IngressError::Connect(s) if s == "WSS_RECOVERY_BUDGET" => {
                                "WSS_RECOVERY_BUDGET"
                            }
                            IngressError::Protocol(_) => "WSS_PROTOCOL_ERROR",
                            _ => "WSS_SESSION_FAILED",
                        }
                    };
                    self.transport(&sink, "DEGRADED", code, attempt, last_slot);
                    // Error text may contain provider response bodies/URLs: emit safe classes only.
                    darkbot_common::console::line(format!(
                        "[ws_rpc] state=DEGRADED attempt={attempt} code={code}"
                    ));
                    let delay = reconnect_delay(
                        self.cfg.reconnect_initial,
                        self.cfg.reconnect_max,
                        failures,
                        t,
                    )
                    .max(recovery.retry_after);
                    if self.cfg.recovery_budget.is_some_and(|b| {
                        failures >= self.cfg.recovery_attempts
                            || outage.elapsed().saturating_add(delay) >= b
                    }) {
                        self.transport(
                            &sink,
                            "INCOMPLETE_ABORT",
                            "WSS_RECOVERY_EXHAUSTED",
                            attempt,
                            last_slot,
                        );
                        // L3: a control/health event lost to a full queue is counted as dropped.
                        if let Err(ChannelError::Full(_)) = sink.try_push(IngressEvent::Fatal {
                            source: SourceKind::WsRpcControl,
                            detail: "WSS_RECOVERY_EXHAUSTED".into(),
                            t_rx_ns: self.clock.now_ns(),
                        }) {
                            sink.give_up();
                        }
                        return Err(IngressError::Connect("WSS_RECOVERY_EXHAUSTED".into()));
                    }
                    let _ = e; // no secret-bearing Display
                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {},
                        _ = wait_shutdown(&shutdown) => return Ok(()),
                    }
                }
            }
        }
        Ok(())
    }

    fn ready(
        &self,
        sink: &Producer<IngressEvent>,
        halt: &IngressHalt,
        attempt: u32,
        t_rx_ns: u64,
    ) -> Result<(), IngressError> {
        sink.try_push(IngressEvent::Reconnected {
            source: SourceKind::WsRpcControl,
            attempt,
            t_rx_ns,
        })
        .map_err(|_| {
            halt.raise();
            IngressError::Protocol("WSS_READY_NOT_DELIVERED".into())
        })
    }

    fn transport(
        &self,
        sink: &Producer<IngressEvent>,
        state: &'static str,
        code: &'static str,
        attempt: u32,
        last_slot: Option<u64>,
    ) {
        // L3: a control/health event lost to a full queue is counted as dropped.
        if let Err(ChannelError::Full(_)) = sink.try_push(IngressEvent::Transport {
            state,
            code,
            attempt,
            last_slot,
            t_rx_ns: self.clock.now_ns(),
        }) {
            sink.give_up();
        }
    }

    async fn session(
        &self,
        sink: &Producer<IngressEvent>,
        shutdown: &Shutdown,
        halt: &IngressHalt,
        slots: &SlotWatch,
        last_slot: &mut Option<u64>,
        recovery: &mut SessionAttempt,
    ) -> Result<(), IngressError> {
        let runtime = crate::runtime_config::active();
        let requests = build_subscribe_requests(&self.cfg.subscription, self.cfg.commitment);
        let _socket_permit = self
            .provider_budget
            .acquire_socket(requests.len() as u64)
            .map_err(|e| IngressError::Config(e.into()))?;
        let attempt = recovery.attempt;
        let deadline = recovery.deadline;
        let healthy = &mut recovery.healthy;
        let retry_after = &mut recovery.retry_after;
        darkbot_common::console::line(format!("[ws_rpc] session attempt={} connecting", attempt));
        let connect_budget = deadline
            .map(|d| d.saturating_duration_since(tokio::time::Instant::now()))
            .unwrap_or(self.cfg.idle_timeout)
            .min(self.cfg.idle_timeout)
            .min(Duration::from_millis(runtime.limits.wss_connect_timeout_ms));
        let _permit = if self.cfg.recovery_budget.is_some() {
            let deadline = std::time::Instant::now()
                + deadline
                    .map(|d| d.saturating_duration_since(tokio::time::Instant::now()))
                    .unwrap_or(connect_budget);
            let budget = self.provider_budget.clone();
            let cancellation = shutdown.clone();
            Some(
                tokio::task::spawn_blocking(move || {
                    budget.acquire_cancellable(
                        crate::provider::Workload::Websocket,
                        deadline,
                        Some(&cancellation),
                    )
                })
                .await
                .map_err(|_| IngressError::Connect("WSS_ADMISSION_JOIN".into()))?
                .map_err(|_| IngressError::Connect("WSS_ADMISSION_BUDGET".into()))?,
            )
        } else {
            None
        };
        let connect_budget = deadline
            .map(|d| d.saturating_duration_since(tokio::time::Instant::now()))
            .unwrap_or(connect_budget)
            .min(connect_budget);
        let endpoint =
            if runtime.fallback && runtime.wss_endpoints.first() == Some(&self.cfg.ws_url) {
                runtime
                    .wss_endpoints
                    .get(
                        (attempt.saturating_sub(1) as usize)
                            .min(runtime.wss_endpoints.len().saturating_sub(1)),
                    )
                    .unwrap_or(&self.cfg.ws_url)
            } else {
                &self.cfg.ws_url
            };
        let mut wire = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
        let message_bytes = (runtime.limits.wss_message_mib * 1024 * 1024) as usize;
        wire.max_message_size = Some(message_bytes);
        wire.max_frame_size = wire.max_message_size;
        // V2-14 P3-04: unsent writes (the Pongs to a peer that stops reading) stay bounded by one
        // message of `wss_message_mib` on top of the write buffer; beyond it the newest Pong
        // replaces the queued one (tungstenite) and the reader goes on.
        wire.max_write_buffer_size = wire.write_buffer_size.saturating_add(message_bytes);
        let (ws, _) = tokio::time::timeout(
            connect_budget,
            tokio_tungstenite::connect_async_with_config(endpoint, Some(wire), false),
        )
        .await
        .map_err(|_| IngressError::Connect("WSS_CONNECT_TIMEOUT".into()))?
        .map_err(|e| {
            if let tokio_tungstenite::tungstenite::Error::Http(r) = &e {
                if r.status().as_u16() == 429 {
                    let header = crate::provider::retry_after_header(
                        r.headers().get("retry-after").and_then(|h| h.to_str().ok()),
                        std::time::SystemTime::now(),
                    );
                    *retry_after = header.delay().max(Duration::from_millis(1));
                    if self.cfg.recovery_budget.is_some() {
                        self.provider_budget.rate_limited_header(header);
                    }
                }
            }
            IngressError::Connect("WSS_CONNECT_FAILED".into())
        })?;
        drop(_permit);
        let (mut tx, mut rx) = ws.split();

        // Subscribe-Requests senden; Bestätigungen (id → subscription) einsammeln.
        let request_total = requests.len();
        darkbot_common::console::line(format!(
            "[ws_rpc] session attempt={} connected; subscriptions={} (programs={} filtered_programs={} accounts={} slots={} roots={} tx={})",
            attempt,
            request_total,
            self.cfg.subscription.programs.len(),
            self.cfg.subscription.program_filters.len(),
            self.cfg.subscription.accounts.len(),
            self.cfg.subscription.slots as u8,
            self.cfg.subscription.roots as u8,
            self.cfg.subscription.transactions_mentioning.len()
        ));
        let mut pending: HashMap<u64, (SubKind, Option<Pubkey>)> = HashMap::new();
        let mut acc_iter = self.cfg.subscription.accounts.iter();
        for (id, kind, req) in &requests {
            let pk = if *kind == SubKind::Account {
                acc_iter.next().copied()
            } else {
                None
            };
            pending.insert(*id, (*kind, pk));
            let send_budget = deadline
                .map(|d| d.saturating_duration_since(tokio::time::Instant::now()))
                .unwrap_or(self.cfg.idle_timeout)
                .min(self.cfg.idle_timeout);
            if send_budget.is_zero() {
                return Err(IngressError::Connect("WSS_RECOVERY_BUDGET".into()));
            }
            tokio::time::timeout(send_budget, tx.send(Message::Text(req.to_string())))
                .await
                .map_err(|_| IngressError::Connect("WSS_SUBSCRIBE_TIMEOUT".into()))?
                .map_err(|e| IngressError::Connect(e.to_string()))?;
        }
        let mut sub_ids: HashMap<u64, (SubKind, Option<Pubkey>)> = HashMap::new();
        let ack_started = tokio::time::Instant::now();
        let ack_deadline = ack_started
            + ack_window(
                self.cfg.idle_timeout,
                request_total,
                self.cfg.ack_per_subscription,
                deadline.map(|d| d.saturating_duration_since(ack_started)),
            );
        let mut stream_ready = false; // [FIX-36]
        let mut frames: u64 = 0;
        let mut last_frame_ns: Option<u64> = None;
        let mut last_health_ns: u64 = self.clock.now_ns();
        let heartbeat = self.cfg.heartbeat_interval;
        let mut last_app_frame_at = tokio::time::Instant::now();
        let mut last_progress_at = last_app_frame_at;
        let mut progress_slot = *last_slot;
        let mut first_progress_at = None;
        let mut recovered = false;
        let mut next_heartbeat_at = heartbeat.map(|h| tokio::time::Instant::now() + h);

        loop {
            if shutdown.is_requested() {
                return Ok(());
            }
            let now = tokio::time::Instant::now();
            if !stream_ready && now >= ack_deadline {
                return Err(IngressError::Connect("WSS_ACK_TIMEOUT".into()));
            }
            if !*healthy && deadline.is_some_and(|d| now >= d) {
                return Err(IngressError::Connect("WSS_RECOVERY_BUDGET".into()));
            }
            if stream_ready
                && self.cfg.subscription.slots
                && last_progress_at.elapsed() >= self.cfg.idle_timeout
            {
                return Err(IngressError::Connect("WSS_SLOT_PROGRESS_TIMEOUT".into()));
            }
            // [C0-MO] Transport heartbeat (only when configured). A failed send is a connection
            // loss and takes the existing reconnect/Gap path.
            if let (Some(h), Some(at)) = (heartbeat, next_heartbeat_at) {
                let now = tokio::time::Instant::now();
                if now >= at {
                    tokio::time::timeout(
                        Duration::from_secs(5),
                        tx.send(Message::Ping(Default::default())),
                    )
                    .await
                    .map_err(|_| IngressError::Connect("WSS_HEARTBEAT_TIMEOUT".into()))?
                    .map_err(|e| IngressError::Connect(format!("heartbeat: {e}")))?;
                    next_heartbeat_at = Some(now + h);
                }
            }
            let now = tokio::time::Instant::now();
            let wait = heartbeat_wait(
                self.cfg.idle_timeout,
                now.saturating_duration_since(last_app_frame_at),
                next_heartbeat_at.map(|at| at.saturating_duration_since(now)),
            );
            let wait = wait.min(Duration::from_millis(250));
            let frame = tokio::time::timeout(wait, rx.next()).await;
            // V2-14 P3-02, measurement contract: T_rx is the userland dequeue of the frame from
            // the WebSocket stream (after TLS and framing, before the parse) — not the socket
            // arrival. In a burst, later frames wait in the stream's buffers and their T_rx
            // includes that wait; a `T_rx_first` (a reader task stamping the arrival) needs its
            // own measurement contract first and is not built.
            let t_rx_ns = self.clock.now_ns();
            // Health-Event alle 5 s (Ledger: stream health), unabhängig vom Frame-Inhalt. Z19:
            // also while the stream is silent (no frame before the wake-up), so the frame age
            // keeps rising in the event stream until the idle timeout ends the session.
            let health_interval_ns =
                self.cfg.health_interval.as_nanos().min(u64::MAX as u128) as u64;
            if t_rx_ns.saturating_sub(last_health_ns) >= health_interval_ns {
                last_health_ns = t_rx_ns;
                // L3: a control/health event lost to a full queue is counted as dropped.
                if let Err(ChannelError::Full(_)) = sink.try_push(IngressEvent::Health(
                    session_health(attempt, frames, last_frame_ns, *last_slot, t_rx_ns),
                )) {
                    sink.give_up();
                }
            }
            if frame.is_err() && last_app_frame_at.elapsed() < self.cfg.idle_timeout {
                continue; // woke up for the heartbeat, not because the stream went idle
            }
            let frame = match frame {
                Err(_) => return Err(IngressError::Connect("idle timeout".into())),
                Ok(None) => return Err(IngressError::Connect("stream closed".into())),
                Ok(Some(Err(e))) => return Err(IngressError::Connect(e.to_string())),
                Ok(Some(Ok(f))) => f,
            };

            self.provider_budget
                .account_bytes(frame.len())
                .map_err(|e| IngressError::Protocol(e.into()))?;
            // [C0-MO r5] Only application frames prove the subscription stream is alive;
            // transport Ping/Pong (incl. replies to our heartbeat) never reset the idle budget.

            let text = match frame {
                // V2-14 P3-01: the frame's buffer becomes the text (no copy).
                Message::Text(t) => t,
                Message::Binary(b) => {
                    String::from_utf8(b).map_err(|_| IngressError::Protocol("WSS_UTF8".into()))?
                }
                // V2-14 P3-04: tungstenite queues the Pong itself and the next read poll flushes
                // it (loopback proof `p3_04_a_ping_is_answered_by_the_read_poll`); a blocked
                // write never stops this reader.
                Message::Ping(_) => continue,
                Message::Close(_) => return Err(IngressError::Connect("close frame".into())),
                _ => continue,
            };
            let v: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    // Fail closed: ein nicht lesbarer Frame kann ein State-Update gewesen sein.
                    return Err(IngressError::Protocol(format!("unparseable frame: {}", e)));
                }
            };

            // Subscribe-Bestätigung: {"id":n,"result":<subscription>}
            if let (Some(id), Some(res)) = (v.get("id").and_then(|i| i.as_u64()), v.get("result")) {
                if let Some(sub) = res.as_u64() {
                    if let Some(entry) = pending.remove(&id) {
                        sub_ids.insert(sub, entry);
                    }
                }
                let acked = request_total.saturating_sub(pending.len());
                if acked == request_total || acked == 1 || acked.is_multiple_of(100) {
                    darkbot_common::console::line(format!(
                        "[ws_rpc] session attempt={} subscription ACKs={}/{}",
                        attempt, acked, request_total
                    ));
                }
                // [FIX-36] StreamReady-Barriere: alle Subscriptions bestätigt → genau ein `Reconnected`.
                if !stream_ready && pending.is_empty() {
                    stream_ready = true;
                    last_progress_at = tokio::time::Instant::now();
                    last_app_frame_at = last_progress_at;
                    self.transport(sink, "READY", "SUBSCRIPTIONS_ACKED", attempt, *last_slot);
                    darkbot_common::console::line(format!(
                        "[ws_rpc] session attempt={} StreamReady; ACK phase {} ms for {} subscriptions",
                        attempt,
                        ack_started.elapsed().as_millis(),
                        request_total
                    ));
                    if self.cfg.recovery_budget.is_none() {
                        self.ready(sink, halt, attempt, t_rx_ns)?;
                    }
                }
                continue;
            }
            if let Some(err) = v.get("error") {
                return Err(IngressError::Protocol(format!("rpc error: {}", err)));
            }

            if v.get("method").is_some()
                && !v
                    .pointer("/params/subscription")
                    .and_then(Value::as_u64)
                    .is_some_and(|id| sub_ids.contains_key(&id))
            {
                return Err(IngressError::Protocol("WSS_UNKNOWN_SUBSCRIPTION".into()));
            }
            let events = parse_notification(&v, &sub_ids, t_rx_ns)?;
            if stream_ready && !events.is_empty() {
                last_app_frame_at = tokio::time::Instant::now();
                last_frame_ns = Some(t_rx_ns);
                frames += 1;
            }
            for ev in events {
                let progress = match &ev {
                    IngressEvent::Slot(s) => Some(s.slot),
                    IngressEvent::Account(a) if !self.cfg.subscription.slots => Some(a.slot),
                    _ => None,
                };
                if stream_ready && progress.is_some_and(|s| progress_slot.is_none_or(|p| s > p)) {
                    progress_slot = progress;
                    last_progress_at = tokio::time::Instant::now();
                    let since = first_progress_at.get_or_insert(last_progress_at);
                    if !recovered {
                        recovered = true;
                        if self.cfg.recovery_budget.is_some() {
                            self.ready(sink, halt, attempt, t_rx_ns)?;
                        }
                        self.transport(
                            sink,
                            if attempt == 1 {
                                "CAPTURING"
                            } else {
                                "RECOVERED"
                            },
                            "APPLICATION_SLOT_PROGRESS",
                            attempt,
                            progress,
                        );
                    }
                    // One frame / a flapping connection cannot reset the outage budget.
                    if last_progress_at.duration_since(*since) >= healthy_reset() {
                        *healthy = true;
                    }
                }
                if !deliver(sink, halt, slots, last_slot, ev, t_rx_ns)? {
                    return Ok(());
                }
            }
        }
    }
}

/// Hands one event to the owner queue. F4: the wire-observed slot (`last_slot`, the shared
/// `SlotWatch` [FIX-40]) advances only for events actually delivered, so a Gap after a full
/// queue starts at the last delivered slot, never at a dropped frame. `Ok(false)`: the owner is
/// gone (session ends normally).
fn deliver(
    sink: &Producer<IngressEvent>,
    halt: &IngressHalt,
    slots: &SlotWatch,
    last_slot: &mut Option<u64>,
    ev: IngressEvent,
    t_rx_ns: u64,
) -> Result<bool, IngressError> {
    let slot = match &ev {
        IngressEvent::Slot(s) => Some(s.slot),
        IngressEvent::Account(a) => Some(a.slot),
        _ => None,
    };
    match sink.try_push(ev) {
        Ok(()) => {
            if let Some(slot) = slot {
                *last_slot = Some(last_slot.map_or(slot, |p| p.max(slot)));
                slots.observe(slot);
            }
            Ok(true)
        }
        Err(ChannelError::Full(_)) => {
            // Kein stiller Drop: Out-of-Band-Halt (garantiert sichtbar) + Gap-Versuch.
            halt.raise();
            sink.give_up();
            if let Err(ChannelError::Full(_)) = sink.try_push(IngressEvent::Gap {
                source: SourceKind::WsRpcControl,
                from_slot: *last_slot,
                to_slot: None,
                t_rx_ns,
            }) {
                sink.give_up();
            }
            Err(IngressError::Protocol("sink overflow".into()))
        }
        Err(ChannelError::Disconnected(_)) => Ok(false),
    }
}

async fn wait_shutdown(shutdown: &Shutdown) {
    while !shutdown.is_requested() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
/// A01: the periodic health of the session of connect attempt `attempt` (1 = the first): its
/// reconnects are the connect attempts before it (failed ones included), never a placeholder 0.
fn session_health(
    attempt: u32,
    frames: u64,
    last_frame_ns: Option<u64>,
    last_slot: Option<u64>,
    t_rx_ns: u64,
) -> super::SourceHealth {
    super::SourceHealth {
        source: SourceKind::WsRpcControl,
        connected: true,
        last_frame_age_ns: last_frame_ns.map(|l| t_rx_ns.saturating_sub(l)),
        frames,
        reconnects: attempt.saturating_sub(1),
        last_slot,
        t_rx_ns,
    }
}

pub(crate) fn reconnect_delay(
    initial: Duration,
    maximum: Duration,
    failures: u32,
    seed: u64,
) -> Duration {
    let base = initial
        .saturating_mul(1u32 << failures.saturating_sub(1).min(16))
        .min(maximum);
    // Per-attempt monotonic jitter avoids synchronized retries; it never shortens Retry-After.
    base.saturating_add(Duration::from_millis(seed % 251))
        .min(maximum)
}

/// One bounded read-only liveness observation through the production ingress, no new feed.
/// All subscriptions must ACK; two increasing slots and source application data are required.
pub fn audit_liveness(
    url: String,
    program: Pubkey,
    transactions: bool,
    budget: Duration,
) -> Result<(u64, u64), &'static str> {
    use super::{IngressEvent, IngressHalt, IngressSource, SlotWatch};
    let sub = Subscription {
        slots: true,
        logs_mentioning: if transactions { vec![] } else { vec![program] },
        transactions_mentioning: if transactions { vec![program] } else { vec![] },
        ..Default::default()
    };
    let mut cfg = WsRpcConfig::new(url, sub);
    cfg.idle_timeout = budget;
    cfg.recovery_budget = Some(budget);
    cfg.recovery_attempts = 1;
    cfg.heartbeat_interval = Some(Duration::from_secs(1));
    let (tx, rx) = darkbot_common::channel::bounded("boot-wss", 256);
    let stop = darkbot_common::lifecycle::Shutdown::new();
    let worker_stop = stop.clone();
    let halt = IngressHalt::new();
    let worker_halt = halt.clone();
    let source = WsRpcControlSource::new(cfg, Clock::new());
    let thread =
        std::thread::spawn(move || source.run(tx, worker_stop, worker_halt, SlotWatch::new()));
    let end = std::time::Instant::now() + budget;
    let (mut ack, mut first, mut last, mut data) = (false, None, None, false);
    while std::time::Instant::now() < end && !halt.is_raised() {
        while let Some(event) = rx.try_pop() {
            match event {
                IngressEvent::Transport { state: "READY", .. } => ack = true,
                IngressEvent::Slot(s) if ack => {
                    first.get_or_insert(s.slot);
                    last = Some(s.slot);
                }
                IngressEvent::Logs(_) | IngressEvent::Transaction(_) if ack => data = true,
                IngressEvent::Gap { .. } | IngressEvent::Fatal { .. } => halt.raise(),
                _ => {}
            }
        }
        if ack && data && first.zip(last).is_some_and(|(a, b)| b > a) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    stop.request();
    let ended = thread.join();
    if ended.is_err() || halt.is_raised() {
        return Err("BOOT_WSS_FAILED");
    }
    if ack && data {
        if let Some((a, b)) = first.zip(last).filter(|(a, b)| b > a) {
            return Ok((a, b));
        }
    }
    Err("BOOT_WSS_LIVENESS_UNPROVEN")
}

#[cfg(test)]
mod tests {
    /// A01: the health of a session counts the connect attempts before it (the second: one),
    /// never the former placeholder 0.
    #[test]
    fn a01_session_health_counts_the_reconnects_before_it() {
        assert_eq!(session_health(1, 0, None, None, 5).reconnects, 0);
        let h = session_health(2, 7, Some(3), Some(9), 5);
        assert_eq!(
            (h.reconnects, h.frames, h.last_frame_age_ns),
            (1, 7, Some(2))
        );
        assert_eq!(
            session_health(u32::MAX, 0, None, None, 0).reconnects,
            u32::MAX - 1
        );
    }

    use super::*;
    include!("ws_rpc_recovery_tests.rs");
    include!("ws_rpc_stop_tests.rs");

    /// F4: wire-observed slot progress follows delivery. An event refused by a full queue moves
    /// neither `last_slot` (the Gap origin) nor the shared SlotWatch; the halt is raised.
    #[test]
    fn slot_progress_follows_queue_delivery() {
        let slot = |n: u64| {
            IngressEvent::Slot(SlotUpdate {
                slot: n,
                parent: Some(n - 1),
                status: SlotStatus::Processed,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 0,
            })
        };
        let account = |n: u64| {
            IngressEvent::Account(AccountUpdate {
                pubkey: Pubkey::new_unique(),
                owner: Pubkey::new_unique(),
                lamports: 1,
                data: vec![],
                slot: n,
                write_version: None,
                source: SourceKind::WsRpcControl,
                t_rx_ns: 0,
            })
        };
        let (tx, rx) = darkbot_common::channel::bounded("f4", 2);
        let (halt, slots) = (IngressHalt::new(), SlotWatch::new());
        let mut last = None;
        assert!(deliver(&tx, &halt, &slots, &mut last, slot(10), 0).unwrap());
        assert!(deliver(&tx, &halt, &slots, &mut last, account(12), 0).unwrap());
        assert_eq!((last, slots.get()), (Some(12), 12));
        // Queue full: slot 20 is refused and never becomes the Gap origin or the slot clock.
        assert!(deliver(&tx, &halt, &slots, &mut last, slot(20), 0).is_err());
        assert!(halt.is_raised());
        assert_eq!((last, slots.get()), (Some(12), 12));
        assert!(matches!(rx.try_pop(), Some(IngressEvent::Slot(s)) if s.slot == 10));
        assert!(matches!(rx.try_pop(), Some(IngressEvent::Account(a)) if a.slot == 12));
        assert!(rx.try_pop().is_none(), "neither slot 20 nor a Gap fit");
        // A consumer that is gone ends the session without advancing the slot either.
        drop(rx);
        assert!(!deliver(&tx, &halt, &slots, &mut last, slot(30), 0).unwrap());
        assert_eq!((last, slots.get()), (Some(12), 12));
    }

    #[test]
    fn subscribe_requests_cover_all_kinds() {
        let p = Pubkey::new_unique();
        let a = Pubkey::new_unique();
        let sub = Subscription {
            programs: vec![p],
            program_filters: vec![],
            accounts: vec![a],
            slots: true,
            roots: true,
            transactions_mentioning: vec![],
            logs_mentioning: vec![],
        };
        let reqs = build_subscribe_requests(&sub, "confirmed");
        assert_eq!(reqs.len(), 4);
        assert_eq!(reqs[0].2["method"], "programSubscribe");
        assert_eq!(reqs[1].2["method"], "accountSubscribe");
        assert_eq!(reqs[2].2["method"], "slotSubscribe");
        assert_eq!(reqs[3].2["method"], "rootSubscribe");
        assert_eq!(reqs[0].2["params"][1]["encoding"], "base64");
    }

    #[test]
    fn c0mo_market_subscriptions_are_opt_in() {
        let p = Pubkey::new_unique();
        // Shadow-shaped subscription: no transaction/logs subscriptions (FIX-139 unchanged).
        let shadow = Subscription {
            slots: true,
            roots: true,
            ..Subscription::default()
        };
        assert!(build_subscribe_requests(&shadow, "confirmed")
            .iter()
            .all(|r| r.2["method"] != "logsSubscribe" && r.2["method"] != "transactionSubscribe"));
        let market = Subscription {
            logs_mentioning: vec![p],
            ..Subscription::default()
        };
        let reqs = build_subscribe_requests(&market, "confirmed");
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].1, SubKind::Logs);
        assert_eq!(reqs[0].2["method"], "logsSubscribe");
        assert_eq!(reqs[0].2["params"][0]["mentions"][0], p.to_string());
    }

    #[test]
    fn shadow_config_keeps_heartbeat_disabled_and_wait_unchanged() {
        let cfg = WsRpcConfig::new("wss://h".into(), Subscription::default());
        assert_eq!(cfg.heartbeat_interval, None);
        let idle = cfg.idle_timeout;
        // Without heartbeat the read wait is exactly the idle timeout, as before.
        assert_eq!(
            heartbeat_wait(idle, Duration::from_secs(59), None),
            Duration::from_secs(1)
        );
        // With heartbeat: the shorter of remaining idle budget and time to the next ping.
        assert_eq!(
            heartbeat_wait(idle, Duration::from_secs(10), Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
        assert_eq!(
            heartbeat_wait(idle, Duration::from_secs(58), Some(Duration::from_secs(30))),
            Duration::from_secs(2)
        );
        assert_eq!(
            heartbeat_wait(idle, Duration::from_secs(70), Some(Duration::from_secs(30))),
            Duration::ZERO
        );
    }

    #[test]
    fn transport_ping_pong_never_counts_as_stream_activity() {
        assert!(is_application_frame(&Message::Text("{}".to_string())));
        assert!(is_application_frame(&Message::Binary(vec![1u8])));
        assert!(!is_application_frame(&Message::Ping(Default::default())));
        assert!(!is_application_frame(&Message::Pong(Default::default())));
        assert!(!is_application_frame(&Message::Close(None)));
    }

    #[test]
    fn c0mo_transaction_subscription_requests_successful_and_failed() {
        let p = Pubkey::new_unique();
        let sub = Subscription {
            transactions_mentioning: vec![p],
            ..Subscription::default()
        };
        let reqs = build_subscribe_requests(&sub, "finalized");
        assert_eq!(reqs.len(), 1);
        let filter = &reqs[0].2["params"][0];
        assert_eq!(reqs[0].2["method"], "transactionSubscribe");
        assert_eq!(filter["accountInclude"][0], p.to_string());
        assert!(
            filter.get("failed").is_none(),
            "failed filter would select one status class only"
        );
        assert!(filter.get("vote").is_none());
        assert_eq!(reqs[0].2["params"][1]["commitment"], "finalized");
        assert_eq!(reqs[0].2["params"][1]["transactionDetails"], "full");
    }

    #[test]
    fn c0mo_logs_and_transaction_notifications_decode_at_the_wire() {
        use crate::chain::TxStatus;
        let ids = HashMap::new();
        let logs = json!({"jsonrpc":"2.0","method":"logsNotification","params":{"subscription":7,
            "result":{"context":{"slot":42},"value":{"signature":"sigL","err":null,"logs":["Program x invoke [1]"]}}}});
        match parse_notification(&logs, &ids, 5).unwrap().pop().unwrap() {
            IngressEvent::Logs(l) => {
                let l = l.logs;
                assert_eq!(
                    (l.signature.as_str(), l.slot, l.status),
                    ("sigL", 42, TxStatus::Success)
                );
                assert_eq!(l.logs.map(|v| v.len()), Some(1));
            }
            other => panic!("expected Logs, got {other:?}"),
        }
        let failed = json!({"jsonrpc":"2.0","method":"logsNotification","params":{"subscription":7,
            "result":{"context":{"slot":43},"value":{"signature":"sigF","err":{"InstructionError":[0,{"Custom":1}]},"logs":null}}}});
        match parse_notification(&failed, &ids, 5).unwrap().pop().unwrap() {
            IngressEvent::Logs(l) => {
                assert!(matches!(l.logs.status, TxStatus::Failed(_)) && l.logs.logs.is_none())
            }
            other => panic!("expected Logs, got {other:?}"),
        }
        // A missing status is UNKNOWN, never success; a non-string log line is a protocol error.
        let absent = json!({"jsonrpc":"2.0","method":"logsNotification","params":{"subscription":7,
            "result":{"context":{"slot":44},"value":{"signature":"sigA","logs":[]}}}});
        match parse_notification(&absent, &ids, 5).unwrap().pop().unwrap() {
            IngressEvent::Logs(l) => {
                assert_eq!(l.logs.status, TxStatus::Unknown("TX_META_STATUS_MISSING"))
            }
            other => panic!("expected Logs, got {other:?}"),
        }
        let broken = json!({"jsonrpc":"2.0","method":"logsNotification","params":{"subscription":7,
            "result":{"context":{"slot":44},"value":{"signature":"sigB","err":null,"logs":[1]}}}});
        assert!(parse_notification(&broken, &ids, 5).is_err());
        let sig = "1".repeat(64);
        let tx = json!({"jsonrpc":"2.0","method":"transactionNotification","params":{"subscription":8,
            "result":{"signature":sig,"slot":44,"transaction":{"transaction":{"signatures":[sig],"message":{"accountKeys":[],"instructions":[]}},
            "meta":{"err":null,"fee":5000,"logMessages":["a"]},"version":0}}}});
        match parse_notification(&tx, &ids, 5).unwrap().pop().unwrap() {
            IngressEvent::Transaction(t) => {
                let decoded = t.tx.unwrap();
                let meta = decoded.meta.unwrap();
                assert_eq!(
                    (meta.fee, meta.log_messages),
                    (Some(5000), Some(vec!["a".into()]))
                );
            }
            other => panic!("expected Transaction, got {other:?}"),
        }
        // An undecodable payload keeps its identity and reason (no reconnect loop, no tree).
        let odd = json!({"jsonrpc":"2.0","method":"transactionNotification","params":{"subscription":8,
            "result":{"signature":"sigT","slot":45,"transaction":{"transaction":{}}}}});
        match parse_notification(&odd, &ids, 5).unwrap().pop().unwrap() {
            IngressEvent::Transaction(t) => {
                assert_eq!((t.signature.as_str(), t.slot), ("sigT", 45));
                assert_eq!(t.tx, Err(crate::chain::ChainError("NOTIFICATION_DECODE")));
            }
            other => panic!("expected Transaction, got {other:?}"),
        }
    }

    #[test]
    fn parses_program_notification() {
        let pk = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let data = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        let msg = json!({
            "jsonrpc":"2.0","method":"programNotification",
            "params":{"subscription":7,"result":{"context":{"slot":123},
              "value":{"pubkey":pk.to_string(),"account":{"lamports":5,"owner":owner.to_string(),
                       "data":[data,"base64"],"executable":false,"rentEpoch":0}}}}
        });
        let ev = parse_notification(&msg, &HashMap::new(), 999).unwrap();
        match &ev[0] {
            IngressEvent::Account(a) => {
                assert_eq!(a.pubkey, pk);
                assert_eq!(a.owner, owner);
                assert_eq!(a.slot, 123);
                assert_eq!(a.data, vec![1, 2, 3]);
                assert_eq!(a.t_rx_ns, 999);
                assert_eq!(a.write_version, None);
            }
            _ => panic!("expected Account"),
        }
    }

    #[test]
    fn parses_slot_and_root() {
        let slot = json!({"method":"slotNotification","params":{"subscription":1,"result":{"parent":9,"root":5,"slot":10}}});
        let ev = parse_notification(&slot, &HashMap::new(), 1).unwrap();
        assert!(matches!(
            ev[0],
            IngressEvent::Slot(SlotUpdate {
                slot: 10,
                parent: Some(9),
                ..
            })
        ));
        let root = json!({"method":"rootNotification","params":{"subscription":2,"result":42}});
        let ev = parse_notification(&root, &HashMap::new(), 1).unwrap();
        assert!(matches!(ev[0], IngressEvent::Root { slot: 42, .. }));
    }

    /// N8: the ACK window grows with the subscriptions but never past the recovery budget.
    #[test]
    fn ack_window_scales_with_subscriptions_within_recovery_budget() {
        let idle = Duration::from_secs(60);
        let per = Duration::from_millis(5);
        assert_eq!(ack_window(idle, 0, per, None), idle);
        assert_eq!(
            ack_window(idle, 10_000, per, None),
            idle + Duration::from_secs(50)
        );
        assert_eq!(
            ack_window(idle, 10_000, per, Some(Duration::from_secs(90))),
            Duration::from_secs(90)
        );
        assert_eq!(
            ack_window(idle, 3, per, Some(Duration::ZERO)),
            Duration::ZERO
        );
        assert_eq!(
            ack_window(idle, usize::MAX, Duration::from_secs(u64::MAX / 2), None),
            Duration::MAX
        );
    }
    #[test]
    fn account_notification_needs_known_subscription() {
        let msg = json!({"method":"accountNotification","params":{"subscription":3,"result":{"context":{"slot":1},"value":{"lamports":1,"owner":Pubkey::new_unique().to_string(),"data":["","base64"]}}}});
        assert!(parse_notification(&msg, &HashMap::new(), 1).is_err());
    }
}
