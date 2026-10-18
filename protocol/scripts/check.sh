#!/usr/bin/env bash
# Reproducible contract gate, including linked Git worktrees and untracked outputs.
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:---check}"
if [[ "$mode" != --check && "$mode" != --generate ]]; then
  echo 'usage: protocol/scripts/check.sh [--check|--generate]' >&2
  exit 2
fi
buf lint
p0_tmp=$(mktemp -d)
trap 'rm -rf "$p0_tmp"' EXIT
p0_base=${PROTOCOL_BASE_REF:-}
if [[ -z "$p0_base" ]]; then
  for candidate in main origin/main; do
    if git -C .. rev-parse --verify --quiet "$candidate^{commit}" >/dev/null; then
      p0_base=$candidate
      break
    fi
  done
fi
if [[ -z "$p0_base" ]]; then
  echo 'No compatibility baseline. Set PROTOCOL_BASE_REF to a fetched Git ref.' >&2
  exit 1
fi
mkdir -p "$p0_tmp/base"
git -C .. archive "$p0_base" protocol | tar -x -C "$p0_tmp/base"
buf breaking --against "$p0_tmp/base/protocol"
if [[ "$mode" == --generate ]]; then
  buf generate
  buf generate --template buf.gen.docs.yaml
  bash scripts/schema-id.sh
else
  mkdir -p "$p0_tmp/protocol"
  cp -R schemas buf.yaml buf.gen.yaml buf.gen.docs.yaml scripts "$p0_tmp/protocol/"
  (
    cd "$p0_tmp/protocol"
    buf generate
    buf generate --template buf.gen.docs.yaml
    bash scripts/schema-id.sh
  )
  diff -ru "$p0_tmp/frontend/src/core/generated" ../frontend/src/core/generated
  diff -ru "$p0_tmp/docs/protocol" ../docs/protocol
  cmp "$p0_tmp/protocol/schema_id.txt" schema_id.txt
fi
echo "protocol contract: PASS (compatibility baseline: $p0_base)"
