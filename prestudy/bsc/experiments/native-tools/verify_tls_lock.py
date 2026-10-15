#!/usr/bin/env python3
"""Verifies Cargo.lock after the TLS crates of the runner were added.

Usage: verify_tls_lock.py <lock before> <lock after> <gate0 lock>

Fails unless
  1. every package of the lock before is in the lock after, with the same
     version, source and checksum: nothing was upgraded, downgraded or
     removed;
  2. the pinned TLS crates are present at exactly these versions;
  3. every new package that the Gate-0 lock also has carries the same
     checksum there, so that the bytes are the ones resolved before;
  4. no aws-lc crate entered the lock (the runner uses the ring provider).

New packages the Gate-0 lock does not have are listed for review and fail
the check, so that nothing enters unnoticed.
"""
import re
import sys

PINNED = {
    "rustls": "0.23.45",
    "rustls-pki-types": "1.15.1",
    "rustls-webpki": "0.103.15",
    "webpki-roots": "1.0.9",
    "ring": "0.17.14",
    "untrusted": "0.9.0",
}
FORBIDDEN = ("aws-lc-rs", "aws-lc-sys", "aws-lc-fips-sys")

BLOCK = re.compile(r"\[\[package\]\]\n(.*?)(?=\n\[\[package\]\]|\Z)", re.S)
FIELD = re.compile(r'^(name|version|source|checksum) = "([^"]*)"$', re.M)


def packages(path):
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    found = {}
    for block in BLOCK.findall(text):
        fields = dict(FIELD.findall(block))
        key = (fields["name"], fields["version"])
        found[key] = (fields.get("source"), fields.get("checksum"))
    return found


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    before, after, gate0 = (packages(path) for path in sys.argv[1:])
    problems = []

    for key, value in sorted(before.items()):
        if key not in after:
            problems.append("removed or changed version: %s %s" % key)
        elif after[key] != value:
            problems.append("changed source or checksum: %s %s" % key)

    for name, version in sorted(PINNED.items()):
        if (name, version) not in after:
            present = sorted(v for (n, v) in after if n == name)
            problems.append("%s must be %s, lock has %s" % (name, version, present or "none"))

    new = sorted(set(after) - set(before))
    unknown = []
    for key in new:
        if key[0] in FORBIDDEN:
            problems.append("forbidden crate: %s %s" % key)
        if key in gate0:
            if gate0[key][1] != after[key][1]:
                problems.append("checksum differs from the Gate-0 lock: %s %s" % key)
        else:
            unknown.append(key)
    for key in unknown:
        problems.append("new crate not in the Gate-0 lock, review it: %s %s" % key)

    print("new packages: %d" % len(new))
    for name, version in new:
        mark = "gate0 checksum ok" if (name, version) in gate0 else "NOT IN GATE-0 LOCK"
        print("  %-24s %-10s %s" % (name, version, mark))
    if problems:
        print("FAILED")
        for problem in problems:
            print("  " + problem)
        sys.exit(1)
    print("OK: no existing package changed; TLS crates pinned; checksums match the Gate-0 lock")


if __name__ == "__main__":
    main()
