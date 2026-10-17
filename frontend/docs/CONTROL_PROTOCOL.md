# Project 0 Control – SOLL-Control-, API- und Sicherheitsvertrag

**Stand:** 2026-10-17. **Status:** DESIGN / NO IMPLEMENTATION. Nur die in [PROJECT_0_CODE_MAP](PROJECT_0_CODE_MAP.md) genannten Bestandstypen und Skripte existieren. [Phasenplan](FRONTEND_PHASENPLAN.md) · [Project_0-Vertrag](../../project_0/docs/PHASENPLAN.md).

## 1. Prinzipien und Besitzer

- **`p0-longtail`** bleibt der einzige fachliche Rust-Owner, mit vier bestehenden Crates und kanonischem Chain-State.
- **`p0ctl`** = vorgeschlagene dünne Rust-Control-CLI mit optional lokalem Supervisor, nicht zweite Trading-, Chain- oder Economics-Engine.
- **Project 0 Control** = vorhandenes React-Frontend unter `frontend/src/features/project0/`, nicht neue App.
- Terminal und Browser verwenden **dieselbe** versionierte Backend-Konfiguration und identische Validierung. Alle neuen DTO-/API-Namen hier sind **SOLL**, nicht in Git als lauffähige Routen vorhanden.
- Single Host / Single State Owner braucht kein Redis. Source-, Capture-, Replay-/Verifier- und UI-Records werden nach den bestehenden Ownership-Regeln abgegrenzt.
- **JFI:** intern native Rust-/Binärdaten, `.p0j` als persistente Wahrheit. Browser-Wire-Format vor Umsetzung gegen JFI-02 abgleichen; bestehender JSON-React-Client ist *keine* allgemeine Freigabe für neue interne JSON-Parser/Dateien/Queues.

## 2. Terminal-Vertrag (SOLL)

~~~bash
p0ctl doctor
p0ctl preflight --duration 4h --chain-verify strict
p0ctl run start --duration 4h --capture-source logs --chain-verify off --profile standard
p0ctl run status --run RUN_ID
p0ctl run stop --run RUN_ID
p0ctl runs list
p0ctl logs --run RUN_ID --follow
p0ctl verify --run RUN_ID --mode strict --resume-verification
p0ctl report --run RUN_ID --report-markers all --report-types all
p0ctl optimize --run RUN_ID --profile-space offline  # erst FEUI-07
~~~

**IST-Befehl, bis `p0ctl` gebaut ist:** `./project_0/scripts/test.sh --duration 4h --chain-verify off --capture-source logs --report-markers all --report-types all`. Der Befehl ist kein Beweis, dass 4h-/Provider-/Capacity-Gates bereits bestanden sind.

## 3. Versionierte Typen/DTOs (SOLL)

| DTO | Felder und Invarianten |
|---|---|
| `ControlCapabilities` | `schema_version,source_sha,binary_hash,provider_qualification,features,parameter_descriptors` – tatsächliche Rust-Descriptors ausgeben; UI-Defaults nicht kopieren |
| `EffectiveRunConfig` | `profile,duration_seconds,capture_source,verification_mode,limits,sources,restart_required,config_hash` – `Resolved::parse` und `MeasurementPlan` autoritativ |
| `PreflightResult` | `duration_s,capacity_verdict,checks,rate_basis,verify_budget,provider_verdict,start_allowed,reasons` – vorhandenes `CapacityReport` wiederverwenden |
| `RunIdentity` | `capture_run_id,manifest_digest,source_sha,binary_sha,plan_version,start_utc` – vor dem Capture vergeben, niemals neu berechnen beim Report |
| `RunStatus` | `run_id,phase,exit_reason,clock,transport,live_coverage,storage,verification,economics,safety,last_seq` – getrennte Statusachsen |
| `VerificationIdentity` | `verification_spec_id,verification_attempt_id,verification_result_id?,mode,checkpoint,proof_status` – spec stabil, attempts einzeln, result immutable |
| `ReportQuery` | `run_id,selected_verification_result_id?,types,markers,related_types,related_events,view,provenance,from_utc,to_utc,min_net_usd,format` – ausschließlich Ansicht |
| `RawRecordPage` | `run_id,evidence_version,records,next_cursor?,journal_root_hash` – begrenzte Datenmengen |
| `StrategyProfile` | `profile_id,version,parameters,owner,source_hash,mode,evidence_ids,risk_limits,approval_state` – erst spätere Phase |
| `OptimizationResult` | `baseline_id,candidate_id,evidence_digest,known_net,unknown_counts,uncertainty,holdout_status,non_claims` – kein Gewinnversprechen |

**IST-Vorlagen:** `RuntimeConfig,Limits,Resolved,MeasurementPlan,MeasurementConfigV2,RunConfig,VerificationConfig,ReportQuery,MarketStats,StorageStatus,CapacityReport,ChainTx`. Namen wie `RunIdentity` und `StrategyProfile` oben sind zu implementieren.

**Präzision:** `lamports`, `u128/i128` USD-micro, Slots und Nanosekunden nicht als unpräzise JavaScript-`number` für Geldentscheidungen auswerten. UNKNOWN, FAILED, nicht vorhandene Proofs explizit darstellen; nie still auf 0 setzen.

## 4. Geplante Browser-API (noch nicht vorhanden)

**Frontend IST:** `core/api/client.ts` nutzt `/api/v1`; `RealtimeClient.ts` `/ws/v1/events`; `vite.config.ts` verweist auf `localhost:8100` als Dev-Proxy. Diese Dateien liefern keinen funktionierenden Backend-HTTP-Control-Server.

