# DarkBot BSC Dual-Venue Roadmap — Flap.sh + Four.meme

Status: active implementation roadmap; Phase A seam integrated, Phase C is next  
Parent architecture: `docs/MULTICHAIN_ROADMAP.md`  
Implementation handoff / phase gates: `docs/BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md`  
Original design baseline before BSC work: `main@9a0b5cb32f46bce78f2fb18e4f0f5672bac07f7d`  
Current implementation anchor: `main@004f440c39c147087a7385bf16a4dc6660058936`

## Roadmap continuity and current status — 2026-09-25

This file is the **single authoritative active BSC milestone sequence and status roadmap**.

Document responsibilities are fixed:

- **this file** = what is DONE / NEXT / PENDING and in what order work is allowed to happen;
- **`docs/BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md`** = exact technical contract and acceptance rules for the active phase;
- **`docs/MULTICHAIN_ROADMAP.md`** = historical parent architecture only; it MUST NOT override current phase scope or numbering;
- **`docs/phase_a/PHASE_A_REPORT.md`** = immutable historical evidence only.

Current repository/documentation HEAD for the Phase-C handoff is:

```
main@a65ba4aa0129c02328ccdb8d9426741ab3fdfc9d
```

The Phase-A production-code seam underneath it was integrated at:

```
main@004f440c39c147087a7385bf16a4dc6660058936
```

No other roadmap is allowed to change the active ordering below.

Crosswalk:

| Phase | Milestones | Status | Exact scope |
| --- | --- | --- | --- |
| Phase A | M0-M3 | **IMPLEMENTED** | multichain seam, Solana/Pump wrappers, profiles/runtime assembly, backward compatibility; strict new real-capture Solana parity still OPEN |
| Phase-A evidence cleanup | validation tools only | **REQUIRED BEFORE PHASE-C CODE** | fix/supplement parity PASS/exit-code enforcement and benchmark of actual assembly wiring; no Solana semantic change |
| Phase B | optional cleanup | **SKIPPED** | do not start unless an independent concrete need is approved |
| Phase C | M4 + M5 + M6 | **NEXT** | EVM core + BSC chain + shared dispatcher + minimal read-only capture storage; no real venues/economics/trading |
| Phase D | M7 | **PENDING** | Flap read-only decoder only |
| Phase E | M8 | **PENDING** | Four.meme read-only decoder only |
| Phase F | M9 | **PENDING** | one combined real 30-minute BSC capture for both venues |
| post-Phase-F economics gate | unnumbered prerequisite before M10 | **BLOCKED** | verified quote/fee/tax economics + eth_call/helper parity using evidence from the real capture |
| post-Phase-F | M10-M17 | **BLOCKED** | Strategy C multichain, replay, PAPER, send/inclusion, venue execution, ordered backrun, forward capture, GUI |

Phase-A/M0-M3 detail at the current anchor:

- M1 adapter interfaces: DONE.
- M2 Solana/Pump wrappers: DONE.
- M3 generic config/profile/runtime seam and generic identity mapping: DONE for the Phase-A seam; generic/BSC storage additions required by capture remain to be completed with Phase C.
- M0 code baseline/evidence freeze exists, but the old real `clean-v2` database is no longer available. The strict real-data Solana parity gate must therefore be re-established later with a new, separate Solana 30-minute capture. Do not fabricate the missing old result.
- `docs/phase_a/PHASE_A_REPORT.md` remains historical evidence and must not be rewritten as a later-phase status document.

The next implementation block is therefore **Phase C / M4-M6**, not a new architecture rewrite and not Phase B.

Before touching Phase-C production code, repair/supplement the two known Phase-A validation-tool gaps documented in `BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md`:

- `compare_parity.py`: advertised config-hash invariants must participate in PASS and FAIL must return non-zero;
- `bench_hotpath.py`: future benchmark must exercise the real current assembly/injection wiring rather than only the retained fallback path.

Historical Phase-A artifacts remain unchanged.

### Non-negotiable Phase-C semantics

Phase C must implement all of the following before it can be accepted:

- same-sender+nonce different hashes are competing candidates; observation order alone is never definitive replacement;
- canonical path is independent from pending visibility;
- each BSC HTTP/WSS connection verifies `eth_chainId == 56` on connect/reconnect;
- hash-only pending enrichment is bounded/asynchronous and preserves original first-seen time;
- reconnect/restart performs deterministic missing-block canonical backfill before canonical coverage is marked recovered;
- pending gaps are retained because missed mempool history is not fabricated;
- block hash + parent hash + bounded ancestry drive reorg logic; orphan evidence is preserved with canonicality transition audit;
- chain transaction identity is separate from per-provider observation identity;
- arbitrary EVM uint256-capable values are stored losslessly, never float;
- `REPLACED`/`DROPPED` require an explicit evidence-based finalization rule, not observation order or timeout alone;
- a mandatory integrated `Fake WSS/RPC -> BSC chain -> dispatcher -> Recorder/SqliteWriter -> DB readback` test covers disconnect, backfill, reorg and shutdown;
- Phase C measures parse/dispatch/enrichment/queue latency only and does not claim the later 3 ms socket-handoff LIVE contract is proven.

The detailed contract is in `docs/BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md`; these bullets are not optional.

