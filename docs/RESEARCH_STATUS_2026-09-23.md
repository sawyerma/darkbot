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


---

## 23. Live-PAPER local latency measurement completed

A 10-minute live-PAPER run was executed with the real host monotonic clock and Strategy C launch-sniper config.

Run:

`latency_chain_20260923_paper_20260923T203355Z_ae42f8`

The run reported `qualification_status = UNQUALIFIED_DATA`. This does **not** invalidate the latency timing samples. In the current reporter, Strategy C with `creator_sold_before_fill = LEGACY_FREE_CANCEL` is explicitly diagnostic-only for economic qualification and returns `UNQUALIFIED_DATA` by design. The timestamps themselves remain real local wall/monotonic-clock measurements.

### 23.1 T1 -> T2: Helius local receive -> decode

443 Helius CREATE samples:

- min: **0.086375 ms**
- p50: **0.190792 ms**
- p95: **0.638959 ms**
- p99: **0.966000 ms**
- max: **1.120750 ms**
- mean: **0.257456 ms**

This second live-PAPER measurement is consistent with the clean-v2 capture measurement and confirms that local decoding is not the dominant latency component.

### 23.2 T1 -> T3: local receive -> decision

205 real ENTRY decisions:

- min: **0.386083 ms**
- p50: **0.524209 ms**
- p95: **1.360916 ms**
- p99: **2.131750 ms**
- max: **3.625750 ms**
- mean: **0.711430 ms**

### 23.3 data-ready -> decision

205 ENTRY decisions:

- min: **0.252209 ms**
- p50: **0.338667 ms**
- p95: **0.862834 ms**
- p99: **1.395958 ms**
- max: **2.621000 ms**
- mean: **0.471185 ms**

### 23.4 T3 -> T4: decision -> enqueue

205 ENTRY decisions:

- min: **0.005750 ms**
- p50: **0.015708 ms**
- p95: **0.038833 ms**
- p99: **0.067750 ms**
- max: **0.085500 ms**
- mean: **0.020226 ms**

### 23.5 T1 -> T4: local receive -> enqueue total

205 ENTRY decisions:

- min: **0.392500 ms**
- p50: **0.539334 ms**
- p95: **1.394291 ms**
- p99: **2.202542 ms**
- max: **3.668666 ms**
- mean: **0.731656 ms**

### 23.6 Consequence for the old fixed-delay replay interpretation

The local software path is now measured and is very small. If an old replay scenario is labeled `LOCAL_RECEIVE_PLUS_3MS`, the residual time after the measured local path would be only approximately:

- p50 residual: **3.000 - 0.539 = 2.461 ms**
- p95 residual: **3.000 - 1.394 = 1.606 ms**
- p99 residual: **3.000 - 2.203 = 0.797 ms**

This is **not** evidence that a real transaction can land in the residual time. It demonstrates why real T4 -> T5 measurement is mandatory before interpreting the 3–6 ms replay cliff as physically tradable.

---

## 24. Persistent HTTP RPC timing from camper Mac

Using a persistent HTTPS connection and repeated `getLatestBlockhash(commitment=processed)` calls against Helius:

### Helius HTTP

100 samples:

- min: **105.583709 ms**
- p50: **125.299625 ms**
- p95: **146.862084 ms**
- p99: **158.000166 ms**
- max: **179.972458 ms**
- mean: **127.509662 ms**

Interpretation:

- this is request -> response latency on the current camper/mobile route,
- it is **not** transaction send -> landed latency,
- it contains outbound + Helius/RPC handling + inbound network time,
- it must not be inserted directly as `entry_delay_ms`.

### Existing public RPC HTTP

The same rapid 100-request test against the existing public RPC aborted with:

`HTTP 429: Connection rate limits exceeded`

Therefore no valid 100-sample HTTP latency distribution exists for the existing public RPC from this test. It remains unsuitable as the primary low-latency submission/reference path under this rate behavior.

