"""Deterministic replay of recorded inputs and the recorded time/callback schedule through the SAME
StateOwner/PAPER adapter with ReplayClock (spec: replay.py). No own creator/target/retry logic.

Knowledge rule: at model time t only fee snapshots received <= t are visible (historical costs, T27);
later events never change earlier decisions (T20).
"""

from __future__ import annotations

import sqlite3
from dataclasses import dataclass, field
from decimal import Decimal
from typing import Any

from darkbot.clock import ReplayClock, ReplayScheduler
from darkbot.config import RuntimeConfig, canonical_json, sha256_text
from darkbot.domain import CostEvidence, FeeSnapshot, RawEnvelope, utc_iso
from darkbot.execution import ExecutionPolicy, QuoteEngine
from darkbot.fees import FeeCache
from darkbot.paper import PaperExecutionAdapter, StateOwner
from darkbot.storage import Recorder, SqliteWriter, upsert_config_snapshot


@dataclass
class ReplayInput:
    source_run_id: str
    epoch_id: str
    start_mono_ns: int
    start_utc_ns: int
    observations: list[tuple[int, int, str, str, str, str]]  # (receive_mono_ns, arrival_sequence, feed_id, connection_id, receive_utc, payload)
    fee_snapshots: list[FeeSnapshot]
    gap_intervals: list[tuple[int, int | None]]
    lateness: dict[str, int]  # attempt key (position suffix + side + number) -> callback lateness ns
    dataset_hash: str
    end_mono_ns: int  # recorded run end (runs.ended_mono_ns) or, if missing, the last observation


def load_replay_input(conn: sqlite3.Connection, run_id: str, cfg: RuntimeConfig) -> ReplayInput:
    conn.row_factory = sqlite3.Row
    run = conn.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
    if run is None:
        raise ValueError(f"run {run_id} not found")
    obs = conn.execute("SELECT receive_mono_ns, arrival_sequence, feed_id, connection_id, receive_utc, raw_payload FROM feed_observations WHERE run_id=? AND raw_payload IS NOT NULL ORDER BY receive_mono_ns, arrival_sequence, feed_id", (run_id,)).fetchall()
    observations = [(r["receive_mono_ns"], r["arrival_sequence"], r["feed_id"], r["connection_id"], r["receive_utc"], r["raw_payload"]) for r in obs]
    fees = []
    refresh = cfg.costs["snapshot_refresh_policy"]
    max_age = {"PROTOCOL_GLOBAL_ACCOUNT": refresh["protocol_max_age_ms"], "NETWORK_FEE": refresh["network_fee_max_age_ms"], "RENT_EXEMPTION": refresh["rent_max_age_ms"], "PRIORITY_POLICY": refresh["network_fee_max_age_ms"]}
    for r in conn.execute("SELECT * FROM fee_snapshots WHERE run_id=? ORDER BY received_mono_ns", (run_id,)):
        import json

        norm = json.loads(r["normalized_fee_components"])
        evidence = CostEvidence.UNKNOWN if r["validation_status"] != "VALID" else CostEvidence.MEASURED
        fees.append(FeeSnapshot(r["fee_snapshot_id"], r["source_type"], r["source_provider"], r["source_method"], tuple(json.loads(r["source_account_addresses"])), r["source_slot"], r["observed_utc"], r["received_mono_ns"], r["commitment"],
                                r["protocol_program_id"], r["idl_version_or_hash"], json.loads(r["raw_values"]), norm, r["rounding_rules"], r["applicability"], r["freshness_policy_hash"], r["validation_status"], r["config_hash"],
                                max_age[r["source_type"]] * 1_000_000, evidence))
    gaps = [(q["start_mono_ns"], q["end_mono_ns"]) for q in conn.execute("SELECT start_mono_ns, end_mono_ns FROM quality_events WHERE run_id=? AND kind IN ('CANONICAL_GAP','CANONICAL_GAP_RECOVERED_STATE_ONLY') ORDER BY start_mono_ns", (run_id,))]
    lateness: dict[str, int] = {}
    for a in conn.execute("SELECT attempt_id, position_id, side, attempt_number, scheduled_fill_mono_ns, callback_actual_mono_ns FROM execution_attempts WHERE run_id=? AND callback_actual_mono_ns IS NOT NULL", (run_id,)):
        key = f"{a['position_id'].split('_p')[-1]}:{a['side']}:{a['attempt_number']}"
        lateness[key] = max(0, a["callback_actual_mono_ns"] - a["scheduled_fill_mono_ns"])
    hashes = [r[0] for r in conn.execute("SELECT raw_payload_hash FROM feed_observations WHERE run_id=? ORDER BY receive_mono_ns, arrival_sequence, feed_id", (run_id,))]
    dataset_hash = sha256_text(canonical_json({"run": run_id, "n": len(hashes), "hashes": hashes, "decoder": cfg.protocol["decoder_version"], "coverage": "CANONICAL_COVERAGE_V2"}))
    start = observations[0][0] if observations else 0
    last_obs = observations[-1][0] if observations else 0
    ended = run["ended_mono_ns"] if "ended_mono_ns" in run.keys() else None
    end = max(last_obs, ended) if ended is not None else last_obs
    return ReplayInput(run_id, run["host_clock_epoch_id"], start, 0, observations, fees, gaps, lateness, dataset_hash, end)


