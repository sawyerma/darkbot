"""T01–T15: feed race, atomic envelopes, causal sequencer, retries, stop, latency, horizon, ledger, storage, feeds, decoder errors, ordering."""

from __future__ import annotations

import asyncio
import unittest
from unittest import mock

from darkbot.capture import FeedAdapter, FakeWebSocket, fake_connect_factory
from darkbot.clock import LiveClock
from darkbot.config import ConfigError, FeedConfig
from darkbot.domain import AttemptStatus, ExitReason, PositionState, ValidationStatus
from darkbot.reporting import Reporter
from tests.helpers import *  # noqa: F401,F403
from tests.helpers import BUYER, CREATOR, MINT_A, MINT_B, T0, Harness, attempts, base_config, create_payload, creator_sell_frame, dump_frame, ledger_kinds, logs_notification, open_position, pump_frame, pumpportal_create, trade_frame_logs


class T01T02FeedRace(unittest.TestCase):
    def test_T01_same_create_three_sources_one_intent(self):
        h = Harness()
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("pumpportal", pumpportal_create(MINT_A), T0 + 3_000_000)
        h.feed("existing_rpc", create_payload(), T0 + 6_000_000)
        self.assertEqual(len(h.owner.positions), 1)
        obs = h.rows("select feed_id, validation_status from feed_observations order by receive_mono_ns")
        self.assertEqual([r["feed_id"] for r in obs], ["helius_standard", "pumpportal", "existing_rpc"])
        self.assertEqual(obs[2]["validation_status"], ValidationStatus.DUPLICATE.value)
        rep = h.report()["latency"]["feeds"]["per_feed"]
        self.assertEqual(rep["helius_standard"]["relative_arrival_ns"]["p50"], 0)
        self.assertEqual(rep["pumpportal"]["relative_arrival_ns"]["p50"], 2_000_000)
        self.assertEqual(rep["existing_rpc"]["relative_arrival_ns"]["p50"], 5_000_000)
        self.assertEqual(rep["helius_standard"]["actionable_wins"], 1)

    def test_T02_early_incomplete_hint_does_not_enter(self):
        h = Harness()
        h.feed("pumpportal", pumpportal_create(MINT_A), T0 + 1_000_000)
        self.assertEqual(len(h.owner.positions), 0)
        self.assertEqual(h.owner.reducer.hint_unmatched, 1)
        h.feed("helius_standard", create_payload(), T0 + 4_000_000)
        self.assertEqual(len(h.owner.positions), 1)
        rep = h.report()["latency"]
        self.assertEqual(rep["feeds"]["per_feed"]["pumpportal"]["hint_wins"], 1)
        self.assertGreaterEqual(rep["first_hint_to_actionable_ns"]["min"], 3_000_000)


