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
# The 100 s measurement plan as pinned data (review 13): the archived reader's input does not pass through the
# current reader's parser; `plan_text` of the Rust text form must yield exactly this plan (asserted below).
PLAN_100S={'schema_version':1,'duration_seconds':100,
           'snapshot_offsets_ns':[25000000000,50000000000,75000000000,100000000000],
           'budgets':{'bootstrap_seconds':600,'stream_ready_seconds':600,'shadow_drain_seconds':180,'market_drain_seconds':120,
                      'quiescence_grace_seconds':150,'strict_seconds':14400,'gap_seconds':180,'enrichment_seconds':900,
                      'shutdown_grace_seconds':180}}


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
        native=REPO/'project_0/native';(project/'native').mkdir()
        for entry in native.iterdir():
            if entry.name!='target':(project/'native'/entry.name).symlink_to(entry)
        # Isolated test-local adapter (never production): the archived pinned reader of ARCHIVED expects the
        # plan as JSON on stdout; it gets the pinned plan PLAN_100S — independent of the current reader's parser —
        # and fails loudly for any other duration.
        release=project/'native/target/release';release.mkdir(parents=True)
        adapter=release/'p0-measurement-plan'
        adapter.write_text('#!'+sys.executable+'\n'
            'import json,os,sys\n'
            'if os.environ.get("P0_MEASUREMENT_DURATION")!="100s":sys.exit("ADAPTER_DURATION_NOT_PINNED")\n'
            f'print(json.dumps({PLAN_100S!r}))\n')
        adapter.chmod(0o755)
        return scripts

    def test_v1_evidence_window_records_and_snapshots_are_unchanged(self):
        if not shutil.which('git'):self.skipTest('git unavailable')
        legacy_plan=REPO/'project_0/native/target/release/p0-measurement-plan'
        with tempfile.TemporaryDirectory() as folder, offline():
            tmp=Path(folder);scripts=self.archived_project(tmp)
            probe=('import json,sys;from pathlib import Path;import measurement_contract as c,audit_market_quick as a,market_report as r\n'
                   'run=Path(sys.argv[1]);e=c.finalize_capture(run);d,m=r.normalize(run)\n'
                   'print(json.dumps({"window":a.read_window(run),"key":m["version"]}))')
            variants=[('manifest',lambda root:manifest_run(root,'r1','v1'))]
            if legacy_plan.exists():
                # The current parser yields exactly the pinned plan from the Rust authority's text form.
                text=subprocess.run([str(legacy_plan),'--historical-plan'],env={**clean_env(),'P0_MEASUREMENT_DURATION':'100s'},
                                    capture_output=True,text=True,timeout=60)
                self.assertEqual(text.returncode,0,text.stderr);self.assertEqual(r.plan_text(text.stdout),PLAN_100S)
                # A run without any plan record: the current reader takes the plan from the Rust authority's
                # isolated offline text path, the archived reader from it through the test-local adapter.
                def legacy(root):
                    run=manifest_run(root,'r1','v1');(run/'measurement_manifest.json').unlink();(run/'measurement_phases.jsonl').unlink()
                    (run/'market/summary.json').write_text(json.dumps({'observation_started_at':a.iso_ns(100*a.NS),'observation_ended_at':a.iso_ns(200*a.NS),
                        'configured_duration_seconds':100,'capture_clock_complete':True}))
                    return run
                variants.append(('legacy_identity',legacy))
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
