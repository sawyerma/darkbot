# Modulplan — zuschaltbare Pfade für den BSC-Bot (V1)

**Stand:** 2026-10-22 · **Bezug:** `sawyerma/darkbot` `main` @ `f2b5840` · **Status:** Planentwurf. Die Grundsätze in §1 hat der Nutzer am 2026-10-22 entschieden. Alles Weitere wird erst verbindlich, wenn der Nutzer den Änderungstext in §11 freigibt und er in die Roadmap übernommen ist.
**Verhältnis zu anderen Dokumenten:** Roadmap, Testplan und Masterplan bleiben in Kraft. `BACKRUN_ERWEITERUNG_ANTRAG_V4.md` bleibt die Detailbeschreibung der Backrun-Module (Zuordnung in §3).

**Kennzeichnung:** **[Nutzer]** = vom Nutzer entschieden · **[belegt]** = im Repository oder einer genannten Quelle nachgeprüft · **[Vorschlag]** = braucht Freigabe.

---

## 1. Grundsätze [Nutzer]

1. Was **kostenlos** ist und **früher Wissen** bringt, kommt **früher** in den Phasenplan.
2. Jeder Signal-, Filter-, Ausführungs- und Beobachtungsweg ist ein **Modul mit Schalter**, nicht fest eingebaut.
3. **Kostenverursachende Module** lassen sich ein- und ausschalten. Ob sie sich lohnen, wird **gemessen**, nicht angenommen.
4. Ziel ist die **Profitabilität**, nicht die maximale Zahl an N+1-Treffern.

Daraus folgen vier technische Regeln [Vorschlag]:

5. **Ein Kern:** ein Trading-Kern, ein Intent- und Nonce-Eigentümer, genau eine Entscheidung pro Launch, egal wie viele Module ihn melden.
6. **Kein Bremsen:** Ein optionales Modul darf den bestehenden Pfad nie verlangsamen. Der Hot-Path wartet auf kein optionales Modul.
7. **Abschalten ist immer sofort erlaubt, Einschalten nie automatisch.** Einschalten geht nur über ein neues, freigegebenes Profil.
8. **Kein Trade ist ein gültiges Ergebnis.** Ein zu teurer Pfad bedeutet: kein Kauf über diesen Pfad, keine Eskalation.

## 2. Was davon im Plan schon angelegt ist [belegt]

Der Modulplan baut nichts doppelt. Die Grundidee steht bereits in euren Verträgen:

| Grundsatz | Fundstelle |
|---|---|
| Mehrere Eingänge, „first verified actionable information wins", kein serielles Warten auf eine langsamere Quelle, globale Dublettenfilterung | Masterplan §12 „Ingress, C2 state and control-plane counterexamples" |
| Private/WSS-Feeds sind „optional separately measured adapters" und ersetzen geth nicht stillschweigend | Masterplan §12 |
| Ausführungswege als eingefrorene, versionierte `CapabilityRecord`s; `Enabled` vs. `Unqualified`; dokumentiert ≠ qualifiziert; `AdapterTable` mit `generation` je Sitzung | `trading/src/submission_policy/capability.rs`; Adapter heute: `LocalGeth`, `Club48Builder`, `BlockRazorBuilder`, `BlockRazorRpc` |
| Laufzeitberechtigung an qualifiziertes Profil und Build-/Config-Generation gebunden; unbekannter Zustand deaktiviert betroffene Einträge | Masterplan „Attempt, campaign and release verdicts" |
| Gleiche Kern-Pipeline in Replay / PAPER / SHADOW / LIVE | Masterplan Phase I |
| Kauf **und Verkauf** müssen ökonomisch abgedeckt sein, bevor Strategie-Ökonomie abgenommen wird | Masterplan Phase G |
| Vor jeder dauerhaften Annahme eines bezahlten Providers wird ein eigener Builder geprüft | Testplan G0.9 / G0.10 |
| Verboten: „remote quote/RPC lookup in the decision hot path"; vor dem Senden keine synchronen RPC/HTTP-Reads, kein Dateizugriff, kein Laden von Konfiguration; JSON intern verboten | Roadmap §20 „Architecture invariants"; Masterplan §10 „Entry hotpath and latency construction"; Roadmap-Binärgrenze 2026-10-16 |

