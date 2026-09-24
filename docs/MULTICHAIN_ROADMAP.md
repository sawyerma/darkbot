# DarkBot Multichain Roadmap

Baseline: `main@84b5d95c345261829a53a3cf76e497f7859ae45e`

Goal: evolve DarkBot from a Solana/Pump.fun-specific runtime into a multichain launch-trading research/execution framework where one configuration selects a chain + venue combination, e.g.

- Solana + Pump.fun
- BNB Chain + Four.meme
- later: another chain + another launchpad

The existing Solana behavior, historical replays, Strategy C results, and current tests must remain reproducible while BSC is added.

---

## 1. Architectural rule

Do **not** model "BSC" as another Pump-like protocol inside the current Solana classes.

Separate three layers:

1. **Core** — chain/venue independent:
   - strategy lifecycle
   - portfolio/positions
   - replay
   - storage
   - reporting
   - latency telemetry
   - qualification
   - research sweeps

2. **Chain adapter** — chain mechanics:
   - network transport
   - transaction identity
   - block/slot identity
   - pending/mempool semantics
   - native asset/base unit
   - network fee model
   - transaction build/sign/send
   - confirmation/finality

3. **Venue adapter** — launchpad/protocol mechanics:
   - CREATE detection
   - BUY/SELL decoding
   - creator identity
   - protocol state
   - quote math
   - protocol fees
   - migration/completion
   - exact transaction templates

This is mandatory because BNB Chain is a chain, while Four.meme is a venue. Future BSC launchpads should reuse the BSC chain adapter, and future Solana launchpads should reuse the Solana chain adapter.

---

## 2. Current coupling that must be broken

The current repository is not yet multichain. The main coupling points are:

- `darkbot/paper.py`
  - constructs `PumpDecoder`, `MarketReducer`, and Pump-specific `QuoteEngine` directly
  - portfolio is expressed in lamports
  - actionable CREATE requires SOL/WSOL quote scope

- `darkbot/market.py`
  - Pump Borsh decoder
  - Pump event names/layouts
  - Solana signature/slot/instruction-index identity
  - curve fields use virtual SOL/token reserves

- `darkbot/execution.py`
  - Pump bonding-curve quote model
  - lamport accounting
  - Solana priority-fee/tip/rent semantics

- `darkbot/fees.py`
  - Solana `getRecentPrioritizationFees`
  - `getFeeForMessage`
  - SPL rent/account assumptions
  - Pump protocol fee-account decoding

- `darkbot/capture.py`
  - Solana `logsSubscribe`
  - PumpPortal-specific transport

- `darkbot/config.py`
  - Solana/Pump-specific schema and cross-checks
  - lamports
  - SOL/USD FX
  - Pump event layouts

- `darkbot/domain.py`
  - much of the lifecycle is reusable
  - but many field names are Solana-specific: `mint`, `slot`, `*_lamports`, `CurveState`

The correct approach is an incremental adapter refactor, not a rewrite.

---

## 3. Target package structure

Target structure:

```
darkbot/
    core/
        domain.py
        engine.py
        strategy.py
        replay.py
        reporting.py
        storage.py
        clock.py
        qualification.py

    chains/
        base.py
        registry.py

        solana/
            chain.py
            feeds.py
            fees.py
            execution.py
            tx.py
            units.py

        evm/
            base.py
            rpc.py
            mempool.py
            fees.py
            tx.py

        bsc/
            chain.py
            feeds.py
            execution.py
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

        fourmeme/
            protocol.py
            abi.py
            decoder.py
            reducer.py
            quote.py
            execution.py

    strategies/
        base.py
        strategy_a_fast_creator.py
        strategy_b_legacy_5buy.py
        strategy_c_legacy_backend.py
        strategy_c_multichain.py
```

Do **not** physically move every existing file at once. First introduce interfaces/wrappers around the current modules, prove exact Solana parity, then move files in later cleanup commits.

---

## 4. Runtime selection

The runtime config becomes explicit about chain and venue.

Example Solana profile:

```json
{
  "market": {
    "chain": "solana-mainnet",
    "venue": "pumpfun"
  }
}
```

Example BSC profile:

```json
{
  "market": {
    "chain": "bsc-mainnet",
    "venue": "fourmeme"
  }
}
```

The configuration file remains the source of truth. Do not add a hidden CLI override that changes the chain without affecting the config hash.

Suggested configs:

```
config/solana_pumpfun_strategy_c.json
config/bsc_fourmeme_strategy_c.json
```

