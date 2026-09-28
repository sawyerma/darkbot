"""Stage-3 backtest tests: recorded-state reconstruction, slot gating and real fill-model reuse."""

from __future__ import annotations

import tempfile
import unittest

from types import SimpleNamespace
from decimal import Decimal
from pathlib import Path

from darkbot.backtest import BacktestEngine, BacktestPolicy, Candidate, CurvePoint
from darkbot.config import StrategyConfig, load_protocol_fee_config
from darkbot.domain import EventType, QuoteScope
from darkbot.execution import FeeRates, FeeTier, LatencyAssumptions, NetworkCostAssumptions, ObservedCurveState, PaperFillModel, ProtocolFeeConfig
from darkbot.replay import ReplayEngine


class BacktestTests(unittest.TestCase):
    def _engine(self) -> BacktestEngine:
        engine = BacktestEngine.__new__(BacktestEngine)
        engine._strategy = StrategyConfig(
            candidate_name="test",
            observation_ms=1000,
            filters=(),
            trade_amount_lamports=50_000_000,
            target_net_multiple=Decimal("10"),
            stop_net_multiple=Decimal("0"),
            max_hold_ms=1000,
            base_fee_lamports=5_000,
            priority_fee_lamports=100_000,
            max_slippage_bps=300,
            entry_latency_ms=100,
        )
        engine._policy = BacktestPolicy(1, 0, 1, 0)
        protocol = ProtocolFeeConfig((FeeTier(0, FeeRates(0, 95, 30)),), "fee-config")
        engine._protocol_fees = protocol
        engine._fill = PaperFillModel(protocol, NetworkCostAssumptions(5_000, 100_000), LatencyAssumptions(0, 0, 100, 100, "ASSUMED"))
        return engine

    @staticmethod
    def _points() -> tuple[CurvePoint, ...]:
        def state(offset: int, sequence: int) -> ObservedCurveState:
            return ObservedCurveState(
                30_000_000_000,
                1_073_000_000_000_000,
                793_100_000_000_000,
                1_000_000_000_000_000,
                True,
                offset,
                f"sig-{sequence}",
                100 + sequence,
                sequence,
            )

        return (
            CurvePoint(state(0, 1), EventType.CREATE),
            CurvePoint(state(2_000_000_000, 2), EventType.BUY),
            CurvePoint(state(3_000_000_000, 3), EventType.SELL),
        )

    def test_timeout_trade_reuses_paper_fill_model_and_is_net_negative_after_costs(self) -> None:
        engine = self._engine()
        candidate = Candidate("launch", "mint", 1_000_000_000, {}, self._points())
        result = engine._simulate_candidate(candidate)
        self.assertTrue(result.closed_position)
        self.assertEqual("TIMEOUT", result.exit_reason)
        self.assertLess(result.net_pnl_lamports, 0)
        self.assertGreater(result.pump_fees_lamports, 0)
        self.assertEqual(210_000, result.network_fees_lamports)

    def test_slot_is_reserved_from_decision_until_first_trade_closes(self) -> None:
        engine = self._engine()
        points = self._points()
        first = Candidate("a", "mint-a", 1_000_000_000, {}, points)
        second = Candidate("b", "mint-b", 1_500_000_000, {}, points)
        results, skipped = engine._simulate_candidates([first, second])
        self.assertEqual(1, len(results))
        self.assertEqual(1, skipped)

    def test_replay_reconstructs_real_reserves_from_virtual_reserve_delta(self) -> None:
        context = {
            "launch": {
                "initial_virtual_token_reserves": 1_073_000,
                "initial_real_token_reserves": 793_100,
                "name": "n",
                "symbol": "s",
                "creator": "creator",
                "curve_address_provider": "curve",
                "token_total_supply": 1_000_000,
            }
        }
        row = {
            "id": 1,
            "launch_id": "launch",
            "event_type": "BUY",
            "mint": "mint",
            "transaction_signature": "sig",
            "provider_event_index": 0,
            "actor": "buyer",
            "quote_lamports": 100,
            "token_base_units": 1000,
            "virtual_sol_reserves": 30_000,
            "virtual_token_reserves": 1_070_000,
            "receive_offset_ns_from_session_start": 1,
            "utc_received_ns": 2,
            "chain_slot": 3,
            "provider_receive_sequence": 4,
        }
        event = ReplayEngine.normalized_event(row, context)
        self.assertEqual(790_100, event.real_token_reserves)

    def test_observe_only_launch_is_gated_before_strategy_evaluation(self) -> None:
        engine = self._engine()

        class MustNotEvaluate:
            def evaluate(self, features: dict[str, object]) -> None:
                raise AssertionError("StrategyEngine must not evaluate OBSERVE_ONLY launches")

        engine._strategy_engine = MustNotEvaluate()
        engine._reader = SimpleNamespace(
            load_session=lambda _session_id: SimpleNamespace(
                data_complete=True,
                end_offset_ns=10_000_000_000,
            ),
            load_launch_context=lambda _session_id: {
                "launch": {
                    "t0_receive_offset_ns": 0,
                    "accepted": 0,
                    "quote_scope": QuoteScope.OBSERVE_ONLY.value,
                }
            },
            quality_intervals=lambda _session_id: [],
        )
        engine._replay_decision_snapshots = lambda _session, _launches: {"launch": {}}
        engine._curve_points = lambda _session_id, _launches: {}
        engine._simulate_candidates = lambda _candidates: ([], 0)

        report = engine.run("session")

        self.assertEqual(1, report.unsupported_quote)
        self.assertEqual(0, report.rejected_filter)
        self.assertEqual(0, report.candidates_passed)

    def test_protocol_fee_snapshot_loader_rejects_strategy_style_floats_and_loads_tiers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fee.json"
            path.write_text(
                '{"account":"fee","read_slot":null,"read_utc_ns":null,"fee_tiers":['
                '{"market_cap_lamports_threshold":0,"lp_fee_bps":0,"protocol_fee_bps":95,"creator_fee_bps":30}]}'
            )
            config = load_protocol_fee_config(path)
            self.assertEqual(95, config.fee_tiers[0].fees.protocol_fee_bps)
            self.assertEqual(30, config.fee_tiers[0].fees.creator_fee_bps)


if __name__ == "__main__":
    unittest.main()
