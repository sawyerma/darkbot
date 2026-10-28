import { useEffect, useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import { analyticsCommand, autostartGrantCommand, autostartPreviewCommand, bootNetworkCommand, bootOfflineCommand, bootStartCommand, untilStoppedDuration, useAnalyticsControl } from "@/core/realtime/analyticsControl";
import { captureButtons, captureCommand, useCaptureControl } from "@/core/realtime/captureControl";
import { UNTIL_STOPPED, durationText, untilStoppedBlock, untilStoppedRunStatus, untilStoppedStopAction } from "../model/analytics";
import type { AnalyticsView, AutostartGrantView, BootFlowView } from "../model/analytics";
import type { ParameterView } from "../model/status";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

const BOOT_ACTIONS: ReadonlySet<string> = new Set(["project0.boot.offline", "project0.boot.network", "project0.boot.start", "project0.boot.cancel",
  "project0.autostart.preview", "project0.autostart.grant", "project0.autostart.revoke"]);
/** The durations `p0ctl run start --duration` takes. */
const DURATIONS = ["10m", "1h", "5h", "24h"] as const;

/** DEC-07: whole seconds left in the network record's TTL window at `nowS` (0 once expired);
 *  null without a record. */
export function ttlLeft(flow: BootFlowView, nowS: bigint): bigint | null {
  if (flow.expiresUnixS == null) return null;
  return flow.expiresUnixS > nowS ? flow.expiresUnixS - nowS : 0n;
}

const iso = (s: bigint) => new Date(Number(s) * 1000).toISOString().slice(11, 19);
const step = (s: BootFlowView["network"]) => `${s.state}${s.code ? ` · ${s.code}` : ""}${s.runId ? ` · run ${s.runId}` : ""}`;
const utc = (s: bigint) => new Date(Number(s) * 1000).toISOString().replace("T", " ").slice(0, 19);

/** DEC-16: the binding of a standing autostart approval, shown before it is granted and while it is in force. */
function GrantRows({ g, nowS, prefix }: { g: AutostartGrantView; nowS: bigint; prefix: string }) {
  return <>
    <DetailRow title={`${prefix} · digest`} value={g.digest}/>
    <DetailRow title={`${prefix} · run`} value={`${g.mode} · config ${g.configSha256.slice(0, 12)} · executables ${g.binarySha256.slice(0, 12)} · ${g.duration} (window ${g.checkpointSeconds} s, runway ${g.minimumRunwaySeconds} s)`}/>
    <DetailRow title={`${prefix} · evidence`} value={`qualification ${g.qualificationId} · provider evidence ${g.providerEvidenceId} · genesis ${g.genesisHash} · source ${g.captureSource}`}/>
    <DetailRow title={`${prefix} · budget per window`} value={`rpc_rps ${g.rpcRps} · max_requests ${g.maxRequests} · max_total_mib ${g.maxTotalMib}`}/>
    <DetailRow title={`${prefix} · limits`} value={`maximum runtime ${g.maxRuntimeSeconds} s per autostarted run · expires ${utc(g.expiresUnixS)} UTC${g.expiresUnixS < nowS ? " (expired)" : ""} · recorded ${utc(g.recordedUnixS)} UTC`}/>
  </>;
}

/**
 * DEC-07 (N7, N-5): the boot controller on the Capture tab — the same `p0ctl boot` as the
 * terminal: the offline preflight with the flow's TTL (fixed between runs), the operator's
 * network approval of the offline token, the network audit, the start approval of the network
 * token and the start. Each approval is its own explicit command naming the token shown;
 * nothing is approved or repeated automatically — a new attempt is a new offline preflight.
 */
export function BootSection({ analytics, topicStatus, ttl }: { analytics: AnalyticsView | null; topicStatus: TopicStatus; ttl: ParameterView | null }) {
  const live = topicStatus === "LIVE";
  const flow = live ? analytics?.boot ?? null : null;
  const { busy, receipt } = useAnalyticsControl();
  const [mode, setMode] = useState<"econ4h" | "shadow" | "market">("econ4h");
  const [ttlInput, setTtl] = useState("");
  const [genesis, setGenesis] = useState("");
  const [evidence, setEvidence] = useState("");
  const [chosenSource, setSource] = useState<"logs" | "transactions">("logs");
  const [duration, setDuration] = useState<string>(DURATIONS[0]);
  const until = analytics?.untilStopped ?? null;
  const q = live ? until?.qualification ?? null : null;
  const [windowInput, setWindow] = useState("");
  const [runwayInput, setRunway] = useState("");
  const [qualificationInput, setQualification] = useState("");
  const [maxRuntime, setMaxRuntime] = useState("");
  const [validFor, setValidFor] = useState("");
  const [armed, setArmed] = useState<"revoke" | "stop" | null>(null);
  // The grant confirmation belongs to exactly the digest it was armed for.
  const [armedGrant, setArmedGrant] = useState<string | null>(null);
  const capture = useCaptureControl();
  const [nowS, setNow] = useState(() => BigInt(Math.floor(Date.now() / 1000)));
  useEffect(() => {
    const t = setInterval(() => setNow(BigInt(Math.floor(Date.now() / 1000))), 1000);
    return () => clearInterval(t);
  }, []);
  const blocked = busy ? "Command in progress" : !live ? "Boot controller projection not current" : "";
  // DEC-16: the window, runway and qualification default to the newest raw-capture qualification.
  const windowValue = windowInput || (q ? durationText(q.checkpointSeconds) : "");
  const runwayValue = runwayInput || (q ? durationText(q.minimumRunwaySeconds) : "");
  const qualificationId = qualificationInput || (q?.id ?? "");
  const untilWhy = untilStoppedBlock({ mode, window: windowValue, runway: runwayValue, qualificationId, nowS }, q);
  const unbounded = duration === UNTIL_STOPPED;
  const horizon = unbounded ? untilStoppedDuration(windowValue, runwayValue) : duration;
  const offline = unbounded && (untilWhy !== "" || horizon === null)
    ? { problem: untilWhy || "Window and runway: durations (s/m/h/d)." }
    : bootOfflineCommand(mode, ttlInput, horizon ?? duration);
  const left = flow ? ttlLeft(flow, nowS) : null;
  // A Shadow run captures logs (`run start` passes no source for it).
  const source = flow?.mode === "shadow" ? "logs" : chosenSource;
  const active = flow != null && [flow.network.state, flow.start.state].some(s => s === "QUEUED" || s === "RUNNING");
  const network = flow
    ? bootNetworkCommand({ flowId: flow.flowId, token: flow.offlineToken, genesisHash: genesis, providerEvidence: evidence, captureSource: source })
    : { problem: "Run the offline preflight first." };
  const networkWhy = !flow ? "" : flow.offlineVerdict === "BLOCKED" ? "The offline preflight blocks"
    : flow.network.state !== "NOT_RUN" ? `Network audit ${flow.network.state}; a new attempt is a new offline preflight`
      : "problem" in network ? network.problem : "";
  const flowUnbounded = flow?.duration.startsWith(`${UNTIL_STOPPED}:`) ?? false;
  const start = flow
    ? bootStartCommand({ flowId: flow.flowId, token: flow.networkToken ?? "", duration: flow.duration, captureSource: source, chainVerify: "off",
      qualification: flowUnbounded ? qualificationId : undefined })
    : { problem: "No flow." };
  const offlineWhy = active ? "A boot step of this flow is active" : "problem" in offline ? offline.problem : "";
  const startWhy = !flow ? "" : flow.network.state !== "COMPLETED" || flow.networkVerdict !== "READY" ? "No READY network audit"
    : flow.start.state !== "NOT_RUN" ? `Start ${flow.start.state}; a new attempt is a new offline preflight`
      : left === 0n ? "The network audit's TTL has expired; a new attempt is a new offline preflight"
        : "problem" in start ? start.problem : "";
  const running = flow != null && [flow.network.state, flow.start.state].some(s => s === "RUNNING");
  // DEC-16: a running unbounded run ends only by its stop (drain, seal, final status) — an
  // econ4h or Shadow run through its boot flow's start step, the raw capture through the
  // desktop capture control.
  const run = live ? until?.run ?? null : null;
  const stopAction = untilStoppedStopAction(flow, run);
  const stop = captureButtons(capture.process, capture.busy).stop;
  const stopWhy = !run ? "No unbounded run" : run.sealed ? "The run has ended"
    : stopAction === null ? "The running boot flow is another run than the one shown"
      : stopAction === "project0.boot.cancel" ? blocked : stop.reason;
  const preview = live ? until?.preview ?? null : null;
  const granted = live ? until?.autostart ?? null : null;
  const previewCmd = autostartPreviewCommand({ qualification: qualificationId, window: windowValue, runway: runwayValue, genesisHash: genesis, maxRuntime, validFor, captureSource: source });
  const previewWhy = mode !== "market" ? "The app autostarts the raw capture (mode market) only" : untilWhy || ("problem" in previewCmd ? previewCmd.problem : "");
  const grantCmd = autostartGrantCommand(armedGrant ?? preview?.digest ?? "");
  return <Section title="Boot audit and start" aside={<span className="p0-quiet">{flow ? `flow ${flow.flowId.slice(0, 8)} · ${flow.mode}` : "no flow"}</span>}>
    <DetailRow title="TTL · registry" value={ttl ? `${ttl.value} s · ${ttl.minimum}…${ttl.maximum} s · ${ttl.source} · fixed per flow, changes between runs` : "unknown (no system status); the controller checks the bounds"}/>
    <div className="p0-formgrid">
      <label>Boot mode<select value={mode} onChange={e => setMode(e.target.value as "econ4h" | "shadow" | "market")}>
        <option value="econ4h">Market + Shadow (econ4h)</option><option value="shadow">Shadow only</option>
        <option value="market">Raw data only (market)</option></select></label>
      <label>TTL · seconds<input type="number" min={ttl ? String(ttl.minimum) : undefined} max={ttl ? String(ttl.maximum) : undefined}
        placeholder={ttl ? `${ttl.value} (registry)` : "registry value"} value={ttlInput} onChange={e => setTtl(e.target.value)}/></label>
      <label>Boot duration<select value={duration} onChange={e => setDuration(e.target.value)}>{DURATIONS.map(d => <option key={d} value={d}>{d}</option>)}
        <option value={UNTIL_STOPPED} disabled={untilWhy !== "" && !unbounded}>Until stopped</option></select></label>
    </div>
    <div className="p0-formgrid">
      <label>Until stopped · window<input value={windowValue} onChange={e => setWindow(e.target.value)} placeholder="e.g. 1h (the qualification's)"/></label>
      <label>Until stopped · runway<input value={runwayValue} onChange={e => setRunway(e.target.value)} placeholder="e.g. 24h (the qualification's)"/></label>
      <label>Until stopped · qualification id<input value={qualificationId} onChange={e => setQualification(e.target.value)} placeholder="16 hex digits (p0ctl horizon qualify)"/></label>
    </div>
    <DetailRow title="Until stopped" value={untilWhy ? `locked · ${untilWhy}` : `available · qualification ${qualificationId} · window ${windowValue} · runway ${runwayValue} · the start admits it again (RunHorizon::admit)`}/>
    {q ? <DetailRow title={`Qualification · ${q.mode}`} value={`${q.id} · ${q.complete ? "every part PASS" : `open: ${q.open.join("; ")}`} · evidence ${q.evidenceId ?? "—"}${q.evidenceValidUntilUnixS != null ? ` fresh until ${utc(q.evidenceValidUntilUnixS)} UTC` : ""}`}/> : null}
    {live && analytics?.bootError ? <DetailRow title="Boot store" value={analytics.bootError}/> : null}
    <div className="p0-actions"><button className="p0-secondary" disabled={blocked !== "" || offlineWhy !== ""} title={blocked || offlineWhy || undefined}
      onClick={() => { if ("params" in offline) void analyticsCommand("project0.boot.offline", { params: offline.params }); }}>Run offline preflight</button></div>
    {flow ? <>
      <DetailRow title="Offline preflight" value={`${flow.offlineVerdict} · token ${flow.offlineToken} · TTL ${flow.ttlSeconds} s (${flow.ttlSource}) · duration ${flow.duration}`}/>
      {flow.offlineBlocked.map(b => <DetailRow key={`o${b}`} title="Offline · not passed" value={b}/>)}
      <div className="p0-formgrid">
        <label>Genesis hash<input value={genesis} onChange={e => setGenesis(e.target.value)} placeholder="Mainnet genesis pin"/></label>
        <label>Provider evidence · record id<input value={evidence} onChange={e => setEvidence(e.target.value)} placeholder="16 hex digits (p0ctl boot evidence import)"/></label>
        <label>Boot capture source<select value={source} disabled={flow.mode === "shadow"} onChange={e => setSource(e.target.value as "logs" | "transactions")}>
          <option value="logs">Logs</option><option value="transactions">Transactions</option></select></label>
      </div>
      <div className="p0-actions"><button className="p0-secondary" disabled={blocked !== "" || networkWhy !== ""} title={blocked || networkWhy || undefined}
        onClick={() => { if ("params" in network) void analyticsCommand("project0.boot.network", { params: network.params }); }}>
        Approve network reads · token {flow.offlineToken}</button></div>
      <DetailRow title="Network audit" value={`${step(flow.network)}${flow.networkVerdict ? ` · ${flow.networkVerdict}` : ""}${flow.networkToken ? ` · token ${flow.networkToken}` : ""}`}/>
      {flow.networkBlocked.map(b => <DetailRow key={`n${b}`} title="Network · not passed" value={b}/>)}
      {flow.observedUnixS != null && flow.expiresUnixS != null
        ? <DetailRow title="TTL window" value={`${iso(flow.observedUnixS)}–${iso(flow.expiresUnixS)} UTC · ${left === 0n ? "expired" : `${left} s left`}`}/> : null}
      <div className="p0-actions"><button className="p0-primary" disabled={blocked !== "" || startWhy !== ""} title={blocked || startWhy || undefined}
        onClick={() => { if ("params" in start) void analyticsCommand("project0.boot.start", { params: start.params }); }}>
        Approve start · token {flow.networkToken ?? "—"} · {flowUnbounded ? `until stopped · qualification ${qualificationId || "—"}` : flow.duration}</button>
        {running ? <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined}
          onClick={() => void analyticsCommand("project0.boot.cancel", { resourceId: flow.flowId })}>Stop boot step</button> : null}</div>
      <DetailRow title="Start" value={step(flow.start)}/>
    </> : <p className="p0-quiet">{live ? "No boot flow yet." : `Boot controller projection ${topicStatus}; not current.`}</p>}
    {run ? <>
      <DetailRow title="Until stopped · run" value={`${run.runId} · qualification ${run.qualificationId} · since ${utc(run.startedUnixS)} UTC · window ${run.checkpointSeconds} s · ${run.windows} windows closed`}/>
      <DetailRow title="Until stopped · last window" value={run.lastVerdict ? `${run.lastVerdict}${run.lastEndedUnixS != null ? ` · ${utc(run.lastEndedUnixS)} UTC` : ""}${run.lastDetail ? ` · ${run.lastDetail}` : ""}` : "none closed yet"}/>
      <DetailRow title="Until stopped · status" value={untilStoppedRunStatus(run)}/>
      <div className="p0-actions"><button className="p0-secondary" disabled={stopWhy !== ""} title={stopWhy || undefined} onClick={() => setArmed("stop")}>Stop run · drain and seal</button></div>
      {armed === "stop" && stopWhy === "" ? <><Notice>The run drains (its capture, its winner watches), seals its journals and its windows journal, and records its final status (complete for an ordered end of a run that never halted).</Notice>
        <div className="p0-actions"><button className="p0-secondary" onClick={() => {
          setArmed(null);
          if (stopAction === "project0.boot.cancel" && flow) void analyticsCommand("project0.boot.cancel", { resourceId: flow.flowId });
          else void captureCommand("project0.capture.stop");
        }}>Confirm stop</button>
          <button className="p0-secondary" onClick={() => setArmed(null)}>Cancel</button></div></> : null}
    </> : null}
    <DetailRow title="Standing autostart approval" value={granted ? `in force · digest ${granted.digest}${granted.expiresUnixS < nowS ? " · expired" : ""}` : "none · the app autostarts nothing (AUTOSTART_BOOT_REQUIRED)"}/>
    {granted ? <>
      <GrantRows g={granted} nowS={nowS} prefix="In force"/>
      <div className="p0-actions"><button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined} onClick={() => setArmed("revoke")}>Revoke standing approval</button></div>
      {armed === "revoke" ? <div className="p0-actions"><button className="p0-secondary" onClick={() => { setArmed(null); void analyticsCommand("project0.autostart.revoke", {}); }}>Confirm revoke · {granted.digest}</button>
        <button className="p0-secondary" onClick={() => setArmed(null)}>Cancel</button></div> : null}
    </> : null}
    <div className="p0-formgrid">
      <label>Autostart · maximum runtime<input value={maxRuntime} onChange={e => setMaxRuntime(e.target.value)} placeholder="per autostarted run, e.g. 7d"/></label>
      <label>Autostart · valid for<input value={validFor} onChange={e => setValidFor(e.target.value)} placeholder="e.g. 30d (within the evidence freshness)"/></label>
    </div>
    <div className="p0-actions"><button className="p0-secondary" disabled={blocked !== "" || previewWhy !== ""} title={blocked || previewWhy || undefined}
      onClick={() => { if ("params" in previewCmd) void analyticsCommand("project0.autostart.preview", { params: previewCmd.params }); }}>Preview standing approval</button></div>
    {preview ? <>
      <GrantRows g={preview} nowS={nowS} prefix="Preview"/>
      <div className="p0-actions"><button className="p0-primary" disabled={blocked !== "" || "problem" in grantCmd} title={blocked || ("problem" in grantCmd ? grantCmd.problem : undefined)}
        onClick={() => setArmedGrant(preview.digest)}>Grant exactly this binding · {preview.digest}</button></div>
      {armedGrant === preview.digest ? <><Notice>Grant the standing approval {preview.digest}: autostarts of exactly the run above ({preview.mode}, {preview.duration}, config {preview.configSha256.slice(0, 12)}), its qualification, provider evidence, genesis pin and window budget, each for at most {String(preview.maxRuntimeSeconds)} s, until {utc(preview.expiresUnixS)} UTC. Every autostart still runs the offline audit, the network audit and the start; LIVE stays disabled.</Notice>
        <div className="p0-actions"><button className="p0-primary" onClick={() => { setArmedGrant(null); if ("params" in grantCmd) void analyticsCommand("project0.autostart.grant", { params: grantCmd.params }); }}>Confirm grant · {preview.digest}</button>
          <button className="p0-secondary" onClick={() => setArmedGrant(null)}>Cancel</button></div></> : null}
    </> : null}
    {live && until?.errors.length ? until.errors.map(e => <DetailRow key={`u${e}`} title="Until stopped · store" value={e}/>) : null}
    {receipt && BOOT_ACTIONS.has(receipt.action) ? <Notice><span data-testid="p0-boot-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""}</span></Notice> : null}
    <p className="p0-note">The network audit reads Mainnet read-only only after its approval; the start reads Mainnet for the flow's duration after its own approval, within the record's TTL, with chain verification off (online strict is a terminal action: p0ctl boot start --chain-verify strict --provider-evidence). A new attempt is a new offline preflight; nothing is approved or repeated automatically. Until stopped runs only with a complete qualification, both approvals and its window gates (a violated gate halts it); a standing approval permits the app's autostart of exactly its run and is no LIVE approval.</p>
  </Section>;
}
