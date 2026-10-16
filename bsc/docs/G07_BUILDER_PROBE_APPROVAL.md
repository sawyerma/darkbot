# G0.7 Builder-Probe — Freigabevorlage (Pre-Arm-Checks 6 und 10)

Stand: 2026-10-16. **Probe einmal gesendet (18:34Z) mit deiner Freigabe; Ergebnis in §9.** Vorher: Der Probe ist vorbereitet, in drei Runden adversariell geprüft und getestet, aber nur als Trockenlauf gelaufen: keine Verbindung, kein Secret gelesen, nichts geschrieben. Der frühere Plan `15ee10f9…` (Commit `9ab5fe6`) ist überholt und wird nicht mehr akzeptiert. Gesendet wird erst, wenn du genau diesen Plan freigibst:

**Plan-SHA-256: `77957205fd844dea51d4eb97a4b12c345978952fafc91c95cefec75d28f9abf5`**

Guard bleibt `UNARMED`, der aktive v2-Vertrag ist unverändert (`026b534f…`). Der Probe berührt weder die G0.7-Wallets noch ihre Schlüssel noch die signierten DRY-Bytes noch die G0.7-Sperre.

## 1. Was der Probe zeigen kann — und was nicht

Zum Zeitpunkt der **Einreichung**, getrennt für 48Club und BlockRazor:
- ob die Methode `eth_sendBundle` und das exakte G0.7-Anfrageformat geparst werden (Felder, Integer-`maxBlockNumber`, 48Club `maxTimestamp`, BlockRazor `Authorization`);
- ob 48Club `noMerge` und `positionFirst` als typisierte Felder liest (Typ-Kontrollen);
- ob abgelaufene Ablaufwerte (`maxBlockNumber`, `maxTimestamp`) schon bei der Einreichung abgelehnt werden;
- ob es vor der Simulation eine Preisschranke gibt (Preis-Kontrollen mit 1 wei).

**Nicht zeigen kann er**, was erst beim Blockbau entschieden wird oder ausführbare Transaktionen braucht:
- ob 0,05 gwei für eine Aufnahme ausreicht; Durchschnittsprüfungen nach der Simulation bleiben ungeprüft;
- ob `noMerge`/`positionFirst` wirken;
- ob 48Club `maxBlockNumber` neben `maxTimestamp` durchsetzt;
- ob ein Bundle eingeschlossen würde.

Diese Punkte bleiben UNQUALIFIED und brauchen deine ausdrückliche Annahme.

## 2. Warum die Bundles nicht ausführbar sind

- **Wegwerf-Konten:** Jeder Fall hat **seine eigenen zwei Wegwerf-Konten**, abgeleitet aus öffentlichen Seeds, niemals die G0.7-Wallets. Vor allem anderen prüft der Live-Lauf nur lesend, dass alle 22 Konten Guthaben 0 und Pending-Nonce 0 haben, sonst STOP.
- **Jede Transaktion:**
  - Nonce 2^40, unerreichbar: Ein Konto zählt von 0 in Einerschritten. Selbst wenn jemand die Konten finanzierte, wäre nichts ausführbar.
  - Wert 0, Empfänger = Absender, keine Daten, Gaslimit 21 000, Chain 56.
  - Gaspreis 50 000 000 wei wie G0.7, in den zwei Preis-Kontrollen 1 wei.
- **Kein Fall teilt sich Absender, Transaktion oder Bundle mit einem anderen.** Weder eine Duplikat- noch eine Absender-Regel eines Builders verknüpft zwei Fälle (Test).
- **Signer:** signiert vom G0.7-Signer (`transactions.rs`, byte-gleich zu `565d811`). Ein Test rechnet die Bytes aus den deklarierten Parametern nach.
- **Weiterleitung:** Leitet ein Builder die Transaktionen ins Netz weiter, bleiben sie wirkungslos. Sie sind dann öffentlich, enthalten aber nichts Geheimes.

## 3. Wie der Probe sendet, und was der Hash bindet

- **Derselbe Code wie G0.7:** Er bindet die G0.7-Module selbst ein, nicht kopiert: `http_request`, `connect_tls`, `transport_alive`, den Head-Poller, `bundle_window` und die Body-Bausteine `club48_bundle_body`/`blockrazor_bundle_body`. Diese nutzt G0.7 ebenfalls, ein Golden-Test pinnt das Wire-Format.
- **G0.7-Form:** Die Fälle 1 und 9 sind bis auf die Transaktionen und die daraus abgeleitete `id` byte-gleich zu dem, was G0.7 bei gleichem Head senden würde (Test).
- **Eine Request-Funktion:** Jeder Request entsteht in einer einzigen Funktion (`case_request`: Transaktionen, Ablaufwerte, Body, Host, Authorization-Wahl, Framing). Live-Lauf und durchgerechnete Beispiele nutzen dieselbe.
- **Ablauf pro Fall**, wie bei G0.7:
  1. Builder-Verbindung aufbauen.
  2. Auf einen neuen Block warten (bis zu 3 neue Heads, bis einer die G0.7-Frische-Schranken besteht, sonst wird der Fall ausgelassen und nicht gesendet).
  3. Verbindung auf Lebendigkeit prüfen.
  4. Senden, Antwort lesen, keine Wiederholung.

  Mindestens 2 s Abstand; 11 Requests, je höchstens einmal, in fester Reihenfolge.
- **Reihenfolge der Schritte:**
  - erst die Lese-Prüfungen (Konten leer, Head-Poller und ein Head lesend);
  - dann die Einmal-Sperre `bsc/.local/g07/builder-probe-77957205fd84/`;
  - dann die Requests.

  Derselbe Plan läuft nie zweimal.
- **Evidence:** liegt privat in diesem Verzeichnis (`probe.evidence.json`, 0600, atomar ersetzt).
  - Vor dem ersten Request steht dort ein Bericht ohne Fälle; scheitert das, wird nichts gesendet und die Sperre freigegeben.
  - Danach wird sie nach **jedem** Fall fortgeschrieben (Status `IN_PROGRESS`, am Ende `COMPLETE`).
  - **Scheitert ein Schreiben nach einem Fall, stoppt der Probe sofort:** Kein weiterer Fall wird gesendet, der Bericht geht als `STOPPED_EVIDENCE_WRITE_FAILED` auf stdout, Exit 3. Die Sperre bleibt bestehen, weil schon ein Request hinausgegangen sein kann.

  Pro Fall stehen dort:
  - Head, Head-Hash und Millisekunden-Phase;
  - die gesendeten Werte und der ganze Body mit seiner Länge;
  - die Zeitpunkte von Verbindung, Schreiben und Antwort;
  - die rohe Antwort und die Klasse.

  Die Länge des ganzen Requests wird bewusst nicht gespeichert, weil sie die Token-Länge verriete. Der Token steht nur im Header, nie in Evidence oder Ausgabe.