---

## 25. Bodman Helius HTTP attempt

A PowerShell persistent-`HttpClient` test was attempted on the Windows host physically connected to the DELTA Bodman fixed network.

The request returned:

`HTTP Unauthorized: Unauthorized`

As a result:

- **no Bodman Helius HTTP latency distribution was measured**,
- the earlier Bodman **10 ms ICMP RTT** remains valid,
- the failed HTTP attempt must not be converted into a latency value.

Before retrying this test, the Helius API-key/endpoint authorization on that Windows session must be corrected/verified. Do not print or commit the API key.

---

## 26. Transaction-latency test environment prepared

The Mac has no Solana CLI installed:

`NO_SOLANA_CLI`

A separate temporary Python virtual environment was therefore created for the transaction-latency experiment:

`/tmp/darkbot-solana-latency`

Installed successfully:

- `solders==0.29.0`
- `jsonalias==0.1.1`
- `typing-extensions==4.16.0`

A new temporary latency-test keypair was generated locally:

- public key: `BFaakhXHciEFJMZgBbrRa2KEXswH1ZaRKsqjk16XmTPM`
- private key file: `/tmp/darkbot_latency_keypair.json`

The private key file is temporary/local and **must never be committed, pasted into documentation, SQLite, logs, or GitHub**.

### 26.1 Existing funded DarkBot test wallet already exists

A new funded wallet is not inherently required.

An earlier DarkBot test/bot wallet was already created and funded on Solana Mainnet:

- public key: `3rKiMw7mDTbxrnwsFhGjGgJ81o5YeysBrkvkXdRUQ9d5`
- confirmed funding at the time: **0.005 SOL = 5,000,000 lamports**
- the locally held private key was previously verified to derive exactly this public key

Therefore the preferred next action is:

1. **reuse the existing funded test wallet if its private key is still available locally and controlled by the user**;
2. do **not** fund the newly generated `BFaakh...` wallet merely because it was generated;
3. use the new throwaway wallet only if the old test-wallet key is no longer available or a deliberately isolated test wallet is desired.

No private key or seed phrase is to be placed in this repository.

---

## 27. Updated measured latency chain

The current best-known chain is now:

```text
T0  real Pump.fun CREATE executes / becomes observable on Solana
 |
 | A1 validator/leader -> Helius ingestion            UNKNOWN absolute
 | A2 Helius processing/distribution                  UNKNOWN absolute
 | A3 Helius -> host network
 |    camper Helius ICMP avg RTT: 117.307 ms
 |    Bodman Helius ICMP RTT:      10 ms
 v
T1  Helius frame reaches DarkBot
 |
 | receive -> decode
 | live PAPER p50/p95/p99: 0.191 / 0.639 / 0.966 ms
 v
T2  decoded/data ready
 |
 | reducer + strategy + admission + quote
 v
T3  decision done
 |    T1->T3 p50/p95/p99: 0.524 / 1.361 / 2.132 ms
 |
 | decision -> enqueue
 |    p50/p95/p99: 0.0157 / 0.0388 / 0.0678 ms
 v
T4  local enqueue/submission point
 |    T1->T4 total p50/p95/p99: 0.539 / 1.394 / 2.203 ms
 |
 | E transaction build/sign/send -> provider/leader -> execution
 |    REAL T4->T5 STILL UNKNOWN
 v
T5  BUY actually executes/lands on Solana
```

The major unresolved engineering measurement is now **T4 -> T5**.

---

## 28. Replay calibration limitation discovered

The current measured-latency loader in `darkbot/ab_replay.py` accepts only arrays of non-negative **integer milliseconds**:

- `entry_delay_ms`
- `exit_delay_ms`

This is too coarse for the now-measured sub-millisecond / low-single-digit-millisecond path because values such as **0.539 ms**, **1.394 ms**, and **2.203 ms** would be quantized materially.

