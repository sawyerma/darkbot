"""Pump.fun venue adapter (BSC handoff §2.3 "Wrap existing decoder/reducer/quote behavior").

Thin delegation only: the decoder, reducer and quote engine are the unchanged production classes, constructed with
exactly the arguments ``paper.StateOwner.__init__`` used before Phase A. No Pump logic is copied or re-implemented here.
"""

from __future__ import annotations

from typing import Any

from darkbot.solana_pump.domain import MarketState, NormalizedEvent, QuoteScopeStatus, VenueState
from darkbot.solana_pump.execution import ExecutionPolicy, QuoteEngine
from darkbot.solana_pump.market import MarketReducer, PumpDecoder
from darkbot.venues.base import VenueIdentity

PUMPFUN = VenueIdentity(venue_id="pumpfun", chain_id="solana-mainnet", display_name="Pump.fun")


class PumpFunVenueAdapter:
    identity: VenueIdentity = PUMPFUN

    def build_decoder(self, *, cfg: Any) -> PumpDecoder:
        return PumpDecoder(cfg.protocol)

    def build_reducer(self, *, cfg: Any, run_id: str) -> MarketReducer:
        """``run_id`` is the canonical-id prefix (StateOwner passes ``canonical_prefix or run_id``, unchanged)."""
        reducer = MarketReducer(run_id, cfg.storage["state_history_per_mint"], cfg.storage["max_tracked_mints"], cfg.storage["trade_log_per_mint"], cfg.storage["undecodable_points_max"])
        reducer.signature_history_max = cfg.storage["signature_history_max"]
        reducer.dedup_keys_per_mint = cfg.storage["dedup_keys_per_mint"]
        reducer.set_quote_constants(cfg.protocol["wsol_mint"], cfg.protocol["native_sol_curve_sentinel"])
        return reducer

    def build_quote_engine(self, *, cfg: Any) -> QuoteEngine:
        return QuoteEngine(ExecutionPolicy.from_config(cfg))

    def is_actionable_launch(self, state: Any) -> bool:
        """Same predicate the StateOwner applies to a new CREATE (SOL/WSOL quote scope on an active bonding curve)."""
        return state.quote_scope_status == QuoteScopeStatus.SOL_WSOL and state.venue_state == VenueState.BONDING_CURVE_ACTIVE

    def creator_of(self, state: Any) -> str | None:
        return state.creator

    # ------------------------------------------------------------------ generic identity mapping (handoff §7.1)
    @staticmethod
    def asset_id_of(value: MarketState | NormalizedEvent | Any) -> str | None:
        """asset_id = mint (compatibility mapping; database columns are not renamed in Phase A)."""
        return getattr(value, "mint", None)

    @staticmethod
    def native_amount_base_units_of(event: NormalizedEvent | Any) -> int | None:
        """native_amount_base_units = sol_amount (lamports)."""
        return getattr(event, "sol_amount", None)
