"""Local QuoteEngine, fee application/rounding, attempt states, slippage, retry rules and ledger
booking rules (spec: execution.py). One shared implementation for PAPER, Replay and Backtest.
No feed, SQLite or RPC access here; fees.py supplies snapshots, this module applies them.
"""

from __future__ import annotations

from dataclasses import dataclass
from decimal import Decimal
from typing import Any

from darkbot.domain import (
    BASIS_POINTS, NS_PER_MS, AttemptStatus, CostBasis, CostEvidence, CoverageState, CurveState, InvalidQuote, LedgerRecord, QuoteSnapshot, Side, VenueState, utc_iso,
)


class LiveBroadcastForbidden(RuntimeError):
    """Raised by every path that would send a transaction or call a paid trade API (T22)."""


def send_transaction(*_args: Any, **_kwargs: Any) -> None:
    raise LiveBroadcastForbidden("PAPER mode: sendTransaction is a forbidden path (live_broadcast_enabled=false)")


def paid_trade_api_call(*_args: Any, **_kwargs: Any) -> None:
    raise LiveBroadcastForbidden("PAPER mode: paid trade/account API subscriptions are forbidden")


def assert_paper_boundary(mode: str, live_broadcast_enabled: bool) -> None:
    if mode != "PAPER" or live_broadcast_enabled:
        raise LiveBroadcastForbidden(f"mode={mode} live_broadcast_enabled={live_broadcast_enabled}: LIVE is not implemented in phase 1")


