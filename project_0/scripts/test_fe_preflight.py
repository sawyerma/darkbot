import base64
import unittest
import io,json
import audit_market_quick as a
import fe_readonly_preflight as p

class Preflight(unittest.TestCase):
    def test_three_read_calls_no_send_or_simulation_and_no_parity_claim(self):
        class RPC:
            def __init__(self):self.calls=[]
            def call(self,method,params):
                self.calls.append(method)
                if len(self.calls)==1:
                    raw=(2).to_bytes(4,'little')+bytes([7])*32;owner=p.LOADER;slot=10
                elif len(self.calls)==2:
                    raw=(3).to_bytes(4,'little')+(9).to_bytes(8,'little')+bytes([0])*33;owner=p.LOADER;slot=11
                else:
                    return {'context':{'slot':12},'value':[None,None]}
                return {'context':{'slot':slot},'value':{'owner':owner,'executable':True,'data':[base64.b64encode(raw).decode(),'base64']}}
        rpc=RPC();result=p.probe(rpc)
        self.assertEqual(rpc.calls,['getAccountInfo','getAccountInfo','getMultipleAccounts'])
        self.assertEqual(result['deployment_slot'],9);self.assertEqual(result['source_parity'],'UNKNOWN')
        self.assertFalse(result['pilot_ready']);self.assertFalse(result['submission_enabled'])
    def test_owner_encoding_and_loader_mismatch_fail_closed(self):
        for value in [None,{'owner':'wrong'}, {'owner':p.LOADER,'data':['','jsonParsed']}, {'owner':p.LOADER,'data':['AAAAAA==','base64']}]:
            with self.assertRaises(a.AuditError):p.account(value,p.LOADER,2)

    def test_real_transport_allowlist_stays_readonly_and_default_index_only(self):
        body=lambda req,timeout:io.BytesIO(json.dumps({'result':{'context':{'slot':1},'value':None}}).encode())
        rpc=a.RPC('http://fixture.invalid',opener=body,sleep=lambda _:None,allowed_methods={'getAccountInfo','getMultipleAccounts'})
        self.assertIsNone(rpc.call('getAccountInfo',[])['value'])
        with self.assertRaises(a.AuditError):rpc.call('sendTransaction',[])
        with self.assertRaises(a.AuditError):a.RPC('http://fixture.invalid',allowed_methods={'sendTransaction'})
        default=a.RPC('http://fixture.invalid',opener=body)
        with self.assertRaises(a.AuditError):default.call('getAccountInfo',[])
