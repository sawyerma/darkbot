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

The clean Solana replay baseline from the research status must remain reproducible.

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

During migration:

```python
self.decoder = decoder if decoder is not None else PumpDecoder(cfg.protocol)
self.reducer = reducer if reducer is not None else MarketReducer(...)
self.engine = quote_engine if quote_engine is not None else QuoteEngine(...)
```

This temporary fallback is allowed only to preserve old tests while adapter injection is introduced.

After parity is proven, runtime code must always inject components.

Do not change Strategy C semantics in this step.

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

# 18. Unified timing fields

Required for BSC:

```
pending_first_seen_mono_ns
decode_done_mono_ns
decision_done_mono_ns
intent_enqueued_mono_ns
submit_mono_ns
rpc_ack_mono_ns
create_included_mono_ns
buy_included_mono_ns
receipt_seen_mono_ns
```

Derived:

```
pending_to_decode_ns
pending_to_decision_ns
pending_to_enqueue_ns
pending_to_submit_ns
pending_lead_to_create_ns
pending_to_buy_ns
create_to_buy_ns
```

Also persist block ordering:

```
create_tx_index
buy_tx_index
buy_minus_create_tx_index
```

Milliseconds alone are insufficient for same-block execution analysis.

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

Only one BSC network ingest.

Both recognizers consume the same original `receive_mono_ns`.

Required report per venue:

```
launches_total
pending_seen
pending_coverage_pct
predicted_asset_id_available_pct
pending_lead_ms_min
pending_lead_ms_p50
pending_lead_ms_p95
pending_lead_ms_p99
pending_lead_ms_max
reverted_creates
replaced_creates
dropped_creates
first_external_buy_delay
trades_0_100ms
trades_0_250ms
trades_0_500ms
trades_0_750ms
trades_0_1000ms
creator_sell_count
decode_unknown_count
unsupported_mode_count
```

No economic conclusion before quote/fee verification.

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

## 21.1 Regression gate

Before any BSC feature merge:

```
pytest
```

must remain fully green.

Also run the existing Solana clean replay and compare frozen metrics.

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

## Phase A — architecture only

Implement:

- chain interfaces
- venue interfaces
- profile registry
- runtime assembly
- dependency injection seam

Do **not** implement BSC networking yet.

Acceptance:

- full existing suite passes
- exact Solana behavior unchanged

Commit separately.

## Phase B — Solana wrapper parity

Create Solana/Pump adapters around existing code.

Do not rewrite Pump code.

Acceptance:

- full suite passes
- clean replay exact parity

Commit separately.

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

1. inspect current repository
2. add `chains/base.py`
3. add `chains/registry.py`
4. add `venues/base.py`
5. add `venues/registry.py`
6. add `runtime/profiles.py`
7. add `runtime/assembly.py`
8. add minimal config support for `market_profile_id`
9. inject runtime components without changing existing Solana behavior
10. add registry/assembly tests
11. run entire test suite
12. run frozen Solana replay parity check
13. stop and report

Do **not** add BSC networking in this phase.

---

# 28. Phase A acceptance criteria

All must be true:

- existing test suite green;
- no existing test deleted or weakened;
- Solana/Pump runtime still works;
- Solana clean replay metrics unchanged;
- `solana-mainnet:pumpfun` resolves correctly;
- `bsc-mainnet:flap` exists as registry metadata only;
- `bsc-mainnet:fourmeme` exists as registry metadata only;
- selecting an unimplemented BSC runtime fails explicitly with a clear `NotImplementedError` or readiness error;
- no Flap/Four contract constant invented;
- no live broadcasting added;
- no historical DB migration required yet.

Only after Phase A passes should Phase B start.
