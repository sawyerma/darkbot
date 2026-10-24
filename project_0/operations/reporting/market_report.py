"""Unfiltered, reproducible evidence normalization; queries only affect renderers.

No RPC or pricing engine. Chain and hypothetical records remain separate. Stored
production economics is imported with provenance, never recomputed from estimates.
"""
from dataclasses import asdict
from pathlib import Path
import collections
import csv
import fcntl
import io
import json
import math
import os
import sqlite3
import sys
import subprocess
import uuid
if __package__ in (None,''):sys.path.insert(0,str(Path(__file__).resolve().parents[2]))  # run as a file: make `operations` importable
from operations.verification import audit_market_quick as a
from operations.cli import measurement_contract as c
from operations.capture import run_store

VERSION='C0-E1/NormalizedReport/2'  # record, snapshot and renderer format
DATASET_VERSION='C0-E1/NormalizedReport/3'  # dataset identity, bound to the run layout contract
PRIORITY={'strict':3,'index':2,'live':1,'shadow':0,'unverified':-1}


def values(path):
    if path.exists():
        for row,_ in a.rows(path):yield row


def selected_result(run,rid=None):
    candidates=[]
    for p in (run/'verification').glob('*/results/*.json'):
        if rid and p.stem!=rid:continue
        v=json.loads(p.read_text());validate_result(run,v)
        candidates.append(v)
    if rid and len(candidates)!=1:raise c.ContractError('RESULT_NOT_FOUND_OR_AMBIGUOUS')
    return max(candidates,key=lambda v:v['finished_unix_ns']) if candidates else None


def strict_directory(run,checkpoint,batch):
    directory=(checkpoint.parent/batch['directory']).resolve()
    if not directory.is_relative_to(run.resolve()):raise c.ContractError('STRICT_BATCH_PATH_INVALID')
    for name,expected in batch['files'].items():
        path=(directory/name).resolve()
        if not path.is_relative_to(directory):raise c.ContractError('STRICT_BATCH_PATH_INVALID')
        if a.base58_encode(bytes.fromhex(c.file_hash(path)))!=expected:raise c.ContractError('STRICT_BATCH_EVIDENCE_CHANGED')
    return directory


def validate_result(run,result):
    sid=result.get('verification_spec_id','')
    if len(sid)!=64 or any(x not in '0123456789abcdef' for x in sid):raise c.ContractError('RESULT_SPEC_BINDING_INVALID')
    specpath=run/'verification'/sid/'spec.json'
    if not specpath.is_file():raise c.ContractError('RESULT_SPEC_BINDING_MISSING')
    spec=json.loads(specpath.read_text())
    if c.digest({k:v for k,v in spec.items() if k!='verification_spec_id'})!=sid:raise c.ContractError('RESULT_SPEC_DIGEST_MISMATCH')
    capture=c.finalize_capture(run)
    if (spec.get('capture_run_id')!=capture['capture_run_id'] or result.get('capture_run_id')!=capture['capture_run_id']
        or result.get('capture_evidence_digest')!=capture['evidence_digest']
        or spec.get('mode')!=result.get('mode') or spec.get('window')!=a.read_window(run)):
        raise c.ContractError('RESULT_SPEC_CAPTURE_MISMATCH')
    payload={k:x for k,x in result.items() if k!='verification_result_id'}
    if c.digest(payload)!=result.get('verification_result_id'):raise c.ContractError('RESULT_DIGEST_MISMATCH')
    for item in result['artifacts'].values():
        artifact=(run/item['path']).resolve()
        if not artifact.is_relative_to(run.resolve()) or c.file_hash(artifact)!=item['sha256']:raise c.ContractError('RESULT_EVIDENCE_CHANGED')
        if result['mode']=='strict':
            batch=json.loads(artifact.read_text())
            if batch.get('spec')!=sid or batch.get('proof',{}).get('complete') is not True:raise c.ContractError('STRICT_BATCH_PROOF_UNBOUND')
            if not {'transactions.jsonl','liquidations.jsonl','coverage.jsonl'}<=set(batch.get('files',{})):raise c.ContractError('STRICT_BATCH_PROOF_MISSING')
            strict_directory(run,artifact,batch)


def finite(value):
    return value if isinstance(value,(int,float)) and not isinstance(value,bool) and math.isfinite(value) else None


def deltas(pre,post):
    if not isinstance(pre,list) or not isinstance(post,list) or len(pre)!=len(post):return None
    if any(type(n) is not int for n in pre+post):return None
    return [str(y-x) for x,y in zip(pre,post)]


