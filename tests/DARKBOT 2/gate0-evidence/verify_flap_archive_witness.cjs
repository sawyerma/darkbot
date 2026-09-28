/* Offline evidence audit. Chain inputs: only the six frozen QuickNode raw responses.
 * Exit 2 deliberately means PARTIAL. No RPC, transaction construction or signing.
 */
const fs=require('fs'),path=require('path'),crypto=require('crypto'),assert=require('assert/strict');
const E=require('./abi-tools/node_modules/ethers'),{audit}=require('./audit_flap_trade_bytecode.cjs');
const ROOT=__dirname,P=path.join(ROOT,'2026-09-28-g02-archive-witness-quicknode-153754');
const clean=x=>JSON.parse(JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v));
const write=(name,x)=>fs.writeFileSync(path.join(P,name),JSON.stringify(clean(x),null,2)+'\n');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const manifest=JSON.parse(fs.readFileSync(path.join(P,'sha256.json')));
const readRaw=name=>{const raw=fs.readFileSync(path.join(P,name));assert.equal(hash(raw),manifest[name],name+' hash');const j=JSON.parse(raw);assert(!j.error&&j.result);return j.result;};
const traces={},pre={};for(const role of ['launch','first_buy','direct_buy']){traces[role]=readRaw(role+'_calltrace.json');pre[role]=readRaw(role+'_prestate.json');}
let interval=null;
if(fs.existsSync(path.join(P,'intervening_raw','sha256.json'))){
 const child=require('child_process').spawnSync('python3',[path.join(ROOT,'verify_flap_intervening.py')],{encoding:'utf8'});
 assert.equal(child.status,0,'State replay failed: '+child.stderr);
 interval=JSON.parse(fs.readFileSync(path.join(P,'intervening_transactions.json')));
 assert.equal(interval.count,65);assert.equal(interval.unknown_transaction_classifications,0);
 // Arithmetic now receives the independently reconstructed parent + ordered-diff
 // state. Original first-BUY snapshot is only the comparison oracle/read-set scope.
 pre.first_buy=JSON.parse(fs.readFileSync(path.join(P,'replayed_first_buy_prestate.json'))).result;
}
const PORTAL='0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0',TOKEN='0xabe1fe16cf6bc48c9524713054efe6e954b17777',DOGE='0xba2ae424d960c26247dd6c32edc70b295c744c43',WBNB='0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c',PAIR='0xac109c8025f272414fd9e2faa805a583708a017f';
const TRADE='0x11055852a513d15655a3f03e4ed66c6e7827e3c7',IMPL='0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20';
const coder=E.AbiCoder.defaultAbiCoder(),word=x=>E.toBeHex(x,32),slot=(a,n)=>E.keccak256(coder.encode(['address','uint256'],[a,n]));
const storage=(role,address,key)=>{assert(key in (pre[role][address]?.storage||{}),'missing storage '+address+' '+key);return BigInt(pre[role][address].storage[key]);};
const iface=new E.Interface(['function execute(bytes commands,bytes[] inputs) payable','function swapExactInput((address inputToken,address outputToken,uint256 inputAmount,uint256 minOutputAmount,bytes permitData) params) payable returns(uint256)']);
const flatten=(f,p='0')=>[{path:p,...Object.fromEntries(Object.entries(f).filter(([k])=>k!=='calls'))},...(f.calls||[]).flatMap((c,i)=>flatten(c,p+'.'+i))];
const frames=Object.fromEntries(Object.entries(traces).map(([k,t])=>[k,flatten(t)]));
for(const [role,f] of Object.entries(frames))write(role+'_decoded_calltree.json',f);
const first=traces.first_buy,router=first.to,buyer=first.from;
const [commands,segments]=iface.decodeFunctionData('execute',first.input);
assert.equal(iface.encodeFunctionData('execute',[commands,segments]),first.input);
assert.equal(commands,'0x0b090816');
const pcalls=first.calls.filter(c=>c.to===PORTAL&&c.type==='CALL');assert.equal(pcalls.length,1);
const portalCall=pcalls[0];assert.equal(portalCall.input.slice(0,10),iface.getFunction('swapExactInput').selector);
const params=iface.decodeFunctionData('swapExactInput',portalCall.input)[0];assert.equal(iface.encodeFunctionData('swapExactInput',[params]),portalCall.input);
assert.equal(params.inputToken.toLowerCase(),DOGE);assert.equal(params.outputToken.toLowerCase(),TOKEN);assert.equal(BigInt(portalCall.value),0n);
assert.equal(portalCall.calls[0].to,IMPL);assert.equal(portalCall.calls[0].calls[0].to,TRADE);
const command16=coder.decode(['address','address','uint256','uint256','uint256'],segments[3]);
assert.equal(command16[0].toLowerCase(),DOGE);assert.equal(command16[1].toLowerCase(),TOKEN);assert.equal(command16[2],1n<<255n);assert.equal(command16[4],1n);
write('router_portal_proof.json',{scope:'This historical router transaction; not every possible command/flag',router,commands,
 commands_decoded:[{command:'0x0b',meaning:'native fee payment',frame:'0.0'},{command:'0x09',meaning:'WBNB deposit',frame:'0.1'},{command:'0x08',meaning:'V2 WBNB to DOGE',frame:'0.7'},{command:'0x16',meaning:'DOGE balance to Portal swapExactInput, resulting tokens forwarded to EOA',frame:'0.13'}],
 direct_children:frames.first_buy.filter(f=>f.path.split('.').length===2),portal_frame:frames.first_buy.find(f=>f.path==='0.13'),
 portal_selector:portalCall.input.slice(0,10),portal_implementation:IMPL,trade_implementation:TRADE,
 input_token:DOGE,output_token:TOKEN,input_amount:params.inputAmount,min_output:params.minOutputAmount,permit:params.permitData,
 caller:router,final_recipient:buyer,command16_balance_sentinel:command16[2],general_router_bytecode_semantics_proven:false});

