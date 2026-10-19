# Graphite V7 – freigegebene Designreferenz

Die Designvorlage liegt direkt unter `frontend/design/reference/project0_graphite_v7_fleet.html`. Sie ist eine **statische/interaktive HTML-Designreferenz**, keine zweite React-App und keine LIVE-/Backend-Implementierung.

Die verbindlichen Implementierungsregeln stehen in [Frontend Master-Phasenplan](../../docs/FRONTEND_PHASENPLAN.md) und [V0 Graphite Freeze](../../docs/V0_GRAPHITE_FLEET.md). Fable soll die fünf vorhandenen React-Panels unter `frontend/src/features/project0/` an dieser Vorlage prüfen, nicht neu erfinden. `Wallets` bleibt visuell unverändert; BSC übernimmt lediglich das Designsystem.

**Referenz-Integrität:** Der ursprünglich lokal freigegebene HTML-Stand hat SHA-256 `887aac385b04ca315615844399f42ff611b10de204a5cb9e3bc7ffb2c87eef36`. Die von ChatGPT bereitgestellte Repository-Textkopie ist **nicht als byte-identisch nachgewiesen**; visuelle Abnahme und exakter Hash-Abgleich gegen das Original bleiben erforderlich. Die beiden ursprünglichen Screenshots (`project0_graphite_v7_fleet_liquidator.png`, `project0_graphite_v7_fleet_wallets.png`) sind noch als Referenz lokal verfügbar, aber derzeit nicht im Repository gesichert.

## Global Overview Graphite F7-OVR (2026-10-19)

The new **global** Overview has its own [static HTML reference](darkbot_overview_control_v2.html) and [embedded SVG visual](darkbot_overview_control_v2.svg) derived from the existing shared Graphite tokens and real Overview cards. Its **binding** functional, status and command semantics are in [Frontend Master §10](../../docs/FRONTEND_PHASENPLAN.md). This is a **reference only**; all controls are disabled and demo status text is not live evidence. The original Project_0 V7 reference and Wallets freeze are untouched.
