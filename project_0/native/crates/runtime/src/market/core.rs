//! Market observation state and event bookkeeping.
use super::*;

// ------------------------------------------------------------------------------------------
// Observer core (pure bookkeeping; I/O only via MarketSink)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct MarketStats {
    pub transactions_received: u64,
    pub unique_transactions: u64,
    pub duplicate_transactions: u64,
    pub successful_transactions: u64,
    pub failed_transactions: u64,
    pub liquidation_transactions: u64,
    pub liquidation_events: u64,
    pub provisional_classic_events: u64,
    pub provisional_deleverage_events: u64,
    pub provisional_bankruptcy_events: u64,
    pub failed_tx_receiver_events_ignored: u64,
    pub foreign_program_events: u64,
    pub decoder_error_count: u64,
    pub decoder_errors_successful_tx: u64,
    pub base64_error_count: u64,
    /// Successful transactions whose logs were truncated by the runtime (total).
    pub logs_truncated: u64,
    /// [r6] Truncated logs proven complete for liquidations via the F8 instruction truth.
    pub logs_truncated_qualified: u64,
    /// [r6] Truncated logs that could not be checked (fetch/parse/identity/...): fail closed.
    pub logs_truncated_unqualified: u64,
    /// [r6] Instruction end_liquidations and logged receiver events disagree: fail closed.
    pub logs_truncated_liquidation_mismatch: u64,
    pub truncated_instruction_end_liquidations: u64,
    pub truncated_log_end_liquidations: u64,
    pub log_messages_missing: u64,
    pub gap_count: u64,
    pub reconnect_count: u64,
    pub transport_reconnect_attempts: u64,
    pub transport_recovered: u64,
    pub transport_http_429: u64,
    pub ingress_overflow_count: u64,
    pub fatal_count: u64,
    pub rpc_fallback_count: u64,
    pub enrichment_requested: u64,
    pub enrichment_succeeded: u64,
    pub enrichment_failed: u64,
    pub enrichment_skipped_queue_full: u64,
    pub protocol_flat_fee_total_lamports: u64,
    pub tx_fee_total_lamports: u64,
    pub tx_fee_unknown_transactions: u64,
    pub events_outside_observation: u64,
    pub events_after_cutoff: u64,
    pub slot_notifications: u64,
    pub max_stream_silence_ms: u64,
    pub first_observed_slot: Option<u64>,
    pub last_observed_slot: Option<u64>,
}

/// [r6] A successful transaction with truncated logs awaiting instruction-based qualification.
pub(super) struct TruncatedPending {
    pub(super) signature: String,
    pub(super) slot: u64,
    /// Receiver events decoded from the (truncated) logs as `(target, receiver)`.
    pub(super) logged: Vec<(Pubkey, Pubkey)>,
}

/// [r6] Outcome of checking a truncated-log transaction against the F8 instruction truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TruncationQualification {
    /// Instruction end_liquidations == logged events (multiset of target+receiver).
    Qualified { instruction_liquidations: usize },
    /// Instruction end_liquidations and logged events differ.
    Mismatch { instruction_liquidations: usize },
    /// Could not be checked; bounded reason code.
    Unqualified(&'static str),
}

/// [r6] Pure qualification of a fetched/payload transaction (typed F8 view) against the logged
/// receiver events. No second decoder: `end_liquidation_instructions` is the winner-parser truth.
pub fn qualify_truncated(
    tx_json: Value,
    signature: &str,
    slot: u64,
    program: &Pubkey,
    logged: &[(Pubkey, Pubkey)],
) -> TruncationQualification {
    let tx: RpcTransaction = match serde_json::from_value(tx_json) {
        Ok(t) => t,
        Err(_) => return TruncationQualification::Unqualified("TX_DECODE"),
    };
    if check_transaction_identity(&tx, signature, slot).is_err() {
        return TruncationQualification::Unqualified("IDENTITY_MISMATCH");
    }
    let Some(meta) = tx.meta.as_ref() else {
        return TruncationQualification::Unqualified("META_MISSING");
    };
    if meta.err.is_some() {
        return TruncationQualification::Unqualified("TX_FAILED_ON_CHAIN");
    }
    match end_liquidation_instructions(&tx, program) {
        Err(e) => TruncationQualification::Unqualified(e.code()),
        Ok(mut instruction) => {
            let mut logged = logged.to_vec();
            instruction.sort();
            logged.sort();
            if instruction == logged {
                TruncationQualification::Qualified {
                    instruction_liquidations: instruction.len(),
                }
            } else {
                TruncationQualification::Mismatch {
                    instruction_liquidations: instruction.len(),
                }
            }
        }
    }
}

pub(super) struct DeferredLiquidation {
    pub(super) signature: String,
    pub(super) slot: u64,
    pub(super) t_rx_ns: u64,
    pub(super) observed_unix_s: u64,
    pub(super) events: Vec<ReceiverEvent>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReceiverRank {
    pub receiver: String,
    pub liquidation_events: u64,
    pub gross_edge_total_usd: f64,
}

pub struct ObserverCore {
    pub(super) econ: Option<crate::analytics::econ::Handle>,
    pub(super) program: Pubkey,
    pub(super) source: MarketSource,
    pub(super) enrich_cfg: EnrichConfig,
    pub(super) observing: bool,
    /// `(start_t_rx_ns, cutoff_t_rx_ns)` on the shared monotonic ingress clock.
    pub(super) window: Option<(u64, u64)>,
    pub(super) seen: HashSet<String>,
    pub(super) liquidation_signatures: HashSet<String>,
    pub(super) targets: HashSet<String>,
    pub(super) edges: Vec<f64>,
    pub(super) receivers: HashMap<String, (u64, f64)>,
    pub(super) deferred: VecDeque<DeferredLiquidation>,
    pub(super) truncated_pending: VecDeque<TruncatedPending>,
    pub(super) truncated_unqualified_reasons: BTreeMap<&'static str, u64>,
    pub(super) incomplete_reasons: BTreeSet<&'static str>,
    pub(super) last_slot_frame_ns: Option<u64>,
    pub(super) last_slot_progress_ns: Option<u64>,
    pub(super) enrichment_init_error: Option<String>,
    pub(super) gap_cfg: GapConfig,
    pub(super) reconcile_cfg: ReconcileConfig,
    pub(super) reconciliation: reconcile::Reconciliation,
    pub(super) truth: truth::TruthLedger,
    pub(super) deployment_before: Option<paths::Deployment>,
    pub(super) deployment_after: Option<paths::Deployment>,
    pub(super) deployment_error: Option<&'static str>,
    pub(super) gaps: Vec<gap::MarketGap>,
    pub(super) gap_stats: gap::GapStats,
    pub stats: MarketStats,
}

/// Final verdict. `MARKET_DATA_VALID` needs a complete capture AND a verified source.
pub fn market_verdict(
    stream_ready: bool,
    market_capture_complete: bool,
    source_coverage_verified: bool,
) -> &'static str {
    if !stream_ready {
        "MARKET_OBSERVER_STREAM_NOT_READY"
    } else if !market_capture_complete {
        "MARKET_DATA_INCOMPLETE"
    } else if !source_coverage_verified {
        "MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED"
    } else {
        "MARKET_DATA_VALID"
    }
}