- **Was der Hash bindet:**
  - den Plantext: alle 22 Konten, alle rohen Transaktionen, Hosts, Bodies mit den Head-Platzhaltern, Reihenfolge, Anzahl, Abstand und die Frische-Regeln aus den echten Konstanten;
  - je Fall die exakten Request-Bytes für **einen** festen Test-Head (Phase 200 ms, Alter 100 ms), gebaut von der Request-Funktion des Live-Laufs;
  - den **SHA-256 jeder ausgeführten Quelldatei**: `g07_builder_probe/{main.rs,plan.rs}`, die eingebundenen G0.7-Module `shared.rs`, `builder_submission.rs`, `chain_reads.rs`, `inputs.rs`, `transactions.rs` und das Bibliotheksmodul `acceptance_guard.rs` (liefert die Hash-Funktion);
  - den SHA-256 von **`Cargo.lock`**: Versionen und Prüfsummen jeder externen Bibliothek, die `cargo --locked` beim Bauen durchsetzt;
  - den SHA-256 von **`rust-toolchain.toml`** (Compiler 1.93.1).

  Jede Änderung an Probe, G0.7-Modulen, Abhängigkeiten oder Toolchain-Angabe ändert damit den Hash. Deine Freigabe gilt genau diesem Code und Abhängigkeitsstand.
- **Was der Hash nicht bindet:**
  - den tatsächlich laufenden Compiler: Ohne rustup setzt niemand `rust-toolchain.toml` durch. Vor dem Bauen deshalb `rustc --version` prüfen; hier ist es Homebrew-rustc 1.93.1.
  - weitere Workspace-Crates, die mitgelinkt, aber vom Probe nicht ausgeführt werden (z. B. `darkbot-bsc-trading`).
  - Fenster- und Ablaufcode jenseits des einen Test-Heads; weitere Phasen decken die Unit-Tests ab.
  - Sätze im Plantext wie „never recorded“ oder „else STOP“ beschreiben den Code; ihre Wahrheit sichern Review und Tests, der Quell-Hash verhindert, dass sie sich unbemerkt ändert.

## 4. Die elf Fälle und wie ihre Antworten zu lesen sind

**Antwortklassen** (Grobsortierung; maßgeblich ist die roh gespeicherte Antwort): `ACCEPTED`, `METHOD_REJECTED`, `FORMAT_REJECTED`, `EXECUTION_REJECTED`, `PRICE_REJECTED`, `EXPIRY_REJECTED`, `BUNDLE_REJECTED_UNSPECIFIED`, `OTHER_REJECTED`, `HTTP_ERROR`, `UNPARSEABLE`, `TRANSPORT_ERROR` (nach Schreibbeginn), `TRANSPORT_ERROR_NOT_SENT`, `SKIPPED_NO_FRESH_HEAD`.

**Lese-Regeln vorab:**
1. **Nur auswertbare Antworten zählen.** Die Schlüsse gelten nur für 2xx-Antworten mit lesbarem JSON-RPC-Inhalt. `HTTP_ERROR`, `TRANSPORT_ERROR*`, `UNPARSEABLE` und `SKIPPED` lassen die Frage offen, im Fall selbst oder in seinem Vergleichsfall. Steht bei `HTTP_ERROR` ein JSON-RPC-Fehler im gespeicherten Body, wird nach dem Body gelesen.
2. **Vergleichsfall muss geparst sein.** Die Fälle 2–8 zählen nur, wenn Fall 1 `ACCEPTED` oder `EXECUTION_REJECTED` ist; die Fälle 10–11 nur, wenn Fall 9 so ist. Sonst sind sie nicht auswertbar.
3. **Erwartete Antwort auf die leeren Konten:** „insufficient funds … balance 0“ oder „nonce too high …“ ist `EXECUTION_REJECTED`, kein Preis- und kein Ablaufsignal.
4. **`-38000`:** ohne erkannten Ausführungs-, Preis- oder Ablaufbegriff ist es `BUNDLE_REJECTED_UNSPECIFIED`.

| Fall | Was er zeigt |
|---|---|
| 1 `48club-g07-shape` | `ACCEPTED`/`EXECUTION_REJECTED` → Methode und Format werden geparst (Check 6 Format für 48Club bestanden). Über Ablauf und Preis sagt das nichts. `METHOD_REJECTED`/`FORMAT_REJECTED` → ADAPT des G0.7-Bodys. `PRICE_REJECTED` → ADAPT Gaspreis. `EXPIRY_REJECTED` → erst Ankunftszeit prüfen (unten), sonst ADAPT Ablaufregel. |
| 2 `48club-no-maxtimestamp` | Ablehnung mit Bezug auf `maxTimestamp` → Pflichtfeld, G0.7 sendet es zu Recht. Sonst: optional oder nicht bei Einreichung geprüft. |
| 3 `48club-maxtimestamp-minus-one` | Nur mit der gespeicherten **Millisekunden-Phase** lesbar; nur etwa jeder zehnte Head ist aussagekräftig. Bei Phase < 100 ms ist `MAXTS-1` gleich der Head-Sekunde; dann zeigt `EXPIRY_REJECTED`, dass die +1 nötig ist. Bei Phase ≥ 100 ms liegt `MAXTS-1` nach der Head-Sekunde: Eine Annahme beweist nichts. Ein `EXPIRY_REJECTED` deutet dann auf eine Prüfung gegen die aktuelle Zeit (mit `request_written_unix_ms` vergleichen). Die +1 bleibt so oder so die robuste Wahl. |
| 4 `48club-past-block` | `EXPIRY_REJECTED` → 48Club prüft `maxBlockNumber` bei Einreichung (ein Hinweis, dass es das Feld auswertet; kein Beweis für den Blockbau). Andere Antworten → bei Einreichung nicht geprüft, oder erst nach der Simulation. |
| 5 `48club-past-timestamp` | `EXPIRY_REJECTED` → 48Club prüft `maxTimestamp` bei Einreichung. Sonst: nicht bei Einreichung geprüft, oder erst nach der Simulation. |
| 6 `48club-underpriced-control` | `PRICE_REJECTED` hier **und** Fall 1 `ACCEPTED`/`EXECUTION_REJECTED` → 0,05 gwei besteht eine Preisschranke **vor** der Simulation. Eine Durchschnittsprüfung danach, so wie beide Builder sie dokumentieren, bleibt ungeprüft. Gleiche Antwort wie Fall 1 → nicht feststellbar (keine Prüfung bei Einreichung, oder sie liegt hinter der Simulation); der Preis bleibt UNQUALIFIED (Check 7, dokumentbasiert). |
| 7 `48club-nomerge-type-control` | `FORMAT_REJECTED`, die rohe Meldung nennt `noMerge`, und Fall 1 ist geparst → 48Club liest das Feld typisiert, erkennt es also (Check 10: Feld erkannt, Wirkung weiter UNQUALIFIED). `FORMAT_REJECTED` mit anderem Feld → nicht auswertbar. Annahme → Feld ignoriert oder umgewandelt, ohne Aussage. |
| 8 `48club-positionfirst-type-control` | Wie Fall 7, für `positionFirst`. |
| 9 `blockrazor-g07-shape` | `ACCEPTED`/`EXECUTION_REJECTED` → Methode, Format und Token angenommen (Check 6 Format für BlockRazor bestanden). `HTTP_ERROR` 401/403 → Token-Problem. `METHOD_REJECTED`/`FORMAT_REJECTED` → ADAPT des Bodys. `PRICE_REJECTED` → ADAPT Gaspreis. `EXPIRY_REJECTED` → Ankunftszeit prüfen, sonst ADAPT. |
| 10 `blockrazor-past-block` | `EXPIRY_REJECTED` → bestätigt die Doku (-38000 unter dem aktuellen Block). |
| 11 `blockrazor-underpriced-control` | Wie Fall 6, für BlockRazor gegen Fall 9. |

