"""Offline G0.2 evidence checks. No network, signer, production adapter or benchmark."""
import hashlib
import json
from pathlib import Path
from keccak_audit import k256

P = Path(__file__).resolve().parent / '2026-09-27-g01'
inputs = {}


def read(name):
    raw = (P / name).read_bytes()
    inputs[name] = hashlib.sha256(raw).hexdigest()
    return json.loads(raw)


def replies(name):
    rows = read(name)
    assert len({r['id'] for r in rows}) == len(rows)
    assert all('error' not in r for r in rows)
    return {r['id']: r['result'] for r in rows}


def create2(deployer, salt, init):
    assert len(deployer) == 20 and len(salt) == 32
    pre = b'\xff' + deployer + salt + k256(init)
    assert len(pre) == 85
    return '0x' + k256(pre)[12:].hex(), pre


# Published EIP-1014 examples, independent of the Flap expected address.
vectors = [
    ('00'*20, '00'*32, '00', '4d1a2e2bb4f88f0250f26ffff098b0b30b26bf38'),
    ('deadbeef'+'00'*16, '00'*32, '00', 'b928f69bb1d91cd65274e3c79d8986362984fda3'),
    ('deadbeef'+'00'*16, '00'*12+'feed'+'00'*18, '00', 'd04116cdd17bebe565eb2422f2497e06cc1c9833'),
    ('00'*20, '00'*32, 'deadbeef', '70f2b2914a2a4b783faefb75f459a580616fcb5e'),
    ('00'*16+'deadbeef', '00'*28+'cafebabe', 'deadbeef', '60f3f640a8508fc6a86d45df051962668e1e8ac7'),
    ('00'*16+'deadbeef', '00'*28+'cafebabe', 'deadbeef'*11, '1d8bfdc5d46dc4f61d6b6115972536ebe6a8854c'),
    ('00'*20, '00'*32, '', 'e33c0c7f7df4809055c3eba6c09cfe4baf1bd9e0'),
]
for address, salt, init, expected in vectors:
    assert create2(bytes.fromhex(address), bytes.fromhex(salt), bytes.fromhex(init))[0] == '0x'+expected

fixture = replies('flap_fixture_rpc/001.response.json')
tx, receipt = fixture[1], fixture[2]
assert tx['hash'] == receipt['transactionHash'] and receipt['status'] == '0x1'
assert tx['blockHash'] == receipt['blockHash']
body = bytes.fromhex(tx['input'][10:])
assert int.from_bytes(body[:32], 'big') == 32
body = body[32:]
words = [body[i:i+32] for i in range(0, 26*32, 32)]
assert len(words[-1]) == 32
assert tx['input'][:10] == '0x8cb5772c' and int.from_bytes(words[25], 'big') == 6
salt, quote = words[4], '0x'+words[6][-20:].hex()
runtime_rows = read('flap_token_code_rpc3/001.response.json')
runtime = bytes.fromhex(next(r['result'] for r in runtime_rows if r['id'] == 2)[2:])
assert len(runtime) == 45
assert runtime[:10].hex() == '363d3d373d3d3d363d73'
assert runtime[30:].hex() == '5af43d82803e903d91602b57fd5bf3'
implementation = '0x'+runtime[10:30].hex()
init = bytes.fromhex('3d602d80600a3d3981f3') + runtime
predicted, preimage = create2(bytes.fromhex(tx['to'][2:]), salt, init)
created_topic = '0x'+k256(b'TokenCreated(uint256,address,uint256,address,string,string,string)').hex()
created = [l for l in receipt['logs'] if l['address'] == tx['to'] and l['topics'][0] == created_topic]
assert len(created) == 1
actual = '0x'+bytes.fromhex(created[0]['data'][2:])[3*32+12:4*32].hex()
assert predicted == actual
rpc_hash = replies('flap_v6_corrected_create2_rpc/001.response.json')[1]
rpc_pre = read('flap_v6_corrected_create2_rpc/001.request.json')[0]['params'][0]
assert rpc_pre == '0x'+preimage.hex() and rpc_hash == '0x'+k256(preimage).hex()
assert replies('flap_v6_hash1_rpc2/001.response.json')[1] == '0x'+k256(init).hex()
old_request = read('flap_v6_hash2_request.json')[0]['params'][0]
old_pre = bytes.fromhex(old_request[2:])
assert len(old_pre) == 97 and old_pre == b'\xff'+bytes(12)+preimage[1:]
assert '0x'+k256(old_pre)[12:].hex() == '0xaf5b39fd975d2f1ea9d89ecd8d09676155cb818a'

