# DarkNode – Gesamtinventur der Planung und verbindliche Restnachweise V1

Stand: 27.10.2026. Dokumentstatus: PLANUNGSVERTRAG. **Kein Code und kein Test wird durch diese Datei als implementiert oder bestanden ausgegeben.** Maßgeblich ist der Gesamtphasenplan; P1 und P5 haben ihre bestehenden detaillierten Pläne.

## 1. Wahrheitstabelle: Planung ≠ Laufzeitnachweis

| Bereich | Fachliche Soll-Spezifikation | Verbindliche Hauptreferenz | Zur Laufzeit noch nachzuweisen |
|---|---|---|---|
| Gate 1 / P0 | Quellbaseline, Governance, Optionen und Restentscheidungen | GATE_1.md; P0_RESEARCH_UND_VERTRAEGE_V1.md | Pine-Originale, Demo-Capabilities, Source-Inventar, konkrete Architecture-Spikes |
| P1 | Bitget Public SBE V5, echte Einzeltrades, L0/L1/L2, 1s/MTF, 24h+7d | PHASENPLAN_P1_V1.md; P1_BAR_SCHEMA_AND_TRADE_ORDER_V1.md | Rust, V5-Decoder, T01–T27, 7 Tage; **V5 60s-Probe 8/62 Parserfehler** |
| P2 | Qualifizierter Historie→Live-Warm-up/Handover ohne doppelte Signale | P2_P3_P4_DETAILVERTRAG_V1.md | Datenlücken-/Restart- und Cross-Segment-Test |
| P3 | Typisierter Graph, Bool3, Indikatoren, Pine-Goldens, immutable Strategie | P2_P3_P4_DETAILVERTRAG_V1.md | Pine-Original und ausführende Rust-Nodes/Golden-Vectors |
| P4 | Replay/Backtest im selben Kern, Kosten-/Fill-Tiers und getrennte OOS-Qualifikation | P2_P3_P4_DETAILVERTRAG_V1.md | Market-/Maker-/L2-Parität und echte Kosten/Slippage |
| P5 | Bitget UTA V3; One-way/Isolated als V1-Soll; S0–S7; Journal, Stops und Recovery | PHASENPLAN_P5_V1.md; P5_ORDER_STATE_UND_CRASH_MATRIX_V1.md | 28 A-Tests; DN-P5-ALL-001; private Demo-Konto-/Order-Gates |
| Wallet | Private WS plus REST-Kontoabgleich; Equity/Available/Positionen/Stops | WALLET_ACCOUNT_SYNC_V1.md | echte Demo-Readbacks; Wallet-Publisher/UI |
| P6 | Ein Bot E2E Paper und autorisierte Demo; viele Entry/Exit/Rearm-Zyklen | P6_P7_P8_QUALIFIKATION_V1.md | E2E-Runner, authentische Demo-Evidence |
| P7 | Fünf Graphite-Reiter, funktionaler React-Flow-Node-Editor, eine Gateway-Verbindung | P6_P7_P8_QUALIFIKATION_V1.md; DN_PROTO_CONTRACT_V1.md | React/Protobuf-Implementierung, Feld-/Pixeldiff-/Accessibility-Gates |
| P8 | Fault-/Security-/Tail-Latency-/Soak-Abnahme; Kosten-OOS getrennt | P6_P7_P8_QUALIFIKATION_V1.md | alle echten Negativtests und messungsbasierten Grenzwerte |
| P9 | ausdrücklich autorisierter Echtgeldpilot erst nach positiven Gates | P9_P10_FREIGABE_UND_FLEET_V1.md | tatsächliche Eignung, OOS-Kosten, Account-Readback und Pilot-Evidenz |
| P10 | Fleet mit getrennten Symbolen/Risikobudgets und einem Account-Owner | P9_P10_FREIGABE_UND_FLEET_V1.md | parallele Bots, globale Limits, Korrelations-/Owner-Tests |
| Betrieb | systemd/Linux, Keys, Backups, unabhängiger Alarm, Update/Recovery | BETRIEBSRUNBOOK_V1.md | Zielhost, SLO, Rotation, Restore, Watchdog- und Incident-Drills |
| Domain/Wire | DarkNode eigene Fachprotos/Topics und kontinuierlicher Feldabgleich | DN_PROTO_CONTRACT_V1.md; P0_RESEARCH_UND_VERTRAEGE_V1.md | CI, Codegen, echte Publisher-/UI-/Gateway-Goldens |

