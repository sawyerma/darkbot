import { useEffect, useState } from "react";
import { REPORT_MARKERS } from "../model/contracts";
import type { CaptureMode, CaptureSource, VerificationMode, ReportMarker } from "../model/contracts";
import type { CaptureView } from "../model/status";
import type { AnalyticsView } from "../model/analytics";
import { RunsSection } from "../components/RunsSection";
import type { TopicStatus } from "@/core/realtime";
import { captureButtons, captureCommand, useCaptureControl, watchCaptureControl } from "@/core/realtime/captureControl";
import { Section, DetailRow, TabStrip, Notice } from "../components/PanelPrimitives";
import { DASH, fmtAgeNs, fmtProvider, orDash } from "../model/statusFormat";
import { counterOf, phaseLabel } from "../model/statusSelectors";
import { RUN_START_VERIFY, runStartPreview } from "../model/capturePreview";

const modes = ([{ value: "off", label: "Off" }, { value: "index", label: "Index" }, { value: "strict", label: "Strict" }, { value: "crosscheck", label: "Crosscheck" }] as const)
  .map(m => RUN_START_VERIFY.has(m.value) ? m : { ...m, disabled: true, title: `Blocked by p0ctl run start (CHAIN_VERIFY_${m.value}_BLOCKED)` });
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
      <DetailRow title="Stream" value={t ? `${t.connected == null ? "unknown" : t.connected ? "connected" : "disconnected"} · frame age ${fmtAgeNs(t.lastFrameAgeNs)} · reconnects ${t.reconnects ?? DASH}${t.lastCode ? ` · last ${t.lastCode}` : ""}` : DASH}/>
      <DetailRow title="Transactions / liquidation events" value={`${orDash(counterOf(capture, "unique_transactions"))} / ${orDash(counterOf(capture, "liquidation_events"))}`}/>
      <DetailRow title="Verdict (live)" value={a?.verdict ?? DASH}/>
      <DetailRow title="Provider budget" value={fmtProvider(capture?.provider)}/>
      {capture?.phase?.phase === "ENDED" ? <DetailRow title="Result" value={capture.phase.endResult}/> : null}
    </> : <p className="p0-quiet">No capture reported by Project 0.</p>}
  </Section>;
}

