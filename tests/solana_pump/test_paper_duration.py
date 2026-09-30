"""Regression tests for required PAPER total runtime."""
from __future__ import annotations

import unittest
from pathlib import Path
from unittest.mock import patch

from darkbot.cli import build_parser
from darkbot.solana_pump.config import ConfigError, load_runtime_config
from darkbot.main import run_paper

ROOT = Path(__file__).resolve().parents[2]


class PaperDurationTests(unittest.TestCase):
    def test_cli_slot_override_preserves_fixed_and_usd_capital_per_slot(self):
        for profile in ("strategy_c_fast_creator_hold_2x.json", "strategy_a_creator_exit_usd_slots.json"):
            with self.subTest(profile=profile):
                path = ROOT / "config" / "solana_pump" / profile
                before = load_runtime_config(path)
                captured = {}
                class FakeApplication:
                    def __init__(self, cfg, *args, **kwargs):
                        captured["cfg"] = cfg
                    async def run(self):
                        return {"status": "COMPLETED"}
                args = build_parser().parse_args(["paper", "--config", str(path), "--db", "unused.sqlite", "--campaign", "test", "--max-open-positions", "10", "--max-duration-minutes", "1"])
                with patch("darkbot.main.Application", FakeApplication):
                    run_paper(args.config, args.db, args.campaign, args.max_open_positions, args.max_duration_minutes)
                after = captured["cfg"]
                self.assertEqual(after.portfolio["max_open_positions"], 10)
                balance = before.portfolio["initial_paper_balance_lamports"]
                slots = before.portfolio["max_open_positions"]
                expected = 0 if before.portfolio["initial_paper_balance_mode"] == "USD_PER_SLOT" else (balance * 10 + slots - 1) // slots
                self.assertEqual(after.portfolio["initial_paper_balance_lamports"], expected)

    def test_cli_requires_max_duration_minutes(self):
        parser = build_parser()
        with self.assertRaises(SystemExit):
            parser.parse_args([
                "paper",
                "--config", str(ROOT / "config" / "solana_pump" / "phase1.json"),
                "--db", "data/solana_pump/test.sqlite",
                "--campaign", "test",
            ])

    def test_run_paper_rejects_nonpositive_duration(self):
        for value in (None, 0, -1):
            with self.assertRaisesRegex(ConfigError, "--max-duration-minutes must be > 0"):
                run_paper(ROOT / "config" / "solana_pump" / "phase1.json", Path("data/solana_pump/test.sqlite"), "test",
                          max_duration_minutes=value)

    def test_run_paper_converts_minutes_to_milliseconds(self):
        captured = {}

        class FakeApplication:
            def __init__(self, cfg, db_path, campaign, mode="PAPER", max_duration_ms=None, **kwargs):
                captured["max_duration_ms"] = max_duration_ms

            async def run(self):
                return {"status": "COMPLETED"}

        with patch("darkbot.main.Application", FakeApplication):
            result = run_paper(ROOT / "config" / "solana_pump" / "phase1.json", Path("data/solana_pump/test.sqlite"), "test",
                               max_duration_minutes=7)
        self.assertEqual(result["status"], "COMPLETED")
        self.assertEqual(captured["max_duration_ms"], 7 * 60_000)


if __name__ == "__main__":
    unittest.main()
