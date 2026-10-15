"""Project_0 run storage and run layout contract.

New econ4h runs default to data/runs/<run_id> in layout V2; P0_RUN_STORE=legacy keeps data/econ4h
in layout V1. Market observer runs keep data/market (V1) unless P0_RUN_STORE=runs.
Selects the root and layout of a new run, creates its directory exactly once, and maps
logical run artifacts to paths for layout V1 (every historical run) and layout V2
(data/runs/<run_id>/{manifest.json,config.json,capture,snapshots,verification,reports,logs}).
Run identity never depends on the root: capture_run_id comes from the run's manifest, or for
legacy V1 runs from the directory name and summary hash (measurement_contract.capture_identity).

This file is hash-pinned into verification specs, index checkpoints and report datasets,
because the pinned readers resolve every run path through ARTIFACTS. Any change creates new
identities; see docs/architecture/run_layout_versioning.md.
"""
from pathlib import Path
import fcntl
import json
import os
import re
import sys
import uuid

PROJECT=Path(__file__).resolve().parents[2]
RUN_ID=re.compile(r'[A-Za-z0-9_-]{1,128}')  # same character set as econ.rs and market_observer.rs
STORES={'legacy':{'econ4h':'data/econ4h','market':'data/market'},'runs':{'econ4h':'data/runs','market':'data/runs'}}
DEFAULT_STORE={'econ4h':'runs','market':'legacy'}  # standalone C0-MO has no pre-capture manifest, so it keeps V1 in data/market
ROOTS=('data/econ4h','data/market','data/runs')  # a run id must stay unique across all of them
LAYOUT_V1='Project0/RunLayout/1'
LAYOUT_V2='Project0/RunLayout/2'
LAYOUT_TAGS={'v1':LAYOUT_V1,'v2':LAYOUT_V2}
MANIFEST_V2_SCHEMA='C0-E1/MeasurementManifest/3'
RUN_CONFIG_SCHEMA='Project0/RunConfig/1'
V2_ONLY=('manifest.json','config.json','capture','snapshots','logs')  # never at the root of a V1 run
V2_ROOT=('manifest.json','config.json','capture','snapshots','verification','reports','logs')
ARTIFACTS={  # logical name: (V1 path, V2 path); None = not part of that layout, '' = the run directory
    'manifest':('measurement_manifest.json','manifest.json'),
    'config':(None,'config.json'),
    'phases':('measurement_phases.jsonl','capture/measurement_phases.jsonl'),
    'market':('market','capture/market'),
    'market_summary':('market/summary.json','capture/market/summary.json'),
    'shadow_evaluations':('shadow_evaluations.jsonl','capture/shadow_evaluations.jsonl'),
    'shadow_offsets':('shadow_offsets.bin','capture/shadow_offsets.bin'),
    'opportunities':('opportunities.jsonl','capture/opportunities.jsonl'),
    'daily_summary':('daily_summary.jsonl','capture/daily_summary.jsonl'),
    'mt0':('mt0.jsonl','capture/mt0.jsonl'),
    'canonical_market_shadow':('canonical_market_shadow.jsonl','snapshots/canonical_market_shadow.jsonl'),
    'opportunity_economics':('opportunity_economics.jsonl','snapshots/opportunity_economics.jsonl'),
    **{f'hour_{i:02}':(f'hour_{i:02}.json',f'snapshots/hour_{i:02}.json') for i in range(1,5)},
    'economics_final':('economics_final.json','snapshots/economics_final.json'),
    'measurement_plan':('measurement_plan.json',None),
    'capture_legacy_identity':('capture_legacy_identity.json',None),
    'capture_evidence':('capture_evidence_manifest.json','verification/capture_evidence_manifest.json'),
    'verification':('verification','verification'),
    'reports':('reports','reports'),
    'post_capture_result':('post_capture_result.json','reports/post_capture_result.json'),
    **{f'post_capture_report_{f}':(f'post_capture_report.{f}',f'reports/post_capture_report.{f}') for f in ('table','json','csv')},
    'logs':('','logs'),
    'build_log':('build.log','logs/build.log'),
    'runtime_log':('runtime.log','logs/runtime.log'),
    'console_summary':('console_summary.txt','logs/console_summary.txt'),
    'gap_proxy_log':('gap_proxy.log','logs/gap_proxy.log'),
}
CONFIG_PARTS=('measurement_plan','measurement_config','operational_config','verification_preflight','storage_preflight','candidate_manifest')
CONFIG_REQUIRED=('measurement_plan','storage_preflight','candidate_manifest')
MESSAGES={'RUN_STORE_INVALID':'P0_RUN_STORE must be legacy or runs; run kind must be econ4h or market; layout must be v1 or v2',
          'RUN_ROOT_CONFLICT':'explicit output directory contradicts P0_RUN_STORE=runs',
          'RUN_ID_INVALID':'run id must be 1-128 characters of A-Z a-z 0-9 _ -',
          'RUN_ID_COLLISION':'run id already exists; runs are never reused or overwritten',
          'RUN_NOT_FOUND_OR_AMBIGUOUS':'no run, or more than one run, matches this path or id',
          'RUN_LAYOUT_INVALID':'run directory has a mixed, incomplete or malformed layout',
          'RUN_LAYOUT_UNSUPPORTED':'run layout version is not supported by this implementation',
          'RUN_ARTIFACT_UNDEFINED':'artifact is not part of this run layout',
          'RUN_CONFIG_INVALID':'run configuration parts are missing, unknown, malformed or already written'}