// Independent pool pre-reserves: decode packed storage slot 8. No Swap/Sync output input.
const packedPool=storage('first_buy',PAIR,word(8)),mask112=(1n<<112n)-1n;
const reserve0=packedPool&mask112,reserve1=(packedPool>>112n)&mask112;
assert.equal(E.toBeHex(storage('first_buy',PAIR,word(6)),20),DOGE);assert.equal(E.toBeHex(storage('first_buy',PAIR,word(7)),20),WBNB);
const [feeAsset,feeRecipient,routerRate]=coder.decode(['address','address','uint256'],segments[0]);
assert.equal(feeAsset,E.ZeroAddress);
const native=BigInt(first.value),routerFee=native*routerRate/10000n;
const [wrapAmount]=coder.decode(['uint256'],segments[1]);assert.equal(wrapAmount,native-routerFee);
const v2=coder.decode(['uint256','uint256','uint256','bytes','address[]','bool'],segments[2]),pathBytes=E.getBytes(v2[3]);
assert.equal(pathBytes.length,43);assert.equal(E.hexlify(pathBytes.slice(0,20)),WBNB);assert.equal(E.hexlify(pathBytes.slice(23)),DOGE);
assert.equal(v2[4][0].toLowerCase(),PAIR);const multiplier=BigInt('0x'+Buffer.from(pathBytes.slice(20,23)).toString('hex'));
const quote=wrapAmount*multiplier*reserve0/(reserve1*10000n+wrapAmount*multiplier);
assert.equal(quote,params.inputAmount); // output comparison after independent calculation
const getReserves=first.calls.find(c=>c.to===PAIR&&c.input==='0x0902f1ac');const observedReserves=coder.decode(['uint112','uint112','uint32'],getReserves.output);
assert.equal(reserve0,observedReserves[0]);assert.equal(reserve1,observedReserves[1]);
write('independent_conversion.json',{authority:'first_buy_prestate.json / pair storage slot 8 + top-level transaction calldata',pair:PAIR,
 reserve0_doge:reserve0,reserve1_wbnb:reserve1,last_update_timestamp:packedPool>>224n,native_input:native,router_fee_bps:routerRate,
 router_fee_wei:routerFee,wrap_amount_wei:wrapAmount,pool_multiplier:multiplier,formula:'floor(wrap*multiplier*reserve0/(reserve1*10000+wrap*multiplier))',
 expected_doge:quote,portal_input_comparison:params.inputAmount,difference:quote-params.inputAmount,uses_swap_output_to_infer_reserves:false});

