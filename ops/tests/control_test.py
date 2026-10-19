#!/usr/bin/env python3
"""Integration of real browser, gateway and launchd in the existing isolated fixture only."""
import shutil, subprocess
import lifecycle_test as h
try:
    release=h.WORK/'app/releases/test';release.mkdir(parents=True)
    for name,source in [('darkbot-gateway','gateway/target/debug/darkbot-gateway'),('p0-market-capture-reported','project_0/native/target/debug/p0-market-capture-reported'),('darkbotctl','ops/darkbotctl')]:shutil.copy2(h.ROOT/source,release/name)
    (release/'darkbotctl').chmod(0o755)
    shutil.copytree(h.ROOT/'frontend/dist',release/'dist')
    (h.WORK/'app/current').symlink_to(release,target_is_directory=True)
    (h.WORK/'app.conf').write_text('P0_APP_AUTOSTART_CAPTURE=1\nP0_APP_CAPTURE_SECS=until-stopped\n')
    (h.WORK/'capture.env').write_text(f'P0_WS_URL=ws://127.0.0.1:{h.server.server_address[1]}\nP0_PROGRAM_ID=11111111111111111111111111111111\nP0_MARKET_SOURCE=logs\nP0_MARKET_READY_TIMEOUT_SECS=4\nP0_MARKET_DRAIN_TIMEOUT_SECS=3\nP0_MARKET_HEARTBEAT_SECS=1\n')
    h.run('up','--no-open')
    subprocess.run(['node',str(h.ROOT/'ops/tests/control_browser.mjs'),str(h.port),str(h.WORK),str(h.ROOT)],check=True,timeout=150,env=h.env)
finally:
    h.run('down',ok=False)
    for kind in ('capture','gateway'):subprocess.run(['launchctl','bootout',h.domain+'/'+h.label+'.'+kind],capture_output=True)
    h.server.shutdown()
    print('Isolated evidence:',h.WORK,flush=True)
