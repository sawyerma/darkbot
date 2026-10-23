# C1/R1 — launch-path coverage of the deployment drift check (2026-10-23)

Read-only: `bsc/tools/g06a_launch_path_trace.cjs` (local EthereumJS over RPCStateManager, public
`bsc-dataseed`, block 129352181 `0xb48c4cd4…b9d3`), throwaway public vanity salt and dummy callers;
no key, no signature, no send. Raw per-code lines: `launch_path_trace.txt`.

**R2/T2:** `newTokenV6` returned exactly the CREATE2-predicted token (`0x9983…7777`); launch and first BUY succeeded.

**C1/R1 — PROVEN GAP.** The readiness/runner drift check pins the Portal proxy, its implementation
(`0x7a94…`) and the Tax-V3 implementation code. Covered transitively (address fixed in immutable code):
`impl → 0x4f95… → 0xfee6… → 0x407f… / 0xbd99… / 0xd674…`. **Not covered** — executed on the path but
replaceable without changing any pinned code:

| Path | Contract | Why not covered |
|---|---|---|
| launch | `0xffb69d59…264f` → `0x88056bfe…84dd` | EIP-1967 proxy (admin-upgradable); proxy address fixed in `0x4f95…`, implementation in storage |
| BUY | `0x8a08d98c…ab0e` → `0x3e5c6364…d36e` | 171-byte proxy, implementation in storage slot 0 (`masterCopy()` `a619486e`) |
| launch/BUY | clones → `0xb133e600…7a9a`, `0xa39a573d…ee98` | implementation addresses not in any checked immutable code (storage-selected) |

**Smallest correction (not yet implemented):** bind this launch-path code identity (address, code SHA-256,
proxy implementation) as a record in the v5.26 requalification and its successor contract, and re-verify it
at the pinned preflight block in readiness and the runner (mismatch = STOP). Until then the existing drift
check is not a complete launch-path identity.
