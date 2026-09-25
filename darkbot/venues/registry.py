"""Venue registry (BSC handoff §3/§5): resolve a venue id to ONE executable venue adapter, once at runtime start.

``pumpfun`` is the only executable venue in Phase A. ``flap`` and ``fourmeme`` are identity metadata only: no
contracts, ABIs, selectors, decoders, quotes or fee rules exist (handoff §0.2 "no protocol guessing").
"""

from __future__ import annotations

from typing import Any, Callable

from darkbot.venues.base import VenueIdentity


class UnknownVenue(ValueError):
    """The venue id is not registered at all."""


class VenueNotImplemented(NotImplementedError):
    """The venue exists as metadata but has no executable adapter in this phase."""


def _pumpfun_factory() -> Any:
    from darkbot.venues.pumpfun.venue import PumpFunVenueAdapter  # lazy: keeps registry import free of decoder/engine modules

    return PumpFunVenueAdapter()


VENUE_IDENTITIES: dict[str, VenueIdentity] = {
    "pumpfun": VenueIdentity(venue_id="pumpfun", chain_id="solana-mainnet", display_name="Pump.fun"),
    "flap": VenueIdentity(venue_id="flap", chain_id="bsc-mainnet", display_name="Flap.sh"),
    "fourmeme": VenueIdentity(venue_id="fourmeme", chain_id="bsc-mainnet", display_name="Four.meme"),
}

VENUE_FACTORIES: dict[str, Callable[[], Any]] = {
    "pumpfun": _pumpfun_factory,
}


def venue_identity(venue_id: str) -> VenueIdentity:
    try:
        return VENUE_IDENTITIES[venue_id]
    except KeyError as exc:
        raise UnknownVenue(f"unknown venue {venue_id!r}; known: {sorted(VENUE_IDENTITIES)}") from exc


def load_venue(venue_id: str) -> Any:
    """Executable adapter for ``venue_id``; explicit failure for unknown or not-yet-implemented venues."""
    identity = venue_identity(venue_id)
    factory = VENUE_FACTORIES.get(venue_id)
    if factory is None:
        raise VenueNotImplemented(f"venue {identity.venue_id!r} on {identity.chain_id!r} is registered as metadata only; no executable adapter in Phase A")
    adapter = factory()
    if adapter.identity != identity:
        raise UnknownVenue(f"venue adapter identity mismatch for {venue_id!r}: {adapter.identity} != {identity}")
    return adapter