@dataclass
class ReplayResult:
    run_id: str
    owner: StateOwner
    digest: dict[str, Any] = field(default_factory=dict)


def _lateness_key(attempt_id: str) -> str:
    # attempt_id = {run}_p{n}_{buy|sell}{k}
    tail = attempt_id.split("_p")[-1]
    n, rest = tail.split("_", 1)
    side = "BUY" if rest.startswith("buy") else "SELL"
    return f"{n}:{side}:{rest[3:] if side == 'BUY' else rest[4:]}"


def run_replay(conn: sqlite3.Connection, replay_input: ReplayInput, cfg: RuntimeConfig, new_run_id: str, campaign_id: str, *, mode: str = "REPLAY", variant_id: str | None = None,
               persist: bool = True, persist_observations: bool = False, compute_features: bool = False, end_mono_ns: int | None = None, drain_ns: int | None = None,
               stress_fee_multiplier: Decimal = Decimal(1), stress_adverse_bps: int = 0, mint_filter: set[str] | None = None) -> ReplayResult:
    """Replay the frozen dataset through StateOwner; returns the owner (RAM truth) and persists rows when requested."""
    clock = ReplayClock(f"replay_{new_run_id}", replay_input.start_mono_ns, replay_input.start_utc_ns)
    scheduler = ReplayScheduler()
    recorder = Recorder(cfg.storage["queue_capacity_records"])
    writer = SqliteWriter(conn, recorder, cfg.storage["batch_max_records"], cfg.storage["flush_interval_ms"], lambda m: None) if persist else None
    if persist:
        for kind, (h, content) in cfg.snapshot_kinds().items():
            upsert_config_snapshot(conn, kind, h, content, "replay", utc_iso(0))
        conn.execute("INSERT OR IGNORE INTO runs(run_id,campaign_id,mode,started_utc,code_revision,spec_hash,config_hash,host_clock_epoch_id,status,quality_status,start_balance_lamports,parent_run_id,dataset_hash) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
                     (new_run_id, campaign_id, mode, utc_iso(0), "replay", "", cfg.config_hash, clock.epoch_id, "RUNNING", "OK", cfg.portfolio["initial_paper_balance_lamports"], replay_input.source_run_id, replay_input.dataset_hash))
        for s in replay_input.fee_snapshots:
            recorder.try_append(("fee_snapshot", (FeeSnapshot(**{**{k: getattr(s, k) for k in s.__slots__}, "fee_snapshot_id": f"{new_run_id}_{s.fee_snapshot_id}"}), new_run_id)))
    fee_cache = FeeCache()
    fee_iter = iter(sorted(replay_input.fee_snapshots, key=lambda s: s.received_mono_ns))
    next_fee = next(fee_iter, None)
    prefix = new_run_id if persist else replay_input.source_run_id

    def apply_fees_until(t: int) -> None:
        nonlocal next_fee
        while next_fee is not None and next_fee.received_mono_ns <= t:
            renamed = FeeSnapshot(**{**{k: getattr(next_fee, k) for k in next_fee.__slots__}, "fee_snapshot_id": f"{new_run_id}_{next_fee.fee_snapshot_id}"}) if persist else next_fee
            setattr(fee_cache, {"PROTOCOL_GLOBAL_ACCOUNT": "protocol", "NETWORK_FEE": "network", "RENT_EXEMPTION": "rent", "PRIORITY_POLICY": "priority_policy"}[next_fee.source_type], renamed)
            next_fee = next(fee_iter, None)

    lateness = replay_input.lateness
    adapter = PaperExecutionAdapter(clock, scheduler, "PAPER", False, cfg.latency_model["entry_delay_ms"] * 1_000_000, cfg.latency_model["exit_delay_ms"] * 1_000_000, lateness_provider=lambda aid: lateness.get(_lateness_key(aid), 0))
    engine = QuoteEngine(ExecutionPolicy.from_config(cfg), stress_fee_multiplier, stress_adverse_bps)
    owner = StateOwner(cfg, new_run_id, clock, scheduler, recorder, adapter, fee_cache.basis, "PAPER", variant_id=variant_id, canonical_prefix=replay_input.source_run_id if not persist_observations else None,
                       persist_observations=persist_observations, compute_features=compute_features, engine=engine, mint_allowlist=mint_filter)
    for f in cfg.feeds:
        owner.on_feed_health(f.id, True, replay_input.start_mono_ns)
    gaps = list(replay_input.gap_intervals)
    gi = 0
    apply_fees_until(replay_input.start_mono_ns)
    stop_at = end_mono_ns if end_mono_ns is not None else replay_input.end_mono_ns

    def fire_until(t: int, strict: bool = False) -> None:
        due = scheduler.next_due_ns()
        while due is not None and (due < t if strict else due <= t):
            clock.advance_to(max(clock.monotonic_ns(), due))
            apply_fees_until(clock.monotonic_ns())
            scheduler.fire_due(due, clock.monotonic_ns())
            due = scheduler.next_due_ns()

    for receive, seq, feed_id, conn_id, utc, payload in replay_input.observations:
        if receive > stop_at:
            break
        while gi < len(gaps) and gaps[gi][0] <= receive:
            owner.reducer.open_gap(gaps[gi][0])
            if gaps[gi][1] is not None and gaps[gi][1] <= receive:
                owner.reducer.close_gap(gaps[gi][1])
            gi += 1
        fire_until(receive, strict=True)  # same model time: market/venue state first, then scheduled fills
        clock.advance_to(max(clock.monotonic_ns(), receive))
        apply_fees_until(receive)
        owner.on_raw(RawEnvelope(feed_id, conn_id, receive, utc, seq, payload))
        if seq % 5000 == 0:
            owner.maybe_evict(receive)
    # Model time never runs past the recorded window: timers beyond it are not fired (no invented fills, exposures stay censored).
    # drain_ns can only extend within the recording; a research embargo beyond it is a no-op (positions crossing the end are purged/censored).
    horizon = stop_at if drain_ns is None else min(stop_at + drain_ns, replay_input.end_mono_ns)
    fire_until(horizon)
    clock.advance_to(max(clock.monotonic_ns(), horizon))
    owner.finalize(clock.monotonic_ns())
    if writer is not None:
        writer.flush_sync()
        conn.execute("UPDATE runs SET status='COMPLETED', ended_utc=?, ended_mono_ns=?, end_balance_lamports=?, quality_status=? WHERE run_id=?", (utc_iso(0), clock.monotonic_ns(), owner.portfolio.cash_lamports + owner.portfolio.reserved_lamports, "OK" if not owner.counters.internal_errors else "INTERNAL_ERROR", new_run_id))
    return ReplayResult(new_run_id, owner, digest(owner))


