"""Offline tests for the capture tool: classification, spend cap, time basis, ack/migration rules, validity, manifest (no network)."""

from __future__ import annotations

import argparse
import json
import shutil
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path
from typing import Any

from darkbot import capture as cap
from darkbot.capture import CaptureConfig, CaptureSession, classify, max_paid_messages_from_cap, parse, write_manifest
from darkbot.cli import ContractError, find_project_root, load_active_contract, sha256_file, sha256_json, validate_fixtures

PROJECT_ROOT = find_project_root(Path(__file__))
ACTIVE, _ = load_active_contract(PROJECT_ROOT)
assert ACTIVE is not None
CONTRACT = ACTIVE.document
SPEC = CONTRACT["phase0_fixture_contract"]
FEED = CONTRACT["market_data_source_v1"]["feed_pricing_observed"]
MINT = "SELFTESTMINT"
SECOND_NS = cap.NANOSECONDS_PER_SECOND


def config(out: Path, max_paid: int | None = None, trade_seconds: int = 60) -> CaptureConfig:
    limit = max_paid if max_paid is not None else SPEC["minimum_trade_events"] * 2
    return CaptureConfig(
        url="wss://selftest.invalid/api/data", out=out,
        tx_type_field="txType", create_value="create", buy_value="buy", sell_value="sell",
        mint_field="mint", pool_field="pool", pool_value="pump",
        signature_field="signature", trader_field="traderPublicKey", sol_field="solAmount", token_field="tokenAmount",
        vsol_field="vSolInBondingCurve", vtoken_field="vTokensInBondingCurve",
        spend_cap_sol=Decimal(limit * FEED["lamports_per_paid_message"]) / Decimal(SPEC["lamports_per_sol"]),
        lamports_per_paid_message=FEED["lamports_per_paid_message"], lamports_per_sol=SPEC["lamports_per_sol"],
        max_paid_messages=limit, minimum_trade_events=SPEC["minimum_trade_events"],
        create_wait_seconds=60, trade_seconds=trade_seconds, migration_wait_seconds=120,
        ping_interval_ms=CONTRACT["operational_limits_v1"]["keys"]["websocket_ping_interval_ms"]["initial"], migration_only=False,
    )


CREATE_TEXT = '{"txType": "create", "mint": "SELFTESTMINT", "pool": "pump", "signature": "sigC", "traderPublicKey": "dev", "solAmount": 1.5, "tokenAmount": 50000000.25, "vSolInBondingCurve": 31.5, "vTokensInBondingCurve": 1000000000}'
MIGRATION_TEXT = '{"mint": "SELFTESTMINT", "pool": "pump-amm", "signature": "sigM"}'


def trade_text(index: int, side: str, mint: str = MINT) -> str:
    return f'{{"txType": "{side}", "mint": "{mint}", "signature": "sig{index}", "traderPublicKey": "t{index % 3}", "solAmount": 0.1, "tokenAmount": 10, "vSolInBondingCurve": 31.6, "vTokensInBondingCurve": 999999990}}'


def run_session(cfg: CaptureConfig, sides: list[str], with_migration: bool = True, ack_frame: bool = False) -> tuple[CaptureSession, list[list[str]]]:
    """Feed a create, trades 50 ms apart (monotonic), optionally an ack-looking frame and a migration; collect outbound frames."""
    base_mono, base_utc = 5 * SECOND_NS, 1_700_000_000 * SECOND_NS
    session = CaptureSession(cfg, CONTRACT, started_mono_ns=base_mono - SECOND_NS)
    outbound = [session.initial_frames(), session.on_message(CREATE_TEXT, base_utc, base_mono)]
    if ack_frame:
        outbound.append(session.on_message('{"message": "Successfully subscribed to token trades"}', base_utc + 1, base_mono + 1_000_000))
    for index, side in enumerate(sides):
        offset = 50_000_000 * (index + 1)
        outbound.append(session.on_message(trade_text(index, side), base_utc + offset + 7_000_000_000, base_mono + offset))  # UTC deliberately skewed
    if with_migration:
        outbound.append(session.on_message(MIGRATION_TEXT, base_utc + 10 * SECOND_NS, base_mono + 10 * SECOND_NS))
    return session, outbound


