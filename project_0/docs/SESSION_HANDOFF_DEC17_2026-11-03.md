# SESSION_HANDOFF DEC-17 — 03.11.2026 (Project_0, Claude-Session-Wechsel)

> **Zweck:** Die nächste Claude-Session soll ohne Wissensverlust und in derselben Qualität weiterarbeiten.
> Dieses Dokument nennt die verbindlichen Aufträge in Rangfolge, die Regeln, den genauen Git- und
> Arbeitsstand, die nächsten Schritte, die Werkzeuge mit Quelltext, die Fallstricke und die offenen
> Nutzerentscheidungen.
> Es **ersetzt keine** der verbindlichen Quellen: PHASENPLAN, ENTSCHEIDUNGEN, STATUS, BUGS und die
> Auto-Memory. Es sagt, wo sie stehen und was gerade gilt.
> Bei Widerspruch gilt die jeweils neueste Nutzeranweisung, dann PHASENPLAN/ENTSCHEIDUNGEN, dann dieses
> Dokument.

---

## 0. Sofort-Einstieg (in dieser Reihenfolge)

1. Lies dieses Dokument vollständig.
2. Die Auto-Memory wird automatisch geladen: `MEMORY.md` unter
   `/Users/sawyer_ma/.claude/projects/-Users-sawyer-ma-Desktop-Firma-2-DarkMa-6-DarkBot-DarkBot/memory/`.
   Lies dort mindestens diese Dateien vollständig:
   - `project0-order-20261102-final-rev1.md`
   - `project0-order-20261103-dec17-sequence.md`
   - `feedback-p0-efficiency-20261102.md`
   - `project0-prod-ctrl-binding.md`
   - `bsc-absolute-boundary.md`
   - `project0-isolated-worktree.md`
   - `feedback-minimal-docs.md`
   - `feedback-schema-id-bsc-golden.md`
3. Lies im PHASENPLAN den DEC-17-Abschnitt:
   `project_0/docs/PHASENPLAN.md`, Zeile ≈5819 „DEC-17 — Operator-Auth, Wallet-Modell, …".
   Lies außerdem P8-CTRL.9–.11 (Zeilen ≈4040–4090).
4. Prüfe den Git-Stand (§7) **bevor** du irgendetwas änderst.
5. Arbeite die nächsten Schritte in §9 ab. Es gibt **eine offene Nutzerfrage** (§9, Schritt 0); stelle
   sie gleich zu Beginn.

---

## 1. Projekt, Pfade, Beteiligte

- **Projekt:** DarkBot. Darin sind zwei voneinander getrennte Projekte:
  - **Project_0** (Solana / marginfi Liquidation, Rust-Core) — **deine** Zuständigkeit.
  - **BSC** (`bsc/`) — **fremdes Projekt**, gehört der BSC-Session.
- **Kanonischer lokaler Checkout:** `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`
  - Branch `main`, wird **nur** fast-forward aktualisiert: `git merge --ff-only origin/main`.
  - Dort liegen ungetrackte BSC-Dateien (Liste aus `git status`); **nie anfassen**.
- **Einziger Project_0-Arbeits-Worktree:** `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-work`
  - detached auf origin/main, **kein Branch**.
  - Alle Änderungen, Gates und Pushes laufen hier. Lege **keine** weiteren Worktrees oder Branches an.
- **Remote:** `https://github.com/sawyerma/darkbot.git`. Push mit `git push origin HEAD:main`, nur
  fast-forward, **nie** force.
- **BSC-Session:**
  - Name `BSC_v1`.
  - Erreichbar über das Tool `SendMessage`. Die Adresse vorher mit `ListAgents` holen; die letzte war
    `uds:/tmp/cc-socks/85281.sock` und kann sich ändern.
  - BSC_v1 arbeitet nur im eigenen Worktree `DarkBot-bsc-rev1/wtEA`.
- **Nutzer:** Sawyer (s.maier@dct-delta.de).
  - Kommunikation **auf Deutsch**, kurz.
  - Der Nutzer prüft GitHub selbst: Erst ein gepushter SHA mit grüner CI gilt als bestätigt.
- **Sprache im Code:** Kommentare und Commit-Messages englisch, im vorhandenen Stil.
  - STATUS-Zeilen: englisch/deutsch gemischt, je eine Zeile.
  - ENTSCHEIDUNGEN/PHASENPLAN: deutsch.

---

## 2. Verbindliche Aufträge — Rangfolge und Inhalt

### 2.1 „LETZTER VERBINDLICHER ARBEITSAUFTRAG – Project_0 REV.1.0" (02.11.2026)

Memory: `project0-order-20261102-final-rev1`.

1. FE-01 offline vollständig, danach ein Offline-Nachweis aus Realdaten.
   - Limits: 12 Calls, 1 RPS, 120 s, 48 MiB.
   - **Erledigt.** C-01 `edf159af0e91fb03` steht auf `P1A_PARITY PASS`, Schritt 7 ist promoviert
     (Referenz `5BrAnWXJ…`, `mrgn-0.1.12-rc2`).
2. **Nie ein Mainnet-Lauf mit Code ohne Gate+CI.**
3. Ablauf pro Paket:
   - vollständig implementieren;
   - betroffene Tests;
   - Fixes gesammelt;
   - höchstens **ein** Review;
   - **ein** Gate auf dem finalen Code;
   - Push nach `main`;
   - CI auf exakt diesem SHA.
4. Danach in dieser Reihenfolge:
   - C-11/P0-024-Evidence-Gültigkeit;
   - FE-01-Plan + Token;
   - c01 fe01 + Parität;
   - BOOT-05 nur bei vollem PASS;
   - **C-04 → C-05 → C-10** (inkl. P0-027);
   - dann **alles** restliche REV.1.0/DEC-17. Nichts fällt weg.
5. Keine Fragen zu entschiedenen Punkten. Keine Freigabeanfragen für Offline-Arbeit. Keine langen
   Statusmeldungen. Keine erfundenen Betreiberentscheidungen.
6. **Berichte enthalten nur:**
   - fertige Funktionen;
   - bestätigte Bugs mit Fix;
   - bestandene Prüfungen;
   - SHA + CI;
   - verbleibende REV.1.0-Pakete;
   - echte Betreiberentscheidungen.
   Unverifiziertes heißt NOT_RUN, PARTIAL oder BLOCKED.
7. **Endziel:** REV.1.0 vollständig bis zur technischen Gesamtabnahme. Nach C-10 **nicht** stoppen.

### 2.2 DEC-17-Reihenfolge (03.11.2026)

