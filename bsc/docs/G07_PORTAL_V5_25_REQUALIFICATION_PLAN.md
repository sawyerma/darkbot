# G0.7 attempt 1: requalification against Flap Portal v5.25.0

Status: 2026-10-17. **Read-only analysis complete; sections 3 and 4 implemented (option A) and tested offline; the requalification DRY run (DRY-1 signing) is pending with the user.**

- Nothing was sent. No signature was made, no builder was contacted, and the guard stays `UNARMED`.
- The DRY salt was not disclosed: every on-chain probe used a random salt and dummy strings of equal length.
- Trigger: the read-only readiness of 2026-10-17 (`native/evidence/g07/readiness_20261017T101215Z/`) → `CURRENT_DEPLOYMENT_DRIFT = STOP`.

## 1. Findings (read-only analysis)

Method: public BSC RPC `bsc-dataseed.bnbchain.org`, heads ~128 153 600. JSON-RPC wire only.

| Topic | Result |
|---|---|
| Proxy `0xe2ce…9de0` | Code unchanged (`1fc365bf…`, 2 882 bytes). |
| Implementation | `0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20` (v5.24.0, `2ac893fc…`, 20 475 bytes, solc 0.8.26 with metadata) → `0xe13227370996f892a56d2ff4a30f6026887b48fb` (v5.25.0, `42c91172…`, 20 347 bytes, built without CBOR metadata). |
| Upgrade block / transaction | Determined in R0 (§8): block 128 103 397 (`0x8d0c79ab…badb`), transaction `0x5a4faae5…3d4a`, log index 235, receipt status 1. At the time of this analysis public RPCs refused `eth_getLogs` over the range and BscScan blocked automated access. |
| Selectors (dispatcher) | 100 → 99. All six G0.7 functions unchanged: `newTokenV6` `8cb5772c`, `swapExactInput` `ef7ec2e7`, `getQuoteTokenConfiguration` `26ef20d5`, `getFeeRate` `84e5eed0`, `enableTaxOnBondingCurve` `1d31e153`, `version` `54fd4d50`. Removed: `recoverStuckTaxProcessorToTaxToken(address,address)` `5fbbeee7`, `auditorRunIdempotent()` `b2299337`. Added: `40c9450e` (unknown). |
| Bytecode | Opcode similarity 0.983 (70 changed runs, 316 opcodes). The version string and nine embedded addresses (immutables) changed. All nine are redeployed helper contracts: one is code-identical, the others differ slightly. The TAX_V3 implementation `0x024f18294970b5c76c0691b87f138a0317156422` is used unchanged. |
| Read state | Native quote configuration, protocol fees, tax flag, proxy code and TAX_V3 implementation are equal to the DRY baseline (readiness drift comparison). |
| Token address derivation | `eth_call newTokenV6` (random salt) returns exactly the offline-predicted CREATE2 address (portal + TAX_V3 clone). The vanity rule is unchanged (`VanityAddressRequirementNotMet(address)` without the `…7777` suffix). The DRY token `0xe51c4deb1038afcd95ef12d4a649cdbcf6fa7777` is still free (no code). |
| Launch gas | `eth_estimateGas` 5 496 441 (DRY estimate 5 502 725). Signed limit 6 603 270 (+20 %) remains sufficient. |
| Launch→BUY economics | `eth_simulateV1`, sequential, current state, random salt, BUY 1 000 000 000 000 wei, `minOutput` 1. Launch status 1, gas 5 244 088. BUY status 1, gas 391 139, output **176693134169499757962**, identical to the DRY quote and the DRY simulation. |
| BUY gas | 391 139 here versus 334 654 in the DRY simulation (+17 %; different simulator and accounting, plus a possible effect of the new implementation). Signed limit 3 000 000 remains far above it. The budget uses limits, not consumption, and is unchanged. |
| Wallets / gas price | Nonces 0/0; launcher and buyer 0.005 BNB each; `eth_gasPrice` 50 000 000 wei = signed gas price. |

**Interpretation:** every input to the signed bytes is unchanged:

- nonce, gas price, gas limits, destination, values;
- launch calldata (salt);
- BUY calldata (token, `minOutput` = 99 % of the unchanged expected output).

