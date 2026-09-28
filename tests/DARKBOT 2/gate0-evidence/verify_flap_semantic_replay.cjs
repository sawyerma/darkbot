// G0.2 offline verifier. Historical message execution only; no network/signing/TX sender.
const fs=require('fs'),path=require('path'),assert=require('assert/strict'),crypto=require('crypto');
const E=require('./abi-tools/node_modules/ethers'),{run}=require('./replay_flap_evm.cjs');
const P=path.join(__dirname,'2026-09-28-g02-archive-witness-quicknode-153754');
const read=n=>JSON.parse(fs.readFileSync(path.join(P,n))),clean=x=>JSON.parse(JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v));
const save=(n,x)=>fs.writeFileSync(path.join(P,n),JSON.stringify(clean(x),null,2)+'\n');
const PORTAL='0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0',TOKEN='0xabe1fe16cf6bc48c9524713054efe6e954b17777';
const DOGE='0xba2ae424d960c26247dd6c32edc70b295c744c43',EXT='0x4eb3e98d9b79755fe2b8aacb8528b63746e9867a';
const coder=E.AbiCoder.defaultAbiCoder(),word=n=>E.toBeHex(n,32),map=(a,n)=>E.keccak256(coder.encode(['address','uint256'],[a,n]));
const asBig=x=>BigInt(x==='0x'||x===undefined?0:x);
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function complete(x){assert.equal(x.exception,null);assert.equal(x.all_frames_match,true);assert.deepEqual(x.missing_storage,[]);assert.deepEqual(x.missing_accounts,[]);}
async function main(){
 const replays={};for(const role of ['launch','first_buy','direct_buy']){
  replays[role]=await run(role,{quiet:true,...(role==='first_buy'?{state:read('replayed_first_buy_prestate.json').result}:{})});complete(replays[role]);
 }
 const fixture=Object.fromEntries(JSON.parse(fs.readFileSync(path.join(__dirname,'2026-09-27-g01/flap_fixture_rpc/001.response.json'))).map(r=>[r.id,r.result]));
 const receipts={launch:fixture[2],first_buy:JSON.parse(fs.readFileSync(path.join(__dirname,'2026-09-28-g02-economics/router_receipt.json'))),direct_buy:fixture[6]};
 const results={};
 for(const [role,x] of Object.entries(replays)){
  assert.equal(receipts[role].status,'0x1');
  assert.deepEqual(x.logs,receipts[role].logs.map(l=>({address:l.address,topics:l.topics,data:l.data})),role+' complete receipt logs');
  results[role]={frames:x.frame_count,all_frames_match:true,logs:x.logs.length,all_logs_match:true,missing_accounts:0,missing_storage:0};
 }
 // Compare every changed launch storage/code/nonce from the separate tracer diff.
 // EOA gas debits and block fee account are transaction settlement, outside runCall.
 const diff=read('intervening_raw/diff_124415357_000.response.json').result;
 let slots=0,codes=0,nonces=0;const excluded=[];
 for(const [address,post] of Object.entries(diff.post)){
  const actual=replays.launch.final_state[address];
  for(const [key,v] of Object.entries(post.storage||{})){assert(actual,address);assert.equal(asBig(actual.storage[key]),asBig(v),address+' '+key);slots++;}
  if(post.code!==undefined){assert.equal(actual.code,post.code);codes++;}
  if(post.nonce!==undefined){assert.equal(asBig(actual.nonce),asBig(post.nonce));nonces++;}
  if(post.balance!==undefined)excluded.push({address,field:'balance',reason:'TX gas debit/fee settlement excluded from message execution'});
 }
 for(const [address,pre] of Object.entries(diff.pre))for(const key of Object.keys(pre.storage||{}))if(!(key in (diff.post[address]?.storage||{}))){
  assert.equal(asBig(replays.launch.final_state[address]?.storage[key]),0n,'deleted storage');slots++;
 }
 // Launch-produced state independently agrees with the post-launch read set used by the 65-diff replay.
 const interval=read('intervening_transactions.json');let launchBridge=0;
 for(const [address,z] of Object.entries(interval.post_launch_first_buy_read_set)){
  const computed=replays.launch.final_state[address];if(!computed)continue;
  for(const [key,v] of Object.entries(z.storage||{}))if(key in computed.storage){assert.equal(asBig(computed.storage[key]),asBig(v));launchBridge++;}
 }
 const base=map(TOKEN,152),priceKey=map(TOKEN,187),amountKey=word(BigInt(base)+1n);
 const pendingState={token:TOKEN,price:asBig(replays.launch.final_state[PORTAL].storage[priceKey]),reserve:asBig(replays.launch.final_state[PORTAL].storage[amountKey])&((1n<<128n)-1n),supply:asBig(replays.launch.final_state[PORTAL].storage[amountKey])>>128n};
 assert.equal(pendingState.reserve,78962581156n);assert.equal(pendingState.supply,21453245135239692933310825n);
 // Read actual balances by executing the historical token balanceOf bytecode locally.
 const tokenI=new E.Interface(['function balanceOf(address) view returns(uint256)']);
 const acquisitions={};
 for(const role of ['first_buy','direct_buy']){
  const root=read(role+'_calltrace.json').result,pre=role==='first_buy'?read('replayed_first_buy_prestate.json').result:read(role+'_prestate.json').result;
  const getter=async state=>{const r=await run(role,{state,to:TOKEN,value:0,calldata:tokenI.encodeFunctionData('balanceOf',[root.from]),noSave:true,quiet:true});assert.equal(r.exception,null);assert.deepEqual(r.missing_storage,[]);assert.deepEqual(r.missing_accounts,[]);return asBig(r.return_value);};
  const before=await getter(pre),after=await getter(replays[role].final_state),received=after-before;
  const expected=role==='first_buy'?6243568333969093909994622n:28438305464314215025968609n;
  assert.equal(received,expected);
  acquisitions[role]={recipient:root.from,balance_before:before,balance_after:after,expected_output:received,observed_output_comparison:expected,difference:received-expected,observed_output_used_as_input:false};
 }
 // Derive the signed oracle digest from the actually executed KECCAK operands, then reconstruct from launch fields/constants.
 const launchTypes=['string','string','string','uint8','bytes32','uint8','address','uint256','address','bytes','bytes32','bytes','uint8','uint8','uint16','uint16','uint64','uint64','uint16','uint16','uint16','uint16','uint256','address','address','uint8'];
 const li=new E.Interface(['function newTokenV6(('+launchTypes.join(',')+')) payable returns(address)']);
 const root=read('launch_calltrace.json').result,lp=li.decodeFunctionData('newTokenV6',root.input)[0];
 const oracle=coder.decode(['uint256','uint256','uint256','bytes'],lp[11]);assert.equal(oracle[0],0n);assert.equal(oracle[1],pendingState.price);
 const hashes=replays.launch.oracle_steps.filter(s=>s.op==='KECCAK256');assert.equal(hashes.length,3);
 const structWords=coder.decode(Array(7).fill('bytes32'),hashes[0].preimage);
 const domainWords=coder.decode(Array(5).fill('bytes32'),hashes[1].preimage);
 // Bind fixed digest words to runtime literals, not values learned from oracle responses.
 const extCode=read('launch_prestate.json').result[EXT].code;
 for(const literal of [structWords[0],domainWords[0],domainWords[1],domainWords[2]])assert(extCode.includes('7f'+literal.slice(2)),'oracle domain/type constant in runtime');
 const computedStruct=coder.encode(['bytes32','address','bytes32','uint256','uint256','uint256','uint256'],[structWords[0],lp[6],lp[4],lp[25],oracle[0],oracle[1],oracle[2]]);
 const computedDomain=coder.encode(['bytes32','bytes32','bytes32','uint256','address'],[domainWords[0],domainWords[1],domainWords[2],56,PORTAL]);
 assert.equal(computedStruct,hashes[0].preimage);assert.equal(computedDomain,hashes[1].preimage);
 const digest=E.keccak256(E.concat(['0x1901',E.keccak256(computedDomain),E.keccak256(computedStruct)]));assert.equal(digest,hashes[2].hash);
 const recovered=E.recoverAddress(digest,oracle[3]).toLowerCase();assert.equal(recovered,'0x53f3a6115f6a2cc0ca8d388d7c169ecc9f5058ac');
 const roleFrame=replays.launch.comparisons.find(c=>c.computed.input.startsWith('0x91d14854')).computed;
 const [role,signer]=coder.decode(['bytes32','address'],'0x'+roleFrame.input.slice(10));assert.equal(signer.toLowerCase(),recovered);
 const roleReads=replays.launch.reads.filter(r=>r.selector==='0x91d14854'&&r.code_address!==PORTAL);assert.equal(roleReads.length,1);assert.equal(asBig(roleReads[0].value),1n);
 const negative=[];
 const atDeadline=await run('launch',{timestamp:oracle[2],noSave:true,quiet:true});assert.equal(atDeadline.exception,null);assert.deepEqual(atDeadline.missing_storage,[]);assert.deepEqual(atDeadline.missing_accounts,[]);
 assert.equal(atDeadline.final_state[PORTAL].storage[amountKey],replays.launch.final_state[PORTAL].storage[amountKey]);
 const expired=await run('launch',{timestamp:oracle[2]+1n,noSave:true,quiet:true});assert.equal(expired.exception,'revert');assert.deepEqual(expired.missing_storage,[]);
 negative.push({case:'timestamp=deadline+1',exception:expired.exception,return_data:expired.return_value,rejected:true});
 const revoked=read('launch_prestate.json').result;revoked[PORTAL].storage[roleReads[0].slot]=word(0);
 const denied=await run('launch',{state:revoked,noSave:true,quiet:true});assert.equal(denied.exception,'revert');assert.deepEqual(denied.missing_storage,[]);
 negative.push({case:'oracle signer role revoked in pre-resident state',exception:denied.exception,return_data:denied.return_value,rejected:true});
 const missing=read('launch_prestate.json').result;delete missing[PORTAL].storage[roleReads[0].slot];
 const unavailable=await run('launch',{state:missing,noSave:true,quiet:true});assert(unavailable.exception?.startsWith('UNWITNESSED_STORAGE'));assert.equal(unavailable.missing_storage.length,1);
 negative.push({case:'required cached storage missing',exception:'UNWITNESSED_STORAGE',rejected:true});
 const oracleProof={source:'Executed extension runtime + launch calldata + signer-role storage',extension:EXT,type_hash:structWords[0],domain_constants:domainWords.slice(0,3),
  struct_fields:['typeHash','quoteToken','salt','tokenVersion','word0/mode','price','deadline'],struct_preimage:computedStruct,domain_preimage:computedDomain,digest,recovered_signer:recovered,required_role:role,role_slot:roleReads[0].slot,
  deadline:oracle[2],admissibility:'execution block.timestamp <= signed deadline; future inclusion timestamp is not known at pending ingress',at_deadline_accepted:true,deadline_plus_one_rejected:true,negative_controls:negative};
 const pendingProof={status:'HISTORICAL_MODE_SEMANTICS_PROVEN',scope:'newTokenV6 / TOKEN_TAXED_V3 / witnessed DOGE quote + extension mode 0 / exact pinned code; conditional on complete coherent pre-resident state and valid execution-time deadline',
  source:'Pending-equivalent launch calldata + PRE-launch code/storage + chain context. No launch output/log used as execution input.',
  independently_executed_launch_frames:116,all_48_launch_logs_match:true,launch_changed_storage_checks:slots,launch_created_code_checks:codes,launch_nonce_checks:nonces,launch_to_intervening_state_bridge_checks:launchBridge,
  initial_token_state:pendingState,oracle:oracleProof,excluded_transaction_settlement:excluded,
  pending_availability:'Dependency provenance/precomputability established; actual live cache/ingress coverage is not measured by historical replay.',
  fail_closed:'Missing/stale state, unsupported code/mode, insufficient expiry margin or unresolved ordering: SKIP. No synchronous state fetch.'};
 save('pending_initial_state_evidence.json',pendingProof);
 const semantic={status:'OFFLINE_MESSAGE_REPLAY_EXACT',roles:results,launch_state:pendingProof,acquisitions,
  backend:'@ethereumjs/evm 10.1.3; chainId 56 / Cancun',dependency_lock_sha256:sha(fs.readFileSync(path.join(__dirname,'evm-audit-tools/package-lock.json'))),
  source_url:'https://github.com/ethereumjs/ethereumjs-monorepo',license:'MPL-2.0 for evm/statemanager/util/block; MIT for common',dependency_provenance:Object.fromEntries(['evm','common','statemanager','util','block'].map(n=>{const p=JSON.parse(fs.readFileSync(path.join(__dirname,'evm-audit-tools/node_modules/@ethereumjs',n,'package.json')));return [p.name,{version:p.version,license:p.license,repository:p.repository,source_commit:p.gitHead||null,source_identity:'Exact npm release + resolved tarball integrity in package-lock.json; no invented Git commit'}];})),no_recorded_callee_outputs_used_as_execution_inputs:true,
  first_buy_prestate_authority:'Parent block state + launch diff + all 65 ordered TX diffs, independently checked by verify_flap_intervening.py; launch EVM effects cross-checked above.',
  excluded:['BSC block/consensus validation and transaction gas settlement','Proof of actual live ingress/cache coverage','Unseen modes, addresses, tax rules or code versions','Transaction construction/signing/sending and latency']};
 save('semantic_replay_proof.json',semantic);
 console.log(JSON.stringify(clean({status:semantic.status,roles:results,launch_storage_checks:slots,launch_bridge_checks:launchBridge,acquisitions,oracle_negative_controls:negative}),null,2));
 return semantic;
}
module.exports={main};if(require.main===module)main().catch(e=>{console.error(e);process.exitCode=1;});
