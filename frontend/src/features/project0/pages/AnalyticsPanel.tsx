import { useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import type { LiquidatorDraft, Project0Tab } from "../model/contracts";
import { countOf, countSum, h0Line, jobCommandBlock, jobReadableRuns, sideBySide, type AnalyticsJobView, type AnalyticsRunView, type AnalyticsView, type JobResultView } from "../model/analytics";
import { REPORT_MARKERS } from "../model/contracts";
import { usdMicroDisplay } from "../model/format";
import { Section, DetailRow, TabStrip, Notice } from "../components/PanelPrimitives";
import { analyticsCommand, backtestCommand, h0Command, optimizeCommand, recheck, useAnalyticsControl, type GridInput, type PointInput } from "@/core/realtime/analyticsControl";

type AnalTab = "report" | "backtest" | "optimize";
const tabs: ReadonlyArray<{ value: AnalTab; label: string }> = [
  { value: "report", label: "Report" }, { value: "backtest", label: "Backtest" }, { value: "optimize", label: "Optimize" }];

/** One line of a job's result headline; counterfactual values are never realized profit. */
function resultLine(r: JobResultView | null, mode?: string): string {
  if (!r) return "";
  if (r.kind === "BACKTEST") {
    const c = r.counterfactual;
    const nets = c?.scenarios.map(s => `${usdMicroDisplay(s.capitalUsdMicro.toString())} → ${usdMicroDisplay(s.netUsdMicro)}`).join(", ") ?? "—";
    return ` · ${r.liquidations} liquidations · ${c?.matched ?? "—"} matched · ${c?.wins ?? "—"} wins · Shadow net ${nets} · landing ${c?.landing ?? "—"}`;
  }
  if (r.kind === "H0") return ` · ${h0Line(r)}`;
  // The mode's own verified status (never canonical unless strict); the master verdict is separate.
  if (r.kind === "POST_CAPTURE") return ` · verification ${mode ?? "—"} ${r.verificationOk ? "ok" : "incomplete"} · report ${r.reportOk ? "ok" : "incomplete"} · ${r.verdict ?? "qualification NOT_RUN"}`;
  const chosen = r.chosen ? `min profit ${usdMicroDisplay(r.chosen.minProfitUsdMicro)} · funding ${r.chosen.funding.length ? r.chosen.funding.join(",") : "all"}` : "no point chosen (NO_SAMPLE)";
  return ` · ${r.points} points · ${chosen} · holdout ${r.chosenObjectiveBps ?? "—"} bps vs baseline ${r.baselineObjectiveBps ?? "—"} bps${r.profile ? ` · Shadow candidate ${r.profile}` : ""}`;
}

function JobRow({ job, cancelBlock }: { job: AnalyticsJobView; cancelBlock: string }) {
  return <><DetailRow title={`${job.kind} · ${job.run} · ${job.jobId.slice(0, 8)}`}
    value={`${job.state}${job.code ? ` · ${job.code}` : ""}${resultLine(job.result, job.verification?.chainVerify)}`}/>
    {job.state === "RUNNING" ? <div className="p0-actions"><button className="p0-secondary" disabled={cancelBlock !== ""} title={cancelBlock || undefined}
      onClick={() => void analyticsCommand("project0.analytics.job.cancel", { resourceId: job.jobId })}>Cancel job</button></div> : null}</>;
}

const KINDS = ["CLASSIC", "RECEIVERSHIP", "FORCED_DELEVERAGE", "BANKRUPTCY_SETTLEMENT"] as const;

/** DEC-08: the run's chain verification methods side by side (index, crosscheck, strict): their
 *  status, the four liquidation kinds, the six markers, found and missing events, deviations and
 *  costs, each as its records hold it ("—": unknown or never run). */
function MethodsCompared({ run }: { run: AnalyticsRunView | null }) {
  const m = run?.verification ?? [];
  if (m.length === 0) return <DetailRow title="Chain verification" value="No native verification recorded for this run"/>;
  return <>
    <DetailRow title="Verification · status" value={sideBySide(m, x => `${x.status}${x.reason ? ` (${x.reason})` : ""}`)}/>
    {KINDS.map(k => <DetailRow key={k} title={`Verification · ${k}`} value={sideBySide(m, x => countOf(x.qualified, k))}/>)}
    {REPORT_MARKERS.map(k => <DetailRow key={k.id} title={`Verification · marker ${k.label}`} value={sideBySide(m, x => countOf(x.markers, k.id))}/>)}
    <DetailRow title="Verification · found events (MATCH)" value={sideBySide(m, x => countOf(x.comparison, "MATCH"))}/>
    <DetailRow title="Verification · missing events" value={sideBySide(m, x => countSum(x.comparison, ["MISSING_"]))}/>
    <DetailRow title="Verification · deviations" value={sideBySide(m, x => `${countSum(x.comparison, ["TYPE_MISMATCH", "TARGET_OR_ACTOR_MISMATCH", "AMOUNT_MISMATCH"])}${x.disagreements != null ? ` · ${x.disagreements} disagreements` : ""}`)}/>
    <DetailRow title="Verification · transactions live / slot mismatch / missing live / capture only" value={sideBySide(m, x => x.transactions.length === 0 ? "—"
      : `${countOf(x.transactions, "MATCH")} / ${countOf(x.transactions, "SLOT_MISMATCH")} / ${countOf(x.transactions, "MISSING_LIVE")} / ${countOf(x.transactions, "BOT_ONLY_INDEX_MISSING")}`)}/>
    <DetailRow title="Verification · unresolved / unknown" value={sideBySide(m, x => `${x.unresolved ?? "—"} / ${x.unknown ?? "—"}`)}/>
    <DetailRow title="Verification · costs" value={sideBySide(m, x => `${x.calls ?? "—"} requests · ${x.bytes ?? "—"} bytes · all attempts ${x.callsTotal ?? "—"} / ${x.bytesTotal ?? "—"}`)}/>
    <p className="p0-note">Nothing is promoted: INDEX_VERIFIED &lt; CROSSCHECK_VERIFIED &lt; CANONICAL_VERIFIED; only strict's canonical result can count toward the C0-E1 master, with every other gate.</p>
  </>;
}

/** The inputs of one backtest point; empty until the operator enters them (no hidden default). */
function PointFields({ point, set }: { point: PointInput; set: (p: PointInput) => void }) {
  const field = (key: keyof PointInput, label: string, placeholder: string) =>
    <label>{label}<input value={point[key]} placeholder={placeholder} onChange={e => set({ ...point, [key]: e.target.value })}/></label>;
  return <div className="p0-formgrid">
    {field("minProfitUsd", "Minimum profit · USD", "run's recorded minimum or higher")}
    {field("capitalUsd", "Capital scenarios · USD", "e.g. 100,1000")}
    {field("funding", "Funding classes", "all or NO_SWAP_INVENTORY,…")}
    {field("maxSeconds", "Budget · seconds", "required")}
    {field("maxRamMib", "Budget · RAM MiB", "required")}
  </div>;
}

export function AnalyticsPanel({ analytics, topicStatus, createDraft, goto }: {
  analytics: AnalyticsView | null; topicStatus: TopicStatus;
  createDraft: (x?: Pick<LiquidatorDraft, "capitalUsd" | "minimumNetUsd">) => void; goto: (x: Project0Tab) => void;
}) {
  const [tab, setTab] = useState<AnalTab>("report"); const [capital, setCapital] = useState(""); const [minimumNet, setMinimumNet] = useState("");
  const [msg, setMsg] = useState(""); const [wanted, setWanted] = useState("");
  const [point, setPoint] = useState<Omit<PointInput, "runId">>({ minProfitUsd: "", capitalUsd: "", funding: "all", maxSeconds: "", maxRamMib: "" });
  const [grid, setGrid] = useState<GridInput>({ minProfitUsdGrid: "", fundingSets: "all", holdoutBps: "", maxPoints: "", saveProfile: false });
  const { busy, receipt } = useAnalyticsControl();
  const runs = jobReadableRuns(analytics);
  const run = runs.find(r => r.runId === wanted) ?? runs[0] ?? null;
  const jobs = analytics?.jobs ?? [];
  const latest = jobs.find(j => j.run === run?.runId && j.kind === "BACKTEST" && j.state === "COMPLETED") ?? null;
  const backtest = latest?.result?.kind === "BACKTEST" ? latest.result : null;
  const h0Job = jobs.find(j => j.kind === "H0" && j.state === "COMPLETED") ?? null;
  const h0 = h0Job?.result?.kind === "H0" ? h0Job.result : null;
  const live = topicStatus === "LIVE";
  const input: PointInput = { ...point, runId: run?.runId ?? "" };
  const blocked = jobCommandBlock(analytics, topicStatus, busy);
  // Cancel needs a current list only (a running job is what it cancels).
  const cancelBlock = busy ? "Command in progress" : live ? "" : `Job status not current (${topicStatus})`;
  const start = (kind: "backtest" | "optimize") => {
    const typed = kind === "backtest" ? backtestCommand(input) : optimizeCommand(input, grid);
    if ("problem" in typed) { setMsg(typed.problem); return; }
    setMsg("");
    void analyticsCommand(kind === "backtest" ? "project0.analytics.backtest.start" : "project0.analytics.optimize.start", { params: typed.params });
  };
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
      <MethodsCompared run={run}/>
      <DetailRow title="Market liquidations (verified)" value={backtest ? backtest.liquidations.toString() : "—"}/>
      <DetailRow title="Counterfactual Shadow wins / matched" value={backtest?.counterfactual ? `${backtest.counterfactual.wins} / ${backtest.counterfactual.matched}` : "—"}/>
      {backtest?.counterfactual?.scenarios.map(s => <DetailRow key={s.capitalUsdMicro.toString()} title={`Shadow net · capital ${usdMicroDisplay(s.capitalUsdMicro.toString())}`} value={usdMicroDisplay(s.netUsdMicro)}/>)}
      <DetailRow title="Realized net PnL" value={backtest ? backtest.realized : "—"}/>
      <p className="p0-note">From the newest completed backtest of the run; counterfactual Shadow results (landing UNPROVEN) are never realized profit.</p>
      <DetailRow title="H0 · historical liquidations (every saved run)" value={h0 ? h0Line(h0) : "no completed H0 report"}/>
      <div className="p0-formgrid">
        <label>H0 budget · seconds<input value={point.maxSeconds} placeholder="required" onChange={e => setPoint({ ...point, maxSeconds: e.target.value })}/></label>
        <label>H0 budget · RAM MiB<input value={point.maxRamMib} placeholder="required" onChange={e => setPoint({ ...point, maxRamMib: e.target.value })}/></label>
      </div>
      <button className="p0-primary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => {
        const typed = h0Command(point);
        if ("problem" in typed) { setMsg(typed.problem); return; }
        setMsg("");
        void analyticsCommand("project0.analytics.h0.start", { params: typed.params });
      }}>Run H0 report</button>
      <p className="p0-note">H0 reads the verified liquidations of every saved run in the registry's window (the same job as p0ctl job start h0); a band is an inferred upper bound, never a competitor's configuration.</p></> : null}
    {tab === "backtest" ? <><p>Replay the run's sealed evidence through the production join (native job, offline; the same action as p0ctl job start backtest).</p>
      <PointFields point={input} set={({ runId: _, ...rest }) => setPoint(rest)}/>
      <button className="p0-primary" disabled={blocked !== "" || !run} title={blocked || undefined} onClick={() => start("backtest")}>Run backtest</button></> : null}
    {tab === "optimize" ? <><PointFields point={input} set={({ runId: _, ...rest }) => setPoint(rest)}/>
      <div className="p0-formgrid"><label>Grid · minimum profits USD<input value={grid.minProfitUsdGrid} placeholder="e.g. 1,2,5" onChange={e => setGrid({ ...grid, minProfitUsdGrid: e.target.value })}/></label>
        <label>Grid · funding sets<input value={grid.fundingSets} placeholder="all; NO_SWAP_INVENTORY" onChange={e => setGrid({ ...grid, fundingSets: e.target.value })}/></label>
        <label>Holdout · bps<input value={grid.holdoutBps} placeholder="required" onChange={e => setGrid({ ...grid, holdoutBps: e.target.value })}/></label>
        <label>Maximum points<input value={grid.maxPoints} placeholder="required" onChange={e => setGrid({ ...grid, maxPoints: e.target.value })}/></label>
        <label className="p0-check"><input type="checkbox" checked={grid.saveProfile} onChange={e => setGrid({ ...grid, saveProfile: e.target.checked })}/>Save the chosen point as an offline Shadow candidate</label></div>
      <div className="p0-formgrid"><label>Draft capital · USD<input type="number" value={capital} onChange={e => setCapital(e.target.value)}/></label>
      <label>Draft minimum net edge · USD<input type="number" min="0" value={minimumNet} onChange={e => setMinimumNet(e.target.value)}/></label></div>
      <div className="p0-actions"><button className="p0-primary" disabled={blocked !== "" || !run} title={blocked || undefined} onClick={() => start("optimize")}>Optimize</button><button className="p0-secondary"
        onClick={() => { if (!/^\d+(?:\.\d{1,6})?$/.test(capital) || Number(capital) <= 0 || !/^\d+(?:\.\d{1,6})?$/.test(minimumNet)) {
          setMsg("Enter a positive capital and nonnegative net edge."); return }
          createDraft({ capitalUsd: capital, minimumNetUsd: minimumNet }); goto("liquidator"); }}>Add as bot draft →</button></div>
      <p className="p0-note">Only typed input values are copied to a draft; a saved optimizer profile is an offline Shadow candidate, never a LIVE setting.</p></> : null}
    {msg ? <Notice>{msg}</Notice> : null}
    {blocked ? <p className="p0-quiet">{blocked}</p> : null}
    {receipt ? <Notice><span data-testid="p0-analytics-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""} · {receipt.action.endsWith(".start") ? "job" : "request"} {receipt.id.slice(0, 8)}</span>
      {["UNKNOWN", "PENDING", "PENDING_RECONCILE"].includes(receipt.phase) ? <> · outcome open: check the job list before a new command <button className="p0-secondary" onClick={() => void recheck()}>Look up again</button></> : null}</Notice> : null}</Section>
  <Section title="Analytics jobs" aside={<span className="p0-quiet">{jobs.length} listed</span>}>
    {jobs.length === 0 ? <p className="p0-quiet">{analytics ? "No analytics jobs." : "No job list received (a Project 0 process publishes it)."}</p> : null}
    {jobs.map(j => <JobRow key={j.jobId} job={j} cancelBlock={cancelBlock}/>)}
  </Section></>;
}
