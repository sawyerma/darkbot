# DarkBot Fast Lane Wire V2

Status: C1.2 byte layout plus the C1.3 lane-lifecycle protocol. This document defines the byte layout and the lifecycle shared by pinned bsc-geth and `darkbot-fastlane`. C1.3 changes no offset, size or field. C1.4 binds peer ingress to these lanes; C1.5 replaces the Rust owned-copy receive path with a borrowed guard.

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
