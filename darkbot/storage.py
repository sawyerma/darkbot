"""SQLite migrations, bounded Recorder queue, one writer thread, transactional rows/hashes and
consistent read snapshots (spec: storage.py). The writer never mutates trading state.

Recorder.try_append is the ONLY hot-path entry: a put_nowait into a bounded deque with an
overflow flag. Row building, JSON serialisation and payload hashing happen in the writer thread.
"""

from __future__ import annotations

import collections
import dataclasses
import hashlib
import json
import sqlite3
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Iterable
from enum import Enum

from darkbot.config import canonical_json
from darkbot.domain import FeeSnapshot, LedgerRecord, PositionRecord, QuoteSnapshot, RawEnvelope, utc_iso

MIGRATIONS_DIR = Path(__file__).resolve().parent.parent / "migrations"
SCHEMA_VERSION = 1
SPEC_VERSION = "1.3.2"


class StorageError(RuntimeError):
    pass


# --------------------------------------------------------------------------- connection / migration


def open_connection(db_path: Path, read_only: bool = False) -> sqlite3.Connection:
    if read_only:
        conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, check_same_thread=False)
    else:
        db_path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(str(db_path), check_same_thread=False, isolation_level=None)
        conn.execute("PRAGMA journal_mode=WAL")
    conn.execute("PRAGMA foreign_keys=ON")
    conn.row_factory = sqlite3.Row
    return conn


def apply_migrations(conn: sqlite3.Connection) -> list[str]:
    """Apply phase-1 migrations in order (additive, IF NOT EXISTS); legacy/ is history only."""
    applied: list[str] = []
    conn.execute("CREATE TABLE IF NOT EXISTS phase1_applied_migrations (name TEXT PRIMARY KEY, applied_utc TEXT NOT NULL)")
    done = {r[0] for r in conn.execute("SELECT name FROM phase1_applied_migrations")}
    for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
        if path.name in done:
            continue
        conn.executescript(path.read_text(encoding="utf-8"))
        conn.execute("INSERT INTO phase1_applied_migrations(name, applied_utc) VALUES (?,?)", (path.name, utc_iso(time.time_ns())))
        applied.append(path.name)
    row = conn.execute("SELECT version FROM phase1_schema_version WHERE version=?", (SCHEMA_VERSION,)).fetchone()
    if row is None:
        conn.execute("INSERT INTO phase1_schema_version(version, spec_version, applied_utc) VALUES (?,?,?)", (SCHEMA_VERSION, SPEC_VERSION, utc_iso(time.time_ns())))
    return applied


