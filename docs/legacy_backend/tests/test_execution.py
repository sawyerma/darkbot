"""Stage-3 PaperFillModel tests: exact integer curve math, fees, latency and slippage guards."""

from __future__ import annotations

import unittest
from decimal import Decimal

from darkbot.execution import (
    AttemptStatus,
    ExecutionModelError,
    ExitSignal,
    FeeRates,
    FeeTier,
    LatencyAssumptions,
    NetworkCostAssumptions,
    ObservedCurveState,
    PaperFillModel,
    ProtocolFeeConfig,
)


class PaperFillModelTests(unittest.TestCase):
    def setUp(self) -> None:
        self.protocol = ProtocolFeeConfig(
            fee_tiers=(FeeTier(0, FeeRates(0, 95, 30)),),
            account="8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt",
            read_slot=1,
            read_utc_ns=2,
        )
        self.network = NetworkCostAssumptions(base_fee_lamports=5_000, priority_fee_lamports=100_000)
        self.latency = LatencyAssumptions(100, 200, 500, 700, "ASSUMED")
        self.model = PaperFillModel(self.protocol, self.network, self.latency)
        self.state = ObservedCurveState(
            virtual_sol_reserves=30_000_000_000,
            virtual_token_reserves=1_073_000_000_000_000,
            real_token_reserves=793_100_000_000_000,
            fee_market_cap_supply=1_000_000_000_000_000,
            creator_fee_enabled=True,
            receive_offset_ns=1_000_000_000,
            signature="sig",
            slot=123,
            receive_sequence=7,
        )

    def test_fee_tier_uses_highest_matching_threshold(self) -> None:
        config = ProtocolFeeConfig(
            fee_tiers=(
                FeeTier(0, FeeRates(0, 100, 10)),
                FeeTier(10_000, FeeRates(0, 80, 20)),
                FeeTier(20_000, FeeRates(0, 60, 30)),
            ),
            account="fee-config",
        )
        self.assertEqual(100, config.rates_for_market_cap(9_999).protocol_fee_bps)
        self.assertEqual(80, config.rates_for_market_cap(10_000).protocol_fee_bps)
        self.assertEqual(60, config.rates_for_market_cap(999_999).protocol_fee_bps)

    def test_buy_quote_matches_current_exact_quote_in_integer_formula(self) -> None:
        quote = self.model.quote_buy(self.state, 50_000_000)
        self.assertEqual(49_382_715, quote.net_sol_lamports)
        self.assertEqual(469_136, quote.fees.protocol_lamports)
        self.assertEqual(148_149, quote.fees.creator_lamports)
        self.assertEqual(1_763_352_433_103, quote.tokens_out)
        self.assertEqual(50_000_000, quote.spent_lamports)
        self.assertEqual(0, quote.unspent_lamports)
        self.assertFalse(quote.capped_by_real_reserves)

    def test_creator_fee_is_zero_when_curve_has_no_creator(self) -> None:
        no_creator = ObservedCurveState(
            self.state.virtual_sol_reserves,
            self.state.virtual_token_reserves,
            self.state.real_token_reserves,
            self.state.fee_market_cap_supply,
            False,
            self.state.receive_offset_ns,
            self.state.signature,
            self.state.slot,
            self.state.receive_sequence,
        )
        quote = self.model.quote_buy(no_creator, 50_000_000)
        self.assertEqual(0, quote.fees.creator_lamports)
        self.assertGreater(quote.tokens_out, self.model.quote_buy(self.state, 50_000_000).tokens_out)

    def test_sell_quote_uses_curve_gross_then_ceiling_fees(self) -> None:
        quote = self.model.quote_sell(self.state, 1_763_352_433_103)
        self.assertEqual(49_220_670, quote.gross_sol_lamports)
        self.assertEqual(467_597, quote.fees.protocol_lamports)
        self.assertEqual(147_663, quote.fees.creator_lamports)
        self.assertEqual(48_605_410, quote.net_sol_lamports)

    def test_entry_fill_uses_latency_proxy_and_charges_network_and_rent(self) -> None:
        decision_offset = 1_000_000_000
        fill_state = ObservedCurveState(
            self.state.virtual_sol_reserves,
            self.state.virtual_token_reserves,
            self.state.real_token_reserves,
            self.state.fee_market_cap_supply,
            True,
            decision_offset + 700_000_000,
            "fill-sig",
            124,
            8,
        )
        attempt = self.model.simulate_entry(self.state, fill_state, decision_offset, 50_000_000, 300, account_rent_lamports=2_000_000)
        self.assertEqual(AttemptStatus.FILLED, attempt.status)
        self.assertEqual(decision_offset + 800_000_000, attempt.fill_offset_ns)
        self.assertEqual(105_000, attempt.network_fee_lamports)
        self.assertEqual(52_105_000, attempt.entry_cost_lamports)
        self.assertEqual("fill-sig", attempt.proxy.signature)

    def test_entry_rejects_when_fill_tokens_breach_min_tokens_out(self) -> None:
        worse = ObservedCurveState(
            virtual_sol_reserves=35_000_000_000,
            virtual_token_reserves=920_000_000_000_000,
            real_token_reserves=640_000_000_000_000,
            fee_market_cap_supply=self.state.fee_market_cap_supply,
            creator_fee_enabled=True,
            receive_offset_ns=1_700_000_000,
            signature="worse",
            slot=125,
            receive_sequence=9,
        )
        attempt = self.model.simulate_entry(self.state, worse, 1_000_000_000, 50_000_000, 300)
        self.assertEqual(AttemptStatus.SLIPPAGE_REJECTED, attempt.status)
        self.assertEqual(105_000, attempt.entry_cost_lamports)
        self.assertEqual(0, attempt.account_rent_lamports)

    def test_exit_slippage_rejection_charges_network_only(self) -> None:
        tokens = 1_763_352_433_103
        worse = ObservedCurveState(
            virtual_sol_reserves=25_000_000_000,
            virtual_token_reserves=1_200_000_000_000_000,
            real_token_reserves=793_100_000_000_000,
            fee_market_cap_supply=self.state.fee_market_cap_supply,
            creator_fee_enabled=True,
            receive_offset_ns=1_900_000_000,
            signature="worse-sell",
            slot=126,
            receive_sequence=10,
        )
        attempt = self.model.simulate_exit(self.state, worse, 1_000_000_000, tokens, 300)
        self.assertEqual(AttemptStatus.SLIPPAGE_REJECTED, attempt.status)
        self.assertEqual(-105_000, attempt.net_return_lamports)
        self.assertEqual(1_000_000_000 + 1_000_000_000, attempt.fill_offset_ns)

    def test_target_stop_use_net_realizable_value(self) -> None:
        token_amount = 1_500_000_000_000
        entry_cost = 50_000_000
        self.assertEqual(ExitSignal.STOP, self.model.exit_signal(self.state, token_amount, entry_cost, Decimal("2"), Decimal("0.99")))
        rich = ObservedCurveState(
            virtual_sol_reserves=100_000_000_000,
            virtual_token_reserves=600_000_000_000_000,
            real_token_reserves=300_000_000_000_000,
            fee_market_cap_supply=self.state.fee_market_cap_supply,
            creator_fee_enabled=True,
            receive_offset_ns=2_000_000_000,
            signature="rich",
            slot=127,
            receive_sequence=11,
        )
        self.assertEqual(ExitSignal.TARGET, self.model.exit_signal(rich, token_amount, entry_cost, Decimal("2"), Decimal("0.65")))

    def test_invalid_proxy_time_is_rejected(self) -> None:
        future = ObservedCurveState(
            self.state.virtual_sol_reserves,
            self.state.virtual_token_reserves,
            self.state.real_token_reserves,
            self.state.fee_market_cap_supply,
            True,
            2_000_000_000,
            "future",
            128,
            12,
        )
        with self.assertRaises(ExecutionModelError):
            self.model.simulate_entry(self.state, future, 1_000_000_000, 50_000_000, 300)


if __name__ == "__main__":
    unittest.main()
