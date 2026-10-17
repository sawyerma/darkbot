//! Typed market capture rows: the payloads of `MARKET_TRANSACTION`, `MARKET_LIQUIDATION` and
//! `MARKET_COVERAGE` (schema 2, JFI-FIX-03). Producers build these values; the journal stores
//! their native encoding. The `legacy_json` renders reproduce the historical JSONL rows for the
//! explicit offline export only (pinned readers); no production path calls them.
use super::*;
use crate::analytics::{journal, payload::Payload};
use darkbot_solana::chain::{self, ChainLogs};
use darkbot_solana::provider::ProviderSnapshot;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------------------
// MARKET_TRANSACTION
// ------------------------------------------------------------------------------------------

/// One observed or verified transaction. The payload is the decoded transaction of the wire
/// boundary; nothing else is retained (no provider JSON tree).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketTransaction {
    pub signature: String,
    pub slot: u64,
    pub origin: TxOrigin,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TxOrigin {
    /// `transactionSubscribe`; an undecodable payload keeps its reason code.
    TransactionSubscribe {
        observed_unix_s: u64,
        t_rx_ns: u64,
        tx: Result<ChainTx, String>,
    },
    /// `logsSubscribe`: status and log lines only.
    LogsSubscribe {
        observed_unix_s: u64,
        t_rx_ns: u64,
        logs: ChainLogs,
    },
    /// Finalized block entry recovered after capture (never a live receipt).
    GapBackfill { transaction_index: u64, tx: ChainTx },
    /// Program-relevant entry of the finalized full-slot scan.
    FullSlot {
        transaction_index: u64,
        blockhash: String,
        block_time: Option<i64>,
        tx: ChainTx,
    },
}

impl Payload for MarketTransaction {
    const KIND: u16 = journal::kind::MARKET_TRANSACTION;
}

impl MarketTransaction {
    /// The decoded transaction of this row, if it carries one.
    pub fn chain_tx(&self) -> Option<&ChainTx> {
        match &self.origin {
            TxOrigin::TransactionSubscribe { tx: Ok(tx), .. }
            | TxOrigin::GapBackfill { tx, .. }
            | TxOrigin::FullSlot { tx, .. } => Some(tx),
            _ => None,
        }
    }
    /// The logs notification of a `logsSubscribe` row.
    pub fn chain_logs(&self) -> Option<&ChainLogs> {
        match &self.origin {
            TxOrigin::LogsSubscribe { logs, .. } => Some(logs),
            _ => None,
        }
    }
    pub fn t_rx_ns(&self) -> Option<u64> {
        match &self.origin {
            TxOrigin::TransactionSubscribe { t_rx_ns, .. }
            | TxOrigin::LogsSubscribe { t_rx_ns, .. } => Some(*t_rx_ns),
            _ => None,
        }
    }

