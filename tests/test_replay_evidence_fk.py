"""Cross-database replay evidence and persistence failure regressions; no network."""
import json
import sqlite3
import unittest

from darkbot.config import canonical_json, config_hash
from darkbot.domain import NS_PER_MS
from darkbot.replay import load_replay_input, run_replay
from darkbot.storage import apply_migrations
from tests.helpers import Harness, base_config, create_payload, MINT_A, MINT_B, T0


class ReplayEvidenceTests(unittest.TestCase):
    def source(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 0, "exit_delay_ms": 0}))
        self.addCleanup(h.conn.close)
        h.feed("helius_standard", create_payload(), T0 + NS_PER_MS)
        h.feed("helius_standard", create_payload(MINT_B, sig="second", slot=101), T0 + 2 * NS_PER_MS)
        h.finish(T0 + 3 * NS_PER_MS)
        return h

    def target(self):
        conn = sqlite3.connect(":memory:", isolation_level=None)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys=ON")
        apply_migrations(conn)
        self.addCleanup(conn.close)
        return conn

    def test_different_replay_cost_config_keeps_source_evidence_fk(self):
        h = self.source()
        cfg = h.cfg.with_overrides(execution={"slippage_bps": h.cfg.execution["slippage_bps"] + 1})
        data = load_replay_input(h.conn, h.run_id, cfg)
        out = self.target()
        result = run_replay(out, data, cfg, "replayed", "camp", persist_observations=True)
        self.assertFalse(result.owner.disk_error)
        self.assertEqual(out.execute("PRAGMA foreign_key_check").fetchall(), [])
        self.assertEqual(out.execute("SELECT COUNT(*) FROM market_events WHERE event_type='CREATE'").fetchone()[0], 2)
        self.assertEqual({r[0] for r in out.execute("SELECT config_hash FROM fee_snapshots")}, {h.cfg.cost_model_hash})
        self.assertEqual(out.execute("SELECT kind FROM config_snapshots WHERE config_hash=?", (h.cfg.cost_model_hash,)).fetchone()[0], "COST_MODEL")

    def test_missing_corrupt_and_wrong_kind_source_fail_before_replay(self):
        for corruption in ("missing", "content", "kind"):
            with self.subTest(corruption=corruption):
                h = self.source()
                h.conn.execute("PRAGMA foreign_keys=OFF")
                if corruption == "missing":
                    h.conn.execute("DELETE FROM config_snapshots WHERE config_hash=?", (h.cfg.cost_model_hash,))
                elif corruption == "content":
                    h.conn.execute("UPDATE config_snapshots SET canonical_json='{}' WHERE config_hash=?", (h.cfg.cost_model_hash,))
                else:
                    h.conn.execute("UPDATE config_snapshots SET kind='POLICY' WHERE config_hash=?", (h.cfg.cost_model_hash,))
                with self.assertRaisesRegex(ValueError, "DATASET_INTEGRITY_VIOLATION"):
                    load_replay_input(h.conn, h.run_id, h.cfg)

    def test_destination_conflict_fails_without_creating_run(self):
        h = self.source()
        data = load_replay_input(h.conn, h.run_id, h.cfg)
        for kind, content in (("COST_MODEL", "{}"), ("POLICY", data.source_config_snapshots[0][2])):
            out = self.target()
            out.execute("INSERT INTO config_snapshots(config_hash,kind,canonical_json,created_utc,source) VALUES (?,?,?,?,?)", (h.cfg.cost_model_hash, kind, content, "now", "test"))
            with self.assertRaisesRegex(ValueError, "destination config snapshot conflict"):
                run_replay(out, data, h.cfg, "conflict", "camp")
            self.assertEqual(out.execute("SELECT COUNT(*) FROM runs").fetchone()[0], 0)

    def test_dataset_binds_fee_config_reference_and_source_content(self):
        h = self.source()
        before = load_replay_input(h.conn, h.run_id, h.cfg)
        content = h.cfg.cost_model_content()
        content["execution"] = dict(content["execution"], slippage_bps=h.cfg.execution["slippage_bps"] + 1)
        changed_hash = config_hash(content)
        h.conn.execute("INSERT INTO config_snapshots(config_hash,kind,canonical_json,created_utc,source) VALUES (?,?,?,?,?)", (changed_hash, "COST_MODEL", canonical_json(content), "now", "test"))
        h.conn.execute("UPDATE fee_snapshots SET config_hash=?", (changed_hash,))
        after = load_replay_input(h.conn, h.run_id, h.cfg)
        self.assertNotEqual(before.dataset_hash, after.dataset_hash)
        self.assertEqual(after.dataset_hash, load_replay_input(h.conn, h.run_id, h.cfg).dataset_hash)

    def test_writer_failure_stops_entries_and_marks_storage_error(self):
        h = self.source()
        cfg = h.cfg.with_overrides(storage={"batch_max_records": 1})
        data = load_replay_input(h.conn, h.run_id, cfg)
        out = self.target()
        out.execute("CREATE TRIGGER reject_fee BEFORE INSERT ON fee_snapshots BEGIN SELECT RAISE(FAIL, 'forced write failure'); END")
        result = run_replay(out, data, cfg, "broken", "camp", persist_observations=True)
        self.assertTrue(result.owner.disk_error)
        self.assertTrue(result.owner.stop_new_entries)
        self.assertIn("forced write failure", result.owner.last_error)
        self.assertEqual(result.owner.counters.entry_intents, 1)
        self.assertEqual(out.execute("SELECT quality_status FROM runs WHERE run_id='broken'").fetchone()[0], "STORAGE_ERROR")

    def test_dataset_binds_fee_selection_identity_and_derived_ttl(self):
        h = self.source()
        original_hash = load_replay_input(h.conn, h.run_id, h.cfg).dataset_hash
        replacements = {
            "source_provider": "different_provider",
            "source_account_addresses": '["different_account"]',
            "protocol_program_id": "different_program",
            "idl_version_or_hash": "different_idl",
            "freshness_policy_hash": "different_policy",
            "commitment": "finalized",
        }
        for column, replacement in replacements.items():
            with self.subTest(column=column):
                saved = h.conn.execute(f"SELECT {column},fee_snapshot_id FROM fee_snapshots").fetchall()
                h.conn.execute(f"UPDATE fee_snapshots SET {column}=?", (replacement,))
                try:
                    self.assertNotEqual(original_hash, load_replay_input(h.conn, h.run_id, h.cfg).dataset_hash)
                finally:
                    h.conn.executemany(f"UPDATE fee_snapshots SET {column}=? WHERE fee_snapshot_id=?", saved)
        refresh = dict(h.cfg.costs["snapshot_refresh_policy"])
        refresh["protocol_max_age_ms"] += 1
        changed_ttl = h.cfg.with_overrides(costs={"snapshot_refresh_policy": refresh})
        self.assertNotEqual(original_hash, load_replay_input(h.conn, h.run_id, changed_ttl).dataset_hash)
        self.assertEqual(original_hash, load_replay_input(h.conn, h.run_id, h.cfg).dataset_hash)
