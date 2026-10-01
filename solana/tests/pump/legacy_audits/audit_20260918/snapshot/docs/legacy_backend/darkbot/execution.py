"""Pure Pump bonding-curve execution model for BACKTEST, PAPER and LIVE expectation.

This module owns deterministic quote/fill arithmetic only. It does not read RPC, choose
candidates, reserve risk slots, persist rows, sign transactions or broadcast anything.
Callers inject a session-bound protocol fee snapshot, explicit network-cost assumptions
and explicit latency assumptions. All monetary arithmetic is integer base units; Decimal
is used only for dimensionless diagnostics.
"""

from __future__ import annotations

import enum
from dataclasses import dataclass
from decimal import Decimal

from darkbot.domain import NANOSECONDS_PER_MILLISECOND

BASIS_POINTS = 10_000


class ExecutionModelError(ValueError):
    """The fill model received an invalid state or configuration value."""


class AttemptStatus(str, enum.Enum):
    """Outcome of one submitted paper entry/exit attempt; no retry policy lives here."""

    FILLED = "FILLED"
    SLIPPAGE_REJECTED = "SLIPPAGE_REJECTED"
    CURVE_UNAVAILABLE = "CURVE_UNAVAILABLE"


class ExitSignal(str, enum.Enum):
    """Pure position-value decision; timeout remains the caller's one-shot timer."""

    HOLD = "HOLD"
    TARGET = "TARGET"
    STOP = "STOP"


@dataclass(frozen=True, slots=True)
class FeeRates:
    """One on-chain fee tier. LP bps are retained for audit but not charged on Pump curves."""

    lp_fee_bps: int
    protocol_fee_bps: int
    creator_fee_bps: int

    def __post_init__(self) -> None:
        for name, value in (
            ("lp_fee_bps", self.lp_fee_bps),
            ("protocol_fee_bps", self.protocol_fee_bps),
            ("creator_fee_bps", self.creator_fee_bps),
        ):
            if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value <= BASIS_POINTS:
                raise ExecutionModelError(f"{name} must be an int within 0..{BASIS_POINTS}")


@dataclass(frozen=True, slots=True)
class FeeTier:
    """Fee rates active at and above one market-cap threshold, in quote base units."""

    market_cap_lamports_threshold: int
    fees: FeeRates

    def __post_init__(self) -> None:
        if isinstance(self.market_cap_lamports_threshold, bool) or not isinstance(self.market_cap_lamports_threshold, int) or self.market_cap_lamports_threshold < 0:
            raise ExecutionModelError("market_cap_lamports_threshold must be a non-negative int")


@dataclass(frozen=True, slots=True)
class ProtocolFeeConfig:
    """Read-only snapshot of Pump FeeConfig for SOL bonding curves; never a strategy knob."""

    fee_tiers: tuple[FeeTier, ...]
    account: str
    read_slot: int | None = None
    read_utc_ns: int | None = None

    def __post_init__(self) -> None:
        if not self.fee_tiers:
            raise ExecutionModelError("fee_tiers must not be empty")
        thresholds = tuple(tier.market_cap_lamports_threshold for tier in self.fee_tiers)
        if thresholds != tuple(sorted(thresholds)):
            raise ExecutionModelError("fee_tiers must be sorted by ascending market-cap threshold")
        if not self.account:
            raise ExecutionModelError("protocol fee config account must not be empty")

    def rates_for_market_cap(self, market_cap_lamports: int) -> FeeRates:
        """Return the highest threshold not above market cap, matching Pump fee-tier semantics."""
        if market_cap_lamports < 0:
            raise ExecutionModelError("market_cap_lamports must be non-negative")
        selected = self.fee_tiers[0]
        for tier in self.fee_tiers:
            if market_cap_lamports < tier.market_cap_lamports_threshold:
                break
            selected = tier
        return selected.fees


