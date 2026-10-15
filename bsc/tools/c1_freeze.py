#!/usr/bin/env python3
"""C1.10 freeze tooling.

Binds everything that defines the C1 candidate into one freeze record:

  pinned geth base commit, the C1 overlay sources and tool (the patch),
  the geth binary, the Wire V2 specification, the Rust binaries, the C1
  configuration, the host/profile evidence, the accepted test inventory, the
  accepted raw measurement evidence, the exact build commands and the exact
  non-claims.

The tool refuses to freeze, and reports `INCOMPLETE` with every reason, when
an input is missing, when a gate is not locally accepted, when a later gate
is accepted before an earlier one, when a template is offered as evidence or
when a required non-claim is absent. `READY_TO_FREEZE` is only a statement
that the inputs are complete. `IMPLEMENTED_TESTED/FROZEN` is written only
with --freeze, by the person who performs the freeze, and can be checked
again with --verify.

Existing code and passing code review are not acceptance: every gate entry
is a human statement of local acceptance with its evidence files.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

SCHEMA_INPUTS = "darkbot.c1.freeze_inputs.v1"
SCHEMA_RECORD = "darkbot.c1.freeze_record.v1"
PIN = "cdb7548b5baacfdae92f9f63437d6456411665f3"
# Qualified strictly in this order; a later gate needs every earlier one.
GATES = ("C1.5", "C1.6", "C1.7", "C1.8", "C1.9", "C1.9b")
# Evidence of the gates accepted before this tranche.
PRIOR_EVIDENCE = (
    "bsc/native/evidence/c1/c1-1-v1-baseline-20261006.json",
    "bsc/native/evidence/c1/c1-2-wire-v2-20261006.json",
    "bsc/native/evidence/c1/c1-3-lane-lifecycle-20261007.json",
    "bsc/native/evidence/c1/c1-4-geth-ingress-20261007.json",
    "bsc/native/evidence/c1/c1-4a-early-latency-20261008.json",
)
ARTIFACTS = ("geth_binary", "wire_v2_spec", "overlay_sources", "overlay_tool", "host_profile_evidence",
             "build_commands")
ARTIFACT_LISTS = ("rust_binaries", "c1_config", "test_inventory")
REQUIRED_NON_CLAIMS = (
    "T0_TO_T_BUY_LT_1MS = NOT_PROVEN",
    "MAINNET_INGRESS_COVERAGE = NOT_PROVEN",
    "PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED",
    "LIVE = BLOCKED",
)
TEMPLATE_MARK = b"TEMPLATE_NOT_EVIDENCE"
FROZEN = "IMPLEMENTED_TESTED/FROZEN"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def sha256_path(path: Path) -> str:
    """Hash of a file, or of every file below a directory with its path."""
    if path.is_file():
        return sha256_file(path)
    digest = hashlib.sha256()
    for item in sorted(p for p in path.rglob("*") if p.is_file()):
        digest.update(str(item.relative_to(path)).encode())
        digest.update(b"\0")
        digest.update(sha256_file(item).encode())
    return digest.hexdigest()


def git_head(path: Path) -> str | None:
    try:
        out = subprocess.run(["git", "-C", str(path), "rev-parse", "HEAD"],
                             check=True, capture_output=True, text=True)
        return out.stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def git_output(repo: Path, *args: str) -> bytes:
    return subprocess.run(["git", "-C", str(repo), *args], check=True,
                          capture_output=True).stdout


def source_binding(repo: Path) -> dict:
    """Bind every tracked native file (all crates, tools, overlay and build inputs).

    Also compare each byte to HEAD, independently of Git's stat cache,
    assume-unchanged or skip-worktree bits. Ignored build outputs are bound
    separately as binary artifacts. No caller-selected source subset.
    """
    root = git_output(repo, "rev-parse", "--show-toplevel").decode().strip()
    if Path(root).resolve() != repo.resolve():
        raise ValueError("DarkBot root is not the Git repository root")
    status = git_output(repo, "status", "--porcelain=v1", "--untracked-files=all").decode()
    entries = git_output(repo, "ls-tree", "-r", "-z", "HEAD", "--", "bsc/engine", "bsc/geth", "bsc/tools", "bsc/native").split(b"\0")
    actual, committed = hashlib.sha256(), hashlib.sha256()
    count = 0
    for entry in entries:
        if not entry:
            continue
        meta, name = entry.split(b"\t", 1)
        mode, kind, oid = meta.split()
        if kind != b"blob" or mode not in (b"100644", b"100755"):
            raise ValueError("unsupported C1 source entry: " + name.decode())
        path = repo / name.decode()
        if path.is_symlink():
            raise ValueError("C1 source symlink: " + name.decode())
        data = path.read_bytes()
        original = git_output(repo, "cat-file", "blob", oid.decode())
        for digest, content in ((actual, data), (committed, original)):
            digest.update(mode + b" " + name + b"\0" + hashlib.sha256(content).digest())
        count += 1
    if count == 0:
        raise ValueError("final C1 source tree is empty or absent")
    return {"algorithm": "sha256(mode SP repo_path NUL sha256(file_bytes)), sorted git tree order",
            "scope": "all tracked bsc/engine, bsc/geth, bsc/tools and bsc/native (evidence) files", "files": count,
            "sha256": actual.hexdigest(), "head_sha256": committed.hexdigest(),
            "working_tree": "clean" if not status else "dirty", "git_status": status}


def is_template(path: Path) -> bool:
    if path.is_dir():
        return False
    with path.open("rb") as handle:
        return TEMPLATE_MARK in handle.read(1 << 20)


class Evaluation:
    def __init__(self, repo: Path) -> None:
        self.repo = repo
        self.missing: list[str] = []
        self.bindings: dict = {}

    def bind(self, label: str, relative: object) -> str | None:
        """Hashes one input; records why it cannot be bound."""
        if not isinstance(relative, str) or not relative:
            self.missing.append(f"{label}: not given")
            return None
        path = self.repo / relative
        if not path.exists():
            self.missing.append(f"{label}: {relative} does not exist")
            return None
        if is_template(path):
            self.missing.append(f"{label}: {relative} is a template, not evidence")
            return None
        return sha256_path(path)

    def bind_list(self, label: str, values: object) -> dict:
        if not isinstance(values, list) or not values:
            self.missing.append(f"{label}: no file given")
            return {}
        out = {}
        for relative in values:
            digest = self.bind(label, relative)
            if digest is not None:
                out[relative] = digest
        return out


def evaluate(inputs: dict, repo: Path, geth_head=None) -> dict:
    """The freeze evaluation: bindings plus every reason it is incomplete."""
    geth_head = geth_head or git_head
    ev = Evaluation(repo)
    if inputs.get("schema") != SCHEMA_INPUTS:
        ev.missing.append(f"inputs schema is not {SCHEMA_INPUTS}")
    if inputs.get("pinned_geth_commit") != PIN:
        ev.missing.append(f"pinned_geth_commit is not {PIN}")
    checkout = inputs.get("geth_checkout")
    observed = geth_head(repo / checkout) if isinstance(checkout, str) and checkout else None
    if observed != PIN:
        ev.missing.append(f"geth checkout HEAD is {observed!r}, not the pin")
    ev.bindings["pinned_geth_commit"] = PIN
    head = git_head(repo)
    ev.bindings["darkbot_commit"] = head
    if not head or inputs.get("darkbot_commit") != head:
        ev.missing.append("darkbot_commit: missing HEAD or exact expected commit mismatch")
    try:
        source = source_binding(repo)
        ev.bindings["final_c1_source"] = source
        if source["working_tree"] != "clean":
            ev.missing.append("DarkBot working tree is dirty")
        if source["sha256"] != source["head_sha256"]:
            ev.missing.append("final C1 source differs from committed source")
        if inputs.get("final_c1_source_sha256") != source["sha256"]:
            ev.missing.append("final C1 source hash missing or mismatch")
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        ev.missing.append("final C1 source/working tree cannot be bound: " + str(exc))

    artifacts = inputs.get("artifacts") or {}
    ev.bindings["artifacts"] = {name: ev.bind(name, artifacts.get(name)) for name in ARTIFACTS}
    for name in ARTIFACT_LISTS:
        ev.bindings["artifacts"][name] = ev.bind_list(name, artifacts.get(name))
    ev.bindings["prior_gate_evidence"] = {path: ev.bind("prior gate evidence", path) for path in PRIOR_EVIDENCE}
    ev.bindings["raw_measurement_evidence"] = ev.bind_list("raw_measurement_evidence",
                                                           inputs.get("raw_measurement_evidence"))

    gates = {entry.get("gate"): entry for entry in inputs.get("gates") or [] if isinstance(entry, dict)}
    ev.bindings["gates"] = {}
    blocked_by = None
    for gate in GATES:
        entry = gates.get(gate)
        if entry is None:
            ev.missing.append(f"{gate}: no acceptance entry")
            blocked_by = blocked_by or gate
            continue
        accepted = (entry.get("verdict") == "PASS" and bool(entry.get("accepted_by"))
                    and bool(entry.get("accepted_at")))
        if not accepted:
            ev.missing.append(f"{gate}: not locally accepted (verdict {entry.get('verdict')!r})")
        elif blocked_by is not None:
            ev.missing.append(f"{gate}: accepted before {blocked_by}; gates are accepted in order")
        evidence = ev.bind_list(f"{gate} evidence", entry.get("evidence"))
        if gate == "C1.9":
            production_reports = []
            for path in evidence:
                try:
                    report = json.loads((repo / path).read_text())
                    production_reports.append(
                        isinstance(report, dict)
                        and report.get("schema") == "darkbot.c1.native_latency.v1"
                        and report.get("outbound_mode") == "production_devp2p_selected_peer_handoff"
                        and report.get("integrity") == "PASS"
                        and report.get("c1_9_qualification_eligible") is True)
                except (OSError, ValueError, UnicodeError):
                    pass
            if not any(production_reports):
                ev.missing.append("C1.9: missing eligible production devp2p handoff report")
        ev.bindings["gates"][gate] = {
            "verdict": entry.get("verdict"),
            "accepted_by": entry.get("accepted_by"),
            "accepted_at": entry.get("accepted_at"),
            "evidence": evidence,
        }
        if not accepted:
            blocked_by = blocked_by or gate

    non_claims = inputs.get("non_claims") or []
    for claim in REQUIRED_NON_CLAIMS:
        if not any(isinstance(line, str) and line.startswith(claim) for line in non_claims):
            ev.missing.append(f"non-claim missing: {claim}")
    ev.bindings["non_claims"] = non_claims

    canonical = json.dumps(ev.bindings, sort_keys=True, separators=(",", ":")).encode()
    return {
        "schema": SCHEMA_RECORD,
        "status": "INCOMPLETE" if ev.missing else "READY_TO_FREEZE",
        "missing": ev.missing,
        "bindings": ev.bindings,
        "bindings_sha256": hashlib.sha256(canonical).hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, help="DarkBot repository root")
    parser.add_argument("--inputs", required=True, help="freeze inputs JSON")
    parser.add_argument("--output", help="where to write the freeze record")
    parser.add_argument("--freeze", action="store_true", help="write the FROZEN record if complete")
    parser.add_argument("--verify", help="a FROZEN record to check against the current files")
    args = parser.parse_args()

    repo = Path(args.repo).resolve()
    inputs = json.loads(Path(args.inputs).read_text())
    record = evaluate(inputs, repo)

    if args.verify:
        frozen = json.loads(Path(args.verify).read_text())
        if frozen.get("status") != FROZEN:
            print("C1_10_VERIFY_FAIL the record is not a frozen record", file=sys.stderr)
            return 3
        frozen_digest = hashlib.sha256(json.dumps(frozen.get("bindings"), sort_keys=True,
                                               separators=(",", ":")).encode()).hexdigest()
        if (record["missing"] or frozen_digest != frozen.get("bindings_sha256")
                or record["bindings_sha256"] != frozen.get("bindings_sha256")):
            for reason in record["missing"]:
                print(f"C1_10_VERIFY_FAIL {reason}", file=sys.stderr)
            print("C1_10_VERIFY_FAIL the files no longer match the frozen record", file=sys.stderr)
            return 3
        print(f"C1_10_FROZEN_RECORD_VERIFIED {frozen['bindings_sha256']}")
        return 0

    if record["missing"]:
        for reason in record["missing"]:
            print(f"C1_10_INCOMPLETE {reason}", file=sys.stderr)
        if args.freeze:
            print("C1_10_FREEZE_REFUSED", file=sys.stderr)
    elif args.freeze:
        record["status"] = FROZEN
    if args.output:
        Path(args.output).write_text(json.dumps(record, indent=2) + "\n")
    print(f"status={record['status']} bindings_sha256={record['bindings_sha256']}")
    return 2 if record["missing"] else 0


if __name__ == "__main__":
    sys.exit(main())
