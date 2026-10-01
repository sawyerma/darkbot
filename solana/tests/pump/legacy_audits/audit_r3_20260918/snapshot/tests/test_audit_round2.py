"""Counterexamples for audit round 2 (S01–S17 + CLI): each test reproduces the reported defect against the fixed code."""

from __future__ import annotations

import base64
import json
import sqlite3
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path
from unittest import mock

from darkbot.backtest import chronological_split, delay_scenarios, register_variants, run_research
from darkbot.config import build_runtime_config
from darkbot.execution import ExecutionPolicy, QuoteEngine
from darkbot.market import b58encode, encode_fields, event_discriminator
from darkbot.replay import load_replay_input, run_replay
from darkbot.storage import open_connection
from darkbot.domain import ExitReason, PositionState
from tests.helpers import BUYER, CREATOR, MINT_A, MINT_B, PROGRAM, ROOT, T0, Harness, attempts, base_config, create_frame_logs, create_payload, creator_sell_frame, dump_frame, logs_notification, open_position, pump_frame, trade_frame_logs, trade_log_line


def _v2cfg(cfg, token_program="TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"):
    raw = json.loads(json.dumps(cfg.raw))
    raw["protocol"]["create_event_tail_layouts"].append({"layout_id": "V2", "tail_fields": [["quote_mint", "pubkey"], ["token_program", "pubkey"]], "quote_mint_field": "quote_mint", "implied_quote_asset": None, "verification_status": "VERIFIED_TEST", "token_program": token_program})
    return build_runtime_config(cfg.path, raw)


class S01S03(unittest.TestCase):
    def test_S03_partial_copy_never_poisons_complete_copy(self):
        h = Harness()
        logs = create_frame_logs(MINT_A, CREATOR)
        logs.insert(3, "Program data: " + base64.b64encode(event_discriminator("FutureEvent") + b"\1" * 40).decode())  # unknown event after the create (same log index for the create on every provider)
        h.feed("existing_rpc", logs_notification("s", 1, logs), T0 + 1_000_000)  # PARTIAL first (unknown discriminator in the tx)
        self.assertEqual(len(h.owner.positions), 0)
        r = h.feed("helius_standard", create_payload(sig="s", slot=1), T0 + 2_000_000)  # COMPLETE copy of the same tx
        self.assertEqual([e.validation.value for e in r.events], ["VALID"])
        self.assertEqual(len(h.owner.positions), 1)
        r2 = h.feed("existing_rpc", create_payload(sig="s", slot=1), T0 + 3_000_000)  # third copy is a duplicate
        self.assertEqual([e.validation.value for e in r2.events], ["DUPLICATE"])

    def test_S15_conflicting_content_and_reorg_block_entries(self):
        h = Harness()
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        # same canonical id, different content -> CONFLICT, quote uncertain, quality event
        alt = logs_notification("sigC", 100, create_frame_logs(MINT_A, CREATOR, vsol=31_000_000_000))
        r = h.feed("existing_rpc", alt, T0 + 4_000_000)
        self.assertEqual([e.validation.value for e in r.events], ["CONFLICT"])
        self.assertTrue(h.owner.reducer.states[MINT_A].quote_uncertain)
        self.assertEqual(h.rows("select count(*) from quality_events where kind='EVENT_CONFLICT'")[0][0], 1)
        # a create whose tx is later reported failed (processed reorg) -> quote uncertain, no entry on that mint
        h2 = Harness()
        h2.feed("helius_standard", create_payload(MINT_B, sig="sigB", slot=5), T0 + 1_000_000)
        h2.feed("helius_standard", creator_sell_frame(h2, MINT_B, "csB", 6), T0 + 2_000_000)
        h2.advance(T0 + 3_000_000)
        h2.feed("existing_rpc", logs_notification("sigB", 5, [], err={"InstructionError": [0, "Custom"]}), T0 + 4_000_000)
        self.assertTrue(h2.owner.reducer.states[MINT_B].quote_uncertain)
        self.assertEqual(h2.owner.reducer.reorg_conflicts, 1)
        h3 = Harness()
        h3.owner.reducer.state_for(MINT_A).quote_uncertain = True
        h3.feed("helius_standard", create_payload(), T0 + 1_000_000)
        self.assertEqual(h3.rows("select reason from decisions")[0]["reason"], "QUOTE_CONFLICT")


