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
//! Status: UNTESTED / VERIFY (tokio-tungstenite API, Helius-WSS-Verhalten,
//! Credit-Verbrauch der programSubscribe → Ledger S1-05).

use super::{
    AccountUpdate, IngressError, IngressEvent, IngressHalt, IngressSource, SlotStatus, SlotUpdate, SlotWatch, SourceKind,
    Subscription,
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

#[derive(Clone, Debug)]
pub struct WsRpcConfig {
    /// z. B. wss://<provider>/?api-key=... — nur Konfiguration, nie im Code.
    pub ws_url: String,
    pub subscription: Subscription,
    /// Commitment für Account-/Program-Subscriptions.
    pub commitment: &'static str,
    pub reconnect_initial: Duration,
    pub reconnect_max: Duration,
    /// Ohne Frame innerhalb dieser Zeit gilt der Stream als tot (Ping/Pong reicht nicht).
    pub idle_timeout: Duration,
}

impl WsRpcConfig {
    pub fn new(ws_url: String, subscription: Subscription) -> Self {
        WsRpcConfig {
            ws_url,
            subscription,
            commitment: "confirmed",
            reconnect_initial: Duration::from_millis(500),
            reconnect_max: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
        }
    }
}

pub struct WsRpcControlSource {
    cfg: WsRpcConfig,
    clock: Clock,
}

impl WsRpcControlSource {
    pub fn new(cfg: WsRpcConfig, clock: Clock) -> Self {
        WsRpcControlSource { cfg, clock }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubKind { // [FIX-52] matches parse_notification visibility
    Program,
    Account,
    Slot,
    Root,
    Transaction,
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
        out.push((id, SubKind::Slot, json!({"jsonrpc":"2.0","id":id,"method":"slotSubscribe"})));
        id += 1;
    }
    if sub.roots {
        out.push((id, SubKind::Root, json!({"jsonrpc":"2.0","id":id,"method":"rootSubscribe"})));
        id += 1;
    }
    // VERIFY: `transactionSubscribe` ist eine Provider-Erweiterung (Helius Enhanced WSS, nicht Standard-RPC).
    // Für Stufe 1 optional; Standardweg für Shadow bleibt getBlock/getTransaction in P6-LT.
    for p in &sub.transactions_mentioning {
        out.push((
            id,
            SubKind::Transaction,
            json!({"jsonrpc":"2.0","id":id,"method":"transactionSubscribe",
                   "params":[{"accountInclude":[p.to_string()], "failed": true},
                             {"commitment":commitment, "encoding":"json", "transactionDetails":"full", "maxSupportedTransactionVersion": 1}]}),
        ));
        id += 1;
    }
    out
}

fn parse_account_value(v: &Value, pubkey: Pubkey, slot: u64, t_rx_ns: u64) -> Result<AccountUpdate, IngressError> {
    let acc = v;
    let lamports = acc
        .get("lamports")
        .and_then(|x| x.as_u64())
        .ok_or_else(|| IngressError::Protocol("account.lamports missing".into()))?;
    let owner_s = acc
        .get("owner")
        .and_then(|x| x.as_str())
        .ok_or_else(|| IngressError::Protocol("account.owner missing".into()))?;
    let owner = Pubkey::from_str(owner_s).map_err(|e| IngressError::Protocol(format!("owner: {}", e)))?;
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
    let params = msg.get("params").ok_or_else(|| IngressError::Protocol("params missing".into()))?;
    let sub_id = params.get("subscription").and_then(|s| s.as_u64());
    let result = params.get("result").ok_or_else(|| IngressError::Protocol("result missing".into()))?;

    match method {
        "programNotification" => {
            let slot = result.pointer("/context/slot").and_then(|s| s.as_u64()).ok_or_else(|| IngressError::Protocol("context.slot missing".into()))?;
            let pk_s = result
                .pointer("/value/pubkey")
                .and_then(|x| x.as_str())
                .ok_or_else(|| IngressError::Protocol("value.pubkey missing".into()))?;
            let pk = Pubkey::from_str(pk_s).map_err(|e| IngressError::Protocol(format!("pubkey: {}", e)))?;
            let acc = result
                .pointer("/value/account")
                .ok_or_else(|| IngressError::Protocol("value.account missing".into()))?;
            Ok(vec![IngressEvent::Account(parse_account_value(acc, pk, slot, t_rx_ns)?)])
        }
        "accountNotification" => {
            let slot = result.pointer("/context/slot").and_then(|s| s.as_u64()).ok_or_else(|| IngressError::Protocol("context.slot missing".into()))?;
            let pk = sub_id
                .and_then(|id| sub_ids.get(&id))
                .and_then(|(_, pk)| *pk)
                .ok_or_else(|| IngressError::Protocol("accountNotification for unknown subscription".into()))?;
            let acc = result.get("value").ok_or_else(|| IngressError::Protocol("value missing".into()))?;
            Ok(vec![IngressEvent::Account(parse_account_value(acc, pk, slot, t_rx_ns)?)])
        }
        "slotNotification" => {
            let slot = result.get("slot").and_then(|s| s.as_u64()).ok_or_else(|| IngressError::Protocol("slot".into()))?;
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
            // VERIFY: Helius-Format {result:{signature, slot, transaction:{transaction, meta, version}}}
            let sig = result.get("signature").and_then(|s| s.as_str()).ok_or_else(|| IngressError::Protocol("signature missing".into()))?.to_string();
            let slot = result.get("slot").and_then(|s| s.as_u64()).ok_or_else(|| IngressError::Protocol("slot missing".into()))?;
            let tx = result.get("transaction").cloned().unwrap_or(Value::Null);
            let version = super::detect_version_json(&tx);
            let err = tx.pointer("/meta/err").filter(|e| !e.is_null()).map(|e| e.to_string());
            Ok(vec![IngressEvent::Transaction(super::TransactionObserved {
                signature: sig, slot, tx_index_in_slot: None, version, err, source: SourceKind::WsRpcControl, t_rx_ns,
            })])
        }
        "rootNotification" => {
            let slot = result.as_u64().ok_or_else(|| IngressError::Protocol("root".into()))?;
            Ok(vec![IngressEvent::Root { slot, source: SourceKind::WsRpcControl, t_rx_ns }])
        }
        _ => Ok(Vec::new()),
    }
}

impl IngressSource for WsRpcControlSource {
    fn kind(&self) -> SourceKind {
        SourceKind::WsRpcControl
    }

    fn run(self, sink: Producer<IngressEvent>, shutdown: Shutdown, halt: IngressHalt, slots: SlotWatch) -> Result<(), IngressError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| IngressError::Config(format!("tokio: {}", e)))?;
        rt.block_on(self.run_async(sink, shutdown, halt, slots))
    }
}

