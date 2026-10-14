"""C0-E1 V2 shared terminal/frontend contract. No trading or submission imports.

The Rust MeasurementPlan remains the sole duration/boundary calculator.
Run/verification configuration and display queries are separate typed values.
"""
from dataclasses import dataclass, asdict
from pathlib import Path
import argparse
import hashlib
import json
import math
import os
import shutil
import time
import uuid

SCHEMA='C0-E1/MeasurementConfigV2'
PROTOCOL='33c67987a661f59c6683a65485ebb4f0ef030b77'
KINDS=('CLASSIC','RECEIVERSHIP','FORCED_DELEVERAGE','BANKRUPTCY_SETTLEMENT')
MARKERS=('lending_account_liquidate','start_liquidation','end_liquidation','start_deleverage','end_deleverage','lending_pool_handle_bankruptcy')
RELATED=('drift_claim_bad_debt','purge_deleverage_balance','marginfi_account_keeper_close_order','marginfi_account_init_liq_record')

class ContractError(Exception): pass

def digest(value):
    return hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False).encode()).hexdigest()

def file_hash(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as f:
        for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
    return h.hexdigest()

def immutable(path,value):
    value=json.loads(json.dumps(value,allow_nan=False))
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True)
    if path.exists():
        if json.loads(path.read_text())!=value:raise ContractError('IMMUTABLE_CONFLICT')
        return
    tmp=path.with_name('.'+path.name+'.'+uuid.uuid4().hex)
    try:
        with tmp.open('x') as f:
            json.dump(value,f,sort_keys=True,indent=2,allow_nan=False);f.write('\n');f.flush();os.fsync(f.fileno())
        os.link(tmp,path)
        fd=os.open(path.parent,os.O_RDONLY)
        try:os.fsync(fd)
        finally:os.close(fd)
    finally:
        tmp.unlink(missing_ok=True)

def selections(text,allowed):
    """all, none, explicit names, or all,-name. Only report rendering uses this."""
    active=set()
    for item in text.split(','):
        if item=='all':active.update(allowed)
        elif item=='none':active.clear()
        else:
            remove=item.startswith('-');name=item[1:] if remove else item.lstrip('+')
            if name not in allowed:raise argparse.ArgumentTypeError('unknown report selector: '+name)
            if remove:active.discard(name)
            else:active.add(name)
    return tuple(x for x in allowed if x in active)

@dataclass(frozen=True)
class RunConfig:
    duration: str|None=None
    capture_source: str='logs'

@dataclass(frozen=True)
class VerificationConfig:
    mode: str='off'
    rps: int=2
    max_calls: int=2000
    max_seconds: int=300
    max_bytes: int=256*1024*1024
    max_disk_bytes: int=8*1024**3
    max_ram_bytes: int=2*1024**3
    min_free_bytes: int=1024**3
    max_pages: int=50
    timeout: int=15
    attempts: int=3
    def validate(self):
        if self.mode not in ('off','index','strict','crosscheck'):raise ContractError('VERIFICATION_MODE')
        quantities=(self.rps,self.max_calls,self.max_seconds,self.max_bytes,self.max_ram_bytes,self.max_disk_bytes,self.min_free_bytes,self.max_pages,self.timeout,self.attempts)
        if any(type(n) is not int or n<1 or n>2**63-1 for n in quantities):
            raise ContractError('VERIFICATION_BUDGET_INVALID')
        return self

@dataclass(frozen=True)
class ReportQuery:
    types: tuple=KINDS
    markers: tuple=MARKERS
    related_events: bool=False
    related_types: tuple=RELATED
    view: str='summary'
    provenance: str='best'
    from_utc: str|None=None
    to_utc: str|None=None
    min_net_usd: float|None=None
    winners: bool=True
    competition: bool=True
    economics: bool=True
    format: str='table'

    def __post_init__(self):
        for value,allowed in ((self.types,KINDS),(self.markers,MARKERS),(self.related_types,RELATED)):
            if not isinstance(value,(list,tuple)) or any(x not in allowed for x in value) or len(set(value))!=len(value):
                raise ContractError('REPORT_SELECTION_INVALID')
        if self.view not in ('summary','liquidations','winners','competition','economics') or self.provenance not in ('best','live','index','strict','shadow','all') or self.format not in ('table','json','csv'):
            raise ContractError('REPORT_ENUM_INVALID')
        if any(type(v) is not bool for v in (self.related_events,self.winners,self.competition,self.economics)):
            raise ContractError('REPORT_BOOLEAN_INVALID')
        if self.min_net_usd is not None and (type(self.min_net_usd) not in (int,float) or not math.isfinite(self.min_net_usd)):
            raise ContractError('REPORT_NET_NONFINITE')
        from audit_market_quick import utc_ns
        if self.from_utc:utc_ns(self.from_utc)
        if self.to_utc:utc_ns(self.to_utc)
        if self.from_utc and self.to_utc and utc_ns(self.from_utc)>=utc_ns(self.to_utc):
            raise ContractError('REPORT_WINDOW_INVALID')