    /// Historical `transactions.jsonl` row (offline render).
    pub fn legacy_json(&self) -> Value {
        let block_entry = |tx: &ChainTx, index: u64| {
            let mut raw = chain::encode_entry(tx);
            raw["slot"] = self.slot.into();
            raw["transactionIndex"] = index.into();
            raw
        };
        let err_text = |s: &TxStatus| {
            chain::encode_status(s)
                .filter(|v| !v.is_null())
                .map(|v| v.to_string())
        };
        match &self.origin {
            TxOrigin::TransactionSubscribe {
                observed_unix_s,
                t_rx_ns,
                tx,
            } => {
                let mut row = json!({"observed_at_utc": utc_iso(*observed_unix_s), "t_rx_ns": t_rx_ns,
                    "slot": self.slot, "signature": self.signature, "source": "transactionSubscribe"});
                match tx {
                    Ok(tx) => {
                        let status = tx.status();
                        row["tx_version"] = legacy_version_label(tx).into();
                        row["success"] = status.is_success().into();
                        row["err"] = err_text(&status).into();
                        row["raw"] = json!({"signature": self.signature, "slot": self.slot,
                            "transaction": chain::encode_entry(tx)});
                    }
                    Err(code) => {
                        row["tx_version"] = Value::Null;
                        row["success"] = false.into();
                        row["err"] = Value::Null;
                        row["raw"] = Value::Null;
                        row["decode_error"] = code.clone().into();
                    }
                }
                row
            }
            TxOrigin::LogsSubscribe {
                observed_unix_s,
                t_rx_ns,
                logs,
            } => json!({"observed_at_utc": utc_iso(*observed_unix_s), "t_rx_ns": t_rx_ns,
                "slot": self.slot, "signature": self.signature, "tx_version": Value::Null,
                "success": logs.status.is_success(), "err": err_text(&logs.status),
                "source": "logsSubscribe", "raw": chain::encode_logs(logs)}),
            TxOrigin::GapBackfill {
                transaction_index,
                tx,
            } => json!({"source": "gap_backfill", "live_timestamp_qualified": false,
                "t_rx_ns": Value::Null, "observed_at_utc": Value::Null, "slot": self.slot,
                "signature": self.signature, "transaction_index": transaction_index,
                "success": tx.status().is_success(), "raw": block_entry(tx, *transaction_index)}),
            TxOrigin::FullSlot {
                transaction_index,
                blockhash,
                block_time,
                tx,
            } => json!({"source": "full_slot_reconciliation", "slot": self.slot,
                "transaction_index": transaction_index, "signature": self.signature,
                "blockhash": blockhash, "block_time": block_time,
                "raw": block_entry(tx, *transaction_index)}),
        }
    }
}

/// The historical `tx_version` label (`V0` / `V1` / `Legacy`) of a decoded transaction.
fn legacy_version_label(tx: &ChainTx) -> &'static str {
    use darkbot_solana::chain::TxWireVersion as V;
    if tx.message.transaction_config.is_some() {
        return "V1";
    }
    match tx.version {
        Some(V::Number(1)) => "V1",
        Some(V::Number(0)) => "V0",
        Some(V::Legacy) => "Legacy",
        _ if tx.message.address_table_lookups.is_some() => "V0",
        _ => "Legacy",
    }
}

