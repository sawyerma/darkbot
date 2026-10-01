"""Gebuehrenevidenz einer Capture/PAPER-Datenbank pruefen - READ-ONLY.

Vier Stufen:
  1. Rohbestand der Fee-Snapshots je Komponente/Status, Fehlertexte
  2. Zeitleiste: fuer jede Sekunde, ob eine vollstaendige frische Basis fuer BUY und SELL existierte (select_cost_basis, is_fresh, fee_terms)
  3. Abstaende der Abrufe vs. Maximalalter (max_age_ns stammt aus der beim Source-Run gespeicherten FRESHNESS_POLICY)
  4. Echter RAM-Replay der angegebenen Strategie-Config durch run_replay (StateOwner -> Strategy -> QuoteEngine -> fee_terms):
     Verdikt ohne nachgebauten Entry-Pfad. Slots hoch, damit SLOT_LIMIT nicht mit UNQUALIFIED_COSTS vermischt wird.
     Exit-Codes: 0 PASS (Fills, 0x UNQUALIFIED_COSTS) | 2 keine Snapshots | 3 PARTIAL (Fills + UNQUALIFIED) | 4 FAIL | 5 OFFEN | 6 KEIN FILL

Aufruf (im DarkBot-Ordner):
  python3 tools/solana_pump/fee_evidence_check.py --db data/solana_pump/market_capture_30m_20260920.sqlite --run-id <run> --config config/solana_pump/strategy_c_pre_pump_01.json
"""
import argparse
import json
import sqlite3
from collections import Counter, defaultdict
from itertools import groupby
from pathlib import Path

from darkbot.solana_pump.config import load_runtime_config
from darkbot.solana_pump.domain import NS_PER_MS, CostBasis, InvalidQuote, Side
from darkbot.solana_pump.execution import ExecutionPolicy, fee_terms
from darkbot.solana_pump.fees import select_cost_basis
from darkbot.solana_pump.replay import load_replay_input, run_replay
from darkbot.solana_pump.storage import open_connection

ap = argparse.ArgumentParser()
ap.add_argument("--db", type=Path, required=True)
ap.add_argument("--run-id", default=None)
ap.add_argument("--config", type=Path, required=True, help="Strategie-/Execution-Config fuer Stufe 4; Fee-Freshness in Stufe 2/3 stammt aus dem Source-Run")
ap.add_argument("--slots", type=int, default=2000, help="Slots fuer den Diagnose-Replay (hoch, damit SLOT_LIMIT nicht als Fee-Problem erscheint)")
ap.add_argument("--step-ms", type=int, default=1000, help="Raster der Fee-Zeitleiste")
args = ap.parse_args()

conn = open_connection(args.db, read_only=True)
conn.row_factory = sqlite3.Row
run_id = args.run_id or conn.execute("SELECT run_id FROM runs ORDER BY started_utc DESC LIMIT 1").fetchone()["run_id"]
run = conn.execute("SELECT * FROM runs WHERE run_id=?", (run_id,)).fetchone()
if run is None:
    raise SystemExit(f"run {run_id} nicht gefunden")
cols = run.keys()
start = run["started_mono_ns"] if "started_mono_ns" in cols and run["started_mono_ns"] else None
end = run["ended_mono_ns"] if "ended_mono_ns" in cols and run["ended_mono_ns"] else None
print(f"run: {run_id}\nmode={run['mode']} status={run['status']} quality={run['quality_status']} readiness={run['readiness'] if 'readiness' in cols else '?'} "
      f"dauer={(end - start) / 1e9 if start and end else '?'} s")
print("  -> readiness ECONOMIC_PAPER_READY = Gebuehrenbasis war am Ende vollstaendig; CAPTURE_READY = nur Aufnahme, keine wirtschaftlichen Entries moeglich\n")