class RunStoreError(Exception): pass

def resolve_store(kind,explicit=None,store=None,project=PROJECT):
    """(store, absolute root) for a new run of `kind`. An explicit directory keeps the legacy
    semantics unless it is the runs root itself, and must not contradict P0_RUN_STORE=runs."""
    runs=Path(os.path.abspath(Path(project)/'data/runs'))
    if store is None:store=os.environ.get('P0_RUN_STORE') or None
    if store is None:
        if explicit:store='runs' if os.path.realpath(Path(explicit).expanduser())==os.path.realpath(runs) else 'legacy'
        else:store=DEFAULT_STORE.get(kind)
    if kind not in STORES.get(store,{}):raise RunStoreError('RUN_STORE_INVALID')
    default=Path(os.path.abspath(Path(project)/STORES[store][kind]))
    if not explicit:return store,default
    root=Path(os.path.abspath(Path(explicit).expanduser()))
    if store=='runs' and os.path.realpath(root)!=os.path.realpath(default):raise RunStoreError('RUN_ROOT_CONFLICT')
    return store,root

def run_root(kind,explicit=None,store=None,project=PROJECT):
    return resolve_store(kind,explicit,store,project)[1]

def run_layout(kind,explicit=None,store=None,project=PROJECT):
    """Layout V2 for econ4h runs in the runs store; every other run keeps layout V1."""
    return 'v2' if kind=='econ4h' and resolve_store(kind,explicit,store,project)[0]=='runs' else 'v1'

def check_run(root,run_id,project=PROJECT):
    """Validate a new run without creating anything. The id pattern excludes path separators and dots."""
    if not isinstance(run_id,str) or not RUN_ID.fullmatch(run_id):raise RunStoreError('RUN_ID_INVALID')
    path=Path(os.path.abspath(root))/run_id
    if os.path.lexists(path) or any(os.path.lexists(Path(project)/r/run_id) for r in ROOTS):raise RunStoreError('RUN_ID_COLLISION')
    return path

