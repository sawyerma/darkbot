#!/usr/bin/env python3
"""One isolated macOS/BSC package run: real reporter/gateway/launchd/browser.
The first readiness job uses the native binary with a missing contract (read-only public RPC, no egress).
Cancellation/crash use a fixture child. Production labels and files are untouched.
"""
import os, pathlib, shutil, subprocess
import lifecycle_test as h
# The shipped binaries of one build profile: debug by default, release for the
# operational acceptance (the artifacts of `darkbotctl build`); dbrec_show is a test helper.
profile=os.environ.get('DARKBOT_E2E_PROFILE','debug')
assert profile in ('debug','release'), profile
runtime=h.ROOT/f'bsc/engine/target/{profile}/bsc_runtime'
try:
    release=h.WORK/'app/releases/test';release.mkdir(parents=True)
    for name,source in [('darkbot-gateway',f'gateway/target/{profile}/darkbot-gateway'),('darkbotctl','ops/darkbotctl'),('bsc-control.sh','ops/bsc-control.sh'),('bsc-status-reporter',f'bsc/engine/target/{profile}/bsc-status-reporter'),('g07_pre_live_readiness',f'bsc/engine/target/{profile}/g07_pre_live_readiness'),('bsc_runtime',f'bsc/engine/target/{profile}/bsc_runtime')]:
        shutil.copy2(h.ROOT/source,release/name)
    (release/'darkbotctl').chmod(0o755)
    shutil.copytree(h.ROOT/'frontend/dist',release/'dist')
    source=h.WORK/'app/snapshot';(source/'bsc/tools').mkdir(parents=True)
    shutil.copy2(h.ROOT/'bsc/tools/readiness_job.py',source/'bsc/tools/readiness_job.py')
    # The job's public read-only endpoint and clock, as bsc_sync copies them into every snapshot.
    (source/'bsc/config').mkdir(parents=True)
    shutil.copy2(h.ROOT/'bsc/config/readiness.public.env',source/'bsc/config/readiness.public.env')
    # The runtime's real configuration; only the census interval is shortened for the test.
    ingress=(h.ROOT/'bsc/config/ingress.public.env').read_text()
    assert 'BSC_RUNTIME_CENSUS_MS=60000\n' in ingress
    # The C3 PAPER identity: a fresh local key of this run, outside the snapshot (never funded).
    assert 'BSC_EXEC_PAPER_IDENTITY=~/.darkbot/bsc/paper_identity.key\n' in ingress
    paper_key=h.WORK/'paper/paper_identity.key'
    ingress=ingress.replace('BSC_EXEC_PAPER_IDENTITY=~/.darkbot/bsc/paper_identity.key\n',f'BSC_EXEC_PAPER_IDENTITY={paper_key}\n')
    (source/'bsc/config/ingress.public.env').write_text(ingress.replace('BSC_RUNTIME_CENSUS_MS=60000\n','BSC_RUNTIME_CENSUS_MS=5000\n'))
    # Phase H: Strategy C's frozen gas evidence, as bsc_sync copies it into every snapshot.
    for name in next(l.split('=',1)[1] for l in ingress.splitlines() if l.startswith('BSC_STRATEGY_C_GAS_EVIDENCE=')).split(','):
        (source/name).parent.mkdir(parents=True,exist_ok=True);shutil.copy2(h.ROOT/name,source/name)
    paper=subprocess.run([str(runtime),'paper-identity','init','--config',str(source/'bsc/config/ingress.public.env')],capture_output=True,text=True,check=True).stdout
    assert paper.startswith('paper_sender=0x') and len(paper.strip())==len('paper_sender=0x')+40, paper
    (source/'COMMIT').write_text('isolated-fixture');(source/'SYNCED_AT').write_text('1792232000');(source/'SNAPSHOT_SHA256').write_text('a'*64)
    evidence=source/'bsc/native/evidence/g07/fixture';evidence.mkdir(parents=True)
    shutil.copy2(h.ROOT/'protocol/golden/bsc/readiness.dbrec',evidence/'readiness.dbrec')
    # Committed native qualification records, so every panel shows real public leaves end to end.
    # The committed C1.9 qualification run (binary records), as the sync copies it (BSC-023).
    c1=source/'bsc/native/evidence/c1';c1.mkdir(parents=True)
    for record in (h.ROOT/'bsc/native/evidence/c1').glob('c1-9-native-latency-20261025T234144*.dbrec'):shutil.copy2(record,c1/record.name)
    for name in ('g06a_prepare_20261023T211704Z','g07_binary_import_20261016'):
        target=source/'bsc/native/evidence/g06a'/name;target.mkdir(parents=True)
        for record in (h.ROOT/'bsc/native/evidence/g06a'/name).glob('*.dbrec'):shutil.copy2(record,target/record.name)
    # The raw salt of the economics record must never reach the browser (census SECRET_NEVER_BROWSER).
    shown=subprocess.run([str(h.ROOT/'bsc/engine/target/debug/dbrec_show'),str(source/'bsc/native/evidence/g06a/g06a_prepare_20261023T211704Z/economics_crosscheck.dbrec')],capture_output=True,text=True,check=True).stdout
    (h.WORK/'salt.txt').write_text(next(l.split('=',1)[1].strip() for l in shown.splitlines() if l.startswith('salt =')))
    # V2 §2.3: the real G0.8 verifier on a never-sent G0.7 attempt, against the free
    # read-only RPC of readiness.public.env, finds no canonical receipt: its UNRESOLVED
    # record goes into the snapshot and must reach every tab unchanged.
    g08=source/'bsc/native/evidence/g08/never_sent';g08.mkdir(parents=True)
    rpc=next(l.split('=',1)[1] for l in (h.ROOT/'bsc/config/readiness.public.env').read_text().splitlines() if l.startswith('BSC_READINESS_RPC_HTTPS='))
    subprocess.run(['cargo','test','-q','-p','darkbot-bsc-qualification','--all-features','--test','g08_reads_g07_evidence','--','--ignored','--exact','g08_records_unresolved_for_a_never_sent_attempt_on_a_real_rpc'],cwd=h.ROOT/'bsc/engine',env={**os.environ,'BSC_MAINNET_RPC_HTTPS':rpc,'DARKBOT_G08_E2E_OUT':str(g08)},check=True,timeout=600)
    assert (g08/'unresolved.dbrec').is_file()
    (h.WORK/'app/bsc-source').symlink_to(source,target_is_directory=True)
    (release/'bsc-root').symlink_to(source,target_is_directory=True)
    (h.WORK/'app/current').symlink_to(release,target_is_directory=True)
    h.run('up','--no-open')
    subprocess.run(['node',str(h.ROOT/'ops/tests/bsc_control_browser.mjs'),str(h.port),str(h.WORK),str(h.ROOT)],check=True,timeout=180,env=h.env)
    # Paket 8 review L3: launchd restarts the runtime after a crash (SIGKILL); the restart is a new run
    # and the C2 owner restores its checkpoint.
    import time
    journal=h.WORK/'run/bsc-runtime/journal'
    runs=lambda: sorted(p.name for p in journal.iterdir() if p.is_dir())
    status=lambda: subprocess.run([str(h.ROOT/'bsc/engine/target/debug/dbrec_show'),str(h.WORK/'run/bsc-runtime/status.dbrec')],capture_output=True,text=True).stdout
    # The browser test left C2 and Strategy C BLOCKED (requested LIVE): SHADOW again, so the owner runs
    # and checkpoints, and the restarted run is the one the strategy replay below takes.
    gen=int(subprocess.run([str(runtime),'source','show','--state',str(h.WORK/'run/bsc-sources')],capture_output=True,text=True,check=True).stdout.splitlines()[0].split('=')[1])
    h.run('bsc','source','request','BSC_STATE_C2','SHADOW',str(gen),'e2e-c2-crash')
    h.run('bsc','source','request','BSC_STRATEGY_C','SHADOW',str(gen+1),'e2e-strategy-crash')
    # Plan 27.10.: a wallet switch ENABLED before the crash is DISABLED by the restarted runtime.
    h.run('bsc','source','request','BSC_WALLET_LAUNCHER','ENABLED',str(gen+2),'e2e-launcher-crash')
    show=lambda: subprocess.run([str(runtime),'source','show','--state',str(h.WORK/'run/bsc-sources')],capture_output=True,text=True,check=True).stdout
    assert 'BSC_WALLET_LAUNCHER requested=ENABLED effective=ENABLED' in show(), show()
    for _ in range(60):
        if 'state.checkpoint_writes' in status() and 'state.checkpoint_writes = 0' not in status(): break
        time.sleep(1)
    before=runs()
    subprocess.run(['launchctl','kill','SIGKILL',h.domain+'/'+h.label+'.bsc.runtime'],check=True)
    restored=''
    for _ in range(90):
        time.sleep(1)
        if len(runs())>len(before):
            restored=next((l for l in status().splitlines() if l.startswith('state.checkpoint_restore')),'')
            if 'RESTORED' in restored: break
    assert len(runs())>len(before) and 'RESTORED' in restored, (before, runs(), restored)
    for _ in range(30):
        if 'BSC_WALLET_LAUNCHER requested=DISABLED effective=DISABLED' in show(): break
        time.sleep(1)
    assert 'BSC_WALLET_LAUNCHER requested=DISABLED effective=DISABLED' in show(), show()
    print('PASS restart: the wallet switch ENABLED before the crash is DISABLED (RESTART_RESET)', flush=True)
    # Strategy C runs in the restarted run before it is stopped (the replay below needs its switch).
    for _ in range(30):
        if 'strategy.state = "SHADOW"' in status(): break
        time.sleep(1)
    assert 'strategy.state = "SHADOW"' in status(), status()
    print('PASS runtime crash: launchd restart, new run', runs()[-1], restored, flush=True)
    # Phase H: the finished run replays through Strategy C to exactly its journaled outputs.
    h.run('down')
    replay=subprocess.run([str(runtime),'strategy-replay','--rundir',str(h.WORK/'run'),'--config',str(source/'bsc/config/ingress.public.env')],capture_output=True,text=True,timeout=120)
    print(replay.stdout,flush=True)
    assert replay.returncode==0 and ': PASS (' in replay.stdout, replay.stdout+replay.stderr
finally:
    h.run('down',ok=False)
    for kind in ('bsc.readiness','bsc.reporter','bsc.runtime','gateway'):
        subprocess.run(['launchctl','bootout',h.domain+'/'+h.label+'.'+kind],capture_output=True)
    h.server.shutdown()
    print('Isolated BSC evidence:',h.WORK,'profile',profile,flush=True)