At Phase F, the synchronized experiment uses two independent databases:

```
data/market_capture_bsc_30m.sqlite
    BSC only
    Flap.sh + Four.meme in one shared BSC capture

data/market_capture_solana_30m.sqlite
    Solana only
    Pump.fun
```

They may run during the same 30-minute wall-clock interval, but data from one chain must never be written to or replayed as the other chain.

## 0. Goal

Add BNB Smart Chain as one chain implementation and add **two completely separate launchpad/venue implementations** on top of it:

- `solana-mainnet / pumpfun`
- `bsc-mainnet / flap`
- `bsc-mainnet / fourmeme`

The GUI must later expose these as separate selectable markets:

```
Solana / Pump.fun
BSC / Flap.sh
BSC / Four.meme
```

Internally, never use names such as `BSC1` or `BSC2`. Stable IDs are:

```
solana-mainnet:pumpfun
bsc-mainnet:flap
bsc-mainnet:fourmeme
```

The architecture must allow a future venue or chain to be added without editing Strategy C core.

The current Solana/Pump.fun implementation and all historical Strategy C replay results remain immutable evidence. Do not rewrite legacy semantics to make BSC fit.

---

# 1. Non-negotiable architecture

The code is split into four layers:

```
GUI / control plane
        |
        v
Core strategy + replay + portfolio + reporting
        |
        +--------------------------+
        |                          |
        v                          v
Chain adapter                 Venue adapter
(network mechanics)           (launchpad mechanics)
        |                          |
        +------------+-------------+
                     |
                     v
              normalized market
```

## 1.1 Core — shared by every chain and venue

The core owns only concepts that are universal:

- strategy lifecycle
- positions
- portfolio
- entry/exit decisions
- observation windows
- target/timeout logic
- replay scheduler
- PAPER execution lifecycle
- quality/evidence handling
- latency telemetry
- persistence contract
- reporting
- qualification
- research sweeps

The core must **not** know:

- Solana program IDs
- Borsh
- SPL token accounts
- lamports
- EVM selectors
- BSC gas
- Flap Portal
- Four.meme TokenManager
- any protocol-specific curve formula

## 1.2 Chain adapter — shared by all venues on one chain

A chain adapter owns network mechanics.

### Solana chain adapter

Owns:

- Helius / Solana feeds
- slot/signature mechanics
- Solana network fee semantics
- transaction build/sign/send
- commitment/confirmation semantics
- optional future Sender/Shred paths

### EVM base adapter

Shared by BSC and future EVM chains:

- JSON-RPC
- WebSocket
- pending transaction model
- receipts
- blocks
- reorg handling
- nonce/replacement semantics
- EIP-1559 / legacy gas abstractions
- transaction signing
- RPC/provider health

### BSC chain adapter

Specializes EVM for BNB Chain:

- chain id 56
- BNB native asset
- BSC block timing telemetry
- BSC pending transaction sources
- local `bsc-geth` IPC/WS
- builder/relay submission interfaces
- BSC gas semantics
- BSC confirmation/reorg policy

The BSC adapter knows **nothing** about Flap or Four.meme.

## 1.3 Venue adapter — one package per launchpad

### Pump.fun adapter

Owns:

- Pump Borsh decoder
- CREATE/BUY/SELL/COMPLETE/MIGRATE semantics
- Pump curve state
- Pump quote math
- Pump protocol fees
- Pump transaction templates

### Flap adapter

Owns:

- Portal contract versions/addresses
- launch selectors
- pending CREATE decode
- token-address prediction only if proven deterministic
- bonding-curve state
- quote math
- `quoteExactInput`
- `swapExactInput`
- protocol/tax rules
- migration to PancakeSwap
- Flap-specific anti-bot/tax/vault modes

### Four.meme adapter

Owns:

- TokenManager / TokenManager2 / Helper versions
- CREATE call decoding
- TokenCreate / TokenPurchase / TokenSale / LiquidityAdded events
- version/mode detection
- TaxToken
- X Mode
- AntiSniperFeeMode
- Four.meme quote math / helper verification
- Four.meme protocol fees
- graduation/migration behavior

Neither BSC venue is allowed to import the other venue package.

---

# 2. Target code structure

Use this as the target, but migrate incrementally rather than moving everything at once.

```
darkbot/
    core/
        domain.py
        engine.py
        strategy.py
        replay.py
        paper.py
        portfolio.py
        reporting.py
        storage.py
        qualification.py
        clock.py

    chains/
        base.py
        registry.py

        solana/
            chain.py
            feeds.py
            fees.py
            tx.py
            execution.py
            units.py

        evm/
            base.py
            rpc.py
            websocket.py
            mempool.py
            blocks.py
            receipts.py
            nonce.py
            gas.py
            tx.py
            reorg.py

        bsc/
            chain.py
            feeds.py
            fees.py
            execution.py
            builders.py
            units.py

    venues/
        base.py
        registry.py

        pumpfun/
            protocol.py
            decoder.py
            reducer.py
            quote.py
            execution.py

        flap/
            protocol.py
            contracts.py
            abi/
            pending.py
            decoder.py
            reducer.py
            quote.py
            fees.py
            execution.py
            migration.py

        fourmeme/
            protocol.py
            contracts.py
            abi/
            pending.py
            decoder.py
            reducer.py
            quote.py
            fees.py
            execution.py
            migration.py

    strategies/
        base.py
        strategy_a_fast_creator.py
        strategy_b_legacy_5buy.py
        strategy_c_legacy_backend.py
        strategy_c_multichain.py

    runtime/
        assembly.py
        profiles.py
```

