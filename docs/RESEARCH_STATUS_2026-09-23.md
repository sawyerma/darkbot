# DarkBot Research / Latency Status — 2026-09-23

## 0. Purpose

This document freezes the current investigation state for the Pump.fun launch-sniper / Strategy C work.

It records:

- what the old backend/research actually showed,
- what has been reproduced on the current clean 30-minute capture,
- what was only a best-case PAPER/replay assumption,
- what latency has now been measured,
- what is still unknown,
- and the exact next steps before any claim of a live-tradable edge.

The central rule for all further work is:

> **Never call a scenario "0 ms total latency".**
>
> `entry_delay_ms = 0` only means zero *additional modeled submit-to-fill delay* inside the PAPER/replay execution model. It is not zero time from the real Pump.fun CREATE on Solana to a real BUY landing on-chain.

---

## 1. Repository / implementation state before this status commit

Branch:

`main`

Last verified HEAD before this document:

`4af978e18f91dea5d921ed7ef284b15d55d6c987` — `Make writer shutdown drain lossless`

Recent relevant commits before this document:

- `4af978e...` — writer shutdown drain lossless
- `d8e3c6f...` — fee retry backoff from connectivity config
- `70b02d0...` — fee retries + replay freshness bound to source
- `c50670a...` — economic fee-evidence gate + replay diagnostics
- `e16a0a6...` — RAM-only Strategy C 30m analysis script

Last verified unit suite during this investigation:

- **253 / 253 tests PASS**

No production code was changed for the legacy-compatibility replay experiments described below. The legacy evidence compatibility behavior was injected only in RAM for diagnostics.

---

## 2. Clean source recording used for the current investigation

Database:

`data/market_capture_30m_clean_v2.sqlite`

Run:

`market_capture_30m_clean_v2_capture_20260923T112208Z_994750`

Dataset hash used by the analyses:

`8dbc5bf633f4c589fa816ce637fd18c2dd85383932de6e36d4209b49938b3636`

### 2.1 Capture quality

Clean-v2 capture summary:

- status: `COMPLETED`
- quality: `OK`
- economic readiness: `ECONOMIC_PAPER_READY`
- Helius messages: **133,209**
- existing RPC messages: **133,190**
- CREATEs observed: **512**
- capture-only actionable CREATEs: **447**
- raw observations: **266,399**
- writer committed records: **303,679**
- writer failures: **0**
- writer quarantined records: **0**
- disk error: **false**
- slow shutdown: **false**
- shutdown drain: about **2.658 ms**
- economic profile samples: **130**
- fee snapshots: **193**
- fee raster complete/fresh: about **99.8%**

This recording was produced after the writer-shutdown loss fix and is the current clean reference dataset.

---

## 3. Historical backend / old research: what it actually showed

### 3.1 PRE_PUMP was not the historical positive hypothesis

Historical PRE_PUMP-style filtering was not the strongest result. Historical research already showed PRE_PUMP itself negative in important 500 ms-style scenarios.

The later research change (`R-005`) intentionally tested a different hypothesis:

- `observation_ms = 0`
- direct CREATE -> entry decision
- no filters
- theoretical `entry_latency_ms = 0`
- target around `1.5x` in the first best-case diagnostic
- creator-sell exit enabled

The original research explicitly treated 0 ms as a **theoretical best-case diagnostic**, not as a statement that the real bot could buy 0 ms after the on-chain CREATE.

### 3.2 Historical old-backend modeled dataset

Historical analysis basis:

- launches: **335**
- tradable launches: **249**
- launches with full 10-minute path: **93**

Selected historical modeled results:

| Target | Entry delay | Hold/timeout | Net PnL | Profit Factor |
|---|---:|---:|---:|---:|
| 1.5x | 0 ms | 0.5 s | +0.610 SOL | 1.85 |
| 1.5x | 0 ms | 1 s | +0.331 SOL | 1.26 |
| 1.5x | 0 ms | 2 s | +0.275 SOL | 1.17 |
| 1.5x | 0 ms | 5 s | -0.014 SOL | 0.99 |
| 2.0x | 0 ms | 0.5 s | +0.707 SOL | 1.99 |
| 2.0x | 0 ms | 1 s | +0.498 SOL | 1.39 |
| 3.5x | 0 ms | 0.5 s | +1.306 SOL | 2.83 |

