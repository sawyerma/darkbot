#!/usr/bin/env python3
"""Offline snapshot/import integrity and secret exclusion, in a temporary fixture only."""
import hashlib,importlib.util,os,pathlib,shutil,subprocess,tempfile,unittest
REPO=pathlib.Path(__file__).resolve().parents[2]
REPORTER=pathlib.Path(os.environ.get('DARKBOT_TEST_REPORTER',REPO/'bsc/engine/target/debug/bsc-status-reporter'))
GOLDEN=REPO/'protocol/golden/bsc/readiness.dbrec'
spec=importlib.util.spec_from_file_location('status_sync',pathlib.Path(__file__).with_name('status_sync.py'));mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
@unittest.skipUnless(REPORTER.is_file(),'build bsc-status-reporter first')
class SyncTests(unittest.TestCase):
 def test_public_snapshot_and_immutable_import(self):
  with tempfile.TemporaryDirectory(prefix='darkbot-sync-') as folder:
   base=pathlib.Path(folder);root=base/'repo';root.mkdir();app=base/'app';run=base/'run'
   subprocess.run(['git','init','-q',str(root)],check=True)
   (root/'bsc/config').mkdir(parents=True);(root/'bsc/tools').mkdir()
   (root/'bsc/config/wallet.public.env').write_text('BSC_WALLET_STATUS=UNPROVISIONED\n')
   (root/'bsc/tools/readiness_job.py').write_text('# fixture\n')
   (root/'bsc/.env').write_text('PRIVATE_KEY=NEVER_COPY_THIS\n')
   subprocess.run(['git','-C',str(root),'add','bsc/config','bsc/tools'],check=True)
   subprocess.run(['git','-C',str(root),'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture'],check=True)
   launch='12345678-1234-1234-1234-123456789abc'
   relative=pathlib.Path('bsc/native/evidence/g07/gui_readiness_'+launch)
   output=run/'bsc-operational'/relative;output.mkdir(parents=True)
   raw=GOLDEN.read_bytes();(output/'readiness.dbrec').write_bytes(raw)
   # BSC-048: the typed provenance, sealed by the native reporter, is the import gate.
   snap=base/'jobsnap';snap.mkdir();(snap/'SNAPSHOT_SHA256').write_text('a'*64);(snap/'COMMIT').write_text('fixture')
   sealed=subprocess.run([str(REPORTER),'--seal-readiness-provenance',str(output),launch,str(relative)],env={'PATH':'/usr/bin:/bin','DARKBOT_BSC_ROOT':str(snap),'DARKBOT_RUNDIR':str(run)},capture_output=True,text=True)
   self.assertEqual(sealed.returncode,0,sealed.stderr)
   release=root/'bsc/engine/target/release';release.mkdir(parents=True);shutil.copy2(REPORTER,release/'bsc-status-reporter')
   mod.sync(root,app,run)
   snapshot=(app/'bsc-source').resolve();self.assertFalse((snapshot/'bsc/.env').exists())
   self.assertEqual((root/relative/'readiness.dbrec').read_bytes(),raw)
   self.assertEqual((snapshot/relative/'readiness.dbrec').read_bytes(),raw)
   self.assertTrue((snapshot/'SNAPSHOT_SHA256').is_file())
   self.assertEqual((root/relative/'provenance.dbrec').read_bytes(),(output/'provenance.dbrec').read_bytes())
   self.assertFalse(any(p.suffix=='.json' for p in snapshot.rglob('*')))
   manifest=(snapshot/'SOURCES.SHA256SUMS').read_bytes()
   self.assertEqual((snapshot/'SNAPSHOT_SHA256').read_text(),hashlib.sha256(manifest).hexdigest())
   self.assertIn(hashlib.sha256(raw).hexdigest()+'  ./'+str(relative/'readiness.dbrec'),manifest.decode())
   (root/'bsc/config/wallet.public.env').write_text('BSC_WALLET_STATUS=CHANGED\n')
   mod.sync(root,app,run);self.assertNotEqual(snapshot,(app/'bsc-source').resolve())
   self.assertIn('UNPROVISIONED',(snapshot/'bsc/config/wallet.public.env').read_text())
   (root/relative/'readiness.dbrec').write_bytes(b'conflict')
   with self.assertRaises(ValueError):mod.sync(root,app,run)
   # A result whose provenance is not its own is never imported.
   (root/relative/'readiness.dbrec').write_bytes(raw)
   other=run/'bsc-operational/bsc/native/evidence/g07/gui_readiness_other-1';other.mkdir()
   for name in ['readiness.dbrec','provenance.dbrec']:(other/name).write_bytes((output/name).read_bytes())
   with self.assertRaises(ValueError):mod.sync(root,app,run)
   self.assertFalse((root/'bsc/native/evidence/g07/gui_readiness_other-1').exists())
 def test_candidate_mirror_is_public_and_requests_need_the_native_store(self):
  with tempfile.TemporaryDirectory(prefix='darkbot-sync-') as folder:
   base=pathlib.Path(folder);root=base/'repo';root.mkdir();app=base/'app';run=base/'run'
   subprocess.run(['git','init','-q',str(root)],check=True)
   (root/'bsc/tools').mkdir(parents=True);(root/'bsc/tools/readiness_job.py').write_text('# fixture\n')
   subprocess.run(['git','-C',str(root),'add','bsc/tools'],check=True)
   subprocess.run(['git','-C',str(root),'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture'],check=True)
   state=root/'bsc/.local/g07_candidates';state.mkdir(parents=True)
   for name in ['a.candidate.dbrec','b.candidate.dbrec','selection.dbrec','a.qualification.dbrec','b.salt.dbrec','a.salt.dbrec']:(state/name).write_bytes(name.encode())
   mod.sync(root,app,run)
   snapshot=(app/'bsc-source').resolve();mirror=snapshot/'bsc/native/state/g07_candidates'
   self.assertEqual(sorted(p.name for p in mirror.iterdir()),['a.candidate.dbrec','a.qualification.dbrec','b.candidate.dbrec','selection.dbrec'])
   self.assertFalse(any('salt' in p.name for p in snapshot.rglob('*')))
   self.assertNotIn('salt',(snapshot/'SOURCES.SHA256SUMS').read_text())
   # A pending GUI request is never skipped: without the native store build the sync stops.
   requests=run/'bsc-operational/g07_candidates/requests';requests.mkdir(parents=True)
   (requests/'1b4e28ba-2fa1-41d2-883f-0016d3cca427.request.dbrec').write_bytes(b'request')
   with self.assertRaises(ValueError):mod.sync(root,app,run)
 def test_the_snapshot_carries_the_checkout_the_public_config_and_exactly_the_v5_bound_inputs(self):
  # BSC-050 A-scan: GUI readiness evaluates the bound evidence and the checkout's latch/authority.
  with tempfile.TemporaryDirectory(prefix='darkbot-sync-') as folder:
   base=pathlib.Path(folder);root=base/'repo';root.mkdir();app=base/'app';run=base/'run'
   subprocess.run(['git','init','-q',str(root)],check=True)
   (root/'bsc/config').mkdir(parents=True);(root/'bsc/tools').mkdir()
   (root/'bsc/tools/readiness_job.py').write_text('# fixture\n')
   (root/'bsc/config/readiness.public.env').write_text('BSC_READINESS_RPC_HTTPS=https://rpc.example.invalid/\n')
   subprocess.run(['git','-C',str(root),'add','bsc/tools'],check=True)
   subprocess.run(['git','-C',str(root),'-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','fixture'],check=True)
   (root/'bsc/config/g07_attempt1_acceptance_v5.dbrec').write_bytes(b'v5 fixture')
   sign=root/'bsc/.local/g06a_sign/g06a_sign_20261025T000000Z';sign.mkdir(parents=True)
   (sign/'dry_result.dbrec').write_bytes(b'bound dry')
   (root/'bsc/config/g07_producer_witness_source_attempt1.json').write_bytes(b'{"legacy":1}')
   release=root/'bsc/engine/target/release';release.mkdir(parents=True)
   lister=release/'g07_pre_live_readiness'
   def bound(*paths):
    lister.write_text('#!/bin/sh\n[ "$1" = --bound-inputs ] || exit 9\nprintf "%s\\n" '+' '.join(repr(p) for p in paths)+'\n');lister.chmod(0o755)
   bound('bsc/.local/g06a_sign/g06a_sign_20261025T000000Z/dry_result.dbrec','bsc/config/g07_producer_witness_source_attempt1.json')
   mod.sync(root,app,run)
   snapshot=(app/'bsc-source').resolve()
   self.assertEqual((snapshot/'CHECKOUT').read_text().strip(),str(root.resolve()))
   self.assertTrue((snapshot/'bsc/config/readiness.public.env').is_file())
   self.assertEqual((snapshot/'bsc/.local/g06a_sign/g06a_sign_20261025T000000Z/dry_result.dbrec').read_bytes(),b'bound dry')
   manifest=(snapshot/'SOURCES.SHA256SUMS').read_text()
   self.assertIn('./bsc/config/g07_producer_witness_source_attempt1.json',manifest)
   for escape in ['../outside','/etc/hosts','bsc/missing.dbrec']:
    bound(escape)
    with self.assertRaises(ValueError,msg=escape):mod.sync(root,app,run)
   lister.unlink()
   with self.assertRaises(ValueError):mod.sync(root,app,run)
if __name__=='__main__':unittest.main()