def digest(owner: StateOwner) -> dict[str, Any]:
    """Comparable summary of decisions/positions/ledger for parity checks (T19/T40/T45)."""
    positions = []
    for p in sorted(owner.positions.values(), key=lambda p: p.position_id):
        positions.append({"mint": p.mint, "state": p.state.value, "exit_reason": p.exit_reason.value if p.exit_reason else None, "origin_trigger": p.origin_trigger.value if p.origin_trigger else None,
                          "net_pnl": p.net_pnl_lamports, "tokens": p.token_balance_base_units, "entry_cost": p.entry_economic_cost_lamports, "fees": p.total_fees_lamports, "exit_attempts": p.exit_attempts,
                          "unresolved_reason": p.unresolved_reason, "research_status": p.research_status, "open_mono_ns": p.open_mono_ns, "close_mono_ns": p.close_mono_ns})
    attempts = [{"id": h.attempt_id.split("_p")[-1], "side": h.intent.side.value, "n": h.intent.attempt_number, "submit": h.submit_mono_ns, "scheduled": h.scheduled_fill_mono_ns, "quote_state": h.intent.quote.quote_state_event_id.split("|", 1)[-1]} for h in owner.adapter.submitted]
    return {"positions": positions, "attempts": attempts, "cash": owner.portfolio.cash_lamports, "reserved": owner.portfolio.reserved_lamports, "locked_rent": owner.portfolio.locked_rent_lamports,
            "ledger_cash_sum": owner.portfolio.ledger_cash_sum, "counters": {k: v for k, v in owner.counters.__dict__.items() if k != "quality_events"}}
