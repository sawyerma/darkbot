# DarkBot — Phase D / M7 Flap.sh Read-Only Decoder Contract

Status: **FUTURE CONTRACT / BLOCKED UNTIL PHASE C IS ACCEPTED**  
Milestone: **M7**  
Scope: **Flap.sh read-only recognition and decoding only**  
End rule: **STOP after Phase D review; do not begin Phase E automatically**

---

## 1. Purpose

Phase D adds Flap.sh as a read-only venue decoder on top of the already accepted shared BSC/EVM capture infrastructure.

The goal is to turn verified Flap pending/canonical chain evidence into lossless venue-specific decoded evidence while preserving:

- the original BSC receive timestamps;
- the underlying chain transaction/log/receipt identity;
- provider provenance;
- canonical/orphan/uncertain state;
- raw evidence integrity.

Phase D does not trade and does not model profitability.

---

## 2. Preconditions

Phase D must not start until:

1. Phase C is independently reviewed and accepted with no open Phase-C blocker;
2. restart/backfill, canonical durability/high-water, reorg uncertainty, earliest multi-provider first-seen, bounded-state loss reporting and RPC provenance are proven correct;
3. the complete repository test suite is green;
4. `docs/phase_a/**` remains frozen;
5. Flap Mainnet BSC evidence required by §3 is collected and frozen.

If any precondition is false:

**STOP.**

---

## 3. Mandatory Flap evidence freeze before coding protocol constants

No Flap address, selector, topic, ABI field, mode, version or derivation rule may be invented.

Before a production Flap constant is added, freeze an evidence manifest containing, where applicable:

- BSC chain id;
- current supported Portal/deployment contract address(es);
- contract version/implementation relationship;
- ABI source and ABI hash;
- deployed bytecode/code hash where useful for version identity;
- verified CREATE/launch selector(s);
- verified BUY selector(s);
- verified SELL selector(s);
- verified migration/graduation event/topic(s);
- verified CREATE/BUY/SELL canonical transaction hashes used as fixtures;
- verified pending launch transaction sample(s) where available;
- evidence for deterministic token-address derivation, or explicit statement that it is not proven;
- evidence for any mode bits/fields that must be decoded;
- source provenance: URL/repository, commit/version, chain, retrieval date and license where source code is used.

Preferred evidence order:

1. verified deployed contract/source/ABI;
2. official Flap-maintained sources;
3. direct BSC transaction/log evidence;
4. independent public reference implementations only as cross-checks.

A frontend/API response alone is not sufficient proof of on-chain semantics.

Unknown or conflicting evidence => **UNSUPPORTED / STOP**, never guess.

---

## 4. Architecture boundary

Phase D must reuse:

`BSC ingest -> BSC canonical/pending state -> shared dispatcher -> Flap recognizer/decoder`

Do not add another BSC WebSocket/RPC stack for Flap.

Do not add another canonical tracker.

Do not add another persistence architecture.

Do not create a shared `bsc_launchpad.py` with Flap/Four conditionals.

Flap-specific code belongs under the Flap venue package only.

No Flap module may own BSC reconnect/reorg/provider mechanics.

---

## 5. Minimal target package

Use only files that have concrete responsibility. A likely shape is:

```
darkbot/venues/flap/
    __init__.py
    protocol.py
    recognizer.py
    pending.py
    decoder.py
```

Additional files are allowed only when actual verified complexity requires them.

Do not create empty future files for quote, fees, execution or migration economics in Phase D.

---

## 6. Hot-path recognizer

The Flap recognizer must be cheap and deterministic.

Allowed hot-path checks:

1. destination contract/address;
2. 4-byte selector;
3. event topic for canonical logs;
4. small fixed/version lookup from frozen in-memory data.

Forbidden on recognizer hot path:

- HTTP/RPC requests;
- database queries;
- filesystem reads;
- ABI downloads;
- DNS;
- JSON reserialization for persistence;
- expensive metadata lookup;
- Strategy logic;
- quote/economic calculations.

Only matched evidence proceeds to full Flap decoding.

---

## 7. Pending CREATE decoding

Decode only fields proven by the frozen ABI/fixtures.

At minimum, where present and verified:

- source chain tx id;
- creator/sender;
- target Portal/version;
- launch selector/version;
- calldata launch parameters;
- initial native value;
- explicit mode/config fields;
- original `first_seen_mono_ns`;
- provider observation references;
- predicted asset/token address only when deterministic derivation is proven.

If token address is not provably derivable before inclusion:

`predicted_asset_id = None / UNKNOWN`

Do not guess.

A decoded pending launch remains pending evidence, not canonical existence.

---

## 8. Canonical decoding

Decode verified Flap canonical evidence for:

- CREATE / launch;
- BUY;
- SELL;
- migration/graduation where directly evidenced;
- configuration/mode events only where needed for correct later interpretation.

Canonical venue evidence must reference the underlying BSC transaction/log/receipt records instead of duplicating raw chain evidence.

Reorg/orphan/uncertain transitions from Phase C must propagate to decoded venue evidence.

A Flap event from an orphaned block must not remain canonically active.

---

## 9. Normalized event mapping

Map verified Flap events into the existing shared normalized vocabulary without changing Strategy C:

- CREATE
- BUY
- SELL
- COMPLETE/MIGRATE only when the verified protocol semantics justify that mapping.

Preserve venue-specific decoded payload separately.

Never force an uncertain Flap event into a generic event type merely to fit the model.

Unknown semantics remain UNKNOWN/UNSUPPORTED.

---

## 10. Version and mode handling

Every supported Flap version/mode must be explicit.

Rules:

- version identity comes from frozen evidence;
- unknown address/version/implementation => fail closed;
- unknown selector => not decoded as a known Flap event;
- unknown mode that can affect interpretation => decoded evidence marked UNSUPPORTED/UNKNOWN;
- never silently fall back to a "default" version.

No web frontend may silently change the active decoder version.

---

## 11. Storage

Use additive storage only if the existing Phase-C venue mapping/evidence tables cannot represent the required decoded fields.

Requirements:

- one chain evidence record remains the source;
- Flap rows reference chain evidence;
- original receive timestamp is not regenerated;
- exact integer/base-unit values only;
- raw calldata/log data remains lossless or integrity-addressable;
- version/ABI identity is persisted;
- canonical/orphan/uncertain state remains auditable;
- schema remains readable for existing Solana/BSC data.

Do not duplicate an entire BSC transaction per Flap event.

---

## 12. Explicitly out of scope

Phase D must not implement:

- Flap quote engine;
- bonding-curve economics;
- protocol fee model;
- buy/sell tax model;
- slippage model;
- `eth_call` quote parity;
- approval/permit economics;
- transaction construction;
- ExecutionPlan;
- signing;
- broadcast;
- Strategy C multichain;
- BSC replay;
- PAPER;
- LIVE;
- Four.meme decoding.

If any of those appears necessary to decode read-only evidence, separate the minimum factual decode field from the future economic interpretation.

---

## 13. Required deterministic tests

At minimum:

### Recognition
- known supported Portal/address + CREATE selector matches;
- unrelated address does not match;
- known address + unrelated selector does not falsely match;
- unknown version/address fails closed.

### Pending
- valid pending CREATE fixture decodes exactly;
- malformed/truncated calldata rejected;
- original first-seen timestamp preserved exactly;
- provider provenance preserved;
- deterministic asset id derived only for a proven vector;
- unproven derivation returns UNKNOWN rather than a guess.

### Canonical
- CREATE fixture;
- BUY fixture;
- SELL fixture;
- migration/graduation fixture if supported by evidence;
- receipt revert does not become successful canonical venue action;
- orphaned event becomes non-canonical;
- UNCERTAIN chain state does not become definitive venue state;
- duplicate canonical evidence is idempotent.

### Versioning
- every supported ABI/version fixture;
- wrong version rejected;
- unknown selector/topic rejected;
- version identity persisted.

### Dispatcher integration
- shared BSC dispatcher sends matching evidence to Flap recognizer;
- Four/synthetic unrelated recognizer does not alter Flap evidence;
- no new BSC connection is created;
- no duplicate raw chain evidence;
- original chain timestamps unchanged.

---

## 14. Fixture quality

Fixtures must be derived from real verified BSC evidence or verified ABI vectors.

For each fixture preserve enough provenance to reproduce why it is trusted:

- tx hash/block;
- contract/version;
- ABI/source identity;
- expected decoded fields;
- raw calldata/log payload.

Do not use invented mainnet constants in a fixture.

Synthetic malformed/boundary fixtures are allowed only for negative tests and must be labeled synthetic.

---

## 15. Performance

Measure Flap recognizer + decoder overhead independently.

Report:

- sample count;
- min;
- p50;
- p95;
- p99;
- max.

Compare dispatcher-only Phase-C baseline with dispatcher+Flap.

No HTTP/RPC/DB work is allowed in the local decode hot path.

Any reproducible regression must be explained and corrected if caused by unnecessary work.

Do not claim the later 3 ms LIVE socket-handoff contract is proven.

---

## 16. Simplicity and code-quality gate

Phase D must remain the smallest implementation that correctly decodes the verified protocol.

Reject:

- new generic framework layers;
- second dispatcher;
- second BSC state machine;
- copied EVM transport;
- new legacy/fallback path;
- hardcoded private provider URLs;
- credentials/secrets;
- vendored virtual environments/dependencies;
- speculative files/classes for future economics;
- Flap/Four shared conditionals.

Necessary verified protocol constants are allowed and must be evidence-backed/versioned.

---

## 17. Full regression gate

Before edits:

- complete suite;
- compileall;
- evidence manifest frozen.

After edits:

- targeted Phase-D tests;
- dispatcher integration tests;
- complete suite;
- compileall;
- relevant performance comparison;
- verify frozen Phase-A evidence unchanged.

Existing Solana/Pump semantics must remain unchanged.

---

## 18. Acceptance

Phase D is accepted only when:

- all supported Flap constants are evidence-backed;
- pending CREATE is decoded correctly;
- canonical CREATE/BUY/SELL are decoded correctly;
- migration decoding is correct if claimed supported;
- unknown versions/modes fail closed;
- chain timestamp/provenance/canonicality are preserved;
- no economics/trading code exists;
- all tests are green;
- no unexplained performance regression exists.

Then:

**STOP. Do not begin Phase E automatically.**
