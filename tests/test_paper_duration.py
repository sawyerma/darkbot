"""Regression tests for required PAPER total runtime."""
from __future__ import annotations

import unittest
from pathlib import Path
from unittest.mock import patch

from darkbot.cli import build_parser
from darkbot.config import ConfigError
from darkbot.main import run_paper

ROOT = Path(__file__).resolve().parent.parent


class PaperDurationTests(unittest.TestCase):
    def test_cli_requires_max_duration_minutes(self):
        parser = build_parser()
        with self.assertRaises(SystemExit):
            parser.parse_args([
                "paper",
                "--config", str(ROOT / "config" / "phase1.json"),
                "--db", "data/test.sqlite",
                "--campaign", "test",
            ])

    def test_run_paper_rejects_nonpositive_duration(self):
        for value in (None, 0, -1):
            with self.assertRaisesRegex(ConfigError, "--max-duration-minutes must be > 0"):
                run_paper(ROOT / "config" / "phase1.json", Path("data/test.sqlite"), "test",
                          max_duration_minutes=value)

    def test_run_paper_converts_minutes_to_milliseconds(self):
        captured = {}

        class FakeApplication:
            def __init__(self, cfg, db_path, campaign, mode="PAPER", max_duration_ms=None, **kwargs):
                captured["max_duration_ms"] = max_duration_ms

            async def run(self):
                return {"status": "COMPLETED"}

        with patch("darkbot.main.Application", FakeApplication):
            result = run_paper(ROOT / "config" / "phase1.json", Path("data/test.sqlite"), "test",
                               max_duration_minutes=7)
        self.assertEqual(result["status"], "COMPLETED")
        self.assertEqual(captured["max_duration_ms"], 7 * 60_000)


if __name__ == "__main__":
    unittest.main()
