// Offline microbenchmark only. No network, signing, TX encoding, or sending.
const fs=require('fs'),path=require('path'),os=require('os'),assert=require('assert/strict'),crypto=require('crypto');
const E=require('./abi-tools/node_modules/ethers');
const base=__dirname,old=path.join(base,'2026-09-27-g01'),archive=path.join(base,'2026-09-28-g02-archive-witness-quicknode-153754');
const out=path.join(base,'2026-09-28-g02-latency-offline');fs.mkdirSync(out,{recursive:true});
const sources={};const read=(p)=>{const raw=fs.readFileSync(p);sources[path.relative(base,p)]=crypto.createHash('sha256').update(raw).digest('hex');return JSON.parse(raw);};
const blocks=read(path.join(old,'flap_buy_order_blocks_rpc/001.response.json')).map(r=>r.result);
const fixture=read(path.join(old,'flap_fixture_rpc/001.response.json')),first=read(path.join(old,'flap_first_external_buy_rpc/001.response.json')).find(x=>x.id===1).result;
const launch=fixture.find(x=>x.id===1).result,receipt=fixture.find(x=>x.id===2).result;
const launchBlock=blocks.find(b=>b.hash===launch.blockHash),buyBlock=blocks.find(b=>b.hash===first.blockHash);
assert.equal(launch.hash,'0x5b8cc9893c403a66d4f43bd73ed5c69227807661a58d1d6b5611def3f1d8352c');
assert.equal(first.hash,'0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800');
assert.equal(receipt.blockHash,launchBlock.hash);assert.equal(buyBlock.parentHash,launchBlock.hash);
assert.equal(BigInt(launch.transactionIndex),0n);assert.equal(BigInt(first.transactionIndex),15n);
const refs=(tx,b)=>({transaction:tx.hash,block:Number(BigInt(b.number)),index:Number(BigInt(tx.transactionIndex)),block_hash:b.hash,
 block_milliTimestamp:Number(BigInt(b.milliTimestamp)),utc:new Date(Number(BigInt(b.milliTimestamp))).toISOString()});
const historical={launch:refs(launch,launchBlock),first_buy:refs(first,buyBlock),block_timestamp_gap_ms:Number(BigInt(buyBlock.milliTimestamp)-BigInt(launchBlock.milliTimestamp)),
 measurement:'Difference between included blocks milliTimestamp values, NOT signal-to-execution latency',
 pending_seen_to_send_ms:null,pending_seen_to_execution_ms:null,individual_tx_execution_wallclock_ms:null};
const calc=read(path.join(archive,'first_buy_calculation.json')),trace=read(path.join(archive,'launch_calltrace.json')).result,pre=read(path.join(archive,'first_buy_prestate.json')).result;
assert.equal(trace.input,launch.input);
const types=['string','string','string','uint8','bytes32','uint8','address','uint256','address','bytes','bytes32','bytes','uint8','uint8','uint16','uint16','uint64','uint64','uint16','uint16','uint16','uint16','uint256','address','address','uint8'];
const iface=new E.Interface(['function newTokenV6(('+types.join(',')+')) payable returns(address)']),coder=E.AbiCoder.defaultAbiCoder();
const token='0xabe1fe16cf6bc48c9524713054efe6e954b17777',runtime=pre[token].code,initHash=E.keccak256('0x3d602d80600a3d3981f3'+runtime.slice(2));
const c=Object.fromEntries(Object.entries(calc.inputs).filter(([k])=>k!=='cached_quote_price_slot').map(([k,v])=>[k,BigInt(v)]));
function quote(price){
 const r=c.base_r_literal*10n**18n/price,h=c.h_literal,k=r*(c.total_supply+h)/10n**18n;
 const q=c.wrap_amount*c.pool_multiplier*c.pool_reserve_doge/(c.pool_reserve_wbnb*10000n+c.wrap_amount*c.pool_multiplier);
 const totalFee=q*(c.protocol_fee_bps+c.tax_bps)/10000n,reserve=c.portal_reserve_before+q-totalFee;
 const denominator=r+reserve*10n**10n;
 return c.total_supply+h-(k*10n**18n+denominator-1n)/denominator-c.portal_circulating_before;
}
const arithmetic=()=>quote(c.cached_quote_price);
const decodeAddressQuote=()=>{
 const d=iface.decodeFunctionData('newTokenV6',trace.input)[0];
 const address=E.getCreate2Address(trace.to,d[4],initHash).toLowerCase();
 const price=coder.decode(['uint256','uint256','uint256','bytes'],d[11])[1];
 return [address,quote(price)];
};
assert.equal(arithmetic(),BigInt(calc.steps.actual_output));const checked=decodeAddressQuote();assert.equal(checked[0],token);assert.equal(checked[1],arithmetic());
let sink=0n;
const consume=x=>{sink^=Array.isArray(x)?x[1]:x;};
const raw={clock_pair_overhead:[],arithmetic_only:[],launch_decode_create2_quote:[]},runs=[];
const stats=a=>{const s=[...a].sort((a,b)=>a-b),q=p=>s[Math.ceil(p*s.length)-1];return {samples:s.length,min_ns:s[0],p50_ns:q(.5),p95_ns:q(.95),p99_ns:q(.99),max_ns:s.at(-1),mean_ns:s.reduce((a,b)=>a+b,0)/s.length};};
for(let i=0;i<5000;i++){const t=process.hrtime.bigint();raw.clock_pair_overhead.push(Number(process.hrtime.bigint()-t));}
const sample=(name,fn,round)=>{for(let i=0;i<200;i++)consume(fn());const a=[];for(let i=0;i<2000;i++){const t=process.hrtime.bigint();const v=fn();const ns=Number(process.hrtime.bigint()-t);consume(v);a.push(ns);}raw[name].push(...a);runs.push({round,name,...stats(a)});};
const loadBefore=os.loadavg();
for(let round=1;round<=3;round++){sample('arithmetic_only',arithmetic,round);sample('launch_decode_create2_quote',decodeAddressQuote,round);}
const result={timestamp_utc:new Date().toISOString(),historical,
 machine:{platform:process.platform,arch:process.arch,cpu:os.cpus()[0].model,node:process.version,ethers:E.version,load_before:loadBefore,load_after:os.loadavg()},
 benchmark:{scope:'Warm single-process, single historical fixture, preloaded exact historical state. 3 runs x 2000 samples; 200 warmup calls per run/category.',
 includes:{arithmetic_only:'V2 output + fee arithmetic + r/h/k derivation + curve output',launch_decode_create2_quote:'ABI decode launch calldata + CREATE2 address + extension price decode + same arithmetic'},
 excludes:['network receive','mempool detection','state refresh/invalidation','strategy filters','transaction build','signing','nonce management','socket handoff','RPC acknowledgement','inclusion','on-chain execution'],
 correctness_expected_output:calc.steps.actual_output,clock:'process.hrtime.bigint monotonic; timing overhead included, not subtracted',runs,
 aggregate:Object.fromEntries(Object.entries(raw).map(([k,a])=>[k,stats(a)])),sink:sink.toString(),not_a_g03_harness:true,not_a_latency_gate_pass:true},sources};
fs.writeFileSync(path.join(out,'raw_samples_ns.json'),JSON.stringify(raw));
fs.writeFileSync(path.join(out,'latency_result.json'),JSON.stringify(result,null,2)+'\n');
console.log(JSON.stringify({historical,local_benchmark_ns:result.benchmark.aggregate,machine:result.machine},null,2));
