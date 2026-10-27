# DarkNode – P5 Order-State-Machine, Schutzdeckung und Crash-Matrix V1

Stand: 27.10.2026. **Spezifikation, nicht implementiert**; P5-A01–A28/DN-P5-ALL-001 weiterhin NOT_RUN. Autorität: PHASENPLAN_P5_V1.md (S0–S7 und V1 One-way + Isolated) sowie WALLET_ACCOUNT_SYNC_V1.md. Dieses Dokument ist die Soll-Logik, kein neuer Trading-Kern.

## 1. Zwei getrennte Achsen – niemals Zustände vermischen

**Account/Risk:** UNINITIALIZED → RECONCILING → HALTED (Default nach Neustart) → ACTIVE (nur nach vollständig bestandenem Readback/ausdrücklich erlaubter Aktivierung). REDUCING verbietet risk-erhöhende Orders; HALTED erlaubt nur Query/Cancel/Überwachung. EMERGENCY_RECOVERY ist ein **eigenes, enges und auditierbares** Risikoreduktionsverfahren, das niemals eine allgemeine HALTED-Umgehung ist.

**Order/Lifecycle:** DRAFT → VALIDATED → RESERVED → INTENT_DURABLE → SUBMIT_PENDING → ACKNOWLEDGED/WORKING → PARTIAL_FILLED → FILLED oder CANCEL_PENDING → CANCELLED. Zu *jedem* Übergang durch Timeout/Crash/Streamkonflikt ist UNKNOWN ein **sticky** konservativer Zustand, der nur durch exchange-autoritativen Order-/Fill-/Positions-/Stop-Abgleich aufgelöst wird; ACK≠Fill, CancelAck≠Cancelled, lokales SignalEnde≠Order verschwunden. Orderzustand und Position-/Stopzustand sind getrennte Dimensionen.

**StopCoverage:** ABSENT, REQUESTED, EXCHANGE_ACK_UNKNOWN, CONFIRMED_PARTIAL, CONFIRMED_FULL, STALE, MISMATCH/PROTECTION_UNKNOWN, CLOSING/RECONCILING. Ein gesendeter Preset-SL-Parameter ist **nicht** EXCHANGE_CONFIRMED. Schutz muss je tatsächlicher Position und ggf. gegliederter Partial-Fill-Menge den Exchange-Status/Trigger/Side/Lots erfüllen. Pricegap/Counterparty-/Börsenausfall bleiben ungedecktes Restrisiko auch bei CONFIRMED_FULL.

## 2. Nicht verhandelbare Invarianten

1. Maximal **ein** serialisierter Account-/Order-/Risk-Owner (Single Writer) pro UTA-Unterkonto; keine parallelen REST-/WS-Client-Verantwortlichen mit unabhängigem Inventory.
2. Vor erstem Send liegt eine *dauerhafte* unveränderliche Intent-ID, clientOid, Requesthash, Bot/GraphVersion, Konto/Instrument, Risk-Reservation und Journal-Cursor vor. Disk-full oder nicht bestätigter fsync ⇒ kein neuer Entry.
3. Alle ausstehenden und UNKNOWN Entries reservieren mögliche Exposure; Exchange-Position plus Working-Orders plus Unknown-Send-Risiko → harte S1-Limits, nie Margin-only.
4. Kein Closing/Cancel/Replacement ohne typgeprüfte Side/PosSide/reduceOnly/Qty und Rücklesung; im V1 One-way+Isolated. Kein Flip, kein Hedge, kein blindes Re-Send von alter clientOid nach Idempotenzfenster.
5. Private WS + REST mit opaker IDs/Typ/Times/Revisionsqualität; Out-of-Order-Reconcile statt neueste lokale Eventzeit als Brokerposition. Fremdorders, ADL, Liquidation und manuelle Bitget-App-Eingriffe erzeugen NO_ENTRY/RECONCILING.
6. Kein neuer Entry bei fehlendem/mengenabweichendem oder nicht verifiziertem Schutz, veraltetem P1-/Private-Feed, Market-/Instrument-Skalenkonflikt, Accountdrift, unqualifiziertem Hebel/Mode oder Verlustgrenze.
7. Stop-Loss-Erhöhung in Gewinnrichtung nur nach kontrolliertem Exchange-Modify und **neuer** Statusbestätigung; Schutzlücke bei Cancel/Replace muss explizit gemessen/abgewiesen werden. Keine Garantie für vollständigen Fill.

## 3. Entscheidungs-/Prioritätsmatrix

