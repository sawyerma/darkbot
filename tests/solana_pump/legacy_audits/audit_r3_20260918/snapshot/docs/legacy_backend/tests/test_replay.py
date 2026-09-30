"""Stage-2 replay tests: deterministic reconstruction and strict LIVE/REPLAY parity."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from darkbot.clock import LiveClock, ReplayClock, ReplayScheduler
from darkbot.config import load_limits
from darkbot.domain import EventType, NormalizedEvent, SnapshotOrigin, load_contract
from darkbot.market import DEFAULT_PUBKEY, MarketDataService


class ReplayClockTests(unittest.TestCase):
    def test_scheduler_fires_deadline_causally(self) -> None:
        clock = ReplayClock(1_000_000_000, 2_000_000_000)
        scheduler = ReplayScheduler()
        fired: list[int] = []
        scheduler.schedule_at(clock.started_at_monotonic_ns + 10, lambda at: fired.append(at))
        scheduler.fire_due(clock.started_at_monotonic_ns + 9)
        self.assertEqual([], fired)
        scheduler.fire_due(clock.started_at_monotonic_ns + 10)
        self.assertEqual([clock.started_at_monotonic_ns + 10], fired)


class SharedCreateInputTests(unittest.TestCase):
    def test_replay_create_preserves_quote_scope_and_snapshot_path(self) -> None:
        active = load_contract(Path(__file__))
        contract = active.document
        limits = load_limits(contract, chain_ws_url="wss://replay.invalid", mode="BACKTEST")
        clock = ReplayClock(10_000_000_000, 20_000_000_000)
        scheduler = ReplayScheduler()
        snapshots: list[dict] = []
        service = MarketDataService(
            contract,
            limits,
            clock,
            scheduler,
            None,
            None,
            "replay-session",
            1,
            "feature-set",
            origin=SnapshotOrigin.REPLAY,
            on_snapshot=lambda _tracker, _deadline, features: snapshots.append(features),
        )
        create = NormalizedEvent(
            EventType.CREATE,
            "mint",
            "sig-create",
            0,
            "creator",
            100,
            10,
            31_000_000_000,
            990_000_000_000,
            None,
            1_000,
            20_000_001_000,
            123,
            1,
            name="n",
            symbol="s",
            creator="creator",
            bonding_curve="curve",
            token_total_supply=1_000_000_000_000,
        )
        service.on_normalized_event(
            create,
            launch_id="launch",
            initial_reserves=(30_000_000_000, 1_000_000_000_000, 800_000_000_000),
            quote_mint=DEFAULT_PUBKEY,
        )
        self.assertEqual(1, service.counters.launches_tradable)
        first_deadline = 1_000 + limits.runtime_snapshot_ms[0] * 1_000_000
        clock.advance_to(first_deadline)
        scheduler.fire_due(clock.started_at_monotonic_ns + first_deadline)
        self.assertEqual(1, len(snapshots))
        self.assertIn("buy_count", snapshots[0])


if __name__ == "__main__":
    unittest.main()