Memory: `project0-order-20261103-dec17-sequence`. Vom Nutzer nach Prüfung bestätigt („passt alles weiter").

- Offline-Reihenfolge: **DEC-17f → DEC-17c → DEC-17b → DEC-17d → P8-CTRL.11**, jeweils mit den
  tatsächlichen Abhängigkeiten.
- **DEC-17b vor DEC-17d**, damit der Exit-Pfad nicht zweimal umgebaut wird.
- **DEC-17d:** Unbekannte WSOL-Bestände dürfen **nie** ignoriert werden. Die Abrechnung muss nachweislich
  positions- und walletgenau bleiben.
- **C-04 → C-05 → C-10 bleiben Pflicht.** Sie brauchen Run-Tokens bzw. -Freigaben des Nutzers und werden
  nach den DEC-17-Paketen nachgeholt.
- Ein lokaler SHA gilt erst nach Push + CI als bestätigt.

### 2.3 Effizienzauftrag (02.11.2026)

Memory: `feedback-p0-efficiency-20261102`, `feedback-test-efficiency-units`.

- Zusammenhängende Funktionen vollständig und integriert bauen. Keine künstlich kleinen Pakete, keine
  Architektur-Entwürfe, keine Extra-Inventuren.
- Während der Implementierung nur gezielte Tests. RED→GREEN bei unabhängigen Bugs. Mutation nur für
  kritische geänderte Logik.
- **Ein** volles Gate erst auf dem finalen Code eines Pakets. Kein Gate nach Einzeländerungen. Ein Gate
  nie mitten im Paket abbrechen.
- Höchstens ein optionaler Read-only-Reviewer, nur bei kritischem oder komplexem Code. Review-Fixes
  gesammelt.
- Goldens, Re-Pins und Schema-IDs nur, wenn wirklich nötig.
- **Push mindestens alle ≤30 min**, wenn ein konsistenter grüner Zwischenstand existiert
  (`feedback-push-every-30min`). Das Paket-Gate und die STATUS-Einträge kommen am Paketende.

### 2.4 Ältere, weiter gültige Grundaufträge (Auszug)

- **REV.1.0 (27.10.):** Der ganze PHASENPLAN REV.1.0 ist Pflicht. Nichts wird entfernt oder eigenmächtig
  nach REV.2.0/3.0 verschoben (`project0-rev1-20261027`).
- **P0-PROD-CTRL (25.10.):** No Mock, No Legacy, No JSON. Ein nativer Controller. Voller Status- und
  Command-Rundweg bis in den Reiter (`project0-prod-ctrl-binding`, siehe §4).
- **Fertig-Kriterium (25.10.):** Eine Funktion ist erst fertig, wenn sie produktiv verdrahtet ist und den
  Frontend-Rundweg hat; ein grüner Test allein reicht nicht (`project0-fix-all-completion-criteria`).
- **Abnahmestandard (20.10.):** fail-before-Test; alte Binärlayouts lesbar; PARTIAL/BLOCKED ehrlich
  (`project0-acceptance-standard`).
- **Minimal-Doku:** je Paket eine Zeile in STATUS/BUGS. Commit-Message = Betreff + höchstens ~5 kurze
  Zeilen. Keine Review- oder Mutations-Prosa (`feedback-minimal-docs`).
- **Shared Keychain:** nach REV.2.0 verschoben. Freeze: kein neuer Credential-Store-Code
  (`project-shared-keychain-rev2`).

---

## 3. Absolute Regeln und Verbote (wörtlich ernst nehmen)

1. **`bsc/**` nie anfassen.** Nicht ändern, löschen, formatieren, stagen, committen, resetten oder
   stashen, auch nicht „zur Reparatur". BSC-Dateien ändert nur BSC_v1.
2. **Commits nur mit Pathspec:** `git commit -m … -- <pfade>`.
   - Vorher `git status --short` prüfen.
   - Nachher `git show --stat HEAD` prüfen.
   - Taucht eine fremde Datei auf: **STOP, kein Push**.
   - Nie `git add -A` / `git add .` / `git commit -a`.
3. **Kein Branch, kein Force-Push, keine fremden Änderungen überschreiben.** Kein `rm -rf`, kein
   `git worktree remove --force`, kein `git clean -fd`, kein erzwungener Reset.
4. **Keine Prozesse anderer Projekte beenden**, insbesondere keine BSC-Prozesse. PID 44297 ist tabu,
   keine BSC-Runtime starten. In Shell-Harnesses `launchctl` stubben (`feedback-shell-harness-no-launchctl`).
5. **Keine Secrets in GitHub.**
   - Keine API-Keys, Private Keys, Wallet-Seeds oder sonstigen Geheimnisse.
   - Secrets bleiben ausschließlich in ihrem lokalen Credential-Store.
   - Secrets nie ausgeben oder loggen.
6. **Keine neuen Mainnet-Reads, Provider-Messungen, wirtschaftlichen Operationen** oder sonstigen
   freigabepflichtigen Aktionen ohne konkrete Nutzerfreigabe bzw. Token für genau diesen Lauf.
   Der Classifier blockt Mainnet-Reads ohne ausdrückliche Freigabeformulierung.
7. **Kein Signieren, kein LIVE, kein Echtgeld.** FREE_ONLY: nur kostenlose Provider.
8. **Keychain-Dialoge:** Der Nutzer klickt nicht ständig „Erlauben" („das sind meine Passwörter").
   - Keine Aktionen, die macOS-Keychain-Dialoge auslösen, ohne Vorankündigung.
   - Ein B-KC-Dummy-Test braucht vorherige Ankündigung (`project0-decision-20261101-rpy-bkc`).
9. **Ein Gate zur Zeit** pro Worktree/Target. Vorher `pgrep -fl gate.sh` prüfen. Kein
   `git checkout` in einem Worktree, in dem ein Gate läuft.
10. **Status-Frage = Stopp:** Wenn der Nutzer „sag mir bevor du weitermachst …" schreibt: antworten, dann
    nichts weiter tun, bis er „weiter" sagt (`feedback-stop-means-wait`).
11. **Bei Unsicherheit oder nach einer Kontext-Kompaktierung** zuerst PHASENPLAN (relevanter §), STATUS,
    `docs/manifests/*.md` und die Auftrags-Memory neu lesen, dann handeln. Nicht improvisieren
    (`feedback-read-plan-first`).

---

## 4. Qualitäts- und Architekturregeln (No Mock / No Legacy / …)

- **No Mock / No Stub / No Legacy:**
  - Jede Pflichtfunktion ist echter nativer Rust-Code im produktiven Pfad.
  - Keine produktiven Mocks, Platzhalter, Fake-Werte, toten oder doppelten Produktionspfade.
  - Tests nutzen die echten Produktions-APIs (Loopback-Endpunkte, Recording-Receiver, synthetische Keys).
- **No internal JSON:**
  - Intern nur typisierte Binärframes: `.p0j`-Journal, `analytics::payload` (bincode, `Payload`-Kind).
  - JSON nur am externen Wire (Solana JSON-RPC/WSS, Jupiter) und in historischen Lesern hinter dem
    Feature `historical`.
  - Vor jedem Commit den Diff nach `serde_json`, `json!` und `append_legacy` in Nicht-Wire-Code
    durchsuchen (`project0-no-internal-json`).
- **No Hardcoding:**
  - Jede einstellbare Grenze steht in der typisierten Konfiguration (`runtime_config::Limits`,
    Registry, Profile, Env), validiert, mit abgeleiteten Defaults.
  - Keine Literale in Logik oder Meldungen (`feedback-no-hardcoding`).
  - Ein neuer Limits-Parameter erhöht `LIMITS_REVISION`, zieht den Pin-Test nach und erfordert das
    `system_status`-Golden neu.
- **UNKNOWN ist nie 0:** Unbekannte Werte sind `None`/unset, nie 0. Fail closed.
- **Ein nativer Controller:**
  - `p0ctl` und das Frontend lösen dieselbe Aktion aus.
  - Statusweg: Rust-Owner → `status_tap` → `owner.proto` → Reporter → Gateway → WS → React-Adapter →
    Store → **bestehender** Reiter.
  - Rückweg: Reiter → POST `/api/v1/command` → Gateway → Engine → Receipt → STATE.
  - Die **fünf Reiter bleiben fest:** Overview, Liquidator, Capture, Analytics, Wallets.
  - `WalletsPanel.tsx` und `graphite.css` sind design-frozen (`npm run check:freeze`); neue Anzeigen
    kommen als eigene Section daneben.
