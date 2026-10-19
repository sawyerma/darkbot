import { chromium } from '../../frontend/node_modules/playwright/index.mjs';
import assert from 'node:assert/strict';
const [port,dir,mode]=process.argv.slice(2);
const browser=await chromium.launch({headless:true});
try {
 const page=await browser.newPage(); const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`http://127.0.0.1:${port}/project-0`);
 const health=page.getByRole('button',{name:/Health ·/});await health.click();
 await page.getByText('Until stopped',{exact:false}).first().waitFor();
 if(mode==='live'){
   await page.waitForFunction(()=>document.querySelector('.p0-badge')?.textContent.match(/Live · unverified|Degraded/),{},{timeout:18000});
   const row=page.getByText('Transactions · unique / success / failed / unknown',{exact:true}).locator('..');
   const first=await row.innerText();
   for(let i=0;i<20 && await row.innerText()===first;i++) await page.waitForTimeout(250);
   assert.notEqual(await row.innerText(),first);

 }else{
   await page.waitForFunction(()=>document.querySelector('.p0-badge')?.textContent.includes('HTTP 429'),{},{timeout:18000});
 }
 console.log('BROWSER',mode,await health.innerText());assert.deepEqual(errors,[]);
 await page.screenshot({path:`${dir}/health-${mode}.png`,fullPage:true});
}finally{await browser.close();}
