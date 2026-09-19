ALTER TABLE runs ADD COLUMN strategy_id TEXT;
ALTER TABLE runs ADD COLUMN strategy_config_hash TEXT;
CREATE TABLE IF NOT EXISTS strategy_observations (
  observation_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id),
  mint TEXT NOT NULL, strategy_id TEXT NOT NULL, strategy_config_hash TEXT NOT NULL,
  create_signature TEXT NOT NULL, observation_start INTEGER NOT NULL,
  observation_end INTEGER NOT NULL, decision_actual_mono_ns INTEGER,
  buy_count INTEGER NOT NULL, unique_buyers INTEGER NOT NULL, qualified INTEGER,
  status TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS fx_snapshots (
  fx_snapshot_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id),
  evidence_id TEXT NOT NULL, source TEXT NOT NULL, observed_utc TEXT NOT NULL,
  received_mono_ns INTEGER NOT NULL, amount TEXT, raw_json TEXT NOT NULL,
  raw_hash TEXT NOT NULL, validation_status TEXT NOT NULL, timestamp_basis TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS entry_sizing (
  decision_id TEXT PRIMARY KEY REFERENCES decisions(decision_id) DEFERRABLE INITIALLY DEFERRED,
  run_id TEXT NOT NULL REFERENCES runs(run_id), mint TEXT NOT NULL, trade_size_usd TEXT NOT NULL,
  fx_snapshot_id TEXT NOT NULL REFERENCES fx_snapshots(fx_snapshot_id) DEFERRABLE INITIALLY DEFERRED,
  swap_budget_lamports INTEGER NOT NULL, curve_input_lamports INTEGER NOT NULL, max_swap_debit_lamports INTEGER NOT NULL, sizing_basis TEXT NOT NULL,
  decision_mono_ns INTEGER NOT NULL
);
