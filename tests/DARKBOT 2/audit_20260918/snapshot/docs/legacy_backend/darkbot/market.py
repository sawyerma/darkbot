"""Market data (LEAN_V1 sections 3, 4, 7): chain stream -> decoded events -> RAM trackers -> SQLite rows.

Components: ChainLogSource (logsSubscribe on the Pump program, reconnect with backoff),
PumpEventDecoder (Borsh events from "Program data:" log lines), PumpPortalCreateSource
(optional cross-check of creates), TokenTracker (O(1) aggregates), DeadlineHeap (one timer),
MarketDataService (launch lifecycle, exact cutoff, coverage and gap intervals, eviction,
health sampler). MarketDataService is shared by live and replay: replay feeds
on_normalized_event with a ReplayClock/ReplayScheduler.
"""

from __future__ import annotations

import asyncio
import base64
import binascii
import hashlib
import heapq
import json
import struct
from dataclasses import dataclass, field
from decimal import ROUND_HALF_EVEN, Decimal
from typing import Any, Callable

from darkbot.cli import canonical_json, sha256_json
from darkbot.clock import Clock, Scheduler
from darkbot.config import Limits
from darkbot.domain import (
    LAMPORTS_PER_SOL,
    NANOSECONDS_PER_MILLISECOND,
    NANOSECONDS_PER_SECOND,
    CoverageState,
    DataQualityIntervalRow,
    DataQualityReason,
    EventRow,
    EventType,
    FeatureSnapshotRow,
    InsertOp,
    LaunchRow,
    LaunchStatus,
    NormalizedEvent,
    ParseStatus,
    ProviderObservationRow,
    QuoteScope,
    Severity,
    SnapshotOrigin,
    SystemEventRow,
    UpdateOp,
    WriteOp,
    new_text_id,
)
from darkbot.storage import IdAllocator, StorageWriter
from darkbot.strategy import FeatureCalculator

PROGRAM_DATA_PREFIX = "Program data: "
WSOL_MINT = "So11111111111111111111111111111111111111112"
DEFAULT_PUBKEY = "11111111111111111111111111111111"
PUBKEY_BYTES = 32
DISCRIMINATOR_BYTES = 8
FLAG_GAP, FLAG_AMBIGUOUS, FLAG_LATE = "GAP", "AMBIGUOUS_EVENT", "LATE_EVENT"
PURPOSE_SNAPSHOT, PURPOSE_HORIZON_END = "SNAPSHOT", "HORIZON_END"
_B58_ALPHABET = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def b58encode(raw: bytes) -> str:
    number = int.from_bytes(raw, "big")
    encoded = bytearray()
    while number > 0:
        number, remainder = divmod(number, 58)
        encoded.append(_B58_ALPHABET[remainder])
    leading = len(raw) - len(raw.lstrip(b"\0"))
    return (b"1" * leading + bytes(reversed(encoded))).decode("ascii")


def event_discriminator(name: str) -> bytes:
    return hashlib.sha256(f"event:{name}".encode("ascii")).digest()[:DISCRIMINATOR_BYTES]


# --------------------------------------------------------------------------- decoder


@dataclass(frozen=True, slots=True)
class DecodedEvent:
    kind: str  # "create" | "trade"
    log_index: int
    raw_base64: str
    fields: dict[str, Any]


class _Reader:
    def __init__(self, data: bytes) -> None:
        self.data = data
        self.pos = 0

    def remaining(self) -> int:
        return len(self.data) - self.pos

    def take(self, count: int) -> bytes:
        if self.remaining() < count:
            raise ValueError("borsh payload truncated")
        chunk = self.data[self.pos : self.pos + count]
        self.pos += count
        return chunk

    def u64(self) -> int:
        return struct.unpack("<Q", self.take(8))[0]

    def i64(self) -> int:
        return struct.unpack("<q", self.take(8))[0]

    def boolean(self) -> bool:
        return self.take(1) != b"\0"

    def pubkey(self) -> str:
        return b58encode(self.take(PUBKEY_BYTES))

    def string(self) -> str:
        length = struct.unpack("<I", self.take(4))[0]
        return self.take(length).decode("utf-8", errors="replace")