The normal command stays conceptually identical:

```
python -m darkbot capture --config config/bsc_fourmeme_strategy_c.json ...
python -m darkbot paper   --config config/bsc_fourmeme_strategy_c.json ...
python -m darkbot replay  --config config/bsc_fourmeme_strategy_c.json ...
```

---

## 5. Core interfaces

### 5.1 ChainAdapter

Create `darkbot/chains/base.py`.

Required contract:

```python
class ChainAdapter(Protocol):
    chain_id: str
    native_symbol: str
    native_decimals: int

    def build_feeds(self, cfg, clock, callbacks): ...
    def build_fee_resolver(self, cfg, clock): ...
    def build_execution_adapter(self, cfg, clock, scheduler): ...
    def tx_id(self, raw) -> str | None: ...
    def block_ref(self, raw) -> str | int | None: ...
```

The chain adapter knows **network mechanics only**. It must not know Strategy C.

### 5.2 VenueAdapter

Create `darkbot/venues/base.py`.

Required contract:

```python
class VenueAdapter(Protocol):
    venue_id: str
    chain_id: str

    def decoder(self, cfg): ...
    def reducer(self, cfg, run_id): ...
    def quote_engine(self, cfg): ...
    def is_actionable_launch(self, state) -> bool: ...
    def creator_of(self, state) -> str | None: ...
```

The venue adapter knows protocol semantics, but not RPC transport.

### 5.3 Runtime assembly

Create registries:

```
chains/registry.py
venues/registry.py
```

Then runtime assembly becomes:

```python
chain = load_chain(cfg.market["chain"])
venue = load_venue(cfg.market["venue"])

assert venue.chain_id == chain.chain_id

components = RuntimeComponents(
    chain=chain,
    venue=venue,
    feeds=chain.build_feeds(...),
    fee_resolver=chain.build_fee_resolver(...),
    decoder=venue.decoder(cfg),
    reducer=venue.reducer(cfg, run_id),
    quote_engine=venue.quote_engine(cfg),
)
```

`StateOwner` receives those objects instead of constructing `PumpDecoder` and `QuoteEngine` itself.

This is the main seam that makes the project multichain.

---

## 6. Domain model migration

Do not destroy the current Solana names in one commit.

### Phase A — additive generic fields

Introduce generic concepts:

- `asset_id` — Solana mint or EVM token address
- `tx_id` — Solana signature or EVM transaction hash
- `block_ref` — Solana slot or EVM block number/hash
- `native_amount_base_units` — lamports on Solana, wei on BSC
- `native_symbol` — SOL or BNB
- `native_decimals` — 9 for SOL, 18 for BNB
- `chain_id`
- `venue_id`
- `signal_origin`

Keep the current fields temporarily:

- `mint`
- `slot`
- `sol_amount`
- `*_lamports`

Solana writes both old and generic fields during the transition. BSC writes only generic fields plus compatibility values where useful.

### Phase B — internal generic accounting

After parity tests pass, rename internal economic concepts:

```
cash_lamports              -> cash_native
reserved_lamports          -> reserved_native
net_pnl_lamports           -> net_pnl_native
network_fee_lamports       -> network_fee_native
tip_lamports               -> tip_native
```

Do this internally first. Database legacy columns can remain for historical compatibility until a later migration.

---

## 7. Normalized market contract

Strategy code must not receive raw Solana logs or raw EVM logs.

The normalized event contract becomes:

```python
NormalizedEvent(
    event_type=CREATE|BUY|SELL|COMPLETE|MIGRATE,
    chain_id=...,
    venue_id=...,
    asset_id=...,
    actor=...,
    creator=...,
    native_amount_base_units=...,
    token_amount_base_units=...,
    tx_id=...,
    block_ref=...,
    event_index=...,
    receive_mono_ns=...,
    venue_payload=...
)
```

Venue-specific reserves/curve fields live in `venue_payload` or a venue state object and are consumed by that venue's quote engine.

Strategy C should consume only chain-neutral features wherever possible:

- buy count
- unique buyers
- sell count
- creator sold
- creator initial-buy fraction
- native/USD volume
- market-cap USD
- price/target multiple
- elapsed time

Do not use raw `largest_buy_sol` as the permanent multichain feature. Preserve it only for the exact legacy Solana Strategy C contract.

---

## 8. Strategy C policy

Do **not** modify `LEGACY_BACKEND_FILTER_V1` semantics. It is historical evidence and must remain reproducible.

Add:

```
STRATEGY_C_MULTICHAIN_V1
```

