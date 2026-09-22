"""Deterministic replay of recorded inputs and the recorded time/callback schedule through the SAME
StateOwner/PAPER adapter with ReplayClock (spec: replay.py). No own creator/target/retry logic.

Knowledge rule: at model time t only fee snapshots received <= t are visible (historical costs, T27);
later events never change earlier decisions (T20).
"""

from __future__ import annotations

import sqlite3
import json
from dataclasses import dataclass, field
from pathlib import Path
from decimal import Decimal
from datetime import datetime, timezone
from itertools import groupby
from typing import Any

from darkbot.clock import ReplayClock, ReplayScheduler
from darkbot.config import RuntimeConfig, canonical_json, sha256_text
from darkbot.domain import CostEvidence, FeeSnapshot, RawEnvelope, utc_iso
from darkbot.execution import ExecutionPolicy, QuoteEngine
from darkbot.fees import FEE_KINDS, FeeCache, select_cost_basis
from darkbot.fx import FxSnapshot
from darkbot.paper import PaperExecutionAdapter, StateOwner
from darkbot.storage import Recorder, SqliteWriter, upsert_config_snapshot

DATASET_SCHEMA_VERSION = "REPLAY_DATASET_V4_SOURCE_FRESHNESS_POLICY"
SOURCE_CONFIG_KINDS = {"COST_MODEL", "FRESHNESS_POLICY"}


def _fee_source_config_refs(fees) -> set[tuple[str, str]]:
    refs: set[tuple[str, str]] = set()
    for fee in fees:
        if not fee.config_hash or not fee.freshness_policy_hash:
            raise ValueError("DATASET_INTEGRITY_VIOLATION: fee evidence missing source config reference")
        refs.add((fee.config_hash, "COST_MODEL"))
        refs.add((fee.freshness_policy_hash, "FRESHNESS_POLICY"))
    return refs


def _validate_source_configs(required_refs: set[tuple[str, str]], rows):
    """Validate all source configs referenced by fee evidence."""
    by_hash: dict[str, tuple[str, str]] = {}
    for row in rows:
        h, kind, content = row[:3]
        try:
            normalized = canonical_json(json.loads(content))
        except (ValueError, TypeError) as exc:
            raise ValueError("DATASET_INTEGRITY_VIOLATION: config snapshot invalid JSON") from exc
        if kind not in SOURCE_CONFIG_KINDS or sha256_text(normalized) != h:
            raise ValueError(f"DATASET_INTEGRITY_VIOLATION: config snapshot hash/kind mismatch: {h}")
        previous = by_hash.get(h)
        if previous is not None and previous != (kind, normalized):
            raise ValueError("DATASET_INTEGRITY_VIOLATION: conflicting source config snapshots")
        by_hash[h] = (kind, normalized)
    missing = []
    for h, expected_kind in sorted(required_refs):
        stored = by_hash.get(h)
        if stored is None:
            missing.append(f"{expected_kind}:{h}")
        elif stored[0] != expected_kind:
            raise ValueError(f"DATASET_INTEGRITY_VIOLATION: source config kind mismatch: {h} expected={expected_kind} actual={stored[0]}")
    if missing:
        raise ValueError("DATASET_INTEGRITY_VIOLATION: fee evidence references missing config snapshot(s): " + ",".join(missing))
    return by_hash


def _source_max_age_ns(source_type: str, freshness_policy_hash: str, source_configs: dict[str, tuple[str, str]]) -> int:
    stored = source_configs.get(freshness_policy_hash)
    if stored is None or stored[0] != "FRESHNESS_POLICY":
        raise ValueError(f"DATASET_INTEGRITY_VIOLATION: missing source freshness policy: {freshness_policy_hash}")
    try:
        policy = json.loads(stored[1])
        key = {
            "PROTOCOL_GLOBAL_ACCOUNT": "protocol_max_age_ms",
            "NETWORK_FEE": "network_fee_max_age_ms",
            "RENT_EXEMPTION": "rent_max_age_ms",
            "PRIORITY_POLICY": "network_fee_max_age_ms",
        }[source_type]
        value = policy[key]
    except (KeyError, TypeError, ValueError) as exc:
        raise ValueError("DATASET_INTEGRITY_VIOLATION: malformed source freshness policy") from exc
    if type(value) is not int or value <= 0:
        raise ValueError("DATASET_INTEGRITY_VIOLATION: invalid max_age in source freshness policy")
    return value * 1_000_000


