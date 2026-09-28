-- Additive: record the monotonic end of a run so replay never fires model timers beyond the recorded observation window
-- (timeout_and_observation_contract.end_of_run: unobserved later executions are never invented).
ALTER TABLE runs ADD COLUMN ended_mono_ns INTEGER;
