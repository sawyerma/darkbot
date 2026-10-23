# DarkBot BSC – Early Launch Detection und On-Chain-BUY

**Verbindlicher Plan:** [PHASENPLAN.md (v7)](docs/1_PLAN/PHASENPLAN.md) · **Status:** G0.7/G0.8 vor Produktions-LIVE · **In Arbeit:** ein nativer Rust-BSC-Core, zusätzliche früheste Signaleingänge sind geplant.

DarkBot BSC soll fremde Flap- und Four.meme-Launches möglichst früh erkennen, einen validen BUY rechtzeitig vorbereiten und **als erster unabhängiger externer Käufer direkt auf der BSC ausführen**. Es gibt keinen Trading-Website-, Frontend- oder Browser-Schritt im BUY-Pfad.

**Erfolg = BUY tatsächlich auf der Chain ausgeführt und Token im Wallet-Chain-State erhalten.** Weder das Abschicken noch ein Builder-ACK noch eine Wallet-Anzeige allein zählen als Kauf.

## Architektur – fünf parallele Signaleingänge, eine gemeinsame Engine

GitHub rendert dieses **Mermaid-Blockschaltbild direkt**. Es zeigt die **verbindliche Zielarchitektur** aus Phasenplan §5a, nicht den derzeit bereits produktiv verdrahteten Zustand.

```mermaid
flowchart TB
    CHAIN["BSC P2P / Pending-Launch-Transaktionen"]

    subgraph SOURCES["FÜNF UNABHÄNGIGE FRÜHERKENNUNGSQUELLEN"]
        direction LR
        G["Eigene geth-Fast-Lane<br/>OFF / SHADOW / LIVE"]
        P["PublicNode WSS<br/>OFF / SHADOW / LIVE"]
        D["dRPC WSS<br/>OFF / SHADOW / LIVE"]
        B["BlockReq WSS<br/>OFF / SHADOW / LIVE"]
        F["48Club Auction Feed<br/>OFF / SHADOW / LIVE"]
    end

    CHAIN --> G
    CHAIN --> P
    CHAIN --> D
    CHAIN --> B
    CHAIN --> F

    G --> N["Gepinnter geth-Hook<br/>native binäre IPC"]
    P --> W["Isolierte WSS-Protokolladapter<br/>Health / Rate-Limits je Quelle"]
    D --> W
    B --> W
    F --> PRIV["Privater Feed-Adapter<br/>Auth / Target-Semantik"]

    W --> V["Externes Protokoll dekodieren<br/>prüfen und SOFORT binär übergeben"]
    N --> BIN["Gemeinsame native Binär-Eingangsgrenze"]
    V --> BIN
    PRIV --> BIN

    BIN --> MODE{"Quellenmodus und<br/>Qualifikation?"}
    MODE -->|"OFF"| OFF["Kein Kandidat"]
    MODE -->|"SHADOW / unqualifiziert"| OBS["Nur Beobachtung<br/>binäres Journal"]
    MODE -->|"LIVE + gesund + qualifiziert"| FIRST["FIRST VERIFIED ACTIONABLE WINS<br/>früheste vollständig nutzbare Information"]

    FIRST --> DEDUP["Launch-Identität / Deduplizierung<br/>ein Launch = ein Intent"]
    DEDUP --> CORE["EIN RUST-TRADING-CORE<br/>Venue / Tokenadresse / State<br/>Quote / Kosten / Risiko"]
    CORE --> SAFE{"Globale wirtschaftliche Freigabe<br/>Safety / Budget / Nonce PASS?"}
    SAFE -->|"NEIN"| STOP["Kein BUY"]
    SAFE -->|"JA"| SIGN["BUY aufbauen und signieren<br/>T_ready"]
    SIGN --> OWNER["Ein Intent- und Nonce-Owner<br/>T_queue und T_send messen"]

    OWNER --> E1["Local geth / P2P"]
    OWNER --> E2["48Club Builder"]
    OWNER --> E3["BlockRazor Builder"]
    E1 --> PROD["Builder / Validator<br/>Launch N → unser BUY N+1"]
    E2 --> PROD
    E3 --> PROD

    PROD --> BOUGHT["T_buy – BUY erfolgreich ausgeführt<br/>Token dem Wallet im Chain-State zugeordnet"]
    BOUGHT --> VERIFY["G0.8: On-Chain-Kontrolle<br/>Token / Menge / N+1 / Kosten<br/>Konkurrenz / T0-Witness"]
    VERIFY --> OBS

    classDef input fill:#dbeafe,stroke:#2563eb,color:#1e3a8a
    classDef core fill:#dcfce7,stroke:#16a34a,color:#14532d
    classDef safety fill:#fef3c7,stroke:#d97706,color:#78350f
    class G,P,D,B,F input
    class FIRST,DEDUP,CORE,SIGN,OWNER,BOUGHT core
    class MODE,SAFE,STOP safety
```

