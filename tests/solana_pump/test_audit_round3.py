"""Audit round 3 counterexamples (9 x P1 + residuals): reserve freshness, conflicts/reorgs on pending fills, decode gaps, foreign trade tails,
research freeze, unknown fee basis at fill, stress bounds, campaign-wide re-entry, writer FK give-up, DB upgrade rows, fill cost columns,
dataset hash contents, V1 feature history/truncation, utilization bound, eviction of closed positions."""

from __future__ import annotations

import base64
import json
import sqlite3
import tempfile
import threading
import unittest
from decimal import Decimal
from pathlib import Path

from darkbot.solana_pump.backtest import run_research
from darkbot.solana_pump.config import build_runtime_config
from darkbot.solana_pump.domain import AttemptStatus, CoverageState, ExitReason, LedgerRecord, PositionState, QuoteScopeStatus, Side, utc_iso
from darkbot.solana_pump.execution import ExecutionPolicy, QuoteEngine, evaluate_fill
from darkbot.solana_pump.market import b58encode, encode_fields, event_discriminator
from darkbot.solana_pump.replay import load_replay_input
from darkbot.solana_pump.storage import Recorder, SqliteWriter, open_connection, prior_campaign_mints
from tests.helpers import BUYER, CREATOR, MINT_A, MINT_B, PROGRAM, ROOT, T0, Harness, attempts, base_config, create_frame_logs, create_payload, creator_sell_frame, fee_basis, ledger_kinds, logs_notification, open_position, pump_frame, trade_log_line


class P1Freshness(unittest.TestCase):
    def test_P1_1_reserves_older_than_quote_max_age_never_fill(self):
        h = Harness()
        pos = open_position(h)
        h.advance(T0 + 1_000_000 + 590_000_000_000)  # timeout with 590 s old reserves
        self.assertEqual(pos.state, PositionState.OPEN)
        self.assertEqual([a for a in attempts(h) if a["side"] == "SELL"], [])
        self.assertIn("EXIT_LATCHED_PROXY_STATE_STALE", [r["reason"] for r in h.rows("select reason from decisions")])
        # a fill whose proxy is older than the limit at the modeled fill time is UNKNOWN, never FILLED
        h2 = Harness(base_config(latency_model={"exit_delay_ms": 100}))
        pos2 = open_position(h2)
        h2.feed("helius_standard", creator_sell_frame(h2, MINT_A, "cs", 101), T0 + 2_000_000)  # trigger + submit at 2 ms, fill at 102 ms
        h2.feed("helius_standard", pump_frame(h2, MINT_A, 10, "p", 102), T0 + 2_100_000)  # last state at 2.1 ms -> age at fill 99.9 ms: OK
        h2.advance(T0 + 200_000_000)
        self.assertEqual(pos2.state, PositionState.CLOSED)
        h3 = Harness(base_config(latency_model={"exit_delay_ms": 300}))
        pos3 = open_position(h3)
        h3.feed("helius_standard", creator_sell_frame(h3, MINT_A, "cs", 101), T0 + 2_000_000)  # fill at 302 ms, last state 300 ms old
        h3.advance(T0 + 400_000_000)
        sell = [a for a in attempts(h3) if a["side"] == "SELL"][0]
        self.assertEqual(sell["status"], AttemptStatus.UNKNOWN.value)
        self.assertEqual(sell["failure_reason"], "PROXY_STATE_STALE")
        self.assertEqual(pos3.state, PositionState.UNRESOLVED)