/// Deterministic nearest-rank quantile on an ascending slice: index `ceil(q*n)-1`, clamped.
pub fn quantile_nearest_rank(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() || !(0.0..=1.0).contains(&q) {
        return None;
    }
    let n = sorted.len();
    let rank = ((q * n as f64).ceil() as usize).clamp(1, n);
    Some(sorted[rank - 1])
}

pub(super) fn ratio(part: f64, total: f64) -> Option<f64> {
    if total > 0.0 {
        Some(part / total)
    } else {
        None
    }
}

impl ObserverCore {
    pub fn new(program: Pubkey, source: MarketSource, enrich_cfg: EnrichConfig) -> Self {
        Self {
            econ: None,
            program,
            source,
            enrich_cfg,
            observing: false,
            window: None,
            seen: HashSet::new(),
            liquidation_signatures: HashSet::new(),
            targets: HashSet::new(),
            edges: Vec::new(),
            receivers: HashMap::new(),
            deferred: VecDeque::new(),
            truncated_pending: VecDeque::new(),
            truncated_unqualified_reasons: BTreeMap::new(),
            incomplete_reasons: BTreeSet::new(),
            last_slot_frame_ns: None,
            last_slot_progress_ns: None,
            enrichment_init_error: None,
            gap_cfg: GapConfig::default(),
            reconcile_cfg: ReconcileConfig::default(),
            reconciliation: reconcile::Reconciliation::default(),
            truth: truth::TruthLedger::default(),
            deployment_before: None,
            deployment_after: None,
            deployment_error: None,
            gaps: Vec::new(),
            gap_stats: gap::GapStats::default(),
            stats: MarketStats::default(),
        }
    }

    /// Open the observation window `[start_t_rx_ns, cutoff_t_rx_ns]`.
    pub fn start_observation(&mut self, start_t_rx_ns: u64, cutoff_t_rx_ns: u64) {
        self.observing = true;
        self.window = Some((start_t_rx_ns, cutoff_t_rx_ns));
    }

    pub fn stop_observation(&mut self) {
        self.observing = false;
    }

    pub fn is_observing(&self) -> bool {
        self.observing
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty()
    }

    pub fn mark_incomplete(&mut self, reason: &'static str) {
        self.incomplete_reasons.insert(reason);
    }

    pub fn coverage_complete(&self) -> bool {
        self.gaps.len() as u64 == self.gap_stats.gap_recovered
            && self.stats.gap_count == self.gaps.iter().map(|g| g.raw_failures).sum::<u64>()
            && !self.incomplete_reasons.contains("GAP_STATE_CAPACITY")
            && self.stats.ingress_overflow_count == 0
            && self.stats.fatal_count == 0
    }

    /// [r6] Truncated logs no longer fail by themselves: only unqualified or mismatching ones do
    /// (and any still awaiting qualification).
    pub fn decode_complete(&self) -> bool {
        self.stats.log_messages_missing == 0
            && self.stats.decoder_errors_successful_tx == 0
            && self.stats.logs_truncated_unqualified == 0
            && self.stats.logs_truncated_liquidation_mismatch == 0
            && self.truncated_pending.is_empty()
    }

    pub fn has_truncated_pending(&self) -> bool {
        !self.truncated_pending.is_empty()
    }

    pub(super) fn apply_truncation(&mut self, q: TruncationQualification) {
        match q {
            TruncationQualification::Qualified {
                instruction_liquidations,
            } => {
                self.stats.logs_truncated_qualified += 1;
                self.stats.truncated_instruction_end_liquidations +=
                    instruction_liquidations as u64;
            }
            TruncationQualification::Mismatch {
                instruction_liquidations,
            } => {
                self.stats.logs_truncated_liquidation_mismatch += 1;
                self.stats.truncated_instruction_end_liquidations +=
                    instruction_liquidations as u64;
                self.mark_incomplete("TRUNCATED_LIQUIDATION_MISMATCH");
            }
            TruncationQualification::Unqualified(code) => {
                self.stats.logs_truncated_unqualified += 1;
                *self.truncated_unqualified_reasons.entry(code).or_insert(0) += 1;
                self.mark_incomplete("LOGS_TRUNCATED_UNQUALIFIED");
            }
        }
    }

    /// Records a successful truncated transaction: counts and returns the logged pairs.
    pub(super) fn note_truncated(&mut self, decoded: &LogDecode) -> Vec<(Pubkey, Pubkey)> {
        self.stats.logs_truncated += 1;
        self.stats.truncated_log_end_liquidations += decoded.events.len() as u64;
        decoded
            .events
            .iter()
            .map(|e| (e.target, e.receiver))
            .collect()
    }

