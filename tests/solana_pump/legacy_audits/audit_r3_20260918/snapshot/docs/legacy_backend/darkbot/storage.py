"""SQLite storage (LEAN_V1 section 4): one writer thread, one write connection, read-only readers.

Writer protocol (contract persistence.storage_writer_protocol, reduced to what runs now):
- enqueue_bulk(ops) enqueues one envelope; envelopes are written whole or not at all;
- enqueue_critical(ops) returns a Future resolved after COMMIT (call_soon_threadsafe);
- the thread waits on a Condition (no polling); the only timeout is the flush deadline
  while unflushed BULK rows exist;
- write failures never try to persist themselves: rows stay queued, on_state(CRITICAL)
  is reported to the loop, the writer retries with backoff, and the controller records
  the audit after recovery;
- above the hard limit the oldest whole envelopes are dropped and reported, never silently.
"""

from __future__ import annotations

import asyncio
import hashlib
import re
import sqlite3
import threading
import time
from collections import deque
from dataclasses import astuple
from pathlib import Path
from typing import Any, Callable, Iterator, Sequence

from darkbot.cli import apply_migration, canonical_json, sha256_json
from darkbot.config import Limits
from darkbot.domain import INTEGER_PK_TABLES, ROW_TABLE, InsertOp, ModelVersionRow, StorageState, UpdateOp, WriteOp, new_text_id, row_columns

MIGRATION_FILE = re.compile(r"^(\d{4})_.*\.sql$")


class SchemaMismatch(RuntimeError):
    """The database does not match the migrations on disk."""


# --------------------------------------------------------------------------- connections


def open_connection(db_path: Path, limits: Limits, read_only: bool = False) -> sqlite3.Connection:
    """Pragmas in contract order: journal_mode (write only), foreign_keys, busy_timeout, synchronous, wal_autocheckpoint."""
    if read_only:
        connection = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, check_same_thread=False)
        connection.execute(f"PRAGMA busy_timeout = {int(limits.busy_timeout_ms)}")
        return connection
    connection = sqlite3.connect(db_path, isolation_level=None, check_same_thread=False)
    connection.execute("PRAGMA journal_mode = WAL")
    connection.execute("PRAGMA foreign_keys = ON")
    connection.execute(f"PRAGMA busy_timeout = {int(limits.busy_timeout_ms)}")
    connection.execute(f"PRAGMA synchronous = {limits.synchronous}")
    connection.execute(f"PRAGMA wal_autocheckpoint = {int(limits.wal_autocheckpoint_pages)}")
    return connection


def apply_migrations(connection: sqlite3.Connection, migrations_dir: Path, contract_version: str) -> list[str]:
    """Apply NNNN_*.sql in order; each file + its schema_version row is one atomic transaction (cli.apply_migration)."""
    files = sorted(path for path in migrations_dir.iterdir() if MIGRATION_FILE.match(path.name))
    if not files:
        raise SchemaMismatch(f"no migrations in {migrations_dir}")
    has_table = connection.execute("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'").fetchone()
    applied: dict[int, tuple[str, str]] = {}
    if has_table:
        applied = {row[0]: (row[1], row[2]) for row in connection.execute("SELECT version, migration_name, migration_sha256 FROM schema_version")}
    names: list[str] = []
    for path in files:
        version = int(MIGRATION_FILE.match(path.name).group(1))  # type: ignore[union-attr]
        sql = path.read_text(encoding="utf-8")
        digest = hashlib.sha256(sql.encode("utf-8")).hexdigest()
        if version in applied:
            if applied[version][1] != digest:
                raise SchemaMismatch(f"migration {path.name} changed after it was applied")
            continue
        apply_migration(connection, sql, contract_version, path.name)
        names.append(path.name)
    count = connection.execute("SELECT count(*) FROM schema_version").fetchone()[0]
    if count != len(files):
        raise SchemaMismatch(f"schema_version has {count} rows for {len(files)} migration files")
    return names


