const fs=require('fs'),path=require('path'),assert=require('assert/strict'),crypto=require('crypto');

const ROOT=path.resolve(__dirname,'../..');
const P=path.join(ROOT,'gate0/evidence/g04_quicknode_coverage_v3');

const E=require(path.join(
  ROOT,'tests/DARKBOT 2/gate0-evidence/abi-tools/node_modules/ethers'
));

const bytes=fs.readFileSync(path.join(P,'capture.json'));
const manifest=JSON.parse(fs.readFileSync(path.join(P,'sha256.json')));

assert.equal(
  crypto.createHash('sha256').update(bytes).digest('hex'),
  manifest['capture.json']
);

const d=JSON.parse(bytes);

const portal='0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';
const doge='0xba2ae424d960c26247dd6c32edc70b295c744c43';
const extension='0x3526f6a0e790306d64c896eeda5ec66052ea17889e20632ff0cf512923153dad';

const types=[
  'string','string','string','uint8','bytes32','uint8','address','uint256',
  'address','bytes','bytes32','bytes','uint8','uint8','uint16','uint16',
  'uint64','uint64','uint16','uint16','uint16','uint16','uint256',
  'address','address','uint8'
];

const it=new E.Interface([
  'function newTokenV6(('+types.join(',')+')) payable returns(address)'
]);

const pending=new Set(d.pending_hashes.map(x=>x.toLowerCase()));
const portalPending=new Map(
  d.portal_pending
    .filter(x=>x.tx?.hash)
    .map(x=>[x.tx.hash.toLowerCase(),x])
);

const censusStart=30n*1000000000n;
const censusEnd=570n*1000000000n;

const censusHeads=d.head_events.filter(h=>{
  const t=BigInt(h.local_received_ns);
  return t>=censusStart&&t<=censusEnd;
});

assert(censusHeads.length>0,'No heads in census window');

const minN=censusHeads.reduce((a,h)=>BigInt(h.number_dec)<a?BigInt(h.number_dec):a,BigInt(censusHeads[0].number_dec));
const maxN=censusHeads.reduce((a,h)=>BigInt(h.number_dec)>a?BigInt(h.number_dec):a,BigInt(censusHeads[0].number_dec));

const latestHeadHash=new Map();
for(const h of censusHeads)
  latestHeadHash.set(h.number_dec,h.hash?.toLowerCase());

const blocksByNumber=new Map();

for(const b of d.blocks){
  const n=BigInt(b.number);
  if(n<minN||n>maxN) continue;

  const key=n.toString();
  if(!blocksByNumber.has(key)) blocksByNumber.set(key,[]);
  blocksByNumber.get(key).push(b);
}

const selected=[];
const missingBlocks=[];

for(let n=minN;n<=maxN;n++){
  const key=n.toString();
  const candidates=blocksByNumber.get(key)||[];
  const preferred=latestHeadHash.get(key);

  let b=preferred
    ? candidates.find(x=>x.hash?.toLowerCase()===preferred)
    : null;

  if(!b&&candidates.length) b=candidates[candidates.length-1];

  if(!b) missingBlocks.push(key);
  else selected.push(b);
}

const launches=[];

for(const b of selected){
  for(const tx of b.portal_txs||[]){
    if(tx.to?.toLowerCase()!==portal) continue;
    if(!tx.input?.startsWith('0x8cb5772c')) continue;

    let lp;
    try{
      lp=it.decodeFunctionData('newTokenV6',tx.input)[0];
      assert.equal(
        it.encodeFunctionData('newTokenV6',[lp]).toLowerCase(),
        tx.input.toLowerCase()
      );
    }catch(e){
      launches.push({
        tx_hash:tx.hash?.toLowerCase(),
        block_number:b.number,
        decode_error:String(e.message||e)
      });
      continue;
    }

    const hash=tx.hash.toLowerCase();
    const pp=portalPending.get(hash);

    const scopeMatch=
      lp[25]===6n &&
      lp[6].toLowerCase()===doge &&
      lp[10].toLowerCase()===extension;

    let pendingActionable=false;

    if(pp?.tx?.input){
      try{
        const p=it.decodeFunctionData('newTokenV6',pp.tx.input)[0];
        pendingActionable=
          it.encodeFunctionData('newTokenV6',[p]).toLowerCase()===
          pp.tx.input.toLowerCase();
      }catch{}
    }

    launches.push({
      tx_hash:hash,
      block_number:b.number,
      token_version:lp[25].toString(),
      quote_token:lp[6].toLowerCase(),
      quote_amount:lp[7].toString(),
      extension_id:lp[10].toLowerCase(),
      buy_tax:lp[14].toString(),
      sell_tax:lp[15].toString(),
      frozen_scope_match:scopeMatch,
      seen_in_pending:pending.has(hash),
      full_pending_object_seen:!!pp,
      pending_actionable:pendingActionable,
      first_actionable_local_ns:pendingActionable
        ? pp.local_received_ns
        : null
    });
  }
}

