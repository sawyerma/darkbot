"""argparse for doctor, paper, report and research; validates arguments, calls the responsible functions, sets exit codes."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from darkbot.config import ConfigError, load_runtime_config, redact

EXIT_OK, EXIT_CONFIG, EXIT_RUNTIME, EXIT_NOT_READY = 0, 2, 3, 4


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="python -m darkbot", description="DarkBot Phase-1 PAPER backend (spec 1.3.2). LIVE broadcast is technically disabled.")
    sub = p.add_subparsers(dest="command", required=True)
    d = sub.add_parser("doctor", help="config, free feed readiness, fee provenance, database and PAPER boundary checks (D01-D06); secrets redacted")
    d.add_argument("--config", type=Path, required=True)
    d.add_argument("--db", type=Path, default=None, help="probe database (default data/doctor.sqlite)")
    d.add_argument("--json", action="store_true", help="print full JSON report")
    pp = sub.add_parser("paper", help="live PAPER with auto-recording and periodic status; Ctrl-C stops controlled")
    pp.add_argument("--config", type=Path, required=True)
    pp.add_argument("--db", type=Path, required=True)
    pp.add_argument("--campaign", required=True)
    pp.add_argument("--max-open-positions", type=int, default=None,
                    help="override portfolio.max_open_positions for this PAPER run")
    pp.add_argument("--max-duration-minutes", type=int, required=True,
                    help="required total PAPER runtime in minutes; the run stops automatically when reached")
    r = sub.add_parser("report", help="deterministic latency/PnL/quality reports")
    r.add_argument("--db", type=Path, required=True)
    r.add_argument("--campaign", required=True)
    r.add_argument("--out", type=Path, required=True)
    r.add_argument("--run-id", default=None, help="specific run (default: latest PAPER run of the campaign)")
    rs = sub.add_parser("research", help="pre-registered target/latency variants on the frozen dataset; test split locked until policy freeze")
    rs.add_argument("--db", type=Path, required=True)
    rs.add_argument("--campaign", required=True)
    rs.add_argument("--config", type=Path, required=True, help="research config (config/research.json)")
    rs.add_argument("--runtime-config", type=Path, default=Path("config/phase1.json"))
    rs.add_argument("--out", type=Path, required=True)
    rs.add_argument("--source-run-id", action="append", default=None, help="recording to evaluate; repeat for a dataset, required to select fresh forward recordings")
    rs.add_argument("--max-variants", type=int, default=None)
    ab = sub.add_parser("ab-replay", help="same-capture Strategy A/B replay into a NEW output database; source is read-only")
    ab.add_argument("--source-db", type=Path, required=True)
    ab.add_argument("--source-run-id", required=True)
    ab.add_argument("--output-db", type=Path, required=True)
    ab.add_argument("--out", type=Path, required=True)
    ab.add_argument("--config-a", type=Path, default=Path("config/phase1.json"))
    ab.add_argument("--config-b", type=Path, default=Path("config/strategy_b.json"))
    ab.add_argument("--research-config", type=Path, default=Path("config/research_ab.json"))
    ab.add_argument("--measured-latency", type=Path, default=None)
    return p


def cmd_doctor(args: argparse.Namespace) -> int:
    from darkbot.main import run_doctor_cli

    result = run_doctor_cli(args.config, args.db)
    if args.json:
        print(redact(json.dumps(result, indent=2, default=str)))
    else:
        for cid, c in result["checks"].items():
            print(f"{cid}: {c['status']}  " + redact(json.dumps({k: v for k, v in c.items() if k not in ('status', 'integrity', 'python', 'snapshots')}, default=str))[:400])
        print(f"READINESS: {result['readiness']}  run_id={result['run_id']}")
    return EXIT_OK if result["readiness"] != "NOT_READY" else EXIT_NOT_READY


def cmd_paper(args: argparse.Namespace) -> int:
    from darkbot.main import run_paper

    summary = run_paper(args.config, args.db, args.campaign,
                        max_open_positions=args.max_open_positions,
                        max_duration_minutes=args.max_duration_minutes)
    # complete_delivery_contract: artifacts are produced automatically after every run, including interrupts
    try:
        from darkbot.reporting import Reporter, persist_report, write_outputs
        from darkbot.storage import open_connection

        conn = open_connection(args.db)
        report = Reporter(conn).build(summary["run_id"])
        out = Path("exports") / args.campaign / summary["run_id"]
        files = write_outputs(report, conn, out)
        persist_report(conn, report, f"report_{summary['run_id']}_{report['created_utc']}")
        conn.close()
        summary["artifacts"] = [str(f) for f in files]
        summary["qualification_status"] = report["qualification_status"]
    except Exception as exc:  # noqa: BLE001
        summary["artifacts_error"] = f"{type(exc).__name__}: {exc}"
    print(redact(json.dumps(summary, indent=2, default=str)))
    if summary.get("feeds_started", 0) == 0 or summary["readiness"] == "NOT_READY":
        print("paper: no eligible feed was ready (NOT_READY) – see feed_setup; exit code 4", file=sys.stderr)
        return EXIT_NOT_READY
    if summary["status"] not in ("COMPLETED", "INTERRUPTED") or "artifacts_error" in summary:
        return EXIT_RUNTIME
    return EXIT_OK


def cmd_report(args: argparse.Namespace) -> int:
    from darkbot.reporting import Reporter, persist_report, write_outputs
    from darkbot.storage import open_connection

    if not args.db.exists():
        print(f"database {args.db} does not exist", file=sys.stderr)
        return EXIT_RUNTIME
    conn = open_connection(args.db)
    run_id = args.run_id
    if run_id is None:
        row = conn.execute("SELECT run_id FROM runs WHERE campaign_id=? AND mode='PAPER' ORDER BY started_utc DESC LIMIT 1", (args.campaign,)).fetchone()
        if row is None:
            print(f"no PAPER run for campaign {args.campaign}", file=sys.stderr)
            return EXIT_RUNTIME
        run_id = row[0]
    report = Reporter(conn).build(run_id)
    files = write_outputs(report, conn, args.out)
    persist_report(conn, report, f"report_{run_id}_{report['created_utc']}")
    conn.close()
    print(f"run {run_id}: {report['qualification_status']} -> " + ", ".join(str(f) for f in files))
    return EXIT_OK


def cmd_research(args: argparse.Namespace) -> int:
    from darkbot.backtest import run_research
    from darkbot.storage import open_connection

    cfg = load_runtime_config(args.runtime_config)
    conn = open_connection(args.db)
    report = run_research(conn, args.campaign, cfg, args.config, args.out, max_variants=args.max_variants, source_run_ids=args.source_run_id)
    conn.close()
    print(f"research {args.campaign}: variants={report['variants_registered']} selection={report['selection_for_limit_1']} -> {args.out}")
    return EXIT_OK


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "ab-replay":
            from darkbot.ab_replay import compare_capture
            result = compare_capture(args.source_db, args.source_run_id, args.output_db, args.out,
                                     load_runtime_config(args.config_a), load_runtime_config(args.config_b), args.research_config, args.measured_latency)
            print(f"A/B replay: {len(result['results'])} runs -> {args.out / 'comparison.json'}")
            return EXIT_OK
        if args.command == "doctor":
            return cmd_doctor(args)
        if args.command == "paper":
            return cmd_paper(args)
        if args.command == "report":
            return cmd_report(args)
        if args.command == "research":
            return cmd_research(args)
    except ConfigError as exc:
        print(f"CONFIG ERROR: {redact(str(exc))}", file=sys.stderr)
        return EXIT_CONFIG
    except KeyboardInterrupt:
        return EXIT_OK
    return EXIT_CONFIG
