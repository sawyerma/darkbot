# Builder bundle semantics — documentation review (v9.1 B1/provider, 2026-10-23)

Read-only GETs of official documentation, 2026-10-23 16:15–16:19Z; no builder endpoint contacted.
Sources: web3.48.club/docs/mev-builder{,/sendBundle,/bundle-status,/auction-transaction-feed};
docs.48.club (send-bundle, f.a.q., trouble-shooting, bundle status); github.com/BlockRazorinc/blockrazor_docs_en@0a8d506e
(block-builder/send-bundle.md, README.md, trace-bundle.md), blockrazor.io/support/.

| Item | 48Club Puissant | BlockRazor | G0.7 status |
|---|---|---|---|
| All-or-nothing / 2nd-TX revert | web3: "executed atomically"; non-whitelisted revert drops the bundle. FAQ: reorg can land it partially | only "suitable for … atomicity"; trace example "non-reverting tx in bundle failed" | UNQUALIFIED (no measured case) |
| Contiguous [A,B] | "preferred transaction order" only | merging allowed by default; G0.7 sends no noMerge | UNQUALIFIED |
| noMerge / positionFirst | positionFirst = strict order by priorityFee; default allows late tail append | same flags exist; G0.7 does not send them | not top-of-block; UNQUALIFIED |
| Expiry | maxBlockNumber (GitBook default +100) and maxTimestamp; window max not documented | maxBlockNumber default +100, inclusiveness not documented | G0.7 sends head+2 (+ maxTimestamp for 48Club) |
| Pricing | auctionPrice = Σ unique gas × 0.9 + BNB to 0x4848…4848; first-TX vs average gas check contradictory | ≥ 0.05 gwei, per-tx vs average contradictory; ranks BNB to builder EOA **0xc7d4187c…dB5C** (changed 2026-10-16 from 0x1266…5F20) | 0.05 gwei, tip 0: accepted/competitive UNQUALIFIED |
| ACK | bundle hash; "acceptance … not determined at the time of your request"; status API 5-block delay | bundle hash; inclusion only via paid Trace Bundle | ACK ≠ inclusion (G0.8 decides) |
| Coverage | own/partner validators only ("blocks we do not have the rights to propose") | self-claim "ranks first"; no list | UNQUALIFIED (R6) |

Consequences: the 2026-10-16 builder refresh evidence is stale (BlockRazor EOA, 48Club maxTimestamp note); the
builder policy tool tracks only floor/ranking wording, not atomicity/expiry/flags. Closing any row needs a
provider-specific measured case with canonical receipts — an economic test, not authorized here.
