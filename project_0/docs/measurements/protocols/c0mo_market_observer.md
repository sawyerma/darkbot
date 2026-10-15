# Unfiltered Live Market Observer (C0-MO)

> Measurement protocol. Moved unchanged from `project_0/README.md` §17 (commit `4c271cb`); only this heading and note were added.

**Zweck.** Ungefilterte, read-only Beobachtung realer Marginfi-Mainnet-Aktivität. Läuft im bestehenden
`p0-longtail` als `P0_MODE=market`; zweigt vor jedem Trading-Objekt ab (kein Keypair, kein Signer, keine
Simulation, keine Submission, keine Health-/Candidate-/P1-C-/F1-/F8-/Routing-/Funding-Filter).

**Start.**

```bash
cd "/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"
P0_MARKET_DURATION_SECS=14400 project_0/scripts/run_market_observer.sh                          # Helius Developer+
P0_MARKET_SOURCE=logs P0_MARKET_DURATION_SECS=14400 project_0/scripts/run_market_observer.sh    # Helius Free
```

Smoke-Test: `P0_MARKET_DURATION_SECS=600`. Commitment: `P0_MARKET_COMMITMENT=finalized` (Default; `confirmed`
nur für Smoke-Tests). Endpunkte: explizites `P0_WS_URL`/`P0_RPC_URL` hat Vorrang; sonst leitet `HELIUS_API_KEY` sie ab
(`transactions` → `wss://atlas-mainnet.helius-rpc.com`, `logs` → `wss://mainnet.helius-rpc.com` + RPC).
Helius Free → `P0_MARKET_SOURCE=logs`; Helius Developer+ → `P0_MARKET_SOURCE=transactions` (bevorzugt). Ob der Provider
`transactionSubscribe` annimmt, beweist erst der Subscription-ACK im Smoke. Ohne RPC bei `logs` ist das Enrichment
nicht verfügbar; Capture, Summary und Report laufen trotzdem. Unter `P0_MARKET_MIN_SAMPLE` (Default 30) Liquidationen
meldet die Summary `INSUFFICIENT_SAMPLE`: dann keine Verteilungs- oder Tageshochrechnung.

**Ablauf.** (1) Capture bis `t_rx(StreamReady) + Dauer` auf der monotonen Ingress-Clock, ohne jeden RPC-Aufruf;
(2) Ingress stoppen und Queue drainen – Events mit `t_rx ≤ Cutoff` zählen, spätere nicht; (3) nur bei `logs`:
begrenztes `getTransaction`-Enrichment der beobachteten Liquidationen (ein Fehler des Enrichment-Clients verhindert
nie Summary und Report); (4) Summary. Die Market-Session sendet einen Transport-Heartbeat (WebSocket-Ping,
`P0_MARKET_HEARTBEAT_SECS`, Default 30 s); `slotSubscribe` läuft nur als Slot-/Stream-Telemetrie mit.

**Output** (`project_0/data/market/market_<UTC_RUN_ID>/`, mit `P0_RUN_STORE=runs` unter `project_0/data/runs/`; nicht committen): `transactions.jsonl`,
`liquidations.jsonl`, `coverage.jsonl`, `summary.json`, `final_report.txt`.

**Verdicts.** `MARKET_DATA_VALID` (vollständige Aufnahme **und** verifizierte Quelle; derzeit ist keine Quelle
verifiziert) · `MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED` (vollständige Aufnahme; ob Transaktionen, die das Programm nur
über eine Address Lookup Table referenzieren, geliefert werden, ist weder für `transactions` noch für `logs` belegt) ·
`MARKET_DATA_INCOMPLETE` (Gap, Overflow, Fatal, fehlende Logs, Decoder-Fehler, nicht qualifizierte oder abweichende abgeschnittene Logs, Drain unvollständig,
vorzeitiges Ende) · `MARKET_OBSERVER_STREAM_NOT_READY`. Enrichment-Vollständigkeit (`enrichment_complete`,
`fee_totals_complete`) wird getrennt ausgewiesen und ändert die Gross Edge nie.

**Bedeutung.** Market Observer misst Marktaktivität und Markt-Gross-Edge (`seized − repaid`, USD).
Er misst NICHT: DarkBot Capture Rate, DarkBot Executable Edge, DarkBot realized PnL, P1-C-Profitabilität.
