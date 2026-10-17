# G0.7/G0.8: Binary migration (JSON ban, roadmap e19fdde)

Status: 2026-10-17. **Implementation exists; targeted completion in progress.**

Verification baseline: `5946b4dca5aeffbb44bef91106c17c9784f66444`.

| Package | Actual status | Evidence / remaining work |
|---|---|---|
| M1 | VERIFIED | `cargo test --locked -p darkbot-bsc-record`: 6 PASS. |
| M2 | VERIFIED | `darkbot-bsc-legacy-import`: 3 PASS; `g07_legacy_import --check` reproduces the committed import byte-for-byte. |
| M3 | VERIFIED | `acceptance_guard` integration: 7 PASS, including reproduced proposed contract, four input bindings and UNARMED rejection. |
| M4 | VERIFIED | Native probe DBREC001; fixed wire fragments; typed response persistence. Execution lib 21, probe binary 28, BUY binary 22, guard 7, probe integration 5, no-send 10 PASS. |
| M5 | VERIFIED | Existing Rust preflight builds with all required feature targets; no Node preflight. Full current network readiness is separate M7. |
| M6 | VERIFIED | Mandatory durable result including missing receipts; receipt/event/transfer attribution; routed Portal events; canonical receipt-set and reorg checks. 23 focused tests PASS. |
| M7 | OPEN | Active readiness still uses v2/JSON/Node; native replacement pending. |
| M8 | OPEN | Initial feature-enabled all-target check failed in the probe; no final acceptance claimed. |

The inventory and work-package design below describe the migration baseline, not a request to reimplement completed packages. Original evidence and frozen signed transactions remain unchanged.

- Binding source: `BSC_DUAL_VENUE_ROADMAP.md`, the "BINDING DATA-PATH INVARIANT" section and "Now, G0.7/G0.8".
- Unchanged throughout:
  - the frozen signed launch/BUY bytes;
  - the DRY hashes;
  - the BUY/signing algorithms;
  - all historical JSON files (immutable legacy evidence);
  - the guard (`UNARMED`).
- Nothing is sent; verified phase completions are committed and pushed.

## 1. Inventory: internal JSON in the G0.7/G0.8 path

| # | Component | Location | Current JSON use | Class | Action |
|---|---|---|---|---|---|
| 1 | Approval contract + guard | `execution/src/acceptance_guard.rs`, `config/g01_mainnet_acceptance_proposal_v2.json`, untracked draft `docs/G07_ATTEMPT1_ACCEPTANCE_V2_DRAFT.json` | Guard parses JSON (21 fields) | internal, banned | Binary contract v3 with a new reviewed binding. The guard reads only binary. v2 becomes legacy, never authority. |
| 2 | DRY evidence | `native/evidence/g06a/g06a_final_dry_20261015T222241Z/{dry_result,preflight}.json` (sha `0efe78a7…`, `3eb5563d…`) | `load_json` in G0.7, Python in the readiness check | legacy, hash-bound | Offline import into a binary record that carries the legacy SHA-256 as provenance |
| 3 | Builder policy | `config/g06a_builder_floor_addendum_2026-10-11.json` (`ca76a93c…`) | `validate_builder_policy` | legacy | Offline import |
| 4 | Producer witness | `config/g07_producer_witness_source_attempt1.json` (`78660c7d…`), `producer_*_v1.schema.json` | `validate_producer_witness` | legacy | Offline import. The schema check becomes a typed Rust check. |
| 5 | Current preflight | `chain_reads.rs::run_current_preflight` → node `tools/g06a_preflight_readonly.cjs` → JSON in `/tmp` + stdout | internal interchange | banned | Port to Rust (decision B) |
| 6 | RPC via curl (G0.7, G0.8) | `rpc()` returns `serde_json::Value` all the way into the business logic | wire allowed, internal representation banned | partial | Decode at the boundary into typed values (quantity, data, receipt, block excerpt). No `Value` leaves the boundary function. |
| 7 | Builder requests | `club48_bundle_body` / `blockrazor_bundle_body` via `json!().to_string()` | wire, but "without general JSON serialization" | banned (generator) | Fixed byte template, byte-identical to the existing golden test |
| 8 | Builder responses | `response_body` stored raw in evidence and journal (`PROVIDER_RESULT`) | internal persistence | banned | Typed result at the boundary: HTTP status, bundle hash, error code, bounded error text, and SHA-256 + length of the raw response |
| 9 | G0.7 evidence (provisional/final) | `main.rs` `json!` + `to_string_pretty` | internal | banned | Binary record. `EvidenceFile` durability (0600, fsync, rename, dir sync) unchanged. The text view is derived. |
| 10 | Capture journal | binary DBJNL001 frames. Payloads are `key=value` text, not JSON. | – | allowed except #8 | Change only `PROVIDER_RESULT` |
| 11 | G0.8 `g08_mainnet_verify` | reads G0.7 JSON. Chain capture embeds JSON in the journal (`DBCHAIN1`). Result is JSON. | internal | banned | Reads the binary G0.7 record. Chain capture as typed binary fields (`DBCHAIN2`). Binary result plus a derived text view. |
| 12 | G0.8 `g08_producer_timing` | JSON output | internal | banned | Binary |
| 13 | Builder probe | `g07_builder_probe` writes a JSON report. The 18:34Z evidence `5636c463…` is JSON. | internal | banned | Make the writer binary; this changes the plan hash, so any new run needs a new approval. Import the 18:34Z evidence offline; the original stays untouched. |
| 14 | Readiness | `tools/g07_pre_live_readiness.sh` with Python `json.load` | internal | banned | Rust readiness check over the binary records |

