#!/usr/bin/env python3
"""The job wrapper keeps the validated native readiness verdict (PASS_READ_ONLY/BLOCKED/STOP)
and treats an exit code that contradicts the record as a runner failure. Temporary fixture
only; needs a built bsc-status-reporter (debug target or DARKBOT_TEST_REPORTER)."""
import os,pathlib,shutil,subprocess,sys,tempfile,unittest
TOOLS=pathlib.Path(__file__).resolve().parent
REPO=TOOLS.parents[1]
WRAPPER=pathlib.Path(os.environ.get('DARKBOT_TEST_WRAPPER',TOOLS/'readiness_job.py'))
REPORTER=pathlib.Path(os.environ.get('DARKBOT_TEST_REPORTER',REPO/'bsc/engine/target/debug/bsc-status-reporter'))
GOLDEN=REPO/'protocol/golden/bsc/readiness.dbrec' # status BLOCKED, mode OFFLINE_ZERO_EGRESS
def job(exit_code):
 with tempfile.TemporaryDirectory(prefix='darkbot-readiness-') as folder:
  base=pathlib.Path(folder);root=base/'root';run=base/'run';tools=base/'bin';root.mkdir();tools.mkdir()
  (root/'SNAPSHOT_SHA256').write_text('a'*64);(root/'COMMIT').write_text('fixture')
  shutil.copy2(REPORTER,tools/'bsc-status-reporter')
  runner=tools/'g07_pre_live_readiness';runner.write_text(f"#!/bin/sh\ncat '{GOLDEN}'\nexit {exit_code}\n");runner.chmod(0o755)
  env={'PATH':'/usr/bin:/bin','DARKBOT_BSC_ROOT':str(root),'DARKBOT_RUNDIR':str(run),'DARKBOT_BSC_LAUNCH_ID':'job-1','DARKBOT_BSC_READINESS_BIN':str(runner)}
  process=subprocess.run([sys.executable,str(WRAPPER)],env=env,capture_output=True,text=True,timeout=60)
  result=dict(l.split('=',1) for l in (run/'bsc.readiness.result').read_text().splitlines())
  out=run/'bsc-operational/bsc/native/evidence/g07/gui_readiness_job-1'
  published=(out/'readiness.dbrec').is_file()
  if published:
   # BSC-048: the provenance is the typed record the sync's import gate accepts; no JSON.
   assert not (out/'provenance.json').exists()
   checked=subprocess.run([str(REPORTER),'--check-readiness-provenance',str(out)],env={'PATH':'/usr/bin:/bin','DARKBOT_BSC_ROOT':str(root),'DARKBOT_RUNDIR':str(run)},capture_output=True,text=True)
   assert checked.returncode==0 and 'launch_id=job-1' in checked.stdout, checked.stderr
  return process.returncode,result,published
@unittest.skipUnless(REPORTER.is_file(),'build bsc-status-reporter first')
class WrapperTests(unittest.TestCase):
 def test_a_native_blocked_verdict_stays_blocked(self):
  code,result,published=job(2)
  self.assertEqual((result['result_code'],result['exit_code'],published,code),('BLOCKED','2',True,2))
 def test_an_exit_code_contradicting_the_record_is_a_runner_failure(self):
  code,result,published=job(0)
  self.assertEqual((result['result_code'],published,code),('FAILED',False,2))
if __name__=='__main__':unittest.main()
