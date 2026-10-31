import {chromium} from '../../frontend/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,chmodSync,mkdirSync,symlinkSync} from 'node:fs';
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
// A wait longer than two seconds is named on stderr with the run's elapsed time, so a harness timeout shows where it stood.
const t0=Date.now();const elapsed=()=>((Date.now()-t0)/1000).toFixed(1)+' s';
const until=async(fn,msg)=>{const at=Date.now();for(let i=0;i<150;i++){if(await fn()){if(Date.now()-at>2000)process.stderr.write(`[${elapsed()}] waited ${((Date.now()-at)/1000).toFixed(1)} s: ${msg}\n`);return;}await new Promise(r=>setTimeout(r,200));}throw Error(msg);};
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
 {const venues=await p.getByRole('heading',{name:'2. '+sections.monitoring[1],exact:true}).locator('xpath=ancestor::section[1]').innerText();assert(venues.includes('BSC_VENUE_FLAP')&&venues.includes('BSC_VENUE_OPENFOUR')&&!venues.includes('FUTURE_PHASE_BLOCKED'),'native venue rows in Monitoring §2: '+venues.slice(0,300));}assert((await body()).includes('Record · g06a.builder-floor-policy.v2'));
 // Order V2 §5: the five sources from their native owner; real requests through gateway → darkbotctl → bsc_runtime.
 const srcLine=id=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:new RegExp('^'+id+'$')})}).first();
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('0'),'native source owner generation 0');
 for(const id of ['BSC_INGRESS_GETH','BSC_INGRESS_PUBLICNODE','BSC_INGRESS_DRPC','BSC_INGRESS_BLOCKREQ','BSC_INGRESS_48CLUB'])assert((await srcLine(id).innerText()).includes('OFF'),id+' OFF');
 const requestSource=async(id,mode,expect,receipt='bsc-source-receipt')=>{
  await p.getByTestId(`bsc-source-${id}-${mode}`).click();
  await p.getByRole('alertdialog').getByRole('button',{name:'Anfordern',exact:true}).click();
  await until(async()=> (await p.getByTestId(receipt).innerText()).includes(expect),`${id} ${mode} receipt ${expect}`);
 };
 await requestSource('BSC_INGRESS_PUBLICNODE','SHADOW','outcome=APPLIED source=BSC_INGRESS_PUBLICNODE requested=SHADOW effective=SHADOW');
 await until(async()=> (await srcLine('BSC_INGRESS_PUBLICNODE').innerText()).includes('SHADOW'),'PUBLICNODE effective SHADOW pushed');
 // The read-only runtime (launchd) connects the effective SHADOW source: real verified pending txs from PublicNode.
 await until(async()=> (await p.locator('.g-line',{hasText:'BSC runtime'}).first().innerText()).includes('RUNNING'),'BSC runtime RUNNING');
