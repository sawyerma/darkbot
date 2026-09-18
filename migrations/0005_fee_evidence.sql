-- Preserve CostEvidence across recording and replay; older rows remain UNKNOWN.
ALTER TABLE fee_snapshots ADD COLUMN evidence TEXT NOT NULL DEFAULT 'UNKNOWN';
