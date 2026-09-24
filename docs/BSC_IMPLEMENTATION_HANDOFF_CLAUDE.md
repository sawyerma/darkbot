# DarkBot BSC Dual-Venue — Implementation Handoff for Claude

**Purpose:** implementation contract for the next coding agent.  
**Repository:** `sawyerma/darkbot`  
**Target branch:** `main` only unless explicitly instructed otherwise.  
**Parent design documents:**

- `docs/MULTICHAIN_ROADMAP.md`
- `docs/BSC_DUAL_VENUE_ROADMAP.md`
- `docs/RESEARCH_STATUS_2026-09-23.md`
- `docs/STRATEGY_C_SOURCE_OF_TRUTH.md`

This document is intentionally prescriptive. Do not redesign the architecture unless a concrete contradiction in the repository requires it. If a contradiction is found, stop and report it with exact file/line references before changing semantics.

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

The Solana parity baseline for this refactor is **not** a historical diagnostic result and is **not** hard-coded to an old commit. Before any Phase-A code change, freeze the current implementation revision and run a strict, unmodified production replay as described in Section 0.5. The historical commit `4af978e18f91dea5d921ed7ef284b15d55d6c987` remains useful documentary context only; it is not automatically the Phase-A starting revision.

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

Change the constructor additively:

```python
class StateOwner:
    def __init__(
        self,
        ...,
        decoder=None,
        reducer=None,
        quote_engine=None,
        market_profile=None,
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

## 9.4 `quote.py`

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

## 10.4 `quote.py`

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

Required pending states:

```
PENDING
REPLACED
DROPPED
INCLUDED_SUCCESS
INCLUDED_REVERTED
UNKNOWN
```

For launch transactions add venue lifecycle:

```
PENDING_CREATE
CREATED_ACTIVE
CREATE_REVERTED
CREATE_DROPPED
CREATE_REPLACED
COMPLETED
MIGRATED
```

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

## 15.2 New `pending_transactions`

Minimum schema:

```sql
CREATE TABLE pending_transactions (
    run_id TEXT NOT NULL,
    chain_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    tx_hash TEXT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT,
    nonce INTEGER NOT NULL,
    value_native TEXT NOT NULL,
    calldata_selector TEXT,
    first_seen_mono_ns INTEGER NOT NULL,
    last_seen_mono_ns INTEGER NOT NULL,
    replacement_tx_hash TEXT,
    final_status TEXT,
    included_block_number INTEGER,
    included_block_hash TEXT,
    included_tx_index INTEGER,
    receipt_status INTEGER,
    PRIMARY KEY (run_id, provider_id, tx_hash)
);
```

Use TEXT for values that may exceed signed SQLite integer range.

## 15.3 New `pending_launches`

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
    create_included_mono_ns INTEGER,
    pending_lead_ns INTEGER,
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
- Solana profile resolves Pump
- Flap profile resolves BSC + Flap
- Four profile resolves BSC + Four

## 21.3 EVM tests

Create:

```
tests/test_evm_pending.py
tests/test_evm_receipts.py
tests/test_evm_nonce.py
tests/test_evm_reorg.py
tests/test_bsc_gas.py
tests/test_bsc_dispatcher.py
```

Must cover:

- pending decode
- replacement by same sender+nonce
- dropped tx
- success receipt
- revert receipt
- gas charged on revert
- duplicate provider frames
- same tx seen by two providers
- block/log ordering
- reorg rollback semantics

## 21.4 Flap tests

```
tests/test_flap_pending.py
tests/test_flap_decoder.py
tests/test_flap_quote.py
tests/test_flap_fees.py
tests/test_flap_execution_plan.py
```

Use verified captured fixture data.

No made-up fixture constants.

## 21.5 Four.meme tests

```
tests/test_fourmeme_pending.py
tests/test_fourmeme_decoder.py
tests/test_fourmeme_quote.py
tests/test_fourmeme_fees.py
tests/test_fourmeme_modes.py
tests/test_fourmeme_execution_plan.py
```

Explicitly test unknown mode refusal.

---

# 22. Implementation phases for Claude

## Phase A — architecture + minimal Solana/Pump wrapper parity

Phase A includes the **minimum Solana/Pump wrappers required to make the adapter seam real**. This resolves the earlier phase-boundary ambiguity.

Implement:

- freeze pre-change baseline per Section 0.5;
- chain interfaces;
- venue interfaces;
- profile registry;
- runtime assembly;
- dependency-injection seam;
- minimal Solana chain wrapper around existing implementation;
- minimal Pump.fun venue wrapper around existing implementation;
- legacy config compatibility without changing existing config hashes;
- explicit non-runnable BSC profile metadata.

Rules:

- reuse existing Solana/Pump classes/functions;
- do not move/rewrite Pump decoder, reducer, quote or fee logic unless strictly necessary for wrapping;
- preserve existing `StateOwner(engine=...)` replay injection and all replay-specific parameters;
- do not implement BSC networking;
- do not implement Flap/Four contracts;
- do not add live broadcasting.

Acceptance:

- full existing suite passes;
- exact strict Solana replay parity passes per Sections 0.5 and 21.1;
- legacy configs without `market_profile_id` still resolve to Solana/Pump;
- those legacy config hashes remain unchanged;
- `bsc-mainnet:flap` and `bsc-mainnet:fourmeme` exist as metadata only and fail explicitly if execution/runtime construction is requested.

Commit Phase A separately, then **STOP** and report.

## Phase B — optional adapter cleanup after Phase-A approval

Phase B is **not** where Solana wrappers are first introduced.

Only after Phase A is independently reviewed and accepted may Phase B:

- remove temporary fallback wiring that is proven unnecessary;
- improve adapter type boundaries;
- reduce imports/coupling;
- add non-semantic cleanup.

It must still preserve exact Solana parity.

Do not start Phase B automatically.

## Phase C — EVM/BSC capture primitives

Implement generic EVM envelopes + BSC feed abstractions + dispatcher.

No protocol economics.

Acceptance:

- synthetic pending/receipt/replacement tests pass
- no live broadcasting exists

Commit separately.

## Phase D — Flap read-only decoder

Only after verified ABI/contracts are supplied/frozen.

Acceptance:

- pending CREATE recognized
- canonical CREATE/BUY/SELL recognized
- fixture tests pass
- unknown versions rejected

Commit separately.

## Phase E — Four.meme read-only decoder

Same.

Commit separately.

## Phase F — combined 30-minute capture

Add CLI/config support for both BSC venues in capture-only mode.

Acceptance:

- one feed timestamp origin
- both venue reports
- no broadcast path

Only after this capture decide quote/execution priorities.

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

# 27. First task to execute now

Implement **Phase A only**.

Do not jump ahead.

Specifically:

1. inspect the current repository and the parent design/source-of-truth documents;
2. record the actual full pre-change commit SHA and working-tree status;
3. run and save the complete pre-change test-suite result;
4. create/run/save the strict production Solana replay baseline per Section 0.5;
5. add `chains/base.py`;
6. add `chains/registry.py`;
7. add `venues/base.py`;
8. add `venues/registry.py`;
9. add `runtime/profiles.py`;
10. add `runtime/assembly.py`;
11. add **minimal Solana chain and Pump.fun venue wrappers in Phase A**, reusing existing implementation;
12. add backward-compatible effective `market_profile_id` resolution without mutating old canonical config JSON/hashes;
13. preserve all existing replay-specific dependency injection and offline semantics;
14. inject runtime components into normal Solana runtime paths without semantic change;
15. register BSC/Flap and BSC/Four as metadata only; attempting to run them must fail explicitly;
16. add registry/assembly/backward-compatibility tests;
17. run the entire post-change test suite;
18. rerun the exact pre-change strict Solana replay baseline;
19. perform and save the exact parity comparison;
20. stop and report.

Do **not** add BSC networking, EVM decoding, Flap/Four contract constants, or live broadcasting in this phase.

---

# 28. Phase A acceptance criteria

All must be true:

- actual pre-change full commit SHA was recorded;
- complete pre-change test result was recorded;
- strict pre-change Solana replay baseline manifest exists;
- complete post-change test suite is green;
- no existing test was deleted, weakened, skipped, or rewritten merely to accept new behavior;
- minimal Solana/Pump wrappers are present and reuse existing implementation;
- Solana/Pump runtime still works;
- strict before/after replay parity passes under the exact same command/runner and dataset;
- lamport-denominated results compare as exact integers, not rounded SOL;
- decision/event/attempt/fill/fee/ledger semantics match under the predetermined run-id-independent mapping;
- diagnostic legacy-compatible monkeypatch results were not promoted into production semantics;
- existing replay `engine` injection, `canonical_prefix`, recorded fee evidence, stress parameters, virtual clock and other replay-specific inputs remain intact;
- replay remains offline;
- legacy config without `market_profile_id` resolves effectively to `solana-mainnet:pumpfun`;
- legacy config hashes remain unchanged;
- `solana-mainnet:pumpfun` resolves correctly;
- `bsc-mainnet:flap` exists as registry metadata only;
- `bsc-mainnet:fourmeme` exists as registry metadata only;
- selecting an unimplemented BSC runtime fails explicitly with a clear `NotImplementedError` or readiness error;
- no Flap/Four contract constant was invented;
- no BSC networking was added;
- no live broadcasting was added;
- no historical DB migration is required in Phase A.

After Phase A passes, **STOP**. Do not begin Phase B until the Phase-A implementation and parity artifacts have been independently reviewed and approved.
