# DarkBot Tests

Tests are separated by ownership:

- [common/](common/) — genuinely cross-project registry/runtime-seam tests.
- [bsc/](bsc/) — BSC-specific tests and retained Gate-0 evidence.
- [solana_pump/](solana_pump/) — Pump.fun/PAPER/replay/strategy/economics tests, fixtures and test helpers.
- [project_0/](project_0/) — Project 0 tests as implementation starts.

Project-specific fixtures and helpers stay inside the owning project test directory. The test root contains navigation/package files only.
