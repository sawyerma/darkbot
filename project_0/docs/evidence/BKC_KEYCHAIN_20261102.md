# B-KC — Keychain-Zugriff `p0ctl → macOS-Schlüsselbund → Runtime-Binary` (02.11.2026)

Nutzerfreigabe: `ENTSCHEIDUNGEN.md` §2 Nr. 18, Nutzerentscheidungen 01.11.2026 (B-KC), bestätigt 02.11.2026. Lokal, ausschließlich synthetisches Dummy-Credential, kein Netz (Endpoint `http://127.0.0.1:9`), keine echten Keys, kein Mainnet. Der Nutzer hat jeden Dialog selbst beantwortet und lässt für seine Passwörter grundsätzlich **kein „Immer erlauben“** zu.

**Umgebung:**
- macOS 15.6.1, Login-Schlüsselbund.
- Service `de.darkma.darkbot.project0.provider`; vor dem Test **kein** Eintrag dieses Services vorhanden.

**Binaries:** Release-Builds aus `DarkBot-p0-work` auf `a40f260a`, ad-hoc signiert, ohne Team-ID.

| Binary | SHA-256 | CDHash |
|---|---|---|
| `p0ctl` | `6f47744e499620260dad3e0c3ad3194c1a78b8cb45ad95a9f1f453b9fde99cd6` | `cc6814e6d3714130ae1c47872ebab762c90e1451` |
| `p0-probe-costs` | `79116bb0d656f3e8a1a259e4279edabeaf84d60fd096316626cd45d436e14dc2` | `3c6580796c0ec2d80d2b0301c9066993d0039430` |

`p0-probe-costs` liest die Credentials in `provider_registry::activate_tool()`. Danach scheitert es vor jedem RPC an `P0_KEYPAIR_PATH` (`Error: NotPresent`). Dieser Fehler belegt daher einen erfolgreichen Keychain-Read; ein fehlgeschlagener Read endet vorher mit `CREDENTIAL_UNAVAILABLE`.

**Einrichtung** in einem temporären Testprojekt, ohne Dialog, weil `p0ctl` den Eintrag selbst anlegt:

```
p0ctl --project P provider add --id kc-dummy-rpc --kind rpc --endpoint http://127.0.0.1:9 --auth query:api-key --credential kc-dummy --plan free --entitlement dummy-free   # exit 0, v1
<synthetischer Wert auf stdin> | p0ctl --project P provider credential set --credential kc-dummy                                                                             # exit 0, v2
p0ctl --project P provider select --kind rpc --id kc-dummy-rpc                                                                                                              # exit 0, v3 (selected=true, kc-dummy@1 active)
```

**Zugriffsrechte des Eintrags** (`security dump-keychain -a`, nur auf diesen Service gefiltert, ohne Geheimnis):
- `decrypt` darf nur `…/release/p0ctl` mit `cdhash cc6814e6…`.
- `partition_id` ist `cdhash:cc6814e6…`.
- `change_acl` hat `applications (0)`.
- Nach allen Läufen unverändert.

| Lauf | Kontext | Kommando | Dialog / Antwort | Dauer | Ergebnis | Befund |
|---|---|---|---|---|---|---|
| A | interaktiv (Terminal-Sitzung) | `env -i PATH=… HOME=… P0_PROJECT_DIR=P p0-probe-costs` | ja / „Erlauben“ | 94 s | exit 1 `NotPresent` | **PASS** Read |
| C1 | launchd `user/501` | Testjob gestartet und sofort wieder entladen (Wartefehler im Testskript) | — | — | nie gelaufen | **ungültig**, wiederholt als C1b |
| C1b | **nichtinteraktiv**: launchd `user/501`, `LimitLoadToSessionType=Background` | derselbe Prozess als Job | keiner möglich | 3 s | exit 1 `CREDENTIAL_UNAVAILABLE … (-25308)` | **fail-closed** (`errSecInteractionNotAllowed`) |
| B | interaktiv, Wiederholung | wie A | ja, erneut / „Erlauben“ | 6 s | exit 1 `NotPresent` | **PASS** Read; keine dauerhafte Berechtigung |
| C2 | nichtinteraktiv (`user/501`) | wie C1b | keiner | 1 s | `CREDENTIAL_UNAVAILABLE … (-25308)` | fail-closed |
| B2 | interaktiv | wie A | ja / „Erlauben“ | 9 s | exit 1 `NotPresent` | **PASS** Read |
| C3 | nichtinteraktiv (`user/501`) | wie C1b | keiner | <1 s | `CREDENTIAL_UNAVAILABLE … (-25308)` | fail-closed |
| D | **Produktionspfad**: LaunchAgent `gui/501` (wie `ops/darkbotctl`) | derselbe Prozess als Job | ja / „Erlauben“ | 15 s | exit 1 `NotPresent` | **PASS** Read |
| N | Negativkontrolle: Eintrag gelöscht (`security delete-generic-password`, nur dieses Konto), Registry weiter `kc-dummy@1 active selected` | wie A | keiner | <1 s | exit 1 `CREDENTIAL_UNAVAILABLE … (-25300)` | **PASS** (`errSecItemNotFound`, vor Keypair und Netz) |

**Aufräumen:**
- `p0ctl … credential revoke` (v4), `unselect --kind rpc` (v5), `remove --id kc-dummy-rpc` (v6).
- Danach kein Eintrag des Services mehr im Schlüsselbund.
- Kein launchd-Testjob geladen, nichts in `~/Library/LaunchAgents`.

**Verdikt:**
- **PASS — beaufsichtigter Betrieb:** `p0ctl` schreibt; jedes andere Binary liest nach Bestätigung des macOS-Dialogs. Das gilt aus dem Terminal und als LaunchAgent im `gui`-Kontext der Produktion. Es gibt eine Bestätigung je Start, keine dauerhafte Berechtigung.
- **PASS — Negativfälle:** Ein fehlender Eintrag und ein nicht erlaubter Zugriff enden vor jedem weiteren Schritt mit `CREDENTIAL_UNAVAILABLE`.
- **BLOCKED — unbeaufsichtigter Betrieb:** Ohne Person am Bildschirm kann kein Binary außer dem anlegenden `p0ctl` ein Provider-Credential lesen. Das betrifft:
  - ein Hintergrundkontext (`-25308`);
  - ein `gui`-LaunchAgent ohne Antwort (Dialog blockiert);
  - jeden Rebuild, auch von `p0ctl`, weil sich dessen cdhash ändert.

  Für die genehmigten Läufe C-04, C-05 und C-10 heißt das: Der Nutzer bestätigt den Dialog je Start. Unbeaufsichtigte Dauerläufe (DEC-16-Autostart) brauchen eine gesonderte Entscheidung zum Zugriffsmodell (BUGS P0-029).
