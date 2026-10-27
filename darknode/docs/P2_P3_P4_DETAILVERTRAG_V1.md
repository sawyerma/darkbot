# DarkNode – P2/P3/P4: Handover, Node-IR und Backtest-Qualifikation V1

Stand: 27.10.2026. Status: spezifiziert, Implementierung und Tests NOT_RUN. P1.5–P1.7 bleiben allein zuständig für L0/L1/L2/Rohtrades/Barbildung; dieser Vertrag erzeugt **keinen zweiten Speicher-/Bar-Owner**.

## P2. Historie → deterministischer Live-Handover

**Input:** P1 InstrumentVersion, qualifizierte Trade-ID-/Timestamps-/Sequenz-Streams, finalisierte 1s-/MTF-Barversionen und Coverage-/Gap-Manifest. Ein Barfenster ist halboffen [start_UTC,end_UTC); nur finalisierte Bars und eindeutig definierte Partial-Bar-Policy gelangen in einen barclose-indizierten Graph.

**Warmup:** Für jeden Node das maximale benötigte Fenster über alle Timeframes einschließlich Indikatorzustand, vorangehender Cross-Werte und vorlaufender Event-Flags berechnen. Warmup zuerst aus qualifizierter Historie; unzureichende Vergangenheit, Revisionskonflikte, nicht eindeutige Trade-Order, vorübergehender Missing-Data-Status und neu startender Indikator geben INVALID/NO_ENTRY aus. Keine einfach erfundenen Indikator-Startwerte.

**Cutover:** P1 publiziert logische Watermarks/Finality je Symbol/TF. Backfill bis zur bestätigten Live-Grenze, Liveereignisse während der Überlappung mit IDs puffern, deduplizieren/ordnen; ein Event wird **genau einmal** in den Indikatorzustand übernommen. Alte REST-Bar- und WS-Bar-Ergebnisse dürfen bereits verifizierte Originaltrade-Reihenfolge nicht verdecken. Revision/late trade erfordert explizite Replay-/Invalidate-Policy, keine stille rückwirkende Handelskorrektur.

**Restore:** graph_version, instrument_version, P1-watermark, source/capture-ID, node state_hash und nonce-Rearm werden konsistent gesichert; nach Reboot gegen replayerzeugten Signal-/Bar-Trace vergleichen. Bei Konflikt kein neues Signal; offene Order-/Position wird unabhängig über P5 reconciliiert.

**P2-Golden/Negativmatrix:** gleiche ms-Timestamps, Overlap-Duplikate, Hole über Warmup, Partial-Bar, Stunden-/Tageswechsel UTC, WS-Reconnect, Backfill-Response Out-of-Order, kalte und warme Engine, late trade/Gap, Source-Gap mitten in 5s-Window, kurz vor Signalflanke und per-Node-Historical-Finality. PASS nur bei identischem Signal-Trace und belegtem NO_ENTRY unter Defekt.

## P3. Typisierte Node-Registry, unabhängige Indikatorvalidierung und Strategy-IR

- Ein **GraphCompiler**, ein **Evaluator** für Backtest/Live. Nicht Frontend-Node-Execution. Graph als DAG, Knoten mit stabiler NodeType-ID und Version, Instance-ID, typisierten Ports, Skala/Einheiten (raw Williams -100…0 vs norm 0…100), TF/Clock, Warmup und INVALID-Policy. Ungültige Kanten/Cycles/Einheiten/Doppelausgänge blockieren das Publish/Activate; optionale Eingänge nur ausdrücklich im Typvertrag.
- V1 Gruppen exakt Marktdaten, Indikatoren, Bedingungen, Aktionen, Risiko; erste echte Typen CandleSource, Williams %R, Andean, BX, Compare/Cross, AND/OR/NOT/XOR (ggf. NOR/NAND), Confirm und Long/Short/Exit; Risk-Policy wird vom Order-Owner geprüft. Kein Benutzer-Code-Node, keine zweite Simple/Advanced-Execution-Engine.
- **Bool3:** TRUE/FALSE/INVALID mit dreiwertiger Wahrheitstabelle und Fail-Closed Entry. Keine logische Negation von INVALID→TRUE; AND(FALSE,INVALID)=FALSE kann zur vollständigen Wahrheitstabelle gehören, aber INVALID darf nie einen Entry auslösen. Graph-Compiler trennt Validate-Fehler und Dateninvalidität. Für alle Operatoren vollständige Tabelle testen.
- **Cross-Policy:** Vorgängerwert und aktuelle *finalisierte* Kerze/Signal-Flanke nach explizit versionierter EvaluateClock; auf Startup keine künstliche Cross-Flanke; Bar-Close nur einmal je TF; 5s in 1m-MTF darf keine Zukunfts-1m-Kerze lesen. Zwischen identischen TRUE-Bars nicht dauernd neu enter; Rearm erst nach bestätigtem Exchange-FLAT und klar definierter Re-Entry-Flanke.
- **Action-Priorität:** Schutz-/Emergency-Owner vor neuen Entries; Exit vor entgegengerichtetem Entry; bei LONG+SHORT gleichzeitig qualifizierter HOLD, keine automatische Hedge-Position. BOTH erlaubt Richtungsauswahl, nicht parallele Gegenpositionen. Indikatorwerte niemals signierten Request erzeugen; nur typisierte Intents.
- **Immutable Graph:** normalisierte Knoten-/Kanten-/Paramter-/Plugin-Typversion, TF und Evaluationsregeln eindeutig serialisiert, Hash aus kanonischer Form, Save-Draft→Compile/Validate→Backtest→Qualify→Activate mit Compare-and-Swap/Autorisierung. Änderung an Grenzwert, Einheit, Warmup oder Priorität erzwingt neue Version und neue Qualification; React darf alte laufende Graphversion nicht mutieren.

