#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
from pathlib import Path

PIN_COMMIT = "cdb7548b5baacfdae92f9f63437d6456411665f3"
PIN_TREE = "d8d88097521d43cef66047fdd93dd6825a2a7843"
EXPECTED_TRACKED = [
    "core/blockchain.go",
    "eth/handler.go",
    "eth/handler_eth.go",
    "eth/protocols/eth/peer.go",
]
EXPECTED_UNTRACKED = [
    ".darkbot-c1-fastlane-v1",
    "darkbot/fastlane/fastlane.go",
    "darkbot/fastlane/fastlane_test.go",
]
MARKER = "DARKBOT_C1_FASTLANE_V1"


def run(cwd: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=check,
    )


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def diff_new_file(root: Path, rel: str) -> bytes:
    result = subprocess.run(
        ["git", "diff", "--no-index", "--binary", "--", "/dev/null", rel],
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode not in (0, 1):
        raise SystemExit(
            f"cannot render untracked patch for {rel}: "
            + result.stderr.decode("utf-8", errors="replace")
        )
    return result.stdout


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} /path/to/bsc-geth", file=sys.stderr)
        return 2

    source = Path(sys.argv[1]).resolve()
    if not (source / ".git").is_dir():
        raise SystemExit(f"not a git checkout: {source}")

    tool = Path(__file__).resolve()
    darkbot_root = tool.parents[3]
    evidence_dir = darkbot_root / "bsc/native/evidence/c1"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    patch_path = evidence_dir / "c1-geth-fastlane-v1.patch"
    json_path = evidence_dir / "c1-geth-fastlane-v1.json"

    head = run(source, "git", "rev-parse", "HEAD").stdout.strip()
    tree = run(source, "git", "rev-parse", "HEAD^{tree}").stdout.strip()
    if head != PIN_COMMIT or tree != PIN_TREE:
        raise SystemExit(
            f"C1_EVIDENCE_FAIL pinned source mismatch head={head} tree={tree}"
        )

    status_lines = [
        line
        for line in run(source, "git", "status", "--porcelain").stdout.splitlines()
        if line.strip()
    ]
    tracked = []
    untracked = []
    unexpected = []
    for line in status_lines:
        code = line[:2]
        rel = line[3:]
        if code == "??":
            untracked.append(rel.rstrip("/"))
        else:
            tracked.append(rel)

    if sorted(tracked) != sorted(EXPECTED_TRACKED):
        raise SystemExit(
            "C1_EVIDENCE_FAIL tracked set mismatch: "
            + json.dumps(sorted(tracked))
        )

    normalized_untracked = set()
    for rel in untracked:
        if rel == "darkbot":
            normalized_untracked.update(
                [
                    "darkbot/fastlane/fastlane.go",
                    "darkbot/fastlane/fastlane_test.go",
                ]
            )
        else:
            normalized_untracked.add(rel)

    for rel in sorted(normalized_untracked):
        if rel not in EXPECTED_UNTRACKED and rel != "geth":
            unexpected.append(rel)
    if unexpected:
        raise SystemExit(
            "C1_EVIDENCE_FAIL unexpected untracked paths: "
            + json.dumps(unexpected)
        )

    marker = source / ".darkbot-c1-fastlane-v1"
    if not marker.is_file() or marker.read_text().strip() != MARKER:
        raise SystemExit("C1_EVIDENCE_FAIL overlay marker missing or wrong")

    for rel in EXPECTED_UNTRACKED[1:]:
        if not (source / rel).is_file():
            raise SystemExit(f"C1_EVIDENCE_FAIL missing overlay source {rel}")

    tracked_patch = run(
        source,
        "git",
        "diff",
        "--binary",
        "HEAD",
        "--",
        *EXPECTED_TRACKED,
    ).stdout.encode()
    patch = bytearray(tracked_patch)
    for rel in EXPECTED_UNTRACKED:
        patch.extend(diff_new_file(source, rel))
    patch_path.write_bytes(bytes(patch))

    binary = source / "geth"
    binary_sha = sha256(binary) if binary.is_file() else None
    binary_size = binary.stat().st_size if binary.is_file() else None

    go_version_result = run(source, "go", "version", check=False)
    go_version = (
        go_version_result.stdout.strip()
        if go_version_result.returncode == 0
        else "UNAVAILABLE"
    )

    source_hashes = {
        rel: sha256(source / rel)
        for rel in EXPECTED_TRACKED + EXPECTED_UNTRACKED
    }

    manifest = {
        "schema": "darkbot.c1.geth_fastlane_evidence.v1",
        "base": {
            "repository": "https://github.com/bnb-chain/bsc.git",
            "release": "v1.7.8",
            "commit": PIN_COMMIT,
            "tree": PIN_TREE,
        },
        "overlay_marker": MARKER,
        "tracked_modified": EXPECTED_TRACKED,
        "untracked_overlay": EXPECTED_UNTRACKED,
        "source_sha256": source_hashes,
        "patch": {
            "path": "bsc/native/evidence/c1/c1-geth-fastlane-v1.patch",
            "sha256": hashlib.sha256(patch).hexdigest(),
            "bytes": len(patch),
        },
        "go": {
            "version": go_version,
            "go_mod_sha256": sha256(source / "go.mod"),
            "go_sum_sha256": sha256(source / "go.sum"),
        },
        "built_geth": {
            "present": binary.is_file(),
            "sha256": binary_sha,
            "bytes": binary_size,
        },
        "acceptance_claim": {
            "compile_and_tests": "USER_REPORTED_C1_GETH_HOOKS_ALL_GATES_PASS",
            "latency": "NOT_YET_MEASURED",
            "arrival_advantage": "NOT_YET_PROVEN",
            "live_authorization": False,
            "t0_to_t_buy_lt_1ms": "NOT_YET_PROVEN",
        },
    }
    json_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    print("C1_GETH_PATCH_EVIDENCE_READY")
    print(f"patch={patch_path}")
    print(f"patch_sha256={manifest['patch']['sha256']}")
    print(f"manifest={json_path}")
    print(f"geth_binary_sha256={binary_sha or 'NONE'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
