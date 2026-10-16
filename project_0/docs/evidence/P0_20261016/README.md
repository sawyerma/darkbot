# P0 — Gate verdict (2026-10-16)

| Gate P0 | Result | Evidence |
|---|---|---|
| Reuse matrix complete | PASS | `REUSE_MATRIX.md`, `docs/architecture/p0_reuse_matrix.md` |
| License per reuse component | PASS | `LICENSE_MATRIX.md` |
| Routing/API/free-tier/cost audit | PASS (provider items marked UNKNOWN below stay re-check items before enabling) | this file |
| Health strategy | PASS — marginfi crate, `HealthPriceMode::Client` | `ORACLE_MODEL.md` |
| Oracle model | PASS | `ORACLE_MODEL.md` |
| Staging path | PASS — `stag8s…`, same cluster, needs `staging` build + allowlist (P5/P8) | `STAGING_MODEL.md` |
| No wheel reinvented | PASS | `REUSE_MATRIX.md` |

No staging run, no Mainnet request, no provider API call was made for P0.

## Routing / API cost audit (P0-05, P0.2A)

Sources: provider documentation and the `0dotxyz/solana-dex-superagg` repository, read 2026-10-16.

| Item | Finding | Decision |
|---|---|---|
| `solana-dex-superagg` | MIT; not on crates.io; HEAD 2.0.0 untagged (last tag v1.2.1); Jupiter and DFlow paths return a finished serialized TX, Titan path builds and signs a full TX; no instruction output, no program allowlist check; Solana 4.x graph (`solana-client =4.2.2`) | REMOTE_AGGREGATOR_CLIENT, not linked (cannot be placed inside the receivership TX) |
| Jupiter (used) | DarkBot calls `GET /swap/v2/build` (raw instructions, ExactIn only, `maxAccounts`), program checked against `JUP6…` (`execution/route.rs:173`). Free plan 1 RPS, keyless 0.5 RPS; paid from $25/month (10 RPS); no SLA | FREE_ONLY: route call per opportunity, not a hot-path stream; paid plan only by separate decision |
| Titan | Gateway REST and DART free at 1 RPS; streaming (Direct WS) enterprise/distributor only; router `T1TAN…` | allowlisted; no hot-path use on FREE_ONLY |
| DFlow | own aggregator program, not in the receivership allowlist; prod key on request; limits/prices UNKNOWN | not used |
| Jito block engine | default sends without auth, 1 req/s per IP per region; min tip 1000 lamports; bundle ≤ 5 TX | S3 |
| Pyth Hermes | API key required since 2026-08-26 (trial, then paid); on-chain sponsored push feeds stay free | DarkBot does not call Hermes (`ORACLE_MODEL.md`) |
| Yellowstone gRPC | client/proto Apache-2.0, server plugin AGPL-3.0; free public endpoint UNKNOWN | S1-RACE only |
| Helius | Free 1M credits, 10 req/s; enhanced WS from $49/month | FREE_ONLY provider qualification stays FE-05 |

UNKNOWN (re-check before enabling the respective path): Jupiter overage rate and v1 shutdown date; Titan V2 program ID and
provider routes outside `T1TAN`; DFlow limits/prices; Hermes keyed prices; Yellowstone free endpoint; Jito UUID limits.
