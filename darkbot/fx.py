"""Receipt-time SOL/USD evidence, fetched only by a background task and replayed verbatim."""
from __future__ import annotations
import asyncio
import json
import urllib.request
from dataclasses import dataclass, asdict
from decimal import Decimal, InvalidOperation
from typing import Any
from darkbot.config import canonical_json, sha256_text


@dataclass(frozen=True)
class FxSnapshot:
    snapshot_id: str
    source: str
    observed_utc: str
    received_mono_ns: int
    amount: str | None
    raw_json: str
    raw_hash: str
    validation_status: str
    timestamp_basis: str = "LOCAL_RECEIPT_NOT_PROVIDER_MARKET_TIME"

    @classmethod
    def from_payload(cls, payload: str, source: str, observed_utc: str, received_mono_ns: int):
        try:
            data = json.loads(payload)["data"]
            rate = Decimal(data["amount"])
            valid = data["base"] == "SOL" and data["currency"] == "USD" and rate.is_finite() and rate > 0
        except (KeyError, TypeError, ValueError, InvalidOperation):
            valid = False
        raw_hash = sha256_text(payload)
        identity = sha256_text(canonical_json([source, observed_utc, received_mono_ns, raw_hash]))
        return cls(identity, source, observed_utc, received_mono_ns, str(rate) if valid else None,
                   payload, raw_hash, "VALID" if valid else "INVALID")

    def budget(self, usd: str, now: int, max_age_ms: int) -> int | None:
        if self.validation_status != "VALID" or self.amount is None or not 0 <= now - self.received_mono_ns <= max_age_ms * 1_000_000:
            return None
        return int(Decimal(usd) * 1_000_000_000 / Decimal(self.amount))

    def row(self, run_id: str):
        return {"fx_snapshot_id": f"{run_id}_{self.snapshot_id}", "run_id": run_id,
                **{k: v for k, v in asdict(self).items() if k != "snapshot_id"}, "evidence_id": self.snapshot_id}


def fetch_spot(source: str, timeout_seconds: float) -> str:
    request = urllib.request.Request(source, headers={"User-Agent": "DarkBot-PAPER"})
    with urllib.request.urlopen(request, timeout=timeout_seconds) as response:
        return response.read().decode("utf-8")


async def run_fx_background(config: dict[str, Any], clock, on_snapshot, stop_event, fetch=fetch_spot):
    """No callbacks into strategy from HTTP threads; deliver evidence on the StateOwner event loop."""
    while not stop_event.is_set():
        try:
            payload = await asyncio.to_thread(fetch, config["source_url"], config["request_timeout_ms"] / 1000)
        except Exception as exc:
            payload = json.dumps({"error": type(exc).__name__})
        on_snapshot(FxSnapshot.from_payload(payload, config["source_url"], clock.utc_iso(), clock.monotonic_ns()))
        try:
            await asyncio.wait_for(stop_event.wait(), config["refresh_interval_ms"] / 1000)
        except asyncio.TimeoutError:
            pass
