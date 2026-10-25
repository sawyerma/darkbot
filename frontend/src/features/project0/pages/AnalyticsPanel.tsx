import { useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import type { LiquidatorDraft, Project0Tab } from "../model/contracts";
import { jobReadableRuns, type AnalyticsJobView, type AnalyticsView, type JobResultView } from "../model/analytics";
import { usdMicroDisplay } from "../model/format";
import { Section, DetailRow, TabStrip, Notice } from "../components/PanelPrimitives";

type AnalTab = "report" | "backtest" | "optimize";
const tabs: ReadonlyArray<{ value: AnalTab; label: string }> = [
  { value: "report", label: "Report" }, { value: "backtest", label: "Backtest" }, { value: "optimize", label: "Optimize" }];

/** One line of a job's result headline; counterfactual values are never realized profit. */
function resultLine(r: JobResultView | null): string {
  if (!r) return "";
  if (r.kind === "BACKTEST") {
    const c = r.counterfactual;
    const nets = c?.scenarios.map(s => `${usdMicroDisplay(s.capitalUsdMicro.toString())} → ${usdMicroDisplay(s.netUsdMicro)}`).join(", ") ?? "—";
    return ` · ${r.liquidations} liquidations · ${c?.matched ?? "—"} matched · ${c?.wins ?? "—"} wins · Shadow net ${nets} · landing ${c?.landing ?? "—"}`;
  }
  const chosen = r.chosen ? `min profit ${usdMicroDisplay(r.chosen.minProfitUsdMicro)} · funding ${r.chosen.funding.length ? r.chosen.funding.join(",") : "all"}` : "no point chosen (NO_SAMPLE)";
  return ` · ${r.points} points · ${chosen} · holdout ${r.chosenObjectiveBps ?? "—"} bps vs baseline ${r.baselineObjectiveBps ?? "—"} bps${r.profile ? ` · Shadow candidate ${r.profile}` : ""}`;
}

function JobRow({ job }: { job: AnalyticsJobView }) {
  return <DetailRow title={`${job.kind} · ${job.run} · ${job.jobId.slice(0, 8)}`}
    value={`${job.state}${job.code ? ` · ${job.code}` : ""}${resultLine(job.result)}`}/>;
}

export function AnalyticsPanel({ analytics, topicStatus, createDraft, goto }: {
  analytics: AnalyticsView | null; topicStatus: TopicStatus;
  createDraft: (x?: Pick<LiquidatorDraft, "capitalUsd" | "minimumNetUsd">) => void; goto: (x: Project0Tab) => void;
}) {
  const [tab, setTab] = useState<AnalTab>("report"); const [capital, setCapital] = useState(""); const [minimumNet, setMinimumNet] = useState("");
  const [msg, setMsg] = useState(""); const [wanted, setWanted] = useState("");
  const runs = jobReadableRuns(analytics);
  const run = runs.find(r => r.runId === wanted) ?? runs[0] ?? null;
  const jobs = analytics?.jobs ?? [];
  const latest = jobs.find(j => j.run === run?.runId && j.kind === "BACKTEST" && j.state === "COMPLETED") ?? null;
  const backtest = latest?.result?.kind === "BACKTEST" ? latest.result : null;
  const live = topicStatus === "LIVE";
  return <><Section title="Runs & analysis" aside={<span className="p0-badge">{live ? `${analytics?.runnerBusy ? "job running" : "idle"}` : topicStatus}</span>}>
    {!live && analytics ? <p className="p0-quiet">Last reported jobs ({topicStatus}); not current.</p> : null}
    {analytics?.storeError ? <Notice>Job store unreadable: {analytics.storeError}</Notice> : null}
    <label>Recorded run<select value={run?.runId ?? ""} disabled={runs.length === 0} onChange={e => setWanted(e.target.value)}>
      {runs.length === 0 ? <option value="">{analytics ? "No completed native run" : "No run list received"}</option> : null}
      {runs.map(r => <option key={r.runId} value={r.runId}>{r.runId}{r.qualification ? ` · ${r.qualification.verdict}` : ""}</option>)}
    </select></label>
    <TabStrip minor label="Analysis mode" options={tabs} value={tab} onChange={setTab}/>
    {tab === "report" ? <>
      <DetailRow title="Qualification" value={run?.qualification ? `${run.qualification.verdict} · verification ${run.qualification.verification}` : "—"}/>
      <DetailRow title="Market liquidations (verified)" value={backtest ? backtest.liquidations.toString() : "—"}/>
      <DetailRow title="Counterfactual Shadow wins / matched" value={backtest?.counterfactual ? `${backtest.counterfactual.wins} / ${backtest.counterfactual.matched}` : "—"}/>
      {backtest?.counterfactual?.scenarios.map(s => <DetailRow key={s.capitalUsdMicro.toString()} title={`Shadow net · capital ${usdMicroDisplay(s.capitalUsdMicro.toString())}`} value={usdMicroDisplay(s.netUsdMicro)}/>)}
      <DetailRow title="Realized net PnL" value={backtest ? backtest.realized : "—"}/>
      <p className="p0-note">From the newest completed backtest of the run; counterfactual Shadow results (landing UNPROVEN) are never realized profit.</p></> : null}
    {tab === "backtest" ? <><p>Replay the run's sealed evidence through the production join (native job, offline).</p>
      <button disabled className="p0-primary" title="Start: p0ctl job start backtest">Run backtest</button></> : null}
    {tab === "optimize" ? <><div className="p0-formgrid"><label>Capital · USD<input type="number" value={capital} onChange={e => setCapital(e.target.value)}/></label>
      <label>Minimum net edge · USD<input type="number" min="0" value={minimumNet} onChange={e => setMinimumNet(e.target.value)}/></label></div>
      <div className="p0-actions"><button disabled className="p0-primary" title="Start: p0ctl job start optimize">Optimize</button><button className="p0-secondary"
        onClick={() => { if (!/^\d+(?:\.\d{1,6})?$/.test(capital) || Number(capital) <= 0 || !/^\d+(?:\.\d{1,6})?$/.test(minimumNet)) {
          setMsg("Enter a positive capital and nonnegative net edge."); return }
          createDraft({ capitalUsd: capital, minimumNetUsd: minimumNet }); goto("liquidator"); }}>Add as bot draft →</button></div>
      <p className="p0-note">Only typed input values are copied to a draft; a saved optimizer profile is an offline Shadow candidate, never a LIVE setting.</p></> : null}
    {msg ? <Notice>{msg}</Notice> : null}</Section>
  <Section title="Analytics jobs" aside={<span className="p0-quiet">{jobs.length} listed</span>}>
    {jobs.length === 0 ? <p className="p0-quiet">{analytics ? "No analytics jobs." : "No job list received (a Project 0 process publishes it)."}</p> : null}
    {jobs.map(j => <JobRow key={j.jobId} job={j}/>)}
  </Section></>;
}
