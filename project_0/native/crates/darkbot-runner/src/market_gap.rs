//! Bounded finalized-block recovery after capture and drain. Never a submission path.
use super::*;
use crate::winner::resolve_account_keys;
use serde::Serialize;

#[derive(Clone, Copy, Debug)]
pub struct GapConfig {
    pub max_gaps: usize,
    pub max_slots: u64,
    pub max_attempts: u32,
    pub timeout_ms: u64,
    pub retry_ms: u64,
    pub phase_max_secs: u64,
    pub max_transactions: u64,
}
impl Default for GapConfig {
    fn default() -> Self {
        Self {
            max_gaps: 32,
            max_slots: 512,
            max_attempts: 3,
            timeout_ms: 10_000,
            retry_ms: 250,
            phase_max_secs: 180,
            max_transactions: 2_000_000,
        }
    }
}
impl GapConfig {
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let n = |key: &str, default: u64, max: u64| -> Result<u64, String> {
            let value = match get(key) {
                Some(v) => v.parse().map_err(|_| format!("invalid {key}"))?,
                None => default,
            };
            if value == 0 || value > max {
                return Err(format!("out of range {key}"));
            }
            Ok(value)
        };
        let d = Self::default();
        Ok(Self {
            max_gaps: n("P0_MARKET_GAP_MAX_GAPS", d.max_gaps as u64, 256)? as usize,
            max_slots: n("P0_MARKET_GAP_MAX_SLOTS", d.max_slots, 4096)?,
            max_attempts: n("P0_MARKET_GAP_MAX_ATTEMPTS", u64::from(d.max_attempts), 10)? as u32,
            timeout_ms: n("P0_MARKET_GAP_TIMEOUT_MS", d.timeout_ms, 60_000)?,
            retry_ms: n("P0_MARKET_GAP_RETRY_MS", d.retry_ms, 10_000)?,
            phase_max_secs: n(
                "P0_MARKET_GAP_PHASE_MAX_SECS",
                d.phase_max_secs,
                u64::from(u32::MAX),
            )?,
            max_transactions: n(
                "P0_MARKET_GAP_MAX_TRANSACTIONS",
                d.max_transactions,
                2_000_000,
            )?,
        })
    }
}

#[derive(Debug, Serialize)]
pub(super) struct MarketGap {
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
    recovery_status: &'static str,
    error_code: Option<&'static str>,
    slots_checked: u64,
    transactions_scanned: u64,
    liquidation_events: u64,
}
#[derive(Default)]
pub(super) struct GapStats {
    pub gap_recovery_attempted: u64,
    pub gap_recovered: u64,
    pub gap_slots_requested: u64,
    pub gap_slots_fetched: u64,
    pub gap_slots_skipped_confirmed: u64,
    pub gap_slots_failed: u64,
    pub gap_transactions_scanned: u64,
    pub gap_transactions_deduplicated: u64,
    pub gap_transactions_recovered: u64,
    pub gap_liquidation_transactions: u64,
    pub gap_liquidation_events: u64,
}

pub(super) enum GapSlot {
    Block(Value),
    // A finalized successor's parent link proves there is no canonical block in
    // the open interval. A null getBlock or error -32007 alone does NOT prove it.
    Skipped {
        successor: u64,
        parent: u64,
        blockhash: String,
    },
}
type SlotFetcher<'a> = dyn FnMut(u64, Duration) -> Result<GapSlot, &'static str> + 'a;

