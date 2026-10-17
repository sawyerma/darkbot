import { useState } from "react";
import type { FinanceSnapshot } from "../model/contracts";
import { financialRows } from "../model/format";
import { Section, DetailRow, TabStrip, Notice } from "../components/PanelPrimitives";
type WalletTab="balances"|"send"|"auto";
const tabs:[{value:WalletTab;label:string},{value:WalletTab;label:string},{value:WalletTab;label:string}]=[
 {value:"balances",label:"Balances"},{value:"send",label:"Send"},{value:"auto",label:"Auto-Sweep"}];
export function WalletsPanel({finance}:{finance:FinanceSnapshot|null}){
 const [tab,setTab]=useState<WalletTab>("balances");const [message,setMessage]=useState("");
 const [threshold,setThreshold]=useState("");const [share,setShare]=useState("");
 return <Section title="Wallets & Treasury">
 <TabStrip minor label="Wallet views" value={tab} onChange={v=>{setTab(v);setMessage("")}} options={tabs}/>
 {tab==="balances"?<><div className="p0-row"><span>Project_0 · Solana wallet</span><span className="p0-badge">Not connected</span></div>
 {financialRows(finance).map(([title,value])=><DetailRow key={title} title={title} value={value}/>)}
 <div className="p0-actions"><button className="p0-secondary" onClick={()=>setTab("send")}>Send funds →</button>
 <button className="p0-secondary" onClick={()=>setTab("auto")}>Auto-Sweep →</button></div></>:null}
 {tab==="send"?<><div className="p0-formgrid">
 <label>Destination wallet<select><option>Phantom · Solana</option><option>MetaMask · Solana</option></select></label>
 <label>Asset<select><option>USDC</option><option>SOL</option></select></label></div>
 <label>Solana destination address<input placeholder="Public Solana address" autoComplete="off"/></label>
 <label>Amount<input type="number" min="0" placeholder="0.00"/></label>
 <button className="p0-secondary" onClick={()=>setMessage("Transfer unavailable: no reconciled balance, destination validation or value-bearing authorization.")}>Review transfer</button>
 <p className="p0-note">Preview only. No transaction can be signed or sent.</p></>:null}
 {tab==="auto"?<><div className="p0-formgrid"><label>Realized profit threshold · USDC
 <input type="number" min="0" value={threshold} onChange={e=>setThreshold(e.target.value)}/></label>
 <label>Payout share · %<input type="number" min="1" max="100" value={share} onChange={e=>setShare(e.target.value)}/></label></div>
 <DetailRow title="Destination" value="Must be verified"/><DetailRow title="Auto-Sweep status" value="Disabled"/>
 <button className="p0-secondary" onClick={()=>setMessage("Rule draft only. No treasury policy is saved or authorized.")}>Preview rule</button>
 <p className="p0-note">Only finalized, unreserved profit can later qualify for payout.</p></>:null}
 {message?<Notice>{message}</Notice>:null}</Section>;
}