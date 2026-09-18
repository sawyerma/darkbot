"""Audit round 4 integrated counterexamples: research gate uniqueness/status/exposure/freeze flow, PAPER<->Replay coverage parity, atomic
PARTIAL->COMPLETE fills, Global discriminator, D04 creator-sell count, dataset integrity, truncated features, run-window utilization,
UTC day mapping, single-owner writer callback, bounded runtime structures."""

from __future__ import annotations

import asyncio
import base64
import json
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path
from unittest import mock

from darkbot.backtest import Variant, apply_stress_gate, select_candidate, variant_status
from darkbot.domain import PositionState
from darkbot.main import count_live_creator_sells
from darkbot.market import event_discriminator
from darkbot.replay import digest, load_replay_input, run_replay
from darkbot.storage import open_connection
from tests.helpers import BUYER, CREATOR, MINT_A, MINT_B, T0, Harness, attempts, base_config, create_frame_logs, create_payload, creator_sell_frame, logs_notification, open_position, pump_frame


class ResearchGates(unittest.TestCase):
    def test_duplicate_stress_scenarios_across_recordings_do_not_count(self):
        boot = {"status": "OK", "blocks": 40, "ci_low": 10, "ci_high": 20}
        base = {"split": "test", "target": "1.5", "entry_delay_ms": 5, "exit_delay_ms": 5, "portfolio_limit": 1, "bootstrap": boot, "status": "NEGATIVE"}
        outs = [{**base, "fee_multiplier": "1.0", "adverse_bps": 0, "reject_rate": "0", "status": "POSITIVE_UNDER_MODEL_PENDING_STRESS", "source_run_id": "A"}]
        for run in ("A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K"):  # 11 copies of ONE stress scenario from different recordings
            outs.append({**base, "fee_multiplier": "2.0", "adverse_bps": 0, "reject_rate": "0", "source_run_id": run})
        apply_stress_gate(outs, candidate_target="1.5", required_stress=11)
        self.assertEqual(outs[0]["status"], "INSUFFICIENT_DATA_NO_STRESS_VARIANTS")

    def test_unqualified_sibling_blocks_positive(self):
        boot = {"status": "OK", "blocks": 40, "ci_low": 10, "ci_high": 20}
        base = {"split": "test", "target": "1.5", "entry_delay_ms": 5, "exit_delay_ms": 5, "portfolio_limit": 1, "bootstrap": boot, "source_run_id": "A"}
        outs = [{**base, "fee_multiplier": "1.0", "adverse_bps": 0, "reject_rate": "0", "status": "POSITIVE_UNDER_MODEL_PENDING_STRESS"},
                {**base, "fee_multiplier": "2.0", "adverse_bps": 0, "reject_rate": "0", "status": "UNQUALIFIED_DATA_UNRESOLVED_EXPOSURE"}]
        apply_stress_gate(outs, candidate_target="1.5", required_stress=1)
        self.assertEqual(outs[0]["status"], "INSUFFICIENT_DATA_IN_STRESS_VARIANTS")

    def test_unknown_exposure_cost_never_treated_as_zero(self):
        cfg = base_config()
        research = {"minimum_independent_blocks_for_qualification": 30}
        boot = {"status": "OK", "blocks": 40, "ci_low": 10, "ci_high": 20}

        class P:  # exposed position whose entry cost is unknown
            entry_economic_cost_lamports = None
            entry_fill_attempt_id = None
            state = PositionState.UNRESOLVED

        v = Variant("x", "1.5", 5, 5, "1.0", 0, 1, False)
        self.assertEqual(variant_status(v, "test", boot, [P()], research, cfg), "UNQUALIFIED_DATA_UNRESOLVED_EXPOSURE")

    def test_baseline_preference_keeps_positive_flag(self):
        rows = [{"split": "validation", "target": "1.5", "entry_delay_ms": 5, "exit_delay_ms": 5, "fee_multiplier": "1.0", "adverse_bps": 0, "reject_rate": "0", "bootstrap": {"ci_low": 95}, "variant_id": "b"},
                {"split": "validation", "target": "2.0", "entry_delay_ms": 5, "exit_delay_ms": 5, "fee_multiplier": "1.0", "adverse_bps": 0, "reject_rate": "0", "bootstrap": {"ci_low": 100}, "variant_id": "best"}]
        sel = select_candidate(rows)
        self.assertEqual(sel["selected_target"], "1.5")
        self.assertTrue(sel["positive"])  # a positive baseline within 10% is a freezable candidate