// Phase F: the segmented capture's storage health, from the same runtime status.
await until(async()=> /HEALTHY[\s\S]*sealed segments \d+ · durable to sequence/.test(await p.locator('.g-line',{hasText:'BSC capture'}).first().innerText()),'BSC capture HEALTHY');
 await until(async()=> {const t=await srcLine('BSC_INGRESS_PUBLICNODE').innerText();const n=Number((t.match(/full (\d+)/)||[])[1]);return t.includes('HEALTHY')&&n>0;},'PUBLICNODE HEALTHY with real verified full txs');
 assert(!(await srcLine('BSC_INGRESS_GETH').innerText()).includes('HEALTHY'),'GETH not connected (OFF)');
 // The canonical follower (free public RPC) follows real Mainnet blocks: Monitoring §6.
 await until(async()=> {const t=await p.locator('.g-line',{hasText:'Canonical follower'}).first().innerText();return t.includes('FOLLOWING')&&/head \d+/.test(t)&&/blocks [1-9]/.test(t);},'canonical follower FOLLOWING real blocks');
 {const t=await p.locator('.g-line',{hasText:'Canonical launches (G0.4 denominator)'}).first().innerText();assert(t.includes('Flap')&&/Four \d+ \(token derivable from the launch bytes \d+, Classic candidate of the two frozen pairs, not derivable \d+, actionable before the block \d+, no proven derivation \d+, derived ≠ canonical \d+; Four deployment (UNPROBED|MATCH at block \d+|DRIFT at block \d+: (?:[^()]|\([^()]*\))+|PROBE_FAILED: (?:[^()]|\([^()]*\))+)\) · reverted launch calls \d+/.test(t),'G0.4 denominator line with the Phase E Four count and the Four drift guard verdict: '+t);}
 await requestSource('BSC_INGRESS_DRPC','LIVE','effective=BLOCKED block_reason=ECONOMIC_LIVE_DISABLED');
 await until(async()=> {const t=await srcLine('BSC_INGRESS_DRPC').innerText();return t.includes('BLOCKED')&&t.includes('requested LIVE')&&t.includes('ECONOMIC_LIVE_DISABLED');},'DRPC LIVE stays BLOCKED (global LIVE switch off)');
 await requestSource('BSC_INGRESS_48CLUB','SHADOW','effective=BLOCKED block_reason=SOURCE_ACCESS_UNQUALIFIED');
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('3'),'generation 3 pushed');
 // A stale generation is the owner's refusal; the state does not move.
 const staleSource=await api.post(base+"/api/v1/command",{headers,data:Buffer.from(toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project:2,requestId:crypto.randomUUID(),action:"bsc.source.mode.set",resourceId:'BSC_INGRESS_GETH:SHADOW',expected:{version:{case:'configVersion',value:0n}},issuedAtUnixNs:BigInt(Date.now())*1_000_000n})))});
 const staleR=fromBinary(proto.CommandResponseSchema,new Uint8Array(await staleSource.body()));assert.equal(staleR.phase,proto.CommandPhase.FAILED,'stale request fails');
 assert((await srcLine('BSC_INGRESS_GETH').innerText()).includes('OFF'),'stale request changed nothing');
 for(const bad of ['BSC_INGRESS_GETH:ARMED','BSC_INGRESS_X:SHADOW','BSC_INGRESS_GETH']){const r=await api.post(base+'/api/v1/command',{headers,data:Buffer.from(toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project:2,requestId:crypto.randomUUID(),action:'bsc.source.mode.set',resourceId:bad,expected:{version:{case:'configVersion',value:3n}},issuedAtUnixNs:BigInt(Date.now())*1_000_000n})))});assert.equal(fromBinary(proto.CommandResponseSchema,new Uint8Array(await r.body())).status,proto.CommandStatus.REJECTED,bad);}
 // The C2 state component of the same owner (PHASENPLAN §10): SHADOW applies, LIVE stays BLOCKED.
 const compLine=()=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:/^BSC_STATE_C2$/})}).first();
 await requestSource('BSC_STATE_C2','SHADOW','source=BSC_STATE_C2 requested=SHADOW effective=SHADOW','bsc-component-receipt');
 // The CAS of the next request is the pushed generation 4 (the hint text names SHADOW too).
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('4')&&(await compLine().innerText()).includes('requested SHADOW'),'C2 component SHADOW pushed (generation 4)');
 // The runtime starts the C2 owner on the pushed generation: real canonical blocks anchor it
 // (no own account in READ_ONLY, so an anchored head is coherent).
 const stateLine=()=>p.locator('.g-line',{hasText:'C2 chain state'}).first();
 await until(async()=> {const t=await stateLine().innerText();return t.includes('COHERENT')&&!t.includes('NOT_COHERENT')&&/head \d+/.test(t)&&/retained blocks [1-9]/.test(t);},'C2 owner SHADOW: coherent on real canonical blocks');
 await until(async()=> /blocks [1-9]/.test(await p.locator('.g-line',{hasText:'C2 canonical and pending effects'}).first().innerText()),'C2 effects counted');
 // C3 (PHASENPLAN §11): BSC_EXEC SHADOW probes the live Flap deployment read-only. On the witnessed
 // deployment (v5.27.0, economics reconfirmed) it waits for C2's nonce fact of the PAPER sender (every
 // 20 blocks) and arms its fenced lanes; a live Portal past it (since 26.10. v5.27.2, not source-verified)
 // fails closed as DEPLOYMENT_DRIFT with the observed version and arms nothing.
 // Plan v9 §2.3: Flap launches reach C3 and Strategy C only while the Flap venue is SHADOW (Monitoring §2);
 // Four.meme has no path to admit to, so its SHADOW is BLOCKED with that reason.
 await until(async()=> (await srcLine('BSC_VENUE_FLAP').innerText()).includes('OFF'),'Flap venue OFF initially');
 await requestSource('BSC_VENUE_FLAP','SHADOW','source=BSC_VENUE_FLAP requested=SHADOW effective=SHADOW','bsc-venue-receipt');
 // The CAS of the next request is the pushed generation 5 (a request on the old one is refused stale).
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('5')&&(await srcLine('BSC_VENUE_FLAP').innerText()).includes('requested SHADOW'),'Flap venue SHADOW pushed (generation 5)');
 await requestSource('BSC_VENUE_FOUR_CLASSIC','SHADOW','source=BSC_VENUE_FOUR_CLASSIC requested=SHADOW effective=BLOCKED block_reason=FOUR_EXECUTION_PATH_MISSING','bsc-venue-receipt');
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('6')&&(await srcLine('BSC_VENUE_FOUR_CLASSIC').innerText()).includes('FOUR_EXECUTION_PATH_MISSING'),'venues pushed (generation 6)');
 assert((await srcLine('BSC_VENUE_FLAP').innerText()).includes('last set by '),'venue shows the request that set it');
 // Plan v9 §2.5: the global runtime mode (Launch / BUY §5), OFF initially, bounds the components:
 // PAPER lets the C3 path run to the PAPER sink (zero-balance PAPER identity, nothing sent).
 await p.getByRole('tab',{name:'Launch / BUY',exact:true}).click();
 await until(async()=> (await p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:/^Runtime mode$/})}).first().locator('.bsc-state').innerText()).trim()==='OFF','runtime mode OFF initially');
 await p.getByTestId('bsc-switch-BSC_RUNTIME_MODE-PAPER').click();
 await p.getByRole('alertdialog').getByRole('button',{name:'Anfordern',exact:true}).click();
 await until(async()=> (await p.getByTestId('bsc-runtime-mode-receipt').innerText()).includes('source=BSC_RUNTIME_MODE requested=PAPER effective=PAPER'),'runtime mode PAPER receipt');
 await until(async()=> await p.getByTestId('bsc-switch-BSC_RUNTIME_MODE-PAPER').isDisabled(),'runtime mode PAPER pushed');
 await p.getByRole('tab',{name:'Monitoring',exact:true}).click();
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('7'),'generation 7 pushed');
 const execLine=()=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:/^C3 PAPER path$/})}).first();
 const execComp=()=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:/^BSC_EXEC$/})}).first();
 await requestSource('BSC_EXEC','SHADOW','source=BSC_EXEC requested=SHADOW effective=SHADOW','bsc-component-receipt');
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('8')&&(await execComp().innerText()).includes('requested SHADOW'),'BSC_EXEC SHADOW pushed (generation 8)');
 let c3=null;
 for(let i=0;;i++){const t=await execLine().innerText();
  if(t.includes('ARMED')&&t.includes('v5.27.0')&&t.includes('PAPER sender 0x')){c3='ARMED v5.27.0';break;}
  const drift=/DEPLOYMENT_DRIFT[\s\S]*Portal (v5\.\d+\.\d+) probed at block \d+/.exec(t);
  if(drift&&drift[1]!=='v5.27.0'&&t.includes('PAPER sender 0x')){c3=`DEPLOYMENT_DRIFT live ${drift[1]}`;break;}
  if(/IDENTITY_REFUSED|PROBE_FAILED|TERMS_DRIFT|ECONOMICS_UNQUALIFIED|FUNDED|ARM_FAILED/.test(t)||i>=450)throw Error('C3 PAPER path neither ARMED nor DEPLOYMENT_DRIFT: '+t);await new Promise(r=>setTimeout(r,200));}
 const sinkLine=async()=> await p.locator('.g-line',{hasText:'C3 path to the PAPER sink'}).first().innerText();
 if(c3.startsWith('ARMED'))await until(async()=> (await sinkLine()).includes('lanes 64/64'),'C3 lanes armed (64/64), nothing sent');
 else await until(async()=> {const t=await sinkLine();return t.includes('lanes 0/0')&&t.includes('to the PAPER sink 0');},'C3 drift: no lane armed, nothing to the PAPER sink');
 await requestSource('BSC_EXEC','LIVE','effective=BLOCKED block_reason=ECONOMIC_LIVE_DISABLED','bsc-component-receipt');
 await until(async()=> {const t=await execComp().innerText();return t.includes('BLOCKED')&&t.includes('requested LIVE')&&t.includes('ECONOMIC_LIVE_DISABLED');},'BSC_EXEC LIVE stays BLOCKED');
 await until(async()=> (await execLine().innerText()).includes('OFF'),'C3 path off on LIVE→BLOCKED');
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('9'),'generation 9 pushed');
 // Phase H: Strategy C (BSC_STRATEGY_C) SHADOW keeps a PAPER ledger on the follower's real Flap events
 // with the snapshot's frozen gas evidence; LIVE stays BLOCKED and the strategy stops.
 const strategyLine=()=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:/^Strategy C$/})}).first();
 const strategyComp=()=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:/^BSC_STRATEGY_C$/})}).first();
 await requestSource('BSC_STRATEGY_C','SHADOW','source=BSC_STRATEGY_C requested=SHADOW effective=SHADOW','bsc-component-receipt');
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('10')&&(await strategyComp().innerText()).includes('requested SHADOW'),'BSC_STRATEGY_C SHADOW pushed (generation 10)');
 await until(async()=> {const t=await strategyLine().innerText();return t.includes('SHADOW')&&t.includes('following ')&&t.includes('PAPER input 10000000000000000 wei');},'Strategy C SHADOW with its configured limits');
 assert(/launches \d+ · passed \d+ · rejected \d+/.test(await p.locator('.g-line',{hasText:'Strategy C decisions'}).first().innerText()),'Strategy C decisions line');
 assert(/entered \d+ · cancelled \d+/.test(await p.locator('.g-line',{hasText:'Strategy C PAPER ledger'}).first().innerText()),'Strategy C PAPER ledger line');
 // Phase I: the strategy's decisions take the C3 PAPER path only when BSC_EXEC is ARMED; otherwise they are counted.
 assert(/decisions without an armed C3 path \d+/.test(await p.locator('.g-line',{hasText:'Strategy C PAPER ledger'}).first().innerText()),'Strategy C unarmed decisions counted');
 assert(/Strategy C intents \d+ · at the PAPER sink \d+/.test(await p.locator('.g-line',{hasText:'Strategy C PAPER path'}).first().innerText()),'Strategy C PAPER path line under C3');
 await requestSource('BSC_STRATEGY_C','LIVE','effective=BLOCKED block_reason=ECONOMIC_LIVE_DISABLED','bsc-component-receipt');
 await until(async()=> {const t=await strategyComp().innerText();return t.includes('BLOCKED')&&t.includes('requested LIVE')&&t.includes('ECONOMIC_LIVE_DISABLED');},'BSC_STRATEGY_C LIVE stays BLOCKED');
 await until(async()=> (await strategyLine().innerText()).includes('NOT_RUNNING'),'Strategy C stopped on LIVE→BLOCKED');
 await until(async()=> (await p.locator('.g-line',{hasText:'Source modes generation'}).first().innerText()).includes('11'),'generation 11 pushed');
 await requestSource('BSC_STATE_C2','LIVE','effective=BLOCKED block_reason=ECONOMIC_LIVE_DISABLED','bsc-component-receipt');
 await until(async()=> {const t=await compLine().innerText();return t.includes('BLOCKED')&&t.includes('requested LIVE')&&t.includes('ECONOMIC_LIVE_DISABLED');},'C2 LIVE stays BLOCKED (the LIVE gate)');
 // BLOCKED is not SHADOW: the owner stops (its window checkpointed for the next start).
 await until(async()=> (await stateLine().innerText()).includes('NOT_RUNNING'),'C2 owner stopped on LIVE→BLOCKED');
 await p.screenshot({path:`${dir}/bsc-monitoring.png`,fullPage:true});
 // Plan 27.10.: the two independent native wallet switches (Wallets) and the economic switches (Launch / BUY),
 // DISABLED initially; enabled for this test and disabled again. The native check every Mainnet path calls
 // (`bsc_runtime control require`) reads this isolated owner state explicitly (`--state`); the runners
 // themselves read only the account's state, and a HOME pointing at this one is refused (below).
 mkdirSync(`${dir}/home/.darkbot`,{recursive:true});symlinkSync(`${dir}/run`,`${dir}/home/.darkbot/run`);
 const check=(args,env)=>{try{return {code:0,out:execFileSync(`${dir}/app/current/bsc_runtime`,['control','require',...args],{env,encoding:'utf8'})};}catch(e){return {code:e.status,out:String(e.stdout)};}};
 const requireSwitches=(...ids)=>check(['--state',`${dir}/run/bsc-sources`,...ids],{PATH:'/usr/bin:/bin'});
 const switchLine=label=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:new RegExp('^'+label+'$')})}).first();
 // The effective state badge itself (the hint text names DISABLED in any case).
 const switchState=async label=>(await switchLine(label).locator('.bsc-state').innerText()).trim();
 const gen=async()=>BigInt((await switchLine('Launcher wallet switch generation').innerText()).match(/(\d+)/)[1]);
 const requestSwitch=async(id,mode,receipt,expect)=>{
  await p.getByTestId(`bsc-switch-${id}-${mode}`).click();
  await p.getByRole('alertdialog').getByRole('button',{name:'Anfordern',exact:true}).click();
  await until(async()=> (await p.getByTestId(receipt).innerText()).includes(expect),`${id} ${mode} receipt ${expect}`);
  // The pushed state shows the request (its button locks): the next request's CAS is that generation.
  await until(async()=> await p.getByTestId(`bsc-switch-${id}-${mode}`).isDisabled(),`${id} ${mode} pushed`);
 };
 await p.getByRole('tab',{name:'Wallets',exact:true}).click();
 await until(async()=> (await switchState('Launcher wallet switch'))==='DISABLED'&&(await switchState('Buyer wallet switch'))==='DISABLED','both wallet switches DISABLED initially');
 let r=requireSwitches('BSC_WALLET_LAUNCHER');assert(r.code===2&&r.out.includes('BSC_WALLET_LAUNCHER DISABLED'),'terminal runner refused while DISABLED: '+JSON.stringify(r));
 await requestSwitch('BSC_WALLET_LAUNCHER','ENABLED','bsc-wallet-launcher-receipt','outcome=APPLIED source=BSC_WALLET_LAUNCHER requested=ENABLED effective=ENABLED');
 await until(async()=> (await switchState('Launcher wallet switch'))==='ENABLED','Launcher ENABLED pushed');
 assert.equal(await switchState('Buyer wallet switch'),'DISABLED','Buyer independent: still DISABLED');
 r=requireSwitches('BSC_WALLET_LAUNCHER');assert(r.code===0&&r.out.includes('controls=ENABLED'),'terminal runner sees the GUI ENABLE: '+JSON.stringify(r));
 r=requireSwitches('BSC_WALLET_LAUNCHER','BSC_WALLET_BUYER');assert(r.code===2&&r.out.includes('BSC_WALLET_BUYER DISABLED')&&!r.out.includes('BSC_WALLET_LAUNCHER DISABLED'),'buyer still refused: '+JSON.stringify(r));
 // Review 27.10. M1: a runner whose HOME points at this (ENABLED) state still refuses: only the account's state counts.
 r=check(['BSC_WALLET_LAUNCHER'],{PATH:'/usr/bin:/bin',HOME:`${dir}/home`});assert(r.code===2&&r.out.includes('HOME is not the account home'),'foreign HOME refused: '+JSON.stringify(r));
 // Terminal → GUI: the same owner request from darkbotctl reaches the Wallets tab with its request id.
 execFileSync(`${root}/ops/darkbotctl`,['bsc','source','request','BSC_WALLET_BUYER','ENABLED',String(await gen()),'e2e-cli-buyer'],{env:process.env,encoding:'utf8'});
 await until(async()=> (await switchState('Buyer wallet switch'))==='ENABLED','CLI ENABLE of the buyer pushed');
 assert((await switchLine('Buyer wallet switch generation').innerText()).includes('e2e-cli-buyer'),'the CLI request is the receipt shown');
 // A stale CAS and a wrong version kind change nothing.
 const switchPost=async(action,resourceId,version)=>fromBinary(proto.CommandResponseSchema,new Uint8Array(await (await api.post(base+'/api/v1/command',{headers,data:Buffer.from(toBinary(proto.CommandRequestSchema,create(proto.CommandRequestSchema,{project:2,requestId:crypto.randomUUID(),action,resourceId,expected:{version},issuedAtUnixNs:BigInt(Date.now())*1_000_000n})))})).body()));
 assert.equal((await switchPost('bsc.wallet.set','BSC_WALLET_BUYER:DISABLED',{case:'walletPolicyVersion',value:0n})).phase,proto.CommandPhase.FAILED,'stale wallet CAS fails');
 const wrongKind=await switchPost('bsc.wallet.set','BSC_WALLET_BUYER:DISABLED',{case:'configVersion',value:await gen()});
 assert.equal(wrongKind.status,2);assert.equal(wrongKind.rejectReason,5);
 assert.equal((await switchPost('bsc.wallet.set','BSC_ECON_SEND:ENABLED',{case:'walletPolicyVersion',value:await gen()})).rejectReason,5,'a wallet action takes no economic switch');
 assert.equal(await switchState('Buyer wallet switch'),'ENABLED','refused requests changed nothing');
 await requestSwitch('BSC_WALLET_BUYER','DISABLED','bsc-wallet-buyer-receipt','requested=DISABLED effective=DISABLED');
 await requestSwitch('BSC_WALLET_LAUNCHER','DISABLED','bsc-wallet-launcher-receipt','requested=DISABLED effective=DISABLED');
 await until(async()=> (await switchState('Launcher wallet switch'))==='DISABLED'&&(await switchState('Buyer wallet switch'))==='DISABLED','both wallets DISABLED again');
 r=requireSwitches('BSC_WALLET_LAUNCHER');assert.equal(r.code,2,'terminal runner refused again after DISABLE');
 await p.screenshot({path:`${dir}/bsc-wallet-switches.png`,fullPage:true});
 // The economic switches on Launch / BUY: SEND and the global LIVE switch, enabled and disabled again.
 await p.getByRole('tab',{name:'Launch / BUY',exact:true}).click();
 await until(async()=> (await switchState('SEND switch'))==='DISABLED','SEND DISABLED initially');
 await requestSwitch('BSC_ECON_SEND','ENABLED','bsc-economic-attempt-receipt','source=BSC_ECON_SEND requested=ENABLED effective=ENABLED');
 r=requireSwitches('BSC_ECON_SEND');assert.equal(r.code,0,'SEND ENABLED reaches the runner check');
 r=requireSwitches('BSC_ECON_SEND','BSC_ECON_SIGN','BSC_WALLET_BUYER');assert(r.code===2&&r.out.includes('BSC_ECON_SIGN DISABLED'),'one switch never enables another');
 await requestSwitch('BSC_ECON_LIVE','ENABLED','bsc-economic-authorization-receipt','source=BSC_ECON_LIVE requested=ENABLED effective=ENABLED');
 await p.getByRole('tab',{name:'Monitoring',exact:true}).click();
 await until(async()=> {const t=await srcLine('BSC_INGRESS_DRPC').innerText();return t.includes('BLOCKED')&&t.includes('LIVE_GRANT_MISSING');},'global LIVE ENABLED: DRPC LIVE still BLOCKED, next missing condition');
 await p.getByRole('tab',{name:'Launch / BUY',exact:true}).click();
 await requestSwitch('BSC_ECON_LIVE','DISABLED','bsc-economic-authorization-receipt','source=BSC_ECON_LIVE requested=DISABLED effective=DISABLED');
 await requestSwitch('BSC_ECON_SEND','DISABLED','bsc-economic-attempt-receipt','source=BSC_ECON_SEND requested=DISABLED effective=DISABLED');
 r=requireSwitches('BSC_ECON_SEND');assert.equal(r.code,2,'SEND disabled again');
 // Plan 27.10. (8b4721bb): the persisted daily BUY attempt limit (UTC day), 0 initially: set for this
 // test, shown with today's use, invalid input refused natively and by the gateway, then 0 again.
 const limitLine=()=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:/^BUY attempts per UTC day$/})}).first();
 await until(async()=> /^0\b/.test(await limitLine().locator('.g-right').innerText())&&(await limitLine().innerText()).includes('0 of 0 used on'),'daily BUY limit 0 initially');
 const setLimit=async(n)=>{
  await p.getByTestId('bsc-buy-limit-input').fill(String(n));
  await p.getByTestId('bsc-buy-limit-set').click();
  await p.getByRole('alertdialog').getByRole('button',{name:'Anfordern',exact:true}).click();
  await until(async()=> (await p.getByTestId('bsc-buy-limit-receipt').innerText()).includes(`source=BSC_BUY_ATTEMPTS_PER_DAY requested=${n} effective=${n}`),`limit ${n} receipt`);
  await until(async()=> (await limitLine().innerText()).includes(`0 of ${n} used on`),`limit ${n} pushed`);
 };
 await setLimit(3);
 assert((await limitLine().innerText()).includes('3 remaining'),'remaining budget shown');
 const badLimit=await switchPost('bsc.attempt.limit.set','BSC_BUY_ATTEMPTS_PER_DAY:-1',{case:'configVersion',value:0n});
 assert.equal(badLimit.status,2);assert.equal(badLimit.rejectReason,5);
 await setLimit(0);
 // Plan 28.10.: the LIVE tab. Activation through the GUI up to an effective LIVE (the synthetic, verified
 // qualification of this isolated snapshot), the grant of a confirmed scope whose SHA-256 is the native
 // owner's, then deactivation and stop. This runtime has no LIVE path (no BSC_LIVE_* keys, no key):
 // nothing can be signed or sent.
 process.stderr.write(`[${elapsed()}] LIVE tab\n`);
 const liveLine=label=>p.locator('.g-line').filter({has:p.locator('span:first-child',{hasText:new RegExp('^'+label+'$')})}).first();
 const unmet=async()=>await p.getByTestId('bsc-live-conditions').locator('tr[data-met="false"]').count();
 const confirm=async()=>await p.getByRole('alertdialog').getByRole('button',{name:'Anfordern',exact:true}).click();
 await p.getByRole('tab',{name:'LIVE',exact:true}).click();
 await p.getByRole('heading',{name:'9. '+sections.live[8],exact:true}).waitFor();
 await expectSections('live');
 await until(async()=> (await liveLine('LIVE gate').innerText()).includes('BLOCKED'),'LIVE gate BLOCKED initially');
 await until(async()=> (await liveLine('LIVE qualification').innerText()).includes('VERIFIED'),'synthetic qualification VERIFIED');
 await until(async()=> (await unmet())>5,'the checklist names what is missing');
 await until(async()=> (await liveLine('LIVE path').innerText()).includes('NOT_CONFIGURED'),'no LIVE path in this runtime');
 await p.getByRole('tab',{name:'Wallets',exact:true}).click();
 await requestSwitch('BSC_WALLET_LAUNCHER','ENABLED','bsc-wallet-launcher-receipt','requested=ENABLED effective=ENABLED');
 await requestSwitch('BSC_WALLET_BUYER','ENABLED','bsc-wallet-buyer-receipt','requested=ENABLED effective=ENABLED');
 await p.getByRole('tab',{name:'Launch / BUY',exact:true}).click();
 for(const [id,receipt] of [['BSC_ECON_SIGN','bsc-economic-authorization-receipt'],['BSC_ECON_ARM','bsc-economic-authorization-receipt'],['BSC_ECON_SEND','bsc-economic-attempt-receipt'],['BSC_ECON_ATTEMPT','bsc-economic-attempt-receipt']])
  await requestSwitch(id,'ENABLED',receipt,`source=${id} requested=ENABLED effective=ENABLED`);
 await requestSwitch('BSC_RUNTIME_MODE','LIVE','bsc-runtime-mode-receipt','source=BSC_RUNTIME_MODE requested=LIVE');
 await p.getByRole('tab',{name:'LIVE',exact:true}).click();
 await requestSwitch('BSC_ECON_LIVE','ENABLED','bsc-live-econ-receipt','source=BSC_ECON_LIVE requested=ENABLED effective=ENABLED');
 await p.getByTestId('bsc-buy-limit-input').fill('1');await p.getByTestId('bsc-buy-limit-set').click();await confirm();
 await until(async()=> (await p.getByTestId('bsc-buy-limit-receipt').innerText()).includes('outcome=APPLIED source=BSC_BUY_ATTEMPTS_PER_DAY requested=1 effective=1'),'daily limit 1 from the LIVE tab');
 // Every next request takes the pushed generation as its CAS: wait for the push first.
 await until(async()=> (await liveLine('Daily BUY attempts').innerText()).includes('0 of 1 used on'),'daily limit 1 pushed to the LIVE tab');
 await p.getByTestId('bsc-live-total-input').fill('1');await p.getByTestId('bsc-live-total-set').click();await confirm();
 await until(async()=> (await p.getByTestId('bsc-live-total-receipt').innerText()).includes('outcome=APPLIED source=BSC_BUY_ATTEMPTS_TOTAL requested=1'),'campaign limit 1');
 await until(async()=> (await liveLine('Campaign BUY attempts').innerText()).includes('0 of 1'),'campaign limit pushed');
 // The grant: the dialog shows the exact canonical scope and its digest; the native owner computes the same.
 await p.getByTestId('bsc-live-grant-max-input').fill('2000000000000000');
 await p.getByTestId('bsc-live-grant-max-spend').fill('50000000000000000');
 await p.getByTestId('bsc-live-grant-attempts').fill('1');
 await p.getByTestId('bsc-live-grant-hours').fill('1');
 // BSC3-069: the execution risk, exactly the configured executor's.
 for(const [k,v] of [['buy-slippage','100'],['exit-slippage','100'],['gas-price','50000000'],['buy-gas','3000000'],['approve-gas','100000'],['sell-gas','1500000']])await p.getByTestId(`bsc-live-grant-${k}`).fill(v);
 await p.getByTestId('bsc-live-grant-review').click();
 await p.getByTestId('bsc-live-grant-dialog').waitFor();
 const scope=await p.getByTestId('bsc-live-grant-scope').innerText();const sha=(await p.getByTestId('bsc-live-grant-sha').innerText()).trim();
 const field=k=>scope.match(new RegExp('^'+k+'=(.*)$','m'))[1];
 const native=execFileSync(`${dir}/app/current/bsc_runtime`,['live','scope','--venue',field('venue'),'--strategies',field('strategies'),'--sources',field('sources'),'--adapters',field('adapters'),'--max-input-wei',field('max_input_wei'),'--max-spend-wei',field('max_spend_wei'),'--max-attempts',field('max_attempts'),'--expires-unix-ms',field('expires_unix_ms'),'--max-buy-slippage-bps',field('max_buy_slippage_bps'),'--max-exit-slippage-bps',field('max_exit_slippage_bps'),'--max-gas-price-wei',field('max_gas_price_wei'),'--max-buy-gas-limit',field('max_buy_gas_limit'),'--max-approve-gas-limit',field('max_approve_gas_limit'),'--max-sell-gas-limit',field('max_sell_gas_limit')],{encoding:'utf8'});
 assert.equal(native.replace(/scope_sha256=[0-9a-f]{64}\nrisk_scope_sha256=[0-9a-f]{64}\n$/,''),scope.endsWith('\n')?scope:scope+'\n','the shown scope is the native canonical text');
 assert(native.includes('scope_sha256='+sha),'the confirmed digest is the native one');
 await p.getByTestId('bsc-live-grant-confirm').click();
 await until(async()=> (await p.getByTestId('bsc-live-grant-receipt').innerText()).includes('outcome=APPLIED'),'grant APPLIED');
 await until(async()=> (await liveLine('Grant scope SHA-256').innerText()).includes(sha),'grant pushed with its digest');
 // The scope: what the grant names and is not yet requested LIVE, from this tab.
 for(const id of ['BSC_VENUE_FLAP','BSC_INGRESS_PUBLICNODE']){
  await p.getByTestId(`bsc-live-scope-${id}`).click();await confirm();
  // The receipt names the owner's decision now: the gate is clear and the grant names it.
  await until(async()=> (await p.getByTestId('bsc-live-scope-receipt').innerText()).includes(`source=${id} requested=LIVE effective=LIVE generation=`),`${id} LIVE receipt effective LIVE`);
  await until(async()=> (await p.getByTestId(`bsc-live-scope-${id}`).count())===0,`${id} LIVE pushed`);
 }
 // Effective LIVE: every condition holds, every entity of the scope is LIVE now.
 await until(async()=> (await liveLine('LIVE gate').innerText()).includes('CLEAR'),'LIVE gate CLEAR through the GUI');
 assert.equal(await unmet(),0,'every condition met');
 {const rows=await p.getByTestId('bsc-live-scope').locator('tr').all();assert.equal(rows.length,5,'runtime, C2, BSC_EXEC, Flap, PublicNode');for(const row of rows)assert.equal((await row.locator('td').nth(2).innerText()).trim(),'LIVE','scope effective LIVE: '+(await row.innerText()));}
 {const out=execFileSync(`${dir}/app/current/bsc_runtime`,['live','status','--state',`${dir}/run/bsc-sources`,'--root',`${dir}/app/bsc-source`],{encoding:'utf8'});assert(out.includes('qualification=VERIFIED')&&out.includes('live=CLEAR'),'the CLI sees the same effective LIVE: '+out);}
 await p.screenshot({path:`${dir}/bsc-live-clear.png`,fullPage:true});
 // Deactivate: the grant is revoked; stop: the economic LIVE switch off.
 await p.getByTestId('bsc-live-revoke-request').click();await confirm();
 await until(async()=> (await p.getByTestId('bsc-live-revoke-receipt').innerText()).includes('outcome=APPLIED'),'revoke APPLIED');
 await until(async()=> {const t=await liveLine('LIVE gate').innerText();return t.includes('BLOCKED')&&t.includes('LIVE_GRANT_MISSING');},'revoked: BLOCKED LIVE_GRANT_MISSING');
 await requestSwitch('BSC_ECON_LIVE','DISABLED','bsc-live-econ-receipt','source=BSC_ECON_LIVE requested=DISABLED effective=DISABLED');
 await p.getByTestId('bsc-live-total-input').fill('0');await p.getByTestId('bsc-live-total-set').click();await confirm();
 await until(async()=> (await p.getByTestId('bsc-live-total-receipt').innerText()).includes('outcome=APPLIED source=BSC_BUY_ATTEMPTS_TOTAL requested=0'),'campaign limit 0 applied');
 await until(async()=> (await liveLine('Campaign BUY attempts').innerText()).includes('0 of 0'),'campaign limit 0 again');
 await p.screenshot({path:`${dir}/bsc-live.png`,fullPage:true});
 // Everything back as before: sources and venue SHADOW, runtime PAPER, every switch DISABLED, limit 0.
 await p.getByRole('tab',{name:'Monitoring',exact:true}).click();
 // Monitoring names the LIVE tab's decision: after the stop, BLOCKED with the gate's first reason.
 await until(async()=> {const t=await srcLine('BSC_INGRESS_PUBLICNODE').innerText();return t.includes('requested LIVE')&&t.includes('BLOCKED')&&t.includes('ECONOMIC_LIVE_DISABLED');},'Monitoring follows the LIVE gate');
 for(const [id,receipt] of [['BSC_INGRESS_PUBLICNODE','bsc-source-receipt'],['BSC_VENUE_FLAP','bsc-venue-receipt']]){
  await requestSource(id,'SHADOW',`source=${id} requested=SHADOW effective=SHADOW`,receipt);
  await until(async()=> (await srcLine(id).innerText()).includes('requested SHADOW'),`${id} SHADOW pushed`);
 }
 await p.getByRole('tab',{name:'Launch / BUY',exact:true}).click();
 await requestSwitch('BSC_RUNTIME_MODE','PAPER','bsc-runtime-mode-receipt','source=BSC_RUNTIME_MODE requested=PAPER');
 for(const [id,receipt] of [['BSC_ECON_SIGN','bsc-economic-authorization-receipt'],['BSC_ECON_ARM','bsc-economic-authorization-receipt'],['BSC_ECON_SEND','bsc-economic-attempt-receipt'],['BSC_ECON_ATTEMPT','bsc-economic-attempt-receipt']])
  await requestSwitch(id,'DISABLED',receipt,`source=${id} requested=DISABLED effective=DISABLED`);
 await setLimit(0);
 await p.getByRole('tab',{name:'Wallets',exact:true}).click();
 await requestSwitch('BSC_WALLET_LAUNCHER','DISABLED','bsc-wallet-launcher-receipt','requested=DISABLED effective=DISABLED');
 await requestSwitch('BSC_WALLET_BUYER','DISABLED','bsc-wallet-buyer-receipt','requested=DISABLED effective=DISABLED');
 process.stderr.write(`[${elapsed()}] LIVE tab restored\n`);
 await p.getByRole('tab',{name:'Overview',exact:true}).click();
 await until(async()=> (await switchLine('Wallet switches').innerText()).includes('Launcher wallet DISABLED · Buyer wallet DISABLED'),'Overview wallet summary');
 assert(!(await switchLine('Economic switches').innerText()).includes('ENABLED'),'every economic switch DISABLED at the end');
 await p.getByRole('tab',{name:'Monitoring',exact:true}).click();
 // The G0.4 census, replayed by the runtime from its journals: Analytics §2, no verdict.
 await p.getByRole('tab',{name:'Analytics',exact:true}).click();
 await until(async()=> (await p.locator('.g-line',{hasText:'criteria'}).filter({hasText:'CRITERIA_UNCONFIRMED'}).count())>0,'G0.4 census on Analytics, CRITERIA_UNCONFIRMED');
 assert((await p.locator('.g-line',{hasText:'flap_launches'}).count())>0,'census denominator row');
 // BSC-023: the C1.9 native latency report (boundary, clock, eligibility) on Analytics §3.
 assert((await p.locator('.g-line',{hasText:'c1_9_qualification_eligible'}).filter({hasText:'true'}).count())>0,'C1.9 report eligible on Analytics');
 assert((await p.locator('.g-line',{hasText:'clock.domain'}).count())>0,'C1.9 clock domain row');
 await p.screenshot({path:`${dir}/bsc-analytics.png`,fullPage:true});
 await p.getByRole('tab',{name:'Monitoring',exact:true}).click();

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
 await p.getByTestId('project-logs-bsc').click();await p.getByRole('heading',{name:'bsc-readiness',exact:true}).waitFor();await until(async()=> (await p.getByRole('dialog').innerText()).includes('FAILED'),'crash FAILED in the loaded log');await p.getByRole('button',{name:'Close logs',exact:true}).click();
 writeFileSync(`${dir}/mode`,'hold');await p.getByTestId('project-startstop-bsc').click();await until(async()=> (await state()).state==='RUNNING','run for Stop All');
 await p.getByTestId('overview-stop-all').click();await p.getByRole('alertdialog').getByRole('button',{name:'Aufzeichnungen stoppen',exact:true}).click();await until(async()=> (await state()).state==='STOPPED','Stop All BSC');
 assert.equal((await api.get(base+'/healthz')).status(),200);assert.deepEqual(errors,[]);
 // Narrow screen: every tab fits the viewport, nothing widens the page.
 await p.setViewportSize({width:390,height:844});await p.goto(base+'/');await p.getByTestId('project-link-bsc').click();
 await until(async()=> (await body()).includes('isolated-fixture'),'mobile BSC feed');
 assert.match(await p.locator('.g-line',{hasText:'Snapshot age'}).first().innerText(),/\d+ s/,'snapshot age shown');
 for(const [tab,panel] of [['Overview','overview'],['Launch / BUY','launch'],['Monitoring','monitoring'],['Analytics','analytics'],['Wallets','wallets'],['LIVE','live']]){
  await p.getByRole('tab',{name:tab,exact:true}).click();await p.locator('h2.g-section-title').first().waitFor();await expectSections(panel);
  const [scroll,width,wide]=await p.evaluate(()=>[document.documentElement.scrollWidth,window.innerWidth,[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>window.innerWidth+1&&![...e.children].some(c=>c.getBoundingClientRect().right>window.innerWidth+1)).slice(0,5).map(e=>`${e.tagName}.${e.className}: ${(e.textContent||'').slice(0,60)}`)]);
  assert(scroll<=width+1,`${tab} overflows on mobile: ${scroll} > ${width} · ${wide.join(' | ')}`);
  await p.screenshot({path:`${dir}/bsc-mobile-${panel}.png`,fullPage:true});
 }
 writeFileSync(`${dir}/bsc-test.json`,JSON.stringify({passed:true,errors},null,2));assert.equal(pageRequests.filter(r=>r.path==='/api/v1/bsc/process').length,0,'BSC process status is never fetched by the page');
 writeFileSync(`${dir}/bsc-requests.json`,JSON.stringify({p0PollsInIdleWindow:p0Polls,requests:pageRequests},null,1));
 console.log(`PASS native verdict ${verdict}, source owner OFF/SHADOW/LIVE→BLOCKED/stale/invalid via gateway→darkbotctl→bsc_runtime→reporter→Monitoring, venues (Flap SHADOW, Four BLOCKED), runtime mode PAPER, daily BUY limit 3→0 (invalid refused), wallet switches LAUNCHER/BUYER and economic SEND/LIVE ENABLE→DISABLE via GUI and CLI with the native check on the same state, a foreign HOME refused (stale CAS, wrong version kind refused), canonical follower FOLLOWING and G0.4 census (CRITERIA_UNCONFIRMED), C2 component SHADOW (owner coherent on real blocks)/LIVE→BLOCKED (owner stopped), C3 BSC_EXEC SHADOW (live probe: ${c3}${c3.startsWith('ARMED')?', C2 nonce fact, 64 fenced PAPER lanes':', fail-closed, no lane'})/LIVE→BLOCKED, Strategy C SHADOW (PAPER ledger on real Flap events)/LIVE→BLOCKED, C1.9 report on Analytics, real G0.8 UNRESOLVED (no canonical receipts) on Overview/Monitoring/Analytics, reporter→gateway→frontend push, zero BSC status/command polling (Project 0 capture watcher still polls: ${p0Polls} GETs in 8 s, separate owner gate), lost ACK → one lookup and one execution, §13 section order on all six tabs (desktop and 390 px without overflow), the LIVE tab: GUI activation to an effective LIVE (gate CLEAR, synthetic verified qualification, the CLI sees the same), grant of a confirmed scope whose SHA-256 is the native owner's, revoke → LIVE_GRANT_MISSING, stop, campaign limit, no LIVE path (nothing signed or sent), native records on screen without salt, source detach/reattach with locked control, snapshot age, UNKNOWN runtime, snapshots, no send, CSRF, allowlist, idempotency, launch CAS, cancel, STOPPING/STOPPED, crash FAILED, logs and Stop All`);
}finally{await browser.close();}
