# DarkBot BSC Dual-Venue — Phase A Report

Implementation contract: `docs/BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md` §22/§27/§28 plus the Phase-A go-ahead
(synthetic regression parity allowed as an additional test; clean-v2 stays the binding production oracle).
All evidence files referenced below live in `docs/phase_a/` (checksums in `SHA256SUMS.txt`).
One-off runners were executed from `/tmp/phase_a/` (outside the production source tree); byte-identical
copies with recorded SHA-256 are in `docs/phase_a/tools/`.

```
PHASE:                  A — architecture + minimal Solana/Pump wrapper parity (no Phase B started)

BASELINE_COMMIT:        05971e4ed35a8a98349e298d2de4e61f8584fc5b
                        (GitHub archive comment of darkbot-main__7_.zip; workspace has no .git)
SOURCE_MANIFEST_SHA256: pre-change  0594c977b4433f986b16c2a21da824c3e7a97017c7ed3c341c0e1ba58c8f4a3c  (73 files, source_manifest_pre.txt)
                        post-change 7eb2117c196faf8df52d267b7ddd42cc3d3ace1e654bf619bdf67465e712b9d7  (93 files, source_manifest_post.txt; after review fix R1)
ENVIRONMENT:            Python 3.12.3, Linux 6.18.44 x86_64, Intel Xeon @ 2.10GHz, 1 vCPU, no network, websockets/pytest not installed
```

## FILES ADDED

```
darkbot/chains/__init__.py
darkbot/chains/base.py                 ChainIdentity + ChainAdapter Protocol (verbatim §4.1)
darkbot/chains/registry.py             chain_identity()/load_chain(); solana-mainnet executable, bsc-mainnet metadata -> ChainNotImplemented(NotImplementedError)
darkbot/chains/solana/__init__.py
darkbot/chains/solana/chain.py         SolanaChainAdapter: delegates to capture.FeedAdapter / fees.FeeResolver / config.resolve_* (feed/fee wiring moved from main.py, unchanged)
darkbot/venues/__init__.py
darkbot/venues/base.py                 VenueIdentity + VenueAdapter Protocol (verbatim §4.2)
darkbot/venues/registry.py             venue_identity()/load_venue(); pumpfun executable, flap/fourmeme metadata -> VenueNotImplemented(NotImplementedError)
darkbot/venues/pumpfun/__init__.py
darkbot/venues/pumpfun/venue.py        PumpFunVenueAdapter: delegates to PumpDecoder / MarketReducer / QuoteEngine (same constructor arguments StateOwner used)
darkbot/runtime/__init__.py
darkbot/runtime/profiles.py            MarketProfile, MARKET_PROFILES (solana READY, bsc-mainnet:flap / bsc-mainnet:fourmeme RESEARCH), load_market_profile()
darkbot/runtime/assembly.py            RuntimeCallbacks, RuntimeComponents, RuntimePreflight, preflight_market_profile(), assemble_runtime(), build_feeds();
                                       replay path deliberately NOT routed through it (§5.1)
tests/test_chain_registry.py           4 tests
tests/test_venue_registry.py           5 tests
tests/test_runtime_assembly.py         7 tests
tests/test_market_profiles.py          8 tests
tests/test_multichain_domain.py        3 tests
tests/test_bsc_profile_fail_fast.py    7 tests (review fix R1: run_capture / run_paper / run_doctor_cli / Application with a BSC profile -> MarketProfileNotRunnable,
                                       no DB file, no runs row, no RUNNING artefact, no sqlite-writer thread; assembly precedes open_storage)
tests/fixtures/config_hashes_baseline_05971e4.json   all 9 runtime-config snapshot hashes frozen BEFORE the change
docs/phase_a/**                         evidence (this report, manifests, benchmark results, diffs, runners)
data/phase_a/synthetic_capture_seed20260924.sqlite   frozen synthetic source recording (58 MB, gitignored *.sqlite; SHA-256 below)
```

## FILES MODIFIED

