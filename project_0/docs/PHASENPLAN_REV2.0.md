# Project_0 – Phasenplan REV.2.0
## Gemeinsamer Credential-Store (SHARED-KEYCHAIN-CREDENTIALS-V1) und optionale Optimierungen

**Stand:** 02.11.2026 (Nutzerentscheidung)
**Status:** GEPLANT / DEFERRED – nicht implementiert, nicht abgenommen; Start nur per Nutzerauftrag nach dem Auslöser (§1)
**Zentraler Vertrag:** [`frontend/docs/FRONTEND_PHASENPLAN.md` §14](../../frontend/docs/FRONTEND_PHASENPLAN.md) (SHC-01 … SHC-07, Einfrier-Regel, Abnahmematrix)
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
3. Jede zulässige Änderung am Keychain-Teil von `providers.rs` zieht im selben Commit den Code-Pin dieses Dokuments, Anhang A und den Code-Pin in §14 nach. So bleibt die spätere Extraktion ein reines Verschieben.
4. Drei Klassen von Geheimnissen bleiben getrennt (§14.0): Wallet-Schlüssel (Keypair-Datei, P8-CTRL-Gates), Provider-Credentials (DEC-18-Store) und Operator-Freigabe (DEC-17a). Nur die mittlere Klasse ist Gegenstand von R2-SHC.

### 3. R2-SHC – Arbeitspakete

| Paket | Inhalt | Wer | Abschluss |
|---|---|---|---|
| **A+B** | `shared/`-Crates nach Anhang A, Project_0-Adapter (§3.1), `project0-status.yml`, `gateway.yml`, `gate.sh`, Versions-Tripwire, N-4-Zeile `shared/**`, Lockfiles | Project_0-Session | **ein** finaler Code-Commit; ein Gate, höchstens ein Reviewer, Push `main`, CI nach N-4/N-6 auf dem exakten SHA |
| **C** | BSC-Adapter und `bsc.yml` nach BSC `Phasenplan Rev2.0.md` Anhang B | BSC-Session, nach A+B auf `main` | eigener BSC-Commit mit BSC-Gesamtabnahme auf dem finalen SHA |
| **D** | Dokumentabschluss: §14-Status, DEC-18-Zeile, `BUGS.md`, `STATUS.md` je eine Zeile | jeweilige Session | nach den echten Gate-/CI-Nachweisen |

Die Pakete A+B und C werden nicht vermischt: Die Project_0-Session ändert nichts unter `bsc/**`, die BSC-Session nichts unter `shared/**` (§14 SHC-06).

#### 3.1 P0-SHC-01 – Datei- und Import-Diff

**Paket A+B in einem Commit:** Original aus `project_0/native/crates/solana/src/providers.rs` auslagern: `Secret`, `CredentialStore`, macOS-`Keychain`, `DirectoryStore`. Die neuen Crates im bestehenden `project_0/native/crates/solana/Cargo.toml` als normale Pfadabhängigkeiten ergänzen (Einträge **innerhalb** der vorhandenen `[dependencies]`):

```toml
darkbot-credentials = { path = "../../../../shared/credentials" }
darkbot-keychain = { path = "../../../../shared/keychain" }
```

`test-support` im bestehenden `[features]`-Abschnitt **ersetzen** durch:

