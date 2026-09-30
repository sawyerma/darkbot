"""market tests: Borsh decoding, tracker aggregates, exact cutoff, snapshot features, gap intervals, live == replay."""

from __future__ import annotations

import base64
import json
import struct
import unittest
from decimal import Decimal
from pathlib import Path
from typing import Any

import copy
import dataclasses

from darkbot import domain
from darkbot.clock import ReplayClock, ReplayScheduler
from darkbot.config import load_limits
from darkbot.domain import EventType, InsertOp, NormalizedEvent, SnapshotOrigin, UpdateOp
from darkbot.market import DEFAULT_PUBKEY, WSOL_MINT, ChainLogSource, MarketDataService, PumpEventDecoder, RawNotification, b58encode, event_discriminator

CONTRACT = domain.load_contract(Path(__file__)).document
LIMITS = load_limits(CONTRACT, chain_ws_url="wss://selftest.invalid")
PINNED_TAIL = LIMITS.create_event_tail_layout
LIMITS_PINNED = LIMITS
LIMITS_UNVERIFIED = dataclasses.replace(LIMITS, create_event_tail_layout=())
MS = domain.NANOSECONDS_PER_MILLISECOND
SOL = domain.LAMPORTS_PER_SOL
KEYS = {name: bytes([index]) * 32 for index, name in enumerate(("mint", "dev", "a", "b", "c", "curve", "quote"), start=1)}


def key(name: str) -> str:
    return b58encode(KEYS[name])


