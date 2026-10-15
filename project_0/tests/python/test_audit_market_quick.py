"""Offline P3 qualification: real RPC fixtures + generated fault/transport fixtures."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0/scripts on sys.path
import base64
import copy
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest
import urllib.error
from unittest.mock import patch
import audit_market_quick as a

from p0_market import START,END,KEY,TARGET,ACTOR,NAMES,payload,tx,row,FakeRPC,TempRunBase

ROOT=p0_paths.PROJECT  # docs/evidence/AUDIT_20261005 is read in place, never copied


class TempRun(TempRunBase,unittest.TestCase):
    def test_utc_bounds_ignore_last_received_slot(self):
        w=a.read_window(self.run)
        self.assertEqual((w['start_ns'],w['end_ns']),(START,END))
        self.assertNotIn('slot',w)
        summary=json.loads((self.run/'market/summary.json').read_text());summary['observation_ended_at']=a.iso_ns(130*a.NS)
        (self.run/'market/summary.json').write_text(json.dumps(summary))
        self.assertEqual(a.read_window(self.run)['end_ns'],END)

    def test_p1_manifest_exact_nanosecond_boundary(self):
        origin=START+123456789
        (self.run/'measurement_manifest.json').write_text(json.dumps({'schema':'C0-E1/MeasurementManifest/1','plan':{'duration_seconds':61}}))
        self.write('measurement_phases.jsonl',[{'phase':'BOOTSTRAP'},{'phase':'CAPTURE','details':{'capture_start_unix_ns':origin,'capture_cutoff_unix_ns':origin+61*a.NS}}])
        w=a.read_window(self.run)
        self.assertEqual(w['start_ns'],origin);self.assertEqual(a.utc_ns(w['end_utc']),origin+61*a.NS)

    def test_pagination_dedup_and_short_pages_do_not_stop_early(self):
        rpc=FakeRPC({None:[row('head',30,220),row('one',20,150)],
            'one':[row('one',20,150),row('two',19,149)],'two':[row('old',10,90)]},
            {'one':tx('one',20,150),'two':tx('two',19,149)})
        s=self.execute(rpc)
        self.assertEqual(s['status'],'INDEX_VERIFIED');self.assertEqual(s['qualified_liquidations']['RECEIVERSHIP'],2)
        self.assertEqual(rpc.methods['getTransaction'],2)
        self.assertFalse(s['canonical_verified']);self.assertFalse(s['c0_e1_master_pass'])

    def test_resume_does_not_repeat_pages_or_completed_transactions(self):
        rpc=self.normal_rpc()
        first=self.execute(rpc,pages=1)
        self.assertEqual(first['reason'],'INDEX_PAGE_BUDGET')
        second_rpc=self.normal_rpc();second=self.execute(second_rpc)
        self.assertEqual(second['status'],'INDEX_VERIFIED')
        self.assertEqual(second_rpc.requests,[('getSignaturesForAddress',[a.PROGRAM,{'commitment':'finalized','limit':1000,'before':'one'}])])
        third_rpc=self.normal_rpc();third=self.execute(third_rpc)
        self.assertEqual(third['status'],'INDEX_VERIFIED');self.assertEqual(third_rpc.calls,0)

    def test_resume_raw_page_after_checkpoint_interruption(self):
        rpc=self.normal_rpc();audit=a.Audit(self.run,rpc)
        original=a.atomic_json
        def interrupt(path,value):
            original(path,value)
            if path.parent.name=='pages':raise KeyboardInterrupt()
        with patch.object(a,'atomic_json',side_effect=interrupt):
            self.assertEqual(audit.execute()['reason'],'INTERRUPTED_RESUMABLE')
        audit.close()
        second=self.normal_rpc();s=self.execute(second)
        self.assertEqual(s['status'],'INDEX_VERIFIED')
        self.assertEqual(second.methods['getSignaturesForAddress'],1)

    def test_null_blocktime_resolved_by_transaction_or_remains_unknown(self):
        for resolved in [150,None]:
            with self.subTest(resolved=resolved):
                rpc=FakeRPC({None:[row('head',30,220),row('one',20,None)],'one':[row('old',10,90)]},{'one':tx('one',20,resolved)})
                # separate output for each source identity
                if resolved is None:
                    self.run=self.run.parent/'other';(self.run/'market').mkdir(parents=True)
                    (self.run/'market/summary.json').write_text(json.dumps({'observation_started_at':a.iso_ns(START),'observation_ended_at':a.iso_ns(END),'configured_duration_seconds':100}))
                s=self.execute(rpc)
                self.assertEqual(s['status'],'INDEX_VERIFIED' if resolved else 'HISTORICAL_INCOMPLETE')
                if resolved is None:self.assertEqual(s['unknown_transactions'],1)

    def test_missing_transaction_retry_and_429_saved(self):
        rpc=self.normal_rpc();rpc.txs['one']=None
        s=self.execute(rpc);self.assertEqual(s['unresolved_transactions'],1)
        rpc=self.normal_rpc();rpc.txs['one']=a.AuditError('RPC_RATE_LIMIT')
        self.assertEqual(self.execute(rpc)['reason'],'RPC_RATE_LIMIT')
        rpc=self.normal_rpc();s=self.execute(rpc)
        self.assertEqual(s['status'],'INDEX_VERIFIED');self.assertEqual(rpc.methods['getSignaturesForAddress'],0)

    def test_page_limits_and_exhaustion_never_prove_zero(self):
        s=self.execute(FakeRPC({None:[]},{}))
        self.assertEqual(s['status'],'HISTORICAL_INCOMPLETE');self.assertFalse(s['zero_liquidations_proven_within_index_scope'])

    def test_failed_and_unknown_versions_not_zero_success(self):
        rpc=self.normal_rpc();rpc.pages[None][1]['err']={'failed':True};rpc.txs['one']['meta']['err']={'failed':True}
        s=self.execute(rpc);self.assertEqual(s['status'],'INDEX_VERIFIED');self.assertEqual(s['qualified_liquidations']['RECEIVERSHIP'],0)

    def test_outage_distinct_from_live_signature_missing_detection(self):
        self.write('market/coverage.jsonl',[
            {'event':'MARKET_SLOT_BOUNDARY_EVIDENCE','timestamp_utc':a.iso_ns(START),'slot':1},
            {'event':'MARKET_SLOT_BOUNDARY_EVIDENCE','timestamp_utc':a.iso_ns(140*a.NS),'slot':19},
            {'event':'GAP','timestamp_utc':a.iso_ns(140*a.NS),'counted':True}])
        s=self.execute(self.normal_rpc())
        self.assertEqual(s['comparison_counts']['MISSING_DURING_OUTAGE'],1)
        self.assertEqual(s['live_coverage']['tail_without_slot_evidence_seconds'],60)

    def test_live_signature_and_amount_type_comparison(self):
        self.write('market/transactions.jsonl',[{'signature':'one','slot':20,'source':'logsSubscribe','success':True}])
        event=a.decode_payload(payload('RECEIVERSHIP'))
        self.write('market/liquidations.jsonl',[{**event,'signature':'one','slot':20,'source':'live','event_index':0,'seized_value_usd':999}])
        s=self.execute(self.normal_rpc());self.assertEqual(s['comparison_counts']['AMOUNT_MISMATCH'],1)

    def test_source_mutation_rejected_and_no_rpc_access_reports_open(self):
        self.assertEqual(self.execute(None)['reason'],'RPC_ACCESS_MISSING')
        (self.run/'market/summary.json').write_text('{}')
        with self.assertRaises(a.AuditError):a.Audit(self.run,self.normal_rpc())

    def test_unknown_version_provider_error_does_not_skip_other_transactions(self):
        rpc=FakeRPC({None:[row('head',30,220),row('one',20,150),row('two',19,149)],'two':[row('old',10,90)]},
            {'one':a.AuditError('RPC_UNSUPPORTED_TX_VERSION'),'two':tx('two',19,149)})
        s=self.execute(rpc)
        self.assertEqual(s['status'],'HISTORICAL_INCOMPLETE')
        self.assertEqual(s['qualified_liquidations']['RECEIVERSHIP'],1)
        self.assertEqual(s['unresolved_transactions'],1)

    def test_shadow_exact_event_identity_and_unknown_economics_preserved(self):
        ev=a.decode_payload(payload('RECEIVERSHIP'))
        ev.update(signature='one',slot=20,event_index=0,liquidation_type='RECEIVERSHIP')
        self.write('canonical_market_shadow.jsonl',[{'join':{'market':ev,'our':{'economics_known':False,'selected_costs':None}}}])
        self.execute(self.normal_rpc())
        rows=[json.loads(s) for s in (self.run/'verification/index_v3/comparison.jsonl').read_text().splitlines()]
        ev=rows[0]['darkbot']['events'][0]
        self.assertEqual(ev['shadow_status'],'SHADOW_MATCH')
        self.assertIsNone(ev['shadow_evidence']['our']['selected_costs'])
        self.assertFalse(ev['historical_reconstruction_is_live_race_evidence'])

    def test_type_mismatch_and_slot_mismatch(self):
        self.write('market/transactions.jsonl',[{'signature':'one','slot':19,'source':'logsSubscribe'}])
        self.write('canonical_market_shadow.jsonl',[{'join':{'market':{'signature':'one','slot':20,'event_index':0,'type':'CLASSIC','target':TARGET,'actor':ACTOR}}}])
        s=self.execute(self.normal_rpc())
        self.assertEqual(s['comparison_counts']['SLOT_MISMATCH'],1)
        row=json.loads((self.run/'verification/index_v3/comparison.jsonl').read_text())
        self.assertEqual(row['darkbot']['events'][0]['comparisons'][0]['status'],'TYPE_MISMATCH')

    def test_malformed_cursor_and_conflicting_duplicates_fail_closed(self):
        rpc=FakeRPC({None:[row('head',30,220),row('one',20,150)],'one':[row('one',20,151)]},{})
        s=self.execute(rpc);self.assertEqual(s['reason'],'INDEX_DUPLICATE_CONFLICT')
        self.assertEqual(s['status'],'HISTORICAL_INCOMPLETE')

    def test_empty_input_file_cannot_mutate_between_resumes(self):
        self.execute(None)
        self.write('market/transactions.jsonl',[{'signature':'one','slot':20,'source':'logsSubscribe'}])
        self.assertEqual(self.execute(self.normal_rpc())['reason'],'LOCAL_INPUT_CHANGED_DURING_RESUME')



class DecodeTests(unittest.TestCase):
    def test_all_four_valid_payloads(self):
        for kind in a.KINDS:
            result=a.detect(tx(kind=kind),10)
            self.assertEqual(result['classification'],'CLASSIFIED',result)
            self.assertEqual(result['program_events'],{kind:1})
            self.assertEqual(result['events'][0]['target'],TARGET)
            self.assertEqual(result['fee_lamports'],12345)

    def test_start_end_not_double_counted(self):
        for kind in ('RECEIVERSHIP','FORCED_DELEVERAGE'):
            r=a.detect(tx(kind=kind),10)
            self.assertEqual(r['program_events'],{kind:1})
            self.assertEqual(r['classification'],'CLASSIFIED',r)
            self.assertEqual(r['liquidation_pairs'][0]['status'],'COMPLETE')

    def test_incomplete_reversed_duplicate_and_mismatched_pairs(self):
        for kind in ('RECEIVERSHIP','FORCED_DELEVERAGE'):
            for fault in ('start_only','end_only','reversed','duplicate_start','target','actor','cpi_start'):
                with self.subTest(kind=kind,fault=fault):
                    t=tx(kind=kind);ix=t['transaction']['message']['instructions']
                    if fault=='start_only':
                        ix.pop();t['meta']['logMessages']=t['meta']['logMessages'][:2]
                    elif fault=='end_only':
                        ix.pop(0);t['meta']['logMessages']=t['meta']['logMessages'][2:]
                    elif fault=='reversed':ix.reverse()
                    elif fault=='duplicate_start':ix.insert(0,copy.deepcopy(ix[0]))
                    elif fault in ('target','actor'):ix[0]['accounts'][0 if fault=='target' else 3]=0
                    else:
                        start=ix[0];ix[0]={'programIdIndex':1,'data':'','accounts':[]}
                        t['meta']['innerInstructions']=[{'index':0,'instructions':[start]}]
                    r=a.detect(t,10)
                    self.assertEqual(r['classification'],'UNKNOWN',r)
                    self.assertEqual(r['liquidation_pairs'][0]['status'],'INCOMPLETE_OR_AMBIGUOUS')
                    t['meta']['err']={'InstructionError':[0,'failed']}
                    r=a.detect(t,10)
                    self.assertEqual(r['classification'],'FAILED_TRANSACTION')
                    self.assertEqual(r['events'],[])

    def test_duplicate_event_cannot_fill_other_instruction_position(self):
        t=tx(kind='CLASSIC');t['transaction']['message']['instructions']*=2
        logs=t['meta']['logMessages'];event=logs[1]
        t['meta']['logMessages']=[logs[0],event,event,logs[2],logs[0],logs[2]]
        r=a.detect(t,10)
        self.assertEqual(r['program_events'],{'CLASSIC':2})
        self.assertEqual(r['classification'],'UNKNOWN')
        self.assertIn('COMPLETION_EVENT_POSITION_AMBIGUOUS',r['unknown_reasons'])

    def test_six_markers_exact_discriminators(self):
        self.assertEqual(len(a.INSTRUCTION_TYPES),6)
        for name,kind in a.INSTRUCTION_TYPES.items():
            discriminator=a.hashlib.sha256(('global:'+name).encode()).digest()[:8]
            self.assertEqual(a.DISC[discriminator],kind)
            self.assertEqual(a.MARKER_NAMES[discriminator],name)

    def test_loaded_program_key_cpi_and_supported_versions(self):
        t=tx(version=0,kind='CLASSIC');ix=t['transaction']['message']['instructions'][0];ix['programIdIndex']=3
        t['transaction']['message'].update(accountKeys=['11111111111111111111111111111111',TARGET,ACTOR],addressTableLookups=[{'accountKey':'x'}],instructions=[{'programIdIndex':0,'data':'','accounts':[]}])
        t['meta']['loadedAddresses']['readonly']=[a.PROGRAM]
        t['meta']['innerInstructions']=[{'index':0,'instructions':[ix]}]
        t['meta']['logMessages']=['Program 11111111111111111111111111111111 invoke [1]']+[s.replace('invoke [1]','invoke [2]') for s in t['meta']['logMessages']]+['Program 11111111111111111111111111111111 success']
        for version in [0,1]:
            t['version']=version;r=a.detect(t,10)
            self.assertEqual(r['classification'],'CLASSIFIED',r)
            self.assertEqual(r['events'][0]['instruction_position'],(0,0))
        del t['meta']['loadedAddresses'];self.assertEqual(a.detect(t,10)['classification'],'UNKNOWN')

    def test_truncated_malformed_foreign_and_failed(self):
        t=tx();t['meta']['logMessages'].insert(2,'Log truncated');self.assertEqual(a.detect(t,10)['classification'],'UNKNOWN')
        t=tx();t['meta']['logMessages'][-2]='Program data: !!!!';self.assertEqual(a.detect(t,10)['classification'],'UNKNOWN')
        t=tx();t['meta']['logMessages']=[s.replace(a.PROGRAM,'11111111111111111111111111111111') for s in t['meta']['logMessages']]
        r=a.detect(t,10);self.assertEqual(r['program_events'],{});self.assertEqual(r['classification'],'UNKNOWN')
        t=tx();t['meta']['err']={'InstructionError':[0,'bad']};r=a.detect(t,10);self.assertEqual(r['classification'],'FAILED_TRANSACTION');self.assertEqual(r['events'],[])
        t=tx();t['version']=2;self.assertEqual(a.detect(t,10)['classification'],'UNKNOWN')
        t=tx();t['meta']=None;self.assertEqual(a.detect(t,10)['classification'],'UNKNOWN')

    def test_real_six_mainnet_receivership_fixtures(self):
        fixtures=[]
        for p in (ROOT/'docs/evidence/AUDIT_20261005').glob('*.json'):
            v=json.loads(p.read_text())
            if isinstance(v,dict):v=v.get('result',v)
            if isinstance(v,dict) and 'transaction' in v:
                fixtures.append((p,v))
        self.assertEqual(len(fixtures),6)
        for p,t in fixtures:
            r=a.detect(t,t['slot'])
            self.assertEqual(r['classification'],'CLASSIFIED',(p.name,r['unknown_reasons']))
            self.assertEqual(r['program_events'].get('RECEIVERSHIP'),1,p.name)
            self.assertEqual(r['signature'],p.stem)

    def test_completion_identity_and_caught_cpi_fail_closed(self):
        t=tx();t['transaction']['message']['instructions'][-1]['accounts'][0]=2
        self.assertEqual(a.detect(t,10)['classification'],'UNKNOWN')
        t=tx();t['meta']['logMessages'][-1]=f'Program {a.PROGRAM} failed: caught'
        r=a.detect(t,10);self.assertEqual(r['program_events'],{});self.assertEqual(r['classification'],'UNKNOWN')

    def test_multiple_events_and_positions(self):
        t=tx(kind='CLASSIC');t['transaction']['message']['instructions']*=2;t['meta']['logMessages']*=2
        r=a.detect(t,10);self.assertEqual(r['classification'],'CLASSIFIED')
        self.assertEqual([e['instruction_position'] for e in r['events']],[(0,None),(1,None)])
        self.assertEqual([e['event_index'] for e in r['events']],[0,1])



class HttpTests(unittest.TestCase):
    def test_429_retry_after_and_counter_includes_failed_attempts(self):
        times=[0.0];sleeps=[];calls=[]
        def sleep(s):sleeps.append(s);times[0]+=s
        def opener(*args,**kwargs):
            calls.append(1)
            if len(calls)==1:raise urllib.error.HTTPError('https://SECRET',429,'SECRET',{'Retry-After':'3'},None)
            return io.BytesIO(b'{"result": []}')
        rpc=a.RPC('https://SECRET',opener=opener,sleep=sleep,clock=lambda:times[0])
        self.assertEqual(rpc.call('getSignaturesForAddress',[]),[])
        self.assertEqual(rpc.calls,2);self.assertGreaterEqual(max(sleeps),3)

    def test_persistent_429_is_bounded_and_secret_free(self):
        def opener(*args,**kwargs):raise urllib.error.HTTPError('https://SECRET',429,'SECRET',{},None)
        rpc=a.RPC('https://SECRET',attempts=2,opener=opener,sleep=lambda _:None)
        with self.assertRaisesRegex(a.AuditError,'^RPC_RATE_LIMIT$'):rpc.call('getTransaction',[])
        self.assertEqual(rpc.calls,2)
        with self.assertRaisesRegex(a.AuditError,'INDEX_METHOD_FORBIDDEN'):rpc.call('getBlock',[])


if __name__=='__main__':unittest.main(verbosity=2)
