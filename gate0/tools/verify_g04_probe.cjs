// Offline source-capability audit. No coverage, T_send or acquisition PASS inferred.
const fs=require('fs'),path=require('path'),assert=require('assert/strict'),crypto=require('crypto');
const R=path.resolve(__dirname,'../..'),P=path.join(R,'gate0/evidence/g04_quicknode_probe');
const E=require(path.join(R,'tests/DARKBOT 2/gate0-evidence/abi-tools/node_modules/ethers'));
const bytes=fs.readFileSync(path.join(P,'capture.json')),manifest=JSON.parse(fs.readFileSync(path.join(P,'sha256.json')));
assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),manifest['capture.json']);
const d=JSON.parse(bytes),portal='0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const types=['string','string','string','uint8','bytes32','uint8','address','uint256','address','bytes','bytes32','bytes','uint8','uint8','uint16','uint16','uint64','uint64','uint16','uint16','uint16','uint16','uint256','address','address','uint8'];
const it=new E.Interface(['function newTokenV6(('+types.join(',')+')) payable returns(address)']);
let full=0,hashes=0,missing=0,portalCount=0,heads=0,logCount=0;const launches=[];
for(const p of d.packets){const m=p.message.params;if(!m)continue;const v=m.result;
 if(m.subscription===d.subscriptions[3])heads++;
 if(m.subscription===d.subscriptions[2])logCount++;
 if(m.subscription!==d.subscriptions[1])continue;
 if(typeof v==='string'){hashes++;continue;}full++;
 if(!['hash','from','to','input','value','nonce','gas'].every(k=>k in v))missing++;
 if(v.to?.toLowerCase()!==portal)continue;portalCount++;if(!v.input?.startsWith('0x8cb5772c'))continue;
 const lp=it.decodeFunctionData('newTokenV6',v.input)[0];assert.equal(it.encodeFunctionData('newTokenV6',[lp]),v.input);
 const logs=d.packets.filter(q=>q.message.params?.subscription===d.subscriptions[2]&&q.message.params.result.transactionHash===v.hash&&!q.message.params.result.removed);
 launches.push({tx_hash:v.hash,token_version:lp[25].toString(),quote_token:lp[6].toLowerCase(),quote_amount:lp[7].toString(),extension_id:lp[10],buy_tax:lp[14].toString(),sell_tax:lp[15].toString(),
 frozen_scope_match:lp[25]===6n&&lp[6].toLowerCase()==='0xba2ae424d960c26247dd6c32edc70b295c744c43'&&lp[10]==='0x3526f6a0e790306d64c896eeda5ec66052ea17889e20632ff0cf512923153dad',
 matching_portal_logs:logs.length,pending_to_first_log_delivery_ns:logs.length?(BigInt(logs[0].local_received_ns)-BigInt(p.local_received_ns)).toString():null,
 timing_scope:'Same observer monotonic clock; subscription delivery gap only. Not T0, pre-lead, T_send, or T_buy.'});
}
assert.deepEqual({pending_full:full,pending_hash_only:hashes,pending_missing_fields:missing,portal_pending:portalCount,selected_v6_pending:launches.length,canonical_portal_logs:logCount,heads},d.stats);
const result={status:'G0.4_PARTIAL_SOURCE_CAPABILITY_ONLY',packet_count:d.packets.length,duration_ns:d.duration_ns,stop_reason:d.stop_reason,stats:d.stats,launches,
 qualified_frozen_scope_launches:launches.filter(x=>x.frozen_scope_match).length,canonical_launch_denominator:null,coverage_rate:null,T_send:null,T_buy:null,
 limits:['One bounded capture; no independent canonical launch census','No own-geth/48Club/BlockRazor comparison','V6 selector alone is insufficient scope qualification','No full state/actionable or purchase-latency evidence']};
fs.writeFileSync(path.join(P,'analysis.json'),JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({status:result.status,packets:result.packet_count,full_pending:full,v6_launches:launches.length,frozen_scope_launches:result.qualified_frozen_scope_launches,coverage_rate:null}));
