"""T16–T35: fees/rent/snapshots/double counting, fixtures with hand-computed numbers, replay determinism, look-ahead, research, broadcast boundary, restart, censoring, protocol variants, sizing, slot race, schema."""

from __future__ import annotations

import json
import os
import sqlite3
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path

from darkbot.backtest import chronological_split, run_research
from darkbot.capture import pumpportal_frame
from darkbot.config import ConfigError, load_json_strict
from darkbot.domain import AttemptStatus, CostEvidence, CoverageState, CurveState, ExitReason, FeeSnapshot, PositionState, QuoteScopeStatus, QuoteSnapshot, Side, VenueState, utc_iso
from darkbot.execution import LiveBroadcastForbidden, QuoteEngine, assert_paper_boundary, ceil_bps, paid_trade_api_call, send_transaction
from darkbot.fees import FeeResolver, build_fee_probe_message, compact_u16
from darkbot.market import PumpDecoder, b58encode, encode_fields, event_discriminator
from darkbot.paper import PaperExecutionAdapter
from darkbot.replay import digest, load_replay_input, run_replay
from darkbot.reporting import comparable
from darkbot.storage import REQUIRED_TABLES, apply_migrations, integrity_check, mark_interrupted_runs, open_connection
from tests.helpers import BUYER, CREATOR, MINT_A, MINT_B, ROOT, R_TOK0, SUPPLY, T0, V_SOL0, V_TOK0, Harness, attempts, base_config, create_frame_logs, create_payload, creator_sell_frame, dump_frame, fee_basis, ledger_kinds, logs_notification, open_position, pump_frame