Historical latency sensitivity for one 2x / 1s scenario:

| Modeled latency | Net PnL | PF |
|---:|---:|---:|
| 0 ms | +0.707 SOL | 1.99 |
| 2 ms | +0.060 SOL | 1.05 |
| 3 ms | -0.180 SOL | 0.86 |
| 5 ms | -0.498 SOL | 0.61 |
| 25 ms | -0.862 SOL | 0.33 |
| 100 ms | -0.847 SOL | 0.35 |

This historical result established **extreme modeled sensitivity to very early entry state**, but did **not** prove a real end-to-end 0–2 ms chain-to-fill capability.

### 3.3 Historical microfilter observation

An old microfilter experiment using approximately:

- >= 3 BUYs,
- 25–100 ms observation,
- then 100 ms modeled latency,
- target about 2x / 1s,

showed PF around **2.3–2.4** on the full old dataset, but the final chronological third was negative.

That makes it an in-sample research lead, not validated forward edge.

### 3.4 Historical PAPER winner must not be misrepresented

One old PAPER target winner existed:

- mint beginning `2wj7...`
- target age about **2417 ms**
- net multiple about **1.51534**
- trade PnL about **+50,748,176 lamports**

However, cumulative PAPER PnL at that point was still about **-25,323,519 lamports** due to earlier losses.

Therefore:

> An individual old PAPER winner is evidence that the target path occurred, **not evidence that the whole old live-PAPER run was profitable**.

---

## 4. Current Strategy C launch-sniper configuration

Current config:

`config/strategy_c_launch_sniper_0ms_15x.json`

Core policy:

- strategy: `LEGACY_BACKEND_FILTER_V1`
- candidate: `LAUNCH_SNIPER_0MS_15X_BEST_CASE`
- `observation_ms = 0`
- `filters = []`
- entry trigger: `FIRST_ACTIONABLE_CREATE`
- target: `1.5x`
- ordinary stop disabled
- creator-sell exit enabled
- entry swap budget: **98,370,000 lamports**
- timeout in base config: **590,000 ms**
- entry re-entry: one submitted entry per mint/campaign
- quote asset: SOL only
- entry delay: **0 ms**
- exit delay: **0 ms**
- zero-delay run cannot qualify the edge
- slippage limit: **300 bps**
- entry max attempts: **1**
- exit max attempts: **3**
- fill evidence: `OBSERVED_STATE_PROXY`

Important old-backend compatibility point:

The historical Strategy C configuration had one `entry_latency_ms` field. The current Strategy C compatibility tests intentionally map that old latency to both modern entry and exit delay for the old-style equivalence test. New research can vary entry and exit independently.

---

## 5. Current strict replay results on clean-v2

### 5.1 Current PRE_PUMP-style test

Current PRE_PUMP / no-filter comparison work did **not** reproduce a robust positive edge under the current strict evidence semantics.

Selected no-filter 100 ms-latency results across observation windows were negative, e.g.:

- W0: resolved 50 / unresolved 112, PnL about **-0.0246 SOL**
- W250: resolved 48 / unresolved 142, about **-0.0435 SOL**
- W500: resolved 33 / unresolved 161, about **-0.1975 SOL**
- W750: about **-0.1752 SOL**
- W1000: about **-0.1779 SOL**

This is not the same hypothesis as the old zero-age launch-sniper test.

### 5.2 Base launch sniper, long timeout, strict current semantics

Config basis:

- target 1.5x
- observation 0
- entry/exit modeled delay 0 ms
- long timeout (590 s)

Result:

- CREATEs: **512**
- actionable for this strategy path: **204**
- entry intents: **204**
- entry fills: **204**
- resolved: **126**
- failed: **0**
- unresolved: **57**
- censored: **21**
- wins: **41**
- win rate: **32.54%**
- exits:
  - creator sell: **103**
  - target: **23**
