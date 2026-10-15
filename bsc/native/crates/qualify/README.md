# Q5A — reale TLS-Transport-Qualifikation

`u5_tls_qualify` misst den eingefrorenen Produktions-Transport (`TlsConnectionOwner`, `TlsTransport`) gegen die realen Builder-Endpunkte und schreibt Evidenz. Es qualifiziert nichts selbst: kein `QualifiedTransport`, keine Provider-Eigenschaft wird auf qualifiziert gesetzt.

Gesendet wird nur der sichere Probe:

```json
{"jsonrpc":"2.0","id":"q5a-<provider>-<phase>-<nnn>","method":"eth_sendBundle",
 "params":[{"txs":[],"maxBlockNumber":0,"noMerge":true,"positionFirst":true}]}
```

Keine Transaktion, kein Wert. Ein JSON-RPC-Fehler ist die erwartete Antwort.

## Ablauf auf dem Zielsystem

1. **Lock ergänzen (offline, transaktional):**
   ```bash
   sh bsc/tools/lock_qualify.sh
   ```
   Es kommt genau ein Paket dazu, `darkbot-qualify`; das Skript bricht sonst ab und stellt den Lock wieder her.
2. **Gates** wie gewohnt; erwartet 271 + 32 = 303 Tests.
3. **Committen.** Nur aus einem sauberen, committeten Baum kann der Lauf PASS werden: Das Build-Skript bindet Commit, „Baseline ist Vorfahr", „eingefrorene Quellen seit `92fa520` unverändert" und „Arbeitsbaum sauber".
4. **Release bauen:**
   ```bash
   cargo build --release --locked --offline -p darkbot-qualify
   ```
5. **Messen**, das Token ohne Shell-History:
   ```bash
   read -rs Q5A_BLOCKRAZOR_AUTHORIZATION && export Q5A_BLOCKRAZOR_AUTHORIZATION
   Q5A_EVIDENCE_PATH=q5a-tls-evidence.json bsc/native/target/release/u5_tls_qualify
   ```
   Dauer etwa 25 s je Provider (10 + 100 Proben im Abstand von 200 ms, dann drei Reconnects mit Probe). Exit-Status: 0 PASS, 1 FAIL, 2 INCOMPLETE, 3 Harness-Fehler.
6. **Gegenprüfen**, mit dem Token noch in der Umgebung (sucht es in der Datei):
   ```bash
   python3 bsc/tools/q5a_check_evidence.py q5a-tls-evidence.json
   ```
   Der Prüfer wendet dieselben Regeln wie der Evaluator an, rechnet jede Übergabedauer aus den Roh-Zeitstempeln nach (`t_boundary_ns − t_write_check_ns`) und bildet daraus Zusammenfassung, Ursachen und Urteile. Jede Abweichung von der Datei meldet er als INCONSISTENT.

## Umgebung

| Variable | Bedeutung |
|---|---|
| `Q5A_BLOCKRAZOR_AUTHORIZATION` | BlockRazor-Authorization. Wird nie ausgegeben oder geschrieben; die Evidenz wird vor dem Schreiben darauf geprüft. Fehlt sie, wird BlockRazor nur bis zum Handshake gemessen (`UNQUALIFIED_AUTH_MISSING`). |
| `Q5A_PROVIDERS` | `club48`, `blockrazor` oder beide (Default). Mit nur einem Provider kann der Lauf nicht PASS werden. |
| `Q5A_EVIDENCE_PATH` | Ziel der Evidenz. Eine bestehende Datei wird nie überschrieben. |

Hosts: `puissant-builder.48.club:443` und `virginia.builder.blockrazor.io:443`, fest aus dem eingefrorenen TLS-Modul. Ein BlockRazor-Fast-Submit-Host wäre eine eigene Qualifikation mit eigener Generation.

## Was diese Evidenz nie qualifiziert

48Club `positionFirst` in der Blockplatzierung, `noMerge`, Reihenfolge, Atomarität und Same-Block beider Builder, Pre-Submission, Platzierung N/N+1, Builder-Inclusion, jede Provider-Eigenschaft, `T0 → T_buy`. Die Liste steht auch in jeder Evidenzdatei.

Die Regel, nach der eine vollständig gerahmte HTTP-Antwort mit einem anderen Status als 200/429 die Verbindung für die nächste Probe wieder `Ready` setzt, gilt nur in diesem Harness. Sie qualifiziert ausdrücklich nicht die Antwort-Lebenszyklus-Semantik des Produktions-Runners, der eine solche Verbindung `Stale` setzt.
