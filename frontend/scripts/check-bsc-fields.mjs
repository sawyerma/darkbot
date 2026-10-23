import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import {fileURLToPath} from "node:url";
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),"../..");
const manifest=JSON.parse(fs.readFileSync(path.join(root,"frontend/contracts/bsc-fields.json")));
const sha=p=>crypto.createHash("sha256").update(fs.readFileSync(path.join(root,p))).digest("hex");
const proto=fs.readFileSync(path.join(root,manifest.schema),"utf8");
if(sha(manifest.schema)!==manifest.schema_sha256)throw Error("BSC schema changed: re-review F0-VAR");
const fields=new Map(manifest.fields.map(f=>[f.field,f]));
for(const [,message,body] of proto.matchAll(/message (\w+) \{([\s\S]*?)\n\}/g))for(const [,type,name] of body.matchAll(/^\s*(?:optional |repeated )?(\w+) (\w+) = \d+;/gm)){
 const key=message+"."+name;const f=fields.get(key);
 if(!f||f.type!==type||!f.owner||!f.unit||!f.required_gate||sha(f.source_path)!==f.source_sha)throw Error("Unqualified field "+key);
 fields.delete(key);
}
if(fields.size)throw Error("Stale manifest fields");
for(const binding of manifest.command_bindings)if(sha(binding.source_path)!==binding.source_sha)throw Error("Command source changed: "+binding.source_path);
const native=JSON.parse(fs.readFileSync(path.join(root,manifest.native_field_census)));
for(const f of native)if(!f.symbol||!f.type||!f.owner||!f.unit||!f.sensitivity||sha(f.source_path)!==f.source_sha)throw Error("Stale native census: "+f.schema+"."+f.symbol);
// F0 field placement: every proto field and every native census field has exactly one
// frontend place (panel + section of FRONTEND_PHASENPLAN §13) and one connection status.
const STATUS=new Set(["IST_CONNECTED","IST_BUG","SOLL_MISSING","FUTURE_PHASE_BLOCKED","EXCLUDED"]);
const sections=manifest.panel_sections;
const placed=(where,panel,section,status)=>{
 if(!STATUS.has(status))throw Error(where+": status "+status);
 if(panel==="none"||panel==="shared"){if(section!==0||(panel==="none")!==(status==="EXCLUDED"))throw Error(where+": "+panel+" needs section 0"+(panel==="none"?" and EXCLUDED":""));return}
 if(!sections[panel]||!(section>=1&&section<=sections[panel].length))throw Error(where+": no section "+panel+"/"+section);
 if(status==="EXCLUDED")throw Error(where+": EXCLUDED needs panel none");
};
for(const f of manifest.fields)placed(f.field,f.frontend_panel,f.section_order,f.status);
const rank=m=>m==="*"?0:typeof m==="string"&&m.startsWith("prefix:")?m.length:Number.MAX_SAFE_INTEGER;
const hits=(m,s)=>m==="*"||(Array.isArray(m)?m.includes(s):s.startsWith(m.slice(7)));
const used=new Set();const count={};
const NEVER=new Set(["SECRET_NEVER_BROWSER","BINARY_EXCLUDED","PRIVATE_RECORD_FIELD_AUDIT_REQUIRED"]);
for(const f of native){
 const id=f.schema+"."+f.symbol;
 const cands=manifest.native_bindings.map((r,i)=>[r,i]).filter(([r])=>r.schemas.includes(f.schema)&&hits(r.match,f.symbol));
 if(!cands.length)throw Error("Unplaced native field "+id);
 const best=Math.max(...cands.map(([r])=>rank(r.match)));
 const win=cands.filter(([r])=>rank(r.match)===best);
 if(win.length!==1)throw Error("Ambiguous placement "+id);
 const [r,i]=win[0];used.add(i);
 placed(id,r.panel,r.section,r.status);
 if(NEVER.has(f.sensitivity)&&r.status!=="EXCLUDED")throw Error(f.sensitivity+" must stay EXCLUDED: "+id);
 const k=r.panel+"/"+r.status;count[k]=(count[k]||0)+1;
}
manifest.native_bindings.forEach((r,i)=>{if(!used.has(i))throw Error("Dead native binding rule "+i+" "+JSON.stringify(r.match).slice(0,80))});
const total=s=>Object.entries(count).filter(([k])=>k.endsWith("/"+s)).reduce((a,[,n])=>a+n,0);
console.log("BSC F0-VAR: PASS "+native.length+" native fields placed ("+[...STATUS].map(s=>s+" "+total(s)).join(", ")+"), "+manifest.fields.length+" proto fields placed");