- **Binärkompatibilität:**
  - Ändert sich ein persistiertes Layout, braucht es explizite alte Layout-Typen und eine Migration
    (Schema-Version); alte Journale bleiben lesbar.
  - Bei bincode-Enums neue Varianten **nur am Ende** anhängen, bestehende Varianten nie ändern.
    Beispiel: `RecordEvent::WalletRebalancePlan` neu, `RebalancePlan` bleibt lesbar.
- **Phasentreue:** Nur was der PHASENPLAN verlangt, nichts Erfundenes. Externe Gates bleiben ehrlich
  NOT_RUN oder BLOCKED.
- **Statusstufen:** `LOCAL_PASS` / `OFFLINE_VERIFIED` (Gate + CI grün) / `EXTERNAL_QUALIFIED` / `PARTIAL` /
  `BLOCKED` / `NOT_RUN`.

---

## 5. Arbeitsablauf pro Paket (konkret)

```bash
W=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-work
cd $W && git fetch -q origin && git status --short | grep -v '^?? bsc/'   # muss leer sein (außer Paketdateien)
# vor Paketbeginn: auf origin/main stehen
git checkout -q --detach origin/main      # nur wenn nichts Lokales offen ist!
# … implementieren …
cd project_0/native && cargo fmt --all
cargo clippy --locked --all-targets --all-features -- -D warnings; echo exit=$?
cargo test -p darkbot-runner --features crate_health --lib -- <filter>   # gezielt
# Commit (Pathspec!)
cd $W && git add <neue Dateien> && git commit -q -m "<betreff>" -- <pfade> && git show --stat HEAD
# EIN Gate auf dem finalen Code (Hintergrund, ~45–60 min), siehe §15.1
# grün ⇒ fetch, ggf. rebase, Dateiliste prüfen, push, CI beobachten (§15.2/15.3), main-Checkout ff
git fetch -q origin && git merge-base --is-ancestor origin/main HEAD && git push origin HEAD:main
cd /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot && git fetch -q origin && git merge --ff-only origin/main
```

- **Commit-Attribution:** Jede Commit-Message endet mit
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
  (bzw. mit der Zeile aus der System-Erinnerung der neuen Session).
- **Wenn origin/main vorgerückt ist** (meist BSC): `git rebase origin/main`.
  - Ein neues volles Gate ist nur nötig, wenn der Rebase P0-relevante Dateien mitbringt (z. B.
    `protocol/`, `frontend/src/core/`, `gateway/`, `Cargo.lock`).
  - Bei reinen `bsc/`-Änderungen genügt der Gate-Nachweis des Paket-SHA. Der Schritt `bsc_fields`
    darf erneut laufen.
  - Nach einem Rebase ändern sich SHAs: Doku-SHAs erst **nach** dem Push eintragen
    (`feedback-rebase-before-sha-docs`).
- **Ändert sich `protocol/schema_id.txt`:** **vor** dem Push BSC_v1 die neue ID schicken. BSC_v1 zeichnet
  `protocol/golden/bsc/engine.stream.hex` neu auf. Bis dahin ist die bsc-CI rot; das ehrlich berichten.
- **Ändert sich `gateway/src/control.rs`:** im selben Commit `frontend/contracts/bsc-fields.json` neu pinnen.
  Gateway-Code: `cargo fmt` + clippy mit stable.
- **Doku nach dem Push:**
  - eine STATUS-Zeile (bzw. BUGS-Zeile), SHA + CI-Run-ID;
  - eigener `docs(p0)`-Commit (Pfad `project_0/docs/**` triggert keine P0-CI).

---

## 6. Wo was steht (Dokumente)

