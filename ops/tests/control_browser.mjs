import { chromium } from '../../frontend/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import { create, toBinary, fromBinary } from '../../frontend/node_modules/@bufbuild/protobuf/dist/esm/index.js';
// TypeScript descriptors are emitted for this test by esbuild, already supplied with Vite.
import { createRequire } from 'node:module';
const require=createRequire(new URL('../../frontend/package.json',import.meta.url));
const { buildSync }=require('esbuild');
const [port,dir,root]=process.argv.slice(2);const base=`http://127.0.0.1:${port}`;
const bundle=`${dir}/protocol.cjs`;
buildSync({entryPoints:[`${root}/frontend/src/core/generated/darkbot/control/v1/commands_pb.ts`],bundle:true,platform:'node',format:'cjs',outfile:bundle});
const proto=require(bundle);
const streamBundle=`${dir}/stream.cjs`;
buildSync({stdin:{contents:'export * from "./stream_pb.ts"; export * from "./topics_pb.ts";',resolveDir:`${root}/frontend/src/core/generated/darkbot/control/v1`,loader:'ts'},bundle:true,platform:'node',format:'cjs',outfile:streamBundle});
const stream=require(streamBundle);
const browser=await chromium.launch({headless:true});
const p=await browser.newPage();const errors=[];p.on('pageerror',e=>errors.push(e.message));
// Every WebSocket of the page with its decoded client and server frames.
const sockets=[];
p.on('websocket',ws=>{const s={sent:[],got:[],closed:false};sockets.push(s);
 const bytes=f=>typeof f.payload==='string'?null:new Uint8Array(f.payload);
 ws.on('framesent',f=>{const b=bytes(f);if(b)s.sent.push(fromBinary(stream.ClientMessageSchema,b));});
 ws.on('framereceived',f=>{const b=bytes(f);if(b)s.got.push(fromBinary(stream.ServerMessageSchema,b));});
 ws.on('close',()=>{s.closed=true;});});