def ceil_bps(amount: int, bps: int) -> int:
    return -(-(amount * bps) // BASIS_POINTS) if bps else 0


def priority_lamports(compute_unit_limit: int, micro_lamports_per_cu: int) -> int:
    """legacy/v0 ComputeBudget: ceil(CU-Limit * CU-Preis-in-MicroLamports / 1e6)."""
    return -(-(compute_unit_limit * micro_lamports_per_cu) // 1_000_000)


@dataclass(frozen=True, slots=True)
class ExecutionPolicy:
    slippage_bps: int
    quote_max_age_ns: int
    proxy_state_max_age_ns: int
    max_exit_attempts: int
    exit_retry_backoff_ns: tuple[int, ...]
    entry_max_attempts: int
    signature_count: int
    compute_unit_limit: int
    sender_tip_lamports: int
    tip_evidence: CostEvidence
    new_token_account_per_position: bool
    close_token_account_on_full_exit: bool
    quote_model_hash: str
    cost_model_hash: str
    rounding_version: str
    curve_budget_lamports: int
    target_net_multiple: Decimal
    irrecoverable_cost_lamports: int = 0
    irrecoverable_evidence: CostEvidence = CostEvidence.VALID_ZERO
    spl_token_program: str = ""
    token_2022_program: str = ""
    token_2022_supported: bool = False
    account_profiles: dict[str, Any] | None = None

    @classmethod
    def from_config(cls, cfg: Any) -> "ExecutionPolicy":
        ex, co, st = cfg.execution, cfg.costs, cfg.strategy
        return cls(
            spl_token_program=cfg.protocol["spl_token_program"], token_2022_program=cfg.protocol["token_2022_program"],
            token_2022_supported=ex["token_account_policy"]["token_2022_supported"], account_profiles=ex["token_account_policy"]["profiles"],
            slippage_bps=ex["slippage_bps"], quote_max_age_ns=ex["quote_max_age_ms"] * NS_PER_MS, proxy_state_max_age_ns=ex["quote_max_age_ms"] * NS_PER_MS,
            max_exit_attempts=ex["max_exit_attempts"], exit_retry_backoff_ns=tuple(b * NS_PER_MS for b in ex["exit_retry_backoff_ms"]), entry_max_attempts=ex["entry_max_attempts"],
            signature_count=ex["signature_count"], compute_unit_limit=ex["priority_policy"]["compute_unit_limit"], sender_tip_lamports=co["sender_tip_lamports"],
            tip_evidence=CostEvidence.NOT_APPLICABLE if co["sender_tip_lamports"] == 0 else CostEvidence.CONFIGURED,
            new_token_account_per_position=ex["token_account_policy"]["assume_new_token_account_per_position"],
            close_token_account_on_full_exit=ex["token_account_policy"]["close_token_account_on_full_exit"],
            quote_model_hash=cfg.quote_model_hash, cost_model_hash=cfg.cost_model_hash, rounding_version=cfg.protocol["curve_rounding_version"],
            curve_budget_lamports=st["entry_amount_lamports"], target_net_multiple=Decimal(st["target_net_multiple"]),
            irrecoverable_cost_lamports=co["irrecoverable_account_cost_lamports"] or 0,
            irrecoverable_evidence=CostEvidence.VALID_ZERO if co["irrecoverable_account_cost_provenance"].startswith("VALID_ZERO") else CostEvidence.CONFIGURED,
        )


@dataclass(frozen=True, slots=True)
class FeeTerms:
    protocol_bps: int
    creator_bps: int
    other_bps: int
    base_fee_lamports: int
    priority_fee_lamports: int
    compute_unit_price_micro_lamports: int
    rent_lamports: int
    evidence: str
    ids: dict[str, str]
    token_program: str
    account_profile_id: str | None
    account_size_bytes: int
    message_template_hash: str | None
    instruction_name: str | None


def fee_terms(basis: CostBasis, policy: ExecutionPolicy, now_mono_ns: int, state: CurveState | None = None, side: Side = Side.BUY) -> FeeTerms | InvalidQuote:
    """Assemble cost terms from snapshots valid at now_mono_ns; NULL or stale is UNKNOWN, never zero and never a fill basis."""
    if not basis.complete:
        missing = [n for n, s in (("protocol", basis.protocol), ("network", basis.network), ("rent", basis.rent), ("priority_policy", basis.priority_policy)) if s is None]
        return InvalidQuote("UNQUALIFIED_COSTS", f"missing fee snapshots: {missing}")
    for name, snap in (("protocol", basis.protocol), ("network", basis.network), ("rent", basis.rent), ("priority_policy", basis.priority_policy)):
        assert snap is not None
        if snap.evidence == CostEvidence.UNKNOWN:
            return InvalidQuote("UNQUALIFIED_COSTS", f"{name} fee evidence unknown")
        if snap.validation_status != "VALID":
            return InvalidQuote("UNQUALIFIED_COSTS", f"{name} fee snapshot invalid ({snap.validation_status})")
        if not snap.is_fresh(now_mono_ns):
            return InvalidQuote("UNQUALIFIED_COSTS", f"{name} fee snapshot stale")
    p = basis.protocol.normalized_fee_components
    if p.get("fee_tiers") and state is not None:
        if state.virtual_token_reserves <= 0:
            return InvalidQuote("CURVE_UNAVAILABLE")
        cap = state.virtual_sol_reserves * p["standard_supply_base_units"] // state.virtual_token_reserves
        tier = p["fee_tiers"][0]
        for candidate in p["fee_tiers"]:
            if candidate["market_cap_lamports_threshold"] <= cap:
                tier = candidate
        p = {**p, "protocol_fee_bps": tier["fees"]["protocol_fee_bps"], "creator_fee_bps": tier["fees"]["creator_fee_bps"]}
        if p.get("creator_fee_configurable") and state.creator_fee_bps:
            p["creator_fee_bps"] = state.creator_fee_bps
        if not state.creator_fee_applicable:
            p["creator_fee_bps"] = 0
    n = basis.network.normalized_fee_components
    r = basis.rent.normalized_fee_components
    pp = basis.priority_policy.normalized_fee_components
    program = (state.token_program if state else None) or policy.spl_token_program
    if program not in (policy.spl_token_program, policy.token_2022_program) or (program == policy.token_2022_program and not policy.token_2022_supported):
        return InvalidQuote("UNSUPPORTED_TOKEN_PROGRAM")
    profile = (policy.account_profiles or {}).get(program, {})
    if program == policy.token_2022_program and (state is None or state.account_profile_id != profile.get("profile_id")):
        return InvalidQuote("TOKEN_ACCOUNT_PROFILE_UNVERIFIED")
    if "execution_profiles" in n or "account_profiles" in r:
        n = n.get("execution_profiles", {}).get(program, {}).get(side.value, {})
        r = r.get("account_profiles", {}).get(program, {})
        if (n.get("validation_status") != "VALID" or r.get("validation_status") != "VALID"
                or n.get("profile_id") != profile.get("profile_id") or r.get("profile_id") != profile.get("profile_id")
                or r.get("account_size_bytes") != profile.get("account_size_bytes")):
            return InvalidQuote("UNQUALIFIED_COSTS", "missing or mismatched program/side/account profile")
    elif program != policy.spl_token_program:
        return InvalidQuote("UNQUALIFIED_COSTS", "legacy scalar cost evidence cannot price Token-2022")
    if r.get("account_size_bytes") != profile.get("account_size_bytes"):
        return InvalidQuote("UNQUALIFIED_COSTS", "rent account size disagrees with the selected profile")
    for comp, keys in ((p, ("protocol_fee_bps", "creator_fee_bps")),
                       (n, ("base_fee_lamports", "priority_fee_lamports")),
                       (r, ("token_account_rent_lamports", "account_size_bytes")),
                       (pp, ("compute_unit_price_micro_lamports",))):
        if any(type(comp.get(key)) is not int or comp[key] < 0 for key in keys):
            return InvalidQuote("UNQUALIFIED_COSTS", "missing/invalid selected cost component")
    if n.get("network_fee_total_lamports", n["base_fee_lamports"] + n["priority_fee_lamports"]) != n["base_fee_lamports"] + n["priority_fee_lamports"]:
        return InvalidQuote("UNQUALIFIED_COSTS", "inconsistent selected network fee")
    for comp, source in ((p, "protocol"), (n, "network"), (r, "rent"), (pp, "priority_policy")):
        for key, value in comp.items():
            if value is None and key.endswith("_lamports") or (value is None and key.endswith("_bps")):
                return InvalidQuote("UNQUALIFIED_COSTS", f"{source}.{key} is NULL (unknown, not zero)")
    evidences = {basis.protocol.evidence, basis.network.evidence, basis.rent.evidence, basis.priority_policy.evidence}
    evidence = "MEASURED" if evidences <= {CostEvidence.MEASURED, CostEvidence.KNOWN_RULE} else "CONFIGURED" if CostEvidence.CONFIGURED in evidences else "MODELED"
    return FeeTerms(
        token_program=program, account_profile_id=profile.get("profile_id"), account_size_bytes=int(r["account_size_bytes"]),
        message_template_hash=n.get("message_template_hash", basis.network.raw_values.get("message_template_hash")), instruction_name=n.get("instruction_name"),
        protocol_bps=int(p["protocol_fee_bps"]), creator_bps=int(p["creator_fee_bps"]), other_bps=int(p.get("other_protocol_fee_bps", 0)),
        base_fee_lamports=int(n["base_fee_lamports"]), priority_fee_lamports=int(n["priority_fee_lamports"]),
        compute_unit_price_micro_lamports=int(pp["compute_unit_price_micro_lamports"]), rent_lamports=int(r["token_account_rent_lamports"]), evidence=evidence,
        ids={"protocol": basis.protocol.fee_snapshot_id, "network": basis.network.fee_snapshot_id, "rent": basis.rent.fee_snapshot_id, "priority_policy": basis.priority_policy.fee_snapshot_id},
    )


class QuoteEngine:
    """Pure deterministic Pump bonding-curve quote with exact integer rounding (sizing_contract)."""

    def __init__(self, policy: ExecutionPolicy, stress_network_fee_multiplier: Decimal = Decimal(1), stress_adverse_price_bps: int = 0, stress_exit_reject_rate: Decimal = Decimal(0), stress_seed: int = 0) -> None:
        self.policy = policy
        self._seq = 0
        self.stress_network_fee_multiplier = stress_network_fee_multiplier
        self.stress_adverse_price_bps = stress_adverse_price_bps
        self.stress_exit_reject_rate = stress_exit_reject_rate
        self.stress_seed = stress_seed

    def stress_rejects(self, attempt_key: str) -> bool:
        """Conservative failure scenario: a pre-registered fraction of first exit attempts is rejected, chosen deterministically from seed+attempt key."""
        if self.stress_exit_reject_rate <= 0:
            return False
        import hashlib as _h

        digest = _h.sha256(f"{self.stress_seed}|{attempt_key}".encode()).digest()
        return Decimal(int.from_bytes(digest[:8], "big")) / Decimal(2**64) < self.stress_exit_reject_rate

    def _next_id(self, run_prefix: str) -> str:
        self._seq += 1
        return f"{run_prefix}q{self._seq}"

    @staticmethod
    def tokens_for_budget(vsol: int, vtok: int, rtok: int | None, budget: int) -> tuple[int, int, bool]:
        """Largest token output whose curve input does not exceed the budget; returns (tokens, curve_input, capped)."""
        if budget <= 0 or vsol <= 0 or vtok <= 0:
            return 0, 0, False
        tokens = budget * vtok // (vsol + budget)
        capped = False
        if rtok is not None and tokens > rtok:
            tokens, capped = rtok, True
        if tokens <= 0:
            return 0, 0, capped
        curve_input = (vsol * tokens) // (vtok - tokens) + 1
        if curve_input > budget:  # protect against rounding edge; never exceed the nominal budget
            tokens -= 1
            curve_input = (vsol * tokens) // (vtok - tokens) + 1 if tokens > 0 else 0
        return tokens, curve_input, capped

    @staticmethod
    def sol_for_tokens(vsol: int, vtok: int, tokens: int) -> int:
        if tokens <= 0 or vsol <= 0 or vtok <= 0:
            return 0
        return tokens * vsol // (vtok + tokens)

    def quote_buy(self, state: CurveState, basis: CostBasis, now_mono_ns: int, run_prefix: str, coverage: CoverageState, budget: int | None = None) -> QuoteSnapshot | InvalidQuote:
        if state.venue_state != VenueState.BONDING_CURVE_ACTIVE:
            return InvalidQuote("VENUE_INVALID", state.venue_state.value)
        if coverage == CoverageState.GAP:
            return InvalidQuote("COVERAGE_GAP")
        terms = fee_terms(basis, self.policy, now_mono_ns, state)
        if isinstance(terms, InvalidQuote):
            return terms
        age = now_mono_ns - state.receive_mono_ns
        if age > self.policy.proxy_state_max_age_ns:  # execution.quote_max_age_ms: reserves older than the limit are never a quote basis
            return InvalidQuote("PROXY_STATE_STALE", f"age_ns={age} coverage={coverage.value}")
        budget = self.policy.curve_budget_lamports if budget is None else budget
        tokens, curve_input, capped = self.tokens_for_budget(state.virtual_sol_reserves, state.virtual_token_reserves, state.real_token_reserves, budget)
        if tokens <= 0:
            return InvalidQuote("CURVE_UNAVAILABLE", "no token output for budget")
        proto = ceil_bps(curve_input, terms.protocol_bps)
        creator = ceil_bps(curve_input, terms.creator_bps)
        other = ceil_bps(curve_input, terms.other_bps)
        total = proto + creator + other
        network = terms.base_fee_lamports + terms.priority_fee_lamports
        rent = terms.rent_lamports if self.policy.new_token_account_per_position else 0
        swap_debit = curve_input + total
        # buy_v2 max_sol_cost includes protocol/creator fees (instruction semantics); slippage bound applied to the fee-inclusive debit
        max_input = swap_debit * (BASIS_POINTS + self.policy.slippage_bps) // BASIS_POINTS
        return QuoteSnapshot(
            token_program=terms.token_program, account_profile_id=terms.account_profile_id, account_size_bytes=terms.account_size_bytes,
            message_template_hash=terms.message_template_hash, instruction_name=terms.instruction_name,
            quote_snapshot_id=self._next_id(run_prefix), side=Side.BUY, quote_state_event_id=state.event_id, fee_snapshot_id=terms.ids["protocol"], created_mono_ns=now_mono_ns,
            knowledge_cutoff_mono_ns=state.receive_mono_ns, curve_input_lamports=curve_input, gross_curve_output_lamports=None, token_amount_base_units=tokens,
            protocol_fee_lamports=proto, creator_fee_lamports=creator, other_protocol_fee_lamports=other, protocol_fee_total_lamports=total,
            swap_wallet_debit_lamports=swap_debit, swap_wallet_credit_lamports=None, network_fee_total_lamports=network, base_fee_lamports=terms.base_fee_lamports,
            priority_fee_lamports=terms.priority_fee_lamports, tip_lamports=self.policy.sender_tip_lamports, rent_delta_lamports=rent, irrecoverable_cost_lamports=self.policy.irrecoverable_cost_lamports,
            slippage_limit_bps=self.policy.slippage_bps, min_output_base_units=None, max_input_base_units=max_input, quote_age_ns=age,
            quote_model_hash=self.policy.quote_model_hash, rounding_version=self.policy.rounding_version, quality_status="OK" if coverage == CoverageState.OBSERVED else coverage.value,
            signature_count=self.policy.signature_count, compute_unit_limit=self.policy.compute_unit_limit, compute_unit_price_micro_lamports=terms.compute_unit_price_micro_lamports,
            cost_evidence_status=terms.evidence, unspent_budget_lamports=budget - curve_input, capped_by_real_reserves=capped,
        )

    def quote_buy_swap_budget(self, state: CurveState, basis: CostBasis, now_mono_ns: int, run_prefix: str,
                              coverage: CoverageState, swap_budget_lamports: int) -> QuoteSnapshot | InvalidQuote:
        """Invert the shared buy quote for a fee-inclusive swap budget (integer lamports).

        Pump/creator fees consume the swap budget. Network fees, rent and the existing
        slippage allowance remain additional. Bounded binary search uses quote_buy's
        current fee tiers/rounding/real-reserve cap, without a second fee model.
        """
        if swap_budget_lamports <= 0:
            return InvalidQuote("SWAP_BUDGET_TOO_SMALL")
        probe = self.quote_buy(state, basis, now_mono_ns, run_prefix, coverage, budget=swap_budget_lamports)
        if isinstance(probe, InvalidQuote) or probe.swap_wallet_debit_lamports <= swap_budget_lamports:
            return probe
        low, high = 1, swap_budget_lamports - 1
        best = None
        while low <= high:
            middle = (low + high) // 2
            quote = self.quote_buy(state, basis, now_mono_ns, run_prefix, coverage, budget=middle)
            if isinstance(quote, InvalidQuote):
                if quote.reason != "CURVE_UNAVAILABLE":
                    return quote
                low = middle + 1
            elif quote.swap_wallet_debit_lamports <= swap_budget_lamports:
                best = quote
                low = middle + 1
            else:
                high = middle - 1
        return best if best is not None else InvalidQuote("SWAP_BUDGET_TOO_SMALL")

    def quote_sell(self, state: CurveState, basis: CostBasis, now_mono_ns: int, run_prefix: str, coverage: CoverageState, tokens: int, rent_refund_lamports: int) -> QuoteSnapshot | InvalidQuote:
        if state.venue_state != VenueState.BONDING_CURVE_ACTIVE:
            return InvalidQuote("VENUE_INVALID", state.venue_state.value)
        if coverage == CoverageState.GAP:
            return InvalidQuote("COVERAGE_GAP")
        if tokens <= 0:
            return InvalidQuote("NO_TOKENS")
        terms = fee_terms(basis, self.policy, now_mono_ns, state, Side.SELL)
        if isinstance(terms, InvalidQuote):
            return terms
        age = now_mono_ns - state.receive_mono_ns
        if age > self.policy.proxy_state_max_age_ns:
            return InvalidQuote("PROXY_STATE_STALE", f"age_ns={age} coverage={coverage.value}")
        gross = self.sol_for_tokens(state.virtual_sol_reserves, state.virtual_token_reserves, tokens)
        if gross <= 0:
            return InvalidQuote("CURVE_UNAVAILABLE", "zero gross output")
        proto = ceil_bps(gross, terms.protocol_bps)
        creator = ceil_bps(gross, terms.creator_bps)
        other = ceil_bps(gross, terms.other_bps)
        total = proto + creator + other
        credit = gross - total
        if credit <= 0:
            return InvalidQuote("CURVE_UNAVAILABLE", "fees exceed gross")
        network = terms.base_fee_lamports + terms.priority_fee_lamports
        min_output = credit * (BASIS_POINTS - self.policy.slippage_bps) // BASIS_POINTS
        refund = rent_refund_lamports if self.policy.close_token_account_on_full_exit else 0
        return QuoteSnapshot(
            token_program=terms.token_program, account_profile_id=terms.account_profile_id, account_size_bytes=terms.account_size_bytes,
            message_template_hash=terms.message_template_hash, instruction_name=terms.instruction_name,
            quote_snapshot_id=self._next_id(run_prefix), side=Side.SELL, quote_state_event_id=state.event_id, fee_snapshot_id=terms.ids["protocol"], created_mono_ns=now_mono_ns,
            knowledge_cutoff_mono_ns=state.receive_mono_ns, curve_input_lamports=None, gross_curve_output_lamports=gross, token_amount_base_units=tokens,
            protocol_fee_lamports=proto, creator_fee_lamports=creator, other_protocol_fee_lamports=other, protocol_fee_total_lamports=total,
            swap_wallet_debit_lamports=None, swap_wallet_credit_lamports=credit, network_fee_total_lamports=network, base_fee_lamports=terms.base_fee_lamports,
            priority_fee_lamports=terms.priority_fee_lamports, tip_lamports=self.policy.sender_tip_lamports, rent_delta_lamports=-refund, irrecoverable_cost_lamports=0,
            slippage_limit_bps=self.policy.slippage_bps, min_output_base_units=min_output, max_input_base_units=None, quote_age_ns=age,
            quote_model_hash=self.policy.quote_model_hash, rounding_version=self.policy.rounding_version, quality_status="OK" if coverage == CoverageState.OBSERVED else coverage.value,
            signature_count=self.policy.signature_count, compute_unit_limit=self.policy.compute_unit_limit, compute_unit_price_micro_lamports=terms.compute_unit_price_micro_lamports,
            cost_evidence_status=terms.evidence,
        )

    def net_liquidation_value(self, state: CurveState, basis: CostBasis, now_mono_ns: int, coverage: CoverageState, tokens: int, booked_failed_costs: int) -> int | None:
        """Modeled gross sell proceeds minus sell protocol total, expected exit network fee/tip, minus booked failed-attempt costs.
        Rent refund is NOT counted as profit. None when unknown (UNKNOWN != 0)."""
        q = self.quote_sell(state, basis, now_mono_ns, "nlv", coverage, tokens, 0)
        if isinstance(q, InvalidQuote):
            return None
        return q.swap_wallet_credit_lamports - q.network_fee_total_lamports - q.tip_lamports - booked_failed_costs

    def exit_reserve_lamports(self, basis: CostBasis, now_mono_ns: int, state: CurveState | None = None) -> int | None:
        terms = fee_terms(basis, self.policy, now_mono_ns, state, Side.SELL)
        if isinstance(terms, InvalidQuote):
            return None
        return self.policy.max_exit_attempts * (terms.base_fee_lamports + terms.priority_fee_lamports + self.policy.sender_tip_lamports)

    def entry_fixed_cost_lamports(self, basis: CostBasis, now_mono_ns: int, state: CurveState) -> int | None:
        """Entry costs outside the fee-inclusive swap debit: network/tip, token-account rent and irrecoverable account cost."""
        terms = fee_terms(basis, self.policy, now_mono_ns, state, Side.BUY)
        if isinstance(terms, InvalidQuote):
            return None
        rent = terms.rent_lamports if self.policy.new_token_account_per_position else 0
        return terms.base_fee_lamports + terms.priority_fee_lamports + self.policy.sender_tip_lamports + rent + self.policy.irrecoverable_cost_lamports

    def quote_buy_total_slot_budget(self, state: CurveState, basis: CostBasis, now_mono_ns: int, run_prefix: str,
                                    coverage: CoverageState, total_budget_lamports: int, exit_reserve_lamports: int) -> QuoteSnapshot | InvalidQuote:
        """Fit one BUY into a strict total slot-capital budget.

        The cap includes the entry max-sol-cost/slippage bound, entry network/tip/rent/irrecoverable
        costs and the reserved exit-attempt costs. This makes N slots require at most N times the
        declared slot capital instead of silently needing an extra fee buffer.
        """
        fixed = self.entry_fixed_cost_lamports(basis, now_mono_ns, state)
        if fixed is None:
            return InvalidQuote("UNQUALIFIED_COSTS")
        available_for_max_input = total_budget_lamports - fixed - exit_reserve_lamports
        if available_for_max_input <= 0:
            return InvalidQuote("SLOT_BUDGET_TOO_SMALL")
        factor = BASIS_POINTS + self.policy.slippage_bps
        # max y with floor(y*factor/BASIS_POINTS) <= available_for_max_input
        swap_budget = ((available_for_max_input + 1) * BASIS_POINTS - 1) // factor
        if swap_budget <= 0:
            return InvalidQuote("SLOT_BUDGET_TOO_SMALL")
        quote = self.quote_buy_swap_budget(state, basis, now_mono_ns, run_prefix, coverage, swap_budget)
        if isinstance(quote, InvalidQuote):
            return quote
        required = (quote.max_input_base_units or quote.swap_wallet_debit_lamports or 0) + fixed + exit_reserve_lamports
        if required > total_budget_lamports:
            return InvalidQuote("SLOT_BUDGET_ROUNDING_OVERFLOW")
        return quote

    def entry_required_cash(self, quote: QuoteSnapshot, exit_reserve: int) -> int:
        return quote.entry_cash_debit + exit_reserve


# --------------------------------------------------------------------------- fill model


@dataclass(frozen=True, slots=True)
class FillEvaluation:
    status: AttemptStatus
    actual: QuoteSnapshot | None
    fees_charged_lamports: int | None  # None = UNKNOWN (never zero)
    failure_reason: str | None
    proxy_age_ns: int | None
    cost_evidence_status: str | None = None


def evaluate_fill(engine: QuoteEngine, expected: QuoteSnapshot, fill_state: CurveState | None, basis: CostBasis, modeled_fill_mono_ns: int, run_prefix: str, coverage: CoverageState, rent_refund: int) -> FillEvaluation:
    """Compare the expected quote with the quote at the OBSERVED_STATE_PROXY of the modeled fill time.
    A landed-but-failed transaction pays its network fee (base+priority) and tip; no protocol fee, no rent."""
    ev = _evaluate_fill(engine, expected, fill_state, basis, modeled_fill_mono_ns, run_prefix, coverage, rent_refund)
    if ev.status == AttemptStatus.UNKNOWN:
        return ev
    terms = fee_terms(basis, engine.policy, modeled_fill_mono_ns, fill_state, expected.side)
    if isinstance(terms, InvalidQuote):
        return FillEvaluation(AttemptStatus.UNKNOWN, None, None, "COSTS_UNKNOWN_AT_MODELED_FILL", ev.proxy_age_ns)
    if ev.status == AttemptStatus.FILLED and expected.side == Side.SELL and engine.stress_rejects(expected.quote_snapshot_id):
        ev = FillEvaluation(AttemptStatus.SLIPPAGE_REJECTED, None, 0, "STRESS_SCENARIO_REJECT", ev.proxy_age_ns)
    if ev.status == AttemptStatus.FILLED:
        if engine.stress_network_fee_multiplier != 1 or engine.stress_adverse_price_bps:
            ev = _apply_stress(engine, ev, expected)
        if ev.status == AttemptStatus.FILLED:
            return ev
    network = int(Decimal(terms.base_fee_lamports + terms.priority_fee_lamports) * engine.stress_network_fee_multiplier)
    evidence = "MODELED_STRESS" if engine.stress_network_fee_multiplier != 1 or engine.stress_adverse_price_bps or engine.stress_exit_reject_rate else terms.evidence
    return FillEvaluation(ev.status, None, network + engine.policy.sender_tip_lamports, ev.failure_reason, ev.proxy_age_ns, evidence)


def _apply_stress(engine: QuoteEngine, ev: FillEvaluation, expected: QuoteSnapshot) -> FillEvaluation:
    """Research stress: network fee multiplier and adverse price bps are declared model assumptions applied once to the modeled fill,
    never a second deduction of fees/impact already inside the quote."""
    q = ev.actual
    assert q is not None
    network = int(Decimal(q.network_fee_total_lamports) * engine.stress_network_fee_multiplier)
    d = _asdict(q)
    d["network_fee_total_lamports"] = network
    d["priority_fee_lamports"] = network - q.base_fee_lamports
    if q.side == Side.BUY and q.swap_wallet_debit_lamports is not None:
        d["swap_wallet_debit_lamports"] = q.swap_wallet_debit_lamports * (BASIS_POINTS + engine.stress_adverse_price_bps) // BASIS_POINTS
        d["curve_input_lamports"] = d["swap_wallet_debit_lamports"] - q.protocol_fee_total_lamports
    elif q.swap_wallet_credit_lamports is not None:
        d["swap_wallet_credit_lamports"] = q.swap_wallet_credit_lamports * (BASIS_POINTS - engine.stress_adverse_price_bps) // BASIS_POINTS
    d["cost_evidence_status"] = "MODELED_STRESS"
    nq = QuoteSnapshot(**d)
    stressed_network_and_tip = network + nq.tip_lamports
    if nq.side == Side.BUY and expected.max_input_base_units is not None and nq.swap_wallet_debit_lamports is not None and nq.swap_wallet_debit_lamports > expected.max_input_base_units:
        return FillEvaluation(AttemptStatus.SLIPPAGE_REJECTED, None, stressed_network_and_tip, "STRESS_ADVERSE_PRICE_EXCEEDS_MAX_INPUT", ev.proxy_age_ns)
    if nq.side == Side.SELL and expected.min_output_base_units is not None and nq.swap_wallet_credit_lamports is not None and nq.swap_wallet_credit_lamports < expected.min_output_base_units:
        return FillEvaluation(AttemptStatus.SLIPPAGE_REJECTED, None, stressed_network_and_tip, "STRESS_ADVERSE_PRICE_BELOW_MIN_OUTPUT", ev.proxy_age_ns)
    fees = nq.entry_cash_debit if nq.side == Side.BUY else nq.network_fee_total_lamports + nq.tip_lamports + nq.protocol_fee_total_lamports
    return FillEvaluation(ev.status, nq, fees, ev.failure_reason, ev.proxy_age_ns)


def _evaluate_fill(engine: QuoteEngine, expected: QuoteSnapshot, fill_state: CurveState | None, basis: CostBasis, modeled_fill_mono_ns: int, run_prefix: str, coverage: CoverageState, rent_refund: int) -> FillEvaluation:
    network_and_tip = expected.network_fee_total_lamports + expected.tip_lamports
    if fill_state is None:
        return FillEvaluation(AttemptStatus.UNKNOWN, None, None, "NO_PROXY_STATE_AT_MODELED_FILL", None)
    if expected.token_program and ((fill_state.token_program or engine.policy.spl_token_program) != expected.token_program or (fill_state.token_program == engine.policy.token_2022_program and fill_state.account_profile_id != expected.account_profile_id)):
        return FillEvaluation(AttemptStatus.UNKNOWN, None, None, "TOKEN_ACCOUNT_PROFILE_CHANGED", None)
    proxy_age = modeled_fill_mono_ns - fill_state.receive_mono_ns
    if fill_state.venue_state != VenueState.BONDING_CURVE_ACTIVE:
        return FillEvaluation(AttemptStatus.VENUE_INVALID, None, network_and_tip, f"VENUE_{fill_state.venue_state.value}", proxy_age)
    if coverage == CoverageState.GAP:
        return FillEvaluation(AttemptStatus.UNKNOWN, None, None, "COVERAGE_GAP_AT_MODELED_FILL", proxy_age)
    if proxy_age > engine.policy.proxy_state_max_age_ns:
        return FillEvaluation(AttemptStatus.UNKNOWN, None, None, "PROXY_STATE_STALE", proxy_age)
    if isinstance(fee_terms(basis, engine.policy, modeled_fill_mono_ns, fill_state, expected.side), InvalidQuote):
        return FillEvaluation(AttemptStatus.UNKNOWN, None, None, "COSTS_UNKNOWN_AT_MODELED_FILL", proxy_age)  # unknown fee rule: never a "safe" failed attempt with old fees
    if expected.side == Side.BUY:
        actual = engine.quote_buy(fill_state, basis, modeled_fill_mono_ns, run_prefix, coverage, budget=expected.curve_input_lamports)
        if isinstance(actual, InvalidQuote):
            return FillEvaluation(AttemptStatus.CURVE_UNAVAILABLE, None, network_and_tip, actual.reason, proxy_age)
        # exact-token semantics: same token amount requested; recompute cost of expected tokens at fill state
        tokens = expected.token_amount_base_units
        vsol, vtok = fill_state.virtual_sol_reserves, fill_state.virtual_token_reserves
        if fill_state.real_token_reserves is not None and tokens > fill_state.real_token_reserves:
            return FillEvaluation(AttemptStatus.CURVE_UNAVAILABLE, None, network_and_tip, "INSUFFICIENT_REAL_TOKEN_RESERVES", proxy_age)
        curve_input = (vsol * tokens) // (vtok - tokens) + 1
        terms = fee_terms(basis, engine.policy, modeled_fill_mono_ns, fill_state, expected.side)
        assert not isinstance(terms, InvalidQuote)
        proto, creator, other = ceil_bps(curve_input, terms.protocol_bps), ceil_bps(curve_input, terms.creator_bps), ceil_bps(curve_input, terms.other_bps)
        debit = curve_input + proto + creator + other
        if expected.max_input_base_units is not None and debit > expected.max_input_base_units:
            # A rejected attempt is charged using the fee rule valid at the
            # modeled fill, never the stale submit quote.
            return FillEvaluation(AttemptStatus.SLIPPAGE_REJECTED, None, actual.network_fee_total_lamports + actual.tip_lamports,
                                  f"max_sol_cost_exceeded debit={debit}", proxy_age)
        filled = QuoteSnapshot(**{**_asdict(actual), "quote_snapshot_id": actual.quote_snapshot_id, "curve_input_lamports": curve_input, "token_amount_base_units": tokens,
                                  "protocol_fee_lamports": proto, "creator_fee_lamports": creator, "other_protocol_fee_lamports": other, "protocol_fee_total_lamports": proto + creator + other,
                                  "swap_wallet_debit_lamports": debit, "max_input_base_units": expected.max_input_base_units, "min_output_base_units": expected.min_output_base_units,
                                  "unspent_budget_lamports": None, "capped_by_real_reserves": False})
        return FillEvaluation(AttemptStatus.FILLED, filled, filled.entry_cash_debit, None, proxy_age)
    actual = engine.quote_sell(fill_state, basis, modeled_fill_mono_ns, run_prefix, coverage, expected.token_amount_base_units, rent_refund)
    if isinstance(actual, InvalidQuote):
        return FillEvaluation(AttemptStatus.CURVE_UNAVAILABLE, None, network_and_tip, actual.reason, proxy_age)
    assert expected.min_output_base_units is not None and actual.swap_wallet_credit_lamports is not None
    if actual.swap_wallet_credit_lamports < expected.min_output_base_units:
        return FillEvaluation(AttemptStatus.SLIPPAGE_REJECTED, None, actual.network_fee_total_lamports + actual.tip_lamports,
                              f"min_sol_output_not_met credit={actual.swap_wallet_credit_lamports} min={expected.min_output_base_units}", proxy_age)
    return FillEvaluation(AttemptStatus.FILLED, actual, actual.network_fee_total_lamports + actual.tip_lamports + actual.protocol_fee_total_lamports, None, proxy_age)


def _asdict(q: QuoteSnapshot) -> dict[str, Any]:
    return {name: getattr(q, name) for name in q.__slots__}  # type: ignore[attr-defined]


# --------------------------------------------------------------------------- retry policy


def next_retry_submit_mono_ns(policy: ExecutionPolicy, attempt_number_just_failed: int, callback_actual_mono_ns: int) -> int | None:
    """Return the submit time of the next exit attempt or None when retries are exhausted (time strictly advances)."""
    if attempt_number_just_failed >= policy.max_exit_attempts:
        return None
    backoff = policy.exit_retry_backoff_ns[attempt_number_just_failed - 1]
    return callback_actual_mono_ns + max(backoff, 1)


# --------------------------------------------------------------------------- ledger rules


def ledger_entry(run_id: str, position_id: str | None, attempt_id: str | None, mono_ns: int, utc_ns: int, kind: str, cash: int, rent: int, tokens: int, cost: int, provenance: str) -> LedgerRecord:
    return LedgerRecord(f"{attempt_id or position_id or run_id}:{kind}", run_id, position_id, attempt_id, mono_ns, utc_iso(utc_ns), kind, cash, rent, tokens, cost, provenance)


def book_entry_fill(run_id: str, position_id: str, attempt_id: str, mono_ns: int, utc_ns: int, q: QuoteSnapshot) -> LedgerRecord:
    return ledger_entry(run_id, position_id, attempt_id, mono_ns, utc_ns, "ENTRY_FILL", -q.entry_cash_debit, q.rent_delta_lamports, q.token_amount_base_units, q.entry_economic_cost, f"OBSERVED_STATE_PROXY;{q.cost_evidence_status}")


def book_failed_attempt(run_id: str, position_id: str, attempt_id: str, mono_ns: int, utc_ns: int, side: Side, fees: int, status: AttemptStatus, evidence: str) -> LedgerRecord:
    return ledger_entry(run_id, position_id, attempt_id, mono_ns, utc_ns, f"{side.value}_FAILED_FEE", -fees, 0, 0, fees, f"{status.value};{evidence}")


def book_exit_fill(run_id: str, position_id: str, attempt_id: str, mono_ns: int, utc_ns: int, q: QuoteSnapshot) -> LedgerRecord:
    assert q.swap_wallet_credit_lamports is not None
    cost = q.protocol_fee_total_lamports + q.network_fee_total_lamports + q.tip_lamports + q.irrecoverable_cost_lamports
    return ledger_entry(run_id, position_id, attempt_id, mono_ns, utc_ns, "EXIT_FILL", q.sell_cash_credit, q.rent_delta_lamports, -q.token_amount_base_units, cost, f"OBSERVED_STATE_PROXY;{q.cost_evidence_status}")
