# DarkBot BSC — Archived Historical Implementation Handoff

Status: **ARCHIVED / DO NOT EXECUTE AS CURRENT PLAN**
Updated: 2026-09-27

This file preserves historical implementation reasoning only.

It is **not** a current scheduler, phase order, performance contract or authorization to implement BSC work.

Current authoritative order:

1. `docs/GATE0_TESTPLAN_SHORT.md`
2. `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md`
3. `docs/BSC_DUAL_VENUE_ROADMAP.md`
4. `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`
5. active phase-specific contract

Current BSC rule:

```
Gate 0 first:
current venue evidence
-> pre-T0 actionability proof
-> minimal native build/sign proof
-> pending-coverage proof
-> real inclusion-path race including ordered builder paths
-> controlled Mainnet first-BUY ordering proof
-> PASS / ADAPT / STOP

Only after PASS:
C0 -> C1 -> C2 -> C3 -> full venue decoders -> F -> G -> H -> I -> J -> K
```

Historical claims below such as:
- starting new BSC work at C0 before Gate 0;
- using `-10 ms vs canonical` as the product success gate;
- treating local geth acceptance as the main latency proof;
- postponing builder/backrun testing until late LIVE phases;
- delaying all venue BUY semantics until after C3;

are **superseded and must not be followed**.

The historical text remains below only for audit/history.

---

# DarkBot BSC Dual-Venue — Implementation Handoff for Claude

**Purpose:** implementation contract for the next coding agent.  
**Repository:** `sawyerma/darkbot`  
**Target branch:** `main` only unless explicitly instructed otherwise.  
**Parent design documents:**

- `docs/MULTICHAIN_ROADMAP.md`
- `docs/BSC_DUAL_VENUE_ROADMAP.md`
- `docs/RESEARCH_STATUS_2026-09-23.md`
- `docs/STRATEGY_C_SOURCE_OF_TRUTH.md`

Phase-specific future contracts:

- `docs/BSC_PHASE_B_OPTIONAL_CLEANUP_DRAFT.md` — optional draft only; not approved;
- `docs/BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md` — Phase D/M7;
- `docs/BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md` — Phase E/M8;
- `docs/BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md` — Phase F/M9;
- `docs/BSC_ECONOMICS_GATE_CONTRACT.md` — mandatory post-M9 economics gate.

The active roadmap decides when a future contract becomes executable. A future contract file does not itself authorize starting that phase.

This document is intentionally prescriptive. Do not redesign the architecture unless a concrete contradiction in the repository requires it. If a contradiction is found, stop and report it with exact file/line references before changing semantics.

## Current implementation anchor and continuation point — 2026-09-25

The authoritative **Phase-A production-code implementation anchor** is:

```
main@004f440c39c147087a7385bf16a4dc6660058936
```

Historical documentation-continuity anchors after the Phase-A code seam include:

```
main@a65ba4aa0129c02328ccdb8d9426741ab3fdfc9d
main@3562890a037652079ba94cf4c100811b7f497d44
```

These are historical documentation commits, not a dynamically maintained "current HEAD".

**The exact starting HEAD for any coding run is supplied by that run's external task.** Claude must compare that task-provided SHA with `git rev-parse HEAD` and STOP on mismatch. Do not infer a current HEAD from a SHA written in this document.

That commit already integrates **Phase A** of this handoff:

- chain/venue adapter contracts and registries;
- runtime market-profile registry and assembly;
- legacy-config compatibility without changing historical Solana config hashes;
- additive `StateOwner` decoder/reducer/market-profile dependency injection while preserving the existing `engine=` replay injection;
- thin Solana/Pump.fun wrappers around the existing implementation;
- explicit `bsc-mainnet:flap` and `bsc-mainnet:fourmeme` metadata profiles that fail closed for normal runtime execution;
- fail-fast profile preflight before storage side effects.

Do **not** reimplement or replace that seam. New BSC work must extend it.

The entire historical Phase-A evidence tree under `docs/phase_a/` is **byte-for-byte frozen evidence**, including reports, scripts, manifests, checksums and benchmark artifacts. It must not be edited, replaced, reformatted or regenerated in place for later phases.

### Authoritative source-of-truth order and exact current state

There must be no interpretation about which document controls which decision:

1. **`docs/BSC_DUAL_VENUE_ROADMAP.md`** controls the active BSC phase/milestone order and the current DONE/NEXT/PENDING status.
2. **This file, `docs/BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md`,** controls the technical implementation contract and acceptance rules for the active phase.
3. **`docs/MULTICHAIN_ROADMAP.md`** is parent architecture and historical design rationale only. Its older milestone numbering and older phase scope MUST NOT be used to schedule current BSC work where it differs from the two documents above.
4. **`docs/phase_a/PHASE_A_REPORT.md`** is immutable historical evidence. It is not the current task scheduler and must not be rewritten to make later work look already completed.

Exact state at the start of Phase C:

| Area | Status | Meaning |
| --- | --- | --- |
| Phase A / M0-M3 code seam | **IMPLEMENTED** | chain/venue registries, Solana/Pump wrappers, profile/runtime assembly, config-hash compatibility, StateOwner DI and fail-fast BSC metadata are in production code |
| Existing regression suite | **287/287 PASS in the recorded Phase-A result** | historical result only; rerun before Phase-C edits |
| Synthetic Phase-A replay parity | **PASS in historical Phase-A evidence** | useful regression evidence, not the missing strict real-capture oracle |
| Strict real-capture Solana parity | **OPEN** | must be re-established on a new separate Solana capture; never fabricate it |
| Phase-A validation tooling | **REVIEW FIX REQUIRED BEFORE PHASE-C CODE** | two concrete tooling defects below must be corrected without changing Solana semantics |
| Phase B cleanup | **SKIPPED** | do not start it unless a separate concrete review requires it |
| Phase C / M4-M6 | **NEXT** | generic EVM core + BSC chain + shared BSC dispatcher + minimum read-only capture storage |
| Phase D / M7 | **PENDING** | Flap read-only decoder only |
| Phase E / M8 | **PENDING** | Four.meme read-only decoder only |
| Phase F / M9 | **PENDING** | combined real 30-minute BSC capture |
| Quote/fee/tax economics | **POST-PHASE-F** | only after the real BSC capture establishes actual protocol versions/modes/evidence |
| Strategy C multichain / BSC replay / PAPER / LIVE | **POST-PHASE-F** | not part of Phase C/D/E/F decoder/capture work |

### Mandatory Phase-C preflight and validation-tool correction

The order is mandatory and removes all ambiguity:

1. **Before ANY edit at all** — including validation-tool work — record `git status --short`, `git rev-parse HEAD`, Python version, run the complete existing test suite, and run `python3 -m compileall darkbot tests`.
2. Save new preflight/test/benchmark outputs outside `docs/phase_a/`.
3. Only if the untouched baseline is green, create maintained corrected validation tools outside the frozen evidence tree.
4. Run tests for those maintained tools.
5. Only then begin Phase-C production-code work.

The two known historical tool defects are in frozen files:

- `docs/phase_a/tools/compare_parity.py`
  - it reports effective/file config-hash equality but does not fully enforce those advertised invariants in its final PASS decision;
  - it does not return a non-zero process exit code on parity failure.

- `docs/phase_a/tools/bench_hotpath.py`
  - it constructs `StateOwner` and feed adapters through the retained fallback/direct path;
  - therefore it does not fully exercise the current `Application.run -> assemble_runtime -> injected components` wiring even though its historical description says it is constructed like `Application.run`.

**Do not modify either frozen file.**

Create the maintained corrected versions at exactly:

```
tools/validation/compare_parity.py
tools/validation/bench_hotpath.py
```

If `tools/` or `tools/validation/` does not yet exist, create it.

Requirements:

- the maintained parity tool must enforce every invariant it reports as required, including intended source/dataset/runner/config-hash equality, and must exit non-zero on FAIL;
- add deterministic tests for PASS, each required mismatch class, and FAIL exit status;
- the maintained hot-path benchmark must exercise the actual current Phase-A runtime assembly/injection seam while preserving equivalent workload and metric meaning;
- new validation outputs belong under `exports/phase_c_validation/` (or temporary test directories for automated tests), never under `docs/phase_a/`;
- `git diff -- docs/phase_a` must remain empty throughout the task;
- do not change Solana/Pump/Strategy-C semantics to make a validation tool pass.

These are Phase-C preflight/evidence-tool fixes, not Phase B and not a Solana refactor.

### Formal Solana parity status

Phase-A code integration is complete, but the old real `clean-v2` source database used for the intended strict production replay parity check is no longer available locally. Therefore:

