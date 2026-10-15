# DarkBot Fast Lane Wire V2

Status: C1.2 byte layout, the C1.3 lane-lifecycle protocol and the C1.4 geth ingress record semantics. This document defines the byte layout and the lifecycle shared by pinned bsc-geth and `darkbot-fastlane`. C1.3 changes no offset, size or field. C1.4 binds peer ingress to these lanes; C1.5 replaces the Rust owned-copy receive path with a borrowed guard.

## Byte order and alignment

- all integer fields are little-endian;
- arena alignment: 128 bytes;
- SuperHeader: 256 bytes;
- LaneDirectory entry: 128 bytes;
- per-lane header: 384 bytes = three separately owned 128-byte regions;
- slot header: 256 bytes;
- slot stride: multiple of 128 bytes, 512..262144 bytes;
- capacity: 1..4096 slots per lane;
- generation: non-zero;
- magic: `DBFLN002`;
- version: 2;
- endian marker: `0x0102`;
- CRC kind: 1 = CRC32C Castagnoli.

The arena contains one SuperHeader, exactly `lane_count` directory entries, padding to the next 128-byte boundary, then each lane region. Every lane offset, lane header and slot start is 128-byte aligned.

## SuperHeader

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | magic |
| 8 | 2 | version |
| 10 | 2 | endian marker |
| 12 | 4 | header size = 256 |
| 16 | 8 | generation |
| 24 | 4 | lane count |
| 28 | 4 | directory entry size = 128 |
| 32 | 4 | lane header size = 384 |
| 36 | 4 | slot header size = 256 |
| 40 | 4 | alignment = 128 |
| 44 | 4 | CRC kind = 1 |
| 48 | 8 | total arena bytes |
| 56..255 | 200 | reserved zero |

## LaneDirectory entry

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 2 | lane id |
| 2 | 2 | direction |
| 4 | 2 | lane class |
| 6 | 2 | flags |
| 8 | 4 | capacity |
| 12 | 4 | slot stride |
| 16 | 8 | lane offset from arena start |
| 24 | 8 | lane byte length |
| 32..127 | 96 | reserved zero |

Directions:

- 1 = geth → Rust
- 2 = Rust → geth

Initial C1.2 lane classes:

- 1 = peer ingress
- 2 = txpool evidence
- 3 = chain evidence
- 4 = outbound handoff evidence
- 5 = signed transaction egress

Lane ids are unique inside one arena.

## Lane header

The 384-byte header is deliberately split by writer ownership.

### Producer-owned region, bytes 0..127

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | write_sequence |
| 8 | 8 | dropped_full |
| 16 | 8 | oversize |
| 24 | 8 | producer_status |
| 32..127 | 96 | reserved |

### Consumer-owned region, bytes 128..255

| Offset | Size | Field |
| ---: | ---: | --- |
| 128 | 8 | read_sequence |
| 136 | 8 | corrupt_records |
| 144 | 8 | drained_epoch |
| 152..255 | 104 | reserved |

### Lifecycle region, bytes 256..383

| Offset | Size | Field |
| ---: | ---: | --- |
| 256 | 8 | generation |
| 264 | 8 | lane_epoch |
| 272 | 4 | lifecycle state |
| 276 | 4 | reserved |
| 280 | 8 | final_write_sequence |
| 288 | 8 | shutdown |
| 296..327 | 32 | owner tag |
| 328 | 8 | quarantine_reason (reserved zero in C1.2; named by C1.3) |
| 336..383 | 48 | reserved |

## Lane lifecycle (C1.3)

States: `FREE=0`, `BOUND=1`, `CLOSING=2`, `QUARANTINED=3`. Peer-ingress lanes are created `FREE` with `lane_epoch = 1`; dedicated non-peer lanes (txpool, chain, outbound evidence, egress) are created `BOUND` and stay `BOUND` for the generation unless quarantined.

```text
FREE --bind--> BOUND --close--> CLOSING --drain ack--> (drained) --release--> FREE
any state --quarantine--> QUARANTINED   (terminal for the generation)
```

### Atomic access

Every lifecycle field is read and written atomically only: `lane_epoch`, `final_write_sequence`, `shutdown`, `quarantine_reason` and `drained_epoch` as 64-bit words, the state as a 32-bit word, and the 32-byte owner tag as four 64-bit little-endian words at offsets 296, 304, 312 and 320. No side mixes plain and atomic access to these bytes. Only the lane `generation` is plain: it is written once at creation and never changed.