class PumpEventDecoder:
    """Anchor events of the Pump program: discriminator = sha256("event:<Name>")[:8], then Borsh fields in IDL order.

    Only the leading fields the tracker needs are parsed; newer IDL versions append fields,
    which are reported as tail bytes and stored raw for later verification, never guessed.
    """

    def __init__(
        self,
        create_tail_layout: tuple[tuple[str, str], ...] = (),
        program_id: str | None = None,
        known_event_names: tuple[str, ...] = (),
    ) -> None:
        self.trade_discriminator = event_discriminator("TradeEvent")
        self.create_discriminator = event_discriminator("CreateEvent")
        self.create_tail_layout = create_tail_layout
        self.program_id = program_id
        self.known_event_discriminators = {event_discriminator(name) for name in known_event_names}
        self.undecodable = 0
        self.unknown_discriminators = 0
        self.known_ignored_events = 0

    def decode_logs(self, logs: list[str]) -> tuple[list[DecodedEvent], list[str]]:
        """Decode Pump-owned Program-data frames and return raw unhandled Pump frames for audit.

        A logsSubscribe transaction can contain CPI logs from several programs. Program-data lines
        do not carry the emitting program id, so the invocation stack is tracked and only frames
        emitted while the Pump program is active are interpreted as Pump events. Synthetic tests
        without invocation markers remain supported.
        """
        events: list[DecodedEvent] = []
        unhandled: list[str] = []
        program_stack: list[str] = []
        saw_program_context = False
        for index, line in enumerate(logs):
            if line.startswith("Program "):
                parts = line.split()
                if len(parts) >= 3 and parts[2] == "invoke":
                    saw_program_context = True
                    program_stack.append(parts[1])
                    continue
                if len(parts) >= 3 and (parts[2] == "success" or parts[2] == "failed:"):
                    program_id = parts[1]
                    if program_stack and program_stack[-1] == program_id:
                        program_stack.pop()
                    elif program_id in program_stack:
                        reverse_index = program_stack[::-1].index(program_id)
                        del program_stack[len(program_stack) - reverse_index - 1 :]
                    continue
            if not line.startswith(PROGRAM_DATA_PREFIX):
                continue
            if saw_program_context and (not program_stack or program_stack[-1] != self.program_id):
                continue
            encoded = line[len(PROGRAM_DATA_PREFIX) :].strip()
            try:
                data = base64.b64decode(encoded, validate=True)
            except (binascii.Error, ValueError):
                self.undecodable += 1
                unhandled.append(encoded)
                continue
            if len(data) < DISCRIMINATOR_BYTES:
                self.undecodable += 1
                unhandled.append(encoded)
                continue
            discriminator, payload = data[:DISCRIMINATOR_BYTES], data[DISCRIMINATOR_BYTES:]
            try:
                if discriminator == self.trade_discriminator:
                    events.append(DecodedEvent("trade", index, encoded, self.decode_trade(payload)))
                elif discriminator == self.create_discriminator:
                    events.append(DecodedEvent("create", index, encoded, self.decode_create(payload, self.create_tail_layout)))
                elif discriminator in self.known_event_discriminators:
                    self.known_ignored_events += 1
                    unhandled.append(encoded)
                else:
                    self.unknown_discriminators += 1
                    unhandled.append(encoded)
            except ValueError:
                self.undecodable += 1
                unhandled.append(encoded)
        return events, unhandled

    @staticmethod
    def decode_trade(payload: bytes) -> dict[str, Any]:
        reader = _Reader(payload)
        fields = {
            "mint": reader.pubkey(),
            "sol_amount": reader.u64(),
            "token_amount": reader.u64(),
            "is_buy": reader.boolean(),
            "user": reader.pubkey(),
            "timestamp": reader.i64(),
            "virtual_sol_reserves": reader.u64(),
            "virtual_token_reserves": reader.u64(),
            "real_sol_reserves": reader.u64(),
            "real_token_reserves": reader.u64(),
        }
        fields["tail_bytes"] = reader.remaining()
        return fields

    @staticmethod
    def decode_create(payload: bytes, tail_layout: tuple[tuple[str, str], ...] = ()) -> dict[str, Any]:
        """Known prefix through token_total_supply; the tail is parsed only by the pinned layout, never guessed.

        With an empty layout the tail stays raw (`tail_bytes`, `tail_base64`) and `quote_mint` stays None, which the
        service maps to OBSERVE_ONLY. The first real capture reveals the tail; pinning it is a contract change.
        """
        reader = _Reader(payload)
        fields: dict[str, Any] = {
            "name": reader.string(),
            "symbol": reader.string(),
            "uri": reader.string(),
            "mint": reader.pubkey(),
            "bonding_curve": reader.pubkey(),
            "user": reader.pubkey(),
            "creator": None,
            "timestamp": None,
            "virtual_token_reserves": None,
            "virtual_sol_reserves": None,
            "real_token_reserves": None,
            "token_total_supply": None,
            "quote_mint": None,
        }
        if reader.remaining() >= PUBKEY_BYTES + 8 + 4 * 8:
            fields["creator"] = reader.pubkey()
            fields["timestamp"] = reader.i64()
            fields["virtual_token_reserves"] = reader.u64()
            fields["virtual_sol_reserves"] = reader.u64()
            fields["real_token_reserves"] = reader.u64()
            fields["token_total_supply"] = reader.u64()
        for name, kind in tail_layout:
            fields[name] = _read_typed(reader, kind)
        fields["tail_bytes"] = reader.remaining()
        fields["tail_base64"] = base64.b64encode(reader.take(reader.remaining())).decode() if reader.remaining() else None
        return fields


def _read_typed(reader: _Reader, kind: str) -> Any:
    if kind == "bool":
        return reader.boolean()
    if kind == "u64":
        return reader.u64()
    if kind == "i64":
        return reader.i64()
    if kind == "pubkey":
        return reader.pubkey()
    if kind == "string":
        return reader.string()
    raise ValueError(f"unsupported tail field type {kind!r}")


# --------------------------------------------------------------------------- sources


@dataclass(frozen=True, slots=True)
class RawNotification:
    receive_sequence: int
    received_monotonic_ns: int
    utc_received_ns: int
    slot: int | None
    signature: str
    failed: bool
    logs: list[str]


