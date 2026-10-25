#!/usr/bin/env python3
"""One read-only native readiness run. TERM forwards to its own child process group.
No key loading, signing, dry run, builder call, forced kill or implicit retry.
"""
import os, pathlib, signal, subprocess, sys, time

root = pathlib.Path(os.environ['DARKBOT_BSC_ROOT']).resolve()
rundir = pathlib.Path(os.environ['DARKBOT_RUNDIR'])
launch = os.environ['DARKBOT_BSC_LAUNCH_ID']
binary = os.environ['DARKBOT_BSC_READINESS_BIN']
assert launch and all(c.isalnum() or c == '-' for c in launch)
stopping = False
child = None

def stop(_signal, _frame):
    global stopping
    stopping = True
    if child is not None and child.poll() is None:
        try: os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError: pass

signal.signal(signal.SIGTERM, stop)
signal.signal(signal.SIGINT, stop)
os.umask(0o077)
rundir.mkdir(parents=True, exist_ok=True)
temporary = rundir / ('bsc.readiness.' + launch + '.tmp')
result = 'FAILED'
exit_code = 1
verdict = None
try:
    # Exact allowlist: never inherit a signing key, RPC token or builder authorization.
    env = {'PATH': '/usr/bin:/bin:/usr/sbin:/sbin',
           'BSC_MAINNET_RPC_HTTPS': 'https://bsc-dataseed.bnbchain.org'}
    with temporary.open('xb') as output:
        child = subprocess.Popen([binary, '--current-rpc', '--binary'], cwd=root,
                                 env=env, stdout=output, start_new_session=True)
        if stopping: stop(signal.SIGTERM, None)
        exit_code = child.wait()
        output.flush()
        os.fsync(output.fileno())
    if not stopping and 0 < temporary.stat().st_size <= 16 * 1024 * 1024:
        with temporary.open('rb') as data: valid = data.read(8) == b'DBREC001'
        if valid:
            validator=pathlib.Path(binary).with_name('bsc-status-reporter')
            checked=subprocess.run([str(validator),'--validate-readiness',str(temporary)],env={'PATH':'/usr/bin:/bin','DARKBOT_BSC_ROOT':str(root),'DARKBOT_RUNDIR':str(rundir)},capture_output=True,timeout=10)
            valid=checked.returncode==0
            if valid:
                # The typed native verdict is the result; it must agree with the exit code
                # (0 only for PASS_READ_ONLY, 2 for BLOCKED/STOP), otherwise the runner failed.
                typed=dict(l.split('=',1) for l in checked.stdout.decode().splitlines() if '=' in l)
                verdict=typed.get('status')
                valid=verdict in ('PASS_READ_ONLY','BLOCKED','STOP') and exit_code==(0 if verdict=='PASS_READ_ONLY' else 2)
        if valid:
            final_directory = rundir / 'bsc-operational/bsc/native/evidence/g07' / ('gui_readiness_' + launch)
            directory = final_directory.with_name('.' + final_directory.name)
            directory.mkdir(parents=True, exist_ok=False)
            os.replace(temporary, directory / 'readiness.dbrec')
            # BSC-048: the native reporter seals the typed provenance (provenance.dbrec, no JSON).
            sealed=subprocess.run([str(validator),'--seal-readiness-provenance',str(directory),launch,
                                   str(final_directory.relative_to(rundir/'bsc-operational'))],
                                  env={'PATH':'/usr/bin:/bin','DARKBOT_BSC_ROOT':str(root),'DARKBOT_RUNDIR':str(rundir)},capture_output=True,timeout=10)
            if sealed.returncode!=0: raise ValueError('provenance not sealed')
            fd=os.open(directory,os.O_RDONLY)
            try: os.fsync(fd)
            finally: os.close(fd)
            os.rename(directory,final_directory)
            fd=os.open(final_directory.parent,os.O_RDONLY)
            try: os.fsync(fd)
            finally: os.close(fd)
            result = verdict
    if stopping: result = 'STOPPED'
except Exception as error:
    # Exception type is diagnostic; never echo an environment or child payload.
    print('READINESS_RUNNER_FAILED:', type(error).__name__, file=sys.stderr)
finally:
    temporary.unlink(missing_ok=True)
    target = rundir / 'bsc.readiness.result'
    tmp = rundir / ('bsc.readiness.result.' + launch + '.tmp')
    with tmp.open('x') as file:
        file.write(f'launch_id={launch}\nended_unix_s={int(time.time())}\nresult_code={result}\nexit_code={exit_code}\n')
        file.flush(); os.fsync(file.fileno())
    os.replace(tmp, target)
    fd = os.open(rundir, os.O_RDONLY)
    try: os.fsync(fd)
    finally: os.close(fd)
    print(f'BSC_READINESS_RESULT={result} launch_id={launch}', flush=True)
sys.exit(0 if stopping else (0 if exit_code == 0 and result == 'PASS_READ_ONLY' else 2))