@dataclass(frozen=True)
class MeasurementConfigV2:
    run: RunConfig
    verification: VerificationConfig
    report: ReportQuery
    schema: str=SCHEMA


def parser(description='C0-E1 measurement/verification/report contract'):
    p=argparse.ArgumentParser(description=description,epilog="Operational options: --profile smoke|standard|performance|custom, --config FILE, --rpc-rps N, --rpc-concurrency N, --print-effective-config. Full typed schema: native/target/release/p0-longtail --help. Capture/verifier profiles never authorize LIVE.")
    p.add_argument('--run');p.add_argument('--duration')
    p.add_argument('--capture-source',choices=['logs','transactions'],default='logs')
    p.add_argument('--chain-verify','--verify','--mode',dest='mode',choices=['off','index','strict','crosscheck'],default='off')
    p.add_argument('--report-types',type=lambda s:selections(s,KINDS),default=KINDS)
    p.add_argument('--report-markers',type=lambda s:selections(s,MARKERS),default=MARKERS)
    p.add_argument('--report-related-events',choices=['on','off'],default='off')
    p.add_argument('--report-related-types',type=lambda s:selections(s,RELATED),default=RELATED)
    p.add_argument('--report-view',choices=['summary','liquidations','winners','competition','economics'],default='summary')
    p.add_argument('--report-provenance',choices=['best','live','index','strict','shadow','all'],default='best')
    p.add_argument('--report-from');p.add_argument('--report-to');p.add_argument('--report-min-net-usd',type=float)
    for flag in ('winners','competition','economics'):p.add_argument('--report-'+flag,choices=['on','off'],default='on')
    p.add_argument('--format',choices=['table','json','csv'],default='table')
    for name,default,aliases in (
        ('rps',2,['--rps']),('max-calls',2000,['--max-calls']),('max-seconds',300,['--max-seconds']),
        ('max-bytes',256*1024*1024,['--max-bytes']),('max-disk-bytes',8*1024**3,[]),
        ('max-ram-bytes',2*1024**3,[]),('min-free-bytes',1024**3,[])):
        p.add_argument('--verify-'+name,*aliases,dest=name.replace('-','_'),type=int,default=default)
    p.add_argument('--max-pages',type=int,default=50);p.add_argument('--timeout',type=int,default=15)
    p.add_argument('--attempts',type=int,default=3)
    p.add_argument('--resume-verification',action='store_true')
    p.add_argument('--verification-result',help='immutable verification_result_id for report reproducibility')
    p.add_argument('--provider-qualification',type=Path,help='measured FREE_ONLY quota/archive qualification JSON')
    p.add_argument('--offline',action='store_true');p.add_argument('--preflight',action='store_true')
    return p

def config(args):
    verification=VerificationConfig(**{f:getattr(args,f) for f in VerificationConfig.__dataclass_fields__}).validate()
    query=ReportQuery(args.report_types,args.report_markers,args.report_related_events=='on',args.report_related_types,
        args.report_view,args.report_provenance,args.report_from,args.report_to,args.report_min_net_usd,
        args.report_winners=='on',args.report_competition=='on',args.report_economics=='on',args.format)
    if query.min_net_usd is not None and not math.isfinite(query.min_net_usd):raise ContractError('REPORT_NET_NONFINITE')
    from audit_market_quick import utc_ns
    if query.from_utc:utc_ns(query.from_utc)
    if query.to_utc:utc_ns(query.to_utc)
    if query.from_utc and query.to_utc and utc_ns(query.from_utc)>=utc_ns(query.to_utc):raise ContractError('REPORT_WINDOW_INVALID')
    return MeasurementConfigV2(RunConfig(args.duration,args.capture_source),verification,query)


def capture_identity(run):
    manifest=run/'measurement_manifest.json'
    if manifest.exists():
        m=json.loads(manifest.read_text())
        return {'capture_run_id':m.get('capture_run_id',m['run_id']), 'manifest_sha256':file_hash(manifest),
                'provenance':'PRE_CAPTURE_MANIFEST' if m.get('capture_run_id') else 'LEGACY_MANIFEST_RUN_ID'}
    # Legacy import is a distinct immutable identity; never rewrite old manifests.
    p=run/'capture_legacy_identity.json'
    if p.exists():return json.loads(p.read_text())
    source=run/'market/summary.json'
    if not source.exists():source=run/'summary.json'
    v={'capture_run_id':'legacy-'+digest({'name':run.name,'summary':file_hash(source)}),
       'manifest_sha256':None,'provenance':'LEGACY_IMPORT_NOT_PRE_CAPTURE'}
    immutable(p,v);return v


