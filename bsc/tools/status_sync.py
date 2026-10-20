#!/usr/bin/env python3
"""Explicit desktop-to-runtime public snapshot. Never copy secrets or rewrite evidence.
GUI results retain bsc/native/evidence/g07/gui_readiness_<UUID>/ paths on import.
Their provenance declares READ_ONLY_OPERATIONAL, not gate qualification.
"""
import hashlib, json, os, pathlib, shutil, subprocess, sys, time, uuid

def digest(data): return hashlib.sha256(data).hexdigest()
def regular(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 16*1024*1024:
        raise ValueError('unsafe source: '+str(path))
    return path.read_bytes()
def sync(root, app, run):
    root=root.resolve(); base=app/'bsc-snapshots'; base.mkdir(parents=True,exist_ok=True)
    # Import immutable operational results, preserving every relative path and byte.
    operational=run/'bsc-operational'
    for directory in sorted((operational/'bsc/native/evidence/g07').glob('gui_readiness_*')):
        if directory.is_symlink(): raise ValueError('symlink result')
        meta=json.loads(regular(directory/'provenance.json'))
        raw=regular(directory/'readiness.dbrec')
        if meta['kind']!='READ_ONLY_OPERATIONAL' or meta['result_sha256']!=digest(raw): raise ValueError('invalid result provenance')
        target=root/directory.relative_to(operational)
        if target.exists():
            if regular(target/'readiness.dbrec')!=raw or regular(target/'provenance.json')!=regular(directory/'provenance.json'): raise ValueError('immutable evidence conflict')
        else:
            target.parent.mkdir(parents=True,exist_ok=True)
            staging=target.with_name('.'+target.name+'.'+uuid.uuid4().hex)
            staging.mkdir(mode=0o700)
            for name in ['readiness.dbrec','provenance.json']: (staging/name).write_bytes(regular(directory/name))
            os.rename(staging,target)
    dest=base/uuid.uuid4().hex; dest.mkdir(mode=0o700)
    files=[]
    for parent in ['bsc/native/evidence/g06a','bsc/native/evidence/g07','bsc/native/evidence/g08']:
        for path in sorted((root/parent).rglob('*.dbrec')):
            if any(p.is_symlink() for p in path.parents if p!=root): raise ValueError('symlink source')
            files.append(path)
    for pattern in ['builder_requirements_refresh_*.json','*/probe.evidence.json']:
        files.extend(sorted((root/'bsc/native/evidence/g06a').glob(pattern)))
    for name in ['bsc/config/g07_attempt1_acceptance_v4.dbrec','bsc/config/wallet.public.env','bsc/tools/readiness_job.py']:
        path=root/name
        if path.exists(): files.append(path)
    manifest={}
    for path in files:
        relative=path.relative_to(root); data=regular(path)
        target=dest/relative; target.parent.mkdir(parents=True,exist_ok=True); target.write_bytes(data)
        manifest[str(relative)]=digest(data)
    commit=subprocess.check_output(['git','-C',str(root),'rev-parse','HEAD'],text=True).strip()
    if subprocess.check_output(['git','-C',str(root),'status','--porcelain','--','bsc'],text=True).strip(): commit+='-dirty'
    (dest/'COMMIT').write_text(commit+'\n'); (dest/'SYNCED_AT').write_text(str(int(time.time())))
    raw=json.dumps(manifest,sort_keys=True,separators=(',',':')).encode()
    (dest/'sources.json').write_bytes(raw); (dest/'SNAPSHOT_SHA256').write_text(digest(raw))
    link=app/('.bsc-source-'+uuid.uuid4().hex); link.symlink_to(dest); os.replace(link,app/'bsc-source')
    print('BSC snapshot synced:',len(files),'public files; source',commit)
if __name__=='__main__':
    os.umask(0o077)
    sync(*(pathlib.Path(p) for p in sys.argv[1:4]))