class ChainLogSource:
    """Solana logsSubscribe (mentions = Pump program). Push only; reconnect with exponential backoff (one-shot timer)."""

    def __init__(self, limits: Limits, clock: Clock, on_notification: Callable[[RawNotification], None], on_connection: Callable[[bool, int], None]) -> None:
        self._limits = limits
        self._clock = clock
        self._on_notification = on_notification
        self._on_connection = on_connection
        self._sequence = 0
        self._stopping = False
        self._ws: Any = None

    def subscribe_frame(self) -> str:
        return json.dumps({"jsonrpc": "2.0", "id": 1, "method": "logsSubscribe", "params": [{"mentions": [self._limits.chain_program_id]}, {"commitment": self._limits.chain_commitment}]})

    def parse_frame(self, text: str, monotonic_ns: int, utc_ns: int) -> RawNotification | None:
        try:
            message = json.loads(text)
        except json.JSONDecodeError:
            return None
        if not isinstance(message, dict) or message.get("method") != "logsNotification":
            return None
        try:
            result = message["params"]["result"]
            value = result["value"]
            self._sequence += 1
            return RawNotification(self._sequence, monotonic_ns, utc_ns, result.get("context", {}).get("slot"), str(value["signature"]), value.get("err") is not None, list(value.get("logs") or []))
        except (KeyError, TypeError):
            return None

    async def run(self) -> None:
        import websockets

        backoff_ms = self._limits.reconnect_backoff_initial_ms
        while not self._stopping:
            try:
                async with websockets.connect(self._limits.chain_ws_url, ping_interval=self._limits.websocket_ping_interval_ms / 1000, max_size=self._limits.websocket_max_message_bytes) as ws:
                    self._ws = ws
                    await ws.send(self.subscribe_frame())
                    self._on_connection(True, self._clock.monotonic_ns())
                    backoff_ms = self._limits.reconnect_backoff_initial_ms
                    async for raw in ws:
                        text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
                        notification = self.parse_frame(text, self._clock.monotonic_ns(), self._clock.utc_ns())
                        if notification is not None:
                            self._on_notification(notification)
            except asyncio.CancelledError:
                raise
            except Exception:  # noqa: BLE001 - any transport failure is a disconnect; the interval records it
                if self._stopping:
                    break
            self._ws = None
            self._on_connection(False, self._clock.monotonic_ns())
            if self._stopping:
                break
            await asyncio.sleep(backoff_ms / 1000)
            backoff_ms = min(backoff_ms * 2, self._limits.reconnect_backoff_max_ms)

    async def close(self) -> None:
        self._stopping = True
        if self._ws is not None:
            await self._ws.close()


class PumpPortalCreateSource:
    """Optional free cross-check: PumpPortal subscribeNewToken; only creates, only counters (never a paid subscription)."""

    def __init__(self, limits: Limits, clock: Clock, on_create: Callable[[str, int], None]) -> None:
        self._limits = limits
        self._clock = clock
        self._on_create = on_create
        self._stopping = False
        self._ws: Any = None

    async def run(self) -> None:
        import websockets

        assert self._limits.pumpportal_ws_url is not None
        backoff_ms = self._limits.reconnect_backoff_initial_ms
        while not self._stopping:
            try:
                async with websockets.connect(self._limits.pumpportal_ws_url, ping_interval=self._limits.websocket_ping_interval_ms / 1000) as ws:
                    self._ws = ws
                    await ws.send(json.dumps({"method": "subscribeNewToken"}))
                    backoff_ms = self._limits.reconnect_backoff_initial_ms
                    async for raw in ws:
                        text = raw.decode("utf-8") if isinstance(raw, bytes) else raw
                        try:
                            message = json.loads(text)
                        except json.JSONDecodeError:
                            continue
                        if isinstance(message, dict) and message.get("txType") == "create" and isinstance(message.get("mint"), str):
                            self._on_create(message["mint"], self._clock.monotonic_ns())
            except asyncio.CancelledError:
                raise
            except Exception:  # noqa: BLE001
                if self._stopping:
                    break
            self._ws = None
            if self._stopping:
                break
            await asyncio.sleep(backoff_ms / 1000)
            backoff_ms = min(backoff_ms * 2, self._limits.reconnect_backoff_max_ms)

    async def close(self) -> None:
        self._stopping = True
        if self._ws is not None:
            await self._ws.close()


# --------------------------------------------------------------------------- tracker


