"""Clocks and schedulers (Runbook 4.2). Live: monotonic_ns and loop.call_at. Replay: advanced by the replay engine."""

from __future__ import annotations

import asyncio
import heapq
import time
from dataclasses import dataclass, field
from typing import Callable, Protocol

from darkbot.domain import NANOSECONDS_PER_SECOND


class Clock(Protocol):
    started_at_monotonic_ns: int
    started_at_utc_ns: int

    def monotonic_ns(self) -> int: ...
    def utc_ns(self) -> int: ...
    def offset_ns(self, monotonic_ns: int | None = None) -> int: ...


@dataclass
class LiveClock:
    started_at_monotonic_ns: int = field(default_factory=time.monotonic_ns)
    started_at_utc_ns: int = field(default_factory=time.time_ns)

    def monotonic_ns(self) -> int:
        return time.monotonic_ns()

    def utc_ns(self) -> int:
        return time.time_ns()

    def offset_ns(self, monotonic_ns: int | None = None) -> int:
        return (monotonic_ns if monotonic_ns is not None else time.monotonic_ns()) - self.started_at_monotonic_ns


@dataclass
class ReplayClock:
    """Time of a recorded session; advanced forwards only by the replay engine."""

    started_at_monotonic_ns: int
    started_at_utc_ns: int
    current_offset_ns: int = 0

    def monotonic_ns(self) -> int:
        return self.started_at_monotonic_ns + self.current_offset_ns

    def utc_ns(self) -> int:
        return self.started_at_utc_ns + self.current_offset_ns

    def offset_ns(self, monotonic_ns: int | None = None) -> int:
        if monotonic_ns is not None:
            return monotonic_ns - self.started_at_monotonic_ns
        return self.current_offset_ns

    def advance_to(self, offset_ns: int) -> None:
        if offset_ns < self.current_offset_ns:
            raise ValueError(f"replay clock cannot go backwards ({offset_ns} < {self.current_offset_ns})")
        self.current_offset_ns = offset_ns


Callback = Callable[[int], None]


class Scheduler(Protocol):
    def schedule_at(self, fire_monotonic_ns: int, callback: Callback) -> object: ...
    def cancel(self, handle: object) -> None: ...


class LiveScheduler:
    """Exactly one loop.call_at per schedule_at; the callback receives the actual monotonic time."""

    def __init__(self, loop: asyncio.AbstractEventLoop, clock: Clock) -> None:
        self._loop = loop
        self._clock = clock

    def schedule_at(self, fire_monotonic_ns: int, callback: Callback) -> asyncio.TimerHandle:
        delay = max(0.0, (fire_monotonic_ns - self._clock.monotonic_ns()) / NANOSECONDS_PER_SECOND)
        return self._loop.call_at(self._loop.time() + delay, lambda: callback(self._clock.monotonic_ns()))

    def cancel(self, handle: object) -> None:
        if isinstance(handle, asyncio.TimerHandle):
            handle.cancel()


class ReplayScheduler:
    """Ordered timers fired by the replay engine when replay time reaches them (same time: insertion order)."""

    def __init__(self) -> None:
        self._entries: list[tuple[int, int, Callback]] = []
        self._sequence = 0
        self._cancelled: set[int] = set()

    def schedule_at(self, fire_monotonic_ns: int, callback: Callback) -> int:
        self._sequence += 1
        heapq.heappush(self._entries, (fire_monotonic_ns, self._sequence, callback))
        return self._sequence

    def cancel(self, handle: object) -> None:
        if isinstance(handle, int):
            self._cancelled.add(handle)

    def fire_due(self, monotonic_ns: int) -> int:
        """Fire every timer with fire time <= monotonic_ns, in time order; returns the count fired."""
        fired = 0
        while self._entries and self._entries[0][0] <= monotonic_ns:
            fire_ns, sequence, callback = heapq.heappop(self._entries)
            if sequence in self._cancelled:
                self._cancelled.discard(sequence)
                continue
            callback(fire_ns)
            fired += 1
        return fired

    def pending(self) -> int:
        return len(self._entries) - len(self._cancelled)
