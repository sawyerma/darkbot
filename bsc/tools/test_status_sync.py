#!/usr/bin/env python3
"""Offline snapshot/import integrity and secret exclusion, in a temporary fixture only."""
import hashlib,importlib.util,json,pathlib,subprocess,tempfile,unittest
spec=importlib.util.spec_from_file_location('status_sync',pathlib.Path(__file__).with_name('status_sync.py'));mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
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
   relative=pathlib.Path('bsc/native/evidence/g07/gui_readiness_12345678-1234-1234-1234-123456789abc')
   output=run/'bsc-operational'/relative;output.mkdir(parents=True)
   raw=b'fixture immutable bytes';(output/'readiness.dbrec').write_bytes(raw)
   (output/'provenance.json').write_text(json.dumps({'kind':'READ_ONLY_OPERATIONAL','result_sha256':hashlib.sha256(raw).hexdigest()}))
   mod.sync(root,app,run)
   snapshot=(app/'bsc-source').resolve();self.assertFalse((snapshot/'bsc/.env').exists())
   self.assertEqual((root/relative/'readiness.dbrec').read_bytes(),raw)
   self.assertEqual((snapshot/relative/'readiness.dbrec').read_bytes(),raw)
   self.assertTrue((snapshot/'SNAPSHOT_SHA256').is_file())
   (root/'bsc/config/wallet.public.env').write_text('BSC_WALLET_STATUS=CHANGED\n')
   mod.sync(root,app,run);self.assertNotEqual(snapshot,(app/'bsc-source').resolve())
   self.assertIn('UNPROVISIONED',(snapshot/'bsc/config/wallet.public.env').read_text())
   (root/relative/'readiness.dbrec').write_bytes(b'conflict')
   with self.assertRaises(ValueError):mod.sync(root,app,run)
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
   self.assertNotIn('salt',(snapshot/'sources.json').read_text())
   # A pending GUI request is never skipped: without the native store build the sync stops.
   requests=run/'bsc-operational/g07_candidates/requests';requests.mkdir(parents=True)
   (requests/'1b4e28ba-2fa1-41d2-883f-0016d3cca427.request.dbrec').write_bytes(b'request')
   with self.assertRaises(ValueError):mod.sync(root,app,run)
if __name__=='__main__':unittest.main()