Do not create a shared `bsc_launchpad.py` containing Flap and Four.meme conditionals. Common BSC/EVM behavior belongs in `chains/`; launchpad-specific behavior belongs in separate `venues/` packages.

---

# 3. Runtime profiles and GUI selection

The backend exposes a stable market-profile registry.

```python
MarketProfile(
    profile_id="bsc-mainnet:flap",
    chain_id="bsc-mainnet",
    venue_id="flap",
    display_name="BSC / Flap.sh",
)

MarketProfile(
    profile_id="bsc-mainnet:fourmeme",
    chain_id="bsc-mainnet",
    venue_id="fourmeme",
    display_name="BSC / Four.meme",
)
```

Existing Solana becomes:

```python
MarketProfile(
    profile_id="solana-mainnet:pumpfun",
    chain_id="solana-mainnet",
    venue_id="pumpfun",
    display_name="Solana / Pump.fun",
)
```

## 3.1 GUI contract

The GUI never contains protocol addresses or trading logic.

It requests:

```
GET /api/market-profiles
```

and receives:

```json
[
  {
    "id": "solana-mainnet:pumpfun",
    "chain": "Solana",
    "venue": "Pump.fun",
    "status": "READY"
  },
  {
    "id": "bsc-mainnet:flap",
    "chain": "BSC",
    "venue": "Flap.sh",
    "status": "RESEARCH"
  },
  {
    "id": "bsc-mainnet:fourmeme",
    "chain": "BSC",
    "venue": "Four.meme",
    "status": "RESEARCH"
  }
]
```

The user selects exactly one trading profile.

Backend receives only the stable ID:

```
market_profile_id = "bsc-mainnet:flap"
```

The profile determines chain + venue + config.

The GUI must not translate `BSC / Flap.sh` into ad-hoc flags.

## 3.2 Research capture exception

Trading/PAPER uses one active venue.

Research capture may enable both BSC venues simultaneously:

```json
{
  "market": {
    "chain": "bsc-mainnet",
    "venues": ["flap", "fourmeme"],
    "mode": "CAPTURE_MULTI_VENUE"
  }
}
```

This uses **one BSC mempool connection and one block/receipt connection**, then dispatches transactions to both venue recognizers.

That is how both venues can be measured in the same 30-minute run without duplicating network work.

---

# 4. Shared normalized domain

Introduce generic identifiers additively.

```
asset_id
tx_id
block_ref
block_number
block_hash
tx_index
event_index
chain_id
venue_id
native_amount_base_units
native_symbol
native_decimals
signal_origin
```

Legacy Solana fields remain during migration:

```
mint
slot
signature
sol_amount
*_lamports
```

Solana initially writes both generic and legacy fields.

BSC writes generic fields.

Do not delete the legacy fields until old databases and old replays have a defined migration path.

---

# 5. Universal event contract

Both Flap and Four.meme must normalize into the same event vocabulary:

```
CREATE
BUY
SELL
COMPLETE
MIGRATE
```

BSC additionally needs pre-canonical lifecycle events:

```
PENDING_CREATE
PENDING_BUY
PENDING_SELL
PENDING_REPLACED
PENDING_DROPPED
TX_REVERTED
```

Normalized event example:

```python
NormalizedEvent(
    event_type=EventType.CREATE,
    chain_id="bsc-mainnet",
    venue_id="flap",
    asset_id="0x...",
    actor="0x...",
    creator="0x...",
    tx_id="0x...",
    block_ref=123,
    event_index=4,
    native_amount_base_units=...,
    token_amount_base_units=...,
    receive_mono_ns=...,
    venue_payload={...},
)
```

Strategy C must consume normalized events/features, not raw logs.

---

# 6. BSC shared capture pipeline

One BSC process should capture both venues.

```
bsc-geth / provider pending feed
          |
          v
    PendingTxEnvelope
          |
          v
   VenueDispatcher
      /       \
     /         \
 FlapMatcher  FourMemeMatcher
     |             |
     v             v
 FlapDecoder   FourMemeDecoder
     \             /
      \           /
       v         v
   Normalized events
          |
          v
      recorder
```

Canonical block/log feed runs in parallel:

```
new block / logs / receipts
          |
          v
   CanonicalDispatcher
      /          \
   Flap          Four
      \          /
       v        v
 reconcile pending -> included/reverted/replaced
```

## 6.1 Why one BSC feed

Do not open one full mempool connection per venue unless a provider forces it.

A pending BSC transaction already contains:

- tx hash
- sender
- nonce
- destination
- calldata
- value
- gas fields

Routing by destination address and selector is cheap.

This gives:

- one network ingest
- one timestamp origin
- directly comparable Flap/Four lead times
- lower CPU/network overhead
- no double-counting of pending transactions

---

# 7. Venue dispatch contract

Create a cheap hot-path recognizer.

```python
class VenueRecognizer(Protocol):
    venue_id: str

    def maybe_matches_pending(self, tx: PendingTxEnvelope) -> bool: ...
    def maybe_matches_log(self, log: CanonicalLog) -> bool: ...
```