def token_deltas(balances):
    pre,post=balances.get('pre'),balances.get('post')
    if not isinstance(pre,list) or not isinstance(post,list):return None
    def keyed(rows):
        out={}
        for r in rows:
            key=(r['accountIndex'],r['mint'],r.get('owner'))
            if key in out:raise ValueError('DUPLICATE_TOKEN_BALANCE')
            out[key]=(int(r['uiTokenAmount']['amount']),r['uiTokenAmount']['decimals'])
        return out
    try:
        before,after=keyed(pre),keyed(post);result=[]
        for key in sorted(set(before)|set(after),key=str):
            # Missing side is not proof of a zero balance / account creation.
            x,y=before.get(key),after.get(key)
            result.append({'account_index':key[0],'mint':key[1],'owner':key[2],
                'delta_native_exact':str(y[0]-x[0]) if x and y and x[1]==y[1] else None,
                'decimals':(y or x)[1],'reason':None if x and y else 'BALANCE_SIDE_MISSING'})
        return result
    except (KeyError,ValueError,TypeError):return None


def priority_fee(tx):
    return a.requested_priority_fee(tx)


class Dataset:
    def __init__(self,directory):
        self.directory=directory
        self.db=sqlite3.connect(directory/'normalized.sqlite3')
        self.db.executescript('''PRAGMA cache_size=-8192; PRAGMA temp_store=FILE;
            CREATE TABLE IF NOT EXISTS evidence(entity TEXT,source TEXT,digest TEXT,data TEXT,PRIMARY KEY(entity,source,digest));
            CREATE TABLE IF NOT EXISTS shadow(entity TEXT PRIMARY KEY,data TEXT);
            CREATE TABLE IF NOT EXISTS receipts(sig TEXT,slot INTEGER,ns INTEGER,utc TEXT,PRIMARY KEY(sig,slot));''')
    def put(self,record,source):
        record['source']=source
        key=c.digest([record.get('signature'),record.get('slot'),record['record_kind'],
            record.get('instruction_position') if record['record_kind']!='LIQUIDATION' else None,record.get('event_index'),record.get('liquidation_type'),record.get('related_type')])
        record['entity_id']=key
        self.db.execute('INSERT OR IGNORE INTO evidence VALUES(?,?,?,?)',(key,source,c.digest(record),json.dumps(record,allow_nan=False)))
    def transaction(self,decoded,source,raw=None,metadata=None,proof_binding=None):
        metadata=metadata or {};sig,slot=decoded.get('signature'),decoded.get('slot')
        if not isinstance(sig,str) or type(slot) is not int:raise c.ContractError('NORMALIZED_TX_IDENTITY_UNKNOWN')
        success=decoded.get('success');classification=decoded.get('classification','UNKNOWN')
        qualified=success is True and classification=='CLASSIFIED' and source in ('index','strict') and proof_binding is not None
        base={'schema':VERSION,'signature':sig,'slot':slot,'transaction_index':(raw or {}).get('transactionIndex'),
            'block_time':decoded.get('block_time',(raw or {}).get('blockTime')),
            'block_utc':None,'live_t_rx_ns':metadata.get('t_rx_ns'),'live_received_utc':metadata.get('observed_at_utc'),
            'capture_source':metadata.get('source'),'verification_binding':proof_binding,'outcome':'SUCCESS' if success is True else 'FAILED' if success is False else 'UNKNOWN',
            'classification':classification,'boundary_second':decoded.get('boundary_second',False),'protocol_pin':c.PROTOCOL,'decoder_version':a.SCHEMA,
            'instruction_position':None,'event_index':None,'liquidation_type':None,'related_type':None,
            'markers':[],'pairing':decoded.get('liquidation_pairs',[]),'target':None,'actor':None,'winner':None,
            'asset_mint':None,'liability_mint':None,'native_amounts':None,'transaction_fee_lamports':decoded.get('fee_lamports'),
            'priority_fee_lamports':priority_fee(raw) if raw else decoded.get('priority_fee_lamports'),'jito_tip_lamports':None,'incremental_rent_lamports':None,
            'market_gross_edge_usd':None,'executable_shadow_net_edge_usd':None,'realized_pnl_usd':None,
            'required_capital_usd':None,'known_costs':{},'unknown_costs':['JITO_TIP','RENT','SWAP_ROUTE_COST','REALIZED_PNL'],
            'lamport_deltas':deltas(**decoded.get('lamport_balances',{'pre':None,'post':None})),
            'token_deltas':token_deltas(decoded.get('token_balances',{})),
            'missing_reasons':list(decoded.get('unknown_reasons',[])),'gap_correlation':metadata.get('live_coverage','UNKNOWN'),
            'chain_liquidation_confirmed':False,'shadow':None,'competition':None,
            'verification_scope':'TRANSACTION_EVIDENCE; coverage is separate','fee_scope':'TRANSACTION_NOT_PER_EVENT'}
        if type(base['block_time']) is int:base['block_utc']=a.iso_ns(base['block_time']*a.NS)
        if base['transaction_fee_lamports'] is not None:base['known_costs']['meta_fee_lamports']=base['transaction_fee_lamports']
        else:base['unknown_costs'].append('TRANSACTION_FEE')
        if base['priority_fee_lamports'] is not None:base['known_costs']['priority_fee_lamports']=base['priority_fee_lamports']
        else:base['unknown_costs'].append('PRIORITY_FEE')
        if source=='live':
            self.db.execute('INSERT OR IGNORE INTO receipts VALUES(?,?,?,?)',(sig,slot,base['live_t_rx_ns'],base['live_received_utc']))
        self.put({**base,'record_kind':'TRANSACTION'},source)
        for ix in decoded.get('instruction_evidence',[]):
            if not ix.get('marker') and not ix.get('related_type'):continue
            record={**base,'record_kind':'RELATED_NON_LIQUIDATION' if ix.get('related_type') else 'INSTRUCTION_MARKER',
                'instruction_position':ix['position'],'markers':[ix['marker']] if ix.get('marker') else [],
                'related_type':ix.get('related_type'),'target':ix.get('target'),'actor':ix.get('actor'),
                'instruction_accounts':ix.get('accounts'),'affected_bank':ix.get('affected_bank'),'fee_recipient':ix.get('fee_recipient')}
            if ix.get('marker'):
                record['liquidation_type']={'RECEIVERSHIP_START':'RECEIVERSHIP','RECEIVERSHIP_END':'RECEIVERSHIP',
                    'FORCED_DELEVERAGE_START':'FORCED_DELEVERAGE'}.get(ix['kind'],ix['kind'])
            self.put(record,source)
        for ev in decoded.get('events',[]):
            kind=ev.get('type',ev.get('liquidation_type'))
            if kind not in c.KINDS:continue
            marker=[ix['marker'] for ix in decoded.get('instruction_evidence',[]) if ix.get('marker') and ix.get('position')==ev.get('instruction_position')]
            record={**base,'record_kind':'LIQUIDATION','instruction_position':ev.get('instruction_position'),
                'event_index':ev.get('event_index'),'liquidation_type':kind,'markers':marker,'target':ev.get('target'),
                'actor':ev.get('actor',ev.get('liquidator',ev.get('receiver'))),'asset_mint':ev.get('asset_mint'),
                'liability_mint':ev.get('liability_mint'),'native_amounts':ev.get('native_evidence'),
                'market_gross_edge_usd':finite(ev.get('gross_edge_usd')),'chain_liquidation_confirmed':qualified}
            if record['market_gross_edge_usd'] is None:
                seized=finite(ev.get('seized_value_usd'));repaid=finite(ev.get('repaid_value_usd'))
                if seized is not None and repaid is not None:record['market_gross_edge_usd']=seized-repaid
            record['permissionless_profit_opportunity']=kind in ('CLASSIC','RECEIVERSHIP')
            if qualified and record['permissionless_profit_opportunity']:record['winner']=record['actor']
            if not qualified:record['missing_reasons']=record['missing_reasons']+['CHAIN_EXECUTION_UNQUALIFIED']
            if ev.get('winner_net_edge_complete') is True and qualified:
                record['realized_pnl_usd']=finite(ev.get('winner_net_edge_usd'))
            self.put(record,source)
    def close(self):self.db.close()


