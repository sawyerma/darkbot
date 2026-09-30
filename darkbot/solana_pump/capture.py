"""Feed adapters: Helius/RPC logsSubscribe and optional free PumpPortal (spec: capture.py).
Connections, subscription ACKs, receive timestamps and arrival_sequence, reconnect with
backoff+jitter, source health. No decisions here. Transport errors, provider errors, decode
errors and internal invariants are reported as separate kinds (F13).
"""

from __future__ import annotations

import asyncio
import json
import random
from dataclasses import dataclass, field
from typing import Any, Callable, Awaitable

from darkbot.solana_pump.config import FeedConfig, redact
from darkbot.solana_pump.domain import NS_PER_MS, RawEnvelope, utc_iso
from darkbot.solana_pump.execution import LiveBroadcastForbidden

ALLOWED_PUMPPORTAL_METHODS = ("subscribeNewToken", "subscribeMigration")
FORBIDDEN_PUMPPORTAL_METHODS = ("subscribeTokenTrade", "subscribeAccountTrade", "subscribeRaydiumLiquidity")


class SubscriptionRejected(RuntimeError):
    pass


@dataclass
class FeedStats:
    messages: int = 0
    reconnects: int = 0
    connections: int = 0
    acks: int = 0
    ack_failures: int = 0
    transport_errors: int = 0
    provider_errors: int = 0
    ready: bool = False
    last_error: str | None = None
    last_receive_mono_ns: int | None = None
    disabled_reason: str | None = None


def pumpportal_frame(method: str, keys: list[str] | None = None) -> str:
    if method in FORBIDDEN_PUMPPORTAL_METHODS or method not in ALLOWED_PUMPPORTAL_METHODS:
        raise LiveBroadcastForbidden(f"PumpPortal method {method} is not allowed (only free NewToken/Migration hints)")
    frame: dict[str, Any] = {"method": method}
    if keys:
        frame["keys"] = keys
    return json.dumps(frame)


def logs_subscribe_frame(program_id: str, commitment: str, request_id: int = 1) -> str:
    return json.dumps({"jsonrpc": "2.0", "id": request_id, "method": "logsSubscribe", "params": [{"mentions": [program_id]}, {"commitment": commitment}]})


def is_logs_ack(message: Any, request_id: int = 1) -> tuple[bool, str | None]:
    """(is_ack_frame, error). A frame with our id is the ACK; result must be an integer subscription id."""
    if not isinstance(message, dict) or message.get("id") != request_id:
        return False, None
    if "error" in message:
        return True, str(message["error"])
    return True, None if isinstance(message.get("result"), int) else "ACK_WITHOUT_SUBSCRIPTION_ID"


async def websockets_connect(url: str, max_size: int, ping_interval_s: float) -> Any:
    import websockets  # lazy: not required for tests/replay/report

    return await websockets.connect(url, max_size=max_size, ping_interval=ping_interval_s)