- realized PnL: **+1.721447882 SOL**
- pessimistic PnL: **-3.885938518 SOL**
- EV / resolved: **+0.013662285 SOL**

Interpretation:

The closed positions were positive, but the unresolved exposure dominates the conservative result. This is not yet robust evidence.

### 5.3 Short timeout 1.5x / 500 ms, strict current semantics

Result:

- fills: **204**
- resolved: **110**
- unresolved: **94**
- wins: **33**
- win rate: **30.0%**
- timeout exits: **100**
- target exits: **10**
- realized PnL: **+1.626390844 SOL**
- pessimistic PnL: **-7.620877956 SOL**
- EV / resolved: **+0.014785371 SOL**

Unresolved diagnostic:

- total unresolved: **94**
- `ATTEMPT_STATUS_UNKNOWN`: **93**
- `QUOTE_CONFLICT_ON_OPEN_POSITION`: **1**
- 93 unknown cases had one exit attempt
- those 93 were timeout-origin exits
- no failed exit attempts
- no retry exhaustion

The dominant reason was the current conservative global unknown-frame evidence gate, not ordinary economic losses.

---

## 6. Legacy-compatible evidence diagnostic on clean-v2

For comparison only, a RAM-only monkeypatch was used to make the current reducer closer to the old per-mint evidence semantics:

- preserve per-mint decode gaps,
- preserve canonical/provider gaps,
- ignore global unattributable undecodable points for unrelated mints.

No repository file or database was changed by this monkeypatch.

### 6.1 1.5x / 500 ms, zero modeled delay

Legacy-compatible result:

- fills: **204**
- resolved: **203**
- unresolved: **1**
- wins: **51**
- target exits: **10**
- timeout exits: **193**
- realized PnL: **+1.778632001 SOL**
- pessimistic PnL: **+1.680256801 SOL**
- EV: **+0.008761734 SOL**
- one `QUOTE_CONFLICT_ON_OPEN_POSITION`

This demonstrated that the old short-horizon hypothesis can be reproduced on the clean-v2 market path **under old-like evidence treatment**.

It does not make the current strict replay qualification obsolete; it explains why the strict and old-style results diverge.

---

## 7. Target × timeout scan under legacy-compatible evidence

Targets tested:

- 1.5x
- 2.0x
- 2.5x
- 3.0x
- 3.5x

Timeouts tested:

- 250 ms
- 500 ms
- 750 ms
- 1000 ms
- 2000 ms
- 5000 ms

Zero entry/exit modeled delay.

Strongest in-sample region:

### 3.0x / 750 ms

- pessimistic PnL: **+3.173117 SOL**
- realized PnL: **+3.271492 SOL**
- EV: about **+0.016116 SOL**
- PF: about **8.657**
- target exits: **6**
- unresolved: **1**

### 3.5x / 750 ms

Numerically identical to the 3.0x / 750 ms run in this capture.

Interpretation:

The same observed state jumps crossed both thresholds, so the identical result does not mean the exact target is uniquely optimal.

The result is highly concentrated: only a small number of target exits drive a large fraction of profit. This must be quantified before any strategy freeze.

---

## 8. Joint modeled entry/exit latency scan

Target: 3.0x

Timeout: 750 ms

Entry and exit delay were first varied together.

Selected result:

- 0 ms: strongly positive
- 2 ms: still positive
- 3 ms: still positive
- 5 ms: around break-even / slightly negative
- >= 10 ms: negative

This initially looked like a generic latency cliff.

The later split-entry/exit tests showed that this interpretation was incomplete.

---

## 9. Entry latency and exit latency separated

Target: 3.0x

Timeout: 750 ms

Legacy-compatible evidence diagnostic.

### 9.1 Entry varied, exit fixed at 0 ms

