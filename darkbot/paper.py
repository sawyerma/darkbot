"""StateOwner with portfolio/positions/reservations/latches and the PAPER execution adapter
(spec: paper.py). The StateOwner is the single mutating owner; it applies the MarketReducer,
calls pure strategy/execution functions and processes outcomes serially. Same engine for PAPER,
Replay and Backtest (only clock/scheduler/feeds differ).

Critical section per message: normalize -> apply atomic envelope -> exposure ticks -> admission ->
decision -> reserve -> adapter enqueue -> telemetry put_nowait. No await, no locks, no DB, no
hashing, no prints.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field, replace
from decimal import Decimal
from typing import Any, Callable

from darkbot.domain import (
    NS_PER_MS, QuoteScopeStatus, OCCUPIED_SLOT_STATES, AttemptHandle, AttemptStatus, CostBasis, CoverageState, CurveState, DecisionAction, ExecutionOutcome, ExitReason, FillEvidence,
    Intent, InvalidQuote, MarketState, NormalizedEnvelope, PositionRecord, PositionState, QuoteSnapshot, RawEnvelope, Side, VenueState, Completeness,
)
from darkbot.execution import ExecutionPolicy, FillEvaluation, LiveBroadcastForbidden, QuoteEngine, assert_paper_boundary, book_entry_fill, book_exit_fill, book_failed_attempt, evaluate_fill, fee_terms, next_retry_submit_mono_ns
from darkbot.market import ApplyResult, MarketReducer, Normalizer, PumpDecoder
from darkbot.storage import ObservationRecord, RecordGroup, Recorder, RowRecord, UpdateRecord
from darkbot.strategies import make_strategy
from darkbot.strategy import FEATURE_SET_V1, FEATURE_SET_V2, FeatureCalculator, FeatureEvent, FeatureInputs, entry_admission, pick_exit_trigger, target_reached


class PaperExecutionAdapter:
    """submit(Intent) -> AttemptHandle; the modeled fill is scheduled after the declared delay and the
    outcome is delivered back to the StateOwner with the ACTUAL callback time."""

    def __init__(self, clock: Any, scheduler: Any, mode: str, live_broadcast_enabled: bool, entry_delay_ns: int, exit_delay_ns: int, lateness_provider: Callable[[str], int] | None = None, delay_provider: Callable[[Intent], int] | None = None) -> None:
        assert_paper_boundary(mode, live_broadcast_enabled)
        self.clock, self.scheduler = clock, scheduler
        self.mode, self.live_broadcast_enabled = mode, live_broadcast_enabled
        self.entry_delay_ns, self.exit_delay_ns = entry_delay_ns, exit_delay_ns
        self.delay_provider = delay_provider
        self.lateness_provider = lateness_provider  # replay: recorded callback lateness per attempt key
        self._seq = 0
        self.on_outcome: Callable[[ExecutionOutcome], None] | None = None
        self.pending: dict[str, AttemptHandle] = {}  # only attempts awaiting their callback (bounded by open exposure)
        self.submitted: list[tuple[str, str, int, int, int, str]] = []  # compact audit log (attempt_id, side, n, submit, scheduled, quote_state_event_id)

    def submit(self, intent: Intent) -> AttemptHandle:
        assert_paper_boundary(self.mode, self.live_broadcast_enabled)
        self._seq += 1
        submit = self.clock.monotonic_ns()
        scheduled = submit + (self.delay_provider(intent) if self.delay_provider else intent.delay_ns)
        handle = AttemptHandle(f"{intent.position_id}_{intent.side.value.lower()}{intent.attempt_number}", intent, submit, scheduled)
        self.pending[handle.attempt_id] = handle
        self.submitted.append((handle.attempt_id, intent.side.value, intent.attempt_number, submit, scheduled, intent.quote.quote_state_event_id))

        def fire(actual_mono_ns: int, h: AttemptHandle = handle) -> None:
            assert self.on_outcome is not None
            self.pending.pop(h.attempt_id, None)
            if self.lateness_provider is not None and hasattr(self.clock, "advance_to"):
                self.clock.advance_to(max(self.clock.monotonic_ns(), actual_mono_ns))
            self.on_outcome(ExecutionOutcome(h.attempt_id, h.intent, h.submit_mono_ns, h.scheduled_fill_mono_ns, actual_mono_ns))

        # Replay delivery occurs at the recorded callback time. The outcome
        # retains scheduled_fill_mono_ns as model time.
        callback_due = scheduled
        if self.lateness_provider is not None:
            callback_due += max(0, self.lateness_provider(handle.attempt_id))
        self.scheduler.schedule_at(callback_due, fire)
        return handle

    def send_transaction(self, *_a: Any, **_k: Any) -> None:  # T22: forbidden path
        raise LiveBroadcastForbidden("PAPER adapter never broadcasts")


@dataclass
class Portfolio:
    cash_lamports: int
    reserved_lamports: int = 0
    locked_rent_lamports: int = 0
    ledger_cash_sum: int = 0

    @property
    def free_cash(self) -> int:
        return self.cash_lamports


@dataclass
class Counters:
    raw_messages: int = 0
    decode_errors: int = 0
    internal_errors: int = 0
    creates: int = 0
    creates_actionable: int = 0
    hints: int = 0
    duplicates: int = 0
    entry_intents: int = 0
    entry_fills: int = 0
    entry_failed: int = 0
    exits_creator: int = 0
    exits_target: int = 0
    exits_timeout: int = 0
    exit_fills: int = 0
    exit_failed_attempts: int = 0
    exit_exhausted: int = 0
    unresolved: int = 0
    censored: int = 0
    ineligible: dict[str, int] = field(default_factory=dict)
    quality_events: int = 0


class StateOwner:
    def __init__(self, cfg: Any, run_id: str, clock: Any, scheduler: Any, recorder: Recorder, adapter: PaperExecutionAdapter, basis_getter: Callable[[], CostBasis], mode: str,
                 economic_intents_allowed: bool = True, feature_sink: Callable[[Callable[[], None]], None] | None = None, variant_id: str | None = None,
                 canonical_prefix: str | None = None, persist_observations: bool = True, compute_features: bool = True, engine: QuoteEngine | None = None, mint_allowlist: set[str] | None = None,
                 prior_submitted_mints: set[str] | None = None, capture_only: bool = False) -> None:
        self.cfg, self.run_id, self.clock, self.scheduler, self.recorder, self.adapter, self.basis_getter, self.mode = cfg, run_id, clock, scheduler, recorder, adapter, basis_getter, mode
        self.economic_intents_allowed = economic_intents_allowed
        self.capture_only = capture_only
        self.feature_sink = feature_sink or (lambda job: job())
        self.variant_id = variant_id
        adapter.on_outcome = self.on_outcome
        self.decoder = PumpDecoder(cfg.protocol)
        self.normalizer = Normalizer(self.decoder)
        self.reducer = MarketReducer(canonical_prefix or run_id, cfg.storage["state_history_per_mint"], cfg.storage["max_tracked_mints"], cfg.storage["trade_log_per_mint"], cfg.storage["undecodable_points_max"])
        self.reducer.signature_history_max = cfg.storage["signature_history_max"]
        self.reducer.dedup_keys_per_mint = cfg.storage["dedup_keys_per_mint"]
        self.reducer.set_quote_constants(cfg.protocol["wsol_mint"], cfg.protocol["native_sol_curve_sentinel"])
        self.telemetry_max = cfg.storage["telemetry_samples_max"]
        self.fee_history: list[tuple[int, str, Any]] = []  # (received_mono_ns, kind, snapshot): fee rule valid at a model time = latest snapshot received <= t
        self.spl_token_program = cfg.protocol["spl_token_program"]
        self.token_2022_supported = cfg.execution["token_account_policy"]["token_2022_supported"]
        self.decode_issues: dict[str, int] = {}
        self.persist_observations = persist_observations
        self.mint_allowlist = mint_allowlist  # research splits: entries only for launches of the evaluated split
        self.compute_features = compute_features
        self.strategy = None if capture_only else make_strategy(cfg.strategy)
        self.policy = ExecutionPolicy.from_config(cfg)
        self.engine = engine or QuoteEngine(self.policy)
        self.portfolio = Portfolio(cfg.portfolio["initial_paper_balance_lamports"])
        self.initial_capital_lamports = cfg.portfolio["initial_paper_balance_lamports"]
        self.capital_initialized = cfg.portfolio["initial_paper_balance_mode"] == "FIXED_LAMPORTS"
        self.max_open = cfg.portfolio["max_open_positions"]
        self.timeout_ns = cfg.strategy["strategy_timeout_ms"] * NS_PER_MS
        self.horizon_ns = cfg.research["research_horizon_ms"] * NS_PER_MS
        self.entry_delay_ns = cfg.latency_model["entry_delay_ms"] * NS_PER_MS
        self.exit_delay_ns = cfg.latency_model["exit_delay_ms"] * NS_PER_MS
        self.feature_offsets_ns = tuple(o * NS_PER_MS for o in cfg.research["feature_snapshot_offsets_ms"])
        self.features = FeatureCalculator(cfg.research["feature_ratio_decimal_places"], cfg.research["feature_first_buyer_window"])
        self.positions: dict[str, PositionRecord] = {}
        self.active_ids: set[str] = set()
        self.position_by_mint: dict[str, str] = {}
        self.submitted_mints: set[str] = set(prior_submitted_mints or ())  # ONE_SUBMITTED_ENTRY_PER_MINT_PER_CAMPAIGN survives restarts (loaded from the campaign DB)
        self.health_history: list[tuple[int, str, bool]] = []  # (mono_ns, feed_id, ready) for historical feature views (V1)
        self.fee_history_max = cfg.storage["fee_history_max"]
        self.error_text_max = cfg.storage["error_text_max_chars"]
        self.counters = Counters()
        self.stop_new_entries = False
        self.disk_error = False
        self.feed_ready: dict[str, bool] = {}
        self.eligible_trade_feeds = {f.id for f in cfg.feeds if f.enabled and f.transport == "logsSubscribe" and {"CREATE", "BUY", "SELL"} <= set(f.provides)}
        self.pending_attempts: dict[str, dict[str, Any]] = {}
        self.quote_by_attempt: dict[str, QuoteSnapshot] = {}  # expected quote per attempt (audit); pruned with terminal positions
        self.attempts_by_position: dict[str, list[str]] = {}
        self.obs_seq = 0
        self.dec_seq = 0
        self.pos_seq = 0
        self.q_seq = 0
        self.telemetry: dict[str, list[int]] = {"receive_to_decode_ns": [], "ready_to_decision_ns": [], "decision_to_enqueue_ns": [], "receive_to_decision_ns": [], "callback_lateness_ns": []}
        self.quality_seq = 0
        self.last_error: str | None = None
        self.transport_by_feed = {f.id: f.transport for f in cfg.feeds}
        self.commitment_by_feed = {f.id: f.commitment for f in cfg.feeds}
        self.first_create_receive: dict[str, int] = {}
        self.feature_jobs_scheduled: set[str] = set()
        self.fx_snapshot = None

    def on_fx_snapshot(self, snapshot):
        self.fx_snapshot = snapshot
        self._record(RowRecord("fx_snapshots", snapshot.row(self.run_id)))
        if not self.capital_initialized and self.cfg.portfolio["initial_paper_balance_mode"] == "USD_PER_SLOT":
            total_usd = Decimal(self.cfg.portfolio["slot_capital_usd"]) * self.max_open
            capital = snapshot.budget(str(total_usd), self.clock.monotonic_ns(), self.cfg["fx"]["max_age_ms"])
            if capital:
                self.portfolio.cash_lamports = capital
                self.initial_capital_lamports = capital
                self.capital_initialized = True
                self._record(UpdateRecord("runs", "run_id", self.run_id, {"start_balance_lamports": capital}))

    def record_observation(self, observation, qualified, actual, status=None):
        if status is None:
            status = "OBSERVING" if qualified is None else "DECIDED"
        self._record(RowRecord("strategy_observations", {
            "observation_id": f"{self.run_id}_obs_{observation.mint}", "run_id": self.run_id, "mint": observation.mint,
            "strategy_id": self.cfg.strategy_id, "strategy_config_hash": self.cfg.strategy_config_hash,
            "create_signature": observation.signature, "observation_start": observation.start, "observation_end": observation.end,
            "decision_actual_mono_ns": actual, "buy_count": observation.buy_count, "unique_buyers": len(observation.buyers),
            "qualified": qualified, "status": status}, upsert=True))

    # ------------------------------------------------------------------ helpers
    def _tele(self, key: str, value: int) -> None:
        lst = self.telemetry[key]
        lst.append(value)
        if len(lst) > self.telemetry_max:
            del lst[: self.telemetry_max // 2]

    def on_fee_snapshots(self, snapshots: list[Any]) -> None:
        """Fee snapshots are knowledge with a receive time; the rule valid at a modeled fill is the latest snapshot received before it (S07)."""
        kinds = {"PROTOCOL_GLOBAL_ACCOUNT": "protocol", "NETWORK_FEE": "network", "RENT_EXEMPTION": "rent", "PRIORITY_POLICY": "priority_policy"}
        for snap in snapshots:
            self.fee_history.append((snap.received_mono_ns, kinds[snap.source_type], snap))
            # Each pending attempt retains its effective modeled-fill basis,
            # independently of global history compaction and callback delay.
            for meta in self.pending_attempts.values():
                if snap.received_mono_ns <= meta["handle"].scheduled_fill_mono_ns:
                    old = meta.get("fill_basis", meta["basis"])
                    values = {k: getattr(old, k) for k in ("protocol", "network", "rent", "priority_policy")}
                    values[kinds[snap.source_type]] = snap
                    meta["fill_basis"] = CostBasis(**values)
        if len(self.fee_history) > self.fee_history_max:
            latest_by_kind: dict[str, int] = {}
            for i, (_, kind, _) in enumerate(self.fee_history):
                latest_by_kind[kind] = i
            keep = set(latest_by_kind.values())
            for i in range(len(self.fee_history) - 1, -1, -1):
                if len(keep) >= self.fee_history_max:
                    break
                keep.add(i)
            self.fee_history = [item for i, item in enumerate(self.fee_history) if i in keep]
        # Background cost recovery is itself a wakeup; a quiet mint must not
        # need another trade to release an already latched liquidation intent.
        now = self.clock.monotonic_ns()
        for pid in sorted(self.active_ids):
            pos = self.positions[pid]
            if pos.latched_triggers:
                self._maybe_exit(pos, self.reducer.state_for(pos.mint), now)

    def basis_at(self, mono_ns: int) -> CostBasis:
        latest: dict[str, Any] = {}
        for received, kind, snap in self.fee_history:
            if received <= mono_ns:
                latest[kind] = snap
        if not latest:
            return self.basis_getter()
        return CostBasis(latest.get("protocol"), latest.get("network"), latest.get("rent"), latest.get("priority_policy"))

    def _curve(self, st: MarketState, entry: tuple | None = None) -> CurveState | None:
        if entry is None:
            if not st.curve_known or st.last_state_event_id is None:
                return None
            return CurveState(st.last_state_event_id, st.last_state_event_id.split("|")[1], st.last_state_slot, st.last_state_sequence, st.last_state_receive_mono_ns or 0, st.virtual_sol_reserves, st.virtual_token_reserves, st.real_sol_reserves, st.real_token_reserves, st.venue_state, st.creator_fee_bps, st.creator != self.cfg.protocol["native_sol_curve_sentinel"], st.token_program, st.account_profile_id)  # type: ignore[arg-type]
        rec, seq, cid, slot, vsol, vtok, rsol, rtok = entry
        return CurveState(cid, cid.split("|")[1], slot, seq, rec, vsol, vtok, rsol, rtok, VenueState.BONDING_CURVE_ACTIVE, st.creator_fee_bps, st.creator != self.cfg.protocol["native_sol_curve_sentinel"], st.token_program, st.account_profile_id)

    def _coverage_at(self, st: MarketState, start: int, end: int | None) -> CoverageState:
        if self.reducer.gap_intersects(start, end):
            return CoverageState.GAP
        return st.coverage_state

    def _record(self, rec: Any) -> None:
        if self.recorder.try_append(rec) == Recorder.OVERFLOW and not self.stop_new_entries:
            self._quality("RECORDER_OVERFLOW", "CRITICAL", "RUN", canonical_gap=True, details={"queue_capacity": self.recorder.capacity}, lost=None)
            self.stop_new_entries = True

    def _quality(self, kind: str, severity: str, scope: str, canonical_gap: bool, details: dict[str, Any], lost: int | None, feed_id: str | None = None, start: int | None = None, end: int | None = None, recovery: str = "OPEN", evidence: list[str] | None = None) -> None:
        self.quality_seq += 1
        self.counters.quality_events += 1
        now = self.clock.monotonic_ns()
        self.recorder.try_append(RowRecord("quality_events", {"quality_event_id": f"{self.run_id}_q{self.quality_seq}", "run_id": self.run_id, "feed_id": feed_id, "start_mono_ns": start if start is not None else now, "end_mono_ns": end,
                                                              "kind": kind, "severity": severity, "scope": scope, "lost_records_count": lost, "details": details, "canonical_gap": canonical_gap, "coverage_version": "CANONICAL_COVERAGE_V2",
                                                              "recovery_status": recovery, "evidence_observation_ids": evidence or []}), priority=True)

    def _position_record(self, pos: PositionRecord) -> None:
        self._record(("position", pos))

    def occupied_slots(self) -> int:
        """Counts only positions still able to occupy a slot; terminal positions leave the active index (no full-history scan)."""
        n = 0
        for pid in list(self.active_ids):
            state = self.positions[pid].state
            if state in OCCUPIED_SLOT_STATES:
                n += 1
            elif state in (PositionState.CLOSED, PositionState.ENTRY_FAILED, PositionState.INELIGIBLE):
                self.active_ids.discard(pid)
        return n

    def recording_ok(self) -> bool:
        return not (self.recorder.overflowed or self.disk_error or self.stop_new_entries)

    # ------------------------------------------------------------------ feed health -> canonical coverage (provider_coverage_contract)
    def on_feed_health(self, feed_id: str, ready: bool, mono_ns: int, reason: str = "") -> None:
        prev = self.feed_ready.get(feed_id)
        self.feed_ready[feed_id] = ready
        if prev == ready:
            return
        self.health_history.append((mono_ns, feed_id, ready))
        if len(self.health_history) > self.fee_history_max:
            del self.health_history[: self.fee_history_max // 2]
        eligible_ready = any(self.feed_ready.get(f) for f in self.eligible_trade_feeds)
        if not ready:
            if eligible_ready or feed_id not in self.eligible_trade_feeds:
                self._quality("PROVIDER_DISCONNECT", "WARNING", "FEED", canonical_gap=False, details={"reason": reason, "covered_by_other_feed": eligible_ready}, lost=None, feed_id=feed_id, start=mono_ns)
            else:
                self.reducer.open_gap(mono_ns)
                self._quality("CANONICAL_GAP", "CRITICAL", "ALL_MINTS", canonical_gap=True, details={"reason": reason, "last_eligible_feed": feed_id}, lost=None, feed_id=feed_id, start=mono_ns)
                for pos in self.positions.values():
                    if pos.state in OCCUPIED_SLOT_STATES and not pos.canonical_provider_gap_flag:
                        pos.canonical_provider_gap_flag = True
                        self._position_record(pos)
        else:
            if self.reducer.gap_open_since is not None and feed_id in self.eligible_trade_feeds:
                start = self.reducer.gap_open_since
                self.reducer.close_gap(mono_ns)
                self._quality("CANONICAL_GAP_RECOVERED_STATE_ONLY", "WARNING", "ALL_MINTS", canonical_gap=True, details={"gap_start_mono_ns": start, "note": "history during gap not reconstructable"}, lost=None, feed_id=feed_id, start=start, end=mono_ns, recovery="RECOVERED_STATE_ONLY")
            elif feed_id in self.eligible_trade_feeds:
                self._quality("PROVIDER_RECONNECT", "INFO", "FEED", canonical_gap=False, details={}, lost=None, feed_id=feed_id, start=mono_ns, end=mono_ns, recovery="RECOVERED")

    def on_transport_error(self, feed_id: str, kind: str, message: str, mono_ns: int) -> None:
        self._quality(kind, "WARNING", "FEED", canonical_gap=False, details={"message": message[:200]}, lost=None, feed_id=feed_id, start=mono_ns, end=mono_ns, recovery="RECONNECTING")

    def on_disk_error(self, message: str) -> None:
        self.disk_error = True
        self.stop_new_entries = True
        self.last_error = message

    # ------------------------------------------------------------------ hot path
    def on_raw(self, raw: RawEnvelope) -> ApplyResult | None:
        self.counters.raw_messages += 1
        transport = self.transport_by_feed.get(raw.feed_id, "logsSubscribe")
        try:
            env = self.normalizer.normalize(raw, transport, 0)
        except Exception as exc:  # noqa: BLE001 - decoder programming error: diagnostic, never a transport reconnect (T14)
            self.counters.internal_errors += 1
            self.last_error = f"DECODER_INTERNAL_ERROR: {type(exc).__name__}: {exc}"
            self._quality("DECODER_INTERNAL_ERROR", "CRITICAL", "FEED", canonical_gap=False, details={"error": self.last_error[: self.error_text_max]}, lost=None, feed_id=raw.feed_id)
            return None
        if env is None:
            return None
        decode_done = self.clock.monotonic_ns()
        env = NormalizedEnvelope(env.signature, env.slot, env.success, env.events, env.completeness, env.provenance, decode_done, env.decoder_version, env.error)
        self._tele("receive_to_decode_ns", decode_done - raw.receive_mono_ns)
        if env.completeness in (Completeness.UNDECODABLE, Completeness.PARTIAL):
            self.counters.decode_errors += 1
            self.decode_issues[raw.feed_id] = self.decode_issues.get(raw.feed_id, 0) + 1
        try:
            self.reducer.protected_mints = {self.positions[pid].mint for pid in self.active_ids} | set(getattr(self.strategy, "observations", {}))
            result = self.reducer.apply(env, raw)
            if self.capture_only:
                # Pure market capture: decode/reduce/persist only. No strategy callback,
                # no admission, no intent and no simulated execution.
                for mint in result.new_creates:
                    first = mint not in self.first_create_receive
                    if first:
                        self.counters.creates += 1
                        self.first_create_receive[mint] = raw.receive_mono_ns
                    st = self.reducer.states[mint]
                    if first and st.quote_scope_status == QuoteScopeStatus.SOL_WSOL and st.venue_state == VenueState.BONDING_CURVE_ACTIVE:
                        self.counters.creates_actionable += 1
            else:
                # P0: exposed positions first, then immediately actionable CREATEs
                for mint in result.touched:
                    if mint in self.position_by_mint:
                        self._position_tick(mint, raw.receive_mono_ns)
                self.strategy.on_events(self, result, raw, env)
                for mint in result.new_creates:
                    if mint not in self.first_create_receive:
                        self.counters.creates += 1
                    self.first_create_receive.setdefault(mint, raw.receive_mono_ns)
                    self.strategy.on_create(self, mint, raw, env, decode_done)
        except Exception as exc:  # noqa: BLE001 - consumer invariant violation (T14)
            self.counters.internal_errors += 1
            self.last_error = f"STATE_OWNER_INTERNAL_ERROR: {type(exc).__name__}: {exc}"
            self._quality("STATE_OWNER_INTERNAL_ERROR", "CRITICAL", "RUN", canonical_gap=False, details={"error": self.last_error[: self.error_text_max]}, lost=None, feed_id=raw.feed_id)
            self.stop_new_entries = True
            return None
        if self.persist_observations:
            self._telemetry(raw, env, result, decode_done)
        return result

    def _telemetry(self, raw: RawEnvelope, env: NormalizedEnvelope, result: ApplyResult, decode_done: int) -> None:
        self.obs_seq += 1
        obs_id = f"{self.run_id}_o{self.obs_seq}"
        events: list[tuple[int, int | None, str | None, str | None, str | None, str]] = []
        new_me: list[dict[str, Any]] = []
        for k, ea in enumerate(result.events):
            ev = ea.event
            events.append((k, ev.instruction_index, ev.mint, ev.event_type.value, ea.canonical_event_id, ea.validation.value))
            if ea.is_new and ea.canonical_event_id is not None and ea.validation.value in ("VALID", "STALE", "CONFLICT"):
                st = self.reducer.states[ev.mint]  # type: ignore[index]
                actionable = ev.event_type.value == "CREATE" and ev.mint in result.new_creates and st.quote_scope_status == QuoteScopeStatus.SOL_WSOL and st.venue_state == VenueState.BONDING_CURVE_ACTIVE
                sol_scope = st.quote_scope_status == QuoteScopeStatus.SOL_WSOL  # foreign/unknown quote amounts never land in SOL columns (S12)
                new_me.append({"canonical_event_id": ea.canonical_event_id, "run_id": self.run_id, "signature": env.signature, "instruction_index": ev.instruction_index, "slot": env.slot, "mint": ev.mint, "event_type": ev.event_type.value,
                               "account_profile_id": st.account_profile_id, "actor": ev.actor, "creator": ev.creator or st.creator, "token_program": ev.token_program or st.token_program,
                               "sol_amount": ev.sol_amount if sol_scope else None, "token_amount": ev.token_amount, "token_total_supply": ev.token_total_supply or st.token_total_supply,
                               "creator_fee_bps": st.creator_fee_bps, "virtual_sol_reserves": ev.virtual_sol_reserves if sol_scope else None, "virtual_token_reserves": ev.virtual_token_reserves if sol_scope else None,
                               "real_sol_reserves": ev.real_sol_reserves if sol_scope else None, "real_token_reserves": ev.real_token_reserves if sol_scope else None, "first_seen_mono_ns": raw.receive_mono_ns, "actionable_mono_ns": decode_done if actionable else None,
                               "winner_feed_id": raw.feed_id, "source_observation_id": obs_id if k == 0 else f"{obs_id}-{k}", "commitment": self.commitment_by_feed.get(raw.feed_id),
                               "quality_status": ea.validation.value, "quote_mint_raw": ev.quote_mint_raw, "quote_field_origin": ev.quote_field_origin, "quote_layout_hash": ev.quote_layout_hash, "normalized_quote_asset": ev.normalized_quote_asset,
                               "quote_scope_status": ev.quote_scope_status.value if ev.quote_scope_status else None, "venue_state": st.venue_state.value, "canonical_coverage_state": st.coverage_state.value})
            if ea.upgraded and ea.canonical_event_id is not None:
                st = self.reducer.states[ev.mint]  # type: ignore[index]
                actionable = ev.event_type.value == "CREATE" and ev.mint in result.new_creates and st.quote_scope_status == QuoteScopeStatus.SOL_WSOL and st.venue_state == VenueState.BONDING_CURVE_ACTIVE
                self._record(UpdateRecord("market_events", "canonical_event_id", ea.canonical_event_id, {"account_profile_id": st.account_profile_id, "actionable_mono_ns": decode_done if actionable else None, "winner_feed_id": raw.feed_id, "source_observation_id": obs_id if k == 0 else f"{obs_id}-{k}",
                                                                                                          "quality_status": "VALID_UPGRADED_ACCOUNT_PROFILE" if ea.profile_upgraded else "VALID_UPGRADED_FROM_PARTIAL", "quote_scope_status": st.quote_scope_status.value, "venue_state": st.venue_state.value, "canonical_coverage_state": st.coverage_state.value}))
            if ea.validation.value == "DUPLICATE":
                self.counters.duplicates += 1
            if ev.event_type.value.startswith("HINT"):
                self.counters.hints += 1
        self._record(ObservationRecord(obs_id, self.run_id, raw, decode_done, env.slot, env.signature, env.completeness.value, tuple(events), tuple(new_me)))
        for mint in result.decode_gaps:
            self._quality("DECODE_GAP", "WARNING", f"MINT:{mint}", canonical_gap=True, details={"reason": "unknown Pump event in a transaction touching this mint; curve state unknowable until the next complete state"}, lost=None, feed_id=raw.feed_id, start=raw.receive_mono_ns, evidence=[obs_id])
        for mint, cid, kind in result.conflicts:
            self._quality("EVENT_CONFLICT", "WARNING", f"MINT:{mint}", canonical_gap=False, details={"canonical_event_id": cid, "kind": kind, "effect": "quote_uncertain=true; no new entry for this mint"}, lost=None, feed_id=raw.feed_id, start=raw.receive_mono_ns, end=raw.receive_mono_ns, recovery="MANUAL_REVIEW", evidence=[obs_id])
        for mint, old, new in result.venue_changes:  # migration_contract.recording (outside the decision step)
            st = self.reducer.states[mint]
            self._quality("VENUE_TRANSITION", "WARNING", f"MINT:{mint}", canonical_gap=False, details={**st.migration_evidence, "quote_invalidation_mono_ns": raw.receive_mono_ns, "slot": env.slot, "signature": env.signature,
                                                                                                          "position_reason": self._venue_reason(new) if mint in self.position_by_mint else None}, lost=None, feed_id=raw.feed_id, start=raw.receive_mono_ns, end=raw.receive_mono_ns, recovery="VENUE_LOCKED", evidence=[obs_id])

    # ------------------------------------------------------------------ entry
    def _entry_decision(self, mint: str, raw: RawEnvelope, env: NormalizedEnvelope, decode_done: int, policy_rejection: str | None = None) -> None:
        st = self.reducer.states[mint]
        data_ready = decode_done
        basis = self.basis_getter()
        now = self.clock.monotonic_ns()
        creator_sold = st.creator_sell_event_id is not None
        gap = st.coverage_state == CoverageState.GAP or self.reducer.gap_open_since is not None
        verdict = self.strategy.admission(st, gap_flag=gap, creator_sell_known=creator_sold, already_submitted=mint in self.submitted_mints or mint in self.position_by_mint, slots_free=self.occupied_slots() < self.max_open,
                                  capital_ok=True, costs_ok=basis.complete and all(s.is_fresh(now) for s in (basis.protocol, basis.network, basis.rent, basis.priority_policy)),  # type: ignore[union-attr]
                                  recording_ok=self.recording_ok(), quote_ok=True)
        quote: QuoteSnapshot | InvalidQuote | None = None
        exit_reserve = 0
        reason = verdict.reason
        eligible = verdict.eligible
        budget = None
        if policy_rejection is not None:
            eligible, reason = False, policy_rejection
        if eligible and self.cfg.strategy["sizing_mode"] == "USD_SWAP_BUDGET":
            budget = self.fx_snapshot.budget(self.cfg.strategy["trade_size_usd"], now, self.cfg["fx"]["max_age_ms"]) if self.fx_snapshot else None
            if not budget:
                eligible, reason = False, "SOL_USD_EVIDENCE_MISSING_OR_STALE"
        if eligible and not self.economic_intents_allowed:
            eligible, reason = False, "CAPTURE_ONLY_MODE"
        if eligible and self.mint_allowlist is not None and mint not in self.mint_allowlist:
            eligible, reason = False, "OUT_OF_RESEARCH_SPLIT"
        if eligible and st.unsupported_fee_mode:
            eligible, reason = False, "UNSUPPORTED_FEE_MODE_" + st.unsupported_fee_mode
        if eligible and st.quote_uncertain:
            eligible, reason = False, "QUOTE_CONFLICT"
        if eligible and st.decode_gap_since is not None:
            eligible, reason = False, "DECODE_GAP"
        if eligible and (st.token_program not in (self.spl_token_program, self.cfg.protocol["token_2022_program"]) or (st.token_program == self.cfg.protocol["token_2022_program"] and not self.token_2022_supported)):
            eligible, reason = False, "UNSUPPORTED_TOKEN_PROGRAM"
        if eligible:
            curve = self._curve(st)
            if curve is None:
                quote = InvalidQuote("CURVE_UNAVAILABLE")
            else:
                reserve = self.engine.exit_reserve_lamports(basis, now, curve)
                if reserve is None:
                    quote = InvalidQuote("UNQUALIFIED_COSTS")
                else:
                    exit_reserve = reserve
                    if budget is not None and self.cfg.strategy["entry_amount_semantics"] == "TOTAL_SLOT_BUDGET_INCLUDING_ALL_COSTS":
                        quote = self.engine.quote_buy_total_slot_budget(curve, basis, now, f"{self.run_id}_", st.coverage_state, budget, exit_reserve)
                    elif budget is not None:
                        quote = self.engine.quote_buy_swap_budget(curve, basis, now, f"{self.run_id}_", st.coverage_state, budget)
                    else:
                        quote = self.engine.quote_buy(curve, basis, now, f"{self.run_id}_", st.coverage_state)
            if isinstance(quote, InvalidQuote):
                eligible, reason = False, quote.reason
            elif self.portfolio.cash_lamports < self._max_entry_debit(quote) + exit_reserve:
                eligible, reason = False, "INSUFFICIENT_CAPITAL"
        self.dec_seq += 1
        decision_id = f"{self.run_id}_d{self.dec_seq}"
        if eligible and budget is not None:
            self._record(RowRecord("entry_sizing", {"decision_id": decision_id, "run_id": self.run_id, "mint": mint,
                "trade_size_usd": self.cfg.strategy["trade_size_usd"], "fx_snapshot_id": f"{self.run_id}_{self.fx_snapshot.snapshot_id}",
                "swap_budget_lamports": budget, "curve_input_lamports": quote.curve_input_lamports, "max_swap_debit_lamports": quote.max_input_base_units, "sizing_basis": self.cfg.strategy["sizing_mode"], "decision_mono_ns": now}))
        if not eligible:
            self.counters.ineligible[reason] = self.counters.ineligible.get(reason, 0) + 1
            decision_done = self.clock.monotonic_ns()
            self._tele("ready_to_decision_ns", decision_done - data_ready)
            self._tele("receive_to_decision_ns", decision_done - raw.receive_mono_ns)
            self._record(RowRecord("decisions", {"decision_id": decision_id, "run_id": self.run_id, "mint": mint, "event_id": st.create_event_id, "policy_hash": self.cfg.policy_hash, "receive_mono_ns": raw.receive_mono_ns,
                                                 "data_ready_mono_ns": data_ready, "decision_done_mono_ns": decision_done, "intent_enqueued_mono_ns": None, "action": DecisionAction.SKIP.value, "reason": reason, "position_id": None,
                                                 "quote_state_event_id": st.last_state_event_id, "quote_age_ns": None, "quote_snapshot_id": None, "arrival_sequence": raw.arrival_sequence}))
            self._schedule_research_end(mint, raw.receive_mono_ns)
            self._schedule_features(mint, raw.receive_mono_ns)
            return
        assert isinstance(quote, QuoteSnapshot)
        self.counters.creates_actionable += 1
        # atomic reservation in the same serial RAM step (no await): max debit with slippage bound + exit reserve
        required = self._max_entry_debit(quote) + exit_reserve
        self.portfolio.cash_lamports -= required
        self.portfolio.reserved_lamports += required
        self.pos_seq += 1
        position_id = f"{self.run_id}_p{self.pos_seq}"
        pos = PositionRecord(position_id, self.run_id, mint, st.creator, self.cfg.policy_hash, PositionState.ENTRY_INTENT, decision_id, variant_id=self.variant_id, target_net_multiple=self.cfg.strategy["target_net_multiple"],
                             research_end_mono_ns=raw.receive_mono_ns + self.horizon_ns, venue_state=st.venue_state, canonical_provider_gap_flag=False, reserved_cash_lamports=required, exit_reserve_lamports=exit_reserve)
        self.positions[position_id] = pos
        self.active_ids.add(position_id)
        self.position_by_mint[mint] = position_id
        self.submitted_mints.add(mint)
        self.counters.entry_intents += 1
        decision_done = self.clock.monotonic_ns()
        if self._quote_stale(quote, decision_done):  # execution.quote_max_age_ms: cancel before submit (no fee), release reservation, no re-entry
            self.portfolio.cash_lamports += required
            self.portfolio.reserved_lamports -= required
            pos.state = PositionState.ENTRY_FAILED
            pos.unresolved_reason = "QUOTE_STALE_BEFORE_SUBMIT"
            pos.research_status = "ENTRY_CANCELLED"
            self.counters.ineligible["QUOTE_STALE"] = self.counters.ineligible.get("QUOTE_STALE", 0) + 1
            self._record(RecordGroup((("quote_snapshot", (quote, self.run_id, position_id)),
                                      RowRecord("decisions", {"decision_id": decision_id, "run_id": self.run_id, "mint": mint, "event_id": st.create_event_id, "policy_hash": self.cfg.policy_hash, "receive_mono_ns": raw.receive_mono_ns,
                                                              "data_ready_mono_ns": data_ready, "decision_done_mono_ns": decision_done, "intent_enqueued_mono_ns": None, "action": DecisionAction.CANCEL_INTENT.value, "reason": "QUOTE_STALE", "position_id": position_id,
                                                              "quote_state_event_id": quote.quote_state_event_id, "quote_age_ns": decision_done - quote.created_mono_ns, "quote_snapshot_id": quote.quote_snapshot_id, "arrival_sequence": raw.arrival_sequence}),
                                      ("position", pos))))
            self._schedule_research_end(mint, raw.receive_mono_ns)
            return
        intent = Intent(f"{position_id}_i1", position_id, mint, Side.BUY, 1, decision_id, quote, {"protocol": quote.fee_snapshot_id}, self.entry_delay_ns, decision_done)
        handle = self.adapter.submit(intent)
        pos.state = PositionState.ENTRY_PENDING
        pos.submitted_entry = True
        pos.entry_attempts = 1
        enqueued = self.clock.monotonic_ns()
        self._tele("ready_to_decision_ns", decision_done - data_ready)
        self._tele("receive_to_decision_ns", decision_done - raw.receive_mono_ns)
        self._tele("decision_to_enqueue_ns", enqueued - decision_done)
        self.pending_attempts[handle.attempt_id] = {"handle": handle, "basis": basis, "coverage": st.coverage_state}
        self.quote_by_attempt[handle.attempt_id] = quote
        self.attempts_by_position.setdefault(position_id, []).append(handle.attempt_id)
        # telemetry (values only; serialisation in writer); FK-coupled rows travel as one group
        self._record(RecordGroup((("quote_snapshot", (quote, self.run_id, position_id)),
                                  RowRecord("decisions", {"decision_id": decision_id, "run_id": self.run_id, "mint": mint, "event_id": st.create_event_id, "policy_hash": self.cfg.policy_hash, "receive_mono_ns": raw.receive_mono_ns,
                                                          "data_ready_mono_ns": data_ready, "decision_done_mono_ns": decision_done, "intent_enqueued_mono_ns": enqueued, "action": DecisionAction.ENTRY.value, "reason": reason, "position_id": position_id,
                                                          "quote_state_event_id": quote.quote_state_event_id, "quote_age_ns": quote.quote_age_ns, "quote_snapshot_id": quote.quote_snapshot_id, "arrival_sequence": raw.arrival_sequence}),
                                  ("position", pos),
                                  RowRecord("execution_attempts", self._attempt_row(handle, decision_id, quote, basis, AttemptStatus.SUBMITTED)))))
        self._schedule_research_end(mint, raw.receive_mono_ns)
        self._schedule_features(mint, raw.receive_mono_ns)

    @staticmethod
    def _max_entry_debit(quote: QuoteSnapshot) -> int:
        """sizing_contract.entry_required_available_cash: fee-inclusive max_sol_cost (slippage bound) + network + tip + rent + irrecoverable."""
        return (quote.max_input_base_units or quote.swap_wallet_debit_lamports or 0) + quote.network_fee_total_lamports + quote.tip_lamports + quote.rent_delta_lamports + quote.irrecoverable_cost_lamports

    def _quote_stale(self, quote: QuoteSnapshot, now: int) -> bool:
        return now - quote.created_mono_ns > self.policy.quote_max_age_ns

    def _attempt_row(self, handle: AttemptHandle, decision_id: str, quote: QuoteSnapshot, basis: CostBasis, status: AttemptStatus) -> dict[str, Any]:
        intent = handle.intent
        return {"attempt_id": handle.attempt_id, "run_id": self.run_id, "position_id": intent.position_id, "side": intent.side.value, "attempt_number": intent.attempt_number, "adapter": "PAPER", "status": status.value,
                "submit_mono_ns": handle.submit_mono_ns, "scheduled_fill_mono_ns": handle.scheduled_fill_mono_ns, "callback_actual_mono_ns": None, "modeled_fill_mono_ns": None, "rpc_ack_mono_ns": None, "chain_observed_mono_ns": None,
                "transaction_signature": None, "model_hash": self.cfg.policy_hash, "quote_state_event_id": quote.quote_state_event_id,
                "amount_in_base_units": quote.swap_wallet_debit_lamports if intent.side == Side.BUY else quote.token_amount_base_units,
                "amount_out_base_units": quote.token_amount_base_units if intent.side == Side.BUY else quote.swap_wallet_credit_lamports, "slippage_bps": quote.slippage_limit_bps, "failure_reason": None,
                "fees_charged_lamports": None, "fee_snapshot_id": quote.fee_snapshot_id, "quote_snapshot_id": quote.quote_snapshot_id, "message_template_hash": quote.message_template_hash,
                "token_program": quote.token_program, "account_profile_id": quote.account_profile_id, "account_size_bytes": quote.account_size_bytes, "instruction_name": quote.instruction_name,
                "cost_model_hash": self.cfg.cost_model_hash, "signature_count": quote.signature_count, "compute_unit_limit": quote.compute_unit_limit, "compute_unit_price_micro_lamports": quote.compute_unit_price_micro_lamports,
                "protocol_fee_lamports": quote.protocol_fee_lamports, "creator_fee_lamports": quote.creator_fee_lamports, "other_protocol_fee_lamports": quote.other_protocol_fee_lamports, "base_fee_lamports": quote.base_fee_lamports,
                "priority_fee_lamports": quote.priority_fee_lamports, "network_fee_total_lamports": quote.network_fee_total_lamports, "sender_tip_lamports": quote.tip_lamports,
                "rent_locked_lamports": quote.rent_delta_lamports if intent.side == Side.BUY else 0, "rent_refunded_lamports": -quote.rent_delta_lamports if intent.side == Side.SELL else 0, "irrecoverable_account_cost_lamports": quote.irrecoverable_cost_lamports,
                "cost_evidence_status": quote.cost_evidence_status, "fee_reconciliation_status": "PRECOMPUTED", "entry_curve_input_lamports": quote.curve_input_lamports, "attempt_decision_id": decision_id,
                "fill_evidence_status": FillEvidence.NOT_APPLICABLE.value, "proxy_event_id": None, "proxy_signature": None, "proxy_slot": None, "proxy_receive_sequence": None, "proxy_receive_mono_ns": None, "proxy_age_ns": None,
                "next_observed_event_id_after_modeled_fill": None, "fill_commitment": None}

    # ------------------------------------------------------------------ exposure ticks
    def _position_tick(self, mint: str, receive_mono_ns: int) -> None:
        pos = self.positions[self.position_by_mint[mint]]
        st = self.reducer.states[mint]
        now = self.clock.monotonic_ns()
        if (pos.entry_fill_mono_ns is not None and pos.token_balance_base_units > 0
                and st.creator_sell_event_id is not None and self.strategy.creator_exit_enabled()
                and not any(r == ExitReason.CREATOR_SELL for r, _ in pos.latched_triggers)):
            pos.latched_triggers.append((ExitReason.CREATOR_SELL, now))
            if pos.origin_trigger is None:
                pos.origin_trigger = pick_exit_trigger(pos.latched_triggers)[0]
            self._position_record(pos)
        if st.venue_state != VenueState.BONDING_CURVE_ACTIVE and pos.venue_state != st.venue_state:
            pos.venue_state = st.venue_state
            if pos.state == PositionState.OPEN or pos.exit_quote_recoverable:
                self._unresolved(pos, self._venue_reason(st.venue_state), now)
            elif pos.state in (PositionState.ENTRY_PENDING, PositionState.EXIT_PENDING):
                pos.quality_status = "VENUE_INVALIDATED_WHILE_PENDING"
                self._position_record(pos)
            return
        if pos.state != PositionState.OPEN and not pos.exit_quote_recoverable:
            return
        if st.quote_uncertain:
            self._unresolved(pos, "QUOTE_CONFLICT_ON_OPEN_POSITION", now)
            return
        if self.reducer.gap_open_since is not None or st.coverage_state == CoverageState.GAP or self.reducer.gap_intersects(pos.open_mono_ns or now, now):
            pos.canonical_provider_gap_flag = True
        if st.decode_gap_since is None and not any(r == ExitReason.TARGET for r, _ in pos.latched_triggers):
            curve = self._curve(st)
            if curve is not None:
                nlv = self.engine.net_liquidation_value(curve, self.basis_getter(), now, st.coverage_state, pos.token_balance_base_units, pos.failed_attempt_cost_lamports)
                if self.strategy.target_reached(nlv, pos.entry_economic_cost_lamports, Decimal(pos.target_net_multiple)):
                    pos.latched_triggers.append((ExitReason.TARGET, now))
        self._maybe_exit(pos, st, now)

    def _maybe_exit(self, pos: PositionRecord, st: MarketState, now: int) -> None:
        if (pos.state != PositionState.OPEN and not pos.exit_quote_recoverable) or pos.exit_intent_pending or pos.exhausted or not pos.latched_triggers:
            return
        if pos.retry_not_before_mono_ns is not None and now < pos.retry_not_before_mono_ns:
            return  # retries must advance time: wait for the scheduled backoff timer
        if pos.origin_trigger is None:
            picked = pick_exit_trigger(pos.latched_triggers)
            if picked is None:
                return
            pos.origin_trigger = picked[0]
        curve = self._curve(st)
        basis = self.basis_getter()
        if st.quote_uncertain or st.quote_scope_status != QuoteScopeStatus.SOL_WSOL:
            quote: QuoteSnapshot | InvalidQuote = InvalidQuote("QUOTE_CONFLICT" if st.quote_uncertain else st.quote_scope_status.value)
        elif st.decode_gap_since is not None:
            quote = InvalidQuote("DECODE_GAP")
        else:
            quote = self.engine.quote_sell(curve, basis, now, f"{self.run_id}_", st.coverage_state, pos.token_balance_base_units, pos.locked_rent_lamports) if curve else InvalidQuote("CURVE_UNAVAILABLE")
        self.dec_seq += 1
        decision_id = f"{self.run_id}_d{self.dec_seq}"
        action = DecisionAction.EXIT if pos.exit_attempts == 0 else DecisionAction.EXIT_RETRY
        if isinstance(quote, InvalidQuote):
            # latched: exit intent persists, retried when the next valid state arrives (no fictitious fill)
            decision_done = self.clock.monotonic_ns()
            self._record(RowRecord("decisions", {"decision_id": decision_id, "run_id": self.run_id, "mint": pos.mint, "event_id": st.last_state_event_id, "policy_hash": self.cfg.policy_hash, "receive_mono_ns": st.last_state_receive_mono_ns,
                                                 "data_ready_mono_ns": now, "decision_done_mono_ns": decision_done, "intent_enqueued_mono_ns": None, "action": action.value, "reason": f"EXIT_LATCHED_{quote.reason}", "position_id": pos.position_id,
                                                 "quote_state_event_id": st.last_state_event_id, "quote_age_ns": None, "quote_snapshot_id": None, "arrival_sequence": None}))
            pos.quality_status = f"EXIT_LATCHED_{quote.reason}"
            self._position_record(pos)
            if quote.reason == "VENUE_INVALID":
                self._unresolved(pos, self._venue_reason(st.venue_state), now)
            elif quote.reason in ("QUOTE_CONFLICT", "UNSUPPORTED_QUOTE"):
                self._unresolved(pos, f"{quote.reason}_ON_OPEN_POSITION", now)
            else:
                self._watch_exit_quote(pos, now)
            return
        assert isinstance(quote, QuoteSnapshot)
        decision_done = self.clock.monotonic_ns()
        if self._quote_stale(quote, decision_done):
            pos.quality_status = "EXIT_LATCHED_QUOTE_STALE"
            self._position_record(pos)
            self._record(RowRecord("decisions", {"decision_id": decision_id, "run_id": self.run_id, "mint": pos.mint, "event_id": st.last_state_event_id, "policy_hash": self.cfg.policy_hash, "receive_mono_ns": st.last_state_receive_mono_ns,
                                                 "data_ready_mono_ns": now, "decision_done_mono_ns": decision_done, "intent_enqueued_mono_ns": None, "action": DecisionAction.CANCEL_INTENT.value, "reason": "QUOTE_STALE", "position_id": pos.position_id,
                                                 "quote_state_event_id": quote.quote_state_event_id, "quote_age_ns": decision_done - quote.created_mono_ns, "quote_snapshot_id": None, "arrival_sequence": None}))
            self._watch_exit_quote(pos, now)
            return
        pos.exit_quote_blocked_since_mono_ns = None
        pos.exit_quote_recoverable = False
        pos.unresolved_reason = None
        pos.quality_status = "OK"
        attempt_no = pos.exit_attempts + 1
        intent = Intent(f"{pos.position_id}_x{attempt_no}", pos.position_id, pos.mint, Side.SELL, attempt_no, decision_id, quote, {"protocol": quote.fee_snapshot_id}, self.exit_delay_ns, decision_done, origin_trigger=pos.origin_trigger)
        handle = self.adapter.submit(intent)
        enqueued = self.clock.monotonic_ns()
        pos.exit_attempts = attempt_no
        pos.exit_intent_pending = True
        pos.state = PositionState.EXIT_PENDING
        self._tele("decision_to_enqueue_ns", enqueued - decision_done)
        self.pending_attempts[handle.attempt_id] = {"handle": handle, "basis": basis, "coverage": st.coverage_state}
        self.quote_by_attempt[handle.attempt_id] = quote
        self.attempts_by_position.setdefault(pos.position_id, []).append(handle.attempt_id)
        self._record(RecordGroup((("quote_snapshot", (quote, self.run_id, pos.position_id)),
                                  RowRecord("decisions", {"decision_id": decision_id, "run_id": self.run_id, "mint": pos.mint, "event_id": st.last_state_event_id, "policy_hash": self.cfg.policy_hash, "receive_mono_ns": st.last_state_receive_mono_ns,
                                                          "data_ready_mono_ns": now, "decision_done_mono_ns": decision_done, "intent_enqueued_mono_ns": enqueued, "action": action.value, "reason": pos.origin_trigger.value, "position_id": pos.position_id,
                                                          "quote_state_event_id": quote.quote_state_event_id, "quote_age_ns": quote.quote_age_ns, "quote_snapshot_id": quote.quote_snapshot_id, "arrival_sequence": None}),
                                  RowRecord("execution_attempts", self._attempt_row(handle, decision_id, quote, basis, AttemptStatus.SUBMITTED)),
                                  ("position", pos))))

    def _watch_exit_quote(self, pos: PositionRecord, now: int) -> None:
        """One diagnostic deadline per blocked intent, without paid attempts or
        polling. Later trusted state/cost inputs may recover this blockage only."""
        if pos.exit_quote_blocked_since_mono_ns is not None:
            return
        pos.exit_quote_blocked_since_mono_ns = now
        due = now + self.cfg.connectivity["position_state_stale_ms"] * NS_PER_MS
        self.scheduler.schedule_at(due, lambda actual, pid=pos.position_id, start=now: self._on_exit_quote_deadline(pid, start, actual))

    def _on_exit_quote_deadline(self, position_id: str, start: int, actual: int) -> None:
        pos = self.positions.get(position_id)
        if pos is None or pos.state != PositionState.OPEN or pos.exit_quote_blocked_since_mono_ns != start:
            return
        self._maybe_exit(pos, self.reducer.state_for(pos.mint), actual)
        if pos.state == PositionState.OPEN and pos.exit_quote_blocked_since_mono_ns == start:
            reason = pos.quality_status
            self._unresolved(pos, "EXIT_QUOTE_UNAVAILABLE", actual)
            pos.exit_quote_recoverable = True
            pos.quality_status = reason
            self._position_record(pos)

    @staticmethod
    def _venue_reason(venue: VenueState) -> str:
        return {VenueState.MIGRATION_SUSPECTED: "MIGRATION_SUSPECTED_UNVERIFIED", VenueState.MIGRATED_UNSUPPORTED: "MIGRATED_UNSUPPORTED", VenueState.CURVE_COMPLETE: "CURVE_COMPLETE_UNSUPPORTED"}.get(venue, "VENUE_UNKNOWN")

    def _unresolved(self, pos: PositionRecord, reason: str, now: int) -> None:
        pos.exit_quote_recoverable = False
        if pos.state == PositionState.UNRESOLVED and pos.unresolved_reason == reason:
            return
        pos.state = PositionState.UNRESOLVED
        pos.unresolved_reason = reason
        pos.quality_status = "UNRESOLVED_EXPOSURE"
        self.counters.unresolved += 1
        self._position_record(pos)

    # ------------------------------------------------------------------ outcomes (serial, actual callback time)
    def on_outcome(self, outcome: ExecutionOutcome) -> None:
        try:
            self._on_outcome(outcome)
        except Exception as exc:  # noqa: BLE001
            self.counters.internal_errors += 1
            self.last_error = f"OUTCOME_INTERNAL_ERROR: {type(exc).__name__}: {exc}"
            self._quality("STATE_OWNER_INTERNAL_ERROR", "CRITICAL", "RUN", canonical_gap=False, details={"error": self.last_error[: self.error_text_max]}, lost=None)
            self.stop_new_entries = True

    def _on_outcome(self, outcome: ExecutionOutcome) -> None:
        meta = self.pending_attempts.pop(outcome.attempt_id, None)
        if meta is None:
            return
        intent = outcome.intent
        pos = self.positions[intent.position_id]
        st = self.reducer.state_for(intent.mint)
        modeled_fill = outcome.scheduled_fill_mono_ns
        now = outcome.callback_actual_mono_ns
        self._tele("callback_lateness_ns", max(0, now - modeled_fill))
        basis = meta.get("fill_basis", meta["basis"])  # rule valid at the modeled fill, not at submit (S07)
        entry = self.reducer.proxy_state_at(st, modeled_fill)
        venue_invalid = st.venue_state != VenueState.BONDING_CURVE_ACTIVE and st.venue_change_mono_ns is not None and st.venue_change_mono_ns <= modeled_fill
        fill_state = self._curve(st, entry) if entry else None
        if venue_invalid and fill_state is not None:
            fill_state = replace(fill_state, venue_state=st.venue_state)
        coverage = self._coverage_at(st, entry[0] if entry else modeled_fill, modeled_fill)
        unknowable = self.reducer.unknowable_between(st, entry[0] if entry else outcome.submit_mono_ns, modeled_fill) if not venue_invalid else None
        conflict_at_fill = st.quote_conflict_mono_ns is not None and st.quote_conflict_mono_ns <= modeled_fill
        if conflict_at_fill or (st.quote_scope_status != QuoteScopeStatus.SOL_WSOL and st.quote_conflict_mono_ns is None):
            unknowable = "QUOTE_CONFLICT" if conflict_at_fill else st.quote_scope_status.value
        if unknowable is not None:
            ev = FillEvaluation(AttemptStatus.UNKNOWN, None, None, unknowable, (modeled_fill - entry[0]) if entry else None)
        else:
            ev = evaluate_fill(self.engine, intent.quote, fill_state, basis, modeled_fill, f"{self.run_id}_", coverage, pos.locked_rent_lamports)
        next_event = self.reducer.next_event_after(st, modeled_fill)
        upd: dict[str, Any] = {"status": ev.status.value, "callback_actual_mono_ns": now, "modeled_fill_mono_ns": modeled_fill, "failure_reason": ev.failure_reason, "fees_charged_lamports": ev.fees_charged_lamports,
                               "network_fee_snapshot_id": basis.network.fee_snapshot_id if basis.network else None, "rent_fee_snapshot_id": basis.rent.fee_snapshot_id if basis.rent else None,
                               "priority_fee_snapshot_id": basis.priority_policy.fee_snapshot_id if basis.priority_policy else None, "fee_snapshot_id": basis.protocol.fee_snapshot_id if basis.protocol else None,
                               "fee_reconciliation_status": "APPLIED_AT_MODELED_FILL" if ev.status != AttemptStatus.UNKNOWN else "UNRESOLVED_UNKNOWN", "decoder_version": self.decoder.version,
                               "fill_evidence_status": (FillEvidence.OBSERVED_STATE_PROXY if ev.status != AttemptStatus.UNKNOWN else FillEvidence.UNKNOWN).value,
                               "proxy_event_id": fill_state.event_id if fill_state else None, "proxy_signature": fill_state.signature if fill_state else None, "proxy_slot": fill_state.slot if fill_state else None,
                               "proxy_receive_sequence": fill_state.receive_sequence if fill_state else None, "proxy_receive_mono_ns": fill_state.receive_mono_ns if fill_state else None, "proxy_age_ns": ev.proxy_age_ns,
                               "next_observed_event_id_after_modeled_fill": next_event, "fill_commitment": "PROCESSED_PROXY" if ev.status != AttemptStatus.UNKNOWN else None}
        if ev.actual is not None:
            q = ev.actual
            fill_q = QuoteSnapshot(**{**{k: getattr(q, k) for k in q.__slots__}, "quote_snapshot_id": f"{outcome.attempt_id}_fill"})  # type: ignore[attr-defined]
            self._record(("quote_snapshot", (fill_q, self.run_id, pos.position_id)))
            upd["fill_quote_snapshot_id"] = fill_q.quote_snapshot_id
            upd.update({k: getattr(q, k) for k in ("token_program", "account_profile_id", "account_size_bytes", "message_template_hash", "instruction_name")})
            upd.update({"network_fee_total_lamports": q.network_fee_total_lamports, "base_fee_lamports": q.base_fee_lamports, "priority_fee_lamports": q.priority_fee_lamports, "sender_tip_lamports": q.tip_lamports,
                        "compute_unit_price_micro_lamports": q.compute_unit_price_micro_lamports, "rent_locked_lamports": q.rent_delta_lamports if intent.side == Side.BUY else 0,
                        "rent_refunded_lamports": -q.rent_delta_lamports if intent.side == Side.SELL else 0, "irrecoverable_account_cost_lamports": q.irrecoverable_cost_lamports, "slippage_bps": q.slippage_limit_bps})
            upd.update({"amount_in_base_units": q.swap_wallet_debit_lamports if intent.side == Side.BUY else q.token_amount_base_units, "amount_out_base_units": q.token_amount_base_units if intent.side == Side.BUY else q.swap_wallet_credit_lamports,
                        "protocol_fee_lamports": q.protocol_fee_lamports, "creator_fee_lamports": q.creator_fee_lamports, "other_protocol_fee_lamports": q.other_protocol_fee_lamports, "entry_curve_input_lamports": q.curve_input_lamports,
                        "cost_evidence_status": q.cost_evidence_status})
        elif ev.status == AttemptStatus.UNKNOWN:
            upd.update({"cost_evidence_status": "UNKNOWN", "network_fee_total_lamports": None, "base_fee_lamports": None,
                        "priority_fee_lamports": None, "sender_tip_lamports": None, "protocol_fee_lamports": None,
                        "creator_fee_lamports": None, "other_protocol_fee_lamports": None, "rent_locked_lamports": None,
                        "rent_refunded_lamports": None, "irrecoverable_account_cost_lamports": None})
        else:
            terms = fee_terms(basis, self.policy, modeled_fill, fill_state, intent.side)
            assert not isinstance(terms, InvalidQuote)
            upd.update({k: getattr(terms, k) for k in ("token_program", "account_profile_id", "account_size_bytes", "message_template_hash", "instruction_name")})
            network = ev.fees_charged_lamports - self.policy.sender_tip_lamports
            upd.update({"cost_evidence_status": ev.cost_evidence_status, "network_fee_total_lamports": network,
                        "base_fee_lamports": terms.base_fee_lamports, "priority_fee_lamports": network - terms.base_fee_lamports,
                        "sender_tip_lamports": self.policy.sender_tip_lamports, "protocol_fee_lamports": 0, "creator_fee_lamports": 0,
                        "other_protocol_fee_lamports": 0, "rent_locked_lamports": 0, "rent_refunded_lamports": 0,
                        "irrecoverable_account_cost_lamports": 0, "compute_unit_price_micro_lamports": terms.compute_unit_price_micro_lamports})
        self._record(UpdateRecord("execution_attempts", "attempt_id", outcome.attempt_id, upd))
        if intent.side == Side.BUY:
            self._entry_outcome(pos, st, outcome, ev, now, modeled_fill)
        else:
            self._exit_outcome(pos, st, outcome, ev, now, modeled_fill)

    def _entry_outcome(self, pos: PositionRecord, st: MarketState, outcome: ExecutionOutcome, ev: Any, now: int, modeled_fill: int) -> None:
        required = pos.reserved_cash_lamports
        if ev.status == AttemptStatus.FILLED:
            q: QuoteSnapshot = ev.actual
            self.portfolio.reserved_lamports -= required
            self.portfolio.cash_lamports += required - q.entry_cash_debit - pos.exit_reserve_lamports
            self.portfolio.reserved_lamports += pos.exit_reserve_lamports
            self.portfolio.locked_rent_lamports += q.rent_delta_lamports
            self.portfolio.ledger_cash_sum -= q.entry_cash_debit
            pos.reserved_cash_lamports = pos.exit_reserve_lamports
            pos.locked_rent_lamports = q.rent_delta_lamports
            pos.entry_economic_cost_lamports = q.entry_economic_cost
            pos.entry_cash_debit_lamports = q.entry_cash_debit
            pos.total_fees_lamports += q.protocol_fee_total_lamports + q.network_fee_total_lamports + q.tip_lamports
            pos.protocol_fee_lamports += q.protocol_fee_total_lamports
            pos.network_fee_lamports += q.network_fee_total_lamports
            pos.tip_lamports += q.tip_lamports
            pos.token_balance_base_units = q.token_amount_base_units
            pos.state = PositionState.OPEN
            pos.open_mono_ns = modeled_fill
            pos.entry_fill_mono_ns = modeled_fill
            pos.entry_fill_attempt_id = outcome.attempt_id
            pos.timeout_due_mono_ns = modeled_fill + self.timeout_ns
            self.counters.entry_fills += 1
            self._record(RecordGroup((book_entry_fill(self.run_id, pos.position_id, outcome.attempt_id, modeled_fill, self.clock.utc_ns(), q), ("position", pos))))
            self.scheduler.schedule_at(pos.timeout_due_mono_ns, lambda actual, pid=pos.position_id: self._on_timeout(pid, actual))
            # latched creator sells received between submit and this (possibly late) callback are handled now, at actual processing time
            self._position_tick(pos.mint, now)
        elif ev.status == AttemptStatus.UNKNOWN:
            pos.canonical_provider_gap_flag = pos.canonical_provider_gap_flag or ev.failure_reason in ("COVERAGE_GAP_AT_MODELED_FILL", "CANONICAL_GAP", "DECODE_GAP", "UNDECODABLE_FRAME_IN_WINDOW")
            self._unresolved(pos, "ATTEMPT_STATUS_UNKNOWN", now)
            pos.quality_status = f"UNKNOWN_{ev.failure_reason}"
            self._position_record(pos)
        else:
            fees = ev.fees_charged_lamports or 0
            self.portfolio.reserved_lamports -= required
            self.portfolio.cash_lamports += required - fees
            self.portfolio.ledger_cash_sum -= fees
            pos.reserved_cash_lamports = 0
            pos.failed_attempt_cost_lamports += fees
            pos.total_fees_lamports += fees
            pos.network_fee_lamports += fees
            pos.state = PositionState.ENTRY_FAILED
            pos.close_mono_ns = now
            pos.net_pnl_lamports = -fees
            pos.research_status = "ENTRY_FAILED"
            self.counters.entry_failed += 1
            self._record(book_failed_attempt(self.run_id, pos.position_id, outcome.attempt_id, modeled_fill, self.clock.utc_ns(), Side.BUY, fees, ev.status, ev.cost_evidence_status or "UNKNOWN"))
            self._position_record(pos)

    def _exit_outcome(self, pos: PositionRecord, st: MarketState, outcome: ExecutionOutcome, ev: Any, now: int, modeled_fill: int) -> None:
        pos.exit_intent_pending = False
        if ev.status == AttemptStatus.FILLED:
            q: QuoteSnapshot = ev.actual
            credit = q.sell_cash_credit
            self.portfolio.cash_lamports += credit + pos.reserved_cash_lamports
            self.portfolio.reserved_lamports -= pos.reserved_cash_lamports
            self.portfolio.locked_rent_lamports += q.rent_delta_lamports
            self.portfolio.ledger_cash_sum += credit
            pos.reserved_cash_lamports = 0
            pos.locked_rent_lamports += q.rent_delta_lamports
            pos.token_balance_base_units -= q.token_amount_base_units
            pos.total_fees_lamports += q.protocol_fee_total_lamports + q.network_fee_total_lamports + q.tip_lamports
            pos.protocol_fee_lamports += q.protocol_fee_total_lamports
            pos.network_fee_lamports += q.network_fee_total_lamports
            pos.tip_lamports += q.tip_lamports
            pos.realized_credit_lamports += credit
            pos.state = PositionState.CLOSED
            pos.close_mono_ns = modeled_fill
            pos.exit_reason = pos.origin_trigger
            pos.net_pnl_lamports = -pos.entry_cash_debit_lamports - pos.failed_attempt_cost_lamports + credit + pos.locked_rent_lamports
            pos.research_status = "CLOSED" if pos.research_end_mono_ns is None or modeled_fill <= pos.research_end_mono_ns else "CLOSED_AFTER_HORIZON"
            self.counters.exit_fills += 1
            if pos.exit_reason == ExitReason.CREATOR_SELL:
                self.counters.exits_creator += 1
            elif pos.exit_reason == ExitReason.TARGET:
                self.counters.exits_target += 1
            elif pos.exit_reason == ExitReason.TIMEOUT:
                self.counters.exits_timeout += 1
            self._record(RecordGroup((book_exit_fill(self.run_id, pos.position_id, outcome.attempt_id, modeled_fill, self.clock.utc_ns(), q), ("position", pos))))
            return
        if ev.status == AttemptStatus.UNKNOWN:
            self._unresolved(pos, "ATTEMPT_STATUS_UNKNOWN", now)
            pos.quality_status = f"UNKNOWN_{ev.failure_reason}"
            self._position_record(pos)
            return
        fees = ev.fees_charged_lamports or 0
        self.portfolio.reserved_lamports -= fees
        self.portfolio.ledger_cash_sum -= fees
        pos.reserved_cash_lamports -= fees
        pos.failed_attempt_cost_lamports += fees
        pos.total_fees_lamports += fees
        pos.network_fee_lamports += fees
        self.counters.exit_failed_attempts += 1
        self._record(book_failed_attempt(self.run_id, pos.position_id, outcome.attempt_id, modeled_fill, self.clock.utc_ns(), Side.SELL, fees, ev.status, ev.cost_evidence_status or "UNKNOWN"))
        if ev.status == AttemptStatus.VENUE_INVALID:
            self._unresolved(pos, self._venue_reason(st.venue_state), now)
            return
        retry_at = next_retry_submit_mono_ns(self.policy, pos.exit_attempts, now)
        if retry_at is None:
            pos.exhausted = True
            self.counters.exit_exhausted += 1
            self._unresolved(pos, "EXIT_RETRIES_EXHAUSTED", now)
            return
        pos.state = PositionState.OPEN
        pos.retry_not_before_mono_ns = retry_at
        self._position_record(pos)
        self.scheduler.schedule_at(retry_at, lambda actual, pid=pos.position_id: self._on_retry(pid, actual))

    def _on_retry(self, position_id: str, actual: int) -> None:
        pos = self.positions[position_id]
        if pos.state != PositionState.OPEN:
            return
        self._maybe_exit(pos, self.reducer.state_for(pos.mint), actual)

    def _on_timeout(self, position_id: str, actual: int) -> None:
        pos = self.positions[position_id]
        if pos.state != PositionState.OPEN or pos.exit_intent_pending:
            return
        if not any(r == ExitReason.TIMEOUT for r, _ in pos.latched_triggers):
            pos.latched_triggers.append((ExitReason.TIMEOUT, actual))
        self._maybe_exit(pos, self.reducer.state_for(pos.mint), actual)

    # ------------------------------------------------------------------ research horizon and features (background)
    def _schedule_research_end(self, mint: str, create_receive: int) -> None:
        self.scheduler.schedule_at(create_receive + self.horizon_ns, lambda actual, m=mint: self._on_research_end(m, actual))

    def _on_research_end(self, mint: str, actual: int) -> None:
        pid = self.position_by_mint.get(mint)
        if pid is not None:
            pos = self.positions[pid]
            if pos.state not in (PositionState.CLOSED, PositionState.ENTRY_FAILED, PositionState.INELIGIBLE):
                pos.research_status = "CENSORED"
                self.counters.censored += 1
                self._position_record(pos)  # no booking, no state change: tracker stays active (F08)

    def _schedule_features(self, mint: str, create_receive: int) -> None:
        if mint in self.feature_jobs_scheduled or not self.feature_offsets_ns or not self.compute_features:
            return
        self.feature_jobs_scheduled.add(mint)
        self.scheduler.schedule_at(create_receive + max(self.feature_offsets_ns) + 1, lambda actual, m=mint, t0=create_receive: self._compute_features(m, t0, actual))

    def _compute_features(self, mint: str, t0: int, actual: int) -> None:
        st = self.reducer.states.get(mint)
        if st is None:
            return
        log = list(st.trade_log)
        truncated_from = st.trade_log_truncated_from_mono_ns
        gap_intervals = list(self.reducer.gap_intervals)
        decode_gaps = list(st.decode_gap_intervals)
        create = (st.initial_virtual_sol_reserves, st.initial_virtual_token_reserves, st.initial_real_token_reserves, st.initial_buy_tokens, st.token_total_supply)
        health = list(self.health_history)
        primary = sorted(self.eligible_trade_feeds)[:1]  # V1 legacy = single-provider view of the first eligible feed
        quote_amount = self.cfg.strategy["entry_amount_lamports"]
        rows: list[Any] = []

        def v1_gap_at(cutoff: int) -> bool | None:
            """Was the single V1 provider disconnected at any time in [t0, cutoff]? Derived from recorded health transitions only."""
            if not primary or not health:
                return None
            feed = primary[0]
            ready = None
            gap = False
            for mono, fid, state in health:
                if fid != feed:
                    continue
                if mono <= t0:
                    ready = state
                elif mono <= cutoff:
                    if not state:
                        gap = True
                    ready = state
            if ready is False:
                gap = True
            return gap

        def job() -> None:
            for offset in self.feature_offsets_ns:
                cutoff = t0 + offset
                truncated = truncated_from is not None and truncated_from <= cutoff
                events = tuple(FeatureEvent(t, r, a, s, vs, vt, rt) for t, r, a, s, vs, vt, rt, in_create, _ in log if r <= cutoff and not in_create)
                intersects = any((ge is None or ge >= t0) and gs <= cutoff for gs, ge in gap_intervals) or any((ge is None or ge >= t0) and gs <= cutoff for gs, ge in decode_gaps)
                coverage = CoverageState.GAP.value if intersects else CoverageState.OBSERVED.value  # historical: derived from the gap intervals up to the cutoff, never from later state
                v1_gap = v1_gap_at(cutoff)
                for fs in ((FEATURE_SET_V2, FEATURE_SET_V1) if self.cfg.research["legacy_feature_export_enabled"] else (FEATURE_SET_V2,)):
                    inp = FeatureInputs(t0, cutoff, create[0], create[1], create[1], create[2], create[3], create[4], quote_amount, events, coverage, intersects, v1_gap)
                    values, nulls = self.features.compute(inp, fs, history_complete=not truncated)
                    rows.append(RowRecord("feature_snapshots", {"feature_snapshot_id": f"{self.run_id}_f_{mint}_{fs}_{offset}", "run_id": self.run_id, "mint": mint, "feature_set_id": fs, "feature_contract_hash": self.cfg.feature_contract_hash,
                                                                "cutoff_mono_ns": cutoff, "computed_mono_ns": actual, "source_event_ids_json": [e[8] for e in log if e[1] <= cutoff and not e[7]], "features_json": values, "null_reasons_json": nulls,
                                                                "coverage_version": "CANONICAL_COVERAGE_V2" if fs == FEATURE_SET_V2 else "PROVIDER_V1",
                                                                "quality_status": "TRUNCATED_TRADE_LOG" if truncated else ("OK" if not intersects else "GAP_INTERSECTS")}))
            for r in rows:
                self.recorder.try_append(r)

        self.feature_sink(job)

    # ------------------------------------------------------------------ lifecycle
    def finalize(self, stop_mono_ns: int) -> None:
        """End of run: no new entries; pending outcomes are not invented; exposures reported as unresolved/censored."""
        self.stop_new_entries = True
        for observation in getattr(self.strategy, "observations", {}).values():
            self.record_observation(observation, None, None)
            self._record(UpdateRecord("strategy_observations", "observation_id", f"{self.run_id}_obs_{observation.mint}", {"status": "CENSORED_RUN_END"}))
        if self.decode_issues:
            self._quality("DECODE_GAP_SUMMARY", "WARNING", "RUN", canonical_gap=False, details={"partial_or_undecodable_frames_by_feed": dict(self.decode_issues), "note": "frames with unknown/undecodable Pump events; affected creates were not actionable"}, lost=sum(self.decode_issues.values()), recovery="RUN_END")
        for attempt_id, meta in list(self.pending_attempts.items()):
            self._record(UpdateRecord("execution_attempts", "attempt_id", attempt_id, {"status": AttemptStatus.UNKNOWN.value, "failure_reason": "RUN_ENDED_BEFORE_CALLBACK", "fill_evidence_status": FillEvidence.UNKNOWN.value, "cost_evidence_status": "UNKNOWN"}))
        for pos in self.positions.values():
            if pos.state in (PositionState.OPEN, PositionState.EXIT_PENDING, PositionState.ENTRY_PENDING, PositionState.ENTRY_INTENT):
                pos.quality_status = "OPEN_AT_RUN_END"
                if pos.research_status == "OPEN":
                    pos.research_status = "CENSORED_RUN_END"
                    self.counters.censored += 1
                self._position_record(pos)
        self.reducer.evict_inactive(stop_mono_ns, 0, set(self.position_by_mint))

    def maybe_evict(self, now_mono_ns: int) -> int:
        """Bounded memory: drop inactive mint trackers (never active positions, never during their research horizon)."""
        active = {p.mint for p in self.positions.values() if p.state not in (PositionState.CLOSED, PositionState.ENTRY_FAILED, PositionState.INELIGIBLE)}
        protected = active | set(getattr(self.strategy, "observations", {})) | {m for m, t in self.first_create_receive.items() if now_mono_ns - t <= self.horizon_ns}
        removed = self.reducer.evict_inactive(now_mono_ns, self.cfg.storage["inactive_mint_retention_ms"] * NS_PER_MS, protected)
        for mint in list(self.position_by_mint):
            if mint not in self.reducer.states:  # closed/failed position whose tracker was evicted: record stays in self.positions (persisted), index only
                self.position_by_mint.pop(mint, None)
        for m in list(self.first_create_receive):
            if m not in self.reducer.states:
                self.first_create_receive.pop(m, None)
                self.feature_jobs_scheduled.discard(m)
                if hasattr(self.strategy, "decided"):
                    self.strategy.decided.discard(m)
        return removed

    def status(self) -> dict[str, Any]:
        c = self.counters
        return {"raw": c.raw_messages, "creates": c.creates, "actionable": c.creates_actionable, "intents": c.entry_intents, "fills": c.entry_fills, "entry_failed": c.entry_failed, "exit_fills": c.exit_fills,
                "unresolved": c.unresolved, "open": sum(1 for p in self.positions.values() if p.state in (PositionState.OPEN, PositionState.EXIT_PENDING, PositionState.ENTRY_PENDING)),
                "cash": self.portfolio.cash_lamports, "reserved": self.portfolio.reserved_lamports, "locked_rent": self.portfolio.locked_rent_lamports,
                "realized_pnl": sum(p.net_pnl_lamports for p in self.positions.values() if p.net_pnl_lamports is not None), "gap_open": self.reducer.gap_open_since is not None,
                "recorder_depth": self.recorder.depth(), "recorder_overflow": self.recorder.overflow_count, "feeds": dict(self.feed_ready), "ineligible": dict(c.ineligible), "internal_errors": c.internal_errors, "last_error": self.last_error}

    def cash_reconciliation(self) -> dict[str, int | bool]:
        initial = self.initial_capital_lamports
        expected = initial + self.portfolio.ledger_cash_sum
        actual = self.portfolio.cash_lamports + self.portfolio.reserved_lamports
        return {"initial": initial, "ledger_cash_sum": self.portfolio.ledger_cash_sum, "cash_plus_reserved": actual, "reconciled": expected == actual, "locked_rent": self.portfolio.locked_rent_lamports}
