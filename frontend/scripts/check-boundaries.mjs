// No extra dependencies required. Run npm run lint:boundaries.
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../src");
function* files(dir){for(const e of readdirSync(dir,{withFileTypes:true})){const p=path.join(dir,e.name);if(e.isDirectory())yield* files(p);else if(/\.(ts|tsx|js|jsx)$/.test(e.name))yield p;}}
const rel=p=>path.relative(root,p).split(path.sep).join("/");
const targets=(txt)=>{const r=[];
 for(const re of [/\b(?:import|export)\s+(?:[^;"']*?\s+from\s*)?["']([^"']+)["']/g,/\bimport\s*\(\s*["']([^"']+)["']\s*\)/g,/\brequire\s*\(\s*["']([^"']+)["']\s*\)/g])for(const m of txt.matchAll(re))r.push(m[1]);return [...new Set(r)];};
const errors=[];
for(const file of files(root)){const origin=rel(file),feature=origin.match(/^features\/([^/]+)\//)?.[1],adapter=/^features\/[^/]+\/api\/[^/]+Adapter\.ts$/.test(origin);
for(const name of targets(readFileSync(file,"utf8"))){const abs=name.startsWith("@/")?path.resolve(root,name.slice(2)):name.startsWith(".")?path.resolve(path.dirname(file),name):null;if(!abs)continue;
const target=rel(abs),toFeature=target.match(/^features\/([^/]+)\//)?.[1];
if(origin.startsWith("core/")&&toFeature)errors.push(origin+": core imports "+name);
if(feature&&toFeature&&feature!==toFeature)errors.push(origin+": cross-feature import "+name);
if(origin.startsWith("features/overview/")&&toFeature)errors.push(origin+": overview imports "+name);
if(target.startsWith("core/generated/")&&!origin.startsWith("core/realtime/")&&!adapter)errors.push(origin+": generated types outside adapter "+name);
}}
if(errors.length){for(const e of errors)console.error(e);process.exitCode=1;}
else console.log("Frontend import boundaries: PASS");
