# DarkBot — Phase E / M8 Four.meme Read-Only Decoder Contract

Status: **FUTURE CONTRACT / BLOCKED UNTIL PHASE D IS ACCEPTED**  
Milestone: **M8**  
Scope: **Four.meme read-only recognition and decoding only**  
End rule: **STOP after Phase E review; do not begin Phase F automatically**

---

## 1. Purpose

Phase E adds Four.meme as a second independent read-only BSC venue decoder while reusing exactly the same Phase-C BSC ingest, canonical state and dispatcher used by Flap.

The purpose is factual decoding only.

No economics, strategy or execution is implemented here.

---

## 2. Preconditions

Phase E must not start until:

1. Phase C is accepted with all data-integrity blockers closed;
2. Phase D is independently reviewed and accepted;
3. Flap remains isolated in its own venue package;
4. complete regression suite is green;
5. Four.meme Mainnet BSC evidence required by §3 is frozen.

Failure of any precondition => **STOP**.

---

## 3. Mandatory Four.meme evidence freeze

Before adding production Four.meme constants, freeze evidence for every supported current Mainnet version/mode.

Where applicable:

- TokenManager/TokenManager2 deployed address(es);
- implementation/version relationship;
- Helper contract/version/address only as factual protocol evidence, not as a runtime hot-path dependency;
- ABI source and ABI hash;
- bytecode/code hash where useful;
- verified CREATE selector(s);
- verified BUY/purchase selector(s);
- verified SELL selector(s);
- TokenCreate topic/event;
- TokenPurchase topic/event;
- TokenSale topic/event;
- LiquidityAdded/graduation topic/event;
- version-detection rule;
- verified TaxToken evidence/fields;
- verified X Mode evidence/fields;
- verified AntiSniperFeeMode evidence/fields;
- canonical CREATE/BUY/SELL/liquidity fixture tx hashes;
- pending CREATE sample(s) where available;
- deterministic token-address derivation proof, or explicit UNKNOWN;
- source URL/repository, commit/version, chain, retrieval date and license for copied/derived source.

Do not assume all documented modes are active today.

Only support modes/versions proven by current Mainnet evidence.

Unknown/conflicting evidence => UNSUPPORTED.

---

## 4. Architecture boundary

Four.meme must plug into the same:

`BSC ingest -> BSC state -> shared dispatcher -> Four.meme recognizer/decoder`

Do not:

- create another BSC feed;
- create another canonical tracker;
- copy Flap decoder code and add conditionals;
- import Flap from Four.meme;
- import Four.meme from Flap;
- move venue-specific mode logic into `chains/bsc`.

Common EVM/BSC mechanics stay in chain code.

Four.meme protocol mechanics stay in its venue package.

---

## 5. Minimal target package

Likely minimal structure:

```
darkbot/venues/fourmeme/
    __init__.py
    protocol.py
    recognizer.py
    pending.py
    decoder.py
```

Add files only when concrete verified responsibility requires them.

No quote/fees/execution modules in Phase E.

---

## 6. Hot-path recognition

Cheap recognition only:

- destination contract;
- selector;
- canonical event topic;
- small frozen version lookup.

No network request, DB lookup, filesystem read, external API, metadata query or economic calculation on the recognizer/decoder hot path.

Private/high-level Four.meme APIs are not a required runtime dependency for public on-chain launch observation.

---

## 7. Pending CREATE decoding

Decode verified public calldata only.

At minimum where proven:

- source chain tx;
- sender/creator;
- manager/version;
- CREATE selector/version;
- public create arguments;
- launch native value;
- public mode/version fields relevant to factual interpretation;
- original first-seen timestamp;
- provider observation references;
- asset/token id only when deterministically proven.

Do not infer hidden/private API fields that are not present in public chain evidence.

Pending evidence remains non-canonical until chain inclusion.

---

## 8. Canonical decoding

Decode verified:

- CREATE;
- BUY / purchase;
- SELL;
- liquidity/graduation event where supported by evidence;
- factual mode/config fields emitted or derivable from verified chain evidence.

Receipt failure/revert must never be normalized as successful trade action.

Reorg/orphan/uncertain transitions propagate from chain state.

---

## 9. Mode/version model

Four.meme can have materially different modes.

Therefore:

- mode identity must be explicit;
- version identity must be explicit;
- unknown mode/version fails closed;
- no "normal mode" default when evidence is missing;
- TaxToken/X/AntiSniper labels are used only when proven by source/fixtures;
- factual mode decoding is allowed in Phase E;
- economic meaning of those modes is deferred to the Economics Gate.

Do not encode fee/tax percentages in Phase E merely because a mode name is known.

---

## 10. Normalized event mapping

Map verified events to shared types:

- CREATE;
- BUY;
- SELL;
- COMPLETE/MIGRATE only where the verified liquidity/graduation semantics justify it.

Keep venue mode/version and raw decoded fields in venue payload/evidence.

Unknown event semantics remain venue-specific UNKNOWN/UNSUPPORTED rather than forced into a generic type.

---

## 11. Storage

Same principles as Phase D:

- chain evidence stored once;
- Four.meme evidence references it;
- exact timestamps/provenance preserved;
- exact integer values;
- ABI/version/mode identity persisted;
- canonical/orphan/uncertain transitions auditable;
- additive schema only when necessary;
- no duplicate raw BSC transaction per venue.

---

## 12. Explicitly out of scope

No:

- Four.meme quote engine;
- Helper quote integration as runtime economics;
- fee/tax model;
- X Mode economics;
- AntiSniperFee economics;
- slippage model;
- approval/permit execution;
- ExecutionPlan;
- signing/broadcast;
- Strategy C multichain;
- BSC replay;
- PAPER;
- LIVE;
- Flap changes except a minimal shared-interface fix proven necessary.

Any shared-interface change must remain additive and prove Flap/Solana regressions.

---

## 13. Required deterministic tests

### Recognition
- each supported manager/version;
- valid CREATE selector;
- unrelated destination;
- unknown selector;
- unknown version.

### Pending
- valid CREATE fixture;
- malformed calldata;
- original first-seen preserved;
- provider provenance preserved;
- deterministic asset-id vector if proven;
- unknown derivation -> UNKNOWN.

### Canonical
- CREATE;
- BUY;
- SELL;
- liquidity/graduation where verified;
- revert;
- duplicate idempotency;
- orphan propagation;
- UNCERTAIN propagation.

### Modes
For every mode actually claimed supported:

- positive fixture;
- negative/neighboring fixture;
- unknown mode fails closed;
- no fee/tax value invented from mode label.

### Shared dispatcher
- same BSC transaction can be inspected by Flap and Four recognizers without duplicate chain storage;
- one recognizer cannot mutate shared evidence;
- original timestamps remain identical;
- no second network ingest.

---

## 14. Real fixture provenance

Every positive Mainnet fixture must state:

- tx/block/log identity;
- contract/version;
- source ABI/hash;
- expected decode;
- mode evidence if relevant.

Synthetic fixtures are for malformed/boundary tests only and must not establish Mainnet protocol truth.

---

## 15. Performance

Measure:

- Four recognizer time;
- Four full decode time;
- shared dispatcher with both Flap + Four enabled.

Report min/p50/p95/p99/max/sample count.

The dual-recognizer local path must remain bounded and non-blocking.

Do not add external API/RPC calls to the decoder path.

No full 3 ms LIVE claim.

---

## 16. Simplicity/code-quality gate

Must remain simple:

- one BSC ingest;
- one canonical state;
- one dispatcher;
- two isolated venue recognizers/decoders.

Reject:

- venue-specific transport stacks;
- duplicate raw storage;
- generic framework inflation;
- new legacy fallbacks;
- hardcoded secrets/private endpoints;
- virtual environment/vendor folders;
- speculative economics/execution code.

---

## 17. Full regression gate

Before:

- full suite;
- compileall;
- evidence freeze.

After:

- targeted Four tests;
- dual-dispatcher integration tests;
- all Phase-D Flap tests;
- complete suite;
- compileall;
- performance comparison;
- frozen Phase-A evidence unchanged.

---

## 18. Acceptance

Accept Phase E only when:

- every supported contract/version/mode is evidence-backed;
- pending CREATE factual decode is correct;
- canonical CREATE/BUY/SELL/liquidity decode is correct for claimed supported versions;
- unknown modes/versions fail closed;
- Flap remains unchanged except explicitly proven additive shared fixes;
- no economics or trading code has been introduced;
- shared BSC ingest remains singular;
- all tests and performance review pass.

Then:

**STOP. Do not begin Phase F automatically.**