| Entry | Fills | Resolved | Unresolved | TARGET | Pessimistic PnL | Realized PnL | PF |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 ms | 204 | 203 | 1 | 6 | +3.173117 | +3.271492 | 8.657 |
| 1 ms | 185 | 184 | 1 | 2 | +1.005810 | +1.104185 | 3.584 |
| 2 ms | 182 | 182 | 1 | 2 | +0.919543 | +1.022399 | 3.393 |
| 3 ms | 181 | 181 | 1 | 2 | +0.854306 | +0.957161 | 3.227 |
| 4 ms | 175 | 175 | 1 | 0 | -0.020366 | +0.082490 | 1.192 |
| 5 ms | 172 | 172 | 1 | 0 | -0.108438 | -0.005582 | 0.987 |
| 10 ms | 167 | — | 1 | 0 | -0.231532 | -0.128677 | 0.702 |
| 25 ms | 165 | — | 1 | 0 | -0.239750 | — | 0.683 |
| 50 ms | 161 | — | 1 | 0 | -0.358922 | — | 0.402 |
| 100 ms | 160 | — | 1 | 0 | -0.359632 | — | 0.406 |
| 250 ms | 154 | — | 1 | 0 | -0.460372 | — | 0.168 |
| 500 ms | 151 | — | 1 | 0 | -0.470580 | — | 0.114 |

The large change is associated with the **entry timing**, not the exit timing.

Entry-failed counts increased rapidly:

- 1 ms: **19**
- 2 ms: **21**
- 3 ms: **22**
- 4 ms: **28**
- 5 ms: **31**
- 6 ms: **34**
- 10 ms: **36**
- 25 ms: **38**
- 50 ms: **42**
- 100 ms: **43**
- 250 ms: **49**
- 500 ms: **52**

The exact underlying statuses behind `ENTRY_FAILED` are still being diagnosed and must be understood before interpreting this as a physical millisecond requirement.

### 9.2 Exit varied, entry fixed at 0 ms

Exit delay alone did **not** produce the same collapse.

Selected results:

| Entry | Exit | Pessimistic PnL | Realized PnL | PF |
|---:|---:|---:|---:|---:|
| 0 ms | 0 ms | +3.173117 | +3.271492 | 8.657 |
| 0 ms | 1 ms | +4.892566 | +5.089322 | 12.911 |
| 0 ms | 5 ms | +5.725397 | +5.922153 | 14.875 |
| 0 ms | 25 ms | +5.938483 | +6.135238 | 15.384 |
| 0 ms | 100 ms | +3.393088 | +3.786605 | 9.972 |
| 0 ms | 250 ms | +0.078669 | +1.062452 | 3.523 |

A delayed modeled exit can improve replay PnL because the observed-state proxy at the later fill time can be more favorable.

This must **not** be interpreted as "slower exits are better in real trading". It is a property of the replay fill-proxy/path assumptions.

---

## 10. Mixed entry / exit latency pairs

Target: 3.0x

Timeout: 750 ms

Measured mixed scenarios:

| Entry | Exit | Fills | Failed | Resolved | Unresolved | TARGET | Pessimistic PnL | Realized PnL | EV | PF |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 3 | 185 | 19 | 183 | 2 | 1 | +1.243482 | +1.440238 | +0.007870 | 4.374 |
| 1 | 5 | 185 | 19 | 183 | 2 | 1 | +1.243482 | +1.440238 | +0.007870 | 4.374 |
| 1 | 10 | 185 | 19 | 183 | 2 | 1 | +1.243561 | +1.440316 | +0.007871 | 4.374 |
| 2 | 4 | 182 | 21 | 181 | 2 | 1 | +1.157216 | +1.358452 | +0.007505 | 4.182 |
| 2 | 6 | 182 | 21 | 181 | 2 | 1 | +1.157216 | +1.358452 | +0.007505 | 4.182 |
| 2 | 10 | 182 | 21 | 181 | 2 | 1 | +1.157294 | +1.358530 | +0.007506 | 4.183 |
| 3 | 6 | 181 | 22 | 180 | 2 | 1 | +1.092056 | +1.293292 | +0.007185 | 4.012 |
| 3 | 8 | 181 | 22 | 180 | 2 | 1 | +1.092056 | +1.293292 | +0.007185 | 4.012 |
| 3 | 12 | 181 | 22 | 180 | 2 | 1 | +1.089504 | +1.290740 | +0.007171 | 4.006 |
| 6 | 10 | 169 | 34 | 169 | 1 | 0 | -0.197554 | -0.094698 | -0.000560 | 0.781 |
| 6 | 15 | 169 | 34 | 169 | 1 | 0 | -0.197369 | -0.094513 | -0.000559 | 0.781 |
| 6 | 20 | 169 | 34 | 169 | 1 | 0 | -0.197162 | -0.094306 | -0.000558 | 0.782 |
| 10 | 20 | 167 | 36 | 167 | 1 | 0 | -0.220615 | -0.117759 | -0.000705 | 0.725 |
| 20 | 30 | 167 | 36 | 167 | 1 | 0 | -0.221036 | -0.118181 | -0.000708 | 0.722 |
| 40 | 50 | 163 | 40 | 163 | 1 | 0 | -0.242716 | -0.139861 | -0.000858 | 0.674 |

