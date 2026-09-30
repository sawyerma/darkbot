"""Strategy policies sharing the same market and execution engine."""
from darkbot.strategies.base import StrategyPolicy
from darkbot.strategies.strategy_a_fast_creator import FastCreatorHold


def make_strategy(config) -> StrategyPolicy:
    strategy_id = config.get("strategy_id", FastCreatorHold.strategy_id)
    if strategy_id == FastCreatorHold.strategy_id:
        return FastCreatorHold(config)
    if strategy_id == "LEGACY_BACKEND_FILTER_V1":
        from darkbot.strategies.strategy_c_legacy_backend import LegacyBackendFilter
        return LegacyBackendFilter(config)
    from darkbot.strategies.strategy_b_legacy_5buy import Legacy5Buy5Unique
    return Legacy5Buy5Unique(config)
