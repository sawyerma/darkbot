//! Transport gap bookkeeping of the live capture: outage intervals from GAP/RECONNECT/slot
//! evidence, never repaired here. Finalized repair is the strict verifier's full-slot scan;
//! `recover_transaction` admits a program transaction found there that was not seen live.
use super::rows::{MarketCoverage, MarketTransaction, TxOrigin};
use super::*;
use crate::winner::resolve_account_keys;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
pub struct GapConfig {
    /// Bounded number of retained outage intervals; more is GAP_STATE_CAPACITY (INCOMPLETE).
    pub max_gaps: usize,
}
impl Default for GapConfig {
    fn default() -> Self {
        Self { max_gaps: 32 }
    }
}
impl GapConfig {
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let max_gaps = match get("P0_MARKET_GAP_MAX_GAPS") {
            Some(v) => v.parse().map_err(|_| "invalid P0_MARKET_GAP_MAX_GAPS")?,
            None => Self::default().max_gaps as u64,
        };
        if max_gaps == 0 || max_gaps > 256 {
            return Err("out of range P0_MARKET_GAP_MAX_GAPS".into());
        }
        Ok(Self {
            max_gaps: max_gaps as usize,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarketGap {
    from_slot: Option<u64>,
    pub(super) raw_failures: u64,
    last_failure_t_rx_ns: Option<u64>,
    first_failure_unix_s: u64,
    last_failure_unix_s: u64,
    restored_unix_s: Option<u64>,
    first_post_reconnect_slot: Option<u64>,
    first_post_reconnect_t_rx_ns: Option<u64>,
    gap_t_rx_ns: Option<u64>,
    last_healthy_t_rx_ns: Option<u64>,
    reconnect_t_rx_ns: Option<u64>,
    reconnect_attempt: Option<u64>,
}
/// Program transactions of the finalized full-slot scan that were not received live.
#[derive(Default)]
pub(super) struct GapStats {
    pub gap_transactions_deduplicated: u64,
    pub gap_transactions_recovered: u64,
    pub gap_liquidation_transactions: u64,
    pub gap_liquidation_events: u64,
}

impl ObserverCore {
    pub(super) fn record_gap(&mut self, at: Option<u64>, now: u64, from_slot: Option<u64>) {
        if let Some(g) = self
            .gaps
            .last_mut()
            .filter(|g| g.first_post_reconnect_slot.is_none())
        {
            // Preserve the earliest unknown boundary. New ACKs do not prove restoration.
            g.raw_failures = g.raw_failures.saturating_add(1);
            g.last_failure_t_rx_ns = at;
            g.last_failure_unix_s = now;
            g.reconnect_t_rx_ns = None;
            g.reconnect_attempt = None;
            return;
        }
        if self.gaps.len() >= self.gap_cfg.max_gaps {
            self.mark_incomplete("GAP_STATE_CAPACITY");
            return;
        }
        self.gaps.push(MarketGap {
            from_slot,
            raw_failures: 1,
            last_failure_t_rx_ns: at,
            first_failure_unix_s: now,
            last_failure_unix_s: now,
            restored_unix_s: None,
            first_post_reconnect_slot: None,
            first_post_reconnect_t_rx_ns: None,
            gap_t_rx_ns: at,
            last_healthy_t_rx_ns: self.last_slot_progress_ns,
            reconnect_t_rx_ns: None,
            reconnect_attempt: None,
        });
    }
    pub(super) fn record_gap_reconnect(&mut self, at: Option<u64>, attempt: Option<u64>) {
        for g in &mut self.gaps {
            if g.first_post_reconnect_slot.is_none()
                && at.zip(g.gap_t_rx_ns).is_some_and(|(a, b)| a >= b)
            {
                g.reconnect_t_rx_ns = at;
                g.reconnect_attempt = attempt;
            }
        }
    }
    pub(super) fn close_gap_ranges(
        &mut self,
        slot: u64,
        at: u64,
        now: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        if !self.in_window(Some(at)) {
            return Ok(());
        }
        for g in &mut self.gaps {
            if g.first_post_reconnect_slot.is_none()
                && g.reconnect_t_rx_ns.is_some_and(|r| at >= r)
                && g.from_slot.is_none_or(|s| slot >= s)
            {
                g.first_post_reconnect_slot = Some(slot);
                g.restored_unix_s = Some(now);
                g.first_post_reconnect_t_rx_ns = Some(at);
                sink.coverage(&MarketCoverage::GapBoundary {
                    observed_unix_s: now,
                    t_rx_ns: at,
                    gap: g.clone(),
                })?;
            }
        }
        Ok(())
    }

    /// Measured outage interval; a stalled feed starts at the last proven slot,
    /// not only when the idle timer finally diagnoses it. Missing bounds stay UNKNOWN.
    pub(super) fn transport_outage_ns(&self, end: u64) -> Option<u64> {
        let mut total = 0u64;
        let mut previous_end = 0;
        for g in &self.gaps {
            let start = g.last_healthy_t_rx_ns.or(g.gap_t_rx_ns)?;
            let stop = g.first_post_reconnect_t_rx_ns.unwrap_or(end).min(end);
            let start = start.max(previous_end);
            total = total.saturating_add(stop.saturating_sub(start));
            previous_end = previous_end.max(stop);
        }
        Some(total)
    }

    pub(super) fn recover_transaction(
        &mut self,
        slot: u64,
        index: usize,
        tx: &ChainTx,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<Result<(), &'static str>> {
        // Validate all transactions, including those irrelevant to Marginfi: missing
        // loaded addresses must never turn an unknown relevance decision into false.
        if tx.slot != slot || tx.transaction_index != Some(index as u64) {
            return Ok(Err("TX_IDENTITY"));
        }
        if !matches!(
            tx.version,
            Some(darkbot_solana::chain::TxWireVersion::Legacy)
                | Some(darkbot_solana::chain::TxWireVersion::Number(0 | 1))
        ) {
            return Ok(Err("TX_VERSION_UNSUPPORTED_OR_MISSING"));
        }
        let success = match tx.status() {
            darkbot_solana::chain::TxStatus::Success => true,
            darkbot_solana::chain::TxStatus::Failed(_) => false,
            darkbot_solana::chain::TxStatus::Unknown("TX_META_MISSING") => {
                return Ok(Err("TX_META_STATUS_MISSING"))
            }
            darkbot_solana::chain::TxStatus::Unknown(code) => return Ok(Err(code)),
        };
        let Some(signature) = tx.signature() else {
            return Ok(Err("TX_SIGNATURE_MISSING"));
        };
        if check_transaction_identity(tx, signature, slot).is_err() {
            return Ok(Err("TX_IDENTITY"));
        }
        let keys = match resolve_account_keys(tx) {
            Ok(k) => k,
            Err(e) => return Ok(Err(e.code())),
        };
        if !keys.contains(&self.program) {
            return Ok(Ok(()));
        }
        let Some(logs) = tx.meta.as_ref().and_then(|m| m.log_messages.clone()) else {
            return Ok(Err("LOG_MESSAGES_MISSING"));
        };
        let decoded = decode_receiver_events(&logs, &self.program);
        if success {
            if decoded.decoder_errors != 0 {
                return Ok(Err("DECODER_ERROR"));
            }
            let pairs: Vec<_> = decoded
                .events
                .iter()
                .map(|e| (e.target, e.receiver))
                .collect();
            match qualify_truncated(tx, signature, slot, &self.program, &pairs) {
                TruncationQualification::Qualified { .. } => {}
                TruncationQualification::Mismatch { .. } => {
                    return Ok(Err("LIQUIDATION_INSTRUCTION_LOG_MISMATCH"))
                }
                TruncationQualification::Unqualified(code) => return Ok(Err(code)),
            }
        }
        if self.seen.contains(signature) {
            self.gap_stats.gap_transactions_deduplicated += 1;
            return Ok(Ok(()));
        }
        if !self.admit_identity(signature, slot, Some(success)) {
            return Ok(Err("MARKET_RETENTION"));
        }
        self.gap_stats.gap_transactions_recovered += 1;
        sink.transaction(&MarketTransaction {
            signature: signature.to_owned(),
            slot,
            origin: TxOrigin::GapBackfill {
                transaction_index: index as u64,
                tx: tx.clone(),
            },
        })?;
        self.absorb_decode(&decoded, success);
        if success && decoded.truncated {
            let pairs = self.note_truncated(&decoded);
            let q = qualify_truncated(tx, signature, slot, &self.program, &pairs);
            self.apply_truncation(q);
        }
        if success && !decoded.events.is_empty() {
            self.gap_stats.gap_liquidation_transactions += 1;
            self.gap_stats.gap_liquidation_events += decoded.events.len() as u64;
            self.aggregate(
                signature,
                slot,
                None,
                u32::try_from(index).ok(),
                &decoded.events,
            );
            self.stats.enrichment_requested += 1;
            self.stats.enrichment_succeeded += 1;
            // D08: a gap row never gets time 0; the gap stays unrecovered instead.
            let Ok(now_s) = unix_now_s() else {
                return Ok(Err(darkbot_common::timing::CLOCK_UNAVAILABLE));
            };
            self.write_liquidation_rows(
                sink,
                signature,
                slot,
                None,
                now_s,
                &decoded.events,
                Some(tx),
                "GAP_BLOCK_PAYLOAD",
            )?;
        }
        Ok(Ok(()))
    }
}

impl RpcFetcher {
    /// Bounded JSON body and sanitized error codes; no provider text or endpoint escapes.
    pub(super) fn gap_request_metered(
        &self,
        method: &str,
        params: Value,
        deadline: Instant,
        timeout_ms: u64,
        response_limit: u64,
    ) -> Result<(Value, u64), &'static str> {
        let limits = &darkbot_solana::runtime_config::active().limits;
        let timeout_ms = timeout_ms.min(limits.rpc_timeout_ms);
        let deadline = deadline.min(Instant::now() + Duration::from_millis(timeout_ms));
        let _permit = darkbot_solana::provider::shared()
            .acquire(darkbot_solana::provider::Workload::History, deadline)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("RECOVERY_BUDGET");
        }
        let body = json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params});
        darkbot_solana::provider::shared()
            .account_bytes(serde_json::to_vec(&body).map_err(|_| "RPC_ENCODE")?.len())?;
        let response = self
            .client
            .post(&self.url)
            .timeout(remaining.min(Duration::from_millis(timeout_ms)))
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    "RPC_TIMEOUT"
                } else {
                    "RPC_TRANSPORT"
                }
            })?;
        darkbot_solana::provider::observe_response(&response, method)?;
        let limit = response_limit
            .min(64 * 1024 * 1024)
            .min(limits.max_response_mib * 1024 * 1024);
        let bytes = darkbot_solana::provider::read_body(
            response,
            limit,
            &darkbot_solana::provider::shared(),
        )?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "RPC_JSON")?;
        if value.get("error").is_some() {
            return Err(match value["error"]["code"].as_i64() {
                Some(-32007) => "SKIPPED_OR_LEDGER_MISSING",
                Some(-32009 | -32001) => "RPC_PRUNED_OR_UNAVAILABLE",
                Some(-32015) => "RPC_UNSUPPORTED_VERSION",
                // A rate limit answered in the JSON-RPC body (as the pinned verifier reads it).
                Some(429 | -32005) => {
                    darkbot_solana::provider::shared()
                        .rate_limited_header(darkbot_solana::provider::RetryAfterHeader::Absent);
                    "RPC_RATE_LIMIT"
                }
                _ => "RPC_ERROR",
            });
        }
        let length = bytes.len() as u64;
        value
            .get("result")
            .cloned()
            .filter(|v| !v.is_null())
            .map(|value| (value, length))
            .ok_or("RPC_MISSING_BLOCK")
    }
}