# ---------------------------------------------------------------- Rohbestand
rows = conn.execute("SELECT source_type, validation_status, received_mono_ns, raw_values, evidence FROM fee_snapshots WHERE run_id=? ORDER BY received_mono_ns", (run_id,)).fetchall()
print(f"fee_snapshots: {len(rows)} Zeilen")
if not rows:
    print("  KEINE Fee-Snapshots. Der Fee-Task hat nie etwas geliefert: entweder kein HTTP-RPC (HELIUS_API_KEY / EXISTING_RPC_HTTP_URL fehlten in der Shell)\n"
          "  oder der Task ist vor dem ersten Abruf gestorben. Pruefe: python3 -m darkbot doctor --config <profil>  -> D02.missing_env und D05.")
    raise SystemExit(2)
by = Counter((r["source_type"], r["validation_status"], r["evidence"]) for r in rows)
for (st, vs, evidence), n in sorted(by.items()):
    print(f"  {st:24s} {vs:22s} evidence={str(evidence):14s} {n:5d}")
errors = Counter()
for r in rows:
    if r["validation_status"] != "VALID" or r["evidence"] == "UNKNOWN":
        try:
            rv = json.loads(r["raw_values"]) if r["raw_values"] else {}
        except ValueError:
            rv = {}
        msg = rv.get("error") or rv.get("acquisition_error") or rv.get("reason") or rv.get("derivation") or str({k: v for k, v in rv.items() if "error" in str(k).lower()})[:120]
        errors[(r["source_type"], r["validation_status"], str(r["evidence"]), str(msg)[:140])] += 1
if errors:
    print("\nFehlertexte ungueltiger / unbekannter Snapshots:")
    for (st, vs, evidence, msg), n in errors.most_common(12):
        print(f"  {n:4d}x {st} {vs} evidence={evidence}: {msg}")

# ---------------------------------------------------------------- Zeitleiste mit der Engine-Regel
cfg = load_runtime_config(args.config)
frozen = load_replay_input(conn, run_id, cfg)
fees = sorted(frozen.fee_snapshots, key=lambda s: s.received_mono_ns)
policy = ExecutionPolicy.from_config(cfg)
t0 = start or (fees[0].received_mono_ns if fees else 0)
t1 = end or (frozen.end_mono_ns if getattr(frozen, "end_mono_ns", None) else (fees[-1].received_mono_ns if fees else t0))
basis = CostBasis(None, None, None, None)
batches = [(t, list(g)) for t, g in groupby(fees, key=lambda s: s.received_mono_ns)]
bi = 0
step = NS_PER_MS * args.step_ms
ok_s = 0
reasons = Counter()
first_ok = None
gaps = []
gap_start = None
t = t0
while t <= t1:
    while bi < len(batches) and batches[bi][0] <= t:
        basis = select_cost_basis(basis, batches[bi][1]); bi += 1
    invalid = []
    for side in (Side.BUY, Side.SELL):
        terms = fee_terms(basis, policy, t, None, side)
        if isinstance(terms, InvalidQuote):
            invalid.append(f"{side.value}={terms.reason}: {terms.detail}")
    if invalid:
        reasons[" | ".join(invalid)] += 1
        if gap_start is None:
            gap_start = t
    else:
        ok_s += 1
        first_ok = first_ok or t
        if gap_start is not None:
            gaps.append((gap_start, t)); gap_start = None
    t += step
if gap_start is not None:
    gaps.append((gap_start, t1))
total_s = max(1, int((t1 - t0) / step) + 1)
print(f"\nZeitleiste ({args.step_ms}-ms-Raster, BUY+SELL fee_terms der Engine, max_age aus Source-FRESHNESS_POLICY): vollstaendige frische Basis in {ok_s} von {total_s} Punkten ({100 * ok_s / total_s:.1f}%)")
print(f"  erste vollstaendige Basis nach {((first_ok - t0) / 1e9) if first_ok else '-'} s")
if reasons:
    print("  Gruende ohne Basis:")
    for k, n in reasons.most_common(8):
        duration_s = n * args.step_ms / 1000.0
        print(f"    {duration_s:8.3f} s  {k}")
if gaps:
    longest = sorted(gaps, key=lambda g: g[1] - g[0], reverse=True)[:5]
    print("  laengste Luecken:", ", ".join(f"{(a - t0) / 1e9:.0f}s..{(b - t0) / 1e9:.0f}s" for a, b in longest))
