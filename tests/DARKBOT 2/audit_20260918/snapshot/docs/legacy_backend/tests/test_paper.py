"""PAPER runtime regression tests for live-forward lifecycle accounting."""

from __future__ import annotations

import unittest
from decimal import Decimal
from types import SimpleNamespace

from darkbot.config import StrategyConfig
from darkbot.execution import (
    FeeRates,
    FeeTier,
    LatencyAssumptions,
    NetworkCostAssumptions,
    PaperFillModel,
    ProtocolFeeConfig,
)
from darkbot.paper import PaperPhase, PaperRuntime, PaperTrade


class PaperRuntimeTests(unittest.TestCase):

    def _runtime(self) -> PaperRuntime:
        strategy = StrategyConfig(
            candidate_name="paper-test",
            observation_ms=1000,
            filters=(),
            trade_amount_lamports=50_000_000,
            target_net_multiple=Decimal("2"),
            stop_net_multiple=Decimal("0.65"),
            max_hold_ms=30_000,
            base_fee_lamports=5_000,
            priority_fee_lamports=100_000,
            max_slippage_bps=300,
            entry_latency_ms=800,
        )
        protocol = ProtocolFeeConfig(
            (FeeTier(0, FeeRates(0, 95, 30)),),
            "fee-config",
        )
        fill = PaperFillModel(
            protocol,
            NetworkCostAssumptions(5_000, 100_000),
            LatencyAssumptions(0, 0, 800, 800, "ASSUMED"),
        )
        return PaperRuntime(
            strategy,
            fill,
            SimpleNamespace(),
            SimpleNamespace(),
            max_open_positions=2,
            account_rent_lamports=0,
        )

    def test_stop_clears_active_slots_without_inventing_pnl(self) -> None:
        runtime = self._runtime()

        pending_tracker = SimpleNamespace(
            launch_id="pending",
            mint="pending-mint",
        )
        open_tracker = SimpleNamespace(
            launch_id="open",
            mint="open-mint",
        )

        runtime._active["pending"] = PaperTrade(
            tracker=pending_tracker,
            decision_offset_ns=1,
            decision_state=None,
            history=[],
            phase=PaperPhase.ENTRY_PENDING,
        )
        runtime._active["open"] = PaperTrade(
            tracker=open_tracker,
            decision_offset_ns=2,
            decision_state=None,
            history=[],
            phase=PaperPhase.OPEN,
        )

        runtime.stop()
        summary = runtime.summary()

        self.assertEqual(1, summary["pending_entries_cancelled"])
        self.assertEqual(1, summary["interrupted_positions"])
        self.assertEqual(0, summary["active_slots"])
        self.assertEqual(0, summary["pending_entries"])
        self.assertEqual(0, summary["open_positions"])
        self.assertEqual(0, summary["net_pnl_lamports"])
        self.assertEqual(0, summary["positions_closed"])
        self.assertEqual(0, summary["unresolved_zero"])


if __name__ == "__main__":
    unittest.main()
