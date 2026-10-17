# DarkBot Frontend

**Einzige aktive Roadmap:** [FRONTEND_PHASENPLAN.md](docs/FRONTEND_PHASENPLAN.md). F0 bis F8; SINGLE_LIQUIDATOR_FIRST; das existierende Project_0 Graphite V0 bleibt visuell eingefroren.

## Aktive Dokumente
- [Master-Phasenplan](docs/FRONTEND_PHASENPLAN.md) – einheitliche F0…F8-Implementierung, IST/SOLL, Tests, Abhängigkeiten.
- [Gateway Wire v1.3](docs/GATEWAY_WIRE_PROTOCOL_V1_3.md) – Protobuf, Topic-STATE, Journal-Events, Source-Liveness, zwei Catch-up-Barrieren.
- [Governance & Sicherheit](docs/GOVERNANCE_UND_SICHERHEIT.md) – Freigaben, Commands und Projektgrenzen.
- [Source Variable Matrix](docs/SOURCE_VARIABLE_MATRIX.md) – Project_0 + BSC quellenbasierte Anfangsinventur; kompletter Census vor Code.
- [Eingefrorenes Graphite-V0-Design](docs/V0_GRAPHITE_FLEET.md) – fünf Tabs und Wallets ohne Redesign.
- [Graphite/Finance-Anhang](docs/GRAPHITE_V5_UI_FINANCE_CONTRACT.md) – Finanzfelder und UX, kein älteres konkurrierendes Design.
- [Boot/Backtest/Optimize-Anhang](docs/STARTUP_BACKTEST_OPTIMIZATION.md).
- [Wallets/Treasury-Anhang](docs/WALLETS_TREASURY.md).
- [Historische P0-Code-Map](docs/PROJECT_0_CODE_MAP.md) – nicht blind übernehmen; Source-HEAD prüfen.
- [Archiv ersetzter Frontend-Verträge](docs/delete/README.md) – nur Historie, keine normativen Planvorgaben.

## Architektur
Eine React/Vite-SPA mit / (global overview), /project-0 (fünf Graphite-V0 Tabs), /bsc (eigene fünf BSC-Tabs), /solana (Route erhalten). Strikt getrennte Rust-Engines; Gateway ist read-only + autorisierter Command-Proxy, keine neue Trading-Engine. Kein Browser-Polling: Topic-WS Push, StateFull-Latest, durable Ereigniscursor, Engine-Liveness, Reconnect und gezielte On-demand History.

## Ist-Status
Die Routen und P0 Graphite-V0-Shell existieren; globale Overview ist bislang statisch, BSC nur Platzhalter, Live-WS ist JSON Legacy mit globaler Seq. Protobuf, Gateway, native Adapter, F0-VAR-Census und echtes Backend Control sind noch offen. Keine LIVE-Freigabe durch Frontend-Plan.

## Development
```sh
cd frontend
npm ci
npm run dev
```

Bestehender Vite-Dev-Port 8080, Proxy-Backend 8100 (Konfiguration prüfen). Typecheck, Lint, Build, Smoke und Browser-Tests vor jeder tatsächlichen Gate-Freigabe ausführen und Nachweise erfassen. Git-Docs-Gesamtbereinigung verändert keine Engine.