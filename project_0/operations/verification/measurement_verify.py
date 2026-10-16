"""Post-capture lifecycle around the existing P3 verifier and Rust strict engine."""
from dataclasses import asdict
from pathlib import Path
import fcntl
import json
import os
import resource
import shutil
import subprocess
import sys
import time
import uuid
if __package__ in (None,''):sys.path.insert(0,str(Path(__file__).resolve().parents[2]))  # run as a file: make `operations` importable
from operations.verification import audit_market_quick as audit
from operations.cli import measurement_contract as c
from operations.cli import operational_config
from operations.capture import run_store


def resource_guard(output,v,deadline):
    if time.monotonic()>=deadline:raise audit.AuditError('VERIFICATION_TIME_BUDGET')
    if shutil.disk_usage(output).free<v.min_free_bytes:raise audit.AuditError('VERIFICATION_DISK_FREE')
    size=sum(p.stat().st_size for p in output.rglob('*') if p.is_file())
    if size>=v.max_disk_bytes:raise audit.AuditError('VERIFICATION_DISK_BUDGET')
    rss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform!='darwin':rss*=1024
    if rss+64*1024**2>v.max_ram_bytes:raise audit.AuditError('VERIFICATION_RAM_BUDGET')


def archived_specs(run,spec):
    """Specs for this capture and mode written by another implementation (schema, decoder pins or layout)."""
    found=[]
    for p in sorted((run/'verification').glob('*/spec.json')):
        old=json.loads(p.read_text())
        if (old.get('capture_run_id')==spec['capture_run_id'] and old.get('mode')==spec['mode']
            and any(old.get(k)!=spec.get(k) for k in ('schema','decoder_pins','layout'))):
            found.append(f"{p.parent.name} ({old.get('schema')})")
    return found


def journal_views(run):
    """PERSIST-04: the JSONL/JSON views the pinned readers consume are explicit exports of the
    run's binary journals. Before the capture is hashed, render missing views (for example after
    a killed capture) and require every existing view to equal its render byte for byte.
    Historical runs without journals are left exactly as they are."""
    if not any(p.suffix=='.p0j' for p in run.rglob('*.p0j') if 'verification' not in p.relative_to(run).parts):return None
    binary=run_store.PROJECT/'native/target/release/p0-journal'
    if not binary.exists():raise c.ContractError('JOURNAL_TOOL_MISSING: build p0-journal (capture.sh builds it)')
    p=subprocess.run([str(binary),'renders',str(run),'--create'],capture_output=True,text=True,timeout=3600)
    if p.returncode:raise c.ContractError('JOURNAL_VIEWS_INVALID: '+(p.stderr.strip().splitlines() or ['?'])[-1][:500])
    return json.loads(p.stdout)


