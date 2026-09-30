"""C01-C21: Strategy C (LEGACY_BACKEND_FILTER_V1) = the old backend's policy on the shared engine."""
import json
import sys
import unittest
from decimal import Decimal
from pathlib import Path
from types import SimpleNamespace

from darkbot.ab_replay import _comparison_configs
from darkbot.config import ConfigError, load_runtime_config
from darkbot.domain import NS_PER_MS, AttemptStatus, EventType, ExitReason, PositionState
from darkbot.replay import digest, load_replay_input, run_replay
from darkbot.strategies.strategy_c_legacy_backend import (
    DATA_INCOMPLETE, PASS, REJECT_FILTER, LegacyTracker, evaluate_filters, parse_filters,
)
from darkbot.strategy import EXIT_PRIORITY, FEATURE_NAMES, pick_exit_trigger, stop_reached
from tests.helpers import *
from tests.helpers import _pubkey

START = T0 + NS_PER_MS
PROFILES = ("fast_creator_hold_2x", "pre_pump_01", "launch_sniper_0ms_15x", "paper_explore_01", "example", "smoke_all")


def cconfig(profile="paper_explore_01", **overrides):
    cfg = load_runtime_config(ROOT / f"config/solana_pump/strategy_c_{profile}.json")
    overrides.setdefault("latency_model", {"entry_delay_ms": 0, "exit_delay_ms": 0})
    return cfg.with_overrides(**overrides)


def make_c(profile="paper_explore_01", **overrides):
    return Harness(cconfig(profile, **overrides))


def end_of(h):
    return START + h.cfg.strategy["observation_ms"] * NS_PER_MS


def buys(h, actors, mint=MINT_A, start=START, prefix="b", sol=20_000_000, first_ms=100, last_ms=None):
    """Evenly spaced BUYs; the last one lands close to the deadline so the decision quote is fresh."""
    last_ms = h.cfg.strategy["observation_ms"] - 100 if last_ms is None else last_ms
    for n, actor in enumerate(actors):
        at = first_ms + int((last_ms - first_ms) * n / max(1, len(actors) - 1))
        h.feed("helius_standard", pump_frame(h, mint, sol, prefix + str(n), 101 + n, actor), start + at * NS_PER_MS)


def dev_buy(fraction="0.05"):
    tok = int(Decimal(fraction) * SUPPLY)
    sol = 1_000_000_000
    return {"initial_buy": (sol, tok)}


def qualified(h, mint=MINT_A, start=START, prefix="b", sig="sigC"):
    """PAPER_EXPLORE_01: >=2 BUY, >=2 unique, net flow >= 0.03 SOL, dev_initial_buy <= 0.20."""
    h.feed("helius_standard", create_payload(mint, sig=sig, **dev_buy()), start)
    buys(h, [BUYER, BUYER2], mint, start, prefix)
    h.advance(start + h.cfg.strategy["observation_ms"] * NS_PER_MS)


def observation(h, mint=MINT_A):
    row = h.rows("select * from strategy_observations where mint=?", (mint,))[0]
    return row, (json.loads(row["features_json"]) if row["features_json"] else None)


