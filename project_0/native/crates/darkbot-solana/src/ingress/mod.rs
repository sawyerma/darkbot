//! S1-01: IngressSource-Contract + Event-Modell.
//! S1-04: Transaktionsversions-Erkennung (config → versioned → legacy).
//!
//! Beide Adapter-Klassen (WsRpcControlSource jetzt, GeyserSource später bei
//! `S1-RACE`) liefern dasselbe Event-Modell. Businesslogik sieht nur `IngressEvent`.

pub mod ws_rpc;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// [FIX-40] Zuletzt beobachteter Chain-Slot, vom Ingress lockfrei aktualisiert, exakt bei T_sign gelesen
/// (Winner-Vergleich). Kein RPC, kein Umweg über den Decision-Loop.
#[derive(Clone, Debug, Default)]
pub struct SlotWatch {
    slot: Arc<std::sync::atomic::AtomicU64>,
}

impl SlotWatch {
    pub fn new() -> Self { Self::default() }
    #[inline]
    pub fn observe(&self, slot: u64) { self.slot.fetch_max(slot, Ordering::Relaxed); }
    #[inline]
    pub fn get(&self) -> u64 { self.slot.load(Ordering::Relaxed) }
}

/// Out-of-Band-Signal: Sink-Overflow oder Fatal, das **nicht** über den (vollen) Channel
/// transportiert werden kann. Der Consumer prüft es vor jeder Entscheidung → HALT(FeedGap).
#[derive(Clone, Debug, Default)]
pub struct IngressHalt {
    flag: Arc<AtomicBool>,
}

impl IngressHalt {
    pub fn new() -> Self { Self::default() }
    pub fn raise(&self) { self.flag.store(true, Ordering::SeqCst); }
    pub fn is_raised(&self) -> bool { self.flag.load(Ordering::SeqCst) }
    /// Nur nach vollständiger Recovery zurücksetzen.
    pub fn clear(&self) { self.flag.store(false, Ordering::SeqCst); }
}

use darkbot_common::channel::Producer;
use darkbot_common::lifecycle::Shutdown;
use solana_sdk::pubkey::Pubkey;
use std::fmt;

/// Kennung der Quelle für Journal/Provenienz (Record.source).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceKind {
    /// JSON-WebSocket + RPC. Erlaubt für Long-Tail/Premium-LIVE, Bootstrap, Recovery, Evidence.
    WsRpcControl = 1,
    /// Binärer Geyser-Stream. Voraussetzung für Race-LIVE (S1-RACE PASS).
    Geyser = 2,
    /// RPC-Snapshot (getProgramAccounts / getMultipleAccounts).
    Snapshot = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotStatus {
    Processed,
    Confirmed,
    Finalized,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountUpdate {
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub data: Vec<u8>,
    pub slot: u64,
    /// Nur Geyser liefert write_version; WSS/RPC: None (Ordnung dann nur über slot).
    pub write_version: Option<u64>,
    pub source: SourceKind,
    /// T_rx auf der gemeinsamen monotonen Uhr (darkbot_common::timing::Clock).
    pub t_rx_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotUpdate {
    pub slot: u64,
    pub parent: Option<u64>,
    pub status: SlotStatus,
    pub source: SourceKind,
    pub t_rx_ns: u64,
}

/// Transaktions-Beobachtung (Shadow/H0/Winner-Vergleich). Rohdaten bleiben JSON/Bytes,
/// die Version wird in der Pflichtreihenfolge erkannt (siehe `detect_version_*`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionObserved {
    pub signature: String,
    pub slot: u64,
    pub tx_index_in_slot: Option<u32>,
    pub version: TxVersion,
    pub err: Option<String>,
    pub source: SourceKind,
    pub t_rx_ns: u64,
}

/// Quellen-Gesundheit, periodisch vom Adapter emittiert (Ledger: stream health).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceHealth {
    pub source: SourceKind,
    pub connected: bool,
    pub last_frame_age_ns: Option<u64>,
    pub frames: u64,
    pub reconnects: u32,
    pub last_slot: Option<u64>,
    pub t_rx_ns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IngressEvent {
    Account(AccountUpdate),
    Transaction(TransactionObserved),
    Health(SourceHealth),
    Slot(SlotUpdate),
    /// Neuer Root (Finalisierung) — Fork-Policy in S2.
    Root { slot: u64, source: SourceKind, t_rx_ns: u64 },
    /// Verbindung neu aufgebaut. Für WSS ohne Replay bedeutet das: State muss
    /// per Snapshot reconciled werden, bevor wieder getradet wird.
    Reconnected { source: SourceKind, attempt: u32, t_rx_ns: u64 },
    /// Lücke bekannt: `from_slot` = letzter vertrauenswürdiger Slot, `to_slot` = erster nach der Lücke.
    Gap { source: SourceKind, from_slot: Option<u64>, to_slot: Option<u64>, t_rx_ns: u64 },
    /// Adapterfehler, der nicht selbst behoben wurde (fail closed beim Empfänger).
    Fatal { source: SourceKind, detail: String, t_rx_ns: u64 },
}

impl IngressEvent {
    pub fn t_rx_ns(&self) -> u64 {
        match self {
            IngressEvent::Account(a) => a.t_rx_ns,
            IngressEvent::Transaction(t) => t.t_rx_ns,
            IngressEvent::Health(h) => h.t_rx_ns,
            IngressEvent::Slot(s) => s.t_rx_ns,
            IngressEvent::Root { t_rx_ns, .. }
            | IngressEvent::Reconnected { t_rx_ns, .. }
            | IngressEvent::Gap { t_rx_ns, .. }
            | IngressEvent::Fatal { t_rx_ns, .. } => *t_rx_ns,
        }
    }
}

#[derive(Debug)]
pub enum IngressError {
    Connect(String),
    Protocol(String),
    Config(String),
}

impl fmt::Display for IngressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngressError::Connect(s) => write!(f, "ingress connect: {}", s),
            IngressError::Protocol(s) => write!(f, "ingress protocol: {}", s),
            IngressError::Config(s) => write!(f, "ingress config: {}", s),
        }
    }
}

