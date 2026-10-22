# BSC-Plan — Revisionen

**Aktuelle Revision: v6 (2026-10-22).** Nächster produktiver Gate-Schritt bleibt G0.7; v6 ist eine zusätzliche spätere Ingress-Architektur-/Qualifikationsanforderung, keine Mainnet-LIVE-Freigabe.

Es gibt **einen** BSC-Plan. Er besteht aus drei Dateien in diesem Ordner: Roadmap (Phasenplan), Prüfliste (Gate-0-Testplan) und Bauplan (Masterplan). Jede datierte verbindliche Ergänzung in einer der drei Dateien ist eine neue Revision des ganzen Plans. Die Dateien behalten ihre Namen; die Revision ergibt sich aus der jüngsten verbindlichen Ergänzung.

**Regel ab jetzt:** Jede verbindliche Änderung an einer der drei Dateien bekommt hier eine neue Zeile und eine neue Nummer. Jede Messung und jeder Ledger-Eintrag nennt die Plan-Revision, unter der sie entstanden ist („Messung v5").

## Revisionen

| Rev. | Datum | Änderung | Fundstelle | Messungen dieser Revision (Ledger) |
|---|---|---|---|---|
| **v1** | 2026-09-27/28 | Gate-0-Prüfliste und G0.1-Evidence-Freeze; Ledger angelegt | `GATE0_TESTPLAN_SHORT.md` (Ursprungsfassung); `3_BEWEISE/GATE0_G01_EVIDENCE_FREEZE_2026-09-27.md`; `3_BEWEISE/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md` | 28.09.–02.10.: QuickNode-WSS 4/4 Flap-Launches vorher gesehen (29.09.); Testnet-BUY N+1 im selben Block (01.10.); G0.5 Transport p99 0,527 ms BlockRazor / 0,584 ms 48Club (02.10.) |
| **v2** | 2026-10-03/04 | Prüfliste: frühere Akquisition und Pflicht-Capture T0 + 15 s; Bauplan konsolidiert (Unit 5 Core eingefroren) | `GATE0_TESTPLAN_SHORT.md` ab Zeile 553 („Current task amendment — 2026-10-03"); `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` Zeile 3 („Revision: 2026-10-04") | 03.–10.10.: C1.x-Qualifikation, C1.9 Native-Latenz (08.10.), C1.10A-Checkpoint (10.10.) |
| **v3** | 2026-10-11/13 | G0.7 als Einzelversuch mit Spend-Freeze; Roadmap-Update G0.6a–G0.8 | `GATE0_TESTPLAN_SHORT.md` ab Zeile 383 („single-attempt spend freeze — 2026-10-11"); `BSC_DUAL_VENUE_ROADMAP.md` ab Zeile 70 („qualification update — binding 2026-10-13") | 11.10.: Final DRY PASS, 1-gwei-Profil |
| **v4** | 2026-10-16/17 | JSON intern verboten, binärer Hot-Path; Bauplan Phase-F-Speicher-/Offline-Analyse-Vertrag; Strukturreform R1–R13 | `BSC_DUAL_VENUE_ROADMAP.md` ab Zeile 43 („BINARY HOT PATH … 2026-10-16"); `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` ab Zeile 504 („binding Phase F extension (2026-10-17)"); `3_BEWEISE/EVIDENCE.md` | 16.–17.10.: Builder-Probe (Check 6), Builder-Refresh, Regression 479 PASS / 0 FAIL / 5 ignored |
| **v5** | 2026-10-20/21 | G0.7-Klarstellung: Code bis zur Freigabegrenze fertigstellen, dann wirtschaftliche Freigabe; Offline-Bugfix-Ausnahme; Portal-Drift v5.26.0 festgestellt | `BSC_DUAL_VENUE_ROADMAP.md` Zeile 10 (2026-10-20) und Zeile 6 (2026-10-21); `STATUS.md` Checkpoint 2026-10-21 | 20.10.: Requalifikation gegen Portal v5.25.0 PASS (`3_BEWEISE/G07_REQUALIFICATION_RESULT_20261020.md`); 21.10.: Readiness `CURRENT_DEPLOYMENT_DRIFT=STOP`; 22.10.: WSS-Messungen (lauf1: 116 Launches, 41,4 % vorher sichtbar; Flap 52,4 %, Four.meme 14,7 %) = **Messung v5**, Werkzeuge in `../../tools/br0/` |

| **v6** | **2026-10-22** | **Fünf unabhängig OFF/SHADOW/LIVE schaltbare BSC-Ingressquellen; FIRST VERIFIED ACTIONABLE WINS; unverändertes T0 und physisches T_buy; WSS-Requalifikation nur je qualifizierter Quelle** | `BSC_DUAL_VENUE_ROADMAP.md` §5a, `GATE0_TESTPLAN_SHORT.md` datierter WSS-Zusatz, `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` §12, `3_BEWEISE/WSS_INGRESS_BEOBACHTUNG_2026-10-22.md` | WSS-Tests vom 22.10. wurden unter **v5** ausgeführt, unter v6 nur neu eingeordnet; T0-Witness und realer Mainnet-G0.7-BUY weiterhin UNKNOWN/OPEN |

## Was unter v6 gilt

- **G0.7/G0.8 bleiben unverändert vorrangig**; v6 autorisiert keine C1/C2-LIVE-Integration, keine neue Echtgeldtransaktion und keine Zahlungen für 48Club.
- Die neue Quelle-Matrix wird nach dem bestehenden Stop-/Freigabevertrag qualifiziert.
- Messungen vom 22.10. bleiben historische **v5-Messungen**.

## Was unter v5 umgesetzt wird

- **G0.7** nach v3 (ein Versuch, Spend-Freeze) mit v5-Klarstellung (Code fertig vor Freigabe).
- Offen vor dem Scharfschalten: Umgang mit der Drift v5.25 → v5.26 (Requalifikation nach `4_G07_FREIGABE/G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md` als Vorlage), Builder-Refresh ≤ 24 h, V4-Vertrag als `.dbrec`, Freigabe des Approved-Hash.
- Anträge in `6_VORSCHLAEGE/` gehören zu **keiner** Revision, bis sie freigegeben und hier eingetragen sind.
