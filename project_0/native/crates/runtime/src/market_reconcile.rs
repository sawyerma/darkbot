//! Canonical finalized block reconciliation, invoked only after capture and drain.
//! Slot notifications define an explicit slot-complete window; they are NOT finality proof.
use super::*;
use crate::winner::{resolve_account_keys, ChainInstruction, ChainTx, TxStatus};
use darkbot_solana::chain::TxWireVersion;
use serde::Serialize;

#[derive(Clone, Copy, Debug)]
pub struct ReconcileConfig {
    pub max_slots: u64,
    pub max_transactions: u64,
    pub max_attempts: u32,
    pub timeout_ms: u64,
    pub retry_ms: u64,
    pub phase_max_secs: u64,
    pub finality_wait_secs: u64,
    pub requests_per_second: u64,
}
impl Default for ReconcileConfig {
    fn default() -> Self {
        Self {
            max_slots: 100_000,
            max_transactions: 200_000_000,
            max_attempts: 5,
            timeout_ms: 10_000,
            retry_ms: 250,
            phase_max_secs: 14_400,
            finality_wait_secs: 60,
            requests_per_second: 5,
        }
    }
}
impl ReconcileConfig {
    pub fn from_lookup(get: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        let n = |key: &str, default: u64, max: u64| -> Result<u64, String> {
            let v = get(key)
                .map(|v| v.parse::<u64>())
                .transpose()
                .map_err(|_| format!("invalid {key}"))?
                .unwrap_or(default);
            if v == 0 || v > max {
                return Err(format!("out of range {key}"));
            }
            Ok(v)
        };
        let d = Self::default();
        Ok(Self {
            max_slots: n("P0_MARKET_RECONCILE_MAX_SLOTS", d.max_slots, 100_000)?,
            max_transactions: n(
                "P0_MARKET_RECONCILE_MAX_TRANSACTIONS",
                d.max_transactions,
                500_000_000,
            )?,
            max_attempts: n(
                "P0_MARKET_RECONCILE_MAX_ATTEMPTS",
                d.max_attempts as u64,
                10,
            )? as u32,
            timeout_ms: n("P0_MARKET_RECONCILE_TIMEOUT_MS", d.timeout_ms, 60_000)?,
            retry_ms: n("P0_MARKET_RECONCILE_RETRY_MS", d.retry_ms, 10_000)?,
            phase_max_secs: n(
                "P0_MARKET_RECONCILE_PHASE_MAX_SECS",
                d.phase_max_secs,
                u64::from(u32::MAX),
            )?,
            requests_per_second: n(
                "P0_MARKET_RECONCILE_REQUESTS_PER_SECOND",
                d.requests_per_second,
                20,
            )?,
            finality_wait_secs: n(
                "P0_MARKET_RECONCILE_FINALITY_WAIT_SECS",
                d.finality_wait_secs,
                300,
            )?,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct SlotBoundary {
    pub slot: u64,
    pub t_rx_ns: u64,
    pub observed_unix_s: u64,
    pub provenance: &'static str,
}
#[derive(Default, Debug, Serialize)]
pub(super) struct Reconciliation {
    pub market_slot_start: Option<SlotBoundary>,
    pub market_slot_end: Option<SlotBoundary>,
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
    pub error: Option<&'static str>,
}
type ReconcileRpc<'a> = dyn FnMut(&str, Value, Duration) -> Result<Value, &'static str> + 'a;

/// Validate ALL key/index views before deciding relevance, including inner instructions.
/// Legacy/v0 and the JSON v1 account/instruction view are covered by real F8 fixtures.
pub(super) fn validate_full_transaction(raw: &Value) -> Result<ChainTx, &'static str> {
    let tx = darkbot_solana::chain::decode_transaction(raw).map_err(|e| e.0)?;
    if !matches!(
        tx.version,
        Some(TxWireVersion::Legacy) | Some(TxWireVersion::Number(0 | 1))
    ) {
        return Err("TX_VERSION_UNSUPPORTED_OR_MISSING");
    }
    match tx.status() {
        TxStatus::Success | TxStatus::Failed(_) => {}
        TxStatus::Unknown("TX_META_MISSING") => return Err("TX_META_STATUS_MISSING"),
        TxStatus::Unknown(code) => return Err(code),
    }
    let signature = tx.signature().ok_or("TX_SIGNATURE_MISSING")?;
    check_transaction_identity(&tx, signature, tx.slot).map_err(|_| "TX_IDENTITY")?;
    let keys = resolve_account_keys(&tx).map_err(|e| e.code())?;
    // A present but empty loadedAddresses object must not disguise an unresolved LUT.
    // (Lookup keys and u8 indexes are validated by the typed wire decode.)
    if let Some(lookups) = tx
        .message
        .address_table_lookups
        .as_ref()
        .filter(|l| !l.is_empty())
    {
        let wanted = lookups.iter().fold([0usize; 2], |w, l| {
            [
                w[0] + l.writable_indexes.len(),
                w[1] + l.readonly_indexes.len(),
            ]
        });
        let loaded = tx
            .meta
            .as_ref()
            .and_then(|m| m.loaded_addresses.as_ref())
            .ok_or("LOADED_ADDRESSES_MISSING")?;
        if wanted != [loaded.writable.len(), loaded.readonly.len()] {
            return Err("LOADED_ADDRESS_COUNT_MISMATCH");
        }
    }
    let valid_ix = |ix: &ChainInstruction| -> Result<(), &'static str> {
        if usize::from(ix.program_id_index) >= keys.len()
            || ix.accounts.iter().any(|i| usize::from(*i) >= keys.len())
        {
            return Err("INSTRUCTION_INDEX_OUT_OF_RANGE");
        }
        Ok(())
    };
    for ix in &tx.message.instructions {
        valid_ix(ix)?;
    }
    // None means recording was unavailable. Even an otherwise irrelevant transaction
    // cannot establish complete CPI coverage in that case.
    let inner = tx
        .meta
        .as_ref()
        .and_then(|m| m.inner_instructions.as_ref())
        .ok_or("INNER_INSTRUCTIONS_UNAVAILABLE")?;
    let mut indices = HashSet::new();
    for group in inner {
        if usize::from(group.index) >= tx.message.instructions.len() || !indices.insert(group.index)
        {
            return Err("INNER_INDEX");
        }
        for ix in &group.instructions {
            valid_ix(ix)?;
        }
    }
    Ok(tx)
}

impl ObserverCore {
    pub(super) fn record_slot_boundary(
        &mut self,
        slot: u64,
        at: u64,
        now: u64,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        if !self.in_window(Some(at)) {
            return Ok(());
        }
        let r = &mut self.reconciliation;
        // Monotonic max prevents a delayed/out-of-order notification shrinking the window.
        let b = SlotBoundary {
            slot,
            t_rx_ns: at,
            observed_unix_s: now,
            provenance: "IN_WINDOW_SLOT_SUBSCRIBE_PROCESSED",
        };
        if r.market_slot_start.is_none() {
            r.market_slot_start = Some(b.clone());
        }
        if r.market_slot_end
            .as_ref()
            .is_none_or(|old| slot >= old.slot)
        {
            r.market_slot_end = Some(b);
        }
        sink.coverage(&json!({"event":"MARKET_SLOT_BOUNDARY_EVIDENCE", "slot":slot,
            "t_rx_ns":at,"timestamp_utc":utc_iso(now),"detail":{
                "market_slot_start":r.market_slot_start,"market_slot_end":r.market_slot_end,
                "window_semantics":"inclusive whole slots from first to highest in-window slot notification; partial temporal boundary slots included",
                "finality_proven":false}}))
    }

    pub(super) fn reconcile_slots(
        &mut self,
        rpc: &mut ReconcileRpc<'_>,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        if self.observing {
            return Err(std::io::Error::other("RECONCILIATION_DURING_CAPTURE"));
        }
        if self.reconciliation.attempted {
            return Ok(());
        }
        self.reconciliation.attempted = true;
        let cfg = self.reconcile_cfg;
        let deadline = Instant::now() + Duration::from_secs(cfg.phase_max_secs);
        let mut request = |method: &str, params: Value| -> Result<Value, &'static str> {
            let mut result = Err("RECONCILIATION_DEADLINE");
            for attempt in 0..cfg.max_attempts {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err("RECONCILIATION_DEADLINE");
                }
                result = rpc(
                    method,
                    params.clone(),
                    remaining.min(Duration::from_millis(cfg.timeout_ms)),
                );
                if Instant::now() >= deadline {
                    return Err("RECONCILIATION_DEADLINE");
                }
                if result.is_ok() {
                    break;
                }
                if attempt + 1 < cfg.max_attempts {
                    std::thread::sleep(
                        retry_delay(result.as_ref().err().copied(), attempt, cfg.retry_ms)
                            .min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
            }
            result
        };
        let result = self.reconcile_range(&mut request, deadline, sink)?;
        self.reconciliation.error = result.err();
        self.reconciliation.slots_unresolved = self.reconciliation.slots_expected.saturating_sub(
            self.reconciliation.slots_fetched + self.reconciliation.slots_skipped_confirmed,
        );
        self.reconciliation.complete =
            self.reconciliation.error.is_none() && self.reconciliation.slots_unresolved == 0;
        sink.coverage(
            &json!({"event":"FULL_SLOT_RECONCILIATION_FINAL", "detail":self.reconciliation}),
        )?;
        Ok(())
    }

    fn reconcile_range(
        &mut self,
        request: &mut dyn FnMut(&str, Value) -> Result<Value, &'static str>,
        deadline: Instant,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<Result<(), &'static str>> {
        macro_rules! require {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => return Ok(Err(e)),
                }
            };
        }
        let r = &mut self.reconciliation;
        let (Some(lo), Some(hi)) = (&r.market_slot_start, &r.market_slot_end) else {
            return Ok(Err("CANONICAL_SLOT_BOUNDARY_MISSING"));
        };
        let (lo, hi) = (lo.slot, hi.slot);
        let Some(n) = hi.checked_sub(lo).and_then(|d| d.checked_add(1)) else {
            return Ok(Err("CANONICAL_SLOT_RANGE_INVALID"));
        };
        r.slots_expected = n;
        if n > self.reconcile_cfg.max_slots {
            return Ok(Err("RECONCILIATION_SLOT_BUDGET"));
        }
        let finality_deadline = (Instant::now()
            + Duration::from_secs(self.reconcile_cfg.finality_wait_secs))
        .min(deadline);
        let tip = loop {
            let tip = require!(request("getSlot", json!([{"commitment":"finalized"}]))
                .and_then(|v| v.as_u64().ok_or("FINALIZED_TIP_INVALID")));
            if tip >= hi {
                break tip;
            }
            if Instant::now() >= finality_deadline {
                return Ok(Err("WINDOW_NOT_FINALIZED"));
            }
            std::thread::sleep(
                Duration::from_secs(1)
                    .min(finality_deadline.saturating_duration_since(Instant::now())),
            );
        };
        // Request a bounded successor beyond hi so a trailing skipped slot can be proven
        // by a finalized parent link, never by absence from getBlocks alone.
        let search_end = hi.saturating_add(512).min(tip);
        let list = require!(request(
            "getBlocks",
            json!([lo,search_end,{"commitment":"finalized"}])
        ));
        let produced = require!(list.as_array().ok_or("PRODUCED_SLOT_LIST_INVALID"));
        let mut previous = None;
        for entry in produced {
            let s = require!(entry.as_u64().ok_or("PRODUCED_SLOT_LIST_INVALID"));
            if s < lo || s > search_end || previous.is_some_and(|p| s <= p) {
                return Ok(Err("PRODUCED_SLOT_LIST_INVALID"));
            }
            previous = Some(s);
        }
        r.slots_produced = produced
            .iter()
            .filter(|v| v.as_u64().is_some_and(|s| s <= hi))
            .count() as u64;
        let mut next = lo;
        let mut previous_block: Option<(u64, String)> = None;
        for entry in produced {
            let slot = entry.as_u64().unwrap();
            if next > hi {
                break;
            }
            let block = match request(
                "getBlock",
                json!([slot, {"encoding":"json","transactionDetails":"full",
                "rewards":false,"commitment":"finalized","maxSupportedTransactionVersion":1}]),
            ) {
                Ok(v) => v,
                Err(e) => {
                    self.reconciliation.slots_failed += 1;
                    sink.coverage(&json!({"event":"FULL_SLOT_RESULT","slot":slot,"status":"FAILED","error":e}))?;
                    return Ok(Err(e));
                }
            };
            let parent = require!(block["parentSlot"]
                .as_u64()
                .filter(|p| *p < slot)
                .ok_or("BLOCK_HEADER_INVALID"));
            let hash = require!(block["blockhash"]
                .as_str()
                .filter(|h| !h.is_empty())
                .ok_or("BLOCK_HEADER_INVALID"));
            let prev_hash = require!(block["previousBlockhash"]
                .as_str()
                .filter(|h| !h.is_empty())
                .ok_or("BLOCK_HEADER_INVALID"));
            if let Some((p, h)) = &previous_block {
                if parent != *p || prev_hash != h {
                    return Ok(Err("FINALIZED_PARENT_CHAIN_MISMATCH"));
                }
            } else if parent >= lo {
                return Ok(Err("PRODUCED_SLOT_LIST_OMISSION"));
            }
            for skipped in next..slot.min(hi.saturating_add(1)) {
                if parent >= skipped {
                    return Ok(Err("SKIP_PARENT_UNPROVEN"));
                }
                sink.coverage(&json!({"event":"FULL_SLOT_RESULT","slot":skipped,"status":"SKIPPED_CONFIRMED",
                    "successor_slot":slot,"parent_slot":parent,"successor_blockhash":hash,"proof":"FINALIZED_PARENT_LINK"}))?;
                self.reconciliation.slots_skipped_confirmed += 1;
            }
            if slot > hi {
                next = hi.saturating_add(1);
                break;
            }
            let txs = require!(block["transactions"]
                .as_array()
                .ok_or("BLOCK_TRANSACTIONS_MISSING"));
            for (index, raw) in txs.iter().enumerate() {
                if Instant::now() >= deadline {
                    return Ok(Err("RECONCILIATION_DEADLINE"));
                }
                if self.reconciliation.transactions_scanned >= self.reconcile_cfg.max_transactions {
                    return Ok(Err("RECONCILIATION_TRANSACTION_BUDGET"));
                }
                self.reconciliation.transactions_scanned += 1;
                let mut raw = raw.clone();
                let obj = require!(raw.as_object_mut().ok_or("TX_SHAPE"));
                obj.insert("slot".into(), json!(slot));
                obj.insert("transactionIndex".into(), json!(index));
                let tx = require!(validate_full_transaction(&raw));
                let keys = require!(resolve_account_keys(&tx).map_err(|e| e.code()));
                if keys.contains(&self.program) {
                    self.reconciliation.marginfi_transactions += 1;
                    // Persist full relevant TX before any liquidation/strategy decision.
                    sink.transaction(&json!({"source":"full_slot_reconciliation","slot":slot,"transaction_index":index,
                        "signature":tx.signatures[0],"blockhash":hash,"block_time":block["blockTime"],"raw":raw}))?;
                    let signature = &tx.signatures[0];
                    let live = self
                        .truth
                        .live_receipts
                        .get(signature)
                        .filter(|(s, _)| *s == slot)
                        .map(|(_, t)| *t);
                    let events = require!(truth::classify(
                        &raw,
                        &self.program,
                        block["blockTime"].as_i64(),
                        live
                    ));
                    if self.truth.signatures.contains(signature) {
                        self.truth.duplicate_transactions += 1;
                        return Ok(Err("DUPLICATE_CANONICAL_TRANSACTION"));
                    }
                    let capacity = self.econ.as_ref().map(|h| h.capacity()).unwrap_or(100_000);
                    if self.truth.signatures.len() >= capacity
                        || self.truth.events.len().saturating_add(events.len()) > capacity
                    {
                        self.truth.retention_error = true;
                        return Ok(Err("CANONICAL_MARKET_RETENTION"));
                    }
                    self.truth.signatures.insert(signature.clone());
                    for event in events {
                        sink.liquidation(
                            &json!({"source":"canonical_market_truth","event":event}),
                        )?;
                        if let Some(h) = &self.econ {
                            h.emit(crate::econ::Evidence::CanonicalMarket(Box::new(
                                event.clone(),
                            )));
                        }
                        self.truth.events.push(event);
                    }
                    require!(self.recover_transaction(slot, index, &raw, sink)?);
                }
            }
            sink.coverage(&json!({"event":"FULL_SLOT_RESULT","slot":slot,"status":"PRODUCED_FETCHED",
                "blockhash":hash,"parent_slot":parent,"previous_blockhash":prev_hash,"block_time":block["blockTime"],"transactions_scanned":txs.len()}))?;
            self.reconciliation.slots_fetched += 1;
            // A progress unit is counted ONLY after the full block (including
            // relevant transactions and canonical evidence) has passed validation
            // and its coverage row was accepted by the sink.
            // Do not repeat unchanged Shadow status during slow post-capture RPC.
            if self.reconciliation.slots_fetched == 1
                || self.reconciliation.slots_fetched % 50 == 0
                || self.reconciliation.slots_fetched == self.reconciliation.slots_produced
            {
                eprintln!(
                    "[MARKET/CANONICAL] phase=SCANNING verified_blocks={} produced_blocks={} confirmed_skips={} expected_slots={} marginfi_tx={} liquidations={}",
                    self.reconciliation.slots_fetched,
                    self.reconciliation.slots_produced,
                    self.reconciliation.slots_skipped_confirmed,
                    self.reconciliation.slots_expected,
                    self.reconciliation.marginfi_transactions,
                    self.truth.events.len()
                );
            }
            let time = block["blockTime"].as_i64();
            if self.reconciliation.slots_fetched == 1 {
                self.reconciliation.first_block_time = time;
            }
            self.reconciliation.last_block_time = time;
            previous_block = Some((slot, hash.to_owned()));
            next = slot.saturating_add(1);
        }
        if next <= hi {
            return Ok(Err("TRAILING_SLOT_UNPROVEN"));
        }
        Ok(Ok(()))
    }

    pub(super) fn full_reconciliation_phase(
        &mut self,
        url: Option<&str>,
        sink: &mut dyn MarketSink,
    ) -> std::io::Result<()> {
        let cfg = self.reconcile_cfg;
        let rpc = url.ok_or("RPC_URL_MISSING").and_then(|url| {
            RpcFetcher::new(url.into(), cfg.timeout_ms, "finalized").map_err(|_| "RPC_INIT")
        });
        // Bounded post-drain read-ahead. Validation/ledger order remains strictly
        // canonical; a failed prefetched block is still retried by reconcile_slots.
        // Retain at most eight responses, never the full four-hour block payload.
        let pacer = RequestPacer::new(cfg.requests_per_second);
        let mut produced = Vec::new();
        let mut cache = BTreeMap::new();
        let mut previous_request = None;
        self.reconcile_slots(
            &mut |method, params, remaining| {
                let rpc = rpc.as_ref().map_err(|e| *e)?;
                let deadline = Instant::now() + remaining;
                if method == "getBlocks" {
                    pacer.wait(deadline)?;
                    let result = rpc.gap_request(method, params, deadline, cfg.timeout_ms)?;
                    produced = result
                        .as_array()
                        .map(|v| v.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    return Ok(result);
                }
                if method == "getBlock" {
                    let slot = params[0].as_u64().ok_or("BLOCK_SLOT_INVALID")?;
                    let options = params[1].clone();
                    return read_ahead_block(
                        slot,
                        &produced,
                        &mut cache,
                        &mut previous_request,
                        &|s| {
                            pacer.wait(deadline)?;
                            rpc.gap_request(
                                "getBlock",
                                json!([s, options]),
                                deadline,
                                cfg.timeout_ms,
                            )
                        },
                    );
                }
                pacer.wait(deadline)?;
                rpc.gap_request(method, params, deadline, cfg.timeout_ms)
            },
            sink,
        )
    }
}

