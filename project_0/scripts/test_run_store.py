"""Offline run storage contract regressions. Synthetic temporary directories only; no network."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import audit_market_quick as a
import measurement_contract as c
import run_store as s
from test_measurement_plan import clean_env

SCRIPTS=Path(__file__).resolve().parent
REPO=SCRIPTS.parents[1]


def legacy_run(root,name):
    run=root/name;(run/'market').mkdir(parents=True)
    (run/'market/summary.json').write_text(json.dumps({'capture_clock_complete':True,'live_coverage_status':'LIVE_COVERAGE_COMPLETE'}))
    (run/'market/coverage.jsonl').write_text('{"event":"DRAINED"}\n')
    (run/'hour_01.json').write_text('{"window":1}\n')
    return run


class RunRoot(unittest.TestCase):
    def test_legacy_default_runs_opt_in_and_invalid_store(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            self.assertEqual(s.run_root('econ4h',store='legacy',project=p),p/'data/econ4h')
            self.assertEqual(s.run_root('market',store='legacy',project=p),p/'data/market')
            self.assertEqual(s.run_root('econ4h',store='runs',project=p),p/'data/runs')
            self.assertEqual(s.run_root('market',store='runs',project=p),p/'data/runs')
            for store,kind in (('bogus','econ4h'),('runs','shadow'),('legacy','')):
                with self.assertRaisesRegex(s.RunStoreError,'RUN_STORE_INVALID'):s.run_root(kind,store=store,project=p)
            with patch.dict(os.environ,{'P0_RUN_STORE':'runs'}):
                self.assertEqual(s.run_root('econ4h',project=p),p/'data/runs')
            with patch.dict(os.environ,{'P0_RUN_STORE':''}):
                self.assertEqual(s.run_root('econ4h',project=p),p/'data/econ4h')

    def test_explicit_root_kept_unless_it_contradicts_runs_store(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);other=p/'volume/econ'
            self.assertEqual(s.run_root('econ4h',str(other),store='legacy',project=p),other)
            with self.assertRaisesRegex(s.RunStoreError,'RUN_ROOT_CONFLICT'):s.run_root('econ4h',str(other),store='runs',project=p)
            (p/'data/runs').mkdir(parents=True);(p/'alias').symlink_to(p/'data/runs')
            self.assertEqual(s.run_root('market',str(p/'alias'),store='runs',project=p),p/'alias')


class Allocate(unittest.TestCase):
    def test_creates_once_and_never_reuses_or_overwrites(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);root=p/'data/runs'
            run=s.allocate_run(root,'econ4h_20261015T000000Z',project=p)
            self.assertEqual(run,root/'econ4h_20261015T000000Z');self.assertTrue(run.is_dir())
            (run/'measurement_manifest.json').write_text('{"run_id":"x"}')
            with self.assertRaisesRegex(s.RunStoreError,'RUN_ID_COLLISION'):s.allocate_run(root,'econ4h_20261015T000000Z',project=p)
            self.assertEqual((run/'measurement_manifest.json').read_text(),'{"run_id":"x"}')

    def test_rejects_traversal_and_invalid_ids_without_creating_anything(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);root=p/'data/runs'
            for bad in ('','.','..','../x','a/b','/abs','a b','ü','x'*129,'run\n',None):
                with self.assertRaisesRegex(s.RunStoreError,'RUN_ID_INVALID'):s.allocate_run(root,bad,project=p)
            self.assertFalse(root.exists());self.assertEqual(sorted(x.name for x in p.iterdir()),[])
            self.assertTrue(s.allocate_run(root,'A-z_09-'+'x'*121,project=p).is_dir())

    def test_run_id_unique_across_all_run_roots(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            (p/'data/econ4h/shared').mkdir(parents=True)
            (p/'data/market').mkdir(parents=True);(p/'data/market/dangling').symlink_to(p/'missing')
            for name in ('shared','dangling'):
                with self.assertRaisesRegex(s.RunStoreError,'RUN_ID_COLLISION'):s.allocate_run(p/'data/runs',name,project=p)
            explicit=p/'volume';explicit.mkdir();(explicit/'taken').symlink_to(p/'missing')
            with self.assertRaisesRegex(s.RunStoreError,'RUN_ID_COLLISION'):s.allocate_run(explicit,'taken',project=p)
            self.assertFalse((p/'data/runs/shared').exists())

    def test_lock_serializes_cross_root_check_and_creation(self):
        import fcntl,threading
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);(p/'data').mkdir();result={}
            def allocate():
                try:result['path']=s.allocate_run(p/'data/runs','raced',project=p)
                except s.RunStoreError as exc:result['error']=str(exc)
            held=os.open(p/'data',os.O_RDONLY)
            try:
                fcntl.flock(held,fcntl.LOCK_EX)
                worker=threading.Thread(target=allocate);worker.start();worker.join(0.5)
                self.assertTrue(worker.is_alive());self.assertFalse((p/'data/runs/raced').exists())
                (p/'data/econ4h/raced').mkdir(parents=True)  # a concurrent run claims the id in another root
            finally:
                os.close(held)
            worker.join(5)
            self.assertEqual(result,{'error':'RUN_ID_COLLISION'});self.assertFalse((p/'data/runs/raced').exists())

    def test_cli_prints_paths_and_stable_error_codes(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)/'runs'
            ok=subprocess.run(['python3',str(SCRIPTS/'run_store.py'),'allocate',str(root),'cli_run'],capture_output=True,text=True)
            self.assertEqual((ok.returncode,ok.stdout.strip()),(0,str(root/'cli_run')))
            again=subprocess.run(['python3',str(SCRIPTS/'run_store.py'),'allocate',str(root),'cli_run'],capture_output=True,text=True)
            self.assertEqual(again.returncode,2);self.assertTrue(again.stderr.startswith('RUN_ID_COLLISION:'))
            check=subprocess.run(['python3',str(SCRIPTS/'run_store.py'),'check',str(root),'fresh'],capture_output=True,text=True)
            self.assertEqual((check.returncode,check.stdout.strip()),(0,str(root/'fresh')));self.assertFalse((root/'fresh').exists())
            check=subprocess.run(['python3',str(SCRIPTS/'run_store.py'),'check',str(root),'cli_run'],capture_output=True,text=True)
            self.assertEqual(check.returncode,2);self.assertTrue(check.stderr.startswith('RUN_ID_COLLISION:'))
            bad=subprocess.run(['python3',str(SCRIPTS/'run_store.py'),'root','econ4h'],env={**clean_env(),'P0_RUN_STORE':'bogus'},capture_output=True,text=True)
            self.assertEqual(bad.returncode,2);self.assertTrue(bad.stderr.startswith('RUN_STORE_INVALID:'))


class Identity(unittest.TestCase):
    def test_capture_and_evidence_identity_do_not_depend_on_the_root(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            old=legacy_run(p/'old/data/econ4h','econ4h_20261015T010203Z')
            new=legacy_run(p/'new/data/runs','econ4h_20261015T010203Z')
            self.assertEqual(c.capture_identity(old),c.capture_identity(new))
            self.assertTrue(c.capture_identity(old)['capture_run_id'].startswith('legacy-'))
            self.assertEqual(c.finalize_capture(old)['evidence_digest'],c.finalize_capture(new)['evidence_digest'])
            manifest={'run_id':'econ4h_x','capture_run_id':'capture-fixed-id'}
            for base in (p/'a/data/econ4h/run1',p/'b/data/runs/run1'):
                base.mkdir(parents=True);(base/'measurement_manifest.json').write_text(json.dumps(manifest))
            self.assertEqual(c.capture_identity(p/'a/data/econ4h/run1')['capture_run_id'],'capture-fixed-id')
            self.assertEqual(c.capture_identity(p/'a/data/econ4h/run1'),c.capture_identity(p/'b/data/runs/run1'))

    def test_existing_readers_accept_runs_store_paths(self):
        # verify.sh and report.sh resolve --run through find_run; an explicit path works for any root.
        with tempfile.TemporaryDirectory() as folder:
            run=legacy_run(Path(folder)/'data/runs','market_20261015T000000Z')
            self.assertEqual(a.find_run(str(run)),run.resolve())

    def test_historical_local_runs_still_resolve_by_id_read_only(self):
        roots=[REPO/'project_0'/r for r in ('data/econ4h','data/market') if (REPO/'project_0'/r).is_dir()]
        names=[x.name for root in roots for x in sorted(root.iterdir()) if x.is_dir()][:3]
        if not names:self.skipTest('no local historical runs on this machine')
        for name in names:
            self.assertTrue(a.find_run(name).is_dir())


class Launchers(unittest.TestCase):
    def environment(self,tmp):
        public=subprocess.check_output(['bash','-c','source "$1"; printf "%s" "$P0_EXPECTED_PUBKEY"','test',str(REPO/'project_0/config/wallet.public.env')],text=True)
        alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';n=0
        for char in public:n=n*58+alphabet.index(char)
        key=tmp/'dummy.json';key.write_text(json.dumps([0]*32+list(n.to_bytes(32,'big'))));key.chmod(0o600)
        return {**clean_env(),'P0_ENV_FILE':str(tmp/'absent.env'),'P0_RPC_URL':'http://127.0.0.1:1','P0_WS_URL':'ws://127.0.0.1:1',
                'P0_KEYPAIR_PATH':str(key),'P0_MAX_SLIPPAGE_BPS':'50','P0_MARKET_WS_URL':'ws://127.0.0.1:1'}

    def run_script(self,name,env):
        return subprocess.run(['bash',str(SCRIPTS/name)],env=env,capture_output=True,text=True,timeout=30,cwd=tempfile.gettempdir())

    def test_econ_launcher_fails_closed_before_build(self):
        with tempfile.TemporaryDirectory() as folder:
            tmp=Path(folder);env=self.environment(tmp);out=tmp/'out'
            cases=[({'P0_RUN_STORE':'runs','P0_ECON_OUT_DIR':str(out),'P0_ECON_RUN_ID':'r1'},'RUN_ROOT_CONFLICT'),
                   ({'P0_RUN_STORE':'bogus','P0_ECON_RUN_ID':'r1'},'RUN_STORE_INVALID'),
                   ({'P0_ECON_OUT_DIR':str(out),'P0_ECON_RUN_ID':'../escape'},'RUN_ID_INVALID')]
            for extra,code in cases:
                with self.subTest(code=code):
                    p=self.run_script('run_econ4h.sh',{**env,**extra})
                    self.assertEqual(p.returncode,2,p.stderr);self.assertIn(code,p.stderr);self.assertNotIn('C0-E1: build',p.stdout)
            self.assertFalse(out.exists());self.assertFalse((tmp/'escape').exists())
            (out/'taken').mkdir(parents=True);(out/'taken/keep.json').write_text('{}')
            p=self.run_script('run_econ4h.sh',{**env,'P0_ECON_OUT_DIR':str(out),'P0_ECON_RUN_ID':'taken'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr)
            self.assertEqual(sorted(x.name for x in (out/'taken').iterdir()),['keep.json'])

    def test_market_and_gap_launchers_resolve_through_the_same_contract(self):
        with tempfile.TemporaryDirectory() as folder:
            tmp=Path(folder);env=self.environment(tmp);out=tmp/'market'
            p=self.run_script('run_market_observer.sh',{**env,'P0_RUN_STORE':'runs','P0_MARKET_OUT_DIR':str(out)})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ROOT_CONFLICT',p.stderr);self.assertFalse(out.exists())
            (out/'m1').mkdir(parents=True)
            p=self.run_script('run_market_observer.sh',{**env,'P0_MARKET_OUT_DIR':str(out),'P0_MARKET_RUN_ID':'m1'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr);self.assertNotIn('Unfiltered Live Market Observer',p.stdout)
            p=self.run_script('run_gap_reconnect_smoke.sh',{**env,'P0_RUN_STORE':'bogus'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_STORE_INVALID',p.stderr)
            econ=tmp/'econ';(econ/'g1').mkdir(parents=True);(econ/'g1/keep.json').write_text('{}')
            p=self.run_script('run_gap_reconnect_smoke.sh',{**env,'P0_ECON_OUT_DIR':str(econ),'P0_ECON_RUN_ID':'g1'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr)
            self.assertNotIn('proxy',p.stdout+p.stderr);self.assertEqual([x.name for x in (econ/'g1').iterdir()],['keep.json'])

    def test_launcher_end_to_end_legacy_and_runs_store_synthetic(self):
        # Copy of the launcher in a temporary project, so the default roots are temporary too.
        plan=REPO/'project_0/native/target/release/p0-measurement-plan'
        if not plan.exists():self.skipTest('release p0-measurement-plan not built')
        with tempfile.TemporaryDirectory() as folder:
            tmp=Path(folder);project=tmp/'repo/project_0'
            (project/'scripts').mkdir(parents=True);(project/'config').mkdir()
            for name in ('run_econ4h.sh','run_store.py'):shutil.copy2(SCRIPTS/name,project/'scripts'/name)
            shutil.copy2(REPO/'project_0/config/wallet.public.env',project/'config/wallet.public.env')
            git=['git','-C',str(tmp/'repo'),'-c','user.name=t','-c','user.email=t@invalid','-c','core.hooksPath=/dev/null','-c','commit.gpgsign=false']
            subprocess.run(git+['init','-q'],check=True);subprocess.run(git+['commit','-q','--allow-empty','-m','fixture'],check=True)
            target=tmp/'target/release';target.mkdir(parents=True);(target/'p0-measurement-plan').symlink_to(plan.resolve())
            capture=target/'p0-longtail'
            capture.write_text('#!/usr/bin/env bash\nset -eu\ncp "$P0_MEASUREMENT_PLAN" "$P0_ECON_OUT_DIR/$P0_ECON_RUN_ID/consumed_plan.json"\n'
                               'printf "%s|%s" "$P0_ECON_OUT_DIR" "$P0_ECON_RUN_ID"\n')
            capture.chmod(0o700)
            mock=tmp/'mock';mock.mkdir();(mock/'cargo').write_text('#!/usr/bin/env bash\nexit 0\n');(mock/'cargo').chmod(0o700)
            (mock/'sitecustomize.py').write_text('import shutil\nshutil.disk_usage = lambda path: (type("Disk", (), {"free": 2**50}))()\n')
            env={**self.environment(tmp),'CARGO_TARGET_DIR':str(target.parent),'PATH':str(mock)+os.pathsep+os.environ['PATH'],'PYTHONPATH':str(mock)}
            launch=lambda extra:subprocess.run(['bash',str(project/'scripts/run_econ4h.sh'),'--duration','61s'],env={**env,**extra},
                                               capture_output=True,text=True,timeout=30,cwd=tmp)
            for store,run_id,root in (({},'old_run','data/econ4h'),({'P0_RUN_STORE':'runs'},'new_run','data/runs')):
                with self.subTest(store=store):
                    p=launch({**store,'P0_ECON_RUN_ID':run_id})
                    self.assertEqual(p.returncode,0,p.stderr)
                    run=project/root/run_id
                    self.assertIn(f'{run.parent}|{run_id}',p.stdout)
                    self.assertEqual(json.loads((run/'measurement_plan.json').read_text()),json.loads((run/'consumed_plan.json').read_text()))
                    self.assertTrue((run/'candidate_manifest.json').is_file() and (run/'storage_preflight.json').is_file())
                    self.assertEqual(a.find_run(str(run)),run.resolve())
            p=launch({'P0_RUN_STORE':'runs','P0_ECON_RUN_ID':'old_run'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr)
            self.assertFalse((project/'data/runs/old_run').exists())

    def test_changed_launchers_parse(self):
        for name in ('run_econ4h.sh','run_market_observer.sh','run_gap_reconnect_smoke.sh'):
            self.assertEqual(subprocess.run(['bash','-n',str(SCRIPTS/name)]).returncode,0,name)


class GitIgnore(unittest.TestCase):
    def test_runs_ignored_but_evidence_and_config_remain_eligible(self):
        if not shutil.which('git'):self.skipTest('git unavailable')
        def ignored(path):
            p=subprocess.run(['git','-C',str(REPO),'check-ignore','-q','--no-index','--',path])
            return p.returncode==0
        self.assertTrue(ignored('project_0/data/runs/econ4h_x/manifest.json'))
        self.assertTrue(ignored('project_0/data/runs/econ4h_x/capture/transactions.jsonl'))
        for path in ('project_0/docs/evidence/NEW_20261015/results.json','project_0/config/profiles/smoke.json',
                     'project_0/exports/report.json','project_0/data/.gitkeep'):
            self.assertFalse(ignored(path),path)


if __name__=='__main__':
    unittest.main(verbosity=2)
