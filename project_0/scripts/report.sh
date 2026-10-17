#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# JFI-03: a run with native records gets the legacy views the pinned readers need by an explicit
# offline export from its journals (`p0ctl export`); the run itself never writes them.
legacy_views() {
  local run="" prev=""
  for a in "$@"; do
    if [[ "$prev" == --run ]]; then run="$a"; fi
    case "$a" in --run=*) run="${a#--run=}" ;; esac
    prev="$a"
  done
  [[ -n "$run" ]] || return 0
  local ctl="${CARGO_TARGET_DIR:-$DIR/../native/target}/release/p0ctl"
  if [[ ! -x "$ctl" ]]; then
    echo "LEGACY_EXPORT_UNAVAILABLE: build p0ctl (cargo build --release --bin p0ctl) for runs with native records" >&2
    return 0
  fi
  "$ctl" --project "$DIR/.." export --run "$run" >&2
}
legacy_views "$@" || exit 2
# --run ID|PATH is located across all run roots and its layout checked before the pinned readers run.
exec python3 "$DIR/../operations/capture/run_store.py" exec "$DIR/../operations/reporting/market_report.py" "$@"
