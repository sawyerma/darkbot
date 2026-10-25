#!/usr/bin/env python3
"""Compares baseline vs candidate plain-text results (from parse_gotest.py).

usage: compare.py <baseline_prefix> <candidate_prefix> <out_prefix> [reruns.txt]

reruns.txt (optional, tab separated): side  package  test  attempt  status
Writes <out_prefix>.package_table.txt, <out_prefix>.classification.txt and
<out_prefix>.rerun_targets.txt. All plain text.
"""
import sys
from collections import defaultdict

NONPASS = {"FAIL", "TIMEOUT", "NO_RESULT", "BUILD_FAIL"}


def read_tab(path):
    rows = []
    with open(path) as fh:
        for line in fh:
            if line.startswith("#") or not line.strip():
                continue
            rows.append(line.rstrip("\n").split("\t"))
    return rows


def main() -> int:
    bpre, cpre, out = sys.argv[1], sys.argv[2], sys.argv[3]
    reruns = defaultdict(list)
    if len(sys.argv) > 4:
        for side, pkg, test, _attempt, status in read_tab(sys.argv[4]):
            reruns[(side, pkg, test)].append(status)

    bp = {r[0]: r for r in read_tab(bpre + ".packages.txt")}
    cp = {r[0]: r for r in read_tab(cpre + ".packages.txt")}
    bt = {(r[0], r[1]): r[2] for r in read_tab(bpre + ".tests.txt")}
    ct = {(r[0], r[1]): r[2] for r in read_tab(cpre + ".tests.txt")}

    # Package table.
    pkg_class_counts = defaultdict(int)
    with open(out + ".package_table.txt", "w") as fh:
        fh.write("# package\tbaseline\tcandidate\tb_top(pass/fail/skip/other)\tc_top(pass/fail/skip/other)"
                 "\tb_sub(pass/fail/skip/other)\tc_sub(pass/fail/skip/other)\tpackage_class\n")
        for pkg in sorted(set(bp) | set(cp)):
            b = bp.get(pkg)
            c = cp.get(pkg)
            bs = b[1] if b else "ABSENT"
            cs = c[1] if c else "ABSENT"
            if b is None:
                cls = "CANDIDATE_ONLY"
            elif c is None:
                cls = "MISSING_IN_CANDIDATE"
            elif bs == cs:
                cls = "SAME_" + bs if bs not in NONPASS else "BASELINE_FAILURE"
            elif bs in NONPASS and cs in NONPASS:
                cls = "BASELINE_FAILURE"
            elif bs in NONPASS:
                cls = "DIFFERS_BASELINE_WORSE"
            elif cs in NONPASS:
                cls = "DIFFERS_CANDIDATE_WORSE"
            else:
                cls = "DIFFERS_OTHER"
            pkg_class_counts[cls] += 1

            def fmt(r, off):
                return "-" if r is None else "/".join(r[off:off + 4])
            fh.write("\t".join([pkg, bs, cs, fmt(b, 3), fmt(c, 3), fmt(b, 7), fmt(c, 7), cls]) + "\n")
        fh.write("\n# package class counts\n")
        for k in sorted(pkg_class_counts):
            fh.write(f"# {k}\t{pkg_class_counts[k]}\n")

    # Test classification.
    classes = defaultdict(list)
    rerun_targets = set()
    for key in sorted(set(bt) | set(ct)):
        pkg, test = key
        b = bt.get(key, "ABSENT")
        c = ct.get(key, "ABSENT")
        top = test.split("/")[0]
        if b == "ABSENT":
            cls = "CANDIDATE_ONLY_PASS" if c in ("PASS", "SKIP") else "CANDIDATE_ONLY_FAIL"
            reason = f"exists only with overlay; candidate={c}"
        elif c == "ABSENT":
            cls = "MISSING_IN_CANDIDATE"
            reason = f"baseline={b}; not reported in candidate"
            rerun_targets.add((pkg, top))
        elif b == c:
            if b == "PASS":
                cls = "SAME_PASS"
            elif b == "SKIP":
                cls = "SAME_SKIP"
            else:
                cls = "BASELINE_FAILURE"
            reason = f"baseline={b} candidate={c}"
        elif b in NONPASS and c in NONPASS:
            cls = "BASELINE_FAILURE"
            reason = f"baseline={b} candidate={c}"
        else:
            rerun_targets.add((pkg, top))
            rb = reruns.get(("baseline", pkg, top), [])
            rc = reruns.get(("candidate", pkg, top), [])
            reason = f"suite baseline={b} candidate={c}; rerun(top={top}) baseline={','.join(rb) or '-'} candidate={','.join(rc) or '-'}"
            if c in NONPASS and b not in NONPASS:
                base = "REGRESSION_CANDIDATE"
            elif b in NONPASS and c not in NONPASS:
                base = "FIXED_CANDIDATE"
            else:
                base = "SKIP_DIFFERENCE"
            if rb and rc:
                b_any_fail = any(s in NONPASS for s in rb)
                c_any_fail = any(s in NONPASS for s in rc)
                if base == "REGRESSION_CANDIDATE":
                    cls = "REGRESSION" if (all(s in NONPASS for s in rc) and not b_any_fail) else "FLAKY"
                elif base == "FIXED_CANDIDATE":
                    cls = "FIXED" if (all(s in NONPASS for s in rb) and not c_any_fail) else "FLAKY"
                else:
                    cls = "SKIP_DIFFERENCE"
            else:
                cls = base
        classes[cls].append((pkg, test, reason))

    with open(out + ".classification.txt", "w") as fh:
        fh.write("# test classification (top-level tests and subtests), baseline=pristine pin, candidate=pin+DarkBot overlay\n")
        fh.write("# counts: class\ttop_level\tsubtests\n")
        for cls in sorted(classes):
            top = sum(1 for _, t, _ in classes[cls] if "/" not in t)
            fh.write(f"# {cls}\t{top}\t{len(classes[cls]) - top}\n")
        for cls in sorted(classes):
            if cls in ("SAME_PASS", "SAME_SKIP"):
                continue
            fh.write(f"\n== {cls}\n")
            for pkg, test, reason in classes[cls]:
                fh.write(f"{pkg}\t{test}\t{reason}\n")
    with open(out + ".rerun_targets.txt", "w") as fh:
        for pkg, top in sorted(rerun_targets):
            fh.write(f"{pkg}\t{top}\n")
    for k in sorted(pkg_class_counts):
        print("pkg", k, pkg_class_counts[k])
    for cls in sorted(classes):
        print("test", cls, len(classes[cls]))
    print("rerun_targets", len(rerun_targets))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
