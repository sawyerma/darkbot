// G0.3 offline semantic/transaction/quote binding; never sends a transaction.
const fs=require('fs'),path=require('path'),assert=require('assert/strict'),crypto=require('crypto'),cp=require('child_process');
const R=path.resolve(__dirname,'../..'),B=path.join(R,'tests/DARKBOT 2/gate0-evidence'),P=path.join(B,'2026-09-28-g02-archive-witness-quicknode-153754'),O=path.join(R,'gate0/evidence');
const E=require(path.join(B,'abi-tools/node_modules/ethers')),{run}=require(path.join(B,'replay_flap_evm.cjs'));
const read=p=>JSON.parse(fs.readFileSync(p)),hash=b=>crypto.createHash('sha256').update(b).digest('hex'),clean=x=>JSON.parse(JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v));
function command(args){const r=cp.spawnSync(args[0],args.slice(1),{cwd:R,encoding:'utf8'});assert.equal(r.status,0,r.stderr);return r.stdout;}
function save(n,x){fs.writeFileSync(path.join(O,n),JSON.stringify(clean(x),null,2)+'\n');}
const TOKEN='0xabe1fe16cf6bc48c9524713054efe6e954b17777',PORTAL='0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0',LM='0xbc9e1a3695bc22e624d82a042281bcf33500c8b7',MC='0x556b9306565093c855aea9ae92a594704c2cd59e',POOL='0xce6160bb594fc055c943f59de92cee30b8c6b32c';
const word=n=>E.toBeHex(n,32),int=x=>BigInt(x==='0x'||x===undefined?0:x),abi=E.AbiCoder.defaultAbiCoder();
async function main(){
 assert.equal(hash(fs.readFileSync(path.join(O,'g03_direct_parent_state_full.json'))),'3a090b8398c581ee0b3a06c1a1a7cfe288dcdd231a106b6a60ab22511ff532b7');
 const raw=path.join(O,'g03_activation_raw');for(const [n,h] of Object.entries(read(path.join(raw,'sha256.json'))))assert.equal(hash(fs.readFileSync(path.join(raw,n))),h);
 const closure=read(path.join(O,'g03_parent_closure.json'));
 for(const [a,x] of Object.entries(closure.state))for(const [field,v] of Object.entries(x)){
  if(field==='storage')for(const [s,y] of Object.entries(v)){const n='parent_'+a.slice(2)+'_slot_'+s.slice(2);assert.equal(read(path.join(raw,n+'.response.json')).result,y);assert.deepEqual(read(path.join(raw,n+'.request.json')).params,[a,s,{blockHash:closure.block_hash,requireCanonical:true}]);}
  else{const n='parent_'+a.slice(2)+'_'+field;assert.equal(read(path.join(raw,n+'.response.json')).result,v);assert.deepEqual(read(path.join(raw,n+'.request.json')).params,[a,{blockHash:closure.block_hash,requireCanonical:true}]);}
 }
 command(['node','gate0/tools/probe_sequence.cjs']);const seq=read(path.join(O,'g03_sequential_execution.json'));
 const before=seq.launch.final_state,historical=read(path.join(P,'direct_buy_calltrace.json')).result;
 const caller=historical.from;
 const valid=r=>{assert.equal(r.exception,null);assert.deepEqual(r.missing_accounts,[]);assert.deepEqual(r.missing_storage,[]);};valid(seq.launch);valid(seq.buy);assert(seq.launch.all_frames_match);
 const getter=async(state,to,signature,args=[])=>{const it=new E.Interface(['function '+signature]);const f=it.fragments[0];const r=await run('direct_buy',{state,to,value:0,calldata:it.encodeFunctionData(f.name,args),blockNumber:124415357,noSave:true,quiet:true});valid(r);return int(r.return_value);};
 const oldTs=await getter(before,LM,'lastRewardTimestamp() view returns(uint32)');const newTs=await getter(seq.buy.final_state,LM,'lastRewardTimestamp() view returns(uint32)');
 assert.equal(oldTs,1790549253n);assert.equal(newTs,1790549254n);
 const lmLiquidity=await getter(before,LM,'lmLiquidity() view returns(uint128)');assert.equal(lmLiquidity,315711145240336558n);
 const oldBalance=await getter(before,TOKEN,'balanceOf(address) view returns(uint256)',[caller]),newBalance=await getter(seq.buy.final_state,TOKEN,'balanceOf(address) view returns(uint256)',[caller]);
 const acquired=newBalance-oldBalance;assert.equal(acquired,int(seq.buy.return_value));assert.equal(acquired,30669575743216741084405716n);
 const debit=int(before[caller].balance)-int(seq.buy.final_state[caller].balance);assert.equal(debit,150000000000000000n);
 const rust=JSON.parse(command(['cargo','run','--manifest-path','gate0/Cargo.toml','--locked','--offline','--quiet','--bin','g03','--','gate0/evidence/g03_rust_inputs.json']));
 const rustInput=read(path.join(O,'g03_rust_inputs.json'));
 assert.equal(rustInput.launch_calldata,read(path.join(P,'launch_calltrace.json')).result.input);
 assert.equal(rustInput.launch_calldata_keccak,E.keccak256(rustInput.launch_calldata));
 assert.equal(BigInt(rustInput.sqrt_price_x96),int(before[POOL].storage[word(0)])&((1n<<160n)-1n));
 assert.equal(BigInt(rustInput.liquidity),int(before[POOL].storage[word(5)]));
 assert.equal(BigInt(rust.expected_output),acquired);assert.equal(rust.token.toLowerCase(),TOKEN);assert.equal(rust.portal.toLowerCase(),PORTAL);
 const poolCall=seq.buy.comparisons.find(c=>c.computed.to===POOL&&c.computed.input.startsWith('0x128acb08')).computed;
 const poolOutputs=abi.decode(['int256','int256'],poolCall.output);assert.equal(-poolOutputs[0],BigInt(rust.quote_input));assert.equal(poolOutputs[1],debit);
 assert.equal(int(seq.buy.final_state[POOL].storage[word(0)])&((1n<<160n)-1n),BigInt(rust.sqrt_next));
 const tick=s=>((int(s[POOL].storage[word(0)])>>160n)&((1n<<24n)-1n));assert.equal(tick(before),tick(seq.buy.final_state),'single segment quote must not cross a tick');
 const iface=new E.Interface(['function swapExactInput((address inputToken,address outputToken,uint256 inputAmount,uint256 minOutputAmount,bytes permit) params) payable returns(uint256)']);
 const params=iface.decodeFunctionData('swapExactInput',rust.calldata)[0];assert.equal(iface.encodeFunctionData('swapExactInput',[params]),rust.calldata);
 assert.equal(params.minOutputAmount,acquired*9900n/10000n);assert.equal(BigInt(rust.min_output),params.minOutputAmount);
 const tests=[];
 for(const [name,min,success] of [['local_test_slippage_100bps',params.minOutputAmount,true],['exact_output_bound',acquired,true],['one_unit_above_output',acquired+1n,false]]){
  const calldata=iface.encodeFunctionData('swapExactInput',[[E.ZeroAddress,TOKEN,debit,min,'0x']]);
  if(name==='local_test_slippage_100bps')assert.equal(calldata,rust.calldata);
  const result=await run('direct_buy',{state:before,calldata,blockNumber:124415357,noSave:true,quiet:true});
  assert.deepEqual(result.missing_storage,[]);assert.deepEqual(result.missing_accounts,[]);
  if(success){valid(result);assert.equal(int(result.return_value),acquired);assert.equal(await getter(result.final_state,TOKEN,'balanceOf(address) view returns(uint256)',[caller])-oldBalance,acquired);}
  else{assert.equal(result.exception,'revert');assert.equal(await getter(result.final_state,TOKEN,'balanceOf(address) view returns(uint256)',[caller]),oldBalance);}
  tests.push({name,min_output:min,expected_success:success,exception:result.exception,return_value:result.return_value,frames:result.frame_count});
 }
 const noCode=structuredClone(before);delete noCode[MC];const negative=await run('direct_buy',{state:noCode,blockNumber:124415357,noSave:true,quiet:true});assert.equal(negative.exception,'UNWITNESSED_CODE '+MC);
 tests.push({name:'missing_contract_is_evidence_error_not_trade_revert',exception:negative.exception});
 const preLaunch=read(path.join(O,'g03_merged_parent_state.json')).state;
 for(const [a,x] of Object.entries(closure.state))if(!preLaunch[a])preLaunch[a]=x;else{Object.assign(preLaunch[a].storage,x.storage);}
 const early=await run('direct_buy',{state:preLaunch,blockNumber:124415357,noSave:true,quiet:true});assert.equal(early.exception,'revert');assert.deepEqual(early.missing_storage,[]);assert.deepEqual(early.missing_accounts,[]);
 tests.push({name:'before_launch_rejected',exception:early.exception,return_value:early.return_value});
 // Historical signed envelope is already independently reconstructed by the existing Rust main.
 const envelope=command(['cargo','run','--manifest-path','gate0/Cargo.toml','--locked','--offline','--quiet','--bin','darkbot-gate0']);assert(envelope.includes('G0.3 STEP4 PASS'));
 const rows=read(path.join(B,'2026-09-27-g01/flap_fixture_rpc/001.response.json'));const tx=rows.find(r=>r.id===5).result;
 assert.equal(int(tx.nonce),int(before[caller].nonce));assert.equal(tx.from,caller);assert.equal(tx.to,PORTAL);assert.equal(tx.input,historical.input);assert.equal(int(tx.value),debit);
 assert(int(before[caller].balance)>=int(tx.value)+int(tx.gas)*int(tx.gasPrice));
 const transitions=[];
 for(const label of ['358_016','359_017']){
  const d=read(path.join(raw,label+'_diff.response.json')).result;const call=read(path.join(raw,label+'_calls.response.json')).result;
  const flatten=t=>[t,...(t.calls||[]).flatMap(flatten)];const lm=flatten(call).find(t=>t.to===LM&&t.input.startsWith('0x214a6fe2'));assert(lm&&!lm.error);assert.equal(call.to,PORTAL);
  const old=d.pre[LM].storage[word(2)],next=d.post[LM].storage[word(2)];assert.equal(int(old)&((1n<<128n)-1n),lmLiquidity);assert.equal(int(next)&((1n<<128n)-1n),lmLiquidity);
  const req=read(path.join(raw,label+'_diff.request.json'));
  transitions.push({tx:req.params[0],block:label==='358_016'?124415358:124415359,index:label==='358_016'?16:17,old,new:next,old_timestamp:int(old)>>128n,new_timestamp:int(next)>>128n,semantics:'External direct BUY invokes V3 pool reward accumulation; not a token activation prerequisite'});
 }
 const result={status:'G0.3_PASS_PINNED_OFFLINE_SCOPE',scope:'Frozen newTokenV6/TAXED_V3/DOGE mode, historical direct caller and 0.15 BNB; exact single-segment parent V3 state',
  actual_parent_block_hash:closure.block_hash,no_later_state_patch:true,launch_frames:seq.launch.frame_count,buy_frames:seq.buy.frame_count,receipt_like_logs:seq.buy.logs.length,
  expected_token:TOKEN,recipient:caller,balance_before:oldBalance,balance_after:newBalance,acquired,expected_output_from_rust:rust.expected_output,difference:acquired-BigInt(rust.expected_output),native_debit_excluding_gas:debit,
  creator_acquisition_separate:true,activation:'After successful launch state transition; before-launch negative test reverts. No external reward update is required beforehand.',
  historical_signature_and_nonce_compatible:true,tx_hash:tx.hash,historical_minimum:'6000000000000000000000000',test_policy_minimum:rust.min_output,test_slippage_bps:100,production_slippage_policy_authorized:false,
  slot2_transitions:transitions,tests,quote:rust,scope_limits:['Offline message execution, not live acquisition or ordering','Historical signed transaction uses historical minimum; Rust policy calldata is separately simulated and not signed with the historical wallet','No generalized tick-crossing/native quote implementation or live cache/ingress coverage','No T_send or T_buy latency measured','Gas settlement excluded from message debit; envelope funding checked against gas-limit times gas price']};
 save('g03_result.json',result);save('g03_rust_output.json',rust);
 const names=fs.readdirSync(O).filter(n=>n.endsWith('.json')&&n!=='g03_sha256.json');const hashes=Object.fromEntries(names.map(n=>[n,hash(fs.readFileSync(path.join(O,n)))]));
 for(const name of ['gate0/src/main.rs','gate0/src/bin/g03.rs','gate0/Cargo.toml','gate0/Cargo.lock','gate0/tools/probe_sequence.cjs','gate0/tools/collect_g03_activation.py','gate0/tools/verify_g03.cjs','tests/DARKBOT 2/gate0-evidence/replay_flap_evm.cjs'])hashes['../../'+name]=hash(fs.readFileSync(path.join(R,name)));
 save('g03_sha256.json',hashes);
 console.log(JSON.stringify(clean({status:result.status,launch_frames:result.launch_frames,buy_frames:result.buy_frames,acquired,difference:result.difference,tests,slot2_transitions:transitions}),null,2));
}
main().catch(e=>{console.error(e);process.exitCode=1});
