"""A vs Strategy C auf der eingefrorenen 30-Minuten-Capture - REINE AUSWERTUNG (v2, Review-Korrekturen eingearbeitet).

- Quelle read-only, KEINE Datenbank wird geschrieben; jeder Replay laeuft im RAM (persist=False), Ergebnis = StateOwner.
- Strategy-C-Features werden beim Aufzeichnen abgefangen (StateOwner.record_observation), nicht aus einer Tabelle gelesen.
- Drei Ebenen: C0/C1 = unveraendertes PRE_PUMP_01 (Legacy-Cancel bzw. TX_LANDS, 1 und 10 Slots, native 800/800 ms),
  C2 = Suche (kein Filter, 0/250/500/750/1000 ms, hohe Slotzahl, Filter nachtraeglich auf der Feature-/Outcome-Matrix),
  A = native (eigene Latenz/Quote-Age) und normalisiert (Latenz/Quote-Age wie C2).
- Oekonomik identisch fuer alle Laeufe: CENSORED ausgeschlossen (bezahlte Gebuehren bleiben Verlust), UNRESOLVED =
  Totalverlust inkl. bereits bezahlter Fehlversuche, ENTRY_FAILED = Netzwerkgebuehr, ENTRY_CANCELLED = kostenfrei (separat).
- Ausgabe: nur Dateien in OUT_DIR (c_filter_results.csv, matrix_<run>.csv, summary.md).

Alle Zahlen sind IN-SAMPLE auf einer Capture. `screen_eligible` (>= MIN_RESOLVED) ist ein Explorationsfilter, KEINE
Qualifikation im Sinne des Research-Vertrags (pilot_min_unique_creates / independent blocks / bootstrap).
Aufruf (im DarkBot-Ordner):
  python3 ac_c_30m_test_ram.py                       # --mode search: nur C2-Suche (Standard)
  python3 ac_c_30m_test_ram.py --mode control        # C0/C1: unveraendertes PRE_PUMP_01, Legacy vs TX_LANDS, 1 und 10 Slots
  python3 ac_c_30m_test_ram.py --mode a              # A native + A normalisiert
  python3 ac_c_30m_test_ram.py --mode all
  Optionen: --db --run-id --out --latency-ms --windows 0,250,500,750,1000
"""
import argparse
import csv
import itertools
import json
import statistics
import sys
from decimal import Decimal
from pathlib import Path

from darkbot.config import load_runtime_config
from darkbot.paper import StateOwner
from darkbot.replay import load_replay_input, run_replay
from darkbot.storage import open_connection
from darkbot.strategies.strategy_c_legacy_backend import FILTERABLE_FEATURES, evaluate_filters, parse_filters

_ap = argparse.ArgumentParser(description="A vs Strategy C - RAM-Auswertung einer eingefrorenen Capture (keine DB-Schreibzugriffe)")
_ap.add_argument("--mode", choices=("search", "control", "a", "all"), default="search")
_ap.add_argument("--db", type=Path, default=Path("data/market_capture_30m_20260920.sqlite"))
_ap.add_argument("--run-id", default="market_capture_30m_20260920_capture_20260920T130659Z_18460a")
_ap.add_argument("--out", type=Path, default=None, help="Standard: exports/ac_c_30m_<mode>_L<latenz>")
_ap.add_argument("--latency-ms", type=int, default=100, help="Latenz fuer C2-Suche und A_NORMALIZED (Kontrollen bleiben bei ihrer nativen 800/800)")
_ap.add_argument("--windows", default="0,250,500,750,1000")
ARGS = _ap.parse_args()
SOURCE_DB, SOURCE_RUN, LATENCY_MS, MODE = ARGS.db, ARGS.run_id, ARGS.latency_ms, ARGS.mode
WINDOWS = [int(w) for w in ARGS.windows.split(",")]
OUT_DIR = ARGS.out or Path(f"exports/ac_c_30m_{MODE}_L{LATENCY_MS}")
QUOTE_MAX_AGE_MS = 600000
SLOTS = 2000
RAM_QUEUE = {"queue_capacity_records": 5_000_000}  # persist=False: nichts leert den Recorder; Ueberlauf wuerde still neue Entries sperren
C_BASE_PATH = Path("config/strategy_c_pre_pump_01.json")           # 0.09837 SOL Swap-Budget, 2x, Stop 0, 300 s
A_PATH = Path("config/strategy_a_creator_exit_usd_slots.json")       # aktuelle A: $10/Slot, Target aus, Creator-Exit, 590 s
PRE_PUMP_01 = [{"feature": "buy_count", "min": "3", "max": None}, {"feature": "unique_buyers", "min": "3", "max": None},
               {"feature": "sell_count", "min": "0", "max": "0"}, {"feature": "dev_initial_buy", "min": "0", "max": "0.20"}]
