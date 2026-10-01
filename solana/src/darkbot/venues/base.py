"""Venue adapter contract (BSC handoff §4.2). Protocol semantics only, never transport."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol


@dataclass(frozen=True, slots=True)
class VenueIdentity:
    venue_id: str
    chain_id: str
    display_name: str


class VenueAdapter(Protocol):
    identity: VenueIdentity

    def build_decoder(self, *, cfg: Any) -> Any:
        ...

    def build_reducer(self, *, cfg: Any, run_id: str) -> Any:
        ...

    def build_quote_engine(self, *, cfg: Any) -> Any:
        ...

    def is_actionable_launch(self, state: Any) -> bool:
        ...

    def creator_of(self, state: Any) -> str | None:
        ...