Every state change is a compare-and-swap. The field writes that belong to a transition happen before it and are published by its Release; a reader Acquire-loads the state before reading those fields. Who changes the state:

- allocator (geth, one serialized owner, outside the transaction hot path): `FREE→BOUND`, `BOUND→CLOSING`, `CLOSING→FREE`;
- consumer or allocator: any state `→QUARANTINED`.

### Bind (`FREE → BOUND`)

1. require state `FREE`;
2. require `shutdown == 0`; `shutdown` is terminal (set by a runtime shutdown or a quarantine) and is never cleared;
3. `next = lane_epoch + 1`; if `lane_epoch == UINT64_MAX`, fail (`epoch exhausted`) without any change;
4. require `write_sequence == read_sequence`; otherwise quarantine the lane (reason late write) and fail;
5. store owner, `final_write_sequence = 0`, `lane_epoch = next`;
6. compare-and-swap `FREE → BOUND`.

`drained_epoch` of the previous binding is not cleared: it names an older epoch, which never equals the new one.

The lease of this binding is `(lane id, owner, lane_epoch)`. The peer handler is the lane's only producer until it closes.

### Publish

Unchanged from C1.2: the producer Acquire-loads the state and publishes only on `BOUND`. Publish on `FREE`, `CLOSING` or `QUARANTINED` is refused without writing a slot.

### Close (`BOUND → CLOSING`)

Called by the producer after its last publish:

1. require state `BOUND` and the lease's owner and epoch;
2. `final_write_sequence = write_sequence`;
3. compare-and-swap `BOUND → CLOSING`.

### Drain acknowledgement (consumer)

On a `CLOSING` lane the consumer keeps consuming the old owner's records. When `read_sequence == final_write_sequence` and `write_sequence == final_write_sequence`, it Release-stores `drained_epoch = lane_epoch`. If `write_sequence != final_write_sequence`, a record was written after the close: the consumer quarantines the lane and reports it.

### Release (`CLOSING → FREE`)

1. require state `CLOSING` and the lease's owner and epoch;
2. require `write_sequence == final_write_sequence`; otherwise quarantine the lane and fail;
3. require `drained_epoch == lane_epoch` and `read_sequence == final_write_sequence`; otherwise fail (`not drained`), with no change;
4. clear owner and `final_write_sequence`;
5. compare-and-swap `CLOSING → FREE`.

Only then may a new peer receive the lane.

### Quarantine

1. `shutdown = 1`;
2. compare-and-swap `quarantine_reason` from 0 to the reason: the first quarantining party's reason is kept;
3. compare-and-swap the state from whatever it is to `QUARANTINED`. Exactly one call makes this transition per lane and generation.

Quarantine is idempotent and terminal for the generation: no bind, close, release, publish, drain acknowledgement or consumption is accepted afterwards. It destroys no record, advances no sequence and performs no repair. Recovery is a new generation.

| quarantine_reason | Meaning |
| ---: | --- |
| 0 | none |
| 1 | committed sequence mismatch |
| 2 | payload length beyond the slot |
| 3 | record lane epoch differs from the lane's |
| 4 | CRC32C mismatch |
| 5 | unknown record kind |
| 6 | record written outside its binding (late write) |
| 7 | unknown lifecycle state value |
| 8 | restart: the generation was abandoned |
| 9 | operator |

### Consumer admission and corruption

For every poll the consumer loads `read_sequence`, Acquire-loads `write_sequence`, then Acquire-loads the state. Loading the state after `write_sequence` guarantees that a record published under `BOUND` is never seen with an older state.

1. `QUARANTINED`: refuse (`lane quarantined`, with the reason); count nothing.
2. Any state value other than `FREE`, `BOUND`, `CLOSING` and `QUARANTINED`: quarantine with reason 7, whether or not a record is waiting.
3. Empty lane: no record; if `shutdown == 1` report the lane's end.
4. `BOUND`: admit the next record. `CLOSING`: admit it only while `read_sequence < final_write_sequence`. `FREE`, or `CLOSING` at or past `final_write_sequence`: the record was written outside its binding; quarantine with reason 6.
5. Validate in this order: committed sequence, payload length, record lane epoch, CRC32C, record kind. The first failure quarantines the lane with its reason.

