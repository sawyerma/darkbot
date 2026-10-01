-- Capture-analysis fields for canonical market events.
-- Additive only; historical rows remain NULL where the original recorder did not persist the field.

ALTER TABLE market_events ADD COLUMN sol_amount INTEGER;
ALTER TABLE market_events ADD COLUMN token_amount INTEGER;
ALTER TABLE market_events ADD COLUMN token_total_supply INTEGER;
ALTER TABLE market_events ADD COLUMN creator_fee_bps INTEGER;