COUNT_GRID = {"buy_count": (1, 2, 3, 4, 5, 6, 8), "unique_buyers": (1, 2, 3, 4, 5, 6), "sell_count": (0, 1, 2)}
DEV_GRID = ("0.01", "0.02", "0.05", "0.10", "0.20")
QUANTILES = (0.10, 0.25, 0.50, 0.75, 0.90)
MIN_RESOLVED = 15
MAX_UNRESOLVED_SHARE = 0.10
TOP_SINGLES = 16

# ---------------------------------------------------------------- Features im RAM abfangen (kein Tabellenzugriff)
CAPTURED: dict[str, dict[str, dict]] = {}
_orig_record_observation = StateOwner.record_observation


def _capture_observation(self, observation, qualified, actual, status=None, extra=None):
    CAPTURED.setdefault(self.run_id, {})[observation.mint] = {
        "status": status or ("OBSERVING" if qualified is None else "DECIDED"), "qualified": qualified, "observation_start": observation.start,
        "features": (extra or {}).get("features_json") or {}, "decision_action": (extra or {}).get("decision_action")}
    return _orig_record_observation(self, observation, qualified, actual, status, extra)


StateOwner.record_observation = _capture_observation


# ---------------------------------------------------------------- Matrix aus dem RAM
def outcome_of(pos, nominal_cost):
    sunk = int(pos.failed_attempt_cost_lamports or 0)  # bereits bezahlte Netzwerkgebuehren fehlgeschlagener Versuche
    exposure = pos.entry_economic_cost_lamports or pos.entry_cash_debit_lamports or pos.reserved_cash_lamports or nominal_cost
    row = {"state": pos.state.value, "exit_reason": pos.exit_reason.value if pos.exit_reason else None, "research_status": pos.research_status,
           "unresolved_reason": pos.unresolved_reason, "entry_cost": pos.entry_economic_cost_lamports, "pnl": pos.net_pnl_lamports,
           "sunk_fees": sunk, "cost_at_risk": int(exposure or 0) + sunk}
    if pos.state.value == "CLOSED" and pos.net_pnl_lamports is not None:
        row["outcome"] = "RESOLVED"                       # net_pnl enthaelt die Fehlversuche bereits
    elif pos.state.value == "ENTRY_FAILED":
        row["outcome"] = "ENTRY_CANCELLED" if pos.research_status == "ENTRY_CANCELLED" else "ENTRY_FAILED"  # kostenfrei vs. Gebuehr bezahlt
    elif pos.state.value == "UNRESOLVED" or pos.research_status in ("UNRESOLVED", "UNKNOWN"):
        row["outcome"] = "UNRESOLVED"
    else:
        row["outcome"] = "CENSORED"  # am Capture-Ende offen: kein ehrlicher Wert; nur die bezahlten Gebuehren (sunk_fees) sind sicher
    return row


