# Echte PumpPortal-Capture-Fixtures

Dieses Verzeichnis ist leer, bis eine reale Coin-Sequenz aufgezeichnet wurde (Gate `PAID_FIXTURE_CAPTURE`).
Was hier landen muss und wie es geprüft wird, definiert ausschließlich `phase0_fixture_contract` im aktiven Vertrag.

Dateien:

- `create.json` – das rohe Create-Event **eines** Coins
- `trades.json` – Array der rohen Trade-Events **desselben** Coins in Empfangsreihenfolge (Mindestanzahl und Buy+Sell laut Vertrag)
- `migration.json` – ein rohes Migration-Event (gleicher Coin, oder `migration_mint_matches_capture: false` im Manifest); Status laut `migration_detection_v1`, solange unverifiziert `MIGRATION_CANDIDATE_UNVERIFIED`
- `manifest.json` – Provenienz mit genau den `manifest_required_fields`: `provider`, `schema_version`, `captured_at_utc`, `source` (URL ohne Key), `capture_mint`, `field_map`, `tx_type_values`, `files`, `redactions`, `warmup_measurement`, `offsets_time_basis` (`monotonic_ns`), `feed_cost`, `trade_subscription_stop_reason`, `capture_stop_reason`, `migration_status`, `migration_mint_matches_capture`, `capture_validity`
- `trades.offsets.json` – Index → Offset in ms **ausschließlich aus `time.monotonic_ns()`** relativ zum Create-Empfang; nie aus UTC-Differenzen
- `raw_hashes.json` – `raw_sha256` je Rohobjekt

`warmup_measurement` enthält `subscribe_sent_utc_ns`, `subscribe_sent_offset_ns`, `first_trade_received_utc_ns`, `first_trade_received_offset_ns`, `subscribe_ack_offset_ns` (null), `ack_message_raw` (null) und `ack_candidates_raw` (unbekannte Frames nach `subscribe_sent`, Rohformat). Kein Frame gilt als Subscription-Ack, solange `subscription_ack_proven=false`.

`feed_cost` beweist das Kostenlimit: `paid_message_limit` aus `capture_spend_cap_sol` und `feed_pricing_observed.lamports_per_paid_message`, `paid_messages_at_unsubscribe <= paid_message_limit`, `paid_message_count`, `estimated_cost_lamports`.

`capture_validity` muss `valid: true` mit allen `capture_validity_checks` melden; ein Capture mit Exit-Code ≠ 0 besteht das Gate nie.

Alle Hashes (`files`, `schema_version`, `raw_sha256` je Objekt) kommen aus einem Werkzeug, nie aus Handrechnung:

```bash
cd backend && PYTHONPATH=. python -m darkbot.cli fixture-hashes
```

Rohobjekte bleiben unverändert (nur dokumentierte Secret-Redaktion). Nichts erfinden, nichts umformatieren, keine Zugangsdaten hier ablegen.
`synthetic_selftest: true` ist nur für Validator-Tests in temporären Verzeichnissen erlaubt und besteht das Gate nie.