def expected_entry(vsol, vtok, rtok, budget=98_370_000, pbps=100, cbps=30, network=7000, rent=2_039_280):
    tokens = budget * vtok // (vsol + budget)
    tokens = min(tokens, rtok)
    curve_input = -(-(vsol * tokens) // (vtok - tokens))
    proto, creator = -(-(curve_input * pbps) // 10_000), -(-(curve_input * cbps) // 10_000)
    swap = curve_input + proto + creator
    return dict(tokens=tokens, curve_input=curve_input, proto=proto, creator=creator, swap=swap, economic=swap + network, cash_debit=swap + network + rent)


def expected_sell(vsol, vtok, tokens, pbps=100, cbps=30, network=7000, rent=2_039_280):
    gross = tokens * vsol // (vtok + tokens)
    proto, creator = -(-(gross * pbps) // 10_000), -(-(gross * cbps) // 10_000)
    credit = gross - proto - creator
    return dict(gross=gross, proto=proto, creator=creator, credit=credit, cash_credit=credit - network + rent)


class T16T18Costs(unittest.TestCase):
    def test_T16_stale_fees_diagnostic_only(self):
        cfg = base_config()
        stale = fee_basis(cfg, T0 - 10 * 60 * 1_000_000_000)  # 10 minutes old -> protocol/network stale
        h = Harness(cfg, fees=stale)
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        self.assertEqual(len(h.owner.positions), 0)
        self.assertEqual(h.rows("select reason from decisions")[0]["reason"], "UNQUALIFIED_COSTS")
        self.assertEqual(h.rows("select count(*) from feed_observations")[0][0], 1)  # measurement still recorded
        self.assertEqual(h.report()["qualification_status"], "UNQUALIFIED_COSTS")

    def test_T17_hand_computed_fixture(self):
        h = Harness()
        pos = open_position(h)
        e = expected_entry(V_SOL0, V_TOK0, R_TOK0)
        self.assertEqual(pos.token_balance_base_units, e["tokens"])
        self.assertEqual(pos.entry_economic_cost_lamports, e["economic"])
        self.assertEqual(pos.entry_cash_debit_lamports, e["cash_debit"])
        self.assertLessEqual(e["curve_input"], 98_370_000)
        # our paper buy is counterfactual: the curve only moves with observed trades. Reserves must be fresh (quote_max_age_ms) at the timeout:
        # one small observed buy 100 ms before the timeout supplies the valid basis; the sell is computed on that post-trade state.
        hb_sol, hb_tok = 1_000_000, 1_000_000 * V_TOK0 // (V_SOL0 + 1_000_000)
        h.feed("helius_standard", pump_frame(h, MINT_A, hb_sol, "hb", 101), T0 + 1_000_000 + 590_000_000_000 - 100_000_000)
        h.advance(T0 + 1_000_000 + 590_000_000_000)
        s = expected_sell(V_SOL0 + hb_sol, V_TOK0 - hb_tok, e["tokens"])
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertEqual(pos.exit_reason, ExitReason.TIMEOUT)
        self.assertEqual(pos.net_pnl_lamports, s["cash_credit"] - e["cash_debit"])
        self.assertEqual(pos.net_pnl_lamports, s["credit"] - 7000 - e["economic"])
        self.assertEqual(h.owner.portfolio.cash_lamports, 1_000_000_000 + pos.net_pnl_lamports)
        self.assertTrue(h.owner.cash_reconciliation()["reconciled"])
        rep = h.report()["economic"]
        self.assertEqual(rep["realized_net_pnl_lamports"], pos.net_pnl_lamports)
        self.assertEqual(rep["expectancy_per_intent_lamports"], pos.net_pnl_lamports)
        # failed intent expectancy: a rejected entry counts with its fee
        h2 = Harness(base_config(latency_model={"entry_delay_ms": 20}))
        h2.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h2.feed("helius_standard", pump_frame(h2, MINT_A, 20_000_000_000, "pump", 101), T0 + 5_000_000)
        h2.advance(T0 + 30_000_000)
        self.assertEqual(h2.report()["economic"]["expectancy_per_intent_lamports"], -7000)

    def test_T18_missing_feed_and_tie(self):
        h = Harness()
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("existing_rpc", create_payload(), T0 + 1_000_000)  # identical mono ns -> tie
        rep = h.report()["latency"]["feeds"]
        pf = rep["per_feed"]
        self.assertEqual((pf["helius_standard"]["ties"], pf["existing_rpc"]["ties"]), (1, 1))
        self.assertEqual((pf["helius_standard"]["hint_wins"], pf["existing_rpc"]["hint_wins"]), (0, 0))
        self.assertEqual(pf["pumpportal"]["missing"], 1)
        self.assertIsNone(pf["pumpportal"]["relative_arrival_ns"]["p50"])
        self.assertEqual(rep["pairwise_common_creates"]["existing_rpc__vs__helius_standard"]["ties"], 1)


class T19T21Replay(unittest.TestCase):
    def _scenario_db(self, tmp: Path) -> tuple[Harness, str]:
        h = Harness(db_path=str(tmp / "s.sqlite"))
        pos = open_position(h)
        h.feed("helius_standard", pump_frame(h, MINT_A, 3_000_000_000, "p1", 101), T0 + 5_000_000)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 102), T0 + 9_000_000)
        h.feed("helius_standard", create_payload(MINT_B, sig="sigB", slot=200), T0 + 50_000_000)
        h.feed("helius_standard", dump_frame(h, MINT_B, 100_000_000_000_000, "d", 201), T0 + 60_000_000)
        for i in range(2, 6):  # further launches spaced beyond the split embargo
            m = b58encode(f"m{i}".encode().ljust(32, b"\0"))
            h.feed("helius_standard", create_payload(m, sig=f"sig{i}", slot=300 + i), T0 + i * 700_000_000_000)
            h.feed("helius_standard", creator_sell_frame(h, m, f"cs{i}", 400 + i), T0 + i * 700_000_000_000 + 5_000_000)
        h.finish(T0 + 6 * 700_000_000_000)
        h.conn.execute("UPDATE runs SET status='COMPLETED', ended_utc=?, end_balance_lamports=? WHERE run_id='run'", (utc_iso(0), h.owner.portfolio.cash_lamports + h.owner.portfolio.reserved_lamports))
        return h, str(tmp / "s.sqlite")

    def test_T19_replay_twice_identical(self):
        with tempfile.TemporaryDirectory() as d:
            h, db = self._scenario_db(Path(d))
            conn = open_connection(Path(db))
            rin = load_replay_input(conn, "run", h.cfg)
            r1 = run_replay(conn, rin, h.cfg, "rep1", "camp", persist=True)
            r2 = run_replay(conn, rin, h.cfg, "rep2", "camp", persist=True)
            self.assertEqual(r1.digest, r2.digest)
            self.assertEqual(r1.digest["positions"], digest(h.owner)["positions"])
            from darkbot.reporting import Reporter
            a, b = Reporter(conn).build("rep1"), Reporter(conn).build("rep2")
            for key in ("economic", "quality", "qualification_status"):
                self.assertEqual(json.dumps(a[key], sort_keys=True, default=str).replace("rep1", "X"), json.dumps(b[key], sort_keys=True, default=str).replace("rep2", "X"))
            self.assertNotIn("created_utc", comparable(a))
            self.assertEqual(conn.execute("PRAGMA foreign_key_check").fetchall(), [])

    def test_T20_future_events_do_not_change_earlier_decisions(self):
        with tempfile.TemporaryDirectory() as d:
            h, db = self._scenario_db(Path(d))
            conn = open_connection(Path(db))
            rin = load_replay_input(conn, "run", h.cfg)
            early = run_replay(conn, rin, h.cfg, "early", "camp", persist=False, end_mono_ns=T0 + 10_000_000)
            full = run_replay(conn, rin, h.cfg, "full", "camp", persist=False)
            self.assertEqual(early.digest["positions"][0], full.digest["positions"][0])
            self.assertEqual(early.digest["attempts"][:2], full.digest["attempts"][:2])
            self.assertLess(len(early.digest["positions"]), len(full.digest["positions"]))

    def test_T21_paired_and_portfolio_separate(self):
        with tempfile.TemporaryDirectory() as d:
            h, db = self._scenario_db(Path(d))
            conn = open_connection(Path(db))
            out = Path(d) / "research"
            rep = run_research(conn, "camp", h.cfg, ROOT / "config" / "research.json", out, source_run_ids=["run"], persist=False, max_variants=3)
            kinds = {r.get("portfolio_limit") for r in rep["results"] if r.get("variant_id") != "ALL"}
            self.assertEqual(kinds, {"paired", 1, 10})
            self.assertTrue(any(r["split"] == "test" and r["status"] == "LOCKED_UNTIL_POLICY_FREEZE" for r in rep["results"]))
            self.assertTrue((out / "target_comparison.csv").exists())
            splits = chronological_split([(f"m{i}", T0 + i * 10**9) for i in range(10)], [Decimal("0.6"), Decimal("0.2"), Decimal("0.2")], 10**9)
            self.assertEqual(sum(len(v) for v in splits.values()), 8)  # two launches purged by embargo at the boundaries


class T22T24Boundary(unittest.TestCase):
    def test_T22_paper_broadcast_boundary(self):
        for fn in (send_transaction, paid_trade_api_call):
            with self.assertRaises(LiveBroadcastForbidden):
                fn("tx")
        with self.assertRaises(LiveBroadcastForbidden):
            assert_paper_boundary("LIVE", False)
        with self.assertRaises(LiveBroadcastForbidden):
            assert_paper_boundary("PAPER", True)
        with self.assertRaises(LiveBroadcastForbidden):
            PaperExecutionAdapter(None, None, "LIVE", True, 0, 0)
        with self.assertRaises(LiveBroadcastForbidden):
            pumpportal_frame("subscribeTokenTrade")
        with self.assertRaises(ConfigError):
            _cfg_with({"live_broadcast_enabled": True})
        h = Harness()
        with self.assertRaises(LiveBroadcastForbidden):
            h.adapter.send_transaction(b"x")

    def test_T23_process_restart_new_epoch(self):
        with tempfile.TemporaryDirectory() as d:
            db = Path(d) / "r.sqlite"
            h = Harness(db_path=str(db))
            pos = open_position(h)
            h.flush()
            conn = open_connection(db)
            self.assertEqual(mark_interrupted_runs(conn, "camp", utc_iso(0)), 1)
            self.assertEqual(conn.execute("SELECT status FROM runs WHERE run_id='run'").fetchone()[0], "INTERRUPTED")
            self.assertEqual(conn.execute("SELECT state FROM positions").fetchone()[0], "OPEN")  # never closed by restart
            h2 = Harness(db_path=str(db), run_id="run2")
            self.assertNotEqual(h.clock.epoch_id, h2.clock.epoch_id) if h.clock.epoch_id != "epoch_test" else None
            from darkbot.clock import LiveClock
            self.assertNotEqual(LiveClock().epoch_id, LiveClock().epoch_id)
            rep = h2.report()
            self.assertEqual(rep["economic"]["entry_intents"], 0)  # runs are separate epochs, no cross-run subtraction

    def test_T24_gap_during_position_censoring_visible(self):
        h = Harness()
        pos = open_position(h)
        h.owner.on_feed_health("helius_standard", False, T0 + 2_000_000, "closed")
        h.owner.on_feed_health("existing_rpc", False, T0 + 2_000_000, "closed")
        h.owner.on_feed_health("helius_standard", True, T0 + 4_000_000, "c2")
        h.feed("helius_standard", pump_frame(h, MINT_A, 10_000_000, "p", 101), T0 + 5_000_000)
        self.assertTrue(pos.canonical_provider_gap_flag)
        h.owner.finalize(T0 + 6_000_000)
        rep = h.report()
        self.assertEqual(rep["qualification_status"], "UNQUALIFIED_DATA")
        self.assertEqual(rep["economic"]["censored"], 1)
        self.assertEqual(rep["quality"]["positions_with_canonical_gap"], [pos.position_id])
        self.assertLess(rep["economic"]["conservative_lower_bound_pnl_lamports_token_residual_zero"], 0)


def _cfg_with(top: dict):
    raw = load_json_strict(ROOT / "config" / "phase1.json")
    raw.update(top)
    from darkbot.config import build_runtime_config
    return build_runtime_config(ROOT / "config" / "phase1.json", raw)


class T25T30Protocol(unittest.TestCase):
    def test_T25_protocol_variants(self):
        h = Harness()
        # unpinned tail -> non-actionable
        h.feed("helius_standard", logs_notification("u", 1, create_frame_logs(MINT_A, CREATOR, tail=b"\0" * 32)), T0 + 1_000_000)
        self.assertEqual(h.owner.reducer.states[MINT_A].quote_scope_status, QuoteScopeStatus.QUOTE_UNKNOWN_LAYOUT_UNVERIFIED)
        self.assertEqual(len(h.owner.positions), 0)
        # unknown discriminator in the create tx -> PARTIAL, non-actionable
        import base64
        logs = create_frame_logs(MINT_B, CREATOR)
        logs.insert(2, "Program data: " + base64.b64encode(event_discriminator("FutureEvent") + b"\1" * 40).decode())
        r = h.feed("helius_standard", logs_notification("p", 2, logs), T0 + 2_000_000)
        self.assertEqual(r.envelope.completeness.value, "PARTIAL")
        self.assertEqual(len(h.owner.positions), 0)
        # pinned v2 tail with quote_mint field: WSOL / native sentinel actionable, other -> UNSUPPORTED_QUOTE
        cfg = base_config()
        raw = json.loads(json.dumps(cfg.raw))
        raw["protocol"]["create_event_tail_layouts"].append({"layout_id": "CREATE_EVENT_V2_QUOTE_MINT", "tail_fields": [["quote_mint", "pubkey"]], "quote_mint_field": "quote_mint", "implied_quote_asset": None, "verification_status": "VERIFIED_TEST_FIXTURE", "token_program": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"})
        from darkbot.config import build_runtime_config
        cfg2 = build_runtime_config(cfg.path, raw)
        for mint_name, quote, expect in (("q1", cfg.protocol["wsol_mint"], QuoteScopeStatus.SOL_WSOL), ("q2", cfg.protocol["native_sol_curve_sentinel"], QuoteScopeStatus.SOL_WSOL), ("q3", b58encode(b"usdc".ljust(32, b"\0")), QuoteScopeStatus.UNSUPPORTED_QUOTE)):
            h2 = Harness(cfg2)
            mint = b58encode(mint_name.encode().ljust(32, b"\0"))
            tail = encode_fields([("quote_mint", quote)], [["quote_mint", "pubkey"]])
            h2.feed("helius_standard", logs_notification("s", 1, create_frame_logs(mint, CREATOR, tail=tail)), T0 + 1_000_000)
            self.assertEqual(h2.owner.reducer.states[mint].quote_scope_status, expect)
            self.assertEqual(len(h2.owner.positions), 1 if expect == QuoteScopeStatus.SOL_WSOL else 0)

    def test_T26_fee_snapshot_missing_or_null(self):
        cfg = base_config()
        from darkbot.clock import ReplayClock
        clock = ReplayClock("e", T0, 0)
        r = FeeResolver(cfg, None, clock, "run", "NONE")
        snaps = r.resolve_all()
        self.assertTrue(all(s.evidence == CostEvidence.UNKNOWN for s in snaps if s.source_type != "NETWORK_FEE"))
        self.assertIsNone(snaps[0].normalized_fee_components["protocol_fee_bps"])

        def rpc(method, params):
            if method == "getFeeForMessage":
                return {"context": {"slot": 1}, "value": None}
            if method == "getLatestBlockhash":
                return {"context": {"slot": 1}, "value": {"blockhash": b58encode(b"\1" * 32)}}
            if method == "getRecentPrioritizationFees":
                return [{"slot": 1, "prioritizationFee": 5000}]
            if method == "getMinimumBalanceForRentExemption":
                return 2039280
            if method == "getAccountInfo":
                return {"context": {"slot": 1}, "value": {"owner": "wrong", "data": ["AA==", "base64"]}}
            raise AssertionError(method)

        r2 = FeeResolver(cfg, rpc, clock, "run", "test")
        snaps = r2.resolve_all()
        net = next(s for s in snaps if s.source_type == "NETWORK_FEE")
        self.assertEqual(net.validation_status, "RPC_RETURNED_NULL")
        self.assertIsNone(net.normalized_fee_components["network_fee_total_lamports"])
        proto = next(s for s in snaps if s.source_type == "PROTOCOL_GLOBAL_ACCOUNT")
        self.assertEqual(proto.validation_status, "INVALID_OWNER")
        h = Harness(cfg, fees=r2.cache)
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        self.assertEqual(h.rows("select reason from decisions")[0]["reason"], "UNQUALIFIED_COSTS")
        self.assertEqual(h.rows("select count(*) from feed_observations")[0][0], 1)
        self.assertEqual(compact_u16(300), b"\xac\x02")
        self.assertEqual(len(build_fee_probe_message(1, 200000, 1000, b"\0" * 32)), 3 + 1 + 64 + 32 + 1 + (1 + 1 + 1 + 5) + (1 + 1 + 1 + 9))

    def test_T27_fees_change_between_buy_and_sell(self):
        with tempfile.TemporaryDirectory() as d:
            h = Harness(db_path=str(d + "/f.sqlite"))
            pos = open_position(h)
            new = fee_basis(h.cfg, T0 + 2_000_000, protocol_bps=200, creator_bps=50, run_id="run2")
            for s in (new.protocol, new.network, new.rent, new.priority_policy):
                h.recorder.try_append(("fee_snapshot", (s, "run")))
            h.fees = new
            h.owner.on_fee_snapshots([new.protocol, new.network, new.rent, new.priority_policy])  # as Application._on_fee_snapshots does
            h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 3_000_000)
            h.advance(T0 + 4_000_000)
            a = attempts(h)
            self.assertNotEqual(a[0]["fee_snapshot_id"], a[1]["fee_snapshot_id"])
            self.assertEqual(a[1]["fee_snapshot_id"], "run2_fee_protocol")
            self.assertEqual(pos.state, PositionState.CLOSED)
            h.finish(T0 + 5_000_000)
            conn = open_connection(Path(d) / "f.sqlite")
            rin = load_replay_input(conn, "run", h.cfg)
            rep = run_replay(conn, rin, h.cfg, "rep", "camp", persist=False)
            self.assertEqual(rep.digest["positions"], digest(h.owner)["positions"])  # replay keeps historical fee knowledge

    def test_T28_no_double_counting(self):
        h = Harness()
        pos = open_position(h)
        h.feed("helius_standard", pump_frame(h, MINT_A, 1_000_000, "hb", 101), T0 + 1_000_000 + 590_000_000_000 - 100_000_000)
        h.advance(T0 + 1_000_000 + 590_000_000_000)
        a = attempts(h)
        buy, sell = a[0], a[1]
        self.assertEqual(buy["network_fee_total_lamports"], buy["base_fee_lamports"] + buy["priority_fee_lamports"])
        self.assertEqual(buy["fees_charged_lamports"], buy["amount_in_base_units"] + buy["network_fee_total_lamports"] + buy["sender_tip_lamports"] + buy["rent_locked_lamports"])
        self.assertEqual(buy["amount_in_base_units"], buy["entry_curve_input_lamports"] + buy["protocol_fee_lamports"] + buy["creator_fee_lamports"] + buy["other_protocol_fee_lamports"])
        l = {r["kind"]: r for r in h.rows("select * from ledger")}
        self.assertEqual(l["EXIT_FILL"]["cash_delta_lamports"], sell["amount_out_base_units"] - sell["network_fee_total_lamports"] - sell["sender_tip_lamports"] + sell["rent_refunded_lamports"])
        self.assertEqual(pos.net_pnl_lamports, l["ENTRY_FILL"]["cash_delta_lamports"] + l["EXIT_FILL"]["cash_delta_lamports"])
        self.assertEqual(buy["slippage_bps"], 300)
        self.assertNotIn("slippage", json.dumps([dict(r) for r in h.rows("select provenance from ledger")]).lower())

    def test_T29_token_account_rent(self):
        h = Harness()
        pos = open_position(h)
        self.assertEqual(attempts(h)[0]["rent_locked_lamports"], 2_039_280)
        h.feed("helius_standard", pump_frame(h, MINT_A, 1_000_000, "hb", 101), T0 + 1_000_000 + 590_000_000_000 - 100_000_000)
        h.advance(T0 + 1_000_000 + 590_000_000_000)
        a = attempts(h)
        self.assertEqual(a[1]["rent_refunded_lamports"], 2_039_280)
        self.assertEqual(sum(r["locked_rent_delta_lamports"] for r in h.rows("select * from ledger")), 0)
        h2 = Harness(base_config(execution={"token_account_policy": {**base_config().execution["token_account_policy"], "assume_new_token_account_per_position": False, "close_token_account_on_full_exit": False, "spl_token_account_size_bytes": 165, "token_2022_supported": False}}))
        open_position(h2)
        self.assertEqual(attempts(h2)[0]["rent_locked_lamports"], 0)
        self.assertEqual(h2.owner.portfolio.locked_rent_lamports, 0)

    def test_T30_precomputed_vs_reconciled(self):
        """Attempt costs are precomputed at submit and applied with the fee rule valid at the modeled fill; an attempt whose modeled fill lies in a
        coverage gap stays UNKNOWN (costs NULL, position UNRESOLVED) and is never reconciled with a later (future) state."""
        h = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        self.assertEqual(attempts(h)[0]["fee_reconciliation_status"], "PRECOMPUTED")
        h.owner.on_feed_health("helius_standard", False, T0 + 10_000_000, "closed")
        h.owner.on_feed_health("existing_rpc", False, T0 + 10_000_000, "closed")
        h.advance(T0 + 60_000_000)
        pos = h.position()
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        a = attempts(h)[0]
        self.assertEqual(a["status"], AttemptStatus.UNKNOWN.value)
        self.assertIsNone(a["fees_charged_lamports"])
        self.assertEqual(a["fee_reconciliation_status"], "UNRESOLVED_UNKNOWN")
        d0 = dict(h.rows("select * from decisions")[0])
        h.owner.on_feed_health("helius_standard", True, T0 + 70_000_000, "c2")
        h.feed("helius_standard", pump_frame(h, MINT_A, 10_000_000, "p", 101), T0 + 80_000_000)
        a = attempts(h)[0]
        self.assertEqual(a["status"], AttemptStatus.UNKNOWN.value)  # no future-state reconciliation, no back-dated fill
        self.assertIsNone(a["fill_commitment"])
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(dict(h.rows("select * from decisions")[0]), d0)
        self.assertTrue(pos.canonical_provider_gap_flag)
        self.assertEqual(h.owner.occupied_slots(), 1)
        self.assertEqual(h.report()["qualification_status"], "UNQUALIFIED_COSTS")
        # fee rule change between submit and modeled fill: the fill applies the rule valid at the modeled fill time
        h2 = Harness(base_config(latency_model={"entry_delay_ms": 50}))
        h2.feed("helius_standard", create_payload(), T0 + 1_000_000)
        newer = fee_basis(h2.cfg, T0 + 20_000_000, protocol_bps=200, creator_bps=50, run_id="run2")
        for x in (newer.protocol, newer.network, newer.rent, newer.priority_policy):
            h2.recorder.try_append(("fee_snapshot", (x, "run")))
        h2.fees = newer
        h2.owner.on_fee_snapshots([newer.protocol, newer.network, newer.rent, newer.priority_policy])
        h2.advance(T0 + 60_000_000)
        a2 = attempts(h2)[0]
        self.assertEqual(a2["status"], "FILLED")
        self.assertEqual(a2["fee_snapshot_id"], "run2_fee_protocol")  # rule valid at modeled fill, not at submit
        self.assertEqual(a2["fee_reconciliation_status"], "APPLIED_AT_MODELED_FILL")
        self.assertEqual(a2["protocol_fee_lamports"], -(-(a2["entry_curve_input_lamports"] * 200) // 10_000))
        self.assertIsNotNone(a2["fill_quote_snapshot_id"])
        self.assertEqual(h2.rows("select count(*) from quote_snapshots where quote_snapshot_id=?", (a2["fill_quote_snapshot_id"],))[0][0], 1)


class T31T35Contracts(unittest.TestCase):
    def test_T31_timeout_vs_research_end(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 20000}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("helius_standard", pump_frame(h, MINT_A, 1_000_000, "hb0", 101), T0 + 1_000_000 + 20_000_000_000 - 50_000_000)  # fresh reserves for the 20 s delayed fill
        h.advance(T0 + 1_000_000 + 20_000_000_000)
        pos = h.position()
        self.assertEqual(pos.state, PositionState.OPEN)
        self.assertEqual(pos.timeout_due_mono_ns, T0 + 1_000_000 + 20_000_000_000 + 590_000_000_000)
        h.advance(T0 + 1_000_000 + 600_000_000_000 + 1)
        self.assertEqual(pos.research_status, "CENSORED")
        self.assertEqual(pos.state, PositionState.OPEN)
        self.assertEqual(ledger_kinds(h), ["ENTRY_FILL"])
        h.feed("helius_standard", pump_frame(h, MINT_A, 1_000_000, "hb1", 102), T0 + 1_000_000 + 610_000_000_000 - 100_000_000)
        h.advance(T0 + 1_000_000 + 610_000_000_000 + 1)
        self.assertEqual([x["side"] for x in attempts(h)], ["BUY", "SELL"])
        self.assertEqual(pos.origin_trigger, ExitReason.TIMEOUT)

    def test_T32_timeout_without_fresh_quote(self):
        h = Harness(base_config(latency_model={"exit_delay_ms": 20}))
        pos = open_position(h)
        h.owner.on_feed_health("helius_standard", False, T0 + 2_000_000, "closed")
        h.owner.on_feed_health("existing_rpc", False, T0 + 2_000_000, "closed")
        h.advance(T0 + 1_000_000 + 590_000_000_000)
        self.assertEqual(pos.state, PositionState.OPEN)
        self.assertEqual([x for x in attempts(h) if x["side"] == "SELL"], [])
        self.assertTrue(any(r["reason"].startswith("EXIT_LATCHED") for r in h.rows("select reason from decisions")))
        h.owner.on_feed_health("helius_standard", True, T0 + 1_000_000 + 591_000_000_000, "c2")
        base = T0 + 1_000_000 + 592_000_000_000
        h.feed("helius_standard", pump_frame(h, MINT_A, 10_000_000, "p", 101), base)  # latched exit executes on the first valid state
        for i, t in enumerate((base + 10_000_000, base + 80_000_000, base + 250_000_000)):
            h.feed("helius_standard", dump_frame(h, MINT_A, 300_000_000_000_000, f"d{i}", 102 + i), t)
        h.advance(base + 500_000_000)
        self.assertEqual(pos.state, PositionState.UNRESOLVED)
        self.assertEqual(pos.unresolved_reason, "EXIT_RETRIES_EXHAUSTED")
        self.assertEqual(len([x for x in attempts(h) if x["side"] == "SELL"]), 3)
        self.assertEqual(pos.origin_trigger, ExitReason.TIMEOUT)

    def test_T33_sizing_contract_worked_example(self):
        q = QuoteSnapshot("q", Side.BUY, "e", "f", 0, 0, 98_370_000, None, 1, 1_000_000, 0, 0, 1_000_000, 99_370_000, None, 105_000, 5000, 100_000, 0, 2_000_000, 0, 300, None, 99_370_000, 0, "m", "r", "OK", 1, 200000, 1, "MODELED")
        self.assertEqual(q.entry_economic_cost, 99_475_000)
        self.assertEqual(q.entry_cash_debit, 101_475_000)
        # no silently smaller entry: insufficient capital -> INSUFFICIENT_CAPITAL
        h = Harness(base_config(portfolio={"initial_paper_balance_lamports": 100_000_000}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        self.assertEqual(h.rows("select reason from decisions")[0]["reason"], "INSUFFICIENT_CAPITAL")
        h2 = Harness()
        open_position(h2)
        a = attempts(h2)[0]
        self.assertLessEqual(a["entry_curve_input_lamports"], 98_370_000)
        self.assertGreater(a["entry_curve_input_lamports"], 98_370_000 - 10)  # budget fully used up to integer rounding
        qs = h2.rows("select max_input_base_units, swap_wallet_debit_lamports from quote_snapshots")[0]
        self.assertEqual(qs["max_input_base_units"], qs["swap_wallet_debit_lamports"] * 10300 // 10000)  # max_sol_cost fee-inclusive, not the bare curve budget
        reserve = h2.owner.engine.exit_reserve_lamports(h2.fees.basis(), T0)
        self.assertEqual(reserve, 3 * 7000)
        self.assertEqual(h2.owner.portfolio.reserved_lamports, reserve)

    def test_T34_simultaneous_creates_one_slot(self):
        h = Harness()
        h.feed("helius_standard", create_payload(MINT_A), T0 + 1_000_000)
        h.feed("existing_rpc", create_payload(MINT_B, sig="sigB", slot=101), T0 + 1_000_000)
        self.assertEqual(len(h.owner.positions), 1)
        self.assertEqual(h.rows("select reason from decisions order by decision_id")[1]["reason"], "SLOT_LIMIT")
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 102), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        self.assertEqual(h.owner.occupied_slots(), 0)
        h.feed("helius_standard", create_payload(MINT_B, sig="sigB", slot=101), T0 + 4_000_000)  # duplicate must not reopen the skipped launch
        self.assertEqual(len(h.owner.positions), 1)

    def test_T35_schema_and_foreign_keys(self):
        conn = sqlite3.connect(":memory:")
        conn.execute("PRAGMA foreign_keys=ON")
        apply_migrations(conn)
        info = integrity_check(conn)
        self.assertEqual(info["foreign_key_check"], [])
        self.assertEqual(info["integrity_check"], "ok")
        self.assertTrue(set(REQUIRED_TABLES) <= set(info["tables"]))
        self.assertEqual(info["foreign_keys_pragma"], 1)
        with self.assertRaises(sqlite3.IntegrityError):
            conn.execute("INSERT INTO runs(run_id,campaign_id,mode,started_utc,code_revision,spec_hash,config_hash,host_clock_epoch_id,status,quality_status,start_balance_lamports) VALUES ('r','c','PAPER','t','c','s','missing','e','RUNNING','OK',1)")
        h = Harness()
        pos = open_position(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        self.assertEqual(h.fk_check(), [])
        self.assertEqual(h.rows("select count(*) from quote_snapshots q left join fee_snapshots f on q.fee_snapshot_id=f.fee_snapshot_id where f.fee_snapshot_id is null")[0][0], 0)
        with self.assertRaises(sqlite3.IntegrityError):
            h.conn.execute("INSERT INTO execution_attempts(attempt_id,run_id,position_id,side,attempt_number,adapter,status,submit_mono_ns,scheduled_fill_mono_ns,model_hash,slippage_bps,cost_model_hash,cost_evidence_status,fee_reconciliation_status,attempt_decision_id,fill_evidence_status) "
                           "SELECT 'dup',run_id,position_id,side,attempt_number,adapter,status,submit_mono_ns,scheduled_fill_mono_ns,model_hash,slippage_bps,cost_model_hash,cost_evidence_status,fee_reconciliation_status,attempt_decision_id,fill_evidence_status FROM execution_attempts LIMIT 1")


if __name__ == "__main__":
    unittest.main()
