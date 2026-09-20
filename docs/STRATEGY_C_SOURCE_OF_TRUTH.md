# DarkBot Strategy C – Source of Truth

**Strategy-ID:** `LEGACY_BACKEND_FILTER_V1`
**Inhalt:** die vollständige Handels-Policy des alten Backends (`old_system/backend`), 1:1 aus dem Code gelesen.
**Prinzip wie bei B:** nur die Policy wird übernommen. Feeds, Decoder, Dedup, Coverage, Fees, QuoteEngine, Storage, Replay bleiben die aktuelle Engine.

Quellen (alle Zeilenangaben beziehen sich auf `old_system/backend/darkbot/`):
`paper.py` (Lifecycle, Entry, Exit), `execution.py` (Quote-/Fill-/Signal-Mathematik), `strategy.py` (17 Features, Filter), `market.py` (Tradable-Gate, Zählung, Cutoff, Creator-Sell-Erkennung), `config.py` (StrategyConfig), `main.py` (Verdrahtung), `backtest.py` (Offline-Variante), `config/*.json` (6 Profile). Die `.bak`-Dateien vom 13.09. zeigen: Creator-Sell-Logik war in jeder Version enthalten; am 13.09. kamen nur der 0-ms-Entry (`on_launch`) und reine Positionsmetriken dazu.

---

## 1. Was das alte Backend war

Keine feste Strategie, sondern eine **config-getriebene Filter-Engine mit fest verdrahteten Exit-Regeln**:

```text
CREATE (SOL-Quote)
→ observation_ms beobachten (0 / 500 / 1000 ms)
→ Feature-Snapshot mit exaktem Cutoff
→ alle Filter {feature, min, max} müssen bestehen
→ Slot frei?  sonst Kandidat endgültig verworfen
→ Entry-Fill nach fester Latenz, außer Creator hat bis dahin verkauft
→ halten
→ EXIT bei: Creator-Sell  >  Netto-Target  >  Netto-Stop  |  Timeout
```

Strategy A und B sind jeweils nur ein Ausschnitt davon. Strategy C ist der ganze Mechanismus; die sechs alten Config-Dateien sind Profile von C.

---

## 2. Parameter (identisch zur alten `StrategyConfig`, `config.py` 143–160)

```text
candidate_name
observation_ms            0..n   (alt live nur 0 / 500 / 1000 möglich, siehe 4.4)
filters[]                 {feature, min, max}, max. laut Contract, jedes Feature höchstens einmal
trade_amount_lamports     Swap-Budget INKL. Pump-/Creator-Fee
target_net_multiple       > 1
stop_net_multiple         0 <= x < 1
max_hold_ms               ab erfolgreichem Entry-Fill
base_fee_lamports         alt fix 5.000
priority_fee_lamports     alt fix 100.000
max_slippage_bps          alt 300
entry_latency_ms          gilt für Entry UND Exit (main.py 110–116)
```

CLI zusätzlich: `--max-open-positions` (Default **1**), `--account-rent-lamports` (Default **0**).
Nicht konfigurierbar, sondern hart im Code: Creator-Sell-Exit, Creator-Skip vor Fill, eine Entscheidung pro Launch.

---

## 3. Die sechs alten Profile

