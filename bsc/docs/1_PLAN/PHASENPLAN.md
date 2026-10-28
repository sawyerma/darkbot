# BSC PHASENPLAN — DarkBot Gate-0 First

Status: **ACTIVE AUTHORITATIVE ROADMAP**
Updated: 2026-10-28 (prioritize complete native LIVE and GUI implementation before comprehensive system test; historical versions preserved)

## VERBINDLICHE SOFORTENTSCHEIDUNG (28.10.2026) — LIVE-CODE VOR DEM GESAMTSYSTEMTEST

**Höchste aktuelle Entwicklungspriorität.** Vor Beginn der umfassenden neuen Schalter-, WSS-, E2E- und BUY-/SELL-Testkampagne muss Claude den **vollständigen produktiven BSC-LIVE-Code jetzt implementieren, integrieren, committen und pushen**. Diese Entscheidung übersteuert alle älteren Abschnitte, die die **technische Implementierung** von J/LIVE bis nach G0.7/G0.8 verschieben oder nur einen dauerhaft BLOCKED-LIVE-Platzhalter zulassen. **Keine weitere Programmierungsphase nach der Gesamtabnahme**, nur noch tatsächliche Betreiber- und wirtschaftliche Freigaben. Bereits fertig belegte Bausteine weiterverwenden; keine neue Bot-Architektur, kein Parallel-Core, keine Wiederholung abgeschlossener Phasen.

**Gewünschter Bedienablauf:** Im BSC-Reiter Wallets schaltet der Betreiber BSC_WALLET_LAUNCHER und BSC_WALLET_BUYER ausdrücklich ENABLED. Anschließend öffnet er einen **sichtbaren, direkt erreichbaren BSC-Reiter LIVE** und kann dort einen konkreten qualifizierten LIVE-Umfang (Source/Venue/Profil/Strategie, BUY+SELL, Transport, Limits/Budget) mit den erforderlichen weiteren wirtschaftlichen Zustimmungen aktivieren. Das System zeigt fehlende Voraussetzungen und fordert erforderliche Autorisierungen **in der UI**, nicht durch nachträgliches Ändern von Rust, .env, Guard-Konstanten, Git-Dateien oder manuelles Programmieren. Sind alle bindenden Voraussetzungen erfüllt, muss der native Owner requested=LIVE als **effective=LIVE** tatsächlich wirksam machen und die produktive Engine steuern; fehlen sie, wird mit genauem Grund sicher BLOCKED gemeldet. **Wallet-ENABLE allein darf nie automatisch signieren, senden oder die anderen Freigaben erteilen.** Ein LIVE-Knopf mit dauerhaft BLOCKED selbst nach vollständiger gültiger Autorisierung ist **keine Umsetzung**.

**Codeumfang J/LIVE jetzt als EIN Paket technisch fertigstellen:**

1. **Native Autorität.** In bsc/engine/crates/execution/src/sources.rs und allen echten Callern die bisher hartkodierte Regel entfernen, dass LIVE selbst bei BSC_ECON_LIVE ENABLED niemals effective LIVE sein kann. Stattdessen echte, atomare, evidenz- und scopegebundene LIVE-Gates mit requested/effective/block_reason, CAS/Receipt/Restart/Revocation, Wallet-/Wirtschaftsschaltern, Policy- und Attempt-Authority einbauen. Bei fehlenden Genehmigungen: fail-closed, niemals stillschweigend PASS. Nicht bloß die Blockierung auskommentieren; die notwendige geprüfte LIVE-Ausführung **implementieren**.
2. **Ein Production-Core von Signal bis Kauf.** Bestehende native WSS-/GETH-/Ingress-, verifizierte FULL-Signed-TX-/Launch-/Token-/Venue-Decoding-, C2-State-/Nonce-/Reorg-, C3-Economics-/Quote-/Admission-/Sign-, Lane-/Queue- und Inclusion-/Provider-Adapter **real zusammenschalten**. Die erste vollständig verifizierte und für den tatsächlich freigegebenen Launch-/Creator-/Tax-/Quote-Scope handlungsfähige Quelle darf den Kauf auslösen; HASH_ONLY nie. BUY muss bis zum echten, autorisierten Send-Adapter und zur kanonischen Receipt-/Positionserfassung verdrahtet sein, nicht nur zur PAPER-Senke. Fremdlaunch-/AUTO-Profile, falls als LIVE-Scope vorgesehen, mit unabhängiger Tokenableitung und korrekter Economics fertigstellen; kein unqualifizierter Profile-Fallback.
3. **Realer Exit- und Positionspfad.** Approval/SELL, Gas/Tax/Slippage, tatsächliche Tokenposition, Nonce, Exit-Bedingungen, Spend-Limit, Reorg-/Crash-/UNKNOWN-Handling, Receipt-basierte Kosten und realisierter Netto-PnL müssen im **produktiven** gleichen Rust-System implementiert sein. PAPER-SELL ist kein echter Exit-PASS; wirtschaftlicher Betrieb wird bei nicht zulässigem/qualifiziertem Exit nicht heimlich enabled.
4. **Alle 20 Controls plus Quellenfähigkeit.** Fünf Source-IDs (GETH, PUBLICNODE, DRPC, BLOCKREQ, 48CLUB), drei Komponenten, drei Venues, sieben Wallet-/ECON-Schalter, Runtime-Modus und Tageslimit im einen Owner mit fachlich passenden Abhängigkeiten. Eine technisch vorhandene und qualifizierte Quelle/Transportart ist tatsächlich anschließbar; fehlender Providerzugang, nicht vorhandener synchronisierter Geth-Node oder unqualifizierte Four-/48Club-Funktionen bleiben mit ehrlichem UNQUALIFIED/BLOCKED sichtbar, **nicht als funktionierend behauptet**. Nicht benötigte externe Provider blockieren keinen anderen vollständig qualifizierten Modus. Keine kostenpflichtige Registrierung oder Hardware ohne gesonderte Zustimmung.
5. **LIVE-Frontend und ein einziger Steuerweg.** Den ausdrücklich gewünschten **BSC-LIVE-Reiter** zum bestehenden BSC-Frontend hinzufügen, Native → Protobuf → Gateway → Store → LIVE-/Wallets-/Monitoring-UI vollständig verdrahten (bestehende BSC-Ansichten bleiben erhalten). Wallets zuerst einzeln ENABLE; danach im LIVE-Reiter Quelle/Venue/Strategie, zulässiger BUY-/SELL-Transport, Candidate/Guard/Provider-Status, tägliches und kumulatives Attempt-/Geldlimit, tatsächliche Positionsrisiken und exakte Genehmigungen sehen. Aktivieren, DISABLE/STOP, Recovery und bestätigte Receipts mit requested/effective/status/error **funktionieren real**, nicht nur als UI-Demo. Keine verborgene Terminalpflicht für den Betreiber bei zulässiger LIVE-Aktivierung.
6. **Finanzielle Sicherung und Idempotenz.** Guard/V5/Chain/Byte-/Nonce-/Fee-/Budget-/Builder-Frische-/Source-Capability-/Attempt-Authority bleiben echte Prüfer vor jeder wirtschaftlichen Freigabe. Persistent atomare Gesamtgrenze der beschlossenen maximal zehn G0.7-/Folgeversuche einschließlich UNKNOWN und Tageslimit, keine automatischen Retries/Doppelsends, sauberer Kill-/Crash-/Restart-/Disconnect-/Reorg-/Provider-Antwortverlust. Alle Wallet- und Geldschalter initial DISABLED, beim Restart fail-closed; separates DISABLE stoppt zukünftige Freigaben, behauptet nie einen bereits gesendeten BUY rückgängig zu machen.

**Exakte geänderte Reihenfolge:** (I) **sofort vollständiger produktiver LIVE-Implementierungsblock** einschließlich lokaler Entwicklungs-Unit-/Build-Sanity-Checks, ausschließlich synthetisch und ohne Mainnet-Egress; (II) **erst DANACH umfassende System-Testkampagne**: vollständige Schalter-Matrix A01–A16/B01–B16/C01–C20 sowie logische Unterkombinationen, alle fünf Quellen einzeln mit ehrlichem Access-/WSS-Verdikt, WSS-Früherkennung, Native-Launch→BUY→SELL, Latenzen, Race/Crash/Recovery, native CLI/Protobuf/Gateway/Frontend, Release/CI und unabhängiger Review. Tests der Codeänderungen während der Implementierung sind zulässig; die **abschließende Gesamtprüfung** darf keine fehlende LIVE-Implementierung schönprüfen. (III) SIGN-DRY/V5/Readiness und exakte G0.7-Freigabe, soweit nicht parallel schlüsselfrei vorbereitbar; (IV) ein echter G0.7-BUY nur nach neuer ausdrücklicher wirtschaftlicher Freigabe, danach G0.8. Die optionale G0.4-Sichtung bleibt auf insgesamt höchstens zehn Minuten begrenzt und ist keine Wartepflicht.

**Nicht verwechselt werden dürfen technische Fertigstellung und Echtgeldfreigabe:** Diese Nutzerentscheidung autorisiert **JETZT die vollständige LIVE-Codeentwicklung**, keine echten Wallet-Key-SIGN-Läufe, ARM/Grant/SEND/BUY/SELL, keine Aktivierung der finanzierten Betreiber-Wallets und keine kostenpflichtigen Dienste. G0.7/G0.8, Source-/Venue-/Wirtschaftsqualifikation und explizite Nutzerentscheidung bleiben die **späteren Aktivierungsgates**, die bereits jetzt im Code produktiv verdrahtet sein müssen; kein späteres Nachprogrammieren. Auch bei fehlendem originalen Producer-Witness bleibt die physische T0-Latenz UNKNOWN, nicht erfunden PASS.

**Claude-Ausführung verbindlich:** Nicht nochmals ein Konzept/Audit als Ersatz für Code senden. Ab sofort Implementierung nach Code-Abhängigkeiten, unabhängiger Reviewer nach finalem SHA, Commit/Push auf origin/main, STATUS und zentralen Messplan an die neue Reihenfolge anpassen. Als Done-Definition eine tatsächlich über die UI aktivierbare, mit synthetischen Genehmigungen und nichtsendendem Testtransport erreichte **effective=LIVE**-Engine bis BUY+SELL und darunterliegender Fachlogik belegen; fehlende echte Betreiber-/Mainnet-Genehmigungen bleiben sauber BLOCKED. Geltung nur BSC, keine fremde Project_0-Änderung.

---

## AKTUELLE NUTZERENTSCHEIDUNG (27.10.2026) — ZEHN BUY-VERSUCHE GESAMT, MINIMALBETRAG UND SEPARATER SELL-NACHWEIS

**Vorrang für die folgende gezielte Echtgeld-Testserie:** **Maximal 10 echte wirtschaftliche BUY-Versuche insgesamt, einschließlich des ersten eigenen G0.7-Self-Launch-BUYs** — **nicht 1 + 10**. Ein Revert, Timeout nach möglichem Write, `UNKNOWN` oder sonstiger freigegebener wirtschaftlicher Versuch zählt zur Obergrenze, nicht nur ein erfolgreicher Kauf. Bei 10 ist für diese Kampagne STOP, auch bei Wechsel des UTC-Tags oder Neustart. Die bereits implementierte Einstellung `BSC_BUY_ATTEMPTS_PER_DAY` begrenzt ausschließlich die Versuche je UTC-Tag; sie ist **kein** kampagnenübergreifender 10er-Zähler. Bis die Gesamtgrenze im bestehenden Native-Owner atomar, dauerhaft und CLI-/Frontend-sichtbar gebunden oder gleichwertig extern autoritativ erzwungen ist, keine automatisierte Zehnerserie freigeben. Individuelle V5-/Attempt-Authority und wirtschaftliche Freigabe bleiben pro Versuch nötig. Keine automatische Ausführung einer festgelegten Stückzahl, keine Auto-Retries, Test darf nach früherem FAIL/STOP jederzeit enden.

**Minimaler Einsatz, kein erratener Mindestwert:** Für jeden geplanten Venue-/Caller-/Quote-/Deployment-Scope den *kleinsten reproduzierbar erfolgreichen, technisch und wirtschaftlich zulässigen BUY-Input* zunächst read-only/lokal qualifizieren (Launch erfolgreich, anschließender BUY erfolgreich, positiver und korrekt zuordenbarer Token-Zufluss, richtige minOutput-/Fee-/Tax-Rundung). Historische `0,000001 BNB` des eigenen G0.7-Profils sind nur ein zuvor qualifizierter Kandidat, kein für die aktuelle Portalversion bewiesenes absolutes Minimum; `1 wei` ist ausdrücklich nicht final qualifiziert. Gas, Builder/Tip, Launch, Approval und ein späterer SELL sind zusätzliche Kosten. Vor der Serie ein **kumulatives Worst-Case-Budget einschließlich fehlgeschlagener, umgekehrter oder unverkäuflicher Positionen** festlegen und technisch begrenzen; keine Freigabe nur mit dem Nominal-BUY-Wert. Kein höheres Kapital auf Verdacht, kein automatisches Anheben des Limits.

**Messplan pro tatsächlich freigegebenem Versuch:** `1..10`, Profil `SELF_A/SELF_B/FOREIGN_AUTO`, Candidate/Chain/Wallet-Identität, exakter nominaler BUY-Input, Budget/Worst-Case, Ingress je Quelle (hash/full/actionable), Build-/Send-Zeitstempel `T_actionable/T_ready/T_queue/T_send` und Adapter/Clock-Definition, kanonischer Launch-/BUY-Block und Index, N+1 ja/nein, Konkurrenz vor dem eigenen BUY, Token-Balance-Delta, `SUCCESSFUL_BUY/FAIL/UNRESOLVED`, alle tatsächlich angefallenen Kosten und auf der tatsächlichen Walletposition basierende Exit-/PnL-Daten **sofern ein echter SELL qualifiziert und ausgeführt wurde**. Nach jedem Versuch Ergebnis und kumulative Zähler (Versuche, positive Tokenkäufe, Fehlversuche, Unknowns, realer Spend/Restbudget) auswerten. Erfolgsquote **erfolgreiche BUYs / alle wirtschaftlich gestarteten BUY-Versuche**; zu jeder Messzahl Nenner und Roh-Evidence. Keine Langzeitbeobachtung als Voraussetzung, die pro Versuch vertraglich nötigen kurzen Original-Capture-/Canonical-Reconciliation-Schritte bleiben erhalten.

**SELL ausdrücklich getrennt:** Aktuell im Rust-Code vorhandene Strategy-C-Exit-Regeln und signierte PAPER-Approve-/SELL-Pfade sind **kein nachgewiesener echter Mainnet-SELL-Pfad**. Die bestehende PAPER-Default-Policy in `bsc/config/ingress.public.env` ist `target_net_multiple=2`, `stop_net_multiple=0.5`, `hold_ms=600000`, CreatorSell vorrangig, verkauft die volle Tokenposition, PAPER-Fill erst nach einem konfigurierten weiteren Block. Sie darf **nicht stillschweigend als live freigegeben** gelten. Vor einem als BUY→SELL oder realisiert-netto bezeichneten Test sind echte unterstützte Exit-Calldata, ggf. Approval, SELL-Gas, Nonce, Wallet-/SELL-Autorisierung, Max-Slippage, mögliche Reverts/Migration, Chain-Receipts und native Safety an derselben Position nachzuweisen. Keine automatische zehnfache Kapitalbindung ohne geklärte Möglichkeit, Positionen zu schließen; gegebenenfalls nach dem ersten G0.7-BUY STOP, bis ein echter Exit oder eine ausdrückliche Risikoentscheidung vorliegt.

**Die frühen Beobachtungs-/Echtgeldgates bleiben getrennt:** eigener G0.7-BUY mit G0.8-Auswertung zuerst, danach fremde Launches nur nach qualifiziertem fremdem BUY-Profil. Eine Serie von 10 ist ein technischer reproduzierbarer Kleinbetragsversuch, keine statistische Produktions-J-Freigabe und kein Nachweis für `T_buy-T0 <1ms` ohne Original-Producer-Witness.

---

## AKTUELLE NUTZERENTSCHEIDUNG (28.10.2026) — MAXIMAL 10 MINUTEN OPTIONALES G0.4, DANACH EREIGNIS- UND LIMITGESTEUERTE BUY/SELL-VERSUCHE

Diese Entscheidung hat Vorrang vor den älteren sieben Kalendertagen aus G0.4/V10-8 und den hier bislang nur vorgeschlagenen Zeit-/Stichprobenschwellen, soweit sie als **Wartepflicht vor gezielten Käufen** verstanden werden. Falls eine ergänzende rein lesende explorative Source-/Mempool-Sichtung überhaupt erforderlich ist, dauert sie **höchstens 10 Minuten**, danach geordneter Stopp. **Keine Mindestmessdauer und keine G0.4-Wartepflicht vor SIGN-DRY, V5-Readiness oder G0.7.** Bereits gesammelte Rohdaten werden verwendet; keine erneute zehnminütige Pflichtkampagne starten. Ist das bestehende read-only-G0.4-Capture bereits über 10 Minuten aktiv, über den vorhandenen nativen Stop-Mechanismus **geordnet beenden** und Journal/Hashes/Rohdaten unverändert sichern, nicht killen oder neu schreiben. Diese Git-Änderung beendet einen bereits laufenden Prozess nicht automatisch.

**Die Messung dient nur der technischen Früh-Sichtbarkeit und Quellenqualität**, nicht dem Nachweis eines profitablen Coins. Zu wenig Stichprobe => `NOT_EVALUABLE/UNKNOWN`, niemals erfundener Coverage-/Markt-PASS. Die unverändert archivierten alten Kriterien und historischen Rohdaten bleiben lesbar; 7-Tage-Census und die in `g04_criteria.env` nur `PROPOSED` Schwellen sind nach dieser Entscheidung **keine obligatorische Vorbedingung** eines einzelnen kontrollierten G0.7-BUY-Versuchs.

