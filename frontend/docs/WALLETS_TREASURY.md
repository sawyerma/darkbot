# Wallets & Treasury – Project_0 / DarkBot Frontend (FEUI-10)

**Verbindlicher SOLL-Vertrag, 2026-10-17.** Modul wurde fachlich geplant, ist **nicht** als Wallet-/Treasury-Auszahlungsengine implementiert. Ergänzt [FRONTEND_PHASENPLAN](FRONTEND_PHASENPLAN.md), [PROJECT_0_CODE_MAP](PROJECT_0_CODE_MAP.md), [CONTROL_PROTOCOL](CONTROL_PROTOCOL.md) und [STARTUP_BACKTEST_OPTIMIZATION](STARTUP_BACKTEST_OPTIMIZATION.md). Fachliche LIVE-/Wallet-/P8-/FE-04/FE-06/FE-07-/JFI-Verträge aus [Project_0 PHASENPLAN](../../project_0/docs/PHASENPLAN.md) behalten Vorrang. **Dieses Dokument ist keine Autorisierung, Geld zu senden.**

## 1. Produktziele / Wallet-Ansicht

Unter `Project 0 Control → Wallets & Treasury` sollen alle **konfigurierten öffentlichen Walletidentitäten** des entsprechenden Projekts abrufbar sein. Auf der gemeinsamen DarkBot-Übersicht können zusätzlich BSC-/Solana-Wallets über eigene Domainadapter angezeigt werden; **keine Cross-Project-Vermischung** von Guthaben, Signierern, Policy, Nonce, Reservation oder Berechtigung.

**Wallet-Karten / Felder:**
- `project_id`, `chain_id` / GenesisHash, `wallet_role` (z. B. project_0_trading, fee_payer, read_only_destination, treasury), `wallet_public_key`, Label, Quelle/Config-Provenienz, Signer-Verfügbarkeit **ohne Private Key**, letzte on-chain-verifizierte Slot-/Zeitangabe und Quote-/FX-Zeit.
- Pro SPL-Token: Mint-Pubkey, Token-Programm (SPL/Token-2022), Decimals, Token-Account/ATA, exakte Atomics und formatierte Menge, Quote und USD-Schätzwert getrennt. SOL: Lamports und Fee-/Rent-/Reserve-Anteile.
- Separate Bilanzfelder: `onchain_balance`, `pending_inbound`, `reserved_for_inflight`, `reserved_for_liquidation`, `reserved_for_fee_rent`, `minimum_working_capital`, `unknown_or_unreconciled`, `available_spendable`, `realized_net_profit`, `profit_available_for_sweep`, `swept_lifetime`. **Saldo ist kein Gewinn.**
- Historie: Token-In/Out, Swap, Liquidation, Fee, Tip, Rent, abgeglichene Trade-PnL, manual/auto sweep, TX-Signatur, Zeitpunkt, Finality, Gebühren, Restinventar und Reconciliation-Provenienz. Portfolio-/Token-Summen dürfen nur mit belegter Gleichzeitigkeit, Bewertungsquelle und bekannter Preisbasis zusammengeführt werden.
- Öffentliches Walletkonfigurationsfile `project_0/config/wallet.public.env` enthält `P0_EXPECTED_PUBKEY` und `P0_WALLET_STATUS=PROVISIONED` (**IST**). Die öffentliche Identität ist keine Bestätigung von Liquidität, Profit, on-chain Saldo oder Signerzugriff. Keine privaten Schlüssel, Seeds, Browserlocalstorage-Keybags oder Wallet-Secret-Dateipfade an React/Vite/WS liefern.

## 2. Zielwallets / Netzwerke