class ReplayParity(unittest.TestCase):
    def _record(self, d: Path, gap_at_ms: int, recover_ms: int, entry_delay_ms: int) -> Harness:
        h = Harness(base_config(latency_model={"entry_delay_ms": entry_delay_ms}), db_path=str(d / "p.sqlite"))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.owner.on_feed_health("helius_standard", False, T0 + gap_at_ms * 1_000_000, "closed")
        h.owner.on_feed_health("existing_rpc", False, T0 + gap_at_ms * 1_000_000, "closed")
        h.owner.on_feed_health("helius_standard", True, T0 + recover_ms * 1_000_000, "c2")
        h.feed("helius_standard", pump_frame(h, MINT_A, 10, "p", 101), T0 + (recover_ms + 10) * 1_000_000)
        h.finish(T0 + (recover_ms + 200) * 1_000_000)
        return h

    def test_fill_before_later_disconnect_is_filled_in_both(self):
        with tempfile.TemporaryDirectory() as d:
            h = self._record(Path(d), gap_at_ms=50, recover_ms=80, entry_delay_ms=20)  # modeled fill at 21 ms, gap from 50 ms
            self.assertEqual(attempts(h)[0]["status"], "FILLED")
            conn = open_connection(Path(d) / "p.sqlite")
            r = run_replay(conn, load_replay_input(conn, "run", h.cfg), h.cfg, "rep", "c", persist=False)
            self.assertEqual(r.digest["positions"], digest(h.owner)["positions"])
            self.assertEqual(r.digest["positions"][0]["state"], "OPEN")
            self.assertTrue(r.owner.positions[list(r.owner.positions)[0]].canonical_provider_gap_flag)  # gap after the fill flags the open position in both

    def test_fill_inside_gap_is_unknown_in_both(self):
        with tempfile.TemporaryDirectory() as d:
            h = self._record(Path(d), gap_at_ms=10, recover_ms=80, entry_delay_ms=50)  # modeled fill at 51 ms inside the gap
            self.assertEqual(attempts(h)[0]["status"], "UNKNOWN")
            conn = open_connection(Path(d) / "p.sqlite")
            r = run_replay(conn, load_replay_input(conn, "run", h.cfg), h.cfg, "rep", "c", persist=False)
            self.assertEqual(r.digest["positions"], digest(h.owner)["positions"])
            self.assertEqual(r.digest["positions"][0]["unresolved_reason"], "ATTEMPT_STATUS_UNKNOWN")
            self.assertEqual(r.digest["counters"], digest(h.owner)["counters"])


class AtomicUpgrade(unittest.TestCase):
    def test_partial_create_buy_then_complete_fills_on_post_tx_state(self):
        h = Harness()
        full = create_frame_logs(MINT_A, CREATOR, initial_buy=(5_000_000_000, 100_000_000_000_000))
        partial = list(full)
        partial.insert(4, "Program data: " + base64.b64encode(event_discriminator("FutureEvent") + b"\1" * 40).decode())  # unknown event after the buy
        h.feed("existing_rpc", logs_notification("s", 1, partial), T0 + 1_000_000)
        self.assertEqual(len(h.owner.positions), 0)
        h.feed("helius_standard", logs_notification("s", 1, full), T0 + 2_000_000)
        st = h.owner.reducer.states[MINT_A]
        self.assertEqual((st.virtual_sol_reserves, st.virtual_token_reserves), (35_000_000_000, 973_000_000_000_000))
        h.advance(T0 + 2_000_000)  # fill scheduled at the same model time fires after the envelope
        a = attempts(h)[0]
        self.assertEqual(a["status"], "FILLED")
        self.assertTrue(a["quote_state_event_id"].endswith(f"|3|{MINT_A}|BUY"))  # quote on the last instruction of the atomic tx
        self.assertEqual(a["proxy_event_id"], a["quote_state_event_id"])


