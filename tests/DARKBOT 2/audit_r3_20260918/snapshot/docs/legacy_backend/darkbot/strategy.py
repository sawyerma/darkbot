"""Features and filters (LEAN_V1 section 4). Same code for live and replay; never imports cli's oracle.

FeatureCalculator.snapshot turns a TokenTracker's O(1) aggregates into the 17 contract
features (feature_contract_v1 order; int, decimal string, bool or null). Rounding follows
phase0_fixture_contract.precision_rule: quotients quantized to ratio_decimal_places,
ROUND_HALF_EVEN; volumes exact; nulls where the definition says so.

StrategyEngine.evaluate applies the typed filters {feature, min, max}: null -> failed,
otherwise min <= value <= max with null bounds meaning unbounded. No operator strings.
"""

from __future__ import annotations

from dataclasses import dataclass
from decimal import ROUND_HALF_EVEN, Decimal
from typing import TYPE_CHECKING, Any

from darkbot.config import StrategyConfig
from darkbot.domain import LAMPORTS_PER_SOL, NANOSECONDS_PER_MILLISECOND, CoverageState, DecisionAction, decimal_str

if TYPE_CHECKING:
    from darkbot.market import TokenTracker

GAP_FLAGS = ("GAP", "AMBIGUOUS_EVENT")


class FeatureCalculator:
    def __init__(self, contract: dict[str, Any], research_quote_lamports: int | None = None) -> None:
        spec = contract["phase0_fixture_contract"]
        self.names: tuple[str, ...] = tuple(contract["feature_contract_v1"]["definitions"])
        self.boolean_feature: str = contract["feature_contract_v1"]["boolean_feature"]
        self.first_buyer_window = int(spec["first_buyer_window"])
        self.quantum = Decimal(1).scaleb(-int(spec["ratio_decimal_places"]))
        self.lamports_per_sol = Decimal(LAMPORTS_PER_SOL)
        self.research_quote_lamports = research_quote_lamports

    def ratio(self, numerator: Decimal, denominator: Decimal) -> Decimal:
        return (numerator / denominator).quantize(self.quantum, rounding=ROUND_HALF_EVEN)

    def price(self, vsol: int | None, vtok: int | None) -> Decimal | None:
        if vsol is None or vtok is None or vsol <= 0 or vtok <= 0:
            return None
        return self.ratio(Decimal(vsol), Decimal(vtok))

    def snapshot(self, tracker: "TokenTracker") -> dict[str, Any]:
        """All 17 features in contract order; values are int, decimal string, bool or None."""
        t = tracker
        buy_volume, sell_volume = Decimal(t.buy_lamports), Decimal(t.sell_lamports)
        price_t0 = self.price(t.t0_vsol, t.t0_vtok)
        price_t = self.price(t.last_vsol, t.last_vtok)
        values: dict[str, Any] = {
            "buy_count": t.buy_count,
            "sell_count": t.sell_count,
            "buy_volume_sol": str(buy_volume / self.lamports_per_sol),
            "sell_volume_sol": str(sell_volume / self.lamports_per_sol),
            "net_buy_flow_sol": str((buy_volume - sell_volume) / self.lamports_per_sol),
            "unique_buyers": len(t.per_buyer),
            "unique_buyer_ratio": decimal_str(self.ratio(Decimal(len(t.per_buyer)), Decimal(t.buy_count))) if t.buy_count else None,
            "largest_buy_sol": str(Decimal(t.largest_buy_lamports) / self.lamports_per_sol) if t.buy_count else None,
            "buyer_volume_concentration": decimal_str(self._hhi(t.per_buyer, buy_volume)) if buy_volume else None,
            "first5_buyer_concentration": decimal_str(self.ratio(sum((Decimal(t.per_buyer[b]) for b in t.first_buyers[: self.first_buyer_window]), Decimal(0)), buy_volume)) if buy_volume else None,
            "first_sell_latency_ms": str(Decimal(t.first_sell_offset_ns - t.t0_offset_ns) / Decimal(NANOSECONDS_PER_MILLISECOND)) if t.first_sell_offset_ns is not None else None,
            "price_change": decimal_str((price_t / price_t0 - 1).quantize(self.quantum, rounding=ROUND_HALF_EVEN)) if price_t0 is not None and price_t is not None else None,
            "max_drawdown": decimal_str(t.max_drawdown) if price_t0 is not None else None,
            "dev_initial_buy": decimal_str(self.ratio(Decimal(t.initial_buy_tokens), Decimal(t.token_total_supply))) if t.initial_buy_tokens is not None and t.token_total_supply else None,
            "curve_progress_pct": decimal_str(self._curve_progress(t)),
            "research_estimated_price_impact": decimal_str(self._impact(t.last_vsol, t.last_vtok, price_t)),
            "provider_gap_flag": t.coverage_state != CoverageState.OBSERVED or any(flag in t.flags for flag in GAP_FLAGS),
        }
        return {name: values[name] for name in self.names}

    def _hhi(self, per_buyer: dict[str, int], total: Decimal) -> Decimal:
        return sum((self.ratio(Decimal(volume), total) ** 2 for volume in per_buyer.values()), Decimal(0)).quantize(self.quantum, rounding=ROUND_HALF_EVEN)

    def _curve_progress(self, t: "TokenTracker") -> Decimal | None:
        if t.last_vtok is None or t.initial_vtok is None or not t.initial_real_tok:
            return None
        real = Decimal(t.last_vtok - (t.initial_vtok - t.initial_real_tok))
        return (100 * (1 - real / Decimal(t.initial_real_tok))).quantize(self.quantum, rounding=ROUND_HALF_EVEN)

    def _impact(self, vsol: int | None, vtok: int | None, price_t: Decimal | None) -> Decimal | None:
        quote = self.research_quote_lamports
        if quote is None or price_t is None or vsol is None or vtok is None:
            return None
        tokens_out = vtok - (vsol * vtok) // (vsol + quote)
        if tokens_out <= 0:
            return None
        average = self.ratio(Decimal(quote), Decimal(tokens_out))
        return (average / price_t - 1).quantize(self.quantum, rounding=ROUND_HALF_EVEN)