### 10.1 Current interpretation

Within this replay model:

- entry <= 3 ms remains strongly positive,
- entry around 4–5 ms is near break-even,
- entry >= 6 ms is negative,
- changing exit from 3 to 10 ms at a fixed 1 ms entry changes almost nothing,
- changing exit from 4 to 10 ms at a fixed 2 ms entry changes almost nothing,
- changing exit from 6 to 12 ms at a fixed 3 ms entry changes almost nothing.

Therefore the current replay cliff is overwhelmingly an **entry-side effect**.

However:

> This is a replay-time effect relative to the locally recorded feed receive time and the observed-state proxy model. It is not yet a measured real chain-CREATE-to-BUY-landed requirement.

---

## 11. Exact meaning of current latency model

### 11.1 Live PAPER

Current live PAPER flow:

```text
real Pump.fun CREATE executes / reaches processed state
        |
        | provider ingestion / distribution          UNKNOWN absolute
        | internet path to host                     measurable network component
        v
WebSocket frame delivered to process
        |
        | receive_mono_ns stamped before parsing
        v
JSON/Borsh decode
        |
        | measured locally
        v
MarketReducer applies atomic envelope
        |
        | strategy/admission/fees/curve/quote
        v
decision_done_mono_ns
        |
        | intent creation / adapter submit
        v
submit_mono_ns
        |
        | entry_delay_ms (SIMULATED)
        v
PAPER scheduled fill
```

Live PAPER uses the real monotonic host clock, so real CPU time between receive, decode, decision, and enqueue is measurable.

### 11.2 Replay

Replay advances a virtual clock to recorded events.

Python CPU execution itself does not advance the replay clock as real wall-clock time.

Therefore an `ENTRY=3 ms` replay means approximately:

`recorded local receive time + modeled 3 ms fill delay`

It does **not** mean:

`real Solana CREATE execution + 3 ms`.

This distinction is mandatory in all future reports.

---

## 12. Network latency measurements — Camper / mobile connection

### 12.1 WebSocket RTT measured from the Mac in the camper

Persistent WebSocket ping:

#### Helius

- min: **100.944 ms**
- p50: **110.205 ms**
- p95: **130.869 ms**
- p99: **144.667 ms**
- max: **169.432 ms**
- mean: **115.023 ms**
- crude RTT/2 p50: **55.102 ms**

#### Public Solana RPC

- min: **94.405 ms**
- p50: **112.981 ms**
- p95: **136.496 ms**
- p99: **140.873 ms**
- max: **153.587 ms**
- mean: **114.802 ms**
- crude RTT/2 p50: **56.490 ms**

Startup TCP/TLS values were also measured but are **not per-trade costs**, because the WebSocket stays open.

### 12.2 ICMP from the camper Mac

Helius resolved to:

`198.13.128.10`

50-packet ICMP result:

