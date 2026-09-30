"""Regression tests for strategy-free capture and offline edge-analysis plumbing."""
from __future__ import annotations

import json
import sqlite3
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

from darkbot.cli import build_parser
from darkbot.edge_analysis import PathPoint, _event_for_rule, _materialize_source, run_edge_analysis
from darkbot.main import run_capture

ROOT = Path(__file__).resolve().parents[2]


class CaptureCliTests(unittest.TestCase):
    def test_capture_requires_duration(self):
        parser = build_parser()
        with self.assertRaises(SystemExit):
            parser.parse_args([
                "capture",
                "--config", str(ROOT / "config" / "solana_pump" / "solana_pump" / "strategy_a_creator_exit_usd_slots.json"),
                "--db", "data/solana_pump/capture.sqlite",
                "--campaign", "capture_test",
            ])

    def test_capture_passes_strategy_free_mode_to_application(self):
        captured = {}

        class FakeApplication:
            def __init__(self, cfg, db_path, campaign, mode="PAPER", max_duration_ms=None,
                         economic=True, capture_only=False, **kwargs):
                captured.update({
                    "mode": mode,
                    "max_duration_ms": max_duration_ms,
                    "economic": economic,
                    "capture_only": capture_only,
                })

            async def run(self):
                return {"status": "COMPLETED"}

        with patch("darkbot.main.Application", FakeApplication):
            result = run_capture(
                ROOT / "config" / "solana_pump" / "solana_pump" / "strategy_a_creator_exit_usd_slots.json",
                Path("data/solana_pump/capture.sqlite"),
                "capture_test",
                max_duration_minutes=15,
            )
        self.assertEqual(result["status"], "COMPLETED")
        self.assertEqual(captured["mode"], "CAPTURE")
        self.assertEqual(captured["max_duration_ms"], 15 * 60_000)
        self.assertFalse(captured["economic"])
        self.assertTrue(captured["capture_only"])

    def test_edge_analysis_cli_accepts_db_or_zip_source(self):
        parser = build_parser()
        args = parser.parse_args([
            "edge-analysis",
            "--source", "old_system/exports/basetest_backend_v0.zip",
            "--out", "old_system/exports/backend_edge_analysis",
        ])
        self.assertEqual(args.command, "edge-analysis")
        self.assertEqual(args.source, Path("old_system/exports/basetest_backend_v0.zip"))


class LegacySourceTests(unittest.TestCase):
    def test_zip_selection_prefers_relevant_schema_over_larger_empty_db(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            empty = root / "large.db"
            conn = sqlite3.connect(empty)
            conn.execute("CREATE TABLE junk(x BLOB)")
            conn.execute("INSERT INTO junk VALUES (?)", (b"x" * 100000,))
            conn.commit()
            conn.close()

            relevant = root / "backend.db"
            conn = sqlite3.connect(relevant)
            conn.execute("CREATE TABLE trades(mint TEXT, pnl REAL)")
            conn.execute("INSERT INTO trades VALUES ('m', 1.0)")
            conn.commit()
            conn.close()

            archive = root / "source.zip"
            with zipfile.ZipFile(archive, "w") as zf:
                zf.write(empty, empty.name)
                zf.write(relevant, relevant.name)

            selected, temp, inventory = _materialize_source(archive)
            try:
                self.assertEqual(selected.name, "backend.db")
                self.assertEqual(len(inventory), 2)
            finally:
                if temp is not None:
                    temp.cleanup()

    def test_legacy_database_emits_schema_inventory_instead_of_crashing(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            db = root / "legacy.db"
            conn = sqlite3.connect(db)
            conn.execute("CREATE TABLE trades(mint TEXT, pnl REAL, multiple REAL)")
            conn.execute("INSERT INTO trades VALUES ('m', 1.5, 1.2)")
            conn.commit()
            conn.close()

            out = root / "out"
            result = run_edge_analysis(db, out)
            self.assertEqual(result["analysis_status"], "LEGACY_CAPTURE_EXPORTED_FOR_ADAPTER")
            report = json.loads((out / "legacy_schema_inventory.json").read_text())
            self.assertEqual(report["status"], "LEGACY_SCHEMA_REQUIRES_ADAPTER")
            self.assertIn("trades", report["tables"])
            self.assertTrue(report["likely_data_tables"])
            summary = json.loads((out / "edge_summary.json").read_text())
            self.assertEqual(summary["analysis_status"], "LEGACY_CAPTURE_EXPORTED_FOR_ADAPTER")


class EdgeRuleTests(unittest.TestCase):
    @staticmethod
    def point(t_ms, multiple, event_type="BUY", actor=None, creator="dev"):
        return PathPoint(
            mint="mint", event_type=event_type, t_ms=float(t_ms), receive_mono_ns=int(t_ms * 1_000_000),
            actor=actor, creator=creator, sol_amount=None, token_amount=None,
            virtual_sol_reserves=1, virtual_token_reserves=1, real_sol_reserves=1, real_token_reserves=1,
            creator_fee_bps=0, token_program=None, account_profile_id=None,
            venue_state="BONDING_CURVE_ACTIVE", quote_scope_status="SOL_WSOL",
            net_multiple=multiple, net_pnl_lamports=1 if multiple and multiple > 1 else -1,
        )

    def test_target_or_dev_uses_first_trigger(self):
        from decimal import Decimal

        points = [
            self.point(1000, Decimal("1.10")),
            self.point(2000, Decimal("1.25")),
            self.point(3000, Decimal("1.15"), event_type="SELL", actor="dev"),
        ]
        chosen = _event_for_rule(points, 0.0, target=Decimal("1.20"), dev=True)
        self.assertIsNotNone(chosen)
        self.assertEqual(chosen.t_ms, 2000.0)

    def test_min_hold_loser_rule_waits_until_hold(self):
        from decimal import Decimal

        points = [
            self.point(1000, Decimal("0.80")),
            self.point(5000, Decimal("0.95")),
            self.point(7000, Decimal("1.10")),
        ]
        chosen = _event_for_rule(points, 0.0, min_hold_ms=5000, hard_ms=10000,
                                 exit_if_not_profitable_after_hold=True)
        self.assertIsNotNone(chosen)
        self.assertEqual(chosen.t_ms, 5000.0)


if __name__ == "__main__":
    unittest.main()
