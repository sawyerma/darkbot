"""Synthetic run directories in layout V1 and V2 (paths from run_store.ARTIFACTS)."""
import json
import p0_paths  # noqa: F401  (puts project_0/scripts on sys.path)
import audit_market_quick as a
import run_store as s

PLAN={'schema_version':1,'duration_seconds':100,'snapshot_offsets_ns':[25*a.NS,50*a.NS,75*a.NS,100*a.NS],'budgets':{}}
TX={'signature':'sig1','slot':20,'source':'logsSubscribe','success':True,'t_rx_ns':5,'raw':{'value':{'logs':[]}}}


def manifest_run(root,name,layout,run_id='r1'):
    """Identical synthetic capture content written in either layout (paths as in run_store.ARTIFACTS)."""
    run=root/name;p=lambda key:s.artifact(run,key,layout)
    p('market').mkdir(parents=True);p('phases').parent.mkdir(parents=True,exist_ok=True)
    manifest={'schema':'C0-E1/MeasurementManifest/2','run_id':run_id,'capture_run_id':run_id,'plan':PLAN}
    if layout=='v2':manifest.update(schema=s.MANIFEST_V2_SCHEMA,layout=s.LAYOUT_V2)
    p('manifest').write_text(json.dumps(manifest))
    p('phases').write_text(json.dumps({'phase':'CAPTURE','details':{'capture_start_unix_ns':100*a.NS,'capture_cutoff_unix_ns':200*a.NS}})+'\n')
    p('market_summary').write_text(json.dumps({'capture_clock_complete':True,'live_coverage_status':'LIVE_COVERAGE_COMPLETE','observation_ended_at':a.iso_ns(200*a.NS)}))
    (p('market')/'transactions.jsonl').write_text(json.dumps(TX)+'\n')
    for name in ('liquidations.jsonl','coverage.jsonl'):(p('market')/name).write_text('')
    return run


def legacy_run(root,name):
    run=root/name;(run/'market').mkdir(parents=True)
    (run/'market/summary.json').write_text(json.dumps({'capture_clock_complete':True,'live_coverage_status':'LIVE_COVERAGE_COMPLETE'}))
    (run/'market/coverage.jsonl').write_text('{"event":"DRAINED"}\n')
    (run/'hour_01.json').write_text('{"window":1}\n')
    return run


def v2_run(root,name,layout=s.LAYOUT_V2):
    run=manifest_run(root,name,'v2',name)
    if layout!=s.LAYOUT_V2:(run/'manifest.json').write_text(json.dumps({'layout':layout,'run_id':name}))
    return run