const active=s=>{const set=new Set();for(const m of s.sent){if(m.msg.case==='subscribe')set.add(m.msg.value.topic);if(m.msg.case==='unsubscribe')set.delete(m.msg.value.topic);}return set;};
const api=p.request;
const encode=(id,action,project=1)=>toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project,requestId:id,action,issuedAtUnixNs:BigInt(Date.now())*1_000_000n}));
const getProcess=async()=>await (await api.get(base+'/api/v1/project0/process')).json();
const until=async(fn,msg)=>{for(let i=0;i<100;i++){if(await fn())return;await new Promise(r=>setTimeout(r,200));}throw Error(msg);};
try{
 // Authentication is mandatory even on local read endpoints.
 assert.equal((await api.get(base+'/api/v1/project0/process')).status(),401);
 assert.equal((await api.get(base+'/api/v1/logs?source=all')).status(),401);
 await p.goto(base+'/');
 await until(async()=>await p.getByTestId('project-process-project0').innerText().then(x=>x.includes('STOPPED')),'initial stopped');
 for(const project of ['bsc','solana']){
  assert.equal(await p.getByTestId(`project-startstop-${project}`).isDisabled(),true);
  assert.equal(await p.getByTestId(`project-logs-${project}`).isDisabled(),project==='solana');
 }
 const boot=await api.get(base+'/api/v1/session');const csrf=boot.headers()['x-csrf-token'];assert(csrf);
 const id=crypto.randomUUID();const data=Buffer.from(encode(id,'project0.capture.start'));
 const headers={'Content-Type':'application/x-protobuf',Origin:base,'X-CSRF-Token':csrf};
 assert.equal((await api.post(base+'/api/v1/command',{headers:{'Content-Type':'application/x-protobuf',Origin:base},data})).status(),403);
 assert.equal((await api.post(base+'/api/v1/command',{headers:{...headers,'X-CSRF-Token':'wrong'},data})).status(),403);
 assert.equal((await api.post(base+'/api/v1/command',{headers:{...headers,Origin:'https://evil.invalid'},data})).status(),403);
 assert.equal((await api.post(base+'/api/v1/command',{headers,data:Buffer.alloc(5000)})).status(),413);
 const forbidden=await api.post(base+'/api/v1/command',{headers,data:Buffer.from(encode(crypto.randomUUID(),'project0.capture.start',2))});
 assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await forbidden.body())).status,3);
 // DEC-07: a raw capture reads Mainnet only through its approved boot flow (mode market). The
 // Overview Start is disabled with that reason, and a direct start is refused before any
 // controller call: no launchd job, no reporter epoch. The admitted start stays offline NOT_RUN.
 const start=p.getByTestId('project-startstop-project0');
 assert.equal(await start.isDisabled(),true);
 assert((await p.locator('.ov-disabled-action',{has:start}).getAttribute('title')).startsWith('BOOT_AUDIT_REQUIRED'));
 const answers=await Promise.all([api.post(base+'/api/v1/command',{headers,data}),api.post(base+'/api/v1/command',{headers,data})]);
 for(const r of await Promise.all(answers.map(async r=>fromBinary(proto.CommandResponseSchema,new Uint8Array(await r.body()))))){
  assert.equal(r.status,2);assert.equal(r.rejectReason,3);assert(r.detail.startsWith('BOOT_AUDIT_REQUIRED'),r.detail);assert.equal(r.commandId,'');
 }
 assert.equal((await getProcess()).state,'STOPPED');
 assert.equal(existsSync(`${dir}/run/capture.launch`),false);assert.equal(existsSync(`${dir}/run/p0.generation`),false);
 // A stop is accepted without a capture; its key is durable: another action under it conflicts.
 const stopId=crypto.randomUUID();const stopData=Buffer.from(encode(stopId,'project0.capture.stop'));
 const stopped=fromBinary(proto.CommandResponseSchema,new Uint8Array(await (await api.post(base+'/api/v1/command',{headers,data:stopData})).body()));
 assert.equal(stopped.status,1);assert.equal(stopped.commandId,stopId);
 const conflict=await api.post(base+'/api/v1/command',{headers,data:Buffer.from(encode(stopId,'project0.capture.start'))});
 assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await conflict.body())).rejectReason,1);
 assert.equal((await api.get(base+'/api/v1/logs?source=../../.env')).status(),400);
 await p.getByTestId('project-logs-project0').click();
 await p.getByRole('dialog').getByRole('heading',{name:'Project 0',exact:true}).waitFor();
 await p.screenshot({path:`${dir}/project0-logs.png`,fullPage:true});
 await p.getByRole('button',{name:'Close logs',exact:true}).click();
 await p.getByTestId('overview-show-all-logs').click();
 await p.getByRole('heading',{name:'gateway',exact:true}).waitFor();
 await p.getByRole('button',{name:'Close logs',exact:true}).click();
 // A gateway restart preserves idempotency: the stop's key returns its own receipt.
 execFileSync(`${root}/ops/darkbotctl`,['reload-gateway'],{env:{...process.env,DARKBOT_TEST_DIR:dir,DARKBOT_TEST_PORT:port},stdio:'pipe'});
 await until(async()=>{try{return(await api.get(base+'/healthz')).ok();}catch{return false;}},'gateway restart');
 const reboot=await api.get(base+'/api/v1/session');headers['X-CSRF-Token']=reboot.headers()['x-csrf-token'];
 const replay=await api.post(base+'/api/v1/command',{headers,data:stopData});
 assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await replay.body())).commandId,stopId);
 assert.equal((await getProcess()).state,'STOPPED');
 // One shared WebSocket across routes; overview.aggregates only while the overview is shown.
 const OV=stream.TopicId.OVERVIEW_AGGREGATES, BSC=stream.TopicId.BSC_STATUS;
 await p.goto(base+'/');
 await until(async()=>{const s=sockets.at(-1);return !!s&&!s.closed&&active(s).has(OV);},'overview subscribed on /');
 const opened=sockets.length, live=sockets.at(-1);
 await p.getByTestId('project-link-project0').click();await p.waitForURL(base+'/project-0');
 await until(async()=>!active(live).has(OV),'overview released on /project-0');
 await p.goBack();await p.waitForURL(base+'/');
 await until(async()=>active(live).has(OV),'overview subscribed again on /');
 await p.getByTestId('project-link-bsc').click();await p.waitForURL(base+'/bsc');
 await until(async()=>!active(live).has(OV)&&active(live).has(BSC),'BSC topics without overview on /bsc');
 assert.equal(sockets.length,opened,'route changes reuse the one WebSocket');assert.equal(live.closed,false);
 // Gateway crash: launchd restarts it; the page reconnects by itself and resubscribes this route only.
 const gatewayLabel=`com.darkbot.test.${dir.split('.').at(-1)}.gateway`;
 assert(gatewayLabel.startsWith('com.darkbot.test.'));
 execFileSync('/bin/launchctl',['kill','SIGKILL',`gui/${process.getuid()}/${gatewayLabel}`]);
 await until(async()=>live.closed,'socket closed by the gateway crash');
 const acked=(s,topic)=>s.sent.some(m=>m.msg.case==='subscribe'&&m.msg.value.topic===topic&&s.got.some(a=>a.msg.case==='controlAck'&&a.msg.value.requestId===m.msg.value.requestId&&a.msg.value.status===stream.AckStatus.ACCEPTED));
 await until(async()=>{const s=sockets.at(-1);return s!==live&&!s.closed&&s.got.some(m=>m.msg.case==='welcome'&&m.msg.value.status===stream.WelcomeStatus.ACCEPTED)&&acked(s,BSC);},'reconnected and resubscribed');
 const again=sockets.at(-1);
 assert.equal(active(again).has(OV),false);
 assert.equal(sockets.filter(s=>!s.closed).length,1,'exactly one open WebSocket after reconnect');
 assert.deepEqual(errors,[]);
 writeFileSync(`${dir}/control-test.json`,JSON.stringify({passed:true,errors},null,2));
 console.log('PASS Overview Start gated by the boot flow (BOOT_AUDIT_REQUIRED, no controller call), durable stop receipt and conflict, logs, session/CSRF/origin/body limits, BSC+Solana disabled, gateway restart idempotency, one WebSocket across routes and crash reconnect');
}finally{await browser.close();}