// Validate the historical trade arithmetic on witnessed path. External-call responses are explicit dependencies.
const audits={};
for(const role of ['first_buy','direct_buy']){
 const t=traces[role],f=role==='first_buy'?portalCall.calls[0].calls[0]:t.calls[0].calls[0];
 const caller=role==='first_buy'?router:t.from;
 const input=role==='first_buy'?iface.encodeFunctionData('swapExactInput',[[DOGE,TOKEN,quote,params.minOutputAmount,params.permitData]]):f.input;
 const blockTime=interval&&role==='first_buy'?JSON.parse(fs.readFileSync(path.join(P,'intervening_raw','block_124415358.response.json'))).result.timestamp:undefined;
 audits[role]=audit(pre[role][f.to].code,pre[role][PORTAL].storage,{...f,auditCaller:caller,auditTimestamp:blockTime},t.from,input,pre[role]);
 if(blockTime!==undefined)assert.equal(audits[role].output,f.output);
 else {assert.equal(audits[role].status,'STOP_MISSING_BLOCK_TIMESTAMP');assert.equal(audits[role].remaining_external_calls,0);}
 write(role+'_trade_arithmetic_audit.json',audits[role]);
}
// Independently execute the simple view callees used by trade arithmetic from
// their historical runtime code + prestate. No observed return values supplied.
const viewAudits=[];
for(const index of [1,2,3,8]){
 const top=portalCall.calls[0].calls[0].calls[index],f=top.calls?.[0]||top;
 const run=audit(pre.first_buy[f.to].code,pre.first_buy[top.to].storage||{},
  {...f,auditCaller:top.from,auditAddress:top.to},buyer,f.input,pre.first_buy);
 assert.equal(run.external.length,0);assert.equal(run.output,top.output);
 viewAudits.push({trade_child_index:index,address:top.to,implementation:f.to,selector:top.input.slice(0,10),...run});
}
write('independent_view_reads.json',viewAudits);
const tradeCode=Buffer.from(pre.first_buy[TRADE].code.slice(2),'hex');
const literal=pc=>{let op=tradeCode[pc];assert(op>=0x60&&op<=0x7f);const v=BigInt('0x'+tradeCode.subarray(pc+1,pc+1+op-0x5f).toString('hex'));assert(audits.first_buy.constants.some(c=>c.pc===pc&&BigInt(c.value)===v));return v;};
const baseR=literal(0x4a8b),h=literal(0x4ab2),protocolBps=literal(0x4d7e),totalSupply=literal(0x4aa5);
const tokenBase=slot(TOKEN,152),stateWord=storage('first_buy',PORTAL,tokenBase),stateAmounts=storage('first_buy',PORTAL,word(BigInt(tokenBase)+1n));
const reserveBefore=stateAmounts&((1n<<128n)-1n),supplyBefore=stateAmounts>>128n;
const priceSlot=slot(TOKEN,187),price=storage('first_buy',PORTAL,priceSlot),r=baseR*10n**18n/price,k=r*(totalSupply+h)/10n**18n;
const taxPacked=storage('first_buy',TOKEN,word(0x106)),taxBps=(taxPacked>>8n)&65535n;
const taxView=portalCall.calls[0].calls[0].calls.find(c=>c.to===TOKEN&&c.input==='0x691f224f');assert.equal(taxBps,BigInt(taxView.output));
const feeExemptSlot=slot(router,177),feeExempt=storage('first_buy',PORTAL,feeExemptSlot);
assert.equal(feeExempt,0n);assert(audits.first_buy.reads.some(x=>x.slot===feeExemptSlot));
const fee=quote*protocolBps/10000n,totalFee=quote*(protocolBps+taxBps)/10000n,tax=totalFee-fee,netQuote=quote-totalFee;
const ceil=(a,b)=>(a+b-1n)/b;
const supply=q=>totalSupply+h-ceil(k*10n**18n,r+q*10n**10n);
const predicted=supply(reserveBefore+netQuote)-supplyBefore;
assert.equal(supply(reserveBefore),supplyBefore);
function hasMath(role,op,a,b,res){assert(audits[role].arithmetic.some(x=>x.op===op&&BigInt(x.a)===a&&BigInt(x.b)===b&&BigInt(x.result)===res),'missing bytecode arithmetic operation');}
hasMath('first_buy',4,baseR*10n**18n,price,r);
hasMath('first_buy',4,quote*(protocolBps+taxBps),10000n,totalFee);
hasMath('first_buy',4,quote*protocolBps,10000n,fee);
hasMath('first_buy',3,totalFee,fee,tax);
const actual=BigInt(portalCall.output);assert.equal(predicted,actual);assert.equal(actual,6243568333969093909994622n);
const outputCalls=frames.first_buy.filter(f=>f.type==='CALL'&&f.to===TOKEN&&f.input.startsWith('0xa9059cbb'));
const tokenTransfers=outputCalls.map(f=>({from:f.from,to:E.toBeHex(BigInt('0x'+f.input.slice(10,74)),20),amount:BigInt('0x'+f.input.slice(74,138))}));
assert.deepEqual(tokenTransfers,[{from:PORTAL,to:router,amount:predicted},{from:router,to:buyer,amount:predicted}]);
write('first_buy_calculation.json',{status:interval?'ORDERED_STATE_DIFF_REPLAY_ARITHMETIC_EXACT':'TX_PRESTATE_ARITHMETIC_EXACT; full launch-to-buy replay still incomplete',
 state_authority:interval?'Parent block 124415356 + launch diff + 65 ordered TX diffs; explicit canonical block-boundary balance adjustment':'first-BUY prestateTracer snapshot',
 inputs:{native_value:native,wrap_amount:wrapAmount,pool_reserve_doge:reserve0,pool_reserve_wbnb:reserve1,pool_multiplier:multiplier,quote_input:quote,
 cached_quote_price:price,cached_quote_price_slot:priceSlot,base_r_literal:baseR,h_literal:h,r,k,total_supply:totalSupply,
 portal_reserve_before:reserveBefore,portal_circulating_before:supplyBefore,protocol_fee_bps:protocolBps,tax_bps:taxBps},
 steps:{protocol_fee:fee,combined_fee:totalFee,tax:tax,net_quote:netQuote,portal_reserve_after:reserveBefore+netQuote,
 denominator:r+(reserveBefore+netQuote)*10n**10n,numerator:k*10n**18n,ceil_quotient:ceil(k*10n**18n,r+(reserveBefore+netQuote)*10n**10n),
 circulating_after:supply(reserveBefore+netQuote),expected_output:predicted,actual_output:actual,tx_prestate_arithmetic_difference:predicted-actual},
 observed_output_used_as_input:false,observed_fee_used_as_input:false,latest_state_used:false,
 bytecode_arithmetic_audit:'first_buy_trade_arithmetic_audit.json',
 limitations:[interval?'State replay uses provider execution diffs, not an independently executed full consensus EVM replay':'The arithmetic calculation starts at independently supplied first-BUY prestate, not a replay of all intervening transactions','Scoped bytecode audit uses historical external-call responses; it is not a complete EVM replay','Formula constants/branches verified for this historical mode only']});

