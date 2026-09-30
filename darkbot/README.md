# DarkBot Runtime Ownership

The runtime is organized by application. Protocol/business logic must stay inside the owning project package.

## Structure

~~~text
darkbot/
├── main.py              shared stable application facade
├── cli.py               shared stable CLI facade
├── app/                 shared control-plane/application registry
├── common/              truly project-independent infrastructure only
├── solana/              shared native Solana infrastructure boundary
├── bsc/                 BSC Birth Entry application
├── solana_pump/         Solana/Pump.fun application implementation
├── project_0/           Project 0 liquidation application
├── runtime/             shared runtime assembly/profile selection
├── chains/              shared chain contracts/registry
└── venues/              shared venue contracts/registry
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

## Compatibility facades

Several historical top-level modules remain as thin import facades so existing callers/tests do not break during the structural migration. New implementation work must target the project-owned packages, not those facades.

The facades contain no independent business logic and can be retired after all external imports have migrated.