The newly computed bytes are therefore expected to stay identical to the DRY hashes:

- launch `0xdb468c01…5146`
- BUY `0xb086f683…db32`

What changed is the code behind the proxy. The DRY-2 simulation and the preflight baseline (implementation and code hash) are bound to v5.24.0. Under the scope rule ("mismatch is STOP") the attempt may only be armed after DRY-1 to DRY-4 are re-run against v5.25.0.

This analysis does not prove that the exact DRY bytes (with the real salt) execute on v5.25.0. That is the task of DRY-2, run locally without disclosing the salt.

## 2. Requalification scope (DRY-1 to DRY-4, binary output)

Basis: the frozen candidate unchanged — same wallets, salt, values, gas, `minOutput` policy (100 bps), single attempt. No new economic commitment, as long as the bytes come out identical.

| Step | What runs | Binary output (DBREC001, no JSON file) | Acceptance |
|---|---|---|---|
| R0 | Upgrade evidence without `eth_getLogs` (§8): every header since the DRY head hash-verified and parent-linked, logs blooms, receipts of every bloom candidate verified against `receiptsRoot`; both chain ends anchored on an archive endpoint, implementation slot before/after each event (read-only, free public endpoints). | `portal_upgrade_scan.dbrec` (R0) and `portal_upgrade.dbrec` (closed at the DRY-0 block), type 24 v2. | Exactly one upgrade v5.24.0→v5.25.0 since the DRY head; coverage complete from the frozen block hash to the preflight block hash. |
| DRY-0 baseline | Current read-only preflight through the existing Rust preflight (`run_current_preflight`). | `preflight.dbrec` (preflight schema, native, without `_import.*`). | Equal to the v5.24.0 baseline except the three drift fields; nonces 0/0; funding PASS. |
| DRY-1 bytes and signers | `g06a_self_launch_bundle`: sign locally with the key environment variables (subshell, `env -i`, no output of secret values). | `dry_result.dbrec` (native DRY schema). | Launch/BUY hashes equal `0xdb468c01…` / `0xb086f683…`; signers equal the wallets. |
| DRY-2 independent simulation | The existing EthereumJS simulation (`g06a_local_simulate`, `g06a_economics_evm_crosscheck`) against current state, with the exact signed bytes, locally (state via RPC reads, calldata never on the network). | `local_simulation.dbrec`, `economics_crosscheck.dbrec`. | Launch and BUY successful; output = `176693134169499757962`; `output_difference = 0`; token = `0xe51c…7777`. |
| DRY-3 gas floor and budget | Builder requirements re-determined from the official documentation pages (`g06a_builder_policy_record`, read-only) plus floor/budget comparison. | `builder_policy.dbrec` (native, type 22 v2, with each fetched source: URL, fetch time, HTTP status, body SHA-256, finding). | Floor read from BlockRazor's rule sentence = 50 000 000 wei; no explicit 48Club minimum; budget unchanged (481 163 500 000 000 wei). Any other finding is a STOP. |
| DRY-4 expiry, nonces, zero egress | Existing structural zero-egress proof and expiry projection. | Fields in `dry_result.dbrec`. | Zero egress; nonces 0/0. |
| Comparison | Field-by-field reports new DRY ↔ frozen DRY (`dbrec_compare`, derived Markdown) and the typed verdict `g06a_requal_check` (offline). | `requalification_status.dbrec` (type 26) plus derived views. | Every field equal except an explicit allow-list per record (head, block hash, implementation and code, gas price, balances, measured gas, projection, contract path/hash, builder evidence hash); signed hashes, token, salt, `minOutput`, economics and safety parameters identical. Anything else is `FLAP_REQUALIFICATION=STOP`. |

## 3. Binary output of the DRY toolchain (gap and decision)

The DRY tools still write JSON today. They are outside the minimal G0.7/G0.8 migration scope, but a new DRY run needs them without internal JSON:

- `qualification/src/bin/g06a_self_launch_bundle.rs` and `g06a_native_bnb_quote.rs` use `serde_json`.
- `tools/g06a_local_simulate.cjs` and `tools/g06a_economics_evm_crosscheck.cjs` write JSON.
- `tools/g06a_final_dry.sh` chains the old node preflight and the JSON files.

