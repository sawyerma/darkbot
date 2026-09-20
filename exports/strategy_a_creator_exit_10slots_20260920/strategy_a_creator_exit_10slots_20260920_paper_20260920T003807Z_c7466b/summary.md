# DarkBot Phase-1 PAPER Bericht – Run strategy_a_creator_exit_10slots_20260920_paper_20260920T003807Z_c7466b

Kampagne: strategy_a_creator_exit_10slots_20260920 | Modus: PAPER | Config-Hash: f2a351d112fc2815 | Query: REPORT_QUERY_V1

Strategie: FAST_CREATOR_HOLD_V1 | Strategy-Hash: 9e57cd79bd7028286aafb2fe5f243849da0d7ea2d8b449c5bd4c886725cf05fd

**Qualifikationsstatus: UNQUALIFIED_COSTS** – fee rules not fully verified or attempt costs unknown

## Umfang
- Beobachtete CREATEs (lokale Union): 552, davon actionable: 486
- Entry-Intents: 23, Fills: 23, Entry-Fehler: 0
- Exits: Creator-Sell 12, Target 0, Timeout 3; Exit-Fehlversuche 0, Retry-Erschoepfungen 0
- Offen am Ende: 0, UNRESOLVED: 8 {'QUOTE_CONFLICT_ON_OPEN_POSITION': 3, 'EXIT_QUOTE_UNAVAILABLE': 5}, zensiert: 7
- Ineligible nach Grund: {'UNQUALIFIED_COSTS': 176, 'UNSUPPORTED_QUOTE': 72, 'UNSUPPORTED_FEE_MODE_MAYHEM': 15, 'VENUE_CURVE_COMPLETE': 12, 'SLOT_LIMIT': 272, 'UNSUPPORTED_FEE_MODE_HOLDER_REWARD': 3}

## Wirtschaft (Modell, OBSERVED_STATE_PROXY)
- Realisiertes Netto-PnL: 840340247 Lamports (0.840340247 SOL)
- Unrealized: UNKNOWN_EQUITY_INTERVAL; ungeklaerte Exposure (Entry-Kosten): 688012626 Lamports; konservative Untergrenze (Token-Restwert 0): 152327621
- Erwartungswert je Intent: 56022683.13333333, je Fill: 56022683.13333333, Median-PnL: -2251652, Gewinnrate: 0.3333333333333333
- Profit Factor: 21.577443710910728 ; Max Drawdown: 22505238; Kapitalauslastung: 0.8202655267234576
- Gebuehren nach Art: {'protocol_fee_lamports': 38246729, 'creator_fee_lamports': 12077924, 'network_fee_total_lamports': 192400, 'sender_tip_lamports': 0, 'rent_locked_lamports': 34818320, 'rent_refunded_lamports': 21193760}; Kosten-Evidenz: {'MEASURED': 37}; Fill-Evidenz: {'OBSERVED_STATE_PROXY': 37}
- Cash-Reconciliation: {'initial': 901144453, 'ledger_cash_sum': 55360464, 'end_cash_per_ledger': 956504917, 'run_end_balance': 1041361354, 'reconciled': False}
- Block-Bootstrap (Intent-PnL): {'blocks': 1, 'mean': 56022683.13333333, 'ci_low': None, 'ci_high': None, 'status': 'INSUFFICIENT_BLOCKS', 'seed': 20260916, 'resamples': 2000}

## Latenz (lokal gemessen, gleiche Clock-Epoch)
- receive→decode: {'n': 516201, 'min': 4458, 'p50': 12125, 'p95': 118750, 'p99': 346541, 'max': 24432042, 'p99_low_confidence': False}
- receive→decision (Entries): {'n': 23, 'min': 449333, 'p50': 726708, 'p95': 2088375, 'p99': 2396292, 'max': 2396292, 'p99_low_confidence': True}
- decision→enqueue: {'n': 37, 'min': 12708, 'p50': 18500, 'p95': 74833, 'p99': 81041, 'max': 81041, 'p99_low_confidence': True}
- Callback-Lateness: {'n': 37, 'min': 47666, 'p50': 69709, 'p95': 313083, 'p99': 339375, 'max': 339375, 'p99_low_confidence': True}
- Modellverzoegerung Entry/Exit (SIMULATED): 0 / 0 ns

## Feeds (relativ, Union-Nenner, keine globale Vollstaendigkeit)
- helius_standard: n=258091, matched=552, hint-wins=549, actionable-wins=480, ties=0, missing=0, rel p50/p95=0/0 ns
- existing_rpc: n=258110, matched=552, hint-wins=3, actionable-wins=6, ties=0, missing=0, rel p50/p95=179292125/269216292 ns
- pumpportal: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=552, rel p50/p95=None/None ns

## Datenqualitaet
- Quality-Events: 43539, kanonische Gaps: 20, Provider-Disconnects: 0, Recorder-Overflows: 0, interne Fehler: 0
- Positionen mit kanonischem Gap: []; Migrations-/Curve-complete-Exposures: []
- Quote-Scope der CREATEs: {'SOL_WSOL': 498, 'UNSUPPORTED_QUOTE': 54}
- Fee-Snapshots: {'VALID': 142, 'RPC_ERROR': 30, 'PRIORITY_UNKNOWN': 9}

## Annahmen
- Historischer Fremd-Orderflow reagiert nicht auf unseren hypothetischen Trade (Gegenfaktualitaetsannahme). Simulierte Inklusion ist eine Annahme, keine gemessene Landung. Fill-Evidenz ist OBSERVED_STATE_PROXY. Keine Zahl behauptet global frueheste Erkennung, sicheren Fill oder garantierten Profit.
- Latenzmodell: ASSUMED_OPTIMISTIC_BASELINE; Slippage-Grenze 300 bps ist eine Ausfuehrungsgrenze, keine gezahlte Gebuehr.