impl WsRpcControlSource {
    async fn run_async(self, sink: Producer<IngressEvent>, shutdown: Shutdown, halt: IngressHalt, slots: SlotWatch) -> Result<(), IngressError> {
        let mut attempt: u32 = 0;
        let mut backoff = self.cfg.reconnect_initial;
        let mut last_slot: Option<u64> = None;

        while !shutdown.is_requested() {
            attempt += 1;
            match self.session(&sink, &shutdown, &halt, &slots, &mut last_slot, attempt).await {
                Ok(()) => return Ok(()), // Shutdown
                Err(e) => {
                    let t = self.clock.now_ns();
                    // Out-of-Band zuerst: sichtbar, auch wenn der Channel voll ist.
                    halt.raise();
                    // Reconnect ist für WSS immer eine Lücke (kein Replay).
                    let _ = sink.try_push(IngressEvent::Gap {
                        source: SourceKind::WsRpcControl,
                        from_slot: last_slot,
                        to_slot: None,
                        t_rx_ns: t,
                    });
                    // [FIX-36] KEIN `Reconnected` hier — der Stream ist unten. StreamReady kommt aus der
                    // nächsten Session, sobald alle ACKs da sind.
                    eprintln!("[ws_rpc] session ended: {} — reconnect in {:?}", e, backoff);
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(self.cfg.reconnect_max);
                }
            }
        }
        Ok(())
    }