- packet loss: **0%**
- min: **96.870 ms**
- average: **117.307 ms**
- max: **141.714 ms**
- stddev: **11.474 ms**

Public Solana RPC first 50-packet run:

- min: **91.083 ms**
- average: **107.710 ms**
- max: **160.610 ms**
- stddev: **13.362 ms**

Public Solana RPC second 50-packet run:

- min: **93.387 ms**
- average: **102.889 ms**
- max: **122.953 ms**
- stddev: **7.169 ms**

The public RPC resolved to `64.130.57.92` on the Mac during these tests.

---

## 13. Network latency measurements — DELTA Bodman fixed connection

Windows host physically connected at the Bodman office network.

No software installation was required.

### 13.1 Helius ICMP

Helius resolved to the same endpoint as the Mac:

`198.13.128.10`

50 packets:

- packet loss: **0%**
- minimum: **10 ms**
- average: **10 ms**
- maximum: **10 ms**

Traceroute reached the Helius endpoint in about **10 ms** via DE-CIX / TeraSwitch path.

This is an excellent fixed-network baseline for the exact same Helius IP.

### 13.2 Public Solana RPC ICMP

Windows resolved:

`64.130.50.23`

50 packets:

- packet loss: **0%**
- minimum: **10 ms**
- average: **10 ms**
- maximum: **12 ms**

This public-RPC IP is different from the Mac's public-RPC IP, so the Helius same-IP comparison is the cleaner fixed-vs-mobile comparison.

### 13.3 Mobile penalty estimate for the Helius endpoint

Exact same Helius IP:

- camper Mac ICMP average: **117.307 ms RTT**
- Bodman fixed network: **10 ms RTT**

RTT difference:

**~107.307 ms**

If the path is treated as approximately symmetric, the crude extra one-way delay attributable to the camper/mobile route is:

**~53.65 ms one way**

This is only a network-path approximation.

It does **not** measure Helius internal validator ingestion or provider processing.

---

## 14. Provider arrival race on the clean-v2 capture

### 14.1 All matched Solana signatures

Matched signatures:

**133,188**

Delta definition:

`Helius receive time - existing RPC receive time`

Negative means Helius arrived first.

Results:

- minimum: **-6102.638 ms**
- p01: **-5087.979 ms**
- p05: **-2313.815 ms**
- median: **-187.430 ms**
- p95: **-51.327 ms**
- p99: **-27.850 ms**
- maximum: **+120.047 ms**
- mean: **-432.905 ms**

Counts:

- Helius earlier: **133,039**
- existing RPC earlier: **149**
- ties: **0**

Helius therefore arrived first for about **99.89%** of matched signatures.

The large negative outliers mostly demonstrate delayed notifications on the public RPC; the mean should not be interpreted as a typical absolute Helius chain latency.

### 14.2 CREATE-only provider race

Matched CREATEs:

**512**

Results:

- minimum: **-5732.762 ms**
- p01: **-569.624 ms**
- p05: **-265.465 ms**
- median: **-161.434 ms**
- p95: **-39.908 ms**
- p99: **-17.851 ms**
- maximum: **+1.290 ms**
- mean: **-187.698 ms**

Counts:

- Helius earlier: **511 / 512**
- existing RPC earlier: **1 / 512**

For this dataset, Helius is clearly the primary fast feed. The public Solana RPC is useful as a secondary/reference source, not as the preferred low-latency CREATE source.

---

## 15. Local CREATE receive -> decode latency

Measured directly from the clean-v2 recording.

### Helius

512 CREATEs:

- min: **0.071625 ms**
- p50: **0.268959 ms**
- p95: **0.995500 ms**
- p99: **1.348625 ms**
- max: **2.064583 ms**
- mean: **0.398323 ms**

### Existing RPC

512 CREATEs:

- min: **0.057375 ms**
- p50: **0.212250 ms**
- p95: **0.870375 ms**
- p99: **1.089584 ms**
- max: **1.500708 ms**
- mean: **0.310886 ms**

Conclusion:

The local JSON/Borsh decode is currently sub-millisecond at p50 and around 1 ms at p95. It is not the dominant latency component.

