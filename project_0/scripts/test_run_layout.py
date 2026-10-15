"""Run layout V1/V2 contract, versioned identities and archived-resume detection. Offline, synthetic temporary runs only."""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import audit_market_quick as a
import market_report as r
import measurement_contract as c
import measurement_verify as v
import run_store as s
from test_measurement_plan import clean_env

SCRIPTS=Path(__file__).resolve().parent
REPO=SCRIPTS.parents[1]
ARCHIVED='908b14c'  # checkpoint p0-before-claude-iter15-20261015: last pinned readers before layout V2
PLAN={'schema_version':1,'duration_seconds':100,'snapshot_offsets_ns':[25*a.NS,50*a.NS,75*a.NS,100*a.NS],'budgets':{}}
TX={'signature':'sig1','slot':20,'source':'logsSubscribe','success':True,'t_rx_ns':5,'raw':{'value':{'logs':[]}}}


def manifest_run(root,name,layout,run_id='r1'):
    """Identical synthetic capture content written in either layout (paths as in run_store.ARTIFACTS)."""
    run=root/name;p=lambda key:s.artifact(run,key,layout)
    p('market').mkdir(parents=True);p('phases').parent.mkdir(parents=True,exist_ok=True)
    manifest={'schema':'C0-E1/MeasurementManifest/2','run_id':run_id,'capture_run_id':run_id,'plan':PLAN}
    if layout=='v2':manifest.update(schema=s.MANIFEST_V2_SCHEMA,layout=s.LAYOUT_V2)
    p('manifest').write_text(json.dumps(manifest))
    p('phases').write_text(json.dumps({'phase':'CAPTURE','details':{'capture_start_unix_ns':100*a.NS,'capture_cutoff_unix_ns':200*a.NS}})+'\n')
    p('market_summary').write_text(json.dumps({'capture_clock_complete':True,'live_coverage_status':'LIVE_COVERAGE_COMPLETE','observation_ended_at':a.iso_ns(200*a.NS)}))
    (p('market')/'transactions.jsonl').write_text(json.dumps(TX)+'\n')
    for name in ('liquidations.jsonl','coverage.jsonl'):(p('market')/name).write_text('')
    return run


def offline():
    return patch.dict(os.environ,clean_env(),clear=True)


class ArtifactTable(unittest.TestCase):
    def test_literal_paths_match_rust_run_layout(self):
        expected={'manifest':('measurement_manifest.json','manifest.json'),'phases':('measurement_phases.jsonl','capture/measurement_phases.jsonl'),
                  'market':('market','capture/market'),'shadow_evaluations':('shadow_evaluations.jsonl','capture/shadow_evaluations.jsonl'),
                  'shadow_offsets':('shadow_offsets.bin','capture/shadow_offsets.bin'),'hour_01':('hour_01.json','snapshots/hour_01.json'),
                  'economics_final':('economics_final.json','snapshots/economics_final.json'),
                  'opportunity_economics':('opportunity_economics.jsonl','snapshots/opportunity_economics.jsonl'),
                  'canonical_market_shadow':('canonical_market_shadow.jsonl','snapshots/canonical_market_shadow.jsonl'),
                  'opportunities':('opportunities.jsonl','capture/opportunities.jsonl'),'build_log':('build.log','logs/build.log'),
                  'capture_evidence':('capture_evidence_manifest.json','verification/capture_evidence_manifest.json'),
                  'post_capture_result':('post_capture_result.json','reports/post_capture_result.json')}
        for name,(v1,v2) in expected.items():
            self.assertEqual((s.artifact(Path('/r'),name,'v1'),s.artifact(Path('/r'),name,'v2')),(Path('/r')/v1,Path('/r')/v2),name)
        for name,layout in (('config','v1'),('measurement_plan','v2'),('capture_legacy_identity','v2')):
            with self.assertRaisesRegex(s.RunStoreError,'RUN_ARTIFACT_UNDEFINED'):s.artifact(Path('/r'),name,layout)
        self.assertTrue(all(rel is None or rel=='' or rel.split('/')[0] in s.V2_ROOT for _,rel in s.ARTIFACTS.values()))