The drain acknowledgement applies steps 1 and 2 as well before it looks at `CLOSING`.

Only the call that moves the lane into `QUARANTINED` for reasons 1–5 or 7 increments `corrupt_records` and reports the defect; every later poll reports `lane quarantined` and counts nothing. `read_sequence` never advances past a refused record, so no corrupt record is skipped.

### Epochs and sequences across reuse

- Every bind increments `lane_epoch`; an epoch is never reused; exhaustion fails closed.
- Every record carries the `lane_epoch` of its binding, and the consumer requires it to equal the lane's current `lane_epoch`.
- `write_sequence` and `read_sequence` are never reset on reuse. At release `read_sequence == write_sequence == final_write_sequence`, so every committed slot of an earlier binding holds a `committed_sequence <= final_write_sequence`, which can never equal a later expected `read_sequence + 1`. Even if it did, its record epoch is older than the lane's. A slot of an earlier binding is therefore never accepted.

### Generation and restart

The arena generation is authoritative: opening with another generation, or a lane header whose generation differs from the SuperHeader, fails closed. A new runtime generation creates a new arena (`create_new`); it never reuses the old one. Before abandoning the old arena, the restarting runtime may open it with its own generation and quarantine every lane, which stops any old producer. There is no hot-path repair and no automatic destructive recovery.

### Shutdown and mapping lifetime

Shutdown sets `shutdown = 1` on every lane and refuses every later bind and pool allocation. Publish refuses on `shutdown = 1`. Close, drain acknowledgement and release of existing bindings stay possible, so a shutdown ends deterministically. A consumer drains what was written and then sees the lane's end.

No mapping is unmapped while a user can still reach it:

- geth: the runtime that owns the arena keeps the mapping and its role lease for the life of the process. Deactivating the runtime makes it unreachable for new callers, but a publisher that loaded it earlier keeps a valid mapping. The operating system unmaps and unlocks at process exit. There is no reference count in the publish path.
- Rust: each producer, consumer and arena handle owns its own mapping and unmaps it only when dropped, which ownership orders after its last use.

### Runtime role leases

Each Fast-Lane directory has two exclusive roles, held independently and at the same time: `<dir>/geth-owner.lock` and `<dir>/rust-owner.lock`. A role is taken with `flock(LOCK_EX | LOCK_NB)` on that file and held through the open descriptor for the role's lifetime; a second process of the same role is refused. The kernel lock is the only source of exclusivity. The file's content is diagnostic:

```text
version=1
role=<geth-owner|rust-owner>
generation=<arena generation>
pid=<process id>
```

Closing the descriptor, or the death of the process, releases the lock. Lock files are never deleted. The arena generation remains authoritative over the lease content.

### Arena files, generations and restart

An arena lives at `<dir>/fastlane-v2-g<generation as 20 decimal digits>.mmap`, so a new generation never collides with an earlier generation's file. Creation uses `O_EXCL`; an existing file at the path is never attached, truncated or overwritten. Create fails with an explicit `arena exists` result carrying the probe.

A probe reads only the SuperHeader and classifies the file:

- absent;
- current generation;
- stale generation, with the observed generation;
- not an arena.

Stale arena files in a directory can be listed. Nothing is deleted automatically; disposal belongs to the startup orchestration (C1.8). Opening an arena with another generation fails closed.

### Peer-lane pool

The pool owns all peer-ingress lanes of one arena. Its bounds are `max_peers + churn_reserve`, which must equal the arena's peer-lane count, so a configuration asking for more peers than the arena qualifies fails at startup. Allocation:

- refuses explicitly (`peer limit`) when `max_peers` lanes are `BOUND`;
- binds only a `FREE` lane, never takes a `BOUND`, `CLOSING` or `QUARANTINED` one;
- returns an explicit `pool exhausted` result when no lane is `FREE`;
- never waits, sleeps or spins for a lane;
- counts allocations, exhaustions, peer-limit refusals and releases.

The pool is the allocator role above; its calls are serialized (a mutex in geth). After shutdown began, allocation is refused. Mapping a pool result to GAP/health evidence is C1.4.

## C1.4 geth ingress records

C1.4 changes no offset, size or field of Wire V2. It fixes the meaning of fields for the records geth's ETH ingress publishes.

### Lane plan