---

## 16. Current end-to-end latency budget

The real future LIVE path must be decomposed as:

```text
T0  Pump.fun CREATE executes / is observed at processed commitment
 |
 | A1 validator/leader -> provider ingestion
 | A2 provider decode/distribution
 | A3 provider -> host network path
 v
T1  WebSocket frame reaches DarkBot host
 |
 | B JSON + Pump/Borsh decode
 v
T2  decode_done
 |
 | C1 MarketReducer
 | C2 Strategy C decision/admission
 | C3 fee/curve/quote calculation
 v
T3  decision_done
 |
 | D intent creation / enqueue / tx construction
 v
T4  transaction submission
 |
 | E1 outbound network
 | E2 RPC/TPU/Jito/provider submission processing
 | E3 leader inclusion/execution
 v
T5  BUY actually lands / executes on Solana
```

The full physical latency is:

`T0 -> T5 = A1 + A2 + A3 + B + C + D + E`

### Currently measured

- mobile host <-> Helius RTT
- fixed-network host <-> Helius RTT
- Helius-vs-public-RPC relative arrival
- Helius CREATE receive -> decode
- current replay sensitivity to modeled post-submit entry delay

### Currently not measured

- absolute T0 -> provider emission
- absolute provider emission -> T1 separate from provider processing
- real T2/T1 -> T3/T4 local path on an economic live-PAPER strategy run
- real transaction build/sign/send time
- real send -> RPC/TPU acknowledgement
- real send -> landed/processed BUY
- leader inclusion variability

---

## 17. Important interpretation of the 10 ms Bodman result

The Bodman 10 ms Helius RTT is useful and should be used as a **fixed-network baseline**.

For the exact same Helius IP:

- mobile RTT average: 117.307 ms
- Bodman RTT: 10 ms

The mobile route therefore adds roughly 107 ms of RTT relative to Bodman.

However:

> Do not simply subtract 53.65 ms from a replay's `entry_delay_ms`.

The replay delay begins at a different logical point.

The Bodman measurement helps estimate the networking part of the future T0->T1 / T4->T5 paths, but it does not identify Helius internal ingestion time or real transaction landing time.

---

## 18. What must be diagnosed next

### P0 — identify the real cause of ENTRY_FAILED

The current latency scan loses entries very quickly:

- 1 ms -> 19 failed
- 2 ms -> 21
- 3 ms -> 22
- 4 ms -> 28
- 5 ms -> 31
- 6 ms -> 34

The active diagnostic must determine the exact attempt statuses/reasons.

Before interpreting the 3–6 ms replay cliff as a physical speed requirement, determine whether failures are caused by:

- creator sold before modeled fill,
- slippage rejection,
- curve state unavailable,
- venue transition,
- stale/invalid quote,
- unknown evidence,
- or another execution-state rule.

This is the immediate next analysis item.

### P1 — measure live PAPER local reaction time

Run an economic live-PAPER strategy with real host clock and collect:

- `receive_to_decode_ns`
- `receive_to_decision_ns`
- `ready_to_decision_ns`
- `decision_to_enqueue_ns`
- callback lateness

The code already records these timestamps.

This gives the real:

`T1 -> T4`

local CPU/software latency.

### P2 — explain 204 Strategy-C entries vs 512 observed CREATEs

The current zero-filter launch-sniper still creates only about 204 entry intents/fills from 512 observed CREATEs, while capture-only actionable count was 447.

This admission denominator must be decomposed exactly before "all launches" statistics are considered final.

### P3 — profit concentration / outlier analysis

The strongest 3x/750ms result has only a few target exits.

Required:

- per-trade PnL ranking
- top-1 contribution
- top-3 contribution
- top-5 contribution
- PnL with top winner removed
- PnL with top 3 removed
- chronological thirds
- rolling-window stability
- creator-sell vs timeout vs target contribution

### P4 — fresh out-of-sample capture

Do not continue optimizing target/timeout/latency on the same 30-minute sample indefinitely.

