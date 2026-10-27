# DarkNode – P9/P10: Echtgeld-Freigabe und Bot-Fleet-Gates V1

Stand: 27.10.2026. **Nur künftige Soll-Verträge.** Keine Echtgeldfreigabe, keine Orders, kein P9/P10-Test durchgeführt. P9 ist absichtlich vom technisch funktionierenden Demo-Bot getrennt.

## P9: Formale, manuell autorisierte LIVE-Entscheidung

**Eingangsvoraussetzungen (alles am exakt gleichen Build-/Graph-/Adapter-/Account-Policy-SHA):** Gate 1 und P0/P1/P2/P3/P4/P5/P6/P7/P8 abgezeichnet; alle Required-CI-/DN-PROTO-/P5-A-/P1-Gates tatsächlich PASS, offene FAIL/BLOCKED/NOT_RUN sicherheitskritischer Fälle schließen LIVE aus. OOS-Kosten-/Fill-/Funding-/Nonfill-Qualifikationsreport separat positiv bei vorab fixiertem Testkriterium; reine Backtest-Rendite kein Safety-Beleg.

**Kontokonfiguration:** gesondertes Echtgeld-UTA-V3-Unterkonto, geeignete juristische/vertragliche Zugangsberechtigung für Futures in der eigenen Jurisdiktion tatsächlich geprüft, Read/Trade-API-Key ohne Withdrawal/Transfer mit IP-Allowlist, erwarteter One-way+Isolated-Mode, tatsächlicher Hebel und risikobasierte Quote/Notional-Grenzen; keine Annahme wegen erfolgreicher Demo. Leere Kontokonfiguration oder ungeklärte Mode-/Stop-Capability ⇒ NO_ENTRY.

**Manual Sign-Off Record:** Datum und Rolle des freigebenden Menschen, genauer Git-/Docker-/Strategy-/Instrument-/Fee-Hash, API-UID-/Key-ID-Referenz (niemals Secret), Zahl der bestandenen Fault-/Stop-/Reset-/Restore-Tests, Hostregion/Latency/Watchdog, genehmigte Risiko-/Kapital-/Drawdownlimits und Liquidationsbuffer, erlaubtes Symbol, Stop-Typ/Trigger, die Account-/Positions-/Stop-Baseline und die unabhängige Alarmprobe. P9-Arming ist **einmalige versionierte Entscheidung** mit Ablauf/Revocation; jede materielle Strategie-/Risk-/Stop-/Hoständerung führt wieder durch die relevanten Gates.

**Pilot:** nur nach separater expliziter Echtgeld-Orderfreigabe und freigegebener Minimalposition. Eng definierter Stop-/Fill-/Fee-/Funding-/Slippage-/Reconcile-Capture; keine schleichende Größensteigerung und kein automatisches Aufrüsten durch System-Neustart. Bei Accountdrift, Stopverlust, alter Equity, unbekanntem Orderstatus, Gateway-Konflikt oder Funding-/Liquidation-Pufferbruch: NO_ENTRY/REDUCING/Alarm, kein unkontrollierter Close-Retry.

**Fallbacks:** HA-/Container-Upgrades erzeugen niemals zwei gleichzeitige signierende Order-Owner; bei Host-/Netz-/Keyausfall bleiben börsenseitige Stops nur im tatsächlich bestätigten Rahmen aktiv. Verschwundene lokale Exit-/Trailing-Logik ist kein vollständig geschützter Zustand. Ein fehlender unabhängig erreichbarer Alarm blockiert P9.

## P10: Von einem Bot zur kontrollierten Fleet

- Mindestens zwei Botinstanzen zuerst **unterschiedliche** feste Symbole und immutable Graphversionen; jeder Bot hat eindeutige BotID, Kapital-/Risikobudget, Position-/Order-/Stop-/PnL-Journalreferenzen, eigene Signal-/Rearm-States. **Ein** Portfolio-/Account-Risk-Owner aggregiert alle Reservierungen und Exchange-Positionen – keine separaten signierenden Owner pro Bot auf demselben UTA-Konto.
- Identischer Futures-Kontrakt für mehrere Bots **verboten**, bis eine eigens versionierte Netting-/Allocation-Engine technisch nachgewiesen und freigegeben ist. Kein automatischer Coinwechsel aufgrund Performance.
- Gemeinsame UID-Budgets vor jeder Order: Available/Equity, Summe aus bestätigter Position + Working/Pending/UNKNOWN Exposure, Margin/Liquidations-/Funding-Buffer, per-Bot/UID-Notional, Drawdown und Stale-/Quality-Gates; nicht die Exchange-Margin doppelt als Budget abziehen.
- Race-Matrix: zwei Bots wollen zugleich neuen Entry; fremde Bitget-App-Order, globales HALT, ein Bot-Stopversagen, ein Bot-Disconnect, ein BTC-/ETH-Preisgap und zwei gleichzeitige Emergency-Schließungen. Ein Kontoausfall führt zu gemeinsamem konservativem Zustand; nicht betroffene Bots dürfen nicht still den Risk-Owner umgehen.
- Frontend-Botübersicht/Wallet zeigt Account-Total und *explizit interne* Bot-Zuteilung getrennt, keine „vier getrennten Bitget-Wallets“-Behauptung. Telemetrie und Eventcursor pro Bot und Konto nachvollziehbar.
- Fleet-Abnahme: deterministischer Allokations-/Fairness-/Race-/Restart-/Drop-Capture plus Demo-Soak bei maximal geplanter Botzahl, Latenzbudget unter gleichzeitiger Last, wiederholbarer Git-SHA, kein Doppelorder-/Overexposure-/Stop-Mismatch-Fall.

## Rollback und Qualitätslabels

Versionierte Rücknahme für einzelne Bot-Konfiguration und Gesamtfleet; keine fremden Pending-Orders beim Downgrade ignorieren. Wirtschaftlich, technisch und autorisiert sind **drei getrennte** Gate-Dimensionen. Keine automatische Hochstufung nach profitabler Woche. P9/P10 bleiben NOT_RUN bis zur tatsächlichen Durchführung.