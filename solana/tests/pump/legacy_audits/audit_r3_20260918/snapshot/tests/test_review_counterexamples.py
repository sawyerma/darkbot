"""Counterexamples from the external review: batch-split FK loss, invented replay exits after the recording, unverified fee layout marked VALID,
doctor counting non-actionable CREATEs, in-sample/zero-delay research qualification."""

from __future__ import annotations

import base64
import tempfile
import unittest
from pathlib import Path

from darkbot.backtest import Variant, apply_stress_gate, variant_status
from darkbot.clock import ReplayClock
from darkbot.fees import FeeResolver
from darkbot.market import b58encode, encode_fields
from darkbot.replay import load_replay_input, run_replay
from darkbot.storage import open_connection
from tests.helpers import MINT_A, T0, Harness, base_config, create_payload, creator_sell_frame, open_position, create_frame_logs, logs_notification, CREATOR


class BatchSplit(unittest.TestCase):
    def test_batch_boundary_never_loses_fk_coupled_rows(self):
        for batch_max in (1, 2, 3, 5, 7):
            h = Harness()
            h.writer.batch_max = batch_max
            open_position(h)
            h.feed("helius_standard", creator_sell_frame(h, MINT_A, "cs", 101), T0 + 2_000_000)
            h.advance(T0 + 3_000_000)
            # emulate the writer thread: drain in small batches exactly as _run does, including the FK-merge path
            held = []
            while h.recorder.depth() or held:
                batch = held + h.recorder.drain(batch_max)
                held = [] if h.writer._commit(batch) else batch
                if held and not h.recorder.depth():
                    self.fail(f"batch_max={batch_max}: FK-coupled records could not be committed: {h.writer.last_error}")
            self.assertEqual(h.writer.disk_error, False, batch_max)
            self.assertEqual(h.conn.execute("select count(*) from positions").fetchone()[0], 1, batch_max)
            self.assertEqual(h.conn.execute("select count(*) from decisions").fetchone()[0], 2, batch_max)
            self.assertEqual(h.conn.execute("select count(*) from execution_attempts").fetchone()[0], 2, batch_max)
            self.assertEqual(h.conn.execute("select count(*) from ledger").fetchone()[0], 2, batch_max)
            self.assertEqual(h.conn.execute("PRAGMA foreign_key_check").fetchall(), [], batch_max)


class ReplayEnd(unittest.TestCase):
    def test_replay_never_invents_exits_after_recording_end(self):
        with tempfile.TemporaryDirectory() as d:
            h = Harness(db_path=str(Path(d) / "r.sqlite"))
            pos = open_position(h)
            h.finish(T0 + 5_000_000)  # recording ends 5 ms after the fill; strategy timeout would be 590 s later
            conn = open_connection(Path(d) / "r.sqlite")
            r = run_replay(conn, load_replay_input(conn, "run", h.cfg), h.cfg, "rep", "c", persist=False)
            p = r.digest["positions"][0]
            self.assertEqual(p["state"], "OPEN")
            self.assertIsNone(p["exit_reason"])
            self.assertEqual(p["research_status"], "CENSORED_RUN_END")
            self.assertEqual(len(r.digest["attempts"]), 1)
            self.assertEqual(r.owner.clock.monotonic_ns(), T0 + 5_000_000)
            # interrupted recording without ended_mono_ns: the last observation bounds the model time
            conn.execute("UPDATE runs SET ended_mono_ns=NULL WHERE run_id='run'")
            r2 = run_replay(conn, load_replay_input(conn, "run", h.cfg), h.cfg, "rep2", "c", persist=False)
            self.assertEqual(r2.digest["positions"][0]["state"], "OPEN")