- do not invent or backfill the missing parity result;
- do not use the positive legacy-compatible diagnostic replay as the production parity oracle;
- after the BSC capture path is ready, record a new, separate Solana 30-minute capture in `data/market_capture_solana_30m.sqlite`;
- replay that exact same new Solana database once with baseline `05971e4ed35a8a98349e298d2de4e61f8584fc5b` and once with the current/refactored code;
- require semantic and exact-lamport equality.

This parity task is **Solana-only** and must never use the BSC database.

### Phase / milestone crosswalk from the current commit

Use the later dual-venue milestone numbering in `docs/BSC_DUAL_VENUE_ROADMAP.md` as authoritative for BSC implementation order:

| Handoff phase | Dual-venue milestones | Status at `004f440` |
| --- | --- | --- |
| Phase A | M0-M3 architecture/seam work | IMPLEMENTED; strict real-DB Solana parity must be re-established on a new Solana capture |
| Phase B | optional adapter cleanup only | SKIP unless an independent review identifies a concrete need |
| Phase C | M4 EVM core + M5 BSC chain + M6 shared BSC dispatcher | NEXT |
| Phase D | M7 Flap read-only decoder | PENDING |
| Phase E | M8 Four.meme read-only decoder | PENDING |
| Phase F | M9 combined 30-minute BSC capture | PENDING |

The older milestone numbering in `docs/MULTICHAIN_ROADMAP.md` is architectural history. Where numbering differs, the later dual-venue roadmap and this handoff govern the implementation sequence.

### Capture database separation

When Phase F is accepted, run the two 30-minute captures in parallel but keep them completely separate:

```
BSC / Flap + Four.meme
-> data/market_capture_bsc_30m.sqlite

Solana / Pump.fun
-> data/market_capture_solana_30m.sqlite
```

The BSC database is for BSC pending/canonical coverage and later BSC research.  
The Solana database is for the strict `05971e4` vs current Solana parity replay.  
Never merge, cross-use, or compare rows across these databases as if they belonged to the same chain.

---

# 0. Non-negotiable rules

## 0.1 Preserve Solana behavior exactly

The existing Solana/Pump.fun system is frozen evidence.

Do **not** change the semantics of:

- `LEGACY_BACKEND_FILTER_V1`
- existing Pump.fun decoding
- existing Solana fee accounting
- existing replay timing semantics
- existing historical SQLite datasets
- existing Strategy A/B/C behavior
- current PAPER safety boundary

All existing tests must remain green.

Historical Phase-A methodology required freezing the then-current implementation revision before Phase-A code changes and running the strict production replay described in Section 0.5. That instruction is **historical and must not restart Phase A now**. The historical commit `4af978e18f91dea5d921ed7ef284b15d55d6c987` remains documentary context only. The still-missing strict real-capture Solana parity evidence remains OPEN and must be re-established later exactly as specified by the active roadmap.

## 0.2 No protocol guessing

For Flap.sh and Four.meme:

- do not invent contract addresses;
- do not invent ABI entries;
- do not infer selectors from function names without verified ABI/source;
- do not invent bonding-curve formulas;
- do not guess token-address derivation;
- do not assume taxes are zero;
- do not assume approve/permit behavior;
- do not assume pending transaction visibility;
- do not assume a builder guarantees ordering.

If a protocol detail is not verified, model it as **UNKNOWN / UNSUPPORTED**, not as zero/default.

## 0.3 PAPER/replay must never broadcast

The existing `LiveBroadcastForbidden` boundary remains mandatory.

```
CAPTURE -> no signing, no broadcast
REPLAY  -> no signing, no broadcast
PAPER   -> no signing, no broadcast
LIVE    -> separate future adapter only
```

Do not weaken this to make tests convenient.

## 0.4 One BSC chain implementation, two isolated venue implementations

Correct:

```
BSC chain
├── Flap venue
└── Four.meme venue
```

Incorrect:

```
BSC1 bot
BSC2 bot
```

Incorrect:

```
if venue == "flap":
    ...
elif venue == "fourmeme":
    ...
```

inside generic BSC transport/fee/nonce code.

## 0.5 Phase-A baseline and exact parity contract

This section is mandatory and must be completed **before the first implementation edit**.

### 0.5.1 Freeze the actual starting revision

Record the current full commit SHA at execution time:

```bash
git rev-parse HEAD
git status --short
```

The resulting full SHA is the Phase-A baseline revision. Do not substitute a historically documented commit merely because it appears in research notes.

Also record the current working-tree status. Unrelated local captures/exports must not be modified, deleted, staged, or committed as part of Phase A.

### 0.5.2 Run the complete pre-change test suite

Run the repository's complete existing test suite **before any source edit** and persist the exact command and complete result.

Minimum requirement:

```bash
pytest
```

If the repository normally requires a more specific invocation, use that actual invocation and record it verbatim.

Store:

- full baseline commit SHA;
- Python version/environment;
- exact test command;
- pass/fail/skip counts;
- complete stdout/stderr or durable test-result artifact.

A failing pre-change suite is not silently accepted. Stop and report it.

### 0.5.3 Create a strict Solana replay baseline before the refactor

Run one **strict, unmodified production Solana/Pump.fun replay** on the current implementation.

The baseline manifest must record:

- full baseline commit SHA;
- source database path;
- source run id;
- dataset hash;
- complete runtime config content and config hash;
- complete research/replay parameters;
- exact replay command or exact one-off runner command used;
- SHA-256 of any one-off runner used;
- output database/artifacts;
- exact integer economic results in **lamports**;
- semantic decision/event/attempt/fill/fee/ledger digests described below.

Use the existing clean source dataset when available:

```
data/market_capture_30m_clean_v2.sqlite
source run:
market_capture_30m_clean_v2_capture_20260923T112208Z_994750

documented dataset hash:
8dbc5bf633f4c589fa816ce637fd18c2dd85383932de6e36d4209b49938b3636
```

The runner/config used for the Phase-A parity baseline must use the **current strict production semantics**. It must not use the historical RAM-only legacy-evidence compatibility monkeypatches documented in `RESEARCH_STATUS_2026-09-23.md`.

If there is no existing single CLI command for the required strict replay, a read-only one-off runner may be created **outside the production source tree** (for example under `/tmp`). It must:

- import the current production replay implementation normally;
- not monkeypatch decoder/reducer/evidence semantics;
- not modify source data;
- record its exact file hash and command;
- be used identically before and after the refactor.

### 0.5.4 Required parity comparison

After Phase A, rerun the **same** baseline procedure and compare before/after.

The comparison must include at minimum:

- normalized/canonical event semantic identities;
- decision sequence and decision action/reason;
- entry and exit attempt sequence;
- attempt statuses;
- fills and fill quantities;
- fee evidence actually selected;
- fee amounts;
- ledger records;
- position terminal states;
- exit reasons;
- realized PnL in exact lamports;
- pessimistic PnL in exact lamports;
- unresolved exposure/cost in exact lamports.

Do **not** use rounded SOL values as an equality criterion.

Generated run-specific IDs are allowed to differ. Compare them through a predetermined semantic mapping, not by raw generated id string. Use stable semantic keys such as:

```
event:
(tx/signature, instruction-or-log index, asset/mint, event type)

decision:
(asset/mint, source create tx/signature, action, reason)

attempt:
(asset/mint, side, attempt number)

fee evidence:
(kind, source/provider/method, canonical normalized content)

position:
(asset/mint, entry decision semantic key)
```

If a generated ID contains a run-id/canonical-prefix component, strip/map only that documented run-specific component; do not ignore substantive differences.

### 0.5.5 Diagnostic legacy-compatible runs are separate

The positive diagnostic figures in the research document, including the legacy-compatible 3.0x/750 ms experiments, are **not** the mandatory production parity oracle.

Those diagnostic runs may be repeated as an additional research check, but:

- they stay clearly labeled diagnostic;
- RAM-only/monkeypatch evidence semantics stay outside production code;
- their output must not force a production-semantic change;
- failure to reproduce a diagnostic monkeypatch result is not permission to alter strict production behavior.

---

# 1. Final runtime model

The project must expose three independent market profiles:

```
solana-mainnet:pumpfun
bsc-mainnet:flap
bsc-mainnet:fourmeme
```

Future GUI labels:

```
Solana / Pump.fun
BSC / Flap.sh
BSC / Four.meme
```

The stable backend ID is authoritative. The display label is not.

---

# 2. Required architecture

## 2.1 Core

Shared by all chains/venues:

- strategy lifecycle
- replay
- PAPER
- portfolio
- positions
- event normalization
- persistence
- reporting
- latency telemetry
- quality/evidence handling
- qualification

The core must not import Flap/Four/Pump-specific code after the refactor seam is complete.

## 2.2 Chain adapters

### Solana

Existing behavior wrapped, not rewritten.

### EVM base

Shared infrastructure:

- JSON-RPC
- WebSocket
- pending tx envelopes
- block envelopes
- logs
- receipts
- nonce/replacement semantics
- reorg semantics
- gas accounting primitives
- transaction signing primitives