@dataclass(frozen=True, slots=True)
class NetworkCostAssumptions:
    """Per-submitted-transaction Solana costs; account rent is supplied per fill attempt."""

    base_fee_lamports: int
    priority_fee_lamports: int

    def __post_init__(self) -> None:
        for name, value in (("base_fee_lamports", self.base_fee_lamports), ("priority_fee_lamports", self.priority_fee_lamports)):
            if isinstance(value, bool) or not isinstance(value, int) or value < 0:
                raise ExecutionModelError(f"{name} must be a non-negative int")

    @property
    def per_attempt_lamports(self) -> int:
        return self.base_fee_lamports + self.priority_fee_lamports


@dataclass(frozen=True, slots=True)
class LatencyAssumptions:
    """Versioned assumed/measured path timing. No random latency is invented in the model."""

    decision_to_build_ms: int
    build_to_broadcast_ms: int
    landing_ms: int
    exit_landing_ms: int
    status: str

    def __post_init__(self) -> None:
        for name, value in (
            ("decision_to_build_ms", self.decision_to_build_ms),
            ("build_to_broadcast_ms", self.build_to_broadcast_ms),
            ("landing_ms", self.landing_ms),
            ("exit_landing_ms", self.exit_landing_ms),
        ):
            if isinstance(value, bool) or not isinstance(value, int) or value < 0:
                raise ExecutionModelError(f"{name} must be a non-negative int")
        if self.status not in {"ASSUMED", "MEASURED"}:
            raise ExecutionModelError("latency status must be ASSUMED or MEASURED")

    @property
    def entry_total_ms(self) -> int:
        return self.decision_to_build_ms + self.build_to_broadcast_ms + self.landing_ms

    @property
    def exit_total_ms(self) -> int:
        return self.decision_to_build_ms + self.build_to_broadcast_ms + self.exit_landing_ms


@dataclass(frozen=True, slots=True)
class ObservedCurveState:
    """One OBSERVED_STATE_PROXY; it is an observed proxy, never claimed as exact landing state."""

    virtual_sol_reserves: int
    virtual_token_reserves: int
    real_token_reserves: int
    fee_market_cap_supply: int
    creator_fee_enabled: bool
    receive_offset_ns: int
    signature: str
    slot: int | None
    receive_sequence: int

    def __post_init__(self) -> None:
        for name, value in (
            ("virtual_sol_reserves", self.virtual_sol_reserves),
            ("virtual_token_reserves", self.virtual_token_reserves),
            ("real_token_reserves", self.real_token_reserves),
            ("fee_market_cap_supply", self.fee_market_cap_supply),
            ("receive_offset_ns", self.receive_offset_ns),
            ("receive_sequence", self.receive_sequence),
        ):
            if isinstance(value, bool) or not isinstance(value, int) or value < 0:
                raise ExecutionModelError(f"{name} must be a non-negative int")
        if not isinstance(self.creator_fee_enabled, bool):
            raise ExecutionModelError("creator_fee_enabled must be bool")

    @property
    def curve_available(self) -> bool:
        return self.virtual_sol_reserves > 0 and self.virtual_token_reserves > 0 and self.real_token_reserves > 0

    @property
    def market_cap_lamports(self) -> int:
        if self.virtual_token_reserves <= 0:
            raise ExecutionModelError("cannot compute market cap with zero virtual token reserves")
        return self.virtual_sol_reserves * self.fee_market_cap_supply // self.virtual_token_reserves


@dataclass(frozen=True, slots=True)
class FeeBreakdown:
    protocol_lamports: int
    creator_lamports: int

    @property
    def total_lamports(self) -> int:
        return self.protocol_lamports + self.creator_lamports


@dataclass(frozen=True, slots=True)
class BuyQuote:
    """Exact-quote-in Pump bonding-curve quote for a fixed spendable SOL budget."""

    spendable_lamports: int
    net_sol_lamports: int
    tokens_out: int
    fees: FeeBreakdown
    spent_lamports: int
    unspent_lamports: int
    capped_by_real_reserves: bool