# --------------------------------------------------------------------------- ids and reader


class IdAllocator:
    """INTEGER primary keys are assigned in the event-loop thread: MAX(id) at start, then sequential."""

    def __init__(self, connection: sqlite3.Connection) -> None:
        self._next: dict[str, int] = {}
        for table in INTEGER_PK_TABLES:
            current = connection.execute(f"SELECT COALESCE(MAX(id), 0) FROM {table}").fetchone()[0]
            self._next[table] = int(current) + 1

    def next(self, table: str) -> int:
        value = self._next[table]
        self._next[table] = value + 1
        return value


class StorageReader:
    """Read-only connection for startup, status and replay; never used in the hot path."""

    def __init__(self, db_path: Path, limits: Limits) -> None:
        self.connection = open_connection(db_path, limits, read_only=True)

    def one(self, sql: str, params: Sequence[Any] = ()) -> tuple | None:
        return self.connection.execute(sql, params).fetchone()

    def all(self, sql: str, params: Sequence[Any] = ()) -> list[tuple]:
        return self.connection.execute(sql, params).fetchall()

    def iter(self, sql: str, params: Sequence[Any] = (), arraysize: int = 1000) -> Iterator[tuple]:
        cursor = self.connection.execute(sql, params)
        cursor.arraysize = arraysize
        while True:
            rows = cursor.fetchmany()
            if not rows:
                return
            yield from rows

    def close(self) -> None:
        self.connection.close()


# --------------------------------------------------------------------------- statements


def _insert_sql(row: Any) -> tuple[str, tuple]:
    table = ROW_TABLE[type(row)]
    columns = row_columns(type(row))
    placeholders = ", ".join("?" for _ in columns)
    return f"INSERT INTO {table} ({', '.join(columns)}) VALUES ({placeholders})", astuple(row)


def _update_sql(op: UpdateOp) -> tuple[str, tuple]:
    assignments = ", ".join(f"{name} = ?" for name in op.values)
    where = " AND ".join(f"{name} = ?" for name in op.key)
    return f"UPDATE {op.table} SET {assignments} WHERE {where}", (*op.values.values(), *op.key.values())


def statement_for(op: WriteOp) -> tuple[str, tuple]:
    if isinstance(op, InsertOp):
        return _insert_sql(op.row)
    return _update_sql(op)


# --------------------------------------------------------------------------- writer


