# P4 local qualification — 2026-10-13

Base: aa37dd36b9a432e0b3489b6841eaba49e423e673.
Capture finalization is local-only; no full slot scan, gap backfill or enrichment
RPC is implicitly run. Pending gaps/log qualification remain incomplete evidence.
Capture clock/live coverage/verification axes are separate; master gates unchanged.

The new p0-strict-verify driver reuses reconcile_slots and its existing canonical
classifier. UTC bracket discovery is bounded and fail-closed on missing archive /
blocktime. 32-slot batches overlap the previous produced block/hash; all parent,
skip and full-transaction proofs are reused. Immutable committed batches + hashed
cached RPC responses support restart, with preserved partial evidence. More than
100k total slots is segmented, not a raised per-batch protocol assumption.

MeasurementManifest/2 persists capture_run_id before capture (v1 is still read).
measurement_contract.py defines MeasurementConfigV2 and separate typed RunConfig,
VerificationConfig, ReportQuery. Legacy identity imports do not change old files.
Capture evidence is hashed by streaming after completion; immutable verification
spec/attempt/result IDs never relabel live coverage. verify.sh supports off/index/
strict, explicit resume and local preflight; default index requires measured
FREE_ONLY provider quota evidence. Optional crosscheck is accepted but remains
INCOMPLETE until independent source qualification; it is never canonical.
P5 auto-post-capture launch/report wiring is the next package, not claimed here.

Budgets: <=5 configured requests/s, effective history <=2/s, serial strict reads
through existing P2 admission/circuit. Per-attempt call/time/response-byte limits,
RSS headroom, disk/free-space checks. Successful response bytes are measured;
failed-request wire bytes are UNKNOWN, not zero. No actual quota/archive claim.

Executed from repository root:

    cargo test --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health,txv1 --lib --locked --offline
    cargo build --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health,txv1 --release --bin p0-longtail --bin p0-measurement-plan --bin p0-strict-verify --locked --offline
    python3 -m unittest discover -s project_0/scripts -p test_measurement_contract.py -v
    python3 -m unittest discover -s project_0/scripts -p test_audit_market_quick.py -v
    python3 -m unittest discover -s project_0/scripts -p test_measurement_plan.py -v

Results: Rust 190 PASS / 0 FAIL / 1 pre-existing ignored; release PASS;
Python 5 contract + 29 index + 4 duration/launcher PASS.
Prior test failure retained: old test expected finalizer enrichment; corrected to
assert NOT_RUN/no enrichment request, original fee-UNKNOWN checks retained.
New strict tests cover resumed batches, skip/parent continuity, corrupted evidence,
changed scope and disk budget; capture TIMER/off and abort both finalize with no RPC.

OPEN/NOT_RUN: actual provider/WSS/archive qualification, Mainnet smoke, full matrix
without txv1, P5/P6 complete invariants. No RPC, new capture, LIVE or submission run.
