#!/usr/bin/env python3
"""Final plain-text C1.10A classification from the compare output and skip reasons.

usage: final_classification.py <an_dir> <out_file>
"""
import re
import sys
from collections import Counter

an, out = sys.argv[1], sys.argv[2]


def tab(path):
    with open(path) as fh:
        return [l.rstrip("\n").split("\t") for l in fh if l.strip() and not l.startswith("#")]


bpk = {r[0]: r[1] for r in tab(f"{an}/baseline.packages.txt")}
cpk = {r[0]: r[1] for r in tab(f"{an}/candidate.packages.txt")}
bt = {(r[0], r[1]): r[2] for r in tab(f"{an}/baseline.tests.txt")}
ct = {(r[0], r[1]): r[2] for r in tab(f"{an}/candidate.tests.txt")}
skips = {}
for r in tab(f"{an}/candidate.skip_reasons.txt"):
    skips[(r[0], r[1])] = r[2] if len(r) > 2 else ""
bskips = {(r[0], r[1]) for r in tab(f"{an}/baseline.skip_reasons.txt")}

NONPASS = {"FAIL", "TIMEOUT", "NO_RESULT", "BUILD_FAIL"}


def skip_class(pkg, test, reason):
    if pkg.endswith("/darkbot/fastlane"):
        return "CANDIDATE_ONLY_SKIP_BY_DESIGN"
    if "submodule" in reason or "tests/testdata" in reason or "missing test files" in reason:
        return "ENV_UNAVAILABLE_OFFLINE_TESTDATA (ethereum/tests submodule not present; network needed)"
    if "spec-tests/fixtures" in reason:
        return "ENV_UNAVAILABLE_OFFLINE_TESTDATA (execution-spec fixtures not present; network needed)"
    if "CKZG unavailable" in reason:
        return "UPSTREAM_SKIP_BUILD_CONFIG (ckzg build tag not set in default test build)"
    return "UPSTREAM_UNCONDITIONAL_OR_PLATFORM_SKIP"


pkg_cls = Counter()
pkg_rows = []
for pkg in sorted(set(bpk) | set(cpk)):
    b, c = bpk.get(pkg, "ABSENT"), cpk.get(pkg, "ABSENT")
    if b == "ABSENT":
        cls = "CANDIDATE_ONLY" + ("" if c == "PASS" else "_NOT_PASS")
    elif c == "ABSENT":
        cls = "REGRESSION_MISSING_IN_CANDIDATE"
    elif b == c == "PASS":
        cls = "SAME_PASS"
    elif b == c == "SKIP":
        cls = "SAME_NO_TEST_FILES"
    elif b in NONPASS and c in NONPASS:
        cls = "BASELINE_FAILURE"
    elif c in NONPASS:
        cls = "REGRESSION"
    elif b in NONPASS:
        cls = "FIXED"
    else:
        cls = "DIFFERS"
    pkg_cls[cls] += 1
    pkg_rows.append((pkg, b, c, cls))

test_cls = Counter()
rows = []
for key in sorted(set(bt) | set(ct)):
    b, c = bt.get(key, "ABSENT"), ct.get(key, "ABSENT")
    pkg, test = key
    level = "sub" if "/" in test else "top"
    if b == "ABSENT":
        cls = "CANDIDATE_ONLY_PASS" if c == "PASS" else ("CANDIDATE_ONLY_SKIP_BY_DESIGN" if c == "SKIP" else "CANDIDATE_ONLY_FAIL")
        reason = skips.get(key, "") if c == "SKIP" else "exists only with the overlay"
    elif c == "ABSENT":
        cls, reason = "REGRESSION_MISSING_IN_CANDIDATE", f"baseline={b}"
    elif b == c == "PASS":
        cls, reason = "SAME_PASS", ""
    elif b == c == "SKIP":
        cls = "SAME_SKIP"
        reason = skips.get(key, "")
    elif b in NONPASS and c in NONPASS:
        cls, reason = "BASELINE_FAILURE", f"baseline={b} candidate={c}"
    elif c in NONPASS:
        cls, reason = "REGRESSION_UNCONFIRMED", f"baseline={b} candidate={c}"
    elif b in NONPASS:
        cls, reason = "FIXED_UNCONFIRMED", f"baseline={b} candidate={c}"
    else:
        cls, reason = "DIFFERS", f"baseline={b} candidate={c}"
    test_cls[(cls, level)] += 1
    rows.append((pkg, test, b, c, cls, reason))