**Handelssteuerung statt Pflichtlaufzeit:** Spätere Tests sind **ereignisgesteuerte, gezielte BUY- und – sobald nativ qualifiziert – SELL-Versuche**, ohne festes Stunden-/Tagesfenster für die Botlaufzeit. Statt eines Zeit-Stopps einen **typisierten, über Terminal UND Frontend auf denselben Native-Owner gerouteten konfigurierbaren Tageshöchstwert** für neue wirtschaftliche BUY-Versuche vorsehen, zum Beispiel `max_buy_attempts_per_day=10`. Dieser Grenzwert zählt *alle* freigegebenen wirtschaftlichen BUY-Versuche einschließlich gesendeter/unklarer Versuche, nicht nur erfolgreiche Tokenkäufe; der genaue Tageswechsel (Zeitzone) wird eindeutig im Vertrag angegeben. Dauerhafter atomarer Zähler mit Crash-/Restart-/Retry-/Idempotenztests und sichtbarem Verbrauch/Restbudget. Der Tageshöchstwert ist **keine automatische Ausführungsfreigabe** und kein Ziel, zehn Trades zu erzwingen. **Default: keine wirtschaftlichen Versuche erlaubt**, bis die notwendigen nativen Wallet-/LIVE-/Attempt-Controls ausdrücklich und für den jeweiligen Test gültig aktiviert sind. G0.7 bleibt zunächst ein individuell freigegebener, begrenzter Versuch; weitere reale Versuche benötigen ihre jeweiligen aktuellen Freigaben.

**Pro Versuch automatisch erfassen:** Hash-/Full-/Actionable-Quellenzeiten der verfügbaren Ingress-Verbindungen, unabhängig abgeleitete Tokenadresse, Quote, Nonce, Entscheidung, T_ready/T_queue/T_send, Canonical-Inclusion, tatsächlicher Token-Zufluss und Kosten. Bei einem qualifizierten SELL zusätzlich Exit, Netto-PnL und Gebühren auf derselben gebundenen Positionsidentität. Vorher keine Profitabilitätsbehauptung. Physisches T_buy-T0 ohne Original-Producer-Witness weiter `UNKNOWN`. Technische Anbieter-/Profil-Qualifikation, Anti-Replay, Budget, STOP und Wallet-`DISABLED` bleiben intakt.

---

## NEUESTE NUTZERENTSCHEIDUNG (27.10.2026) — BSC-WALLET-CONTROLS UND TECHNISCHE SCHALTBARKEIT

Diese Entscheidung ist für die weitere **technische Implementierung** aktueller als ältere Entwicklungsstopps. Alle BSC-Funktionen, auch die technisch vorbereitete Phase J und ihre Controls, dürfen ohne echten Mainnet-BUY vollständig implementiert, integriert und mit Testidentitäten geprüft werden. Historische Gate-0-Evidence und wirtschaftliche Korrektheitsprüfungen bleiben erhalten.

**Zwei unabhängige native Wallet-Schalter:** `LAUNCHER` und `BUYER` bekommen jeweils `ENABLE/DISABLE`. Initialzustand, fehlender Zustand und Wiederanlauf: **beide DISABLED**. Jeder produktive Pfad einschließlich direkter CLI-Runner muss den Zustand vor Mainnet-SIGN/SEND prüfen. Ein GUI-only-Schalter genügt nicht. Bereits gesendete Transaktionen können durch DISABLE nicht rückgängig gemacht werden.

**Weitere Controls:** Vorhandene Source-/Component-/Candidate-Schalter wiederverwenden; fehlende fachlich getrennte Venue-, Runtime- und wirtschaftliche Freigabesteuerungen ohne neue Architektur technisch ergänzen. Alle wirtschaftlichen Controls haben Initialzustand `DISABLED/OFF`. Ein Wallet-ENABLE aktiviert niemals automatisch LIVE, Signieren oder Senden. Sicherheitsprüfungen für Chain, Kandidat/V5, Nonce, Budget, Anti-Replay und Providerqualität bleiben verbindlich.

**Ein Steuerungsweg:** Native Rust-Owner → typisierte, persistierte Konfiguration mit `requested/effective`, CAS-Generation, Receipt und Blockgrund → sanitisiertes `darkbot.bsc.v1` Protobuf → gemeinsames Gateway → bestehende BSC-Adapter/Store → fünf bestehende React-Reiter. Launcher und Buyer mit eigenen Enable-/Disable-Bedienelementen in `Wallets`; identische native Authority für Terminal und Frontend. F0-VAR und Native-/CLI-/Proto-/Gateway-/UI-E2E einschließlich Crash/Recovery nachweisen. Kein Project_0-Eingriff.

**IST 27.10.:** technisch umgesetzt und verdrahtet (`b72bf2ed` ff., Belege `STATUS.md`); alle Schalter DISABLED.

**Umsetzungsstatus:** Dieser Plantext ist eine SOLL-Vorgabe; er aktiviert nichts. Während der Umsetzung sind beide Wallets und sämtliche wirtschaftlichen Controls deaktiviert; kein echtes SIGN/ARM/SEND/BUY oder Providerkauf. Ausstehende G0.7/G0.8-Evidence blockiert nicht die technische Implementierung, bleibt aber für reale Qualifikationsaussagen offen.

---

## GELTENDE REGELN — KONSOLIDIERTE LESEREGEL (25.10.2026, NEUESTE NUTZERENTSCHEIDUNG)

**Bei widersprechenden älteren datierten Entwicklungsstopps gilt dieser neueste bindende Nachtrag.** Historische Abschnitte und damalige Code-/Teststatus bleiben als Historie erhalten. Das `bsc/docs/1_PLAN/GATE0_TESTPLAN_SHORT.md` bleibt fachliche Autorität für echte G0.7/G0.8-, Kosten-, Timing- und wirtschaftliche Sicherheitsnachweise; die hier ausdrücklich **aufgehobene Voraussetzung „erst real kaufen, dann Offline-Code schreiben“** darf nicht über einen alten Status oder einen untergeordneten Auftrag wieder eingeführt werden. Aktueller Code-/Abnahmestand steht in `bsc/docs/1_PLAN/STATUS.md`.

1. **Entwicklung ohne Kauf:** Gate A/B und BSC-050 zuerst; danach C0/C1 einschließlich C1.10, C2, anschließend **C3, D, F, G, H und I** in ihrer vorhandenen fachlichen Reihenfolge **vollständig offline implementieren, integriert testen und technisch abnehmen**, ohne auf einen echten Mainnet-BUY zu warten. **E** nur mit versions-/deployment-/ABI-geprüften, eingefrorenen Four-Classic-/OpenFour-Verträgen aus G0.1, niemals mit angenommenen Protokolldaten. C0–I dürfen keinen Mainnet-SEND bewirken. **J, KB und K behalten ihre eigenen echten produktiven/qualifikatorischen Gates und sind durch diese Freigabe nicht operativ freigegeben.** Für C3–I gilt dieselbe Beschränkung auf kostenlose, zulässige, technische Offline-/read-only-Operationen und Testnet-Qualifikation wie für C1/C2.
2. **REALER G0.7/G0.8-Erwerbsversuch separat:** Eine tatsächliche Mainnet-Transaktion, echte Mainnet-Key-SIGN/VERIFY-Schritte, ARM, Funding, Mainnet-Nonce-Burn, bezahlter Providerzugriff, Send, BUY und LIVE benötigen jeweils eine **gesonderte ausdrückliche Nutzerfreigabe**. Diese Phasenplanänderung erteilt keine davon. Standard `FROZEN_APPROVED_V5_SHA256="UNARMED"`, `approved=false`, `execution_ready=false`, kein Auto-Retry. G0.7→G0.8→kanonischer tatsächlicher Erwerb/Ordering/Kosten→PASS/ADAPT/STOP bleibt vor **operativer wirtschaftlicher Produktionsaktivierung** zwingend.
3. **Technische ENABLE/DISABLE-Pflicht:** Echte Rust-/Go-Owner und ausführbare/native Pfade, keine produktiven Mocks/Stub/Fakes/Platzhalter. Wo fachlich vorgesehen: echte `requested/effective`-Modi, validierte Konfiguration, durable Generations-/CAS-/Receipt-Semantik, Statuskette bis ins bestehende BSC-Frontend und Negativtests. Ein angefordertes, aber nicht wirtschaftlich genehmigtes `LIVE` bleibt nativ `effective=OFF/BLOCKED` mit begründetem Status; kein SIGN/SEND. Source-/Venue-Modi `OFF|SHADOW|LIVE` nicht mit Job-START/STOP oder globaler wirtschaftlicher Autorität verwechseln. Browser-Deaktivierung allein schützt nicht.
4. **G0.8 und physische Latenz getrennt:** Ein kanonischer, voll belegter Erwerbs-/Ordering-/Kosten-**PASS nach den unveränderten echten Verifier-Kriterien** darf trotz fehlendem Original-Producer-Zeitzeugen ausgegeben werden; `T_buy−T0 <1ms` bleibt **UNKNOWN/NOT_PROVEN**, nie als Sub-ms- oder Gesamt-LIVE-PASS erfinden. Fehlende Receipts oder unvollständige Capture = **UNRESOLVED**, nicht ADAPT/PASS oder „sicher nicht gesendet“. Vor J ist ohne qualifizierten Witness eine weitere **ausdrückliche Nutzer-Risikoentscheidung** nötig, gestützt auf wirklich gemessene vergleichbare Phase-I-SHADOW/PAPER-Platzierung gegen fremde BUYs, Ausfälle, Nenner und Kosten; bloßes Abnicken/Simulation ohne Marktwirklichkeit reicht nicht.
5. **Marktmessung:** G0.4/V10-8 read-only, `FREE_ONLY`, höchstens sieben Kalendertage; **vor Messbeginn** Source-/Venue-/Block-/Denominator, Mindeststichprobe und numerische WEITER/UMSTEUERN/STOPP-Schwellen versioniert dokumentieren, anschließend nicht ergebnisorientiert ändern. Sie darf schon **jetzt** parallel zu BSC-050/C1 starten, wenn sie G0.7 nicht verzögert. Ihr Ergebnis ist vor **operativem** C2-/J-Enablement erforderlich, **nicht** vor C2-Code-/Testabschluss. PRIVATE/UNKNOWN-Sichtbarkeitslücken ehrlich beziffern.
6. **Profil-/Bundle-Entscheidungen (technisch bindend, kein Geld-Approval):** BSC-008 **Option A**: bestehende über `execution::bundle_wire` gebundene `send_head+2`, `noMerge=true`, `positionFirst=true`, weiterhin **PROVIDER_UNQUALIFIED** für nicht real belegte Wirkungen. BSC-011 **Option B**: Chain-Ergebnis und Evidence-Integrität separat, Schema v6. BSC-049 **SELF_A** für die bestehende einzelne Candidate-Generation; SELF_B bleibt nur eigenes separat zu qualifizierendes Profil. Alle eingefrorenen Raw-Bytes/Hashes/Nonces/Gas-/Wert-/Budget-Grenzen unverändert; **keine** Freigabe eines konkreten V5-SHA oder Kaufversuchs.
7. **Review ohne Nutzerschleife:** „Every phase STOPs for independent review“ bedeutet ab jetzt nach jeweils **technisch vollständig abgeschlossenem** Phasengate **ein read-only unabhängiger Reviewer auf dem exakten finalen SHA**, mit einem zusammengefassten Gate aus tatsächlichen Release-/Test-/E2E-/Feld-/Safety-Belegen. HIGH/CRITICAL zuerst beheben und Gate wiederholen; nach bestandenem Gate **ohne neue Nutzerfrage** zur nächsten offline freigegebenen Phase. Kein Agentenschwarm, kein erneut ausgeführter Grundlagen-Audit als Ersatz für Implementierung. Nutzerfrage nur bei echter wirtschaftlicher Freigabe, fachlich offener wirtschaftlicher Entscheidung oder belegtem unauflösbarem technischen Blocker.
8. **Chapel-Testnet und Mainnet:** Read-only BSC-Mainnet-RPC/GetH-Observation ist erlaubt (`FREE_ONLY`); **Mainnet-geth für diese Offline-Kampagne empfangend, Outbound-Egress-Worker = 0**. **C1.7/C1.9b-Testnet-Sends auf Chapel, Chain-ID 97, sind ausschließlich mit dedizierten, vom Mainnet strikt getrennten wertlosen Testnet-Keys und Testnet-RPC erlaubt**, nach tatsächlicher Chain-ID-Prüfung und nur für die vorgesehenen Tests. Keine Mainnet-Keys, kein Chain-56-Senden, keine kostenpflichtigen Provider, kein Mainnet-Key-Rollover. Testnet-Transaktionen sind keine Mainnet-/Erwerbs-/T0-Evidence und keine LIVE-Freigabe.
9. **Definition „TECHNISCH FERTIG“ (für jedes Ticket/jede Phase):** (a) jede heute technisch realisierbare bestätigte Anforderung im **produktiven** Native-Owner vorhanden, ohne Mock/Stub/Legacy-/JSON-Ersatz oder toten Caller; (b) sämtliche relevanten echten Caller, Safety-Pfade und fachlichen Interfaces integriert; (c) reale Binary-/Gateway-/Protobuf-/Frontend-E2E-Regression inklusive bestehender F0-VAR-Zeile, soweit öffentlich; (d) risikogerechte positive, negative, Crash-/Restart-/Recovery-/Reorg-/UNKNOWN-/Idempotenztests; (e) tatsächlich betroffene `fmt`, `clippy -D warnings`, Go/Rust-Tests, echte Release-/Proto-/Gateway-/Frontend-Builds und Regression auf dem gepushten, abgeglichenen Commit erfolgreich (genehmigte, exakt dokumentierte Freeze-Hash-Ausnahmen nur mit gleichwertigem nachprüfbarem Formatierungsbeleg); (f) Evidence, ausführbare Testbefehle/Exitcodes, Commit-SHA und aktualisierter ehrlicher Status auf `origin/main`. Fehlt ausschließlich reale autorisierungspflichtige Mainnet-/Provider-/Producer-Evidence: **`TECHNISCH FERTIG — EXTERNAL_BLOCKED: ...`** mit exakter Liste; ansonsten **OFFEN** mit konkreten fehlenden Implementierungs-/Testpunkten. `LOCAL_PASS` ist niemals allein eine Fertigmeldung; keine erfundenen Gesamt-PASS-Ergebnisse oder „weitgehend“.
10. **Umsetzung ohne Wiederholung:** Aktuelle Main-Commits/Source-Hashes mit phasenrichtigen Tickets abgleichen, BSC-050 **einmalig** auf alle aktiven BSC- und tatsächlichen geteilten Proto-/Gateway-/Frontend-Caller ausführen, dann Code statt Dokumentschleifen liefern. C1-C2 und danach C3/D/F/G/H/I inkl. notwendiger UI-Integration vollständig entwickeln. Kein zweiter BSC-Runtime-Stack, kein internes JSON außer zwingendem externem JSON-RPC am Wire-Rand, keine vorgetäuschte LIVE-Fähigkeit. Project_0-/Solana-Owner und eingefrorene signierte Evidence nicht eigenmächtig ändern.

### BINDENDE G0.7-VORBEREITUNGS-/BETRIEBSREGELN (2026-10-25; KEINE FREIGABE)

- **Remote-Attempt-Authority BSC-005:** Der tatsächlich implementierte Ref-Prefix ist `refs/darkbot/g07-attempt-authority/<attempt_id>` auf `origin` (nicht `refs/darkbot/attempt-authority/`). Grant = separate Nutzeraktion nach exakter V5-Genehmigung, CAS im Runner **nach Preflight, vor lokalem Latch**, anschließend frischer Head und Release. Nicht erreichbare/unklare Authority ⇒ kein Release.
- **BINDENDE T5-KONFORME CAS-/RELEASE-GRENZE (KORREKTUR 25.10.2026):** Erfolgreicher Remote-CAS setzt die Authority zunächst auf CONSUMED. Ausschließlich ein kontrollierter, beweisbar bytefreier PRE_EGRESS_ABORT **vor** der Release-Barriere darf denselben Claim per Remote-CAS CONSUMED→AUTHORIZED zurückgeben, wenn der durable RELEASED-Marker fehlt, Latch-/Journal-Identität stimmen und kein Egress möglich war. Nach der Barriere, bei Crash/Kill, UNKNOWN, unklarem/fehlgeschlagenem Latch/Remote, oder mit fremdem/veraltetem Claim bleibt CONSUMED; kein automatischer Retry. Ein zurückgegebener Claim darf keinen gültigen Guard-Proof mehr liefern; Neustart nur nach bewusster Operatorentscheidung und frischer Pflichtprüfung, keine zweite Grant-Erteilung. `attempt_authority.rs::Claim::return_unsent` und `g07_mainnet_self_launch_buy/main.rs::release_unsent` sind **nicht pauschal zu entfernen**. **Offen: der Compiler erzwingt die Pre-Release-Grenze noch nicht mittels konsumierbarem `Unsent`-/Barrier-Typ**; diese Abgrenzung samt Tests für jede Abbruchstufe/Journalstatus, RELEASED-Marker, Kill zwischen CAS und Barriere, stale/returned Claim und Remote-CAS fail-closed vor wirtschaftlicher Autorisierung implementieren. CODE_GAP/OPEN nur für diese Typ-/Regression-Absicherung, bisherige geprüfte Remote-CAS-Funktion bleibt LOCAL_PASS/REMOTE_QUALIFIED in ihrem Scope. Keine SIGN/ARM/GRANT/SEND-Freigabe durch diesen Text.
- **Provider/Archive:** Für eine irgendwann separat genehmigte SIGN-/H2-Requalifikation müssen aktuelle gebundene und kanonische R0/H2-States verfügbar sein; G0.8 ausschließlich mit **aktuell** per `--qualify-history` qualifiziertem Archiv-RPC (Blast war am 25.10. qualifiziert, diese historische Feststellung ist kein ewiger Provider-PASS). Kein ungeprüfter Dataseed-Fallback.
- **Zeitfenster/Versuch:** Quellzeit der ältesten gebundenen Builder-Policy + maximal 24 Stunden für SIGN→V5-Vorschlag→Readiness→gesondertes V5-Approval→Arming→Grant→Versuch. Bei Frist-/Chain-/Wallet-/Gas-/Nonce-/Receipt-/Hash-/Budget-Unklarheit fail-closed. Vorbereiteter Disarm darf **nie** vor Nutzergenehmigung/Versuchsende produktiv gepusht werden. Jeder spätere echte Versuch ist wirtschaftlich separat freizugeben; kein Auto-Retry.
- **Nur Runbook und Artefakte vorbereiten, nicht ausführen:** In `bsc/docs/1_PLAN/STATUS.md` ein verantwortbares Operator-Runbook mit Schritten **Funding (separat autorisiert) → Key-SIGN (separat autorisiert, Nutzerterminal) → V5-Vorschlag → echte PASS_READ_ONLY Readiness → Nutzerprüfung der exakten gebundenen SHA/Bytes/Budget/Nonces/Builder-Werte → Arming → Grant → genau ein Versuch → Disarm nach Runner-Ende → G0.8/Archiv** und je Schritt konkret verifizierbare Soll-Ausgabe/Abbruchbedingungen dokumentieren. **Disarm-Patch getrennt lokal vorbereitet, nicht pushen/armen**; vom Operator vor wirtschaftlicher Aktion kontrollieren. V5-Review-Checkliste: Candidate/ID/Token, signierte Launch-/BUY-Hashes, Nonces (nachgewiesene Istwerte; nicht blind 0/0), Gasprice/Limits, BUY-Input, gemeinsames Maximum **481163500000000 wei**, `send_head+2`, 48Club-Flags, 15-s-Builder-Frist, Policy-Freshness und Attempt-Authority. Test **fehlende Canonical-Receipts/kein Inclusion-Beweis ⇒ G0.8 UNRESOLVED, Frontend identisch** am echten Native-/Gateway-/UI-Rundweg nachweisen. Keine echte Geld-Operation ist aus dieser Anweisung genehmigt.

