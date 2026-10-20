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
for(const f of native)if(!f.symbol||!f.type||!f.owner||!f.unit||sha(f.source_path)!==f.source_sha)throw Error("Stale native census: "+f.schema+"."+f.symbol);
console.log("BSC F0-VAR: PASS (missing native sources remain explicitly blocked)");
