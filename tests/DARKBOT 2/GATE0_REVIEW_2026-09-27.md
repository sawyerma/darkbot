**DarkBot – Prüfung des First-BUY-Plans, 27.09.2026**

Geprüfter lokaler HEAD: `4c5ff58cb3044721c86c45c12b4d7cc614039610`. Arbeitsverzeichnis sauber. Keine Änderung am Projekt vorgenommen, keine Transaktion gesendet.

**Entscheidung: G0.1 kann beginnen. C0 bleibt gesperrt. Der neue Ansatz prüft die richtige Produktfrage, aber sein PASS-Vertrag braucht die unten beschriebenen Ergänzungen. Ein erfolgreicher Kauf innerhalb von 0–1 ms ist derzeit nicht nachgewiesen.**

Vollständig gelesen: GATE0_TESTPLAN_SHORT, BSC_BIRTH_ENTRY_ARCHITECTURE, BSC_NATIVE_SINGLE_RUNTIME_CONTRACT, BSC_DUAL_VENUE_ROADMAP und MULTICHAIN_ROADMAP. Zusätzlich geprüft: Retirement-Vertrag, bestehende Runtime-Assembly/Registries/Profile, relevanter Strategy-C-Entry-Pfad und Konfiguration, historische Performance-Evidenz sowie der vollständige Dateidiff für `darkbot/`, `tests/`, `migrations/`, `config/` gegenüber `3562890`. Dieser Diff ist leer. Der separat beschriebene Pass-6-ZIP-Prototyp gehört nicht zum geprüften Main-Stand; seine Implementierung ist damit nicht erneut auditiert.

Auf Main existieren keine getrackten Rust-Quelldateien/Cargo-Manifeste. BSC ist weiterhin nicht ausführbare Profil-Metadaten. Der bestehende [Execution-Adapter](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/darkbot/execution.py:21) verhindert LIVE ausdrücklich. Es gibt deshalb noch keinen nativen BSC-BUY-Pfad, dessen Laufzeit oder Käufe man aus diesem Repository bestätigen könnte.

Unabhängiger Regressionstest in einer isolierten Kopie dieses HEAD, Python 3.14.3:

- `python3 -m unittest discover -s tests -p 'test_*.py'`: **287 Tests, OK**, 7,206 Sekunden.
- `python3 -m compileall -q darkbot tests`: Exit 0.
- [Vollständiges Testprotokoll](/var/folders/hc/2j77cvl53nn7zvvb1y_pvprr0000gn/T/darkbot_gate0_audit_7hyj9t33/tests.log).

Diese Tests belegen die vorhandenen Regressionseigenschaften, keine BSC-Inclusion oder First-BUY-Leistung.

**Die Produktgrenze**