@dataclass(frozen=True, slots=True)
class SellQuote:
    """Pump bonding-curve sell quote after protocol/creator fees, before network fee."""

    token_amount: int
    gross_sol_lamports: int
    fees: FeeBreakdown
    net_sol_lamports: int


@dataclass(frozen=True, slots=True)
class EntryAttempt:
    """One paper entry submission evaluated at decision and fill OBSERVED_STATE_PROXY states."""

    status: AttemptStatus
    decision_offset_ns: int
    fill_offset_ns: int
    min_tokens_out: int
    expected_quote: BuyQuote
    fill_quote: BuyQuote
    network_fee_lamports: int
    account_rent_lamports: int
    entry_cost_lamports: int
    proxy: ObservedCurveState

    @property
    def filled(self) -> bool:
        return self.status == AttemptStatus.FILLED


@dataclass(frozen=True, slots=True)
class ExitAttempt:
    """One paper exit submission; retry policy is intentionally outside this value object."""

    status: AttemptStatus
    trigger_offset_ns: int
    fill_offset_ns: int
    min_sol_output_lamports: int
    expected_quote: SellQuote
    fill_quote: SellQuote
    network_fee_lamports: int
    net_return_lamports: int
    proxy: ObservedCurveState

    @property
    def filled(self) -> bool:
        return self.status == AttemptStatus.FILLED


