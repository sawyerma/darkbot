# DarkBot Tests

Tests are separated by project:

- [common/](common/) — shared registry/runtime-seam tests.
- [bsc/](bsc/) — BSC-specific tests and retained Gate-0 evidence.
- [solana_pump/](solana_pump/) — Pump.fun/PAPER/replay/strategy/economics tests.
- [project_0/](project_0/) — Project 0 tests as implementation starts.

Shared helpers and fixtures remain at the test root only when they are genuinely consumed by more than one project.