**Grenzfälle:**
- **Simulation vor Parameterprüfung:** Prüft ein Builder die Transaktionen vor Ablauf und Preis, antworten die Fälle 2–6, 10 und 11 vermutlich ebenfalls `EXECUTION_REJECTED`. Sie sind dann ohne Aussage, und auch Fall 1/9 sagt dann nichts über Ablauf oder Preis.
- **Ankunftszeit:** Ein `EXPIRY_REJECTED` in Fall 1, 3 oder 9 zählt erst, wenn es auch zur Ankunftszeit von G0.7 eingetreten wäre. Der Probe baut die Verbindung wie G0.7 vor dem Head-Read auf. Trotzdem vergleichen:
  - `request_written_unix_ms` mit `send_head_milli_timestamp`+900;
  - `floor(request_written_unix_ms/1000)` mit `max_timestamp`.

  Erklärt allein eine verspätete Ankunft die Ablehnung, ist sie kein ADAPT für G0.7.
- **Ausgelassener Fall:** Ein `SKIPPED` oder `TRANSPORT_ERROR_NOT_SENT` in Fall 1 oder 9 lässt Check 6 für diesen Builder offen. Eine Wiederholung nur der fehlenden Fälle braucht einen geänderten Plan mit neuem Hash.

## 5. Risiken des Probe-Laufs

- **BlockRazor:** Drei nicht ausführbare Bundles laufen unter deinem Token und könnten auf Rate-Limit oder Reputation zählen.
- **48Club:** acht Requests ohne Authentifizierung von deiner IP.
- **Transaktionen und Konten:** Die Wegwerf-Transaktionen können öffentlich werden (wirkungslos, siehe §2). Die Wegwerf-Schlüssel sind öffentlich ableitbar; das ist beabsichtigt, sie schützen nichts.
- **Kein Geld, kein G0.7-Zustand:** kein Geld, keine G0.7-Bytes, keine G0.7-Sperre (eigene Sperre `builder-probe-…`).
- **Abbruch:**
  - Scheitert eine Lese-Prüfung (RPC, Konten, Head), bricht der Lauf **vor** der Sperre ab, und der Plan bleibt nutzbar.
  - Scheitert das Anlegen der Evidence-Datei oder ihr erstes Schreiben, gibt der Probe die Sperre wieder frei (noch nichts gesendet).
  - Scheitert ein Schreiben nach einem gesendeten Fall, stoppt der Probe ohne weitere Requests und behält die Sperre.
  - Jeder Abbruch nach dem ersten Request verbraucht den Plan. Ein neuer Lauf braucht dann deine Entscheidung: Sperrverzeichnis umbenennen, oder ein geänderter Plan.

## 6. Ausführen (nur nach deiner Freigabe)

Aus dem Repo-Root, mit synchronisierter Uhr (NTP, z. B. `sntp time.apple.com`).

Bauen (im Workspace; vorher `rustc --version` = 1.93.1 prüfen):

```bash
(cd bsc/engine && cargo build --locked --offline -p darkbot-bsc-execution --features g07-mainnet --bin g07_builder_probe)
```

Trockenlauf (zeigt genau den Plan unten und seinen Hash, sendet nichts):

```bash
bsc/engine/target/debug/g07_builder_probe
```

Live-Lauf: Nur die zwei benötigten Werte gelangen in den Probe, die Wallet-Schlüssel aus `bsc/secrets.env` nie. Die Datei wird nur in einer Wegwerf-Subshell geladen; fehlt ein Wert, bricht die Subshell ab; der Probe startet mit `env -i` und genau diesen Variablen:

```bash
( set -a; . bsc/secrets.env; set +a; : "${BSC_MAINNET_RPC_HTTPS:?missing}" "${DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION:?missing}"; exec env -i PATH="$PATH" BSC_MAINNET_RPC_HTTPS="$BSC_MAINNET_RPC_HTTPS" DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION="$DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION" DARKBOT_G07_PROBE_SEND=YES DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256=77957205fd844dea51d4eb97a4b12c345978952fafc91c95cefec75d28f9abf5 bsc/engine/target/debug/g07_builder_probe )
```

- Ohne `YES` oder mit einem anderen Hash wird nichts gesendet (getestet). Der Trockenlauf gibt absichtlich keinen fertigen Sendebefehl aus.
- Hinweis: RPC-URL und Token stehen kurz in den Prozess-Argumenten von `env`; auf einem Einzelplatz-Mac unkritisch.

## 7. Danach

- Ergebnis aus `bsc/.local/g07/builder-probe-77957205fd84/probe.evidence.json` nach §4 lesen.
- Pre-Arm-Checks 6/10 im v2-Entwurf entsprechend setzen: PASS soweit prüfbar, ADAPT oder UNQUALIFIED mit deiner Annahme. Das ändert den Entwurfs-Hash.
- **Preis:** Der Preis-Teil von Check 6 bleibt UNQUALIFIED. Eine Trennung in Fall 6/11 zeigt nur eine Schranke vor der Simulation. `PRICE_REJECTED` in Fall 1 oder 9 → ADAPT.
- Bei einem `ADAPT` aus Fall 1 oder 9 wird G0.7 nicht scharfgeschaltet, bevor die Anpassung samt DRY-Folgen geklärt ist.

## 8. Code und Tests (Stand 2026-10-16, offline)

- **Probe:** `bsc/engine/crates/execution/src/bin/g07_builder_probe/{main.rs,plan.rs}`, `tests/g07_builder_probe.rs`.
  - Commit `9ab5fe6` enthält Plan `15ee10f9…`.
  - Diese Revision ist **nicht committet**: Evidence-Stopp sowie Bindung von `Cargo.lock`, Toolchain und Guard-Modul.
- **Probe-Tests:**
  - Unit 26/26: eigene Konten und nicht ausführbare Transaktionen je Fall; G0.7-Form gleich den G0.7-Bodies; jede Variante ändert nur das Genannte; Plan deterministisch, mit Quell-Hashes und Beispielen; Klassifikation mit echten geth-Meldungen.
    - Neu: Ein gescheitertes Schreiben stoppt vor dem nächsten Fall (für jede Position geprüft), und geschrieben wird direkt nach jedem Send.
  - Integration 5/5. Neu: kein ignoriertes Evidence-Schreiben, ein Bericht vor dem ersten Send, keine Sperrfreigabe nach Beginn der Sendeschleife, Exit 3 beim Stopp. Außerdem: Trockenlauf ohne Eingaben, deterministisch, mit **gepinntem Plan-Hash** und ohne Sendebefehl; Live ohne oder mit falschem Hash verweigert; Freigabe vor jedem Lesen, Sperren und Verbinden; Requests nur aus der gemeinsamen Funktion, Lebendigkeitsprüfung vor dem Schreiben; kein Zugriff auf G0.7-Schlüssel.
- **G0.7:**
  - Binary 20/20; neu: Ein nicht stagebarer Evidence-Name hinterlässt keinen Platzhalter. Das ist eine kleine Korrektur in `inputs.rs`, die auch G0.7 zugutekommt.
  - `g07_no_send` 10/10, `acceptance_guard` 6/6, Lib 21/21, G0.8-Kompatibilität 2/2.
- **Signierte Bytes:** `transactions.rs` byte-gleich zu `565d811`, G0.7-Signierpfad unverändert.
- **Werkzeuge:** `cargo fmt`, `check` und `clippy` sauber (eine vorbestehende Warnung).
- **Review:** drei adversarielle Runden. Kein Sicherheitsmangel gefunden. Alle Befunde zur Aussagekraft sind in diesem Plan behoben bzw. in §4 als Lese-Regel festgehalten.

