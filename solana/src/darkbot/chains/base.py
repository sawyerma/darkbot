"""Chain adapter contract (BSC handoff §4.1). No venue logic here."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol, Sequence


@dataclass(frozen=True, slots=True)
class ChainIdentity:
    chain_id: str
    family: str
    native_symbol: str
    native_decimals: int


class ChainAdapter(Protocol):
    identity: ChainIdentity

    def build_feed_adapters(
        self,
        *,
        cfg: Any,
        clock: Any,
        on_raw: Any,
        on_health: Any,
        on_error: Any,
    ) -> Sequence[Any]:
        ...

    def build_fee_resolver(self, *, cfg: Any, clock: Any, run_id: str) -> Any:
        ...

    def block_ref_from_raw(self, raw: Any) -> str | int | None:
        ...

    def tx_id_from_raw(self, raw: Any) -> str | None:
        ...
