#!/usr/bin/env bash
# C0-E1 duration entry point. Validation and all boundaries live in MeasurementPlan.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ "$#" == 1 && ( "$1" == --help || "$1" == -h ) ]]; then
  cat <<'HELP'
Usage: ./project_0/scripts/test.sh --duration 10m|1h|5h|24h
Duration: positive integer with s/m/h suffix, 1..86400 seconds.
Four exact quarter snapshots; fractional seconds are retained in nanoseconds.
Capture starts after Shadow bootstrap and market StreamReady.
Bootstrap, drain and verification have independent budgets; total time is longer.
Existing local endpoint/signer configuration and all safety gates remain required.
Legacy run_econ4h.sh and consistent P0_ECON_* settings remain supported.
P1 retains legacy strict verification; index/crosscheck CLI wiring follows in P3.
LIVE/submission remain disabled. No Mainnet run is started by --help.
HELP
  exit 0
fi
if [[ "$#" != 2 || "$1" != --duration ]]; then
  echo 'Usage: test.sh --duration 10m|1h|5h|24h (also integer seconds, e.g. 61s)' >&2
  exit 2
fi
# Pass the CLI value separately so sourcing .env cannot silently replace it.
exec "$SCRIPT_DIR/run_econ4h.sh" --duration "$2"