    /// [r6] Post-capture qualification of truncated `logs`-source transactions (never during
    /// capture). Same bounded fetch contract as enrichment; failures fail closed.
    pub fn qualify_truncated_deferred(
        &mut self,
        fetch: &mut dyn FnMut(&str) -> Result<Option<Value>, String>,
        wait: &mut dyn FnMut(Duration),
        budget_left: &mut dyn FnMut() -> bool,
    ) {
        let retry = Duration::from_millis(self.enrich_cfg.retry_ms);
        let max_attempts = self.enrich_cfg.max_attempts;
        while let Some(p) = self.truncated_pending.pop_front() {
            let mut q = TruncationQualification::Unqualified("FETCH_FAILED");
            for attempt in 1..=max_attempts {
                if !budget_left() {
                    q = TruncationQualification::Unqualified("PHASE_BUDGET_EXHAUSTED");
                    break;
                }
                self.stats.rpc_fallback_count += 1;
                if let Ok(Some(tx)) = fetch(&p.signature) {
                    q = qualify_truncated(tx, &p.signature, p.slot, &self.program, &p.logged);
                    break;
                }
                if attempt < max_attempts {
                    wait(retry);
                }
            }
            self.apply_truncation(q);
        }
    }

    /// Every still-pending truncated transaction fails closed with `code`.
    pub fn fail_truncated_pending(&mut self, code: &'static str) {
        while self.truncated_pending.pop_front().is_some() {
            self.apply_truncation(TruncationQualification::Unqualified(code));
        }
    }

    pub fn enrichment_complete(&self) -> bool {
        self.stats.enrichment_failed == 0
            && self.stats.enrichment_succeeded == self.stats.enrichment_requested
    }

    /// Inside the window? `None` (no timestamp, e.g. channel overflow) counts while observing.
    pub(super) fn in_window(&self, t_rx_ns: Option<u64>) -> bool {
        if !self.observing {
            return false;
        }
        match (self.window, t_rx_ns) {
            (Some((start, cutoff)), Some(t)) => t >= start && t <= cutoff,
            _ => true,
        }
    }

    /// Write a coverage event. GAP / RECONNECT / INGRESS_OVERFLOW / FATAL count only inside
    /// the observation window; outside they are recorded with `counted=false`.
    pub fn coverage(
        &mut self,
        sink: &mut dyn MarketSink,
        kind: &str,
        now_unix_s: u64,
        t_rx_ns: Option<u64>,
        detail: Value,
    ) -> std::io::Result<()> {
        let counted = self.in_window(t_rx_ns);
        if counted {
            match kind {
                "GAP" => {
                    self.stats.gap_count += 1;
                    self.record_gap(t_rx_ns, now_unix_s, &detail);
                    self.mark_incomplete("GAP");
                }
                "RECONNECT" => {
                    self.stats.reconnect_count += 1;
                    self.record_gap_reconnect(t_rx_ns, &detail);
                }
                "INGRESS_OVERFLOW" => {
                    self.stats.ingress_overflow_count += 1;
                    self.mark_incomplete("INGRESS_OVERFLOW");
                }
                "FATAL" => {
                    self.stats.fatal_count += 1;
                    self.mark_incomplete("FATAL");
                }
                _ => {}
            }
        }
        sink.coverage(&json!({
            "event": kind,
            "timestamp_utc": utc_iso(now_unix_s),
            "t_rx_ns": t_rx_ns,
            "observing": self.observing,
            "counted": counted,
            "detail": detail,
        }))
    }

    /// Slot notifications are telemetry; their spacing documents stream liveness.
    pub(super) fn on_slot_frame(&mut self, t_rx_ns: u64) {
        if !self.in_window(Some(t_rx_ns)) {
            return;
        }
        self.stats.slot_notifications += 1;
        if let Some(prev) = self.last_slot_frame_ns {
            let gap_ms = t_rx_ns.saturating_sub(prev) / 1_000_000;
            self.stats.max_stream_silence_ms = self.stats.max_stream_silence_ms.max(gap_ms);
        }
        self.last_slot_frame_ns = Some(t_rx_ns);
    }

    /// Dedupe + window + counters. `false` = not part of the measurement.
    pub(super) fn admit(
        &mut self,
        signature: &str,
        slot: u64,
        success: bool,
        t_rx_ns: u64,
    ) -> bool {
        if !self.observing {
            self.stats.events_outside_observation += 1;
            return false;
        }
        if let Some((start, cutoff)) = self.window {
            if t_rx_ns < start {
                self.stats.events_outside_observation += 1;
                return false;
            }
            if t_rx_ns > cutoff {
                self.stats.events_after_cutoff += 1;
                return false;
            }
        }
        let cap = self.econ.as_ref().map(|h| h.capacity()).unwrap_or(100_000);
        if self.truth.live_receipts.len() < cap || self.truth.live_receipts.contains_key(signature)
        {
            self.truth
                .live_receipts
                .entry(signature.to_owned())
                .or_insert((slot, t_rx_ns));
        } else {
            self.truth.retention_error = true;
        }
        self.admit_identity(signature, slot, success)
    }

    pub(super) fn admit_identity(&mut self, signature: &str, slot: u64, success: bool) -> bool {
        if self
            .econ
            .as_ref()
            .is_some_and(|h| self.seen.len() >= h.capacity() && !self.seen.contains(signature))
        {
            self.mark_incomplete("C0_E1_MARKET_RETENTION_OVERFLOW");
            self.econ
                .as_ref()
                .unwrap()
                .shared
                .retention_overflow
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return false;
        }
        self.stats.transactions_received += 1;
        if !self.seen.insert(signature.to_string()) {
            self.stats.duplicate_transactions += 1;
            return false;
        }
        self.stats.unique_transactions += 1;
        if success {
            self.stats.successful_transactions += 1;
        } else {
            self.stats.failed_transactions += 1;
        }
        self.stats.first_observed_slot =
            Some(self.stats.first_observed_slot.map_or(slot, |s| s.min(slot)));
        self.stats.last_observed_slot =
            Some(self.stats.last_observed_slot.map_or(slot, |s| s.max(slot)));
        true
    }

    pub(super) fn absorb_decode(&mut self, d: &LogDecode, success: bool) {
        self.stats.decoder_error_count += d.decoder_errors;
        self.stats.foreign_program_events += d.foreign_program_events;
        self.stats.base64_error_count += d.base64_errors;
        if success {
            if d.decoder_errors > 0 {
                self.stats.decoder_errors_successful_tx += d.decoder_errors;
                self.mark_incomplete("DECODER_ERROR");
            }
        } else {
            self.stats.failed_tx_receiver_events_ignored += d.events.len() as u64;
        }
    }

