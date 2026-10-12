# P3 marker qualification — 2026-10-13

Base: c78ae68. Command from project_0/scripts:

    python3 -m unittest test_audit_market_quick -v

29 tests PASS. Includes six captured Mainnet receivership transactions (offline),
all four classes, six discriminator markers, start/end pairing, absent/reversed/
duplicate/mismatched pairs, failed TX, CPI/LUT/legacy/v0/v1, truncated logs and
per-instruction duplicate-event rejection. No new RPC or capture executed.

Pinned protocol source: marginfi 33c67987a661f59c6683a65485ebb4f0ef030b77.
`end_deleverage` was already recognized. This fixes the missing start marker and
qualification of its pair; no assertion that every previous forced event was lost.
Decoder identity changed: new verification uses index_v2. Existing index_v1 raw
responses/checkpoints remain untouched; they are not silently relabeled with the
new decoder. Historic five-hour index qualification remains OPEN / RPC_RATE_LIMIT.
