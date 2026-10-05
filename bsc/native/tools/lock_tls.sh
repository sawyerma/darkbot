#!/bin/sh
# Adds the TLS crates of the runner to Cargo.lock, at the versions the
# Gate-0 lock has, and verifies the result. Run once, with network access,
# from anywhere: sh bsc/native/tools/lock_tls.sh
#
# 1. `cargo metadata` resolves the new dependencies and writes them into
#    Cargo.lock; packages already locked keep their versions.
# 2. `cargo update --precise` sets each transitive TLS crate to its Gate-0
#    version (a no-op where it already is).
# 3. verify_tls_lock.py fails unless nothing else changed and every new
#    checksum equals the Gate-0 lock's.
# 4. `cargo fetch --locked` downloads the sources for the offline gates.
#
# Transactional: if any step fails, or the run is interrupted, Cargo.lock
# is restored byte for byte to what it was before. Only a run in which
# every step succeeded leaves the new lock in place.
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
        echo "lock_tls: failed; Cargo.lock restored to its state before this run" >&2
        [ "$status" -ne 0 ] || status=1
    fi
    rm -f "$backup"
    exit "$status"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cargo metadata --format-version 1 >/dev/null
for pin in rustls-pki-types@1.15.1 rustls-webpki@0.103.15 ring@0.17.14 untrusted@0.9.0; do
    cargo update --package "${pin%@*}" --precise "${pin#*@}"
done
python3 tools/verify_tls_lock.py "$backup" Cargo.lock ../gates/gate0/Cargo.lock
cargo fetch --locked

committed=1
echo "Cargo.lock updated and verified. Review its diff, then run the gates with --locked --offline."
