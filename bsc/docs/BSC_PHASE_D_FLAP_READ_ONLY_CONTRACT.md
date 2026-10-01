# DarkBot — Phase D / M7 Flap Production Rust Decoder Contract

Status: **BLOCKED UNTIL BSC GATE-0 PASS + C0-C3 ACCEPTED**
Milestone: **M7**
End rule: **STOP after independent Phase-D review**

Normative parents:
- `docs/GATE0_TESTPLAN_SHORT.md`
- `docs/BSC_DUAL_VENUE_ROADMAP.md`
- `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`

## 1. Scope

Phase D creates the **full production Flap decoder** in the single Rust runtime.

Gate 0 is explicitly allowed to contain a smaller Flap feasibility probe before Phase D. That probe exists only to prove:
- launch recognition;
- pre-`T0` token/address derivation where possible;
- exact first-BUY construction;
- ordered execution feasibility.

It must not become a second production decoder.

## 2. Preconditions

Do not start full Phase D until:

1. Flap has PASS from Gate 0;
2. C0-C3 production foundation is accepted;
3. current Flap Mainnet evidence is refrozen;
4. FastLane capture/replay identity is proven;
5. no unresolved reproducible >3 ms local production path remains;
6. full repository regression is green.

## 3. Evidence freeze

For every supported current Flap deployment/version freeze:

- BSC chain id;
- contract/deployment address;
- implementation/version relationship;
- ABI source/hash;
- code hash where useful;
- launch/BUY/SELL selectors;
- relevant event topics;
- canonical fixture tx/block/log identities;
- pending launch samples;
- deterministic token-address derivation proof or explicit UNKNOWN;
- mode/config fields;
- provenance/retrieval date.

Unknown/conflicting => UNSUPPORTED.

Gate-0 facts must be reconfirmed if contracts changed.

## 4. Implementation ownership

Normative implementation:

```
native/crates/darkbot-core/src/venues/flap.rs
```

One production implementation only.

Python may analyze fixtures/results but never determine a LIVE trade.

## 5. Hot-path recognition

Order:

1. destination;
2. 4-byte selector;
3. fixed version lookup;
4. full verified decode.

Forbidden:
- HTTP/RPC/DNS;
- DB/filesystem;
- dynamic ABI downloads;
- JSON reserialization;
- reflection;
- logging allocation.

Malformed/oversized input fails closed.

## 6. Pending semantics

Preserve:

- first-seen timestamp;
- sequence/source;
- tx hash;
- sender/nonce;
- destination/version;
- selector;
- public launch parameters;
- native value;
- explicit mode/config;
- predicted token id only when derivation is proven.

Pending evidence is not canonical existence.

## 7. Canonical semantics

Decode only verified:
- launch;
- BUY;
- SELL;
- migration/graduation;
- factual mode/config transitions.

Receipt revert is never success.
Reorg/orphan/uncertain state remains auditable.

## 8. Identity across modes

Required:

```
fixture bytes -> same Rust decoder
capture bytes -> same Rust decoder
replay bytes  -> same Rust decoder
LIVE bytes    -> same Rust decoder
```

No Python/alternate LIVE decoder.

## 9. Performance

Measure recognizer and full decode separately on target-like hardware.

Report p50/p95/p99/p99.9/p99.99/max.

A deterministic >1 ms venue decode path is a defect/review item. This is a local decoder metric, not proof of actual BUY execution.

## 10. Out of scope

Phase D does not add:
- Strategy C;
- second ingress;
- second state machine;
- alternative execution semantics.

Full economics remains Phase G, but any economics already required for a valid Gate-0 first BUY is treated as validated feasibility evidence and must not be contradicted.

## 11. Tests

At minimum:
- every supported address/version;
- selector positives/negatives;
- malformed/truncated/oversized calldata;
- unknown version;
- pending launch fixture;
- canonical launch/BUY/SELL;
- revert;
- duplicate/idempotency;
- orphan/uncertain;
- exact timestamp preservation;
- FastLane binary roundtrip;
- capture→replay identical decode;
- no common-path allocation/latency regression where measurable.

## 12. Acceptance

Accept only if:
- all claimed protocol facts are evidence-backed;
- Gate-0 Flap facts remain valid;
- Rust decoder is sole production source of truth;
- unknowns fail closed;
- capture/replay/LIVE share the decoder;
- regression + latency gates pass.

Then STOP.
