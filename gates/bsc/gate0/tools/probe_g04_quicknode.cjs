// Bounded READ-ONLY source capability probe, not the Rust BUY runtime or a latency benchmark.
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const secret=fs.readFileSync(0,'utf8').trim();if(!secret.startsWith('https://'))throw Error('HTTPS endpoint required');
const url=secret.replace(/^https:/,'wss:');const out=path.resolve(__dirname,'../evidence/g04_quicknode_probe');fs.mkdirSync(out,{recursive:true});
const start=process.hrtime.bigint(),packets=[],sent=[],subscriptions={},errors=[];let count=0,bytes=0,done=false,timer;
const stats={pending_full:0,pending_hash_only:0,pending_missing_fields:0,portal_pending:0,selected_v6_pending:0,canonical_portal_logs:0,heads:0};
const ws=new WebSocket(url);const elapsed=()=>process.hrtime.bigint()-start;
function finish(reason){if(done)return;done=true;clearTimeout(timer);try{ws.close()}catch{}
 const data={scope:'One bounded external QuickNode capability sample; no own-geth/48Club/BlockRazor comparison',duration_ns:elapsed().toString(),stop_reason:reason,subscriptions,stats,errors,requests:sent,
  coverage:'NOT_QUALIFIED; selected canonical launch denominator and state/actionable completeness are not established',T_send:null,T_buy:null,packets};
 const body=JSON.stringify(data,null,2)+'\n';if(body.includes(secret)||body.includes(url)||body.includes(secret.split('/').filter(Boolean).at(-1)))throw Error('Credential detected; refusing persistence');
 fs.writeFileSync(path.join(out,'capture.json'),body);fs.writeFileSync(path.join(out,'sha256.json'),JSON.stringify({"capture.json":crypto.createHash('sha256').update(body).digest('hex')},null,2)+'\n');
 console.log(JSON.stringify({stop_reason:reason,messages:count,...stats,errors}));setTimeout(()=>process.exit(0),100);
}
ws.addEventListener('open',()=>{for(const [id,params] of [[1,['newPendingTransactions',true]],[2,['logs',{address:'0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0'}]],[3,['newHeads']]]){const r={jsonrpc:'2.0',id,method:'eth_subscribe',params};sent.push(r);ws.send(JSON.stringify(r));}});
ws.addEventListener('message',event=>{const received=elapsed().toString(),raw=String(event.data);if(bytes+raw.length>8*1024*1024){finish('8MiB bound');return;}bytes+=raw.length;count++;let d;try{d=JSON.parse(raw)}catch{errors.push({error:'non-JSON frame'});finish('protocol error');return;}
 packets.push({local_received_ns:received,message:d});
 if(d.error){errors.push({id:d.id,code:d.error.code,message:d.error.message});}
 if(d.id&&d.result)subscriptions[d.id]=d.result;
 if(d.params){const v=d.params.result,id=d.params.subscription;
  if(id===subscriptions[1]){if(typeof v==='string')stats.pending_hash_only++;else{stats.pending_full++;if(!['hash','from','to','input','value','nonce','gas'].every(k=>k in v))stats.pending_missing_fields++;if(v.to?.toLowerCase()==='0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0'){stats.portal_pending++;if(v.input?.startsWith('0x8cb5772c'))stats.selected_v6_pending++;}}}
  if(id===subscriptions[2])stats.canonical_portal_logs++;
  if(id===subscriptions[3])stats.heads++;
 }
 if(count>=4000)finish('4000-message bound');
});
ws.addEventListener('error',()=>{errors.push({error:'WebSocket connection error; endpoint redacted'});finish('connection error');});
ws.addEventListener('close',()=>finish('server closed'));timer=setTimeout(()=>finish('60-second bound'),60000);