// ------------------------------------------------------------------------------------------
// MARKET_LIQUIDATION
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MarketLiquidation {
    /// A receivership event decoded by the market observer, with its enrichment.
    Receiver(ReceiverRow),
    /// An event of the independent finalized full-slot classification.
    Canonical(Box<truth::TruthEvent>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReceiverRow {
    pub signature: String,
    pub slot: u64,
    /// `None` for an event recovered from a finalized block (no live receipt).
    pub t_rx_ns: Option<u64>,
    pub observed_unix_s: u64,
    pub event: ReceiverEvent,
    pub enrichment: TxEnrichment,
    pub enrichment_status: String,
}

impl Payload for MarketLiquidation {
    const KIND: u16 = journal::kind::MARKET_LIQUIDATION;
}

impl MarketLiquidation {
    pub fn slot(&self) -> u64 {
        match self {
            MarketLiquidation::Receiver(r) => r.slot,
            MarketLiquidation::Canonical(e) => e.slot,
        }
    }
    pub fn t_rx_ns(&self) -> Option<u64> {
        match self {
            MarketLiquidation::Receiver(r) => r.t_rx_ns,
            MarketLiquidation::Canonical(e) => e.live_t_rx_ns,
        }
    }

    /// Historical `liquidations.jsonl` row (offline render).
    pub fn legacy_json(&self) -> Value {
        match self {
            MarketLiquidation::Canonical(event) => {
                json!({"source": "canonical_market_truth", "event": event})
            }
            MarketLiquidation::Receiver(r) => {
                let (ev, e) = (&r.event, &r.enrichment);
                let token_deltas: Vec<Value> = e
                    .receiver_token_deltas
                    .iter()
                    .map(|d| json!({"mint": d.mint, "native_delta": d.native_delta.to_string(), "decimals": d.decimals}))
                    .collect();
                json!({
                    "observed_at_utc": r.t_rx_ns.map(|_| utc_iso(r.observed_unix_s)),
                    "t_rx_ns": r.t_rx_ns,
                    "source": if r.t_rx_ns.is_some() { "live" } else { "gap_backfill" },
                    "live_timestamp_qualified": r.t_rx_ns.is_some(),
                    "slot": r.slot,
                    "signature": r.signature,
                    "event_index": ev.event_index,
                    "tx_version": e.tx_version,
                    "target": ev.target.to_string(),
                    "receiver": ev.receiver.to_string(),
                    "seized_equity_usd": ev.seized_equity_usd,
                    "repaid_equity_usd": ev.repaid_equity_usd,
                    "gross_edge_usd": ev.gross_edge_usd(),
                    "protocol_flat_fee_lamports": ev.protocol_flat_fee_lamports,
                    "tx_fee_lamports": e.tx_fee_lamports,
                    "fee_payer": e.fee_payer,
                    "receiver_lamport_delta": e.receiver_lamport_delta.map(|d| d.to_string()),
                    "receiver_token_deltas": token_deltas,
                    "receiver_token_deltas_complete": e.token_deltas_complete,
                    "cu_limit": e.cu_limit,
                    "cu_price_micro_lamports": e.cu_price_micro_lamports,
                    "transaction_success": true,
                    "enrichment_status": r.enrichment_status,
                })
            }
        }
    }
}

// ------------------------------------------------------------------------------------------
// MARKET_COVERAGE
// ------------------------------------------------------------------------------------------

/// Every coverage record of capture, gap bookkeeping and full-slot verification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MarketCoverage {
    /// A transport/lifecycle event with window counting (`ObserverCore::coverage`).
    Window {
        observed_unix_s: u64,
        t_rx_ns: Option<u64>,
        observing: bool,
        counted: bool,
        event: WindowEvent,
    },
    ShadowWarmupWait {
        observed_unix_s: u64,
    },
    ShadowWarmupFailed {
        error: String,
    },
    ShadowWarmupReady {
        at_ns: u64,
        observed_unix_s: u64,
    },
    /// In-window slot notification extending the slot-complete window (not finality proof).
    SlotBoundary {
        slot: u64,
        t_rx_ns: u64,
        observed_unix_s: u64,
        start: Option<SlotBoundaryRow>,
        end: Option<SlotBoundaryRow>,
    },
    ReconciliationFinal(ReconciliationRow),
    SlotFailed {
        slot: u64,
        error: String,
    },
    SlotSkipped {
        slot: u64,
        successor_slot: u64,
        parent_slot: u64,
        successor_blockhash: String,
    },
    SlotProduced {
        slot: u64,
        blockhash: String,
        parent_slot: u64,
        previous_blockhash: String,
        block_time: Option<i64>,
        transactions_scanned: u64,
    },
    GapBoundary {
        observed_unix_s: u64,
        t_rx_ns: u64,
        gap: gap::MarketGap,
    },
    /// `full_window_proof`: the slot lies inside a completed full-slot reconciliation.
    GapSlotResult {
        gap_index: u64,
        slot: u64,
        error_code: Option<String>,
        full_window_proof: bool,
    },
    GapSlotSkipped {
        slot: u64,
        successor_slot: u64,
        parent_slot: u64,
        successor_blockhash: String,
    },
    GapResult {
        gap_index: u64,
        recovered: bool,
        gap: gap::MarketGap,
    },
    GapBlockFetched {
        slot: u64,
        blockhash: String,
        parent_slot: u64,
        transactions: u64,
    },
    GapRecoveryStarted {
        observed_unix_s: u64,
        observing: bool,
    },
}

/// Transport and lifecycle events recorded through the observation window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WindowEvent {
    Start(Box<CaptureStart>),
    StreamReady {
        attempt: u32,
        observation_start_t_rx_ns: u64,
        observation_cutoff_t_rx_ns: u64,
    },
    TransportHealth {
        connected: bool,
        last_application_frame_age_ns: Option<u64>,
        application_frames: u64,
        last_slot: Option<u64>,
    },
    TransportState {
        state: String,
        code: String,
        attempt: u32,
        last_valid_slot: Option<u64>,
    },
    Reconnect {
        attempt: u32,
    },
    Gap {
        from_slot: Option<u64>,
        to_slot: Option<u64>,
    },
    Fatal {
        detail: String,
    },
    IngressThreadEnded,
    IngressOverflow {
        overflow_total: u64,
        drain: bool,
    },
    StopRequested {
        reason: String,
    },
    Drained {
        drain_complete: bool,
        events_after_cutoff: u64,
    },
    ShadowRpcQuiescent {
        at_ns: u64,
    },
    ShadowRpcBarrierFailed {
        at_ns: u64,
        error: String,
    },
    EnrichmentInitFailed {
        error: String,
    },
    Stopped {
        reason: String,
        drain_complete: bool,
        write_error: Option<String>,
    },
}