Recognition must be cheap:

1. destination contract address
2. 4-byte selector
3. optionally event topic

Only after a match do we run the full ABI decoder.

No JSON reserialization, HTTP calls, database queries, or external API calls on the hot path.

---

# 8. Flap.sh implementation

## 8.1 Source-of-truth policy

Freeze:

- chain id
- Portal address
- Portal version
- ABI hash
- supported launch selectors
- supported trade selectors
- protocol fee rule
- token-tax rule
- graduation/migration rule
- Pancake route assumptions

All addresses and ABIs are versioned configuration snapshots.

Never silently follow a changed web frontend.

## 8.2 Pending CREATE

Implement:

```
FlapPendingDecoder
```

Responsibilities:

- detect launch call to Portal
- decode launch calldata
- creator
- salt / deployment parameters
- quote asset
- tax/vault mode
- initial value / initial purchase
- metadata reference if present
- predict token address only when exact CREATE2 derivation has been verified

Output:

```python
PendingLaunch(
    venue_id="flap",
    tx_id=...,
    creator=...,
    predicted_asset_id=... | None,
    first_seen_mono_ns=...,
    launch_params=...,
)
```

A missing deterministic token address is not guessed.

## 8.3 Canonical Flap events

Decode and persist:

- launch/create
- curve BUY
- curve SELL
- migration/graduation to PancakeSwap
- tax/vault configuration events that affect economics

The exact event names/topics are frozen from the verified ABI.

## 8.4 Quotes

Implement a dedicated:

```
FlapQuoteEngine
```

The primary model must be deterministic from contract state.

Validation path:

1. local formula
2. `eth_call quoteExactInput`
3. compare exact integer result
4. historical executed swap
5. require exact/defined rounding parity

A quote mismatch blocks economic qualification.

## 8.5 Trade execution plan

The venue does not sign/send.

It returns:

```python
ExecutionPlan(
    chain_id="bsc-mainnet",
    venue_id="flap",
    to=portal,
    value_wei=...,
    calldata=...,
    gas_limit_hint=...,
    slippage_bound=...,
)
```

BSC chain execution signs and submits it.

## 8.6 Sell path

Prefer protocol-supported permit flow when verified.

Do not assume approve is free or absent.

The cost model must support:

- permit path
- approve + sell path
- failed approval
- failed sell
- post-graduation Pancake path

## 8.7 Flap risks to detect before entry

At minimum:

- token tax rate
- buy tax
- sell tax
- vault mode
- unsupported quote asset
- unsupported Portal version
- migration already started
- trading restrictions
- unexpected implementation
- quote mismatch

Unknown mode => no qualified entry.

---

# 9. Four.meme implementation

## 9.1 Source-of-truth policy

Freeze:

- TokenManager V1 address
- TokenManager2 V2 address
- Helper version/address
- ABI hashes
- TokenCreate/TokenPurchase/TokenSale/LiquidityAdded topics
- version rules
- tax mode
- X Mode
- AntiSniperFeeMode
- graduation behavior

New tokens are expected through TokenManager2, but runtime detection must still verify version.

## 9.2 Pending CREATE

Implement:

```
FourMemePendingDecoder
```

Responsibilities:

- recognize TokenManager2 create transaction
- decode `createArg` and other public calldata
- identify creator/sender
- identify mode bits that affect economics
- extract/predict token address only when deterministically derivable from on-chain logic/calldata
- record launch value / presale information

The private Four.meme API is not used as a hot-path dependency for observing other users.

If the API publishes useful public metadata, it may be used asynchronously after the pending tx is already captured.

## 9.3 Canonical events

Decode:

- TokenCreate
- TokenPurchase
- TokenSale
- LiquidityAdded
- mode/tax configuration needed for economics

## 9.4 Quotes

Implement:

```
FourMemeQuoteEngine
```

Validation:

1. local math
2. Helper `tryBuy` / `trySell`
3. exact historical transactions
4. integer/rounding parity

Handle separately:

- normal V2 token
- TaxToken
- X Mode
- AntiSniperFeeMode

Unknown or unsupported mode => no qualified entry.

## 9.5 Execution plan

Return a generic BSC `ExecutionPlan`; BSC signs/sends.

The Four.meme package owns calldata selection:

- `buyToken`
- `buyTokenAMAP`
- required special-mode method
- `sellToken`
- approval requirements

The BSC package never selects Four.meme methods.

---

# 10. Strategy C separation

Keep:

```
LEGACY_BACKEND_FILTER_V1
```

unchanged forever for historical Solana reproduction.

Add:

```
STRATEGY_C_MULTICHAIN_V1
```

It consumes chain-neutral features:

- launch age
- buy count
- sell count
- unique buyers
- creator sold
- creator initial-buy fraction
- native volume
- USD volume
- market cap USD
- price multiple
- tax/fee burden
- pending lead time
- canonical coverage quality

Venue-specific features may exist, but they are namespaced:

```
flap.tax_bps
flap.vault_mode
fourmeme.anti_sniper_mode
fourmeme.tax_bps
```

A generic Strategy C config cannot silently depend on one venue-specific field.

---

# 11. Latency semantics

BSC gives us an earlier signal class than current Solana `processed`.

