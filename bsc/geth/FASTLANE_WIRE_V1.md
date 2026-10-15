# C1 Fast-Lane wire contract v1

Status: **IMPLEMENTATION CONTRACT / NOT YET QUALIFIED**

Pinned chain client:

```text
bnb-chain/bsc v1.7.8
commit cdb7548b5baacfdae92f9f63437d6456411665f3
tree   d8d88097521d43cef66047fdd93dd6825a2a7843
```

The local IPC is two independent shared-memory SPSC rings:

```text
geth -> ingress ring -> Rust
Rust -> egress ring  -> geth
```

Each ring has exactly one producer and one consumer. File creation is a
bootstrap responsibility and is deliberately independent of producer/consumer
role so geth can create both files before Rust starts.

## Layout

All integers are little-endian. The mapping base and every slot are 64-byte
aligned. Version 1 uses:

```text
RingHeader size = 128 bytes
SlotHeader size = 192 bytes
slot_size       = configured multiple of 64
capacity        = configured fixed slot count
```

RingHeader byte offsets:

```text
0    [8]  magic = "DBFLN001"
8    u16  version = 1
10   u16  direction: 1 geth->Rust, 2 Rust->geth
12   u32  header_size = 128
16   u64  generation
24   u32  capacity
28   u32  slot_size
32   [24] reserved
56   u64 atomic write_sequence
64   u64 atomic read_sequence
72   u64 atomic dropped_full
80   u64 atomic corrupt_records
88   u64 atomic shutdown
96   [24] reserved
```

SlotHeader byte offsets:

```text
0    u64 atomic committed_sequence
8    u16 record_kind
10   u16 flags
12   u32 payload_len
16   u64 timestamp_ns
24   [16] source
40   [32] peer/source identity
72   [32] hash
104  [32] aux
136  u32 CRC32(payload)
140  [20] reserved
192  payload bytes...
```

The committed slot marker is `sequence + 1`; `u64::MAX` means not
published. The producer fills metadata/payload and then performs a Release
store to `committed_sequence`, followed by a Release update to
`write_sequence`. The consumer observes `write_sequence` and
`committed_sequence` with Acquire loads and advances `read_sequence` with a
Release store only after copying/validating the record.

Unread data is never overwritten. Full ring => explicit `QueueFull` and
`dropped_full += 1`. Generation, direction, capacity, slot size, magic and
version mismatches fail closed.

## Record kinds

```text
1 EARLY_TX_SEEN
2 TXPOOL_ACCEPTED
3 CANONICAL_HEAD
4 REORG
5 OUTBOUND_SIGNED_TX
6 OUTBOUND_PEER_HANDOFF
7 GAP
8 SHUTDOWN
```

`EARLY_TX_SEEN` is the earliest safely decoded **complete signed transaction
envelope** available from the pinned geth network handler, before txpool
acceptance. Hash-only announcements are not promoted to this record.

`TXPOOL_ACCEPTED` is separate evidence and never replaces first-seen.

`CANONICAL_HEAD` / `REORG` are provenance/reconciliation inputs. They do
not authorize trading by themselves.

`OUTBOUND_SIGNED_TX` carries the exact signed transaction bytes selected by
Rust. `OUTBOUND_PEER_HANDOFF` is geth-side evidence after the selected
network write/handoff boundary. Local geth acceptance is not acquisition.

## Clock

Both processes must use the same host monotonic clock family. C1 uses
`clock_gettime(CLOCK_MONOTONIC)` nanoseconds for cross-process boundaries.
Wall-clock timestamps and BSC block timestamps are not substituted for this
clock.

## Restart / corruption

A new process generation creates new ring files with `create_new` semantics.
Existing files are not silently reused. Reopen is allowed only when every
header identity field and the expected generation match. Corrupt sequence,
CRC, payload bound or header identity fails closed and increments explicit
evidence where applicable.

No automatic recovery from a stale ring generation is part of C1. C2 owns
canonical safe-resume.

## Scope

This wire contract does not prove source coverage, arrival advantage,
allocation freedom, local-geth inclusion quality, provider semantics, C3
integrated latency or `T0 -> T_buy < 1 ms`. Those require their named later
qualification gates.