/// The configured capture at START (transport policy as applied by the WSS adapter).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaptureStart {
    pub source: String,
    pub commitment: String,
    pub program: Pubkey,
    pub configured_duration_seconds: u64,
    pub ws_host: String,
    /// Outage budget of the WSS adapter (`None`: unbounded attempts only).
    pub recovery_budget_ms: Option<u64>,
    pub recovery_attempts: u32,
    pub idle_timeout_ms: u64,
    pub healthy_reset_ms: u64,
    pub readiness: String,
    pub provider_budget: ProviderSnapshot,
}

/// Owned copy of `reconcile::SlotBoundary` for storage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlotBoundaryRow {
    pub slot: u64,
    pub t_rx_ns: u64,
    pub observed_unix_s: u64,
    pub provenance: String,
}

/// Owned copy of `reconcile::Reconciliation` for storage (same keys).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationRow {
    pub market_slot_start: Option<SlotBoundaryRow>,
    pub market_slot_end: Option<SlotBoundaryRow>,
    pub attempted: bool,
    pub complete: bool,
    pub slots_expected: u64,
    pub slots_produced: u64,
    pub slots_skipped_confirmed: u64,
    pub slots_fetched: u64,
    pub slots_failed: u64,
    pub slots_unresolved: u64,
    pub transactions_scanned: u64,
    pub marginfi_transactions: u64,
    pub first_block_time: Option<i64>,
    pub last_block_time: Option<i64>,
    pub error: Option<String>,
}

impl Payload for MarketCoverage {
    const KIND: u16 = journal::kind::MARKET_COVERAGE;
}

impl MarketCoverage {
    pub fn t_rx_ns(&self) -> Option<u64> {
        match self {
            MarketCoverage::Window { t_rx_ns, .. } => *t_rx_ns,
            MarketCoverage::SlotBoundary { t_rx_ns, .. }
            | MarketCoverage::GapBoundary { t_rx_ns, .. } => Some(*t_rx_ns),
            _ => None,
        }
    }
    pub fn slot(&self) -> Option<u64> {
        match self {
            MarketCoverage::SlotBoundary { slot, .. }
            | MarketCoverage::SlotFailed { slot, .. }
            | MarketCoverage::SlotSkipped { slot, .. }
            | MarketCoverage::SlotProduced { slot, .. }
            | MarketCoverage::GapSlotResult { slot, .. }
            | MarketCoverage::GapSlotSkipped { slot, .. }
            | MarketCoverage::GapBlockFetched { slot, .. } => Some(*slot),
            _ => None,
        }
    }

