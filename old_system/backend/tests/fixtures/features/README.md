# Feature-Fixtures

`feature_cases.json` wird erst nach den echten Provider-Fixtures erzeugt und ist per `source_manifest_sha256` an deren Manifest gebunden.
Struktur und Prüfregeln stehen in `phase0_fixture_contract` (`feature_cases_document_fields`, `curve_inputs_fields`, `feature_case_fields`, `precision_rule`) und `feature_contract_v1` des aktiven Vertrags.

```json
{
  "feature_set_version": <feature_contract_v1.version>,
  "source_manifest_sha256": "<sha256 von pumpportal/manifest.json>",
  "curve_inputs": {
    "token_decimals": <int 0..18 | null>, "token_decimals_source": "<Provenienz, z. B. RPC-Read des Mint-Accounts mit Slot>",
    "create_virtual_sol_reserves": <int | null>, "create_virtual_token_reserves": <int | null>,
    "initial_virtual_token_reserves": <int | null>, "initial_real_token_reserves": <int | null>, "token_total_supply": <int | null>,
    "initial_buy_token_base_units": <int | null>, "research_quote_amount_lamports": <int | null>,
    "protocol_config_source": "<Provenienz oder 'nicht gelesen'>"
  },
  "cases": [
    {
      "case_id": "…",
      "deadline_ms": <eine Snapshot-Zeit aus strategy.*_snapshot_ms>,
      "coverage_state_at_deadline": "UNKNOWN_WARMUP | OBSERVED | GAP",
      "data_quality_interval_intersects": false,
      "normalized_events": [
        {"raw_sha256": "<sha256 des kanonischen Rohobjekts>", "receive_offset_ms": "12.5", "event_type": "BUY",
         "actor": "…", "quote_lamports": 123, "token_base_units": null, "virtual_sol_reserves": 30000000000, "virtual_token_reserves": null}
      ],
      "expected": {"<jedes der 17 Features>": <int | Dezimalstring | true/false | null>},
      "null_reasons": {"<feature>": "warum null"},
      "derivation": "wie die Erwartungswerte unabhängig bestimmt wurden"
    }
  ]
}
```

Regeln:

- Jedes normalisierte Event verweist auf ein reales Rohobjekt (`raw_sha256`); Offsets kommen aus `trades.offsets.json` (monoton).
- Alle 17 Features werden vom Orakel in `cli.py` nachgerechnet: E-Features aus den Events, R/C-Features aus `curve_inputs`, `provider_gap_flag` aus den Coverage-Feldern.
- Keine Annahmen: ist `token_decimals` null, müssen `token_base_units`/`virtual_token_reserves` aller Events null sein und die reserveabhängigen Features null mit `null_reasons`.
- Geldwerte nie als Gleitkommazahl.
