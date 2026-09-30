"""Shared DarkBot application facade.

The concrete applications live below project packages. The existing Solana/Pump.fun API is re-exported here so callers keep one stable top-level entry point while BSC and Project 0 remain separate applications.
"""
from darkbot.app.registry import app_catalog
from darkbot.solana_pump.main import *  # noqa: F401,F403

__all__ = [name for name in globals() if not name.startswith("_")]