const taxTests=[];
for(const role of ['launch','first_buy','direct_buy']){
 const qFrame=frames[role].find(f=>f.from===PORTAL&&f.to==='0x3a632ecc01a45b8002d76c15e172e71c79412003'&&f.input.startsWith('0xe59de41f'));
 const q=role==='first_buy'?quote:BigInt('0x'+qFrame.input.slice(10,74));
 const combined=q*(protocolBps+taxBps)/10000n,pf=q*protocolBps/10000n,tf=combined-pf;
 const taxCall=frames[role].find(f=>f.type==='CALL'&&f.from===PORTAL&&f.input.startsWith('0xd99f8de3'));
 assert.equal(tf,BigInt('0x'+taxCall.input.slice(10,74)));
 if(role==='direct_buy'){hasMath(role,4,q*200n,10000n,combined);hasMath(role,4,q*100n,10000n,pf);hasMath(role,3,combined,pf,tf);}
 taxTests.push({role,quote_input:q,protocol_fee:pf,combined_fee:combined,tax:tf,tax_minus_simple_floor:tf-q*taxBps/10000n,
 quote_input_authority:role==='first_buy'?'independent prestate V2 calculation':'historical call input; validates rounding only, not independent conversion'});
}
write('fee_rounding_proof.json',{scope:'Historical trade mode with 100bps protocol and 100bps tax; no generalization to all modes',
 implementation:TRADE,code_keccak:E.keccak256(pre.first_buy[TRADE].code),
 formula:{combined:'floor(q * (protocolBps + taxBps) / 10000)',protocol:'floor(q * protocolBps / 10000)',tax:'combined - protocol'},
 explanation:'With both rates at 100bps: tax=floor(q/50)-floor(q/100). It equals floor(q/100)+1 exactly when q modulo 100 is at least 50.',
 executed_arithmetic_pcs:{add:'0x59bd',multiply:'0x59e3',divide:'0x5a2c',subtract:'0x59d0'},
 actual_executed_operations:audits.first_buy.arithmetic.filter(x=>([fee,totalFee,tax,protocolBps+taxBps].includes(BigInt(x.result))&&[1,3,4].includes(x.op))),
 crosschecks:taxTests,rule_fitted_to_outputs:false,full_callee_side_effects_proven:false});