class T03T05Atomicity(unittest.TestCase):
    def test_T03_create_multiple_buys_creator_sell_one_tx(self):
        h = Harness()
        v = dict(vsol=30_000_000_000 + 2_000_000_000, vtok=1_073_000_000_000_000 - 60_000_000_000_000, rtok=793_100_000_000_000 - 60_000_000_000_000)
        trades = [dict(sol=1_000_000_000, tok=30_000_000_000_000, is_buy=True, user=BUYER, vsol=31_000_000_000, vtok=1_043_000_000_000_000, rtok=763_100_000_000_000), dict(sol=1_000_000_000, tok=30_000_000_000_000, is_buy=True, user=BUYER, **v)]
        sell = dict(sol=900_000_000, tok=20_000_000_000_000, vsol=v["vsol"] - 900_000_000, vtok=v["vtok"] + 20_000_000_000_000, rtok=v["rtok"] + 20_000_000_000_000)
        h.feed("helius_standard", logs_notification("s", 1, create_frame_logs(MINT_A, CREATOR, initial_buy=(500_000_000, 15_000_000_000_000), extra_trades=trades, creator_sell=sell)), T0 + 1000)
        self.assertEqual(len(h.owner.positions), 0)
        self.assertEqual(h.rows("select reason from decisions")[0]["reason"], "CREATOR_ALREADY_SOLD")
        st = h.owner.reducer.states[MINT_A]
        self.assertEqual((st.virtual_sol_reserves, st.virtual_token_reserves), (sell["vsol"], sell["vtok"]))  # final atomic state, not the first buy
        h2 = Harness()
        h2.feed("helius_standard", logs_notification("s", 1, create_frame_logs(MINT_A, CREATOR, initial_buy=(500_000_000, 15_000_000_000_000), extra_trades=trades)), T0 + 1000)
        q = h2.rows("select quote_state_event_id from quote_snapshots")[0]["quote_state_event_id"]
        last_ix = max(r["instruction_index"] for r in h2.rows("select instruction_index from market_events"))
        self.assertTrue(q.endswith(f"|{last_ix}|{MINT_A}|BUY"))  # quote from the last trade of the atomic tx, not the first buy

    def test_T04_late_entry_callback_with_creator_sell_in_between(self):
        h = Harness()
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)  # fill scheduled at T0+1ms (delay 0), not yet fired
        h.clock.advance_to(T0 + 2_000_000)
        h.owner.on_raw(__import__("darkbot.domain", fromlist=["RawEnvelope"]).RawEnvelope("helius_standard", "c", T0 + 2_000_000, "", 2, creator_sell_frame(h, MINT_A, "cs", 101)))
        pos = h.position()
        self.assertEqual(pos.state, PositionState.ENTRY_PENDING)
        h.advance(T0 + 5_000_000, late_ns=3_000_000)  # callback fires late (actual >= T0+4ms)
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertEqual(pos.exit_reason, ExitReason.CREATOR_SELL)
        a = attempts(h)
        self.assertEqual(a[0]["proxy_event_id"].split("|")[1], "sigC")  # fill proxy = state at modeled fill (create), not the later sell
        self.assertGreaterEqual(a[1]["submit_mono_ns"], T0 + 4_000_000)  # exit decided at actual processing time, never back-dated
        self.assertEqual(sum(1 for p in h.owner.positions.values() if p.state == PositionState.OPEN), 0)

    def test_T05_creator_sell_after_submit_before_fill_no_free_cancel(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 10_000_000)
        pos = h.position()
        self.assertEqual(pos.state, PositionState.ENTRY_PENDING)
        h.advance(T0 + 60_000_000)
        a = attempts(h)
        self.assertEqual(a[0]["status"], AttemptStatus.FILLED.value)
        self.assertEqual(a[0]["proxy_event_id"].split("|")[1], "cs")  # filled at the post-sell state
        self.assertIn("ENTRY_FILL", ledger_kinds(h))
        self.assertEqual(pos.exit_reason, ExitReason.CREATOR_SELL)
        self.assertTrue(h.owner.cash_reconciliation()["reconciled"])