| candidate_name | obs | Filter | Size | Target | Stop | Hold | Latenz |
|---|---|---|---|---|---|---|---|
| `FAST_CREATOR_HOLD_2X` | 500 | – | 98.370.000 | 2.0 | 0 | 590 s | 800 |
| `PRE_PUMP_01_500MS_10USD_2X` | 500 | buy_count ≥ 3; unique_buyers ≥ 3; sell_count = 0; dev_initial_buy 0..0.20 | 98.370.000 | 2.0 | 0 | 300 s | 800 |
| `LAUNCH_SNIPER_0MS_15X_BEST_CASE` | 0 | – | 98.370.000 | 1.5 | 0 | 590 s | **0** |
| `PAPER_EXPLORE_01_NOT_EDGE` | 1000 | buy_count ≥ 2; unique_buyers ≥ 2; net_buy_flow_sol ≥ 0.03; dev_initial_buy ≤ 0.20 | 50.000.000 | 2.0 | 0.65 | 30 s | 800 |
| `EXAMPLE_NOT_A_DECISION` | 1000 | buy_count ≥ 8; unique_buyers ≥ 6; net_buy_flow_sol ≥ 3; dev_initial_buy ≤ 0.20 | 50.000.000 | 2.0 | 0.65 | 30 s | 800 |
| `SMOKE_ALL_TRADABLE_NOT_EDGE` | 1000 | – | 50.000.000 | 2.0 | 0.65 | 30 s | 800 |

Alle: base 5.000, priority 100.000, slippage 300 bps.
Die Namen sagen selbst, was ernst gemeint war: `EXAMPLE`, `NOT_EDGE`, `SMOKE` und `BEST_CASE` (0 ms Latenz) sind Test-/Grenzfallprofile. Die beiden echten Kandidaten sind **`FAST_CREATOR_HOLD_2X`** und **`PRE_PUMP_01_500MS_10USD_2X`**. Eine Config mit 5 BUY / 5 Unique / 1.05 / 60 s (heutige Strategy B) existiert im Backend nicht.

Strategy C liefert alle sechs als `config/strategy_c_<name>.json`; kein Profil ist in Python hartkodiert.

---

## 4. Vorauswahl – exakt

### 4.1 Tradable-Gate (vor jeder Strategie)
`market.py` 760–769: handelbar nur, wenn `quote_mint == 11111111111111111111111111111111` (SOL). Alles andere wird getrackt, aber nie gehandelt. `paper.py` 187: `if not tracker.tradable: return` – solche Launches erzeugen nicht einmal eine Decision.

### 4.2 T0 und Zählregeln
- T0 = lokaler monotoner Empfang der CREATE-Notification.
- Trades in der CREATE-Transaktion derselben Mint (Initial-BUY des Creators) zählen in **keinem** Feature (`market.py` 664–668). Sie liefern nur (a) den Curve-Startzustand (Reserven nach dem Dev-Buy) und (b) `dev_initial_buy`.
- Fehlgeschlagene Transaktionen werden verworfen (645–647). Trades auf unbekannte Mints sind Orphans.
- Dedup über `signature|type|mint|event_index`.
- Creator = `CreateEvent.creator`, ersatzweise `user`.

### 4.3 Exakter Cutoff
Deadline = `T0 + observation_ms`. Vor dem Anwenden eines Events mit `receive > deadline` wird der Snapshot eingefroren (737–739), sonst feuert der Timer. **Events mit `receive <= deadline` zählen, spätere nie.** Entscheidung genau einmal pro Launch (`_decided`, `paper.py` 184–190).

### 4.4 Snapshot-Raster (alte Einschränkung, nicht übernehmen)
Live gab es Snapshots nur bei 500 und 1000 ms (Contract `runtime_snapshot_ms`); `on_snapshot` reagiert nur, wenn die Deadline exakt `T0 + observation_ms` ist (`paper.py` 159–165). 0 ms lief über `on_launch` direkt aus dem CREATE. In C ist `observation_ms` frei; Timer über die bestehende Clock/Scheduler-Abstraktion wie bei B.

### 4.5 Die 17 Features (`strategy.py` 45–70)

