import "@/shared/ui/graphite/control-dialog.css";
import { useEffect,useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { readinessCommand,useReadinessControl,watchReadinessControl } from "@/core/realtime/readinessControl";
export function ReadinessActions(){
 const {process,busy,receipt}=useReadinessControl();const [confirm,setConfirm]=useState(false);
 useEffect(()=>watchReadinessControl(),[]);
 const running=process?.state==="RUNNING";const enabled=process?.controller_available&&!busy&&(running||process.state==="STOPPED"||process.state==="FAILED");
 useEffect(()=>{if(!running||busy)setConfirm(false);},[running,busy]);
 return <><p className="g-note">Readiness job · {process?.state||"UNKNOWN"}</p><AlertDialog.Root open={confirm&&running&&!busy} onOpenChange={setConfirm}>
 <button className="g-secondary" disabled={!enabled} data-testid="bsc-readiness-control" onClick={()=>{if(running)setConfirm(true);else void readinessCommand("bsc.readiness.start");}}>{running?"Stop readiness":"Run readiness"}</button>
 <AlertDialog.Portal><AlertDialog.Overlay className="ov-log-overlay"/><AlertDialog.Content className="ov-confirm-view"><AlertDialog.Title>Readiness-Prüfung stoppen?</AlertDialog.Title><AlertDialog.Description>Nur die laufende BSC-Prüfung wird abgebrochen. Capture und Gateway laufen weiter.</AlertDialog.Description><div className="ov-confirm-actions"><AlertDialog.Cancel asChild><button className="ov-btn">Abbrechen</button></AlertDialog.Cancel><AlertDialog.Action asChild><button className="ov-btn" onClick={()=>{setConfirm(false);void readinessCommand("bsc.readiness.stop");}}>Prüfung stoppen</button></AlertDialog.Action></div></AlertDialog.Content></AlertDialog.Portal></AlertDialog.Root>
 {receipt?<p role="status">{receipt.phase} · {receipt.detail}<br/>Command {receipt.id}</p>:null}</>;
}