| Lane | ID | Class | Initial state | Capacity × stride | Sole producer |
| --- | --- | --- | --- | --- | --- |
| Txpool | 1 | `TXPOOL_EVIDENCE` | `BOUND` | 4096 × 4096 | geth `txBroadcastLoop` |
| Peer *n* | 16 + *n* | `PEER_INGRESS` | `FREE` | 64 × 4096 | the bound peer's handler goroutine |

- There are `max_peers + churn_reserve` peer lanes. The qualified bound for that sum is 256.
- Startup fails above the bound. Handler start also fails if geth's `MaxPeers` exceeds the configured `max_peers`.
- Configuration: `DARKBOT_FASTLANE_V2_DIR`, `DARKBOT_FASTLANE_V2_GENERATION`, `DARKBOT_FASTLANE_V2_MAX_PEERS`, `DARKBOT_FASTLANE_V2_CHURN_RESERVE`.
- The arena file, role lease, generation, shutdown and mapping lifetime follow the C1.3 rules above.

### Peer lane = connection

1. **Bind.** After geth registered the peer (`runEthPeer`, after `registerPeer`), one FREE peer lane is bound to the peer's node ID, outside the transaction hot path.
2. **Lease.** The lease (lane id, owner = node ID, epoch) is held on the connection's `eth.Peer`. Publishing needs no lock and no lookup.
3. **Single producer.** The peer's handler goroutine, which runs the connection's message loop, is the lane's only producer.
4. **Close.** Close is deferred in `runEthPeer`. It runs after the message loop returned, i.e. after the last publish, and before `unregisterPeer`: `BOUND → CLOSING`.
5. **Release.** The lane is released only after Rust acknowledged the drain of that epoch. Release is attempted on later binds and snapshots.
6. **Stale handles.** Publishing checks `lane_epoch == lease epoch` before writing anything. A closed handle, or any handle of an earlier binding, never writes into a later binding.

If no lane can be bound, the peer still takes part in normal ETH processing, but its Fast-Lane coverage gap is explicit, never silent:

- **Reasons:** `pool_exhausted`, `peer_limit`, `shutdown`, `bind_failed`.
- **Counters:** uncovered binds and the uncovered peer's prefilter-matching candidates are counted.
- **Gap events:** bounded structured events (peer, reason, monotonic time) record each gap.

A lane that can no longer be released (quarantined) leaves the pool visibly (`lane_lost`).

### Boundaries

| Boundary | Where | Count | Actionable |
| --- | --- | --- | --- |
| `T_packet_decoded` | Immediately after the Transactions/PooledTransactions packet is decoded into transaction objects, before geth's basic validation for the hook (`handleTransactions`). | Once per packet. | No, diagnostic only. |
| `T_ingress` | Immediately after that validation succeeded, before prefilter, serialization and publication. | Once per packet. | Yes. |

A packet the validation rejects publishes nothing. Its error reaches geth unchanged, and geth drops the peer as before. Fast-Lane refusals (queue full, oversize, lane state, stale lease) are counted and never returned to geth.

### `EARLY_TX_SEEN` on a peer lane

| Field | C1.4 meaning |
| --- | --- |
| `timestamp_ns` | `T_ingress` |
| `aux[0..8]` | `T_packet_decoded`, u64 little-endian, `≤ timestamp_ns` |
| `aux[8..32]` | zero, reserved |
| `clock_id` | `1` (`CLOCK_MONOTONIC`) for both times |
| `source` | `eth-broadcast` (Transactions) or `eth-pooled` (PooledTransactions), zero-padded to 16 bytes |
| `peer` | the sending peer's 32-byte node ID |
| `hash` | the transaction hash |
| payload | the canonical signed transaction (`MarshalBinary`) |

Golden: `T_packet_decoded = 0x0102030405060708` encodes as `aux = 0807060504030201` followed by 24 zero bytes.

### `TXPOOL_ACCEPTED` on the Txpool lane

Every transaction the local txpool accepted is published, independent of the peer prefilter. The fields are:

- `source = txpool`, zero `peer`, zero `aux`;
- `timestamp_ns` is the observation time in `txBroadcastLoop`;
- the payload is the canonical signed transaction.

Queue-full and oversize refusals are counted in the lane header. They make the false-negative audit incomplete, not passed.

### Prefilter

After `T_ingress`, the peer handler applies a conservative superset filter to each transaction:

1. destination in the address set;
2. 4-byte selector in the selector set;
3. only then serialize and publish.

A non-matching transaction is not published on the peer lane; its normal geth path is unchanged. The filter is never venue authority; Rust is.

The scope is an immutable startup snapshot (`DBC14PF1`). Its canonical bytes are:

```text
"DBC14PF1" | chain_id u64 LE | address count u16 LE | addresses (20 B, ascending)
| selector count u16 LE | (selector 4 B, role u8) ascending by selector    role 1 = launch
```

The peer-ingress prefilter is the supported launch-selector superset (C1 plan §7). For C1.4 it is taken only from the committed Rust venue scope (`darkbot-core::venues::flap`):

- deployment `bsc_mainnet_v5_24_0` (chain 56, Portal `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0`);
- `NEW_TOKEN_V6_SELECTOR` `0x8cb5772c`, the launch call of every supported `newTokenV6` mode.

BUY traffic (`swapExactInput`) is not launch detection and is not published on peer lanes; its normal geth path is unchanged. The only role value is 1 (launch).

```text
canonical  444243313450463138000000000000000100e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de001008cb5772c01
fingerprint CRC32C 0x1692c814
```

Go, the Rust mirror and a darkbot-core test recomputing the bytes from the venue constants must all produce this value.

### False-negative audit (Rust, outside the hot path)

The audit decodes each `TXPOOL_ACCEPTED` payload itself (legacy, types 1–4) and judges it against the scope. It shares no parsing with geth.

**Ordering.** A transaction's peer-lane publish happens before geth hands it to the fetcher and txpool, so it happens before its `TXPOOL_ACCEPTED` publish. The audit therefore drains the Txpool lane first, then every peer lane, and only then judges.

**Time rule.** Per hash the audit keeps the earliest valid peer-lane `T_ingress` (`EARLY_TX_SEEN.timestamp_ns`). An in-scope transaction counts as observed only if that earliest `T_ingress` is not later than its `TXPOOL_ACCEPTED.timestamp_ns`; equality counts as observed. A peer record stamped after the acceptance does not count, even if it is drained before the judgement.

**Comparability.** Only records inside the C1.4 contract take part in the comparison:
- `EARLY_TX_SEEN` needs `clock_id` 1, source `eth-broadcast` or `eth-pooled`, zero `aux[8..32]` and `0 < T_packet_decoded ≤ T_ingress`.
- `TXPOOL_ACCEPTED` needs `clock_id` 1, source `txpool`, zero `peer` and zero `aux`.

A record outside that contract is counted and makes the audit result invalid evidence; it is never treated as an observation.

**Verdicts.**
- In-scope transactions without an observation at or before their acceptance are C1.4 false-negative evidence and fail the qualification.
- The evidence names the transaction, its acceptance time and, if a peer lane carried it later, that later `T_ingress`.
- Txpool-lane drops, refusals or undecodable payloads make the audit incomplete.

**Qualification (fail-closed).** `IngressAuditV2::qualification(txpool_lane_stats, producer_counters)` runs at a quiescent point, in this order:

1. The Txpool lane is sealed: `shutdown` is set (geth `ShutdownIngressV2`, which runs after the handler stopped `txBroadcastLoop`; Rust `ArenaV2::begin_shutdown`). From then on every publish is refused.
2. The final producer counters are read after the last producer call and before the qualification.
3. The audit polls until the lane is drained.

A lane that is drained but not sealed is not closed: the producer could publish the next record.

On a sealed lane the consumer returns `Shutdown` once `read == write` (C1.3 semantics, unchanged). The audit treats that as the normal end of the lane, never as an error. Records written before the shutdown are still read in full. Every other consumer error (including `LaneQuarantined`, although quarantine also sets `shutdown`) is counted and makes the evidence invalid. The producer counters are those of `fastlane.IngressV2Snapshot()` (`txpool_published`, `txpool_refused`, `txpool_serialize_errors`, `txpool_clock_errors`, each counting transactions). They are the only evidence of a transaction the producer could not stamp, serialize or publish, because such a transaction leaves no `TXPOOL_ACCEPTED` record. They travel outside the wire; the layout is unchanged. The result is `Pass` only if every condition holds:

| Condition | Otherwise |
|---|---|
| no invalid peer or txpool record, no foreign record kind, no lane consumer error | `InvalidEvidence` |
| Txpool lane without corrupt records, not quarantined, no recorded quarantine reason | `InvalidEvidence` |
| no false negative | `FalseNegatives(n)` |
| producer counters available | `Incomplete` |
| producer `refused`, `serialize_errors`, `clock_errors` all zero | `Incomplete` |
| Txpool lane `dropped_full` and `oversize` zero | `Incomplete` |
| no unsupported or malformed txpool payload | `Incomplete` |
| Txpool lane `shutdown` set: producer window closed | `Incomplete` (`txpool producer window still open`) |
| Txpool lane state `BOUND` | `InvalidEvidence` |
| Txpool lane `read_sequence == write_sequence` | `Incomplete` |
| producer `published == write_sequence` | `Incomplete` |
| every read txpool record judged | `Incomplete` |

### V1 status after C1.4

The C1.4 overlay removes V1 peer ingress (`EARLY_TX_SEEN`) and V1 `TXPOOL_ACCEPTED`. The V1 ring still carries `CANONICAL_HEAD`, `REORG` and local-geth outbound until later C1 packages migrate them. V1 is not C1.4-authoritative ingress.

## Slot header

| Offset | Size | Field |
| ---: | ---: | --- |
| 0 | 8 | committed_sequence |
| 8 | 2 | record kind |
| 10 | 2 | flags |
| 12 | 4 | payload length |
| 16 | 8 | monotonic timestamp ns |
| 24 | 4 | clock id |
| 28 | 4 | reserved zero |
| 32 | 16 | source |
| 48 | 32 | peer |
| 80 | 32 | hash |
| 112 | 32 | aux |
| 144 | 8 | lane epoch |
| 152 | 4 | CRC32C |
| 156..255 | 100 | reserved zero |
| 256.. | variable | payload |

An unused slot has `committed_sequence = UINT64_MAX`.

## Publication and consumption

Producer:

1. read its lane-local `write_sequence`;
2. acquire the lane-local `read_sequence` when capacity must be checked;
3. refuse full/oversize/shutdown without overwriting unread data;
4. set slot `committed_sequence = UINT64_MAX`;
5. write metadata and payload;
6. compute/store CRC32C;
7. Release-store `committed_sequence = write_sequence + 1`;
8. Release-store lane `write_sequence = write_sequence + 1`.

Consumer:

1. read lane-local `read_sequence`;
2. Acquire-load lane `write_sequence`;
3. if non-empty, Acquire-load the slot `committed_sequence`;
4. require `committed_sequence == read_sequence + 1`;
5. validate payload bound, lane epoch and CRC32C before exposing the record;
6. after consumption, Release-store `read_sequence = read_sequence + 1`.

There is no global ordering across lanes. Lane position must never be interpreted as cross-lane chronology.

## CRC32C byte domain

The CRC is Castagnoli CRC32C, reflected polynomial `0x82f63b78`, init `0xffffffff`, final xor `0xffffffff`.

For one record the exact CRC input is:

```text
slot bytes [8, 152)
followed immediately by
payload bytes [256, 256 + payload_len)
```

Thus the CRC covers kind, flags, payload length, timestamp, clock id, reserved-zero word, source, peer, hash, aux, lane epoch and payload. It excludes `committed_sequence`, the CRC field and padding.

Standard check:

```text
CRC32C("123456789") = 0xe3069283
```

Cross-language V2 golden record:

```text
kind         = 1
flags        = 3
payload      = "signed-transaction"
timestamp_ns = 123456789
clock_id     = 1
source       = 16 × 0x11
peer         = 32 × 0x22
hash         = 32 × 0x33
aux          = 32 × 0x44
lane_epoch   = 7

CRC32C(metadata[8:152] || payload) = 0xde714cce
```

Go and Rust must both assert these vectors before C1.2 can pass.

## C1.2 non-claims

Wire V2 correctness alone does not prove:

- peer-lane lifecycle/reuse safety (C1.3; implemented by the protocol above, qualified only by its tests and later C1.8 stress);
- actionable ingress placement or filter coverage (C1.4);
- Rust zero-copy hot consumption (C1.5);
- local-geth fanout latency (C1.7/C1.9);
- Mainnet ingress coverage;
- provider ordering/inclusion;
- `T0 → T_buy < 1 ms`;
- LIVE authorization.
