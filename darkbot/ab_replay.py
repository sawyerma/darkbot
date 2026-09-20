"""Paired strategy research over one frozen capture, using the shared replay and reports."""
from __future__ import annotations
import json
from datetime import datetime
from pathlib import Path
from darkbot.config import ConfigError, build_runtime_config, config_hash, load_json_strict
from darkbot.domain import NS_PER_MS
from darkbot.replay import load_replay_input, run_replay
from darkbot.reporting import Reporter, write_outputs
from darkbot.storage import apply_migrations, open_connection, upsert_config_snapshot


def load_measured_distribution(path):
    if path is None:
        return None
    data = load_json_strict(path)
    if set(data) != {"source", "observed_utc", "entry_delay_ms", "exit_delay_ms"} or not isinstance(data["source"], str) or not data["source"].strip():
        raise ConfigError("measured latency requires source, observed_utc and entry/exit_delay_ms sample arrays")
    try:
        stamp = datetime.fromisoformat(data["observed_utc"].replace("Z", "+00:00"))
        if stamp.tzinfo is None:
            raise ValueError("timestamp must include timezone")
    except (ValueError, TypeError, AttributeError) as exc:
        raise ConfigError("invalid measured latency observed_utc") from exc
    for key in ("entry_delay_ms", "exit_delay_ms"):
        values = data[key]
        if not isinstance(values, list) or not values or any(type(v) is not int or v < 0 for v in values):
            raise ConfigError("measured delays must be nonempty arrays of nonnegative integer milliseconds")
    return data


def compare_capture(source_db: Path, source_run_id: str, output_db: Path, out: Path, config_a, config_b,
                    research_path: Path, measured_path: Path | None = None):
    """Source opened read-only. Each variant gets its own persisted run, same frames/fees/FX/portfolio/delay assumptions."""
    if source_db.resolve() == output_db.resolve() or output_db.exists():
        raise ConfigError("A/B output database must be a new file, separate from the source capture")
    if config_a.strategy_id != "FAST_CREATOR_HOLD_V1" or config_b.strategy_id not in ("LEGACY_5BUY_5UNIQUE_V1", "LEGACY_BACKEND_FILTER_V1"):
        raise ConfigError("A/B requires Strategy A first, then a Strategy B or Strategy C config")
    shared = set(config_a.raw) - {"strategy", "config_version", "fx"}
    if any(config_a.raw[k] != config_b.raw[k] for k in shared) or config_a.raw["fx"]["max_age_ms"] != config_b.raw["fx"]["max_age_ms"]:
        raise ConfigError("A/B configs must share engine, costs, feeds, portfolio, latency and FX freshness settings")
    research = load_json_strict(research_path)
    if set(research) != {"config_version", "portfolio_position_limits", "latency_scenarios", "measured_distribution_required_scenario", "missing_measurements_policy"}:
        raise ConfigError("invalid A/B research config keys")
    limits = research["portfolio_position_limits"]
    if not isinstance(limits,list) or not limits or any(type(x) is not int or x < 1 for x in limits):
        raise ConfigError("A/B portfolio limits must be positive integers")
    scenarios = research["latency_scenarios"]
    if not isinstance(scenarios,list) or not scenarios:
        raise ConfigError("A/B requires latency scenarios")
    for sc in scenarios:
        if set(sc) != {"label", "entry_delay_ms", "exit_delay_ms"} or not isinstance(sc['label'],str) or any(type(sc[k]) is not int or sc[k] < 0 for k in ('entry_delay_ms','exit_delay_ms')):
            raise ConfigError("invalid A/B latency scenario")
    measured = load_measured_distribution(measured_path)
    measured_hash = config_hash(measured) if measured else None
    if measured:
        scenarios = scenarios + [{"label": research["measured_distribution_required_scenario"], "entry_delay_ms": 0, "exit_delay_ms": 0}]
    source = open_connection(source_db, read_only=True)
    try:
        frozen = load_replay_input(source, source_run_id, config_a)
        source_quality = source.execute("SELECT quality_status FROM runs WHERE run_id=?", (source_run_id,)).fetchone()[0]
        mints = [r[0] for r in source.execute("SELECT DISTINCT mint FROM market_events WHERE run_id=?", (source_run_id,))]
    finally:
        source.close()
    conn = open_connection(output_db)
    apply_migrations(conn)
    results = []
    try:
        if measured:
            upsert_config_snapshot(conn, "MEASURED_EXECUTION_DISTRIBUTION", measured_hash, measured, str(measured_path), measured['observed_utc'])
        upsert_config_snapshot(conn, "AB_RESEARCH", config_hash(research), research, str(research_path), "")
        for limit in limits:
            for scenario_index, scenario in enumerate(scenarios):
                empirical = measured is not None and scenario_index == len(scenarios)-1
                delays = {}
                if empirical:
                    for mint in mints:
                        for side,key in (("BUY","entry_delay_ms"),("SELL","exit_delay_ms")):
                            for attempt in range(1,config_a.execution['max_exit_attempts']+1):
                                index = int(config_hash([mint,side,attempt,measured_hash]),16) % len(measured[key])
                                delays[mint,side,attempt] = measured[key][index]*NS_PER_MS
                provider = (lambda intent: delays[intent.mint,intent.side.value,intent.attempt_number]) if empirical else None
                for tag,cfg in (("A",config_a),("B",config_b)):
                    variant=cfg.with_overrides(portfolio={'max_open_positions':limit},latency_model={**scenario,'measured_distribution_id':measured_hash if empirical else None})
                    run_id=f"ab_{tag}_L{limit}_S{scenario_index}_{frozen.dataset_hash[:12]}"
                    result=run_replay(conn,frozen,variant,run_id,'strategy_ab',persist=True,persist_observations=True,
                                      replay_callback_lateness=False,execution_delay_provider=provider)
                    if result.owner.disk_error or result.owner.recorder.overflowed or result.owner.counters.internal_errors:
                        raise RuntimeError("A/B replay storage or engine failure")
                    if source_quality != "OK":
                        conn.execute("UPDATE runs SET quality_status=? WHERE run_id=?", ("SOURCE_"+str(source_quality),run_id))
                    report=Reporter(conn).build(run_id)
                    write_outputs(report,conn,out/run_id)
                    results.append({'strategy_id':variant.strategy_id,'strategy_config_hash':variant.strategy_config_hash,
                                    'run_id':run_id,'portfolio_limit':limit,'latency_scenario':scenario['label'],
                                    'dataset_hash':frozen.dataset_hash,'creates':report['creates_observed_union'],
                                    'qualified_creates':report['qualified_creates'],'economic':report['economic'],
                                    'latency':report['latency'],'qualification_status':report['qualification_status']})
    finally:
        conn.close()
    summary={'dataset_hash':frozen.dataset_hash,'source_run_id':source_run_id,
             'comparison_scope':'Same frames, fee/FX evidence, decoder, engine and costs; strategy policy only varies within each portfolio/latency pair',
             'callback_lateness':'Scenario execution delays shared; source per-position callback lateness excluded because A/B position numbering differs',
             'measured_distribution':{'status':'AVAILABLE' if measured else 'UNAVAILABLE_NO_MEASUREMENTS_SUPPLIED','hash':measured_hash},
             'usd_evidence_status':'RECORDED' if frozen.fx_snapshots else 'MISSING_B_ENTRIES_FAIL_CLOSED',
             'results':results}
    out.mkdir(parents=True,exist_ok=True)
    (out/'comparison.json').write_text(json.dumps(summary,indent=2)+'\n')
    return summary