impl std::error::Error for IngressError {}

/// Was ein Adapter abonniert. Provider-neutral.
#[derive(Clone, Debug, Default)]
pub struct Subscription {
    /// Ungefilterte Program-Subscriptions.
    pub programs: Vec<Pubkey>,
    /// Gefilterte programSubscribe-Subscriptions (Solana RPC filters). [FIX-79]
    pub program_filters: Vec<(Pubkey, Vec<serde_json::Value>)>,
    /// Einzelne Accounts (Oracles, FeeState, LUTs …).
    pub accounts: Vec<Pubkey>,
    pub slots: bool,
    pub roots: bool,
    /// Transaktionen, die ein Programm berühren (Shadow/Winner-Vergleich; Stufe 1 optional).
    pub transactions_mentioning: Vec<Pubkey>,
}

/// Contract für alle Ingress-Adapter. `run` blockiert bis Shutdown oder Fatal.
pub trait IngressSource {
    fn kind(&self) -> SourceKind;
    fn run(self, sink: Producer<IngressEvent>, shutdown: Shutdown, halt: IngressHalt, slots: SlotWatch) -> Result<(), IngressError>;
}

// ---------------------------------------------------------------------------
// S1-04: Transaktionsversion — Reihenfolge ist zwingend (Solana Foundation):
//   1. Message.config vorhanden  → v1
//   2. versioned == true / version == 0 → v0
//   3. sonst legacy
// Nie umgekehrt: v1 würde sonst als v0 mit leerem Compute-Budget gelesen.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxVersion {
    Legacy,
    V0,
    V1,
}

/// Erkennung auf einer JSON-RPC-Transaktion (`getTransaction`/`getBlock`, encoding json).
/// VERIFY: Feldname des v1-Config-Objekts in der JSON-Darstellung (`transaction.message.config`
/// laut Upgrade-Doku für gRPC; für JSON-RPC am realen Objekt prüfen).
pub fn detect_version_json(tx: &serde_json::Value) -> TxVersion {
    if tx
        .pointer("/transaction/message/config")
        .map(|c| !c.is_null())
        .unwrap_or(false)
    {
        return TxVersion::V1;
    }
    match tx.get("version") {
        Some(serde_json::Value::Number(n)) if n.as_u64() == Some(1) => TxVersion::V1,
        Some(serde_json::Value::Number(n)) if n.as_u64() == Some(0) => TxVersion::V0,
        Some(serde_json::Value::String(s)) if s == "legacy" => TxVersion::Legacy,
        _ => {
            if tx
                .pointer("/transaction/message/addressTableLookups")
                .map(|v| v.is_array())
                .unwrap_or(false)
            {
                TxVersion::V0
            } else {
                TxVersion::Legacy
            }
        }
    }
}

/// Erkennung auf serialisierten Bytes: erstes Byte der Message nach dem
/// Signatur-Vektor. Prefix-Byte: 0x80 = v0, 0x81 = v1 (Solana Foundation),
/// sonst legacy (Byte = num_required_signatures).
pub fn detect_version_bytes(tx_bytes: &[u8]) -> Option<TxVersion> {
    let (nsigs, mut off) = read_compact_u16(tx_bytes)?;
    off += (nsigs as usize) * 64;
    let b = *tx_bytes.get(off)?;
    Some(match b {
        0x81 => TxVersion::V1,
        0x80 => TxVersion::V0,
        x if x & 0x80 == 0 => TxVersion::Legacy,
        _ => return None,
    })
}

fn read_compact_u16(buf: &[u8]) -> Option<(u16, usize)> {
    let mut val: u16 = 0;
    let mut off = 0usize;
    for i in 0..3 {
        let b = *buf.get(off)?;
        off += 1;
        val |= ((b & 0x7f) as u16) << (7 * i);
        if b & 0x80 == 0 {
            return Some((val, off));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_wins_over_version_field() {
        let v = json!({"version": 0, "transaction": {"message": {"config": {"computeUnitLimit": 1}}}});
        assert_eq!(detect_version_json(&v), TxVersion::V1);
    }

    #[test]
    fn version_field_and_legacy() {
        assert_eq!(detect_version_json(&json!({"version": 0, "transaction": {"message": {}}})), TxVersion::V0);
        assert_eq!(detect_version_json(&json!({"version": "legacy", "transaction": {"message": {}}})), TxVersion::Legacy);
        assert_eq!(detect_version_json(&json!({"transaction": {"message": {}}})), TxVersion::Legacy);
        assert_eq!(
            detect_version_json(&json!({"transaction": {"message": {"addressTableLookups": []}}})),
            TxVersion::V0
        );
    }

    #[test]
    fn bytes_prefix() {
        // 1 Signatur (64 Bytes) + Prefix
        let mut b = vec![1u8];
        b.extend_from_slice(&[0u8; 64]);
        b.push(0x81);
        assert_eq!(detect_version_bytes(&b), Some(TxVersion::V1));
        b[65] = 0x80;
        assert_eq!(detect_version_bytes(&b), Some(TxVersion::V0));
        b[65] = 0x01;
        assert_eq!(detect_version_bytes(&b), Some(TxVersion::Legacy));
        assert_eq!(detect_version_bytes(&[1u8]), None);
    }
}
