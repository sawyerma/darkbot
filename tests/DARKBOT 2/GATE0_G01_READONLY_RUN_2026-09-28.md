> **Superseded findings, 2026-09-28:** The CREATE2 mismatch was caused by erroneous 32-byte deployer padding; the correct 20-byte formula exactly matches the token. The quote token is DOGE/8, not WBNB. The previous BUY fixture is the fifth external BUY, not the first. See [verified correction](G02_FLAP_CORRECTION_2026-09-28.md) and `gate0-evidence/verify_flap_g02.py`. Historical sections below are retained as an audit trail.

# G0.1 read-only run — 2026-09-28

Project code was not changed. No key was read, no transaction was signed, and no transaction was sent.

## Commands and results

### 1. OpenFour source freeze

```sh
curl -L --fail --silent --show-error https://api.github.com/repos/four-meme-community/openfour-docs/commits/main
curl -L --fail --silent --show-error https://raw.githubusercontent.com/four-meme-community/openfour-docs/main/README.md
curl -L --fail --silent --show-error https://raw.githubusercontent.com/four-meme-community/openfour-docs/main/docs/integration-guide.md
```

Result:

- Repository: `four-meme-community/openfour-docs`
- Pinned commit: `0527d529d59c6a3fbc9927ce391ecc97c66e1b01`
- The README explicitly says the sample contracts are unaudited/reference-only and do not contain the production Core/Registry/Deployer.
- The integration guide defines OpenFour as a separate protocol family with Registry, Core, Tools, FeeRouter and ZapRouter.
- Pinned files and SHA-256 hashes are under `gate0-evidence/2026-09-27-g01/openfour/`.

### 2. Classic Four fixture

```sh
python3 gate0-evidence/read_rpc.py \\
  --url https://bsc-dataseed.bnbchain.org \\
  --out gate0-evidence/2026-09-27-g01/canonical_fixture_scan \\
  --scan-blocks 256
```

Result:

- Chain ID: `56`.
- Canonical create candidate found after a bounded 32-block scan.
- TX: `0x8cc24c0cb9f40ee92240959fbafece1a2fc22f2e367b42366050a33cba827e5b`.
- Block: `124415333`, hash `0xbcf5ab6e8e741e11acb17495d58a05af93f2ce75d6acf1e4ffbd3fa253a94abd`.
- Destination: TokenManager2 `0x5c952063c7fc8610ffdb798152d69f0b9550762b`.
- Selector: `0x519ebb10` (`createToken(bytes,bytes)`).
- Receipt status: `0x1`; receipt block/transaction identity matched; event was not removed.
- Decoded `TokenCreate` identifies token `0xa89a24209a3ae8ba1ffb3f0567b69ef4765bffff`, creator `0x1985ec0215a94db223f5ae98b436074c4be1219b`, requestId `103111172`, symbol `YING`.

This is a real launch/create fixture. It is not yet a proof of an external BUY.

### 3. Classic Helper3 state probe

```sh
python3 gate0-evidence/read_rpc.py \\
  --url https://bsc-dataseed.bnbchain.org \\
  --out gate0-evidence/2026-09-27-g01/four_helper_probe/rpc_latest2 \\
  --requests gate0-evidence/2026-09-27-g01/four_helper_probe/requests_latest2.json
```

Result:

- `getTokenInfo(address)` selector: `0x1f69565f`.
- Helper: `0xF251F83e40a78868FcfA3FA4599Dad6494E46034`.
- Latest read returned 12 ABI words.
- Word 0 is `2` and word 1 is the exact TokenManager2 address.
- The raw response is preserved; unknown tuple positions are intentionally not renamed without the helper ABI.
- A historical call at the old fixture block returned public-RPC `missing trie node`; the latest-state call succeeded. This is an RPC archive limitation, not a protocol conclusion.

### 4. OpenFour Registry/Core/Tools resolution

```sh
python3 gate0-evidence/read_rpc.py \\
  --url https://bsc-dataseed.bnbchain.org \\
  --out gate0-evidence/2026-09-27-g01/openfour/registry_call_rpc \\
  --requests gate0-evidence/2026-09-27-g01/openfour/registry_calls.json

python3 gate0-evidence/read_rpc.py \\
  --url https://bsc-dataseed.bnbchain.org \\
  --out gate0-evidence/2026-09-27-g01/openfour/core_calls_rpc \\
  --requests gate0-evidence/2026-09-27-g01/openfour/core_calls.json
```

Result at current latest state:

- Registry: `0x912CEf0C3aE9Ab6eB3Ec87cab69371cFb317Ab94`.
- `openFourCore()` → `0xebe7b6c1089d9f72ad07f34e36d898e44e5e27f3`.
- `openFourTool()` → `0x97da82f56fef1a0b974587a48135fa9da2a4e905`.
- `feeRouter()` → `0x7383b244b416db16aa9dd0ab7c296aa60255b9a2`.
- `wrappedNative()` → `0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c`.
- Registry/Core/Tools are proxies. EIP-1967 implementation slots resolve to:
  - Registry: `0x9d6f9e62a2ffe33ce81bd1f291bdea74e3a9dc6b`.
  - Core: `0x42a7378dcbc7dcd644b5c9b196c5451f847cbedc`.
  - Tools: `0x7eca56ed477345ad17cf327f3b54fa5226434b98`.
