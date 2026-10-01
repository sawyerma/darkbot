# DarkBot Phase-1 PAPER Bericht – Run p0_exit_sanity_paper_20260919T223903Z_854405

Kampagne: p0_exit_sanity | Modus: PAPER | Config-Hash: d477997581a78bbb | Query: REPORT_QUERY_V1

**Qualifikationsstatus: UNQUALIFIED_COSTS** – fee rules not fully verified or attempt costs unknown

## Umfang
- Beobachtete CREATEs (lokale Union): 96, davon actionable: 79
- Entry-Intents: 3, Fills: 3, Entry-Fehler: 0
- Exits: Creator-Sell 1, Target 1, Timeout 0; Exit-Fehlversuche 0, Retry-Erschoepfungen 0
- Offen am Ende: 1, UNRESOLVED: 0 {}, zensiert: 1
- Ineligible nach Grund: {'UNQUALIFIED_COSTS': 9, 'SLOT_LIMIT': 62, 'UNSUPPORTED_QUOTE': 16, 'UNSUPPORTED_FEE_MODE_MAYHEM': 4, 'UNSUPPORTED_FEE_MODE_HOLDER_REWARD': 1, 'VENUE_CURVE_COMPLETE': 1}

## Wirtschaft (Modell, OBSERVED_STATE_PROXY)
- Realisiertes Netto-PnL: 58329793 Lamports (0.058329793 SOL)
- Unrealized: UNKNOWN_EQUITY_INTERVAL; ungeklaerte Exposure (Entry-Kosten): 99604825 Lamports; konservative Untergrenze (Token-Restwert 0): -41275032
- Erwartungswert je Intent: 29164896.5, je Fill: 29164896.5, Median-PnL: 29164896.5, Gewinnrate: 0.5
- Profit Factor: 18.200157405736434 ; Max Drawdown: 3391236; Kapitalauslastung: 0.8007621172038456
- Gebuehren nach Art: {'protocol_fee_lamports': 5281241, 'creator_fee_lamports': 1667761, 'network_fee_total_lamports': 26000, 'sender_tip_lamports': 0, 'rent_locked_lamports': 4541520, 'rent_refunded_lamports': 3027680}; Kosten-Evidenz: {'MEASURED': 5}; Fill-Evidenz: {'OBSERVED_STATE_PROXY': 5}
- Cash-Reconciliation: {'initial': 1000000000, 'ledger_cash_sum': -42788872, 'end_cash_per_ledger': 957211128, 'run_end_balance': 957211128, 'reconciled': True}
- Block-Bootstrap (Intent-PnL): {'blocks': 1, 'mean': 29164896.5, 'ci_low': None, 'ci_high': None, 'status': 'INSUFFICIENT_BLOCKS', 'seed': 20260916, 'resamples': 2000}

## Latenz (lokal gemessen, gleiche Clock-Epoch)
- receive→decode: {'n': 44911, 'min': 4959, 'p50': 19000, 'p95': 290459, 'p99': 643333, 'max': 4498667, 'p99_low_confidence': False}
- receive→decision (Entries): {'n': 3, 'min': 287292, 'p50': 819042, 'p95': 914333, 'p99': 914333, 'max': 914333, 'p99_low_confidence': True}
- decision→enqueue: {'n': 5, 'min': 15208, 'p50': 32583, 'p95': 42583, 'p99': 42583, 'max': 42583, 'p99_low_confidence': True}
- Callback-Lateness: {'n': 5, 'min': 113125, 'p50': 173750, 'p95': 238542, 'p99': 238542, 'max': 238542, 'p99_low_confidence': True}
- Modellverzoegerung Entry/Exit (SIMULATED): 0 / 0 ns

## Feeds (relativ, Union-Nenner, keine globale Vollstaendigkeit)
- helius_standard: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=96, rel p50/p95=None/None ns
- existing_rpc: n=44911, matched=96, hint-wins=96, actionable-wins=79, ties=0, missing=0, rel p50/p95=0/0 ns
- pumpportal: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=96, rel p50/p95=None/None ns

## Datenqualitaet
- Quality-Events: 3530, kanonische Gaps: 6, Provider-Disconnects: 0, Recorder-Overflows: 0, interne Fehler: 0
- Positionen mit kanonischem Gap: []; Migrations-/Curve-complete-Exposures: []
- Quote-Scope der CREATEs: {'SOL_WSOL': 80, 'UNSUPPORTED_QUOTE': 16}
- Fee-Snapshots: {'VALID': 20, 'RPC_ERROR': 2}

## Annahmen
- Historischer Fremd-Orderflow reagiert nicht auf unseren hypothetischen Trade (Gegenfaktualitaetsannahme). Simulierte Inklusion ist eine Annahme, keine gemessene Landung. Fill-Evidenz ist OBSERVED_STATE_PROXY. Keine Zahl behauptet global frueheste Erkennung, sicheren Fill oder garantierten Profit.
- Latenzmodell: ASSUMED_OPTIMISTIC_BASELINE; Slippage-Grenze 300 bps ist eine Ausfuehrungsgrenze, keine gezahlte Gebuehr.