Record:

```
T_PENDING_SEEN
T_DECODE
T_DECISION
T_ENQUEUE
T_SUBMIT
T_ACK
T_CREATE_INCLUDED
T_BUY_INCLUDED
T_RECEIPT
```

Derived:

```
pending_lead_to_create
pending_to_decision
pending_to_submit
pending_to_buy
create_to_buy
buy_tx_index_minus_create_tx_index
```

The key quantity for Strategy C on BSC is not only milliseconds.

It is also **ordering**:

```
CREATE tx index = n
BUY tx index    = n + 1 / n + k
```

A BUY received early but ordered before CREATE may revert.

Therefore every BSC replay/live test must distinguish:

- arrival latency
- inclusion latency
- same-block ordering

---

# 12. Execution modes on BSC

Implement research/execution modes explicitly.

## 12.1 CONFIRMED_POST_CREATE

Signal only after canonical CREATE.

Purpose:

- baseline
- easiest correctness path
- likely slower economically

## 12.2 PUBLIC_MEMPOOL_REACTIVE

See pending CREATE, prepare BUY, submit normally.

Purpose:

- determine whether ordinary BSC propagation can produce useful same-block ordering

## 12.3 ORDERED_BACKRUN

See pending CREATE and submit through a builder/relay mechanism designed to place BUY after the CREATE.

Purpose:

- primary low-latency Strategy C candidate

Telemetry must prove actual block ordering.

Do not infer success from ACK.

---

# 13. BSC execution architecture

Shared for Flap and Four.meme:

```
Venue ExecutionPlan
       |
       v
BscExecutionAdapter
       |
       +--> nonce manager
       +--> gas policy
       +--> signer
       +--> public RPC sender
       +--> builder/relay sender
       +--> local node fallback
       +--> receipt observer
```

## 13.1 Nonce manager

Must support:

- one hot wallet
- local next-nonce cache
- pending nonce reconciliation
- replacements
- dropped tx
- restart persistence
- never reuse a nonce accidentally

## 13.2 Sender interface

```python
class BscSender(Protocol):
    async def submit(self, signed_tx, context) -> SubmissionResult: ...
```

Implementations later:

- `PublicRpcSender`
- `LocalNodeSender`
- `BuilderRelaySender`

Venue code does not care which sender is active.

## 13.3 Builder abstraction

Do not hardcode one MEV company.

```
BscBuilderProvider
    submit_bundle(...)
    submit_private_tx(...)
    health(...)
```

Provider implementations can be selected by config.

---

# 14. PAPER/replay safety

Maintain the existing hard PAPER boundary.

```
capture  -> never signs
replay   -> never signs
paper    -> never signs
live     -> may sign/broadcast
```

Adding BSC must not weaken `LiveBroadcastForbidden`.

Live-capable execution is a separate adapter and mode.

---

# 15. Storage model

Additive migration only.

## 15.1 Runs

Add:

- market_profile_id
- chain_id
- venue_id
- native_symbol
- native_decimals

## 15.2 Pending transactions

New table:

```
pending_transactions
```

Fields:

- run_id
- chain_id
- provider
- tx_hash
- from_address
- to_address
- nonce
- value_native
- calldata_selector
- first_seen_mono_ns
- last_seen_mono_ns
- replacement_tx_hash
- final_status
- included_block_number
- included_block_hash
- included_tx_index
- receipt_status

## 15.3 Pending launches

New table:

```
pending_launches
```

Fields:

- run_id
- venue_id
- tx_hash
- creator
- predicted_asset_id nullable
- first_seen_mono_ns
- decoded_mode
- launch_params_json
- canonical_asset_id nullable
- canonical_create_event_id nullable
- create_inclusion_observed_mono_ns nullable
- pending_lead_observed_ns nullable

## 15.4 Market events

Add generic:

- asset_id
- tx_id
- block_number
- block_hash
- tx_index
- event_index
- chain_id
- venue_id
- native_amount_base_units

Do not rewrite old Solana data.

---

# 16. Cost accounting

## 16.1 Shared BSC network costs

Record per attempt:

- gas_limit
- gas_used
- effective_gas_price
- actual gas cost
- builder/relay tip
- reverted-tx gas
- approval gas
- replacement gas

Never assume a reverted transaction costs zero.

## 16.2 Flap economic costs

Record separately:

- protocol fee
- buy tax
- sell tax
- vault-specific fee
- migration route cost
- slippage
- gas
- relay tip

## 16.3 Four.meme economic costs

Record separately:

- protocol fee
- tax token fee
- anti-sniper fee
- X-mode fee/constraints
- approval
- gas
- relay tip
- slippage

Unknown fee mode => economic result is unresolved, not zero.

---

# 17. One 30-minute BSC capture for both venues

After the shared BSC feed and both decoders exist, run one synchronized capture.

```
duration: 30 minutes
chain: BSC mainnet
venues:
  - flap
  - fourmeme
mempool: enabled
canonical blocks/logs: enabled
broadcast: forbidden
```

For each venue report independently:

- launches_total
- pending_launches_seen
- pending_coverage_pct
- deterministic_asset_id_precreate_pct
- create_reverted_pct
- replaced/dropped pct
- pending lead min/p50/p95/p99/max
- time from create inclusion to first external BUY
- trades in first 100/250/500/750/1000 ms
- creator sell distribution
- tax/mode distribution
- curve price path
- volume
- decoding completeness