**Outside the G0.7/G0.8 minimal scope (recorded as OPEN, not touched now):**

- the completed qualification binaries `g06a_native_bnb_quote`, `g06a_self_launch_bundle`, `u5_tls_qualify`;
- `qualification::json`;
- the DRY tools `g06a_*.cjs`.

These are historical, are not run again, and will be cleaned up or retired before production (I/J).

## 2. Format decision (decision A)

**A1 (recommended): new small crate `darkbot-bsc-record`, single-record file `DBREC001`.**

Layout:

- Header: magic (8 bytes), u16 format version, u16 record type, u32 schema version, u32 field count.
- Fields as TLV: u16 tag, u8 type, u32 length, then the value. Tags strictly ascending.
- Trailer: SHA-256 over all preceding bytes.

Types: `Bool`, `U64`, `U256` (32-byte BE), `Address` (20 bytes), `B256`, `Bytes`, and bounded `Text` (UTF-8, for identifiers and provider error text).

The decoder is fail-closed:

- magic, version and type must match exactly;
- tags must be ascending, with no duplicates and no unknown tags;
- every required field must be present;
- lengths must be within bounds;
- the trailer must match.

The encoding is deterministic, so the file SHA-256 is the binding hash. A `dbrec-show` tool derives the Markdown/text view. No new dependencies (`sha2` is already pinned).

**A2:** use the journal format DBJNL001 with new `RecordKind`s, one frame per record. This reuses CRC/SHA, but it changes the C0-qualified enum and mixes the capture stream with contract/config records.

## 3. Work packages (2–5 targeted checks each, no full regression until the parity proof)

1. **M1 Codec** (`crates/record`): encoder/decoder, golden vectors, fail-closed cases (truncated, wrong tag, wrong trailer, unknown tag).
2. **M2 Offline legacy import** (`g07_legacy_import`, cold path, the only audited place that still reads legacy JSON):
   - Reads #2, #3, #4 and #13. Before reading, checks the SHA-256 against the bound hashes.
   - Writes binary records with `legacy_sha256`.
   - Writes a field-parity report as a Markdown view: every field G0.7/G0.8 reads today, side by side as legacy value and binary value.
3. **M3 Contract v3 + guard:**
   - A Rust tool builds the binary contract from typed values: the reviewed v2 draft plus binary paths and hashes.
   - The guard reads only `DBREC` and stays `FROZEN_APPROVED_V3_SHA256 = "UNARMED"`.
   - The fail-closed tests carry over 1:1.