export function CapturePanel({ capture, topicStatus, analytics, analyticsStatus }: {
  capture: CaptureView | null; topicStatus: TopicStatus; analytics: AnalyticsView | null; analyticsStatus: TopicStatus;
}) {
  const [preset, setPreset] = useState<string>("4h"); const [amount, setAmount] = useState("75"); const [unit, setUnit] = useState("m");
  const [source, setSource] = useState<CaptureSource>("logs"); const [observation, setObservation] = useState<CaptureMode>("econ4h");
  const [verify, setVerify] = useState<VerificationMode>("off");
  const [markers, setMarkers] = useState<ReportMarker[]>(REPORT_MARKERS.map(m => m.id));
  const { process, busy, receipt } = useCaptureControl();
  useEffect(() => watchCaptureControl(), []);
  const buttons = captureButtons(process, busy);
  // A stop confirmation belongs to the observed run it was asked for; it expires with that run.
  const runKey = process ? `${process.run_id}#${process.pid ?? ""}` : null;
  const [armedFor, setArmedFor] = useState<string | null>(null);
  const confirmStop = armedFor !== null && armedFor === runKey && buttons.stop.enabled;
  const preview = runStartPreview({ duration: preset === "custom" ? amount + unit : preset, source, mode: observation, verify, markers });
  return <><RunningCapture capture={capture} topicStatus={topicStatus}/>
  <RunsSection analytics={analytics} topicStatus={analyticsStatus}/>
  <Section title="Raw data capture"><DetailRow title="Duration" value="Presets are optional"/>
    <div className="p0-pills">
      {PRESETS.map(d => <button key={d} className={preset === d ? "p0-selected" : ""} onClick={() => setPreset(d)}>{d === "custom" ? "Custom" : d}</button>)}
      <button disabled title="Desktop control only (darkbotctl capture start until-stopped). The GUI start uses the configured duration; p0ctl run start requires a fixed --duration.">Until stopped · desktop control</button>
    </div>
    {preset === "custom" ? <div className="p0-formgrid"><label>Duration<input type="number" min="1" value={amount} onChange={e => setAmount(e.target.value)}/></label>
      <label>Unit<select value={unit} onChange={e => setUnit(e.target.value)}><option value="s">Seconds</option><option value="m">Minutes</option><option value="h">Hours</option><option value="d">Days</option></select></label></div> : null}
    <div className="p0-formgrid"><label>Source<select value={source} onChange={e => setSource(e.target.value as CaptureSource)}><option value="logs">Logs</option><option value="transactions">Transactions</option></select></label>
      <label>Observation<select value={observation} onChange={e => setObservation(e.target.value as CaptureMode)}><option value="econ4h">Market + Shadow</option><option value="market">Raw data only</option><option value="shadow">Shadow only</option></select></label></div>
    <div className="p0-actions"><button className="p0-primary" disabled={!buttons.start.enabled} title={buttons.start.reason || undefined}
        onClick={() => { setArmedFor(null); void captureCommand("project0.capture.start"); }}>Check & start</button>
      <button className="p0-secondary" disabled={!buttons.stop.enabled} title={buttons.stop.reason || undefined}
        onClick={() => setArmedFor(runKey)}>Stop</button></div>
    {confirmStop ? <><Notice>darkbotctl capture stop: the journal drains and seals, autostart stays suspended until the next start, and a running BSC readiness check is stopped too. The gateway keeps running.</Notice>
      <div className="p0-actions"><button className="p0-secondary" onClick={() => { setArmedFor(null); void captureCommand("project0.capture.stop"); }}>Confirm stop</button>
        <button className="p0-secondary" onClick={() => setArmedFor(null)}>Cancel</button></div></> : null}
    {receipt ? <Notice><span data-testid="p0-capture-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""} · process {process?.state ?? "UNKNOWN"}</span></Notice> : null}
    <p className="p0-note">Check & start launches the configured desktop market capture (darkbotctl capture start-configured: raw data without Shadow, duration from its configuration, no boot audit or preflight). The selections in this panel are drafts and are not sent.</p>
  </Section>
  <Section title="Chain verification" aside={<span className="p0-quiet">After capture</span>}>
    <TabStrip options={modes.map(x => ({ ...x }))} value={verify} onChange={setVerify} label="Verification mode"/>
    <p className="p0-note">p0ctl run start accepts off and strict (strict needs provider evidence); index and crosscheck are blocked there.</p></Section>
  <Section title="Report markers" aside={<div className="p0-actions"><button className="p0-secondary" onClick={() => setMarkers(REPORT_MARKERS.map(x => x.id))}>All</button><button className="p0-secondary" onClick={() => setMarkers([])}>None</button></div>}>
    <div className="p0-formgrid">{REPORT_MARKERS.map(m => <label key={m.id} className="p0-check"><input type="checkbox" checked={markers.includes(m.id)} onChange={e => setMarkers(old => e.target.checked ? [...old, m.id] : old.filter(x => x !== m.id))}/>{m.label}</label>)}</div>
    <p className="p0-note">Only report visibility changes; raw capture is always unfiltered.</p></Section>
  <Section title="CLI preview"><div className="p0-code">{preview.command ?? `— · ${preview.problem}`}</div>
    <DetailRow title="Selected report markers" value={markers.length + " / 6"}/>
    <p className="p0-note">For the terminal; nothing runs from here. run start runs the boot audit and the market + shadow measurement; &lt;FILE&gt; and &lt;TOKEN&gt; are the operator's sealed network audit and its approval.</p></Section></>;
}
