import {chromium} from '../../frontend/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,chmodSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {create,toBinary,fromBinary} from '../../frontend/node_modules/@bufbuild/protobuf/dist/esm/index.js';
import {createRequire} from 'node:module';
const require=createRequire(new URL('../../frontend/package.json',import.meta.url));
const [port,dir,root]=process.argv.slice(2);const base=`http://127.0.0.1:${port}`;
require('esbuild').buildSync({entryPoints:[`${root}/frontend/src/core/generated/darkbot/control/v1/commands_pb.ts`],bundle:true,platform:'node',format:'cjs',outfile:`${dir}/commands.cjs`});
const proto=require(`${dir}/commands.cjs`);
const browser=await chromium.launch({headless:true});const p=await browser.newPage();const api=p.request;const errors=[];
// Every request the page itself makes; the harness API context below is not counted.
const pageRequests=[];p.on('request',r=>pageRequests.push({method:r.method(),path:new URL(r.url()).pathname}));
const statusGets=(from,path)=>pageRequests.slice(from).filter(r=>r.method==='GET'&&r.path===path).length;
const idle=ms=>new Promise(r=>setTimeout(r,ms));
// FRONTEND_PHASENPLAN §13: every tab shows its numbered sections in exactly this order.
const sections=JSON.parse(readFileSync(`${root}/frontend/contracts/bsc-fields.json`,'utf8')).panel_sections;
const salt=readFileSync(`${dir}/salt.txt`,'utf8').trim();
const headings=async()=>(await p.locator('h2.g-section-title').allInnerTexts()).map(t=>t.trim());
const expectSections=async(panel)=>assert.deepEqual(await headings(),sections[panel].map((t,i)=>`${i+1}. ${t}`),panel+' section order');
const body=async()=>await p.locator('body').innerText();
p.on('pageerror',e=>errors.push(e.message));p.on('dialog',d=>{errors.push('native dialog');void d.dismiss();});
const until=async(fn,msg)=>{for(let i=0;i<150;i++){if(await fn())return;await new Promise(r=>setTimeout(r,200));}throw Error(msg);};
const state=async()=>await(await api.get(base+'/api/v1/bsc/process')).json();
try{
 assert.equal((await api.get(base+'/api/v1/bsc/process')).status(),401);
 await p.goto(base+'/');
 await until(async()=> (await p.getByTestId('project-process-bsc').innerText()).includes('STOPPED'),'BSC initial stopped');
 await until(async()=>!(await p.getByTestId('project-startstop-bsc').isDisabled()),'BSC controller available');
 const card=p.getByTestId('project-card-bsc');
 await until(async()=> (await card.innerText()).includes('Pre-Live: BLOCKED'),'native BLOCKED reaches the overview unchanged');
 assert.equal(await card.locator('dd').first().innerText(),'UNKNOWN');
 assert.equal(await p.getByTestId('project-startstop-solana').isDisabled(),true);
 const boot=await api.get(base+'/api/v1/session');const headers={'Content-Type':'application/x-protobuf',Origin:base,'X-CSRF-Token':boot.headers()['x-csrf-token']};
 const encode=(action,id=crypto.randomUUID(),resourceId='')=>Buffer.from(toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project:2,requestId:id,action,resourceId,issuedAtUnixNs:BigInt(Date.now())*1_000_000n})));
 assert.equal((await api.post(base+'/api/v1/command',{headers:{'Content-Type':'application/x-protobuf',Origin:base},data:encode('bsc.readiness.start')})).status(),403);
 for(const action of ['g07','arm','probe','send','bsc.dry.start','bsc.arm','bsc.probe','bsc.send']){
  const res=await api.post(base+'/api/v1/command',{headers,data:encode(action)});assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await res.body())).status,3);
 }
 await p.getByTestId('project-startstop-bsc').click();
 await until(async()=>{const s=await state();return s.run_id&&s.state==='STOPPED';},'native readiness result');
 const verdict=readFileSync(`${dir}/run/bsc.readiness.result`,'utf8').match(/^result_code=(\w+)$/m)?.[1];
 assert(['BLOCKED','STOP','PASS_READ_ONLY'].includes(verdict),'native verdict, not a wrapper exit-code mapping: '+verdict);
 // No browser HTTP polling on the global page: BSC state is pushed (Project 0 capture keeps its own watcher, separate owner gate).
 let mark=pageRequests.length;await idle(8000);
 assert.equal(statusGets(mark,'/api/v1/bsc/process'),0);assert.equal(statusGets(mark,'/api/v1/command'),0);
 const p0Polls=statusGets(mark,'/api/v1/project0/process');
 await p.getByTestId('project-link-bsc').click();
 await until(async()=> (await p.locator('body').innerText()).includes('isolated-fixture'),'BSC panel real feed');
 const gateLine=id=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:new RegExp('^'+id.replace('.','\\.')+'$')})}).first();
 await until(async()=> (await gateLine('G0.8').innerText()).includes('UNRESOLVED'),'Overview G0.8 gate UNRESOLVED');
 assert((await gateLine('G0.8').innerText()).includes('CHAIN_UNKNOWN · EVIDENCE_UNKNOWN'),'Overview G0.8 dimensions UNKNOWN');
 mark=pageRequests.length;await idle(8000);
 for(const path of ['/api/v1/bsc/process','/api/v1/project0/process','/api/v1/command'])assert.equal(statusGets(mark,path),0,'periodic GET on /bsc: '+path);
 assert((await p.locator('body').innerText()).includes('Data as of'));
 await expectSections('overview');
 for(const name of ['BUY','Send','Arm','Probe','Run DRY'])assert.equal(await p.getByRole('button',{name,exact:true}).count(),0);
 await p.screenshot({path:`${dir}/bsc-overview.png`,fullPage:true});
 await p.getByRole('tab',{name:'Wallets',exact:true}).click();
 await until(async()=> (await p.locator('body').innerText()).includes('Wallet snapshots'),'wallet snapshots');
 await expectSections('wallets');
 await until(async()=> (await body()).includes('launcher_pending_nonce · g06a.preflight-readonly.v3'),'native preflight nonce in Wallets section 6');
 await p.screenshot({path:`${dir}/bsc-wallets.png`,fullPage:true});
 await p.getByRole('tab',{name:'Launch / BUY',exact:true}).click();
 await p.getByRole('heading',{name:'8. '+sections.launch[7],exact:true}).waitFor();
 await expectSections('launch');
 await until(async()=> (await body()).includes('protocol_fee_bps · g06a.native-bnb-quote.v2'),'native quote leaf in Launch section 3');
 for(const record of ['g06a.native-bnb-quote.v2','g06a.economics-evm-crosscheck.v2','g06a.launch-path','g07.portal-upgrade-scan.v2','g06a.prepare-handoff','g06a.self-launch-bundle-dry.import.v1','g06a.local-simulation.import.v1'])assert((await body()).includes('Record · '+record),record);
 const html=(await p.content()).toLowerCase();assert(!html.includes(salt.toLowerCase().replace(/^0x/,'')),'raw salt reached the browser');assert(!(await body()).includes('salt · g06a.economics'));
 const checks=p.getByLabel('Filter readiness checks');
 await checks.fill('no-such-native-field');
 const checkSection=checks.locator('xpath=ancestor::section[1]');
 assert((await checkSection.innerText()).includes('No matching fields.'));
 await checkSection.getByRole('button',{name:'Clear filter',exact:true}).click();
 assert.equal(await checks.inputValue(),'');
 await until(async()=> (await checkSection.innerText()).includes('V5_CONTRACT'),'new readiness result reaches Launch panel');
 await until(async()=> (await p.locator('.g-line',{hasText:'Pre-Live Readiness'}).first().innerText()).includes(verdict),'Launch panel shows the native verdict');
 assert.equal(await p.getByTestId('bsc-readiness-control').count(),1);
 await p.screenshot({path:`${dir}/bsc-launch.png`,fullPage:true});
 await p.getByRole('tab',{name:'Analytics',exact:true}).click();
 await p.getByLabel('Report',{exact:true}).selectOption('g08');
 // V2 §2.3: the real verifier's UNRESOLVED (no canonical receipts) reaches the report unchanged.
 await until(async()=> (await p.locator('.g-line',{hasText:'Native verdict'}).first().innerText()).includes('UNRESOLVED'),'G0.8 native UNRESOLVED in the report');
 await expectSections('analytics');
 assert((await body()).includes('g06a.local-simulation.import.v1'),'evidence record list');
 await p.getByRole('tab',{name:'Monitoring',exact:true}).click();
 await until(async()=> (await body()).includes('BSC_INGRESS_GETH'),'monitoring future sources');
 await expectSections('monitoring');
 const g08Line=await p.locator('.g-line',{hasText:'G0.8 verdict'}).first().innerText();
 assert(g08Line.includes('UNRESOLVED')&&g08Line.includes('CHAIN_UNKNOWN · EVIDENCE_UNKNOWN'),'G0.8 UNRESOLVED with both dimensions UNKNOWN: '+g08Line);
 const g08Section=p.getByRole('heading',{name:'5. '+sections.monitoring[4],exact:true}).locator('xpath=ancestor::section[1]');
 const g08Text=await g08Section.innerText();
 assert(g08Text.includes('Record · g08.mainnet-acquisition-unresolved.v2')&&g08Text.includes('SENT_TRANSACTION_UNRESOLVED'),'native UNRESOLVED record in its G0.8 section 5');
 for(const claim of ['CHAIN_PASS','CHAIN_ADAPT','EVIDENCE_COMPLETE'])assert(!(await body()).includes(claim),'no acquisition claim: '+claim);
 assert((await body()).includes('FUTURE_PHASE_BLOCKED'));assert((await body()).includes('Record · g06a.builder-floor-policy.v2'));
 // Order V2 §5: the five sources from their native owner; real requests through gateway → darkbotctl → bsc_runtime.
 const srcLine=id=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:new RegExp('^'+id+'$')})}).first();
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('0'),'native source owner generation 0');
 for(const id of ['BSC_INGRESS_GETH','BSC_INGRESS_PUBLICNODE','BSC_INGRESS_DRPC','BSC_INGRESS_BLOCKREQ','BSC_INGRESS_48CLUB'])assert((await srcLine(id).innerText()).includes('OFF'),id+' OFF');
 const requestSource=async(id,mode,expect)=>{
  await p.getByTestId(`bsc-source-${id}-${mode}`).click();
  await p.getByRole('alertdialog').getByRole('button',{name:'Anfordern',exact:true}).click();
  await until(async()=> (await p.getByTestId('bsc-source-receipt').innerText()).includes(expect),`${id} ${mode} receipt ${expect}`);
 };
 await requestSource('BSC_INGRESS_PUBLICNODE','SHADOW','outcome=APPLIED source=BSC_INGRESS_PUBLICNODE requested=SHADOW effective=SHADOW');
 await until(async()=> (await srcLine('BSC_INGRESS_PUBLICNODE').innerText()).includes('SHADOW'),'PUBLICNODE effective SHADOW pushed');
 await requestSource('BSC_INGRESS_DRPC','LIVE','effective=BLOCKED block_reason=ECONOMIC_LIVE_NOT_AUTHORIZED');
 await until(async()=> {const t=await srcLine('BSC_INGRESS_DRPC').innerText();return t.includes('BLOCKED')&&t.includes('requested LIVE')&&t.includes('ECONOMIC_LIVE_NOT_AUTHORIZED');},'DRPC LIVE stays BLOCKED');
 await requestSource('BSC_INGRESS_48CLUB','SHADOW','effective=BLOCKED block_reason=SOURCE_ACCESS_UNQUALIFIED');
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('3'),'generation 3 pushed');
 // A stale generation is the owner's refusal; the state does not move.
 const staleSource=await api.post(base+"/api/v1/command",{headers,data:Buffer.from(toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project:2,requestId:crypto.randomUUID(),action:"bsc.source.mode.set",resourceId:'BSC_INGRESS_GETH:SHADOW',expected:{version:{case:'configVersion',value:0n}},issuedAtUnixNs:BigInt(Date.now())*1_000_000n})))});
 const staleR=fromBinary(proto.CommandResponseSchema,new Uint8Array(await staleSource.body()));assert.equal(staleR.phase,proto.CommandPhase.FAILED,'stale request fails');
 assert((await srcLine('BSC_INGRESS_GETH').innerText()).includes('OFF'),'stale request changed nothing');
 for(const bad of ['BSC_INGRESS_GETH:ARMED','BSC_INGRESS_X:SHADOW','BSC_INGRESS_GETH']){const r=await api.post(base+'/api/v1/command',{headers,data:Buffer.from(toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project:2,requestId:crypto.randomUUID(),action:'bsc.source.mode.set',resourceId:bad,expected:{version:{case:'configVersion',value:3n}},issuedAtUnixNs:BigInt(Date.now())*1_000_000n})))});assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await r.body())).status,proto.CommandStatus.REJECTED,bad);}
 await p.screenshot({path:`${dir}/bsc-monitoring.png`,fullPage:true});
 await p.screenshot({path:`${dir}/bsc-analytics.png`,fullPage:true});

 // A bounded fixture child is used for explicit cancellation and crash tests only.
 const fixture=`#!/usr/bin/python3\nimport pathlib,signal,time,sys\nmode=pathlib.Path(${JSON.stringify(dir+'/mode')}).read_text().strip()\nif mode=='crash': sys.exit(19)\ndef stop(a,b):\n time.sleep(3);sys.exit(0)\nsignal.signal(signal.SIGTERM,stop)\nwhile True: time.sleep(.1)\n`;
 writeFileSync(`${dir}/app/releases/test/g07_pre_live_readiness`,fixture);chmodSync(`${dir}/app/releases/test/g07_pre_live_readiness`,0o755);writeFileSync(`${dir}/mode`,'hold');
 // Lost acknowledgement: the gateway executes the start, the page never sees the answer.
 await p.getByRole('tab',{name:'Launch / BUY',exact:true}).click();
 await until(async()=>!(await p.getByTestId('bsc-readiness-control').isDisabled()),'BSC launch control');
 const beforeLost=(await state()).run_id;let posts=0;
 await p.route('**/api/v1/command',async route=>{if(route.request().method()!=='POST')return route.continue();posts++;await route.fetch();await route.abort('connectionreset');});
 mark=pageRequests.length;
 await p.getByTestId('bsc-readiness-control').click();
 await until(async()=> (await state()).state==='RUNNING','lost-ACK start executed once');
 await until(async()=>statusGets(mark,'/api/v1/command')===1,'one targeted lookup');
 await until(async()=> (await p.getByTestId('bsc-readiness-control').innerText()).includes('Stop'),'control unlocked by the pushed job');
 await idle(4000);await p.unroute('**/api/v1/command');
 assert.equal(posts,1);assert.equal(pageRequests.slice(mark).filter(r=>r.method==='POST'&&r.path==='/api/v1/command').length,1);
 assert.equal(statusGets(mark,'/api/v1/command'),1);assert.notEqual((await state()).run_id,beforeLost);
 await p.getByTestId('bsc-readiness-control').click();await p.getByRole('alertdialog').getByRole('button',{name:'Prüfung stoppen',exact:true}).click();
 await until(async()=> (await state()).state==='STOPPED','stop after lost-ACK test');
 // Source detach and reconnect: the reporter dies, the page shows DETACHED and locks the control; launchd restarts it.
 execFileSync('launchctl',['kill','SIGKILL',`gui/${process.getuid()}/com.darkbot.test.${dir.split('.').pop()}.bsc.reporter`]);
 await until(async()=> (await p.locator('.g-line',{hasText:'Plan source quality'}).first().innerText()).includes('DETACHED'),'reporter loss shows SOURCE_DETACHED');
 assert.equal(await p.getByTestId('bsc-readiness-control').isDisabled(),true,'no control on a detached source');
 await until(async()=> (await p.locator('.g-line',{hasText:'Plan source quality'}).first().innerText()).includes('LIVE'),'reporter restarted and reattached');
 await until(async()=>!(await p.getByTestId('bsc-readiness-control').isDisabled()),'control back after reattach');
 await p.goto(base+'/');await until(async()=>!(await p.getByTestId('project-startstop-bsc').isDisabled()),'start button');
 await p.getByTestId('project-startstop-bsc').click();await until(async()=> (await state()).state==='RUNNING','held job');
 const running=await state();const id=crypto.randomUUID();const data=encode('bsc.readiness.start',id,running.run_id);
 const answers=await Promise.all([api.post(base+'/api/v1/command',{headers,data}),api.post(base+'/api/v1/command',{headers,data})]);
 const receipts=await Promise.all(answers.map(async a=>fromBinary(proto.CommandResponseSchema,new Uint8Array(await a.body()))));assert.equal(receipts[0].commandId,receipts[1].commandId);assert.equal(receipts[0].status,1);assert.equal((await state()).pid,running.pid);
 const stale=await api.post(base+'/api/v1/command',{headers,data:encode('bsc.readiness.stop',crypto.randomUUID(),crypto.randomUUID())});assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await stale.body())).rejectReason,2);
 await until(async()=> (await p.getByTestId('project-startstop-bsc').innerText()).includes('Stop'),'stop available');
 await p.getByTestId('project-startstop-bsc').click();await p.getByRole('alertdialog').getByRole('button',{name:'Abbrechen',exact:true}).click();assert.equal((await state()).state,'RUNNING');
 await p.getByTestId('project-startstop-bsc').click();await p.getByRole('alertdialog').getByRole('button',{name:'Prüfung stoppen',exact:true}).click();
 await until(async()=> (await state()).state==='STOPPING','actual STOPPING');
 assert.equal(await p.getByRole('alertdialog').count(),0);
 await until(async()=> (await state()).state==='STOPPED','orderly stop');assert.equal((await api.get(base+'/healthz')).status(),200);
 writeFileSync(`${dir}/mode`,'crash');await until(async()=> (await p.getByTestId('project-startstop-bsc').innerText()).includes('Run readiness'),'restart button');
 await p.getByTestId('project-startstop-bsc').click();await until(async()=> (await state()).state==='FAILED','crash FAILED');
 await until(async()=> (await p.getByTestId('project-process-bsc').innerText()).includes('FAILED'),'UI FAILED');
 await p.getByTestId('project-logs-bsc').click();await p.getByRole('heading',{name:'bsc-readiness',exact:true}).waitFor();assert((await p.getByRole('dialog').innerText()).includes('FAILED'));await p.getByRole('button',{name:'Close logs',exact:true}).click();
 writeFileSync(`${dir}/mode`,'hold');await p.getByTestId('project-startstop-bsc').click();await until(async()=> (await state()).state==='RUNNING','run for Stop All');
 await p.getByTestId('overview-stop-all').click();await p.getByRole('alertdialog').getByRole('button',{name:'Aufzeichnungen stoppen',exact:true}).click();await until(async()=> (await state()).state==='STOPPED','Stop All BSC');
 assert.equal((await api.get(base+'/healthz')).status(),200);assert.deepEqual(errors,[]);
 // Narrow screen: every tab fits the viewport, nothing widens the page.
 await p.setViewportSize({width:390,height:844});await p.goto(base+'/');await p.getByTestId('project-link-bsc').click();
 await until(async()=> (await body()).includes('isolated-fixture'),'mobile BSC feed');
 assert.match(await p.locator('.g-line',{hasText:'Snapshot age'}).first().innerText(),/\d+ s/,'snapshot age shown');
 for(const [tab,panel] of [['Overview','overview'],['Launch / BUY','launch'],['Monitoring','monitoring'],['Analytics','analytics'],['Wallets','wallets']]){
  await p.getByRole('tab',{name:tab,exact:true}).click();await p.locator('h2.g-section-title').first().waitFor();await expectSections(panel);
  const [scroll,width,wide]=await p.evaluate(()=>[document.documentElement.scrollWidth,window.innerWidth,[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>window.innerWidth+1&&![...e.children].some(c=>c.getBoundingClientRect().right>window.innerWidth+1)).slice(0,5).map(e=>`${e.tagName}.${e.className}: ${(e.textContent||'').slice(0,60)}`)]);
  assert(scroll<=width+1,`${tab} overflows on mobile: ${scroll} > ${width} · ${wide.join(' | ')}`);
  await p.screenshot({path:`${dir}/bsc-mobile-${panel}.png`,fullPage:true});
 }
 writeFileSync(`${dir}/bsc-test.json`,JSON.stringify({passed:true,errors},null,2));assert.equal(pageRequests.filter(r=>r.path==='/api/v1/bsc/process').length,0,'BSC process status is never fetched by the page');
 writeFileSync(`${dir}/bsc-requests.json`,JSON.stringify({p0PollsInIdleWindow:p0Polls,requests:pageRequests},null,1));
 console.log(`PASS native verdict ${verdict}, source owner OFF/SHADOW/LIVE→BLOCKED/stale/invalid via gateway→darkbotctl→bsc_runtime→reporter→Monitoring, real G0.8 UNRESOLVED (no canonical receipts) on Overview/Monitoring/Analytics, reporter→gateway→frontend push, zero BSC status/command polling (Project 0 capture watcher still polls: ${p0Polls} GETs in 8 s, separate owner gate), lost ACK → one lookup and one execution, §13 section order on all five tabs (desktop and 390 px without overflow), native records on screen without salt, source detach/reattach with locked control, snapshot age, UNKNOWN runtime, snapshots, no send, CSRF, allowlist, idempotency, launch CAS, cancel, STOPPING/STOPPED, crash FAILED, logs and Stop All`);
}finally{await browser.close();}
