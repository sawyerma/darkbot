import { chromium } from '../../frontend/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { create, toBinary, fromBinary } from '../../frontend/node_modules/@bufbuild/protobuf/dist/esm/index.js';
// TypeScript descriptors are emitted for this test by esbuild, already supplied with Vite.
import { createRequire } from 'node:module';
const require=createRequire(new URL('../../frontend/package.json',import.meta.url));
const { buildSync }=require('esbuild');
const [port,dir,root]=process.argv.slice(2);const base=`http://127.0.0.1:${port}`;
const bundle=`${dir}/protocol.cjs`;
buildSync({entryPoints:[`${root}/frontend/src/core/generated/darkbot/control/v1/commands_pb.ts`],bundle:true,platform:'node',format:'cjs',outfile:bundle});
const proto=require(bundle);
const browser=await chromium.launch({headless:true});
const p=await browser.newPage();const errors=[];p.on('pageerror',e=>errors.push(e.message));
const api=p.request;
const encode=(id,action,project=1)=>toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project,requestId:id,action,issuedAtUnixNs:1n}));
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
 // UI Start launches the actual isolated capture; only supervisor status changes the label.
 await p.getByTestId('project-startstop-project0').click();
 await until(async()=> (await getProcess()).state==='RUNNING','UI start');
 await until(async()=> (await p.getByTestId('project-process-project0').innerText()).includes('RUNNING'),'UI running');
 const first=await getProcess();const generation=readFileSync(`${dir}/run/p0.generation`,'utf8');
 // Concurrent identical requests share one durable receipt and cannot start a second process.
 const answers=await Promise.all([api.post(base+'/api/v1/command',{headers,data}),api.post(base+'/api/v1/command',{headers,data})]);
 const receipts=await Promise.all(answers.map(async r=>fromBinary(proto.CommandResponseSchema,new Uint8Array(await r.body()))));
 assert.equal(receipts[0].commandId,receipts[1].commandId);assert.equal(receipts[0].status,1);
 assert.equal((await getProcess()).pid,first.pid);assert.equal(readFileSync(`${dir}/run/p0.generation`,'utf8'),generation);
 const conflict=await api.post(base+'/api/v1/command',{headers,data:Buffer.from(encode(id,'project0.capture.stop'))});
 assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await conflict.body())).rejectReason,1);
 assert.equal((await api.get(base+'/api/v1/logs?source=../../.env')).status(),400);
 await p.getByTestId('project-logs-project0').click();
 await p.getByRole('dialog').getByRole('heading',{name:'Project 0',exact:true}).waitFor();
 assert((await p.getByRole('dialog').innerText()).includes('P0/REPORTER'));
 await p.screenshot({path:`${dir}/project0-logs.png`,fullPage:true});
 await p.getByRole('button',{name:'Close logs',exact:true}).click();
 await p.getByTestId('overview-show-all-logs').click();
 await p.getByRole('heading',{name:'gateway',exact:true}).waitFor();
 await p.getByRole('button',{name:'Close logs',exact:true}).click();
 // Kill only this fixture's capture to verify FAILED is real supervisor state.
 const testLabel=`com.darkbot.test.${dir.split('.').at(-1)}.capture`;
 assert(testLabel.startsWith('com.darkbot.test.'));
 execFileSync('/bin/launchctl',['kill','SIGKILL',`gui/${process.getuid()}/${testLabel}`]);
 await until(async()=> (await getProcess()).state==='FAILED','crash state');
 await until(async()=> (await p.getByTestId('project-process-project0').innerText()).includes('FAILED'),'UI failed');
 await p.getByTestId('project-startstop-project0').click();
 await until(async()=> (await p.getByTestId('project-startstop-project0').innerText())==='Stop','restart after failure');
 // The individual Stop button seals the journal and preserves gateway availability.
 p.on('dialog',d=>{ errors.push(`Unexpected native dialog: ${d.type()}`);void d.dismiss(); });
 const singleRun=(await getProcess()).run_id;
 await p.getByTestId('project-startstop-project0').click();
 await p.getByRole('alertdialog').getByRole('button',{name:'Abbrechen',exact:true}).click();
 assert.equal((await getProcess()).state,'RUNNING');
 await p.getByTestId('project-startstop-project0').click();
 await p.getByRole('alertdialog').getByRole('button',{name:'Capture stoppen',exact:true}).click();
 await until(async()=> (await getProcess()).state==='STOPPED','individual stop');
 assert(readFileSync(`${dir}/app/data/market/app-${singleRun}/market_completion.p0j`).length>0);
 await until(async()=> (await p.getByTestId('capture-command-receipt').innerText()).includes('COMPLETED'),'individual receipt');
 await p.reload();
 await until(async()=> (await p.getByTestId('capture-command-receipt').innerText()).includes('COMPLETED'),'receipt restored after reload');
 await p.getByTestId('project-startstop-project0').click();
 await until(async()=> (await p.getByTestId('project-startstop-project0').innerText())==='Stop','next run');
 const stopTarget=(await getProcess()).run_id;
 // Stop All is deliberately scoped to P0; gateway remains reachable throughout drain.
 await p.getByTestId('overview-stop-all').click();
 await p.getByRole('alertdialog').getByRole('button',{name:'Aufzeichnungen stoppen',exact:true}).click();
 let sawStopping=false;
 await until(async()=>{const s=await getProcess();sawStopping ||= s.state==='STOPPING';return s.state==='STOPPED';},'UI stop all');
 assert.equal((await api.get(base+'/healthz')).status(),200);
 assert(readFileSync(`${dir}/app/data/market/app-${stopTarget}/market_completion.p0j`).length>0);
 assert(readFileSync(`${dir}/run/capture.result`,'utf8').includes(`launch_id=${stopTarget}`));
 await until(async()=> (await p.getByTestId('capture-command-receipt').innerText()).includes('COMPLETED'),'stop receipt');
 // A gateway restart preserves idempotency: old start request must not restart a stopped capture.
 execFileSync(`${root}/ops/darkbotctl`,['reload-gateway'],{env:{...process.env,DARKBOT_TEST_DIR:dir,DARKBOT_TEST_PORT:port},stdio:'pipe'});
 await until(async()=>{try{return(await api.get(base+'/healthz')).ok();}catch{return false;}},'gateway restart');
 const reboot=await api.get(base+'/api/v1/session');headers['X-CSRF-Token']=reboot.headers()['x-csrf-token'];
 const replay=await api.post(base+'/api/v1/command',{headers,data});
 assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await replay.body())).commandId,receipts[0].commandId);
 assert.equal((await getProcess()).state,'STOPPED');
 assert.deepEqual(errors,[]);
 writeFileSync(`${dir}/control-test.json`,JSON.stringify({passed:true,sawStopping,run:first.run_id,errors},null,2));
 console.log('PASS UI Start / Stop / Stop All / logs / crash FAILED / sealed journals / restored receipt, real launchd state, session/CSRF/origin/body limits, BSC+Solana disabled, concurrent and durable idempotency; STOPPING observed:',sawStopping);
}finally{await browser.close();}