## 9. Ergebnis des Probe-Laufs (2026-10-16, 18:34:11–18:34:49Z)

- **Lauf:** genau einmal, mit deiner Freigabe des Plans `77957205…`, nach dem Rezept aus §6.
  - Vorprüfungen: Plan-Hash = freigegebener Hash, rustc 1.93.1, Uhr +31 ms ± 21 ms (NTP), keine Sperre vorhanden.
  - Exit 0, Status `COMPLETE`, alle 11 Fälle gesendet.
  - Ankunft 254–345 ms nach dem Head-Zeitstempel, Antworten nach 25 ms (48Club) bzw. 160–263 ms (BlockRazor).
- **Evidence:** `bsc/.local/g07/builder-probe-77957205fd84/probe.evidence.json`, byte-gleich versioniert unter `bsc/native/evidence/g06a/builder_probe_20261016T183411Z/probe.evidence.json`, SHA-256 `5636c463d6247b90d17d9bc977a6e50aae499434a17d241f16f05b87ba95cc7f`. Sie enthält keinen Token und keine RPC-URL (geprüft).
- **Lese-Regeln:** Alle Antworten sind HTTP 200 mit lesbarem JSON-RPC. Die Vergleichsfälle 1 und 9 sind geparst, damit sind alle Fälle auswertbar.

| Fall | Antwort (roh, gekürzt) | Klasse | Lesart |
|---|---|---|---|
| 1 `48club-g07-shape` | `-32000 nonce too high … tx: 1099511627776 state: 0` | `EXECUTION_REJECTED` | Methode und Format geparst; da 48Club die Ablaufwerte vor den Transaktionen prüft (Fälle 4/5), haben die G0.7-Werte HEAD+2/MAXTS die Einreichungsprüfung bestanden |
| 2 `48club-no-maxtimestamp` | `nonce too high` | `EXECUTION_REJECTED` | `maxTimestamp` ist bei Einreichung optional |
| 3 `48club-maxtimestamp-minus-one` | `nonce too high` (Phase 950 ms) | `EXECUTION_REJECTED` | wie erwartet ohne Aussage (Phase ≥ 100 ms) |
| 4 `48club-past-block` | `-38000 the maxBlockNumber should not be less than currentBlockNum` | `EXPIRY_REJECTED` | 48Club prüft `maxBlockNumber` bei Einreichung |
| 5 `48club-past-timestamp` | `-38000 the maxTimestamp(1792175670) should not be less than currentBlockTimestamp(1792175672)` | `EXPIRY_REJECTED` | 48Club prüft `maxTimestamp ≥` aktuelle Blockzeit |
| 6 `48club-underpriced-control` | `nonce too high` (1 wei) | `EXECUTION_REJECTED` | keine Preisschranke vor der Simulation; Preis UNQUALIFIED |
| 7 `48club-nomerge-type-control` | `-32602 … cannot unmarshal string into Go struct field SendBundleArgs.noMerge of type bool` | `FORMAT_REJECTED` | Feld erkannt (typisiertes bool); Wirkung UNQUALIFIED |
| 8 `48club-positionfirst-type-control` | `-32602 … SendBundleArgs.positionFirst of type bool` | `FORMAT_REJECTED` | Feld erkannt (typisiertes bool); Wirkung UNQUALIFIED |
| 9 `blockrazor-g07-shape` | `-32000 nonce too high … tx: 1099511627776 state: 0` | `EXECUTION_REJECTED` | Methode, Format und Token angenommen; HEAD+2 hat die Einreichungsprüfung bestanden (Fall 10) |
| 10 `blockrazor-past-block` | `-38000 the maxBlockNumber should be larger than currentBlockNum` | `EXPIRY_REJECTED` | BlockRazor prüft `maxBlockNumber > ` aktueller Block bei Einreichung |
| 11 `blockrazor-underpriced-control` | `-38000 gasprice must be at least 0.05 gwei` | `PRICE_REJECTED` | Preisschranke vor der Simulation genau 0,05 gwei; G0.7 mit 0,05 gwei besteht sie (Fall 9), ohne Spielraum |

**Ergebnis für die Pre-Arm-Checks:**

- **Check 6 — bei Einreichung bestanden, beide Builder.** Methode, Format und Token werden angenommen. Beide prüfen die Ablaufwerte vor den Transaktionen und haben die G0.7-Werte angenommen. BlockRazors Preisschranke liegt genau bei 0,05 gwei, und G0.7 besteht sie.
- **Check 6 — offen und UNQUALIFIED,** jeweils mit deiner ausdrücklichen Annahme vor der Freigabe:
  - 48Club-Preis: keine Prüfung bei Einreichung beobachtbar;
  - Durchsetzung von `maxBlockNumber` neben `maxTimestamp` beim Blockbau;
  - Annahme eines gültigen Bundles und Aufnahme.
- **Check 10 — PARTIAL.** 48Club erkennt `noMerge` und `positionFirst` als typisierte Felder; ihre Wirkung bleibt UNQUALIFIED.
- **Bestätigt:** Ein Builder kann den Ablauf schon bei der Einreichung ablehnen. Der G0.7-Versuch hängt also auch an einer pünktlichen Ankunft; die G0.7-Frische-Schranken sind darauf ausgelegt.

Im v2-Entwurf eingetragen (Pre-Arm-Checks 6, 7, 10, 48Club-Text, Send-Head-Regel); neuer Entwurfs-Hash `1b66718d476e938fc6c936cf3c0b4d49ebc59cc2765c043600130b05de8ccf42` (Guard-Replik bestanden, inaktiv).

## Anhang — der exakte Plan (Ausgabe des Trockenlaufs)

**Platzhalter** in den Bodies:

| Platzhalter | Bedeutung |
|---|---|
| `{HEAD+2}`, `{HEAD-1}` | Blocknummern relativ zum Send-Head |
| `{MAXTS}` | die G0.7-Regel `floor((HEAD_MS+900)/1000)+1` |
| `{MAXTS-1}` | eine Sekunde weniger |
| `{HEAD_S-2}` | zwei Sekunden vor der Head-Sekunde |

Alles andere wird byte-genau so gesendet. Die `example`-Zeilen zeigen die Werte und den Request-Hash für den festen Test-Head; die `source`-Zeilen die SHA-256 der ausgeführten Quelldateien, von `Cargo.lock` und der Toolchain-Angabe.