class StorageWriter:
    def __init__(self, connection: sqlite3.Connection, loop: asyncio.AbstractEventLoop, limits: Limits, on_state: Callable[[StorageState, dict[str, Any]], None]) -> None:
        self._connection = connection
        self._loop = loop
        self._limits = limits
        self._on_state = on_state
        self._cond = threading.Condition()
        self._critical: deque[tuple[Sequence[WriteOp], asyncio.Future]] = deque()
        self._bulk: deque[tuple[float, Sequence[WriteOp], asyncio.Future | None]] = deque()
        self._bulk_rows = 0
        self._stopping = False
        self._thread = threading.Thread(target=self._run, name="storage-writer", daemon=False)
        self._exit: asyncio.Future = loop.create_future()
        self.state = StorageState.OK
        self.dropped_envelopes = 0
        self.dropped_rows = 0
        self.committed_rows = 0
        self.committed_transactions = 0
        self.failures = 0
        self.rejected_envelopes = 0

    # ------------------------------------------------------------------ producer side (event-loop thread)
    def start(self) -> None:
        self._thread.start()

    def depth(self) -> tuple[int, int]:
        return self._bulk_rows, len(self._critical)

    def enqueue_bulk(self, ops: Sequence[WriteOp]) -> None:
        if not ops:
            return
        with self._cond:
            self._bulk.append((time.monotonic(), tuple(ops), None))
            self._bulk_rows += len(ops)
            depth = self._bulk_rows
            dropped = self._drop_oldest_if_over_hard_limit()
            self._cond.notify()
        if dropped:
            self._on_state(StorageState.CRITICAL, {"code": "STORAGE_LOSS", "dropped_envelopes": dropped[0], "dropped_rows": dropped[1]})
        elif depth >= self._limits.queue_bulk_critical_rows and self.state != StorageState.CRITICAL:
            self.state = StorageState.CRITICAL
            self._on_state(StorageState.CRITICAL, {"code": "QUEUE_CRITICAL", "depth": depth})
        elif depth >= self._limits.queue_bulk_warning_rows and self.state == StorageState.OK:
            self.state = StorageState.DEGRADED
            self._on_state(StorageState.DEGRADED, {"code": "QUEUE_WARNING", "depth": depth})

    def enqueue_critical(self, ops: Sequence[WriteOp]) -> asyncio.Future:
        future: asyncio.Future = self._loop.create_future()
        with self._cond:
            self._critical.append((tuple(ops), future))
            self._cond.notify()
        return future

    async def flush(self) -> None:
        """Barrier: resolves after every BULK envelope enqueued before it has been committed (CRITICAL jobs keep their priority)."""
        future: asyncio.Future = self._loop.create_future()
        with self._cond:
            self._bulk.append((time.monotonic(), (), future))
            self._cond.notify()
        await asyncio.wait_for(future, timeout=max(self._limits.critical_ack_timeout_ms, self._limits.bulk_flush_interval_ms) / 1000 * 4)

    async def stop(self) -> None:
        """Flush, let the thread exit, wait for its exit signal (no executor, no polling), close the connection."""
        await self.flush()
        with self._cond:
            self._stopping = True
            self._cond.notify()
        await asyncio.wait_for(self._exit, timeout=self._limits.critical_ack_timeout_ms / 1000)
        self._thread.join()
        self._connection.close()

    def _drop_oldest_if_over_hard_limit(self) -> tuple[int, int] | None:
        """Called under the lock: drop whole envelopes until below the critical depth; report counts."""
        if self._bulk_rows < self._limits.queue_bulk_hard_limit_rows:
            return None
        envelopes = rows = 0
        while self._bulk and self._bulk_rows >= self._limits.queue_bulk_critical_rows:
            _, ops, future = self._bulk.popleft()
            self._bulk_rows -= len(ops)
            if future is not None:
                self._loop.call_soon_threadsafe(_set_result, future)
            envelopes += 1
            rows += len(ops)
        self.dropped_envelopes += envelopes
        self.dropped_rows += rows
        return envelopes, rows

    # ------------------------------------------------------------------ consumer side (writer thread)
    def _run(self) -> None:
        backoff_ms = self._limits.reconnect_backoff_initial_ms
        while True:
            with self._cond:
                while not self._critical and not self._bulk and not self._stopping:
                    self._cond.wait()
                while not self._critical and self._bulk and not self._stopping:
                    age = time.monotonic() - self._bulk[0][0]
                    remaining = self._limits.bulk_flush_interval_ms / 1000 - age
                    if remaining <= 0 or self._bulk_rows >= self._limits.bulk_batch_max_rows or any(future is not None for _, _, future in self._bulk):
                        break
                    self._cond.wait(timeout=remaining)
                if self._critical:
                    ops, future = self._critical.popleft()
                    job: tuple[str, Sequence[WriteOp], asyncio.Future | None, list] = ("critical", ops, future, [])
                elif self._bulk:
                    taken: list[tuple[float, Sequence[WriteOp], asyncio.Future | None]] = []
                    rows = 0
                    while self._bulk and rows < self._limits.bulk_batch_max_rows:
                        item = self._bulk.popleft()
                        taken.append(item)
                        rows += len(item[1])
                    self._bulk_rows -= rows
                    job = ("bulk", tuple(op for _, ops, _ in taken for op in ops), None, taken)
                else:
                    self._loop.call_soon_threadsafe(_set_result, self._exit)
                    return
            ok = self._execute(job[1], job[2]) if job[0] == "critical" else self._execute_bulk(job[3])
            if ok:
                backoff_ms = self._limits.reconnect_backoff_initial_ms
                if self.state != StorageState.OK and job[0] == "bulk":
                    with self._cond:
                        depth = self._bulk_rows
                    if depth < self._limits.queue_bulk_warning_rows:
                        self.state = StorageState.OK
                        self._loop.call_soon_threadsafe(self._on_state, StorageState.OK, {"code": "STORAGE_RECOVERED"})
                continue
            if job[0] == "bulk":
                with self._cond:
                    for item in reversed(job[3]):
                        self._bulk.appendleft(item)
                        self._bulk_rows += len(item[1])
                    self._cond.wait(timeout=backoff_ms / 1000)
                backoff_ms = min(backoff_ms * 2, self._limits.reconnect_backoff_max_ms)

    def _execute_bulk(self, envelopes: list[tuple[float, Sequence[WriteOp], asyncio.Future | None]]) -> bool:
        """One transaction for the whole batch; on a logical rejection, fall back to envelope-by-envelope so only the invalid envelope is dropped."""
        ops = tuple(op for _, envelope, _ in envelopes for op in envelope)
        try:
            self._transaction(ops)
        except sqlite3.IntegrityError:
            for _, envelope, _ in envelopes:
                try:
                    self._transaction(envelope)
                except sqlite3.IntegrityError as exc:
                    self.failures += 1
                    self.rejected_envelopes += 1
                    self._loop.call_soon_threadsafe(self._on_state, self.state, {"code": "STORAGE_ENVELOPE_REJECTED", "error": str(exc), "rows": len(envelope)})
                except sqlite3.Error as exc:
                    return self._transient_failure(exc)
        except sqlite3.Error as exc:
            return self._transient_failure(exc)
        for _, _, future in envelopes:
            if future is not None:
                self._loop.call_soon_threadsafe(_set_result, future)
        return True

    def _transient_failure(self, exc: sqlite3.Error) -> bool:
        self.failures += 1
        self.state = StorageState.CRITICAL
        self._loop.call_soon_threadsafe(self._on_state, StorageState.CRITICAL, {"code": "STORAGE_WRITE_FAILED", "error": str(exc)})
        return False

    def _transaction(self, ops: Sequence[WriteOp]) -> None:
        try:
            self._connection.execute("BEGIN IMMEDIATE")
            for op in ops:
                sql, params = statement_for(op)
                self._connection.execute(sql, params)
            self._connection.execute("COMMIT")
        except sqlite3.Error:
            try:
                if self._connection.in_transaction:
                    self._connection.execute("ROLLBACK")
            except sqlite3.Error:
                pass
            raise
        self.committed_rows += len(ops)
        self.committed_transactions += 1

    def _execute(self, ops: Sequence[WriteOp], future: asyncio.Future | None) -> bool:
        """CRITICAL job: its own transaction; the result reaches the caller through the Future."""
        try:
            self._transaction(ops)
        except sqlite3.Error as exc:
            self.failures += 1
            if future is not None:
                self._loop.call_soon_threadsafe(_set_exception, future, exc)
            return True
        if future is not None:
            self._loop.call_soon_threadsafe(_set_result, future)
        return True


def _set_result(future: asyncio.Future) -> None:
    if not future.done():
        future.set_result(None)


def _set_exception(future: asyncio.Future, exc: BaseException) -> None:
    if not future.done():
        future.set_exception(exc)


# --------------------------------------------------------------------------- models (insert-if-new by hash)


def model_row(kind: str, content: dict[str, Any], source: str, now_utc_ns: int) -> ModelVersionRow:
    return ModelVersionRow(new_text_id(), kind, canonical_json(content), sha256_json(content), source, None, now_utc_ns)


def find_model_id(connection: sqlite3.Connection, kind: str, content_sha256: str) -> str | None:
    row = connection.execute("SELECT id FROM model_versions WHERE kind = ? AND content_sha256 = ?", (kind, content_sha256)).fetchone()
    return None if row is None else str(row[0])
