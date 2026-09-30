"""Shared DarkBot CLI facade.

Current executable commands are provided by the Solana/Pump.fun application. Additional project applications register separately instead of adding protocol logic here.
"""
from darkbot.solana_pump.cli import *  # noqa: F401,F403
from darkbot.solana_pump.cli import main