const valid=launches.filter(x=>!x.decode_error);
const v6Den=valid.length;
const v6Seen=valid.filter(x=>x.seen_in_pending).length;
const v6Actionable=valid.filter(x=>x.pending_actionable).length;

const scope=valid.filter(x=>x.frozen_scope_match);
const scopeDen=scope.length;
const scopeSeen=scope.filter(x=>x.seen_in_pending).length;
const scopeActionable=scope.filter(x=>x.pending_actionable).length;

const result={
  status:scopeDen>0
    ? 'G0.4_PARTIAL_CANONICAL_COVERAGE_SAMPLE'
    : 'G0.4_PARTIAL_NO_FROZEN_SCOPE_DENOMINATOR',
  census:{
    first_block:minN.toString(),
    last_block:maxN.toString(),
    expected_blocks:Number(maxN-minN+1n),
    selected_blocks:selected.length,
    missing_blocks:missingBlocks.length,
    missing_block_numbers:missingBlocks
  },
  source:{
    reconnects:d.stats.reconnects,
    connection_gaps:d.connection_gaps,
    discontinuities:d.discontinuities,
    rpc_errors:d.errors
  },
  canonical_v6_launch_denominator:v6Den,
  canonical_v6_seen_pending:v6Seen,
  canonical_v6_actionable:v6Actionable,
  canonical_v6_pending_coverage:v6Den?v6Seen/v6Den:null,
  canonical_v6_actionable_coverage:v6Den?v6Actionable/v6Den:null,
  frozen_scope_launch_denominator:scopeDen,
  frozen_scope_seen_pending:scopeSeen,
  frozen_scope_actionable:scopeActionable,
  frozen_scope_pending_coverage:scopeDen?scopeSeen/scopeDen:null,
  frozen_scope_actionable_coverage:scopeDen?scopeActionable/scopeDen:null,
  launches:valid,
  T0:null,
  T_send:null,
  T_buy:null,
  limitations:[
    'QuickNode-only baseline; no own-geth/48Club/BlockRazor comparison',
    'No BUY latency inferred from subscription timing',
    'No value-bearing transaction sent',
    'Gate remains PARTIAL even if this bounded sample reaches 100% coverage'
  ]
};

fs.writeFileSync(
  path.join(P,'analysis.json'),
  JSON.stringify(result,null,2)+'\n'
);

console.log(JSON.stringify({
  status:result.status,
  census_blocks_expected:result.census.expected_blocks,
  census_blocks_selected:result.census.selected_blocks,
  census_blocks_missing:result.census.missing_blocks,
  reconnects:result.source.reconnects,
  connection_gaps:result.source.connection_gaps.length,
  rpc_errors:result.source.rpc_errors.length,
  canonical_v6_launches:v6Den,
  canonical_v6_seen_pending:v6Seen,
  canonical_v6_actionable:v6Actionable,
  v6_pending_coverage:result.canonical_v6_pending_coverage,
  v6_actionable_coverage:result.canonical_v6_actionable_coverage,
  frozen_scope_launches:scopeDen,
  frozen_scope_seen_pending:scopeSeen,
  frozen_scope_actionable:scopeActionable,
  frozen_scope_pending_coverage:result.frozen_scope_pending_coverage,
  frozen_scope_actionable_coverage:result.frozen_scope_actionable_coverage
},null,2));
