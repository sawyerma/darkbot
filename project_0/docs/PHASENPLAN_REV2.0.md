# Project_0 – Phasenplan REV.2.0
## Gemeinsamer Credential-Store (SHARED-KEYCHAIN-CREDENTIALS-V1) und optionale Optimierungen

**Stand:** 02.11.2026 (Nutzerentscheidung)
**Status:** GEPLANT / DEFERRED – nicht implementiert, nicht abgenommen; Start nur per Nutzerauftrag nach dem Auslöser (§1)
**Zentraler Vertrag:** [`frontend/docs/FRONTEND_PHASENPLAN.md` §14](../../frontend/docs/FRONTEND_PHASENPLAN.md) (SHC-01 … SHC-08, Einfrier- und Cutover-Regel, Abnahmematrix) mit **§14-A = einmaliger Code-Entwurf aller `shared/**`-Dateien**; dieses Dokument enthält nur die Project_0-spezifische Integration
**Fachliche Autorität unverändert:** [`PHASENPLAN.md`](PHASENPLAN.md) (Revisionseinteilung 27.10.2026), Status [`BUGS.md`](BUGS.md), Entscheidungen [`ENTSCHEIDUNGEN.md`](ENTSCHEIDUNGEN.md) Nr. 18 (Nachtrag 02.11.2026)
**Umfang:** Project_0 und `shared/**` (nur die Project_0-Session schreibt `shared/**`, §14 SHC-06); BSC-Anschluss in `bsc/docs/1_PLAN/Phasenplan Rev2.0.md` Anhang B (BSC-Session)
**Code-Pin:** `project_0/native/crates/solana/src/providers.rs` Stand `02ed04a8`

### 1. Einordnung, Auslöser und Reihenfolge

REV.2.0 ist ein **Folgeplan**. REV.1.0 (`PHASENPLAN.md`, Revisionseinteilung 27.10.2026) behält die Priorität; dieses Dokument startet nichts, ändert keinen REV.1.0-Umfang und verschiebt nichts aus REV.1.0 hierher.

**Auslöser (beide Bedingungen):** (a) Project_0-REV.1.0-Softwareabnahme abgeschlossen **und** (b) BSC-REV.1.0 abgeschlossen (R14-Abschlussmatrix). Früher nur durch ausdrückliche Nutzerentscheidung, wenn ein zweites Projekt nachweislich ein Credential im Schlüsselbund braucht. Start immer per Nutzerauftrag („Phasenplan Rev2.0 umsetzen“), nie automatisch.

**Inhalt von REV.2.0 (Project_0), in dieser Reihenfolge:**
1. **R2-SHC** – gemeinsamer Credential-Store (§3). Pflichtpaket von REV.2.0, zuerst.
2. **R2-OPT** – die im `PHASENPLAN.md` als REV.2.0 markierten optionalen Optimierungen (§4), jeweils nur nach messbarem Nutzen und eigener Nutzerentscheidung.

REV.2.0 erteilt keine Freigabe für Mainnet-Zugriffe, Signieren, Senden, LIVE oder bezahlte Anbieter. REV.3.0 (Spezialtechnik) bleibt unberührt.

### 2. Einfrier-Regel bis zur Umsetzung (gilt ab 02.11.2026)

1. Kein neuer Provider-Credential-Store-Code: kein zweites `CredentialStore`/`Secret`/`Keychain`/`DirectoryStore`/`system_store()`, kein weiterer Keychain- oder Secret-Speicher. Der DEC-18-Store (Service `de.darkma.darkbot.project0.provider`, Account `{uid}/{credential.id}@{revision}.{nonce}`) bleibt unverändert; Änderungen daran nur als Bugfix.
2. **Nicht eingefroren:** DEC-17a/P8-CTRL-02 (Operator-Freigabeschlüssel, Secure Enclave mit userPresence) als eigene Klasse außerhalb des Credential-Stores, und eine Lösung für P0-029 (Zugriffsmodell des unbeaufsichtigten Starts), die den Store-Code ändern darf.
3. Jede zulässige Änderung am Keychain-Teil von `providers.rs` zieht im selben Commit den Code-Pin dieses Dokuments sowie Code-Pin und Code-Entwurf (§14-A) in §14 nach. So bleibt die spätere Extraktion ein reines Verschieben.
4. Drei Klassen von Geheimnissen bleiben getrennt (§14.0): Wallet-Schlüssel (Keypair-Datei, P8-CTRL-Gates), Provider-Credentials (DEC-18-Store) und Operator-Freigabe (DEC-17a). Nur die mittlere Klasse ist Gegenstand von R2-SHC.
5. **Cutover-Regel:** Bis zum jeweiligen kontrollierten Credential-Cutover gelten die bestehenden zugelassenen und gehärteten Speicher-/Startverträge unverändert (Project_0: DEC-18-Schlüsselbund mit einmaligem `.env`-Import; BSC: `.token`-Datei für LIVE, `DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION` für G0.7). Nach abgeschlossenem Cutover gilt für den migrierten Provider-Credential-Pfad ausschließlich der gemeinsame Schlüsselbund-Store, ohne stillschweigenden Datei-/ENV-Fallback. Wallet-Private-Keys und Operator-Freigaben bleiben eigenständige Klassen. Kein bestehender produktiver Pfad wird allein durch eine Dokumentationsänderung abgeschaltet.