Vier verschiedene Größen dürfen nicht vermischt werden: lokale Reaktion, Aufnahme/Reihenfolge im Block, tatsächlicher Token-Erwerb und späterer Empfang der Kaufbestätigung. Ein vorbereiteter BUY kann zusammen mit dem Launch vorab an einen geeigneten Builder gelangen und direkt dahinter ausgeführt werden; die Bestätigung erreicht uns später. Ob unser konkreter Weg das wiederholt schafft, muss G0.7/G0.8 zeigen. Die BNB-Dokumentation beschreibt, dass Builder Transaktionen zusammenstellen und der Proposer zwischen Blockgeboten auswählt. Lokal schneller Code kontrolliert diese Auswahl nicht. [BNB PBS](https://docs.bnbchain.org/bnb-smart-chain/validator/mev/overview/).

Die erste Chart-Kerze definiert keinen präzisen Kaufzeitpunkt. Der belastbare Produktnachweis ist ein erfolgreicher Erwerb an einer vorher festgelegten frühen Ausführungsposition. Auch ein Builder-/Validator-Zeittrace muss seinen ausführenden Prozess, Blockhash, Zeitpunktbegriff und Messunsicherheit nennen: lokale EVM-Ausführungsdauer ist keine universelle Netzwerk-Uhr. Die vorhandene Regel, keine Millisekunden aus Blocktimestamps zu erfinden, ist richtig.

**A. BLOCKER – noch keine belastbare Freigabe für den Produktionsbau**

**A1 – PASS ist nicht quantitativ abnehmbar.** Fundstellen: [G0.8, Zeile 217](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:217), [G0.11, Zeile 255](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:255). Der Text verlangt „competitive repeatability“, ohne Mindestquote, Stichprobenumfang, Zeitfenster oder Kostenobergrenze. G0.8 beginnt sogar mit „For each successful validation“, obwohl anschließend Fehler erfasst werden sollen. Einzelne gelungene Käufe könnten so eine überwiegend scheiternde Methode qualifizieren.

Auswirkung: Ein scheinbares PASS kann erneut einen langen Produktionsbau rechtfertigen, ohne die gewünschte Trefferquote zu belegen.

Minimale Korrektur: Vor den werttragenden Tests ein unveränderliches Versuchsmanifest je Venue/Version/Modus festlegen: erlaubte Handelsgröße, maximale gesamte Entry-Kosten einschließlich Builder-Bid, Testumfang/Zeiträume, Mindest-Erfolgsquote und statistische Unsicherheit. Alle Versuche einschließlich Reverts, Nichtaufnahme, Timeout, zu später Ausführung und SKIP erfassen. Zusätzlich den unabhängigen kanonischen Launch-Gesamtbestand mit sichtbaren und tatsächlich versuchten Fällen abgleichen. Bedingte Erfolgsquote unter versuchten Käufen und Gesamtabdeckung getrennt ausweisen. Unzureichende Daten erlauben kein PASS. Die Zahlen sind Produktanforderungen und dürfen nicht nach dem Ergebnis gewählt werden.

**B. LATENCY LOSS – Messgrenzen lassen noch versteckte Verzögerung zu**

**B1 – G0.3 endet vor dem tatsächlichen Egress.** Fundstellen: [G0.3, Zeile 182](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:182), [Produktions-Timing, Zeile 279](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md:279). G0.3 endet bei `T_ready`, die Architektur fordert dagegen bis zum Inclusion-Handoff. Auch `L7 = inclusion-adapter handoff` legt nicht fest, ob nur eine interne Queue befüllt wurde.

Auswirkung: Ein schneller Signaturbenchmark kann langsame Serialisierung, Adapter-Wartezeit oder Transportwarteschlangen verdecken.

Minimale Korrektur: `T_ready` beibehalten, aber zusätzlich vom vollständigen nutzbaren Signal bis zur tatsächlich definierten Transportübergabe messen, einschließlich IPC-Wartezeit, TX-/Bundle-Encoding, Transportvorbereitung und Backpressure. Bei lokalem geth interne Übergabe und ausgehenden Peer-Handoff getrennt messen; beim Builder interne Queue und Übergabe an die offene Verbindung trennen. `T_signal` auch für Fälle ohne Pre-Launch-Signal definieren. Rohwerte, p50/p95/p99/max und Deadline-Verletzungen unter realistischer Gesamtlast und Launch-Bursts erfassen.

Die frühere Vorgabe „über 3 ms keinen neuen Entry mehr senden“ ist im neuen Vertrag nicht gleichwertig enthalten: [Zeile 318](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md:318) definiert einen Release-Blocker, keinen Abbruch pro Trade. Soll die harte lokale Grenze weiter gelten, muss sie explizit vor der irreversiblen Übergabe geprüft werden. Bereits übergebene Bytes sind durch eine spätere Deadline-Prüfung nicht zurückholbar; solche Verletzungen bleiben sichtbar. Ein lokaler Abbruch garantiert weiterhin keine Ausführung innerhalb von 3 ms.

**B2 – C3 prüft vor den späteren vollständigen Komponenten.** Fundstellen: [C3, Zeile 210](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_DUAL_VENUE_ROADMAP.md:210), [D/E bis J, Zeile 230](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_DUAL_VENUE_ROADMAP.md:230). Vollständige Decoder, Economics und Strategy C kommen nach C3. J verlangt Revalidierung, aber keine präzise Wiederholung des vollständigen Performance-/First-BUY-Gates mit dem endgültigen Release-Binary.

Auswirkung: Ein kleiner Gate-0-Prototyp kann bestehen, während der spätere Produktpfad das Zeitbudget verliert.

Minimale Korrektur: C3 als frühen Integrationstest behalten. Nach H/I und vor breitem LIVE dieselbe Timing- und kontrollierte First-BUY-Abnahme mit vollständigem Produktionspfad, aktivem Recorder, normaler Hintergrundlast und allen freigegebenen Venues erneut verlangen. Keine neue Architektur nötig. Semantische Gegenproben zwischen Gate-0-Fixtures und Produktionscode müssen dieselben Kaufabsichten/Inputs ergeben. Nur tatsächlich notwendige später ergänzte Arbeit darf zusätzliche Laufzeit kosten; sie muss im Gesamtbudget bleiben.

**C. CORRECTNESS RISK – ein grüner Receipt allein reicht nicht**

**C1 – „Erster BUY“ braucht Transaktions- und interne Ausführungsordnung.** Fundstellen: [Ordering-Ziel, Zeile 54](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:54), G0.6/G0.8.

Die Unterlagen vergleichen Launch-TX N und unseren nachfolgenden BUY. Four dokumentiert aber einen Creator-Prebuy; Flap dokumentiert `buyOnCreation`. Eine eigene spätere Transaktion kann keinen Kauf überholen, der bereits innerhalb derselben Launch-Transaktion ausgeführt wird. Das ist eine Transaktionsgrenze, kein lösbares Mikrosekundenproblem. [Four Create Guide](https://raw.githubusercontent.com/four-meme-community/fourmeme-docs/main/docs/create-guide.md), [Flap Portal-Interface](https://docs.flap.sh/flap/developers/token-launcher-developers/launch-token-through-portal).

Minimale Korrektur: Je Modus definieren, ob das Ziel der allererste Kauf oder der erste separat ausführbare externe BUY nach der vollständigen Aktivierungstransaktion ist. Creator-/interne Käufe und deren Preiswirkung separat mitzählen. Blockhash, TX-Index, relevante Log-/Call-Reihenfolge und Initialzustand nach dem gesamten Launch erfassen. `receipt.status == 1` um verifizierten Venue-Kaufeffekt, richtigen Token, richtigen Empfänger, positive tatsächlich erhaltene Netto-Tokenmenge und tatsächlich bezahlten Betrag ergänzen. Reorgte Erfolge separat zurücknehmen. Ein gescheiterter innerer Call darf nicht durch einen erfolgreichen äußeren Receipt als Kauf gelten.

Four weist zudem ausdrücklich darauf hin, dass `TokenPurchase.account` den Empfänger meint und das Event-Feld `fee` bei AntiSniperFeeMode nicht alle Gebühren enthält. Entry-Preis daher aus realen Zahlungs-/Tokenänderungen und vollständigen Kosten bestimmen. [Four Trade Guide](https://raw.githubusercontent.com/four-meme-community/fourmeme-docs/main/docs/trade-guide.md).

**C2 – Vorbereiteter State ist vorgeschrieben, konkurrierende Käufe sind noch nicht konkret getestet.** Fundstellen: [Pre-resident state, Zeile 125](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md:125), [G0.6, Zeile 203](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:203).

Nonce/Gas/Budget im RAM zu halten ist notwendig, definiert aber nicht die Gültigkeit bei zwei gleichzeitig entstehenden Coins. Zwei unterschiedliche Käufe mit derselben Wallet-Nonce konkurrieren; ein Kauf mit Nonce n+1 hängt am Vorgänger n. Änderungen an Tokenadresse, Nonce, Value oder Gebühren erfordern bei entsprechend geänderten signierten TX-Daten eine neue Signatur.

Minimale Korrektur: Schon im kleinen Harness explizite Fälle für zwei parallele Launches, doppelte Signale, mehrere Sendewege, hängende Vorgänger-Nonce, unbekannten Sendestatus und erschöpftes reserviertes Budget testen. Eine eindeutige Intent-/Nonce-Zuordnung und klare Zulassungsregel reichen zunächst; kein voller Produktions-Reorg-Manager erforderlich. Nach unklarem Sendestatus nicht blind mit neuer Nonce denselben Kauf erneut senden. Stale State und fehlende Voraussetzungen führen zu einem gezählten SKIP.

**C3 – Bundle-Optionen und Gültigkeitsende sind nicht fest eingefroren.** Fundstellen: [G0.5/G0.6](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:198). Die aktuelle 48Club-Dokumentation nennt unter anderem `noMerge`, `positionFirst`, `revertingTxHashes` und `maxBlockNumber`; letzterer hat einen Default von aktuellem Block plus 100. `backrunTarget` ordnet den Target vor, ist aber allein kein Nachweis des gewonnenen frühesten Kaufs. [48Club Send Bundle](https://docs.48.club/puissant-builder/send-bundle).

Auswirkung: Späte oder anders platzierte Aufnahme kann als erfolgreicher Geburtseinstieg fehlgedeutet werden. Ein öffentlich verbreiteter Roh-BUY hat nicht automatisch dieselbe Gültigkeitsbegrenzung wie der Bundle-Umschlag.

Minimale Korrektur: Pro Adapter Reihenfolge-/Merge-/Revert-Regeln, konkrete Gültigkeitsgrenzen und sicheren Fallback einfrieren und experimentell nachweisen. Keine behauptete allgemeine Adjazenzgarantie aus einem ACK ableiten. Abgelaufene Versuche weiter auf späte Aufnahme prüfen. Mehrwegversand derselben TX nicht ohne zusätzliche Evidenz als Beweis eines einzelnen Gewinner-Adapters werten; Vergleichsversuche unter vorab festgelegten vergleichbaren Bedingungen durchführen.

**D. MISSING PROOF – diese Experimente entscheiden über den nächsten Schritt**

**D1 – Der Flap-Snapshot ist veraltet; G0.1 muss tatsächliche Deployments einfrieren.** Fundstelle: [Zeile 126](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:126). Dort steht v5.8.6. Die am Prüftag abrufbare offizielle Tabelle nennt für BNB Portal v5.23.1. Das ist ein Dokumentationsbefund, noch keine unabhängige On-chain-Verifikation. [Flap Deployment-Tabelle](https://docs.flap.sh/flap/developers/deployed-contract-addresses).

Minimale Korrektur: Den alten Wert als historischen Snapshot kennzeichnen. In G0.1 Chain-ID, Blocknummer/-hash, Proxy/Implementation soweit anwendbar, Codehash, ABI-/Quellstand, Selector und tatsächlich getestete Modi gemeinsam einfrieren. Änderungen invalidieren die zugehörige Qualifikation. Nicht bloß eine Versionsnummer austauschen und dann Mainnet-Kompatibilität behaupten.

**D2 – Actionable Ingress und Inclusion müssen gemeinsam qualifiziert werden.** Fundstellen: [G0.2–G0.5](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:179), [Earliest actionable ingress](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md:94).

Ein eigener geth sieht nicht zwangsläufig den Launch früh genug. Die 48Club Auction Feed-Dokumentation beschreibt dagegen simulierte Logs/State-Daten vor Aufnahme, gefilterte Topics einschließlich Four.meme Create und Targets, die auch Bundles sein können. Daraus folgt eine zu testende zusätzliche Eingangsquelle, keine bereits bewiesene vollständige Flap-/Four-Abdeckung. [48Club Auction Feed](https://docs.48.club/puissant-builder/auction-transaction-feed).

Minimale Korrektur: Je Venue/Modus eine Input-Tabelle erstellen: jedes BUY-Pflichtfeld, früheste Quelle, Vorberechenbarkeit, benötigter State, Invalidierungsregel und verbleibende Arbeit nach dem Signal. Lokale Pending-TX und geeignete verifizierte Builder-Push-Evidenz mit dem passenden Inclusion-Pfad testen. Simulierte Daten bleiben vorläufig; ihren erfolgreichen Kauf bestätigt ausschließlich die kanonische Ausführung. Nur „vor unserem lokalen Blockempfang gesehen“ beweist noch kein numerisches Pre-T0-Zeitfenster.

**D3 – Die Hardware- und Laufzeitthese ist plausibel, aber ungemessen.** Fundstellen: [Target host, Abschnitt 11](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md:255), G0.3. Es gibt keinen nativen Harness und keine dafür gemessene Hardware-Konfiguration auf Main. Ein begrenzter, vorinitialisierter lokaler Decode/Build/Sign/Egress-Pfad unter 1 ms ist eine sinnvolle technische Hypothese. Weder Rust als Sprache noch CPU-Pinning beweisen sie; insbesondere nicht jede Lastspitze.

Die [historischen Solana-Daten](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/RESEARCH_STATUS_2026-09-23.md:1450) zeigen bereits Sub-ms-TX-Build/Sign/Serialisierung bei wesentlich späterer beobachteter Verarbeitung. Die [Phase-A-Messung](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/phase_a/bench_post_fix.json) endet bei PAPER-Enqueue. Beides ist kein BSC-Sende- oder Kaufnachweis. Ebenso lässt sich die Solana-Replay-EV-Kurve nicht als BSC-3-ms-Grenze übernehmen.

**E. ALREADY CORRECT – beibehalten**

- Gate 0 vor dem großen Produktionsbau und kontrollierte Mainnet-Prüfung vor dessen Freigabe.
- Tatsächlicher Erwerb und Ordering als Produktziel; ACK, Simulation und „same block“ allein reichen nicht.
- Direkte Contract-Ausführung ohne Website. Four dokumentiert `buyToken`/`buyTokenAMAP`, Flap `swapExactInput`; die Hot-Path-freie Verfügbarkeit aller konkreten Inputs muss weiterhin bewiesen werden. [Four Trade Guide](https://raw.githubusercontent.com/four-meme-community/fourmeme-docs/main/docs/trade-guide.md), [Flap Trade Tokens](https://docs.flap.sh/flap/developers/wallet-and-terminal-and-bot-developers/trade-tokens).
- Vorinitialisierte Templates, Signer, Connections und State; fehlende notwendige Daten führen zu SKIP.
- Kein synchroner RPC/Quote/DB/Filesystem-Zugriff im kritischen Pfad, eine Rust-Semantik für BSC, Offline-Python, begrenzte Queues und sichtbarer Overflow.
- Solana bleibt bestehen. Es wird nicht aus Symmetrie neu geschrieben.
- Nur eine separat qualifizierte Venue wird promoviert; der [letzte Roadmap-Abschnitt](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_DUAL_VENUE_ROADMAP.md:389) regelt das bereits. D/E und F sind entsprechend nur für freigegebene Venues erforderlich.
- G0.9/G0.10 prüfen die Builder-Abhängigkeit einschließlich Proposer-Reichweite. Ein eigener Builder wird nicht bloß deshalb als gleichwertig angenommen, weil seine Software läuft.
- Strategy C hat schon einen [expliziten Null-Beobachtungsfenster-Pfad](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/darkbot/strategies/strategy_c_legacy_backend.py:233). Ein späteres BSC-Birth-Profil muss seine Inputs rechtzeitig kennen; Profile mit nachträglichen Beobachtungsfiltern dürfen nicht als First-BUY-Profil qualifiziert werden.

**Antworten auf die sechs Freigabefragen**

1. **Ist die Roadmap technisch umsetzbar?** Als gestufter Machbarkeitsnachweis ja. Nicht jede Venue/jeder Launch-Modus muss bestehen. G0.5 umfasst zunächst trockene Adapterqualifikation; werttragende Rennen beginnen erst nach G0.6 innerhalb der autorisierten G0.7-Tests.
2. **Verhindert ein Architekturfehler das Ziel grundsätzlich?** Für einen frühen separat ausführbaren externen BUY ist kein solcher zwingender Architekturfehler belegt. Für „jeden Coin garantiert binnen 0–1 ms gekauft“ bietet der Plan keine Garantie. Unbekannte/private Launches, Creator-Käufe innerhalb der Launch-TX und externe Blockauswahl sind tatsächliche Grenzen, keine durch einen Rewrite beseitigten Verzögerungen.
3. **Ist lokal ≤1 ms realistisch?** Als messbares Ziel für den kleinen vorinitialisierten Pfad plausibel, derzeit unbewiesen. Als garantierte Zeit bis zum tatsächlichen Kauf nicht aus der Architektur ableitbar.
4. **Was muss vor C0 bewiesen sein?** Verifizierte Contract-/Modus-Fakten; vollständige rechtzeitige BUY-Inputs; echte Sequential Validity; State-/Nonce-Korrektheit; vollständige lokale Egress-Messung; repräsentative Coverage; wiederholte kanonische Erwerbs-/Ordering-Belege unter vorab festgelegter Kosten- und Erfolgsanforderung. Freigabe ausschließlich für den so qualifizierten Venue-/Modus-/Pfad-/Größenbereich.
5. **Was muss vor G0.1 geändert werden?** Kein weiterer Architektur-Neustart. Den veralteten Flap-Snapshot markieren und G0.1 mit der verbindlichen Erstellung des Input-/Evidence-/Abnahmemanifests beauftragen. Die hier genannten Bedingungen müssen vor den betreffenden Benchmarks/Mainnet-Tests feststehen, statt rückwirkend ergänzt zu werden.
6. **Verhindert Gate 0 einen weiteren falschen langen Anlauf?** Der Mechanismus reduziert dieses Risiko erheblich. In der aktuellen Form lässt der unbestimmte PASS-Vertrag noch falsche Freigaben zu. Mit quantitativer Abnahme, echten Erwerbsbelegen und verbindlicher Produktions-Requalifikation kann er den großen Bau wirksam sperren, solange das Produktziel nicht bewiesen ist. Er garantiert weder jede zukünftige First-BUY-Position noch Profitabilität.