/// A retry must fetch only the failed slot. Expanding read-ahead during backoff
/// adds unrelated RPC load precisely while the provider is rejecting requests.
fn read_ahead_block<F>(
    slot: u64,
    produced: &[u64],
    cache: &mut BTreeMap<u64, Result<Value, &'static str>>,
    previous_request: &mut Option<u64>,
    fetch: &F,
) -> Result<Value, &'static str>
where
    F: Fn(u64) -> Result<Value, &'static str> + Sync,
{
    let retry = *previous_request == Some(slot);
    *previous_request = Some(slot);
    if retry {
        return fetch(slot);
    }
    if let Some(value) = cache.remove(&slot) {
        return value;
    }
    if let Some(index) = produced.iter().position(|s| *s == slot) {
        let group: Vec<_> = produced[index..]
            .iter()
            .take(8)
            .copied()
            .filter(|s| !cache.contains_key(s))
            .collect();
        cache.extend(prefetch_blocks(&group, fetch));
        cache.remove(&slot).unwrap_or(Err("PREFETCH_SLOT_MISSING"))
    } else {
        fetch(slot)
    }
}

fn prefetch_blocks<F>(slots: &[u64], fetch: &F) -> BTreeMap<u64, Result<Value, &'static str>>
where
    F: Fn(u64) -> Result<Value, &'static str> + Sync,
{
    std::thread::scope(|scope| {
        let jobs: Vec<_> = slots
            .iter()
            .take(8)
            .map(|s| (*s, scope.spawn(move || fetch(*s))))
            .collect();
        jobs.into_iter()
            .map(|(slot, job)| (slot, job.join().unwrap_or(Err("BLOCK_FETCH_WORKER_FAILED"))))
            .collect()
    })
}
#[cfg(test)]
mod prefetch_tests {
    use super::*;
    #[test]
    fn bounded_fetch_keeps_slot_identity_and_errors() {
        let slots: Vec<_> = (10..30).collect();
        let result = prefetch_blocks(&slots, &|s| {
            if s == 12 {
                Err("RPC_FAILURE")
            } else {
                Ok(json!({"slot":s}))
            }
        });
        assert_eq!(result.len(), 8);
        assert_eq!(result[&10].as_ref().unwrap()["slot"], 10);
        assert_eq!(result[&12], Err("RPC_FAILURE"));
        assert_eq!(
            result.keys().copied().collect::<Vec<_>>(),
            (10..18).collect::<Vec<_>>()
        );
    }
}

