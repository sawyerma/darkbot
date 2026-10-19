#!/usr/bin/env python3
"""macOS launchd integration, real capture/gateway, local-only fake Solana.
Every kill/bootout is restricted to this test's unique com.darkbot.test.* jobs.
No production HOME, labels, sockets, credentials or provider calls are used.
"""
import base64, hashlib, json, os, pathlib, plistlib, re, select, shutil, socket, socketserver, struct, subprocess, tempfile, threading, time
from concurrent.futures import ThreadPoolExecutor
ROOT = pathlib.Path(__file__).resolve().parents[2]
WORK = pathlib.Path(tempfile.mkdtemp(prefix='darkbot-test.', dir='/private/tmp'))
(WORK/'.isolated').touch()
RATE_LIMIT = threading.Event()
class Solana(socketserver.BaseRequestHandler):
    def handle(self):
        s = self.request; s.settimeout(3)
        def read(n):
            out=b''
            while len(out)<n:
                part=s.recv(n-len(out))
                if not part: raise EOFError()
                out+=part
            return out
        def send(obj, opcode=1):
            data=json.dumps(obj).encode() if opcode==1 else obj
            s.sendall(bytes([128|opcode])+(bytes([len(data)]) if len(data)<126 else b'\x7e'+struct.pack('!H',len(data)))+data)
        try:
            header=b''
            while not header.endswith(b'\r\n\r\n'): header+=read(1)
            if RATE_LIMIT.is_set():
                s.sendall(b'HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n'); return
            key=re.search(br'Sec-WebSocket-Key:\s*(\S+)',header,re.I)[1]
            accept=base64.b64encode(hashlib.sha1(key+b'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest())
            s.sendall(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+b'\r\n\r\n')
            subs={}; slot=100; tick=time.monotonic()
            while True:
                if select.select([s],[],[],0.05)[0]:
                    a,b=read(2); n=b&127
                    if n==126:n=struct.unpack('!H',read(2))[0]
                    if n==127:n=struct.unpack('!Q',read(8))[0]
                    mask=read(4) if b&128 else None; data=read(n)
                    if mask:data=bytes(c^mask[i%4] for i,c in enumerate(data))
                    if a&15==8: send(data,8);return
                    if a&15==9:send(data,10);continue
                    if a&15!=1:continue
                    msg=json.loads(data);method=msg.get('method','')
                    if method.endswith('Subscribe'):
                        subs[method]=msg['id'];send({'jsonrpc':'2.0','id':msg['id'],'result':msg['id']})
                if time.monotonic()-tick<0.15:continue
                tick=time.monotonic();slot+=1
                if 'slotSubscribe' in subs:send({'jsonrpc':'2.0','method':'slotNotification','params':{'subscription':subs['slotSubscribe'],'result':{'slot':slot,'parent':slot-1,'root':slot-2}}})
                if 'logsSubscribe' in subs:send({'jsonrpc':'2.0','method':'logsNotification','params':{'subscription':subs['logsSubscribe'],'result':{'context':{'slot':slot},'value':{'signature':('1'*80)+str(slot).replace('0','2'),'err':None,'logs':[]}}}})
        except (OSError, EOFError, ValueError):pass
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

try:
    release=WORK/'app/releases/test';release.mkdir(parents=True)
    for name,source in [('darkbot-gateway','gateway/target/debug/darkbot-gateway'),('p0-market-capture-reported','project_0/native/target/debug/p0-market-capture-reported')]:shutil.copy2(ROOT/source,release/name)
    shutil.copytree(ROOT/'frontend/dist',release/'dist')
    (WORK/'app/current').symlink_to(release,target_is_directory=True)
    (WORK/'app.conf').write_text('P0_APP_AUTOSTART_CAPTURE=1\nP0_APP_CAPTURE_SECS=until-stopped\n')
    (WORK/'capture.env').write_text(f'P0_WS_URL=ws://127.0.0.1:{server.server_address[1]}\nP0_PROGRAM_ID=11111111111111111111111111111111\nP0_MARKET_SOURCE=logs\nP0_MARKET_OUT_DIR={WORK}/data\nP0_MARKET_READY_TIMEOUT_SECS=4\nP0_MARKET_DRAIN_TIMEOUT_SECS=3\nP0_MARKET_HEARTBEAT_SECS=1\n')
    run('app');first=wait(pid);firstlaunch=launch();firstgen=wait(lambda:gen())
    with ThreadPoolExecutor(max_workers=4) as pool:list(pool.map(lambda _:run('capture','start','until-stopped'),range(4)))
    assert pid()==first and launch()==firstlaunch and gen()==firstgen
    plist=plistlib.loads((WORK/'run/capture.plist').read_bytes());assert plist['KeepAlive'] is False
    assert (WORK/'run/capture.plist').stat().st_mode&0o777==0o600
    browser('live')
    # Gateway crash/restart cannot change the capture run/generation.
    subprocess.run(['launchctl','kill','SIGKILL',domain+'/'+label+'.gateway'],check=True)
    run('up','--no-open');assert pid()==first and gen()==firstgen
    browser('live')
    stop=subprocess.Popen([str(ROOT/'ops/darkbotctl'),'capture','stop'],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    wait(lambda:(WORK/'run/capture.stopping').exists() or stop.poll() is not None)
    run('capture','start','until-stopped')
    print(stop.communicate(timeout=35)[0]);assert stop.returncode==0
    if launch()!=firstlaunch: # If first run already ended, old waiter must not wait on this new run.
        assert pid();run('capture','stop')
    assert (WORK/'app/data/market'/('app-'+firstlaunch)/'market_completion.p0j').exists()
    assert result()['launch_id']==launch()
    assert (WORK/'run/capture.result').stat().st_mode&0o777==0o600
    # B0: after user stop app must not start; only an explicit start can lift it.
    marker=WORK/'run/capture.user_stopped'
    assert marker.is_file(), 'manual stop marker required'
    stopped_launch=launch()
    run('app')
    assert pid() is None and launch()==stopped_launch and marker.is_file()
    run('capture','start','until-stopped')
    assert pid() and launch()!=stopped_launch and not marker.exists()
    crashlaunch=launch();crashgen=gen();wait(pid)
    subprocess.run(['launchctl','kill','SIGKILL',domain+'/'+label+'.capture'],check=True)
    wait(lambda:pid() is None)
    assert result()['launch_id']!=crashlaunch # Never attribute previous result to crashed run.
    run('app');wait(lambda:gen()>crashgen);assert launch()!=crashlaunch
    down=subprocess.Popen([str(ROOT/'ops/darkbotctl'),'down'],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
    wait(lambda:(WORK/'run/lifecycle.down').exists())
    rejected=run('capture','start','until-stopped',ok=False);assert rejected.returncode!=0
    print(down.communicate(timeout=35)[0]);assert down.returncode==0 and pid() is None and pid('gateway') is None
    assert marker.is_file(), 'down must suspend autostart'
    down_launch=launch()
    run('app');assert pid() is None and launch()==down_launch and marker.is_file()
    # Fresh explicit start removes the marker, even when Gateway was stopped.
    run('capture','start','until-stopped')
    assert pid() and not marker.exists() and launch()!=down_launch
    subprocess.run(['launchctl','kill','SIGKILL',domain+'/'+label+'.capture'],check=True)
    wait(lambda:pid() is None)
    RATE_LIMIT.set();run('app');limitedlaunch=launch();browser('429');wait(lambda:pid() is None,35)
    assert result()['launch_id']==limitedlaunch
    assert result()['code']=='MARKET_OBSERVER_STREAM_NOT_READY'
    run('app');assert launch()==limitedlaunch and pid() is None
    run('capture','stop');assert marker.is_file() # no-op stop also suspends next app
    run('app');assert pid() is None and launch()==limitedlaunch and marker.is_file()
    print('PASS: concurrent start, gateway crash, manual-stop autostart suppression, fresh explicit start, down/start exclusion, 429 and cooldown',flush=True)
    (WORK/'PASS').write_text('all lifecycle checks passed\n')
finally:
    run('down',ok=False)
    for kind in ('capture','gateway'):
        subprocess.run(['launchctl','bootout',domain+'/'+label+'.'+kind],capture_output=True)
    server.shutdown()
    print('Isolated evidence:',WORK,flush=True)
