# PHASE P6-LT — Shadow-Betrieb (3-Tage-Livetest ohne Geld)

Status: Phasen-Definition, ergänzt v4.4 §P6-LT. Keine neue Architektur.

## Ziel

Den **finalen Produktionscode** drei Tage gegen Mainnet laufen lassen, identisch zu LIVE bis auf eine Stelle: an der Send-Boundary wird nicht gesendet, Safety reserviert kein Kapital. Ergebnis ist Evidence, kein Geld:

```text
Hat der Bot Liquidationen erkannt?            → Health-Records
Hätte er eine gültige TX gebaut?              → Build + Simulation
Hätte er sie rechtzeitig gebaut?              → T_sign vs. Slot des echten Gewinners
Hätte sie sich gelohnt?                       → virtueller Netto (Economics)
Läuft er drei Tage ohne Panic/unerklärten HALT? → Stabilität
```

## Was Shadow NICHT beweist

Landing, Tips, Jito-Verhalten, echte Token-Deltas, realisierter PnL. Das ist P8-LT mit kleinem Kapital. Shadow qualifiziert alles davor.

## Voraussetzungen (Gates vor Start)

```text
G1  cargo build + cargo test grün (beide Feature-Sets), Logs im Ledger
G2  C0-GO im Ledger (Census gelaufen, Long-Tail > Kostenbasis)
G3  P4A-03 Sizing minimal: Pair-Wahl, Repay-Betrag über max_seizable_for_repay, calc_amount aus Crate
G4  RouteProvider: Jupiter swap-instructions (Free-Tier bestätigt, P0-02) ODER nur NO_SWAP mit virtuellem Inventar
G5  Bootstrap auf Mainnet: StreamReady → Snapshot (P0 + Dependencies) → Reconcile sauber → READY, Health-Records stichprobenartig gegen TS-SDK/eva01 geprüft
```

Ohne G3 misst der Shadow nur Erkennung, keine Pläne. Das ist kein Shadow-Mangel, sondern ein Produktstand.

## Messstrukturen (Bau, quellenunabhängig)

Drei Zusätze zum bestehenden Runner, alle hinter der Send-Boundary, kein Einfluss auf den Live-Pfad:

**M1 Shadow-Simulation.** Die signierte TX (bzw. das Bundle) wird im Shadow ebenso simuliert wie im Live-Pfad; Ergebnis `SimResult` und CU in den Record. Beantwortet "fehlerfrei gebaut", nicht nur "gebaut".

**M2 Winner-Matcher.** Nach jedem Shadow-Kandidaten: `getSignaturesForAddress(account)` ab dem Kandidaten-Slot, Transaktionen mit `end_liquidation` finden, Gewinner (liquidation_receiver), Landed-Slot, TX-Version, Tip (falls Bundle sichtbar). Vergleich: `unser T_sign` vs. `Gewinner-Slot` → LossReason:

```text
STATE_RACE        Gewinner-Slot < unser T_detect-Slot      (zu spät gesehen)
BUILD/ROUTING     T_sign − T_detect > Deadline              (zu langsam gebaut)
THRESHOLD_TOO_HIGH Gewinner bei niedrigerem Netto als unser MIN_PROFIT
ECONOMIC_SKIP     wir haben bewusst verworfen
INGRESS           Gewinner vor unserem T_rx
NONE              wir waren vorher fertig (Shadow-Win)
```

**M3 Virtuelles Ledger.** Pro Kandidat: erwarteter Netto (Economics), Funding-Klasse, TxVersion, Simulation PASS/FAIL, LossReason. Tagesweise aggregiert: Kandidaten, Shadow-Wins, Summe erwarteter Netto der Wins, Latenz p50/p95/p99 pro Stufe. Jede Zeile trägt `pnl_complete=false`.

## Konfiguration

```text
P0_MODE=shadow            Helius Free (WSS + RPC), kein Paid-Provider
Mac durchgehend an        Sleep/Updates aus; Unterbrechung = Reconnect-Test, kein Abbruch
MIN_PROFIT                deine Kostenbasis (Fees + Tip + Swap), nichts anderes
Keypair                   nur zum Signieren; Wallet darf leer sein
Dauer                     72 h, danach Auswertung — nicht vorher nachjustieren
```

## Ablauf

```text
Tag 0   Start, Bootstrap-Log prüfen (G5), erste 100 Health-Records plausibilisieren
Tag 1–3 laufen lassen; nur eingreifen bei Panic/unerklärtem HALT (dann: Ursache, Fix, Neustart = neuer Test)
Tag 3   Auswertung aus dem Ledger, Gate-Entscheidung
```

## Messungen (Ledger-Pflichtfelder)

```text
Stabilität     uptime, reconnects, recoveries, orphaned writes, HALTs mit Grund, Panics
Erkennung      MarginfiAccount-Updates/Tag, liquidierbar erkannt, Unsupported (fail-closed) mit Grund
Planung        Pläne gebaut je FundingClass × TxVersion, verworfen mit Grund, Bytes/Accounts/CU
Simulation     PASS/FAIL je Klasse, Fehlergründe
Latenz         T_rx→T_detect, T_detect→T_ready, T_ready→T_sign, p50/p95/p99/max, Sample-Zahl
Race           Shadow-Wins, Losses je LossReason, Gewinner-Konzentration, deren TxVersion
Ökonomie       erwarteter Netto der Wins (virtuell), Kostenbasis, MIN_PROFIT
```

## Gates

```text
PASS   kein Panic; jeder HALT erklärt und recovered; Reconcile nach jedem Reconnect sauber;
       Simulation-FAIL-Rate unter dem vorab festgelegten Wert; ≥ N Shadow-Wins (N vorher im Ledger fixiert)
ADAPT  Wins vorhanden, aber Verluste dominieren eine Stufe (BUILD/ROUTING/THRESHOLD) → gezielt diese Stufe
STOP   null Shadow-Wins über 72 h UND Gewinner konsistent vor unserem T_rx → Nische existiert mit WSS nicht;
       Entscheidung Geyser (Stufe 2) oder Markt wechseln — nicht weiterbauen
```

## Danach

PASS → P8-LT: Staging-LIVE (echte, wertlose TXs), dann Mainnet mit dedizierter Wallet und Limits. Erst dort fallen Landing, Tips und Token-Deltas als Messwerte an. Erst dort heißt es "funktionsfähig".

## Nicht-Ziele dieser Phase

Keine Latenzoptimierung, kein Provider-Wechsel, kein Tuning von MIN_PROFIT während des Laufs, keine zweite Codebasis für "Test". Der Shadow ist der Produktionscode.
