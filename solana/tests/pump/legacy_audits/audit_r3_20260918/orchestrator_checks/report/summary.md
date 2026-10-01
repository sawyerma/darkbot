# DarkBot Phase-1 PAPER Bericht – Run audit_r3_no_feeds_paper_20260918T122010Z_b24307

Kampagne: audit_r3_no_feeds | Modus: PAPER | Config-Hash: 3fedc8378e70ed60 | Query: REPORT_QUERY_V1

**Qualifikationsstatus: UNQUALIFIED_COSTS** – fee rules not fully verified or attempt costs unknown

## Umfang
- Beobachtete CREATEs (lokale Union): 0, davon actionable: 0
- Entry-Intents: 0, Fills: 0, Entry-Fehler: 0
- Exits: Creator-Sell 0, Target 0, Timeout 0; Exit-Fehlversuche 0, Retry-Erschoepfungen 0
- Offen am Ende: 0, UNRESOLVED: 0 {}, zensiert: 0
- Ineligible nach Grund: {}

## Wirtschaft (Modell, OBSERVED_STATE_PROXY)
- Realisiertes Netto-PnL: 0 Lamports (0 SOL)
- Unrealized: NONE; ungeklaerte Exposure (Entry-Kosten): 0 Lamports; konservative Untergrenze (Token-Restwert 0): 0
- Erwartungswert je Intent: None, je Fill: None, Median-PnL: None, Gewinnrate: None
- Profit Factor: None NO_TRADES; Max Drawdown: 0; Kapitalauslastung: None
- Gebuehren nach Art: {'protocol_fee_lamports': 0, 'creator_fee_lamports': 0, 'network_fee_total_lamports': 0, 'sender_tip_lamports': 0, 'rent_locked_lamports': 0, 'rent_refunded_lamports': 0}; Kosten-Evidenz: {}; Fill-Evidenz: {}
- Cash-Reconciliation: {'initial': 1000000000, 'ledger_cash_sum': 0, 'end_cash_per_ledger': 1000000000, 'run_end_balance': 1000000000, 'reconciled': True}
- Block-Bootstrap (Intent-PnL): {'blocks': 0, 'mean': None, 'ci_low': None, 'ci_high': None, 'status': 'INSUFFICIENT_BLOCKS', 'seed': 20260916, 'resamples': 2000}

## Latenz (lokal gemessen, gleiche Clock-Epoch)
- receive→decode: {'n': 0, 'min': None, 'p50': None, 'p95': None, 'p99': None, 'max': None, 'p99_low_confidence': True}
- receive→decision (Entries): {'n': 0, 'min': None, 'p50': None, 'p95': None, 'p99': None, 'max': None, 'p99_low_confidence': True}
- decision→enqueue: {'n': 0, 'min': None, 'p50': None, 'p95': None, 'p99': None, 'max': None, 'p99_low_confidence': True}
- Callback-Lateness: {'n': 0, 'min': None, 'p50': None, 'p95': None, 'p99': None, 'max': None, 'p99_low_confidence': True}
- Modellverzoegerung Entry/Exit (SIMULATED): 0 / 0 ns

## Feeds (relativ, Union-Nenner, keine globale Vollstaendigkeit)
- helius_standard: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=0, rel p50/p95=None/None ns
- existing_rpc: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=0, rel p50/p95=None/None ns
- pumpportal: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=0, rel p50/p95=None/None ns

## Datenqualitaet
- Quality-Events: 0, kanonische Gaps: 0, Provider-Disconnects: 0, Recorder-Overflows: 0, interne Fehler: 0
- Positionen mit kanonischem Gap: []; Migrations-/Curve-complete-Exposures: []
- Quote-Scope der CREATEs: {}
- Fee-Snapshots: {}

## Annahmen
- Historischer Fremd-Orderflow reagiert nicht auf unseren hypothetischen Trade (Gegenfaktualitaetsannahme). Simulierte Inklusion ist eine Annahme, keine gemessene Landung. Fill-Evidenz ist OBSERVED_STATE_PROXY. Keine Zahl behauptet global frueheste Erkennung, sicheren Fill oder garantierten Profit.
- Latenzmodell: ASSUMED_OPTIMISTIC_BASELINE; Slippage-Grenze 300 bps ist eine Ausfuehrungsgrenze, keine gezahlte Gebuehr.
