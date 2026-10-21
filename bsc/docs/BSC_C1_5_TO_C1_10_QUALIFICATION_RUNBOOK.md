# DarkBot BSC — C1.5–C1.10 local qualification runbook

**Candidate:** one implementation tranche for C1.5–C1.10 against `faba91395198d1deb21cd1a318f12d1c2b653aab`, pinned geth `cdb7548b5baacfdae92f9f63437d6456411665f3`.
**Rule:** gates are accepted strictly in order: C1.5 → C1.6 → C1.7 → C1.8 → C1.9 → C1.9b → C1.10. A failure stops every later gate, although its code is present. Nothing here is a result; each command produces one.

All Rust commands run in `bsc/engine`. Use a disposable checkout for geth
qualification so previously accepted binaries/evidence remain reproducible:

```bash
git clone --shared --no-checkout bsc/vendor/bsc-geth /tmp/darkbot-c1-qualification
git -C /tmp/darkbot-c1-qualification checkout --detach cdb7548b5baacfdae92f9f63437d6456411665f3
python3 bsc/tools/c1_apply_geth_overlay.py /tmp/darkbot-c1-qualification
# Run the Go commands below in /tmp/darkbot-c1-qualification.
```

## Reopened accepted interfaces

The tranche changes accepted code in three places. Each needs its earlier gate rerun before C1.5 is judged:

| Changed | Why | Rerun |
|---|---|---|
| `v2.rs`: `try_consume` now runs through the borrowed guard (same validation, plus a slot-stability check at release) | C1.5 | C1.2 and C1.3 Rust tests (`cargo test -p darkbot-bsc-geth-ipc`), C1.3 interop |
| `ingress.go`: the arena lane plan gains the Chain lane and, when configured, the outbound lanes; one config field; shutdown stops outbound first | C1.7, C1.8 | C1.4 Go tests, C1.4 interop, C1.4a harness |
| geth hooks: chain evidence and outbound move from the V1 ring to V2 lanes; V1 init/close leave the handler; overlay marker is now `DARKBOT_C1_FASTLANE_C1_7` | C1.7, C1.8 | overlay apply, `go test ./eth/protocols/eth ./eth ./core`, `go build ./cmd/geth` |

Peer-ingress semantics (`T_packet_decoded`, `T_ingress`, prefilter, one handler = one lane) are unchanged. The Wire V2 layout is unchanged. The bitwise CRC32C is unchanged.

## Common gates (before C1.5)

```bash
cargo fmt --all -- --check
cargo clippy -p darkbot-bsc-geth-ipc --all-targets --locked --offline -- -D warnings
cargo test -p darkbot-bsc-trading --test c14_prefilter_scope --locked --offline
gofmt -l darkbot/fastlane eth/handler_eth.go eth/handler.go eth/protocols/eth/peer.go core/blockchain.go   # must print nothing
go build ./cmd/geth
bsc/tools/c13_lifecycle_interop.sh     # C1_3_GO_RUST_LIFECYCLE_INTEROP_PASS
bsc/tools/c14_ingress_interop.sh       # C1_4_GO_RUST_INGRESS_INTEROP_PASS
```

**Fast-Lane directory trust contract (E7-5, Go `trust.go` = Rust `trust.rs`):** geth and the Rust runtime run as the **same Unix user**; separate users are not supported and fail closed. The Fast-Lane directory is owned by that user and not writable by group/others (`0700` recommended, e.g. `mktemp -d`); every ancestor of its resolved path is owned by root or that user and is not group/other-writable unless sticky. Role leases, arenas and V1 rings are opened via a verified directory descriptor with `O_NOFOLLOW|O_CLOEXEC` and must be regular `0600`-type files of that user with one link; symlinks, hard links, foreign owners or open modes are refused before lock/truncate/mmap. ACLs are not inspected; a compromised process of the same user is outside the boundary.

## C1.5 — Rust HOT0 / zero-copy

```bash
cargo test -p darkbot-bsc-geth-ipc --locked --offline
cargo test -p darkbot-bsc-geth-ipc --release --locked --offline
cargo test -p darkbot-bsc-geth-ipc --locked --offline --test c15_hot0_alloc
```

Exit: lifetime tests (the `compile_fail` doctests of `RecordGuardV2`), `protocol_v2::c15_tests`, `hot0::tests` and the allocation measurement pass in debug and release; the allocation test reports zero allocations on the HOT0 path and at least one per record on the owned control path.

## C1.6 — M4 development profile

```bash
cargo test -p darkbot-bsc-geth-ipc --locked --offline host::
cargo run -p darkbot-bsc-geth-ipc --release --locked --offline --example c16_host_profile -- --profile S --out /tmp/c16-a.json
cargo run -p darkbot-bsc-geth-ipc --release --locked --offline --example c16_host_profile -- --profile S --out /tmp/c16-b.json
cmp /tmp/c16-a.json /tmp/c16-b.json           # reproducible report
cargo run -p darkbot-bsc-geth-ipc --release --locked --offline --example c16_host_profile -- --profile P ; echo "exit=$?"   # 2: refused, not downgraded
go test -race -run 'TestC16' ./darkbot/fastlane
```

Exit: reproducible host/profile evidence; every unavailable value labelled; Profile P refused in C1. No statement about the Linux C3 host.

## C1.7 — local-geth outbound

