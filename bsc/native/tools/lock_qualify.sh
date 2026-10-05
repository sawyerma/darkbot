#!/bin/sh
# Adds the workspace member darkbot-qualify to Cargo.lock and verifies that
# nothing else changed. It brings no new external crate, so this runs
# offline: sh bsc/native/tools/lock_qualify.sh
#
# Transactional: if a step fails or the run is interrupted, Cargo.lock is
# restored byte for byte.
set -eu
native=$(cd "$(dirname "$0")/.." && pwd)
cd "$native"
backup=$(mktemp)
cp Cargo.lock "$backup"
committed=0

finish() {
    status=$?
    if [ "$committed" -ne 1 ]; then
        cp "$backup" Cargo.lock
        echo "lock_qualify: failed; Cargo.lock restored to its state before this run" >&2
        [ "$status" -ne 0 ] || status=1
    fi
    rm -f "$backup"
    exit "$status"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cargo metadata --offline --format-version 1 >/dev/null
python3 tools/verify_lock_member.py "$backup" Cargo.lock darkbot-qualify

committed=1
echo "Cargo.lock updated and verified. Review its diff, then run the gates with --locked --offline."