@dataclass
class ReplayInput:
    source_run_id: str
    epoch_id: str
    start_mono_ns: int
    start_utc_ns: int
    observations: list[tuple[int, int, str, str, str, str]]  # (receive_mono_ns, arrival_sequence, feed_id, connection_id, receive_utc, payload)
    fee_snapshots: list[FeeSnapshot]
    health_transitions: list[tuple[int, str, bool]]  # recorded feed readiness changes, replayed through the same on_feed_health path as PAPER
    lateness: dict[str, int]  # attempt key (position suffix + side + number) -> callback lateness ns
    dataset_hash: str
    end_mono_ns: int  # recorded run end (runs.ended_mono_ns) or, if missing, the last observation

    fx_snapshots: list[FxSnapshot] = field(default_factory=list)
    # Fee evidence retains both the COST_MODEL and FRESHNESS_POLICY hashes under which it was observed.
    # Cross-database replay carries both source rows; strategy/execution may differ, source TTL may not.
    source_config_snapshots: list[tuple[str, str, str, str, str, int | None, str | None]] = field(default_factory=list)

def load_replay_input(conn: sqlite3.Connection, run_id: str, cfg: RuntimeConfig) -> ReplayInput:
    conn.row_factory = sqlite3.Row
    run = conn.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
    if run is None:
        raise ValueError(f"run {run_id} not found")
    obs = conn.execute("SELECT receive_mono_ns, arrival_sequence, feed_id, connection_id, receive_utc, raw_payload, raw_payload_hash FROM feed_observations WHERE run_id=? AND raw_payload IS NOT NULL ORDER BY receive_mono_ns, arrival_sequence, feed_id", (run_id,)).fetchall()
    observations = []
    for r in obs:  # dataset integrity: stored payloads are re-hashed; a manipulated payload never replays under the old hash
        actual = sha256_text(r["raw_payload"])
        if actual != r["raw_payload_hash"]:
            raise ValueError(f"DATASET_INTEGRITY_VIOLATION: feed_observations payload hash mismatch at receive_mono_ns={r['receive_mono_ns']} feed={r['feed_id']}")
        observations.append((r["receive_mono_ns"], r["arrival_sequence"], r["feed_id"], r["connection_id"], r["receive_utc"], r["raw_payload"]))
    fee_rows = list(conn.execute("SELECT * FROM fee_snapshots WHERE run_id=? ORDER BY received_mono_ns", (run_id,)))
    required_source_refs: set[tuple[str, str]] = set()
    for row in fee_rows:
        if not row["config_hash"] or not row["freshness_policy_hash"]:
            raise ValueError("DATASET_INTEGRITY_VIOLATION: fee evidence missing source config reference")
        required_source_refs.add((row["config_hash"], "COST_MODEL"))
        required_source_refs.add((row["freshness_policy_hash"], "FRESHNESS_POLICY"))

    source_config_snapshots = [
        tuple(row)
        for config_hash in sorted({h for h, _kind in required_source_refs})
        for row in conn.execute(
            "SELECT config_hash,kind,canonical_json,created_utc,source,valid_from_slot,verified_utc "
            "FROM config_snapshots WHERE config_hash=?",
            (config_hash,),
        )
    ]
    source_content = _validate_source_configs(required_source_refs, source_config_snapshots)

    fees = []
    for r in fee_rows:
        norm = json.loads(r["normalized_fee_components"])
        try:
            evidence = CostEvidence(r["evidence"] or "UNKNOWN")
        except (KeyError, IndexError, ValueError):
            evidence = CostEvidence.UNKNOWN
        fees.append(FeeSnapshot(r["fee_snapshot_id"], r["source_type"], r["source_provider"], r["source_method"], tuple(json.loads(r["source_account_addresses"])), r["source_slot"], r["observed_utc"], r["received_mono_ns"], r["commitment"],
                                r["protocol_program_id"], r["idl_version_or_hash"], json.loads(r["raw_values"]), norm, r["rounding_rules"], r["applicability"], r["freshness_policy_hash"], r["validation_status"], r["config_hash"],
                                _source_max_age_ns(r["source_type"], r["freshness_policy_hash"], source_content), evidence))
    transitions: list[tuple[int, str, bool]] = []
    for q in conn.execute("SELECT kind, feed_id, start_mono_ns, end_mono_ns FROM quality_events WHERE run_id=? AND kind IN ('PROVIDER_DISCONNECT','CANONICAL_GAP','PROVIDER_RECONNECT','CANONICAL_GAP_RECOVERED_STATE_ONLY') ORDER BY quality_event_id", (run_id,)):
        if q["feed_id"] is None:
            continue
        if q["kind"] in ("PROVIDER_DISCONNECT", "CANONICAL_GAP"):
            transitions.append((q["start_mono_ns"], q["feed_id"], False))
        else:
            transitions.append((q["end_mono_ns"] if q["end_mono_ns"] is not None else q["start_mono_ns"], q["feed_id"], True))
    transitions.sort(key=lambda t: t[0])
    gaps = transitions
    lateness: dict[str, int] = {}
    for a in conn.execute("SELECT attempt_id, position_id, side, attempt_number, scheduled_fill_mono_ns, callback_actual_mono_ns FROM execution_attempts WHERE run_id=? AND callback_actual_mono_ns IS NOT NULL", (run_id,)):
        key = f"{a['position_id'].split('_p')[-1]}:{a['side']}:{a['attempt_number']}"
        lateness[key] = max(0, a["callback_actual_mono_ns"] - a["scheduled_fill_mono_ns"])
    hashes = [(r[0], r[1], r[2], r[3]) for r in conn.execute("SELECT raw_payload_hash, receive_mono_ns, arrival_sequence, feed_id FROM feed_observations WHERE run_id=? ORDER BY receive_mono_ns, arrival_sequence, feed_id", (run_id,))]
    fee_ids = [(r[0], r[1], r[2], sha256_text(str(r[3]) + "|" + str(r[4]) + "|" + str(r[5]) + "|" + str(r[6])))
               for r in conn.execute("SELECT fee_snapshot_id, snapshot_hash, received_mono_ns, normalized_fee_components, validation_status, raw_values, evidence FROM fee_snapshots WHERE run_id=? ORDER BY received_mono_ns, fee_snapshot_id", (run_id,))]
    start = run["started_mono_ns"] if "started_mono_ns" in run.keys() and run["started_mono_ns"] is not None else (observations[0][0] if observations else 0)
    delta = datetime.fromisoformat(run["started_utc"].replace("Z", "+00:00")) - datetime(1970, 1, 1, tzinfo=timezone.utc)
    start_utc = (delta.days * 86400 + delta.seconds) * 1_000_000_000 + delta.microseconds * 1000
    last_obs = observations[-1][0] if observations else 0
    ended = run["ended_mono_ns"] if "ended_mono_ns" in run.keys() else None
    end = max(last_obs, ended) if ended is not None else last_obs
    # dataset freeze covers everything replay consumes: frames with timing/feeds, fee knowledge with timing, gaps, callback lateness, run window, decoder/coverage/config
    decoder_code_hash = sha256_text(canonical_json({name: (Path(__file__).resolve().parent / name).read_text(encoding="utf-8") for name in ("market.py", "token_accounts.py")}))  # same version string with changed decoder code = new dataset version
    canonical_rows = [dict(r) for r in conn.execute("SELECT * FROM market_events WHERE run_id=? ORDER BY canonical_event_id", (run_id,))]
    quality_rows = [dict(r) for r in conn.execute("SELECT * FROM quality_events WHERE run_id=? ORDER BY quality_event_id", (run_id,))]
    fx_snapshots = []
    if conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='fx_snapshots'").fetchone():
        for row in conn.execute("SELECT * FROM fx_snapshots WHERE run_id=? ORDER BY received_mono_ns,fx_snapshot_id", (run_id,)):
            snapshot = FxSnapshot.from_payload(row["raw_json"], row["source"], row["observed_utc"], row["received_mono_ns"])
            if (snapshot.raw_hash != row["raw_hash"] or snapshot.snapshot_id != row["evidence_id"] or snapshot.amount != row["amount"]
                    or snapshot.validation_status != row["validation_status"] or snapshot.timestamp_basis != row["timestamp_basis"]):
                raise ValueError("DATASET_INTEGRITY_VIOLATION: SOL/USD evidence mismatch")
            fx_snapshots.append(snapshot)
    run_manifest = dict(run)
    # Bind every consumed fee attribute, including selection identity and TTL derived
    # from the source run's recorded freshness policy.
    fee_manifest = [
        {key: (fee.evidence.value if key == "evidence" else getattr(fee, key)) for key in fee.__slots__}
        for fee in sorted(fees, key=lambda fee: (fee.received_mono_ns, fee.fee_snapshot_id))
    ]
    dataset_hash = sha256_text(canonical_json({"dataset_schema_version": DATASET_SCHEMA_VERSION,
        "source_evidence_configs": sorted((h, kind, content) for h, (kind, content) in source_content.items()),
        "fee_config_references": sorted((f.fee_snapshot_id, f.config_hash, f.freshness_policy_hash) for f in fees),
        "fee_manifest": fee_manifest,
        "run": run_id, "fx": [s.__dict__ for s in fx_snapshots], "run_manifest": run_manifest, "canonical_events": canonical_rows, "quality_events": quality_rows, "epoch": run["host_clock_epoch_id"], "config_hash": run["config_hash"], "frames": hashes, "fees": fee_ids, "health": transitions, "lateness": sorted(lateness.items()),
        "start_mono_ns": start, "end_mono_ns": end, "ended_mono_ns": ended, "decoder": cfg.protocol["decoder_version"], "decoder_code_hash": decoder_code_hash, "protocol_pinning_hash": cfg.protocol_hash,
        "coverage": "CANONICAL_COVERAGE_V2", "spec": "1.3.2"}))
    return ReplayInput(
        run_id, run["host_clock_epoch_id"], start, start_utc, observations,
        fees, transitions, lateness, dataset_hash, end, fx_snapshots,
        source_config_snapshots,
    )


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
               stress_fee_multiplier: Decimal = Decimal(1), stress_adverse_bps: int = 0, mint_filter: set[str] | None = None, stress_reject_rate: Decimal = Decimal(0), stress_seed: int = 0, replay_callback_lateness: bool = True, execution_delay_provider=None) -> ReplayResult:
    """Replay the frozen dataset through StateOwner; returns the owner (RAM truth) and persists rows when requested."""
    if persist or replay_input.source_config_snapshots:
        required_refs = _fee_source_config_refs(replay_input.fee_snapshots)
        source_content = _validate_source_configs(required_refs, replay_input.source_config_snapshots)
    else:
        source_content = {}
    if persist:
        for h, (kind, content) in source_content.items():
            existing = conn.execute("SELECT kind,canonical_json FROM config_snapshots WHERE config_hash=?", (h,)).fetchone()
            if existing is not None and (existing[0] != kind or existing[1] != content):
                raise ValueError(f"DATASET_INTEGRITY_VIOLATION: destination config snapshot conflict: {h}")
    clock = ReplayClock(f"replay_{new_run_id}", replay_input.start_mono_ns, replay_input.start_utc_ns)
    scheduler = ReplayScheduler()
    recorder = Recorder(cfg.storage["queue_capacity_records"], cfg.storage["priority_headroom_records"])
    writer_errors: list[str] = []
    owner_ref: list[StateOwner] = []

    def on_writer_error(message: str) -> None:
        if not writer_errors:
            writer_errors.append(message)
        if owner_ref:
            owner_ref[0].on_disk_error(message)

    writer = SqliteWriter(
        conn, recorder, cfg.storage["batch_max_records"], cfg.storage["flush_interval_ms"],
        on_writer_error, cfg.storage["writer_locked_retries"], cfg.storage["fk_merge_max_batches"],
    ) if persist else None
    if persist:
        for kind, (h, content) in cfg.snapshot_kinds().items():
            upsert_config_snapshot(conn, kind, h, content, "replay", utc_iso(0))
        conn.executemany(
            "INSERT OR IGNORE INTO config_snapshots("
            "config_hash,kind,canonical_json,created_utc,source,valid_from_slot,verified_utc"
            ") VALUES (?,?,?,?,?,?,?)",
            replay_input.source_config_snapshots,
        )
        conn.execute("INSERT OR IGNORE INTO runs(run_id,campaign_id,mode,started_utc,code_revision,spec_hash,config_hash,host_clock_epoch_id,status,quality_status,start_balance_lamports,parent_run_id,dataset_hash,started_mono_ns) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                     (new_run_id, campaign_id, mode, utc_iso(0), "replay", "", cfg.config_hash, clock.epoch_id, "RUNNING", "OK", cfg.portfolio["initial_paper_balance_lamports"], replay_input.source_run_id, replay_input.dataset_hash, replay_input.start_mono_ns))
        conn.execute("UPDATE runs SET strategy_id=?,strategy_config_hash=? WHERE run_id=?", (cfg.strategy_id, cfg.strategy_config_hash, new_run_id))
        for s in replay_input.fee_snapshots:
            recorder.try_append(("fee_snapshot", (FeeSnapshot(**{**{k: getattr(s, k) for k in s.__slots__}, "fee_snapshot_id": f"{new_run_id}_{s.fee_snapshot_id}"}), new_run_id)))
    fee_cache = FeeCache()
    fee_iter = iter(sorted(replay_input.fee_snapshots, key=lambda s: s.received_mono_ns))
    next_fee = next(fee_iter, None)
    prefix = new_run_id if persist else replay_input.source_run_id

    def apply_fees_until(t: int, notify: bool = True) -> list[FeeSnapshot]:
        nonlocal next_fee
        snapshots = []
        while next_fee is not None and next_fee.received_mono_ns <= t:
            renamed = FeeSnapshot(**{**{k: getattr(next_fee, k) for k in next_fee.__slots__}, "fee_snapshot_id": f"{new_run_id}_{next_fee.fee_snapshot_id}"}) if persist else next_fee
            snapshots.append(renamed)
            next_fee = next(fee_iter, None)
        for _, grouped in groupby(snapshots, key=lambda s: s.received_mono_ns):
            effective = select_cost_basis(fee_cache.basis(), list(grouped))
            for kind in FEE_KINDS.values():
                setattr(fee_cache, kind, getattr(effective, kind))
        if snapshots and owner_ref and notify:
            owner_ref[0].on_fee_snapshots(snapshots)
        return snapshots

    lateness = replay_input.lateness if replay_callback_lateness else {}
    adapter = PaperExecutionAdapter(clock, scheduler, "PAPER", False, cfg.latency_model["entry_delay_ms"] * 1_000_000, cfg.latency_model["exit_delay_ms"] * 1_000_000, lateness_provider=lambda aid: lateness.get(_lateness_key(aid), 0), delay_provider=execution_delay_provider)
    engine = QuoteEngine(ExecutionPolicy.from_config(cfg), stress_fee_multiplier, stress_adverse_bps, stress_reject_rate, stress_seed)
    owner = StateOwner(cfg, new_run_id, clock, scheduler, recorder, adapter, fee_cache.basis, "PAPER", variant_id=variant_id, canonical_prefix=replay_input.source_run_id if not persist_observations else None,
                       persist_observations=persist_observations, compute_features=compute_features, engine=engine, mint_allowlist=mint_filter)
    owner_ref.append(owner)
    fx_iter = iter(sorted(replay_input.fx_snapshots, key=lambda s: s.received_mono_ns))
    next_fx = next(fx_iter, None)

    def apply_fx_until(t):
        nonlocal next_fx
        while next_fx is not None and next_fx.received_mono_ns <= t:
            owner.on_fx_snapshot(next_fx)
            next_fx = next(fx_iter, None)

    apply_fx_until(replay_input.start_mono_ns)
    owner.on_fee_snapshots([snap for snap in (fee_cache.protocol, fee_cache.network, fee_cache.rent, fee_cache.priority_policy) if snap is not None])
    transitions = list(replay_input.health_transitions)
    ti = 0
    if not transitions:  # legacy recordings without health events: all configured feeds ready from the start
        for f in cfg.feeds:
            owner.on_feed_health(f.id, True, replay_input.start_mono_ns)
    apply_fees_until(replay_input.start_mono_ns)
    stop_at = end_mono_ns if end_mono_ns is not None else replay_input.end_mono_ns

    def apply_health_until(t: int, strict: bool = False) -> None:
        nonlocal ti
        while ti < len(transitions) and (transitions[ti][0] < t if strict else transitions[ti][0] <= t):
            mono, fid, ready = transitions[ti]
            clock.advance_to(max(clock.monotonic_ns(), mono))
            owner.on_feed_health(fid, ready, mono, "replayed")
            ti += 1

    def fire_until(t: int, strict: bool = False) -> None:
        while True:
            pending = [v for v in (scheduler.next_due_ns(), next_fee.received_mono_ns if next_fee else None, next_fx.received_mono_ns if next_fx else None) if v is not None]
            if not pending:
                return
            due = min(pending)
            if not (due < t if strict else due <= t):
                return
            apply_health_until(due)  # a health change at or before the timer is visible to the callback, exactly as in PAPER
            clock.advance_to(max(clock.monotonic_ns(), due))
            apply_fees_until(clock.monotonic_ns())
            apply_fx_until(clock.monotonic_ns())
            scheduler.fire_due(due, clock.monotonic_ns())

    for receive, observations in groupby(replay_input.observations, key=lambda row: row[0]):
        if receive > stop_at:
            break
        fire_until(receive, strict=True)  # same model time: market/venue state first, then scheduled fills
        apply_health_until(receive)
        clock.advance_to(max(clock.monotonic_ns(), receive))
        snapshots = apply_fees_until(receive, notify=False)
        apply_fx_until(receive)
        for _, seq, feed_id, conn_id, utc, payload in observations:
            owner.on_raw(RawEnvelope(feed_id, conn_id, receive, utc, seq, payload))
            if seq % 5000 == 0:
                owner.maybe_evict(receive)
        if snapshots:
            owner.on_fee_snapshots(snapshots)
        if writer is not None and recorder.depth() >= cfg.storage["batch_max_records"]:
            writer.flush_sync()  # replay driver, outside the StateOwner critical section
    # Model time never runs past the recorded window: timers beyond it are not fired (no invented fills, exposures stay censored).
    # drain_ns can only extend within the recording; a research embargo beyond it is a no-op (positions crossing the end are purged/censored).
    horizon = stop_at if drain_ns is None else min(stop_at + drain_ns, replay_input.end_mono_ns)
    fire_until(horizon)
    apply_health_until(horizon)
    clock.advance_to(max(clock.monotonic_ns(), horizon))
    owner.finalize(clock.monotonic_ns())
    if writer is not None:
        writer.flush_sync()
        if writer_errors:
            owner.on_disk_error(writer_errors[0])
        quality_status = "STORAGE_ERROR" if writer_errors or writer.disk_error else ("OK" if not owner.counters.internal_errors else "INTERNAL_ERROR")
        conn.execute("UPDATE runs SET status='COMPLETED', ended_utc=?, ended_mono_ns=?, end_balance_lamports=?, quality_status=? WHERE run_id=?", (utc_iso(0), clock.monotonic_ns(), owner.portfolio.cash_lamports + owner.portfolio.reserved_lamports, quality_status, new_run_id))
    return ReplayResult(new_run_id, owner, digest(owner))


def digest(owner: StateOwner) -> dict[str, Any]:
    """Comparable summary of decisions/positions/ledger for parity checks (T19/T40/T45)."""
    positions = []
    for p in sorted(owner.positions.values(), key=lambda p: p.position_id):
        positions.append({"mint": p.mint, "state": p.state.value, "exit_reason": p.exit_reason.value if p.exit_reason else None, "origin_trigger": p.origin_trigger.value if p.origin_trigger else None,
                          "net_pnl": p.net_pnl_lamports, "tokens": p.token_balance_base_units, "entry_cost": p.entry_economic_cost_lamports, "fees": p.total_fees_lamports, "exit_attempts": p.exit_attempts,
                          "unresolved_reason": p.unresolved_reason, "research_status": p.research_status, "open_mono_ns": p.open_mono_ns, "close_mono_ns": p.close_mono_ns})
    attempts = [{"id": aid.split("_p")[-1], "side": side, "n": n, "submit": submit, "scheduled": scheduled, "quote_state": qs.split("|", 1)[-1]} for aid, side, n, submit, scheduled, qs in owner.adapter.submitted]
    return {"positions": positions, "attempts": attempts, "cash": owner.portfolio.cash_lamports, "reserved": owner.portfolio.reserved_lamports, "locked_rent": owner.portfolio.locked_rent_lamports,
            "ledger_cash_sum": owner.portfolio.ledger_cash_sum, "counters": {k: v for k, v in owner.counters.__dict__.items() if k != "quality_events"}}
