import copy
from dataclasses import replace
import io
import csv
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import audit_market_quick as a
import measurement_contract as c
import market_report as r
import test_audit_market_quick as f

class Reports(unittest.TestCase):
    setUp=f.TempRun.setUp
    tearDown=f.TempRun.tearDown
    write=f.TempRun.write
    def setup_events(self):
        live=[];historical=[]
        for i,kind in enumerate(c.KINDS):
            tx=f.tx('tx'+str(i),10+i,120+i,kind)
            live.append({'source':'transactionSubscribe','signature':'tx'+str(i),'slot':10+i,'t_rx_ns':i+1,'observed_at_utc':a.iso_ns((120+i)*a.NS),'raw':tx})
            d=a.detect(tx,10+i);d['block_time']=120+i;d['membership']='IN_WINDOW'
            historical.append({'historical':d,'darkbot':{}})
        for i,name in enumerate(c.RELATED):
            tx=f.tx('related'+str(i),30+i,140+i,'CLASSIC')
            tx['transaction']['message']['instructions'][0]['data']=a.base58_encode(a.hashlib.sha256(('global:'+name).encode()).digest()[:8])
            tx['meta']['logMessages']=[f'Program {a.PROGRAM} invoke [1]',f'Program {a.PROGRAM} success']
            live.append({'source':'transactionSubscribe','signature':'related'+str(i),'slot':30+i,'raw':tx})
            d=a.detect(tx,30+i);d['block_time']=140+i
            self.assertEqual(d['events'],[])
            self.assertEqual(d['instruction_evidence'][0]['related_type'],name)
            historical.append({'historical':d,'darkbot':{}})
        self.write('market/transactions.jsonl',live)
        self.write('opportunity_economics.jsonl',[{'target':f.TARGET,'market':None,'our':{'account':f.TARGET,'economics_known':True,'net_profit':9999,'signature':'shadow-sig','timing':{'t_rx_ns':7}}}])
        evidence=c.finalize_capture(self.run)
        out=self.run/'verification/test';out.mkdir(parents=True)
        self.write('verification/test/comparison.jsonl',historical)
        payload={'capture_run_id':evidence['capture_run_id'],'verification_spec_id':'test','verification_attempt_id':'test',
            'mode':'index','summary':{'status':'INDEX_VERIFIED'},'artifacts':{'comparison':{'path':'verification/test/comparison.jsonl','sha256':c.file_hash(out/'comparison.jsonl')}},'finished_unix_ns':1}
        result={**payload,'verification_result_id':c.digest(payload)}
        c.immutable(out/'results'/(result['verification_result_id']+'.json'),result)
        return result

    def test_four_classes_six_markers_four_related_no_double_count(self):
        result=self.setup_events();out,m=r.normalize(self.run,result)
        records=list(r.values(out/'all_records.jsonl'))
        self.assertEqual(sum(x['record_kind']=='LIQUIDATION' for x in records),4)
        self.assertEqual(sum(x['record_kind']=='INSTRUCTION_MARKER' for x in records),6)
        related=[x for x in records if x['record_kind']=='RELATED_NON_LIQUIDATION']
        self.assertEqual(len(related),4)
        self.assertTrue(all(not x['chain_liquidation_confirmed'] and x['winner'] is None for x in related))
        shadow=[x for x in records if x['record_kind']=='SHADOW_ONLY']
        self.assertEqual(len(shadow),1);self.assertIsNone(shadow[0]['realized_pnl_usd'])
        for row in records:
            if row['record_kind']=='LIQUIDATION':self.assertEqual({x['source'] for x in row['provenance']},{'index','live'})
        self.assertEqual(m['live_coverage_status'],'LIVE_COVERAGE_INCOMPLETE')
        self.assertFalse(m['c0_e1_master_pass'])

    def test_individual_filters_and_raw_hash_invariance(self):
        result=self.setup_events();out,m=r.normalize(self.run,result)
        records=list(r.values(out/'all_records.jsonl'));before=c.file_hash(out/'all_records.jsonl')
        raw=c.file_hash(self.run/'market/transactions.jsonl')
        for name in c.MARKERS:
            q=replace(c.ReportQuery(),markers=(name,),provenance='all')
            view=[v for x in records if (v:=r.visible(x,q))]
            self.assertEqual(sum(x['record_kind']=='INSTRUCTION_MARKER' for x in view),1)
            self.assertEqual(sum(x['record_kind']=='LIQUIDATION' for x in view),4)
        for kind in c.KINDS:
            q=replace(c.ReportQuery(),types=(kind,))
            self.assertEqual(sum(r.visible(x,q) is not None for x in records if x['record_kind']=='LIQUIDATION'),1)
        for kind in c.RELATED:
            q=replace(c.ReportQuery(),related_events=True,related_types=(kind,))
            self.assertEqual(sum(r.visible(x,q) is not None for x in records if x['record_kind']=='RELATED_NON_LIQUIDATION'),1)
        self.assertEqual(raw,c.file_hash(self.run/'market/transactions.jsonl'))
        self.assertEqual(before,c.file_hash(out/'all_records.jsonl'))
        self.assertEqual(r.normalize(self.run,result)[1],m)

    def test_renderer_semantics_equal_and_identity_result_bound(self):
        result=self.setup_events();out,_=r.normalize(self.run,result)
        records=[v for x in r.values(out/'all_records.jsonl') if (v:=r.visible(x,c.ReportQuery(provenance='all',related_events=True)))]
        outputs={}
        for fmt in ('json','csv','table'):
            s=io.StringIO();r.render(iter(records),fmt,s);outputs[fmt]=s.getvalue()
        self.assertEqual(json.loads(outputs['json']),records)
        self.assertEqual([json.loads(x['record_json']) for x in csv.DictReader(io.StringIO(outputs['csv']))],records)
        self.assertEqual([json.loads(x.split('\t',1)[1]) for x in outputs['table'].splitlines()[1:]],records)
        one=r.report(self.run,c.ReportQuery(),result['verification_result_id'],io.StringIO())
        two=r.report(self.run,c.ReportQuery(),result['verification_result_id'],io.StringIO())
        self.assertEqual(one['report_query_identity'],two['report_query_identity'])

    def test_failed_related_and_priority_requested_limit(self):
        t=f.tx(kind='CLASSIC');keys=t['transaction']['message']['accountKeys'];keys.append('ComputeBudget111111111111111111111111111111')
        for raw in [bytes([2])+(200000).to_bytes(4,'little'),bytes([3])+(1001).to_bytes(8,'little')]:
            t['transaction']['message']['instructions'].insert(0,{'programIdIndex':3,'accounts':[],'data':a.base58_encode(raw)})
        t['meta']['computeUnitsConsumed']=1
        self.assertEqual(r.priority_fee(t),201)
        t['transaction']['message']['instructions'].pop(0);self.assertIsNone(r.priority_fee(t))
        for name in c.RELATED:
            t=f.tx(kind='CLASSIC');t['transaction']['message']['instructions'][0]['data']=a.base58_encode(a.hashlib.sha256(('global:'+name).encode()).digest()[:8])
            t['meta']['err']={'failed':True};d=a.detect(t,10)
            self.assertEqual(d['classification'],'FAILED_TRANSACTION');self.assertFalse(d['events'])
            self.assertEqual(d['instruction_evidence'][0]['related_type'],name)

    def test_interrupted_report_resume_and_foreign_result_rejected(self):
        result=self.setup_events()
        original=r.write_snapshots
        with patch.object(r,'write_snapshots',side_effect=OSError('interrupted')):
            with self.assertRaises(OSError):r.normalize(self.run,result)
        out,manifest=r.normalize(self.run,result)
        self.assertEqual(r.normalize(self.run,result)[1],manifest)
        self.assertTrue((out/'economics_final.json').exists())
        foreign={**result,'capture_run_id':'another-run'}
        with self.assertRaisesRegex(c.ContractError,'RESULT_CAPTURE_MISMATCH'):r.normalize(self.run,foreign)

    def test_unknown_fee_and_no_verification_never_zero_market(self):
        self.write('market/transactions.jsonl',[{'source':'logsSubscribe','signature':'raw','slot':1,'success':True,'raw':{'value':{'logs':[]}}}])
        out,m=r.normalize(self.run)
        row=next(r.values(out/'all_records.jsonl'))
        self.assertIsNone(row['transaction_fee_lamports']);self.assertIsNone(row['realized_pnl_usd'])
        self.assertEqual(m['report_status'],'PROVISIONAL/INCOMPLETE')
        final=json.loads((out/'economics_final.json').read_text())
        self.assertIsNone(final['market_total_liquidations'])
        self.assertEqual(final['end_ns'],200*a.NS)

if __name__=='__main__':unittest.main()