### Technisches Zusammenspiel

1. **Alle fünf Quellen gleichzeitig.** geth, PublicNode, dRPC, BlockReq und 48Club beobachten parallel. Keine Quelle hält eine schnellere auf. 48Club ist ein gesondert zu qualifizierender privater Feed und wird nicht als kostenlos vorausgesetzt.
2. **Externe Protokollgrenze, danach ausschließlich binär.** Ethereum-WebSockets liefern JSON-RPC als **fremdes Netzwerkprotokoll**. Nur der isolierte Provider-Adapter dekodiert diesen Wire-Frame und erzeugt ein geprüftes, typisiertes **Binärereignis**. **Kein internes JSON** – weder IPC, State, Konfiguration, Journal noch HOT0. Die geth-Fast-Lane liefert bereits native binäre IPC.
3. **Individuelle Quellenschalter.** OFF beendet ihren Kandidaten-Ingress; SHADOW empfängt und misst ohne BUY-Auslösung; LIVE darf **nur qualifizierte** Signale an den Core liefern. Jeder Provider hat getrennten Health-/Rate-Limit-/Reconnect-Zustand. Ein LIVE-Signal **schaltet niemals den globalen Handels-Guard scharf**. Notwendige Chain-/Reorg-/Safety-Dienste dürfen nicht mit der geth-Trade-Quelle versehentlich abgeschaltet werden.
4. **First Verified Actionable Wins.** Nicht der früheste nackte Hash, sondern die früheste **vollständig und unabhängig als kaufvorbereitungsfähig verifizierte** Information gewinnt. Keine serielle Providerabfrage und kein Remote-Lookup im nativen HOT0. Doppelte Quellenmeldungen erzeugen keinen zweiten Intent.
5. **Nur eine Trading-Engine.** Derselbe Rust-Core verantwortet Venue-/Deployment-Verifikation, Tokenableitung, Quote, Risiko, Signatur, Intent, Nonce und Exposure. Der BUY geht anschließend über einen **separat qualifizierten** geth-, 48Club- oder BlockRazor-Inclusion-Adapter.
6. **Echter Kauf und unabhängige Prüfung.** Der Produzent führt die Launch-Transaktion und danach unseren BUY aus. G0.8 weist kanonischen Tokenzugang, richtige Walletadresse, Kaufposition N/N+1, konkurrierende erfolgreiche Käufe, Kosten und die getrennte Producer-Timing-Evidence nach.

## Zeitmodell: tatsächlicher BUY im Block, nicht nur abgesendet

```mermaid
flowchart TB
    PRE["T_actionable<br/>Launch-Information verwertbar erkannt"]
    READY["T_ready<br/>gültiger BUY vorbereitet / signiert"]
    SEND["T_send<br/>Übergabe an qualifizierten Transport"]

    subgraph PRODUCER["AUSFÜHRUNG INNERHALB EINES PRODUZIERTEN BLOCKS"]
        direction TB
        START["Launch-Transaktion N ist ausgeführt<br/>T0 = erste externe Kaufbarkeit"]
        END["BUY-Transaktion N+1 erfolgreich ausgeführt<br/>T_buy = Token im Wallet-Chain-State"]
        START -->|"physisches Produktziel: unter 1 ms"| END
    end

    ARRIVE["T_head_local<br/>Blockmeldung später bei unserem Rechner"]
    VERIFY["Receipt / kanonischer Block / Tokenbestand<br/>später unabhängig verifiziert"]

    PRE --> READY --> SEND --> START
    END --> ARRIVE --> VERIFY

    classDef onchain fill:#dcfce7,stroke:#16a34a,color:#14532d
    class START,END onchain
```

**Das 1–2-ms-Ziel darf niemals als bloße Versandlatenz umgedeutet werden.** Gemeint ist: Der tatsächliche BUY muss **beim produzierenden Validator/Builder nach T0 erfolgreich ausgeführt** werden und unseren Tokenbestand im produzierten Chain-State verändern – idealerweise als **direkte Transaktion N+1**. Der verbindliche Grenzwert ist sogar strenger: **T_buy − T0 < 1 ms.** Zwei Millisekunden sind **keine** stillschweigend gelockerte Abnahmeschwelle. Die spätere Anzeige im lokalen Wallet kann länger dauern und gehört nicht zur physischen Kaufzeit.

**BSC-Blockintervall (~450 ms) ist nicht BUY-Latenz.** Das Intervall beschreibt den Rhythmus der Blockproduktion, nicht den Abstand zwischen zwei Transaktionsausführungen innerhalb eines Blocks. **T_head_local ist nicht T0.** Auch 232 ms Vorsprung vor unserer lokalen Blockmeldung beweisen nicht 232 ms Vorsprung vor T0. **N/N+1 beweist die Reihenfolge, aber keine physische Sub-ms-Dauer:** Die benötigt Original-Producer-Ausführungsnachweise; ohne sie gilt die Zeitbewertung **UNKNOWN**.

