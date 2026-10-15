#!/usr/bin/env python3
"""Verifies that Cargo.lock gained exactly one workspace member and nothing else.

Usage: verify_lock_member.py <lock before> <lock after> <member name>

Fails unless every package of the lock before is in the lock after with the
same version, source and checksum, and the only new package is the named
member: a path package, without source and checksum.
"""
import re
import sys

BLOCK = re.compile(r"\[\[package\]\]\n(.*?)(?=\n\[\[package\]\]|\Z)", re.S)
FIELD = re.compile(r'^(name|version|source|checksum) = "([^"]*)"$', re.M)
DEPENDENCY = re.compile(r'^ "([^"]+)",$', re.M)


def packages(path):
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    found = {}
    for block in BLOCK.findall(text):
        fields = dict(FIELD.findall(block))
        key = (fields["name"], fields["version"])
        found[key] = (fields.get("source"), fields.get("checksum"), DEPENDENCY.findall(block))
    return found


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    before, after = packages(sys.argv[1]), packages(sys.argv[2])
    member = sys.argv[3]
    problems = []
    for key, value in sorted(before.items()):
        if key not in after:
            problems.append("removed or changed version: %s %s" % key)
        elif after[key][:2] != value[:2]:
            problems.append("changed source or checksum: %s %s" % key)
    new = sorted(set(after) - set(before))
    if [name for name, _ in new] != [member]:
        problems.append("new packages %s, expected only %s" % (new, member))
    for key in new:
        source, checksum, dependencies = after[key]
        if source is not None or checksum is not None:
            problems.append("%s is not a path package" % key[0])
        print("new: %s %s, dependencies %s" % (key[0], key[1], dependencies))
    if problems:
        print("FAILED")
        for problem in problems:
            print("  " + problem)
        sys.exit(1)
    print("OK: only %s was added; nothing else changed" % member)


if __name__ == "__main__":
    main()
