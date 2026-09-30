"""Monotonic real clock, deterministic replay clock and schedulers (spec: clock.py).

Every mono_ns value belongs to exactly one clock epoch (host_clock_epoch_id); values from
different epochs are never subtracted (T23). Callbacks receive the ACTUAL fire time so late
processing is measurable (callback_lateness_ns) and never back-dated.
"""

from __future__ import annotations

import asyncio
import heapq
import time
import uuid
from dataclasses import dataclass, field
from typing import Callable

from darkbot.domain import NS_PER_S, utc_iso

Callback = Callable[[int], None]


def new_epoch_id() -> str:
    return "epoch_" + uuid.uuid4().hex[:16]


@dataclass
class LiveClock:
    epoch_id: str = field(default_factory=new_epoch_id)
    started_at_monotonic_ns: int = field(default_factory=time.monotonic_ns)
    started_at_utc_ns: int = field(default_factory=time.time_ns)

    def monotonic_ns(self) -> int:
        return time.monotonic_ns()

    def utc_ns(self) -> int:
        return time.time_ns()

    def utc_iso(self) -> str:
        return utc_iso(time.time_ns())


@dataclass
class ReplayClock:
    """Logical model time of a recorded run; only advances forwards (driven by the replay engine)."""

    epoch_id: str
    started_at_monotonic_ns: int
    started_at_utc_ns: int
    current_mono_ns: int = 0

    def __post_init__(self) -> None:
        if self.current_mono_ns == 0:
            self.current_mono_ns = self.started_at_monotonic_ns

    def monotonic_ns(self) -> int:
        return self.current_mono_ns

    def utc_ns(self) -> int:
        return self.started_at_utc_ns + (self.current_mono_ns - self.started_at_monotonic_ns)

    def utc_iso(self) -> str:
        return utc_iso(self.utc_ns())

    def advance_to(self, mono_ns: int) -> None:
        if mono_ns < self.current_mono_ns:
            raise ValueError(f"replay clock cannot go backwards ({mono_ns} < {self.current_mono_ns})")
        self.current_mono_ns = mono_ns


class LiveScheduler:
    def __init__(self, loop: asyncio.AbstractEventLoop, clock: LiveClock) -> None:
        self._loop = loop
        self._clock = clock

    def schedule_at(self, fire_mono_ns: int, callback: Callback) -> asyncio.TimerHandle:
        delay = max(0.0, (fire_mono_ns - self._clock.monotonic_ns()) / NS_PER_S)
        return self._loop.call_later(delay, lambda: callback(self._clock.monotonic_ns()))

    def cancel(self, handle: object) -> None:
        if isinstance(handle, asyncio.TimerHandle):
            handle.cancel()


class ReplayScheduler:
    """Deterministic timer heap; fire_due(now) runs callbacks in (fire_time, insertion order)."""

    def __init__(self) -> None:
        self._heap: list[tuple[int, int, Callback]] = []
        self._seq = 0
        self._cancelled: set[int] = set()
        self.late_ns_samples: list[int] = []

    def schedule_at(self, fire_mono_ns: int, callback: Callback) -> int:
        self._seq += 1
        heapq.heappush(self._heap, (fire_mono_ns, self._seq, callback))
        return self._seq

    def cancel(self, handle: object) -> None:
        if isinstance(handle, int):
            self._cancelled.add(handle)

    def next_due_ns(self) -> int | None:
        while self._heap and self._heap[0][1] in self._cancelled:
            heapq.heappop(self._heap)
        return self._heap[0][0] if self._heap else None

    def fire_due(self, now_mono_ns: int, actual_mono_ns: int | None = None) -> int:
        """Fire every timer with fire_time <= now. actual_mono_ns models a late callback (>= fire time)."""
        fired = 0
        while self._heap and self._heap[0][0] <= now_mono_ns:
            fire_at, seq, callback = heapq.heappop(self._heap)
            if seq in self._cancelled:
                self._cancelled.discard(seq)
                continue
            actual = max(fire_at, actual_mono_ns if actual_mono_ns is not None else now_mono_ns)
            self.late_ns_samples.append(actual - fire_at)
            callback(actual)
            fired += 1
        return fired

    def pending(self) -> int:
        return sum(1 for item in self._heap if item[1] not in self._cancelled)
