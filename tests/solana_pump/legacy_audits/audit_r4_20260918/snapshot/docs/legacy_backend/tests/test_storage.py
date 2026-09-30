"""storage tests (real SQLite): atomic migrations, ids, writer protocol, and one end-to-end SHADOW run with a fake chain source."""

from __future__ import annotations

import asyncio
import json
import shutil
import sqlite3
import tempfile
import unittest
from pathlib import Path
from typing import Any

from darkbot import domain, main
from darkbot.config import load_limits
from darkbot.domain import InsertOp, Mode, SessionRow, SystemEventRow, UpdateOp
from darkbot.market import RawNotification
from darkbot.storage import IdAllocator, SchemaMismatch, StorageReader, StorageWriter, apply_migrations, open_connection
from tests.test_market import KEYS, SOL, create_line, trade_line

ACTIVE = domain.load_contract(Path(__file__))
CONTRACT = ACTIVE.document
MIGRATIONS = ACTIVE.project_root / CONTRACT["persistence"]["migrations_dir"]
LIMITS = load_limits(CONTRACT, chain_ws_url="wss://selftest.invalid")


def session_row(session_id: str, sequence: int) -> SessionRow:
    return SessionRow(session_id, sequence, Mode.SHADOW.value, None, CONTRACT["document"]["version"], "test", None, None, None, None, None, None, None, None, 1, 1, None, 1)