def matrix_from_ram(owner, nominal_cost):
    positions = {p.mint: p for p in owner.positions.values()}
    rows = []
    for mint, obs in sorted(CAPTURED.get(owner.run_id, {}).items(), key=lambda kv: (kv[1]["observation_start"], kv[0])):
        feat = obs["features"]
        row = {"mint": mint, "observation_start": obs["observation_start"], "decision_action": obs["decision_action"], "provider_gap_flag": feat.get("provider_gap_flag"),
               **{name: feat.get(name) for name in FILTERABLE_FEATURES}}
        if obs["status"] not in ("DECIDED", "DATA_INCOMPLETE"):
            row["outcome"] = "OBSERVATION_CENSORED"  # Fenster am Capture-Ende noch offen (CENSORED_RUN_END): nie entschieden
        elif obs["status"] == "DATA_INCOMPLETE":
            row["outcome"] = "DATA_INCOMPLETE"
        elif mint not in positions:
            row["outcome"] = "NO_ENTRY"
        else:
            row.update(outcome_of(positions[mint], nominal_cost))
        rows.append(row)
    return rows


# ---------------------------------------------------------------- Filter-Screen
EXCLUDED = ("DATA_INCOMPLETE", "OBSERVATION_CENSORED")  # weder gehandelt noch fuer Filtergrenzen/Ranking benutzt


def round_sig(value, digits=4):
    value = Decimal(value)
    if value == 0:
        return Decimal(0)
    return value.quantize(Decimal(1).scaleb(value.adjusted() - (digits - 1))).normalize()


def thresholds(matrix):
    out = {}
    for feature in FILTERABLE_FEATURES:
        if feature in COUNT_GRID:
            out[feature] = [Decimal(v) for v in COUNT_GRID[feature]]
        elif feature == "dev_initial_buy":
            out[feature] = [Decimal(v) for v in DEV_GRID]
        else:
            vals = sorted(Decimal(str(r[feature])) for r in matrix if r["outcome"] not in EXCLUDED and r.get(feature) is not None)
            out[feature] = sorted({round_sig(vals[min(len(vals) - 1, int(q * len(vals)))]) for q in QUANTILES}) if len(vals) >= 10 else []
    return out


def passes(rules, row):
    f = {name: row.get(name) for name in FILTERABLE_FEATURES}
    f["provider_gap_flag"] = bool(row.get("provider_gap_flag"))
    return evaluate_filters(rules, f)[0] == "PASS"


def evaluate_filter_set(raw_rules, matrix):
    rules = parse_filters(raw_rules)
    passing = [r for r in matrix if r["outcome"] not in EXCLUDED and passes(rules, r)]
    return economics(passing) | {"filters": json.dumps(raw_rules, separators=(",", ":")), "pass_signature": ",".join(sorted(r["mint"] for r in passing))}


def economics(rows):
    """Eine Definition fuer Filtersaetze UND ganze Laeufe (A, C0, C1)."""
    resolved = [r for r in rows if r["outcome"] == "RESOLVED"]
    unresolved = [r for r in rows if r["outcome"] == "UNRESOLVED"]
    failed = [r for r in rows if r["outcome"] == "ENTRY_FAILED"]
    censored = [r for r in rows if r["outcome"] == "CENSORED"]
    pnl = [int(r["pnl"]) for r in resolved]
    failed_cost = sum(int(r.get("pnl") or 0) for r in failed)                       # Netzwerkgebuehr des Fehlschlags
    censored_sunk = -sum(int(r.get("sunk_fees") or 0) for r in censored)            # bezahlte Fehlversuche offener Positionen
    total = sum(pnl) + failed_cost + censored_sunk
    pessimistic = total - sum(int(r.get("cost_at_risk") or 0) for r in unresolved)  # UNRESOLVED = Totalverlust inkl. Fehlversuche
    n = len(resolved)
    n_econ = n + len(unresolved) + len(failed)                                      # jeder Versuch mit Geld im Spiel
    resolved_capital = sum(int(r["entry_cost"]) for r in resolved if r.get("entry_cost") is not None)
    exposed_capital = resolved_capital + sum(int(r.get("cost_at_risk") or 0) for r in unresolved)
    share = len(unresolved) / (n + len(unresolved)) if n + len(unresolved) else None
    wins, losses = [v for v in pnl if v > 0], [-v for v in pnl if v < 0]
    exits = {}
    for r in resolved:
        exits[r.get("exit_reason") or "?"] = exits.get(r.get("exit_reason") or "?", 0) + 1
    return {"n_pass": len(rows), "n_resolved": n, "n_unresolved": len(unresolved), "n_failed": len(failed), "n_cancelled": sum(r["outcome"] == "ENTRY_CANCELLED" for r in rows),
            "n_censored": len(censored), "n_no_entry": sum(r["outcome"] == "NO_ENTRY" for r in rows), "n_economic_attempts": n_econ,
            "unresolved_share": round(share, 4) if share is not None else None,
            "pnl_lamports": total, "pnl_pessimistic_lamports": pessimistic, "entry_failed_cost_lamports": failed_cost, "censored_sunk_fees_lamports": censored_sunk,
            "ev_per_resolved_lamports": sum(pnl) / n if n else None, "pessimistic_ev_per_resolved_lamports": pessimistic / n if n else None,
            "pessimistic_ev_per_economic_attempt_lamports": pessimistic / n_econ if n_econ else None,
            "resolved_capital_lamports": resolved_capital, "exposed_capital_lamports": exposed_capital,
            "ev_per_resolved_capital_pct": 100 * total / resolved_capital if resolved_capital else None,
            "pessimistic_per_exposed_capital_pct": 100 * pessimistic / exposed_capital if exposed_capital else None,
            "winrate": len(wins) / n if n else None, "profit_factor": sum(wins) / sum(losses) if losses else None,
            "median_pnl_lamports": statistics.median(pnl) if pnl else None, "exits": json.dumps(exits, sort_keys=True),
            "screen_eligible": n >= MIN_RESOLVED and (share or 0) <= MAX_UNRESOLVED_SHARE}