class LegacyFeatureParityTests(unittest.TestCase):
    """The ported tracker must produce exactly the old backend's feature snapshot for the same events."""

    def test_C00_feature_parity_with_old_backend_code(self):
        legacy_root = str(ROOT / "old_system" / "backend")
        saved = {k: v for k, v in sys.modules.items() if k == "darkbot" or k.startswith("darkbot.")}
        for k in saved:
            del sys.modules[k]
        sys.path.insert(0, legacy_root)
        try:
            from darkbot.domain import EventType as OldType, NormalizedEvent
            from darkbot.market import TokenTracker
            from darkbot.strategy import FeatureCalculator
            contract = {"phase0_fixture_contract": {"first_buyer_window": 5, "ratio_decimal_places": 12},
                        "feature_contract_v1": {"definitions": list(FEATURE_NAMES), "boolean_feature": "provider_gap_flag"}}
            calc = FeatureCalculator(contract, None)
            old = TokenTracker("L", "M", 1_000, (), 10**18, calc.quantum, 30_500_000_000, 1_055_000_000_000_000, 30_000_000_000, 1_073_000_000_000_000,
                               793_100_000_000_000, 10**15, 18_000_000_000_000, 500_000_000, tradable=True, creator="C", first_buyer_window=5)
            script = [("BUY", "w1", 40_000_000, 30_540_000_000, 1_053_600_000_000_000, 2_000), ("BUY", "w2", 90_000_000, 30_630_000_000, 1_050_500_000_000_000, 3_000),
                      ("SELL", "w1", 20_000_000, 30_610_000_000, 1_051_200_000_000_000, 4_000), ("BUY", "w1", 10_000_000, 30_620_000_000, 1_050_860_000_000_000, 5_000),
                      ("BUY", "w3", 70_000_000, 30_690_000_000, 1_048_470_000_000_000, 6_000)]
            for n, (kind, actor, lamports, vsol, vtok, at) in enumerate(script):
                old.apply(NormalizedEvent(OldType(kind), "M", f"s{n}", 0, actor, lamports, 1, vsol, vtok, 700_000_000_000_000, at, 0, 1, n))
            expected = calc.snapshot(old)
        finally:
            sys.path.remove(legacy_root)
            for k in [k for k in sys.modules if k == "darkbot" or k.startswith("darkbot.")]:
                del sys.modules[k]
            sys.modules.update(saved)
        new = LegacyTracker("M", "create", 1_000, 10**18, Decimal(1).scaleb(-12), 5, 30_500_000_000, 1_055_000_000_000_000, 1_073_000_000_000_000,
                            793_100_000_000_000, 10**15, 18_000_000_000_000)
        for kind, actor, lamports, vsol, vtok, at in script:
            new.apply(SimpleNamespace(event_type=EventType(kind), actor=actor, sol_amount=lamports, virtual_sol_reserves=vsol, virtual_token_reserves=vtok), at)
        self.assertEqual(new.snapshot(provider_gap_flag=False), expected)
        self.assertEqual(expected["buy_count"], 4)
        self.assertIsNotNone(expected["dev_initial_buy"])

    def test_filter_engine_null_bounds_and_first_failed(self):
        rules = parse_filters([{"feature": "buy_count", "min": 3, "max": None}, {"feature": "sell_count", "min": 0, "max": 0}, {"feature": "dev_initial_buy", "min": None, "max": "0.20"}])
        base = {"buy_count": 3, "sell_count": 0, "dev_initial_buy": "0.20", "provider_gap_flag": False}
        self.assertEqual(evaluate_filters(rules, base)[:2], (PASS, None))  # C05: bounds inclusive
        self.assertEqual(evaluate_filters(rules, {**base, "dev_initial_buy": None})[:2], (REJECT_FILTER, "dev_initial_buy"))  # C04: NULL fails
        self.assertEqual(evaluate_filters(rules, {**base, "buy_count": 2, "sell_count": 1})[:2], (REJECT_FILTER, "buy_count"))  # first in config order
        self.assertEqual(evaluate_filters(rules, {**base, "provider_gap_flag": True})[0], DATA_INCOMPLETE)  # C06
        self.assertEqual(evaluate_filters((), {"provider_gap_flag": False})[0], PASS)  # C07