def _b58decode(text: str) -> bytes:
    alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    number = 0
    for char in text:
        number = number * 58 + alphabet.index(char)
    raw = number.to_bytes((number.bit_length() + 7) // 8, "big")
    return b"\0" * (len(text) - len(text.lstrip("1"))) + raw


def pack_string(text: str) -> bytes:
    raw = text.encode()
    return struct.pack("<I", len(raw)) + raw


def trade_line(mint: bytes, sol: int, tokens: int, is_buy: bool, user: bytes, vsol: int, vtok: int, real_tok: int = 700, tail: bytes = b"") -> str:
    body = mint + struct.pack("<QQ?", sol, tokens, is_buy) + user + struct.pack("<qQQQQ", 1, vsol, vtok, 5, real_tok)
    return "Program data: " + base64.b64encode(event_discriminator("TradeEvent") + body + tail).decode()


def create_line(mint: bytes, user: bytes, vtok: int, vsol: int, real: int, supply: int, quote: bytes | None = None) -> str:
    body = pack_string("Name") + pack_string("SYM") + pack_string("uri") + mint + KEYS["curve"] + user + user + struct.pack("<qQQQQ", 1, vtok, vsol, real, supply)
    if quote is not None:
        body += KEYS["quote"] + b"\0" + b"\0" + quote + struct.pack("<Q", vsol)
    return "Program data: " + base64.b64encode(event_discriminator("CreateEvent") + body).decode()


class FakeWriter:
    def __init__(self) -> None:
        self.ops: list[Any] = []
        self.committed_rows = self.dropped_envelopes = self.failures = 0

    def enqueue_bulk(self, ops: list[Any]) -> None:
        self.ops.extend(ops)

    def depth(self) -> tuple[int, int]:
        return 0, 0

    def rows(self, table: str) -> list[Any]:
        return [op.row for op in self.ops if isinstance(op, InsertOp) and domain.ROW_TABLE[type(op.row)] == table]

    def updates(self, table: str) -> list[UpdateOp]:
        return [op for op in self.ops if isinstance(op, UpdateOp) and op.table == table]


class FakeIds:
    def __init__(self) -> None:
        self.counters: dict[str, int] = {}

    def next(self, table: str) -> int:
        self.counters[table] = self.counters.get(table, 0) + 1
        return self.counters[table]


def make_service(origin: SnapshotOrigin = SnapshotOrigin.LIVE, writer: FakeWriter | None = None, snapshots: list | None = None, limits=LIMITS_PINNED) -> tuple[MarketDataService, ReplayClock, ReplayScheduler, FakeWriter]:
    clock = ReplayClock(started_at_monotonic_ns=10 * domain.NANOSECONDS_PER_SECOND, started_at_utc_ns=1_700_000_000 * domain.NANOSECONDS_PER_SECOND)
    scheduler = ReplayScheduler()
    writer = writer or FakeWriter()
    service = MarketDataService(
        CONTRACT, limits, clock, scheduler, writer, FakeIds(), "session-1", 1, "featureset-1", origin=origin, backtest_run_id=None if origin == SnapshotOrigin.LIVE else "run-1",
        on_snapshot=(lambda tracker, deadline, features: snapshots.append((tracker.mint, deadline, features))) if snapshots is not None else None,
    )
    return service, clock, scheduler, writer


def notification(seq: int, mono_ns: int, logs: list[str], signature: str | None = None, failed: bool = False) -> RawNotification:
    return RawNotification(seq, mono_ns, 1_700_000_000 * domain.NANOSECONDS_PER_SECOND + mono_ns, 100 + seq, signature or f"sig{seq}", failed, logs)


def feed(service: MarketDataService, clock: ReplayClock, scheduler: ReplayScheduler, seq: int, offset_ms: int, logs: list[str], signature: str | None = None) -> None:
    mono = clock.started_at_monotonic_ns + offset_ms * MS
    clock.advance_to(offset_ms * MS)
    scheduler.fire_due(mono)
    service.on_notification(notification(seq, mono, logs, signature))


class DecoderTests(unittest.TestCase):
    def test_trade_and_create_round_trip(self) -> None:
        decoder = PumpEventDecoder()
        events, unknown = decoder.decode_logs(["Program log: Instruction: Buy", trade_line(KEYS["mint"], 123, 456, True, KEYS["a"], 30 * SOL, 1_000_000, tail=b"\0" * 40), "Program 6EF8 success"])
        self.assertEqual((len(events), unknown), (1, []))
        trade = events[0].fields
        self.assertEqual((trade["mint"], trade["sol_amount"], trade["token_amount"], trade["is_buy"], trade["user"], trade["virtual_sol_reserves"], trade["virtual_token_reserves"], trade["tail_bytes"]), (key("mint"), 123, 456, True, key("a"), 30 * SOL, 1_000_000, 40))
        line = create_line(KEYS["mint"], KEYS["dev"], 1_073_000, 30 * SOL, 793_100, 1_000_000_000, quote=KEYS["quote"])
        created = decoder.decode_logs([line])[0][0].fields
        self.assertEqual((created["name"], created["symbol"], created["mint"], created["creator"], created["virtual_token_reserves"], created["token_total_supply"]), ("Name", "SYM", key("mint"), key("dev"), 1_073_000, 1_000_000_000))
        self.assertEqual((created["quote_mint"], created["tail_bytes"]), (None, 74), "unverified layout: the tail stays raw, the quote mint is not guessed")
        pinned = PumpEventDecoder(PINNED_TAIL).decode_logs([line])[0][0].fields
        self.assertEqual((pinned["is_mayhem_mode"], pinned["is_cashback_enabled"], pinned["quote_mint"], pinned["virtual_quote_reserves"], pinned["tail_bytes"], pinned["tail_base64"]), (False, False, key("quote"), 30 * SOL, 0, None))

    def test_unknown_and_broken_frames_are_counted_and_returned_raw(self) -> None:
        decoder = PumpEventDecoder()
        unknown = "Program data: " + base64.b64encode(event_discriminator("SomethingElse") + b"\0" * 8).decode()
        truncated = "Program data: " + base64.b64encode(event_discriminator("TradeEvent") + b"\0" * 10).decode()
        events, raw = decoder.decode_logs([unknown, truncated, "Program data: !!!"])
        self.assertEqual(events, [])
        self.assertEqual(raw, [unknown.split(": ", 1)[1], truncated.split(": ", 1)[1], "!!!"])
        self.assertEqual((decoder.unknown_discriminators, decoder.undecodable), (1, 2))

    def test_only_pump_owned_program_data_is_decoded_and_known_events_are_not_unknown(self) -> None:
        decoder = PumpEventDecoder(PINNED_TAIL, LIMITS.chain_program_id, LIMITS.known_pump_event_names)
        foreign_program = "11111111111111111111111111111111"
        foreign = "Program data: " + base64.b64encode(event_discriminator("ForeignEvent") + b"\1" * 8).decode()
        known = "Program data: " + base64.b64encode(event_discriminator("CompleteEvent") + b"\2" * 40).decode()
        trade = trade_line(KEYS["mint"], 1, 2, True, KEYS["a"], 3, 4)
        logs = [
            f"Program {LIMITS.chain_program_id} invoke [1]",
            f"Program {foreign_program} invoke [2]",
            foreign,
            f"Program {foreign_program} success",
            known,
            trade,
            f"Program {LIMITS.chain_program_id} success",
        ]
        events, unhandled = decoder.decode_logs(logs)
        self.assertEqual([event.kind for event in events], ["trade"])
        self.assertEqual(unhandled, [known.split(": ", 1)[1]])
        self.assertEqual((decoder.known_ignored_events, decoder.unknown_discriminators, decoder.undecodable), (1, 0, 0))

    def test_subscribe_frame_and_notification_parsing(self) -> None:
        source = ChainLogSource(LIMITS, ReplayClock(0, 0), lambda raw: None, lambda connected, ns: None)
        frame = json.loads(source.subscribe_frame())
        self.assertEqual(frame["params"], [{"mentions": [LIMITS.chain_program_id]}, {"commitment": LIMITS.chain_commitment}])
        text = json.dumps({"jsonrpc": "2.0", "method": "logsNotification", "params": {"result": {"context": {"slot": 7}, "value": {"signature": "s", "err": None, "logs": ["x"]}}, "subscription": 1}})
        raw = source.parse_frame(text, 5, 6)
        assert raw is not None
        self.assertEqual((raw.slot, raw.signature, raw.failed, raw.logs, raw.receive_sequence), (7, "s", False, ["x"], 1))
        self.assertIsNone(source.parse_frame('{"jsonrpc":"2.0","result":633,"id":1}', 5, 6))


class ServiceTests(unittest.TestCase):
    def launch(self, service: MarketDataService, clock: ReplayClock, scheduler: ReplayScheduler, quote: bytes | None = _b58decode(DEFAULT_PUBKEY)) -> None:
        create = create_line(KEYS["mint"], KEYS["dev"], 1_073_000_000, 30 * SOL, 793_100_000, 1_000_000_000, quote=quote)
        initial_buy = trade_line(KEYS["mint"], 1 * SOL, 34_000_000, True, KEYS["dev"], 31 * SOL, 1_039_000_000)
        feed(service, clock, scheduler, 1, 1000, [create, initial_buy], signature="sigCreate")

    def test_launch_trades_cutoff_and_snapshot(self) -> None:
        snapshots: list = []
        service, clock, scheduler, writer = make_service(snapshots=snapshots)
        self.launch(service, clock, scheduler)
        launch = writer.rows("launches")[0]
        self.assertEqual((launch.initial_virtual_sol_reserves, launch.initial_virtual_token_reserves, launch.initial_real_token_reserves, launch.initial_buy_token_base_units, launch.initial_buy_quote_lamports, launch.coverage_state), (30 * SOL, 1_073_000_000, 793_100_000, 34_000_000, 1 * SOL, "OBSERVED"))
        self.assertEqual((launch.quote_scope, launch.accepted, launch.acceptance_reason), ("SOL_WSOL", 1, "SOL_DEFAULT_QUOTE_PUMP"))
        create_event = writer.rows("events")[0]
        self.assertEqual((create_event.event_type, create_event.virtual_sol_reserves, create_event.virtual_token_reserves), ("CREATE", 31 * SOL, 1_039_000_000), "t0 state is the state after the initial buy")
        self.assertEqual(service.counters.trades_tracked, 0, "the initial buy is not part of E")
        # trades: a buys 0.5 SOL @1100ms, b buys 0.25 @1400ms, a sells 0.1 @1700ms, orphan mint @1750ms, c buys 2 @2100ms (after 1000ms deadline, before its timer)
        feed(service, clock, scheduler, 2, 1100, [trade_line(KEYS["mint"], SOL // 2, 15_000_000, True, KEYS["a"], 31 * SOL + SOL // 2, 1_024_000_000)])
        feed(service, clock, scheduler, 3, 1400, [trade_line(KEYS["mint"], SOL // 4, 7_000_000, True, KEYS["b"], 31 * SOL + 3 * SOL // 4, 1_017_000_000)])
        feed(service, clock, scheduler, 4, 1700, [trade_line(KEYS["mint"], SOL // 10, 3_000_000, False, KEYS["a"], 31 * SOL + 65 * SOL // 100, 1_020_000_000)])
        feed(service, clock, scheduler, 5, 1750, [trade_line(KEYS["b"], SOL, 1, True, KEYS["c"], 31 * SOL, 1_000_000_000)])
        self.assertEqual(service.counters.trades_orphan, 1)
        self.assertEqual([deadline for _, deadline, _ in snapshots], [1000 * MS + 500 * MS], "500 ms deadline fired by the scheduler before the 1400 ms trade")
        # event at 2100 ms arrives before the 1000 ms deadline timer fires (fire_due runs before on_notification in feed)
        mono = clock.started_at_monotonic_ns + 2100 * MS
        clock.advance_to(2100 * MS)
        service.on_notification(notification(6, mono, [trade_line(KEYS["mint"], 2 * SOL, 60_000_000, True, KEYS["c"], 33 * SOL + 65 * SOL // 100, 960_000_000)]))
        self.assertEqual(len(snapshots), 2, "exact cutoff: the 2000 ms snapshot was frozen before the later event was applied")
        _, _, features = snapshots[1]
        q = Decimal(1).scaleb(-int(CONTRACT["phase0_fixture_contract"]["ratio_decimal_places"]))
        price_t0 = (Decimal(31 * SOL) / Decimal(1_039_000_000)).quantize(q)
        price_t = (Decimal(31 * SOL + 65 * SOL // 100) / Decimal(1_020_000_000)).quantize(q)
        expected = {
            "buy_count": 2, "sell_count": 1, "buy_volume_sol": "0.75", "sell_volume_sol": "0.1", "net_buy_flow_sol": "0.65", "unique_buyers": 2,
            "unique_buyer_ratio": str(Decimal(1).quantize(q)), "largest_buy_sol": "0.5",
            "buyer_volume_concentration": str(((Decimal(SOL // 2) / Decimal(3 * SOL // 4)).quantize(q) ** 2 + (Decimal(SOL // 4) / Decimal(3 * SOL // 4)).quantize(q) ** 2).quantize(q)),
            "first5_buyer_concentration": str(Decimal(1).quantize(q)), "first_sell_latency_ms": "700", "price_change": str((price_t / price_t0 - 1).quantize(q)),
            "dev_initial_buy": str((Decimal(34_000_000) / Decimal(1_000_000_000)).quantize(q)),
            "curve_progress_pct": str((100 * (1 - Decimal(1_020_000_000 - (1_073_000_000 - 793_100_000)) / Decimal(793_100_000))).quantize(q)),
            "research_estimated_price_impact": None, "provider_gap_flag": False,
        }
        for name, value in expected.items():
            with self.subTest(feature=name):
                self.assertEqual(features[name], value)
        peak = max((Decimal(31 * SOL + SOL // 2) / Decimal(1_024_000_000)).quantize(q), (Decimal(31 * SOL + 3 * SOL // 4) / Decimal(1_017_000_000)).quantize(q), price_t0)
        self.assertEqual(features["max_drawdown"], str((1 - price_t / peak).quantize(q)))
        self.assertEqual(list(features), list(domain.feature_names(CONTRACT)))
        scheduler.fire_due(clock.started_at_monotonic_ns + 2100 * MS)
        self.assertEqual(len(snapshots), 2, "timer does not re-freeze a deadline frozen by the cutoff")
        self.assertEqual(len(writer.rows("feature_snapshots")), 2)
        self.assertEqual(writer.rows("feature_snapshots")[1].features_json, json.dumps({k: features[k] for k in sorted(features)}, separators=(",", ":"), ensure_ascii=False))
        # duplicate delivery of the same signature/index is ignored
        service.on_notification(notification(7, mono, [trade_line(KEYS["mint"], 2 * SOL, 60_000_000, True, KEYS["c"], 33 * SOL + 65 * SOL // 100, 960_000_000)], "sig6"))
        self.assertEqual(service.counters.trades_duplicate, 1)

    def test_quote_scope_decides_tradability_not_tracking(self) -> None:
        service, clock, scheduler, writer = make_service()
        self.launch(service, clock, scheduler)
        launch = writer.rows("launches")[0]
        self.assertEqual((launch.quote_scope, launch.accepted, launch.acceptance_reason, launch.quote_mint), ("SOL_WSOL", 1, "SOL_DEFAULT_QUOTE_PUMP", DEFAULT_PUBKEY))
        self.assertEqual(service.counters.launches_tradable, 1)
        other, oclock, osched, owriter = make_service()
        self.launch(other, oclock, osched, quote=KEYS["quote"])
        olaunch = owriter.rows("launches")[0]
        self.assertEqual((olaunch.quote_scope, olaunch.accepted), ("OBSERVE_ONLY", 0))
        self.assertTrue(olaunch.acceptance_reason.startswith("UNSUPPORTED_QUOTE_"))
        self.assertIn(key("mint"), other.trackers, "unsupported quote is still tracked for data")
        unverified, uclock, usched, uwriter = make_service(limits=LIMITS_UNVERIFIED)
        self.launch(unverified, uclock, usched, quote=None)
        ulaunch = uwriter.rows("launches")[0]
        self.assertEqual((ulaunch.quote_scope, ulaunch.accepted, ulaunch.acceptance_reason, ulaunch.quote_mint), ("OBSERVE_ONLY", 0, "QUOTE_UNKNOWN_LAYOUT_UNVERIFIED", None), "SOL-only rule: unknown quote is tracked but never tradable")
        self.assertIn(key("mint"), unverified.trackers)
        mismatch, mclock, msched, mwriter = make_service()
        self.launch(mismatch, mclock, msched, quote=None)  # pinned layout but the payload has no tail: decode error, raw payload kept
        self.assertEqual(mwriter.rows("launches"), [])
        self.assertEqual(mwriter.rows("provider_observations")[0].parse_status, "PARSE_ERROR")

    def test_unknown_program_data_is_stored_raw_not_dropped(self) -> None:
        service, clock, scheduler, writer = make_service()
        unknown = "Program data: " + base64.b64encode(event_discriminator("CompleteEvent") + b"\1" * 40).decode()
        feed(service, clock, scheduler, 1, 100, [unknown])
        observation = writer.rows("provider_observations")[0]
        self.assertEqual((observation.parse_status, service.counters.unknown_payload_observations), ("IGNORED", 1))
        self.assertEqual(json.loads(observation.redacted_payload_json)["unknown_program_data"], [unknown.split(": ", 1)[1]])
        self.assertIsNotNone(observation.parse_error)

    def test_gap_interval_eviction_and_shutdown(self) -> None:
        service, clock, scheduler, writer = make_service()
        self.launch(service, clock, scheduler)
        clock.advance_to(1200 * MS)
        service.on_connection(False, clock.monotonic_ns())
        interval = writer.rows("data_quality_intervals")[0]
        self.assertEqual((interval.reason, interval.end_offset_ns), ("RECONNECT", None))
        self.assertEqual(writer.updates("launches")[-1].values, {"coverage_state": "GAP", "data_complete": 0})
        clock.advance_to(1300 * MS)
        service.on_connection(True, clock.monotonic_ns())
        self.assertEqual(writer.updates("data_quality_intervals")[-1].values, {"end_offset_ns": 1300 * MS})
        feed(service, clock, scheduler, 2, 1600, [trade_line(KEYS["mint"], SOL, 1, True, KEYS["a"], 32 * SOL, 1_000_000_000)])
        scheduler.fire_due(clock.started_at_monotonic_ns + 2100 * MS)
        snapshot = writer.rows("feature_snapshots")[-1]
        self.assertEqual((snapshot.coverage_state, snapshot.data_complete), ("GAP", 0))
        self.assertTrue(json.loads(snapshot.features_json)["provider_gap_flag"])
        horizon_end = 1000 * MS + LIMITS.research_horizon_ms * MS
        scheduler.fire_due(clock.started_at_monotonic_ns + horizon_end)
        self.assertEqual(writer.updates("launches")[-1].values, {"status": "EVICTED"})
        self.assertEqual(service.trackers, {})
        service.close_all()
        reasons = [row.reason for row in writer.rows("data_quality_intervals")]
        self.assertIn("PROCESS_RESTART", reasons)

    def test_live_and_replay_snapshots_are_identical(self) -> None:
        live_snapshots: list = []
        live, clock, scheduler, writer = make_service(snapshots=live_snapshots)
        self.launch(live, clock, scheduler)
        feed(live, clock, scheduler, 2, 1100, [trade_line(KEYS["mint"], SOL // 2, 15_000_000, True, KEYS["a"], 31 * SOL + SOL // 2, 1_024_000_000)])
        feed(live, clock, scheduler, 3, 1700, [trade_line(KEYS["mint"], SOL // 10, 3_000_000, False, KEYS["a"], 31 * SOL + 4 * SOL // 10, 1_030_000_000)])
        scheduler.fire_due(clock.started_at_monotonic_ns + 2100 * MS)
        self.assertEqual(len(live_snapshots), 2)
        # replay: rebuild NormalizedEvents from the stored rows (what replay.py will do) and feed a REPLAY service
        replay_snapshots: list = []
        replay, rclock, rscheduler, rwriter = make_service(SnapshotOrigin.REPLAY, snapshots=replay_snapshots)
        launch_row = writer.rows("launches")[0]
        for row in writer.rows("events"):
            event = NormalizedEvent(EventType(row.event_type), row.mint, row.transaction_signature, row.provider_event_index, row.actor, row.quote_lamports, row.token_base_units, row.virtual_sol_reserves, row.virtual_token_reserves, None, row.receive_offset_ns_from_session_start, row.utc_received_ns, row.chain_slot, row.provider_receive_sequence, name=launch_row.name, symbol=launch_row.symbol, creator=launch_row.creator, bonding_curve=launch_row.curve_address_provider, token_total_supply=1_000_000_000)
            rclock.advance_to(row.receive_offset_ns_from_session_start)
            rscheduler.fire_due(rclock.monotonic_ns())
            if event.event_type == EventType.CREATE:
                replay.on_normalized_event(event, launch_id=launch_row.id, initial_reserves=(launch_row.initial_virtual_sol_reserves, launch_row.initial_virtual_token_reserves, launch_row.initial_real_token_reserves))
            else:
                replay.on_normalized_event(event)
        rscheduler.fire_due(rclock.started_at_monotonic_ns + 2100 * MS)
        self.assertEqual([(m, d, f) for m, d, f in replay_snapshots], live_snapshots)
        self.assertEqual([row.features_json for row in rwriter.rows("feature_snapshots")], [row.features_json for row in writer.rows("feature_snapshots")])
        self.assertEqual(rwriter.rows("feature_snapshots")[0].computed_in, "REPLAY")
        self.assertEqual(rwriter.rows("events"), [], "replay never rewrites events")


if __name__ == "__main__":
    unittest.main()
