#!/usr/bin/env bash
# Compatibility entry point: forwards to capture.sh market. Review for removal in structure step 30.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/capture.sh" market "$@"
