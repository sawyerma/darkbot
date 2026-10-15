import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0/scripts on sys.path
import base64
import unittest
import io,json
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from unittest.mock import patch, Mock
from pathlib import Path
from tempfile import TemporaryDirectory
import audit_market_quick as a
import fe_readonly_preflight as p

class Preflight(unittest.TestCase):
    def test_full_dispatch_preserves_private_rust_config_and_never_smoke_probes(self):
        config={'fe_mode':'full','limits':{'fe_seconds':37,'shutdown_seconds':9}}
        env={'P0_OPERATIONAL_CONFIG_FILE':'/private/validated-rust-config.json'}
        @contextmanager
        def resolve(args):
            self.assertEqual(args,['--fe-mode','full','--fe-calls','57'])
            yield config,{'live_authorized':False},env
        with TemporaryDirectory() as d, patch.object(p.operational_config,'resolve',resolve), \
                patch.object(Path,'is_file',return_value=True), patch.object(p.subprocess,'run',return_value=Mock(returncode=2)) as run, \
                patch.object(p,'probe',side_effect=AssertionError('smoke must not run')):
            output=str(Path(d)/'new')
            self.assertEqual(p.main(['--execute','--output',output,'--provider-qualification','proof.json','--fe-mode','full','--fe-calls','57']),2)
            self.assertEqual(run.call_args.kwargs,{'env':env,'timeout':46})
            self.assertIn('proof.json',run.call_args.args[0])
            self.assertNotIn('--fe-calls',run.call_args.args[0]) # already in private config

    def test_full_needs_measured_provider_evidence_before_execution(self):
        @contextmanager
        def resolve(args):yield {'fe_mode':'full'}, {}, {}
        with TemporaryDirectory() as d, patch.object(p.operational_config,'resolve',resolve), \
                patch.object(p.subprocess,'run') as run, redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):p.main(['--execute','--output',str(Path(d)/'new'),'--fe-mode','full'])
            run.assert_not_called()

    def test_inspection_needs_no_permission_output_or_network(self):
        @contextmanager
        def resolve(args):yield {}, {'mode':'full','submission_enabled':False}, {}
        with patch.object(p.operational_config,'resolve',resolve), patch.object(p,'probe') as probe, \
                patch.object(p.subprocess,'run') as run, redirect_stdout(io.StringIO()) as text:
            self.assertEqual(p.main(['--fe-mode','full','--print-effective-config']),0)
            self.assertFalse(json.loads(text.getvalue())['submission_enabled'])
            probe.assert_not_called();run.assert_not_called()

    def test_three_read_calls_no_send_or_simulation_and_no_parity_claim(self):
        class RPC:
            def __init__(self):self.calls=[]
            def call(self,method,params):
                self.calls.append(method)
                if len(self.calls)==1:
                    raw=(2).to_bytes(4,'little')+bytes([7])*32;owner=p.LOADER;slot=10
                elif len(self.calls)==2:
                    raw=(3).to_bytes(4,'little')+(9).to_bytes(8,'little')+bytes([0])*33;owner=p.LOADER;slot=11
                else:
                    return {'context':{'slot':12},'value':[None,None]}
                return {'context':{'slot':slot},'value':{'owner':owner,'executable':True,'data':[base64.b64encode(raw).decode(),'base64']}}
        rpc=RPC();result=p.probe(rpc)
        self.assertEqual(rpc.calls,['getAccountInfo','getAccountInfo','getMultipleAccounts'])
        self.assertEqual(result['deployment_slot'],9);self.assertEqual(result['source_parity'],'UNKNOWN')
        self.assertFalse(result['pilot_ready']);self.assertFalse(result['submission_enabled'])
    def test_owner_encoding_and_loader_mismatch_fail_closed(self):
        for value in [None,{'owner':'wrong'}, {'owner':p.LOADER,'data':['','jsonParsed']}, {'owner':p.LOADER,'data':['AAAAAA==','base64']}]:
            with self.assertRaises(a.AuditError):p.account(value,p.LOADER,2)

    def test_real_transport_allowlist_stays_readonly_and_default_index_only(self):
        body=lambda req,timeout:io.BytesIO(json.dumps({'result':{'context':{'slot':1},'value':None}}).encode())
        rpc=a.RPC('http://fixture.invalid',opener=body,sleep=lambda _:None,allowed_methods={'getAccountInfo','getMultipleAccounts'})
        self.assertIsNone(rpc.call('getAccountInfo',[])['value'])
        with self.assertRaises(a.AuditError):rpc.call('sendTransaction',[])
        with self.assertRaises(a.AuditError):a.RPC('http://fixture.invalid',allowed_methods={'sendTransaction'})
        default=a.RPC('http://fixture.invalid',opener=body)
        with self.assertRaises(a.AuditError):default.call('getAccountInfo',[])
