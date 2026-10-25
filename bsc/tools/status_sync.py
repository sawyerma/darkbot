#!/usr/bin/env python3
"""Explicit desktop-to-runtime public snapshot. Never copy secrets or rewrite evidence.
GUI results retain bsc/native/evidence/g07/gui_readiness_<UUID>/ paths on import.
Their typed provenance (provenance.dbrec, checked by the native reporter) declares
READ_ONLY_OPERATIONAL, not gate qualification. No internal JSON (BSC-048).
GUI candidate requests are applied or rejected by the native store first; only the
public candidate/selection records are mirrored, never a salt record.
"""
import hashlib, os, pathlib, shutil, subprocess, sys, time, uuid

def digest(data): return hashlib.sha256(data).hexdigest()
def regular(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 16*1024*1024:
        raise ValueError('unsafe source: '+str(path))
    return path.read_bytes()
CANDIDATE_STATE='bsc/.local/g07_candidates'
CANDIDATE_MIRROR='bsc/native/state/g07_candidates'
CANDIDATE_PUBLIC=['a.candidate.dbrec','b.candidate.dbrec','selection.dbrec','a.qualification.dbrec','b.qualification.dbrec']
def apply_candidate_requests(root, run):
    requests=run/'bsc-operational/g07_candidates/requests'
    pending=[p for p in sorted(requests.glob('*.request.dbrec')) if not p.with_name(p.name.replace('.request.','.result.')).exists()] if requests.is_dir() else []
    if not pending: return
    binary=root/'bsc/engine/target/release/g07_candidate'
    if not binary.is_file(): raise ValueError('pending candidate request but g07_candidate is not built')
    # The native store is the only authority; its decision is recorded per request.
    done=subprocess.run([str(binary),'apply-requests','--rundir',str(run)],cwd=root,env={'PATH':'/usr/bin:/bin'},capture_output=True,text=True,timeout=60)
    if done.returncode!=0: raise ValueError('candidate request apply failed: '+done.stderr.strip()[-300:])
    for line in done.stdout.splitlines(): print(line)
def sync(root, app, run):
    root=root.resolve(); base=app/'bsc-snapshots'; base.mkdir(parents=True,exist_ok=True)
    apply_candidate_requests(root, run)
    # Import immutable operational results, preserving every relative path and byte.
    operational=run/'bsc-operational'
    for directory in sorted((operational/'bsc/native/evidence/g07').glob('gui_readiness_*')):
        if directory.is_symlink(): raise ValueError('symlink result')
        reporter=root/'bsc/engine/target/release/bsc-status-reporter'
        if not reporter.is_file(): raise ValueError('pending GUI readiness result but bsc-status-reporter is not built')
        # The native import gate: the result and exactly its own read-only provenance.
        checked=subprocess.run([str(reporter),'--check-readiness-provenance',str(directory)],env={'PATH':'/usr/bin:/bin','DARKBOT_BSC_ROOT':str(root),'DARKBOT_RUNDIR':str(run)},capture_output=True,timeout=10)
        if checked.returncode!=0: raise ValueError('invalid result provenance')
        raw=regular(directory/'readiness.dbrec')
        target=root/directory.relative_to(operational)
        if target.exists():
            if regular(target/'readiness.dbrec')!=raw or regular(target/'provenance.dbrec')!=regular(directory/'provenance.dbrec'): raise ValueError('immutable evidence conflict')
        else:
            target.parent.mkdir(parents=True,exist_ok=True)
            staging=target.with_name('.'+target.name+'.'+uuid.uuid4().hex)
            staging.mkdir(mode=0o700)
            for name in ['readiness.dbrec','provenance.dbrec']: (staging/name).write_bytes(regular(directory/name))
            os.rename(staging,target)
    dest=base/uuid.uuid4().hex; dest.mkdir(mode=0o700)
    files=[]
    for parent in ['bsc/native/evidence/g06a','bsc/native/evidence/g07','bsc/native/evidence/g08']:
        for path in sorted((root/parent).rglob('*.dbrec')):
            if any(p.is_symlink() for p in path.parents if p!=root): raise ValueError('symlink source')
            files.append(path)
    for name in ['bsc/config/g07_attempt1_acceptance_v4.dbrec','bsc/config/g07_attempt1_acceptance_v5.dbrec','bsc/config/wallet.public.env','bsc/config/readiness.public.env','bsc/config/ingress.public.env','bsc/tools/readiness_job.py']:
        path=root/name
        if path.exists(): files.append(path)
    # BSC-050 A-scan: exactly the files the v5 contract binds, as the native readiness names them.
    if (root/'bsc/config/g07_attempt1_acceptance_v5.dbrec').exists():
        lister=root/'bsc/engine/target/release/g07_pre_live_readiness'
        if not lister.is_file(): raise ValueError('a v5 contract exists but g07_pre_live_readiness is not built')
        listed=subprocess.run([str(lister),'--bound-inputs'],cwd=root,env={'PATH':'/usr/bin:/bin'},capture_output=True,text=True,timeout=60)
        if listed.returncode!=0: raise ValueError('the v5-bound inputs cannot be listed: '+listed.stderr.strip()[-300:])
        for name in listed.stdout.splitlines():
            relative=pathlib.PurePosixPath(name)
            if relative.is_absolute() or '..' in relative.parts or not name: raise ValueError('unsafe bound input: '+name)
            path=root
            for part in relative.parts:
                path=path/part
                if path.is_symlink(): raise ValueError('bound input linked: '+name)
            if not path.is_file(): raise ValueError('bound input missing: '+name)
            files.append(path)
    files=list(dict.fromkeys(files))
    manifest={}
    for path in files:
        relative=path.relative_to(root); data=regular(path)
        target=dest/relative; target.parent.mkdir(parents=True,exist_ok=True); target.write_bytes(data)
        manifest[str(relative)]=digest(data)
    for name in CANDIDATE_PUBLIC:
        path=root/CANDIDATE_STATE/name
        if not path.exists(): continue
        data=regular(path); target=dest/CANDIDATE_MIRROR/name
        target.parent.mkdir(parents=True,exist_ok=True); target.write_bytes(data)
        manifest[CANDIDATE_MIRROR+'/'+name]=digest(data)
    commit=subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip()
    if subprocess.check_output(['git','-C',str(root),'status','--porcelain','--','bsc'],text=True).strip(): commit+='-dirty'
    (dest/'COMMIT').write_text(commit+'\n'); (dest/'SYNCED_AT').write_text(str(int(time.time())))
    # The main checkout whose attempt latch and shared authority the GUI readiness reads.
    (dest/'CHECKOUT').write_text(str(root)+'\n')
    # The snapshot manifest in the evidence SHA256SUMS format (BSC-048: no JSON); its hash is the snapshot identity.
    raw=''.join(f'{manifest[name]}  ./{name}\n' for name in sorted(manifest)).encode()
    (dest/'SOURCES.SHA256SUMS').write_bytes(raw); (dest/'SNAPSHOT_SHA256').write_text(digest(raw))
    link=app/('.bsc-source-'+uuid.uuid4().hex); link.symlink_to(dest); os.replace(link,app/'bsc-source')
    print('BSC snapshot synced:',len(files),'public files; source',commit)
if __name__=='__main__':
    os.umask(0o077)
    sync(*(pathlib.Path(p) for p in sys.argv[1:4]))
