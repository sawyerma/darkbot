# Strategy B – ausschließlich LEAN als historische Referenz

Aktuelle Autorität: [STRATEGY_B_SOURCE_OF_TRUTH.md](STRATEGY_B_SOURCE_OF_TRUTH.md), Arbeitsbasis `367c97cae79bc6d3517917786fdc0b00b9b812fb` (R8_2c). Technische Architektur, Quotes und Sicherheit kommen aus aktueller Engine/SPEC. Historische Dokumente überschreiben diese Vorgaben nicht.

## Freigegebene unveränderte Referenz

`/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/alt/Concept/lean_v1_frozen.zip`

SHA-256: `1e0b39e47e87e4b1f46d7a3ea5e5f8913809c90a3b2692837fba3083b73b4fd0`.

Enthält `LEAN_V1.md` und `DarkBot_LEAN_V1_v1.2.zip`. Inhalte nur im Speicher gelesen, keine Extraktion in den Produktivbaum und kein Ausführen alter Infrastruktur. Keine Rohaufnahme des positiven Legacy-Laufs in diesem Archiv vorhanden.

Transparenz zur Auftragsänderung: Die erste Aufgabenfassung hatte zusätzlich `backend.zip` als Referenz freigegeben. Dieses wurde vor Eingang der neuen Source of Truth lesend untersucht. Die neue Fassung schließt es aus: keine weitere Nutzung, keine Übernahme seiner Infrastruktur und keine Ableitung der aktuellen Policy daraus. Die aktive Semantikprüfung unten stützt sich ausschließlich auf das freigegebene LEAN-Archiv und den neuen Auftrag.

## LEAN-Semantik und ausdrücklich übersteuernde Vorgaben

| Thema | Freigegebene historische Evidenz | Aktuelle Strategy B |
|---|---|---|
| Initial-BUY | Verschachteltes `backend/darkbot/market.py:599–603` schließt gleichmintige Trades der CREATE-Transaktion aus Featurezählern aus. | Initial-BUY zählt nicht zu 5/5; State-Verarbeitung bleibt modern. |
| BUY / Unique | Verschachteltes `market.py:425–429`: jedes BUY erhöht den Zähler; Wallet-Schlüssel bestimmt Unique. | Gültige kanonische BUYs und tatsächlicher Actor; kein leeres Wallet als Unique zählen. |
| Parameter | LEAN ist ein allgemeines Filter-/Backtest-Profil, keine verbindliche 5/5-Konfiguration. | 1.000 ms / 5 BUY / 5 Unique / Target 1,05 / Hold 60 s aus neuer Source of Truth. |
| Stop | LEAN beschreibt einen Netto-Stop-Schwellwert. | Explizit kein gewöhnlicher Stop; `stop_net_multiple=0` darf keinen versteckten Stop erzeugen. |
| Slots | LEAN beschreibt V1 mehrfach mit einem Slot. | Gemeinsamer Portfolio-Parameter, Szenarien 1 und 10, kein Policy-Hardcode. |
| USD | LEAN Abschnitt 8 trennt Swap-Budget einschließlich Protokoll-/Creator-Gebühren von zusätzlichem Netzwerk/Rent. | **$10 Swap-Budget einschließlich Pump-/Creator-Gebühren; Base/Priority/Rent zusätzlich**, nach heutiger QuoteEngine. |
| Unbekannte Fills | LEAN beschreibt u.a. `UNRESOLVED_ZERO`. | Nicht übernehmen: heutige UNKNOWN/CENSORED-/Coverage-/Kostenregeln gelten. |

## USD-Klarstellung

Die neue Source of Truth präzisiert die kurze Nutzerantwort „$10 Swap-Budget; Gebühren/Rent zusätzlich“. Die zwischenzeitliche Auslegung als $10 reiner Curve-Input wird verworfen. Der evidenzbasierte SOL-Betrag begrenzt den Swap einschließlich Pump-/Creator-Gebühren. Zusätzliche Infrastrukturkosten werden getrennt verbucht. Keine alte Gebührenformel übernehmen, keine konstante Lamport-Umrechnung.

## SOL/USD-Quellvertrag

Optionaler öffentlicher Spot-Abruf außerhalb des Hot Paths: `GET https://api.coinbase.com/v2/prices/SOL-USD/spot`. Die [offizielle API-Dokumentation](https://docs.cdp.coinbase.com/coinbase-app/track-apis/prices) beschreibt das Pair-Schema und Zugriff ohne Authentifizierung. Ein read-only Schema-Probeabruf am 2026-09-19 um 22:48:43 UTC lieferte HTTP 200, `data.base=SOL`, `currency=USD`, decimal-string `amount`.

Der Probe-Preis wird nicht als Konstante oder Ersatz historischer Kurse verwendet. JSON enthält keinen Markt-Zeitstempel: lokale Empfangszeit ist Empfangsevidenz, keine behauptete Exchange-Tickzeit. Ohne passende gespeicherte USD-Evidenz ist eine alte Aufnahme kein qualifizierter $10-Strategy-B-Nachweis.

Die historische Referenz (+$9,55 bei 21 Trades / etwa 897,65 Sekunden) bleibt ausdrücklich unbestätigt; kein aktueller Profitnachweis.