class S08S12(unittest.TestCase):
    def test_S08_token_2022_rejected_when_unsupported(self):
        cfg = _v2cfg(base_config())
        h = Harness(cfg)
        tail = encode_fields([("quote_mint", cfg.protocol["wsol_mint"]), ("token_program", cfg.protocol["token_2022_program"])], [["quote_mint", "pubkey"], ["token_program", "pubkey"]])
        h.feed("helius_standard", logs_notification("s", 1, create_frame_logs(MINT_A, CREATOR, tail=tail)), T0 + 1_000_000)
        self.assertEqual(len(h.owner.positions), 0)
        self.assertEqual(h.rows("select reason from decisions")[0]["reason"], "UNSUPPORTED_TOKEN_PROGRAM")
        tail_ok = encode_fields([("quote_mint", cfg.protocol["wsol_mint"]), ("token_program", cfg.protocol["spl_token_program"])], [["quote_mint", "pubkey"], ["token_program", "pubkey"]])
        h2 = Harness(cfg)
        h2.feed("helius_standard", logs_notification("s", 1, create_frame_logs(MINT_A, CREATOR, tail=tail_ok)), T0 + 1_000_000)
        self.assertEqual(len(h2.owner.positions), 1)

    def test_S12_foreign_quote_reserves_never_enter_sol_state_or_columns(self):
        cfg = _v2cfg(base_config())
        h = Harness(cfg)
        tail = encode_fields([("quote_mint", b58encode(b"USDC".ljust(32, b"\0"))), ("token_program", cfg.protocol["spl_token_program"])], [["quote_mint", "pubkey"], ["token_program", "pubkey"]])
        h.feed("helius_standard", logs_notification("s", 1, create_frame_logs(MINT_A, CREATOR, tail=tail)), T0 + 1_000_000)
        st = h.owner.reducer.states[MINT_A]
        self.assertIsNone(st.virtual_sol_reserves)
        self.assertEqual(st.state_history, [])
        row = h.rows("select virtual_sol_reserves, virtual_token_reserves, quote_scope_status from market_events")[0]
        self.assertEqual((row["virtual_sol_reserves"], row["virtual_token_reserves"], row["quote_scope_status"]), (None, None, "UNSUPPORTED_QUOTE"))
        h.feed("helius_standard", logs_notification("t", 2, trade_frame_logs(MINT_A, 10, 10, True, BUYER, 31_000_000_000, 1_000_000_000_000_000, 700_000_000_000_000)), T0 + 2_000_000)
        self.assertIsNone(h.owner.reducer.states[MINT_A].virtual_sol_reserves)  # trades on a foreign-quote curve stay out of SOL state too
        self.assertEqual(h.rows("select virtual_sol_reserves from market_events where event_type='BUY'")[0][0], None)

    def test_trade_tail_layout_is_evidence_only(self):
        h = Harness()
        open_position(h)
        tail = encode_fields([("fee_recipient", b58encode(b"fr".ljust(32, b"\0"))), ("fee_basis_points", 95), ("fee", 1), ("creator", CREATOR), ("creator_fee_basis_points", 30), ("creator_fee", 1)],
                             [["fee_recipient", "pubkey"], ["fee_basis_points", "u64"], ["fee", "u64"], ["creator", "pubkey"], ["creator_fee_basis_points", "u64"], ["creator_fee", "u64"]])
        st = h.owner.reducer.states[MINT_A]
        line = trade_log_line(MINT_A, 10_000_000, 10_000_000_000, True, BUYER, st.virtual_sol_reserves + 10_000_000, st.virtual_token_reserves - 10_000_000_000, st.real_token_reserves - 10_000_000_000, tail=tail)
        r = h.feed("helius_standard", logs_notification("tt", 101, [f"Program {PROGRAM} invoke [1]", line, f"Program {PROGRAM} success"]), T0 + 2_000_000)
        ev = r.events[0].event
        self.assertEqual(ev.quote_layout_hash, "TRADE_EVENT_TAIL_CANDIDATE_FEES_2025")
        self.assertFalse(ev.extra["tail_verified"])
        self.assertEqual(ev.extra["tail_values"]["fee_basis_points"], 95)
        self.assertEqual(attempts(h)[0]["protocol_fee_lamports"], -(-(attempts(h)[0]["entry_curve_input_lamports"] * 100) // 10_000))  # fees still from the verified snapshot rule, never from the unverified tail


class S13S14(unittest.TestCase):
    def test_S13_failed_entries_count_in_expectancy_bootstrap_and_utilization(self):
        h = Harness(base_config(latency_model={"entry_delay_ms": 20}))
        h.feed("helius_standard", create_payload(), T0 + 1_000_000)
        h.feed("helius_standard", pump_frame(h, MINT_A, 20_000_000_000, "p", 101), T0 + 5_000_000)
        h.advance(T0 + 30_000_000)
        e = h.report()["economic"]
        self.assertEqual(e["resolved_intents"], 1)
        self.assertEqual(e["expectancy_per_intent_lamports"], -7000)
        self.assertEqual(e["block_bootstrap_per_intent_pnl"]["blocks"], 1)
        self.assertGreater(e["capital_utilization_fraction"], 0)

    def test_S14_provenance_columns(self):
        h = Harness()
        pos = open_position(h)
        h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
        h.advance(T0 + 3_000_000)
        for a in attempts(h):
            self.assertEqual(a["status"], "FILLED")
            self.assertTrue(a["fill_quote_snapshot_id"].endswith("_fill"))
            self.assertEqual(h.rows("select side from quote_snapshots where quote_snapshot_id=?", (a["fill_quote_snapshot_id"],))[0]["side"], a["side"])
            for col in ("network_fee_snapshot_id", "rent_fee_snapshot_id", "priority_fee_snapshot_id", "fee_snapshot_id"):
                self.assertEqual(h.rows("select count(*) from fee_snapshots where fee_snapshot_id=?", (a[col],))[0][0], 1)
            self.assertEqual(a["decoder_version"], "PUMP_LOG_DECODER_V1")
            self.assertEqual(a["fee_reconciliation_status"], "APPLIED_AT_MODELED_FILL")
        self.assertEqual(h.fk_check(), [])


class S05S06S09S10(unittest.TestCase):
    def _recording(self, d: Path, run_id: str, n: int = 3) -> Harness:
        h = Harness(db_path=str(d / "r.sqlite"), run_id=run_id)
        h.conn.execute("UPDATE runs SET mode='PAPER' WHERE run_id=?", (run_id,))
        for i in range(n):
            m = b58encode(f"{run_id}{i}".encode().ljust(32, b"\0"))
            h.feed("helius_standard", create_payload(m, sig=f"{run_id}s{i}", slot=100 + i), T0 + i * 700_000_000_000)
            h.feed("helius_standard", creator_sell_frame(h, m, f"{run_id}c{i}", 200 + i), T0 + i * 700_000_000_000 + 5_000_000)
        h.finish(T0 + n * 700_000_000_000)
        return h

    def test_S05_multiple_recordings_never_collide(self):
        with tempfile.TemporaryDirectory() as d:
            self._recording(Path(d), "runA")
            hb = self._recording(Path(d), "runB")
            conn = open_connection(Path(d) / "r.sqlite")
            rep = run_research(conn, "camp", hb.cfg, ROOT / "config" / "research.json", Path(d) / "out", persist=True, max_variants=2)
            ids = [r["run_id"] for r in rep["results"] if r.get("variant_id") != "ALL"]
            self.assertEqual(len(ids), len(set(ids)))
            self.assertTrue(all(("runA" in i) or ("runB" in i) for i in ids))
            per_run = {r[0]: r[1] for r in conn.execute("SELECT parent_run_id, COUNT(*) FROM runs WHERE mode='RESEARCH_REPLAY' GROUP BY parent_run_id")}
            self.assertEqual(set(per_run), {"runA", "runB"})
            for rid in ids:
                self.assertEqual(len({r[0] for r in conn.execute("SELECT DISTINCT run_id FROM positions WHERE run_id=?", (rid,))}), 1 if conn.execute("SELECT COUNT(*) FROM positions WHERE run_id=?", (rid,)).fetchone()[0] else 0)
            with self.assertRaises(ValueError):
                run_research(conn, "camp", hb.cfg, ROOT / "config" / "research.json", Path(d) / "out2", persist=True, max_variants=2)  # identical rerun collides -> refused

    def test_S06_dataset_hash_covers_fees_gaps_lateness_and_run_end(self):
        with tempfile.TemporaryDirectory() as d:
            h = self._recording(Path(d), "runA", n=1)
            conn = open_connection(Path(d) / "r.sqlite")
            base = load_replay_input(conn, "runA", h.cfg).dataset_hash
            conn.execute("UPDATE runs SET ended_mono_ns=ended_mono_ns+1 WHERE run_id='runA'")
            h1 = load_replay_input(conn, "runA", h.cfg).dataset_hash
            conn.execute("UPDATE fee_snapshots SET received_mono_ns=received_mono_ns+1 WHERE run_id='runA' AND fee_snapshot_id LIKE '%protocol%'")
            h2 = load_replay_input(conn, "runA", h.cfg).dataset_hash
            conn.execute("INSERT INTO quality_events(quality_event_id,run_id,start_mono_ns,end_mono_ns,kind,severity,scope,details,canonical_gap,coverage_version,recovery_status,evidence_observation_ids) VALUES ('g','runA',1,2,'CANONICAL_GAP','CRITICAL','ALL_MINTS','{}',1,'V2','OPEN','[]')")
            h3 = load_replay_input(conn, "runA", h.cfg).dataset_hash
            conn.execute("UPDATE execution_attempts SET callback_actual_mono_ns=callback_actual_mono_ns+5 WHERE run_id='runA'")
            h4 = load_replay_input(conn, "runA", h.cfg).dataset_hash
            self.assertEqual(len({base, h1, h2, h3, h4}), 5)

    def test_S09_config_values_take_effect(self):
        cfg = base_config(costs={"irrecoverable_account_cost_lamports": 1234, "irrecoverable_account_cost_provenance": "CONFIGURED_OPERATOR"})
        h = Harness(cfg)
        pos = open_position(h)
        a = attempts(h)[0]
        self.assertEqual(a["irrecoverable_account_cost_lamports"], 1234)
        self.assertEqual(a["fees_charged_lamports"], a["amount_in_base_units"] + a["network_fee_total_lamports"] + a["rent_locked_lamports"] + 1234)
        self.assertEqual(pos.entry_economic_cost_lamports, a["amount_in_base_units"] + a["network_fee_total_lamports"] + 1234)
        from darkbot.config import ConfigError
        with self.assertRaises(ConfigError):
            base_config(costs={"irrecoverable_account_cost_lamports": None, "irrecoverable_account_cost_provenance": "UNKNOWN"})
        research = json.loads((ROOT / "config" / "research.json").read_text())
        scen = delay_scenarios(research)
        self.assertIn((250, 0), scen)
        self.assertIn((0, 250), scen)
        variants = register_variants(research, h.cfg)
        self.assertTrue(any(v.reject_rate == "0.10" for v in variants))
        eng = QuoteEngine(ExecutionPolicy.from_config(h.cfg), stress_exit_reject_rate=Decimal("0.5"), stress_seed=7)
        picks = [eng.stress_rejects(f"q{i}") for i in range(200)]
        self.assertTrue(40 < sum(picks) < 160)
        self.assertEqual(picks, [QuoteEngine(ExecutionPolicy.from_config(h.cfg), stress_exit_reject_rate=Decimal("0.5"), stress_seed=7).stress_rejects(f"q{i}") for i in range(200)])  # deterministic per seed

    def test_S10_block_split_keeps_blocks_whole_and_purges_embargo(self):
        block = 3600 * 10**9
        launches = [(f"m{i}", T0 + (i // 3) * block + (i % 3) * 60 * 10**9) for i in range(30)]  # 10 blocks x 3 launches
        splits = chronological_split(launches, [Decimal("0.6"), Decimal("0.2"), Decimal("0.2")], embargo=0, block_length_ns=block)
        for name, mints in splits.items():
            blocks = {int((t - T0) // block) for m, t in launches if m in mints}
            for other, om in splits.items():
                if other != name:
                    self.assertFalse(blocks & {int((t - T0) // block) for m, t in launches if m in om}, (name, other))
        self.assertEqual(sum(len(v) for v in splits.values()), 30)
        purged = chronological_split(launches, [Decimal("0.6"), Decimal("0.2"), Decimal("0.2")], embargo=59 * 60 * 10**9, block_length_ns=block)
        self.assertLess(sum(len(v) for v in purged.values()), 30)  # launches within the embargo before a boundary are dropped


class S11S16S17(unittest.TestCase):
    def test_S11_features_for_skipped_creates_and_historical_coverage(self):
        h = Harness()
        open_position(h)
        h.feed("helius_standard", create_payload(MINT_B, sig="sigB", slot=200), T0 + 2_000_000)  # SLOT_LIMIT -> skipped, still gets snapshots
        h.owner.on_feed_health("helius_standard", False, T0 + 1_200_000_000, "closed")  # gap after the last cutoff
        h.owner.on_feed_health("existing_rpc", False, T0 + 1_200_000_000, "closed")
        h.advance(T0 + 3_000_000_000)
        rows = h.rows("select mint, quality_status, coverage_version from feature_snapshots where mint=?", (MINT_B,))
        self.assertEqual(len(rows), 10)
        self.assertTrue(all(r["quality_status"] == "OK" for r in rows))  # gap after cutoff never leaks into earlier cutoffs

    def test_S16_operative_limits_come_from_config(self):
        cfg = base_config(storage={"telemetry_samples_max": 10, "priority_headroom_records": 3, "queue_capacity_records": 6, "trade_log_per_mint": 2})
        h = Harness(cfg)
        self.assertEqual(h.owner.telemetry_max, 10)
        self.assertEqual(h.owner.reducer.trade_log_per_mint, 2)
        self.assertEqual(h.recorder.priority_headroom, 3)
        self.assertEqual(h.writer.locked_retries, 100)
        src = "\n".join((ROOT / "darkbot" / f).read_text() for f in ("paper.py", "storage.py", "fees.py", "main.py"))
        for token in ("20000", "512 * 1024", "0.2,", "retries < 100"):
            self.assertNotIn(token, src)

    def test_S17_paper_produces_artifacts_and_honest_exit_code(self):
        from darkbot import cli
        with tempfile.TemporaryDirectory() as d:
            h = Harness(db_path=str(Path(d) / "p.sqlite"))
            h.conn.execute("UPDATE runs SET mode='PAPER'")
            open_position(h)
            h.finish(T0 + 5_000_000)
            summary = {"run_id": "run", "status": "INTERRUPTED", "readiness": "CAPTURE_READY", "feeds_started": 1}
            with mock.patch("darkbot.main.run_paper", return_value=summary), mock.patch("darkbot.cli.Path", Path):
                import os
                cwd = os.getcwd()
                os.chdir(d)
                try:
                    code = cli.main(["paper", "--config", str(ROOT / "config" / "phase1.json"), "--db", str(Path(d) / "p.sqlite"), "--campaign", "camp"])
                finally:
                    os.chdir(cwd)
            self.assertEqual(code, 0)
            out = Path(d) / "exports" / "camp" / "run"
            self.assertTrue((out / "report.json").exists() and (out / "summary.md").exists() and (out / "positions.csv").exists() and (out / "latency_by_feed.csv").exists())
            conn = sqlite3.connect(str(Path(d) / "p.sqlite"))
            self.assertEqual(conn.execute("SELECT COUNT(*) FROM reports").fetchone()[0], 1)
            with mock.patch("darkbot.main.run_paper", return_value={"run_id": "run", "status": "COMPLETED", "readiness": "NOT_READY", "feeds_started": 0}):
                code = cli.main(["paper", "--config", str(ROOT / "config" / "phase1.json"), "--db", str(Path(d) / "p.sqlite"), "--campaign", "camp"])
            self.assertEqual(code, cli.EXIT_NOT_READY)


if __name__ == "__main__":
    unittest.main()


class S02StaleQuotes(unittest.TestCase):
    def test_S02_stale_fee_basis_never_fills_an_exit(self):
        """No refreshed fee snapshot -> the timeout exit stays latched (no fictitious FILLED); the first fresh snapshot lets it execute."""
        h = Harness()
        h.auto_refresh = False  # background resolver "dead"
        pos = open_position(h)
        h.advance(T0 + 1_000_000 + 590_000_000_000)
        self.assertEqual(pos.state, PositionState.OPEN)
        self.assertEqual([a for a in attempts(h) if a["side"] == "SELL"], [])
        self.assertTrue(any(r["reason"] == "EXIT_LATCHED_UNQUALIFIED_COSTS" for r in h.rows("select reason from decisions")))
        h.refresh_fees(h.clock.monotonic_ns())
        h.feed("helius_standard", pump_frame(h, MINT_A, 10_000_000, "p", 101), T0 + 1_000_000 + 591_000_000_000)
        h.advance(T0 + 1_000_000 + 592_000_000_000)
        self.assertEqual(pos.state, PositionState.CLOSED)
        self.assertEqual(pos.exit_reason, ExitReason.TIMEOUT)
        sell = [a for a in attempts(h) if a["side"] == "SELL"][0]
        self.assertGreaterEqual(sell["submit_mono_ns"], T0 + 1_000_000 + 591_000_000_000)
        self.assertEqual(sell["cost_evidence_status"], "MEASURED")
        self.assertNotIn("STALE", json.dumps(attempts(h)))
