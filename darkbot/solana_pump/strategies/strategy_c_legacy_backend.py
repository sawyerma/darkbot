"""Strategy C - LEGACY_BACKEND_FILTER_V1: the complete trading policy of the old backend.

Port of old_system/backend/darkbot/{market.TokenTracker, strategy.FeatureCalculator,
strategy.StrategyEngine, paper.PaperRuntime} onto the shared StrategyPolicy interface.
Only the policy lives here; feeds, dedup, coverage, fees, quotes, fills, retries, storage and
replay are the shared modern engine.

Policy (old paper.py / execution.py semantics):
  preselect : actionable SOL CREATE -> observe observation_ms -> freeze the 17 features at the
              exact cutoff (receive <= deadline) -> every typed filter {feature, min, max} must
              pass; NULL feature value = failed; coverage gap = DATA_INCOMPLETE (no entry).
  entry     : one decision per mint, slot busy = discarded for good; entry is dropped at no cost
              when the creator sold up to the modeled entry fill (default LEGACY_FREE_CANCEL).
              Optional TX_LANDS keeps the submitted BUY in normal modeled-fill evaluation;
              it does not guarantee a fill or bypass costs, coverage or slippage checks.
  exit      : CREATOR_SELL > TARGET > STOP on the same state, TIMEOUT from the successful fill.
              Creator exit is hard-wired (never configurable in the old backend).
              stop_net_multiple == 0 still fires when the net realizable value is <= 0.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from decimal import ROUND_HALF_EVEN, Decimal
from typing import Any

from darkbot.solana_pump.domain import LAMPORTS_PER_SOL, NS_PER_MS, Completeness, QuoteScopeStatus, VenueState
from darkbot.solana_pump.strategy import FEATURE_NAMES, entry_admission, stop_reached, target_reached

BOOLEAN_FEATURE = "provider_gap_flag"
# Old PAPER never passed research_quote_lamports into MarketDataService (main.py), so this feature was
# always NULL live and a filter on it could never pass. Rejected at config load instead of never trading.
ALWAYS_NULL_IN_LEGACY_PAPER = "research_estimated_price_impact"
FILTERABLE_FEATURES = tuple(n for n in FEATURE_NAMES if n not in (BOOLEAN_FEATURE, ALWAYS_NULL_IN_LEGACY_PAPER))

PASS, REJECT_FILTER, DATA_INCOMPLETE = "PASS", "REJECT_FILTER", "DATA_INCOMPLETE"


def _dstr(value: Decimal | None) -> str | None:
    return None if value is None else str(value)


@dataclass(frozen=True, slots=True)
class FilterRule:
    feature: str
    min: Decimal | None
    max: Decimal | None


def parse_filters(raw_filters: list[Any]) -> tuple[FilterRule, ...]:
    """Typed filters exactly like old config.strategy_from_content; raises ValueError on any deviation."""
    rules: list[FilterRule] = []
    seen: set[str] = set()
    for index, rule in enumerate(raw_filters):
        if not isinstance(rule, dict) or set(rule) != {"feature", "min", "max"}:
            raise ValueError(f"filters[{index}] must be {{feature, min, max}}")
        feature = rule["feature"]
        if feature == ALWAYS_NULL_IN_LEGACY_PAPER:
            raise ValueError(f"filters[{index}]: {feature} was always NULL in the old PAPER runtime and can never pass")
        if feature not in FILTERABLE_FEATURES or feature in seen:
            raise ValueError(f"filters[{index}].feature {feature!r} is not a filterable feature or is duplicated")
        seen.add(feature)
        bounds: list[Decimal | None] = []
        for key in ("min", "max"):
            value = rule[key]
            if value is None:
                bounds.append(None)
                continue
            if isinstance(value, (bool, float)):
                raise ValueError(f"filters[{index}].{key} must be an exact decimal (int or string), not {type(value).__name__}")
            try:
                bounds.append(Decimal(value))
            except Exception as exc:  # noqa: BLE001 - reported as config error by the caller
                raise ValueError(f"filters[{index}].{key} is not a decimal: {value!r}") from exc
        if bounds[0] is not None and bounds[1] is not None and bounds[0] > bounds[1]:
            raise ValueError(f"filters[{index}]: min > max")
        rules.append(FilterRule(feature, bounds[0], bounds[1]))
    return tuple(rules)


@dataclass
class LegacyTracker:
    """Per-mint O(1) aggregates, field for field the old TokenTracker (only what the 17 features need)."""

    mint: str
    signature: str  # CREATE transaction: its trades (creator initial buy) never enter any counter
    start: int
    end: int
    quantum: Decimal
    first_buyer_window: int
    t0_vsol: int | None  # reserves AFTER the create transaction (post initial buy), the old t0 price basis
    t0_vtok: int | None
    initial_vtok: int | None  # CreateEvent reserves (pre initial buy), for curve_progress_pct
    initial_real_tok: int | None
    token_total_supply: int | None
    initial_buy_tokens: int | None
    buy_count: int = 0
    sell_count: int = 0
    buy_lamports: int = 0
    sell_lamports: int = 0
    per_buyer: dict[str, int] = field(default_factory=dict)
    first_buyers: list[str] = field(default_factory=list)
    largest_buy_lamports: int = 0
    first_sell_mono_ns: int | None = None
    last_vsol: int | None = None
    last_vtok: int | None = None
    price_peak: Decimal | None = None
    max_drawdown: Decimal = Decimal(0)

    def __post_init__(self) -> None:
        self.last_vsol, self.last_vtok = self.t0_vsol, self.t0_vtok
        self.price_peak = self._price(self.t0_vsol, self.t0_vtok)

    @property
    def buyers(self) -> set[str]:  # shared strategy_observations columns (buy_count / unique_buyers)
        return set(self.per_buyer)

    def _ratio(self, numerator: Decimal, denominator: Decimal) -> Decimal:
        return (numerator / denominator).quantize(self.quantum, rounding=ROUND_HALF_EVEN)

    def _price(self, vsol: int | None, vtok: int | None) -> Decimal | None:
        if vsol is None or vtok is None or vsol <= 0 or vtok <= 0:
            return None
        return self._ratio(Decimal(vsol), Decimal(vtok))

    def apply(self, event: Any, receive_mono_ns: int) -> None:
        lamports = event.sol_amount or 0
        actor = event.actor or ""
        if event.event_type.value == "BUY":
            self.buy_count += 1
            self.buy_lamports += lamports
            self.per_buyer[actor] = self.per_buyer.get(actor, 0) + lamports
            if actor not in self.first_buyers and len(self.first_buyers) < self.first_buyer_window:
                self.first_buyers.append(actor)
            self.largest_buy_lamports = max(self.largest_buy_lamports, lamports)
        else:
            self.sell_count += 1
            self.sell_lamports += lamports
            if self.first_sell_mono_ns is None:
                self.first_sell_mono_ns = receive_mono_ns
        if event.virtual_sol_reserves is not None and event.virtual_token_reserves is not None:
            self.last_vsol, self.last_vtok = event.virtual_sol_reserves, event.virtual_token_reserves
            price = self._price(self.last_vsol, self.last_vtok)
            if price is not None:
                self.price_peak = price if self.price_peak is None else max(self.price_peak, price)
                drawdown = (1 - price / self.price_peak).quantize(self.quantum, rounding=ROUND_HALF_EVEN)
                self.max_drawdown = max(self.max_drawdown, drawdown)

    def snapshot(self, provider_gap_flag: bool) -> dict[str, Any]:
        """The 17 contract features in contract order; int, decimal string, bool or None (old FeatureCalculator.snapshot)."""
        lps = Decimal(LAMPORTS_PER_SOL)
        buy_volume, sell_volume = Decimal(self.buy_lamports), Decimal(self.sell_lamports)
        price_t0 = self._price(self.t0_vsol, self.t0_vtok)
        price_t = self._price(self.last_vsol, self.last_vtok)
        curve_progress = None
        if self.last_vtok is not None and self.initial_vtok is not None and self.initial_real_tok:
            real = Decimal(self.last_vtok - (self.initial_vtok - self.initial_real_tok))
            curve_progress = (100 * (1 - real / Decimal(self.initial_real_tok))).quantize(self.quantum, rounding=ROUND_HALF_EVEN)
        hhi = None
        if buy_volume:
            hhi = sum((self._ratio(Decimal(v), buy_volume) ** 2 for v in self.per_buyer.values()), Decimal(0)).quantize(self.quantum, rounding=ROUND_HALF_EVEN)
        values: dict[str, Any] = {
            "buy_count": self.buy_count,
            "sell_count": self.sell_count,
            "buy_volume_sol": str(buy_volume / lps),
            "sell_volume_sol": str(sell_volume / lps),
            "net_buy_flow_sol": str((buy_volume - sell_volume) / lps),
            "unique_buyers": len(self.per_buyer),
            "unique_buyer_ratio": _dstr(self._ratio(Decimal(len(self.per_buyer)), Decimal(self.buy_count))) if self.buy_count else None,
            "largest_buy_sol": str(Decimal(self.largest_buy_lamports) / lps) if self.buy_count else None,
            "buyer_volume_concentration": _dstr(hhi),
            "first5_buyer_concentration": _dstr(self._ratio(sum((Decimal(self.per_buyer[b]) for b in self.first_buyers[: self.first_buyer_window]), Decimal(0)), buy_volume)) if buy_volume else None,
            "first_sell_latency_ms": str(Decimal(self.first_sell_mono_ns - self.start) / Decimal(NS_PER_MS)) if self.first_sell_mono_ns is not None else None,
            "price_change": _dstr((price_t / price_t0 - 1).quantize(self.quantum, rounding=ROUND_HALF_EVEN)) if price_t0 is not None and price_t is not None else None,
            "max_drawdown": _dstr(self.max_drawdown) if price_t0 is not None else None,
            "dev_initial_buy": _dstr(self._ratio(Decimal(self.initial_buy_tokens), Decimal(self.token_total_supply))) if self.initial_buy_tokens is not None and self.token_total_supply else None,
            "curve_progress_pct": _dstr(curve_progress),
            ALWAYS_NULL_IN_LEGACY_PAPER: None,
            BOOLEAN_FEATURE: provider_gap_flag,
        }
        return {name: values[name] for name in FEATURE_NAMES}


def evaluate_filters(rules: tuple[FilterRule, ...], features: dict[str, Any]) -> tuple[str, str | None, list[dict[str, Any]]]:
    """Old StrategyEngine.evaluate: NULL -> failed, otherwise min <= value <= max with null bounds unbounded."""
    evaluations: list[dict[str, Any]] = []
    first_failed: str | None = None
    for rule in rules:
        value = features.get(rule.feature)
        passed = False
        if value is not None and not isinstance(value, bool):
            number = Decimal(value)
            passed = (rule.min is None or number >= rule.min) and (rule.max is None or number <= rule.max)
        evaluations.append({"feature": rule.feature, "min": _dstr(rule.min), "max": _dstr(rule.max), "value": value, "passed": passed})
        if not passed and first_failed is None:
            first_failed = rule.feature
    if features.get(BOOLEAN_FEATURE) is True:
        return DATA_INCOMPLETE, first_failed, evaluations
    if first_failed is not None:
        return REJECT_FILTER, first_failed, evaluations
    return PASS, None, evaluations


class LegacyBackendFilter:
    strategy_id = "LEGACY_BACKEND_FILTER_V1"

    def __init__(self, config):
        self.config = config
        self.rules = parse_filters(config["filters"])
        self.stop = Decimal(str(config["stop_net_multiple"]))
        self.observations: dict[str, LegacyTracker] = {}
        self.decided: set[str] = set()

    # ------------------------------------------------------------------ preselection
    def on_create(self, owner, mint, raw, envelope, decode_done):
        if mint in self.observations or mint in self.decided or mint in owner.submitted_mints:
            return
        state = owner.reducer.states[mint]
        # Same actionable-protocol gate as the other observing policy: only a complete SOL-quoted CREATE is a launch.
        if (envelope.completeness != Completeness.COMPLETE or state.quote_scope_status != QuoteScopeStatus.SOL_WSOL
                or state.venue_state != VenueState.BONDING_CURVE_ACTIVE or not state.curve_known
                or state.quote_uncertain or state.decode_gap_since is not None or state.unsupported_fee_mode
                or state.token_program not in (owner.spl_token_program, owner.cfg.protocol["token_2022_program"])
                or (state.token_program == owner.cfg.protocol["token_2022_program"] and (not state.account_profile_id or not owner.token_2022_supported))):
            return
        research = owner.cfg.research
        start = raw.receive_mono_ns
        obs = LegacyTracker(
            mint, envelope.signature, start, start + self.config["observation_ms"] * NS_PER_MS,
            Decimal(1).scaleb(-int(research["feature_ratio_decimal_places"])), int(research["feature_first_buyer_window"]),
            state.virtual_sol_reserves, state.virtual_token_reserves,  # state after the whole create transaction = old t0 reserves
            state.initial_virtual_token_reserves, state.initial_real_token_reserves, state.token_total_supply, state.initial_buy_tokens,
        )
        if self.config["observation_ms"] == 0:
            # old on_launch: a zero-age strategy is evaluated directly from the validated CREATE, no artificial timer
            self._decide(owner, obs, raw, envelope, decode_done, decode_done)
            return
        self.observations[mint] = obs
        owner.record_observation(obs, None, None)

        def complete(actual):
            if self.observations.pop(mint, None) is not obs:
                return
            self._decide(owner, obs, raw, envelope, obs.end, actual)

        owner.scheduler.schedule_at(obs.end, complete)

    def on_events(self, owner, result, raw, envelope):
        if envelope.completeness != Completeness.COMPLETE:
            return
        for applied in result.events:
            event = applied.event
            obs = self.observations.get(event.mint)
            # exact cutoff: receive <= deadline counts, later never; trades of the CREATE transaction never count
            if (obs is not None and obs.start <= raw.receive_mono_ns <= obs.end and envelope.signature != obs.signature
                    and applied.is_new and applied.validation.value == "VALID" and applied.applied_state
                    and event.event_type.value in ("BUY", "SELL")):
                obs.apply(event, raw.receive_mono_ns)

    def _decide(self, owner, obs, raw, envelope, data_ready, actual):
        mint = obs.mint
        self.decided.add(mint)
        st = owner.reducer.states.get(mint)
        if st is None:
            owner.record_observation(obs, None, actual, status=DATA_INCOMPLETE)
            return
        gap = (owner.reducer.gap_intersects(obs.start, obs.end)
               or any((end is None or end >= obs.start) and start <= obs.end for start, end in st.decode_gap_intervals))
        features = obs.snapshot(provider_gap_flag=bool(gap))
        action, first_failed, evaluations = evaluate_filters(self.rules, features)
        extra = {"features_json": features, "filter_evaluations_json": evaluations, "first_failed_filter": first_failed, "decision_action": action}
        if action == DATA_INCOMPLETE:
            owner.record_observation(obs, None, actual, status=DATA_INCOMPLETE, extra=extra)
            reason = "OBSERVATION_COVERAGE_GAP"
        else:
            owner.record_observation(obs, action == PASS, actual, extra=extra)
            reason = None if action == PASS else f"REJECT_FILTER_{first_failed}"
        owner._entry_decision(mint, raw, envelope, data_ready, policy_rejection=reason)

    # ------------------------------------------------------------------ entry / exit rules
    def admission(self, state, **checks):
        return entry_admission(state, **checks)  # includes CREATOR_ALREADY_SOLD and SLOT_LIMIT (slot busy = discarded for good)

    def cancel_entry_if_creator_sold_before_fill(self):
        # TX_LANDS retains the normal fill checks; a successful BUY then enters
        # the shared creator-sell exit path. Known sells still block admission.
        return self.config.get("creator_sold_before_fill", "LEGACY_FREE_CANCEL") == "LEGACY_FREE_CANCEL"

    def creator_exit_enabled(self):
        return True  # hard-wired in the old PAPER runtime

    def target_reached(self, value, entry_cost, target):
        return target_reached(value, entry_cost, target)

    def stop_reached(self, value, entry_cost):
        return stop_reached(value, entry_cost, self.stop)