| Feature | Definition | NULL wenn |
|---|---|---|
| buy_count / sell_count | Anzahl BUY / SELL bis Cutoff | – |
| buy_volume_sol / sell_volume_sol / net_buy_flow_sol | Lamports-Summen / 1e9, exakt | – |
| unique_buyers | Anzahl unterschiedlicher BUY-Actors | – |
| unique_buyer_ratio | unique_buyers / buy_count | buy_count = 0 |
| largest_buy_sol | größter Einzel-BUY | kein BUY |
| buyer_volume_concentration | HHI: Σ (Volumen_i / Gesamt)² | BUY-Volumen = 0 |
| first5_buyer_concentration | Gesamtvolumen der ersten 5 unterschiedlichen Käufer / BUY-Volumen | BUY-Volumen = 0 |
| first_sell_latency_ms | (erster SELL − T0) in ms | kein SELL |
| price_change | price_t / price_t0 − 1, price = vsol / vtok | Preis fehlt |
| max_drawdown | max über Events von 1 − price / bisheriger Peak (Peak startet bei price_t0) | price_t0 fehlt |
| dev_initial_buy | Initial-BUY-Tokens / token_total_supply | **kein Initial-BUY** |
| curve_progress_pct | 100 · (1 − real_t / initial_real), real_t = vtok_t − (initial_vtok − initial_real) | Startwerte fehlen |
| research_estimated_price_impact | Durchschnittspreis eines Referenz-Buys / price_t − 1 | live immer NULL (main.py übergibt kein `research_quote_lamports`) |
| provider_gap_flag | bool, nicht filterbar | – |

Quotienten: `ROUND_HALF_EVEN` auf `ratio_decimal_places` (aktuelle Engine: 12, first_buyer_window 5 – passt).
Exakt übernommen: `actor = event.actor or ""` (ein fehlender Actor wäre die Wallet `""`). Praktisch ohne Wirkung, weil jedes TradeEvent einen `user` trägt.

### 4.6 Filterauswertung (`strategy.py` 117–138)
- Jeder Filter: `value is not None and (min is None or value >= min) and (max is None or value <= max)`.
- **NULL ⇒ Filter nicht bestanden.** Konsequenz: `dev_initial_buy max 0.20` lässt nur Launches durch, bei denen der Creator überhaupt einen Initial-BUY gemacht hat (in der Aufnahme fehlte er bei 19 von 249).
- Reihenfolge der Ergebnisse: Gap-Flag (`GAP`, `AMBIGUOUS_EVENT`, `MISSING_FIELD` oder `provider_gap_flag = true`) ⇒ `DATA_INCOMPLETE`; sonst erster fehlgeschlagener Filter ⇒ `REJECT_FILTER` (mit `first_failed_filter`); sonst `PASS`.
- Leere Filterliste ⇒ jeder handelbare Launch ist `PASS`.

### 4.7 Slot-Regel (`paper.py` 206–214)
`len(active) >= max_open_positions` ⇒ `SKIPPED_SLOT_BUSY`. Belegt zählen ENTRY_PENDING, OPEN und EXIT_PENDING. **Kein Nachrücken, keine Warteschlange:** der Launch ist entschieden und kommt nie wieder. Mit Default 1 Slot und 590 s Hold ergibt das rund einen Trade pro zehn Minuten.

---

## 5. Entry – exakt

1. Decision-Zeit = Deadline (bzw. T0 bei 0 ms). `decision_state` = letzter Curve-Zustand zu diesem Zeitpunkt. Fehlt er ⇒ `ENTRY_CURVE_UNAVAILABLE`, PnL 0.
2. Modellierter Fill = `decision + entry_latency_ms`. Slot ist ab Decision reserviert.
3. **Creator-Skip** (`paper.py` 309–322): `creator_sell_offset <= fill_offset` ⇒ `PAPER_ENTRY_SKIPPED_CREATOR_SOLD`, Slot frei, **keine Kosten**. Deckt Creator-Sells während der Observation UND während der Entry-Latenz ab. (Strategy A prüft nur zum Decision-Zeitpunkt, B gar nicht.)
4. `fill_state` = letzter beobachteter Zustand mit `receive <= fill_offset` – ohne Alterslimit.
5. Quote (`execution.py` 279–301), exact-quote-in:
   ```text
   fee_bps    = protocol_bps + (creator_bps falls Creator gesetzt)      # alt 95 + 30
   net_sol    = budget * 10000 // (10000 + fee_bps)
   fees       = ceil(net_sol*protocol_bps/10000) + ceil(net_sol*creator_bps/10000)
   falls net_sol + fees > budget: net_sol -= Überschuss
   curve_in   = net_sol - 1
   tokens     = curve_in * vtok // (vsol + curve_in)
   spent      = net_sol + fees
   ```
   ⇒ `trade_amount_lamports` ist **Swap-Budget inklusive Pump-/Creator-Fee** (gleiche Semantik wie heute B: `SWAP_BUDGET_INCLUDING_PROTOCOL_FEES`; nicht die Gesamtbudget-Semantik der aktuellen A-Config).
