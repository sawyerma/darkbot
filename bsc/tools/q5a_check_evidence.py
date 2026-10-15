#!/usr/bin/env python3
"""Recomputes a Q5A evidence file independently of the harness.

Usage: q5a_check_evidence.py <evidence.json>

Applies the same rules as the harness's evaluator (verdict.rs), from the
raw samples:

- the handoff duration of every sample is recomputed from its raw
  timestamps, t_boundary_ns - t_write_check_ns, and the summary
  (count, nearest-rank p50/p95/p99, maximum, misses at or above
  1,000,000 ns, their fraction) is built from these recomputed values;
- every handed-off sample must carry generation, peer, write check,
  boundary, duration and wire bytes, the boundary must not lie before the
  write check, the stored duration must equal the recomputed one, the
  kernel must have taken bytes, and the peer must be that of the
  connection the sample belongs to;
- all measured samples must carry a duration;
- the build identity must have exactly the expected values;
- every sample must have every field of the schema.

It compares reasons, provider verdicts, summary, counts and the overall
verdict with what the harness wrote. If Q5A_BLOCKRAZOR_AUTHORIZATION is set,
the file is also searched for it.

Exit status: 0 consistent, 1 inconsistent, 2 the secret was found,
3 the file is unreadable or not this schema.
"""
import json
import math
import os
import re
import sys

SCHEMA = "darkbot.bsc.q5a.tls-transport-evidence.v1"
MISS_NS = 1_000_000
BASELINE = "92fa5204777e566acbaccf111721d9295ffb3a90"
IMPLEMENTATION = "darkbot-runner/tls-v1 rustls-0.23.45 ring http/1.1"
REQUIRED = {"warmup": 10, "measured": 100, "pacing_ms": 200, "reconnect_cycles": 3}
FAILS = {
    "connect_failed", "not_http11", "skipped", "transport_failures", "deadline_misses",
    "connection_changed_during_series", "reconnect_failed", "generation_not_increased",
    "previous_facts_still_current", "reconnect_probe_failed",
}
ANSWERED = {"ack", "reject", "rate_limited", "http_status"}
SAMPLE_FIELDS = [
    "phase", "index", "generation", "peer", "state_before", "request_plaintext_bytes",
    "body_bytes", "body_sha256", "outcome", "prepare_ns", "t_write_check_ns", "t_boundary_ns",
    "write_check_to_boundary_ns", "wire_bytes", "response", "response_latency_ns", "state_after",
]


def nearest_rank(values, percent):
    if not values:
        return None
    rank = min(max(math.ceil(percent * len(values) / 100), 1), len(values))
    return values[rank - 1]


def identity_reasons(identity):
    reasons = []

    def unknown(value):
        return value in ("", "unknown")

    def exact(field, wanted, other):
        value = identity.get(field, "")
        if value != wanted:
            reasons.append("identity_unknown" if unknown(value) else other)

    def flag(field, wanted, unwanted, opposite):
        value = identity.get(field, "")
        if value == wanted:
            return
        if value == unwanted:
            reasons.append(opposite)
        elif unknown(value):
            reasons.append("identity_unknown")
        else:
            reasons.append("identity_invalid")

    exact("baseline", BASELINE, "baseline_mismatch")
    head = identity.get("git_head", "")
    if unknown(head):
        reasons.append("identity_unknown")
    elif not re.fullmatch(r"[0-9a-fA-F]{40}", head):
        reasons.append("identity_invalid")
    flag("git_dirty", "false", "true", "working_tree_dirty")
    flag("baseline_ancestor", "true", "false", "baseline_not_ancestor")
    flag("frozen_sources_unchanged", "true", "false", "frozen_sources_changed")
    for field in ("rustc", "cargo"):
        if unknown(identity.get(field, "")):
            reasons.append("identity_unknown")
    exact("profile", "release", "not_release_build")
    exact("rustls", "0.23.45", "rustls_version")
    exact("ring", "0.17.14", "ring_version")
    exact("transport_implementation", IMPLEMENTATION, "identity_invalid")
    return reasons