4. **M4 G0.7:**
   - Read inputs from binary records.
   - Typed RPC/builder boundary (#6–#8).
   - Builder byte templates, byte-identical to the golden test.
   - Binary evidence.
   - `PROVIDER_RESULT` typed.
   - Signed bytes and DRY hashes are proven unchanged by the existing tests.
5. **M5 Preflight port:** the read-only checks of `g06a_preflight_readonly.cjs` in Rust (eth_call/getStorageAt/getCode/getBalance plus the same limits). Field parity against the imported baseline.
6. **M6 G0.8:** both binaries read and write binary; `DBCHAIN2`; `g08_reads_g07_evidence` test against the binary record.
7. **M7 Readiness:** Rust check replaces the Python JSON parsing.
8. **M8 Parity proof + retirement:**
   - Prove the five properties the roadmap requires (byte/field parity, deterministic decoding, signed-TX references, crash recovery, G0.8 compatibility).
   - Only then remove the active JSON readers/writers from G0.7/G0.8/guard/probe.
   - One full regression run.
   - Update the ledger and STATUS.

## 4. Mapping to the roadmap's required proofs

| Roadmap requirement | Proof |
|---|---|
| Byte/field equivalence | M2 parity report + M4 golden template + unchanged signed-TX tests |
| Guard fail-closed | M3: the existing guard test set against DBREC, plus manipulated-record cases |
| Durable recovery | `EvidenceFile`/latch tests unchanged on binary evidence; crash-in-the-middle case |
| Test parity | Every existing G0.7/G0.8 test keeps passing with binary inputs (renamed only where the file extension appears) |
| G0.8 compatibility | `g08_reads_g07_evidence` on the binary record (the same 12 fields) |

## 5. After the migration (unchanged order)

1. Economic approval of contract v3 (new binding, `attempts.maximum` 1, budget unchanged).
2. Builder refresh ≤ 24 h before arming.
3. Arming commit by the user.
4. Exactly one run.
5. G0.8.
6. Disarm.

### M4/M5 targeted completion — 2026-10-17

Feature-enabled workspace/all-target `cargo check --locked` PASS. Offline targeted checks: 93 PASS total (counts above); loopback TLS test required sandbox exemption, no external builder probe. Stale JSON source assertions were updated to their binary equivalents. G0.7 encode failure now preserves previous durable binary evidence. `transactions.rs`, frozen inputs and historical probe evidence are unchanged. Probe dry-plan hash is now `daef6789e4b02307e1e7d6ab706f0316142025c6222e0bc0a2d92e2243f59808`; the old approval cannot authorize this changed generator. No new probe requested or performed. M6–M8 remain open.

### M6 targeted completion — 2026-10-17

Code baseline after M4/M5: `432218d`. G0.8 result schema v4 additionally records receipt-attributable net token acquisition and the fee already included in BUY value. The frozen official IPortal TokenBought ABI is reused. Canonical receipt success, sender/recipient, token, event amount/value and net Transfer acquisition must agree; balance alone is insufficient. Costs use actual transaction value plus effective receipt gas, with no second addition of the event fee. All intervening canonical receipts are scanned for Portal events, including router-internal buys, with exact block/transaction/receipt membership and unique log indices. Capture checks parent continuity, the launch block binding and every captured block hash again after closure. Whole-second timestamp uncertainty conservatively adds one second at the trailing edge and another boundary block; no producer-time claim.

`DARKBOT_G08_EVIDENCE_PATH` is mandatory; durable UNRESOLVED is written before RPC, so interrupted/failed reconciliation cannot lose the attempt. Missing receipts persist UNRESOLVED; reorgs cannot PASS. Encoding/write/fsync errors cannot produce PASS. Qualification tests: verifier 12, timing 5, binary compatibility/negative witness 4, G0.7-reader compatibility 2: **23 PASS**. Synthetic cases establish offline failure behavior only. Original Mainnet producer witness, actual acquisition and live canonical capture remain NOT_EXECUTED.