**Drei V1-Golden-Drafts (eigene Beispielregeln, keine qualifizierte Strategie):** N1 Williams norm >70 LONG/<30 SHORT, separate LongExit <55 / ShortExit >45; N2 Williams+Andean+BX Bool3 AND; N3 CrossAbove(70)/CrossBelow(30) und separate Exit-Crosses. Validierung anhand eigenständig spezifizierter Formeln, Normierung, Warmup und Grenzfälle; Beispielregeln nicht mit einer validierten Handelsstrategie gleichsetzen.

**Unabhängige Validierung:** Formeln, Algorithmusvarianten, Eingaben und Parameter eindeutig versionieren; Python optional für getrennte numerische Kontrollrechnungen; Golden-Datei enthält vollständige Zeitreihe inkl. Uhrzeit, Quellbars, Node-Ausgänge, False/Invalid, Signals, GraphHash, Replay-Hash, Toleranz je Indikatorklasse. Last Price/Mark Price und BarClose/TickEvent getrennt, keine beliebige numerische Rundungsgrenze. Regression von NaN/Overflow/Unbounded/Division-by-zero; Property/Fuzz und Golden Tests für alle Dateigrenzen.

## P4. Kostenbewusste, reproduzierbare Simulation

- **Gemeinsamer Kern:** P3 evaluator/effective policy identisch in Backtest/Live; getrennt nur das *Execution-Port*-Interface (historischer Broker-Simulator vs Bitget). Kein Live-Spezial-Branch der Indikatoren. Jeder Versuch fixiert Source-/Engine-/Graph-/Instrument-/Fee-/FillModel-Version/Hash und Dataset-Coverage.
- **Fillstufen:** (1) Candle only für grobes Research, fehlende intrabar Ausführungsreihenfolge ausdrücklich UNKNOWN; bei SL+TP in derselben Bar konservativer Worst-Case, kein erfundener Fill. (2) Original-Trade-Replay mit fixierter Tick-Order, realitätsnahem Spread und Slippage; Orderbuch/L2-Stufe nur nach glaubwürdiger L2-Capture-/Queue-Simulation. Kein Candle-Return als Beweis realer Stop-/Market-Fills.
- **Kosten:** kontospezifisch maker/taker, Ein- plus Ausstieg, Spread, instrumentabhängige Tick/Lot, Marktimpact/Adverse Selection, Funding-Zahlung am tatsächlichen Termin, Exchange-/Management-/Rundungsfehler, Preisgap-Liquidationsrisiko. Ein *Demo-Fill* beweist keine Live-Slippage.
- **Cash/Position:** Base-Quantity, Quote-Notional, margin, realised/unrealised PnL, Entry/Exit-Fee, Funding und Equity sauber mit Einheiten und Kontowährung. P5-Risk-Budget-Formel mit identischen Fee-/Stop-Reserven; max Drawdown inkl. offener Verbindlichkeiten. Negative Tests für Tick/Lot, negative PnL, Mark/Last-Trigger und Stop/Liquidation vor geplantem SL.
- **Statistik:** In-sample/OOS/walk-forward mit *vorab festgefrorenen* Splits und Signal-Parametern; Uncertainty/Trading-Frequenz, Kosten-Break-even, Negativmärkte und Sensitivität auf Slippage/Non-Fill. Keine OOS-Leakage durch Label-Features, MTF oder überlappende Triple-Barrier-Outcomes. Keine fixe erfundene Mindest-Sharpe oder Edge; Grenzwerte und Stichprobenanforderungen vor der Qualifikation versionieren.
- **Outputs:** Event-Trace pro Bar/Trade/Signal/Riskdecision/virtual order/fill/exit, Reason Code, DataQuality, FeeModel, hashes, reproducible command, Net PnL in quote currency, OOS-Gate verdict. Ergebnislabel RESEARCH/QUALIFIED/REJECTED; eine profitable Kurve ist **keine** P5-Order-/P9-LIVE-Erlaubnis.

## Exit-Kriterien

P2: gleiche Warmup/Handover-Inputdaten → bitgleich gleiche Indicator Inputs/Signals; bei Gap NO_ENTRY. P3: Graph-Compiler-/Bool3-/Node-Version-/mathematische Golden-Tests bestanden. P4: reproduzierbare Replay-/Fee-/No-Lookahead-Goldens, dokumentierte Fillmodel-Grenzen und gesonderter ökonomischer OOS-Report. Bis echte Läufe vorliegen alle NOT_RUN.