class PaperFillModel:
    """Deterministic Pump curve quote/fill model shared by Backtest, TESTMODE and LIVE expectation.

    It deliberately does not select the proxy state. The caller must supply the last observed
    state whose receive_offset is <= the modeled fill time, preserving the OBSERVED_STATE_PROXY
    contract and making live/replay use the same arithmetic.
    """

    def __init__(self, protocol_fees: ProtocolFeeConfig, network_costs: NetworkCostAssumptions, latency: LatencyAssumptions) -> None:
        self.protocol_fees = protocol_fees
        self.network_costs = network_costs
        self.latency = latency

    def entry_fill_offset_ns(self, decision_offset_ns: int) -> int:
        return decision_offset_ns + self.latency.entry_total_ms * NANOSECONDS_PER_MILLISECOND

    def exit_fill_offset_ns(self, trigger_offset_ns: int) -> int:
        return trigger_offset_ns + self.latency.exit_total_ms * NANOSECONDS_PER_MILLISECOND

    def quote_buy(self, state: ObservedCurveState, spendable_lamports: int) -> BuyQuote:
        """Mirror current Pump exact-quote-in integer math; fees come from the injected FeeConfig."""
        self._non_negative_int(spendable_lamports, "spendable_lamports")
        if spendable_lamports == 0 or not state.curve_available:
            return BuyQuote(spendable_lamports, 0, 0, FeeBreakdown(0, 0), 0, spendable_lamports, False)

        rates = self._active_rates(state)
        creator_bps = rates.creator_fee_bps if state.creator_fee_enabled else 0
        total_fee_bps = rates.protocol_fee_bps + creator_bps
        net_sol = spendable_lamports * BASIS_POINTS // (BASIS_POINTS + total_fee_bps)
        fees = self._fees_for(net_sol, rates.protocol_fee_bps, creator_bps)
        over = net_sol + fees.total_lamports - spendable_lamports
        if over > 0:
            net_sol = max(0, net_sol - over)
        if net_sol <= 1:
            return BuyQuote(spendable_lamports, net_sol, 0, fees, net_sol + fees.total_lamports, spendable_lamports - (net_sol + fees.total_lamports), False)

        curve_input = net_sol - 1
        raw_tokens = curve_input * state.virtual_token_reserves // (state.virtual_sol_reserves + curve_input)
        capped = raw_tokens > state.real_token_reserves
        tokens = min(raw_tokens, state.real_token_reserves)
        spent = net_sol + fees.total_lamports
        return BuyQuote(spendable_lamports, net_sol, tokens, fees, spent, spendable_lamports - spent, capped)

    def quote_sell(self, state: ObservedCurveState, token_amount: int) -> SellQuote:
        """Mirror Pump sell integer math: curve gross output, then ceiling-rounded fees."""
        self._non_negative_int(token_amount, "token_amount")
        if token_amount == 0 or not state.curve_available:
            return SellQuote(token_amount, 0, FeeBreakdown(0, 0), 0)
        gross = token_amount * state.virtual_sol_reserves // (state.virtual_token_reserves + token_amount)
        rates = self._active_rates(state)
        creator_bps = rates.creator_fee_bps if state.creator_fee_enabled else 0
        fees = self._fees_for(gross, rates.protocol_fee_bps, creator_bps)
        return SellQuote(token_amount, gross, fees, max(0, gross - fees.total_lamports))

    def simulate_entry(
        self,
        decision_state: ObservedCurveState,
        fill_state: ObservedCurveState,
        decision_offset_ns: int,
        spendable_lamports: int,
        max_slippage_bps: int,
        account_rent_lamports: int = 0,
    ) -> EntryAttempt:
        """Evaluate one submitted exact-quote-in entry; a failed landed attempt still pays network fee."""
        self._slippage_bps(max_slippage_bps)
        self._non_negative_int(account_rent_lamports, "account_rent_lamports")
        fill_offset_ns = self.entry_fill_offset_ns(decision_offset_ns)
        if fill_state.receive_offset_ns > fill_offset_ns:
            raise ExecutionModelError("fill_state is later than modeled entry fill time")
        expected = self.quote_buy(decision_state, spendable_lamports)
        actual = self.quote_buy(fill_state, spendable_lamports)
        min_tokens = expected.tokens_out * (BASIS_POINTS - max_slippage_bps) // BASIS_POINTS
        network = self.network_costs.per_attempt_lamports

        if actual.tokens_out <= 0 or actual.capped_by_real_reserves:
            return EntryAttempt(AttemptStatus.CURVE_UNAVAILABLE, decision_offset_ns, fill_offset_ns, min_tokens, expected, actual, network, 0, network, fill_state)
        if actual.tokens_out < min_tokens:
            return EntryAttempt(AttemptStatus.SLIPPAGE_REJECTED, decision_offset_ns, fill_offset_ns, min_tokens, expected, actual, network, 0, network, fill_state)
        entry_cost = actual.spent_lamports + network + account_rent_lamports
        return EntryAttempt(AttemptStatus.FILLED, decision_offset_ns, fill_offset_ns, min_tokens, expected, actual, network, account_rent_lamports, entry_cost, fill_state)

    def simulate_exit(
        self,
        trigger_state: ObservedCurveState,
        fill_state: ObservedCurveState,
        trigger_offset_ns: int,
        token_amount: int,
        max_slippage_bps: int,
    ) -> ExitAttempt:
        """Evaluate one sell attempt using min_sol_output semantics; failed attempts pay network fee."""
        self._slippage_bps(max_slippage_bps)
        fill_offset_ns = self.exit_fill_offset_ns(trigger_offset_ns)
        if fill_state.receive_offset_ns > fill_offset_ns:
            raise ExecutionModelError("fill_state is later than modeled exit fill time")
        expected = self.quote_sell(trigger_state, token_amount)
        actual = self.quote_sell(fill_state, token_amount)
        min_output = expected.net_sol_lamports * (BASIS_POINTS - max_slippage_bps) // BASIS_POINTS
        network = self.network_costs.per_attempt_lamports

        if actual.net_sol_lamports <= 0:
            return ExitAttempt(AttemptStatus.CURVE_UNAVAILABLE, trigger_offset_ns, fill_offset_ns, min_output, expected, actual, network, -network, fill_state)
        if actual.net_sol_lamports < min_output:
            return ExitAttempt(AttemptStatus.SLIPPAGE_REJECTED, trigger_offset_ns, fill_offset_ns, min_output, expected, actual, network, -network, fill_state)
        return ExitAttempt(AttemptStatus.FILLED, trigger_offset_ns, fill_offset_ns, min_output, expected, actual, network, actual.net_sol_lamports - network, fill_state)

    def net_exit_value_lamports(self, state: ObservedCurveState, token_amount: int) -> int:
        """Net realizable value for target/stop evaluation, including one modeled exit network fee."""
        quote = self.quote_sell(state, token_amount)
        return quote.net_sol_lamports - self.network_costs.per_attempt_lamports

    def exit_signal(self, state: ObservedCurveState, token_amount: int, entry_cost_lamports: int, target_multiple: Decimal, stop_multiple: Decimal) -> ExitSignal:
        """Target/stop use net realizable value, never chart price."""
        self._non_negative_int(entry_cost_lamports, "entry_cost_lamports")
        if target_multiple <= 1:
            raise ExecutionModelError("target_multiple must be > 1")
        if not Decimal(0) <= stop_multiple < Decimal(1):
            raise ExecutionModelError("stop_multiple must be within [0, 1)")
        value = Decimal(self.net_exit_value_lamports(state, token_amount))
        basis = Decimal(entry_cost_lamports)
        if value >= basis * target_multiple:
            return ExitSignal.TARGET
        if value <= basis * stop_multiple:
            return ExitSignal.STOP
        return ExitSignal.HOLD

    def entry_price_impact(self, state: ObservedCurveState, quote: BuyQuote) -> Decimal | None:
        """Average fill price / pre-trade marginal price - 1; diagnostic only, not curve arithmetic."""
        if quote.tokens_out <= 0 or state.virtual_sol_reserves <= 0 or state.virtual_token_reserves <= 0:
            return None
        average = Decimal(quote.net_sol_lamports) / Decimal(quote.tokens_out)
        marginal = Decimal(state.virtual_sol_reserves) / Decimal(state.virtual_token_reserves)
        return average / marginal - Decimal(1)

    def observed_price_slippage(self, decision_state: ObservedCurveState, fill_state: ObservedCurveState) -> Decimal | None:
        """Observed marginal-price movement between decision and fill proxies; diagnostic only."""
        if min(decision_state.virtual_sol_reserves, decision_state.virtual_token_reserves, fill_state.virtual_sol_reserves, fill_state.virtual_token_reserves) <= 0:
            return None
        decision_price = Decimal(decision_state.virtual_sol_reserves) / Decimal(decision_state.virtual_token_reserves)
        fill_price = Decimal(fill_state.virtual_sol_reserves) / Decimal(fill_state.virtual_token_reserves)
        return fill_price / decision_price - Decimal(1)

    def _active_rates(self, state: ObservedCurveState) -> FeeRates:
        return self.protocol_fees.rates_for_market_cap(state.market_cap_lamports)

    @staticmethod
    def _fees_for(amount: int, protocol_bps: int, creator_bps: int) -> FeeBreakdown:
        return FeeBreakdown(PaperFillModel._ceil_fee(amount, protocol_bps), PaperFillModel._ceil_fee(amount, creator_bps))

    @staticmethod
    def _ceil_fee(amount: int, bps: int) -> int:
        if amount <= 0 or bps <= 0:
            return 0
        return (amount * bps + BASIS_POINTS - 1) // BASIS_POINTS

    @staticmethod
    def _non_negative_int(value: int, name: str) -> None:
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            raise ExecutionModelError(f"{name} must be a non-negative int")

    @staticmethod
    def _slippage_bps(value: int) -> None:
        PaperFillModel._non_negative_int(value, "max_slippage_bps")
        if value > BASIS_POINTS:
            raise ExecutionModelError(f"max_slippage_bps must be <= {BASIS_POINTS}")