```toml
test-support = ["darkbot-credentials/test-support"]
```

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
2. `git log 02ed04a8..origin/main -- project_0/native/crates/solana/src/providers.rs`: Hat sich der Keychain-Teil geändert, zuerst Anhang A auf den aktuellen Code nachziehen (reines Verschieben des aktuellen Codes, kein Rückgriff auf den alten Entwurf).
3. **Bestand am Pin (02.11.2026), neu zu prüfen:** `system_store()` wird von `runtime/src/bin/p0ctl.rs` und `runtime/src/orchestration/provider_registry.rs` aufgerufen; `CredentialStore`/`Secret` nutzt `provider_registry.rs` (`NoCredentialStore`, auch in `boot_audit_x1_tests.rs`); `TEST_STORE_ENV` nutzen die Runtime-Tests `dec18_c01_read.rs`, `dec18_provider_measure.rs`, `dec18_provider_transport.rs`, `p0ctl_provider.rs`. Kein direkter `Keychain`-Aufrufer außerhalb `providers.rs` → `Keychain` ohne Wrapper entfernen.
4. **Features:** `solana`-Feature `test-support` schaltet auch `ingress/geyser_loopback` (`ingress/mod.rs`); das Feature bleibt, es wird nur um `darkbot-credentials/test-support` ergänzt. `runtime/Cargo.toml` hat `darkbot-solana = { …, features = ["test-support"] }` als dev-dependency und erreicht den Shared-`DirectoryStore` darüber. `gate.sh` testet nur im Debug-Profil; der Schritt `release` baut `--release --all-features` und muss ohne Dateistore bauen (SHC-T03). `cargo test --release` gehört nicht zum Gate und wird nicht als bestanden gemeldet.
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
3. Prüfpunkte §5 abarbeiten; jede Abweichung vom Pin zuerst in Anhang A nachziehen.
4. Paket A+B umsetzen: `shared/` nach Anhang A, Adapter nach §3.1, CI/Gate nach §14 SHC-06, Tests nach §3.3; vor dem Live-Keychain-Test (SHC-T02) den Nutzer auf den Dialog hinweisen.
5. Gate, höchstens ein Reviewer, Push `main`, CI nach N-4/N-6 auf dem exakten SHA.
6. Die BSC-Session über Paket C (BSC Anhang B) informieren; Paket C nicht selbst ausführen.
7. Paket D: Statuszeilen nachziehen.
8. R2-OPT: Kandidaten aus §4 mit vorhandener Mess-Evidence vorschlagen und auf die Nutzerentscheidung warten; ohne Evidence nichts bauen.

Bericht an den Nutzer kurz: fertige Pakete, Tests, Commit, CI, offene Punkte.

---

# Anhang A – Einmaliger Implementierungsentwurf `shared/**`

Aus dem Nutzervertrag SHARED-KEYCHAIN-CREDENTIALS-V1 (Revision 2, SHC-04), unverändert übernommen; zwei Kommentarfehler sind korrigiert (§14 SHC-04). Die Blöcke sind direkt den benannten `shared/`-Dateien zuzuordnen. **Keiner davon wird zusätzlich in `project_0/` oder `bsc/` implementiert.** Nach der Implementierung definiert allein die Rustdoc in den tatsächlichen Dateien die API. Bei einer Abweichung vom Code am Pin gilt das Verhalten des Pin-Codes (§5 Nr. 2).

**`shared/Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["credentials", "keychain"]
```

**`shared/credentials/Cargo.toml`**

```toml
[package]
name = "darkbot-credentials"
version = "0.1.0"
edition = "2021"
publish = false
description = "Credential store contract and secret type shared by every DarkBot project"
license = "UNLICENSED"

[features]
# DirectoryStore for offline tests. Only effective in a debug build; a release build never
# contains it, whatever its features (same rule as Project_0 today).
test-support = []

[dependencies]
# Pinned to the zeroize version in both consumer lockfiles (SHC-03); a second zeroize version in
# any lockfile graph is refused by the version tripwire (SHC-06).
zeroize = "=1.9.0"
```

**`shared/credentials/src/lib.rs`**

```rust
//! DarkBot credential store contract, shared by every project (Project_0, BSC, DarkNode, …).
//!
//! This crate is the single source of the contract SHARED-KEYCHAIN-CREDENTIALS-V1:
//!
//! - [`CredentialStore`]: one item per account. `put`, `get`, `delete` (idempotent) and
//!   `list` (account names only, never secrets).
//! - [`Secret`]: a secret's bytes, zeroized on drop, never printed.
//! - [`identity::ServiceId`]: a validated OS-store service name; exactly one per project.
//! - [`errors`]: the fixed error codes every backend reports.
//! - [`DirectoryStore`] (debug `test-support` builds only): the one offline test store, so no
//!   project writes a second one.
//!
//! The crate knows nothing about providers, wallets, Solana, BSC or any GUI. Every project owns
//! its registry, account scheme, revisions, receipts and authorization; the OS backend is the
//! crate `darkbot-keychain`.
//!
//! A service name separates *names* in the OS store. It is no process isolation: whether a
//! process of the same OS account may read another project's item is decided by the OS store's
//! ACL and its user prompt, never here.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod errors;
pub mod identity;
mod secret;
mod store;

#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
mod directory;

pub use secret::Secret;
pub use store::CredentialStore;

#[cfg(any(test, all(feature = "test-support", debug_assertions)))]
pub use directory::DirectoryStore;
```