class StrategyCTests(unittest.TestCase):
    def test_creator_fill_policy_validation_and_default_compatibility(self):
        default = cconfig()
        self.assertNotIn("creator_sold_before_fill", default.strategy)
        self.assertEqual(default.config_hash, cconfig().config_hash)
        for mode in ("LEGACY_FREE_CANCEL", "TX_LANDS"):
            h = make_c(strategy={"creator_sold_before_fill": mode})
            self.addCleanup(h.conn.close)
            self.assertEqual(h.owner.strategy.cancel_entry_if_creator_sold_before_fill(), mode == "LEGACY_FREE_CANCEL")
        with self.assertRaises(ConfigError):
            cconfig(strategy={"creator_sold_before_fill": "FORCE_FILL"})
        with self.assertRaises(ConfigError):
            base_config(strategy={"creator_sold_before_fill": "TX_LANDS"})

    def test_tx_lands_executes_buy_and_creator_exit_with_costs(self):
        h = make_c(strategy={"creator_sold_before_fill": "TX_LANDS"}, latency_model={"entry_delay_ms": 100, "exit_delay_ms": 0})
        self.addCleanup(h.conn.close)
        qualified(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 120, tok=10_000), end_of(h) + 50 * NS_PER_MS)
        h.advance(end_of(h) + 100 * NS_PER_MS)
        self.assertEqual(h.owner.counters.entry_cancelled_creator_sold, 0)
        self.assertEqual(h.position().state, PositionState.CLOSED)
        self.assertEqual(h.position().exit_reason, ExitReason.CREATOR_SELL)
        attempts = h.rows("SELECT side,status,fees_charged_lamports FROM execution_attempts ORDER BY submit_mono_ns")
        self.assertEqual([(r["side"], r["status"]) for r in attempts], [("BUY", "FILLED"), ("SELL", "FILLED")])
        self.assertTrue(all(r["fees_charged_lamports"] > 0 for r in attempts))

    def test_tx_lands_never_forces_fill_on_stale_reserves(self):
        h = make_c(strategy={"creator_sold_before_fill": "TX_LANDS"}, latency_model={"entry_delay_ms": 800, "exit_delay_ms": 0}, execution={"quote_max_age_ms": 250})
        self.addCleanup(h.conn.close)
        qualified(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 120, tok=10_000), end_of(h) + 100 * NS_PER_MS)
        h.advance(end_of(h) + 800 * NS_PER_MS)
        self.assertEqual(h.owner.counters.entry_fills, 0)
        self.assertEqual(h.owner.counters.entry_cancelled_creator_sold, 0)
        self.assertEqual(h.rows("SELECT status FROM execution_attempts WHERE side='BUY'")[0][0], "UNKNOWN")

    def test_free_cancel_report_and_research_cannot_qualify(self):
        from darkbot.backtest import Variant, variant_status
        from darkbot.reporting import summary_markdown
        h = make_c(latency_model={"entry_delay_ms": 100, "exit_delay_ms": 0})
        self.addCleanup(h.conn.close)
        qualified(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 120, tok=10_000), end_of(h) + 50 * NS_PER_MS)
        h.finish(end_of(h) + 100 * NS_PER_MS)
        report = h.report()
        self.assertEqual(report["economic"]["entry_cancelled_free_of_cost"], 1)
        self.assertEqual(report["economic"]["entry_cancellations_by_reason"], {"CREATOR_SOLD_BEFORE_FILL": 1})
        self.assertEqual(report["model_assumptions"]["creator_sold_before_fill"], "LEGACY_FREE_CANCEL")
        self.assertIn("kostenlose Creator-Stornos: 1", summary_markdown(report))
        self.assertEqual(report["qualification_status"], "UNQUALIFIED_DATA")
        self.assertIn("LEGACY_FREE_CANCEL_DIAGNOSTIC_ONLY", report["qualification_reasons"][0])
        v = Variant("positive", "1.5", 25, 25, "1.0", 0, 1, False)
        boot = {"status": "OK", "blocks": 100, "ci_low": 100}
        for split in ("train", "validation", "test", "forward"):
            self.assertEqual(variant_status(v, split, boot, [], h.cfg.research, h.cfg), "DIAGNOSTIC_LEGACY_FREE_CANCEL_CANNOT_QUALIFY")
        empty = make_c()
        self.addCleanup(empty.conn.close)
        self.assertEqual(empty.report()["qualification_status"], "UNQUALIFIED_DATA")

    def test_C01_non_sol_quote_never_decides(self):
        from darkbot.market import b58encode, encode_fields
        raw = json.loads(json.dumps(cconfig().raw))
        raw["protocol"]["create_event_tail_layouts"].append({"layout_id": "V2", "tail_fields": [["quote_mint", "pubkey"]], "quote_mint_field": "quote_mint", "implied_quote_asset": None,
                                                             "verification_status": "VERIFIED_TEST_FIXTURE", "token_program": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"})
        from darkbot.config import build_runtime_config
        h = Harness(build_runtime_config(ROOT / "config/solana_pump/strategy_c_paper_explore_01.json", raw))
        tail = encode_fields([("quote_mint", b58encode(b"USDC".ljust(32, b"\0")))], [["quote_mint", "pubkey"]])
        h.feed("helius_standard", create_payload(tail=tail), START)
        self.assertNotIn(MINT_A, h.owner.strategy.observations)
        h.advance(end_of(h))
        self.assertEqual(h.owner.counters.entry_intents, 0)
        self.assertFalse(h.rows("select * from strategy_observations"))

    def test_C02_create_transaction_trades_only_feed_dev_buy_and_t0_price(self):
        h = make_c()
        h.feed("helius_standard", create_payload(**dev_buy("0.05")), START)
        tracker = h.owner.strategy.observations[MINT_A]
        self.assertEqual((tracker.buy_count, tracker.sell_count), (0, 0))
        self.assertGreater(tracker.t0_vsol, V_SOL0)  # old t0 reserves = state after the creator's initial buy
        buys(h, [BUYER, BUYER2]); h.advance(end_of(h))
        row, f = observation(h)
        self.assertEqual((f["buy_count"], f["unique_buyers"], Decimal(f["dev_initial_buy"])), (2, 2, Decimal("0.05")))
        self.assertEqual((row["buy_count"], row["unique_buyers"], row["qualified"], row["decision_action"]), (2, 2, 1, PASS))

    def test_C03_exact_cutoff_inclusive_then_frozen(self):
        h = make_c(); end = end_of(h)
        h.feed("helius_standard", create_payload(**dev_buy()), START)
        h.feed("helius_standard", pump_frame(h, MINT_A, 20_000_000, "b0", 101, BUYER), START + 500 * NS_PER_MS)
        h.feed("helius_standard", pump_frame(h, MINT_A, 20_000_000, "boundary", 102, BUYER2), end)
        h.advance(end)
        self.assertEqual(h.owner.counters.entry_intents, 1)
        h.feed("helius_standard", pump_frame(h, MINT_A, 20_000_000, "late", 103, _pubkey("late")), end + 1)
        self.assertEqual(observation(h)[1]["buy_count"], 2)

    def test_C04_null_feature_fails_filter(self):
        h = make_c()
        h.feed("helius_standard", create_payload(), START)  # no creator initial buy -> dev_initial_buy NULL
        buys(h, [BUYER, BUYER2]); h.advance(end_of(h))
        row, f = observation(h)
        self.assertIsNone(f["dev_initial_buy"])
        self.assertEqual((row["qualified"], row["decision_action"], row["first_failed_filter"]), (0, REJECT_FILTER, "dev_initial_buy"))
        self.assertEqual(h.owner.counters.ineligible, {"REJECT_FILTER_dev_initial_buy": 1})
        self.assertEqual(h.owner.counters.entry_intents, 0)

    def test_C05_filter_miss_records_every_evaluation(self):
        h = make_c()
        h.feed("helius_standard", create_payload(**dev_buy("0.30")), START)
        buys(h, [BUYER, BUYER]); h.advance(end_of(h))
        row, _ = observation(h)
        verdicts = {e["feature"]: e["passed"] for e in json.loads(row["filter_evaluations_json"])}
        self.assertEqual(verdicts, {"buy_count": True, "unique_buyers": False, "net_buy_flow_sol": True, "dev_initial_buy": False})
        self.assertEqual(row["first_failed_filter"], "unique_buyers")

    def test_C06_gap_in_window_is_data_incomplete(self):
        h = make_c()
        h.feed("helius_standard", create_payload(**dev_buy()), START); buys(h, [BUYER, BUYER2])
        for f in h.cfg.feeds:
            h.owner.on_feed_health(f.id, False, START + 950 * NS_PER_MS, "test")
        for f in h.cfg.feeds:
            h.owner.on_feed_health(f.id, True, START + 960 * NS_PER_MS)
        h.advance(end_of(h))
        row, f = observation(h)
        self.assertEqual((row["status"], row["qualified"], row["decision_action"]), (DATA_INCOMPLETE, None, DATA_INCOMPLETE))
        self.assertTrue(f["provider_gap_flag"])
        self.assertEqual(h.owner.counters.entry_intents, 0)

    def test_C07_empty_filters_pass_every_tradable_launch(self):
        h = make_c("smoke_all")
        h.feed("helius_standard", create_payload(), START)
        buys(h, [BUYER], first_ms=900); h.advance(end_of(h))
        self.assertEqual(observation(h)[0]["qualified"], 1)
        self.assertEqual(h.owner.counters.entry_intents, 1)

    def test_C07b_state_freshness_is_the_shared_engine_rule_not_strategy_c(self):
        """Old backend: last observed state, any age. Shared engine: reserves older than execution.quote_max_age_ms are
        never a quote basis. Same filter PASS, different engine outcome - visible as PROXY_STATE_STALE, switchable in config."""
        for max_age_ms, intents, skipped in ((250, 0, {"PROXY_STATE_STALE": 1}), (600_000, 1, {})):
            h = make_c("smoke_all", execution={"quote_max_age_ms": max_age_ms})
            h.feed("helius_standard", create_payload(), START)
            buys(h, [BUYER], first_ms=100); h.advance(end_of(h))
            self.assertEqual(observation(h)[0]["qualified"], 1)
            self.assertEqual((h.owner.counters.entry_intents, h.owner.counters.ineligible), (intents, skipped))

    def test_C08_busy_slot_discards_candidate_for_good(self):
        h = make_c(); qualified(h)
        second = end_of(h) + NS_PER_MS
        qualified(h, MINT_B, second, "second", sig="c2")
        self.assertEqual(h.owner.counters.entry_intents, 1)
        self.assertEqual(h.owner.counters.ineligible.get("SLOT_LIMIT"), 1)
        self.assertEqual(observation(h, MINT_B)[0]["qualified"], 1)  # the filter verdict itself stays PASS
        h.feed("helius_standard", create_payload(MINT_B, sig="c2-again", **dev_buy()), second + 5_000 * NS_PER_MS)
        self.assertNotIn(MINT_B, h.owner.strategy.observations)  # decided exactly once
        h10 = make_c(portfolio={"max_open_positions": 10}); qualified(h10); qualified(h10, MINT_B, end_of(h10) + NS_PER_MS, "second", sig="c2")
        self.assertEqual(h10.owner.counters.entry_fills, 2)

    def test_C09_creator_sell_during_observation_no_entry_no_cost(self):
        h = make_c()
        h.feed("helius_standard", create_payload(**dev_buy()), START)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 101, tok=10_000), START + 50 * NS_PER_MS)
        buys(h, [BUYER, BUYER2]); h.advance(end_of(h))
        self.assertEqual(observation(h)[0]["qualified"], 1)
        self.assertEqual(h.owner.counters.entry_intents, 0)
        self.assertEqual(h.owner.counters.ineligible, {"CREATOR_ALREADY_SOLD": 1})
        self.assertFalse(h.rows("select * from ledger"))

    def test_C10_creator_sell_before_modeled_fill_cancels_entry_free(self):
        h = make_c(latency_model={"entry_delay_ms": 800, "exit_delay_ms": 800}); qualified(h)
        p = h.position(); cash = h.owner.portfolio.cash_lamports + p.reserved_cash_lamports
        self.assertEqual(p.state, PositionState.ENTRY_PENDING)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 120, tok=10_000), end_of(h) + 400 * NS_PER_MS)
        h.advance(end_of(h) + 800 * NS_PER_MS)
        self.assertEqual((p.state, p.unresolved_reason, p.net_pnl_lamports), (PositionState.ENTRY_FAILED, "CREATOR_SOLD_BEFORE_FILL", 0))
        self.assertEqual((h.owner.portfolio.cash_lamports, h.owner.portfolio.reserved_lamports), (cash, 0))
        attempt = h.rows("select status, fees_charged_lamports from execution_attempts")[0]
        self.assertEqual(tuple(attempt), (AttemptStatus.CANCELLED_CREATOR_SOLD.value, 0))
        self.assertEqual(h.rows("select action, reason from decisions where action='ENTRY_CANCEL'")[0]["reason"], "CREATOR_SOLD_BEFORE_FILL")
        self.assertFalse(h.rows("select * from ledger")); self.assertFalse(h.fk_check()); self.assertFalse(h.disk_errors)
        self.assertEqual(h.owner.occupied_slots(), 0)

    def test_C10b_other_policies_never_cancel_on_creator_sell(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 800, "exit_delay_ms": 0}))
        h.feed("helius_standard", create_payload(), START)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 101, tok=10_000), START + 700 * NS_PER_MS)
        h.advance(START + 800 * NS_PER_MS)
        self.assertEqual(h.owner.counters.entry_cancelled_creator_sold, 0)
        self.assertEqual(h.owner.counters.entry_fills, 1)

    def test_C12_fixed_swap_budget_contains_pump_fees_network_and_rent_on_top(self):
        h = make_c(); qualified(h)
        budget = h.cfg.strategy["entry_amount_lamports"]
        buy = h.rows("select * from execution_attempts where side='BUY'")[0]
        self.assertLessEqual(buy["amount_in_base_units"], budget)
        self.assertGreater(buy["amount_in_base_units"], budget - 10)
        self.assertEqual(buy["entry_curve_input_lamports"] + buy["protocol_fee_lamports"] + buy["creator_fee_lamports"] + buy["other_protocol_fee_lamports"], buy["amount_in_base_units"])
        self.assertGreater(h.position().entry_economic_cost_lamports, budget)
        self.assertFalse(h.rows("select * from entry_sizing"))  # no USD/FX evidence involved in FIXED_SOL

    def test_C13_creator_sell_after_fill_beats_target_on_same_state(self):
        h = make_c(); qualified(h); p = h.position()
        self.assertEqual(p.state, PositionState.OPEN)
        h.owner.engine.net_liquidation_value = lambda *a: p.entry_economic_cost_lamports * 5
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 130, tok=10_000), end_of(h) + NS_PER_MS)
        h.advance(end_of(h) + NS_PER_MS)
        self.assertEqual({r for r, _ in p.latched_triggers}, {ExitReason.CREATOR_SELL, ExitReason.TARGET})
        self.assertEqual(p.exit_reason, ExitReason.CREATOR_SELL)

    def test_C14_net_target_boundary(self):
        s = make_c().owner.strategy
        self.assertFalse(s.target_reached(1999, 1000, Decimal("2.0")))
        self.assertTrue(s.target_reached(2000, 1000, Decimal("2.0")))

    def test_C15_stop_multiple_exits(self):
        h = make_c(); qualified(h); p = h.position()
        h.owner.engine.net_liquidation_value = lambda *a: int(p.entry_economic_cost_lamports * Decimal("0.66"))
        h.owner._position_tick(MINT_A, end_of(h)); self.assertFalse(p.latched_triggers)
        h.owner.engine.net_liquidation_value = lambda *a: int(p.entry_economic_cost_lamports * Decimal("0.65"))
        h.owner._position_tick(MINT_A, end_of(h)); h.advance(end_of(h))
        self.assertEqual(p.exit_reason, ExitReason.STOP)
        self.assertEqual(h.owner.counters.exits_stop, 1)
        h.finish(end_of(h) + NS_PER_MS)
        self.assertEqual(h.report()["economic"]["exits"]["STOP"], 1)

    def test_C16_stop_zero_holds_until_net_value_is_gone(self):
        h = make_c("pre_pump_01")
        self.assertFalse(h.cfg.strategy["ordinary_stop_enabled"])
        s = h.owner.strategy
        for value in (900, 500, 100, 1):
            self.assertFalse(s.stop_reached(value, 1000))
        self.assertTrue(s.stop_reached(0, 1000)); self.assertTrue(s.stop_reached(-5, 1000))
        self.assertFalse(s.stop_reached(None, 1000))
        self.assertTrue(stop_reached(650, 1000, Decimal("0.65"))); self.assertFalse(stop_reached(651, 1000, Decimal("0.65")))

    def test_C17_timeout_from_successful_fill(self):
        h = make_c(latency_model={"entry_delay_ms": 100, "exit_delay_ms": 0}); qualified(h)
        h.advance(end_of(h) + 100 * NS_PER_MS); p = h.position()
        self.assertEqual(p.timeout_due_mono_ns, p.entry_fill_mono_ns + h.cfg.strategy["strategy_timeout_ms"] * NS_PER_MS)
        due = p.timeout_due_mono_ns
        h.feed("helius_standard", pump_frame(h, MINT_A, 10_000, "fresh", 150), due - NS_PER_MS)
        h.advance(due - 1); self.assertEqual(p.state, PositionState.OPEN)
        h.advance(due); self.assertEqual(p.exit_reason, ExitReason.TIMEOUT)

    def test_C18_same_timestamp_priority(self):
        self.assertEqual(EXIT_PRIORITY, (ExitReason.CREATOR_SELL, ExitReason.TARGET, ExitReason.STOP, ExitReason.TIMEOUT))
        self.assertEqual(pick_exit_trigger([(ExitReason.TIMEOUT, 5), (ExitReason.STOP, 5)])[0], ExitReason.STOP)
        self.assertEqual(pick_exit_trigger([(ExitReason.STOP, 5), (ExitReason.TARGET, 5), (ExitReason.CREATOR_SELL, 5)])[0], ExitReason.CREATOR_SELL)
        self.assertEqual(pick_exit_trigger([(ExitReason.CREATOR_SELL, 9), (ExitReason.STOP, 5)])[0], ExitReason.STOP)  # first causal trigger wins

    def test_C19_all_six_legacy_profiles_load_with_own_hash_and_exact_parameters(self):
        old_names = {"fast_creator_hold_2x": "strategy_fast_creator_hold_2x", "pre_pump_01": "strategy_pre_pump_01", "launch_sniper_0ms_15x": "strategy_launch_sniper_15x",
                     "paper_explore_01": "strategy_paper_explore_01", "example": "strategy_example", "smoke_all": "strategy_smoke_all"}
        hashes = set()
        for profile in PROFILES:
            cfg = load_runtime_config(ROOT / f"config/solana_pump/strategy_c_{profile}.json")
            old = json.loads((ROOT / "old_system/backend/config" / f"{old_names[profile]}.json").read_text())
            st = cfg.strategy
            self.assertEqual(cfg.strategy_id, "LEGACY_BACKEND_FILTER_V1")
            self.assertEqual((st["candidate_name"], st["observation_ms"], st["entry_amount_lamports"], st["strategy_timeout_ms"]),
                             (old["candidate_name"], old["observation_ms"], old["trade_amount_lamports"], old["max_hold_ms"]))
            self.assertEqual((Decimal(st["target_net_multiple"]), Decimal(st["stop_net_multiple"])), (Decimal(old["target_net_multiple"]), Decimal(old["stop_net_multiple"])))
            self.assertEqual([(f["feature"], None if f["min"] is None else Decimal(f["min"]), None if f["max"] is None else Decimal(f["max"])) for f in st["filters"]],
                             [(f["feature"], None if f["min"] is None else Decimal(str(f["min"])), None if f["max"] is None else Decimal(str(f["max"]))) for f in old["filters"]])
            self.assertEqual((cfg.latency_model["entry_delay_ms"], cfg.latency_model["exit_delay_ms"]), (old["entry_latency_ms"], old["entry_latency_ms"]))
            self.assertEqual((cfg.execution["slippage_bps"], cfg.portfolio["max_open_positions"]), (old["max_slippage_bps"], 1))
            self.assertEqual(cfg.execution["quote_max_age_ms"], cfg.research["research_horizon_ms"])  # old: last observed state, any age within the tracker horizon
            hashes.add(cfg.strategy_config_hash)
        self.assertEqual(len(hashes), len(PROFILES))

    def test_C19b_hard_wired_legacy_rules_are_not_configurable(self):
        for strategy in ({"creator_sell_exit_enabled": False}, {"target_exit_enabled": False}, {"stop_net_multiple": "1"}, {"stop_net_multiple": "0", "ordinary_stop_enabled": True},
                         {"entry_amount_semantics": "CURVE_INPUT_EXCLUDING_ALL_FEES_AND_RENT"}, {"same_event_exit_priority": ["CREATOR_SELL", "TARGET", "TIMEOUT"]},
                         {"filters": [{"feature": "provider_gap_flag", "min": None, "max": None}]}, {"filters": [{"feature": "research_estimated_price_impact", "min": None, "max": "1"}]},
                         {"filters": [{"feature": "buy_count", "min": 1, "max": None}, {"feature": "buy_count", "min": 2, "max": None}]},
                         {"filters": [{"feature": "buy_count", "min": 5, "max": 2}]}, {"filters": [{"feature": "buy_count", "min": 1.5, "max": None}]}):
            with self.assertRaises(ConfigError, msg=str(strategy)):
                cconfig(strategy=strategy)
        with self.assertRaises(ConfigError):  # filters stay forbidden for the other policies
            base_config(strategy={"filters": [{"feature": "buy_count", "min": 1, "max": None}]})

    def test_C20_zero_observation_decides_directly_from_create(self):
        h = make_c("launch_sniper_0ms_15x")
        h.feed("helius_standard", create_payload(), START)
        self.assertFalse(h.owner.strategy.observations)
        self.assertEqual(h.owner.counters.entry_intents, 1)
        h.advance(START)
        self.assertEqual(h.position().state, PositionState.OPEN)
        row, f = observation(h)
        self.assertEqual((row["observation_start"], row["observation_end"], row["qualified"], f["buy_count"]), (START, START, 1, 0))

    def test_C21_paper_and_replay_take_identical_decisions(self):
        h = make_c(); qualified(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "csell", 140, tok=10_000), end_of(h) + 50 * NS_PER_MS)
        h.advance(end_of(h) + 50 * NS_PER_MS)
        self.assertEqual(h.position().exit_reason, ExitReason.CREATOR_SELL)
        h.finish(end_of(h) + 100 * NS_PER_MS)
        replay = run_replay(h.conn, load_replay_input(h.conn, "run", h.cfg), h.cfg, "replayed", "camp", persist=True, persist_observations=True)
        self.assertEqual(digest(h.owner), replay.digest)
        rows = h.rows("select features_json, qualified, decision_action from strategy_observations where mint=? order by run_id", (MINT_A,))
        self.assertEqual(len(rows), 2); self.assertEqual(tuple(rows[0]), tuple(rows[1]))
        self.assertFalse(h.fk_check()); self.assertFalse(h.disk_errors)

    def test_C22_run_identity_separate_from_a_and_b(self):
        a, b, c = Harness(), Harness(load_runtime_config(ROOT / "config/solana_pump/strategy_b.json")), make_c()
        self.assertEqual(len({x.cfg.strategy_id for x in (a, b, c)}), 3)
        self.assertEqual(len({x.cfg.strategy_config_hash for x in (a, b, c)}), 3)
        self.assertEqual(tuple(c.rows("select strategy_id, strategy_config_hash from runs")[0]), (c.cfg.strategy_id, c.cfg.strategy_config_hash))

    def test_C23_observation_censored_at_recording_end(self):
        h = make_c(); h.feed("helius_standard", create_payload(**dev_buy()), START); h.finish(START + 500 * NS_PER_MS)
        row = h.rows("select * from strategy_observations")[0]
        self.assertEqual(row["status"], "CENSORED_RUN_END"); self.assertIsNone(row["qualified"])


    def test_C24_ab_replay_normalizes_legacy_latency_and_quote_age(self):
        a = load_runtime_config(ROOT / "config/solana_pump/phase1.json")
        c = load_runtime_config(ROOT / "config/solana_pump/strategy_c_pre_pump_01.json")
        _a, normalized_c, note = _comparison_configs(a, c)
        self.assertEqual(normalized_c.execution["quote_max_age_ms"], a.execution["quote_max_age_ms"])
        self.assertEqual(note["quote_max_age_ms"]["config_b_original"], 600000)
        self.assertEqual(note["quote_max_age_ms"]["value"], 250)
        self.assertEqual(note["latency_model"], "RESEARCH_SCENARIO_OVERRIDES_PROFILE_DEFAULTS")
        self.assertEqual(c.execution["quote_max_age_ms"], 600000)  # original profile is untouched


    def test_C25_replay_slot_limit_scales_fixed_capital(self):
        c = load_runtime_config(ROOT / "config/solana_pump/strategy_c_pre_pump_01.json")
        base = c.portfolio["initial_paper_balance_lamports"]
        variant = c.with_overrides(portfolio=c.portfolio_limit_overrides(10))
        self.assertEqual(variant.portfolio["max_open_positions"], 10)
        self.assertEqual(variant.portfolio["initial_paper_balance_lamports"], base * 10)

    def test_C26_usd_per_slot_keeps_fx_derived_capital(self):
        a = load_runtime_config(ROOT / "config/solana_pump/strategy_a_creator_exit_usd_slots.json")
        variant = a.with_overrides(portfolio=a.portfolio_limit_overrides(10))
        self.assertEqual(variant.portfolio["max_open_positions"], 10)
        self.assertEqual(variant.portfolio["initial_paper_balance_lamports"], 0)
        self.assertEqual(variant.portfolio["initial_paper_balance_mode"], "USD_PER_SLOT")


if __name__ == "__main__":
    unittest.main()