@dataclass
class TokenTracker:
    """Per-launch RAM state; every field is updated in O(1) per event (contract runtime_model.feature_computation)."""

    launch_id: str
    mint: str
    t0_offset_ns: int
    deadlines_ns: tuple[int, ...]
    horizon_end_ns: int
    quantum: Decimal
    t0_vsol: int | None
    t0_vtok: int | None
    initial_vsol: int | None
    initial_vtok: int | None
    initial_real_tok: int | None
    token_total_supply: int | None
    initial_buy_tokens: int | None
    initial_buy_lamports: int | None
    tradable: bool = False
    creator_fee_enabled: bool = False
    creator: str | None = None
    last_real_tok: int | None = None
    last_signature: str = ""
    last_slot: int | None = None
    last_receive_sequence: int = 0
    last_state_offset_ns: int = 0
    coverage_state: CoverageState = CoverageState.OBSERVED
    coverage_start_ns: int | None = None
    status: LaunchStatus = LaunchStatus.OBSERVING
    frozen: dict[int, int] = field(default_factory=dict)  # deadline offset -> snapshot id
    buy_count: int = 0
    sell_count: int = 0
    buy_lamports: int = 0
    sell_lamports: int = 0
    per_buyer: dict[str, int] = field(default_factory=dict)
    first_buyers: list[str] = field(default_factory=list)
    largest_buy_lamports: int = 0
    first_sell_offset_ns: int | None = None
    creator_sell_offset_ns: int | None = None
    first_trade_offset_ns: int | None = None
    last_vsol: int | None = None
    last_vtok: int | None = None
    price_peak: Decimal | None = None
    max_drawdown: Decimal = Decimal(0)
    event_count: int = 0
    last_event_offset_ns: int = 0
    seen_keys: set[str] = field(default_factory=set)
    flags: set[str] = field(default_factory=set)
    first_buyer_window: int = 5

    def __post_init__(self) -> None:
        self.last_vsol, self.last_vtok = self.t0_vsol, self.t0_vtok
        if self.last_real_tok is None:
            self.last_real_tok = self.initial_real_tok
        if self.last_state_offset_ns == 0:
            self.last_state_offset_ns = self.t0_offset_ns
        self.price_peak = self._price(self.t0_vsol, self.t0_vtok)

    def _price(self, vsol: int | None, vtok: int | None) -> Decimal | None:
        if vsol is None or vtok is None or vsol <= 0 or vtok <= 0:
            return None
        return (Decimal(vsol) / Decimal(vtok)).quantize(self.quantum, rounding=ROUND_HALF_EVEN)

    def apply(self, event: NormalizedEvent) -> bool:
        """Apply one BUY/SELL; returns False for duplicates (same provider_event_key)."""
        key = event.provider_event_key
        if key in self.seen_keys:
            return False
        self.seen_keys.add(key)
        self.event_count += 1
        self.last_event_offset_ns = event.receive_offset_ns
        if self.first_trade_offset_ns is None:
            self.first_trade_offset_ns = event.receive_offset_ns
        lamports = event.quote_lamports or 0
        actor = event.actor or ""
        if event.event_type == EventType.BUY:
            self.buy_count += 1
            self.buy_lamports += lamports
            self.per_buyer[actor] = self.per_buyer.get(actor, 0) + lamports
            if actor not in self.first_buyers and len(self.first_buyers) < self.first_buyer_window:
                self.first_buyers.append(actor)
            self.largest_buy_lamports = max(self.largest_buy_lamports, lamports)
        elif event.event_type == EventType.SELL:
            self.sell_count += 1
            self.sell_lamports += lamports
            if self.first_sell_offset_ns is None:
                self.first_sell_offset_ns = event.receive_offset_ns
            if (
                self.creator is not None
                and actor == self.creator
                and self.creator_sell_offset_ns is None
            ):
                self.creator_sell_offset_ns = event.receive_offset_ns
        if event.virtual_sol_reserves is not None and event.virtual_token_reserves is not None:
            self.last_vsol, self.last_vtok = event.virtual_sol_reserves, event.virtual_token_reserves
            if event.real_token_reserves is not None:
                self.last_real_tok = event.real_token_reserves
            self.last_signature = event.signature
            self.last_slot = event.chain_slot
            self.last_receive_sequence = event.receive_sequence
            self.last_state_offset_ns = event.receive_offset_ns
            price = self._price(self.last_vsol, self.last_vtok)
            if price is not None:
                self.price_peak = price if self.price_peak is None else max(self.price_peak, price)
                drawdown = (1 - price / self.price_peak).quantize(self.quantum, rounding=ROUND_HALF_EVEN)
                self.max_drawdown = max(self.max_drawdown, drawdown)
        return True


# --------------------------------------------------------------------------- deadline heap


class DeadlineHeap:
    """One heap, at most one pending timer; entries (fire_offset_ns, sequence, launch_id, purpose)."""

    def __init__(self, scheduler: Scheduler, started_at_monotonic_ns: int, dispatch: Callable[[str, str, int], None]) -> None:
        self._scheduler = scheduler
        self._base = started_at_monotonic_ns
        self._dispatch = dispatch
        self._heap: list[tuple[int, int, str, str]] = []
        self._sequence = 0
        self._handle: object | None = None
        self._armed_for: int | None = None

    def push(self, fire_offset_ns: int, launch_id: str, purpose: str) -> None:
        self._sequence += 1
        heapq.heappush(self._heap, (fire_offset_ns, self._sequence, launch_id, purpose))
        self._arm()

    def _arm(self) -> None:
        if not self._heap:
            return
        earliest = self._heap[0][0]
        if self._armed_for is not None and self._armed_for <= earliest:
            return
        if self._handle is not None:
            self._scheduler.cancel(self._handle)
        self._armed_for = earliest
        self._handle = self._scheduler.schedule_at(self._base + earliest, self._on_fire)

    def _on_fire(self, actual_monotonic_ns: int) -> None:
        self._handle, self._armed_for = None, None
        now_offset = actual_monotonic_ns - self._base
        while self._heap and self._heap[0][0] <= now_offset:
            fire_offset, _, launch_id, purpose = heapq.heappop(self._heap)
            self._dispatch(purpose, launch_id, fire_offset)
        self._arm()

    def __len__(self) -> int:
        return len(self._heap)


# --------------------------------------------------------------------------- service


@dataclass
class Counters:
    notifications: int = 0
    failed_transactions: int = 0
    decoded_events: int = 0
    creates: int = 0
    launches_tracked: int = 0
    launches_tradable: int = 0
    launches_unsupported_quote: int = 0
    launches_quote_unknown: int = 0
    launches_rejected_limit: int = 0
    unknown_payload_observations: int = 0
    trades_tracked: int = 0
    trades_orphan: int = 0
    trades_duplicate: int = 0
    trades_late: int = 0
    snapshots: int = 0
    evictions: int = 0
    pumpportal_creates: int = 0
    pumpportal_before_chain: int = 0
    pumpportal_after_chain: int = 0
    pumpportal_lag_ns_total: int = 0

    def as_dict(self) -> dict[str, int]:
        return dict(self.__dict__)


