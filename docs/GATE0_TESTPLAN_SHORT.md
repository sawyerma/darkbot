# Gate 0 — Kurzplan

Status: **FIRST / HARD GATE**

## Ziel

Vor jedem weiteren Solana- oder BSC-Ausbau messen wir, wie schnell ein neuer Coin ab dem ersten tatsächlich kaufbaren Zeitpunkt gekauft werden kann.

Testobjekte:
- Solana / Pump.fun
- BSC / Four.meme
- BSC / Flap

## Zeitdefinition

```
T_pre   = frühester verwertbarer Hinweis vor Handelbarkeit
T0      = Coin erstmals tatsächlich kaufbar
T_ready = BUY vollständig gebaut + signiert
T_send  = BUY an schnellsten Sendepfad übergeben
T_buy   = unser BUY tatsächlich erfolgreich ausgeführt
```

Kennzahlen:

```
pre_lead  = T_pre - T0
ready_gap = T_ready - T0
send_gap  = T_send - T0
buy_delay = T_buy - T0
```

**Hauptmetrik: `buy_delay`.**

## Gate

- `buy_delay < 1 ms` → PASS
- `1–3 ms` → ADAPT, nur konkreten gemessenen Engpass beheben und erneut testen
- `>3 ms` → aktueller Pfad FAIL
- `~10 ms+` trotz schnellstem realistischen Pfad → STOP für diese Venue/Chain

Zusätzlich messen:
- erster technisch möglicher BUY: JA/NEIN
- Anzahl erfolgreicher BUYs vor uns
- tatsächlicher Einstiegspreis vs. erster möglicher Einstiegspreis
- Revert / Drop / Failure

## Minimaler Test-Harness

Ein kleiner nativer Rust-Harness, kein kompletter Bot:

```
gate0/
  Cargo.toml
  src/
    main.rs
    clock.rs
    result.rs
    solana_pump.rs
    bsc_four.rs
    bsc_flap.rs
```

Keine Strategy C, keine große DB-Architektur, keine UI.

## Testablauf

1. Messkern bauen: monotone Nanosekunden-Zeitmessung, Hotpath ohne DB/Print.
2. Pump.fun-Probe: Launch erkennen, BUY ableiten/bauen/signieren, Zeiten messen.
3. Four.meme-Probe: Pending/Launch früh erkennen, BUY vorbereiten, Zeiten messen.
4. Flap-Probe: identischer Test unter gleichen Bedingungen.
5. Trockenlauf: Parser, Timing, Build/Sign lokal prüfen; noch kein Broadcast.
6. Hardware-Test auf Zielsystem.
7. Kontrollierter Mainnet-Test mit kleinem Testbetrag.
8. On-chain auswerten: `T_buy-T0`, Reihenfolge, BUYs vor uns, Fehler.
9. Ergebnis dokumentieren und Plan entscheiden: PASS / ADAPT / STOP.

## BSC-Vorlauf separat bewerten

```
pre_lead deutlich < 0 ms  → echter Vorab-Vorteil
pre_lead ~ -10 ms oder besser → ursprüngliches Ziel bestätigt
pre_lead ~ 0 ms            → kein relevanter Vorab-Vorteil
T_pre > T0                 → Vorab-Vorteil für diese Venue widerlegt
```

## Projektregel

Gate 0 kommt vor:
- weiterem BSC-Ausbau
- Solana-Native-Rebuild
- Strategy-C-Integration in den neuen Low-Latency-Pfad

Erst reale Messergebnisse entscheiden, ob der Plan fortgeführt, angepasst oder beendet wird.