Before measured real distributions are used for final latency calibration, the replay measured-distribution format should be upgraded to preserve at least **microsecond precision** (preferably integer nanoseconds to match the engine clock).

This change should be made only after the exact semantic origin of the measured sample is frozen, so that no stage is double-counted.

---

## 29. Exact work still required from this frozen point

### Immediate P0 — finish ENTRY_FAILED diagnosis

Still open. Determine exact attempt statuses behind the sharp modeled entry-delay failure growth. Do not interpret the 3–6 ms replay cliff as pure physical latency until this is resolved.

### Immediate P1 — T4 -> T5 real transaction measurement

Use the existing funded DarkBot test wallet if its local private key is still available.

For each controlled Mainnet test transaction, timestamp separately:

1. transaction build start/end,
2. signing start/end,
3. `sendTransaction` start,
4. RPC response / signature returned,
5. first `getSignatureStatuses` state = processed,
6. first Helius return-feed observation of the same signature.

Required output distributions:

- build time,
- sign time,
- send -> RPC ACK,
- send -> processed,
- send -> Helius return observation,
- processed -> Helius return observation.

These must be recorded as raw per-sample values, not only averages.

### P2 — repeat HTTP RPC timing from Bodman with valid Helius authorization

Goal: compare the same persistent `getLatestBlockhash` call between camper/mobile and Bodman fixed network.

The existing Windows attempt produced Unauthorized and therefore has no usable latency result.

### P3 — decompose 204 Strategy-C entries vs 512 CREATEs

Still open and required before final all-launch economic interpretation.

### P4 — profit concentration and chronological stability

Still required:

- top-1/top-3/top-5 contribution,
- PnL excluding biggest winners,
- chronological thirds,
- rolling-window stability,
- exit-reason contribution.

### P5 — upgrade measured-latency format precision

After T4->T5 semantics are measured/frozen:

- replace integer-ms-only measured samples with microsecond or nanosecond precision,
- keep stage origin explicit,
- avoid adding provider/network stages twice,
- use measured distributions rather than a single median.

### P6 — build calibrated realistic replay distribution

For a replay whose market observations already begin at recorded local Helius receive timestamps, the initial calibrated entry execution delay should represent the stages **after that recorded receive point**, primarily:

`T1 -> T4 measured local software path + T4 -> T5 measured real execution path`

Do not automatically add the Bodman one-way network estimate to this value; the capture's receive timestamp already defines the replay market-data origin.

### P7 — fresh low-latency forward capture

The clean-v2 capture was recorded over the camper/mobile path. Because the candidate edge is highly time-sensitive, a final forward test should be captured on a stable low-latency fixed/server path after the execution semantics are frozen.

Then run the frozen strategy without target/timeout tuning on that new dataset.

---

## 30. Updated stop/go rule

Do **not** claim a live-tradable edge until all of the following are true:

- ENTRY_FAILED mechanism is understood,
- real T1->T4 distribution is measured — **DONE**,
- real T4->T5 distribution is measured — **OPEN**,
- measured distribution is represented without millisecond quantization loss — **OPEN**,
- 204/512 admission denominator is explained — **OPEN**,
- profit concentration/stability is quantified — **OPEN**,
- a frozen candidate survives a fresh forward low-latency dataset — **OPEN**.

The current work has narrowed the unknowns substantially, but real transaction landing latency remains the critical missing measurement.


---

## 31. First Devnet T4 -> T5 execution-path measurement

A controlled 10-transaction Solana Devnet test was executed from the camper Mac using:

- network: Solana Devnet
- HTTP RPC: `api.devnet.solana.com`
- WebSocket: `api.devnet.solana.com`
- local signer: temporary test keypair `BFaakhXHciEFJMZgBbrRa2KEXswH1ZaRKsqjk16XmTPM`
- wallet balance before test: **1.0 Devnet SOL**
- `skipPreflight = true`
- `maxRetries = 0`
- 10 signed transfers

