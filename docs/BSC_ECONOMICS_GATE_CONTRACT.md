# DarkBot — BSC Production Economics Gate Contract

Status: **BLOCKED UNTIL GATE-0 PASS + PHASE-F ACCEPTANCE**
Updated: 2026-09-27

Normative parents:
- `docs/GATE0_TESTPLAN_SHORT.md`
- `docs/BSC_DUAL_VENUE_ROADMAP.md`
- `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`

## Gate-0 relationship

Gate 0 MUST already contain the **minimal exact economics needed to construct a valid first BUY** for the tested venue/mode: required value/input, mandatory fees/tax behavior that affects validity, gas requirements and slippage bounds.

This Economics Gate does not postpone first-BUY feasibility. It expands that minimal proof into the complete production economic model after Phase F.

Executable production economics live once in Rust and are reused by Replay/PAPER/LIVE.

Python may validate/analyze outputs but is not a BSC decision/quote engine.

No synchronous `eth_call`, Helper, HTTP/RPC or external quote request is allowed in the production decision hot path. External calls are evidence/differential-test tools only.

Local quote/economics latency is an engineering metric. It is not proof of an actual BUY.

---

## 1. Purpose

This gate converts factual Flap/Four chain evidence into verified economic models.

Only after this gate passes may BSC Strategy C replay claim to evaluate expected value after costs.

This gate does not optimize a strategy.

It proves:

- quote/output math;
- protocol fees;
- mode/tax effects;
- BSC network gas costs;
- revert/failure costs;
- replacement/nonce-attempt costs;
- approval/permit/sell-path costs where applicable;
- migration/post-graduation path economics where required.

Unknown economics fail closed.

---

## 2. Preconditions

Do not start until:

1. Phase F clean real BSC capture is accepted;
2. observed active Flap/Four contract versions/modes are enumerated;
3. real fixture transactions exist for every mode/version claimed supported;
4. Phase-C evidence/timing/provenance is trustworthy;
5. full suite is green.

The real Phase-F dataset determines which versions/modes deserve support.

Do not implement speculative economics for unobserved historical modes merely because documentation mentions them.

---

## 3. Architecture ownership

Keep responsibilities narrow.

### BSC chain layer owns

- native gas units;
- receipt gas used;
- effective gas price;
- transaction fee accounting;
- failed/reverted tx gas cost;
- replacement/attempt network-cost accounting;
- base-unit/native-decimal semantics.

### Flap venue layer owns

- deterministic quote math/state interpretation;
- Flap protocol fee;
- Flap tax/mode effects;
- Flap approval/permit/sell requirements;
- Flap migration/post-graduation venue path.

### Four.meme venue layer owns

- deterministic quote/helper semantics;
- Four protocol fee;
- Four tax/mode effects;
- TaxToken/X/AntiSniper economics;
- Four sell/approval path;
- graduation/post-curve venue path.

Do not create one giant `bsc_economics.py` with venue conditionals.

---

## 4. Evidence freeze for every economic rule

Every economic constant/formula must have provenance.

Freeze, per supported protocol version/mode:

- contract address/version;
- ABI hash;
- relevant source-code/version identity if available;
- state variables used by quote;
- rounding rule;
- fee basis and recipient if relevant to accounting;
- tax basis/order;
- mode-specific rule;
- helper/quote contract identity;
- sample block/state;
- historical tx/receipt used for parity.

No magic percentage or gas number.

Unknown rule => UNKNOWN/UNSUPPORTED, never zero.

---

## 5. Exact arithmetic

All calculations use integer base units or explicit exact decimal configuration where unavoidable.

Never binary float for economic state.

Required:

- exact rounding direction at every division;
- explicit fee ordering;
- explicit tax ordering;
- exact input/output units;
- overflow-safe arithmetic;
- boundary tests.

A 1-base-unit mismatch must be explainable. Do not hide mismatch with tolerances unless the underlying external interface itself is non-deterministic, in which case the cause must be documented.

---

## 6. Quote-engine validation hierarchy

For each venue/version/mode:

1. derive local deterministic quote from frozen protocol state;
2. validate against authoritative on-chain view/helper call at the same state where available;
3. validate against real captured/historical executed transactions;
4. validate exact rounding/fee/tax decomposition.

For `eth_call`/Helper parity:

- pin chain id;
- pin contract/version;
- pin block/state;
- pin exact input;
- record raw return;
- compare exact integer result.

If exact historical pre-transaction state cannot be reconstructed, do not fake transaction-state parity. Mark that evidence class unavailable and rely only on valid reproducible evidence.

---

## 7. Flap economics

Implement only versions/modes observed and verified.

Required where relevant:

- buy quote;
- sell quote;
- curve state transition assumptions;
- protocol fee;
- creator/other fee if actually present;
- buy tax;
- sell tax;
- vault/mode effects;
- slippage bound semantics;
- permit path cost/requirements;
- approve + sell path cost/requirements;
- failed approval/sell cost;
- migration/graduation path;
- post-migration venue economics only if Strategy research will include that path.

Unknown tax/mode => not economically qualified.

---

## 8. Four.meme economics

Implement only verified active versions/modes.

Required where relevant:

- buy quote;
- sell quote;
- Helper/on-chain parity;
- protocol fee;
- TaxToken effects;
- X Mode effects;
- AntiSniperFeeMode effects;
- mode-dependent fee/tax ordering;
- approval/sell path;
- graduation/liquidity path;
- post-graduation economics only if later research includes it.

