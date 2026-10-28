import { useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import { analyticsCommand, useAnalyticsControl, verifyCommand, type VerifyMode } from "@/core/realtime/analyticsControl";
import { jobCommandBlock, methodLine, type AnalyticsView } from "../model/analytics";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

/**
 * A12/F-03: the recorded runs with their post-capture state as the native records hold it
 * (project0.analytics.jobs): layout, capture completion, the newest qualification record, the
 * newest post-capture job and (DEC-08) every chain verification method with its status,
 * progress, provider, requests, bytes, checkpoint and block reason. A08/F-02: the post-capture
 * chain (verification off, or index|strict|crosscheck from the checkpoint, report,
 * qualification) is started here through the same controller as `p0ctl job start post-capture`;
 * an online history read needs the operator's provider evidence.
 */
export function RunsSection({ analytics, topicStatus }: { analytics: AnalyticsView | null; topicStatus: TopicStatus }) {
  const runs = analytics?.runs ?? [];
  const { busy, receipt } = useAnalyticsControl();
  const [mode, setMode] = useState<VerifyMode>("off");
  const [seconds, setSeconds] = useState(""); const [mib, setMib] = useState(""); const [msg, setMsg] = useState("");
  const verifiable = runs.filter(r => r.layout === "NATIVE_V2" && r.captureComplete && !r.running);
  const [wanted, setWanted] = useState("");
  const run = verifiable.find(r => r.runId === wanted) ?? verifiable[0] ?? null;
  const blocked = jobCommandBlock(analytics, topicStatus, busy);
  const start = () => {
    const typed = verifyCommand({ runId: run?.runId ?? "", mode, resume: mode !== "off", maxSeconds: seconds, maxRamMib: mib });
    if ("problem" in typed) { setMsg(typed.problem); return; }
    setMsg("");
    void analyticsCommand("project0.capture.verify.start", { params: typed.params });
  };
  return <Section title="Recorded runs · post-capture" aside={<span className="p0-badge">{topicStatus}</span>}>
    {topicStatus !== "LIVE" && analytics ? <p className="p0-quiet">Last reported runs ({topicStatus}); not current.</p> : null}
    {runs.length === 0 ? <p className="p0-quiet">{analytics ? "No runs under the project's run roots." : "No run list received (a Project 0 process publishes it)."}</p> : null}
    {runs.map(r => {
      const q = r.qualification;
      const qualification = q == null ? "no qualification record"
        : q.error ? `qualification unreadable · ${q.error}`
        : `${q.verdict} · verification ${q.verification}${q.failed.length ? ` · failed ${q.failed.join(", ")}` : ""}`;
      const job = analytics?.jobs.find(j => j.kind === "POST_CAPTURE" && j.run === r.runId);
      return <div key={`${r.root}/${r.runId}`}><DetailRow title={`${r.runId}${r.running ? " · running" : ""}`}
        value={`${r.layout} · ${r.captureComplete ? "capture complete" : "capture not complete"} · ${qualification}${job ? ` · post-capture job ${job.state}${job.code ? ` (${job.code})` : ""}` : ""}`}/>
        {r.verification.map(m => <DetailRow key={m.specId} title={`${r.runId} · verification ${m.method}`} value={methodLine(m)}/>)}</div>;
    })}
    {verifiable.length ? <>
      <div className="p0-formgrid">
        <label>Run<select value={run?.runId ?? ""} onChange={e => setWanted(e.target.value)}>{verifiable.map(r => <option key={r.runId} value={r.runId}>{r.runId}</option>)}</select></label>
        <label>Verification<select value={mode} onChange={e => setMode(e.target.value as VerifyMode)}>
          <option value="off">Off (identities, no chain read)</option><option value="index-cache">Index from the checkpoint (offline)</option>
          <option value="strict-cache">Strict from the checkpoint (offline)</option><option value="crosscheck-cache">Crosscheck from the checkpoints (offline)</option></select></label>
        <label>Budget · seconds<input value={seconds} placeholder="required" onChange={e => setSeconds(e.target.value)}/></label>
        <label>Budget · RAM MiB<input value={mib} placeholder="required" onChange={e => setMib(e.target.value)}/></label>
      </div>
      <div className="p0-actions"><button className="p0-primary" disabled={blocked !== ""} title={blocked || undefined} onClick={start}>Verify, report & qualify</button></div>
      {blocked ? <p className="p0-quiet">{blocked}</p> : null}
      <p className="p0-note">An online chain verification reads the chain with the operator's provider evidence: p0ctl job start post-capture --chain-verify index|strict|crosscheck --provider-evidence FILE (terminal). Crosscheck needs a second, separately qualified endpoint; without one it is NOT_RUN.</p>
    </> : null}
    {msg ? <Notice>{msg}</Notice> : null}
    {receipt?.action === "project0.capture.verify.start" ? <Notice><span data-testid="p0-verify-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""}</span></Notice> : null}
  </Section>;
}
