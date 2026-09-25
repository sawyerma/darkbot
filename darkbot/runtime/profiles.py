"""Market profile registry (BSC handoff §4.3): stable backend IDs for chain + venue combinations.

Pure metadata: no runtime imports, no networking, no protocol constants. The stable ``profile_id`` is
authoritative; ``display_name`` is presentation only. Profiles with status ``RESEARCH`` exist as metadata
only in Phase A and are never runnable (see ``darkbot.runtime.assembly``).
"""

from __future__ import annotations

from dataclasses import dataclass

DEFAULT_MARKET_PROFILE_ID = "solana-mainnet:pumpfun"  # effective profile of every legacy config without market_profile_id (resolved OUTSIDE the raw config, never written into it)

PROFILE_STATUS_READY = "READY"
PROFILE_STATUS_RESEARCH = "RESEARCH"


class UnknownMarketProfile(ValueError):
    """The requested profile id is not registered."""


@dataclass(frozen=True, slots=True)
class MarketProfile:
    profile_id: str
    chain_id: str
    venue_id: str
    display_name: str
    status: str

    @property
    def runnable(self) -> bool:
        return self.status == PROFILE_STATUS_READY


MARKET_PROFILES: dict[str, MarketProfile] = {
    "solana-mainnet:pumpfun": MarketProfile(
        profile_id="solana-mainnet:pumpfun",
        chain_id="solana-mainnet",
        venue_id="pumpfun",
        display_name="Solana / Pump.fun",
        status=PROFILE_STATUS_READY,
    ),
    "bsc-mainnet:flap": MarketProfile(
        profile_id="bsc-mainnet:flap",
        chain_id="bsc-mainnet",
        venue_id="flap",
        display_name="BSC / Flap.sh",
        status=PROFILE_STATUS_RESEARCH,
    ),
    "bsc-mainnet:fourmeme": MarketProfile(
        profile_id="bsc-mainnet:fourmeme",
        chain_id="bsc-mainnet",
        venue_id="fourmeme",
        display_name="BSC / Four.meme",
        status=PROFILE_STATUS_RESEARCH,
    ),
}


def load_market_profile(profile_id: str) -> MarketProfile:
    """Metadata lookup only; never constructs runtime components."""
    try:
        return MARKET_PROFILES[profile_id]
    except KeyError as exc:
        raise UnknownMarketProfile(f"unknown market profile {profile_id!r}; known: {sorted(MARKET_PROFILES)}") from exc


def market_profile_catalog() -> list[dict[str, str]]:
    """GUI/control-plane view (roadmap §3.1): stable id + status, no trading logic."""
    return [{"id": p.profile_id, "chain_id": p.chain_id, "venue_id": p.venue_id, "display_name": p.display_name, "status": p.status} for p in MARKET_PROFILES.values()]