impl ObserverCore {
    pub(super) fn record_gap(&mut self, at: Option<u64>, now: u64, detail: &Value) {
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
            from_slot: detail["from_slot"].as_u64(),
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
            recovery_status: "PENDING",
            error_code: None,
            slots_checked: 0,
            transactions_scanned: 0,
            liquidation_events: 0,
        });
    }
    pub(super) fn record_gap_reconnect(&mut self, at: Option<u64>, detail: &Value) {
        for g in &mut self.gaps {
            if g.first_post_reconnect_slot.is_none()
                && at.zip(g.gap_t_rx_ns).is_some_and(|(a, b)| a >= b)
            {
                g.reconnect_t_rx_ns = at;
                g.reconnect_attempt = detail["attempt"].as_u64();
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
                sink.coverage(
                    &json!({"event":"GAP_BOUNDARY", "timestamp_utc":utc_iso(now),
                    "t_rx_ns":at, "detail":g}),
                )?;
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

    /// This API refuses to invoke its fetcher until capture has stopped. Production
    /// additionally invokes it only after a successful drain in finish_capture().
    pub(super) fn recover_gaps(
        &mut self,
        fetch: &mut SlotFetcher<'_>,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        if self.observing {
            return Err(std::io::Error::other("GAP_RECOVERY_DURING_CAPTURE"));
        }
        let deadline = Instant::now() + Duration::from_secs(self.gap_cfg.phase_max_secs);
        for index in 0..self.gaps.len() {
            if self.gaps[index].recovery_status == "RECOVERED" {
                continue;
            }
            self.gap_stats.gap_recovery_attempted += 1;
            let g = &self.gaps[index];
            let range = match (g.from_slot, g.first_post_reconnect_slot) {
                (None, _) => Err("FROM_SLOT_UNKNOWN"),
                (_, None) => Err("POST_RECONNECT_SLOT_UNKNOWN"),
                (Some(lo), Some(hi)) if hi < lo || hi - lo >= self.gap_cfg.max_slots => {
                    Err("GAP_SLOT_BUDGET")
                }
                (Some(lo), Some(hi)) => Ok(lo..=hi),
            };
            let before_tx = self.gap_stats.gap_transactions_scanned;
            let before_liq = self.gap_stats.gap_liquidation_events;
            let mut failure = range.as_ref().err().copied();
            if let Ok(range) = range {
                self.gap_stats.gap_slots_requested += *range.end() - *range.start() + 1;
                for slot in range {
                    if self.reconciliation.complete
                        && self
                            .reconciliation
                            .market_slot_start
                            .as_ref()
                            .is_some_and(|b| slot >= b.slot)
                        && self
                            .reconciliation
                            .market_slot_end
                            .as_ref()
                            .is_some_and(|b| slot <= b.slot)
                    {
                        self.gaps[index].slots_checked += 1;
                        sink.coverage(&json!({"event":"GAP_SLOT_RESULT", "gap_index":index,
                            "slot":slot,"error_code":null,"proof":"FULL_FINALIZED_SLOT_RECONCILIATION"}))?;
                        continue;
                    }
                    let mut result = Err("RECOVERY_BUDGET");
                    for attempt in 0..self.gap_cfg.max_attempts {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if remaining.is_zero() {
                            result = Err("RECOVERY_BUDGET");
                            break;
                        }
                        result = fetch(slot, remaining);
                        if result.is_ok() {
                            break;
                        }
                        if attempt + 1 < self.gap_cfg.max_attempts {
                            std::thread::sleep(
                                Duration::from_millis(self.gap_cfg.retry_ms)
                                    .min(deadline.saturating_duration_since(Instant::now())),
                            );
                        }
                    }
                    if Instant::now() >= deadline {
                        result = Err("RECOVERY_BUDGET");
                    }
                    let checked = match result {
                        Ok(GapSlot::Block(block)) => {
                            self.gap_stats.gap_slots_fetched += 1;
                            self.recover_block(slot, block, deadline, sink)?
                        }
                        Ok(GapSlot::Skipped {
                            successor,
                            parent,
                            blockhash,
                        }) if parent < slot && slot < successor && !blockhash.is_empty() => {
                            self.gap_stats.gap_slots_skipped_confirmed += 1;
                            sink.coverage(&json!({"event":"GAP_SLOT_SKIPPED_CONFIRMED", "slot":slot,
                                "proof":"finalized_successor_parent_link", "successor_slot":successor,
                                "parent_slot":parent, "successor_blockhash":blockhash}))?;
                            Ok(())
                        }
                        Ok(GapSlot::Skipped { .. }) => Err("SKIP_PROOF_INVALID"),
                        Err(code) => Err(code),
                    };
                    self.gaps[index].slots_checked += 1;
                    sink.coverage(&json!({"event":"GAP_SLOT_RESULT", "gap_index":index,
                        "slot":slot, "error_code":checked.as_ref().err()}))?;
                    if let Err(code) = checked {
                        self.gap_stats.gap_slots_failed += 1;
                        failure = Some(code);
                        // The entire gap stays unqualified; do not burn its remaining RPC budget.
                        break;
                    }
                }
            }
            let g = &mut self.gaps[index];
            g.error_code = failure;
            g.recovery_status = if failure.is_none() {
                "RECOVERED"
            } else {
                "UNRECOVERED"
            };
            g.transactions_scanned += self.gap_stats.gap_transactions_scanned - before_tx;
            g.liquidation_events += self.gap_stats.gap_liquidation_events - before_liq;
            if failure.is_none() {
                self.gap_stats.gap_recovered += 1;
            }
            sink.coverage(
                &json!({"event":if failure.is_none() {"GAP_RECOVERED"} else {"GAP_UNRECOVERED"},
                "gap_index":index, "detail":g}),
            )?;
        }
        if self.coverage_complete() {
            self.incomplete_reasons.remove("GAP");
        }
        Ok(())
    }

    fn recover_block(
        &mut self,
        slot: u64,
        block: Value,
        deadline: Instant,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<Result<(), &'static str>> {
        if block["parentSlot"].as_u64().is_none_or(|p| p >= slot)
            || block["blockhash"].as_str().is_none_or(str::is_empty)
        {
            return Ok(Err("BLOCK_HEADER_INVALID"));
        }
        let Some(transactions) = block["transactions"].as_array() else {
            return Ok(Err("BLOCK_TRANSACTIONS_MISSING"));
        };
        for (index, raw) in transactions.iter().enumerate() {
            if Instant::now() >= deadline {
                return Ok(Err("RECOVERY_BUDGET"));
            }
            if self.gap_stats.gap_transactions_scanned >= self.gap_cfg.max_transactions {
                return Ok(Err("GAP_TRANSACTION_BUDGET"));
            }
            self.gap_stats.gap_transactions_scanned += 1;
            let mut raw = raw.clone();
            let Some(obj) = raw.as_object_mut() else {
                return Ok(Err("TX_SHAPE"));
            };
            obj.insert("slot".into(), json!(slot));
            obj.insert("transactionIndex".into(), json!(index));
            if let Err(code) = self.recover_transaction(slot, index, &raw, sink)? {
                return Ok(Err(code));
            }
        }
        sink.coverage(&json!({"event":"GAP_BLOCK_FETCHED", "slot":slot,
            "blockhash":block["blockhash"], "parent_slot":block["parentSlot"], "transactions":transactions.len()}))?;
        Ok(Ok(()))
    }

    pub(super) fn recover_transaction(
        &mut self,
        slot: u64,
        index: usize,
        raw: &Value,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<Result<(), &'static str>> {
        // Validate all transactions, including those irrelevant to Marginfi: missing
        // loaded addresses must never turn an unknown relevance decision into false.
        if !matches!(raw["version"].as_str(), Some("legacy"))
            && !matches!(raw["version"].as_u64(), Some(0 | 1))
        {
            return Ok(Err("TX_VERSION_UNSUPPORTED_OR_MISSING"));
        }
        if raw["meta"].get("err").is_none() {
            return Ok(Err("TX_META_STATUS_MISSING"));
        }
        let tx: RpcTransaction = match serde_json::from_value(raw.clone()) {
            Ok(v) => v,
            Err(_) => return Ok(Err("TX_DECODE")),
        };
        let Some(signature) = tx.transaction.signatures.first() else {
            return Ok(Err("TX_SIGNATURE_MISSING"));
        };
        if check_transaction_identity(&tx, signature, slot).is_err() {
            return Ok(Err("TX_IDENTITY"));
        }
        let keys = match resolve_account_keys(&tx) {
            Ok(k) => k,
            Err(e) => return Ok(Err(e.code())),
        };
        if !keys.contains(&self.program) {
            return Ok(Ok(()));
        }
        let success = raw["meta"]["err"].is_null();
        let logs: Option<Vec<String>> = raw["meta"]["logMessages"]
            .as_array()
            .and_then(|a| a.iter().map(|v| v.as_str().map(str::to_owned)).collect());
        let Some(logs) = logs else {
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
            match qualify_truncated(raw.clone(), signature, slot, &self.program, &pairs) {
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
        if !self.admit_identity(signature, slot, success) {
            return Ok(Err("MARKET_RETENTION"));
        }
        self.gap_stats.gap_transactions_recovered += 1;
        sink.transaction(
            &json!({"source":"gap_backfill", "live_timestamp_qualified":false,
            "t_rx_ns":null, "observed_at_utc":null, "slot":slot, "signature":signature,
            "transaction_index":index, "success":success, "raw":raw}),
        )?;
        self.absorb_decode(&decoded, success);
        if success && decoded.truncated {
            let pairs = self.note_truncated(&decoded);
            let q = qualify_truncated(raw.clone(), signature, slot, &self.program, &pairs);
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
            self.write_liquidation_rows(
                sink,
                signature,
                slot,
                None,
                unix_now_s(),
                &decoded.events,
                Some(raw),
                "GAP_BLOCK_PAYLOAD",
            )?;
        }
        Ok(Ok(()))
    }

    pub(super) fn gap_recovery_phase(
        &mut self,
        url: Option<&str>,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        if self.stats.gap_count == 0 {
            return Ok(());
        }
        if self.observing {
            return Err(std::io::Error::other("GAP_RECOVERY_DURING_CAPTURE"));
        }
        sink.coverage(
            &json!({"event":"GAP_RECOVERY_STARTED", "phase":"POST_CAPTURE_AFTER_DRAIN",
            "timestamp_utc":utc_iso(unix_now_s()), "observing":self.observing}),
        )?;
        let before = self.stats.clone();
        let cfg = self.gap_cfg;
        let rpc = url.ok_or("RPC_URL_MISSING").and_then(|u| {
            RpcFetcher::new(u.into(), cfg.timeout_ms, "finalized").map_err(|_| "RPC_INIT")
        });
        let result = self.recover_gaps(
            &mut |slot, remaining| match &rpc {
                Ok(rpc) => rpc.gap_slot(slot, remaining, cfg),
                Err(code) => Err(*code),
            },
            sink,
        );
        if let (Some(h), Some((_, cutoff))) = (&self.econ, self.window) {
            h.emit(crate::econ::Evidence::MarketCounters {
                at_ns: cutoff,
                before,
                after: Box::new(self.stats.clone()),
            });
        }
        result
    }
}

impl RpcFetcher {
    /// Bounded JSON body and sanitized error codes; no provider text or endpoint escapes.
    pub(super) fn gap_request(
        &self,
        method: &str,
        params: Value,
        deadline: Instant,
        timeout_ms: u64,
    ) -> Result<Value, &'static str> {
        self.gap_request_metered(method, params, deadline, timeout_ms, 64 * 1024 * 1024)
            .map(|(value, _bytes)| value)
    }
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

    fn gap_slot(
        &self,
        slot: u64,
        remaining: Duration,
        cfg: GapConfig,
    ) -> Result<GapSlot, &'static str> {
        let deadline = Instant::now() + remaining;
        let config = json!({"encoding":"json", "transactionDetails":"full", "rewards":false,
            "commitment":"finalized", "maxSupportedTransactionVersion":1});
        match self.gap_request("getBlock", json!([slot, config]), deadline, cfg.timeout_ms) {
            Ok(block) => Ok(GapSlot::Block(block)),
            Err("SKIPPED_OR_LEDGER_MISSING") => {
                // getBlocks is only a successor locator, never absence proof.
                let end = slot.checked_add(cfg.max_slots).ok_or("SLOT_OVERFLOW")?;
                let blocks = self.gap_request(
                    "getBlocks",
                    json!([slot + 1, end, {"commitment":"finalized"}]),
                    deadline,
                    cfg.timeout_ms,
                )?;
                let successor = blocks
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_u64)
                    .filter(|s| *s > slot && *s <= end)
                    .ok_or("SKIP_SUCCESSOR_UNAVAILABLE")?;
                let header = self.gap_request("getBlock", json!([successor, {"transactionDetails":"none",
                    "rewards":false, "commitment":"finalized", "maxSupportedTransactionVersion":1}]), deadline, cfg.timeout_ms)?;
                let parent = header["parentSlot"]
                    .as_u64()
                    .filter(|p| *p < slot)
                    .ok_or("SKIP_PARENT_UNPROVEN")?;
                let blockhash = header["blockhash"]
                    .as_str()
                    .filter(|h| !h.is_empty())
                    .ok_or("SKIP_HEADER_MISSING")?
                    .to_owned();
                Ok(GapSlot::Skipped {
                    successor,
                    parent,
                    blockhash,
                })
            }
            Err(code) => Err(code),
        }
    }
}