### BSC

Specialization:

- chain id 56
- BNB native units
- BSC provider config
- BSC mempool/pending transport
- block/log/receipt collection
- BSC fee model
- future builder/sender abstraction

## 2.3 Venue adapters

### Pump.fun

Wrap existing decoder/reducer/quote/execution-plan behavior.

### Flap

Completely separate package.

### Four.meme

Completely separate package.

---

# 3. Target package skeleton

Create incrementally:

```
darkbot/
  chains/
    __init__.py
    base.py
    registry.py

    solana/
      __init__.py
      chain.py

    evm/
      __init__.py
      domain.py
      rpc.py
      websocket.py
      mempool.py
      blocks.py
      receipts.py
      nonce.py
      gas.py

    bsc/
      __init__.py
      chain.py
      feeds.py
      fees.py
      execution.py
      builders.py
      units.py

  venues/
    __init__.py
    base.py
    registry.py

    pumpfun/
      __init__.py
      venue.py

    flap/
      __init__.py
      venue.py
      contracts.py
      pending.py
      decoder.py
      reducer.py
      quote.py
      fees.py
      execution.py

    fourmeme/
      __init__.py
      venue.py
      contracts.py
      pending.py
      decoder.py
      reducer.py
      quote.py
      fees.py
      execution.py

  runtime/
    __init__.py
    assembly.py
    profiles.py
```

Do not move all legacy files in one commit. First wrap them.

---

# 4. Exact interfaces to add

## 4.1 `darkbot/chains/base.py`

```python
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol, Sequence


@dataclass(frozen=True, slots=True)
class ChainIdentity:
    chain_id: str
    family: str
    native_symbol: str
    native_decimals: int


class ChainAdapter(Protocol):
    identity: ChainIdentity

    def build_feed_adapters(
        self,
        *,
        cfg: Any,
        clock: Any,
        on_raw: Any,
        on_health: Any,
        on_error: Any,
    ) -> Sequence[Any]:
        ...

    def build_fee_resolver(self, *, cfg: Any, clock: Any, run_id: str) -> Any:
        ...

    def block_ref_from_raw(self, raw: Any) -> str | int | None:
        ...

    def tx_id_from_raw(self, raw: Any) -> str | None:
        ...
```

Do not add venue logic here.

## 4.2 `darkbot/venues/base.py`

```python
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Protocol


@dataclass(frozen=True, slots=True)
class VenueIdentity:
    venue_id: str
    chain_id: str
    display_name: str


class VenueAdapter(Protocol):
    identity: VenueIdentity

    def build_decoder(self, *, cfg: Any) -> Any:
        ...

    def build_reducer(self, *, cfg: Any, run_id: str) -> Any:
        ...

    def build_quote_engine(self, *, cfg: Any) -> Any:
        ...

    def is_actionable_launch(self, state: Any) -> bool:
        ...

    def creator_of(self, state: Any) -> str | None:
        ...
```

## 4.3 Runtime profile

`darkbot/runtime/profiles.py`

```python
from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class MarketProfile:
    profile_id: str
    chain_id: str
    venue_id: str
    display_name: str
    status: str
```

Initial registry:

```python
MARKET_PROFILES = {
    "solana-mainnet:pumpfun": MarketProfile(
        profile_id="solana-mainnet:pumpfun",
        chain_id="solana-mainnet",
        venue_id="pumpfun",
        display_name="Solana / Pump.fun",
        status="READY",
    ),
    "bsc-mainnet:flap": MarketProfile(
        profile_id="bsc-mainnet:flap",
        chain_id="bsc-mainnet",
        venue_id="flap",
        display_name="BSC / Flap.sh",
        status="RESEARCH",
    ),
    "bsc-mainnet:fourmeme": MarketProfile(
        profile_id="bsc-mainnet:fourmeme",
        chain_id="bsc-mainnet",
        venue_id="fourmeme",
        display_name="BSC / Four.meme",
        status="RESEARCH",
    ),
}
```

---

# 5. Runtime assembly

Create `darkbot/runtime/assembly.py`.

```python
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True, slots=True)
class RuntimeComponents:
    market_profile: Any
    chain: Any
    venue: Any
    decoder: Any
    reducer: Any
    quote_engine: Any
    fee_resolver: Any
    feeds: tuple[Any, ...]


def assemble_runtime(*, cfg, clock, run_id, callbacks) -> RuntimeComponents:
    profile = load_market_profile(cfg.market_profile_id)
    chain = load_chain(profile.chain_id)
    venue = load_venue(profile.venue_id)

    if venue.identity.chain_id != chain.identity.chain_id:
        raise ValueError(
            f"venue/chain mismatch: {venue.identity.venue_id} "
            f"requires {venue.identity.chain_id}, got {chain.identity.chain_id}"
        )

    decoder = venue.build_decoder(cfg=cfg)
    reducer = venue.build_reducer(cfg=cfg, run_id=run_id)
    quote_engine = venue.build_quote_engine(cfg=cfg)
    fee_resolver = chain.build_fee_resolver(
        cfg=cfg,
        clock=clock,
        run_id=run_id,
    )
    feeds = tuple(
        chain.build_feed_adapters(
            cfg=cfg,
            clock=clock,
            on_raw=callbacks.on_raw,
            on_health=callbacks.on_health,
            on_error=callbacks.on_error,
        )
    )

    return RuntimeComponents(
        market_profile=profile,
        chain=chain,
        venue=venue,
        decoder=decoder,
        reducer=reducer,
        quote_engine=quote_engine,
        fee_resolver=fee_resolver,
        feeds=feeds,
    )
```

Adapt names to actual repository conventions, but preserve the separation.

### 5.1 Assembly scope: do not flatten replay-specific injection

The generic assembly example is for normal runtime component selection. It does **not** replace specialized replay construction.

The existing replay path already injects replay-specific behavior into `StateOwner` and must keep doing so. In particular, preserve the current semantics of:

- `StateOwner(..., engine=...)`;
- replay-created quote engine with stress parameters;
- `canonical_prefix`;
- recorded/source fee snapshots and source-policy binding;
- `persist_observations`;
- `compute_features`;
- `mint_allowlist` / replay filters;
- callback-lateness handling;
- `execution_delay_provider`;
- virtual replay clock/offline scheduling;
- source dataset integrity checks.

Do not force replay through a generic live/capture assembly path if doing so changes any of these semantics. Shared factories may be used only where they preserve the exact existing replay contract.

Replay remains fully offline and must never require a live feed, live RPC, provider connection, signing key, or broadcaster.

---

# 6. Refactor `StateOwner` by dependency injection

Current problem in `darkbot/paper.py`:

- it constructs `PumpDecoder` directly;
- it constructs `MarketReducer` directly;
- it assumes SOL/Pump semantics.

Change the constructor additively while preserving the existing `engine` injection:

```python
class StateOwner:
    def __init__(
        self,
        ...,
        engine=None,          # EXISTING injection; do not replace it
        decoder=None,         # additive
        reducer=None,         # additive
        market_profile=None,  # additive metadata
    ):
        ...
```

During migration, preserve the **existing** replay injection seam. The current code already accepts `engine=...`; do not replace or shadow it with a second competing parameter.

A compatible shape is conceptually:

```python
class StateOwner:
    def __init__(
        self,
        ...,
        engine=None,          # EXISTING replay injection — preserve semantics
        decoder=None,         # additive
        reducer=None,         # additive
        market_profile=None,  # additive metadata
        canonical_prefix=None,
        persist_observations=True,
        compute_features=True,
        mint_allowlist=None,
        ...
    ):
        self.decoder = decoder if decoder is not None else PumpDecoder(cfg.protocol)
        self.reducer = reducer if reducer is not None else MarketReducer(...)
        self.engine = engine if engine is not None else QuoteEngine(...)
```

The exact signature must follow the real repository, not this abbreviated example. The key rule is that existing arguments and behavior remain intact.

Temporary Pump/Solana fallback construction is allowed to preserve legacy callers during Phase A.

Do not change Strategy C semantics in this step.

### 6.1 Legacy config and hash compatibility

Phase A must not retroactively alter existing Solana config hashes.

For legacy configs with no explicit `market_profile_id`:

```
effective market profile = solana-mainnet:pumpfun
```

but the default must be interpreted **outside the canonical raw JSON used for the legacy config hash**.

Do **not** implement this by mutating the legacy raw config with:

```python
raw.setdefault("market_profile_id", "solana-mainnet:pumpfun")
```

before `canonical_json(raw)`, because that would change historical config hashes.

Requirements:

- old config file + old code semantics => same existing config hash after Phase A;
- explicit new profile metadata may participate in hashes for newly authored multichain configs;
- absent profile remains backward-compatible Solana/Pump;
- existing replay/source config foreign-key/evidence bindings must remain valid.

