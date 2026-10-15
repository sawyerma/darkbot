"""Synthetic Marginfi transactions, fake RPC and temporary V1 runs shared by unit and integration tests."""
import base64
import copy
import json
from pathlib import Path
import struct
import tempfile
import p0_paths  # noqa: F401  (puts project_0/scripts on sys.path)
import audit_market_quick as a
import measurement_contract as c

START=100* a.NS
END=200*a.NS
KEY=bytes([2])*32
TARGET=a.base58_encode(KEY)
ACTOR=a.base58_encode(bytes([3])*32)
NAMES={v:k for k,v in a.INSTRUCTION_TYPES.items()}


def payload(kind):
    disc=next(d for d,k in a.EVENT_DISCRIMINATORS.items() if k==kind)
    if kind in ('RECEIVERSHIP','FORCED_DELEVERAGE'):
        return disc+KEY+bytes([3])*32+struct.pack('<dd',105,100)+(struct.pack('<I',5000) if kind=='RECEIVERSHIP' else b'')
    header=b'\x01'+bytes([3])*32+KEY*3
    if kind=='CLASSIC':
        return disc+header+KEY*6+struct.pack('<12d',*[float(i) for i in range(12)])
    return disc+header+KEY*2+struct.pack('<3d',10,6,4)


def tx(sig='sig',slot=10,bt=150,kind='RECEIVERSHIP',version='legacy'):
    name=NAMES['RECEIVERSHIP_END' if kind=='RECEIVERSHIP' else kind]
    raw=a.hashlib.sha256(('global:'+name).encode()).digest()[:8]
    if kind=='CLASSIC':
        raw+=struct.pack('<QH',42,0)
    result = {'slot':slot,'blockTime':bt,'version':version,
        'transaction':{'signatures':[sig],'message':{'accountKeys':[a.PROGRAM,TARGET,ACTOR],
            'instructions':[{'programIdIndex':0,'data':a.base58_encode(raw),'accounts':({'CLASSIC':[0,1,1,1,2,1,0,0], 'RECEIVERSHIP':[1,0,0,2,0,0,0,0], 'FORCED_DELEVERAGE':[1,0,0,2], 'BANKRUPTCY_SETTLEMENT':[0,2,0,1]}[kind])}]}},
        'meta':{'err':None,'fee':12345,'loadedAddresses':{'writable':[],'readonly':[]},
                'innerInstructions':[], 'logMessages':[f'Program {a.PROGRAM} invoke [1]',
                    'Program data: '+base64.b64encode(payload(kind)).decode(),f'Program {a.PROGRAM} success']}}

    if kind in ('RECEIVERSHIP','FORCED_DELEVERAGE'):
        marker='start_liquidation' if kind=='RECEIVERSHIP' else 'start_deleverage'
        result['transaction']['message']['instructions'].insert(0,{'programIdIndex':0,
            'data':a.base58_encode(a.hashlib.sha256(('global:'+marker).encode()).digest()[:8]),
            'accounts':[1,0,0,2,0]})
        result['meta']['logMessages']=[f'Program {a.PROGRAM} invoke [1]',f'Program {a.PROGRAM} success']+result['meta']['logMessages']
    return result


def row(sig,slot,bt,err=None):
    return {'signature':sig,'slot':slot,'blockTime':bt,'err':err}


class FakeRPC:
    def __init__(self,pages,txs):
        self.pages,self.txs=pages,txs
        self.calls,self.bytes_received,self.bytes_sent=0,0,0
        self.methods=a.collections.Counter()
        self.requests=[]
    def call(self,method,params):
        self.calls+=1;self.methods[method]+=1;self.requests.append((method,params))
        if method=='getSignaturesForAddress':
            result=self.pages[params[1].get('before')]
        elif method=='getTransaction':
            result=self.txs.get(params[0])
        else:
            raise AssertionError('full blocks forbidden')
        if isinstance(result,Exception):
            raise result
        return copy.deepcopy(result)



class TempRunBase:
    """Synthetic V1 run in a temporary directory; mixed into TestCase classes."""
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory()
        self.run=Path(self.tmp.name)/'run'
        (self.run/'market').mkdir(parents=True)
        (self.run/'market/summary.json').write_text(json.dumps({
            'observation_started_at':a.iso_ns(START),'observation_ended_at':a.iso_ns(END),
            'configured_duration_seconds':100,'full_slot_reconciliation':{'market_slot_end':{'slot':11}}}))
        self.write('market/transactions.jsonl',[])
        self.write('market/coverage.jsonl',[])
    def tearDown(self):
        self.tmp.cleanup()
    def write(self,name,rows):
        (self.run/name).write_text(''.join(json.dumps(r)+'\n' for r in rows))
    def execute(self,rpc,pages=50):
        audit=a.Audit(self.run,rpc,pages)
        try:return audit.execute()
        finally:audit.close()
    def normal_rpc(self):
        return FakeRPC({None:[row('head',30,220),row('one',20,150)],'one':[row('old',10,90)]}, {'one':tx('one',20,150)})


def setup_events(self):
    live=[];historical=[]
    for i,kind in enumerate(c.KINDS):
        tx=tx('tx'+str(i),10+i,120+i,kind)
        live.append({'source':'transactionSubscribe','signature':'tx'+str(i),'slot':10+i,'t_rx_ns':i+1,'observed_at_utc':a.iso_ns((120+i)*a.NS),'raw':tx})
        d=a.detect(tx,10+i);d['block_time']=120+i;d['membership']='IN_WINDOW'
        historical.append({'historical':d,'darkbot':{}})
    for i,name in enumerate(c.RELATED):
        tx=tx('related'+str(i),30+i,140+i,'CLASSIC')
        tx['transaction']['message']['instructions'][0]['data']=a.base58_encode(a.hashlib.sha256(('global:'+name).encode()).digest()[:8])
        tx['meta']['logMessages']=[f'Program {a.PROGRAM} invoke [1]',f'Program {a.PROGRAM} success']
        live.append({'source':'transactionSubscribe','signature':'related'+str(i),'slot':30+i,'raw':tx})
        d=a.detect(tx,30+i);d['block_time']=140+i
        self.assertEqual(d['events'],[])
        self.assertEqual(d['instruction_evidence'][0]['related_type'],name)
        historical.append({'historical':d,'darkbot':{}})
    self.write('market/transactions.jsonl',live)
    self.write('opportunity_economics.jsonl',[{'target':TARGET,'market':None,'our':{'account':TARGET,'economics_known':True,'net_profit':9999,'signature':'shadow-sig','timing':{'t_rx_ns':7}}}])
    evidence=c.finalize_capture(self.run)
    spec=c.specification(self.run,c.VerificationConfig(mode='index'),'offline-fixture');sid=spec['verification_spec_id']
    out=self.run/'verification'/sid;out.mkdir(parents=True);c.immutable(out/'spec.json',spec)
    self.write(str((out/'comparison.jsonl').relative_to(self.run)),historical)
    payload={'capture_run_id':evidence['capture_run_id'],'verification_spec_id':sid,'capture_evidence_digest':evidence['evidence_digest'],'verification_attempt_id':'test',
        'mode':'index','summary':{'status':'INDEX_VERIFIED'},'artifacts':{'comparison':{'path':str((out/'comparison.jsonl').relative_to(self.run)),'sha256':c.file_hash(out/'comparison.jsonl')}},'finished_unix_ns':1}
    result={**payload,'verification_result_id':c.digest(payload)}
    c.immutable(out/'results'/(result['verification_result_id']+'.json'),result)
    return result
