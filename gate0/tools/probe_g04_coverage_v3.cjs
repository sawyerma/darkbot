const fs=require('fs'),path=require('path'),crypto=require('crypto');

const secret=fs.readFileSync(0,'utf8').trim();
if(!secret.startsWith('https://')) throw Error('HTTPS endpoint required');

const parsed=new URL(secret);
const wsUrl=secret.replace(/^https:/,'wss:');
const OUT=path.resolve(__dirname,'../evidence/g04_quicknode_coverage_v3');

if(fs.existsSync(path.join(OUT,'capture.json')))
  throw Error('v3 capture already exists; refusing overwrite');

fs.mkdirSync(OUT,{recursive:true});

const TOTAL_MS=600000;
const DRAIN_MS=12000;
const FETCH_INTERVAL_MS=250; // max 4 RPC/s
const portal='0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0';

const start=process.hrtime.bigint();
const now=()=>process.hrtime.bigint()-start;

let socket=null,connectionSeq=0,nextId=1000;
let captureEnded=false,finalized=false;
let lastHead=null,activeGap=null;

const subReq=new Map(),subKinds=new Map(),blockReq=new Map();
const queue=[],queued=new Set(),completed=new Set();

const pendingHashes=new Set();
const portalPending=new Map();
const blocks=new Map();
const headEvents=[],connectionGaps=[],errors=[],discontinuities=[];

const stats={
  pending_full:0,
  pending_hash_only:0,
  pending_missing_fields:0,
  portal_pending:0,
  selected_v6_pending:0,
  heads:0,
  block_fetch_ok:0,
  block_fetch_errors:0,
  portal_block_txs:0,
  reconnects:0
};

function enqueue(method,param,reason,attempt=0){
  const key=method+':'+String(param).toLowerCase();
  if(completed.has(key)||queued.has(key)||
     [...blockReq.values()].some(x=>x.key===key)) return;
  queued.add(key);
  queue.push({method,param,reason,attempt,key});
}

function requeue(meta){
  if(meta.attempt>=3){
    errors.push({type:'block_fetch_abandoned',method:meta.method,param:meta.param});
    return;
  }
  setTimeout(()=>enqueue(meta.method,meta.param,meta.reason,meta.attempt+1),1500);
}

function processBlock(b,meta){
  if(!b?.hash) return;
  const h=b.hash.toLowerCase();
  if(blocks.has(h)) return;

  const txs=Array.isArray(b.transactions)?b.transactions:[];
  const pts=txs.filter(t=>
    typeof t==='object' &&
    t.to?.toLowerCase()===portal
  );

  blocks.set(h,{
    hash:h,
    number:b.number,
    parentHash:b.parentHash,
    timestamp:b.timestamp,
    fetched_local_ns:now().toString(),
    request_reason:meta.reason,
    transaction_count:txs.length,
    portal_txs:pts
  });

  stats.block_fetch_ok++;
  stats.portal_block_txs+=pts.length;
}

function connect(){
  if(captureEnded||finalized) return;

  const sock=new WebSocket(wsUrl);
  socket=sock;
  const seq=++connectionSeq;

  sock.addEventListener('open',()=>{
    if(seq>1) stats.reconnects++;

    if(activeGap){
      activeGap.end_local_ns=now().toString();
      connectionGaps.push(activeGap);
      activeGap=null;
    }

    for(const [kind,params] of [
      ['pending',['newPendingTransactions',true]],
      ['heads',['newHeads']]
    ]){
      const id=nextId++;
      subReq.set(id,kind);
      sock.send(JSON.stringify({
        jsonrpc:'2.0',id,method:'eth_subscribe',params
      }));
    }
  });

  sock.addEventListener('message',ev=>{
    let d;
    try{d=JSON.parse(String(ev.data))}
    catch{
      errors.push({type:'non_json_frame'});
      return;
    }

    if(d.error){
      if(blockReq.has(d.id)){
        const meta=blockReq.get(d.id);
        blockReq.delete(d.id);
        stats.block_fetch_errors++;
        errors.push({
          type:'block_rpc_error',
          code:d.error.code,
          message:d.error.message,
          method:meta.method,
          attempt:meta.attempt
        });
        requeue(meta);
      }else{
        errors.push({
          type:'rpc_error',
          code:d.error.code,
          message:d.error.message
        });
      }
      return;
    }

    if(subReq.has(d.id)){
      const kind=subReq.get(d.id);
      subReq.delete(d.id);
      subKinds.set(d.result,kind);
      return;
    }

    if(blockReq.has(d.id)){
      const meta=blockReq.get(d.id);
      blockReq.delete(d.id);
      completed.add(meta.key);
      processBlock(d.result,meta);
      return;
    }

    if(captureEnded||!d.params) return;

    const kind=subKinds.get(d.params.subscription);
    const v=d.params.result;
    const t=now().toString();

    if(kind==='pending'){
      if(typeof v==='string'){
        stats.pending_hash_only++;
        pendingHashes.add(v.toLowerCase());
        return;
      }

      stats.pending_full++;

      if(!['hash','from','to','input','value','nonce','gas'].every(k=>k in v))
        stats.pending_missing_fields++;

      if(v.hash) pendingHashes.add(v.hash.toLowerCase());

      if(v.to?.toLowerCase()===portal){
        stats.portal_pending++;
        if(v.input?.startsWith('0x8cb5772c')) stats.selected_v6_pending++;

        const h=v.hash?.toLowerCase();
        if(h&&!portalPending.has(h))
          portalPending.set(h,{local_received_ns:t,tx:v});
      }
      return;
    }

    if(kind==='heads'){
      stats.heads++;

      const n=BigInt(v.number);
      headEvents.push({
        local_received_ns:t,
        number:v.number,
        number_dec:n.toString(),
        hash:v.hash?.toLowerCase(),
        parentHash:v.parentHash?.toLowerCase(),
        timestamp:v.timestamp
      });

      enqueue('hash',v.hash,'newHead');

      if(lastHead!==null){
        if(n>lastHead+1n){
          discontinuities.push({
            type:'head_gap',
            from:(lastHead+1n).toString(),
            to:(n-1n).toString(),
            detected_local_ns:t
          });

          for(let x=lastHead+1n;x<n;x++)
            enqueue('number','0x'+x.toString(16),'headGapBackfill');
        }else if(n<=lastHead){
          discontinuities.push({
            type:'reorg_or_out_of_order',
            previous:lastHead.toString(),
            current:n.toString(),
            detected_local_ns:t
          });
        }
      }

      if(lastHead===null||n>lastHead) lastHead=n;
    }
  });

  sock.addEventListener('error',()=>{
    errors.push({
      type:'websocket_error',
      local_ns:now().toString()
    });
  });

  sock.addEventListener('close',()=>{
    if(socket===sock) socket=null;

    for(const [id,meta] of [...blockReq.entries()]){
      blockReq.delete(id);
      requeue(meta);
    }

    subReq.clear();
    subKinds.clear();

    if(!captureEnded&&!finalized){
      if(!activeGap)
        activeGap={
          start_local_ns:now().toString(),
          reason:'websocket_closed'
        };
      setTimeout(connect,1000);
    }
  });
}