6. Prüfungen (`simulate_entry` 314–339):
   - `tokens <= 0` oder `tokens > real_token_reserves` ⇒ `CURVE_UNAVAILABLE`
   - `tokens_fill < tokens_decision * (10000 − slippage) // 10000` ⇒ `SLIPPAGE_REJECTED`
   - in beiden Fällen: Netzwerkgebühr verloren, Slot frei, kein zweiter Versuch.
7. `entry_cost = spent + (base + priority) + rent`. Das ist die Basis für Target und Stop.
8. Timeout-Trigger = `fill_offset + max_hold_ms` (Uhr startet am erfolgreichen Fill).

---

## 6. Ausstieg – exakt

Bewertung nur bei neuen BUY/SELL-Zuständen der Mint mit `fill_offset < receive <= timeout_trigger` (`paper.py` 256–297). Der Fill-Zustand selbst wird nicht bewertet.

**Netto-Liquidationswert** (`execution.py` 365–368):
```text
gross = tokens * vsol // (vtok + tokens)
nlv   = gross − ceil-Fees(gross) − (base + priority)
```

**Priorität auf demselben Zustand:**

| # | Grund | Bedingung | Trigger-Zeit |
|---|---|---|---|
| 1 | `CREATOR_SELL` | erster SELL mit `actor == creator` nach unserem Fill | Empfangszeit dieses SELLs |
| 2 | `TARGET` | `nlv >= entry_cost * target_net_multiple` | Empfangszeit des Zustands |
| 3 | `STOP` | `nlv <= entry_cost * stop_net_multiple` | Empfangszeit des Zustands |
| – | `TIMEOUT` | Timer bei `fill + max_hold_ms` | Timer-Zeit; Zustände danach werden ignoriert |

Wichtig:
- Creator-Sell ist **immer aktiv**, in jedem Profil. Erkannt wird nur der direkte SELL der Creator-Wallet (keine Transfers, keine Zweitwallets).
- **`stop = 0` ist nicht „kein Stop":** `nlv <= 0` löst STOP aus, also sobald der Verkaufserlös die eine Netzwerkgebühr nicht mehr deckt (Rug auf ~0). In der Nachrechnung trat das real auf.
- Target und Stop sind Netto-Werte, nie Chartpreis.

**Exit-Fill** (`simulate_exit` 341–363): Fill = Trigger + `entry_latency_ms`, Zustand = letzter mit `receive <= fill`. `min_out = sell_net(trigger_state) * (10000 − slippage) // 10000`, wobei `sell_net` = Curve-Erlös nach Pump-/Creator-Fee, vor Netzwerkgebühr. Erlös am Fill-Zustand darunter ⇒ `SLIPPAGE_REJECTED`; Erlös ≤ 0 ⇒ `CURVE_UNAVAILABLE`. In beiden Fällen: Netzwerkgebühr verloren, **sofortiger Retry** mit neuem Trigger = alte Fill-Zeit, **unbegrenzt** (`paper.py` 510–539).

**PnL** (555–559): `exit_net − network_exit − entry_cost − Σ fehlgeschlagene Exit-Gebühren`.

**Stop des Laufs** (858–889): Pending Entries werden storniert, offene Positionen als `INTERRUPTED` aus dem PnL ausgeschlossen – kein erfundener Verkauf.

