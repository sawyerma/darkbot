"""Project registry for the shared DarkBot control plane/frontend.

This module owns presentation/control-plane metadata only. Protocol and trading logic stays inside each project package.
"""
from dataclasses import dataclass

@dataclass(frozen=True, slots=True)
class AppDescriptor:
    app_id: str
    display_name: str
    status: str
    package: str

_APPS = (
    AppDescriptor("solana_pump", "Solana / Pump.fun", "ACTIVE", "darkbot.solana_pump"),
    AppDescriptor("bsc", "BSC / Birth Entry", "GATE0", "darkbot.bsc"),
    AppDescriptor("project_0", "Solana / Project 0", "PLANNED", "darkbot.project_0"),
)

def app_catalog() -> tuple[AppDescriptor, ...]:
    return _APPS
