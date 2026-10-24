# Project_0: F0-VAR — verbindliche Bestandsaufnahme

Basis-Commit: `6ed654f54a970990636cc12013b8cb03fa49ebb4` (Erstfassung). Rohinventar aktuell `9dbb6607` (`F0_VAR_MANIFEST.json`); semantische Differenzanalyse Paket 0 auf `30140710` in [F0_VAR_01_MATRIX](F0_VAR_01_MATRIX.md). **Stand: Paket 0 abgeschlossen, Folgepakete offen. Keine LIVE- oder GUI-Steuerungsfreigabe.**

## Warum

Der Gateway/Reporter-Transport ist für `project0.status` nachgewiesen. Der fachliche Vertrag zwischen der umfangreichen Project_0-Runtime und den Graphite-V0-Ansichten wurde noch nicht vollständig erhoben. Ursprünglich trug `StatusProjection` nur neun Felder; heute sind `SystemStatus` (10) und `CaptureState` (13) der Wire-Vertrag, weitere Owner-Werte laufen weiterhin an der GUI vorbei (siehe Matrix C).

## Scope

Ein Inventar ALLER versionierten `project_0/native/crates/**/*.rs` (auch private `pub(super)`- und andere interne Strukturen), Project_0-Operationsskripte und öffentliche Konfiguration; Gateway-Topic-Registry; alle Project_0-Frontend-Panels/Verträge/Stores/Adapter; `protocol/schemas/**/*.proto` sowie relevante Frontend-Realtime-Verbindungen.

`python3 project_0/operations/f0_var_inventory.py` erzeugt die **vier Rohinventare** und ein Manifest nur unter `project_0/docs/`:

| Datei | Erhebung |
|---|---|
| `F0_VAR_RUST_FIELDS.tsv` | Rust-Deklarationen, Felder, Varianten und Zeilen |
| `F0_VAR_GUI_SURFACES.tsv` | GUI-Labels, Kontrolltexte und TS-Deklarationen |
| `F0_VAR_ENV_REFERENCES.tsv` | literale Umgebungsvariablennamen samt Fundort, keine Werte |
| `F0_VAR_PUSH_ANCHORS.tsv` | Publisher-/Consumer-Anker im Quellcode |
| `F0_VAR_MANIFEST.json` | feststellbare Scan-Abdeckung, Quell-HEAD, Grenzen |

**Wichtig:** Das Tool liefert nur einen reproduzierbaren Lexer-Census. Es darf nicht als fachlich vollständige Runtime-Inventur ausgegeben werden. Private Metadaten, abgeleitete Berechnungen, Makros, zeitliche Phasen und dynamische Abhängigkeiten benötigen Code-Review. Kein Scanner darf `.env`-Werte, API-Keys, Private Keys oder Wallet-Secrets in Dokumente schreiben.

## Fachliche Abnahmekriterien

Für **jedes** sichtbare GUI-Feld (auch leere Platzhalter), jedes Steuerelement und jeden vorgesehenen Topic-Payload dokumentieren:

1. Engine-Quelle: `Pfad::Typ.Feld` mit Zeile und verantwortlichem Owner.
2. Laufzeitbereich: Market-Capture, Econ4h, Shadow, Liquidator, Boot-Audit, Preflight, Journal oder Post-Capture.
3. Datentyp, Einheit, `None`-/UNAVAILABLE-Semantik, Original vs abgeleitet.
4. Push-Auslöser und Rate, Reconnect-/DETACHED-Verhalten, Sequence-/Generation-Ordnung.
5. Topic und exakte Proto-Feldnummer, Reporter, Adapter, Store und Anzeigestelle.
6. Qualifikation: beobachteter Wert vs kanonisch/finalisiert; insbesondere Wallet/PnL/Readiness.
7. Read-only vs Command; Gate/Berechtigung/Idempotenz für jede Bedienfunktion.
8. Regressionstest mit realem oder testweisem Publisher; kein Fake-LIVE und kein Rückfall auf Polling.

Bis alle Felder zugeordnet oder ausdrücklich als `UNVERIFIED` markiert sind, werden **keine isolierten Schema-/GUI-Patches** zum Ersatz des fehlenden Vertrages vorgezogen.

## Scope des gelieferten WriterLane-Entwurfs

Der hochgeladene WriterLane-Code wird als Implementierungsvorschlag **nach** dem Inventar geprüft. Er ist derzeit kein freigegebener Patch: Das Verschieben von `StatusProjection.queue_bytes` und `lag_frames` (Feldnummern 7/8) auf ein neues `writer`-Feld verändert das Schema absichtlich und erfordert exakte Migration, alle Clients sowie gemeinsam gebautes Gateway/Reporter/Frontend. Allein `optional` garantiert bei der aktuellen strikten Schema-ID-Prüfung keine kompatible Deployment-Phase.