def recomputed(sample):
    """The handoff duration from the raw timestamps, if both are there."""
    check, boundary = sample["t_write_check_ns"], sample["t_boundary_ns"]
    if check is None or boundary is None:
        return None
    return boundary - check


def handoff_issue(sample, scope):
    """The first issue with a handed-off sample, as verdict.rs::handoff_issue."""
    if sample["outcome"]["kind"] != "handed_off":
        return None
    for field, issue in (
        ("generation", "missing_generation"),
        ("peer", "missing_peer"),
        ("t_write_check_ns", "missing_write_check"),
        ("t_boundary_ns", "missing_boundary"),
        ("write_check_to_boundary_ns", "missing_duration"),
        ("wire_bytes", "missing_wire_bytes"),
    ):
        if sample[field] is None:
            return issue
    if sample["t_boundary_ns"] < sample["t_write_check_ns"]:
        return "boundary_before_write_check"
    if recomputed(sample) != sample["write_check_to_boundary_ns"]:
        return "duration_mismatch"
    if sample["wire_bytes"] <= 0:
        return "zero_wire_bytes"
    if sample["peer"] != scope["peer"]:
        return "peer_mismatch"
    return None


def answered(sample):
    response = sample.get("response")
    return response is not None and response["class"] in ANSWERED