**Neu am Modulplan:** ein einheitlicher Katalog mit Kostenklassen, drei Schalterzustände für alle Modularten (nicht nur Ausführung), das Vorziehen der kostenlosen Read-only-Module und ein festes Messverfahren für die Profitabilität.

## 3. Modulkatalog

**Kostenklassen:**

| Klasse | Bedeutung |
|---|---|
| **K0** | kostenlos, nur lesend (öffentlicher RPC mit Ratenlimit oder eigener Node; der Node ist Infrastruktur des Plans, keine Providergebühr) |
| **K1** | Zugang kostenlos, Kosten nur bei Ausführung (Gas) |
| **K2** | Kapitalbindung (z. B. 48 Soul Points), kein Abo, aber Kurs- und Opportunitätsrisiko |
| **K3** | Abo (Monatsgebühr) |
| **K4** | variable Zahlung je Trade an Dritte (Builder-Zahlung) |

**„Berührt G0.7?"** heißt: Ändert das Modul den eingefrorenen G0.7-Codepfad? Wenn ja, wären DRY und Requalifikation hinfällig, deshalb kommen solche Module erst nach G0.8.

### 3.1 Beobachtung (nur lesend, außerhalb des Kerns)

| ID | Zweck | Klasse | Berührt G0.7? | Frühester Start | Grundlage |
|---|---|---|---|---|---|
| **OBS-1** | Fremde Launches und N+1-Käufer messen (Four Classic) | K0 | nein | **sofort** nach Freigabe | BR0 Version 4, 15/15 Tests [belegt] |
| **OBS-2** | Drift-Wächter: Portal-/TokenManager-Upgrades früh erkennen (Flap v5.24 → v5.25 am 2026-10-17, → v5.26 am 2026-10-21) | K0 | nein | **sofort** | `readiness_job.py` und `g07_pre_live_readiness --current-rpc`, beide read-only ohne Key [belegt]; neu nur der periodische Aufruf |
| **OBS-3** | Builder-Beobachtung auffrischen | K0 | nein | **sofort**, Pflicht ohnehin ≤ 24 h vor dem Scharfschalten | Freigabe-Review G0.7, Schritt 5 [belegt] |
| **OBS-4** | Verkaufs-/Ausstiegsbeobachtung: Was passiert nach dem Launch? Verkäufe (`TokenSale`), Kursverlauf, Graduierung | K0 | nein | **sofort** | `TokenSale` laut archivierter TokenManager2-ABI mit gleichem Layout wie `TokenPurchase`, Topic `0x0a5575b3…ae19` selbst berechnet [belegt]; Werkzeug neu [Vorschlag] |
| **OBS-5** | 48Club-Feed mitschneiden und mit geth vergleichen (BR1) | K2 | nein | nach OBS-1 und Kapitalentscheidung | Antrag V4, BR1 |
| **OBS-6** | WSS-Wettlauf: Welche kostenlose Quelle sieht welchen Launch wie früh vor dem Block? | K0 | nein | **sofort** | `wss_race.mjs`, Tests 6/6, Anleitung `WSS_MESSUNG_ANLEITUNG.md` |

### 3.2 Signal (Eingänge in den Kern)

| ID | Zweck | Klasse | Standard | Phase | Grundlage |
|---|---|---|---|---|---|
| **SIG-1** | Eigener geth / Fast Lane | K0 | **an** (Basis) | C1 → F | bestehend |
| **SIG-2** | 48Club-Auction-Feed als zusätzliche Quelle | K2 | **aus** | F/I, wenn OBS-5 Nutzen zeigt | Antrag V4, BR2 |
| **SIG-3** | Bezahlte Mempool-Streams | K3 | **aus, nicht geplant** | nur wenn G0.9/G0.10 und Messung es begründen | Testplan G0.9/G0.10 |
| **SIG-4** | Kostenlose öffentliche WSS-Quellen (z. B. PublicNode, dRPC) als zusätzlicher paralleler Eingang | K0 | **SCHATTEN** | Messung **sofort** (OBS-6), LIVE nach Zahlen | Roadmap §20 verbietet öffentliches WSS nur als *alleiniges* primäres Signal [belegt]; Ratenlimits beachten (Ledger: QuickNode-Abbruch nach 15 Anfragen/s) |

