import { chromium } from '../../frontend/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
const [port,dir]=process.argv.slice(2);const base=`http://127.0.0.1:${port}`;
const label=kind=>`com.darkbot.test.${dir.split('.').at(-1)}.${kind}`;
const pid=kind=>execFileSync('/bin/launchctl',['list',label(kind)],{encoding:'utf8'}).match(/"PID" = (\d+);/)?.[1];
const browser=await chromium.launch({headless:true});
const p=await browser.newPage();const errors=[];p.on('pageerror',e=>errors.push(e.message));p.on('dialog',d=>{errors.push('native dialog');void d.dismiss();});
const until=async(fn,msg)=>{for(let i=0;i<150;i++){if(await fn())return;await new Promise(r=>setTimeout(r,200));}throw Error(msg);};
const text=async id=>await p.getByTestId(id).innerText();
try{
 await p.goto(base+'/');
 await until(async()=>(await text('project-process-project0')).includes('STOPPED'),'P0 initial stopped');
 await until(async()=>!(await p.getByTestId('project-startstop-bsc').isDisabled()),'BSC controller available');
 // DEC-07: the raw capture starts only through its boot flow; Stop All covers what runs.
 assert.equal(await p.getByTestId('project-startstop-project0').isDisabled(),true);
 assert((await p.locator('.ov-disabled-action',{has:p.getByTestId('project-startstop-project0')}).getAttribute('title')).startsWith('BOOT_AUDIT_REQUIRED'));
 await p.getByTestId('project-startstop-bsc').click();
 await until(async()=>(await text('project-process-bsc')).includes('RUNNING'),'BSC readiness fixture running');
 const gateway=pid('gateway');assert(gateway);
 await p.getByTestId('overview-stop-all').click();
 await p.getByRole('alertdialog').getByRole('button',{name:'Aufzeichnungen stoppen',exact:true}).click();
 // BSC never confirms its stop: the stop stays open and is never shown as complete.
 await until(async()=>(await text('overview-system')).includes('BSC · PENDING'),'BSC stop stays open');
 await new Promise(r=>setTimeout(r,5000));
 const system=await text('overview-system');
 assert(system.includes('BSC · PENDING')&&!system.includes('BSC · COMPLETED'),'a partial stop is never shown as complete');
 assert((await text('project-process-bsc')).includes('STOPPING'));
 assert((await text('project-process-project0')).includes('STOPPED'));
 assert.equal(await p.getByTestId('project-startstop-bsc').isDisabled(),true,'an unconfirmed stop keeps the BSC control locked');
 assert.equal((await p.request.get(base+'/healthz')).status(),200);
 assert.equal(pid('gateway'),gateway,'the gateway keeps running');
 assert.deepEqual(errors,[]);
 writeFileSync(`${dir}/stop-all-partial.json`,JSON.stringify({passed:true,system},null,2));
 console.log('PASS Stop All with an unconfirmed BSC stop: BSC PENDING/STOPPING and locked, P0 Start gated (BOOT_AUDIT_REQUIRED) and STOPPED, gateway unchanged');
}finally{await browser.close();}