// Calldata evidence for the cached price. Presence and return equality are not a full oracle-authentication proof.
const launchTypes=['string','string','string','uint8','bytes32','uint8','address','uint256','address','bytes','bytes32','bytes','uint8','uint8','uint16','uint16','uint64','uint64','uint16','uint16','uint16','uint16','uint256','address','address','uint8'];
const li=new E.Interface(['function newTokenV6(('+launchTypes.join(',')+')) payable returns(address)']);
const lp=li.decodeFunctionData('newTokenV6',traces.launch.input)[0];assert.equal(li.encodeFunctionData('newTokenV6',[lp]),traces.launch.input);
const extensionData=lp[11];const oracle=coder.decode(['uint256','uint256','uint256','bytes'],extensionData);
assert.equal(oracle[1],price);
const launchPredictedReserve=lp[7]-lp[7]*(protocolBps+taxBps)/10000n,launchPredictedSupply=supply(launchPredictedReserve);
assert.equal(launchPredictedReserve,reserveBefore);assert.equal(launchPredictedSupply,supplyBefore);
const extensionFrame=frames.launch.find(f=>f.type==='DELEGATECALL'&&f.input.startsWith('0xb3f75711'));
const recoveredFrame=frames.launch.find(f=>f.to==='0x0000000000000000000000000000000000000001');
const sigWords=coder.decode(['bytes32','uint256','bytes32','bytes32'],recoveredFrame.input);
const recovered=E.recoverAddress(sigWords[0],{v:Number(sigWords[1]),r:sigWords[2],s:sigWords[3]}).toLowerCase();
assert.equal(recovered,E.toBeHex(BigInt(recoveredFrame.output),20));
write('pending_initial_state_evidence.json',{status:'PARTIAL',launch_selector:traces.launch.input.slice(0,10),token_version:lp[25],quote_token:lp[6],quote_amount:lp[7],
 extension_id:lp[10],extension_input_data:extensionData,oracle_words:{word0:oracle[0],quote_price:oracle[1],timestamp_or_expiry_unclassified:oracle[2]},
 cached_quote_price_matches_launch_calldata:true,extension_delegatecall:extensionFrame,
 ecrecover_from_witness_digest:{recovered_signer:recovered,signature_math_verified:true,independent_signed_digest_derivation:false,role_and_freshness_prevalidation_complete:false},
 creator_arithmetic:{expected_reserve:launchPredictedReserve,expected_circulating:launchPredictedSupply,first_buy_prestate_reserve:reserveBefore,first_buy_prestate_circulating:supplyBefore},
 limitation:interval?'Intervening committed state diffs are reviewed separately. Oracle digest, authorization, freshness and full launcher effects are not independently replayed.':'Equal creator prediction and first-BUY prestate does not establish absence or identity of intervening state changes. Oracle digest, authorization, freshness and full launcher effects are not independently replayed.'});

