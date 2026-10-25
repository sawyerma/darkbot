#!/usr/bin/env python3
"""Prints package, test and skip message for every skipped test (plain text)."""
import json, sys, collections
out = collections.defaultdict(list); st = {}
for line in open(sys.argv[1], encoding="utf-8", errors="replace"):
    e = json.loads(line)
    t = e.get("Test")
    if t is None or not e.get("Package"):
        continue
    k = (e["Package"], t)
    if e["Action"] == "output":
        out[k].append(e["Output"])
    elif e["Action"] in ("pass", "fail", "skip"):
        st[k] = e["Action"]
for k in sorted(st):
    if st[k] != "skip":
        continue
    msg = [l.strip() for l in out[k] if not l.startswith("=== ") and not l.lstrip().startswith("--- SKIP")]
    print(f"{k[0]}\t{k[1]}\t{' | '.join(m for m in msg if m)[:300]}")