---

## BINDING USER DECISION — 2026-10-25: C0/C1/C2 OFFLINE-ENTWICKLUNG FREIGEGEBEN; WIRTSCHAFTLICHE AUSFÜHRUNG WEITER GESPERRT

**Neue ausdrückliche Nutzerentscheidung mit Vorrang vor allen älteren widersprechenden C1/C2-Entwicklungsstopps in diesem Dokument und untergeordneten STATUS-/Bug-/Auftragsdokumenten.** Ab sofort dürfen die vorhandenen BSC-Phasen **C0 (soweit nötig), C1 einschließlich C1.9/C1.9b/C1.10 und C2** nach ihrer technischen Abhängigkeitsreihenfolge vollständig **offline entwickelt, repariert, nativ integriert, lokal/reproduzierbar getestet, dokumentiert und für ihren nachweisbaren technischen Scope eingefroren** werden, **ohne auf G0.7/G0.8-Mainnet-Ergebnisse zu warten**. Zuvor bestätigte Gate-A-/Gate-B-Code- und Integrationslücken zuerst schließen; **BSC-050-Vollinventur vor der umfassenden C1/C2-Fortsetzung** durchführen. C2 respektiert die fachlichen C1-Interface-/Freeze-Abhängigkeiten; alte Statusangaben „C1 PAUSED“ bzw. „C2 BLOCKED“ aufgrund des ausstehenden Mainnet-BUY gelten für die *Offline-Entwicklung* ab dieser Entscheidung nicht mehr. Bereits verifizierte Arbeit nicht neu bauen. Bestehende C0–C2-Scope-, Golden-, Race-, Release-, Wire-, Safety- und Frontend-Abnahmekriterien bleiben unverändert verbindlich.

**Absolute wirtschaftliche Grenze unverändert:** Diese Entscheidung genehmigt **weder echte Wallet-Key-SIGN/VERIFY-Läufe noch ARM, Funding, echte Nonce-Burns, bezahlte Provideraktionen, Transaktions-Broadcast/SEND, Mainnet-BUY, LIVE oder automatische Retries**. Kein Geldtransfer und keine on-chain Transaktion im Rahmen dieses Auftrags. `FROZEN_APPROVED_V5_SHA256="UNARMED"`, `approved=false`, `execution_ready=false` bleiben. Alle solchen Schritte bedürfen weiterhin ihrer jeweils **separaten ausdrücklichen Nutzerfreigabe**. Read-only öffentliche RPC-Abfragen und lokale/synthetische Negativtests dürfen nicht als echte Kauf-/Producer-Evidence gelten.

**G0.7/G0.8 als reale Qualifikationsgrenze unverändert:** Die Abfolge **separat genehmigter echter G0.7-BUY → G0.8 kanonischer Erwerbs-/Reihenfolge-/Kostennachweis → PASS/ADAPT/STOP** bleibt für **wirtschaftliche Freischaltung, Produktionsaktivierung/LIVE und Behauptungen über tatsächlichen Erwerb oder physische T0→T_buy-Latenz** verpflichtend. Ein technisch grünes C1.10/C2-Offline-Gate ist **kein** produktiver Gate-0-PASS. Fehlender Original-Producer-Witness bleibt `UNKNOWN/UNQUALIFIED`, nicht `PASS`. Nachweislich noch nicht extern prüfbare Teilkriterien getrennt als `EXTERNAL_BLOCKED/NOT_RUN` führen, statt sie zu fingieren oder sämtliche unabhängig machbaren Codearbeiten zu stoppen.

**Reihenfolge für Claude:** (1) offene ausführbare Gate-A-/Gate-B-Code-/Frontend-/Schema-Integrationen, (2) BSC-050-Vollinventur mit konkreten aktiven Fehlern, (3) C1 samt vorhandenem Freeze-/geth-/HOT0-Regression-Gate technisch fertigstellen, (4) C2 mit echtem nativen State-/Nonce-/Reorg-/Recovery-Owner samt erforderlicher Integration technisch fertigstellen, (5) danach C3, D, F, G, H und I nach ihren Fachabhängigkeiten offline gemäß „GELTENDE REGELN“ Nr. 1 (Erweiterung vom 25.10.); E erst nach dem G0.1-Four-Vertragsfreeze; **J, KB und K bleiben gesperrt**. Kein produktiver Mock, Fake, Legacy-Fallback, internes JSON, unnötiges Hardcoding oder paralleler Runtime-Pfad; Test-Fixtures sind keine produktiven Ersatzimplementierungen. Pro Paket tatsächliche RED/GREEN-/Integrations-/Release-/Frontend-Nachweise, dokumentierten sachlichen Status und Commit/Push auf `origin/main`. **`LOCAL_PASS` allein bedeutet weder betriebsbereit noch vollständige reale Qualifikation.** Untergeordnete Dokumente sind auf diese neue Prioritätsregel anzupassen; historische Evidenz und signierte/gebundene Bytes unverändert lassen.


### BINDING 2026-10-25 — G0.8-TIMING-UNKNOWN, MARKTMESSUNG VOR OPERATIVEM C2 UND ECHTE ENABLE/DISABLE-SEMANTIK

**Regel 1 — G0.8-Erwerbsverdikt unabhängig vom Producer-Zeitzeugen:** Ein fehlender originaler Producer-/Builder-/Validator-Witness für `T_buy − T0` verhindert **nicht** das eigenständige G0.8-Verdikt zum **tatsächlichen kanonischen Token-Erwerb, zur Transaktionsreihenfolge, zum Wettbewerb und zu den vollständigen Kosten**. Bei erfüllten realen G0.8-Erwerbskriterien darf genau dieser *Erwerbs-/Ordering-/Kosten-Scope* `PASS` erhalten, auch wenn die **physische** Zeitdifferenz mangels Witness separat `UNKNOWN/NOT_PROVEN` bleibt. **Niemals** einen allgemeinen Sub-1-ms-, Wettbewerbs- oder Gesamtproduktions-PASS aus N/N+1, Builder-ACK, RPC-Blockzeit, lokalen Zeitstempeln oder einer einmaligen Self-Launch-Platzierung ableiten. Bei unvollständigen kanonischen Receipts/Capture-/Acquisition-Beweisen bleibt G0.8 `UNRESOLVED`, nicht `ADAPT`, `PASS` oder „sicher nicht gesendet“; ein beobachteter nicht aufgenommener Bundle-Versuch ist **keine** neue Sende-/Retry-Freigabe. `chain_outcome` und `evidence_integrity` (Schema v6) bleiben getrennt. G0.11 darf über den **nachgewiesenen engen Gate-0-Erwerbsscope** entscheiden; die physische Timing-Anforderung bleibt ausdrücklich offen.

**Späteres J/LIVE trotz `T_buy−T0=UNKNOWN` nur über eine qualifizierte Original-Witness-Quelle ODER eine gesonderte, dokumentierte ausdrückliche Nutzer-Risikoentscheidung.** Die alternative Risikoentscheidung muss **vor** jeder J/LIVE-Freigabe auf echten, nachweisbar vergleichbaren PAPER-/SHADOW-Marktdaten aus Phase I beruhen: Anteil eigener simuliert/technisch korrekt platzierbarer BUYs an der frühesten zulässigen Position nach fremden Launches, Abstand zum ersten tatsächlich erfolgreichen fremden BUY, Stichprobe/Denominator, Ausfälle, Gebühren, Venue-/Builder-Kontext und Messgrenzen. Reine lokale Sink-Zeiten oder Self-Launch-Ergebnisse genügen nicht. Fehlt dieser belastbare Wettbewerbsnachweis, **keine UNKNOWN-Risiko-Ausnahme für J**. Auch bei ausdrücklicher Risikoannahme bleibt der physische `<1 ms`-Nachweis `UNKNOWN`, nicht `PASS`; separate LIVE-/Budget-/Sicherheitsautoritäten werden dadurch nicht ersetzt. Diese Ausnahme ist **keine aktuelle LIVE-Genehmigung**.

**Regel 2 — begrenzte G0.4/V10-8-Marktmessung vor operativer C2-/J-Freischaltung:** Nach einem **realen G0.8-Erwerbs-PASS** ist vor der **operativen Freischaltung der C2-basierten Produktionsverarbeitung** eine getrennte **rein lesende** Marktevidenz erforderlich: fremde Launches pro Tag, Anteil vor dem relevanten Block mit vollständig verifizierbarer/actionable Transaktion sichtbar, Anteil geeigneter Venue-/Token-Profile und beobachtete Konkurrenz/Platzierung. **Vor** Messbeginn Source-/Chain-/Venue-Scope, Denominator, Zählweise, Messdauer (Standard: maximal **7 Kalendertage**), Mindeststichprobe sowie numerische **WEITER/UMSTEUERN/STOPP**-Schwellen im existierenden Evidence-/Planregister festlegen; nicht anhand der Ergebnisse nachträglich optimieren. Private, nicht sichtbare Launches und nicht identifizierbare Pakete bleiben als Erfassungsgrenze `UNKNOWN`. Nach Ablauf der Frist tatsächliche Ergebnisse und mögliche Nichtqualifikation dokumentieren; keine endlose Verlängerung oder erfundene PASS-Quote. Messungen dürfen **bereits vor G0.7 read-only und parallel** begonnen werden, sofern sie dessen Vorbereitung nicht verzögern, keine bezahlten Dienste/Nutzeraktionen auslösen und keine wirtschaftliche Ausführung eröffnen. G0.4 kann den C1.10-Offline-Freeze begleiten.

**WICHTIGE TRENNUNG:** Diese Marktmessung ist **keine Sperre für die am 2026-10-25 bereits ausdrücklich freigegebene technische Offline-Entwicklung, C1.10-Abnahme und vollständige C2-Codeimplementierung samt Tests**. Gesperrt sind bis zu ihren jeweiligen Gates allein **wirtschaftliche Ausführung, produktive Aktivierung sowie unbelegte Erfolgsbehauptungen**. Ein `ADAPT/STOP/UNRESOLVED` aus G0.8 verbietet keine unabhängig erlaubten Offline-Codefixes, erteilt aber keine operative C2-/J-Freigabe.

**ENABLE/DISABLE ist echte Software, kein Ersatzcode:** Alle planmäßig vorgesehenen Source-/Venue-/Adapter-/Runtime-Modi und berechtigten Controls müssen im zuständigen Fachpaket mit **einem realen nativen Rust-/Go-Owner**, typisierter validierter Konfiguration, dauerhaftem `requested/effective`-Zustand, Generation/CAS/Receipt, korrektem binärem Status→Protobuf→Gateway→bestehendem BSC-Frontend und Negativ-/Wiederanlauf-/Sicherheitstests implementiert werden. `OFF`/`SHADOW`/`LIVE` sind nur für die fachlich definierten Source-/Venue-Modi gültig; Jobs haben ihre eigenen START/STOP-Zustände. Ein nicht freigegebener Modus darf **als reale implementierte Funktion** angezeigt werden, aber der **Native-Guard hält `effective=OFF` bzw. `BLOCKED/DISABLED` mit echtem `block_reason`**, auch bei per API/CLI erzwungenem `requested=LIVE`. Nie einen funktionslosen GUI-Schalter, Mock-/Stub-Controller, statischen Fake-Status oder rein optische Sperre als „implementiert“ verkaufen. Bisher nicht genehmigte wirtschaftliche SIGN/ARM/BUY/SEND-Aktionen bleiben **ohne aktivierbaren UI-/API-Befehl**; eine zukünftige Freigabe muss ausdrücklich das entsprechende native Safety-/Wire-Gate eröffnen. Auch `SHADOW` darf keinen realen BUY senden. Pflichtketten-/Reorg-/Safety-Dienste werden durch das Abschalten einer Signalquelle nicht abgeschaltet.

**Prüf-/Abnahmepflicht:** Mit fehlender LIVE-Freigabe muss der reale produktive Aufrufpfad eine Aktivierungsanforderung fail-closed ablehnen, einen unveränderten effektiven Zustand und einen begründeten Status/Receipt ausgeben und **nachweislich null Sign-/Send-/Wirtschafts-Egress** erzeugen; ohne Freigabe keine Test-Fake-Aktivierung. `CODE_FIXED`, `LOCAL_PASS`, `INTEGRATION_VERIFIED`, `EXTERNAL_BLOCKED/NOT_RUN` und echte produktive Qualifikation sind strikt getrennt. Fehlende externe Evidence ist kein Ersatz für fehlerfreie vollständig verdrahtete Software.


---


## BINDING v9.1 D1–D6 — G0.6a DRY secret isolation and PREPARE → SIGN/VERIFY split (2026-10-23)

