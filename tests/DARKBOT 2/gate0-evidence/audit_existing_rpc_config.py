"""Inspect provider/RPC settings only; never print credential values or wallet fields."""
import json,os,pathlib,re,urllib.parse,zipfile
WORK=pathlib.Path(__file__).resolve().parent.parent
REPO=pathlib.Path('/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot')
OUT=WORK/'gate0-evidence/2026-09-28-g02-archive-witness'
OUT.mkdir(exist_ok=True)
URL=re.compile(r'(?:https?|wss?)://[^\s\"\'<>]+')
PROVIDER=re.compile(r'node[_-]?real|mega[_-]?node|d[_-]?rpc|quick[_-]?node|quiknode|chainstack|p2pify',re.I)
WALLET=re.compile(r'wallet|private.?key|mnemonic|seed|passphrase|signer',re.I)
KEY=re.compile(r'^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$')
hits=[];env_settings=[];skipped=[];scanned=0
# Only provider-specific variables and explicitly named RPC endpoints are inspected.
def dotenv(path):
 global scanned
 if not path.is_file():return
 scanned+=1
 with path.open(errors='replace') as f:
  for number,line in enumerate(f,1):
   m=KEY.match(line)
   if not m:continue
   name=m[1]
   if WALLET.search(name):continue
   if not PROVIDER.search(name) and 'RPC' not in name.upper():continue
   value=m[2].strip().strip('\"\'')
   item=dict(file=str(path),line=number,setting=name,configured=bool(value))
   urls=URL.findall(value)
   # Publish hostname and non-secret structure only.
   item['endpoints']=[describe(u) for u in urls]
   item['provider_named_credential']=bool(PROVIDER.search(name) and re.search(r'key|token|auth',name,re.I) and value and not urls)
   env_settings.append(item)

def describe(url):
 try:
  p=urllib.parse.urlsplit(url.rstrip('.,);`'))
  host=p.hostname or ''
  provider=next((name for name,pattern in [('NodeReal',r'nodereal'),('dRPC',r'drpc'),('QuickNode',r'quicknode|quiknode'),('Chainstack',r'chainstack|p2pify')] if re.search(pattern,host,re.I)),None)
  public_hosts={'bsc.nodereal.io','bsc.drpc.org'}
  redacted_host=host if host in public_hosts or not provider or host.startswith('docs.') else '[redacted '+provider+' host]'
  return dict(host=redacted_host,provider=provider,path_segments=len([x for x in p.path.split('/') if x]),
              has_query=bool(p.query),has_userinfo=bool(p.username or p.password),
              authenticated_shape=bool(provider and not host.startswith('docs.') and (p.path not in ('','/') or p.query or p.username or host not in public_hosts)))
 except Exception:return {'parse_error':True}

def content(path,raw):
 global scanned
 scanned+=1
 # Recorded public RPC evidence/config sources. Suppress any wallet-named line.
 for number,line in enumerate(raw.splitlines(),1):
  if WALLET.search(line.split('=',1)[0]) or not PROVIDER.search(line):continue
  for u in URL.findall(line):
   if not PROVIDER.search(urllib.parse.urlsplit(u.rstrip('.,);`')).hostname or ''):continue
   hits.append(dict(file=str(path),line=number,endpoint=describe(u)))

for path in REPO.glob('.env*'):dotenv(path)
for path in WORK.glob('.env*'):dotenv(path)
for name in ['.zshenv','.zprofile','.zshrc','.bash_profile','.bashrc']:
 dotenv(pathlib.Path.home()/name)
for n,v in os.environ.items():
 if WALLET.search(n) or not (PROVIDER.search(n) or 'RPC' in n.upper()):continue
 env_settings.append(dict(file='process environment',setting=n,configured=bool(v),endpoints=[describe(u) for u in URL.findall(v)],provider_named_credential=bool(PROVIDER.search(n) and re.search(r'key|token|auth',n,re.I) and v)))
for root in [WORK,REPO/'config',REPO/'darkbot/config.py',REPO/'docs']:
 paths=[root] if root.is_file() else root.rglob('*')
 for path in paths:
  if not path.is_file() or any(x in path.parts for x in ['.git','node_modules','__pycache__','tests']):continue
  if path.name.startswith('.env'):
   dotenv(path)
   continue
  if path.suffix not in ['.json','.toml','.yaml','.yml','.md','.py','.txt']:continue
  if path.stat().st_size>5000000:continue
  try:content(path,path.read_text(errors='replace'))
  except OSError as e:skipped.append(dict(file=str(path),error=type(e).__name__))
# Inspect archive member names/config contents in-memory; never extract wallet/key files.
zip_configs=[]
for path in WORK.glob('*.zip'):
 try:
  with zipfile.ZipFile(path) as z:
   for item in z.infolist():
    name=pathlib.PurePosixPath(item.filename)
    if WALLET.search(item.filename) or item.file_size>5000000 or any(x in name.parts for x in ['.git','node_modules','__pycache__']):continue
    if name.name.startswith('.env'):
     zip_configs.append(dict(zip=str(path),member=item.filename,note='Env member not opened; avoid unrelated wallet values'))
    elif 'config' in name.name.lower() and name.suffix in ['.json','.toml','.yaml','.yml','.py']:
     content(str(path)+'::'+item.filename,z.read(item).decode(errors='replace'))
 except (OSError,zipfile.BadZipFile) as e:skipped.append(dict(file=str(path),error=type(e).__name__))
result=dict(scope='workspace evidence, local repo dotenv/RPC config and docs, selective shell startup RPC assignments, process provider/RPC vars, config members of local ZIPs; no shell history/keychain/wallet files',files_scanned=scanned,
            settings=env_settings,provider_url_hits=hits,zip_env_members=zip_configs,skipped=skipped,
            credential_candidates=sum(1 for x in env_settings if x.get('provider_named_credential')),
            authenticated_shape_candidates=sum(1 for x in hits if x['endpoint'].get('authenticated_shape'))+sum(1 for x in env_settings for u in x.get('endpoints',[]) if u.get('authenticated_shape')))
(OUT/'existing_rpc_config_audit.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='provider_url_hits'},indent=2))
print('Provider URL hosts only:',sorted({x['endpoint'].get('host') for x in hits if x['endpoint'].get('host')}))