Planned changes:

1. **Rust parts:** both qualification binaries write native DBREC records instead of JSON.
   - New native schemas in `record/src/schema/g06a.rs`: the same fields as the import schemas, without `_import.*`, plus provenance (tool, source digest, observation time, head).
   - The preflight comes from the existing Rust preflight, so there is no second implementation.
2. **Independent EVM simulation (DRY-2): decision required.**
   - **Option A (recommendation):** keep the qualified EthereumJS simulation unchanged and replace only its output with a small, fixed DBREC encoder in JS.
     - The Rust decoder stays the sole authority: the output is decoded fail-closed.
     - A golden test pins identical bytes for identical values.
     - No new Rust dependency; the simulation logic stays untouched.
   - **Option B:** port the independent simulation to Rust (`revm` as a new pinned dependency).
     - It is just as independent, but means a larger dependency surface and a new implementation of qualified logic.
3. **`g06a_final_dry.sh`:**
   - chains the binary steps;
   - the old node preflight is no longer called;
   - the old JSON chain is kept only as a historical script.

## 4. Contract, guard and G0.7 consequences

- **Native inputs:** the v3 contract binds four import records, each carrying a legacy SHA-256. A new DRY produces native records with no legacy file.
- **v4 contract:**
  - binds the record hashes of the new native records;
  - binds the superseded v3 proposal as a reference;
  - legacy bindings become optional for native inputs.
- **Guard:** reads v4; `InputHashes.legacy_sha256` is optional for native inputs. Fail-closed tests carry over 1:1.
- **New approval:** a changed contract needs its own new, reviewed binding. The user's economic approval later refers to the v4 hash.
- **G0.7:**
  - `load_record` accepts native records (provenance instead of the import hash);
  - all checks remain (DRY hashes, preflight drift against the **new** baseline, guard).
  - `transactions.rs` and the release path stay unchanged.
- **Readiness:** binds v4 and reports, read-only against the new baseline:
  - deployment;
  - nonces, funding;
  - gas;
  - head;
  - builder freshness.
- **Builder probe:** not part of the requalification. The builder interfaces did not change because of the Portal upgrade. The dry-plan hash is `8e24138b…`; there is no approval and none is requested.

## 5. Order and approval points

1. Decide the DRY-2 output (option A or B). This is the only architecture decision.
2. Implement section 3 and section 4 with targeted tests (codec/schema, golden, guard v4, G0.7 no-send, readiness). No full regression until acceptance.
3. Read-only R0 and DRY-0 (operator RPC; no secret values in output).
4. **User:** start DRY-1 locally (key environment variables in a subshell). This step only signs locally and sends nothing.
5. DRY-2 to DRY-4 locally/read-only; DRY comparison report; evidence under `native/evidence/g06a/g06a_final_dry_<UTC>/` (DBREC + derived views + `SHA256SUMS`).
6. Write the v4 proposal (`approved=false`) and show the approved variant's hash.
7. Builder refresh ≤ 24 h, then readiness `--current-rpc`.
8. **Stop:** economic approval of the exact v4 hash by the user, then a separate arming commit by the user, then exactly one G0.7 run, then G0.8.

**What triggers a new economic decision:** if DRY-1 does not produce identical hashes, or DRY-2 deviates from the values above (output, success, token), the candidate changes. That means a new approval of the changed bytes/values, never an automatic adjustment.

## 6. Not claimed

- No inclusion, builder semantics or T0→BUY timing.
- The eth_call/eth_simulateV1 results are read-only analysis with a random salt and dummy strings. They are not DRY evidence and not a Mainnet BUY.
- The +17 % BUY gas is a simulator comparison, not a confirmed cost increase.

## 7. Implementation status (2026-10-17, option A)

Nothing was sent or signed, and the guard stays `UNARMED` (`FROZEN_APPROVED_V4_SHA256 = "UNARMED"`).

