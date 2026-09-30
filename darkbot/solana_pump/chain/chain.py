"""Solana chain adapter (BSC handoff §2.2 "Existing behavior wrapped, not rewritten").

Every method delegates to the existing production implementation (``capture.FeedAdapter``, ``fees.FeeResolver``,
``config.resolve_feed_endpoints``/``resolve_rpc_http``). The feed/fee wiring below is the exact wiring that
``main.Application.run`` used before Phase A; it is only relocated behind the chain seam. Nothing here runs on the
per-message hot path: adapters are built once at runtime start and the StateOwner keeps direct bound-method references.
"""

from __future__ import annotations

import json
from typing import Any

from darkbot.capture import FeedAdapter
from darkbot.chains.base import ChainIdentity
from darkbot.config import resolve_feed_endpoints, resolve_rpc_http
from darkbot.domain import RawEnvelope
from darkbot.fees import FeeResolver, http_rpc

SOLANA_MAINNET = ChainIdentity(chain_id="solana-mainnet", family="solana", native_symbol="SOL", native_decimals=9)


class SolanaChainAdapter:
    identity: ChainIdentity = SOLANA_MAINNET

    # ------------------------------------------------------------------ feeds (was: Application.run feed loop)
    def feed_setup_status(self, cfg: Any) -> dict[str, str]:
        """Per configured feed the same status strings Application reported before Phase A (DISABLED / DISABLED_OPTIONAL / FAILED / STARTING)."""
        endpoints = resolve_feed_endpoints(cfg)
        status: dict[str, str] = {}
        for feed in cfg.feeds:
            ep = endpoints[feed.id]
            if not feed.enabled:
                status[feed.id] = "DISABLED"
            elif not ep.available:
                status[feed.id] = f"DISABLED_OPTIONAL missing env {ep.missing_env}" if not feed.required else f"FAILED missing env {ep.missing_env}"
            else:
                status[feed.id] = "STARTING"
        return status

    def build_feed_adapters(self, *, cfg: Any, clock: Any, on_raw: Any, on_health: Any, on_error: Any) -> tuple[FeedAdapter, ...]:
        """One production FeedAdapter per enabled feed with an available endpoint, in config order; callbacks are passed through unchanged."""
        endpoints = resolve_feed_endpoints(cfg)
        adapters: list[FeedAdapter] = []
        for feed in cfg.feeds:
            ep = endpoints[feed.id]
            if not feed.enabled or not ep.available:
                continue
            adapters.append(FeedAdapter(feed, ep.url, cfg.connectivity, cfg.protocol["program_id"], clock, on_raw, on_health, on_error))
        return tuple(adapters)

    # ------------------------------------------------------------------ fees (was: Application.run resolver wiring)
    def build_fee_resolver(self, *, cfg: Any, clock: Any, run_id: str) -> FeeResolver:
        rpc_ep = resolve_rpc_http(cfg)
        rpc = http_rpc(rpc_ep.url, cfg.connectivity["rpc_http_timeout_ms"] / 1000) if rpc_ep.available else None
        return FeeResolver(cfg, rpc, clock, run_id, "rpc_http" if rpc_ep.available else "NONE")

    # ------------------------------------------------------------------ generic identity mapping (handoff §7.1: tx_id = signature, block_ref = slot)
    @staticmethod
    def _notification_value(raw: Any) -> tuple[dict[str, Any], dict[str, Any]] | None:
        payload = raw.payload if isinstance(raw, RawEnvelope) else raw
        try:
            message = json.loads(payload) if isinstance(payload, str) else payload
        except (TypeError, ValueError):
            return None
        if not isinstance(message, dict):
            return None
        result = (message.get("params") or {}).get("result") if isinstance(message.get("params"), dict) else None
        if not isinstance(result, dict):
            return None
        return (result.get("context") or {}), (result.get("value") or {})

    def tx_id_from_raw(self, raw: Any) -> str | None:
        parsed = self._notification_value(raw)
        if parsed is None:
            return None
        signature = parsed[1].get("signature") if isinstance(parsed[1], dict) else None
        return signature if isinstance(signature, str) else None

    def block_ref_from_raw(self, raw: Any) -> str | int | None:
        parsed = self._notification_value(raw)
        if parsed is None:
            return None
        slot = parsed[0].get("slot") if isinstance(parsed[0], dict) else None
        return slot if isinstance(slot, int) and not isinstance(slot, bool) else None