```
darkbot/config.py   +23 lines: optional explicit top-level `market_profile_id` (schema key added ONLY when present, validated against
                    MARKET_PROFILES); RuntimeConfig.market_profile_id / has_explicit_market_profile resolve the legacy default
                    OUTSIDE raw/canonical_json. No setdefault into raw.
darkbot/paper.py    +17/-5 lines in StateOwner.__init__ only: additive keyword params `decoder=None, reducer=None, market_profile=None`;
                    existing `engine=` injection untouched; legacy fallback construction byte-for-byte the previous code, now guarded
                    so a non-Solana profile cannot silently fall back to Pump decoding (NotImplementedError). No hot-path change.
darkbot/main.py     Application.__init__: preflight_market_profile(cfg) as the FIRST statement (review fix R1) -> a metadata-only/unknown/
                    incompatible profile is rejected before any run or storage side effect. Application.run: assemble_runtime() runs
                    BEFORE open_storage() (no migration, no runs INSERT, no writer start for a rejected profile); FeeResolver/FeedAdapter/
                    decoder/reducer/engine come from the components (same objects, same arguments; wiring order resolver -> StateOwner ->
                    gate clock -> feeds unchanged); feeds are built after the StateOwner exists and receive its bound methods directly
                    (no lambda/task hop). run_doctor: preflight before the doctor database is touched. Summary gains a `market_profile` block.
```
Full unified diff: `phase_a.diff` (1389 lines; 3 production files modified, 13 new modules, 7 new test files/fixtures, 0 files removed).

### Review fix R1 (fail fast for non-runnable profiles)
Finding (reviewer): `Application.run()` executed `open_storage()` (migrations, `runs` INSERT with status RUNNING, writer start) before
`assemble_runtime()` raised `MarketProfileNotRunnable`, leaving a RUNNING run row behind for `bsc-mainnet:flap` / `bsc-mainnet:fourmeme`.
Fix: order is now profile -> runnable -> chain -> venue -> compatibility (`preflight_market_profile`, no side effects) in
`Application.__init__` and `run_doctor`; in `Application.run()` the full component assembly precedes `open_storage()`.
Verified by `tests/test_bsc_profile_fail_fast.py` at the real entry points (`run_capture`, `run_paper`, `run_doctor_cli`, `Application(...)`):
exception raised, database file never created, pre-existing campaign database gets no new `runs` row and no RUNNING row, no
`sqlite-writer` thread alive; positive control: Solana `Application(...)` still constructs without side effects.

## SEMANTIC CHANGES

```
TRADING / REPLAY SEMANTICS: NONE for Solana/Pump/PAPER/CAPTURE/REPLAY/Strategy A/B/C:
- LEGACY_BACKEND_FILTER_V1, Pump decoding, Solana fee accounting, replay timing, PAPER boundary: untouched (market.py, execution.py,
  fees.py, replay.py, strategies/*, storage.py, domain.py unchanged — see source_manifest_post.txt).
- Hot path (StateOwner.on_raw -> reducer -> strategy -> decision -> adapter.submit): unchanged code; the only new attribute
  (self.market_profile) is never read per message. No registry/profile lookup per message; no new queue/task/process/serialization hop.
- Replay still constructs StateOwner(engine=QuoteEngine(stress...)) with canonical_prefix, persist_observations, compute_features,
  mint_allowlist, lateness_provider, execution_delay_provider, virtual clock and dataset integrity checks exactly as before
  (guarded by tests.test_runtime_assembly.test_replay_keeps_its_own_engine_injection_and_stays_offline). Replay needs no feed/RPC.
- No storage migration, no column rename, no domain dataclass change (generic identity mapping asset_id=mint, tx_id=signature,
  block_ref=slot, native_amount_base_units=sol_amount is exposed through the adapters; additive DB columns deferred to M3).
ADDITIVE OUTPUT CHANGE:
- `market_profile` metadata block ({profile_id, chain_id, venue_id, display_name, explicit_in_config}) added to the Application run
  summary (stdout/JSON of paper/capture/doctor); no database row or report field changed.
- Application exposes `preflight` and `components` attributes (metadata for callers/tests).
NEW explicit failures (intended):
- Application(...)/run_paper/run_capture/run_doctor/assemble_runtime/StateOwner fallback for market_profile_id != solana-mainnet:pumpfun
  -> NotImplementedError (MarketProfileNotRunnable), raised BEFORE any storage side effect (review fix R1).
- Unknown market_profile_id in a config -> ConfigError; unknown chain/venue -> UnknownChain/UnknownVenue (ValueError);
  venue/chain mismatch -> ValueError.
```

## CONFIG HASH COMPATIBILITY