    pub(super) fn aggregate(
        &mut self,
        signature: &str,
        slot: u64,
        t_rx_ns: Option<u64>,
        transaction_index: Option<u32>,
        events: &[ReceiverEvent],
    ) {
        if self.liquidation_signatures.insert(signature.to_string()) {
            self.stats.liquidation_transactions += 1;
        }
        for ev in events {
            if let Some(h) = &self.econ {
                h.emit(crate::analytics::econ::Evidence::Market(
                    crate::analytics::econ::MarketEvent {
                        signature: signature.into(),
                        slot,
                        t_rx_ns,
                        source: if t_rx_ns.is_some() {
                            crate::analytics::econ::MarketEvidenceSource::Live
                        } else {
                            crate::analytics::econ::MarketEvidenceSource::GapBackfill
                        },
                        transaction_index,
                        event: ev.clone(),
                    },
                ));
            }
            if self
                .econ
                .as_ref()
                .is_some_and(|h| self.edges.len() >= h.capacity())
            {
                self.mark_incomplete("C0_E1_EVENT_RETENTION_OVERFLOW");
                self.econ
                    .as_ref()
                    .unwrap()
                    .shared
                    .retention_overflow
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                continue;
            }
            let edge = ev.gross_edge_usd();
            self.stats.liquidation_events += 1;
            self.stats.protocol_flat_fee_total_lamports += u64::from(ev.protocol_flat_fee_lamports);
            self.edges.push(edge);
            self.targets.insert(ev.target.to_string());
            let r = self
                .receivers
                .entry(ev.receiver.to_string())
                .or_insert((0, 0.0));
            r.0 += 1;
            r.1 += edge;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_liquidation_rows(
        &mut self,
        sink: &mut dyn MarketSink,
        signature: &str,
        slot: u64,
        t_rx_ns: Option<u64>,
        observed_unix_s: u64,
        events: &[ReceiverEvent],
        tx: Option<&Value>,
        enrichment_status: &str,
    ) -> std::io::Result<()> {
        let mut fee_counted = false;
        for ev in events {
            let e = tx.map(|t| enrich(t, &ev.receiver)).unwrap_or_default();
            if !fee_counted {
                fee_counted = true;
                match e.tx_fee_lamports {
                    Some(fee) => self.stats.tx_fee_total_lamports += fee,
                    None => self.stats.tx_fee_unknown_transactions += 1,
                }
            }
            let token_deltas: Vec<Value> = e
                .receiver_token_deltas
                .iter()
                .map(|d| json!({"mint": d.mint, "native_delta": d.native_delta.to_string(), "decimals": d.decimals}))
                .collect();
            sink.liquidation(&json!({
                "observed_at_utc": t_rx_ns.map(|_| utc_iso(observed_unix_s)),
                "t_rx_ns": t_rx_ns,
                "source": if t_rx_ns.is_some() { "live" } else { "gap_backfill" },
                "live_timestamp_qualified": t_rx_ns.is_some(),
                "slot": slot,
                "signature": signature,
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
                "enrichment_status": enrichment_status,
            }))?;
        }
        Ok(())
    }

    /// Route one ingress event (everything except the first StreamReady, which opens the window).
    pub fn on_ingress_event(
        &mut self,
        ev: IngressEvent,
        now_unix_s: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        let before = self.econ.as_ref().map(|_| self.stats.clone());
        let at = ev.t_rx_ns();
        let result = match ev {
            IngressEvent::Health(h) => self.coverage(
                sink,
                "TRANSPORT_HEALTH",
                now_unix_s,
                Some(h.t_rx_ns),
                json!({"connected":h.connected,"last_application_frame_age_ns":h.last_frame_age_ns,
                    "application_frames":h.frames,"last_slot":h.last_slot}),
            ),
            IngressEvent::Transport {
                state,
                code,
                attempt,
                last_slot,
                t_rx_ns,
            } => {
                if state == "RECOVERING" {
                    self.stats.transport_reconnect_attempts += 1;
                }
                if state == "RECOVERED" {
                    self.stats.transport_recovered += 1;
                }
                if code == "WSS_HTTP_429" {
                    self.stats.transport_http_429 += 1;
                }
                if state == "INCOMPLETE_ABORT" {
                    self.mark_incomplete("WSS_RECOVERY_EXHAUSTED");
                }
                self.coverage(sink, "TRANSPORT_STATE", now_unix_s, Some(t_rx_ns),
                    json!({"state":state,"code":code,"attempt":attempt,"last_valid_slot":last_slot}))
            }
            IngressEvent::Reconnected {
                attempt, t_rx_ns, ..
            } => self.coverage(
                sink,
                "RECONNECT",
                now_unix_s,
                Some(t_rx_ns),
                json!({"attempt": attempt}),
            ),
            IngressEvent::Gap {
                from_slot,
                to_slot,
                t_rx_ns,
                ..
            } => self.coverage(
                sink,
                "GAP",
                now_unix_s,
                Some(t_rx_ns),
                json!({"from_slot": from_slot, "to_slot": to_slot}),
            ),
            IngressEvent::Fatal {
                detail, t_rx_ns, ..
            } => self.coverage(
                sink,
                "FATAL",
                now_unix_s,
                Some(t_rx_ns),
                json!({"detail": detail}),
            ),
            IngressEvent::Transaction(t) if self.source == MarketSource::Transactions => {
                self.on_transaction(&t, now_unix_s, sink)
            }
            IngressEvent::Logs(l) if self.source == MarketSource::Logs => {
                self.on_logs(&l, now_unix_s, sink)
            }
            IngressEvent::Slot(s) => {
                let progress = self
                    .reconciliation
                    .market_slot_end
                    .as_ref()
                    .is_none_or(|b| s.slot > b.slot);
                self.record_slot_boundary(s.slot, s.t_rx_ns, now_unix_s, sink)?;
                self.close_gap_ranges(s.slot, s.t_rx_ns, now_unix_s, sink)?;
                if let Some(h) = &self.econ {
                    h.emit(crate::analytics::econ::Evidence::Slot {
                        slot: s.slot,
                        t_rx_ns: s.t_rx_ns,
                    });
                }
                if progress && self.in_window(Some(s.t_rx_ns)) {
                    self.last_slot_progress_ns = Some(s.t_rx_ns);
                }
                self.on_slot_frame(s.t_rx_ns);
                Ok(())
            }
            _ => Ok(()),
        };
        if let (Some(h), Some(before)) = (&self.econ, before) {
            h.emit(crate::analytics::econ::Evidence::MarketCounters {
                at_ns: at,
                before,
                after: Box::new(self.stats.clone()),
            });
        }
        result
    }

    /// `transactions` source: the full payload is the primary raw evidence; no RPC.
    pub fn on_transaction(
        &mut self,
        t: &TransactionObserved,
        now_unix_s: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        let success = t.err.is_none();
        if !self.admit(&t.signature, t.slot, success, t.t_rx_ns) {
            return Ok(());
        }
        sink.transaction(&json!({
            "observed_at_utc": utc_iso(now_unix_s),
            "t_rx_ns": t.t_rx_ns,
            "slot": t.slot,
            "signature": t.signature,
            "tx_version": format!("{:?}", t.version),
            "success": success,
            "err": t.err,
            "source": "transactionSubscribe",
            "raw": t.raw,
        }))?;
        let tx = t.raw.get("transaction");
        let logs: Option<Vec<String>> = tx
            .and_then(|x| x.pointer("/meta/logMessages"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| l.as_str().map(str::to_string))
                    .collect()
            });
        let Some(logs) = logs else {
            if success {
                self.stats.log_messages_missing += 1;
                self.mark_incomplete("LOG_MESSAGES_MISSING");
            }
            return Ok(());
        };
        let decoded = decode_receiver_events(&logs, &self.program);
        self.absorb_decode(&decoded, success);
        if success && decoded.truncated {
            // [r6] the payload already carries the full transaction: qualify immediately, no RPC.
            let logged = self.note_truncated(&decoded);
            let q = match tx {
                Some(Value::Object(obj)) => {
                    let mut with_slot = obj.clone();
                    with_slot.insert("slot".to_string(), json!(t.slot));
                    qualify_truncated(
                        Value::Object(with_slot),
                        &t.signature,
                        t.slot,
                        &self.program,
                        &logged,
                    )
                }
                _ => TruncationQualification::Unqualified("TX_DECODE"),
            };
            self.apply_truncation(q);
        }
        if !success || decoded.events.is_empty() {
            return Ok(());
        }
        self.aggregate(
            &t.signature,
            t.slot,
            Some(t.t_rx_ns),
            t.tx_index_in_slot,
            &decoded.events,
        );
        self.stats.enrichment_requested += 1;
        let meta_available = tx.and_then(|x| x.get("meta")).is_some_and(Value::is_object);
        let status = if meta_available {
            self.stats.enrichment_succeeded += 1;
            "PAYLOAD"
        } else {
            self.stats.enrichment_failed += 1;
            "PAYLOAD_META_MISSING"
        };
        self.write_liquidation_rows(
            sink,
            &t.signature,
            t.slot,
            Some(t.t_rx_ns),
            now_unix_s,
            &decoded.events,
            tx,
            status,
        )
    }

    /// `logs` source: economics from logs immediately; enrichment deferred until after capture.
    pub fn on_logs(
        &mut self,
        l: &LogsObserved,
        now_unix_s: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        let success = l.err.is_none();
        if !self.admit(&l.signature, l.slot, success, l.t_rx_ns) {
            return Ok(());
        }
        sink.transaction(&json!({
            "observed_at_utc": utc_iso(now_unix_s),
            "t_rx_ns": l.t_rx_ns,
            "slot": l.slot,
            "signature": l.signature,
            "tx_version": Value::Null,
            "success": success,
            "err": l.err,
            "source": "logsSubscribe",
            "raw": l.raw,
        }))?;
        // A timeboxed diagnostic can be terminated before graceful finalization.
        // Make each admitted live signature visible on disk immediately rather
        // than letting the final BufWriter batch disappear on SIGTERM.
        sink.flush()?;
        if l.logs.is_empty() {
            if success {
                self.stats.log_messages_missing += 1;
                self.mark_incomplete("LOG_MESSAGES_MISSING");
            }
            return Ok(());
        }
        let decoded = decode_receiver_events(&l.logs, &self.program);
        self.absorb_decode(&decoded, success);
        if success {
            for ev in &decoded.additional_events {
                match ev.kind {
                    truth::LiquidationType::Classic => self.stats.provisional_classic_events += 1,
                    truth::LiquidationType::ForcedDeleverage => {
                        self.stats.provisional_deleverage_events += 1
                    }
                    truth::LiquidationType::BankruptcySettlement => {
                        self.stats.provisional_bankruptcy_events += 1
                    }
                    truth::LiquidationType::Receivership => {
                        unreachable!("original receiver decoder handles this type")
                    }
                }
                eprintln!(
                    "[MARKET/LIQUIDATION] provisional=true type={} slot={} signature={} target={} actor={} gross_usd={}",
                    ev.kind.as_str(), l.slot, l.signature, ev.target, ev.actor,
                    ev.gross_edge_usd.map(|v| format!("{v:.6}")).unwrap_or_else(|| "UNKNOWN".to_owned())
                );
            }
        }
        if success && decoded.truncated {
            // [r6] qualified after capture against the fetched transaction (bounded, counted).
            let logged = self.note_truncated(&decoded);
            if self.truncated_pending.len() >= self.enrich_cfg.max_pending {
                self.apply_truncation(TruncationQualification::Unqualified("QUEUE_FULL"));
            } else {
                self.truncated_pending.push_back(TruncatedPending {
                    signature: l.signature.clone(),
                    slot: l.slot,
                    logged,
                });
            }
        }
        if !success || decoded.events.is_empty() {
            return Ok(());
        }
        self.aggregate(&l.signature, l.slot, Some(l.t_rx_ns), None, &decoded.events);
        // The accounting ledger remains deferred for exact post-capture enrichment.
        // The operator must still see every successfully decoded live liquidation NOW.
        for ev in &decoded.events {
            eprintln!(
                "[MARKET/RECEIVERSHIP] provisional=true slot={} signature={} event={} target={} receiver={} gross_usd={:.6}",
                l.slot, l.signature, ev.event_index, ev.target, ev.receiver, ev.gross_edge_usd()
            );
        }
        self.stats.enrichment_requested += 1;
        if self.deferred.len() >= self.enrich_cfg.max_pending {
            self.stats.enrichment_failed += 1;
            self.stats.enrichment_skipped_queue_full += 1;
            return self.write_liquidation_rows(
                sink,
                &l.signature,
                l.slot,
                Some(l.t_rx_ns),
                now_unix_s,
                &decoded.events,
                None,
                "SKIPPED_QUEUE_FULL",
            );
        }
        self.deferred.push_back(DeferredLiquidation {
            signature: l.signature.clone(),
            slot: l.slot,
            t_rx_ns: l.t_rx_ns,
            observed_unix_s: now_unix_s,
            events: decoded.events,
        });
        Ok(())
    }

    /// Post-capture enrichment of every deferred liquidation. Never runs during capture.
    /// `fetch` = one `getTransaction` attempt, `wait` = pause between attempts,
    /// `budget_left` = phase time budget. Captured gross edge is never removed.
    pub fn enrich_deferred(
        &mut self,
        sink: &mut dyn MarketSink,
        fetch: &mut dyn FnMut(&str) -> Result<Option<Value>, String>,
        wait: &mut dyn FnMut(Duration),
        budget_left: &mut dyn FnMut() -> bool,
    ) -> std::io::Result<()> {
        let retry = Duration::from_millis(self.enrich_cfg.retry_ms);
        let max_attempts = self.enrich_cfg.max_attempts;
        while let Some(p) = self.deferred.pop_front() {
            let mut status = "UNRESOLVED";
            let mut resolved: Option<Value> = None;
            for attempt in 1..=max_attempts {
                if !budget_left() {
                    status = "SKIPPED_PHASE_TIMEOUT";
                    break;
                }
                self.stats.rpc_fallback_count += 1;
                if let Ok(Some(tx)) = fetch(&p.signature) {
                    if verify_tx_identity(&tx, &p.signature, p.slot).is_ok() {
                        resolved = Some(tx);
                    } else {
                        status = "IDENTITY_MISMATCH";
                    }
                    break;
                }
                if attempt < max_attempts {
                    wait(retry);
                }
            }
            match resolved {
                Some(tx) => {
                    self.stats.enrichment_succeeded += 1;
                    self.write_liquidation_rows(
                        sink,
                        &p.signature,
                        p.slot,
                        Some(p.t_rx_ns),
                        p.observed_unix_s,
                        &p.events,
                        Some(&tx),
                        "RPC_GET_TRANSACTION",
                    )?;
                }
                None => {
                    self.stats.enrichment_failed += 1;
                    self.write_liquidation_rows(
                        sink,
                        &p.signature,
                        p.slot,
                        Some(p.t_rx_ns),
                        p.observed_unix_s,
                        &p.events,
                        None,
                        status,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Phase 3 entry point. Never fails the run because of the enrichment client: an init error
    /// is recorded (coverage + summary) and all deferred liquidations are written unenriched, so
    /// captured gross edge and the final report are always produced.
    pub fn enrichment_phase(
        &mut self,
        sink: &mut dyn MarketSink,
        fetcher: EnrichmentFetcher<'_>,
        wait: &mut dyn FnMut(Duration),
        budget_left: &mut dyn FnMut() -> bool,
        now_unix_s: u64,
    ) -> std::io::Result<()> {
        match fetcher {
            Ok(fetch) => self.enrich_deferred(sink, fetch, wait, budget_left),
            Err(e) => {
                self.coverage(
                    sink,
                    "ENRICHMENT_INIT_FAILED",
                    now_unix_s,
                    None,
                    json!({"error": e}),
                )?;
                self.enrichment_init_error = Some(e);
                self.write_deferred_unenriched(sink, "ENRICHMENT_INIT_FAILED")
            }
        }
    }

    /// Write every still-deferred liquidation without enrichment (economics already counted).
    pub fn write_deferred_unenriched(
        &mut self,
        sink: &mut dyn MarketSink,
        status: &str,
    ) -> std::io::Result<()> {
        while let Some(p) = self.deferred.pop_front() {
            self.stats.enrichment_failed += 1;
            self.write_liquidation_rows(
                sink,
                &p.signature,
                p.slot,
                Some(p.t_rx_ns),
                p.observed_unix_s,
                &p.events,
                None,
                status,
            )?;
        }
        Ok(())
    }

    /// Ranking by liquidation events desc, then gross edge desc, then receiver asc.
    pub fn receiver_ranking(&self) -> Vec<ReceiverRank> {
        let mut v: Vec<ReceiverRank> = self
            .receivers
            .iter()
            .map(|(r, (n, g))| ReceiverRank {
                receiver: r.clone(),
                liquidation_events: *n,
                gross_edge_total_usd: *g,
            })
            .collect();
        v.sort_by(|a, b| {
            b.liquidation_events
                .cmp(&a.liquidation_events)
                .then_with(|| b.gross_edge_total_usd.total_cmp(&a.gross_edge_total_usd))
                .then_with(|| a.receiver.cmp(&b.receiver))
        });
        v
    }

    pub fn summary(&self, cfg: &SummaryContext) -> Value {
        let s = &self.stats;
        let observed_end = self.window.map(|(start, cutoff)| {
            start
                .saturating_add((cfg.actual_observation_seconds.max(0.0) * 1e9) as u64)
                .min(cutoff)
        });
        let outage = observed_end.and_then(|end| self.transport_outage_ns(end));
        let mut sorted = self.edges.clone();
        sorted.sort_by(f64::total_cmp);
        let total: f64 = sorted.iter().sum();
        let n = sorted.len();
        let hours = cfg.actual_observation_seconds / 3_600.0;
        let per_hour = |x: f64| if hours > 0.0 { Some(x / hours) } else { None };
        let ranking = self.receiver_ranking();
        let total_events = s.liquidation_events as f64;
        let mut by_gross = ranking.clone();
        by_gross.sort_by(|a, b| {
            b.gross_edge_total_usd
                .total_cmp(&a.gross_edge_total_usd)
                .then_with(|| a.receiver.cmp(&b.receiver))
        });
        let top = ranking.first();
        let top3_events: u64 = ranking.iter().take(3).map(|r| r.liquidation_events).sum();
        let top3_gross: f64 = by_gross
            .iter()
            .take(3)
            .map(|r| r.gross_edge_total_usd)
            .sum();
        let coverage_complete = self.coverage_complete();
        let decode_complete = self.decode_complete();
        let receivership_transport_capture_complete = coverage_complete
            && decode_complete
            && cfg.full_duration_observed
            && cfg.drain_complete;
        // Master contract: transport completeness is no longer whole-market proof.
        // Until the full-slot/all-liquidation-path engine is qualified, fail closed.
        let full_slot_reconciliation_complete = self.reconciliation.complete;
        let liquidation_path_coverage_complete = paths::verified(
            self.deployment_before.as_ref(),
            self.deployment_after.as_ref(),
            self.reconciliation.market_slot_end.as_ref().map(|b| b.slot),
        );
        let market_capture_complete = cfg.stream_ready
            && cfg.full_duration_observed
            && cfg.drain_complete
            && full_slot_reconciliation_complete
            && !self.truth.retention_error
            && liquidation_path_coverage_complete;
        let source_coverage_verified =
            full_slot_reconciliation_complete && liquidation_path_coverage_complete;
        let enrichment_complete = self.enrichment_complete();
        let mut reasons: Vec<&str> = self.incomplete_reasons.iter().copied().collect();
        if !full_slot_reconciliation_complete {
            reasons.push(
                self.reconciliation
                    .error
                    .unwrap_or("FULL_SLOT_RECONCILIATION_PENDING"),
            );
        }
        if !liquidation_path_coverage_complete {
            reasons.push(
                self.deployment_error
                    .unwrap_or("AUDITED_DEPLOYMENT_CONTINUITY_UNPROVEN"),
            );
        }
        if cfg.stream_ready
            && !cfg.full_duration_observed
            && !reasons.contains(&"OBSERVATION_ENDED_EARLY")
        {
            reasons.push("OBSERVATION_ENDED_EARLY");
        }
        if cfg.stream_ready && !cfg.drain_complete && !reasons.contains(&"DRAIN_INCOMPLETE") {
            reasons.push("DRAIN_INCOMPLETE");
        }
        let verdict = market_verdict(
            cfg.stream_ready,
            market_capture_complete,
            source_coverage_verified,
        );
        let avg = if n > 0 { Some(total / n as f64) } else { None };
        let sample_status = if s.liquidation_events < cfg.min_sample_liquidations {
            "INSUFFICIENT_SAMPLE"
        } else {
            "SAMPLE_SUFFICIENT"
        };
        let receivers: Vec<Value> = ranking
            .iter()
            .map(|r| {
                json!({
                    "receiver": r.receiver,
                    "liquidation_events": r.liquidation_events,
                    "gross_edge_total_usd": r.gross_edge_total_usd,
                    "liquidation_share": ratio(r.liquidation_events as f64, total_events),
                    "gross_edge_share": ratio(r.gross_edge_total_usd, total),
                })
            })
            .collect();
        json!({
            "title": "PROJECT 0 MARKET OBSERVER — FINAL",
            "run_id": cfg.run_id,
            "source": self.source.as_str(),
            "commitment": cfg.commitment,
            "program": self.program.to_string(),
            "process_started_at": cfg.process_started_at,
            "stream_ready_at": cfg.stream_ready_at,
            "observation_started_at": cfg.observation_started_at,
            "observation_ended_at": cfg.observation_ended_at,
            "configured_duration_seconds": cfg.configured_duration_seconds,
            "actual_observation_seconds": cfg.actual_observation_seconds,
            "verdict": verdict,
            "market_capture_complete": market_capture_complete,
            "receivership_transport_capture_complete": receivership_transport_capture_complete,
            "full_slot_reconciliation_complete": full_slot_reconciliation_complete,
            "full_slot_reconciliation": self.reconciliation,
            "canonical_market_truth": self.truth.summary(self.reconciliation.first_block_time.zip(self.reconciliation.last_block_time).map(|(a,b)|(b-a) as f64)),
            "liquidation_path_coverage_complete": liquidation_path_coverage_complete,
            "liquidation_count_scope": "RECEIVERSHIP_ONLY_NOT_TOTAL_MARGINFI_MARKET",
            "live_source_coverage_verified": self.source.coverage_verified(),
            "deployment_continuity":{"before":self.deployment_before,"after":self.deployment_after,"error":self.deployment_error,"source_pin":"33c67987a661f59c6683a65485ebb4f0ef030b77","bytecode_source_equivalence":"NOT_ATTESTED; pinned source is contract authority; hash continuity against existing upgrade-watch reference"},
            "completeness_authority":"FULL_FINALIZED_SLOT_RECONCILIATION_AND_PATH_QUALIFICATION",
            "transport_issues_are_telemetry":true,
            "transport_incomplete_reasons":self.incomplete_reasons,
            "coverage_complete": coverage_complete,
            "decode_complete": decode_complete,
            "drain_complete": cfg.drain_complete,
            "live_coverage_status": if receivership_transport_capture_complete { "LIVE_COVERAGE_COMPLETE" } else { "LIVE_COVERAGE_INCOMPLETE" },
            "capture_clock_complete": cfg.stream_ready && cfg.full_duration_observed,
            "verification_status": if full_slot_reconciliation_complete { "CANONICAL_VERIFIED" } else if self.reconciliation.attempted { "HISTORICAL_INCOMPLETE" } else { "NOT_RUN" },
            "source_coverage_verified": source_coverage_verified,
            "source_coverage_note": if source_coverage_verified {"Finalized full-slot scan is authoritative; live WSS source completeness remains separately unverified"} else {self.source.coverage_note()},
            "incomplete_reasons": reasons,
            "first_observed_slot": s.first_observed_slot,
            "last_observed_slot": s.last_observed_slot,
            "transactions_received": s.transactions_received,
            "unique_transactions": s.unique_transactions,
            "duplicate_transactions": s.duplicate_transactions,
            "successful_transactions": s.successful_transactions,
            "failed_transactions": s.failed_transactions,
            "liquidation_transactions": s.liquidation_transactions,
            "liquidation_events": s.liquidation_events,
            "liquidations_per_hour": per_hour(total_events),
            "gross_edge_total_usd": total,
            "gross_edge_per_hour_usd": per_hour(total),
            "gross_edge_avg_usd": avg,
            "gross_edge_per_liquidation_usd": avg,
            "sample_status": sample_status,
            "sample_min_liquidation_events": cfg.min_sample_liquidations,
            "unique_targets": self.targets.len(),
            "enrichment_available": cfg.enrichment_available,
            "gross_edge_median_usd": quantile_nearest_rank(&sorted, 0.5),
            "gross_edge_p90_usd": quantile_nearest_rank(&sorted, 0.9),
            "gross_edge_p99_usd": quantile_nearest_rank(&sorted, 0.99),
            "gross_edge_max_usd": sorted.last().copied(),
            "gross_edge_min_usd": sorted.first().copied(),
            "quantile_method": QUANTILE_METHOD,
            "protocol_flat_fee_total_lamports": s.protocol_flat_fee_total_lamports,
            "tx_fee_total_lamports": s.tx_fee_total_lamports,
            "tx_fee_unknown_transactions": s.tx_fee_unknown_transactions,
            "fee_totals_complete": enrichment_complete && s.tx_fee_unknown_transactions == 0,
            "enrichment_complete": enrichment_complete,
            "enrichment_requested": s.enrichment_requested,
            "enrichment_succeeded": s.enrichment_succeeded,
            "enrichment_failed": s.enrichment_failed,
            "enrichment_skipped_queue_full": s.enrichment_skipped_queue_full,
            "enrichment_init_error": self.enrichment_init_error,
            "rpc_fallback_count": s.rpc_fallback_count,
            "unique_receivers": ranking.len(),
            "top_receiver": top.map(|r| r.receiver.clone()),
            "top_receiver_liquidation_share": top.and_then(|r| ratio(r.liquidation_events as f64, total_events)),
            "top_receiver_gross_edge_share": top.and_then(|r| ratio(r.gross_edge_total_usd, total)),
            "top3_receiver_liquidation_share": ratio(top3_events as f64, total_events),
            "top3_receiver_gross_edge_share": ratio(top3_gross, total),
            "gap_count": s.gap_count,
            "gap_recovery_attempted": self.gap_stats.gap_recovery_attempted,
            "gap_recovered": self.gap_stats.gap_recovered,
            "gap_unrecovered": (self.gaps.len() as u64).saturating_sub(self.gap_stats.gap_recovered),
            "gap_intervals": self.gaps.len(),
            "coverage_outage_seconds": outage.map(|ns| ns as f64/1e9),
            "transport_covered_seconds": outage.map(|ns| (cfg.actual_observation_seconds-ns as f64/1e9).max(0.0)),
            "transport_coverage_provenance":"LOCAL_SLOT_PROGRESS_AND_JOURNALED_OUTAGE_INTERVALS_NOT_SOURCE_COMPLETENESS",
            "last_live_slot_age_s": observed_end.zip(self.last_slot_progress_ns).map(|(end,last)| end.saturating_sub(last) as f64/1e9),
            "gap_count_raw": s.gap_count,
            "gaps_merged": self.gaps.iter().map(|g| g.raw_failures.saturating_sub(1)).sum::<u64>(),
            "provider_budget": darkbot_solana::provider::shared().snapshot(),
            "gap_slots_requested": self.gap_stats.gap_slots_requested,
            "gap_slots_fetched": self.gap_stats.gap_slots_fetched,
            "gap_slots_skipped_confirmed": self.gap_stats.gap_slots_skipped_confirmed,
            "gap_slots_failed": self.gap_stats.gap_slots_failed,
            "gap_transactions_scanned": self.gap_stats.gap_transactions_scanned,
            "gap_transactions_deduplicated": self.gap_stats.gap_transactions_deduplicated,
            "gap_transactions_recovered": self.gap_stats.gap_transactions_recovered,
            "gap_liquidation_transactions": self.gap_stats.gap_liquidation_transactions,
            "gap_liquidation_events": self.gap_stats.gap_liquidation_events,
            "gaps": self.gaps,
            "reconnect_count": s.reconnect_count,
            "reconnect_attempts": s.transport_reconnect_attempts,
            "reconnect_successful": s.transport_recovered,
            "wss_http_429_count": s.transport_http_429,
            "ingress_overflow_count": s.ingress_overflow_count,
            "fatal_count": s.fatal_count,
            "decoder_error_count": s.decoder_error_count,
            "decoder_errors_successful_tx": s.decoder_errors_successful_tx,
            "base64_error_count": s.base64_error_count,
            "foreign_program_events": s.foreign_program_events,
            "failed_tx_receiver_events_ignored": s.failed_tx_receiver_events_ignored,
            "logs_truncated": s.logs_truncated,
            "logs_truncated_total": s.logs_truncated,
            "logs_truncated_qualified": s.logs_truncated_qualified,
            "logs_truncated_unqualified": s.logs_truncated_unqualified,
            "logs_truncated_liquidation_mismatch": s.logs_truncated_liquidation_mismatch,
            "truncated_instruction_end_liquidations": s.truncated_instruction_end_liquidations,
            "truncated_log_end_liquidations": s.truncated_log_end_liquidations,
            "truncated_unqualified_reasons": self.truncated_unqualified_reasons,
            "log_messages_missing": s.log_messages_missing,
            "events_outside_observation": s.events_outside_observation,
            "events_after_cutoff": s.events_after_cutoff,
            "slot_notifications": s.slot_notifications,
            "max_stream_silence_ms": s.max_stream_silence_ms,
            "receivers": receivers,
            "non_claims": [
                "market gross edge only (seized - repaid, USD, protocol valuation)",
                "not DarkBot executable edge (P1-C/F1/F8)",
                "not DarkBot capture rate",
                "not DarkBot realized PnL (P8)",
                "no per-day/month/year extrapolation from a single run",
            ],
        })
    }
}