def allocate_run(root,run_id,project=PROJECT,layout='v1'):
    """Create root/run_id exactly once. An exclusive lock on the project's data directory serializes
    the cross-root check with creation, so concurrent runs cannot claim one id in two roots."""
    if layout not in LAYOUT_TAGS:raise RunStoreError('RUN_STORE_INVALID')
    check_run(root,run_id,project)
    data=Path(project)/'data';data.mkdir(parents=True,exist_ok=True)
    fd=os.open(data,os.O_RDONLY)
    try:
        fcntl.flock(fd,fcntl.LOCK_EX)
        path=check_run(root,run_id,project)
        path.parent.mkdir(parents=True,exist_ok=True)
        try:path.mkdir()
        except FileExistsError:raise RunStoreError('RUN_ID_COLLISION') from None
    finally:
        os.close(fd)
    if layout=='v2':
        for name in ('capture','snapshots','logs'):(path/name).mkdir()
    return path

def detect_layout(run):
    """'v1' or 'v2'; mixed, incomplete or malformed structures raise instead of being guessed."""
    run=Path(run);manifest=run/'manifest.json'
    if os.path.lexists(manifest):
        try:m=json.loads(manifest.read_text()) if manifest.is_file() else None
        except (ValueError,OSError):m=None
        layout=m.get('layout') if isinstance(m,dict) else None
        if isinstance(layout,str) and layout!=LAYOUT_V2 and re.fullmatch(r'Project0/RunLayout/\d+',layout):raise RunStoreError('RUN_LAYOUT_UNSUPPORTED')
        if layout!=LAYOUT_V2 or m.get('schema')!=MANIFEST_V2_SCHEMA:raise RunStoreError('RUN_LAYOUT_INVALID')
        if any(not p.name.startswith('.') and p.name not in V2_ROOT for p in run.iterdir()):raise RunStoreError('RUN_LAYOUT_INVALID')
        return 'v2'
    if any(os.path.lexists(run/name) for name in V2_ONLY):raise RunStoreError('RUN_LAYOUT_INVALID')
    return 'v1'

def artifact(run,name,layout=None):
    """Path of a logical run artifact in the run's layout."""
    layout=layout or detect_layout(run)
    if layout not in LAYOUT_TAGS:raise RunStoreError('RUN_STORE_INVALID')
    rel=ARTIFACTS[name][layout=='v2']
    if rel is None:raise RunStoreError('RUN_ARTIFACT_UNDEFINED')
    return Path(run)/rel if rel else Path(run)

def market_dir(run,layout=None):
    """V1 keeps its historical fallback to the run directory when market/ is absent."""
    layout=layout or detect_layout(run);market=artifact(run,'market',layout)
    return market if layout=='v2' or market.is_dir() else Path(run)

def market_summary(run,layout=None):
    layout=layout or detect_layout(run);summary=artifact(run,'market_summary',layout)
    return summary if layout=='v2' or summary.exists() else Path(run)/'summary.json'

def config_part(run,part,layout=None):
    """A pre-capture configuration document: a separate file in V1, a key of config.json in V2."""
    if part not in CONFIG_PARTS:raise RunStoreError('RUN_ARTIFACT_UNDEFINED')
    layout=layout or detect_layout(run)
    if layout=='v1':
        p=Path(run)/(part+'.json')
        return json.loads(p.read_text()) if p.exists() else None
    config=json.loads(artifact(run,'config','v2').read_text())
    if config.get('schema')!=RUN_CONFIG_SCHEMA or part not in config:raise RunStoreError('RUN_CONFIG_INVALID')
    return config[part]

def write_config(run,stage):
    """Assemble config.json of a V2 run exactly once from the staged pre-capture documents."""
    run,stage=Path(run),Path(stage);parts={}
    for p in stage.iterdir():
        if p.suffix!='.json' or p.stem not in CONFIG_PARTS or not p.is_file():raise RunStoreError('RUN_CONFIG_INVALID')
        try:parts[p.stem]=json.loads(p.read_text())
        except (ValueError,UnicodeError):raise RunStoreError('RUN_CONFIG_INVALID') from None
    if any(name not in parts for name in CONFIG_REQUIRED):raise RunStoreError('RUN_CONFIG_INVALID')
    value={'schema':RUN_CONFIG_SCHEMA,'layout':LAYOUT_V2,'run_id':run.name,**{name:parts.get(name) for name in CONFIG_PARTS}}
    target=run/'config.json';tmp=run/('.config.json.'+uuid.uuid4().hex)
    try:
        with tmp.open('x') as f:
            json.dump(value,f,sort_keys=True,indent=2,allow_nan=False);f.write('\n');f.flush();os.fsync(f.fileno())
        try:os.link(tmp,target)
        except FileExistsError:raise RunStoreError('RUN_CONFIG_INVALID') from None
        fd=os.open(run,os.O_RDONLY)
        try:os.fsync(fd)
        finally:os.close(fd)
    finally:
        tmp.unlink(missing_ok=True)
    return target

