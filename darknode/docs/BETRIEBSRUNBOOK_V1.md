# DarkNode – Betriebshandbuch: 24/7-Host, Secrets, Monitoring, Restore, Updates V1

Stand: 27.10.2026. Dokument = normativer Betriebssollvertrag, **nicht** installierte Infrastruktur und **kein** bestandenes DR-/Incident-Drill. Bezieht sich auf P1.9/1.10, P5 §5d, Wallet, P8 und P9.

## 1. Ownership und Prozessgrenzen

Linux als V1-Ziel, dedizierter unprivilegierter systemd-Rust-Dienst, alleiniger Account-/Order-/Risk-Owner und separater signierender Key-Kontext. Public-P1-Capture, SQLite-Control-Store, L0/L1/L2-Writer, private Bitget WS/REST, Alarm-Outbox und Gateway-Reporter haben explizite Health-/Backpressure-/Shutdown-Verträge. Browser/React und Gateway-Ausfall dürfen keinen bereits gestarteten Rust-Stop-Audit oder unabhängigen Alarm stoppen. Kein zusätzlicher Trading-Owner durch Auto-Restart ohne Lock/Lease/Fencing; nur ein aktiver signierender Writer je Account.

**Startup:** immer UNARMED/RECONCILING, prüfen Account UID/Mode/Keys, Watchdog, Storage/Journal-Version, Clockqualität, Instrument, Strategiehash, aktive Orders/Fills/Positionen/Stops und Riskbudget, dann Readiness. Auto-READY ist nicht Auto-SIGN/LIVE. Ohne vollständigen Readback keine neuen Orders.

## 2. Secrets und Rollen

systemd-creds/verschlüsselte Service-Credentials als **Designziel**, nach Host-Systemtest mit TPM-/Recovery-/Unseal-Pfad; Dienstnutzer ohne Adminrechte, restriktive FS-ACL, TLS-Zertifikatprüfung, Whitelist für Host-Egress und Secret-Dateien. Separat READ_ONLY, DEMO_WRITE, LIVE_WRITE und ADMIN/MODE_CHANGE; Withdrawal/Transfer/Brokerverwaltung nicht an den Trading-Bot geben. API-Key/Passphrase niemals in Chatlogs/Git, UI, Protobuf, Crashdump, Trace, Environmentdump oder Evidenz. Kein Klartext in normalen Backups; Secrets-Restore-Verfahren in eigenem Recovery-Credential-Pfad.

**Rotation:** neues Credential read-only vorqualifizieren, offene Exchange-Orders/Stops und laufenden Account-Owner sichern, Quiesce und Schlüsselwechsel ohne zwei parallel signierende Owner, Reauth/REST-WS-Readback, alte Credentials revoken; erzwungener Revoke ⇒ NO_ENTRY/HALTED/Alarm, nicht blind neu signieren. Rotationsintervall und Rollen pro Betreiber vor P9 fixieren.

## 3. Welche Alarmklassen? (Keine grüne Scheinanzeige)

| Klasse | Trigger | Minimalreaktion / Besitzer |
|---|---|---|
| SEV0 | Schutzorder fehlt/inkonsistent, unklare offene Position, unkontrollierte Order, gefährdeter Liquidations-/Riskbuffer | sofort NO_ENTRY/PROTECTION_UNKNOWN, qualifizierter Emergency-Runbook, **externer Alarm**, Betreiber |
| SEV1 | Hostprozess tot, unabhängiger Heartbeat fehlt, private WS stale, Journal/fsync/Disk fast voll, Restarts in Schleife, Clock-/UID-Abweichung | neue Entries sperren, unabhängiger Alarm + Reconcile; kein Auto-LIVE |
| SEV2 | Public L1/L2-Gap, Orderbook-Seq, Daten-/Indikatorwarmup unvollständig, GUI/Gateway detached, History-Import blockiert | kein ungültiger Signaltrade, Backfill/Replay statt Schweigen |
| SEV3 | Perf-/CPU/Queue/RateLimit Trend, Backtest-/UI-Jobs verzögert ohne Orderrisiko | Diagnose/Capacity, kein Entry-Bypass |

Alarmquelle und tatsächliche Zustellbestätigung **außerhalb** des DarkNode-Prozesses/Hosts messen. Host-interner Watchdog kann den vollständigen Hostausfall nicht allein melden. SQLite durable Alarm-Outbox und unabhängiges Heartbeat-/Dead-man-Checking nach P1-Plan; CountDown-Cancel-All von Bitget bleibt default OFF wegen möglicher Schutzorder-Stornierung.

## 4. Runbooks für typische Incidents

**WS disconnected:** AccountSync=STALE/RECONCILING, NO_ENTRY, alte Exchange-Stops nur als zuletzt bestätigt mit Zeitlabel zeigen, REST auf Bitget begrenzt abfragen, nach vollständigem Konten-/Order-/Stop-Readback und Safety-Gates erst Betreiber-Rearm.