def finalize_capture(run):
    """Streaming file index; never includes subsequently appended verifier/report outputs."""
    target=run/'capture_evidence_manifest.json'
    if target.exists():return json.loads(target.read_text())
    identity=capture_identity(run)
    summary=run/'market/summary.json'
    if not summary.exists():summary=run/'summary.json'
    if not summary.exists():raise ContractError('CAPTURE_NOT_FINALIZED')
    s=json.loads(summary.read_text())
    files={}
    for p in sorted(run.rglob('*')):
        rel=p.relative_to(run)
        if rel.parts[0] in ('verification','reports') or p.name.startswith('.') or not p.is_file():continue
        if p.name in ('capture_evidence_manifest.json','capture_legacy_identity.json'):continue
        before=p.stat();sha=file_hash(p);after=p.stat()
        if (before.st_size,before.st_mtime_ns)!=(after.st_size,after.st_mtime_ns):raise ContractError('CAPTURE_STILL_MUTATING')
        files[str(rel)]={'bytes':before.st_size,'sha256':sha}
    # A historical proof cannot upgrade this axis. Legacy full-slot recovery was
    # sometimes able to erase reasons, so require positive independent transport facts.
    live=s.get('live_coverage_status') or ('LIVE_COVERAGE_COMPLETE' if
        s.get('receivership_transport_capture_complete') is True else 'LIVE_COVERAGE_INCOMPLETE')
    v={'schema':'C0-E1/CaptureEvidence/2',**identity,'live_coverage_status':live,
       'capture_clock_complete':s.get('capture_clock_complete',s.get('full_duration_observed',False)),
       'files':files,'evidence_digest':digest(files)}
    immutable(target,v);return v


def check_capture(run,evidence,hashes=False):
    for name,info in evidence['files'].items():
        p=run/name
        if not p.is_file() or p.stat().st_size!=info['bytes'] or (hashes and file_hash(p)!=info['sha256']):
            raise ContractError('CAPTURE_EVIDENCE_CHANGED')


def endpoint():
    if os.environ.get('P0_OPERATIONAL_CONFIG_FILE'):
        data=json.loads(Path(os.environ['P0_OPERATIONAL_CONFIG_FILE']).read_text())
        if data.get('rpc_endpoints'):return data['rpc_endpoints'][0]
    url=os.environ.get('P0_AUDIT_RPC_URL') or os.environ.get('P0_RPC_URL')
    if not url and os.environ.get('HELIUS_API_KEY'):url='https://mainnet.helius-rpc.com/?api-key='+os.environ['HELIUS_API_KEY']
    return url


def preflight(run,v,qualification,url):
    v.validate();free=shutil.disk_usage(run).free
    q=json.loads(qualification.read_text()) if qualification else {}
    binding=hashlib.sha256((url or 'OFFLINE').encode()).hexdigest()
    reasons=[]
    if free<v.min_free_bytes+min(v.max_disk_bytes,64*1024**2):reasons.append('DISK_FREE_INSUFFICIENT')
    if v.mode!='off':
        if not url:reasons.append('RPC_ACCESS_MISSING')
        if (q.get('policy')!='FREE_ONLY' or q.get('provider_binding')!=binding
            or q.get('measured_at_unix_s',0)<time.time()-86400 or q.get('measured_at_unix_s',0)>time.time()
            or q.get('passed') is not True or not q.get('evidence_sha256')):reasons.append('FREE_ONLY_PROVIDER_QUALIFICATION_OPEN')
        if q.get('allowed_rps',0)<v.rps or q.get('available_calls',0)<v.max_calls:reasons.append('PROVIDER_QUOTA_INSUFFICIENT_OR_UNKNOWN')
        if v.mode=='strict' and q.get('archive_qualified') is not True:reasons.append('ARCHIVE_QUALIFICATION_OPEN')
    return {'passed':not reasons,'reasons':reasons,'free_disk_bytes':free,'requested':asdict(v),
            'allowed':{'qualified_rps':q.get('allowed_rps'),'inflight':1,'policy':'FREE_ONLY'},
            'effective_rps':v.rps,'provider_binding':binding,'qualification_digest':digest(q) if q else None}


def specification(run,v,provider_binding):
    from audit_market_quick import read_window
    identity=capture_identity(run)
    files=[Path(__file__),Path(__file__).with_name('audit_market_quick.py')]
    native=Path(__file__).resolve().parents[1]/'native/crates/darkbot-runner/src'
    if v.mode=='strict':
        files += [native/x for x in ('market_verification.rs','market_reconcile.rs','market_truth.rs','market_gap.rs')]
        binary=native.parents[2]/'target/release/p0-strict-verify'
        if binary.exists():files.append(binary)
    payload={'schema':'C0-E1/VerificationSpec/2','capture_run_id':identity['capture_run_id'],
             'capture_identity':identity,'window':read_window(run),'mode':v.mode,'protocol_pin':PROTOCOL,
             'decoder_pins':{p.name:file_hash(p) for p in files},'policy':{'name':'FREE_ONLY','provider_binding':provider_binding},
             'budgets':asdict(v)}
    return {**payload,'verification_spec_id':digest(payload)}