**`shared/credentials/src/store.rs`**

```rust
//! The store contract (signatures unchanged from Project_0 `providers.rs`).

use crate::Secret;

/// The store of credentials: one item per account (the account scheme is the project's). The
/// secret is handed over and returned as a [`Secret`]; the store never logs it.
pub trait CredentialStore {
    /// Writes or replaces the item of `account`.
    fn put(&self, account: &str, secret: &[u8]) -> Result<(), String>;

    /// The item of `account`. A missing item is `CREDENTIAL_UNAVAILABLE` (fail closed).
    fn get(&self, account: &str) -> Result<Secret, String>;

    /// Removes the item of `account`. A missing item is no error (deletion is idempotent).
    fn delete(&self, account: &str) -> Result<(), String>;

    /// The accounts of every item under `prefix` (their names, never their secrets).
    fn list(&self, prefix: &str) -> Result<Vec<String>, String>;
}
```

**`shared/credentials/src/secret.rs`**

```rust
//! A credential's secret (moved unchanged from Project_0 `providers.rs`).

use zeroize::Zeroizing;

/// A credential's secret: zeroized on drop, never printed (`Debug` is redacted).
pub struct Secret(Zeroizing<Vec<u8>>);

impl Secret {
    /// Wraps bytes that are already zeroizing.
    pub fn new(bytes: Zeroizing<Vec<u8>>) -> Self {
        Self(bytes)
    }

    /// The secret's bytes. Only at an already validated native transport boundary.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl core::fmt::Debug for Secret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_exposes_its_bytes_and_never_prints_them() {
        let s = Secret::new(Zeroizing::new(b"SECRET0".to_vec()));
        assert_eq!(s.expose(), b"SECRET0");
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert!(!format!("{s:#?}").contains("SECRET0"));
    }
}
```

**`shared/credentials/src/identity.rs`**

```rust
//! The OS-store service name of a project.

/// A validated service name: 1–128 bytes of `[A-Za-z0-9._-]`, e.g.
/// `de.darkma.darkbot.project0.provider`. One per project; never a secret, never user input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceId(&'static str);

impl ServiceId {
    /// Validates `value`. A refusal never echoes the input.
    pub fn parse(value: &'static str) -> Result<Self, String> {
        let ok = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
        if !ok {
            return Err("CREDENTIAL_SERVICE_INVALID ([A-Za-z0-9._-]{1,128})".into());
        }
        Ok(Self(value))
    }

    /// The service name.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_id_is_a_bounded_dotted_name() {
        assert_eq!(
            ServiceId::parse("de.darkma.darkbot.project0.provider")
                .unwrap()
                .as_str(),
            "de.darkma.darkbot.project0.provider"
        );
        assert!(ServiceId::parse("de.darkma.darkbot.bsc.provider").is_ok());
        for bad in ["", "with space", "slash/name", "ümlaut"] {
            let e = ServiceId::parse(bad).unwrap_err();
            assert!(e.starts_with("CREDENTIAL_SERVICE_INVALID"), "{bad}");
        }
        let long: &'static str = Box::leak("a".repeat(129).into_boxed_str());
        assert!(ServiceId::parse(long).is_err());
    }
}
```

**`shared/credentials/src/errors.rs`**

```rust
//! The fixed error codes of every backend, byte-identical to the original Project_0 texts.

/// A store operation, for its error code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreOp {
    /// `put`
    Write,
    /// `get`
    Read,
    /// `delete`
    Delete,
    /// `list`
    List,
}

/// The code of a failed OS-store operation. `account` is part of a read failure only; an
/// account is a name, never a secret.
pub fn os_error(op: StoreOp, account: &str, code: i32) -> String {
    match op {
        StoreOp::Write => format!("CREDENTIAL_STORE_WRITE: {code}"),
        StoreOp::Read => format!("CREDENTIAL_UNAVAILABLE: {account} ({code})"),
        StoreOp::Delete => format!("CREDENTIAL_STORE_DELETE: {code}"),
        StoreOp::List => format!("CREDENTIAL_STORE_LIST: {code}"),
    }
}

/// No approved store on this system: every credential operation fails closed. The text is the
/// one Project_0 reported before the extraction.
pub const STORE_UNAVAILABLE: &str =
    "CREDENTIAL_STORE_UNAVAILABLE: no approved credential store on this system";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_are_the_original_project0_texts() {
        assert_eq!(
            os_error(StoreOp::Write, "x", -25300),
            "CREDENTIAL_STORE_WRITE: -25300"
        );
        assert_eq!(
            os_error(StoreOp::Read, "reg1/helius@1.aaaa", -25308),
            "CREDENTIAL_UNAVAILABLE: reg1/helius@1.aaaa (-25308)"
        );
        assert_eq!(os_error(StoreOp::Delete, "x", 1), "CREDENTIAL_STORE_DELETE: 1");
        assert_eq!(os_error(StoreOp::List, "", 2), "CREDENTIAL_STORE_LIST: 2");
        assert!(STORE_UNAVAILABLE.starts_with("CREDENTIAL_STORE_UNAVAILABLE"));
    }
}
```