class ClassifyTests(unittest.TestCase):
    def test_kinds(self) -> None:
        cfg = config(Path("."))
        self.assertEqual(classify({"txType": "create", "mint": "m"}, cfg, CONTRACT), cap.KIND_CREATE)
        self.assertEqual(classify({"txType": "buy", "mint": "m"}, cfg, CONTRACT), cap.KIND_BUY)
        self.assertEqual(classify({"txType": "sell", "mint": "m"}, cfg, CONTRACT), cap.KIND_SELL)
        self.assertEqual(classify({"mint": "m", "pool": "pump-amm", "signature": "s"}, cfg, CONTRACT), cap.KIND_MIGRATION_CANDIDATE)
        self.assertEqual(classify({"mint": "m", "pool": "pump-amm"}, cfg, CONTRACT), cap.KIND_OTHER, "a frame without signature is never a migration candidate")
        self.assertEqual(classify({"mint": "m", "signature": "s", "txType": "buy"}, cfg, CONTRACT), cap.KIND_BUY)
        self.assertEqual(classify({"message": "Successfully subscribed"}, cfg, CONTRACT), cap.KIND_OTHER)
        self.assertEqual(classify("not an object", cfg, CONTRACT), cap.KIND_OTHER)

    def test_parse_keeps_decimal(self) -> None:
        obj = parse('{"solAmount": 0.123456789123}')
        assert obj is not None
        self.assertEqual(obj["solAmount"], Decimal("0.123456789123"))
        self.assertIsNone(parse("not json"))

    def test_stop_reasons_and_checks_are_contract_values(self) -> None:
        for name in (cap.SUB_STOP_PAID_LIMIT, cap.SUB_STOP_WINDOW, cap.SUB_STOP_CONNECTION, cap.SUB_STOP_CAPTURE_ENDED, cap.SUB_STOP_NEVER):
            self.assertIn(name, SPEC["trade_subscription_stop_reasons"])
        for name in (cap.STOP_DONE, cap.STOP_MIGRATION_WAIT, cap.STOP_CREATE_WAIT, cap.STOP_CONNECTION, cap.STOP_MIGRATION_ONLY):
            self.assertIn(name, SPEC["capture_stop_reasons"])
        self.assertIn(cap.MIGRATION_NOT_OBSERVED, SPEC["migration_statuses"])
        for name in (cap.CHECK_CREATE, cap.CHECK_MIN_TRADES, cap.CHECK_SIDES, cap.CHECK_WARMUP, cap.CHECK_HASHES, cap.CHECK_MIGRATION):
            self.assertIn(name, SPEC["capture_validity_checks"])