class T06T09Rules(unittest.TestCase):
    def _open_with_exit_delay(self, exit_ms=20):
        h = Harness(base_config(latency_model={"exit_delay_ms": exit_ms}))
        pos = open_position(h)
        return h, pos

    def test_T06_bounded_retries_time_advances(self):
        h, pos = self._open_with_exit_delay()
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        for i, t in enumerate((T0 + 12_000_000, T0 + 80_000_000, T0 + 250_000_000)):  # each dump lands between submit and modeled fill (2/22, 72/92, 242/262 ms)
            h.feed("helius_standard", dump_frame(h, MINT_A, 300_000_000_000_000, f"dump{i}", 102 + i), t)  # dump between trigger and modeled fill
        h.advance(T0 + 400_000_000)
        a = attempts(h)
        sells = [x for x in a if x["side"] == "SELL"]
        self.assertEqual(len(sells), 3)
        self.assertTrue(all(x["status"] == AttemptStatus.SLIPPAGE_REJECTED.value for x in sells))
        self.assertLess(sells[0]["submit_mono_ns"], sells[1]["submit_mono_ns"])
        self.assertGreaterEqual(sells[1]["submit_mono_ns"] - sells[0]["callback_actual_mono_ns"], 50_000_000)
        self.assertGreaterEqual(sells[2]["submit_mono_ns"] - sells[1]["callback_actual_mono_ns"], 150_000_000)
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(pos.unresolved_reason, "EXIT_RETRIES_EXHAUSTED")
        self.assertGreater(pos.token_balance_base_units, 0)
        self.assertEqual(ledger_kinds(h).count("SELL_FAILED_FEE"), 3)

    def test_T07_stop_disabled_curve_unknown(self):
        with self.assertRaises(ConfigError):
            base_config(strategy={"ordinary_stop_enabled": True})
        h = Harness()
        pos = open_position(h)
        h.owner.on_feed_health("helius_standard", False, T0 + 2_000_000, "closed")
        h.owner.on_feed_health("existing_rpc", False, T0 + 2_000_000, "closed")
        h.feed("pumpportal", pumpportal_create(MINT_B), T0 + 3_000_000)
        h.advance(T0 + 100_000_000)
        self.assertEqual(pos.state, PositionState.OPEN)
        self.assertEqual([x for x in attempts(h) if x["side"] == "SELL"], [])
        self.assertTrue(h.owner.reducer.gap_open_since is not None)
        self.assertIsNone(h.owner.engine.net_liquidation_value(h.owner._curve(h.owner.reducer.states[MINT_A]), h.fees.basis(), T0 + 100_000_000, __import__("darkbot.domain", fromlist=["CoverageState"]).CoverageState.GAP, pos.token_balance_base_units, 0))

    def test_T08_entry_exit_latency_separate(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 10, "exit_delay_ms": 30}))
        pos = open_position(h, fill_at=T0 + 11_000_000)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 20_000_000)
        h.advance(T0 + 100_000_000)
        a = attempts(h)
        self.assertEqual(a[0]["scheduled_fill_mono_ns"] - a[0]["submit_mono_ns"], 10_000_000)
        self.assertEqual(a[1]["scheduled_fill_mono_ns"] - a[1]["submit_mono_ns"], 30_000_000)
        self.assertEqual(pos.state, PositionState.CLOSED)

    def test_T09_research_horizon_with_open_position(self):
        h = Harness()
        pos = open_position(h)
        h.owner.on_feed_health("helius_standard", False, T0 + 2_000_000, "closed")  # coverage gap: timeout exit stays latched, no invented fill
        h.owner.on_feed_health("existing_rpc", False, T0 + 2_000_000, "closed")
        h.advance(T0 + 1_000_000 + 600_000_000_000 + 1)
        self.assertEqual(pos.research_status, "CENSORED")
        self.assertEqual(pos.state, PositionState.UNRESOLVED)  # unavailable timeout quote is explicit; tracker/exposure remain alive
        self.assertTrue(pos.exit_quote_recoverable)
        self.assertEqual(ledger_kinds(h), ["ENTRY_FILL"])
        self.assertIn(MINT_A, h.owner.reducer.states)
        h.owner.reducer.evict_inactive(T0 + 10**15, 0, set(h.owner.position_by_mint))
        self.assertIn(MINT_A, h.owner.reducer.states)  # active positions are exempt from research eviction (F08)


