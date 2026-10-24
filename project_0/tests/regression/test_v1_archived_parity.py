"""Regression: V1 output of the re-pinned readers equals the archived pinned readers (checkpoint p0-before-claude-iter15-20261015)."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from operations.verification import audit_market_quick as a
from operations.reporting import market_report as r
from operations.cli import measurement_contract as c
from p0_paths import clean_env
from p0_runs import manifest_run

REPO=p0_paths.REPO
ARCHIVED='908b14c'  # last pinned readers before layout V2


def offline():
    return patch.dict(os.environ,clean_env(),clear=True)


class ArchivedParity(unittest.TestCase):
    """V1 output of the re-pinned readers equals the archived pinned readers, apart from identity versions."""
    def archived_project(self,tmp):
        project=tmp/'project_0';scripts=project/'scripts';scripts.mkdir(parents=True)
        for name in ('measurement_contract.py','audit_market_quick.py','market_report.py','operational_config.py'):
            p=subprocess.run(['git','-C',str(REPO),'show',f'{ARCHIVED}:project_0/scripts/{name}'],capture_output=True)
            if p.returncode:self.skipTest(f'archived commit {ARCHIVED} unavailable')
            (scripts/name).write_bytes(p.stdout)
        (project/'native').symlink_to(REPO/'project_0/native')
        return scripts

    def test_v1_evidence_window_records_and_snapshots_are_unchanged(self):
        if not shutil.which('git'):self.skipTest('git unavailable')
        with tempfile.TemporaryDirectory() as folder, offline():
            tmp=Path(folder);scripts=self.archived_project(tmp)
            probe=('import json,sys;from pathlib import Path;import measurement_contract as c,audit_market_quick as a,market_report as r\n'
                   'run=Path(sys.argv[1]);e=c.finalize_capture(run);d,m=r.normalize(run)\n'
                   'print(json.dumps({"window":a.read_window(run),"key":m["version"]}))')
            # The former `legacy_identity` variant (a run without any plan record, planned through the JSON
            # output of p0-measurement-plan) is BLOCKED since that JSON handoff was removed (P0-PROD-CTRL-02).
            variants=[('manifest',lambda root:manifest_run(root,'r1','v1'))]
            for name,make in variants:
                with self.subTest(variant=name):
                    old,new=make(tmp/name/'old'),make(tmp/name/'new')
                    p=subprocess.run([sys.executable,'-c',probe,str(old)],cwd=scripts,capture_output=True,text=True,env={**clean_env(),'PYTHONPATH':str(scripts)},timeout=60)
                    self.assertEqual(p.returncode,0,p.stderr);archived=json.loads(p.stdout)
                    self.assertEqual(archived['key'],'C0-E1/NormalizedReport/2')
                    c.finalize_capture(new);dataset,manifest=r.normalize(new)
                    self.assertEqual(a.read_window(new),archived['window'])
                    for rel in ('capture_evidence_manifest.json','capture_legacy_identity.json'):
                        self.assertEqual((old/rel).exists(),(new/rel).exists(),rel)
                        if (old/rel).exists():self.assertEqual((old/rel).read_bytes(),(new/rel).read_bytes(),rel)
                    olddata=next((old/'reports').glob('dataset_*'))
                    for rel in ('all_records.jsonl','hour_01.json','hour_04.json','economics_final.json'):
                        self.assertEqual((olddata/rel).read_bytes(),(dataset/rel).read_bytes(),rel)
                    self.assertNotEqual(olddata.name,dataset.name)  # dataset identity is versioned (NormalizedReport/3)


class ConfigExamples(unittest.TestCase):
    """config/examples are secret-free and fail closed in the real Python preflight (Rust examples: runtime_config.rs)."""
    def test_provider_qualification_example_never_passes_and_examples_hold_no_secrets(self):
        examples=REPO/'project_0/config/examples'
        with tempfile.TemporaryDirectory() as folder, offline():
            pf=c.preflight(Path(folder),c.VerificationConfig(mode='index'),examples/'provider_qualification.example.json','https://rpc.invalid/')
            self.assertFalse(pf['passed']);self.assertIn('FREE_ONLY_PROVIDER_QUALIFICATION_OPEN',pf['reasons'])
        for path in examples.glob('*.json'):
            text=path.read_text().lower();json.loads(text)
            for secret in ('http://','https://','ws://','wss://','api-key','api_key','keypair','secret'):self.assertNotIn(secret,text,path.name)


if __name__=='__main__':
    unittest.main(verbosity=2)