class FeeLayout(unittest.TestCase):
    def _rpc(self, cfg, owner):
        ga = cfg.protocol["global_account"]
        pk = lambda n: b58encode(n.encode().ljust(32, b"\0"))
        data = b"\0" * 8 + encode_fields([("initialized", True), ("authority", pk("a")), ("fee_recipient", pk("b")), ("initial_virtual_token_reserves", 1), ("initial_virtual_sol_reserves", 1), ("initial_real_token_reserves", 1),
                                           ("token_total_supply", 1), ("fee_basis_points", 95), ("withdraw_authority", pk("c")), ("enable_migrate", True), ("pool_migration_fee", 1), ("creator_fee_basis_points", 30)], ga["layout_after_discriminator"])
        return lambda m, p: {"context": {"slot": 1}, "value": {"owner": owner, "data": [base64.b64encode(data).decode(), "base64"]}}

    def test_unverified_layout_is_never_valid(self):
        cfg = base_config()
        self.assertFalse(cfg.protocol["global_account"]["verification_status"].startswith("VERIFIED"))
        s = FeeResolver(cfg, self._rpc(cfg, cfg.protocol["program_id"]), ReplayClock("e", T0, 0), "run", "t").resolve_protocol()
        self.assertEqual(s.validation_status, "LAYOUT_UNVERIFIED")
        self.assertIsNone(s.normalized_fee_components["protocol_fee_bps"])
        self.assertEqual(s.raw_values["candidate_protocol_fee_bps"], 95)  # visible for verification, not used
        self.assertEqual(s.evidence.value, "UNKNOWN")
        verified = cfg.with_overrides(protocol={"global_account": {**cfg.protocol["global_account"], "verification_status": "VERIFIED_2026-09-18_IDL_CHECK"}})
        s2 = FeeResolver(verified, self._rpc(verified, verified.protocol["program_id"]), ReplayClock("e", T0, 0), "run", "t").resolve_protocol()
        self.assertEqual(s2.validation_status, "VALID")
        self.assertEqual(s2.normalized_fee_components["protocol_fee_bps"], 95)
        s3 = FeeResolver(verified, self._rpc(verified, "SomeOtherOwner"), ReplayClock("e", T0, 0), "run", "t").resolve_protocol()
        self.assertEqual(s3.validation_status, "INVALID_OWNER")


class DoctorCounting(unittest.TestCase):
    def test_non_actionable_creates_do_not_count_as_valid_samples(self):
        h = Harness()
        h.feed("helius_standard", logs_notification("u", 1, create_frame_logs(MINT_A, CREATOR, tail=b"\0" * 40)), T0 + 1_000_000)  # unknown layout
        h.feed("helius_standard", create_payload(b58encode(b"ok".ljust(32, b"\0")), sig="ok", slot=2), T0 + 2_000_000)
        observed = h.rows("select count(*) from market_events where event_type='CREATE'")[0][0]
        actionable = h.rows("select count(*) from market_events where event_type='CREATE' and actionable_mono_ns is not null")[0][0]
        self.assertEqual((observed, actionable), (2, 1))  # doctor D03 uses the actionable count for minimum_valid_create_samples


class ResearchQualification(unittest.TestCase):
    def test_in_sample_and_zero_delay_cannot_qualify(self):
        cfg = base_config()
        research = {"minimum_independent_blocks_for_qualification": 30, "random_seed": 1}
        boot = {"status": "OK", "blocks": 40, "ci_low": 1000, "ci_high": 2000}
        v0 = Variant("x", "1.5", 0, 0, "1.0", 0, 1, False)
        v5 = Variant("y", "1.5", 5, 5, "1.0", 0, 1, False)
        self.assertEqual(variant_status(v0, "train", boot, [], research, cfg), "DIAGNOSTIC_ZERO_DELAY_CANNOT_QUALIFY")
        self.assertEqual(variant_status(v5, "train", boot, [], research, cfg), "CANDIDATE_IN_SAMPLE_NOT_QUALIFYING")
        self.assertEqual(variant_status(v5, "validation", boot, [], research, cfg), "CANDIDATE_IN_SAMPLE_NOT_QUALIFYING")
        self.assertEqual(variant_status(v5, "test", boot, [], research, cfg), "POSITIVE_UNDER_MODEL_PENDING_STRESS")
        self.assertEqual(variant_status(v5, "test", {**boot, "blocks": 10}, [], research, cfg), "INSUFFICIENT_DATA")
        self.assertEqual(variant_status(v5, "test", {**boot, "ci_low": -1}, [], research, cfg), "NEGATIVE")
        base = {"split": "test", "target": "1.5", "entry_delay_ms": 5, "exit_delay_ms": 5, "portfolio_limit": 1}
        outs = [{**base, "fee_multiplier": "1.0", "adverse_bps": 0, "status": "POSITIVE_UNDER_MODEL_PENDING_STRESS", "bootstrap": boot},
                {**base, "fee_multiplier": "2.0", "adverse_bps": 300, "status": "NEGATIVE", "bootstrap": {**boot, "ci_low": -5}}]
        apply_stress_gate(outs)
        self.assertEqual(outs[0]["status"], "NEGATIVE_UNDER_STRESS")
        outs[1]["bootstrap"]["ci_low"] = 1
        outs[0]["status"] = "POSITIVE_UNDER_MODEL_PENDING_STRESS"
        apply_stress_gate(outs)
        self.assertEqual(outs[0]["status"], "POSITIVE_UNDER_MODEL")
        solo = [{**base, "fee_multiplier": "1.0", "adverse_bps": 0, "status": "POSITIVE_UNDER_MODEL_PENDING_STRESS", "bootstrap": boot}]
        apply_stress_gate(solo)
        self.assertEqual(solo[0]["status"], "INSUFFICIENT_DATA_NO_STRESS_VARIANTS")


if __name__ == "__main__":
    unittest.main()