def build_filter_sets(matrix):
    th = thresholds(matrix)
    results = []

    def add(raw, kind):
        r = evaluate_filter_set(raw, matrix); r["kind"] = kind; results.append(r); return r

    add([], "NO_FILTER")
    add(PRE_PUMP_01, "CONTROL_PRE_PUMP_01")
    singles = []
    for feature, values in th.items():
        for value in values:
            for bound in ("min", "max"):
                if feature == "sell_count" and bound == "min":
                    continue
                rule = {"feature": feature, "min": str(value) if bound == "min" else None, "max": str(value) if bound == "max" else None}
                r = add([rule], "SINGLE")
                if r["screen_eligible"] and r["pessimistic_ev_per_resolved_lamports"] is not None:
                    singles.append((r["pessimistic_ev_per_resolved_lamports"], [rule]))
    singles.sort(key=lambda x: x[0], reverse=True)
    best = [rules for _, rules in singles[:TOP_SINGLES]]
    for k in (2, 3):
        for combo in itertools.combinations(best, k):
            raw = [r for group in combo for r in group]
            if len({r["feature"] for r in raw}) == len(raw):
                add(raw, f"COMBO{k}")
    return results


def run_stats(owner, nominal_cost=0):
    rows = [outcome_of(p, nominal_cost) for p in owner.positions.values()]
    return economics(rows) | {"fills": owner.counters.entry_fills, "skips": dict(owner.counters.ineligible)}


def replay(frozen, cfg, run_id, *, independent):
    """RAM-Replay mit Invarianten: kein Recorder-Ueberlauf, kein Disk-/Engine-Fehler; bei Suchlaeufen keine Slot-/Kapitalkonkurrenz."""
    print(f"[{run_id}] Replay (RAM) ...", flush=True)
    result = run_replay(None, frozen, cfg, run_id, "ac_c_30m_test", mode="REPLAY", persist=False, persist_observations=False, replay_callback_lateness=False)
    o = result.owner
    if o.counters.internal_errors or o.disk_error or o.recorder.overflowed:
        raise RuntimeError(f"{run_id}: internal_errors={o.counters.internal_errors} disk_error={o.disk_error} recorder_overflow={o.recorder.overflowed}: {o.last_error}")
    if independent:
        for reason in ("SLOT_LIMIT", "INSUFFICIENT_CAPITAL"):
            if o.counters.ineligible.get(reason):
                raise RuntimeError(f"{run_id}: {reason}={o.counters.ineligible[reason]} - nachtraegliche Filter waeren nicht mehr unabhaengige Laeufe")
    return result