class MarketDataService:
    def __init__(
        self,
        contract: dict[str, Any],
        limits: Limits,
        clock: Clock,
        scheduler: Scheduler,
        writer: StorageWriter | None,
        ids: IdAllocator | None,
        session_id: str,
        session_sequence: int,
        feature_set_model_id: str,
        origin: SnapshotOrigin = SnapshotOrigin.LIVE,
        backtest_run_id: str | None = None,
        research_quote_lamports: int | None = None,
        on_snapshot: Callable[[TokenTracker, int, dict[str, Any]], None] | None = None,
        on_curve_state: Callable[[TokenTracker], None] | None = None,
        on_launch: Callable[[TokenTracker, dict[str, Any]], None] | None = None,
    ) -> None:
        self.contract = contract
        self.limits = limits
        self.clock = clock
        self.writer = writer
        self.ids = ids
        self.session_id = session_id
        self.session_sequence = session_sequence
        self.feature_set_model_id = feature_set_model_id
        self.origin = origin
        self.backtest_run_id = backtest_run_id
        self.on_snapshot = on_snapshot
        self.on_curve_state = on_curve_state
        self.on_launch = on_launch
        self.features = FeatureCalculator(contract, research_quote_lamports)
        self.decoder = PumpEventDecoder(limits.create_event_tail_layout, limits.chain_program_id, limits.known_pump_event_names)
        self.quantum = self.features.quantum
        self.trackers: dict[str, TokenTracker] = {}
        self._by_launch: dict[str, TokenTracker] = {}
        self.heap = DeadlineHeap(scheduler, clock.started_at_monotonic_ns, self._dispatch)
        self.counters = Counters()
        self.open_intervals: dict[DataQualityReason, int] = {}
        self.last_message_offset_ns: int | None = None
        self.accepting = True
        self._pumpportal_recent: dict[str, int] = {}
        self._chain_recent: dict[str, int] = {}
        self._health_handle: object | None = None
        self._scheduler = scheduler
        self.snapshot_ms = limits.runtime_snapshot_ms
        self.horizon_ns = limits.research_horizon_ms * NANOSECONDS_PER_MILLISECOND

    # ------------------------------------------------------------------ writing
    def _write(self, ops: list[WriteOp]) -> None:
        if self.writer is not None and ops:
            self.writer.enqueue_bulk(ops)

    def system_event(self, event_type: str, severity: Severity, details: dict[str, Any]) -> None:
        if self.writer is None or self.ids is None:
            return
        row = SystemEventRow(self.ids.next("system_events"), self.session_id, event_type, severity.value, self.clock.utc_ns(), canonical_json(details))
        self._write([InsertOp(row)])

    # ------------------------------------------------------------------ live input
    def on_notification(self, raw: RawNotification) -> None:
        self.counters.notifications += 1
        self.last_message_offset_ns = self.clock.offset_ns(raw.received_monotonic_ns)
        self._close_interval(DataQualityReason.STALE)
        if raw.failed:
            self.counters.failed_transactions += 1
            return
        undecodable_before = self.decoder.undecodable
        decoded, unknown = self.decoder.decode_logs(raw.logs)
        offset = self.clock.offset_ns(raw.received_monotonic_ns)
        self.counters.decoded_events += len(decoded)
        creates = [item for item in decoded if item.kind == "create"]
        trades = [item for item in decoded if item.kind == "trade"]
        relevant = bool(creates) or any(item.fields["mint"] in self.trackers for item in trades)
        if not relevant:
            self.counters.trades_orphan += len(trades)
            if unknown:  # unknown discriminators or broken frames are kept raw for later verification, never dropped silently
                status = ParseStatus.PARSE_ERROR if self.decoder.undecodable > undecodable_before else ParseStatus.IGNORED
                self._observation(raw, decoded, offset, unknown, status)
                self.counters.unknown_payload_observations += 1
            return
        observation_id = self._observation(raw, decoded, offset, unknown, ParseStatus.PARSED)
        for create in creates:
            initial_buy = next((t for t in trades if t.fields["mint"] == create.fields["mint"] and t.fields["is_buy"]), None)
            self._on_chain_create(create, initial_buy, raw, offset, observation_id)
        for trade in trades:
            if any(trade.fields["mint"] == create.fields["mint"] for create in creates):
                continue  # initial buy of the create transaction: visible only through the launch row
            event = self._normalize_trade(trade, raw, offset)
            if event.mint in self.trackers:
                self.on_normalized_event(event, observation_id=observation_id)
            else:
                self.counters.trades_orphan += 1

    def _observation(self, raw: RawNotification, decoded: list[DecodedEvent], offset: int, unknown: list[str], status: ParseStatus) -> int:
        payload = {"signature": raw.signature, "slot": raw.slot, "program_data": [item.raw_base64 for item in decoded], "unknown_program_data": unknown}
        observation_id = self.ids.next("provider_observations") if self.ids is not None else 0
        error = None if status == ParseStatus.PARSED else f"{len(unknown)} program-data frame(s) not decodable with the pinned layouts"
        row = ProviderObservationRow(
            observation_id, self.session_id, self.session_sequence, raw.receive_sequence, raw.received_monotonic_ns, offset, raw.utc_received_ns, raw.slot,
            sha256_json(payload), canonical_json(payload), status.value, error,
        )
        self._write([InsertOp(row)])
        return observation_id

    def _normalize_trade(self, trade: DecodedEvent, raw: RawNotification, offset: int) -> NormalizedEvent:
        f = trade.fields
        return NormalizedEvent(
            EventType.BUY if f["is_buy"] else EventType.SELL, f["mint"], raw.signature, trade.log_index, f["user"], int(f["sol_amount"]), int(f["token_amount"]),
            int(f["virtual_sol_reserves"]), int(f["virtual_token_reserves"]), int(f["real_token_reserves"]), offset, raw.utc_received_ns, raw.slot, raw.receive_sequence,
        )

    def _on_chain_create(self, create: DecodedEvent, initial_buy: DecodedEvent | None, raw: RawNotification, offset: int, observation_id: int) -> None:
        self.counters.creates += 1
        f = create.fields
        mint = f["mint"]
        self._note_chain_create(mint, raw.received_monotonic_ns)
        if mint in self.trackers or not self.accepting:
            return
        quote_mint = f.get("quote_mint")
        buy = initial_buy.fields if initial_buy is not None else None
        t0_vsol = int(buy["virtual_sol_reserves"]) if buy else f.get("virtual_sol_reserves")
        t0_vtok = int(buy["virtual_token_reserves"]) if buy else f.get("virtual_token_reserves")
        event = NormalizedEvent(
            EventType.CREATE, mint, raw.signature, create.log_index, f["user"], int(buy["sol_amount"]) if buy else None, int(buy["token_amount"]) if buy else None,
            t0_vsol, t0_vtok, int(buy["real_token_reserves"]) if buy else f.get("real_token_reserves"), offset, raw.utc_received_ns, raw.slot, raw.receive_sequence,
            name=f["name"], symbol=f["symbol"], creator=f.get("creator") or f["user"], bonding_curve=f["bonding_curve"], token_total_supply=f.get("token_total_supply"),
        )
        self._register_launch(event, observation_id, quote_mint, f.get("virtual_sol_reserves"), f.get("virtual_token_reserves"), f.get("real_token_reserves"))

    # ------------------------------------------------------------------ shared input (live and replay)
    def on_normalized_event(
        self,
        event: NormalizedEvent,
        observation_id: int | None = None,
        launch_id: str | None = None,
        initial_reserves: tuple[int | None, int | None, int | None] | None = None,
        quote_mint: str | None = None,
    ) -> None:
        """Shared event input for live normalization and offline replay.

        Replay passes the persisted launch id, pre-buy reserves and quote mint for CREATE;
        BUY/SELL require no replay-only state.  The tracker and feature path remain identical.
        """
        if event.event_type == EventType.CREATE:
            if event.mint not in self.trackers:
                vsol, vtok, real = initial_reserves if initial_reserves is not None else (None, None, event.real_token_reserves)
                self._register_launch(event, observation_id or 0, quote_mint, vsol, vtok, real, launch_id=launch_id)
            return
        tracker = self.trackers.get(event.mint)
        if tracker is None:
            self.counters.trades_orphan += 1
            return
        if event.receive_offset_ns > tracker.horizon_end_ns:
            self.counters.trades_late += 1
            return
        for deadline in tracker.deadlines_ns:  # exact cutoff: freeze before applying a later event
            if deadline not in tracker.frozen and event.receive_offset_ns > deadline:
                self.freeze_snapshot(tracker, deadline)
        if not tracker.apply(event):
            self.counters.trades_duplicate += 1
            return
        self.counters.trades_tracked += 1
        if self.origin == SnapshotOrigin.LIVE and self.ids is not None:
            self._write(self._event_ops(event, tracker, observation_id or 0))
            if tracker.first_trade_offset_ns == event.receive_offset_ns:
                self._write([UpdateOp("launches", {"id": tracker.launch_id}, {"first_trade_received_offset_ns": event.receive_offset_ns})])
        if self.on_curve_state is not None:
            self.on_curve_state(tracker)

    def _register_launch(self, event: NormalizedEvent, observation_id: int, quote_mint: str | None, initial_vsol: int | None, initial_vtok: int | None, initial_real: int | None, launch_id: str | None = None) -> None:
        """Every Pump launch is tracked (data); only SOL/WSOL-quoted launches are tradable (LEAN_V1 rule: SOL pairs only).

        quote_mint None = layout unverified -> OBSERVE_ONLY; Pubkey::default() is the on-chain SOL quote sentinel; another quote is unsupported in V1.
        """
        if len(self.trackers) >= self.limits.tracker_limit:
            self.counters.launches_rejected_limit += 1
            self.system_event("TRACKER_LIMIT", Severity.WARN, {"code": "TRACKER_LIMIT", "message": "launch not tracked", "mint": event.mint})
            return
        if quote_mint == DEFAULT_PUBKEY:
            scope, reason = QuoteScope.SOL_WSOL, "SOL_DEFAULT_QUOTE_PUMP"
            self.counters.launches_tradable += 1
        elif quote_mint is None:
            scope, reason = QuoteScope.OBSERVE_ONLY, "QUOTE_UNKNOWN_LAYOUT_UNVERIFIED"
            self.counters.launches_quote_unknown += 1
        else:
            scope, reason = QuoteScope.OBSERVE_ONLY, f"UNSUPPORTED_QUOTE_{quote_mint}"
            self.counters.launches_unsupported_quote += 1
        tradable = scope == QuoteScope.SOL_WSOL
        launch_id = launch_id or new_text_id()
        t0 = event.receive_offset_ns
        gap_open = bool(self.open_intervals)
        coverage = CoverageState.GAP if gap_open else CoverageState.OBSERVED
        if self.origin == SnapshotOrigin.LIVE and self.ids is not None:
            row = LaunchRow(
                launch_id, self.session_id, event.mint, event.symbol, event.name, event.creator, "pump", quote_mint, scope.value, LaunchStatus.OBSERVING.value,
                observation_id, t0, int(tradable), reason, None, event.bonding_curve, None,
                initial_vsol, initial_vtok, initial_real, event.token_base_units, event.quote_lamports, self.limits.research_horizon_ms, "FULL",
                None, None, None, t0, coverage.value, 0, None, int(not gap_open),
            )
            self._write([InsertOp(row), InsertOp(self._event_row(event, launch_id, observation_id))])
        self.counters.launches_tracked += 1
        tracker = TokenTracker(
            launch_id, event.mint, t0, tuple(t0 + ms * NANOSECONDS_PER_MILLISECOND for ms in self.snapshot_ms), t0 + self.horizon_ns, self.quantum,
            event.virtual_sol_reserves, event.virtual_token_reserves, initial_vsol, initial_vtok, initial_real, event.token_total_supply, event.token_base_units, event.quote_lamports,
            tradable=tradable,
            creator_fee_enabled=event.creator not in (None, "", DEFAULT_PUBKEY),
            creator=event.creator,
            last_real_tok=event.real_token_reserves,
            last_signature=event.signature,
            last_slot=event.chain_slot,
            last_receive_sequence=event.receive_sequence,
            last_state_offset_ns=t0,
            coverage_state=coverage, coverage_start_ns=t0, first_buyer_window=self.features.first_buyer_window,
        )
        if gap_open:
            tracker.flags.add(FLAG_GAP)
        self.trackers[event.mint] = tracker
        self._by_launch[launch_id] = tracker

        # observation_ms == 0 is evaluated directly from the validated CREATE
        # state. No artificial snapshot delay is introduced.
        if self.on_launch is not None:
            self.on_launch(tracker, self.features.snapshot(tracker))

        for deadline in tracker.deadlines_ns:
            self.heap.push(deadline, launch_id, PURPOSE_SNAPSHOT)
        self.heap.push(tracker.horizon_end_ns, launch_id, PURPOSE_HORIZON_END)

    def _event_row(self, event: NormalizedEvent, launch_id: str, observation_id: int) -> EventRow:
        assert self.ids is not None
        return EventRow(
            self.ids.next("events"), self.session_id, launch_id, observation_id, event.provider_event_key, event.event_type.value, event.mint, event.signature, event.event_index,
            event.actor, event.quote_lamports, event.token_base_units, event.virtual_sol_reserves, event.virtual_token_reserves, self.session_sequence, event.receive_sequence,
            event.receive_offset_ns, event.utc_received_ns, event.chain_slot, 0, 0, 1,
        )

    def _event_ops(self, event: NormalizedEvent, tracker: TokenTracker, observation_id: int) -> list[WriteOp]:
        return [InsertOp(self._event_row(event, tracker.launch_id, observation_id))]

    # ------------------------------------------------------------------ deadlines
    def _dispatch(self, purpose: str, launch_id: str, fire_offset: int) -> None:
        tracker = self._by_launch.get(launch_id)
        if tracker is None:
            return
        if purpose == PURPOSE_SNAPSHOT:
            self.freeze_snapshot(tracker, fire_offset)
        elif purpose == PURPOSE_HORIZON_END:
            self.evict(tracker, LaunchStatus.EVICTED, data_complete=None)

    def freeze_snapshot(self, tracker: TokenTracker, deadline_offset_ns: int) -> None:
        if deadline_offset_ns in tracker.frozen:
            return
        features = self.features.snapshot(tracker)
        snapshot_id = self.ids.next("feature_snapshots") if self.ids is not None else 0
        tracker.frozen[deadline_offset_ns] = snapshot_id
        self.counters.snapshots += 1
        if self.writer is not None and self.ids is not None:
            row = FeatureSnapshotRow(
                snapshot_id, tracker.launch_id, self.session_id if self.origin == SnapshotOrigin.LIVE else None, self.backtest_run_id, self.feature_set_model_id, self.origin.value,
                deadline_offset_ns, tracker.coverage_state.value, tracker.coverage_start_ns, canonical_json(features), sha256_json(features), int(not tracker.flags),
            )
            self._write([InsertOp(row)])
        if self.on_snapshot is not None:
            self.on_snapshot(tracker, deadline_offset_ns, features)

    def evict(self, tracker: TokenTracker, status: LaunchStatus, data_complete: int | None) -> None:
        self.trackers.pop(tracker.mint, None)
        self._by_launch.pop(tracker.launch_id, None)
        self.counters.evictions += 1
        tracker.status = status
        if self.origin == SnapshotOrigin.LIVE:
            values: dict[str, int | str | None] = {"status": status.value}
            if data_complete is not None:
                values["data_complete"] = data_complete
            self._write([UpdateOp("launches", {"id": tracker.launch_id}, values)])

    # ------------------------------------------------------------------ coverage and intervals
    def on_connection(self, connected: bool, monotonic_ns: int) -> None:
        if connected:
            self._close_interval(DataQualityReason.RECONNECT)
            self.system_event("CHAIN_CONNECTED", Severity.INFO, {"code": "CHAIN_CONNECTED", "message": "logsSubscribe active"})
        else:
            self._open_interval(DataQualityReason.RECONNECT, {"code": "CHAIN_DISCONNECTED"})
            self.system_event("CHAIN_DISCONNECTED", Severity.WARN, {"code": "CHAIN_DISCONNECTED", "message": "reconnecting with backoff"})

    def _open_interval(self, reason: DataQualityReason, details: dict[str, Any]) -> None:
        if reason in self.open_intervals or self.ids is None:
            return
        interval_id = self.ids.next("data_quality_intervals")
        self.open_intervals[reason] = interval_id
        self._write([InsertOp(DataQualityIntervalRow(interval_id, self.session_id, None, self.clock.offset_ns(), None, reason.value, canonical_json(details)))])
        for tracker in self.trackers.values():
            if FLAG_GAP not in tracker.flags:
                tracker.flags.add(FLAG_GAP)
                tracker.coverage_state = CoverageState.GAP
                self._write([UpdateOp("launches", {"id": tracker.launch_id}, {"coverage_state": CoverageState.GAP.value, "data_complete": 0})])

    def _close_interval(self, reason: DataQualityReason) -> None:
        interval_id = self.open_intervals.pop(reason, None)
        if interval_id is not None:
            self._write([UpdateOp("data_quality_intervals", {"id": interval_id}, {"end_offset_ns": self.clock.offset_ns()})])

    def apply_replay_quality_boundary(self, reason: DataQualityReason, active: bool) -> None:
        """Apply one recorded data-quality interval boundary to replay RAM only.

        Live intervals are persisted by ``_open_interval``/``_close_interval``. Replay must
        reproduce their effect on tracker coverage without writing a second copy to SQLite.
        A tracker that overlaps any gap remains GAP for its lifetime, matching live behavior.
        """
        if self.origin != SnapshotOrigin.REPLAY:
            raise RuntimeError("replay quality boundaries are valid only for REPLAY origin")
        if active:
            if reason in self.open_intervals:
                return
            self.open_intervals[reason] = -1
            for tracker in self.trackers.values():
                tracker.flags.add(FLAG_GAP)
                tracker.coverage_state = CoverageState.GAP
        else:
            self.open_intervals.pop(reason, None)

    # ------------------------------------------------------------------ pumpportal cross-check
    def on_pumpportal_create(self, mint: str, monotonic_ns: int) -> None:
        self.counters.pumpportal_creates += 1
        chain_ns = self._chain_recent.get(mint)
        if chain_ns is not None:
            self.counters.pumpportal_after_chain += 1
            self.counters.pumpportal_lag_ns_total += monotonic_ns - chain_ns
        else:
            self._pumpportal_recent[mint] = monotonic_ns
            self._trim(self._pumpportal_recent)

    def _note_chain_create(self, mint: str, monotonic_ns: int) -> None:
        pp_ns = self._pumpportal_recent.pop(mint, None)
        if pp_ns is not None:
            self.counters.pumpportal_before_chain += 1
            self.counters.pumpportal_lag_ns_total += pp_ns - monotonic_ns
        self._chain_recent[mint] = monotonic_ns
        self._trim(self._chain_recent)

    def _trim(self, recent: dict[str, int]) -> None:
        while len(recent) > self.limits.tracker_limit:
            recent.pop(next(iter(recent)))

    # ------------------------------------------------------------------ health sampler (the one periodic timer)
    def start_health_sampler(self) -> None:
        interval = self.limits.health_sample_interval_ms * NANOSECONDS_PER_MILLISECOND
        self._health_handle = self._scheduler.schedule_at(self.clock.monotonic_ns() + interval, self._health_sample)

    def _health_sample(self, actual_monotonic_ns: int) -> None:
        now = self.clock.offset_ns(actual_monotonic_ns)
        stale_ns = self.limits.stream_stale_timeout_ms * NANOSECONDS_PER_MILLISECOND
        if self.last_message_offset_ns is not None and now - self.last_message_offset_ns > stale_ns:
            self._open_interval(DataQualityReason.STALE, {"code": "STREAM_STALE", "last_message_offset_ns": self.last_message_offset_ns})
        depth = self.writer.depth() if self.writer is not None else (0, 0)
        details = {
            "code": "HEALTH", "message": "periodic sample", "offset_ns": now, "active_trackers": len(self.trackers), "heap": len(self.heap), "queue_bulk_depth": depth[0],
            "queue_critical_depth": depth[1], "open_intervals": [reason.value for reason in self.open_intervals], "decoder_undecodable": self.decoder.undecodable,
            "decoder_unknown_discriminators": self.decoder.unknown_discriminators, "decoder_known_ignored_events": self.decoder.known_ignored_events,
            "writer_committed_rows": self.writer.committed_rows if self.writer else 0,
            "writer_dropped_envelopes": self.writer.dropped_envelopes if self.writer else 0, "writer_failures": self.writer.failures if self.writer else 0, **self.counters.as_dict(),
        }
        self.system_event("HEALTH", Severity.INFO, details)
        interval = self.limits.health_sample_interval_ms * NANOSECONDS_PER_MILLISECOND
        self._health_handle = self._scheduler.schedule_at(actual_monotonic_ns + interval, self._health_sample)

    # ------------------------------------------------------------------ shutdown
    def stop_accepting(self) -> None:
        self.accepting = False

    def close_all(self) -> None:
        """Controlled shutdown: PROCESS_RESTART interval, every open interval closed, every active launch evicted with data_complete=0."""
        if self.ids is not None:
            self._open_interval(DataQualityReason.PROCESS_RESTART, {"code": "PROCESS_RESTART"})
        for tracker in list(self.trackers.values()):
            self.evict(tracker, LaunchStatus.EVICTED, data_complete=0)
        for reason in list(self.open_intervals):
            self._close_interval(reason)
        if self._health_handle is not None:
            self._scheduler.cancel(self._health_handle)


def lamports_to_sol(lamports: int) -> Decimal:
    return Decimal(lamports) / Decimal(LAMPORTS_PER_SOL)


def seconds_to_ns(seconds: int) -> int:
    return seconds * NANOSECONDS_PER_SECOND