Also report combined chain-level:

- pending tx throughput
- provider gaps
- block interval distribution
- reorgs
- CPU load
- ingest queue depth
- decode latency

This directly answers which venue is better under the same network conditions.

---

# 18. Strategy C replay matrix

Run Flap and Four.meme separately from the same capture.

## 18.1 Entry latency

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

## 18.2 Target

```
1.5x
2.0x
3.0x
3.5x
```

## 18.3 Timeout

```
250
500
750
1000
2000 ms
```

## 18.4 Execution model

Run separately:

- post-create
- pending reactive
- ordered backrun

Do not mix them in one number.

## 18.5 Required outputs

For each variant:

- launches
- eligible
- attempts
- fills
- reverts
- unresolved
- targets
- creator exits
- timeouts
- realized PnL
- pessimistic PnL
- EV/resolved
- EV/attempt
- PF
- win rate
- total protocol/tax cost
- total gas
- total relay tips
- top-1/top-3/top-5 PnL contribution
- chronological thirds
- venue-mode breakdown

---

# 19. Exact development milestones

## M0 — freeze current Solana evidence

- current main SHA recorded
- all existing tests green
- clean 30-minute Solana replay frozen
- no changes to legacy Strategy C

## M1 — adapter interfaces

Add:

```
darkbot/chains/base.py
darkbot/chains/registry.py
darkbot/venues/base.py
darkbot/venues/registry.py
darkbot/runtime/assembly.py
```

No behavior change.

## M2 — wrap Solana/Pump

Inject existing Pump decoder/reducer/quote/feed/fee objects.

Gate:

- full suite green
- exact clean-v2 replay parity

## M3 — generic IDs/units/storage

Add generic fields while preserving old columns.

Gate:

- old DB readable
- old replay unchanged

## M4 — EVM core

Implement generic, venue-free EVM capture primitives:

- JSON-RPC + WebSocket parsing/health/error handling;
- immutable pending/block/transaction/log/receipt evidence;
- exact hex/integer handling with lossless uint256-capable storage representation;
- provider pending capability: full object / hash only / unavailable;
- bounded asynchronous hash-only enrichment with dedup/concurrency/overflow accounting;
- sender+nonce **candidate competition**, not observation-order replacement;
- block ancestry and reorg primitives.

Unit-test with synthetic fixtures.

No BSC venue logic. No quotes. No trading.

## M5 — BSC chain

Implement one read-only BSC specialization:

- chain id 56 / `eth_chainId == 0x38`;
- BNB / 18 decimals;
- BSC provider config and health;
- chain-id verification on connect/reconnect;
- pending ingress;
- independent canonical head/block/transaction/log/receipt collection;
- last-completely-processed canonical high-water mark;
- reconnect/restart missing-block backfill with hash/parent-hash continuity checks;
- pending-gap tracking separate from canonical-gap recovery;
- bounded ancestry + reorg processing with orphan evidence retained;
- auditable canonicality transitions;
- BSC timing telemetry.

No venue logic.

## M6 — shared BSC multi-venue dispatcher + minimum capture persistence

Implement one shared BSC dispatcher and the minimum read-only persistence needed to test/use it:

- one pending ingest;
- one canonical ingest;
- one original timestamp origin;
- recognizer protocol for synthetic Phase-C recognizers;
- chain transaction evidence stored once per `(run_id, chain_id, tx_hash)`;
- provider observations stored independently per provider/tx;
- no duplicate raw chain evidence per future venue;
- exact big-int round-trip;
- bounded Recorder/Writer integration;
- mandatory integrated Fake-WSS/RPC -> chain -> dispatcher -> SQLite test including disconnect, canonical backfill, short reorg and clean shutdown.

Gate:

- original pending timestamp passed unchanged to every recognizer;
- no raw chain transaction duplication across recognizers/providers;
- provider timing evidence retained separately;
- canonical path works when pending feed missed a transaction;
- replacement/drop finalization is evidence-based and tested;
- all Phase-C tests + full Solana regressions pass;
- no signing or broadcast path exists.

After M6: **STOP and review.**

## M7 — Flap read-only decoder

Only after Phase C acceptance and verified Flap contracts/ABI/fixtures are frozen.

Implement read-only:

- pending CREATE recognition;
- canonical CREATE/BUY/SELL/migration decoding supported by verified evidence;
- version/mode validation;
- unknown versions/modes rejected.

Explicitly NOT M7:

- quote engine;
- protocol fee/tax economics;
- `eth_call` quote parity;
- execution plan;
- signing/broadcast.

Gate:

- verified ABI/fixture vectors;
- historical/captured transaction decoding parity;
- unknown versions fail closed;
- full regression suite green.

After M7: **STOP and review.**

## M8 — Four.meme read-only decoder

Only after M7 review and verified Four.meme contracts/ABI/fixtures are frozen.

Implement read-only:

- pending CREATE recognition;
- canonical CREATE/BUY/SELL/liquidity decoding supported by verified evidence;
- explicit verified mode/version identification;
- unknown mode/version refusal.

Explicitly NOT M8:

- quote engine;
- protocol fee/tax economics;
- Helper quote parity;
- execution plan;
- signing/broadcast.