Raw result file created locally:

`exports/devnet_t4_t5_latency_20260923T211907Z.json`

### 31.1 Important validity limitation

All 10 transactions reached validator processing but returned:

`InsufficientFundsForRent: account_index = 1`

Reason: the randomly generated receiver account did not yet exist and each test attempted to create/fund it with only 1–10 lamports, below the Devnet rent-exempt minimum for a new system account.

Therefore this run is **not a successful-transfer execution sample**. It remains useful as a measurement of the public-Devnet send/receive/validator-processing path for transactions that were received and executed far enough to produce an on-chain instruction error.

Do not treat these values as final successful BUY/transfer T4->T5 calibration.

### 31.2 Local transaction preparation

10 samples:

| Stage | min ms | p50 ms | p95 ms | p99 ms | max ms | mean ms |
|---|---:|---:|---:|---:|---:|---:|
| Blockhash RPC | 107.068 | 123.082 | 147.030 | 147.030 | 159.518 | 128.050 |
| TX build | 0.061 | 0.099 | 0.118 | 0.118 | 0.203 | 0.106 |
| TX sign | 0.178 | 0.336 | 0.375 | 0.375 | 0.396 | 0.310 |
| TX serialize | 0.011 | 0.019 | 0.023 | 0.023 | 0.097 | 0.026 |

The build/sign/serialize CPU work is sub-millisecond and is not the dominant component.

### 31.3 Public Devnet network/validator path

10 samples:

| Stage | min ms | p50 ms | p95 ms | p99 ms | max ms | mean ms |
|---|---:|---:|---:|---:|---:|---:|
| send -> RPC ACK | 115.711 | 122.421 | 137.230 | 137.230 | 380.872 | 150.318 |
| send -> receivedSignature | 290.162 | 358.204 | 391.529 | 391.529 | 439.075 | 355.275 |
| send -> processed (WS) | 343.866 | 414.624 | 445.364 | 445.364 | 468.624 | 404.640 |
| send -> processed (HTTP polling) | 458.038 | 539.884 | 585.946 | 585.946 | 632.333 | 541.180 |

Derived `receivedSignature -> processed (WS)` across these 10 samples:

- min: about **0.061 ms**
- p50: about **53.703 ms**
- p95: about **104.930 ms**
- max: about **106.061 ms**
- mean: about **49.365 ms**

### 31.4 Interpretation

This first Devnet run establishes that the measurement instrumentation works end-to-end:

- transaction build/sign/serialization are measured locally,
- RPC send acknowledgement is separated from validator receipt,
- `receivedSignature` is separated from `processed`,
- HTTP polling is visibly slower than the WebSocket processed notification and must not be used as the primary timing source.

However, because every transaction failed the rent condition, a second run with an already-created/rent-funded receiver is required before this path can be treated as a successful transaction baseline.

Also, public Solana Devnet and the camper/mobile route are not equivalent to the intended future Mainnet low-latency submission path. These measurements validate method and order of magnitude only; they are not yet the final Strategy-C execution distribution.

---

## 32. Immediate next Devnet correction

Before repeating the 10 timing samples:

1. create one receiver account on Devnet with at least the RPC-reported rent-exempt minimum for zero data,
2. verify that setup transaction succeeds,
3. then send the 10 tiny timed transfers to that already-existing receiver,
4. require `err = null` for every sample used in the successful T4->T5 distribution,
5. retain the failed-rent run separately as a transport/processing diagnostic rather than silently replacing it.

After the successful Devnet run, compare:

- failed-vs-successful send -> ACK,
- failed-vs-successful send -> receivedSignature,
- failed-vs-successful send -> processed,
- local build/sign/serialize distributions.

Only after that should the next submission-path experiment move to a faster provider/low-latency host and eventually Mainnet-equivalent conditions.
