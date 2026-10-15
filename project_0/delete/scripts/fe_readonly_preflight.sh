#!/usr/bin/env bash
# Compatibility entry point: forwards to preflight.sh. Review for removal in structure step 30.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/preflight.sh" "$@"