class MigrationAndIdTests(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.db = self.dir / "bot.db"

    def test_migrations_apply_once_and_detect_tampering(self) -> None:
        connection = open_connection(self.db, LIMITS)
        self.assertEqual(apply_migrations(connection, MIGRATIONS, CONTRACT["document"]["version"]), ["0001_initial.sql"])
        self.assertEqual(apply_migrations(connection, MIGRATIONS, CONTRACT["document"]["version"]), [])
        self.assertEqual(connection.execute("PRAGMA journal_mode").fetchone()[0], "wal")
        connection.execute("UPDATE schema_version SET migration_sha256 = 'tampered'")
        with self.assertRaises(SchemaMismatch):
            apply_migrations(connection, MIGRATIONS, CONTRACT["document"]["version"])
        connection.close()

    def test_id_allocator_continues_after_max(self) -> None:
        connection = open_connection(self.db, LIMITS)
        apply_migrations(connection, MIGRATIONS, CONTRACT["document"]["version"])
        connection.execute("INSERT INTO sessions (id, persistent_session_sequence, mode, contract_version, code_commit, started_at_utc_ns, started_at_monotonic_ns) VALUES ('s', 1, 'SHADOW', 'x', 'c', 1, 1)")
        connection.execute("INSERT INTO system_events (id, session_id, event_type, severity, utc_ns, details_json) VALUES (41, 's', 'X', 'INFO', 1, '{}')")
        ids = IdAllocator(connection)
        self.assertEqual((ids.next("system_events"), ids.next("system_events"), ids.next("events")), (42, 43, 1))
        connection.close()


class WriterTests(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir)
        self.db = self.dir / "bot.db"
        self.states: list[tuple[str, dict[str, Any]]] = []

    def on_state(self, state: Any, details: dict[str, Any]) -> None:
        self.states.append((state.value, details))

    def test_critical_ack_bulk_batches_and_poison_envelope(self) -> None:
        async def scenario() -> None:
            loop = asyncio.get_running_loop()
            connection = open_connection(self.db, LIMITS)
            apply_migrations(connection, MIGRATIONS, CONTRACT["document"]["version"])
            transactions: list[str] = []
            connection.set_trace_callback(lambda sql: transactions.append(sql) if sql.startswith("COMMIT") else None)
            writer = StorageWriter(connection, loop, LIMITS, self.on_state)
            writer.start()
            await asyncio.wait_for(writer.enqueue_critical([InsertOp(session_row("s1", 1))]), timeout=2)
            reader = StorageReader(self.db, LIMITS)
            self.assertEqual(reader.one("SELECT persistent_session_sequence FROM sessions"), (1,), "CRITICAL is visible to a second connection after the ack")
            ids = IdAllocator(connection)
            for index in range(30):
                writer.enqueue_bulk([InsertOp(SystemEventRow(ids.next("system_events"), "s1", "T", "INFO", index, "{}"))])
            await writer.flush()
            self.assertEqual(reader.one("SELECT count(*) FROM system_events"), (30,))
            self.assertLessEqual(len(transactions), 4, "30 envelopes are batched into few transactions (session + batches)")
            writer.enqueue_bulk([InsertOp(SystemEventRow(ids.next("system_events"), "no-such-session", "T", "INFO", 0, "{}"))])  # FK violation: poison envelope
            writer.enqueue_bulk([InsertOp(SystemEventRow(ids.next("system_events"), "s1", "T", "INFO", 99, "{}")), UpdateOp("sessions", {"id": "s1"}, {"code_commit": "updated"})])
            await writer.flush()
            self.assertEqual(reader.one("SELECT count(*) FROM system_events"), (31,), "only the poison envelope is dropped")
            self.assertEqual(reader.one("SELECT code_commit FROM sessions"), ("updated",))
            self.assertEqual(writer.rejected_envelopes, 1)
            self.assertTrue(any(details.get("code") == "STORAGE_ENVELOPE_REJECTED" for _, details in self.states), self.states)
            await writer.stop()
            reader.close()
            self.assertFalse(writer._thread.is_alive())

        asyncio.run(scenario())


class FakeChainSource:
    """Replaces ChainLogSource in the end-to-end test: feeds one launch with trades, then waits until cancelled."""

    instances: list["FakeChainSource"] = []

    def __init__(self, limits: Any, clock: Any, on_notification: Any, on_connection: Any) -> None:
        self.clock = clock
        self.on_notification = on_notification
        self.on_connection = on_connection
        self.closed = False
        FakeChainSource.instances.append(self)

    async def run(self) -> None:
        self.on_connection(True, self.clock.monotonic_ns())
        logs = [[create_line(KEYS["mint"], KEYS["dev"], 1_073_000_000, 30 * SOL, 793_100_000, 1_000_000_000, quote=b"\0" * 32), trade_line(KEYS["mint"], SOL, 34_000_000, True, KEYS["dev"], 31 * SOL, 1_039_000_000)]]
        for index in range(1, 6):
            logs.append([trade_line(KEYS["mint"], SOL // 10, 1_000_000, index % 3 != 0, KEYS["a" if index % 2 else "b"], 31 * SOL + index * SOL // 10, 1_039_000_000 - index * 1_000_000)])
        for sequence, lines in enumerate(logs, start=1):
            self.on_notification(RawNotification(sequence, self.clock.monotonic_ns(), self.clock.utc_ns(), 1000 + sequence, f"sig{sequence}", False, lines))
            await asyncio.sleep(0.01)
        await asyncio.Event().wait()

    async def close(self) -> None:
        self.closed = True


class EndToEndShadowTests(unittest.TestCase):
    def test_shadow_run_writes_session_launch_events_and_stops_cleanly(self) -> None:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        db = directory / "bot.db"
        original = main.ChainLogSource
        main.ChainLogSource = FakeChainSource  # type: ignore[misc]
        self.addCleanup(setattr, main, "ChainLogSource", original)
        controller = main.Controller(CONTRACT, CONTRACT["document"]["version"], LIMITS, db, MIGRATIONS, Mode.SHADOW)

        async def scenario() -> int:
            task = asyncio.create_task(controller.run())
            await asyncio.sleep(0.5)
            controller.request_stop("test")
            return await asyncio.wait_for(task, timeout=10)

        self.assertEqual(asyncio.run(scenario()), 0)
        connection = sqlite3.connect(db)
        try:
            session = connection.execute("SELECT mode, final_state, operational_limits_model_id, ended_at_utc_ns FROM sessions").fetchone()
            self.assertEqual((session[0], session[1]), ("SHADOW", "STOPPED"))
            self.assertIsNotNone(session[2])
            self.assertIsNotNone(session[3])
            self.assertEqual(connection.execute("SELECT count(*) FROM model_versions").fetchone()[0], 2)
            launch = connection.execute("SELECT status, data_complete, initial_buy_quote_lamports FROM launches").fetchone()
            self.assertEqual(launch, ("EVICTED", 0, SOL), "shutdown evicts the open launch and marks it incomplete")
            self.assertEqual(connection.execute("SELECT count(*) FROM events WHERE event_type IN ('BUY','SELL')").fetchone()[0], 5)
            self.assertEqual(connection.execute("SELECT count(*) FROM events WHERE event_type = 'CREATE'").fetchone()[0], 1)
            self.assertEqual(connection.execute("SELECT count(*) FROM provider_observations").fetchone()[0], 6)
            reasons = [row[0] for row in connection.execute("SELECT reason FROM data_quality_intervals")]
            self.assertEqual(reasons, ["PROCESS_RESTART"])
            self.assertIsNotNone(connection.execute("SELECT end_offset_ns FROM data_quality_intervals").fetchone()[0])
            states = [json.loads(row[0])["to"] for row in connection.execute("SELECT details_json FROM system_events WHERE event_type = 'STATE_CHANGE' ORDER BY id")]
            self.assertEqual(states, ["RUNNING", "STOPPED"])
            self.assertTrue(FakeChainSource.instances[-1].closed)
        finally:
            connection.close()


if __name__ == "__main__":
    unittest.main()