### 3. R2-SHC – Arbeitspakete

| Paket | Inhalt | Wer | Abschluss |
|---|---|---|---|
| **A+B** | `shared/`-Crates nach §14-A, Project_0-Adapter (§3.1), `project0-status.yml`, `gateway.yml`, `gate.sh`, Versions-Tripwire, N-4-Zeile `shared/**`, Lockfiles | Project_0-Session | **ein** finaler Code-Commit; ein Gate, höchstens ein Reviewer, Push `main`, CI nach N-4/N-6 auf dem exakten SHA |
| **C** | BSC-Adapter und `bsc.yml` nach BSC `Phasenplan Rev2.0.md` Anhang B | BSC-Session, nach A+B auf `main` | eigener BSC-Commit mit BSC-Gesamtabnahme auf dem finalen SHA |
| **D** | Dokumentabschluss: §14-Status, DEC-18-Zeile, `BUGS.md`, `STATUS.md` je eine Zeile | jeweilige Session | nach den echten Gate-/CI-Nachweisen |

Die Pakete A+B und C werden nicht vermischt: Die Project_0-Session ändert nichts unter `bsc/**`, die BSC-Session nichts unter `shared/**` (§14 SHC-06).

#### 3.1 P0-SHC-01 – Datei- und Import-Diff

**Paket A+B in einem Commit:** Original aus `project_0/native/crates/solana/src/providers.rs` auslagern: `Secret`, `CredentialStore`, macOS-`Keychain`, `DirectoryStore`. Die neuen Crates im bestehenden `project_0/native/crates/solana/Cargo.toml` als normale Pfadabhängigkeiten ergänzen (Einträge **innerhalb** der vorhandenen `[dependencies]`):

```toml
darkbot-credentials = { path = "../../../../shared/credentials" }
darkbot-keychain = { path = "../../../../shared/keychain" }
```

`test-support` im bestehenden `[features]`-Abschnitt **erhalten und ergänzen**, nichts entfernen. Am Code-Pin hat das Feature keine Abhängigkeiten; es schaltet über `cfg(feature = "test-support")` die Offline-Testhilfen für abhängige Crates (u. a. den Loopback-gRPC-Server `ingress/geyser_loopback`), und das bleibt so. Konkreter Diff gegen den Stand am Pin (Kommentar und Featurename unverändert, nur die Liste erweitert):

```diff
 # Offline test helpers for dependent crates (loopback gRPC server); never in a release build.
-test-support = []
+test-support = ["darkbot-credentials/test-support"]
```

Hat das Feature beim Start der Umsetzung weitere Einträge, bleiben sie alle erhalten und `"darkbot-credentials/test-support"` wird nur angehängt (§5 Nr. 4).

**Testabhängigkeit:** In `[dev-dependencies]` von `solana/Cargo.toml` `darkbot-credentials = { path = "../../../../shared/credentials", features = ["test-support"] }` ergänzen. Am Code-Pin **erforderlich**, nicht nur „falls nötig“: `system_store()` nutzt den `DirectoryStore` unter `cfg(test)`, und `gate.sh` `test_default` läuft ohne Features (§5 Nr. 4). Feature-Unification und die Debug/Release-Kombination real kompilieren. `DirectoryStore` wird **nicht** nochmals in P0 implementiert.

**In `providers.rs` am bisherigen Typ-/Trait-Ort statt der alten Definitionen:**

