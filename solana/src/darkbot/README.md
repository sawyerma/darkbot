# DarkBot Runtime Ownership

The runtime is organized by application. Protocol/business logic must stay inside the owning project package.

## Structure

~~~text
darkbot/
├── __main__.py          package entry point / dispatcher
├── app/                 shared control-plane/application registry
├── common/              genuinely project-independent infrastructure only
├── runtime/             shared runtime assembly/profile selection
├── chains/              shared chain contracts/registry
├── venues/              shared venue contracts/registry
├── solana/              shared native Solana primitives
├── bsc/                 BSC Birth Entry application
├── solana_pump/         Solana/Pump.fun application implementation
└── project_0/           Project 0 liquidation application
~~~

## Ownership rules

- Pump.fun market, strategy, economics, PAPER/replay and protocol code belongs in `solana_pump/`.
- Project 0 health, state, economics, routing and receivership code belongs in `project_0/`.
- BSC launch, venue, state, economics and execution code belongs in `bsc/`.
- Shared Solana transport/state/execution primitives used by both Solana applications belong in `solana/`.
- Only genuinely project-independent components belong in `common/`.
- `main.py` and `cli.py` are orchestration/facade surfaces, not protocol-logic locations.
- Shared registries may select applications/components but may not implement strategy/protocol behavior.
- Do not duplicate a Solana stack separately in Pump.fun and Project 0.

## Canonical imports

Project implementation code is imported from its owning package. Historical top-level Pump.fun compatibility modules have been retired; new and existing callers must use `darkbot.solana_pump.*`. Shared chain/venue/runtime contracts remain under their shared packages.
