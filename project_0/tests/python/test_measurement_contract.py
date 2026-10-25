import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import copy
from dataclasses import replace
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from operations.cli import measurement_contract as c
from operations.verification import measurement_verify as v
from operations.verification import audit_market_quick as a
import p0_market as fixtures

class ContractTests(unittest.TestCase):
    setUp=fixtures.TempRunBase.setUp
    tearDown=fixtures.TempRunBase.tearDown
    write=fixtures.TempRunBase.write
    def test_operational_profile_is_refused_instead_of_silently_ignored(self):
        # Review 7 (P0-PROD-CTRL-02): the historical reader has no operational-config bridge; a configured profile
        # would be ignored (no ceilings, no fallback), so a network run with one is refused before any read.
        import contextlib, io, os
        for key in ('P0_PROFILE','P0_OPERATIONAL_CONFIG_FILE'):
            err=io.StringIO()
            with patch.dict(os.environ,{key:'smoke'}),contextlib.redirect_stderr(err),contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(v.main(['--run','historical_run','--mode','index']),2)
            self.assertIn('HISTORICAL_PROFILE_UNSUPPORTED',err.getvalue())

    def test_off_no_rpc_and_immutable_lifecycle(self):
        before={p.name:c.file_hash(p) for p in (self.run/'market').iterdir()}
        with patch.object(a.RPC,'call',side_effect=AssertionError('network forbidden')):
            one=v.verify(self.run,c.VerificationConfig(),offline=True)
            two=v.verify(self.run,c.VerificationConfig(),offline=True,resume=True)
        self.assertEqual(one['summary']['status'],'NOT_RUN')
        self.assertEqual(one['capture_run_id'],two['capture_run_id'])
        self.assertEqual(one['verification_spec_id'],two['verification_spec_id'])
        self.assertNotEqual(one['verification_attempt_id'],two['verification_attempt_id'])
        self.assertNotEqual(one['verification_result_id'],two['verification_result_id'])
        self.assertEqual(before,{p.name:c.file_hash(p) for p in (self.run/'market').iterdir()})
        self.assertEqual(one['live_coverage_status'],'LIVE_COVERAGE_INCOMPLETE')
        self.assertFalse(two['master_pass'])
        with self.assertRaisesRegex(c.ContractError,'SPEC_EXISTS'):v.verify(self.run,c.VerificationConfig(),offline=True)
        with self.assertRaisesRegex(c.ContractError,'IMMUTABLE_CONFLICT'):c.immutable(self.run/'capture_evidence_manifest.json',{})

    def test_preflight_no_qualification_never_calls_rpc(self):
        with patch.object(c,'endpoint',return_value='https://secret'),patch.object(a.RPC,'call',side_effect=AssertionError('network')):
            r=v.verify(self.run,c.VerificationConfig(mode='index'))
        self.assertEqual(r['summary']['reason'],'PREFLIGHT_FAILED')
        self.assertFalse(r['summary']['canonical_verified'])
        self.assertNotIn('https://secret',json.dumps(r))

    def test_spec_invariant_to_results_and_display_parameters(self):
        vc=c.VerificationConfig(mode='index')
        one=c.specification(self.run,vc,'opaque')
        (self.run/'reports').mkdir();(self.run/'reports/example.json').write_text('{}')
        two=c.specification(self.run,vc,'opaque')
        self.assertEqual(one,two)
        self.assertNotEqual(one['verification_spec_id'],c.specification(self.run,replace(vc,mode='strict'),'opaque')['verification_spec_id'])
        self.assertNotEqual(one['verification_spec_id'],c.specification(self.run,vc,'other-provider')['verification_spec_id'])

    def test_config_filters_and_limits(self):
        p=c.parser()
        cfg=c.config(p.parse_args(['--duration','61s','--verify','index','--report-types','all,-CLASSIC',
            '--report-markers','none','--report-related-events','on','--report-related-types','drift_claim_bad_debt']))
        self.assertEqual(cfg.run.duration,'61s')
        self.assertEqual(cfg.verification.mode,'index')
        self.assertNotIn('CLASSIC',cfg.report.types)
        self.assertEqual(cfg.report.markers,())
        self.assertEqual(cfg.report.related_types,('drift_claim_bad_debt',))
        for kwargs in ({'rps':0},{'max_calls':-1},{'max_seconds':0},{'max_ram_bytes':False}):
            with self.assertRaises(c.ContractError):c.VerificationConfig(**kwargs).validate()

    def test_capture_mutation_and_disk_fail_closed(self):
        ev=c.finalize_capture(self.run)
        self.write('market/transactions.jsonl',[{'changed':True}])
        with self.assertRaisesRegex(c.ContractError,'CAPTURE_EVIDENCE_CHANGED'):c.check_capture(self.run,ev)
        with patch.object(v.shutil,'disk_usage',return_value=type('D',(),{'free':0})()):
            with self.assertRaisesRegex(a.AuditError,'DISK_FREE'):v.resource_guard(self.run,c.VerificationConfig(),v.time.monotonic()+10)

if __name__=='__main__':unittest.main()