def raw_tx(row):
    raw=row.get('raw',{})
    if isinstance(raw,dict) and isinstance(raw.get('transaction'),dict):return raw
    # transactionSubscribe has an additional value/transaction envelope.
    val=raw.get('value',raw) if isinstance(raw,dict) else {}
    inner=val.get('transaction') if isinstance(val,dict) else None
    if isinstance(inner,dict) and isinstance(inner.get('transaction'),dict):return {**inner,'slot':row.get('slot'),'version':val.get('version',inner.get('version','legacy'))}
    return None


def load_live(ds,run):
    market=run_store.market_dir(run,c.layout(run))
    for row in values(market/'transactions.jsonl'):
        source=row.get('source','')
        normalized_source='live' if source in ('logsSubscribe','transactionSubscribe') else 'unverified'
        raw=raw_tx(row)
        if raw:
            decoded=a.detect(raw,row['slot'])
        else:
            logs=((row.get('raw') or {}).get('value') or {}).get('logs',[])
            events,errors=a.decode_logs(logs)
            if row.get('success') is not True:events=[]
            decoded={'signature':row.get('signature'),'slot':row.get('slot'),'success':row.get('success'),
                'classification':'UNKNOWN','events':events,'unknown_reasons':errors+['LIVE_INSTRUCTION_VIEW_UNAVAILABLE']}
        ds.transaction(decoded,normalized_source,raw,row)