---

# 6.2 Phase-C mandatory EVM/BSC semantics — no alternative interpretation

The following rules are mandatory for Phase C and override any older/example wording elsewhere in the repository:

## 6.2.1 Same sender + nonce means competing candidates, not immediate replacement

For EVM/BSC, different hashes observed for the same `(chain_id, sender, nonce)` are **competing nonce candidates**.

Observation order alone MUST NOT create a definitive `old -> replaced_by -> new` conclusion.

The first observed hash may still be the one that becomes canonical.

A definitive replacement/drop conclusion requires the documented canonical/finality resolution rule. All hashes and all provider observations are retained.

Tests must include:

- A observed, then B observed, A lands;
- A observed, then B observed, B lands;
- neither lands before run end;
- a previously included candidate is removed by reorg and the replacement branch changes the resolved outcome.

## 6.2.2 Canonical reconnect requires deterministic backfill

WebSocket reconnect does not restore missed canonical history.

The BSC chain layer must retain a last-completely-processed canonical high-water mark with block-hash evidence. After reconnect or restart it must:

1. verify the provider chain identity;
2. obtain current canonical head;
3. fetch all missing blocks from the local high-water mark through current head;
4. fetch required transactions/logs/receipts;
5. verify hash/parent-hash continuity;
6. process recovered data through the same normal canonical/reorg path;
7. mark canonical coverage recovered only after successful catch-up.

Backfill must be idempotent. Late/old heads must not silently roll accepted canonical state backwards.

Pending/mempool history lost during disconnect is different: it normally cannot be reconstructed. Persist a pending-coverage gap; never fabricate recovered pending evidence.

## 6.2.3 Every BSC endpoint must prove chain id 56

Each configured BSC HTTP and WebSocket connection must call/verify `eth_chainId` at initial connection and after a new underlying connection/reconnect.

Required result:

```
0x38 == 56
```

A mismatching endpoint is unhealthy and contributes no pending or canonical evidence.

## 6.2.4 Hash-only pending enrichment is bounded and asynchronous

A hash-only pending notification must never trigger a synchronous `eth_getTransactionByHash` inside the receive callback.

Use a bounded asynchronous enrichment mechanism with:

- bounded queue;
- bounded in-flight RPC count;
- tx-hash deduplication;
- bounded retained pending/enrichment state;
- explicit overflow/overload/failure quality states;
- no silent evidence loss.

Persist `first_seen_mono_ns` separately from enrichment request/completion timestamps. Enrichment time never replaces first-seen time.

## 6.2.5 Chain transaction identity and provider observation are separate

Do not duplicate one blockchain transaction because multiple providers observed it.

Persist conceptually:

- chain transaction identity: `(run_id, chain_id, tx_hash)`;
- provider observation identity: `(run_id, provider_id, tx_hash)`.

Provider observations retain capability, first/last seen, raw evidence/integrity reference and enrichment timing. Chain transaction evidence retains exact transaction content and canonical associations.

## 6.2.6 Canonicality changes are auditable

Block hash, not block number alone, identifies a branch member.

A reorg must preserve orphaned raw evidence and preserve an audit trail/equivalent immutable evidence of canonicality transitions. Transaction/log/receipt canonical associations remain reversible until the configured finality rule resolves them.

If a retained common ancestor cannot be determined, canonical certainty becomes an explicit quality gap; do not invent state.

## 6.2.7 DROPPED/REPLACED require an explicit finalization rule

Do not use timeout alone as a definitive `DROPPED` rule.

At run end, unresolved mempool observations remain `PENDING_ONLY` or `UNKNOWN` unless explicit evidence supports a stronger state.

`REPLACED` is not final merely because another same-nonce hash was observed.

The exact finalization rule must be documented and deterministically tested.

## 6.2.8 Integrated Phase-C capture-path test is mandatory

Phase C is not accepted on isolated unit tests alone.

At least one deterministic integrated test must cover:

```
Fake WSS/RPC
-> BSC provider/chain layer
-> EVM pending + hash-only enrichment
-> independent canonical processing
-> disconnect/reconnect + missing-block backfill
-> short reorg
-> shared dispatcher with synthetic recognizers
-> Recorder
-> SqliteWriter
-> SQLite readback
-> orderly shutdown
```

It must verify bounded queues, exact chain-id handshake, no duplicated chain transaction evidence, independent provider observations, canonical/orphan transitions and persistence readback.

Do not use Pump components as fake BSC components. Do not create real Flap/Four recognizers for this test.

## 6.2.9 Phase-C performance scope

Phase C measures only the read-only capture/dispatch foundation:

- pending receive -> generic parse complete;
- pending receive -> full tx available (hash-only providers, reported separately);
- pending receive -> dispatcher complete;
- dispatcher -> recorder enqueue;
- writer queue wait separately;
- enrichment/writer queue depth and overflow.

The later hard `T_PENDING_SEEN -> T_SOCKET_HANDOFF <= 3 ms` LIVE contract cannot be proven in Phase C because build/sign/send do not exist yet. Phase C must not claim that it has proven that final 3 ms contract.

# 7. Generic domain additions

Add fields **without deleting existing Solana fields**.

## 7.1 Generic asset identity

Where practical, add:

```
asset_id: str | None
chain_id: str | None
venue_id: str | None
tx_id: str | None
block_ref: str | int | None
native_amount_base_units: int | None
```

Compatibility mapping for Solana:

```
asset_id = mint
tx_id = signature
block_ref = slot
native_amount_base_units = sol_amount
```

Do not rename database columns yet.

## 7.2 EVM-specific raw envelope

Create `darkbot/chains/evm/domain.py`.

```python
from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class PendingTxEnvelope:
    chain_id: str
    provider_id: str
    tx_hash: str
    from_address: str
    to_address: str | None
    nonce: int
    value_wei: int
    input_hex: str
    gas: int | None
    gas_price_wei: int | None
    max_fee_per_gas_wei: int | None
    max_priority_fee_per_gas_wei: int | None
    receive_mono_ns: int


@dataclass(frozen=True, slots=True)
class CanonicalLogEnvelope:
    chain_id: str
    provider_id: str
    block_number: int
    block_hash: str
    tx_hash: str
    tx_index: int
    log_index: int
    address: str
    topics: tuple[str, ...]
    data_hex: str
    receive_mono_ns: int


@dataclass(frozen=True, slots=True)
class ReceiptEnvelope:
    chain_id: str
    provider_id: str
    tx_hash: str
    block_number: int
    block_hash: str
    tx_index: int
    status: int
    gas_used: int
    effective_gas_price_wei: int
    receive_mono_ns: int
```

These objects contain no Flap/Four logic.

---

# 8. BSC multi-venue dispatch

Create one dispatcher for capture mode.

```python
class VenueRecognizer(Protocol):
    venue_id: str

    def maybe_matches_pending(self, tx: PendingTxEnvelope) -> bool:
        ...

    def maybe_matches_log(self, log: CanonicalLogEnvelope) -> bool:
        ...


class BscVenueDispatcher:
    def __init__(self, recognizers):
        self._recognizers = tuple(recognizers)

    def pending_targets(self, tx):
        return tuple(
            r for r in self._recognizers
            if r.maybe_matches_pending(tx)
        )

    def log_targets(self, log):
        return tuple(
            r for r in self._recognizers
            if r.maybe_matches_log(log)
        )
```

Hot-path rule:

```
address + 4-byte selector/topic check
-> full decode only after match
```

Forbidden in the hot-path recognizer:

- HTTP
- DB query
- filesystem
- metadata fetch
- JSON reserialization
- logging large payloads

---

# 9. Flap package contract

## 9.1 `contracts.py`

This file must initially contain **no invented addresses**.

Use a typed structure:

```python
@dataclass(frozen=True, slots=True)
class FlapContracts:
    portal: str
    portal_version: str
    abi_hash: str
```

Values come from config snapshots after verification.

## 9.2 `pending.py`

```python
@dataclass(frozen=True, slots=True)
class FlapPendingLaunch:
    tx_hash: str
    creator: str
    predicted_asset_id: str | None
    quote_asset_id: str | None
    first_seen_mono_ns: int
    launch_params: dict
    verification_status: str
```

Rules:

- only set `predicted_asset_id` if deterministic derivation is proven;
- otherwise `None`;
- unknown mode => `verification_status="UNSUPPORTED_OR_UNKNOWN"`.

## 9.3 `decoder.py`

Separate methods:

```python
class FlapDecoder:
    def decode_pending(self, tx: PendingTxEnvelope): ...
    def decode_log(self, log: CanonicalLogEnvelope): ...
```

Never use Four.meme ABI here.

## 9.4 `quote.py` — POST-PHASE-F ECONOMICS ONLY