def integrity_check(conn: sqlite3.Connection) -> dict[str, Any]:
    fk = [dict(r) for r in conn.execute("PRAGMA foreign_key_check")]
    integrity = conn.execute("PRAGMA integrity_check").fetchone()[0]
    tables = [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
    return {"foreign_key_check": fk, "integrity_check": integrity, "tables": tables, "schema_version": SCHEMA_VERSION, "foreign_keys_pragma": conn.execute("PRAGMA foreign_keys").fetchone()[0]}


REQUIRED_TABLES = ("runs", "config_snapshots", "feed_observations", "market_events", "decisions", "positions", "execution_attempts", "ledger", "quality_events", "research_variants", "reports", "fee_snapshots", "quote_snapshots", "feature_snapshots")


def write_readback_test(conn: sqlite3.Connection) -> bool:
    conn.execute("CREATE TABLE IF NOT EXISTS doctor_probe(k TEXT PRIMARY KEY, v TEXT)")
    token = utc_iso(time.time_ns())
    conn.execute("INSERT OR REPLACE INTO doctor_probe(k,v) VALUES ('probe',?)", (token,))
    ok = conn.execute("SELECT v FROM doctor_probe WHERE k='probe'").fetchone()[0] == token
    conn.execute("DROP TABLE doctor_probe")
    return ok


def upsert_config_snapshot(conn: sqlite3.Connection, kind: str, config_hash: str, content: Any, source: str, created_utc: str, verified_utc: str | None = None) -> None:
    conn.execute("INSERT OR IGNORE INTO config_snapshots(config_hash, kind, canonical_json, created_utc, source, valid_from_slot, verified_utc) VALUES (?,?,?,?,?,NULL,?)",
                 (config_hash, kind, canonical_json(content), created_utc, source, verified_utc))


# --------------------------------------------------------------------------- records (immutable hand-offs)


@dataclass(frozen=True, slots=True)
class ObservationRecord:
    observation_id: str
    run_id: str
    raw: RawEnvelope
    decode_done_mono_ns: int | None
    slot: int | None
    signature: str | None
    completeness: str
    # per event: (k, instruction_index, mint, event_type, canonical_event_id, validation)
    events: tuple[tuple[int, int | None, str | None, str | None, str | None, str], ...]
    new_market_events: tuple[dict[str, Any], ...]


@dataclass(frozen=True, slots=True)
class RecordGroup:
    """Records that reference each other (decision/position/quote/attempt) travel as ONE unit so a batch boundary can never split them."""

    records: tuple[Any, ...]


@dataclass(frozen=True, slots=True)
class RowRecord:
    table: str
    values: dict[str, Any]
    upsert: bool = False


@dataclass(frozen=True, slots=True)
class UpdateRecord:
    table: str
    key: str
    key_value: Any
    values: dict[str, Any]


class Recorder:
    """try_append -> ACCEPTED|OVERFLOW; bounded deque; never blocks or awaits."""

    ACCEPTED, OVERFLOW = "ACCEPTED", "OVERFLOW"

    def __init__(self, capacity: int, priority_headroom: int = 256) -> None:
        self.capacity = capacity
        self.priority_headroom = priority_headroom
        self._queue: collections.deque[Any] = collections.deque()
        self._lock = threading.Lock()
        self.overflowed = False
        self.overflow_count = 0
        self.accepted = 0
        self._wakeup = threading.Event()
        self.disabled = False

    def try_append(self, record: Any, priority: bool = False) -> str:
        if self.disabled:
            return self.OVERFLOW
        limit = self.capacity + (self.priority_headroom if priority else 0)  # quality events documenting an overflow must never be lost to it
        if len(self._queue) >= limit:
            self.overflowed = True
            self.overflow_count += 1
            return self.OVERFLOW
        self._queue.append(record)
        self.accepted += 1
        if not self._wakeup.is_set():
            self._wakeup.set()
        return self.ACCEPTED

    def append_many(self, records: Iterable[Any]) -> str:
        status = self.ACCEPTED
        for r in records:
            if self.try_append(r) == self.OVERFLOW:
                status = self.OVERFLOW
        return status

    def depth(self) -> int:
        return len(self._queue)

    def drain(self, max_records: int) -> list[Any]:
        out: list[Any] = []
        while self._queue and len(out) < max_records:
            out.append(self._queue.popleft())
        if not self._queue:
            self._wakeup.clear()
        return out

    def wait(self, timeout_s: float) -> None:
        self._wakeup.wait(timeout_s)


class SqliteWriter:
    """Dedicated thread: drains the recorder, builds rows, hashes payloads and commits in batches."""

    def __init__(self, conn: sqlite3.Connection, recorder: Recorder, batch_max: int, flush_interval_ms: int, on_disk_error: Callable[[str], None], locked_retries: int = 100, fk_merge_max_batches: int = 20,
                 quarantine_path: Path | None = None, busy_timeout_ms: int = 100, retry_backoff_ms: int = 50) -> None:
        self.conn = conn
        self.recorder = recorder
        self.batch_max = batch_max
        self.locked_retries = locked_retries
        self.fk_merge_max_batches = fk_merge_max_batches
        self.quarantine_path = quarantine_path
        self.busy_timeout_ms = busy_timeout_ms
        self.retry_backoff_s = retry_backoff_ms / 1000
        self._abort = threading.Event()
        self.fk_hold_rounds = 0
        self.fk_holds = 0
        self.quarantined_records = 0
        self.flush_interval_s = flush_interval_ms / 1000
        self.on_disk_error = on_disk_error
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, name="sqlite-writer", daemon=True)
        self.committed_records = 0
        self.committed_batches = 0
        self.failures = 0
        self.last_error: str | None = None
        self.disk_error = False
        self.fk_split = False
        self._commit_ok = False
        self.batch_latency_ns: list[int] = []

    def start(self) -> None:
        self._thread.start()

    def stop(self, timeout_s: float = 30.0) -> bool:
        self._stop.set()
        self._thread.join(timeout_s)
        if self._thread.is_alive():
            self._abort.set()
            self.conn.interrupt()
            self.disk_error = True
            self.last_error = f"WRITER_SHUTDOWN_TIMEOUT after {timeout_s:.3f}s"
            self.on_disk_error(self.last_error)
            return False
        return True

    def wait_until_stopped(self) -> None:
        self._thread.join()

    def _run(self) -> None:
        self.conn.execute(f"PRAGMA busy_timeout={self.busy_timeout_ms}")
        held: list[Any] = []
        while not self._stop.is_set() or self.recorder.depth() or held:
            batch = held + self.recorder.drain(self.batch_max)
            held = []
            if batch:
                if self._abort.is_set():
                    self._quarantine(batch, "SHUTDOWN_DEADLINE_EXCEEDED")
                    continue
                if self._commit(batch):
                    self.fk_hold_rounds = 0
                elif self.fk_split:
                    self.fk_hold_rounds += 1
                    exhausted = self.fk_hold_rounds >= self.fk_merge_max_batches or (self._stop.is_set() and self.recorder.depth() == 0)
                    if exhausted:
                        self._quarantine(batch, "FK_TARGET_NEVER_ARRIVED")  # bounded: never an endless silent retry loop
                        self.fk_hold_rounds = 0
                    else:
                        held = batch  # deferred FK target may still be in the queue: retry with more records
                        if not self._stop.is_set() and self.recorder.depth() == 0:
                            self.recorder.wait(self.flush_interval_s)
                else:
                    self._quarantine(batch, "WRITE_FAILED")
            else:
                self.recorder.wait(self.flush_interval_s)

    def _quarantine(self, batch: list[Any], reason: str) -> None:
        """Records that can never be committed are written to a quarantine file and the run is marked degraded; nothing is dropped silently."""
        self.quarantined_records += len(batch)
        self.last_error = f"{reason}: {len(batch)} records quarantined ({self.last_error})"
        if self.quarantine_path is not None:
            try:
                self.quarantine_path.parent.mkdir(parents=True, exist_ok=True)
                with self.quarantine_path.open("a", encoding="utf-8") as fh:
                    for record in batch:
                        fh.write(json.dumps({"reason": reason, "type": type(record).__name__, "record": _jsonable(record)}, ensure_ascii=False) + "\n")
            except OSError as exc:
                self.last_error += f"; quarantine write failed: {exc}"
        self.disk_error = True
        self.on_disk_error(self.last_error)

    def flush_sync(self) -> None:
        """Drain everything now in the calling thread (tests/replay); unresolved FK batches are quarantined, never spun on."""
        batch = self.recorder.drain(10**9)
        if batch and not self._commit(batch):
            self._quarantine(batch, "FK_TARGET_NEVER_ARRIVED" if self.fk_split else "WRITE_FAILED")

    def _commit(self, batch: list[Any], retries: int = 0) -> bool:
        started = time.monotonic_ns()
        self.fk_split = False
        self._commit_ok = False
        try:
            self.conn.execute("BEGIN")
            for record in batch:
                for sql, params in rows_for(record):
                    self.conn.execute(sql, params)
            self.conn.execute("COMMIT")
            self.committed_records += len(batch)
            self.committed_batches += 1
            self.batch_latency_ns.append(time.monotonic_ns() - started)
            if len(self.batch_latency_ns) > 10000:
                del self.batch_latency_ns[:5000]
            self._commit_ok = True
            return True
        except sqlite3.Error as exc:
            try:
                self.conn.execute("ROLLBACK")
            except sqlite3.Error:
                pass
            self.failures += 1
            self.last_error = f"{type(exc).__name__}: {exc}"
            if isinstance(exc, (sqlite3.OperationalError, sqlite3.DatabaseError)) and "locked" in str(exc).lower() and retries < self.locked_retries and not self._stop.is_set() and not self._abort.is_set():
                self._stop.wait(self.retry_backoff_s)
                return self._commit(batch, retries + 1)
            if isinstance(exc, sqlite3.IntegrityError) and "FOREIGN KEY" in str(exc):
                self.fk_split = True
                self.fk_holds += 1
                self.failures -= 1  # a deferred-FK hold is not a failure unless it ends in quarantine
                return False
            self.disk_error = True
            self.on_disk_error(self.last_error)
            return False