identity = replies('flap_quote_identity_rpc/001.response.json')
def string_result(value):
    b = bytes.fromhex(value[2:]); off = int.from_bytes(b[:32], 'big')
    size = int.from_bytes(b[off:off+32], 'big')
    assert off+32+size <= len(b)
    return b[off+32:off+32+size].decode()
quote_info = dict(address=quote, name=string_result(identity[1]),
                  symbol=string_result(identity[2]), decimals=int(identity[3], 16), identity_state='latest')
assert quote_info['symbol'] == 'DOGE' and quote_info['decimals'] == 8

blocks = replies('flap_buy_order_blocks_rpc/001.response.json')
receipts = replies('flap_buy_order_receipts_rpc/001.response.json')
buy_topic = '0x'+k256(b'TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)').hex()
buys = []; receipt_count = 0
for i in range(1, 5):
    block, rs = blocks[i], receipts[i]
    assert int(block['number'], 16) == 124415356+i
    if i > 1:
        assert block['parentHash'] == blocks[i-1]['hash']
    rs = sorted(rs, key=lambda r: int(r['transactionIndex'], 16))
    assert [r['transactionHash'] for r in rs] == block['transactions']
    assert [int(r['transactionIndex'], 16) for r in rs] == list(range(len(rs)))
    receipt_count += len(rs)
    for r in rs:
        assert r['blockHash'] == block['hash'] and r['blockNumber'] == block['number']
        for log in r['logs']:
            assert log['transactionHash'] == r['transactionHash'] and log['blockHash'] == block['hash']
            assert not log.get('removed', False)
            if log['address'] != tx['to'] or not log['topics'] or log['topics'][0] != buy_topic:
                continue
            data = bytes.fromhex(log['data'][2:])
            assert len(data) == 7*32
            if '0x'+data[44:64].hex() != actual:
                continue
            assert r['status'] == '0x1'
            buys.append(dict(tx=r['transactionHash'], block=int(block['number'], 16),
                tx_index=int(r['transactionIndex'],16), log_index=int(log['logIndex'],16),
                buyer='0x'+data[76:96].hex(), amount=int.from_bytes(data[96:128],'big'),
                quote_amount_event_field_eth=int.from_bytes(data[128:160],'big'),
                fee_event=int.from_bytes(data[160:192],'big'),
                creator_buy=r['transactionHash'] == tx['hash']))
buys.sort(key=lambda b: (b['block'], b['tx_index'], b['log_index']))
external = [b for b in buys if not b['creator_buy']]
assert external, 'No external BUY found in the complete receipt range'
first_tx = replies('flap_first_external_buy_rpc/001.response.json')[1]
first_event = external[0]
assert first_tx['hash'] == first_event['tx']
assert int(first_tx['blockNumber'],16) == first_event['block']
assert int(first_tx['transactionIndex'],16) == first_event['tx_index']
assert first_tx['blockHash'] == blocks[2]['hash']
first_envelope = {k:first_tx[k] for k in ['hash','from','to','value']}
first_envelope.update(selector=first_tx['input'][:10],
                      direct_to_portal=first_tx['to'] == tx['to'],
                      value_wei=int(first_tx['value'],16),
                      internal_call_semantics='NOT_VERIFIED_FROM_TOP_LEVEL_CALLDATA')
result = dict(
    status='ADDRESS_REPRODUCED_FOR_FIXTURE_G02_STILL_IN_PROGRESS',
    eip1014_vectors_passed=len(vectors), init_code_bytes=len(init), preimage_bytes=len(preimage),
    portal=tx['to'], salt='0x'+salt.hex(), embedded_implementation=implementation,
    init_code_hash='0x'+k256(init).hex(), predicted=predicted, actual=actual, match=True,
    independent_rpc_hash_match=True,
    invalid_old_preimage_bytes=len(old_pre), error_cause='Deployer incorrectly padded from 20 to 32 bytes',
    quote=quote_info, launch_quote_raw=int.from_bytes(words[7],'big'),
    launch_value_wei=int(tx['value'],16), complete_blocks_checked=4, receipts_checked=receipt_count,
    ordered_buys=buys, first_external_buy=external[0], first_external_envelope=first_envelope,
    remaining=['Derive launch curve/config/extension state from pre-signal cache and calldata',
               'Reproduce first external router BUY with its actual internal inputs and fees/rounding; distinguish later direct Portal BUYs',
               'Prove config invalidation and pending visibility; no BSC latency or inclusion proof yet'],
    input_sha256=inputs)
(P/'flap_g02_verified_correction.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k != 'input_sha256'},indent=2))