class StoreAndLayout(unittest.TestCase):
    def test_store_selects_layout_and_explicit_directories_keep_legacy_semantics(self):
        with tempfile.TemporaryDirectory() as folder, offline():
            p=Path(folder);runs=p/'data/runs';other=p/'elsewhere'
            self.assertEqual(s.resolve_store('econ4h',project=p),('runs',runs))
            self.assertEqual(s.run_layout('econ4h',project=p),'v2')
            self.assertEqual(s.resolve_store('market',project=p),('legacy',p/'data/market'))
            self.assertEqual(s.run_layout('econ4h',store='legacy',project=p),'v1')
            self.assertEqual(s.run_layout('econ4h',store='runs',project=p),'v2')
            self.assertEqual(s.run_layout('market',store='runs',project=p),'v1')
            self.assertEqual(s.resolve_store('econ4h',str(runs),project=p),('runs',runs))
            self.assertEqual(s.run_layout('econ4h',str(runs),project=p),'v2')
            self.assertEqual(s.resolve_store('econ4h',str(other),project=p),('legacy',other))
            self.assertEqual(s.run_layout('econ4h',str(other),project=p),'v1')
            with self.assertRaisesRegex(s.RunStoreError,'RUN_ROOT_CONFLICT'):s.run_layout('econ4h',str(other),store='runs',project=p)
            with patch.dict(os.environ,{'P0_RUN_STORE':'legacy'}):self.assertEqual(s.run_layout('econ4h',str(runs),project=p),'v1')

    def test_allocate_v2_creates_only_the_skeleton(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);run=s.allocate_run(p/'data/runs','new',project=p,layout='v2')
            self.assertEqual(sorted(x.name for x in run.iterdir()),['capture','logs','snapshots'])
            with self.assertRaisesRegex(s.RunStoreError,'RUN_LAYOUT_INVALID'):s.detect_layout(run)  # in progress: no manifest yet
            with self.assertRaisesRegex(s.RunStoreError,'RUN_STORE_INVALID'):s.allocate_run(p/'data/runs','other',project=p,layout='v3')
            self.assertFalse((p/'data/runs/other').exists())

    def test_detection_whitelist_and_unsupported_versions(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            self.assertEqual(s.detect_layout(manifest_run(p,'v1','v1')),'v1')
            run=manifest_run(p,'v2','v2');self.assertEqual(s.detect_layout(run),'v2')
            (run/'.DS_Store').write_text('');(run/'verification').mkdir();(run/'reports').mkdir()
            self.assertEqual(s.detect_layout(run),'v2')
            bad={'v1_name_in_v2':lambda r:(r/'hour_01.json').write_text('{}'),
                 'v1_dir_in_v2':lambda r:(r/'market').mkdir(),
                 'schema_2':lambda r:(r/'manifest.json').write_text(json.dumps({'layout':s.LAYOUT_V2,'schema':'C0-E1/MeasurementManifest/2'})),
                 'free_tag':lambda r:(r/'manifest.json').write_text(json.dumps({'layout':'v2','schema':s.MANIFEST_V2_SCHEMA}))}
            for name,damage in bad.items():
                with self.subTest(name=name):
                    run=manifest_run(p,name,'v2');damage(run)
                    with self.assertRaisesRegex(s.RunStoreError,'RUN_LAYOUT_INVALID'):s.detect_layout(run)
                    with self.assertRaisesRegex(a.AuditError,'RUN_LAYOUT_INVALID'):a.find_run(str(run))
            run=manifest_run(p,'future','v2');(run/'manifest.json').write_text(json.dumps({'layout':'Project0/RunLayout/3'}))
            with self.assertRaisesRegex(s.RunStoreError,'RUN_LAYOUT_UNSUPPORTED'):s.detect_layout(run)


class RunConfig(unittest.TestCase):
    def test_config_json_is_assembled_once_from_known_parts(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);run=s.allocate_run(p/'data/runs','r1',project=p,layout='v2');stage=p/'stage';stage.mkdir()
            for name in ('measurement_plan','storage_preflight'):(stage/(name+'.json')).write_text(json.dumps({'part':name}))
            with self.assertRaisesRegex(s.RunStoreError,'RUN_CONFIG_INVALID'):s.write_config(run,stage)  # candidate_manifest missing
            (stage/'candidate_manifest.json').write_text('{')
            with self.assertRaisesRegex(s.RunStoreError,'RUN_CONFIG_INVALID'):s.write_config(run,stage)
            (stage/'candidate_manifest.json').write_text('{"part":"candidate_manifest"}');(stage/'notes.txt').write_text('x')
            with self.assertRaisesRegex(s.RunStoreError,'RUN_CONFIG_INVALID'):s.write_config(run,stage)
            (stage/'notes.txt').unlink()
            config=json.loads(s.write_config(run,stage).read_text())
            self.assertEqual((config['schema'],config['layout'],config['run_id']),(s.RUN_CONFIG_SCHEMA,s.LAYOUT_V2,'r1'))
            self.assertEqual(config['measurement_plan'],{'part':'measurement_plan'});self.assertIsNone(config['measurement_config'])
            self.assertEqual(sorted(k for k in config if k not in ('schema','layout','run_id')),sorted(s.CONFIG_PARTS))
            with self.assertRaisesRegex(s.RunStoreError,'RUN_CONFIG_INVALID'):s.write_config(run,stage)
            self.assertEqual(sorted(x.name for x in run.iterdir()),['capture','config.json','logs','snapshots'])


class Identities(unittest.TestCase):
    def test_v1_and_v2_share_capture_identity_and_records_but_not_reader_identities(self):
        with tempfile.TemporaryDirectory() as folder, offline():
            p=Path(folder);out={}
            for layout in ('v1','v2'):
                run=manifest_run(p/layout,'r1',layout)
                evidence=c.finalize_capture(run);spec=c.specification(run,c.VerificationConfig(),'binding')
                dataset,manifest=r.normalize(run)
                out[layout]=dict(run=run,evidence=evidence,spec=spec,manifest=manifest,records=(dataset/'all_records.jsonl').read_bytes(),
                                 snapshot=json.loads((dataset/'hour_04.json').read_text()))
            v1,v2=out['v1'],out['v2']
            self.assertEqual(v1['evidence']['capture_run_id'],v2['evidence']['capture_run_id'])
            self.assertEqual((v1['evidence']['schema'],v2['evidence']['schema']),('C0-E1/CaptureEvidence/2','C0-E1/CaptureEvidence/3'))
            self.assertTrue((v1['run']/'capture_evidence_manifest.json').is_file())
            self.assertTrue((v2['run']/'verification/capture_evidence_manifest.json').is_file())
            self.assertEqual(v2['evidence']['layout'],s.LAYOUT_V2)
            for x,tag in ((v1,s.LAYOUT_V1),(v2,s.LAYOUT_V2)):
                self.assertEqual((x['spec']['schema'],x['spec']['layout']),(c.SPEC_SCHEMA,tag))
                self.assertIn('run_store.py',x['spec']['decoder_pins'])
                self.assertEqual((x['manifest']['version'],x['manifest']['layout']),(r.DATASET_VERSION,tag))
                self.assertEqual(x['manifest']['layout_sha256'],c.file_hash(SCRIPTS/'run_store.py'))
            self.assertNotEqual(v1['spec']['verification_spec_id'],v2['spec']['verification_spec_id'])
            self.assertEqual(v1['records'],v2['records']);self.assertNotEqual(v1['records'],b'')
            self.assertEqual(json.loads(v1['records'].splitlines()[0])['schema'],r.VERSION)
            self.assertEqual(v1['snapshot'],v2['snapshot'])

    def test_v2_needs_its_manifest_and_market_streams(self):
        with tempfile.TemporaryDirectory() as folder, offline():
            run=manifest_run(Path(folder),'r1','v2')
            (run/'capture/market/liquidations.jsonl').unlink()
            audit=a.Audit(run,None)
            try:self.assertEqual(audit.execute()['reason'],'RUN_ARTIFACT_MISSING')
            finally:audit.close()
            m=json.loads((run/'manifest.json').read_text());del m['capture_run_id'];(run/'manifest.json').write_text(json.dumps(m))
            with self.assertRaisesRegex(c.ContractError,'CAPTURE_MANIFEST_INVALID'):c.capture_identity(run)


class Resume(unittest.TestCase):
    def test_archived_spec_is_never_resumed_with_the_current_implementation(self):
        with tempfile.TemporaryDirectory() as folder, offline():
            run=manifest_run(Path(folder),'r1','v1');cfg=c.VerificationConfig()
            old=dict(c.specification(run,cfg,'x'),schema='C0-E1/VerificationSpec/2');del old['layout']
            (run/'verification'/('a'*64)).mkdir(parents=True);(run/'verification'/('a'*64)/'spec.json').write_text(json.dumps(old))
            with self.assertRaisesRegex(c.ContractError,'RESUME_REQUIRES_ARCHIVED_IMPLEMENTATION: '+'a'*64+r' \(C0-E1/VerificationSpec/2\)'):
                v.verify(run,cfg,resume=True)
            self.assertEqual([x.name for x in (run/'verification').iterdir() if x.is_dir()],['a'*64])
            first=v.verify(run,cfg)  # a new verification under the current spec is allowed
            self.assertEqual(v.verify(run,cfg,resume=True)['verification_spec_id'],first['verification_spec_id'])

    def test_index_checkpoint_of_another_implementation_fails_closed(self):
        with tempfile.TemporaryDirectory() as folder, offline():
            run=manifest_run(Path(folder),'r1','v2');a.Audit(run,None).close()
            db=run/'verification/index_v3/checkpoint.sqlite3'
            def stored(**change):
                with sqlite3.connect(db) as conn:
                    identity=json.loads(conn.execute("SELECT value FROM state WHERE key='identity'").fetchone()[0])
                    identity.update(change)
                    conn.execute("UPDATE state SET value=? WHERE key='identity'",(json.dumps(identity),))
                return identity
            self.assertEqual(stored()['schema'],a.CHECKPOINT)
            original=stored()
            for change,code in (({'schema':'C0-E1/INDEX/2'},'RESUME_REQUIRES_ARCHIVED_IMPLEMENTATION'),
                                ({'decoder_sha256':'0'*64},'RESUME_REQUIRES_ARCHIVED_IMPLEMENTATION'),
                                ({'window':{'start_ns':1}},'RESUME_IDENTITY_MISMATCH')):
                with self.subTest(change=change):
                    stored(**{**original,**change})
                    with self.assertRaisesRegex(a.AuditError,code):a.Audit(run,None)
            stored(**original);a.Audit(run,None).close()


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
        legacy_plan=REPO/'project_0/native/target/release/p0-measurement-plan'
        with tempfile.TemporaryDirectory() as folder, offline():
            tmp=Path(folder);scripts=self.archived_project(tmp)
            probe=('import json,sys;from pathlib import Path;import measurement_contract as c,audit_market_quick as a,market_report as r\n'
                   'run=Path(sys.argv[1]);e=c.finalize_capture(run);d,m=r.normalize(run)\n'
                   'print(json.dumps({"window":a.read_window(run),"key":m["version"]}))')
            variants=[('manifest',lambda root:manifest_run(root,'r1','v1'))]
            if legacy_plan.exists():
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


class SelfTest(unittest.TestCase):
    def test_self_test_without_tests_fails_closed(self):
        with tempfile.TemporaryDirectory() as folder:
            scripts=Path(folder)/'project_0/scripts';scripts.mkdir(parents=True)
            for name in ('audit_market_quick.py','run_store.py'):shutil.copy2(SCRIPTS/name,scripts/name)
            call=lambda:subprocess.run([sys.executable,str(scripts/'audit_market_quick.py'),'--self-test'],capture_output=True,text=True,timeout=60,env=clean_env())
            p=call();self.assertEqual(p.returncode,2);self.assertIn('SELF_TEST_NOT_FOUND',p.stderr)
            (scripts/'test_audit_market_quick.py').write_text('')
            self.assertEqual(call().returncode,2)


if __name__=='__main__':
    unittest.main(verbosity=2)
