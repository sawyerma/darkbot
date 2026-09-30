"""Phase A review fix (P0): a metadata-only BSC market profile must be rejected BEFORE any run/storage side effect.

Integration level: the real entry points run_capture / run_paper / run_doctor_cli / Application(...) with a config that
selects bsc-mainnet:flap or bsc-mainnet:fourmeme raise MarketProfileNotRunnable and leave no database file, no runs row,
no RUNNING artefact and no sqlite-writer thread behind.
"""

from __future__ import annotations

import inspect
import json
import sqlite3
import tempfile
import threading
import unittest
from pathlib import Path

from darkbot.config import load_json_strict
from darkbot.main import Application, run_capture, run_doctor_cli, run_paper
from darkbot.runtime.assembly import MarketProfileNotRunnable
from tests.helpers import ROOT, base_config


def _writer_threads() -> list[str]:
    return [t.name for t in threading.enumerate() if t.name == "sqlite-writer" and t.is_alive()]


class BscProfileFailFastTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.configs = {}
        raw = load_json_strict(ROOT / "config" / "solana_pump" / "phase1.json")
        for pid in ("bsc-mainnet:flap", "bsc-mainnet:fourmeme"):
            path = self.tmp / f"{pid.replace(':', '_')}.json"
            path.write_text(json.dumps({**raw, "market_profile_id": pid}))
            self.configs[pid] = path
        self.writers_before = _writer_threads()

    def _assert_no_side_effects(self, db: Path):
        self.assertFalse(db.exists(), f"database file was created: {db}")
        self.assertFalse(db.with_suffix(".sqlite-wal").exists())
        self.assertFalse(db.with_suffix(".quarantine.jsonl").exists())
        self.assertEqual(_writer_threads(), self.writers_before, "a sqlite-writer thread was started")

    def test_run_capture_rejects_bsc_profiles_before_storage(self):
        for pid, cfg_path in self.configs.items():
            db = self.tmp / f"capture_{pid.split(':')[1]}.sqlite"
            with self.subTest(profile=pid):
                with self.assertRaises(MarketProfileNotRunnable) as ctx:
                    run_capture(cfg_path, db, "bsc_test_capture", max_duration_minutes=1)
                self.assertIn(pid, str(ctx.exception))
                self._assert_no_side_effects(db)

    def test_run_paper_rejects_bsc_profiles_before_storage(self):
        for pid, cfg_path in self.configs.items():
            db = self.tmp / f"paper_{pid.split(':')[1]}.sqlite"
            with self.subTest(profile=pid):
                with self.assertRaises(MarketProfileNotRunnable):
                    run_paper(cfg_path, db, "bsc_test_paper", max_open_positions=1, max_duration_minutes=1)
                self._assert_no_side_effects(db)

    def test_run_doctor_rejects_bsc_profiles_before_touching_doctor_db(self):
        for pid, cfg_path in self.configs.items():
            db = self.tmp / f"doctor_{pid.split(':')[1]}.sqlite"
            with self.subTest(profile=pid):
                with self.assertRaises(MarketProfileNotRunnable):
                    run_doctor_cli(cfg_path, db)
                self._assert_no_side_effects(db)

    def test_application_construction_itself_fails_fast(self):
        from darkbot.config import load_runtime_config
        for pid, cfg_path in self.configs.items():
            db = self.tmp / f"app_{pid.split(':')[1]}.sqlite"
            with self.subTest(profile=pid):
                with self.assertRaises(NotImplementedError):
                    Application(load_runtime_config(cfg_path), db, "bsc_test_app", mode="CAPTURE", max_duration_ms=1000, economic=False, capture_only=True)
                self._assert_no_side_effects(db)

    def test_existing_database_gets_no_new_run_row(self):
        # a pre-existing campaign database must not receive a RUNNING row for a rejected profile
        db = self.tmp / "existing.sqlite"
        conn = sqlite3.connect(db)
        from darkbot.storage import apply_migrations
        apply_migrations(conn)
        conn.commit()
        before = conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0]
        conn.close()
        with self.assertRaises(MarketProfileNotRunnable):
            run_capture(self.configs["bsc-mainnet:flap"], db, "bsc_test_existing", max_duration_minutes=1)
        conn = sqlite3.connect(db)
        self.assertEqual(conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0], before)
        self.assertEqual(conn.execute("SELECT COUNT(*) FROM runs WHERE status='RUNNING'").fetchone()[0], 0)
        conn.close()
        self.assertEqual(_writer_threads(), self.writers_before)

    def test_solana_profile_still_constructs_application(self):
        app = Application(base_config(), self.tmp / "solana.sqlite", "c")  # positive control: legacy path unchanged, no side effect at construction
        self.assertEqual(app.preflight.market_profile.profile_id, "solana-mainnet:pumpfun")
        self.assertFalse((self.tmp / "solana.sqlite").exists())

    def test_assembly_precedes_storage_in_application_run(self):
        src = inspect.getsource(Application.run)
        self.assertLess(src.index("assemble_runtime("), src.index("self.open_storage()"))
        src_init = inspect.getsource(Application.__init__)
        self.assertLess(src_init.index("preflight_market_profile("), src_init.index("self.cfg, self.db_path"))


if __name__ == "__main__":
    unittest.main()