### 3.3 Filter (im Kern, alle K0)

Jeder Filter ist eine eigene, schaltbare Regel. Der Hot-Path darf nur lokal Vorhandenes nutzen: Launch-Payload, vorab berechnete Listen, eingefrorene Konstanten.

| ID | Zweck | Datenquelle | Phase | Offene Frage |
|---|---|---|---|---|
| **FLT-1** | Venue- und Modus-Filter: V1, OpenFour, TaxToken, AntiSniper, X Mode, Quote-Asset → nur qualifizierte Varianten | Launch-Payload; Modi bisher nur über State-Read erkennbar | E/G | Ob der Modus **ohne** RPC aus dem `createToken`-Payload ableitbar ist, ist offen (G0.1 nennt diese Dimensionen). Nicht ableitbar → Filter lehnt ab |
| **FLT-2** | Ökonomie-Tor: erwarteter Nettowert ≥ Schwelle | Economics-Gate-Vertrag | G | Kostenmodell je Modul (§5) |
| **FLT-3** | Gebots-Obergrenze: Builder-Zahlung über Limit → kein Trade | Profil | G | Limit aus OBS-1/OBS-5-Daten |
| **FLT-4** | Creator- und Wallet-Listen (Sperr- und Erlaubnisliste) | vorab aus OBS-1/OBS-4 berechnet, lokal geladen | H | Kriterien aus Daten, nicht geraten |
| **FLT-5** | Ausstiegsfähigkeit: kein LIVE-Kauf ohne qualifizierten Verkaufsweg | Masterplan Phase G | G | Verkaufsweg je Variante |

### 3.4 Ausführung (bestehende `CapabilityRecord`s plus eine neue Art)

| ID | Adapter | Klasse | Status heute | Grundlage |
|---|---|---|---|---|
| **EXE-1** | `LocalGeth` | K1 | Record vorhanden, nicht qualifiziert | `capability.rs` [belegt] |
| **EXE-2** | `Club48Builder` | K1 + K4 | Record vorhanden, nicht qualifiziert | dto. |
| **EXE-3** | `BlockRazorBuilder` | K1 + K4 | Record vorhanden, nicht qualifiziert | dto. |
| **EXE-4** | `BlockRazorRpc` | K1 + K4 | Record vorhanden, nicht qualifiziert | dto. |
| **EXE-5** | 48Club-Backrun (`backrunTarget`) | K1 + K4 | **neu**, startet als `Unqualified` | Antrag V4, BR2/BR3 |

### 3.5 Steuerung (Kern)

| ID | Zweck | Phase |
|---|---|---|
| **CTL-1** | **Modulprofil:** binärer, versionierter Record (DBREC, kein JSON) mit Zustand jedes Moduls. Er wird beim Sitzungsstart geladen, ist hash-gebunden und im Hot-Path unveränderlich. Er verallgemeinert die bestehende `AdapterTable`/`generation` auf Signale und Filter | erster Punkt nach G0.8 |
| **CTL-2** | **Abschalter:** schaltet ein Modul sofort ab, ohne Neustart, mit Journaleintrag. Nie automatisch einschalten. Umsetzung als atomares Flag im Speicher, das ein Steuer-Thread setzt; der Hot-Path liest nur dieses Flag, denn Dateizugriff und Konfigurationsladen vor dem Senden sind verboten (Masterplan §10) | mit CTL-1 |
| **CTL-3** | **Budget je Modul** mit automatischer Abschaltung (§5.4) | mit CTL-1 |
| **CTL-4** | **Herkunft je Entscheidung:** welches Modul hat zuerst gemeldet, welche Filter haben entschieden, welcher Ausführungsweg, welche Kosten | mit CTL-1 |

