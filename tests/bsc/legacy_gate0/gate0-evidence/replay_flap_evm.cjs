// G0.2 evidence verifier only: offline message execution, never signs/submits a TX.
const fs=require('fs'),path=require('path'),assert=require('assert/strict'),crypto=require('crypto');
const lib=n=>require('./evm-audit-tools/node_modules/@ethereumjs/'+n);
const {createEVM}=lib('evm'),{createCustomCommon,Mainnet,Hardfork}=lib('common'),{SimpleStateManager}=lib('statemanager');
const {Account,createAddressFromString:A,hexToBytes:H,bytesToHex:X}=lib('util');
const E=require('./abi-tools/node_modules/ethers');
const P=path.join(__dirname,'2026-09-28-g02-archive-witness-quicknode-153754');
const read=p=>JSON.parse(fs.readFileSync(p)),clean=x=>JSON.parse(JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v));
const save=(name,x)=>fs.writeFileSync(path.join(P,name),JSON.stringify(clean(x),null,2)+'\n');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function verifyRaw(){for(const [name,digest] of Object.entries(read(path.join(P,'sha256.json'))))assert.equal(sha(fs.readFileSync(path.join(P,name))),digest);}
function flatten(f,out=[]){out.push(f);for(const c of f.calls||[])flatten(c,out);return out;}
async function run(role,options={}){
 verifyRaw();
 const trace=read(path.join(P,role+'_calltrace.json')).result;
 const inputState=options.state||read(path.join(P,role+'_prestate.json')).result;
 const blocks=read(path.join(__dirname,'2026-09-27-g01/flap_buy_order_blocks_rpc/001.response.json')).map(r=>r.result);
 const number=options.blockNumber??{launch:124415357,first_buy:124415358,direct_buy:124415360}[role],header=blocks.find(b=>Number(BigInt(b.number))===number);assert(header);
 const common=createCustomCommon({chainId:56},Mainnet,{hardfork:Hardfork.Cancun});
 const state=new SimpleStateManager({common});
 for(const [address,x] of Object.entries(inputState)){
  await state.putAccount(A(address),new Account(BigInt(x.nonce||0),BigInt(x.balance||0)));
  await state.putCode(A(address),H(x.code||'0x'));
  for(const [key,v] of Object.entries(x.storage||{}))await state.putStorage(A(address),H(key),H(v));
 }
 const unknownAccounts=new Set(),unknownStorage=[],created=new Set(),writes=[],reads=[],messages=[],active=[],ops=new Set(),oracleSteps=[];
 const rawGetAccount=state.getAccount.bind(state),rawGetStorage=state.getStorage.bind(state),rawPutStorage=state.putStorage.bind(state),rawGetCode=state.getCode.bind(state);
 const isPrecompile=s=>BigInt(s)>0n&&BigInt(s)<=10n;
 const codeReads=[];
 state.getCode=async a=>{const address=a.toString();codeReads.push(address);if(!inputState[address]&&!created.has(address)&&!isPrecompile(address))throw Error('UNWITNESSED_CODE '+address);return rawGetCode(a);};
 state.getAccount=async a=>{const s=a.toString();if(!inputState[s]&&!created.has(s)&&!isPrecompile(s))unknownAccounts.add(s);return rawGetAccount(a);};
 state.getStorage=async(a,k)=>{
  const address=a.toString(),key=X(k),map=state.topStorageStack();
  if(!map.has(address+'_'+key)&&!created.has(address)&&!(key in (inputState[address]?.storage||{}))){unknownStorage.push({address,key});throw Error('UNWITNESSED_STORAGE '+address+' '+key);}
  const value=await rawGetStorage(a,k);reads.push({address,slot:key,value:X(value),frame_index:messages.indexOf(active.at(-1)),code_address:active.at(-1)?.code_address,selector:active.at(-1)?.input.slice(0,10)});return value;
 };
 state.putStorage=async(a,k,v)=>{writes.push({address:a.toString(),slot:X(k),value:X(v)});return rawPutStorage(a,k,v);};
 const evm=await createEVM({common,stateManager:state,blockchain:{getBlock:async n=>{const b=blocks.find(b=>BigInt(b.number)===BigInt(n));if(!b)throw Error('UNWITNESSED_BLOCKHASH '+n);return {hash:()=>H(b.hash)};}}});
 evm.events.on('newContract',({address})=>created.add(address.toString()));
 evm.events.on('step',s=>{
  ops.add(s.opcode.name);
  if(s.codeAddress?.toString()==='0x4eb3e98d9b79755fe2b8aacb8528b63746e9867a'&&['KECCAK256','TIMESTAMP','LT','GT','EQ','SLOAD','CHAINID'].includes(s.opcode.name)){
   const x={pc:s.pc,op:s.opcode.name,stack:s.stack.slice(-4)};
   if(s.opcode.name==='KECCAK256'){const offset=Number(s.stack.at(-1)),size=Number(s.stack.at(-2));x.preimage=X(s.memory.slice(offset,offset+size));x.hash=E.keccak256(x.preimage);}
   oracleSteps.push(x);
  }
 });
 evm.events.on('beforeMessage',m=>{
  const item={depth:m.depth,from:m.delegatecall?m.to.toString():m.caller.toString(),to:m.to?(m.delegatecall?m.codeAddress.toString():m.to.toString()):null,code_address:m.to?m.codeAddress.toString():null,
   execution_context:m.to?.toString()||null,effective_msg_sender:m.caller.toString(),
   type:m.to?(m.delegatecall?'DELEGATECALL':m.isStatic?'STATICCALL':'CALL'):(m.salt?'CREATE2':'CREATE'),input:X(m.data),value:m.value};
  messages.push(item);active.push(item);
 });
 evm.events.on('afterMessage',r=>{const item=active.pop();item.output=X(r.execResult.returnValue);item.error=r.execResult.exceptionError?.error||null;item.execution_gas=r.execResult.executionGasUsed;if(r.createdAddress)item.to=r.createdAddress.toString();});
 const block={header:{number:BigInt(header.number),coinbase:A(header.miner),timestamp:BigInt(options.timestamp??header.timestamp),difficulty:BigInt(header.difficulty),prevRandao:H(header.mixHash),gasLimit:BigInt(header.gasLimit),baseFeePerGas:BigInt(header.baseFeePerGas||0),getBlobGasPrice:()=>{throw Error('UNWITNESSED_BLOB_PRICE')}}};
 let result,exception;
 try{result=await evm.runCall({caller:A(options.caller||trace.from),origin:A(options.caller||trace.from),to:A(options.to||trace.to),value:BigInt(options.value??trace.value??0),data:H(options.calldata||trace.input),gasLimit:BigInt(trace.gas),block,gasPrice:0n});}
 catch(e){exception=e.message;}
 const missingAccounts=[...unknownAccounts].filter(a=>!created.has(a));
 const observed=flatten(trace),comparisons=messages.map((m,i)=>{const f=observed[i];return {index:i,match:!!f&&m.type===f.type&&m.from===f.from&&m.to===f.to&&m.input===f.input&&m.output===(f.output||'0x')&&m.value===BigInt(f.value||0)&&m.error===(f.error||null),computed:m,observed:f?{type:f.type,from:f.from,to:f.to,input:f.input,output:f.output,value:f.value,error:f.error}:null};});
 const logs=(result?.execResult.logs||[]).map(([address,topics,data])=>({address:X(address),topics:topics.map(X),data:X(data)}));
 const final={};for(const address of new Set([...Object.keys(inputState),...created])){
  const account=await rawGetAccount(A(address));const stor={};
  for(const [k,v] of state.topStorageStack())if(k.startsWith(address+'_'))stor[k.slice(43)]=E.toBeHex(BigInt(X(v)==='0x'?0:X(v)),32);
  final[address]={balance:account?.balance||0n,nonce:account?.nonce||0n,code:X(await rawGetCode(A(address))),storage:stor};
 }
 const output={role,backend:'@ethereumjs/evm 10.1.3',profile:'chainId 56; Cancun opcode semantics; not a BSC consensus/block-validation claim',
  inputs:'Raw calldata + historical prestate code/storage + block header; no calltree outputs supplied to execution',
  exception:exception||result?.execResult.exceptionError?.error||null,return_value:result?X(result.execResult.returnValue):null,
  missing_accounts:missingAccounts,missing_storage:unknownStorage,created:[...created],opcodes:[...ops].sort(),
  frame_count:messages.length,observed_frame_count:observed.length,all_frames_match:messages.length===observed.length&&comparisons.every(c=>c.match),
  comparisons,reads,writes,logs,final_state:final,oracle_steps:oracleSteps,code_reads:codeReads,
  limitations:['Message semantics only; no transaction pool, fee settlement, BSC consensus, signing, sending, or latency claim','Historical block timestamp is an execution-context input; pending-time admissibility needs a separate freshness proof']};
 if(!options.noSave)save(role+'_evm_replay.json',output);
 if(!options.quiet)console.log(JSON.stringify({role,exception:output.exception,frames:messages.length,expected_frames:observed.length,all_frames_match:output.all_frames_match,missing_accounts:missingAccounts,missing_storage:unknownStorage.slice(0,3),mismatches:comparisons.filter(x=>!x.match).slice(0,2).map(x=>({index:x.index,type:x.computed.type,to:x.computed.to,error:x.computed.error}))},null,2));
 return output;
}
module.exports={run};
if(require.main===module)(async()=>{for(const role of process.argv.slice(2).length?process.argv.slice(2):['first_buy','launch','direct_buy'])await run(role);})().catch(e=>{console.error(e);process.exitCode=1;});
