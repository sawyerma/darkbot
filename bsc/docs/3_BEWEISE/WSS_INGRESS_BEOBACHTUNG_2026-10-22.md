# 2026-10-22 – BSC ingress WSS observations (plan v5, cited by v6)

**Provenance:** User-supplied Mac terminal logs and 30-minute run summary; this is a new bounded observation appendix, not a rewrite of the immutable historical master ledger. Original run artifacts are not included in this appendix; treat totals as reported measurements pending manifest/file hash binding. All timings use local reception/remote path estimates, **not original producer T0**.

| Test | Observations | Qualification |
|---|---|---|
| 10 s endpoint probe | PublicNode: 2,506 pending hash notifications; dRPC: 1,616; BlockReq: 95; 48Club accepted WSS but `newPendingAuctions` rejected due required arg missing | Pending availability only; not actionable or T0 |
| 60 s same-hash race | PublicNode 10,349 unique; BlockReq 398 unique; common 354: PublicNode first 309, BlockReq first 41, 4 tied; median BlockReq-PublicNode arrival 271.8 ms | Local reception race on shared hashes, not launch timing |
| 180 s venue race | 13 canonical launches (Flap 9, Four Classic 4); PublicNode before local block reception 5 Flap, 0 Four; median lead 231.8 ms; BlockReq 0 | Local block reference only, **not T0** |
| 30 min `lauf1` | 116 launches: Flap 82 (43/82 pre-local-head any source = 52.4%; PublicNode 39/82), Four Classic 34 (5/34 =14.7%; PublicNode 4/34); 48/116 any source early (41.4%); 68/116 no source early | Ingress census, uneven source uptime |
| 30 min provider stats | PublicNode 397,901 msgs, connected 100%, early 43/116, p50 lead 157.6 ms and p90 404.5 ms; dRPC connected 52.6%, early 27/55 in observed scope; BlockReq early 1/116 | Provider-scope denominators **not interchangeable** |
| Full-TX format probe | PublicNode `newPendingTransactions(true)` returned object with `hash`, `to`, `input` five/five sampled | Full JSON object presence, **not yet cryptographic/signed actionability** |
| 180 s T0 boundary probe | 12 evaluated launches, 3 Pending full sightings; 2 before local head (264.99 ms and 122.72 ms), 1 after (+6.18 ms); original producing execution witness 0 | `T0=UNKNOWN` for every sample |

**Do not infer:** 232 ms or 157.6 ms before T0; builder receipt before accepting block; source-speed superiority over pinned geth; full private flow coverage; same-block first BUY; `T_buy - T0 <1 ms`; availability/cost of the 48Club private Auction Feed. Approximate Frankfurt/US path estimates (30/74 ms) are separate round-trip-derived observations; subtraction from local block-reception lead is **not** proof of builder admission before T0.

**Next validation:** simultaneously measure actionable/full (not just hash) arrival on pinned geth and each public feed, source health/limits, native Rust actionability and ingress-to-real handoff, while G0.7/G0.8 remains governed by separate explicit approval. To prove physical `T_buy - T0`, obtain original producing builder/validator execution witness; independently verify true canonical N/N+1 and positive token acquisition.
