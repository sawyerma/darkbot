# DarkBot Protocol – gemeinsame Wire-Schemafamilie

Autorität: `frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md` (Transport) und
`frontend/docs/GOVERNANCE_UND_SICHERHEIT.md` (Freigaben). Dieses Verzeichnis ist die
einzige Quelle für Rust (`prost`) und TypeScript (`protobuf-es`). Handgeschriebene
Codecs sind verboten.

## 1. Pakete

| Paket                 | Inhalt                                                   | Status |
|-----------------------|----------------------------------------------------------|--------|
| `darkbot.control.v1`  | Envelope, Handshake, Topics, STATE/EVENT, Commands, Numerik | SOLL, F2 |
| `darkbot.project0.v1` | typisierte Project_0-Projektionen und Command-Params      | nach F0-VAR |
| `darkbot.bsc.v1`      | typisierte BSC-Projektionen und Command-Params            | nach F0-VAR |

`control/v1` importiert keine Domänenpakete. Payloads sind `bytes`, statisch an
`TopicId` gebunden (`TopicDescriptor.payload_type`). Der Client hält dieselbe Tabelle
zur Compile-Zeit und verweigert Topics, deren Bindung im `Welcome` abweicht.
`google.protobuf.Any` und JSON-in-bytes sind nicht erlaubt.

## 2. Nachrichtensatz (exakt 11)

Client: `Hello`, `Subscribe`, `Unsubscribe`, `Pong`
Server: `Welcome`, `ControlAck`, `StateFull`, `Event`, `CatchupComplete`, `Heartbeat`, `Error`

`StatePart` ist Feld 8 in `ServerMessage`, reserviert, nicht implementiert.

## 3. Versionierung

- `protocol_major` / `protocol_minor` nur im Handshake. Major-Mismatch = fail-closed.
- Minor-Erhöhung: nur additive optionale Felder. Alter Client muss neue Frames lesen,
  neuer Client alte Frames. Golden-Test in beide Richtungen.
- Field-Nummern werden nie wiederverwendet. Entfernte Felder: `reserved N; reserved "name";`.
- `schema_id` = SHA-256 über sortierte Proto-Relativpfade + byte-längenpräfixierte
  Dateiinhalte (`scripts/schema-id.sh`). Im Hello/Welcome nur Fingerprint/Diagnose:
  **ungleiche Hashes dürfen bei kompatiblem Minor-Schema nicht automatisch sperren**.
  Major-Mismatch, inkompatible Änderungen und falsche TopicDescriptor-Bindungen sperren.
- Remote-Plugin-Versionen in `buf.gen.yaml` sind feste Kandidaten, jedoch hier noch
  **NOT_RUN**. Erst `buf lint`, `buf generate`, Rust/TS-Build und Goldens ausführen,
  passende Runtime-Dependency `@bufbuild/protobuf` im Frontend installieren und
  Versionen/Lockfiles qualifizieren; niemals bloß aufgrund der YAML ein PASS ausweisen.

## 4. Codegen

    cd protocol
    ../protocol/scripts/schema-id.sh
    buf lint
    buf generate

Schema-Decimal-Wrapper werden durch protobuf selbst nicht auf Format oder Wertebereich
validiert. Rust/TS-Domainadapter müssen kanonisches `-0`/führende Nullen,
Signed-/Unsigned-Grenzen und feldspezifische Einheiten prüfen. Der EVENT-Cursor
`uint64` ist erst nach F0-VAR-Zuordnung zu beiden nativen Journalpositionen freigegeben.
STATE-Publishergeneration muss persistent streng monoton sein; ohne nachgewiesene
Quelle für Engine-/Gatewaypublishergeneration kein F3-PASS.

TypeScript landet in `frontend/src/core/generated/` (nur generiert, nie editieren,
in ESLint-Boundaries nur aus `core/realtime/**` und `features/*/api/*Adapter.ts` importierbar).
Rust landet in `protocol/gen/rust/` bis das Gateway-Crate in F3 seinen Pfad festlegt.

## 5. Golden-Fixtures (F2-Abnahme)

`fixtures/` enthält deterministische `TEST_ONLY`-Vektoren, niemals fachliche Evidenz:

- `hello_welcome_accepted.bin`, `welcome_rejected_major.bin`
- `state_full_uint64_max.bin` – Typgrenzentest für generation/seq = u64::MAX,
  source_timestamp = i64::MAX; kein live-fähiger Generation-Wraparound
- `numeric_signed_decimal_negative.bin` – `"-123456789012345678901234567890"`
- `numeric_wei_u256_max.bin`
- `event_cursor_pair.bin`, `catchup_complete_incomplete.bin`, `catchup_complete_duplicate_replay.bin`
- `server_message_minor_added_field.bin` – vom neueren Schema erzeugt, vom älteren gelesen

Jede Fixture wird in Rust encodiert, in TS decodiert und wertgleich verglichen, und umgekehrt.
PASS nur nach realem Lauf mit Exit-Code; sonst NOT_RUN.

## 6. Was hier nicht hingehört

Keine Secrets, keine RPC-URLs, keine Signer-Typen, keine Engine-internen Journalformate
(`.p0j`, DBREC bleiben nativ). Keine Freigabe-Logik: ein Schema beweist keine Ausführbarkeit.