| Was | Wo |
|---|---|
| **Phasenplan (REV.1.0, verbindlich)** | `project_0/docs/PHASENPLAN.md` (+ `PHASENPLAN.json`). DEC-17: Z. ≈5819–5834. DEC-18: ≈5836 ff. P8-CTRL.9/.10/.11: ≈4040–4090. P8-CTRL Schalter/WALLET: ≈3940–3980. v4.5.1 Ressourcenreservation: ≈3457. REV-Markierung: Z. 11 |
| **Entscheidungen** | `project_0/docs/ENTSCHEIDUNGEN.md` v1.5 — §2 Nr. 17 (DEC-17 a–g), Nr. 18 (DEC-18), §4 Freigaben C-01…C-12 |
| **Status je Ticket/Paket** | `project_0/docs/STATUS.md` — Zeile „DEC-17b wallet model" (≈376), „P8-CTRL-02/03" (≈374/375), „S3-R00…R03" (≈332), „C-11" (≈323), „P1B/C-01" (≈299). Abschnitt „1. Bottom line" ist teilweise veraltet (bekannt) |
| **Bugs/Tickets** | `project_0/docs/BUGS.md` — P0-016…P0-030 (§23), DEC-Restpunkte §22.5 |
| **Messungen** | `project_0/docs/MEASUREMENT_LEDGER.md/.json` |
| **F0-VAR (Feldinventar)** | `project_0/docs/F0_VAR_*.tsv/.md/.json` — Stand DEC-18; wird zur Phasenabnahme erneuert, nicht je DEC-17-Paket |
| **Manifeste/Deployment** | `project_0/docs/manifests/` (`CURRENT_MAINNET.md`, `UPCOMING_0_1_12.md`) |
| **REV.2.0 (nicht jetzt!)** | `project_0/docs/PHASENPLAN_REV2.0.md` (§7 nur auf ausdrückliches „Phasenplan Rev2.0 umsetzen") |
| **Vorige Übergabe** | `project_0/docs/SESSION_HANDOFF_DEC18_2026-11-01.md` (Historie DEC-18) |
| **Arbeits-Worktree-Anleitung** | `project_0/README.md` „Arbeits-Worktree" |
| **Gate-Skript** | `project_0/scripts/gate.sh` (Schritte: fmt, clippy_all, clippy_default, clippy_runner_default, clippy_runner_txv1, test_all, test_default, test_runner_default, test_runner_txv1, bash_n, release, python) |
| **Protokoll/Proto** | `protocol/schemas/darkbot/project0/v1/*.proto`, `protocol/schema_id.txt`, Goldens `protocol/golden/project0/*.hex`, Check/Generate `protocol/scripts/check.sh` |
| **Frontend P0** | `frontend/src/features/project0/` (Adapter `api/ownerAdapter.ts`, Modelle `model/owner.ts`, Seiten `pages/`, Sections `components/`), E2E `frontend/tests/project0-status.spec.ts` |
| **CI** | `.github/workflows/project0-status.yml` (Pfade protocol/frontend/gateway/project_0 außer docs), `gateway.yml` (gateway/protocol), `bsc.yml` (bsc/protocol/frontend core …) |

---

## 7. Git- und Arbeitsstand (03.11.2026, Sessionende)

### 7.1 Auf `main` (gepusht, CI grün)

| SHA | Inhalt | CI |
|---|---|---|
| `75ac47d1` + `7754fa56` | DEC-17a Operator-Auth (Secure Enclave, `--confirm`, Epoch-Nonce, Evidence-SHA-Bindung) + Review-Fixes | grün |
| `fc8844fc` | DEC-17e `p0ctl journal archive` (korruptes Journal → `data/archive` + Manifest, ARCHIVED) | grün |
| `f5676276` | DEC-17g Safety-HALT sperrt neue Versuche, nie Exits | grün |
| `bf705a50`, `973e0d1b`, `4c7e82d5` | DEC-17f Operator-Tools unter Owner-Permits, Kostenlimits `prewarm_max_lamports`/`probe_max_lamports` (Limits rev 23) | grün (macOS-Flake bsc rerun) |
| `5191cba5` | DEC-17c `JitoGrpcSubmitter` (vendored mev-protos 46ead86a, FREE_ONLY) | Project 0 status contract grün |
| `1ccd3cd4` | **DEC-17b B1** Signers mit Rollen (`signers` `PUBKEY:ROLE+ROLE`, TREASURY nur lesend, `P0_SIGNER_KEYPAIRS`, Limits rev 24), Schalter je Signer, FE-06-Pilot genau eine selbstzahlende Wallet | Project 0 status contract **grün** |
| `9f330f4a` | (BSC) Audit E3; schema_id damals `c6c4c767…` | alle drei grün |

`origin/main` = `9f330f4a`. Der Haupt-Checkout `DarkBot/` ist auf `9f330f4a` fast-forwarded.

### 7.2 Lokal, NICHT gepusht — im Worktree `DarkBot-p0-work`

- **`db588cf3` feat(p0): DEC-17b B2+B3** (direkt auf `9f330f4a`).
  - Betreff beginnt mit „DEC-17b B2+B3 — one wallet per signer …".
  - Nach einem Rebase am Betreff wiederfinden.
- **Übergabe-Commit `docs(p0)`** mit diesem Dokument, direkt darauf.
- **Gate:** Der volle Gate-Lauf auf `db588cf3` wurde durch das Sessionende **abgebrochen** und ist
  **NOT_RUN**. Deshalb ist nichts gepusht.
- **Lokal bereits bestanden** (Working Tree = Commit-Inhalt):
  - Lib-Tests Owner/One-shot/Inventory/Hold/Reservation: 272 grün, darin 9 neue DEC-17b-Tests;
  - DEC-17b-Filter 9/9;
  - Reporter 14/14 (Goldens neu aufgezeichnet);
  - `cargo clippy --all-targets --all-features -D warnings` sauber;
  - `cargo fmt` sauber;
  - Frontend: Vitest 157/157, `npm run build` (inkl. bsc-fields + type-check), `lint:boundaries`,
    `check:freeze`, `lint:css`, Playwright `test:project0` 20/20, `test:code-split` 3/3;
  - `protocol/scripts/check.sh --generate`: Contract PASS (`buf breaking` gegen main).
- **schema_id ändert sich** von `c6c4c767…` auf
  **`afcdf3a3ca20cdeaaab9e509c7eb7582d9211a77d39ed1fe91bac22f191d2229`**.
  - BSC_v1 ist informiert und hat zugesagt, bis zu diesem Push nichts unter `protocol/` zu pushen.
  - Nach dem Push zeichnet BSC_v1 `engine.stream.hex` auf dem gepushten Stand neu auf.
  - **Du musst BSC_v1 den gepushten SHA schicken.**
- **Erwartete CI nach dem Push:**
  - „Project 0 status contract": grün erwartet;
  - „gateway" (Pfad `protocol/**`): grün erwartet;
  - „bsc" (Pfade `protocol/**`, `frontend/src/core/**`): **rot**, bis BSC_v1 das Golden neu aufzeichnet.
    Das ehrlich so berichten.

### 7.3 Was `db588cf3` enthält (DEC-17b B2+B3)

**Neue Owner-Struktur** `state_owner/wallets.rs`:
- `OwnerWallet` je konfiguriertem Signer mit `liquidator`, `roles`, `payer`, `atas`, eigener `safety`,
  `reservations`, `fe04_held/approval/settle_due/unbooked/non_landing/admission/prewarm_after/admissions`,
  `ledger_view`, `records` (für Grant-Quoten), `hold_evidence`, `selected`.
- Im Owner: `wallets: Vec<OwnerWallet>`. Der Pilot ist `wallets[0]` (`pilot()`).
- `SelectionStats` zählt die Wallet-Wahlen.

**Wallet-Wahl je Kandidat** (`choose_wallet` / `wallet_eligible` in `detect`):
- Gewählt wird unter den zulässigen Wallets die mit dem meisten freien SOL (`wallet_free_sol`: Lamports −
  alle Holds inkl. Exit-Reserve − Policy-Reserve). Bei Gleichstand der niedrigste Key. Kein Round-Robin.
- Zulässig ist eine Wallet nur, wenn sie
  - LIQUIDATOR **und** eigener FEE_PAYER ist (`runs_candidates`),
  - keinen Safety-HALT hat,
  - ein lesbares Buch hat,
  - einen bekannten SOL-Bestand hat.
- Unter **LIVE** zusätzlich:
  - Schalter `WALLET:<pk>` ENABLED;
  - FE-06-Scope lässt die Wallet zu;
  - kein gehaltener Versuch (Single-Flight je Wallet);
  - eigenes FE-04-Approval VALID (Approvals sind wallet-gebunden).
- Ist keine Wallet zulässig, läuft der Pilot; dessen Ablehnung ist dann die des Kandidaten. Die Gründe
  aller Wallets landen in `last_none_eligible`.

**Fehlerisolation:**
- HALT, UNKNOWN oder ein unlesbares Journal sperren **nur diese Wallet**.
- Beim Start (`setup::owner_wallets`): Ein unlesbares Journal blockiert die Wallet für den ganzen Lauf
  (Buch unlesbar, Safety `LedgerInvariant`). Sind alle ausführenden Wallets unlesbar, schlägt der Start
  mit `RESERVATION_JOURNAL_UNREADABLE` fehl.
- Globale Halts (FeedGap/StateGap/ProgramUpgrade) bleiben global.

**FE-04 je Wallet:**
- Settlement, Exit-Admission, Prewarm und Admission laufen mit Wallet-Index.
- `Fe04SettlementWork/Result` tragen `wallet`.
- Lifecycle-Refresh je Wallet (`fe04_refresh_wallet`: Buch, `Ledger::load`, B-Evidence, Records,
  LedgerView, Approval).
- Die B-Evidence wird über alle Wallets summiert (`BEvidence::merge`). Ist eine Wallet unlesbar, gibt es
  gar keine Evidence (fail closed).

**Grant-Quoten über alle Wallets:**
- `Fe04Admission.others` und `ExitCandidate.others` tragen die Records der anderen Wallets.
- `within_limits` und `within_exit_limit` zählen beide.
- `fe06_grant_totals`, `sim_only_use_facts` und die FE-06-Statuszähler summieren über alle Wallets.
- Records einer Wallet, die nie gelesen wurden, ergeben `FE06_QUOTA_UNKNOWN` (fail closed).

**Abonnements** (`setup::wallet_accounts` + `events.rs`):
- Das eigene Konto jedes Signers (SOL).
- Die ATAs jeder ausführenden Wallet für jede Bank-Mint.
- **Bestätigter Bug, damit gefixt:** Das SOL-Konto des Liquidators wurde nie abonniert. Dadurch sah die
  LIVE-Admission den SOL-Bestand immer als unbekannt.

**Router:** `JupiterV2RouteProvider::for_taker(wallet)` — Swaps und Exit-Verkauf laufen für die gewählte
Wallet.

**Rebalancing OBSERVE:**
- Zuerst `inventory::plan_transfers`: Wallet→Wallet-Moves, die eine Wallet unter `min` aus Wallets über
  `max` auf `target` auffüllen, nie unter das eigene `target`, Dust beachtet.
- Danach der Swap-Plan je Wallet in einem gemeinsamen Budget.
- Neuer Record `RecordEvent::WalletRebalancePlan` (angehängte Variante; `RebalancePlan` bleibt lesbar).
- Negativtest: `rebalance.rs` und `inventory.rs` enthalten keinen TX-, Sign- oder Send-Code.

**Status/Proto/Frontend:**
- `status_tap::OwnerWalletRow` und `OwnerWallets.rows/selections/none_eligible/last_none_eligible`.
- Finance und FE-06-Zähler summieren über die Wallets.
- Die bisherigen Abschnitte (Safety, Wallets, Approval, Admission) zeigen den Pilot.
- `owner.proto`:
  - `WalletRow`, `WalletsState` Felder 17–20;
  - `RebalanceTransfer`, `RebalancePlan.transfers=8`;
  - `RebalanceTrade.wallet=6`, `RebalanceBlocked.wallet=3`.
- Reporter-Mapping; Goldens `liquidator_state*.hex` und `wallets_state*.hex` neu.
- Frontend:
  - `WalletRowsSection.tsx` „Wallets · per signer" (read-only) im Wallets-Reiter;
  - Transfers/Wallet in `OwnerSection` (Rebalancing);
  - Adapter und Modell erweitert, Vitest- und Playwright-Tests ergänzt.
- STATUS-Zeile „DEC-17b wallet model" ist ergänzt mit „B2+B3 LOCAL_PASS (gate pending) · B4 split roles
  OPEN".

**Neue Tests:**
- `owner_e2e_tests.rs`:
  - `dec17b_the_wallet_choice_is_deterministic_and_a_failing_wallet_blocks_only_itself`
  - `dec17b_live_runs_only_a_wallet_that_may_send_and_single_flight_is_per_wallet` (echter
    detect-Pfad, ein Send)
  - `dec17b_grant_quotas_need_every_wallets_records`
  - `dec17b_capital_moves_between_wallets_only_as_an_observed_plan`
  - `dec17b_every_wallets_account_and_atas_are_subscribed`
- `setup.rs`: `dec17b_an_unreadable_journal_blocks_only_its_wallet`
- `inventory.rs`: `dec17b_transfers_refill_a_wallet_from_the_others_surplus`
- `one_shot.rs`: der Quoten-Test ist um eine andere Wallet erweitert.

---

## 8. DEC-17 — Stand je Unterpunkt

| DEC-17 | Inhalt (PHASENPLAN ≈5823–5829) | Stand |
|---|---|---|
| a | Operator-Auth Secure Enclave userPresence, Grant/ENABLE nur signiert, Evidence-SHA | **OFFLINE_VERIFIED** (`75ac47d1`, `7754fa56`); reale Touch-ID-Registrierung = Operator-Aktion, NOT_RUN |
| b | Wallet-Modell | B1 **OFFLINE_VERIFIED** `1ccd3cd4`; B2+B3 lokal `db588cf3`, **Gate NOT_RUN**; **B4 (getrennte Rollen) offen — Nutzerfrage, §9 Schritt 0** |
| c | Jito S3-R01 | **OFFLINE_VERIFIED** `5191cba5` (Produktion erst nach S1-RACE PASS; reale Jito-Policy NOT_RUN) |
| d | EXIT-WSOL Dust | **OFFEN — nächstes Paket** (Design §10) |
| e | korruptes Journal archivieren | **OFFLINE_VERIFIED** `fc8844fc` |
| f | Operator-Tools unter Permits | **OFFLINE_VERIFIED** (`bf705a50`…`4c7e82d5`); reale Tool-Läufe NOT_RUN |
| g | Safety-HALT sperrt nie Exits | **OFFLINE_VERIFIED** `f5676276` |
| P8-CTRL.10/.11 | L01–L16-Offline-Matrix, „vollständig integrierter Geldpfad" | **OFFEN** nach DEC-17d (§11) |

**Kenngrößen:**
- `LIMITS_REVISION = 24` = `SIGNERS_REVISION`. Pin in `payload.rs`/`runtime_config` (24, 169 Parameter).
- Journal-Kinds: `OPERATOR_TOOL_USE = 1119` (nach `JOURNAL_ARCHIVE = 1118`).
- Rollen: `runtime_config::roles::{LIQUIDATOR, FEE_PAYER, TREASURY, SIGNING=[LIQUIDATOR,FEE_PAYER]}`.
- Signer-Eintrag: `PUBKEY:ROLE+ROLE`.
- Env: `P0_SIGNERS`, `P0_SIGNER_KEYPAIRS` (`pk=path,…`), Fallback `P0_KEYPAIR_PATH`.
- econ4h lädt keinen Private Key (`P0_EXPECTED_PUBKEY`).

---

## 9. Nächste Schritte (genau in dieser Reihenfolge)

**Schritt 0 — offene Nutzerfrage (unbeantwortet; die Frage wurde beim Sessionwechsel abgebrochen). Gleich
zu Beginn stellen:**

> Getrennte Rollen (Wallet A liquidiert, Wallet B zahlt die Gebühren: zwei Signaturen pro TX, SOL-Holds
> von B für Versuche in A's Journal-Lane, Exit von A mit Gebühren von B) sind **nicht** gebaut. Derzeit
> führt nur eine Wallet aus, die LIQUIDATOR **und** ihr eigener FEE_PAYER ist.
> Gehört „B4 getrennte Rollen" zu REV.1.0 vor DEC-17d? Optionen:
> (a) jetzt bauen (≈1.500 Zeilen: Journal-Intent `fee_payer` mit Schema-Version, Buch-Attribution über
> Wallets, Pipeline mit zwei Signern/SignPermit beider Keys, Builder/Tip/Rent über Fee-Payer, Exit
> Verkäufer≠Zahler, Status);
> (b) nur selbstzahlend: Konfig und LIVE-Gate verlangen das ausdrücklich, reine FEE_PAYER- oder
> LIQUIDATOR-Wallets werden abgelehnt;
> (c) wie (b) jetzt sperren und getrennte Rollen als eigenes späteres Paket festhalten.

Hintergrund:
- PHASENPLAN ≈3942 nennt „Liquidator, Fee-Payer oder weiterer erforderlicher Signer".
- Der FE-06-Scope hat ein eigenes Feld `fee_payer`.
- DEC-17b sagt „Erster Pilot: genau eine Wallet, zugleich Fee-Payer".
- B1 (`capability_control::wallets_usable`) lässt LIVE heute schon mit getrennter LIQUIDATOR- und
  FEE_PAYER-Wallet zu. Ausführen kann eine solche Kombination aber nichts (der Pilot läuft).
- Diese Inkonsistenz wird entweder mit B4 oder mit Option (b)/(c) aufgelöst.
- Frage kurz stellen (AskUserQuestion); die Optionen nicht neu herleiten.

1. **Gate auf B2+B3:**
   - Im Worktree prüfen, dass HEAD = Übergabe-Commit ist, darunter `db588cf3`.
   - `git status` muss sauber sein (außer `?? bsc/…`).
   - `pgrep -fl gate.sh` muss leer sein.
   - Dann das Gate laufen lassen (§15.1, Hintergrund).
   - Ein Python-Fehler mit `TimeoutExpired` (10 s/30 s) ist der bekannte XProtect-Erststart-Flake neuer
     Release-Binaries. In dem Fall nur `P6_GATE_STEPS="python"` erneut laufen lassen (§16).
2. **Grün ⇒ Push:**
   - `git fetch`. Ist origin/main vorgerückt, `rebase` (siehe §5).
   - Dateiliste prüfen: nur `project_0/`, `protocol/`, `frontend/src`, `frontend/tests`,
     `docs/protocol/PROTOCOL.md`.
   - `git push origin HEAD:main`.
3. **CI beobachten** (§15.2), erwartet drei Runs (P0, gateway, bsc). Rot bei bsc wegen des Goldens ist
   erwartet; alles andere ist ein Befund.
4. **BSC_v1 benachrichtigen:** gepushter SHA + schema_id `afcdf3a3…`; bitte `engine.stream.hex` neu
   aufzeichnen. Danach `DarkBot/` per ff aktualisieren.
5. **Doku-Commit (`docs(p0)`):**
   - STATUS-Zeile „DEC-17b wallet model": B2+B3 **OFFLINE_VERIFIED** mit SHA + CI-Run-ID;
   - B4 je nach Nutzerantwort;
   - push.
6. **Je nach Antwort aus Schritt 0:** B4 bauen, oder Option (b)/(c) umsetzen (klein: Konfig-Validierung
   in `runtime_config` + `wallets_usable` verlangt eine selbstzahlende Wallet + Tests).
7. **DEC-17d EXIT-WSOL** (§10), ein Paket: Gate, Push, CI.
8. **P8-CTRL.10/.11** Offline-Matrix L01–L16 (§11).
9. **Danach:**
   - die Freigaben/Tokens für **C-11-Neumessung → C-04 → C-05 → C-10** beim Nutzer anfragen (§13);
   - währenddessen die übrigen Offline-Pakete (§12);
   - am Ende die technische Gesamtabnahme REV.1.0.

---

## 10. DEC-17d EXIT-WSOL — Design (PHASENPLAN ≈5826, Nutzerauftrag 03.11.)

**Text (verbindlich):**
- WSOL-Staub sperrt nur die eigene Position, nie Exits anderer Positionen.
- Beim vollständigen Exit wird das WSOL-ATA per Unwrap geschlossen.
- Ein Rest **unter** einer typisierten Schwelle wird als `DUST`-Record gebucht und als Kosten verbucht.
- Ein Rest **darüber** ist ein eigener Exit mit eigener Admission.
- Nachweis „Close + Re-Create in einer TX" offline per **litesvm** mit dem im Lese-Smoke gelesenen
  Token-/ATA-Programm-Bytecode.
- M1/M2 werden diesem Text zugeordnet; weichen sie ab, gilt dieser Text.

**Zusatz des Nutzers:** Unbekannte WSOL-Bestände dürfen nie ignoriert werden. Die Abrechnung bleibt
nachweislich positions- und walletgenau (je Wallet aus B2/B3).

**Heutiger Code:**
- `execution/one_shot/exit_admission.rs` ≈171 lehnt einen Verkaufs-Exit ab, wenn das WSOL-ATA nicht leer
  oder unbekannt ist (`EXIT_WSOL_ACCOUNT_NOT_EMPTY_OR_UNKNOWN`). Damit sperrt Staub heute **alle**
  Exits; genau das muss weg.
- Der Unwrap-Exit existiert: `route::unwrap_exit_route`, `wsol_account` (create idempotent + close),
  `fe04.rs` `unwrap_payout_lamports` = ATA-Lamports − Rent.

**Plan:**
1. Typisierte Schwelle `wsol_dust_max_lamports` in `runtime_config::Limits`.
   - Validierung, Default sinnvoll und hergeleitet.
   - `LIMITS_REVISION` → 25 und Pin-Test nachziehen.
   - `system_status`-Golden neu: `UPDATE_GOLDEN=1 cargo test -p darkbot-p0-reporter --lib -- golden_frames_match`.
2. Positionsgenaue Zuordnung des WSOL-Bestands je Wallet. Die offenen Positionen des Journals
   (WSOL-Position der Attempt-Approval) bestimmen, wem welcher WSOL-Teil gehört.
   - Unbekannter oder nicht zuordenbarer Rest ⇒ **UNKNOWN**, er blockiert **nur** die betroffene
     Position bzw. Wallet. Niemals ignorieren, niemals 0.
3. Ein Exit einer anderen Position wird durch fremden WSOL-Staub **nicht** gesperrt.
   - Sein Swap muss den Staub unberührt lassen (Close nur beim vollständigen Exit der WSOL-Position).
   - Die Admission rechnet die Staub-Lamports getrennt.
4. Vollständiger Exit der WSOL-Position:
   - Rest ≤ Schwelle ⇒ `DUST`-Record (neuer typisierter Journal-Record/-Kind im FE-04-Journal,
     angehängt, alte Layouts lesbar), als Kosten in Settlement/Ledger (FE-07-Abrechnung) gebucht.
   - Rest > Schwelle ⇒ eigener Exit (eigene Approval/Admission, eigene Buchung).
5. **litesvm-Nachweis:**
   - Neue Dev-Dependency `litesvm` (nicht in `Cargo.lock`; crates.io-Download ist normale Dev-Netzwerk-
     nutzung, kein Mainnet).
   - ELFs liegen **nur** im Worktree unter
     `project_0/data/c01/edf159af0e91fb03/programs/{token.so,token-2022.so,associated-token.so}`
     (untracked, aus C-01).
   - Test: in einer TX `CloseAccount` des WSOL-ATA + `create_idempotent` neu, Lamports/Rent exakt.
   - Test-Fixture-Pfad über die bestehende Data-Konvention, nichts hart kodiert.
6. Status/Frontend: DUST-/UNKNOWN-WSOL je Wallet in Wallets-Rows bzw. NonLanding/Holds sichtbar
   (Proto-Feld, Reporter, Adapter, Test).
7. Tests:
   - Staub blockiert nur die eigene Position;
   - unbekannter WSOL blockiert die Wallet bzw. Position;
   - DUST wird gebucht;
   - Rest > Schwelle wird eigener Exit;
   - alte Journale bleiben lesbar;
   - litesvm-Test.

---

## 11. P8-CTRL.10/.11 — Offline-Abnahme (PHASENPLAN ≈4057–4084)

Matrix L01–L16. Sie läuft über die echten Produktions-APIs mit synthetischen Schlüsseln und einem lokalen
Recording-Receiver, der nie an Mainnet sendet:

- **L01:** Owner-Neustart/Stop/Crash ⇒ alle Schalter DISABLED.
- **L02:** jedes fehlende Gate einzeln ⇒ genauer Blockgrund.
- **L03:** falsche Wallet/Rolle/Identität ⇒ Refused.
- **L04:** Scope-Hash/Ablauf/Revoke/Generation/Evidence.
- **L05:** parallele CLI-/GUI-CAS, gleiche Request-ID.
- **L06:** Crash an jeder Grenze ⇒ kein doppelter Hold.
- **L07:** DISABLE vor/nach Sign bzw. Egress.
- **L08:** Single-Flight und doppeltes Settlement.
- **L09:** UTC-Tageswechsel, Limits.
- **L10:** fehlende Reserven, stale Blockhash.
- **L11:** EXIT_ONLY.
- **L12:** RPC/Jito/Prereq/Tip/JIT/Prepared/Race-Pfad ⇒ dieselben Gates.
- **L13:** Terminal↔Gateway↔Owner↔Frontend gleich.
- **L14:** genau ein Recording-Egress, danach HALT.
- **L15:** Shadow/SIM_ONLY-Signaturen.
- **L16:** Neustart bei UNKNOWN/offener Position.

Dazu Compile-Fail-, Mutation-, Concurrency-, Crash- und Journal-I/O-Tests, Release-Gates und CI.
**OFFLINE_VERIFIED** erst nach Review ohne offene HIGH/CRITICAL-Befunde, mit `gate.sh` und grüner CI.
Vorhandene Tests zuordnen und nur Lücken schließen (keine Testorgie).

---

## 12. Übrige offene REV.1.0-Arbeit (Inventur 03.11., Quellen PP=PHASENPLAN, BU=BUGS, ST=STATUS)

**A. Netzwerk-Kette** (erteilt, braucht Run-Tokens):
- C-11-Neumessung (Digest-Bindung `44fa3cd2` ⇒ neue Messung nötig) → C-04 Network-Audit → C-05 600 s →
  C-10 30 min C0-MO.
- C-10 braucht das Deploy von P0-027 durch den Nutzer.
- Veraltete STATUS-Stellen: Z. ≈54/186/235/322/344.

**B. DEC-17:** siehe §8 — offen sind b-B4 (Frage), d und P8-CTRL.10/.11.

**C. Offline:**
- S3-R00/R02/R03 (PP ≈4507);
- Tagger T-01..03 + H0-04 (ST ≈307/360), jetzt entsperrt;
- S1-04 (ST ≈317), S1-05 (ST ≈318), S2-05 (ST ≈325), S4-04 (ST ≈336);
- P7-01..06 (PP ≈3804);
- P4B-01..05 (PP ≈3881);
- P8-RES globale Reservierung (ST ≈379);
- FEUI-01 Rest (ST ≈397);
- Frontend-Reste F6/F4/SOLL_MISSING (F0-VAR);
- E2-Rest (BU ≈1264);
- BUGS-Reste L3/R19/R20/Q3/Q5/P0-001/002/004 (BU ≈1287–1493);
- DEC-LOW-Reste DEC-10/03/04/11 (BU ≈1540–1548);
- P0-016 (BU ≈1635);
- Saved-Data-Replay market5h (ST ≈200);
- PERSIST-03 (ST ≈139).

**E. Blockiert auf nicht erteilte Läufe:**
- S1-RACE (C-03);
- P6-MT MT6–10 (C-06);
- C0-E1 4h (C-06);
- H0 real (C-02);
- Paritätskorpora;
- DEC-16 real.

**F. Wertbehaftet** (nicht erlaubt): FE-02 signed edge, FE-05/06/07, LIVE.

**Lücke:** Es gibt keine einzelne Checkliste „technische Gesamtabnahme REV.1.0" (PP ≈4846 vs. PP Z. 11).
Das braucht eine Nutzerentscheidung (§14).

---

## 13. Netzwerk-Kette und Freigaben (ENTSCHEIDUNGEN §4, DEC-18 §6)

- **C-01:** Schritte 5–6 erledigt; `edf159af0e91fb03` steht auf `P1A_PARITY PASS` (BUILD; FE01
  `1accaed7f43fb811`); Schritt 7 promoviert. BOOT-05 ohne Ausnahme.
- **C-11:**
  - Werkzeug `p0ctl provider measure` (P0-022).
  - Evidence `0e909773088733b1` / `f9da768249108a52` (02.11.): 12 RPC + 6 WSS SUPPORTED, 1 RPS, kein 429.
  - Durch die Digest-Bindung `44fa3cd2` brauchen C-04/C-05 eine **neue** C-11-Messung.
  - Limits: ≤120 s, 1 RPS, ≤120 Versuche, 16 MiB, 1 WSS, ≤2 gleichzeitig, 5 Subscription-Typen
    (`project0-decision-20261102-c11-limits`).
  - C-04/C-05/C-10 laufen mit `P0_RPC_RPS=1` und `P0_BOOTSTRAP_RPS=1`.
- **C-04** (Network-Audit), **C-05** (≤600 s Kalibrierung), **C-10** (30 min C0-MO, read-only, inkl.
  Token-/ATA-Bytecode-Reads):
  - am 01.11. ERTEILT;
  - jeder Lauf braucht vor dem Start die konkrete Token-/Run-Bestätigung des Nutzers;
  - Provider: das ausgewählte, nachweislich kostenlose Helius-Free-Paar bzw. Public-Solana.
- **C-12 NICHT ERTEILT.** C-02/C-03/C-06/C-07/C-08 nicht erteilt.
- **Nie** einen Mainnet-Lauf auf Code ohne Gate+CI. Bei einem Fehler nicht „nochmal versuchen" ohne
  Ursache und Fix.

---

## 14. Offene echte Nutzerentscheidungen (nicht selbst entscheiden)

1. **DEC-17b B4** getrennte Rollen (§9 Schritt 0).
2. **P0-029:** unbeaufsichtigter Keychain-Zugriff (ACL/cdhash; `docs/evidence/BKC_KEYCHAIN_20261102.md`).
3. **B-8:** N für HOLD_B ACTIVE.
4. **DEC-02:** Sim-Context-Slot.
5. **V7:** Voll-Hold.
6. **A23-05 / P0-030:** flaky Linux-CI-Latenztest.
7. **Drift** P3-04/P5-02.
8. **Grants** C-02/C-03/C-06/C-07/C-08, MT11/12, FE-03, K.
9. **P5-08** Staging.
10. **SIM_ONLY-Signaturen:** Es gibt keinen qualifizierten nicht-sendenden Simulator.
11. **Fehlende Checkliste** „technische Gesamtabnahme REV.1.0".
12. **Reale Touch-ID-Registrierung** des Operator-Schlüssels (`p0ctl operator register`) = Operator-Aktion.

---

## 15. Werkzeuge (Quelltext, weil der alte Scratchpad sitzungsgebunden ist)

Lege sie im **eigenen** Scratchpad der neuen Session an. Platzhalter `$S` = Scratchpad-Pfad der neuen
Session.

### 15.1 Gate-Wrapper `$S/gate-<sha>/run.sh`

Laufzeit ≈45–60 min, im Hintergrund starten (`run_in_background`).

```bash
#!/usr/bin/env bash
cd /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-p0-work
G=__GATEDIR__   # z. B. $S/gate-<sha>
echo "$(git rev-parse HEAD)"
P6_GATE_STEPS="fmt clippy_all clippy_default clippy_runner_default clippy_runner_txv1 test_all test_default test_runner_default test_runner_txv1 bash_n" bash project_0/scripts/gate.sh $G/a > $G/a.out 2>&1; echo gate_a=$?
P6_GATE_STEPS="release python" bash project_0/scripts/gate.sh $G/b > $G/b.out 2>&1; echo gate_b=$?
(cd frontend && npm run -s check:bsc-fields) > $G/c.out 2>&1; echo bsc_fields=$?
grep -h "exit=" $G/b.out $G/a.out
tail -1 $G/c.out
```

- Grün heißt: `gate_a=0`, `gate_b=0`, `bsc_fields=0` und jede Zeile `exit=0`.
- Logs der Einzelschritte liegen in `$G/a/*.log` bzw. `$G/b/*.log` (z. B. `b/python.log`).
- Einzelschritte neu laufen lassen:
  `P6_GATE_STEPS="python" bash project_0/scripts/gate.sh $G/py2`.
- Letzte Referenzzahlen (B1): test_all 1394/0/12, test_default 1230/0/12, runner 535/0/3 (je Feature),
  F0-VAR PASS.

### 15.2 GitHub-API `$S/ci/gh_api.sh` (Token aus dem git-Credential, wird nie ausgegeben)

```bash
#!/usr/bin/env bash
# GitHub REST call for sawyerma/darkbot with the stored git credential; the token is never printed.
# usage: gh_api.sh METHOD PATH [JSON_BODY] [-o FILE]
set -euo pipefail
method=$1; path=$2; body=${3:-}; out=${4:-}
token=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill 2>/dev/null | sed -n 's/^password=//p')
args=(-sS -L -X "$method" -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28")
[[ -n "$token" ]] && args+=(-H "Authorization: Bearer $token")
[[ -n "$body" ]] && args+=(-d "$body")
[[ -n "$out" ]] && args+=(-o "$out")
curl "${args[@]}" "https://api.github.com/repos/sawyerma/darkbot$path"
```

- PATH relativ zum Repo, z. B. `"/actions/runs?head_sha=<voller sha>"`.
- Fehlgeschlagene Jobs neu starten: `POST "/actions/runs/<id>/rerun-failed-jobs"`. Nur bei belegtem
  Flake (z. B. macOS-Runner).

### 15.3 CI-Watch `$S/ci/watch.sh <vollerSHA> <erwartete Runs>` (Hintergrund, ≤60 min)

```bash
#!/usr/bin/env bash
set -uo pipefail
S=$(cd "$(dirname "$0")" && pwd)
sha=$1; want=${2:-1}
for _ in $(seq 1 120); do
  "$S/gh_api.sh" GET "/actions/runs?head_sha=$sha&per_page=20" "" "$S/runs_$sha.json" || true
  done_n=$(python3 -I -c 'import json,sys; r=json.load(open(sys.argv[1])).get("workflow_runs",[]); print(sum(1 for x in r if x["status"]=="completed"), len(r))' "$S/runs_$sha.json" 2>/dev/null)
  set -- $done_n
  if [ "${1:-0}" -ge "$want" ] && [ "${1:-0}" -eq "${2:-0}" ]; then break; fi
  sleep 30
done
python3 -I - "$S/runs_$sha.json" <<'PY'
import json,sys
for r in json.load(open(sys.argv[1])).get("workflow_runs",[]):
    print(r["head_sha"][:8], r["name"], r["status"], r["conclusion"], r["id"])
PY
```

### 15.4 Weitere Befehle

**Reporter-Goldens neu aufzeichnen, wenn sich die Status-Projektion ändert:**
```bash
cd project_0/native
UPDATE_GOLDEN=1 cargo test -p darkbot-p0-reporter --lib -- golden_frames_match
# danach den ganzen Reporter normal testen
cargo test -p darkbot-p0-reporter --lib
```

**Protobuf geändert:**
```bash
bash protocol/scripts/check.sh --generate
```
Der Befehl erzeugt:
- `frontend/src/core/generated/...`;
- `docs/protocol/PROTOCOL.md`;
- `protocol/schema_id.txt` (meldet `protocol contract: PASS`).

`buf` braucht Netz zu buf.build für das ES-Plugin.

**Frontend (wie in der CI):**
```bash
cd frontend
npm test && npm run build && npm run lint:boundaries && npm run check:freeze && npm run lint:css
npm run test:project0 && npm run test:code-split
```

**Gezielte Rust-Tests:**
```bash
cargo test -p darkbot-runner --features crate_health --lib -- <filter>
```

**Hinweise zu den Crates:**
- Paket `darkbot-runner` = Crate `crates/runtime`.
- `state_owner` existiert nur mit Feature `crate_health`.
- Ohne Feature baut `--lib` nicht alles.

---

## 16. Bekannte Fallstricke

- **XProtect-Erststart:**
  - Neue Release-Binaries starten auf macOS beim ersten Mal 7–17 s verzögert.
  - Folge: Python-Gate-Tests mit 10 s/30 s Timeout scheitern (`test_market_report …`,
    `test_econ_launcher_fails_closed_before_build`).
  - Kein Code-Fehler: nur `python` erneut laufen lassen; der Lauf ist dann grün.
- **zsh:**
  - Nie über eine leerzeichengetrennte `$VAR` loopen.
  - Refspec immer quoten: `"${C}:refs/heads/main"`, sonst greift der `:r`-Modifier.
- **Mutation-Restore:** mtime erneuern (`os.utime`), sonst nutzt cargo den Mutanten weiter.
- **Exit-Codes:** Nie `cmd | grep && git commit`; Exit-Codes direkt prüfen.
- **Doku-SHAs** erst nach Push und Rebase eintragen.
- **schema_id:** Jede Proto-Änderung im P0-Schema ändert die gemeinsame `schema_id`. BSC_v1 muss das
  Golden neu aufzeichnen; vorher warnen.
- **Design-Freeze:** `WalletsPanel.tsx` nicht ändern (`check:freeze`); eigene Sections daneben.
- **Python-Gate** braucht Release-Binaries im Worktree-Default `native/target/release` (keine
  CARGO_TARGET_DIR-Umleitung für `release python`).
- **bincode-Enums** (`RecordEvent` u. a.): nur Varianten am Ende anhängen.
- **Testfixture-Owner** (`owner_e2e_tests.rs` `fixture_with`):
  - `wallets[0]` hat `liquidator` ≠ `payer.pubkey()` (Testkonvention).
  - Konten im State werden erst nach dem Recovery-Lauf angenommen (`b01_observing` treibt bis Ready).
- **Test-Reihenfolge:** `owner_e2e_tests` laufen mit `--test-threads=2` im Gate. Tests mit globalem
  Zustand (MT0) beachten.
- **Flaky Linux-CI:** `a23_01_owner_wakeup_latency_is_measured` (P0-030). BSC-macOS-Jobs flaken
  gelegentlich; dann rerun-failed-jobs.

---

## 17. Kommunikation mit dem Nutzer

- Deutsch, kurz, keine Minutenprotokolle.
- **Paketbericht** = fertige Funktionen · bestätigte Bugs mit Fix · bestandene Prüfungen · SHA + CI ·
  verbleibende Pakete als Countdown · echte Betreiberentscheidungen.
- Unverifiziertes = NOT_RUN / PARTIAL / BLOCKED. Nie „fertig" ohne Push + CI.
- Keine Fragen zu Entschiedenem. Keine Freigabeanfragen für Offline-Arbeit.
- Echte Entscheidungen kurz mit AskUserQuestion stellen.
- Der Nutzer ist schnell verärgert bei Ineffizienz:
  - unnötige Tests oder Gates;
  - Mainnet-Wiederholungen;
  - lange Doku;
  - Nachfragen, die der PHASENPLAN beantwortet.
- Vorher immer PHASENPLAN, STATUS und Manifeste prüfen.
- Bei langer Arbeit zwischendurch eine Zeile Stand geben.

---

## 18. BSC-Koordination (Kurz)

- BSC_v1 pusht unabhängig nach `main`. Vor jedem eigenen Push `fetch` und, falls nötig, `rebase`.
- Gemeinsame Dateien (`protocol/`, `gateway/`, `frontend/src/core/`, `frontend/contracts/bsc-fields.json`)
  nur abgestimmt ändern.
- Generierte Dateien neu erzeugen, nie von Hand mergen.
- Aktuelle Absprache: BSC_v1 pusht nichts unter `protocol/`, bis unser DEC-17b-Push (schema_id
  `afcdf3a3…`) auf main ist. Danach zeichnet BSC_v1 `engine.stream.hex` neu auf. **SHA nach dem Push
  senden.**
- BSC_v1 hat mitgeteilt: E3 gepusht (`9f330f4a`); als Nächstes E2 (ohne `protocol/`).