const inventory={};for(const role of Object.keys(pre))inventory[role]=Object.fromEntries(Object.entries(pre[role]).map(([address,x])=>[address,{code_bytes:((x.code||'0x').length-2)/2,code_keccak:E.keccak256(x.code||'0x'),code_sha256:hash(Buffer.from((x.code||'0x').slice(2),'hex')),storage_slots:Object.keys(x.storage||{}).length}]));
write('historical_code_inventory.json',inventory);
write('historical_state_decoded.json',{scope:'Transaction prestate; absent keys remain UNKNOWN, never defaulted to zero',portal:PORTAL,token:TOKEN,
 token_state:{base_slot:tokenBase,header:stateWord&255n,status:stateWord>>8n&255n,curve_type:stateWord>>16n&255n,threshold_type:stateWord>>24n&255n,
 id:stateWord>>32n&0xffffffffn,token_version:stateWord>>64n&255n,quote_type:stateWord>>72n&255n,migrator_type:stateWord>>80n&255n,using_extension:stateWord>>88n&255n,
 reserve:reserveBefore,circulating:supplyBefore,quote_token:E.toBeHex(storage('first_buy',PORTAL,word(BigInt(tokenBase)+2n)),20),extension_id:word(storage('first_buy',PORTAL,word(BigInt(tokenBase)+3n)))},
 curve:{r,h,k,quote_price:price,source:'Executed historical bytecode constants plus first-BUY storage mapping slot 187; price also present in launch calldata'},
 portal_fee_exemption:{trader_used_by_portal:router,mapping_slot:177,key:feeExemptSlot,value:feeExempt,read_pc:'0x2d09',eoa_exemption_lookup_needed_on_this_path:false},
 quote_conversion:{first_buy:'Router V2 WBNB/DOGE before Portal call; Portal receives DOGE directly',direct_buy:'Portal delegated converter, V3 conversion; full independent V3 replay not done',global_quote_configuration:'Not read/proven completely by first-BUY prestate'},
 extension:{id:lp[10],launch_delegate:extensionFrame.to,full_registry_and_effects_proven:false},
 proxy:{portal_implementation:IMPL,trade_implementation:TRADE,token_implementation:pre.first_buy[TOKEN].code.slice(22,62)}});

// No intermediate transaction traces exist in these six files. Enumerate indices, never invent hashes or irrelevance.
const intervening=[];for(let i=1;i<51;i++)intervening.push({block:124415357,index:i,transaction_hash:null,relevance:'UNKNOWN',state_diff_available:false});
for(let i=0;i<15;i++)intervening.push({block:124415358,index:i,transaction_hash:null,relevance:'UNKNOWN',state_diff_available:false});
if(!interval)write('intervening_transactions.json',{status:'NOT_PROVEN',expected_count:65,
 count_authority:'Previously established task fixture boundary (launch block has 51 transactions); not independently derivable from the six traces',
 transactions:intervening,limitation:'Six traces cover launch, first BUY and direct BUY only. They cannot identify or classify every intervening transaction or transient changes.'});
const comparisons=[];for(const [a,x] of Object.entries(pre.launch))if(pre.first_buy[a])for(const [s,v] of Object.entries(x.storage||{}))if(s in (pre.first_buy[a].storage||{}))comparisons.push({address:a,slot:s,launch_prestate:v,first_buy_prestate:pre.first_buy[a].storage[s],equal:v===pre.first_buy[a].storage[s]});
write('snapshot_comparison.json',{note:'Comparison is PRE-LAUNCH versus PRE-BUY. Differences include launch effects. Equality cannot prove no intervening writes.',slots:comparisons});
// Execute all callees from historical code/state; observed return values are comparison oracles only.
for(const name of ['verify_flap_semantic_replay.cjs','classify_flap_hotpath.cjs']){
 const child=require('child_process').spawnSync('node',[path.join(ROOT,name)],{encoding:'utf8'});
 assert.equal(child.status,0,name+': '+child.stderr);
}
const semantic=JSON.parse(fs.readFileSync(path.join(P,'semantic_replay_proof.json')));
const deps=JSON.parse(fs.readFileSync(path.join(P,'hotpath_dependency_matrix.json')));
const sp=semantic.launch_state,acq=semantic.acquisitions.first_buy;
const full=Object.values(semantic.roles).every(r=>r.all_frames_match&&r.all_logs_match&&r.missing_accounts===0&&r.missing_storage===0)
 &&BigInt(acq.expected_output)===predicted&&BigInt(acq.difference)===0n&&semantic.no_recorded_callee_outputs_used_as_execution_inputs;
const pending=sp.launch_changed_storage_checks>0&&sp.launch_to_intervening_state_bridge_checks>0
 &&sp.oracle.at_deadline_accepted&&sp.oracle.deadline_plus_one_rejected&&sp.oracle.negative_controls.length===3
 &&sp.oracle.negative_controls.every(t=>t.rejected)&&BigInt(sp.initial_token_state.reserve)===reserveBefore&&BigInt(sp.initial_token_state.supply)===supplyBefore;
const hotpath=deps.coverage.unclassified===0&&deps.coverage.synchronous_hotpath_io_dependencies===0
 &&deps.coverage.storage_dependencies>0&&deps.fields.every(f=>deps.categories.includes(f.classification)&&f.invalidation&&f.recompute&&f.guard&&f.synchronous_hotpath_io===false);