per_kind = defaultdict(list)
for s in fees:
    per_kind[s.source_type].append(s)
print("\nAbstaende zwischen Snapshots je Komponente (max_age laut Source-FRESHNESS_POLICY):")
for kind, lst in per_kind.items():
    valid = [s for s in lst if s.validation_status == "VALID" and s.evidence.value != "UNKNOWN"]
    deltas = [(b.received_mono_ns - a.received_mono_ns) / 1e9 for a, b in zip(valid, valid[1:])]
    print(f"  {kind:24s} valid={len(valid):4d}/{len(lst):4d}  max_age={lst[0].max_age_ns / 1e9:.0f}s  Abstand VALID->VALID: median={sorted(deltas)[len(deltas) // 2] if deltas else '-'}  max={max(deltas) if deltas else '-'}")
print("\nLesart: Abstand > max_age  => zwischen zwei Abrufen ist die Komponente 'stale' (UNQUALIFIED_COSTS trotz VALID-Zeilen).\n"
      "        nur RPC_ERROR/UNAVAILABLE_NO_RPC => Endpoint/Key-Problem, kein Code-Problem in der Engine.\n"
      "        PRIORITY_UNKNOWN => getRecentPrioritizationFees liefert nichts; Netzwerkgebuehr haengt daran.")

# ---------------------------------------------------------------- 4. echter Replay durch den DarkBot-Entry-Pfad
print(f"\n4. Echter RAM-Replay mit {args.config} ({args.slots} Slots, sonst unveraendert) ...", flush=True)
diag_cfg = cfg.with_overrides(portfolio=cfg.portfolio_limit_overrides(args.slots), storage={"queue_capacity_records": max(5_000_000, cfg.storage["queue_capacity_records"])})
result = run_replay(None, frozen, diag_cfg, f"fee_diag_{run_id}", "fee_evidence_check", mode="REPLAY", persist=False, persist_observations=False)
o = result.owner
if o.recorder.overflowed or o.disk_error or o.counters.internal_errors:
    raise RuntimeError(f"Diagnose-Replay unbrauchbar: overflow={o.recorder.overflowed} disk_error={o.disk_error} internal_errors={o.counters.internal_errors} {o.last_error}")
c = o.counters
print(f"   creates={c.creates} actionable={c.creates_actionable} entry_intents={c.entry_intents} entry_fills={c.entry_fills} entry_failed={c.entry_failed} unresolved={c.unresolved} censored={c.censored}")
skips = dict(sorted(c.ineligible.items(), key=lambda kv: (-kv[1], kv[0])))
print("   Skip-Gruende:", skips or "keine")
unq = skips.get("UNQUALIFIED_COSTS", 0)
print("\n5. VERDIKT")
conn.close()
if c.entry_fills > 0 and unq == 0:
    print(f"   PASS: der echte Entry-/Fee-/Quote-Pfad hat {c.entry_fills} Fills modelliert und 0x UNQUALIFIED_COSTS erzeugt.")
    raise SystemExit(0)
if c.entry_fills > 0:
    print(f"   PARTIAL: {c.entry_fills} Fills, aber {unq}x UNQUALIFIED_COSTS. Fuer eine saubere Edge-Auswertung ist diese Capture nicht vollstaendig wirtschaftlich; Stufe 1-3 zeigt die Luecken.")
    raise SystemExit(3)
if unq > 0:
    print(f"   FAIL: {unq}x UNQUALIFIED_COSTS und kein Fill. Ursache steht in Stufe 1-3 (Komponente, Fehlertext, Luecke).")
    raise SystemExit(4)
if c.entry_intents == 0:
    print("   OFFEN: die Strategie erzeugte keinen Entry-Intent (Filter/Gate); der wirtschaftliche Fill-Pfad wurde nicht ausgefuehrt.")
    raise SystemExit(5)
print("   KEIN FILL: Intents vorhanden, aber kein Fill; Skip-/Fehlergruende oben pruefen.")
raise SystemExit(6)
