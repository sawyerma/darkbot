# Prestudy — BSC (historisch, nicht im Produkt)

Historische BSC-Harnesses, Prototypen, Referenzquellen und Nachweise, ausgelagert aus dem produktiven Projekt `bsc/` während der Strukturreform R1–R8 (siehe `bsc/README.md`).

## Grenzen

- **Kein Build-Member**, kein Laufzeitimport, kein Standard-Suchpfad des Bots. Der einzige BSC-Cargo-Workspace ist `bsc/native/`.
- Inhalte sind **unverändert** (Hash-Abgleich gegen den Git-Index; 1.294 + weitere Dateien bytegleich). Relative Verweise innerhalb dieses Baums können ins Leere zeigen, wenn ihr Ziel in das aktive Produkt übernommen wurde (z. B. `experiments/gate0/src/lib.rs` deklariert die nach `bsc/native` übernommenen Module `acceptance_guard` und `qualification_capture`). Der Baum wird nicht gebaut.
- Herkunft: Commit `7f3c08b` (HEAD vor R1). Die vollständige Abbildung Originalpfad → Ort steht in [`bsc/docs/EVIDENCE.md`](../../bsc/docs/EVIDENCE.md).

## Inventar

| Ort | Herkunft | Inhalt |
|---|---|---|
| `experiments/gate0/` | `bsc/gates/gate0/` | früherer zweiter Cargo-Workspace `darkbot-gate0`: Restcode (`intent_lane`, `restart_fence`, `native_bnb_quote` (Semantik heute in `core`), `p2p_ingress` (Reth-P2P-Sensor, keine Produktionspflicht), `builder_lifecycle_tests`), Binaries G03/G05/G06/G06a-Probes/G07-Testnet, Alt-Tools, `evidence/` (Gate-0-Nachweise), `Cargo.toml`, `Cargo.lock` (Referenz der TLS-Pins). Lokal ggf. `target/` (ignoriert, ~9 GB, löschbar) |
| `experiments/README.md` | `bsc/gates/README.md` | frühere Gates-Übersicht |
| `experiments/native-tools/` | `bsc/native/tools/lock_tls.sh`, `verify_tls_lock.py` | einmalige Unit-5-TLS-Lock-Herstellung gegen den Gate-0-Lock |
| `references/production-plan-reference/` | `bsc/docs/production-plan-reference/` | Unit-1-Review-ZIP, isolierte Quellreferenzen, Source-Audit |
| `references/root-gates/bsc/gate0/src/p2p_ingress.rs` | `gates/bsc/gate0/src/p2p_ingress.rs` (Repo-Root) | abweichende ältere Variante des P2P-Sensors (nicht identisch mit der Gate-0-Kopie) |
| `references/bsc-src/darkbot/` | `bsc/src/darkbot/` | ungenutzte Python-Paketidentität (Phase-1 PAPER) |
| `evidence/tests/` | `bsc/tests/` | 1.062 historische Gate-0-Evidence-/Testdateien (`legacy_gate0/`), alter Python-Regressionstest |
| `evidence/docs/` | `bsc/docs/phase_a/`, `reviews/`, `BSC_PHASE_B_OPTIONAL_CLEANUP_DRAFT.md` | abgeschlossene Phase-A-Artefakte, C0-Unit-1-Review, archivierter Phase-B-Draft |
| `evidence/native/` | `bsc/native/SHA256SUMS*.txt` | eingefrorene Hash-Manifeste der Unit-1/Unit-5-Reviews (Basis: damaliges `bsc/native/`) |
