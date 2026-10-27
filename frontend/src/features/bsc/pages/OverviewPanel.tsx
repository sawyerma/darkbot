import { Line, Section } from "@/shared/ui/graphite/Rows";
import { Notice } from "@/shared/ui/graphite/Notice";
import { selectLaunch, selectStatus, selectWallets, useBscStore } from "../store/bscStore";
import { EvidenceRows, timestamp, ValueState } from "./EvidenceRows";
import { SECTIONS } from "../model/placement";
import { checkStatus } from "../model/checks";
import type { BscTab } from "../BscPage";
import { runtimeView } from "../model/sources";
import { useNativeRecords } from "./useNativeRecords";
import { economicRows, summary, switchRow } from "../model/switches";

const title = (n: number) => `${n}. ${SECTIONS.overview[n - 1]}`;
const age = (s: bigint | undefined) => s && s > 0n ? `${Math.max(0, Math.floor(Date.now() / 1000) - Number(s))} s` : "UNKNOWN";

/** Read-only summary in the fixed §13 order; details live in the four detail tabs. */
export function OverviewPanel({ goto }: { goto: (t: BscTab) => void }) {
  const { projection: s, status } = useBscStore(selectStatus);
  const job = s?.jobs.find(j => j.job === "readiness");
  const r = s?.readiness;
  const runtime = runtimeView(useNativeRecords(), Date.now()).view;
  const { projection: w } = useBscStore(selectWallets);
  const { projection: launch } = useBscStore(selectLaunch);
  const walletSwitches = [switchRow("BSC_WALLET_LAUNCHER", w?.launcherControl), switchRow("BSC_WALLET_BUYER", w?.buyerControl)];
  return <>
    <Section title={title(1)}>
      <Line label="Source quality" value={<ValueState state={status} />} hint="Transport state of bsc.status, never a trading permission" />
      <Line label="Data as of" value={timestamp(s?.dataAsOfUnixS)} hint="Explicit snapshot sync time" />
      <Line label="Snapshot age" value={age(s?.dataAsOfUnixS)} hint="An old snapshot is STALE, not SOURCE_DETACHED" />
      <Line label="Evaluated" value={timestamp(s?.evaluatedUnixS)} />
      <Line label="Source commit" value={s?.sourceCommit || "—"} />
      <Line label="Update source" value="darkbotctl bsc sync" hint="Run in the original repository terminal; existing snapshots remain dated." />
    </Section>
    <Section title={title(2)}>
      <Line label="Readiness job" value={<ValueState state={job?.state} />} hint="Process lifecycle only, not the readiness verdict" />
      <Line label="Job result" value={<ValueState state={job?.resultCode || undefined} />} />
      <Line label="Launch ID" value={job?.launchId || "—"} />
      <Line label="Started / ended" value={`${timestamp(job?.startedUnixS)} / ${timestamp(job?.endedUnixS)}`} />
      <Line label="Controller available" value={job?.controllerAvailable === undefined ? "UNKNOWN" : String(job.controllerAvailable)} hint="Start and stop in Launch / BUY" />
    </Section>
    <Section title={title(3)}>
      <Line label="Compiled guard" value={<ValueState state={s?.guard?.guardState} />} />
      <Line label="Contract approval" value={s?.guard?.approved === undefined ? "UNKNOWN" : String(s.guard.approved)} />
      <Line label="Execution ready" value={s?.guard?.executionReady === undefined ? "UNKNOWN" : String(s.guard.executionReady)} />
      <Line label="Current contract" value={s?.guard?.contractPresent ? s.guard.contractPath : "No active V5 contract"} />
      <Line label="LIVE" value={<ValueState state={s?.liveState} />} />
      <Line label="Wallet switches" value={summary(walletSwitches)} hint="Native permissions; switch them in Wallets. DISABLED initially and after every restart" />
      <Line label="Economic switches" value={summary(economicRows(launch?.economicControls))} hint="Native permissions; switch them in Launch / BUY. None of them is LIVE or an approval" />
    </Section>
    <Section title={title(4)}>
      <Line label="Runtime readiness" value={<ValueState state={runtime.state} />} hint={`${runtime.mode}: read-only observation of the effective SHADOW sources; no build, signature or send exists`} />
      <Line label="Pre-Live verdict" value={<ValueState state={r?.verdict} />} />
      <Line label="Mode" value={<ValueState state={r?.mode || undefined} />} />
      <Line label="Selected candidate binding" value={<ValueState state={r?.candidateBinding || undefined} />} hint="Only BOUND counts for the selected candidate; STALE is history" />
      <Line label="Clock (CLOCK_CONFIG)" value={<ValueState state={checkStatus(r?.checks, "CLOCK_CONFIG")} />} hint="Needs an attested clock/boot ID; the GUI job stays BLOCKED without one" />
      <Line label="Attempt authority (ATTEMPT_AUTHORITY)" value={<ValueState state={checkStatus(r?.checks, "ATTEMPT_AUTHORITY")} />} hint="Shared one-attempt authority on origin: PASS = none yet (grant after approval); STOP = granted or consumed, never again" />
      <Line label="Blocking reasons" value={r?.reasons.length ? r.reasons.join(" · ") : "—"} />
    </Section>
    <Section title={title(5)}>
      {s?.gates.map(g => <Line key={g.gateId} label={g.gateId} value={<ValueState state={g.state} />} hint={[...g.reasonCodes, g.evidence?.path].filter(Boolean).join(" · ")} />)}
    </Section>
    <Section title={title(6)}>
      {s?.warnings.length ? s.warnings.map(w => <Notice key={w}>{w}</Notice>) : <Line label="Warnings" value="—" />}
      <EvidenceRows evidence={r?.evidence} />
    </Section>
    <div className="g-actions"><button className="g-secondary" onClick={() => goto("launch")}>Launch / BUY</button><button className="g-secondary" onClick={() => goto("monitoring")}>Monitoring</button></div>
    <Notice>Read-only observations. A connected reporter, successful readiness check or ARMED guard never grants LIVE permission.</Notice>
  </>;
}