    /// Historical `coverage.jsonl` row (offline render).
    pub fn legacy_json(&self) -> Value {
        match self {
            MarketCoverage::Window {
                observed_unix_s,
                t_rx_ns,
                observing,
                counted,
                event,
            } => {
                let (kind, detail) = event.legacy();
                json!({"event": kind, "timestamp_utc": utc_iso(*observed_unix_s), "t_rx_ns": t_rx_ns,
                    "observing": observing, "counted": counted, "detail": detail})
            }
            MarketCoverage::ShadowWarmupWait { observed_unix_s } => {
                json!({"event": "SHADOW_WARMUP_WAIT", "timestamp_utc": utc_iso(*observed_unix_s)})
            }
            MarketCoverage::ShadowWarmupFailed { error } => {
                json!({"event": "SHADOW_WARMUP_FAILED", "error": error})
            }
            MarketCoverage::ShadowWarmupReady {
                at_ns,
                observed_unix_s,
            } => json!({"event": "SHADOW_WARMUP_READY", "at_ns": at_ns,
                "timestamp_utc": utc_iso(*observed_unix_s)}),
            MarketCoverage::SlotBoundary {
                slot,
                t_rx_ns,
                observed_unix_s,
                start,
                end,
            } => json!({"event": "MARKET_SLOT_BOUNDARY_EVIDENCE", "slot": slot, "t_rx_ns": t_rx_ns,
                "timestamp_utc": utc_iso(*observed_unix_s), "detail": {
                    "market_slot_start": start, "market_slot_end": end,
                    "window_semantics": "inclusive whole slots from first to highest in-window slot notification; partial temporal boundary slots included",
                    "finality_proven": false}}),
            MarketCoverage::ReconciliationFinal(r) => {
                json!({"event": "FULL_SLOT_RECONCILIATION_FINAL", "detail": r})
            }
            MarketCoverage::SlotFailed { slot, error } => {
                json!({"event": "FULL_SLOT_RESULT", "slot": slot, "status": "FAILED", "error": error})
            }
            MarketCoverage::SlotSkipped {
                slot,
                successor_slot,
                parent_slot,
                successor_blockhash,
            } => json!({"event": "FULL_SLOT_RESULT", "slot": slot, "status": "SKIPPED_CONFIRMED",
                "successor_slot": successor_slot, "parent_slot": parent_slot,
                "successor_blockhash": successor_blockhash, "proof": "FINALIZED_PARENT_LINK"}),
            MarketCoverage::SlotProduced {
                slot,
                blockhash,
                parent_slot,
                previous_blockhash,
                block_time,
                transactions_scanned,
            } => json!({"event": "FULL_SLOT_RESULT", "slot": slot, "status": "PRODUCED_FETCHED",
                "blockhash": blockhash, "parent_slot": parent_slot,
                "previous_blockhash": previous_blockhash, "block_time": block_time,
                "transactions_scanned": transactions_scanned}),
            MarketCoverage::GapBoundary {
                observed_unix_s,
                t_rx_ns,
                gap,
            } => json!({"event": "GAP_BOUNDARY", "timestamp_utc": utc_iso(*observed_unix_s),
                "t_rx_ns": t_rx_ns, "detail": gap}),
            MarketCoverage::GapSlotResult {
                gap_index,
                slot,
                error_code,
                full_window_proof,
            } => {
                let mut row = json!({"event": "GAP_SLOT_RESULT", "gap_index": gap_index,
                    "slot": slot, "error_code": error_code});
                if *full_window_proof {
                    row["proof"] = "FULL_FINALIZED_SLOT_RECONCILIATION".into();
                }
                row
            }
            MarketCoverage::GapSlotSkipped {
                slot,
                successor_slot,
                parent_slot,
                successor_blockhash,
            } => json!({"event": "GAP_SLOT_SKIPPED_CONFIRMED", "slot": slot,
                "proof": "finalized_successor_parent_link", "successor_slot": successor_slot,
                "parent_slot": parent_slot, "successor_blockhash": successor_blockhash}),
            MarketCoverage::GapResult {
                gap_index,
                recovered,
                gap,
            } => json!({"event": if *recovered { "GAP_RECOVERED" } else { "GAP_UNRECOVERED" },
                "gap_index": gap_index, "detail": gap}),
            MarketCoverage::GapBlockFetched {
                slot,
                blockhash,
                parent_slot,
                transactions,
            } => json!({"event": "GAP_BLOCK_FETCHED", "slot": slot, "blockhash": blockhash,
                "parent_slot": parent_slot, "transactions": transactions}),
            MarketCoverage::GapRecoveryStarted {
                observed_unix_s,
                observing,
            } => json!({"event": "GAP_RECOVERY_STARTED", "phase": "POST_CAPTURE_AFTER_DRAIN",
                "timestamp_utc": utc_iso(*observed_unix_s), "observing": observing}),
        }
    }
}