def sol(v):
    return f"{(v or 0) / 1e9:+.6f}"


# ---------------------------------------------------------------- Laeufe
src = open_connection(SOURCE_DB, read_only=True)
all_screen, baseline_rows, matrices, controls = [], [], {}, []
try:
    c_base = load_runtime_config(C_BASE_PATH)
    nominal = int(c_base.strategy["entry_amount_lamports"])
    frozen_c = load_replay_input(src, SOURCE_RUN, c_base)
    print("dataset:", frozen_c.dataset_hash, flush=True)
    if MODE in ("control", "all"):
        print("\n=== C0/C1: PRE_PUMP_01 unveraendert (500 ms, 800/800 ms, 600000 ms), Legacy-Cancel vs. TX_LANDS, 1 und 10 Slots ===", flush=True)
        for mode in ("LEGACY_FREE_CANCEL", "TX_LANDS"):
            for slots in (1, 10):
                cfg = c_base.with_overrides(strategy=({} if mode == "LEGACY_FREE_CANCEL" else {"creator_sold_before_fill": mode}),
                                            portfolio=c_base.portfolio_limit_overrides(slots), storage=RAM_QUEUE)
                run_id = f"{'C0' if mode == 'LEGACY_FREE_CANCEL' else 'C1'}_PREPUMP_{mode}_S{slots}"
                st = run_stats(replay(frozen_c, cfg, run_id, independent=False).owner, nominal)
                controls.append({"name": run_id, **st})
                print(f"    fills={st['fills']} resolved={st['n_resolved']} failed={st['n_failed']} cancelled={st['n_cancelled']} unresolved={st['n_unresolved']} "
                      f"censored={st['n_censored']} skips={st['skips']} pnl={sol(st['pnl_lamports'])} pess={sol(st['pnl_pessimistic_lamports'])} SOL", flush=True)
    for i, window in (list(enumerate(WINDOWS, 1)) if MODE in ("search", "all") else []):
        if i == 1:
            print("\n=== C2: SUCHE - kein Filter, TX_LANDS, keine Slot-Konkurrenz ===", flush=True)
        cfg = c_base.with_overrides(
            strategy={"candidate_name": f"C_SCREEN_W{window}", "observation_ms": window, "filters": [], "target_net_multiple": "2.0", "target_exit_enabled": True,
                      "stop_net_multiple": "0", "ordinary_stop_enabled": False, "strategy_timeout_ms": 300000, "creator_sold_before_fill": "TX_LANDS"},
            latency_model={"entry_delay_ms": LATENCY_MS, "exit_delay_ms": LATENCY_MS}, execution={"quote_max_age_ms": QUOTE_MAX_AGE_MS},
            portfolio=c_base.portfolio_limit_overrides(SLOTS), storage=RAM_QUEUE)
        run_id = f"C2_W{window}_L{LATENCY_MS}"
        result = replay(frozen_c, cfg, run_id, independent=True)
        matrix = matrix_from_ram(result.owner, nominal)
        matrices[run_id] = matrix
        results = build_filter_sets(matrix)
        for row in results:
            row["run_id"], row["window_ms"] = run_id, window
            all_screen.append(row)
        no_filter = next(r for r in results if r["kind"] == "NO_FILTER")
        baseline_rows.append({"name": f"C2_NO_FILTER_{window}ms", **no_filter})
        if window == 500:
            baseline_rows.append({"name": "C2_PRE_PUMP_01_filter_500ms", **next(r for r in results if r["kind"] == "CONTROL_PRE_PUMP_01")})
        outcomes = {}
        for row in matrix:
            outcomes[row["outcome"]] = outcomes.get(row["outcome"], 0) + 1
        print(f"    launches={len(matrix)} outcomes={json.dumps(outcomes, sort_keys=True)} skips={dict(result.owner.counters.ineligible)} "
              f"pnl={sol(no_filter['pnl_lamports'])} pess={sol(no_filter['pnl_pessimistic_lamports'])} SOL", flush=True)
        del result
    a_runs = {}
    a_base = load_runtime_config(A_PATH)
    if MODE in ("a", "all"):
        print("\n=== A: native (eigene Latenz 0/0, Quote-Age 250) und normalisiert (wie C2) ===", flush=True)
    for name, overrides in (("A_NATIVE", {}), ("A_NORMALIZED", {"latency_model": {"entry_delay_ms": LATENCY_MS, "exit_delay_ms": LATENCY_MS}, "execution": {"quote_max_age_ms": QUOTE_MAX_AGE_MS}})) if MODE in ("a", "all") else []:
        a_cfg = a_base.with_overrides(portfolio=a_base.portfolio_limit_overrides(SLOTS), storage=RAM_QUEUE, **overrides)
        frozen_a = load_replay_input(src, SOURCE_RUN, a_cfg)
        st = run_stats(replay(frozen_a, a_cfg, name, independent=True).owner)
        a_runs[name] = st
        print(f"    {name}: fills={st['fills']} resolved={st['n_resolved']} failed={st['n_failed']} cancelled={st['n_cancelled']} unresolved={st['n_unresolved']} "
              f"censored={st['n_censored']} skips={st['skips']} pnl={sol(st['pnl_lamports'])} pess={sol(st['pnl_pessimistic_lamports'])} SOL", flush=True)
