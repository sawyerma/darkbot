#!/usr/bin/env bash
# Operator command bound by docs/PHASENPLAN.md ("test.sh --duration 10m|1h|5h|24h"): the typed C0-E1 measurement.
# Identical to `capture.sh econ4h`; kept as the binding name, not as a legacy wrapper.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/capture.sh" econ4h "$@"