Initial BSC test configuration:

- observation: 0 ms
- filters: none
- first actionable launch
- target sweep: 1.5x / 2x / 3x / 3.5x
- timeout sweep initially: 250 / 500 / 750 / 1000 / 2000 ms
- creator-sell exit
- one entry per asset
- full gas/protocol/relay/slippage accounting

Later filters must use generic or USD-normalized features.

---

## 9. Solana adapter migration

Before BSC, wrap the existing Solana path with adapters without changing economics.

### Solana chain adapter

Move/wrap:

- `logsSubscribe` feed logic from `capture.py`
- Helius/RPC endpoints
- Solana fee resolver
- transaction signature/slot identity
- later live sender

### Pump.fun venue adapter

Move/wrap:

- `PumpDecoder`
- Pump `MarketReducer` behavior
- Pump quote engine
- Pump fee-state interpretation
- migration/completion semantics

### Required gate

Run the **entire existing test suite**.

Additionally run the clean 30-minute replay and require exact parity for:

- fill count
- resolved/unresolved
- target exits
- pessimistic PnL
- realized PnL
- PF
- strategy decisions
- canonical event IDs or a documented deterministic migration mapping

No BSC implementation starts until this passes.

---

## 10. BSC chain adapter

BNB Chain is implemented as an EVM chain adapter, not directly inside Four.meme.

### 10.1 Generic EVM layer

`chains/evm/` provides:

- JSON-RPC/WebSocket transport
- `eth_subscribe`
- pending transaction handling
- block/log subscriptions
- nonce/replacement tracking
- receipt parsing
- gas accounting
- transaction building/signing
- receipt/revert semantics

### 10.2 BSC specialization

`chains/bsc/` provides:

- chain/network identity
- BNB native asset
- BSC RPC endpoints
- BSC block timing telemetry
- provider-specific pending-tx feeds
- optional local-node feed
- optional relay/backrun sender

No Four.meme ABI belongs here.

---

## 11. Four.meme venue adapter

The first BSC venue is Four.meme.

Implementation order:

1. freeze verified contract addresses and ABI/source version
2. identify exact token-create transaction selector/event
3. decode pending CREATE transaction input
4. determine whether the future token address is deterministically available before inclusion
5. decode canonical CREATE/BUY/SELL logs
6. identify creator
7. reconstruct exact launch/curve state
8. implement exact quote math
9. implement protocol fee model
10. differential-test quotes against on-chain contract calls/historical transactions

Never infer price from UI screenshots or approximate curves.

---

## 12. BSC pending state model

BSC needs states that Solana/Pump currently does not.

Add:

```
PENDING_CREATE
CREATED_ACTIVE
CREATE_REVERTED
CREATE_DROPPED
CREATE_REPLACED
COMPLETED
MIGRATED
```

A pending transaction is **not canonical evidence**.

For every pending CREATE record:

- first_seen_mono_ns
- provider
- tx_hash
- sender
- nonce
- gas parameters
- decoded venue call
- predicted asset_id if determinable
- replacement tx hash if replaced
- inclusion block/tx index if included
- revert status
- drop timeout

This allows us to measure real public-mempool coverage instead of assuming every launch is visible.

---

## 13. Same-block ordering

Seeing CREATE early is not enough.

A BUY submitted before CREATE executes can revert if the token/venue state does not exist yet.

Therefore BSC execution research must support two modes:

### A. POST_CREATE

Wait until canonical CREATE is observed, then BUY.

Useful baseline, but loses most mempool advantage.

### B. BACKRUN_CREATE

Observe the pending CREATE and send an execution request designed to be ordered immediately **after** the creator transaction in the same block.

This is the strategically important BSC path.

Required telemetry:

- target CREATE tx hash
- CREATE transaction index in block
- BUY transaction index in block
- whether BUY executed after CREATE
- gas/relay cost
- failed/reverted attempts
- signal -> submit
- signal -> BUY inclusion
- CREATE -> BUY ordering distance

Do not call a pending signal an executable edge until same-block ordering has been demonstrated.

---

## 14. Unified latency model

This must be fixed before comparing Solana and BSC.

Record these timestamps for every chain:

```
T_SIGNAL       earliest actionable signal seen locally
T_DECODE       normalized data ready
T_DECISION     strategy decision complete
T_ENQUEUE      execution intent enqueued
T_SUBMIT       transaction sent
T_ACK          sender/provider acknowledgement
T_CREATE_EXEC  target CREATE canonical execution/inclusion
T_BUY_EXEC     our BUY canonical execution/inclusion
```