finally:
    src.close()

# ---------------------------------------------------------------- Ausgabe (nur Dateien in OUT_DIR)
OUT_DIR.mkdir(parents=True, exist_ok=True)
if all_screen:
    with (OUT_DIR / "c_filter_results.csv").open("w", newline="", encoding="utf-8") as fh:
        cols = [k for k in all_screen[0] if k != "pass_signature"]
        w = csv.DictWriter(fh, fieldnames=cols, extrasaction="ignore"); w.writeheader(); w.writerows(all_screen)
for run_id, matrix in matrices.items():
    with (OUT_DIR / f"matrix_{run_id}.csv").open("w", newline="", encoding="utf-8") as fh:
        cols = ["mint", "observation_start", "decision_action", *FILTERABLE_FEATURES, "provider_gap_flag", "outcome", "state", "exit_reason", "research_status",
                "unresolved_reason", "entry_cost", "cost_at_risk", "sunk_fees", "pnl"]
        w = csv.DictWriter(fh, fieldnames=cols, extrasaction="ignore"); w.writeheader(); w.writerows(matrix)

def row_line(name, r):
    return (f"| {name} | {r['n_pass']} | {r['n_resolved']} | {r['n_failed']} | {r['n_cancelled']} | {r['n_unresolved']} | {r['n_censored']} | {r['n_no_entry']} | "
            f"{sol(r['pnl_lamports'])} | {sol(r['pnl_pessimistic_lamports'])} | {sol(r['ev_per_resolved_lamports'])} | {sol(r['pessimistic_ev_per_economic_attempt_lamports'])} | "
            f"{(r['pessimistic_per_exposed_capital_pct'] or 0):+.3f} | {(r['winrate'] or 0):.3f} | {r['exits']} |")


HEAD = ["| run / model | pass | resolved | failed | cancelled | unresolved | censored | no_entry | pnl SOL | pnl pessimistic SOL | EV/resolved SOL | pess EV/econ. attempt SOL | pess/exposed capital % | winrate | exits |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|"]
L = [f"# Strategy C / A - 30 min (RAM-Auswertung, in-sample) - mode {MODE}", "",
     f"Source: `{SOURCE_RUN}` | C2/A_NORMALIZED: latency {LATENCY_MS} ms, quote_max_age {QUOTE_MAX_AGE_MS} ms, TX_LANDS, slots {SLOTS} (keine Slot-Konkurrenz, geprueft)",
     "Oekonomik identisch fuer alle Laeufe: CENSORED ausgeschlossen (bezahlte Fehlversuche bleiben Verlust), UNRESOLVED = Totalverlust inkl. Fehlversuche, "
     "ENTRY_FAILED = Netzwerkgebuehr, ENTRY_CANCELLED = kostenfrei (nur Legacy-Modus / stale-before-submit).", ""]