```text
schema: darkbot.g07.builder-probe.v3
purpose: G0.7 pre-arm checks 6 and 10, submission-time only: eth_sendBundle method, request format, typed noMerge/positionFirst, and whether expiry values and the gas price are checked at submission, for 48Club and BlockRazor. It cannot show building-time behavior (noMerge/positionFirst effect, maxBlockNumber enforcement next to maxTimestamp, inclusion) nor that 0.05 gwei suffices.
non_executable: every case has its own two throwaway accounts derived from public seeds (never the G0.7 wallets 0x3e7b487ae5e1c49ab1cf6222bdb5cf016e86aacd / 0x9c8a0db41914beee52dd49790c4f087e252743de); a live run first checks read-only that all 22 have balance 0 and pending nonce 0, else STOP; every transaction has nonce 1099511627776 (2^40, unreachable), value 0, to = sender, gas limit 21000, chain id 56, signed by the G0.7 signer; no two cases share a sender, transaction or bundle
head: HEAD = a new block the G0.7 head poller sees right before the request; HEAD_S = its timestamp in seconds; MAXTS = floor((HEAD_MS + 2 * 450) / 1000) + 1 (the G0.7 rule, maxBlockNumber offset 2). The G0.7 freshness gates apply (head age <= 450 ms, local clock lag <= 150 ms, monotonic staleness <= 450 ms); a case waits for up to 3 new heads for one that passes, else it is skipped, not sent.
framing: G0.7 http_request over G0.7 connect_tls; per case a new connection, opened before the wait for the head (as G0.7 prewarms) and checked live before the write; Authorization header for BlockRazor only, its value from DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION and never recorded; 11 requests in total, each sent at most once, in this order, at least 2 s apart; response read, no retry
single_use: before the first request a live run latches bsc/.local/g07/builder-probe-<first 12 hex of this plan's SHA-256>/ (after its read-only checks); the same plan never runs twice
worked_example: each case below is also built for HEAD=1000, HEAD_MS=5000200, local clock 5000300 ms by the same request function the live run uses (window, expiry, transactions, body, host, Authorization choice and framing; token replaced by REDACTED); request_sha256 is the SHA-256 of that full HTTP request
source: g07_builder_probe/main.rs sha256=e3e38403ebad995c0cbb1bc59c76580d8d7227115163d0c30ae692a6cdfd39a4
source: g07_builder_probe/plan.rs sha256=8903ecedea5ec0f5fc6626a4f6b9bed70bd270655dc43d875a6c0adbbb68d81b
source: g07_mainnet_self_launch_buy/shared.rs sha256=4ff97782646adc9a3f42960263806f777f7bc68377c374e1a17fa60987b80566
source: g07_mainnet_self_launch_buy/builder_submission.rs sha256=4f009987c2e746dde86266add883df9351d46d3e984dd6b06b88e769a904a439
source: g07_mainnet_self_launch_buy/chain_reads.rs sha256=94d07573f702afeecf0d9371110eec2d563bee535c1f043422fa99d006230ab4
source: g07_mainnet_self_launch_buy/inputs.rs sha256=c3f33ece9ec70a8250a64c989ca8466322ffff912848938f1717162bee0cfaa7
source: g07_mainnet_self_launch_buy/transactions.rs sha256=836b9784eac5bf6411de5318d7d72fa3df0216013075274a5d6b5b66348b3545
source: execution/src/acceptance_guard.rs sha256=f83bc396761dce592b3ebbecf2d06f1edb70aa7d61ae74db1a6e407e8743e80f
source: engine/Cargo.lock sha256=4cdac7fb9af11a90a26033a0d201e73ef6f12a99cd8ece9dc1c7699b92d647c4
source: engine/rust-toolchain.toml sha256=dbf41e60591735dab692473c1d6faac53ed58f262ee4dfe261eebd2595bc7ce1
case1 48club-g07-shape: POST https://puissant-builder.48.club/ authorization=none gas_price=50000000 | Does 48Club parse the exact G0.7 request (method, fields, integer maxBlockNumber, maxTimestamp, noMerge, positionFirst)?
  tx0: from=0x69a5f4187591f66c8874605692b911bb765bd1cf nonce=1099511627776 gas_price=50000000 hash=0x567254aa2af891f7b80975020dcce6be7a2fbf7ba1be32390e68116029e29c1b raw=0xf86a860100000000008402faf0808252089469a5f4187591f66c8874605692b911bb765bd1cf80808193a033a6291f4e2498b2a1bcd1293f166712cadd360c280809c3092c9f85ee1707c0a04c3b4ab40044025e01ad44784fa4b798fc74a9791514593f7d73a763b726f5d1
  tx1: from=0xeab8f9bd3479f9966f796fdc5b1811f60df64fec nonce=1099511627776 gas_price=50000000 hash=0xca876cf868c81ef0592c9cd9522237621ca1793e308d8f236e0a579033ea734a raw=0xf86a860100000000008402faf08082520894eab8f9bd3479f9966f796fdc5b1811f60df64fec80808193a01fca83edcb2c7d413c4e4a67d964649ea7524ea038368c79fea7c75fed805a9da039d8eecf2eddf9ae98e4201e3d0d0a4330141fa5469f9c6e76538cd4406369e4
  body={"id":"0xca876cf868c81ef0592c9cd9522237621ca1793e308d8f236e0a579033ea734a","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"maxTimestamp":{MAXTS},"noMerge":true,"positionFirst":true,"txs":["0xf86a860100000000008402faf0808252089469a5f4187591f66c8874605692b911bb765bd1cf80808193a033a6291f4e2498b2a1bcd1293f166712cadd360c280809c3092c9f85ee1707c0a04c3b4ab40044025e01ad44784fa4b798fc74a9791514593f7d73a763b726f5d1","0xf86a860100000000008402faf08082520894eab8f9bd3479f9966f796fdc5b1811f60df64fec80808193a01fca83edcb2c7d413c4e4a67d964649ea7524ea038368c79fea7c75fed805a9da039d8eecf2eddf9ae98e4201e3d0d0a4330141fa5469f9c6e76538cd4406369e4"]}]}
  example: maxBlockNumber=1002 maxTimestamp=5002 request_sha256=bb24b563e38548c942e50902658f9ec50e0020e85cd6899b895e225dbfbb20c7
case2 48club-no-maxtimestamp: POST https://puissant-builder.48.club/ authorization=none gas_price=50000000 | Does 48Club require maxTimestamp (web3 docs: at least one valid expiry with maxTimestamp)?
  tx0: from=0xd7c11b2a59586fbbf3fe68b1f45a2630db708fcd nonce=1099511627776 gas_price=50000000 hash=0x4fec655fc0f031590c846a159e2d62d8578932ed5708ee181ab00c48cba2305a raw=0xf86a860100000000008402faf08082520894d7c11b2a59586fbbf3fe68b1f45a2630db708fcd80808193a027ddf1761f5370ff3b501d8bc54dabc687274bf81c403d480432342433446617a021ef0627b0279120d13d76b53ceafc15e9cf8042d7915f3b8a20d7fbe2e93c24
  tx1: from=0x91c672b6fbf19b9e77bb36b9537f134e84531e13 nonce=1099511627776 gas_price=50000000 hash=0x5bd2e6a7ab114783457ddcfe0682490be3a148f86db305209b9df14e76276913 raw=0xf86a860100000000008402faf0808252089491c672b6fbf19b9e77bb36b9537f134e84531e1380808194a0761a0204477bbc8f3df5a1eecdc32aa450c53b7225e6c31a3e59d9bca73207cfa02c700c0c96e36d7619b94c01b63a32729170dc32c6ca834d971454fbbc3ab9bf
  body={"id":"0x5bd2e6a7ab114783457ddcfe0682490be3a148f86db305209b9df14e76276913","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"noMerge":true,"positionFirst":true,"txs":["0xf86a860100000000008402faf08082520894d7c11b2a59586fbbf3fe68b1f45a2630db708fcd80808193a027ddf1761f5370ff3b501d8bc54dabc687274bf81c403d480432342433446617a021ef0627b0279120d13d76b53ceafc15e9cf8042d7915f3b8a20d7fbe2e93c24","0xf86a860100000000008402faf0808252089491c672b6fbf19b9e77bb36b9537f134e84531e1380808194a0761a0204477bbc8f3df5a1eecdc32aa450c53b7225e6c31a3e59d9bca73207cfa02c700c0c96e36d7619b94c01b63a32729170dc32c6ca834d971454fbbc3ab9bf"]}]}
  example: maxBlockNumber=1002 maxTimestamp=none request_sha256=caeb64ed584c17127fc230b33cc4fc23fb63e798022f1dfdca312bc8be4096a5
case3 48club-maxtimestamp-minus-one: POST https://puissant-builder.48.club/ authorization=none gas_price=50000000 | Does 48Club refuse maxTimestamp without the +1 second (answer depends on the head's millisecond phase)?
  tx0: from=0xd8a2ede38c0417cc68c6a7719de9f87294ca284c nonce=1099511627776 gas_price=50000000 hash=0xfa980b21fbcb301f3a0505ce6b37ace3fc163d11160a358dd6607b9111836129 raw=0xf86a860100000000008402faf08082520894d8a2ede38c0417cc68c6a7719de9f87294ca284c80808193a078f838198348d1792510ef52f416cd7b2a72d8cfe9dd85ae9e6205d0219b57b0a01286a8fdeb61252288718f43cffc6ac4d667be35b5ff97a79bcd1beff4aba937
  tx1: from=0xdb945dbe392b7e17805d317fa9ae664ff3488b33 nonce=1099511627776 gas_price=50000000 hash=0x6ce40283985ff7f23965fc9b4f8a87bedded9ad7e7c828e8cf0dda0747c9326c raw=0xf86a860100000000008402faf08082520894db945dbe392b7e17805d317fa9ae664ff3488b3380808193a03e61fd930b1c9187e2d4b8702ceaccd3f3c9f8c05b4826669917ed40b742ae43a0548f42f578b9098b870daa12e3934232109bd6cf0ff2fc6f66094864a6391028
  body={"id":"0x6ce40283985ff7f23965fc9b4f8a87bedded9ad7e7c828e8cf0dda0747c9326c","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"maxTimestamp":{MAXTS-1},"noMerge":true,"positionFirst":true,"txs":["0xf86a860100000000008402faf08082520894d8a2ede38c0417cc68c6a7719de9f87294ca284c80808193a078f838198348d1792510ef52f416cd7b2a72d8cfe9dd85ae9e6205d0219b57b0a01286a8fdeb61252288718f43cffc6ac4d667be35b5ff97a79bcd1beff4aba937","0xf86a860100000000008402faf08082520894db945dbe392b7e17805d317fa9ae664ff3488b3380808193a03e61fd930b1c9187e2d4b8702ceaccd3f3c9f8c05b4826669917ed40b742ae43a0548f42f578b9098b870daa12e3934232109bd6cf0ff2fc6f66094864a6391028"]}]}
  example: maxBlockNumber=1002 maxTimestamp=5001 request_sha256=549b0366e58a940e211cd488d57aea79fd07eae4d1a3cbbd9875aa792f9ef695
case4 48club-past-block: POST https://puissant-builder.48.club/ authorization=none gas_price=50000000 | Does 48Club check maxBlockNumber at submission (expired block refused)?
  tx0: from=0xa05a0726ffb1f4115c90d974a9d538695ac0a501 nonce=1099511627776 gas_price=50000000 hash=0xcda77d8dc3e22cb671755cccb8dc70db425cb9f02a31dbf6e413205f16f20592 raw=0xf86a860100000000008402faf08082520894a05a0726ffb1f4115c90d974a9d538695ac0a50180808193a006d1eb7ac7de655df5d32290dc8b7fb04fe9dc7331e3de8dafd25fbf46b7f462a0241309090192eccfe9390975411687373c1814872c75b267747e6d513eb68d0f
  tx1: from=0xd0eccb7b83b7c09fb552f14ccf19b770ec6eec1a nonce=1099511627776 gas_price=50000000 hash=0x2e0799f715d15d728c01a643b143da2d89a2f3cc3779f5eb01f087cebbf74010 raw=0xf86a860100000000008402faf08082520894d0eccb7b83b7c09fb552f14ccf19b770ec6eec1a80808194a0b6da5b0690b0046569370a95a4612f12ab1780ec85031b0f92f525b41cc9fa19a06f8bd7e920b090e84b58b3dc27265a1105ede8aa7c42db4d649c113ba5d8cc4f
  body={"id":"0x2e0799f715d15d728c01a643b143da2d89a2f3cc3779f5eb01f087cebbf74010","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD-1},"maxTimestamp":{MAXTS},"noMerge":true,"positionFirst":true,"txs":["0xf86a860100000000008402faf08082520894a05a0726ffb1f4115c90d974a9d538695ac0a50180808193a006d1eb7ac7de655df5d32290dc8b7fb04fe9dc7331e3de8dafd25fbf46b7f462a0241309090192eccfe9390975411687373c1814872c75b267747e6d513eb68d0f","0xf86a860100000000008402faf08082520894d0eccb7b83b7c09fb552f14ccf19b770ec6eec1a80808194a0b6da5b0690b0046569370a95a4612f12ab1780ec85031b0f92f525b41cc9fa19a06f8bd7e920b090e84b58b3dc27265a1105ede8aa7c42db4d649c113ba5d8cc4f"]}]}
  example: maxBlockNumber=999 maxTimestamp=5002 request_sha256=fbf98d2efc5602ca781c22dab5af1cc2310497ecb578a05cd3b5447a107dfdb0
case5 48club-past-timestamp: POST https://puissant-builder.48.club/ authorization=none gas_price=50000000 | Does 48Club check maxTimestamp at submission (past second refused)?
  tx0: from=0xfd6db543bfa0347527c57dfe625af91fb6f79b1f nonce=1099511627776 gas_price=50000000 hash=0xd2392f61034a801032519590edf25f744ed09597a50ed66bcb89944cd3cd6b2e raw=0xf86a860100000000008402faf08082520894fd6db543bfa0347527c57dfe625af91fb6f79b1f80808194a0e8fd3b80ac9ee6428e1b5d0098e617be10ef4e4947d1824440c3888616e0e49ea0620859f2226a8e02b7713c1531f60aed92a91d50f51168dc824d499110739afe
  tx1: from=0x9941f6d4c140be6117124b6d3830838282940fe9 nonce=1099511627776 gas_price=50000000 hash=0x5743287457c4ecde177a37862dbec735c0d70351cf1d8a75c978ff88f513216a raw=0xf86a860100000000008402faf080825208949941f6d4c140be6117124b6d3830838282940fe980808193a0d55935ea485b13c365b5d7e371dfac8e1d7dc825c56da84f661bb742dd44e400a03daa3a7dc0fdcc55916bbb5e5c667b8d596ffcd6574031566725009a0cb83292
  body={"id":"0x5743287457c4ecde177a37862dbec735c0d70351cf1d8a75c978ff88f513216a","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"maxTimestamp":{HEAD_S-2},"noMerge":true,"positionFirst":true,"txs":["0xf86a860100000000008402faf08082520894fd6db543bfa0347527c57dfe625af91fb6f79b1f80808194a0e8fd3b80ac9ee6428e1b5d0098e617be10ef4e4947d1824440c3888616e0e49ea0620859f2226a8e02b7713c1531f60aed92a91d50f51168dc824d499110739afe","0xf86a860100000000008402faf080825208949941f6d4c140be6117124b6d3830838282940fe980808193a0d55935ea485b13c365b5d7e371dfac8e1d7dc825c56da84f661bb742dd44e400a03daa3a7dc0fdcc55916bbb5e5c667b8d596ffcd6574031566725009a0cb83292"]}]}
  example: maxBlockNumber=1002 maxTimestamp=4998 request_sha256=81bcd27943a8ab09d75d18a0b10d3707f0272158db301085c80e86c5db3d9ba5
case6 48club-underpriced-control: POST https://puissant-builder.48.club/ authorization=none gas_price=1 | Does 48Club check the gas price at submission (1 wei refused while 0.05 gwei is not)?
  tx0: from=0xada7fd9fe3ec8639ab4aa5260fa04aa722915c47 nonce=1099511627776 gas_price=1 hash=0x8f783c7c86ad498e6b8d04e58d5cc7fe86b0164429c21a3776f003c7b7cafe91 raw=0xf866860100000000000182520894ada7fd9fe3ec8639ab4aa5260fa04aa722915c4780808194a0c4c39938e817a87454362abc5f27d40def0ab4edf339f79c4977bc1555b1683aa016b5c8c0e89c6a6b9b6211eb999766d4b60ae3b5ff75bf549f8c873beafb59f1
  tx1: from=0x39906408ec73b9fd179bf99d86962aaa30cd1bf7 nonce=1099511627776 gas_price=1 hash=0xabd117b6abc2c3410f0e3e2bc664ae0e369fec68627213abe7654449f765c7bf raw=0xf86686010000000000018252089439906408ec73b9fd179bf99d86962aaa30cd1bf780808193a026fd62313171cf2c2e1e78f40bce5e763ce86b55a6e370fd7da03d688ad55949a049f599c2ac72297e577f7124d43fcf101fb96247342d92546b666a22ff4da4c2
  body={"id":"0xabd117b6abc2c3410f0e3e2bc664ae0e369fec68627213abe7654449f765c7bf","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"maxTimestamp":{MAXTS},"noMerge":true,"positionFirst":true,"txs":["0xf866860100000000000182520894ada7fd9fe3ec8639ab4aa5260fa04aa722915c4780808194a0c4c39938e817a87454362abc5f27d40def0ab4edf339f79c4977bc1555b1683aa016b5c8c0e89c6a6b9b6211eb999766d4b60ae3b5ff75bf549f8c873beafb59f1","0xf86686010000000000018252089439906408ec73b9fd179bf99d86962aaa30cd1bf780808193a026fd62313171cf2c2e1e78f40bce5e763ce86b55a6e370fd7da03d688ad55949a049f599c2ac72297e577f7124d43fcf101fb96247342d92546b666a22ff4da4c2"]}]}
  example: maxBlockNumber=1002 maxTimestamp=5002 request_sha256=61bef3ecb705dd4d567a1964a0fb63c16c4e7ea1279a5252d94923ee9a0cedf4
case7 48club-nomerge-type-control: POST https://puissant-builder.48.club/ authorization=none gas_price=50000000 | Does 48Club parse noMerge as a typed field (a string value refused)?
  tx0: from=0xcbeb224cec4cede7c1cf0a845eb6e900a759c1f7 nonce=1099511627776 gas_price=50000000 hash=0x5637d6d61551bdefb16302893b920217e8b70ea6b444e0dc7767ef1d0df6d084 raw=0xf86a860100000000008402faf08082520894cbeb224cec4cede7c1cf0a845eb6e900a759c1f780808194a0d3b130426c57c8708c55f8ff1ba49fd2f8b4a15f21d8f6d80244f07c76ca1723a06821ab6312e3f1be9fdfbe24d564b87a080696ec8a4c74d78d559be2b4b0f621
  tx1: from=0x61bcabab254e67248befb7540a84a35c131500f2 nonce=1099511627776 gas_price=50000000 hash=0x6ccdffdb7538ca82511f76e96728eda8e8bf367e71ce6ecaef57b08cf69a237b raw=0xf86a860100000000008402faf0808252089461bcabab254e67248befb7540a84a35c131500f280808194a04a73ec4baa0b98e0e59707340f325162f3b1ed9f88368ed97a3c334d3c5d7e62a05caaa809de667731d56c206e1f6929d3e1897c006d95093dbfe7e43d4f09aa37
  body={"id":"0x6ccdffdb7538ca82511f76e96728eda8e8bf367e71ce6ecaef57b08cf69a237b","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"maxTimestamp":{MAXTS},"noMerge":"true","positionFirst":true,"txs":["0xf86a860100000000008402faf08082520894cbeb224cec4cede7c1cf0a845eb6e900a759c1f780808194a0d3b130426c57c8708c55f8ff1ba49fd2f8b4a15f21d8f6d80244f07c76ca1723a06821ab6312e3f1be9fdfbe24d564b87a080696ec8a4c74d78d559be2b4b0f621","0xf86a860100000000008402faf0808252089461bcabab254e67248befb7540a84a35c131500f280808194a04a73ec4baa0b98e0e59707340f325162f3b1ed9f88368ed97a3c334d3c5d7e62a05caaa809de667731d56c206e1f6929d3e1897c006d95093dbfe7e43d4f09aa37"]}]}
  example: maxBlockNumber=1002 maxTimestamp=5002 request_sha256=d2f6157ede2598c04853ae64ea29db89593c8840c28fa1bbd8175fc27095d9a2
case8 48club-positionfirst-type-control: POST https://puissant-builder.48.club/ authorization=none gas_price=50000000 | Does 48Club parse positionFirst as a typed field (a string value refused)?
  tx0: from=0x8429f24188208811265b5b0a0e700454287dd627 nonce=1099511627776 gas_price=50000000 hash=0x082b2d7bbf9724b1a8adf0f3e100380c4111e13e839fa4a172edc0e8cb3e4b8d raw=0xf86a860100000000008402faf080825208948429f24188208811265b5b0a0e700454287dd62780808194a021b3fa18ddf8aacb6d184f67d95ac7a702192cc5b10be6378dd05ae4a78f3526a028b8b906ab07827ead43a8c1313b02c0c34e99c2e64ccccee8b82ef4d5d1c72e
  tx1: from=0x89dfd7d375af518d40a68fcdaa5b8c75cf351991 nonce=1099511627776 gas_price=50000000 hash=0x8869bbefe98399fcd1b229e1ec02ce303c87c70768d8da099feb81c300f8d5c1 raw=0xf86a860100000000008402faf0808252089489dfd7d375af518d40a68fcdaa5b8c75cf35199180808194a012e57610d47b2cb2b104c6458020976efc37b09d7ec06c528860e2b3f6dc9956a068b8e06edc57a0f702daec6742f94366cbbb1cef3edc3b297ad6aa450c447195
  body={"id":"0x8869bbefe98399fcd1b229e1ec02ce303c87c70768d8da099feb81c300f8d5c1","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"maxTimestamp":{MAXTS},"noMerge":true,"positionFirst":"true","txs":["0xf86a860100000000008402faf080825208948429f24188208811265b5b0a0e700454287dd62780808194a021b3fa18ddf8aacb6d184f67d95ac7a702192cc5b10be6378dd05ae4a78f3526a028b8b906ab07827ead43a8c1313b02c0c34e99c2e64ccccee8b82ef4d5d1c72e","0xf86a860100000000008402faf0808252089489dfd7d375af518d40a68fcdaa5b8c75cf35199180808194a012e57610d47b2cb2b104c6458020976efc37b09d7ec06c528860e2b3f6dc9956a068b8e06edc57a0f702daec6742f94366cbbb1cef3edc3b297ad6aa450c447195"]}]}
  example: maxBlockNumber=1002 maxTimestamp=5002 request_sha256=cf240ff147dc7a5bf25d3a6fae2557f4c2fadef6b1517650907a9ac52557fb07
case9 blockrazor-g07-shape: POST https://virginia.builder.blockrazor.io/ authorization=yes gas_price=50000000 | Does BlockRazor parse the exact G0.7 request (method, fields, integer maxBlockNumber, Authorization)?
  tx0: from=0x8ece271db8c22bb6c53e1661ed10ab0424b49345 nonce=1099511627776 gas_price=50000000 hash=0xf7f11c462cd9570ee5338783f7a2c3b6c544bd92e14c4cc5f6c0f2b8e3253fa8 raw=0xf86a860100000000008402faf080825208948ece271db8c22bb6c53e1661ed10ab0424b4934580808193a04461d6c50cbe189854a5ec992f9f237ff9fd34ca90d126ceff1c430126b7c271a045e462331d7e756b257d5c5e3299405c7eb189c0145a5dfd03fc584dab473448
  tx1: from=0x874d70e013caa214bcd35f76a1306111ef96623d nonce=1099511627776 gas_price=50000000 hash=0x6c995e456dc53113399ea4e28a3051e4fac564dc87689b8818e1be467d68868e raw=0xf86a860100000000008402faf08082520894874d70e013caa214bcd35f76a1306111ef96623d80808193a0b34d501826d80c65a41122d76b0ef61e81f2420d8175a5f33584e7dc29ba0299a004a91929123a4dd21206025977d0bbf37d87bdb0ce1c2d9cb29b6d079f418141
  body={"id":"0x6c995e456dc53113399ea4e28a3051e4fac564dc87689b8818e1be467d68868e","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"txs":["0xf86a860100000000008402faf080825208948ece271db8c22bb6c53e1661ed10ab0424b4934580808193a04461d6c50cbe189854a5ec992f9f237ff9fd34ca90d126ceff1c430126b7c271a045e462331d7e756b257d5c5e3299405c7eb189c0145a5dfd03fc584dab473448","0xf86a860100000000008402faf08082520894874d70e013caa214bcd35f76a1306111ef96623d80808193a0b34d501826d80c65a41122d76b0ef61e81f2420d8175a5f33584e7dc29ba0299a004a91929123a4dd21206025977d0bbf37d87bdb0ce1c2d9cb29b6d079f418141"]}]}
  example: maxBlockNumber=1002 maxTimestamp=none request_sha256=f6532c374b06c8ed51f572be6042b85877adcfd190e2b08351c78bde7ec354c4
case10 blockrazor-past-block: POST https://virginia.builder.blockrazor.io/ authorization=yes gas_price=50000000 | Does BlockRazor check maxBlockNumber at submission (documented -38000 below the current block)?
  tx0: from=0xefbdb19fc847074521f9c2d1a625c33eb41dd3b5 nonce=1099511627776 gas_price=50000000 hash=0x9e3fef06b2fbb49a983ce13649f352ddb6dfb9eee0e7c522164ba49794c539ee raw=0xf86a860100000000008402faf08082520894efbdb19fc847074521f9c2d1a625c33eb41dd3b580808193a06b6f13f23026e6f05aafcf655423c17ebd76da5340d679a3ab8108f53c173f0aa0220a3cb22deb15878e93d76f05f37b66471895586cb395ba0c8936f791e55808
  tx1: from=0xd4d6a8eeaf25926ee62e22964f2418c618d976d7 nonce=1099511627776 gas_price=50000000 hash=0x1203660d76d6f9452828d115aeb577597d45e7763a9c2fb4218750c71e6b08fb raw=0xf86a860100000000008402faf08082520894d4d6a8eeaf25926ee62e22964f2418c618d976d780808193a0a88c0b256b9cd69078490f908b3b8c653bc970f91f74c63915a2c588ed9d830da01c26e0afc48d4f4fbf6e382a60180fc9a42a136218c508102d8e7eb89d1f4ab5
  body={"id":"0x1203660d76d6f9452828d115aeb577597d45e7763a9c2fb4218750c71e6b08fb","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD-1},"txs":["0xf86a860100000000008402faf08082520894efbdb19fc847074521f9c2d1a625c33eb41dd3b580808193a06b6f13f23026e6f05aafcf655423c17ebd76da5340d679a3ab8108f53c173f0aa0220a3cb22deb15878e93d76f05f37b66471895586cb395ba0c8936f791e55808","0xf86a860100000000008402faf08082520894d4d6a8eeaf25926ee62e22964f2418c618d976d780808193a0a88c0b256b9cd69078490f908b3b8c653bc970f91f74c63915a2c588ed9d830da01c26e0afc48d4f4fbf6e382a60180fc9a42a136218c508102d8e7eb89d1f4ab5"]}]}
  example: maxBlockNumber=999 maxTimestamp=none request_sha256=54df75764b1e274cc8431bca5383146c3fae9cffbd318cb6964cdce5c0ce978b
case11 blockrazor-underpriced-control: POST https://virginia.builder.blockrazor.io/ authorization=yes gas_price=1 | Does BlockRazor check the gas price at submission (1 wei refused while 0.05 gwei is not)?
  tx0: from=0x290f02267bcc77287e8913805d0fd57d64dc08fc nonce=1099511627776 gas_price=1 hash=0x524289c8a0177d9c37a280cec19e2b6dd93bb75c638fde9e8a4c865e3157970e raw=0xf866860100000000000182520894290f02267bcc77287e8913805d0fd57d64dc08fc80808194a0efb20ee14d5e70dcd2d389fd7e208ac4895c6caefeb3e6e26d2cb641c31c65c3a036c7ed168ce8bf0275661ad723ddda9ff7a5e08747f5b099e35ffd03193afc05
  tx1: from=0xe1c0a238e874d7232780354edc70cc1eda5a00ec nonce=1099511627776 gas_price=1 hash=0x789d474cfa0729043cd1dbfd4e5e9266f49467a495708739282602f08c1ad2b1 raw=0xf866860100000000000182520894e1c0a238e874d7232780354edc70cc1eda5a00ec80808193a048ca5b6b83a47b111bb96039fcae9f346fcd7446a1d239cb4f07e23513e23ee7a01275f84ef9c8e65cb0e4c19886da7c487fa0f2f42f5d26382d0e96f6cdcbd625
  body={"id":"0x789d474cfa0729043cd1dbfd4e5e9266f49467a495708739282602f08c1ad2b1","jsonrpc":"2.0","method":"eth_sendBundle","params":[{"maxBlockNumber":{HEAD+2},"txs":["0xf866860100000000000182520894290f02267bcc77287e8913805d0fd57d64dc08fc80808194a0efb20ee14d5e70dcd2d389fd7e208ac4895c6caefeb3e6e26d2cb641c31c65c3a036c7ed168ce8bf0275661ad723ddda9ff7a5e08747f5b099e35ffd03193afc05","0xf866860100000000000182520894e1c0a238e874d7232780354edc70cc1eda5a00ec80808193a048ca5b6b83a47b111bb96039fcae9f346fcd7446a1d239cb4f07e23513e23ee7a01275f84ef9c8e65cb0e4c19886da7c487fa0f2f42f5d26382d0e96f6cdcbd625"]}]}
  example: maxBlockNumber=1002 maxTimestamp=none request_sha256=47041402f34f4820bd06f2e9b53a518d9ce32fbbd25a8586464778980ee6fd45
PLAN_SHA256=77957205fd844dea51d4eb97a4b12c345978952fafc91c95cefec75d28f9abf5
```