**Lost ACK / Late Fill:** kein neuer Order-Send bis bereits vorhandene OrderIds/clientOid, Position/Fills/Strategieorders konsistent; nicht auf stale Order-Channel vertrauen. Identität und Exposure reservations erhalten.

**Stop gelöst/gelöscht:** SEV0, STOP_UNKNOWN; Live-Position aus Exchange ermitteln, kontrollierten, deduplizierten Ersatz oder eng begrenzten Emergency-Exit nur über qualifizierten Pfad, sonst HALTED+Alarm. Wechsel der Triggerbasis (Mark/Last) nicht still ändern.

**Disk full / korrupter Journalblock:** Entry-Halt und persistenter Alarm (sofern möglich). Kein riskierender Entry ohne fsync-Durability. Außerbetriebnahme kontrolliert; Wiederanlauf auf saubere Watermarks, Crash-Schnittstellen testen.

**Privater Key kompromittiert:** Rücknahme durch Bitget-Verwaltung, Exit-/Stop-Status read-only durch getrennten Operator-Zugang prüfen, Credentials rotieren und Forensik/Reconciliation durchführen. Keine versprochene Verlustgarantie durch fehlendes Withdrawal.

## 5. Backup und Restore

Off-host, verschlüsselte, versionierte, per Hash/Write-Atomicity prüfbare Backups von Order-/Risk-Journal/SQLite-Control-Registry/Bot/GraphVersion/InstrumentSchema/Manifests/L0-L2-Watermarks. Rücksicherungen brauchen definierte RPO/RTO, unabhängige Restore-Key-Verwahrung, Retention/Storage-/Kosten-/Lifecycle-Plan. Auf derselben Maschine gelagerte Kopien sind **kein** Host-Disaster-Recovery.

**Zwei Pflicht-Restore-Gates:** (A) Restore per off-host Backup mit absichtlich unterbrochenem Snapshot und unterschiedlichem Journal-Cursor; nachweisbarer atomarer Recovery-Cut. (B) neuer Host **ohne Journal**: Bitget REST/private WS alle offenen Positions-/Order-/Stop-Daten lesen, historisch unerreichbare Intents als UNKNOWN erhalten, in HALTED bleiben, keine Rekonstruktion verlorener Oids erraten; bestätigten Exchange-Stop nicht unnötig stornieren. Reconcile/Alarm/ausdrückliches human re-ARM vor jedem Entry.

## 6. Software-Update und Rollback bei offener Position

(1) Risk-Owner auf NO_ENTRY/QUIESCE, alle UNKNOWN Pending-Orders auflösen. (2) Nur nach bestätigter Stopdeckung entweder bis vollständig FLAT warten oder Position OPEN unter nachgewiesenem Exchange-SL halten; die lokale Trailing-/Exit-Logik pausiert, dieses Fenster sichtbar machen. (3) alten Owner komplett beenden/fencen, Journal sichern, versionierte Schema-/Store-Migration erst offline, neue Engine startet HALTED und volle Bitget-Reconciliation. (4) Rücknahme nur zu journal-kompatibler Version mit vordefinierter Migration; nie zwei aktive signierende Engines. (5) Operator sign-off, Stop-/Position-/Safety-Gates erst dann aktive Betriebsfreigabe.

## 7. Operativer Nachweis vor Demo-Soak / LIVE

- Alarms über unabhängigen Kanal tatsächlich ausgelöst und quittiert; Ursache, Zustelllatenz und Host-Ausfallkaskade dokumentiert.
- Prozesskill, Port-/Netzsperre, Clockdrift, Gateway-down, Disk-full, Feedgap, Reconnect, Exchange-Ratelimit, Lost ACK, Hostneustart, Restore/Keywechsel/Test mit offenem Demo-Trade durchgeprüft.
- Hostwahl und Konfiguration: CPU/IO/TLS/WS-Queue, Netzpfad/Latenz-Trace, Storage-Erschöpfung, systemd-Version/Credential-Mechanismus, Berechtigungen/SELinux/Firewall, Zeitquelle, Backup-Storage/RPO/RTO, unabhängiger Alarmdienst.
- Vorab festlegen: zulässige Stale-/p99-/Alarmzeiten, Queue-Sizes, RateLimit-Backoff, Retention, Rollbackzeiten und messbare Pass/Fail-Grenzen. **Keine numerischen Ziele oder bestandenen Verfügbarkeiten erfinden.**
- Logs und Fehlerberichte ohne Geheimnisse; pro Drill exakte Build-SHA, Account-UID-Kürzel, Zeitprofil/Artefakt-Hash, Uhrenqualität, Verifikationsstatus.

P9 LIVE verlangt technische, wirtschaftliche **und** explizite Berechtigungsfreigabe. Solange eine Dimension fehlt, bleibt UNARMED/HALTED.