def verify(run,v,qualification=None,offline=False,resume=False):
    run=Path(run).resolve();journal_views(run)
    evidence=c.finalize_capture(run);c.check_capture(run,evidence,hashes=True)
    url=c.endpoint();pf=c.preflight(run,v,qualification,url)
    spec=c.specification(run,v,pf['provider_binding']);sid=spec['verification_spec_id']
    out=run/'verification'/sid
    if resume and not (out/'spec.json').exists():
        # Never continue an attempt of another implementation; it needs the archived code that wrote it.
        archived=archived_specs(run,spec)
        if archived:raise c.ContractError('RESUME_REQUIRES_ARCHIVED_IMPLEMENTATION: '+', '.join(archived))
    out.mkdir(parents=True,exist_ok=True)
    lock=(out/'writer.lock').open('a')
    try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
    except BlockingIOError:
        lock.close();raise c.ContractError('VERIFIER_ALREADY_RUNNING') from None
    try:
        prior=(out/'spec.json').exists()
        if prior and not resume:raise c.ContractError('SPEC_EXISTS_USE_RESUME_VERIFICATION')
        c.immutable(out/'spec.json',spec)
        attempt=uuid.uuid4().hex;attemptdir=out/'attempts'/attempt;attemptdir.mkdir(parents=True)
        c.immutable(attemptdir/'start.json',{'verification_attempt_id':attempt,'verification_spec_id':sid,
            'started_unix_ns':time.time_ns(),'preflight':pf,'offline':offline})
        summary={'status':'NOT_RUN','canonical_verified':False,'reason':None,'c0_e1_master_pass':False}
        artifacts={};deadline=time.monotonic()+v.max_seconds
        try:
            resource_guard(out,v,deadline)
            if v.mode=='off':pass
            elif not offline and not pf['passed']:
                summary.update(status='HISTORICAL_INCOMPLETE',reason='PREFLIGHT_FAILED',preflight=pf)
            elif v.mode=='crosscheck':
                # Optional independent source must be explicitly qualified; never
                # promote a single index or a same-provider duplicate into a crosscheck.
                summary.update(status='HISTORICAL_INCOMPLETE',reason='INDEPENDENT_CROSSCHECK_QUALIFICATION_OPEN')
            elif v.mode=='index':
                rpc=audit.RPC(url,v.rps,v.timeout,v.attempts,v.max_calls,v.max_seconds,v.max_bytes,**operational_config.transport_options()) if url and not offline else None
                if rpc:
                    call=rpc.call
                    def bounded(method,params):
                        resource_guard(out,v,deadline)
                        value=call(method,params)
                        resource_guard(out,v,deadline)
                        return value
                    rpc.call=bounded
                a=audit.Audit(run,rpc,v.max_pages,v.max_seconds,output=out/'index')
                try:
                    summary=a.execute()
                    # Immutable outcome snapshot; checkpoints may advance later.
                    for name in ('comparison.jsonl','bot_only.jsonl'):
                        dest=attemptdir/name
                        with (a.out/name).open('rb') as src,dest.open('xb') as dst:
                            shutil.copyfileobj(src,dst,1024*1024);dst.flush();os.fsync(dst.fileno())
                        artifacts[name]={'path':str(dest.relative_to(run)),'sha256':c.file_hash(dest)}
                finally:a.close()
            else:
                binary=run_store.PROJECT/'native/target/release/p0-strict-verify'
                if not binary.exists():raise audit.AuditError('STRICT_BINARY_NOT_BUILT')
                job={**asdict(v),'verification_spec_id':sid,'output':str(out/'strict'),
                     'program':audit.PROGRAM,'start_ns':spec['window']['start_ns'],'end_ns':spec['window']['end_ns'],
                     'rps':v.rps,'offline':offline}
                c.immutable(attemptdir/'job.json',job)
                with (attemptdir/'worker_stderr.txt').open('x') as err:
                    proc=subprocess.run([str(binary),str(attemptdir/'job.json')],capture_output=False,
                        stdout=subprocess.PIPE,stderr=err,timeout=v.max_seconds+20,text=True)
                if proc.returncode:raise audit.AuditError('STRICT_WORKER_FAILED')
                summary=json.loads(proc.stdout)
                summary['binary_sha256']=c.file_hash(binary)
                for batch in summary.get('batches',[]):
                    path=out/'strict'/batch['checkpoint']
                    artifacts[batch['checkpoint']]={'path':str(path.relative_to(run)),'sha256':c.file_hash(path)}
            resource_guard(out,v,deadline)
            c.check_capture(run,evidence)
        except (audit.AuditError,c.ContractError) as exc:
            summary.update(status='HISTORICAL_INCOMPLETE',canonical_verified=False,reason=str(exc))
        except KeyboardInterrupt:
            summary.update(status='HISTORICAL_INCOMPLETE',canonical_verified=False,reason='INTERRUPTED_RESUMABLE')
        except subprocess.TimeoutExpired:
            summary.update(status='HISTORICAL_INCOMPLETE',canonical_verified=False,reason='VERIFICATION_TIME_BUDGET')
        except (OSError,ValueError,KeyError,TypeError):
            summary.update(status='HISTORICAL_INCOMPLETE',canonical_verified=False,reason='LOCAL_IO_OR_CHECKPOINT_INVALID')
        payload={'schema':'C0-E1/VerificationResult/2','capture_run_id':evidence['capture_run_id'],
            'verification_spec_id':sid,'verification_attempt_id':attempt,'mode':v.mode,
            'capture_evidence_digest':evidence['evidence_digest'],'live_coverage_status':evidence['live_coverage_status'],
            'window':spec['window'],'summary':summary,'artifacts':artifacts,'finished_unix_ns':time.time_ns(),
            'master_pass':False,'live_enabled':False,'submission_enabled':False}
        result={**payload,'verification_result_id':c.digest(payload)}
        c.immutable(out/'results'/(result['verification_result_id']+'.json'),result)
        c.immutable(attemptdir/'end.json',result)
        return result
    finally:
        fcntl.flock(lock,fcntl.LOCK_UN);lock.close()


def main(argv=None):
    parser=c.parser('Read-only C0-E1 verifier. No capture, signer or submission.')
    parser.set_defaults(mode='index')
    try:
        with operational_config.parsed(parser,argv) as (args,effective,environment):
            return execute(parser,args)
    except (OSError,ValueError) as exc:
        print('HISTORICAL_INCOMPLETE: OPERATIONAL_CONFIG_INVALID',file=sys.stderr);return 2


def execute(parser,args):
    try:
        cfg=c.config(args)
        if not args.run:parser.error('--run required')
        run=audit.find_run(args.run)
        if args.preflight:
            result=c.preflight(run,cfg.verification,args.provider_qualification,c.endpoint())
            print(json.dumps(result,indent=2));return 0 if result['passed'] else 2
        result=verify(run,cfg.verification,args.provider_qualification,args.offline,args.resume_verification)
        print(json.dumps(result,indent=2))
        return 0 if result['summary']['status'] in ('NOT_RUN','INDEX_VERIFIED','CANONICAL_VERIFIED','CROSSCHECK_VERIFIED') else 2
    except (c.ContractError,audit.AuditError) as exc:
        print('HISTORICAL_INCOMPLETE:',str(exc),file=sys.stderr);return 2
    except (OSError,ValueError,KeyError,TypeError):
        print('HISTORICAL_INCOMPLETE: LOCAL_INPUT_OR_IO_ERROR',file=sys.stderr);return 2

if __name__=='__main__':sys.exit(main())