Gate:

- verified ABI/fixture vectors;
- normal/tax/X/AntiSniper modes are decoded only where evidence proves them;
- unknown modes fail closed;
- historical/captured transaction decoding parity;
- full regression suite green.

After M8: **STOP and review.**

## M9 — combined 30-minute real BSC capture

Run Flap and Four.meme simultaneously on the single shared BSC chain ingest.

Required:

- one original pending timestamp origin;
- independent canonical launch census;
- canonical reconnect/backfill and reorg handling active;
- per-provider pending coverage/gaps retained;
- raw evidence integrity;
- separate venue reports;
- no signing/broadcast.

Decision point: compare launch count, pending coverage, lead time, deterministic asset-id availability, decoded modes/taxes where observable, and market activity.

After M9: **STOP and analyze the real dataset before economics implementation.**

## Mandatory post-M9 economics gate — before M10

Only after the combined real capture identifies the actual deployed protocol versions/modes:

- implement/freeze Flap quote + fee/tax economics and verify local results against authoritative/on-chain `eth_call` evidence where available;
- implement/freeze Four.meme mode-aware quote + fee/tax economics and verify against authoritative Helper/on-chain evidence where available;
- model unknown economic modes as UNKNOWN/UNSUPPORTED, never zero/default;
- prove actual BSC gas/revert/replacement cost accounting;
- freeze the economic model before Strategy C multichain research.

M10 must not start until this gate passes.

## M10 — Strategy C multichain

Add `STRATEGY_C_MULTICHAIN_V1`.

Do not modify legacy C.

## M11 — BSC replay

Run full matrix separately for Flap and Four.meme.

Freeze best hypotheses before new data.

## M12 — BSC PAPER

Real feeds, no broadcast.

Measure:

- pending receive -> decode
- decode -> decision
- decision -> enqueue

## M13 — controlled send/inclusion test

Use simple BSC transactions first.

Measure:

- build
- sign
- submit
- ACK
- inclusion
- receipt

## M14 — venue execution test

Very small controlled venue test only after quote/cost semantics are frozen.

Measure actual CREATE/BUY ordering and costs.

## M15 — ordered backrun integration

Add builder-provider adapters.

Prove:

- target CREATE included
- BUY follows CREATE
- no accidental pre-CREATE revert
- cost accounting exact

## M16 — forward capture

Freeze candidate, collect new data, no retuning.

## M17 — GUI market selector

Expose:

```
Solana / Pump.fun
BSC / Flap.sh
BSC / Four.meme
```

Each profile shows:

- READY / RESEARCH / DISABLED
- feed health
- selected strategy
- PAPER/LIVE mode
- current network
- cost model status

Switching profile restarts the runtime cleanly; never hot-swap chain state inside an active run.

---

# 20. GUI/backend boundary

The GUI sends desired configuration.

The backend remains authoritative.

Example:

```json
{
  "market_profile_id": "bsc-mainnet:flap",
  "strategy_profile_id": "strategy_c_multichain_v1",
  "mode": "PAPER"
}
```

Backend validates:

- profile exists
- chain/venue compatible
- required feed configured
- decoder version frozen
- cost model ready
- wallet/network match in LIVE
- PAPER never broadcasts

GUI cannot bypass readiness gates.

---

# 21. Lessons from existing public bots/projects

Use public projects as **reference implementations**, not as unquestioned source of truth.

## 21.1 `1chimaruGin/bsc-mempool`

Useful architecture ideas:

- local synced `bsc-geth`
- IPC/WS hot feed
- pending transaction filtering
- Rust/tokio low-latency pipeline
- shadow / tiny / full phase gates
- disk-persisted nonce manager
- MEV relay primary path + local node fallback
- separate public-mempool and confirmed strategy books
- paper mode before live

Borrow the architecture pattern.

Do not copy implementation code unless its license is explicitly compatible and reviewed.

## 21.2 `four-meme-community/four-meme-ai`

Useful Four.meme reference:

- MIT licensed
- TokenManager2 creation/trading
- Helper quote flow
- contract addresses
- event names
- TaxToken handling
- X Mode / AntiSniperFeeMode awareness
- API create flow

Use it to build fixtures and cross-check ABI/method semantics.

Do not make its private API a hot-path dependency for observing public launches.

## 21.3 `bitquery/four-meme-api`

Useful for:

- independent mempool queries
- Four.meme protocol filtering
- validation of CREATE/BUY/SELL visibility
- cross-check during capture development

Do not use a high-level analytics API as the final hot path until measured against local-node/provider feeds.

## 21.4 Flap official/public references

Use Flap-maintained contract/interface repositories as source material for:

- Portal interfaces
- Vault interfaces
- deployed contract references
- test fixtures

A public Flap launchpad implementation also demonstrates:

- Portal launch flow
- `quoteExactInput`
- `swapExactInput`
- permit sell path
- migration to PancakeSwap
- CREATE2-style deployment

Any non-official project's claims about current versions/addresses must be independently verified on-chain before freezing DarkBot config.

## 21.5 BNB Chain MEV documentation

Use the official BNB Chain builder/PBS documentation for the execution abstraction.

Design DarkBot around a provider-neutral builder interface because BSC supports multiple builder providers.

---

