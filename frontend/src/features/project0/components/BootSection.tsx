import { useEffect, useState } from "react";
import type { TopicStatus } from "@/core/realtime";
import { analyticsCommand, bootNetworkCommand, bootOfflineCommand, bootStartCommand, useAnalyticsControl } from "@/core/realtime/analyticsControl";
import type { AnalyticsView, BootFlowView } from "../model/analytics";
import type { ParameterView } from "../model/status";
import { DetailRow, Notice, Section } from "./PanelPrimitives";

const BOOT_ACTIONS: ReadonlySet<string> = new Set(["project0.boot.offline", "project0.boot.network", "project0.boot.start", "project0.boot.cancel"]);
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
  const [nowS, setNow] = useState(() => BigInt(Math.floor(Date.now() / 1000)));
  useEffect(() => {
    const t = setInterval(() => setNow(BigInt(Math.floor(Date.now() / 1000))), 1000);
    return () => clearInterval(t);
  }, []);
  const blocked = busy ? "Command in progress" : !live ? "Boot controller projection not current" : "";
  const offline = bootOfflineCommand(mode, ttlInput, duration);
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
  const start = flow
    ? bootStartCommand({ flowId: flow.flowId, token: flow.networkToken ?? "", duration: flow.duration, captureSource: source, chainVerify: "off" })
    : { problem: "No flow." };
  const offlineWhy = active ? "A boot step of this flow is active" : "problem" in offline ? offline.problem : "";
  const startWhy = !flow ? "" : flow.network.state !== "COMPLETED" || flow.networkVerdict !== "READY" ? "No READY network audit"
    : flow.start.state !== "NOT_RUN" ? `Start ${flow.start.state}; a new attempt is a new offline preflight`
      : left === 0n ? "The network audit's TTL has expired; a new attempt is a new offline preflight"
        : "problem" in start ? start.problem : "";
  const running = flow != null && [flow.network.state, flow.start.state].some(s => s === "RUNNING");
  return <Section title="Boot audit and start" aside={<span className="p0-quiet">{flow ? `flow ${flow.flowId.slice(0, 8)} · ${flow.mode}` : "no flow"}</span>}>
    <DetailRow title="TTL · registry" value={ttl ? `${ttl.value} s · ${ttl.minimum}…${ttl.maximum} s · ${ttl.source} · fixed per flow, changes between runs` : "unknown (no system status); the controller checks the bounds"}/>
    <div className="p0-formgrid">
      <label>Boot mode<select value={mode} onChange={e => setMode(e.target.value as "econ4h" | "shadow" | "market")}>
        <option value="econ4h">Market + Shadow (econ4h)</option><option value="shadow">Shadow only</option>
        <option value="market">Raw data only (market)</option></select></label>
      <label>TTL · seconds<input type="number" min={ttl ? String(ttl.minimum) : undefined} max={ttl ? String(ttl.maximum) : undefined}
        placeholder={ttl ? `${ttl.value} (registry)` : "registry value"} value={ttlInput} onChange={e => setTtl(e.target.value)}/></label>
      <label>Boot duration<select value={duration} onChange={e => setDuration(e.target.value)}>{DURATIONS.map(d => <option key={d} value={d}>{d}</option>)}</select></label>
    </div>
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
        Approve start · token {flow.networkToken ?? "—"} · {flow.duration}</button>
        {running ? <button className="p0-secondary" disabled={blocked !== ""} title={blocked || undefined}
          onClick={() => void analyticsCommand("project0.boot.cancel", { resourceId: flow.flowId })}>Stop boot step</button> : null}</div>
      <DetailRow title="Start" value={step(flow.start)}/>
    </> : <p className="p0-quiet">{live ? "No boot flow yet." : `Boot controller projection ${topicStatus}; not current.`}</p>}
    {receipt && BOOT_ACTIONS.has(receipt.action) ? <Notice><span data-testid="p0-boot-command-receipt">Command {receipt.phase}{receipt.detail ? ` · ${receipt.detail}` : ""}</span></Notice> : null}
    <p className="p0-note">The network audit reads Mainnet read-only only after its approval; the start reads Mainnet for the flow's duration after its own approval, within the record's TTL, with chain verification off (online strict is a terminal action: p0ctl boot start --chain-verify strict --provider-evidence). A new attempt is a new offline preflight; nothing is approved or repeated automatically.</p>
  </Section>;
}