@dataclass(frozen=True, slots=True)
class FilterEvaluation:
    feature: str
    min: Decimal | None
    max: Decimal | None
    value: Any
    passed: bool

    def as_json(self) -> dict[str, Any]:
        return {"feature": self.feature, "min": decimal_str(self.min), "max": decimal_str(self.max), "value": self.value, "passed": self.passed}


@dataclass(frozen=True, slots=True)
class Decision:
    action: DecisionAction
    first_failed_filter: str | None
    evaluations: tuple[FilterEvaluation, ...]
    data_quality_flags: tuple[str, ...]


class StrategyEngine:
    """Deterministic filter evaluation; no database, no provider, no RPC."""

    def __init__(self, config: StrategyConfig) -> None:
        self.config = config

    def evaluate(self, features: dict[str, Any], data_quality_flags: tuple[str, ...] = ()) -> Decision:
        evaluations: list[FilterEvaluation] = []
        first_failed: str | None = None
        for rule in self.config.filters:
            value = features.get(rule.feature)
            passed = False
            if value is not None and not isinstance(value, bool):
                number = Decimal(value)
                passed = (rule.min is None or number >= rule.min) and (rule.max is None or number <= rule.max)
            evaluations.append(FilterEvaluation(rule.feature, rule.min, rule.max, value, passed))
            if not passed and first_failed is None:
                first_failed = rule.feature
        if any(flag in GAP_FLAGS or flag == "MISSING_FIELD" for flag in data_quality_flags) or features.get("provider_gap_flag") is True:
            action = DecisionAction.DATA_INCOMPLETE
        elif first_failed is not None:
            action = DecisionAction.REJECT_FILTER
        else:
            action = DecisionAction.PASS
        return Decision(action, first_failed, tuple(evaluations), tuple(data_quality_flags))