# 22. What we should copy vs. not copy

## Safe to reuse conceptually

- local full-node hot feed
- pending tx first-seen timestamps
- selector/address prefilter
- persistent nonce manager
- builder abstraction
- relay fallback
- shadow/PAPER -> tiny -> full operational phases
- read-only verification scripts
- ABI fixture testing
- exact receipt reconciliation

## Reuse only with license + verification

- source code
- ABI JSON copied from third-party repos
- contract addresses
- protocol constants
- calldata encoders

## Never copy blindly

- private keys
- hard-coded wallets
- undocumented RPCs
- magic gas constants
- claims of profitability
- approximate bonding-curve formulas
- anti-sniper bypass tricks
- transaction ordering assumptions

---

# 23. Performance contract — optimize toward 0 ms, hard local entry deadline 3 ms

The performance objective is to remove every avoidable local delay. **0 ms is the optimization direction.**

For future BSC LIVE entries, the hard local deadline is:

```
T_PENDING_SEEN -> T_SOCKET_HANDOFF <= 3,000,000 ns
```

where `T_SOCKET_HANDOFF` means the fully built and signed transaction has been handed to an already-open sender/network transport. It does not mean ACK or on-chain inclusion.

If the 3 ms deadline has already expired before handoff, the new entry is not sent and is recorded as `LOCAL_ENTRY_DEADLINE_EXCEEDED`. Late entries remain in statistics. The rule does not suppress exits/risk-reduction actions.

Internal performance targets should be materially below the hard deadline:

```
pending receive -> venue recognized      p50 < 0.10 ms
recognized -> fully decoded              p50 < 0.50 ms
pending receive -> decision              p50 < 1.00 ms
decision -> enqueue                      p50 < 0.10 ms
pending receive -> socket handoff        p99 < 3.00 ms
```

Measure p50/p95/p99/max and raw samples for:

- receive -> recognize;
- receive -> decode;
- receive -> decision;
- decision -> enqueue;
- queue wait;
- transaction build;
- signing;
- receive -> socket handoff;
- socket handoff -> ACK.

The full hot path through socket handoff must contain no waiting DB/filesystem/metadata/RPC quote fetch, no per-trade connection establishment, and no adapter-induced process/task/serialization hop. Profiles/adapters are resolved at startup; contracts/selectors/templates/connections and freshness-gated nonce/gas/protocol state are prepared outside the hot path. Missing/stale required state blocks the trade rather than causing a synchronous fetch.

Phase A must also run the same real wall-clock receive->decode->decision->enqueue benchmark before and after the adapter refactor on the same hardware. Replay parity alone is not a performance test.

Both Flap and Four.meme must later be benchmarked simultaneously under combined ingest load.

These are local engineering deadlines, not promises of 3 ms to canonical BSC execution. Feed/provider, sender, builder placement, ACK, inclusion and canonical ordering are measured separately.

Storage remains off the hot path through the existing writer architecture.

---

# 24. Provider strategy

Development order:

1. hosted BSC WSS/RPC for correctness
2. compare at least two pending feeds
3. local `bsc-geth` node for true mempool visibility and stable timestamps
4. only then evaluate paid low-latency providers
5. builder/relay paths are measured independently

For every provider record:

- first-seen pending timestamp
- missing pending transactions
- duplicates
- disconnects
- block lag
- receipt lag
- RTT
- rate limits

No provider is selected from marketing claims alone.

---

# 25. Research decision criteria

The goal is not to declare Flap or Four.meme better in advance.

The first synchronized capture decides.

Continue a venue only if:

- launch count is sufficient
- public pending coverage is useful
- future asset/action parameters are actionable before inclusion
- quote/fee semantics can be reproduced exactly
- pessimistic Strategy C EV remains positive under measured latency/cost
- PnL is not dominated by one or two launches
- forward data confirms it

Because both venues share the BSC chain layer, supporting the second venue after the first should be substantially cheaper than adding a new chain.

---

# 26. Definition of clean separation

The design is correct only if all these are true:

### Adding a new BSC launchpad

Requires:

- new `venues/<name>/`
- ABI/config
- fixtures/tests
- no edits to BSC mempool implementation
- no edits to Strategy C core

### Adding a new EVM chain

Requires:

- new `chains/<chain>/`
- chain config
- provider config
- no edits to Flap/Four code unless that venue actually supports the new chain

### Changing GUI selection

Requires:

- market profile registry entry
- no trading logic in frontend

### Running Flap vs Four

Uses:

- same BSC clock
- same pending feed
- same block feed
- same chain fee framework
- same core Strategy C
- separate venue decode/quote/cost/execution implementations

That is the target architecture.

---

# 27. Immediate next work

Do these in order:

1. freeze current Solana test baseline
2. create chain/venue interfaces
3. wrap Solana/Pump and prove exact parity
4. add generic IDs/units without breaking old DBs
5. implement EVM/BSC pending + canonical capture
6. add shared multi-venue dispatcher
7. implement Flap read-only decoder
8. implement Four.meme read-only decoder
9. run one 30-minute BSC capture for both
10. measure which venue actually provides the better Strategy C input
11. only then implement exact quotes/costs and full replay
12. live execution remains later

Do **not** build two independent BSC bots. Build one BSC chain layer with two isolated venue adapters.
