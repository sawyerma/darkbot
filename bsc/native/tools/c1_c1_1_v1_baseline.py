#!/usr/bin/env python3
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

PIN = "cdb7548b5baacfdae92f9f63437d6456411665f3"
MARKER = "DARKBOT_C1_FASTLANE_V1"
PROBE_NAME = "c1_v1_concurrent_publishers_probe_test.go"

PROBE = r'''package fastlane

import (
    "encoding/binary"
    "runtime"
    "sync"
    "sync/atomic"
    "testing"

    "github.com/ethereum/go-ethereum/common"
)

func TestC1V1ConcurrentPublishersProbe(t *testing.T) {
    const producers = 8
    const rounds = 4000

    oldProcs := runtime.GOMAXPROCS(0)
    if oldProcs < producers {
        runtime.GOMAXPROCS(producers)
        defer runtime.GOMAXPROCS(oldProcs)
    }

    path := testRingPath(t)
    ring, err := createRing(path, 0xC101, 16, 1024, directionIngress)
    if err != nil {
        t.Fatal(err)
    }
    defer ring.close()

    for round := 0; round < rounds; round++ {
        readBefore := atomic.LoadUint64(ring.headerU64(offReadSeq))
        writeBefore := atomic.LoadUint64(ring.headerU64(offWriteSeq))
        if readBefore != writeBefore {
            t.Fatalf("probe precondition: ring not empty read=%d write=%d", readBefore, writeBefore)
        }

        start := make(chan struct{})
        var wg sync.WaitGroup
        errs := make(chan error, producers)
        wg.Add(producers)

        for id := 0; id < producers; id++ {
            id := id
            go func() {
                defer wg.Done()
                <-start
                var payload [16]byte
                binary.LittleEndian.PutUint64(payload[0:8], uint64(round))
                binary.LittleEndian.PutUint64(payload[8:16], uint64(id))
                hash := common.BytesToHash(payload[:])
                errs <- ring.publish(Record{
                    Kind:        EarlyTxSeen,
                    TimestampNS: uint64(round + 1),
                    Hash:        hash,
                    Payload:     payload[:],
                })
            }()
        }

        close(start)
        wg.Wait()
        close(errs)
        for err := range errs {
            if err != nil {
                t.Fatalf("unexpected publish error before ring could fill: %v", err)
            }
        }

        readAfter := atomic.LoadUint64(ring.headerU64(offReadSeq))
        writeAfter := atomic.LoadUint64(ring.headerU64(offWriteSeq))
        delta := writeAfter - readAfter
        if delta != producers {
            t.Logf(
                "C1_V1_SPSC_VIOLATION_REPRODUCED round=%d expected_visible=%d observed_visible=%d read=%d write=%d",
                round, producers, delta, readAfter, writeAfter,
            )
            return
        }

        seen := make(map[[32]byte]struct{}, producers)
        for id := 0; id < producers; id++ {
            record, ok, err := ring.tryConsume()
            if err != nil {
                t.Logf("C1_V1_SPSC_VIOLATION_REPRODUCED round=%d consume_error=%v", round, err)
                return
            }
            if !ok {
                t.Logf("C1_V1_SPSC_VIOLATION_REPRODUCED round=%d missing_record=%d", round, id)
                return
            }
            key := [32]byte(record.Hash)
            if _, exists := seen[key]; exists {
                t.Logf("C1_V1_SPSC_VIOLATION_REPRODUCED round=%d duplicate_hash=%x", round, record.Hash[:])
                return
            }
            seen[key] = struct{}{}
        }
    }

    t.Fatalf("C1_V1_PROBE_INCONCLUSIVE: no logical collision observed in %d synchronized rounds; rerun with -race", rounds)
}
'''


def run(root: Path, *args: str, env: dict[str, str] | None = None) -> int:
    print("+ " + " ".join(args), flush=True)
    return subprocess.run(args, cwd=root, env=env).returncode


def output(root: Path, *args: str) -> str:
    return subprocess.check_output(args, cwd=root, text=True).strip()


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} /path/to/bsc-geth", file=sys.stderr)
        return 2

    root = Path(sys.argv[1]).resolve()
    if not (root / ".git").is_dir():
        raise SystemExit(f"not a git checkout: {root}")

    head = output(root, "git", "rev-parse", "HEAD")
    if head != PIN:
        raise SystemExit(f"C1_1_FAIL pinned geth mismatch: {head} != {PIN}")

    marker = root / ".darkbot-c1-fastlane-v1"
    if not marker.is_file() or marker.read_text().strip() != MARKER:
        raise SystemExit("C1_1_FAIL V1 overlay marker missing")

    source = root / "darkbot/fastlane/fastlane.go"
    text = source.read_text()
    required = [
        "write := atomic.LoadUint64(r.headerU64(offWriteSeq))",
        "atomic.StoreUint64(r.headerU64(offWriteSeq), write+1)",
        "manager.ingress.publish",
    ]
    missing = [needle for needle in required if needle not in text]
    if missing:
        raise SystemExit("C1_1_FAIL source is not the expected V1 SPSC baseline: " + repr(missing))

    env = os.environ.copy()
    gates = [
        ("go", "test", "./darkbot/fastlane"),
        ("go", "test", "./eth/protocols/eth", "./eth"),
        ("go", "test", "./core"),
        ("go", "build", "./cmd/geth"),
    ]
    for command in gates:
        if run(root, *command, env=env) != 0:
            raise SystemExit("C1_1_FAIL baseline compile/test gate failed")

    print("C1_1_V1_BASELINE_BUILD_TEST_PASS", flush=True)

    probe = root / "darkbot/fastlane" / PROBE_NAME
    if probe.exists():
        raise SystemExit(f"C1_1_FAIL temporary probe already exists: {probe}")

    try:
        probe.write_text(PROBE)
        print("+ go test -race -run ^TestC1V1ConcurrentPublishersProbe$ -count=1 -v ./darkbot/fastlane", flush=True)
        result = subprocess.run(
            [
                "go",
                "test",
                "-race",
                "-run",
                "^TestC1V1ConcurrentPublishersProbe$",
                "-count=1",
                "-v",
                "./darkbot/fastlane",
            ],
            cwd=root,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        print(result.stdout, end="")
        confirmed = (
            "C1_V1_SPSC_VIOLATION_REPRODUCED" in result.stdout
            or "WARNING: DATA RACE" in result.stdout
            or "DATA RACE" in result.stdout
        )
        if not confirmed:
            raise SystemExit(
                "C1_1_INCONCLUSIVE V1 defect was not reproduced; no C1.1 PASS claimed"
            )
    finally:
        try:
            probe.unlink()
        except FileNotFoundError:
            pass

    if probe.exists():
        raise SystemExit("C1_1_FAIL temporary probe cleanup failed")

    print("C1_1_V1_MULTI_PRODUCER_DEFECT_CONFIRMED", flush=True)
    print("C1_1_PASS", flush=True)
    print(f"base={PIN}", flush=True)
    print("scope=V1 compile/test baseline plus expected multi-producer SPSC defect reproduction", flush=True)
    print("latency_baseline=NOT_ACCEPTED_FOR_CONCURRENT_PEERS", flush=True)
    print("next=C1.2 Wire V2", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
