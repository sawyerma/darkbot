#!/usr/bin/env python3
"""Remove generator formatting-only drift; never alter tokens or wire descriptors."""
from pathlib import Path
for directory, pattern in [(Path("../frontend/src/core/generated"), "*.ts"), (Path("../docs/protocol"), "*.md")]:
    for path in directory.rglob(pattern):
        path.write_text("\n".join(line.rstrip() for line in path.read_text().splitlines()).rstrip() + "\n")
