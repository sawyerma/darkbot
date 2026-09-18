"""Shared pure entry/exit rules and versioned research features (spec: strategy.py).

Entry/exit functions are pure and cheap (hot path); feature functions are only called in the
background with frozen inputs/cutoffs (legacy_feature_contract.compute_schedule).
"""

from __future__ import annotations

from dataclasses import dataclass
from decimal import ROUND_HALF_EVEN, Decimal
from typing import Any

from darkbot.domain import LAMPORTS_PER_SOL, NS_PER_MS, CoverageState, ExitReason, MarketState, QuoteScopeStatus, VenueState

FEATURE_NAMES: tuple[str, ...] = (
    "buy_count", "sell_count", "buy_volume_sol", "sell_volume_sol", "net_buy_flow_sol", "unique_buyers", "unique_buyer_ratio",
    "largest_buy_sol", "buyer_volume_concentration", "first5_buyer_concentration", "first_sell_latency_ms", "price_change", "max_drawdown",
    "dev_initial_buy", "curve_progress_pct", "research_estimated_price_impact", "provider_gap_flag",
)
FEATURE_SET_V1 = "FEATURE_CONTRACT_V1_LEGACY"
FEATURE_SET_V2 = "FEATURE_SET_V2_CANONICAL_COVERAGE"
EXIT_PRIORITY = (ExitReason.CREATOR_SELL, ExitReason.TARGET, ExitReason.TIMEOUT)


@dataclass(frozen=True, slots=True)
class EntryVerdict:
    eligible: bool
    reason: str


def entry_admission(state: MarketState, *, gap_flag: bool, creator_sell_known: bool, already_submitted: bool, slots_free: bool, capital_ok: bool, costs_ok: bool, recording_ok: bool, quote_ok: bool) -> EntryVerdict:
    """Minimal safety checks only (ultra_low_latency_contract.minimal_safety_checks); no predictive filters."""
    if state.quote_scope_status != QuoteScopeStatus.SOL_WSOL:
        return EntryVerdict(False, str(state.quote_scope_status.value))
    if state.venue_state != VenueState.BONDING_CURVE_ACTIVE:
        return EntryVerdict(False, f"VENUE_{state.venue_state.value}")
    if gap_flag:
        return EntryVerdict(False, "CANONICAL_PROVIDER_GAP")
    if creator_sell_known:
        return EntryVerdict(False, "CREATOR_ALREADY_SOLD")
    if already_submitted:
        return EntryVerdict(False, "DUPLICATE_ENTRY")
    if not recording_ok:
        return EntryVerdict(False, "RECORDING_DEGRADED")
    if not costs_ok:
        return EntryVerdict(False, "UNQUALIFIED_COSTS")
    if not slots_free:
        return EntryVerdict(False, "SLOT_LIMIT")
    if not capital_ok:
        return EntryVerdict(False, "INSUFFICIENT_CAPITAL")
    if not quote_ok:
        return EntryVerdict(False, "QUOTE_INVALID")
    return EntryVerdict(True, "FIRST_ACTIONABLE_CREATE")


def net_multiple(net_liquidation_value: int, entry_economic_cost: int) -> Decimal | None:
    if entry_economic_cost <= 0:
        return None
    return Decimal(net_liquidation_value) / Decimal(entry_economic_cost)


def target_reached(net_liquidation_value: int | None, entry_economic_cost: int, target: Decimal) -> bool:
    if net_liquidation_value is None:
        return False
    multiple = net_multiple(net_liquidation_value, entry_economic_cost)
    return multiple is not None and multiple >= target


def pick_exit_trigger(candidates: list[tuple[ExitReason, int]]) -> tuple[ExitReason, int] | None:
    """First causal trigger wins; exact same decision timestamp -> CREATOR_SELL > TARGET > TIMEOUT."""
    if not candidates:
        return None
    earliest = min(t for _, t in candidates)
    tied = [r for r, t in candidates if t == earliest]
    for reason in EXIT_PRIORITY:
        if reason in tied:
            return reason, earliest
    return None