**`shared/credentials/src/directory.rs`**

```rust
//! The one offline test store (moved unchanged from Project_0 `providers.rs`): a directory, one
//! file per account. Only in a debug build with `test-support`, never in a release build.

use crate::{CredentialStore, Secret};
use zeroize::Zeroizing;

/// A test store under a directory, one file per account.
pub struct DirectoryStore(
    /// The directory.
    pub std::path::PathBuf,
);

impl DirectoryStore {
    fn path(&self, account: &str) -> std::path::PathBuf {
        self.0.join(account.replace('/', "%"))
    }
}

impl CredentialStore for DirectoryStore {
    fn put(&self, account: &str, secret: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.0).map_err(|e| format!("CREDENTIAL_STORE_WRITE: {e}"))?;
        std::fs::write(self.path(account), secret)
            .map_err(|e| format!("CREDENTIAL_STORE_WRITE: {e}"))
    }

    fn get(&self, account: &str) -> Result<Secret, String> {
        std::fs::read(self.path(account))
            .map(|b| Secret::new(Zeroizing::new(b)))
            .map_err(|_| format!("CREDENTIAL_UNAVAILABLE: {account}"))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match std::fs::remove_file(self.path(account)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("CREDENTIAL_STORE_DELETE: {e}")),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        let entries = match std::fs::read_dir(&self.0) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(format!("CREDENTIAL_STORE_LIST: {e}")),
        };
        let mut out = vec![];
        for e in entries {
            let name = e.map_err(|e| format!("CREDENTIAL_STORE_LIST: {e}"))?;
            let account = name.file_name().to_string_lossy().replace('%', "/");
            if account.starts_with(prefix) {
                out.push(account);
            }
        }
        out.sort();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moved from Project_0: one item per account; a missing one is unavailable (fail closed), a
    /// deletion idempotent; a listing names accounts under a prefix only; a secret never prints.
    #[test]
    fn a_credential_revision_is_its_own_item() {
        let dir = std::env::temp_dir().join(format!("darkbot_cred_{}_{}", std::process::id(), std::thread::current().name().unwrap_or("test")));
        let _ = std::fs::remove_dir_all(&dir);
        let store = DirectoryStore(dir.clone());
        store.put("reg1/helius@1.aaaa", b"one").unwrap();
        store.put("reg2/helius@1.bbbb", b"other").unwrap();
        let secret = store.get("reg1/helius@1.aaaa").unwrap();
        assert_eq!(secret.expose(), b"one");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert!(store
            .get("reg1/helius@2.aaaa")
            .unwrap_err()
            .starts_with("CREDENTIAL_UNAVAILABLE"));
        assert_eq!(
            store.list("reg1/").unwrap(),
            vec!["reg1/helius@1.aaaa".to_string()]
        );
        store.delete("reg1/helius@1.aaaa").unwrap();
        store.delete("reg1/helius@1.aaaa").unwrap();
        assert!(store.get("reg1/helius@1.aaaa").is_err());
        assert_eq!(store.list("reg2/").unwrap().len(), 1, "another registry untouched");
        let _ = std::fs::remove_dir_all(dir);
    }
}
```

**`shared/keychain/Cargo.toml`**

```toml
[package]
name = "darkbot-keychain"
version = "0.1.0"
edition = "2021"
publish = false
description = "OS credential store backends for DarkBot (macOS keychain)"
license = "UNLICENSED"

[dependencies]
darkbot-credentials = { path = "../credentials" }
zeroize = "=1.9.0"

[target.'cfg(target_os = "macos")'.dependencies]
# Must resolve to the security-framework version and features already in
# project_0/native/Cargo.lock (one copy per workspace; checked by the version tripwire, SHC-06).
security-framework = { version = "=3.7.0", default-features = false }
```

