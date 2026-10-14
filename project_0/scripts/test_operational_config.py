"""Offline operational CLI/bridge regressions. Release binary required; no network."""
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import hashlib
import unittest
import urllib.error
from unittest.mock import patch
import operational_config as o
import measurement_contract as c
import audit_market_quick as a
from test_measurement_plan import clean_env

class OperationalTests(unittest.TestCase):
    def test_compiled_profiles_inspection_and_plan_share_boundaries(self):
        plan=Path(__file__).resolve().parents[1]/'native/target/release/p0-measurement-plan'
        for duration in ['10m','1h','5h','24h','61s','25h']:
            with o.resolve(['--profile','custom','--duration',duration,'--rpc-rps','25','--bootstrap-seconds','20'],clean_env()) as (config,report,env):
                proc=subprocess.run([str(plan)],env=env,capture_output=True,text=True,check=True)
                p=json.loads(proc.stdout)
                self.assertEqual(p['snapshot_offsets_ns'],[p['duration_seconds']*10**9*i//4 for i in range(1,5)])
                self.assertEqual(p['budgets']['bootstrap_seconds'],20)
                self.assertEqual(report['values']['limits']['rpc_rps'],25)
                self.assertFalse(report['submission_authorized'])
                self.assertEqual(config['duration'],duration)

    def test_cli_precedence_no_secret_logging_and_private_cleanup(self):
        with o.resolve(['--rpc-rps','17','--rpc-endpoints','https://secret.invalid/PRIVATE'],{**clean_env(),'P0_RPC_RPS':'3'}) as (config,report,env):
            self.assertEqual(config['limits']['rpc_rps'],17)
            self.assertNotIn('PRIVATE',json.dumps(report))
            private=Path(env['P0_OPERATIONAL_CONFIG_FILE'])
            self.assertEqual(private.stat().st_mode & 0o777,0o600)
        self.assertFalse(private.exists())

    def test_wrapper_print_has_no_capture_or_verification_side_effect(self):
        with patch.dict(os.environ,clean_env(),clear=True),patch('sys.stdout',new_callable=io.StringIO) as out:
            with self.assertRaises(SystemExit) as exc:
                with o.parsed(c.parser(),['--profile','smoke','--print-effective-config']):self.fail('started workflow')
            self.assertEqual(exc.exception.code,0)
            self.assertEqual(json.loads(out.getvalue())['values']['profile'],'smoke')

    def test_shared_legacy_verification_flags_compile_to_same_contract(self):
        with patch.dict(os.environ,clean_env(),clear=True):
            with o.parsed(c.parser(),['--profile','custom','--duration','61s','--verify-rps','12','--verify','index']) as (args,effective,env):
                self.assertEqual(args.rps,12);self.assertEqual(args.mode,'index')
                self.assertEqual(effective['values']['limits']['verify_rps'],12)
                self.assertEqual(c.config(args).run.duration,'61s')
                self.assertEqual(o.transport_options()['rps_ceiling'],2)

    def test_actual_requested_quota_must_be_qualified(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);url='https://fixture.invalid'
            q=root/'q.json';q.write_text(json.dumps({'policy':'FREE_ONLY','provider_binding':hashlib.sha256(url.encode()).hexdigest(),
                'measured_at_unix_s':time.time()-1,'passed':True,'evidence_sha256':'fixture','allowed_rps':2,'available_calls':2000}))
            pf=c.preflight(root,c.VerificationConfig(mode='index',rps=8),q,url)
            self.assertFalse(pf['passed']);self.assertEqual(pf['effective_rps'],8)
            self.assertIn('PROVIDER_QUOTA_INSUFFICIENT_OR_UNKNOWN',pf['reasons'])

    def test_rpc_retry_after_adaptive_fallback_and_total_payload_budget(self):
        now=[0.0];seen=[]
        def sleep(n):now[0]+=n
        def opener(req,timeout):
            seen.append(req.full_url)
            if len(seen)==1:raise urllib.error.HTTPError(req.full_url,429,'limited',{'Retry-After':'2'},None)
            return io.BytesIO(b'{"result":42}')
        rpc=a.RPC('http://primary.invalid',rps=20,opener=opener,sleep=sleep,clock=lambda:now[0],adaptive=True,
            fallback_urls=['http://primary.invalid','http://backup.invalid'])
        self.assertEqual(rpc.call('getTransaction',[]),42)
        self.assertEqual(seen,['http://primary.invalid','http://backup.invalid'])
        self.assertGreaterEqual(now[0],2);self.assertEqual(rpc.effective_rps,10)
        for _ in range(8):rpc.call('getTransaction',[])
        self.assertGreater(rpc.effective_rps,10)
        with self.assertRaisesRegex(a.AuditError,'RPC_BYTE_BUDGET'):
            a.RPC('http://fixture.invalid',max_bytes=1,opener=lambda *a,**k:self.fail('budget exhausted but sent')).call('getTransaction',[])
        with self.assertRaisesRegex(a.AuditError,'INDEX_METHOD_FORBIDDEN'):rpc.call('sendTransaction',[])

if __name__=='__main__':unittest.main()
