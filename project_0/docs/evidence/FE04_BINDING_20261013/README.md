# FE-04 signed adapter binding — local qualification, 2026-10-13

The durable journal now accepts only an owned, immutable signed payload that passed the existing adapter's full simulation. Production callers cannot use the former hash-plus-arbitrary-callback API. Wallet payer, required signatures and cryptographic signatures are checked. The domain-separated SHA256 binds transport, member count, order, lengths and all signed bytes; single RPC/Jito transactions and bundles cannot be substituted. The same adapter reference receives exactly the simulated transactions. A bundle requires bundle simulation even with one member.

Before the single adapter call, HandoffUnknown is fsynced. Success ACK, timeout, rejection and mismatched ACK all retain the wallet holds. ACK signature membership/order, path, wire length, handoff time and bundle ID are validated. Restart and a second trigger remain blocked. No automatic resend or release on transport error.

Tests: 9 journal/transport regressions PASS, including 12 path/outcome combinations (RPC/Jito single/Jito bundle × ACK/transport/rejection/incorrect signature). Wrong payer/signature, tampered message, duplicate TX, changed order/path, failed/unavailable simulation and wrong adapter are rejected. Workspace: txv1 350 PASS / 0 FAIL / 1 existing ignored; health 348 PASS / 0 FAIL / 1 existing ignored. Python 59 PASS. Both release builds PASS. Changed-region Rust formatting and diff whitespace PASS. Differential Clippy: 38 baseline/current warning keys, zero additions.

## Remaining gates

FE-04 is PARTIAL_LOCAL, not PILOT_READY. This is an adapter binding API, not a connected terminal pilot: production-owned current Intent/Current assembly, authorization lifecycle and actual finalized wallet/exit reconciliation remain open. The normal LIVE entrypoint stays blocked; no authorizing profile was created, no real transaction submitted, no Mainnet call or user-owned 5h replay performed. FE-01/02 current parity, profitable floor/exit and full production simulation remain unproven. Mainnet check awaits explicit permission.