| Größe | Bedeutung | Beweist tatsächlichen Kauf? |
|---|---|---|
| T_actionable → T_send | Lokale Reaktion und Versand | Nein |
| T0 → T_buy | Tatsächliche erfolgreiche Ausführung nach Launch beim Producer | Strenges Ziel **unter 1 ms**, mit Original-Witness |
| N/N+1 im selben kanonischen Block | Unser BUY direkt nach dem Launch, keine externen erfolgreichen Käufe dazwischen | Beweist Position bei erfolgreicher Akquisition, **nicht** die Zeit |
| T_head_local / lokale Wallet-Anzeige | Späterer Empfang des Blocks / Darstellung des Wallet-Bestands | Weder T0 noch T_buy |

## Entwicklungsstand: Plan und LIVE klar unterscheiden

| Modul | Stand |
|---|---|
| Native geth-Fast-Lane | Bestehende qualifizierte Teilpfade; kein allgemeiner Produktions-LIVE-Nachweis |
| PublicNode | Full-TX-Empfang auf Mac read-only gemessen; noch nicht als Rust-LIVE-Eingang qualifiziert |
| dRPC / BlockReq | Read-only verglichen, teilweise Rate-Limits; keine LIVE-Freigabe |
| 48Club Auction Feed | Berechtigung / Feed-Semantik nicht qualifiziert; kein implizit kostenloser Zugriff |
| Fünf OFF/SHADOW/LIVE-Schalter | Im Phasenplan §5a verbindlich vorgesehen, **noch zu implementieren** |
| G0.7 / G0.8 | **Jetzt vorrangig**: kontrollierter Mainnet-BUY und tatsächliche Token-Akquisitionsprüfung; bis zur gesonderten Freigabe **UNARMED** |
| T_buy − T0 < 1 ms | Unverändertes Ziel, physischer Nachweis noch **UNKNOWN** |

**Verbindliche Reihenfolge:** G0.7 → G0.8 → PASS / ADAPT / STOP → danach C1/C2 und weitere Produktionsphasen. Die fünf Quellen werden nicht als vorgezogene Architekturarbeit umgesetzt. **FREE_ONLY** ist Standard; keine kostenpflichtigen Provider-Abos oder Soul-Point-Kapitalbindung ohne ausdrückliche gesonderte Freigabe.

## Repository-Navigation

| Pfad | Funktion |
|---|---|
| [docs/1_PLAN/PHASENPLAN.md](docs/1_PLAN/PHASENPLAN.md) | **Verbindlicher Phasenplan, Rang 1** – Stop-Regeln, Gesamtfolge, fünf Quellen §5a |
| [docs/1_PLAN/GATE0_TESTPLAN_SHORT.md](docs/1_PLAN/GATE0_TESTPLAN_SHORT.md) | **Abnahmevertrag, Rang 2** – tatsächlicher Kauf und Zeit-/Safety-Evidence |
| [docs/1_PLAN/BSC_PRODUCTION_BUILD_MASTER_PLAN.md](docs/1_PLAN/BSC_PRODUCTION_BUILD_MASTER_PLAN.md) | **Technischer Bauplan, Rang 3** |
| [docs/1_PLAN/STATUS.md](docs/1_PLAN/STATUS.md) / [REVISIONEN.md](docs/1_PLAN/REVISIONEN.md) | Implementierungsstand und Revision v7 |
| [docs/2_REGELN/](docs/2_REGELN/) | Technische Bausteinverträge |
| [docs/3_BEWEISE/](docs/3_BEWEISE/) | Historisches Ledger und getrennte neue Evidence |
| [docs/4_G07_FREIGABE/](docs/4_G07_FREIGABE/) | G0.7-Freigabe-/Requalifikationsunterlagen |
| [docs/5_OFFENE_FEHLER/](docs/5_OFFENE_FEHLER/) | Bug-Register |
| [docs/6_VORSCHLAEGE/](docs/6_VORSCHLAEGE/) | Noch nicht verbindliche Zusatzvorschläge |
| [docs/9_ALT/](docs/9_ALT/) | Historisch, **keine Arbeitsaufträge** |
| [engine/](engine/) | Gemeinsame BSC-Rust-Engine und Tests |
| [geth/](geth/) | Gepinnter bsc-geth und P2P-Fast-Lane |
| [config/](config/) / [tools/](tools/) | Verträge / Diagnose |
| [native/](native/) | Native historische Evidence, keine zweite Trading-Engine |

Die bisherige lange README mit den abgeschlossenen **R1–R13-Refactoring-Aufträgen** wird [unverändert im Archiv](docs/9_ALT/README_BSC_R1_R13_2026-10-16.md) aufbewahrt und ist **keine aktuelle Arbeitsanweisung**.
