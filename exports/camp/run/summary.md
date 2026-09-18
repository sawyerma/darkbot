# DarkBot Phase-1 PAPER Bericht – Run run

Kampagne: camp | Modus: PAPER | Config-Hash: b6ab71156dd1c10d | Query: REPORT_QUERY_V1

**Qualifikationsstatus: UNQUALIFIED_DATA** – unresolved exposure exceeds configured maximum

## Umfang
- Beobachtete CREATEs (lokale Union): 1, davon actionable: 1
- Entry-Intents: 1, Fills: 1, Entry-Fehler: 0
- Exits: Creator-Sell 0, Target 0, Timeout 0; Exit-Fehlversuche 0, Retry-Erschoepfungen 0
- Offen am Ende: 1, UNRESOLVED: 0 {}, zensiert: 1
- Ineligible nach Grund: {}

## Wirtschaft (Modell, OBSERVED_STATE_PROXY)
- Realisiertes Netto-PnL: 0 Lamports (0 SOL)
- Unrealized: UNKNOWN_EQUITY_INTERVAL; ungeklaerte Exposure (Entry-Kosten): 99655810 Lamports; konservative Untergrenze (Token-Restwert 0): -99655810
- Erwartungswert je Intent: None, je Fill: None, Median-PnL: None, Gewinnrate: None
- Profit Factor: None NO_TRADES; Max Drawdown: 0; Kapitalauslastung: 0.8
- Gebuehren nach Art: {'protocol_fee_lamports': 983700, 'creator_fee_lamports': 295110, 'network_fee_total_lamports': 7000, 'sender_tip_lamports': 0, 'rent_locked_lamports': 2039280, 'rent_refunded_lamports': 0}; Kosten-Evidenz: {'MEASURED': 1}; Fill-Evidenz: {'OBSERVED_STATE_PROXY': 1}
- Cash-Reconciliation: {'initial': 1000000000, 'ledger_cash_sum': -101695090, 'end_cash_per_ledger': 898304910, 'run_end_balance': 898304910, 'reconciled': True}
- Block-Bootstrap (Intent-PnL): {'blocks': 0, 'mean': None, 'ci_low': None, 'ci_high': None, 'status': 'INSUFFICIENT_BLOCKS', 'seed': 20260916, 'resamples': 2000}

## Latenz (lokal gemessen, gleiche Clock-Epoch)
- receive→decode: {'n': 1, 'min': 0, 'p50': 0, 'p95': 0, 'p99': 0, 'max': 0, 'p99_low_confidence': True}
- receive→decision (Entries): {'n': 1, 'min': 0, 'p50': 0, 'p95': 0, 'p99': 0, 'max': 0, 'p99_low_confidence': True}
- decision→enqueue: {'n': 1, 'min': 0, 'p50': 0, 'p95': 0, 'p99': 0, 'max': 0, 'p99_low_confidence': True}
- Callback-Lateness: {'n': 1, 'min': 0, 'p50': 0, 'p95': 0, 'p99': 0, 'max': 0, 'p99_low_confidence': True}
- Modellverzoegerung Entry/Exit (SIMULATED): 0 / 0 ns

## Feeds (relativ, Union-Nenner, keine globale Vollstaendigkeit)
- helius_standard: n=1, matched=1, hint-wins=1, actionable-wins=1, ties=0, missing=0, rel p50/p95=0/0 ns
- existing_rpc: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=1, rel p50/p95=None/None ns
- pumpportal: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=1, rel p50/p95=None/None ns

## Datenqualitaet
- Quality-Events: 2, kanonische Gaps: 0, Provider-Disconnects: 0, Recorder-Overflows: 0, interne Fehler: 0
- Positionen mit kanonischem Gap: []; Migrations-/Curve-complete-Exposures: []
- Quote-Scope der CREATEs: {'SOL_WSOL': 1}
- Fee-Snapshots: {'VALID': 4}

## Annahmen
- Historischer Fremd-Orderflow reagiert nicht auf unseren hypothetischen Trade (Gegenfaktualitaetsannahme). Simulierte Inklusion ist eine Annahme, keine gemessene Landung. Fill-Evidenz ist OBSERVED_STATE_PROXY. Keine Zahl behauptet global frueheste Erkennung, sicheren Fill oder garantierten Profit.
- Latenzmodell: ASSUMED_OPTIMISTIC_BASELINE; Slippage-Grenze 300 bps ist eine Ausfuehrungsgrenze, keine gezahlte Gebuehr.
