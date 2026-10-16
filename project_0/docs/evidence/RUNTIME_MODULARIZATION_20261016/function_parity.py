from pathlib import Path
import tarfile,re,subprocess,hashlib,collections,json
out=Path(__import__('sys').argv[1]) if len(__import__('sys').argv)>1 else Path(__file__).parent;r=Path('/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot');base='project_0/native/crates/runtime/src/'
lex=re.compile(r'//[^\n]*|/\*[\s\S]*?\*/|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'|[A-Za-z_]\w*|[^\s]',re.M)
def functions(s,path):
    s=re.sub(r'\b(crate|darkbot_runner)::(?:analytics|execution|orchestration|market)::',r'\1::',s).replace('darkbot_runner::','crate::')
    s=s.replace('use crate::{econ, market_observer};', 'use crate::econ; use crate::market_observer;').replace('use crate::analytics::econ;', 'use crate::econ;').replace('use crate::observer as market_observer;', 'use crate::market_observer;')
    # File includes are checked separately against their resolved unchanged bytes.
    s=re.sub(r'(include_(?:str|bytes)!\(\s*")[^"]*/([^"/]+)(")',r'\1\2\3',s)
    fmt=subprocess.run(['rustfmt','--edition','2021','--config','skip_children=true','--emit','stdout'],input=s,text=True,capture_output=True)
    assert fmt.returncode==0,(path,fmt.stderr)
    ts=[m[0] for m in lex.finditer(fmt.stdout) if not m[0].startswith(('//','/*'))]
    found=[]
    for i,t in enumerate(ts):
        if t!='fn' or i+2>=len(ts):continue
        name=ts[i+1]
        if not re.fullmatch(r'\w+',name):continue
        j=i+2
        while j<len(ts) and ts[j] not in ['{',';']:j+=1
        if j==len(ts) or ts[j]==';':continue
        depth=1;k=j+1
        while depth and k<len(ts):
            if ts[k]=='{':depth+=1
            elif ts[k]=='}':depth-=1
            k+=1
        body=ts[j:k]
        # A trailing comma is optional and can be introduced by formatting after path length changes.
        body=[v for q,v in enumerate(body) if not (v==',' and q+1<len(body) and body[q+1] in [')','}',']'])]
        h=hashlib.sha256('\n'.join(body).encode()).hexdigest();found.append((name,h,path))
    return found
old=[];new=[]
with tarfile.open(out/'source-before.tar.gz') as t:
    for m in t.getmembers():
        if m.name.startswith(base) and m.name.endswith('.rs'):old+=functions(t.extractfile(m).read().decode(),m.name)
for p in (r/base).rglob('*.rs'):new+=functions(p.read_text(),str(p.relative_to(r)))
a=collections.Counter((n,h) for n,h,p in old);b=collections.Counter((n,h) for n,h,p in new)
missing=a-b;added=b-a
report={'before_functions':len(old),'after_functions':len(new),'identical_function_bodies':sum((a&b).values()),'changed_or_extracted_before':[{'function':n,'path':p,'body_sha256':h} for n,h,p in old if (n,h) in missing], 'new_or_changed_after':[{'function':n,'path':p,'body_sha256':h} for n,h,p in new if (n,h) in added]}
(out/'function-parity.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='new_or_changed_after'},indent=2))
