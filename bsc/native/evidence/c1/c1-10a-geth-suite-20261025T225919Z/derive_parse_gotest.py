#!/usr/bin/env python3
"""Derives plain-text per-package and per-test results from raw `go test -json`.

usage: parse_gotest.py <raw.json> <out_prefix>
writes <out_prefix>.packages.txt and <out_prefix>.tests.txt (tab separated).

Package status: PASS, FAIL, SKIP (no test files / all skipped at package level),
BUILD_FAIL, TIMEOUT (test binary hit -timeout), NO_RESULT.
Test status: PASS, FAIL, SKIP, TIMEOUT (was running when its package timed out),
NO_RESULT (started, no terminal event, package not timed out).
"""
import json
import sys
from collections import defaultdict


def main() -> int:
    raw, prefix = sys.argv[1], sys.argv[2]
    pkg_action = {}
    pkg_elapsed = {}
    pkg_output = defaultdict(list)
    pkg_build_failed = set()
    test_status = {}
    test_elapsed = {}
    test_started = set()
    build_output = defaultdict(list)
    bad_lines = 0
    with open(raw, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                ev = json.loads(line)
            except ValueError:
                bad_lines += 1
                continue
            action = ev.get("Action")
            if action in ("build-output", "build-fail"):
                ip = ev.get("ImportPath", "")
                if action == "build-output":
                    build_output[ip].append(ev.get("Output", ""))
                continue
            pkg = ev.get("Package")
            if not pkg:
                continue
            test = ev.get("Test")
            if test is None:
                if action in ("pass", "fail", "skip"):
                    pkg_action[pkg] = action
                    pkg_elapsed[pkg] = ev.get("Elapsed", 0.0)
                    if ev.get("FailedBuild"):
                        pkg_build_failed.add(pkg)
                elif action == "output":
                    pkg_output[pkg].append(ev.get("Output", ""))
                continue
            key = (pkg, test)
            if action == "run":
                test_started.add(key)
            elif action in ("pass", "fail", "skip"):
                test_status[key] = action.upper()
                test_elapsed[key] = ev.get("Elapsed", 0.0)

    pkg_status = {}
    for pkg in sorted(set(pkg_action) | {k[0] for k in test_started}):
        out = "".join(pkg_output.get(pkg, []))
        act = pkg_action.get(pkg)
        if pkg in pkg_build_failed or "[build failed]" in out or "[setup failed]" in out:
            st = "BUILD_FAIL"
        elif "panic: test timed out after" in out:
            st = "TIMEOUT"
        elif act is None:
            st = "NO_RESULT"
        else:
            st = act.upper()
        pkg_status[pkg] = st

    for key in sorted(test_started):
        if key not in test_status:
            test_status[key] = "TIMEOUT" if pkg_status.get(key[0]) == "TIMEOUT" else "NO_RESULT"

    counts = defaultdict(lambda: defaultdict(int))
    for (pkg, test), st in test_status.items():
        level = "top" if "/" not in test else "sub"
        counts[pkg][(level, st)] += 1

    with open(prefix + ".packages.txt", "w") as fh:
        fh.write("# package\tstatus\telapsed_s\ttop_pass\ttop_fail\ttop_skip\ttop_other\tsub_pass\tsub_fail\tsub_skip\tsub_other\n")
        for pkg in sorted(pkg_status):
            c = counts[pkg]
            top_other = sum(v for (lv, s), v in c.items() if lv == "top" and s not in ("PASS", "FAIL", "SKIP"))
            sub_other = sum(v for (lv, s), v in c.items() if lv == "sub" and s not in ("PASS", "FAIL", "SKIP"))
            fh.write("\t".join([
                pkg, pkg_status[pkg], f"{pkg_elapsed.get(pkg, 0.0):.3f}",
                str(c[("top", "PASS")]), str(c[("top", "FAIL")]), str(c[("top", "SKIP")]), str(top_other),
                str(c[("sub", "PASS")]), str(c[("sub", "FAIL")]), str(c[("sub", "SKIP")]), str(sub_other),
            ]) + "\n")
    with open(prefix + ".tests.txt", "w") as fh:
        fh.write("# package\ttest\tstatus\telapsed_s\n")
        for (pkg, test) in sorted(test_status):
            fh.write(f"{pkg}\t{test}\t{test_status[(pkg, test)]}\t{test_elapsed.get((pkg, test), 0.0):.3f}\n")
    if build_output:
        with open(prefix + ".build_output.txt", "w") as fh:
            for ip in sorted(build_output):
                fh.write(f"== {ip}\n{''.join(build_output[ip])}\n")
    print(f"packages={len(pkg_status)} tests={len(test_status)} bad_json_lines={bad_lines}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