```
Legacy config (no market_profile_id): effective profile = solana-mainnet:pumpfun, resolved by RuntimeConfig.market_profile_id;
raw and canonical JSON are NOT mutated.
All 9 shipped runtime configs: RUNTIME_CONFIG / STRATEGY / POLICY / COST_MODEL / FRESHNESS_POLICY / QUOTE_MODEL / FEATURE_CONTRACT /
COVERAGE_VERSION / PROTOCOL hashes identical to the values frozen before the change
(tests/fixtures/config_hashes_baseline_05971e4.json == docs/phase_a/config_hashes_pre.json, verified by
tests.test_market_profiles.test_all_shipped_configs_keep_their_pre_phase_a_hashes).
Example: config/strategy_c_launch_sniper_0ms_15x.json config_hash 3782e39d82d1b0a52fd34961a7afd21d1cdc7837b74b094cb7847e4eb20593d7 before and after.
Explicit market_profile_id (newly authored multichain config) participates in config_hash; policy/cost hashes unchanged
(test_explicit_profile_participates_in_config_hash). Existing replay/source config FK/evidence bindings unaffected
(replay of the frozen dataset validated its fee-evidence config references before and after: PASS).
```

## SOLANA PARITY

```
SYNTHETIC REPLAY PARITY:        PASS   (parity_pre_vs_post.json)
CLEAN-V2 PRODUCTION PARITY:     PENDING / DATASET NOT PRESENT
                                data/market_capture_30m_clean_v2.sqlite, run market_capture_30m_clean_v2_capture_20260923T112208Z_994750,
                                documented dataset hash 8dbc5bf633f4c589fa816ce637fd18c2dd85383932de6e36d4209b49938b3636
                                are not in the provided workspace. The identical runner (tools/strict_replay.py) can be pointed at it:
                                python3 docs/phase_a/tools/strict_replay.py <repo> data/market_capture_30m_clean_v2.sqlite \
                                  market_capture_30m_clean_v2_capture_20260923T112208Z_994750 config/strategy_c_launch_sniper_0ms_15x.json \
                                  <slots> <new_output.sqlite> <manifest.json> <label>   (once on baseline 05971e4, once after Phase A; then tools/compare_parity.py)
```

Synthetic regression parity details (strict, unmodified production semantics, no monkeypatch, no RAM-only legacy-evidence diagnostics):

