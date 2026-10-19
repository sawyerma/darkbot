import { useState } from "react";
import { REPORT_MARKERS } from "../model/contracts";
import type { CaptureMode, CaptureSource, VerificationMode, ReportMarker } from "../model/contracts";
import type { CaptureView } from "../model/status";
import type { TopicStatus } from "@/core/realtime";
import { Section, DetailRow, TabStrip, Notice } from "../components/PanelPrimitives";
import { DASH, fmtAgeNs, orDash } from "../model/statusFormat";
import { counterOf, phaseLabel } from "../model/statusSelectors";

const modes = [{ value: "off", label: "Off" }, { value: "index", label: "Index" }, { value: "strict", label: "Strict" }, { value: "crosscheck", label: "Crosscheck" }] as const;
const PRESETS = ["4h", "30s", "10m", "1h", "24h", "custom"] as const;

function RunningCapture({ capture, topicStatus }: { capture: CaptureView | null; topicStatus: TopicStatus }) {
  const run = capture?.run ?? null;
  const t = capture?.transport ?? null;
  const a = capture?.assessment ?? null;
  return <Section className="p0-live-section" title="Running capture" aside={<span className="p0-badge">{phaseLabel(capture)}</span>}>
    {run ? <>
      <DetailRow title="Status feed" value={`${topicStatus} · last reported values`}/>
      <DetailRow title="Run" value={`${run.runId} · ${run.source}/${run.commitment}`}/>
      <DetailRow title="Configured duration" value={run.configuredDurationSeconds === 0n ? "Until stopped" : `${run.configuredDurationSeconds} s`}/>
      <DetailRow title="Stream" value={t ? `${t.connected == null ? "unknown" : t.connected ? "connected" : "disconnected"} · frame age ${fmtAgeNs(t.lastFrameAgeNs)}${t.lastCode ? ` · last ${t.lastCode}` : ""}` : DASH}/>
      <DetailRow title="Transactions / liquidation events" value={`${orDash(counterOf(capture, "unique_transactions"))} / ${orDash(counterOf(capture, "liquidation_events"))}`}/>
      <DetailRow title="Verdict (live)" value={a?.verdict ?? DASH}/>
      {capture?.phase?.phase === "ENDED" ? <DetailRow title="Result" value={capture.phase.endResult}/> : null}
    </> : <p className="p0-quiet">No capture reported by Project 0.</p>}
  </Section>;
}

export function CapturePanel({ capture, topicStatus }: { capture: CaptureView | null; topicStatus: TopicStatus }) {
  const [preset, setPreset] = useState<string>("4h"); const [amount, setAmount] = useState("75"); const [unit, setUnit] = useState("m");
  const [source, setSource] = useState<CaptureSource>("logs"); const [observation, setObservation] = useState<CaptureMode>("econ4h");
  const [verify, setVerify] = useState<VerificationMode>("off");
  const [markers, setMarkers] = useState<ReportMarker[]>(REPORT_MARKERS.map(m => m.id));
  const [message, setMessage] = useState("");
  const duration = preset === "custom" ? "--duration " + amount + unit : "--duration " + preset;
  const command = "p0ctl run start " + duration + " --capture-source " + source + " --mode " + observation + " --chain-verify " + verify;
  return <><RunningCapture capture={capture} topicStatus={topicStatus}/>
  <Section title="Raw data capture"><DetailRow title="Duration" value="Presets are optional"/>
    <div className="p0-pills">
      {PRESETS.map(d => <button key={d} className={preset === d ? "p0-selected" : ""} onClick={() => setPreset(d)}>{d === "custom" ? "Custom" : d}</button>)}
      <button disabled title="Desktop capture runs until explicitly stopped; GUI control commands are not wired.">Until stopped · desktop control</button>
    </div>
    {preset === "custom" ? <div className="p0-formgrid"><label>Duration<input type="number" min="1" value={amount} onChange={e => setAmount(e.target.value)}/></label>
      <label>Unit<select value={unit} onChange={e => setUnit(e.target.value)}><option value="s">Seconds</option><option value="m">Minutes</option><option value="h">Hours</option><option value="d">Days</option></select></label></div> : null}
    <div className="p0-formgrid"><label>Source<select value={source} onChange={e => setSource(e.target.value as CaptureSource)}><option value="logs">Logs</option><option value="transactions">Transactions</option></select></label>
      <label>Observation<select value={observation} onChange={e => setObservation(e.target.value as CaptureMode)}><option value="econ4h">Market + Shadow</option><option value="market">Raw data only</option><option value="shadow">Shadow only</option></select></label></div>
    <div className="p0-actions"><button className="p0-primary" onClick={() => setMessage("Boot/preflight backend not connected. No capture was started.")}>Check & start</button>
      <button className="p0-secondary" onClick={() => setMessage("Stop command not connected. Stop via ops/darkbotctl capture stop.")}>Stop</button></div>{message ? <Notice>{message}</Notice> : null}
  </Section>
  <Section title="Chain verification" aside={<span className="p0-quiet">After capture</span>}>
    <TabStrip options={modes.map(x => ({ ...x }))} value={verify} onChange={setVerify} label="Verification mode"/>
    <p className="p0-note">Existing CLI modes; strict/crosscheck still need qualified evidence and provider capacity.</p></Section>
  <Section title="Report markers" aside={<div className="p0-actions"><button className="p0-secondary" onClick={() => setMarkers(REPORT_MARKERS.map(x => x.id))}>All</button><button className="p0-secondary" onClick={() => setMarkers([])}>None</button></div>}>
    <div className="p0-formgrid">{REPORT_MARKERS.map(m => <label key={m.id} className="p0-check"><input type="checkbox" checked={markers.includes(m.id)} onChange={e => setMarkers(old => e.target.checked ? [...old, m.id] : old.filter(x => x !== m.id))}/>{m.label}</label>)}</div>
    <p className="p0-note">Only report visibility changes; raw capture is always unfiltered.</p></Section>
  <Section title="CLI preview"><div className="p0-code">{command}</div>
    <DetailRow title="Selected report markers" value={markers.length + " / 6"}/>
    <p className="p0-note">Proposed p0ctl command. The control binary is not implemented; nothing runs here.</p></Section></>;
}