if controls:
    L += ["## C0/C1 - PRE_PUMP_01 unveraendert (500 ms, 800/800 ms, 600000 ms, Slot-Konkurrenz aktiv)", "", *HEAD]
    for r in controls:
        L.append(row_line(f"{r['name']} (fills {r['fills']}, skips {json.dumps(r['skips'], sort_keys=True)})", r))
    L.append("")
if a_runs:
    L += ["## A - native vs. normalisiert (aktuelle A: $10/Slot, Target aus, Creator-Exit, 590 s)", "", *HEAD]
    for name, r in a_runs.items():
        L.append(row_line(f"{name} (fills {r['fills']}, skips {json.dumps(r['skips'], sort_keys=True)})", r))
    L.append("")
if baseline_rows:
    L += ["## C2 - NO_FILTER je Fenster (Preis des Wartens) und PRE_PUMP_01-Filter nachtraeglich bei 500 ms", "", *HEAD]
    for r in baseline_rows:
        L.append(row_line(r["name"], r))
for window in (WINDOWS if all_screen else []):
    unique = {}
    for r in all_screen:
        if r["window_ms"] == window and r["screen_eligible"] and r["kind"] not in ("NO_FILTER", "CONTROL_PRE_PUMP_01"):
            old = unique.get(r["pass_signature"])
            if old is None or len(json.loads(r["filters"])) < len(json.loads(old["filters"])):
                unique[r["pass_signature"]] = r  # identische Trefferliste: einfachster Filtersatz
    ranked = sorted(unique.values(), key=lambda r: r["pessimistic_ev_per_resolved_lamports"] or -10**30, reverse=True)[:15]
    L += ["", f"## C2 Top-Filter - {window} ms (Rang: pessimistischer PnL je aufgeloestem Trade; screen_eligible = >= {MIN_RESOLVED} resolved, unresolved <= {MAX_UNRESOLVED_SHARE:.0%})", "",
          "| kind | filters | resolved | failed | unresolved | censored | pnl pessimistic SOL | pess EV/resolved SOL | pess EV/econ. attempt SOL | pess/exposed capital % | winrate | PF | exits |",
          "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|"]
    for r in ranked:
        L.append(f"| {r['kind']} | `{r['filters'].replace('|', '/')}` | {r['n_resolved']} | {r['n_failed']} | {r['n_unresolved']} | {r['n_censored']} | {sol(r['pnl_pessimistic_lamports'])} | "
                 f"{sol(r['pessimistic_ev_per_resolved_lamports'])} | {sol(r['pessimistic_ev_per_economic_attempt_lamports'])} | {(r['pessimistic_per_exposed_capital_pct'] or 0):+.3f} | "
                 f"{(r['winrate'] or 0):.3f} | {'-' if r['profit_factor'] is None else round(r['profit_factor'], 3)} | {r['exits']} |")
L += ["", "## Lesehinweise", "- In-sample auf einer Capture: der beste von tausenden Filtersaetzen sieht immer gut aus. Kandidaten einfrieren, auf NEUER laengerer Capture validieren.",
      "- `screen_eligible` ist ein Explorationsfilter, keine Qualifikation nach Research-Vertrag.", "- Nur `pnl pessimistic` ranken. UNRESOLVED sind meist die toten Tokens.",
      "- CENSORED (am Ende offen) und OBSERVATION_CENSORED (Fenster am Ende offen) sind ausgeschlossen; bei 590 s Hold betrifft das bei A die letzten ~10 Minuten.",
      "- C2 ohne Slot-Konkurrenz (SLOT_LIMIT/INSUFFICIENT_CAPITAL = 0 geprueft); C0/C1 zeigen die Slot-Wirkung des unveraenderten Profils."]
summary = "\n".join(L)
(OUT_DIR / "summary.md").write_text(summary, encoding="utf-8")
print("\n=== FERTIG ===\nOUT:    ", OUT_DIR, "\nSUMMARY:", OUT_DIR / "summary.md", "\n")
print(summary)