**`shared/keychain/src/lib.rs`**

```rust
//! The OS credential store backends of DarkBot.
//!
//! macOS: the user's login keychain, generic passwords of one service per project
//! ([`KeychainStore`]). Any other system: no backend, every operation fails closed with
//! `CREDENTIAL_STORE_UNAVAILABLE`. A Linux backend is a separate, qualification-bound
//! implementation of [`darkbot_credentials::CredentialStore`] in this crate; never a plaintext
//! file, an environment variable or a mock.
//!
//! This crate selects no provider, authorizes nothing and signs nothing.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use darkbot_credentials::{identity::ServiceId, CredentialStore};
#[cfg(not(target_os = "macos"))]
use darkbot_credentials::errors::STORE_UNAVAILABLE;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::KeychainStore;

/// The system's credential store of `service` (a project's validated service name).
pub fn open_store(service: &'static str) -> Result<Box<dyn CredentialStore>, String> {
    let service = ServiceId::parse(service)?;
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(KeychainStore::new(service)))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = service;
        Err(STORE_UNAVAILABLE.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invalid_service_opens_nothing() {
        assert!(open_store("not a service").is_err());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn without_a_backend_the_store_is_unavailable() {
        assert_eq!(
            open_store("de.darkma.darkbot.test").err().unwrap(),
            STORE_UNAVAILABLE
        );
    }
}
```

**`shared/keychain/src/macos.rs`**

```rust
//! The macOS login keychain (moved from Project_0 `providers.rs`; the only change is the
//! service as a field instead of a constant).

use darkbot_credentials::{
    errors::{os_error, StoreOp},
    identity::ServiceId,
    CredentialStore, Secret,
};
use zeroize::Zeroizing;

/// Generic passwords of one service in the user's login keychain.
pub struct KeychainStore {
    service: ServiceId,
}

impl KeychainStore {
    /// A store of `service`.
    pub const fn new(service: ServiceId) -> Self {
        Self { service }
    }
}

/// `errSecItemNotFound`.
const ITEM_NOT_FOUND: i32 = -25300;

impl CredentialStore for KeychainStore {
    fn put(&self, account: &str, secret: &[u8]) -> Result<(), String> {
        security_framework::passwords::set_generic_password(self.service.as_str(), account, secret)
            .map_err(|e| os_error(StoreOp::Write, account, e.code()))
    }

    fn get(&self, account: &str) -> Result<Secret, String> {
        security_framework::passwords::get_generic_password(self.service.as_str(), account)
            .map(|b| Secret::new(Zeroizing::new(b)))
            .map_err(|e| os_error(StoreOp::Read, account, e.code()))
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(
            self.service.as_str(),
            account,
        ) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(os_error(StoreOp::Delete, account, e.code())),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(self.service.as_str())
            .load_attributes(true)
            .limit(Limit::All)
            .search();
        let results = match found {
            Ok(r) => r,
            Err(e) if e.code() == ITEM_NOT_FOUND => return Ok(vec![]),
            Err(e) => return Err(os_error(StoreOp::List, "", e.code())),
        };
        Ok(results
            .iter()
            .filter_map(|r| r.simplify_dict())
            .filter_map(|d| d.get("acct").cloned())
            .filter(|a| a.starts_with(prefix))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real login keychain; run on an operator's Mac only:
    /// `DARKBOT_KEYCHAIN_LIVE_TEST=1 cargo test -p darkbot-keychain -- --ignored`.
    #[test]
    #[ignore]
    fn a_live_keychain_round_trip_under_a_test_service() {
        if std::env::var_os("DARKBOT_KEYCHAIN_LIVE_TEST").is_none() {
            return;
        }
        let store = KeychainStore::new(ServiceId::parse("de.darkma.darkbot.test").unwrap());
        let account = format!("test/{}", std::process::id());
        store.put(&account, b"SECRET0").unwrap();
        assert_eq!(store.get(&account).unwrap().expose(), b"SECRET0");
        assert!(store.list("test/").unwrap().contains(&account));
        store.delete(&account).unwrap();
        store.delete(&account).unwrap();
        assert!(store
            .get(&account)
            .unwrap_err()
            .starts_with("CREDENTIAL_UNAVAILABLE"));
    }
}
```
