"""P6 local-only invariants. All runs/providers are temporary synthetic fixtures."""
import sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'fixtures'))
import p0_paths  # shared paths; puts project_0 on sys.path
import copy
from dataclasses import replace
import io
import itertools
import json
from pathlib import Path
import unittest
from unittest.mock import patch
from operations.verification import audit_market_quick as a
from operations.cli import measurement_contract as c
from operations.verification import measurement_verify as v
from operations.reporting import market_report as r
import p0_market as f


class Qualification(unittest.TestCase):
    setUp=f.TempRunBase.setUp
    tearDown=f.TempRunBase.tearDown
    write=f.TempRunBase.write
    setup_events=f.setup_events

    def test_display_cartesian_matrix_preserves_raw_proof_and_economics(self):
        result=self.setup_events();out,manifest=r.normalize(self.run,result)
        records=list(r.values(out/'all_records.jsonl'))
        raw_before={p.name:c.file_hash(p) for p in (self.run/'market').iterdir()}
        snapshot_before={p.name:c.file_hash(p) for p in out.glob('*.json')}
        proof_before=c.digest(result);records_before=c.digest(records);combinations=0
        for provenance,view,winners,competition,economics in itertools.product(
                ('best','all','live','index','strict','shadow'),
                ('summary','liquidations','winners','competition','economics'),(False,True),(False,True),(False,True)):
            q=c.ReportQuery(provenance=provenance,view=view,winners=winners,competition=competition,economics=economics,related_events=True)
            rows=[z for x in records if (z:=r.visible(x,q)) is not None]
            self.assertEqual(len({x['entity_id'] for x in rows}),len(rows))
            for row in rows:
                if row['record_kind']=='SHADOW_ONLY':
                    self.assertFalse(row['chain_liquidation_confirmed']);self.assertIsNone(row.get('winner'));self.assertIsNone(row.get('realized_pnl_usd'))
                if not winners:self.assertNotIn('winner',row)
                if not competition:self.assertNotIn('competition',row)
                if not economics:self.assertNotIn('transaction_fee_lamports',row)
            for fmt in ('table','json','csv'):
                stream=io.StringIO();r.render(rows,fmt,stream)
                if fmt=='json':self.assertEqual(json.loads(stream.getvalue()),rows)
                elif fmt=='csv':self.assertEqual([json.loads(x['record_json']) for x in r.csv.DictReader(io.StringIO(stream.getvalue()))],rows)
                else:self.assertEqual([json.loads(x.split('\t',1)[1]) for x in stream.getvalue().splitlines()[1:]],rows)
            combinations+=1
        self.assertEqual(combinations,240)
        self.assertEqual(c.digest(records),records_before)
        self.assertEqual(proof_before,c.digest(result))
        self.assertEqual(raw_before,{p.name:c.file_hash(p) for p in (self.run/'market').iterdir()})
        self.assertEqual(snapshot_before,{p.name:c.file_hash(p) for p in out.glob('*.json')})
        self.assertEqual(manifest['records_sha256'],c.file_hash(out/'all_records.jsonl'))

    def test_related_discriminators_cpi_lut_and_negative_programs(self):
        for name,version in itertools.product(c.RELATED,('legacy',0,1)):
            tx=f.tx(kind='CLASSIC',version=version);ix=tx['transaction']['message']['instructions'][0]
            ix['data']=a.base58_encode(a.hashlib.sha256(('global:'+name).encode()).digest()[:8])
            tx['meta']['logMessages']=[f'Program {a.PROGRAM} invoke [1]',f'Program {a.PROGRAM} success']
            self.assertEqual(a.detect(tx,10)['instruction_evidence'][0]['related_type'],name)
            foreign=copy.deepcopy(tx);foreign['transaction']['message']['accountKeys'][0]='11111111111111111111111111111111'
            self.assertFalse(any(x.get('related_type') for x in a.detect(foreign,10)['instruction_evidence']))
            text_only=copy.deepcopy(tx);text_only['transaction']['message']['instructions'][0]['data']=a.base58_encode(b'notadisc')
            text_only['meta']['logMessages'].insert(1,'Program log: Instruction: '+name)
            self.assertFalse(any(x.get('related_type') for x in a.detect(text_only,10)['instruction_evidence']))
            if version=='legacy':continue
            ix['programIdIndex']=3
            tx['transaction']['message'].update(accountKeys=['11111111111111111111111111111111',f.TARGET,f.ACTOR],addressTableLookups=[{'accountKey':'x'}],instructions=[{'programIdIndex':0,'data':'','accounts':[]}])
            tx['meta']['loadedAddresses']['readonly']=[a.PROGRAM]
            tx['meta']['innerInstructions']=[{'index':0,'instructions':[ix]}]
            decoded=a.detect(tx,10)
            self.assertEqual(decoded['instruction_evidence'][0]['position'],(0,0))
            self.assertEqual(decoded['instruction_evidence'][0]['related_type'],name)
            self.assertEqual(decoded['events'],[])

    def test_time_filters_half_open_unknown_net_retained(self):
        result=self.setup_events();out,_=r.normalize(self.run,result);records=list(r.values(out/'all_records.jsonl'))
        q=c.ReportQuery(from_utc=a.iso_ns(120*a.NS),to_utc=a.iso_ns(121*a.NS),min_net_usd=1e6)
        rows=[z for x in records if (z:=r.visible(x,q))]
        self.assertTrue(rows)
        self.assertTrue(all(x['signature']=='tx0' for x in rows))
        self.assertTrue(all(x['net_filter_status']=='UNKNOWN' for x in rows))
        for invalid in ({'types':('WRONG',)},{'min_net_usd':float('nan')},{'winners':'on'}, {'from_utc':a.iso_ns(2*a.NS),'to_utc':a.iso_ns(a.NS)}):
            with self.assertRaises(c.ContractError):c.ReportQuery(**invalid)

    def test_off_resume_result_ids_and_report_binding(self):
        with patch.object(a.RPC,'call',side_effect=AssertionError('network forbidden')):
            first=v.verify(self.run,c.VerificationConfig(),offline=True)
            one=r.report(self.run,c.ReportQuery(),first['verification_result_id'],io.StringIO())
            second=v.verify(self.run,c.VerificationConfig(),offline=True,resume=True)
            two=r.report(self.run,c.ReportQuery(),second['verification_result_id'],io.StringIO())
        self.assertEqual(first['capture_run_id'],second['capture_run_id'])
        self.assertEqual(first['verification_spec_id'],second['verification_spec_id'])
        self.assertNotEqual(first['verification_attempt_id'],second['verification_attempt_id'])
        self.assertNotEqual(first['verification_result_id'],second['verification_result_id'])
        self.assertNotEqual(one['report_query_identity'],two['report_query_identity'])
        self.assertEqual(one['historical_status'],'HISTORICAL_INCOMPLETE')
        self.assertEqual(two['live_coverage_status'],'LIVE_COVERAGE_INCOMPLETE')
        self.assertFalse(second['live_enabled']);self.assertFalse(second['submission_enabled'])
        self.assertEqual(r.selected_result(self.run,first['verification_result_id']),first)

    def test_cached_reports_revalidate_proof_and_snapshots(self):
        result=self.setup_events();out,_=r.normalize(self.run,result)
        snapshot=out/'hour_01.json';original=snapshot.read_bytes();snapshot.write_text('{}')
        with self.assertRaisesRegex(c.ContractError,'SNAPSHOT_EVIDENCE_CHANGED'):r.normalize(self.run,result)
        snapshot.write_bytes(original)
        proof=self.run/result['artifacts']['comparison']['path'];proof.write_text('{}\n')
        with self.assertRaisesRegex(c.ContractError,'RESULT_EVIDENCE_CHANGED'):r.normalize(self.run,result)

    def test_strict_fixture_replay_and_nested_batch_integrity(self):
        # Fixture proof status is supplied deliberately; this is not real chain qualification.
        self.write('shadow_evaluations.jsonl',[])
        evidence=c.finalize_capture(self.run)
        spec=c.specification(self.run,c.VerificationConfig(mode='strict'),'offline-fixture');sid=spec['verification_spec_id']
        root=self.run/'verification'/sid;batchdir=root/'batch';batchdir.mkdir(parents=True);c.immutable(root/'spec.json',spec)
        tx=f.tx('strict-event',10,150,'CLASSIC');decoded=a.detect(tx,10);ev=decoded['events'][0]
        event={**ev,'liquidation_type':'CLASSIC','transaction_index':0,'liquidator':ev['actor'],
            'visible_payments':{},'winner_net_edge_complete':False,'block_time':150,'permissionless_profit_opportunity':True}
        (batchdir/'transactions.jsonl').write_text(json.dumps({'signature':'strict-event','slot':10,'block_time':150,'raw':tx})+'\n')
        (batchdir/'liquidations.jsonl').write_text(json.dumps({'source':'canonical_market_truth','event':event})+'\n')
        (batchdir/'coverage.jsonl').write_text('')
        batch={'spec':sid,'proof':{'complete':True},'directory':'batch','files':{p.name:a.base58_encode(bytes.fromhex(c.file_hash(p))) for p in batchdir.iterdir()}}
        checkpoint=root/'batch.json';c.immutable(checkpoint,batch)
        payload={'capture_run_id':evidence['capture_run_id'],'capture_evidence_digest':evidence['evidence_digest'],
                 'verification_spec_id':sid,'verification_attempt_id':'fixture','mode':'strict',
                 'summary':{'status':'CANONICAL_VERIFIED'},'finished_unix_ns':1,
                 'artifacts':{'batch':{'path':str(checkpoint.relative_to(self.run)),'sha256':c.file_hash(checkpoint)}}}
        result={**payload,'verification_result_id':c.digest(payload)}
        out,manifest=r.normalize(self.run,result)
        self.assertEqual(manifest['economics_replay']['status'],'PASS',manifest['economics_replay'])
        self.assertEqual(manifest['live_coverage_status'],'LIVE_COVERAGE_INCOMPLETE')
        final=json.loads((out/'economics_final.json').read_text())
        self.assertEqual(final['market_total_liquidations'],1)
        self.assertEqual(final['economics']['cumulative']['market']['ALL']['events'],1)
        self.assertFalse(final['master_pass']);self.assertFalse(final['live_enabled'])
        self.assertEqual(len(final['economics']['cumulative']['capital']),6)
        # A checkpoint with raw liquidation TXs but no canonical event cannot produce zero economics.
        (batchdir/'liquidations.jsonl').write_text('')
        batch['files']['liquidations.jsonl']=a.base58_encode(bytes.fromhex(c.file_hash(batchdir/'liquidations.jsonl')))
        second=root/'batch_without_event.json';c.immutable(second,batch)
        partial={**payload,'artifacts':{'batch':{'path':str(second.relative_to(self.run)),'sha256':c.file_hash(second)}}}
        partial={**partial,'verification_result_id':c.digest(partial)}
        partial_out,partial_manifest=r.normalize(self.run,partial)
        self.assertEqual(partial_manifest['economics_replay']['reason'],'CANONICAL_EVENT_INPUT_MISMATCH')
        self.assertEqual(json.loads((partial_out/'economics_final.json').read_text())['economics']['status'],'UNKNOWN')
        (batchdir/'transactions.jsonl').write_text('{}\n')
        with self.assertRaisesRegex(c.ContractError,'STRICT_BATCH_EVIDENCE_CHANGED'):r.normalize(self.run,result)

    def test_crosscheck_offline_never_canonical_or_paid(self):
        with patch.object(a.RPC,'call',side_effect=AssertionError('network forbidden')):
            result=v.verify(self.run,c.VerificationConfig(mode='crosscheck'),offline=True)
        self.assertEqual(result['summary']['status'],'HISTORICAL_INCOMPLETE')
        self.assertFalse(result['summary']['canonical_verified'])
        self.assertFalse(result['master_pass']);self.assertFalse(result['submission_enabled'])

    def test_truncated_raw_jsonl_stops_report(self):
        (self.run/'market/transactions.jsonl').write_text('{"signature":"partial"')
        with self.assertRaisesRegex(a.AuditError,'TRUNCATED'):r.normalize(self.run)

if __name__=='__main__':unittest.main()