# --------------------------------------------------------------------------- research features (background only)


@dataclass(frozen=True, slots=True)
class FeatureEvent:
    event_type: str  # BUY | SELL
    receive_mono_ns: int
    actor: str
    sol_amount: int
    virtual_sol_reserves: int | None
    virtual_token_reserves: int | None
    real_token_reserves: int | None


@dataclass(frozen=True, slots=True)
class FeatureInputs:
    t0_mono_ns: int
    cutoff_mono_ns: int
    create_vsol: int | None
    create_vtok: int | None
    initial_virtual_token_reserves: int | None
    initial_real_token_reserves: int | None
    initial_buy_tokens: int | None
    token_total_supply: int | None
    research_quote_lamports: int
    events: tuple[FeatureEvent, ...]  # deduplicated BUY/SELL with receive <= cutoff, create-tx trades excluded
    coverage_state_at_cutoff: str
    quality_interval_intersects: bool
    v1_provider_connected_gap: bool | None  # legacy single-provider view for V1 export


class FeatureCalculator:
    """17 legacy features, formulas per legacy_feature_contract.legacy_oracle_precision_rule."""

    def __init__(self, ratio_decimal_places: int, first_buyer_window: int) -> None:
        self.quantum = Decimal(1).scaleb(-ratio_decimal_places)
        self.first_buyer_window = first_buyer_window

    def r(self, value: Decimal) -> Decimal:
        return value.quantize(self.quantum, rounding=ROUND_HALF_EVEN)

    def ratio(self, num: Decimal, den: Decimal) -> Decimal:
        return self.r(num / den)

    def price(self, vsol: int | None, vtok: int | None) -> Decimal | None:
        if vsol is None or vtok is None or vsol <= 0 or vtok <= 0:
            return None
        return self.ratio(Decimal(vsol), Decimal(vtok))

    def compute(self, inputs: FeatureInputs, feature_set: str) -> tuple[dict[str, Any], dict[str, str]]:
        ev = [e for e in inputs.events if e.receive_mono_ns <= inputs.cutoff_mono_ns]
        buys = [e for e in ev if e.event_type == "BUY"]
        sells = [e for e in ev if e.event_type == "SELL"]
        lps = Decimal(LAMPORTS_PER_SOL)
        buy_vol, sell_vol = Decimal(sum(e.sol_amount for e in buys)), Decimal(sum(e.sol_amount for e in sells))
        per_buyer: dict[str, int] = {}
        first_buyers: list[str] = []
        for e in buys:
            per_buyer[e.actor] = per_buyer.get(e.actor, 0) + e.sol_amount
            if e.actor not in first_buyers:
                first_buyers.append(e.actor)
        nulls: dict[str, str] = {}
        price_t0 = self.price(inputs.create_vsol, inputs.create_vtok)
        last_state = next((e for e in reversed(ev) if e.virtual_sol_reserves is not None and e.virtual_token_reserves is not None), None)
        price_t = self.price(last_state.virtual_sol_reserves, last_state.virtual_token_reserves) if last_state else price_t0
        f: dict[str, Any] = {}
        f["buy_count"] = len(buys)
        f["sell_count"] = len(sells)
        f["buy_volume_sol"] = format(buy_vol / lps, "f")
        f["sell_volume_sol"] = format(sell_vol / lps, "f")
        f["net_buy_flow_sol"] = format((buy_vol - sell_vol) / lps, "f")
        f["unique_buyers"] = len(per_buyer)
        f["unique_buyer_ratio"] = format(self.ratio(Decimal(len(per_buyer)), Decimal(len(buys))), "f") if buys else self._null(nulls, "unique_buyer_ratio", "buy_count=0")
        f["largest_buy_sol"] = format(Decimal(max(e.sol_amount for e in buys)) / lps, "f") if buys else self._null(nulls, "largest_buy_sol", "no BUY")
        if buy_vol:
            hhi = self.r(sum((self.ratio(Decimal(v), buy_vol) ** 2 for v in per_buyer.values()), Decimal(0)))
            f["buyer_volume_concentration"] = format(hhi, "f")
            f["first5_buyer_concentration"] = format(self.ratio(Decimal(sum(per_buyer[b] for b in first_buyers[: self.first_buyer_window])), buy_vol), "f")
        else:
            f["buyer_volume_concentration"] = self._null(nulls, "buyer_volume_concentration", "total=0")
            f["first5_buyer_concentration"] = self._null(nulls, "first5_buyer_concentration", "total=0")
        f["first_sell_latency_ms"] = format(Decimal(sells[0].receive_mono_ns - inputs.t0_mono_ns) / Decimal(NS_PER_MS), "f") if sells else self._null(nulls, "first_sell_latency_ms", "no SELL")
        if price_t0 is not None and price_t is not None:
            f["price_change"] = format(self.r(price_t / price_t0 - 1), "f")
            peak = price_t0
            mdd = Decimal(0)
            for e in ev:
                p = self.price(e.virtual_sol_reserves, e.virtual_token_reserves)
                if p is None:
                    continue
                peak = max(peak, p)
                mdd = max(mdd, self.r(1 - p / peak))
            f["max_drawdown"] = format(mdd, "f")
        else:
            f["price_change"] = self._null(nulls, "price_change", "price missing/nonpositive")
            f["max_drawdown"] = self._null(nulls, "max_drawdown", "no positive price")
        if inputs.initial_buy_tokens is not None and inputs.token_total_supply:
            f["dev_initial_buy"] = format(self.ratio(Decimal(inputs.initial_buy_tokens), Decimal(inputs.token_total_supply)), "f")
        else:
            f["dev_initial_buy"] = self._null(nulls, "dev_initial_buy", "missing/supply=0")
        vtok_t = last_state.virtual_token_reserves if last_state else inputs.create_vtok
        if vtok_t is not None and inputs.initial_virtual_token_reserves is not None and inputs.initial_real_token_reserves:
            real_t = Decimal(vtok_t - (inputs.initial_virtual_token_reserves - inputs.initial_real_token_reserves))
            f["curve_progress_pct"] = format(self.r(100 * (1 - real_t / Decimal(inputs.initial_real_token_reserves))), "f")
        else:
            f["curve_progress_pct"] = self._null(nulls, "curve_progress_pct", "initial missing/0")
        vsol_t = last_state.virtual_sol_reserves if last_state else inputs.create_vsol
        if price_t is not None and vsol_t and vtok_t:
            q = inputs.research_quote_lamports
            tokens_out = vtok_t - (vsol_t * vtok_t) // (vsol_t + q)
            if tokens_out > 0:
                avg = self.ratio(Decimal(q), Decimal(tokens_out))
                f["research_estimated_price_impact"] = format(self.r(avg / price_t - 1), "f")
            else:
                f["research_estimated_price_impact"] = self._null(nulls, "research_estimated_price_impact", "tokens_out<=0")
        else:
            f["research_estimated_price_impact"] = self._null(nulls, "research_estimated_price_impact", "no valid virtual reserves")
        if feature_set == FEATURE_SET_V2:
            f["provider_gap_flag"] = inputs.coverage_state_at_cutoff != CoverageState.OBSERVED.value or inputs.quality_interval_intersects
        else:
            if inputs.v1_provider_connected_gap is None:
                f["provider_gap_flag"] = True
                nulls["provider_gap_flag"] = "V1 single-provider state unknown -> true when unknown"
            else:
                f["provider_gap_flag"] = inputs.v1_provider_connected_gap or inputs.quality_interval_intersects
        return {name: f[name] for name in FEATURE_NAMES}, nulls

    @staticmethod
    def _null(nulls: dict[str, str], name: str, reason: str) -> None:
        nulls[name] = reason
        return None
