"""Project_0 run storage contract for new measurement runs.

Selects the root of a new run (legacy data/econ4h and data/market, or data/runs with
P0_RUN_STORE=runs) and creates its directory exactly once. Run identity never depends
on the root: capture_run_id comes from the run's manifest, or for legacy runs from the
directory name and summary hash (measurement_contract.capture_identity).

Deliberately separate from measurement_contract.py, audit_market_quick.py and
market_report.py: their file hashes are pinned into verification specs, index
checkpoints and report identities. Those readers understand layout V1 only; the CLI
entry points locate a run here and hand them its directory.

Layouts: V1 is every run written so far and carries no layout marker. V2 is the target
layout (manifest.json, config.json, capture/, snapshots/, verification/, reports/, logs/)
and is identified only by `layout` in manifest.json. Anything in between is rejected.
"""
from pathlib import Path
import fcntl
import json
import os
import re
import sys

PROJECT=Path(__file__).resolve().parents[1]
RUN_ID=re.compile(r'[A-Za-z0-9_-]{1,128}')  # same character set as econ.rs and market_observer.rs
STORES={'legacy':{'econ4h':'data/econ4h','market':'data/market'},'runs':{'econ4h':'data/runs','market':'data/runs'}}
ROOTS=('data/econ4h','data/market','data/runs')  # a run id must stay unique across all of them
LAYOUT_V2='Project0/RunLayout/2'
V2_ONLY=('manifest.json','config.json','capture','snapshots','logs')  # never at the root of a V1 run
MESSAGES={'RUN_STORE_INVALID':'P0_RUN_STORE must be legacy or runs; run kind must be econ4h or market',
          'RUN_ROOT_CONFLICT':'explicit output directory contradicts P0_RUN_STORE=runs',
          'RUN_ID_INVALID':'run id must be 1-128 characters of A-Z a-z 0-9 _ -',
          'RUN_ID_COLLISION':'run id already exists; runs are never reused or overwritten',
          'RUN_NOT_FOUND_OR_AMBIGUOUS':'no run, or more than one run, matches this path or id',
          'RUN_LAYOUT_INVALID':'run directory has a mixed or malformed layout',
          'RUN_LAYOUT_UNSUPPORTED':'layout V2 is not readable by the current pinned verification and report readers'}

class RunStoreError(Exception): pass

def run_root(kind,explicit=None,store=None,project=PROJECT):
    """Absolute root for a new run of `kind`. An explicit directory keeps working,
    but must not contradict the selected runs store."""
    store=store if store is not None else (os.environ.get('P0_RUN_STORE') or 'legacy')
    if kind not in STORES.get(store,{}):raise RunStoreError('RUN_STORE_INVALID')
    default=Path(os.path.abspath(Path(project)/STORES[store][kind]))
    if not explicit:return default
    root=Path(os.path.abspath(Path(explicit).expanduser()))
    if store=='runs' and os.path.realpath(root)!=os.path.realpath(default):raise RunStoreError('RUN_ROOT_CONFLICT')
    return root

def check_run(root,run_id,project=PROJECT):
    """Validate a new run without creating anything. The id pattern excludes path separators and dots."""
    if not isinstance(run_id,str) or not RUN_ID.fullmatch(run_id):raise RunStoreError('RUN_ID_INVALID')
    path=Path(os.path.abspath(root))/run_id
    if os.path.lexists(path) or any(os.path.lexists(Path(project)/r/run_id) for r in ROOTS):raise RunStoreError('RUN_ID_COLLISION')
    return path

def allocate_run(root,run_id,project=PROJECT):
    """Create root/run_id exactly once. An exclusive lock on the project's data directory serializes
    the cross-root check with creation, so concurrent runs cannot claim one id in two roots."""
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
    return path

def detect_layout(run):
    """'v1' or 'v2'; mixed or malformed structures raise instead of being guessed."""
    run=Path(run);manifest=run/'manifest.json'
    if os.path.lexists(manifest):
        try:layout=json.loads(manifest.read_text()).get('layout') if manifest.is_file() else None
        except (ValueError,AttributeError,OSError):layout=None
        if layout!=LAYOUT_V2:raise RunStoreError('RUN_LAYOUT_INVALID')
        return 'v2'
    if any(os.path.lexists(run/name) for name in V2_ONLY):raise RunStoreError('RUN_LAYOUT_INVALID')
    return 'v1'

def locate_run(value,project=PROJECT):
    """Existing V1 run by explicit directory, or by exact run id across all run roots.
    An id found in more than one root is an error, never silently resolved."""
    if not isinstance(value,str) or not value:raise RunStoreError('RUN_NOT_FOUND_OR_AMBIGUOUS')  # Path('') is the cwd
    direct=Path(value).expanduser()
    if direct.is_dir():run=direct.resolve()
    else:
        if not RUN_ID.fullmatch(value):raise RunStoreError('RUN_NOT_FOUND_OR_AMBIGUOUS')
        found={(Path(project)/r/value).resolve() for r in ROOTS if (Path(project)/r/value).is_dir()}
        if len(found)!=1:raise RunStoreError('RUN_NOT_FOUND_OR_AMBIGUOUS')
        run=found.pop()
    if detect_layout(run)!='v1':raise RunStoreError('RUN_LAYOUT_UNSUPPORTED')
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

def main(argv):
    """root KIND [DIR] | check ROOT RUN_ID | allocate ROOT RUN_ID | locate RUN -> prints the absolute path;
    exec TARGET.py ARGS... -> runs a reader with --run located."""
    try:
        if len(argv)>=2 and argv[0]=='exec':
            args=run_arguments(argv[2:]);os.execv(sys.executable,[sys.executable,argv[1],*args])
        if len(argv) in (2,3) and argv[0]=='root':print(run_root(argv[1],argv[2] if len(argv)==3 else None))
        elif len(argv)==2 and argv[0]=='locate':print(locate_run(argv[1]))
        elif len(argv)==3 and argv[0]=='check':print(check_run(argv[1],argv[2]))
        elif len(argv)==3 and argv[0]=='allocate':print(allocate_run(argv[1],argv[2]))
        else:print('usage: run_store.py root KIND [DIR] | check ROOT RUN_ID | allocate ROOT RUN_ID | locate RUN | exec TARGET.py ARGS',file=sys.stderr);return 2
        return 0
    except RunStoreError as exc:
        code=str(exc);print(code+': '+MESSAGES[code],file=sys.stderr);return 2
    except OSError:
        print('RUN_STORE_IO: cannot access or create the run directory, or start the reader',file=sys.stderr);return 2

if __name__=='__main__':sys.exit(main(sys.argv[1:]))