This subsection defines a future target interface. **Do not implement it in Phase D/M7.** Flap Phase D is read-only decoding only. Quote/fee/economic implementation starts only after Phase F/M9 real capture and the post-capture economics gate.

```python
class FlapQuoteEngine:
    def quote_buy(...): ...
    def quote_sell(...): ...
    def verify_against_eth_call(...): ...
```

No approximate floating-point math.

Use integer arithmetic.

If the local result disagrees with verified `eth_call`, return invalid/unknown and block qualification.

---

# 10. Four.meme package contract

Same isolation pattern.

## 10.1 `contracts.py`

```python
@dataclass(frozen=True, slots=True)
class FourMemeContracts:
    token_manager_v1: str | None
    token_manager_v2: str
    helper: str | None
    abi_hashes: dict[str, str]
```

Again: values only after verification.

## 10.2 `pending.py`

```python
@dataclass(frozen=True, slots=True)
class FourMemePendingLaunch:
    tx_hash: str
    creator: str
    predicted_asset_id: str | None
    mode: str | None
    first_seen_mono_ns: int
    launch_params: dict
    verification_status: str
```

## 10.3 `decoder.py`

```python
class FourMemeDecoder:
    def decode_pending(self, tx: PendingTxEnvelope): ...
    def decode_log(self, log: CanonicalLogEnvelope): ...
```

Must distinguish supported modes explicitly.

Never coerce unknown mode to "normal".

## 10.4 `quote.py` — POST-PHASE-F ECONOMICS ONLY

This subsection defines a future target interface. **Do not implement it in Phase E/M8.** Four.meme Phase E is read-only decoding only. Quote/fee/economic implementation starts only after Phase F/M9 real capture and the post-capture economics gate.

```python
class FourMemeQuoteEngine:
    def quote_buy(...): ...
    def quote_sell(...): ...
    def verify_against_helper(...): ...
```

Quote verification must include mode-specific behavior where applicable.

---

# 11. Generic execution plan

Venue packages build calldata; chain package signs/sends.

Create a generic object:

```python
@dataclass(frozen=True, slots=True)
class ExecutionPlan:
    chain_id: str
    venue_id: str
    asset_id: str
    side: str
    to_address: str
    value_native: int
    calldata_hex: str
    gas_limit_hint: int | None
    min_output_base_units: int | None
    max_input_native: int | None
    metadata: dict
```

Example architecture:

```
FlapQuote/Execution
      |
      v
ExecutionPlan
      |
      v
BscExecutionAdapter
      |
      +--> signer
      +--> nonce manager
      +--> gas policy
      +--> sender
```

Four.meme uses the same generic `ExecutionPlan` type but constructs different calldata.

---

# 12. BSC sender architecture

Do not implement live broadcasting in the first BSC milestones.

Define interfaces only:

```python
class BscSender(Protocol):
    async def submit(self, signed_tx: bytes, context: dict) -> Any:
        ...


class BscBuilderProvider(Protocol):
    async def submit_private_tx(self, signed_tx: bytes, context: dict) -> Any:
        ...

    async def submit_bundle(self, signed_txs: tuple[bytes, ...], context: dict) -> Any:
        ...
```

Future implementations:

```
PublicRpcSender
LocalNodeSender
BuilderRelaySender
```

Do not bind Strategy C to a particular provider.

---

# 13. Nonce manager requirements

Create a BSC nonce manager before LIVE.

Required invariants:

1. one nonce may belong to only one logical attempt at a time;
2. replacement transactions retain attempt linkage;
3. restart can reconcile local vs chain pending nonce;
4. dropped tx can be marked;
5. no silent nonce reuse;
6. no DB write on critical signing path if avoidable;
7. all replacement relationships persisted.

Suggested states:

```
RESERVED
SUBMITTED
REPLACED
INCLUDED
REVERTED
DROPPED
UNKNOWN
```

---

# 14. BSC pending lifecycle

Required pending/current-resolution concepts include:

```
PENDING
PENDING_ONLY
INCLUDED_SUCCESS
INCLUDED_REVERTED
REPLACED
DROPPED
UNKNOWN
```

The states are not assigned from observation order alone.

For the same `(chain_id, sender, nonce)`, every distinct hash remains a competing candidate until canonical/finality evidence resolves the competition. A later-observed hash does not automatically replace an earlier-observed hash.

`INCLUDED_SUCCESS` / `INCLUDED_REVERTED` are provisional while their block can still be orphaned under the configured finality policy.

`REPLACED` and `DROPPED` require the explicit evidence-based finalization rule defined and tested by the BSC chain implementation. Timeout alone is insufficient.

At run end, unresolved pending evidence is `PENDING_ONLY` or `UNKNOWN`, not silently `DROPPED`.

For future launch transactions, venue lifecycle may later include:

```
PENDING_CREATE
CREATED_ACTIVE
CREATE_REVERTED
CREATE_DROPPED
CREATE_REPLACED
COMPLETED
MIGRATED
```

Those venue states are not implemented in Phase C because no real Flap/Four recognizer exists yet.

Do not equate `pending seen` with `launch exists`.

---

# 15. Storage additions

Use additive migrations only.

## 15.1 `runs`

Add nullable fields first:

```
market_profile_id
chain_id
venue_id
native_symbol
native_decimals
```

Legacy runs remain valid.

## 15.2 EVM/BSC chain transaction + provider observation storage

The earlier one-table example keyed by `(run_id, provider_id, tx_hash)` is superseded because it duplicates blockchain transaction identity across providers.

The Phase-C schema must separate two concepts.

### Chain transaction evidence

One logical transaction per:

```
(run_id, chain_id, tx_hash)
```

Store exact/lossless transaction content, sender, recipient, nonce, value, calldata/gas fields and current canonical association/status.

### Provider pending observation

One observation relationship per conceptually:

```
(run_id, provider_id, tx_hash)
```

Store:

- provider capability;
- first_seen_mono_ns;
- last_seen_mono_ns;
- raw payload or lossless integrity reference;
- enrichment_requested_mono_ns where applicable;
- full_tx_available_mono_ns where applicable;
- enrichment status/error;
- pending-gap/quality linkage where applicable.

The exact table names and foreign keys should follow DarkBot conventions, but these two identities MUST remain separate.

Use TEXT or another verified exact representation for values that may exceed signed SQLite INTEGER range. Never float.

Also persist blocks, canonicality transitions, canonical transaction associations, logs and receipts so that reorg changes can be reconstructed without deleting old evidence.

## 15.3 New `pending_launches` — FUTURE VENUE STORAGE

`pending_launches` is a venue-derived table for Phase D/E/F after real recognizers exist. It is **not required for Phase C synthetic dispatcher operation**. If introduced later, it references the chain transaction evidence instead of duplicating raw transaction content.

```sql
CREATE TABLE pending_launches (
    run_id TEXT NOT NULL,
    venue_id TEXT NOT NULL,
    tx_hash TEXT NOT NULL,
    creator TEXT NOT NULL,
    predicted_asset_id TEXT,
    first_seen_mono_ns INTEGER NOT NULL,
    decoded_mode TEXT,
    launch_params_json TEXT NOT NULL,
    verification_status TEXT NOT NULL,
    canonical_asset_id TEXT,
    canonical_create_event_id TEXT,
    create_inclusion_observed_mono_ns INTEGER,
    pending_lead_observed_ns INTEGER,
    PRIMARY KEY (run_id, venue_id, tx_hash)
);
```

---

# 16. BSC fee model

Do not reuse Solana fee objects by changing field labels only.

Create generic economic concepts and BSC-specific snapshots.

Required actual network cost:

```
gas_cost_wei = gas_used * effective_gas_price_wei
```

Additionally track:

- relay/builder tip
- reverted tx gas
- replacement tx gas
- approval gas
- protocol fee
- token tax
- slippage

A reverted BSC transaction is an economic loss.

Never classify failed gas cost as zero.

---

# 17. Strategy C

Do not alter:

```
LEGACY_BACKEND_FILTER_V1
```

Add:

```
STRATEGY_C_MULTICHAIN_V1
```

First BSC research profile:

```json
{
  "strategy_id": "STRATEGY_C_MULTICHAIN_V1",
  "observation_ms": 0,
  "filters": [],
  "entry_trigger": "FIRST_ACTIONABLE_LAUNCH",
  "creator_sell_exit_enabled": true,
  "ordinary_stop_enabled": false,
  "target_exit_enabled": true,
  "entry_reentry_policy": "ONE_SUBMITTED_ENTRY_PER_ASSET_PER_CAMPAIGN"
}
```

Do not hard-code target/timeout in the strategy class. Those remain config/research variables.

---

# 18. Unified timing and ordering semantics

For BSC, distinguish **local observation time** from **on-chain ordering**.

Required local monotonic timestamps:

