# DarkBot Phase-1 PAPER Bericht – Run edge_probe_01_paper_20260919T220756Z_804f03

Kampagne: edge_probe_01 | Modus: PAPER | Config-Hash: d477997581a78bbb | Query: REPORT_QUERY_V1

**Qualifikationsstatus: UNQUALIFIED_COSTS** – fee rules not fully verified or attempt costs unknown

## Umfang
- Beobachtete CREATEs (lokale Union): 341, davon actionable: 262
- Entry-Intents: 2, Fills: 2, Entry-Fehler: 0
- Exits: Creator-Sell 1, Target 0, Timeout 0; Exit-Fehlversuche 0, Retry-Erschoepfungen 0
- Offen am Ende: 1, UNRESOLVED: 0 {}, zensiert: 1
- Ineligible nach Grund: {'VENUE_CURVE_COMPLETE': 7, 'SLOT_LIMIT': 192, 'UNSUPPORTED_QUOTE': 32, 'UNQUALIFIED_COSTS': 68}

## Wirtschaft (Modell, OBSERVED_STATE_PROXY)
- Realisiertes Netto-PnL: -4357966 Lamports (-0.004357966 SOL)
- Unrealized: UNKNOWN_EQUITY_INTERVAL; ungeklaerte Exposure (Entry-Kosten): 99604825 Lamports; konservative Untergrenze (Token-Restwert 0): -103962791
- Erwartungswert je Intent: -4357966.0, je Fill: -4357966.0, Median-PnL: -4357966, Gewinnrate: 0.0
- Profit Factor: 0.0 ; Max Drawdown: 4357966; Kapitalauslastung: 0.9920479537955872
- Gebuehren nach Art: {'protocol_fee_lamports': 2785379, 'creator_fee_lamports': 879594, 'network_fee_total_lamports': 15600, 'sender_tip_lamports': 0, 'rent_locked_lamports': 3027680, 'rent_refunded_lamports': 1513840}; Kosten-Evidenz: {'MEASURED': 3}; Fill-Evidenz: {'OBSERVED_STATE_PROXY': 3}
- Cash-Reconciliation: {'initial': 1000000000, 'ledger_cash_sum': -105476631, 'end_cash_per_ledger': 894523369, 'run_end_balance': 894523369, 'reconciled': True}
- Block-Bootstrap (Intent-PnL): {'blocks': 1, 'mean': -4357966.0, 'ci_low': None, 'ci_high': None, 'status': 'INSUFFICIENT_BLOCKS', 'seed': 20260916, 'resamples': 2000}

## Latenz (lokal gemessen, gleiche Clock-Epoch)
- receive→decode: {'n': 293737, 'min': 4292, 'p50': 11959, 'p95': 141750, 'p99': 421792, 'max': 6689750, 'p99_low_confidence': False}
- receive→decision (Entries): {'n': 2, 'min': 1699541, 'p50': 1699541, 'p95': 1973417, 'p99': 1973417, 'max': 1973417, 'p99_low_confidence': True}
- decision→enqueue: {'n': 3, 'min': 70500, 'p50': 97375, 'p95': 215375, 'p99': 215375, 'max': 215375, 'p99_low_confidence': True}
- Callback-Lateness: {'n': 3, 'min': 337625, 'p50': 570125, 'p95': 689958, 'p99': 689958, 'max': 689958, 'p99_low_confidence': True}
- Modellverzoegerung Entry/Exit (SIMULATED): 0 / 0 ns

## Feeds (relativ, Union-Nenner, keine globale Vollstaendigkeit)
- helius_standard: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=341, rel p50/p95=None/None ns
- existing_rpc: n=293737, matched=341, hint-wins=341, actionable-wins=262, ties=0, missing=0, rel p50/p95=0/0 ns
- pumpportal: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=341, rel p50/p95=None/None ns

## Datenqualitaet
- Quality-Events: 19712, kanonische Gaps: 1033, Provider-Disconnects: 0, Recorder-Overflows: 0, interne Fehler: 0
- Positionen mit kanonischem Gap: []; Migrations-/Curve-complete-Exposures: []
- Quote-Scope der CREATEs: {'SOL_WSOL': 307, 'UNSUPPORTED_QUOTE': 34}
- Fee-Snapshots: {'VALID': 81, 'RPC_ERROR': 14, 'PRIORITY_UNKNOWN': 2}

## Annahmen
- Historischer Fremd-Orderflow reagiert nicht auf unseren hypothetischen Trade (Gegenfaktualitaetsannahme). Simulierte Inklusion ist eine Annahme, keine gemessene Landung. Fill-Evidenz ist OBSERVED_STATE_PROXY. Keine Zahl behauptet global frueheste Erkennung, sicheren Fill oder garantierten Profit.
- Latenzmodell: ASSUMED_OPTIMISTIC_BASELINE; Slippage-Grenze 300 bps ist eine Ausfuehrungsgrenze, keine gezahlte Gebuehr.