def locate_run(value,project=PROJECT):
    """Existing V1 or V2 run by explicit directory, or by exact run id across all run roots.
    An id found in more than one root is an error, never silently resolved."""
    if not isinstance(value,str) or not value:raise RunStoreError('RUN_NOT_FOUND_OR_AMBIGUOUS')  # Path('') is the cwd
    direct=Path(value).expanduser()
    if direct.is_dir():run=direct.resolve()
    else:
        if not RUN_ID.fullmatch(value):raise RunStoreError('RUN_NOT_FOUND_OR_AMBIGUOUS')
        found={(Path(project)/r/value).resolve() for r in ROOTS if (Path(project)/r/value).is_dir()}
        if len(found)!=1:raise RunStoreError('RUN_NOT_FOUND_OR_AMBIGUOUS')
        run=found.pop()
    detect_layout(run)
    return run

def run_arguments(argv,project=PROJECT):
    """Replace `--run ID|PATH` and `--run=ID|PATH` by the located run directory."""
    if '--help' in argv or '-h' in argv:return list(argv)
    out=[];i=0
    while i<len(argv):
        if argv[i]=='--run' and i+1<len(argv):out+=['--run',str(locate_run(argv[i+1],project))];i+=2
        elif argv[i].startswith('--run='):out.append('--run='+str(locate_run(argv[i][6:],project)));i+=1
        else:out.append(argv[i]);i+=1
    return out

USAGE=('usage: run_store.py root KIND [DIR] | layout KIND [DIR] | check ROOT RUN_ID | allocate ROOT RUN_ID [v1|v2] | '
       'locate RUN | config RUN_DIR STAGE_DIR | path RUN ARTIFACT [v1|v2] | exec TARGET.py ARGS')

def main(argv):
    """Prints absolute paths (or the layout); exec TARGET.py ARGS... runs a reader with --run located."""
    try:
        if len(argv)>=2 and argv[0]=='exec':
            args=run_arguments(argv[2:]);os.execv(sys.executable,[sys.executable,argv[1],*args])
        n=len(argv);cmd=argv[0] if argv else None
        if n in (2,3) and cmd=='root':print(run_root(argv[1],argv[2] if n==3 else None))
        elif n in (2,3) and cmd=='layout':print(run_layout(argv[1],argv[2] if n==3 else None))
        elif n==2 and cmd=='locate':print(locate_run(argv[1]))
        elif n==3 and cmd=='check':print(check_run(argv[1],argv[2]))
        elif n in (3,4) and cmd=='allocate':print(allocate_run(argv[1],argv[2],layout=argv[3] if n==4 else 'v1'))
        elif n==3 and cmd=='config':print(write_config(argv[1],argv[2]))
        elif n in (3,4) and cmd=='path' and argv[2] in ARTIFACTS:print(artifact(Path(os.path.abspath(argv[1])),argv[2],argv[3] if n==4 else None))
        else:print(USAGE,file=sys.stderr);return 2
        return 0
    except RunStoreError as exc:
        code=str(exc);print(code+': '+MESSAGES[code],file=sys.stderr);return 2
    except (OSError,ValueError):
        print('RUN_STORE_IO: cannot access or create the run directory, or start the reader',file=sys.stderr);return 2

if __name__=='__main__':sys.exit(main(sys.argv[1:]))