```
Source dataset:      data/phase_a/synthetic_capture_seed20260924.sqlite
                     SHA-256 d17136ae1c02b345d15e5e3393a223e804c615680217e1f8417e0e7d6993e2f9 (frozen read-only BEFORE any source edit)
                     generated by tools/gen_source_capture.py (SHA-256 84ed491188f86fb8f3da5ca491d4b2abaac7d0bef41acbe7e37a0000beeee443, seed 20260924)
                     through production StateOwner(capture_only=True) + Recorder + SqliteWriter + production encode_fields/event_discriminator:
                     300 CREATEs (300 actionable), 11,010 canonical events (8,909 BUY / 1,801 SELL / 300 CREATE), 22,109 frames on
                     helius_standard + existing_rpc + pumpportal hints, 352 fee snapshots (refreshed every 20 s), 2 PROVIDER_DISCONNECT,
                     1 CANONICAL_GAP (2 s), 644 failed-tx frames, 50 undecodable frames, 30-minute recorded window
Source run id:       phase_a_synth_capture_capture_synthetic_seed20260924
Dataset hash:        c62ca4ceeebc093fd6c7988fb2caf494f0397c5c8ef879b2f7565b8d859f95d7 (identical before/after: decoder_code_hash unchanged)
Config:              config/strategy_c_launch_sniper_0ms_15x.json (file config_hash 3782e39d…93d7) + production portfolio_limit_overrides(10)
                     -> effective config_hash identical before/after (see manifests)
Replay parameters:   run_replay(persist=True, persist_observations=True, compute_features=False, stress 1/0/0/seed 0,
                     replay_callback_lateness=True, execution_delay_provider=None, mint_filter=None, end/drain=None)
Runner:              tools/strict_replay.py SHA-256 75a965d91d611babf5e454bac093f4b301c36a9824ac1532c8f4ebe62adca3ab (same file pre/post)
Runner determinism:  pre vs pre2 identical (determinism_pre_vs_pre2.json)
Outputs:             replay_manifest_pre.json.gz / replay_manifest_post.json.gz / replay_manifest_post_fix.json.gz (full semantic content), *.summary.json
Final tree:          re-run after review fix R1 (post_fix): parity_pre_vs_post_fix.json -> PASS, identical dataset hash, runner and results

EXACT INTEGER RESULTS (lamports), identical before and after:
  realized_net_pnl_lamports                        1,468,058,175
  pessimistic_pnl_lamports_token_residual_zero       484,285,585
  unresolved_exposure_entry_cost_lamports            983,772,590
  locked_rent_lamports_end                            20,392,800
  fees: protocol 115,107,196 / creator 34,532,187 / network 741,100 / tip 0 / rent_locked 114,199,680 / rent_refunded 93,806,880
  cash: initial 10,000,000,000, ledger_cash_sum 463,892,785, end 10,463,892,785 (reconciled), RAM cash 10,463,674,985, reserved 217,800
  counts: 356 decisions, 56 entry intents, 56 entry fills, 0 entry failed, exits CREATOR_SELL 19 / TARGET 27 / STOP 0 / TIMEOUT 0,
          10 UNRESOLVED (ATTEMPT_STATUS_UNKNOWN, all censored across the canonical gap/run end), 112 attempts, 102 fills, 102 ledger rows,
          11,010 events; qualification_status UNQUALIFIED_DATA (zero-delay run, as designed)

SEMANTIC COMPARISON (run-id-independent keys; only the generated replay run-id prefix is stripped, nothing else is ignored):
  events           11,010  equal   key (signature, instruction_index, mint, event_type) -> all market_events columns
  decisions           356  equal   ordered by (receive_mono_ns, arrival_sequence): action, reason, quote state, timestamps, position
  attempts            112  equal   (position n, side, attempt number): status, submit/scheduled/modeled fill, amounts, fee evidence id, fee amounts, failure reason, fill evidence
  fills               102  equal
  selected fee evidence   equal   fee_snapshot_ids referenced by attempts + full fee_snapshots content
  ledger              102  equal
  positions            56  equal   state, exit reason, net pnl, entry cost, fees, unresolved reason, research status, open/close
  quote snapshots         equal
  quality events          equal
  RAM digest (replay.digest) equal
  semantic_sha256 d9725dcfc33088ce88c3a03fdda12082c729a1cba69f7c51072820e5d517e18c before and after
```

## PERFORMANCE BEFORE / AFTER

Benchmark: `tools/bench_hotpath.py` (SHA-256 1cfda126481f1bac5d46e064fa12842070dce3b45939d180bcfe4d13a99537ba, same file pre/post),
real `LiveClock` + `LiveScheduler` (asyncio) + `Recorder` + `SqliteWriter` thread + `PaperExecutionAdapter` + `StateOwner`
(constructed as in `Application.run`), production `FeedAdapter` per feed with the production `FakeWebSocket` transport injected via
its `connect` parameter, input = the frozen synthetic recording (22,109 frames, 3 feeds) time-compressed, 1000 slots so every CREATE
produces an ENTRY intent, fee snapshots published through `StateOwner.on_fee_snapshots` every 5 s. Metrics are read from the persisted
production telemetry (`feed_observations`, `decisions`, `execution_attempts`). Same host, same process type, runs executed back to back.
Raw samples are in `bench_pre.json` / `bench_post.json` (`raw_samples`).

PRIMARY — 10x time compression (~120 msg/s, ≈ real Pump.fun load), 190 s per run, writer enabled, 0 internal errors, identical
sample counts in every run. Four runs, interleaved on the same host: `pre` (baseline tree) -> `post` (Phase A) -> `post_fix`
(Phase A + R1; the benchmark does not go through Application, so R1 cannot change it) -> `pre2` (untouched baseline tree at
/tmp/orig, run last as a noise control). Values in ms.