- **Phantom Solana:** SOL und für Solana gültige SPL-Token an den bestätigten **Solana-Empfangspubkey**, nicht an eine EVM-Adresse. Native Mints/Token-Accounts/ATA und gegebenenfalls Empfänger-ATA-Kosten prüfen.
- **MetaMask Solana:** Metamask unterstützt Solana-Konten; die konkrete **Solana-Adresse** auswählen und kryptografisch/formal validieren. **Nicht** die MetaMask-`0x...`-EVM-Adresse für einen nativen Solana-Transfer übernehmen.
- **MetaMask EVM (z. B. BSC/Ethereum):** ist **nicht** direktes Ziel einer Solana-SPL-Transaktion. Nur als eigenständige Chain-/Bridge-/Swap-Anforderung in einem späteren gesondert genehmigten Bridge-Adapter, mit Slippage, Zielchain, Fees, Mint-/Token-Mapping, Finality und zusätzlichem Bridge-Risiko. Nicht als automatischer Gewinnsweep mit EVM-Ziel stillschweigend implementieren.
- **BSC-Bot / fremde Domain:** Quelle und Auszahlungsadapter separat. Project_0 darf BSC-Walletsaldo oder BSC-Transaktionen nicht aus Solana-Ownerzuständen ableiten.
- Empfänger müssen als `TreasuryDestination` (SOLL: `id,label,chain_id,address,asset_allowlist,verification,created_at,changed_at,enabled`) registriert und bewusst bestätigt werden. Neue/geänderte Zieladresse: getrennte Freigabe, Out-of-Band-Anzeige der vollständigen Adresse und optional kleiner belegter Testtransfer **nur nach zusätzlicher expliziter Autorisierung**. Adress-/Owner-/Mint-/Chain-Mismatch = **BLOCKED**. Keine Destination-Suche per Browsertext ohne Whitelist.