```rust
pub use darkbot_credentials::{CredentialStore, Secret};

#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
pub use darkbot_credentials::DirectoryStore;

/// Der vorhandene Service – exakt gleich, damit echte alte Keychain-Items erhalten bleiben.
pub const KEYCHAIN_SERVICE: &str = "de.darkma.darkbot.project0.provider";

/// Bestehender Test-Konfigurationsname; kein Produktiv-Fallback.
pub const TEST_STORE_ENV: &str = "P0_TEST_CREDENTIAL_STORE_DIR";

pub fn system_store() -> Result<Box<dyn CredentialStore>, String> {
    #[cfg(any(test, all(feature = "test-support", debug_assertions)))]
    if let Some(dir) = std::env::var_os(TEST_STORE_ENV) {
        return Ok(Box::new(DirectoryStore(dir.into())));
    }
    darkbot_keychain::open_store(KEYCHAIN_SERVICE)
}
```

**Wichtig:** Vor dem Build sämtliche `providers::Keychain`-, `DirectoryStore`-, `security_framework`-, `CredentialStore`- und `Secret`-Referenzen im gesamten P0-Workspace suchen (Bestand am Pin: §5 Nr. 3). `Keychain` war ein Unit-Struct; `KeychainStore::new(ServiceId)` ist kein Alias-Konstruktor. Bei externen `Keychain`-Aufrufern einen kompatiblen P0-Konstruktor ergänzen, ansonsten den Typ entfernen. `NoCredentialStore` in `provider_registry.rs` implementiert den gemeinsamen Trait unverändert. Alte lokale Definitionen aus `providers.rs` vollständig löschen, kein paralleler Produktiv-Codepfad. `security-framework` verlässt `solana/Cargo.toml` nur, wenn kein anderer Nutzer im Crate bleibt.

#### 3.2 P0-SHC-02 – Unveränderte Fach- und Binärverträge

- `ProviderRegistry::account()` bleibt **wortgleich**: `{uid}/{credential.id}@{revision}.{nonce}`; `KEYCHAIN_SERVICE` bleibt bytegleich. Der Store darf kein Secret unter einem anderen macOS-Service oder Account schreiben.
- `provider_registry.rs::decide()` mit synchronisiertem Lock/CAS, Secret-Put vor Registry-Commit, durablem Receipt und `sweep()` bleibt vollständig in Project_0; keine Shared-Registry.
- `activate_credentials()`, `authorize_with()`, `ActiveCredential`, autorisierte RPC-/WSS-/Geyser-/Jupiter-/Jito-Transporte und einmalige Startbindung bleiben in Project_0; Keychain-Read nicht in Hotpaths verschieben.
- `p0ctl provider credential set|rotate|revoke|import-env` und der bestehende Gateway-/Graphite-Rundweg bleiben unverändert. Keine bereits erfolgreiche DEC-18-/C-11-/C-01-Qualifikation ohne Code-/Binding-Änderung wiederholen.
- `P0-029` bleibt **OFFEN** (`BUGS.md`): fremde und neu gebaute Binaries benötigen nach `docs/evidence/BKC_KEYCHAIN_20261102.md` einen macOS-Dialog; ein Background-launchd-Kontext scheitert mit `-25308`. Shared-Code allein macht den DEC-16-Autostart nicht tauglich.

#### 3.3 P0-SHC-03 – Exakte Regressionen und CI

1. **Vorher/nachher (SHC-T02):** einen bereits vorhandenen Test-Credential-Eintrag unter originalem Service/Account (ohne Secret im Testbericht) nach dem neuen Build lesen; kein Neuimport, keine neue Revision. Ein bloßer Unit-Test des Stringschemas genügt nicht. Ablauf wie B-KC (§6): nur ein Dummy-Credential, kein Netz, Ankündigung des macOS-Dialogs, Eintrag danach löschen.
2. `CredentialStore`-Trait/Secret-Debug und originale `CREDENTIAL_*`-Fehlerstrings, einschließlich der non-macOS-Fail-Closed-Nachricht, verifizieren (SHC-T01).
3. `provider_registry::decide/sweep`, `NoCredentialStore`, Registry-CAS, Rotation/Revoke, Receipt-/Crash-/Orphan-Recovery und Fallback-Sperren regressionsprüfen (SHC-T04).
4. Im bestehenden Project_0-Gate die neuen Schritte `shared_fmt`, `shared_clippy`, `shared_test` aus §14 SHC-06 registrieren und in der bestehenden Project_0-CI-Matrix zuordnen. Pfadfilter `shared/**` und N-4-Pfad→Job-Tabelle (`PHASENPLAN.md` und `ENTSCHEIDUNGEN.md` N-4) ergänzen. Tests ohne echte Keychain in Linux CI und dokumentierte echte Keychain-Tests auf dem Mac getrennt deklarieren.
5. Projekt- und Shared-`Cargo.lock` nach kontrolliertem Update mit `--locked` verifizieren, inklusive `zeroize 1.9.0` / `security-framework 3.7.0` (Versions-Tripwire SHC-06); Review nur auf tatsächliche Änderungen, finales vollständiges Gate und Pflicht-CI auf dem geprüften SHA. Git direkt `main`, kein Branch/Force-Push.

