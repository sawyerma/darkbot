# Fable Review — PHASENPLAN v4.4 → AUTHORITATIVE

Datum: 2026-09-30
Ergebnis: **keine materielle Korrektur nötig. v4.4 promoted.**
Eine nicht-materielle Wortkorrektur (§3, v1-Absatz): "64 Accounts nach v1-Spezifikation" → Runtime-Limit für legacy/v0/v1 (Solana Foundation, Upgrade-Seite).

## Geprüft gegen Primärquellen (in dieser Sitzung selbst geholt)

| Aussage in v4.4 | Quelle | Status |
|---|---|---|
| Tx v1: 4096 B, keine LUT, 64 Accounts, CU/Loaded-Data Default 0, Priority Fee Gesamt-Lamports, ComputeBudget-Ix No-op, base64 >1232, aktiv seit Epoch 1035 | solana.com/upgrades/larger-transaction-sizes | BELEGT |
| Yellowstone: `Message.config` ab proto 12.6.0, Plugin ≥15.1.1, Erkennung config → versioned → legacy | dieselbe Seite | BELEGT |
| Jito-Solana ≥ 4.2.2 für v1 in Blöcken | dieselbe Seite | BELEGT (Gate bleibt Messung) |
| QuickNode `fromSlot` ~3000 Slots, `subscribeReplayInfo` | QuickNode-Doku | BELEGT |
| Helius Free: 0 $, 1M Credits, 10 RPS, gPA 5/s, Standard-WSS, kein Mainnet-gRPC; WSS 2 Credits/0,1 MB | helius.dev/pricing, docs/billing | BELEGT |
| Staked Connections nur in Paid-Plänen (SWQoS-Realität für unstaked Direct-TPU) | helius.dev/pricing | BELEGT |
| eva01: `marginfi` Crate als Health-Engine, Tag 0.1.11-rc1, superagg = Remote-Client (Jupiter/Titan/DFlow), kein v1, keine LICENSE | eva01 README + Cargo.toml, superagg README | BELEGT |
| marginfi-v2 Apache-2.0 | github.com/0dotxyz Org-Seite | BELEGT |
| 0.1.12: Tagging, Premium-Kurve, Bank-Resize 1856→3904 (ohne Discriminator), Scope 27/28, Costly 4, neue Receivership-Regel, Flat Fee, Mainnet-ETA 12.10. | Release mrgn-0.1.12-rc1 | BELEGT |
| `start_liquidation`: `end_liquidation` letzte Ix der TX, Instructions-Sysvar | IDL-Doku (decoders) | BELEGT |
| LiquidationCache: maint = SPOT, equity = EMA; Seize-Grenze nicht aus Kommentar, sondern aus `liquidate_end.rs` | IDL-Doku + v4.4 §P3 | KORREKT BEHANDELT |
| superagg MIT | nicht selbst gesehen | von Astra bestätigt, offen für P0-02 |
| eva01 `MIN_PROFIT`-Check in `liquidator.rs` | nicht selbst gesehen (GitHub-Verzeichnis gesperrt) | offen für P0-01 |

## Checkliste 12A — Antworten

**A Ingress.** S1-Trennung korrekt. `SubscribeDeshred` ist pre-execution und ausdrücklich nicht kanonisch; K-Einordnung korrekt. Kostenlose Mainnet-Binary-Quelle: keine belegt; S1-08 misst. Bis dahin `S1-LT` allein.

**B Submission.** Direct TPU nach K: korrekt. Unstaked Kapazität: von Agave stake-gewichtet vergeben, Rest für unstaked Peers ist klein und wird unter Last zuerst gedrosselt; exakte Werte sind versionsabhängig → in K aus Agave-Source pinnen, nicht aus Sekundärquellen. Für Race-Primary bräuchte es eigenen Stake, einen Stake-Partner oder einen Staked-Connection-Provider (paid). First-Euro: `RpcSubmitter` + Jito JSON-RPC reicht; Jito-Bundle-Semantik (atomar, Tip nur bei Landing) in P5 real prüfen. Redundante Submission: nicht in Stufe 1; gleiche Signatur landet nur einmal, aber RPC-Revert zahlt Fees → erst P7.

**C Prebuild.** Sicher vorab: Account-Listen, AccountMeta-Reihenfolge, Instruction-Data-Skelette ohne Beträge, LUT-Plan (v0), Resource-Profil (v1), Serialisierungs-Buffer. Spät zu patchen: Blockhash, seize/repay-Beträge, Tip/Fee, Preis-Update-Ix (falls Pull-Oracle), v1 `TransactionConfig`, danach Signatur. Invalidation-Events, die fehlen könnten: Tag-Löschung, Timer-Reset (25 %-Regel), PREMIUM_ACTIVE-Flag-Wechsel, LUT-Deaktivierung, FeeState-Änderung. → in P2-Ticket aufnehmen.

**D Economics/Scope.** Versteckte Paid-Pflicht: keine, solange S1-08 keine Binary-Quelle erzwingt; Jupiter/Titan-API-Keys sind in P0 als Prüfpunkt erfasst. Theoretische Optimierungen: alle bereits in K. Ein Core: ja. Race + Long-Tail gleichzeitig: ja, über LIVE-Freigabe getrennt.

## Für die Implementierung offen (keine Blocker)

- P0-02 Lizenz superagg selbst öffnen.
- P0-03 Oracle-Modell aus eva01 lesen (Pull-Update in Receivership-TX ja/nein) — entscheidet, ob `PriceSource` in S1 gebaut wird.
- S1-05 WSS-Credit-Verbrauch der `programSubscribe` gegen 1M/Monat messen; ggf. auf Near-Threshold-Accounts filtern.
