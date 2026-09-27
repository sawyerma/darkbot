# DarkBot — Phase D / M7 Flap.sh Rust Decoder Contract

Status: **BLOCKED UNTIL NATIVE PHASES C0-C3 ARE ACCEPTED**
Milestone: **M7**
End rule: **STOP after independent Phase-D review**

Normative parent: `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`.

## 1. Purpose

Phase D adds factual Flap recognition/decoding directly to the single normative Rust runtime.

It does not create a Python trading decoder and does not create a special LIVE decoder.

The exact same Flap Rust module must later run in native capture, deterministic replay, PAPER/shadow and LIVE.

## 2. Preconditions

Do not start until:

1. Phase C candidate is accepted/merged or explicitly frozen as reference baseline;
2. Native Runtime Gate is accepted;
3. exact FastLaneEvent capture/replay identity is proven;
4. T0-T8 native benchmark has no unresolved reproducible >3 ms runtime path;
5. Flap Mainnet evidence is frozen;
6. full repository regression is green.

## 3. Evidence freeze

Before coding constants, freeze evidence for every supported Flap deployment/version:

- BSC chain id;
- contract/deployment address;
- implementation/version relationship;
- ABI source + hash;
- bytecode/code hash where useful;
- verified launch/BUY/SELL selectors;
- relevant event topics;
- canonical fixture tx/block/log identities;
- pending launch samples;
- deterministic token-address derivation proof or explicit UNKNOWN;
- mode/config fields actually present on-chain;
- provenance/retrieval date.

Unknown/conflicting evidence => UNSUPPORTED. Never guess.

## 4. Normative implementation location

Flap runtime semantics belong in Rust, inside the single `darkbot-core` venue module.

Target shape:

```
native/crates/darkbot-core/src/venues/flap.rs
```

Split files only when measured/verified complexity requires it.

Python may contain fixture generation, analysis and reference tooling only. Python output is not allowed to determine a LIVE trade.

## 5. Hot-path recognizer

Use the cheapest verified checks first:

1. destination address;
2. 4-byte selector;
3. small fixed version lookup;
4. full decode only after match.

Forbidden in recognizer/decoder:

- HTTP/RPC/DNS;
- DB/filesystem;
- dynamic ABI downloads;
- JSON reserialization;
- general reflection;
- strategy/economics;
- logging allocation on common path.

Large or malformed calldata must fail closed without creating a false match.

## 6. Pending decoding

Decode only verified fields available in FastLaneEvent/raw signed transaction evidence.

Preserve:

- exact native first_seen timestamp;
- sequence id/source;
- tx hash;
- sender/nonce;
- destination/version;
- selector;
- public launch parameters;
- native value;
- explicit mode/config fields;
- predicted token id only if derivation is proven.

Pending evidence is never canonical existence.

## 7. Canonical decoding

Decode verified canonical evidence for:

- CREATE/launch;
- BUY;
- SELL;
- migration/graduation where proven;
- factual mode/config transitions.

Revert must not become a successful action.
Reorg/orphan/uncertain transitions must remain auditable.

## 8. Capture/replay/live identity

Fixtures must be expressible as FastLaneEvent/canonical-event bytes.

A positive fixture is tested by feeding it to the same Rust function used in LIVE.

Forbidden:

```
fixture -> Python decoder -> expected runtime event
LIVE    -> different Rust decoder
```

Required:

```
fixture bytes -> Rust decoder
LIVE bytes    -> same Rust decoder
```

## 9. Performance gate

Measure Flap recognize and decode separately on target-like hardware.

Report p50/p95/p99/p99.9/p99.99/max.

The implementation must not materially consume the 0.5 ms T0-T8 engineering budget. Any deterministic >1 ms Flap decode path must be treated as a defect unless evidence proves an unavoidable protocol-size case, which then requires a fail-closed latency policy.

## 10. Out of scope

No:

- quote/economics;
- Strategy C;
- tx execution;
- PAPER/LIVE broadcast;
- Four.meme changes except shared interface fixes proven necessary;
- second network ingest;
- second state machine.

## 11. Tests

At minimum:

- each supported address/version;
- selector positives/negatives;
- malformed/truncated/oversize calldata;
- unknown version;
- pending launch fixture;
- canonical CREATE/BUY/SELL;
- revert;
- duplicate/idempotency;
- orphan/uncertain propagation;
- exact timestamp preservation;
- FastLaneEvent binary round-trip;
- capture -> replay -> identical decode result;
- no allocation/latency regression on common recognizer path where measurable.

## 12. Acceptance

Phase D is accepted only if:

- all claimed protocol facts are evidence-backed;
- Rust decoder is the only normative Flap runtime decoder;
- capture/replay share the exact same decoder;
- unknowns fail closed;
- full tests are green;
- latency gate passes.

Then **STOP**. Do not start Phase E automatically.