class P1ConflictsGaps(unittest.TestCase):
    def test_P1_2_conflict_before_modeled_fill_makes_pending_buy_unknown(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("existing_rpc", logs_notification("sigC", 100, create_frame_logs(MINT_A, CREATOR, vsol=31_000_000_000)), T0 + 10_000_000)  # conflicting copy
        h.advance(T0 + 60_000_000)
        pos = h.position()
        a = attempts(h)[0]
        self.assertEqual(a["status"], AttemptStatus.UNKNOWN.value)
        self.assertEqual(a["failure_reason"], "QUOTE_CONFLICT")
        self.assertIsNone(a["fees_charged_lamports"])
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(ledger_kinds(h), [])

    def test_P1_3_unknown_pump_event_opens_decode_gap(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        st = h.owner.reducer.states[MINT_A]
        logs = [f"Program {PROGRAM} invoke [1]", "Program data: " + base64.b64encode(event_discriminator("FutureEvent") + b"\2" * 40).decode(),
                trade_log_line(MINT_A, 10, 10, True, BUYER, st.virtual_sol_reserves + 10, st.virtual_token_reserves - 10, st.real_token_reserves - 10), f"Program {PROGRAM} success"]
        h.feed("helius_standard", logs_notification("mystery", 101, logs), T0 + 10_000_000)  # PARTIAL tx touching the mint before the fill
        self.assertIsNotNone(st.decode_gap_since)
        h.advance(T0 + 60_000_000)
        a = attempts(h)[0]
        self.assertEqual(a["status"], AttemptStatus.UNKNOWN.value)
        self.assertEqual(a["failure_reason"], "DECODE_GAP")
        self.assertEqual(h.position().state, PositionState.UNRESOLVED)
        self.assertEqual(h.rows("select count(*) from quality_events where kind='DECODE_GAP'")[0][0], 1)
        h.feed("helius_standard", pump_frame(h, MINT_A, 10, "p", 102), T0 + 70_000_000)  # next complete state closes the gap, history stays lost
        self.assertIsNone(st.decode_gap_since)
        self.assertEqual(st.coverage_state, CoverageState.RECOVERED_STATE_ONLY)
        # unattributable undecodable frame inside a fill window -> UNKNOWN as well
        h2 = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h2.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h2.feed("helius_standard", "{not json", T0 + 20_000_000)
        h2.advance(T0 + 60_000_000)
        self.assertEqual(attempts(h2)[0]["failure_reason"], "UNDECODABLE_FRAME_IN_WINDOW")

    def test_P1_4_foreign_quote_trade_tail_never_overwrites_sol_state(self):
        cfg = base_config()
        raw = json.loads(json.dumps(cfg.raw))
        raw["protocol"]["events"]["TradeEvent"]["tail_layouts"] = [{"layout_id": "TRADE_V2_QUOTE", "tail_fields": [["quote_mint", "pubkey"]], "quote_mint_field": "quote_mint", "verification_status": "VERIFIED_TEST"}]
        cfg2 = build_runtime_config(cfg.path, raw)
        h = Harness(cfg2)
        pos = open_position(h)
        st = h.owner.reducer.states[MINT_A]
        vsol_before = st.virtual_sol_reserves
        tail = encode_fields([("quote_mint", b58encode(b"USDC".ljust(32, b"\0")))], [["quote_mint", "pubkey"]])
        line = trade_log_line(MINT_A, 5_000_000_000, 100_000_000_000_000, False, CREATOR, vsol_before - 5_000_000_000, st.virtual_token_reserves + 100_000_000_000_000, st.real_token_reserves + 100_000_000_000_000, tail=tail)
        h.feed("helius_standard", logs_notification("foreign", 101, [f"Program {PROGRAM} invoke [1]", line, f"Program {PROGRAM} success"]), T0 + 2_000_000)
        self.assertEqual(st.virtual_sol_reserves, vsol_before)  # foreign amounts never entered SOL state
        self.assertEqual(st.quote_scope_status, QuoteScopeStatus.UNSUPPORTED_QUOTE)
        h.advance(T0 + 3_000_000)
        self.assertEqual([a for a in attempts(h) if a["side"] == "SELL"], [])
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(pos.unresolved_reason, "QUOTE_CONFLICT_ON_OPEN_POSITION")
        self.assertEqual(h.rows("select virtual_sol_reserves from market_events where signature='foreign'")[0][0], None)
        self.assertEqual(h.rows("select count(*) from quality_events where kind='EVENT_CONFLICT'")[0][0], 1)


class P1CostsStressResearch(unittest.TestCase):
    def test_P1_6_unknown_fee_basis_at_fill_is_unknown_not_failed(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        bad = fee_basis(h.cfg, T0 + 20_000_000, status="RPC_ERROR", run_id="bad")  # fee rule becomes unknown before the modeled fill
        for x in (bad.protocol, bad.network, bad.rent, bad.priority_policy):
            h.recorder.try_append(("fee_snapshot", (x, "run")))
        h.fees = bad
        h.auto_refresh = False
        h.owner.on_fee_snapshots([bad.protocol, bad.network, bad.rent, bad.priority_policy])
        h.advance(T0 + 60_000_000)
        a = attempts(h)[0]
        self.assertEqual(a["status"], AttemptStatus.UNKNOWN.value)
        self.assertEqual(a["failure_reason"], "COSTS_UNKNOWN_AT_MODELED_FILL")
        self.assertIsNone(a["fees_charged_lamports"])
        self.assertEqual(ledger_kinds(h), [])
        self.assertEqual(h.position().state, PositionState.UNRESOLVED)

    def test_P1_7_stressed_debit_beyond_max_input_is_rejected(self):
        h = Harness()
        pos = open_position(h)
        st = h.owner.reducer.states[MINT_A]
        curve = h.owner._curve(st)
        expected = h.owner.quote_by_attempt[attempts(h)[0]["attempt_id"]]
        strict = QuoteEngine(ExecutionPolicy.from_config(h.cfg), Decimal("2.0"), 400)  # +400 bps adverse > 300 bps max_sol_cost bound
        ev = evaluate_fill(strict, expected, curve, h.fees.basis(), curve.receive_mono_ns, "x", CoverageState.OBSERVED, 0)
        self.assertEqual(ev.status, AttemptStatus.SLIPPAGE_REJECTED)
        self.assertEqual(ev.failure_reason, "STRESS_ADVERSE_PRICE_EXCEEDS_MAX_INPUT")
        self.assertEqual(ev.fees_charged_lamports, 2 * expected.network_fee_total_lamports)
        mild = QuoteEngine(ExecutionPolicy.from_config(h.cfg), Decimal("2.0"), 100)
        ev2 = evaluate_fill(mild, expected, curve, h.fees.basis(), curve.receive_mono_ns, "x", CoverageState.OBSERVED, 0)
        self.assertEqual(ev2.status, AttemptStatus.FILLED)
        self.assertLessEqual(ev2.actual.swap_wallet_debit_lamports, expected.max_input_base_units)

    def test_P1_5_test_split_without_frozen_candidate_never_qualifies(self):
        with tempfile.TemporaryDirectory() as d:
            h = Harness(db_path=str(Path(d) / "r.sqlite"))
            h.conn.execute("UPDATE runs SET mode='PAPER'")
            for i in range(6):
                m = b58encode(f"z{i}".encode().ljust(32, b"\0"))
                h.feed("helius_standard", create_payload(m, sig=f"s{i}", slot=100 + i), T0 + i * 4_000_000_000_000)
                h.feed("helius_standard", creator_sell_frame(h, m, f"c{i}", 200 + i), T0 + i * 4_000_000_000_000 + 5_000_000)
            h.finish(T0 + 6 * 4_000_000_000_000)
            rc = json.loads((ROOT / "config" / "research.json").read_text())
            rc["test_split_unlocked"] = True
            p = Path(d) / "research_unlocked.json"
            p.write_text(json.dumps(rc))
            conn = open_connection(Path(d) / "r.sqlite")
            with self.assertRaises(ValueError):  # unlocking without a prior freeze is refused before any test run is persisted
                run_research(conn, "camp", h.cfg, p, Path(d) / "out", persist=True, max_variants=2)
            self.assertEqual(conn.execute("SELECT COUNT(*) FROM runs WHERE mode='RESEARCH_REPLAY'").fetchone()[0], 0)
            # locked stage first (no positive candidate on synthetic data -> nothing frozen), then a manual freeze -> forward stage runs and creates test runs
            rep0 = run_research(conn, "camp", h.cfg, ROOT / "config" / "research.json", Path(d) / "out0", persist=True, max_variants=2)
            self.assertEqual(rep0["selection_for_limit_1"]["stage"], "freeze")
            self.assertFalse(rep0["selection_for_limit_1"]["would_freeze"])
            from darkbot.solana_pump.config import config_hash
            from darkbot.solana_pump.storage import upsert_config_snapshot
            freeze_key = config_hash({"campaign": "camp", "kind": "POLICY_FREEZE"})
            upsert_config_snapshot(conn, "POLICY_FREEZE", freeze_key, {"campaign": "camp", "dataset": rep0["dataset_hash"], "selected_target": "1.1", "runtime_config_hash": h.cfg.config_hash, "research_policy_hash": config_hash({k: v for k, v in rc.items() if k not in ("test_split_unlocked", "forward_test", "policy_freeze_note")}), "stage": "VALIDATION"}, "test", utc_iso(0))
            rep = run_research(conn, "camp", h.cfg, p, Path(d) / "out", persist=True, max_variants=2)  # forward stage: distinct run ids, no collision
            test_rows = [r for r in rep["results"] if r.get("split") == "test" and r.get("variant_id") != "ALL"]
            self.assertTrue(test_rows)
            self.assertEqual(rep["selection_for_limit_1"]["frozen_target"], "1.1")
            self.assertTrue(conn.execute("SELECT COUNT(*) FROM runs WHERE mode='RESEARCH_REPLAY' AND run_id LIKE '%_holdout_test_%'").fetchone()[0] > 0)
            self.assertFalse(any(r.get("status") == "POSITIVE_UNDER_MODEL" for r in rep["results"]))  # no stress siblings/insufficient data -> never positive


class P1RestartWriter(unittest.TestCase):
    def test_P1_8_same_mint_never_bought_twice_across_restarts(self):
        with tempfile.TemporaryDirectory() as d:
            db = str(Path(d) / "c.sqlite")
            h = Harness(db_path=db, run_id="run1")
            open_position(h)
            h.finish(T0 + 5_000_000)
            conn = open_connection(Path(d) / "c.sqlite")
            self.assertEqual(prior_campaign_mints(conn, "camp"), {MINT_A})
            h2 = Harness(db_path=db, run_id="run2")  # restart of the same campaign
            h2.feed("helius_standard", create_payload(sig="again", slot=300), T0 + 1_000_000)
            self.assertEqual(len(h2.owner.positions), 0)
            self.assertEqual(h2.rows("select reason from decisions where run_id='run2'")[0]["reason"], "DUPLICATE_ENTRY")
            h3 = Harness(db_path=db, run_id="run3", campaign="other")  # a different campaign is unaffected
            h3.feed("helius_standard", create_payload(sig="again2", slot=301), T0 + 1_000_000)
            self.assertEqual(len(h3.owner.positions), 1)

    def test_P1_9_writer_gives_up_on_permanent_fk_miss_with_quarantine(self):
        with tempfile.TemporaryDirectory() as d:
            h = Harness(db_path=str(Path(d) / "w.sqlite"))
            errors: list[str] = []
            q = Path(d) / "quarantine.jsonl"
            w = SqliteWriter(h.conn, h.recorder, 2, 5, errors.append, locked_retries=3, fk_merge_max_batches=3, quarantine_path=q)
            h.recorder.try_append(LedgerRecord("orphan:x", "run", "no_such_position", "no_such_attempt", 1, "t", "X", 0, 0, 0, 0, "test"))
            w.start()
            w.stop(timeout_s=5)
            self.assertFalse(w._thread.is_alive())  # no endless loop
            self.assertEqual(w.quarantined_records, 1)
            self.assertTrue(errors and "FK_TARGET_NEVER_ARRIVED" in errors[0])
            self.assertTrue(q.exists() and "orphan:x" in q.read_text())
            self.assertTrue(w.disk_error)


class Residuals(unittest.TestCase):
    def test_S03_db_row_reflects_upgrade(self):
        h = Harness()
        logs = create_frame_logs(MINT_A, CREATOR)
        logs.insert(3, "Program data: " + base64.b64encode(event_discriminator("FutureEvent") + b"\1" * 40).decode())
        h.feed("existing_rpc", logs_notification("s", 1, logs), T0 + 1_000_000)
        h.feed("helius_standard", create_payload(sig="s", slot=1), T0 + 2_000_000)
        row = h.rows("select actionable_mono_ns, winner_feed_id, quality_status from market_events where event_type='CREATE'")[0]
        self.assertIsNotNone(row["actionable_mono_ns"])
        self.assertEqual(row["winner_feed_id"], "helius_standard")
        self.assertEqual(row["quality_status"], "VALID_UPGRADED_FROM_PARTIAL")

    def test_S07_S14_attempt_costs_match_fill_quote_and_position(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        newer = fee_basis(h.cfg, T0 + 20_000_000, cu_price=30_000, run_id="run2")  # priority 6000 -> network 11000 at the fill
        for x in (newer.protocol, newer.network, newer.rent, newer.priority_policy):
            h.recorder.try_append(("fee_snapshot", (x, "run")))
        h.fees = newer
        h.owner.on_fee_snapshots([newer.protocol, newer.network, newer.rent, newer.priority_policy])
        h.feed("helius_standard", pump_frame(h, MINT_A, 10, "hb", 101), T0 + 30_000_000)
        h.advance(T0 + 60_000_000)
        a = attempts(h)[0]
        q = h.rows("select network_fee_total_lamports from quote_snapshots where quote_snapshot_id=?", (a["fill_quote_snapshot_id"],))[0]
        self.assertEqual(a["network_fee_total_lamports"], 11_000)
        self.assertEqual(a["network_fee_total_lamports"], q["network_fee_total_lamports"])
        self.assertEqual(a["priority_fee_lamports"], 6_000)
        self.assertEqual(h.position().network_fee_lamports, 11_000)
        self.assertEqual(a["fees_charged_lamports"], a["amount_in_base_units"] + 11_000 + a["rent_locked_lamports"])

    def test_S06_hash_changes_with_fee_contents_and_protocol_pinning(self):
        with tempfile.TemporaryDirectory() as d:
            h = Harness(db_path=str(Path(d) / "r.sqlite"))
            open_position(h)
            h.finish(T0 + 5_000_000)
            conn = open_connection(Path(d) / "r.sqlite")
            base = load_replay_input(conn, "run", h.cfg).dataset_hash
            conn.execute("UPDATE fee_snapshots SET normalized_fee_components=replace(normalized_fee_components,'100','101') WHERE fee_snapshot_id='run_fee_protocol'")
            h1 = load_replay_input(conn, "run", h.cfg).dataset_hash
            other = h.cfg.with_overrides(protocol={"decoder_version": "PUMP_LOG_DECODER_V2"})
            h2 = load_replay_input(conn, "run", other).dataset_hash
            self.assertEqual(len({base, h1, h2}), 3)

    def test_S11_v1_history_and_truncation_flag(self):
        h = Harness(base_config(storage={"trade_log_per_mint": 2}))
        pos = open_position(h)
        h.owner.on_feed_health("existing_rpc", False, T0 + 100_000_000, "closed")  # first eligible feed (V1 provider) down inside the window
        h.owner.on_feed_health("existing_rpc", True, T0 + 200_000_000, "c2")
        for i in range(4):
            h.feed("helius_standard", pump_frame(h, MINT_A, 10, f"p{i}", 101 + i), T0 + 300_000_000 + i * 10_000_000)
        h.advance(T0 + 3_000_000_000)
        rows = h.rows("select feature_set_id, cutoff_mono_ns, features_json, quality_status from feature_snapshots where mint=? order by feature_set_id, cutoff_mono_ns", (MINT_A,))
        v1 = [r for r in rows if r["feature_set_id"] == "FEATURE_CONTRACT_V1_LEGACY"]
        flags = [json.loads(r["features_json"])["provider_gap_flag"] for r in v1]
        self.assertEqual(flags, [False, True, True, True, True])  # 0 ms cutoff before the outage; later cutoffs see it (from recorded history, not current state)
        self.assertTrue(all(r["quality_status"] == "TRUNCATED_TRADE_LOG" for r in rows if r["cutoff_mono_ns"] >= T0 + 1_000_000 + 500_000_000))

    def test_S13_utilization_bounded_and_daily_by_close_day(self):
        h = Harness()
        pos = open_position(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        h.feed("helius_standard", pump_frame(h, MINT_A, 10, "later", 102), T0 + 4_000_000)
        e = h.report()["economic"]
        self.assertLessEqual(e["capital_utilization_fraction"], 1.0)
        self.assertGreater(e["capital_utilization_fraction"], 0)
        self.assertEqual(list(e["daily_pnl"]["by_day"].values()), [pos.net_pnl_lamports])
        self.assertIn("close", e["daily_pnl"]["note"])

    def test_S16_closed_positions_do_not_block_eviction(self):
        h = Harness()
        pos = open_position(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertEqual(h.owner.maybe_evict(T0 + 3_000_000 + 700_000_000_000 + 900_000_000_000), 1)
        self.assertNotIn(MINT_A, h.owner.reducer.states)
        self.assertIn(pos.position_id, h.owner.positions)  # bookkeeping survives; only the market tracker is freed
        self.assertIn(MINT_A, h.owner.submitted_mints)


if __name__ == "__main__":
    unittest.main()
