#!/usr/bin/env python3
"""macOS launchd integration, real capture/gateway, a local-only Solana endpoint that counts connections.
Every kill/bootout is restricted to this test's unique com.darkbot.test.* jobs.
No production HOME, labels, sockets, credentials or provider calls are used.
"""
import os, pathlib, plistlib, re, shutil, socket, socketserver, subprocess, tempfile, threading, time
from concurrent.futures import ThreadPoolExecutor
ROOT = pathlib.Path(__file__).resolve().parents[2]
WORK = pathlib.Path(tempfile.mkdtemp(prefix='darkbot-test.', dir='/private/tmp'))
(WORK/'.isolated').touch()
CONNECTIONS = []
class Solana(socketserver.BaseRequestHandler):
    """The configured Solana endpoint: an unapproved capture never connects to it (DEC-07)."""
    def handle(self):
        CONNECTIONS.append(time.time())
class Server(socketserver.ThreadingTCPServer):
    daemon_threads=True
server=Server(('127.0.0.1',0),Solana);threading.Thread(target=server.serve_forever,daemon=True).start()
with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
env={k:v for k,v in os.environ.items() if not k.startswith(('P0_','HELIUS_','DARKBOT_'))}
env.update(DARKBOT_TEST_DIR=str(WORK),DARKBOT_TEST_PORT=str(port))
label=f'com.darkbot.test.{WORK.name.split(".")[-1]}'
domain=f'gui/{os.getuid()}'
def run(*args, ok=True):
    p=subprocess.run([str(ROOT/'ops/darkbotctl'),*args],env=env,text=True,capture_output=True,timeout=350)
    print('$ darkbotctl',*args,'\n'+p.stdout+p.stderr,flush=True)
    if ok and p.returncode:raise AssertionError((args,p.returncode))
    return p

def pid(kind='capture'):
    p=subprocess.run(['launchctl','list',label+'.'+kind],capture_output=True,text=True)
    m=re.search(r'"PID" = (\d+);',p.stdout)
    return int(m[1]) if m else None

def wait(fn, timeout=30):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        value=fn()
        if value:return value
        time.sleep(.1)
    raise AssertionError('timeout waiting for '+str(fn))

def launch():return (WORK/'run/capture.launch').read_text().strip()
def gen():return int((WORK/'run/p0.generation').read_text()) if (WORK/'run/p0.generation').exists() else 0
def result():return dict(l.split('=',1) for l in (WORK/'run/capture.result').read_text().splitlines())
def browser(mode):
    p=subprocess.run(['node',str(ROOT/'ops/tests/lifecycle_browser.mjs'),str(port),str(WORK),mode],text=True,capture_output=True,timeout=40)
    print(p.stdout+p.stderr,flush=True);assert p.returncode==0

if __name__ == "__main__":
    try:
        release=WORK/'app/releases/test';release.mkdir(parents=True)
        for name,source in [('darkbot-gateway','gateway/target/debug/darkbot-gateway'),('p0-market-capture-reported','project_0/native/target/debug/p0-market-capture-reported')]:shutil.copy2(ROOT/source,release/name)
        shutil.copytree(ROOT/'frontend/dist',release/'dist')
        (WORK/'app/current').symlink_to(release,target_is_directory=True)
        (WORK/'app.conf').write_text('P0_APP_AUTOSTART_CAPTURE=1\nP0_APP_CAPTURE_SECS=until-stopped\n')
        (WORK/'capture.env').write_text(f'P0_WS_URL=ws://127.0.0.1:{server.server_address[1]}\nP0_PROGRAM_ID=11111111111111111111111111111111\nP0_MARKET_SOURCE=logs\nP0_MARKET_OUT_DIR={WORK}/data\nP0_MARKET_READY_TIMEOUT_SECS=4\nP0_MARKET_DRAIN_TIMEOUT_SECS=3\nP0_MARKET_HEARTBEAT_SECS=1\n')
        # DEC-07: the raw capture reads Mainnet only admitted to its approved boot flow (mode
        # market). The autostart and every unapproved start are refused, and nothing is read; the
        # admitted start (and with it the run lifecycle: drain, crash attribution, 429 cooldown)
        # stays offline NOT_RUN.
        out=run('app');assert 'AUTOSTART_BOOT_REQUIRED' in out.stdout+out.stderr
        wait(lambda:pid('gateway'));assert pid() is None and not (WORK/'run/capture.launch').exists()
        with ThreadPoolExecutor(max_workers=4) as pool:refused=list(pool.map(lambda _:run('capture','start','until-stopped',ok=False),range(4)))
        assert all(r.returncode and 'BOOT_AUDIT_REQUIRED' in r.stdout+r.stderr for r in refused)
        assert pid() is None and not (WORK/'run/capture.launch').exists()
        browser('gate')
        # The capture refuses itself under launchd: a start naming an admission record that does
        # not exist reaches the binary, which ends before any read with its own code.
        forged=dict(env,P0_MODE='market',P0_NETWORK_AUDIT=str(WORK/'absent.p0j'),P0_NETWORK_AUDIT_TOKEN='0123456789abcdef',
                    P0_NETWORK_AUDIT_APPROVED_AT=str(int(time.time())),P0_MEASUREMENT_DURATION='1m',P0_PROJECT_DIR=str(WORK/'app'))
        p=subprocess.run([str(ROOT/'ops/darkbotctl'),'capture','start','60'],env=forged,text=True,capture_output=True,timeout=60)
        print('$ darkbotctl capture start 60 (unknown record)\n'+p.stdout+p.stderr,flush=True)
        assert p.returncode and 'BOOT_NETWORK_EVIDENCE_INVALID' in p.stdout+p.stderr
        assert result()['code']=='BOOT_NETWORK_EVIDENCE_INVALID' and result()['launch_id']==launch()
        wait(lambda:pid() is None)
        plist=plistlib.loads((WORK/'run/capture.plist').read_bytes());assert plist['KeepAlive'] is False
        assert plist['EnvironmentVariables']['P0_NETWORK_AUDIT']==str(WORK/'absent.p0j')
        assert (WORK/'run/capture.plist').stat().st_mode&0o777==0o600
        assert (WORK/'run/capture.result').stat().st_mode&0o777==0o600
        # Gateway crash/restart starts no capture.
        gateway=pid('gateway')
        subprocess.run(['launchctl','kill','SIGKILL',domain+'/'+label+'.gateway'],check=True)
        run('up','--no-open');assert wait(lambda:pid('gateway') not in (None,gateway)) and pid() is None
        browser('gate')
        # A stop without a capture is a no-op that still suspends the next autostart.
        run('capture','stop');marker=WORK/'run/capture.user_stopped';assert marker.is_file()
        out=run('app');assert pid() is None and marker.is_file()
        run('down');assert pid() is None and pid('gateway') is None and marker.is_file()
        refused=run('capture','start','until-stopped',ok=False);assert refused.returncode and 'BOOT_AUDIT_REQUIRED' in refused.stdout+refused.stderr
        assert CONNECTIONS==[], 'a refused start never reads the network'
        print('PASS: autostart and every unapproved start refused (BOOT_AUDIT_REQUIRED), the capture refuses an unknown record itself under launchd, gateway crash/restart and down start nothing, no network read; admitted run lifecycle NOT_RUN offline',flush=True)
        (WORK/'PASS').write_text('all lifecycle checks passed\n')
    finally:
        run('down',ok=False)
        for kind in ('capture','gateway'):
            subprocess.run(['launchctl','bootout',domain+'/'+label+'.'+kind],capture_output=True)
        server.shutdown()
        print('Isolated evidence:',WORK,flush=True)
