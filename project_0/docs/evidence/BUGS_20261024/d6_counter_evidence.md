# BUGS D6 — market counter evidence only on change (2026-10-24, LOCAL_PASS, offline)

`market/core.rs::on_ingress_event` emitted a `MarketCounters` evidence (two boxed `MarketStats`
copies, one channel slot, one worker aggregation with a `Vec`/`BTreeMap`) for every ingress event,
also when no counter changed (e.g. transport health frames). Now it is emitted only when a
counter changed, transactions were admitted (`observed_slots`), or the event is the first of a
live hour — the latter keeps the running `max_stream_silence_ms` carried into an hour that has
unchanged events only, so every hourly figure stays what full emission gave. Slot/coverage/
heartbeat evidence is unchanged. `Handle::hour` is now the one hour definition of core and worker.

Test `d6_counter_evidence_only_on_change_or_a_new_hour`: real `ObserverCore` with an econ handle,
8 ingress events (hour 1 unchanged-only) → 4 counter evidences instead of 8, and the real worker's
four hourly capture counters are identical to full per-event emission (hour 1 keeps 300 ms).
Mutations fail-before: always emit (8), no new-hour carry (3, parity broken).
Gate: test_all 817/0/10, test_default 815/0/10, runner 375/0/3 ×2, clippy ×4, release, Python;
fmt re-run PASS after formatting the test file.