**Abnahme Project_0:** SHC-T01–T04 und T08–T11 aus §14 SHC-07; T05 gemeinsam mit BSC nach Paket C; T06/T07 gehören zu BSC.

**Definition of Done R2-SHC (Project_0):** ein zentraler Credential-Store, bestehende Project_0-Items/Registry unverändert funktional, keine veränderte echte Autorisierung. Erst nach echten Gate-/CI-/ACL-Nachweisen `OFFLINE_VERIFIED`, sonst `OPEN/PARTIAL/NOT_RUN`. Keine Freigabe echter Finanzaktionen durch diese Extraktion.

### 4. R2-OPT – optionale Optimierungen (bestehende REV.2.0-Markierungen im `PHASENPLAN.md`)

Diese Arbeiten sind bereits im `PHASENPLAN.md` als REV.2.0 markiert; hier stehen sie nur gesammelt, damit REV.2.0 vollständig ist. Jede braucht einen **messbaren Nutzen** aus vorhandener Evidence und eine eigene Nutzerentscheidung, bevor sie gebaut wird.

| Stelle im `PHASENPLAN.md` | REV.2.0-Umfang |
|---|---|
| Revisionseinteilung (Kopf, 27.10.2026) | weitergehendes CPU-/SIMD-/PGO-/Cache-/Routing-/Provider-/Region-Tuning; neuer spekulativer kostenloser Pre-Execution-Sensor nach messbarem Nutzen |
| P5 „Routing“ | zusätzliche Routen-/Quote-Cache-Feinoptimierung |
| P6-MT.12 „Apple-M4 Runtime“ | nicht vertraglich geforderte Mikrooptimierung erst bei Messbedarf |
| Phase K (Kopf) | zusätzliche produktive Beschleunigung nach belegtem Bedarf |
| K-INGRESS-PRE `SubscribeDeshred` | optionale produktive spekulative Sensoranbindung bei erwiesenem Zusatznutzen |
| K-INGRESS-PRE FREE_ONLY-Alternativen | optionaler zusätzlicher LocalShred-/UDP-Sensor nach GO |
| K „CPU“ | PGO/SIMD/Cache-/ISA-Tuning über die P6-MT-/Hotpath-Gates hinaus |
| K „Routing“ | zusätzliche Routen-/Quote-Cache-Feinoptimierung |
| K „Submission“ | zusätzliche Region-/Redundanzoptimierung |

Die REV.1.0-Pflichtteile derselben Abschnitte und alle REV.3.0-Teile bleiben dort, wo sie stehen.

### 5. Verbindliche Prüfpunkte vor Beginn (am dann aktuellen `main`)