const fetchTimer=setInterval(()=>{
  if(finalized||queue.length===0) return;
  if(!socket||socket.readyState!==1) return;

  const meta=queue.shift();
  queued.delete(meta.key);

  const id=nextId++;
  blockReq.set(id,meta);

  const method=meta.method==='hash'
    ? 'eth_getBlockByHash'
    : 'eth_getBlockByNumber';

  socket.send(JSON.stringify({
    jsonrpc:'2.0',
    id,
    method,
    params:[meta.param,true]
  }));
},FETCH_INTERVAL_MS);

const progressTimer=setInterval(()=>{
  console.error(
    `[g04-v3] ${(Number(now())/1e9).toFixed(0)}s`+
    ` pending=${stats.pending_full}`+
    ` heads=${stats.heads}`+
    ` blocks=${blocks.size}`+
    ` portalPending=${stats.portal_pending}`+
    ` queue=${queue.length}`+
    ` reconnects=${stats.reconnects}`
  );
},60000);

function finalize(){
  if(finalized) return;
  finalized=true;

  clearInterval(fetchTimer);
  clearInterval(progressTimer);

  if(activeGap){
    activeGap.end_local_ns=now().toString();
    connectionGaps.push(activeGap);
  }

  try{socket?.close()}catch{}

  const data={
    scope:'G0.4 QuickNode v3 bounded canonical-block baseline',
    endpoint_host:parsed.host,
    planned_capture_ms:TOTAL_MS,
    actual_duration_ns:now().toString(),
    stats,
    pending_hashes:[...pendingHashes],
    portal_pending:[...portalPending.values()],
    blocks:[...blocks.values()],
    head_events:headEvents,
    connection_gaps:connectionGaps,
    discontinuities,
    errors,
    unfinished:{
      queue:queue.length,
      inflight:blockReq.size
    },
    claims:{
      T0:null,
      T_send:null,
      T_buy:null,
      buy_latency:null
    }
  };

  const body=JSON.stringify(data,null,2)+'\n';

  const sensitive=[secret,wsUrl,parsed.pathname,parsed.search]
    .filter(x=>x&&x.length>8);

  if(sensitive.some(x=>body.includes(x)))
    throw Error('Credential detected; refusing persistence');

  fs.writeFileSync(path.join(OUT,'capture.json'),body);

  fs.writeFileSync(
    path.join(OUT,'sha256.json'),
    JSON.stringify({
      'capture.json':crypto.createHash('sha256').update(body).digest('hex')
    },null,2)+'\n'
  );

  console.log(JSON.stringify({
    status:'CAPTURE_COMPLETE',
    duration_s:Number(now())/1e9,
    ...stats,
    unique_pending_hashes:pendingHashes.size,
    unique_blocks:blocks.size,
    connection_gaps:connectionGaps.length,
    discontinuities:discontinuities.length,
    errors:errors.length,
    unfinished_queue:queue.length,
    unfinished_inflight:blockReq.size
  },null,2));
}

setTimeout(()=>{
  captureEnded=true;
  console.error('[g04-v3] 600s capture complete; draining block queue...');
  setTimeout(finalize,DRAIN_MS);
},TOTAL_MS);

connect();