- Official ABI files from the pinned commit are preserved and hashed in the same directory. The ABI confirms `createToken(bytes,bytes)`, `buy(...)`, `buyByBudget(...)`, `estimateBuy(...)` and `estimateBuyByBudget(...)`.

## Current gate status

`G0.1 = PARTIAL EVIDENCE`, not PASS.

Still required before any implementation claim:

1. Flap current canonical launch fixture and exact ABI/source freeze.
2. Four Classic external BUY fixture with receipt/log/state proof and exact selected overload.
3. OpenFour canonical create/BUY fixture; Registry resolution alone is not launch proof.
4. Explicit G0.2 pre-computability table, including whether the token address is knowable before T0.
5. G0.3 local Launch→BUY correctness harness.
6. Only after dry qualification: controlled value-bearing test, then latency qualification on dedicated Linux hardware.

### 5. Flap live version check

```sh
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed.bnbchain.org --out gate0-evidence/2026-09-27-g01/flap_version_rpc --requests gate0-evidence/2026-09-27-g01/flap_version_call.json
```

Result: Portal `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0` returns `v5.24.0` from `version()`. The public deployment page still listed `v5.23.1`, so that documentation value is stale for the live contract. The implementation slot remains `0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20`.

This reopens the Flap scope: its ABI, launch mode, BUY mode and canonical fixture must be frozen against v5.24.0 before implementation.

### 6. Flap launch → external BUY ordering fixture

```sh
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed.bnbchain.org --out gate0-evidence/2026-09-27-g01/flap_fixture_rpc --requests gate0-evidence/2026-09-27-g01/flap_fixture_requests.json
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed.bnbchain.org --out gate0-evidence/2026-09-27-g01/flap_between_rpc --requests gate0-evidence/2026-09-27-g01/flap_between_requests.json
```

Result:

- Flap launch `newTokenV7` selector `0x8cb5772c` succeeded in block `124415357`, transaction index `0`.
- `TokenCreated` decoded token `0xabe1fe16cf6bc48c9524713054efe6e954b17777`, name `Koyuki`, symbol `こゆき`.
- First observed external Flap BUY for that token was block `124415358`, transaction index `16`, tx `0x73716f5fffab9493483d8eae7c1b170bc4d33feaa8346bb07d5c6a0d91d2595b`, receipt status `1`, buyer `0x9ac2168fd668e3c0ffc8359fe4aa89e430fa9c99`.
- A second external BUY followed in block `124415359`, transaction index `17`, tx `0x6f723464dbb64ba0d5e6f468b3a411161e7861baa6214dc7d38938b5a46514b6`, receipt status `1`.
- The block-`124415360` BUY `0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b` also succeeded and emitted `TokenBought` plus a target-token `Transfer` to its buyer.

This proves launch/BUY event decoding and ordering for one historical Flap fixture. It is not DarkBot’s purchase and provides no local latency measurement.

### 7. Bounded OpenFour canonical search

```sh
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed.bnbchain.org --out gate0-evidence/2026-09-27-g01/openfour/scan64_rpc --requests gate0-evidence/2026-09-27-g01/openfour/scan64_requests.json
```

Result:

- Reference latest block: `124419142`.
- 64 full-transaction blocks were checked.
- No transaction to the resolved OpenFour Core used `createToken(bytes,bytes)` (`0x519ebb10`).
- No direct OpenFour Core transaction occurred in that bounded window.

Therefore OpenFour still has no canonical launch fixture in this evidence set and remains unqualified for primary scope.

## 8. Current production-path test and local latency

```sh
python3 -m unittest discover -s tests -p 'test_*.py'
python3 docs/phase_a/tools/bench_hotpath.py \
  /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot \
  /Users/sawyer_ma/Documents/ChatGPT/DARKBOT\ 2/synthetic_capture_seed20260924.sqlite \
  phase_a_synth_capture_capture_synthetic_seed20260924 \
  /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/config/strategy_c_launch_sniper_0ms_15x.json \
  100 20 \
  /Users/sawyer_ma/Documents/ChatGPT/DARKBOT\ 2/gate0_phasea_bench100.sqlite \
  /Users/sawyer_ma/Documents/ChatGPT/DARKBOT\ 2/gate0_phasea_bench100.json current_mac_100
```

Result:

- Existing suite: **287 tests OK** in 13.337 s.
- In-memory Python syntax compile: **119 files OK**.
- Current Mac Phase-A PAPER hotpath benchmark: 22,109 frames, 300 ENTRY intents, writer failures 0, overflow false.
- `receive -> decode`: p99 `0.859333 ms`.
- `receive -> decision`: p99 `2.545375 ms`.
- `decision -> enqueue`: p99 `0.078375 ms`.
- `receive -> enqueue`: p99 `2.992500 ms`, max `3.559292 ms`.
- Queue callback lateness: p99 `0.756292 ms`.

This is the existing Solana/PAPER path on the Mac. It is not a BSC measurement and it does not include BSC transaction build/sign/transport handoff. It is already borderline against a 3 ms local enqueue gate and exceeds it in the maximum sample.
