#!/usr/bin/env bash
# Project_0 preflight entry point: the native read-only FE-01 observation (`p0-fe-qualify`).
# `--fe-mode smoke --execute --output DIR` (P1A deployment/target observation) or
# `--fe-mode full …` (FE-01 scope). No signer, submission or value transfer; no Python, no JSON.
# Works from any working directory.
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="${P0_ENV_FILE:-$DIR/../.env}"
if [[ -f "$ENV_FILE" ]]; then
  MODE="$(stat -c '%a' "$ENV_FILE" 2>/dev/null || stat -f '%Lp' "$ENV_FILE")"
  [[ "$MODE" == 600 ]] || { echo 'env permissions must be 600' >&2; exit 2; }
  set -a
  source "$ENV_FILE"
  set +a
fi
BIN="${CARGO_TARGET_DIR:-$DIR/../native/target}/release/p0-fe-qualify"
[[ -x "$BIN" ]] || { echo "build release p0-fe-qualify first: cargo build --release --manifest-path $DIR/../native/Cargo.toml --features 'crate_health txv1' --bin p0-fe-qualify" >&2; exit 2; }
exec "$BIN" "$@"
