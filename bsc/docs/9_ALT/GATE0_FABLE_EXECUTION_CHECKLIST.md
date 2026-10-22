# Gate-0 Fable execution checklist

> **ABSOLUTE TOP PRIORITY / STOP RULE — FABLE TARGET:** Do not perform any unrelated Gate-0 work until a **real successful on-chain BUY with actual token acquisition** has a **measured/proven `T0 → T_buy < 1 ms`**. Only work directly required to execute, instrument, prove, or reduce that real BUY path below 1 ms is permitted. `T_signal → T_send`, transport handoff, builder ACK, receipt or ordering alone are not the target and do not unlock further work.

Status: ACTIVE
Date: 2026-10-02

This checklist maps the external Fable comparison findings to the Gate-0 latency objective. It does not replace the authoritative BSC roadmap.

## Immediate controlled Mainnet latency path

Required before the first controlled Launch -> BUY measurement:

- Current Flap Mainnet Portal/version/implementation pinned: PASS.
- Exact native-BNB Curve-20 quote and minOutput cross-check: PASS.
- BUY prepared before T0 whenever the controlled launch makes all fields knowable in advance: REQUIRED.
- Ordered bundle [Launch, BUY]: PASS in the dry harness.
- 48Club noMerge=true: REQUIRED.
- 48Club positionFirst=true: REQUIRED.
- 48Club maxBlock tight window: REQUIRED.
- BlockRazor ordered bundle in parallel with byte-identical transactions: REQUIRED.
- Parallel fan-out must not create a second economic attempt: REQUIRED.
- Record T_ready, T_queue and T_send independently per builder.
- Record actual token acquisition, canonical block and transaction indices.
- PASS ordering evidence: Launch=N, BUY=N+1, successful external BUYs between=0.
- Do not claim a numeric sub-ms T0->T_buy without a trusted builder/validator execution trace.

## Network-path work from Fable

BlockRazor Fast Submit:
- Current provider documentation says Fast Submit uses a dedicated HTTP domain and removes the general proxy layer.
- The dedicated domain is supplied after subscription; there is no public universal Fast Submit hostname to hard-code.
- Gate-0 must support a runtime-supplied Fast Submit endpoint and compare it against the standard path using Builder/Bundle Trace.
- Do not purchase/enable it solely on provider benchmark claims; measure first when access exists.

48Club:
- Puissant eth_sendBundle supports noMerge, positionFirst and backrunTarget.
- positionFirst is mandatory for the N+1 qualification path.
- noMerge is mandatory for the controlled ordered-bundle qualification.
- Auction Feed is a separate private-flow ingress class, not a prerequisite for the controlled self-launch latency experiment.

## Private-flow work from Fable

48Club Auction Feed:
- newPendingAuctions requires 48 Soul Points.
- newPendingAuctionsWithState requires 480 Soul Points.
- Feed supplies target hash plus filtered logs/state changes, not the target raw signed transaction.
- When access exists, the adapter must map the actionable target to backrunTarget and derive the BUY without a synchronous hot-path RPC fetch.

BlockRazor Private Mempool:
- Requires provider access/authentication.
- Delivers authorized structured fields from private BlockRazor RPC orderflow.
- Adapter remains access-gated until credentials/service access exist.

These access-gated products must not delay the first controlled Mainnet latency experiment.

## Pre-sign/helper rule

Controlled self-launch:
- Launch salt, predicted token and exact BUY are known before T0.
- Therefore the target measurement must pre-build and pre-sign the BUY before T0.
- Build/sign work performed after T_signal in a diagnostic benchmark is conservative and is not the final optimized self-launch path.

Foreign launch:
- Pre-signing is valid only when all signed BUY fields are actually knowable before T0.
- A helper/executor contract may be evaluated only if it can remove a proven information dependency without changing semantics or introducing a new external round trip.
- Do not claim that a helper can pre-sign an unknown token/salt unless the execution contract can deterministically resolve that value on-chain.

## Safety/economics

- Positive minOutput remains mandatory.
- Current native-BNB fee/tax semantics remain bound to live Portal evidence.
- First-block/tax/anti-sniping modes must fail closed when not supported by the exact venue decoder.
- No public broadcast of the controlled launch before ordered builder submission.
- Budget guard remains fail-closed.

## After first controlled latency result

Only after T_signal->T_send and controlled execution/ordering evidence exist:
1. compare standard BlockRazor vs Fast Submit if a dedicated domain is available;
2. use Bundle Trace / builder receive-time evidence to separate client->builder network delay from local work;
3. integrate Auction Feed/backrunTarget when 48SP access exists;
4. integrate BlockRazor Private Mempool when access exists;
5. evaluate multi-region placement from measured builder-receive times;
6. collect real public-vs-private launch statistics before buying additional infrastructure.
