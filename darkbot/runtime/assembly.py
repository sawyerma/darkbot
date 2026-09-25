"""Runtime assembly (BSC handoff §5): profile -> chain + venue -> components, resolved ONCE at runtime start.

Scope (handoff §5.1): this is the normal PAPER/CAPTURE/DOCTOR component selection. It does NOT replace the
specialised replay construction in ``darkbot.replay`` (``StateOwner(engine=...)``, canonical_prefix, recorded fee
snapshots, stress parameters, virtual clock, mint filters, lateness, delay provider). Replay stays offline and untouched.

Hot-path rule: nothing in this module is called per message. ``assemble_runtime`` returns already-built objects; the
StateOwner and the feed adapters keep direct references to them.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable

from darkbot.chains.registry import load_chain
from darkbot.runtime.profiles import DEFAULT_MARKET_PROFILE_ID, MarketProfile, load_market_profile
from darkbot.venues.registry import load_venue


class MarketProfileNotRunnable(NotImplementedError):
    """The profile exists as metadata but cannot be executed in this phase (status != READY)."""


@dataclass(frozen=True, slots=True)
class RuntimeCallbacks:
    on_raw: Callable[..., Any]
    on_health: Callable[..., Any]
    on_error: Callable[..., Any]


@dataclass(frozen=True, slots=True)
class RuntimeComponents:
    market_profile: MarketProfile
    chain: Any
    venue: Any
    decoder: Any
    reducer: Any
    quote_engine: Any
    fee_resolver: Any
    feeds: tuple[Any, ...]


def effective_market_profile_id(cfg: Any) -> str:
    """Legacy configs carry no ``market_profile_id``; the default is applied here, outside the hashed raw config (handoff §6.1)."""
    return getattr(cfg, "market_profile_id", None) or DEFAULT_MARKET_PROFILE_ID


def resolve_market_profile(cfg: Any) -> MarketProfile:
    return load_market_profile(effective_market_profile_id(cfg))


def require_runnable(profile: MarketProfile) -> MarketProfile:
    if not profile.runnable:
        raise MarketProfileNotRunnable(f"market profile {profile.profile_id!r} ({profile.display_name}) has status {profile.status}: metadata only, not runnable in Phase A")
    return profile


@dataclass(frozen=True, slots=True)
class RuntimePreflight:
    market_profile: MarketProfile
    chain: Any
    venue: Any


def preflight_market_profile(cfg: Any) -> RuntimePreflight:
    """Fail-fast gate to run BEFORE any run/storage side effect (DB open, migrations, runs INSERT, writer start).

    Order: resolve profile -> require runnable -> load chain -> load venue -> venue/chain compatibility.
    Builds no decoder/reducer/engine/resolver/feed and touches no file, socket or database.
    """
    profile = require_runnable(resolve_market_profile(cfg))
    chain = load_chain(profile.chain_id)
    venue = load_venue(profile.venue_id)
    if venue.identity.chain_id != chain.identity.chain_id:
        raise ValueError(f"venue/chain mismatch: {venue.identity.venue_id} requires {venue.identity.chain_id}, got {chain.identity.chain_id}")
    return RuntimePreflight(market_profile=profile, chain=chain, venue=venue)


def assemble_runtime(*, cfg: Any, clock: Any, run_id: str, callbacks: RuntimeCallbacks | None, canonical_prefix: str | None = None) -> RuntimeComponents:
    """Build the executable components for the effective market profile of ``cfg``.

    ``callbacks=None`` builds everything except the feed adapters (``feeds=()``); use ``build_feeds`` after the
    StateOwner exists so the feeds receive its bound methods directly (no lambda/task hop on the receive path).
    Unknown chain/venue, venue/chain mismatch and metadata-only (BSC) profiles fail explicitly (same gate as
    ``preflight_market_profile``; callers with storage side effects must run the preflight first).
    """
    pre = preflight_market_profile(cfg)
    profile, chain, venue = pre.market_profile, pre.chain, pre.venue
    decoder = venue.build_decoder(cfg=cfg)
    reducer = venue.build_reducer(cfg=cfg, run_id=canonical_prefix or run_id)
    quote_engine = venue.build_quote_engine(cfg=cfg)
    fee_resolver = chain.build_fee_resolver(cfg=cfg, clock=clock, run_id=run_id)
    feeds: tuple[Any, ...] = ()
    if callbacks is not None:
        feeds = tuple(chain.build_feed_adapters(cfg=cfg, clock=clock, on_raw=callbacks.on_raw, on_health=callbacks.on_health, on_error=callbacks.on_error))
    return RuntimeComponents(market_profile=profile, chain=chain, venue=venue, decoder=decoder, reducer=reducer, quote_engine=quote_engine, fee_resolver=fee_resolver, feeds=feeds)


def build_feeds(components: RuntimeComponents, *, cfg: Any, clock: Any, callbacks: RuntimeCallbacks) -> tuple[Any, ...]:
    """Second stage of assembly: feed adapters bound directly to the consumer callbacks."""
    return tuple(components.chain.build_feed_adapters(cfg=cfg, clock=clock, on_raw=callbacks.on_raw, on_health=callbacks.on_health, on_error=callbacks.on_error))
