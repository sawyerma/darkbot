#!/usr/bin/env python3
"""Bind one coordinated build to its schema and exact binaries/frontend assets."""
import hashlib,json,pathlib,sys
root=pathlib.Path(__file__).resolve().parents[1]
target=root/'gateway/target/darkbot-release.json'
files=['gateway/target/release/darkbot-gateway','project_0/native/target/release/p0-market-capture-reported','project_0/native/target/release/p0ctl','project_0/native/target/release/p0-analytics-reporter','bsc/engine/target/release/bsc-status-reporter','bsc/engine/target/release/g07_pre_live_readiness','bsc/engine/target/release/g07_candidate','bsc/engine/target/release/bsc_runtime','ops/darkbotctl','ops/bsc-control.sh','bsc/tools/readiness_job.py']
# BSC REV.1.0 R8: the operator's G0.7/G0.8 tools, the record viewer, the status sync and the BSC configs.
files += ['bsc/engine/target/release/'+b for b in ('g07_mainnet_self_launch_buy','g07_builder_probe','g07_attempt_authority','g08_mainnet_verify','dbrec_show')]
files += ['bsc/tools/status_sync.py'] + sorted(str(p.relative_to(root)) for p in (root/'bsc/config').glob('*.env'))
files += sorted(str(p.relative_to(root)) for p in (root/'frontend/dist').rglob('*') if p.is_file())
def sha(name):
    h=hashlib.sha256()
    with (root/name).open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
    return h.hexdigest()
state={'schema_id':(root/'protocol/schema_id.txt').read_text().strip(),'files':{name:sha(name) for name in files}}
if '--write' in sys.argv:
    target.write_text(json.dumps(state,indent=2)+'\n')
else:
    if json.loads(target.read_text())!=state:raise SystemExit('Release changed or stale: run darkbotctl build before deploy')
print('Coordinated release manifest: PASS')
