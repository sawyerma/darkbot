#!/usr/bin/env python3
"""One isolated macOS/BSC package run: real reporter/gateway/launchd/browser.
The first readiness job uses the native binary with a missing contract (zero egress).
Cancellation/crash use a fixture child. Production labels and files are untouched.
"""
import pathlib, shutil, subprocess
import lifecycle_test as h
try:
    release=h.WORK/'app/releases/test';release.mkdir(parents=True)
    for name,source in [('darkbot-gateway','gateway/target/debug/darkbot-gateway'),('darkbotctl','ops/darkbotctl'),('bsc-control.sh','ops/bsc-control.sh'),('bsc-status-reporter','bsc/engine/target/debug/bsc-status-reporter'),('g07_pre_live_readiness','bsc/engine/target/debug/g07_pre_live_readiness')]:
        shutil.copy2(h.ROOT/source,release/name)
    (release/'darkbotctl').chmod(0o755)
    shutil.copytree(h.ROOT/'frontend/dist',release/'dist')
    source=h.WORK/'app/snapshot';(source/'bsc/tools').mkdir(parents=True)
    shutil.copy2(h.ROOT/'bsc/tools/readiness_job.py',source/'bsc/tools/readiness_job.py')
    (source/'COMMIT').write_text('isolated-fixture');(source/'SYNCED_AT').write_text('1792232000');(source/'SNAPSHOT_SHA256').write_text('a'*64)
    evidence=source/'bsc/native/evidence/g07/fixture';evidence.mkdir(parents=True)
    shutil.copy2(h.ROOT/'protocol/golden/bsc/readiness.dbrec',evidence/'readiness.dbrec')
    (h.WORK/'app/bsc-source').symlink_to(source,target_is_directory=True)
    (release/'bsc-root').symlink_to(source,target_is_directory=True)
    (h.WORK/'app/current').symlink_to(release,target_is_directory=True)
    h.run('up','--no-open')
    subprocess.run(['node',str(h.ROOT/'ops/tests/bsc_control_browser.mjs'),str(h.port),str(h.WORK),str(h.ROOT)],check=True,timeout=180,env=h.env)
finally:
    h.run('down',ok=False)
    for kind in ('bsc.readiness','bsc.reporter','gateway'):
        subprocess.run(['launchctl','bootout',h.domain+'/'+h.label+'.'+kind],capture_output=True)
    h.server.shutdown()
    print('Isolated BSC evidence:',h.WORK,flush=True)