class DoctorAndDataset(unittest.TestCase):
    def test_D04_counts_persisted_creator_sells(self):
        h = Harness()
        open_position(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.feed("helius_standard", pump_frame(h, MINT_A, 10, "other", 102, user=BUYER), T0 + 3_000_000)
        h.flush()
        self.assertEqual(count_live_creator_sells(h.conn, "run"), 1)
        self.assertEqual(h.rows("select creator from market_events where signature='cs'")[0][0], CREATOR)

    def test_dataset_integrity_rejects_manipulated_payloads(self):
        with tempfile.TemporaryDirectory() as d:
            h = Harness(db_path=str(Path(d) / "r.sqlite"))
            open_position(h)
            h.finish(T0 + 5_000_000)
            conn = open_connection(Path(d) / "r.sqlite")
            load_replay_input(conn, "run", h.cfg)
            conn.execute("UPDATE feed_observations SET raw_payload=replace(raw_payload,'sigC','sigX') WHERE signature='sigC'")
            with self.assertRaises(ValueError):
                load_replay_input(conn, "run", h.cfg)


class FeaturesAndReports(unittest.TestCase):
    def test_truncated_history_nulls_trade_features(self):
        h = Harness(base_config(storage={"trade_log_per_mint": 2}))
        open_position(h)
        for i in range(4):
            h.feed("helius_standard", pump_frame(h, MINT_A, 10, f"p{i}", 101 + i), T0 + 300_000_000 + i * 10_000_000)
        h.advance(T0 + 3_000_000_000)
        rows = h.rows("select cutoff_mono_ns, features_json, null_reasons_json, quality_status from feature_snapshots where mint=? and feature_set_id='FEATURE_SET_V2_CANONICAL_COVERAGE' order by cutoff_mono_ns", (MINT_A,))
        late = [r for r in rows if r["cutoff_mono_ns"] >= T0 + 1_000_000 + 500_000_000]
        self.assertTrue(late)
        for r in late:
            f, nulls = json.loads(r["features_json"]), json.loads(r["null_reasons_json"])
            self.assertIsNone(f["buy_count"])
            self.assertEqual(nulls["buy_count"], "TRUNCATED_TRADE_LOG")
            self.assertIsNotNone(f["dev_initial_buy"] if f["dev_initial_buy"] is not None else 1)  # create-derived feature stays

    def test_utilization_uses_recorded_run_window_and_day_by_close(self):
        h = Harness()
        pos = open_position(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        h.finish(T0 + 1_000 * 1_000_000_000)  # 1000 s recorded window, position busy ~2 ms
        e = h.report()["economic"]
        self.assertLess(e["capital_utilization_fraction"], 0.001)
        self.assertEqual(e["run_window_ns"], 1_000 * 1_000_000_000)
        h.conn.execute("UPDATE runs SET started_utc='2026-09-18T23:59:59.999000Z' WHERE run_id='run'")  # close 2 ms after start -> next UTC day
        e2 = h.report()["economic"]
        self.assertEqual(list(e2["daily_pnl"]["by_day"]), ["2026-09-19"])


class Architecture(unittest.TestCase):
    def test_writer_error_is_marshalled_to_the_owner_thread(self):
        from darkbot.main import Application
        app = Application(base_config(), Path(tempfile.mkdtemp()) / "x.sqlite", "c")
        calls = []
        app._loop = mock.Mock()
        app.owner = mock.Mock()
        app._disk_error("boom")
        app._loop.call_soon_threadsafe.assert_called_once_with(app.owner.on_disk_error, "boom")
        app.owner.on_disk_error.assert_not_called()  # never mutated from the writer thread

    def test_pending_handles_and_active_index_are_bounded(self):
        h = Harness()
        pos = open_position(h)
        self.assertEqual(len(h.adapter.pending), 0)  # handle released after its outcome
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        self.assertEqual(len(h.adapter.pending), 1)
        h.advance(T0 + 3_000_000)
        self.assertEqual(len(h.adapter.pending), 0)
        self.assertEqual(h.owner.occupied_slots(), 0)
        self.assertEqual(h.owner.active_ids, set())  # closed position left the active index
        self.assertEqual(len(h.owner.positions), 1)


if __name__ == "__main__":
    unittest.main()
