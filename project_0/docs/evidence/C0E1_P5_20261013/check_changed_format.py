from pathlib import Path
import subprocess,difflib,re,sys
# Reproducible changed-region check; original baseline formatting is not certified.
base="2c62d135f8beb755a82a63a3be5daf9d5420130f"
root=Path.cwd()
paths=subprocess.check_output(['git','diff',base,'--name-only'],text=True).splitlines()
paths += ['project_0/native/crates/darkbot-runner/src/bin/p0-econ-replay.rs']
write='--write' in sys.argv
fail=[]
for name in dict.fromkeys(paths):
 if not name.startswith('project_0/') or not name.endswith('.rs'): continue
 p=Path(name); raw=p.read_text(); lines=raw.splitlines(keepends=True)
 try: old=subprocess.check_output(['git','show',base+':'+name],stderr=subprocess.DEVNULL,text=True).splitlines(keepends=True)
 except subprocess.CalledProcessError: old=[]
 changed=[(b1,b2) for op,a1,a2,b1,b2 in difflib.SequenceMatcher(None,old,lines,autojunk=False).get_opcodes() if op!='equal']
 # Format stdin: suppress recursive child modules and never modify unrelated files.
 formatted=subprocess.run(['rustfmt','--edition','2021','--config','skip_children=true','--emit','stdout'],input=raw,text=True,capture_output=True,check=True).stdout.splitlines(keepends=True)
 edits=[]
 for op,a1,a2,b1,b2 in difflib.SequenceMatcher(None,lines,formatted,autojunk=False).get_opcodes():
  if op=='equal': continue
  if any(a1<hi and a2>lo or (a1==a2 and lo<=a1<hi) for lo,hi in changed): edits.append((a1,a2,formatted[b1:b2]))
 if edits:
  fail.append(name)
  if write:
   for a,b,chunk in reversed(edits): lines[a:b]=chunk
   p.write_text(''.join(lines))
print(('formatted' if write else 'unformatted changed files'),len(fail))
for p in fail: print(p)
if fail and not write:sys.exit(1)
