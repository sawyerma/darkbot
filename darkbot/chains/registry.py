"""Chain registry (BSC handoff §3/§5): resolve a chain id to ONE executable chain adapter, once at runtime start.

``solana-mainnet`` is the only executable chain in Phase A. ``bsc-mainnet`` is registered as identity metadata only;
requesting its executable adapter fails explicitly (``ChainNotImplemented`` is a ``NotImplementedError``). No EVM/BSC
networking, gas, nonce or builder code exists yet.
"""

from __future__ import annotations

from typing import Any, Callable

from darkbot.chains.base import ChainIdentity


class UnknownChain(ValueError):
    """The chain id is not registered at all."""


class ChainNotImplemented(NotImplementedError):
    """The chain exists as metadata but has no executable adapter in this phase."""


def _solana_factory() -> Any:
    from darkbot.solana_pump.chain.chain import SolanaChainAdapter  # lazy: keeps registry import free of feed/fee modules

    return SolanaChainAdapter()


# chain_id -> identity metadata (documented in the handoff: BSC mainnet = EVM family, BNB native, 18 decimals)
CHAIN_IDENTITIES: dict[str, ChainIdentity] = {
    "solana-mainnet": ChainIdentity(chain_id="solana-mainnet", family="solana", native_symbol="SOL", native_decimals=9),
    "bsc-mainnet": ChainIdentity(chain_id="bsc-mainnet", family="evm", native_symbol="BNB", native_decimals=18),
}

# chain_id -> factory of an executable adapter; chains without a factory are metadata only
CHAIN_FACTORIES: dict[str, Callable[[], Any]] = {
    "solana-mainnet": _solana_factory,
}


def chain_identity(chain_id: str) -> ChainIdentity:
    try:
        return CHAIN_IDENTITIES[chain_id]
    except KeyError as exc:
        raise UnknownChain(f"unknown chain {chain_id!r}; known: {sorted(CHAIN_IDENTITIES)}") from exc


def load_chain(chain_id: str) -> Any:
    """Executable adapter for ``chain_id``; explicit failure for unknown or not-yet-implemented chains."""
    identity = chain_identity(chain_id)
    factory = CHAIN_FACTORIES.get(chain_id)
    if factory is None:
        raise ChainNotImplemented(f"chain {identity.chain_id!r} ({identity.family}) is registered as metadata only; no executable adapter in Phase A")
    adapter = factory()
    if adapter.identity != identity:
        raise UnknownChain(f"chain adapter identity mismatch for {chain_id!r}: {adapter.identity} != {identity}")
    return adapter
