import json
from pathlib import Path
p=Path('gate0-evidence/2026-09-27-g01/flap_fixture_rpc/001.response.json')
# first response contains launch tx
j=json.loads(p.read_text())[0]['result']; raw=bytes.fromhex(j['input'][2:]); body=raw[4:]
# outer tuple starts at byte offset in body (0x20)
tuple_body=body[int.from_bytes(body[:32],'big'):]
W=[tuple_body[i:i+32] for i in range(0,len(tuple_body),32)]
def u(i): return int.from_bytes(W[i],'big')
def addr(i): return '0x'+W[i][-20:].hex()
def dyn(i):
 off=u(i); n=int.from_bytes(tuple_body[off:off+32],'big'); return tuple_body[off+32:off+32+n]
out={
 'txHash':j['hash'],'selector':j['input'][:10],'to':j['to'],
 'function':'newTokenV6','params':{
  'name':dyn(0).decode(),'symbol':dyn(1).decode(),'meta':dyn(2).decode(),
  'dexThresh':u(3),'salt':'0x'+W[4].hex(),'migratorType':u(5),
  'quoteToken':addr(6),'quoteAmt':u(7),'beneficiary':addr(8),
  'permitData':'0x'+dyn(9).hex(),'extensionID':'0x'+W[10].hex(),
  'extensionData':'0x'+dyn(11).hex(),'dexId':u(12),'lpFeeProfile':u(13),
  'buyTaxRate':u(14),'sellTaxRate':u(15),'taxDuration':u(16),
  'antiFarmerDuration':u(17),'mktBps':u(18),'deflationBps':u(19),
  'dividendBps':u(20),'lpBps':u(21),'minimumShareBalance':u(22),
  'dividendToken':addr(23),'commissionReceiver':addr(24),'tokenVersion':u(25),
 },'checks':{
  'outer_tuple_offset':int.from_bytes(body[:32],'big'),'token_version_is_taxed_v3':u(25)==6,
  'tax_sum_bps':u(18)+u(19)+u(20)+u(21),'quote_value_matches_tx':int(j['value'],16)==u(7)
 }}
Path('gate0-evidence/2026-09-27-g01/flap_v6_fixture_decoded.json').write_text(json.dumps(out,indent=2,ensure_ascii=False)+'\n')
print(json.dumps(out,indent=2,ensure_ascii=False))
