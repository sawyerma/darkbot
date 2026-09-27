# DarkBot — Phase E / M8 Four.meme Rust Decoder Contract

Status: **BLOCKED UNTIL PHASE D IS ACCEPTED**
Milestone: **M8**
End rule: **STOP after independent Phase-E review**

Normative parent: `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`.

## 1. Purpose

Phase E adds Four.meme factual recognition/decoding to the same Rust runtime already used by the native BSC Fast Lane and Flap.

There is no second Four.meme decoder for tests, capture or Python.

## 2. Preconditions

- Phase C/reference baseline accepted;
- Native Runtime Gate accepted;
- Phase D accepted;
- Four.meme Mainnet evidence frozen;
- full regression green;
- no unresolved native latency blocker.

## 3. Evidence freeze

Freeze for every supported current Mainnet version/mode:

- TokenManager/TokenManager2 addresses and implementation identities;
- Helper identity only as validation evidence, never hot-path dependency;
- ABI hash/source;
- useful bytecode/code hashes;
- CREATE/BUY/SELL selectors;
- TokenCreate/TokenPurchase/TokenSale topics;
- LiquidityAdded/graduation evidence;
- version-detection rule;
- TaxToken/X/AntiSniper mode evidence;
- canonical fixtures;
- pending launch fixtures;
- deterministic token-id derivation proof or UNKNOWN;
- provenance and retrieval date.

Only observed/verified modes are supported. Unknown => UNSUPPORTED.

## 4. Runtime ownership

Normative code lives in:

```
native/crates/darkbot-core/src/venues/fourmeme.rs
```

Common BSC mechanics stay in shared Rust state/event code.

Flap and Four.meme may share generic fixed EVM primitives, but neither venue may contain conditional logic for the other.

## 5. Recognition/decode

Recognition order:

1. destination;
2. selector/topic;
3. fixed version/mode lookup;
4. verified decode.

No RPC, HTTP, DB, filesystem, JSON, metadata API or dynamic ABI lookup.

Malformed/oversized inputs fail closed and cannot enter Strategy later as valid venue evidence.

## 6. Pending and canonical semantics

Pending decode preserves exact native first-seen/sequence/source identity.

Canonical decode supports only verified:

- CREATE;
- BUY;
- SELL;
- liquidity/graduation;
- factual mode/config state.

Receipt failure/revert never maps to success.

Reorg/orphan/uncertain semantics remain those of the shared Rust state model.

## 7. Mode/version rules

Mode and version are explicit.

No silent "normal" default.

TaxToken/X/AntiSniper labels are factual only in Phase E. Their economic formulas/rates are deferred to the Economics Gate.

## 8. Single-runtime parity

Every test fixture enters the same Rust decoder API used by LIVE.

The exact Four.meme decoder is reused by:

- Phase-F capture processing;
- M11 replay;
- M12 PAPER/shadow;
- LIVE.

No separate Python or "fast live" implementation is permitted.

## 9. Performance

Measure recognizer and full decode separately and with Flap simultaneously enabled.

Report p50/p95/p99/p99.9/p99.99/max.

Destination/selector rejection for unrelated BSC traffic must remain extremely cheap and bounded.

Any reproducible >1 ms decoder path is a defect/review item because the total native engineering target is <0.5 ms and hard economic failure is >3 ms.

## 10. Out of scope

No economics, Strategy C, tx execution, broadcast, approval logic, second feed, second state engine or Python trading implementation.

## 11. Required tests

- every supported manager/version;
- selector positives/negatives;
- unknown version/mode;
- malformed/truncated/oversized calldata;
- pending launch fixtures;
- canonical CREATE/BUY/SELL/liquidity fixtures;
- revert;
- duplicate/idempotency;
- orphan/uncertain;
- explicit mode fixtures;
- FastLaneEvent binary round-trip;
- same captured event -> same Rust decode in replay;
- Flap+Four shared-input test without duplicate raw evidence.

## 12. Acceptance

Accept only when evidence, correctness, single-runtime parity and latency tests pass.

Then **STOP**. Do not begin Phase F automatically.