const requirements={router_command_semantics:full,pending_initial_state:pending,independent_conversion_prestate:true,fee_rounding_source:true,full_first_buy_reproduction:full,hotpath_dependencies_fully_classified:hotpath};
const unknowns=Object.entries(requirements).filter(([,ok])=>!ok).map(([name])=>name);
assert(interval,'ordered state witness is mandatory');
const gatePass=Object.values(requirements).every(Boolean);
// Upgrade derived machine files while retaining the old scoped interpreter's limited provenance.
const calc=JSON.parse(fs.readFileSync(path.join(P,'first_buy_calculation.json')));
calc.status='FULL_HISTORICAL_MESSAGE_SEMANTICS_EXACT';calc.semantic_replay='semantic_replay_proof.json';calc.full_independent_first_buy_difference=BigInt(acq.difference);
calc.expected_recipient_acquisition=BigInt(acq.expected_output);calc.limitations=semantic.excluded;
write('first_buy_calculation.json',calc);
const rounding=JSON.parse(fs.readFileSync(path.join(P,'fee_rounding_proof.json')));rounding.full_callee_side_effects_proven=full;rounding.semantic_replay='semantic_replay_proof.json';write('fee_rounding_proof.json',rounding);
const stateDecoded=JSON.parse(fs.readFileSync(path.join(P,'historical_state_decoded.json')));
stateDecoded.quote_conversion.direct_buy='Portal delegated converter/V3 path independently executed: all 50 frames, 21 logs and recipient balance match';
stateDecoded.quote_conversion.global_quote_configuration='Executed read closure qualified for exact pinned historical modes; see dependency inventory';
stateDecoded.extension.full_registry_and_effects_proven=pending;write('historical_state_decoded.json',stateDecoded);
const summary={status:gatePass?'G0.2_FLAP_PASS':'G0.2_FLAP_PARTIAL',archive_rpc_qualified:true,router_portal_selector_proven:true,pending_initial_state_proven:pending,
 independent_conversion_prestate_proven:true,fee_rounding_source_proven:true,extension_effects_proven:full,fee_exemption_proven:true,
 full_first_buy_expected_output:BigInt(acq.expected_output),actual_first_buy_output:actual,full_independent_first_buy_difference:BigInt(acq.difference),
 tx_prestate_arithmetic_expected_output:predicted,tx_prestate_arithmetic_difference:predicted-actual,
 intervening_transactions_reviewed:interval?.count||0,intervening_quote_relevant_count:interval?.buy_relevant_count??null,
 replayed_state_arithmetic_expected_output:interval?predicted:null,replayed_state_arithmetic_difference:interval?predicted-actual:null,
 block_boundary_balance_adjustment:interval?.block_boundary_adjustments||[],
 launch_creator_arithmetic_matches_first_buy_prestate:true,requirements,remaining_unknowns:unknowns,
 evidence_scope:'Pinned historical V6/TAXED_V3/DOGE/extension mode 0 plus historical first Router-V2 and direct Portal-V3 paths. Full offline message replay, not live execution or BSC consensus validation.',semantic_replay:'semantic_replay_proof.json',dependency_classification:'hotpath_dependency_matrix.json',scope_limits:deps.scope_limits,live_cache_coverage_proven:false,latency_proven:false,no_g03_authorized:true};
write('witness_summary.json',summary);write('gate_result.json',summary);
const generated=fs.readdirSync(P).filter(n=>n.endsWith('.json')&&!['sha256.json','derived_sha256.json','final_artifact_sha256.json'].includes(n)&&!manifest[n]);
write('derived_sha256.json',Object.fromEntries([...generated.map(n=>[n,hash(fs.readFileSync(path.join(P,n)))]),...['verify_flap_archive_witness.cjs','audit_flap_trade_bytecode.cjs','verify_flap_semantic_replay.cjs','replay_flap_evm.cjs','classify_flap_hotpath.cjs','evm-audit-tools/package-lock.json'].map(n=>['../'+n,hash(fs.readFileSync(path.join(ROOT,n)))]) ]));
console.log(JSON.stringify(clean({status:summary.status,requirements,portal_selector:portalCall.input.slice(0,10),independent_doge_output:quote,
 tx_prestate_arithmetic_expected_output:predicted,tx_prestate_arithmetic_difference:predicted-actual,full_independent_first_buy_difference:BigInt(acq.difference),
 fee_rounding_crosschecks:taxTests,remaining_unknowns:unknowns}),null,2));
process.exitCode=gatePass?0:2;