impl WindowEvent {
    /// Historical `event` name and `detail` object.
    fn legacy(&self) -> (&'static str, Value) {
        match self {
            WindowEvent::Start(s) => (
                "START",
                json!({
                "source": s.source, "commitment": s.commitment, "program": s.program.to_string(),
                "configured_duration_seconds": s.configured_duration_seconds, "ws_host": s.ws_host,
                "transport_policy": {"recovery_seconds": s.recovery_budget_ms.map(|ms| ms / 1000),
                    "maximum_failed_attempts": s.recovery_attempts,
                    "slot_idle_seconds": s.idle_timeout_ms / 1000,
                    "healthy_reset_seconds": s.healthy_reset_ms / 1000, "readiness": s.readiness},
                "provider_budget": s.provider_budget}),
            ),
            WindowEvent::StreamReady {
                attempt,
                observation_start_t_rx_ns,
                observation_cutoff_t_rx_ns,
            } => (
                "STREAM_READY",
                json!({"attempt": attempt,
                "observation_start_t_rx_ns": observation_start_t_rx_ns,
                "observation_cutoff_t_rx_ns": observation_cutoff_t_rx_ns}),
            ),
            WindowEvent::TransportHealth {
                connected,
                last_application_frame_age_ns,
                application_frames,
                last_slot,
            } => (
                "TRANSPORT_HEALTH",
                json!({"connected": connected,
                "last_application_frame_age_ns": last_application_frame_age_ns,
                "application_frames": application_frames, "last_slot": last_slot}),
            ),
            WindowEvent::TransportState {
                state,
                code,
                attempt,
                last_valid_slot,
            } => (
                "TRANSPORT_STATE",
                json!({"state": state, "code": code, "attempt": attempt,
                "last_valid_slot": last_valid_slot}),
            ),
            WindowEvent::Reconnect { attempt } => ("RECONNECT", json!({"attempt": attempt})),
            WindowEvent::Gap { from_slot, to_slot } => {
                ("GAP", json!({"from_slot": from_slot, "to_slot": to_slot}))
            }
            WindowEvent::Fatal { detail } => ("FATAL", json!({"detail": detail})),
            WindowEvent::IngressThreadEnded => ("FATAL", json!({"reason": "INGRESS_THREAD_ENDED"})),
            WindowEvent::IngressOverflow {
                overflow_total,
                drain,
            } => (
                "INGRESS_OVERFLOW",
                if *drain {
                    json!({"overflow_total": overflow_total, "phase": "drain"})
                } else {
                    json!({"overflow_total": overflow_total})
                },
            ),
            WindowEvent::StopRequested { reason } => ("STOP_REQUESTED", json!({"reason": reason})),
            WindowEvent::Drained {
                drain_complete,
                events_after_cutoff,
            } => (
                "DRAINED",
                json!({"drain_complete": drain_complete,
                "events_after_cutoff": events_after_cutoff}),
            ),
            WindowEvent::ShadowRpcQuiescent { at_ns } => (
                "SHADOW_RPC_QUIESCENT",
                json!({"at_ns": at_ns, "error": Value::Null}),
            ),
            WindowEvent::ShadowRpcBarrierFailed { at_ns, error } => (
                "SHADOW_RPC_BARRIER_FAILED",
                json!({"at_ns": at_ns, "error": error}),
            ),
            WindowEvent::EnrichmentInitFailed { error } => {
                ("ENRICHMENT_INIT_FAILED", json!({"error": error}))
            }
            WindowEvent::Stopped {
                reason,
                drain_complete,
                write_error,
            } => (
                "STOPPED",
                json!({"reason": reason, "drain_complete": drain_complete,
                "write_error": write_error}),
            ),
        }
    }
}

/// Offline render of one typed market frame to its historical JSONL row.
pub fn legacy_view(kind: u16, schema: u16, bytes: &[u8]) -> std::io::Result<Value> {
    use crate::analytics::payload::decode;
    match kind {
        journal::kind::MARKET_TRANSACTION => {
            decode::<MarketTransaction>(kind, schema, bytes).map(|r| r.legacy_json())
        }
        journal::kind::MARKET_LIQUIDATION => {
            decode::<MarketLiquidation>(kind, schema, bytes).map(|r| r.legacy_json())
        }
        journal::kind::MARKET_COVERAGE => {
            decode::<MarketCoverage>(kind, schema, bytes).map(|r| r.legacy_json())
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "NO_LEGACY_VIEW",
        )),
    }
}
