# DarkBot Phase-1 PAPER Bericht – Run strategy_a_creator_exit_10slots_clean_15m_20260920_paper_20260920T063312Z_c244df

Kampagne: strategy_a_creator_exit_10slots_clean_15m_20260920 | Modus: PAPER | Config-Hash: f2a351d112fc2815 | Query: REPORT_QUERY_V1

Strategie: FAST_CREATOR_HOLD_V1 | Strategy-Hash: 9e57cd79bd7028286aafb2fe5f243849da0d7ea2d8b449c5bd4c886725cf05fd

**Qualifikationsstatus: UNQUALIFIED_COSTS** – fee rules not fully verified or attempt costs unknown

## Umfang
- Beobachtete CREATEs (lokale Union): 206, davon actionable: 182
- Entry-Intents: 14, Fills: 14, Entry-Fehler: 0
- Exits: Creator-Sell 4, Target 0, Timeout 1; Exit-Fehlversuche 0, Retry-Erschoepfungen 0
- Offen am Ende: 1, UNRESOLVED: 8 {'EXIT_QUOTE_UNAVAILABLE': 8}, zensiert: 9
- Ineligible nach Grund: {'UNQUALIFIED_COSTS': 78, 'UNSUPPORTED_FEE_MODE_HOLDER_REWARD': 16, 'UNSUPPORTED_FEE_MODE_MAYHEM': 39, 'UNSUPPORTED_QUOTE': 25, 'INSUFFICIENT_CAPITAL': 37, 'VENUE_CURVE_COMPLETE': 6}

## Wirtschaft (Modell, OBSERVED_STATE_PROXY)
- Realisiertes Netto-PnL: -74308930 Lamports (-0.07430893 SOL)
- Unrealized: UNKNOWN_EQUITY_INTERVAL; ungeklaerte Exposure (Entry-Kosten): 792577142 Lamports; konservative Untergrenze (Token-Restwert 0): -866886072
- Erwartungswert je Intent: -14861786.0, je Fill: -14861786.0, Median-PnL: -13235906, Gewinnrate: 0.0
- Profit Factor: 0.0 ; Max Drawdown: 74308930; Kapitalauslastung: 0.7945622027817959
- Gebuehren nach Art: {'protocol_fee_lamports': 15091251, 'creator_fee_lamports': 4765666, 'network_fee_total_lamports': 98800, 'sender_tip_lamports': 0, 'rent_locked_lamports': 21193760, 'rent_refunded_lamports': 7569200}; Kosten-Evidenz: {'MEASURED': 19}; Fill-Evidenz: {'OBSERVED_STATE_PROXY': 19}
- Cash-Reconciliation: {'initial': 921786422, 'ledger_cash_sum': -880510632, 'end_cash_per_ledger': 41275790, 'run_end_balance': 41275790, 'reconciled': True}
- Block-Bootstrap (Intent-PnL): {'blocks': 1, 'mean': -14861786.0, 'ci_low': None, 'ci_high': None, 'status': 'INSUFFICIENT_BLOCKS', 'seed': 20260916, 'resamples': 2000}

## Latenz (lokal gemessen, gleiche Clock-Epoch)
- receive→decode: {'n': 188140, 'min': 4500, 'p50': 25500, 'p95': 270667, 'p99': 527417, 'max': 10143667, 'p99_low_confidence': False}
- receive→decision (Entries): {'n': 14, 'min': 466166, 'p50': 1590958, 'p95': 3118375, 'p99': 3118375, 'max': 3118375, 'p99_low_confidence': True}
- decision→enqueue: {'n': 19, 'min': 16625, 'p50': 69458, 'p95': 90167, 'p99': 90167, 'max': 90167, 'p99_low_confidence': True}
- Callback-Lateness: {'n': 19, 'min': 59416, 'p50': 368792, 'p95': 634416, 'p99': 634416, 'max': 634416, 'p99_low_confidence': True}
- Modellverzoegerung Entry/Exit (SIMULATED): 0 / 0 ns

## Feeds (relativ, Union-Nenner, keine globale Vollstaendigkeit)
- helius_standard: n=94051, matched=206, hint-wins=202, actionable-wins=176, ties=0, missing=0, rel p50/p95=0/0 ns
- existing_rpc: n=94089, matched=206, hint-wins=4, actionable-wins=6, ties=0, missing=0, rel p50/p95=162347959/256053583 ns
- pumpportal: n=0, matched=0, hint-wins=0, actionable-wins=0, ties=0, missing=206, rel p50/p95=None/None ns

## Datenqualitaet
- Quality-Events: 21480, kanonische Gaps: 4, Provider-Disconnects: 0, Recorder-Overflows: 0, interne Fehler: 0
- Positionen mit kanonischem Gap: []; Migrations-/Curve-complete-Exposures: []
- Quote-Scope der CREATEs: {'SOL_WSOL': 189, 'UNSUPPORTED_QUOTE': 17}
- Fee-Snapshots: {'VALID': 84, 'RPC_ERROR': 20, 'PRIORITY_UNKNOWN': 2}

## Annahmen
- Historischer Fremd-Orderflow reagiert nicht auf unseren hypothetischen Trade (Gegenfaktualitaetsannahme). Simulierte Inklusion ist eine Annahme, keine gemessene Landung. Fill-Evidenz ist OBSERVED_STATE_PROXY. Keine Zahl behauptet global frueheste Erkennung, sicheren Fill oder garantierten Profit.
- Latenzmodell: ASSUMED_OPTIMISTIC_BASELINE; Slippage-Grenze 300 bps ist eine Ausfuehrungsgrenze, keine gezahlte Gebuehr.