## 4. Schalter: Zustände und Regeln [Vorschlag]

| Zustand | Bedeutung |
|---|---|
| **AUS** | Modul läuft nicht. |
| **SCHATTEN** | Modul läuft und liefert an den Kern. Der Kern trifft eine Schattenentscheidung und protokolliert sie. Für diesen Pfad wird **nichts signiert und nichts gesendet**. |
| **LIVE** | Modul darf an echten Trades mitwirken. |

1. **LIVE** setzt voraus: Qualifikation des Moduls für genau diesen Scope ist PASS, der Nutzer hat den Profil-Hash freigegeben, das Budget ist gesetzt.
2. **Standard:** K0- und K1-Module starten im SCHATTEN, K2- bis K4-Module stehen auf **AUS**.
3. **Abschalten** (LIVE → SCHATTEN oder AUS) ist jederzeit sofort erlaubt. **Einschalten** geht nur über ein neues Profil, einen Neustart und eine Freigabe.
4. **Eine Entscheidung pro Launch:** Dublettenschlüssel ist die Launch-Identität (Venue, Token, Launch-Hash). Die erste verifizierte, handlungsfähige Meldung gewinnt (Masterplan §12). Weitere Meldungen ergänzen nur noch die Herkunft.
5. **Keine Wiederholung über einen anderen Pfad:** Ein möglicherweise schon gesendeter BUY wird nie über einen zweiten Pfad erneut versucht. Es gibt einen Nonce-Eigentümer.
6. **Unabhängigkeit, gemessen statt behauptet:** Jedes optionale Modul läuft in eigenem Thread mit eigener Queue. Abnahmetest: Latenz des SIG-1-Pfads (p99) auf der Linux-Zielmaschine mit allen optionalen Modulen im SCHATTEN gegenüber allen AUS. Die zulässige Abweichung ist die Streuung zwischen zwei AUS-Läufen.
7. **Profile sind Dateien, keine Laufzeitschalter im Hot-Path:** Der Hot-Path liest nur das geladene Profil, nie eine entfernte Konfiguration.

**Beispielprofile:**

| Profil | Inhalt |
|---|---|
| **P0 „Kostenlos"** | SIG-1, alle Filter, qualifizierte EXE-1…4. Das ist der Standard |
| **P1** | P0 + SIG-2 im SCHATTEN (nur sinnvoll mit 48 SP) |
| **P2** | P0 + SIG-2 und EXE-5 LIVE |

Jedes Profil ist ein Record mit eigenem Hash und eigener Freigabe.

## 5. Wie entschieden wird, welche Module sich lohnen [Vorschlag]

### 5.1 Drei Evidenzstufen, nie vermischt

| Stufe | Herkunft | Was sie zeigt | Kosten |
|---|---|---|---|
| **E1 exakt** | SCHATTEN und Replay | Abdeckung (welche Launches sieht welches Modul), Vorlauf (zuerst gesehen je Quelle), Entscheidungsunterschied (Kern mit und ohne Modul auf denselben Daten) | keine Trades |
| **E2 Schätzung** | SCHATTEN + OBS-4 | hypothetisches Ergebnis der unterschiedlichen Entscheidungen, aus späteren On-Chain-Kursen und Verkäufen. Immer als SCHÄTZUNG markiert, mit offengelegten Annahmen | keine Trades |
| **E3 real** | LIVE | realisierter Nettogewinn je Trade **einschließlich Verkauf** | echte Trades |

### 5.2 Filter kostenlos vergleichen

Weil der Kern deterministisch ist (Replay-Grundlage in C0 vorhanden, volle Abdeckung in Phase I), lassen sich Filterprofile **offline** auf denselben aufgezeichneten Signalen gegeneinander laufen lassen. Das kostet nichts. Ausführungswege lassen sich so nicht vergleichen, denn die Aufnahme in einen Block ist nur live messbar.

