# G0.7 preparation — 2026-10-20

**Decision: STOP / not ready for economic authorization. No submission. Guard UNARMED.**

| Item | Verified result |
| --- | --- |
| R0 | PASS in native requalification verdict; 858891 headers through preflight block 128724651, linked and archive-anchored; upgrade at 128103397, receipt-root proof |
| DRY-0 through DRY-4 | PASS; SHA256SUMS verified; exact signed bytes unchanged; zero egress |
| Portal | v5.25.0; implementation `0xe13227370996f892a56d2ff4a30f6026887b48fb` |
| Launch transaction | `0xdb468c01529ee4c9597eae056f420f0d4d6d0753a1f785f8dbb3163028be5146` |
| BUY transaction | `0xb086f683550d1aa221db2add1aa1426444b7ef716de98594c40b9b6da765db32` |
| V4 proposal SHA-256 | `5da0068b0c8b1f7e3c682cc4ea7ce3950d83147e94946b46aafb11f6dd50ec3d` |
| Proposal state | approved=false; execution_ready=false; maximum attempts=1 |
| Approved-variant SHA-256 | `4308f2cb97414aea7d4ce9469ab101eb3f29ff1d272df0a72930e49a56c29ad0`; computed only, not written or authorized |
| Current readiness | BLOCKED, exit 2; native record at 2026-10-20 12:11:03 UTC |
| Readiness checks passed | V4 guard/input bindings; DRY1–4; budget/gas/slippage; signed hashes; absent attempt latch; baseline and current deployment/nonces/funding/gas; builder freshness; clock/head/config; no signed egress |
| Builder freshness | Official documentation fetched 2026-10-20; oldest source Unix 1792497980; 24-hour window enforced; strictest explicit floor remains 50000000 wei |
| BUY input | 1000000000000 wei = 0.000001 BNB |
| Worst-case gas reserve | 480163500000000 wei = 0.0004801635 BNB; limits 6603270 + 3000000 gas at 50000000 wei/gas |
| Maximum total loss budget | 481163500000000 wei = 0.0004811635 BNB; launch value 0, builder tip 0; no automatic increase or retry |
| G0.8 preparation | Existing g08_mainnet_verify and g08_producer_timing binaries built. Acquisition/order/cost reconciliation consumes G0.7 evidence plus the chain journal; timing additionally requires original producer witness. No execution result exists yet. |

## Remaining blockers

- `BUILDER_SEMANTICS`: existing probe establishes submission checks only. Inclusion and the building-time effects of noMerge/positionFirst remain unqualified. Pre-arm checks 6/10 are not upgraded to PASS.
- `PRODUCER_WITNESS`: original producer execution witness is absent; T0-to-BUY is unproven. This is distinct from token acquisition or block ordering.
- No economic approval, approved contract write, arming commit or real BUY has occurred. No authorization is requested while the native readiness reports BLOCKED.

## Evidence

- Requalification: `bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/`; native verdict `requalification_status.dbrec`, SHA-256 `a9854d47b3665427341444bb4f1ac25fde5d550a5cd124cd6f824bb98f541795`.
- Proposal: `bsc/config/g07_attempt1_acceptance_v4.dbrec`.
- Current readiness: `bsc/native/evidence/g07/readiness_20261020T121103Z/readiness.dbrec` with derived view and SHA256SUMS. The earlier 12:09:18 record preserves the diagnosed schema failure.
- Runtime dependency repair: installed the existing locked EVM/ABI packages. Resumed at the failed EVM step; R0, preflight and quote hashes were unchanged. No second historical scan.
- Code repair: shared builder-policy validator now binds the embedded schema name to the decoded native/imported record variant. One targeted offline regression passed for both variants, rejection of a mismatched schema and unchanged gas-floor/policy guards. No signing, transaction construction or economic limits changed.

The native qualification artifacts and proposal remain local for review. The running Project-0 capture was not stopped. No GUI deployment was performed for this qualification repair.
