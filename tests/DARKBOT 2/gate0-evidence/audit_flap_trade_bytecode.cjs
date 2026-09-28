/* Offline, scoped arithmetic audit of the historical PortalTrade bytecode.
 * This is NOT a consensus EVM: no gas accounting or complete external-call execution.
 * External effects are explicitly supplied by the historical call witness. Never a gate PASS by itself.
 * No networking, transaction creation, signing, or submission.
 */
const {keccak256}=require('./abi-tools/node_modules/ethers');
const MASK=(1n<<256n)-1n;
const hex=x=>'0x'+x.toString(16).padStart(64,'0');
const addr=x=>'0x'+(x&((1n<<160n)-1n)).toString(16).padStart(40,'0');
function audit(codeHex, storage, frame, origin, inputHex=frame.input, accounts={}) {
 const code=Buffer.from(codeHex.slice(2),'hex'),data=Buffer.from(inputHex.slice(2),'hex');
 const stack=[],mem=Buffer.alloc(2**20),state=new Map(Object.entries(storage).map(([k,v])=>[BigInt(k),BigInt(v)]));
 const reads=[],writes=[],arithmetic=[],external=[],branches=[],constants=[];
 let pc=0, high=0, returndata=Buffer.alloc(0), ci=0, steps=0;
 const pop=()=>{if(!stack.length)throw Error('stack underflow');return stack.pop()};
 const push=x=>stack.push(BigInt(x)&MASK);
 const region=(o,n)=>{o=Number(o);n=Number(n);if(o<0||n<0||o+n>mem.length)throw Error('memory bound');high=Math.max(high,Math.ceil((o+n)/32)*32);return mem.subarray(o,o+n)};
 const put=(o,b,n=b.length)=>{const dest=region(o,n);dest.fill(0);b.copy(dest,0,0,Number(n));};
 const signed=x=>x>>255n?x-(1n<<256n):x;
 const jump=d=>{if(code[Number(d)]!==0x5b)throw Error('invalid jump');pc=Number(d)};
 function done(output){if(ci!==(frame.calls||[]).length)throw Error('unconsumed external calls');return {output:'0x'+output.toString('hex'),reads,writes,arithmetic,external,branches,constants,steps,limitations:['No gas accounting','External calls use recorded results; side effects are not replayed','Does not prove pending-state provenance or intervening transactions']};}
 try {
 while(pc<code.length && steps++<200000){
  const at=pc,op=code[pc++];
  if(op>=0x60&&op<=0x7f){const n=op-0x5f;const v=BigInt('0x'+code.subarray(pc,pc+n).toString('hex'));push(v);constants.push({pc:at,bytes:n,value:v.toString()});pc+=n;continue;}
  if(op>=0x80&&op<=0x8f){push(stack[stack.length-(op-0x7f)]);continue;}
  if(op>=0x90&&op<=0x9f){const j=stack.length-1-(op-0x8f);[stack[j],stack[stack.length-1]]=[stack[stack.length-1],stack[j]];continue;}
  if([1,2,3,4,5,6,7,10,0x10,0x11,0x12,0x13,0x14,0x16,0x17,0x18,0x1a,0x1b,0x1c,0x1d].includes(op)){
   const a=pop(),b=pop();let v;
   switch(op){case 1:v=a+b;break;case 2:v=a*b;break;case 3:v=a-b;break;case 4:v=b?a/b:0n;break;
    case 5:v=b?signed(a)/signed(b):0n;break;case 6:v=b?a%b:0n;break;case 7:v=b?signed(a)%signed(b):0n;break;
    case 10: {let n=b,x=a;v=1n;while(n){if(n&1n)v=v*x&MASK;x=x*x&MASK;n>>=1n;}break;}
    case 0x10:v=+(a<b);break;case 0x11:v=+(a>b);break;case 0x12:v=+(signed(a)<signed(b));break;case 0x13:v=+(signed(a)>signed(b));break;case 0x14:v=+(a===b);break;
    case 0x16:v=a&b;break;case 0x17:v=a|b;break;case 0x18:v=a^b;break;case 0x1a:v=a<32n?(b>>((31n-a)*8n))&255n:0n;break;
    case 0x1b:v=a<256n?b<<a:0n;break;case 0x1c:v=a<256n?b>>a:0n;break;case 0x1d:v=a<256n?signed(b)>>a:(signed(b)<0n?-1n:0n);break;
   }push(v);if([1,2,3,4,5,6,7,10].includes(op))arithmetic.push({pc:at,op,a:a.toString(),b:b.toString(),result:(BigInt(v)&MASK).toString()});continue;
  }
  switch(op){
   case 0: return done(Buffer.alloc(0));
   case 8:case 9:{const a=pop(),b=pop(),n=pop();push(n?(op===8?a+b:a*b)%n:0n);break;}
   case 0x0b:{let i=pop(),x=pop();if(i<32n){let bit=i*8n+7n,mask=(1n<<(bit+1n))-1n;x=x&(1n<<bit)?x|MASK^mask:x&mask;}push(x);break;}
   case 0x15:push(pop()===0n?1:0);break;
   case 0x19:push(~pop());break;
   case 0x20:{const o=pop(),n=pop();push(BigInt(keccak256(region(o,n))));break;}
   case 0x30:push(BigInt(frame.auditAddress||frame.from));break; // explicit execution context, including delegatecall storage/address
   case 0x32:push(BigInt(origin));break;
   case 0x33:push(BigInt(frame.auditCaller));break;
   case 0x34:push(BigInt(frame.value||'0x0'));break;
   case 0x35:{const o=Number(pop());const b=Buffer.alloc(32);data.subarray(o,o+32).copy(b);push(BigInt('0x'+b.toString('hex')));break;}
   case 0x36:push(data.length);break;
   case 0x37:{let o=pop(),i=Number(pop()),n=Number(pop());put(o,data.subarray(i,i+n),n);break;}
   case 0x38:push(code.length);break;
   case 0x39:{let o=pop(),i=Number(pop()),n=Number(pop());put(o,code.subarray(i,i+n),n);break;}
   case 0x3b:{const a=addr(pop());if(!accounts[a])throw Error('missing code account '+a);push(((accounts[a].code||'0x').length-2)/2);break;}
   case 0x3d:push(returndata.length);break;
   case 0x3e:{let o=pop(),i=Number(pop()),n=Number(pop());if(i+n>returndata.length)throw Error('returndata bound');put(o,returndata.subarray(i,i+n),n);break;}
   case 0x42:if(frame.auditTimestamp!==undefined){push(BigInt(frame.auditTimestamp));break;}return {status:'STOP_MISSING_BLOCK_TIMESTAMP',pc:at,reads,writes,arithmetic,external,branches,constants,steps,remaining_external_calls:(frame.calls||[]).length-ci,limitations:['Scoped arithmetic only, not a complete EVM replay','No block timestamp supplied to this scoped audit','No gas accounting','External-call results come from call witness, not full callee execution']};
   case 0x50:pop();break;
   case 0x51:push(BigInt('0x'+region(pop(),32).toString('hex')));break;
   case 0x52:{const o=pop(),v=pop();put(o,Buffer.from(hex(v).slice(2),'hex'));break;}
   case 0x53:{const o=pop(),v=pop();region(o,1)[0]=Number(v&255n);break;}
   case 0x54:{const k=pop();if(!state.has(k))throw Error('missing witnessed storage '+hex(k));push(state.get(k));reads.push({pc:at,slot:hex(k),value:hex(state.get(k))});break;}
   case 0x55:{const k=pop(),v=pop();state.set(k,v);writes.push({pc:at,slot:hex(k),value:hex(v)});break;}
   case 0x56:jump(pop());break;
   case 0x57:{const d=pop(),c=pop();branches.push({pc:at,to:Number(d),taken:c!==0n});if(c)jump(d);break;}
   case 0x58:push(at);break;
   case 0x59:push(high);break;
   case 0x5a:push(10000000);break; // gas only forwarded to witnessed calls; not a gas/revert feasibility proof
   case 0x5b:break;
   case 0x5e:{const dst=pop(),src=pop(),n=pop();put(dst,Buffer.from(region(src,n)));break;}
   case 0x5f:push(0);break;
   case 0xf1:case 0xfa:case 0xf4:{
    pop();const to=addr(pop()),value=op===0xf1?pop():0n,off=pop(),len=pop(),outOff=pop(),outLen=pop();
    const input='0x'+region(off,len).toString('hex'),w=(frame.calls||[])[ci++];
    if(!w||w.to!==to||w.input!==input||(op!==0xf4&&BigInt(w.value||0)!==value))throw Error('external call mismatch at '+at.toString(16)+' target '+to+' selector '+input.slice(0,10));
    external.push({pc:at,index:ci-1,type:op===0xf1?'CALL':op===0xfa?'STATICCALL':'DELEGATECALL',to,input,value:value.toString(),return_data_source:'historical call witness; dependency explicitly retained'});
    returndata=Buffer.from((w.output||'0x').slice(2),'hex');const n=Math.min(Number(outLen),returndata.length);put(outOff,returndata.subarray(0,n),n);push(w.error?0:1);break;
   }
   case 0xf3:{const o=pop(),n=pop();return done(region(o,n));}
   case 0xfd:{const o=pop(),n=pop();throw Error('REVERT '+region(o,n).toString('hex'));}
   default:if(op>=0xa0&&op<=0xa4){const o=pop(),n=pop();region(o,n);for(let i=0;i<op-0xa0;i++)pop();break;}throw Error('unsupported opcode 0x'+op.toString(16));
  }
 }
 throw Error('execution bound');
 }catch(e){e.message+=' [pc=0x'+(pc-1).toString(16)+', steps='+steps+']';throw e;}
}
module.exports={audit};