class FeedAdapter:
    """async receive -> RawEnvelope(feed_id, connection_id, receive_mono_ns, receive_utc, arrival_sequence, payload); ACK/readiness separate."""

    def __init__(self, feed: FeedConfig, url: str, connectivity: dict[str, Any], program_id: str, clock: Any, on_raw: Callable[[RawEnvelope], Any],
                 on_health: Callable[[str, bool, int, str], None], on_error: Callable[[str, str, str, int], None], connect: Callable[..., Awaitable[Any]] = websockets_connect) -> None:
        self.feed, self.url, self.conn_cfg, self.program_id, self.clock = feed, url, connectivity, program_id, clock
        self.on_raw, self.on_health, self.on_error, self.connect = on_raw, on_health, on_error, connect
        self.stats = FeedStats()
        self.sequence = 0
        self._stop = asyncio.Event()
        self._ws: Any = None
        self.rng = random.Random(feed.id)

    def stop(self) -> None:
        self._stop.set()

    async def run(self) -> None:
        backoff_ms = self.conn_cfg["reconnect_initial_ms"]
        while not self._stop.is_set():
            connection_id = f"{self.feed.id}-c{self.stats.connections + 1}"
            try:
                ws = await self.connect(self.url, self.conn_cfg["websocket_max_message_bytes"], self.conn_cfg["websocket_ping_interval_ms"] / 1000)
                self._ws = ws
                self.stats.connections += 1
                try:
                    await self._subscribe(ws, connection_id)
                    backoff_ms = self.conn_cfg["reconnect_initial_ms"]
                    async for frame in ws:
                        receive_mono = self.clock.monotonic_ns()  # before any parsing
                        text = frame.decode("utf-8") if isinstance(frame, bytes) else frame
                        self.sequence += 1
                        self.stats.messages += 1
                        self.stats.last_receive_mono_ns = receive_mono
                        self.on_raw(RawEnvelope(self.feed.id, connection_id, receive_mono, utc_iso(self.clock.utc_ns()), self.sequence, text))
                        if self._stop.is_set():
                            break
                finally:
                    try:
                        await ws.close()
                    except Exception:  # noqa: BLE001
                        pass
                    self._ws = None
            except asyncio.CancelledError:
                raise
            except SubscriptionRejected as exc:
                self.stats.ack_failures += 1
                self.stats.provider_errors += 1
                self.stats.last_error = redact(str(exc))
                self.on_error(self.feed.id, "SUBSCRIPTION_REJECTED", self.stats.last_error, self.clock.monotonic_ns())
            except Exception as exc:  # noqa: BLE001 - transport failure: reconnect with backoff; never masks consumer errors (StateOwner catches its own)
                self.stats.transport_errors += 1
                self.stats.last_error = redact(f"{type(exc).__name__}: {exc}")
                self.on_error(self.feed.id, "TRANSPORT_ERROR", self.stats.last_error, self.clock.monotonic_ns())
            if self.stats.ready:
                self.stats.ready = False
                self.on_health(self.feed.id, False, self.clock.monotonic_ns(), self.stats.last_error or "closed")
            if self._stop.is_set():
                break
            self.stats.reconnects += 1
            jitter = 1 + self.rng.uniform(-float(self.conn_cfg["jitter_fraction"]), float(self.conn_cfg["jitter_fraction"]))
            await asyncio.sleep(backoff_ms * jitter / 1000)
            backoff_ms = min(backoff_ms * 2, self.conn_cfg["reconnect_max_ms"])

    async def _subscribe(self, ws: Any, connection_id: str) -> None:
        timeout = self.conn_cfg["subscription_ack_timeout_ms"] / 1000
        if self.feed.transport == "logsSubscribe":
            await ws.send(logs_subscribe_frame(self.program_id, self.feed.commitment or "processed"))
            deadline = asyncio.get_running_loop().time() + timeout
            while True:
                remaining = deadline - asyncio.get_running_loop().time()
                if remaining <= 0:
                    raise SubscriptionRejected("ACK_TIMEOUT")
                frame = await asyncio.wait_for(ws.recv(), timeout=remaining)
                text = frame.decode("utf-8") if isinstance(frame, bytes) else frame
                try:
                    message = json.loads(text)
                except json.JSONDecodeError:
                    continue
                is_ack, error = is_logs_ack(message)
                if is_ack:
                    if error:
                        raise SubscriptionRejected(error)
                    break
            self.stats.acks += 1
        else:
            for method in ALLOWED_PUMPPORTAL_METHODS:
                await ws.send(pumpportal_frame(method))
            # PumpPortal has no JSON-RPC ACK; readiness = connection open and no immediate error frame
            self.stats.acks += 1
        self.stats.ready = True
        self.on_health(self.feed.id, True, self.clock.monotonic_ns(), connection_id)


class FakeWebSocket:
    """Test transport: scripted frames with optional per-frame delays and a scripted close/error."""

    def __init__(self, frames: list[Any], ack: str | None = None, fail_after: int | None = None) -> None:
        self.frames = list(frames)
        self.ack = ack
        self.sent: list[str] = []
        self.fail_after = fail_after
        self.closed = False
        self._i = 0

    async def send(self, text: str) -> None:
        self.sent.append(text)

    async def recv(self) -> str:
        if self.ack is not None:
            ack, self.ack = self.ack, None
            return ack
        return await self.__anext__()

    def __aiter__(self) -> "FakeWebSocket":
        return self

    async def __anext__(self) -> str:
        if self.fail_after is not None and self._i >= self.fail_after:
            raise ConnectionError("scripted transport failure")
        if self._i >= len(self.frames):
            raise StopAsyncIteration
        item = self.frames[self._i]
        self._i += 1
        if isinstance(item, tuple):
            await asyncio.sleep(item[0])
            return item[1]
        return item

    async def close(self) -> None:
        self.closed = True


def fake_connect_factory(sessions: list[FakeWebSocket]) -> Callable[..., Awaitable[Any]]:
    sessions = list(sessions)

    async def connect(url: str, max_size: int, ping: float) -> FakeWebSocket:
        if not sessions:
            raise ConnectionError("no more scripted sessions")
        return sessions.pop(0)

    return connect