# --------------------------------------------------------------------------- row building (writer thread only)


def _insert(table: str, values: dict[str, Any], upsert: bool = False) -> tuple[str, tuple]:
    cols = ",".join(values)
    marks = ",".join("?" for _ in values)
    params = tuple(_sql(v) for v in values.values())
    if upsert:
        pk = next(iter(values))
        sets = ",".join(f"{k}=excluded.{k}" for k in values if k != pk)
        return f"INSERT INTO {table}({cols}) VALUES ({marks}) ON CONFLICT({pk}) DO UPDATE SET {sets}", params
    return f"INSERT OR IGNORE INTO {table}({cols}) VALUES ({marks})", params


def _sql(v: Any) -> Any:
    if isinstance(v, bool):
        return 1 if v else 0
    if isinstance(v, (dict, list, tuple)):
        return canonical_json(v)
    if hasattr(v, "value") and not isinstance(v, (int, str)):
        return v.value
    return v


def _jsonable(value: Any) -> Any:
    """Lossless quarantine representation; diagnostics are bounded separately."""
    if isinstance(value, Enum):
        return value.value
    if dataclasses.is_dataclass(value):
        return {f.name: _jsonable(getattr(value, f.name)) for f in dataclasses.fields(value)}
    if hasattr(value, "__slots__"):
        return {name: _jsonable(getattr(value, name)) for name in value.__slots__ if hasattr(value, name)}
    if isinstance(value, dict):
        return {str(k): _jsonable(v) for k, v in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [_jsonable(v) for v in value]
    if isinstance(value, bytes):
        return {"__bytes_hex__": value.hex()}
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)