fn retry_delay(error: Option<&str>, attempt: u32, base_ms: u64) -> Duration {
    if matches!(
        error,
        Some("RPC_RATE_LIMIT" | "RPC_TIMEOUT" | "RPC_TRANSPORT" | "RPC_READ")
    ) {
        Duration::from_millis(
            base_ms
                .max(1000)
                .saturating_mul(1u64 << attempt.min(5))
                .min(30_000),
        )
    } else {
        Duration::from_millis(base_ms)
    }
}
pub(super) struct RequestPacer {
    next: std::sync::Mutex<Instant>,
    interval: Duration,
}
impl RequestPacer {
    pub(super) fn new(per_second: u64) -> Self {
        Self {
            next: std::sync::Mutex::new(Instant::now()),
            interval: Duration::from_nanos(1_000_000_000u64.div_ceil(per_second.max(1))),
        }
    }
    fn reserve(&self, now: Instant, deadline: Instant) -> Result<Duration, &'static str> {
        let mut next = self
            .next
            .lock()
            .map_err(|_| "RECONCILIATION_PACER_POISONED")?;
        let at = (*next).max(now);
        if at >= deadline {
            return Err("RECONCILIATION_DEADLINE");
        }
        *next = at + self.interval;
        Ok(at.saturating_duration_since(now))
    }
    pub(super) fn wait(&self, deadline: Instant) -> Result<(), &'static str> {
        std::thread::sleep(self.reserve(Instant::now(), deadline)?);
        if Instant::now() >= deadline {
            Err("RECONCILIATION_DEADLINE")
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
mod pacing_tests {
    use super::*;
    #[test]
    fn failed_prefetched_slot_retries_alone_without_extending_batch() {
        let calls = std::sync::Mutex::new(Vec::new());
        let produced: Vec<_> = (0..16).collect();
        let mut cache = BTreeMap::new();
        let mut previous = None;
        let fetch = |slot| {
            calls.lock().unwrap().push(slot);
            if slot == 3 {
                Err("RPC_RATE_LIMIT")
            } else {
                Ok(json!(slot))
            }
        };
        for slot in 0..3 {
            assert!(read_ahead_block(slot, &produced, &mut cache, &mut previous, &fetch).is_ok());
        }
        assert_eq!(
            read_ahead_block(3, &produced, &mut cache, &mut previous, &fetch),
            Err("RPC_RATE_LIMIT")
        );
        assert_eq!(calls.lock().unwrap().len(), 8);
        assert_eq!(
            read_ahead_block(3, &produced, &mut cache, &mut previous, &fetch),
            Err("RPC_RATE_LIMIT")
        );
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 9);
        assert_eq!(calls[8], 3);
        assert_eq!(cache.len(), 4);
    }
    #[test]
    fn concurrent_reservations_are_spaced_and_deadline_is_hard() {
        let p = RequestPacer::new(5);
        let now = Instant::now();
        let end = now + Duration::from_secs(1);
        for i in 0..5 {
            assert_eq!(p.reserve(now, end).unwrap(), Duration::from_millis(i * 200));
        }
        assert_eq!(p.reserve(now, end), Err("RECONCILIATION_DEADLINE"));
    }
    #[test]
    fn transient_rpc_backoff_is_bounded() {
        assert_eq!(
            retry_delay(Some("RPC_RATE_LIMIT"), 0, 250),
            Duration::from_secs(1)
        );
        assert_eq!(
            retry_delay(Some("RPC_RATE_LIMIT"), 1, 250),
            Duration::from_secs(2)
        );
        assert_eq!(
            retry_delay(Some("RPC_RATE_LIMIT"), 10, 250),
            Duration::from_secs(30)
        );
        assert_eq!(
            retry_delay(Some("RPC_TIMEOUT"), 1, 250),
            Duration::from_secs(2)
        );
    }
}