| Part | Implementation |
|---|---|
| Native DRY schemas | `record/src/schema/g06a.rs`: native variants with provenance (`_provenance.tool`, `_provenance.observed_unix_ms`) for DRY result (type 19), local simulation (20), economics cross-check (21) and builder policy (22). Each shares its field list with the import variant through a macro; the import variants are byte-identical (`g07_legacy_import --check`). New: native quote (23), Portal upgrade scan R0 (24). |
| JS DBREC codec | `tools/dbrec.cjs`: canonical encoder and strict decoder. Schema tables are generated from the Rust schemas (`tools/dbrec_schemas.generated.cjs`, `dbrec_js_schemas`), and the record crate test fails if they are stale. |
| EthereumJS tools | `g06a_economics_evm_crosscheck.cjs` reads `preflight.dbrec` and writes `economics_crosscheck.dbrec`. `g06a_local_simulate.cjs` writes `local_simulation.dbrec`. The simulation logic is unchanged, and JSON appears only in `rpc()` (wire). |
| Rust DRY binaries | `g06a_native_bnb_quote` (preflight record → quote record). `g06a_self_launch_bundle` reads the simulation record and writes the native DRY record; its bundle projection uses G0.7's fixed wire templates (golden test). |
| DRY-0/R0 | `g06a_preflight_record`: current preflight through the existing Rust preflight; R0 coverage in its own read-only module (`g06a_preflight_record/coverage.rs`: header hash = keccak256(rlp(header)), parent links, bloom test, receipts trie root, typed JSON-RPC wire boundary). `chain_reads.rs` of G0.7 is byte-identical to `c57262c`. |
| Script | `tools/g06a_final_dry.sh` is binary-only: R0 coverage, DRY-0, quote, cross-check and simulation (state of the preflight block via the archive endpoint), builder re-determination, DRY-1..4, comparison reports (`dbrec_compare` exit 2 = STOP) and the typed verdict `g06a_requal_check` (no `|| true` on any verdict). The old JSON chain is kept as `g06a_final_dry_legacy_v2.sh`. |
| Builder refresh | `g06a_builder_policy_record <frozen_policy> <out>`: fetches the official pages, reads the floor from BlockRazor's rule sentence, checks the 48Club pages for any documented requirement, STOP on any change; nothing recalculated. `dbrec_derive` (free editing of `observed_date`) is retired. Readiness `BUILDER_FRESHNESS` uses the fetch time of the oldest source; an import is always `BLOCKED`. |
| Contract v4 | `record/src/schema/g07.rs` (`contract::V4`, type 25; field list shared with v3, v3 byte-identical). It binds the four G0.7 inputs (legacy hash only for imports), the local simulation, the economics cross-check, the R0 scan and the superseded v3 proposal. Generator: `g07_acceptance_contract <requal_dir> <builder_policy> [--check \| --approved-sha \| --write-approved]`; `--check-v3` reproduces v3. |
| Guard | Only `V4_RELATIVE_PATH` authorizes. `UNARMED` is refused before any read. Legacy hashes are optional per input; a mismatch between import and native is refused. |
| G0.7 | Native DRY and preflight records (`d::NATIVE`); builder policy native or import; witness source import. Evidence schema v4 (legacy hashes optional). `transactions.rs` is byte-identical to `565d811`; `builder_submission.rs` is unchanged. |
| Readiness | Binds v4 and checks the four inputs plus local simulation, economics cross-check and R0 scan (legacy hashes where an input is an import). Without v4: `V4_CONTRACT BLOCKED`. |

**Tests (targeted, offline):**
- `cargo check --locked --workspace --all-targets --features darkbot-bsc-execution/g07-mainnet`: PASS with 0 warnings.
- 179 tests PASS:
  - execution lib 22, preflight writer 19, probe binary 28, BUY binary 22, readiness binary 21, guard 8, probe integration 5, no-send 10, readiness integration 5;
  - DRY bundle 1, G0.8 verifier 12, timing 5, G0.8 binary 4, G0.7→G0.8 2, JS↔Rust parity 3;
  - codec 7, `dbrec_derive` 2 (retired since, §8), legacy import 3.
- Stand-in records derived from the frozen data test only the v4 wiring. They are not requalification evidence.
- Probe dry-plan hash `a5ad014f9ffebd32b6c2cc7fc36fa054d9c7118fdb1e76b8f3c5ae1606fbd039` (unchanged by §8: the shared G0.7 modules are byte-identical to `c57262c`). No probe approval exists and none is requested.

The run instructions of this section are superseded by §8.
