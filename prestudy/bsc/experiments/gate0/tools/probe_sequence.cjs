// Offline G0.3: parent -> launch -> immediate direct BUY. No future-state patch.
const fs=require('fs'),path=require('path'),assert=require('assert/strict');
const R=path.resolve(__dirname,'../..'),B=path.join(R,'tests/DARKBOT 2/gate0-evidence');
const {run}=require(path.join(B,'replay_flap_evm.cjs'));
const read=p=>JSON.parse(fs.readFileSync(p)),clean=x=>JSON.parse(JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v));
async function main(){
 const merged=read(path.join(R,'gate0/evidence/g03_merged_parent_state.json')).state;
 const closureFile=path.join(R,'gate0/evidence/g03_parent_closure.json');
 if(fs.existsSync(closureFile))for(const [a,v] of Object.entries(read(closureFile).state)){
  if(!merged[a])merged[a]=v;else for(const [k,x] of Object.entries(v))if(k==='storage'){for(const [s,y] of Object.entries(x)){if(s in (merged[a].storage||{}))assert.equal(BigInt(merged[a].storage[s]),BigInt(y));else (merged[a].storage??={})[s]=y;}}else{if(k in merged[a])assert.equal(merged[a][k],x);else merged[a][k]=x;}
 }
 if(process.argv.includes('--before')){const b=await run('direct_buy',{state:merged,blockNumber:124415357,noSave:true,quiet:true});console.log(JSON.stringify({exception:b.exception,frames:b.frame_count,missing_storage:b.missing_storage}));return;}
 const launch=await run('launch',{state:merged,noSave:true,quiet:true});assert.equal(launch.exception,null);assert(launch.all_frames_match);
 const historical=read(path.join(B,'2026-09-28-g02-archive-witness-quicknode-153754/direct_buy_prestate.json')).result;
 const zeros=[];for(const a of launch.created)for(const s of Object.keys(historical[a]?.storage||{}))if(!(s in launch.final_state[a].storage)){launch.final_state[a].storage[s]='0x'+'0'.repeat(64);zeros.push({address:a,slot:s,reason:'Newly created account; not written by launch'});}
 const buy=await run('direct_buy',{state:launch.final_state,blockNumber:124415357,noSave:true,quiet:true});
 const out={scope:'Immediate direct BUY using parent-block state and computed launch effects; NO later storage values',launch,zero_initialized_created_slots:zeros,buy};
 fs.writeFileSync(path.join(R,'gate0/evidence/g03_sequential_execution.json'),JSON.stringify(clean(out),null,2)+'\n');
 console.log(JSON.stringify({exception:buy.exception,frames:buy.frame_count,return_value:buy.return_value,missing_accounts:buy.missing_accounts,missing_storage:buy.missing_storage,created_zero_slots:zeros.length}));
}
main().catch(e=>{console.error(e);process.exitCode=1});