1. Auslöser (§1) erfüllt und Nutzerauftrag vorhanden; sonst melden und nicht beginnen.
2. `git log 02ed04a8..origin/main -- project_0/native/crates/solana/src/providers.rs`: Hat sich der Keychain-Teil geändert, zuerst §14-A (`frontend/docs/FRONTEND_PHASENPLAN.md`) auf den aktuellen Code nachziehen (reines Verschieben des aktuellen Codes, kein Rückgriff auf den alten Entwurf).
3. **Bestand am Pin (02.11.2026), neu zu prüfen:** `system_store()` wird von `runtime/src/bin/p0ctl.rs` und `runtime/src/orchestration/provider_registry.rs` aufgerufen; `CredentialStore`/`Secret` nutzt `provider_registry.rs` (`NoCredentialStore`, auch in `boot_audit_x1_tests.rs`); `TEST_STORE_ENV` nutzen die Runtime-Tests `dec18_c01_read.rs`, `dec18_provider_measure.rs`, `dec18_provider_transport.rs`, `p0ctl_provider.rs`. Kein direkter `Keychain`-Aufrufer außerhalb `providers.rs` → `Keychain` ohne Wrapper entfernen.
4. **Features:** `solana`-Feature `test-support` wird nach §3.1 erhalten und nur um `darkbot-credentials/test-support` ergänzt; bestehende Einträge und alle `cfg(feature = "test-support")`-Nutzer (am Pin u. a. `ingress/geyser_loopback` über `ingress/mod.rs`) bleiben unverändert, keine Feature-Abhängigkeit wird entfernt. `runtime/Cargo.toml` hat `darkbot-solana = { …, features = ["test-support"] }` als dev-dependency und erreicht den Shared-`DirectoryStore` darüber. `gate.sh` testet nur im Debug-Profil; der Schritt `release` baut `--release --all-features` und muss ohne Dateistore bauen (SHC-T03). `cargo test --release` gehört nicht zum Gate und wird nicht als bestanden gemeldet.
5. Lockfiles: `zeroize 1.9.0` in `project_0/native/Cargo.lock` und `bsc/engine/Cargo.lock`, `security-framework 3.7.0` in `project_0/native/Cargo.lock` erneut bestätigen.
6. Workflows: `project0-status.yml` (Flow-Arrays in `pull_request` und `push`), `gateway.yml` und `bsc.yml` (Block-Listen) gegen §14 SHC-06 prüfen; Workflowänderung ⇒ N-4-Tabelle im selben Commit.
7. BSC-Session informieren, dass A+B kommt; deren Worktree und offene Arbeit nicht berühren.

### 6. Ausführungsregeln

- Effizienzregel 02.11.2026: zusammenhängend vollständig implementieren, gezielte Tests während der Arbeit, höchstens ein read-only Reviewer, gesammelte Fixes, **ein** finales `gate.sh`, Push, CI auf dem exakten SHA.
- Nur `main`, kein Branch, kein Force-Push; Commit mit Pathspec, Dateiliste vor dem Push prüfen; nie `bsc/**` aus der Project_0-Session.
- Kein Netz, kein Mainnet, keine echten Keys. Echte Keychain-Tests nur mit Dummy-Credential, nach Ankündigung des macOS-Dialogs (Nutzer bestätigt selbst), Eintrag danach entfernen; Keychain-Metadaten nur für den eigenen Service, nie ein globaler Dump.
- Kein Hardcoding, keine neue Architektur, kein interner JSON-Pfad; vorhandenen Code verschieben statt neu schreiben.
- Doku minimal: je eine Zeile in `BUGS.md`, `STATUS.md`, §14-Status; Commit-Nachricht kurz.

### 7. Auftrag an Claude („Phasenplan Rev2.0 umsetzen“)

1. Lesen: dieses Dokument, `frontend/docs/FRONTEND_PHASENPLAN.md` §14, die Haupt-`README.md` (Abschnitt „Gemeinsamer Credential-Store“), den Kopf von `PHASENPLAN.md` (Revisionseinteilung), `BUGS.md` P0-029, `docs/evidence/BKC_KEYCHAIN_20261102.md`, `ENTSCHEIDUNGEN.md` Nr. 18.
2. Auslöser prüfen (§1). Nicht erfüllt ⇒ dem Nutzer den Stand melden und nur auf ausdrückliche Anweisung beginnen.
3. Prüfpunkte §5 abarbeiten; jede Abweichung vom Pin zuerst in §14-A nachziehen.
4. Paket A+B umsetzen: `shared/` nach §14-A, Adapter nach §3.1, CI/Gate nach §14 SHC-06, Tests nach §3.3; vor dem Live-Keychain-Test (SHC-T02) den Nutzer auf den Dialog hinweisen.
5. Gate, höchstens ein Reviewer, Push `main`, CI nach N-4/N-6 auf dem exakten SHA.
6. Die BSC-Session über Paket C (BSC Anhang B) informieren; Paket C nicht selbst ausführen.
7. Paket D: Statuszeilen nachziehen.
8. R2-OPT: Kandidaten aus §4 mit vorhandener Mess-Evidence vorschlagen und auf die Nutzerentscheidung warten; ohne Evidence nichts bauen.

Bericht an den Nutzer kurz: fertige Pakete, Tests, Commit, CI, offene Punkte.