**Abweichung PAPER ↔ alter Backtest:** `backtest.py` kennt weder Creator-Skip noch Creator-Exit und bucht nach `exit_max_sell_attempts` (Default 1) einen Totalverlust `UNRESOLVED_ZERO`. **Strategy C folgt PAPER**, nicht dem Backtest.

---

## 7. Umsetzung in der aktuellen Engine

Neues Modul `darkbot/strategies/strategy_c_legacy_backend.py`, gleiche `StrategyPolicy`-Schnittstelle. Was fehlt bzw. anzupassen ist:

| Punkt | Heute | Für C nötig |
|---|---|---|
| Config-Enum | `FAST_CREATOR_HOLD_V1 \| LEGACY_5BUY_5UNIQUE_V1` (`config.py` 67) | `LEGACY_BACKEND_FILTER_V1` ergänzen; Schema: `filters[{feature,min,max}]`, `stop_net_multiple`, `observation_ms` frei; keine eingefrorenen Werte wie bei B (492–498) |
| Observation + Features | B zählt nur BUY/Unique im RAM | C führt pro Mint O(1)-Aggregate aller 16 numerischen Features im RAM (wie alter `TokenTracker`), Entscheidung im Timer-Callback; Coverage-Gap ⇒ `DATA_INCOMPLETE` wie bei B |
| Creator-Skip | A: nur `creator_sell_known` zur Decision; B: hart `False` | C: Decision-Check **plus** Cancel, wenn Creator-Sell bis zum modellierten Entry-Fill eintrifft (kostenfrei, `ENTRY_CANCEL`, Grund `CREATOR_SOLD_BEFORE_FILL`) |
| STOP | `ExitReason` kennt nur CREATOR_SELL / TARGET / TIMEOUT (`domain.py` 93–96) | `STOP` ergänzen; `EXIT_PRIORITY = CREATOR_SELL > TARGET > STOP > TIMEOUT`; Policy-Methode `stop_reached(nlv, entry_cost, stop)`; Latch in `_position_tick` neben TARGET (`paper.py` 591–596) |
| Creator-Exit | `creator_exit_enabled()` | C: immer `True` |
| Slot belegt | `SLOT_LIMIT` ⇒ SKIP | passt (einmalige Entscheidung bleibt) |
| Sizing | `USD_SWAP_BUDGET` + `SWAP_BUDGET_INCLUDING_PROTOCOL_FEES` | passt; alternativ `FIXED_SOL` für exakte Legacy-Nachstellung. `98.370.000 = $10` nicht hartkodieren |
| Latenz | `latency_model` 0/0 | gemeinsames Szenario `LEGACY_BACKEND_LATENCY 800/800` (und 0/0 für das Sniper-Profil); nicht in die Strategy-Klasse |
| Reporting | B: observation_start/end, buy_count, unique_buyers, qualified | C: kompletter Feature-Snapshot, `first_failed_filter`, Auswertung je Filter, Decision-Action |

**Bewusst NICHT aus dem alten Backend übernehmen** (Engine bleibt modern): feste 5.000/100.000-Gebühren und Rent 0, statische 95/30-bps-Fee-Datei, unbegrenzte Sofort-Retries (heute: 3 Versuche mit Backoff), `UNRESOLVED_ZERO`, Snapshot-Raster 500/1000.

**Ein Punkt verändert Ergebnisse stark und muss als Szenario sichtbar sein:** das alte Backend füllte immer auf dem letzten bekannten Zustand, egal wie alt. Die aktuelle Engine verlangt Reserven jünger als `quote_max_age_ms = 250` und latcht/zensiert sonst. Bei toten Tokens verkaufte das alte Backend also fiktiv zum letzten Kurs; die neue Engine tut das nicht. Für den A/B/C-Vergleich gilt die neue Regel für alle drei gleich; zusätzlich ein Research-Szenario `LEGACY_STALE_STATE_FILL` nur zur Nachstellung alter Zahlen.

---