### 5.3 Live-Vergleich ohne Rosinenpicken

- Profile werden nach einer **vorab festgelegten Regel** zugeteilt, z. B. nach einem Bit des Launch-Hashes. Niemand wählt pro Trade aus.
- Beide Profile bekommen dasselbe Budget und denselben Scope.
- Stichprobengröße und Entscheidungsgrenze werden vor dem Start festgelegt (Wilson- bzw. Konfidenzgrenzen wie im Antrag V4, BR4).

### 5.4 Entscheidungsregel je kostenpflichtigem Modul (vor Start festlegen)

```
Modulbeitrag = Netto-Ergebnis(Profil mit Modul) − Netto-Ergebnis(Profil ohne Modul) − Fixkosten des Moduls im Zeitraum
```

- Das Modul bleibt **LIVE** nur, wenn die **untere** Konfidenzgrenze des Modulbeitrags > 0 ist.
- Sonst geht es zurück auf SCHATTEN oder AUS.
- **Automatische Abschaltung** (nie automatische Einschaltung), wenn das Modulbudget erreicht ist oder der laufende Beitrag über eine vorab festgelegte Fensterlänge negativ ist.

### 5.5 Kennzahlen je Modul und Profil

- gesehene Launches
- Vorlauf
- Entscheidungsunterschiede
- Trades
- N+1-Quote
- Fehlversuche nach Klasse
- Gas
- Builder-Zahlungen
- Tokengebühren
- realisierter Nettogewinn
- Kapitalbindung
- Kosten je Modul

Der **realisierte Nettogewinn** ist die Zielgröße. Die N+1-Quote ist nur eine Hilfsgröße.

## 6. Kostenregeln [Nutzer, als Regeln formuliert]

1. **Keine Pflicht-Abos.** Standardprofil ist P0. K3-Module sind nicht geplant und brauchen eine eigene Begründung nach G0.9/G0.10.
2. **Keine automatische Kapitalbindung.** 48 SP erst nach einer Entscheidung auf Grundlage von OBS-1 und OBS-4 (siehe §8).
3. **Kein Bieterkrieg.** Liegt die Builder-Zahlung über der Obergrenze, wird nicht gekauft.
4. **Kein LIVE-Kauf ohne qualifizierten Verkaufsweg.** N+1 allein ist kein Gewinn.
5. **Getrennte Erfolgsstatistik** je Modul und Profil.
6. **Kein Versprechen:** Ein positiver Erwartungswert sichert nicht jeden Trade. Das System ist darauf ausgelegt, schlechte Gelegenheiten auszulassen, nicht Gewinn zu garantieren.

## 7. Einordnung in den Phasenplan

```
JETZT (nur lesend, Kern unberührt)     OBS-1  OBS-2  OBS-3  OBS-4  OBS-6
        │  laufen parallel und liefern Daten
G0.7 → G0.8   unverändert (Kern eingefroren)
G0.9          nutzt OBS-1/OBS-3: Builder-Abhängigkeit klassifizieren
nach G0.8     CTL-1…4 als erster Punkt, vor dem C1.10-Freeze
C1 → C2/C3    SIG-1 als Basismodul
F             SIG-1-Abdeckung; OBS-5/SIG-2 im SCHATTEN, falls 48 SP entschieden
G             FLT-1/2/3/5; Kostenmodell je Modul
H             FLT-4; Offline-Filtervergleich per Replay (E1/E2)
I             alle Module im SCHATTEN; Profilvergleich E1/E2
J             LIVE mit P0; danach Rotation mit Zusatzmodulen, wenn E1/E2 Nutzen zeigen
KB / K        unverändert
```

