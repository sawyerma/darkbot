# Global Overview – implementation boundary (Part 6)

This slice adds gateway-owned overview.aggregates proto, typed read-only
Zustand projection, card navigation, and a Graphite view.
The Gateway and its engine feeds do NOT exist yet. No financial data is invented.

The adapter propagates schema/contract errors so StateFull seq is not advanced.
The page hides stale/detached aggregates including finance. LIVE transport never
grants trading authorization. Old JSON RealtimeClient remains for F7 deletion,
but this UI uses only the new gateway connection via RouteTopics.

F2/F3 NOT_VERIFIED: protobuf generated TS, @bufbuild/protobuf lock/install,
Rust/TS golden-byte tests, source liveness, browser typecheck/build and
Playwright are outstanding. Do not claim any PASS without running them.