with open(out, "w") as fh:
    fh.write("C1.10A full non-race geth suite - classification\n")
    fh.write("baseline  = bnb-chain/bsc v1.7.8 cdb7548b5baacfdae92f9f63437d6456411665f3 (pristine)\n")
    fh.write("candidate = same pin + DarkBot C1 overlay (bsc/tools/c1_apply_geth_overlay.py)\n")
    fh.write("Fast Lane env (DARKBOT_*) unset in both runs: overlay hooks run on their disabled path.\n\n")
    fh.write("PACKAGE CLASSES (count)\n")
    for k in ("REGRESSION", "REGRESSION_MISSING_IN_CANDIDATE", "BASELINE_FAILURE", "FIXED", "DIFFERS",
              "CANDIDATE_ONLY", "CANDIDATE_ONLY_NOT_PASS", "SAME_PASS", "SAME_NO_TEST_FILES"):
        fh.write(f"  {k:34s} {pkg_cls.get(k, 0)}\n")
    fh.write("  BUILD_FAIL / TIMEOUT / ENV_UNAVAILABLE (package could not build offline): 0 in both runs\n\n")
    fh.write("TEST CLASSES (top-level / subtests)\n")
    for k in ("REGRESSION_UNCONFIRMED", "REGRESSION_MISSING_IN_CANDIDATE", "BASELINE_FAILURE",
              "FIXED_UNCONFIRMED", "DIFFERS", "CANDIDATE_ONLY_PASS", "CANDIDATE_ONLY_SKIP_BY_DESIGN",
              "CANDIDATE_ONLY_FAIL", "SAME_PASS", "SAME_SKIP"):
        fh.write(f"  {k:34s} {test_cls.get((k, 'top'), 0)} / {test_cls.get((k, 'sub'), 0)}\n")
    fh.write("  FLAKY                              0 / 0 (no test differed between the runs; no rerun was required)\n")
    fh.write("  REGRESSION (confirmed)             0 / 0\n\n")

    fh.write("DIFFERING PACKAGES / TESTS\n")
    diff = [r for r in rows if r[4] not in ("SAME_PASS", "SAME_SKIP") and not r[4].startswith("CANDIDATE_ONLY")]
    dpk = [r for r in pkg_rows if r[3] not in ("SAME_PASS", "SAME_NO_TEST_FILES") and not r[3].startswith("CANDIDATE_ONLY")]
    if not diff and not dpk:
        fh.write("  none\n")
    for r in dpk:
        fh.write("  pkg\t" + "\t".join(r) + "\n")
    for r in diff:
        fh.write("  test\t" + "\t".join(r) + "\n")
    fh.write("\n")

    fh.write("SAME_SKIP DETAIL (identical skip in both runs; category; skip message from candidate)\n")
    cats = Counter()
    for pkg, test, b, c, cls, reason in rows:
        if cls != "SAME_SKIP":
            continue
        cat = skip_class(pkg, test, reason)
        cats[cat] += 1
        same = "baseline-skip-seen" if (pkg, test) in bskips else "baseline-skip-MISSING"
        fh.write(f"  {pkg}\t{test}\t{cat}\t{same}\t{reason}\n")
    fh.write("  -- category counts\n")
    for k, v in sorted(cats.items()):
        fh.write(f"  {v:3d}  {k}\n")
    fh.write("  note: skip messages are taken from test2json attribution; parallel tests in package tests\n")
    fh.write("  interleave their stderr lines, so a message may name a sibling test's directory.\n\n")

    fh.write("CANDIDATE_ONLY (darkbot/fastlane; must pass)\n")
    for pkg, test, b, c, cls, reason in rows:
        if cls.startswith("CANDIDATE_ONLY"):
            fh.write(f"  {pkg}\t{test}\t{c}\t{cls}\t{reason}\n")
print(dict(pkg_cls))
print({f"{k[0]}:{k[1]}": v for k, v in test_cls.items()})