def evaluate(provider, identity):
    reasons = identity_reasons(identity)
    workload = provider["workload"]
    if workload != REQUIRED:
        reasons.append("workload_not_qualification")
    auth = provider["auth_scope"]
    if auth == "UNQUALIFIED_AUTH_MISSING":
        reasons.append("authorization_missing")
    if auth == "UNQUALIFIED_AUTH_INVALID":
        reasons.append("authorization_invalid")
    connect = provider["connect"]
    connected = connect["ok"]
    if connected:
        if connect["facts"]["http"] != "Http11":
            reasons.append("not_http11")
    else:
        reasons.append("connect_failed")
    probing = connected and auth in ("NOT_REQUIRED", "PRESENT")

    samples = list(provider["samples"])
    probes = [cycle["probe"] for cycle in provider["reconnects"] if cycle["probe"] is not None]
    every = samples + probes
    measured = [s for s in samples if s["phase"] == "measured"]
    kinds = [s["outcome"]["kind"] for s in every]
    skipped, unknown = kinds.count("skipped"), kinds.count("unknown")
    if skipped:
        reasons.append("skipped")
    if unknown:
        reasons.append("transport_failures")
    if probing and len(measured) != workload["measured"]:
        reasons.append("measured_samples_missing")

    # Durations from the raw timestamps, for the samples that carry one.
    durations = sorted(
        recomputed(s) if recomputed(s) is not None else s["write_check_to_boundary_ns"]
        for s in measured
        if s["write_check_to_boundary_ns"] is not None
    )
    if probing and len(durations) != workload["measured"]:
        reasons.append("measured_timing_missing")
    misses = sum(1 for d in durations if d >= MISS_NS)
    if misses:
        reasons.append("deadline_misses")

    scoped = []
    if connected:
        initial = connect["facts"]
        if any(s["generation"] is not None and s["generation"] != initial["generation"]
               for s in samples):
            reasons.append("connection_changed_during_series")
        scoped += [(s, initial) for s in samples]
    for cycle in provider["reconnects"]:
        if cycle["result"]["ok"] and cycle["probe"] is not None:
            scoped.append((cycle["probe"], cycle["result"]["facts"]))
    if any(handoff_issue(sample, scope) for sample, scope in scoped):
        reasons.append("inconsistent_evidence")

    cycles = provider["reconnects"]
    if probing and len(cycles) < workload["reconnect_cycles"]:
        reasons.append("reconnect_cycles_missing")
    for cycle in cycles:
        result = cycle["result"]
        if not result["ok"]:
            reasons.append("reconnect_failed")
            continue
        generation = result["facts"]["generation"]
        previous = cycle["previous_generation"]
        if previous is None or previous + 1 != generation:
            reasons.append("generation_not_increased")
        if cycle["previous_still_current"]:
            reasons.append("previous_facts_still_current")
        probe = cycle["probe"]
        ok = (
            probe is not None and probe["outcome"]["kind"] == "handed_off"
            and answered(probe) and probe["generation"] == generation
        )
        if not ok:
            reasons.append("reconnect_probe_failed")

    if any(reason in FAILS for reason in reasons):
        verdict = "FAIL"
    elif reasons:
        verdict = "INCOMPLETE"
    else:
        verdict = "PASS"
    summary = {
        "count": len(durations),
        "p50_ns": nearest_rank(durations, 50),
        "p95_ns": nearest_rank(durations, 95),
        "p99_ns": nearest_rank(durations, 99),
        "max_ns": durations[-1] if durations else None,
        "limit_ns": MISS_NS,
        "at_or_above_limit": misses,
    }
    fraction = misses / len(durations) if durations else 0.0
    states = [s["state_after"] for s in every]
    counts = {
        "handed_off": kinds.count("handed_off"),
        "skipped": skipped,
        "unknown": unknown,
        "stale_after": states.count("stale"),
        "disconnected_after": states.count("disconnected"),
        "reconnects": len(cycles),
    }
    return verdict, reasons, summary, fraction, counts


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    try:
        with open(sys.argv[1], encoding="utf-8") as handle:
            text = handle.read()
        evidence = json.loads(text)
    except (OSError, ValueError) as error:
        print("unreadable: %s" % error)
        sys.exit(3)
    if evidence.get("schema") != SCHEMA:
        print("not a Q5A evidence file")
        sys.exit(3)
    secret = os.environ.get("Q5A_BLOCKRAZOR_AUTHORIZATION", "")
    if secret and secret in text:
        print("SECRET FOUND in the evidence file")
        sys.exit(2)

    problems = []
    verdicts = []
    for provider in evidence["providers"]:
        name = provider["provider"]
        every = list(provider["samples"]) + [
            cycle["probe"] for cycle in provider["reconnects"] if cycle["probe"] is not None
        ]
        missing = sorted({field for sample in every for field in SAMPLE_FIELDS if field not in sample})
        if missing:
            problems.append("%s: samples lack fields %s" % (name, missing))
            verdicts.append((name, "INCOMPLETE"))
            continue
        verdict, reasons, summary, fraction, counts = evaluate(provider, evidence["identity"])
        verdicts.append((name, verdict))
        written = [reason["reason"] for reason in provider["reasons"]]
        if sorted(set(written)) != sorted(set(reasons)):
            problems.append("%s: reasons %s, recomputed %s" % (name, written, reasons))
        if provider["transport_verdict"] != verdict:
            problems.append("%s: verdict %s, recomputed %s" % (
                name, provider["transport_verdict"], verdict))
        for key, value in summary.items():
            if provider["summary"].get(key) != value:
                problems.append("%s: summary %s %s, recomputed %s" % (
                    name, key, provider["summary"].get(key), value))
        if abs(provider["summary"].get("fraction_at_or_above_limit", -1.0) - fraction) > 5e-7:
            problems.append("%s: fraction %s, recomputed %s" % (
                name, provider["summary"].get("fraction_at_or_above_limit"), fraction))
        for key, value in counts.items():
            if provider["counts"].get(key) != value:
                problems.append("%s: count %s %s, recomputed %s" % (
                    name, key, provider["counts"].get(key), value))
        print("%-10s transport %-10s (recomputed)  reasons: %s" % (
            name, verdict, ", ".join(reasons) or "none"))

    names = {name for name, _ in verdicts}
    if any(verdict == "FAIL" for _, verdict in verdicts):
        overall = "FAIL"
    elif all(verdict == "PASS" for _, verdict in verdicts) and names == {"club48", "blockrazor"}:
        overall = "PASS"
    else:
        overall = "INCOMPLETE"
    written = evidence["verdict"]["tls_transport_qualification"]
    if written != overall:
        problems.append("overall: %s, recomputed %s" % (written, overall))
    print("TLS_TRANSPORT_QUALIFICATION = %s (recomputed)" % overall)
    if problems:
        print("INCONSISTENT")
        for problem in problems:
            print("  " + problem)
        sys.exit(1)
    print("consistent with the harness")


if __name__ == "__main__":
    main()
