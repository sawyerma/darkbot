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
READINESS=REPO/'bsc/engine/target/debug/g07_pre_live_readiness'
CONFIG='# public readiness configuration\nBSC_READINESS_RPC_HTTPS=https://rpc.example.invalid/\nDARKBOT_G07_CLOCK_ID=macos.CLOCK_UPTIME_RAW\n'
def job(exit_code,config=CONFIG,runner_bin=None,seen=None):
 with tempfile.TemporaryDirectory(prefix='darkbot-readiness-') as folder:
  base=pathlib.Path(folder);root=base/'root';run=base/'run';tools=base/'bin';root.mkdir();tools.mkdir()
  (root/'SNAPSHOT_SHA256').write_text('a'*64);(root/'COMMIT').write_text('fixture');(root/'CHECKOUT').write_text('/fixture/checkout\n')
  if config is not None:(root/'bsc/config').mkdir(parents=True);(root/'bsc/config/readiness.public.env').write_text(config)
  shutil.copy2(REPORTER,tools/'bsc-status-reporter')
  runner=tools/'g07_pre_live_readiness'
  if runner_bin: shutil.copy2(runner_bin,runner)
  else: runner.write_text(f"#!/bin/sh\nenv > '{base}/runner.env'\ncat '{GOLDEN}'\nexit {exit_code}\n");runner.chmod(0o755)
  env={'PATH':'/usr/bin:/bin','DARKBOT_BSC_ROOT':str(root),'DARKBOT_RUNDIR':str(run),'DARKBOT_BSC_LAUNCH_ID':'job-1','DARKBOT_BSC_READINESS_BIN':str(runner)}
  process=subprocess.run([sys.executable,str(WRAPPER)],env=env,capture_output=True,text=True,timeout=60)
  if seen is not None and (base/'runner.env').exists():seen.update(l.split('=',1) for l in (base/'runner.env').read_text().splitlines() if '=' in l)
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
 def test_the_job_takes_rpc_and_clock_from_the_public_config_and_the_boot_id_from_the_os(self):
  # BSC-050 A-scan: no hardcoded endpoint; the snapshot's checkout for latch/authority.
  seen={}
  code,result,_=job(2,seen=seen)
  self.assertEqual(result['result_code'],'BLOCKED')
  self.assertEqual(seen.get('BSC_MAINNET_RPC_HTTPS'),'https://rpc.example.invalid/')
  self.assertEqual(seen.get('DARKBOT_G07_CLOCK_ID'),'macos.CLOCK_UPTIME_RAW')
  boot=subprocess.run(['/usr/sbin/sysctl','-n','kern.bootsessionuuid'],capture_output=True,text=True).stdout.strip()
  self.assertEqual(seen.get('DARKBOT_G07_BOOT_ID'),boot)
  self.assertEqual(seen.get('DARKBOT_BSC_CHECKOUT'),'/fixture/checkout')
 def test_without_a_configured_endpoint_the_readiness_binary_never_starts(self):
  for config in [None,'DARKBOT_G07_CLOCK_ID=x\n','BSC_READINESS_RPC_HTTPS=http://plain.invalid/\n','BSC_READINESS_RPC_HTTPS=https://ok.invalid/\nOTHER=1\n']:
   seen={}
   code,result,published=job(0,config=config,seen=seen)
   self.assertEqual((result['result_code'],published,code,seen),('FAILED',False,2,{}),config)
 @unittest.skipUnless(READINESS.is_file(),'build g07_pre_live_readiness first')
 def test_the_real_readiness_binary_end_to_end(self):
  # The real native binary in an empty snapshot: no v5 contract, a typed BLOCKED.
  code,result,published=job(None,runner_bin=READINESS)
  self.assertEqual((result['result_code'],result['exit_code'],published,code),('BLOCKED','2',True,2))
if __name__=='__main__':unittest.main()