Derived metrics:

```
signal_to_decision
signal_to_submit
signal_to_buy
signal_lead_to_create = T_CREATE_EXEC - T_SIGNAL
buy_offset_from_create = T_BUY_EXEC - T_CREATE_EXEC
```

Reference rule:

- current Solana `processed` signal is the old baseline
- BSC pending can have positive pre-create lead
- comparisons use actual timestamps, not overloaded labels such as "0 ms total latency"

---

## 15. Storage migration

Use additive migrations only.

Add to `runs`:

- chain_id
- network_id
- venue_id
- native_symbol
- native_decimals

Add to observations/events:

- chain_id
- venue_id
- tx_id
- block_number
- block_hash
- tx_index
- event_index
- signal_origin
- first_seen_mono_ns
- canonical_seen_mono_ns
- native_amount_base_units

Add new table:

```
pending_transactions
```

Suggested fields:

- run_id
- chain_id
- venue_id
- tx_hash
- sender
- nonce
- first_seen_mono_ns
- last_seen_mono_ns
- decoded_action
- asset_id
- target_contract
- gas_price/effective fee fields
- replacement_tx_hash
- final_status
- included_block
- included_tx_index
- receipt_status

Do not rewrite historical Solana DB files.

---

## 16. Cost model

### Solana

Keep current verified components:

- protocol fee
- creator fee
- base fee
- priority fee
- sender tip
- account rent/close refund
- failed-attempt cost

### BSC

New cost components:

- protocol buy/sell fee
- gas used × effective gas price
- relay/builder/backrun tip
- approval gas if protocol requires a separate approval
- failed/reverted transaction gas
- slippage
- any token/venue-specific tax only if verified

All costs are in wei first, then converted to BNB/USD for reporting.

A positive BSC edge is invalid unless failed/reverted attempts are charged pessimistically.

---

## 17. FX

Generalize `fx.py`.

Current SOL/USD-specific behavior becomes:

```
native_asset -> USD
```

Profiles:

- Solana: SOL/USD
- BSC: BNB/USD

Strategy sizing remains possible in USD per slot.

---

## 18. Execution adapters

Preserve PAPER safety.

Interfaces:

```
PaperExecutionAdapter
SolanaLiveExecutionAdapter   # future
BscLiveExecutionAdapter      # future
```

PAPER/replay never broadcasts.

BSC live implementation must not be enabled during capture/research phases.

The BSC adapter eventually needs:

- hot nonce handling
- prebuilt transaction templates
- signing
- direct/low-latency sender
- backrun/bundle path if selected
- receipt observation
- replacement/cancel semantics

---

## 19. First BSC experiment — capture only

Do this before changing Strategy C economics.

Run a standalone/integrated **30-minute Four.meme capture**.

For every launch record:

- pending first-seen timestamp
- CREATE inclusion timestamp
- public-mempool visibility
- token address known before inclusion: yes/no
- pending lead time
- creator
- initial creator/dev buy
- all subsequent BUY/SELL events
- price/curve state
- gas
- transaction ordering
- revert/replacement/drop status

Primary output:

```
launches_total
pending_seen_before_create
pending_coverage_pct

lead_ms:
  min
  p50
  p95
  p99
  max

token_known_pre_create_pct
```

This answers whether BSC actually gives DarkBot the expected timing advantage.

---

## 20. First Strategy C replay on BSC

After decoder/quote/cost validation:

Run the same capture through `STRATEGY_C_MULTICHAIN_V1`.

Latency sweep:

```
0 ms
1 ms
2 ms
3 ms
5 ms
10 ms
25 ms
50 ms
100 ms
150 ms
200 ms
300 ms
```

Also model exact same-block backrun ordering separately from post-create execution.

For every scenario report:

- eligible launches
- entry attempts
- fills
- failed/reverted
- unresolved
- target exits
- creator exits
- timeouts
- realized PnL
- pessimistic PnL
- EV/resolved
- EV/economic attempt
- PF
- win rate
- capital exposure
- gas/relay/protocol cost
- chronological thirds
- profit concentration

Do not rank on realized PnL. Use pessimistic PnL.

---

## 21. Engineering go/no-go gates

These are project decision gates, not claims about BSC.

### Gate B1 — feed useful

Continue if the capture shows a material share of launches before inclusion.

Initial diagnostic target:

- pending coverage >= 70%
- p50 pending lead >= 50 ms

Strong result:

- coverage >= 90%
- p50 lead >= 100 ms

If those fail, investigate another provider/local node before rejecting the chain.

### Gate B2 — token actionable pre-create

Need either:

- deterministic token address/required BUY parameters before inclusion, or
- a reliable same-block mechanism that can reference/create the required state

If neither exists, public mempool visibility alone is not enough.

### Gate B3 — quote/cost exactness

No economic replay until:

- protocol math differential tests pass
- gas model is measured
- reverted attempts are charged
- protocol fees are verified

### Gate B4 — 30-minute edge

A 30-minute run is only a diagnostic.

Continue if pessimistic EV is positive under measured latency/cost assumptions.

### Gate B5 — forward validation

Before any live-money claim:

- freeze one candidate
- new capture
- >= 1,000 launches or an equivalent sufficiently large out-of-sample dataset
- positive pessimistic EV
- positive chronological splits
- no single launch dominates PnL
- unresolved exposure below threshold

---

## 22. Tests

Add test groups.

### Core

- chain registry
- venue registry
- unsupported chain/venue mismatch
- generic amount/asset identity
- config hashing includes chain+venue
- replay deterministic across adapters

### Solana regression

Every current test remains green.

Add explicit Solana parity tests against the current clean 30-minute capture.

### EVM/BSC

- pending tx decoding
- nonce replacement
- dropped pending tx
- reverted create
- canonical receipt/log decode
- reorg handling
- gas charging on revert
- fee accounting
- transaction ordering
- Four.meme create decode
- Four.meme buy/sell decode
- quote differential vectors

### Strategy C

- same policy behavior across normalized Solana/BSC fixtures
- no accidental SOL-specific feature usage in multichain strategy
- latency sweep reproducibility

---

## 23. Implementation sequence

### M0 — freeze baseline

- record current main SHA
- full test suite green
- freeze current Solana 30-minute result
- no semantic changes

### M1 — introduce adapter contracts

Files:

```
darkbot/chains/base.py
darkbot/chains/registry.py
darkbot/venues/base.py
darkbot/venues/registry.py
```

No behavior change.

### M2 — wrap existing Solana/Pump path

Create:

```
darkbot/chains/solana/
darkbot/venues/pumpfun/
```

Inject decoder/reducer/quote/fee/feed components into `StateOwner` and `Application`.

Gate: exact Solana parity.

### M3 — generic config/storage IDs

Add `market.chain`, `market.venue`, generic chain/venue columns, generic asset/tx/block identities.

Gate: old DB/replay compatibility.

### M4 — generic EVM/BSC capture layer

Create:

```
darkbot/chains/evm/
darkbot/chains/bsc/
```

Implement pending + canonical block/log capture only.

No trading logic.

### M5 — Four.meme decoder

Create:

```
darkbot/venues/fourmeme/
```

Freeze ABI/contracts, decode pending creates and canonical trades.

### M6 — 30-minute BSC capture

Measure pending coverage and lead time.

This is the first decision point.

### M7 — Four.meme quote + fee model

Implement exact math and gas model.

Differential tests required.

### M8 — Strategy C multichain

Add `STRATEGY_C_MULTICHAIN_V1`.

Run BSC latency/target/timeout replay.

### M9 — BSC PAPER

Real feed, no broadcast.

Measure local signal -> decision -> enqueue path.

### M10 — controlled execution-path measurement

Measure BSC transaction submission/inclusion separately.

No assumption that RPC ACK equals execution.

### M11 — backrun research

If public pending CREATE is useful, test same-block CREATE->BUY ordering with the selected relay/builder path.

### M12 — fresh forward validation

Freeze candidate and validate on a new dataset.

Only after this consider live execution.

---

## 24. What should remain untouched initially

Do not rewrite these before adapter parity exists:

- existing Strategy A
- existing Strategy B
- `LEGACY_BACKEND_FILTER_V1`
- clean-v2 Solana dataset
- current Solana research status
- historical replay semantics
- old database files

The multichain work must be additive.

---

## 25. Definition of success

The architecture is complete when this works without strategy/core forks:

```
Solana + Pump.fun
    -> normalized market events
    -> Strategy C
    -> generic portfolio/replay/reporting

BSC + Four.meme
    -> normalized market events
    -> Strategy C
    -> generic portfolio/replay/reporting
```

Adding another chain later should require:

1. one new chain adapter,
2. optionally a reusable EVM/Solana base,
3. one venue adapter,
4. config + fixtures,
5. no edits to Strategy C core.

That is the target design.
