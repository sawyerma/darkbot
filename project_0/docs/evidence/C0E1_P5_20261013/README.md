# P5 local qualification — 2026-10-13

Implementation continues P3 aa37dd3 / P4 2c62d13. Only project_0 changed.

- Python: 44 PASS. Runner lib crate_health,txv1: 191 PASS, 0 FAIL, 1 existing ignored.
- Four release binaries built (9 existing runner warnings); changed Rust region format and shell syntax PASS. No whole-repository formatting.
- First runner attempt: 190 PASS, 1 FAIL, 1 ignored. The timer fixture's fixed 4.15s sleep raced the fourth automatic poll. Test now waits for the real hour_04 file with a 10s bound; production logic/gates unchanged. Original failure retained.
- Normalized unfiltered immutable records; actual chain entities and hypothetical Shadow kept separate. Same dataset feeds all renderers and display-only typed filters (including four individual related operations).
- Existing canonical Rust selector/economics reused for saved strict-event/Shadow joins; four exact MeasurementPlan boundaries and final output. If finalized canonical inputs or original Shadow are unavailable, economics replay stays NOT_RUN/INCOMPLETE, not a synthetic zero.
- Tiny P4 integration addition only: return already committed strict batch checkpoints even when a later batch fails, so P5 can report partial evidence. No second verifier or P4 redesign.
- Capture launcher persists MeasurementConfigV2, drains then invokes the existing verifier and report. Verification off uses no historical RPC. Provider qualification still required for index/strict. A completed capture can retain a nonzero technical qualification result; it is not Master PASS.
- No actual 5h replay, capture, provider measurement, Mainnet smoke, LIVE or submission performed. P6 full local matrix follows separately.

Exact commands and actual output are adjacent. These tests do not qualify provider quotas, historical completeness, stable four-hour operation or profitability.
