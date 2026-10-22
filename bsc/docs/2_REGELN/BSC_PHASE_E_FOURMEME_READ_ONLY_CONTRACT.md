# DarkBot — Phase E / M8 Four.meme Production Rust Decoder Contract

Status: **BLOCKED UNTIL BSC GATE-0 PASS + PHASE D/C0-C3 ACCEPTED AS APPLICABLE**
Milestone: **M8**
End rule: **STOP after independent Phase-E review**

Normative parents:
- `docs/GATE0_TESTPLAN_SHORT.md`
- `../1_PLAN/PHASENPLAN.md`
- `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`

## 1. Scope

Phase E creates the full production Four.meme decoder in the same Rust runtime.

Gate 0 may contain a smaller Four.meme feasibility probe before Phase E. It must answer:
- launch recognition;
- whether token address + mandatory BUY inputs are knowable before `T0`;
- exact first-BUY construction;
- ordered launch→BUY feasibility.

The probe is not a second production decoder.

## 2. Preconditions

Do not start full Phase E until:

- Four.meme has PASS from Gate 0 if Four is being promoted;
- C0-C3 production foundation is accepted;
- Phase D/shared venue infrastructure is accepted where required by the roadmap;
- current Four.meme Mainnet evidence is refrozen;
- full regression is green;
- no unresolved native local-latency blocker remains.

## 3. Evidence freeze

For every supported current version/mode freeze:

- TokenManager/TokenManager2 address and implementation identity;
- Helper identity only as validation evidence, not hot-path dependency;
- ABI source/hash;
- useful code hashes;
- launch/create selector;
- BUY/SELL selectors;
- TokenCreate/TokenPurchase/TokenSale topics;
- liquidity/graduation evidence;
- version/mode detection;
- TaxToken/X/AntiSniper factual mode evidence;
- canonical fixtures;
- pending launch fixtures;
- deterministic token-address derivation proof or explicit UNKNOWN;
- provenance/retrieval date.

Critical rule:

> Never assume the token address is knowable from a pending create transaction merely because the canonical `TokenCreate` event later contains it.

Gate 0 must have proven this for the promoted mode, or the production runtime must treat pre-execution address availability as UNKNOWN.

## 4. Implementation ownership

Normative implementation:

```
engine/crates/trading/src/venues/fourmeme.rs
```

No second Python or fast-LIVE decoder.

## 5. Recognition/decode

Order:

1. destination;
2. selector/topic;
3. fixed version/mode lookup;
4. verified decode.

No RPC, HTTP, DB, filesystem, JSON metadata API or dynamic ABI lookup in the hot path.

Malformed/oversized inputs fail closed.

## 6. Pending/canonical semantics

Pending evidence preserves exact first-seen/sequence/source identity.

Canonical decode supports only verified:
- launch;
- BUY;
- SELL;
- liquidity/graduation;
- factual mode/config state.

Receipt failure/revert never maps to success.

## 7. Mode/version

No silent "normal" default.

TaxToken/X/AntiSniper labels are factual unless their economics are separately verified.

Gate-0 first-BUY economics for a supported mode must remain valid; complete economics belongs to Phase G.

## 8. Mode identity

Same Four.meme Rust decoder is used in:
- capture;
- replay;
- PAPER;
- LIVE.

## 9. Performance

Measure recognizer/full decode separately and with Flap enabled.

Report p50/p95/p99/p99.9/p99.99/max.

A reproducible >1 ms decoder path is a defect/review item. This is local runtime latency, not a purchase claim.

## 10. Out of scope

No Strategy C, second feed/state engine or duplicate execution implementation.

## 11. Tests

At minimum:
- every supported manager/version;
- selector positives/negatives;
- unknown version/mode;
- malformed/truncated/oversized calldata;
- pending launch fixtures;
- canonical launch/BUY/SELL/liquidity fixtures;
- revert;
- duplicate/idempotency;
- orphan/uncertain;
- explicit modes;
- FastLane roundtrip;
- same event→same replay decode;
- Flap+Four shared-input behavior;
- token-address pre-execution assumption tests: proven derivation or explicit UNKNOWN.

## 12. Acceptance

Accept only when:
- current evidence supports every claimed fact;
- Gate-0 promoted Four.meme assumptions remain true;
- Rust decoder is sole production source;
- unknowns fail closed;
- capture/replay/PAPER/LIVE use the same decoder;
- regression + latency tests pass.

Then STOP.
