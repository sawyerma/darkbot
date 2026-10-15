"""Private terminal bridge to the authoritative Rust operational config compiler.
No Python defaults or parameter schema: the Rust binary owns both.
"""
from contextlib import contextmanager
from pathlib import Path
import json
import os
import subprocess
import tempfile

@contextmanager
def resolve(arguments, env=None):
    environment=dict(os.environ if env is None else env)
    binary=Path(environment.get('P0_CONFIG_BIN',str(Path(__file__).resolve().parents[2]/'native/target/release/p0-longtail')))
    if not binary.is_file():raise ValueError('OPERATIONAL_CONFIG_BINARY_NOT_BUILT')
    with tempfile.TemporaryDirectory(prefix='p0-operational-') as directory:
        path=Path(directory)/'config.json'
        result=subprocess.run([str(binary),*arguments,'--export-environment',str(path)],env=environment,capture_output=True,text=True)
        if result.returncode:raise ValueError('OPERATIONAL_CONFIG_INVALID: '+result.stderr.strip())
        config=json.loads(path.read_text());effective=json.loads(result.stdout)
        environment['P0_OPERATIONAL_CONFIG_FILE']=str(path)
        # Existing Python read-only consumers use the exact compiled endpoints.
        if config['rpc_endpoints']:environment['P0_RPC_URL']=config['rpc_endpoints'][0]
        if config['wss_endpoints']:environment['P0_WS_URL']=config['wss_endpoints'][0]
        yield config,effective,environment

# Wire names bridge the existing MeasurementConfigV2; defaults/types remain Rust-owned.
VERIFICATION = {
    'rps':('verify_rps',1,('--verify-rps','--rps')),
    'max_calls':('verify_calls',1,('--verify-max-calls','--max-calls')),
    'max_seconds':('verify_seconds',1,('--verify-max-seconds','--max-seconds')),
    'max_bytes':('verify_mib',1048576,('--verify-max-bytes','--max-bytes')),
    'max_disk_bytes':('verify_disk_mib',1048576,('--verify-max-disk-bytes',)),
    'max_ram_bytes':('verify_ram_mib',1048576,('--verify-max-ram-bytes',)),
    'min_free_bytes':('verify_free_mib',1048576,('--verify-min-free-bytes',)),
    'max_pages':('verify_pages',1,('--max-pages',)),
    'timeout':('verify_timeout_seconds',1,('--timeout',)),
    'attempts':('rpc_attempts',1,('--attempts',)),
}

@contextmanager
def parsed(parser,argv):
    import sys
    argv=list(sys.argv[1:] if argv is None else argv)
    args,extra=parser.parse_known_args(argv)
    if not extra and not (os.environ.get('P0_PROFILE') or os.environ.get('P0_OPERATIONAL_CONFIG_FILE')):
        yield args,None,dict(os.environ);return
    flags={a.split('=',1)[0] for a in argv if a.startswith('--')}
    if args.duration:extra+=['--duration',args.duration]
    for field,(name,scale,aliases) in VERIFICATION.items():
        if flags.intersection(aliases):
            value=getattr(args,field)
            if value%scale:parser.error(f'{aliases[0]} with operational profiles requires whole MiB')
            extra+=['--'+name.replace('_','-'),str(value//scale)]
    if flags.intersection(('--chain-verify','--verify','--mode')):extra+=['--verification',args.mode]
    with resolve(extra) as (config,effective,environment):
        if "--print-effective-config" in flags:
            print(json.dumps(effective,indent=2));raise SystemExit(0)
        for field,(name,scale,_) in VERIFICATION.items():setattr(args,field,config['limits'][name]*scale)
        args.duration=config['duration'];args.mode=config['verification']
        old=dict(os.environ)
        try:
            os.environ.update(environment)
            yield args,effective,environment
        finally:
            os.environ.clear();os.environ.update(old)


def transport_options():
    """Values already validated/exported by Rust, no second defaults/schema."""
    path=os.environ.get('P0_OPERATIONAL_CONFIG_FILE')
    if not path:return {}
    config=json.loads(Path(path).read_text());limits=config['limits']
    return {'max_response_bytes':limits['max_response_mib']*1048576,
            'rps_ceiling':min(limits['rpc_rps'],limits['history_rps']),
            'calls_ceiling':limits['max_requests'],'bytes_ceiling':limits['max_total_mib']*1048576,
            'backoff_initial':limits['rpc_backoff_ms']/1000,
            'backoff_max':limits['rpc_backoff_max_ms']/1000,'adaptive':config['adaptive'],
            'fallback_urls':config['rpc_endpoints'] if config['fallback'] else ()}
