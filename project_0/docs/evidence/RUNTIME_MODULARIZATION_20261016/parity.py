from pathlib import Path
import tarfile,re,difflib,json,collections,subprocess
out=Path(__import__('sys').argv[1]) if len(__import__('sys').argv)>1 else Path(__file__).parent;r=Path('/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot');base=Path('project_0/native/crates/runtime/src')
with tarfile.open(out/'source-before.tar.gz') as tar:
    names=tar.getnames();p=next(x for x in names if x.endswith('runtime/src/main.rs'));original=tar.extractfile(p).read().decode().replace('darkbot_runner::','crate::')
start=original.index('    loop {\n        mt0_owner.enter');end=original.index('    if let Some(h) = &econ {\n        let ws = winner_tracker.stats();',start)
body=original[start+len('    loop {\n'):end][:-6]
markers=[('lifecycle',0),('remote_apply',body.index('        // [FIX-108][R4] Apply completed')),('drain_gate',body.index('        if let Some(h) = &econ {',body.index('winner_tracker.dispatch_due'))),('dispatch',body.index('        if !pending.is_empty()')),('sweep',body.index('        mt0_owner.enter(darkbot_common::mt0::OwnerCat::Sweep)')),('events',body.index('        mt0_owner.enter(darkbot_common::mt0::OwnerCat::Wait)')),('detect',body.index('        mt0_owner.enter(darkbot_common::mt0::OwnerCat::Detect)')),('telemetry',body.index('        // [FIX-108][R4] Winner/Reconcile RPC'))]
parts={name:body[a:markers[i+1][1] if i+1<len(markers) else len(body)] for i,(name,a) in enumerate(markers)}
lex=re.compile(r'//[^\n]*|/\*[\s\S]*?\*/|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'|[A-Za-z_]\w*|[^\s]',re.M)
def norm(s):
    s=re.sub(r'\b(?:self|owner)\s*\.\s*(?:metrics\s*\.\s*)?','',s)
    s=re.sub(r'crate::(?:analytics|execution|orchestration|market)::', 'crate::', s)
    s=re.sub(r'\b(\w+):\s*\1\s*,',r'\1,',s)
    f=subprocess.run(['rustfmt','--edition','2021','--emit','stdout'],input='fn parity() {\n'+s+'\n}',text=True,capture_output=True)
    assert f.returncode==0,f.stderr
    s=f.stdout
    t=[m[0] for m in lex.finditer(s) if not m[0].startswith(('//','/*'))]
    return [v for i,v in enumerate(t) if not (v==',' and i+1<len(t) and t[i+1] in (')','}',']'))]
results={}
for name,old in parts.items():
    if name=='drain_gate':continue
    new=(r/base/f'orchestration/state_owner/{name}.rs').read_text()
    ts=[m for m in lex.finditer(new) if not m[0].startswith(('//','/*','"'))]
    fn=next(i for i,m in enumerate(ts) if m[0]=='fn')
    begin=next(i for i in range(fn,len(ts)) if ts[i][0]=='{');depth=1;j=begin+1
    while depth:
        if ts[j][0]=='{':depth+=1
        elif ts[j][0]=='}':depth-=1
        j+=1
    new=new[ts[begin].end():ts[j-1].start()].rstrip()
    endings={'lifecycle':'true','remote_apply':'Ok(worker_now_s)','dispatch':'Ok(())','events':'Ok(OwnerStep::Next(detect))','detect':'Ok(true)'}
    ending=endings.get(name)
    if ending:
        assert new.endswith(ending),(name,new[-100:]);new=new[:-len(ending)]
    if name=='lifecycle':new=new.replace('return false','break')
    if name=='events':
        new=new.replace('ingress.spawn(', 'spawn_ingress(')
        new=new.replace('return Ok(OwnerStep::Continue)','continue')
        new=re.sub(r'fetch,\s*&mut \|s: &mut StateCore\| \{\s*while let Some\(ev\) = self.consumer.try_pop\(\) \{\s*s.on_event\(ev\);\s*\}\s*\},', '&mut fetch, &mut drain,', new)
    if name=='detect':new=new.replace('return Ok(false)','continue')
    a=norm(old);b=norm(new);ok=a==b;results[name]={'normalized_tokens_identical':ok,'before_tokens':len(a),'after_tokens':len(b)}
    if not ok:
        diff='\n'.join(difflib.unified_diff(a,b,fromfile='before',tofile='after',n=4));(out/f'parity-{name}.diff').write_text(diff);print(name,'DIFF',diff[:4000])
    else: print(name,'IDENTICAL',len(a),'tokens')
# String literals (including messages, JSON keys and defaults) must all survive startup/owner extraction.
new='\n'.join(p.read_text() for p in (r/base/'orchestration/state_owner').glob('*.rs') if p.name not in ['accounts.rs','candidates.rs','tests.rs'])+'\n'+(r/base/'orchestration/state_owner.rs').read_text()+'\n'+(r/base/'orchestration/startup.rs').read_text()+'\n'+(r/base/'main.rs').read_text()
strings=lambda s:collections.Counter(m[0] for m in lex.finditer(s) if m[0].startswith('"') and not m[0].startswith('"crate_health"'))
a=strings(original);b=strings(new)
# Module-location attributes necessarily change. Compare application literals, not Rust file paths.
for c in [a,b]:
    for k in list(c):
        if k.endswith('.rs"'):del c[k]
results['application_literals']={'identical':a==b,'removed':dict(a-b),'added':dict(b-a)}
print('application literals',results['application_literals'])
(out/'parity.json').write_text(json.dumps(results,indent=2)+'\n')
