#!/usr/bin/env python3
"""Stop All with an unconfirmed stop, isolated: an isolated BSC readiness fixture ignores SIGTERM;
the Project_0 raw capture starts only through its boot flow (DEC-07) and stays stopped.
Only com.darkbot.test.* labels; no production instance."""
import shutil, subprocess
import lifecycle_test as h
try:
    release=h.WORK/'app/releases/test';release.mkdir(parents=True)
    for name,source in [('darkbot-gateway','gateway/target/debug/darkbot-gateway'),('p0-market-capture-reported','project_0/native/target/debug/p0-market-capture-reported'),('darkbotctl','ops/darkbotctl'),('bsc-control.sh','ops/bsc-control.sh'),('bsc-status-reporter','bsc/engine/target/debug/bsc-status-reporter')]:shutil.copy2(h.ROOT/source,release/name)
    (release/'darkbotctl').chmod(0o755)
    shutil.copytree(h.ROOT/'frontend/dist',release/'dist')
    source=h.WORK/'app/snapshot';(source/'bsc/tools').mkdir(parents=True)
    shutil.copy2(h.ROOT/'bsc/tools/readiness_job.py',source/'bsc/tools/readiness_job.py')
    # The job reads its endpoint from the snapshot's public config; the fixture binary never calls it.
    (source/'bsc/config').mkdir();(source/'bsc/config/readiness.public.env').write_text('BSC_READINESS_RPC_HTTPS=https://readiness.invalid\n')
    (source/'COMMIT').write_text('isolated-fixture');(source/'SYNCED_AT').write_text('1792232000');(source/'SNAPSHOT_SHA256').write_text('a'*64)
    (h.WORK/'app/bsc-source').symlink_to(source,target_is_directory=True)
    (release/'bsc-root').symlink_to(source,target_is_directory=True)
    # The stop never takes effect: SIGTERM is ignored until the test releases the fixture.
    readiness=release/'g07_pre_live_readiness'
    readiness.write_text(f"#!/usr/bin/python3\nimport pathlib,signal,time\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\nwhile not pathlib.Path({str(h.WORK/'release-readiness')!r}).exists(): time.sleep(.1)\n")
    readiness.chmod(0o755)
    (h.WORK/'app/current').symlink_to(release,target_is_directory=True)
    (h.WORK/'app.conf').write_text('P0_APP_AUTOSTART_CAPTURE=1\nP0_APP_CAPTURE_SECS=until-stopped\n')
    (h.WORK/'capture.env').write_text(f'P0_WS_URL=ws://127.0.0.1:{h.server.server_address[1]}\nP0_PROGRAM_ID=11111111111111111111111111111111\nP0_MARKET_SOURCE=logs\nP0_MARKET_READY_TIMEOUT_SECS=4\nP0_MARKET_DRAIN_TIMEOUT_SECS=3\nP0_MARKET_HEARTBEAT_SECS=1\n')
    h.run('up','--no-open')
    subprocess.run(['node',str(h.ROOT/'ops/tests/stop_all_partial_browser.mjs'),str(h.port),str(h.WORK)],check=True,timeout=150,env=h.env)
finally:
    (h.WORK/'release-readiness').touch()
    h.run('down',ok=False)
    for kind in ('capture','bsc.readiness','bsc.reporter','gateway'):subprocess.run(['launchctl','bootout',h.domain+'/'+h.label+'.'+kind],capture_output=True)
    h.server.shutdown()
    print('Isolated evidence:',h.WORK,flush=True)