## 8. Pflicht-Tests

```text
C01  Nicht-SOL-Quote → keine Decision
C02  Initial-BUY der CREATE-Tx zählt in keinem Feature, setzt aber dev_initial_buy und price_t0
C03  Event mit receive == deadline zählt, receive > deadline zählt nicht
C04  Filter mit NULL-Wert → REJECT_FILTER (dev_initial_buy ohne Initial-BUY)
C05  min/max inklusiv; null-Grenze = unbegrenzt; first_failed_filter = erster in Config-Reihenfolge
C06  Gap im Fenster → DATA_INCOMPLETE, weder Qualifier noch Filter-Miss
C07  Leere Filterliste → PASS für jeden handelbaren Launch
C08  Slot belegt (inkl. ENTRY_PENDING) → SKIP, Launch wird nie erneut entschieden
C09  Creator-Sell während Observation → kein Entry, keine Kosten
C10  Creator-Sell zwischen Decision und modelliertem Fill → Entry-Cancel, keine Kosten
C11  Entry-Slippage > max → abgelehnt, nur Netzwerkgebühr, kein zweiter Versuch
C12  Budget enthält Pump-/Creator-Fee; Netzwerk/Rent zusätzlich; entry_cost = Summe
C13  Creator-Sell nach Fill → EXIT CREATOR_SELL, auch wenn derselbe Zustand Target erfüllt
C14  nlv = target·cost − 1 → HOLD; nlv = target·cost → TARGET
C15  stop 0.65: nlv <= 0.65·cost → STOP
C16  stop 0: nlv 0.10·cost → HOLD; nlv <= 0 → STOP
C17  Timeout exakt max_hold_ms nach Entry-Fill; spätere Zustände lösen nichts mehr aus
C18  Gleicher Zeitstempel: CREATOR_SELL > TARGET > STOP > TIMEOUT
C19  Alle sechs Profile laden, je eigener strategy_config_hash
C20  PAPER und Replay liefern identische C-Decisions; A- und B-Tests bleiben PASS
C21  Replay der Legacy-Aufnahme über Adapter: Filter-Funnel entspricht Anhang A
```

---

## Anhang A – Befund aus der erhaltenen Backend-Aufnahme

`exports/basetest_backend_v0.zip` → PAPER-Session 12.09. 23:51:30 – 13.09. 00:06:28 UTC, **897,65 s**, öffentlicher RPC, `processed`, 335 CREATEs, 249 handelbar, 9.578 BUY / 8.845 SELL, Snapshots bei 500/1000 ms. Bei 125 der 249 handelbaren Launches verkaufte der Creator innerhalb der Session, Median 38 s nach CREATE, 18 davon binnen 5 s.

Die Laufzeit ist exakt die „historische positive Referenz" der B-Dokumente; die Rohaufnahme liegt also im Repo (MASTER_TODO P1.6 nennt sie fälschlich unauffindbar). Welche Config live lief, ist nicht gespeichert (`strategy_versions` leer, PAPER-Events nur stdout).

Nachrechnung mit dem **originalen** `PaperFillModel` und PAPER-Semantik aus Abschnitt 4–6 (eigene Berechnung, 15 Minuten, in-sample, kein Nachweis):

| Profil | Slots | Filter bestanden | gefüllt | Creator-Exit | Target | Timeout | Stop | netto ≈ |
|---|---|---|---|---|---|---|---|---|
| FAST_CREATOR_HOLD_2X | 1 | 249 | 3 | 1 | 0 | 1 | 0 | −0,86 $ |
| FAST_CREATOR_HOLD_2X | 10 | 249 | 44 | 20 | 4 | 9 | 1 | +4,40 $ |
| PRE_PUMP_01 | 1 | 23 | 4 | 4 | 0 | 0 | 0 | −2,70 $ |
| PRE_PUMP_01 | 10 | 23 | 14 | 12 | 0 | 2 | 0 | −12,20 $ |
| LAUNCH_SNIPER_0MS (0 ms!) | 10 | 249 | 52 | 22 | 11 | 8 | 1 | +36,40 $ |
| PAPER_EXPLORE_01 | 10 | 61 | 45 | 7 | 1 | 32 | 4 | +1,13 $ |
| SMOKE_ALL | 10 | 249 | 191 | 20 | 7 | 131 | 27 | −37,42 $ |

