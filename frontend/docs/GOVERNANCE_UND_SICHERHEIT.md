# DarkBot Frontend – Governance und Sicherheitsgrenzen V1.3

Status: SOLL; keine LIVE-Freigabe. Wire in GATEWAY_WIRE_PROTOCOL_V1_3.md. Master in FRONTEND_PHASENPLAN.md.

## 1. Fach-Owner
Project_0 Solana/Marginfi ist einziger Owner von P0 State/Economics/Funding/Wallet/Execution und binärer .p0j Evidence. BSC ist einziger Owner von G0.6a/G0.7/G0.8, Portal/Builder/Gas/Wallet/DBREC-BSC-Journal. Eigene Solana-Route bleibt getrennt. DarkBot Gateway besitzt nur Anzeige- und Command-Transport, niemals eine zweite Trading-, Reconciliation-, Pricing-, Wallet- oder Freigabeengine. Kein gemeinsamer Signer, keine gemeinsame Walletreservation, keine projektübergreifende Execution.

## 2. Projektgates und Prioritäten
Project_0 Single-Liquidator-FIRST: C0-E1 4h, JFI, PERSIST, SK, FE-00…07 unverändert; FE-06 nicht autorisiert, eigener positiver Realized-Net-Profit nicht nachgewiesen. BSC G0.7/G0.8 nicht ausgeführt, Portal v5.25 drift, V4-Guard UNARMED; neue Requalifikation und ausdrückliche Nutzerfreigabe nötig. Frontend-F0…F7 weder neuer Blocker für P0 4h noch Ersatz für BSC Gate0. Multi-Bot F8 später, erst nach qualifiziertem Einzelbot und explizitem neuem Auftrag.

## 3. Sicherheits- und Commandgrenzen
Same-origin Gateway liefert SPA/API/WS; Session HttpOnly, SameSite Strict, Secure bei HTTPS, begrenzte Laufzeit; Host-/Origin-Allowlist beim Upgrade und HTTP POST; CSRF Token bei Mutation; CSP connect-src self und authentisierte Gateway↔Engine IPC. Gateway prüft Transportberechtigung, ENGINE prüft bei jeder mutierenden Aktion erneut native Projekt-/Wallet-/Chain-/Limit-/Approval-/Version- und Durability-Gates. Idempotency-Key/Receipt durable Engine Owner; Ack ≠ ausgeführt. UNKNOWN nach verlorener Antwort ist kein Repeat-GO.

## 4. Daten-/Evidenz-Regeln
Sechs Marginfi-Instruktionsmarker, vier Liquidationsklassen und vier Related Ereignisse sind REPORT-Filter, niemals Rohcapture-Unterdrückung. off/index/strict/crosscheck unterscheiden. Finanzdaten nur aus finalisierten, bewerteten Ledgern; P0 Overview und Wallets gleiche finance_snapshot_id. BSC- und P0-Wallets getrennt; globale PnL/Capital-Summe nur bei vergleichbarer USD-Preiszeit, Finality, Liabilities und ohne Transfers double count; sonst Aggregate unavailable. Unbekannt als UNKNOWN/—, nicht als 0. OFFLINE/TEST_ONLY-Replay darf nicht als LIVE/Echtprofit gelten.

## 5. Variables/Secrets
SOURCE_VARIABLE_MATRIX.md bindet jedes UI-Feld an Source-Owner, native Einheit, Typ, Provenienz, Gate. Geheim: BSC Buyer/Launcher Private Keys, Builder Authorization, tokenisierte RPC URLs, P0 Wallet Keys/Secrets; nie React, VITE_*, Proto-Responses oder CI Logs. BSC G0.7 signierte Felder sind readonly gebunden, keine willkürlichen GUI-Slider/Live-Toggles. Draft nicht gleich Bot.

## 6. Design und Isolation
Project_0 Graphite V0 fünf Pill-Tabs und Wallets (Balances/Send/Auto-Sweep + Zeilenfolge) visuell eingefroren; BSC teilt Design-Tokens, nicht Business-Logik. ESLint cross-feature-import boundary und per-domain Zustand-Stores. Frontend-Bereinigung betrifft nur frontend/docs, niemals Engine, Projektstatus, Evidence, Signer oder BSC-/P0-Dokumente. Alle Freigaben ausschließlich nach projektinternen Phasenplänen und ihrer echten Evidence.