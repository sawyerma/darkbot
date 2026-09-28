-- Additive provenance columns (fill_evidence_contract / attempt_cost_fields): concrete fill quote, per-kind fee snapshot ids, decoder version.
ALTER TABLE execution_attempts ADD COLUMN fill_quote_snapshot_id TEXT REFERENCES quote_snapshots(quote_snapshot_id) ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE execution_attempts ADD COLUMN network_fee_snapshot_id TEXT REFERENCES fee_snapshots(fee_snapshot_id) ON DELETE RESTRICT;
ALTER TABLE execution_attempts ADD COLUMN rent_fee_snapshot_id TEXT REFERENCES fee_snapshots(fee_snapshot_id) ON DELETE RESTRICT;
ALTER TABLE execution_attempts ADD COLUMN priority_fee_snapshot_id TEXT REFERENCES fee_snapshots(fee_snapshot_id) ON DELETE RESTRICT;
ALTER TABLE execution_attempts ADD COLUMN decoder_version TEXT;