**Warum OBS-1 bis OBS-4 jetzt laufen können, CTL-1 aber erst nach G0.8:** Die Beobachtungsmodule sind eigenständige Read-only-Werkzeuge außerhalb des Kerns. Sie ändern keinen Byte des eingefrorenen G0.7-Pfads. CTL-1 ändert den Kern. Vor G0.7 würde das DRY, Requalifikation und Readiness-Nachweise ungültig machen und G0.7 weiter verzögern. Nach G0.8 und vor dem C1.10-Freeze ist der günstigste Zeitpunkt, denn C2/C3 bauen dann schon darauf auf.

**Zuordnung zum Antrag V4:** BR0 = OBS-1 · BR1 = OBS-5 · BR2 = SIG-2 + EXE-5 · BR3 = Nachweispfad für EXE-5 · BR4 = Live-Vergleich nach §5.3 · BR5 = Einordnung.

## 8. Entscheidungspunkte für den Nutzer

| Wann | Entscheidung | Grundlage |
|---|---|---|
| jetzt | OBS-1 bis OBS-4 und OBS-6 als Read-only-Ausnahme von der Stop-Regel freigeben | dieser Plan, Antrag V4, WSS-Anleitung |
| nach dem ersten OBS-1/OBS-4-Bericht | 48 SP beschaffen (K2) oder nicht. Der Punktestand wird über 48 Tage gemittelt, deshalb kann sich der Feed-Zugang nach dem Kauf verzögern. Wie stark, hängt davon ab, wie 48Club Tage vor dem Einstieg zählt — vorher bei 48Club klären. Früh entscheiden, aber nicht vor Daten | Häufigkeit fremder N+1-Käufe, Ausstiegsdaten |
| nach G0.8 | CTL-1…4 umsetzen | G0.8-Verdikt |
| vor J | Freigabe des P0-Profil-Hashes | Phase-I-Ergebnisse |
| nach E1/E2 | Rotation mit Zusatzmodulen freigeben (eigenes Budget) | §5 |

## 9. Was dieser Plan nicht ändert

`T_buy − T0 < 1 ms` als Produktziel; G0.7/G0.8 und ihre Freigabe; Guard `UNARMED`; Deployment-Codehash-Prüfung; Verlustreserve; JSON-Verbot; Ownership-Regeln; Originaldokumente.

## 10. Offene Fragen

1. Ist der Four-Classic-Modus (AntiSniper, TaxToken, Quote) **ohne RPC** aus dem Launch-Payload ableitbar? Davon hängt ab, ob FLT-1 im Hot-Path filtern kann (G0.1).
2. Welcher Verkaufsweg ist je Variante qualifizierbar (FLT-5, Phase G)?
3. Gilt die 48-SP-Schwelle zum Zeitpunkt der Entscheidung noch, und wie zählt die 48-Tage-Mittelung Tage vor dem Einstieg? Vor jeder Kapitalbindung bei 48Club klären.
4. Toleranz des Unabhängigkeitstests (§4.6) auf der Linux-Zielmaschine.

## 11. Änderungstext für die Roadmap (zur Freigabe)

Nach Freigabe durch den Nutzer in `PHASENPLAN.md` einfügen:

> **§ Modulare Pfade (beschlossen am …):** Signal-, Filter-, Ausführungs- und Beobachtungswege sind Module mit den Zuständen AUS / SCHATTEN / LIVE gemäß `MODULPLAN_ZUSCHALTBARE_PFADE_V1.md` §3–§6. Kostenlose Module mit Erkenntnisgewinn werden vorgezogen. Kostenverursachende Module sind standardmäßig AUS, und ihr Nutzen wird nach §5 gemessen. Ein Kern, eine Entscheidung pro Launch, ein Nonce-Eigentümer. Abschalten ist immer sofort erlaubt, Einschalten nur über ein freigegebenes Profil.
>
> **Stop-Regel, weitere Ausnahme (beschlossen am …):** OBS-1 bis OBS-4 und OBS-6 (nur lesend, außerhalb des Kerns, kein Key, keine Signatur, kein Senden) dürfen vor G0.7/G0.8 laufen.
>
> **Phasentabelle:** CTL-1…4 als erster Punkt nach G0.8, vor dem C1.10-Freeze.