def load_result(ds,run,result):
    if not result:return
    validate_result(run,result)
    proof={k:result[k] for k in ('capture_run_id','capture_evidence_digest','verification_spec_id','verification_result_id')}
    if result['mode']=='index':
        for item in result['artifacts'].values():
            if not item['path'].endswith('/comparison.jsonl'):continue
            for row in values(run/item['path']):
                ds.transaction(row['historical'],'index',metadata=row.get('darkbot'),proof_binding=proof)
                for ev in row.get('darkbot',{}).get('events',[]):
                    if ev.get('shadow_status')=='SHADOW_MATCH':load_shadow(ds,ev['shadow_evidence'])
    elif result['mode']=='strict':
        seen=set()
        for item in result['artifacts'].values():
            checkpoint=run/item['path'];batch=json.loads(checkpoint.read_text())
            directory=checkpoint.parent/batch['directory']
            # Each saved batch was pinned by the final result. Its files must still match
            # the Rust SHA-256/base58 digest before any canonical evidence is consumed.
            for name,expected in batch['files'].items():
                actual=a.base58_encode(bytes.fromhex(c.file_hash(directory/name)))
                if actual!=expected:raise c.ContractError('STRICT_BATCH_EVIDENCE_CHANGED')
            for row in values(directory/'transactions.jsonl'):
                key=(row['signature'],row['slot'])
                if key in seen:continue
                seen.add(key)
                raw=row['raw'];decoded=a.detect(raw,row['slot']);decoded['block_time']=row.get('block_time')
                ds.transaction(decoded,'strict',raw,proof_binding=proof)


def load_shadow(ds,row,replace_existing=False):
    wrapper=row.get('join',row);market=wrapper.get('market');our=wrapper.get('our')
    if not our and not market:return
    if market:
        # Exact event key. A target/slot alone is not an executable plan match.
        key=c.digest([market.get('signature'),market.get('slot'),market.get('event_index'),market.get('liquidation_type',market.get('type')),market.get('target')])
        if wrapper.get('identity_status') not in ('TYPE_TARGET_BANK_PAIR_CANONICAL_EVENT','EXACT_PLAN_MATCH','MATCH'):
            row={**row,'executable_identity_unproven':True}
    else:key='shadow-'+c.digest(row)
    ds.db.execute(('INSERT OR REPLACE' if replace_existing else 'INSERT OR IGNORE')+' INTO shadow VALUES(?,?)',(key,json.dumps(row,allow_nan=False)))


def best_record(rows):
    rows.sort(key=lambda r:PRIORITY[r['source']],reverse=True)
    factual=[r for r in rows if r['source']!='unverified']
    fallback=factual or rows
    record=dict(rows[0]);record['provenance']=[{'source':r['source'],'digest':c.digest(r)} for r in rows]
    # Fields from lower-ranked ACTUAL evidence may fill missing facts, but not
    # UNKNOWN outcome/qualification from a stronger source. Shadow is separate.
    for key in ('asset_mint','liability_mint','native_amounts','transaction_fee_lamports','priority_fee_lamports',
                'jito_tip_lamports','incremental_rent_lamports','lamport_deltas','token_deltas','market_gross_edge_usd'):
        if record.get(key) is None:
            record[key]=next((r[key] for r in fallback if r.get(key) is not None),None)
    record['field_provenance']={key:next((r['source'] for r in fallback if r.get(key)==value and value is not None),None)
                                for key,value in record.items() if key not in ('provenance','shadow')}
    positions={tuple(r['instruction_position']) for r in fallback if r.get('instruction_position') is not None}
    if len(positions)>1:
        record['classification']='UNKNOWN';record['chain_liquidation_confirmed']=False;record['winner']=None
        record['missing_reasons']=record.get('missing_reasons',[])+['CONFLICTING_INSTRUCTION_POSITIONS']
    return record


def normalize(run,result=None):
    run=Path(run).resolve()
    reports=run/'reports';reports.mkdir(exist_ok=True)
    with (reports/'normalizer.lock').open('a') as lock:
        try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:raise c.ContractError('REPORT_ALREADY_RUNNING') from None
        return normalize_locked(run,result)


