import { chromium } from '../../frontend/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
const [port,dir,mode]=process.argv.slice(2);
const browser=await chromium.launch({headless:true});
try {
 const page=await browser.newPage(); const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`http://127.0.0.1:${port}/project-0`);
 // DEC-07: no capture runs; the raw capture's Check & start is the boot flow's.
 await page.getByRole('tablist',{name:'Project 0 areas'}).getByRole('tab',{name:'Capture',exact:true}).click();
 const start=page.getByRole('button',{name:'Check & start',exact:true});
 assert.equal(await start.isDisabled(),true);assert((await start.getAttribute('title')).startsWith('BOOT_AUDIT_REQUIRED'));
 console.log('BROWSER',mode,'Check & start gated');assert.deepEqual(errors,[]);
 await page.screenshot({path:`${dir}/capture-${mode}.png`,fullPage:true});
}finally{await browser.close();}