```
pending_first_seen_mono_ns
decode_done_mono_ns
decision_done_mono_ns
intent_enqueued_mono_ns
submit_mono_ns
rpc_ack_mono_ns
create_inclusion_observed_mono_ns
buy_inclusion_observed_mono_ns
receipt_seen_mono_ns
```

Do **not** name a local arrival timestamp `create_execution_time` or imply that it is the exact EVM execution instant.

Derived local-observation latencies:

```
pending_to_decode_ns
pending_to_decision_ns
pending_to_enqueue_ns
pending_to_submit_ns
pending_to_create_inclusion_observed_ns
pending_to_buy_inclusion_observed_ns
create_observed_to_buy_observed_ns
```

These values include provider/network observation delay and are not exact on-chain execution intervals.

Persist canonical ordering evidence separately:

```
create_block_number
create_block_hash
create_tx_index
create_log_index

buy_block_number
buy_block_hash
buy_tx_index
buy_log_index
```

For same-block analysis, ordering is valid only when both transactions belong to the same block hash that is currently on the accepted canonical chain.

For different blocks, compare canonical ancestry/block order, not arrival timestamps.

At minimum, the chain layer must retain enough evidence to answer:

- was CREATE on the accepted canonical chain?
- was BUY on the accepted canonical chain?
- were they in the same canonical block hash?
- if yes, was `buy_tx_index > create_tx_index`?
- if a reorg removed either block, was the prior conclusion rolled back?

Block number and transaction indices **alone are insufficient** because a reorg can replace a block at the same height.

Milliseconds remain useful for local-system/provider performance, while block hash + canonical-chain membership + tx/log indices establish actual chain ordering.

---


# 18.1 Mandatory ultra-low-latency performance contract

The architectural objective is **as close to 0 ns of avoidable local delay as practical**. Zero is an optimization direction, not a physically achievable measured value.

For the later BSC hot path, the hard local reaction deadline is:

```
3,000,000 ns = 3.000 ms
```

measured from:

```
T_PENDING_SEEN
    =
first byte-complete / parseable relevant pending notification
received by DarkBot on the selected hot feed
```

to:

```
T_SOCKET_HANDOFF
    =
the fully built and signed BUY transaction has been handed to the
already-open network transport / sender implementation for transmission
```

This is deliberately stricter than "decision -> enqueue". Queue handoff alone is not success.

Required sub-stage timestamps:

```
pending_first_seen_mono_ns
decode_done_mono_ns
decision_done_mono_ns
intent_enqueued_mono_ns
queue_dequeued_mono_ns
tx_build_start_mono_ns
tx_build_done_mono_ns
sign_start_mono_ns
sign_done_mono_ns
socket_handoff_mono_ns
rpc_ack_mono_ns
```

Required derived local metrics:

```
pending_to_decode_ns
pending_to_decision_ns
decision_to_enqueue_ns
enqueue_queue_wait_ns
queue_dequeue_to_build_done_ns
sign_ns
build_and_sign_ns
pending_to_socket_handoff_ns
socket_handoff_to_ack_ns
```

## 18.1.1 Hard 3 ms entry deadline

For future BSC LIVE entry execution:

- if `pending_to_socket_handoff_ns <= 3_000_000`, the local deadline is met;
- if the deadline is already exceeded **before network handoff**, do not send a new entry;
- record the event as `LOCAL_ENTRY_DEADLINE_EXCEEDED`;
- deadline violations remain in all statistics and denominators;
- never delete or hide slow samples;
- exits/risk-reduction actions must **not** be suppressed by this entry deadline.

This rule guarantees that DarkBot does not knowingly originate a new BSC entry after its own 3 ms local budget has already expired. It does **not** guarantee 3 ms to on-chain inclusion.

## 18.1.2 Hot path rules — apply through network handoff

From `T_PENDING_SEEN` through `T_SOCKET_HANDOFF`, there must be no avoidable waiting work:

Forbidden:

- SQLite/database reads or writes;
- filesystem access;
- metadata/network lookups;
- RPC quote verification;
- REST calls;
- DNS lookup on each trade;
- opening a new HTTP/TCP/TLS/WebSocket connection per trade;
- configuration parsing;
- registry/profile lookup;
- ABI loading/parsing from disk;
- JSON reserialization for persistence/logging;
- synchronous logging;
- waiting for the writer;
- process/task hops introduced only by the adapter architecture;
- sleeps/backoff;
- fetching nonce/gas/protocol constants on demand when they can be prepared ahead of time.

Required:

- chain/venue/profile resolution once at startup;
- direct references to already-created adapter objects on the hot path;
- pre-opened/warmed sender connections;
- preloaded verified ABI/selectors/contracts;
- locally cached, freshness-gated nonce/gas/protocol state;
- precomputed transaction templates/static calldata parts where possible;
- local deterministic quote/fee calculation only;
- non-blocking telemetry/write queue;
- minimal allocations/copies;
- explicit failure/skip if required verified state is missing or stale.

Correctness wins over speed: missing required state means **no entry**, not a blocking fetch inside the hot path.

## 18.1.3 Phase-A performance preservation gate

Replay parity alone is insufficient because replay virtual time does not expose additional CPU latency.

Before the first Phase-A code change, run a real wall-clock processing benchmark on the same host/environment used for the post-change test.

The benchmark must exercise the existing real processing path with representative recorded input and the normal writer/telemetry load. Measure at least:

```
receive -> decode
receive -> decision
decision -> enqueue
receive -> enqueue
queue wait
```

Record raw samples and:

```
min / p50 / p95 / p99 / max / mean
sample count
CPU/platform/Python version
writer enabled state
input dataset/hash
exact benchmark command
```

After Phase A, rerun the identical benchmark on the same hardware/environment.

Acceptance:

- no reproducible latency regression attributable to the adapter layer;
- no new queue/task/process/serialization boundary in the hot path merely because adapters were introduced;
- `receive -> enqueue` p99 target remains **< 3.000 ms**;
- any reproducible p50/p95/p99 degradation must be explained and corrected before Phase A is accepted.

The historical Solana measurement (`receive -> enqueue` p50 about 0.539 ms, p99 about 2.203 ms) is context, not the Phase-A oracle; the fresh before/after benchmark is authoritative.

## 18.1.4 Later BSC performance acceptance

When BSC Flap and Four.meme are implemented, benchmark **both enabled simultaneously**, because production capture/recognition may run both recognizers.

Report for each venue and combined load:

```
p50 / p95 / p99 / max
deadline violation count
deadline violation percentage
```

for:

- pending -> recognized;
- pending -> decoded;
- pending -> decision;
- decision -> enqueue;
- queue wait;
- build;
- sign;
- pending -> socket handoff;
- socket handoff -> ACK.

The primary local entry performance gate is `pending -> socket handoff <= 3 ms`.

The optimization target remains **minimum possible latency toward 0 ms**, but no result may be described as "0 ms" unless the measured interval is actually zero at the timer's resolution.

## 18.1.5 Feed/sender path selection is empirical

Do not assume that:

- a local BSC node is always the fastest feed;
- a paid provider is always faster;
- the geographically closest endpoint is best;
- a builder always gives better placement;
- ACK implies inclusion or correct ordering.

Compare candidate paths using synchronized local first-seen/send timestamps and canonical-chain outcomes.

Feed comparison must measure:

- complete pending-data availability;
- first-seen lead;
- missing-transaction rate;
- p50/p95/p99 jitter;
- disconnect/gap behavior.

Sender/builder comparison must measure:

- socket handoff -> ACK;
- canonical inclusion rate;
- same-block CREATE -> BUY ordering;
- transaction index distance behind CREATE;
- revert/failure rate;
- actual gas/tip/relay cost.

Select the production path from measured results, not marketing claims.

---

# 19. Combined BSC 30-minute capture

This is the first real BSC milestone.

The capture mode must allow:

```json
{
  "chain": "bsc-mainnet",
  "venues": ["flap", "fourmeme"],
  "capture_only": true
}
```

Only one BSC network ingest is required for pending traffic, and both recognizers must consume the same original local receive timestamp.

However, the dataset must contain **two independent evidence paths**:

### 19.1 Pending-path evidence

Purpose: measure what DarkBot could know before canonical inclusion.

Persist the complete raw pending transaction evidence needed to reproduce decoding, including at minimum:

- provider id;
- full tx hash;
- sender;
- destination;
- nonce;
- value;
- calldata/input;
- gas fields;
- first-seen local monotonic timestamp;
- replacement relationship;
- raw provider payload or a lossless canonical raw representation plus integrity hash.

Do not store only the decoded launch object.

### 19.2 Independent canonical launch census

Purpose: establish the denominator of all launches that actually occurred, including launches never seen in the public pending feed.

Independently scan canonical BSC blocks/logs/receipts for Flap and Four.meme launch events.

The canonical census must not depend on a launch having first existed in `pending_launches`.

This makes the coverage definition:

```
pending_coverage =
canonical launches seen in pending before inclusion
/
all canonical launches in the capture window
```

A pending-only denominator is invalid.

Persist enough raw canonical data to reproduce the census:

- block number;
- block hash;
- parent hash;
- transaction hash;
- transaction index;
- transaction input/value where required;
- raw logs/topics/data;
- log index;
- receipt status;
- gas used/effective gas price;
- local observation timestamps;
- integrity hashes/raw payloads as appropriate.

### 19.3 Reorg contract

Canonicality is mutable until the configured confirmation/finality policy is satisfied.

The capture layer must:

1. track block hash and parent hash;
2. detect canonical-head replacement/reorg;
3. mark events/receipts from orphaned blocks non-canonical;
4. roll back/reconcile derived launch/trade state;
5. accept the replacement canonical block/events;
6. retain both the original raw evidence and the canonicality transition for audit.

Do not delete orphaned raw evidence. Mark it.

### 19.4 Required reports

Per venue:

```
canonical_launches_total
pending_seen_before_inclusion
pending_coverage_pct
predicted_asset_id_available_pct
pending_lead_observed_ms_min
pending_lead_observed_ms_p50
pending_lead_observed_ms_p95
pending_lead_observed_ms_p99
pending_lead_observed_ms_max
reverted_creates
replaced_creates
dropped_pending_creates
orphaned_create_observations
first_external_buy_observed_delay
same_block_first_buy_count
trades_0_100ms_observed
trades_0_250ms_observed
trades_0_500ms_observed
trades_0_750ms_observed
trades_0_1000ms_observed
creator_sell_count
decode_unknown_count
unsupported_mode_count
raw_evidence_integrity_failures
canonical_census_gaps
```

Any millisecond `*_observed` metric is a local/provider observation metric, not an exact EVM execution interval.

No economic conclusion before quote/fee verification and canonical-census quality checks pass.

---

# 20. Replay matrix after economics are verified

For each venue independently:

## Entry delay

```
0
1
2
3
5
10
25
50
100
150
200
300 ms
```

## Target

```
1.5x
2.0x
3.0x
3.5x
```

## Timeout

```
250
500
750
1000
2000 ms
```

## Execution model

```
POST_CREATE
PENDING_REACTIVE
ORDERED_BACKRUN
```

Required outputs:

```
eligible
attempts
fills
reverts
unresolved
targets
creator_exits
timeouts
realized_pnl
pessimistic_pnl
ev_per_resolved
ev_per_attempt
profit_factor
win_rate
gas_total
protocol_fee_total
tax_total
relay_tip_total
top1_pnl_share
top3_pnl_share
top5_pnl_share
chronological_third_1
chronological_third_2
chronological_third_3
```

---

# 21. Test plan

## 21.1 Regression and exact Solana parity gate

Phase A has a strict before/after gate.

Before source changes:

1. freeze the actual full `git rev-parse HEAD`;
2. run and store the complete existing test-suite result;
3. run and store the strict production Solana replay baseline defined in Section 0.5.

After Phase A:

1. run the same complete test command;
2. run the exact same replay command/runner against the same source dataset;
3. compare using the semantic mapping defined in Section 0.5;
4. require exact integer equality for lamport-denominated economics.

Required equalities include:

- event semantics;
- decisions/actions/reasons;
- attempts/statuses;
- fills;
- selected fee evidence and fee values;
- ledger economics;
- position outcomes;
- exact realized PnL lamports;
- exact pessimistic PnL lamports;
- unresolved exposure/cost lamports.

Diagnostic legacy-compatible monkeypatch runs are separate optional checks and are not the production oracle.

## 21.2 New core tests

Create:

```
tests/test_chain_registry.py
tests/test_venue_registry.py
tests/test_runtime_assembly.py
tests/test_market_profiles.py
tests/test_multichain_domain.py
```

Test:

- unknown chain rejected
- unknown venue rejected
- venue/chain mismatch rejected
- config hash changes when market profile changes
- Solana profile resolves executable Solana + Pump components
- Flap profile resolves **metadata only** to `bsc-mainnet:flap` and runtime construction fails explicitly in Phase A
- Four profile resolves **metadata only** to `bsc-mainnet:fourmeme` and runtime construction fails explicitly in Phase A

## 21.3 Phase-C EVM/BSC tests

Create focused tests following repository naming conventions, covering at minimum:

- full-object pending decode;
- hash-only pending decode;
- malformed/error JSON-RPC;
- exact hex/integer parsing and uint256 round-trip;
- duplicate pending frames;
- same tx observed by multiple providers without duplicating chain transaction identity;
- bounded asynchronous hash enrichment, dedup, concurrency and overflow;
- first-seen timestamp preserved separately from full-data-available timestamp;
- correct `eth_chainId == 56` verification and rejection of mismatch;
- same-sender+nonce candidate competition where A lands;
- same-sender+nonce candidate competition where B lands;
- unresolved candidates at run end;
- success/revert receipts;
- duplicate log/receipt idempotency;
- normal canonical extension;
- disconnect/reconnect with missing-block backfill;
- backfill idempotency;
- pending gap retained despite canonical recovery;
- one-block and multi-block reorg;
- orphan/canonical transition audit;
- missing retained common ancestor -> explicit quality uncertainty;
- transaction canonical reassociation after reorg;
- shared dispatcher zero/one/multiple synthetic recognizer matches;
- original timestamp preservation;
- queue overflow visible as quality failure;
- integrated `Fake WSS/RPC -> BSC chain -> dispatcher -> Recorder/Writer -> DB readback` scenario required by §6.2.8.

The integrated test is mandatory, not optional.

## 21.4 Flap tests — Phase D only

Phase D is **read-only decoder work only**. Use verified captured/authoritative fixture data.

Required scope:

- pending CREATE recognition;
- canonical CREATE/BUY/SELL/migration recognition as supported by verified evidence;
- ABI/version fixture vectors;
- unknown versions/modes rejected;
- no quote engine;
- no fee/tax economic model;
- no execution plan;
- no signing/broadcast.

Quote/fee/eth_call economic parity tests are post-Phase-F.

## 21.5 Four.meme tests — Phase E only

Phase E is **read-only decoder work only**. Use verified captured/authoritative fixture data.

Required scope:

- pending CREATE recognition;
- canonical CREATE/BUY/SELL/liquidity recognition as supported by verified evidence;
- explicit verified mode/version decoding;
- unknown mode refusal;
- no quote engine;
- no fee/tax economic model;
- no execution plan;
- no signing/broadcast.

Helper/quote/fee economic parity tests are post-Phase-F.

---

# 22. Implementation phases for Claude

## Phase A — architecture + minimal Solana/Pump wrapper parity — IMPLEMENTED / HISTORICAL

Phase A is complete production-code history. **Do not execute this section as a task.**

Implemented contents:

- chain interfaces;
- venue interfaces;
- profile registry;
- runtime assembly;
- dependency-injection seam;
- minimal Solana chain wrapper around the existing implementation;
- minimal Pump.fun venue wrapper around the existing implementation;
- legacy config compatibility without changing existing config hashes;
- explicit non-runnable BSC profile metadata.

Preserved constraints:

- existing Solana/Pump classes/functions remain the semantic source;
- existing `StateOwner(engine=...)` replay injection and replay-specific parameters remain preserved;
- no Phase-A BSC networking or real Flap/Four implementation was introduced;
- BSC/Flap and BSC/Four profiles remain fail-closed metadata until later phases.

Historical Phase-A evidence is retained under `docs/phase_a/` and is frozen. The missing strict new real-capture Solana parity proof remains OPEN; do not restart Phase A to fabricate it.

## Phase B — optional adapter cleanup after Phase-A approval — OPTIONAL / SKIPPED FOR NOW

Detailed status/possible scope is kept in `docs/BSC_PHASE_B_OPTIONAL_CLEANUP_DRAFT.md`.

That file is explicitly DRAFT / NOT APPROVED and may be reviewed/changed before any use.

Phase B is **not** where Solana wrappers are first introduced.

Only after Phase A is independently reviewed and accepted may Phase B:

- remove temporary fallback wiring that is proven unnecessary;
- improve adapter type boundaries;
- reduce imports/coupling;
- add non-semantic cleanup.

It must still preserve exact Solana parity.

Do not start Phase B automatically.

## Phase C — EVM/BSC capture primitives — NEXT

Phase C is exactly **M4 + M5 + M6**.

Before any Phase-C edit, follow the mandatory preflight order near the top of this file: untouched baseline tests/compileall first, then maintained validation tools outside `docs/phase_a/`, then Phase-C production code.

Implement:

- generic EVM evidence/parsing/RPC/WS primitives;
- provider pending capability model;
- bounded asynchronous hash-only enrichment;
- BSC chain identity verification (`eth_chainId == 56`);
- independent canonical block/transaction/log/receipt path;
- canonical reconnect/restart backfill;
- sender+nonce competing-candidate model;
- bounded ancestry + deterministic reorg handling;
- auditable canonical/orphan transitions;
- shared BSC dispatcher with synthetic recognizer protocol;
- minimum generic/BSC capture storage with chain-transaction/provider-observation separation;
- Phase-C performance telemetry;
- mandatory integrated Fake-WSS/RPC-to-SQLite test.

Do not implement real Flap/Four recognizers.

No protocol economics.

No signing/broadcast.

Acceptance requires all §6.2 rules and §21.3 tests, the integrated test, the complete regression suite and compileall.

After Phase C: **STOP**.

## Phase D — Flap read-only decoder — PENDING

Full contract: `docs/BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md`.

Start only after Phase C has been independently reviewed/accepted and verified Flap ABI/contracts/fixtures are supplied/frozen.

Implement **read-only decoding only**:

- pending CREATE recognition;
- canonical CREATE/BUY/SELL/migration decoding supported by verified evidence;
- version/mode refusal for unknown evidence.

Do **not** implement quote math, protocol economics, fee/tax models, execution plan, signing or broadcast.

After Phase D: **STOP**.

## Phase E — Four.meme read-only decoder — PENDING

Full contract: `docs/BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md`.

Start only after Phase D review and verified Four.meme ABI/contracts/fixtures are supplied/frozen.

Implement **read-only decoding only**:

- pending CREATE recognition;
- canonical CREATE/BUY/SELL/liquidity decoding supported by verified evidence;
- explicit verified mode/version handling;
- unknown-mode refusal.

Do **not** implement quote math, protocol economics, fee/tax models, execution plan, signing or broadcast.

After Phase E: **STOP**.

## Phase F — combined 30-minute BSC capture — PENDING

Full contract: `docs/BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md`.

Mandatory post-capture economics contract: `docs/BSC_ECONOMICS_GATE_CONTRACT.md`.

Add the final capture-only CLI/config path that runs Flap + Four.meme recognizers on one shared BSC ingest/timestamp origin and an independent canonical census.

Acceptance:

- one pending/canonical chain ingest;
- same original receive timestamp distributed to both recognizers;
- independent canonical denominator;
- reconnect/backfill and reorg accounting active;
- both venue reports;
- raw evidence integrity checks;
- no signing/broadcast.

After the real Phase-F capture, analyze actual versions/modes/evidence.

Only **after Phase F** may quote math, protocol fee/tax economics and `eth_call`/helper parity be implemented and frozen. Only after that economics gate may Strategy C multichain/replay/PAPER work start.

---

# 23. How to use public/reference code

Public code is useful, but treat it as evidence, not truth.

## Allowed

- study architecture
- compare ABI signatures
- create independent test vectors
- inspect nonce patterns
- inspect mempool filtering patterns
- inspect builder abstraction patterns
- inspect quote helper usage

## Before copying source code

Record:

- repository URL
- commit SHA
- file path
- license
- exact copied/derived portion

Do not paste GPL/unknown-license code into DarkBot without explicit approval.

## Never copy

- secrets
- wallet addresses as defaults
- hardcoded private RPC endpoints
- undocumented relay credentials
- magic gas numbers
- profitability assumptions

---

# 24. Reference implementation patterns worth borrowing

## Local BSC node pattern

Target production topology later:

```
local / colocated bsc-geth
        |
        +--> IPC/WS pending tx
        +--> blocks/logs
        |
        v
DarkBot BSC ingest
        |
        +--> Flap recognizer
        +--> Four recognizer
```

A local node avoids relying on a provider that only exposes transaction hashes or delayed pending data.

Do not require a local node for the first correctness implementation.

## Hot path

```
socket frame
-> parse minimal tx envelope
-> address/selector match
-> venue pending decode
-> normalized launch signal
-> strategy decision
-> enqueue
```

Everything else is asynchronous.

## Persistence

Use existing writer queue architecture.

No SQLite call directly in receive/decode/decision hot path.

---

# 25. Explicit anti-hallucination checklist for Claude

Before creating any Flap/Four protocol constant, answer internally:

1. What exact source proves this?
2. Is it current?
3. Is it Mainnet BSC?
4. Is it the correct protocol version?
5. Do we have ABI hash/version evidence?
6. Does an on-chain transaction confirm it?

If not, use a placeholder and mark it unresolved.

Before changing Solana code:

1. Is this required for adapter injection?
2. Does it change historical semantics?
3. Can it be done as a wrapper instead?

If semantics might change, stop.

Before adding shared code:

1. Is this truly chain-general?
2. Or only EVM-general?
3. Or only BSC-general?
4. Or only venue-specific?

Put it at the narrowest correct level.

---

# 26. Required output from Claude after each phase

Claude must report:

```
PHASE:
FILES ADDED:
FILES MODIFIED:
SEMANTIC CHANGES:
SOLANA PARITY:
PERFORMANCE BEFORE:
PERFORMANCE AFTER:
PERFORMANCE DELTA:
TESTS RUN:
TEST RESULT:
OPEN QUESTIONS:
UNVERIFIED CONSTANTS:
NEXT PHASE:
COMMIT:
```

No vague "implemented successfully".

For tests include exact command and pass/fail count.

For replay parity include exact metrics.

---

# 27. Current task to execute now — Phase C / M4-M6

Execute **Phase C only**, plus the mandatory Phase-C preflight/validation-tool correction defined above.

Required order:

1. verify the exact starting SHA supplied by the external task; STOP on mismatch;
2. record untouched working-tree status and Python version;
3. **before ANY edit**, run and save the complete existing test suite and `python3 -m compileall darkbot tests`;
4. confirm `git diff -- docs/phase_a` is empty;
5. create maintained corrected tools at `tools/validation/compare_parity.py` and `tools/validation/bench_hotpath.py`; never modify `docs/phase_a/**`;
6. add/run deterministic validation-tool tests;
7. implement M4 generic EVM core exactly as specified;
8. implement M5 BSC chain exactly as specified;
9. implement M6 shared BSC dispatcher + minimum read-only capture persistence exactly as specified;
10. run targeted Phase-C tests;
11. run the mandatory integrated Fake-WSS/RPC -> BSC -> dispatcher -> Recorder/SqliteWriter -> DB readback test;
12. run the complete post-change test suite and compileall;
13. run the maintained current-runtime benchmark where applicable;
14. verify `git diff -- docs/phase_a` is still empty;
15. report exact results and STOP.

Do not execute Phase A again.

Do not execute Phase B.

Do not begin Phase D.

Do not implement real Flap or Four.meme recognizers, quote/fee/tax economics, Strategy C multichain, BSC PAPER/LIVE, signing or broadcasting.

Do not commit or push unless the external task explicitly overrides that rule.

---

# 28. Phase C acceptance criteria

All must be true:

- task-provided starting SHA was verified exactly;
- untouched pre-edit full test-suite result was recorded before any edit;
- untouched pre-edit compileall result was recorded before any edit;
- `docs/phase_a/**` remained byte-for-byte unchanged and `git diff -- docs/phase_a` is empty;
- maintained corrected validation tools exist under `tools/validation/`, not under frozen evidence;
- maintained parity tool enforces required source/dataset/runner/config-hash invariants and exits non-zero on FAIL;
- maintained hot-path benchmark exercises the real current runtime assembly/injection seam;
- new validation outputs are outside `docs/phase_a/`;
- M4 generic EVM core is implemented without venue-specific logic;
- M5 BSC chain verifies `eth_chainId == 56`, supports independent pending/canonical paths, bounded enrichment, reconnect/backfill and reorg handling;
- same-sender+nonce hashes remain competing candidates until evidence-based resolution;
- pending gaps are never fabricated as recovered;
- M6 uses one shared BSC dispatcher and preserves original receive timestamps;
- chain transaction identity and provider-observation identity remain separate;
- EVM uint256-capable values round-trip losslessly;
- canonical/orphan transitions remain auditable;
- `REPLACED`/`DROPPED` follow the explicit tested finalization rule;
- mandatory integrated Phase-C capture-path test passes;
- complete post-change test suite is green;
- post-change compileall is green;
- existing Solana/Pump/Strategy A/B/C semantics remain unchanged;
- legacy config hashes remain unchanged;
- BSC Flap/Four market profiles remain RESEARCH/non-runnable;
- no real Flap/Four implementation exists;
- no BSC quote/fee/tax economic model exists;
- no signer/private-key/broadcast path exists;
- full LIVE `T_PENDING_SEEN -> T_SOCKET_HANDOFF <= 3 ms` is **not** claimed as proven in Phase C;
- strict new real-capture Solana parity remains explicitly OPEN unless it was genuinely run later on the required new Solana capture.

After Phase C passes: **STOP and report. Do not begin Phase D.**