**Gesamturteil:** P1/P5/Wallet/Protobuf sind bereits spezifiziert, nicht ausgeführt. Die zusätzlichen Pläne schließen die größten *fachlichen Dokumentationslücken*; sie schließen **nicht** die Gate-1-, Account-, Code- und Evidenzlücken.

## 2. Noch nicht allein durch Planung entscheidbar

**Echte externe Evidenz:** Authentifizierte Bitget-UTA-Capabilities des konkret vorgesehenen Unterkontos; One-way/Isolated-, Leverage-, Reduce-only- und Schutzorder-Readbacks; reale Gebühren-/Funding-/Fill-Parameter; Private-WS-Abbrüche und Stop-Partial-Fills; komplette USDT-Futures-Symbolliste; kein Fake-PASS.

**Benutzervorgaben:** tatsächliches API-Unterkonto, spätere Demo-Order-Berechtigung, Konto-/Bot-Risikokapital, Verlust-/Drawdowngrenzen, erlaubte Strategie und gewünschter Risikohorizont, belegte Pine-Vergleichsdatei, künftiger Host und signierte LIVE-Freigabe. Ohne diese Werte keine willkürlichen Defaults und keine Orders.

**Engineering-/Testentscheidungen:** Hostregion/Latenz- und Stale-Schwellen, Bitget REST-vs-privater WS-Ordertransport, exakter TP/SL-Trigger, konkrete Cloud-/Backup-/Secret-Lösung, Golden-Toleranzen und statistische Mindestkriterien: vor jeweiligen Experimenten bzw. Abnahmeläufen versionieren, anstatt nach Messergebnissen zu optimieren.

## 3. Kein neues Doppel-System

- Ein DarkNode-Rust-Strategie-/Risk-/Order-Owner; keine zweite Engine in Browser/Python, keine BSC-/Project_0-Laufzeitregeln.
- Protobuf/Wire nur über bestehenden Gateway; eigenes Paket darkbot.darknode.v1, ProjectId 4 und Topics 70–89 bis zur tatsächlichen Governance-Implementierung nur *geplante* IDs.
- Keine zusätzlichen DarkNode-Tabs zu Overview / Builder / Backtest / Live / Wallet. Wallet/Live/Overview teilen denselben Account-State.
- Test-Evidenz bekommt Quelle, exakten Commit, Test-ID, Input-/Capture-Hash, Resultat/Reason, Qualitätsstatus und Time-/Hostprofil. Fehlende Tests sind NOT_RUN, fehlende Capability BLOCKED für die betroffene Test-Lane, echte Regression FAIL.
- Einzelne Dokumente sind normative Detailreferenzen. Der Masterplan beschreibt Zuständigkeit und Gates, kopiert nicht den Detailcode.

## 4. Beweiskette zum Abschluss

G1/P0 Design-Entscheide → P1/P5 *echte Daten/Capability* → M1-A echter Read-only-Durchstich → P2/P3/P4-Kern/Goldens → qualifizierter M1-B/Demo → P6-24/7-Zyklen/P7-UI → P8 Safety-/Latency-/OOS-Gates → **separate** autorisierte P9-LIVE-Entscheidung. Eine Planungsabnahme begründet keine Freigabe des nachfolgenden Schritts.

## 5. Planungsfreigabe vs. technische Freigabe

Der vollständige **Dokumentationsumfang** darf nach Review als SPECIFIED markiert werden, ohne einen einzigen Test als PASS zu kennzeichnen. Die Phase GATE 1 bleibt OPEN, solange ihre echten Quellen- und Experimentnachweise nicht vorliegen. Für P5 sind P5-A01–A28 und DN-P5-ALL-001 NOT_RUN; DN-PROTO-001–008 ebenfalls NOT_RUN. Kein SIGN/ARM/LIVE.