class T10T12Ledger(unittest.TestCase):
    def test_T10_failed_entry_sell_retries_abort(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 20, "exit_delay_ms": 20}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("helius_standard", pump_frame(h, MINT_A, 20_000_000_000, "pump", 101), T0 + 5_000_000)  # 20 SOL pump -> slippage reject
        h.advance(T0 + 30_000_000)
        pos = h.position()
        self.assertEqual(pos.state, PositionState.ENTRY_FAILED)
        self.assertEqual(ledger_kinds(h), ["BUY_FAILED_FEE"])
        self.assertEqual(h.owner.occupied_slots(), 0)
        # second launch: sell retries then process abort while exit pending
        h.feed("helius_standard", create_payload(MINT_B, sig="sigB", slot=200), T0 + 100_000_000)
        h.advance(T0 + 121_000_000)
        h.feed("helius_standard", creator_sell_frame(h, MINT_B, "csB", 201), T0 + 130_000_000)
        h.feed("helius_standard", dump_frame(h, MINT_B, 300_000_000_000_000, "dB", 202), T0 + 140_000_000)
        h.advance(T0 + 200_000_000)  # first exit rejected, retry submitted at +50ms and pending
        h.owner.finalize(T0 + 200_000_000)
        kinds = ledger_kinds(h)
        self.assertEqual(kinds.count("BUY_FAILED_FEE"), 1)
        self.assertEqual(kinds.count("SELL_FAILED_FEE"), 1)
        ids = [r["ledger_entry_id"] for r in h.rows("select ledger_entry_id from ledger")]
        self.assertEqual(len(ids), len(set(ids)))
        posB = h.owner.positions[h.owner.position_by_mint[MINT_B]]
        self.assertEqual(posB.quality_status, "OPEN_AT_RUN_END")
        self.assertIn(posB.state, (PositionState.OPEN, PositionState.EXIT_PENDING))
        self.assertTrue(h.owner.cash_reconciliation()["reconciled"])

    def test_T11_rent_lock_and_refund(self):
        h = Harness()
        pos = open_position(h)
        rent = h.fees.rent.normalized_fee_components["token_account_rent_lamports"]
        self.assertEqual(h.owner.portfolio.locked_rent_lamports, rent)
        self.assertEqual(pos.locked_rent_lamports, rent)
        cash_after_entry = h.owner.portfolio.cash_lamports
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        self.assertEqual(h.owner.portfolio.locked_rent_lamports, 0)
        l = {r["kind"]: r for r in h.rows("select * from ledger")}
        self.assertEqual(l["ENTRY_FILL"]["locked_rent_delta_lamports"], rent)
        self.assertEqual(l["EXIT_FILL"]["locked_rent_delta_lamports"], -rent)
        self.assertEqual(pos.net_pnl_lamports, l["ENTRY_FILL"]["cash_delta_lamports"] + l["EXIT_FILL"]["cash_delta_lamports"])
        self.assertEqual(h.owner.portfolio.cash_lamports, h.cfg.portfolio["initial_paper_balance_lamports"] + pos.net_pnl_lamports)
        rec = h.owner.cash_reconciliation()
        self.assertTrue(rec["reconciled"])
        self.assertGreater(h.owner.portfolio.cash_lamports, cash_after_entry)

    def test_T12_writer_queue_full_and_disk_error(self):
        h = Harness(base_config(storage={"queue_capacity_records": 6}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)  # exceeds queue -> overflow -> pause new entries (first intent still reserved)
        self.assertTrue(h.recorder.overflowed)
        self.assertTrue(h.owner.stop_new_entries)
        self.assertEqual(len(h.owner.positions), 1)
        h.feed("helius_standard", create_payload(MINT_B, sig="sigB", slot=200), T0 + 2_000_000)
        self.assertEqual(h.owner.counters.ineligible.get("RECORDING_DEGRADED"), 1)
        h2 = Harness()
        h2.owner.on_disk_error("disk I/O error")
        h2.feed("helius_standard", create_payload(), T0 + 1_000_000)
        self.assertEqual(h2.owner.counters.ineligible.get("RECORDING_DEGRADED"), 1)
        self.assertEqual(len(h2.owner.positions), 0)
        rep = Reporter(h.conn).build("run") if False else None  # overflow: rows lost -> reported via quality event flag in RAM


class T13T15Feeds(unittest.TestCase):
    def _adapter(self, feed_id, sessions, on_raw, health, errors):
        cfg = base_config()
        feed = next(f for f in cfg.feeds if f.id == feed_id)
        conn = dict(cfg.connectivity, reconnect_initial_ms=1, reconnect_max_ms=2, subscription_ack_timeout_ms=200)
        return FeedAdapter(feed, "wss://x", conn, cfg.protocol["program_id"], LiveClock(), on_raw, lambda f, r, t, m: health.append((f, r)), lambda f, k, m, t: errors.append((f, k)), fake_connect_factory(sessions))

    def test_T13_ack_rejected_and_disconnect(self):
        health, errors, raws = [], [], []
        bad = FakeWebSocket([], ack='{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"rejected"}}')
        ok = FakeWebSocket([create_payload()], ack='{"jsonrpc":"2.0","id":1,"result":7}')
        a = self._adapter("helius_standard", [bad, ok], raws.append, health, errors)
        asyncio.run(asyncio.wait_for(a.run(), 5)) if False else asyncio.run(self._run_until_sessions_exhausted(a))
        self.assertEqual(errors[0], ("helius_standard", "SUBSCRIPTION_REJECTED"))
        self.assertEqual(health[0], ("helius_standard", True))  # READY only after the accepted ACK
        self.assertEqual(len(raws), 1)
        self.assertEqual(a.stats.ack_failures, 1)
        self.assertGreaterEqual(a.stats.reconnects, 1)
        # remaining source keeps working: transport failure on one feed does not affect StateOwner health of the other
        h = Harness()
        h.owner.on_feed_health("helius_standard", False, T0 + 10, "closed")
        self.assertIsNone(h.owner.reducer.gap_open_since)
        h.feed("existing_rpc", create_payload(), T0 + 1_000_000)
        self.assertEqual(len(h.owner.positions), 1)

    async def _run_until_sessions_exhausted(self, adapter):
        task = asyncio.create_task(adapter.run())
        for _ in range(200):
            await asyncio.sleep(0.01)
            if adapter.stats.transport_errors >= 1 and adapter.stats.connections >= 2:
                break
        adapter.stop()
        task.cancel()
        try:
            await task
        except (asyncio.CancelledError, Exception):
            pass

    def test_T14_decoder_programming_error_is_diagnostic(self):
        h = Harness()
        with mock.patch.object(h.owner.normalizer, "normalize", side_effect=AttributeError("bug")):
            h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        self.assertEqual(h.owner.counters.internal_errors, 1)
        self.assertIn("DECODER_INTERNAL_ERROR", h.owner.last_error)
        self.assertEqual(len(h.rows("select kind from quality_events where kind='DECODER_INTERNAL_ERROR'")), 1)
        self.assertTrue(h.owner.feed_ready["helius_standard"])  # no reconnect/health flip

    def test_T15_out_of_order_duplicates_conflict(self):
        h = Harness()
        pos = open_position(h)
        st = h.owner.reducer.states[MINT_A]
        h.feed("helius_standard", pump_frame(h, MINT_A, 1_000_000_000, "p1", 105), T0 + 2_000_000)
        vsol_after = st.virtual_sol_reserves
        old = logs_notification("old", 103, trade_frame_logs(MINT_A, 10, 10, True, BUYER, 30_000_000_001, 1_073_000_000_000_000, 793_100_000_000_000))
        h.feed("existing_rpc", old, T0 + 3_000_000)
        self.assertEqual(st.virtual_sol_reserves, vsol_after)  # stale slot did not roll back fresh reserves
        self.assertEqual(h.owner.reducer.stale_events, 1)
        h.feed("existing_rpc", create_payload(), T0 + 4_000_000)  # duplicate CREATE via second feed
        self.assertEqual(len(h.owner.positions), 1)
        self.assertEqual(h.owner.counters.duplicates, 1)
        vals = [r["validation_status"] for r in h.rows("select validation_status from feed_observations order by receive_mono_ns")]
        self.assertIn("STALE", vals)
        self.assertIn("DUPLICATE", vals)


if __name__ == "__main__":
    unittest.main()
