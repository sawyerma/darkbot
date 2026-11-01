# SESSION_HANDOFF DEC-18 — 01.11.2026 (Freeze 19:25 UTC)

> **Zeitpunktbezogener Arbeitsstand, keine fachliche Autorität.** Maßgeblich bleiben:
> - `PHASENPLAN.md`: Anforderungen, Reihenfolge und Gates.
> - `ENTSCHEIDUNGEN.md`: Entscheidungen und Freigaben.
> - `BUGS.md`: Ticketstatus.
> - `STATUS.md`: nachgewiesener Stand.
> - `MEASUREMENT_LEDGER.md`: Messungen.
>
> Weicht dieses Dokument von ihnen ab, gilt die Autorität. Die bekannten Abweichungen (Doku-Rückstand) stehen in §5.6. Erstellt auf Nutzerauftrag „Vollständiger Projekt-Freeze und verlustfreie Übergabe“ (01.11.2026) von Claude Code (Opus 5.5), Session `8ceb2389-8e44-43ec-bc65-b2539d595387`. Alle SHAs, Zählungen und Hashes wurden beim Freeze aus Git, Dateien, Gate-Logs, CI-API und dem Sitzungsverlauf erhoben. Was nicht belegbar ist, steht als `UNKNOWN` mit der nötigen Nachprüfung.

## 0 Kurzstand

| Punkt | Wert |
|---|---|
| `origin/main` beim Freeze | `4fbf4ddb7b46b8647dcbbfdedfad824080e7a9c6`: BSC-Docs-Commit (`A bsc/docs/1_PLAN/BSC_REV1_CONTINUATION_HANDOFF.md`) auf `fffa6944` |
| letzter Project_0-Commit auf `main` | `fffa69440dc8a85d2a7ac05021df4cfe05ebfcd0`: DEC-18 Paket 2, Re-Check-Fixes |
| ungepushte Project_0-Commits | `0a48870f4f56a7c33b401101a8da32c16fbd6d4b` (Paket 2b: Re-Check-NITs N1/N2) und `106300ae004f9e140a96f1dd5842a25663bd656b` (Paket 4: P0-020, P0-019-GUI) — lokal vorhanden und als Patch in Anhang A (bit-exakt rekonstruierbar, §4.4) |
| `UNCOMMITTED_CODE` | **NONE**: kein gestagter oder ungestagter Code in einem Project_0-Worktree. Einzige untracked Datei ist der Symlink `frontend/node_modules` in `DarkBot-p0-ctrl` |
| Prüfstand `0a48870f`/`106300ae` | lokale Einzelprüfungen grün (§7.3). **`gate.sh`, Reviewer und CI: NOT_RUN** ⇒ Paket nicht abgeschlossen, kein PASS |
| letztes abgeschlossenes Paket | **DEC-18 Paket 2** (`35f58ff0`, `69f0103b`, `fffa6944`): Gate PASS auf dem patch-identischen `9479acb3`; Reviewer PASS (zweiter Re-Check); CI `41302873400` success (7/7 Jobs) auf `fffa6944` |
| nächster Schritt | §11 Schritte 1–9: Integritätsprüfung, Rebase der zwei Commits auf das dann aktuelle `main`, vollständiges Gate, ein Reviewer, Push, CI (3 Workflows) |
| Mainnet/Messungen in dieser Session | **keine**: kein Mainnet-Read, keine Provider-Messung, kein Signing, keine Transaktion. `MEASUREMENT_LEDGER.md` zuletzt `72a99cc2`, unverändert |
| BSC | unberührt: kein BSC-Prozess beendet, keine Datei unter `bsc/` geändert oder committet |

## 1 Freeze-Protokoll (Auftrag §1)

| Prüfung | Ergebnis |
|---|---|
| laufende Builds/Tests/Gates/Reviewer | keine. `ps` beim Freeze: kein `gate.sh`/`cargo`/`rustc`/Playwright/Vitest/p0-Prozess aus `DarkBot-p0-ctrl` oder `DarkBot-p0-mut`. Alle Reviewer-Agenten beendet (letzter Report 11:15 UTC) |
| Gate `106300ae` | Skript vorbereitet (Session-Scratchpad `gate-106300ae/run.sh`), **nie gestartet**: keine Ausgabedateien ⇒ `NOT_RUN` |
| Gate `c8c30144` (Paket 2, Zwischenstand) | gestartet, dann abgebrochen (`[killed]`), ersetzt durch Gate `9479acb3` ⇒ `NOT_COMPLETED`, ohne Aussagekraft |
| Gate `5be5a30b` (Paket 2, erster Stand) | vollständig gelaufen, **FAILED** (§7.2), Ursachen in `c8c30144` behoben |
| offene Schreiboperationen | keine. Letzter Commit `106300ae` 11:44:34 UTC; danach nur Lese-Operationen und `git fetch` |
| Stash | leer (alle Worktrees) |
| destruktive Git-Operationen beim Freeze | keine (kein reset/clean/checkout/rebase in einem Repo-Worktree). Die Patch-Prüfung lief in einem separaten `git clone --shared` im Session-Scratchpad |
| Secrets | keine in diesem Dokument und keine im Patch (§12) |
| parallele BSC-Arbeit | Während des Freezes hat die BSC-Session `4fbf4ddb` gepusht (nur `bsc/docs/1_PLAN/BSC_REV1_CONTINUATION_HANDOFF.md`). Kein Konflikt mit Project_0 (§3.1) |

## 2 Autoritäten — Referenzen

Stand `fffa6944`; identisch auf `4fbf4ddb`, das nur eine BSC-Datei hinzufügt.

| Datei | SHA-256 (Inhalt) | letzter Commit | relevante Stellen (Zeilen bei `fffa6944`) |
|---|---|---|---|
| `project_0/docs/PHASENPLAN.md` | `a5b88f52cf3a99a3cb4a526f6a76d78cb4e4e55688380d27ea744484c4447e24` | `04d1d1ad` | `### DEC-18` Z. 5833 ff. (§1.2 Bindung, §1.3 Nr. 1–10, §2 BOOT-05, §3 C-11, §4 Jupiter, §6 Reihenfolge Z. 5891, §7 DoD); DEC-18-Descriptor Z. 1820; BOOT-05/P1B→P1A Z. 1309; „Vorrang DEC-18“ Z. 4437; DEC-Reihenfolge Z. 5707; C-01…C-12 Z. 5911; Reihenfolge 2a Z. 5931; DEC-17 Z. 5821/5831 |
| `project_0/docs/ENTSCHEIDUNGEN.md` | `b7c9223dbbbaa106d1e682add06754dcd8bbe53182b040cb246d0a0770394d44` | `04d1d1ad` | §2 Nr. 18 Z. 353–411 (DEC-18 vollständig); §2 Nr. 17 Z. 342 (DEC-17); §3 Messfragen B-1…B-9 Z. 416 ff.; §4 Freigaben C-01…C-12 Z. 432–449 und Gesamtscope Z. 451 |
| `project_0/docs/BUGS.md` | `1f0295c6d1282dbd9e54b8526e7e0c94bb850630441b87a7d80e357e19519077` | `46c38ff4` | Tabelle P0-016…P0-027 Z. 1505–1516; Abschnitte `### P0-016` … `### P0-027` Z. 1630–1664 |
| `project_0/docs/STATUS.md` | `4fa998b747576a569595254b90eb4878bc53864d7e6b0755b9d14707082aeff7` | `46c38ff4` | Zeile DEC-07 Z. 85; P1B-01…03 Z. 299; **DEC-18 provider config end-to-end Z. 322**; C-11 Z. 323 |
| `project_0/docs/MEASUREMENT_LEDGER.md` | `7cb730acf863eb13c369a29ed63dcd8caf6709b92efbc72504e18f99b5868cf8` | `72a99cc2` | keine DEC-18/C-11-Messung (letzter Eintrag 25.10.2026, Z. 6025) |

Weitere DEC-18-Commits auf `main`:
- `04d1d1ad`: DEC-18 in ENTSCHEIDUNGEN/PHASENPLAN überführt.
- `14ed3a16`: DEC-17 a–g.

## 3 Git-Zustand

### 3.1 Remote und Basis
- Remote `origin` = `https://github.com/sawyerma/darkbot.git`. Das Repository ist **öffentlich**.
- `origin/main` = `4fbf4ddb7b46b8647dcbbfdedfad824080e7a9c6` (Fetch beim Freeze). Historie `fffa6944..4fbf4ddb` = genau 1 BSC-Commit (`A bsc/docs/1_PLAN/BSC_REV1_CONTINUATION_HANDOFF.md`).
- **Überschneidung mit den ungepushten Dateien: keine** (`comm -12` leer). `git apply --cached --check` des Patches auf `4fbf4ddb` ist sauber. Ergebnisbaum `aedce0a1a417997f596506c5325cca25ff91e8f8` = Baum von `106300ae` plus die BSC-Datei.
- Basiscommit der ungepushten Arbeit: `fffa69440dc8a85d2a7ac05021df4cfe05ebfcd0` (`merge-base 106300ae origin/main`). Baum `c0a11a6f46a711c1c40bae477516fde58cf90a72`.

### 3.2 Worktrees (alle teilen das `.git` von `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`)

| Pfad | HEAD | Branch | Rolle | `git status --porcelain=v1 -uall` | ungepusht |
|---|---|---|---|---|---|
| `…/6_DarkBot/DarkBot` | `1d29cec7fc42ea89a0f08a8f223bc2e4c77b8da9` | `main` (0 vor, 5 hinter `origin/main`) | **BSC-Checkout der BSC-Session — nicht anfassen** | 54 untracked, alle unter `bsc/` (BSC-eigen; Liste in Anhang B); nichts gestaged oder modifiziert | keine |
| `…/6_DarkBot/DarkBot-p0-ctrl` | `106300ae004f9e140a96f1dd5842a25663bd656b` | detached | **Project_0-Entwicklung** | `?? frontend/node_modules`: Symlink → `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-project0/frontend/node_modules`, kein Code | `0a48870f`, `106300ae` |
| `…/6_DarkBot/DarkBot-p0-mut` | `106300ae004f9e140a96f1dd5842a25663bd656b` | detached | **Gate-/Rebase-Worktree** (`CARGO_TARGET_DIR` separat) | sauber (0 Zeilen) | dieselben zwei Commits |
| `…/6_DarkBot/DarkBot-p0-gate` | `6ec7c50d2d9c53e133c458d960d53281e7e38caa` | detached | alte Gate-Kopie | sauber | keine (Vorfahr von `origin/main`) |
| `…/6_DarkBot/DarkBot-project0` | `3bf081d04ba736f096b9276ba45fbf33003d99ef` | detached | alte P0-Kopie, liefert `frontend/node_modules` | sauber | keine (Vorfahr) |
| `…/6_DarkBot/DarkBot-bsc-rev1/wtEA` | `9d5ed3a0…` | detached | BSC (fremd) | nicht geprüft | — |
| `…/6_DarkBot/DarkBot-bsc-rev1/wtR2` | `1d29cec7…` | detached | BSC (fremd) | nicht geprüft | — |
| `…/6_DarkBot/DarkBot-bsc-rev1/wtR8` | `1d29cec7…` | detached | BSC (fremd) | nicht geprüft | — |

Gestaged, ungestaged, verschoben oder gelöscht ist in keinem Project_0-Worktree etwas. Alle Stashes sind leer.

### 3.3 Ungepushte Commits

| Commit | Parent | Baum | Inhalt |
|---|---|---|---|
| `0a48870f4f56a7c33b401101a8da32c16fbd6d4b` (11:43:57 UTC) | `fffa6944` | `f9206278d42adafa3607ae211a5645984f02b8c7` | „fix(p0): DEC-18 package 2 re-check (N1, N2)“, 2 Dateien, +46/−2 |
| `106300ae004f9e140a96f1dd5842a25663bd656b` (11:44:34 UTC) | `0a48870f` | `8a071ddc0b73b34e3c441b75f329c45828553f71` | „feat(p0): DEC-18 package 4 (P0-020, P0-019 GUI)“, 32 Dateien (1 neu) |
| gesamt `fffa6944..106300ae` | | | 34 Dateien, +2208/−82, 1 neue Datei, keine Löschung/Umbenennung/Modusänderung |

- **Erreichbarkeit:** nur über die HEADs (und Reflogs) der detached Worktrees `DarkBot-p0-ctrl` und `DarkBot-p0-mut`. Es gibt keinen Branch (Branch-Verbot). Bis zum Push nicht beide HEADs wegbewegen; sonst ist der Patch (Anhang A) die Quelle.
- **Committer = Autor:** Sawyer Ma `<mmw-solutions@web.de>`, Committer-Datum = Autor-Datum. Deshalb reproduziert `git am --committer-date-is-author-date` dieselben SHAs (§4.4).

### 3.4 BSC-Konfliktpotenzial
- Beim Freeze **kein Konflikt**: `origin/main` hat seit der Basis nur eine BSC-Datei hinzugefügt.
- Pusht BSC später auf gemeinsam genutzte Dateien, sind diese Pfade betroffen:
  - `frontend/contracts/bsc-fields.json`: 13 `source_sha`-Pins von `gateway/src/control.rs`, neu gepinnt `aae39293…` → `cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0`.
  - `protocol/schema_id.txt`: `106300ae` = `09470f3d52329639999b1da38d345cddfb55f77b43f85a8d1a994b0ee17b5d49`; `fffa6944` = `28e8a26a29c3abd73d427e6f08b8cbc6203d14ca6c240a96aba87b6163955ddb`.
  - `docs/protocol/PROTOCOL.md` und `frontend/src/core/generated/**`, beide generiert.
  - `protocol/schemas/**` und `gateway/src/control.rs`.
- **Auflösungsrezept** (am 01.11. so angewandt):
  1. `main`s Version von `schema_id.txt` nehmen.
  2. `bash protocol/scripts/check.sh --generate` (regeneriert PROTOCOL.md, generierte TS und Schema-ID über beide Änderungen).
  3. Bei `bsc-fields.json` die `source_sha` für `gateway/src/control.rs` auf `shasum -a 256 gateway/src/control.rs` setzen und **nur diese Zeilen** ändern.
  4. Prüfen: `(cd frontend && npm run -s check:bsc-fields)` und `PROTOCOL_BASE_REF=origin/main bash protocol/scripts/check.sh --check` müssen PASS liefern.

## 4 Sicherung des ungepushten Codes

### 4.1 Erklärung
- `UNCOMMITTED_CODE = NONE`.
- `UNPUSHED_COMMITS = 2` (`0a48870f`, `106300ae`), vollständig als `git format-patch --binary --full-index` in **Anhang A**.
- Der Patch enthält keine Binärdatei, kein CR-Zeichen und keine Zeile `~~~~`.

| Eigenschaft | Wert |
|---|---|
| Erzeugung | `git format-patch --binary --full-index --stdout fffa69440dc8a85d2a7ac05021df4cfe05ebfcd0..106300ae004f9e140a96f1dd5842a25663bd656b` |
| SHA-256 | `6c2cc4ed45a9cb808537ac102f7ee35775886e0e481a0474b1812d190ad381a6` |
| Größe | 220547 Bytes, 3339 Zeilen |
| Ziel-Commits | `0a48870f4f56a7c33b401101a8da32c16fbd6d4b`, `106300ae004f9e140a96f1dd5842a25663bd656b` |
| Ziel-Baum | `8a071ddc0b73b34e3c441b75f329c45828553f71` |

### 4.2 Dateien (SHA-256 des Inhalts vor und nach)

| Datei | ± | Modus | SHA-256 bei `fffa6944` | SHA-256 bei `106300ae` | Commit · Zweck |
|---|---|---|---|---|---|
| `project_0/native/crates/solana/src/providers.rs` | +36/−0 | 100644 | `39ae934d8c1e953b1d419e774066807db7fff70e87a2dfe631997eac0464d223` | `77c0281a0e10795b23604169aa8d359c7903cb3db77baffedb9e4d25bd7775e7` | 0a48870f · N1: `ProviderDescriptor::validate` lehnt einen Geyser-Provider mit anderer Auth als `none`/`header:x-token` ab (`PROVIDER_AUTH_UNSUPPORTED`); `ProviderRegistry::validate` lehnt eine Geyser-Selektion ab (`PROVIDER_SELECTION_GEYSER`); Test in `a_registry_version_is_valid_only_whole` |
| `project_0/native/crates/runtime/src/orchestration/startup.rs` | +10/−2 | 100644 | `632b469b27ce497739a4a59b8fbf0ffd3513a69c3c891e973e83dadb4cff9f65` | `ab848ed0372bdea3531c08574e9e360129c23ba174061e3666658c30eb2a5ccd` | 0a48870f · N2: econ4h-Drift (`activated_is_admitted`) wird zur Admission-Ablehnung `BOOT_IDENTITY_DRIFT` und geht an `run_econ4h` (lokales Manifest bleibt); **kein eigener Test** |
| `protocol/schemas/darkbot/project0/v1/analytics.proto` | +127/−0 | 100644 | `1813b85fa2beebe5544f2587d3418e50da73e0d165065445c734b4d480f07d35` | `8e932ce64fe65a96e9bb8c2f5fb74b4c00339aeab9b448f2c8de780fe8aacfc6` | 106300ae · `AnalyticsJobs.providers` (10) `ProviderRegistryView`, `providers_error` (11); `RegistryProvider`, `RegistryCredential`; Commands `ProviderDescriptorCommand`, `ProviderPriorityCommand`, `ProviderSelectCommand`, `ProviderQualifyCommand`, `ProviderCredentialCommand` (`bytes secret = 2`, nur Transport) |
| `protocol/schemas/darkbot/project0/v1/system.proto` | +21/−0 | 100644 | `5416fc466233e5da8a52c8d0b5c7a0dd4e3d0dcc3c5da869643d2bef0be2d5ba` | `89086af49c90866aa1d9356379efbb72fd6070a66eefe38fde77289f81ca9711` | 106300ae · `EffectiveRecord.providers = 22` (`RunProvider`: Kind, ID, Bindung, Registry-Version, Credential-ID/-Revision, Plan; ohne Endpoint) |
| `protocol/schema_id.txt` | +1/−1 | 100644 | `be6667fac78b1673e5fa8d7e1109fec8e51e05a193d053105959b5a3ebf89296` | `a9723ac7a1d01841ce55740883c680988300e25dac4e15ddc57ed9bf7b974982` | 106300ae · Schema-ID regeneriert (`09470f3d…`) |
| `protocol/golden/project0/analytics_jobs.hex` | +1/−1 | 100644 | `07cd855be6bf2f39a033cd42767d2bc2c3d3fd6b019a1b7a9f0303b1e7dd1ca2` | `5f0a87690b3694beb0637e18d9b1db77e26eb7e37752c2aef5f028e714b68223` | 106300ae · Golden mit Provider-Registry-Fixture |
| `docs/protocol/PROTOCOL.md` | +191/−0 | 100644 | `22dae7617578a02321a62856dc27d144ad449d8ea8ce8d26373f8176f0d88b13` | `0c57fb4c3a2bc9828ea0031796a4e3481c512811f49b70243a641200d8361ff9` | 106300ae · generierte Protokolldoku |
| `frontend/src/core/generated/darkbot/project0/v1/analytics_pb.ts` | +455/−33 | 100644 | `f9aecf0cd1e03d27835c3989456add796be563ec1eb93f1c6886140159794ded` | `d8d374fab5e126b355f6733c2efa7def9799102e84d7921d22083283898635f0` | 106300ae · generiert (buf) |
| `frontend/src/core/generated/darkbot/project0/v1/system_pb.ts` | +73/−2 | 100644 | `53d9d09ba95a3ea5860a6e80247111e201d0101faf27fa91703b37cc8928b93b` | `d83ec7720fdc3c17fa0d300d2c2dbcf8b1b003fcc7263aa51e322228baffd40d` | 106300ae · generiert (buf) |
| `project_0/native/crates/runtime/src/orchestration/provider_registry.rs` | +143/−0 | 100644 | `e7c438abd085e032972de0f25ae802f1701b557e63ade249da92d3a01f9fa754` | `a8f9f65ae713cd84c2c797762e595acbaad6e674152278686f1b2df143eb5fc4` | 106300ae · `RegistryView`/`ProviderView`/`CredentialView`, `projection(project)`: Hosts, Auth-Name, Credential-ID, Bindung, Selektion, Blockgrund; kopierter Store = Fehlertext; Test `the_projection_names_hosts_and_revisions_never_a_secret` |
| `project_0/native/crates/runtime/src/analytics/jobs.rs` | +8/−0 | 100644 | `f0f351b8086846b77b2c781ad705dfce3b9feb6918102b2acf993a226ae9efc8` | `f2f8cff7ca038d03e27f65f1b8ebaf786d7eef4ef70f2e57c5868dc95510273b` | 106300ae · `AnalyticsSnapshot.providers/providers_error` aus `provider_registry::projection` (Topic 15) |
| `project_0/native/crates/runtime/src/status_tap.rs` | +1/−0 | 100644 | `70da17925d5344534e20e3170f047add7351210c9b027dfb0653c1424ece0fa7` | `12080c3eff103700185fed3b20c46d5bf7be866522bf97b58cbf3ed3b4043c89` | 106300ae · Re-Export `Plan, ProviderBinding, ProviderKind, PROVIDER_KINDS` (der Reporter hat keine direkte solana-Abhängigkeit) |
| `project_0/native/crates/reporter/src/analytics.rs` | +45/−2 | 100644 | `8857f7e0e44b28db48e61f27369a03a1251f3340df67344d3d8562f1f88f66c4` | `a0c42ca69db90c8ef6d6841c3a5b81d46a4126042fdef1570f569dca9236414c` | 106300ae · Mapping Registry-View → Protobuf `providers()` |
| `project_0/native/crates/reporter/src/lib.rs` | +92/−1 | 100644 | `cf16100bf823c5e0c678e8e4c4223ad4103d75c30352af3f80cd35cf0acfd04a` | `1d433ac14b6fd08b30f4aee21749c2297719801e8b0ce566a4be8b2c933d7ba9` | 106300ae · `RunProvider`-Mapping (EffectiveRecord), Fixture `golden_providers()`, Worst-Case-Framegröße mit Providern |
| `gateway/src/control.rs` | +571/−10 | 100644 | `aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39` | `cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0` | 106300ae · 12 `PROVIDER_ACTIONS`, 2 `SECRET_ACTIONS` (credential set/rotate); `provider_shape` (key_shaped, id_ok, token_like, endpoint_ok, auth_ok, entitlement_ok, KINDS) vor dem Ledger; `SecretBuf` (volatile Wipe bei Drop); `separate_secret` (Ledger hält den Request ohne Secret); `dispatch_job(…, secret)` gibt das Secret nur über stdin an p0ctl, sonst `Stdio::null()`; `expect_version`; PROVIDER_RECEIPT-Parsing; Test `dec18_provider_commands_map_onto_p0ctl_and_no_secret_is_recorded` |
| `gateway/README.md` | +1/−1 | 100644 | `a05536fd284cc5b09b1b4680ac5e50d838c8ef4c404193b92794645c4f2cf7ee` | `39648c7993f8f38f39fa41cab76a93cec90864ebe3a5535e5328b04dc883dc07` | 106300ae · Hinweis Credential-Pfad |
| `ops/darkbotctl` | +4/−2 | **100755** | `1878ddad3416b56b1440da5210461963e202d704f7f54e43bfdc2959e9f7e338` | `5112bcbba4105c0bb21f4555cc48ddc5e4005e1c1751d485a28c90b77710bc68` | 106300ae · Route `darkbotctl p0 provider …` (exec, stdin durchgereicht) |
| `frontend/contracts/bsc-fields.json` | +13/−13 | 100644 | `93b53f158cbb2c212ca7c54719cbdf4d2f8ff2a12c6224165282a18ca3283d8d` | `5cb940b3859585e152bd4a3769537d6f5d75c8c92de53072dbb7dfa5b077239a` | 106300ae · nur 13 `source_sha` von `gateway/src/control.rs` neu gepinnt (geprüft: keine andere Zeile) |
| `frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md` | +1/−0 | 100644 | `738fb5bfa8fb19adecd20ae58b17ae7d13222c8c8716dc5d036920eb5ca8e017` | `9a5c07faadca46b0ecbf66b8cadc3bf3d71460c318a8b10f27c657dae16174d3` | 106300ae · §7 Credential-Eingang |
| `frontend/src/app/projectRegistry.ts` | +2/−1 | 100644 | `87664d86d982605f97768bb69d2b41679d74d84c1311e8874dbaff4e0ec99437` | `acd5e4d77b889f20769d7df31847c16ba37814d0285d8f65ee8c163b8865c54a` | 106300ae · Liquidator-Reiter abonniert zusätzlich `PROJECT0_ANALYTICS_JOBS` (Topic 15) |
| `frontend/src/core/realtime/analyticsControl.ts` | +68/−2 | 100644 | `1faab7363a0e68155697dc36d4ffb9f3e2a6d4a9ff38dd87116b3c89127856f9` | `5aa81f4147dfb76276c3e49e84e2477fc1ddbc47594f733d2412d90401aadfdb` | 106300ae · Builder `providerDescriptorCommand`, `providerPriorityCommand`, `providerSelectCommand`, `providerQualifyCommand`, `providerCredentialCommand` |
| `frontend/src/core/realtime/analyticsControl.test.ts` | +52/−0 | 100644 | `b6bf9cb9af7c15a4156b58b25185c7971bfa34bce6ca2650db9aadae0c363fc0` | `2e1745826ce32475f08489e68c20942a2120f8c570c5bc60330ac9002537ab57` | 106300ae · Vitest der Builder (Ablehnung Key-URL/Userinfo/Key-ID) |
| `frontend/src/features/project0/api/analyticsAdapter.ts` | +18/−2 | 100644 | `12995e64a699afa8acc04c44b8c92450bf778c2216fa8f9d163e3f47c8522481` | `a15dd696b8a979ff33ea029f2ed59f6e67cb1908398dbbef8b87394705297671` | 106300ae · Topic-15-Mapping der Registry |
| `frontend/src/features/project0/api/analyticsAdapter.test.ts` | +19/−0 | 100644 | `5986114295e729e2651388ed58e6ed453ed0425f84633f9f519885f0b0650134` | `1abe88a3ed568bb09a44a66006a1f0beb918bacfababc5f02997a646564f0887` | 106300ae · Vitest Mapping |
| `frontend/src/features/project0/api/project0ViewAdapter.ts` | +4/−0 | 100644 | `9ed8aa21ccc3773ad14c8ca49215823684341c32d49c5943ca19917f8ec86b95` | `289d90aa5332771b1f3790b90073e62afaf69935b0219e0693ac6ea2bef79250` | 106300ae · `runProviders` aus `EffectiveRecord.providers` |
| `frontend/src/features/project0/model/analytics.ts` | +46/−0 | 100644 | `304260376be8bd6c5390dfc5e4ba71f96047ff28d6f162a901583d232115f324` | `6e3f55400fea6b6ee3b8036a5b17f1d7a27f1c71ae96d3854c7c734190f991cb` | 106300ae · View-Typen Registry/Provider/Credential |
| `frontend/src/features/project0/model/status.ts` | +16/−0 | 100644 | `a08d0125e8399b466ee38222b333a4afd9fcf7603c44b95858f54f382cf1cc4c` | `331ef6631ebc29b4eba64edcd3a9d3dd2e710b1c71c02e074b3d3b6de106705d` | 106300ae · `RunProviderView` |
| `frontend/src/features/project0/model/statusSelectors.test.ts` | +1/−1 | 100644 | `84e59dc8803cb07d98c8d2df7d8ba132c87153a47f1146783d24b5b745b3999c` | `099e3f3316ffb0109ad65f0a36bf4767fe85022cad097c4b03142a7fbe78e4a3` | 106300ae · Fixture `runProviders: []` |
| `frontend/src/features/project0/components/ProvidersSection.tsx` | +113 (**neu**) | 100644 | ABSENT | `5eabd46ed41d8ed6e0fac37dc2a0acdd3cf659ea7b0008d7d4e06f666a494057` | 106300ae · Provider-Abschnitt: Liste, Add/Update, Enable/Disable, Priority, Select, Qualify, Credential set/rotate/revoke, Receipt, effective-Provider des laufenden Runs |
| `frontend/src/features/project0/pages/CapturePanel.tsx` | +2/−0 | 100644 | `2c44a062a1f640b905664bc93236c7cc8a100691384992edf22f31f1dda6db93` | `09418b384636ccd9008d674e3fcb2c90ec81a1ad792e8b12c348dc862baace55` | 106300ae · `ProvidersSection` RPC/WSS/Geyser |
| `frontend/src/features/project0/pages/LiquidatorPanel.tsx` | +4/−1 | 100644 | `d61366d7d45a5cbfefc32023f2363de5bacc94fd8198b89eba0d8b6436603073` | `f187051731c5f5af1aee1e04303a046ab27b798f9801c2cc7f100bc8a11f045d` | 106300ae · `ProvidersSection` Jupiter/Jito |
| `frontend/src/features/project0/Project0Page.tsx` | +1/−1 | 100644 | `d87b3ab8b4974674d140293d3d26fcbabc1fd5e7dc8860ca416375d3466d7729` | `b4629261311a4e2b097746a9ea568a1f0fda72949e1087511acd87502297569c` | 106300ae · Liquidator erhält Analytics-Topic |
| `frontend/tests/project0-status.spec.ts` | +63/−1 | 100644 | `ebbee2f85527e9278048296bd4f593753618d9b2c96956276122e68edf67b136` | `f3925771c14f84e9ccbe9d2affc69459fbe336e7dbaec96bbdd532fd60b370c0` | 106300ae · Playwright „DEC-18: the provider registry reaches Capture and Liquidator…“ (dekodierte POST-Parameter, Receipt) |
| `project_0/docs/F0_VAR_01_MATRIX.md` | +5/−5 | 100644 | `58590a33ab8607b2094b479ae76ed27b07315ed0c77dd5c69930539bd4bf4554` | `03a51011972a90131b1d72909f829d85f9321f791720a3b68e017682e770c3c1` | 106300ae · F0-VAR-Zeilen (C-CP-3 u. a.) |

### 4.3 Wiederherstellung — Weg A (lokal vorhanden, bevorzugt)
`git -C /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-ctrl cat-file -t 106300ae004f9e140a96f1dd5842a25663bd656b` liefert `commit`. Dann wie §11 Schritt 4 fortfahren (Rebase in `DarkBot-p0-mut`).

### 4.4 Wiederherstellung — Weg B (nur aus GitHub + diesem Dokument)

```bash
# Arbeitsverzeichnis für Hilfsdateien (nicht im Repo)
W=$(mktemp -d)
cd /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-ctrl
git fetch origin main
git show origin/main:project_0/docs/SESSION_HANDOFF_DEC18_2026-11-01.md > "$W/handoff.md"
# Anhang A extrahieren: Zeilen zwischen den Markern, ohne die beiden Fence-Zeilen
awk '/^<!-- DEC18-PATCH-END -->$/{f=0} f{print} /^<!-- DEC18-PATCH-BEGIN -->$/{f=1}' "$W/handoff.md" | sed '1d;$d' > "$W/dec18_unpushed.patch"
shasum -a 256 "$W/dec18_unpushed.patch"   # MUSS 6c2cc4ed45a9cb808537ac102f7ee35775886e0e481a0474b1812d190ad381a6 sein
# In einem SAUBEREN Worktree (vorher: git status --porcelain leer), z. B. DarkBot-p0-mut:
git -C ../DarkBot-p0-mut status --porcelain=v1 -uall          # muss leer sein
git -C ../DarkBot-p0-mut checkout --detach fffa69440dc8a85d2a7ac05021df4cfe05ebfcd0
GIT_COMMITTER_NAME="Sawyer Ma" GIT_COMMITTER_EMAIL="mmw-solutions@web.de" \
  git -C ../DarkBot-p0-mut am --committer-date-is-author-date < "$W/dec18_unpushed.patch"
git -C ../DarkBot-p0-mut rev-parse HEAD HEAD~1 HEAD^{tree}
# erwartet: 106300ae004f9e140a96f1dd5842a25663bd656b / 0a48870f4f56a7c33b401101a8da32c16fbd6d4b / 8a071ddc0b73b34e3c441b75f329c45828553f71
```

**Beim Freeze geprüft:** In einem separaten `git clone --shared --no-checkout` (Session-Scratchpad) ergab `git apply --cached` auf `fffa6944` den Baum `8a071ddc…`. `git am --committer-date-is-author-date` erzeugte **exakt** `0a48870f…` und `106300ae…` (bit-exakte Rekonstruktion). Auf `4fbf4ddb` angewandt: sauber, Baum `aedce0a1…`. Diese Prüfung lief noch nicht gegen die aus diesem Dokument extrahierte Fassung; sie wird vor dem Push mit genau dieser Datei wiederholt (§12).

## 5 DEC-18 Implementierungsstand (aus dem Code von `fffa6944` bzw. `106300ae`)

Statusstufen nach DEC-18 §7: `DESIGN_APPROVED` → `CODE_COMPLETE` → `OFFLINE_VERIFIED` → `EXTERNAL_QUALIFIED`. Ein Paket gilt hier erst nach Gate, Reviewer und CI auf dem finalen SHA als fertig.

### 5.1 Paket 1 — native Provider-Grundlage (auf `main`, Gate PASS, Reviewer PASS, CI grün)
SHAs: `1ed29dba` (Feature) → `82ae2952` (Review-Fixes M1, L1–L9) → `1a99307b` (Re-Check L1–L6) → `5b9004b0` (Re-Check 2: M1, L2–L5) → `3d39b438` (Re-Check 3: L1–L2, Nits); P0-026 = `7e07724a`.

| Punkt | Stand | Ort (Funktion) | Beleg |
|---|---|---|---|
| `ProviderDescriptor`, `ProviderRegistry` | **fertig** | `solana/src/providers.rs` (ProviderKind rpc/wss/geyser/jupiter/jito, AuthKind none/query:/header:/PathSegment, Plan FREE_ONLY, Descriptor-Felder, `validate`) | Gate PASS `4290ff26`(=`3d39b438`), CI `41293251980` |
| kanonischer Endpoint, Provider-Bindung | **fertig** | `canonical_endpoint` (keine Userinfo, keine Query, kein Fragment, TLS außer Loopback, Token-artige Pfadsegmente abgelehnt), `ProviderRegistry::binding_of` (Domain `darkbot.project0.provider-binding.v1`, bincode, einziger Einstieg) | Tests in `providers.rs` |
| Credential-ID, unveränderliche Revision | **fertig** | Keychain-Account `<uid>/<id>@<rev>.<nonce>`; Revision global eindeutig (uid je Registry, Nonce je Revision) | Review-M1-Test (Racer) |
| macOS-Keychain | **fertig (offline)** | `Keychain` (security-framework 3.7.0, bereits im Graph); Test-Store nur `cfg(any(test, all(feature="test-support", debug_assertions)))` | Tests nutzen `DirectoryStore`, nie den echten Keychain. **Realer Zugriff durch Runtime-/Reporter-Binaries (ACL, headless) UNKNOWN** → Blocker B-KC (§10) |
| `.env`-Import | **fertig** | `p0ctl provider credential import-env` (einmalig, nie über ein bestehendes Credential, Original bleibt); Regeln in `apply` | `p0ctl_provider` |
| `p0ctl provider …` | **fertig** | `status`, `add`, `update`, `remove`, `enable`, `disable`, `priority`, `select`, `unselect`, `qualify`, `credential set`, `rotate`, `revoke`, `import-env`; Secret nur über verdeckten Prompt oder stdin | `crates/runtime/tests/p0ctl_provider.rs` |
| Versionierung, CAS, Idempotenz, Receipts, Restart | **fertig** | `provider_registry::decide`/`apply` unter exklusivem Lock; `registry_<v>.p0j` create-new = CAS; Receipt je Request-ID mit Action-Digest; Rebuild eines verlorenen Receipts nur für die eigene Action | Tests `a_lost_race_leaves_no_secret_and_the_sweep_removes_orphans` u. a. |
| Selection, Disable, Rotation, Qualifikationsverlust | **fertig** | Selektion braucht freien, aktiven Provider mit aktivem Credential; Identitätsänderung oder Rotation löscht die Qualifikation; `qualifies` verlangt `passed` + `FREE_ONLY` (WSS: `wss_free_only`) | Re-Check-Tests |
| Kopierte Registry (fremdes Projekt) | **fertig** | `owned()` (`provider_registry.rs:687`); Status zeigt FOREIGN | — |
| P0-026 Laufzeitdaten git-ignoriert | **ERLEDIGT_LOKAL** | `project_0/.gitignore`, Test `p0_026_runtime_state_ignored` | fail-before 10 Verzeichnisse (lt. Commit `7e07724a`), CI `41291003171`/`41293251980` |

### 5.2 Paket 2 — Runtime/Transport (auf `main` `35f58ff0`, `69f0103b`, `fffa6944`)
Gate PASS auf patch-identischem `9479acb3`; Reviewer zweiter Re-Check PASS; CI `41302873400` success. Jeder Pfad unten ist anhand des Codes bei `fffa6944` beschrieben.

| Produktionspfad | Stand | Code (bei `fffa6944`) | Test / Beleg |
|---|---|---|---|
| `RuntimeConfig` / `Resolved` | **umgestellt** | `RuntimeConfig.providers: Vec<BoundProvider>`; `Resolved::bind_registry` (`solana/src/runtime_config.rs:1671`): RPC/WSS = Registry-Selektion (mit Fallback die selektierbaren anderen nach Priorität), Jito = Jito-Selektion, Jupiter = Registry-Selektion sonst `providers::default_profile` (`providers.rs:638`, `default_profile:v1`), Geyser = per `source_geyser_provider` benannter Provider, Bundle-Simulator und B-Quellen per Registry-ID (nach der RPC/WSS-Schleife, R6-L1) oder Loopback (`BoundProvider::local`); `endpoint_value` (`:753`) beantwortet `P0_RPC_URL`/`P0_WS_URL` aus der aktiven Config; `environment()` (`:1068`) exportiert **keinen** Endpoint (`:1078`); `registry_version`/`qualification` `#[serde(skip)]` (nicht im Digest, R5-L5); `LIMITS_REVISION 20`, Revision-19-Records lesbar | runtime_config-Tests; X1 |
| Boot-Audit, Identitätsbindung | **umgestellt** | `boot_audit::start_config` (`:236`), `mode_config` (`:250`) → `provider_registry::bind` (`:699`); `launched_identity` (`:264`); `activated_is_admitted` (`:1741`, R6-M1); `market_admission` (`:1838`); die Bindungen sind Teil des Konfigurationsdigests | `x1_a_registry_bound_start_is_the_audited_one_after_its_activation`, `x1_start_config_is_identical_in_p0ctl_and_the_launched_runtime` (`boot_audit_x1_tests.rs:426/520`) |
| Network-Audit (BOOT) | **umgestellt (offline)** | `orchestration/boot_network.rs`: RPC über `provider::rpc_client` (`:188`), WSS-Liveness `ingress/ws_rpc.rs::audit_liveness` (`:988`); Credentials erst nach dem Approval-Token (`provider_registry::install_credentials` `:760`), danach `credentials_of` (`:713`) gegen die aktuelle Registry (`PROVIDER_BINDING_CHANGED`) | realer C-04 **NOT_RUN** |
| RPC-Clients | **umgestellt** | `provider::rpc_client` → `rpc_client_with` → `BudgetedSender` → `send_until` (`provider.rs:998`) → `providers::authorize(endpoint)` (`:1075`) → `http.post(target.url())`, Header `set_sensitive`; async Client `redirect::Policy::none()` | `tests/dec18_provider_transport.rs::dec18_the_transport_applies_the_bound_credential_and_nothing_else_holds_it` (**MUT-1**), `provider.rs::dec18_the_rpc_transport_follows_no_redirect` (`:1783`, R6-N2) |
| WSS-Clients | **umgestellt** | `ws_rpc.rs:636` `providers::ws_client_request(endpoint)` → `connect_async_with_config`; Request-Target origin-form `/?api-key=…` (`authorize_with` setzt `/` bei leerem Pfad, R5-H2); Gap-Proxy `runtime/examples/p0-gap-ws-proxy.rs:36/139` ebenso | `providers.rs::a_wss_query_credential_is_an_origin_form_request_target` (`:1464`, echter tungstenite-Handshake, **MUT-2**) |
| Historische/Strict-Reader | **umgestellt** | `market_gap.rs:261` `RpcFetcher::gap_request_metered` → `provider::authorized_blocking` (`provider.rs:908`) mit `blocking_http()` (`:936`, kein Redirect); Verifier mit explizitem Projekt (R5-L2); `VerificationSpec` `SPEC_SCHEMA /2` (Bindung statt URL-Hash) | `market/dec18_transport_tests.rs` (**MUT-3**, **MUT-4**); positiver Antwortpfad **mit** Credential: **offen T-1** (§6.4) |
| Market-Capture, Enrichment | **umgestellt (offline)** | `market/capture.rs:332` `operational_lookup` (P0_WS_URL/P0_RPC_URL aus der aktiven Config); `admitted_network` (`:341`), `admitted_config` (`:366`): `start_config(market)` → `activated_is_admitted` → activate → `install_credentials`; Enrichment über `endpoint_value("P0_RPC_URL")` (`:1419`); Parser `market/configuration.rs:151–154` ohne Helius-Ableitung; Deployment-Inspektion `market/deployment.rs:57` über `authorized_blocking` | `capture.rs::dec18_an_admitted_capture_runs_on_its_admitted_providers` (`:1346`, In-Process). **Echtes Binary mit gültigem Record: NOT_RUN** (P0-016) |
| Geyser, Source-Control | **umgestellt** | `ingress/sources.rs:214` `ProviderConfig::yellowstone(bound)` (nur Kind Geyser, Auth `none`/`header:x-token`); `installed_token` (`:319`) über `authorize`; `state_owner/setup.rs:44`; ohne gebundenen Provider `SOURCE_PROVIDER_UNBOUND`; `P0_GEYSER_PUBLICNODE_TOKEN` und produktives `publicnode()` entfernt (`GeyserConfig::publicnode` nur `#[cfg(test)]`). Lokal in `0a48870f`: Ablehnung schon beim Eintrag (N1) | Tests in `69f0103b` |
| Jupiter-Route, P6-MT.7-Bucket | **umgestellt** | `execution/route.rs:406` `JupiterV2RouteProvider::new_with(config, …)` (Endpoint, Tier nach Credential, `jupiter_timeout_ms`); Request über `authorized_blocking` (`:499`); `P0_JUPITER_API_KEY` entfernt; **Budget unverändert der P6-MT.7-Bucket** (`self.budget`, `route.rs:828`) — nicht neu implementiert | Jupiter-Header-Teil in `dec18_every_blocking…` (nur `crate_health`); Default/Selektion/Timeout-Tests (lt. Commit `35f58ff0`) |
| Jito/Bundle-Transport | **umgestellt** | `solana/src/submit/jito.rs:82` (Block-Engine) und `:285` (Bundle-Simulator) über `authorized_blocking`; `execution/bundle_path.rs` `BundleConfig::of` (gebundener Simulator; ungebundene ID schließt zu, Text `JITO_UNCONFIGURED`); benannte Block-Engine nur Loopback; LIVE/Submission bleiben aus | Tests lt. Commit `35f58ff0`/`fffa6944` |
| Credentials am realen Aufruf | **umgestellt** | einziger Weg: `providers::authorize`/`authorize_path`/`ws_client_request`, `provider::authorized_blocking`, `sources::installed_token`; `activate_credentials` (`providers.rs:823`) einmal je Prozess (OnceLock), `CREDENTIAL_ENDPOINT_DUPLICATE`, `CREDENTIAL_WITHOUT_AUTH`; `solana_client::RpcClient` nur über `BudgetedSender` (übrige `RpcClient::new` nur in Tests) | Grep beim Freeze: keine produktive `connect_async(`/`.post(`-Umgehung außer den genannten |
| **Ausnahme: Python-Tools** | **NICHT umgestellt** | `operations/capture/c0_census.py:312–329`, `operations/cli/measurement_contract.py:262–263`, `operations/verification/audit_market_quick.py:1213–1215` leiten die Helius-URL weiter aus `HELIUS_API_KEY`/`P0_RPC_URL` ab | offen **R-PY** (§10) |
| Bindung über Restart und Audit | **umgestellt** | Bindungen im Digest; `credentials_of` vergleicht die aktuelle Bindung (`PROVIDER_BINDING_CHANGED`, `CREDENTIAL_UNAVAILABLE`); eine fremde Registry-Änderung ist keine Drift (R5-L5); Run-Identität unveränderlich; Registry-Änderungen gestaged bis zum nächsten Start | Rotationstest (Drift), Test „unrelated change is no drift“ |
| `P0_RPC_URL`/`P0_WS_URL`-Vererbung | **umgestellt** | Nur noch Operator-Eingabe für Loopback (`BoundProvider::local`); Remote → `PROVIDER_ENDPOINT_UNREGISTERED`, URL mit Key → `PROVIDER_URL_QUERY` (ohne Echo); kein Export; `P0_MARKET_WS_URL` nur Loopback (`startup.rs:344`, Smoke-Gap-Proxy); `p0-probe-v1` nur lokaler Node; `ops/darkbotctl:237` reicht `P0_WS_URL` weiter (Loopback-Tests) | X1 mit Post-Aktivierungs-Umgebung |
| Konfigurationswechsel p0ctl → Boot-Controller → Runtime | **umgestellt (offline)** | Derselbe `start_config` in p0ctl (audit, doctor, run start) und im gestarteten Runtime; `activated_is_admitted` vor dem ersten Credential-Read (econ4h `startup.rs:227`, Shadow `:237`) | X1 je Modus/Arg-Set/Dauer. **Positiver launchd-Realstart NOT_RUN** (P0-016) |

### 5.3 Paket 2b — `0a48870f` (lokal, ungepusht)
- **Inhalt:**
  - R7-N1: Geyser-Auth wird beim Eintrag geprüft, und es gibt keine Geyser-Selektion.
  - R7-N2: Die econ4h-Drift behält ihr Manifest.
- **Tests:**
  - N1: `a_registry_version_is_valid_only_whole` erweitert, 1 passed (11:17 UTC).
  - N2: kein Test.
- **Prüfungen:** `cargo fmt`/`clippy` für diesen Stand nicht separat belegt ⇒ `UNKNOWN` (das Gate deckt es ab). Gate, Reviewer und CI: **NOT_RUN**.

### 5.4 Paket 4 — `106300ae` (lokal, ungepusht): P0-020 und P0-019-GUI
- **Status-Hinweg:**
  1. `provider_registry::projection` → `AnalyticsSnapshot.providers` (Topic 15 `project0.analytics.jobs`).
  2. Reporter `analytics.rs::providers()` → Protobuf `AnalyticsJobs.providers/providers_error`.
  3. Gateway (Latest-Value-Topic) → WS → `analyticsAdapter.ts` → Store → `ProvidersSection` in Capture (rpc/wss/geyser) und Liquidator (jupiter/jito; Liquidator abonniert jetzt Topic 15).
  4. Laufender Run: `EffectiveRecord.providers` → `project0ViewAdapter.ts` `runProviders` (effective).
  5. Registry = staged/requested.
- **Command-Rückweg:**
  1. Reiter → Builder in `analyticsControl.ts`.
  2. Typisierter Protobuf-POST (`project0.provider.*`, 12 Actions).
  3. Gateway `control.rs`:
     - `provider_shape` vor jedem Record;
     - `separate_secret` (Ledger ohne Secret);
     - `dispatch_job` mappt auf `p0ctl provider …` mit `--request-id`/`--expect-version`;
     - Secret nur über stdin;
     - `SecretBuf`-Wipe.
  4. Nativer Controller `provider_registry::decide`.
  5. Redigiertes PROVIDER_RECEIPT → Ack-Detail; Replay liefert dasselbe Detail.
- **CLI-Route:** `darkbotctl p0 provider …`.
- **Prüfstand:** §7.3. Gate, Reviewer und CI **NOT_RUN**.
- **Noch nicht nachgewiesen (DEC-18 §1.3 Nr. 9):** verlorenes ACK, Frontend-Detach, Berechtigung und konkurrierende Eingaben, jeweils speziell für Provider-Commands. Der generische Command-Pfad hat eigene Tests; deren Abdeckung für die neuen Actions ist `UNKNOWN` und vom Reviewer zu prüfen.

### 5.5 Tickets P0-016 … P0-027 (BUGS-Status bei `fffa6944` vs. tatsächlicher Stand)

| Ticket | BUGS.md (bei `fffa6944`) | tatsächlicher Stand beim Freeze | was fehlt |
|---|---|---|---|
| P0-016 realer Boot-Startpfad E2E | OFFEN · MEDIUM | unverändert offen: `reporter/tests/capture_boot_gate.rs` prüft nur Ablehnungen (`dec07_an_unapproved_raw_capture_reads_nothing`, `dec07_a_direct_runtime_start_without_its_record_reads_nothing`); der positive Fall läuft nur In-Process (`dec18_an_admitted_capture_runs_on_its_admitted_providers`) | positiver Real-Binary-/launchd-Start mit gültigem Record; braucht realen C-04-Record oder ein crate-übergreifendes Fixture |
| P0-017 Provider-Descriptor/Runtime | TEILWEISE | Descriptor/Registry/CLI (Paket 1) **und** Runtime-Verdrahtung (Paket 2) auf `main`; Projektion/GUI lokal (`106300ae`) | Gate/Review/CI Paket 4; BUGS/STATUS-Nachtrag; `requested/effective`/Blockgrund/Restart-Regel als Felder: GUI zeigt staged (Registry) vs. effective (Run) und `selection_block` — Vollständigkeit gegen §1.3 Nr. 1 vom Reviewer prüfen lassen |
| P0-018 secretfreie Bindung | TEILWEISE | `binding_of` ersetzt `hash(url)` in Boot-Audit, Evidence, Strict-Spec (Schema 2), Kapazität, Crosscheck (Paket 2, `main`); URL-Hash-Proofs nie anerkannt | BUGS/STATUS-Nachtrag (Kandidat `OFFLINE_VERIFIED` nach Doku-Review) |
| P0-019 Credential-Store, CLI/GUI-Eingang | TEILWEISE | Keychain + CLI (Paket 1), Runtime-Installation (Paket 2) auf `main`; GUI-/Gateway-Eingang lokal (`106300ae`) | Gate/Review/CI Paket 4; B-KC; R-PY (keine parallele `.env`-Autorität) |
| P0-020 Protobuf/Gateway/Reporter/Frontend | OFFEN | **CODE_COMPLETE lokal** (`106300ae`), ungeprüft | Gate, Reviewer, CI, Push; fehlende §1.3-Nr.-9-Tests |
| P0-021 Jupiter-Provider-Konfiguration | OFFEN | **auf `main`** (`35f58ff0`): Default-Profil statt Konstante, `new_with`, Timeout aus Config, P6-MT.7-Bucket unverändert | BUGS/STATUS-Nachtrag mit Beleg |
| P0-022 C-11-Messwerkzeug | OFFEN | nicht begonnen (kein `measure`, kein `ProbeSpec` im Code) | vollständig (§10 D7) |
| P0-023 JSON-Import unter `historical` | OFFEN | unverändert: `provider_evidence.rs:39` `from_measured_json` nicht feature-gated; Feature `historical` existiert (`runtime/Cargo.toml:14`) | §10 D4 |
| P0-024 Read-only ohne Privatschlüssel | OFFEN | unverändert: `state_owner/setup.rs:240` `tx::load_keypair_json(P0_KEYPAIR_PATH)`, `scripts/capture.sh:67` `need P0_KEYPAIR_PATH` | §10 D5 (vor C-05 Pflicht) |
| P0-025 P1B/IDL/Upgrade/Token/ATA-Read-Werkzeuge | OFFEN | nicht begonnen | §10 D9 |
| P0-026 Laufzeitdaten/Git-Identität | ERLEDIGT_LOKAL `7e07724a` | auf `main`, CI grün | — |
| P0-027 Desktop-Boot/Deployment | OFFEN | nicht am Code verifiziert, nicht begonnen | §10 D11 (vor C-10) |

### 5.6 Bekannte Doku-Abweichungen (nicht in dieser Session korrigiert — Freeze)
1. `BUGS.md`/`STATUS.md` sind auf dem Stand von Paket 1 (`46c38ff4`). Paket 2 auf `main` (`35f58ff0`/`69f0103b`/`fffa6944`, CI `41302873400`) ist dort **nicht** nachgetragen: STATUS Z. 322 sagt „Paket 2 in Arbeit“, P0-021 steht auf OFFEN, obwohl der Code auf `main` ist.
2. BUGS P0-017 nennt CI `41291003171` für `1ed29dba`/`82ae2952`/`3d39b438`. Laut GitHub-API gehört `41291003171` zu **`82ae2952`**. Der Lauf für `3d39b438` ist **`41293251980`** (beide success, 7/7 Jobs). Für `1ed29dba`, `1a99307b`, `5b9004b0`, `7e07724a`, `35f58ff0`, `69f0103b` existiert **kein** eigener Lauf; sie wurden zusammen mit dem jeweiligen Tip gepusht.
3. `project_0/docs/F0_VAR_ENV_REFERENCES.tsv` (generiertes Inventar) listet noch entfernte Variablen (`P0_GEYSER_PUBLICNODE_TOKEN`, `P0_JUPITER_API_KEY`, `HELIUS_API_KEY` in `configuration.rs`). Es ist nicht test-gepinnt und im Doku-Schritt zu regenerieren.
4. Der Memory-Eintrag `project0-order-20261101-dec18.md` (lokal, außerhalb Git) enthält SHAs vor dem Rebase; maßgeblich ist dieses Dokument.

## 6 Reviewer-Befunde (vollständig)

**Legende fail-before:**
- `MUT-n`: Mutation in dieser Session ausgeführt, Test schlug fehl, Original mit frischer mtime wiederhergestellt (§7.4).
- `CM`: laut Commit-Nachricht vor dem Fix rot; in dieser Übergabe nicht erneut ausgeführt.
- `NEU`: Test eines mit dem Fix eingeführten Pfads; ohne Fix nicht kompilierbar, also kein semantischer Mutationsbeleg.
- `NV`: nicht belegt.

**Abbildung Review-SHA → `main`-SHA:** per identischer `git patch-id --stable` und leerem `project_0`-Diff geprüft:

| Review-SHA | `main`-SHA |
|---|---|
| `6df2fbfd` | `7e07724a` |
| `6ca23566` | `1ed29dba` |
| `5855d04d` | `82ae2952` |
| `5eab8302` | `1a99307b` |
| `d257102b` | `5b9004b0` |
| `4290ff26` | `3d39b438` |
| `5be5a30b` | ersetzt durch `c8c30144` (amend) = `35f58ff0` |
| `acc46afb` | `69f0103b` |
| `9479acb3` | `fffa6944` |

Reviewer: je Paket ein read-only Agent (P0-PROD-CTRL-10), Agent-IDs in Klammern.

### 6.1 Paket 1
**R1: Review von `6ca23566`(=`1ed29dba`) + `6df2fbfd`(=`7e07724a`)** (Agent `a569c2d33cc8d82b3`, 08:16 UTC). **FAIL.** Fixes in `82ae2952`.

| ID | Sev | Ursache · Datei/Funktion · Fehlerpfad | Fix (Commit) | Test · fail-before | Ergebnis/offen |
|---|---|---|---|---|---|
| R1-M1 | MEDIUM | `provider_registry.rs` `decide` nicht serialisiert; `Keychain::put` ist Upsert. Retry/konkurrierender Rotate (gleiche Revision) → Verlierer löscht beim Commit-Fehler das Item des Gewinners → Receipt `applied=true`, Credential weg (`CREDENTIAL_UNAVAILABLE`); doppelter `import-env` | ganzes `decide` unter exklusivem Lock; Nonce je Revision, uid je Registry, Account `<uid>/<id>@<rev>.<nonce>` (`82ae2952`) | Zwei-Racer-Test · `CM` („fails 6/6 without the lock“) | R2 bestätigt |
| R1-L1 | LOW | Lost-Receipt-Rebuild ignoriert die Action (`provider_registry.rs` Rebuild) | Version nennt Action + Digest; andere Action unter der Request-ID = Konflikt | Test · `NV` | R2 bestätigt |
| R1-L2 | LOW | Key im falschen Argument landet im Receipt/stdout; `PROVIDER_OPTION_UNKNOWN` echote Werte (`p0ctl.rs`) | `ProviderAction::check` vor jeder Persistenz; nie Echo | CLI-Test · `NV` | R2-L1 Reste → `1a99307b` |
| R1-L3 | LOW | Regeln nur in der CLI (Kind-Immutability, `.env`-Regeln, origin); Qualifikation bleibt nach Bindungsänderung | alle Regeln in `apply`; Identitätsänderung/Rotation/Revoke löscht die Qualifikation | Tests · `NV` | R2-5 Rest → `1a99307b` |
| R1-L4 | LOW | Keychain-Reste (Orphans, obsolete/revoked Items, Löschung bei fsync-Fehler) | Sweep unter Lock; Commit mit verlinkter Version ist kein verlorener Commit | `a_lost_race_leaves_no_secret_and_the_sweep_removes_orphans` · `NV` | R2 bestätigt |
| R1-L5 | LOW | Revisionen nicht global eindeutig (gelöschtes `data/providers` → `helius@1` erneut) | Nonce/uid | Test · `NV` | R2 bestätigt |
| R1-L6 | LOW | `binding(Option<&CredentialRef>)` ohne Abgleich | `ProviderRegistry::binding_of` einziger Einstieg; Limits über Config-Digest | — · `NV` | R2 bestätigt |
| R1-L7 | LOW | `read_secret`: Echo nicht wiederhergestellt bei Signal/Panic, unbegrenzt, Realloc-Kopien, `Debug` leakt | Puffer einmalig auf Grenze, termios-Guard (SIGINT/TERM/HUP), `Secret` mit redigiertem `Debug` | — · `NV` | R2 bestätigt; Nits (SIG_IGN) → `5b9004b0` |
| R1-L8 | LOW | `cargo test --release` würde den echten Keychain beschreiben | CLI-Test nur `debug_assertions` | — | R2: Release-Lib-Tests kompilieren nicht (fail-closed) |
| R1-L9 | LOW | kein Path-Auth, `Plan::Unknown` selektierbar | `AuthKind::PathSegment`, Token-Pfadsegmente abgelehnt, Unknown nie selektiert | Tests · `NV` | R2 bestätigt |
| R1-KC | LOW | Keychain-ACL: Items vertrauen nur dem erzeugenden (unsignierten) Binary; Runtime/Reporter → Prompt oder `errSecInteractionNotAllowed` headless | Entscheidung dokumentiert (BUGS P0-019: macOS-Standarddialog, Laufzeit liest einmal nach `owned()`) | **kein Test möglich (realer Keychain)** | **offen B-KC** |
| R1-Nits | NIT | `--capacity-rps` nicht löschbar u. a.; P0-026-Test prüft nur Ignore-Regeln | `--capacity-rps -`; P0-026 beweist stabile Identität im echten Git-Projekt | — | erledigt |

**R2: Re-Check von `5855d04d`(=`82ae2952`)** (Agent `a2122e9c9c444084e`, 08:37). **PASS.** Lows 1–6 → `1a99307b`.

| ID | Sev | Befund | Fix (`1a99307b`) | Ergebnis |
|---|---|---|---|---|
| R2-1 | LOW | Rohwerte noch geechot (`update --id`, `PROVIDER_NUMBER_INVALID`, `--env-file`) | ID vor Lookup geprüft, Zahl/Pfad nie gezeigt | R3 bestätigt |
| R2-2 | LOW | key-förmige IDs (32 Hex) akzeptiert; Request-IDs bis 64 alnum | ≥16 Hex nie ID/Entitlement; Request-ID-Residuum dokumentiert | R3 bestätigt |
| R2-3 | LOW | falsche Sweep-Notiz auf jeder Nicht-Credential-Entscheidung | Sweep nur bei `touches_credentials` | R3 bestätigt |
| R2-4 | LOW | CLI-Update kann eine parallele Änderung zurückdrehen | Default `expected` = gelesene Version | R3 bestätigt |
| R2-5 | LOW | Qualifikationsregel nur in der CLI | Evidence-Auflösung + Bindungsabgleich in `decide`/`apply` | R3 bestätigt |
| R2-6 | LOW | Sweep-Scope uid nicht projektgebunden (kopierte Registry) | `owned()`, Adopt mit neuer uid | R3 bestätigt |
| R2-Nits | NIT | Release-Lib-Tests kompilieren nicht; SIG_IGN; `token_like`-Grenzen; Usage; `import-env`-Zeroize; Rebuild liest alle Versionen; Rotation sweept gepinnte Revision | teils `1a99307b`/`5b9004b0`; Hinweis „Runtime liest Secret einmal beim Start“ → Paket 2 umgesetzt | — |

**R3: Re-Check von `5eab8302`(=`1a99307b`)** (Agent `a3c68a2a3df25ac3c`, 08:52). **FAIL.** Fixes in `5b9004b0`.

| ID | Sev | Befund · Ort | Fix (`5b9004b0`) | Test · fail-before | Ergebnis |
|---|---|---|---|---|---|
| R3-M1 | MEDIUM (Regression) | `providers.rs` `token_like`: Wortaufteilung lässt UUID-Pfadtoken (rpcpool) und base64url durch → Key im Klartext in `registry_*.p0j` | Segment ohne Trenner ≥16 Hex mit Ziffern+Buchstaben oder ≥20 Zeichen Groß/Klein/Ziffer = Token | UUID- und base64url-Test · `CM` („per-word rule alone lets the UUID through“) | R4 bestätigt |
| R3-L2 | LOW | `qualifies` prüft nur die Bindung, nicht `passed`/`FREE_ONLY`/`wss_free_only` | verlangt (`PROVIDER_EVIDENCE_NOT_PASSING`) | Test · `NV` | R4 bestätigt |
| R3-L3 | LOW | `AuthKind`-Namen ohne `key_shaped` | Namen nie key-förmig/token-artig | Test · `NV` | R4 bestätigt |
| R3-L4 | LOW | Update-Digest enthält Basis-Qualifikation → Retry-Konflikt | CLI sendet `qualification = None` | — | R4 bestätigt |
| R3-L5 | LOW | `owned()` nur in Tests; Status ohne FOREIGN | Status zeigt FOREIGN; Usage zur Adoption | — | R4 bestätigt; Runtime ruft `owned()` vor dem Keychain (Paket 2) |
| R3-Nits | NIT | HiddenPrompt-Signalfenster; `env_file::parse` Zeroize; FOREIGN-Pfad-Echo | Disposition zuerst gelesen | — | R4 bestätigt |

**R4: Re-Check von `d257102b`(=`5b9004b0`)** (Agent `aa1abf6b3e2f44d88`, 09:00). **PASS**, Rest-Lows → `3d39b438` (danach **nicht erneut reviewt**, nur LOW/NIT).

| ID | Sev | Befund | Behandlung (`3d39b438`) | Status |
|---|---|---|---|---|
| R4-1 | LOW | weitere Key-Formen (lowercase-hyphen nicht-hex, 16–19-stelliges base64url) gehen durch | Grenzen der Token-Regel dokumentiert | FIXED_NOT_REREVIEWED (Restrisiko dokumentiert) |
| R4-2 | LOW | Registry unter alter Regel gespeichert → bei strengerer Regel blockiert alles | Fail-closed mit benannter Recovery (`data/providers/` archivieren, Provider neu anlegen; Keychain-Items bleiben) | FIXED_NOT_REREVIEWED |
| R4-3…6 | NIT | Doc-Kommentar `token_like`; False Positives; WSS-Branch ohne Kind-Check; gespeicherte AuthKind-Namen in `validate`; kein CLI-Test FOREIGN | gespeicherter Auth-Name besteht die Eingangsregel (`3d39b438`); Rest NIT | teils offen (NIT) |

### 6.2 Paket 2
**R5: Review von `5be5a30b`** (Agent `a43acbd287d396d27`, 10:26). **FAIL** (4 HIGH, 1 MEDIUM). Fixes in `c8c30144` (= `35f58ff0`).

| ID | Sev | Ursache · Datei/Funktion · Fehlerpfad | Fix | Regressionstest · fail-before | Ergebnis |
|---|---|---|---|---|---|
| R5-H1 | HIGH | **Audit-/Runtime-Identity-Drift nach Provider-Auswahl.** `runtime_config.rs::environment()` exportierte `P0_RPC_URL`/`P0_WS_URL`. Die Admission (`startup.rs:184/234`, `capture.rs:341`, `boot_audit.rs:1783–1805` `launched_identity` mit `std::env`) löste aus der überschriebenen Umgebung neu auf → `bind_named` → `PROVIDER_ENDPOINT_UNREGISTERED` → `BOOT_IDENTITY_DRIFT`. Jeder registry-gebundene Start (econ4h, Shadow, Market-Longtail, `admit_unbounded`) wurde abgelehnt | kein Endpoint-Export mehr; Leser nehmen `endpoint_value` der aktiven Config (`setup.rs`, `operational_lookup`, `run_econ4h`) | `x1_a_registry_bound_start_is_the_audited_one_after_its_activation` (Registry-Selektion + Umgebung nach Aktivierung) · `NV` (als „schlägt gegen den alten Export fehl“ begründet, **Mutation nicht ausgeführt**) | R6 bestätigt |
| R5-H2 | HIGH | **WSS-Request-Target.** `providers.rs` `authorize_with` hängt `?api-key=` an den kanonischen Endpoint ohne `/`; tungstenite schreibt `GET ?api-key=K HTTP/1.1` (nicht origin-form) → 400 → `WSS_CONNECT_FAILED` für Helius-Free-WSS (C-04 `audit_liveness`, Shadow/econ4h-Ingress, Gap-Proxy) | `/` bei leerem Pfad vor `?` | `a_wss_query_credential_is_an_origin_form_request_target` (echter Handshake, assert `/?api-key=…`) · **MUT-2** | R6 bestätigt |
| R5-H3 | HIGH | **Transporte umgehen die Credential-Bindung.** `market/deployment.rs:39–56` `client.post(url)` (Capture `deployment_before`, `strict_driver::prove_deployment`) und `market_gap.rs`/`market/fetch.rs` `RpcFetcher` (Verifier-History-Reads) gingen ohne Key → 401 | beide über `provider::authorized_blocking`; `RpcFetcher` mit `blocking_http()` | `dec18_every_blocking_transport_applies_the_bound_credential_and_follows_no_redirect` · **MUT-3** | R6 bestätigt |
| R5-H4 | HIGH | Standalone-Rohcapture (`p0-market-capture-reported`, Boot-Modus `market`) band nie die Registry, installierte keine Credentials, las `P0_WS_URL`/`P0_RPC_URL` roh | `capture.rs` `admitted_network`/`admitted_config`: `start_config(market)`, Aktivierung, Credential-Installation nach der Admission | `dec18_an_admitted_capture_runs_on_its_admitted_providers` (In-Process) · `NEU`. **Positive Real-Binary-Abnahme fehlt** (`capture_boot_gate.rs` nur negativ) → P0-016 | R6 bestätigt; Realstart offen |
| R5-M1 | MEDIUM | veraltete Reporter-Tests (`reporter/tests/config.rs` `missing_source`, `logs_helius`) → `test_default` rot | auf DEC-18-Regel umgestellt | Gate `5be5a30b` rot (2 Fehler je Set) → Gate `9479acb3` grün | erledigt |
| R5-L1 | LOW | **Redirect-Policy:** reqwest-Default (bis 10 Redirects) → Header-Credential an fremden Host | `redirect::Policy::none()` (async + `blocking_http`) | blocking: **MUT-4**; async: `dec18_the_rpc_transport_follows_no_redirect` (R6-N2) · `NV` | R6/R7 bestätigt |
| R5-L2 | LOW | Verifier band das Projekt aus `P0_PROJECT_DIR` statt `--project` | Projekt explizit durch cli/cli_with/post_capture/jobs/p0ctl | Integrationstest umgestellt · `NV` | R6 bestätigt |
| R5-L3 | LOW | Keychain-Read vor Owner-Lock/Admission | binden früh, Credentials nach Lock+Admission | — · `NV` | R6 bestätigt |
| R5-L4 | LOW | lokale Provider konnten die Registry ersetzen (Evidence/Audit akzeptierten sie) | `free_registry_provider` (`provider_evidence.rs:195`) in `validate_provider_evidence` und Boot-WSS-Check | FE-Evidence-Test erweitert · `NV` | R6 bestätigt |
| R5-L5 | LOW | `registry_version`/`qualification` im Digest → jede Registry-Änderung invalidiert alles | `#[serde(skip)]`; `credentials_of` vergleicht je Provider | „unrelated change is no drift, rotation is“ · `NV` | R6 bestätigt |
| R5-NITs | NIT | Testlücken (Remote ohne Key, WSS-Injektion, X1 ohne Registry); FE-Print vor Bindung; doppelte Loopback-Liste; `SPEC_SCHEMA` nicht erhöht; Kommentare; `.env.example`-Aussage (capture.sh/smoke.sh exportierten `.env`) | alle umgesetzt (ein Loopback-Helper, `SPEC_SCHEMA /2`, FE-Print bindet zuerst, capture.sh/smoke.sh leiten nichts ab) | — | R6 bestätigt. **Python-Tools weiter offen (R-PY)** |

**R6: Re-Check von `c8c30144`** (11:03). **FAIL** (1 MEDIUM). Fixes in `9479acb3` (= `fffa6944`).

| ID | Sev | Ursache · Ort | Fix | Test · fail-before | Ergebnis |
|---|---|---|---|---|---|
| R6-M1 | MEDIUM | **TOCTOU:** die Rohcapture löst nach `market_admission` ein zweites Mal auf (`capture.rs:341–377`), aktiviert diese Config ohne Vergleich mit der zugelassenen Identität; p0-longtail (`startup.rs:136–143`) vergleicht eine Re-Resolution statt der aktiven Config | `boot_audit::activated_is_admitted`: Digest der aktivierten Config = Digest des Records; Capture prüft vor der Aktivierung, p0-longtail nach der Admission und vor dem Credential-Read | Race-Test (Rotation zwischen Admission und Aktivierung → `BOOT_IDENTITY_DRIFT`/`CAPTURE_PROVIDER_REFUSED`); X1-Assertion „Runtime-Aktivierung hat den auditierten Digest“ je Modus/Arg-Set/Dauer und mit Registry · `NEU`/`NV` | R7 bestätigt |
| R6-L1 | LOW | `bind_registry` band Hilfs-RPC doppelt, wenn auch Run-RPC → `PROVIDER_BOUND_UNUSED` | Hilfs-IDs nach der RPC/WSS-Schleife | Test mit `--fallback on`, Simulator = selektierter RPC, B = Fallback · `NV` | R7 bestätigt |
| R6-N1 | NIT | `BundleConfig::of` → `None` mit Text „…unset“ | `JITO_UNCONFIGURED` nennt jede Ursache | — | R7 bestätigt |
| R6-N2 | NIT | No-Redirect des async-Transports ungetestet | `dec18_the_rpc_transport_follows_no_redirect` | Test · `NV` (Reviewer: würde bei erlaubten Redirects fehlschlagen) | R7 bestätigt |
| R6-N3 | NIT | `activate_credentials` schlüsselt nur nach Endpoint | dokumentiert (ein authentifizierender Provider je Endpoint) | — | R7 bestätigt |

**R7: zweiter Re-Check `acc46afb`(=`69f0103b`, Geyser) + `9479acb3`(=`fffa6944`)** (11:15). **PASS** (kein CRITICAL/HIGH/MEDIUM).

| ID | Sev | Befund | Behandlung | Status |
|---|---|---|---|---|
| R7-N1 | NIT | ungenutzter Geyser-Provider wirkt auf jeden Start (falsche Auth → `setup.rs:26–30`, fehlendes Item → `install_credentials`), Registry-Geyser-Selektion ignoriert | Eintrag lehnt Auth ≠ none/`header:x-token` ab; keine Geyser-Selektion (Source-Unit wählt per ID) | **lokal `0a48870f`**, ungeprüft (Gate/Review/CI NOT_RUN) |
| R7-N2 | NIT | econ4h-Ablehnung durch `activated_is_admitted` überspringt das lokale Manifest | als Admission-Fehler an `run_econ4h` | **lokal `0a48870f`**, ungeprüft, **ohne Test** |

### 6.3 Paket 4
Kein Review durchgeführt (`NOT_RUN`). Prüfumfang für den nächsten Reviewer:
- Diff `fffa6944..<rebased SHA>` ohne `bsc/`;
- Secret-Separation vor dem Ledger, `SecretBuf`-Wipe, stdin-Hand-off, `Stdio::null()`;
- `provider_shape` = native Regeln;
- Replay/Idempotenz;
- Receipt-Parsing;
- Projektion secretfrei (nur Host);
- Topic 15 im Liquidator;
- §1.3 Nr. 9-Tests;
- generierter Code und Schema-ID.

### 6.4 `RPC_MISSING_BLOCK` — geänderte Testassertion (ausdrücklich)
- **Datei/Test:** `project_0/native/crates/runtime/src/market/dec18_transport_tests.rs`, Test `dec18_every_blocking_transport_applies_the_bound_credential_and_follows_no_redirect`.
- **Geschehen (10:37 UTC):** Der Loopback-Server antwortet `{"jsonrpc":"2.0","id":1,"result":null}`. `RpcFetcher::gap_request_metered` (`market_gap.rs:261–330`) liefert bei `result: null` `Err("RPC_MISSING_BLOCK")` (`:328`). Die ursprüngliche Assertion `assert!(read.is_ok())` schlug daher fehl. Das war kein Transportfehler: Die Anfrage hatte den Server erreicht.
- **Änderung:** `let _ = fetcher.gap_request_metered("getSlot", …)` mit Kommentar „A null result is no block; the request is what this checks“. Geprüft wird nur noch `first(&history_seen)[0].starts_with("POST /?api-key=HIST-SECRET ")`, also dass der Request mit dem gebundenen Credential herausgeht.
- **Bewertung:** Der Test prüft für den History-Reader **nur den Request-Transport**, nicht den Antwortpfad.
- **Positiver Antwortpfad:** Er ist für `RpcFetcher` ohne Credential durch `market_verification.rs::header_probes_never_serve_a_full_block_read` belegt (`loopback_chain()`, echte Ergebnisse über denselben `authorized_blocking`-Pfad). Die Kombination „Credential angewandt **und** Nicht-Null-Ergebnis dekodiert und gezählt“ ist **nicht** durch einen Test belegt.
- **Offene Aufgabe T-1:** Im selben Testmodul einen zweiten History-Server mit `"result": 123` (oder Blockliste) und gebundenem Query-Credential anlegen. Dann `Ok((json!(123), len))` und den Credential-Request-Line prüfen; fail-before per Mutation (`authorized_blocking` → bare `post`). Danach ist der Befund geschlossen.

## 7 Tests, Gates, CI

Gate-Wrapper: `bash <scratch>/gate-<sha>/run.sh` (Vorlage §11).
- Er läuft in `DarkBot-p0-mut` mit separatem `CARGO_TARGET_DIR`.
- Teil a: `P6_GATE_STEPS="fmt clippy_all clippy_default clippy_runner_default clippy_runner_txv1 test_all test_default test_runner_default test_runner_txv1 bash_n" bash project_0/scripts/gate.sh <dir>/a`.
- Teil b: `P6_GATE_STEPS="release python" … <dir>/b`.
- Danach `(cd frontend && npm run -s check:bsc-fields)`.
- Zählungen sind `passed/failed/ignored`.

### 7.1 Paket 1
| Lauf | SHA | Ergebnis |
|---|---|---|
| Gate | `4290ff26` (patch-identisch `3d39b438`) | **PASS**: fmt 0; clippy_all/default/runner_default/runner_txv1 0; test_all 1330/0/11; test_default 1171/0/11; test_runner_default 513/0/3; test_runner_txv1 513/0/3; release 0; python OK (skipped=2); bsc-fields PASS (3302 native, 199 proto) |
| CI | `82ae2952` | run `41291003171` **success**, 7 Jobs: native, project0-gate (fmt+clippy×4), (test_default), (test_all), (runner default+txv1), (release bash_n python), contract-and-ui |
| CI | `3d39b438` | run `41293251980` **success**, dieselben 7 Jobs |
| Reviewer | — | R1 FAIL → R2 PASS → R3 FAIL → R4 PASS (§6.1) |

### 7.2 Paket 2
| Lauf | SHA | Ergebnis |
|---|---|---|
| Gate | `5be5a30b` | **FAILED**: clippy_runner_default/txv1 exit 101 (`network` dead code ohne `crate_health`); test_all 1332/**2**/11, test_default 1174/**2**/11 (`reporter/tests/config.rs`); python failures=1 (`test_run_store` Shadow-Launcher erwartete rpc_host/URL-Redaction); runner 514/0/3; release 0; bsc-fields PASS |
| Gate | `c8c30144` | gestartet, abgebrochen ⇒ **NOT_COMPLETED** |
| Gate | `9479acb3` (patch-identisch `fffa6944`) | **PASS**: fmt 0; clippy×4 0; test_all 1340/0/11; test_default 1182/0/11; test_runner_default 517/0/3; test_runner_txv1 517/0/3; release 0; python OK (skipped=2); bsc-fields PASS (3302/199) |
| Rebase-Prüfung | `9479acb3` → `fffa6944` | `project_0`-Diff leer; Nicht-bsc-Diff = Dateien der BSC-Commits; bsc-fields PASS |
| CI | `fffa6944` | run `41302873400` **success**, 7/7 Jobs (native 124837273392, test_all 124837273564, fmt/clippy 124837273618, contract-and-ui 124837273619, release/bash_n/python 124837273712, test_default 124837273781, runner 124837273902) |
| Reviewer | — | R5 FAIL → R6 FAIL → R7 PASS (§6.2) |

### 7.3 Paket 2b + 4 (lokal, Working Tree vor dem Commit)
| Prüfung | Kommando | Ergebnis | Status |
|---|---|---|---|
| N1-Test | `cargo test -p darkbot-solana --lib` (Filter `a_registry_version_is_valid_only_whole`) | 1 passed | PASS (lokal) |
| N2 | — | kein Test | **OFFEN** |
| Projektion | `cargo test --all-features -p darkbot-runner --lib the_projection_names_hosts_and_revisions_never_a_secret` | 1 passed | PASS (lokal) |
| Gateway | `cd gateway && cargo fmt --check && cargo clippy -q --all-targets -- -D warnings && cargo test -q` | fmt 0; clippy ohne Befund; Tests 59+1+1+2+18 passed, 1 ignored | PASS (lokal) |
| Gateway-Mutation | **MUT-5** (§7.4) | Test schlug fehl, wiederhergestellt | PASS |
| Protokoll | `PROTOCOL_BASE_REF=origin/main bash protocol/scripts/check.sh --check` | `protocol contract: PASS` (Baseline `fffa6944`), Schema-ID `09470f3d…` | PASS (lokal) |
| Codegen | `bash protocol/scripts/check.sh --generate` | PASS (nach Merge mit BSC-Proto neu generiert) | PASS (lokal) |
| Frontend Playwright | `cd frontend && npx playwright test -c playwright.project0.config.ts` (nach `npm run build`) | **18 passed** (inkl. DEC-18-Test) | PASS (lokal) |
| Frontend Vitest | `npx vitest run` | 29 Dateien, **148 passed** (davon 14 Provider-Command-Tests) | PASS (lokal) |
| Lint | `npm run -s lint` (eslint), `lint:boundaries`, `lint:css` | 0 / PASS / PASS | PASS (lokal) |
| bsc-fields | `npm run -s check:bsc-fields` (11:43:57) | PASS (3456 native, 274 proto) | PASS (lokal) |
| Reporter-Tests | `cargo test -p darkbot-p0-reporter` | nicht separat gelaufen | **NOT_RUN** |
| `check:freeze`, `test:code-split` (CI contract-and-ui) | — | — | **NOT_RUN** |
| `gate.sh` (alle Feature-Sets, release, python) | Vorlage §11 | — | **NOT_RUN** |
| Reviewer | — | — | **NOT_RUN** |
| CI (3 Workflows: project0-status, gateway, bsc) | — | — | **NOT_RUN** |

### 7.4 Mutationen dieser Session (alle wiederhergestellt, mtime erneuert)
| ID | Datei | Mutation | getöteter Test | Beobachtung |
|---|---|---|---|---|
| MUT-1 | `solana/src/provider.rs` `send_until` | `http.post(target.url())` → `http.post(endpoint)` | `dec18_the_transport_applies_the_bound_credential_and_nothing_else_holds_it` | „every request carries the bound credential: ["POST / HTTP/1.1"]“ |
| MUT-2 | `solana/src/providers.rs` `authorize_with` | `/`-Einfügung bei leerem Pfad entfernt | `a_wss_query_credential_is_an_origin_form_request_target` | FAILED |
| MUT-3 (Negativmutation 1) | `runtime/src/market/deployment.rs` | `authorized_blocking(...)` → `client.post(url)` | `dec18_every_blocking_transport_…` | Request-Line `POST / HTTP/1.1` ohne Key |
| MUT-4 (Negativmutation 2) | `solana/src/provider.rs` `blocking_http` | `.redirect(Policy::none())` entfernt | `dec18_every_blocking_transport_…` | „no request reached the redirect's target“ verletzt |
| MUT-5 | `gateway/src/control.rs` `separate_secret` | gibt den Original-Request (mit Secret) zurück | `dec18_provider_commands_map_onto_p0ctl_and_no_secret_is_recorded` | „the ledger holds no secret“ |

Paket 1 (frühere Sessionhälfte, laut Commit-Nachrichten):
- M1-Racer „fails 6/6 without the lock“;
- P0-026 „failed before for 10 directories“;
- R3-M1 UUID-Test.

Die vollständige Wiederherstellung der Originale nach MUT-1…4 belegt das spätere Gate `9479acb3` auf dem committeten Inhalt. Bei MUT-5 belegt sie `grep -c "c.secret = Default::default();" = 1` nach dem Restore.

### 7.5 Weitere Testgruppen dieser Session (nicht einzeln gezählt)
`cargo test --all-features -p darkbot-runner --lib x1_` → 9 passed (10:39, vor dem R6-Fix); Solana-Crate 185 passed (11:00). Diese Zwischenstände sind durch das Gate `9479acb3` ersetzt.

## 8 Architektur und Projektregeln

**Git/Projekt** (Nutzerregeln; Memory-Dateien `bsc-absolute-boundary`, `no-commit-push`, `project0-isolated-worktree`, `feedback-*`):
- Project_0 strikt getrennt von BSC: **nie `bsc/` ändern**; Commits nur per Pathspec (`git commit -- <pfade>`); vor jedem Commit die Dateiliste prüfen (`git show --name-only` ohne `bsc/`); bei fremder Datei STOP.
- Nur `main`, **kein Branch, kein Force-Push**; Push `git push origin HEAD:main` aus einem detached Worktree, nach Rebase auf `origin/main`.
- Rebase-Prüfung:
  - `project_0`-Diff zum gegateten SHA ist leer;
  - der Nicht-bsc-Diff entspricht exakt den Dateien der BSC-Commits;
  - `check:bsc-fields` liefert PASS.
- SHAs erst nach dem Rebase in Doku zitieren.
- Höchstens 2 Agenten gleichzeitig; ein read-only Reviewer je Paket (P0-PROD-CTRL-10); keine Audit-Schleifen; kein konkurrierender Schreibzugriff (ein Gate je Worktree/Target, nie zwei `gate.sh` parallel).
- Fertige Pakete nicht neu implementieren.
- Paketabschluss nur mit: fail-before-Test je Fix, Gate auf dem finalen SHA, Reviewer auf diesem SHA, CI grün auf dem gepushten SHA. Erst dann folgt die Doku als eine Zeile je Ticket (minimal docs).
- Mindestens alle 30 min einen grünen Stand pushen; nie ungetestete WIP als PASS.
- Testdisziplin:
  - 1–3 gezielte Tests je Änderung, max. 5 Aufrufe je Schritt, `-j 2`;
  - volle Regression nur am Paketende;
  - Commits nie über `| grep` gaten (Exit-Codes direkt);
  - zsh: keine Wortaufteilung bei `$VAR` (für Schleifen `bash -c`);
  - `echo ====` vermeiden (zsh `=`-Expansion).
- Mutationen mit frischer mtime wiederherstellen (`os.utime`), sonst nutzt cargo den Mutanten-Build.
- Gateway-Änderungen: `cargo fmt --check` + `cargo clippy --all-targets -D warnings` auf stable (CI 1.99).
- Nie einen git-archive-Export nach `bsc/engine/target` bauen.
- Statusfrage des Nutzers („sag mir bevor du weitermachst“) = antworten, dann warten.

**Architektur:**

| Grundsatz | Umsetzung / Referenz |
|---|---|
| eine native Rust-Konfigurationsautorität | `solana/src/runtime_config.rs` (`RuntimeConfig`, `Resolved`, `ConfigFile`, DEC-05); Provider-Registry als deren Quelle (`bind_registry`), keine zweite Registry |
| ein fachlicher Controller für CLI und Frontend | `orchestration/provider_registry::decide`/`apply`; CLI `p0ctl provider …`; GUI über Gateway → `p0ctl` (gleiche Validatoren, CAS, Receipts) |
| intern typisiert/binär, kein interner JSON-Datenpfad | `.p0j`-Records (bincode, Domain-separiert); Protobuf nur Wire; JSON nur an externen Solana/Jupiter/Jito-Grenzen und in historischen Lesern (`historical`-Feature, P0-023 offen) |
| kein produktiver Mock/Stub/Legacy | Test-Stores nur `cfg(test)`/`test-support`+`debug_assertions`; `GeyserConfig::publicnode` nur Test; Default-Profil Jupiter versioniert statt Konstante |
| kein Hardcoding | Grenzen über `Limits`/Registry/Env mit abgeleiteten Defaults (Memory `feedback-no-hardcoding`) |
| Protobuf-Verträge | `protocol/schemas/darkbot/project0/v1/*.proto`, Codegen `protocol/scripts/check.sh --generate`, `schema_id.txt`, Goldens `protocol/golden/project0/*` |
| bestehender Gateway | `gateway/src/control.rs` (Session/CSRF/Origin/Host, Idempotenz-Ledger, Command → p0ctl) |
| genau fünf Project_0-Reiter | Overview, Liquidator, Capture, Analytics, Wallets (`frontend/src/app/projectRegistry.ts`); kein neuer Reiter |
| Status-Hinweg / Command-Rückweg | §5.4 |
| Credentials | nur macOS-Keychain; nie in Protobuf-STATE, Journal, Receipt, Gateway-Ledger, Browser, Logs; Gateway trennt das Secret vor dem Ledger |
| FREE_ONLY | `Plan::Free` erforderlich, Paid abgelehnt; kein Paid-Fallback; Qualifikation nur über passende FREE_ONLY-Evidence |
| Konfiguration ≠ Qualifikation ≠ Freigabe ≠ Betrieb | Registry (staged) / `qualification` (Evidence-ID) / Boot-Approval-Token + C-Freigaben / `EffectiveRecord` (effective) |

## 9 Verbindliche Entscheidungen und Freigaben (unverändert übernommen, ENTSCHEIDUNGEN.md §2 Nr. 18 und §4)

- **DEC-18:** Die vollständige Provider-/Credential-Verwaltung inkl. Protobuf, Gateway und Frontend wird **vor** den freigegebenen Mainnet-Messungen implementiert und offline abgenommen (§1.3, §6.1). Status lt. ENTSCHEIDUNGEN Z. 411: `DESIGN_APPROVED`. Tatsächlich: Pakete 1+2 auf `main`, Paket 4 lokal; P0-022/023/024 offen.
- **BOOT-05 / C-01:**
  - P1B→P1A nur nach vollständigem, belegtem Paritäts-PASS (C-01 Schritte 5–6, DEC-18 §2.1–2.5).
  - Keine automatische Promotion, kein bloßes Hash-Re-Pinning.
  - **C-12 NICHT ERTEILT**: kein `DEPLOYMENT_NOT_REFERENCE`-Market-Bypass, kein Fallback.
  - Nach Promotion neuer vollständiger Network-Audit.
- **C-11** (nur das Messwerkzeug, vor BOOT-05/C-04):
  - ≤ 120 s, 1 RPC-Request/s, ≤ 120 RPC-Versuche, ≤ 16 MiB kumulierte RPC-Response-Bytes;
  - **eine** WSS-Verbindung, ≤ 2 gleichzeitig aktive und ≤ 3 unterschiedliche im `ProbeSpec` genehmigte Read-Subscriptions (die dritte nur sequenziell nach Abmeldung);
  - genau ein manuell ausgewähltes FREE_ONLY-Paar, keine Paralleltests/Lastsonden;
  - modusbezogener `ProbeSpec` (BOOT-Audit, 600-s-econ4h-Kalibrierung, 30-min-C0-MO) aus realen Callgraphs;
  - typisierter `.p0j`-`PROVIDER_EVIDENCE`, keine Selbst-Qualifikation;
  - konkrete Offline-Token-Bestätigung durch den Nutzer über den Boot-/Control-Weg.
- **C-01** Schritte 5–6 ERTEILT (read-only, eigener vorab bestätigter Read-Plan, nicht von C-11 mitautorisiert). Schritt 7 BEDINGT (nur nach Paritäts-PASS, mit Review/Gate/CI und erneutem Audit).
- **C-04** Network-Audit ERTEILT, nur nach Preflights und gültiger Provider-Evidence; alle BOOT-03…08 wirklich PASS; nach Promotion erneut.
- **C-05:** eine Kalibrierung ≤ 600 s, read-only, ohne privaten Produktivschlüssel (P0-024).
- **C-10:** eine 30-min-C0-MO-Beobachtung inkl. genehmigter Token-/ATA-Bytecode-Reads (DEC-17 d), read-only; kein C0-MO-4h-PASS.
- **Nicht erteilt:** C-02, C-03, C-06 (4h-Master), C-07 (72-h-Shadow), C-08, C-09, C-12. Kein LIVE, kein Signing, keine Submission, keine wirtschaftliche Transaktion.
- **Primäres Paar:** Helius Free. Sekundär **manuell** Public Solana (`https://api.mainnet-beta.solana.com`, `wss://api.mainnet-beta.solana.com`). Kein stiller Fallback. Capture-Quelle zunächst `logs`.
- **Wallet/ECON:** alle `WALLET:<Pubkey>`, `ECON_SIGN`, `ECON_ARM`, `ECON_SEND`, `ECON_ATTEMPT`, `ECON_LIVE` initial und nach Restart `DISABLED`. BOOT-PASS/P1A-Promotion sind **keine** wirtschaftliche Freigabe. FE-04, FE-06, SignPermit, EgressPermit und Safety-Gates bleiben zusätzlich verbindlich. `live_enabled`/`submission_enabled`-Altprüfungen nie ersatzlos entfernen.
- **Jupiter:** nur Quote/Build-Reads über den P6-MT.7-Bucket; keine `/execute`/Submit-Aufrufe.
- Keine Entscheidung wurde in dieser Session neu interpretiert oder geändert.

## 10 Abhängigkeitskette bis zum genehmigten Smoke

| # | Aufgabe | was fehlt technisch / Dateien | wiederverwenden | kleinster nächster Schritt | Beweis-Tests | Voraussetzung | externe Freigabe/Messung | danach schließbar |
|---|---|---|---|---|---|---|---|---|
| D1 | Paket 2b+4 abschließen | `0a48870f`, `106300ae` ungeprüft | Gate-Wrapper, Rebase-Prüfung, `watch.sh`-Muster | §11 Schritte 4–9 | `gate.sh` voll, Gateway/Frontend/Playwright/`check.sh --check`, Reviewer, CI ×3 | Rebase auf `origin/main` | keine | P0-020 → OFFLINE_VERIFIED (mit D3-Tests); R7-N1/N2 |
| D2 | Doku nachziehen | BUGS P0-017…021, STATUS Z. 322, F0-VAR-Inventar regenerieren, CI-ID-Korrektur (§5.6) | bestehende Zeilen | ein Docs-Commit nach D1 | `test_bugs_register.py` (Gate python) | D1 gepusht | keine | Doku konsistent |
| D3 | DEC-18-Restpunkte | T-1 (§6.4); R-PY: `c0_census.py`, `measurement_contract.py`, `audit_market_quick.py` ohne `HELIUS_API_KEY`-Ableitung (Registry/Keychain oder nur `historical`); fehlende §1.3-Nr.-9-Tests (verlorenes ACK, Detach, Berechtigung, Konkurrenz) für Provider-Commands; N2-Test; B-KC | `provider_registry::credentials_of`, `activate_tool` (`:778`), generische Command-Tests in `control.rs` | T-1-Test | je Fix fail-before | D1 | B-KC: lokaler realer Keychain-Lauf mit **Dummy-Credential** (kein Netz) durch Nutzer/Operator | P0-019, P0-020 OFFLINE_VERIFIED |
| D4 | P0-023 | `provider_evidence.rs:39` `from_measured_json` hinter `historical`; Consumer `p0ctl boot evidence import`, `p0-journal import-provider-evidence` abgleichen | Feature `historical` (DEC-09, `runtime/Cargo.toml:14`) | Consumer-Inventar | Build ohne/mit Feature, historischer Lesetest | D1 | keine | P0-023 |
| D5 | P0-024 | `state_owner/setup.rs:240` lädt Keypair; `scripts/capture.sh:67`; öffentlicher Taker-/Wallet-Pubkey, `UNSIGNED_SIMULATION` (`sig_verify=false`) | typisierte Payer-Konfiguration (neu, DEC-18 §4.3); Exit-Admission | Payer-Typ (Key vs. Public-only) | Test: econ4h-Kalibrierung startet ohne `P0_KEYPAIR_PATH`; Signaturpfade weiter gesperrt | D1 | keine | P0-024 (Pflicht vor C-05) |
| D6 | **DEC-18 offline vollständig** | D1–D5 grün, CI auf finalem SHA | — | — | Gate+Reviewer+CI | D1–D5 | keine | DEC-18 §6.1, P0-017…021/023/024 |
| D7 | P0-022 C-11-Werkzeug | `p0ctl provider measure`; `ProbeSpec` je Modus aus Callgraphs (BOOT: getGenesisHash/getVersion/getHealth/getSlot/getMultipleAccounts; econ4h-600s und C0-MO-30min aus ihren Adaptern); Ergebnis je Methode `SUPPORTED`/`METHOD_UNSUPPORTED`/`NOT_PROVEN`/`BLOCKED`; Pre-Audit-Read-Permit mit Offline-Token; C-11-Grenzen hart | `provider_evidence.rs` Store/Validierung, `boot_control` Token-Fluss, `provider::shared()` Budget, `BoundProvider`-Bindung, `qualify` | ProbeSpec-Ableitung + Abweichungstest | Loopback: Timeout, 429, falsche Credential-Revision, Drift, kaputter Record, Doppelmessung, Abbruch, Wiederholung, Restart (DEC-18 §3.8) | D6 | keine (Bau offline) | P0-022 OFFLINE_VERIFIED |
| D8 | C-11-Messung | realer Lauf | D7 | Nutzer bestätigt Offline-Token | Record, `p0ctl provider qualify` | D7 | **C-11 Token des Nutzers**; Helius-Free-Key im Keychain | Provider qualifiziert (EXTERNAL_QUALIFIED) |
| D9 | P0-025 + C-01 Schritte 5–6 | native Read-Werkzeuge: Bank-Resize-Census je Group, IDL/Upgrade-Authority, ProgramData-Bytecode, Token-/ATA-Bytecode (litesvm nur nach Bewertung) | Provider-Transport, budgetierte Reads, typisierte Records | Werkzeugdesign offline | Offline-Fixtures | D6 (Bau), D8 (Provider) | **C-01-Read-Plan + Token des Nutzers** | P0-025; Paritätsbeleg |
| D10 | C-01 Schritt 7 | Promotion P1B→P1A (`CURRENT_MAINNET.md`, Code-Pins) nur bei vollständigem Paritäts-PASS | RC2 `c9decd7f` ist kein Beweis | — | Review/Gate/CI | D9 PASS | bedingt erteilt | BOOT-05; sonst BLOCKED mit Ursachen |
| D11 | P0-027 | Desktop-Market-Flow: `APP_HOME` ohne Git, Release ohne `p0-longtail` → BOOT-01 | `darkbotctl build` → `deploy`, Boot-Controller | Codeprüfung (Befund verifizieren) | Integrationstest | vor C-10 | keine | P0-027 |
| D12 | C-04 Network-Audit | neuer Audit der finalen Identität, alle BOOT-03…08 PASS | Boot-Controller | — | Audit-Record | D8, D10 (bzw. BLOCKED), D11 | **C-04 Token** | P0-016 positiver Realstart möglich |
| D13 | C-05 600-s-Kalibrierung | — | Startpfad | — | Run-/Provider-/Budget-/Coverage-Ledger | D12, D5 | **C-05 Token** | MEASUREMENT_LEDGER |
| D14 | C-10 30-min-C0-MO | inkl. Token-/ATA-Bytecode-Reads | derselbe Startpfad | — | Ledger, kein 4h-PASS | D12, D11 | **C-10 Token** | — |

Danach: DEC-17 (P8-CTRL-02 zuerst) und die übrigen REV.1.0-Pakete nach PHASENPLAN.

**Blocker B-KC:** Der Zugriff der Runtime-/Reporter-Binaries auf Keychain-Items, die `p0ctl` angelegt hat, ist `UNKNOWN`: unsignierte Builds sind cdhash-gebunden, headless droht `errSecInteractionNotAllowed`. Das muss vor D8 mit einem **Dummy-Credential** lokal real geprüft werden (kein Netz nötig). Andernfalls scheitert der erste echte Start mit `CREDENTIAL_UNAVAILABLE`.

## 11 Wiedereinstieg für den nächsten Claude-Account

**Orte:**
- Repository-Haupt-Checkout: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`. Das ist BSC (nicht anfassen); dort laufen ggf. BSC-Sessions.
- Project_0-Entwicklung: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-ctrl` (detached).
- Gate/Rebase/Push: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-mut` (detached).
- Maßgeblicher Commit: `origin/main` (beim Freeze `4fbf4ddb`) plus die zwei lokalen Commits `0a48870f`/`106300ae` (Basis `fffa6944`).

**Zuletzt:**
- Zuletzt erfolgreich: Gate `9479acb3` PASS, CI `41302873400` success; lokal Frontend 18/18 Playwright, 148/148 Vitest, Gateway 81 passed, `check.sh --check` PASS.
- Zuletzt fehlgeschlagen: Gate `5be5a30b` (behoben).
- Zuletzt bearbeiteter Befund: R7-N1/N2 (in `0a48870f`).
- In Bearbeitung war Paket 4 (P0-020), Dateien §4.2. Es ist committet, aber ungeprüft.

**Nicht erneut ausführen:**
- Pakete 1/2 nicht neu implementieren.
- DEC-18 nicht erneut in ENTSCHEIDUNGEN/PHASENPLAN übertragen (`04d1d1ad`).
- P0-026 nicht erneut bearbeiten.
- Bestandene Reviews R2/R4/R7 nicht wiederholen.
- Keine Mainnet-Reads/Messungen ohne konkretes Nutzer-Token.

**Prozedur** (Schritte 1–3 nur lesen; Änderungen erst ab Schritt 4, nachdem alles abgeglichen ist):

```bash
# 1. Repo, Remote, Worktrees, Status (nur lesen)
cd /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-ctrl
git fetch origin main && git rev-parse origin/main
git worktree list
for w in DarkBot DarkBot-p0-ctrl DarkBot-p0-mut; do echo "== $w"; git -C ../$w rev-parse HEAD; git -C ../$w status --porcelain=v1 -uall | head -60; done
# erwartet: p0-ctrl/p0-mut HEAD 106300ae…, p0-mut sauber, p0-ctrl nur "?? frontend/node_modules"
git log --oneline fffa69440dc8a85d2a7ac05021df4cfe05ebfcd0..106300ae004f9e140a96f1dd5842a25663bd656b   # 2 Commits
git rev-parse 106300ae^{tree}        # 8a071ddc0b73b34e3c441b75f329c45828553f71
git log --oneline 106300ae..origin/main   # was seit dem Freeze auf main kam (BSC?)
git diff --name-only fffa6944 origin/main | sort > /tmp/p0_main_files; git diff --name-only fffa6944 106300ae | sort > /tmp/p0_wip_files; comm -12 /tmp/p0_main_files /tmp/p0_wip_files   # Überschneidungen → §3.4

# 2. Handoff + Patch prüfen (nur lesen); Weg B nur falls 106300ae fehlt (§4.4)
git show origin/main:project_0/docs/SESSION_HANDOFF_DEC18_2026-11-01.md | shasum -a 256
# (den Wert mit dem in der Commit-Nachricht des Handoff-Commits genannten vergleichen)

# 3. Autoritäten lesen: PHASENPLAN ### DEC-18 (§6), ENTSCHEIDUNGEN §2 Nr. 18 + §4, BUGS P0-016…027, STATUS Z. 322/323
#    Memory-Index (falls vorhanden): ~/.claude/projects/-Users-sawyer-ma-Desktop-Firma-2-DarkMa-6-DarkBot-DarkBot/memory/MEMORY.md

# 4. Rebase der zwei Commits in DarkBot-p0-mut auf das aktuelle main (Worktree sauber!)
cd /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-mut
git status --porcelain=v1 -uall      # muss leer sein
git rebase origin/main               # Konflikte nur gemäß §3.4 lösen (regenerieren, nicht von Hand)
NEW=$(git rev-parse HEAD)
git diff --stat 106300ae $NEW -- project_0 gateway frontend/src frontend/tests ops protocol/schemas   # muss leer sein, außer durch BSC-Änderungen gemeinsamer Dateien erklärbar
diff <(git diff --name-only 106300ae $NEW -- . ':!bsc' | sort) <(git diff --name-only fffa6944 origin/main -- . ':!bsc' | sort) && echo SAME
(cd frontend && npm run -s check:bsc-fields)
git show --name-only --format= HEAD HEAD~1 | grep -c '^bsc/'   # muss 0 sein

# 5. Volles Gate auf $NEW (ein Gate zur Zeit; eigener Target-Ordner)
G=$HOME/p0-gate-$NEW; mkdir -p "$G"
CARGO_TARGET_DIR=$HOME/p0-target-mut P6_GATE_STEPS="fmt clippy_all clippy_default clippy_runner_default clippy_runner_txv1 test_all test_default test_runner_default test_runner_txv1 bash_n" bash project_0/scripts/gate.sh "$G/a" > "$G/a.out" 2>&1; echo gate_a=$?
CARGO_TARGET_DIR=$HOME/p0-target-mut P6_GATE_STEPS="release python" bash project_0/scripts/gate.sh "$G/b" > "$G/b.out" 2>&1; echo gate_b=$?
(cd frontend && npm run -s check:bsc-fields); grep -h "exit=" "$G/a.out" "$G/b.out"
(cd gateway && cargo fmt --check && cargo clippy -q --all-targets -- -D warnings && cargo test -q)
PROTOCOL_BASE_REF=origin/main bash protocol/scripts/check.sh --check
(cd project_0/native && cargo test -p darkbot-p0-reporter)
# Frontend wie CI (contract-and-ui) im Dev-Worktree, der frontend/node_modules als Symlink hat:
cd ../DarkBot-p0-ctrl && git status --porcelain=v1 -uall   # nur "?? frontend/node_modules"
git checkout --detach $NEW
(cd frontend && npm test && npm run build && npm run lint && npm run lint:boundaries && npm run check:freeze && npm run lint:css && npm run test:project0 && npm run test:code-split)
cd ../DarkBot-p0-mut

# 6. Ein read-only Reviewer (P0-PROD-CTRL-10) auf `git diff fffa6944 $NEW -- . ':!bsc'`, Prüfumfang §6.3 + R7-N1/N2 + T-1-Hinweis
# 7. Befunde fixen (je Fix fail-before-Test, Mutation mit frischer mtime), Gate auf finalem SHA wiederholen
# 8. Push ohne Force:  git push origin HEAD:main   (nur wenn origin/main == Basis des Rebase; sonst zurück zu Schritt 4)
# 9. CI: drei Workflows (project0-status, gateway, bsc) auf dem gepushten SHA müssen success sein (GitHub-API /actions/runs?head_sha=<sha>)
# 10. Docs-Commit (D2), dann D3 → D4 → D5 → D6, dann P0-022 (D7); Netzwerkschritte erst mit Nutzer-Token
```

Der nächste Commit soll der Rebase von `0a48870f` + `106300ae` auf `main` sein, nach Gate und Review, ggf. mit Review-Fixes. Danach folgt das Paket Doku (D2), dann DEC-18-Restpunkte (D3–D5), dann C-11 (P0-022).

## 12 Integritätsprüfung vor dem Push

| Prüfpunkt | Ergebnis |
|---|---|
| alle offenen Tickets P0-016…027 enthalten | ja (§5.5) |
| erledigte Arbeit mit SHA und CI belegt | ja (§5.1, §5.2, §7). Lücke: kein CI-Lauf für Zwischen-SHAs (§5.6 Nr. 2) |
| lokale Änderungen reproduzierbar | ja: Patch Anhang A, SHA-256 `6c2cc4ed…`; `git am` reproduziert exakt `0a48870f`/`106300ae` (geprüft im Scratch-Clone) |
| untracked Dateien berücksichtigt | ja: p0-ctrl nur Symlink `frontend/node_modules`; Haupt-Checkout 54 BSC-Dateien (Anhang B), nicht Project_0 |
| Basiscommit/Patch stimmen | ja: Basis `fffa6944`, Baum vorher `c0a11a6f…`, nachher `8a071ddc…` |
| SHA-256-Werte angegeben | ja (§2, §4) |
| aktueller HEAD korrekt | `origin/main` `4fbf4ddb` beim Freeze (vor dem Push erneut geprüft) |
| Entscheidungen konsistent mit PHASENPLAN/ENTSCHEIDUNGEN | ja: §9 aus ENTSCHEIDUNGEN Z. 353–451 übernommen, nichts geändert |
| Reviewer-Befunde vollständig | ja: R1–R7 aus den Agent-Reports (§6); Paket 4 ohne Review |
| keine Geheimnisse | Patch geprüft: kein Wert aus `project_0/.env` enthalten (Abgleich ohne Ausgabe); nur Testfixtures (`KEY`, `KEY-ONE-SECRET`, `PLAYWRIGHT-KEY-1`, `TYPED-KEY`, `DEP-SECRET` u. a., Platzhalter-UUID `12345678-1234-…`); kein Private Key, keine Seed-Phrase |
| umgesetzt/getestet/gepusht/offen klar getrennt | ja (§0, §5, §7) |
| Widersprüche Git ↔ Doku | ja, benannt in §5.6 (BUGS/STATUS-Rückstand, CI-ID, F0-VAR-Inventar) |
| nächste Schritte ausführbar | §11 mit Kommandos |
| `UNKNOWN` | Keychain-ACL realer Binaries (B-KC); fmt-Stand von `0a48870f` allein; Abdeckung der generischen §1.3-Nr.-9-Tests für Provider-Actions |

## Anhang B — `git status --porcelain=v1 -uall` des Haupt-Checkouts `DarkBot` beim Freeze (54 Zeilen, alle BSC-eigen, nicht anfassen)

```text
?? bsc/config/g07_attempt1_acceptance_v4.dbrec
?? bsc/config/g07_attempt1_acceptance_v4.e1-review.dbrec
?? bsc/docs/9_ALT/Backup_v1.zip
?? bsc/engine/crates/execution/src/runtime_live.rs.orig
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/COMPARISON_dry_result.md
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/COMPARISON_economics_crosscheck.md
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/COMPARISON_local_simulation.md
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/COMPARISON_preflight.md
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/SHA256SUMS
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/builder_policy.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/builder_policy.view.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/dry_result.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/dry_result.view.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/economics_crosscheck.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/economics_crosscheck.out.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/local_simulation.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/local_simulation.out.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/native_quote.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/native_quote.view.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/portal_upgrade.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/portal_upgrade_scan.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/portal_upgrade_scan.progress.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/portal_upgrade_scan.view.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/preflight.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/preflight.progress.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/preflight.view.txt
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/requalification_status.dbrec
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/requalification_status.view.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/SHA256SUMS
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/builder_policy.dbrec
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/builder_policy.view.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/economics_crosscheck.dbrec
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/economics_crosscheck.out.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/launch_path.dbrec
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/launch_path.out.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/launch_path.view.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/native_quote.dbrec
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/native_quote.view.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/portal_upgrade.dbrec
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/portal_upgrade.view.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/portal_upgrade_scan.dbrec
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/preflight.dbrec
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/preflight.progress.txt
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/preflight.view.txt
?? bsc/native/evidence/g07/readiness_20261020T120918Z/SHA256SUMS
?? bsc/native/evidence/g07/readiness_20261020T120918Z/clock-context.txt
?? bsc/native/evidence/g07/readiness_20261020T120918Z/readiness.dbrec
?? bsc/native/evidence/g07/readiness_20261020T120918Z/readiness.view.txt
?? bsc/native/evidence/g07/readiness_20261020T120918Z/stderr.txt
?? bsc/native/evidence/g07/readiness_20261020T121103Z/SHA256SUMS
?? bsc/native/evidence/g07/readiness_20261020T121103Z/clock-context.txt
?? bsc/native/evidence/g07/readiness_20261020T121103Z/readiness.dbrec
?? bsc/native/evidence/g07/readiness_20261020T121103Z/readiness.view.txt
?? bsc/native/evidence/g07/readiness_20261020T121103Z/stderr.txt
```

## Anhang A — Wiederherstellungsdaten: Patch `fffa6944..106300ae` (2 Commits, 34 Dateien)

- Erzeugt mit: `git format-patch --binary --full-index --stdout fffa69440dc8a85d2a7ac05021df4cfe05ebfcd0..106300ae004f9e140a96f1dd5842a25663bd656b`
- SHA-256 des Patches (Inhalt zwischen den Fence-Zeilen, LF-Zeilenenden): `6c2cc4ed45a9cb808537ac102f7ee35775886e0e481a0474b1812d190ad381a6` · 220547 Bytes · 3339 Zeilen
- Extraktion und Anwendung: §4.4. Ergebnis: Commits `0a48870f4f56a7c33b401101a8da32c16fbd6d4b`, `106300ae004f9e140a96f1dd5842a25663bd656b`, Baum `8a071ddc0b73b34e3c441b75f329c45828553f71`
- Nicht bearbeiten: Jede Änderung zwischen den Markern bricht die SHA-256-Prüfung.

<!-- DEC18-PATCH-BEGIN -->
~~~~~~~~~~~~~~~~ diff
From 0a48870f4f56a7c33b401101a8da32c16fbd6d4b Mon Sep 17 00:00:00 2001
From: Sawyer Ma <mmw-solutions@web.de>
Date: Sun, 1 Nov 2026 12:43:57 +0100
Subject: [PATCH 1/2] =?UTF-8?q?fix(p0):=20DEC-18=20package=202=20re-check?=
 =?UTF-8?q?=20(N1,=20N2)=20=E2=80=94=20a=20Geyser=20provider=20is=20a=20Ye?=
 =?UTF-8?q?llowstone=20one,=20and=20a=20drifted=20econ4h=20start=20keeps?=
 =?UTF-8?q?=20its=20manifest?=
MIME-Version: 1.0
Content-Type: text/plain; charset=UTF-8
Content-Transfer-Encoding: 8bit

- N1: a Geyser provider authenticates by the protocol's header:x-token or not at all (refused at
  entry, PROVIDER_AUTH_UNSUPPORTED — never a provider every start then fails on), and the registry
  selects no Geyser provider (the source unit names its own: PROVIDER_SELECTION_GEYSER); tested.
- N2: an econ4h configuration drifted since its admission (activated_is_admitted) is an
  admission refusal (BOOT_IDENTITY_DRIFT) handed to run_econ4h, so the refused run keeps its local
  manifest like every other refusal.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
---
 .../runtime/src/orchestration/startup.rs      | 12 +++++--
 .../native/crates/solana/src/providers.rs     | 36 +++++++++++++++++++
 2 files changed, 46 insertions(+), 2 deletions(-)

diff --git a/project_0/native/crates/runtime/src/orchestration/startup.rs b/project_0/native/crates/runtime/src/orchestration/startup.rs
index d8d84975529a1f1782d540ea89446a82b859b5e7..589966e6bd77659fb0e6dc296c0634886651a84d 100644
--- a/project_0/native/crates/runtime/src/orchestration/startup.rs
+++ b/project_0/native/crates/runtime/src/orchestration/startup.rs
@@ -223,8 +223,16 @@ pub fn run_with(after_owner_lock: &mut dyn FnMut()) -> Result<(), Box<dyn std::e
         })
     };
     if mode == "econ4h" {
-        if let Ok(record) = &admission {
-            admitted(record).map_err(OperationalConfigError)?;
+        // A configuration drifted since the admission is an admission refusal (the refused run
+        // keeps its local manifest, like every other).
+        let admission = admission.and_then(|record| match admitted(&record) {
+            Ok(()) => Ok(record),
+            Err(e) => {
+                crate::console::line(format!("BOOT admission refused: {e}"));
+                Err("BOOT_IDENTITY_DRIFT")
+            }
+        });
+        if admission.is_ok() {
             credentials(&bound_registry).map_err(OperationalConfigError)?;
         }
         let code = admission.as_ref().err().copied();
diff --git a/project_0/native/crates/solana/src/providers.rs b/project_0/native/crates/solana/src/providers.rs
index 4e8ec1a9c8aaf9b1eed73d47eba6a9501ef7ebe5..eeb770314bb50bde3ba136b0af152010565f0f53 100644
--- a/project_0/native/crates/solana/src/providers.rs
+++ b/project_0/native/crates/solana/src/providers.rs
@@ -307,6 +307,16 @@ impl ProviderDescriptor {
         if AuthKind::parse(&self.auth.text())? != self.auth {
             return Err("PROVIDER_AUTH_NAME ([A-Za-z0-9_-]{1,64})".into());
         }
+        // A Geyser provider is a Yellowstone gRPC source: none or the protocol's token header.
+        let yellowstone = crate::ingress::sources::YELLOWSTONE_TOKEN_HEADER;
+        if self.kind == ProviderKind::Geyser
+            && !matches!(&self.auth, AuthKind::None)
+            && self.auth != AuthKind::Header(yellowstone.into())
+        {
+            return Err(format!(
+                "PROVIDER_AUTH_UNSUPPORTED: a Geyser provider authenticates by header:{yellowstone} or none"
+            ));
+        }
         if self.qualification.as_ref().is_some_and(|q| !nonce_ok(q)) {
             return Err("PROVIDER_QUALIFICATION_INVALID".into());
         }
@@ -587,6 +597,11 @@ impl ProviderRegistry {
         }
         let mut kinds = std::collections::BTreeSet::new();
         for (kind, id) in &self.selected {
+            // The Geyser source unit names its provider (`source-geyser-provider`, S1-02): a
+            // selection of the kind would be a second authority.
+            if *kind == ProviderKind::Geyser {
+                return Err("PROVIDER_SELECTION_GEYSER: the Geyser source unit names its provider (source-geyser-provider)".into());
+            }
             if !kinds.insert(*kind) {
                 return Err(format!("PROVIDER_SELECTION_DUPLICATE: {}", kind.name()));
             }
@@ -1358,6 +1373,27 @@ mod tests {
             .validate()
             .unwrap_err()
             .starts_with("PROVIDER_SELECTION_DISABLED"));
+        // A Geyser provider authenticates by the Yellowstone token header or not at all; the
+        // Geyser source unit names its provider, so the registry selects none of the kind.
+        let mut geyser = helius();
+        (geyser.id, geyser.kind) = ("geyser".into(), ProviderKind::Geyser);
+        geyser.auth = AuthKind::Query("token".into());
+        assert!(geyser
+            .validate()
+            .unwrap_err()
+            .starts_with("PROVIDER_AUTH_UNSUPPORTED"));
+        geyser.auth = AuthKind::Header("x-token".into());
+        assert!(geyser.validate().is_ok());
+        let mut selects_geyser = r.clone();
+        selects_geyser.providers[0].enabled = true;
+        selects_geyser.providers.push(geyser);
+        selects_geyser
+            .selected
+            .push((ProviderKind::Geyser, "geyser".into()));
+        assert!(selects_geyser
+            .validate()
+            .unwrap_err()
+            .starts_with("PROVIDER_SELECTION_GEYSER"));
         let mut bad = r.clone();
         bad.uid = String::new();
         assert_eq!(bad.validate().unwrap_err(), "PROVIDER_REGISTRY_UID_INVALID");
-- 
2.53.0


From 106300ae004f9e140a96f1dd5842a25663bd656b Mon Sep 17 00:00:00 2001
From: Sawyer Ma <mmw-solutions@web.de>
Date: Sun, 1 Nov 2026 12:44:34 +0100
Subject: [PATCH 2/2] =?UTF-8?q?feat(p0):=20DEC-18=20package=204=20(P0-020,?=
 =?UTF-8?q?=20P0-019=20GUI)=20=E2=80=94=20the=20provider=20registry=20end?=
 =?UTF-8?q?=20to=20end=20in=20the=20Capture=20and=20Liquidator=20tabs?=
MIME-Version: 1.0
Content-Type: text/plain; charset=UTF-8
Content-Transfer-Encoding: 8bit

- Status (Hinweg): the registry's secret-free projection (provider_registry::projection: host,
  authentication name, credential id and revision, plan, priority, capacity, binding, selection
  and its block; a copied store refused with its reason) in the analytics snapshot → reporter →
  project0.analytics.jobs (AnalyticsJobs.providers = ProviderRegistryView, field 10/11) → adapter
  → store → ProvidersSection; the running process's bound providers (effective) in
  project0.status (EffectiveRecord.providers = RunProvider, field 22). Requested (registry vN,
  staged: next controlled start) and effective side by side; Capture shows RPC/WSS/Geyser,
  Liquidator Jupiter/Jito (its tab now holds topic 15). No new tab.
- Commands (Rückweg): project0.provider.add|update|remove|enable|disable|priority|select|unselect|
  qualify|credential.set|rotate|revoke through the one POST /api/v1/command path → gateway
  (typed params, CAS: config_version = the registry version, request id) → `darkbotctl p0
  provider …` → `p0ctl provider …` (the same controller, store and receipts as the terminal); the
  registry's PROVIDER_RECEIPT is the receipt's detail.
- Credential input (P0-019): the key travels only in ProviderCredentialCommand.secret; the
  gateway separates it before any durable record (the ledger holds the request without it, a
  replay compares exactly that), hands it to the controller's standard input only (never an
  argument, the environment, a log or the receipt) and wipes its copy on every path; controller
  children never inherit the gateway's standard input. An endpoint with user info, a query, a
  fragment or a token-like path segment and a key-shaped name are refused before the ledger (the
  descriptor's rules; the native registry decides). The GUI clears the key field after sending and
  keeps it in no store or browser storage.
- darkbotctl routes `p0 provider` (stdin passes through its exec). bsc-fields re-pinned for
  gateway/src/control.rs (13 hashes). Wire V1.3 §7 and the gateway README name the credential
  input; F0-VAR C-PV-1/2, L-PV-1/2 connected (C-PV-3/4: C-11, P0-022).
- Tests: gateway — argv per action, CAS required, keys in endpoint/name refused before the ledger,
  the secret only on the controller's stdin and nowhere in the ledger (fails with the separation
  removed), replay without a second run, refusal receipt; reporter golden (analytics_jobs) and the
  frontend decoding it; Vitest builders, the key sent once and kept nowhere; Playwright through
  the tabs (add, a key in the endpoint refused before sending, enable, set key: params decoded,
  field cleared, nothing in storage or page); the registry projection (secret-free, foreign
  store refused).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
---
 docs/protocol/PROTOCOL.md                     | 191 ++++++
 frontend/contracts/bsc-fields.json            |  26 +-
 frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md   |   1 +
 frontend/src/app/projectRegistry.ts           |   3 +-
 .../darkbot/project0/v1/analytics_pb.ts       | 488 ++++++++++++++-
 .../darkbot/project0/v1/system_pb.ts          |  75 ++-
 .../core/realtime/analyticsControl.test.ts    |  52 ++
 .../src/core/realtime/analyticsControl.ts     |  70 ++-
 .../src/features/project0/Project0Page.tsx    |   2 +-
 .../project0/api/analyticsAdapter.test.ts     |  19 +
 .../features/project0/api/analyticsAdapter.ts |  20 +-
 .../project0/api/project0ViewAdapter.ts       |   4 +
 .../project0/components/ProvidersSection.tsx  | 113 ++++
 .../src/features/project0/model/analytics.ts  |  46 ++
 .../src/features/project0/model/status.ts     |  16 +
 .../project0/model/statusSelectors.test.ts    |   2 +-
 .../features/project0/pages/CapturePanel.tsx  |   2 +
 .../project0/pages/LiquidatorPanel.tsx        |   5 +-
 frontend/tests/project0-status.spec.ts        |  64 +-
 gateway/README.md                             |   2 +-
 gateway/src/control.rs                        | 581 +++++++++++++++++-
 ops/darkbotctl                                |   6 +-
 project_0/docs/F0_VAR_01_MATRIX.md            |  10 +-
 .../native/crates/reporter/src/analytics.rs   |  47 +-
 project_0/native/crates/reporter/src/lib.rs   |  93 ++-
 .../crates/runtime/src/analytics/jobs.rs      |   8 +
 .../src/orchestration/provider_registry.rs    | 143 +++++
 .../native/crates/runtime/src/status_tap.rs   |   1 +
 protocol/golden/project0/analytics_jobs.hex   |   2 +-
 protocol/schema_id.txt                        |   2 +-
 .../darkbot/project0/v1/analytics.proto       | 127 ++++
 .../schemas/darkbot/project0/v1/system.proto  |  21 +
 32 files changed, 2162 insertions(+), 80 deletions(-)
 create mode 100644 frontend/src/features/project0/components/ProvidersSection.tsx

diff --git a/docs/protocol/PROTOCOL.md b/docs/protocol/PROTOCOL.md
index d63849b39b36d298cfb1c55d1954946de26c36d5..72d3149c1a1a8ef976d3206152ea54f2808cee3a 100644
--- a/docs/protocol/PROTOCOL.md
+++ b/docs/protocol/PROTOCOL.md
@@ -131,6 +131,14 @@
     - [PostCaptureJobCommand](#darkbot-project0-v1-PostCaptureJobCommand)
     - [PostCaptureResult](#darkbot-project0-v1-PostCaptureResult)
     - [PostCaptureSpec](#darkbot-project0-v1-PostCaptureSpec)
+    - [ProviderCredentialCommand](#darkbot-project0-v1-ProviderCredentialCommand)
+    - [ProviderDescriptorCommand](#darkbot-project0-v1-ProviderDescriptorCommand)
+    - [ProviderPriorityCommand](#darkbot-project0-v1-ProviderPriorityCommand)
+    - [ProviderQualifyCommand](#darkbot-project0-v1-ProviderQualifyCommand)
+    - [ProviderRegistryView](#darkbot-project0-v1-ProviderRegistryView)
+    - [ProviderSelectCommand](#darkbot-project0-v1-ProviderSelectCommand)
+    - [RegistryCredential](#darkbot-project0-v1-RegistryCredential)
+    - [RegistryProvider](#darkbot-project0-v1-RegistryProvider)
     - [RunQualification](#darkbot-project0-v1-RunQualification)
     - [ShadowCounterfactual](#darkbot-project0-v1-ShadowCounterfactual)
     - [UntilStopped](#darkbot-project0-v1-UntilStopped)
@@ -195,6 +203,7 @@
     - [EffectiveRecord.SourcesEntry](#darkbot-project0-v1-EffectiveRecord-SourcesEntry)
     - [OperationalConfig](#darkbot-project0-v1-OperationalConfig)
     - [OperationalParameter](#darkbot-project0-v1-OperationalParameter)
+    - [RunProvider](#darkbot-project0-v1-RunProvider)
     - [SystemStatus](#darkbot-project0-v1-SystemStatus)
 
 - [Scalar Value Types](#scalar-value-types)
@@ -2022,6 +2031,8 @@ i128 as SignedDecimal (*_signed), u128 sums as UnsignedDecimal (*_sum).
 | boot | [BootFlow](#darkbot-project0-v1-BootFlow) |  | DEC-07: the boot controller&#39;s flow (data/boot, `p0ctl boot …`) — the one with an active step, else the newest — shown on the Capture tab: this always-on feed is the controller&#39;s projection while no capture runs. Unset without a flow. |
 | boot_error | [string](#string) |  | DEC-07: why the boot store was not (completely) readable; empty when read. |
 | until_stopped | [UntilStopped](#darkbot-project0-v1-UntilStopped) |  | DEC-16: unbounded runs beside the boot flow — the standing autostart approval, the newest raw-capture qualification and the newest unbounded raw capture. |
+| providers | [ProviderRegistryView](#darkbot-project0-v1-ProviderRegistryView) |  | DEC-18: the provider registry (data/providers, `p0ctl provider …`) as its store holds it — staged: a change is effective at the next controlled start (the running run&#39;s providers: project0.status EffectiveRecord.providers). Unset when the store is unreadable. |
+| providers_error | [string](#string) |  | DEC-18: why the provider registry was not readable (providers is then unset); empty when read. |
 
 
 
@@ -2547,6 +2558,164 @@ The verification request of a post-capture job (A08/F-02).
 
 
 
+<a name="darkbot-project0-v1-ProviderCredentialCommand"></a>
+
+### ProviderCredentialCommand
+DEC-18: params of `project0.provider.credential.set` and `…credential.rotate`. The gateway
+separates the secret before any durable record (its command ledger holds the request without
+it) and hands it to the native controller&#39;s standard input only.
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| credential | [string](#string) |  | Credential id. |
+| secret | [bytes](#bytes) |  | The secret (in transit only). |
+
+
+
+
+
+
+<a name="darkbot-project0-v1-ProviderDescriptorCommand"></a>
+
+### ProviderDescriptorCommand
+DEC-18: params of `project0.provider.add` and `project0.provider.update` (the registry&#39;s
+rules decide: a canonical endpoint without a credential in it, FREE_ONLY, a known kind). An
+update changes only the fields it sets.
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| id | [string](#string) |  | Provider id. |
+| kind | [string](#string) | optional | rpc | wss | geyser | jupiter | jito (add: required). |
+| endpoint | [string](#string) | optional | The endpoint (add: required); refused with a credential in it, never echoed. |
+| auth | [string](#string) | optional | none | query:NAME | header:NAME | path. |
+| credential | [string](#string) | optional | The credential id it authenticates with. |
+| priority | [uint32](#uint32) | optional | Priority among its kind. |
+| capacity_rps | [uint64](#uint64) | optional | Configured capacity in requests per second. |
+| plan | [string](#string) | optional | free | unknown. |
+| entitlement | [string](#string) | optional | The plan&#39;s name. |
+| disabled | [bool](#bool) |  | Added disabled (add only). |
+
+
+
+
+
+
+<a name="darkbot-project0-v1-ProviderPriorityCommand"></a>
+
+### ProviderPriorityCommand
+DEC-18: params of `project0.provider.priority`.
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| id | [string](#string) |  | Provider id. |
+| priority | [uint32](#uint32) |  | The new priority. |
+
+
+
+
+
+
+<a name="darkbot-project0-v1-ProviderQualifyCommand"></a>
+
+### ProviderQualifyCommand
+DEC-18: params of `project0.provider.qualify`.
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| id | [string](#string) |  | Provider id. |
+| evidence | [string](#string) |  | The stored provider evidence record id (16 hex), or &#34;-&#34; to clear the qualification. |
+
+
+
+
+
+
+<a name="darkbot-project0-v1-ProviderRegistryView"></a>
+
+### ProviderRegistryView
+DEC-18: one committed version of the provider registry — secret-free (hosts, credential ids
+and revisions; never an endpoint&#39;s path, a key or a keychain account).
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| version | [uint64](#uint64) |  | The committed version (0: no provider yet); the expected version of every change (CAS). |
+| providers | [RegistryProvider](#darkbot-project0-v1-RegistryProvider) | repeated | Every provider, by kind, priority and id. |
+| credentials | [RegistryCredential](#darkbot-project0-v1-RegistryCredential) | repeated | Every credential&#39;s newest revision. |
+| restart_rule | [string](#string) |  | When a change takes effect: next_controlled_start (DEC-05). |
+
+
+
+
+
+
+<a name="darkbot-project0-v1-ProviderSelectCommand"></a>
+
+### ProviderSelectCommand
+DEC-18: params of `project0.provider.select`.
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| kind | [string](#string) |  | rpc | wss | jupiter | jito. |
+| id | [string](#string) |  | Provider id. |
+
+
+
+
+
+
+<a name="darkbot-project0-v1-RegistryCredential"></a>
+
+### RegistryCredential
+DEC-18: a credential&#39;s newest revision (the secret is the macOS keychain&#39;s alone).
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| id | [string](#string) |  | Credential id. |
+| revision | [uint64](#uint64) |  | Its newest revision (a rotation is a new one, never reused). |
+| state | [string](#string) |  | active | revoked. |
+| origin | [string](#string) |  | input | env:NAME (the one-time .env import). |
+| created_unix_s | [uint64](#uint64) |  | When the revision was created. |
+
+
+
+
+
+
+<a name="darkbot-project0-v1-RegistryProvider"></a>
+
+### RegistryProvider
+DEC-18: one provider of the registry.
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| id | [string](#string) |  | Provider id ([a-z0-9_-]{1,32}). |
+| kind | [string](#string) |  | rpc | wss | geyser | jupiter | jito. |
+| host | [string](#string) |  | The endpoint&#39;s host (its path and scheme stay in the engine). |
+| auth | [string](#string) |  | How it authenticates: none | query:NAME | header:NAME | path (a name, never a value). |
+| credential | [string](#string) |  | The credential it authenticates with (its id); empty without authentication. |
+| priority | [uint32](#uint32) |  | Lower first among the providers of its kind. |
+| capacity_rps | [uint64](#uint64) | optional | Configured capacity in requests per second; unset when not configured. |
+| plan | [string](#string) |  | free | unknown (FREE_ONLY: paid is refused, unknown never selected). |
+| entitlement | [string](#string) |  | The plan&#39;s name as the provider publishes it. |
+| qualification | [string](#string) | optional | The provider evidence record that qualified its binding; unset when not qualified. |
+| enabled | [bool](#bool) |  | Enabled (a disabled provider is never selected). |
+| selected | [bool](#bool) |  | The selected provider of its kind. |
+| binding | [string](#string) | optional | Its secret-free binding (hex SHA-256) with the active credential revision; unset when its credential is revoked or unknown. |
+| selection_block | [string](#string) | optional | Why it cannot be the selected one of its kind now; unset when it can. |
+
+
+
+
+
+
 <a name="darkbot-project0-v1-RunQualification"></a>
 
 ### RunQualification
@@ -3844,6 +4013,7 @@ Secret-free effective operational record (runtime_config::EffectiveConfig).
 | c0_census | [bool](#bool) |  | C0 census on the startup sweep (P0_C0_CENSUS). |
 | source_geyser | [string](#string) |  | Start mode of the Geyser source unit: off | observe | active. |
 | source_geyser_provider | [string](#string) |  | Provider of the Geyser source unit. |
+| providers | [RunProvider](#darkbot-project0-v1-RunProvider) | repeated | DEC-18: the providers the run is bound to (runtime_config::ProviderBinding), secret-free and without their endpoints; empty in a record of an earlier revision. |
 
 
 
@@ -3906,6 +4076,27 @@ One numeric operational parameter of runtime_config::PARAMETERS.
 
 
 
+<a name="darkbot-project0-v1-RunProvider"></a>
+
+### RunProvider
+DEC-18: one provider a run is bound to.
+
+
+| Field | Type | Label | Description |
+| ----- | ---- | ----- | ----------- |
+| kind | [string](#string) |  | rpc | wss | geyser | jupiter | jito. |
+| id | [string](#string) |  | Provider id (local: a named loopback endpoint; jupiter-default: the contract&#39;s profile). |
+| binding | [string](#string) |  | Its secret-free binding (hex SHA-256). |
+| registry_version | [uint64](#uint64) |  | The registry version it was bound from; 0 for a local endpoint or a default profile. |
+| credential_id | [string](#string) |  | The credential id it authenticates with; empty without. |
+| credential_revision | [uint64](#uint64) |  | The credential revision bound. |
+| plan | [string](#string) |  | free | paid | unknown. |
+
+
+
+
+
+
 <a name="darkbot-project0-v1-SystemStatus"></a>
 
 ### SystemStatus
diff --git a/frontend/contracts/bsc-fields.json b/frontend/contracts/bsc-fields.json
index ec7d974653b1081283a3f6ebe9ca5b5d64ad3a18..55aba4f22e657048a4031220744b6fd2136193df 100644
--- a/frontend/contracts/bsc-fields.json
+++ b/frontend/contracts/bsc-fields.json
@@ -6217,7 +6217,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6241,7 +6241,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6265,7 +6265,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6289,7 +6289,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6313,7 +6313,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6337,7 +6337,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6361,7 +6361,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6385,7 +6385,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6409,7 +6409,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6433,7 +6433,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6457,7 +6457,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6601,7 +6601,7 @@
       "sensitivity": "public",
       "provenance": "operator request with the confirmed scope SHA-256; plan 28.10.",
       "quality": "OPERATOR_COMMAND",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "required_gate": "NATIVE_LIVE_GRANT_CONFIRMED_SCOPE",
       "topic_or_command": "bsc.live.grant",
       "test": "control::tests the_live_grant_revoke_and_total_limit_reach_the_native_owner + live.test.ts",
@@ -6661,7 +6661,7 @@
     },
     {
       "source_path": "gateway/src/control.rs",
-      "source_sha": "aae3929338471ccb9a267972300f4d9f37e8f681f6a02ed3a470a82757d59f39",
+      "source_sha": "cfa21782d797a38aea9e3106810f699d3a319ac0db4bbaa8de00efe669b29fa0",
       "owner": "Gateway / ops",
       "gate": "READ_ONLY; no signing; launch UUID CAS; durable receipt; session/CSRF"
     }
diff --git a/frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md b/frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md
index e9d9ae02b62eae8af56570fb128fc01075b9918b..de6a73e7792caa8e026f7c40f86f7412e76e6544 100644
--- a/frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md
+++ b/frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md
@@ -37,6 +37,7 @@ GLOBAL /: overview.aggregates STATE allein; /project-0 Overview: project0.status
 
 ## 7. Commands / Wire-Security
 Mutationen als separate same-origin HTTP POST Protobuf über Gateway-Proxy; kein command per subscription. idempotency key/request_id, project/action/resource, native domain expected_config_version/profile_version/plan_version aus STATE – niemals WS seq als CAS. Gateway HTTP auth/Origin/CSRF/Host/IPC transport, ENGINE allein Wert-/Chain-/Risk-Gates und persistente Receipt-Autorität. Ack ACCEPTED heißt nicht executed. pending_commands[] + bounded recent_command_results[] als immer abonnierter Domain-STATE, verlorenes Ack über STATE oder einmaliges gezieltes GET nach ID. Key+gleiche Payload ein Intent, Key+andere Payload REJECT; zeitlich begrenzte Idempotenz aus der Gateway-Konfiguration (`command_clock_window_ms`, `command_retention_ms`, `command_ledger_records`; Defaults: Fenster = Heartbeat-Intervall × verpasste Heartbeats, Aufbewahrung = 2 × Fenster, 1024 Einträge; per `DARKBOT_GATEWAY_COMMAND_*` überschreibbar, eine Aufbewahrung unter 2 × Fenster wird beim Start abgewiesen): (1) Annahme nur mit |`issued_at_unix_ns` − Empfangszeit| ≤ Fenster (Uhrabweichung + Zustelldauer), sonst INVALID_PARAMS ohne Ledger-Wirkung – unabhängig von Archiv und Verkehr anderer Clients, nach Uhrkorrektur sofort wieder möglich; (2) solange der Key bekannt ist – mindestens die Aufbewahrung nach der letzten Aktualisierung seines Receipts, darüber hinaus bis zu 2 × Einträge neuere Receipts –: gleiche Bytes ⇒ Original-Receipt, andere Bytes (auch nur eine neue Zeit) ⇒ IDEMPOTENCY_CONFLICT; (3) Vergessen nur bei vollem Archiv, nur Receipts, deren letzte Aktualisierung mindestens die Aufbewahrung zurückliegt, das am frühesten ausgestellte zuerst und erst nach dauerhafter Marke `retired_through`; ist keines so alt, werden neue Commands abgelehnt statt eines vorzeitig vergessen; offene Intents nie; ein Ledger nur aus offenen Intents lehnt neue Commands ab; (4) Replay-Regel: eine vergessene Original-Payload liegt stets außerhalb des Fensters und unter der Marke und wird nie erneut ausgeführt; (5) Grenze: derselbe Key mit neuer Zeit nach dem Vergessen ist eine andere Payload und gilt als neuer Intent (der Client verwendet Keys nie wieder); keine automatischen Echtgeld-Retries. SPA same-origin, HttpOnly SameSite Strict (+ Secure bei TLS), Origin-/Host allowlist für WS und POST, CSP connect-src self, authentisierter Engine-hop. Niemals Secret/Signer ans Browser.
+DEC-18 (P0-019/P0-020) Credential-Eingang: `project0.provider.credential.set|rotate` tragen den Schlüssel nur in `params` (`ProviderCredentialCommand.secret`); das Gateway trennt ihn vor jedem dauerhaften Record ab (der Ledger hält den Request ohne Schlüssel, Replay vergleicht genau diesen) und übergibt ihn ausschließlich der Standardeingabe des nativen Controllers (`darkbotctl p0 provider credential …` → `p0ctl`), nie als Argument, Umgebung, Log, Receipt oder Browser-Storage; seine Kopie wird beim Freigeben überschrieben. Ein Endpunkt mit Benutzerinfo/Query/Fragment/tokenartigem Pfadsegment oder ein Name in Schlüsselform wird vor dem Ledger abgewiesen; Gültigkeit entscheidet die native Registry (CAS über `config_version` = Registry-Version, Receipt `PROVIDER_RECEIPT`).
 
 ## 8. Exakte Numerik
 u64 (slots/lamports/in-range nonnegative USD-micro) → TS bigint; Wei/u128/u256 als kanonischer decimal string, negative oder >u64 USD-micro/realized PnL als signierter Decimalstring. Einheiten pro Feld explizit und Golden-Test. Kein JS-Float bei Geld; keine BigInt-Objekte in JSON.stringify Query-Keys.
diff --git a/frontend/src/app/projectRegistry.ts b/frontend/src/app/projectRegistry.ts
index 577089431b0904625a2f24bcf4cfd394ab871d9d..c627495da4200b1030f27e4fedde10324a753e9b 100644
--- a/frontend/src/app/projectRegistry.ts
+++ b/frontend/src/app/projectRegistry.ts
@@ -53,7 +53,8 @@ const project0: ProjectRegistration<Project0Panel> = {
     // Every topic a tab renders (Project0Page holds them while the tab is visible).
     panels: {
       overview: [TopicId.PROJECT0_FINANCE_SUMMARY, TopicId.PROJECT0_LIQUIDATOR_STATE, TopicId.PROJECT0_ANALYTICS_JOBS],
-      liquidator: [TopicId.PROJECT0_LIQUIDATOR_STATE, TopicId.PROJECT0_FINANCE_SUMMARY],
+      // DEC-18: the Jupiter/Jito providers of the registry (project0.analytics.jobs).
+      liquidator: [TopicId.PROJECT0_LIQUIDATOR_STATE, TopicId.PROJECT0_FINANCE_SUMMARY, TopicId.PROJECT0_ANALYTICS_JOBS],
       capture: [TopicId.PROJECT0_CAPTURE_STATE, TopicId.PROJECT0_ANALYTICS_JOBS, TopicId.PROJECT0_LIQUIDATOR_STATE],
       analytics: [TopicId.PROJECT0_ANALYTICS_JOBS],
       wallets: [TopicId.PROJECT0_WALLETS_STATE, TopicId.PROJECT0_FINANCE_SUMMARY],
diff --git a/frontend/src/core/generated/darkbot/project0/v1/analytics_pb.ts b/frontend/src/core/generated/darkbot/project0/v1/analytics_pb.ts
index 8b0a80cc9ca068728090c02d5fd1a0347c5aa36b..600244a9f8ba96859dd2fc15dedb34db4a6d1568 100644
--- a/frontend/src/core/generated/darkbot/project0/v1/analytics_pb.ts
+++ b/frontend/src/core/generated/darkbot/project0/v1/analytics_pb.ts
@@ -12,7 +12,7 @@ import type { Message } from "@bufbuild/protobuf";
  * Describes the file darkbot/project0/v1/analytics.proto.
  */
 export const file_darkbot_project0_v1_analytics: GenFile = /*@__PURE__*/
-  fileDesc("CiNkYXJrYm90L3Byb2plY3QwL3YxL2FuYWx5dGljcy5wcm90bxITZGFya2JvdC5wcm9qZWN0MC52MSLHAgoNQW5hbHl0aWNzSm9icxIZChFwdWJsaXNoZWRfdW5peF9tcxgBIAEoBBITCgtydW5uZXJfYnVzeRgCIAEoCBIvCgRydW5zGAMgAygLMiEuZGFya2JvdC5wcm9qZWN0MC52MS5BbmFseXRpY3NSdW4SLwoEam9icxgEIAMoCzIhLmRhcmtib3QucHJvamVjdDAudjEuQW5hbHl0aWNzSm9iEhMKC3N0b3JlX2Vycm9yGAUgASgJEhQKDHJ1bm5lcl9lcnJvchgGIAEoCRIrCgRib290GAcgASgLMh0uZGFya2JvdC5wcm9qZWN0MC52MS5Cb290RmxvdxISCgpib290X2Vycm9yGAggASgJEjgKDXVudGlsX3N0b3BwZWQYCSABKAsyIS5kYXJrYm90LnByb2plY3QwLnYxLlVudGlsU3RvcHBlZCKGAgoMVW50aWxTdG9wcGVkEjYKCWF1dG9zdGFydBgBIAEoCzIjLmRhcmtib3QucHJvamVjdDAudjEuQXV0b3N0YXJ0R3JhbnQSNAoHcHJldmlldxgCIAEoCzIjLmRhcmtib3QucHJvamVjdDAudjEuQXV0b3N0YXJ0R3JhbnQSRQoNcXVhbGlmaWNhdGlvbhgDIAEoCzIuLmRhcmtib3QucHJvamVjdDAudjEuVW50aWxTdG9wcGVkUXVhbGlmaWNhdGlvbhIxCgNydW4YBCABKAsyJC5kYXJrYm90LnByb2plY3QwLnYxLlVudGlsU3RvcHBlZFJ1bhIOCgZlcnJvcnMYBSADKAkinAMKDkF1dG9zdGFydEdyYW50Eg4KBmRpZ2VzdBgBIAEoCRIMCgRtb2RlGAIgASgJEhUKDWNvbmZpZ19zaGEyNTYYAyABKAkSFQoNYmluYXJ5X3NoYTI1NhgEIAEoCRIQCghkdXJhdGlvbhgFIAEoCRIaChJjaGVja3BvaW50X3NlY29uZHMYBiABKAQSHgoWbWluaW11bV9ydW53YXlfc2Vjb25kcxgHIAEoBBIYChBxdWFsaWZpY2F0aW9uX2lkGAggASgJEhwKFHByb3ZpZGVyX2V2aWRlbmNlX2lkGAkgASgJEhQKDGdlbmVzaXNfaGFzaBgKIAEoCRIWCg5jYXB0dXJlX3NvdXJjZRgLIAEoCRIPCgdycGNfcnBzGAwgASgEEhQKDG1heF9yZXF1ZXN0cxgNIAEoBBIVCg1tYXhfdG90YWxfbWliGA4gASgEEhsKE21heF9ydW50aW1lX3NlY29uZHMYDyABKAQSFgoOZXhwaXJlc191bml4X3MYECABKAQSFwoPcmVjb3JkZWRfdW5peF9zGBEgASgEIqICChlVbnRpbFN0b3BwZWRRdWFsaWZpY2F0aW9uEgoKAmlkGAEgASgJEgwKBG1vZGUYAiABKAkSGgoSY2hlY2twb2ludF9zZWNvbmRzGAMgASgEEh4KFm1pbmltdW1fcnVud2F5X3NlY29uZHMYBCABKAQSEAoIY29tcGxldGUYBSABKAgSDAoEb3BlbhgGIAMoCRIYCgtldmlkZW5jZV9pZBgHIAEoCUgAiAEBEigKG2V2aWRlbmNlX3ZhbGlkX3VudGlsX3VuaXhfcxgIIAEoBEgBiAEBEhsKE3F1YWxpZmllZF9hdF91bml4X3MYCSABKARCDgoMX2V2aWRlbmNlX2lkQh4KHF9ldmlkZW5jZV92YWxpZF91bnRpbF91bml4X3MiuQIKD1VudGlsU3RvcHBlZFJ1bhIOCgZydW5faWQYASABKAkSGAoQcXVhbGlmaWNhdGlvbl9pZBgCIAEoCRIWCg5zdGFydGVkX3VuaXhfcxgDIAEoBBIaChJjaGVja3BvaW50X3NlY29uZHMYBCABKAQSDwoHd2luZG93cxgFIAEoBBIZCgxsYXN0X3ZlcmRpY3QYBiABKAlIAIgBARITCgtsYXN0X2RldGFpbBgHIAEoCRIeChFsYXN0X2VuZGVkX3VuaXhfcxgIIAEoBEgBiAEBEhMKBmhhbHRlZBgJIAEoCUgCiAEBEg4KBnNlYWxlZBgKIAEoCBIQCghjb21wbGV0ZRgLIAEoCEIPCg1fbGFzdF92ZXJkaWN0QhQKEl9sYXN0X2VuZGVkX3VuaXhfc0IJCgdfaGFsdGVkIpgECghCb290RmxvdxIPCgdmbG93X2lkGAEgASgJEgwKBG1vZGUYAiABKAkSHAoPY3JlYXRlZF91bml4X21zGAMgASgESACIAQESFwoPb2ZmbGluZV92ZXJkaWN0GAQgASgJEhUKDW9mZmxpbmVfdG9rZW4YBSABKAkSFwoPb2ZmbGluZV9ibG9ja2VkGAYgAygJEhMKC3R0bF9zZWNvbmRzGAcgASgEEhIKCnR0bF9zb3VyY2UYCCABKAkSLgoHbmV0d29yaxgJIAEoCzIdLmRhcmtib3QucHJvamVjdDAudjEuQm9vdFN0ZXASHAoPbmV0d29ya192ZXJkaWN0GAogASgJSAGIAQESGgoNbmV0d29ya190b2tlbhgLIAEoCUgCiAEBEhcKD25ldHdvcmtfYmxvY2tlZBgMIAMoCRIcCg9vYnNlcnZlZF91bml4X3MYDSABKARIA4gBARIbCg5leHBpcmVzX3VuaXhfcxgOIAEoBEgEiAEBEiwKBXN0YXJ0GA8gASgLMh0uZGFya2JvdC5wcm9qZWN0MC52MS5Cb290U3RlcBIQCghkdXJhdGlvbhgQIAEoCUISChBfY3JlYXRlZF91bml4X21zQhIKEF9uZXR3b3JrX3ZlcmRpY3RCEAoOX25ldHdvcmtfdG9rZW5CEgoQX29ic2VydmVkX3VuaXhfc0IRCg9fZXhwaXJlc191bml4X3MirQEKCEJvb3RTdGVwEg0KBXN0YXRlGAEgASgJEgwKBGNvZGUYAiABKAkSEwoGcnVuX2lkGAMgASgJSACIAQESHAoPc3RhcnRlZF91bml4X21zGAQgASgESAGIAQESHQoQZmluaXNoZWRfdW5peF9tcxgFIAEoBEgCiAEBQgkKB19ydW5faWRCEgoQX3N0YXJ0ZWRfdW5peF9tc0ITChFfZmluaXNoZWRfdW5peF9tcyKYAgoMQW5hbHl0aWNzUnVuEg4KBnJ1bl9pZBgBIAEoCRIMCgRyb290GAIgASgJEg4KBmxheW91dBgDIAEoCRIYChBjYXB0dXJlX2NvbXBsZXRlGAQgASgIEjwKDXF1YWxpZmljYXRpb24YBSABKAsyJS5kYXJrYm90LnByb2plY3QwLnYxLlJ1blF1YWxpZmljYXRpb24SHQoQbW9kaWZpZWRfdW5peF9tcxgGIAEoBEgAiAEBEg8KB3J1bm5pbmcYByABKAgSPQoMdmVyaWZpY2F0aW9uGAggAygLMicuZGFya2JvdC5wcm9qZWN0MC52MS5WZXJpZmljYXRpb25NZXRob2RCEwoRX21vZGlmaWVkX3VuaXhfbXMi+AYKElZlcmlmaWNhdGlvbk1ldGhvZBIOCgZtZXRob2QYASABKAkSDwoHc3BlY19pZBgCIAEoCRIQCghhdHRlbXB0cxgDIAEoBBIPCgdydW5uaW5nGAQgASgIEg4KBnN0YXR1cxgFIAEoCRITCgZyZWFzb24YBiABKAlIAIgBARIWCglyZXN1bHRfaWQYByABKAlIAYgBARIPCgdvZmZsaW5lGAggASgIEh0KEHByb3ZpZGVyX2JpbmRpbmcYCSABKAlIAogBARIYChBjaGVja3BvaW50X3JlYWRzGAogASgEEhIKBWNhbGxzGAsgASgESAOIAQESEgoFYnl0ZXMYDCABKARIBIgBARIVCg1wcm9ncmVzc191bml0GA0gASgJEhUKDXByb2dyZXNzX2RvbmUYDiABKAQSGwoOcHJvZ3Jlc3NfdG90YWwYDyABKARIBYgBARIyCglxdWFsaWZpZWQYECADKAsyHy5kYXJrYm90LnByb2plY3QwLnYxLk5hbWVkQ291bnQSMQoIb2JzZXJ2ZWQYESADKAsyHy5kYXJrYm90LnByb2plY3QwLnYxLk5hbWVkQ291bnQSMAoHbWFya2VycxgSIAMoCzIfLmRhcmtib3QucHJvamVjdDAudjEuTmFtZWRDb3VudBIzCgpjb21wYXJpc29uGBMgAygLMh8uZGFya2JvdC5wcm9qZWN0MC52MS5OYW1lZENvdW50EhoKDWRpc2FncmVlbWVudHMYFCABKARIBogBARIXCgp1bnJlc29sdmVkGBUgASgESAeIAQESFAoHdW5rbm93bhgWIAEoBEgIiAEBEjUKDHRyYW5zYWN0aW9ucxgXIAMoCzIfLmRhcmtib3QucHJvamVjdDAudjEuTmFtZWRDb3VudBIYCgtjYWxsc190b3RhbBgYIAEoBEgJiAEBEhgKC2J5dGVzX3RvdGFsGBkgASgESAqIAQFCCQoHX3JlYXNvbkIMCgpfcmVzdWx0X2lkQhMKEV9wcm92aWRlcl9iaW5kaW5nQggKBl9jYWxsc0IICgZfYnl0ZXNCEQoPX3Byb2dyZXNzX3RvdGFsQhAKDl9kaXNhZ3JlZW1lbnRzQg0KC191bnJlc29sdmVkQgoKCF91bmtub3duQg4KDF9jYWxsc190b3RhbEIOCgxfYnl0ZXNfdG90YWwiKQoKTmFtZWRDb3VudBIMCgRuYW1lGAEgASgJEg0KBWNvdW50GAIgASgEInIKEFJ1blF1YWxpZmljYXRpb24SDwoHdmVyZGljdBgBIAEoCRIUCgx2ZXJpZmljYXRpb24YAiABKAkSDgoGZmFpbGVkGAMgAygJEhgKEHF1YWxpZmljYXRpb25faWQYBCABKAkSDQoFZXJyb3IYBSABKAki7wUKDEFuYWx5dGljc0pvYhIOCgZqb2JfaWQYASABKAkSDAoEa2luZBgCIAEoCRILCgNydW4YAyABKAkSDQoFc3RhdGUYBCABKAkSDAoEY29kZRgFIAEoCRIeChFyZXF1ZXN0ZWRfdW5peF9tcxgGIAEoBEgBiAEBEhwKD3N0YXJ0ZWRfdW5peF9tcxgHIAEoBEgCiAEBEh0KEGZpbmlzaGVkX3VuaXhfbXMYCCABKARIA4gBARITCgttYXhfc2Vjb25kcxgJIAEoBBIVCg1tYXhfcmFtX2J5dGVzGAogASgEEi4KBnBhcmFtcxgLIAEoCzIeLmRhcmtib3QucHJvamVjdDAudjEuSm9iUGFyYW1zEioKBGdyaWQYDCABKAsyHC5kYXJrYm90LnByb2plY3QwLnYxLkpvYkdyaWQSGwoTdmVyaWZpY2F0aW9uX3Jlc3VsdBgNIAEoCRI3CghiYWNrdGVzdBgOIAEoCzIjLmRhcmtib3QucHJvamVjdDAudjEuQmFja3Rlc3RSZXN1bHRIABI3CghvcHRpbWl6ZRgPIAEoCzIjLmRhcmtib3QucHJvamVjdDAudjEuT3B0aW1pemVSZXN1bHRIABI+Cgxwb3N0X2NhcHR1cmUYESABKAsyJi5kYXJrYm90LnByb2plY3QwLnYxLlBvc3RDYXB0dXJlUmVzdWx0SAASKwoCaDAYEiABKAsyHS5kYXJrYm90LnByb2plY3QwLnYxLkgwUmVzdWx0SAASPwoRcG9zdF9jYXB0dXJlX3NwZWMYECABKAsyJC5kYXJrYm90LnByb2plY3QwLnYxLlBvc3RDYXB0dXJlU3BlYxIsCgdoMF9zcGVjGBMgASgLMhsuZGFya2JvdC5wcm9qZWN0MC52MS5IMFNwZWNCCAoGcmVzdWx0QhQKEl9yZXF1ZXN0ZWRfdW5peF9tc0ISChBfc3RhcnRlZF91bml4X21zQhMKEV9maW5pc2hlZF91bml4X21zIlkKBkgwU3BlYxITCgt3aW5kb3dfZGF5cxgBIAEoBBIXCgptaW5fc2FtcGxlGAIgASgESACIAQESEgoKZW5kX3VuaXhfcxgDIAEoA0INCgtfbWluX3NhbXBsZSKQBQoISDBSZXN1bHQSEQoJcmVzdWx0X2lkGAEgASgJEhsKE3dpbmRvd19zdGFydF91bml4X3MYAiABKAMSGQoRd2luZG93X2VuZF91bml4X3MYAyABKAMSFAoMbGlxdWlkYXRpb25zGAQgASgEEhIKCmR1cGxpY2F0ZXMYBSABKAQSFQoNcnVuc192ZXJpZmllZBgGIAEoBBIbChNydW5zX3dpdGhvdXRfcmVzdWx0GAcgASgEEg8KB3dpbm5lcnMYCCABKAQSIQoUdG9wX3dpbm5lcl9zaGFyZV9icHMYCSABKARIAIgBARINCgViYW5kcxgKIAEoBBIVCg1zYW1wbGVfc3RhdHVzGAsgASgJEgwKBHRhZ3MYDCABKAkSGAoLY29tcGV0aXRpdmUYDSABKARIAYgBARIZCgx0cmFuc2FjdGlvbnMYDiABKARIAogBARIcCg9mYWlsZWRfYXR0ZW1wdHMYDyABKARIA4gBARIcCg9jb3ZlcmVkX3NlY29uZHMYECABKARIBIgBARIaCg1iYW5kc19ib3VuZGVkGBEgASgESAWIAQESJwoaYmFuZHNfYmVsb3dfb3VyX21pbl9wcm9maXQYEiABKARIBogBARIZCgx0aXBfYWNjb3VudHMYEyABKARIB4gBAUIXChVfdG9wX3dpbm5lcl9zaGFyZV9icHNCDgoMX2NvbXBldGl0aXZlQg8KDV90cmFuc2FjdGlvbnNCEgoQX2ZhaWxlZF9hdHRlbXB0c0ISChBfY292ZXJlZF9zZWNvbmRzQhAKDl9iYW5kc19ib3VuZGVkQh0KG19iYW5kc19iZWxvd19vdXJfbWluX3Byb2ZpdEIPCg1fdGlwX2FjY291bnRzImMKD1Bvc3RDYXB0dXJlU3BlYxIUCgxjaGFpbl92ZXJpZnkYASABKAkSDwoHb2ZmbGluZRgCIAEoCBIOCgZyZXN1bWUYAyABKAgSGQoRcHJvdmlkZXJfZXZpZGVuY2UYBCABKAgiUAoRUG9zdENhcHR1cmVSZXN1bHQSFwoPdmVyaWZpY2F0aW9uX29rGAEgASgIEhEKCXJlcG9ydF9vaxgCIAEoCBIPCgd2ZXJkaWN0GAMgASgJIn8KCUpvYlBhcmFtcxJGChttaW5fcHJvZml0X3VzZF9taWNyb19zaWduZWQYASABKAsyIS5kYXJrYm90LmNvbnRyb2wudjEuU2lnbmVkRGVjaW1hbBIZChFjYXBpdGFsX3VzZF9taWNybxgCIAMoBBIPCgdmdW5kaW5nGAMgAygJIscBCgdKb2JHcmlkEkYKG21pbl9wcm9maXRfdXNkX21pY3JvX3NpZ25lZBgBIAMoCzIhLmRhcmtib3QuY29udHJvbC52MS5TaWduZWREZWNpbWFsEjUKDGZ1bmRpbmdfc2V0cxgCIAMoCzIfLmRhcmtib3QucHJvamVjdDAudjEuRnVuZGluZ1NldBITCgtob2xkb3V0X2JwcxgDIAEoBBISCgptYXhfcG9pbnRzGAQgASgEEhQKDHNhdmVfcHJvZmlsZRgFIAEoCCIdCgpGdW5kaW5nU2V0Eg8KB2NsYXNzZXMYASADKAki3QEKDkJhY2t0ZXN0UmVzdWx0EhEKCXJlc3VsdF9pZBgBIAEoCRIWCg5jYXB0dXJlX3J1bl9pZBgCIAEoCRIMCgRyb3dzGAMgASgEEhAKCGF0dGVtcHRzGAQgASgEEhQKDGxpcXVpZGF0aW9ucxgFIAEoBBJBCg5jb3VudGVyZmFjdHVhbBgGIAEoCzIpLmRhcmtib3QucHJvamVjdDAudjEuU2hhZG93Q291bnRlcmZhY3R1YWwSEAoIcmVhbGl6ZWQYByABKAkSFQoNcmVzdWx0X3NoYTI1NhgIIAEoDCL9AQoUU2hhZG93Q291bnRlcmZhY3R1YWwSDwoHbWF0Y2hlZBgBIAEoBBIPCgd1bmtub3duGAIgASgEEhgKEGZ1bmRpbmdfZXhjbHVkZWQYAyABKAQSFQoNYmVsb3dfbWluaW11bRgEIAEoBBIMCgR3aW5zGAUgASgEEhIKCmxvc3RfcmFjZXMYBiABKAQSFgoObm9fY2hhaW5fZXZlbnQYByABKAQSNwoJc2NlbmFyaW9zGAggAygLMiQuZGFya2JvdC5wcm9qZWN0MC52MS5DYXBpdGFsU2NlbmFyaW8SDwoHbGFuZGluZxgJIAEoCRIOCgZzYWZldHkYCiABKAki0wIKD0NhcGl0YWxTY2VuYXJpbxIZChFjYXBpdGFsX3VzZF9taWNybxgBIAEoBBIMCgR3aW5zGAIgASgEEkMKGHdpbl9uZXRfdXNkX21pY3JvX3NpZ25lZBgDIAEoCzIhLmRhcmtib3QuY29udHJvbC52MS5TaWduZWREZWNpbWFsEhcKD2ZhaWxlZF9hdHRlbXB0cxgEIAEoBBJGChlmYWlsZWRfY29zdF91c2RfbWljcm9fc3VtGAUgASgLMiMuZGFya2JvdC5jb250cm9sLnYxLlVuc2lnbmVkRGVjaW1hbBIbChNmYWlsZWRfY29zdF91bmtub3duGAYgASgEEhMKC292ZXJfYnVkZ2V0GAcgASgEEj8KFG5ldF91c2RfbWljcm9fc2lnbmVkGAggASgLMiEuZGFya2JvdC5jb250cm9sLnYxLlNpZ25lZERlY2ltYWwinQIKDk9wdGltaXplUmVzdWx0EhEKCXJlc3VsdF9pZBgBIAEoCRIWCg5jYXB0dXJlX3J1bl9pZBgCIAEoCRIOCgZwb2ludHMYAyABKAQSLgoGY2hvc2VuGAQgASgLMh4uZGFya2JvdC5wcm9qZWN0MC52MS5Kb2JQYXJhbXMSOgoOY2hvc2VuX2hvbGRvdXQYBSABKAsyIi5kYXJrYm90LnByb2plY3QwLnYxLk9wdGltaXplU2NvcmUSPAoQYmFzZWxpbmVfaG9sZG91dBgGIAEoCzIiLmRhcmtib3QucHJvamVjdDAudjEuT3B0aW1pemVTY29yZRIVCg1yZXN1bHRfc2hhMjU2GAcgASgMEg8KB3Byb2ZpbGUYCCABKAkikwEKDU9wdGltaXplU2NvcmUSQQoOY291bnRlcmZhY3R1YWwYASABKAsyKS5kYXJrYm90LnByb2plY3QwLnYxLlNoYWRvd0NvdW50ZXJmYWN0dWFsEj8KFG9iamVjdGl2ZV9icHNfc2lnbmVkGAIgASgLMiEuZGFya2JvdC5jb250cm9sLnYxLlNpZ25lZERlY2ltYWwiqwEKEkJhY2t0ZXN0Sm9iQ29tbWFuZBIOCgZydW5faWQYASABKAkSFgoObWluX3Byb2ZpdF91c2QYAiABKAkSEwoLY2FwaXRhbF91c2QYAyADKAQSDwoHZnVuZGluZxgEIAMoCRITCgttYXhfc2Vjb25kcxgFIAEoBBIVCg1tYXhfcmFtX2J5dGVzGAYgASgEEhsKE3ZlcmlmaWNhdGlvbl9yZXN1bHQYByABKAkiTwoMSDBKb2JDb21tYW5kEhMKC3dpbmRvd19kYXlzGAEgASgEEhMKC21heF9zZWNvbmRzGAIgASgEEhUKDW1heF9yYW1fYnl0ZXMYAyABKAQi4gEKEk9wdGltaXplSm9iQ29tbWFuZBI5CghiYXNlbGluZRgBIAEoCzInLmRhcmtib3QucHJvamVjdDAudjEuQmFja3Rlc3RKb2JDb21tYW5kEhsKE21pbl9wcm9maXRfdXNkX2dyaWQYAiADKAkSNQoMZnVuZGluZ19zZXRzGAMgAygLMh8uZGFya2JvdC5wcm9qZWN0MC52MS5GdW5kaW5nU2V0EhMKC2hvbGRvdXRfYnBzGAQgASgEEhIKCm1heF9wb2ludHMYBSABKAQSFAoMc2F2ZV9wcm9maWxlGAYgASgIIl4KEkJvb3RPZmZsaW5lQ29tbWFuZBIMCgRtb2RlGAEgASgJEhgKC3R0bF9zZWNvbmRzGAIgASgESACIAQESEAoIZHVyYXRpb24YAyABKAlCDgoMX3R0bF9zZWNvbmRzIoYBChJCb290TmV0d29ya0NvbW1hbmQSDwoHZmxvd19pZBgBIAEoCRIWCg5hcHByb3ZlZF90b2tlbhgCIAEoCRIUCgxnZW5lc2lzX2hhc2gYAyABKAkSGQoRcHJvdmlkZXJfZXZpZGVuY2UYBCABKAkSFgoOY2FwdHVyZV9zb3VyY2UYBSABKAkiqQEKEEJvb3RTdGFydENvbW1hbmQSDwoHZmxvd19pZBgBIAEoCRIWCg5hcHByb3ZlZF90b2tlbhgCIAEoCRIQCghkdXJhdGlvbhgDIAEoCRIWCg5jYXB0dXJlX3NvdXJjZRgEIAEoCRIUCgxjaGFpbl92ZXJpZnkYBSABKAkSGgoNcXVhbGlmaWNhdGlvbhgGIAEoCUgAiAEBQhAKDl9xdWFsaWZpY2F0aW9uIrIBChdBdXRvc3RhcnRQcmV2aWV3Q29tbWFuZBIVCg1xdWFsaWZpY2F0aW9uGAEgASgJEhIKCmNoZWNrcG9pbnQYAiABKAkSFgoObWluaW11bV9ydW53YXkYAyABKAkSFAoMZ2VuZXNpc19oYXNoGAQgASgJEhMKC21heF9ydW50aW1lGAUgASgJEhEKCXZhbGlkX2ZvchgGIAEoCRIWCg5jYXB0dXJlX3NvdXJjZRgHIAEoCSIvChVBdXRvc3RhcnRHcmFudENvbW1hbmQSFgoOY29uZmlybV9kaWdlc3QYASABKAkilwEKFVBvc3RDYXB0dXJlSm9iQ29tbWFuZBIOCgZydW5faWQYASABKAkSFAoMY2hhaW5fdmVyaWZ5GAIgASgJEg8KB29mZmxpbmUYAyABKAgSGwoTcmVzdW1lX3ZlcmlmaWNhdGlvbhgEIAEoCBITCgttYXhfc2Vjb25kcxgFIAEoBBIVCg1tYXhfcmFtX2J5dGVzGAYgASgEQpcBChdjb20uZGFya2JvdC5wcm9qZWN0MC52MUIOQW5hbHl0aWNzUHJvdG9QAaICA0RQWKoCE0Rhcmtib3QuUHJvamVjdDAuVjHKAhNEYXJrYm90XFByb2plY3QwXFYx4gIfRGFya2JvdFxQcm9qZWN0MFxWMVxHUEJNZXRhZGF0YeoCFURhcmtib3Q6OlByb2plY3QwOjpWMWIGcHJvdG8z", [file_darkbot_control_v1_numeric]);
+  fileDesc("CiNkYXJrYm90L3Byb2plY3QwL3YxL2FuYWx5dGljcy5wcm90bxITZGFya2JvdC5wcm9qZWN0MC52MSKeAwoNQW5hbHl0aWNzSm9icxIZChFwdWJsaXNoZWRfdW5peF9tcxgBIAEoBBITCgtydW5uZXJfYnVzeRgCIAEoCBIvCgRydW5zGAMgAygLMiEuZGFya2JvdC5wcm9qZWN0MC52MS5BbmFseXRpY3NSdW4SLwoEam9icxgEIAMoCzIhLmRhcmtib3QucHJvamVjdDAudjEuQW5hbHl0aWNzSm9iEhMKC3N0b3JlX2Vycm9yGAUgASgJEhQKDHJ1bm5lcl9lcnJvchgGIAEoCRIrCgRib290GAcgASgLMh0uZGFya2JvdC5wcm9qZWN0MC52MS5Cb290RmxvdxISCgpib290X2Vycm9yGAggASgJEjgKDXVudGlsX3N0b3BwZWQYCSABKAsyIS5kYXJrYm90LnByb2plY3QwLnYxLlVudGlsU3RvcHBlZBI8Cglwcm92aWRlcnMYCiABKAsyKS5kYXJrYm90LnByb2plY3QwLnYxLlByb3ZpZGVyUmVnaXN0cnlWaWV3EhcKD3Byb3ZpZGVyc19lcnJvchgLIAEoCSK1AQoUUHJvdmlkZXJSZWdpc3RyeVZpZXcSDwoHdmVyc2lvbhgBIAEoBBI4Cglwcm92aWRlcnMYAiADKAsyJS5kYXJrYm90LnByb2plY3QwLnYxLlJlZ2lzdHJ5UHJvdmlkZXISPAoLY3JlZGVudGlhbHMYAyADKAsyJy5kYXJrYm90LnByb2plY3QwLnYxLlJlZ2lzdHJ5Q3JlZGVudGlhbBIUCgxyZXN0YXJ0X3J1bGUYBCABKAki4gIKEFJlZ2lzdHJ5UHJvdmlkZXISCgoCaWQYASABKAkSDAoEa2luZBgCIAEoCRIMCgRob3N0GAMgASgJEgwKBGF1dGgYBCABKAkSEgoKY3JlZGVudGlhbBgFIAEoCRIQCghwcmlvcml0eRgGIAEoDRIZCgxjYXBhY2l0eV9ycHMYByABKARIAIgBARIMCgRwbGFuGAggASgJEhMKC2VudGl0bGVtZW50GAkgASgJEhoKDXF1YWxpZmljYXRpb24YCiABKAlIAYgBARIPCgdlbmFibGVkGAsgASgIEhAKCHNlbGVjdGVkGAwgASgIEhQKB2JpbmRpbmcYDSABKAlIAogBARIcCg9zZWxlY3Rpb25fYmxvY2sYDiABKAlIA4gBAUIPCg1fY2FwYWNpdHlfcnBzQhAKDl9xdWFsaWZpY2F0aW9uQgoKCF9iaW5kaW5nQhIKEF9zZWxlY3Rpb25fYmxvY2siaQoSUmVnaXN0cnlDcmVkZW50aWFsEgoKAmlkGAEgASgJEhAKCHJldmlzaW9uGAIgASgEEg0KBXN0YXRlGAMgASgJEg4KBm9yaWdpbhgEIAEoCRIWCg5jcmVhdGVkX3VuaXhfcxgFIAEoBCLTAgoZUHJvdmlkZXJEZXNjcmlwdG9yQ29tbWFuZBIKCgJpZBgBIAEoCRIRCgRraW5kGAIgASgJSACIAQESFQoIZW5kcG9pbnQYAyABKAlIAYgBARIRCgRhdXRoGAQgASgJSAKIAQESFwoKY3JlZGVudGlhbBgFIAEoCUgDiAEBEhUKCHByaW9yaXR5GAYgASgNSASIAQESGQoMY2FwYWNpdHlfcnBzGAcgASgESAWIAQESEQoEcGxhbhgIIAEoCUgGiAEBEhgKC2VudGl0bGVtZW50GAkgASgJSAeIAQESEAoIZGlzYWJsZWQYCiABKAhCBwoFX2tpbmRCCwoJX2VuZHBvaW50QgcKBV9hdXRoQg0KC19jcmVkZW50aWFsQgsKCV9wcmlvcml0eUIPCg1fY2FwYWNpdHlfcnBzQgcKBV9wbGFuQg4KDF9lbnRpdGxlbWVudCI3ChdQcm92aWRlclByaW9yaXR5Q29tbWFuZBIKCgJpZBgBIAEoCRIQCghwcmlvcml0eRgCIAEoDSIxChVQcm92aWRlclNlbGVjdENvbW1hbmQSDAoEa2luZBgBIAEoCRIKCgJpZBgCIAEoCSI2ChZQcm92aWRlclF1YWxpZnlDb21tYW5kEgoKAmlkGAEgASgJEhAKCGV2aWRlbmNlGAIgASgJIj8KGVByb3ZpZGVyQ3JlZGVudGlhbENvbW1hbmQSEgoKY3JlZGVudGlhbBgBIAEoCRIOCgZzZWNyZXQYAiABKAwihgIKDFVudGlsU3RvcHBlZBI2CglhdXRvc3RhcnQYASABKAsyIy5kYXJrYm90LnByb2plY3QwLnYxLkF1dG9zdGFydEdyYW50EjQKB3ByZXZpZXcYAiABKAsyIy5kYXJrYm90LnByb2plY3QwLnYxLkF1dG9zdGFydEdyYW50EkUKDXF1YWxpZmljYXRpb24YAyABKAsyLi5kYXJrYm90LnByb2plY3QwLnYxLlVudGlsU3RvcHBlZFF1YWxpZmljYXRpb24SMQoDcnVuGAQgASgLMiQuZGFya2JvdC5wcm9qZWN0MC52MS5VbnRpbFN0b3BwZWRSdW4SDgoGZXJyb3JzGAUgAygJIpwDCg5BdXRvc3RhcnRHcmFudBIOCgZkaWdlc3QYASABKAkSDAoEbW9kZRgCIAEoCRIVCg1jb25maWdfc2hhMjU2GAMgASgJEhUKDWJpbmFyeV9zaGEyNTYYBCABKAkSEAoIZHVyYXRpb24YBSABKAkSGgoSY2hlY2twb2ludF9zZWNvbmRzGAYgASgEEh4KFm1pbmltdW1fcnVud2F5X3NlY29uZHMYByABKAQSGAoQcXVhbGlmaWNhdGlvbl9pZBgIIAEoCRIcChRwcm92aWRlcl9ldmlkZW5jZV9pZBgJIAEoCRIUCgxnZW5lc2lzX2hhc2gYCiABKAkSFgoOY2FwdHVyZV9zb3VyY2UYCyABKAkSDwoHcnBjX3JwcxgMIAEoBBIUCgxtYXhfcmVxdWVzdHMYDSABKAQSFQoNbWF4X3RvdGFsX21pYhgOIAEoBBIbChNtYXhfcnVudGltZV9zZWNvbmRzGA8gASgEEhYKDmV4cGlyZXNfdW5peF9zGBAgASgEEhcKD3JlY29yZGVkX3VuaXhfcxgRIAEoBCKiAgoZVW50aWxTdG9wcGVkUXVhbGlmaWNhdGlvbhIKCgJpZBgBIAEoCRIMCgRtb2RlGAIgASgJEhoKEmNoZWNrcG9pbnRfc2Vjb25kcxgDIAEoBBIeChZtaW5pbXVtX3J1bndheV9zZWNvbmRzGAQgASgEEhAKCGNvbXBsZXRlGAUgASgIEgwKBG9wZW4YBiADKAkSGAoLZXZpZGVuY2VfaWQYByABKAlIAIgBARIoChtldmlkZW5jZV92YWxpZF91bnRpbF91bml4X3MYCCABKARIAYgBARIbChNxdWFsaWZpZWRfYXRfdW5peF9zGAkgASgEQg4KDF9ldmlkZW5jZV9pZEIeChxfZXZpZGVuY2VfdmFsaWRfdW50aWxfdW5peF9zIrkCCg9VbnRpbFN0b3BwZWRSdW4SDgoGcnVuX2lkGAEgASgJEhgKEHF1YWxpZmljYXRpb25faWQYAiABKAkSFgoOc3RhcnRlZF91bml4X3MYAyABKAQSGgoSY2hlY2twb2ludF9zZWNvbmRzGAQgASgEEg8KB3dpbmRvd3MYBSABKAQSGQoMbGFzdF92ZXJkaWN0GAYgASgJSACIAQESEwoLbGFzdF9kZXRhaWwYByABKAkSHgoRbGFzdF9lbmRlZF91bml4X3MYCCABKARIAYgBARITCgZoYWx0ZWQYCSABKAlIAogBARIOCgZzZWFsZWQYCiABKAgSEAoIY29tcGxldGUYCyABKAhCDwoNX2xhc3RfdmVyZGljdEIUChJfbGFzdF9lbmRlZF91bml4X3NCCQoHX2hhbHRlZCKYBAoIQm9vdEZsb3cSDwoHZmxvd19pZBgBIAEoCRIMCgRtb2RlGAIgASgJEhwKD2NyZWF0ZWRfdW5peF9tcxgDIAEoBEgAiAEBEhcKD29mZmxpbmVfdmVyZGljdBgEIAEoCRIVCg1vZmZsaW5lX3Rva2VuGAUgASgJEhcKD29mZmxpbmVfYmxvY2tlZBgGIAMoCRITCgt0dGxfc2Vjb25kcxgHIAEoBBISCgp0dGxfc291cmNlGAggASgJEi4KB25ldHdvcmsYCSABKAsyHS5kYXJrYm90LnByb2plY3QwLnYxLkJvb3RTdGVwEhwKD25ldHdvcmtfdmVyZGljdBgKIAEoCUgBiAEBEhoKDW5ldHdvcmtfdG9rZW4YCyABKAlIAogBARIXCg9uZXR3b3JrX2Jsb2NrZWQYDCADKAkSHAoPb2JzZXJ2ZWRfdW5peF9zGA0gASgESAOIAQESGwoOZXhwaXJlc191bml4X3MYDiABKARIBIgBARIsCgVzdGFydBgPIAEoCzIdLmRhcmtib3QucHJvamVjdDAudjEuQm9vdFN0ZXASEAoIZHVyYXRpb24YECABKAlCEgoQX2NyZWF0ZWRfdW5peF9tc0ISChBfbmV0d29ya192ZXJkaWN0QhAKDl9uZXR3b3JrX3Rva2VuQhIKEF9vYnNlcnZlZF91bml4X3NCEQoPX2V4cGlyZXNfdW5peF9zIq0BCghCb290U3RlcBINCgVzdGF0ZRgBIAEoCRIMCgRjb2RlGAIgASgJEhMKBnJ1bl9pZBgDIAEoCUgAiAEBEhwKD3N0YXJ0ZWRfdW5peF9tcxgEIAEoBEgBiAEBEh0KEGZpbmlzaGVkX3VuaXhfbXMYBSABKARIAogBAUIJCgdfcnVuX2lkQhIKEF9zdGFydGVkX3VuaXhfbXNCEwoRX2ZpbmlzaGVkX3VuaXhfbXMimAIKDEFuYWx5dGljc1J1bhIOCgZydW5faWQYASABKAkSDAoEcm9vdBgCIAEoCRIOCgZsYXlvdXQYAyABKAkSGAoQY2FwdHVyZV9jb21wbGV0ZRgEIAEoCBI8Cg1xdWFsaWZpY2F0aW9uGAUgASgLMiUuZGFya2JvdC5wcm9qZWN0MC52MS5SdW5RdWFsaWZpY2F0aW9uEh0KEG1vZGlmaWVkX3VuaXhfbXMYBiABKARIAIgBARIPCgdydW5uaW5nGAcgASgIEj0KDHZlcmlmaWNhdGlvbhgIIAMoCzInLmRhcmtib3QucHJvamVjdDAudjEuVmVyaWZpY2F0aW9uTWV0aG9kQhMKEV9tb2RpZmllZF91bml4X21zIvgGChJWZXJpZmljYXRpb25NZXRob2QSDgoGbWV0aG9kGAEgASgJEg8KB3NwZWNfaWQYAiABKAkSEAoIYXR0ZW1wdHMYAyABKAQSDwoHcnVubmluZxgEIAEoCBIOCgZzdGF0dXMYBSABKAkSEwoGcmVhc29uGAYgASgJSACIAQESFgoJcmVzdWx0X2lkGAcgASgJSAGIAQESDwoHb2ZmbGluZRgIIAEoCBIdChBwcm92aWRlcl9iaW5kaW5nGAkgASgJSAKIAQESGAoQY2hlY2twb2ludF9yZWFkcxgKIAEoBBISCgVjYWxscxgLIAEoBEgDiAEBEhIKBWJ5dGVzGAwgASgESASIAQESFQoNcHJvZ3Jlc3NfdW5pdBgNIAEoCRIVCg1wcm9ncmVzc19kb25lGA4gASgEEhsKDnByb2dyZXNzX3RvdGFsGA8gASgESAWIAQESMgoJcXVhbGlmaWVkGBAgAygLMh8uZGFya2JvdC5wcm9qZWN0MC52MS5OYW1lZENvdW50EjEKCG9ic2VydmVkGBEgAygLMh8uZGFya2JvdC5wcm9qZWN0MC52MS5OYW1lZENvdW50EjAKB21hcmtlcnMYEiADKAsyHy5kYXJrYm90LnByb2plY3QwLnYxLk5hbWVkQ291bnQSMwoKY29tcGFyaXNvbhgTIAMoCzIfLmRhcmtib3QucHJvamVjdDAudjEuTmFtZWRDb3VudBIaCg1kaXNhZ3JlZW1lbnRzGBQgASgESAaIAQESFwoKdW5yZXNvbHZlZBgVIAEoBEgHiAEBEhQKB3Vua25vd24YFiABKARICIgBARI1Cgx0cmFuc2FjdGlvbnMYFyADKAsyHy5kYXJrYm90LnByb2plY3QwLnYxLk5hbWVkQ291bnQSGAoLY2FsbHNfdG90YWwYGCABKARICYgBARIYCgtieXRlc190b3RhbBgZIAEoBEgKiAEBQgkKB19yZWFzb25CDAoKX3Jlc3VsdF9pZEITChFfcHJvdmlkZXJfYmluZGluZ0IICgZfY2FsbHNCCAoGX2J5dGVzQhEKD19wcm9ncmVzc190b3RhbEIQCg5fZGlzYWdyZWVtZW50c0INCgtfdW5yZXNvbHZlZEIKCghfdW5rbm93bkIOCgxfY2FsbHNfdG90YWxCDgoMX2J5dGVzX3RvdGFsIikKCk5hbWVkQ291bnQSDAoEbmFtZRgBIAEoCRINCgVjb3VudBgCIAEoBCJyChBSdW5RdWFsaWZpY2F0aW9uEg8KB3ZlcmRpY3QYASABKAkSFAoMdmVyaWZpY2F0aW9uGAIgASgJEg4KBmZhaWxlZBgDIAMoCRIYChBxdWFsaWZpY2F0aW9uX2lkGAQgASgJEg0KBWVycm9yGAUgASgJIu8FCgxBbmFseXRpY3NKb2ISDgoGam9iX2lkGAEgASgJEgwKBGtpbmQYAiABKAkSCwoDcnVuGAMgASgJEg0KBXN0YXRlGAQgASgJEgwKBGNvZGUYBSABKAkSHgoRcmVxdWVzdGVkX3VuaXhfbXMYBiABKARIAYgBARIcCg9zdGFydGVkX3VuaXhfbXMYByABKARIAogBARIdChBmaW5pc2hlZF91bml4X21zGAggASgESAOIAQESEwoLbWF4X3NlY29uZHMYCSABKAQSFQoNbWF4X3JhbV9ieXRlcxgKIAEoBBIuCgZwYXJhbXMYCyABKAsyHi5kYXJrYm90LnByb2plY3QwLnYxLkpvYlBhcmFtcxIqCgRncmlkGAwgASgLMhwuZGFya2JvdC5wcm9qZWN0MC52MS5Kb2JHcmlkEhsKE3ZlcmlmaWNhdGlvbl9yZXN1bHQYDSABKAkSNwoIYmFja3Rlc3QYDiABKAsyIy5kYXJrYm90LnByb2plY3QwLnYxLkJhY2t0ZXN0UmVzdWx0SAASNwoIb3B0aW1pemUYDyABKAsyIy5kYXJrYm90LnByb2plY3QwLnYxLk9wdGltaXplUmVzdWx0SAASPgoMcG9zdF9jYXB0dXJlGBEgASgLMiYuZGFya2JvdC5wcm9qZWN0MC52MS5Qb3N0Q2FwdHVyZVJlc3VsdEgAEisKAmgwGBIgASgLMh0uZGFya2JvdC5wcm9qZWN0MC52MS5IMFJlc3VsdEgAEj8KEXBvc3RfY2FwdHVyZV9zcGVjGBAgASgLMiQuZGFya2JvdC5wcm9qZWN0MC52MS5Qb3N0Q2FwdHVyZVNwZWMSLAoHaDBfc3BlYxgTIAEoCzIbLmRhcmtib3QucHJvamVjdDAudjEuSDBTcGVjQggKBnJlc3VsdEIUChJfcmVxdWVzdGVkX3VuaXhfbXNCEgoQX3N0YXJ0ZWRfdW5peF9tc0ITChFfZmluaXNoZWRfdW5peF9tcyJZCgZIMFNwZWMSEwoLd2luZG93X2RheXMYASABKAQSFwoKbWluX3NhbXBsZRgCIAEoBEgAiAEBEhIKCmVuZF91bml4X3MYAyABKANCDQoLX21pbl9zYW1wbGUikAUKCEgwUmVzdWx0EhEKCXJlc3VsdF9pZBgBIAEoCRIbChN3aW5kb3dfc3RhcnRfdW5peF9zGAIgASgDEhkKEXdpbmRvd19lbmRfdW5peF9zGAMgASgDEhQKDGxpcXVpZGF0aW9ucxgEIAEoBBISCgpkdXBsaWNhdGVzGAUgASgEEhUKDXJ1bnNfdmVyaWZpZWQYBiABKAQSGwoTcnVuc193aXRob3V0X3Jlc3VsdBgHIAEoBBIPCgd3aW5uZXJzGAggASgEEiEKFHRvcF93aW5uZXJfc2hhcmVfYnBzGAkgASgESACIAQESDQoFYmFuZHMYCiABKAQSFQoNc2FtcGxlX3N0YXR1cxgLIAEoCRIMCgR0YWdzGAwgASgJEhgKC2NvbXBldGl0aXZlGA0gASgESAGIAQESGQoMdHJhbnNhY3Rpb25zGA4gASgESAKIAQESHAoPZmFpbGVkX2F0dGVtcHRzGA8gASgESAOIAQESHAoPY292ZXJlZF9zZWNvbmRzGBAgASgESASIAQESGgoNYmFuZHNfYm91bmRlZBgRIAEoBEgFiAEBEicKGmJhbmRzX2JlbG93X291cl9taW5fcHJvZml0GBIgASgESAaIAQESGQoMdGlwX2FjY291bnRzGBMgASgESAeIAQFCFwoVX3RvcF93aW5uZXJfc2hhcmVfYnBzQg4KDF9jb21wZXRpdGl2ZUIPCg1fdHJhbnNhY3Rpb25zQhIKEF9mYWlsZWRfYXR0ZW1wdHNCEgoQX2NvdmVyZWRfc2Vjb25kc0IQCg5fYmFuZHNfYm91bmRlZEIdChtfYmFuZHNfYmVsb3dfb3VyX21pbl9wcm9maXRCDwoNX3RpcF9hY2NvdW50cyJjCg9Qb3N0Q2FwdHVyZVNwZWMSFAoMY2hhaW5fdmVyaWZ5GAEgASgJEg8KB29mZmxpbmUYAiABKAgSDgoGcmVzdW1lGAMgASgIEhkKEXByb3ZpZGVyX2V2aWRlbmNlGAQgASgIIlAKEVBvc3RDYXB0dXJlUmVzdWx0EhcKD3ZlcmlmaWNhdGlvbl9vaxgBIAEoCBIRCglyZXBvcnRfb2sYAiABKAgSDwoHdmVyZGljdBgDIAEoCSJ/CglKb2JQYXJhbXMSRgobbWluX3Byb2ZpdF91c2RfbWljcm9fc2lnbmVkGAEgASgLMiEuZGFya2JvdC5jb250cm9sLnYxLlNpZ25lZERlY2ltYWwSGQoRY2FwaXRhbF91c2RfbWljcm8YAiADKAQSDwoHZnVuZGluZxgDIAMoCSLHAQoHSm9iR3JpZBJGChttaW5fcHJvZml0X3VzZF9taWNyb19zaWduZWQYASADKAsyIS5kYXJrYm90LmNvbnRyb2wudjEuU2lnbmVkRGVjaW1hbBI1CgxmdW5kaW5nX3NldHMYAiADKAsyHy5kYXJrYm90LnByb2plY3QwLnYxLkZ1bmRpbmdTZXQSEwoLaG9sZG91dF9icHMYAyABKAQSEgoKbWF4X3BvaW50cxgEIAEoBBIUCgxzYXZlX3Byb2ZpbGUYBSABKAgiHQoKRnVuZGluZ1NldBIPCgdjbGFzc2VzGAEgAygJIt0BCg5CYWNrdGVzdFJlc3VsdBIRCglyZXN1bHRfaWQYASABKAkSFgoOY2FwdHVyZV9ydW5faWQYAiABKAkSDAoEcm93cxgDIAEoBBIQCghhdHRlbXB0cxgEIAEoBBIUCgxsaXF1aWRhdGlvbnMYBSABKAQSQQoOY291bnRlcmZhY3R1YWwYBiABKAsyKS5kYXJrYm90LnByb2plY3QwLnYxLlNoYWRvd0NvdW50ZXJmYWN0dWFsEhAKCHJlYWxpemVkGAcgASgJEhUKDXJlc3VsdF9zaGEyNTYYCCABKAwi/QEKFFNoYWRvd0NvdW50ZXJmYWN0dWFsEg8KB21hdGNoZWQYASABKAQSDwoHdW5rbm93bhgCIAEoBBIYChBmdW5kaW5nX2V4Y2x1ZGVkGAMgASgEEhUKDWJlbG93X21pbmltdW0YBCABKAQSDAoEd2lucxgFIAEoBBISCgpsb3N0X3JhY2VzGAYgASgEEhYKDm5vX2NoYWluX2V2ZW50GAcgASgEEjcKCXNjZW5hcmlvcxgIIAMoCzIkLmRhcmtib3QucHJvamVjdDAudjEuQ2FwaXRhbFNjZW5hcmlvEg8KB2xhbmRpbmcYCSABKAkSDgoGc2FmZXR5GAogASgJItMCCg9DYXBpdGFsU2NlbmFyaW8SGQoRY2FwaXRhbF91c2RfbWljcm8YASABKAQSDAoEd2lucxgCIAEoBBJDChh3aW5fbmV0X3VzZF9taWNyb19zaWduZWQYAyABKAsyIS5kYXJrYm90LmNvbnRyb2wudjEuU2lnbmVkRGVjaW1hbBIXCg9mYWlsZWRfYXR0ZW1wdHMYBCABKAQSRgoZZmFpbGVkX2Nvc3RfdXNkX21pY3JvX3N1bRgFIAEoCzIjLmRhcmtib3QuY29udHJvbC52MS5VbnNpZ25lZERlY2ltYWwSGwoTZmFpbGVkX2Nvc3RfdW5rbm93bhgGIAEoBBITCgtvdmVyX2J1ZGdldBgHIAEoBBI/ChRuZXRfdXNkX21pY3JvX3NpZ25lZBgIIAEoCzIhLmRhcmtib3QuY29udHJvbC52MS5TaWduZWREZWNpbWFsIp0CCg5PcHRpbWl6ZVJlc3VsdBIRCglyZXN1bHRfaWQYASABKAkSFgoOY2FwdHVyZV9ydW5faWQYAiABKAkSDgoGcG9pbnRzGAMgASgEEi4KBmNob3NlbhgEIAEoCzIeLmRhcmtib3QucHJvamVjdDAudjEuSm9iUGFyYW1zEjoKDmNob3Nlbl9ob2xkb3V0GAUgASgLMiIuZGFya2JvdC5wcm9qZWN0MC52MS5PcHRpbWl6ZVNjb3JlEjwKEGJhc2VsaW5lX2hvbGRvdXQYBiABKAsyIi5kYXJrYm90LnByb2plY3QwLnYxLk9wdGltaXplU2NvcmUSFQoNcmVzdWx0X3NoYTI1NhgHIAEoDBIPCgdwcm9maWxlGAggASgJIpMBCg1PcHRpbWl6ZVNjb3JlEkEKDmNvdW50ZXJmYWN0dWFsGAEgASgLMikuZGFya2JvdC5wcm9qZWN0MC52MS5TaGFkb3dDb3VudGVyZmFjdHVhbBI/ChRvYmplY3RpdmVfYnBzX3NpZ25lZBgCIAEoCzIhLmRhcmtib3QuY29udHJvbC52MS5TaWduZWREZWNpbWFsIqsBChJCYWNrdGVzdEpvYkNvbW1hbmQSDgoGcnVuX2lkGAEgASgJEhYKDm1pbl9wcm9maXRfdXNkGAIgASgJEhMKC2NhcGl0YWxfdXNkGAMgAygEEg8KB2Z1bmRpbmcYBCADKAkSEwoLbWF4X3NlY29uZHMYBSABKAQSFQoNbWF4X3JhbV9ieXRlcxgGIAEoBBIbChN2ZXJpZmljYXRpb25fcmVzdWx0GAcgASgJIk8KDEgwSm9iQ29tbWFuZBITCgt3aW5kb3dfZGF5cxgBIAEoBBITCgttYXhfc2Vjb25kcxgCIAEoBBIVCg1tYXhfcmFtX2J5dGVzGAMgASgEIuIBChJPcHRpbWl6ZUpvYkNvbW1hbmQSOQoIYmFzZWxpbmUYASABKAsyJy5kYXJrYm90LnByb2plY3QwLnYxLkJhY2t0ZXN0Sm9iQ29tbWFuZBIbChNtaW5fcHJvZml0X3VzZF9ncmlkGAIgAygJEjUKDGZ1bmRpbmdfc2V0cxgDIAMoCzIfLmRhcmtib3QucHJvamVjdDAudjEuRnVuZGluZ1NldBITCgtob2xkb3V0X2JwcxgEIAEoBBISCgptYXhfcG9pbnRzGAUgASgEEhQKDHNhdmVfcHJvZmlsZRgGIAEoCCJeChJCb290T2ZmbGluZUNvbW1hbmQSDAoEbW9kZRgBIAEoCRIYCgt0dGxfc2Vjb25kcxgCIAEoBEgAiAEBEhAKCGR1cmF0aW9uGAMgASgJQg4KDF90dGxfc2Vjb25kcyKGAQoSQm9vdE5ldHdvcmtDb21tYW5kEg8KB2Zsb3dfaWQYASABKAkSFgoOYXBwcm92ZWRfdG9rZW4YAiABKAkSFAoMZ2VuZXNpc19oYXNoGAMgASgJEhkKEXByb3ZpZGVyX2V2aWRlbmNlGAQgASgJEhYKDmNhcHR1cmVfc291cmNlGAUgASgJIqkBChBCb290U3RhcnRDb21tYW5kEg8KB2Zsb3dfaWQYASABKAkSFgoOYXBwcm92ZWRfdG9rZW4YAiABKAkSEAoIZHVyYXRpb24YAyABKAkSFgoOY2FwdHVyZV9zb3VyY2UYBCABKAkSFAoMY2hhaW5fdmVyaWZ5GAUgASgJEhoKDXF1YWxpZmljYXRpb24YBiABKAlIAIgBAUIQCg5fcXVhbGlmaWNhdGlvbiKyAQoXQXV0b3N0YXJ0UHJldmlld0NvbW1hbmQSFQoNcXVhbGlmaWNhdGlvbhgBIAEoCRISCgpjaGVja3BvaW50GAIgASgJEhYKDm1pbmltdW1fcnVud2F5GAMgASgJEhQKDGdlbmVzaXNfaGFzaBgEIAEoCRITCgttYXhfcnVudGltZRgFIAEoCRIRCgl2YWxpZF9mb3IYBiABKAkSFgoOY2FwdHVyZV9zb3VyY2UYByABKAkiLwoVQXV0b3N0YXJ0R3JhbnRDb21tYW5kEhYKDmNvbmZpcm1fZGlnZXN0GAEgASgJIpcBChVQb3N0Q2FwdHVyZUpvYkNvbW1hbmQSDgoGcnVuX2lkGAEgASgJEhQKDGNoYWluX3ZlcmlmeRgCIAEoCRIPCgdvZmZsaW5lGAMgASgIEhsKE3Jlc3VtZV92ZXJpZmljYXRpb24YBCABKAgSEwoLbWF4X3NlY29uZHMYBSABKAQSFQoNbWF4X3JhbV9ieXRlcxgGIAEoBEKXAQoXY29tLmRhcmtib3QucHJvamVjdDAudjFCDkFuYWx5dGljc1Byb3RvUAGiAgNEUFiqAhNEYXJrYm90LlByb2plY3QwLlYxygITRGFya2JvdFxQcm9qZWN0MFxWMeICH0Rhcmtib3RcUHJvamVjdDBcVjFcR1BCTWV0YWRhdGHqAhVEYXJrYm90OjpQcm9qZWN0MDo6VjFiBnByb3RvMw", [file_darkbot_control_v1_numeric]);
 
 /**
  * project0.analytics.jobs (topic 15, STATE; B03/U-9, STARTUP §4 B/C/E): the runs of the project a
@@ -90,6 +90,23 @@ export type AnalyticsJobs = Message<"darkbot.project0.v1.AnalyticsJobs"> & {
    * @generated from field: darkbot.project0.v1.UntilStopped until_stopped = 9;
    */
   untilStopped?: UntilStopped;
+
+  /**
+   * DEC-18: the provider registry (data/providers, `p0ctl provider …`) as its store holds it —
+   * staged: a change is effective at the next controlled start (the running run's providers:
+   * project0.status EffectiveRecord.providers). Unset when the store is unreadable.
+   *
+   * @generated from field: darkbot.project0.v1.ProviderRegistryView providers = 10;
+   */
+  providers?: ProviderRegistryView;
+
+  /**
+   * DEC-18: why the provider registry was not readable (providers is then unset); empty when
+   * read.
+   *
+   * @generated from field: string providers_error = 11;
+   */
+  providersError: string;
 };
 
 /**
@@ -99,6 +116,411 @@ export type AnalyticsJobs = Message<"darkbot.project0.v1.AnalyticsJobs"> & {
 export const AnalyticsJobsSchema: GenMessage<AnalyticsJobs> = /*@__PURE__*/
   messageDesc(file_darkbot_project0_v1_analytics, 0);
 
+/**
+ * DEC-18: one committed version of the provider registry — secret-free (hosts, credential ids
+ * and revisions; never an endpoint's path, a key or a keychain account).
+ *
+ * @generated from message darkbot.project0.v1.ProviderRegistryView
+ */
+export type ProviderRegistryView = Message<"darkbot.project0.v1.ProviderRegistryView"> & {
+  /**
+   * The committed version (0: no provider yet); the expected version of every change (CAS).
+   *
+   * @generated from field: uint64 version = 1;
+   */
+  version: bigint;
+
+  /**
+   * Every provider, by kind, priority and id.
+   *
+   * @generated from field: repeated darkbot.project0.v1.RegistryProvider providers = 2;
+   */
+  providers: RegistryProvider[];
+
+  /**
+   * Every credential's newest revision.
+   *
+   * @generated from field: repeated darkbot.project0.v1.RegistryCredential credentials = 3;
+   */
+  credentials: RegistryCredential[];
+
+  /**
+   * When a change takes effect: next_controlled_start (DEC-05).
+   *
+   * @generated from field: string restart_rule = 4;
+   */
+  restartRule: string;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.ProviderRegistryView.
+ * Use `create(ProviderRegistryViewSchema)` to create a new message.
+ */
+export const ProviderRegistryViewSchema: GenMessage<ProviderRegistryView> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 1);
+
+/**
+ * DEC-18: one provider of the registry.
+ *
+ * @generated from message darkbot.project0.v1.RegistryProvider
+ */
+export type RegistryProvider = Message<"darkbot.project0.v1.RegistryProvider"> & {
+  /**
+   * Provider id ([a-z0-9_-]{1,32}).
+   *
+   * @generated from field: string id = 1;
+   */
+  id: string;
+
+  /**
+   * rpc | wss | geyser | jupiter | jito.
+   *
+   * @generated from field: string kind = 2;
+   */
+  kind: string;
+
+  /**
+   * The endpoint's host (its path and scheme stay in the engine).
+   *
+   * @generated from field: string host = 3;
+   */
+  host: string;
+
+  /**
+   * How it authenticates: none | query:NAME | header:NAME | path (a name, never a value).
+   *
+   * @generated from field: string auth = 4;
+   */
+  auth: string;
+
+  /**
+   * The credential it authenticates with (its id); empty without authentication.
+   *
+   * @generated from field: string credential = 5;
+   */
+  credential: string;
+
+  /**
+   * Lower first among the providers of its kind.
+   *
+   * @generated from field: uint32 priority = 6;
+   */
+  priority: number;
+
+  /**
+   * Configured capacity in requests per second; unset when not configured.
+   *
+   * @generated from field: optional uint64 capacity_rps = 7;
+   */
+  capacityRps?: bigint;
+
+  /**
+   * free | unknown (FREE_ONLY: paid is refused, unknown never selected).
+   *
+   * @generated from field: string plan = 8;
+   */
+  plan: string;
+
+  /**
+   * The plan's name as the provider publishes it.
+   *
+   * @generated from field: string entitlement = 9;
+   */
+  entitlement: string;
+
+  /**
+   * The provider evidence record that qualified its binding; unset when not qualified.
+   *
+   * @generated from field: optional string qualification = 10;
+   */
+  qualification?: string;
+
+  /**
+   * Enabled (a disabled provider is never selected).
+   *
+   * @generated from field: bool enabled = 11;
+   */
+  enabled: boolean;
+
+  /**
+   * The selected provider of its kind.
+   *
+   * @generated from field: bool selected = 12;
+   */
+  selected: boolean;
+
+  /**
+   * Its secret-free binding (hex SHA-256) with the active credential revision; unset when its
+   * credential is revoked or unknown.
+   *
+   * @generated from field: optional string binding = 13;
+   */
+  binding?: string;
+
+  /**
+   * Why it cannot be the selected one of its kind now; unset when it can.
+   *
+   * @generated from field: optional string selection_block = 14;
+   */
+  selectionBlock?: string;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.RegistryProvider.
+ * Use `create(RegistryProviderSchema)` to create a new message.
+ */
+export const RegistryProviderSchema: GenMessage<RegistryProvider> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 2);
+
+/**
+ * DEC-18: a credential's newest revision (the secret is the macOS keychain's alone).
+ *
+ * @generated from message darkbot.project0.v1.RegistryCredential
+ */
+export type RegistryCredential = Message<"darkbot.project0.v1.RegistryCredential"> & {
+  /**
+   * Credential id.
+   *
+   * @generated from field: string id = 1;
+   */
+  id: string;
+
+  /**
+   * Its newest revision (a rotation is a new one, never reused).
+   *
+   * @generated from field: uint64 revision = 2;
+   */
+  revision: bigint;
+
+  /**
+   * active | revoked.
+   *
+   * @generated from field: string state = 3;
+   */
+  state: string;
+
+  /**
+   * input | env:NAME (the one-time .env import).
+   *
+   * @generated from field: string origin = 4;
+   */
+  origin: string;
+
+  /**
+   * When the revision was created.
+   *
+   * @generated from field: uint64 created_unix_s = 5;
+   */
+  createdUnixS: bigint;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.RegistryCredential.
+ * Use `create(RegistryCredentialSchema)` to create a new message.
+ */
+export const RegistryCredentialSchema: GenMessage<RegistryCredential> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 3);
+
+/**
+ * DEC-18: params of `project0.provider.add` and `project0.provider.update` (the registry's
+ * rules decide: a canonical endpoint without a credential in it, FREE_ONLY, a known kind). An
+ * update changes only the fields it sets.
+ *
+ * @generated from message darkbot.project0.v1.ProviderDescriptorCommand
+ */
+export type ProviderDescriptorCommand = Message<"darkbot.project0.v1.ProviderDescriptorCommand"> & {
+  /**
+   * Provider id.
+   *
+   * @generated from field: string id = 1;
+   */
+  id: string;
+
+  /**
+   * rpc | wss | geyser | jupiter | jito (add: required).
+   *
+   * @generated from field: optional string kind = 2;
+   */
+  kind?: string;
+
+  /**
+   * The endpoint (add: required); refused with a credential in it, never echoed.
+   *
+   * @generated from field: optional string endpoint = 3;
+   */
+  endpoint?: string;
+
+  /**
+   * none | query:NAME | header:NAME | path.
+   *
+   * @generated from field: optional string auth = 4;
+   */
+  auth?: string;
+
+  /**
+   * The credential id it authenticates with.
+   *
+   * @generated from field: optional string credential = 5;
+   */
+  credential?: string;
+
+  /**
+   * Priority among its kind.
+   *
+   * @generated from field: optional uint32 priority = 6;
+   */
+  priority?: number;
+
+  /**
+   * Configured capacity in requests per second.
+   *
+   * @generated from field: optional uint64 capacity_rps = 7;
+   */
+  capacityRps?: bigint;
+
+  /**
+   * free | unknown.
+   *
+   * @generated from field: optional string plan = 8;
+   */
+  plan?: string;
+
+  /**
+   * The plan's name.
+   *
+   * @generated from field: optional string entitlement = 9;
+   */
+  entitlement?: string;
+
+  /**
+   * Added disabled (add only).
+   *
+   * @generated from field: bool disabled = 10;
+   */
+  disabled: boolean;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.ProviderDescriptorCommand.
+ * Use `create(ProviderDescriptorCommandSchema)` to create a new message.
+ */
+export const ProviderDescriptorCommandSchema: GenMessage<ProviderDescriptorCommand> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 4);
+
+/**
+ * DEC-18: params of `project0.provider.priority`.
+ *
+ * @generated from message darkbot.project0.v1.ProviderPriorityCommand
+ */
+export type ProviderPriorityCommand = Message<"darkbot.project0.v1.ProviderPriorityCommand"> & {
+  /**
+   * Provider id.
+   *
+   * @generated from field: string id = 1;
+   */
+  id: string;
+
+  /**
+   * The new priority.
+   *
+   * @generated from field: uint32 priority = 2;
+   */
+  priority: number;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.ProviderPriorityCommand.
+ * Use `create(ProviderPriorityCommandSchema)` to create a new message.
+ */
+export const ProviderPriorityCommandSchema: GenMessage<ProviderPriorityCommand> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 5);
+
+/**
+ * DEC-18: params of `project0.provider.select`.
+ *
+ * @generated from message darkbot.project0.v1.ProviderSelectCommand
+ */
+export type ProviderSelectCommand = Message<"darkbot.project0.v1.ProviderSelectCommand"> & {
+  /**
+   * rpc | wss | jupiter | jito.
+   *
+   * @generated from field: string kind = 1;
+   */
+  kind: string;
+
+  /**
+   * Provider id.
+   *
+   * @generated from field: string id = 2;
+   */
+  id: string;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.ProviderSelectCommand.
+ * Use `create(ProviderSelectCommandSchema)` to create a new message.
+ */
+export const ProviderSelectCommandSchema: GenMessage<ProviderSelectCommand> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 6);
+
+/**
+ * DEC-18: params of `project0.provider.qualify`.
+ *
+ * @generated from message darkbot.project0.v1.ProviderQualifyCommand
+ */
+export type ProviderQualifyCommand = Message<"darkbot.project0.v1.ProviderQualifyCommand"> & {
+  /**
+   * Provider id.
+   *
+   * @generated from field: string id = 1;
+   */
+  id: string;
+
+  /**
+   * The stored provider evidence record id (16 hex), or "-" to clear the qualification.
+   *
+   * @generated from field: string evidence = 2;
+   */
+  evidence: string;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.ProviderQualifyCommand.
+ * Use `create(ProviderQualifyCommandSchema)` to create a new message.
+ */
+export const ProviderQualifyCommandSchema: GenMessage<ProviderQualifyCommand> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 7);
+
+/**
+ * DEC-18: params of `project0.provider.credential.set` and `…credential.rotate`. The gateway
+ * separates the secret before any durable record (its command ledger holds the request without
+ * it) and hands it to the native controller's standard input only.
+ *
+ * @generated from message darkbot.project0.v1.ProviderCredentialCommand
+ */
+export type ProviderCredentialCommand = Message<"darkbot.project0.v1.ProviderCredentialCommand"> & {
+  /**
+   * Credential id.
+   *
+   * @generated from field: string credential = 1;
+   */
+  credential: string;
+
+  /**
+   * The secret (in transit only).
+   *
+   * @generated from field: bytes secret = 2;
+   */
+  secret: Uint8Array;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.ProviderCredentialCommand.
+ * Use `create(ProviderCredentialCommandSchema)` to create a new message.
+ */
+export const ProviderCredentialCommandSchema: GenMessage<ProviderCredentialCommand> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_analytics, 8);
+
 /**
  * DEC-16: unbounded runs (RunHorizon::UntilStopped) as the stores hold them. Expiry and
  * evidence freshness are compared with the viewer's clock; every native gate decides again at
@@ -148,7 +570,7 @@ export type UntilStopped = Message<"darkbot.project0.v1.UntilStopped"> & {
  * Use `create(UntilStoppedSchema)` to create a new message.
  */
 export const UntilStoppedSchema: GenMessage<UntilStopped> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 1);
+  messageDesc(file_darkbot_project0_v1_analytics, 9);
 
 /**
  * DEC-16: a standing autostart approval ("Dauerfreigabe") and the binding it was given for.
@@ -281,7 +703,7 @@ export type AutostartGrant = Message<"darkbot.project0.v1.AutostartGrant"> & {
  * Use `create(AutostartGrantSchema)` to create a new message.
  */
 export const AutostartGrantSchema: GenMessage<AutostartGrant> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 2);
+  messageDesc(file_darkbot_project0_v1_analytics, 10);
 
 /**
  * DEC-16: a qualification of an unbounded run and the parts that did not pass.
@@ -358,7 +780,7 @@ export type UntilStoppedQualification = Message<"darkbot.project0.v1.UntilStoppe
  * Use `create(UntilStoppedQualificationSchema)` to create a new message.
  */
 export const UntilStoppedQualificationSchema: GenMessage<UntilStoppedQualification> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 3);
+  messageDesc(file_darkbot_project0_v1_analytics, 11);
 
 /**
  * DEC-16: an unbounded raw capture's windows journal as far as it is written.
@@ -449,7 +871,7 @@ export type UntilStoppedRun = Message<"darkbot.project0.v1.UntilStoppedRun"> & {
  * Use `create(UntilStoppedRunSchema)` to create a new message.
  */
 export const UntilStoppedRunSchema: GenMessage<UntilStoppedRun> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 4);
+  messageDesc(file_darkbot_project0_v1_analytics, 12);
 
 /**
  * DEC-07 (N7): one boot flow — offline preflight, the operator's network approval, the network
@@ -579,7 +1001,7 @@ export type BootFlow = Message<"darkbot.project0.v1.BootFlow"> & {
  * Use `create(BootFlowSchema)` to create a new message.
  */
 export const BootFlowSchema: GenMessage<BootFlow> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 5);
+  messageDesc(file_darkbot_project0_v1_analytics, 13);
 
 /**
  * One step of a boot flow.
@@ -628,7 +1050,7 @@ export type BootStep = Message<"darkbot.project0.v1.BootStep"> & {
  * Use `create(BootStepSchema)` to create a new message.
  */
 export const BootStepSchema: GenMessage<BootStep> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 6);
+  messageDesc(file_darkbot_project0_v1_analytics, 14);
 
 /**
  * One run directory under the project's run roots.
@@ -699,7 +1121,7 @@ export type AnalyticsRun = Message<"darkbot.project0.v1.AnalyticsRun"> & {
  * Use `create(AnalyticsRunSchema)` to create a new message.
  */
 export const AnalyticsRunSchema: GenMessage<AnalyticsRun> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 7);
+  messageDesc(file_darkbot_project0_v1_analytics, 15);
 
 /**
  * DEC-08 (ENTSCHEIDUNGEN §8.6): one chain verification method of a run, its spec with the
@@ -894,7 +1316,7 @@ export type VerificationMethod = Message<"darkbot.project0.v1.VerificationMethod
  * Use `create(VerificationMethodSchema)` to create a new message.
  */
 export const VerificationMethodSchema: GenMessage<VerificationMethod> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 8);
+  messageDesc(file_darkbot_project0_v1_analytics, 16);
 
 /**
  * A count under a bounded name.
@@ -922,7 +1344,7 @@ export type NamedCount = Message<"darkbot.project0.v1.NamedCount"> & {
  * Use `create(NamedCountSchema)` to create a new message.
  */
 export const NamedCountSchema: GenMessage<NamedCount> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 9);
+  messageDesc(file_darkbot_project0_v1_analytics, 17);
 
 /**
  * The newest C0-E1 master qualification record of a run.
@@ -971,7 +1393,7 @@ export type RunQualification = Message<"darkbot.project0.v1.RunQualification"> &
  * Use `create(RunQualificationSchema)` to create a new message.
  */
 export const RunQualificationSchema: GenMessage<RunQualification> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 10);
+  messageDesc(file_darkbot_project0_v1_analytics, 18);
 
 /**
  * One analytics job of the controller's store.
@@ -1129,7 +1551,7 @@ export type AnalyticsJob = Message<"darkbot.project0.v1.AnalyticsJob"> & {
  * Use `create(AnalyticsJobSchema)` to create a new message.
  */
 export const AnalyticsJobSchema: GenMessage<AnalyticsJob> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 11);
+  messageDesc(file_darkbot_project0_v1_analytics, 19);
 
 /**
  * H0 (Track H0): the window of a historical liquidation report over every saved run.
@@ -1164,7 +1586,7 @@ export type H0Spec = Message<"darkbot.project0.v1.H0Spec"> & {
  * Use `create(H0SpecSchema)` to create a new message.
  */
 export const H0SpecSchema: GenMessage<H0Spec> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 12);
+  messageDesc(file_darkbot_project0_v1_analytics, 20);
 
 /**
  * A completed H0 report's headline (the full report is the job's typed H0 journal and render).
@@ -1312,7 +1734,7 @@ export type H0Result = Message<"darkbot.project0.v1.H0Result"> & {
  * Use `create(H0ResultSchema)` to create a new message.
  */
 export const H0ResultSchema: GenMessage<H0Result> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 13);
+  messageDesc(file_darkbot_project0_v1_analytics, 21);
 
 /**
  * The verification request of a post-capture job (A08/F-02).
@@ -1354,7 +1776,7 @@ export type PostCaptureSpec = Message<"darkbot.project0.v1.PostCaptureSpec"> & {
  * Use `create(PostCaptureSpecSchema)` to create a new message.
  */
 export const PostCaptureSpecSchema: GenMessage<PostCaptureSpec> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 14);
+  messageDesc(file_darkbot_project0_v1_analytics, 22);
 
 /**
  * A completed post-capture chain: each stage's outcome (the qualification record is the run's).
@@ -1389,7 +1811,7 @@ export type PostCaptureResult = Message<"darkbot.project0.v1.PostCaptureResult">
  * Use `create(PostCaptureResultSchema)` to create a new message.
  */
 export const PostCaptureResultSchema: GenMessage<PostCaptureResult> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 15);
+  messageDesc(file_darkbot_project0_v1_analytics, 23);
 
 /**
  * Economic parameters of a backtest point.
@@ -1424,7 +1846,7 @@ export type JobParams = Message<"darkbot.project0.v1.JobParams"> & {
  * Use `create(JobParamsSchema)` to create a new message.
  */
 export const JobParamsSchema: GenMessage<JobParams> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 16);
+  messageDesc(file_darkbot_project0_v1_analytics, 24);
 
 /**
  * The bounded explicit grid of an optimization.
@@ -1473,7 +1895,7 @@ export type JobGrid = Message<"darkbot.project0.v1.JobGrid"> & {
  * Use `create(JobGridSchema)` to create a new message.
  */
 export const JobGridSchema: GenMessage<JobGrid> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 17);
+  messageDesc(file_darkbot_project0_v1_analytics, 25);
 
 /**
  * Admitted funding classes; empty: every class.
@@ -1494,7 +1916,7 @@ export type FundingSet = Message<"darkbot.project0.v1.FundingSet"> & {
  * Use `create(FundingSetSchema)` to create a new message.
  */
 export const FundingSetSchema: GenMessage<FundingSet> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 18);
+  messageDesc(file_darkbot_project0_v1_analytics, 26);
 
 /**
  * A completed backtest (OBSERVED_MARKET and COUNTERFACTUAL_SHADOW).
@@ -1564,7 +1986,7 @@ export type BacktestResult = Message<"darkbot.project0.v1.BacktestResult"> & {
  * Use `create(BacktestResultSchema)` to create a new message.
  */
 export const BacktestResultSchema: GenMessage<BacktestResult> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 19);
+  messageDesc(file_darkbot_project0_v1_analytics, 27);
 
 /**
  * Counterfactual Shadow executions: hypothetical, landing UNPROVEN, safety NOT_EVALUATED.
@@ -1648,7 +2070,7 @@ export type ShadowCounterfactual = Message<"darkbot.project0.v1.ShadowCounterfac
  * Use `create(ShadowCounterfactualSchema)` to create a new message.
  */
 export const ShadowCounterfactualSchema: GenMessage<ShadowCounterfactual> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 20);
+  messageDesc(file_darkbot_project0_v1_analytics, 28);
 
 /**
  * One capital scenario of the counterfactual.
@@ -1718,7 +2140,7 @@ export type CapitalScenario = Message<"darkbot.project0.v1.CapitalScenario"> & {
  * Use `create(CapitalScenarioSchema)` to create a new message.
  */
 export const CapitalScenarioSchema: GenMessage<CapitalScenario> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 21);
+  messageDesc(file_darkbot_project0_v1_analytics, 29);
 
 /**
  * A completed optimization: the point chosen on the training part and its holdout score.
@@ -1788,7 +2210,7 @@ export type OptimizeResult = Message<"darkbot.project0.v1.OptimizeResult"> & {
  * Use `create(OptimizeResultSchema)` to create a new message.
  */
 export const OptimizeResultSchema: GenMessage<OptimizeResult> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 22);
+  messageDesc(file_darkbot_project0_v1_analytics, 30);
 
 /**
  * The score of one point on one part of the window.
@@ -1816,7 +2238,7 @@ export type OptimizeScore = Message<"darkbot.project0.v1.OptimizeScore"> & {
  * Use `create(OptimizeScoreSchema)` to create a new message.
  */
 export const OptimizeScoreSchema: GenMessage<OptimizeScore> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 23);
+  messageDesc(file_darkbot_project0_v1_analytics, 31);
 
 /**
  * Parameters (CommandRequest.params) of project0.analytics.backtest.start: the options of
@@ -1881,7 +2303,7 @@ export type BacktestJobCommand = Message<"darkbot.project0.v1.BacktestJobCommand
  * Use `create(BacktestJobCommandSchema)` to create a new message.
  */
 export const BacktestJobCommandSchema: GenMessage<BacktestJobCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 24);
+  messageDesc(file_darkbot_project0_v1_analytics, 32);
 
 /**
  * Parameters (CommandRequest.params) of project0.analytics.h0.start: the options of
@@ -1917,7 +2339,7 @@ export type H0JobCommand = Message<"darkbot.project0.v1.H0JobCommand"> & {
  * Use `create(H0JobCommandSchema)` to create a new message.
  */
 export const H0JobCommandSchema: GenMessage<H0JobCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 25);
+  messageDesc(file_darkbot_project0_v1_analytics, 33);
 
 /**
  * Parameters (CommandRequest.params) of project0.analytics.optimize.start: the options of
@@ -1974,7 +2396,7 @@ export type OptimizeJobCommand = Message<"darkbot.project0.v1.OptimizeJobCommand
  * Use `create(OptimizeJobCommandSchema)` to create a new message.
  */
 export const OptimizeJobCommandSchema: GenMessage<OptimizeJobCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 26);
+  messageDesc(file_darkbot_project0_v1_analytics, 34);
 
 /**
  * Parameters (CommandRequest.params) of project0.boot.offline: `p0ctl boot offline` — a new
@@ -2010,7 +2432,7 @@ export type BootOfflineCommand = Message<"darkbot.project0.v1.BootOfflineCommand
  * Use `create(BootOfflineCommandSchema)` to create a new message.
  */
 export const BootOfflineCommandSchema: GenMessage<BootOfflineCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 27);
+  messageDesc(file_darkbot_project0_v1_analytics, 35);
 
 /**
  * Parameters of project0.boot.network: the operator's approval of exactly this offline token;
@@ -2062,7 +2484,7 @@ export type BootNetworkCommand = Message<"darkbot.project0.v1.BootNetworkCommand
  * Use `create(BootNetworkCommandSchema)` to create a new message.
  */
 export const BootNetworkCommandSchema: GenMessage<BootNetworkCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 28);
+  messageDesc(file_darkbot_project0_v1_analytics, 36);
 
 /**
  * Parameters of project0.boot.start: the operator's approval of exactly this network token.
@@ -2119,7 +2541,7 @@ export type BootStartCommand = Message<"darkbot.project0.v1.BootStartCommand"> &
  * Use `create(BootStartCommandSchema)` to create a new message.
  */
 export const BootStartCommandSchema: GenMessage<BootStartCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 29);
+  messageDesc(file_darkbot_project0_v1_analytics, 37);
 
 /**
  * DEC-16: parameters of project0.autostart.preview — the terms of a standing autostart
@@ -2184,7 +2606,7 @@ export type AutostartPreviewCommand = Message<"darkbot.project0.v1.AutostartPrev
  * Use `create(AutostartPreviewCommandSchema)` to create a new message.
  */
 export const AutostartPreviewCommandSchema: GenMessage<AutostartPreviewCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 30);
+  messageDesc(file_darkbot_project0_v1_analytics, 38);
 
 /**
  * DEC-16: parameters of project0.autostart.grant — the operator confirms exactly the previewed
@@ -2206,7 +2628,7 @@ export type AutostartGrantCommand = Message<"darkbot.project0.v1.AutostartGrantC
  * Use `create(AutostartGrantCommandSchema)` to create a new message.
  */
 export const AutostartGrantCommandSchema: GenMessage<AutostartGrantCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 31);
+  messageDesc(file_darkbot_project0_v1_analytics, 39);
 
 /**
  * Parameters (CommandRequest.params) of project0.capture.verify.start: the options of
@@ -2265,4 +2687,4 @@ export type PostCaptureJobCommand = Message<"darkbot.project0.v1.PostCaptureJobC
  * Use `create(PostCaptureJobCommandSchema)` to create a new message.
  */
 export const PostCaptureJobCommandSchema: GenMessage<PostCaptureJobCommand> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_analytics, 32);
+  messageDesc(file_darkbot_project0_v1_analytics, 40);
diff --git a/frontend/src/core/generated/darkbot/project0/v1/system_pb.ts b/frontend/src/core/generated/darkbot/project0/v1/system_pb.ts
index a3f1159fd0fee1d678555a15845fa9e0c2c6dd10..56143d03d04ca8667797f68a222160a1b3099afa 100644
--- a/frontend/src/core/generated/darkbot/project0/v1/system_pb.ts
+++ b/frontend/src/core/generated/darkbot/project0/v1/system_pb.ts
@@ -10,7 +10,7 @@ import type { Message } from "@bufbuild/protobuf";
  * Describes the file darkbot/project0/v1/system.proto.
  */
 export const file_darkbot_project0_v1_system: GenFile = /*@__PURE__*/
-  fileDesc("CiBkYXJrYm90L3Byb2plY3QwL3YxL3N5c3RlbS5wcm90bxITZGFya2JvdC5wcm9qZWN0MC52MSJLCgxTeXN0ZW1TdGF0dXMSOwoLb3BlcmF0aW9uYWwYASABKAsyJi5kYXJrYm90LnByb2plY3QwLnYxLk9wZXJhdGlvbmFsQ29uZmlnIsIBChRPcGVyYXRpb25hbFBhcmFtZXRlchINCgVmaWVsZBgBIAEoCRIMCgR1bml0GAIgASgJEg0KBXZhbHVlGAMgASgEEhUKDWRlZmF1bHRfdmFsdWUYBCABKAQSCwoDZW52GAUgASgJEg4KBnNvdXJjZRgGIAEoCRIPCgdtaW5pbXVtGAcgASgEEg8KB21heGltdW0YCCABKAQSEwoLcHJvdmlzaW9uYWwYCSABKAkSEwoLY2hhbmdlX3J1bGUYCiABKAkiywQKD0VmZmVjdGl2ZVJlY29yZBIOCgZzY2hlbWEYASABKAkSDwoHcHJvZmlsZRgCIAEoCRIaChJycGNfZW5kcG9pbnRfY291bnQYAyABKAQSGgoSd3NzX2VuZHBvaW50X2NvdW50GAQgASgEEhAKCGZhbGxiYWNrGAUgASgIEhAKCGFkYXB0aXZlGAYgASgIEhAKCGR1cmF0aW9uGAcgASgJEhQKDHZlcmlmaWNhdGlvbhgIIAEoCRITCgtkaWFnbm9zdGljcxgJIAEoCBIPCgdmZV9tb2RlGAogASgJEhIKCmZlX3RhcmdldHMYCyADKAkSEAoIZmVfZ3JvdXAYDCABKAkSQgoHc291cmNlcxgNIAMoCzIxLmRhcmtib3QucHJvamVjdDAudjEuRWZmZWN0aXZlUmVjb3JkLlNvdXJjZXNFbnRyeRIjChtwcm92aWRlcl9jYXBhY2l0eV9xdWFsaWZpZWQYDiABKAgSEQoJZnJlZV9vbmx5GA8gASgIEhcKD2xpdmVfYXV0aG9yaXplZBgQIAEoCBIdChVzdWJtaXNzaW9uX2F1dGhvcml6ZWQYESABKAgSGQoRdGhyZXNob2xkX3RyaWdnZXIYEiABKAgSEQoJYzBfY2Vuc3VzGBMgASgIEhUKDXNvdXJjZV9nZXlzZXIYFCABKAkSHgoWc291cmNlX2dleXNlcl9wcm92aWRlchgVIAEoCRouCgxTb3VyY2VzRW50cnkSCwoDa2V5GAEgASgJEg0KBXZhbHVlGAIgASgJOgI4ASKLAQoRT3BlcmF0aW9uYWxDb25maWcSPQoKcGFyYW1ldGVycxgBIAMoCzIpLmRhcmtib3QucHJvamVjdDAudjEuT3BlcmF0aW9uYWxQYXJhbWV0ZXISNwoJZWZmZWN0aXZlGAIgASgLMiQuZGFya2JvdC5wcm9qZWN0MC52MS5FZmZlY3RpdmVSZWNvcmRClAEKF2NvbS5kYXJrYm90LnByb2plY3QwLnYxQgtTeXN0ZW1Qcm90b1ABogIDRFBYqgITRGFya2JvdC5Qcm9qZWN0MC5WMcoCE0Rhcmtib3RcUHJvamVjdDBcVjHiAh9EYXJrYm90XFByb2plY3QwXFYxXEdQQk1ldGFkYXRh6gIVRGFya2JvdDo6UHJvamVjdDA6OlYxYgZwcm90bzM");
+  fileDesc("CiBkYXJrYm90L3Byb2plY3QwL3YxL3N5c3RlbS5wcm90bxITZGFya2JvdC5wcm9qZWN0MC52MSJLCgxTeXN0ZW1TdGF0dXMSOwoLb3BlcmF0aW9uYWwYASABKAsyJi5kYXJrYm90LnByb2plY3QwLnYxLk9wZXJhdGlvbmFsQ29uZmlnIsIBChRPcGVyYXRpb25hbFBhcmFtZXRlchINCgVmaWVsZBgBIAEoCRIMCgR1bml0GAIgASgJEg0KBXZhbHVlGAMgASgEEhUKDWRlZmF1bHRfdmFsdWUYBCABKAQSCwoDZW52GAUgASgJEg4KBnNvdXJjZRgGIAEoCRIPCgdtaW5pbXVtGAcgASgEEg8KB21heGltdW0YCCABKAQSEwoLcHJvdmlzaW9uYWwYCSABKAkSEwoLY2hhbmdlX3J1bGUYCiABKAkigAUKD0VmZmVjdGl2ZVJlY29yZBIOCgZzY2hlbWEYASABKAkSDwoHcHJvZmlsZRgCIAEoCRIaChJycGNfZW5kcG9pbnRfY291bnQYAyABKAQSGgoSd3NzX2VuZHBvaW50X2NvdW50GAQgASgEEhAKCGZhbGxiYWNrGAUgASgIEhAKCGFkYXB0aXZlGAYgASgIEhAKCGR1cmF0aW9uGAcgASgJEhQKDHZlcmlmaWNhdGlvbhgIIAEoCRITCgtkaWFnbm9zdGljcxgJIAEoCBIPCgdmZV9tb2RlGAogASgJEhIKCmZlX3RhcmdldHMYCyADKAkSEAoIZmVfZ3JvdXAYDCABKAkSQgoHc291cmNlcxgNIAMoCzIxLmRhcmtib3QucHJvamVjdDAudjEuRWZmZWN0aXZlUmVjb3JkLlNvdXJjZXNFbnRyeRIjChtwcm92aWRlcl9jYXBhY2l0eV9xdWFsaWZpZWQYDiABKAgSEQoJZnJlZV9vbmx5GA8gASgIEhcKD2xpdmVfYXV0aG9yaXplZBgQIAEoCBIdChVzdWJtaXNzaW9uX2F1dGhvcml6ZWQYESABKAgSGQoRdGhyZXNob2xkX3RyaWdnZXIYEiABKAgSEQoJYzBfY2Vuc3VzGBMgASgIEhUKDXNvdXJjZV9nZXlzZXIYFCABKAkSHgoWc291cmNlX2dleXNlcl9wcm92aWRlchgVIAEoCRIzCglwcm92aWRlcnMYFiADKAsyIC5kYXJrYm90LnByb2plY3QwLnYxLlJ1blByb3ZpZGVyGi4KDFNvdXJjZXNFbnRyeRILCgNrZXkYASABKAkSDQoFdmFsdWUYAiABKAk6AjgBIpQBCgtSdW5Qcm92aWRlchIMCgRraW5kGAEgASgJEgoKAmlkGAIgASgJEg8KB2JpbmRpbmcYAyABKAkSGAoQcmVnaXN0cnlfdmVyc2lvbhgEIAEoBBIVCg1jcmVkZW50aWFsX2lkGAUgASgJEhsKE2NyZWRlbnRpYWxfcmV2aXNpb24YBiABKAQSDAoEcGxhbhgHIAEoCSKLAQoRT3BlcmF0aW9uYWxDb25maWcSPQoKcGFyYW1ldGVycxgBIAMoCzIpLmRhcmtib3QucHJvamVjdDAudjEuT3BlcmF0aW9uYWxQYXJhbWV0ZXISNwoJZWZmZWN0aXZlGAIgASgLMiQuZGFya2JvdC5wcm9qZWN0MC52MS5FZmZlY3RpdmVSZWNvcmRClAEKF2NvbS5kYXJrYm90LnByb2plY3QwLnYxQgtTeXN0ZW1Qcm90b1ABogIDRFBYqgITRGFya2JvdC5Qcm9qZWN0MC5WMcoCE0Rhcmtib3RcUHJvamVjdDBcVjHiAh9EYXJrYm90XFByb2plY3QwXFYxXEdQQk1ldGFkYXRh6gIVRGFya2JvdDo6UHJvamVjdDA6OlYxYgZwcm90bzM");
 
 /**
  * Process-level status of Project_0: STATE topic project0.status. Sent when a reporter session
@@ -273,6 +273,14 @@ export type EffectiveRecord = Message<"darkbot.project0.v1.EffectiveRecord"> & {
    * @generated from field: string source_geyser_provider = 21;
    */
   sourceGeyserProvider: string;
+
+  /**
+   * DEC-18: the providers the run is bound to (runtime_config::ProviderBinding), secret-free and
+   * without their endpoints; empty in a record of an earlier revision.
+   *
+   * @generated from field: repeated darkbot.project0.v1.RunProvider providers = 22;
+   */
+  providers: RunProvider[];
 };
 
 /**
@@ -282,6 +290,69 @@ export type EffectiveRecord = Message<"darkbot.project0.v1.EffectiveRecord"> & {
 export const EffectiveRecordSchema: GenMessage<EffectiveRecord> = /*@__PURE__*/
   messageDesc(file_darkbot_project0_v1_system, 2);
 
+/**
+ * DEC-18: one provider a run is bound to.
+ *
+ * @generated from message darkbot.project0.v1.RunProvider
+ */
+export type RunProvider = Message<"darkbot.project0.v1.RunProvider"> & {
+  /**
+   * rpc | wss | geyser | jupiter | jito.
+   *
+   * @generated from field: string kind = 1;
+   */
+  kind: string;
+
+  /**
+   * Provider id (local: a named loopback endpoint; jupiter-default: the contract's profile).
+   *
+   * @generated from field: string id = 2;
+   */
+  id: string;
+
+  /**
+   * Its secret-free binding (hex SHA-256).
+   *
+   * @generated from field: string binding = 3;
+   */
+  binding: string;
+
+  /**
+   * The registry version it was bound from; 0 for a local endpoint or a default profile.
+   *
+   * @generated from field: uint64 registry_version = 4;
+   */
+  registryVersion: bigint;
+
+  /**
+   * The credential id it authenticates with; empty without.
+   *
+   * @generated from field: string credential_id = 5;
+   */
+  credentialId: string;
+
+  /**
+   * The credential revision bound.
+   *
+   * @generated from field: uint64 credential_revision = 6;
+   */
+  credentialRevision: bigint;
+
+  /**
+   * free | paid | unknown.
+   *
+   * @generated from field: string plan = 7;
+   */
+  plan: string;
+};
+
+/**
+ * Describes the message darkbot.project0.v1.RunProvider.
+ * Use `create(RunProviderSchema)` to create a new message.
+ */
+export const RunProviderSchema: GenMessage<RunProvider> = /*@__PURE__*/
+  messageDesc(file_darkbot_project0_v1_system, 3);
+
 /**
  * Operational configuration of the process.
  *
@@ -308,4 +379,4 @@ export type OperationalConfig = Message<"darkbot.project0.v1.OperationalConfig">
  * Use `create(OperationalConfigSchema)` to create a new message.
  */
 export const OperationalConfigSchema: GenMessage<OperationalConfig> = /*@__PURE__*/
-  messageDesc(file_darkbot_project0_v1_system, 3);
+  messageDesc(file_darkbot_project0_v1_system, 4);
diff --git a/frontend/src/core/realtime/analyticsControl.test.ts b/frontend/src/core/realtime/analyticsControl.test.ts
index 35b90783bfda146f7b2f0a1d96c70f558e751f8f..bfb7f3d88f7d3e9e19e28f0a3c1bd1c10b4e0451 100644
--- a/frontend/src/core/realtime/analyticsControl.test.ts
+++ b/frontend/src/core/realtime/analyticsControl.test.ts
@@ -163,3 +163,55 @@ describe("DEC-16 unbounded boot and the standing autostart approval", () => {
     expect("problem" in autostartGrantCommand("")).toBe(true);
   });
 });
+
+describe("DEC-18 provider registry commands", () => {
+  beforeEach(() => useAnalyticsControl.setState({ busy: false, receipt: null }));
+  const input = { id: "helius-rpc", kind: "rpc", endpoint: "https://mainnet.helius-rpc.com", auth: "query:api-key", credential: "helius",
+    priority: "0", capacityRps: "10", plan: "free", entitlement: "helius-free", disabled: false };
+
+  it("types a provider, refusing a key in an endpoint or a name before anything is sent", async () => {
+    const { ProviderDescriptorCommandSchema, ProviderSelectCommandSchema } = await import("../generated/darkbot/project0/v1/analytics_pb");
+    const { providerDescriptorCommand, providerSelectCommand, providerPriorityCommand, providerQualifyCommand } = await import("./analyticsControl");
+    const ok = providerDescriptorCommand(input, true);
+    if (!("params" in ok)) throw new Error(ok.problem);
+    const c = fromBinary(ProviderDescriptorCommandSchema, ok.params);
+    expect([c.id, c.kind, c.endpoint, c.auth, c.credential, c.priority, c.capacityRps, c.plan, c.entitlement])
+      .toEqual(["helius-rpc", "rpc", "https://mainnet.helius-rpc.com", "query:api-key", "helius", 0, 10n, "free", "helius-free"]);
+    // An update sets only what it names.
+    const update = providerDescriptorCommand({ ...input, kind: "", endpoint: "", auth: "", credential: "", priority: "3", capacityRps: "", plan: "", entitlement: "" }, false);
+    if (!("params" in update)) throw new Error(update.problem);
+    const u = fromBinary(ProviderDescriptorCommandSchema, update.params);
+    expect([u.kind, u.endpoint, u.priority, u.capacityRps]).toEqual([undefined, undefined, 3, undefined]);
+    for (const bad of [{ endpoint: "https://mainnet.helius-rpc.com/?api-key=KEY" }, { endpoint: "https://user:KEY@rpc.example" }, { id: "0123456789abcdef0123" },
+      { credential: "0123456789abcdef0123456789abcdef" }, { auth: "header:" }, { plan: "paid" }, { kind: "ftp" }, { capacityRps: "0" }]) {
+      expect("problem" in providerDescriptorCommand({ ...input, ...bad }, true)).toBe(true);
+    }
+    expect("problem" in providerDescriptorCommand({ ...input, endpoint: "" }, true)).toBe(true);
+    const sel = providerSelectCommand("rpc", "helius-rpc");
+    if (!("params" in sel)) throw new Error(sel.problem);
+    expect(fromBinary(ProviderSelectCommandSchema, sel.params)).toMatchObject({ kind: "rpc", id: "helius-rpc" });
+    expect("problem" in providerSelectCommand("geyser", "publicnode")).toBe(true);
+    expect("problem" in providerPriorityCommand("helius-rpc", "x")).toBe(true);
+    expect("problem" in providerQualifyCommand("helius-rpc", "123")).toBe(true);
+    expect("params" in providerQualifyCommand("helius-rpc", "-")).toBe(true);
+  });
+
+  it("sends a credential's key once in the request with the registry version, and keeps it nowhere", async () => {
+    const { ProviderCredentialCommandSchema } = await import("../generated/darkbot/project0/v1/analytics_pb");
+    const { providerCredentialCommand } = await import("./analyticsControl");
+    const fetch = vi.fn().mockResolvedValueOnce(session()).mockResolvedValueOnce(accepted(CommandPhase.COMPLETED, "Provider change recorded: PROVIDER_RECEIPT applied=true"));
+    vi.stubGlobal("fetch", fetch);
+    const typed = providerCredentialCommand("helius", "KEY-ONE-SECRET");
+    if (!("params" in typed)) throw new Error(typed.problem);
+    await analyticsCommand("project0.provider.credential.set", { params: typed.params, configVersion: 7n });
+    const req = fromBinary(CommandRequestSchema, (fetch.mock.calls[1]?.[1] as RequestInit).body as Uint8Array);
+    const c = fromBinary(ProviderCredentialCommandSchema, req.params);
+    expect([req.action, c.credential, new TextDecoder().decode(c.secret)]).toEqual(["project0.provider.credential.set", "helius", "KEY-ONE-SECRET"]);
+    expect(req.expected?.version).toEqual({ case: "configVersion", value: 7n });
+    const state = JSON.stringify(useAnalyticsControl.getState(), (_, v) => (typeof v === "bigint" ? v.toString() : v));
+    expect(state).not.toContain("KEY-ONE-SECRET");
+    expect(useAnalyticsControl.getState().receipt).toMatchObject({ phase: "COMPLETED", action: "project0.provider.credential.set" });
+    for (const bad of ["", "two\nlines"]) expect("problem" in providerCredentialCommand("helius", bad)).toBe(true);
+    expect("problem" in providerCredentialCommand("0123456789abcdef0123", "k")).toBe(true);
+  });
+});
diff --git a/frontend/src/core/realtime/analyticsControl.ts b/frontend/src/core/realtime/analyticsControl.ts
index aed7915af246a6107bad558e25adf73de8c62a74..44cf5e8e78384531bafc5cc6eb45bf71f86b428c 100644
--- a/frontend/src/core/realtime/analyticsControl.ts
+++ b/frontend/src/core/realtime/analyticsControl.ts
@@ -2,7 +2,7 @@ import { create as message, fromBinary, toBinary } from "@bufbuild/protobuf";
 import { create } from "zustand";
 import { CommandLookupResponseSchema, CommandPhase, CommandRequestSchema, CommandResponseSchema, CommandStatus } from "../generated/darkbot/control/v1/commands_pb";
 import { ProjectId } from "../generated/darkbot/control/v1/topics_pb";
-import { AutostartGrantCommandSchema, AutostartPreviewCommandSchema, BacktestJobCommandSchema, BootNetworkCommandSchema, BootOfflineCommandSchema, BootStartCommandSchema, H0JobCommandSchema, OptimizeJobCommandSchema, PostCaptureJobCommandSchema } from "../generated/darkbot/project0/v1/analytics_pb";
+import { AutostartGrantCommandSchema, AutostartPreviewCommandSchema, BacktestJobCommandSchema, BootNetworkCommandSchema, BootOfflineCommandSchema, BootStartCommandSchema, H0JobCommandSchema, OptimizeJobCommandSchema, PostCaptureJobCommandSchema, ProviderCredentialCommandSchema, ProviderDescriptorCommandSchema, ProviderPriorityCommandSchema, ProviderQualifyCommandSchema, ProviderSelectCommandSchema } from "../generated/darkbot/project0/v1/analytics_pb";
 import { CapabilityControlCommandSchema, SourceControlCommandSchema } from "../generated/darkbot/project0/v1/owner_pb";
 
 /**
@@ -14,7 +14,13 @@ import { CapabilityControlCommandSchema, SourceControlCommandSchema } from "../g
 export type JobAction = "project0.analytics.backtest.start" | "project0.analytics.optimize.start" | "project0.analytics.h0.start" | "project0.analytics.job.cancel"
   | "project0.capture.verify.start" | "project0.source.set" | "project0.capability.set"
   | "project0.boot.offline" | "project0.boot.network" | "project0.boot.start" | "project0.boot.cancel"
-  | "project0.autostart.preview" | "project0.autostart.grant" | "project0.autostart.revoke";
+  | "project0.autostart.preview" | "project0.autostart.grant" | "project0.autostart.revoke"
+  | ProviderAction;
+/** DEC-18: the provider registry's changes (`p0ctl provider …`), effective at the next controlled start. */
+export type ProviderAction = "project0.provider.add" | "project0.provider.update" | "project0.provider.remove"
+  | "project0.provider.enable" | "project0.provider.disable" | "project0.provider.priority" | "project0.provider.select"
+  | "project0.provider.unselect" | "project0.provider.qualify" | "project0.provider.credential.set"
+  | "project0.provider.credential.rotate" | "project0.provider.credential.revoke";
 interface Receipt { id: string; action: JobAction; phase: string; detail: string }
 interface AnalyticsControlState { busy: boolean; receipt: Receipt | null }
 export const useAnalyticsControl = create<AnalyticsControlState>(() => ({ busy: false, receipt: null }));
@@ -177,6 +183,66 @@ export function autostartGrantCommand(digest: string): { params: Uint8Array } |
   return { params: toBinary(AutostartGrantCommandSchema, message(AutostartGrantCommandSchema, { confirmDigest: digest })) };
 }
 
+const PROVIDER_ID = /^(?![0-9a-fA-F]{16,}$)[a-z0-9_-]{1,32}$/;
+const PROVIDER_KINDS = ["rpc", "wss", "geyser", "jupiter", "jito"] as const;
+/** The provider form as the operator entered it (empty: not set; an update changes only what it sets). */
+export interface ProviderInput {
+  id: string; kind: string; endpoint: string; auth: string; credential: string;
+  priority: string; capacityRps: string; plan: string; entitlement: string; disabled: boolean;
+}
+
+/** DEC-18: an add or an update of a provider. A key never belongs in an endpoint (its credential
+ *  is set separately): an endpoint with user info, a query or a fragment is refused here as by the
+ *  gateway and the native registry, which decide every rule. */
+export function providerDescriptorCommand(v: ProviderInput, add: boolean): { params: Uint8Array } | { problem: string } {
+  const t = (s: string) => s.trim();
+  if (!PROVIDER_ID.test(t(v.id))) return { problem: "Id: 1–32 of a-z, 0-9, _ or - (never a key)." };
+  if (add && (!t(v.kind) || !t(v.endpoint) || !t(v.plan) || !t(v.entitlement))) return { problem: "A new provider needs its kind, endpoint, plan and entitlement." };
+  if (t(v.kind) && !(PROVIDER_KINDS as readonly string[]).includes(t(v.kind))) return { problem: "Kind: rpc, wss, geyser, jupiter or jito." };
+  if (t(v.endpoint) && (!/^(?:https?|wss?):\/\/[^/@?#\s]+(?:\/[^?#\s]*)?$/.test(t(v.endpoint)))) return { problem: "Endpoint: a URL without user info, query or fragment — a key is the credential's, never the endpoint's." };
+  if (t(v.auth) && !/^(?:none|path|(?:query|header):[A-Za-z0-9_-]{1,64})$/.test(t(v.auth))) return { problem: "Authentication: none, path, query:NAME or header:NAME (a name, never a key)." };
+  if (t(v.credential) && !PROVIDER_ID.test(t(v.credential))) return { problem: "Credential: its id (never the key itself)." };
+  if (t(v.plan) && !["free", "unknown"].includes(t(v.plan))) return { problem: "Plan: free or unknown (FREE_ONLY)." };
+  if (t(v.priority) && !/^\d{1,9}$/.test(t(v.priority))) return { problem: "Priority: a whole number." };
+  if (t(v.capacityRps) && !/^[1-9]\d{0,8}$/.test(t(v.capacityRps))) return { problem: "Capacity: requests per second, at least 1." };
+  if (!add && v.disabled) return { problem: "Disabling is its own request." };
+  const opt = (s: string) => (t(s) ? t(s) : undefined);
+  return {
+    params: toBinary(ProviderDescriptorCommandSchema, message(ProviderDescriptorCommandSchema, {
+      id: t(v.id), kind: opt(v.kind), endpoint: opt(v.endpoint), auth: opt(v.auth), credential: opt(v.credential),
+      priority: t(v.priority) ? Number(t(v.priority)) : undefined, capacityRps: t(v.capacityRps) ? BigInt(t(v.capacityRps)) : undefined,
+      plan: opt(v.plan), entitlement: opt(v.entitlement), disabled: add && v.disabled,
+    })),
+  };
+}
+
+/** DEC-18: a provider's priority among its kind. */
+export function providerPriorityCommand(id: string, priority: string): { params: Uint8Array } | { problem: string } {
+  if (!PROVIDER_ID.test(id) || !/^\d{1,9}$/.test(priority.trim())) return { problem: "Priority: a whole number." };
+  return { params: toBinary(ProviderPriorityCommandSchema, message(ProviderPriorityCommandSchema, { id, priority: Number(priority.trim()) })) };
+}
+
+/** DEC-18: the selected provider of a kind (the Geyser source unit names its own). */
+export function providerSelectCommand(kind: string, id: string): { params: Uint8Array } | { problem: string } {
+  if (!["rpc", "wss", "jupiter", "jito"].includes(kind) || !PROVIDER_ID.test(id)) return { problem: "Select a provider of rpc, wss, jupiter or jito." };
+  return { params: toBinary(ProviderSelectCommandSchema, message(ProviderSelectCommandSchema, { kind, id })) };
+}
+
+/** DEC-18: a provider's qualification by a stored provider evidence record (or "-" to clear it). */
+export function providerQualifyCommand(id: string, evidence: string): { params: Uint8Array } | { problem: string } {
+  if (!PROVIDER_ID.test(id) || !(evidence === "-" || /^[0-9a-f]{16}$/.test(evidence.trim()))) return { problem: "Evidence: a record id (16 hex digits) or - to clear." };
+  return { params: toBinary(ProviderQualifyCommandSchema, message(ProviderQualifyCommandSchema, { id, evidence: evidence.trim() })) };
+}
+
+/** DEC-18 (P0-019): a credential's secret, set or rotated — carried in the request's parameters
+ *  only (the gateway separates it before any record and hands it to the native controller's
+ *  standard input); never kept in a store, the receipt or the browser's storage. */
+export function providerCredentialCommand(credential: string, secret: string): { params: Uint8Array } | { problem: string } {
+  if (!PROVIDER_ID.test(credential)) return { problem: "Credential: its id." };
+  if (secret === "" || /[\u0000-\u001f\u007f]/.test(secret)) return { problem: "Key: the provider's key, one line." };
+  return { params: toBinary(ProviderCredentialCommandSchema, message(ProviderCredentialCommandSchema, { credential, secret: new TextEncoder().encode(secret) })) };
+}
+
 /** The capabilities of mode OFF|OBSERVE|ACTIVE (`capabilityCommand`). */
 export type ModeCapability = "HOLD_B" | "JITO_BUNDLE" | "THRESHOLD_TRIGGER" | "INVENTORY_REBALANCING" | "LOCAL_FEE_MODEL";
 
diff --git a/frontend/src/features/project0/Project0Page.tsx b/frontend/src/features/project0/Project0Page.tsx
index 2692974aaa4e61730ba1035a55152a7c3fcce46a..aceb2bcf0e68ca823fe8d999b2adbfd37f5f336a 100644
--- a/frontend/src/features/project0/Project0Page.tsx
+++ b/frontend/src/features/project0/Project0Page.tsx
@@ -69,7 +69,7 @@ export default function Project0Page() {
     <TabStrip label="Project 0 areas" options={TABS} value={tab} onChange={setTab}/>
     {healthOpen ? <HealthSection/> : null}
     {tab === "overview" ? <OverviewPanel monitor={monitor} financeStatus={finance.topicStatus} capture={capture.value} topicStatus={capture.topicStatus} authorization={authorization} analytics={analytics.value} analyticsStatus={analytics.topicStatus} goto={setTab}/> : null}
-    {tab === "liquidator" ? <LiquidatorPanel fleet={fleet} authorization={authorization} goto={setTab} owner={liquidator.value} ownerStatus={liquidator.topicStatus} finance={liveFinance} system={system.value}/> : null}
+    {tab === "liquidator" ? <LiquidatorPanel fleet={fleet} authorization={authorization} goto={setTab} owner={liquidator.value} ownerStatus={liquidator.topicStatus} finance={liveFinance} system={system.value} analytics={analytics.value} analyticsStatus={analytics.topicStatus}/> : null}
     {tab === "capture" ? <CapturePanel capture={capture.value} topicStatus={capture.topicStatus} analytics={analytics.value} analyticsStatus={analytics.topicStatus} owner={liquidator.value} ownerStatus={liquidator.topicStatus} system={system.value}/> : null}
     {tab === "analytics" ? <AnalyticsPanel analytics={analytics.value} topicStatus={analytics.topicStatus} createDraft={fleet.create} goto={setTab}/> : null}
     {tab === "wallets" ? <><WalletsPanel finance={liveFinance}/><SignerSection owner={liquidator.value} topicStatus={liquidator.topicStatus}/><NonLandingSection wallets={wallets.value} topicStatus={wallets.topicStatus}/><HoldsSection wallets={wallets.value} topicStatus={wallets.topicStatus}/></> : null}
diff --git a/frontend/src/features/project0/api/analyticsAdapter.test.ts b/frontend/src/features/project0/api/analyticsAdapter.test.ts
index 33f40861aff1b1a10f6cbad183b64224a7296b67..679226c6ea3c0127afc8bddd71106013f0a79285 100644
--- a/frontend/src/features/project0/api/analyticsAdapter.test.ts
+++ b/frontend/src/features/project0/api/analyticsAdapter.test.ts
@@ -154,6 +154,25 @@ describe("project0.analytics.jobs (golden frame written by the Rust reporter)",
     expect(untilStoppedStopAction(flow("econ4h", "RUNNING"), null)).toBeNull();
   });
 
+  it("DEC-18: maps the provider registry secret-free (hosts, names, revisions; an unset optional is null)", () => {
+    const r = view.providers!;
+    expect(view.providersError).toBe("");
+    expect(r.version).toBe(7n);
+    expect(r.restartRule).toBe("next_controlled_start");
+    expect(r.providers).toEqual([
+      { id: "helius-rpc", kind: "rpc", host: "mainnet.helius-rpc.com", auth: "query:api-key", credential: "helius", priority: 0,
+        capacityRps: 10n, plan: "free", entitlement: "helius-free", qualification: "0123456789abcdef", enabled: true, selected: true,
+        binding: "b".repeat(64), selectionBlock: null },
+      { id: "public-wss", kind: "wss", host: "api.mainnet-beta.solana.com", auth: "none", credential: "", priority: 5,
+        capacityRps: null, plan: "free", entitlement: "public", qualification: null, enabled: false, selected: false,
+        binding: "c".repeat(64), selectionBlock: "PROVIDER_DISABLED: public-wss" },
+    ]);
+    expect(r.credentials).toEqual([
+      { id: "helius", revision: 3n, state: "active", origin: "input", createdUnixS: 1_760_000_000n },
+      { id: "old", revision: 1n, state: "revoked", origin: "env:HELIUS_API_KEY", createdUnixS: 1_750_000_000n },
+    ]);
+  });
+
   it("rejects malformed wire bytes", () => {
     expect(() => decodeAnalyticsJobs(Uint8Array.from([0xff, 0xff, 0xff]))).toThrow();
   });
diff --git a/frontend/src/features/project0/api/analyticsAdapter.ts b/frontend/src/features/project0/api/analyticsAdapter.ts
index d43d52895690c7512275128a825694c7da2fc2e7..ccadc9dd5a94bdf1f8ebf368f84a7b34b0b832b6 100644
--- a/frontend/src/features/project0/api/analyticsAdapter.ts
+++ b/frontend/src/features/project0/api/analyticsAdapter.ts
@@ -8,8 +8,22 @@ import {
   type JobParams,
   type ShadowCounterfactual,
 } from "@/core/generated/darkbot/project0/v1/analytics_pb";
-import type { UntilStopped } from "@/core/generated/darkbot/project0/v1/analytics_pb";
-import type { AutostartGrantView, BootFlowView, BootStepView, CounterfactualView, JobParamsView, AnalyticsView, UntilStoppedView } from "../model/analytics";
+import type { ProviderRegistryView as ProviderRegistryWire, UntilStopped } from "@/core/generated/darkbot/project0/v1/analytics_pb";
+import type { AutostartGrantView, BootFlowView, BootStepView, CounterfactualView, JobParamsView, AnalyticsView, ProviderRegistryView, UntilStoppedView } from "../model/analytics";
+
+/** DEC-18: the provider registry as its store holds it (secret-free; an unset optional is null). */
+function providers(r: ProviderRegistryWire): ProviderRegistryView {
+  return {
+    version: r.version,
+    restartRule: r.restartRule,
+    providers: r.providers.map((p) => ({
+      id: p.id, kind: p.kind, host: p.host, auth: p.auth, credential: p.credential, priority: p.priority,
+      capacityRps: p.capacityRps ?? null, plan: p.plan, entitlement: p.entitlement, qualification: p.qualification ?? null,
+      enabled: p.enabled, selected: p.selected, binding: p.binding ?? null, selectionBlock: p.selectionBlock ?? null,
+    })),
+    credentials: r.credentials.map((c) => ({ id: c.id, revision: c.revision, state: c.state, origin: c.origin, createdUnixS: c.createdUnixS })),
+  };
+}
 
 /** Throws on invalid bytes, so the registry never acknowledges a broken state. */
 export function decodeAnalyticsJobs(bytes: Uint8Array): AnalyticsJobs {
@@ -142,6 +156,8 @@ export function toAnalyticsView(m: AnalyticsJobs): AnalyticsView {
     boot: m.boot ? boot(m.boot) : null,
     bootError: m.bootError,
     untilStopped: untilStopped(m.untilStopped),
+    providers: m.providers ? providers(m.providers) : null,
+    providersError: m.providersError,
   };
 }
 
diff --git a/frontend/src/features/project0/api/project0ViewAdapter.ts b/frontend/src/features/project0/api/project0ViewAdapter.ts
index a85dd600daeef034cbceb002caf264bc3296bc6c..958e23cd3280085f89ff72b8ea4aa241dd6ffbca 100644
--- a/frontend/src/features/project0/api/project0ViewAdapter.ts
+++ b/frontend/src/features/project0/api/project0ViewAdapter.ts
@@ -152,6 +152,10 @@ export function toSystemView(m: SystemMessage): SystemView {
     ] : []),
     liveAuthorized: effective ? effective.liveAuthorized : null,
     submissionAuthorized: effective ? effective.submissionAuthorized : null,
+    runProviders: (effective?.providers ?? []).map((p) => ({
+      kind: p.kind, id: p.id, binding: p.binding, registryVersion: p.registryVersion,
+      credentialId: p.credentialId, credentialRevision: p.credentialRevision, plan: p.plan,
+    })),
   };
 }
 
diff --git a/frontend/src/features/project0/components/ProvidersSection.tsx b/frontend/src/features/project0/components/ProvidersSection.tsx
new file mode 100644
index 0000000000000000000000000000000000000000..f5514399caebb582b6da87bbb38f1d5d857f5e26
--- /dev/null
+++ b/frontend/src/features/project0/components/ProvidersSection.tsx
@@ -0,0 +1,113 @@
+import { useState } from "react";
+import type { TopicStatus } from "@/core/realtime";
+import {
+  analyticsCommand, providerCredentialCommand, providerDescriptorCommand, providerPriorityCommand,
+  providerQualifyCommand, providerSelectCommand, useAnalyticsControl, type ProviderAction, type ProviderInput,
+} from "@/core/realtime/analyticsControl";
+import type { AnalyticsView, ProviderKind } from "../model/analytics";
+import type { SystemView } from "../model/status";
+import { DetailRow, Notice, Section } from "./PanelPrimitives";
+
+const EMPTY: ProviderInput = { id: "", kind: "", endpoint: "", auth: "", credential: "", priority: "", capacityRps: "", plan: "", entitlement: "", disabled: false };
+
+/**
+ * DEC-18 (P0-020): the provider registry of this tab's kinds (Capture: RPC, WSS, Geyser;
+ * Liquidator: Jupiter, Jito) as the native store holds it — host, authentication name, credential
+ * revision, plan, priority, capacity, binding, selection and why a provider cannot be selected —
+ * beside the providers the running process is bound to (effective). Every change goes through the
+ * same controller as `p0ctl provider …` with the registry version this view showed (CAS) and is
+ * staged: effective at the next controlled start. A credential's key is typed here, sent once in
+ * the command and never kept (the gateway hands it to the controller's standard input; the
+ * macOS keychain holds it).
+ */
+export function ProvidersSection({ title, kinds, analytics, topicStatus, system }: {
+  title: string; kinds: readonly ProviderKind[]; analytics: AnalyticsView | null; topicStatus: TopicStatus; system: SystemView | null;
+}) {
+  const { busy, receipt } = useAnalyticsControl();
+  const registry = analytics?.providers ?? null;
+  const providers = (registry?.providers ?? []).filter(p => (kinds as readonly string[]).includes(p.kind));
+  const used = new Set(providers.map(p => p.credential).filter(Boolean));
+  const credentials = (registry?.credentials ?? []).filter(c => used.has(c.id));
+  const running = (system?.runProviders ?? []).filter(p => (kinds as readonly string[]).includes(p.kind));
+  const [form, setForm] = useState<ProviderInput>({ ...EMPTY, kind: kinds[0] ?? "" });
+  const [target, setTarget] = useState(""); const [priority, setPriority] = useState(""); const [evidence, setEvidence] = useState("");
+  const [credential, setCredential] = useState(""); const [key, setKey] = useState(""); const [msg, setMsg] = useState("");
+  const live = topicStatus === "LIVE";
+  const blocked = busy ? "Command in progress" : !live || !registry ? `Provider registry not current (${topicStatus})` : "";
+  const chosen = providers.find(p => p.id === target) ?? providers[0] ?? null;
+  const send = (action: ProviderAction, typed: { params: Uint8Array } | { problem: string } | null, resourceId?: string) => {
+    if (typed && "problem" in typed) { setMsg(typed.problem); return; }
+    setMsg("");
+    // The change states the registry version this view showed; a newer one is a conflict.
+    void analyticsCommand(action, { params: typed?.params, resourceId, configVersion: registry?.version });
+  };
+  const field = (name: keyof ProviderInput, label: string, placeholder = "") =>
+    <label>{label}<input value={String(form[name])} placeholder={placeholder} autoComplete="off" spellCheck={false}
+      onChange={e => setForm({ ...form, [name]: e.target.value })}/></label>;
+  const setKeyed = (action: "project0.provider.credential.set" | "project0.provider.credential.rotate") => {
+    const typed = providerCredentialCommand(credential.trim(), key);
+    setKey(""); // the key is never kept beyond this one command
+    send(action, typed);
+  };
+  return <Section title={title} aside={<span className="p0-badge">{registry ? `registry v${registry.version} · ${topicStatus}` : topicStatus}</span>}>
+    {analytics?.providersError ? <Notice>Provider registry unreadable: {analytics.providersError}</Notice> : null}
+    {!registry ? <p className="p0-quiet">No provider registry received.</p> : null}
+    {registry && !live ? <p className="p0-quiet">Last reported registry ({topicStatus}); not current.</p> : null}
+    {providers.map(p => <DetailRow key={p.id} title={`${p.kind} · ${p.id}${p.selected ? " · selected" : ""}${p.enabled ? "" : " · disabled"}`}
+      value={`${p.host} · ${p.auth}${p.credential ? ` · credential ${p.credential}` : ""} · ${p.plan} (${p.entitlement}) · priority ${p.priority}`
+        + `${p.capacityRps != null ? ` · ${p.capacityRps} rps` : ""} · binding ${p.binding ? p.binding.slice(0, 16) : "— (credential revoked or unknown)"}`
+        + `${p.qualification ? ` · qualified ${p.qualification}` : " · not qualified"}${p.selectionBlock ? ` · ${p.selectionBlock}` : ""}`}/>)}
+    {registry && !providers.length ? <p className="p0-quiet">No {kinds.join("/")} provider in the registry.</p> : null}
+    {credentials.map(c => <DetailRow key={c.id} title={`Credential · ${c.id}`} value={`revision ${c.revision} · ${c.state} · ${c.origin}`}/>)}
+    <DetailRow title="Running process · effective" value={running.length
+      ? running.map(p => `${p.kind} ${p.id} · ${p.binding.slice(0, 16)}${p.credentialId ? ` · ${p.credentialId}@${p.credentialRevision}` : ""}`).join(" | ")
+      : "none reported"}/>
+    {registry ? <p className="p0-quiet">A change is staged: effective at the next controlled start ({registry.restartRule}); a running process keeps its providers.</p> : null}
+    {providers.length ? <>
+      <div className="p0-formgrid">
+        <label>Provider<select value={chosen?.id ?? ""} onChange={e => setTarget(e.target.value)}>{providers.map(p => <option key={p.id} value={p.id}>{p.kind} · {p.id}</option>)}</select></label>
+        <label>New priority<input value={priority} onChange={e => setPriority(e.target.value)} inputMode="numeric"/></label>
+        <label>Evidence record<input value={evidence} placeholder="16 hex digits, or -" onChange={e => setEvidence(e.target.value)}/></label>
+      </div>
+      <div className="p0-actions">
+        {chosen && chosen.kind !== "geyser" ? <button className="p0-secondary" disabled={blocked !== "" || chosen.selected} title={blocked || undefined}
+          onClick={() => send("project0.provider.select", providerSelectCommand(chosen.kind, chosen.id))}>Select</button> : null}
+        {chosen?.selected && chosen.kind !== "geyser" ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.unselect", null, chosen.kind)}>Unselect</button> : null}
+        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send(chosen.enabled ? "project0.provider.disable" : "project0.provider.enable", null, chosen.id)}>{chosen.enabled ? "Disable" : "Enable"}</button> : null}
+        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.priority", providerPriorityCommand(chosen.id, priority))}>Set priority</button> : null}
+        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.qualify", providerQualifyCommand(chosen.id, evidence))}>Qualify</button> : null}
+        {chosen ? <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.remove", null, chosen.id)}>Remove</button> : null}
+      </div>
+    </> : null}
+    <p className="p0-label">Add or update a provider</p>
+    <div className="p0-formgrid">
+      {field("id", "Id")}
+      <label>Kind<select value={form.kind} onChange={e => setForm({ ...form, kind: e.target.value })}>{kinds.map(k => <option key={k} value={k}>{k}</option>)}</select></label>
+      {field("endpoint", "Endpoint", "https://… (no key in it)")}
+      {field("auth", "Authentication", "none | query:NAME | header:NAME | path")}
+      {field("credential", "Authenticates with")}
+      <label>Plan<select value={form.plan} onChange={e => setForm({ ...form, plan: e.target.value })}><option value="">—</option><option value="free">Free</option><option value="unknown">Unknown (never selected)</option></select></label>
+      {field("entitlement", "Plan name", "e.g. helius-free")}
+      {field("priority", "Priority")}
+      {field("capacityRps", "Capacity · rps")}
+      <label className="p0-check"><input type="checkbox" checked={form.disabled} onChange={e => setForm({ ...form, disabled: e.target.checked })}/>Added disabled</label>
+    </div>
+    <div className="p0-actions">
+      <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => send("project0.provider.add", providerDescriptorCommand(form, true))}>Add</button>
+      <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => send("project0.provider.update", providerDescriptorCommand(form, false))}>Update</button>
+    </div>
+    <p className="p0-label">Credential · macOS keychain</p>
+    <div className="p0-formgrid">
+      <label>Credential<input value={credential} autoComplete="off" spellCheck={false} onChange={e => setCredential(e.target.value)}/></label>
+      <label>Key<input type="password" value={key} autoComplete="off" spellCheck={false} data-testid="p0-provider-key" onChange={e => setKey(e.target.value)}/></label>
+    </div>
+    <div className="p0-actions">
+      <button className="p0-secondary" disabled={blocked !== ""} onClick={() => setKeyed("project0.provider.credential.set")}>Set key</button>
+      <button className="p0-secondary" disabled={blocked !== ""} onClick={() => setKeyed("project0.provider.credential.rotate")}>Rotate key</button>
+      <button className="p0-secondary" disabled={blocked !== ""} onClick={() => send("project0.provider.credential.revoke", null, credential.trim())}>Revoke</button>
+    </div>
+    {blocked ? <p className="p0-quiet">{blocked}</p> : null}
+    {msg ? <Notice>{msg}</Notice> : null}
+    {receipt?.action.startsWith("project0.provider.") ? <Notice><span data-testid="p0-provider-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""}</span></Notice> : null}
+  </Section>;
+}
diff --git a/frontend/src/features/project0/model/analytics.ts b/frontend/src/features/project0/model/analytics.ts
index 711b1aaa8c2838354ac4f6e33098685f2fa01dce..4f29e96fef88132f786db419fd9abcf416ccab1f 100644
--- a/frontend/src/features/project0/model/analytics.ts
+++ b/frontend/src/features/project0/model/analytics.ts
@@ -164,6 +164,52 @@ export interface AnalyticsView {
   bootError: string;
   /** DEC-16: the standing autostart approval, the newest raw-capture qualification and run. */
   untilStopped: UntilStoppedView;
+  /** DEC-18: the provider registry (staged: effective at the next controlled start); null when
+   *  unreadable (providersError says why). */
+  providers: ProviderRegistryView | null;
+  /** DEC-18: why the provider registry was not readable; empty when read. */
+  providersError: string;
+}
+
+/** DEC-18: the provider kinds. */
+export type ProviderKind = "rpc" | "wss" | "geyser" | "jupiter" | "jito";
+
+/** DEC-18: one committed registry version — secret-free (hosts, names, credential revisions). */
+export interface ProviderRegistryView {
+  /** The version every change states (CAS). */
+  version: bigint;
+  providers: readonly RegistryProviderView[];
+  credentials: readonly RegistryCredentialView[];
+  /** When a change takes effect (next_controlled_start). */
+  restartRule: string;
+}
+
+/** DEC-18: one provider of the registry. */
+export interface RegistryProviderView {
+  id: string;
+  kind: string;
+  host: string;
+  /** none | query:NAME | header:NAME | path. */
+  auth: string;
+  credential: string;
+  priority: number;
+  capacityRps: bigint | null;
+  plan: string;
+  entitlement: string;
+  qualification: string | null;
+  enabled: boolean;
+  selected: boolean;
+  binding: string | null;
+  selectionBlock: string | null;
+}
+
+/** DEC-18: a credential's newest revision (its secret is the macOS keychain's alone). */
+export interface RegistryCredentialView {
+  id: string;
+  revision: bigint;
+  state: string;
+  origin: string;
+  createdUnixS: bigint;
 }
 
 /** DEC-16: a standing autostart approval (in force or previewed) and the binding it was given for. */
diff --git a/frontend/src/features/project0/model/status.ts b/frontend/src/features/project0/model/status.ts
index dd7fe496e20c28ac2ec22e2b3df78d1fbb8635fd..c7d30e9707d067cb586800b431343c4fd006fd35 100644
--- a/frontend/src/features/project0/model/status.ts
+++ b/frontend/src/features/project0/model/status.ts
@@ -132,6 +132,22 @@ export interface SystemView {
   settings: ReadonlyMap<string, { value: string; source: string }>;
   liveAuthorized: boolean | null;
   submissionAuthorized: boolean | null;
+  /** DEC-18: the providers the running process is bound to (effective; empty without an
+   *  effective record or before PROVIDERS_REVISION). */
+  runProviders: readonly RunProviderView[];
+}
+
+/** DEC-18: one provider a running process is bound to — its secret-free binding, no endpoint. */
+export interface RunProviderView {
+  kind: string;
+  /** local: a named loopback endpoint; jupiter-default: the contract's profile. */
+  id: string;
+  binding: string;
+  /** 0: a local endpoint or the default profile. */
+  registryVersion: bigint;
+  credentialId: string;
+  credentialRevision: bigint;
+  plan: string;
 }
 
 /** Engine counter names the GUI reads (MarketStats::counters). The contract test checks each
diff --git a/frontend/src/features/project0/model/statusSelectors.test.ts b/frontend/src/features/project0/model/statusSelectors.test.ts
index 458050aea4fb81adddcdcc109bfd9e8641da0def..4677d830ffbd894ae612e8551cc8e1d615367ceb 100644
--- a/frontend/src/features/project0/model/statusSelectors.test.ts
+++ b/frontend/src/features/project0/model/statusSelectors.test.ts
@@ -40,7 +40,7 @@ describe("healthLabel", () => {
 
 describe("execution authorization is the engine's statement, never a fixed GUI claim", () => {
   const system = (liveAuthorized: boolean | null, submissionAuthorized: boolean | null): SystemView =>
-    ({ activated: liveAuthorized != null, profile: null, profileSource: null, parameterCount: 1, parameters: new Map(), settings: new Map(), liveAuthorized, submissionAuthorized });
+    ({ activated: liveAuthorized != null, profile: null, profileSource: null, parameterCount: 1, parameters: new Map(), settings: new Map(), liveAuthorized, submissionAuthorized, runProviders: [] });
   it("is unknown without a live effective record", () => {
     expect(executionAuthorization(null, "LIVE")).toBe("Authorization unknown");
     expect(executionAuthorization(system(null, null), "LIVE")).toBe("Authorization unknown");
diff --git a/frontend/src/features/project0/pages/CapturePanel.tsx b/frontend/src/features/project0/pages/CapturePanel.tsx
index c44e0d637f12a46ff3bb439089d7e2ce3660572d..532a46bc8e8a73c1ad3c1ecde021fa31c34e2a1c 100644
--- a/frontend/src/features/project0/pages/CapturePanel.tsx
+++ b/frontend/src/features/project0/pages/CapturePanel.tsx
@@ -4,6 +4,7 @@ import type { CaptureMode, CaptureSource, VerificationMode, ReportMarker } from
 import type { CaptureView } from "../model/status";
 import type { AnalyticsView } from "../model/analytics";
 import { RunsSection } from "../components/RunsSection";
+import { ProvidersSection } from "../components/ProvidersSection";
 import { SourcesSection } from "../components/SourcesSection";
 import { BootSection } from "../components/BootSection";
 import type { SystemView } from "../model/status";
@@ -57,6 +58,7 @@ export function CapturePanel({ capture, topicStatus, analytics, analyticsStatus,
   <RunsSection analytics={analytics} topicStatus={analyticsStatus}/>
   <BootSection analytics={analytics} topicStatus={analyticsStatus} ttl={system?.parameters.get("boot_token_ttl_seconds") ?? null}/>
   <SourcesSection owner={owner} topicStatus={ownerStatus}/>
+  <ProvidersSection title="Providers · RPC, WSS, Geyser" kinds={["rpc", "wss", "geyser"]} analytics={analytics} topicStatus={analyticsStatus} system={system}/>
   <Section title="Raw data capture"><DetailRow title="Duration" value="Presets are optional"/>
     <div className="p0-pills">
       {PRESETS.map(d => <button key={d} className={preset === d ? "p0-selected" : ""} onClick={() => setPreset(d)}>{d === "custom" ? "Custom" : d}</button>)}
diff --git a/frontend/src/features/project0/pages/LiquidatorPanel.tsx b/frontend/src/features/project0/pages/LiquidatorPanel.tsx
index de5194b751f9bb707059261790c865083f7b0c03..97bce0331e925396e1d572b86facb4755766a06a 100644
--- a/frontend/src/features/project0/pages/LiquidatorPanel.tsx
+++ b/frontend/src/features/project0/pages/LiquidatorPanel.tsx
@@ -4,13 +4,15 @@ import type { FinanceSnapshot, LiquidatorDraft, Project0Tab, StrategyKind } from
 import type { LiquidatorView } from "../model/owner";
 import { usdMicroDisplay } from "../model/format";
 import { OwnerSection } from "../components/OwnerSection";
+import { ProvidersSection } from "../components/ProvidersSection";
+import type { AnalyticsView } from "../model/analytics";
 import type { SystemView } from "../model/status";
 import { validateDraft } from "../hooks/useFleetDrafts";
 import { DetailRow, Section, Notice } from "../components/PanelPrimitives";
 type Fleet={drafts:LiquidatorDraft[];selected:LiquidatorDraft|null;selectedId:string|null;
  select:(id:string|null)=>void;update:(id:string,patch:Partial<LiquidatorDraft>)=>void;
  create:()=>void;duplicate:()=>void;remove:()=>void};
-export function LiquidatorPanel({fleet,authorization,goto,owner,ownerStatus,finance,system}:{fleet:Fleet;authorization:string;goto:(t:Project0Tab)=>void;owner:LiquidatorView|null;ownerStatus:TopicStatus;finance:FinanceSnapshot|null;system:SystemView|null}) {
+export function LiquidatorPanel({fleet,authorization,goto,owner,ownerStatus,finance,system,analytics,analyticsStatus}:{fleet:Fleet;authorization:string;goto:(t:Project0Tab)=>void;owner:LiquidatorView|null;ownerStatus:TopicStatus;finance:FinanceSnapshot|null;system:SystemView|null;analytics:AnalyticsView|null;analyticsStatus:TopicStatus}) {
  const [message,setMessage]=useState("");const d=fleet.selected;
  const patch=(p:Partial<LiquidatorDraft>)=>{if(d)fleet.update(d.id,p)};
  const toggle=(k:StrategyKind,v:boolean)=>{if(d)patch({enabledStrategies:v?Array.from(new Set([...d.enabledStrategies,k])):d.enabledStrategies.filter(x=>x!==k)})};
@@ -20,6 +22,7 @@ export function LiquidatorPanel({fleet,authorization,goto,owner,ownerStatus,fina
  <DetailRow title="Available working capital" value={usdMicroDisplay(finance?.availableCapitalUsdMicro)}/>
  <DetailRow title="Realized net PnL" value={`${usdMicroDisplay(finance?.realizedNetPnlUsdMicro)} · ${finance?.status??"UNKNOWN"}`}/></Section>
  <OwnerSection owner={owner} topicStatus={ownerStatus} system={system}/>
+ <ProvidersSection title="Providers · Jupiter, Jito" kinds={["jupiter","jito"]} analytics={analytics} topicStatus={analyticsStatus} system={system}/>
  <Section title="Bots & strategies" aside={<button className="p0-secondary" onClick={()=>fleet.create()}>+ Add bot</button>}>
  <div className="p0-fleet"><div><p className="p0-quiet">Liquidators · {fleet.drafts.length} drafts</p>
  <div role="listbox" aria-label="Bot drafts">{fleet.drafts.map(x=><button key={x.id} role="option" aria-selected={x.id===fleet.selectedId}
diff --git a/frontend/tests/project0-status.spec.ts b/frontend/tests/project0-status.spec.ts
index 16444d4287692ece2ce192b29d072a74e0f8270e..ad46e62db05d2f0e0ee0c5b3b442d70338fef3be 100644
--- a/frontend/tests/project0-status.spec.ts
+++ b/frontend/tests/project0-status.spec.ts
@@ -5,7 +5,7 @@ import { CapturePhase, CaptureStateSchema } from "../src/core/generated/darkbot/
 import { EffectiveRecordSchema, SystemStatusSchema } from "../src/core/generated/darkbot/project0/v1/system_pb";
 import {
   AnalyticsJobsSchema, AutostartGrantCommandSchema, AutostartPreviewCommandSchema, BacktestJobCommandSchema, BootOfflineCommandSchema, BootStartCommandSchema, H0JobCommandSchema,
-  PostCaptureJobCommandSchema, type AnalyticsJobs,
+  PostCaptureJobCommandSchema, ProviderCredentialCommandSchema, ProviderDescriptorCommandSchema, type AnalyticsJobs,
 } from "../src/core/generated/darkbot/project0/v1/analytics_pb";
 import { CapabilityControlCommandSchema, SourceControlCommandSchema } from "../src/core/generated/darkbot/project0/v1/owner_pb";
 import { AckStatus, ClientMessageSchema, ServerMessageSchema, StateQuality, WelcomeStatus } from "../src/core/generated/darkbot/control/v1/stream_pb";
@@ -691,3 +691,65 @@ test("DEC-16: a complete qualification unlocks Until stopped — the offline pre
   expect([sent[1]!.action, p.qualification, p.checkpoint, p.minimumRunway, p.maxRuntime, p.validFor, p.captureSource])
     .toEqual(["project0.autostart.preview", "0123456789abcdef", "1h", "1d", "7d", "12h", "logs"]);
 });
+
+test("DEC-18: the provider registry reaches Capture and Liquidator; a change and a credential's key take the one command path, the key kept nowhere", async ({ page }) => {
+  const errors: string[] = [];
+  page.on("pageerror", (error) => errors.push(error.message));
+  await feed(page, true, true);
+  const sent: CommandRequest[] = [];
+  const proto = (bytes: Uint8Array) => ({ body: Buffer.from(bytes), contentType: "application/x-protobuf" });
+  await page.route("**/api/v1/session", (r) => r.fulfill({ status: 204, headers: { "x-csrf-token": "csrf" } }));
+  await page.route("**/api/v1/command**", async (r) => {
+    const req = fromBinary(CommandRequestSchema, r.request().postDataBuffer()!);
+    sent.push(req);
+    return r.fulfill(proto(toBinary(CommandResponseSchema, create(CommandResponseSchema, { status: CommandStatus.ACCEPTED,
+      commandId: req.requestId, requestId: req.requestId, phase: CommandPhase.COMPLETED, detail: "PROVIDER_RECEIPT applied=true version=8" }))));
+  });
+  await tab(page, "Capture");
+  await expect(page.getByText("registry v7 · LIVE")).toBeVisible();
+  await expect(page.getByText("rpc · helius-rpc · selected", { exact: true })).toBeVisible();
+  await expect(page.getByText(/mainnet\.helius-rpc\.com · query:api-key · credential helius · free \(helius-free\)/)).toBeVisible();
+  await expect(page.getByText(/PROVIDER_DISABLED: public-wss/)).toBeVisible();
+  await expect(page.getByText("revision 3 · active · input")).toBeVisible();
+  // A new provider: typed, with the registry version the view showed.
+  await page.getByLabel("Id", { exact: true }).fill("public-rpc");
+  await page.getByLabel("Endpoint", { exact: true }).fill("https://api.mainnet-beta.solana.com");
+  await page.getByLabel("Authentication", { exact: true }).fill("none");
+  await page.getByRole("combobox", { name: /^Plan/ }).selectOption("free");
+  await page.getByLabel("Plan name", { exact: true }).fill("public");
+  await page.getByRole("button", { name: "Add", exact: true }).click();
+  await expect(page.getByTestId("p0-provider-command-receipt")).toContainText("COMPLETED · PROVIDER_RECEIPT");
+  const add = sent.at(-1)!;
+  expect(add.action).toBe("project0.provider.add");
+  expect(add.expected?.version).toEqual({ case: "configVersion", value: 7n });
+  const d = fromBinary(ProviderDescriptorCommandSchema, add.params);
+  expect([d.id, d.kind, d.endpoint, d.auth, d.plan, d.entitlement]).toEqual(["public-rpc", "rpc", "https://api.mainnet-beta.solana.com", "none", "free", "public"]);
+  // A key typed into the endpoint is refused before anything is sent.
+  await page.getByLabel("Endpoint", { exact: true }).fill("https://mainnet.helius-rpc.com/?api-key=TYPED-KEY");
+  await page.getByRole("button", { name: "Add", exact: true }).click();
+  await expect(page.getByText(/a key is the credential's, never the endpoint's/)).toBeVisible();
+  expect(sent).toHaveLength(1);
+  // Enabling the disabled WSS provider names it.
+  await page.getByRole("combobox", { name: /^Provider/ }).selectOption("public-wss");
+  await page.getByRole("button", { name: "Enable", exact: true }).click();
+  await expect.poll(() => sent.length).toBe(2);
+  expect([sent[1]!.action, sent[1]!.resourceId]).toEqual(["project0.provider.enable", "public-wss"]);
+  // A credential's key: sent once in the command's parameters, the field cleared, kept nowhere.
+  await page.getByLabel("Credential", { exact: true }).fill("helius");
+  await page.getByTestId("p0-provider-key").fill("PLAYWRIGHT-KEY-1");
+  await page.getByRole("button", { name: "Set key" }).click();
+  await expect.poll(() => sent.length).toBe(3);
+  const set = sent[2]!;
+  expect(set.action).toBe("project0.provider.credential.set");
+  const k = fromBinary(ProviderCredentialCommandSchema, set.params);
+  expect([k.credential, new TextDecoder().decode(k.secret)]).toEqual(["helius", "PLAYWRIGHT-KEY-1"]);
+  await expect(page.getByTestId("p0-provider-key")).toHaveValue("");
+  const stored = await page.evaluate(() => JSON.stringify({ ...localStorage }) + JSON.stringify({ ...sessionStorage }));
+  expect(stored).not.toContain("PLAYWRIGHT-KEY-1");
+  await expect(page.locator("body")).not.toContainText("PLAYWRIGHT-KEY-1");
+  // Liquidator: the Jupiter/Jito providers of the same registry.
+  await tab(page, "Liquidator");
+  await expect(page.getByText("Providers · Jupiter, Jito")).toBeVisible();
+  await expect(page.getByText("No jupiter/jito provider in the registry.")).toBeVisible();
+  expect(errors).toEqual([]);
+});
diff --git a/gateway/README.md b/gateway/README.md
index ddc0e6bc7f80252debeb9636de4fa2acbb587451..22e3bd95de82a9b4517117950b51065276fb78be 100644
--- a/gateway/README.md
+++ b/gateway/README.md
@@ -8,7 +8,7 @@ F3 transport layer for the browser. Protocol contract: `frontend/docs/GATEWAY_WI
 - Hello/Welcome, bounded subscriptions and connections, latest-value state via Tokio watch, ordered generation/sequence, heartbeat/Pong and watermark.
 - Overview source statuses; unavailable engine projections are not synthesized.
 - Engine feeds: Project_0 and BSC reporters over unix sockets, same-user peer check, exact per-source frame bound (a larger frame is a counted engine protocol violation), `StateTooLarge` as a per-topic FRAME_TOO_LARGE with recovery on the next valid state.
-- Commands: same-origin protobuf POST with session/CSRF/Origin/Host and one idempotency ledger with time-bounded idempotency (Wire V1.3 §7) configured by `DARKBOT_GATEWAY_COMMAND_CLOCK_WINDOW_MS`, `_RETENTION_MS` (at least twice the window) and `_LEDGER_RECORDS`.
+- Commands: same-origin protobuf POST with session/CSRF/Origin/Host and one idempotency ledger with time-bounded idempotency (Wire V1.3 §7) configured by `DARKBOT_GATEWAY_COMMAND_CLOCK_WINDOW_MS`, `_RETENTION_MS` (at least twice the window) and `_LEDGER_RECORDS`. DEC-18: a provider credential's key (`project0.provider.credential.set|rotate`) is separated from the request before the ledger records it and reaches only the native controller's standard input; an endpoint or name carrying a key is refused before any record.
 
 ## Boundaries
 EVENT catch-up (F4) and native engine receipts are future phases. Trading and LIVE are not authorized. Gateway-owned overview data cannot be written through external EngineFeed.
diff --git a/gateway/src/control.rs b/gateway/src/control.rs
index f0063a1ae490d37c24d77ca4c0c1dbd1dffa341e..645d605801fdaff26399ad735423859f36ce7e30 100644
--- a/gateway/src/control.rs
+++ b/gateway/src/control.rs
@@ -145,6 +145,160 @@ const AUTOSTART_PREVIEW: &str = "project0.autostart.preview";
 const AUTOSTART_GRANT: &str = "project0.autostart.grant";
 const AUTOSTART_REVOKE: &str = "project0.autostart.revoke";
 const AUTOSTART_ACTIONS: [&str; 3] = [AUTOSTART_PREVIEW, AUTOSTART_GRANT, AUTOSTART_REVOKE];
+/// DEC-18 (P0-020): the provider registry (`p0ctl provider …`) — a provider's add, update,
+/// removal, enablement, priority, selection and qualification, a credential's set, rotation and
+/// revocation; every change with the registry version its view showed (CAS) and its request id,
+/// effective at the next controlled start.
+const PROVIDER_ADD: &str = "project0.provider.add";
+const PROVIDER_UPDATE: &str = "project0.provider.update";
+const PROVIDER_REMOVE: &str = "project0.provider.remove";
+const PROVIDER_ENABLE: &str = "project0.provider.enable";
+const PROVIDER_DISABLE: &str = "project0.provider.disable";
+const PROVIDER_PRIORITY: &str = "project0.provider.priority";
+const PROVIDER_SELECT: &str = "project0.provider.select";
+const PROVIDER_UNSELECT: &str = "project0.provider.unselect";
+const PROVIDER_QUALIFY: &str = "project0.provider.qualify";
+const PROVIDER_CREDENTIAL_SET: &str = "project0.provider.credential.set";
+const PROVIDER_CREDENTIAL_ROTATE: &str = "project0.provider.credential.rotate";
+const PROVIDER_CREDENTIAL_REVOKE: &str = "project0.provider.credential.revoke";
+const PROVIDER_ACTIONS: [&str; 12] = [
+    PROVIDER_ADD,
+    PROVIDER_UPDATE,
+    PROVIDER_REMOVE,
+    PROVIDER_ENABLE,
+    PROVIDER_DISABLE,
+    PROVIDER_PRIORITY,
+    PROVIDER_SELECT,
+    PROVIDER_UNSELECT,
+    PROVIDER_QUALIFY,
+    PROVIDER_CREDENTIAL_SET,
+    PROVIDER_CREDENTIAL_ROTATE,
+    PROVIDER_CREDENTIAL_REVOKE,
+];
+/// DEC-18 (P0-019): the actions that carry a credential's secret — separated from the request
+/// before any durable record (the ledger holds the request without it) and handed to the native
+/// controller's standard input only.
+const SECRET_ACTIONS: [&str; 2] = [PROVIDER_CREDENTIAL_SET, PROVIDER_CREDENTIAL_ROTATE];
+
+/// DEC-18 §1.2: the provider fields' secret-free shape, the native descriptor's rules
+/// (`darkbot_solana::providers`) applied before the ledger, so a key typed into an endpoint or a
+/// name is refused and never recorded; the native controller decides every change.
+mod provider_shape {
+    /// 16 or more hex digits and nothing else (the form of many providers' keys).
+    fn key_shaped(s: &str) -> bool {
+        s.len() >= 16 && s.bytes().all(|b| b.is_ascii_hexdigit())
+    }
+    /// A provider or credential id: 1–32 of `[a-z0-9_-]`, never key-shaped.
+    pub fn id_ok(s: &str) -> bool {
+        !s.is_empty()
+            && s.len() <= 32
+            && !key_shaped(s)
+            && s.bytes()
+                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
+    }
+    /// A token-like path segment (the descriptor's three rules).
+    fn token_like(segment: &str) -> bool {
+        let has = |s: &str, f: fn(&u8) -> bool| s.as_bytes().iter().any(f);
+        let word = segment.split(['-', '_', '.']).any(|w| {
+            w.len() >= 16 && has(w, u8::is_ascii_digit) && has(w, u8::is_ascii_alphabetic)
+        });
+        let joined: String = segment
+            .chars()
+            .filter(|c| !matches!(c, '-' | '_' | '.'))
+            .collect();
+        let hex = joined.len() >= 16
+            && joined.bytes().all(|b| b.is_ascii_hexdigit())
+            && has(&joined, u8::is_ascii_digit)
+            && has(&joined, u8::is_ascii_alphabetic);
+        let mixed = segment.len() >= 20
+            && has(segment, u8::is_ascii_uppercase)
+            && has(segment, u8::is_ascii_lowercase)
+            && has(segment, u8::is_ascii_digit);
+        word || hex || mixed
+    }
+    /// An endpoint without user info, query, fragment or token-like path segment, at most 512
+    /// characters of visible ASCII.
+    pub fn endpoint_ok(url: &str) -> bool {
+        let Some((scheme, rest)) = url.split_once("://") else {
+            return false;
+        };
+        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
+        url.len() <= 512
+            && url.bytes().all(|b| b.is_ascii_graphic())
+            && ["https", "http", "wss", "ws"].contains(&scheme)
+            && !authority.is_empty()
+            && !authority.contains('@')
+            && !url.contains(['?', '#'])
+            && !path.split('/').any(token_like)
+    }
+    /// none | path | query:NAME | header:NAME (a name, never a key).
+    pub fn auth_ok(auth: &str) -> bool {
+        let name = |n: &str| {
+            !n.is_empty()
+                && n.len() <= 64
+                && !key_shaped(n)
+                && !token_like(n)
+                && n.bytes()
+                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
+        };
+        match auth.split_once(':') {
+            None => ["none", "path"].contains(&auth),
+            Some(("query" | "header", n)) => name(n),
+            _ => false,
+        }
+    }
+    /// The plan's published name: 1–32 of `[a-z0-9_.-]`, never key-shaped.
+    pub fn entitlement_ok(s: &str) -> bool {
+        !s.is_empty()
+            && s.len() <= 32
+            && !key_shaped(s)
+            && s.bytes().all(|b| {
+                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
+            })
+    }
+    pub const KINDS: [&str; 5] = ["rpc", "wss", "geyser", "jupiter", "jito"];
+}
+
+/// A credential's secret in transit: its bytes overwritten when it is released, on every path
+/// (volatile: never elided); never printed.
+struct SecretBuf(Vec<u8>);
+
+impl Drop for SecretBuf {
+    fn drop(&mut self) {
+        for b in self.0.iter_mut() {
+            // SAFETY: a valid, exclusive reference to an initialized byte.
+            unsafe { std::ptr::write_volatile(b, 0) };
+        }
+        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
+    }
+}
+
+/// DEC-18 (P0-019): `req` without its secret (the request every durable record holds) and the
+/// secret, for a credential action; `None` when its parameters do not decode or carry none.
+fn separate_secret(req: &CommandRequest) -> Option<(CommandRequest, Option<SecretBuf>)> {
+    use crate::proto::project0::v1::ProviderCredentialCommand;
+    if !SECRET_ACTIONS.contains(&req.action.as_str()) {
+        return Some((req.clone(), None));
+    }
+    let mut c = ProviderCredentialCommand::decode(req.params.as_ref()).ok()?;
+    if c.secret.is_empty() {
+        return None;
+    }
+    // Room for the line end the controller's input takes: the buffer never reallocates (an
+    // unwiped copy left behind).
+    let mut buf = Vec::with_capacity(c.secret.len() + 1);
+    buf.extend_from_slice(&c.secret);
+    let secret = SecretBuf(buf);
+    c.secret = Default::default();
+    Some((
+        CommandRequest {
+            params: c.encode_to_vec().into(),
+            ..req.clone()
+        },
+        Some(secret),
+    ))
+}
+
 const JOB_ACTIONS: [&str; 14] = [
     BACKTEST_START,
     OPTIMIZE_START,
@@ -172,7 +326,9 @@ fn online_history_read(req: &CommandRequest) -> bool {
 }
 fn allowed(project: i32, action: &str) -> bool {
     (project == ProjectId::Project0 as i32
-        && ([START, STOP].contains(&action) || JOB_ACTIONS.contains(&action)))
+        && ([START, STOP].contains(&action)
+            || JOB_ACTIONS.contains(&action)
+            || PROVIDER_ACTIONS.contains(&action)))
         || (project == ProjectId::Bsc as i32
             && ([BSC_START, BSC_STOP, BSC_CANDIDATE, BSC_LIVE_GRANT].contains(&action)
                 || BSC_OWNER_ACTIONS.contains(&action)))
@@ -469,7 +625,9 @@ fn job_args(req: &CommandRequest) -> Option<Vec<String>> {
     use crate::proto::project0::v1::{
         AutostartGrantCommand, AutostartPreviewCommand, BacktestJobCommand, BootNetworkCommand,
         BootOfflineCommand, BootStartCommand, CapabilityControlCommand, H0JobCommand,
-        OptimizeJobCommand, PostCaptureJobCommand, SourceControlCommand,
+        OptimizeJobCommand, PostCaptureJobCommand, ProviderCredentialCommand,
+        ProviderDescriptorCommand, ProviderPriorityCommand, ProviderQualifyCommand,
+        ProviderSelectCommand, SourceControlCommand,
     };
     // CTRL-03: a source change carries its CAS — the configuration version its view showed
     // (`LiquidatorState.source_config_version`), required here (review 5353cdcb); `p0ctl source
@@ -477,7 +635,9 @@ fn job_args(req: &CommandRequest) -> Option<Vec<String>> {
     let expect_version = match req.expected.as_ref().map(|e| e.version) {
         None => None,
         Some(Some(expected_version::Version::ConfigVersion(v)))
-            if req.action == SOURCE_SET || req.action == CAPABILITY_SET =>
+            if req.action == SOURCE_SET
+                || req.action == CAPABILITY_SET
+                || PROVIDER_ACTIONS.contains(&req.action.as_str()) =>
         {
             Some(v)
         }
@@ -639,6 +799,145 @@ fn job_args(req: &CommandRequest) -> Option<Vec<String>> {
             args.extend(["--request-id".into(), req.request_id.clone()]);
             Some(args)
         }
+        // DEC-18 (P0-020): a registry change — its shape here (a key in an endpoint or a name
+        // is never recorded), its rules the native controller's; the registry version the view
+        // showed and the request id always.
+        action if PROVIDER_ACTIONS.contains(&action) => {
+            use provider_shape::{auth_ok, endpoint_ok, entitlement_ok, id_ok, KINDS};
+            if !valid_job_id(&req.request_id) {
+                return None;
+            }
+            let none = req.params.is_empty();
+            let mut args: Vec<String> = ["p0", "provider"].map(String::from).to_vec();
+            let mut push = |items: &[&str]| args.extend(items.iter().map(|s| s.to_string()));
+            match action {
+                PROVIDER_ADD | PROVIDER_UPDATE if req.resource_id.is_empty() => {
+                    let c = ProviderDescriptorCommand::decode(req.params.as_ref()).ok()?;
+                    let add = action == PROVIDER_ADD;
+                    if !id_ok(&c.id)
+                        || (add
+                            && (c.kind.is_none()
+                                || c.endpoint.is_none()
+                                || c.plan.is_none()
+                                || c.entitlement.is_none()))
+                        || (!add && c.disabled)
+                    {
+                        return None;
+                    }
+                    push(&[if add { "add" } else { "update" }, "--id", &c.id]);
+                    if let Some(k) = &c.kind {
+                        if !KINDS.contains(&k.as_str()) {
+                            return None;
+                        }
+                        push(&["--kind", k]);
+                    }
+                    if let Some(e) = &c.endpoint {
+                        if !endpoint_ok(e) {
+                            return None;
+                        }
+                        push(&["--endpoint", e]);
+                    }
+                    if let Some(a) = &c.auth {
+                        if !auth_ok(a) {
+                            return None;
+                        }
+                        push(&["--auth", a]);
+                    }
+                    if let Some(cred) = &c.credential {
+                        if !id_ok(cred) {
+                            return None;
+                        }
+                        push(&["--credential", cred]);
+                    }
+                    if let Some(plan) = &c.plan {
+                        if !["free", "unknown"].contains(&plan.as_str()) {
+                            return None;
+                        }
+                        push(&["--plan", plan]);
+                    }
+                    if let Some(e) = &c.entitlement {
+                        if !entitlement_ok(e) {
+                            return None;
+                        }
+                        push(&["--entitlement", e]);
+                    }
+                    if let Some(n) = c.priority {
+                        push(&["--priority", &n.to_string()]);
+                    }
+                    if let Some(n) = c.capacity_rps {
+                        if n == 0 {
+                            return None;
+                        }
+                        push(&["--capacity-rps", &n.to_string()]);
+                    }
+                    if c.disabled {
+                        push(&["--disabled"]);
+                    }
+                }
+                PROVIDER_REMOVE | PROVIDER_ENABLE | PROVIDER_DISABLE
+                    if none && id_ok(&req.resource_id) =>
+                {
+                    let verb = action.rsplit('.').next()?;
+                    push(&[verb, "--id", &req.resource_id]);
+                }
+                PROVIDER_PRIORITY if req.resource_id.is_empty() => {
+                    let c = ProviderPriorityCommand::decode(req.params.as_ref()).ok()?;
+                    if !id_ok(&c.id) {
+                        return None;
+                    }
+                    push(&[
+                        "priority",
+                        "--id",
+                        &c.id,
+                        "--priority",
+                        &c.priority.to_string(),
+                    ]);
+                }
+                PROVIDER_SELECT if req.resource_id.is_empty() => {
+                    let c = ProviderSelectCommand::decode(req.params.as_ref()).ok()?;
+                    if !KINDS.contains(&c.kind.as_str()) || !id_ok(&c.id) {
+                        return None;
+                    }
+                    push(&["select", "--kind", &c.kind, "--id", &c.id]);
+                }
+                PROVIDER_UNSELECT if none && KINDS.contains(&req.resource_id.as_str()) => {
+                    push(&["unselect", "--kind", &req.resource_id]);
+                }
+                PROVIDER_QUALIFY if req.resource_id.is_empty() => {
+                    let c = ProviderQualifyCommand::decode(req.params.as_ref()).ok()?;
+                    let evidence_ok = c.evidence == "-"
+                        || (c.evidence.len() == 16
+                            && c.evidence
+                                .bytes()
+                                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
+                    if !id_ok(&c.id) || !evidence_ok {
+                        return None;
+                    }
+                    push(&["qualify", "--id", &c.id, "--evidence", &c.evidence]);
+                }
+                PROVIDER_CREDENTIAL_SET | PROVIDER_CREDENTIAL_ROTATE
+                    if req.resource_id.is_empty() =>
+                {
+                    let c = ProviderCredentialCommand::decode(req.params.as_ref()).ok()?;
+                    if !id_ok(&c.credential) {
+                        return None;
+                    }
+                    let verb = action.rsplit('.').next()?;
+                    push(&["credential", verb, "--credential", &c.credential]);
+                }
+                PROVIDER_CREDENTIAL_REVOKE if none && id_ok(&req.resource_id) => {
+                    push(&["credential", "revoke", "--credential", &req.resource_id]);
+                }
+                _ => return None,
+            }
+            args.extend([
+                "--request-id".into(),
+                req.request_id.clone(),
+                "--expect-version".into(),
+                expect_version?.to_string(),
+            ]);
+            Some(args)
+        }
         JOB_CANCEL if req.params.is_empty() && valid_job_id(&req.resource_id) => Some(
             ["p0", "job", "cancel", "--id", &req.resource_id]
                 .map(String::from)
@@ -1312,6 +1611,7 @@ impl Control {
         // DEC-07: project0.capture.start with the start approval of a market flow is the boot
         // controller's start step; a bare one is refused before dispatch (below).
         let job = if JOB_ACTIONS.contains(&req.action.as_str())
+            || PROVIDER_ACTIONS.contains(&req.action.as_str())
             || (req.action == START
                 && req.project == ProjectId::Project0 as i32
                 && !req.params.is_empty())
@@ -1323,7 +1623,9 @@ impl Control {
                         id,
                         CommandStatus::Rejected,
                         CommandRejectReason::InvalidParams,
-                        if req.action == CAPABILITY_SET {
+                        if PROVIDER_ACTIONS.contains(&req.action.as_str()) {
+                            "Provider request invalid (ids [a-z0-9_-]{1,32}, kind rpc|wss|geyser|jupiter|jito, an endpoint without a credential in it, auth none|path|query:NAME|header:NAME, plan free|unknown, the registry version and a credential's secret; never echoed)"
+                        } else if req.action == CAPABILITY_SET {
                             "Capability request invalid (LIVE OFF|LIVE, HOLD_B, JITO_BUNDLE, THRESHOLD_TRIGGER, INVENTORY_REBALANCING or LOCAL_FEE_MODEL OFF|OBSERVE|ACTIVE, ECON_* or WALLET:<pubkey> ENABLED|DISABLED, FE06_GRANT|FE06_REVOKE <digest>; the configuration version, and the owner epoch for all but a move to OFF)"
                         } else if BOOT_ACTIONS.contains(&req.action.as_str()) || req.action == START
                         {
@@ -1362,7 +1664,18 @@ impl Control {
                 "Capture controller not configured",
             );
         };
-        let encoded = req.encode_to_vec();
+        // DEC-18 (P0-019): a credential's secret never enters the ledger — the request is
+        // recorded without it, the secret goes to the controller's standard input only.
+        let Some((stored, secret)) = separate_secret(req) else {
+            return reply(
+                id,
+                CommandStatus::Rejected,
+                CommandRejectReason::InvalidParams,
+                "Credential request invalid: a credential id and its secret",
+            );
+        };
+        let encoded = stored.encode_to_vec();
+        drop(stored);
         match self.load(id) {
             Ok(Some(record)) if record.request == encoded => {
                 // A replay returns the stored receipt with the current ledger phase.
@@ -1430,8 +1743,11 @@ impl Control {
             _ => {}
         }
         if let Some(args) = job {
-            return self.dispatch_job(p, id, encoded, &req.action, args).await;
+            return self
+                .dispatch_job(p, id, encoded, &req.action, args, secret)
+                .await;
         }
+        drop(secret);
         if let Some(source) = source {
             return self.dispatch_source(p, id, encoded, source).await;
         }
@@ -1583,12 +1899,17 @@ impl Control {
         encoded: Vec<u8>,
         action: &str,
         args: Vec<String>,
+        secret: Option<SecretBuf>,
     ) -> CommandResponse {
         let accepted = reply(
             id,
             CommandStatus::Accepted,
             CommandRejectReason::Unspecified,
-            "Analytics job intent persisted; the job's progress is project0.analytics.jobs",
+            if PROVIDER_ACTIONS.contains(&action) {
+                "Provider change intent persisted; its receipt by request id, the registry in project0.analytics.jobs (providers)"
+            } else {
+                "Analytics job intent persisted; the job's progress is project0.analytics.jobs"
+            },
         );
         let mut record = Record {
             request: encoded,
@@ -1606,7 +1927,7 @@ impl Control {
                 "Cannot durably record command (ledger full within the idempotency window, or unavailable); retry later",
             );
         }
-        let output = tokio::time::timeout(self.controller, {
+        let output = tokio::time::timeout(self.controller, async {
             let mut ctl = Command::new(&p.ctl);
             ctl.args(&args).kill_on_drop(true);
             // DEC-07: the boot controller audits the staged capture environment, as the
@@ -1617,10 +1938,50 @@ impl Control {
             {
                 ctl.env("DARKBOT_STAGED", "1");
             }
-            ctl.output()
+            let Some(mut secret) = secret else {
+                // A controller never reads the gateway's own standard input.
+                ctl.stdin(std::process::Stdio::null());
+                return ctl.output().await;
+            };
+            // DEC-18 (P0-019): the secret through the controller's standard input only (never an
+            // argument, the environment or a record); wiped when this ends.
+            use tokio::io::AsyncWriteExt;
+            ctl.stdin(std::process::Stdio::piped())
+                .stdout(std::process::Stdio::piped())
+                .stderr(std::process::Stdio::piped());
+            let mut child = ctl.spawn()?;
+            if let Some(mut stdin) = child.stdin.take() {
+                secret.0.push(b'\n');
+                let written = stdin.write_all(&secret.0).await;
+                drop(stdin);
+                drop(secret);
+                written?;
+            }
+            child.wait_with_output().await
         })
         .await;
+        // DEC-18: the registry's receipt (`PROVIDER_RECEIPT …`, redacted): its outcome either way.
+        let provider_receipt = |out: &std::process::Output| {
+            String::from_utf8_lossy(&out.stdout)
+                .lines()
+                .find(|l| l.starts_with("PROVIDER_RECEIPT "))
+                .map(|l| redact(l.trim()).chars().take(300).collect::<String>())
+        };
+        let provider = PROVIDER_ACTIONS.contains(&action);
         match output {
+            Ok(Ok(out)) if out.status.success() && provider => {
+                record.phase = CommandPhase::Completed as i32;
+                record.detail = format!(
+                    "Provider change recorded, effective at the next controlled start (project0.analytics.jobs providers): {}",
+                    provider_receipt(&out).unwrap_or_else(|| "no receipt line".into())
+                );
+            }
+            Ok(Ok(out)) if provider => {
+                record.phase = CommandPhase::Failed as i32;
+                record.detail = provider_receipt(&out)
+                    .map(|r| format!("Native controller refused: {r}"))
+                    .unwrap_or_else(|| refusal(&out.stderr));
+            }
             Ok(Ok(out)) if out.status.success() => {
                 record.phase = CommandPhase::Completed as i32;
                 record.detail = if action == JOB_CANCEL {
@@ -1655,7 +2016,9 @@ impl Control {
                 record.detail = refusal(&out.stderr);
             }
             _ => {
-                record.detail = if action == CAPABILITY_SET {
+                record.detail = if provider {
+                    "Controller outcome unknown; inspect the provider registry in project0.analytics.jobs before a new request (the same request id returns its receipt)"
+                } else if action == CAPABILITY_SET {
                     "Controller outcome unknown; inspect the LIVE capability in project0.liquidator.state before a new request"
                 } else if BOOT_ACTIONS.contains(&action) || action == START {
                     "Controller outcome unknown; inspect the boot flow in project0.analytics.jobs before a new approval"
@@ -4289,6 +4652,204 @@ mod tests {
     /// `p0ctl job` (through `darkbotctl p0 job`), without observing a capture process; a native
     /// refusal is a FAILED receipt with its code; invalid parameters are rejected before the
     /// ledger and never dispatched.
+    #[tokio::test]
+    async fn dec18_provider_commands_map_onto_p0ctl_and_no_secret_is_recorded() {
+        // DEC-18 (P0-019/P0-020): every registry change through the one controller with its CAS
+        // and request id; a key in an endpoint or a name is refused before the ledger; a
+        // credential's secret reaches only the controller's standard input — never the ledger,
+        // an argument or the receipt; a replay never runs the controller again.
+        use crate::proto::project0::v1::{
+            ProviderCredentialCommand, ProviderDescriptorCommand, ProviderSelectCommand,
+        };
+        use crate::proto::{expected_version, ExpectedVersion};
+        let dir = std::env::temp_dir().join(format!("gw-providers-{}", std::process::id()));
+        let _ = fs::remove_dir_all(&dir);
+        fs::create_dir_all(dir.join("run")).unwrap();
+        let ctl = dir.join("darkbotctl");
+        let (log, input, runs) = (dir.join("argv"), dir.join("stdin"), dir.join("runs"));
+        executable(
+            &ctl,
+            &format!(
+                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{log}'\necho run >> '{runs}'\ncat > '{input}'\nif [ \"$7\" = refused ]; then echo 'PROVIDER_RECEIPT request_id=x applied=false version=4 action=\"select rpc refused\" detail=\"PROVIDER_UNKNOWN: refused\"'; exit 2; fi\necho 'PROVIDER_RECEIPT request_id=x applied=true version=5 action=\"credential set helius\" detail=\"stored\"'\n",
+                log = log.display(),
+                runs = runs.display(),
+                input = input.display()
+            ),
+        );
+        let control = Control::new(
+            Some(Paths {
+                ctl,
+                rundir: dir.join("run"),
+                logdir: dir.clone(),
+                label: "com.darkbot.test.absent.capture".into(),
+                launchctl: absent_launchctl(&dir),
+            }),
+            &Limits::default(),
+        );
+        let request =
+            |n: u8, action: &str, resource: &str, params: Vec<u8>, version: Option<u64>| {
+                CommandRequest {
+                    project: ProjectId::Project0 as i32,
+                    request_id: format!("{n}2345678-1234-1234-1234-123456789abc"),
+                    action: action.into(),
+                    resource_id: resource.into(),
+                    expected: version.map(|v| ExpectedVersion {
+                        version: Some(expected_version::Version::ConfigVersion(v)),
+                    }),
+                    params: params.into(),
+                    issued_at_unix_ns: crate::overview::unix_ns_now(),
+                }
+            };
+        let argv = || fs::read_to_string(&log).unwrap_or_default();
+        let ledger = || {
+            let mut all = Vec::new();
+            for e in fs::read_dir(dir.join("run/commands")).unwrap() {
+                let path = e.unwrap().path();
+                if path.is_file() {
+                    all.extend(fs::read(path).unwrap());
+                }
+            }
+            all
+        };
+        let holds = |bytes: &[u8], needle: &str| {
+            bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
+        };
+        let descriptor = |endpoint: &str, credential: &str| ProviderDescriptorCommand {
+            id: "helius-rpc".into(),
+            kind: Some("rpc".into()),
+            endpoint: Some(endpoint.into()),
+            auth: Some("query:api-key".into()),
+            credential: Some(credential.into()),
+            priority: Some(0),
+            capacity_rps: Some(10),
+            plan: Some("free".into()),
+            entitlement: Some("helius-free".into()),
+            disabled: false,
+        };
+        let add = descriptor("https://mainnet.helius-rpc.com", "helius").encode_to_vec();
+        let r = control
+            .execute(&request(1, PROVIDER_ADD, "", add.clone(), Some(4)))
+            .await;
+        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
+        assert_eq!(
+            argv(),
+            "p0\nprovider\nadd\n--id\nhelius-rpc\n--kind\nrpc\n--endpoint\nhttps://mainnet.helius-rpc.com\n--auth\nquery:api-key\n--credential\nhelius\n--plan\nfree\n--entitlement\nhelius-free\n--priority\n0\n--capacity-rps\n10\n--request-id\n12345678-1234-1234-1234-123456789abc\n--expect-version\n4\n"
+        );
+        assert!(r.detail.contains("PROVIDER_RECEIPT"), "{}", r.detail);
+        // The registry version is required (CAS).
+        let r = control
+            .execute(&request(2, PROVIDER_ADD, "", add, None))
+            .await;
+        assert_eq!(r.reject_reason, CommandRejectReason::InvalidParams as i32);
+        // A key in an endpoint or typed into a name is refused before any record.
+        for (n, params) in [
+            (
+                3,
+                descriptor(
+                    "https://mainnet.helius-rpc.com/?api-key=LEAKED-KEY-1",
+                    "helius",
+                ),
+            ),
+            (
+                4,
+                descriptor("https://user:LEAKED-KEY-1@rpc.example", "helius"),
+            ),
+            (
+                5,
+                descriptor("https://rpc.example/LEAKEDKEY1a2b3c4d5e6f7g8h9", "helius"),
+            ),
+            (
+                6,
+                descriptor(
+                    "https://mainnet.helius-rpc.com",
+                    "0123456789abcdef0123456789abcdef",
+                ),
+            ),
+        ] {
+            let r = control
+                .execute(&request(
+                    n,
+                    PROVIDER_ADD,
+                    "",
+                    params.encode_to_vec(),
+                    Some(4),
+                ))
+                .await;
+            assert_eq!(
+                r.reject_reason,
+                CommandRejectReason::InvalidParams as i32,
+                "{n}"
+            );
+            assert!(!r.detail.contains("LEAKED"), "{}", r.detail);
+        }
+        let l = ledger();
+        assert!(!holds(&l, "LEAKED") && !holds(&l, "0123456789abcdef0123"));
+        // A credential's secret: the controller's standard input only.
+        let set = ProviderCredentialCommand {
+            credential: "helius".into(),
+            secret: b"SECRET-KEY-1".to_vec().into(),
+        }
+        .encode_to_vec();
+        let set_request = request(7, PROVIDER_CREDENTIAL_SET, "", set, Some(4));
+        let r = control.execute(&set_request).await;
+        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
+        assert_eq!(fs::read_to_string(&input).unwrap(), "SECRET-KEY-1\n");
+        assert!(argv().contains("credential\nset\n--credential\nhelius\n"));
+        assert!(!argv().contains("SECRET") && !r.detail.contains("SECRET"));
+        assert!(
+            !holds(&ledger(), "SECRET-KEY-1"),
+            "the ledger holds no secret"
+        );
+        // The same request again is the stored receipt; the controller is not run again.
+        let before = fs::read_to_string(&runs).unwrap().lines().count();
+        let again = control.execute(&set_request).await;
+        assert_eq!(again.phase, CommandPhase::Completed as i32);
+        assert!(again.detail.starts_with("Provider change intent persisted"));
+        assert_eq!(fs::read_to_string(&runs).unwrap().lines().count(), before);
+        // A credential action without its secret is refused.
+        let empty = ProviderCredentialCommand {
+            credential: "helius".into(),
+            secret: Default::default(),
+        }
+        .encode_to_vec();
+        let r = control
+            .execute(&request(8, PROVIDER_CREDENTIAL_ROTATE, "", empty, Some(4)))
+            .await;
+        assert_eq!(r.reject_reason, CommandRejectReason::InvalidParams as i32);
+        // A refused change is FAILED with the registry's receipt.
+        let refused = ProviderSelectCommand {
+            kind: "rpc".into(),
+            id: "refused".into(),
+        }
+        .encode_to_vec();
+        let r = control
+            .execute(&request(9, PROVIDER_SELECT, "", refused, Some(4)))
+            .await;
+        assert_eq!(r.phase, CommandPhase::Failed as i32, "{}", r.detail);
+        assert!(
+            r.detail.contains("PROVIDER_UNKNOWN: refused"),
+            "{}",
+            r.detail
+        );
+        // Remove, revoke and unselect name their resource; a kind is one of the five.
+        let r = control
+            .execute(&request(
+                0,
+                PROVIDER_CREDENTIAL_REVOKE,
+                "helius",
+                vec![],
+                Some(5),
+            ))
+            .await;
+        assert_eq!(r.phase, CommandPhase::Completed as i32, "{}", r.detail);
+        assert!(argv().starts_with("p0\nprovider\ncredential\nrevoke\n--credential\nhelius\n"));
+        let r = control
+            .execute(&request(6, PROVIDER_UNSELECT, "ftp", vec![], Some(5)))
+            .await;
+        assert_eq!(r.reject_reason, CommandRejectReason::InvalidParams as i32);
+        let _ = fs::remove_dir_all(&dir);
+    }
+
     #[tokio::test]
     async fn analytics_job_commands_dispatch_the_p0ctl_job_options() {
         use crate::proto::project0::v1::{BacktestJobCommand, FundingSet, OptimizeJobCommand};
diff --git a/ops/darkbotctl b/ops/darkbotctl
index f873bb528188aa1f91e95c55729cc6be28d96960..c866aad75fa0e4546ec0a669f1de0db13528b958 100755
--- a/ops/darkbotctl
+++ b/ops/darkbotctl
@@ -434,9 +434,11 @@ _gateway_down() {
 # B03/U-9, S1-02, DEC-13, DEC-07: the GUI's analytics jobs, source modes, capability requests and
 # boot flow run the deployed p0ctl (the same `p0ctl job|source|capability|boot` a terminal runs)
 # on the app's data home; a started job's or boot step's worker outlives this call. P8-CTRL.8:
-# `p0 live` previews, grants, revokes and counts FE-06 scopes on that same data home.
+# `p0 live` previews, grants, revokes and counts FE-06 scopes on that same data home. DEC-18:
+# `p0 provider` decides over the provider registry of that data home; a credential's secret
+# passes through the standard input only (exec: nothing here reads it).
 cmd_p0() {
-  case "${1:-}" in job|source|capability|boot|horizon|autostart|live) ;; *) die "p0 job start|cancel|status … | p0 source set UNIT MODE [--expect-version N] | p0 source status | p0 capability set LIVE OFF|LIVE | HOLD_B|JITO_BUNDLE|THRESHOLD_TRIGGER|INVENTORY_REBALANCING|LOCAL_FEE_MODEL OFF|OBSERVE|ACTIVE | ECON_SIGN|ECON_ARM|ECON_SEND|ECON_ATTEMPT|ECON_LIVE|WALLET:<pubkey> ENABLED|DISABLED | FE06_GRANT|FE06_REVOKE <digest> [--expect-version N --expect-epoch E --request-id ID] | p0 capability status | p0 live scope|grant|revoke|status|attempts … | p0 boot offline|network|start|evidence import|cancel|status … | p0 horizon qualify|status … | p0 autostart preview|grant|revoke|status|start …" ;; esac
+  case "${1:-}" in job|source|capability|boot|horizon|autostart|live|provider) ;; *) die "p0 job start|cancel|status … | p0 source set UNIT MODE [--expect-version N] | p0 source status | p0 provider status|add|update|remove|enable|disable|priority|select|unselect|qualify|credential set|rotate|revoke … [--request-id ID --expect-version N] (a credential's secret from the standard input only) | p0 capability set LIVE OFF|LIVE | HOLD_B|JITO_BUNDLE|THRESHOLD_TRIGGER|INVENTORY_REBALANCING|LOCAL_FEE_MODEL OFF|OBSERVE|ACTIVE | ECON_SIGN|ECON_ARM|ECON_SEND|ECON_ATTEMPT|ECON_LIVE|WALLET:<pubkey> ENABLED|DISABLED | FE06_GRANT|FE06_REVOKE <digest> [--expect-version N --expect-epoch E --request-id ID] | p0 capability status | p0 live scope|grant|revoke|status|attempts … | p0 boot offline|network|start|evidence import|cancel|status … | p0 horizon qualify|status … | p0 autostart preview|grant|revoke|status|start …" ;; esac
   [ -x "$P0CTL_BIN" ] || die "p0ctl nicht deployed: darkbotctl build && darkbotctl deploy"
   if [ "$1" = boot ] || [ "$1" = horizon ] || [ "$1" = autostart ]; then
     # DEC-07: a raw-capture flow audits exactly the environment the capture runs with (its
diff --git a/project_0/docs/F0_VAR_01_MATRIX.md b/project_0/docs/F0_VAR_01_MATRIX.md
index ea1c1af0ce8948d7218549717842dba1cbf5b190..36a7daf4806cdaf5ce7c1900084eb8381517b840 100644
--- a/project_0/docs/F0_VAR_01_MATRIX.md
+++ b/project_0/docs/F0_VAR_01_MATRIX.md
@@ -138,13 +138,13 @@ Commitstand: Code `98254f75` für die semantischen Tabellen A–C (Paket 0 und F
 
 ## Nachtrag DEC-18 — Provider-Bedienung (Nutzerentscheidung 01.11.2026, Soll)
 
-DEC-18 (`ENTSCHEIDUNGEN.md` Nr. 18, `PHASENPLAN.md` Abschnitt „DEC-18“) verlangt die vollständige Provider-Konfiguration bis in die bestehenden Reiter **vor** jedem Mainnet-Smoke. Am 01.11.2026 (`14ed3a16`) existiert davon kein projizierter Pfad; alle Einträge sind `SOLL_MISSING` bis zum End-to-End-Nachweis (Tickets `BUGS.md` P0-017…P0-022). Die Summentabelle oben bleibt der Paket-0-Stand; dieser Nachtrag zählt 6 zusätzliche `SOLL_MISSING`.
+DEC-18 (`ENTSCHEIDUNGEN.md` Nr. 18, `PHASENPLAN.md` Abschnitt „DEC-18“) verlangt die vollständige Provider-Konfiguration bis in die bestehenden Reiter **vor** jedem Mainnet-Smoke. Am 01.11.2026 (`14ed3a16`) existiert davon kein projizierter Pfad; alle Einträge sind `SOLL_MISSING` bis zum End-to-End-Nachweis (Tickets `BUGS.md` P0-017…P0-022). Stand DEC-18 Paket 4 (P0-020): C-PV-1/2, L-PV-1/2 verbunden; C-PV-3/4 (C-11, P0-022) weiter `SOLL_MISSING`. Die Summentabelle oben bleibt der Paket-0-Stand; dieser Nachtrag zählt 6 zusätzliche `SOLL_MISSING`.
 
 | ID | Reiter | Native Quelle (Soll) | Inhalt | Command-Rückweg (N-5 DEC-18) | Status |
 |---|---|---|---|---|---|
-| C-PV-1 | Capture/Health | Provider-Descriptor in `RuntimeConfig`/`Resolved` (P0-017) | RPC/WSS/Geyser: Provider-ID, Typ, redigierter Host, Auth-Typ, Priorität, Kapazität, Quelle, Tarif/Entitlement, Version, `requested/effective`, Blockgrund, Restart-Regel | anlegen, bearbeiten, auswählen, de-/aktivieren, Priorität (gestaged, wirksam beim kontrollierten Neustart) | SOLL_MISSING |
-| C-PV-2 | Capture/Health | Credential-Referenz/-Revision (P0-018, P0-019) | Credential-Status, Alias, Revision, Rotation/Widerruf — nie der Key | Credential set/rotate/revoke über den nichtpersistierenden Credential-Eingang | SOLL_MISSING |
+| C-PV-1 | Capture/Health | Provider-Descriptor in `RuntimeConfig`/`Resolved` (P0-017) | RPC/WSS/Geyser: Provider-ID, Typ, redigierter Host, Auth-Typ, Priorität, Kapazität, Quelle, Tarif/Entitlement, Version, `requested/effective`, Blockgrund, Restart-Regel | anlegen, bearbeiten, auswählen, de-/aktivieren, Priorität (gestaged, wirksam beim kontrollierten Neustart) | IST_CONNECTED · Registry (Topic 15 `AnalyticsJobs.providers`) und wirksame Bindungen des laufenden Prozesses (Topic 10 `EffectiveRecord.providers`) im Reiter Capture; Commands `project0.provider.*` (CAS Registry-Version) |
+| C-PV-2 | Capture/Health | Credential-Referenz/-Revision (P0-018, P0-019) | Credential-Status, Alias, Revision, Rotation/Widerruf — nie der Key | Credential set/rotate/revoke über den nichtpersistierenden Credential-Eingang | IST_CONNECTED · Revision/Status/Herkunft je Credential; set/rotate über den Gateway-Eingang (Schlüssel nur Controller-stdin, nie im Ledger), revoke |
 | C-PV-3 | Capture | Provider-Evidence-Record (P0-022) | ausgewählter Provider, Evidence-ID, Messfenster, Ergebnis je Methode/Subscription (`SUPPORTED`/`METHOD_UNSUPPORTED`/`NOT_PROVEN`/`BLOCKED`), Ablehnungsgründe | Messung über den Boot-/Control-Weg (Offline-Token-Bestätigung) | SOLL_MISSING |
 | C-PV-4 | Capture | C-11-`ProbeSpec` je Modus (P0-022) | Methoden/Subscriptions je Modus, Grenzen, Kostenfreiheit, Config-Hash | nein (Anzeige) | SOLL_MISSING |
-| L-PV-1 | Liquidator | Jupiter-Provider-Descriptor (P0-017, P0-021) | Basis-URL (redigiert), Auth-Typ, Tarif, P6-MT.7-Quote und -Bucket-Zustand, Readiness | bearbeiten/auswählen (gestaged) | SOLL_MISSING |
-| L-PV-2 | Liquidator | Jito-Provider-Descriptor (P0-017; DEC-17 c) | Endpoint (redigiert), Auth-Typ (Standard ohne Keypair), Readiness | bearbeiten/auswählen (gestaged) | SOLL_MISSING |
+| L-PV-1 | Liquidator | Jupiter-Provider-Descriptor (P0-017, P0-021) | Basis-URL (redigiert), Auth-Typ, Tarif, P6-MT.7-Quote und -Bucket-Zustand, Readiness | bearbeiten/auswählen (gestaged) | IST_CONNECTED · Registry-Provider/Auswahl im Reiter Liquidator (Default-Profil bis zur Auswahl); P6-MT.7-Bucket-Zustand weiter in der Owner-Projektion |
+| L-PV-2 | Liquidator | Jito-Provider-Descriptor (P0-017; DEC-17 c) | Endpoint (redigiert), Auth-Typ (Standard ohne Keypair), Readiness | bearbeiten/auswählen (gestaged) | IST_CONNECTED · Registry-Provider/Auswahl im Reiter Liquidator |
diff --git a/project_0/native/crates/reporter/src/analytics.rs b/project_0/native/crates/reporter/src/analytics.rs
index fc5c0ee0ea2c51f3d6c412d5126526ad83ef330f..6569e0bbbb8062c367448e5af2ff8dfd7d0f8ce7 100644
--- a/project_0/native/crates/reporter/src/analytics.rs
+++ b/project_0/native/crates/reporter/src/analytics.rs
@@ -6,8 +6,9 @@ use crate::proto::project0::{
     analytics_job, AnalyticsJob, AnalyticsJobs, AnalyticsRun, AutostartGrant, BacktestResult,
     BootFlow, BootStep, CapitalScenario, FundingSet, H0Result, H0Spec, JobGrid, JobParams,
     NamedCount, OptimizeResult, OptimizeScore, PostCaptureResult, PostCaptureSpec,
-    RunQualification, ShadowCounterfactual, UntilStopped, UntilStoppedQualification,
-    UntilStoppedRun, VerificationMethod,
+    ProviderRegistryView, RegistryCredential, RegistryProvider, RunQualification,
+    ShadowCounterfactual, UntilStopped, UntilStoppedQualification, UntilStoppedRun,
+    VerificationMethod,
 };
 use darkbot_runner::analytics::backtest::{Counterfactual, Params, Score};
 use darkbot_runner::analytics::jobs::{
@@ -353,6 +354,48 @@ pub fn analytics_jobs(s: &AnalyticsSnapshot) -> AnalyticsJobs {
         boot: s.boot.as_ref().map(boot),
         boot_error: s.boot_error.clone().unwrap_or_default(),
         until_stopped: Some(until_stopped(&s.until_stopped)),
+        providers: s.providers.as_ref().map(providers),
+        providers_error: s.providers_error.clone().unwrap_or_default(),
+    }
+}
+
+/// DEC-18: the provider registry, values copied as the store holds them (secret-free: hosts,
+/// authentication names, credential ids and revisions).
+fn providers(v: &darkbot_runner::orchestration::provider_registry::RegistryView) -> ProviderRegistryView {
+    ProviderRegistryView {
+        version: v.version,
+        providers: v
+            .providers
+            .iter()
+            .map(|p| RegistryProvider {
+                id: p.id.clone(),
+                kind: p.kind.to_owned(),
+                host: p.host.clone(),
+                auth: p.auth.clone(),
+                credential: p.credential.clone().unwrap_or_default(),
+                priority: p.priority,
+                capacity_rps: p.capacity_rps,
+                plan: p.plan.to_owned(),
+                entitlement: p.entitlement.clone(),
+                qualification: p.qualification.clone(),
+                enabled: p.enabled,
+                selected: p.selected,
+                binding: p.binding.clone(),
+                selection_block: p.selection_block.clone(),
+            })
+            .collect(),
+        credentials: v
+            .credentials
+            .iter()
+            .map(|c| RegistryCredential {
+                id: c.id.clone(),
+                revision: c.revision,
+                state: c.state.to_owned(),
+                origin: c.origin.clone(),
+                created_unix_s: c.created_unix_s,
+            })
+            .collect(),
+        restart_rule: "next_controlled_start".into(),
     }
 }
 
diff --git a/project_0/native/crates/reporter/src/lib.rs b/project_0/native/crates/reporter/src/lib.rs
index 4bac0b6608ef56130e90decad706358249e6e5eb..ed216589fd441c26742648cd94b6e98dcfe433f1 100644
--- a/project_0/native/crates/reporter/src/lib.rs
+++ b/project_0/native/crates/reporter/src/lib.rs
@@ -744,6 +744,24 @@ fn operational_config(o: &OperationalSnapshot) -> OperationalConfig {
             c0_census: e.c0_census,
             source_geyser: e.source_geyser.clone(),
             source_geyser_provider: e.source_geyser_provider.clone(),
+            // DEC-18: the run's providers (secret-free, no endpoint).
+            providers: e
+                .providers
+                .iter()
+                .map(|p| crate::proto::project0::RunProvider {
+                    kind: p.kind.name().to_owned(),
+                    id: p.id.clone(),
+                    binding: p.binding.clone(),
+                    registry_version: p.registry_version,
+                    credential_id: p
+                        .credential
+                        .as_ref()
+                        .map(|(id, _)| id.clone())
+                        .unwrap_or_default(),
+                    credential_revision: p.credential.as_ref().map_or(0, |(_, r)| *r),
+                    plan: p.plan.name().to_owned(),
+                })
+                .collect(),
         }),
     }
 }
@@ -1648,11 +1666,74 @@ mod contract_tests {
             }),
             boot_error: None,
             until_stopped: golden_until_stopped(),
+            providers: Some(golden_providers()),
+            providers_error: None,
         }
     }
 
     /// DEC-16: a standing approval in force, a newer preview awaiting its digest, a raw-capture
     /// qualification with an open part and an unbounded run that ended in order.
+    /// DEC-18: a registry with a selected keyed RPC provider, a disabled WSS one without a
+    /// credential and a revoked credential (secret-free: hosts, names, revisions).
+    fn golden_providers() -> darkbot_runner::orchestration::provider_registry::RegistryView {
+        use darkbot_runner::orchestration::provider_registry::{
+            CredentialView, ProviderView, RegistryView,
+        };
+        RegistryView {
+            version: 7,
+            providers: vec![
+                ProviderView {
+                    id: "helius-rpc".into(),
+                    kind: "rpc",
+                    host: "mainnet.helius-rpc.com".into(),
+                    auth: "query:api-key".into(),
+                    credential: Some("helius".into()),
+                    priority: 0,
+                    capacity_rps: Some(10),
+                    plan: "free",
+                    entitlement: "helius-free".into(),
+                    qualification: Some("0123456789abcdef".into()),
+                    enabled: true,
+                    selected: true,
+                    binding: Some("b".repeat(64)),
+                    selection_block: None,
+                },
+                ProviderView {
+                    id: "public-wss".into(),
+                    kind: "wss",
+                    host: "api.mainnet-beta.solana.com".into(),
+                    auth: "none".into(),
+                    credential: None,
+                    priority: 5,
+                    capacity_rps: None,
+                    plan: "free",
+                    entitlement: "public".into(),
+                    qualification: None,
+                    enabled: false,
+                    selected: false,
+                    binding: Some("c".repeat(64)),
+                    selection_block: Some("PROVIDER_DISABLED: public-wss".into()),
+                },
+            ],
+            credentials: vec![
+                CredentialView {
+                    id: "helius".into(),
+                    revision: 3,
+                    state: "active",
+                    origin: "input".into(),
+                    created_unix_s: 1_760_000_000,
+                },
+                CredentialView {
+                    id: "old".into(),
+                    revision: 1,
+                    state: "revoked",
+                    origin: "env:HELIUS_API_KEY".into(),
+                    created_unix_s: 1_750_000_000,
+                },
+            ],
+        }
+    }
+
     fn golden_until_stopped() -> darkbot_runner::orchestration::until_stopped_view::UntilStoppedView
     {
         use darkbot_runner::orchestration::autostart::{Grant, WindowBudget};
@@ -2627,7 +2708,17 @@ mod contract_tests {
             c0_census: true,
             source_geyser: "observe".into(),
             source_geyser_provider: "p".repeat(64),
-            providers: vec![],
+            // DEC-18: every kind's provider, a fallback list of RPC providers.
+            providers: (0..8)
+                .map(|n| darkbot_runner::status_tap::ProviderBinding {
+                    kind: darkbot_runner::status_tap::PROVIDER_KINDS[n % 5],
+                    id: format!("{n}").repeat(32),
+                    binding: "b".repeat(64),
+                    registry_version: u64::MAX,
+                    credential: Some(("c".repeat(32), u64::MAX)),
+                    plan: darkbot_runner::status_tap::Plan::Free,
+                })
+                .collect(),
         };
         let mut s = golden_snapshot();
         s.operational = Some(Arc::new(operational_snapshot_of(&limits, Some(&effective))));
diff --git a/project_0/native/crates/runtime/src/analytics/jobs.rs b/project_0/native/crates/runtime/src/analytics/jobs.rs
index 9550a2df658f2bccb0e459115321468007f22f2a..e578f35c8514312dfd87d81eb77783a7763a6825 100644
--- a/project_0/native/crates/runtime/src/analytics/jobs.rs
+++ b/project_0/native/crates/runtime/src/analytics/jobs.rs
@@ -1012,6 +1012,10 @@ pub struct AnalyticsSnapshot {
     /// DEC-16: the standing autostart approval, the newest raw-capture qualification and the
     /// newest unbounded raw capture.
     pub until_stopped: crate::orchestration::until_stopped_view::UntilStoppedView,
+    /// DEC-18: the provider registry (staged: effective at the next controlled start).
+    pub providers: Option<crate::orchestration::provider_registry::RegistryView>,
+    /// DEC-18: why the provider registry was not readable; `None` when read.
+    pub providers_error: Option<String>,
 }
 
 fn layout_of(run: &Path) -> &'static str {
@@ -1066,6 +1070,8 @@ pub fn projection(
         Err(e) => (false, Some(e)),
     };
     let (boot, boot_error) = crate::orchestration::boot_control::projection(project);
+    let (providers, providers_error) =
+        crate::orchestration::provider_registry::projection(project);
     AnalyticsSnapshot {
         at_unix_ms: 0,
         runner_busy,
@@ -1076,6 +1082,8 @@ pub fn projection(
         boot,
         boot_error,
         until_stopped: crate::orchestration::until_stopped_view::projection(project),
+        providers,
+        providers_error,
     }
 }
 
diff --git a/project_0/native/crates/runtime/src/orchestration/provider_registry.rs b/project_0/native/crates/runtime/src/orchestration/provider_registry.rs
index 2dc0c6e2e1e765f7d8951d135db354daf05d1e71..51d4c2b118551e89749b8e1a777d12ec08ba24be 100644
--- a/project_0/native/crates/runtime/src/orchestration/provider_registry.rs
+++ b/project_0/native/crates/runtime/src/orchestration/provider_registry.rs
@@ -887,6 +887,97 @@ pub fn touches_credentials(action: &ProviderAction) -> bool {
     )
 }
 
+/// DEC-18 (P0-020): the registry's secret-free projection (topic project0.analytics.jobs): every
+/// provider with its host (never the endpoint's path), authentication name, credential id,
+/// binding and selection block, every credential's newest revision.
+#[derive(Clone, Debug, Default, PartialEq, Eq)]
+pub struct RegistryView {
+    pub version: u64,
+    pub providers: Vec<ProviderView>,
+    pub credentials: Vec<CredentialView>,
+}
+
+/// One provider of [`RegistryView`].
+#[derive(Clone, Debug, PartialEq, Eq)]
+pub struct ProviderView {
+    pub id: String,
+    pub kind: &'static str,
+    pub host: String,
+    pub auth: String,
+    pub credential: Option<String>,
+    pub priority: u32,
+    pub capacity_rps: Option<u64>,
+    pub plan: &'static str,
+    pub entitlement: String,
+    pub qualification: Option<String>,
+    pub enabled: bool,
+    pub selected: bool,
+    /// `None` while its credential is revoked or unknown.
+    pub binding: Option<String>,
+    pub selection_block: Option<String>,
+}
+
+/// One credential of [`RegistryView`].
+#[derive(Clone, Debug, PartialEq, Eq)]
+pub struct CredentialView {
+    pub id: String,
+    pub revision: u64,
+    pub state: &'static str,
+    pub origin: String,
+    pub created_unix_s: u64,
+}
+
+/// The registry's view of `project`, or why it is not readable (a copied store included).
+pub fn projection(project: &Path) -> (Option<RegistryView>, Option<String>) {
+    let registry = match current(project).and_then(|r| owned(project, &r).map(|()| r)) {
+        Ok(r) => r,
+        Err(e) => return (None, Some(e)),
+    };
+    let mut providers: Vec<ProviderView> = registry
+        .providers
+        .iter()
+        .map(|p| ProviderView {
+            id: p.id.clone(),
+            kind: p.kind.name(),
+            host: p.host(),
+            auth: p.auth.text(),
+            credential: p.credential.clone(),
+            priority: p.priority,
+            capacity_rps: p.capacity_rps,
+            plan: p.plan.name(),
+            entitlement: p.entitlement.clone(),
+            qualification: p.qualification.clone(),
+            enabled: p.enabled,
+            selected: registry.selected(p.kind).is_some_and(|s| s.id == p.id),
+            binding: registry.binding_of(p).ok(),
+            selection_block: registry.selection_block(p),
+        })
+        .collect();
+    providers.sort_by(|a, b| (a.kind, a.priority, &a.id).cmp(&(b.kind, b.priority, &b.id)));
+    let credentials = registry
+        .credentials
+        .iter()
+        .map(|c| CredentialView {
+            id: c.id.clone(),
+            revision: c.revision,
+            state: match c.state {
+                CredentialState::Active => "active",
+                CredentialState::Revoked => "revoked",
+            },
+            origin: c.origin.clone(),
+            created_unix_s: c.created_unix_s,
+        })
+        .collect();
+    (
+        Some(RegistryView {
+            version: registry.version,
+            providers,
+            credentials,
+        }),
+        None,
+    )
+}
+
 /// The operator's view of the current version: one line per provider (host only, never a path
 /// or a credential) with its binding and why it cannot be selected, and one per credential (id,
 /// revision, state, origin).
@@ -1178,6 +1269,58 @@ mod tests {
         let _ = std::fs::remove_dir_all(&copy);
     }
 
+    /// DEC-18 (P0-020): the registry's projection is secret-free — every provider with its host
+    /// (never the endpoint's path), authentication name, credential id, binding, selection and why
+    /// it cannot be selected, every credential's revision; an empty store is version 0, a copied
+    /// one is refused with its reason (nothing projected).
+    #[test]
+    fn the_projection_names_hosts_and_revisions_never_a_secret() {
+        let d = project("projection");
+        let store = DirectoryStore(d.join("keychain"));
+        let (empty, error) = projection(&d);
+        assert_eq!((empty.unwrap().version, error), (0, None));
+        let mut p = helius();
+        p.endpoint = "https://mainnet.helius-rpc.com/v0/archive".into();
+        for (n, action, secret) in [
+            (1, ProviderAction::Add(p), None),
+            (2, set(), Some(&b"PROJECTED-KEY"[..])),
+            (
+                3,
+                ProviderAction::Select(ProviderKind::Rpc, "helius-rpc".into()),
+                None,
+            ),
+        ] {
+            let r = decide(&d, &format!("p{n}"), None, &action, secret, &store, n).unwrap();
+            assert!(r.applied, "{r:?}");
+        }
+        let (view, error) = projection(&d);
+        let view = view.unwrap();
+        assert_eq!((view.version, error), (3, None));
+        let p = &view.providers[0];
+        assert_eq!(
+            (p.id.as_str(), p.kind, p.host.as_str(), p.auth.as_str(), p.selected),
+            ("helius-rpc", "rpc", "mainnet.helius-rpc.com", "query:api-key", true)
+        );
+        assert_eq!(p.binding.as_ref().map(String::len), Some(64));
+        assert_eq!(p.selection_block, None);
+        assert_eq!(
+            (view.credentials[0].revision, view.credentials[0].state),
+            (1, "active")
+        );
+        let shown = format!("{view:?}");
+        assert!(!shown.contains("PROJECTED-KEY") && !shown.contains("/v0/archive"), "{shown}");
+        let copy = project("projection_copy");
+        std::fs::create_dir_all(copy.join(STORE)).unwrap();
+        for e in std::fs::read_dir(d.join(STORE)).unwrap() {
+            let e = e.unwrap();
+            std::fs::copy(e.path(), copy.join(STORE).join(e.file_name())).unwrap();
+        }
+        let (none, error) = projection(&copy);
+        assert!(none.is_none() && error.unwrap().starts_with("PROVIDER_REGISTRY_FOREIGN"));
+        let _ = std::fs::remove_dir_all(&d);
+        let _ = std::fs::remove_dir_all(&copy);
+    }
+
     /// DEC-18: a provider and its credential through the store — the secret is stored under its
     /// revision's account before the version naming it is committed, never in a record or a
     /// receipt; a selection needs an active credential; a rotation stores the next revision and
diff --git a/project_0/native/crates/runtime/src/status_tap.rs b/project_0/native/crates/runtime/src/status_tap.rs
index 24326c59b13714b0f9cd6b15185830b030ea2340..3547fc87836d80413d72e02463a4ce8db7426729 100644
--- a/project_0/native/crates/runtime/src/status_tap.rs
+++ b/project_0/native/crates/runtime/src/status_tap.rs
@@ -8,6 +8,7 @@ pub use darkbot_solana::provider::{
     CooldownCause, CooldownStatus, ProviderSnapshot, RetryAfterHeader,
 };
 use darkbot_solana::runtime_config;
+pub use darkbot_solana::providers::{Plan, ProviderBinding, ProviderKind, PROVIDER_KINDS};
 pub use darkbot_solana::runtime_config::{defaults, EffectiveConfig, Limits, Profile};
 pub use darkbot_solana::state::StateCounters;
 use std::sync::{Arc, Condvar, Mutex, MutexGuard};
diff --git a/protocol/golden/project0/analytics_jobs.hex b/protocol/golden/project0/analytics_jobs.hex
index 95fbd8679122e5b4be4bec820894a04e07e74b07..5c44404508026c8ccbfcb723fe07f73f992997d3 100644
--- a/protocol/golden/project0/analytics_jobs.hex
+++ b/protocol/golden/project0/analytics_jobs.hex
@@ -1 +1 @@
-08b897b3c19c3310011aa2070a0572756e2d61120b646174612f65636f6e34681a094e41544956455f5632200130e480b3c19c3342ae050a05696e64657812406161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616118022a0e494e4445585f56455249464945443a40626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262624a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363635007580560e0fd036a05706167657370028201090a07434c41535349438201100a0c52454345495645525348495010018201130a11464f524345445f44454c455645524147458201170a1542414e4b5255505443595f534554544c454d454e548a01090a07434c41535349438a01100a0c52454345495645525348495010018a01130a11464f524345445f44454c455645524147458a01170a1542414e4b5255505443595f534554544c454d454e5492011b0a196c656e64696e675f6163636f756e745f6c69717569646174659201150a1173746172745f6c69717569646174696f6e10029201130a0f656e645f6c69717569646174696f6e10029201120a1073746172745f64656c657665726167659201100a0e656e645f64656c657665726167659201200a1e6c656e64696e675f706f6f6c5f68616e646c655f62616e6b7275707463799a01090a054d4154434810019a01300a2c4d495353494e475f4c49515549444154494f4e5f57495448494e5f4f425345525645445f494e54455256414c1001a80100b00100ba011a0a16424f545f4f4e4c595f494e4445585f4d495353494e471001ba01090a054d415443481001ba01100a0c4d495353494e475f4c4956451002c00109c801d0fd0542c6010a0a63726f7373636865636b12406464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646418012a074e4f545f52554e322243524f5353434845434b5f5345434f4e445f50524f56494445525f4d495353494e474a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636a0570616765731a5b0a0572756e2d62120b646174612f65636f6e34681a094e41544956455f563220012a310a0a554e5245414441424c452a235155414c494649434154494f4e5f554e5245414441424c453a204652414d455f43524330e480b3c19c33228d010a056f70742d3112084f5054494d495a451a0572756e2d61220752554e4e494e47308080b3c19c3338e887b3c19c33483c508080405a250a090a0731353030303030120a8094ebdc0380a8d6b9071a0c4a49545f45584143545f494e622f0a090a07313530303030300a090a07323030303030301200120e0a0c4a49545f45584143545f494e18b8172004280122aa040a0462742d3112084241434b544553541a0572756e2d612209434f4d504c45544544308080b3c19c3338f483b3c19c33483c508080405a250a090a0731353030303030120a8094ebdc0380a8d6b9071a0c4a49545f45584143545f494e72ca030a4072727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272120572756e2d6118012002280332cd0208011002180320042805300638074263088094ebdc0310011a2a0a282d31373031343131383334363034363932333137333136383733303337313538383431303537323820012a290a273334303238323336363932303933383436333436333337343630373433313736383231313435353001426c0880a8d6b90710021a2a0a282d31373031343131383334363034363932333137333136383733303337313538383431303537323820012a290a27333430323832333636393230393338343633343633333734363037343331373638323131343535300142070a052d323030304a08554e50524f56454e52604e4f545f4556414c55415445443a206120536861646f77207265636f72642063617272696573206e6f20736166657479206465636973696f6e2028536861646f77206576616c756174657320776974686f757420612073656e642067617465293a074e4f545f52554e4220090909090909090909090909090909090909090909090909090909090909090922550a0470632d31120c504f53545f434150545552451a0572756e2d612209434f4d504c45544544308080b3c19c3338d884b3c19c3340bc85b3c19c33483c508080408a0102100182010c0a067374726963741001180122e7010a0468302d31120248302209434f4d504c45544544308080b3c19c3338a086b3c19c33408487b3c19c33483c508080409201a8010a0468302d311080d6ffc5061880f09dc706200c280330023801400448882750025a144d494e5f53414d504c455f554e44454349444544625b424c4f434b45443a2048302d30342028746167732c2074616b652074696d65732c207072656d69756d2874292c206f757220745f70726f66697461626c6529206e656564732074686520302e312e3132206465706c6f796d656e74680a700b7805800180a3058801019001019801089a0108081e1880f09dc706229d010a0468302d30120248302209434f4d504c45544544308080b3c19c3338a0f2c7e498334084f3c7e49833483c5080804092015f0a0468302d3010c0d1c2c50618c0ebe0c60620073001400248d83650015a144d494e5f53414d504c455f554e44454349444544622a424c4f434b45443a2048302d3034206e656564732074686520302e312e3132206465706c6f796d656e749a0108081e18c0ebe0c6063a9f010a06666c6f772d31120665636f6e346818e887b3c19c33221d4f46464c494e455f52454144595f4e4554574f524b5f4e4f545f52554e2a1030313233343536373839616263646566385a4203636c694a190a09434f4d504c4554454420cc88b3c19c3328ec8eb3c19c33520552454144595a10666564636261393837363534333231306881f09dc70670dbf09dc7067a090a074e4f545f52554e82010231684a97060aad020a103537633735353031393661383039313912066d61726b65741a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363632240626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262622a14756e74696c2d73746f707065643a31683a32346830901c3880a3054210303132333435363738396162636465664a1038396162636465663031323334353637522c3565796b7434557346763850384e4a64545245705931767a714b715a4b7664704b75633134376477324e39645a046c6f6773600568904e7080047880f5248001c0bfc2c7068801e4f09dc70612ad020a103731636132663135363833643065363612066d61726b65741a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363632240626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262622a14756e74696c2d73746f707065643a31683a32346830901c3880a3054210303132333435363738396162636465664a1038396162636465663031323334353637522c3565796b7434557346763850384e4a64545245705931767a714b715a4b7664704b75633134376477324e39645a046c6f6773600568904e7080047880a3058001c0bfc2c7068801c8f19dc7061a670a103031323334353637383961626364656612066d61726b657418901c2080a305322663617061636974793a204e4f545f52554e20c2b72066726565206469736b20756e6b6e6f776e3a103839616263646566303132333435363740c0bfc2c7064880f09dc706224c0a086d61726b65742d311210303132333435363738396162636465661880f09dc70620901c28023208434f4e54494e55453a0f657665727920676174652068656c6440a0a89ec70650015801
\ No newline at end of file
+08b897b3c19c3310011aa2070a0572756e2d61120b646174612f65636f6e34681a094e41544956455f5632200130e480b3c19c3342ae050a05696e64657812406161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616118022a0e494e4445585f56455249464945443a40626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262624a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363635007580560e0fd036a05706167657370028201090a07434c41535349438201100a0c52454345495645525348495010018201130a11464f524345445f44454c455645524147458201170a1542414e4b5255505443595f534554544c454d454e548a01090a07434c41535349438a01100a0c52454345495645525348495010018a01130a11464f524345445f44454c455645524147458a01170a1542414e4b5255505443595f534554544c454d454e5492011b0a196c656e64696e675f6163636f756e745f6c69717569646174659201150a1173746172745f6c69717569646174696f6e10029201130a0f656e645f6c69717569646174696f6e10029201120a1073746172745f64656c657665726167659201100a0e656e645f64656c657665726167659201200a1e6c656e64696e675f706f6f6c5f68616e646c655f62616e6b7275707463799a01090a054d4154434810019a01300a2c4d495353494e475f4c49515549444154494f4e5f57495448494e5f4f425345525645445f494e54455256414c1001a80100b00100ba011a0a16424f545f4f4e4c595f494e4445585f4d495353494e471001ba01090a054d415443481001ba01100a0c4d495353494e475f4c4956451002c00109c801d0fd0542c6010a0a63726f7373636865636b12406464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646464646418012a074e4f545f52554e322243524f5353434845434b5f5345434f4e445f50524f56494445525f4d495353494e474a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636a0570616765731a5b0a0572756e2d62120b646174612f65636f6e34681a094e41544956455f563220012a310a0a554e5245414441424c452a235155414c494649434154494f4e5f554e5245414441424c453a204652414d455f43524330e480b3c19c33228d010a056f70742d3112084f5054494d495a451a0572756e2d61220752554e4e494e47308080b3c19c3338e887b3c19c33483c508080405a250a090a0731353030303030120a8094ebdc0380a8d6b9071a0c4a49545f45584143545f494e622f0a090a07313530303030300a090a07323030303030301200120e0a0c4a49545f45584143545f494e18b8172004280122aa040a0462742d3112084241434b544553541a0572756e2d612209434f4d504c45544544308080b3c19c3338f483b3c19c33483c508080405a250a090a0731353030303030120a8094ebdc0380a8d6b9071a0c4a49545f45584143545f494e72ca030a4072727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272727272120572756e2d6118012002280332cd0208011002180320042805300638074263088094ebdc0310011a2a0a282d31373031343131383334363034363932333137333136383733303337313538383431303537323820012a290a273334303238323336363932303933383436333436333337343630373433313736383231313435353001426c0880a8d6b90710021a2a0a282d31373031343131383334363034363932333137333136383733303337313538383431303537323820012a290a27333430323832333636393230393338343633343633333734363037343331373638323131343535300142070a052d323030304a08554e50524f56454e52604e4f545f4556414c55415445443a206120536861646f77207265636f72642063617272696573206e6f20736166657479206465636973696f6e2028536861646f77206576616c756174657320776974686f757420612073656e642067617465293a074e4f545f52554e4220090909090909090909090909090909090909090909090909090909090909090922550a0470632d31120c504f53545f434150545552451a0572756e2d612209434f4d504c45544544308080b3c19c3338d884b3c19c3340bc85b3c19c33483c508080408a0102100182010c0a067374726963741001180122e7010a0468302d31120248302209434f4d504c45544544308080b3c19c3338a086b3c19c33408487b3c19c33483c508080409201a8010a0468302d311080d6ffc5061880f09dc706200c280330023801400448882750025a144d494e5f53414d504c455f554e44454349444544625b424c4f434b45443a2048302d30342028746167732c2074616b652074696d65732c207072656d69756d2874292c206f757220745f70726f66697461626c6529206e656564732074686520302e312e3132206465706c6f796d656e74680a700b7805800180a3058801019001019801089a0108081e1880f09dc706229d010a0468302d30120248302209434f4d504c45544544308080b3c19c3338a0f2c7e498334084f3c7e49833483c5080804092015f0a0468302d3010c0d1c2c50618c0ebe0c60620073001400248d83650015a144d494e5f53414d504c455f554e44454349444544622a424c4f434b45443a2048302d3034206e656564732074686520302e312e3132206465706c6f796d656e749a0108081e18c0ebe0c6063a9f010a06666c6f772d31120665636f6e346818e887b3c19c33221d4f46464c494e455f52454144595f4e4554574f524b5f4e4f545f52554e2a1030313233343536373839616263646566385a4203636c694a190a09434f4d504c4554454420cc88b3c19c3328ec8eb3c19c33520552454144595a10666564636261393837363534333231306881f09dc70670dbf09dc7067a090a074e4f545f52554e82010231684a97060aad020a103537633735353031393661383039313912066d61726b65741a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363632240626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262622a14756e74696c2d73746f707065643a31683a32346830901c3880a3054210303132333435363738396162636465664a1038396162636465663031323334353637522c3565796b7434557346763850384e4a64545245705931767a714b715a4b7664704b75633134376477324e39645a046c6f6773600568904e7080047880f5248001c0bfc2c7068801e4f09dc70612ad020a103731636132663135363833643065363612066d61726b65741a40636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363632240626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262622a14756e74696c2d73746f707065643a31683a32346830901c3880a3054210303132333435363738396162636465664a1038396162636465663031323334353637522c3565796b7434557346763850384e4a64545245705931767a714b715a4b7664704b75633134376477324e39645a046c6f6773600568904e7080047880a3058001c0bfc2c7068801c8f19dc7061a670a103031323334353637383961626364656612066d61726b657418901c2080a305322663617061636974793a204e4f545f52554e20c2b72066726565206469736b20756e6b6e6f776e3a103839616263646566303132333435363740c0bfc2c7064880f09dc706224c0a086d61726b65742d311210303132333435363738396162636465661880f09dc70620901c28023208434f4e54494e55453a0f657665727920676174652068656c6440a0a89ec7065001580152be03080712ad010a0a68656c6975732d72706312037270631a166d61696e6e65742e68656c6975732d7270632e636f6d220d71756572793a6170692d6b65792a0668656c697573380a4204667265654a0b68656c6975732d66726565521030313233343536373839616263646566580160016a406262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626262626212a5010a0a7075626c69632d77737312037773731a1b6170692e6d61696e6e65742d626574612e736f6c616e612e636f6d22046e6f6e6530054204667265654a067075626c69636a4063636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363636363721d50524f56494445525f44495341424c45443a207075626c69632d7773731a1f0a0668656c69757310031a066163746976652205696e7075742880f09dc7061a2a0a036f6c6410011a077265766f6b65642212656e763a48454c4955535f4150495f4b45592880c3bbc20622156e6578745f636f6e74726f6c6c65645f7374617274
\ No newline at end of file
diff --git a/protocol/schema_id.txt b/protocol/schema_id.txt
index e5afec60fe612c654a3e87ff2420f18901ac6659..843a3d475fff64e5725861c728df7c7ed576bc14 100644
--- a/protocol/schema_id.txt
+++ b/protocol/schema_id.txt
@@ -1 +1 @@
-28e8a26a29c3abd73d427e6f08b8cbc6203d14ca6c240a96aba87b6163955ddb
\ No newline at end of file
+09470f3d52329639999b1da38d345cddfb55f77b43f85a8d1a994b0ee17b5d49
\ No newline at end of file
diff --git a/protocol/schemas/darkbot/project0/v1/analytics.proto b/protocol/schemas/darkbot/project0/v1/analytics.proto
index cb73856acc6ba8e172136c1cad08be15de409453..f725a3da516dfc5fd243bb77e5353e08b9522798 100644
--- a/protocol/schemas/darkbot/project0/v1/analytics.proto
+++ b/protocol/schemas/darkbot/project0/v1/analytics.proto
@@ -32,6 +32,133 @@ message AnalyticsJobs {
   // DEC-16: unbounded runs beside the boot flow — the standing autostart approval, the newest
   // raw-capture qualification and the newest unbounded raw capture.
   UntilStopped until_stopped = 9;
+  // DEC-18: the provider registry (data/providers, `p0ctl provider …`) as its store holds it —
+  // staged: a change is effective at the next controlled start (the running run's providers:
+  // project0.status EffectiveRecord.providers). Unset when the store is unreadable.
+  ProviderRegistryView providers = 10;
+  // DEC-18: why the provider registry was not readable (providers is then unset); empty when
+  // read.
+  string providers_error = 11;
+}
+
+// DEC-18: one committed version of the provider registry — secret-free (hosts, credential ids
+// and revisions; never an endpoint's path, a key or a keychain account).
+message ProviderRegistryView {
+  // The committed version (0: no provider yet); the expected version of every change (CAS).
+  uint64 version = 1;
+  // Every provider, by kind, priority and id.
+  repeated RegistryProvider providers = 2;
+  // Every credential's newest revision.
+  repeated RegistryCredential credentials = 3;
+  // When a change takes effect: next_controlled_start (DEC-05).
+  string restart_rule = 4;
+}
+
+// DEC-18: one provider of the registry.
+message RegistryProvider {
+  // Provider id ([a-z0-9_-]{1,32}).
+  string id = 1;
+  // rpc | wss | geyser | jupiter | jito.
+  string kind = 2;
+  // The endpoint's host (its path and scheme stay in the engine).
+  string host = 3;
+  // How it authenticates: none | query:NAME | header:NAME | path (a name, never a value).
+  string auth = 4;
+  // The credential it authenticates with (its id); empty without authentication.
+  string credential = 5;
+  // Lower first among the providers of its kind.
+  uint32 priority = 6;
+  // Configured capacity in requests per second; unset when not configured.
+  optional uint64 capacity_rps = 7;
+  // free | unknown (FREE_ONLY: paid is refused, unknown never selected).
+  string plan = 8;
+  // The plan's name as the provider publishes it.
+  string entitlement = 9;
+  // The provider evidence record that qualified its binding; unset when not qualified.
+  optional string qualification = 10;
+  // Enabled (a disabled provider is never selected).
+  bool enabled = 11;
+  // The selected provider of its kind.
+  bool selected = 12;
+  // Its secret-free binding (hex SHA-256) with the active credential revision; unset when its
+  // credential is revoked or unknown.
+  optional string binding = 13;
+  // Why it cannot be the selected one of its kind now; unset when it can.
+  optional string selection_block = 14;
+}
+
+// DEC-18: a credential's newest revision (the secret is the macOS keychain's alone).
+message RegistryCredential {
+  // Credential id.
+  string id = 1;
+  // Its newest revision (a rotation is a new one, never reused).
+  uint64 revision = 2;
+  // active | revoked.
+  string state = 3;
+  // input | env:NAME (the one-time .env import).
+  string origin = 4;
+  // When the revision was created.
+  uint64 created_unix_s = 5;
+}
+
+// DEC-18: params of `project0.provider.add` and `project0.provider.update` (the registry's
+// rules decide: a canonical endpoint without a credential in it, FREE_ONLY, a known kind). An
+// update changes only the fields it sets.
+message ProviderDescriptorCommand {
+  // Provider id.
+  string id = 1;
+  // rpc | wss | geyser | jupiter | jito (add: required).
+  optional string kind = 2;
+  // The endpoint (add: required); refused with a credential in it, never echoed.
+  optional string endpoint = 3;
+  // none | query:NAME | header:NAME | path.
+  optional string auth = 4;
+  // The credential id it authenticates with.
+  optional string credential = 5;
+  // Priority among its kind.
+  optional uint32 priority = 6;
+  // Configured capacity in requests per second.
+  optional uint64 capacity_rps = 7;
+  // free | unknown.
+  optional string plan = 8;
+  // The plan's name.
+  optional string entitlement = 9;
+  // Added disabled (add only).
+  bool disabled = 10;
+}
+
+// DEC-18: params of `project0.provider.priority`.
+message ProviderPriorityCommand {
+  // Provider id.
+  string id = 1;
+  // The new priority.
+  uint32 priority = 2;
+}
+
+// DEC-18: params of `project0.provider.select`.
+message ProviderSelectCommand {
+  // rpc | wss | jupiter | jito.
+  string kind = 1;
+  // Provider id.
+  string id = 2;
+}
+
+// DEC-18: params of `project0.provider.qualify`.
+message ProviderQualifyCommand {
+  // Provider id.
+  string id = 1;
+  // The stored provider evidence record id (16 hex), or "-" to clear the qualification.
+  string evidence = 2;
+}
+
+// DEC-18: params of `project0.provider.credential.set` and `…credential.rotate`. The gateway
+// separates the secret before any durable record (its command ledger holds the request without
+// it) and hands it to the native controller's standard input only.
+message ProviderCredentialCommand {
+  // Credential id.
+  string credential = 1;
+  // The secret (in transit only).
+  bytes secret = 2;
 }
 
 // DEC-16: unbounded runs (RunHorizon::UntilStopped) as the stores hold them. Expiry and
diff --git a/protocol/schemas/darkbot/project0/v1/system.proto b/protocol/schemas/darkbot/project0/v1/system.proto
index ce370ae63fa534bbb177d6beb317f702509441d6..5eed3d52cb631d6a65354e333a2b1958f4ffa393 100644
--- a/protocol/schemas/darkbot/project0/v1/system.proto
+++ b/protocol/schemas/darkbot/project0/v1/system.proto
@@ -80,6 +80,27 @@ message EffectiveRecord {
   string source_geyser = 20;
   // Provider of the Geyser source unit.
   string source_geyser_provider = 21;
+  // DEC-18: the providers the run is bound to (runtime_config::ProviderBinding), secret-free and
+  // without their endpoints; empty in a record of an earlier revision.
+  repeated RunProvider providers = 22;
+}
+
+// DEC-18: one provider a run is bound to.
+message RunProvider {
+  // rpc | wss | geyser | jupiter | jito.
+  string kind = 1;
+  // Provider id (local: a named loopback endpoint; jupiter-default: the contract's profile).
+  string id = 2;
+  // Its secret-free binding (hex SHA-256).
+  string binding = 3;
+  // The registry version it was bound from; 0 for a local endpoint or a default profile.
+  uint64 registry_version = 4;
+  // The credential id it authenticates with; empty without.
+  string credential_id = 5;
+  // The credential revision bound.
+  uint64 credential_revision = 6;
+  // free | paid | unknown.
+  string plan = 7;
 }
 
 // Operational configuration of the process.
-- 
2.53.0

~~~~~~~~~~~~~~~~
<!-- DEC18-PATCH-END -->
