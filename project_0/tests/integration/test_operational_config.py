"""Offline regressions of the historical Python verifier transport (provider quota preflight, RPC retry and
budgets). No Rust bridge, no network. The operational config itself is native (runtime_config tests)."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import io
import json
from pathlib import Path
import tempfile
import time
import hashlib
import unittest
import urllib.error
from operations.cli import measurement_contract as c
from operations.verification import audit_market_quick as a

class OperationalTests(unittest.TestCase):
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
