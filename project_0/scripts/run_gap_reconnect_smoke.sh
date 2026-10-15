#!/usr/bin/env bash
# Compatibility entry point: forwards to smoke.sh. Review for removal in structure step 30.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/smoke.sh" "$@"