    async fn session(
        &self,
        sink: &Producer<IngressEvent>,
        shutdown: &Shutdown,
        halt: &IngressHalt,
        slots: &SlotWatch,
        last_slot: &mut Option<u64>,
        attempt: u32,
    ) -> Result<(), IngressError> {
        let (ws, _) = tokio_tungstenite::connect_async(&self.cfg.ws_url)
            .await
            .map_err(|e| IngressError::Connect(e.to_string()))?;
        let (mut tx, mut rx) = ws.split();

        // Subscribe-Requests senden; Bestätigungen (id → subscription) einsammeln.
        let requests = build_subscribe_requests(&self.cfg.subscription, self.cfg.commitment);
        let mut pending: HashMap<u64, (SubKind, Option<Pubkey>)> = HashMap::new();
        let mut acc_iter = self.cfg.subscription.accounts.iter();
        for (id, kind, req) in &requests {
            let pk = if *kind == SubKind::Account { acc_iter.next().copied() } else { None };
            pending.insert(*id, (*kind, pk));
            tx.send(Message::Text(req.to_string().into()))
                .await
                .map_err(|e| IngressError::Connect(e.to_string()))?;
        }
        let mut sub_ids: HashMap<u64, (SubKind, Option<Pubkey>)> = HashMap::new();
        let mut stream_ready = false; // [FIX-36]
        let mut frames: u64 = 0;
        let mut last_frame_ns: Option<u64> = None;
        let mut last_health_ns: u64 = self.clock.now_ns();

        loop {
            if shutdown.is_requested() {
                return Ok(());
            }
            let frame = tokio::time::timeout(self.cfg.idle_timeout, rx.next()).await;
            let t_rx_ns = self.clock.now_ns(); // T_rx: vor Parse
            // Health-Event alle 5 s (Ledger: stream health), unabhängig vom Frame-Inhalt.
            if t_rx_ns.saturating_sub(last_health_ns) >= 5_000_000_000 {
                last_health_ns = t_rx_ns;
                let _ = sink.try_push(IngressEvent::Health(super::SourceHealth {
                    source: SourceKind::WsRpcControl,
                    connected: true,
                    last_frame_age_ns: last_frame_ns.map(|l| t_rx_ns.saturating_sub(l)),
                    frames,
                    reconnects: 0,
                    last_slot: *last_slot,
                    t_rx_ns,
                }));
            }
            let frame = match frame {
                Err(_) => return Err(IngressError::Connect("idle timeout".into())),
                Ok(None) => return Err(IngressError::Connect("stream closed".into())),
                Ok(Some(Err(e))) => return Err(IngressError::Connect(e.to_string())),
                Ok(Some(Ok(f))) => f,
            };
            frames += 1;
            last_frame_ns = Some(t_rx_ns);
            let text = match frame {
                Message::Text(t) => t.to_string(),
                Message::Ping(p) => {
                    let _ = tx.send(Message::Pong(p)).await;
                    continue;
                }
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
                // [FIX-36] StreamReady-Barriere: alle Subscriptions bestätigt → genau ein `Reconnected`.
                if !stream_ready && pending.is_empty() {
                    stream_ready = true;
                    if let Err(ChannelError::Disconnected(_)) = sink.try_push(IngressEvent::Reconnected {
                        source: SourceKind::WsRpcControl,
                        attempt,
                        t_rx_ns: self.clock.now_ns(),
                    }) {
                        return Ok(());
                    }
                }
                continue;
            }
            if let Some(err) = v.get("error") {
                return Err(IngressError::Protocol(format!("rpc error: {}", err)));
            }

            let events = parse_notification(&v, &sub_ids, t_rx_ns)?;
            for ev in events {
                if let IngressEvent::Slot(s) = &ev {
                    *last_slot = Some(s.slot);
                    slots.observe(s.slot); // [FIX-40]
                }
                if let IngressEvent::Account(a) = &ev {
                    *last_slot = Some(last_slot.map_or(a.slot, |l| l.max(a.slot)));
                    slots.observe(a.slot); // [FIX-40]
                }
                match sink.try_push(ev) {
                    Ok(()) => {}
                    Err(ChannelError::Full(_)) => {
                        // Kein stiller Drop: Out-of-Band-Halt (garantiert sichtbar) + Gap-Versuch.
                        halt.raise();
                        let _ = sink.try_push(IngressEvent::Gap {
                            source: SourceKind::WsRpcControl,
                            from_slot: *last_slot,
                            to_slot: None,
                            t_rx_ns,
                        });
                        return Err(IngressError::Protocol("sink overflow".into()));
                    }
                    Err(ChannelError::Disconnected(_)) => return Ok(()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_requests_cover_all_kinds() {
        let p = Pubkey::new_unique();
        let a = Pubkey::new_unique();
        let sub = Subscription { programs: vec![p], accounts: vec![a], slots: true, roots: true, transactions_mentioning: vec![] };
        let reqs = build_subscribe_requests(&sub, "confirmed");
        assert_eq!(reqs.len(), 4);
        assert_eq!(reqs[0].2["method"], "programSubscribe");
        assert_eq!(reqs[1].2["method"], "accountSubscribe");
        assert_eq!(reqs[2].2["method"], "slotSubscribe");
        assert_eq!(reqs[3].2["method"], "rootSubscribe");
        assert_eq!(reqs[0].2["params"][1]["encoding"], "base64");
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
        assert!(matches!(ev[0], IngressEvent::Slot(SlotUpdate { slot: 10, parent: Some(9), .. })));
        let root = json!({"method":"rootNotification","params":{"subscription":2,"result":42}});
        let ev = parse_notification(&root, &HashMap::new(), 1).unwrap();
        assert!(matches!(ev[0], IngressEvent::Root { slot: 42, .. }));
    }

    #[test]
    fn account_notification_needs_known_subscription() {
        let msg = json!({"method":"accountNotification","params":{"subscription":3,"result":{"context":{"slot":1},"value":{"lamports":1,"owner":Pubkey::new_unique().to_string(),"data":["","base64"]}}}});
        assert!(parse_notification(&msg, &HashMap::new(), 1).is_err());
    }
}