After the implementation/evidence questions above are resolved:

- freeze the candidate policy,
- collect a new forward capture,
- test without tuning on that capture.

### P5 — real transaction submission measurement

Current PAPER cannot measure T4->T5 because it never broadcasts.

Before any LIVE edge claim, add or externally measure:

- transaction build start/end
- sign start/end
- send start/end
- RPC/TPU/Jito acknowledgement
- observed chain execution/processed timestamp
- return-feed arrival timestamp

The difference between "send accepted" and "BUY executed on chain" must remain explicit.

---

## 19. Recommended latency scenarios for further replay

Do not continue calling them "total latency".

Future replay labels should state the model origin, e.g.:

- `LOCAL_RECEIVE_PLUS_1MS`
- `LOCAL_RECEIVE_PLUS_2MS`
- `LOCAL_RECEIVE_PLUS_3MS`
- `LOCAL_RECEIVE_PLUS_4MS`
- `LOCAL_RECEIVE_PLUS_5MS`
- `LOCAL_RECEIVE_PLUS_6MS`

Once live-PAPER T1->T4 is measured and real T4->T5 broadcast latency is characterized, create calibrated scenarios based on those measured distributions instead of arbitrary fixed delays.

A future measured scenario should be built from distributions, not a single median:

- p50
- p95
- p99

for each relevant stage.

---

## 20. Current research conclusion

What is supported now:

1. The old backend's positive short-horizon, zero-age launch-sniper hypothesis was real as a modeled historical research result.
2. It was never proof of 0 ms physical chain-to-fill execution.
3. The same qualitative short-horizon edge can be reproduced on the new clean-v2 capture under old-like evidence semantics.
4. Under that replay model, entry timing is much more important than exit timing.
5. The current 3x / 750 ms in-sample result is strongly positive at local-receive + 0–3 ms modeled entry delay and turns negative around 4–6 ms.
6. The exact reason for that cliff is not yet fully diagnosed because ENTRY_FAILED rises sharply with modeled entry delay.
7. Helius is dramatically faster than the public Solana RPC in this dataset:
   - 511/512 CREATEs arrived first through Helius.
   - CREATE median lead about 161 ms.
8. The Mac's local CREATE decoding is very fast:
   - Helius p50 about 0.269 ms.
9. The camper mobile network is much slower than a good fixed line to the same Helius endpoint:
   - camper Helius ICMP average about 117.3 ms RTT,
   - Bodman fixed line about 10 ms RTT.
10. The Bodman result removes a large mobile-network penalty, but does not solve the unknown provider-ingestion or transaction-landing times.
11. A profitable live edge is **not yet proven**.

---

## 21. Next execution order

1. Finish the running `ENTRY_FAILED` status diagnostic.
2. Decompose why Strategy C admits only 204 of 512 CREATEs.
3. Run real live PAPER and extract T1->T4 latency distributions.
4. Quantify profit concentration and chronological stability.
5. Freeze one candidate strategy.
6. Capture a fresh out-of-sample dataset.
7. Instrument/measure a controlled real transaction submission path for T4->T5.
8. Only then map measured real distributions into replay and decide whether the edge survives realistic execution.

---

## 22. Frozen terminology

Use these terms consistently:

- **chain CREATE time**: real Solana/Pump event origin, currently not known to millisecond precision from logsSubscribe
- **local receive time**: `receive_mono_ns` when the WebSocket frame reaches DarkBot
- **local decode time**: `decode_done_mono_ns`
- **decision time**: `decision_done_mono_ns`
- **enqueue/submit time**: local PAPER adapter submission
- **modeled entry delay**: configured simulated delay from local submit to PAPER scheduled fill
- **real transaction landing latency**: future LIVE send -> on-chain execution, not currently measured
- **legacy-compatible evidence run**: RAM-only diagnostic that removes current global unattributable-undecodable hard gating while preserving per-mint/canonical gaps
- **strict current semantics**: current repository evidence rules without monkeypatch

Never use "0 ms" alone. Always state **0 ms from what timestamp to what timestamp**.
