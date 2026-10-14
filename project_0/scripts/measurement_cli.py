"""One typed parser for capture invocation and its post-capture workflow."""
from dataclasses import asdict
from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import measurement_contract as c
import operational_config


def finish(run,path):
    import measurement_verify as verify
    import market_report
    data=json.loads(Path(path).read_text());cfg=data['config']
    v=c.VerificationConfig(**cfg['verification']).validate()
    q=c.ReportQuery(**cfg['report'])
    qualification=Path(data['provider_qualification']) if data.get('provider_qualification') else None
    result=verify.verify(run,v,qualification)
    with (run/('post_capture_report.'+q.format)).open('x') as f:
        metadata=market_report.report(run,q,result['verification_result_id'],f)
    c.immutable(run/'post_capture_result.json',{'verification_result_id':result['verification_result_id'],
        'report':metadata,'capture_status_separate_from_verification':True})
    print('Verification:',result['summary']['status'],'; report:',metadata['dataset'])
    return 0 if result['summary']['status'] in ('NOT_RUN','INDEX_VERIFIED','CANONICAL_VERIFIED') else 2


def launch(argv):
    p=c.parser('C0-E1: --duration 10m|1h|5h|24h. Rust MeasurementPlan owns every boundary; no total capture timebox.')
    with operational_config.parsed(p,argv) as (args,effective,environment):
        return launch_config(p,argv,args,effective,environment)


def launch_config(p,argv,args,effective,environment):
    cfg=c.config(args)
    if not args.duration:p.error('--duration required')
    if argv.count('--duration')>1:p.error('exactly one duration required')
    if args.run or args.resume_verification:p.error('capture cannot resume; use verify.sh or report.sh on an existing run')
    if args.offline:p.error('capture uses real streams; offline operations are verify.sh/report.sh')
    data={'operational_config':effective,'config':asdict(cfg),'provider_qualification':str(args.provider_qualification.resolve()) if args.provider_qualification else None}
    # Keep CLI precedence over sourced local env. Shell receives only this file;
    # interpretation/validation stays in the shared typed backend.
    with tempfile.TemporaryDirectory(prefix='p0-measurement-cli-') as tmp:
        path=Path(tmp)/'config.json';c.immutable(path,data)
        return subprocess.call([str(Path(__file__).with_name('run_econ4h.sh')),'--measurement-config',str(path)],env=environment)


def prepare(run,path):
    data=json.loads(Path(path).read_text());v=c.VerificationConfig(**data['config']['verification']).validate()
    qpath=Path(data['provider_qualification']) if data.get('provider_qualification') else None
    pf=c.preflight(run,v,qpath,c.endpoint())
    c.immutable(run/'measurement_config.json',data['config'])
    if data.get('operational_config'):
        # Re-resolve after the existing shell has loaded local endpoints. Private
        # file/CLI values still dominate; only inherited endpoints are completed.
        with operational_config.resolve([]) as (_,effective,__):c.immutable(run/'operational_config.json',effective)
    c.immutable(run/'verification_preflight.json',pf)
    if v.mode!='off' and not pf['passed']:raise c.ContractError('VERIFICATION_PREFLIGHT_OPEN; choose off or qualify the FREE_ONLY provider first')


def main():
    try:
        if len(sys.argv)==4 and sys.argv[1] in ('finish','prepare'):
            run=Path(sys.argv[2]).resolve()
            if sys.argv[1]=='prepare':prepare(run,sys.argv[3]);return 0
            return finish(run,sys.argv[3])
        return launch(sys.argv[1:])
    except (c.ContractError,OSError,ValueError,KeyError,TypeError) as exc:
        print(str(exc) if isinstance(exc,c.ContractError) else 'MEASUREMENT_WORKFLOW_INPUT_OR_IO_ERROR',file=sys.stderr);return 2

if __name__=='__main__':sys.exit(main())