A mode label from Phase E does not imply its economics. The formula/rate must be separately proven here.

---

## 9. BSC gas/network cost model

Distinguish:

### Realized historical cost
From receipt/evidence:

`gas_used * effective_gas_price`

plus any other actual chain-level charge if BSC evidence proves one.

### Counterfactual/replay cost
Must use a frozen evidence policy appropriate to the replay timestamp/state.

Do not use a constant "typical gas" magic number.

Model at least:

- successful entry;
- successful exit;
- reverted entry;
- reverted exit;
- approval/permit transaction if required;
- replacement attempts;
- failed/abandoned attempts that consumed gas;
- multiple same-nonce attempts without double-counting non-included candidates that consumed no chain gas.

Receipt evidence wins for realized transactions.

---

## 10. Attempt/replacement accounting

Economic accounting must separate:

- intent;
- submitted attempt;
- replacement candidate;
- included attempt;
- reverted included attempt;
- dropped/non-included attempt.

Do not charge on-chain gas to a transaction that never landed unless there is actual provider/service cost evidence.

Do charge gas for included reverts.

Do not double-count one nonce lifecycle.

Any builder/relay tips are future execution evidence unless actually part of the tested path.

---

## 11. Slippage

Slippage is not a guessed percentage.

Replay/economics must distinguish:

- quote at decision state;
- execution state;
- configured max slippage bound;
- actual output;
- price impact;
- tax/fee components.

When only a bound is known, do not pretend it was the realized cost.

Pessimistic research may intentionally apply a documented adverse bound, but it must be labeled as an assumption/sensitivity scenario, not measured fact.

---

## 12. Failed and unsupported cases

Fail closed if:

- quote state missing/stale beyond documented policy;
- unsupported contract version;
- unsupported mode;
- unknown tax;
- quote parity fails;
- required approval/permit path unknown;
- gas evidence unavailable for a claim requiring exact realized cost;
- migration path affects exit but is unsupported.

No economic qualification with assumed zero cost.

---

## 13. Required test matrix

For each supported venue/version/mode:

### Quote boundaries
- zero/invalid input rejection;
- minimum meaningful input;
- representative small/medium/large inputs;
- near-boundary state;
- exact rounding edge cases.

### Fees/taxes
- fee-free only if protocol evidence proves it;
- each fee component;
- each supported tax/mode;
- combined components/order.

### Buy/sell
- exact local quote parity;
- exact helper/eth_call parity where available;
- historical executed vector;
- revert vector where available.

### Network cost
- success receipt;
- revert receipt;
- replacement lifecycle;
- approval/permit path where applicable.

### Unknown
- unknown version;
- unknown mode;
- missing economic evidence;
- parity mismatch.

All unknowns must block qualification.

---

## 14. Captured-vector validation

Use Phase-F real captured vectors for both venues.

For every economic class claimed supported, maintain a fixture set with:

- chain tx/block/state identity;
- decoded venue version/mode;
- exact input/state;
- expected output;
- fee/tax decomposition;
- receipt gas cost when applicable;
- provenance.

Synthetic vectors supplement boundaries but never establish Mainnet truth.

---

## 15. Economics snapshot/versioning

Freeze a versioned economics snapshot containing:

- chain;
- venue;
- contract version;
- ABI hash;
- quote-model version/hash;
- fee/tax-model version/hash;
- supported modes;
- gas evidence policy;
- freshness policy;
- rounding policy;
- source/evidence manifest hash.

Later Strategy C replay binds to this snapshot.

Changing economics creates a new snapshot/hash; do not silently rewrite old research semantics.

---

## 16. Performance and hot path

The eventual decision hot path must use local deterministic state/math.

Do not make synchronous `eth_call`, Helper HTTP/API or metadata requests part of the future trading decision hot path.

External calls are:

- validation;
- evidence refresh outside hot path;
- controlled fallback only if explicitly designed later and latency-accounted.

Measure local quote latency p50/p95/p99/max.

This gate still does not prove final `T_PENDING_SEEN -> T_SOCKET_HANDOFF <= 3 ms`.

---

## 17. Simplicity/code-quality gate

Keep economics as small venue-specific deterministic modules plus BSC network-cost logic.

Reject:

- one generic formula abstraction that hides venue semantics;
- duplicated BSC gas model inside each venue;
- HTTP-based quote engine when local deterministic math can be proven;
- magic gas/fee/tax constants;
- hardcoded provider endpoints/secrets;
- legacy fallbacks;
- virtual environment/vendor folders;
- strategy tuning mixed into economics.

---

## 18. No strategy optimization in this gate

Do not:

- search targets;
- search timeouts;
- optimize filters;
- rank strategies;
- claim positive EV;
- tune to Phase-F winners.

The only question is:

**Can every supported trade path be priced/costed correctly and reproducibly?**

---

## 19. Acceptance gate before M10

Economics is accepted only when:

- active observed versions/modes have explicit support or explicit UNSUPPORTED status;
- quote math is validated;
- fee/tax math is validated;
- exact rounding is frozen;
- BSC success/revert/replacement gas accounting is proven;
- entry and exit paths have complete cost semantics;
- unknowns fail closed;
- economics snapshot hashes are frozen;
- full suite is green;
- no Solana semantic regression exists;
- no strategy tuning occurred.

Only then may:

**M10 Strategy C Multichain** start.

The first serious BSC expected-value test still occurs later in BSC replay after Strategy C multichain integration.
