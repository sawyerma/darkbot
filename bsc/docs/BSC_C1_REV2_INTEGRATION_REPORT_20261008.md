# C1 REV2 — direct project integration, 2026-10-08

**Status: INTEGRATED / COMPILED / REGRESSION CHECKED / NOT PHASE-QUALIFIED.**
Candidate base: `faba91395198d1deb21cd1a318f12d1c2b653aab`.
Integrated into the existing project at HEAD `12c4616`; unrelated `project_0`
work and earlier raw evidence were preserved. The original candidate patch
applied without conflicts. Vendored geth's previous four patched source files
were independently reconstructed and compared before updating. The old overlay
is backed up in the review workspace; the existing geth executable was preserved.
A fresh current-source geth build was produced at `/tmp/darkbot-c1-rev2-geth-bin`.

## REV2-1 — bind every queued BUY to its selected peer

[Outbound implementation](../native/geth/overlay/darkbot/fastlane/outbound.go):
dispatch takes one immutable target snapshot per worker, using a fixed-size
stack array, and puts that exact target into the job. Selected-count, send and
evidence use that snapshot. Reassignment never retargets an already queued BUY.
Closed/failed sends remain `SEND_ERROR_UNKNOWN`. Queue-full evidence includes
the bound target. Maintenance stays outside dispatch.

[TestC17QueuedBuyKeepsDispatchTargetAfterReassignment](../native/geth/overlay/darkbot/fastlane/outbound_test.go)
queues for A while the worker is held, reassigns to C, then handles the old job
and a later job separately. It checks both successful A and failed A, correct
peer/hash/status evidence, and that only the later job reaches C. Existing
slow/failing-peer, ordering and worker-lifecycle tests remain.

Validation: `go test -race ./darkbot/fastlane` passed in the pinned disposable
geth checkout. The macOS linker emitted an LC_DYSYMTAB warning; the race-test
command exited successfully.

## REV2-2 — measure the actual production transport path

[Harness](../tools/c19_native_latency.sh),
[temporary bridge generator](../tools/c19_prepare_production.py),
[real peer fixture](../tools/c19_production_peer.go.txt),
[analyzer](../tools/c19_analyze.py).

Default mode: `production_devp2p_selected_peer_handoff`. Preconnected localhost
p2p servers negotiate ETH68 and authenticated RLPx; each job uses the actual
`Peer.SendRawTransaction → p2p.Send → protoRW → rlpxTransport` implementation.
The integrated Rust HOT0/egress and persistent C1.7 workers remain in the path.
Worker-owned timestamps are preserved. Receiver decode checks are post-window
transport integrity, never ACK/acquisition timestamps. The measurement fixture
is not a full ETH status/chain session and proves no remote txpool acceptance.

`loopback_microbenchmark` remains selectable and cannot qualify C1.9.
`regression_smoke` cannot qualify C1.9 either. C1.9b remains controlled-launch
handoff → decode → ingress qualification, independently required.

Executed smoke command (not a phase acceptance):

```bash
PATH="$HOME/.local/go1.25.0/bin:$PATH" DARKBOT_C19_PURPOSE=regression_smoke \
DARKBOT_C19_PEERS=2 DARKBOT_C19_WARMUP_PER_PEER=8 \
DARKBOT_C19_SAMPLES_PER_PEER=32 bash bsc/tools/c19_native_latency.sh
```

[Final measured report](../native/evidence/c1/c1-9-native-latency-20261008T221328.json):
integrity PASS; 16 retained warmup records + 64 measured samples, two selected
outbound peers; both legacy and typed transactions decoded. Maximum observed
`T_ingress → L2` = 81,000 ns; egress publication timestamp → first local handoff
= 48,000 ns. These short-run values do not qualify tails, purchase latency or
the full C1 gate. `c1_9_qualification_eligible=false` is explicit in the report.
The earlier 22:11 diagnostic run is also retained; it has no phase acceptance.

The real receiver exposed an additional existing envelope defect:
[overlay tool](../tools/c1_apply_geth_overlay.py) now RLP-wraps EIP-2718
typed transaction bytes as a string inside the ETH Transactions list; legacy
RLP-list envelopes stay intact. Transactions are not decoded/re-signed in the
send path. Empty input fails explicitly.

## REV2-3 — bind the exact complete C1 source and refuse incomplete freeze

[Freeze tool](../tools/c1_freeze.py),
[inputs template](../native/evidence/c1/templates/c1-10-freeze-inputs.template.json),
[negative/verification tests](../tools/test_c1_qualification_tools.py).

Required: expected exact DarkBot commit, clean working tree, deterministic
complete tracked `bsc/native` source hash, source bytes matching HEAD, pinned
geth, overlay/tool, geth/Rust binaries, spec, config, host/topology, accepted
ordered gates, raw evidence, commands and non-claims. Missing HEAD/source,
dirty/untracked source and changed source are refused for readiness, freeze
and verification. Verification also checks the frozen binding record's hash.
A loopback-only C1.9 report cannot satisfy freeze even if labelled PASS manually.

Validation: 20 Python tests passed, including missing HEAD/hash, dirty source,
wrong source hash at the same commit, source changed after freeze,
assume-unchanged bypass, and the corresponding `--verify` refusals. Synthetic
fixtures create no real acceptance records.

## Additional compiler/regression fixes

- [C1.9b capture](../native/crates/fastlane/examples/c19b_capture.rs):
  end the absent-guard borrow before acknowledging a drained lane. This fixes
  E0502 without weakening borrowed-slot lifetimes.
- [C1.8 churn test](../native/crates/fastlane/tests/c18_stress.rs):
  derive a guaranteed different negative-test owner. The original fixed `0xee`
  equals the real owner at cycle 237, incorrectly failing a correct release.
  All 300 churn cycles and the negative owner/epoch checks remain.
- Apply Rust formatting; preserve Wire V2 layout and CRC32C.

## Executed checks and phase disposition

[Recorded regression logs](../native/evidence/c1/rev2-regression-20261008/).

- Rust debug and release: each 122 unit tests passed, 5 explicitly ignored;
  allocation test passed; all 8 stress tests passed; 3 doctests passed.
- `cargo clippy -p darkbot-fastlane --all-targets --locked --offline -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed after formatting.
- Core C1.4 prefilter-scope regression: passed.
- Go fastlane race suite: passed.
- Overlay checks: hook order, outbound hooks and chain ownership passed.
- Complete `go build -o /tmp/darkbot-c1-rev2-geth-bin ./cmd/geth`: passed.
- Python qualification-tool tests: 20 passed.
- Integrated devp2p transport smoke: integrity PASS, explicitly not eligible for gate acceptance.

No C1.5–C1.10 gate is promoted here. Still required, in runbook order:
reopened C1.3/C1.4 interop and C1.4a reruns; complete C1.5 acceptance;
C1.6 reproducibility/QoS acceptance; C1.7 and C1.8 acceptance including the
required hook-package checks; full C1.9 run/acceptance; controlled Testnet C1.9b;
then clean committed C1.10 freeze. No C2 implementation is started.

```text
T0_TO_T_BUY_LT_1MS = NOT_PROVEN
MAINNET_INGRESS_COVERAGE = NOT_PROVEN
PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED
LIVE = BLOCKED
```

The product requirement remains actual successful acquisition
**T0 → T_buy < 1,000,000 ns**, not local send. The masterplan's original-producer
measurement, uncertainty bound and complete 15-second raw capture remain
mandatory. Later arithmetic cannot replace missing original producer timestamps.