```bash
go test -race -run 'TestC17' ./darkbot/fastlane
cargo test -p darkbot-bsc-geth-ipc --locked --offline outbound::
```

Exit: multiple peers, slow peer, failing peer, partial handoff, worker lifecycle, no goroutine per BUY, evidence order and provenance pass under the race detector.

## C1.8 — concurrency and state-safety stress

```bash
go test -race -run 'TestC18|TestC15' ./darkbot/fastlane
go test -race ./darkbot/fastlane
cargo test -p darkbot-bsc-geth-ipc --locked --offline --test c18_stress
cargo test -p darkbot-bsc-geth-ipc --release --locked --offline --test c18_stress
```

plus the ownership check printed by the overlay tool (`C1_8_CHAIN_OWNERSHIP_VERIFIED`) and the review of `bsc/docs/BSC_C1_8_CHAIN_LANE_OWNERSHIP.md`. Exit: no ownership or state invariant violated.

## C1.9 — native latency qualification

```bash
python3 -m unittest test_c1_qualification_tools -v      # in bsc/tools: the analyzer logic
bsc/tools/c19_native_latency.sh
```

The window is fixed by `DARKBOT_C19_PEERS`, `DARKBOT_C19_WARMUP_PER_PEER`, `DARKBOT_C19_SAMPLES_PER_PEER` and `DARKBOT_C19_OUTBOUND_WORKERS` before the run (defaults 4, 32, 2500, 2: 10,000 qualification samples, enough for p99.99). The script writes raw CSVs, the snapshots and `c1-9-native-latency-<stamp>.json` to `bsc/native/evidence/c1/`. It reports integrity and measurements and decides no gate. The default outbound mode is `production_devp2p_selected_peer_handoff`: two selected
local devp2p peers, real authenticated RLPx/protocol transports, and the actual
`Peer.SendRawTransaction` method. Both legacy and typed signed transactions are
received and decoded. Setup is outside the measurement window. This transport
fixture does not establish an ETH chain/status session or remote transaction
acceptance; worker timestamps stop at the local transport return.

`DARKBOT_C19_OUTBOUND_MODE=loopback_microbenchmark` retains the TCP-only
microbenchmark. It is never C1.9-eligible. `DARKBOT_C19_PURPOSE=regression_smoke`
also prevents eligibility; use it for short integration checks before acceptance.
The production bridge/template and all raw evidence are hashed in the report.
C1.9b cannot substitute for the outbound measurement.

## C1.9b — hook/path early qualification (Testnet only)

1. Start the overlay geth on BSC Testnet with the Fast Lane enabled.
2. Start the capture on the same host: `c19b_capture` (see its header for the environment).
3. Send controlled launches; the launcher writes the launches file in the shape of `evidence/c1/templates/c1-9b-controlled-launches.template.json`, with `CLOCK_MONOTONIC` handoff times of the same host and boot.
4. Stop the capture (`touch <out>.stop`) and analyze:

```bash
python3 bsc/tools/c19b_hook_path.py --launches <launches.json> --observations <capture.csv> --output <report.json>
```

The tool refuses Mainnet chain ids and incomparable clocks. No value-bearing Mainnet action is part of this gate. `MAINNET_INGRESS_COVERAGE = NOT_PROVEN` stays.

## C1.10 — freeze

```bash
python3 bsc/tools/c1_freeze.py --repo . --inputs <freeze-inputs.json> --output <record.json>            # INCOMPLETE or READY_TO_FREEZE
python3 bsc/tools/c1_freeze.py --repo . --inputs <freeze-inputs.json> --output <record.json> --freeze   # writes IMPLEMENTED_TESTED/FROZEN only if complete
python3 bsc/tools/c1_freeze.py --repo . --inputs <freeze-inputs.json> --verify <record.json>
```

The inputs file starts from `evidence/c1/templates/c1-10-freeze-inputs.template.json`. Each gate entry is a statement of local acceptance with its evidence files; the tool refuses out-of-order acceptance, missing files, templates and missing non-claims. It also refuses a missing or
mismatched DarkBot HEAD, any dirty/untracked working-tree state, absent/mismatched
`final_c1_source_sha256`, or source bytes different from HEAD even when Git is
configured to assume them unchanged. The complete tracked `bsc/engine`, `bsc/geth`, `bsc/tools` and `bsc/native` (evidence) tree
includes all crates, tools, overlay and build inputs. Binary artifacts remain
separate bindings. C1.9 additionally requires an eligible production-mode report.

After committing the final inputs/sources, obtain the explicit expected source
identity (run from the project root); keep freeze input/output files outside the
repository or intentionally ignored so writing the record does not dirty it:

```bash
python3 -c 'import sys,pathlib,json; sys.path.insert(0,"bsc/tools"); import c1_freeze; r=pathlib.Path.cwd(); print(c1_freeze.git_head(r)); print(json.dumps(c1_freeze.source_binding(r),indent=2))'
```

Set `darkbot_commit` and `final_c1_source_sha256` in the inputs to these reviewed
values. `--verify` reevaluates all source and evidence bindings; it also verifies
the recorded binding checksum. An integrated but uncommitted candidate cannot freeze.

## Non-claims that every result keeps

```text
T0_TO_T_BUY_LT_1MS = NOT_PROVEN
MAINNET_INGRESS_COVERAGE = NOT_PROVEN unless separately qualified
PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED unless separately qualified
LIVE = BLOCKED
```
