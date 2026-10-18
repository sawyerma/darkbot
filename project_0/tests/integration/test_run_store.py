"""Offline run storage contract regressions. Synthetic temporary directories only; no network."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from operations.verification import audit_market_quick as a
from operations.cli import measurement_contract as c
from operations.capture import run_store as s
from p0_paths import clean_env
from p0_runs import legacy_run,manifest_run,v2_run

SCRIPTS=p0_paths.SCRIPTS
RUN_STORE=p0_paths.OPERATIONS/'capture/run_store.py'
REPO=p0_paths.REPO


class RunRoot(unittest.TestCase):
    def test_runs_default_for_econ4h_legacy_opt_out_and_invalid_store(self):
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
                self.assertEqual(s.run_root('econ4h',project=p),p/'data/runs')
                self.assertEqual(s.run_root('market',project=p),p/'data/market')
            with patch.dict(os.environ,{'P0_RUN_STORE':'legacy'}):
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
            ok=subprocess.run(['python3',str(RUN_STORE),'allocate',str(root),'cli_run'],capture_output=True,text=True)
            self.assertEqual((ok.returncode,ok.stdout.strip()),(0,str(root/'cli_run')))
            again=subprocess.run(['python3',str(RUN_STORE),'allocate',str(root),'cli_run'],capture_output=True,text=True)
            self.assertEqual(again.returncode,2);self.assertTrue(again.stderr.startswith('RUN_ID_COLLISION:'))
            check=subprocess.run(['python3',str(RUN_STORE),'check',str(root),'fresh'],capture_output=True,text=True)
            self.assertEqual((check.returncode,check.stdout.strip()),(0,str(root/'fresh')));self.assertFalse((root/'fresh').exists())
            check=subprocess.run(['python3',str(RUN_STORE),'check',str(root),'cli_run'],capture_output=True,text=True)
            self.assertEqual(check.returncode,2);self.assertTrue(check.stderr.startswith('RUN_ID_COLLISION:'))
            bad=subprocess.run(['python3',str(RUN_STORE),'root','econ4h'],env={**clean_env(),'P0_RUN_STORE':'bogus'},capture_output=True,text=True)
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

class Launchers(unittest.TestCase):
    def environment(self,tmp):
        public=subprocess.check_output(['bash','-c','source "$1"; printf "%s" "$P0_EXPECTED_PUBKEY"','test',str(REPO/'project_0/config/wallet.public.env')],text=True)
        alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';n=0
        for char in public:n=n*58+alphabet.index(char)
        key=tmp/'dummy.json';key.write_text(json.dumps([0]*32+list(n.to_bytes(32,'big'))));key.chmod(0o600)
        # P0_CTL_LAUNCH_RUN_ID stands in for `p0ctl run start`, the only starter of econ4h-launcher.
        return {**clean_env(),'P0_ENV_FILE':str(tmp/'absent.env'),'P0_RPC_URL':'http://127.0.0.1:1','P0_WS_URL':'ws://127.0.0.1:1',
                'P0_KEYPAIR_PATH':str(key),'P0_MAX_SLIPPAGE_BPS':'50','P0_MARKET_WS_URL':'ws://127.0.0.1:1','P0_CTL_LAUNCH_RUN_ID':'test'}

    def run_script(self,command,env):
        name,*args=command.split()
        return subprocess.run(['bash',str(SCRIPTS/name),*args],env=env,capture_output=True,text=True,timeout=30,cwd=tempfile.gettempdir())

    def test_econ_launcher_fails_closed_before_build(self):
        with tempfile.TemporaryDirectory() as folder:
            tmp=Path(folder);env=self.environment(tmp);out=tmp/'out'
            direct={k:v for k,v in env.items() if k!='P0_CTL_LAUNCH_RUN_ID'}
            p=self.run_script('capture.sh econ4h-launcher --duration 61s',{**direct,'P0_ECON_OUT_DIR':str(out),'P0_ECON_RUN_ID':'r0'})
            self.assertEqual(p.returncode,2);self.assertIn('LAUNCHER_REQUIRES_P0CTL',p.stderr);self.assertFalse(out.exists())
            cases=[({'P0_RUN_STORE':'runs','P0_ECON_OUT_DIR':str(out),'P0_ECON_RUN_ID':'r1'},'RUN_ROOT_CONFLICT'),
                   ({'P0_RUN_STORE':'bogus','P0_ECON_RUN_ID':'r1'},'RUN_STORE_INVALID'),
                   ({'P0_ECON_OUT_DIR':str(out),'P0_ECON_RUN_ID':'../escape'},'RUN_ID_INVALID')]
            for extra,code in cases:
                with self.subTest(code=code):
                    p=self.run_script('capture.sh econ4h-launcher --duration 61s',{**env,**extra})
                    self.assertEqual(p.returncode,2,p.stderr);self.assertIn(code,p.stderr);self.assertNotIn('C0-E1: build',p.stdout)
            self.assertFalse(out.exists());self.assertFalse((tmp/'escape').exists())
            (out/'taken').mkdir(parents=True);(out/'taken/keep.json').write_text('{}')
            p=self.run_script('capture.sh econ4h-launcher --duration 61s',{**env,'P0_ECON_OUT_DIR':str(out),'P0_ECON_RUN_ID':'taken'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr)
            self.assertEqual(sorted(x.name for x in (out/'taken').iterdir()),['keep.json'])

    def test_market_and_gap_launchers_resolve_through_the_same_contract(self):
        with tempfile.TemporaryDirectory() as folder:
            tmp=Path(folder);env=self.environment(tmp);out=tmp/'market'
            p=self.run_script('capture.sh market',{**env,'P0_RUN_STORE':'runs','P0_MARKET_OUT_DIR':str(out)})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ROOT_CONFLICT',p.stderr);self.assertFalse(out.exists())
            (out/'m1').mkdir(parents=True)
            p=self.run_script('capture.sh market',{**env,'P0_MARKET_OUT_DIR':str(out),'P0_MARKET_RUN_ID':'m1'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr);self.assertNotIn('Unfiltered Live Market Observer',p.stdout)
            p=self.run_script('smoke.sh',{**env,'P0_RUN_STORE':'bogus'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_STORE_INVALID',p.stderr)
            econ=tmp/'econ';(econ/'g1').mkdir(parents=True);(econ/'g1/keep.json').write_text('{}')
            p=self.run_script('smoke.sh',{**env,'P0_ECON_OUT_DIR':str(econ),'P0_ECON_RUN_ID':'g1'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr)
            self.assertNotIn('proxy',p.stdout+p.stderr);self.assertEqual([x.name for x in (econ/'g1').iterdir()],['keep.json'])

    def real_project(self,tmp,operations=False):
        """Temporary repository with the real launcher; native/ is the real workspace (symlink) and CARGO_TARGET_DIR the
        real target, so real cargo reuses the release build. Runs land in this temporary project, never in data/."""
        release=REPO/'project_0/native/target/release'
        if not all((release/b).exists() for b in ('p0-longtail','p0-measurement-plan')):self.skipTest('NOT_RUN: release binaries not built')
        repo=tmp/'repo';project=repo/'project_0';(project/'scripts').mkdir(parents=True);(project/'config').mkdir()
        for name in ('capture.sh','verify.sh','report.sh'):shutil.copy2(SCRIPTS/name,project/'scripts'/name)
        shutil.copy2(REPO/'project_0/config/wallet.public.env',project/'config/wallet.public.env')
        (project/'native').symlink_to(REPO/'project_0/native')
        if operations:shutil.copytree(p0_paths.OPERATIONS,project/'operations',ignore=shutil.ignore_patterns('__pycache__'))
        git=['git','-C',str(repo),'-c','user.name=t','-c','user.email=t@invalid','-c','core.hooksPath=/dev/null','-c','commit.gpgsign=false']
        subprocess.run(git+['init','-q'],check=True);subprocess.run(git+['commit','-q','--allow-empty','-m','fixture'],check=True)
        return project

    def real_env(self,tmp):
        # Real cargo and the real p0-longtail; endpoints are closed local ports, so nothing leaves the machine and the
        # binary fails closed at startup. The key file holds only the public half; the real signer refuses to sign.
        return {**self.environment(tmp),'CARGO_TARGET_DIR':str(REPO/'project_0/native/target'),'TMPDIR':str(tmp/'tmpdir')}

    def test_launcher_end_to_end_real_binary_writes_layout_v1_and_v2(self):
        with tempfile.TemporaryDirectory() as folder:
            tmp=Path(folder);project=self.real_project(tmp,operations=True);(tmp/'tmpdir').mkdir()
            if shutil.disk_usage(tmp).free<8*2**30:self.skipTest('NOT_RUN: launcher storage preflight needs 8 GiB free')
            env=self.real_env(tmp)
            launch=lambda extra:subprocess.run(['bash',str(project/'scripts/capture.sh'),'econ4h-launcher','--duration','61s'],env={**env,**extra},
                                               capture_output=True,text=True,timeout=600,cwd=tmp)
            for store,run_id,root,layout in (({'P0_RUN_STORE':'legacy'},'old_run','data/econ4h','v1'),({},'new_run','data/runs','v2')):
                with self.subTest(layout=layout):
                    p=launch({**store,'P0_ECON_RUN_ID':run_id})
                    # X1: a launcher child without the approved network audit record is refused after its local
                    # manifest, before either owner starts (no network contact).
                    self.assertEqual(p.returncode,2,p.stderr[-2000:])
                    run=project/root/run_id
                    # JFI-03/PERSIST-04: the productive run wrote no JSON/JSONL. The pinned readers below (V2 detection
                    # via manifest.json included) read the views of the explicit offline export.
                    self.assertEqual([str(f) for f in run.rglob('*') if f.suffix in ('.json','.jsonl')],[])
                    x=subprocess.run([str(REPO/'project_0/native/target/release/p0ctl'),'--project',str(project),'export','--run',str(run)],
                                     capture_output=True,text=True,timeout=60)
                    self.assertEqual(x.returncode,0,x.stderr)
                    self.assertEqual(s.detect_layout(run),layout);self.assertEqual(a.find_run(str(run)),run.resolve())
                    manifest=json.loads(s.artifact(run,'manifest').read_text())  # written by the real Rust runtime
                    self.assertEqual(manifest['plan'],s.config_part(run,'measurement_plan'))
                    self.assertEqual((manifest['capture_run_id'],manifest['live_enabled'],manifest['submission_enabled']),(run_id,False,False))
                    self.assertEqual(manifest['schema'],'C0-E1/MeasurementManifest/3' if layout=='v2' else 'C0-E1/MeasurementManifest/2')
                    phases=[json.loads(x)['phase'] for x in s.artifact(run,'phases').read_text().splitlines()]
                    self.assertEqual(phases[0],'BOOTSTRAP')
                    for part in ('storage_preflight','candidate_manifest'):self.assertIsInstance(s.config_part(run,part),dict)
                    for name in ('build_log','console_summary','runtime_log'):
                        self.assertTrue(s.artifact(run,name).is_file(),name)
                    # PERSIST-01: binary journals are primary; the pinned readers' JSONL files are renders.
                    evaluations=s.artifact(run,'shadow_evaluations').with_suffix('.p0j')
                    self.assertTrue(evaluations.is_file())
                    for name in ('shadow_evaluations','shadow_offsets'):self.assertFalse(s.artifact(run,name).exists(),name)
                    tool=REPO/'project_0/native/target/release/p0-journal'
                    # No evaluation was lost (stream COMPLETE); the run failed (phases INCOMPLETE); the market
                    # capture never started (X1 refusal). Every journal verifies end to end.
                    self.assertFalse((s.artifact(run,'market')/'market.p0j').exists())
                    for journal,status in ((evaluations,'COMPLETE'),(s.artifact(run,'phases').with_suffix('.p0j'),'INCOMPLETE')):
                        p=subprocess.run([str(tool),'verify',str(journal)],capture_output=True,text=True,timeout=60)
                        self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(json.loads(p.stdout)['status'],status,journal)
                    # The capture writes no JSONL: views are MISSING until rendered explicitly, then they must be
                    # created once and match byte for byte (rendered across segment chains).
                    p=subprocess.run([str(tool),'renders',str(run)],capture_output=True,text=True,timeout=60)
                    self.assertEqual(p.returncode,0,p.stderr)
                    self.assertTrue(all(v['status'] in ('MATCHES','NO_FRAME','MISSING') for v in json.loads(p.stdout)),p.stdout)
                    p=subprocess.run([str(tool),'renders',str(run),'--create'],capture_output=True,text=True,timeout=60)
                    self.assertEqual(p.returncode,0,p.stderr)
                    self.assertTrue(all(v['status'] in ('CREATED','MATCHES','NO_FRAME') for v in json.loads(p.stdout)),p.stdout)
                    p=subprocess.run([str(tool),'renders',str(run)],capture_output=True,text=True,timeout=60)
                    self.assertTrue(all(v['status'] in ('MATCHES','NO_FRAME') for v in json.loads(p.stdout)),p.stdout)
                    log=s.artifact(run,'runtime_log').read_text()
                    self.assertIn('C0_E1_ADMISSION_REFUSED BOOT_NETWORK_AUDIT_MISSING',log);self.assertNotIn('C0_E1_WARMUP_FAILED',log)
                    self.assertEqual(c.capture_identity(run)['capture_run_id'],run_id)
                    p=subprocess.run(['bash',str(project/'scripts/verify.sh'),'--run',run_id,'--offline','--mode','off'],
                                     env={**clean_env(),'P0_ENV_FILE':str(tmp/'absent.env')},capture_output=True,text=True,timeout=120,cwd=tmp)
                    self.assertEqual(p.returncode,2);self.assertIn('CAPTURE_NOT_FINALIZED',p.stderr)  # real reader, incomplete capture
            new=project/'data/runs/new_run'
            self.assertTrue(set(x.name for x in new.iterdir())<=set(s.V2_ROOT))
            self.assertEqual(json.loads((new/'config.json').read_text())['schema'],s.RUN_CONFIG_SCHEMA)
            self.assertFalse(list((tmp/'tmpdir').glob('p0-run-config.*')))  # staging directory removed on exit
            self.assertFalse(any((project/'data/econ4h/old_run'/n).exists() for n in s.V2_ONLY))
            p=launch({'P0_RUN_STORE':'runs','P0_ECON_RUN_ID':'old_run'})
            self.assertEqual(p.returncode,2);self.assertIn('RUN_ID_COLLISION',p.stderr)
            self.assertFalse((project/'data/runs/old_run').exists())

    def test_shadow_runs_the_real_binary_from_any_directory(self):
        with tempfile.TemporaryDirectory() as folder:
            tmp=Path(folder);project=self.real_project(tmp);caller=tmp/'caller';caller.mkdir();(tmp/'tmpdir').mkdir()
            env=self.real_env(tmp);shutil.copy2(env['P0_KEYPAIR_PATH'],caller/'key.json');(caller/'key.json').chmod(0o600)
            env['P0_KEYPAIR_PATH']='key.json'  # relative to the caller: the signer gate and the runtime must use this same file
            run=lambda extra:subprocess.run(['bash',str(project/'scripts/capture.sh'),'shadow'],env={**env,**extra},capture_output=True,text=True,timeout=600,cwd=caller)
            p=run({'P0_RECORD_PATH':'out/opps.jsonl','P0_SUMMARY_PATH':'out/sum.jsonl','P0_MT0_PATH':'out/mt0.jsonl',
                   'P0_RPC_URL':'http://private:SECRET@127.0.0.1:1/PRIVATE_KEY?q=QUERY_SECRET',
                   'P0_WS_URL':'ws://private:SECRET@127.0.0.1:1/PRIVATE_KEY?q=QUERY_SECRET'})
            out=p.stdout+p.stderr
            self.assertEqual(p.returncode,2,out[-2000:]);self.assertIn('p0-longtail: ',p.stderr)  # real binary started from the repository
            self.assertNotIn('could not find `Cargo.toml`',out);self.assertNotIn('missing required env',out)
            for secret in ('private:','SECRET','PRIVATE_KEY'):
                self.assertNotIn(secret,out)
            self.assertIn('rpc_host=http://127.0.0.1',p.stdout)
            self.assertIn('ws_host=ws://127.0.0.1',p.stdout)
            record=Path(next(l.split('record=',1)[1] for l in p.stdout.splitlines() if 'record=' in l))
            self.assertEqual(record.resolve(),(caller/'out/opps.jsonl').resolve());self.assertTrue((caller/'out').is_dir())
            p=run({});self.assertEqual(p.returncode,2);self.assertIn('p0-longtail: ',p.stderr)
            self.assertTrue((project/'data/shadow').is_dir())  # absolute default under the (temporary) project
            self.assertEqual(sorted(x.name for x in caller.iterdir()),['key.json','out'])
            for extra in ({'P0_LIVE_ENABLED':'true'},{'P0_SUBMISSION_ENABLED':'1'}):
                p=run(extra);self.assertEqual(p.returncode,2);self.assertIn('shadow requires LIVE and submission disabled',p.stderr)
                self.assertNotIn('p0-longtail: ',p.stderr)  # refused before cargo and the binary

    def test_changed_launchers_parse(self):
        self.assertEqual(sorted(p.name for p in SCRIPTS.iterdir() if p.is_file()),
                         ['capture.sh','preflight.sh','report.sh','smoke.sh','test.sh','verify.sh'])
        for name in ('capture.sh','smoke.sh','preflight.sh','verify.sh','report.sh','test.sh'):
            self.assertEqual(subprocess.run(['bash','-n',str(SCRIPTS/name)]).returncode,0,name)


class Layout(unittest.TestCase):
    def test_v1_without_marker_v2_only_with_marker(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            self.assertEqual(s.detect_layout(legacy_run(p,'legacy')),'v1')
            empty=p/'empty';empty.mkdir();self.assertEqual(s.detect_layout(empty),'v1')
            self.assertEqual(s.detect_layout(v2_run(p,'v2')),'v2')

    def test_mixed_or_malformed_layouts_are_rejected_not_guessed(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            with self.assertRaisesRegex(s.RunStoreError,'RUN_LAYOUT_UNSUPPORTED'):s.detect_layout(v2_run(p,'wrong_tag','Project0/RunLayout/9'))
            shutil.rmtree(p/'wrong_tag')
            v2_run(p,'not_a_tag','layout-two')
            (p/'not_json').mkdir();(p/'not_json/manifest.json').write_text('{')
            (p/'list').mkdir();(p/'list/manifest.json').write_text('[]')
            (p/'dir_manifest/manifest.json').mkdir(parents=True)
            for name in ('config.json','capture','snapshots','logs'):
                run=legacy_run(p,'mixed_'+name.replace('.','_'))
                (run/name).mkdir() if '.' not in name else (run/name).write_text('{}')
            for run in sorted(x for x in p.iterdir()):
                with self.subTest(run=run.name):
                    with self.assertRaisesRegex(s.RunStoreError,'RUN_LAYOUT_INVALID'):s.detect_layout(run)


class Locate(unittest.TestCase):
    def test_exact_id_across_all_roots_and_explicit_paths(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder)
            for root,name in (('data/econ4h','e1'),('data/market','m1'),('data/runs','r1')):
                run=legacy_run(p/root,name)
                self.assertEqual(s.locate_run(name,project=p),run.resolve())
                self.assertEqual(s.locate_run(str(run),project=p),run.resolve())
            legacy_run(p/'data/econ4h','twice');legacy_run(p/'data/runs','twice')
            for bad in ('twice','missing','../e1','a/b','',None):
                with self.assertRaisesRegex(s.RunStoreError,'RUN_NOT_FOUND_OR_AMBIGUOUS'):s.locate_run(bad,project=p)
            run=v2_run(p/'data/runs','new_layout')
            self.assertEqual(s.locate_run('new_layout',project=p),run.resolve())
            v2_run(p/'data/runs','future','Project0/RunLayout/3')
            with self.assertRaisesRegex(s.RunStoreError,'RUN_LAYOUT_UNSUPPORTED'):s.locate_run('future',project=p)
            (legacy_run(p/'data/runs','broken')/'logs').mkdir()
            with self.assertRaisesRegex(s.RunStoreError,'RUN_LAYOUT_INVALID'):s.locate_run('broken',project=p)

    def test_one_directory_reached_through_two_roots_is_not_ambiguous(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);run=legacy_run(p/'data/econ4h','same');(p/'data/runs').symlink_to(p/'data/econ4h')
            self.assertEqual(s.locate_run('same',project=p),run.resolve())

    def test_run_arguments_rewrite_only_the_run_option(self):
        with tempfile.TemporaryDirectory() as folder:
            p=Path(folder);run=legacy_run(p/'data/runs','r1').resolve()
            self.assertEqual(s.run_arguments(['--run','r1','--mode','index'],project=p),['--run',str(run),'--mode','index'])
            self.assertEqual(s.run_arguments(['--format','json','--run=r1'],project=p),['--format','json','--run='+str(run)])
            self.assertEqual(s.run_arguments(['--run','missing','--help'],project=p),['--run','missing','--help'])
            self.assertEqual(s.run_arguments(['--offline','--run'],project=p),['--offline','--run'])
            with self.assertRaisesRegex(s.RunStoreError,'RUN_NOT_FOUND_OR_AMBIGUOUS'):s.run_arguments(['--run','missing'],project=p)


class ReaderCli(unittest.TestCase):
    """verify.sh / report.sh with byte-identical copies of the pinned readers in a temporary project."""
    def project(self,tmp):
        project=tmp/'project_0';scripts=project/'scripts';scripts.mkdir(parents=True)
        for name in ('verify.sh','report.sh'):shutil.copy2(SCRIPTS/name,scripts/name)
        shutil.copytree(p0_paths.OPERATIONS,project/'operations',ignore=shutil.ignore_patterns('__pycache__'))
        (project/'native').symlink_to(REPO/'project_0/native')
        return project

    def call(self,project,script,*args):
        env={**clean_env(),'P0_ENV_FILE':str(project/'absent.env')}
        return subprocess.run(['bash',str(project/'scripts'/script),*args],env=env,capture_output=True,text=True,timeout=120,cwd=tempfile.gettempdir())

    def test_wrappers_hand_located_directories_to_the_readers(self):
        # Real verify.sh/report.sh and real readers on complete synthetic runs (deterministic local inputs).
        with tempfile.TemporaryDirectory() as folder:
            project=self.project(Path(folder))
            r1=manifest_run(project/'data/runs','r1','v1','r1').resolve();e1=manifest_run(project/'data/econ4h','e1','v1','e1').resolve()
            p=self.call(project,'report.sh','--run','r1','--format','json')
            self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(len(json.loads(p.stdout)),1)
            self.assertTrue(json.loads(p.stderr.strip().splitlines()[-1])['dataset'].startswith(str(r1/'reports')))
            p=self.call(project,'verify.sh','--run=e1','--offline','--mode','off')
            self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(json.loads(p.stdout)['capture_run_id'],'e1')
            self.assertTrue((e1/'verification').is_dir());self.assertFalse((r1/'verification').exists())
            manifest_run(project/'data/market','r1','v1','r1')
            p=self.call(project,'report.sh','--run','r1')
            self.assertEqual(p.returncode,2);self.assertIn('RUN_NOT_FOUND_OR_AMBIGUOUS',p.stderr);self.assertEqual(p.stdout,'')
        p=subprocess.run(['bash',str(SCRIPTS/'verify.sh'),'--self-test'],env=clean_env(),capture_output=True,text=True,timeout=300,cwd=tempfile.gettempdir())
        self.assertEqual(p.returncode,0,p.stderr[-2000:]);self.assertIn('Ran 29 tests',p.stderr)

    def test_verify_resume_and_report_by_id_keep_identities(self):
        with tempfile.TemporaryDirectory() as folder:
            project=self.project(Path(folder))
            for name in ('cli/measurement_contract.py','verification/audit_market_quick.py','reporting/market_report.py','capture/run_store.py'):
                self.assertEqual(c.file_hash(project/'operations'/name),c.file_hash(p0_paths.OPERATIONS/name))
            run=project/'data/runs/run_v1';(run/'market').mkdir(parents=True)
            (run/'market/summary.json').write_text(json.dumps({'observation_started_at':a.iso_ns(100*a.NS),'observation_ended_at':a.iso_ns(200*a.NS),
                'configured_duration_seconds':100,'full_slot_reconciliation':{'market_slot_end':{'slot':11}}}))
            for name in ('transactions.jsonl','coverage.jsonl'):(run/'market'/name).write_text('')
            first=self.call(project,'verify.sh','--run','run_v1','--offline','--mode','off')
            self.assertEqual(first.returncode,0,first.stderr);first=json.loads(first.stdout)
            second=self.call(project,'verify.sh','--run',str(run),'--offline','--mode','off','--resume-verification')
            self.assertEqual(second.returncode,0,second.stderr);second=json.loads(second.stdout)
            with patch.dict(os.environ,clean_env(),clear=True):
                expected=c.specification(run.resolve(),c.VerificationConfig(),c.preflight(run,c.VerificationConfig(),None,None)['provider_binding'])
            self.assertEqual(first['capture_run_id'],second['capture_run_id'])
            self.assertEqual(first['verification_spec_id'],second['verification_spec_id'])
            self.assertEqual(first['verification_spec_id'],expected['verification_spec_id'])
            self.assertNotEqual(first['verification_attempt_id'],second['verification_attempt_id'])
            by_id=self.call(project,'report.sh','--run','run_v1','--verification-result',first['verification_result_id'],'--format','json')
            by_path=self.call(project,'report.sh','--run',str(run),'--verification-result',first['verification_result_id'],'--format','json')
            self.assertEqual((by_id.returncode,by_path.returncode),(0,0),by_id.stderr+by_path.stderr)
            meta=[json.loads(x.stderr.strip().splitlines()[-1]) for x in (by_id,by_path)]
            self.assertEqual(meta[0]['report_query_identity'],meta[1]['report_query_identity'])
            self.assertEqual(meta[0]['capture_run_id'],first['capture_run_id'])
            # Index checkpoint resume through the id lookup: same identity, honest INCOMPLETE without RPC.
            index=[self.call(project,'verify.sh','--run','run_v1','--offline','--mode','index',*extra) for extra in ((),('--resume-verification',))]
            results=[json.loads(x.stdout) for x in index]
            self.assertEqual([x.returncode for x in index],[2,2])
            self.assertEqual(results[0]['verification_spec_id'],results[1]['verification_spec_id'])
            self.assertTrue(all(x['summary']['status']=='HISTORICAL_INCOMPLETE' and x['summary']['reason']=='RPC_ACCESS_MISSING' for x in results))
            self.assertNotIn('RESUME_IDENTITY_MISMATCH',index[1].stdout+index[1].stderr)
            # Layout V2 by id through the same wrappers: own spec version and layout, writes only verification/ and reports/.
            v2=v2_run(project/'data/runs','run_v2');before={str(x.relative_to(v2)) for x in v2.rglob('*')}
            p=self.call(project,'verify.sh','--run','run_v2','--offline','--mode','off')
            self.assertEqual(p.returncode,0,p.stderr);result=json.loads(p.stdout)
            spec=json.loads((v2/'verification'/result['verification_spec_id']/'spec.json').read_text())
            self.assertEqual((spec['schema'],spec['layout'],result['capture_run_id']),(c.SPEC_SCHEMA,s.LAYOUT_V2,'run_v2'))
            p=self.call(project,'report.sh','--run','run_v2','--verification-result',result['verification_result_id'],'--format','json')
            self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(len(json.loads(p.stdout)),1)
            added={str(x.relative_to(v2)) for x in v2.rglob('*')}-before
            self.assertTrue(added and all(x.split('/')[0] in ('verification','reports') for x in added),sorted(added))
            self.assertIn('verification/capture_evidence_manifest.json',added)


class GitIgnore(unittest.TestCase):
    def test_runs_ignored_but_evidence_and_config_remain_eligible(self):
        if not shutil.which('git'):self.skipTest('git unavailable')
        def ignored(path):
            p=subprocess.run(['git','-C',str(REPO),'check-ignore','-q','--no-index','--',path])
            return p.returncode==0
        self.assertTrue(ignored('project_0/data/runs/econ4h_x/manifest.json'))
        self.assertTrue(ignored('project_0/data/runs/econ4h_x/capture/transactions.jsonl'))
        self.assertTrue(ignored('project_0/data/cache/gap_proxy/g1.abc123/proxy.log'))
        for path in ('project_0/docs/evidence/NEW_20261015/results.json','project_0/config/profiles/smoke.conf',
                     'project_0/exports/report.json','project_0/data/.gitkeep'):
            self.assertFalse(ignored(path),path)


if __name__=='__main__':
    unittest.main(verbosity=2)