def rows_for(record: Any) -> list[tuple[str, tuple]]:
    if isinstance(record, RecordGroup):
        return [row for r in record.records for row in rows_for(r)]
    if isinstance(record, RowRecord):
        return [_insert(record.table, record.values, record.upsert)]
    if isinstance(record, UpdateRecord):
        sets = ",".join(f"{k}=?" for k in record.values)
        return [(f"UPDATE {record.table} SET {sets} WHERE {record.key}=?", tuple(_sql(v) for v in record.values.values()) + (record.key_value,))]
    if isinstance(record, ObservationRecord):
        raw = record.raw
        payload_hash = hashlib.sha256(raw.payload.encode("utf-8")).hexdigest()
        out: list[tuple[str, tuple]] = []
        for me in record.new_market_events:
            out.append(_insert("market_events", me))
        events = record.events or ((0, None, None, None, None, "UNMATCHED"),)
        for k, ix, mint, etype, cid, validation in events:
            out.append(_insert("feed_observations", {
                "observation_id": record.observation_id if k == 0 else f"{record.observation_id}-{k}", "run_id": record.run_id, "feed_id": raw.feed_id, "connection_id": raw.connection_id,
                "receive_mono_ns": raw.receive_mono_ns, "receive_utc": raw.receive_utc, "decode_done_mono_ns": record.decode_done_mono_ns, "slot": record.slot, "signature": record.signature,
                "instruction_index": ix, "mint": mint, "event_type": etype, "canonical_event_id": cid, "completeness": record.completeness, "validation_status": validation,
                "raw_payload": raw.payload if k == 0 else None, "raw_payload_hash": payload_hash, "arrival_sequence": raw.arrival_sequence}))
        return out
    if isinstance(record, LedgerRecord):
        return [_insert("ledger", {"ledger_entry_id": record.ledger_entry_id, "run_id": record.run_id, "position_id": record.position_id, "attempt_id": record.attempt_id, "mono_ns": record.mono_ns, "utc": record.utc,
                                   "kind": record.kind, "cash_delta_lamports": record.cash_delta_lamports, "locked_rent_delta_lamports": record.locked_rent_delta_lamports, "token_delta_base_units": record.token_delta_base_units,
                                   "economic_cost_lamports": record.economic_cost_lamports, "provenance": record.provenance})]
    if isinstance(record, tuple) and len(record) == 2 and isinstance(record[0], str):
        kind, obj = record
        if kind == "fee_snapshot":
            s: FeeSnapshot = obj[0]
            run_id = obj[1]
            snap_hash = hashlib.sha256(canonical_json({"raw": s.raw_values, "norm": s.normalized_fee_components, "src": s.source_method, "slot": s.source_slot, "evidence": s.evidence.value}).encode()).hexdigest()
            return [_insert("fee_snapshots", {"fee_snapshot_id": s.fee_snapshot_id, "run_id": run_id, "source_type": s.source_type, "source_provider": s.source_provider, "source_method": s.source_method,
                                              "source_account_addresses": list(s.source_account_addresses), "source_slot": s.source_slot, "observed_utc": s.observed_utc, "received_mono_ns": s.received_mono_ns,
                                              "commitment": s.commitment, "protocol_program_id": s.protocol_program_id, "idl_version_or_hash": s.idl_version_or_hash, "raw_values": s.raw_values,
                                              "normalized_fee_components": s.normalized_fee_components, "rounding_rules": s.rounding_rules, "applicability": s.applicability, "freshness_policy_hash": s.freshness_policy_hash,
                                              "validation_status": s.validation_status, "evidence": s.evidence.value, "config_hash": s.config_hash, "snapshot_hash": snap_hash})]
        if kind == "quote_snapshot":
            q: QuoteSnapshot = obj[0]
            run_id, position_id = obj[1], obj[2]
            return [_insert("quote_snapshots", {"quote_snapshot_id": q.quote_snapshot_id, "run_id": run_id, "position_id": position_id, "quote_state_event_id": q.quote_state_event_id, "fee_snapshot_id": q.fee_snapshot_id,
                                                "side": q.side.value, "created_mono_ns": q.created_mono_ns, "knowledge_cutoff_mono_ns": q.knowledge_cutoff_mono_ns, "curve_input_lamports": q.curve_input_lamports,
                                                "gross_curve_output_lamports": q.gross_curve_output_lamports, "token_amount_base_units": q.token_amount_base_units, "protocol_fee_total_lamports": q.protocol_fee_total_lamports,
                                                "swap_wallet_debit_lamports": q.swap_wallet_debit_lamports, "swap_wallet_credit_lamports": q.swap_wallet_credit_lamports, "network_fee_total_lamports": q.network_fee_total_lamports,
                                                "tip_lamports": q.tip_lamports, "rent_delta_lamports": q.rent_delta_lamports, "irrecoverable_cost_lamports": q.irrecoverable_cost_lamports, "slippage_limit_bps": q.slippage_limit_bps,
                                                "min_output_base_units": q.min_output_base_units, "max_input_base_units": q.max_input_base_units, "quote_age_ns": q.quote_age_ns, "quote_model_hash": q.quote_model_hash,
                                                "rounding_version": q.rounding_version, "quality_status": q.quality_status})]
        if kind == "position":
            p: PositionRecord = obj
            return [_insert("positions", position_row(p), upsert=True)]
    raise StorageError(f"unknown record type {type(record).__name__}")


