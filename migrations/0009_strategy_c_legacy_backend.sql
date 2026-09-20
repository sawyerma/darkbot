-- Strategy C (LEGACY_BACKEND_FILTER_V1): frozen feature snapshot and typed filter verdict per observation.
-- Additive only; rows of other strategies keep NULL.

ALTER TABLE strategy_observations ADD COLUMN features_json TEXT;
ALTER TABLE strategy_observations ADD COLUMN filter_evaluations_json TEXT;
ALTER TABLE strategy_observations ADD COLUMN first_failed_filter TEXT;
ALTER TABLE strategy_observations ADD COLUMN decision_action TEXT;