| metric | n | run | min | p50 | p95 | p99 | max | mean |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| receive -> decode | 22,109 | pre | 0.0177 | 0.1094 | 0.1657 | 0.2278 | 7.4965 | 0.1143 |
| | | pre2 | 0.0206 | 0.1192 | 0.1746 | 0.2210 | 2.8129 | 0.1224 |
| | | post | 0.0184 | 0.1061 | 0.1536 | 0.1900 | 7.8344 | 0.1092 |
| | | post_fix | 0.0206 | 0.1170 | 0.1648 | 0.2114 | 8.1844 | 0.1195 |
| receive -> decision (ENTRY+EXIT decisions) | 521 | pre | 0.1501 | 0.7723 | 1.2455 | 1.4598 | 1.9651 | 0.6379 |
| | | pre2 | 0.1910 | 0.8738 | 1.1084 | 1.5057 | 1.7278 | 0.6864 |
| | | post | 0.1588 | 0.7497 | 1.0056 | 1.2636 | 1.5349 | 0.6011 |
| | | post_fix | 0.1913 | 0.8517 | 1.0578 | 1.4252 | 2.0448 | 0.6643 |
| decision -> enqueue (ENTRY) | 300 | pre | 0.0155 | 0.0237 | 0.0389 | 0.0772 | 0.1446 | 0.0266 |
| | | pre2 | 0.0188 | 0.0259 | 0.0411 | 0.0551 | 0.0598 | 0.0277 |
| | | post | 0.0165 | 0.0238 | 0.0374 | 0.0607 | 0.1742 | 0.0261 |
| | | post_fix | 0.0198 | 0.0252 | 0.0382 | 0.0512 | 0.5927 | 0.0285 |
| **receive -> enqueue (ENTRY)** | 300 | pre | 0.6725 | **0.8743** | **1.4131** | **1.6606** | 2.0066 | 0.9352 |
| | | pre2 | 0.7756 | **0.9744** | **1.2044** | **1.5945** | 1.7735 | 1.0020 |
| | | post | 0.6379 | **0.8465** | **1.1349** | **1.3531** | 1.5793 | 0.8763 |
| | | post_fix | 0.8020 | **0.9469** | **1.1612** | **1.5757** | 2.0949 | 0.9749 |
| queue wait = entry callback lateness (asyncio) | 300 | pre | 0.0748 | 0.1006 | 3.3073 | 4.0897 | 4.2174 | 0.4514 |
| | | pre2 | 0.0883 | 0.1104 | 3.8473 | 4.1713 | 4.7358 | 0.4799 |
| | | post | 0.0694 | 0.0966 | 3.5503 | 3.9992 | 4.0700 | 0.3687 |
| | | post_fix | 0.0847 | 0.1054 | 3.6452 | 3.9334 | 4.1338 | 0.4456 |

(data-ready -> decision and all raw samples are in the bench_*.json files.)

STRESS — 200x time compression (~2,400 msg/s, recorder saturated, depth max 15,500 / 15,683):
receive->enqueue p50 0.7720 / 0.7421, p95 1.2504 / 1.0422, p99 1.9163 / 1.8090, max 2.0758 / 1.9851 ms; receive->decode p99 0.1666 / 0.1684 ms
(one post-change decode outlier max 22.7 ms vs 2.4 ms pre at full saturation; single sample, p99 identical, not reproducible at 10x).

```
PERFORMANCE DELTA (receive -> enqueue, Phase-A tree vs baseline tree, interleaved pairs):
  pre  -> post      p50 -3.2 %   p95 -19.7 %   p99 -18.5 %
  pre2 -> post_fix  p50 -2.8 %   p95  -3.6 %   p99  -1.2 %
  Baseline-tree noise on this shared 1-vCPU host: p50 0.874 -> 0.974 ms (+11 %) between two runs of byte-identical code; the
  Phase-A tree is at or below the baseline in both interleaved pairs on every hot-path percentile.
  receive -> enqueue p99 (all Phase-A runs): 1.353 ms / 1.576 ms  < 3.000 ms target  (baseline runs: 1.661 ms / 1.595 ms)
  No reproducible adapter-induced regression: identical sample counts, no new hop (StateOwner keeps direct references, FeedAdapter
  receives owner.on_raw bound method directly; verified by test_solana_wraps_existing_fee_resolver_and_feeds).
```

## TESTS RUN / TEST RESULT