B-Referenz (1 s, 5/5, 1.05, 60 s, 400/400 ms) auf derselben Aufnahme: 22 Qualifier, 21 mögliche Fills – die Trade-Zahl stimmt. Die **+9,55 $ sind mit dem Backend-eigenen Modell nicht reproduzierbar**: +5,97 $ mit Slippage-Guard (17 Fills), +1,42 $ ohne, +1,67 $ mit Creator-Skip/-Exit (14 Trades); selbst bei 0/0 ms nur rund +7,8 $.

Lesart: In diesem Datensatz entscheidet fast allein die Latenz (0 ms vs. 800 ms) und der Creator-Exit dominiert die Ausstiege. Das ist ein Hinweis, worauf der A/B/C-Test achten muss, kein Edge-Beleg.

---

## Anhang B – Implementierungsstand

Umgesetzt in `darkbot/strategies/strategy_c_legacy_backend.py` (Tracker, 17 Features, Filter, Policy), verdrahtet über `make_strategy`.

Gemeinsame Engine, additiv (A und B funktional unverändert, alle Alt-Tests PASS):
- `ExitReason.STOP`, `EXIT_PRIORITY = CREATOR_SELL > TARGET > STOP > TIMEOUT`, `strategy.stop_reached`; Latch in `_position_tick` neben TARGET. A/B liefern `stop_reached = False`.
- Creator-Skip vor Fill: `StateOwner._entry_cancelled_creator_sold` (Attempt `CANCELLED_CREATOR_SOLD`, Decision `ENTRY_CANCEL`, keine Kosten, kein Ledger-Eintrag). Nur wenn die Policy `cancel_entry_if_creator_sold_before_fill()` bejaht.
- `FIXED_SOL` + `SWAP_BUDGET_INCLUDING_PROTOCOL_FEES`: `entry_amount_lamports` ist das Swap-Budget (alte `trade_amount_lamports`), über die vorhandene `quote_buy_swap_budget`.
- Migration `0009`: `strategy_observations` + `features_json`, `filter_evaluations_json`, `first_failed_filter`, `decision_action`.
- Config: `same_event_exit_priority` mit STOP, `filters` nur für C erlaubt (max. 8, typisiert), Creator-/Target-Exit für C nicht abschaltbar, `ordinary_stop_enabled` = (`stop_net_multiple` > 0).
- `research_estimated_price_impact` ist als Filter gesperrt: im alten PAPER war das Feature immer NULL, ein Filter darauf hätte nie bestanden.

Zustandsalter: die C-Profile setzen `execution.quote_max_age_ms = research_horizon_ms = 600000`. Das ist die exakte Entsprechung des alten Verhaltens (letzter beobachteter Zustand des Trackers, beliebig alt, Tracker-Horizont 600 s). Mit dem A/B-Wert 250 ms enden ruhige Tokens bei 800 ms Latenz als `PROXY_STATE_STALE` (Entry `UNKNOWN`/`UNRESOLVED`, Slot blockiert) – Test `C07b` zeigt beide Varianten.

Bewusst modern geblieben: Fee-/Rent-/Priority-Snapshots statt fixer 105.000 Lamports, `buy_v2`-Slippage über `max_sol_cost`, 3 Exit-Versuche mit Backoff statt unbegrenzter Sofort-Retries, UNKNOWN/CENSORED statt `UNRESOLVED_ZERO`.

Tests: `tests/test_strategy_c.py` (27). `C00` lädt den Original-Code aus `old_system/backend` und verlangt für dieselbe Event-Folge einen identischen Feature-Snapshot.
