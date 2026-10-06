# DarkBot Fast Lane Wire V2

Status: C1.2 implementation contract. This document defines the byte layout shared by pinned bsc-geth and `darkbot-fastlane`. C1.3 adds lane lifecycle policy; C1.4 binds peer ingress to these lanes; C1.5 replaces the Rust owned-copy receive path with a borrowed guard.

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
| 328..383 | 56 | reserved |

C1.2 initializes `lane_epoch = 1` and state = 1 for configured lanes. C1.3 defines the complete FREE/BOUND/CLOSING lifecycle and reuse rules.

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

- peer-lane lifecycle/reuse safety (C1.3);
- actionable ingress placement or filter coverage (C1.4);
- Rust zero-copy hot consumption (C1.5);
- local-geth fanout latency (C1.7/C1.9);
- Mainnet ingress coverage;
- provider ordering/inclusion;
- `T0 → T_buy < 1 ms`;
- LIVE authorization.