| Ereignis | Zwingendes Verhalten | Nicht erlaubt |
|---|---|---|
| Neuer Entry bei ACTIVE+SYNCED+qualifiziertem STOP-Mechanismus | Riskcheck inkl. Pending, Interest/Notional/Lot/Fee→fsync→ein Send→Order/Fill/Stop verfolgen | Graph/Gateway sendet direkt |
| gleichzeitig Exit+Entry oder Long+Short | erst qualifizierter Exit/HOLD, erst bestätigtes FLAT + Rearm neu enter | Sofortflip/Doppelorder |
| Teilfill | wahre Position/AvgPrice/Stopdeckung abgleichen und ggf. Schutz erhöhen; nicht auf FullFill warten, um Risiko zu erkennen | „Preset=ja“ ohne Exchange-Mengenprüfung |
| Antwort verloren nach Send | Intent UNKNOWN; Order/Fill/Position/Strategy-Orders über beide Streams/REST suchen | erneuter Blind-Send über REST oder WS |
| Cancel angefordert, Late Fill | Cancel-Pending mit weiter reservierter Menge, Stop-/Position update, erneuter REST-Readback | CancelAck als FLAT |
| Stop abgelehnt/gelöscht | PROTECTION_UNKNOWN, Entries sperren, Ersatz nur nach dedupliziertem Readback; ggf. qualifizierter Emergency-Close | unbounded Close-/Stop-Retry |
| externer Fill oder ADL | REST/WS Reconcile, eigene Stopmengen prüfen, UNKNOWN/NO_ENTRY | Position aus Bot-Fill-Journal erfinden |
| Browser/Gateway offline | Rust weiter, unabhängige Alarme/Stop-Audits, Readiness ehrlich | Browser als Safety-Owner |
| Neustart und offene Position | HALTED/RECONCILING, alle Orders/Stops/UID/Hebel/Mode readback | Auto ACTIVE aus altem Flag |
| Hostverlust ohne Journal | Exchange-only Recovery konservativ HALTED, verifizierten Exchange-SL belassen; nicht beweisbare Intent-IDs UNKNOWN | neues Order-Oid aus Vermutung generieren |

## 4. Crash-Schnittstellen, deterministisch zu injizieren

C00 vor Reservation, C01 nach Reservation vor Journalwrite, C02 nach Write vor fsync, C03 nach fsync vor Netzwerk-Submit, C04 während Netzwerk-Write, C05 nach Write vor ACK, C06 nach ACK vor privatem Fill-Event, C07 Teilfill vor Stopbestätigung, C08 Modify-Send vor Stop-Readback, C09 Cancel-Pending vor Late Fill, C10 Emergency-Close vor Readback, C11 Exchange-ADL/Fremdtrade ohne eigenen Order-Event, C12 Neustart ohne lokales Journal, C13 Backup-Restore mit älterem Snapshot, C14 Key-Revoke, C15 WS/REST-Failover nach unbekanntem Send, C16 Rate-Limit-/429-/Time-Drift.

**Jede** Injection protokolliert Vorzustand/Orders/Fills/Position/Stop, Exchange-/Local IDs, Risk-Reservation, zulässige Recovery-Queries, tatsächlichen Endzustand, Fehlerklasse und Alarm. Positive Assertions: kein Entry ohne Risk/Intent, kein doppelter Close, kein Phantom-FLAT, STOP_UNKNOWN sichtbar, kein Auto-LIVE, Position/Stop nur durch bestätigte Exchange-Daten qualifiziert. Eigene Tests dürfen Szenarien simulieren; Demo-Nachweis separat mit realen signierten Testrequests und separater Autorisierung.

## 5. Transport und Zeit

Die private WS-Orderplatzierung und REST werden mit identischer fachlicher Semantik auf Bitget-Demo verglichen. Vor dem Test keine Auswahl nach nomineller Geschwindigkeit; der funktional sichere Transport gewinnt, Latency p50/p95/p99/p99.9 unter Last ist sekundär. Send→ACK, Send→Fill und PartialFill→StopReadback getrennt; Event-Timestamps verschiedener Maschinen nicht als vergleichbare monotone Uhr behandeln. Requesttext/Protobuf/Journal ohne Key/Secret, Trace-ID und Original-OrderID via sicherer Redaction. Die konkrete Numeric-SLO-Festlegung erfolgt vor Live-Demo-/Hostqualifikation.

## 6. Abnahme und Schnittstelle zur bestehenden P5-Matrix

A08/A21/A22/A27: Timeout/Idempotenz/Late Fill/Crash. A17–A20/A28: Schutzdeckung/Gap. A11/A14/A16/A23: Exposure-/Mode-/Emergency. A24–A26: UI-unabhängiger Halt, Totmannschalter default OFF, Restart-/Drift. Diese Cases gehen in **denselben** DN-P5-ALL-001-Runner, keine parallele erfundene Testengine. P5-PLAN = SPECIFIED; alle funktionalen Assertions bis zur realen Implementierung NOT_RUN.