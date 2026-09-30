"""Existing first actionable CREATE policy, extracted without changing its behavior."""
from darkbot.strategy import entry_admission, target_reached


class FastCreatorHold:
    strategy_id = "FAST_CREATOR_HOLD_V1"

    def __init__(self, config):
        self.config = config

    def on_create(self, owner, mint, raw, envelope, decode_done):
        owner._entry_decision(mint, raw, envelope, decode_done)

    def on_events(self, owner, result, raw, envelope):
        pass

    def admission(self, state, **checks):
        return entry_admission(state, **checks)

    def creator_exit_enabled(self):
        return self.config["creator_sell_exit_enabled"]

    def target_reached(self, value, entry_cost, target):
        if not self.config.get("target_exit_enabled", True):
            return False
        return target_reached(value, entry_cost, target)

    def stop_reached(self, value, entry_cost):
        return False

    def cancel_entry_if_creator_sold_before_fill(self):
        return False