```
Command (pre and post, verbatim):  python3 -m unittest discover -s tests -p 'test_*.py' -v
Python: 3.12.3
PRE  (baseline 05971e4, before any edit):  Ran 253 tests in 11.741s  OK   -> 253 PASS, 0 FAIL, 0 ERROR, 0 SKIP   (test_result_pre.txt)
POST (Phase A, before review):            Ran 280 tests in 11.705s  OK   -> 280 PASS, 0 FAIL, 0 ERROR, 0 SKIP   (test_result_post.txt)
POST (Phase A + review fix R1, FINAL):     Ran 287 tests in 13.337s  OK   -> 287 PASS, 0 FAIL, 0 ERROR, 0 SKIP   (test_result_post_fix.txt)
  253 existing tests unchanged (no test deleted, weakened, skipped or rewritten) + 34 new tests:
  test_chain_registry 4, test_venue_registry 5, test_runtime_assembly 7, test_market_profiles 8, test_multichain_domain 3,
  test_bsc_profile_fail_fast 7
Covered: unknown chain rejected; unknown venue rejected; venue/chain mismatch rejected; config hash changes with market profile;
legacy hashes unchanged; Solana profile resolves executable Solana + Pump components; Flap/Four profiles resolve metadata only and
runtime construction (assemble_runtime and StateOwner fallback) fails with NotImplementedError; BSC profiles are rejected by the real
entry points before any storage side effect (fail-fast integration test); injected components are the same
objects and yield an identical replay digest to the legacy construction; replay keeps its own engine injection and stays offline;
no darkbot.venues.flap / fourmeme / chains.evm / chains.bsc package exists.
```

## OPEN QUESTIONS

```
1. CLEAN-V2 PRODUCTION PARITY must still be executed on the machine holding data/market_capture_30m_clean_v2.sqlite
   (baseline run on 05971e4, post run on the Phase-A tree, same runner/command, compare_parity.py). Phase A is not finally accepted before that.
2. tools/strict_replay.py uses run_replay(persist=True, persist_observations=True) and a production portfolio_limit_overrides(<slots>)
   override (10 in the synthetic run) so that more than one position exists per 590 s hold. Confirm the slot count you want for the
   clean-v2 oracle run (1 = pure production config, 10 = research convention); both are recorded in the manifest.
3. Generic asset/tx/block/native-amount fields on the domain dataclasses and additive `runs`/`market_events` columns (handoff §7.1/§15)
   were intentionally NOT added in Phase A (outside the approved file list; "no historical DB migration in Phase A"). The mapping is
   exposed through SolanaChainAdapter.tx_id_from_raw/block_ref_from_raw and PumpFunVenueAdapter.asset_id_of/native_amount_base_units_of.
4. The synthetic dataset data/phase_a/*.sqlite (58 MB) is gitignored on purpose (do not `git add -f`); the generator + seed +
   SHA-256 reproduce it, but the frozen file is the one the manifests refer to.
5. main.py keeps its previous imports of resolve_feed_endpoints/resolve_rpc_http/http_rpc (now unused there) to avoid touching any
   external import surface; removal is Phase-B cleanup.
6. The 200x stress run showed one 22.7 ms receive->decode outlier post-change (max only, p99 unchanged); it did not reproduce in the
   primary run and there is no code path difference in decode. Flagged for the reviewer, not treated as a regression.
```

## UNVERIFIED CONSTANTS

```
NONE introduced. No Flap/Four.meme contract address, ABI, selector, topic, curve formula, tax, gas, nonce or builder constant exists in
the tree. The only BSC metadata are the documented identity facts from the handoff (chain family "evm", native symbol "BNB",
18 decimals) and the display names "BSC / Flap.sh" / "BSC / Four.meme" with status RESEARCH.
```

## STATUS

```
PHASE A IMPLEMENTATION:     COMPLETE
UNIT/REGRESSION TESTS:      287/287 PASS (253 existing + 34 new), 0 FAIL, 0 ERROR, 0 SKIP
SYNTHETIC REPLAY PARITY:    PASS on the final tree (exact lamports + events/decisions/attempts/fills/fee evidence/ledger/positions/RAM digest identical)
CLEAN-V2 PRODUCTION PARITY: PENDING / DATASET NOT PRESENT
PERFORMANCE GATE:           PASS (receive->enqueue p99 1.353 / 1.576 ms Phase A vs 1.661 / 1.595 ms baseline, all < 3.000 ms; no regression)
FAIL-FAST BSC RUNTIME:      PASS (review fix R1; integration test at the real entry points)
PHASE A FINAL ACCEPTANCE:   PENDING CLEAN-V2 VALIDATION
NEXT PHASE:                 STOP (no Phase B started; awaiting independent review + clean-v2 parity)
COMMIT:                     NOT AVAILABLE IN PROVIDED WORKSPACE (.git absent) — apply docs/phase_a/phase_a.diff on 05971e4;
                            new files listed above; post-change source manifest 7eb2117c…b9d7
```
