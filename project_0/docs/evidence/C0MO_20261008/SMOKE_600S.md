# C0-MO r5 / MT0 r2 — 600 s Mainnet Smoke

Date: 2026-10-08

## Transactions source

Helius enhanced transactionSubscribe was unavailable for the configured plan.

Result:

- HTTP 403 Forbidden
- no StreamReady
- verdict: MARKET_OBSERVER_STREAM_NOT_READY

This is a provider capability limitation, not a local runtime/build failure.

## Logs source

Configuration:

- source=logs
- commitment=finalized
- duration=600 s
- read_only=true
- signing=false
- submission=false

Result:

- actual_observation_seconds=600.000000
- transactions_received=95
- unique_transactions=95
- successful_transactions=73
- failed_transactions=22
- liquidation_transactions=6
- liquidation_events=6
- gap_count=0
- reconnect_count=0
- ingress_overflow_count=0
- decoder_error_count=0
- enrichment_requested=6
- enrichment_succeeded=6
- enrichment_failed=0

Observed economics are descriptive only:

- gross_edge_total_usd=30.979129
- gross_edge_per_liquidation_usd=5.163188
- gross_edge_per_hour_usd=185.874774
- sample_status=INSUFFICIENT_SAMPLE

Final verdict:

- verdict=MARKET_DATA_INCOMPLETE
- decode_complete=false
- market_capture_complete=false
- incomplete_reasons=["LOGS_TRUNCATED"]

## Truncated-log qualification

11 / 95 observed transactions contained `Log truncated`.

All 11 were fetched again with getTransaction:

- capture_truncated=11
- rpc_found=11
- rpc_still_truncated=11
- rpc_not_truncated=0
- rpc_failures=0

The same 11 transactions were checked against the qualified F8
end_liquidation instruction semantics:

- END_LIQUIDATION discriminator:
  [110, 11, 244, 54, 229, 181, 22, 184]
- target account index=0
- receiver account index=3

Result:

- truncated_transactions=11
- exact_matches=11
- mismatches=0
- all 11 had end_liquidation instruction count=0
- all 11 had decoded liquidation event count=0

Therefore no missing liquidation was demonstrated in these 11 transactions.

C0-MO r5 is conservatively incomplete because every `Log truncated`
currently invalidates decode completeness. C0-MO r6 will add fail-closed
instruction-based qualification of truncated transactions.

## Qualification state

Local qualification before smoke:

- candidate rustfmt differential: PASS
- build crate_health: PASS
- build crate_health+txv1: PASS
- workspace tests crate_health: PASS
- workspace tests crate_health+txv1: PASS
- differential Clippy crate_health: candidate-only=0
- differential Clippy crate_health+txv1: candidate-only=0

LIVE remains blocked.
No profitability claim is made from this smoke.