| Route SOLL | Verb | Bedeutung |
|---|---|---|
| `/api/v1/project0/capabilities` | GET | Version, Limit-Descriptors, Provider-/Gate-Fähigkeiten |
| `/api/v1/project0/status` | GET | Owner-/Run-/Provider-/Storage-Snapshot |
| `/api/v1/project0/runs` | GET | paginierte Run-Liste |
| `/api/v1/project0/runs/:runId` | GET | State, immutable Identity/Proofs |
| `/api/v1/project0/preflight` | POST | reiner Read-only-Preflight, startet keine Capture |
| `/api/v1/project0/runs` | POST | nach User-Action und Gate: Observe-only Start |
| `/api/v1/project0/runs/:runId/stop` | POST | idempotenter Safe Stop mit Drain/Seal |
| `/api/v1/project0/runs/:runId/verifications` | POST | getrennte, berechtigte Verifier-Jobs |
| `/api/v1/project0/runs/:runId/records` | GET | paginierter Raw Explorer |
| `/api/v1/project0/runs/:runId/reports` | GET | Resultat-gebundene Reports |
| `/api/v1/project0/optimization-jobs` | POST | nur Offline-Sweep, FEUI-07 |

**Browser-Wire & JFI-Gate:** Browser-Transport typisiert/versioniert; bevorzugt binär, wenn erforderlich. Bestehende JSON-Browseransicht ist keine stillschweigende zusätzliche erlaubte Runtime-JSON-Schnittstelle neben Solana JSON-RPC und dem explizit gepinnten Jupiter-Build-Rand. Jede Ausnahme bedarf eines bewussten Vertragsabgleichs; keine internen `serde_json::Value` oder produktiven JSONL-Consumer.

## 5. Lifecycle, Readiness, Verifikation

**State machine SOLL:** `STOPPED → PREFLIGHT → BOOTSTRAP → STREAM_READY → CAPTURE → QUIESCE → DRAIN → JOURNAL_SEALED → VERIFY → REPORT → COMPLETE` mit `DEGRADED`, `RECOVERING`, `ABORTED_INCOMPLETE`, `FAILED`, `BLOCKED`.

Readiness setzt **WSS-ACK und verwertbare Application-Frames/Slot-Fortschritte** sowie die passenden State-/Journal-Gates voraus. Browser-Close verändert keinen Prozesszustand. Crash/Restart darf weder zwei Writer starten noch einen unterbrochenen Capture als vollständige Live-Coverage fortsetzen.

**Sechs unabhängige Statusachsen:** Uhrzeit/Run-Dauer; Live-/Provider-Coverage/Gaps; RAM/Writer/Disk/Journal; Verification off/index/strict; Economics Gross/Shadow/Realized/UNKNOWN; Safety/Permissions. `off=NOT_RUN`, `index` maximal `INDEX_VERIFIED`, `strict` nur mit lückenlosen Proofs `CANONICAL_VERIFIED`; `429` oder Pruning → `HISTORICAL_INCOMPLETE`. Historienreplay ersetzt nie ursprüngliche Live-`T_rx`; Shadow ist kein Chain-Winner.

**WS-Events SOLL:** `project0.run.phase`, `project0.capture.metric`, `project0.storage.metric`, `project0.coverage.metric`, `project0.verification.progress`, `project0.job.terminal`. Client besitzt derzeit `RealtimeEnvelope{seq,ts_ns,type,project?,payload}`; serverseitig Versionen definieren. Sequence-Gap → frischer API-Snapshot, bis dahin `STALE`; niemals interpolieren.

## 6. Sicherheits-/Rollout-Vertrag

- Default-Bindung `127.0.0.1` und lokale Authentisierung; wirksame Origin-/CSRF-/WebSocket-Upgrades/DNS-Rebinding-Kontrollen, nicht nur CORS.
- `POST`-Commands: idempotency key, Run-ID-Lock, positiv qualifizierter Capacity-/Provider-Gate, explizite Useraktion. Kein Shell-Befehl aus unvalidierten Browserdaten.
- Keine Secrets oder RPC-URLs mit API-Keys in `VITE_*`, Browserantworten, Reports oder Logs. Keine beliebigen Pfadangaben aus dem UI.
- Kein generischer `sign`/`live`/`send`-Endpoint, kein Payment per Frontend. `P0_LIVE_ENABLED=false`, `P0_SUBMISSION_ENABLED=false` für die hier geplanten Observe-Runs. Separate FE-06-Berechtigung bleibt unverändert.
- UI-Disconnect/Slow Client blockiert weder Ingress noch Journalwriter, keine unbounded Browser-Queues.

## 7. Contract- und Negativtests

1. Gleiche Terminal-/Browser-Werte liefern identische effektive Backend-Config/ID/Default/Unit; unbekannte Versionsfelder fail-closed.
2. Duration `30s/60s/10m/1h/4h/24h/48h` genau einmal in Rust validiert; kein zweiter UI-Deadline-Rechner.
3. Alle sechs Marker/vier Klassen/vier Related auswählbar, Roh-Capture/Hashes bei jeder Filterkombination identisch; START/END keine Doppelzählung.
4. `off,index,strict` dürfen keine falsche kanonische Coverage behaupten; 429/Diskfull/Stall/Crash liefern nachvollziehbare INCOMPLETE-Evidence.
5. Doppelstart, Stop-Race, Child-Restart, WS-Disconnect, Slow Client, Sequence-Gap/Resync, Secret-Leaks, Auth/CSRF und Fault-Injection.
6. Kein `P0_MIN_PROFIT` über Report-Filter, keine Wallet-/Keypair-Aktion über GUI, kein LIVE-/Paid-Fallback aus Control-Profilen.
7. CI: Typecheck, Lint, Build, Playwright und Rust-Contracttests ohne private Schlüssel oder Mainnet-Aktionen.