**Historischer D1–D6-Planstatus vom 2026-10-23: PLANNED / CODE NOT YET IMPLEMENTED; keine aktuelle HEAD-Statusaussage.** Spätere Implementierungscommits sind pro D1–D6 gegen den aktuellen Code und reale Tests zu prüfen. CODE vorhanden ist nicht gleich E2E/SIGN freigegeben. This is a narrowly scoped implementation package under the existing v8/v9/v9.1 Gate-0 plan, not a replacement roadmap, new engine, second signing algorithm, new UI/control contract or permission to run a genuine signing step. Implement/test the harness changes only with synthetic candidates and test keys until an explicit separate decision. The authoritative current code remains the existing \`bsc/tools/g06a_final_dry.sh\`, \`bsc/tools/g06a_shell_scope.sh\`, native candidate store and current G0.6a DRY tools. Existing V5 guard and DRY5/DRY6 rules remain authoritative. **No real-key signing, arming, funding, nonce burn, builder submission or Mainnet send is authorized by D1–D6.**

**Verified trigger:** The existing shell script sources \`bsc/secrets.env\` and requires both wallet private keys before R0, although only its wallet-bound \`g06a_local_simulate.cjs\` (step 4) and \`g06a_self_launch_bundle\` (step 5) need signing keys. Its \`run()\` subprocess environment allowlist currently includes \`DARKBOT_G06A_SALT\`, so any ordinary \`run\` child can inherit B's otherwise private salt. The key scoping in \`with_keys()\` and the private \`g06a_economics_evm_crosscheck.cjs --private\`/RPC egress guard already exist; **do not reimplement** them. Step 4b (\`g06a_launch_path_trace.cjs\`) is key-free and depends on the preflight and candidate salt, not on step 4's signed simulation. No actual wallet-key leak has been established. This is a process-exposure/safe-operations improvement; it does not improve HOT0 latency or qualify G0.7.

### D1–D6 implementation contract and acceptance

| ID | Required change | Mandatory acceptance |
|---|---|---|
| **D1 — Secret-scoped children (B prerequisite)** | Remove \`DARKBOT_G06A_SALT\` from the general \`allowed_environment()\` allowlist; preserve \`run()\` as strictly nonsecret. Provide a narrowly scoped existing-tool handoff for the selected candidate's secret salt, with a verified candidate ID, profile, generation, deployment/preflight binding and commitment. Give the value only to the exact local EVM consumers that require it; signing-key export stays confined to designated signing/simulation children. Use existing local private candidate storage and existing \`--private\` egress guard; no ordinary subprocess, CLI argument, browser, reporter, record view, stdout/stderr or external RPC may receive B's salt. | Negative process-environment, argv, log/evidence and mock-RPC tests. A wrong/missing/stale candidate or commitment must STOP; no automatic re-roll/fallback to A. Demonstrate that B does not call remote \`eth_call\`/\`eth_estimateGas\` with private launch calldata. Must PASS **before the first real B-DRY**, irrespective of the rest of the split. |
| **D2 — PREPARE (key-free)** | Add a mode to the **existing** DRY harness that requires no \`secrets.env\` or wallet keys, and never invokes step 4 or 5. Run the existing R0, DRY-0/preflight, Quote, current-block Economics EVM cross-check, builder-policy observation and key-free step 4b Launch-Path at H1; A uses its existing public/frozen salt, B only its scoped private candidate. Do not merely terminate after step 4b, since step 4 currently occurs first. RPC configuration must be available without loading wallet keys; credentialed RPC details must not enter process argv/logs. | PREPARE with missing \`secrets.env\` PASS given valid non-key RPC config and candidate; a present secrets file is **not read**; tools receive no signing keys; outputs are clearly \`PREPARE_ONLY / NOT_REQUALIFIED\`, with no misleading DRY PASS, signed bytes or V5 readiness/approval. |
| **D3 — H1→H2 limited handoff** | Transfer only (a) the **validated** R0 scan and its provenance/identity and (b) the unchanged candidate reference/ID/generation (for B, never transfer the salt value). Keep H1 diagnostic records immutable and separate. Verify manifest hashes **and** typed DBREC schema, trusted frozen starting block/hash, exact contiguous header/parent/receipts-root coverage, final anchor and candidate/deployment identity; hashes alone do not establish semantic validity. Extend R0 using the already supported \`--upgrade-scan\`/DRY-0 closure at H2. Store SIGN output in a new private run directory; never edit PREPARE or frozen evidence in place. | Modified/incomplete/missing/substituted scan, wrong candidate/generation, stale/deployment mismatch or noncanonical/reorged endpoints all STOP. No repeated full R0 scan if verified coverage can safely be extended; no implicit H1 economic-result reuse. |
| **D4 — Recompute at H2 before signing** | After R0 extension, compute fresh H2 preflight, local quote, independent local EVM economics, builder policy and Launch-Path against the same correctly pinned H2 state and candidate. Do **not** declare H1 economics valid just because Portal version/code, IDs or basis-point fields match: global curve/fee/tax state can change without a code change or version bump. Only after all mandatory, coherent H2 gates pass may the *separately authorized* key-dependent exact-wallet simulation and DRY1–DRY4 run. Keep DRY5 Launch-Path and B-only DRY6 bindings in the current verifier/V5 chain. On any mismatch, STOP; do not silently refresh a signed candidate or reuse stale signatures. | H2 state/curve change with unchanged superficial drift fields is detected by fresh simulation; altered block hash, quote/output, gas, nonce, wallet, token, launch path, profile, budget or signed identity STOP. Every record has an unambiguous source state and linked candidate; no SIGN with only an H1 preflight. |
| **D5 — A/B exact parity** | Keep existing A historical DBREC/Goldens, deterministic public A salt, signed transaction algorithms and frozen bytes immutable. B uses one persistent CSPRNG candidate, private salt only through D1's scoped handoff, private economics schema, own local simulation/signed hashes, DRY6 and its own V5 candidate binding. Neither A's frozen qualification/approval nor B's private values transfer across profiles. Do not create B or sign using real wallets as part of this coding package. | A regression/byte/hash tests unchanged; B with synthetic candidate and keys exercises the same evidence and approval barriers, rejects A hashes/native economics record/wrong commitment, and has no premature signing or egress. |
| **D6 — Fail-closed regression and evidence** | Test the real orchestration boundaries, not only isolated helper functions. Cover no-keys PREPARE, secrets-file-not-read, process/argv/RPC/log confidentiality, R0 replay/partial scan/reorg, H2 economics drift, A/B mismatch, private output directory permissions and forbidden executable raw-transaction output. Use synthetic wallets and mock RPC, preserve all original frozen evidence and signed A bytes. Document exact test commands, PASS/FAIL, fixtures, hashes and limitations. | No claim of operational E2E PASS from unit tests alone. Confirm the full DRY binary still reports \`network_write_capability=NONE_IN_THIS_BINARY\`; no signed raw RLP/bundles or B salt in potentially Git-trackable \`bsc/native/evidence/g06a/\` or any untracked output destined for publication. Preserve the protocol/front-end goldens and re-pin approved source manifests only when required. |

**Execution order and scope:** Commit the plan first. Implement and test **D1 before any B DRY**; follow with D2→D3→D4→D5→D6 in small commits. A's existing workflow need not be blocked for a speculative redesign, but all real-key signing still requires its **own explicit user authorization**. For this package, no real keys may be accessed or any real-key DRY-1 executed. Existing \`g06a_final_dry.sh\` sequencing, data contracts and V5 validation must remain functionally compatible. Any genuinely unavoidable DBREC schema/record/hash change requires a justified versioned change, golden parity and independently documented requalification; never overwrite historical artifacts.

**Three separate authorities (never combine):** \`PREPARE\` = key-free evidence preview, **not** full qualification; \`SIGN/VERIFY\` = locally signed qualification **only if separately approved for real keys** and **never** Mainnet egress authority; \`LIVE\` = distinct exact-candidate/contract/attempt economic authorization and guarded submission. The current state remains \`FROZEN_APPROVED_V5_SHA256="UNARMED"\`, \`approved=false\`, \`execution_ready=false\`, \`LIVE_SEND=NONE\`, G0.7/G0.8 not executed and physical \`T_buy−T0 < 1 ms\` unverified. **No changes to React, Graphite, frontend phase plans, protobuf/gateway contracts or C1/C2/FOREIGN_AUTO LIVE.**

---

## BINDING v9.1 IMPLEMENTATION/HARDENING AMENDMENT — 2026-10-23: verified findings and executable order

**Authority and scope:** This narrow implementation addendum supplements v8/v9 and the original Gate-0 freezes. It is NOT the rejected Fable v10 replacement, and permits neither a second trading engine nor a second economic attempt. The previously authorized offline/shadow A/B frontend exception remains valid. No C1/C2, foreign AUTO LIVE, broader GUI work, arming, nonce burn, funding, signing or sending is authorized.

**Hard target unchanged:** A real successful token acquisition and canonical first-eligible BUY ordering are distinct from the primary original-producer physical target **T_buy - T0 < 1,000,000 ns**, with conservative witnessed uncertainty. N/N+1, receipts, local reaction timing, adapter handoff, builder ACK, head-relative sensor timing and EVM gas amount are not proof of sub-ms producer execution. Without an original qualified producer/builder/validator witness, the physical result is **UNKNOWN**, not PASS; EXECUTION_GAP_RISK remains **UNVERIFIED** without a guessed microsecond value. G0.7 is ONE separately approved economic smoke attempt; G0.8 can establish acquisition, canonical ordering and actual cost but cannot invent producer timing. A one-attempt success rate does not qualify future product probabilities. The GATE0_TESTPLAN_SHORT.md top stop rule is not relaxed.

### 0. Existing checkpoint — continue, do not reimplement

As of commits 45280e2b and f1d6e630 (2026-10-23), native offline g07_candidate creation/selection SELF_A and SELF_B and generation-CAS storage are committed; B-salt storage is local/private, and the GUI selector is visible but DISABLED with BACKEND_UNAVAILABLE because no authoritative typed gateway/status path exists. Native readiness checks selected candidate against the V4-bound A identity. This is not end-to-end GUI completion or B economic qualification. The G0.7 runner still takes DARKBOT_G07_SALT from environment and sends launch calldata to an external eth_estimateGas RPC. Historical A evidence is frozen, not requalified against current Portal v5.26; the v5.26 drift remains STOP. Compiled guard is UNARMED, approval/execution_ready are false, and LIVE_SEND=NONE.

### 1. Implementation packages — offline only; ordered with explicit acceptance tests

| Order | ID | Required improvement | Acceptance and boundaries |
|---|---|---|---|
| A1 | AB-01 | Complete the isolated **offline-only** A/B frontend-to-native selection/status path. Introduce only an explicitly typed, narrowly allowlisted generation-CAS candidate command and native-authoritative binary status projection. Show selected profile, candidate ID, predicted token, binding hashes and actual qualification/readiness. | Test CLI, gateway, reporter and GUI with stale generation, unknown profile, no backend, armed/attempt state, missing/mismatched candidate, and actual requested/effective state. No arbitrary argv/env/paths, generic shell, wallet access or G0.7 runner launch through the gateway. Keep controls disabled until supported. No salt/key in GUI/status. Existing four gateway capture/readiness commands must remain bounded. |
| A2 | AB-02 | Keep immutable A evidence intact; independently qualify chosen A or B against current Portal using the existing DRY, economics, state, exact signed identity and approval paths. Candidate B needs its own exact token, calldata, chain/wallet/nonces, gas, signed hashes, matching V4 or separately approved successor. | No reusing A's frozen DRY/V4/approval as authority for B. B creation is not B qualification. Selection invalidates stale candidate approval/readiness but never authorizes or consumes a paid attempt. Do not re-run long historic R0 without an actual invalidation. |
| B1 | A2-guard | Strengthen pre-egress V4 guard binding for local_simulation, economics and portal_upgrade records. The read-only readiness already checks the three; the live guard must independently load the exact V4-authorized paths, verify record bytes, SHA-256, schema and identity, and prevent swap/TOCTOU between verification and use. Prefer existing bound V4 paths over additional arbitrary env inputs. | Isolated negative tests for each missing/mutated/swapped/mismatched record. Preserve DRY, builder, witness-source, attempt-ID, budget and anti-replay checks. Never arm production or write an approved V4 simply to test an unreachable live guard branch. |
| B2 | B-salt | Audit every RPC, eth_call, eth_estimateGas, trace, ENV, stdout/stderr, evidence and report path containing launch calldata. For secret profile B, reproduce the required estimateGas/return-token/economics checks using a qualified local EVM state before any separately authorized builder release. B's selected binding must be authoritative in the runner, not a fallback through A's DARKBOT_G07_SALT. Keep historical A behavior unless correction is proven needed. | Fake external RPC/network and negative log/status/evidence tests demonstrate no pre-release B-salt disclosure. Local-simulation failures STOP. Builder submission necessarily reveals calldata to the authorized builder; do not promise secrecy after that point. |
| B3 | C2/R3 | Use EIP-1898 block-hash pinning across the ten preflight state reads (getCode/call/storage/balance); keep pending nonce reads separate, and validate nonce, canonical state and freshness at the intended boundary. Error on unsupported hash parameters or reorg. | Mock/fixture counts all pinned state reads; tests for different load-balanced heads and noncanonical/unsupported hash responses; read-only capability check on real RPC. Do not silently use unpinned latest as fallback. |
| B4 | C4 | Stop placing a credential-bearing full RPC URL on curl process argv. Pass URL/config via stdin or equivalently secured transport, sanitize endpoint/secrets from errors, panics, journals and status. | argv inspection and induced-error regression. Preserve bounded timeout and required external JSON-RPC boundary. |
| B5 | A1-guard | After B1, replace the three literal true claims (dry_pass_all, capture_prearmed, producer_witness_contract_bound) with internal evidence-backed proof capabilities where feasible. A bound witness-source contract is NOT a real producer-timing witness. | Negative/compile-time proofs that a skipped validation cannot create guard authority. No guard weakening or economic changes. |

**Implementation order:** Complete A1/A2 as the continuation of the existing A/B commits. Implement and test B1–B4 before any future arming decision (B2 is a mandatory B-specific prerequisite); B5 follows B1. Small isolated commits; full list of source changes, exact tests run and remaining UNKNOWNs with each package. Reuse existing Rust engine and DBREC schemas; no unnecessary duplicate implementations. Any deliberate schema/hash/signature change needs a new explicitly named requalification, never overwrite the frozen A artifacts.

### 2. Qualification / conditional fixes — prove a gap before changing code

| ID | Verify or fix only as justified | Gate |
|---|---|---|
| C1/R1 | Offline prove whether the already-hashed Portal implementation and its immutable references cover every relevant newTokenV6 helper/deployer. Portal v5.26 changed the implementation slot, so the claim that only version() detected it is false. Add pinned helper/launch-path hashes only if a helper-only change can bypass the existing check; then requalify schema/Golden evidence. | Proof first; no speculative tracing system. |
| R2/T2 | Validate actual simulated newTokenV6 return address against CREATE2 predicted_token on current qualified state. A may use a qualified read-only RPC; B must use local EVM to avoid disclosing the private launch salt. This check does not rule out future drift. | No external B-salt eth_call. |
| C10/R5 | Measure the token-code existence race from first read to release. A second check before next_head may be one block stale; after the head costs RTT. Implement only the least risky evidenced check with fail-closed negative tests. | No speculative RTT on head-to-release path. |
| C3/R4 | Preserve historical v2 PROPOSAL_MAX_RESERVE_WEI meaning and distinguish the current exact V4 maximum 481163500000000 wei. Deduplicate identical V4-value literals only if useful; do not force B to share A's gas/signature identity or add record fields unnecessarily. | Low priority; record changes require requalification. |
| B1/provider | For 48Club and BlockRazor independently qualify second-TX revert behavior, bundle atomicity, expiry, noMerge/positionFirst, pricing, actual proposer coverage and response status. Missing revertingTxHashes alone does NOT prove all-or-nothing behavior. Builder ACK is not canonical inclusion. | Remains UNQUALIFIED absent provider-specific documented or measured evidence. Qualifying an economic outcome is not authorized by this amendment. |
| R6 | Read-only qualified proposer/builder two-block-window coverage, with independent evidence for any validator-to-builder mapping. No invented acceptance percentage. | Measurement first. |
| G0.8 | Keep the recently corrected canonical first external successful BUY ordering by transaction index then log index; regression-test same/different block, competing/failed BUYs, missing receipts, reorg, acquisition token delta, realized debit/cost and unresolved/one-attempt handling. | Ordering alone never yields producer timing PASS. |
| T5/V10-6 | Signed legacy transactions may remain valid after builder bundle expiry. Any possible byte egress counts against the single authorized attempt until an explicit user decision. A separately authorized nonce-consuming transaction can invalidate the old exact nonce on the canonical chain but cannot reset authorization. | NO retry, wallet rotation or nonce burn is authorized here. |
| G0.4/V10-8 | Read-only per-source measurements distinguish hash-only sightings from full verified signed and actionable launch transactions, local head-relative visibility from T0 and public vs private misses. The Oct22 48/116 census was hash/head-relative, not full-TX actionability and not a measured T0 rate. | Foreign AUTO LIVE remains PLANNED, C1/C2 paused. |
| C7 | Measure only with compatible clock domains and documented synchronization/uncertainty. CaptureClock monotonic-relative nanoseconds cannot be directly subtracted from Unix/BEP-520 epoch milliseconds. Journal timing is not an original producing execution witness. | No manufactured sub-ms measurement. |
| Transport | Later low-priority diagnostics: tolerant chunked transfer header parsing, original socket error preservation, explicit receipt status and 15s read-timeout impact. A User-Agent/request framing change needs renewed provider probe fingerprint. | After critical safety packages. |

### 3. Docs, evidence and economic stop

- PHASENPLAN/GATE0: Clarify three separate facts: the ONE G0.7 economic smoke attempt, G0.8 token acquisition/canonical ordering/cost, and the STRICT witnessed producer execution gap T_buy-T0 <1ms. Missing original producer witness => UNKNOWN; a complete Gate-0 timing PASS cannot be manufactured from N+1. Future success-rate qualifications need new evidence and independently approved trials, not reinterpretation of one attempt.
- STATUS: mark previous 2026-10-20 hard-coded BLOCKED statement historical; clarify that a V4 non-approved **proposal exists** while no approved V4 has been written/armed. Preserve current v5.26 drift STOP and note exact A/B qualification/GUI backend status as code changes land.
- Governance: Document actual four allowed gateway actions (project0.capture.start/stop, bsc.readiness.start/stop), and any future typed **offline candidate** action strictly separately from economic arm/send. Requested/effective modes and generation must be grounded in a versioned binary protocol; no unimplemented LIVE toggle may be shown as enabled.
- Evidence required for each task: changed files and commit SHA, test commands and outputs, negative cases, old/new schema/record/contract/DRY/signed-byte hash analysis, and any still unqualified provider/producer issues. Do not claim builds/tests not actually run.
- Explicit economic gate remains outside this coding assignment: newest actual Portal deployment/DRY and per-profile hashes, V4 proposal/approval authority, exact budget/gas/funding/nonces, builder capability/freshness, one-use latch, canonical acquisition verification and original producer timing source must be evaluated, then one-off user authorization of that exact candidate and acknowledged economic risk. Until such distinct approval: FROZEN_APPROVED_V4_SHA256 = UNARMED; approved=false; execution_ready=false; LIVE_SEND=NONE.

**STOP for this task:** Never arm, economically sign/send, nonce-burn, fund, create another paid trial, rewrite historical A approval bytes, weaken the original <1ms acceptance, build new C1/C2/AUTO LIVE execution, introduce new branches or declare Gate-0 PASS. Continue work on the existing main branch with narrowly scoped traceable commits. All code changes remain offline/read-only.

## BINDING v9 AMENDMENT — 2026-10-23: independently verified token addresses and granular ingress selection

**Problem and separation:** An externally received WebSocket/auction/p2p event is evidence about a candidate, **never an authoritative token-address source merely because one or several providers agree**. G0.7 salt profiles A/B apply exclusively to **our own self-launch**; foreign launch candidates use an independently verified decoded transaction. Neither provider count nor GUI selection can override a failed cryptographic/venue/deployment verification.

### 1. Three distinct frontend address/candidate contexts

| Selector | Purpose | Address contract |
|---|---|---|
| `FOREIGN_AUTO` — **Auto: fremde Launches** | Later production foreign Flap/Four launch detection | Recover from an independently validated **complete signed pending launch transaction** using the correct **version-pinned venue-specific** derivation (Flap uses CREATE2 with pinned Portal/deployer and initcode/implementation). No A/B match required. No human-entered or provider-reported token address trusted as authority. |
| `SELF_A` — **Eigener Test A** | Controlled G0.7 self-launch with frozen public salt | Exact predicted address AND signed launch identity must match **separately frozen A**; mismatch = STOP, never auto-swap to B/AUTO. |
| `SELF_B` — **Eigener Test B** | Controlled G0.7 self-launch with private CSPRNG salt | Exact predicted address AND signed launch identity must match **separately qualified B**; mismatch = STOP, never auto-swap to A/AUTO. |

These are **operating/candidate scopes**, **not three mutually-exclusive provider sources** and **not** an alternative to the mandatory source authenticity/address verification. The B profile salt remains secret only up to any unavoidable publication/disclosure; do not advertise lasting on-chain secrecy. The active frozen/approved profile may not be changed while an intent is armed or executing. A/B offline results are separately measured, not silently granted extra paid Mainnet attempts.

### 2. Independent granular enable controls (never conflate dimensions)

1. **Candidate scope**: `FOREIGN_AUTO | SELF_A | SELF_B`.
2. **Five independent ingress source modes**: `BSC_INGRESS_GETH | PUBLICNODE | DRPC | BLOCKREQ | 48CLUB`, each `OFF | SHADOW | LIVE`, as in v6. No sixth provider silently introduced.
3. **Per-venue admission mode**: at least `FLAP | FOUR_CLASSIC | OPENFOUR`, independently `OFF | SHADOW | LIVE`, qualified only where code and evidence exist. A source is economically eligible only if both its own effective mode **and** the venue's effective mode are LIVE; OFF in either layer rejects the candidate for that scope, SHADOW in either layer permits **shadow-only** evaluation.
4. **Automatically derived non-overridable capability/health** per provider and venue/deployment: `HASH_ONLY | FULL_SIGNED_TX | QUALIFIED_PRIVATE_TARGET | UNKNOWN` and `HEALTHY | DEGRADED | RATE_LIMITED | DISCONNECTED | UNQUALIFIED`. A user-selected LIVE setting does not turn a hash, an unqualified private target or a degraded deployment into an actionable trade event. Distinguish **requested** mode vs **effective** mode in GUI/control/evidence.
5. **Global independent execution modes**: `REPLAY | PAPER | SHADOW | LIVE`, economic exact-hash approval, guard, budget, nonce/intent, inclusion route, provider readiness and one-attempt limit. The LIVE ingress/venue switches **never** authorize signing or broadcasting. Required canonical-state, reorg and safety services remain available or the whole operation fails closed, including when trade-ingress geth is OFF.

All settings are typed/versioned **binary control records** with one monotonic configuration generation, atomic activation, validation and rollback. No frontend per-event roundtrip, no JSON internal config and no UI callback in HOT0. Safe mode transitions cannot leave an old queued candidate/intent with new authority.

### 3. Address verification and fastest valid source wins

- Each isolated ingress adapter independently timestamps arrival and decodes its external wire format (external JSON-RPC if the provider requires it) into a bounded native typed/binary event. The pinned geth P2P hook remains independent and must not be starved by WSS/48Club workers.
- For `FULL_SIGNED_TX`, verify signed transaction hash and recovered sender, chain ID, destination Portal/venue address, selector, deployment/ABI version and required launch calldata; extract the creator's salt only when that method actually contains it. **Version-pinned local derivation:** for Flap Tax V3, use the current qualified Portal CREATE2 deployer and exact minimal-proxy initcode hash for the matching implementation, then verify predicted address/vanity against any provided candidate. **Do not apply Flap CREATE2 rules to Four Classic/OpenFour without their own proven venue contract.**
- No synchronous RPC lookup, provider cross-check wait, HTTP/JSON parsing, file/DB reads or on-demand address mining in the native decision/dispatch hot path. Keep verified deployment/chain rules and necessary state preloaded; if unavailable or drifted, mark unqualified and fail closed. A remote provider's `tokenAddress` is an untrusted comparison field, **not a bypass** of local derivation.
- `HASH_ONLY` can contribute first-seen provenance/counters or shadow enrichment only, not an actionable buy by itself; `QUALIFIED_PRIVATE_TARGET` requires its own prequalified versioned proof of exact target/order/simulation and all fields needed for safely preparing a BUY. Do not mislabel 48Club newPendingAuctions as a full signed transaction.
- `FIRST VERIFIED ACTIONABLE WINS`: first fully verified LIVE-eligible event claims the candidate's economic decision **atomically** without waiting for slower peers or a majority/quorum. Later matching events enrich per-source measurement; duplicate notifications cannot spawn another LIVE intent/nonce. A SHADOW-source event is evaluated with the **same Rust business logic** in a separated non-economic scope but **can neither claim nor block** a LIVE candidate.
- Canonical identity / collision handling: retain source provenance, signed launch tx hash, chain ID, venue/deployment/version, salt derivation identity and locally predicted token address. **Same signed TX hash + conflicting signed bytes/address** => source or decoder inconsistency: quarantine disputed source/event and fail closed for that conflicted candidate. **Different launch TX hashes claiming the same predicted token address** => collision/conflict, prohibit economic release pending canonical resolution (no automatic A/B fallback). Never accept “3 of 5 providers agree” as proof if the math, signature, deployment or state fails.
- Asynchronous comparison across all enabled sources logs first hash / first full / first actionable, computed address, exact verified identity, timestamp, mismatch, gaps/drops, eligibility and configuration generation in the existing **one binary journal**. Do not introduce a second engine or delay winning LIVE decisions for comparisons. All addresses are 20-byte native values, with constant-format equality in Rust; no hex-string comparisons in the hot path.
- **No BUY readiness claim from address derivation alone.** Successful launch/actual resulting token code, quote/state and intended BUY feasibility must still pass the existing mandatory conditions; pending TX is not guaranteed to be included. `T0` and on-chain `T_buy` remain independent producer-execution facts. Preserve target `T_buy - T0 < 1,000,000 ns` and UNKNOWN without producer witness.

### 4. GUI observability / tests / phase constraint

The frontend later presents **three different control groups**: (A) candidate scope `FOREIGN_AUTO / SELF_A / SELF_B`; (B) five source-mode switches plus separate venue-mode switches; (C) global runtime/economic safety and individually qualified inclusion adapters. Per-source status shows requested/effective mode, capabilities and health, last first-full/actionable latency and locally **computed** token address; show `VERIFIED / PARTIAL / CONFLICT / UNQUALIFIED / NO_EVENT` distinctly. Comparison is **always-on asynchronous diagnosis**, not a separate trust/verification bypass switch. For own SELF_A/B, an observed launch can additionally be compared against the selected pinned address and signed identity.

Minimal future acceptance matrix: (1) full-TX source ahead of four slower sources => immediate qualified decision; (2) earlier hash-only cannot outrank a later full verified event; (3) wrong chain/selector/deployment/CREATE2 address => fail closed; (4) source mismatch and same-address distinct-launch collisions => fail closed without duplicate intent; (5) SHADOW source cannot block LIVE; (6) source/venue OFF, degraded/unqualified status and rapid config changes cannot bypass mandatory state/safety or consume live nonce; (7) SELF_A/B mismatch or selector switch during armed intent => STOP; (8) 48Club target-only not admitted without provider-specific original qualification. Deterministic negative unit tests are allowed but **not** Mainnet timing/economic proof.

**Phase gate unchanged:** This amendment **specifies the later architecture** and permits only the isolated A/B offline-only frontend preparation already authorized in v8. It **does not** authorize implementation of foreign `AUTO` production ingress, source×venue LIVE switching, new builders, C1/C2 work, additional real G0.7 attempts, R0/DRY re-signing or economic activation before the existing `G0.7 → G0.8 → PASS/ADAPT/STOP` gate. Current guard stays UNARMED.

## BINDING v8 AMENDMENT — 2026-10-23: two Salt profiles A/B and BSC frontend selector

**User decision (binding):** Implement both alternative salt strategies for the **own G0.7 self-launch test** and allow an independent A/B selection in the BSC frontend and native control plane. Both use **the same existing Rust engine, qualification, quote, risk, build/sign and G0.8 verifier**, never duplicated trading engines.

| Candidate | Source of Salt | Evidence and exact approval |
|---|---|---|
| **A / FROZEN_SALT** | Previously frozen, **publicly known** original salt; existing predicted token address | Preserve all immutable historical evidence; the previous Portal-v5.25.0 PASS is invalid for v5.26.0 and must be freshly requalified before any authorization. |
| **B / FRESH_RANDOM_SALT** | Locally generated CSPRNG salt, checked against the official CREATE2 vanity rule (Tax-V3 suffix 7777), never reusing the public deterministic sequence | Separate token address, calldata, signed hashes, matching DRY/economics/evidence and separately approved V4-or-successor fingerprint. Candidate is fixed once qualified, not silently rerolled on each run. |

**Comparison and timing:** A and B support real-code offline DRY/SHADOW evaluation with individually named binary records and equally controlled chain/Portal version, simulation parent state, gas, budget and timestamps. Compare predicted token identity, effective economics, gas, qualification verdict and measured local processing cost; neither variant intrinsically guarantees earlier acquisition. Do not invent builder acceptance, N/N+1, producer T0 or actual acquisition from offline runs.

**Frontend/control-plane contract:** Show a switch **SELF-LAUNCH CANDIDATE: A / B**, separately from five provider OFF/SHADOW/LIVE switches and global REPLAY/PAPER/SHADOW/LIVE + economic arming. Display selected profile, derived address, binding hashes, DRY/readiness, and eligibility. Do not leak a previously undisclosed B salt/private key into web UI, logs, URLs, telemetry, or ordinary status responses. Persist typed binary, generation-bound configuration. Switching profile changes exact candidate identity and invalidates stale readiness, signed-byte authorization and approval for the newly selected candidate; never silently carry them over, change active nonce/exposure, or authorize live signing/send. Disable unsafe switching while an economic intent is armed or executing; fail closed, no automatic A-to-B fallback.

**Narrow offline-scope exception to the prior no-GUI rule:** User expressly authorizes A/B candidate preparation and a frontend selector for **offline/shadow-only use** before G0.7. Do not implement unrelated GUI changes, C1/C2 source integration, hot-path callbacks, builder/live changes or a second runtime. If the selector cannot be isolated from live guard/nonce/signed bytes, retain the design as PLANNED and do not weaken Gate-0.

**Economic limits unchanged:** Current G0.7 guard remains UNARMED; approved=false; execution_ready=false. One-attempt spend cap 481163500000000 wei, existing exact candidate freezes and the G0.7 -> G0.8 -> PASS/ADAPT/STOP order remain. A/B offline testing does not authorize two actual Mainnet attempts, dual concurrent BUYs, re-salting after approval or any broadcast. Each eventual live attempt requires independent fresh qualification, exact signed-byte/hash and one-off express economic authorization. Portal drift and missing producer witness remain unresolved.

**WebSocket distinction for foreign launches:** A/B is **only our own self-launch testing choice**, not a provider/venue mode. For a foreign Flap/Four launch, a sufficiently early *complete pending launch transaction* may expose the launch calldata including the creator-chosen salt. A version-/deployment-pinned ABI decoder can extract it and calculate the eventual CREATE2 token address from correct Portal deployer, initcode and implementation; validate signed transaction identity and on-chain assumptions. If only a hash arrives, a private launch is hidden, the version is unknown or the observation is after T0, do not invent the address or claim pre-T0 discovery. No user-generated salt for foreign launches. The native trading path remains binary and the product goal successful producer execution T_buy - T0 < 1,000,000 ns remains unchanged.

## BINDING OFFLINE BUGFIX EXCEPTION — 2026-10-21

The user explicitly authorized offline corrections of E5–E8 in `bsc_bug_v1.md` despite the C1/C2 development stop below. This exception permits implementation and focused offline regression tests only. It does not qualify or activate C1/C2, change economic parameters, authorize a BUY/nonce burn, or arm G0.7. All production and economic gates remain binding.

## BINDING G0.7 CLARIFICATION — 2026-10-20: finish code before economic approval

**Immediate development objective: complete the existing G0.7/G0.8 code up to the explicit authorization/send boundary. The actual Mainnet BUY is NOT part of this implementation task.** This dated clarification supersedes prior guidance only where it conflates pre-BUY technical readiness with results that require a real future trial. The signed-byte freeze, all safety constraints and the mandatory G0.7 -> G0.8 -> PASS/ADAPT/STOP order remain unchanged.

- **Verified historical evidence:** 2026-10-20 R0 and DRY-0..DRY-4 passed against Flap Portal v5.25.0, with unchanged signed launch/BUY bytes and zero egress; see G07_REQUALIFICATION_RESULT_20261020.md. Do not rerun the long historical scan or an unchanged DRY without evidence of invalidation.
- **Technical pre-LIVE readiness:** PASS_READ_ONLY with exit 0 must be reachable after the mandatory technical conditions actually pass (current chain/Portal, bound V4 draft and immutable evidence, exact signed bytes, nonce, wallet/funding, gas and total budget, head/clock/expiry, one-attempt safety and applicable submission preflight). Genuine failures remain BLOCKED/STOP with exit 2. PASS_READ_ONLY is never economic approval or authority to send. The hard-coded BLOCKED-only code identified on 2026-10-20 must be corrected; code-readiness is not yet verified.
- **Builder outcome:** submission format, price-floor documentation and expiry acceptance have limited evidence. Builder inclusion, 48Club price enforcement and building-time effects of noMerge/positionFirst remain **UNQUALIFIED**, never PASS. These *future outcome risks* must not permanently prevent finishing the code. Where the exact V4 approval contract permits a bounded experiment, explicitly document the residual risks for informed user approval. Never bypass a genuinely mandatory pre-egress check or represent unknown inclusion as verified.
- **Producer execution witness:** source_ready=false means actual T0 -> T_buy and the sub-1-ms timing acceptance remain **UNKNOWN**. The missing original producer timing witness does not by itself prevent preparation of a controlled token-acquisition trial. Binding and integrity of the witness **source contract** remain mandatory.
- **Three separate states:** (1) technical PASS_READ_ONLY, (2) explicit exact-V4 user economic approval and live guard authorization, and (3) actual on-chain acquisition/ordering/timing evidence. No state implies the next. Preserve UNKNOWN/UNQUALIFIED as independent dimensions, not fabricated PASS.
- **Scope until code-ready:** finish existing native G0.7 runner, V4-approval preparation, one-attempt journal/anti-replay and immediate G0.8 acquisition/order/cost reconciliation. Only targeted regression tests for changed logic. No GUI, C1/C2, new venue, refactoring campaign, repeat R0/DRY or synthetic test campaign.
- **Economic boundary:** remain UNARMED, approved=false and execution_ready=false throughout this task. The max one-attempt reserve remains 481163500000000 wei = 0.0004811635 BNB. A separate explicit user approval of the exact **approved-variant V4 SHA-256**, bounded risk and one live attempt is required before any arming, signing for live submission or send. Keep exact hash/byte/chain/wallet/nonce/funding/gas/budget/freshness, evidence durability and anti-replay guards intact.

**Development milestone: G0.7 CODE READY — AWAITING EXPLICIT ECONOMIC APPROVAL. This is NOT G0.7 or Gate-0 PASS.**

## **ABSOLUTE ECONOMIC / LIVE STOP — OFFLINE C0/C1/C2 DEVELOPMENT EXPRESSLY PERMITTED (2026-10-25)**

> **Echte SIGN-/ARM-/Funding-/Nonce-Burn-/Sende-/BUY-/LIVE-Aktionen sind ohne jeweils separate ausdrückliche Nutzerfreigabe verboten.** Keine eigenmächtige wirtschaftliche Ausführung.
>
> **Offline C0/C1/C2 implementieren, testen und technisch qualifizieren ist ab sofort ausdrücklich gestattet**, auch bevor ein G0.7-/G0.8-Mainnet-Versuch durchgeführt wurde; siehe vorrangigen BINDING USER DECISION vom 2026-10-25 oben. Die bereits vorhandenen C1-Komponenten und Evidence weiterführen; C1.10-Offline-Freeze nach seinen echten technischen Kriterien abschließen, danach C2 entsprechend der C1-Verträge. Keine Fake-, Legacy- oder Mock-Produktionsabkürzung.
>
> **Für wirtschaftliche Freischaltung, echten Erwerbsnachweis, Produktions-LIVE und tatsächliche Gate-0-Verdikte bleibt die zwingende Reihenfolge:**
>
> ```text
> separat ausdrücklich genehmigter G0.7 Mainnet-BUY
>   -> G0.8 kanonischer tatsächlicher Erwerb + Ordering + Kosten
>   -> PASS / ADAPT / STOP
>   -> erst dann wirtschaftliche/operative Produktionsfreischaltung
> ```
>
> **Weder C1.10-Offline-Freeze noch C2-Offline-Integration ersetzen einen echten Erwerbsversuch.** Lokale Messungen, Builder-ACKs, N+1, Receipts ohne Erwerb oder Testnet-BUYs beweisen keine physische T_buy−T0-Latenz. Bei fehlendem Original-Producer-Witness bleibt diese UNKNOWN. C3/D/F/G/H/I sind ab dem neuesten 2026-10-25-Nachtrag nach technischen Fachabhängigkeiten ausdrücklich offline entwickelbar; E erst nach dem G0.1-Four-Contract-Freeze; J/KB/K und jede wirtschaftliche Freischaltung bleiben gesperrt.

## BINDING DATA-PATH INVARIANT — BINARY HOT PATH / EXTERNAL PROTOCOL BOUNDARY (2026-10-16)

**Normative scope:** BSC Gate-0-to-production architecture. **JSON is prohibited in all DarkBot-owned internal formats and processing, including cold/control, configuration, authorization contracts, qualification, G0.7/G0.8 evidence and reports; the only protocol exception is unavoidable JSON-RPC on external provider/RPC wire boundaries.** This restriction does **not** authorize any live send, change the G0.6a frozen signed bytes, bypass the acceptance guard, reopen completed DRY evidence, or alter the mandatory G0.7 -> G0.8 -> PASS/ADAPT/STOP -> C1/C2 order. The sub-1-ms local reaction target and the separately witnessed T0 -> T_buy metric retain their existing definitions and acceptance gates.

### Mandatory runtime separation

| Boundary | Rule |
| --- | --- |
| Earliest peer/chain signal -> HOT0 -> classification/state -> quote/policy -> build/sign -> local dispatch/handoff | **Binary only internally:** borrowed/fixed-layout IPC records, typed native Rust state, direct ABI/RLP, preallocated buffers and bounded queues. No JSON parse/serialize, HTTP/JSON-RPC control lookup, WSS, child-process start, filesystem read/write/fsync, or synchronous report generation inside the measured decision/dispatch hot path. Do not move such work into an indirect helper called from it. |
| Builder 48Club / BlockRazor | **Only the externally required HTTP(S) JSON-RPC wire format is exempt.** This is not permission to store, produce or consume JSON as an internal DarkBot representation, in either the hot or cold path. Prepare exact bounded wire-byte templates and TLS connections outside the decision/dispatch hot path; patch only required fields without general JSON serialization. Decode externally received protocol responses at the transport boundary into typed native/binary outcomes; process and persist those outcomes without JSON. Preserve expiry, authentication, byte correctness and replay restrictions. |
| Pinned geth <-> Rust / signed BUY | Native P2P + binary IPC/egress and raw signed RLP; no WSS or JSON bridge as the primary production signal or BUY transport. |
| Live capture / evidence | The canonical runtime event record is a **versioned binary append-only journal** (existing `engine/crates/journal` owner) with ordering, clocks, identity, completeness/drop/error counters and crash-recovery semantics. Bounded asynchronous persistence must not block decision/dispatch. A safety-required durable fence must be completed before the economic release or fail closed; never silently weaken evidence durability to claim lower latency. Journals containing signed, still-executable bytes remain private and untracked. |
| Cold/control/offline | **No internal JSON exception.** Configuration, approval contracts and drafts, DRY/qualification inputs, bootstrap state, G0.7/G0.8 records, audit artifacts and internal interchange use typed, versioned **binary** formats. Human-readable Markdown/text views may be derived for inspection but never replace the canonical binary records. Historical hash-bound JSON files are preserved **immutable as legacy evidence only**, not renewed, generated or used as active execution authority; any required legacy import must be offline, explicitly audited, byte/field-verified and converted to the canonical binary format. Read-only external RPC JSON-RPC is permitted only on its mandatory wire boundary, never as internal storage or representation. |

### Phase-accurate implementation (no new engine or refactor loop)

1. **Now, G0.7/G0.8:** close the internal JSON compliance gap in the existing qualification harness, configuration/authorization contract, probe evidence and G0.8 verification with a **minimal, auditable binary migration before any live authorization**. Do not replace the tested BUY/signing algorithms or alter frozen signed launch/BUY bytes, existing DRY hashes or original historical evidence. Establish byte/field equivalence, guard fail-closed behavior, durable-recovery behavior, test parity and G0.8 reader compatibility with a few targeted regressions. Keep the guard `UNARMED` until a separate explicit approval of the final contract and single economic attempt; a changed approval contract needs its own new reviewed binding. There is **no internal JSON exemption for G0.7/G0.8**. Historical JSON remains immutable evidence, not an active authority.
2. **C1.10 -> C2 offline now; operational egress only after G0.7/G0.8 verdict:** implement the single production runtime and actual binary HOT0 -> state -> BUY -> qualified egress path. Prewarm RPC/HTTP(S)/TLS and serialize provider templates outside its measured boundary. Qualify any bounded dynamic patching independently (byte-exact requests, matching signature hashes, expiry/window and provider policy); do not introduce a second BUY/signing algorithm or use G0.7 as the permanent trading runner.
3. **PAPER / final production hot-path requalification (I) before LIVE (J):** verify real-path allocation/blocking/format boundaries with instrumentation and measured worst-case latency, counters for dropped/late records, replay/crash tests and independent on-chain outcome checks. `PASS` requires evidence from the actual runtime, not from `c19_consumer` fixtures, derived reports, FakeTransport, builder ACKs or local-only handoff.

### Non-regression / acceptance checks

- Prove by focused static review and relevant tests that **no internal JSON serialization, parsing, persistence or interchange exists in hot, cold, configuration, qualification, evidence or G0.7/G0.8 processing**. The only JSON-RPC exception is the external network protocol boundary; responses become typed/native records immediately. Separately measure that no WSS client, child process, file access/fsync, dynamic framing or synchronous reporting executes between production ingress and outbound handoff.
- Record the timing boundaries separately: earliest observation, local receipt/decision, signature completion, egress handoff, provider write and actual on-chain acquisition. Never infer `T0 -> T_buy < 1 ms` from a receipt, block ordering or local timing without original producing-execution witness.
- Before activating migrated G0.7/G0.8 or production components, prove byte/field parity against preserved immutable legacy evidence, deterministic binary decoding, no lost signed-TX references, crash recovery, G0.8 compatibility, cryptographic identity and unchanged safety fences. Retire active internal JSON readers/writers/contract files only after this proof. Preserve all original historic files untouched and keep the migration as narrow as possible.
- Never send real BNB, arm the compiled guard, modify approval state or loosen an economic bound on the strength of this section alone.

## G0.6a–G0.8 qualification update — binding 2026-10-13

**Existing evidence remains valid.** The existing G0.6 DRY1–DRY4 PASS is retained with its original scope and exact parameters. This update does not reopen all of G0.6 or authorize a live transaction. Additional qualification applies to the new G07 minimum-value and dynamic gas candidate and to any modified signed parameters. The absolute *economic and operational* Gate-0 stop rule remains unchanged; C0/C1/C2 offline implementation is explicitly allowed by the 2026-10-25 binding decision.

### Evidence and scope

| Evidence | Status |
| --- | --- |
| Existing G0.6 final DRY | PASS, preserved |
| G07 1 wei local BUY | PARTIAL: two EthereumJS executions each returned 180299145 raw token units, matching buyer balance; launch gas 5223342, BUY gas 167077 |
| Stable minimum qualification | OPEN: later stability attempt failed during local launch, before BUY |
| Historical RPC state | PARTIAL: observed missing trie node; bounded whole-search fresh-preflight restart exists but does not prove persistent state availability |
| Dynamic gas | PARTIAL: current read-only candidate max(eth_gasPrice, verified applicable builder floor), subject to cap; not optimal bidding |
| G0.7 actual Mainnet BUY and G0.8 acquisition/ordering | NOT EXECUTED / NOT AUTHORIZED |

**Update 2026-10-16 (evidence, not a re-rating of the rows above):** the selected G0.7 attempt-1 candidate (BUY 0.000001 BNB, gas 0.05 gwei = current `eth_gasPrice` = verified builder floor, launch gas limit 6 603 270, BUY gas limit 3 000 000) passed the final DRY-1..4 on 2026-10-15 (`bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/`, `dry_result.json` SHA-256 `0efe78a7…`), and read-only readiness reports `FUNDING=PASS` against these exact parameters. The 1-wei TECHNICAL_MINIMUM above stays unqualified; dynamic gas is frozen at the candidate for attempt 1 (no bid optimization). Details: ledger section "2026-10-16 — G0.7-Scope-Abgleich".

**Update 2026-10-17 (evidence, not a re-rating of the rows above):** the internal-JSON migration of G0.7/G0.8 (M1–M8) is implemented and verified offline; the binary v3 contract is an inactive proposal and the guard stays `UNARMED`. The read-only readiness of 2026-10-17 found the Flap Portal upgraded on Mainnet (`portal_version` v5.24.0 → v5.25.0, new implementation `0xe13227370996f892a56d2ff4a30f6026887b48fb`): per the scope rule (mismatch is STOP) the bound DRY and its signed bytes no longer qualify; G0.7 attempt 1 cannot be armed on them. Evidence: `bsc/native/evidence/g07/readiness_20261017T101215Z/`; ledger entry of 2026-10-17.

One wei is the smallest positive native unit, **not a qualified final minimum** until a reproducible, complete launch-then-BUY test passes. Raw token output is not an economically meaningful quantity without independently verified token decimals and fees.

### G07 reproducibility and monotonicity gates

- Keep using existing local EVM crosscheck, diagnostics and bounded complete restart with fresh preflight. Do not mix evidence from different parent blocks. Do not build a new snapshot subsystem unless the existing mechanisms demonstrably cannot reproduce the frozen test.
- Distinguish LAUNCH_FAIL, RPC_STATE_UNAVAILABLE, EVM_DIVERGENCE, BUY_REJECT and QUALIFICATION_INCOMPLETE. Only a true BUY_REJECT **after successful launch** may advance the failed-BUY lower boundary. An intermittent local launch revert invalidates the candidate's final qualification.
- Exponential bracketing and binary search require a supported monotonic-executability assumption. Validate it for the frozen contract/state and relevant amounts; if violated or unprovable, output QUALIFICATION_INCOMPLETE rather than a false exact minimum.
- Maintain distinct outputs for TECHNICAL_MINIMUM and QUALIFIED_MAINNET_TEST_VALUE. A positive local 1-wei token return alone does not authorize a 1-wei Mainnet test.

### Dynamic gas and total-budget gates

- Gas is selected dynamically for the candidate from current BSC eth_gasPrice and verified *applicable* provider rules, not a permanently correct 0.05 or 1 gwei. BlockRazor, 48Club and direct P2P have different admission/ranking economics. Current max-rule is DYNAMIC_GAS_CANDIDATE_ONLY, not proof of the fastest inclusion.
- Freeze the RPC observation, block, builder mode/evidence, gas candidate and approved ceiling. If pricing or total approved spend exceeds the ceiling, STOP; never auto-increase, silently clamp or substitute unqualified bids.
- Budget the **entire** attempt: launch gas + BUY gas + builder tips/payments + BUY input value + other charges. Distinguish worst-case gas-limit reserve from actual charged fees.
- If any final signed field, gas price, nonce, calldata, gas limit or BUY value differs from the prior qualified DRY, requalify the affected exact-byte transaction and necessary G0.6 DRY1–DRY4 acceptance checks. The historical PASS remains intact. Refresh gas and builder requirements before G0.7, then require explicit acceptance and funding approval.

### Priority and deferred optimization

1. Preserve existing G0.6 DRY PASS; fix only observed G07 state/launch instability and verify minimum-search assumptions.
2. Qualify technical minimum, economically allowed minimal test spend, gas candidate and exact signed G0.7 parameters.
3. Run required changed-parameter final DRY checks, followed by explicit authorization for at most the approved Mainnet attempt.
4. G0.7 actual controlled on-chain BUY; G0.8 confirm canonical buyer token acquisition, position relative to launch and competition, builder route and complete costs. Treat T0-to-T_buy as UNKNOWN without a valid producing-execution witness; position N+1 alone is not sub-ms timing.
5. PASS / ADAPT / STOP. Only then permit economic/operational production activation; offline C1/C2 coding and technical qualification may proceed beforehand under the 2026-10-25 decision.

**DYNAMIC_INCLUSION_BID_OPTIMIZER: PLANNED / BLOCKED UNTIL G0.7/G0.8.** Develop distinct P2P, 48Club and BlockRazor gas/bundle/tip policies under hard total-spend and profitability bounds, with safe signing invalidation on bid changes. Benchmark actual successful acquisition, inclusion position, validator reach and net costs, not RPC suggestions, ACKs or local handoff alone.

## TOP PRIORITY — LATENCY MEASUREMENT

The immediate Gate-0 priority is to reach the real end-to-end latency measurement as quickly as technically valid.

Critical unknowns:

```text
BUY latency        NOT MEASURED
T0 -> T_buy        NOT MEASURED
T_signal -> T_send NOT MEASURED for the target Mainnet path
```

These measurements are the decisive unresolved Gate-0 evidence. All other work is supporting work and should be limited to what is strictly necessary to make the measurement valid, safe, reproducible, and interpretable.

Do not delay the latency experiment for duplicate simulations, cosmetic hardening, additional refactoring, or re-testing facts that are already established. Once the minimum blockers for a valid controlled measurement are closed, proceed directly to the measurement.


Normative documents, highest priority first:
1. `bsc/docs/1_PLAN/GATE0_TESTPLAN_SHORT.md` — feasibility and real economic/physical acceptance gate; offline C0–I development override from 2026-10-25 remains explicit
2. `bsc/docs/2_REGELN/BSC_BIRTH_ENTRY_ARCHITECTURE.md` — BSC birth-entry objective and evidence rules
3. `bsc/docs/2_REGELN/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md` — native runtime scope and safety; offline development may precede real Gate-0 PASS
4. phase-specific BSC contracts
5. `bsc/docs/2_REGELN/BSC_PHASE_C_PYTHON_RETIREMENT.md` — retired Python BSC prototype policy

If an older document conflicts with Gate 0, Gate 0 wins.

## 0. NON-NEGOTIABLE EXECUTION INVARIANT — DIRECT ON-CHAIN ONLY

DarkBot **NEVER** uses the Four.meme or Flap website, browser UI, frontend trading flow, frontend API, page refresh, or manual interaction as part of BUY execution.

The BUY path is always:

```
earliest verified chain/pending evidence
-> venue decode / derive required BUY inputs
-> local quote/input calculation
-> construct exact contract transaction or ordered bundle
-> local secp256k1 signing
-> measured inclusion adapter
-> direct BSC execution against the verified venue contract
```

For Four.Meme this means direct interaction with the verified current protocol family selected by Gate 0: either Classic TokenManager2 or OpenFour Core. These are separate protocol families and MUST NOT share assumed launch/BUY semantics. For Flap it means direct interaction with the verified current Flap/BNB Portal contract surface. Exact addresses, versions, selectors and modes must be frozen from current evidence before use.

A builder/bundle provider or local bsc-geth is an **inclusion transport**, not a trading frontend. It may carry the already-constructed signed transaction/bundle to block execution, but it never supplies strategy, quote, BUY logic or readiness.

Website/frontend state is **never** a readiness signal. DarkBot does not wait for a page to list, refresh, enable a button or expose a frontend API response.

The only external facts relevant to earliest valid execution are the verified on-chain/pending facts required by the current contract semantics. Internal strategy/admission/risk rules may still decline a BUY.

If token address and all mandatory BUY inputs are derivable before `T0`, DarkBot precomputes as much as possible before `T0` and may hand an ordered launch->BUY bundle to a proven inclusion path before the launch executes. If any mandatory input is not derivable before `T0`, DarkBot reacts at the earliest chain-visible point at which it becomes derivable. In both cases the execution remains direct on-chain.

Frontend/UI/API latency is therefore outside the BUY latency budget and outside the production dependency graph.

## 1. Core objective

The BSC product succeeds only if it can obtain the earliest technically possible successful BUY after a new launch becomes executable.

Fast local code, pending lead, geth acceptance or builder acknowledgement are not substitutes for actual execution ordering.

## 2. Current state

- Existing Solana/Pump.fun runtime remains operational.
- Python Phase-C BSC/EVM prototype remains retired / do not merge.
- No BSC production runtime is **activated** (economic/operational LIVE) before Gate-0 feasibility is proven. Since the binding user decision of 2026-10-25 („GELTENDE REGELN“ no. 1), the native runtime C0–I may be fully implemented, integrated and technically accepted **offline** beforehand (E only after the G0.1 Four contract freeze); J/KB/K and any Mainnet SIGN/ARM/Funding/BUY/SEND/LIVE remain separately gated.
- Gate 0 may use a deliberately small native Rust measurement harness.

## 3. Phase sequence

| Phase | Status | Deliverable |
| --- | --- | --- |
| A | **DONE / KEEP** | Existing Solana/Pump.fun + Phase-A multichain seam |
| C-PY | **RETIRED** | Old Python BSC/EVM prototype retained only as engineering evidence |
| G0.1 | **IN PROGRESS** | Freeze Flap + Four Classic + OpenFour Mainnet contracts, versions, launch/BUY semantics and live evidence |
| G0.2 | **PASS PER SCOPE / OTHER MODES OPEN** | `G0.2_FLAP_PASS` for the historical DOGE-quote mode (ledger); native-BNB self-launch fields covered by the 2026-10-02 Curve-20 crosscheck PASS and the 2026-10-15 final DRY; other Flap modes, Four Classic and OpenFour open |
| G0.3 | **PASS PER SCOPE** | `G0.3_PASS_PINNED_OFFLINE_SCOPE` (historical case); exact launch→BUY semantics of the frozen G0.7 bytes by DRY-1/DRY-2 (2026-10-15); new wallets/amounts/paths need their own case |
| G0.4 | **PARTIAL** | QuickNode source probe without a frozen-scope denominator; not required for the controlled self-launch; Mainnet ingress coverage NOT PROVEN for external launches |
| G0.5 | **PASS FOR PRESIGNED SELF-LAUNCH / EXTERNAL ADAPT** | 2026-10-02: prepared self-launch local+transport p99 0.584 ms (48Club) / 0.527 ms (BlockRazor), 0/100 > 1 ms each; transport evidence only, not T_buy; external-launch decode/quote/sign path diagnostic p99 1.64/2.22 ms |
| G0.6 | **G0.6a FINAL DRY PASS / MATRIX PARTIAL** | Final DRY PASS 2026-10-11 (1-gwei profile) and 2026-10-15 (attempt-1 candidate); builder semantics (pre-arm checks 6/10) UNQUALIFIED until the approved builder probe runs; full ingress×inclusion and concurrent-launch matrix for external launches still PARTIAL |
| G0.7 | **NEXT / ONLY PRODUCT WORK** | Controlled minimal-value Mainnet BUY with value-bearing execution |
| G0.8 | **IMMEDIATELY AFTER G0.7** | Prove actual token acquisition, ordering, competing external BUYs, costs and entry price |
| G0.9 | **BLOCKED** | Classify external builder dependency: NONE / BENEFICIAL / REQUIRED / UNKNOWN |
| G0.10 | **BLOCKED** | Own-builder feasibility: validator/proposer reach, operating cost, bid competitiveness and independence from third-party providers |
| G0.11 | **BLOCKED** | PASS / ADAPT / STOP decision per venue |
| C0 | **IN PROGRESS / FOUNDATION FROZEN** | Unit-6 startup authorization + recorder/replay foundation IMPLEMENTED_TESTED/FROZEN for foundation scope; carried unchanged through the R1–R13 restructure (2026-10-15/16, regression identical to baseline, STATUS.md); full production qualification remains separate |
| C1 | **OFFLINE DEVELOPMENT AUTHORIZED — C1.10 TECHNICAL FREEZE OPEN** | C1.1–C1.8 PASS and C1.9/C1.9b qualified on the pre-restructure source (ledger); C1.10A checkpoint 2026-10-10: regression PASS with pinned-baseline race exception but `C1_10A_COMPLETE = NO` (full non-race suite, final build, identity binding, frozen-binary C1.9 measurement and freeze still open); no C1.10B acceptance record; after the R1–R13 restructure and C1.5–C1.8 re-integration (2026-10-16) the C1.9/C1.9b binding and the C1.10 freeze must be done on the new source; offline technical freeze authorized by the 2026-10-25 decision; economic production activation still waits for real G0.7/G0.8 |
| C2 | **OFFLINE DEVELOPMENT AUTHORIZED AFTER C1 TECHNICAL INTERFACE/FREEZE; LIVE BLOCKED** | Production pending/canonical/reorg/nonce/replacement state |
| C3 | **OFFLINE DEVELOPMENT AUTHORIZED; ECONOMIC EGRESS DISABLED** | Full native local build/sign/geth technical proof using isolated non-economic or authorized Chapel-97 test context; Mainnet-SEND unavailable until separate approval |
| D | **OFFLINE DEVELOPMENT AUTHORIZED; LIVE DISABLED** | Full native version-bound Flap Rust decoder; no Mainnet BUY |
| E | **OFFLINE DEVELOPMENT CONDITIONED ON G0.1 FOUR CONTRACT FREEZE; LIVE DISABLED** | Full Four Classic/OpenFour decoders only after independent pinned contract/deployment/ABI evidence |
| F | **OFFLINE DEVELOPMENT AUTHORIZED; LIVE DISABLED** | Native capture, journal and pending-coverage/market evidence; no paid services |
| G | **OFFLINE DEVELOPMENT AUTHORIZED; LIVE DISABLED** | Full native economics/quote verification, simulated/recorded data only |
| H | **OFFLINE DEVELOPMENT AUTHORIZED; LIVE DISABLED** | Native Strategy C and deterministic replay/EV; no economic send |
| I | **OFFLINE DEVELOPMENT AUTHORIZED; ECONOMIC LIVE DISABLED** | Real producer code exercised through safe PAPER/SHADOW; no Mainnet send, physically witnessed sub-ms still UNKNOWN |
| J | **BLOCKED** | Production LIVE enablement using Gate-0-proven inclusion adapters |
| KB | **BLOCKED / OPTIONAL AFTER PASS** | Own BSC builder track if external builder access is materially beneficial or required |
| K | **BLOCKED** | Only measured performance escalation |
| S0-S3 | **FUTURE** | Optional later native Solana convergence |

Every phase STOPs for **independent read-only review of the final SHA and the complete applicable technical gate**; once the gate passes, the implementer continues automatically to the next **offline-authorized** phase, without asking the user. HIGH/CRITICAL defects must be corrected before proceeding. A separate explicit user decision is still required for economic SIGN/ARM/BUY/SEND/LIVE and for genuinely unresolved economic choices.

## 4. Gate 0 is not C0

Gate 0 deliberately does **not** require the complete production architecture.

Gate-0 ordering rule: prove the smallest exact BUY semantics first, collect ingress coverage in parallel, and only then invest in fine local latency optimization.

It needs only enough code to answer:

```
Can we know enough before/at first tradability
to construct the exact BUY,
and can that BUY occupy the earliest valid execution position?
```

Do not require full journal/replay/reorg/Strategy C/PAPER before answering this.

## 5. Gate-0 measurement model

```
T_pre = earliest actionable pre-tradability evidence
T0    = first execution point where the coin is buyable
T_buy = our successful BUY execution
```

Desired product target:

```
T_buy - T0 < 1 ms
```

For ordinary BSC chain evidence, do not fabricate a sub-ms `T_buy-T0` number.

Economic PASS evidence is actual transaction ordering:

```
launch position N
our successful BUY = earliest valid position after N
successful competing BUYs between = 0
```

Local engineering target remains <1 ms for the controllable hot path.

## 5a. BSC Parallel Ingress Portfolio — five independently switchable signal sources (binding proposal adopted 2026-10-22, v6)

**Scope and priority.** This is an additive ingress contract, not a second engine, an alternative Gate-0 acceptance rule, an exception for signing/sending, or authority for C1/C2 economic LIVE activation; offline C1/C2 implementation is authorized by the 2026-10-25 binding decision. Preserve G0.7 -> G0.8 -> PASS/ADAPT/STOP, the frozen exact-V4 economic approval, the UNARMED default and all binary internal-format rules. Bounded read-only G0.4 observation of these candidates is allowed; no value-bearing send or unapproved private-feed expense follows from this section.

### 5a.1 Product objective and timing — never confuse block arrival with T0

Goal: first independently actionable evidence of a foreign Flap, Four Classic or OpenFour launch; prepare an exact valid BUY as early as possible, preferably before `T0`; obtain earliest valid successful external acquisition by qualified ordered inclusion. The existing objective **`T_buy - T0 < 1,000,000 ns`** is unchanged and requires original producing-execution evidence. Initial birth-entry placement also requires launch index N, our successful BUY index N+1 in the same canonical block, zero successful external BUYs between and attributable positive token acquisition. N/N+1 alone is not a physical sub-ms timing witness.

- `T_first_hash`: first local hash observation (possibly non-actionable).
- `T_full`: first locally received complete cryptographically bound transaction (where relevant).
- `T_actionable`: first source event independently verified as sufficient for the exact venue, deployment, state and qualifying BUY path.
- `T_ready`: exact valid BUY transaction/bundle completely built and signed.
- `T_queue` and `T_send`: existing separately measured queue and real adapter handoff boundaries.
- `T0`: first producing execution point at which independent external BUY becomes valid following successful launch/activation.
- `T_buy`: our actually successful producing BUY execution with proven acquisition.
- `T_head_local`: local arrival of a block/head, **not T0**. Header timestamp and chain block interval are also **not T0**.

`T_head_local - T_full`, even when positive by hundreds of milliseconds, measures only a lead relative to *local head reception*. It does not establish `T_full < T0`, `T_ready < T0`, builder acceptance, or real `T_buy - T0`. Standard RPC, receipts and timestamps cannot invent the original producer's sub-ms execution clock. Preserve `UNKNOWN` where the original producer/validator witness is missing. Block interval is not an additive component of local `T_actionable -> T_send` latency; physical inclusion happens at the builder/validator and is measured separately.

### 5a.2 Five independent inputs and switches

| Source ID | Input | Class | Initial release eligibility |
|---|---|---|---|
| `BSC_INGRESS_GETH` | Pinned bsc-geth early Fast-Lane hook -> native binary IPC | P2P/full transaction | Native baseline; production LIVE still unqualified |
| `BSC_INGRESS_PUBLICNODE` | PublicNode BSC `eth_subscribe` | Public WSS full pending TX | Read-only evidence; requalification needed |
| `BSC_INGRESS_DRPC` | dRPC BSC `eth_subscribe` | Public WSS pending TX | Read-only evidence; observed public endpoint rate limit |
| `BSC_INGRESS_BLOCKREQ` | BlockReq BSC `eth_subscribe` | Public WSS pending TX | Read-only evidence; observed rate limits/low coverage |
| `BSC_INGRESS_48CLUB` | 48Club `newPendingAuctions` | Authenticated private/auction target | UNQUALIFIED, credentials/Soul Point eligibility not proven |

Each input has a user-controlled, independently persisted configuration mode `OFF | SHADOW | LIVE` and a **separate**, automatically derived `HEALTHY | DEGRADED | RATE_LIMITED | DISCONNECTED | UNQUALIFIED` state. `OFF`: no candidate contribution; close/disable source safely. `SHADOW`: ingest and record without authorizing a source-triggered BUY. `LIVE`: source may contribute only after exact capability, chain/deployment, venue, version, health and release-profile qualification plus separate global economic/live authorization. Setting a source to LIVE is never an arming or send permission. A source disabled for signal ingress must not remove mandatory canonical chain, safety, state, reconciliation or reorg services; if such services lack an authorized replacement, safe operation is blocked.

48Club Auction Feed access is not assumed free; never buy subscriptions, bind capital for Soul Points or silently introduce paid providers. `FREE_ONLY` is the baseline. If a source's free tier degrades, degrade/isolate that source, not the native path or safety controls. Every source is independently configurable from terminal/control plane and later from GUI, with coherent persisted atomic generations and measured mode-transition/rollback semantics.

### 5a.3 Parallel race: FIRST VERIFIED ACTIONABLE WINS

All eligible sources run concurrently. No serial wait, preference delay, or synchronous remote hash/receipt/quote lookup on the trading hot path. Preserve first hash, first full, first actionable and source provenance per source separately. The first independently verified actionable source event wins the candidate decision, regardless of provider brand. A hash-only event cannot outrank an already actionable full event, and merely accepting `newPendingTransactions(true)` is not proof that full TX objects are delivered: verify actual event shape and signed identity. A private builder target qualifies only when its exact capability/simulation/state/ordered-inclusion semantics allow correct construction without inventing missing full transaction bytes.

One canonical candidate/intent/nonce/economic owner and **one existing native Rust venue/state/quote/risk/build/sign implementation** serve all inputs. Duplicates or later enrichments cannot create conflicting intents, nonces, signatures or duplicate economic exposure. Inclusion/output adapters (local geth, 48Club, BlockRazor) remain separately versioned and independently qualified; an ingress source does **not** implicitly authorize a corresponding inclusion adapter. Do not modify existing `submission_policy/capability.rs` to disguise ingress capability as inclusion.

External WSS JSON-RPC is permitted only at the unavoidable network protocol boundary. Validate and convert it to typed/binary events before internal processing; no JSON/WSS parsing, HTTP lookup, file IO, logging, network await or dynamic parsing on the measured native decision/dispatch hot path. Separate source budgets for CPU, sockets, bounded queues, memory, reconnection/rate-limit/backpressure. An ill-behaved source cannot delay, starve or invalidate the native geth Fast Lane. Preserve current single binary journal with explicit source health, gaps, drops and replay identity.

### 5a.4 Qualification, evidence and phases

Per source + venue + deployment/version + inclusion adapter, record: actual full/hash/target object and crypto identity; canonical launch denominator; full and actionable coverage; true on-Mac same-hash/source timestamp races against local geth; p50/p95/p99/max and all tail failures; disconnects/reconnects/rate-limits/drops; resource overhead and nonregression; cost and eligibility. Distinguish `T_head_local - T_actionable` from producer `T0 - T_actionable` and retain UNKNOWN without a valid original source witness. Keep exact raw samples, fixture hashes, source/build/config identities and failed attempts, not only selected winners.

- **G0.4:** scoped read-only parallel observation and evidence under the existing economic/live stop-rule; no production LIVE promotion. Under the 2026-10-25 binding decision, a predefined bounded (default at most 7 days) foreign-launch market census may run in parallel with G0.7 preparation only if G0.7 is not delayed. The census must precede *operational C2 production enablement* after real G0.8 acquisition PASS, **not** C1/C2 offline coding.
- **G0.5/G0.6:** integrated source-to-send and ingress x inclusion qualification when permitted under the Gate-0 execution sequence.
- **G0.7/G0.8:** remain strictly tied to their original frozen controlled minimal-value attempt and approved SHA; no alteration of launch/BUY bytes or broader live source set by this amendment.
- **C1/C2:** source adapters, binary ingress and candidate/state dedup may be implemented and locally qualified offline before G0.7/G0.8; production activation remains after the actual verdict; pinned geth remains an independent baseline.
- **C3/D/E/F/G/H:** exact native hot-path, distinct venue decoders, journal/market evidence, economics and common strategy.
- **I/J:** all-mode switching, failure isolation and strict qualified LIVE gates; no unqualified source can trigger a BUY.
- **K:** additional escalation only with reproducible evidence.

**No automatic success claims.** The 2026-10-22 PublicNode/BlockReq/dRPC observations establish reachable public feeds and some earlier-than-local-head full launch sightings; they do not qualify `T_pre < T0`, a N+1 BUY or a production LIVE source. Recorded original producer execution evidence remains mandatory for actual physical `T_buy - T0 <1 ms` acceptance.


## 6. Gate-0 inclusion race is mandatory

Do not wait until J/K to investigate builders.

BSC production uses PBS/builders, and Gate 0 must compare the real available inclusion paths before a large implementation is justified.

Minimum candidates at the time of test:

- local bsc-geth/P2P;
- 48Club/Puissant ordered/backrun-target path if still supported;
- BlockRazor ordered/bundle path if still supported.

Reverify current APIs immediately before the test.

The winning path is selected from real execution placement, not ACK latency.

## 7. Gate-0 venue rules

### Flap

Before full Phase D, Gate 0 is allowed to implement a **minimal factual Flap probe** sufficient for:
- launch recognition;
- BUY-field preparation matrix;
- token-address derivation proof;
- exact first-BUY construction;
- semantic launch→BUY validity;
- ingress coverage measurement for the selected scope.

This is not the full production decoder.

### Four.Meme Classic / OpenFour

Gate 0 treats Four Classic TokenManager2 and OpenFour Core as separate current protocol families. Before full Phase E, minimal factual probes may be implemented for each family sufficient for the same feasibility questions, but assumptions/decoders must not be shared without evidence.

Do not assume pre-execution token-address derivation. Prove it or record that it is unavailable.

## 8. C0 — production foundation (offline development permitted; activation after PASS)

Offline C0 foundation and C1/C2-dependent technical work may proceed under the 2026-10-25 binding decision; production activation still requires the real Gate-0 verdict. The original foundation deliverables remain:

- create clean Rust workspace;
- versioned native event schema;
- immutable binary journal;
- deterministic replay API;
- bounded queues/overflow evidence;
- exact integer/address/hash primitives;
- native latency records.

STOP.

## 9. C1 — production earliest ingress

Implement minimal pinned bsc-geth hooks:

- earliest practical complete transaction event;
- txpool acceptance;
- canonical/reorg events;
- source peer/message provenance;
- shared-memory SPSC into Rust.

Measure peer decode → Rust visibility.

Pending coverage remains a quality metric; do not assume completeness.

Current local qualification checkpoint, 2026-10-09:

```text
reopened C1.3/C1.4 regressions       PASS
C1.4a REV2                           PASS / GREEN_STRONG_HEADROOM / max 20 us
C1.5 Rust HOT0                       PASS
C1.6 M4 development profile          PASS
C1.7 local-geth outbound             PASS
C1.8 concurrency/state safety        PASS
C1.9 measurement                     COMPLETE / integrity PASS / eligible
C1.9 measured max T_ingress->L2      418 us
C1.9 measured max ingress->1st peer  438 us
C1.9 final source/build binding       OPEN pending final full regression/freeze bind
C1.9b Testnet infrastructure          READY / REAL CHAPEL PEER PASS
C1.9b chain                           97 / 0x61
C1.9b pinned geth                     v1.7.8 / cdb7548b5baacf...
C1.9b controlled ETH/68 hook/path     PASS
C1.9b hardened qualification gate     PASS
C1.9b fresh-pin reproducibility       PASS
C1.9b affected geth tests/build       PASS
C1.9b controlled observations         1/1
C1.9b invalid_records/lane_faults     0 / 0
C1.9b analyzer                        PASS / missed=0
C1.9b Mainnet ingress coverage        NOT PROVEN
C1.10 freeze                          BLOCKED pending complete final C1 regression + bind
T0->T_buy <1 ms                       NOT PROVEN
LIVE                                  BLOCKED
```

Detailed commands, identities, distributions, counters and scope/non-claims are recorded in `GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md`. The C1.9 local selected-peer handoff is not a purchase/acquisition timestamp.

C1.9b controlled Testnet hook/path qualification is now locally PASS. The qualification-only pre-sync override is hardened: it requires `DARKBOT_FASTLANE_V2_SCOPE=c19b-testnet`, `DARKBOT_C19B_ACCEPT_TXS_UNSYNCED=YES`, and network ID 97; invalid combinations fail closed. A fresh-pin generated candidate passed targeted/affected geth tests and build, then the hardened runtime produced 1/1 controlled observation with zero misses, invalid records, or lane faults. The hardened candidate was copied byte-for-byte into the local vendor files and rebuilt/retested. These local source changes are **not yet C1.10-frozen**. The next operation is the complete final C1.5-C1.9/REV2 regression and source/build/config binding, followed by C1.10 freeze.


### C1.10A checkpoint — 2026-10-10

Pre-freeze regression has started. Python qualification tooling, Rust fmt/clippy/debug/release tests, C14 interop, targeted geth tests and `go test -race ./darkbot/fastlane` are PASS.

Broad pinned-geth race suites are not clean even on the untouched accepted pin. `./eth/protocols/eth -race` reproduces the same `CachingDB.NoTries` / `core/state/database.go:280` family on current and clean-pin trees. `./eth -race` also fails on both; the current run reported 55 race warnings and the clean pin 51 with common core signatures and substantial failed-test overlap. This is recorded as `PINNED_BASELINE_PRESENT`, not as a claim of exact race equivalence or a race-free upstream tree. DarkBot Fast-Lane itself is race-clean in the targeted run.

C1.10A remains open until the full non-race geth suite, final exact build, source/build/config/evidence binding and final measurement/freeze steps complete.

STOP.

## 10. C2 — production BSC state

Implement only runtime semantics needed for correctness:

- tx identity/first-seen;
- sender+nonce competition/replacement;
- included/reverted;
- canonical/orphan/uncertain;
- bounded reorg ancestry;
- restart/recovery;
- explicit gaps/provenance.

Port relevant Python Phase-C counterexamples as Rust tests.

**2026-10-25 offline override:** Complete the native C2 implementation and its technical tests after the C1 interfaces/freeze without waiting for G0.7/G0.8. Its economic/operational enablement remains disabled until an independently authorized real G0.8 acquisition PASS **and** the bounded, pre-registered G0.4/V10-8 market measurement are evaluated. Implement real native policy/guard/typed requested/effective modes and full GUI status; no productive mock/stub or purely visual disable. Preserve separate approval for all real signing/egress.

STOP.

## 11. C3 — production local execution engineering gate

C3 no longer pretends to prove a real BUY.

It proves only the production local path:

```
native event
-> Rust
-> decode/state/decision/quote
-> build/sign
-> selected local submission adapter
```

Target local controllable latency: <0.5 ms where achievable; >=1 ms investigated; reproducible >3 ms blocks release until explained.

Actual economic BUY feasibility was already proven in Gate 0.

STOP.

## 12. D/E — full production venue decoders

D = full Flap decoder.
E = full Four.meme decoder.

Gate-0 minimal probes are not independent production engines.

Production decoder requirements:

- one Rust source of truth;
- verified current deployment/version;
- pending + canonical factual decode;
- unknown modes fail closed;
- same functions in capture/replay/PAPER/LIVE;
- no remote fetch in hot path.

STOP after each.

## 13. F — native market evidence

Run the production dual-venue capture.

Primary purpose:

- pending coverage;
- complete/full-tx arrival paths;
- peer first-seen spread;
- venue version/mode census;
- canonical outcomes;
- quality gaps;
- replay identity;
- production ingest/decode tails.

Canonical timing may be retained as diagnostic data, but **`EARLY_TX_SEEN - canonical observation <= -10 ms` is not a product PASS gate**.

The useful pre-lead question is whether actionable evidence arrives before first executability often enough to support the Gate-0-proven path.

### F storage/capture extension — binding ownership, not a new phase

Phase F implements the production storage, bounded-resource and native offline-analysis contract in `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` §11, subsection "Production storage, resource and offline-analysis contract". Preserve the existing `DBJNL001` journal and `DBREC001` standalone-record codecs, recorder/replay and the one Rust workspace; add only narrowly owned modules where missing. Never create a competing journal, trade/quote/strategy implementation, Rust workspace or JSON internal data path.

- Preallocated/typed events -> bounded asynchronous recorder -> batch writer -> integrity-checked, segmented `DBJNL001` on SSD; `DBREC001` for typed configuration/run manifests and standalone evidence. No SSD/SQLite/JSON/conversion, child process, wait for the writer or sync logging on HOT0 -> outbound handoff. Required pre-egress financial durability fences remain intact and are never replaced by an asynchronous queue ACK.
- Bound **total actual capture memory by bytes**, including payload ownership, queued items, batches and caches. Permit bounded adaptive growth, never unbounded RAM; preflight sustainable disk throughput/capacity and RAM headroom before an approved long run. A running capture must not silently lose records, claim COMPLETE with loss, or treat exhausting hardware as a successful run; persist explicit gaps/recovery status and retain diagnostics.
- Build an optional, separately budgeted **offline SQLite index** only from verified sealed journal segments. It is rebuildable and nonauthoritative; raw events, canonical verification, economics and financial state stay in native Rust/binary evidence. Exact integer amounts must not be coerced to lossy SQL REAL.
- Phase F retains its separately required continuous **30-minute** native binary coverage run and exact-core replay; per-attempt 15-second windows remain separate. 4h/24h/48h campaigns are scoped future operating qualifications with explicit CapacityPreflight, not new Gate-0 or universal Phase-F prerequisites. Final RAM/backpressure/replay/crash and true hot-path timing checks belong to Phase I. Document and qualify actual source/build/host evidence; no automatic PASS.

STOP.

## 14. G — full economics

Implement complete executable quote/cost semantics once in Rust.

Gate 0 already contains only the minimal exact economics needed to make its test BUY valid.

Phase G adds full supported-mode economics:

- protocol fees/taxes;
- exact quote/output math;
- gas/revert/replacement costs;
- sell/approval path;
- migration/graduation economics.

No synchronous external quote in the live hot path.

STOP.

## 15. H — Strategy C / EV

Only now add Strategy C to the BSC production runtime.

Use same:
- state;
- venue decode;
- economics;
- builder;
- signer.

Positive expectancy after complete costs is a separate question from Gate-0 feasibility.

STOP.

## 16. I — PAPER/shadow + final production hot-path requalification

Run the same production semantics through build/sign in PAPER/shadow, but distinguish two boundaries:

```
production event
-> full production venue decoder
-> production state
-> production economics / quote
-> production strategy / admission
-> production transaction build
-> production signer
-> production serialization
-> PAPER test-sink handoff = T_shadow_sink
```

`T_shadow_sink` is useful for isolating local production-code cost but is **not** the real transport boundary and must never be reported as `T_send`.

Before broad LIVE is enabled, run a controlled transport revalidation using the actual Gate-0-proven production adapter:

```
production event
-> same full production path
-> actual adapter queue/IPC
-> actual defined transport handoff = T_send
```

The final production program must still satisfy the frozen local latency requirement using the same adapter-specific timing boundary defined in Gate 0 **G0.5**. A fast Gate-0 prototype, C3 result or PAPER test sink cannot waive the real transport revalidation.

Also rerun the required concurrent-launch/nonce stress cases against the final production runtime.

STOP.

## 17. J — production LIVE

J is production enablement, not the first time inclusion is tested. Its first controlled step revalidates the actual production transport boundary before broader LIVE is permitted.

**2026-10-25 additional J gate:** Without an original producer execution witness, physical T_buy−T0 remains UNKNOWN; J/LIVE may only be considered after a separate explicit documented user risk decision grounded in actual Phase-I PAPER/SHADOW positioning against successful foreign BUYs (first eligible position, gap to first competitor, sample denominator, failures/costs), **or** an independently qualified original witness. No exception implies sub-1-ms PASS. The pre-registered bounded G0.4/V10-8 market census and a real G0.8 acquisition/ordering/cost verdict must be available for operational enablement. LIVE remains disabled without distinct economic and native safety approvals; an implemented native controller and its UI may be present while effective LIVE is disabled.

Use only inclusion adapters already validated in Gate 0 and revalidated for current service/API behavior.

Validate:
- actual intended-token acquisition by the intended recipient;
- ordering against external competing BUYs;
- realized gas/tips/slippage and total debit/cost;
- replacement/reconciliation;
- replay/decision parity;
- continued compliance with the final production hot-path latency gate.

STOP.

## 18. KB — own-builder independence track

This phase is activated only if Gate 0 shows that an external builder materially improves or is required for the target inclusion quality.

External builders are not a permanent architectural dependency by default.

Evaluate whether DarkBot can operate its own BSC builder using current official builder interfaces/reference implementations.

The acceptance criteria are infrastructure-level:

- validator/proposer integration is technically available;
- sufficient proposer coverage can be reached;
- valid competitive block bids can be produced reliably;
- operating cost is known;
- maintenance complexity is known;
- third-party dependency is reduced without degrading the measured inclusion result.

Possible outcomes:

- **OWN_BUILDER_PASS** — own builder can replace the external provider for the required quality;
- **OWN_BUILDER_PARTIAL** — hybrid operation is justified;
- **OWN_BUILDER_FAIL** — retain the external builder only where its measured benefit justifies the dependency/cost.

Building or operating a validator is not required by this phase.

STOP.

## 19. K — measured escalation only

Escalate only when production measurements identify the bottleneck.

Possible order:

1. peer-path optimization;
2. inclusion/builder path optimization;
3. geographic sensors;
4. direct Rust devp2p;
5. kernel/NIC specialization.

Do not prebuild all levels.

## 20. Architecture invariants

This section additionally inherits the binding binary-hot-path / external-protocol boundary defined near the top of this roadmap (2026-10-16). The required builder JSON-RPC wire frame is an external protocol exception, not a general runtime JSON processing exception.

Production remains:

```
pinned bsc-geth
+ native event path
+ one Rust runtime
+ one decision implementation
+ one build/sign implementation
+ one recorder/replay format
+ measured inclusion adapters
```

Forbidden:

- Python BSC trading runtime;
- duplicate strategy/decoder/quote implementations;
- public WSS as the sole primary production signal;
- remote quote/RPC lookup in the decision hot path;
- treating local geth acceptance or builder ACK as a BUY;
- claiming exact sub-ms on-chain execution from ordinary block timestamps.

## 21. Decision rule

If Gate 0 returns STOP for both Four.meme and Flap, the BSC Birth-Sniping production roadmap stops.

If one venue passes, production work proceeds only for the proven venue first. The second venue may remain ADAPT/STOP until separately proven.


## Verbindliche BSC-Frontend-Verdrahtung und Phasen-Handoff (2026-10-23)

**Referenz:** `frontend/docs/BSC_FRONTEND_BINDINGS_V1.md` (A/B/C/D-Feldinventur, G1–G23, fünf Panels) und `frontend/docs/FRONTEND_PHASENPLAN.md`. **Keine zweite Architektur:** vorhandene React/Vite-App, `frontend/src/features/bsc/`, Graphite, Gateway, `control/v1` + `bsc/v1`-Protobuf, native `qualification/src/status/projection.rs` und bestehender BSC-Rust-Core sind die einzigen Owner. Frontend-Komponenten dürfen Native-Safety, Quotes, Signaturen und wirtschaftliche Admission nicht nachbilden. Fehlende Felder als `SOLL_MISSING` dokumentieren; keine simulierte LIVE-Bereitschaft.

**Pflicht bei jeder künftigen BSC-Phase:** Jedes neue öffentliche Status-/Messfeld, jedes native Control und jeder Filter bekommt im vorhandenen F0-VAR-Manifest eine Zeile mit `native_path, symbol, source_sha, type/unit, native_owner, sensitivity, evidence/provenance, source_quality, mode/gate, protobuf_field, gateway_topic_or_command, frontend_panel, tests`. Beim jeweiligen Phasenabschluss entweder durchgängig Native → sanitizierte Projektion → Proto/Golden → Gateway → vorhandener BSC-Adapter/Store → React-Panel → End-to-End-/Negativtest **nachweislich** angeschlossen oder als `SOLL_MISSING/BLOCKED` mit Owner/Ticket ausgewiesen. Nie vermeintliche Erledigung nur wegen einer Rust-Struct oder GUI-Darstellung.

| BSC-Phase | Frontend-Zuordnung, erst nach Gate freischalten |
|---|---|
| G0.6a/G0.7/G0.8 | Overview und Launch: Readiness mode/verdict/checks, V5, A/B-Candidate, DRY5/DRY6/R0, Quote/Sim, attempt/latch/budget; Monitoring/Analytics: Builder-Send, Capture, G0.8-Akquisition, Ranking/erster fremder BUY/Miner, Gebühren, originaler T0-Witness; Wallets: historische versus aktuelle Funding-Snapshots und getrennte Nonces. Vor tatsächlichem BUY nur offline/read-only; Gate-0-Reihenfolge unverändert. |
| C0/C1/C2 | Monitoring: fünf unabhängige Source-Health-/Capability-/requested-/effective-Modi, geth-P2P, full/hash/actionable, canonical/reorg/dedup, sichere Konfigurationsgeneration; keine LIVE-Umstellung aus dem Browser ohne native freigegebene CAS-Commands. |
| C3/D/E | Monitoring: qualifizierte Inclusion-Adapter getrennt von Signaleingängen; Flap/Four Classic/OpenFour als getrennte Venues, Deployment-/ABI-/Tokenadressverifikation; Analytics: echte Latenz und Failure-Denominatoren. |
| F | Analytics: natives Journal, 15-s-Window, 30-min-Capture, vollständige Source-Census, Queue-/RAM-/Disk-/Segment-/Manifest-/Replay-Qualität. 4/24/48-h-Läufe nur bei separater Berechtigung. Filter ausschließlich auf Report-/History-Ebene. |
| G/H | Analytics: nachweisbare reale Kosten, Gebühren, Taxes, EV, Outcome und Risiken; Launch: read-only native policies. Kein Frontend-Quote- oder Strategie-Duplikat. |
| I/J | Launch: getrennte requested/effective REPLAY/PAPER/SHADOW/LIVE und tatsächliche wirtschaftliche/native Autorisierung; Monitoring: Inclusion und Reconciliation; Analytics: freezegebundene Kampagne, T0→T_buy original producer evidence. LIVE nur nach J und separater expliziter Freigabe. |
| KB/K | Erweitern nur nach eigener gemessener Qualifikation; bestehende Topics/Controls benutzen oder versioniert additiv erweitern. |

**Sperre:** Dieser historische F0-VAR-Nachtrag selbst erteilt keine Geld-/LIVE-Freigabe. Die neuesten bindenden Nutzerentscheidungen vom 2026-10-25 erlauben die vollständige technisch sichere Offline-Entwicklung von C0–I nach Fachabhängigkeiten (E erst nach G0.1 Four-Freeze), **nicht** Mainnet-BUY oder wirtschaftliche Produktionsaktivierung. Kein Wechsel des Designsystems, keine Parallel-Engine, kein zweites Wire-Protokoll, keine heimliche Änderung von Project_0/Solana und keine Entkopplung des BSC-STOP-Gates.

**Read-only-Bindungsgrenze (konsolidiert):** Die fachliche Gate-Reihenfolge bleibt unverändert. Der aktuelle Readiness-Runner kennt `PASS_READ_ONLY`, `BLOCKED`, `STOP` und `READ_ONLY_RPC|OFFLINE_ZERO_EGRESS`; der GUI-Wrapper verliert heute `BLOCKED` an den Exitcode, und beide Prozessbeobachter (`qualification/src/status/publisher.rs::job`, `gateway/src/control.rs::observe_project`) müssen einen regulär abgeschlossenen `BLOCKED`-Run von Prozess-`FAILED` unterscheiden. Der BSC-Publisher fügt den echten Job bereits vor dem Protobuf-STATE-Send hinzu. Der isolierte GUI-RPC-Child besitzt gegenwärtig keine `DARKBOT_G07_CLOCK_ID`/`DARKBOT_G07_BOOT_ID`; damit bleibt sein `CLOCK_CONFIG` blockiert, solange kein attestierter Clock-Owner existiert. G0.7-Klassifikationen und G0.8-`PASS/ADAPT/STOP/UNRESOLVED` müssen unverfälscht und identitätsgebunden im UI erscheinen, nicht als wirtschaftlicher Gate-PASS ohne Originalevidence. **Die konkreten Frontend-/Gateway-Bugfixes und Tests gehören ausschließlich zum [Astra-Auftrag §E](../../../frontend/docs/ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md) und [Audit](../../../frontend/docs/BSC_FRONTEND_QUERSCHNITT_AUDIT_2026-10-23.md), nicht als zweiter BSC-Arbeitsplan hierher.** Keine Freigabe von Signing, ARM, Mainnet-BUY oder Source-/Venue-LIVE; die Offline-Entwicklung C0–I richtet sich nach „GELTENDE REGELN“ vom 25.10.


### Fünf feste Reiter – vollständige öffentliche Native-Felder, keine Anzeige-Modi (2026-10-24)

**UI-Übergabe ohne neue BSC-Phase:** Die bestehende BSC-React-/Graphite-Oberfläche mit **Overview, Launch / BUY, Monitoring, Analytics, Wallets** bleibt. Die Nutzerentscheidung verwirft Basic/Advanced, Umschalter und HTML-Designreviews. Jeder fachlich öffentliche, qualifizierte Native-Wert wird über die vorhandene Native→sanitisierte Projektion→additives Protobuf/Golden→Gateway STATE→BSC-Adapter/Store-Kette übertragen und im fachlich passenden Reiter **untereinander in festen Abschnitten** angezeigt. Verbindliche Reihenfolge der Abschnitte allein `frontend/docs/FRONTEND_PHASENPLAN.md` §13, Field-Ownership `frontend/docs/BSC_FRONTEND_BINDINGS_V1.md` §12, konkrete Umsetzung `frontend/docs/ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md` §D/§E.

**Phasenbezogene Feldabdeckung:** Gegen die 35 registrierten DBREC-Schemas native/öffentliche und private Leafs unterscheiden, gegen die bisher im generierten 299-Feld-Manifest erfassten 16 Schemas (+ nested readiness-check) inventarisieren und 19 Differenzen qualifizieren. Fehlende zukünftige C1/C2/F–J-Producer nicht vorschnell bauen; `FUTURE_PHASE_BLOCKED` sichtbar, wenn sie fachlich zum Reiter gehören. Bei jedem späteren BSC-Phasenabschluss neue öffentliche Felder samt `native_source/schema/owner/source_sha/type/unit/evidence/quality/gate/proto/field_or_public_field/topic/frontend_panel/section_order/field_order/tests` in der bestehenden F0-Inventur nachführen. Keine zweite Feldliste, keine private DBREC- oder Signing-Daten im Browser.

**Unverändert:** G0.7→G0.8→PASS/ADAPT/STOP, V5-Sicherheitsguard, wirtschaftliche BSC-/Venue-Freigaben, Project_0-Freeze und Native-Signer-/HOT0-Ownership. Diese Dokumentationsänderung ist keine Code-/LIVE-Freigabe.