class SpendCapTests(unittest.TestCase):
    def test_limit_is_derived_from_cap_and_price(self) -> None:
        self.assertEqual(max_paid_messages_from_cap(Decimal("0.01"), SPEC["lamports_per_sol"], 1000), 10000)
        self.assertEqual(max_paid_messages_from_cap(Decimal("0.0005"), SPEC["lamports_per_sol"], 1000), 500)
        with self.assertRaises(ContractError):
            max_paid_messages_from_cap(Decimal("1"), SPEC["lamports_per_sol"], 0)

    def test_paid_limit_closes_subscription_in_the_same_iteration(self) -> None:
        limit = SPEC["minimum_trade_events"]
        cfg = config(Path("."), max_paid=limit)
        sides = ["sell" if index % 5 == 4 else "buy" for index in range(limit + 3)]
        session, outbound = run_session(cfg, sides, with_migration=False)
        self.assertIn("subscribeTokenTrade", outbound[1][0])
        limit_frames = outbound[1 + limit]  # response to the limit-th paid message
        self.assertTrue(any("unsubscribeTokenTrade" in frame for frame in limit_frames), limit_frames)
        self.assertEqual(session.trade_subscription_stop_reason, cap.SUB_STOP_PAID_LIMIT)
        self.assertEqual(session.paid_messages_at_unsubscribe, limit)
        self.assertEqual(len(session.trades), limit, "no trade is stored after the limit")
        self.assertEqual(session.paid_message_count, limit + 3, "in-flight messages after unsubscribe are still counted, never hidden")
        self.assertFalse(session.subscribed)
        self.assertFalse(any("unsubscribeTokenTrade" in frame for frames in outbound[2 + limit :] for frame in frames), "unsubscribe is sent exactly once")

    def test_foreign_mint_trades_count_as_paid_but_are_not_stored(self) -> None:
        cfg = config(Path("."), max_paid=SPEC["minimum_trade_events"] * 2)
        session, _ = run_session(cfg, ["buy"] * 3, with_migration=False)
        session.on_message(trade_text(99, "buy", mint="OTHERMINT"), 1, 5 * SECOND_NS + 900_000_000)
        self.assertEqual(session.paid_message_count, 4)
        self.assertEqual(len(session.trades), 3)

    def test_trade_window_closes_subscription_on_tick(self) -> None:
        cfg = config(Path("."), trade_seconds=1)
        session, _ = run_session(cfg, ["buy", "sell"], with_migration=False)
        self.assertEqual(session.on_tick(session.subscribe_sent_mono_ns + SECOND_NS // 2), [], "window not elapsed yet")
        frames = session.on_tick(session.subscribe_sent_mono_ns + SECOND_NS)
        self.assertTrue(any("unsubscribeTokenTrade" in frame for frame in frames))
        self.assertEqual(session.trade_subscription_stop_reason, cap.SUB_STOP_WINDOW)
        self.assertFalse(session.done(), "capture keeps waiting for the migration on the free subscription")

    def test_migration_wait_ends_capture_and_closes_an_open_subscription(self) -> None:
        cfg = config(Path("."), trade_seconds=10_000)  # window longer than the migration wait
        session, _ = run_session(cfg, ["buy", "sell"], with_migration=False)
        frames = session.on_tick(session.started_mono_ns + cfg.migration_wait_seconds * SECOND_NS)
        self.assertTrue(any("unsubscribeTokenTrade" in frame for frame in frames), "no paid subscription may outlive the capture")
        self.assertEqual(session.capture_stop_reason, cap.STOP_MIGRATION_WAIT)
        self.assertEqual(session.migration_status(), cap.MIGRATION_NOT_OBSERVED)
        self.assertEqual(session.trade_subscription_stop_reason, cap.SUB_STOP_CAPTURE_ENDED)


class AckAndMigrationTests(unittest.TestCase):
    def test_unknown_frames_are_ack_candidates_not_acks(self) -> None:
        session, _ = run_session(config(Path(".")), ["buy", "sell"], ack_frame=True)
        self.assertEqual(len(session.ack_candidates), 1)
        self.assertEqual(session.migration_status(), SPEC["migration_statuses"][1], "unverified candidate until the contract proves the migration shape")


class ValidityTests(unittest.TestCase):
    def test_capture_without_sell_is_not_ok(self) -> None:
        session, _ = run_session(config(Path(".")), ["buy"] * SPEC["minimum_trade_events"])
        validity, exit_code = session.assess()
        self.assertEqual(exit_code, cap.EXIT_TOO_FEW_TRADES)
        self.assertFalse(validity["valid"])
        self.assertEqual(validity["failed"], [cap.CHECK_SIDES])

    def test_too_few_trades_is_not_ok(self) -> None:
        session, _ = run_session(config(Path(".")), ["buy", "sell"])
        self.assertEqual(session.assess()[1], cap.EXIT_TOO_FEW_TRADES)

    def test_missing_migration_is_not_ok(self) -> None:
        sides = ["sell" if index % 5 == 4 else "buy" for index in range(SPEC["minimum_trade_events"])]
        session, _ = run_session(config(Path(".")), sides, with_migration=False)
        validity, exit_code = session.assess()
        self.assertEqual(exit_code, cap.EXIT_NO_MIGRATION)
        self.assertEqual(validity["failed"], [cap.CHECK_MIGRATION])

    def test_main_refuses_while_spend_cap_is_unset(self) -> None:
        args = argparse.Namespace(url="wss://x", out=None, tx_type_field="t", create_value="c", buy_value="b", sell_value="s", mint_field="m", pool_field="p", pool_value="v", signature_field="g", trader_field="a", sol_field="q", token_field="k", vsol_field="vs", vtoken_field="vt", max_paid_messages=None, create_wait_seconds=None, trade_seconds=None, migration_wait_seconds=None, migration_only=False)
        if not str(SPEC["capture_spend_cap_sol"]).startswith("UNSET"):
            self.skipTest("spend cap has been decided; refusal no longer applies")
        with self.assertRaises(ContractError):
            cap.config_from_contract(args, CONTRACT, PROJECT_ROOT)


class ManifestTests(unittest.TestCase):
    def test_manifest_from_session_is_accepted_in_selftest_mode(self) -> None:
        root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, root)
        out = root / SPEC["provider_dir"]
        out.mkdir()
        (root / SPEC["feature_dir"]).mkdir()
        cfg = config(out)
        sides = ["sell" if index % 5 == 4 else "buy" for index in range(SPEC["minimum_trade_events"])]
        session, _ = run_session(cfg, sides, ack_frame=True)
        self.assertEqual(session.capture_stop_reason, None, "subscription still open: window not elapsed")
        session.on_tick(session.subscribe_sent_mono_ns + cfg.trade_seconds * SECOND_NS)
        self.assertEqual(session.capture_stop_reason, cap.STOP_DONE)
        validity, exit_code = session.assess()
        self.assertEqual(exit_code, cap.EXIT_OK)
        manifest_path = write_manifest(session, SPEC["provider_files"], SPEC["manifest_file"], validity)
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["synthetic_selftest"] = True
        manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        self.assertEqual(manifest["files"], {name: sha256_file(out / name) for name in SPEC["provider_files"].values()})
        self.assertEqual(manifest["offsets_time_basis"], SPEC["offsets_time_basis_value"])
        offsets = json.loads((out / "trades.offsets.json").read_text(encoding="utf-8"))
        self.assertEqual(offsets["0"], "50", "offset comes from monotonic time although the UTC clock was skewed by 7 s")
        warmup = manifest["warmup_measurement"]
        self.assertIsNone(warmup["ack_message_raw"])
        self.assertIsNone(warmup["subscribe_ack_offset_ns"])
        self.assertEqual(len(warmup["ack_candidates_raw"]), 1)
        self.assertEqual(warmup["first_trade_received_offset_ns"] - warmup["subscribe_sent_offset_ns"], 50_000_000)
        self.assertEqual(manifest["feed_cost"]["paid_message_count"], len(sides))
        self.assertEqual(manifest["migration_status"], SPEC["migration_statuses"][1])
        hashes = json.loads((out / "raw_hashes.json").read_text(encoding="utf-8"))
        self.assertEqual(hashes["trades"][0], sha256_json(session.trades[0].obj))
        self.assertEqual(validate_fixtures(root, CONTRACT, allow_synthetic=True), ["missing real fixtures: features/feature_cases.json"])
        (root / SPEC["feature_dir"] / SPEC["feature_cases_file"]).write_text(json.dumps(feature_cases(session, hashes, offsets, manifest_path)), encoding="utf-8")
        self.assertEqual(validate_fixtures(root, CONTRACT, allow_synthetic=True), [])


def feature_cases(session: CaptureSession, hashes: dict[str, Any], offsets: dict[str, str], manifest_path: Path) -> dict[str, Any]:
    """Minimal feature cases: E-features computed independently, R/C-features null (no decimals/protocol config read), Q from coverage."""
    definitions = CONTRACT["feature_contract_v1"]["definitions"]
    deadlines = sorted(set(CONTRACT["strategy"]["runtime_snapshot_ms_v1"]) | set(CONTRACT["strategy"]["research_replay_snapshot_ms"]))
    quantum = Decimal(1).scaleb(-SPEC["ratio_decimal_places"])
    lamports = Decimal(SPEC["lamports_per_sol"])
    events = [
        {"raw_sha256": hashes["trades"][i], "receive_offset_ms": offsets[str(i)], "event_type": "BUY" if t.obj["txType"] == "buy" else "SELL", "actor": t.obj["traderPublicKey"],
         "quote_lamports": 100_000_000, "token_base_units": None, "virtual_sol_reserves": 31_600_000_000, "virtual_token_reserves": None}
        for i, t in enumerate(session.trades)
    ]
    curve_inputs = {name: None for name in SPEC["curve_inputs_fields"] if name != "rule"}
    curve_inputs.update({"token_decimals_source": "selftest: mint account not read", "protocol_config_source": "selftest: not read", "create_virtual_sol_reserves": 31_500_000_000})
    cases = []
    for deadline in deadlines:
        visible = [e for e in events if Decimal(e["receive_offset_ms"]) <= deadline]
        buys = [e for e in visible if e["event_type"] == "BUY"]
        sells = [e for e in visible if e["event_type"] == "SELL"]
        expected: dict[str, Any] = {name: None for name in definitions}
        nulls: dict[str, str] = {}
        vol = Decimal(sum(e["quote_lamports"] for e in buys)) / lamports
        svol = Decimal(sum(e["quote_lamports"] for e in sells)) / lamports
        expected.update({"buy_count": len(buys), "sell_count": len(sells), "unique_buyers": len({e["actor"] for e in buys}), "buy_volume_sol": str(vol), "sell_volume_sol": str(svol), "net_buy_flow_sol": str(vol - svol)})
        per: dict[str, Decimal] = {}
        order: list[str] = []
        for e in buys:
            per[e["actor"]] = per.get(e["actor"], Decimal(0)) + Decimal(e["quote_lamports"])
            if e["actor"] not in order:
                order.append(e["actor"])
        total = Decimal(sum(per.values())) if per else Decimal(0)
        if buys:
            expected["unique_buyer_ratio"] = str((Decimal(len(per)) / Decimal(len(buys))).quantize(quantum))
            expected["largest_buy_sol"] = str(max(Decimal(e["quote_lamports"]) for e in buys) / lamports)
            expected["buyer_volume_concentration"] = str(sum(((v / total).quantize(quantum) ** 2 for v in per.values()), Decimal(0)).quantize(quantum))
            expected["first5_buyer_concentration"] = str((sum(per[b] for b in order[: SPEC["first_buyer_window"]]) / total).quantize(quantum))
        else:
            for name in ("unique_buyer_ratio", "largest_buy_sol", "buyer_volume_concentration", "first5_buyer_concentration"):
                nulls[name] = "no BUY before deadline"
        expected["first_sell_latency_ms"] = sells[0]["receive_offset_ms"] if sells else None
        if not sells:
            nulls["first_sell_latency_ms"] = "no SELL before deadline"
        for name in ("price_change", "max_drawdown", "curve_progress_pct", "research_estimated_price_impact", "dev_initial_buy"):
            nulls[name] = "curve inputs not read in selftest (token_decimals/protocol config null)"
        first_trade_ms = Decimal(offsets["0"])
        coverage = "OBSERVED" if first_trade_ms <= deadline else "UNKNOWN_WARMUP"
        expected["provider_gap_flag"] = coverage != "OBSERVED"
        cases.append({"case_id": f"selftest_{deadline}", "deadline_ms": deadline, "coverage_state_at_deadline": coverage, "data_quality_interval_intersects": False, "normalized_events": events, "expected": expected, "null_reasons": nulls, "derivation": "selftest spreadsheet-equivalent"})
    return {"feature_set_version": CONTRACT["feature_contract_v1"]["version"], "source_manifest_sha256": sha256_file(manifest_path), "curve_inputs": curve_inputs, "cases": cases}


if __name__ == "__main__":
    unittest.main()
