"""One-second canonical BUY/actor filter; execution and dedup remain shared."""
from dataclasses import dataclass, field
from darkbot.domain import NS_PER_MS, Completeness, QuoteScopeStatus, VenueState
from darkbot.strategy import entry_admission, target_reached


@dataclass
class Observation:
    mint: str
    signature: str
    start: int
    end: int
    buy_count: int = 0
    buyers: set[str] = field(default_factory=set)


class Legacy5Buy5Unique:
    strategy_id = "LEGACY_5BUY_5UNIQUE_V1"

    def __init__(self, config):
        self.config = config
        self.observations = {}
        self.decided = set()

    def on_create(self, owner, mint, raw, envelope, decode_done):
        if mint in self.observations or mint in self.decided or mint in owner.submitted_mints:
            return
        state = owner.reducer.states[mint]
        # Actionable protocol evidence, independent of portfolio occupancy/capital.
        if (envelope.completeness != Completeness.COMPLETE or state.quote_scope_status != QuoteScopeStatus.SOL_WSOL
                or state.venue_state != VenueState.BONDING_CURVE_ACTIVE or not state.curve_known
                or state.quote_uncertain or state.decode_gap_since is not None or state.unsupported_fee_mode
                or state.token_program not in (owner.spl_token_program, owner.cfg.protocol["token_2022_program"])
                or (state.token_program == owner.cfg.protocol["token_2022_program"] and (not state.account_profile_id or not owner.token_2022_supported))):
            return
        # T0 is the local receipt of the complete actionable CREATE envelope.
        obs = Observation(mint, envelope.signature, raw.receive_mono_ns,
                          raw.receive_mono_ns + self.config["observation_ms"] * NS_PER_MS)
        self.observations[mint] = obs
        owner.record_observation(obs, None, None)

        def complete(actual):
            if self.observations.pop(mint, None) is not obs:
                return
            self.decided.add(mint)
            st = owner.reducer.states.get(mint)
            if st is None:
                owner.record_observation(obs, None, actual, status="DATA_INCOMPLETE")
                return
            coverage_gap = (
                owner.reducer.gap_intersects(obs.start, obs.end - 1)
                or any((end is None or end >= obs.start) and start < obs.end for start, end in st.decode_gap_intervals)
            )
            if coverage_gap:
                # A missing interval can hide BUYs, so this observation must not
                # be recorded as a genuine filter miss (or a genuine qualifier).
                owner.record_observation(obs, None, actual, status="DATA_INCOMPLETE")
                reason = "OBSERVATION_COVERAGE_GAP"
            else:
                qualified = (
                    obs.buy_count >= self.config["min_buy_count"]
                    and len(obs.buyers) >= self.config["min_unique_buyers"]
                )
                owner.record_observation(obs, qualified, actual)
                reason = None if qualified else "OBSERVATION_FILTER_NOT_MET"
            owner._entry_decision(mint, raw, envelope, obs.end, policy_rejection=reason)

        owner.scheduler.schedule_at(obs.end, complete)

    def on_events(self, owner, result, raw, envelope):
        if envelope.completeness != Completeness.COMPLETE:
            return
        for applied in result.events:
            event = applied.event
            obs = self.observations.get(event.mint)
            if (obs is not None and obs.start <= raw.receive_mono_ns < obs.end
                    and envelope.signature != obs.signature and applied.is_new and applied.validation.value == "VALID"
                    and event.event_type.value == "BUY" and event.actor and applied.applied_state):
                obs.buy_count += 1
                obs.buyers.add(event.actor)

    def admission(self, state, **checks):
        checks["creator_sell_known"] = False
        return entry_admission(state, **checks)

    def creator_exit_enabled(self):
        return False

    def target_reached(self, value, entry_cost, target):
        return target_reached(value, entry_cost, target)
