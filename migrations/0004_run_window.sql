-- Additive: monotonic run start so utilization and UTC day mapping use the recorded run window, not the first/last observed frame.
ALTER TABLE runs ADD COLUMN started_mono_ns INTEGER;