**Öffentliche technische Quellen:** [MetaMask Solana](https://support.metamask.io/configure/networks/navigating-solana), [Phantom Receive](https://help.phantom.com/articles/receive-tokens-in-phantom-4406393831187), [Solana TransferChecked](https://solana.com/docs/tokens/basics/transfer-tokens), [Solana ATA](https://solana.com/docs/tokens/basics/create-token-account). Empfangsadressen/Netzwerke, SPL-Mint/Decimals und ATA pro Transfer prüfen.

## 3. Berechnung: tatsächlich auszahlbarer Gewinn

Der Benutzer will `ab einem frei wählbaren Profit X` eine **manuelle oder automatische** Sicherungsüberweisung. Das Programm darf niemals einfach `wallet_total - X` oder eine positive Shadow-Simulation versenden.

**SOLL: `TreasuryAvailability` aus einheitlichem finanziellem Rust-Ledger, exakt pro Wallet/Chain/Asset und Zeitpunkt.**

1. `realized_net_profit_finalized`: nur durch eigene finalisierte Transaktionen, tatsächlich abgeglichene Token-/Lamport-Deltas, Fees/Tip/Rent, realisierte Exits und Restinventarbewertung begründete Net-PnL. Nicht realisierte Mark-to-market-Gewinne, `market_gross_edge`, `shadow_executable_net_edge` und UNKNOWN nicht mitzählen.
2. `eligible_unswept_realized_profit`: eindeutig abgeschlossener Gewinn seit dem letzten belegten Sweep **abzüglich** bereits ausgezahlter Gewinne, finalisierter Verluste, zugewiesener Kosten und unbekannter/gesperrter Vorgänge. Die Grundlage und das Accounting-Fenster müssen versioniert sein.
3. `spendable_asset_balance` pro Token = finalisierter und ausreichend aktueller Token-/SOL-Kontostand minus aktive in-flight / FE-04-/P8-Reservations, noch unbestätigte Sends, Fee-/Tip-/Rent-/ATA-Reserve, aktive Funding-/Liability-Bindung, Mindestarbeitskapital, andere Treasury Holds und Risiko-/Unknown-Puffer. `Unknown` = nicht verfügbar, niemals Null.
4. `withdrawable_asset_amount` = konservatives Minimum aus wirtschaftlich gewinnbezogenem **in genau diesem Asset disponierbarem** Wert und spendable Tokenmenge; USD-Schwelle erfordert eine qualifizierte aktuelle Preisquelle, verlässliche decimals/Einheiten und festgehaltene Bewertungszeit.
5. Wenn Gewinn in Collateral/Mint A entsteht, Empfang aber Stablecoin B verlangt: zunächst **eigenständige, kosten- und risikogeprüfte** Exit-/Swap-Route, die nicht automatisch per Treasury-Gewinnregel autorisiert ist. Keine USD-Schattenumrechnung als spendbarer USDC.
6. `trigger_threshold` gilt für **neu realisierten, disponiblen Gewinn**, nicht für den kumulierten historischen Gewinn oder bloß einen neu hohen Walletsaldo. Ein erfüllter Trigger darf nach einem Transfer nicht erneut feuern, bevor neue Gewinndaten und eine neue Epoch vorliegen.

**Invariante:** `max_transfer = min(unswept_settled_profit_in_asset, post_reservation_spendable_asset, user_policy_cap)`. Sind Bestand, Profitbasis, Empfänger oder FX/Finality nicht beweisbar, ist `max_transfer` **UNKNOWN/BLOCKED**. Keinen Transfer mit Restbetrag unter Fee-/Rent-/Working-Capital-Grenze senden.

## 4. Manuelle Auszahlung – sicherer Benutzerablauf

1. Walletquelle wählen → Ziel `Phantom (Solana)` oder `MetaMask (Solana)` aus registrierter Allowlist auswählen.
2. Asset/SPL-Mint (bzw. SOL) wählen, tatsächlich verfügbares `max_transfer` und Gewinnreserve sehen; Betrag beliebig positiv bis zur konservativen Obergrenze eingeben oder `nur verfügbaren Gewinn` anklicken.
3. `Vorschau` ist read-only: Source/Destination-Pubkey, Chain/GenesisHash, Mint/Decimals, ATA, Fee-Payer, Rent, Compute-/Priority-Kosten, gewünschter Endbetrag, voraussichtlicher Restbestand, aktuelle Reservierungen, TTL/Blockhash und eindeutige Transfer-ID. Bei unbekannter Fee/Quelle/Finality: **Block**.
4. `Überweisung freigeben` ist eine **separat bestätigte value-bearing Aktion**. Signieren ausschließlich im Rust-/sicheren Signerpfad, Browser sendet niemals Private Key/Seed. Für echte Mainnet-Gelder ist eine **separate, konkrete Nutzerautorisierung** erforderlich, die Ziel, Asset, max Betrag, Gebühren, Zeit- und Sicherheitslimits umfasst; FE-06-Freigabe für Liquidation autorisiert Treasury nicht automatisch.
5. Wallet-Reservation/Intent in kanonischem persistenten `.p0j` vor Sign/Send durable (fsync) sichern; mit FE-04/P8 globalen, atomaren Multi-Asset-Reservierungen teilen; `p0ctl` darf neben laufender Trading-Engine keinen zweiten unabhängigen Writer oder konkurrierenden Signer-Pool öffnen.
6. Native SOL-Systemtransfer oder SPL `TransferChecked` mit exaktem Mint/Decimals/Token-Program/ATA (Token-2022 Extensions und Transfer Fees beachten), Simulation soweit unterstützt, dann **genau einen** vom Ledger gebundenen Submit. Status `RESERVED → SIGNED → SUBMITTED/UNKNOWN → FINALIZED_SUCCESS|FINALIZED_FAILED`; Zustand `UNKNOWN` erhält Holds und **verhindert Re-Send** bis eigenständiger Reconciliation.
7. On-chain finalisierte Signatur, Token-/SOL-Deltas, Gebühren, Empfänger und Restkapital nachprüfen; erst dann Sweep-Buchung abschließen und Anzeige aktualisieren. Fehlendes Ergebnis nicht als gesendeten Gewinn zählen.

## 5. Automatische Gewinnabschöpfung (konfigurierbare Standing Policy)

**SOLL: `TreasurySweepPolicy`**, pro `source_wallet,chain_id,destination_id,asset_mint`, versioniert und mit starker spezifischer Benutzerfreigabe. Standard ist `DISABLED`.

| Policy-Feld | Zweck |
|---|---|
| `enabled` | explicit opt-in; per Default false |
| `sweep_mode` | `MANUAL` oder `AUTO` |
| `profit_threshold` | frei konfigurierbar, Einheit Asset oder belegter USD-Wert |
| `sweep_amount_rule` | fester Betrag, Anteil am auszahlbaren Gewinn, alle **freigegebenen** Überschüsse |
| `min_keep_asset_amount` | Mindestbestand je Asset, zusätzlich Trading-/Fee-/Rent-/Funding-Reserve |
| `min_trading_capital_usd` | gebunden an echte disponierbare, liquiditätsbereinigte Vermögensbasis, nicht Float-Schätzung |
| `max_per_transfer, max_per_day, max_total, max_auto_transfers_per_period` | separate frei parametrierbare Betrags-/Frequenz-/Laufzeitlimits |
| `fee_limit, slippage_limit` | nur zulässige Gebühren/Exit-Schritte, jeweils getrennte Autorisierung |
| `destination_allowlist_id, chain_id, token_mint, token_program` | keine stille Ketten-/Empfänger-/Assetmutation |
| `cooldown, approval_expires_at, policy_hash, authorized_scope` | wiederholte Trigger/Änderungs-/Zeit-/Limit-Risiken begrenzen |
| `min_finality, reconcile_before_next, halt_on_unknown` | finalized Chain-Proof und STOP vor erneuter Zahlung |

**Triggerlogik:** `eligible_new_realized_profit >= threshold` UND `wallet_available >= required_reserve + payout + fees` UND `destination verified` UND `policy authorization valid` UND `no unresolved pending/UNKNOWN` UND `provider/chain check pass` → atomare Reservation → gebundene Signierung → Senden → finalisierte Reconciliation.

**AUTO ist möglich, aber erst nach ausdrücklicher, dokumentierter Standing-Autorisierung** für genau Quelle, Ziel, Asset, Limits und Gültigkeit. Kein Bestätigungsdialog bei **jeder** regelkonformen Zahlung erforderlich, sofern die genehmigte Policy dies deckt; jede Policyänderung, neue Wallet oder Überschreitung erfordert erneute Freigabe. Sofortiger Pause-/Disable-/HALT-Schalter, Watchdog, Drift-/Fee-/Balance-Limits, gebundene Test-Evidence. Keine endlose Retry-Schleife. Nicht automatisch von LIVE-disabled in Echtgeldbetrieb wechseln.

**Wirtschaftlicher Hinweis:** Auszahlung ist Vermögensumschichtung, **keine** zusätzlich erzielte Bot-Rendite. Gebühren und möglicher Kapital-/Opportunity-Verlust gehen in die spätere Economics ein; `realized_pnl` und `transferred_to_treasury` bleiben zwei verschiedene Kennzahlen.

## 6. Reale Code-Zuordnung – was bereits existiert

| IST-Pfad/Typ | Status | Nutzung für FEUI-10 |
|---|---|---|
| [`config/wallet.public.env`](../../project_0/config/wallet.public.env) `P0_EXPECTED_PUBKEY,P0_WALLET_STATUS` | öffentliche Identität vorhanden | Initiale Source-Wallet; kein Signer-Zugriff |
| [`execution/one_shot.rs`](../../project_0/native/crates/runtime/src/execution/one_shot.rs) `Approval,Intent,Current,Journal,WalletLock,Phase::{HandoffUnknown,HaltFinalized},Receipt` | FE-04 lokal teilweise, kein Treasury-Adapter | Mustervorlage für exklusive Wallet-Reservation, durablen Send-Intent, UNKNOWN/Finalized und Fee-Holds. **Nicht** eigenmächtig Treasury in bestehende Liquidation-One-Shot-Semantik quetschen; gemeinsames Reservation-/Journal-/Send-Gate wiederverwenden |
| [`execution/one_shot/receipts.rs`](../../project_0/native/crates/runtime/src/execution/one_shot/receipts.rs) `FinalizedObservation,fetch_finalized,observe_finalized` | finalisierte Walletdeltas lokal implementiert | kein fiktiver Transferstatus, Signatur- und Balances-Proof |
| [`execution/pending.rs`](../../project_0/native/crates/runtime/src/execution/pending.rs) `PendingSubmission,ReconcileRecord,ReconcileResult` | Submission-/Reconcile-Basis | Gebühren/Non-Landing/UNKNOWN getrennt |
| [`liquidation/economics.rs`](../../project_0/native/crates/liquidation/src/economics.rs) `CandidateInput,Evaluation,FailedCostMatrix` | lokaler Economics-Core | keine Shadow-Profite als Sweep-Funds |
| [`liquidation/funding.rs`](../../project_0/native/crates/liquidation/src/funding.rs) `FundingClass,FundingContext` | Funding-Basis | Arbeitspuffer, Tokenbedarf und Reserven |
| [`analytics/journal.rs`](../../project_0/native/crates/runtime/src/analytics/journal.rs) | kanonische `.p0j`-Frames | versionierte neue Wallet/Treasury-Budget-/Intent-/Receipt-Kinds mit JFI-/PERSIST-Abnahme |
| [`docs/STATUS.md`](../../project_0/docs/STATUS.md) | FE-04 PARTIAL_LOCAL, FE-07 NOT_RUN, P8-INV OPEN, FE-06 NOT_AUTHORIZED | **keine** vorhandene live-autorisierte automatische Treasury-Funktion |

**SOLL-Rust-Typen:** `WalletPortfolioSnapshot`, `TreasuryDestination`, `TreasuryAvailability`, `TreasurySweepPolicy`, `SweepIntent`, `SweepReceipt`, `SweepPolicyState`, `SweepAudit`; Signier-/Transfer-/Refresh-Zeit-/Scopeverträge erst beim Backend-Code definieren. **SOLL-Frontend:** `frontend/src/features/project0/pages/WalletsTreasury.tsx`, `components/WalletBalances.tsx`, `ManualSweepPanel.tsx`, `SweepPolicyPanel.tsx`, `SweepHistory.tsx`; shared wallet overview nur abstrahiert, keine gemeinsame Transaktions-Engine mit BSC.

## 7. UI-/Control-Kommandos (SOLL, keine aktuellen Funktionen)

~~~bash
p0ctl wallets list --project project_0
p0ctl wallets inspect --wallet SOURCE_PUBLIC_KEY
p0ctl treasury destinations list
p0ctl treasury destination verify --id DESTINATION_ID
p0ctl treasury preview --source SOURCE_ID --destination DESTINATION_ID --asset ASSET_ID --amount ATOMIC_AMOUNT
p0ctl treasury send --intent TRANSFER_INTENT_ID       # nur konkrete separate user approval
p0ctl treasury policy show --source SOURCE_ID
p0ctl treasury policy propose --source SOURCE_ID --destination DESTINATION_ID
p0ctl treasury policy authorize --policy POLICY_ID    # separate begrenzte Standing Authorization
p0ctl treasury policy disable --policy POLICY_ID
p0ctl treasury reconcile --transfer TRANSFER_ID
~~~

**Browser-API SOLL:** `GET /api/v1/project0/wallets`, `GET /api/v1/project0/wallets/:id`, `GET /api/v1/project0/treasury/destinations`, `POST /api/v1/project0/treasury/previews`, `POST /api/v1/project0/treasury/intents`, `POST /api/v1/project0/treasury/intents/:id/authorize`, `GET /api/v1/project0/treasury/transfers`, `GET/POST /api/v1/project0/treasury/policies`, `POST /api/v1/project0/treasury/policies/:id/disable`. **Jede Route muss noch gebaut und nach JFI validiert werden.** Keine generische beliebige `send`-Route, keine unsignierte Nutzereingabe direkt zu `solana_sdk::Transaction`.

## 8. Test-/Abnahmegates

- **W-G01 Public identity:** alle Wallet-/Chain-/Mint-/Owner-/ATA-Konten live im read-only Probegebiet korrekt; falsche oder BSC-/Solana-verwechselte Adresse, falsche Mints/Decimals/Token Extensions, Programmupgrade → BLOCKED.
- **W-G02 Profit-vs-balance:** kein Transfer auf Basis von Gross-/Shadow-/Unrealized-PnL; finalisierte eigene Trading-PnL, verfügbarer Saldo, Reservierungen, unverrechnete Fees/Positions, Mint/Price-Provenienz deterministisch und vollständig.
- **W-G03 Capital safety:** Trading-/Funding-/Tip-/Fee-/Rent-/SOL-Restreserven atomar; gleichzeitige Liquidation + Treasury dürfen nie gleiche Token mehrfach reservieren; P8-RES/P8-INV-Integration.
- **W-G04 Manual transfer:** preview/auth/bind/sign/send/finalized/reconcile, minimaler Testtransfer nur mit separater Freigabe; unbekannter Zustand nie retry/double-send, Fail-Closed bei Crash/FSYNC-/RPC-Fehler.
- **W-G05 Auto payout:** frei wählbares X/Asset/Teilquote, Limits/Frequenz/Cooldown/Gültigkeit, nur unswept neue Profit-Epochen; keine zweite Überweisung bei Reconnect/Restart/Slot-Fork/UNKNOWN, Adressänderung/Policyexpiration blockieren.
- **W-G06 UI:** Guthaben vs Profit vs spendable vs Reserved vs Transferred vs Unknown, vollständige Source-/Recipient-/Amount-/Fee-/Finality-Evidence, keine Secrets; manuelle und automatische Transfers getrennt markiert.
- **W-G07 No value without approval:** Echtgeld-/Treasury-Gate getrennt von Capture/Optimizer/FE-06; ohne dokumentierte konkrete bzw. begrenzte Standing-Autorisierung keinerlei Mainnet-Send. Keine API-/Wallet-Credentials in Browser.
- **W-G08 Compatibility:** keine BSC-Änderungen; Project_0-JFI/native `.p0j`, bestehende FE-/WalletLocks/Journal-/Receipt-Regeln bleiben wirksam; Offline-Testfälle ohne Mainnet/Signer.

## 9. Implementierungsreihenfolge

- **FEUI-10A (früh, read-only):** Public Wallets/Chain-/Mint-/Token-Balances, offene Reservations und transparente Source-/Proofqualität anzeigen. Dies kann parallel zum bestehenden Frontend-Control-Center umgesetzt werden; bei fehlendem Ledger echte Werte UNKNOWN anzeigen.
- **FEUI-10B (nach FE-04/P8 und echtem FE-07-Finalized Accounting):** manuelle sichere Treasury-Vorschau, Transfer-Intent, separater Auth-Gate, native Transfer/Receipt/Recovery und History. Nicht Teil des 4h-Capture-Gates.
- **FEUI-10C (nach W-G01…07):** automatische, eng gebundene Standing-Policy `realized_profit >= X` mit Limits/Reserven/Epoch-Gates, Monitoring, Testfällen und Kill Switch. Auch wenn FE-06-Liquidationsversuche später erlaubt sind, entsteht damit keine automatische Treasury-Erlaubnis.

**Keine neuen Wallet-Adressen, Mindestkapitalwerte, Prozentzahlen, Transfer-Schwellen oder Zielketten hardcoden.** Nutzer wählt sie, Runtime validiert; der Code trennt grundsätzlich read-only Anzeigen von Geldtransferfähigkeiten.
