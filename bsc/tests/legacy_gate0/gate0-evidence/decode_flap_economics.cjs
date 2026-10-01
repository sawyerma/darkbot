// Offline ABI / Keccak authority: ethers 6.15.0. Never signs or submits.
const fs = require('fs'), path = require('path');
const E = require('./abi-tools/node_modules/ethers');
const base=path.join(__dirname,'2026-09-27-g01'), out=path.join(__dirname,'2026-09-28-g02-economics');
const read=(p)=>JSON.parse(fs.readFileSync(p));
const replies=(p)=>Object.fromEntries(read(p).map(x=>[x.id,x.result]));
const fixture=replies(path.join(base,'flap_fixture_rpc/001.response.json'));
const router=replies(path.join(base,'flap_first_external_buy_rpc/001.response.json'))[1];
const iface=new E.Interface(['function swapExactInput((address inputToken,address outputToken,uint256 inputAmount,uint256 minOutputAmount,bytes permitData) params) payable returns (uint256)', 'function execute(bytes commands,bytes[] inputs) payable']);
const clean=x=>JSON.parse(JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v));
const direct=iface.decodeFunctionData('swapExactInput',fixture[5].input)[0];
if(iface.encodeFunctionData('swapExactInput',[direct]).toLowerCase()!==fixture[5].input.toLowerCase())throw Error('direct ABI roundtrip');
const rt=iface.decodeFunctionData('execute',router.input);
if(iface.encodeFunctionData('execute',rt).toLowerCase()!==router.input.toLowerCase())throw Error('router envelope ABI roundtrip');
const coder=E.AbiCoder.defaultAbiCoder();
const commands=Array.from(E.getBytes(rt.commands));
const segments=rt.inputs.map((raw,i)=>({command:commands[i],raw,words:Array.from({length:(raw.length-2)/64},(_,j)=>'0x'+raw.slice(2+j*64,2+(j+1)*64))}));
const launch=read(path.join(base,'flap_v6_fixture_decoded.json'));
const runtime=read(path.join(base,'flap_token_runtime_evidence.json')).runtime_code;
const init='0x3d602d80600a3d3981f3'+runtime.slice(2);
const create2=E.getCreate2Address(launch.to,launch.params.salt,E.keccak256(init)).toLowerCase();
if(create2!=='0xabe1fe16cf6bc48c9524713054efe6e954b17777')throw Error('CREATE2');
const logIface=new E.Interface([
'event Transfer(address indexed from,address indexed to,uint256 value)',
'event Approval(address indexed owner,address indexed spender,uint256 value)',
'event Deposit(address indexed dst,uint256 wad)',
'event Withdrawal(address indexed src,uint256 wad)',
'event Sync(uint112 reserve0,uint112 reserve1)',
'event Swap(address indexed sender,uint256 amount0In,uint256 amount1In,uint256 amount0Out,uint256 amount1Out,address indexed to)',
'event Swap(address indexed sender,address indexed recipient,int256 amount0,int256 amount1,uint160 sqrtPriceX96,uint128 liquidity,int24 tick,uint128 protocolFeesToken0,uint128 protocolFeesToken1)',
'event TokenBought(uint256 ts,address token,address buyer,uint256 amount,uint256 eth,uint256 fee,uint256 postPrice)',
'event TokenCurveSetV2(address token,uint256 r,uint256 h,uint256 k)',
'event TokenDexSupplyThreshSet(address token,uint256 dexSupplyThresh)',
'event FlapTokenCirculatingSupplyChanged(address token,uint256 newSupply)',
'event TokenQuoteTokenSet(address token,address quoteToken)',
'event TaxOnBondingCurvePaid(address indexed token,uint256 amount)',
'event TaxV2OnBondingCurvePaid(address indexed token,uint256 amount)',
]);
function decodeReceipt(r){return {...r,gas_used_decimal:BigInt(r.gasUsed).toString(),gas_price_decimal:BigInt(r.effectiveGasPrice).toString(),gas_cost_wei:(BigInt(r.gasUsed)*BigInt(r.effectiveGasPrice)).toString(),decoded_logs:r.logs.map(l=>{let d;try{d=logIface.parseLog(l)}catch{};return {...l,decoded:d?{name:d.name,signature:d.signature,args:Object.fromEntries(d.fragment.inputs.map((p,i)=>[p.name,clean(d.args[i])]))}:null};})};}
const receipts={launch:decodeReceipt(fixture[2]),router:decodeReceipt(read(path.join(out,'router_receipt.json'))),direct:decodeReceipt(fixture[6])};
fs.writeFileSync(path.join(out,'decoded_receipts.json'),JSON.stringify(receipts,null,2)+'\n');
const allBlocks=replies(path.join(base,'flap_buy_order_receipts_rpc/001.response.json'));
const timeline=Object.values(allBlocks).flat().filter(r=>r.logs.some(l=>l.address.toLowerCase()===create2)).map(decodeReceipt).sort((a,b)=>Number(BigInt(a.blockNumber)-BigInt(b.blockNumber)) || Number(BigInt(a.transactionIndex)-BigInt(b.transactionIndex)));
fs.writeFileSync(path.join(out,'decoded_timeline.json'),JSON.stringify(timeline,null,2)+'\n');
const result={library:'ethers',version:E.version,swap_selector:iface.getFunction('swapExactInput').selector,swap_signature:iface.getFunction('swapExactInput').format('sighash'),create2,init_code_hash:E.keccak256(init),router:{...router,envelope_signature:'execute(bytes,bytes[])',envelope_roundtrip:true,contract_identity:'UNVERIFIED; ABI-compatible envelope only',commands,segments},direct:{...fixture[5],decoded:Object.fromEntries(['inputToken','outputToken','inputAmount','minOutputAmount','permitData'].map((k,i)=>[k,clean(direct[i])])),abi_roundtrip:true}};
const launchNames=Object.keys(launch.params);
const launchTypes=['string','string','string','uint8','bytes32','uint8','address','uint256','address','bytes','bytes32','bytes','uint8','uint8','uint16','uint16','uint64','uint64','uint16','uint16','uint16','uint16','uint256','address','address','uint8'];
const launchIface=new E.Interface(['function newTokenV6(('+launchTypes.join(',')+')) payable returns(address)']);
const decodedLaunch=launchIface.decodeFunctionData('newTokenV6',fixture[1].input)[0];
if(launchIface.encodeFunctionData('newTokenV6',[decodedLaunch]).toLowerCase()!==fixture[1].input.toLowerCase())throw Error('launch roundtrip');
if(decodedLaunch[4]!==launch.params.salt || decodedLaunch[6].toLowerCase()!==launch.params.quoteToken || decodedLaunch[25]!==6n)throw Error('launch fixture mismatch');
result.launch={selector:launchIface.getFunction('newTokenV6').selector,roundtrip:true,params:Object.fromEntries(launchNames.map((k,i)=>[k,clean(decodedLaunch[i])]))};
const stateFile=path.join(out,'state_rpc/001.response.json');
if(fs.existsSync(stateFile)) result.runtime_codehashes=Object.fromEntries(read(stateFile).filter(r=>[6,7,8,9].includes(r.id)&&r.result).map(r=>[r.id,{bytes:E.getBytes(r.result).length,keccak256:E.keccak256(r.result)}]));
fs.writeFileSync(path.join(out,'abi_decode.json'),JSON.stringify(result,null,2)+'\n');
const quote='0xba2ae424d960c26247dd6c32edc70b295c744c43',portal=launch.to,token=create2;
const call=(address,signature,types,args,tag='latest')=>({method:'eth_call',params:[{to:address,data:E.id(signature).slice(0,10)+coder.encode(types,args).slice(2)},tag]});
const requests=[{method:'eth_chainId',params:[]},...['name()','symbol()','decimals()'].map(s=>call(quote,s,[],[])),{method:'web3_sha3',params:[E.hexlify(E.toUtf8Bytes(result.swap_signature))]}];
for(const address of [router.to,'0xac109c8025f272414fd9e2faa805a583708a017f','0xce6160bb594fc055c943f59de92cee30b8c6b32c','0x3a632ecc01a45b8002d76c15e172e71c79412003']) requests.push({method:'eth_getCode',params:[address,'latest']});
for(const tag of ['0x76a6d7d','latest']) {
 requests.push(call(portal,'getTokenV7(address)',['address'],[token],tag));
 requests.push(call(portal,'getQuoteTokenConfiguration(address)',['address'],[quote],tag));
 requests.push(call(portal,'getFeeRate()',[],[],tag));
}
for(const pair of ['0xac109c8025f272414fd9e2faa805a583708a017f','0xce6160bb594fc055c943f59de92cee30b8c6b32c','0x3a632ecc01a45b8002d76c15e172e71c79412003'])for(const s of ['token0()','token1()','factory()'])requests.push(call(pair,s,[],[]));
fs.writeFileSync(path.join(out,'state_requests.json'),JSON.stringify(requests,null,2)+'\n');
console.log(JSON.stringify({version:E.version,swap_selector:result.swap_selector,create2,direct:result.direct.decoded,router_commands:commands,router_words:segments.map(s=>({command:s.command,words:s.words})),decoded_log_counts:Object.fromEntries(Object.entries(receipts).map(([k,r])=>[k,r.decoded_logs.filter(l=>l.decoded).length]))},null,2));