def normalize_locked(run,result=None):
    evidence=c.finalize_capture(run);c.check_capture(run,evidence,hashes=True)
    if result and (result.get('capture_run_id')!=evidence['capture_run_id'] or result.get('capture_evidence_digest',evidence['evidence_digest'])!=evidence['evidence_digest']):
        raise c.ContractError('RESULT_CAPTURE_MISMATCH')
    if result:validate_result(run,result)
    layout=c.layout(run)
    identity={'version':DATASET_VERSION,'layout':run_store.LAYOUT_TAGS[layout],'layout_sha256':c.file_hash(Path(run_store.__file__)),
        'normalizer_sha256':c.file_hash(Path(__file__)),
        'capture_run_id':evidence['capture_run_id'],'capture_digest':evidence['evidence_digest'],
        'decoder_sha256':c.file_hash(Path(a.__file__)),'contract_sha256':c.file_hash(Path(c.__file__)),
        'verification_result_id':result['verification_result_id'] if result else None}
    key=c.digest(identity);out=run/'reports'/('dataset_'+key)
    final=out/'dataset.json'
    if final.exists():
        manifest=json.loads(final.read_text())
        if c.file_hash(out/'all_records.jsonl')!=manifest['records_sha256']:raise c.ContractError('NORMALIZED_DATASET_CHANGED')
        if any(manifest.get(k)!=v for k,v in identity.items()):raise c.ContractError('NORMALIZED_IDENTITY_CHANGED')
        for name,sha in manifest['snapshot_sha256'].items():
            if c.file_hash(out/name)!=sha:raise c.ContractError('SNAPSHOT_EVIDENCE_CHANGED')
        return out,manifest
    # A partial build is preserved; a fresh attempt never overwrites it.
    out.mkdir(parents=True,exist_ok=True);work=out/('attempt_'+uuid.uuid4().hex);work.mkdir()
    ds=Dataset(work);coverage=None
    clock_manifest=run_store.artifact(run,'manifest',layout)
    clock_origin=json.loads(clock_manifest.read_text()).get('clock_origin_unix_ns') if clock_manifest.exists() else None
    try:
        coverage=a.Audit(run,None,output=work/'coverage_index')
        coverage.import_bot()
        load_live(ds,run);load_result(ds,run,result)
        for name in ('canonical_market_shadow','opportunity_economics'):
            for row in values(run_store.artifact(run,name,layout)):load_shadow(ds,row)
        replay=production_replay(run,result,work,ds,clock_origin)
        ds.db.commit();count=0
        with (work/'all_records.jsonl').open('x') as f:
            cursor=ds.db.execute('SELECT entity,data FROM evidence ORDER BY entity,source,digest')
            for entity,group in __import__('itertools').groupby(cursor,key=lambda r:r[0]):
                sources=[json.loads(r[1]) for r in group];record=best_record(sources)
                receipt=ds.db.execute('SELECT ns,utc FROM receipts WHERE sig=? AND slot=?',(record['signature'],record['slot'])).fetchone()
                if receipt:
                    record['live_t_rx_ns'],record['live_received_utc']=receipt
                    record['field_provenance']['live_t_rx_ns']='live'
                    record['field_provenance']['live_received_utc']='live'
                boundary=record.get('block_time') in (coverage.window['start_ns']//a.NS,coverage.window['end_ns']//a.NS)
                record['gap_correlation']=coverage.coverage_at(record.get('block_time'),boundary or any(r.get('boundary_second',False) for r in sources))
                shadowkey=c.digest([record['signature'],record['slot'],record['event_index'],record['liquidation_type'],record['target']])
                shadow=ds.db.execute('SELECT data FROM shadow WHERE entity=?',(shadowkey,)).fetchone()
                if shadow:
                    sh=json.loads(shadow[0]);record['shadow']=sh
                    wrapper=sh.get('join',sh);our=wrapper.get('our') or {}
                    if (not sh.get('executable_identity_unproven') and our.get('economics_known') is True
                        and our.get('simulation_result')=='PASS' and our.get('decision') in ('ShadowReady','ShadowTechnicalReady')
                        and (our.get('timing') or {}).get('t_sign_ns') is not None
                        and (our.get('timing') or {}).get('t_handoff_ns') is None):
                        costs=our.get('selected_costs') or {};n=costs.get('net_profit_usd_micro')
                        if type(n) is int:record['executable_shadow_net_edge_usd']=n/1e6
                    capital=(our.get('selected_costs') or {}).get('required_working_capital_usd_micro')
                    record['required_capital_usd']=capital/1e6 if type(capital) is int and not sh.get('executable_identity_unproven') else None
                    record['required_capital_scope']='WORKING_CAPITAL_ONLY; fee/reserve budgets use existing Rust fundable_by_budget'
                    record['fundable_by_budget']=sh.get('fundable_by_budget',sh.get('capital_bucket_membership_usd'))
                    record['competition']=sh.get('race') or {'race_class':sh.get('race_class'),'local_margin_ns':sh.get('local_observed_race_margin_ns')}
                record['all_source_records']=sources
                record['live_coverage_status']=evidence['live_coverage_status']
                for field in ('asset_mint','liability_mint','realized_pnl_usd','required_capital_usd'):
                    if record.get(field) is None:record['missing_reasons'].append('UNKNOWN_'+field.upper())
                f.write(json.dumps(record,sort_keys=True,allow_nan=False)+'\n');count+=1
            for shadowkey,data in ds.db.execute("SELECT entity,data FROM shadow WHERE entity LIKE 'shadow-%' ORDER BY entity"):
                sh=json.loads(data);our=sh.get('our',{})
                record={'schema':VERSION,'entity_id':shadowkey,'record_kind':'SHADOW_ONLY','classification':'HYPOTHETICAL',
                    'source':'shadow','provenance':[{'source':'shadow','digest':c.digest(sh)}],
                    'signature':our.get('signature'),'slot':our.get('sign_slot',our.get('rx_slot')),
                    'live_t_rx_ns':(our.get('timing') or {}).get('t_rx_ns'),'shadow':sh,'target':sh.get('target',our.get('account')),
                    'live_received_utc':a.iso_ns(clock_origin+our['timing']['t_rx_ns']) if type(clock_origin) is int and type((our.get('timing') or {}).get('t_rx_ns')) is int else None,
                    'chain_liquidation_confirmed':False,'winner':None,'realized_pnl_usd':None,
                    'live_coverage_status':evidence['live_coverage_status'],'missing_reasons':['NO_CHAIN_ENTITY_PROOF']}
                f.write(json.dumps(record,sort_keys=True,allow_nan=False)+'\n');count+=1
            f.flush();os.fsync(f.fileno())
        c.check_capture(run,evidence)
        try:os.link(work/'all_records.jsonl',out/'all_records.jsonl')
        except FileExistsError:
            if c.file_hash(work/'all_records.jsonl')!=c.file_hash(out/'all_records.jsonl'):raise c.ContractError('PARTIAL_DATASET_CONFLICT')
        manifest={**identity,'record_count':count,'records_sha256':c.file_hash(out/'all_records.jsonl'),
            'historical_status':'VERIFIED' if result and result['summary']['status'] in ('INDEX_VERIFIED','CANONICAL_VERIFIED','CROSSCHECK_VERIFIED') else 'HISTORICAL_INCOMPLETE',
            'live_coverage_status':evidence['live_coverage_status'],'verification_status':result['summary']['status'] if result else 'NOT_RUN',
            'report_status':'FINAL_VERIFICATION_RESULT' if result and result['summary']['status'] in ('INDEX_VERIFIED','CANONICAL_VERIFIED','CROSSCHECK_VERIFIED') else 'PROVISIONAL/INCOMPLETE',
            'normalization_unknown_transactions':ds.db.execute("SELECT count(DISTINCT entity) FROM evidence WHERE source IN ('index','strict') AND json_extract(data,'$.record_kind')='TRANSACTION' AND json_extract(data,'$.classification')='UNKNOWN'").fetchone()[0],
            'c0_e1_master_pass':False,'window':a.read_window(run),'economics_replay':replay}
        write_snapshots(run,out,manifest,work/'economics')
        manifest['snapshot_sha256']={name:c.file_hash(out/name) for name in [*(f'hour_{i:02}.json' for i in range(1,5)),'economics_final.json']}
        c.immutable(final,manifest);return out,manifest
    finally:
        if coverage:coverage.close()
        ds.close()


def measurement_plan(run,window):
    if c.layout(run)=='v2':
        # Layout V2 has one plan authority: the pre-capture manifest. No file or binary fallback.
        plan=json.loads(run_store.artifact(run,'manifest','v2').read_text()).get('plan')
        if not isinstance(plan,dict):raise c.ContractError('MEASUREMENT_PLAN_MISSING')
        return plan
    for name in ('measurement_manifest.json','measurement_plan.json'):
        p=run/name
        if p.exists():
            v=json.loads(p.read_text());return v.get('plan',v)
    # A historical run without its own plan record (JFI-03 isolated offline path): the Rust plan
    # authority prints the plan as text (`--historical-plan`; never JSON, never a runtime input).
    # No second quarter formula here.
    binary=run_store.PROJECT/'native/target/release/p0-measurement-plan'
    if not binary.exists():raise c.ContractError('MEASUREMENT_PLAN_BINARY_MISSING')
    duration=(window['end_ns']-window['start_ns'])//a.NS
    env={k:v for k,v in os.environ.items() if not k.startswith('P0_')}
    env['P0_MEASUREMENT_DURATION']=str(duration)+'s'
    p=subprocess.run([str(binary),'--historical-plan'],env=env,capture_output=True,text=True,timeout=10)
    if p.returncode:raise c.ContractError('LEGACY_PLAN_INVALID')
    return plan_text(p.stdout)


def plan_text(text):
    """The Rust plan authority's text form (`key=value` per field in declaration order, nested fields
    dotted, lists in brackets) as the plan; the structure comes from Rust, not from a Python schema."""
    plan={}
    for line in text.splitlines():
        key,value=line.split('=',1)
        *parents,leaf=key.split('.');node=plan
        for k in parents:node=node.setdefault(k,{})
        node[leaf]=[int(x) for x in value[1:-1].split(',')] if value.startswith('[') else int(value)
    return plan


def production_replay(run,result,work,ds,clock_origin):
    """JFI-01/03: the economics replay is native only (`p0ctl backtest` over typed .p0j evidence);
    a historical run has none, and no JSON/JSONL handoff to Rust is built."""
    return {'status':'NOT_RUN','reason':'NATIVE_ONLY: p0ctl backtest reads typed .p0j evidence; historical runs have none'}


def write_snapshots(run,out,manifest,economics):
    window=manifest['window'];plan=measurement_plan(run,window)
    offsets=plan['snapshot_offsets_ns']
    if len(offsets)!=4 or offsets[-1]!=window['end_ns']-window['start_ns']:raise c.ContractError('PLAN_SNAPSHOT_MISMATCH')
    counts=[collections.Counter() for _ in offsets];unknown_time=0
    for row in values(out/'all_records.jsonl'):
        stamp=row.get('block_utc') or row.get('live_received_utc')
        if not stamp:unknown_time+=1;continue
        delta=a.utc_ns(stamp)-window['start_ns']
        bucket=next((i for i,end in enumerate(offsets) if 0<=delta<end),None)
        if bucket is None:continue
        counts[bucket][row['record_kind']]+=1
        if row['record_kind'] in ('LIQUIDATION','SHADOW_ONLY') and row.get('executable_shadow_net_edge_usd') is None:counts[bucket]['UNKNOWN_EXECUTABLE_NET_EDGE']+=1
        if row.get('chain_liquidation_confirmed'):counts[bucket]['CONFIRMED_EVENT_LOWER_BOUND']+=1
    cumulative=collections.Counter()
    for i,(offset,bucket) in enumerate(zip(offsets,counts),1):
        cumulative.update(bucket)
        econ=economics/f'hour_{i:02}.json'
        previous_path=run_store.artifact(run,f'hour_{i:02}')
        previous=json.loads(previous_path.read_text()) if previous_path.exists() else {}
        source_window=previous.get('window',{})
        aligned=(source_window.get('through_seconds')==offset/a.NS and source_window.get('from_seconds')==(offsets[i-2] if i>1 else 0)/a.NS)
        stored_shadow={key:previous[key] for key in ('shadow','capital','competition','economics','race_timing','bottlenecks','unknowns','non_claims') if key in previous} if aligned else None
        value={'schema':VERSION,'capture_run_id':manifest['capture_run_id'],'verification_result_id':manifest['verification_result_id'],
            'dataset_sha256':manifest['records_sha256'],'start_ns':window['start_ns'],
            'delta_start_ns':window['start_ns']+(offsets[i-2] if i>1 else 0),'end_ns':window['start_ns']+offset,
            'start_utc':window['start_utc'],'end_utc':a.iso_ns(window['start_ns']+offset),'plan':plan,
            'record_counts_delta':dict(bucket),'record_counts_cumulative':dict(cumulative),'records_time_unknown':unknown_time,
            'market_total_liquidations':cumulative.get('CONFIRMED_EVENT_LOWER_BOUND',0) if manifest['verification_status']=='CANONICAL_VERIFIED' and manifest['normalization_unknown_transactions']==0 else None,
            'status':manifest['report_status'],'historical_status':manifest['historical_status'],'live_coverage_status':manifest['live_coverage_status'],
            'economics':json.loads(econ.read_text()) if econ.exists() else {'status':'UNKNOWN','reason':'NO_CANONICAL_ECONOMICS_REPLAY'},
            'stored_capture_economics':{'status':'SHADOW_HYPOTHETICAL' if stored_shadow else 'UNKNOWN','data':stored_shadow,
                 'source_sha256':c.file_hash(previous_path) if previous_path.exists() else None,'window_aligned':aligned},
            'master_pass':False,'live_enabled':False}
        c.immutable(out/f'hour_{i:02}.json',value)
        if i==4:c.immutable(out/'economics_final.json',value)


def visible(row,q):
    if q.provenance=='shadow' and row.get('shadow') and row['record_kind']!='SHADOW_ONLY':
        row={'entity_id':'shadow-view-'+row['entity_id'],'related_chain_entity_id':row['entity_id'],
             'record_kind':'SHADOW_ONLY','source':'shadow','classification':'HYPOTHETICAL',
             'shadow':row['shadow'],'target':row.get('target'),'live_received_utc':row.get('live_received_utc'),
             'chain_liquidation_confirmed':False,'winner':None,'realized_pnl_usd':None}
    if q.provenance not in ('best','all'):
        candidates=[r for r in row.get('all_source_records',[row]) if r['source']==q.provenance]
        if not candidates:return None
        selected=best_record(candidates)
        for key in ('live_t_rx_ns','live_received_utc','shadow','competition','live_coverage_status'):
            if key in row:selected[key]=row[key]
        row=selected
    row=dict(row);kind=row['record_kind']
    if kind=='RELATED_NON_LIQUIDATION':
        if not q.related_events or row.get('related_type') not in q.related_types:return None
    elif kind in ('LIQUIDATION','INSTRUCTION_MARKER'):
        if row.get('liquidation_type') not in q.types:return None
        if kind=='INSTRUCTION_MARKER' and not set(row.get('markers',[]))&set(q.markers):return None
    elif kind=='SHADOW_ONLY' and q.provenance not in ('all','shadow'):return None
    # Hiding a marker label never changes event classification or removes an
    # otherwise selected completion. The standalone marker record can be hidden.
    row['markers']=[m for m in row.get('markers',[]) if m in q.markers]
    at=row.get('block_utc') or row.get('live_received_utc')
    if q.from_utc or q.to_utc:
        if not at:return None
        ns=a.utc_ns(at)
        if q.from_utc and ns<a.utc_ns(q.from_utc):return None
        if q.to_utc and ns>=a.utc_ns(q.to_utc):return None
    net=row.get('executable_shadow_net_edge_usd')
    if q.min_net_usd is not None and net is not None and net<q.min_net_usd:return None
    row['net_filter_status']='UNKNOWN' if net is None else 'KNOWN'
    if q.view=='liquidations' and kind not in ('LIQUIDATION','INSTRUCTION_MARKER','RELATED_NON_LIQUIDATION'):return None
    if q.view=='winners' and kind!='LIQUIDATION':return None
    if q.view=='competition' and kind not in ('LIQUIDATION','SHADOW_ONLY'):return None
    if q.view=='economics' and kind not in ('TRANSACTION','LIQUIDATION','SHADOW_ONLY'):return None
    if not q.winners:row.pop('winner',None)
    if not q.competition:row.pop('competition',None)
    if not q.economics:
        for key in ('known_costs','unknown_costs','market_gross_edge_usd','executable_shadow_net_edge_usd','realized_pnl_usd','required_capital_usd'):row.pop(key,None)
    if q.provenance!='all':row.pop('all_source_records',None)
    return display_fields(row,q)


def display_fields(value,q):
    if isinstance(value,list):return [display_fields(x,q) for x in value]
    if not isinstance(value,dict):return value
    result={}
    for key,item in value.items():
        if not q.winners and (key=='winner' or key.startswith('winner_')):continue
        if not q.competition and (key in ('competition','race','race_class','local_margin_ns','local_observed_race_margin_ns') or key.startswith('race_')):continue
        if not q.economics and (key in ('known_costs','unknown_costs','selected_costs','native_amounts','lamport_deltas','token_deltas','fundable_by_budget') or any(x in key for x in ('_usd','fee_lamports','tip_lamports','rent_lamports','economics','profit'))):continue
        result[key]=[m for m in item if m in q.markers] if key=='markers' else display_fields(item,q)
    return result


def render(records,format,stream):
    # All three render the exact same typed row. CSV nests JSON values losslessly;
    # table gives an entity index + full JSON instead of silently dropping fields.
    if format=='csv':
        w=csv.writer(stream);w.writerow(['entity_id','record_json'])
        for r in records:w.writerow([r['entity_id'],json.dumps(r,sort_keys=True,allow_nan=False)])
    elif format=='json':
        stream.write('[');sep=''
        for r in records:stream.write(sep+json.dumps(r,sort_keys=True,allow_nan=False));sep=',\n'
        stream.write(']\n')
    else:
        stream.write('ENTITY_ID\tRECORD_JSON\n')
        for r in records:stream.write(r['entity_id']+'\t'+json.dumps(r,sort_keys=True,allow_nan=False)+'\n')


def report(run,q,rid=None,stream=None):
    result=selected_result(run,rid);out,manifest=normalize(run,result)
    identity={'capture_run_id':manifest['capture_run_id'],'verification_result_id':manifest['verification_result_id'],
        'query':asdict(q),'renderer':VERSION,'dataset_sha256':manifest['records_sha256']}
    queryid=c.digest(identity);destination=run/'reports'/('query_'+queryid)
    destination.mkdir(exist_ok=True)
    c.immutable(destination/'query.json',{**identity,'report_query_identity':queryid,'report_status':manifest['report_status']})
    selected=(v for row in values(out/'all_records.jsonl') if (v:=visible(row,q)) is not None)
    render(selected,q.format,stream or sys.stdout)
    return {'dataset':str(out/'all_records.jsonl'),'report_query_identity':queryid,**manifest}


def main(argv=None):
    p=c.parser('Offline C0-E1 report; display filters never change capture/verifier/economics.')
    args=p.parse_args(argv)
    try:
        cfg=c.config(args)
        if not args.run:p.error('--run required')
        run=a.find_run(args.run);meta=report(run,cfg.report,args.verification_result)
        print(json.dumps(meta,sort_keys=True),file=sys.stderr);return 0
    except (c.ContractError,a.AuditError) as exc:print('REPORT_INCOMPLETE:',str(exc),file=sys.stderr);return 2
    except (OSError,ValueError,KeyError,TypeError):print('REPORT_INCOMPLETE: EVIDENCE_OR_IO_ERROR',file=sys.stderr);return 2

if __name__=='__main__':sys.exit(main())