def position_row(p: PositionRecord) -> dict[str, Any]:
    return {"position_id": p.position_id, "run_id": p.run_id, "variant_id": p.variant_id, "mint": p.mint, "creator": p.creator, "policy_hash": p.policy_hash, "state": p.state.value,
            "entry_decision_id": p.entry_decision_id, "entry_fill_attempt_id": p.entry_fill_attempt_id, "token_balance_base_units": p.token_balance_base_units, "target_net_multiple": p.target_net_multiple,
            "open_mono_ns": p.open_mono_ns, "close_mono_ns": p.close_mono_ns, "exit_reason": p.exit_reason.value if p.exit_reason else None, "net_pnl_lamports": p.net_pnl_lamports,
            "quality_status": p.quality_status, "research_status": p.research_status, "timeout_due_mono_ns": p.timeout_due_mono_ns, "research_end_mono_ns": p.research_end_mono_ns,
            "unresolved_reason": p.unresolved_reason, "venue_state": p.venue_state.value, "canonical_provider_gap_flag": p.canonical_provider_gap_flag,
            "entry_economic_cost_lamports": p.entry_economic_cost_lamports if p.entry_fill_attempt_id else None, "locked_rent_lamports": p.locked_rent_lamports, "total_fees_lamports": p.total_fees_lamports,
            "origin_trigger": p.origin_trigger.value if p.origin_trigger else None, "entry_fill_mono_ns": p.entry_fill_mono_ns}


# --------------------------------------------------------------------------- readers (reports / replay)


class Reader:
    def __init__(self, db_path: Path) -> None:
        self.conn = open_connection(db_path, read_only=True)

    def all(self, sql: str, params: tuple = ()) -> list[sqlite3.Row]:
        return self.conn.execute(sql, params).fetchall()

    def one(self, sql: str, params: tuple = ()) -> sqlite3.Row | None:
        return self.conn.execute(sql, params).fetchone()

    def close(self) -> None:
        self.conn.close()


def prior_campaign_mints(conn: sqlite3.Connection, campaign_id: str) -> set[str]:
    """ONE_SUBMITTED_ENTRY_PER_MINT_PER_CAMPAIGN: every mint with a submitted entry in any PAPER run of the campaign (survives restarts)."""
    return {r[0] for r in conn.execute("SELECT p.mint FROM positions p JOIN runs r ON r.run_id=p.run_id WHERE r.campaign_id=? AND r.mode='PAPER'", (campaign_id,))}


def mark_interrupted_runs(conn: sqlite3.Connection, campaign_id: str, utc: str) -> int:
    """T23: on restart, previously open runs are closed as INTERRUPTED; their positions stay non-closed."""
    cur = conn.execute("UPDATE runs SET status='INTERRUPTED', ended_utc=? WHERE campaign_id=? AND ended_utc IS NULL AND status='RUNNING'", (utc, campaign_id))
    return cur.rowcount
