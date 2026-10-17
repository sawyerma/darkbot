#!/bin/sh
set -eu
script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
python3 - "$script_dir/../schemas" <<'PY'
import hashlib
from pathlib import Path
import sys
root = Path(sys.argv[1]).resolve()
files = sorted(root.rglob("*.proto"))
if not files:
    raise SystemExit("no .proto files found")
h = hashlib.sha256()
for p in files:
    rel = p.relative_to(root).as_posix().encode("utf-8")
    data = p.read_bytes()
    h.update(len(rel).to_bytes(4, "big"))
    h.update(rel)
    h.update(len(data).to_bytes(8, "big"))
    h.update(data)
print(h.hexdigest())
PY
